"""Tests for script_functions.py using synthetic wikitext, bytecode and plugins.

No game data or wiki text is used: the pages and scripts below are made up and only follow
the shapes described in docs/spec-notes/script-functions.md. Run from the repo root:

    python3 -m unittest discover -s tools/capture/tests -v
"""
import os
import struct
import sys
import tempfile
import unittest
import zlib

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)

import script_functions as sf  # noqa: E402

PAGE = """{{Function
 |origin = GECK1
 |summary = Made-up summary.
 |name = FrobWidget
 |alias = Frob
 |referenceType = <nowiki>ActorID|ContainerID</nowiki>
 |returnType = int
 |arguments =
  {{FunctionArgument
   |Name = Target
   |Type = ref
  }}{{FunctionArgument
   |Name = Count
   |Type = int
   |Optional = y
  }}
 |example = player.FrobWidget Thing 2
}}
[[Category:Functions (GECK)]]
[[Category:Widget_Functions]]
"""


def stmt(op, body=b""):
    return struct.pack("<HH", op, len(body)) + body


def call_expr(op, params=b"", count=0):
    payload = struct.pack("<H", count) + params
    return b"X" + struct.pack("<HH", op, len(payload)) + payload


def subrecord(sig, data):
    return sig + struct.pack("<H", len(data)) + data


def record(sig, form_id, data, compress=False):
    flags = 0
    if compress:
        data = struct.pack("<I", len(data)) + zlib.compress(data)
        flags = 0x00040000
    return sig + struct.pack("<IIIIHH", len(data), flags, form_id, 0, 15, 0) + data


def group(label, payload):
    return b"GRUP" + struct.pack("<I", 24 + len(payload)) + label + struct.pack(
        "<iHHHH", 0, 0, 0, 0, 0) + payload


class WikiParsing(unittest.TestCase):
    def test_function_template(self):
        rec = sf.parse_page("FrobWidget", PAGE)
        self.assertEqual(rec["name"], "FrobWidget")
        self.assertEqual(rec["origin"], "GECK1")
        self.assertEqual(rec["aliases"], ["Frob"])
        self.assertEqual(rec["calling_reference"], "ActorID|ContainerID")
        self.assertEqual(rec["returns"], {"type": "int", "name": None})
        self.assertEqual(rec["params"], [
            {"name": "Target", "type": "ref", "kind": "reference"},
            {"name": "Count", "type": "int", "kind": "int", "optional": True}])
        self.assertEqual(rec["categories"], ["Functions (GECK)", "Widget Functions"])

    def test_page_without_template_is_skipped(self):
        self.assertIsNone(sf.parse_page("Category:Stuff", "[[Category:Functions]]"))

    def test_arg_kinds(self):
        for raw, kind in [("float", "float"), ("0/1", "int"), ("x/y/z", "axis"),
                          ("StatName", "actor_value"), ("baseform", "form"),
                          ("string_var", "string"), (None, "unknown"), ("pair", "other")]:
            self.assertEqual(sf.arg_kind(raw), kind, raw)

    def test_xedit_table(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "defs.pas")
            with open(p, "w") as fh:
                fh.write("    (Index:   1; Name: 'GetDistance'; ParamType1: ptReference),\n"
                         "    (Index:   5; Name: 'GetLocked'),\n")
            self.assertEqual(sf.parse_xedit(p), {1: ("GetDistance", ["ptReference"]),
                                                 5: ("GetLocked", [])})

    def test_xedit_table_from_catalogue(self):
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "functions.json")
            with open(p, "w") as fh:
                fh.write('{"functions": [{"name": "GetDistance", "condition_index": 1,'
                         ' "xedit_param_types": ["ptReference"]},'
                         ' {"name": "GetLocked", "condition_index": 5},'
                         ' {"name": "Frob"}]}')
            self.assertEqual(sf.xedit_from_catalogue(p), {1: ("GetDistance", ["ptReference"]),
                                                          5: ("GetLocked", [])})


class Bytecode(unittest.TestCase):
    def test_statements_with_reference_prefix(self):
        code = stmt(0x1D) + struct.pack("<HH", 0x1C, 1) + stmt(0x1022, b"\0\0") + stmt(0x11)
        self.assertEqual(sf.statements(code),
                         [(0x1D, b"", False), (0x1022, b"\0\0", True), (0x11, b"", False)])

    def test_truncated_statement_is_malformed(self):
        self.assertIsNone(sf.statements(stmt(0x1D) + b"\x10\x00\x09"))
        self.assertIsNone(sf.statements(b"\x10\x00\x09\x00\x00"))

    def test_expression_calls(self):
        body = b"\x00\x00" + b" " + call_expr(0x1023) + b" 0 =="
        self.assertEqual(sf.expression_calls(body), [0x1023])

    def test_source_lines_skip_comments_and_declarations(self):
        src = "scn Foo\n; note\nshort bDone\n\nBegin GameMode ; c\n  Disable\nEnd\n"
        self.assertEqual(sf.source_lines(src), ["scn Foo", "Begin GameMode", "Disable", "End"])


class EndToEnd(unittest.TestCase):
    def test_scan_and_build(self):
        src = ("scn TestScript\nshort b\nBegin OnActivate\n"
               "if (FrobWidget == 0)\n  player.Frob Thing\nendif\nMysteryCall\nEnd\n")
        expr = struct.pack("<HH", 0, 0) + b" " + call_expr(0x1100) + b" 0 =="
        code = (stmt(0x1D) + stmt(0x10, struct.pack("<HI", 2, 0)) + stmt(0x16, expr)
                + struct.pack("<HH", 0x1C, 1) + stmt(0x1100, b"\0\0") + stmt(0x19)
                + stmt(0x1200, b"\0\0") + stmt(0x11))
        scpt = (subrecord(b"EDID", b"TestScript\0") + subrecord(b"SCHR", bytes(20))
                + subrecord(b"SCDA", code) + subrecord(b"SCTX", src.encode()))
        ctda = bytes(8) + struct.pack("<HH", 0x100, 0) + bytes(16)
        perk = subrecord(b"CTDA", ctda)
        plugin = (record(b"TES4", 0, b"")
                  + group(b"SCPT", record(b"SCPT", 0x800, scpt, compress=True))
                  + group(b"PERK", record(b"PERK", 0x801, perk)))
        wiki = [sf.parse_page("FrobWidget", PAGE)]
        xedit = {0x100: ("FrobWidget", ["ptReference"])}
        with tempfile.TemporaryDirectory() as d:
            with open(os.path.join(d, "FalloutNV.esm"), "wb") as fh:
                fh.write(plugin)
            scan = sf.scan_masters(d, sf.canonical_map(wiki, xedit))
        self.assertEqual(dict(scan["stats"]), {"scripts": 1, "aligned": 1})
        self.assertEqual(scan["ctda"], {0x100: 1})
        self.assertEqual(dict(scan["begin"][2]), {"OnActivate": 1})
        doc = sf.build(wiki, scan, xedit, {"FrobWidget": {"summary": "Own words."}})
        by_name = {f["name"]: f for f in doc["functions"]}
        frob = by_name["FrobWidget"]
        self.assertEqual(frob["opcodes"], ["0x1100"])
        self.assertEqual(frob["opcode_evidence"], "observed")
        self.assertEqual(frob["condition_index"], 0x100)
        self.assertEqual(frob["condition_uses"], 1)
        self.assertEqual(frob["script_calls"], 2)
        self.assertEqual(frob["summary"], "Own words.")
        self.assertEqual(frob["family"], "vanilla")
        mystery = by_name["MysteryCall"]
        self.assertEqual(mystery["origin"], "observed-only")
        self.assertEqual(mystery["opcodes"], ["0x1200"])
        self.assertEqual(doc["anomalies"], [])
        self.assertIn("| ref.[FrobWidget]", sf.render_md(doc))


if __name__ == "__main__":
    unittest.main()
