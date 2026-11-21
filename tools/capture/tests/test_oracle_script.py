"""Tests for oracle_script.py using tiny synthetic plugins (no game data).

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
REPO = os.path.dirname(os.path.dirname(CAPTURE_DIR))
FUNCTIONS = os.path.join(REPO, "schemas", "script", "functions.json")
sys.path.insert(0, CAPTURE_DIR)

import oracle_script  # noqa: E402


# =========================================================================== builders

def sub(sig, data):
    return sig + struct.pack("<H", len(data)) + data


def zstr(text):
    return text.encode("cp1252") + b"\0"


def record(sig, formid, payload=b"", flags=0, compress=False):
    if compress:
        payload = struct.pack("<I", len(payload)) + zlib.compress(payload)
        flags |= 0x00040000
    return struct.pack("<4sIIIIHH", sig, len(payload), flags, formid, 0, 15, 0) + payload


def group(label, *children):
    body = b"".join(children)
    return struct.pack("<4sI4siHHHH", b"GRUP", 24 + len(body), label, 0, 0, 0, 0, 0) + body


def tes4(masters=()):
    payload = sub(b"HEDR", struct.pack("<fiI", 1.34, 0, 0x800))
    for m in masters:
        payload += sub(b"MAST", zstr(m)) + sub(b"DATA", struct.pack("<Q", 0))
    return record(b"TES4", 0, payload, flags=0x1)


def unit(source, scda=b"\x1d\x00\x00\x00", variables=(), refs=(), locals_=()):
    """SCHR + SCDA + SCTX + SLSD/SCVR + SCRO (FormIDs) + SCRV (variable indices)."""
    out = sub(b"SCHR", struct.pack("<4sIIIHH", b"\0" * 4, len(refs) + len(locals_), len(scda or b""),
                                   len(variables), 0, 1))
    if scda is not None:
        out += sub(b"SCDA", scda)
    if source is not None:
        out += sub(b"SCTX", source.encode("cp1252"))
    for index, name, kind in variables:
        out += sub(b"SLSD", struct.pack("<I12sB7s", index, b"\0" * 12, kind, b"\0" * 7))
        out += sub(b"SCVR", zstr(name))
    for f in refs:
        out += sub(b"SCRO", struct.pack("<I", f))
    for v in locals_:
        out += sub(b"SCRV", struct.pack("<I", v))
    return out


def edid_record(sig, formid, edid, extra=b""):
    return record(sig, formid, sub(b"EDID", zstr(edid)) + extra)


SOURCE = """scn TestScript
short bDone   ; a comment; "not a string"
float fTimer
ref rTarget
====================
begin OnTriggerEnter Player
  if bDone == 0 && TestQuest.iStage >= 2 || GetDead
    set fTimer to -fTimer * 2.50 + .5
    player.StopCombat player
    rTarget.SetAV Lockpick 5
    AddNote "TestNote"
    player.AddItem 0000000f 10
    .Disable
    ShowWarning "a; b"
  elseif (GetStageDone TestQuest 10 == 1)
    set TestQuest.iStage to GameDaysPassed
  else
    PlayGroup Forward 1
  endif
end
"""


def plugin_bytes(compress=False):
    scpt = record(b"SCPT", 0x800, sub(b"EDID", zstr("TestScript")) + unit(
        SOURCE, variables=[(1, "bDone", 1), (2, "fTimer", 0), (3, "rTarget", 0)],
        refs=[0x14, 0x900, 0x0F, 0x910, 0x38], locals_=[3]), compress=compress)
    quest_payload = (sub(b"EDID", zstr("TestQuest")) + sub(b"INDX", struct.pack("<h", 10)) +
                     sub(b"QSDT", b"\0") + unit("SetStage TestQuest 20", scda=b"\x39\x10", refs=[0x900]) +
                     sub(b"QSDT", b"\0") + unit("; only a comment", scda=None) +
                     sub(b"INDX", struct.pack("<h", 20)) + sub(b"QSDT", b"\0") + unit(None, scda=None))
    info = record(b"INFO", 0x920, unit(None, scda=None) + sub(b"NEXT", b"") + unit("player.additem Caps001 5",
                                                                                     scda=b"\x02\x10", refs=[0x14, 0x0F]))
    pack = record(b"PACK", 0x930, sub(b"EDID", zstr("P")) +
                  sub(b"POBA", b"") + sub(b"INAM", b"\0" * 4) + unit("EvaluatePackage", scda=b"\x5e\x10") +
                  sub(b"TNAM", b"\0" * 4) + sub(b"POEA", b"") + sub(b"INAM", b"\0" * 4) + unit(None, scda=None) +
                  sub(b"POCA", b"") + unit("Disable", scda=b"\x22\x10"))
    term = record(b"TERM", 0x940, sub(b"ITXT", zstr("a")) + unit(None, scda=None) + sub(b"ITXT", zstr("b")) +
                  unit("Enable", scda=b"\x21\x10"))
    perk = record(b"PERK", 0x950, sub(b"PRKE", b"\2\0\0") + unit("ResetAI", scda=b"\xfa\x11"))
    refr = record(b"REFR", 0x960, sub(b"NAME", struct.pack("<I", 0x20)) + sub(b"XPPA", b"") +
                  unit("Disable", scda=b"\x22\x10"))
    forms = (edid_record(b"NOTE", 0x910, "TestNote") + edid_record(b"MISC", 0x0F, "Caps001") +
             edid_record(b"GLOB", 0x38, "GameDaysPassed"))
    return b"".join([tes4(), group(b"SCPT", scpt), group(b"QUST", record(b"QUST", 0x900, quest_payload)),
                     group(b"INFO", info), group(b"PACK", pack), group(b"TERM", term), group(b"PERK", perk),
                     group(b"REFR", refr), group(b"MISC", forms)])


def dlc_bytes():
    """A plugin with a master: its stored FormIDs 0x00xxxxxx name the master's forms."""
    scpt = record(b"SCPT", 0x01000800, sub(b"EDID", zstr("DlcScript")) + unit(
        "scn DlcScript\nbegin GameMode\nset TestQuest.iStage to 1\nend", refs=[0x900]))
    return b"".join([tes4(["Base.esm"]), group(b"SCPT", scpt)])


class Fixture:
    def __init__(self, compress=False):
        self.tmp = tempfile.TemporaryDirectory()
        with open(os.path.join(self.tmp.name, "Base.esm"), "wb") as fh:
            fh.write(plugin_bytes(compress))
        with open(os.path.join(self.tmp.name, "Dlc.esm"), "wb") as fh:
            fh.write(dlc_bytes())

    def run(self, plugins=("Base.esm",)):
        try:
            return oracle_script.decompile(self.tmp.name, list(plugins), FUNCTIONS)
        finally:
            self.tmp.cleanup()


# =========================================================================== tests

class Units(unittest.TestCase):
    def test_owner_labels_and_listing(self):
        doc = Fixture().run()
        self.assertEqual(doc["format"], "fnv.script.decompile/1")
        labels = [u["unit"] for u in doc["units"]]
        self.assertEqual(labels, [
            "Base.esm|SCPT 0x00000800",
            "Base.esm|QUST 0x00000900 stage 10 entry 0",
            "Base.esm|QUST 0x00000900 stage 10 entry 1",       # source only: no text
            "Base.esm|INFO 0x00000920 end",
            "Base.esm|PACK 0x00000930 begin",
            "Base.esm|PACK 0x00000930 change",
            "Base.esm|TERM 0x00000940 item 1",
            "Base.esm|PERK 0x00000950 entry 0",
            "Base.esm|REFR 0x00000960 patrol",
        ])
        self.assertNotIn("text", doc["units"][2])
        self.assertEqual(doc["summary"]["units_found"], 13)

    def test_compressed_records_and_masters(self):
        doc = Fixture(compress=True).run(("Base.esm", "Dlc.esm"))
        rows = {u["unit"]: u for u in doc["units"]}
        self.assertIn("Base.esm|SCPT 0x00000800", rows)
        self.assertEqual(rows["Dlc.esm|SCPT 0x01000800"]["text"],
                         ["scriptname", "begin gamemode", "set 0x00000900.istage to 1", "end"])


class Text(unittest.TestCase):
    def setUp(self):
        self.rows = {u["unit"]: u for u in Fixture().run()["units"]}

    def test_canonical_script(self):
        self.assertEqual(self.rows["Base.esm|SCPT 0x00000800"]["text"], [
            "scriptname",
            "begin ontriggerenter 0x00000014",
            # || binds tighter than && (section 3.3).
            "if bdone 0 == 0x00000900.istage 2 >= getdead() || &&",
            "set ftimer to ftimer ~ 2.5 * 0.5 +",
            "0x00000014.stopcombat",                    # surplus argument dropped
            "rtarget.setactorvalue lockpick 5",          # actor value, not a form
            "addnote 0x00000910",                        # quoted Editor ID
            "0x00000014.additem 0x0000000F 10",          # FormID written in hex
            "disable",
            "showwarning \"a; b\"",
            "elseif getstagedone(0x00000900 10) 1 ==",
            "set 0x00000900.istage to 0x00000038",
            "else",
            "playgroup forward 1",
            "endif",
            "end",
        ])

    def test_result_scripts(self):
        self.assertEqual(self.rows["Base.esm|QUST 0x00000900 stage 10 entry 0"]["text"],
                         ["setstage 0x00000900 20"])
        self.assertEqual(self.rows["Base.esm|INFO 0x00000920 end"]["text"], ["0x00000014.additem 0x0000000F 5"])
        self.assertEqual(self.rows["Base.esm|PACK 0x00000930 begin"]["text"], ["evaluatepackage"])

    def test_numbers(self):
        self.assertEqual(oracle_script.number("19.50"), "19.5")
        self.assertEqual(oracle_script.number("3.0"), "3")
        self.assertEqual(oracle_script.number(".002"), "0.002")
        self.assertEqual(oracle_script.number("0.0000001"), "0.0000001")
        self.assertEqual(oracle_script.word("Corpses Eaten"), '"corpses eaten"')


class Cli(unittest.TestCase):
    def run_cli(self, *args):
        return subprocess.run([sys.executable, os.path.join(CAPTURE_DIR, "oracle_script.py"), *args],
                              capture_output=True, text=True)

    def test_usage_error(self):
        self.assertEqual(self.run_cli().returncode, 2)

    def test_unreadable_input(self):
        with tempfile.TemporaryDirectory() as d:
            r = self.run_cli("decompile", d, "Missing.esm", "--functions", FUNCTIONS)
            self.assertEqual(r.returncode, 1)

    def test_writes_document(self):
        f = Fixture()
        self.addCleanup(f.tmp.cleanup)
        out = os.path.join(f.tmp.name, "out.json")
        r = self.run_cli("decompile", f.tmp.name, "Base.esm", "--functions", FUNCTIONS, "-o", out)
        self.assertEqual(r.returncode, 0, r.stderr)
        with open(out, encoding="utf-8") as fh:
            self.assertEqual(json.load(fh)["count"], 9)


if __name__ == "__main__":
    unittest.main()
