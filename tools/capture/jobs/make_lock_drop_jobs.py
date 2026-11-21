#!/usr/bin/env python3
"""Writes the follow-up capture jobs of the lock and drop scenarios
(`L4.sim.trace.goodsprings_locked_safe_*`, `L4.sim.trace.goodsprings_drop_and_retake`;
docs/spec-notes/layer4-captures.md section 2) into this folder. They run unattended
(capture_run.py). Forms are global FormIDs of the ten masters (looked up read-only with
oracle_world.py and capture_steps.edid_index, 2026-10-08); no game data is stored here.

  python tools/capture/jobs/make_lock_drop_jobs.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402

CTX = (cs.MASTERS, None)
STIMPAK = "0x00015169"
GAS_SAFE = "0x0010B2D4"         # VLootSafeFloorTier1WastelandLock, level 25, no key; GSGasStation
BM_RADIO = "0x000E60F4"         # cell BlackMountainRadio2
BM_TRUNK = "0x0015D0D0"         # BMLootTrunk, level 255, key BMTabithaLootKey
BM_KEY = "0x0015D0AC"           # BMTabithaLootKey
DOC_DOOR = "0x00103E61"         # GSDocMitchellHouseIntDoorREF

PROTECT = [{"console": "tgm"}, {"console": "tdetect"}]


def q(expr, label, field, *names):
    return cs.query(cs.number_query(expr, CTX, list(names)), label, field)


def act(stmts, *names):
    return [{"script": cs.run_script(stmts, CTX, list(names))}, {"frames": cs.SETTLE_FRAMES_LONG}]


def job(name, title, start, steps):
    j = cs.job(name, start, steps, cs.MASTERS)
    j["title"] = title
    return j


def lock_reads(label, ref):
    return [q("{0}.GetLocked", label, "locked", ref), q("{0}.GetLockLevel", label, "level", ref)]


def menus(label):
    """Which menu an activation left open, then close them all."""
    return [cs.query(cs.function(["float fV"], [f"let fV := MenuMode {m}", "SetFunctionValue fV"]), label, field)
            for m, field in ((cs.CONTAINER_MENU, "container_menu"), (1014, "lockpick_menu"))] + [
        cs.query(cs.function(["float fV"], ["let fV := MenuMode", "SetFunctionValue fV"]), label, "any_menu"),
        {"console": "CloseAllMenus"}, {"frames": cs.SETTLE_FRAMES_LONG},
        cs.query(cs.function(["float fV"], ["let fV := MenuMode", "SetFunctionValue fV"]), label, "any_menu_after_close")]


def lock_levels():
    """GetLocked at lock level 255: the wiki gives 2 ("locked out") for an impossible
    level, the scenario's capture read 1 after `Lock 255`. Read GetLockLevel with it,
    for a scripted Lock 255 / Lock 100 and for an authored level-255 lock with a key."""
    steps = list(PROTECT)
    steps += lock_reads("gas safe at start", GAS_SAFE)
    steps += act(["{0}.Lock 255"], GAS_SAFE) + lock_reads("gas safe after Lock 255", GAS_SAFE)
    steps += act(["{0}.Activate player"], GAS_SAFE) + menus("gas safe activated at 255")
    steps += act(["{0}.Lock 100"], GAS_SAFE) + lock_reads("gas safe after Lock 100", GAS_SAFE)
    steps += [{"console": "coc BlackMountainRadio2"}, {"until": cs.in_cell_until(BM_RADIO, CTX),
                                                       "max_frames": cs.DOOR_LOOPS},
              {"frames": cs.AFTER_DOOR_FRAMES}, cs.query(cs.cell_query("player", CTX), "arrived", "cell")]
    steps += act(["player.RemoveAllItems"])
    steps += lock_reads("trunk at start", BM_TRUNK)
    steps += act(["{0}.Activate player"], BM_TRUNK) + menus("trunk activated without its key")
    steps += act(["player.AddItem {0} 1 1"], BM_KEY)
    steps += act(["{0}.Activate player"], BM_TRUNK) + menus("trunk activated with its key")
    steps += lock_reads("trunk after opening with its key", BM_TRUNK)
    return job("ld_lock_levels", "GetLocked and GetLockLevel at lock level 255 (about 1 min, no input)",
               "nvftest (inside the Goodsprings gas station); travels to BlackMountainRadio2 with coc "
               "(god mode, AI detection off).", steps)


def scan(k, taken):
    """The k-th created (mod index 255) stimpak reference in the player's cell
    (GetFirstRef 201, IncludeTakenRefs `taken`): its FormID and the number found."""
    n = cs.Names(*CTX)
    base = n.operand(STIMPAK)
    body = n.setup + [f"let rScan := GetFirstRef {cs.INVENTORY_ITEM_REFS} 0 {taken}", "while rScan != 0",
                      "  let rScanBase := rScan.GetBaseObject", f"  if rScanBase == {base}",
                      f"    if rScan.GetSourceModIndex == {cs.CREATED_MOD_INDEX}",
                      f"      if fN == {k}", "        let rHit := rScan", "      endif",
                      "      let fN := fN + 1", "    endif", "  endif", "  let rScan := GetNextRef", "loop"]
    return n.decls + ["ref rScan", "ref rScanBase", "ref rHit", "float fN"], body


def created_refs(label, taken, slots=3):
    decls, body = scan(-1, taken)
    out = [cs.query(cs.function(decls + ["float fV"], body + ["let fV := fN", "SetFunctionValue fV"]),
                    label, f"taken{taken}.found")]
    for k in range(slots):
        decls, body = scan(k, taken)
        out.append(cs.query(cs.function(decls, body + ['SetFunctionValue (GetFormIDString rHit)']),
                            label, f"taken{taken}.ref{k}"))
        for expr, field in (("rHit.GetRefCount", "count"), ("rHit.GetDisabled", "disabled"),
                            ("rHit.GetPos Z", "z")):
            out.append(cs.query(cs.function(decls + ["float fV"], body + [f"let fV := {expr}", "SetFunctionValue fV"]),
                                label, f"taken{taken}.ref{k}.{field}"))
    return out


def all_created(label):
    return created_refs(label, 0) + created_refs(label, 1) + [
        q("{0}.GetItemCount {1}", label, "player_stimpaks", "player", STIMPAK)]


def drop_refs():
    """Which references a drop creates and what a scripted pick-up leaves: the
    scenario's second re-take gave 1 stimpak back for a drop of 2."""
    steps = list(PROTECT)
    steps += act(["player.RemoveAllItems", "player.AddItem {0} 3 1"], STIMPAK)
    steps += [{"script": cs.run_script(["{0}.Activate player"], CTX, [DOC_DOOR])},
              {"until": cs.left_cell_until(DOC_DOOR, CTX), "max_frames": cs.DOOR_LOOPS},
              {"frames": cs.AFTER_DOOR_FRAMES}]
    steps += [cs.query(cs.cell_query("player", CTX), "outside", "cell")] + [
        q(f"{{0}}.GetPos {a}", "outside", f"pos.{a.lower()}", "player") for a in "XYZ"] + [
        q("{0}.GetAngle Z", "outside", "angle_z", "player")]
    steps += act(["player.Drop {0} 1"], STIMPAK) + [{"frames": cs.DROP_FRAMES}] + all_created("after dropping one")
    steps += act(["{0}.Activate player"], f"{cs.CREATED_PREFIX}{STIMPAK}") + all_created("after taking it back")
    steps += act(["player.Drop {0} 2"], STIMPAK) + [{"frames": cs.DROP_FRAMES}] + all_created("after dropping two")
    steps += [{"frames": 600}] + all_created("10 s later")
    return job("ld_drop_refs", "References created by Drop and left by a scripted pick-up (about 1 min, no input)",
               "nvfdoc (inside Doc Mitchell's house; leaves through the front door).", steps)


def drop_stack():
    """Scripted pick-up of a dropped stack: the scenario's re-take of a dropped pair
    gave 1 stimpak back, and ld_drop_refs showed that the drop made one reference of
    count 2. Activate a dropped stack of 2 and of 3 and read what is left."""
    steps = list(PROTECT)
    steps += act(["player.RemoveAllItems", "player.AddItem {0} 5 1"], STIMPAK)
    steps += [{"script": cs.run_script(["{0}.Activate player"], CTX, [DOC_DOOR])},
              {"until": cs.left_cell_until(DOC_DOOR, CTX), "max_frames": cs.DOOR_LOOPS},
              {"frames": cs.AFTER_DOOR_FRAMES}]
    created = f"{cs.CREATED_PREFIX}{STIMPAK}"
    steps += act(["player.Drop {0} 2"], STIMPAK) + [{"frames": cs.DROP_FRAMES}] + all_created("dropped two")
    steps += act(["{0}.Activate player"], created) + all_created("pair activated once")
    steps += act(["{0}.Activate player"], created) + all_created("pair activated twice")
    steps += act(["player.Drop {0} 3"], STIMPAK) + [{"frames": cs.DROP_FRAMES}] + all_created("dropped three")
    steps += act(["{0}.Activate player"], created) + all_created("three activated once")
    return job("ld_drop_stack", "Scripted Activate on a dropped stack of 2 and 3 (about 1 min, no input)",
               "nvfdoc (inside Doc Mitchell's house; leaves through the front door).", steps)


def placed_stack():
    """Does a placed stack (XCNT 12 BB rounds) give one item to a scripted Activate too,
    like a dropped stack (ld_drop_stack)?"""
    steps = list(PROTECT)
    steps += [{"console": "coc GSHouseInterior03"}, {"until": cs.in_cell_until(GS_HOUSE_3, CTX),
                                                     "max_frames": cs.DOOR_LOOPS},
              {"frames": cs.AFTER_DOOR_FRAMES}, cs.query(cs.cell_query("player", CTX), "arrived", "cell")]
    steps += act(["player.RemoveAllItems"])

    def read(label):
        return [q("{0}.GetItemCount {1}", label, "player_bb", "player", AMMO_BB),
                q("{0}.GetRefCount", label, "ref_count", BB_STACK), q("{0}.GetDisabled", label, "disabled", BB_STACK)]
    steps += read("before")
    steps += act(["{0}.Activate player"], BB_STACK) + read("activated once")
    steps += act(["{0}.Activate player"], BB_STACK) + read("activated twice")
    return job("ld_placed_stack", "Scripted Activate on a placed stack of 12 BB rounds (about 30 s, no input)",
               "nvftest (any save; travels to GSHouseInterior03 with coc, god mode, AI detection off).", steps)


GS_HOUSE_3 = "0x00105619"       # cell GSHouseInterior03
BB_STACK = "0x001730EA"         # AmmoBB, XCNT 12, GSHouseInterior03
AMMO_BB = "0x0002935B"          # AmmoBB


def main():
    for j in (lock_levels(), drop_refs(), drop_stack(), placed_stack()):
        path = os.path.join(HERE, f"{j['name']}.steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(j, indent=1) + "\n")
        print(f"{path}: {len(j['steps'])} steps")


if __name__ == "__main__":
    main()
