#!/usr/bin/env python3
"""Generate scenarios/layer2*.json (meshes, animations, textures).

Oracle goldens come from tools/capture/oracle_nif.py and oracle_dds.py, which were
written from docs/spec-notes/{nif,dds}.md and docs/cli-contract.md only. Scenario files
contain only archive names and comparator settings (no game data).
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "scenarios")

MESH_ARCHIVES = ["Fallout - Meshes.bsa", "Update.bsa", "DeadMoney - Main.bsa", "HonestHearts - Main.bsa",
                 "OldWorldBlues - Main.bsa", "LonesomeRoad - Main.bsa", "GunRunnersArsenal - Main.bsa",
                 "CaravanPack - Main.bsa", "ClassicPack - Main.bsa", "MercenaryPack - Main.bsa",
                 "TribalPack - Main.bsa"]
TEXTURE_ARCHIVES = ["Fallout - Textures.bsa", "Fallout - Textures2.bsa", "Update.bsa", "DeadMoney - Main.bsa",
                    "HonestHearts - Main.bsa", "OldWorldBlues - Main.bsa", "LonesomeRoad - Main.bsa",
                    "GunRunnersArsenal - Main.bsa", "CaravanPack - Main.bsa", "ClassicPack - Main.bsa",
                    "MercenaryPack - Main.bsa", "TribalPack - Main.bsa"]
# Every Layer 2 scenario about this archive is a holdout (meshes and textures alike).
HOLDOUT_SUBJECTS = {"HonestHearts - Main.bsa"}
# f32 values widened exactly on both sides; a tiny tolerance only absorbs -0.0 / 0.0.
FLOAT_TOL = {"abs_tol": 0.0, "rel_tol": 0.0}


def stem(name: str) -> str:
    return os.path.splitext(name)[0]


def nif_scenarios():
    out = []
    for a in MESH_ARCHIVES:
        s = stem(a)
        scene = ["{candidate}", "nif", "scene", f"{{data_dir}}/{a}", "--schema", "{nif_schema_dir}"]
        out.append({
            "id": f"L2.nif.stats.{s}", "layer": 2,
            "description": f"Header summary of every NIF/KF in {a}: version, stream version, block and string counts, block types.",
            "oracle": {"golden": f"nif/{a}.nif.stats.json"},
            "candidate": {"argv": ["{candidate}", "nif", "stats", f"{{data_dir}}/{a}"]},
            "comparator": {"kind": "keyed", "select": "/files", "key": "path",
                           "fields": ["version", "user_version", "bs_version", "num_blocks", "strings",
                                      "block_types", "error"]},
            "tags": ["nif", "stats", "M2"]})
        out.append({
            "id": f"L2.nif.objects.{s}", "layer": 2,
            "description": f"Scene graph of every mesh in {a}: node and shape types, names, parents, local transforms.",
            "oracle": {"golden": f"nif/{a}.nif.scene.json"},
            "candidate": {"argv": scene},
            "comparator": {"kind": "keyed", "select": "/objects", "key": "key", **FLOAT_TOL},
            "tags": ["nif", "scene", "M2"]})
        out.append({
            "id": f"L2.nif.shapes.{s}", "layer": 2,
            "description": f"Geometry of every mesh in {a}: vertex and triangle counts, stored streams, bounding sphere, vertex bounds.",
            "oracle": {"golden": f"nif/{a}.nif.scene.json"},
            "candidate": {"argv": scene},
            "comparator": {"kind": "keyed", "select": "/shapes", "key": "key", **FLOAT_TOL},
            "tags": ["nif", "geometry", "M2"]})
        out.append({
            "id": f"L2.nif.skipped.{s}", "layer": 2,
            "description": f"Files in {a} the scene analysis skips (no block-size table, or undecodable).",
            "oracle": {"golden": f"nif/{a}.nif.scene.json"},
            "candidate": {"argv": scene},
            "comparator": {"kind": "exact", "select": "/skipped"},
            "tags": ["nif", "scene", "M2"]})
    return out


def dds_scenarios():
    out = []
    for a in TEXTURE_ARCHIVES:
        s = stem(a)
        out.append({
            "id": f"L2.dds.listing.{s}", "layer": 2,
            "description": f"Every texture in {a}: dimensions, format, mip levels, faces and payload size vs layout.",
            "oracle": {"golden": f"dds/{a}.dds.listing.json"},
            "candidate": {"argv": ["{candidate}", "dds", "list", f"{{data_dir}}/{a}"]},
            "comparator": {"kind": "keyed", "select": "/files", "key": "path"},
            "tags": ["dds", "M2"]})
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    groups = {"layer2_nif.json": nif_scenarios(), "layer2_dds.json": dds_scenarios()}
    for fn, items in groups.items():
        for it in items:
            if any(f"{{data_dir}}/{subj}" in it["candidate"]["argv"] for subj in HOLDOUT_SUBJECTS):
                it["holdout"] = True
        with open(os.path.join(OUT, fn), "w", encoding="utf-8") as f:
            json.dump({"scenarios": items}, f, indent=1)
            f.write("\n")
        print(f"{fn}: {len(items)} scenarios")


if __name__ == "__main__":
    main()
