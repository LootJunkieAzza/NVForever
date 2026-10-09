#!/usr/bin/env python3
"""Writes the Layer 4 play close-out capture jobs (docs/design/layer4-play.md "Open",
docs/design/layer4.md section 5 item 9, physics.md 4.3, 6 and 6.7) into this folder.
They run unattended (capture_run.py; no manual steps). No game data is stored here: only
script text, FormIDs and positions chosen with `fnvtool phys cell --ray` /
`fnvtool world refs` (2026-10-09).

  python tools/capture/jobs/make_closeout_jobs.py

- `pc_drop_fall`  (save nvfdoc; leaves through the front door): does a dropped item
  fall? A scripted `Drop`, the same item nudged by SetPos, a `Drop` with the Pip-Boy
  open (the nearest unattended stand-in for the inventory menu), `PlaceAtMe`, a dropped
  tin can, and the player walking into a dropped stimpak (simulation.md 5 "After the
  drop").
- `pc_push` (save nvfdoc; stays inside Doc Mitchell's house): the player runs or walks
  into a tin can, a whiskey bottle or a stimpak lying on the floor, forward held; player
  and item sampled every loop (physics.md 6, push mass).
- `pc_head` (save nvfdoc; stays inside Doc Mitchell's house): standing jumps under a
  ceiling 256 above the floor (control), under a doorway beam 176 above it and under a
  sloping ceiling 162-186 above it, standing and sneaking; samples every loop give the
  apex, which a ceiling lower than the head's reach cuts short (capsule height).
- `pc_coc` (save nvftest, inside the Goodsprings gas station): where `coc` puts the
  player in four interiors (one with a COCMarkerHeading, three without), then a SetPos
  over nothing in the gas station sampled for 20 s (physics.md 6.7).

Script functions used, from the GECK wiki [documented]: Drop, PlaceAtMe, SetPos, GetPos,
GetAngle, TapControl (control codes in the order of the game's control map: 8 sneak,
12 jump, 14 menu mode, i.e. the Pip-Boy), HoldKey / ReleaseKey with GetControl,
IsSneaking, MenuMode, GetParentCell, Disable, MarkForDelete; xNVSE: GetFirstRef /
GetNextRef, GetSourceModIndex, GetBaseObject, GetFormIDString.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import make_movement_jobs as mm  # noqa: E402

CTX = (cs.MASTERS, None)
START_DOC = "nvfdoc (Doc Mitchell's house); leaves through the front door (god mode, AI detection off)."
START_DOC_INSIDE = "nvfdoc (Doc Mitchell's house); stays inside (god mode, AI detection off)."
START_GAS = "nvftest (inside the Goodsprings gas station); travels with coc (god mode, AI detection off)."

STIMPAK = "0x00015169"
TIN_CAN = "0x00022104"          # TinCan01 (MISC, Clutter\Junk\TinCan01.NIF; collision layer Clutter, dynamic)
BOTTLE = mm.BOTTLE              # WhiskeyBottle01Empty01

JUMP, SNEAK, MENU = 12, 8, 14   # TapControl codes [documented: GECK wiki TapControl; physics.md 6.3]


def created(base, decls_extra=()):
    """Declarations and lines that put in rH the live (not disabled) reference created
    at run time (mod index 255) whose base is `base`, among the item references of the
    loaded cells (the last one the scan meets; jobs keep one at a time)."""
    n = cs.Names(cs.MASTERS, None)
    b = n.operand(base)
    decls = n.decls + ["ref rH", "ref rScan", "ref rScanBase"] + list(decls_extra)
    lines = n.setup + ["let rScan := GetFirstRef 201 2 0", "while rScan != 0", "  let rScanBase := rScan.GetBaseObject",
                       f"  if rScanBase == {b}", f"    if rScan.GetSourceModIndex == {cs.CREATED_MOD_INDEX}",
                       "      if rScan.GetDisabled == 0", "        let rH := rScan", "      endif", "    endif", "  endif",
                       "  let rScan := GetNextRef", "loop"]
    return decls, lines


POSE = ["float fX", "float fY", "float fZ", "float fA", "float fB", "float fC"]


def item_pose(base, label):
    """'x y z ax ay az' of the created item (degrees), or '0 0 0 0 0 0' with none."""
    decls, lines = created(base, POSE)
    return cs.query(cs.function(decls, lines + [
        "if rH", "  let fX := rH.GetPos X", "  let fY := rH.GetPos Y", "  let fZ := rH.GetPos Z",
        "  let fA := rH.GetAngle X", "  let fB := rH.GetAngle Y", "  let fC := rH.GetAngle Z", "endif",
        'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fA) + " " + ($ fB) + " " + ($ fC))']),
        label, "pose")


def item_do(base, *stmts, decls_extra=()):
    decls, lines = created(base, decls_extra)
    return {"script": cs.function(decls, lines + ["if rH"] + ["  " + s for s in stmts] + ["endif"], returns_value=False)}


def remove_item(base):
    return [item_do(base, "rH.Disable", "rH.MarkForDelete"), {"frames": 10}]


def player_pose(label):
    return [mm.num(f"player.GetPos {a}", label, f"pos.{a.lower()}") for a in "XYZ"] + [
        mm.num("player.GetAngle Z", label, "angle_z")]


def reads(base, prefix, at_frames):
    """The item's pose at these loop counts after now."""
    out, done = [], 0
    for f in at_frames:
        out += [{"frames": f - done}, item_pose(base, f"{prefix} t{f:03d}")]
        done = f
    return out


def act(*stmts, names=()):
    return [{"script": cs.run_script(list(stmts), CTX, list(names))}, {"frames": cs.SETTLE_FRAMES}]


def nudge(base, prefix, dz):
    decls_extra = ["float fZ"]
    return [item_do(base, "let fZ := rH.GetPos Z", f"let fZ := fZ + {cs.number(dz)}", "rH.SetPos Z fZ", decls_extra=decls_extra)]


def drop_fall():
    steps = list(mm.PROTECT) + mm.leave_doc_house()
    steps += player_pose("outside")
    steps += act("{0}.AddItem {1} 5 1", "{0}.AddItem {2} 2 1", names=["player", STIMPAK, TIN_CAN])
    # A: a scripted Drop, read over 10 s; B: the same item nudged up 1 unit, then 40.
    steps += act("player.Drop {0} 1", names=[STIMPAK])
    steps += reads(STIMPAK, "A drop", [5, 60, 180, 600])
    steps += nudge(STIMPAK, "B", 1) + reads(STIMPAK, "B nudge1", [5, 60, 180])
    steps += nudge(STIMPAK, "B", 40) + reads(STIMPAK, "B nudge40", [5, 60, 180])
    steps += remove_item(STIMPAK)
    # C: Drop with the Pip-Boy open, then the menus closed.
    steps += [mm.script(f"TapControl {MENU}"), {"frames": 90}, mm.num("MenuMode", "C pipboy open", "menumode")]
    steps += act("player.Drop {0} 1", names=[STIMPAK])
    steps += reads(STIMPAK, "C in menu", [30, 120])
    steps += [{"console": "CloseAllMenus"}, {"frames": 5}, mm.num("MenuMode", "C closed", "menumode")]
    steps += reads(STIMPAK, "C after close", [5, 60, 180, 600])
    steps += remove_item(STIMPAK)
    # D: PlaceAtMe (a reference created at the player, no Drop).
    steps += act("player.PlaceAtMe {0} 1", names=[STIMPAK])
    steps += reads(STIMPAK, "D placeatme", [5, 60, 180])
    steps += remove_item(STIMPAK)
    # E: a dropped tin can, and the same nudged.
    steps += act("player.Drop {0} 1", names=[TIN_CAN])
    steps += reads(TIN_CAN, "E drop can", [5, 60, 180, 600])
    steps += nudge(TIN_CAN, "E", 1) + reads(TIN_CAN, "E nudge1", [5, 60, 180])
    steps += remove_item(TIN_CAN)
    # F: the player walks into a dropped stimpak (forward held 40 loops).
    steps += act("player.Drop {0} 1", names=[STIMPAK])
    steps += reads(STIMPAK, "F drop", [120])
    steps += player_pose("F before walk")
    steps += [mm.hold(0), {"frames": 40}, mm.release(0)]
    steps += reads(STIMPAK, "F walked", [5, 120, 300])
    steps += player_pose("F after walk")
    steps += remove_item(STIMPAK)
    j = cs.job("pc_drop_fall", START_DOC, steps, cs.MASTERS)
    j["title"] = "Do dropped items fall: scripted drops, a nudge, a drop in the Pip-Boy, PlaceAtMe, a tin can (about 2 min, no input)"
    return j


def push_sample(base, label):
    """'px py pz ix iy iz t' each loop: the player's feet, the item, real ms since %{t0}."""
    decls, lines = created(base, ["float fX", "float fY", "float fZ", "float fI", "float fJ", "float fK", "float fT"])
    return cs.query(cs.function(decls, lines + [
        "let fX := player.GetPos X", "let fY := player.GetPos Y", "let fZ := player.GetPos Z",
        "if rH", "  let fI := rH.GetPos X", "  let fJ := rH.GetPos Y", "  let fK := rH.GetPos Z", "endif",
        "let fT := (GetCurrentTime - %{t0}) * 120000",
        'SetFunctionValue (($ fX) + " " + ($ fY) + " " + ($ fZ) + " " + ($ fI) + " " + ($ fJ) + " " + ($ fK) + " " + ($ fT))']),
        label, "sample")


# Trials: (label, item, controls held, start distance south of the item, loops held).
PUSH_TRIALS = [
    ("can run 1", TIN_CAN, [0], 150, 40), ("can run 2", TIN_CAN, [0], 150, 40), ("can run 3", TIN_CAN, [0], 150, 40),
    ("can walk 1", TIN_CAN, [9, 0], 100, 150), ("can walk 2", TIN_CAN, [9, 0], 100, 150),
    ("bottle run 1", BOTTLE, [0], 150, 40), ("bottle run 2", BOTTLE, [0], 150, 40),
    ("stimpak run 1", STIMPAK, [0], 150, 40),
]
# A lane in Doc Mitchell's house: flat floor at 7360 and nothing up to the ceiling for
# x 1488-1584, y 1040-1440 [observed: fnvtool phys cell --ray, 182 vertical rays]; the
# wall is at about y 1450. Inside, so no load door is used: leaving through the front
# door made the game write autosave.fos (pc_drop_fall, 2026-10-09).
LANE_X, ITEM_Y = 1536.0, 1250.0


def push():
    x, z = LANE_X, 7360.0
    steps = list(mm.PROTECT) + [mm.setpos(x, 1060.0, z + 4), {"frames": 120}]
    n = cs.Names(cs.MASTERS, None)
    for label, item, controls, dist, held in PUSH_TRIALS:
        b = n.operand(item)
        # The item 20 units above the floor straight north of the start; it falls and settles.
        steps += [{"script": cs.function(n.decls + ["ref rR"], n.setup + [
            f"let rR := player.PlaceAtMe {b} 1", f"rR.SetPos X {cs.number(x)}", f"rR.SetPos Y {cs.number(ITEM_Y)}",
            f"rR.SetPos Z {cs.number(z + 20)}"], returns_value=False)}, {"frames": 240}]
        steps += [item_pose(item, f"{label} settled")]
        steps += [mm.setpos(x, ITEM_Y - dist, z + 4), mm.script("player.SetAngle Z 0"), {"frames": 120}]
        steps += [item_pose(item, f"{label} before")]
        steps += [mm.remember_t0()] + [push_sample(item, f"{label} s{i:03d}") for i in range(3)]
        steps += [mm.hold(c) for c in controls]
        steps += [push_sample(item, f"{label} s{i:03d}") for i in range(3, 3 + held)]
        steps += [mm.release(c) for c in reversed(controls)]
        steps += [push_sample(item, f"{label} s{i:03d}") for i in range(3 + held, 3 + held + 120)]
        steps += [{"frames": 120}, item_pose(item, f"{label} after")]
        steps += remove_item(item)
        steps += [mm.setpos(x, 1060.0, z + 4), {"frames": 30}]
    j = cs.job("pc_push", START_DOC_INSIDE, steps, cs.MASTERS)
    j["title"] = "Push: the player runs or walks into a tin can, a bottle and a stimpak in Doc Mitchell's house, 8 trials (about 2 min, no input)"
    return j


FLOOR = 7360.0
# Spots in Doc Mitchell's house (floor 7360), ceilings from `fnvtool phys cell --cell
# GSDocMitchellHouse --ray` straight up [observed, our collision]: label, x, y, ceiling z.
HEAD_SPOTS = [
    ("open", 1536.0, 1340.0, 7616.0),       # room ceiling 0x00103E2E, 256 above the floor
    ("beam", 1536.0, 1468.0, 7536.1),       # doorway top in the wall 0x00103E2E / 0x00103E2A, 176.1
    ("slope174", 1864.0, 2116.0, 7534.2),   # sloping ceiling 0x00103E7C, falls 5.8 per 16 units east
    ("slope168", 1880.0, 2116.0, 7528.4),
    ("slope163", 1896.0, 2116.0, 7522.6),
]


def head():
    steps = list(mm.PROTECT)

    def jump(label, x, y, sneak=False):
        s = [mm.setpos(x, y, FLOOR + 4), mm.script("player.SetAngle Z 0"), {"frames": 120}]
        if sneak:
            s += [mm.script(f"TapControl {SNEAK}"), {"frames": 90}, mm.num("player.IsSneaking", label, "sneaking")]
        s += [mm.num("player.GetPos Z", label, "z")]
        s += [mm.remember_t0()] + mm.samples(f"{label} pre", 3)
        s += [mm.script(f"TapControl {JUMP}")] + mm.samples(f"{label} jump", 75)
        if sneak:
            s += [mm.num("player.IsSneaking", f"{label} after", "sneaking"), mm.script(f"TapControl {SNEAK}"), {"frames": 90},
                  mm.num("player.IsSneaking", f"{label} stood", "sneaking")]
        return s + [{"frames": 30}]

    for r in (1, 2):
        for label, x, y, _ in HEAD_SPOTS:
            steps += jump(f"{label} r{r}", x, y)
    for label, x, y, _ in HEAD_SPOTS[:2]:
        steps += jump(f"{label} sneak", x, y, sneak=True)
    # Walking through the doorway under the beam, standing and sneaking.
    for label, sneak in (("walk beam", False), ("walk beam sneak", True)):
        steps += [mm.setpos(1536.0, 1380.0, FLOOR + 4), mm.script("player.SetAngle Z 0"), {"frames": 120}]
        if sneak:
            steps += [mm.script(f"TapControl {SNEAK}"), {"frames": 90}]
        steps += [mm.remember_t0()] + mm.samples(f"{label} pre", 3) + [mm.hold(9), mm.hold(0)] + mm.samples(f"{label} held", 90)
        steps += [mm.release(0), mm.release(9), {"frames": 30}]
        if sneak:
            steps += [mm.script(f"TapControl {SNEAK}"), {"frames": 90}]
    j = cs.job("pc_head", START_DOC_INSIDE, steps, cs.MASTERS)
    j["title"] = "Capsule height: standing jumps under ceilings 256, 176 and 163-174 above the floor, standing and sneaking (about 1.5 min, no input)"
    return j


COC_CELLS = [("GSDocMitchellHouse", "0x00103DF9"), ("GSGasStation", "0x00109038"),
             ("GSProspectorSaloonInterior", "0x00106185"), ("GSGeneralStore", "0x001057F1")]
GAS_VOID = (682.0, -4755.0, 7552.0)    # beside Ringo's cash box, outside the room (physics.md 6.7)


def coc():
    steps = list(mm.PROTECT)
    steps += [cs.query(cs.cell_query("player", CTX), "start", "cell")] + player_pose("start")
    for edid, fid in COC_CELLS:
        steps += [{"console": f"coc {edid}"}, {"until": cs.in_cell_until(fid, CTX), "max_frames": cs.DOOR_LOOPS},
                  {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": 10}]
        steps += [cs.query(cs.cell_query("player", CTX), f"coc {edid}", "cell")] + player_pose(f"coc {edid}")
        steps += [{"frames": 120}] + player_pose(f"coc {edid} 2s")
    # Over nothing in the gas station (the last coc above went elsewhere: back first).
    edid, fid = COC_CELLS[1]
    steps += [{"console": f"coc {edid}"}, {"until": cs.in_cell_until(fid, CTX), "max_frames": cs.DOOR_LOOPS},
              {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": 10}]
    steps += [mm.setpos(*GAS_VOID), mm.remember_t0()] + mm.samples("void", 3)
    for i in range(40):
        steps += [{"frames": 30}, cs.query(mm.SAMPLE, f"void every30 {i:03d}", "sample")]
    steps += [cs.query(cs.cell_query("player", CTX), "void end", "cell")]
    j = cs.job("pc_coc", START_GAS, steps, cs.MASTERS)
    j["title"] = "coc into four interiors, then a SetPos over nothing in the gas station (about 1.5 min, no input)"
    return j


def main():
    for j in (drop_fall(), push(), head(), coc()):
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
