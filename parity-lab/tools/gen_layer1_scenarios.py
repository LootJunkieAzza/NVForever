#!/usr/bin/env python3
"""Generate scenarios/layer1*.json for the shipped FNV masters and archives.

Scenario files contain only file names and comparator settings (no game data), so
they are safe to commit. Re-run after changing the lists below.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
OUT = os.path.join(HERE, "..", "scenarios")

PLUGINS = ["FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm",
           "LonesomeRoad.esm", "GunRunnersArsenal.esm", "CaravanPack.esm", "ClassicPack.esm",
           "MercenaryPack.esm", "TribalPack.esm"]
# Archives small enough to have been captured in the cloud sandbox.
ARCHIVES = ["Update.bsa", "Fallout - Misc.bsa", "Fallout - Invalidation.bsa",
            "DeadMoney - Main.bsa", "DeadMoney - Sounds.bsa", "HonestHearts - Main.bsa",
            "HonestHearts - Sounds.bsa", "OldWorldBlues - Sounds.bsa", "LonesomeRoad - Sounds.bsa",
            "GunRunnersArsenal - Main.bsa", "GunRunnersArsenal - Sounds.bsa",
            "CaravanPack - Main.bsa", "ClassicPack - Main.bsa", "MercenaryPack - Main.bsa",
            "TribalPack - Main.bsa"]
# Archives > 400 MB: goldens must be captured on the user's machine.
LOCAL_ONLY_ARCHIVES = ["Fallout - Meshes.bsa", "Fallout - Textures.bsa", "Fallout - Textures2.bsa",
                       "Fallout - Sound.bsa", "Fallout - Voices1.bsa", "LonesomeRoad - Main.bsa",
                       "OldWorldBlues - Main.bsa"]
# Holdout subjects: EVERY scenario about these files is a holdout, so no visible
# scenario (header, stats, xEdit cross-check, ...) can mirror a hidden one.
HOLDOUT_SUBJECTS = {"LonesomeRoad.esm", "OldWorldBlues - Sounds.bsa"}
NON_CANONICAL = {"Fallout - Invalidation.bsa"}  # third-party generated; no byte-identity claim


def stem(name: str) -> str:
    return os.path.splitext(name)[0]


def plugin_scenarios():
    out = []
    for p in PLUGINS:
        s = stem(p)
        out.append({
            "id": f"L1.esm.inventory.{s}", "layer": 1,
            "description": f"Record inventory of {p}: every record's type, EditorID, FULL name and flags, keyed by file-local FormID.",
            "oracle": {"golden": f"esm/{s}.inventory.json"},
            "candidate": {"argv": ["{candidate}", "esm", "inventory", f"{{data_dir}}/{p}"]},
            "comparator": {"kind": "keyed", "select": "/records", "key": "formid",
                           "fields": ["type", "edid", "full", "flags", "form_version"]},
            "tags": ["esm", "inventory", "M1"]})
        out.append({
            "id": f"L1.esm.header.{s}", "layer": 1,
            "description": f"TES4 header of {p} (version, record count, masters, ONAM size).",
            "oracle": {"golden": f"esm/{s}.inventory.json"},
            "candidate": {"argv": ["{candidate}", "esm", "inventory", f"{{data_dir}}/{p}"]},
            "comparator": {"kind": "exact", "select": "/header"},
            "tags": ["esm", "header", "M1"]})
        out.append({
            "id": f"L1.esm.stats.{s}", "layer": 1,
            "description": f"Structural statistics of {p}: record/group counts by type, compression, HEDR self-check.",
            "oracle": {"golden": f"esm/{s}.stats.json"},
            "candidate": {"argv": ["{candidate}", "esm", "stats", f"{{data_dir}}/{p}"]},
            "comparator": {"kind": "exact"},
            "tags": ["esm", "stats", "M1"]})
        out.append({
            "id": f"L1.esm.roundtrip.{s}", "layer": 1,
            "description": f"Golden-file test: {p} re-serializes byte-identically; all records decompress and subrecords re-serialize.",
            "oracle": {"inline": {"roundtrip_identical": True, "decompress_errors": [],
                                  "subrecord_errors": [], "payload_roundtrip_mismatches": []}},
            "candidate": {"argv": ["{candidate}", "esm", "verify", f"{{data_dir}}/{p}"]},
            "comparator": {"kind": "exact", "subset": True},
            "tags": ["esm", "roundtrip", "M1"]})
    return out


def xedit_scenarios():
    """Cross-tool check against FNVEdit exports (tools/capture/xedit/export_inventory.pas).
    Goldens must be produced on the user's machine; until then these report errors."""
    out = []
    for p in PLUGINS:
        s = stem(p)
        out.append({
            "id": f"L1.esm.inventory_xedit.{s}", "layer": 1,
            "description": f"Record inventory of {p} against an FNVEdit (xEdit) export: type, EditorID, FULL name.",
            "oracle": {"golden": f"esm/{s}.xedit.inventory.json"},
            "candidate": {"argv": ["{candidate}", "esm", "inventory", f"{{data_dir}}/{p}"]},
            "comparator": {"kind": "keyed", "select": "/records", "key": "formid",
                           "fields": ["type", "edid", "full"]},
            "tags": ["esm", "inventory", "xedit", "M1", "needs-local-capture"]})
    return out


def archive_scenarios(names, local_only=False):
    out = []
    for a in names:
        s = stem(a)
        tags = ["bsa", "listing", "M1"] + (["needs-local-capture"] if local_only else [])
        out.append({
            "id": f"L1.bsa.listing.{s}", "layer": 1,
            "description": f"Listing of {a}: every file's path, stored size, compression, decoded size and SHA-256.",
            "oracle": {"golden": f"bsa/{a}.listing.json"},
            "candidate": {"argv": ["{candidate}", "bsa", "list", f"{{data_dir}}/{a}"]},
            "comparator": {"kind": "keyed", "select": "/files", "key": "path",
                           "fields": ["name_hash", "stored_size", "compressed", "size", "sha256"]},
            "tags": tags})
        out.append({
            "id": f"L1.bsa.header.{s}", "layer": 1,
            "description": f"Header fields of {a}.",
            "oracle": {"golden": f"bsa/{a}.listing.json"},
            "candidate": {"argv": ["{candidate}", "bsa", "list", f"{{data_dir}}/{a}", "--no-content-hash"]},
            "comparator": {"kind": "exact", "select": "/header"},
            "tags": tags[:1] + ["header"] + tags[2:]})
        if a not in NON_CANONICAL:
            out.append({
                "id": f"L1.bsa.roundtrip.{s}", "layer": 1,
                "description": f"Golden-file test: {a} rebuilds byte-identically and every file decodes.",
                "oracle": {"inline": {"roundtrip_identical": True, "decode_errors": []}},
                "candidate": {"argv": ["{candidate}", "bsa", "verify", f"{{data_dir}}/{a}"]},
                "comparator": {"kind": "exact", "subset": True},
                "tags": ["bsa", "roundtrip", "M1"] + (["needs-local-capture"] if local_only else [])})
    return out


def main():
    os.makedirs(OUT, exist_ok=True)
    groups = {
        "layer1_esm.json": plugin_scenarios(),
        "layer1_bsa.json": archive_scenarios(ARCHIVES),
        "layer1_bsa_local_only.json": archive_scenarios(LOCAL_ONLY_ARCHIVES, local_only=True),
        "layer1_xedit.json": xedit_scenarios(),
    }
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
