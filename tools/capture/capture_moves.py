#!/usr/bin/env python3
"""In-game movement captures for the Layer 4 physics scenarios (`L4.phys.drops.*`,
`L4.phys.walk.*`): capture jobs made from the candidate's own inputs, and goldens made from
the jobs' output. Physics facts behind the method: docs/spec-notes/physics.md section 6.3.

Inputs (the files the candidate runs, parity-lab/inputs/layer4/):
- drops (`fnv.phys.drops/1`): `name`, `drops: [{label, at: [x, y, z], ...}]`. In game: the
  player is put at `at` with SetPos (X, Y, Z), the job waits until it has started to fall
  (z below the set height by 1 unit, at most UNTIL_FALL loops), then DROP_SETTLE loops,
  and reads the position, then again after DROP_SETTLE more loops.
- walk scripts (`fnv.phys.walk-script/1`) with a `capture` block: `name`, `set_z` (the
  SetPos height of the start), `heading_deg` (compass: 0 north, 90 east; the view), `keys`
  (controls held from the start: "forward", "back", "left", "right", and "run" = the Run
  key, which walks while Always Run is on), `jump` (tap Jump with the keys), `settle_frames`, `samples` (game loops
  sampled after the keys go down), and optionally `pitch_deg` (SetAngle X before the keys,
  positive looks down; back to 0 after) and `reset_at` ([x, y, z]: SetPos there first, wait
  RESET_FRAMES, then close menus with the console's CloseAllMenus, again after the start's
  SetPos, and record MenuMode before the samples; e.g. to stand on land so that the player
  is not swimming any more, physics.md 6.6). The script's moving segments
  (and their `swim_velocity`) must point along `heading_deg`. In game: SetPos, SetAngle Z,
  wait, then one sample per loop of
  "x y z t s" (t: milliseconds of game clock since the start, at TimeScale 30 a real
  millisecond each 1/120000 hour; s: GetSecondsPassed), keys released at the end.

Goldens: drops -> `observations: [{label, feet}]` (the later reading) for each drop that
fell; a drop that did not fall keeps a row with `not_observed` and no `feet` (and is listed
under `not_observed`); walks -> `observations: [{label: "t0.25", feet}]` at
the script's `observe` times after the movement onset: the samples are paired with the
previous loop's clock (positions lag the clock by one loop [observed: fall and jump fits,
physics.md 6.3]), the onset is the last pair before the first change of horizontal position, and
positions are interpolated linearly in time.

  capture_moves.py job <input>... --start TEXT [--leave-door FORMID] [--console-first LINE]... -o job.json
  capture_moves.py golden <out.jsonl> <input> -o golden.json

Standard library only; imports capture_steps.py (job layout, mod-index queries).
"""
import argparse
import json
import math
import sys

import capture_steps as cs

UNTIL_FALL = 1800
DROP_SETTLE = 300
RESET_FRAMES = 300
KEYS = {"forward": 0, "back": 1, "left": 2, "right": 3, "run": 9}   # control codes [documented: GECK wiki TapControl]
# Direction of motion of a key relative to the view, compass degrees (forward + right).
KEY_DIRECTION = {"forward": (1, 0), "back": (-1, 0), "left": (0, -1), "right": (0, 1)}
JUMP = 12
CTX = (cs.MASTERS, None)


class InputError(Exception):
    pass


def fn(decls, body):
    return cs.function(list(decls), list(body))


def script(*lines, decls=()):
    return {"script": cs.function(list(decls), list(lines), returns_value=False)}


def setpos(x, y, z):
    return script(f"player.SetPos X {cs.number(x)}", f"player.SetPos Y {cs.number(y)}", f"player.SetPos Z {cs.number(z)}")


FEET = fn(["float fX", "float fY", "float fZ"],
          ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
           'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ))'])
SAMPLE = fn(["float fX", "float fY", "float fZ", "float fT", "float fS"],
            ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
             "let fT := (GetCurrentTime - %{t0}) * 120000", "let fS := GetSecondsPassed",
             'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ fS))'])


def below(z):
    return fn(["float fV"], [f"if player.GetPos Z < {cs.number(z - 1)}", "  let fV := 1", "endif", "SetFunctionValue fV"])


def kind(doc):
    if doc.get("schema") == "fnv.phys.drops/1":
        return "drops"
    if doc.get("schema") == "fnv.phys.walk-script/1" and isinstance(doc.get("capture"), dict):
        return "walk"
    raise InputError("an input is a fnv.phys.drops/1 file or a walk script with a \"capture\" block")


def name_of(doc):
    n = doc.get("name") if kind(doc) == "drops" else doc["capture"].get("name")
    if not isinstance(n, str) or not n or " " in n:
        raise InputError("inputs need a \"name\" without spaces (drops: top level; walks: in \"capture\")")
    return n


def drop_steps(doc):
    name = name_of(doc)
    steps = []
    for d in doc["drops"]:
        at = d.get("at")
        if not (isinstance(at, list) and len(at) == 3):
            raise InputError(f"drop {d.get('label')}: the capture needs \"at\": [x, y, z]")
        label = f"{name} {d['label']}"
        steps += [setpos(*at), {"until": below(at[2]), "max_frames": UNTIL_FALL},
                  cs.query(below(at[2]), label, "fell"), {"frames": DROP_SETTLE},
                  cs.query(FEET, label, "feet"), {"frames": DROP_SETTLE}, cs.query(FEET, label, "feet_later")]
    return steps


def heading_of(v):
    return math.degrees(math.atan2(v[0], v[1])) % 360.0


def walk_steps(doc):
    c = doc["capture"]
    name = name_of(doc)
    h = float(c["heading_deg"])
    keys = c.get("keys", ["forward"])
    if not keys or any(k not in KEYS for k in keys):
        raise InputError(f"{name}: keys are some of {sorted(KEYS)}")
    f = sum(KEY_DIRECTION.get(k, (0, 0))[0] for k in keys)
    r = sum(KEY_DIRECTION.get(k, (0, 0))[1] for k in keys)
    move = (h + math.degrees(math.atan2(r, f))) % 360.0 if (f, r) != (0, 0) else h
    for s in doc["segments"]:
        for v in (s.get("velocity"), (s.get("swim_velocity") or [])[:2]):
            if v and math.hypot(*v) > 0 and abs((heading_of(v) - move + 180) % 360 - 180) > 0.5:
                raise InputError(f"{name}: a segment moves at heading {heading_of(v):.1f}, the keys at {move:.1f}")
    x, y = doc["start"][:2]
    down = [f"let iK := GetControl {KEYS[k]}\nHoldKey iK" for k in reversed(keys)]
    up = [f"let iK := GetControl {KEYS[k]}\nReleaseKey iK" for k in keys]
    if c.get("jump"):
        down.append(f"TapControl {JUMP}")
    steps = []
    reset = c.get("reset_at")
    if reset is not None:
        if not (isinstance(reset, list) and len(reset) == 3):
            raise InputError(f"{name}: \"reset_at\" is [x, y, z]")
        steps += [setpos(*reset), {"frames": RESET_FRAMES}, {"console": "CloseAllMenus"}, {"frames": 60}]
    steps += [setpos(x, y, c["set_z"]), script(f"player.SetAngle Z {cs.number(h)}")]
    pitch = c.get("pitch_deg")
    if pitch is not None:
        steps.append(script(f"player.SetAngle X {cs.number(float(pitch))}"))
    if reset is not None:
        # Arriving somewhere new can open a menu that stops the clock (physics.md 6.6).
        steps.append({"console": "CloseAllMenus"})
    steps += [{"frames": int(c.get("settle_frames", 120))}]
    if reset is not None:
        steps.append(cs.query(fn(["float fV"], ["let fV := MenuMode", "SetFunctionValue fV"]), f"{name} menumode", "menumode"))
    steps += [{"remember": fn(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"]), "as": "t0"}]
    steps += [cs.query(SAMPLE, f"{name} s{i:03d}", "sample") for i in range(5)]
    steps.append(script(*"\n".join(down).split("\n"), decls=["int iK"]))
    steps += [cs.query(SAMPLE, f"{name} s{i:03d}", "sample") for i in range(5, 5 + int(c.get("samples", 150)))]
    steps += [script(*"\n".join(up).split("\n"), decls=["int iK"]), {"frames": 60}]
    if pitch is not None:
        steps.append(script("player.SetAngle X 0"))
    return steps


def make_job(docs, name, start, leave_door=None, console_first=()):
    steps = [{"console": line} for line in console_first]
    if leave_door:
        steps += [{"script": cs.run_script(["{0}.Activate player"], CTX, [leave_door])},
                  {"until": cs.left_cell_until(leave_door, CTX), "max_frames": cs.DOOR_LOOPS},
                  {"frames": cs.AFTER_DOOR_FRAMES}]
    names = set()
    for d in docs:
        n = name_of(d)
        if n in names:
            raise InputError(f"two inputs are named {n}")
        names.add(n)
        steps += drop_steps(d) if kind(d) == "drops" else walk_steps(d)
    j = cs.job(name, start, steps, cs.MASTERS)
    j["title"] = f"{name}: " + ", ".join(sorted(names)) + " (no input needed)"
    return j


def parse_numbers(text, n):
    v = [float(t) for t in str(text).split()]
    if len(v) != n:
        raise InputError(f"expected {n} numbers, got {text!r}")
    return v


def drops_golden(doc, results):
    name = name_of(doc)
    by = {}
    for r in results:
        by.setdefault(r["label"], {})[r["field"]] = r.get("value", None) if "error" not in r else None
    obs, missing = [], []
    for d in doc["drops"]:
        got = by.get(f"{name} {d['label']}", {})
        feet = got.get("feet_later") or got.get("feet")
        if not got.get("fell") or feet is None:
            # Kept as a row without `feet`, so the lab compares nothing for it (and the
            # candidate's row is not counted as extra).
            why = "did not fall" if feet is not None else "not read"
            obs.append({"label": d["label"], "not_observed": why})
            missing.append({"label": d["label"], "why": why, "feet": parse_numbers(feet, 3) if feet else None})
            continue
        obs.append({"label": d["label"], "feet": parse_numbers(feet, 3),
                    "feet_first": parse_numbers(got["feet"], 3) if got.get("feet") else None})
    return {"format": "fnv.phys.drops-golden/1", "name": name, "observations": obs, "not_observed": missing}


def walk_golden(doc, results):
    name = name_of(doc)
    rows = sorted((r for r in results if r["label"].startswith(name + " s") and r["field"] == "sample" and "value" in r),
                  key=lambda r: r["label"])
    s = [parse_numbers(r["value"], 5) for r in rows]
    if len(s) < 3:
        raise InputError(f"{name}: too few samples")
    # Pair each position with the previous loop's clock (one-loop lag), times in seconds.
    pairs = [(s[i - 1][3] / 1000.0, s[i][:3]) for i in range(1, len(s))]
    # The onset is the first horizontal change: the player sometimes settles by a tenth
    # of a unit in z a few loops before it starts to move [observed: mv_followup_traces].
    onset = next((i - 1 for i in range(1, len(pairs)) if pairs[i][1][:2] != pairs[i - 1][1][:2]), None)
    if onset is None:
        raise InputError(f"{name}: the player never moved")
    t0 = pairs[onset][0]
    o = doc["observe"]
    hz = doc.get("hz", 60)
    obs = []
    for k in range(1, int(o["count"]) + 1):
        dt = k * int(o["every"]) / hz
        t = t0 + dt
        j = next((j for j in range(onset + 1, len(pairs)) if pairs[j][0] >= t), None)
        if j is None:
            break
        (ta, pa), (tb, pb) = pairs[j - 1], pairs[j]
        f = 0.0 if tb == ta else (t - ta) / (tb - ta)
        obs.append({"label": f"t{dt:.2f}", "feet": [round(a + f * (b - a), 3) for a, b in zip(pa, pb)]})
    return {"format": "fnv.phys.walk-golden/1", "name": name, "onset_sample": onset + 1,
            "samples": len(s), "observations": obs}


def main(argv=None):
    ap = argparse.ArgumentParser(prog="capture_moves.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    j = sub.add_parser("job")
    j.add_argument("inputs", nargs="+")
    j.add_argument("--name", required=True)
    j.add_argument("--start", required=True)
    j.add_argument("--leave-door", help="global FormID of a load door to go through first")
    j.add_argument("--console-first", action="append", default=[])
    j.add_argument("-o", "--output")
    g = sub.add_parser("golden")
    g.add_argument("output_jsonl")
    g.add_argument("input")
    g.add_argument("-o", "--output")
    a = ap.parse_args(argv)
    try:
        if a.cmd == "job":
            docs = [cs.load_json(p) for p in a.inputs]
            cs.write(make_job(docs, a.name, a.start, a.leave_door, a.console_first), a.output)
        else:
            with open(a.output_jsonl, encoding="utf-8") as f:
                _, results = cs.read_output(f.readlines())
            doc = cs.load_json(a.input)
            cs.write(drops_golden(doc, results) if kind(doc) == "drops" else walk_golden(doc, results), a.output)
    except (InputError, cs.InputError, KeyError, ValueError) as e:
        print(f"capture_moves.py: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
