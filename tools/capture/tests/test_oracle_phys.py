"""Tests for oracle_phys.py (`heights`) using small synthetic plugins.

Two plugins in load order: Base.esm (a worldspace with a persistent cell, landscape cells
with designed heights, a LAND without VHGT, a LAND with a wrong-size VHGT, a cell without
LAND, and a second worldspace without an Editor ID) and Dep.esm (master Base.esm) which
overrides one cell's LAND. Heights are chosen so that the south-west -> north-east split
gives different values from the other diagonal and from bilinear interpolation.
No game data is used. Run from the repo root:

    python3 -m unittest discover -s tools/capture/tests -v
"""
import io
import json
import os
import struct
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr

TESTS_DIR = os.path.dirname(os.path.abspath(__file__))
CAPTURE_DIR = os.path.dirname(TESTS_DIR)
sys.path.insert(0, CAPTURE_DIR)
sys.path.insert(0, TESTS_DIR)

import oracle_phys  # noqa: E402
import test_oracle_world as tw  # noqa: E402  (synthetic plugin builders)

POINTS = [[5.0, 9.0], [27.0, 3.0], [13.0, 30.0], [30.0, 22.0],
          [8.2, 8.6], [24.6, 7.3], [7.7, 23.2], [23.9, 24.4]]


def vhgt_from(grid_steps, offset=0.0):
    """VHGT whose integrated heights are 8 * (offset + grid_steps[row][col])."""
    steps = []
    for r in range(33):
        for c in range(33):
            if c == 0:
                steps.append(grid_steps[r][0] - (grid_steps[r - 1][0] if r else 0))
            else:
                steps.append(grid_steps[r][c] - grid_steps[r][c - 1])
    return struct.pack("<f", offset) + struct.pack("<1089b", *steps) + b"\0\0\0"


def designed_grid():
    """Flat grid (0) with bumps (in 8-unit steps, h[row][col], rows south -> north)."""
    g = [[0] * 33 for _ in range(33)]
    # Vertex points [u, v] -> h[v][u].
    g[9][5] = 3
    g[3][27] = -4
    g[30][13] = 7
    g[22][30] = -2
    # [8.2, 8.6] (fu 0.2 < fv 0.6, north-west triangle) in square (c 8, r 8):
    # h10 (east) = 80, h01 (north) = 40, h00 = h11 = 0.
    g[8][9] = 10
    g[9][8] = 5
    # [24.6, 7.3] (fu 0.6 >= fv 0.3, south-east triangle) in square (c 24, r 7): h10 = 80.
    g[7][25] = 10
    # [7.7, 23.2] (south-east) in square (c 7, r 23): h11 (north-east) = 48.
    g[24][8] = 6
    # [23.9, 24.4] (south-east) in square (c 23, r 24): h01 (north) = 32, unused by that
    # triangle.
    g[25][23] = 4
    return g


OFFSET = 100.0
BASE_Z = 8.0 * OFFSET
# Expected heights at POINTS for designed_grid() with OFFSET (worked out by hand from the
# triangle formulas of physics.md §7).
DESIGNED = [
    BASE_Z + 24.0,               # vertex (5, 9)
    BASE_Z - 32.0,               # vertex (27, 3)
    BASE_Z + 56.0,               # vertex (13, 30)
    BASE_Z - 16.0,               # vertex (30, 22)
    BASE_Z + 0.6 * 40.0 - 0.2 * 40.0,   # NW: h00 + fv (h01 - h00) + fu (h11 - h01) = 16
    BASE_Z + 0.6 * 80.0 - 0.3 * 80.0,   # SE: fu h10 + fv (h11 - h10) = 24
    BASE_Z + 0.2 * 48.0,         # SE: fv (h11 - h10) = 9.6
    BASE_Z,                      # SE: h01 not used
]
# What the other split (north-west -> south-east diagonal) would give at the four interior
# points; each differs from DESIGNED by more than the lab tolerance (0.5).
OTHER_DIAGONAL = [
    BASE_Z + 0.2 * 80.0 + 0.6 * 40.0,   # fu + fv < 1: SW/SE/NW triangle = 40
    BASE_Z + 0.6 * 80.0,                # SW/SE/NW triangle = 48
    BASE_Z,                             # SW/SE/NW triangle: all corners 0
    BASE_Z + 0.1 * 32.0,                # fu + fv > 1: NE/SE/NW triangle = 3.2
]


def ramp_heights(offset):
    """Expected heights for test_oracle_world.ramp_vhgt: 8 * (offset + 2 v + u)."""
    return [8.0 * (offset + 2.0 * v + u) for (u, v) in POINTS]


def build_base():
    s, rec, grp, blk, cell = tw.sub, tw.record, tw.group, tw.block, tw.cell
    world = grp(
        b"WRLD", 0,
        rec(b"WRLD", 0x100, s(b"EDID", tw.zstr("PhysWorld"))),
        grp(0x100, 1,
            cell(0x200, grid=(0, 0)),                                      # persistent cell
            tw.children(0x200, temporary=[rec(b"LAND", 0x5FF, s(b"VHGT", tw.ramp_vhgt(1.0)))]),
            blk(0, 0,
                cell(0x201, edid="Designed", grid=(0, 0)),
                tw.children(0x201, temporary=[
                    rec(b"LAND", 0x500, s(b"DATA", tw.u32(0)) + s(b"VHGT", vhgt_from(designed_grid(), OFFSET)))]),
                cell(0x203, grid=(1, 0), grid12=True),
                tw.children(0x203, temporary=[rec(b"LAND", 0x502, s(b"VHGT", tw.ramp_vhgt(0.0)), compress=True)]),
                cell(0x204, grid=(0, -1)),                                 # no LAND: not listed
                tw.children(0x204, temporary=[rec(b"REFR", 0x300, tw.placement(0x17))])),
            blk(-1, 0,
                cell(0x202, grid=(-1, 2)),
                tw.children(0x202, temporary=[                             # LAND without VHGT
                    rec(b"LAND", 0x501, s(b"DATA", tw.u32(0)) + s(b"VCLR", b"\0" * (33 * 33 * 3)))]),
                cell(0x205, grid=(-1, -3)),
                tw.children(0x205, temporary=[                             # wrong-size VHGT
                    rec(b"LAND", 0x503, s(b"VHGT", tw.ramp_vhgt(0.0)[:-3]))]))),
        rec(b"WRLD", 0x101, b""),                                          # no EDID
        grp(0x101, 1,
            blk(3, 3,
                cell(0x220, grid=(3, 3)),
                tw.children(0x220, temporary=[rec(b"LAND", 0x520, s(b"VHGT", tw.ramp_vhgt(2.0)))]))),
    )
    return tw.tes4() + world


def build_dep():
    s, rec, grp, blk = tw.sub, tw.record, tw.group, tw.block
    world = grp(
        b"WRLD", 0,
        grp(0x100, 1,
            blk(0, 0,
                tw.children(0x203, temporary=[rec(b"LAND", 0x502, s(b"VHGT", tw.ramp_vhgt(-5.0)))]))))
    return tw.tes4(masters=["Base.esm"]) + world


LOAD_ORDER = ["Base.esm", "Dep.esm"]


class PhysOracleTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = self._tmp.name
        for name, data in (("Base.esm", build_base()), ("Dep.esm", build_dep())):
            with open(os.path.join(self.tmp, name), "wb") as f:
                f.write(data)

    def tearDown(self):
        self._tmp.cleanup()

    def heights(self, *args, plugins=LOAD_ORDER):
        out = os.path.join(self.tmp, "out.json")
        err = io.StringIO()
        with redirect_stderr(err):
            rc = oracle_phys.main(["heights", self.tmp, *plugins, *args, "-o", out])
        self.assertEqual(rc, 0, err.getvalue())
        with open(out, encoding="utf-8") as f:
            return json.load(f), err.getvalue()

    def assertHeights(self, got, want, places=9):
        self.assertEqual(len(got), len(want))
        for i, (g, w) in enumerate(zip(got, want)):
            self.assertAlmostEqual(g, w, places=places, msg=f"point {i} {POINTS[i]}")

    # ----------------------------------------------------------------- document
    def test_document_shape(self):
        doc, _ = self.heights("--world", "physworld")
        self.assertEqual(list(doc), ["schema", "world", "edid", "points", "cells", "load_order"])
        self.assertEqual(doc["schema"], "fnv.phys.heights/1")
        self.assertEqual(doc["world"], "0x00000100")
        self.assertEqual(doc["edid"], "PhysWorld")
        self.assertEqual(doc["points"], POINTS)
        self.assertEqual(doc["load_order"], LOAD_ORDER)
        # Sorted by (x, y); the persistent cell and the cell without LAND are not listed.
        self.assertEqual([c["grid"] for c in doc["cells"]], [[-1, -3], [-1, 2], [0, 0], [1, 0]])
        for c in doc["cells"]:
            self.assertEqual(list(c), ["grid", "heights"])

    def test_designed_heights(self):
        doc, _ = self.heights("--world", "PhysWorld")
        cells = {tuple(c["grid"]): c["heights"] for c in doc["cells"]}
        self.assertHeights(cells[(0, 0)], DESIGNED)
        # Vertices are the vertex heights exactly.
        self.assertEqual(cells[(0, 0)][:4], DESIGNED[:4])
        # The other diagonal would differ beyond the lab tolerance at every interior point
        # (point 4 lies in the north-west triangle, points 5-7 in the south-east one).
        for i, other in enumerate(OTHER_DIAGONAL):
            self.assertGreater(abs(cells[(0, 0)][4 + i] - other), 1.0, f"point {4 + i}")

    def test_null_without_vhgt(self):
        doc, _ = self.heights("--world", "PhysWorld")
        cells = {tuple(c["grid"]): c["heights"] for c in doc["cells"]}
        self.assertIsNone(cells[(-1, 2)])    # LAND with DATA / VCLR only
        self.assertIsNone(cells[(-1, -3)])   # VHGT of the wrong size counts as absent

    def test_land_override(self):
        doc, _ = self.heights("--world", "PhysWorld")
        cells = {tuple(c["grid"]): c["heights"] for c in doc["cells"]}
        self.assertHeights(cells[(1, 0)], ramp_heights(-5.0))           # Dep.esm wins
        base_only, _ = self.heights("--world", "PhysWorld", plugins=["Base.esm"])
        cells = {tuple(c["grid"]): c["heights"] for c in base_only["cells"]}
        self.assertHeights(cells[(1, 0)], ramp_heights(0.0))            # compressed LAND
        self.assertEqual(base_only["load_order"], ["Base.esm"])

    def test_world_by_formid_and_null_edid(self):
        by_edid, _ = self.heights("--world", "PHYSWORLD")
        by_fid, _ = self.heights("--world", "0x00000100")
        self.assertEqual(by_edid, by_fid)
        other, _ = self.heights("--world", "0x00000101")
        self.assertIsNone(other["edid"])
        self.assertEqual([c["grid"] for c in other["cells"]], [[3, 3]])
        self.assertHeights(other["cells"][0]["heights"], ramp_heights(2.0))

    # ----------------------------------------------------------------- surface
    def test_height_at_triangles(self):
        g = [[0.0] * 33 for _ in range(33)]
        g[0][1] = 80.0   # h10 (east) of square (0, 0)
        g[1][0] = 40.0   # h01 (north)
        g[1][1] = 8.0    # h11 (north-east)
        f = oracle_phys.height_at
        self.assertEqual(f(g, 0.0, 0.0), 0.0)
        self.assertEqual(f(g, 1.0, 0.0), 80.0)
        self.assertEqual(f(g, 0.0, 1.0), 40.0)
        self.assertEqual(f(g, 1.0, 1.0), 8.0)
        # South-east triangle (fu >= fv): 0.6*80 + 0.2*(8-80) = 33.6.
        self.assertAlmostEqual(f(g, 0.6, 0.2), 33.6, places=12)
        # North-west triangle (fu < fv): 0.6*40 + 0.2*(8-40) = 17.6.
        self.assertAlmostEqual(f(g, 0.2, 0.6), 17.6, places=12)
        # On the diagonal both formulas agree: t * h11.
        self.assertAlmostEqual(f(g, 0.5, 0.5), 4.0, places=12)
        # The far edges use the last square.
        g[32][32] = 16.0
        self.assertEqual(f(g, 32.0, 32.0), 16.0)
        self.assertIsNone(f(g, -0.1, 3.0))
        self.assertIsNone(f(g, 3.0, 32.5))

    # ----------------------------------------------------------------- CLI
    def test_cli_exit_codes(self):
        script = os.path.join(CAPTURE_DIR, "oracle_phys.py")
        r = subprocess.run([sys.executable, script, "heights", self.tmp, *LOAD_ORDER, "--world", "PhysWorld",
                            "--schema", "ignored"], capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(len(json.loads(r.stdout.decode("utf-8"))["cells"]), 4)
        r = subprocess.run([sys.executable, script, "heights", self.tmp, "Missing.esm", "--world", "PhysWorld"],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "heights", self.tmp, *LOAD_ORDER, "--world", "NoSuchWorld"],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "heights", self.tmp, *LOAD_ORDER], capture_output=True)
        self.assertEqual(r.returncode, 2)  # no --world
        r = subprocess.run([sys.executable, script, "bogus"], capture_output=True)
        self.assertEqual(r.returncode, 2)

    def test_garbage_does_not_crash(self):
        data = build_base()
        for cut in (30, 200, len(data) // 2, len(data) - 7):
            with open(os.path.join(self.tmp, "Cut.esm"), "wb") as f:
                f.write(data[:cut])
            err = io.StringIO()
            out = os.path.join(self.tmp, "cut.json")
            with redirect_stderr(err):
                rc = oracle_phys.main(["heights", self.tmp, "Cut.esm", "--world", "PhysWorld", "-o", out])
            self.assertIn(rc, (0, 1))


if __name__ == "__main__":
    unittest.main()
