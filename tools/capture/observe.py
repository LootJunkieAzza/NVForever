#!/usr/bin/env python3
"""AI observer: watch NPCs in the original game with nvf_capture and turn what they did
into timelines (docs/spec-notes/ai-observer.md).

The bot only observes. It moves the player by teleporting (console coc / cow, script
MoveTo) or by holding the Forward key through xNVSE (walk to a reference), sets the
clock, then samples every actor near the player and a list of named actors wherever they
are (package, procedure, targets, combat, detection, sitting/sleeping, position, ...)
for a span of game time. Base game + xNVSE only: every function used is listed with its
source in the spec note.

  observe.py diag [-o job.steps.json]
      diagnostic job: each probe field, each probe part, `$` number formatting, key
      emulation and a short observation, as separate steps (run it first on a new setup)
  observe.py jobs CAMPAIGN --data <Data> [-o DIR]
      fnv.observe.campaign/1 -> one fnv.capture.steps/1 job per session
  observe.py run CAMPAIGN --game <FNV> --save NAME --data <Data> [--retries N]
      build the jobs and run them unattended with capture_run, restarting the game after
      a crash or hang and retrying the session that was cut off; writes a run report
  observe.py report OUT.jsonl [...] [--data <Data>] [-o timeline.json] [--text]
      plugin output -> fnv.observe.timeline/1: per actor, segments of unchanged AI state
      with game-time spans, and every state change as an event

The game folder and the Data folder are only read. Standard library only. Exit codes: 0
ok, 1 bad input, 2 (run) some session did not end.
"""
import argparse
import json
import math
import os
import shutil
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
if HERE not in sys.path:
    sys.path.insert(0, HERE)

import capture_run as cr  # noqa: E402
import capture_steps as cs  # noqa: E402

CAMPAIGN_FORMAT = "fnv.observe.campaign/1"
TIMELINE_FORMAT = "fnv.observe.timeline/1"
RUN_FORMAT = "fnv.observe.run/1"
PLUGIN_VERSION = 4  # the observe step (tools/capture/nvf_capture/src/runner.rs)

# Form type code of actors for GetFirstRef / GetNumRefs [documented: GECK wiki
# GetFirstRef, "200 for actors"].
ACTOR_TYPE = 200
# Default TimeScale of a game that has not changed it [observed 2026-10-08: TimeScale read
# 30 and was restored to 30, xnvse-capture.md section 7]; used only for time estimates.
DEFAULT_TIMESCALE = 30
# Highest frame rate assumed when sizing max_samples (a safety cap; `until` ends the
# observation at the requested game time).
MAX_FPS = 240
# Seconds allowed per session on top of its game time (start-up, load, travel, quit).
SESSION_MARGIN = 300
DEFAULTS = {"every": 60, "depth": 1, "settle_frames": 300, "timescale": None, "max_actors": 128,
            "console_first": []}

# Probe fields. kind "ref": the expression gives a form, recorded as its load-order
# FormID (GetFormIDString; "00000000" when there is none); "num": a number, recorded with
# `$`. Every function is base game (GECK1) or xNVSE (FOSE1/NVSE) [documented: GECK wiki
# pages of each, docs/spec-notes/ai-observer.md section 2]. GetDistance is left out on
# purpose: the wiki says it ends the script when the target is not in the current cell.
# The fields are split into parts (one script each) to keep each expression short.
PARTS = [
    [("id", "ref", None), ("base", "ref", "GetBaseObject"), ("cell", "ref", "GetParentCell"),
     ("x", "num", "GetPos X"), ("y", "num", "GetPos Y"), ("z", "num", "GetPos Z"), ("heading", "num", "GetAngle Z"),
     ("package", "ref", "GetCurrentPackage"), ("package_type", "num", "GetCurrentAIPackage"),
     ("procedure", "num", "GetCurrentAIProcedure"), ("package_target", "ref", "GetPackageTarget")],
    [("in_combat", "num", "IsInCombat"), ("combat_target", "ref", "GetCombatTarget"),
     ("detects_player", "num", "GetDetected player"), ("alerted", "num", "GetIsAlerted"),
     ("alarmed", "num", "GetAlarmed"), ("sitting", "num", "GetSitting"), ("sleeping", "num", "GetSleeping"),
     ("knocked", "num", "GetKnockedState"), ("talking", "num", "IsTalking"), ("dead", "num", "GetDead"),
     ("unconscious", "num", "GetUnconscious"), ("disabled", "num", "GetDisabled"),
     ("sneaking", "num", "IsSneaking"), ("running", "num", "IsRunning"), ("weapon_out", "num", "IsWeaponOut"),
     ("moving", "num", "IsMoving"), ("health", "num", "GetAV Health"), ("persistent", "num", "IsPersistent"),
     ("creature", "num", "GetIsCreature"), ("teammate", "num", "GetPlayerTeammate")],
]
FIELD_KIND = {name: kind for part in PARTS for name, kind, _ in part}
HEADER = [("days", "num", "GameDaysPassed"), ("hour", "num", "GameHour"), ("timescale", "num", "TimeScale"),
          ("player_x", "num", "player.GetPos X"), ("player_y", "num", "player.GetPos Y"),
          ("player_z", "num", "player.GetPos Z"), ("player_heading", "num", "player.GetAngle Z"),
          ("player_cell", "ref", "player.GetParentCell"), ("player_in_combat", "num", "player.IsInCombat")]

# Names of GetCurrentAIPackage and GetCurrentAIProcedure codes [documented: GECK wiki,
# return value tables of both pages].
PACKAGE_TYPES = {0: "EXPLORE", 1: "FOLLOW", 2: "ESCORT", 3: "EAT", 4: "SLEEP", 5: "COMBAT", 6: "DIALOGUE",
                 7: "ACTIVATE", 8: "ALARM", 9: "SPECTATOR", 10: "FLEE", 11: "TRESPASS", 12: "GREET",
                 13: "WANDER", 14: "TRAVEL", 15: "ACCOMPANY", 16: "USEITEMAT", 17: "AMBUSH",
                 18: "FLEE_NON_COMBAT", 19: "CAST_MAGIC", 20: "COMBAT_LOW", 21: "GET_UP", 24: "DO_NOTHING",
                 28: "SURFACE", 29: "SEARCH_FOR_ATTACKER", 32: "MOVEMENT_BLOCKED", 33: "USE_WEAPON",
                 34: "DIALOGUE", 35: "GUARD", 36: "SANDBOX", 37: "PATROL"}
PROCEDURES = {0: "TRAVEL", 1: "ACTIVATE", 2: "ACQUIRE", 3: "WAIT", 4: "DIALOGUE", 5: "GREET", 6: "GREET_DEAD",
              7: "WANDER", 8: "SLEEP", 9: "OBSERVE_COMBAT", 10: "EAT", 11: "FOLLOW", 12: "ESCORT", 13: "COMBAT",
              14: "ALARM", 15: "PURSUE", 16: "FLEE", 17: "DONE", 18: "YIELD", 19: "TRAVEL_TARGET",
              20: "CREATE_FOLLOW", 21: "GET_UP", 22: "MOUNT_HORSE", 23: "DISMOUNT_HORSE", 24: "DO_NOTHING",
              27: "ACCOMPANY", 28: "USE_ITEM_AT", 29: "SANDMAN", 30: "WAIT_AMBUSH", 31: "SURFACE",
              32: "WAIT_FOR_SPELL", 33: "CHOOSE_CAST", 34: "FLEE_NON_COMBAT", 35: "REMOVE_WORN_ITEMS",
              36: "SEARCH", 37: "CLEAR_MOUNT_POSITION", 38: "SUMMON_DEFEND", 39: "AVOID_AREA",
              40: "UNEQUIP_ARMOR", 41: "PATROL", 42: "USE_WEAPON", 43: "DIALOGUE_ACTIVATE", 44: "GUARD",
              45: "SANDBOX", 46: "USE_IDLE_MARKER", 47: "TAKE_BACK_ITEM", 48: "SITTING", 49: "MOVEMENT_BLOCKED",
              50: "CANNIBAL_FEED"}
# Fields whose change starts a new timeline segment (and is listed as an event).
STATE_FIELDS = ["cell", "package", "package_type", "procedure", "package_target", "in_combat", "combat_target",
                "detects_player", "alerted", "alarmed", "sitting", "sleeping", "knocked", "talking", "dead",
                "unconscious", "disabled", "sneaking", "weapon_out", "teammate", "seen"]
# Control code of Forward for GetControl [documented: GECK wiki TapControl, "0:Forward"].
CONTROL_FORWARD = 0


class InputError(Exception):
    pass


# --------------------------------------------------------------------------- scripts

def nearby_select(depth):
    """Statements that put the %{i}-th actor of the loaded area into rA (or nothing).
    GetNextRef continues the GetFirstRef of the same script run [documented: GECK wiki
    GetNextRef]; each probe walks from the start, so a sample costs n*n/2 steps."""
    return [f"let rA := GetFirstRef {ACTOR_TYPE} {int(depth)}", "let iN := 0", "while iN < %{i}",
            "  let rA := GetNextRef", "  let iN := iN + 1", "loop"]


def tracked_select(vars_):
    lines = []
    for k, v in enumerate(vars_):
        lines.append(f"{'if' if k == 0 else 'elseif'} %{{i}} == {k}")
        lines.append(f"  let rA := {v}")
    return lines + ["endif"] if vars_ else []


def probe_part(part, decls, setup, select, skip_disabled):
    """A probe script returning one '|'-separated record of `part`'s fields for rA, or
    0 when there is no actor (or it is disabled, if skip_disabled)."""
    refs = [f"ref rX{k}" for k, (_, kind, expr) in enumerate(part) if kind == "ref" and expr]
    nums = [f"float fV{k}" for k, (_, kind, _) in enumerate(part) if kind == "num"]
    body = list(setup) + list(select) + ["let fS := 0", "if rA", "  let fS := 1"]
    if skip_disabled:
        body += ["  if rA.GetDisabled", "    let fS := 0", "  endif"]
    body += ["endif", "if fS == 0", "  SetFunctionValue 0", "else"]
    pieces = []
    for k, (_, kind, expr) in enumerate(part):
        if kind == "ref" and expr is None:
            pieces.append("(GetFormIDString rA)")
        elif kind == "ref":
            body.append(f"  let rX{k} := rA.{expr}")
            pieces.append(f"(GetFormIDString rX{k})")
        else:
            body.append(f"  let fV{k} := rA.{expr}")
            pieces.append(f"($ fV{k})")
    body += ["  SetFunctionValue (" + ' + "|" + '.join(pieces) + ")", "endif"]
    return cs.function(list(decls) + ["ref rA"] + refs + ["int iN", "float fS"] + nums, body)


def header_script(kind, expr):
    if kind == "ref":
        return cs.function(["ref rX"], [f"let rX := {expr}", "SetFunctionValue (GetFormIDString rX)"])
    return cs.function(["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"])


def count_script(expr):
    return cs.function(["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"])


def until_hours_script(var, hours):
    """1 once GameDaysPassed has passed the remembered `var` by `hours` game hours."""
    return cs.function(["float fV"], [f"if GameDaysPassed >= %{{{var}}} + {cs.number(hours / 24)}", "  let fV := 1",
                                      "endif", "SetFunctionValue fV"])


def set_hour_script(hour):
    """Move the clock forward to the next `hour` o'clock (today or tomorrow). Setting
    GameHour past 24 carries the day [observed 2026-10-08, xnvse-capture.md section 7]."""
    h = float(hour)
    if not 0 <= h < 24:
        raise InputError(f"hour must be in [0, 24), got {hour!r}")
    return cs.function(["float fH"], [f"let fH := {cs.number(h)}", "if GameHour >= fH", "  let fH := fH + 24",
                                      "endif", "set GameHour to fH"], returns_value=False)


def walk_script(target, within, ctx):
    """One game loop of walking to `target`: face it (GetAngle Z + GetHeadingAngle, the
    wiki's formula on GetHeadingAngle) and hold the key bound to Forward (GetControl 0,
    HoldKey); 1 and the key released once the player is closer than `within`. The target
    must be in the loaded area (GetDistance ends the script otherwise; the drive then
    stops with an error, recorded)."""
    n = cs.Names(*ctx)
    t = n.operand(target)
    return cs.function(n.decls + ["int iK", "float fA", "float fD"],
                       n.setup + [f"let iK := GetControl {CONTROL_FORWARD}", f"let fD := player.GetDistance {t}",
                                  f"if fD < {cs.number(within)}", "  ReleaseKey iK", "  let fA := 1", "else",
                                  f"  let fA := player.GetAngle Z + player.GetHeadingAngle {t}",
                                  "  player.SetAngle Z fA", "  HoldKey iK", "  let fA := 0", "endif",
                                  "SetFunctionValue fA"])


def release_script():
    return cs.function(["int iK"], [f"let iK := GetControl {CONTROL_FORWARD}", "ReleaseKey iK"], returns_value=False)


def groups(depth, max_actors, tracked, ctx):
    """The observe step's groups: actors in the loaded area, and named actors."""
    near = {"name": "nearby", "count": count_script(f"GetNumRefs {ACTOR_TYPE} {int(depth)}"),
            "probe": [probe_part(p, [], [], nearby_select(depth), True) for p in PARTS], "max": int(max_actors)}
    out = [near]
    if tracked:
        n = cs.Names(*ctx)
        vars_ = [n.operand(t) for t in tracked]
        out.append({"name": "tracked", "count": count_script(str(len(tracked))),
                    "probe": [probe_part(p, n.decls, n.setup, tracked_select(vars_), False) for p in PARTS],
                    "max": len(tracked)})
    return out


def fields_doc(tracked):
    return {"header": [[name, kind] for name, kind, _ in HEADER],
            "parts": [[[name, kind] for name, kind, _ in part] for part in PARTS],
            "tracked": list(tracked)}


# --------------------------------------------------------------------------- campaigns

def merged(campaign, session):
    s = dict(DEFAULTS)
    s.update(campaign.get("defaults", {}))
    s.update(session)
    return s


def check_campaign(c):
    if c.get("format") != CAMPAIGN_FORMAT:
        raise InputError(f"not a {CAMPAIGN_FORMAT} document")
    cs.plain_name(c.get("name"), "the campaign name")
    names = [s.get("name") for s in c.get("sessions", [])]
    if not names:
        raise InputError("the campaign has no sessions")
    for n in names:
        cs.plain_name(n, "a session name")
    if len(set(names)) != len(names):
        raise InputError("session names must be unique")
    known = {"name", "go", "hour", "hours", "walk", "track", "every", "depth", "settle_frames", "timescale",
             "max_actors", "console_first"}
    for s in c["sessions"] + [c.get("defaults", {})]:
        extra = set(s) - known
        if extra:
            raise InputError(f"unknown session keys {sorted(extra)}")


def job_name(campaign, session):
    return f"obs_{campaign['name']}_{session['name']}"


def go_steps(go, ctx):
    """Teleport the player: {"coc": cell}, {"cow": [world, x, y]} or {"moveto": ref}."""
    if not go:
        return []
    if not isinstance(go, dict) or len(go) != 1:
        raise InputError(f"go must be one of coc / cow / moveto, got {go!r}")
    (how, what), = go.items()
    if how == "coc":
        if ctx[1] is None:  # no --data: the arrival cannot be checked by FormID
            return [{"console": f"coc {cs.plain_name(what, 'the coc cell')}"}, {"frames": cs.AFTER_DOOR_FRAMES}]
        return cs.go_to_cell_steps(what, ctx)
    if how == "cow":
        if not (isinstance(what, list) and len(what) == 3):
            raise InputError("cow takes [worldspace, x, y]")
        world, x, y = what
        return [{"console": f"cow {cs.plain_name(world, 'the cow worldspace')} {int(x)} {int(y)}"},
                {"frames": cs.AFTER_DOOR_FRAMES}]
    if how == "moveto":
        n = cs.Names(*ctx)
        t = n.operand(what)
        arrived = cs.function(n.decls + ["float fV"], n.setup + [f"let fV := player.GetInSameCell {t}", "SetFunctionValue fV"])
        return [{"script": cs.run_script(["player.MoveTo {0}"], ctx, [what])},
                {"until": arrived, "max_frames": cs.DOOR_LOOPS}, {"frames": cs.AFTER_DOOR_FRAMES}]
    raise InputError(f"unknown go {how!r} (coc, cow or moveto)")


def session_seconds(s):
    ts = s.get("timescale") or DEFAULT_TIMESCALE
    return float(s["hours"]) * 3600 / ts


def session_job(campaign, session, ctx):
    s = merged(campaign, session)
    hours = float(s.get("hours", 0))
    if not hours > 0:
        raise InputError(f"session {s['name']}: hours must be positive")
    every = int(s["every"])
    if every < 1:
        raise InputError(f"session {s['name']}: every must be at least 1")
    tracked = list(s.get("track", []))
    steps = [{"console": line} for line in s["console_first"]]
    if s.get("hour") is not None:
        steps += [{"script": set_hour_script(s["hour"])}, {"frames": cs.SETTLE_FRAMES_LONG}]
    steps += go_steps(s.get("go"), ctx)
    steps.append({"frames": int(s["settle_frames"])})
    if s.get("timescale"):
        steps += [{"remember": cs.global_query("TimeScale"), "as": "ts0"},
                  {"console": f"set TimeScale to {cs.number(s['timescale'])}"}]
    steps.append({"remember": cs.global_query("GameDaysPassed"), "as": "t0"})
    loops = session_seconds(s) * MAX_FPS
    obs = {"label": s["name"], "fields": fields_doc(tracked), "header": [header_script(k, e) for _, k, e in HEADER],
           "groups": groups(s["depth"], s["max_actors"], tracked, ctx), "every": every,
           "until": until_hours_script("t0", hours), "max_samples": int(math.ceil(loops / every)) + 10}
    walk = s.get("walk")
    if walk:
        obs["drive"] = walk_script(walk["to"], float(walk.get("within", 150)), ctx)
        obs["drive_frames"] = int(walk.get("max_frames", 3600))
        obs["drive_end"] = release_script()
    steps.append({"observe": obs})
    if walk:
        steps.append({"script": release_script()})
    if s.get("timescale"):
        steps.append({"console": "set TimeScale to %{ts0}"})
    steps.append({"frames": cs.SETTLE_FRAMES})
    start = f"Observer session {s['name']} of campaign {campaign['name']}: load the campaign's save."
    return cs.job(job_name(campaign, s), start, steps, ctx[0])


def campaign_jobs(campaign, edids=None, masters=cs.MASTERS, only=None):
    check_campaign(campaign)
    out = {}
    for session in campaign["sessions"]:
        if only and session["name"] not in only:
            continue
        out[job_name(campaign, session)] = (merged(campaign, session), session_job(campaign, session, (masters, edids)))
    if only and not out:
        raise InputError(f"no session called {only}")
    return out


# --------------------------------------------------------------------------- diagnostic

def diag_job():
    """Each construct the observer relies on, as its own step, so one failure does not
    hide the others (run on a save with a few NPCs around)."""
    steps = []
    for v in (7, 1234.5678, -98765.4321, 0.00123, 123456.75):
        steps.append(cs.query(cs.function(["float fV"], [f"let fV := {cs.number(v)}", "SetFunctionValue ($ fV)"]),
                              "format", f"$ {v}"))
    steps.append(cs.query(count_script(f"GetNumRefs {ACTOR_TYPE} 1"), "scan", "GetNumRefs"))
    for name, kind, expr in HEADER:
        steps.append(cs.query(header_script(kind, expr), "header", name))
    # Every field on its own, on the first actor of the scan (the player is not in it
    # [observed 2026-10-08, aa_observe_diag]).
    for part in PARTS:
        for f in part:
            script = probe_part([f], [], [], nearby_select(1), True).replace("%{i}", "0")
            steps.append(cs.query(script, "field", f[0]))
    for k, part in enumerate(PARTS):
        for i in range(3):
            steps.append(cs.query(probe_part(part, [], [], nearby_select(1), True).replace("%{i}", str(i)),
                                  "part", f"part{k}.actor{i}"))
    # Key emulation: hold Forward for 60 game loops; the player's position should change.
    pos = [cs.query(header_script("num", f"player.GetPos {a}"), "keys", None) for a in "XY"]
    for q, a in zip(pos, "XY"):
        q["field"] = f"before.{a}"
    steps += pos
    steps.append(cs.query(count_script(f"GetControl {CONTROL_FORWARD}"), "keys", "forward_scancode"))
    steps.append({"script": cs.function(["int iK"], [f"let iK := GetControl {CONTROL_FORWARD}", "HoldKey iK"],
                                        returns_value=False)})
    steps.append({"frames": 60})
    steps.append({"script": release_script()})
    steps.append({"frames": 30})
    for a in "XY":
        steps.append(cs.query(header_script("num", f"player.GetPos {a}"), "keys", f"after.{a}"))
    steps.append({"script": cs.function(["float fA"], ["let fA := player.GetAngle Z + 90", "player.SetAngle Z fA"],
                                        returns_value=False)})
    steps.append({"frames": 5})
    steps.append(cs.query(header_script("num", "player.GetAngle Z"), "keys", "heading_after_turn"))
    # A short observation through the plugin's observe step (plugin version 4).
    steps.append({"observe": {"label": "diag", "fields": fields_doc([]),
                              "header": [header_script(k, e) for _, k, e in HEADER],
                              "groups": groups(1, 64, [], (cs.MASTERS, None)), "every": 30, "max_samples": 5}})
    return cs.job("aa_observe_diag", "Load a save with a few NPCs nearby (e.g. Goodsprings by day).", steps, cs.MASTERS)


# --------------------------------------------------------------------------- running

def supervise(jobs, capture_dir, game, save, retries, stage_dir, clock=time.time, sleep=time.sleep, run=cr.run):
    """Run `jobs` ({name: (session, job)}) unattended; after a crash or hang start the
    game again with the sessions that have not ended. A session that was cut off
    `retries` + 1 times is given up. Returns the run report (dict)."""
    os.makedirs(stage_dir, exist_ok=True)
    paths = {}
    for name, (_, job) in jobs.items():
        paths[name] = os.path.join(stage_dir, f"{name}.steps.json")
        cs.write(job, paths[name])
    state = {n: {"status": "pending", "cut_off": 0, "outputs": []} for n in jobs}
    report = {"format": RUN_FORMAT, "save": save, "started": int(clock()), "rounds": [], "problems": []}
    try:
        _rounds(jobs, paths, state, report, capture_dir, game, save, retries, clock, sleep, run)
    except KeyboardInterrupt:
        # Ctrl+C: keep what the running sessions wrote, close the game, tidy up.
        todo = [n for n, s in state.items() if s["status"] not in ("ended", "given up")]
        for n, s in cr.jobs_state(capture_dir, todo, report.get("_round_start", report["started"])).items():
            if s["output"] and s["output"] not in state[n]["outputs"]:
                state[n]["outputs"].append(s["output"])
            state[n]["status"] = "ended" if s["status"] == "ended" else "interrupted"
        if game.running():
            game.kill()
        report["problems"].append("interrupted (Ctrl+C); the game was closed")
    report.pop("_round_start", None)
    for n in jobs:
        stale = os.path.join(capture_dir, "jobs", f"{n}.steps.json")
        if state[n]["status"] != "ended" and os.path.exists(stale):
            os.remove(stale)  # an unfinished session must not run in a later, unrelated session
    report["sessions"] = state
    return report


def _rounds(jobs, paths, state, report, capture_dir, game, save, retries, clock, sleep, run):
    max_rounds = (retries + 1) * len(jobs) + 1
    for _ in range(max_rounds):
        todo = sorted(n for n, s in state.items() if s["status"] not in ("ended", "given up"))
        if not todo:
            break
        for n in jobs:  # the plugin runs every job file in jobs/: leave only ours that are due
            stale = os.path.join(capture_dir, "jobs", f"{n}.steps.json")
            if n not in todo and os.path.exists(stale):
                os.remove(stale)
        if game.running():
            game.kill()
        timeout = int(sum(session_seconds(jobs[n][0]) for n in todo) * 1.5) + SESSION_MARGIN * len(todo)
        report["_round_start"] = int(clock())
        rr = run({n: (paths[n], jobs[n][1]) for n in todo}, todo, capture_dir, game, save, timeout,
                 kill_on_timeout=True, clock=clock, sleep=sleep)
        report["rounds"].append({"sessions": todo, "problems": rr["problems"],
                                 "jobs": {n: {"status": s["status"], "output": s["output"], "errors": s["errors"]}
                                          for n, s in rr["jobs"].items()}})
        progress = False
        for n in todo:
            s = rr["jobs"][n]
            if s["output"]:
                state[n]["outputs"].append(s["output"])
            if s["status"] == "ended":
                state[n]["status"] = "ended"
                progress = True
            elif s["status"] in ("running", "aborted"):  # it was cut off (crash, hang, timeout)
                progress = True
                state[n]["cut_off"] += 1
                state[n]["status"] = "given up" if state[n]["cut_off"] > retries else "cut off"
        fatal = [p for p in rr["problems"] if "did not start" in p or "did not read auto.json" in p or p.startswith("plugin:")]
        if fatal:
            report["problems"].append("stopped: " + "; ".join(fatal))
            break
        if not progress:
            report["problems"].append("stopped: a round made no progress (no session started)")
            break


# --------------------------------------------------------------------------- report

def parse_value(kind, text, mods):
    if kind == "ref":
        if text is None or text == "" or int(text, 16) == 0:
            return None
        try:
            return cs.to_global(text, mods)
        except (ValueError, cs.InputError):
            return f"?{text}"
    try:
        return float(text)
    except (TypeError, ValueError):
        return None


def read_samples(lines, source=""):
    """(mods, observations [dict]) from one plugin output file; a file cut off by a
    crash is read up to its last complete line."""
    mods, observations, current = {}, [], None
    for line in lines:
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError:
            continue  # the last line of a crashed run may be incomplete
        kind = rec.get("kind")
        if kind == "result" and rec.get("label") == cs.MODINDEX_LABEL and isinstance(rec.get("value"), (int, float)):
            if int(rec["value"]) != cs.NOT_LOADED:
                mods[int(rec["value"])] = rec["field"]
        elif kind == "observing":
            current = {"source": source, "label": rec.get("label"), "fields": rec.get("fields") or {},
                       "samples": [], "end": None}
            observations.append(current)
        elif kind == "sample" and current is not None:
            current["samples"].append(rec)
        elif kind == "observed" and current is not None:
            current["end"] = rec.get("reason")
        elif kind == "drive" and current is not None:
            current.setdefault("drives", []).append({"reason": rec.get("reason"), "loops": rec.get("loops")})
    return mods, observations


def game_time(head):
    """Game time in hours for a sample: the day from GameDaysPassed, the time of day from
    GameHour (which schedules follow). After the clock was moved with `set GameHour`,
    GameDaysPassed lagged GameHour by about 0.054 h [observed 2026-10-08, campaign
    goodsprings]; whole days are taken from GameDaysPassed, rounded to agree with GameHour."""
    days, hour = head.get("days"), head.get("hour")
    if not isinstance(days, (int, float)):
        return None
    if not isinstance(hour, (int, float)):
        return days * 24
    return 24 * round((days * 24 - hour) / 24) + hour


def decode_subject(parts, part_fields, mods):
    rec = {}
    for values, names in zip(parts, part_fields):
        if values is None:
            continue
        texts = values.split("|") if isinstance(values, str) else [values]
        for (name, kind), text in zip(names, texts):
            rec[name] = parse_value(kind, text if isinstance(text, str) else str(text), mods)
    return rec


def label_of(value, field, names):
    if value is None:
        return None
    if field == "package_type":
        return PACKAGE_TYPES.get(int(value), int(value))
    if field == "procedure":
        return PROCEDURES.get(int(value), int(value))
    if isinstance(value, str) and names:
        return names.get(int(value, 16), value) if value.startswith("0x") else value
    return value


def timeline(sources, names=None):
    """fnv.observe.timeline/1 from [(source, lines)]; `names`: global FormID -> Editor ID."""
    actors, events, problems, sessions = {}, [], [], []
    for source, lines in sources:
        mods, observations = read_samples(lines, source)
        if not mods:
            problems.append(f"{source}: no mod indices (not an nvf_capture output?)")
            continue
        for ob in observations:
            f = ob["fields"]
            header_fields = [tuple(h) for h in f.get("header", [])]
            part_fields = [[tuple(x) for x in p] for p in f.get("parts", [])]
            sessions.append({"source": source, "label": ob["label"], "samples": len(ob["samples"]), "end": ob["end"],
                             "drives": ob.get("drives", [])})
            for smp in ob["samples"]:
                head = {}
                for (name, kind), v in zip(header_fields, smp.get("header", [])):
                    head[name] = parse_value(kind, v, mods) if kind == "ref" else v
                t = game_time(head)
                if t is None:
                    problems.append(f"{source}: sample {smp.get('n')} has no game time; skipped")
                    continue
                # One record per actor and sample: a named actor that is also near the
                # player is in both groups; the nearby record wins and marks it seen.
                merged_recs = {}
                for group, subjects in sorted((smp.get("groups") or {}).items(), key=lambda g: g[0] != "nearby"):
                    for parts in subjects or []:
                        rec = decode_subject(parts, part_fields, mods)
                        aid = rec.get("id")
                        if not aid:
                            continue
                        if aid in merged_recs:
                            merged_recs[aid][1].append(group)
                            continue
                        rec["seen"] = group == "nearby" or None
                        merged_recs[aid] = (rec, [group])
                for aid, (rec, grps) in merged_recs.items():
                    if None not in (rec.get("x"), rec.get("y"), head.get("player_x"), head.get("player_y")):
                        rec["player_distance"] = math.hypot(rec["x"] - head["player_x"], rec["y"] - head["player_y"])
                    add_sample(actors, events, aid, grps, t, rec, ob["label"])
    for aid, a in actors.items():
        close(a)
        a["edid"] = label_of(aid, "id", names)
        a["base_edid"] = label_of(a["base"], "base", names)
        for seg in a["segments"]:
            for k in ("cell", "package", "package_target", "combat_target"):
                if seg["state"].get(k):
                    seg["state"][k + "_edid"] = label_of(seg["state"][k], k, names)
            for k in ("package_type", "procedure"):
                if seg["state"].get(k) is not None:
                    seg["state"][k + "_name"] = label_of(seg["state"][k], k, names)
        a["summary"] = summarise(a)
        del a["_open"]
    events.sort(key=lambda e: (e["t"], e["actor"]))
    for e in events:
        e["actor_edid"] = label_of(e["actor"], "id", names)
        if e["field"] in ("package_type", "procedure"):
            e["from_name"], e["to_name"] = label_of(e["from"], e["field"], names), label_of(e["to"], e["field"], names)
        elif isinstance(e["to"], str) or isinstance(e["from"], str):
            e["from_name"], e["to_name"] = label_of(e["from"], e["field"], names), label_of(e["to"], e["field"], names)
    return {"format": TIMELINE_FORMAT, "sources": [s for s, _ in sources], "sessions": sessions,
            "actors": dict(sorted(actors.items())), "events": events, "problems": problems,
            "package_types": PACKAGE_TYPES, "procedures": PROCEDURES}


def add_sample(actors, events, aid, groups_, t, rec, label):
    a = actors.get(aid)
    if a is None:
        a = actors[aid] = {"groups": [], "base": rec.get("base"), "samples": 0, "first": t, "last": t,
                           "segments": [], "_open": None}
    for g in groups_:
        if g not in a["groups"]:
            a["groups"].append(g)
    a["samples"] += 1
    a["last"] = max(a["last"], t)
    state = {k: rec.get(k) for k in STATE_FIELDS}
    pos = [rec.get("x"), rec.get("y"), rec.get("z")]
    seg = a["_open"]
    if seg is not None and seg["session"] == label and seg["state"] == state:
        seg["to"] = t
        seg["samples"] += 1
        seg["end_pos"] = pos
        seg["health"] = rec.get("health")
        if None not in pos and None not in seg["_last_pos"]:
            seg["path"] += math.dist(pos, seg["_last_pos"])
        seg["_last_pos"] = pos
        return
    if seg is not None and seg["session"] == label:
        for k in STATE_FIELDS:
            if seg["state"].get(k) != state.get(k):
                events.append({"t": t, "actor": aid, "field": k, "from": seg["state"].get(k), "to": state.get(k)})
    if seg is not None:
        close(a)
    a["_open"] = {"session": label, "from": t, "to": t, "samples": 1, "state": state, "start_pos": pos,
                  "end_pos": pos, "path": 0.0, "_last_pos": pos, "health": rec.get("health")}


def close(a):
    seg = a["_open"]
    if seg is not None:
        seg.pop("_last_pos", None)
        a["segments"].append(seg)
        a["_open"] = None


def summarise(a):
    by_package, by_procedure, cells = {}, {}, []
    combat = 0.0
    for seg in a["segments"]:
        span = seg["to"] - seg["from"]
        st = seg["state"]
        key = st.get("package_edid") or st.get("package") or "none"
        by_package[key] = by_package.get(key, 0.0) + span
        proc = st.get("procedure_name", "none")
        by_procedure[str(proc)] = by_procedure.get(str(proc), 0.0) + span
        cell = st.get("cell_edid") or st.get("cell")
        if cell and cell not in cells:
            cells.append(cell)
        if st.get("in_combat"):
            combat += span
    return {"hours_observed": sum(s["to"] - s["from"] for s in a["segments"]), "segments": len(a["segments"]),
            "hours_by_package": by_package, "hours_by_procedure": by_procedure, "cells": cells, "combat_hours": combat}


def text_summary(tl):
    out = []
    for s in tl["sessions"]:
        out.append(f"session {s['label']}: {s['samples']} samples, ended: {s['end'] or 'cut off'} ({s['source']})")
    for aid, a in tl["actors"].items():
        out.append(f"{a['edid']} ({a['base_edid']}): {a['samples']} samples over {a['summary']['hours_observed']:.2f} h, "
                   f"{len(a['segments'])} segments, cells {a['summary']['cells']}")
        for pkg, h in sorted(a["summary"]["hours_by_package"].items(), key=lambda x: -x[1]):
            out.append(f"    {h:7.2f} h  {pkg}")
    out.append(f"{len(tl['events'])} state changes; problems: {tl['problems'] or 'none'}")
    return "\n".join(out) + "\n"


# --------------------------------------------------------------------------- main

def main(argv=None, game=None, clock=time.time, sleep=time.sleep):
    ap = argparse.ArgumentParser(prog="observe.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("diag")
    d.add_argument("-o", "--output")
    for cmd in ("jobs", "run"):
        p = sub.add_parser(cmd)
        p.add_argument("campaign")
        p.add_argument("--data", help="game Data folder (read-only), to resolve Editor IDs")
        p.add_argument("--only", action="append", metavar="SESSION", help="only this session (repeatable)")
        if cmd == "jobs":
            p.add_argument("-o", "--output", help="directory for the job files (default: print them)")
        else:
            p.add_argument("--game", required=True, help="the game folder (holds nvse_loader.exe); only read")
            p.add_argument("--save", help="save to load (default: the campaign's)")
            p.add_argument("--capture-dir", default=cr.DEFAULT_CAPTURE_DIR)
            p.add_argument("--saves-dir", default=os.path.join(os.path.expanduser("~"), "Documents", "My Games",
                                                               "FalloutNV", "Saves"),
                           help="where to check that the save exists (only read)")
            p.add_argument("--retries", type=int, default=2, help="restarts allowed per session after a crash (default 2)")
            p.add_argument("--timeline", help="also write the timeline of every ended session here")
            p.add_argument("-o", "--output", help="run report (default: <capture dir>/observe/<campaign>.<time>.run.json)")
    r = sub.add_parser("report")
    r.add_argument("inputs", nargs="+", metavar="OUT.jsonl")
    r.add_argument("--data", help="game Data folder (read-only), to name forms by Editor ID")
    r.add_argument("--text", action="store_true", help="print a readable summary instead of JSON")
    r.add_argument("-o", "--output")
    args = ap.parse_args(argv)
    try:
        if args.cmd == "diag":
            cs.write(diag_job(), args.output)
            return 0
        if args.cmd == "report":
            names = cs.edid_names(args.data) if args.data else None
            sources = []
            for p in args.inputs:
                with open(p, encoding="utf-8") as f:
                    sources.append((p, f.read().splitlines()))
            tl = timeline(sources, names)
            if args.text:
                text = text_summary(tl)
                if args.output:
                    with open(args.output, "w", encoding="utf-8", newline="\n") as f:
                        f.write(text)
                else:
                    sys.stdout.write(text)
            else:
                cs.write(tl, args.output)
            return 0
        campaign = cs.load_json(args.campaign)
        edids = cs.edid_index(args.data) if args.data else None
        jobs = campaign_jobs(campaign, edids, only=args.only)
        if args.cmd == "jobs":
            for name, (_, job) in jobs.items():
                cs.write(job, os.path.join(args.output, f"{name}.steps.json") if args.output else None)
            return 0
        save = args.save or campaign.get("save")
        if not save:
            raise InputError("no save: pass --save or give the campaign a save")
        if args.retries < 0:
            raise InputError("--retries must be 0 or more")
        game = game or cr.Game(args.game)
        problem = game.check() or cr.check_save(save, args.saves_dir)
        if problem:
            raise InputError(problem)
        if game.running():
            raise InputError(f"{cr.GAME_EXE} is already running; close it first")
        others = [n for n in cr.pending_jobs(args.capture_dir) if n not in jobs]
        if others:
            raise InputError(f"other jobs are pending in {os.path.join(args.capture_dir, 'jobs')}: {others}; move them away first")
    except (OSError, json.JSONDecodeError, InputError, cs.InputError, cr.InputError, KeyError, ValueError, TypeError) as e:
        print(f"observe: {e}", file=sys.stderr)
        return 1
    stage = os.path.join(args.capture_dir, "observe", campaign["name"])
    report = supervise(jobs, args.capture_dir, game, save, args.retries, stage, clock=clock, sleep=sleep)
    report["campaign"] = campaign["name"]
    if args.timeline:
        ended = [o for s in report["sessions"].values() if s["status"] == "ended" for o in s["outputs"][-1:]]
        try:
            names = cs.edid_names(args.data) if args.data else None
            sources = []
            for p in ended:
                with open(p, encoding="utf-8") as f:
                    sources.append((p, f.read().splitlines()))
            cs.write(timeline(sources, names), args.timeline)
            report["timeline"] = args.timeline
        except OSError as e:
            report["problems"].append(f"timeline: {e}")
    out = args.output or os.path.join(stage, f"{campaign['name']}.{report['started']}.run.json")
    cs.write(report, out)
    print(f"observe: report {out}")
    for n, s in report["sessions"].items():
        print(f"  {n}: {s['status']} (cut off {s['cut_off']} time(s)), outputs {s['outputs']}")
    return 0 if all(s["status"] == "ended" for s in report["sessions"].values()) else 2


if __name__ == "__main__":
    sys.exit(main())
