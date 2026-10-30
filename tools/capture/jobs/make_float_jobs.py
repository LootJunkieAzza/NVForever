#!/usr/bin/env python3
"""Writes the floating capture job (docs/spec-notes/ragdoll.md section 8: what floats in
the original's water) into this folder. It runs unattended (capture_run.py, save
`nvfstrip`; the job travels to the Lake Mead shore at Boulder Beach with the console's
`coc`). No game data is stored here: only script text, FormIDs of the masters (looked up
read-only with capture_steps.edid_index, 2026-10-25) and positions from the swimming jobs
(make_swim_jobs.py).

  python tools/capture/jobs/make_float_jobs.py

What it does: the player is put at the surface over the lake, is given one of each of
fifteen items, from a pencil to a sledgehammer, and drops them; each dropped reference
is moved 150 units under the surface, over a bed 350-660 units down, spread 40 units
apart; an NCR trooper is placed, moved there too and killed. Then every reference's
height is read at intervals for 30 s: a rising height floats, a falling one sinks.

Script functions used, from the GECK wiki [documented]: AddItem, Drop, PlaceAtMe,
GetFirstRef / GetNextRef (form type 200 actors, 201 inventory items; depth 1 takes the
cells around the player's too), GetBaseObject, GetSourceModIndex, SetPos / GetPos,
KillActor, GetDead, GetCurrentTime.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402

CTX = (cs.MASTERS, None)
CELL = "BoulderBeachCampground"          # WastelandNV (13, 9), Lake Mead shore
WATER_Z = 2600.0                         # CELL XCLW of the shore cells [observed: masters]
# The bed falls east from the shore at (56992, 37536): 326 under the surface 1,536 units
# out [observed: LAND, make_swim_jobs.py]. The items go 150 under at that x.
ITEM_X = 58528.0
ITEM_Y0 = 37200.0
ITEM_DY = 40.0
ITEM_Z = WATER_Z - 150.0
# The player waits at the surface west of them, out of their way.
PLAYER = (58000.0, 37000.0, WATER_Z)
# Items farther than this from the player are not the job's own.
NEAR = 1500
# The trooper goes north of the items, at their depth.
ACTOR = (ITEM_X, 37950.0, ITEM_Z)

# (label, FormID): MISC, ALCH and WEAP items from light to heavy [observed: masters].
ITEMS = [
    ("pencil", "0x0005B63C"),            # Pencil01
    ("tin_can", "0x00022104"),           # TinCan01
    ("teddy_bear", "0x0001F21F"),        # TeddyBear01
    ("cigarette_carton", "0x00034068"),  # CigaretteCarton
    ("prewar_book", "0x000943F2"),       # PreWarBook
    ("wonderglue", "0x0002210A"),        # WonderGlue
    ("sensor_module", "0x00022109"),     # SensorModule
    ("empty_whiskey_bottle", "0x0005B6CE"),  # WhiskeyBottle01Empty01
    ("whiskey", "0x00032C75"),           # Whiskey (ALCH, full bottle)
    ("nuka_cola", "0x0001519E"),         # NukaCola (ALCH)
    ("hot_plate", "0x0005516B"),         # HotPlate
    ("toaster", "0x0009A808"),           # ToasterPrewar
    ("baseball_bat", "0x0000421C"),      # WeapBaseballBat (wood)
    ("lead_pipe", "0x00004337"),         # WeapLeadPipe
    ("sledgehammer", "0x00004351"),      # WeapSledgehammer
]
TROOPER = "0x001543E0"                   # LvlNCRTrooper (NPC_)

# Frames between the height readings (60 a second): 0, 0.5, 1, 2, 4, 8, 12, 20, 30 s.
GAPS = [0, 30, 30, 60, 120, 240, 240, 480, 600]
PROTECT = [{"console": "tgm"}, {"console": "tdetect"}]


def script(lines, decls=()):
    return {"script": cs.function(list(decls), list(lines), returns_value=False)}


def num(expr, label, field, decls=(), setup=()):
    return cs.query(cs.function(list(decls) + ["float fV"], list(setup) + [f"let fV := {expr}", "SetFunctionValue fV"]),
                    label, field)


def find_item(item):
    """Declarations and lines that leave rHit holding the created (mod index 255) reference
    of `item` within NEAR units of the player, in the player's cell or the cells around it.
    Without the distance the scan found weapons with the same base that the save already
    held 3,800 units away (fl_heavy_lake's first run, 2026-10-25)."""
    n = cs.Names(*CTX)
    base = n.operand(item)
    body = n.setup + [f"let rScan := GetFirstRef {cs.INVENTORY_ITEM_REFS} 1 0", "while rScan != 0",
                      "  let rScanBase := rScan.GetBaseObject", f"  if rScanBase == {base}",
                      f"    if rScan.GetSourceModIndex == {cs.CREATED_MOD_INDEX}",
                      f"      if rScan.GetDistance player < {NEAR}", "        let rHit := rScan", "      endif",
                      "    endif", "  endif", "  let rScan := GetNextRef", "loop"]
    return n.decls + ["ref rScan", "ref rScanBase", "ref rHit"], body


def find_actor():
    """rHit: the created actor (mod index 255) within NEAR units of the player, in the
    player's cell or around it."""
    body = ["let rScan := GetFirstRef 200 1 0", "while rScan != 0",
            f"  if rScan.GetSourceModIndex == {cs.CREATED_MOD_INDEX}", f"    if rScan.GetDistance player < {NEAR}",
            "      let rHit := rScan", "    endif", "  endif", "  let rScan := GetNextRef", "loop"]
    return ["ref rScan", "ref rHit"], body


def setpos_hit(find, x, y, z):
    decls, body = find
    return script(body + [f"rHit.SetPos X {cs.number(x)}", f"rHit.SetPos Y {cs.number(y)}",
                          f"rHit.SetPos Z {cs.number(z)}"], decls)


def go_there():
    """coc to the beach cell, wait until the player stands in the shore cells' x range, and
    close the menu that opens there (make_swim_jobs.py go_there)."""
    until = cs.function(["float fV"], ["if player.GetPos X > 53248", "  if player.GetPos X < 61440", "    let fV := 1",
                                       "  endif", "endif", "SetFunctionValue fV"])
    return [{"console": f"coc {CELL}"}, {"until": until, "max_frames": cs.DOOR_LOOPS}, {"frames": cs.AFTER_DOOR_FRAMES},
            {"console": "CloseAllMenus"}, {"frames": 60}, num("MenuMode", "after travel", "menumode")]


def readings(label, items, axes="Z"):
    out = [num(f"(GetCurrentTime - %{{t0}}) * 120000", label, "t_ms")]
    for name, item in items:
        decls, body = find_item(item)
        out += [num(f"rHit.GetPos {a}", label, f"{name}.{a.lower()}", decls, body) for a in axes]
    decls, body = find_actor()
    out += [num("rHit.GetPos Z", label, "trooper.z", decls, body), num("rHit.GetDead", label, "trooper.dead", decls, body)]
    out += [num("player.GetPos Z", label, "player.z")]
    return out


def items_lake(name, title, items, y0, axes="Z"):
    px, py, pz = PLAYER
    steps = list(PROTECT) + go_there()
    steps += [script([f"player.SetPos X {cs.number(px)}", f"player.SetPos Y {cs.number(py)}",
                      f"player.SetPos Z {cs.number(pz)}"]), {"frames": 120}]
    steps += [num(f"player.GetPos {a}", "at the surface", f"player.{a.lower()}") for a in "XYZ"]
    for _, item in items:
        n = cs.Names(*CTX)
        b = n.operand(item)
        steps += [script(n.setup + [f"player.AddItem {b} 1 1"], n.decls), {"frames": cs.SETTLE_FRAMES},
                  script(n.setup + [f"player.Drop {b} 1"], n.decls), {"frames": cs.SETTLE_FRAMES_LONG}]
    steps += [{"frames": 120}]
    # Where each dropped item fell, then under the surface with it.
    for label, item in items:
        decls, body = find_item(item)
        steps += [num(f"rHit.GetPos {a}", "dropped", f"{label}.{a.lower()}", decls, body) for a in axes]
    for i, (_, item) in enumerate(items):
        steps += [setpos_hit(find_item(item), ITEM_X, y0 + i * ITEM_DY, ITEM_Z), {"frames": 2}]
    n = cs.Names(*CTX)
    t = n.operand(TROOPER)
    steps += [script(n.setup + [f"player.PlaceAtMe {t} 1"], n.decls), {"frames": cs.SETTLE_FRAMES_LONG},
              setpos_hit(find_actor(), *ACTOR), {"frames": 10}]
    decls, body = find_actor()
    steps += [script(body + ["rHit.KillActor"], decls)]
    steps += [{"remember": cs.function(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"]), "as": "t0"}]
    for k, gap in enumerate(GAPS):
        if gap:
            steps.append({"frames": gap})
        steps += readings(f"reading {k}", items, axes)
    j = cs.job(name, "nvfstrip (any exterior place); travels to BoulderBeachCampground with coc "
               "(god mode, AI detection off).", steps, cs.MASTERS)
    j["title"] = title
    return j


# The second job (after fl_items_lake, 2026-10-25): in that run the lead pipe and the
# sledgehammer stopped 80-120 units under where they were put and never moved, over 100
# units above the bed the other sinking items reached. Its first run (without NEAR) read
# the save's own bat and pipe 3,800 units away, so the weapons of fl_items_lake are not
# trusted; the sledgehammer, ours that time, floated. Again with NEAR, x and y read too,
# 400 units further south: the three weapons, the hot plate and sensor module (glass
# and metal that sank), and the toaster (sank both times).
HEAVY = [ITEMS[12], ITEMS[13], ITEMS[14], ITEMS[10], ITEMS[6], ITEMS[11]]


def main():
    jobs = [items_lake("fl_items_lake", "Floating: fifteen items and a killed NCR trooper put 150 units under Lake Mead "
                       "(about 1.5 min, no input)", ITEMS, ITEM_Y0),
            items_lake("fl_heavy_lake", "Floating again: the bat, lead pipe, sledgehammer, hot plate, sensor module and "
                       "toaster and a killed NCR trooper, 400 units south of the first run (about 1.5 min, no input)", HEAVY, ITEM_Y0 - 400.0, "XYZ")]
    for j in jobs:
        path = os.path.join(HERE, f"{j['name']}.steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
