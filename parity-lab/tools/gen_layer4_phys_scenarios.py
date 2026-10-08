#!/usr/bin/env python3
"""Generate scenarios/layer4_phys.json (Layer 4 physics).

`L4.phys.heights.<World>`: the physics terrain's height under eight fixed points of every
landscape cell (`fnvtool phys heights`, docs/spec-notes/physics.md §7) against
tools/capture/oracle_phys.py, written from the spec notes only. Scenario files contain
only worldspace names and comparator settings (no game data).
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "scenarios")

LOAD_ORDER = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
              "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm"]
# The worldspaces with landscape, as for L3.world.heights.
LAND_WORLDS = ["TestMap01", "WastelandNV", "FreesideWorld", "GamorrahWorld", "GreenhouseWorld01",
               "FreesideNorthWorld", "FreesideFortWorld", "TheStripWorldNew", "TheFortWorld", "WastelandNVmini",
               "BoulderCityWorld", "Lucky38World", "NVDLC01Villa", "NVDLC01WestTownN", "NVDLC01EastTownS",
               "NVDLC01WestTownS", "NVDLC01VillaDean", "NVDLC01VillaChristine", "NVDLC01EastTownN",
               "NVDLC02ZionCanyon", "NVDLC03BigMT", "NVDLC03Higgs", "NVDLC04Road01World", "NVDLC04DivideWorld",
               "NVDLC04Road02World", "NVDLC04DivideVistaWorld", "NVDLC04NukeNCR", "NVDLC04NukeLegion",
               "NVDLC04NukeSilo2", "NVDLC01TestMap01"]
# Honest Hearts content is the holdout, as in Layers 2 and 3.
HOLDOUT_WORLDS = {"NVDLC02ZionCanyon"}
# The candidate casts f32 rays at world coordinates up to ~2.6e5 units (metres inside
# rapier); against the exact surface its error is at most 0.19 units over every
# worldspace (physics.md §3). The oracle computes in f64. 0.5 units is far below anything
# visible and covers the f32 error with margin. Set before the first comparison.
HEIGHT_TOL = {"abs_tol": 0.5, "rel_tol": 0.0}


def scenarios():
    out = []
    for w in LAND_WORLDS:
        s = {
            "id": f"L4.phys.heights.{w}", "layer": 4,
            "description": f"Physics terrain height under eight fixed points of every landscape cell in {w}.",
            "oracle": {"golden": f"phys/{w}.heights.json"},
            "candidate": {"argv": ["{candidate}", "phys", "heights", "{data_dir}", *LOAD_ORDER,
                                   "--schema", "{schema_dir}", "--world", w], "timeout_s": 300},
            "comparator": {"kind": "keyed", "select": "/cells", "key": "grid", **HEIGHT_TOL},
            "tags": ["physics", "terrain", "M4"],
        }
        if w in HOLDOUT_WORLDS:
            s["holdout"] = True
        out.append(s)
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "layer4_phys.json"), "w", encoding="utf-8") as f:
        json.dump({"scenarios": scenarios()}, f, indent=1)
        f.write("\n")


if __name__ == "__main__":
    main()
