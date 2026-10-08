#!/usr/bin/env python3
"""Writes the swimming capture jobs (Layer 4 workstream 2: swimming against the game;
docs/spec-notes/physics.md section 6.6) into this folder. They run unattended
(capture_run.py, save `nvfdoc`; the jobs leave Doc Mitchell's house and travel with the console's `coc`). No game data
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

import capture_moves as cm  # noqa: E402
import capture_steps as cs  # noqa: E402

START = "nvfdoc (Doc Mitchell's house); leaves through the front door, then coc BoulderBeachCampground (god mode, AI detection off)."
DOC_DOOR = "0x00103E61"         # GSDocMitchellHouseIntDoorREF
CTX = (cs.MASTERS, None)
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
    """Out of Doc Mitchell's house (as the movement jobs do), coc to the beach cell, then
    wait until the player stands in the shore cells' x range, and close the menu that opens
    there: arriving at the beach by coc or by SetPos left MenuMode 1 and the game clock
    standing still (GetCurrentTime unchanged over thousands of loops) until the console's
    CloseAllMenus [observed: sw_diag, sw_diag2, 2026-10-08; which menu is not known]."""
    leave = [{"script": cs.run_script(["{0}.Activate player"], CTX, [DOC_DOOR])},
             {"until": cs.left_cell_until(DOC_DOOR, CTX), "max_frames": cs.DOOR_LOOPS},
             {"frames": cs.AFTER_DOOR_FRAMES}]
    until = cs.function(["float fV"], ["if player.GetPos X > 53248", "  if player.GetPos X < 61440", "    let fV := 1",
                                       "  endif", "endif", "SetFunctionValue fV"])
    return leave + [{"console": f"coc {CELL}"}, {"until": until, "max_frames": cs.DOOR_LOOPS}, {"frames": cs.AFTER_DOOR_FRAMES},
                    {"console": "CloseAllMenus"}, {"frames": 60}, num("MenuMode", "after travel", "menumode")]


# One sample per game loop: "x y z t sw pitch" (t: real milliseconds since %{t0} from the
# game clock at TimeScale 30, as in make_movement_jobs.py; sw: IsSwimming; pitch: GetAngle X).
SAMPLE = cs.function(
    ["float fX", "float fY", "float fZ", "float fT", "float fSw", "float fP"],
    ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
     "let fT := (GetCurrentTime - %{t0}) * 120000", "let fSw := player.IsSwimming", "let fP := player.GetAngle X",
     'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ fSw) + " " + ($ fP))'])
# The same with health: "x y z t sw health".
SAMPLE_H = cs.function(
    ["float fX", "float fY", "float fZ", "float fT", "float fSw", "float fH"],
    ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
     "let fT := (GetCurrentTime - %{t0}) * 120000", "let fSw := player.IsSwimming", "let fH := player.GetAV Health",
     'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ fSw) + " " + ($ fH))'])


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
    """(d) a jump from the floating height; (e) falls into deep water and onto land, health
    read before and after (god mode off, health raised first); (f) held under water at the
    bottom, health over time. Each fall into the water starts from the land: SetPos keeps
    the swimming state (sw_trial: placed 100 above the floating height while swimming, the
    player sank at about 11 units/s instead of falling)."""
    dx, dy = DEEP
    lx, ly, lz = LAND
    on_land = [setpos(lx, ly, lz + 4), angles(90), {"frames": 120}]
    steps = list(PROTECT) + go_there()
    steps += on_land + [setpos(dx, dy, WATER_Z), angles(90), remember_t0()] + samples("enter", 60) + [{"frames": 300}]
    steps += samples("jump pre", 5) + [script("TapControl 12")] + samples("jump", 240)
    steps += [script("TapControl 12")] + samples("jump again", 240)
    steps += on_land + [num("player.GetAV Health", "before tgm", "health"),
                        {"console": "tgm"}, {"frames": 10}, script("player.ModAV Health 5000"), {"frames": 30},
                        num("player.GetAV Health", "raised", "health"), num("player.GetBaseAV Health", "raised", "base")]
    for h in (300, 700, 1500, 3000):
        for label, (x, y, z0) in (("water", (dx, dy, WATER_Z)), ("land", (lx, ly, lz))):
            steps += on_land + [num("player.GetAV Health", f"fall {label} {h} before", "health")]
            steps += [setpos(x, y, z0 + h), angles(90), remember_t0()] + samples(f"fall {label} {h}", 300, health=True)
            steps += [{"frames": 60}, num("player.GetAV Health", f"fall {label} {h}", "health")]
    # (f) dive to the bottom (pitch down, forward held) and stay pressed against it.
    steps += on_land + [setpos(dx, dy, WATER_Z), angles(90), {"frames": 300}, remember_t0(), angles(90, 89), hold(0)]
    steps += samples("held under", 480, every=15, health=True) + [release(0), angles(90, 0)]
    steps += samples("released", 300, health=True)
    steps += on_land + [{"console": "tgm"}, {"frames": 10}, num("player.GetAV Health", "end", "health")]
    j = cs.job("sw_fall_breath", START, steps, cs.MASTERS)
    j["title"] = "Swimming: jumps from the water, falls into water and onto land (health), two minutes held under water (about 5 min, no input)"
    return j


def breath():
    """(f) only: two minutes held at the bottom with god mode off and Health raised (the
    sw_fall_breath run died at its last land fall before reaching this part), and the
    700-unit fall into the water again (how far below the swimming depth a fall stops)."""
    dx, dy = DEEP
    lx, ly, lz = LAND
    on_land = [setpos(lx, ly, lz + 4), angles(90), {"frames": 120}]
    steps = list(PROTECT) + go_there()
    steps += on_land + [setpos(dx, dy, WATER_Z + 700), angles(90), remember_t0()] + samples("fall water 700 again", 240)
    steps += on_land + [setpos(dx, dy, WATER_Z), angles(90), {"frames": 300}]
    steps += [num("player.GetAV Health", "before tgm", "health"), {"console": "tgm"}, {"frames": 10},
              script("player.ModAV Health 5000"), {"frames": 30}, num("player.GetAV Health", "raised", "health")]
    steps += [remember_t0(), angles(90, 89), hold(0)] + samples("held under", 480, every=15, health=True)
    steps += [release(0), angles(90, 0)] + samples("released", 300, every=2, health=True)
    steps += [{"console": "tgm"}, {"frames": 10}, num("player.GetAV Health", "end", "health")]
    j = cs.job("sw_breath", START, steps, cs.MASTERS)
    j["title"] = "Swimming: two minutes held under water (health), a fall into the water (about 3 min, no input)"
    return j


def scenarios():
    """Held-out swim scenarios (L4.phys.walk.swim_*): the candidate's own inputs; each
    starts on land near its water (`reset_at`), so the player is not swimming when placed
    at the surface."""
    lab = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")
    docs = [cs.load_json(os.path.join(lab, f)) for f in ("swim_cove_dive.walk.json", "swim_lake_shore.walk.json")]
    return cm.make_job(docs, "sw_scenarios", "nvfdoc (Doc Mitchell's house); leaves through the front door (god mode, AI detection off).",
                       leave_door=DOC_DOOR, console_first=["tgm", "tdetect"])


H2O_CELL = "NVDLC03SLH2OINT"      # Old World Blues interior with water (CELL XCLW -232)
H2O_CELL_ID = "0x03006881"


def heldout2():
    """Second held-out swim scenarios, made after the stroke cycle (physics.md 6.6):
    forward for 2.5 s above Hoover Dam, backwards for 2 s off Callville Bay."""
    lab = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")
    docs = [cs.load_json(os.path.join(lab, f)) for f in ("swim_dam_forward.walk.json", "swim_bay_back.walk.json")]
    return cm.make_job(docs, "sw_heldout2", "nvfdoc (Doc Mitchell's house); leaves through the front door (god mode, AI detection off).",
                       leave_door=DOC_DOOR, console_first=["tgm", "tdetect"])


def h2o():
    """Interior water (held out): drops in NVDLC03SLH2OINT after coc there."""
    lab = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")
    doc = cs.load_json(os.path.join(lab, "swim_h2o_drops.drops.json"))
    steps = list(PROTECT) + [{"console": f"coc {H2O_CELL}"}, {"until": cs.in_cell_until(H2O_CELL_ID, CTX), "max_frames": cs.DOOR_LOOPS},
                             {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": 60},
                             num("MenuMode", "after travel", "menumode")]
    steps += cm.drop_steps(doc)
    j = cs.job("sw_h2o", "nvfdoc (Doc Mitchell's house); coc NVDLC03SLH2OINT (god mode, AI detection off).", steps, cs.MASTERS)
    j["title"] = "Interior water drops: swim_h2o_drops (no input needed)"
    return j


# Banks of graded slope (physics.md 6.6, leaving the water): straight runs of terrain only
# (no reference within 30 units of the line for 700 units either side of where the LAND
# reaches the floating height, rays with and without meshes agreeing), found from the LAND
# heights of the cells with their own water (2026-10-08). Per bank: name, surface, the
# slope (degrees) and heading (compass, uphill) where the bed meets the floating height,
# the start 450 units out over deep water, and a reset spot on land 700 units up the bank
# (12 above the LAND) where the player stands before each entry, so that it enters by
# falling as in the scenarios.
BANKS = [
    ("lm_12", 2600.0, 12.5, 225.0, (53310, 41534), (52497, 40721, 2790)),
    ("lm_16", 2600.0, 16.4, 212.0, (52974, 41854), (52365, 40878, 2773)),
    ("lm_25", 2600.0, 25.3, 180.0, (127744, 31682), (127744, 30532, 2814)),
    ("lm_30", 2600.0, 30.4, 235.5, (126579, 96767), (125631, 96115, 2974)),
    ("lm_41", 2600.0, 41.5, 186.1, (148528, 30399), (148405, 29256, 3079)),
    ("lm_45", 2600.0, 45.1, 58.0, (72834, 42770), (73810, 43379, 3968)),
    ("lm_49", 2600.0, 49.3, 246.4, (126876, 101812), (125823, 101351, 3157)),
    ("lm_58", 2600.0, 58.3, 32.8, (78604, 37510), (79227, 38477, 3730)),
    ("llv_14", 3000.0, 14.3, 246.0, (29339, 59063), (28288, 58596, 3098)),
    ("llv_25", 3000.0, 24.7, 98.1, (31810, 59455), (32949, 59293, 3076)),
]
# Repeats of what was seen: the swim_lake_shore line at Lake Las Vegas (its own start and
# reset) running and walking, and Boulder Beach's run-out line run from the floating height
# (sw_trial ran it while still sinking and walked it from the floating height).
LLV_LINE = ("llv_line", 3000.0, 27.3, 270.0, (25784, 55100), (24700, 54400, 3092))
BB_LINE = ("bb_line", 2600.0, 28.1, 270.0, (57700, 37536), (55500, 37536, 2964))


def in_square_until(gx, gy):
    """Until the player stands in the exterior grid square (gx, gy)."""
    x0, y0 = gx * 4096, gy * 4096
    return cs.function(["float fV"], [f"if player.GetPos X >= {x0}", f"  if player.GetPos X < {x0 + 4096}",
                                      f"    if player.GetPos Y >= {y0}", f"      if player.GetPos Y < {y0 + 4096}",
                                      "        let fV := 1", "      endif", "    endif", "  endif", "endif",
                                      "SetFunctionValue fV"])


def bank_trial(bank, controls=(0,), n=240, tag=""):
    """Load the bank's own cell with the console's `cow` (CenterOnWorld), as `coc` loads
    Boulder Beach, stand on land, SetPos to the surface over deep water (enter by falling),
    float, then hold the keys towards the bank, one sample per loop. A long SetPos alone
    left the land drawn as distant LOD with the player passing through it and the world
    sometimes frozen (sw_banks first run, the owner's screenshot; physics.md 6.6)."""
    name, surface, _slope, heading, (sx, sy), reset = bank
    label = name + tag
    gx, gy = int(sx // 4096), int(sy // 4096)
    travel = [{"console": f"cow WastelandNV {gx} {gy}"}, {"until": in_square_until(gx, gy), "max_frames": cs.DOOR_LOOPS},
              {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": 60},
              num("MenuMode", f"{label} menumode", "value")]
    return travel + [setpos(*reset), {"console": "CloseAllMenus"}, {"frames": 180}, num("player.GetPos Z", f"{label} reset", "z"),
            setpos(sx, sy, surface), angles(heading), {"console": "CloseAllMenus"}, remember_t0()] + \
        samples(f"{label} settle", 30) + [{"frames": 270}, {"console": "CloseAllMenus"}, {"frames": 30},
                                          num("MenuMode", f"{label} keys menumode", "value")] +         move(label, list(controls), n=n, after=30)


def banks():
    """Leaving the water up banks of graded slope, and the two shore lines again."""
    steps = list(PROTECT) + go_there()
    steps += bank_trial(LLV_LINE) + bank_trial(LLV_LINE, controls=(9, 0), n=480, tag=" walk") + bank_trial(BB_LINE)
    for b in BANKS:
        steps += bank_trial(b)
    j = cs.job("sw_banks", START, steps, cs.MASTERS)
    j["title"] = "Swimming: out of the water up banks of 12 to 58 degrees, and the Lake Las Vegas and Boulder Beach lines again (about 4 min, no input)"
    return j


GUARDIAN_CELL = "CampGuardianCaves"   # interior with placed water (z 6528) and XCLW 6712
GUARDIAN_CELL_ID = "0x0013DD63"


def guardian():
    """Placed water, held out: drops in CampGuardianCaves, each after a fresh coc (so the
    player is not swimming when placed; sw_h2o carried the swimming state from one drop to
    the next, physics.md 6.6)."""
    lab = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")
    doc = cs.load_json(os.path.join(lab, "swim_guardian_drops.drops.json"))
    steps = list(PROTECT)
    for d in doc["drops"]:
        steps += [{"console": f"coc {GUARDIAN_CELL}"}, {"until": cs.in_cell_until(GUARDIAN_CELL_ID, CTX), "max_frames": cs.DOOR_LOOPS},
                  {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": 60},
                  num("MenuMode", f"{d['label']} menumode", "value"), num("player.GetPos Z", f"{d['label']} before", "z")]
        steps += cm.drop_steps(dict(doc, drops=[d]))
    j = cs.job("sw_guardian", "nvfdoc (Doc Mitchell's house); coc CampGuardianCaves before each drop (god mode, AI detection off).",
               steps, cs.MASTERS)
    j["title"] = "Placed water drops: swim_guardian_drops (no input needed)"
    return j


def banks_rest(name="sw_banks_b", which=None):
    """Banks sw_banks did not capture: its second run hung after lm_12 when the game lost
    the focus to another program's window (the owner, 2026-10-08); in sw_banks_b the game
    clock stopped while five trials waited in the water (a menu opened after the travel's
    CloseAllMenus [hypothesis: a level-up after map markers were discovered]), so the
    trials now close menus again just before the keys."""
    steps = list(PROTECT) + go_there()
    for b in BANKS[1:]:
        if which is None or b[0] in which:
            steps += bank_trial(b)
    j = cs.job(name, START, steps, cs.MASTERS)
    j["title"] = f"Swimming: out of the water up banks ({name}, the rest of sw_banks; no input)"
    return j


def diag():
    """Which sample scripts compile, and whether the clock runs after the coc."""
    clock = cs.function(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"])
    steps = list(PROTECT) + go_there()
    steps += [cs.query(clock, "clock", "a"), {"frames": 120}, cs.query(clock, "clock", "b"),
              num("MenuMode", "menumode", "value"), num("player.GetPos X", "pos", "x"), remember_t0()]

    def five(name, extra_decl, extra_let, extra_var):
        return cs.function(["float fX", "float fY", "float fZ", "float fT", extra_decl],
                           ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
                            "let fT := (GetCurrentTime - %{t0}) * 120000", extra_let,
                            f'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ {extra_var}))'])
    variants = {"six": SAMPLE, "health": SAMPLE_H,
                "seconds": five("s", "float fS", "let fS := GetSecondsPassed", "fS"),
                "swimming": five("w", "float fSw", "let fSw := player.IsSwimming", "fSw"),
                "pitch": five("p", "float fP", "let fP := player.GetAngle X", "fP"),
                "health5": five("h", "float fH", "let fH := player.GetAV Health", "fH"),
                "packed": five("q", "float fQ", "let fQ := player.IsSwimming * 1000 + player.GetAngle X", "fQ")}
    for name, sc in variants.items():
        steps += [cs.query(sc, f"variant {name} {i}", "sample") for i in range(3)]
    steps += [num("player.GetAV Rads", "rads", "value"), num("player.GetAV RadiationRads", "radiationrads", "value"),
              num("player.GetAV Health", "health", "value"), num("player.GetBaseAV Health", "base health", "value")]
    j = cs.job("sw_diag", START, steps, cs.MASTERS)
    j["title"] = "Swimming diagnostic: sample scripts, clock after coc (under a minute, no input)"
    return j


def diag2():
    """Why the clock stood still after coc (MenuMode 1 in sw_diag): coc then CloseAllMenus,
    and travel by SetPos instead of coc."""
    clock = cs.function(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"])
    leave = go_there()[:3]  # (written before CloseAllMenus joined go_there)

    def check(tag):
        return [cs.query(clock, f"{tag} clock", "a"), {"frames": 120}, cs.query(clock, f"{tag} clock", "b"),
                num("MenuMode", f"{tag} menumode", "value"), num("GetActiveMenuMode", f"{tag} active menu", "value"),
                num("player.GetPos X", f"{tag} pos", "x"), num("player.GetPos Z", f"{tag} pos", "z")]
    sx, sy, sz = SHORE
    steps = list(PROTECT) + leave + check("after door")
    steps += [setpos(sx, sy, sz + 100), {"frames": 300}] + check("setpos")
    steps += [{"console": f"coc {CELL}"}, {"frames": 300}] + check("coc")
    steps += [{"console": "CloseAllMenus"}, {"frames": 60}] + check("closed")
    steps += [setpos(sx, sy, sz + 100), {"frames": 300}] + check("setpos after coc")
    steps += [remember_t0()] + [cs.query(SAMPLE, f"sample {i}", "sample") for i in range(3)]
    j = cs.job("sw_diag2", START, steps, cs.MASTERS)
    j["title"] = "Swimming diagnostic 2: travel by SetPos or coc, menus (under a minute, no input)"
    return j


def main():
    for j in (diag(), diag2(), trial(), fall_breath(), breath(), scenarios(), heldout2(), h2o(), banks(), banks_rest(),
              banks_rest("sw_banks_c", {"lm_16", "lm_45", "lm_49", "lm_58", "llv_25"}), guardian()):
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
