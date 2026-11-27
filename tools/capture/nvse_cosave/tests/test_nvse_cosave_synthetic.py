"""Offline pure-synthetic unit tests for `tools/capture/nvse_cosave/`.

Requires zero game files and zero captured `.nvse` saves. Builds minimal NVSE v1
co-save buffers in memory and verifies parser, differ, verifier, and read-only guards.
"""

from __future__ import annotations

import struct
import sys
import tempfile
import unittest
from pathlib import Path

_PKG_DIR = Path(__file__).resolve().parents[1]
if str(_PKG_DIR) not in sys.path:
    sys.path.insert(0, str(_PKG_DIR))

import campaigns  # type: ignore[import-not-found]
import harness  # type: ignore[import-not-found]
import parser  # type: ignore[import-not-found]
import verify  # type: ignore[import-not-found]


def _chunk(tag: bytes, version: int, payload: bytes) -> bytes:
    assert len(tag) == 4
    return tag + struct.pack("<II", version, len(payload)) + payload


def _plugin(opcode: int, chunks: list[bytes]) -> bytes:
    body = b"".join(chunks)
    return struct.pack("<III", opcode, len(chunks), len(body)) + body


def _cosave(plugins: list[bytes], nvse_ver: int = 0x00040006, rt_ver: int = 0x040020D0) -> bytes:
    return b"NVSE" + struct.pack("<IIII", 1, nvse_ver, rt_ver, len(plugins)) + b"".join(plugins)


def _sdom(mods: list[str]) -> bytes:
    buf = bytearray([len(mods)])
    for m in mods:
        raw = m.encode("ascii")
        buf += struct.pack("<H", len(raw)) + raw
    return _chunk(b"SDOM", 0, bytes(buf))


class TestNvseCosaveSynthetic(unittest.TestCase):
    def test_parse_all_observed_chunks_and_unknown_passthrough(self) -> None:
        # RVRA stringmap with nested array id, float, form, >255B string
        long_str = ("X" * 260).encode("ascii")
        rvra = bytearray()
        rvra += struct.pack("<BI", 0xFF, 2)
        rvra += struct.pack("<BB", 3, 0)  # stringmap
        rvra += struct.pack("<I", 1) + bytes([0xFF])
        rvra += struct.pack("<I", 4)
        rvra += struct.pack("<H", 1) + b"a" + struct.pack("<BI", 4, 1)
        rvra += struct.pack("<H", 1) + b"n" + struct.pack("<Bd", 1, -3.5)
        rvra += struct.pack("<H", 1) + b"r" + struct.pack("<BI", 2, 0x00104C0F)
        rvra += struct.pack("<H", 1) + b"s" + struct.pack("<BH", 3, len(long_str)) + long_str

        # JPAV
        jpav = (
            struct.pack("<H", 1)
            + struct.pack("<BH", 0x01, 1)
            + struct.pack("<IH", 0x14, 1)
            + struct.pack("<B", 4)
            + b"_aux"
            + struct.pack("<H", 2)
            + struct.pack("<Bd", 1, 12.25)
            + struct.pack("<BH", 4, 2)
            + b"hi"
        )
        # JPRM
        jprm = (
            struct.pack("<H", 1)
            + struct.pack("<BH", 0xFF, 1)
            + struct.pack("<B", 3)
            + b"_rm"
            + struct.pack("<H", 1)
            + struct.pack("<IBI", 0x7, 2, 0x000E3778)
        )
        # JPLR
        jplr = struct.pack("<H", 1) + struct.pack("<IIB", 0x00104C0F, 0x001062AC, 0xFF)
        # JPSV (1 float + 1 form)
        jpsv = (
            struct.pack("<H", 1)
            + struct.pack("<IH", 0x00080664, 2)
            + struct.pack("<BB", 0xFF, 2)
            + b"vF"
            + struct.pack("<d", 99.5)
            + struct.pack("<BB", 0xFF, 2)
            + b"vR"
            + struct.pack("<II", 0x001062AC, 0)
        )
        # SMGJ
        smgj = struct.pack("<H", 1) + struct.pack("<H", 4) + b"Stat" + struct.pack("<i", -7)
        # OSMS
        osms = (
            struct.pack("<H", 1)
            + struct.pack("<BH", 0xFF, 1)
            + struct.pack("<B", 3)
            + b"_sm"
            + struct.pack("<H", 1)
            + struct.pack("<B", 1)
            + b"k"
            + struct.pack("<Bd", 1, 4.5)
        )
        # SOAT
        soat = (
            struct.pack("<H", 1)
            + struct.pack("<BH", 0xFF, 1)
            + struct.pack("<IH", 0x14, 1)
            + struct.pack("<B", 2)
            + b"_t"
            + struct.pack("<ddI", 30.0, 12.5, 4)
        )

        raw = _cosave(
            [
                _plugin(
                    parser.PLUGIN_NVSE,
                    [
                        _sdom(["FalloutNV.esm", "TestMod.esp"]),
                        _chunk(b"SVRA", 2, b""),
                        _chunk(b"RVRA", 2, bytes(rvra)),
                        _chunk(b"EVRA", 2, b""),
                        _chunk(b"SVTS", 0, b""),
                        _chunk(b"EVTS", 0, b""),
                    ],
                ),
                _plugin(
                    parser.PLUGIN_JIP,
                    [
                        _chunk(b"JPAV", 10, jpav),
                        _chunk(b"JPRM", 10, jprm),
                        _chunk(b"JPLR", 9, jplr),
                        _chunk(b"JPSV", 9, jpsv),
                        _chunk(b"JPGF", 1, b"\x00\x00\x00\x00\x00\x00\x80\x3f"),
                    ],
                ),
                _plugin(parser.PLUGIN_JOHNNYGUITAR, [_chunk(b"SMGJ", 1, smgj)]),
                _plugin(parser.PLUGIN_SHOWOFF, [_chunk(b"OSMS", 10, osms), _chunk(b"SOAT", 1, soat)]),
                _plugin(0x00009999, [_chunk(b"UNKN", 3, b"\x01\x02\x03")]),
            ]
        )

        c = parser.NvseCosave.parse(raw)
        self.assertEqual(c.mod_list(), ["FalloutNV.esm", "TestMod.esp"])
        arrs = c.arrays()
        self.assertEqual(len(arrs), 1)
        self.assertEqual(arrs[0]["kind"], "stringmap")
        self.assertEqual(arrs[0]["entries"][3]["val"]["value"], "X" * 260)
        self.assertEqual(len(c.jip_aux_vars()), 1)
        self.assertEqual(len(c.jip_refmaps()), 1)
        self.assertEqual(c.jip_linked_refs(), [{"ref": 0x00104C0F, "linked": 0x001062AC, "mod_index": 0xFF}])
        self.assertEqual(c.jip_added_script_vars()[0]["val"], {"type": "num", "value": 99.5})
        self.assertEqual(c.jip_added_script_vars()[1]["val"], {"type": "form", "value": 0x001062AC})
        self.assertEqual(c.jg_extra_misc_stats(), [{"name": "Stat", "value": -7}])
        self.assertEqual(len(c.showoff_strmaps()), 1)
        self.assertEqual(c.showoff_aux_timers()[0]["flags"], 4)
        unk = c.unknown_chunks()
        self.assertEqual(len(unk), 2)
        self.assertEqual(unk[0]["tag"], "JPGF")
        self.assertEqual(unk[1]["tag"], "UNKN")

    def test_diff_cosaves_detects_added_chunk(self) -> None:
        a = parser.NvseCosave.parse(_cosave([_plugin(parser.PLUGIN_NVSE, [_sdom(["FalloutNV.esm"])])]))
        b = parser.NvseCosave.parse(
            _cosave(
                [
                    _plugin(parser.PLUGIN_NVSE, [_sdom(["FalloutNV.esm"])]),
                    _plugin(
                        parser.PLUGIN_JOHNNYGUITAR,
                        [_chunk(b"SMGJ", 1, struct.pack("<HH", 1, 1) + b"X" + struct.pack("<i", 5))],
                    ),
                ]
            )
        )
        d = parser.diff_cosaves(a, b)
        self.assertEqual(len(d["added"]), 1)
        self.assertEqual(d["added"][0]["chunk"], "0x00003100/SMGJ v1")

    def test_rejects_bad_magic_and_truncation(self) -> None:
        with self.assertRaises(parser.NvseParseError):
            parser.NvseCosave.parse(b"NOPE" + b"\x00" * 16)
        with self.assertRaises(parser.NvseParseError):
            parser.NvseCosave.parse(b"NVSE" + struct.pack("<IIII", 1, 0, 0, 1))

    def test_harness_stage_and_readonly_boundary_guard(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            fake_game = root / "FakeFNV"
            fake_game.mkdir()
            (fake_game / "FalloutNV.exe").write_bytes(b"")
            with self.assertRaises(harness.ReadOnlyBoundaryError):
                harness.assert_safe_out_dir(fake_game / "sub", game_dir=fake_game)

            stage_dir = root / "stage_out"
            loop_p, gr_p, guide_p = harness.stage_campaign_scripts(
                stage_dir,
                campaigns.COSAVE_CORE_JIP,
                game_dir=fake_game,
            )
            self.assertTrue(loop_p.is_file())
            self.assertTrue(gr_p.is_file())
            self.assertTrue(guide_p.is_file())
            self.assertIn("GetFormFromMod", (stage_dir / "Data/NVSE/user_defined_functions/cosaveprobe_s_04_c02_arr_list_inline.gek").read_text("ascii"))

    def test_verify_dir_on_minimal_synthetic_c23(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            p = Path(tmp)
            raw = _cosave(
                [
                    _plugin(parser.PLUGIN_NVSE, [_sdom(["FalloutNV.esm"])]),
                    _plugin(
                        parser.PLUGIN_JIP,
                        [_chunk(b"JPLR", 9, struct.pack("<HIIB", 1, 0x00104C0F, 0x001062AC, 0xFF))],
                    ),
                ]
            )
            (p / "c23_jip_linkedref.nvse").write_bytes(raw)
            passed, failed, _ = verify.verify_dir(p)
            self.assertEqual(passed, 1)
            self.assertEqual(failed, 0)


if __name__ == "__main__":
    unittest.main()
