#!/usr/bin/env python3
"""Writes the movement capture jobs (Layer 4 workstream 2: the character controller
against the game; docs/spec-notes/layer4-captures.md section 2, physics.md section 6)
into this folder. They run unattended (capture_run.py, save `nvfdoc`). No game data is
stored here: only script text, FormIDs and positions in Goodsprings chosen with
`fnvtool phys cell --list-refs` / `phys walk` (2026-10-08).

  python tools/capture/jobs/make_movement_jobs.py

Script functions used, from the GECK wiki [documented]: HoldKey / ReleaseKey (fake a held
key by DirectX scan code), TapControl (control codes: 0 forward, 8 crouch, 9 run,
10 always run, 12 jump), GetControl (the scan code bound to a control),
GetSecondsPassed, GetCurrentTime, GetNumericGameSetting, GetNumericINISetting,
IsRunning, SetAngle, SetPos, GetPos.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_moves as cm  # noqa: E402
import capture_steps as cs  # noqa: E402

LAB_INPUTS = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")
START = "nvfdoc (Doc Mitchell's house); leaves through the front door (god mode, AI detection off)."

CTX = (cs.MASTERS, None)
DOC_DOOR = "0x00103E61"         # GSDocMitchellHouseIntDoorREF
PROTECT = [{"console": "tgm"}, {"console": "tdetect"}]

# A flat, open piece of Goodsprings: LAND at z 8352 over 600 units to the north of it,
# nothing but terrain touched by `fnvtool phys walk` heading north [observed].
RUN_START = (-72400.0, 4100.0)
GROUND_Z = 8352.0


def leave_doc_house():
    return [{"script": cs.run_script(["{0}.Activate player"], CTX, [DOC_DOOR])},
            {"until": cs.left_cell_until(DOC_DOOR, CTX), "max_frames": cs.DOOR_LOOPS},
            {"frames": cs.AFTER_DOOR_FRAMES}]


def num(expr, label, field, decls=()):
    return cs.query(cs.function(list(decls) + ["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"]), label, field)


def script(*lines, decls=()):
    return {"script": cs.function(list(decls), list(lines), returns_value=False)}


def setpos(x, y, z):
    return script(f"player.SetPos X {cs.number(x)}", f"player.SetPos Y {cs.number(y)}", f"player.SetPos Z {cs.number(z)}")


# One sample per game loop: "x y z t s", t = real milliseconds since %{t0} from the
# game clock (GameHour runs at TimeScale game seconds per real second; the jobs leave
# TimeScale at the save's 30, read back as `timescale`), s = GetSecondsPassed of this
# script (the plugin compiles a text once and reuses it).
SAMPLE = cs.function(
    ["float fX", "float fY", "float fZ", "float fT", "float fS"],
    ["let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
     "let fT := (GetCurrentTime - %{t0}) * 120000", "let fS := GetSecondsPassed",
     'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fT) + " " + ($ fS))'])


def samples(prefix, n):
    return [cs.query(SAMPLE, f"{prefix} {i:03d}", "sample") for i in range(n)]


def remember_t0():
    return {"remember": cs.function(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"]), "as": "t0"}


def hold(control):
    return script(f"let iK := GetControl {control}", "HoldKey iK", decls=["int iK"])


def release(control):
    return script(f"let iK := GetControl {control}", "ReleaseKey iK", decls=["int iK"])


def trial():
    """What works unattended: GetSecondsPassed across calls, `$` of floats, the bound
    keys, a few settings, held keys moving the player, a tapped jump, a fall."""
    steps = list(PROTECT) + leave_doc_house()
    steps += [num("GetSecondsPassed", f"seconds passed {i}", "s") for i in range(6)]
    steps += [cs.query(cs.function(["float fV"], ["let fV := 1234.56789", "SetFunctionValue ($ fV)"]), "format", "text")]
    steps += [num(f"GetControl {c}", f"control {c}", "scancode") for c in (0, 1, 8, 9, 10, 12)]
    steps += [num("TimeScale", "timescale", "value"), num("player.IsRunning", "running", "value"),
              num("player.GetAV SpeedMult", "speed mult", "value"), num("player.GetScale", "scale", "value")]
    for name in ("fMoveBaseSpeed", "fMoveRunMult", "fJumpHeightMin", "iActivatePickLength"):
        steps.append(num(f'GetNumericGameSetting "{name}"', f"setting {name}", "value"))
    for name in ("fJumpAnimDelay:HAVOK", "fActivatePickSphereRadius:Interface"):
        steps.append(num(f'GetNumericINISetting "{name}"', f"ini {name}", "value"))
    x, y = RUN_START
    steps += [setpos(x, y, GROUND_Z + 4), script("player.SetAngle Z 0"), {"frames": 120},
              num("player.GetPos Z", "start", "z"), num("player.GetAngle Z", "start", "heading")]
    steps += [remember_t0()] + samples("still", 10)
    steps += [hold(0)] + samples("run", 120) + [release(0), {"frames": 60}]
    steps += [num("player.GetPos Y", "after run", "y")]
    steps += [setpos(x, y, GROUND_Z + 4), script("player.SetAngle Z 0"), {"frames": 120}]
    steps += [hold(9), hold(0)] + samples("run key held", 90)
    steps += [num("player.IsRunning", "run key held", "running")]
    steps += [release(0), release(9), {"frames": 60}]
    steps += [setpos(x, y, GROUND_Z + 4), {"frames": 120}]
    steps += [script("TapControl 12")] + samples("jump", 90) + [{"frames": 60}]
    steps += [setpos(x, y, GROUND_Z + 300)] + samples("fall", 120)
    j = cs.job("mv_trial", "nvfdoc (Doc Mitchell's house); leaves through the front door (god mode, AI detection off).",
               steps, cs.MASTERS)
    j["title"] = "Movement trial: timing, bound keys, settings, held keys, jump, fall (about 1 min, no input)"
    return j


def diag_job():
    """Drops around the B11 tree and rock (tools/capture/jobs/diag_trees_rocks.drops.json)."""
    doc = cs.load_json(os.path.join(HERE, "diag_trees_rocks.drops.json"))
    return cm.make_job([doc], "mv_diag_trees_rocks", START, leave_door=DOC_DOOR, console_first=["tgm", "tdetect"])


def lab_job(name, files):
    """A scenario capture: the candidate's own inputs (capture_moves.py)."""
    docs = [cs.load_json(os.path.join(LAB_INPUTS, f)) for f in files]
    return cm.make_job(docs, name, START, leave_door=DOC_DOOR, console_first=["tgm", "tdetect"])


BOTTLE = "0x0005B6CE"           # WhiskeyBottle01Empty01 (MISC; collision layer Clutter, dynamic)
# The clutter row of B11: targets of the player row (parity-lab/inputs/layer4/b11_goodsprings.drops.json).
CLUTTER_TARGETS = ["collision_marker_b", "static_mailbox", "havok_tree_a", "props_crate", "terrain_rock_a",
                   "noncollidable_agave", "noncollidable_maize", "noncollidable_maize_group",
                   "transparent_small_yucca", "speedtree_shrub", "control_open_ground"]


def created_bottle(decls_extra=()):
    """Declarations and lines that find the bottle created by the job (mod index 255, not
    disabled) in the loaded cells, in ref variable rB."""
    n = cs.Names(cs.MASTERS, None)
    base = n.operand(BOTTLE)
    decls = n.decls + ["ref rB", "ref rScan", "ref rScanBase"] + list(decls_extra)
    lines = n.setup + ["let rScan := GetFirstRef 201 2 0", "while rScan != 0", "  let rScanBase := rScan.GetBaseObject",
                       f"  if rScanBase == {base}", "    if rScan.GetSourceModIndex == 255", "      if rScan.GetDisabled == 0",
                       "        let rB := rScan", "      endif", "    endif", "  endif", "  let rScan := GetNextRef", "loop"]
    return decls, lines


def clutter():
    """B11, clutter row: a whiskey bottle (Clutter layer) placed with PlaceAtMe and moved
    200 units above each target with SetPos; where does it come to rest. Exploratory: the
    behaviour of SetPos on a fresh dynamic object is itself unobserved."""
    drops = {d["label"]: d for d in cs.load_json(os.path.join(LAB_INPUTS, "b11_goodsprings.drops.json"))["drops"]}
    x, y = RUN_START
    steps = list(PROTECT) + leave_doc_house() + [setpos(x, y, GROUND_Z + 4), {"frames": 120}]
    n = cs.Names(cs.MASTERS, None)
    base = n.operand(BOTTLE)
    for label in CLUTTER_TARGETS:
        bx, by, bz = drops[label]["at"]
        steps.append({"script": cs.function(n.decls + ["ref rR"], n.setup + [
            f"let rR := player.PlaceAtMe {base} 1", f"rR.SetPos X {cs.number(bx)}", f"rR.SetPos Y {cs.number(by)}",
            f"rR.SetPos Z {cs.number(bz)}"], returns_value=False)})
        steps.append({"frames": 300})
        decls, lines = created_bottle(["float fX", "float fY", "float fZ"])
        steps.append(cs.query(cs.function(decls, lines + [
            "if rB", "  let fX := rB.GetPos X", "  let fY := rB.GetPos Y", "  let fZ := rB.GetPos Z", "endif",
            'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ))']), f"bottle {label}", "pos"))
        steps.append({"script": cs.function(decls, lines + ["if rB", "  rB.Disable", "  rB.MarkForDelete", "endif"],
                                             returns_value=False)})
        steps.append({"frames": 10})
    j = cs.job("mv_b11_clutter", START, steps, cs.MASTERS)
    j["title"] = "B11 clutter row: a bottle dropped on each layer (about 1 min, no input)"
    return j


FENCE_FACE_X = -72457.44  # the yard fence of SCOL 0x00177CA4 at y 4100: face (normal 1.6 degrees off east)
FENCE_TOP_Z = 8422.4      # and its top [observed: fnvtool phys cell --ray, our collision]
CROSSHAIR = cs.function(["ref rC"], ["let rC := GetCrosshairRef", "if rC", "  SetFunctionValue (GetFormIDString rC)",
                                     "else", '  SetFunctionValue "0"', "endif"])


def values():
    """Controller values readable without someone at the controls: the eye height (the
    pitch at which the crosshair leaves the yard fence's top edge, from two distances, which
    also gives the camera's forward offset) and the activation reach (the distance at which
    the crosshair stops finding the fence face at a fixed pitch). GetCrosshairRef returns
    a reference within activation distance [documented: GECK wiki GetCrosshairRef]."""
    y = RUN_START[1]
    steps = list(PROTECT) + leave_doc_house()
    for name in ("fMoveBaseSpeed", "fMoveRunMult", "fJumpHeightMin", "iActivatePickLength"):
        steps.append(num(f'GetNumericGameSetting "{name}"', f"setting {name}", "value"))
    for d in (40, 80):
        steps += [setpos(FENCE_FACE_X + d, y, GROUND_Z + 4), script("player.SetAngle Z 270"), {"frames": 120},
                  num("player.GetPos X", f"eye d{d}", "x"), num("player.GetPos Z", f"eye d{d}", "z")]
        for p in range(0, 71):
            steps += [script(f"player.SetAngle X {p}"), {"frames": 3}, cs.query(CROSSHAIR, f"eye d{d} pitch {p:02d}", "crosshair")]
    steps += [script("player.SetAngle X 30")]
    for d in range(90, 183, 3):
        steps += [setpos(FENCE_FACE_X + d, y, GROUND_Z + 4), {"frames": 30},
                  num("player.GetPos X", f"reach d{d:03d}", "x"), cs.query(CROSSHAIR, f"reach d{d:03d}", "crosshair")]
    steps += [script("player.SetAngle X 0")]
    # Speeds in other directions and sneaking (heading north from the open start; the
    # keys are those bound to the controls: 0 forward, 1 back, 3 right), and a long fall.
    x, yy = RUN_START
    for label, controls, sneak in (("back", [1], False), ("right", [3], False), ("forward right", [0, 3], False),
                                   ("sneak forward", [0], True)):
        steps += [setpos(x, yy + 300, GROUND_Z + 4), script("player.SetAngle Z 0"), {"frames": 120}]
        if sneak:
            steps += [script("TapControl 8"), {"frames": 60}, num("player.IsSneaking", f"{label}", "sneaking")]
        steps += [remember_t0()] + samples(f"{label}", 3)
        steps += [hold(c) for c in controls] + samples(f"{label} held", 90) + [release(c) for c in controls]
        if sneak:
            steps += [script("TapControl 8"), {"frames": 60}, num("player.IsSneaking", f"{label} after", "sneaking")]
        steps += [{"frames": 30}]
    steps += [setpos(x, yy, GROUND_Z + 2500), remember_t0()] + samples("long fall", 240)
    j = cs.job("mv_values", START, steps, cs.MASTERS)
    j["title"] = "Controller values: eye height, reach, speeds in other directions, a long fall (about 3 min, no input)"
    return j


def air():
    """Calibration (no scenario): horizontal motion in the air. The running-jump capture
    showed the speed rising from 102 to 340 units/s over about 0.2 s after a jump started
    together with the run, and dips after landing (physics.md §6.3); these runs separate
    a key pressed in mid-air, a jump out of a run already at speed, a key released in
    mid-air, and a walking jump."""
    x, y = RUN_START
    steps = list(PROTECT) + leave_doc_house()

    def start(label):
        return [setpos(x, y, GROUND_Z + 4), script("player.SetAngle Z 0"), {"frames": 120}, remember_t0()] + samples(f"{label} pre", 5)

    steps += start("air late forward") + [script("TapControl 12")] + samples("air late forward up", 8)
    steps += [hold(0)] + samples("air late forward held", 80) + [release(0), {"frames": 30}]
    steps += start("run then jump") + [hold(0)] + samples("run then jump run", 25) + [script("TapControl 12")]
    steps += samples("run then jump air", 90) + [release(0), {"frames": 30}]
    steps += start("run jump release") + [hold(0)] + samples("run jump release run", 20) + [script("TapControl 12")]
    steps += samples("run jump release air", 8) + [release(0)] + samples("run jump release after", 70) + [{"frames": 30}]
    steps += start("walk jump") + [hold(9), hold(0)] + samples("walk jump walk", 20) + [script("TapControl 12")]
    steps += samples("walk jump air", 70) + [release(0), release(9), {"frames": 30}]
    j = cs.job("mv_air", START, steps, cs.MASTERS)
    j["title"] = "Air control calibration: four jumps with held keys (about 1 min, no input)"
    return j


def air2():
    """Diagnostic (no scenario), after `mv_air`: which input sets the horizontal speed the
    player approaches in the air (the key held at take-off, the speed at take-off, or a
    small air control), how the speed decays with no key at other speeds and times, and
    the speed after landings with other keys or a key pressed only after the landing
    (physics.md 6.3). Settings read: the jump settings named for Oblivion's engine
    [documented: UESP CS wiki, Category:Jumping]; names unknown to this game fail
    harmlessly (the step records an error)."""
    x, y = RUN_START
    steps = list(PROTECT) + leave_doc_house()
    for name in ("fJumpMoveBase", "fJumpMoveMult", "fJumpDoubleMult", "fJumpFallTimeMin", "fJumpHeightMin"):
        steps.append(num(f'GetNumericGameSetting "{name}"', f"setting {name}", "value"))

    def start(label, z=GROUND_Z + 4):
        return [setpos(x, y, z), script("player.SetAngle Z 0"), {"frames": 120}, remember_t0()] + samples(f"{label} pre", 3)

    def tail(*controls):
        return [release(c) for c in controls] + [{"frames": 40}]

    # Run, jump, release in the air, press again: back to the run speed or to the air control.
    steps += start("repress") + [hold(0)] + samples("repress run", 20) + [script("TapControl 12")]
    steps += samples("repress air", 10) + [release(0)] + samples("repress released", 8) + [hold(0)]
    steps += samples("repress again", 60) + tail(0)
    # Run, jump, switch to Back in the air.
    steps += start("back in air") + [hold(0)] + samples("back in air run", 20) + [script("TapControl 12")]
    steps += samples("back in air air", 10) + [release(0), hold(1)] + samples("back in air back", 60) + tail(1)
    # Run, jump, add Right in the air (Forward still held).
    steps += start("right in air") + [hold(0)] + samples("right in air run", 20) + [script("TapControl 12")]
    steps += samples("right in air air", 10) + [hold(3)] + samples("right in air both", 60) + tail(0, 3)
    # Back and Jump pressed together from standing (as the running-jump scenario forward):
    # the rise towards the backward speed, then a landing with Back held.
    steps += start("back jump") + [hold(1), script("TapControl 12")] + samples("back jump air", 90) + tail(1)
    # Forward pressed one loop after Jump (before the take-off) and three loops after.
    for label, wait in (("forward after 1", 1), ("forward after 3", 3)):
        steps += start(label) + [script("TapControl 12")] + samples(f"{label} wait", wait) + [hold(0)]
        steps += samples(f"{label} held", 80) + tail(0)
    # Releases: from a walking jump, and from a running jump at the apex.
    steps += start("walk release") + [hold(9), hold(0)] + samples("walk release walk", 20) + [script("TapControl 12")]
    steps += samples("walk release air", 8) + [release(0)] + samples("walk release after", 50) + tail(9)
    steps += start("apex release") + [hold(0)] + samples("apex release run", 20) + [script("TapControl 12")]
    steps += samples("apex release air", 26) + [release(0)] + samples("apex release after", 40) + [{"frames": 40}]
    # A fall without a jump with Forward held from the start, and its landing.
    steps += start("fall forward", GROUND_Z + 150) + [hold(0)] + samples("fall forward held", 90) + tail(0)
    # A standing jump with no key; Forward pressed three loops after the landing.
    steps += start("press after landing") + [script("TapControl 12")] + samples("press after landing air", 58)
    steps += [hold(0)] + samples("press after landing held", 50) + tail(0)
    j = cs.job("mv_air2", START, steps, cs.MASTERS)
    j["title"] = "Air control diagnostics: ten jumps and a fall with held keys (about 1.5 min, no input)"
    return j


def drops_job(name, file):
    """Diagnostic drops (a `fnv.phys.drops/1` file in this folder)."""
    doc = cs.load_json(os.path.join(HERE, file))
    return cm.make_job([doc], name, START, leave_door=DOC_DOOR, console_first=["tgm", "tdetect"])


def main():
    jobs = [trial(),
            lab_job("mv_b11_drops", ["b11_goodsprings.drops.json"]),
            lab_job("mv_traces", ["move_fence_slide.walk.json", "move_fence_stop.walk.json",
                                  "move_running_jump.walk.json", "move_porch_steps.walk.json"]),
            clutter(), values(), air(),
            lab_job("mv_followup_traces", ["move_hill_run.walk.json", "move_portal_wall.walk.json"]),
            diag_job(), air2(), drops_job("mv_diag_fence_edge", "diag_fence_edge.drops.json"),
            drops_job("mv_diag_fence_low", "diag_fence_low.drops.json")]
    for j in jobs:
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
