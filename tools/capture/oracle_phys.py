#!/usr/bin/env python3
"""Independent reference implementation ("oracle") of `fnvtool phys heights`.

Written only from docs/spec-notes/physics.md (§3 Terrain, §7 `phys heights`),
docs/spec-notes/world.md (§1 cells, §3 LAND, §5 field layouts), docs/spec-notes/esm.md and
docs/cli-contract.md, deliberately without looking at the Rust implementation, so the
parity lab can compare the physics terrain with an independent reader of the LAND data.
Standard library only; imports oracle_world.py (plugin walking, load-order FormID mapping,
cell / LAND selection, VHGT integration) and through it oracle_esm.py, so all three must
sit together.

What it does, in brief:

  1. Loads the plugins in load order with oracle_world's index (last definition wins,
     including the cell's LAND).
  2. Selects every exterior grid cell of the worldspace (the CELL records in its
     world-children group, minus the persistent cell placed directly in that group) that
     has a LAND record, keyed by its XCLC grid.
  3. Integrates the LAND's VHGT into the 33 x 33 height grid (world.md §3, x 8) and states
     the terrain surface analytically: each 128-unit grid square is split along its
     south-west -> north-east diagonal (physics.md §3, §7). The height under each of the
     eight fixed points of physics.md §7 is emitted; a LAND without VHGT gives null.

usage: oracle_phys.py heights <Data> <plugin>... --world <edid|0xFORMID> [-o out.json]
Plugins are given in load order. `--schema DIR` is accepted for command-line
compatibility with fnvtool and ignored. Exit codes: 0 ok, 1 input unreadable or
worldspace missing, 2 usage error.
"""
import argparse
import math
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_esm  # noqa: E402  (same directory)
import oracle_world  # noqa: E402  (same directory)

HEIGHTS_SCHEMA = "fnv.phys.heights/1"

# physics.md §7: points[i] = [u, v] in grid steps (128 units) east and north of the cell's
# south-west corner. The first four are grid vertices, the last four lie inside squares.
# SPEC-AMBIGUITY: the notes print the points as decimals; they are used as the nearest
# f64 values of those decimals (the candidate may hold them as f32; the difference moves
# a point by under 1e-6 grid steps, far inside the lab's 0.5-unit tolerance).
POINTS = (
    (5.0, 9.0), (27.0, 3.0), (13.0, 30.0), (30.0, 22.0),
    (8.2, 8.6), (24.6, 7.3), (7.7, 23.2), (23.9, 24.4),
)

CELL_UNITS = 4096.0
STEP_UNITS = 128.0
LAST = oracle_world.GRID - 1  # 32: index of the last vertex row / column


class Diagnostics(oracle_world.Diagnostics):
    """oracle_world's collector, printing under this script's name."""

    def emit(self, stream=None) -> None:
        stream = stream or sys.stderr
        for code in sorted(self.counts):
            for detail in self.examples.get(code, []):
                print(f"oracle_phys: {code}: {detail}", file=stream)
            extra = self.counts[code] - len(self.examples.get(code, []))
            if extra > 0:
                print(f"oracle_phys: {code}: ... and {extra} more", file=stream)


# --------------------------------------------------------------------------- surface

def height_at(h, u: float, v: float):
    """Terrain height at (u, v) grid steps from the cell's south-west corner.

    `h[row][col]` are the 33 x 33 vertex heights, rows south -> north (v), columns
    west -> east (u). Inside the square whose south-west vertex is (c, r), with
    fu = u - c and fv = v - r, the square is split along its south-west -> north-east
    diagonal (physics.md §7):
        fu >= fv (south-east triangle): h00 + fu (h10 - h00) + fv (h11 - h10)
        fu <  fv (north-west triangle): h00 + fv (h01 - h00) + fu (h11 - h01)
    At a vertex (fu = fv = 0) this is the vertex height exactly.
    """
    if not (0.0 <= u <= LAST and 0.0 <= v <= LAST):
        return None
    # SPEC-AMBIGUITY: the square holding a point is the one whose south-west vertex is
    # (floor(u), floor(v)); on the north / east edge (u or v = 32) the last square is used
    # (clamped to 31), so fu or fv is 1 there. None of the eight fixed points lies on
    # those edges, so this choice does not affect the output.
    c = min(int(math.floor(u)), LAST - 1)
    r = min(int(math.floor(v)), LAST - 1)
    fu = u - c
    fv = v - r
    h00 = h[r][c]
    h10 = h[r][c + 1]
    h01 = h[r + 1][c]
    h11 = h[r + 1][c + 1]
    if fu >= fv:
        z = h00 + fu * (h10 - h00) + fv * (h11 - h10)
    else:
        z = h00 + fv * (h01 - h00) + fu * (h11 - h01)
    # SPEC-AMBIGUITY: a non-finite result (only possible with a NaN / infinite VHGT
    # offset) is emitted as null, like oracle_world's other float fields.
    return oracle_world.finite(z)


# --------------------------------------------------------------------------- command

def world_edid(index, world):
    _, _, subs = index.subrecords(world, (b"EDID",))
    edid = subs.get(b"EDID")
    return None if edid is None else oracle_esm.zstring(edid)


def heights_document(index, world, load_order) -> dict:
    """fnv.phys.heights/1 for the worldspace `world` (load-order FormID)."""
    cells = []
    for gid in oracle_world.select_cells(index, world):
        _, direct = index.cells[gid]
        if direct:
            continue  # the persistent cell is not a grid cell (world.md §1)
        land = index.cell_land.get(gid)
        if land is None:
            continue  # only cells whose CELL has a LAND record
        _, _, subs = index.subrecords(gid, (b"XCLC",))
        xclc = subs.get(b"XCLC")
        if xclc is None or len(xclc) not in (8, 12):
            # SPEC-AMBIGUITY: a cell of the worldspace that has a LAND but no grid position
            # cannot be keyed by grid; it is skipped and reported (as oracle_world does).
            index.diag.add("cell_without_grid", f"cell 0x{gid:08X} has LAND but no XCLC; skipped")
            continue
        grid = list(oracle_world._XCLC.unpack_from(xclc))
        _, _, lsubs = index.subrecords(land[1], (b"VHGT",))
        vhgt = lsubs.get(b"VHGT")
        # world.md §5: a subrecord whose size does not match its layout is treated as
        # absent, so a wrong-size VHGT counts as "no VHGT" -> null.
        # SPEC-AMBIGUITY: a winning LAND record whose payload cannot be read (e.g. broken
        # compression) is likewise treated as having no VHGT (null) and reported by the
        # reader's diagnostics.
        if vhgt is None or len(vhgt) != oracle_world.VHGT_SIZE:
            heights = None
        else:
            h = oracle_world.integrate_vhgt(vhgt)
            heights = [height_at(h, u, v) for (u, v) in POINTS]
        cells.append({"grid": grid, "heights": heights})
    cells.sort(key=lambda c: (c["grid"][0], c["grid"][1]))
    seen = set()
    for c in cells:
        key = tuple(c["grid"])
        if key in seen:
            # SPEC-AMBIGUITY: the notes observe one cell per grid square; should two cells
            # share one, both entries are kept (in FormID order) and reported.
            index.diag.add("duplicate_grid", f"world 0x{world:08X}: grid {list(key)} appears twice")
        seen.add(key)
    return {
        "schema": HEIGHTS_SCHEMA,
        "world": oracle_world.fid_text(world),
        "edid": world_edid(index, world),
        "points": [[float(u), float(v)] for (u, v) in POINTS],
        "cells": cells,
        "load_order": list(load_order),
    }


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Independent oracle for fnvtool phys heights.")
    sub = parser.add_subparsers(dest="command", required=True)
    p_heights = sub.add_parser("heights", help=f"emit {HEIGHTS_SCHEMA}")
    p_heights.add_argument("data_dir")
    p_heights.add_argument("plugins", nargs="+")
    p_heights.add_argument("--world", required=True)
    p_heights.add_argument("-o", "--output", help="write JSON here instead of stdout")
    p_heights.add_argument("--schema", help="accepted for fnvtool compatibility; ignored")
    args = parser.parse_args(argv)

    diag = Diagnostics()
    index, error = oracle_world.load(args.data_dir, args.plugins, diag)
    if index is None:
        diag.emit()
        print(f"oracle_phys: {error}", file=sys.stderr)
        return 1
    try:
        world, error = index.resolve_world(args.world)
        if world is None:
            diag.emit()
            # SPEC-AMBIGUITY: an unknown worldspace is exit code 1 (as oracle_world).
            print(f"oracle_phys: {error}", file=sys.stderr)
            return 1
        doc = heights_document(index, world, args.plugins)
    finally:
        index.close()
    text = oracle_world.render(doc)
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    diag.emit()
    return 0


if __name__ == "__main__":
    sys.exit(main())
