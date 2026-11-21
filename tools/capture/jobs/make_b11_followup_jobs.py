#!/usr/bin/env python3
"""Writes the B11 follow-up capture jobs for the rock and maize rows of
`L4.phys.drops.b11_goodsprings` (docs/spec-notes/physics.md section 4.2) into this folder.
They run unattended (capture_run.py, save `nvfdoc`, god mode and AI detection off) and
need no one at the controls.

  python tools/capture/jobs/make_b11_followup_jobs.py

The drops come from `diag_maize.drops.json` and `diag_rocks.drops.json` (fnv.phys.drops/1,
so `fnvtool phys cell --drops-file` runs the same drops as the candidate). In game each
drop is: optional console lines (`console_before`), SetPos at `at`, wait until the player
has fallen 1 unit (as capture_moves.py does), then one "x y z t s" sample per game loop
for TRACE loops (t: real milliseconds since the fall began, from the game clock), a scan
for actors near the player, a pause, the feet, another pause and the feet again
(`feet_later`), then optional console lines (`console_after`). Only script text,
FormIDs and positions chosen with `fnvtool phys cell` are stored here, no game data.

Script functions used, from the GECK wiki [documented]: SetPos, GetPos, GetCurrentTime,
GetSecondsPassed, GetFirstRef / GetNextRef (form type 200: actors), GetDistance,
GetFormIDString (xNVSE); console Disable / Enable on a reference.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_moves as cm  # noqa: E402
import capture_steps as cs  # noqa: E402

START = "nvfdoc (Doc Mitchell's house); leaves through the front door (god mode, AI detection off)."
DOC_DOOR = "0x00103E61"         # GSDocMitchellHouseIntDoorREF
TRACE = 120                     # loops sampled after the fall begins (a 290-unit fall takes ~55)
SETTLE = 180                    # loops between the trace and the first reading
LATER = 120                     # loops between the two readings
ACTORS = 200                    # GetFirstRef form type of actors [documented: GECK wiki GetFirstRef]
NEAR = 600                      # actors within this distance are counted

# Actors in the loaded cells around the player (cell depth 1): how many are within NEAR
# units, and the nearest one's distance and FormID. The player itself (distance 0) is skipped.
ACTOR_SCAN = cs.function(
    ["ref rScan", "ref rNear", "float fD", "float fN", "float fMin"],
    ["let fMin := 99999", f"let rScan := GetFirstRef {ACTORS} 1", "while rScan != 0",
     "  let fD := rScan.GetDistance player", "  if fD > 1",
     f"    if fD < {NEAR}", "      let fN := fN + 1", "    endif",
     "    if fD < fMin", "      let fMin := fD", "      let rNear := rScan", "    endif",
     "  endif", "  let rScan := GetNextRef", "loop",
     'SetFunctionValue (($ fN) + " " + ($ fMin) + " " + (GetFormIDString rNear))'])


def drop_steps(doc):
    name = doc["name"]
    steps = []
    for d in doc["drops"]:
        at = d["at"]
        label = f"{name} {d['label']}"
        steps += [{"console": line} for line in d.get("console_before", [])]
        steps += [cm.setpos(*at), {"until": cm.below(at[2]), "max_frames": cm.UNTIL_FALL},
                  cs.query(cm.below(at[2]), label, "fell"),
                  {"remember": cm.fn(["float fV"], ["let fV := GetCurrentTime", "SetFunctionValue fV"]), "as": "t0"}]
        steps += [cs.query(cm.SAMPLE, label, f"s{i:03d}") for i in range(TRACE)]
        steps += [cs.query(ACTOR_SCAN, label, "actors"), {"frames": SETTLE},
                  cs.query(cm.FEET, label, "feet"), {"frames": LATER}, cs.query(cm.FEET, label, "feet_later")]
        steps += [{"console": line} for line in d.get("console_after", [])]
    return steps


def make(drops_file, name, title):
    doc = cs.load_json(os.path.join(HERE, drops_file))
    steps = [{"console": "tgm"}, {"console": "tdetect"},
             {"script": cs.run_script(["{0}.Activate player"], cm.CTX, [DOC_DOOR])},
             {"until": cs.left_cell_until(DOC_DOOR, cm.CTX), "max_frames": cs.DOOR_LOOPS},
             {"frames": cs.AFTER_DOOR_FRAMES}]
    steps += drop_steps(doc)
    j = cs.job(name, START, steps, cs.MASTERS)
    j["title"] = title
    return j


def main():
    jobs = [make("diag_maize.drops.json", "mv_diag_maize",
                 "B11 maize follow-up: 15 traced drops at and around MaizePickable 0x0010B1BB (about 2 min, no input)"),
            make("diag_rocks.drops.json", "mv_diag_rocks",
                 "B11 rock follow-up: repeated drops on two rocks and open ground, a grid on rock a (about 3 min, no input)")]
    for j in jobs:
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
