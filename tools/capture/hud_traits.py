#!/usr/bin/env python3
"""Generate a private HUD trait job from fnvtool's fnv.ui.tree/1 JSON.

[design] Reads a tree file only; never runs fnvtool or the game. Only tile and
trait names from that file enter the job, not values, expressions or warnings.
"""
import argparse
import json
import math
from pathlib import Path
import sys


DEFAULT_TILES = ("HitPoints", "ActionPoints", "SneakMeter", "ReticleCenter",
                 "EnemyHealth", "Messages", "Subtitles", "Info", "QuestReminder")
DEFAULT_TRAITS = ("x", "y", "width", "height", "alpha", "visible", "depth")
READERS = ("GetUIFloat", "GetUIFloatAlt")
# [documented: GECK wiki snapshot 2026-10-28, GetUIFloatInherited rev 41244 (xNVSE
# 6.4.8, ExcludeMenuRoot 6.4.9)] Relative values for x, y and
# depth only, through every parent with an active locus; with ExcludeMenuRoot 1, relative
# to the menu's own root. nvf_capture needs xNVSE 6.4.9, which has both.
INHERITED_TRAITS = ("x", "y", "depth")
INHERITED_READERS = ("GetUIFloatInherited", "GetUIFloatInherited:menu")
# [documented: the same page's examples] This depth reads -50 from the top-level root and
# 0 from HUDMainMenu's: a check of the reads themselves, when the tree has the tile.
INHERITED_CONTROL = "HUDMainMenu/RadiationMeter/radiation_text/depth"
# [design] No per-sample probe limit is stated in nvf_capture/README.md. Split
# wide tiles into groups of at most 64 trait reads plus a presence probe.
GROUP_READS = 64


class InputError(ValueError):
    pass


def name_text(value):
    """[design] Refuse script controls and capture/GECK percent substitutions."""
    if not isinstance(value, str) or not value or any(ord(c) < 32 or ord(c) == 127 for c in value):
        raise InputError("tile and trait names must be non-empty text without control characters")
    if "%" in value:
        raise InputError("percent substitutions are not allowed in tile or trait names")
    return value


def script_path(path):
    # [documented: menu-xml.md section 5] Both slashes separate UI path segments.
    # [documented: GECK wiki, String Formatting] %q quotes inside a string literal.
    return '"' + path.replace("\\", "/").replace('"', "%q") + '"'


def number_script(expression):
    # [design] Full scripts avoid the short form's @name substitution in names.
    return ("float fV\nBegin Function { }\nlet fV := " + expression
            + "\nSetFunctionValue fV\nEnd\n")


def read_script(reader, path):
    name, _, root = reader.partition(":")
    return number_script(f"{name} {script_path(path)}" + (" 1" if root == "menu" else ""))


def readers_for(trait, base, inherited):
    extra = INHERITED_READERS if inherited and trait.casefold() in INHERITED_TRAITS else ()
    return base + extra


def is_numeric(trait):
    # [documented: crates/fnvtool/src/ui_cmd.rs] Current exports hold evaluated
    # scalars, without expr metadata. Accept explicit value/expr entries too.
    if isinstance(trait, dict):
        if trait.get("expr") is not None:
            return True
        trait = trait.get("value")
    return type(trait) in (int, float) and math.isfinite(trait)


def list_tiles(doc):
    """Return (path, traits) for each live tile; template names are not instances."""
    if not isinstance(doc, dict) or doc.get("schema") != "fnv.ui.tree/1":
        raise InputError("expected fnv.ui.tree/1 JSON")
    root = doc.get("tree")
    if not isinstance(root, dict) or root.get("name") != "HUDMainMenu" or doc.get("path") != "HUDMainMenu":
        raise InputError("export the full HUDMainMenu tree, without --at")
    tiles = []

    def visit(node, path):
        if not isinstance(node, dict) or not isinstance(node.get("traits"), dict) or not isinstance(node.get("children"), list):
            raise InputError("each tile must have traits and children")
        for trait in node["traits"]:
            name_text(trait)
        tiles.append((path, node["traits"]))
        children = node["children"]
        for i, child in enumerate(children):
            if not isinstance(child, dict):
                raise InputError("each child must be a tile")
            name = child.get("name")
            if name == "":
                segment = f"*:{i}"
            else:
                segment = name_text(name).replace("\\", "/")
                # [design] Index ambiguous names by absolute child position.
                if any(j != i and isinstance(c, dict) and isinstance(c.get("name"), str)
                       and c["name"].casefold() == name.casefold() for j, c in enumerate(children)):
                    segment = f"*:{i}"
            visit(child, f"{path}/{segment}")

    visit(root, "HUDMainMenu")
    return tiles


def make_job(doc, *, name="hud_traits", start="Load a settled save with the HUD visible.",
             tiles=DEFAULT_TILES, children=12, traits=DEFAULT_TRAITS, inherited=True,
             every=30, max_samples=120, settle_frames=300, shots=3, shot_gap=30):
    for key, value, minimum in (("children", children, 0), ("every", every, 1),
                                ("max_samples", max_samples, 1), ("settle_frames", settle_frames, 1),
                                ("shots", shots, 0), ("shot_gap", shot_gap, 1)):
        if type(value) is not int or not minimum <= value <= 4294967295:
            raise InputError(f"{key} must be an integer in {minimum}..4294967295")
    if not name or not all(c.isascii() and (c.isalnum() or c in "_.-") for c in name):
        raise InputError("job name must be non-empty [A-Za-z0-9_.-]")
    live = list_tiles(doc)
    groups, group_fields, tile_plan = [], {}, []
    reads = 0

    def add_groups(kind, path, queries, count):
        nonlocal reads
        for offset in range(0, len(queries), GROUP_READS):
            part = queries[offset:offset + GROUP_READS]
            group_name = f"{kind}_{len(groups):04}"
            # [documented: nvf_capture/README.md, Observing] A zero first probe
            # drops the subject. A constant keeps zeros and missing (-999) reads.
            groups.append({"name": group_name, "count": number_script(str(count)), "max": count,
                           "probe": [number_script("1")] + [read_script(r, p) for r, p in part]})
            group_fields[group_name] = {"tile": path, "count": count,
                                        "probe": ["present"] + [{"reader": r, "path": p} for r, p in part]}
            reads += count * len(part)

    for path, values in live:
        selected = [t for t, v in values.items() if is_numeric(v)]
        queries = [(reader, f"{path}/{trait}") for trait in selected for reader in readers_for(trait, READERS, inherited)]
        add_groups("file", path, queries, 1)
        tile_plan.append({"path": path, "traits": list(values), "numeric_traits": selected,
                          "file_probes": len(queries), "child_probes": 0})

    traits = list(dict.fromkeys(name_text(t) for t in traits))
    dynamic_tiles = list(dict.fromkeys(name_text(t).replace("\\", "/").strip("/") for t in tiles))
    if any(not t for t in dynamic_tiles):
        raise InputError("child tile paths must not be empty")
    for tile in dynamic_tiles:
        path = tile if tile.startswith("HUDMainMenu/") else f"HUDMainMenu/{tile}"
        # [documented: GECK wiki, GetUIFloatAlt notes] * matches any child;
        # :N picks its index. The observer expands %{i} to 0..count-1.
        queries = [(reader, f"{path}/*:%{{i}}/{trait}") for trait in traits
                   for reader in readers_for(trait, ("GetUIFloatAlt",), inherited)]
        if children:
            add_groups("children", path, queries, children)
        row = next((t for t in tile_plan if t["path"] == path), None)
        if row is None:
            row = {"path": path, "traits": [], "numeric_traits": [], "file_probes": 0, "child_probes": 0}
            tile_plan.append(row)
        row["child_probes"] += children * len(queries)

    known = next((f"{path}/{t}" for path, values in live for t, v in values.items() if is_numeric(v)), None)
    if known is None:
        raise InputError("the tree needs a numeric or expression trait for the existing-path control")
    # [design] An unregistered menu name keeps the negative control outside the
    # HUD subtree, including any children the game creates at run time.
    missing = "__nvf_hud_traits_no_such_menu__/x"
    controls = [(reader, path) for path in (known, missing) for reader in READERS]
    header_names = ["player.health", "player.action_points", "player.heading"]
    header = [number_script(e) for e in ("player.GetAV Health", "player.GetAV ActionPoints", "player.GetAngle Z")]
    header_names += [{"reader": r, "path": p, "control": "exists" if p == known else "missing",
                      "expected": "not -999" if p == known else -999} for r, p in controls]
    header += [read_script(r, p) for r, p in controls]
    tile_paths = {path.casefold() for path, _ in live}
    if inherited and INHERITED_CONTROL.rsplit("/", 1)[0].casefold() in tile_paths:
        for reader, expected in zip(INHERITED_READERS, (-50, 0)):
            controls.append((reader, INHERITED_CONTROL))
            header_names.append({"reader": reader, "path": INHERITED_CONTROL, "control": "documented", "expected": expected})
            header.append(read_script(reader, INHERITED_CONTROL))
    presence = sum(g["max"] for g in groups)
    counts = {"trait_reads": reads, "control_reads": len(controls), "header_values": len(header),
              "presence_probes": presence, "count_queries": len(groups), "groups": len(groups),
              "max_calls_per_sample": len(header) + reads + presence + len(groups) + bool(shots)}
    note = (f"[design] {reads} trait reads + {len(controls)} controls per sample; "
            f"{len(groups)} groups, at most {GROUP_READS} trait reads per subject per group. "
            "No per-sample probe limit is stated in nvf_capture/README.md.")
    fields = {"comment": note, "header": header_names, "groups": group_fields,
              "tiles": tile_plan, "counts": counts,
              "clock": {"sample": "n (zero-based, outside header)", "relative_loops": f"n * {every}"},
              "screen": "[documented: GECK wiki snapshot 2024-09-23, GetScreenTrait] SUP only; no baseline xNVSE screen-size query found; omitted.",
              "shots": {"samples": f"0, {shot_gap}, ...", "max": shots},
              "input": "[documented: crates/fnvtool/src/ui_cmd.rs] Evaluated scalars; expression metadata is not exported."}
    if inherited:
        fields["inherited"] = ("[documented: GECK wiki snapshot 2026-10-28, GetUIFloatInherited rev 41244] "
                                "x, y and depth from the top-level root and, with ExcludeMenuRoot 1, from the menu's root "
                                "(reader GetUIFloatInherited:menu); xNVSE 6.4.8, the flag 6.4.9.")
    obs = {"label": "hud_traits", "every": every, "max_samples": max_samples,
           "header": header, "groups": groups, "fields": fields}
    if shots:
        obs["shots"] = {"label": "hud", "when": number_script("1"), "gap": shot_gap, "max": shots}
    # [design] Wait for gameplay, then use the observer's 300-loop settle pause.
    wait = "Begin Function { }\nif MenuMode == 0\n  SetFunctionValue 1\nelse\n  SetFunctionValue 0\nendif\nEnd\n"
    return {"format": "fnv.capture.steps/1", "name": name, "title": "HUDMainMenu live trait capture",
            "start": start, "steps": [{"until": wait, "max_frames": 3600},
                                      {"frames": settle_frames}, {"observe": obs}]}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("tree", type=Path, help="private full HUDMainMenu fnv.ui.tree/1 JSON file")
    parser.add_argument("--out", "-o", type=Path, default=Path("hud_traits.steps.json"),
                        help="private output path (default: hud_traits.steps.json); basename must be NAME.steps.json")
    parser.add_argument("--start", default="Load a settled save with the HUD visible.", help="starting save/location instructions")
    parser.add_argument("--list", action="store_true", help="print paths, trait names and probe counts without writing a job")
    parser.add_argument("--tiles", nargs="+", default=DEFAULT_TILES, help="tiles whose runtime children are probed (relative to HUDMainMenu)")
    parser.add_argument("--children", type=int, default=12, help="child indices 0..K-1 (default: 12; 0 disables)")
    parser.add_argument("--traits", nargs="+", default=DEFAULT_TRAITS, help="runtime child traits (default: x y width height alpha visible depth)")
    parser.add_argument("--no-inherited", dest="inherited", action="store_false", help="leave out GetUIFloatInherited's x, y and depth reads (xNVSE 6.4.8+; ExcludeMenuRoot 6.4.9+)")
    parser.add_argument("--every", type=int, default=30, help="loops between samples (default: 30)")
    parser.add_argument("--max-samples", type=int, default=120, help="sample count (default: 120)")
    parser.add_argument("--settle-frames", type=int, default=300, help="settle loops after gameplay resumes (default: 300)")
    parser.add_argument("--shots", type=int, default=3, help="maximum HUD-visible screenshots (default: 3; 0 disables)")
    parser.add_argument("--shot-gap", type=int, default=30, help="take shots at samples 0, gap, 2*gap, ... (default: 30)")
    args = parser.parse_args(argv)
    try:
        # [design] Binary JSON loading also accepts PowerShell's UTF-16 exports.
        with args.tree.open("rb") as stream:
            doc = json.load(stream)
        name = "hud_traits"
        if not args.list:
            if not args.out.name.endswith(".steps.json"):
                raise InputError("output basename must be NAME.steps.json")
            name = args.out.name[:-len(".steps.json")]
        job = make_job(doc, name=name, **{key: getattr(args, key) for key in
                       ("start", "tiles", "children", "traits", "inherited", "every", "max_samples", "settle_frames", "shots", "shot_gap")})
        fields = job["steps"][-1]["observe"]["fields"]
        if args.list:
            print(json.dumps({k: fields[k] for k in ("tiles", "counts", "comment", "clock", "screen", "input")}, indent=2))
        else:
            with args.out.open("w", encoding="utf-8") as stream:
                json.dump(job, stream, indent=1)
                stream.write("\n")
            print(f"{args.out}: {fields['comment']}")
        return 0
    except (OSError, ValueError) as exc:
        print(f"hud_traits: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
