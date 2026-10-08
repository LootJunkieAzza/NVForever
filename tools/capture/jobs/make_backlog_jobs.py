#!/usr/bin/env python3
"""Writes the capture jobs for the Layer 4 capture backlog (docs/spec-notes/layer4-captures.md
section 3) into this folder. The jobs run unattended (capture_run.py) on a save inside the
Goodsprings gas station; those that travel use the console's coc and wait until the player
has arrived. Forms are global FormIDs of the ten masters (looked up read-only with
oracle_world.py / oracle_sim.py, 2026-10-08); no game data is stored here.

  python tools/capture/jobs/make_backlog_jobs.py
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402

CTX = (cs.MASTERS, None)
CAPS = "0x0000000F"
RINGO_BOX = "0x001629C0"         # Ringo's cash box (CaravanCashRingo, Respawns), GSGasStation
PRIMM_BOX = "0x0017409C"         # VendorContainerPrimmDenMerchantREF, base without Respawns
GS_STORE = "0x001057F1"          # cell GSGeneralStore
GS_STATION = "0x00109038"        # cell GSGasStation
FRIDGE = "0x00105824"            # VLootFridgeClosedTier1WastelandRS (Respawns), GSGeneralStore
TOOLBOX = "0x001057FF"           # ToolboxContainer, GSGeneralStore
PRIMM_DEN = "0x00153680"         # cell SLProspectorsDenInt
NIPTON_HOUSE = "0x001393F6"      # cell NiptonHouse8Paranoid
TEDDY = "0x0001F21F"             # TeddyBear01, MISC weight 1.0
SPOON = "0x000340BC"             # Spoon01, MISC weight 0.5
AMMO_9MM = "0x0008ED03"          # Ammo9mm
FRAG_MINE_WEAPON = "0x0000433C"  # weapon the candidate says a frag mine disarms to
MINES = ["0x0013A656", "0x0013A655", "0x00139552", "0x00139551"]  # PGRE MineFragProjectile, NiptonHouse8Paranoid

PROTECT = [{"console": "tgm"}, {"console": "tdetect"}]
CLOCK = [("hour", "GameHour"), ("days", "GameDaysPassed"), ("day", "GameDay"), ("month", "GameMonth"),
         ("year", "GameYear")]


def q(expr, label, field, *names):
    return cs.query(cs.number_query(expr, CTX, list(names)), label, field)


def act(stmts, *names):
    return [{"script": cs.run_script(stmts, CTX, list(names))}, {"frames": cs.SETTLE_FRAMES_LONG}]


def clock(label, fields=CLOCK):
    return [cs.query(cs.global_query(g), label, f) for f, g in fields]


def box(label, ref):
    return [q("{0}.GetItemCount {1}", label, "count", ref, CAPS), q("{0}.GetNumItems", label, "items", ref)]


def advance(i, hours):
    return cs.time_steps(i, hours, "hour")


def coc(cell_edid, cell_fid):
    return [{"console": f"coc {cell_edid}"}, {"until": cs.in_cell_until(cell_fid, CTX), "max_frames": cs.DOOR_LOOPS},
            {"frames": cs.AFTER_DOOR_FRAMES}, cs.query(cs.cell_query("player", CTX), "arrived", "cell")]


def job(name, title, start, steps):
    j = cs.job(name, start, steps, cs.MASTERS)
    j["title"] = title
    return j


def calendar():
    """B5: month lengths. Day 27 of every month of 2281 (and February 2280), then five
    steps of +24 hours (GameHour past 24), reading the calendar after each."""
    steps = []
    n = 0
    cases = [(2281, m) for m in range(12)] + [(2280, 1)]
    for year, month in cases:
        steps += [{"console": f"set GameYear to {year}"}, {"console": f"set GameMonth to {month}"},
                  {"console": "set GameDay to 27"}, {"frames": cs.SETTLE_FRAMES}]
        steps += clock(f"y{year} m{month} start", CLOCK[2:])
        for k in range(1, 6):
            steps += advance(n, 24) + clock(f"y{year} m{month} +{k}d", CLOCK[2:])
            n += 1
    return job("bl_calendar", "B5 calendar: month lengths (about 1 min, no input)",
               "Any save (moves the clock about 65 days and sets the date).", steps)


def encumbrance():
    """B7: the over-encumbered threshold and ammunition weight in Hardcore mode. No tgm:
    god mode is said to lift the carry limit [hypothesis], which is what is measured."""
    flags = [
        ("IsPlayerOverencumbered", "let fV := IsPlayerOverencumbered"),
        ("player.IsPlayerOverencumbered", "let fV := player.IsPlayerOverencumbered"),
        ("GetIsPlayerOverencumbered", "let fV := GetIsPlayerOverencumbered"),
        ("player.GetIsPlayerOverencumbered", "let fV := player.GetIsPlayerOverencumbered"),
    ]

    def read(label):
        out = [q("{0}.GetAV InventoryWeight", label, "weight", "player"),
               q("{0}.GetAV CarryWeight", label, "capacity", "player")]
        out += [cs.query(cs.function(["float fV"], [stmt, "SetFunctionValue fV"]), label, f"over:{name}")
                for name, stmt in flags]
        return out

    steps = act(["player.RemoveAllItems"]) + read("E0 empty")
    steps += act(["player.AddItem {0} 199 1"], TEDDY) + read("E1 199 teddy bears")
    steps += act(["player.AddItem {0} 1 1"], TEDDY) + read("E2 200 teddy bears")
    steps += act(["player.AddItem {0} 1 1"], SPOON) + read("E3 200 teddy bears and a spoon")
    steps += act(["player.AddItem {0} 1 1"], SPOON) + read("E4 200 teddy bears and two spoons")
    steps += act(["player.RemoveItem {0} 2"], SPOON) + read("E5 back to 200 teddy bears")
    steps += act(["player.RemoveAllItems", "player.AddItem {0} 100 1"], AMMO_9MM)
    steps += read("H0 100 9mm rounds") + [q("IsHardcore", "H0 100 9mm rounds", "hardcore")]
    steps += act(["player.SetHardcore 1"]) + read("H1 hardcore on") + [q("IsHardcore", "H1 hardcore on", "hardcore")]
    steps += act(["player.SetHardcore 0"]) + read("H2 hardcore off") + [q("IsHardcore", "H2 hardcore off", "hardcore")]
    steps += act(["player.RemoveAllItems"])
    return job("bl_encumbrance", "B7 encumbrance edges and Hardcore ammo weight (about 20 s, no input)",
               "A test character (empties the inventory; Hardcore is switched on and off, not saved).", steps)


def restock_timing():
    """B4: does a merchant box restock on access or on a schedule? Empty Ringo's box, let
    100 hours pass without reading it, read (time T), empty it again, then read at T+24,
    T+48 and every 2 hours from T+50 to T+80."""
    steps = box("R0 at start", RINGO_BOX) + clock("R0 at start", CLOCK[:2])
    steps += act(["{0}.RemoveAllItems"], RINGO_BOX) + box("R1 emptied", RINGO_BOX) + clock("R1 emptied", CLOCK[:2])
    n = 0
    for h in (24, 24, 24, 24, 4):
        steps += advance(n, h)
        n += 1
    steps += box("R2 first read after 100 h", RINGO_BOX) + clock("R2 first read after 100 h", CLOCK[:2])
    steps += act(["{0}.RemoveAllItems"], RINGO_BOX) + box("R3 emptied at T", RINGO_BOX)
    elapsed = 0
    for target in [24, 48] + list(range(50, 82, 2)):
        steps += advance(n, target - elapsed)
        n += 1
        elapsed = target
        steps += box(f"T+{target}", RINGO_BOX) + clock(f"T+{target}", CLOCK[:2])
    return job("bl_restock_timing", "B4 merchant restock: on access or on a schedule (about 1 min, no input)",
               "Inside the Goodsprings gas station; do not open the barter menu.", steps)


def primm_restock():
    """B2: a merchant box whose base has no Respawns flag. Empty it, read it daily for a week."""
    steps = PROTECT + coc("SLProspectorsDenInt", PRIMM_DEN)
    steps += box("P0 at start", PRIMM_BOX) + clock("P0 at start", CLOCK[:2])
    steps += act(["{0}.RemoveAllItems"], PRIMM_BOX) + box("P1 emptied", PRIMM_BOX)
    for d in range(1, 8):
        steps += advance(d, 24) + box(f"P{d + 1} {d * 24} h after emptying", PRIMM_BOX)
        steps += clock(f"P{d + 1} {d * 24} h after emptying", CLOCK[:2])
    return job("bl_primm_restock", "B2 Primm merchant box without the Respawns flag (about 30 s, no input)",
               "Any save (travels to the Prospector's Den with coc; god mode, AI detection off).", steps)


def container_respawn():
    """B3: cell respawn of ordinary containers. Fridge (Respawns flag) and toolbox in the
    General Store: empty both, then leave the cell (coc to the gas station), wait, come
    back (coc) and read: 24 h, 74 h; empty again; 70 h, 74 h."""
    there = coc("GSGeneralStore", GS_STORE)
    away = coc("GSGasStation", GS_STATION)

    def read(label):
        return box(label, FRIDGE) + box(label + " toolbox", TOOLBOX) + clock(label, CLOCK[:2])

    steps = PROTECT + there + read("C0 at start")
    steps += act(["{0}.RemoveAllItems", "{1}.RemoveAllItems"], FRIDGE, TOOLBOX) + read("C1 emptied")
    n = 0
    for label, hours, empty_first in [("C2 back after 24 h away", 24, False), ("C3 back after 74 h away", 74, False),
                                      ("C5 back after 70 h away", 70, True), ("C6 back after 74 h away", 74, False)]:
        if empty_first:
            steps += act(["{0}.RemoveAllItems", "{1}.RemoveAllItems"], FRIDGE, TOOLBOX) + read("C4 emptied again")
        steps += away
        left = hours
        while left > 0:
            steps += advance(n, min(24, left))
            n += 1
            left -= min(24, left)
        steps += there + read(label)
    return job("bl_container_respawn", "B3 cell respawn of a fridge and a toolbox (about 1 min, no input)",
               "Any save (travels between the General Store and the gas station with coc; god mode, AI detection off).",
               steps)


def placed_mines():
    """B10: are placed frag mines live? Read them on arrival and after a pause, then
    activate two (the simulation says: disarmed, the player gets the Frag Mine weapon)."""
    def read(label):
        out = [q("{0}.GetItemCount {1}", label, "count", "player", FRAG_MINE_WEAPON)]
        for m in MINES:
            out += [q("{0}.GetDisabled", f"{label} {m}", "disabled", m)]
            out += [q(f"{{0}}.GetPos {a}", f"{label} {m}", f"pos.{a.lower()}", m) for a in "XYZ"]
            out += [cs.query(cs.cell_query(m, CTX), f"{label} {m}", "cell")]
        return out

    steps = PROTECT + coc("NiptonHouse8Paranoid", NIPTON_HOUSE) + read("M0 on arrival")
    steps += [{"frames": 600}] + read("M1 after 600 game loops")
    steps += act(["{0}.Activate player"], MINES[0]) + [{"frames": 120}] + read("M2 after activating " + MINES[0])
    steps += act(["{0}.Activate player"], MINES[1]) + [{"frames": 120}] + read("M3 after activating " + MINES[1])
    return job("bl_placed_mines", "B10 placed frag mines: live, and what activating one does (about 30 s, no input)",
               "Any save (travels to Nipton house 8 with coc; god mode, AI detection off).", steps)


def main():
    for j in (calendar(), encumbrance(), restock_timing(), primm_restock(), container_respawn(), placed_mines()):
        path = os.path.join(HERE, f"{j['name']}.steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(j, indent=1) + "\n")
        print(f"{path}: {len(j['steps'])} steps")


if __name__ == "__main__":
    main()
