#!/usr/bin/env python3
"""Writes the actor collision capture job (docs/spec-notes/physics.md section 6.8: how the
game keeps characters apart, and how far one moves another) into this folder. It runs
unattended (capture_run.py, save `nvfdoc`; no manual steps). No game data is stored here:
only script text, FormIDs and positions chosen with `fnvtool world refs` and
`fnvtool phys cell --ray` (the lane of `pc_push`, make_closeout_jobs.py).

  python tools/capture/jobs/make_actor_push_jobs.py

`pc_actor_push` (stays inside Doc Mitchell's house, god mode; AI detection is left on,
as in play). Two actors are run into: Doc Mitchell as the save has him, and a settler
made with PlaceAtMe. Every trial samples the player, the actor and the actor's state:
each game loop while the player runs at the actor, every second or third loop in the
longer trials, every sixth once the keys are released.

- A  the player runs and walks straight at the standing actor, forward held (does the
     actor move, how fast, where does it end, does it go back; how far apart the two are
     when they touch);
- B  the same from 14 units to the side (does the player slide round, is the actor
     pushed sideways);
- C  forward held for 4 s running and 6 s walking (does a push go on, to the wall);
- D  the same with the actor restrained (SetRestrained 1: does an actor that may not
     move give way);
- E  the player put on the very spot of the actor (how the game takes two characters
     apart, and which of them moves);
- F  the actor left to walk off by itself, then again with the player standing on the
     way it took (does a walking actor push the standing player, or go round).

Script functions used, from the GECK wiki [documented]: SetPos, GetPos, SetAngle,
GetAngle, PlaceAtMe, Disable, MarkForDelete, SetRestrained, GetRestrained, IsMoving,
IsRunning, GetCurrentAIProcedure, GetCurrentAIPackage, GetSitting, GetScale,
GetInSameCell, GetDisabled, GetDead, HoldKey / ReleaseKey with GetControl (controls: 0
forward, 9 run, i.e. walk while Always Run is on); xNVSE: GetFirstRef / GetNextRef (200:
actors), GetSourceModIndex, GetBaseObject.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import make_movement_jobs as mm  # noqa: E402

START = "nvfdoc (Doc Mitchell's house); stays inside (god mode)."

DOC = "0x00104C0F"              # DocMitchellREF (ACHR, persistent, placed in GSDocMitchellHouse)
SETTLER = "0x00104F09"          # GSSettlerCM (NPC_), made with PlaceAtMe
ACTOR_REFS = 200                # GetFirstRef form type of actors [documented: GECK wiki GetFirstRef]

# The lane of `pc_push`: flat floor at 7360 and nothing up to the ceiling for
# x 1488-1584, y 1040-1440; a wall at about y 1450 [observed: fnvtool phys cell --ray].
LANE_X, FLOOR = 1536.0, 7360.0
SOUTH_Y, NORTH_Y = 1050.0, 1420.0
RUN, WALK = [0], [9, 0]

# px py pz  ax ay az  heading  ms  moving procedure sitting
FIELDS = ["float fX", "float fY", "float fZ", "float fI", "float fJ", "float fK", "float fA", "float fT",
          "float fB", "float fC", "float fH"]


def doc():
    """Declarations, lines and the ref variable holding Doc Mitchell."""
    n = cs.Names(cs.MASTERS, None)
    v = n.operand(DOC)
    return n.decls, n.setup, v


def settler():
    """The same for the settler made by the job: the live actor reference created at run
    time (mod index 255) whose base is SETTLER, among the actors of the player's cell."""
    n = cs.Names(cs.MASTERS, None)
    b = n.operand(SETTLER)
    decls = n.decls + ["ref rH", "ref rScan", "ref rScanBase"]
    lines = n.setup + [f"let rScan := GetFirstRef {ACTOR_REFS} 0 0", "while rScan != 0", "  let rScanBase := rScan.GetBaseObject",
                       f"  if rScanBase == {b}", f"    if rScan.GetSourceModIndex == {cs.CREATED_MOD_INDEX}",
                       "      if rScan.GetDisabled == 0", "        let rH := rScan", "      endif", "    endif", "  endif",
                       "  let rScan := GetNextRef", "loop"]
    return decls, lines, "rH"


def sample(who, label):
    """One loop: the player's feet, the actor's feet and heading, real ms since %{t0},
    IsMoving, GetCurrentAIProcedure, GetSitting of the actor (zeros with no actor)."""
    decls, lines, v = who()
    return cs.query(cs.function(decls + FIELDS, lines + [
        "let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
        f"if {v}", f"  let fI := {v}.GetPos X", f"  let fJ := {v}.GetPos Y", f"  let fK := {v}.GetPos Z",
        f"  let fA := {v}.GetAngle Z", f"  let fB := {v}.IsMoving", f"  let fC := {v}.GetCurrentAIProcedure",
        f"  let fH := {v}.GetSitting", "endif",
        "let fT := (GetCurrentTime - %{t0}) * 120000",
        'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fI) + " " + ($ fJ) + " " + ($ fK) + " " + ($ fA)'
        ' + " " + ($ fT) + " " + ($ fB) + " " + ($ fC) + " " + ($ fH))']),
        label, "sample")


def samples(who, prefix, first, loops, every=1):
    """Samples over `loops` game loops from loop `first`, one every `every` loops (a
    sample takes a loop); labels carry the loop number."""
    out = []
    for i in range(first, first + loops, every):
        out.append(sample(who, f"{prefix} s{i:03d}"))
        if every > 1:
            out.append({"frames": every - 1})
    return out


def actor_num(who, expr, label, field):
    """`expr` (with `{0}` the actor) as a number; -1 with no actor."""
    decls, lines, v = who()
    return cs.query(cs.function(decls + ["float fV"], lines + ["let fV := -1", f"if {v}", f"  let fV := {expr.format(v)}", "endif",
                                                              "SetFunctionValue fV"]), label, field)


def actor_do(who, *stmts):
    decls, lines, v = who()
    return {"script": cs.function(decls, lines + [f"if {v}"] + ["  " + s.format(v) for s in stmts] + ["endif"], returns_value=False)}


def place_actor(who, x, y, heading=180):
    """The actor on the lane's floor, facing south (towards the player's start)."""
    return actor_do(who, f"{{0}}.SetPos X {cs.number(x)}", f"{{0}}.SetPos Y {cs.number(y)}", f"{{0}}.SetPos Z {cs.number(FLOOR)}",
                    f"{{0}}.SetAngle Z {cs.number(heading)}")


def state(who, label):
    """What the actor is and does, read once."""
    return [actor_num(who, e, label, f) for e, f in (
        ("{0}.GetInSameCell player", "same_cell"), ("{0}.GetDisabled", "disabled"), ("{0}.GetDead", "dead"),
        ("{0}.GetScale", "scale"), ("{0}.GetSitting", "sitting"), ("{0}.GetRestrained", "restrained"),
        ("{0}.GetCurrentAIPackage", "package_type"), ("{0}.GetCurrentAIProcedure", "procedure"),
        ("{0}.IsRunning", "running"))]


def run_at(who, label, actor_y, start_y, controls, held, side=0.0, after=180, every=1):
    """The player from `start_y`, `side` units east of the lane, forward held `held`
    loops at the actor standing at `actor_y` (sampled every `every` loops); `after` more
    loops with nothing held (sampled every 6)."""
    steps = [mm.setpos(LANE_X + side, start_y, FLOOR + 4), mm.script("player.SetAngle Z 0"), {"frames": 30},
             place_actor(who, LANE_X, actor_y), {"frames": 45}]
    steps += [mm.remember_t0()] + samples(who, label, 0, 3)
    steps += [mm.hold(c) for c in controls]
    steps += samples(who, label, 3, held, every)
    steps += [mm.release(c) for c in reversed(controls)]
    steps += samples(who, label, 3 + held, after, 6)
    return steps + [{"frames": 30}]


def overlap(who, label):
    """The player put where the actor stands; both sampled for 4 s, nothing held."""
    steps = [mm.setpos(LANE_X, SOUTH_Y, FLOOR + 4), mm.script("player.SetAngle Z 0"), {"frames": 30},
             place_actor(who, LANE_X, 1250.0), {"frames": 45}]
    steps += [mm.remember_t0()] + samples(who, label, 0, 3)
    steps += [mm.setpos(LANE_X, 1250.0, FLOOR)] + samples(who, label, 3, 60) + samples(who, label, 63, 180, 6)
    return steps + [{"frames": 30}]


def walk_off(who, label):
    """The actor left alone for 6 s from the lane's south end (its own walk, if it has
    somewhere to go), then again with the player standing where the actor was 1.5 s into
    that walk (remembered), facing it."""
    decls, lines, v = who()
    px = cs.function(decls + ["float fV"], lines + [f"let fV := {v}.GetPos X", "SetFunctionValue fV"])
    py = px.replace("GetPos X", "GetPos Y")
    steps = [mm.setpos(LANE_X + 40, NORTH_Y, FLOOR + 4), {"frames": 30}, place_actor(who, LANE_X, 1100.0, heading=0), {"frames": 5}]
    steps += [mm.remember_t0()] + samples(who, f"{label} free", 0, 90, 3)
    steps += [{"remember": px, "as": "bx"}, {"remember": py, "as": "by"}] + samples(who, f"{label} free", 92, 268, 4)
    # Again, the player on the spot.
    steps += [place_actor(who, LANE_X, 1100.0, heading=0),
              mm.script("let fI := %{bx}", "let fJ := %{by}", "player.SetPos X fI", "player.SetPos Y fJ",
                        f"player.SetPos Z {cs.number(FLOOR + 4)}", "player.SetAngle Z 180", decls=["float fI", "float fJ"]),
              {"frames": 5}]
    steps += [mm.remember_t0()] + samples(who, f"{label} blocked", 0, 360, 2)
    return steps + [{"frames": 30}]


def subject(who, name):
    steps = state(who, f"{name} as found")
    steps += [mm.remember_t0()] + samples(who, f"{name} as found", 0, 3)
    # A: straight at it. Running covers 150 units in about 0.45 s, walking in 1.7 s.
    # The second of each is followed for 10 s (does the actor go back to where it was).
    for label, controls, held, every, after in (("A run 1", RUN, 60, 1, 180), ("A run 2", RUN, 60, 1, 600),
                                                ("A walk 1", WALK, 240, 2, 180), ("A walk 2", WALK, 240, 2, 600)):
        steps += run_at(who, f"{name} {label}", 1250.0, 1100.0, controls, held, after=after, every=every)
    # B: from the side.
    for label, controls, held, every in (("B run side", RUN, 60, 1), ("B walk side", WALK, 240, 2)):
        steps += run_at(who, f"{name} {label}", 1250.0, 1100.0, controls, held, side=14.0, every=every)
    # C: held on, the actor near the south end so the wall is 280 units behind it.
    for label, controls, held, every in (("C run long", RUN, 240, 2), ("C walk long", WALK, 360, 3)):
        steps += run_at(who, f"{name} {label}", 1160.0, SOUTH_Y, controls, held, every=every)
    # D: restrained.
    steps += [actor_do(who, "{0}.SetRestrained 1"), {"frames": 30}] + state(who, f"{name} restrained")
    for label, controls, held, every in (("D run restrained", RUN, 60, 1), ("D walk restrained", WALK, 240, 2)):
        steps += run_at(who, f"{name} {label}", 1250.0, 1100.0, controls, held, every=every)
    steps += [actor_do(who, "{0}.SetRestrained 0"), {"frames": 30}]
    # E: on the same spot.
    steps += overlap(who, f"{name} E overlap 1") + overlap(who, f"{name} E overlap 2")
    # F: the actor's own walk, free and with the player on its way.
    steps += walk_off(who, f"{name} F")
    return steps


def actor_push():
    steps = [{"console": "tgm"}, mm.setpos(LANE_X, SOUTH_Y, FLOOR + 4), mm.script("player.SetAngle Z 0"), {"frames": 120}]
    steps += [mm.num("player.GetScale", "player", "scale"), mm.num("player.IsRunning", "player", "running"),
              mm.num("player.GetPos Z", "player", "z")]
    for name in ("fMoveBaseSpeed", "fMoveRunMult"):
        steps.append(mm.num(f'GetNumericGameSetting "{name}"', f"setting {name}", "value"))
    # The player alone: the same run and walk with no actor in the lane (speeds to compare).
    steps += [mm.remember_t0()] + mm.samples("alone run pre", 3) + [mm.hold(0)] + mm.samples("alone run", 60) + [mm.release(0), {"frames": 60}]
    steps += [mm.setpos(LANE_X, SOUTH_Y, FLOOR + 4), mm.script("player.SetAngle Z 0"), {"frames": 60}]
    steps += [mm.remember_t0()] + mm.samples("alone walk pre", 3) + [mm.hold(9), mm.hold(0)] + mm.samples("alone walk", 240)
    steps += [mm.release(0), mm.release(9), {"frames": 60}]
    steps += subject(doc, "doc")
    # Doc Mitchell out of the lane, then the settler.
    steps += [actor_do(doc, f"{{0}}.SetPos X {cs.number(2288)}", f"{{0}}.SetPos Y {cs.number(2244)}", f"{{0}}.SetPos Z {cs.number(FLOOR)}"),
              mm.setpos(LANE_X, SOUTH_Y, FLOOR + 4), {"frames": 60}]
    n = cs.Names(cs.MASTERS, None)
    b = n.operand(SETTLER)
    steps += [{"script": cs.function(n.decls + ["ref rR"], n.setup + [
        f"let rR := player.PlaceAtMe {b} 1", f"rR.SetPos X {cs.number(LANE_X)}", "rR.SetPos Y 1250", f"rR.SetPos Z {cs.number(FLOOR)}"],
        returns_value=False)}, {"frames": 180}]
    steps += subject(settler, "settler")
    steps += [actor_do(settler, "{0}.Disable", "{0}.MarkForDelete"), {"frames": 30}]
    j = cs.job("pc_actor_push", START, steps, cs.MASTERS)
    j["title"] = "Actors pushing actors: the player runs and walks into Doc Mitchell and a settler, stands in their way (about 4 min, no input)"
    return j


def main():
    j = actor_push()
    path = os.path.join(HERE, j["name"] + ".steps.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(j, f, indent=1)
        f.write("\n")
    print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
