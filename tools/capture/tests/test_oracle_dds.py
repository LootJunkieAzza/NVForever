"""Tests for oracle_dds.py using synthetic DDS files packed into synthetic BSAs.

The DDS builder follows docs/spec-notes/dds.md section 1; no game data is used.

    python3 -m unittest discover -s tools/capture/tests -v
"""
import json
import os
import struct
import subprocess
import sys
import unittest

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

import oracle_dds  # noqa: E402
from test_oracles import TempDirCase, build_bsa  # noqa: E402


def build_dds(width, height, mips, pf_flags, fourcc=b"\0\0\0\0", bits=0, masks=(0, 0, 0, 0),
              caps2=0, data_size=0, flags=0x000A1007):
    header = [124, flags, height, width, 0, 0, mips] + [0] * 11
    header += [32, pf_flags, struct.unpack("<I", fourcc)[0], bits, *masks]
    header += [0x401008, caps2, 0, 0, 0]
    assert len(header) == 31
    return b"DDS " + struct.pack("<31I", *header) + bytes(data_size)


ARGB = (0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000)


class DdsEntryTest(unittest.TestCase):
    def test_dxt1_with_mips(self):
        # 256x64: levels 256x64, 128x32, 64x16, 32x8, 16x4, 8x2, 4x1, 2x1, 1x1
        expected = 8 * (64 * 16 + 32 * 8 + 16 * 4 + 8 * 2 + 4 * 1 + 2 * 1 + 1 + 1 + 1)
        e = oracle_dds.dds_entry("textures\\a.dds", build_dds(256, 64, 9, 0x4, b"DXT1",
                                                              data_size=expected))
        self.assertEqual(e, {"path": "textures\\a.dds", "width": 256, "height": 64,
                             "mip_count": 9, "levels": 9, "format": "DXT1", "cubemap": False,
                             "faces": 1, "expected_size": expected, "actual_size": expected,
                             "error": None})

    def test_dxt5_and_dxt3(self):
        for cc in (b"DXT3", b"DXT5"):
            e = oracle_dds.dds_entry("b.dds", build_dds(8, 8, 2, 0x4, cc, data_size=80))
            self.assertEqual((e["format"], e["expected_size"]), (cc.decode(), 16 * 4 + 16))

    def test_uncompressed_formats_and_zero_mips(self):
        e = oracle_dds.dds_entry("c.dds", build_dds(4, 2, 0, 0x41, bits=32, masks=ARGB,
                                                    data_size=32, flags=0x1007))
        self.assertEqual((e["format"], e["mip_count"], e["levels"], e["expected_size"]),
                         ("A8R8G8B8", 0, 1, 32))
        e = oracle_dds.dds_entry("d.dds", build_dds(4, 4, 3, 0x40, bits=24,
                                                    masks=(0xFF0000, 0xFF00, 0xFF, 0)))
        self.assertEqual((e["format"], e["expected_size"], e["actual_size"]),
                         ("R8G8B8", 3 * (16 + 4 + 1), 0))
        e = oracle_dds.dds_entry("e.dds", build_dds(2, 2, 2, 0x41, bits=16,
                                                    masks=(0x0F00, 0x00F0, 0x000F, 0xF000)))
        self.assertEqual((e["format"], e["expected_size"]), ("A4R4G4B4", 2 * (4 + 1)))

    def test_cubemap(self):
        per_face = 8 * (4 + 1 + 1)  # 8x8 DXT1: 2x2 blocks, 4x4 -> 1, 2x2 -> 1
        e = oracle_dds.dds_entry("f_e.dds", build_dds(8, 8, 3, 0x4, b"DXT1", caps2=0xFE00,
                                                      data_size=6 * per_face))
        self.assertEqual((e["cubemap"], e["faces"], e["expected_size"], e["actual_size"]),
                         (True, 6, 6 * per_face, 6 * per_face))

    def test_unknown_format(self):
        e = oracle_dds.dds_entry("g.dds", build_dds(4, 4, 1, 0x4, b"ATI2", data_size=16))
        self.assertTrue(e["format"].startswith("unknown(flags=0x4,fourcc=ATI2,bits=0,masks="))
        self.assertIsNone(e["expected_size"])
        self.assertEqual(e["actual_size"], 16)

    def test_truncated_and_invalid(self):
        good = build_dds(16, 16, 1, 0x4, b"DXT1", data_size=128)
        e = oracle_dds.dds_entry("h.dds", good[:-28])
        self.assertIsNone(e["error"])
        self.assertEqual((e["expected_size"], e["actual_size"]), (128, 100))
        for data, message in ((good[:100], "short header"), (b"XXXX" + good[4:], "bad magic"),
                              (b"", "bad magic")):
            e = oracle_dds.dds_entry("i.dds", data)
            self.assertEqual(e["error"], message)
            self.assertTrue(all(e[k] is None for k in e if k not in ("path", "error")))


class DdsArchiveTest(TempDirCase):
    def test_cli_with_embedded_paths(self):
        tex = build_dds(4, 4, 1, 0x4, b"DXT1", data_size=8)
        files = [("textures\\a", "x.dds", tex), ("textures\\a", "y.dds", tex[:50]),
                 ("textures\\b", "n.txt", b"hi")]
        entries = []
        for folder, name, data in files:
            full = f"{folder}\\{name}".encode()
            entries.append((folder, name, bytes([len(full)]) + full + data, "raw"))
        path = self.write("Tex.bsa", build_bsa(entries, archive_flags=0x103))
        script = os.path.join(CAPTURE_DIR, "oracle_dds.py")
        out = os.path.join(self.tmp, "out.json")
        r = subprocess.run([sys.executable, script, "list", path, "--out", out], capture_output=True)
        self.assertEqual(r.returncode, 0, r.stderr)
        with open(out, encoding="utf-8") as f:
            doc = json.load(f)
        self.assertEqual((doc["schema"], doc["archive"]), ("fnv.dds.listing/1", "Tex.bsa"))
        by_path = {e["path"]: e for e in doc["files"]}
        self.assertEqual(set(by_path), {"textures\\a\\x.dds", "textures\\a\\y.dds"})
        x = by_path["textures\\a\\x.dds"]
        self.assertEqual((x["format"], x["expected_size"], x["actual_size"], x["error"]),
                         ("DXT1", 8, 8, None))
        self.assertEqual(by_path["textures\\a\\y.dds"]["error"], "short header")
        r = subprocess.run([sys.executable, script, "list", self.write("bad.bsa", b"")],
                           capture_output=True)
        self.assertEqual(r.returncode, 1)


if __name__ == "__main__":
    unittest.main()
