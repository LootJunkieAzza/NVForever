#!/usr/bin/env python3
"""Generate scenarios/layer3_world.json (worldspaces, cells, references, terrain heights).

Oracle goldens come from tools/capture/oracle_world.py, written from
docs/spec-notes/{esm,world}.md and docs/cli-contract.md only. Scenario files contain only
plugin names, worldspace Editor IDs and comparator settings (no game data).

Capture (once, into the private goldens directory):
  python tools/capture/oracle_world.py refs <Data> <plugins...> --world <W> -o <goldens>/world/<W>.refs.json
  python tools/capture/oracle_world.py refs <Data> <plugins...> --interiors -o <goldens>/world/interiors.refs.json
  python tools/capture/oracle_world.py heights <Data> <plugins...> --world <W> -o <goldens>/world/<W>.heights.json
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "scenarios")

LOAD_ORDER = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
              "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm"]

# Worldspaces with references (refs scenarios) and with landscape (heights scenarios),
# from `fnvtool world list` on the shipped masters. Only Editor IDs are listed here.
REF_WORLDS = [
    "FFEncounterWorld", "TestMap01", "WastelandNV", "FreesideWorld", "GamorrahWorld", "GreenhouseWorld01",
    "FreesideNorthWorld", "FreesideFortWorld", "TheStripWorldNew", "TheFortWorld", "WastelandNVmini",
    "BoulderCityWorld", "Lucky38World", "NVDLC01Villa", "NVDLC01WestTownN", "NVDLC01EastTownS",
    "NVDLC01WestTownS", "NVDLC01VillaDean", "NVDLC01VillaChristine", "NVDLC01EastTownN", "NVDLC02ZionCanyon",
    "NVDLC03BigMT", "NVDLC03Higgs", "NVDLC04Road01World", "NVDLC04DivideWorld", "NVDLC04Road02World",
    "NVDLC04DivideVistaWorld", "NVDLC04NukeNCR", "NVDLC04NukeLegion", "NVDLC04NukeSilo2",
]
LAND_WORLDS = [w for w in REF_WORLDS if w != "FFEncounterWorld"] + ["NVDLC01TestMap01"]
# Honest Hearts content is the holdout, as in Layer 2.
HOLDOUT_WORLDS = {"NVDLC02ZionCanyon"}
# f32 values widened exactly on both sides; only the mean (a sum of 1089 values) may
# differ in the last bits with summation order.
EXACT = {"abs_tol": 0.0, "rel_tol": 0.0}
MEAN_TOL = {"abs_tol": 1e-3, "rel_tol": 0.0}


def base_argv(cmd):
    return ["{candidate}", "world", cmd, "{data_dir}", *LOAD_ORDER, "--schema", "{schema_dir}"]


def refs_pair(sid, label, golden, extra, holdout):
    argv = base_argv("refs") + extra
    out = [
        {"id": f"L3.world.refs.{sid}", "layer": 3,
         "description": f"Every reference placed in {label}: cell, base object and model, placement, flags, enable parent.",
         "oracle": {"golden": golden}, "candidate": {"argv": argv, "timeout_s": 300},
         "comparator": {"kind": "keyed", "select": "/refs", "key": "formid", **EXACT},
         "tags": ["world", "refs", "M3"]},
        {"id": f"L3.world.cells.{sid}", "layer": 3,
         "description": f"Cells of {label}: Editor ID, name, worldspace, grid, flags, water height, reference count, LAND.",
         "oracle": {"golden": golden}, "candidate": {"argv": argv, "timeout_s": 300},
         "comparator": {"kind": "keyed", "select": "/cells", "key": "formid", **EXACT},
         "tags": ["world", "cells", "M3"]},
    ]
    if holdout:
        for s in out:
            s["holdout"] = True
    return out


def scenarios():
    out = []
    for w in REF_WORLDS:
        out += refs_pair(w, f"worldspace {w}", f"world/{w}.refs.json", ["--world", w], w in HOLDOUT_WORLDS)
    out += refs_pair("interiors", "every interior cell", "world/interiors.refs.json", ["--interiors"], False)
    for w in LAND_WORLDS:
        s = {"id": f"L3.world.heights.{w}", "layer": 3,
             "description": f"Terrain heights of every landscape cell in {w}: min, max, mean, corners, centre.",
             "oracle": {"golden": f"world/{w}.heights.json"},
             "candidate": {"argv": base_argv("heights") + ["--world", w], "timeout_s": 300},
             "comparator": {"kind": "keyed", "select": "/cells", "key": "grid", **MEAN_TOL},
             "tags": ["world", "terrain", "M3"]}
        if w in HOLDOUT_WORLDS:
            s["holdout"] = True
        out.append(s)
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    items = scenarios()
    with open(os.path.join(OUT, "layer3_world.json"), "w", encoding="utf-8") as f:
        json.dump({"scenarios": items}, f, indent=1)
        f.write("\n")
    print(f"layer3_world.json: {len(items)} scenarios")


if __name__ == "__main__":
    main()
