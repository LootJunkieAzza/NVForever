"""Tests for tools/capture/xedit/finish_export.py (synthetic data only)."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "xedit"))
import finish_export  # noqa: E402

# What the FNVEdit script writes: escaped quote/backslash/newline, raw non-ASCII text.
EXPORT = (
    '{\n "schema": "fnv.esm.inventory/1",\n "plugin": "T.esm",\n "header": null,\n'
    ' "errors": [],\n "records": [\n'
    ' {"formid": "0x00000800", "type": "WEAP", "edid": "Caf\u00e9\\"q\\\\", '
    '"full": "It\u2019s\\nok\x01", "flags": 0, "form_version": 15}\n ]\n}\n'
)


class FinishExportTests(unittest.TestCase):
    def roundtrip(self, raw: bytes) -> dict:
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "T.xedit.inventory.json"
            p.write_bytes(raw)
            self.assertEqual(finish_export.main([d]), 0)
            out = p.read_bytes()
            out.decode("ascii")  # pure ASCII, hence valid UTF-8
            self.assertEqual(finish_export.main([d]), 0)  # idempotent
            self.assertEqual(p.read_bytes(), out)
            return json.loads(out.decode("utf-8"))

    def check(self, doc: dict) -> None:
        r = doc["records"][0]
        self.assertEqual(r["edid"], 'Caf\u00e9"q\\')
        self.assertEqual(r["full"], "It\u2019s\nok\x01")
        self.assertEqual(r["formid"], "0x00000800")

    def test_ansi_cp1252(self):
        self.check(self.roundtrip(EXPORT.encode("cp1252")))

    def test_utf8_with_and_without_bom(self):
        self.check(self.roundtrip(EXPORT.encode("utf-8")))
        self.check(self.roundtrip(b"\xef\xbb\xbf" + EXPORT.encode("utf-8")))

    def test_utf16(self):
        self.check(self.roundtrip(EXPORT.encode("utf-16")))

    def test_cp1252_undefined_bytes_are_kept(self):
        self.assertEqual(finish_export.decode(b"\x81\x8d\x80"), "\x81\x8d\u20ac")

    def test_rejects_other_documents(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / "X.xedit.inventory.json").write_text('{"schema": "other"}')
            self.assertEqual(finish_export.main([d]), 1)
            self.assertEqual(finish_export.main([str(Path(d) / "empty")]), 1)


if __name__ == "__main__":
    unittest.main()
