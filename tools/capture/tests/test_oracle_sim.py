"""Tests for oracle_sim.py using tiny synthetic plugins (no game data).

Run from the repo root:

    python -m unittest discover -s tools/capture/tests -v
"""
import json
import os
import struct
import subprocess
import sys
import tempfile
import unittest
import zlib

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)

import oracle_sim  # noqa: E402


# =========================================================================== builders

def sub(sig, data):
    return sig + struct.pack("<H", len(data)) + data


def zstr(text):
    return text.encode("cp1252") + b"\0"


def record(sig, formid, payload=b"", flags=0, compress=False, bad_adler=False):
    if compress:
        stream = zlib.compress(payload)
        if bad_adler:
            stream = stream[:-1] + bytes([stream[-1] ^ 0x5A])
        payload = struct.pack("<I", len(payload)) + stream
        flags |= 0x00040000
    return struct.pack("<4sIIIIHH", sig, len(payload), flags, formid, 0, 15, 0) + payload


def group(label, group_type, *children):
    body = b"".join(children)
    if isinstance(label, int):
        label = struct.pack("<I", label)
    return struct.pack("<4sI4siHHHH", b"GRUP", 24 + len(body), label, group_type, 0, 0, 0, 0) + body


def tes4(masters=()):
    payload = sub(b"HEDR", struct.pack("<fiI", 1.34, 0, 0x800))
    for m in masters:
        payload += sub(b"MAST", zstr(m)) + sub(b"DATA", struct.pack("<Q", 0))
    return record(b"TES4", 0, payload, flags=0x1)


def cnto(item, count):
    return sub(b"CNTO", struct.pack("<Ii", item, count))


def ligh(formid, carried):
    data = struct.pack("<iI4sIff", 0, 256, b"\xff\xff\xff\x00", 0x2 if carried else 0x400, 1.0, 90.0)
    return record(b"LIGH", formid, sub(b"EDID", zstr(f"Light{formid:X}")) + sub(b"DATA", data))


def cont(formid, edid=None, full=None, script=None, data_flags=None, entries=(), **kw):
    payload = b""
    if edid is not None:
        payload += sub(b"EDID", zstr(edid))
    if full is not None:
        payload += sub(b"FULL", zstr(full))
    if script is not None:
        payload += sub(b"SCRI", struct.pack("<I", script))
    for item, count in entries:
        payload += cnto(item, count)
        payload += sub(b"COED", bytes(12))  # extra data after an entry is ignored
    if data_flags is not None:
        payload += sub(b"DATA", struct.pack("<Bf", data_flags, 25.0))
    return record(b"CONT", formid, payload, **kw)


# Load order: Master.esm (0), Other.esm (1), Dep.esm (2).
# Dep.esm lists its masters in the order Other.esm, Master.esm, so its top bytes are
# 0 -> Other (1), 1 -> Master (0), 2 -> Dep itself (2).

def build_master():
    body = (
        group(b"WEAP", 0, record(b"WEAP", 0x00001000, sub(b"EDID", zstr("Gun"))))
        + group(b"MISC", 0, record(b"MISC", 0x00001005, sub(b"EDID", zstr("Junk"))))
        + group(b"LIGH", 0, ligh(0x00001003, True), ligh(0x00001004, False))
        + group(b"STAT", 0, record(b"STAT", 0x00001002, sub(b"EDID", zstr("Rock"))))
        + group(b"SCPT", 0, record(b"SCPT", 0x00003000, sub(b"EDID", zstr("ContScript"))))
        + group(b"CONT", 0,
                # deliberately not in FormID order
                cont(0x00002004, edid="Packed", entries=[(0x00001000, 2)], compress=True, bad_adler=True),
                cont(0x00002000, edid="Chest", full="Café Chest", script=0x00003000, data_flags=0x02,
                     entries=[(0x00001000, 1), (0x00001001, 3), (0x00001002, 1), (0x00001003, 1),
                              (0x00001004, 1), (0x00000007, 5), (0x00009999, 1), (0x00000000, 1),
                              (0x05001234, 1), (0x00001005, -2), (0x00001000, 0)]),
                cont(0x00002001, edid="Overridden", data_flags=0x00, entries=[(0x00001000, 1)]),
                cont(0x00002002, edid="WillBeDeleted"),
                cont(0x00002003))
        + group(b"LVLI", 0, record(b"LVLI", 0x00001001, sub(b"EDID", zstr("LL"))))
        + group(b"CELL", 0,
                group(0, 2, group(0, 3,
                                  record(b"CELL", 0x00004000, sub(b"EDID", zstr("Cell"))),
                                  group(0x00004000, 6,
                                        group(0x00004000, 9,
                                              record(b"REFR", 0x00004001,
                                                     sub(b"NAME", struct.pack("<I", 0x00001000))))))))
    )
    return tes4() + body


def build_other():
    body = group(b"CONT", 0, cont(0x01000900, edid="OtherBox", entries=[(0x00001000, 1)]))
    return tes4(["Master.esm"]) + body


def build_dep():
    body = (
        group(b"WEAP", 0, record(b"WEAP", 0x02000800, sub(b"EDID", zstr("NewGun"))))
        + group(b"MISC", 0, record(b"MISC", 0x01001005, flags=0x20))
        + group(b"CONT", 0,
                cont(0x02000010, edid="NewBox", script=0x07000001, data_flags=0x01,
                     entries=[(0x01001000, 1)]),
                cont(0x01002001, edid="OverriddenByDep", full="Dep Box", data_flags=0x03,
                     entries=[(0x02000800, 4), (0x00000900, 1), (0x01004001, 1), (0x03000001, 1)]),
                record(b"CONT", 0x01002002, flags=0x20))
        + group(b"STAT", 0, record(b"STAT", 0x03000801, sub(b"EDID", zstr("OutOfRange"))))
    )
    return tes4(["Other.esm", "Master.esm"]) + body


class TempDirCase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = self._tmp.name

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, name, data):
        with open(os.path.join(self.tmp, name), "wb") as f:
            f.write(data)

    def run_main(self, argv):
        out = os.path.join(self.tmp, "out.json")
        rc = oracle_sim.main(argv + ["-o", out])
        with open(out, encoding="utf-8") as f:
            return rc, json.load(f)


class ContainersTest(TempDirCase):
    def setUp(self):
        super().setUp()
        self.write("Master.esm", build_master())
        self.write("Other.esm", build_other())
        self.write("Dep.esm", build_dep())
        rc, self.doc = self.run_main(["containers", self.tmp, "Master.esm", "Other.esm", "Dep.esm"])
        self.assertEqual(rc, 0)
        self.by_id = {c["formid"]: c for c in self.doc["containers"]}
        self.codes = {w["code"] for w in self.doc["warnings"]}

    def test_format_count_and_ordering(self):
        self.assertEqual(self.doc["format"], "fnv.sim.containers/1")
        ids = [c["formid"] for c in self.doc["containers"]]
        self.assertEqual(ids, ["0x00002000", "0x00002001", "0x00002003", "0x00002004",
                               "0x01000900", "0x02000010"])
        self.assertEqual(self.doc["count"], 6)
        for c in self.doc["containers"]:
            self.assertEqual(list(c), ["formid", "edid", "name", "respawns", "script", "entries"])

    def test_entry_classes(self):
        chest = self.by_id["0x00002000"]
        self.assertEqual(chest["edid"], "Chest")
        self.assertEqual(chest["name"], "Café Chest")
        self.assertEqual(chest["script"], "0x00003000")
        self.assertTrue(chest["respawns"])
        self.assertEqual(chest["entries"], [
            {"item": "0x00001000", "count": 1, "class": "item"},
            {"item": "0x00001001", "count": 3, "class": "leveled_item"},
            {"item": "0x00001002", "count": 1, "class": "other:STAT"},
            {"item": "0x00001003", "count": 1, "class": "item"},          # carried LIGH
            {"item": "0x00001004", "count": 1, "class": "other:LIGH"},    # uncarriable LIGH
            {"item": "0x00000007", "count": 5, "class": "engine_builtin"},
            {"item": "0x00009999", "count": 1, "class": "missing"},
            # null item and out-of-range master index 5 are omitted
            {"item": "0x00001005", "count": -2, "class": "missing"},      # deleted by Dep.esm
            {"item": "0x00001000", "count": 0, "class": "item"},          # count kept as stored
        ])
        self.assertEqual(self.doc["summary"]["null_items"], 1)
        self.assertEqual(self.doc["summary"]["unmappable_items"], 2)
        self.assertIn("cnto_unmappable", self.codes)

    def test_globalisation_and_override(self):
        box = self.by_id["0x00002001"]
        self.assertEqual(box["edid"], "OverriddenByDep")
        self.assertEqual(box["name"], "Dep Box")
        self.assertTrue(box["respawns"])
        self.assertEqual(box["entries"], [
            {"item": "0x02000800", "count": 4, "class": "item"},        # Dep's own WEAP
            {"item": "0x01000900", "count": 1, "class": "other:CONT"},  # Other.esm's CONT
            {"item": "0x00004001", "count": 1, "class": "other:REFR"},  # Master's REFR in a cell
            # 0x03000001: Dep has 2 masters, index 3 is out of range -> omitted
        ])
        new_box = self.by_id["0x02000010"]
        self.assertFalse(new_box["respawns"])  # flags 0x01: bit 1 clear
        self.assertIsNone(new_box["script"])   # SCRI with an out-of-range master index
        self.assertEqual(new_box["entries"], [{"item": "0x00001000", "count": 1, "class": "item"}])
        self.assertIn("scri_unmappable", self.codes)
        other = self.by_id["0x01000900"]
        self.assertEqual(other["entries"], [{"item": "0x00001000", "count": 1, "class": "item"}])

    def test_deleted_winner_and_unmappable_record(self):
        self.assertNotIn("0x00002002", self.by_id)
        self.assertIn("unmappable_formid", self.codes)

    def test_null_fields_and_compressed_record(self):
        empty = self.by_id["0x00002003"]
        self.assertEqual(empty, {"formid": "0x00002003", "edid": None, "name": None,
                                 "respawns": False, "script": None, "entries": []})
        packed = self.by_id["0x00002004"]
        self.assertEqual(packed["edid"], "Packed")
        self.assertEqual(packed["entries"], [{"item": "0x00001000", "count": 2, "class": "item"}])
        self.assertIn("zlib_checksum_mismatch", self.codes)

    def test_summary(self):
        s = self.doc["summary"]
        self.assertEqual(s["respawns"], 2)
        self.assertEqual(s["without_cnto"], 1)
        self.assertEqual(s["entries"], sum(len(c["entries"]) for c in self.doc["containers"]))


class LoadOrderTest(TempDirCase):
    def test_master_not_loaded_is_unmappable(self):
        self.write("Dep.esm", build_dep())
        self.write("Other.esm", build_other())
        rc, doc = self.run_main(["containers", self.tmp, "Other.esm", "Dep.esm"])
        self.assertEqual(rc, 0)
        codes = {w["code"] for w in doc["warnings"]}
        self.assertIn("master_not_loaded", codes)
        ids = [c["formid"] for c in doc["containers"]]
        # Other.esm loads at 0 with a missing master: its own CONT (top byte 1) -> 0x00000900.
        # Dep.esm at 1: own CONT -> 0x01000010; its overrides of Master.esm are unmappable.
        self.assertEqual(ids, ["0x00000900", "0x01000010"])
        self.assertEqual(doc["containers"][0]["entries"], [])  # item in Master.esm: unmappable

    def test_last_definition_wins_within_one_plugin(self):
        body = group(b"CONT", 0, cont(0x00000900, edid="First"), cont(0x00000900, edid="Second"))
        self.write("A.esm", tes4() + body)
        _, doc = self.run_main(["containers", self.tmp, "A.esm"])
        self.assertEqual([c["edid"] for c in doc["containers"]], ["Second"])

    def test_case_insensitive_master_names(self):
        self.write("Master.esm", build_master())
        self.write("Other.esm", tes4(["MASTER.ESM"]) +
                   group(b"CONT", 0, cont(0x00002001, edid="Upper")))
        _, doc = self.run_main(["containers", self.tmp, "Master.esm", "Other.esm"])
        self.assertEqual({c["formid"]: c["edid"] for c in doc["containers"]}["0x00002001"], "Upper")


class RobustnessTest(TempDirCase):
    def test_garbage_does_not_crash(self):
        good = tes4() + group(b"CONT", 0, cont(0x00000900, edid="Box", entries=[(0x00000901, 1)]))
        for n in range(len(tes4()) + 1, len(good)):
            self.write("T.esm", good[:n])
            rc, doc = self.run_main(["containers", self.tmp, "T.esm"])
            self.assertEqual(rc, 0)
            self.assertLessEqual(doc["count"], 1)
        bad_zlib = tes4() + group(b"CONT", 0, record(b"CONT", 0x00000900,
                                                     struct.pack("<I", 50) + b"\x00\x01garbage",
                                                     flags=0x00040000))
        self.write("Z.esm", bad_zlib)
        rc, doc = self.run_main(["containers", self.tmp, "Z.esm"])
        self.assertEqual(rc, 0)
        self.assertEqual(doc["containers"][0]["entries"], [])
        self.assertIn("container_data_unreadable", {w["code"] for w in doc["warnings"]})

    def test_cli_exit_codes(self):
        self.write("Master.esm", build_master())
        script = os.path.join(CAPTURE_DIR, "oracle_sim.py")
        r = subprocess.run([sys.executable, script, "containers", self.tmp, "Master.esm"],
                           capture_output=True)
        self.assertEqual(r.returncode, 0)
        doc = json.loads(r.stdout.decode("utf-8"))
        self.assertEqual(doc["containers"][0]["name"], "Café Chest")
        r = subprocess.run([sys.executable, script, "containers", self.tmp, "Missing.esm"],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        self.write("NotAPlugin.esm", b"hello")
        r = subprocess.run([sys.executable, script, "containers", self.tmp, "NotAPlugin.esm"],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "containers", os.path.join(self.tmp, "nope"),
                            "Master.esm"], capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "containers", self.tmp], capture_output=True)
        self.assertEqual(r.returncode, 2)
        r = subprocess.run([sys.executable, script, "containers", self.tmp, "Master.esm", "master.esm"],
                           capture_output=True)
        self.assertEqual(r.returncode, 2)
        r = subprocess.run([sys.executable, script, "bogus"], capture_output=True)
        self.assertEqual(r.returncode, 2)


if __name__ == "__main__":
    unittest.main()
