#!/usr/bin/env python3
"""Behaviour measures and the NPC interaction map from AI observer outputs
(docs/spec-notes/ai-observer.md section 9).

Reads the plugin's observe output (one or more .jsonl files, any campaigns) and measures:

- conversations between NPCs: who with whom, how often per NPC and per game hour, the
  distance between the two at the start and during, how long they last;
- speech in and out of combat: lines (IsTalking rising edges) per fighter, the gap
  between one fighter's lines, how many speak at once, and a guessed trigger for each
  combat line from what happened just before it;
- melee spacing: the distance between an attacker and its combat target when a swing
  starts (GetAnimAction 2), during the swing, and between swings (backing off);
- encounters: who targets whom, deaths, fleeing, searching, joining a fight;
- the interaction map: talks to, targets, follows, uses (furniture, markers, items),
  with game hours; and what each package does (type code, procedures, hours) against
  its record (`--packages`, the output of `fnvtool ai show`).

  observe_map.py OUT.jsonl [...] [--data <Data>] [--packages show.json] [-o map.json] [--text summary.txt]

Standard library only; the game folder and Data are only read.
"""
import argparse
import math
import os
import statistics
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import observe as ob  # noqa: E402

MAP_FORMAT = "fnv.observe.map/1"
# The player's reference [documented: GECK wiki, PlayerRef is 00000014].
PLAYER = "0x00000014"
# GetCurrentAIPackage codes of dialogue packages (6 and 34 in the wiki's table) and of
# packages that keep an actor with another (FOLLOW, ESCORT, ACCOMPANY).
DIALOGUE_TYPES = {6, 34}
FOLLOW_TYPES = {1, 2, 15}
# GetCurrentAIProcedure codes.
PROC = {v: k for k, v in ob.PROCEDURES.items()}
USE_PROCEDURES = {PROC[n] for n in ("ACTIVATE", "SLEEP", "EAT", "USE_ITEM_AT", "USE_IDLE_MARKER", "SITTING",
                                    "SANDBOX", "GUARD", "USE_WEAPON")}
# GetAnimAction codes [documented: GECK wiki, "revised for GECK NV"]: 2 attack (a melee
# swing or full auto), 3 follow-through (semi auto), 4 back from an attack. A melee swing
# is taken as the action entering 2 or 3 from outside 2-4: a trooper with a knife showed
# 3 far more often than 2 [observed 2026-10-17, melee_spacing].
ANIM_ATTACK, ANIM_SWING, ANIM_ATTACK_PHASES = 2, {2, 3}, {2, 3, 4}
ANIM_STAGGER = 8
# A sample stands for the time to the next one, capped (a long gap is a hitch or a
# cell load, not time spent in a state).
MAX_SAMPLE_HOURS = 5 / 60
# Two dialogue-package records of one pair more than this apart are two conversations.
CONVERSATION_GAP_HOURS = 2 / 60
# Context windows for the guessed trigger of a combat line (real seconds before it)
# [hypothesis: the game says a line on the event, within a moment].
LINE_CONTEXT_SECONDS = 1.5
# Speech up to this long after an actor leaves combat still counts as a combat line
# (e.g. lines on leaving combat).
AFTER_COMBAT_SECONDS = 5.0
# Swings that start within this long of the previous one's end are a combo.
COMBO_SECONDS = 0.6
# Below this speed (units per real second) an attacker counts as stopped before a swing.
STOPPED_SPEED = 30.0

# Weapon classes by words in the weapon's Editor ID [hypothesis: a reading aid; the
# record's skill and animation type would be exact].
WEAPON_CLASSES = [
    ("knife", ("knife", "switchblade", "cleaver", "bumper", "straightrazor", "razor", "shishkebab", "chance")),
    ("machete", ("machete", "gladius")),
    ("unarmed weapon", ("fist", "knuckle", "glove", "bladesofthe", "bearclaw", "deathclawgauntlet", "saturnite",
                        "mantisgauntlet", "yaoguaigauntlet", "zapglove", "pushyglove")),
    ("long melee", ("sledge", "hammer", "bat", "pipe", "cue", "shovel", "tireiron", "spear", "golf", "rebar", "axe",
                    "pickaxe", "rollingpin", "cane", "club", "chainsaw", "thermiclance", "ripper", "katana", "sword",
                    "fireaxe", "oars", "nailboard")),
]


def weapon_class(weapon, creature, base_name, weapon_name):
    if creature:
        return f"creature {base_name or '?'}"
    if not weapon:
        return "unarmed"
    w = (weapon_name or "").lower()
    for cls, words in WEAPON_CLASSES:
        if any(x in w for x in words):
            return cls
    return f"other {weapon_name or weapon}"


def quantiles(xs):
    xs = sorted(x for x in xs if x is not None)
    if not xs:
        return None
    q = statistics.quantiles(xs, n=10, method="inclusive") if len(xs) > 1 else [xs[0]] * 9
    return {"n": len(xs), "min": round(xs[0], 1), "p10": round(q[0], 1), "median": round(statistics.median(xs), 1),
            "p90": round(q[8], 1), "max": round(xs[-1], 1), "mean": round(statistics.fmean(xs), 1)}


def dist(a, b, dims=2):
    pa, pb = (a.get("x"), a.get("y"), a.get("z")), (b.get("x"), b.get("y"), b.get("z"))
    if None in pa[:dims] or None in pb[:dims]:
        return None
    return math.dist(pa[:dims], pb[:dims])


# --------------------------------------------------------------------------- reading

def observations(sources):
    """[{source, label, set, samples: [{t, rs, dt, head, actors: {id: rec}}]}]: each
    sample with game time t (hours), real seconds rs since the observation began (from
    the TimeScale in the header), the game hours it stands for (dt) and every actor's
    decoded record; the player is added as actor PLAYER (position, combat state)."""
    out, problems = [], []
    for source, lines in sources:
        mods, obs = ob.read_samples(lines, source)
        if not mods:
            problems.append(f"{source}: no mod indices (not an nvf_capture output?)")
            continue
        for o in obs:
            f = o["fields"]
            header_fields = [tuple(h) for h in f.get("header", [])]
            part_fields = [[tuple(x) for x in p] for p in f.get("parts", [])]
            samples = []
            for smp in o["samples"]:
                head = {}
                for (name, kind), v in zip(header_fields, smp.get("header", [])):
                    head[name] = ob.parse_value(kind, v, mods) if kind == "ref" else v
                t = ob.game_time(head)
                if t is None:
                    continue
                actors = {}
                for group, subjects in sorted((smp.get("groups") or {}).items(), key=lambda g: g[0] != "nearby"):
                    for parts in subjects or []:
                        rec = ob.decode_subject(parts, part_fields, mods)
                        aid = rec.get("id")
                        if aid and aid not in actors:
                            rec["seen"] = group == "nearby"
                            actors[aid] = rec
                actors[PLAYER] = {"id": PLAYER, "x": head.get("player_x"), "y": head.get("player_y"),
                                  "z": head.get("player_z"), "cell": head.get("player_cell"),
                                  "in_combat": head.get("player_in_combat"), "seen": True, "player": True}
                samples.append({"t": t, "head": head, "actors": actors})
            for k, s in enumerate(samples):
                nxt = samples[k + 1]["t"] if k + 1 < len(samples) else s["t"]
                s["dt"] = min(max(nxt - s["t"], 0.0), MAX_SAMPLE_HOURS)
            rs = 0.0
            for k, s in enumerate(samples):
                if k:
                    ts = s["head"].get("timescale") or ob.DEFAULT_TIMESCALE
                    rs += max(s["t"] - samples[k - 1]["t"], 0.0) * 3600 / ts
                s["rs"] = rs
            out.append({"source": source, "label": o["label"], "set": f.get("set", "full"), "samples": samples,
                        "end": o["end"]})
    return out, problems


def is_actor(ref, known):
    return ref == PLAYER or ref in known


# --------------------------------------------------------------------------- measures

def conversations(o, known):
    """NPC-to-NPC conversations of one observation: a pair is in conversation while
    either has a dialogue package (type 6 or 34) whose target, or whose dialogue target
    (GetDialogueTarget), is the other. The talk starts when both are on it or one is in
    procedure DIALOGUE; before that the starter walks up (procedure DIALOGUE_ACTIVATE)."""
    open_, done = {}, []
    for s in o["samples"]:
        acts = s["actors"]
        pairs = {}
        for aid, r in acts.items():
            if r.get("player"):
                continue
            partner = None
            if r.get("package_type") is not None and int(r["package_type"]) in DIALOGUE_TYPES:
                partner = r.get("package_target")
            if not partner or partner == PLAYER:
                dt_ = r.get("dialogue_target")
                partner = dt_ if dt_ and dt_ != PLAYER else partner
            if not partner or partner == PLAYER or partner == aid:
                continue
            pairs.setdefault(frozenset((aid, partner)), set()).add(aid)
        for pair, who in pairs.items():
            c = open_.get(pair)
            if c is None or s["t"] - c["last"] > CONVERSATION_GAP_HOURS:
                if c is not None:
                    done.append(c)
                a, b = sorted(pair)
                c = open_[pair] = {"a": a, "b": b, "starter": sorted(who)[0] if len(who) == 1 else None,
                                   "first": s["t"], "last": s["t"], "talk_start": None, "start_distance": None,
                                   "talk_distance": None, "distances": [], "lines": 0, "_talking": {},
                                   "cell": (acts.get(a) or {}).get("cell"), "hour": s["head"].get("hour"),
                                   "label": o["label"], "rs_first": s["rs"], "rs_last": s["rs"]}
            c["last"], c["rs_last"] = s["t"], s["rs"]
            ra, rb = acts.get(c["a"]), acts.get(c["b"])
            d = dist(ra, rb) if ra and rb else None
            if d is not None:
                c["distances"].append(d)
                if c["start_distance"] is None:
                    c["start_distance"] = d
            procs = {int(acts[x]["procedure"]) for x in pair if x in acts and acts[x].get("procedure") is not None}
            if c["talk_start"] is None and (len(who) == 2 or PROC["DIALOGUE"] in procs):
                c["talk_start"] = s["t"]
                c["talk_distance"] = d
            for x in pair:
                talking = bool((acts.get(x) or {}).get("talking"))
                if talking and not c["_talking"].get(x):
                    c["lines"] += 1
                c["_talking"][x] = talking
        for pair in [p for p, c in open_.items() if s["t"] - c["last"] > CONVERSATION_GAP_HOURS]:
            done.append(open_.pop(pair))
    done += open_.values()
    out = []
    for c in done:
        c.pop("_talking")
        ds = c.pop("distances")
        c["minutes"] = round((c["last"] - c["first"]) * 60, 2)
        c["real_seconds"] = round(c.pop("rs_last") - c.pop("rs_first"), 1)
        c["talk_minutes"] = round((c["last"] - c["talk_start"]) * 60, 2) if c["talk_start"] is not None else None
        c["distance"] = quantiles(ds)
        for k in ("start_distance", "talk_distance"):
            if c[k] is not None:
                c[k] = round(c[k], 1)
        out.append(c)
    return sorted(out, key=lambda c: c["first"])


def presence_hours(o):
    """Game hours each actor was in the loaded area (nearby scan) of the observation."""
    hours = {}
    for s in o["samples"]:
        for aid, r in s["actors"].items():
            if r.get("seen") and not r.get("player"):
                hours[aid] = hours.get(aid, 0.0) + s["dt"]
    return hours


def speech(o):
    """Lines (IsTalking rising edges) of every actor, with what happened just before
    each, split into combat lines (in combat, or within AFTER_COMBAT_SECONDS of leaving
    it) and others."""
    lines, state, last_combat, events = [], {}, {}, {}
    concurrency, combat_time = {}, {}
    prev = None
    for s in o["samples"]:
        talking_fighters = 0
        for aid, r in s["actors"].items():
            if r.get("player"):
                continue
            st = state.setdefault(aid, {"talking": False, "health": None, "anim": None, "target": None,
                                        "in_combat": False, "dead": False, "proc": None})
            ev = events.setdefault(aid, [])
            in_combat = bool(r.get("in_combat"))
            if in_combat:
                last_combat[aid] = s["rs"]
                combat_time[aid] = combat_time.get(aid, 0.0) + s["dt"] * 3600 / (s["head"].get("timescale") or 30)
            h = r.get("health")
            if st["health"] is not None and h is not None and h < st["health"] - 0.01:
                ev.append((s["rs"], "hit"))
            anim = r.get("anim_action")
            if anim is not None and int(anim) in ANIM_SWING and st["anim"] not in ANIM_ATTACK_PHASES:
                ev.append((s["rs"], "attack"))
            tgt = r.get("combat_target")
            if in_combat and not st["in_combat"]:
                ev.append((s["rs"], "combat start"))
            if st["target"] and not tgt and in_combat:
                ev.append((s["rs"], "target lost"))
            proc = r.get("procedure")
            if proc is not None and st["proc"] != int(proc):
                name = ob.PROCEDURES.get(int(proc))
                if name in ("SEARCH", "PURSUE", "FLEE", "YIELD", "ALARM"):
                    ev.append((s["rs"], name.lower()))
            if r.get("dead") and not st["dead"]:
                for other, oe in events.items():
                    if other != aid:
                        oe.append((s["rs"], "death nearby"))
            talking = bool(r.get("talking"))
            recent_combat = aid in last_combat and s["rs"] - last_combat[aid] <= AFTER_COMBAT_SECONDS
            if talking and recent_combat:
                talking_fighters += 1
            if talking and not st["talking"]:
                ctx = sorted({k for (ts, k) in ev if s["rs"] - LINE_CONTEXT_SECONDS <= ts <= s["rs"]})
                lines.append({"actor": aid, "t": s["t"], "rs": round(s["rs"], 2), "combat": recent_combat,
                              "dead": bool(r.get("dead")), "context": ctx, "label": o["label"]})
            st.update(talking=talking, health=h, anim=None if anim is None else int(anim), target=tgt,
                      in_combat=in_combat, dead=bool(r.get("dead")), proc=None if proc is None else int(proc))
            # keep only recent events
            events[aid] = [e for e in ev if s["rs"] - e[0] <= LINE_CONTEXT_SECONDS]
        if prev is not None and any(st["in_combat"] for st in state.values()):
            concurrency[talking_fighters] = concurrency.get(talking_fighters, 0.0) + (s["rs"] - prev)
        prev = s["rs"]
    return lines, concurrency, combat_time


def line_trigger(ctx):
    """Guessed trigger of a combat line from what happened just before it [hypothesis:
    the topic itself is not readable with base-game or xNVSE functions]."""
    for k, name in (("death nearby", "death response"), ("hit", "hit"), ("target lost", "lost target"),
                    ("search", "search"), ("flee", "flee"), ("combat start", "combat start"), ("attack", "attack"),
                    ("pursue", "pursue"), ("alarm", "alarm"), ("yield", "yield")):
        if k in ctx:
            return name
    return "other"


def melee(o, names):
    """Swings (GetAnimAction rising to 2) of every actor whose weapon is a melee weapon
    or none, and of creatures, with the distance to its combat target."""
    swings, approach = [], {}
    st = {}
    pos_hist = {}
    for s in o["samples"]:
        acts = s["actors"]
        for aid, r in acts.items():
            if r.get("player"):
                continue
            hist = pos_hist.setdefault(aid, [])
            if r.get("x") is not None:
                hist.append((s["rs"], r["x"], r["y"]))
                del hist[:-30]
            anim = r.get("anim_action")
            anim = None if anim is None else int(anim)
            p = st.setdefault(aid, {"anim": None, "swing": None, "last_end": None, "between_max": None})
            tgt_id = r.get("combat_target")
            tgt = acts.get(tgt_id) if tgt_id else None
            d = dist(r, tgt) if tgt else None
            cls = weapon_class(r.get("weapon"), r.get("creature"), names.get(_fid(r.get("base"))),
                               names.get(_fid(r.get("weapon"))))
            melee_like = not cls.startswith("other")
            if melee_like and r.get("in_combat") and d is not None:
                a = approach.setdefault(aid, {"class": cls, "closest": d, "samples": 0})
                a["closest"] = min(a["closest"], d)
                a["samples"] += 1
            if p["swing"] is None and p["last_end"] is not None and d is not None:
                p["between_max"] = d if p["between_max"] is None else max(p["between_max"], d)
            if anim in ANIM_SWING and p["anim"] not in ANIM_ATTACK_PHASES and melee_like:
                speed = None
                old = [h for h in hist if s["rs"] - h[0] >= 0.25]
                if old:
                    h0 = old[-1]
                    span = s["rs"] - h0[0]
                    if span > 0:
                        speed = math.hypot(r["x"] - h0[1], r["y"] - h0[2]) / span
                combo = p["last_end"] is not None and s["rs"] - p["last_end"][0] <= COMBO_SECONDS
                back_off = None
                if p["last_end"] is not None and p["last_end"][1] is not None and p["between_max"] is not None:
                    back_off = p["between_max"] - p["last_end"][1]
                p["swing"] = {"actor": aid, "class": cls, "target": tgt_id, "rs": round(s["rs"], 2), "t": s["t"],
                              "distance": None if d is None else round(d, 1),
                              "dz": None if not tgt or None in (r.get("z"), tgt.get("z")) else round(r["z"] - tgt["z"], 1),
                              "speed_before": None if speed is None else round(speed, 1), "combo": combo,
                              "back_off_before": None if back_off is None else round(back_off, 1),
                              "min_distance": d, "label": o["label"]}
                swings.append(p["swing"])
            if p["swing"] is not None:
                if d is not None:
                    md = p["swing"]["min_distance"]
                    p["swing"]["min_distance"] = d if md is None else min(md, d)
                if anim not in ANIM_ATTACK_PHASES:
                    p["swing"]["seconds"] = round(s["rs"] - p["swing"]["rs"], 2)
                    p["last_end"] = (s["rs"], d)
                    p["between_max"] = d
                    p["swing"] = None
            p["anim"] = anim
    for sw in swings:
        if sw["min_distance"] is not None:
            sw["min_distance"] = round(sw["min_distance"], 1)
    return swings, approach


def _fid(ref):
    if isinstance(ref, str) and ref.startswith("0x"):
        try:
            return int(ref, 16)
        except ValueError:
            return None
    return None


def encounters(o):
    """Combat timeline of one observation: per actor, when it entered and left combat,
    its targets in order, deaths, and spans in FLEE / SEARCH / PURSUE / YIELD / ALARM."""
    actors, prev = {}, {}
    proc_names = {PROC[n]: n for n in ("FLEE", "SEARCH", "PURSUE", "YIELD", "ALARM", "FLEE_NON_COMBAT")}
    for s in o["samples"]:
        for aid, r in s["actors"].items():
            a = actors.setdefault(aid, {"combat": [], "targets": [], "died": None, "procedures": {}, "targeted_by": set()})
            p = prev.get(aid, {})
            ic = bool(r.get("in_combat"))
            if ic and not p.get("in_combat"):
                a["combat"].append([s["t"], None])
            if not ic and p.get("in_combat") and a["combat"]:
                a["combat"][-1][1] = s["t"]
            tgt = r.get("combat_target")
            if tgt and tgt != p.get("combat_target"):
                a["targets"].append([round(s["t"], 4), tgt])
            if r.get("dead") and not p.get("dead"):
                a["died"] = s["t"]
            pr = r.get("procedure")
            if pr is not None and int(pr) in proc_names:
                n = proc_names[int(pr)]
                a["procedures"][n] = a["procedures"].get(n, 0.0) + s["dt"] * 60
            prev[aid] = {"in_combat": ic, "combat_target": tgt, "dead": bool(r.get("dead"))}
    for aid, a in actors.items():
        for _, tgt in a["targets"]:
            if tgt in actors:
                actors[tgt]["targeted_by"].add(aid)
    out = {}
    for aid, a in actors.items():
        if not a["combat"] and not a["targets"]:
            continue
        first = a["combat"][0][0] if a["combat"] else None
        # joined: entered combat before anyone had it as a target [hypothesis: an ally
        # called for help or it saw the fight; neither is readable directly]
        earliest_targeted = min((t for other in actors.values() for t, tg in other["targets"] if tg == aid), default=None)
        joined = first is not None and (earliest_targeted is None or earliest_targeted > first)
        out[aid] = {"combat": [[round(x, 4) if x is not None else None for x in span] for span in a["combat"]],
                    "targets": a["targets"], "died": a["died"], "procedure_minutes": {k: round(v, 2) for k, v in a["procedures"].items()},
                    "targeted_by": sorted(a["targeted_by"]), "joined_untargeted": joined}
    sides = _sides(out)
    for aid, side in sides.items():
        out[aid]["side"] = side
    return out


def _sides(enc):
    """Two-colour the 'targets' graph: actors that target each other are on opposite
    sides; actors on one side never target each other [hypothesis for mixed fights:
    a third party gets a side of its own when the colouring fails]."""
    adj = {}
    for aid, e in enc.items():
        for _, t in e["targets"]:
            adj.setdefault(aid, set()).add(t)
            adj.setdefault(t, set()).add(aid)
    side, nxt = {}, 0
    for start in sorted(adj):
        if start in side:
            continue
        side[start] = nxt
        todo = [start]
        while todo:
            a = todo.pop()
            for b in adj[a]:
                if b not in side:
                    side[b] = side[a] ^ 1  # nxt is even: toggles within the pair
                    todo.append(b)
        nxt += 2
    return side


def interactions(o, known, edges, packages):
    """Adds one observation to the interaction graph (edges) and the package table."""
    for s in o["samples"]:
        hour = s["head"].get("hour")
        hour = int(hour) % 24 if isinstance(hour, (int, float)) else None
        minutes = s["dt"] * 60
        for aid, r in s["actors"].items():
            if r.get("player"):
                continue
            pkg, ptype, proc, ptgt = r.get("package"), r.get("package_type"), r.get("procedure"), r.get("package_target")
            ptype = None if ptype is None else int(ptype)
            proc = None if proc is None else int(proc)

            def add(kind, b, extra=None):
                e = edges.setdefault((kind, aid, b), {"kind": kind, "a": aid, "b": b, "minutes": 0.0, "samples": 0,
                                                      "hours": {}, "labels": set(), "detail": {}})
                e["minutes"] += minutes
                e["samples"] += 1
                if hour is not None:
                    e["hours"][hour] = e["hours"].get(hour, 0.0) + minutes
                e["labels"].add(o["label"])
                if extra is not None:
                    e["detail"][extra] = e["detail"].get(extra, 0.0) + minutes

            if r.get("combat_target"):
                add("targets", r["combat_target"])
            dtg = r.get("dialogue_target")
            if ptype in DIALOGUE_TYPES and ptgt and ptgt != aid:
                add("talks_to", ptgt, ob.PROCEDURES.get(proc, proc))
            elif dtg and dtg != aid:
                add("talks_to", dtg, "dialogue target")
            if ptgt and is_actor(ptgt, known) and (ptype in FOLLOW_TYPES or proc in (PROC["FOLLOW"], PROC["ESCORT"], PROC["ACCOMPANY"])):
                add("follows", ptgt, ob.PACKAGE_TYPES.get(ptype, ptype))
            if ptgt and not is_actor(ptgt, known) and proc in USE_PROCEDURES:
                add("uses", ptgt, ob.PROCEDURES.get(proc, proc))
            if r.get("sitting") and int(r["sitting"]) == 3:
                add("sits", ptgt or "?", f"furniture {int(r['furniture'])}" if r.get("furniture") is not None else None)
            if r.get("sleeping") and int(r["sleeping"]) == 3:
                add("sleeps", ptgt or "?")
            if pkg:
                p = packages.setdefault(pkg, {"actors": set(), "type_codes": {}, "procedures": {}, "hours": {},
                                              "cells": set(), "minutes": 0.0, "targets": {}, "moved": 0.0})
                p["actors"].add(aid)
                p["minutes"] += minutes
                if ptype is not None:
                    p["type_codes"][ptype] = p["type_codes"].get(ptype, 0.0) + minutes
                pn = ob.PROCEDURES.get(proc, proc)
                p["procedures"][pn] = p["procedures"].get(pn, 0.0) + minutes
                if hour is not None:
                    p["hours"][hour] = p["hours"].get(hour, 0.0) + minutes
                if r.get("cell"):
                    p["cells"].add(r["cell"])
                if ptgt:
                    p["targets"][ptgt] = p["targets"].get(ptgt, 0.0) + minutes


# --------------------------------------------------------------------------- the map

def build(sources, names=None, records=None):
    names = names or {}
    obs_, problems = observations(sources)
    known = set()
    for o in obs_:
        for s in o["samples"]:
            known.update(s["actors"])
    nm = lambda ref: names.get(_fid(ref), ref) if ref else ref  # noqa: E731
    convs, lines, swings, encs, edges, packages, sessions = [], [], [], {}, {}, {}, []
    presence, concurrency, combat_time, approach = {}, {}, {}, {}
    for o in obs_:
        sessions.append({"source": o["source"], "label": o["label"], "set": o["set"], "samples": len(o["samples"]),
                         "game_hours": round(o["samples"][-1]["t"] - o["samples"][0]["t"], 3) if o["samples"] else 0,
                         "real_seconds": round(o["samples"][-1]["rs"], 1) if o["samples"] else 0, "end": o["end"]})
        convs += conversations(o, known)
        for aid, h in presence_hours(o).items():
            presence.setdefault(aid, {})
            presence[aid][o["label"]] = presence[aid].get(o["label"], 0.0) + h
        ls, conc, ct = speech(o)
        lines += ls
        for k, v in conc.items():
            concurrency.setdefault(o["label"], {})
            concurrency[o["label"]][k] = concurrency[o["label"]].get(k, 0.0) + v
        for aid, v in ct.items():
            combat_time[aid] = combat_time.get(aid, 0.0) + v
        sw, ap = melee(o, names)
        swings += sw
        for aid, a in ap.items():
            b = approach.setdefault(aid, dict(a))
            b["closest"] = min(b["closest"], a["closest"])
        key = o["label"] if o["label"] not in encs else f"{o['label']}.{len(encs)}"  # repeated runs of a session
        encs[key] = encounters(o)
        interactions(o, known, edges, packages)
    return {"format": MAP_FORMAT, "sources": [s for s, _ in sources], "sessions": sessions, "problems": problems,
            "conversations": conversation_report(convs, presence, nm),
            "speech": speech_report(lines, concurrency, combat_time, nm),
            "melee": melee_report(swings, approach, nm),
            "encounters": {label: {nm(a): _named_enc(e, nm) for a, e in enc.items()} for label, enc in encs.items()},
            "graph": graph_report(edges, presence, nm),
            "packages": package_report(packages, nm, records)}


def _named_enc(e, nm):
    e = dict(e)
    e["targets"] = [[t, nm(x)] for t, x in e["targets"]]
    e["targeted_by"] = [nm(x) for x in e["targeted_by"]]
    return e


def conversation_report(convs, presence, nm):
    by_actor, by_label, by_hour = {}, {}, {}
    for c in convs:
        for x in (c["a"], c["b"]):
            by_actor[x] = by_actor.get(x, 0) + 1
        by_label[c["label"]] = by_label.get(c["label"], 0) + 1
        if isinstance(c["hour"], (int, float)):
            h = int(c["hour"]) % 24
            by_hour[h] = by_hour.get(h, 0) + 1
    per_actor = []
    for aid, hours in presence.items():
        total = sum(hours.values())
        if total <= 0 or aid == PLAYER:
            continue
        n = by_actor.get(aid, 0)
        per_actor.append({"actor": nm(aid), "conversations": n, "hours_loaded": round(total, 2),
                          "per_game_hour": round(n / total, 3)})
    per_actor.sort(key=lambda x: (-x["per_game_hour"], x["actor"]))
    rates = [x["per_game_hour"] for x in per_actor if x["hours_loaded"] >= 1]
    loaded_hours = {}
    for hours in presence.values():
        for label, h in hours.items():
            loaded_hours[label] = loaded_hours.get(label, 0.0) + h
    return {"list": [dict(c, a=nm(c["a"]), b=nm(c["b"]), starter=nm(c["starter"]), cell=nm(c["cell"])) for c in convs],
            "count": len(convs),
            "start_distance": quantiles([c["start_distance"] for c in convs]),
            "talk_distance": quantiles([c["talk_distance"] for c in convs]),
            "median_distance_during": quantiles([c["distance"]["median"] for c in convs if c["distance"]]),
            "minutes": quantiles([c["minutes"] for c in convs]),
            "real_seconds": quantiles([c["real_seconds"] for c in convs]),
            "lines": quantiles([c["lines"] for c in convs]),
            "per_actor": per_actor, "per_actor_hour": quantiles(rates),
            "by_session": {k: {"conversations": v, "actor_hours_loaded": round(loaded_hours.get(k, 0.0), 2),
                               "per_actor_hour": round(v * 2 / loaded_hours[k], 3) if loaded_hours.get(k) else None}
                           for k, v in sorted(by_label.items())},
            "by_hour_of_day": dict(sorted(by_hour.items()))}


def speech_report(lines, concurrency, combat_time, nm):
    combat = [x for x in lines if x["combat"]]
    other = [x for x in lines if not x["combat"]]
    per_actor = {}
    for x in combat:
        per_actor.setdefault(x["actor"], []).append(x)
    gaps, fighters = [], []
    for aid, ls in per_actor.items():
        ls.sort(key=lambda x: (x["label"], x["rs"]))
        g = [b["rs"] - a["rs"] for a, b in zip(ls, ls[1:]) if a["label"] == b["label"]]
        gaps += g
        secs = combat_time.get(aid, 0.0)
        fighters.append({"actor": nm(aid), "lines": len(ls), "combat_seconds": round(secs, 1),
                         "per_minute": round(len(ls) * 60 / secs, 2) if secs > 0 else None,
                         "median_gap": round(statistics.median(g), 1) if g else None,
                         "triggers": _count(line_trigger(x["context"]) for x in ls)})
    fighters.sort(key=lambda f: -f["lines"])
    total_secs = sum(combat_time.values())
    conc = {}
    for label, c in concurrency.items():
        total = sum(c.values())
        conc[label] = {str(k): round(v / total, 3) for k, v in sorted(c.items())} if total else {}
    return {"combat_lines": len(combat), "fighters": fighters,
            "fighter_seconds": round(total_secs, 1),
            "per_fighter_minute": round(len(combat) * 60 / total_secs, 3) if total_secs else None,
            "gap_seconds": quantiles(gaps),
            "triggers": _count(line_trigger(x["context"]) for x in combat),
            "speaking_at_once": conc,
            "other_lines": len(other),
            "list": [dict(x, actor=nm(x["actor"]), trigger=line_trigger(x["context"]) if x["combat"] else None)
                     for x in lines]}


def _count(it):
    out = {}
    for x in it:
        out[x] = out.get(x, 0) + 1
    return dict(sorted(out.items(), key=lambda kv: -kv[1]))


def melee_report(swings, approach, nm):
    by_class = {}
    for sw in swings:
        by_class.setdefault(sw["class"], []).append(sw)
    classes = {}
    for cls, sws in sorted(by_class.items()):
        stopped = [s for s in sws if s["speed_before"] is not None and s["speed_before"] < STOPPED_SPEED]
        classes[cls] = {"swings": len(sws), "distance_at_swing": quantiles([s["distance"] for s in sws]),
                        "closest_in_swing": quantiles([s["min_distance"] for s in sws]),
                        "distance_when_stopped": quantiles([s["distance"] for s in stopped]),
                        "share_stopped": round(len(stopped) / len(sws), 2) if sws else None,
                        "combo_share": round(sum(s["combo"] for s in sws) / len(sws), 2) if sws else None,
                        "back_off_between": quantiles([s["back_off_before"] for s in sws if not s["combo"]]),
                        "swing_seconds": quantiles([s.get("seconds") for s in sws])}
    return {"classes": classes,
            "closest_approach": {nm(a): {"class": v["class"], "closest": round(v["closest"], 1), "samples": v["samples"]}
                                 for a, v in sorted(approach.items())},
            "swings": [dict(s, actor=nm(s["actor"]), target=nm(s["target"])) for s in swings]}


def graph_report(edges, presence, nm):
    nodes = {}
    for aid, hours in presence.items():
        nodes[nm(aid)] = {"hours_loaded": round(sum(hours.values()), 2), "sessions": sorted(hours)}
    out = []
    for e in edges.values():
        out.append({"kind": e["kind"], "a": nm(e["a"]), "b": nm(e["b"]), "minutes": round(e["minutes"], 2),
                    "samples": e["samples"], "hours_of_day": {str(k): round(v, 2) for k, v in sorted(e["hours"].items())},
                    "sessions": sorted(e["labels"]),
                    "detail": {str(k): round(v, 2) for k, v in sorted(e["detail"].items(), key=lambda kv: -kv[1])}})
    out.sort(key=lambda e: (e["kind"], -e["minutes"]))
    return {"nodes": nodes, "edges": out, "edge_counts": _count(e["kind"] for e in out)}


def package_report(packages, nm, records):
    recs = {}
    for r in (records or {}).get("packages", []):
        recs[r.get("form")] = r
    out = {}
    for pid, p in packages.items():
        rec = recs.get(pid)
        total = p["minutes"] or 1.0
        entry = {"actors": sorted(nm(a) for a in p["actors"]), "minutes": round(p["minutes"], 2),
                 "type_codes": {str(ob.PACKAGE_TYPES.get(k, k)): round(v / total, 3) for k, v in p["type_codes"].items()},
                 "procedures": {str(k): round(v / total, 3) for k, v in sorted(p["procedures"].items(), key=lambda kv: -kv[1])},
                 "hours_of_day": sorted(p["hours"]), "cells": sorted(str(nm(c)) for c in p["cells"]),
                 "targets": {str(nm(k)): round(v / total, 3) for k, v in sorted(p["targets"].items(), key=lambda kv: -kv[1])[:5]}}
        if rec:
            entry["record"] = {"type": rec.get("type"), "runtime_type_code": rec.get("runtime_type_code"),
                               "schedule": rec.get("schedule"), "location": (rec.get("location") or {}).get("kind"),
                               "target": rec.get("target")}
            code = rec.get("runtime_type_code")
            seen = {k for k in p["type_codes"]}
            entry["type_matches_record"] = None if code is None else seen <= {code}
            sch = rec.get("schedule") or {}
            if isinstance(sch.get("hour"), int) and sch["hour"] >= 0 and sch.get("duration_hours"):
                start, length = sch["hour"], sch["duration_hours"]
                inside = lambda h: (h - start) % 24 < length  # noqa: E731
                outside = sum(v for h, v in p["hours"].items() if not inside(h))
                entry["share_outside_schedule"] = round(outside / total, 3)
        out[str(nm(pid))] = entry
    return dict(sorted(out.items()))


# --------------------------------------------------------------------------- text

def text(m):
    out = ["AI observer map: measures (numbers only, no game text)", ""]
    for s in m["sessions"]:
        out.append(f"session {s['label']} ({s['set']}): {s['samples']} samples, {s['game_hours']} game h, "
                   f"{s['real_seconds']} real s, ended {s['end'] or 'cut off'}")
    c = m["conversations"]
    out += ["", f"Conversations between NPCs: {c['count']}",
            f"  start distance {c['start_distance']}", f"  distance when talk starts {c['talk_distance']}",
            f"  median distance during {c['median_distance_during']}", f"  game minutes {c['minutes']}",
            f"  real seconds {c['real_seconds']}", f"  lines {c['lines']}",
            f"  per NPC per loaded game hour {c['per_actor_hour']}", f"  by session {c['by_session']}",
            f"  by hour of day {c['by_hour_of_day']}"]
    sp = m["speech"]
    out += ["", f"Combat lines: {sp['combat_lines']} over {sp['fighter_seconds']} fighter-seconds "
            f"({sp['per_fighter_minute']} per fighter-minute); other lines {sp['other_lines']}",
            f"  gap between one fighter's lines (real s) {sp['gap_seconds']}",
            f"  guessed triggers {sp['triggers']}", f"  share of fight time with N speaking {sp['speaking_at_once']}"]
    for f in sp["fighters"][:20]:
        out.append(f"    {f['actor']}: {f['lines']} lines in {f['combat_seconds']} s, median gap {f['median_gap']}, {f['triggers']}")
    out += ["", "Melee swings by class (distance centre to centre, units):"]
    for cls, v in m["melee"]["classes"].items():
        out.append(f"  {cls}: {v['swings']} swings; at swing {v['distance_at_swing']}; closest in swing "
                   f"{v['closest_in_swing']}; stopped share {v['share_stopped']}; combo share {v['combo_share']}; "
                   f"back-off between {v['back_off_between']}")
    out += ["", "Encounters:"]
    for label, enc in m["encounters"].items():
        if not enc:
            continue
        died = sum(1 for e in enc.values() if e["died"])
        fled = sum(1 for e in enc.values() if e["procedure_minutes"].get("FLEE"))
        joined = sum(1 for e in enc.values() if e["joined_untargeted"])
        retarget = sum(max(len(e["targets"]) - 1, 0) for e in enc.values())
        out.append(f"  {label}: {len(enc)} fighters, {died} died, {fled} fled, {joined} joined before being targeted, "
                   f"{retarget} target changes")
    g = m["graph"]
    out += ["", f"Interaction graph: {len(g['nodes'])} actors, edges {g['edge_counts']}"]
    pk = m["packages"]
    mism = [k for k, v in pk.items() if v.get("type_matches_record") is False]
    outside = [k for k, v in pk.items() if (v.get("share_outside_schedule") or 0) > 0.05]
    out += [f"Packages observed: {len(pk)}; type code differs from the record: {len(mism)}; "
            f"used more than 5 % outside the schedule: {len(outside)}"]
    return "\n".join(out) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(prog="observe_map.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("inputs", nargs="+", metavar="OUT.jsonl")
    ap.add_argument("--data", help="game Data folder (read-only), to name forms by Editor ID")
    ap.add_argument("--packages", help="fnv.ai.show/1 output of `fnvtool ai show` for the packages to compare")
    ap.add_argument("-o", "--output", help="map JSON (default: print)")
    ap.add_argument("--text", help="also write the readable summary here ('-' prints it)")
    args = ap.parse_args(argv)
    try:
        names = cs.edid_names(args.data) if args.data else None
        records = cs.load_json(args.packages) if args.packages else None
        sources = []
        for p in args.inputs:
            with open(p, encoding="utf-8") as f:
                sources.append((p, f.read().splitlines()))
    except (OSError, ValueError, cs.InputError) as e:
        print(f"observe_map: {e}", file=sys.stderr)
        return 1
    m = build(sources, names, records)
    if args.output or not args.text:
        cs.write(m, args.output)
    if args.text:
        t = text(m)
        if args.text == "-":
            sys.stdout.write(t)
        else:
            with open(args.text, "w", encoding="utf-8", newline="\n") as f:
                f.write(t)
    return 0


if __name__ == "__main__":
    sys.exit(main())
