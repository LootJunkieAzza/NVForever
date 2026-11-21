"""Tests for oracle_bsa.py and oracle_esm.py using tiny synthetic archives and plugins.

The builders below write minimal but well-formed files following docs/spec-notes; no game
data is used. Run from the repo root:

    python3 -m unittest discover -s tools/capture/tests -v
"""
import hashlib
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

import oracle_bsa  # noqa: E402
import oracle_esm  # noqa: E402

BSA_HEADER_KEYS = {"version", "archive_flags", "file_flags", "folder_count", "file_count",
                   "total_folder_name_length", "total_file_name_length"}
ESM_HEADER_KEYS = {"plugin", "is_master", "flags", "version", "num_records", "next_object_id",
                   "author", "description", "masters", "overridden_forms", "other_subrecords",
                   "file_size"}


# =========================================================================== BSA builder

def build_bsa(entries, archive_flags=0x3, file_flags=0, flags_pad=0, corrupt=None):
    """Build a canonical v104 archive.

    entries: list of (folder, name, decoded_bytes, mode) with mode one of
      "raw"        stored as is
      "zlib"       u32 size + complete zlib stream
      "notrailer"  u32 size + zlib stream without its Adler-32 trailer
      "badadler"   u32 size + zlib stream with a wrong Adler-32
      "empty"      just a u32 size of 0 (decoded must be b"")
    corrupt: optional {"file": name, "folder": folder} whose stored hash is altered.
    """
    corrupt = corrupt or {}
    default_compressed = bool(archive_flags & 0x4)

    def blob_for(data, mode):
        if mode == "raw":
            return data
        if mode == "empty":
            return struct.pack("<I", 0)
        stream = zlib.compress(data)
        if mode == "notrailer":
            stream = stream[:-4]
        elif mode == "badadler":
            stream = stream[:-1] + bytes([stream[-1] ^ 0xFF])
        return struct.pack("<I", len(data)) + stream

    folders = {}
    for folder, name, data, mode in entries:
        f_hash = oracle_bsa.bsa_hash(folder.encode(), True)
        if corrupt.get("folder") == folder:
            f_hash ^= 0x1
        n_hash = oracle_bsa.bsa_hash(name.encode(), False)
        if corrupt.get("file") == name:
            n_hash ^= 0x100000000
        compressed = mode != "raw"
        folders.setdefault((f_hash, folder), []).append(
            (n_hash, name, blob_for(data, mode), compressed != default_compressed))
    ordered = sorted(folders.items())
    for _, files in ordered:
        files.sort()

    folder_count = len(ordered)
    file_count = sum(len(files) for _, files in ordered)
    total_folder_name_length = sum(len(folder) + 1 for (_, folder), _ in ordered)
    names = [name for _, files in ordered for _, name, _, _ in files]
    total_file_name_length = sum(len(n) + 1 for n in names)

    pos = 36 + 16 * folder_count
    block_positions = []
    for (_, folder), files in ordered:
        block_positions.append(pos)
        pos += 1 + len(folder) + 1 + 16 * len(files)
    data_pos = pos + total_file_name_length

    out = bytearray(struct.pack("<4sIIIIIIIHH", b"BSA\0", 104, 36, archive_flags, folder_count,
                                file_count, total_folder_name_length, total_file_name_length,
                                file_flags, flags_pad))
    for ((f_hash, _), files), block in zip(ordered, block_positions):
        out += struct.pack("<QII", f_hash, len(files), block + total_file_name_length)
    blobs = []
    for (_, folder), files in ordered:
        out += bytes([len(folder) + 1]) + folder.encode() + b"\0"
        for n_hash, _, blob, toggle in files:
            size = len(blob) | (0x40000000 if toggle else 0)
            out += struct.pack("<QII", n_hash, size, data_pos)
            data_pos += len(blob)
            blobs.append(blob)
    for n in names:
        out += n.encode() + b"\0"
    for blob in blobs:
        out += blob
    return bytes(out)


# =========================================================================== ESM builder

def sub(sig, data):
    if len(data) > 0xFFFF:
        return b"XXXX" + struct.pack("<HI", 4, len(data)) + sig + struct.pack("<H", 0) + data
    return sig + struct.pack("<H", len(data)) + data


def zstr(text):
    return text.encode("cp1252") + b"\0"


def record(sig, formid, payload, flags=0, form_version=15, compress=False, bad_adler=False):
    if compress:
        stream = zlib.compress(payload)
        if bad_adler:
            stream = stream[:-1] + bytes([stream[-1] ^ 0x5A])
        payload = struct.pack("<I", len(payload)) + stream
        flags |= 0x00040000
    return struct.pack("<4sIIIIHH", sig, len(payload), flags, formid, 0, form_version, 0) + payload


def group(label, group_type, *children):
    body = b"".join(children)
    if isinstance(label, int):
        label = struct.pack("<I", label)
    return struct.pack("<4sI4siHHHH", b"GRUP", 24 + len(body), label, group_type, 0, 0, 0, 0) + body


def tes4(num_records, masters=(), onam=(), author="tester", extra=b"", flags=0x1):
    payload = sub(b"HEDR", struct.pack("<fiI", 1.34, num_records, 0x00000ABC))
    payload += sub(b"CNAM", zstr(author))
    for m in masters:
        payload += sub(b"MAST", zstr(m)) + sub(b"DATA", struct.pack("<Q", 0))
    if onam:
        payload += sub(b"ONAM", struct.pack(f"<{len(onam)}I", *onam))
    payload += extra
    return record(b"TES4", 0, payload, flags=flags)


def build_plugin():
    """TES4 + GMST + WEAP (compressed) + LAND (compressed, bad Adler) + WRLD (XXXX) +
    CELL tree nested five groups deep. Returns (bytes, records_without_tes4, groups)."""
    gmst = record(b"GMST", 0x0100A000, sub(b"EDID", zstr("sTestMessage")) + sub(b"DATA", zstr("Hi")))
    weap = record(b"WEAP", 0x0100A001,
                  sub(b"EDID", zstr("WeapTest")) + sub(b"FULL", zstr("Café Gun’s")) +
                  sub(b"FULL", zstr("second full ignored")) + sub(b"DATA", b"\x01" * 20),
                  compress=True)
    land = record(b"LAND", 0x0100A002, sub(b"DATA", b"\x00" * 4) + sub(b"VHGT", bytes(range(256)) * 4),
                  compress=True, bad_adler=True)
    big = bytes(i & 0xFF for i in range(70000))
    wrld = record(b"WRLD", 0x0100A003,
                  sub(b"EDID", zstr("TestWorld")) + sub(b"OFST", big) + sub(b"FULL", zstr("Test World")))
    cell = record(b"CELL", 0x0100A004, sub(b"EDID", zstr("TestCell")) + sub(b"DATA", b"\x01"))
    refr1 = record(b"REFR", 0x0100A005, sub(b"NAME", struct.pack("<I", 0x0100A001)))
    refr2 = record(b"REFR", 0x0100A006, b"", flags=0x20)  # deleted, no data
    acti = record(b"ACTI", 0x0100A007, sub(b"EDID", zstr("")) + sub(b"FULL", b""), form_version=7)

    body = (
        group(b"GMST", 0, gmst)
        + group(b"WEAP", 0, weap)
        + group(b"LAND", 0, land)
        + group(b"WRLD", 0, wrld)
        + group(b"CELL", 0,
                group(0, 2,
                      group(0, 3,
                            cell,
                            group(0x0100A004, 6,
                                  group(0x0100A004, 9, refr1, refr2)))))
        + group(b"ACTI", 0, acti)
    )
    records = 8   # without TES4
    groups = 10
    header = tes4(records + 1 + groups - 1, masters=["Base.esm"], onam=[0x0100A005, 0x0100A006],
                  extra=sub(b"INTV", struct.pack("<I", 1)))
    return header + body, records, groups


# =========================================================================== tests

class TempDirCase(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp = self._tmp.name

    def tearDown(self):
        self._tmp.cleanup()

    def write(self, name, data):
        path = os.path.join(self.tmp, name)
        with open(path, "wb") as f:
            f.write(data)
        return path

    def run_main(self, module, argv):
        out = os.path.join(self.tmp, "out.json")
        rc = module.main(argv + ["-o", out])
        with open(out, encoding="utf-8") as f:
            return rc, json.load(f)


class BsaHashTest(unittest.TestCase):
    def test_contract_vector(self):
        # From docs/cli-contract.md: meshes\creatures\nvsecuritron\death.kf
        self.assertEqual(oracle_bsa.bsa_hash(b"death.kf", False), 0x1776fd25640574e8)

    def test_normalisation_and_short_names(self):
        self.assertEqual(oracle_bsa.bsa_hash(b"A/B", True), oracle_bsa.bsa_hash(b"a\\b", True))
        # Second-to-last char only contributes when stem length > 2.
        self.assertEqual(oracle_bsa.bsa_hash(b"ab", True) & 0xFF00, 0)
        self.assertEqual(oracle_bsa.bsa_hash(b"abc", True) & 0xFF00, ord("b") << 8)
        self.assertEqual(oracle_bsa.bsa_hash(b"x.wav", False) & 0x80000000, 0x80000000)
        self.assertEqual(oracle_bsa.bsa_hash(b"", True), 0)


class BsaOracleTest(TempDirCase):
    def test_uncompressed_archive(self):
        entries = [
            ("meshes\\a", "one.nif", b"first file", "raw"),
            ("meshes\\a", "two.dds", b"second", "raw"),
            ("textures", "three.txt", b"", "raw"),
        ]
        path = self.write("Plain.bsa", build_bsa(entries, archive_flags=0x3, file_flags=0x3))
        rc, doc = self.run_main(oracle_bsa, [path])
        self.assertEqual(rc, 0)
        self.assertEqual(doc["schema"], "fnv.bsa.listing/1")
        self.assertEqual(doc["archive"], "Plain.bsa")
        self.assertEqual(set(doc["header"]), BSA_HEADER_KEYS)
        self.assertEqual(doc["header"]["folder_count"], 2)
        self.assertEqual(doc["header"]["file_count"], 3)
        self.assertEqual(doc["header"]["file_flags"], 3)
        self.assertEqual(doc["warnings"], [])
        self.assertEqual(doc["errors"], [])
        by_path = {f["path"]: f for f in doc["files"]}
        self.assertEqual(set(by_path), {"meshes\\a\\one.nif", "meshes\\a\\two.dds", "textures\\three.txt"})
        one = by_path["meshes\\a\\one.nif"]
        self.assertEqual(one["name_hash"], "0x%016x" % oracle_bsa.bsa_hash(b"one.nif", False))
        self.assertEqual(one["folder_hash"], "0x%016x" % oracle_bsa.bsa_hash(b"meshes\\a", True))
        self.assertRegex(one["name_hash"], r"^0x[0-9a-f]{16}$")
        self.assertEqual(one["stored_size"], 10)
        self.assertFalse(one["compressed"])
        self.assertEqual(one["size"], 10)
        self.assertEqual(one["sha256"], hashlib.sha256(b"first file").hexdigest())
        self.assertEqual(by_path["textures\\three.txt"]["size"], 0)

    def test_compressed_archive_irregular_streams(self):
        text = b"hello zlib " * 50
        entries = [
            ("menus", "full.xml", text, "zlib"),
            ("menus", "s.txt", b"abcd", "notrailer"),
            ("menus", "do_not_delete.txt", b"", "empty"),
            ("menus", "stored.txt", b"kept raw", "raw"),   # size bit 30 toggles to uncompressed
        ]
        path = self.write("Misc.bsa", build_bsa(entries, archive_flags=0x7))
        rc, doc = self.run_main(oracle_bsa, [path])
        self.assertEqual(rc, 0)
        self.assertEqual(doc["errors"], [])
        by_path = {f["path"]: f for f in doc["files"]}
        full = by_path["menus\\full.xml"]
        self.assertTrue(full["compressed"])
        self.assertEqual(full["size"], len(text))
        self.assertEqual(full["sha256"], hashlib.sha256(text).hexdigest())
        self.assertEqual(full["stored_size"], 4 + len(zlib.compress(text)))
        s = by_path["menus\\s.txt"]
        self.assertTrue(s["compressed"])
        self.assertEqual(s["size"], 4)
        self.assertEqual(s["sha256"], hashlib.sha256(b"abcd").hexdigest())
        empty = by_path["menus\\do_not_delete.txt"]
        self.assertEqual((empty["stored_size"], empty["size"]), (4, 0))
        self.assertEqual(empty["sha256"], hashlib.sha256(b"").hexdigest())
        stored = by_path["menus\\stored.txt"]
        self.assertFalse(stored["compressed"])
        self.assertEqual(stored["stored_size"], 8)   # bit 30 masked out
        self.assertEqual(stored["sha256"], hashlib.sha256(b"kept raw").hexdigest())
        codes = {w["code"] for w in doc["warnings"]}
        self.assertIn("zlib_trailer_missing", codes)
        self.assertIn("compressed_block_empty", codes)

    def test_bad_adler_is_warning(self):
        path = self.write("Bad.bsa", build_bsa([("d", "x.bin", b"payload" * 9, "badadler")], 0x7))
        rc, doc = self.run_main(oracle_bsa, [path])
        self.assertEqual(rc, 0)
        self.assertEqual(doc["files"][0]["size"], 63)
        self.assertIn("zlib_checksum_mismatch", {w["code"] for w in doc["warnings"]})

    def test_hash_mismatches_reported(self):
        entries = [("meshes", "a.nif", b"1", "raw"), ("textures", "b.dds", b"2", "raw")]
        data = build_bsa(entries, corrupt={"file": "a.nif", "folder": "textures"})
        rc, doc = self.run_main(oracle_bsa, [self.write("Hash.bsa", data)])
        self.assertEqual(rc, 0)
        codes = [w["code"] for w in doc["warnings"]]
        self.assertIn("file_hash_mismatch", codes)
        self.assertIn("folder_hash_mismatch", codes)
        self.assertEqual(len(doc["files"]), 2)

    def test_file_flags_padding(self):
        path = self.write("Pad.bsa", build_bsa([("d", "a.wav", b"x", "raw")], 0x3, 0x10, 0x0378))
        _, doc = self.run_main(oracle_bsa, [path])
        self.assertEqual(doc["header"]["file_flags"], 0x10)
        self.assertIn("file_flags_padding_nonzero", {w["code"] for w in doc["warnings"]})

    def test_no_content_hash(self):
        path = self.write("N.bsa", build_bsa([("d", "a.txt", b"abc", "zlib")], 0x7))
        _, doc = self.run_main(oracle_bsa, [path, "--no-content-hash"])
        self.assertNotIn("size", doc["files"][0])
        self.assertNotIn("sha256", doc["files"][0])
        self.assertTrue(doc["files"][0]["compressed"])

    def test_garbage_and_truncation_do_not_crash(self):
        self.assertEqual(oracle_bsa.read_listing(b"", "e.bsa").fatal, True)
        listing = oracle_bsa.read_listing(b"NOPE" + bytes(40), "x.bsa")
        self.assertTrue(listing.fatal)
        self.assertEqual(listing.errors[0]["code"], "bad_magic")
        full = build_bsa([("d", "a.txt", b"abc" * 30, "zlib"), ("e", "b.txt", b"q", "raw")], 0x7)
        for cut in range(0, len(full), 7):
            listing = oracle_bsa.read_listing(full[:cut], "cut.bsa")
            doc = listing.document()
            json.dumps(doc)  # always serialisable
            if cut >= 36 and cut < len(full):
                self.assertTrue(doc["errors"] or doc["warnings"], f"cut at {cut} went unreported")

    def test_cli_exit_codes(self):
        script = os.path.join(CAPTURE_DIR, "oracle_bsa.py")
        bad = self.write("bad.bsa", b"not an archive at all, long enough for a header....")
        r = subprocess.run([sys.executable, script, bad], capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script], capture_output=True)
        self.assertEqual(r.returncode, 2)
        good = self.write("good.bsa", build_bsa([("d", "a.txt", b"abc", "raw")]))
        r = subprocess.run([sys.executable, script, good], capture_output=True)
        self.assertEqual(r.returncode, 0)
        self.assertEqual(json.loads(r.stdout.decode("utf-8"))["files"][0]["path"], "d\\a.txt")


class EsmOracleTest(TempDirCase):
    def setUp(self):
        super().setUp()
        self.data, self.n_records, self.n_groups = build_plugin()
        self.path = self.write("Test.esm", self.data)

    def test_inventory(self):
        rc, doc = self.run_main(oracle_esm, ["inventory", self.path])
        self.assertEqual(rc, 0)
        self.assertEqual(doc["schema"], "fnv.esm.inventory/1")
        self.assertEqual(doc["plugin"], "Test.esm")
        self.assertEqual(doc["errors"], [])
        header = doc["header"]
        self.assertEqual(set(header), ESM_HEADER_KEYS)
        self.assertEqual(header["plugin"], "Test.esm")
        self.assertTrue(header["is_master"])
        self.assertEqual(header["flags"], 1)
        self.assertEqual(header["version"], struct.unpack("<f", struct.pack("<f", 1.34))[0])
        self.assertEqual(header["num_records"], self.n_records + self.n_groups)
        self.assertEqual(header["next_object_id"], "0x00000ABC")
        self.assertEqual(header["author"], "tester")
        self.assertIsNone(header["description"])
        self.assertEqual(header["masters"], ["Base.esm"])
        self.assertEqual(header["overridden_forms"], 2)
        self.assertEqual(header["other_subrecords"], ["INTV"])
        self.assertEqual(header["file_size"], len(self.data))

        recs = doc["records"]
        self.assertEqual([r["formid"] for r in recs], ["0x%08X" % (0x0100A000 + i) for i in range(8)])
        self.assertEqual([r["type"] for r in recs],
                         ["GMST", "WEAP", "LAND", "WRLD", "CELL", "REFR", "REFR", "ACTI"])
        gmst, weap, land, wrld, cell, refr1, refr2, acti = recs
        self.assertEqual((gmst["edid"], gmst["full"], gmst["flags"]), ("sTestMessage", None, 0))
        # Compressed record: decoded, first FULL wins, cp1252 decoded, raw flags kept.
        self.assertEqual(weap["edid"], "WeapTest")
        self.assertEqual(weap["full"], "Café Gun’s")
        self.assertEqual(weap["flags"], 0x00040000)
        # Bad Adler-32 but complete stream of the right length: accepted with a warning.
        self.assertEqual(land["flags"], 0x00040000)
        self.assertIn("zlib_checksum_mismatch", {w["code"] for w in doc["warnings"]})
        # XXXX: the FULL after the 70,000-byte OFST is still found.
        self.assertEqual((wrld["edid"], wrld["full"]), ("TestWorld", "Test World"))
        self.assertEqual(cell["edid"], "TestCell")
        self.assertEqual((refr1["edid"], refr1["full"]), (None, None))
        self.assertEqual(refr2["flags"], 0x20)
        self.assertEqual((acti["edid"], acti["full"], acti["form_version"]), ("", "", 7))
        self.assertTrue(all(r["form_version"] == 15 for r in recs[:-1]))

    def test_stats(self):
        rc, doc = self.run_main(oracle_esm, ["stats", self.path])
        self.assertEqual(rc, 0)
        self.assertEqual(doc["schema"], "fnv.esm.stats/1")
        self.assertEqual(doc["records"], self.n_records + 1)
        self.assertEqual(doc["groups"], self.n_groups)
        self.assertEqual(doc["compressed_records"], 2)
        self.assertEqual(doc["deleted_records"], 1)
        self.assertEqual(doc["hedr_num_records"], self.n_records + self.n_groups)
        self.assertIs(doc["hedr_matches_records_plus_groups_minus_one"], True)
        self.assertEqual(doc["top_groups"], ["GMST", "WEAP", "LAND", "WRLD", "CELL", "ACTI"])
        self.assertEqual(doc["by_type"], {"ACTI": 1, "CELL": 1, "GMST": 1, "LAND": 1, "REFR": 2,
                                          "TES4": 1, "WEAP": 1, "WRLD": 1})
        bgt = doc["by_group_type"]
        self.assertEqual(list(bgt), oracle_esm.GROUP_TYPE_NAMES)
        self.assertEqual(bgt["top"], 6)
        self.assertEqual(bgt["interior_cell_block"], 1)
        self.assertEqual(bgt["interior_cell_subblock"], 1)
        self.assertEqual(bgt["cell_children"], 1)
        self.assertEqual(bgt["cell_temporary_children"], 1)
        self.assertEqual(sum(bgt.values()), self.n_groups)

    def test_hedr_mismatch_detected(self):
        data = tes4(99) + group(b"GMST", 0, record(b"GMST", 1, sub(b"EDID", zstr("a"))))
        _, doc = self.run_main(oracle_esm, ["stats", self.write("M.esp", data)])
        self.assertIs(doc["hedr_matches_records_plus_groups_minus_one"], False)
        _, inv = self.run_main(oracle_esm, ["inventory", self.write("M.esp", data)])
        self.assertTrue(inv["header"]["is_master"])

    def test_non_master_and_missing_fields(self):
        data = tes4(0, flags=0)
        _, doc = self.run_main(oracle_esm, ["inventory", self.write("P.esp", data)])
        self.assertFalse(doc["header"]["is_master"])
        self.assertEqual(doc["header"]["overridden_forms"], 0)
        self.assertEqual(doc["records"], [])

    def test_truncation_and_corruption_do_not_crash(self):
        for cut in list(range(0, 200, 3)) + list(range(200, len(self.data), 997)):
            scan = oracle_esm.scan_plugin(self.data[:cut], "cut.esm")
            json.dumps(oracle_esm.inventory_document(scan))
            json.dumps(oracle_esm.stats_document(scan))
            if 0 < cut < len(self.data):
                # Either a structural error, or (cut on a boundary) the HEDR self-check fails.
                stats = oracle_esm.stats_document(scan)
                self.assertTrue(scan.errors or stats["hedr_matches_records_plus_groups_minus_one"] is False,
                                f"cut at {cut} went unreported")
        # A group whose size overruns its parent, and a record with a broken zlib stream.
        broken = record(b"WEAP", 2, b"\x10\x00\x00\x00garbage!", flags=0x00040000)
        bad = tes4(3) + group(b"WEAP", 0, broken)[:-3]
        scan = oracle_esm.scan_plugin(bad + b"\x00\x00\x00", "bad.esm")
        codes = {e["code"] for e in scan.errors}
        self.assertIn("zlib_bad_header", codes)
        self.assertEqual(scan.records[0]["edid"], None)
        scan = oracle_esm.scan_plugin(tes4(1) + group(b"WEAP", 0)[:4] + struct.pack("<I", 5) + bytes(16), "g.esm")
        self.assertIn("group_size_invalid", {e["code"] for e in scan.errors})

    def test_cli_exit_codes(self):
        script = os.path.join(CAPTURE_DIR, "oracle_esm.py")
        r = subprocess.run([sys.executable, script, "inventory", self.path], capture_output=True)
        self.assertEqual(r.returncode, 0)
        self.assertEqual(json.loads(r.stdout.decode("utf-8"))["records"][1]["full"], "Café Gun’s")
        r = subprocess.run([sys.executable, script, "stats", os.path.join(self.tmp, "missing.esm")],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)
        r = subprocess.run([sys.executable, script, "bogus"], capture_output=True)
        self.assertEqual(r.returncode, 2)


if __name__ == "__main__":
    unittest.main()
