#!/usr/bin/env python3
"""Writes the Layer 4 simulation close-out capture jobs (2026-10-09; docs/design/layer4.md
section 5, docs/spec-notes/layer4-captures.md sections 2 and 3) into this folder. All run
unattended (capture_run.py). No game data is stored here: only script text, FormIDs and
positions chosen with `fnvtool sim containers`, `fnvtool world refs` and `fnvtool phys
cell` (read-only).

  python tools/capture/jobs/make_l4sim_closeout_jobs.py

Jobs and saves (run each save's jobs in one capture_run.py session; the jobs of a session
run in alphabetical order and share the game's state, e.g. the console toggles tgm and
tdetect):

- `cl_a_pipboy_<save>` (nvftest, nvfnew, nvfdoc): is the Pip-Boy Glove (and the Pip-Boy
  3000) equipped, read-only; on nvftest also what `player.RemoveAllItems` does to them,
  and `iHoursToRespawnCell`.
- `cl_b_encumbrance_speed` (nvftest, after cl_a): the over-encumbered threshold seen as
  running speed (B7): forward held for 90 game loops at carried weights 199.5, 200,
  200.5 and 210.5 against a capacity of 200. No god mode (it is said to lift the limit).
- `cl_c_armor_models` (nvftest, after cl_b): which model of an armour lies on the ground:
  four armours placed with PlaceAtMe in a row on flat ground in Goodsprings at noon,
  pictures (`screenshot`), then two dropped from the inventory, then the same after the
  console's `sexchange` (the female models).
- `cl_d_mines_live` (nvftest, last: it may kill the player): placed frag mines in Nipton
  house 8 (B10): the player's own activation (`TapControl 5`, the Activate control
  [documented: GECK wiki TapControl]) looking at one from 70 units, then standing on
  another, with Health raised and no god mode.
- `cl_cell_respawn` (nvftest, its own session): scenario
  `L4.sim.trace.goodsprings_cell_respawn` (parity-lab/inputs/layer4/
  goodsprings_cell_respawn.actions.json): leaving the cell is the console's coc to the gas
  station, coming back coc to the General Store; time moves by GameHour (`--time hour`,
  xnvse-capture.md section 7). Extra fields on the fridge rows (`items`, `days`, `hour`,
  Chet's merchant box) are diagnostics the scenario does not compare.
- `cl_nipton_placed_mines` (nvftest, after cl_cell_respawn): scenario
  `L4.sim.trace.nipton_placed_mines` (scripted Activate on a placed frag mine).
"""
import json
import math
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_frames as cf  # noqa: E402
import capture_steps as cs  # noqa: E402
import make_backlog_jobs as mb  # noqa: E402
import make_movement_jobs as mm  # noqa: E402

LAB_INPUTS = os.path.normpath(os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4"))
CTX = (cs.MASTERS, None)

PIPBOY_GLOVE = "0x00025B83"     # PipBoyGlove (ARMO, non-playable), carried by NPC_ Player 0x7
PIPBOY = "0x00015038"           # PipBoy (ARMO, non-playable), carried by NPC_ Player 0x7
CHET_BOX = "0x00104C7A"         # VendorContainerChet (Respawns, leveled stock), NVVendorChestsCell
TEDDY = mb.TEDDY

# Goodsprings: flat LAND at z 8352 north of this point (make_movement_jobs.RUN_START).
FLAT = mm.RUN_START
FLAT_Z = mm.GROUND_Z
FLAT_GRID = (math.floor(FLAT[0] / cf.CELL), math.floor(FLAT[1] / cf.CELL))

# Armours whose models differ in a way a picture shows (ARMO MODL / MOD2 / MOD3 / MOD4,
# file names from the masters [observed, 2026-10-09]).
ARMORS = [
    ("0x0010EA7F", "GamblerMHat01"),       # MODL fedora, MOD2 fedora _GO, MOD3 pillbox, MOD4 pillbox _GO
    ("0x00020423", "ArmorLeather"),        # MODL OutfitM (biped), MOD2 GO.NIF, MOD3 OutfitF, no MOD4
    ("0x000EF1CD", "OutfitRepublican03"),  # MODL republican_03, MOD2 republican_01_GO, MOD4 republicanF_03_GO
    ("0x0014B054", "HatRaul"),             # MODL only
]

# Nipton house 8 (NiptonHouse8Paranoid): floor 4488 (`fnvtool phys cell`), walls north of
# mine B at y -76 and south at -316.
NIPTON_HOUSE = mb.NIPTON_HOUSE
MINE_A = ("0x0013A656", 668.19, -432.49)
MINE_B = ("0x00139551", 657.51, -219.85)
FLOOR_Z = 4488.0
FRAG_MINE_WEAPON = mb.FRAG_MINE_WEAPON


def num(expr, label, field, *names):
    return cs.query(cs.number_query(expr, CTX, list(names)), label, field)


def script(lines, *names):
    return {"script": cs.run_script(lines, CTX, list(names))}


def go(cell_edid, cell_fid):
    """coc to an interior and wait until the player is there (no observation)."""
    return [{"console": f"coc {cell_edid}"}, {"until": cs.in_cell_until(cell_fid, CTX), "max_frames": cs.DOOR_LOOPS},
            {"frames": cs.AFTER_DOOR_FRAMES}]


def go_flat():
    gx, gy = FLAT_GRID
    return [{"console": f"cow WastelandNV {gx} {gy}"}, {"until": cf.in_square_until(gx, gy), "max_frames": cs.DOOR_LOOPS},
            {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": cf.PLACE_FRAMES}]


def write(job):
    path = os.path.join(HERE, f"{job['name']}.steps.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(job, indent=1) + "\n")
    print(f"{path}: {len(job['steps'])} steps")


# --------------------------------------------------------------------------- Pip-Boy

def pipboy(save, strip):
    def read(label):
        return [num("{0}.GetEquipped {1}", label, "glove_equipped", "player", PIPBOY_GLOVE),
                num("{0}.GetItemCount {1}", label, "glove_count", "player", PIPBOY_GLOVE),
                num("{0}.GetEquipped {1}", label, "pipboy_equipped", "player", PIPBOY),
                num("{0}.GetItemCount {1}", label, "pipboy_count", "player", PIPBOY),
                num("{0}.GetNumItems", label, "items", "player"),
                num("{0}.GetAV InventoryWeight", label, "weight", "player"),
                num("{0}.GetIsSex Female", label, "female", "player"),
                num("{0}.GetLevel", label, "level", "player")]

    steps = read("P0 as loaded")
    steps.append(cs.query(cs.function(["float fV"], ['let fV := GetGameSetting "iHoursToRespawnCell"',
                                                     "SetFunctionValue fV"]), "gmst", "iHoursToRespawnCell"))
    if strip:
        steps += mb.act(["player.RemoveAllItems"]) + read("P1 after player.RemoveAllItems")
        steps += mb.act(["player.EquipItem {0}"], PIPBOY_GLOVE) + read("P2 after EquipItem glove")
    title = "Pip-Boy Glove and Pip-Boy equipped? (read-only" + (", then RemoveAllItems)" if strip else ")")
    return mb.job(f"cl_a_pipboy_{save}", title, save, steps)


# --------------------------------------------------------------------------- B7 speed

def encumbrance_speed(name="cl_b_encumbrance_speed", cases=None):
    """Over-encumbered threshold as running speed: no observable flag compiles
    (bl_encumbrance), but over-encumbered players cannot run [hypothesis to test].
    `cases`: (statements, names, tag) adding items before each run."""
    x, y = FLAT

    def run(tag):
        out = [mm.setpos(x, y, FLAT_Z + 4), mm.script("player.SetAngle Z 0"), {"frames": 120},
               num("{0}.GetAV InventoryWeight", tag, "weight", "player"),
               num("{0}.GetAV CarryWeight", tag, "capacity", "player"), mm.remember_t0()]
        out += mm.samples(f"{tag} still", 5)
        out += [mm.hold(0)] + mm.samples(f"{tag} run", 90)
        out += [num("{0}.IsRunning", tag, "running", "player"), mm.release(0), {"frames": 60}]
        return out

    steps = [{"console": "tdetect"}] + go_flat()
    steps += mb.act(["player.RemoveAllItems"])
    if cases is None:
        cases = [(["player.AddItem {0} 199 1", "player.AddItem {1} 1 1"], [TEDDY, mb.SPOON], "W199.5"),
                 (["player.AddItem {0} 1 1"], [mb.SPOON], "W200.0"),
                 (["player.AddItem {0} 1 1"], [mb.SPOON], "W200.5"),
                 (["player.AddItem {0} 10 1"], [TEDDY], "W210.5")]
    for stmts, names, tag in cases:
        steps += mb.act(stmts, *names) + run(tag)
    steps += mb.act(["player.RemoveAllItems"])
    return mb.job(name, "B7 over-encumbered threshold as running speed (about 1 min, no input)",
                  "nvftest (goes to Goodsprings with cow; no god mode; AI detection off)", steps)


def encumbrance_speed2():
    """Follow-up: 200.5 still ran and 210.5 walked (cl_b_encumbrance_speed); where between?"""
    return encumbrance_speed("cl_b2_encumbrance_speed", [
        (["player.AddItem {0} 200 1", "player.AddItem {1} 2 1"], [TEDDY, mb.SPOON], "W201.0"),
        (["player.AddItem {0} 1 1"], [mb.SPOON], "W201.5"),
        (["player.AddItem {0} 1 1"], [mb.SPOON], "W202.0"),
        (["player.AddItem {0} 1 1"], [TEDDY], "W203.0"),
        (["player.AddItem {0} 2 1"], [TEDDY], "W205.0")])


# --------------------------------------------------------------------------- armour models

def armor_models():
    x, y = FLAT
    xs = [x - 90, x - 30, x + 30, x + 90]

    def place_row(tag, row_y):
        """The four armours in a row 10 units above flat ground (PlaceAtMe, then SetPos);
        each new reference's FormID is recorded."""
        out = []
        for (fid, edid), px in zip(ARMORS, xs):
            n = cs.Names(*CTX)
            base = n.operand(fid)
            body = n.setup + [f"let rI := player.PlaceAtMe {base} 1", f"rI.SetPos X {cs.number(px)}",
                              f"rI.SetPos Y {cs.number(row_y)}", f"rI.SetPos Z {cs.number(FLAT_Z + 10)}",
                              "rI.SetAngle Z 0", "SetFunctionValue (GetFormIDString rI)"]
            out += [cs.query(cs.function(n.decls + ["ref rI"], body), f"{tag} {edid}", "ref"),
                    {"frames": cs.SETTLE_FRAMES_LONG}]
        return out

    def picture(tag, dy, pitch):
        out = [cf.script(cf.FIRST_PERSON), cf.place(x, y + dy, FLAT_Z + 4, 0, pitch), {"frames": cf.PLACE_FRAMES},
               cf.place(x, y + dy, FLAT_Z + 4, 0, pitch), {"console": "CloseAllMenus"}, {"frames": cf.PLACE_FRAMES},
               {"console": "tm"}, {"frames": cf.HUD_FRAMES}, {"screenshot": tag}]
        out += [cf.num(expr, tag, field) for field, expr in cf.RECORDED]
        out += [{"console": "tm"}, cf.script("EnablePlayerControls"), {"frames": cf.HUD_FRAMES}]
        return out

    sex = lambda label: num("{0}.GetIsSex Female", label, "female", "player")  # noqa: E731
    # Noon and NVWastelandClear (WTHR 0x000FFC88 in FalloutNV.esm, load-order index 0).
    steps = go_flat() + [{"console": "set GameHour to 12"}, {"console": "fw 000FFC88"}, {"frames": cf.WEATHER_FRAMES}]
    steps += [sex("sex before"), mm.setpos(x, y, FLAT_Z + 4), {"frames": 60}] + place_row("male row", y + 150)
    steps += [{"frames": 300}] + picture("placed_male_far", 0, 40) + picture("placed_male_near", 70, 55)
    # Dropped from the inventory: Drop puts an item 50 units ahead along the heading and
    # 30 up (simulation.md section 5): the hat to the left, the leather armour to the right.
    steps += [mm.setpos(x, y - 300, FLAT_Z + 4), mm.script("player.SetAngle Z -20"), {"frames": 60}]
    steps += mb.act(["player.AddItem {0} 1 1", "player.Drop {0} 1"], ARMORS[0][0]) + [{"frames": 60}]
    steps += [mm.script("player.SetAngle Z 20"), {"frames": 30}]
    steps += mb.act(["player.AddItem {0} 1 1", "player.Drop {0} 1"], ARMORS[1][0]) + [{"frames": 300}]
    steps += picture("dropped_male", -340, 55)
    # The same with the player female: the console's SexChange [documented: community
    # console references; whether it runs from a job is part of the test], a new row
    # 300 units further north, and the first row seen again.
    steps += [{"console": "player.sexchange"}, {"frames": 120}, sex("sex after")]
    steps += [mm.setpos(x, y + 300, FLAT_Z + 4), {"frames": 60}] + place_row("female row", y + 450)
    steps += [{"frames": 300}] + picture("placed_female_far", 300, 40) + picture("placed_female_near", 370, 55)
    steps += picture("male_row_seen_as_female", 0, 40)
    steps += [{"console": "player.sexchange"}, {"frames": 120}, sex("sex restored")]
    return mb.job("cl_c_armor_models", "Which armour model lies on the ground: pictures (about 2 min, no input)",
                  "nvftest (goes to Goodsprings with cow; noon, clear weather)", steps)


# --------------------------------------------------------------------------- mines

def mines_live():
    def read(tag):
        out = [num("{0}.GetAV Health", tag, "health", "player"),
               num("{0}.GetItemCount {1}", tag, "frag_mines", "player", FRAG_MINE_WEAPON)]
        for fid, _, _ in (MINE_A, MINE_B):
            out += [num("{0}.GetDisabled", tag, f"{fid} disabled", fid),
                    num("{0}.GetPos X", tag, f"{fid} x", fid), num("{0}.GetPos Y", tag, f"{fid} y", fid),
                    num("{0}.GetPos Z", tag, f"{fid} z", fid)]
        out += [num("{0}.GetPos X", tag, "player x", "player"), num("{0}.GetPos Y", tag, "player y", "player"),
                num("{0}.GetPos Z", tag, "player z", "player")]
        return out

    crosshair = cs.query(cs.function(["ref rX"], ["let rX := GetCrosshairRef", "SetFunctionValue (GetFormIDString rX)"]),
                         "crosshair", "ref")
    steps = go("NiptonHouse8Paranoid", NIPTON_HOUSE) + read("N0 on arrival")
    steps += mb.act(["player.SetAV Health 5000"]) + read("N1 health raised")
    # Mine B from 70 units north of it, looking down at it (eye about 123 above the feet,
    # physics.md 6.3): heading 180, pitch atan(123 / 70) = 60 degrees.
    fid, mx, my = MINE_B
    steps += [mm.setpos(mx, my + 70, FLOOR_Z + 2), mm.script("player.SetAngle Z 180", "player.SetAngle X 60"),
              {"frames": 5}, crosshair, mm.script("TapControl 5"), {"frames": 30}] + read("N2 after the Activate control on B")
    steps += [{"frames": 120}] + read("N3 two seconds later")
    steps += [{"console": "CloseAllMenus"}, {"frames": 30}]
    # Standing on mine A.
    fid, mx, my = MINE_A
    steps += [mm.setpos(mx, my, FLOOR_Z + 2), {"frames": 30}] + read("N4 on A, 0.5 s")
    steps += [{"frames": 60}] + read("N5 on A, 1.5 s")
    steps += [{"frames": 120}] + read("N6 on A, 3.5 s")
    steps += [{"frames": 300}] + read("N7 on A, 8.5 s")
    return mb.job("cl_d_mines_live", "B10 placed frag mines: the player's activation and stepping on one (no god mode)",
                  "nvftest (goes to Nipton house 8 with coc; Health raised to 5000; may kill the player)", steps)


def mines_disarm():
    """Follow-up to cl_d_mines_live, whose crosshair met a coffee table in front of mine B
    (GetCrosshairRef 0x0013952F) and the mine went off: mine D 0x0013A655 at (128.5,
    -518.7) has a clear view from 70 units east (`fnvtool phys cell --ray`). The player is
    placed, turned and the Activate control tapped in one script."""
    fid, mx, my = "0x0013A655", 128.5, -518.7

    def read(tag):
        return [num("{0}.GetAV Health", tag, "health", "player"),
                num("{0}.GetItemCount {1}", tag, "frag_mines", "player", FRAG_MINE_WEAPON),
                num("{0}.GetDisabled", tag, "disabled", fid)]

    crosshair = lambda tag: cs.query(cs.function(["ref rX"], ["let rX := GetCrosshairRef",  # noqa: E731
                                                              "SetFunctionValue (GetFormIDString rX)"]), tag, "crosshair")
    steps = go("NiptonHouse8Paranoid", NIPTON_HOUSE) + mb.act(["player.SetAV Health 5000"]) + read("D0 start")
    steps += [mm.script(f"player.SetPos X {cs.number(mx + 70)}", f"player.SetPos Y {cs.number(my)}",
                        f"player.SetPos Z {cs.number(FLOOR_Z + 2)}", "player.SetAngle Z 270", "player.SetAngle X 60",
                        "TapControl 5"), {"frames": 2}, crosshair("D1 2 loops"), {"frames": 8}] + read("D1 10 loops")
    steps += [crosshair("D2 0.5 s"), {"frames": 20}] + read("D2 0.5 s")
    steps += [mm.script("TapControl 5"), {"frames": 30}] + read("D3 second tap, 1 s")
    steps += [{"frames": 180}] + read("D4 4 s") + [{"console": "CloseAllMenus"}]
    return mb.job("cl_d2_mines_disarm", "B10 follow-up: the Activate control on a placed frag mine in view (no god mode)",
                  "nvftest (goes to Nipton house 8 with coc; Health raised to 5000)", steps)


# --------------------------------------------------------------------------- scenario jobs

def scenario_job(name, actions_file, start, home, away=None, console_first=(), extra=None):
    """A scenario job from its fnv.sim.actions/1 file, like `capture_steps.py actions
    --time hour`, with detach_cell / attach_cell done by the console's coc: `away` =
    (Editor ID, FormID) of the cell to go to, `home` the scenario's cell. `extra(i, a,
    steps)` may add diagnostic queries after an action's steps (same labels, other
    fields)."""
    doc = cs.load_json(os.path.join(LAB_INPUTS, actions_file))
    steps = [{"console": c} for c in console_first] + go(*home)
    for i, a in enumerate(doc["actions"]):
        if a["op"] == "detach_cell":
            s = go(*away)
        elif a["op"] == "attach_cell":
            s = go(*home)
        else:
            s = cs.action_steps(i, a, CTX, "hour")
        if extra:
            s = s + extra(i, a)
        steps += s
    return mb.job(name, f"scenario {actions_file} (unattended)", start, steps)


def respawn_extra(i, a):
    """Diagnostics on the fridge rows: stacks, clock and Chet's merchant box."""
    if a["op"] == "add_item" and a["to"] == mb.FRIDGE:
        # Chet's box is emptied and given a teddy bear whenever the fridge is.
        return mb.act(["{0}.RemoveAllItems", "{0}.AddItem {1} 1 1"], CHET_BOX, TEDDY)
    if a["op"] != "get_item_count":
        return []
    label = a["label"]
    out = [num("{0}.GetNumItems", label, "items", a["of"])]
    if a["of"] == mb.FRIDGE:
        out += [cs.query(cs.global_query("GameDaysPassed"), label, "days"),
                cs.query(cs.global_query("GameHour"), label, "hour"),
                num("{0}.GetItemCount {1}", label, "chet_teddy", CHET_BOX, TEDDY),
                num("{0}.GetNumItems", label, "chet_items", CHET_BOX)]
    return out


def main():
    write(pipboy("nvftest", True))
    write(pipboy("nvfnew", False))
    write(pipboy("nvfdoc", False))
    write(encumbrance_speed())
    write(encumbrance_speed2())
    write(armor_models())
    write(mines_live())
    write(mines_disarm())
    write(scenario_job("cl_cell_respawn", "goodsprings_cell_respawn.actions.json",
                       "nvftest (coc between the General Store and the gas station; god mode, AI detection off)",
                       ("GSGeneralStore", mb.GS_STORE), away=("GSGasStation", mb.GS_STATION),
                       console_first=["tgm", "tdetect"], extra=respawn_extra))
    write(scenario_job("cl_nipton_placed_mines", "nipton_placed_mines.actions.json",
                       "nvftest, after cl_cell_respawn (coc to Nipton house 8)", ("NiptonHouse8Paranoid", NIPTON_HOUSE)))


if __name__ == "__main__":
    main()
