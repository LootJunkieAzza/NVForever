#!/usr/bin/env python3
"""Writes the swimming capture jobs (Layer 4 workstream 2: swimming against the game;
docs/spec-notes/physics.md section 6.6) into this folder. They run unattended
(capture_run.py, save `nvftest`; the jobs travel with the console's `coc`). No game data
is stored here: only script text and positions on the Lake Mead shore at Boulder Beach
chosen from the LAND heights and `fnvtool phys walk` / `phys cell` (2026-10-08).

  python tools/capture/jobs/make_swim_jobs.py

Script functions used, from the GECK wiki [documented]: IsSwimming, IsInDangerousWater,
HoldKey / ReleaseKey, TapControl (control codes: 0 forward, 1 back, 2 left, 3 right,
8 sneak, 9 run, 12 jump), GetControl, GetCurrentTime, GetSecondsPassed,
GetNumericGameSetting, SetAngle / GetAngle, SetPos / GetPos, GetAV / ModAV.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402

START = "nvftest (Goodsprings gas station); coc BoulderBeachCampground (god mode, AI detection off)."
CELL = "BoulderBeachCampground"          # WastelandNV (13, 9), Lake Mead shore
WATER_Z = 2600.0                         # CELL XCLW of the shore cells [observed: masters]
# Shore: LAND 12 units above the water at (56992, 37536), falling east at about 12 degrees
# (depth 44 at +256, 140 at +768, 326 at +1536 units); `fnvtool phys walk` east from it
# touches only terrain for 1,735 units [observed].
SHORE = (56992.0, 37536.0, 2616.0)
# Deep water: LAND about 800 below the surface, more to the east and north-east.
DEEP = (59500.0, 38500.0)
# Flat-ish land west of the beach (LAND 2959.5, slope about 8 degrees, terrain only).
LAND = (55500.0, 37536.0, 2959.5)
PROTECT = [{"console": "tgm"}, {"console": "tdetect"}]


def script(*lines, decls=()):
    return {"script": cs.function(list(decls), list(lines), returns_value=False)}


def num(expr, label, field):
    return cs.query(cs.function(["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"]), label, field)


def setpos(x, y, z):
    return script(f"player.SetPos X {cs.number(x)}", f"player.SetPos Y {cs.number(y)}", f"player.SetPos Z {cs.number(z)}")


def angles(heading, pitch=0.0):
    return script(f"player.SetAngle Z {cs.number(heading)}", f"player.SetAngle X {cs.number(pitch)}")


def go_there():
    """coc to the beach cell, then wait until the player stands in the shore cells' x range."""
    until = cs.function(["float fV"], ["if player.GetPos X > 53248", "  if player.GetPos X < 61440", "    let fV := 1",
                                       "  endif", "endif", "SetFunctionValue fV"])
    return [{"console": f"coc {CELL}"}, {"until": until, "max_frames": cs.DOOR_LOOPS}, {"frames": cs.AFTER_DOOR_FRAMES}]


# One sample per game loop: "x y z t sw pitch" (t: real milliseconds since %{t0} from the
# game clock at TimeScale 30, as in make_movement_jobs.py; sw: IsSwimming; pitch: GetAngle X).
SAMPLE = cs.function(
    ["float fX", "float fY", "float fZ", "float fT", "float fW", "float fP"],
    ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
     "let fT := (GetCurrentTime - %{t0}) * 120000", "let fW := player.IsSwimming", "let fP := player.GetAngle X",
     'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ fW) + " " + ($ fP))'])
# The same with health: "x y z t sw health".
SAMPLE_H = cs.function(
    ["float fX", "float fY", "float fZ", "float fT", "float fW", "float fH"],
    ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
     "let fT := (GetCurrentTime - %{t0}) * 120000", "let fW := player.IsSwimming", "let fH := player.GetAV Health",
     'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ fW) + " " + ($ fH))'])


def samples(prefix, n, every=1, health=False):
    out = []
    for i in range(n):
        out.append(cs.query(SAMPLE_H if health else SAMPLE, f"{prefix} {i:04d}", "sample"))
        if every > 1:
            out.append({"frames": every - 1})
    return out


def remember_t0():
    return {"remember": cs.function(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"]), "as": "t0"}


def hold(*controls):
    return script(*[f"let iK := GetControl {c}\nHoldKey iK" for c in controls], decls=["int iK"])


def release(*controls):
    return script(*[f"let iK := GetControl {c}\nReleaseKey iK" for c in controls], decls=["int iK"])


def at_surface(label, x, y, heading, settle=240):
    """SetPos at the water height over deep water, settle, record how it settled."""
    return [setpos(x, y, WATER_Z), angles(heading), remember_t0()] + samples(f"{label} settle", 60) + \
        [{"frames": settle}]


def move(label, controls, n=120, after=90):
    return samples(f"{label} pre", 5) + [hold(*controls)] + samples(f"{label} held", n) + \
        [release(*controls)] + samples(f"{label} after", after)


def trial():
    """Entry and exit depths, floating height, swim speeds, diving and surfacing."""
    sx, sy, sz = SHORE
    dx, dy = DEEP
    steps = list(PROTECT) + go_there()
    for name in ("fJumpSwimmingMult", "fSwimRadiationDamageMult", "fMoveBaseSpeed", "fMoveRunMult", "fJumpHeightMin"):
        steps.append(num(f'GetNumericGameSetting "{name}"', f"setting {name}", "value"))
    steps += [num("TimeScale", "timescale", "value"), num("player.IsSwimming", "on arrival", "swimming")]
    # (a) walking in (Run key held: walk speed), then floating where it stops; then running in.
    steps += [setpos(sx, sy, sz), angles(90), {"frames": 240}, remember_t0()]
    steps += samples("walk in pre", 5) + [hold(9, 0)] + samples("walk in", 780) + [release(0, 9)]
    steps += samples("float", 300)
    steps += [num("player.IsInDangerousWater", "float", "dangerous"), num("player.GetAV Rads", "float", "rads")]
    steps += [setpos(sx, sy, sz), angles(90), {"frames": 240}, remember_t0()]
    steps += samples("run in pre", 5) + [hold(0)] + samples("run in", 300) + [release(0)] + samples("run in after", 120)
    # (a) back out: from deep water towards the shore (west), running, then walking.
    steps += [setpos(57900.0, sy, WATER_Z), angles(270), {"frames": 240}, remember_t0()]
    steps += samples("run out pre", 5) + [hold(0)] + samples("run out", 420) + [release(0)] + samples("run out after", 60)
    steps += [setpos(57700.0, sy, WATER_Z), angles(270), {"frames": 240}, remember_t0()]
    steps += samples("walk out pre", 5) + [hold(9, 0)] + samples("walk out", 900) + [release(0, 9)] + samples("walk out after", 60)
    # (c) speeds from the surface over deep water, each moving east.
    for label, heading, controls in (("forward", 90, [0]), ("forward walk", 90, [9, 0]), ("back", 270, [1]),
                                     ("right", 0, [3]), ("left", 180, [2]), ("forward right", 45, [0, 3])):
        steps += at_surface(label, dx, dy, heading) + move(label, controls)
    steps += at_surface("sneak", dx, dy, 90) + [script("TapControl 8"), {"frames": 60},
                                                  num("player.IsSneaking", "sneak", "sneaking")]
    steps += move("sneak forward", [0]) + [script("TapControl 8"), {"frames": 60},
                                           num("player.IsSneaking", "sneak after", "sneaking")]
    # (d) diving: forward with the view pitched down, then up; release under water.
    for pitch in (30, 60, 89):
        steps += at_surface(f"dive {pitch}", dx, dy, 90) + [angles(90, pitch)] + move(f"dive {pitch}", [0], n=120, after=300)
    steps += at_surface("surface", dx, dy, 90) + [angles(90, 60)] + samples("surface pre", 5) + [hold(0)]
    steps += samples("surface down", 90) + [angles(90, -45)] + samples("surface up", 240) + [release(0)]
    steps += samples("surface after", 120) + [angles(90, 0)]
    steps += at_surface("pitch only", dx, dy, 90) + [angles(90, 89)] + samples("pitch only", 180) + [angles(90, 0)]
    # Jump and sneak keys in water: tapped at the surface, held at the surface, held below it.
    steps += at_surface("jump tap", dx, dy, 90) + [script("TapControl 12")] + samples("jump tap", 120)
    steps += at_surface("jump held", dx, dy, 90) + move("jump held", [12], n=120)
    steps += at_surface("sneak held", dx, dy, 90) + move("sneak held", [8], n=120) + [{"frames": 30},
                                                                                       num("player.IsSneaking", "sneak held", "sneaking")]
    steps += at_surface("deep jump", dx, dy, 90) + [angles(90, 89), hold(0)] + samples("deep jump down", 60)
    steps += [release(0), angles(90, 0), hold(12)] + samples("deep jump held", 180) + [release(12)] + samples("deep jump after", 60)
    j = cs.job("sw_trial", START, steps, cs.MASTERS)
    j["title"] = "Swimming trial: entry/exit depth, floating height, speeds, diving, jump and sneak keys (about 4 min, no input)"
    return j


def fall_breath():
    """(e) falls into deep water and onto land, health read before and after (god mode off,
    health raised first); (f) held under water at the bottom, health over time."""
    dx, dy = DEEP
    lx, ly, lz = LAND
    steps = list(PROTECT) + go_there()
    steps += [setpos(dx, dy, WATER_Z), {"frames": 240}, num("player.GetAV Health", "before tgm", "health"),
              {"console": "tgm"}, {"frames": 10}, script("player.ModAV Health 5000"), {"frames": 30},
              num("player.GetAV Health", "raised", "health"), num("player.GetBaseAV Health", "raised", "base")]
    for label, (x, y, z0) in (("water", (dx, dy, WATER_Z)), ("land", (lx, ly, lz))):
        for h in (300, 700, 1500, 3000):
            steps += [setpos(x, y, z0 + h), angles(90), remember_t0()] + samples(f"fall {label} {h}", 300, health=True)
            steps += [{"frames": 60}, num("player.GetAV Health", f"fall {label} {h}", "health")]
    # (f) dive to the bottom (pitch down, forward held) and stay pressed against it.
    steps += [setpos(dx, dy, WATER_Z), angles(90), {"frames": 240}, remember_t0(), angles(90, 89), hold(0)]
    steps += samples("held under", 480, every=15, health=True) + [release(0), angles(90, 0)]
    steps += samples("released", 300, health=True)
    steps += [{"console": "tgm"}, {"frames": 10}, num("player.GetAV Health", "end", "health")]
    j = cs.job("sw_fall_breath", START, steps, cs.MASTERS)
    j["title"] = "Swimming: falls into water and onto land (health), two minutes held under water (about 4 min, no input)"
    return j


def main():
    for j in (trial(), fall_breath()):
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
