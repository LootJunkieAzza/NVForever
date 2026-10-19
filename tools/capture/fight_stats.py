#!/usr/bin/env python3
"""Shots, hits and damage in an observed fight (docs/spec-notes/npc-behaviour-compare.md
section 9): reads AI observer outputs (plugin version 9, the fight field set and the
`fight` events) and writes, per fighter and overall, the shots begun, the hits it landed
(OnHit with it as the attacker), the damage those did and the distances it fought at.

  fight_stats.py OUT.jsonl [...] [--json stats.json]

- A **shot or swing** is GetAnimAction entering 2 or 3 from anything else
  [documented: GECK wiki GetAnimAction, 2 attack, 3 follow-through]. A full-auto burst
  counts once (2 is held while the trigger is).
- A **hit** is an OnHit event on an actor (first: the target, second: the attacker),
  which xNVSE raises before the damage [documented: GECK wiki NVSE Event Handling: the
  attacker is a projectile when the target is not an actor]; its damage is the
  Health the event carries less the target's Health at the next sample (0 when it did not
  fall), and the hit that killed counts what was left.
- Distances are 2D, between the reference points (GetPos), at the sample of the shot.

Standard library only; the outputs are only read.
"""
import argparse
import json
import math
import statistics
import sys


def quantiles(xs):
    if not xs:
        return None
    xs = sorted(xs)
    pick = lambda q: xs[min(len(xs) - 1, int(q * (len(xs) - 1) + 0.5))]
    return {"n": len(xs), "min": round(xs[0], 1), "median": round(statistics.median(xs), 1), "p90": round(pick(0.9), 1),
            "max": round(xs[-1], 1)}


def read(path):
    """Samples ([(n, t seconds, {id: rec})]) and events of one output file."""
    samples, events, fields, hf, t0, ts = [], [], None, None, None, 30.0
    with open(path, encoding="utf-8") as f:
        for line in f:
            try:
                r = json.loads(line)
            except json.JSONDecodeError:
                continue
            k = r.get("kind")
            if k == "observing":
                fields = [[n for n, _ in p] for p in r["fields"]["parts"]]
                hf = [n for n, _ in r["fields"]["header"]]
            elif k == "sample" and fields:
                h = dict(zip(hf, r["header"]))
                if not isinstance(h.get("hour"), (int, float)) or not isinstance(h.get("days"), (int, float)):
                    continue
                ts = h.get("timescale") or ts
                hours = 24 * round((h["days"] * 24 - h["hour"]) / 24) + h["hour"]
                if t0 is None:
                    t0 = hours
                t = (hours - t0) * 3600 / ts
                recs = {}
                for group in ("nearby", "tracked"):
                    for subj in r["groups"].get(group) or []:
                        rec = {}
                        for names, val in zip(fields, subj):
                            if isinstance(val, str):
                                rec.update(zip(names, val.split("|")))
                        if rec.get("id") and rec["id"] not in recs:
                            recs[rec["id"]] = rec
                samples.append((r["n"], t, recs))
            elif k == "event" and isinstance(r.get("value"), str):
                v = r["value"].split("|")
                events.append({"n": r["n"], "name": r["name"], "first": v[0], "second": v[1], "health": float(v[4])})
    return samples, events


def num(rec, k, default=None):
    try:
        return float(rec[k])
    except (KeyError, TypeError, ValueError):
        return default


def stats(samples, events):
    by_n = {n: (t, recs) for n, t, recs in samples}
    order = [n for n, _, _ in samples]
    nxt = {a: b for a, b in zip(order, order[1:])}
    fighters = {}

    def f(aid):
        return fighters.setdefault(aid, {"shots": 0, "hits": 0, "damage": 0.0, "hit_damage": [], "shot_range": [],
                                         "combat_first": None, "combat_last": None, "died": None, "base": None,
                                         "weapon": None, "creature": None})

    prev = {}
    for n, t, recs in samples:
        for aid, rec in recs.items():
            a = f(aid)
            a["base"], a["weapon"], a["creature"] = rec.get("base"), rec.get("weapon") or a["weapon"], rec.get("creature")
            if rec.get("in_combat") == "1":
                a["combat_first"] = t if a["combat_first"] is None else a["combat_first"]
                a["combat_last"] = t
            if rec.get("dead") == "1" and a["died"] is None:
                a["died"] = t
            act, was = rec.get("anim_action"), prev.get(aid)
            if act in ("2", "3") and was not in ("2", "3") and rec.get("dead") != "1":
                a["shots"] += 1
                tgt = recs.get(rec.get("combat_target") or "")
                if tgt and None not in (num(rec, "x"), num(tgt, "x")):
                    a["shot_range"].append(math.hypot(num(rec, "x") - num(tgt, "x"), num(rec, "y") - num(tgt, "y")))
            prev[aid] = act
    for e in events:
        # Hits on actors only: a shot that hits the world is an OnHit on that reference,
        # with the projectile as the attacker (the probe's Health -1: not an actor).
        if e["name"] != "OnHit" or e["second"] in ("0", "00000000") or e["health"] < 0:
            continue
        a = f(e["second"])
        a["hits"] += 1
        after = by_n.get(nxt.get(e["n"], e["n"]), (None, {}))[1].get(e["first"])
        dmg = max(0.0, e["health"] - num(after, "health", e["health"])) if after else 0.0
        a["damage"] += dmg
        a["hit_damage"].append(dmg)
    out = {}
    for aid, a in fighters.items():
        if not a["shots"] and not a["hits"] and a["combat_first"] is None:
            continue
        out[aid] = {"base": a["base"], "weapon": a["weapon"], "creature": a["creature"], "shots": a["shots"], "hits": a["hits"],
                    "hits_per_shot": round(a["hits"] / a["shots"], 2) if a["shots"] else None,
                    "damage": round(a["damage"], 1), "damage_per_hit": quantiles(a["hit_damage"]),
                    "shot_range": quantiles(a["shot_range"]),
                    "combat": [None if a["combat_first"] is None else round(a["combat_first"], 1),
                               None if a["combat_last"] is None else round(a["combat_last"], 1)],
                    "died": None if a["died"] is None else round(a["died"], 1)}
    shots = sum(a["shots"] for a in out.values())
    hits = sum(a["hits"] for a in out.values())
    spans = [a["combat"] for a in out.values() if a["combat"][0] is not None]
    summary = {"fighters": len(out), "shots": shots, "hits": hits, "hits_per_shot": round(hits / shots, 3) if shots else None,
               "fight": [min(s[0] for s in spans), max(s[1] for s in spans)] if spans else None,
               "deaths": sorted((a["died"], aid) for aid, a in out.items() if a["died"] is not None),
               "damage_per_hit": quantiles([d for a in fighters.values() for d in a["hit_damage"]]),
               "shot_range": quantiles([d for a in fighters.values() for d in a["shot_range"]])}
    return {"summary": summary, "fighters": out}


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("outputs", nargs="+")
    ap.add_argument("--json")
    a = ap.parse_args(argv)
    res = {}
    for p in a.outputs:
        res[p] = stats(*read(p))
    text = json.dumps(res, indent=1)
    if a.json:
        with open(a.json, "w", encoding="utf-8") as f:
            f.write(text + "\n")
    print(text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
