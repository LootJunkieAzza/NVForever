#!/usr/bin/env python3
"""Generate scenarios/layer4_collide.json (collision geometry of every mesh archive).

Oracle goldens come from tools/capture/oracle_collide.py, written from
docs/spec-notes/collision.md (and nif.xml as documentation) only. Scenario files contain
only archive names and comparator settings (no game data).
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "scenarios")

MESH_ARCHIVES = ["Fallout - Meshes.bsa", "Update.bsa", "DeadMoney - Main.bsa", "HonestHearts - Main.bsa",
                 "OldWorldBlues - Main.bsa", "LonesomeRoad - Main.bsa", "GunRunnersArsenal - Main.bsa",
                 "CaravanPack - Main.bsa", "ClassicPack - Main.bsa", "MercenaryPack - Main.bsa",
                 "TribalPack - Main.bsa"]
# Same holdout subject as Layer 2.
HOLDOUT_SUBJECTS = {"HonestHearts - Main.bsa"}
# Bounds are composed through several transforms; the two implementations may round
# differently (f32 vs f64) on coordinates of a few thousand units. 0.01 game units is far
# below anything visible or physically meaningful. Set before the first comparison.
FLOAT_TOL = {"abs_tol": 0.01, "rel_tol": 1e-5}


def stem(name: str) -> str:
    return os.path.splitext(name)[0]


def scenarios():
    out = []
    for a in MESH_ARCHIVES:
        s = stem(a)
        argv = ["{candidate}", "collide", "objects", f"{{data_dir}}/{a}", "--schema", "{nif_schema_dir}"]
        golden = f"collide/{a}.collide.objects.json"
        out.append({
            "id": f"L4.collide.objects.{s}", "layer": 4,
            "description": f"Every solid collision object of every mesh in {a}: body, motion, layer, physical constants, leaf shapes and counts, and bounds in mesh-root game units.",
            "oracle": {"golden": golden},
            "candidate": {"argv": argv},
            "comparator": {"kind": "keyed", "select": "/objects", "key": "key", **FLOAT_TOL},
            "tags": ["collision", "M4"]})
        out.append({
            "id": f"L4.collide.phantoms.{s}", "layer": 4,
            "description": f"Phantom (trigger) bodies in {a}.",
            "oracle": {"golden": golden},
            "candidate": {"argv": argv},
            "comparator": {"kind": "exact", "select": "/phantoms"},
            "tags": ["collision", "M4"]})
        out.append({
            "id": f"L4.collide.skipped.{s}", "layer": 4,
            "description": f"Meshes in {a} whose collision cannot be followed.",
            "oracle": {"golden": golden},
            "candidate": {"argv": argv},
            "comparator": {"kind": "exact", "select": "/skipped"},
            "tags": ["collision", "M4"]})
    for it in out:
        if any(f"{{data_dir}}/{subj}" in it["candidate"]["argv"] for subj in HOLDOUT_SUBJECTS):
            it["holdout"] = True
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    with open(os.path.join(OUT, "layer4_collide.json"), "w", encoding="utf-8") as f:
        json.dump({"scenarios": scenarios()}, f, indent=1)
        f.write("\n")


if __name__ == "__main__":
    main()
