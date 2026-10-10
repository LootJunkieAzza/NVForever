#!/usr/bin/env python3
"""Writes the player-controls capture job (docs/spec-notes/controls.md section 5.4: what
DisablePlayerControls and EnablePlayerControls do to the player in the game) into this
folder. It runs unattended (capture_run.py, save `nvfdoc`). No game data is stored here:
only script text, a FormID and the open ground of the movement jobs.

  python tools/capture/jobs/make_controls_jobs.py

Script functions used, from the GECK wiki [documented]: DisablePlayerControls,
EnablePlayerControls and GetPlayerControlsDisabled (seven optional 0/1 parameters:
movement, Pip-Boy, fighting, POV, looking, rollover text, sneaking); TapControl and
GetControl with the control codes 0 forward, 5 activate, 7 ready item, 8 crouch,
11 auto move, 12 jump, 13 toggle POV, 14 menu mode; HoldKey / ReleaseKey; IsPC1stPerson,
IsSneaking, IsWeaponOut, MenuMode, GetCrosshairRef, GetPos, GetAngle, SetPos, SetAngle;
the console's CloseAllMenus.

The pictures are taken with the HUD on (no `tm`): they show which HUD parts and which
rollover text are drawn while a group is off. The object looked at is the door the job
leaves Doc Mitchell's house by: the player is put back where the door let them out,
facing as they arrived and turned round (which of the two faces the door is read from
GetCrosshairRef).
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import make_movement_jobs as mv  # noqa: E402

# The parameter order of the three functions.
GROUPS = ["movement", "pipboy", "fighting", "pov", "looking", "rollover", "sneaking"]
# Control codes [documented: GECK wiki TapControl].
FORWARD, ACTIVATE, READY, CROUCH, AUTO_MOVE, JUMP, TOGGLE_POV, MENU_MODE = 0, 5, 7, 8, 11, 12, 13, 14

# Game loops a tapped control gets before its effect is read (a weapon draw, a view
# change and crouching each take under a second).
TAP_FRAMES = 120
# The pose the house door leaves the player in, remembered as %{door_x} ... %{door_h}.
DOOR_POSE = [("door_x", "player.GetPos X"), ("door_y", "player.GetPos Y"), ("door_z", "player.GetPos Z"),
             ("door_h", "player.GetAngle Z")]
# Parameter lists with fewer than seven parameters, as FalloutNV.esm's scripts write them
# [observed: fnvtool script decompile], and the shortest one.
SHORT_DISABLES = ("1", "1 1", "0 1 1", "0 0 0 0 1")
SHORT_ENABLES = ("1 1 0", "0 1 1", "0 0 0 0 1")
# The two ways the player looks from there: as they arrived, and turned round.
TURNS = (("as arrived", 0), ("turned round", 180))


def flags(*on):
    """The seven parameters with the named groups set: flags("movement") = "1 0 0 0 0 0 0"."""
    unknown = set(on) - set(GROUPS)
    if unknown:
        raise ValueError(f"unknown control groups: {sorted(unknown)}")
    return " ".join("1" if g in on else "0" for g in GROUPS)


def disable(*on):
    return mv.script("DisablePlayerControls " + flags(*on))


def enable_all():
    return mv.script("EnablePlayerControls")


def tap(control, frames=TAP_FRAMES):
    return [mv.script(f"TapControl {control}"), {"frames": frames}]


def probes(label):
    """GetPlayerControlsDisabled asked one group at a time, then with no parameters."""
    steps = [mv.num("GetPlayerControlsDisabled " + flags(g), label, f"disabled.{g}") for g in GROUPS]
    return steps + [mv.num("GetPlayerControlsDisabled", label, "disabled.any")]


def state(label):
    """What the player is doing: the view, sneaking, the weapon, any menu."""
    return [mv.num("IsPC1stPerson", label, "first_person"), mv.num("player.IsSneaking", label, "sneaking"),
            mv.num("player.IsWeaponOut", label, "weapon_out"), mv.num("MenuMode", label, "menu")]


def home(pitch=0):
    """Back on the open ground, facing north."""
    x, y = mv.RUN_START
    return [mv.setpos(x, y, mv.GROUND_Z + 4), mv.script("player.SetAngle Z 0", f"player.SetAngle X {pitch}"), {"frames": 120}]


def remember_door_pose():
    return [{"remember": cs.function(["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"]), "as": name}
            for name, expr in DOOR_POSE]


def at_door(turn):
    """Back where the door let the player out, the arrival heading turned by `turn` degrees."""
    return [mv.script("let fH := %{door_h} + " + str(turn), "if fH >= 360", "  let fH := fH - 360", "endif",
                      "player.SetPos X %{door_x}", "player.SetPos Y %{door_y}", "player.SetPos Z %{door_z}",
                      "player.SetAngle Z fH", "player.SetAngle X 0", decls=["float fH"]),
            {"frames": 120}]


def where(label):
    return [mv.num("player.GetPos X", label, "x"), mv.num("player.GetPos Y", label, "y"), mv.num("player.GetPos Z", label, "z")]


def look_at_door(label):
    """Both ways from the door's arrival: the crosshair's reference (as text) and a picture
    with the HUD on."""
    steps = []
    for name, turn in TURNS:
        steps += at_door(turn) + [cs.query(mv.CROSSHAIR, f"{label}, {name}", "crosshair"),
                                  {"screenshot": f"{label} {name}".replace(" ", "_")}]
    return steps


def activate_door(label):
    """Activate tapped both ways from the door's arrival, and where the player is after
    each (through the door, or still outside)."""
    steps = []
    for name, turn in TURNS:
        steps += at_door(turn) + tap(ACTIVATE, 300) + where(f"{label}, {name}")
    return steps


def controls_job():
    steps = list(mv.PROTECT) + mv.leave_doc_house() + remember_door_pose() + home()
    steps += probes("start") + state("start")

    # The defaults of the forms without parameters [documented: 1 1 1 1 0 0 0 and all].
    steps += [mv.script("DisablePlayerControls"), {"frames": 30}] + probes("disable default")
    steps += [enable_all(), {"frames": 30}] + probes("enable default")
    # One group at a time; then all seven off and two given back (the others must stay off).
    for g in GROUPS:
        steps += [disable(g), {"frames": 5}] + probes(f"only {g}") + [enable_all(), {"frames": 5}]
    steps += [mv.script("DisablePlayerControls " + flags(*GROUPS)), {"frames": 5}]
    steps += [mv.script("EnablePlayerControls " + flags("movement", "looking")), {"frames": 5}] + probes("all then move and look back")
    steps += [enable_all(), {"frames": 30}]
    # Parameter lists shorter than seven, as shipped scripts write them: which groups do
    # the left-out parameters touch? (VCG00 stage 0 calls `DisablePlayerControls 1 1` and
    # then `0 0 0 0 1 0 0`; a new game read fighting off as well, controls.md 5.2.)
    for form in SHORT_DISABLES:
        steps += [mv.script(f"DisablePlayerControls {form}"), {"frames": 5}] + probes(f"disable {form}") + [enable_all(), {"frames": 5}]
    for form in SHORT_ENABLES:
        steps += [mv.script("DisablePlayerControls " + flags(*GROUPS)), {"frames": 5}, mv.script(f"EnablePlayerControls {form}"),
                  {"frames": 5}] + probes(f"all off then enable {form}") + [enable_all(), {"frames": 5}]
    steps += [{"frames": 30}]

    # Movement off: Forward held, Jump, Auto Move (open points 2).
    steps += home() + [disable("movement"), {"frames": 30}, mv.remember_t0()] + mv.samples("movement off still", 5)
    steps += [mv.hold(FORWARD)] + mv.samples("movement off forward", 60) + [mv.release(FORWARD), {"frames": 30}]
    steps += [mv.script(f"TapControl {JUMP}")] + mv.samples("movement off jump", 60)
    steps += [mv.script(f"TapControl {AUTO_MOVE}")] + mv.samples("movement off auto move", 60)
    steps += [enable_all()] + mv.samples("movement back", 60)
    # Auto Move, if it was switched on while movement was off, is switched off again.
    steps += [mv.hold(FORWARD), {"frames": 5}, mv.release(FORWARD), {"frames": 30}]

    # Fighting: a drawn weapon (fists when unarmed) is put away; does it come back
    # (open point 3)?
    steps += home() + tap(READY) + state("weapon drawn")
    steps += [disable("fighting"), {"frames": TAP_FRAMES}] + state("fighting off") + [{"screenshot": "fighting_off"}]
    steps += tap(READY) + state("fighting off ready tapped")
    steps += [enable_all(), {"frames": TAP_FRAMES}] + state("fighting back")
    steps += tap(READY) + state("fighting back ready tapped")
    steps += [mv.script("if player.IsWeaponOut", f"  TapControl {READY}", "endif"), {"frames": TAP_FRAMES}] + state("weapon away")

    # POV: third person is forced to first; does it come back?
    steps += tap(TOGGLE_POV) + state("third person")
    steps += [disable("pov"), {"frames": TAP_FRAMES}] + state("pov off")
    steps += tap(TOGGLE_POV) + state("pov off toggle tapped")
    steps += [enable_all(), {"frames": TAP_FRAMES}] + state("pov back")
    steps += [mv.script("if IsPC1stPerson == 0", f"  TapControl {TOGGLE_POV}", "endif"), {"frames": TAP_FRAMES}] + state("first person again")

    # Sneaking: a sneaking player is forced out of it; does it come back?
    steps += tap(CROUCH) + state("sneaking")
    steps += [disable("sneaking"), {"frames": TAP_FRAMES}] + state("sneaking off")
    steps += tap(CROUCH) + state("sneaking off crouch tapped")
    steps += [enable_all(), {"frames": TAP_FRAMES}] + state("sneaking back")
    steps += [mv.script("if player.IsSneaking", f"  TapControl {CROUCH}", "endif"), {"frames": TAP_FRAMES}] + state("standing again")

    # Pip-Boy off: Menu Mode must open nothing. CloseAllMenus is the safety net.
    steps += [disable("pipboy"), {"frames": 30}] + tap(MENU_MODE) + state("pipboy off menu tapped")
    steps += [{"console": "CloseAllMenus"}, {"frames": 30}, enable_all(), {"frames": 30}]
    steps += probes("end") + state("end")

    # The door, last (a working Activate takes the player indoors): rollover text and the
    # HUD with nothing off, movement off and rollover text off (open point 1); Activate
    # with movement off, then with everything on as the control.
    steps += look_at_door("door controls on")
    steps += [disable("movement"), {"frames": 60}] + look_at_door("door movement off")
    steps += activate_door("door activate with movement off")
    steps += [enable_all(), disable("rollover"), {"frames": 60}] + look_at_door("door rollover off")
    steps += [enable_all(), {"frames": 60}] + look_at_door("door controls back")
    steps += activate_door("door activate with controls on")

    j = cs.job("pc_controls", mv.START, steps, cs.MASTERS)
    j["title"] = "Player controls: what DisablePlayerControls stops and what comes back (about 3 min, no input)"
    return j


def main():
    j = controls_job()
    path = os.path.join(HERE, j["name"] + ".steps.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(j, f, indent=1)
        f.write("\n")
    print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
