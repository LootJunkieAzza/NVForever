"""Tests for oracle_world.py using small synthetic plugins.

Three plugins in load order: Base.esm (a worldspace with a persistent cell and block
groups, a second worldspace, an interior cell, base objects), Other.esm (no masters, only
shifts the load-order index of the next plugin) and Dep.esm (master Base.esm, loaded at
index 2) which overrides a reference, moves a reference to another cell, adds references
into master cells, overrides a cell and a LAND, and carries an unmappable FormID.
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
import zlib
from contextlib import redirect_stderr

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)

import oracle_world  # noqa: E402


def f32(x):
    """The value an f32 field holding x decodes to (widened, unrounded)."""
    return struct.unpack("<f", struct.pack("<f", x))[0]


# =========================================================================== builders

def sub(sig, data):
    if len(data) > 0xFFFF:
        return b"XXXX" + struct.pack("<HI", 4, len(data)) + sig + struct.pack("<H", 0) + data
    return sig + struct.pack("<H", len(data)) + data


def zstr(text):
    return text.encode("cp1252") + b"\0"


def u32(v):
    return struct.pack("<I", v)


def record(sig, formid, payload, flags=0, compress=False):
    if compress:
        payload = struct.pack("<I", len(payload)) + zlib.compress(payload)
        flags |= 0x00040000
    return struct.pack("<4sIIIIHH", sig, len(payload), flags, formid, 0, 15, 0) + payload


def group(label, group_type, *children):
    body = b"".join(children)
    if isinstance(label, int):
        label = struct.pack("<I", label)
    return struct.pack("<4sI4siHHHH", b"GRUP", 24 + len(body), label, group_type, 0, 0, 0, 0) + body


def block(x, y, *children):
    """Exterior block (type 4) containing one sub-block (type 5)."""
    label = struct.pack("<hh", y, x)
    return group(label, 4, group(label, 5, *children))


def interior_block(*children):
    return group(0, 2, group(0, 3, *children))


def tes4(masters=()):
    payload = sub(b"HEDR", struct.pack("<fiI", 1.34, 0, 0x800))
    for m in masters:
        payload += sub(b"MAST", zstr(m)) + sub(b"DATA", struct.pack("<Q", 0))
    return record(b"TES4", 0, payload, flags=0x1)


def placement(base, pos=(0.0, 0.0, 0.0), rot=(0.0, 0.0, 0.0), extra=b""):
    return sub(b"NAME", u32(base)) + sub(b"DATA", struct.pack("<6f", *pos, *rot)) + extra


def ramp_vhgt(offset):
    """VHGT whose integrated heights are 8 * (offset + 2 * row + col)."""
    steps = []
    for r in range(33):
        for c in range(33):
            if c == 0:
                steps.append(0 if r == 0 else 2)
            else:
                steps.append(1)
    return struct.pack("<f", offset) + struct.pack("<1089b", *steps) + b"\0\0\0"


def ramp_stats(offset):
    o = offset
    return {"min": 8.0 * o, "max": 8.0 * (o + 96), "mean": 8.0 * (o + 48),
            "corners": [8.0 * o, 8.0 * (o + 32), 8.0 * (o + 64), 8.0 * (o + 96)],
            "centre": 8.0 * (o + 48)}


def cell(formid, edid=None, grid=None, interior=False, water=None, full=None, grid12=False):
    payload = b""
    if edid is not None:
        payload += sub(b"EDID", zstr(edid))
    if full is not None:
        payload += sub(b"FULL", zstr(full))
    payload += sub(b"DATA", bytes([0x1 if interior else 0x2]))
    if grid is not None:
        payload += sub(b"XCLC", struct.pack("<ii", *grid) + (b"\x01\0\0\0" if grid12 else b""))
    if water is not None:
        payload += sub(b"XCLW", struct.pack("<f", water))
    return record(b"CELL", formid, payload)


def children(cell_id, persistent=(), temporary=()):
    groups = []
    if persistent:
        groups.append(group(cell_id, 8, *persistent))
    if temporary:
        groups.append(group(cell_id, 9, *temporary))
    return group(cell_id, 6, *groups)


def build_base():
    stat = group(b"STAT", 0,
                 record(b"STAT", 0x400, sub(b"EDID", zstr("Rock01")) + sub(b"MODL", zstr("Meshes/Rocks/Rock01.NIF"))),
                 record(b"STAT", 0x402, sub(b"EDID", zstr("Box")) + sub(b"MODL", zstr("Clutter\\Box.nif"))
                        + sub(b"MODL", zstr("second\\ignored.nif"))),
                 record(b"STAT", 0x403, sub(b"EDID", zstr("Marker"))))
    tree = group(b"TREE", 0, record(b"TREE", 0x401, sub(b"EDID", zstr("Shrub")) + sub(b"MODL", zstr("\\WastelandShrub01.SPT"))))
    world = group(
        b"WRLD", 0,
        record(b"WRLD", 0x100, sub(b"EDID", zstr("TestWorld")) + sub(b"FULL", zstr("Test World"))),
        group(0x100, 1,
              cell(0x200, grid=(0, 0), water=3.4028234663852886e38),          # persistent cell
              children(0x200,
                       persistent=[record(b"REFR", 0x300, placement(0x400, (1.0, 2.0, 3.0), (0.0, 0.0, 0.5)),
                                          flags=0x400)],
                       temporary=[record(b"LAND", 0x502, sub(b"VHGT", ramp_vhgt(100.0)))]),
              block(0, 0,
                    cell(0x201, edid="Cell00", grid=(0, 0), water=2600.0),
                    children(0x201, temporary=[
                        record(b"LAND", 0x500, sub(b"DATA", u32(0)) + sub(b"VHGT", ramp_vhgt(10.0))),
                        record(b"REFR", 0x301, placement(0x400, (0.1, 0.2, 0.3)) + sub(b"XSCL", struct.pack("<f", 2.0))
                               + sub(b"XESP", struct.pack("<IB3x", 0x300, 1))),
                        record(b"ACHR", 0x302, placement(0x17, (5.0, 5.0, 5.0))),       # engine-defined base
                        record(b"NAVM", 0x600, sub(b"DATA", b"\0" * 8)),
                    ]),
                    cell(0x202, edid="Cell10", grid=(1, 0), grid12=True),
                    children(0x202, temporary=[
                        record(b"LAND", 0x501, sub(b"VHGT", ramp_vhgt(0.0)), compress=True),
                        record(b"REFR", 0x303, placement(0x401, (4100.0, 10.0, 0.0))),
                    ]))),
        record(b"WRLD", 0x101, sub(b"EDID", zstr("OtherWorld"))),
        group(0x101, 1,
              block(5, 5,
                    cell(0x220, edid="Far", grid=(5, 5)),
                    children(0x220, temporary=[record(b"REFR", 0x320, placement(0x400))]))),
    )
    interiors = group(
        b"CELL", 0,
        interior_block(
            cell(0x210, edid="TestInterior", full="Test Interior", interior=True, water=-2147483648.0),
            children(0x210,
                     persistent=[record(b"REFR", 0x311, placement(0x402), flags=0x400 | 0x800)],
                     temporary=[record(b"REFR", 0x310, placement(0x402, (7.0, 8.0, 9.0), (0.25, 0.5, 0.75)),
                                       compress=True),
                                record(b"REFR", 0x312, b"", flags=0x20)])))
    return tes4() + stat + tree + world + interiors


def build_other():
    return (tes4()
            + group(b"STAT", 0, record(b"STAT", 0x900, sub(b"EDID", zstr("OtherStat"))))
            + group(b"CELL", 0, interior_block(cell(0x950, edid="OtherInterior", interior=True))))


def build_dep():
    # Stored FormIDs: 0x00xxxxxx = Base.esm (master 0), 0x01xxxxxx = Dep.esm itself.
    stat = group(b"STAT", 0,
                 record(b"STAT", 0x01000801, sub(b"EDID", zstr("DepStat")) + sub(b"MODL", zstr(""))),
                 record(b"STAT", 0x400, sub(b"EDID", zstr("Rock02")) + sub(b"MODL", zstr("rocks\\rock02.nif"))))
    world = group(
        b"WRLD", 0,
        group(0x100, 1,
              block(0, 0,
                    cell(0x201, edid="Cell00Renamed", grid=(0, 0)),             # override, no XCLW
                    children(0x201, temporary=[
                        record(b"REFR", 0x301, placement(0x400, (11.0, 12.0, 13.0))),   # override
                        record(b"REFR", 0x303, placement(0x401, (50.0, 60.0, 70.0))),   # moved from 0x202
                    ]),
                    children(0x202, temporary=[                                      # no CELL record
                        record(b"REFR", 0x01000800, placement(0x01000801)),           # new, own base
                        record(b"LAND", 0x501, sub(b"VHGT", ramp_vhgt(-5.0))),        # LAND override
                        record(b"REFR", 0x05000999, placement(0x400)),                # unmappable
                    ]))))
    interiors = group(
        b"CELL", 0,
        interior_block(children(0x210, temporary=[
            record(b"PGRE", 0x01000802, placement(0x402) + sub(b"XESP", struct.pack("<IB3x", 0, 1)))])))
    return tes4(masters=["Base.esm"]) + stat + world + interiors


LOAD_ORDER = ["Base.esm", "Other.esm", "Dep.esm"]


# =========================================================================== tests

class WorldOracleTest(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = self._tmp.name
        for name, data in (("Base.esm", build_base()), ("Other.esm", build_other()), ("Dep.esm", build_dep())):
            with open(os.path.join(self.tmp, name), "wb") as f:
                f.write(data)

    def tearDown(self):
        self._tmp.cleanup()

    def run_main(self, argv):
        out = os.path.join(self.tmp, "out.json")
        err = io.StringIO()
        with redirect_stderr(err):
            rc = oracle_world.main(argv + ["-o", out])
        self.assertEqual(rc, 0, err.getvalue())
        with open(out, encoding="utf-8") as f:
            return json.load(f), err.getvalue()

    def refs(self, *selection):
        return self.run_main(["refs", self.tmp, *LOAD_ORDER, *selection])

    # ----------------------------------------------------------------- refs --world
    def test_world_refs(self):
        doc, err = self.refs("--world", "testworld")
        self.assertEqual(doc["schema"], "fnv.world.refs/1")
        self.assertEqual(doc["load_order"], LOAD_ORDER)
        self.assertEqual(list(doc), ["schema", "load_order", "cells", "refs"])
        self.assertIn("0x05000999", err)  # unmappable FormID reported
        cells = {c["formid"]: c for c in doc["cells"]}
        self.assertEqual([c["formid"] for c in doc["cells"]], ["0x00000200", "0x00000201", "0x00000202"])
        self.assertEqual(cells["0x00000200"], {
            "formid": "0x00000200", "edid": None, "name": None, "world": "0x00000100", "grid": [0, 0],
            "interior": False, "water_height": 3.4028234663852886e38, "refs": 1, "land": "0x00000502"})
        # Dep.esm's override of cell 0x201 wins: new EDID, no XCLW.
        self.assertEqual(cells["0x00000201"], {
            "formid": "0x00000201", "edid": "Cell00Renamed", "name": None, "world": "0x00000100",
            "grid": [0, 0], "interior": False, "water_height": None, "refs": 3, "land": "0x00000500"})
        self.assertEqual(cells["0x00000202"]["grid"], [1, 0])          # 12-byte XCLC
        self.assertEqual(cells["0x00000202"]["edid"], "Cell10")
        self.assertEqual(cells["0x00000202"]["refs"], 1)
        self.assertEqual(cells["0x00000202"]["land"], "0x00000501")

        refs = {r["formid"]: r for r in doc["refs"]}
        self.assertEqual([r["formid"] for r in doc["refs"]],
                         ["0x00000300", "0x00000301", "0x00000302", "0x00000303", "0x02000800"])
        self.assertEqual(refs["0x00000300"], {
            "formid": "0x00000300", "type": "REFR", "cell": "0x00000200", "base": "0x00000400",
            "base_type": "STAT", "base_edid": "Rock02", "model": "meshes\\rocks\\rock02.nif",
            "position": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, 0.5], "scale": 1.0, "flags": 0x400,
            "persistent": True, "initially_disabled": False, "deleted": False, "enable_parent": None})
        # Overridden by Dep.esm: its DATA, no XSCL / XESP any more.
        r = refs["0x00000301"]
        self.assertEqual(r["position"], [11.0, 12.0, 13.0])
        self.assertEqual(r["scale"], 1.0)
        self.assertIsNone(r["enable_parent"])
        self.assertEqual(r["cell"], "0x00000201")
        # Engine-defined base: mapped, but no type / EDID / model.
        r = refs["0x00000302"]
        self.assertEqual((r["type"], r["base"], r["base_type"], r["base_edid"], r["model"]),
                         ("ACHR", "0x00000017", None, None, None))
        # Moved into cell 0x201 by Dep.esm; .spt model lives in trees\.
        r = refs["0x00000303"]
        self.assertEqual(r["cell"], "0x00000201")
        self.assertEqual(r["position"], [50.0, 60.0, 70.0])
        self.assertEqual((r["base_type"], r["model"]), ("TREE", "trees\\wastelandshrub01.spt"))
        # New reference from Dep.esm (load index 2) in a master cell; blank MODL -> null.
        r = refs["0x02000800"]
        self.assertEqual((r["cell"], r["base"], r["base_edid"], r["model"]),
                         ("0x00000202", "0x02000801", "DepStat", None))
        self.assertNotIn("0x00000320", refs)  # other worldspace
        self.assertNotIn("0x00000600", refs)  # NAVM is not a reference

    def test_world_by_formid(self):
        by_edid, _ = self.refs("--world", "TESTWORLD")
        by_fid, _ = self.refs("--world", "0x00000100")
        self.assertEqual(by_edid, by_fid)
        other, _ = self.refs("--world", "0x00000101")
        self.assertEqual([c["formid"] for c in other["cells"]], ["0x00000220"])
        self.assertEqual([r["formid"] for r in other["refs"]], ["0x00000320"])

    def test_base_record_override_and_xesp(self):
        # Base.esm alone: the original XSCL / XESP of 0x301 and the original Rock01 base.
        doc, _ = self.run_main(["refs", self.tmp, "Base.esm", "--world", "TestWorld"])
        refs = {r["formid"]: r for r in doc["refs"]}
        r = refs["0x00000301"]
        self.assertEqual(r["position"], [f32(0.1), f32(0.2), f32(0.3)])
        self.assertEqual(r["scale"], 2.0)
        self.assertEqual(r["enable_parent"], {"ref": "0x00000300", "opposite": True})
        self.assertEqual((r["base_edid"], r["model"]), ("Rock01", "meshes\\rocks\\rock01.nif"))
        self.assertEqual(refs["0x00000303"]["cell"], "0x00000202")
        self.assertEqual(doc["load_order"], ["Base.esm"])

    # ----------------------------------------------------------------- refs --interiors
    def test_interiors(self):
        doc, _ = self.refs("--interiors")
        self.assertEqual([c["formid"] for c in doc["cells"]], ["0x00000210", "0x01000950"])
        c = doc["cells"][0]
        self.assertEqual(c, {"formid": "0x00000210", "edid": "TestInterior", "name": "Test Interior",
                             "world": None, "grid": None, "interior": True,
                             "water_height": -2147483648.0, "refs": 4, "land": None})
        self.assertEqual(doc["cells"][1]["refs"], 0)
        refs = {r["formid"]: r for r in doc["refs"]}
        self.assertEqual(list(refs), ["0x00000310", "0x00000311", "0x00000312", "0x02000802"])
        r = refs["0x00000310"]  # compressed record
        self.assertEqual(r["position"], [7.0, 8.0, 9.0])
        self.assertEqual(r["rotation"], [0.25, 0.5, 0.75])
        self.assertEqual(r["flags"], 0x00040000)
        self.assertEqual((r["model"], r["persistent"]), ("meshes\\clutter\\box.nif", False))
        r = refs["0x00000311"]
        self.assertEqual((r["persistent"], r["initially_disabled"], r["deleted"]), (True, True, False))
        r = refs["0x00000312"]  # deleted, no subrecords
        self.assertEqual((r["deleted"], r["base"], r["position"], r["rotation"], r["scale"]),
                         (True, None, [0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 1.0))
        r = refs["0x02000802"]  # Dep.esm PGRE in a master interior; XESP with null ref
        self.assertEqual((r["type"], r["cell"], r["enable_parent"]), ("PGRE", "0x00000210", None))

    # ----------------------------------------------------------------- heights
    def test_heights(self):
        doc, _ = self.run_main(["heights", self.tmp, *LOAD_ORDER, "--world", "TestWorld"])
        self.assertEqual(doc["schema"], "fnv.world.heights/1")
        self.assertEqual(doc["world"], "0x00000100")
        self.assertEqual(doc["load_order"], LOAD_ORDER)
        # The persistent cell's LAND (offset 100) is not a grid cell; cell (1, 0) uses
        # Dep.esm's LAND override (offset -5) instead of Base.esm's compressed one.
        self.assertEqual(doc["cells"], [
            {"grid": [0, 0], **ramp_stats(10.0)},
            {"grid": [1, 0], **ramp_stats(-5.0)},
        ])
        base_only, _ = self.run_main(["heights", self.tmp, "Base.esm", "--world", "TestWorld"])
        self.assertEqual(base_only["cells"][1], {"grid": [1, 0], **ramp_stats(0.0)})

    def test_integrate_vhgt(self):
        h = oracle_world.integrate_vhgt(ramp_vhgt(1.5))
        self.assertEqual(h[0][0], 12.0)
        self.assertEqual(h[0][32], 8.0 * 33.5)
        self.assertEqual(h[5][7], 8.0 * (1.5 + 10 + 7))

    def test_normalise_model(self):
        n = oracle_world.normalise_model
        self.assertEqual(n("Meshes/A/B.NIF"), "meshes\\a\\b.nif")
        self.assertEqual(n("a\\b.nif"), "meshes\\a\\b.nif")
        self.assertEqual(n("Tree01.spt"), "trees\\tree01.spt")
        self.assertEqual(n("trees\\Tree01.spt"), "trees\\tree01.spt")
        # TREE models are stored with a leading backslash; it is removed.
        self.assertEqual(n("\\WastelandShrub01.spt"), "trees\\wastelandshrub01.spt")
        self.assertEqual(n("/Clutter/Box.nif"), "meshes\\clutter\\box.nif")
        self.assertEqual(n("\\meshes\\a.nif"), "meshes\\a.nif")
        # Surrounding whitespace is removed before anything else.
        self.assertEqual(n("  \tRocks\\Rock01.nif \r\n"), "meshes\\rocks\\rock01.nif")
        self.assertEqual(n(" \\Tree01.SPT "), "trees\\tree01.spt")
        # Inner spaces are kept; only ASCII letters are lower-cased.
        self.assertEqual(n("A B\\ÀÉ.nif"), "meshes\\a b\\ÀÉ.nif")
        # Prefix check is on the normalised path: a .nif under trees\ still gets meshes\.
        self.assertEqual(n("Trees\\Cactus.NIF"), "meshes\\trees\\cactus.nif")
        self.assertIsNone(n(""))
        self.assertIsNone(n("   "))
        self.assertIsNone(n("\t\r\n"))

    # ----------------------------------------------------------------- robustness / CLI
    def test_garbage_does_not_crash(self):
        data = build_base()
        for cut in (30, 200, len(data) // 2, len(data) - 7):
            with open(os.path.join(self.tmp, "Cut.esm"), "wb") as f:
                f.write(data[:cut])
            err = io.StringIO()
            out = os.path.join(self.tmp, "cut.json")
            with redirect_stderr(err):
                rc = oracle_world.main(["refs", self.tmp, "Cut.esm", "--interiors", "-o", out])
            self.assertIn(rc, (0, 1))

    def test_cli_exit_codes(self):
        script = os.path.join(CAPTURE_DIR, "oracle_world.py")
        r = subprocess.run([sys.executable, script, "refs", self.tmp, *LOAD_ORDER, "--world", "TestWorld"],
                           capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        self.assertEqual(len(json.loads(r.stdout.decode("utf-8"))["refs"]), 5)
        r = subprocess.run([sys.executable, script, "refs", self.tmp, "Missing.esm", "--interiors"],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "heights", self.tmp, *LOAD_ORDER, "--world", "NoSuchWorld"],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "refs", self.tmp, *LOAD_ORDER], capture_output=True)
        self.assertEqual(r.returncode, 2)  # neither --world nor --interiors
        r = subprocess.run([sys.executable, script, "bogus"], capture_output=True)
        self.assertEqual(r.returncode, 2)


if __name__ == "__main__":
    unittest.main()
