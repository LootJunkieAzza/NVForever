#!/usr/bin/env python3
"""Deliberately buggy fake candidate used only by the lab's self-tests.

Each mode prints JSON in the same schema as a golden, with known defects, so the
self-tests can check that the harness detects passes, failures and errors.
"""
import json
import sys

RECORDS = [
    {"formid": "0x00000001", "type": "WEAP", "edid": "Varmint", "flags": 0},
    {"formid": "0x00000002", "type": "NPC_", "edid": "DocMitchell", "flags": 0},
    {"formid": "0x00000003", "type": "QUST", "edid": "VMS01", "flags": 0},
    {"formid": "0x00000004", "type": "CELL", "edid": "Goodsprings", "flags": 1024},
]


def main(mode: str) -> int:
    if mode == "inventory_ok":
        out = {"plugin": "Synthetic.esm", "records": RECORDS}
    elif mode == "inventory_buggy":
        recs = [dict(r) for r in RECORDS]
        recs[1]["edid"] = "DocMitchel"          # changed field
        del recs[3]                              # missing record
        recs.append({"formid": "0x00000099", "type": "MISC", "edid": "Extra", "flags": 0})  # extra
        out = {"plugin": "Synthetic.esm", "records": recs}
    elif mode == "mesh_within_tol":
        out = {"blocks": {"NiNode": 3, "NiTriShape": 2}, "vertices": 120, "bounds": [0.0, 1.0000004, 2.5]}
    elif mode == "mesh_out_of_tol":
        out = {"blocks": {"NiNode": 3, "NiTriShape": 1}, "vertices": 120, "bounds": [0.0, 1.1, 2.5]}
    elif mode == "trace":
        out = {"events": [
            {"t": 0, "event": "SetStage", "quest": "VMS01", "stage": 10},
            {"t": 1, "event": "SetStage", "quest": "VMS01", "stage": 20},
            {"t": 2, "event": "AddItem", "item": "Varmint", "count": 1},
            {"t": 3, "event": "SetStage", "quest": "VMS01", "stage": 25},   # golden says 30
            {"t": 4, "event": "SetStage", "quest": "VMS01", "stage": 40},
        ]}
    elif mode == "crash":
        print("simulated candidate crash", file=sys.stderr)
        return 1
    elif mode == "garbage":
        sys.stdout.write("this is not json")
        return 0
    else:
        print(f"unknown mode {mode}", file=sys.stderr)
        return 2
    json.dump(out, sys.stdout)
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1]))
