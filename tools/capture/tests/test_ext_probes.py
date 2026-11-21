"""Tests for tools/capture/ext_probes.py: a hand-made log and README, no game data."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import ext_probes as ep  # noqa: E402

LOG = """\
[12:00:01] some unrelated console line
EXTPROBE|strings/01_length_find|begin
EXTPROBE|len.basic|5
console> EXTPROBE|find.first|1.000000
EXTPROBE|replace.result|[a+b|c]
EXTPROBE|len.empty|0
EXTPROBE|len.basic|5
EXTPROBE|find.first|2
Error in script ExtProbeStrings01: something went wrong
EXTPROBE|strings/01_length_find|end
EXTPROBE|strings/02_edit|begin
EXTPROBE|extra.id|x
"""

README = """\
# Probes

| File | Id | Expression | Expected | Evidence |
|---|---|---|---|---|
| 01 | `len.basic` | `Sv_Length s` | `5` | [documented: Sv Length] |
| 01 | `len.empty` | `Sv_Length s` | observe | — |
| 01 | `find.first` | `Sv_Find "e", s` | `1` | [documented: Sv Find] |
| 02 | `replace.result` | `Sv_Replace "-\\|+", s` | `[a+b\\|c]` | [documented: Sv Replace] |
| 02 | `replace.max1` | `…` | `[a+b-c]` | [documented: Sv Replace] |
| 03 | `tonumber.bad` | `#s` | to observe (anything) | — |
"""


class Collect(unittest.TestCase):
    def test_lines_markers_duplicates(self):
        rows, files, dups = ep.parse_lines(LOG.splitlines(True))
        got = {r["id"]: r["value"] for r in rows}
        self.assertEqual([r["id"] for r in rows], ["len.basic", "find.first", "replace.result", "len.empty", "extra.id"])
        self.assertEqual(got["find.first"], "2")            # the last value wins
        self.assertEqual(got["replace.result"], "[a+b|c]")  # a value may hold |
        self.assertEqual(dups, [{"id": "find.first", "values": ["1.000000", "2"]}])
        self.assertEqual(files, [{"name": "strings/01_length_find", "begun": True, "ended": True},
                                 {"name": "strings/02_edit", "begun": True, "ended": False}])

    def test_main_writes_the_document(self):
        with tempfile.TemporaryDirectory() as d:
            log = os.path.join(d, "console.txt")
            with open(log, "w", encoding="utf-8") as f:
                f.write(LOG)
            out = os.path.join(d, "ext", "strings.json")
            self.assertEqual(ep.main(["collect", log, "--set", "strings", "--version", "6.4.9", "-o", out]), 0)
            with open(out, encoding="utf-8") as f:
                doc = json.load(f)
            self.assertEqual(doc["format"], "fnv.ext.probes/1")
            self.assertEqual((doc["set"], doc["version"], doc["sources"]), ("strings", "6.4.9", ["console.txt"]))
            self.assertEqual(len(doc["rows"]), 5)

    def test_default_goldens_honours_the_environment(self):
        old = os.environ.get("FNV_GOLDENS")
        os.environ["FNV_GOLDENS"] = "/tmp/g"
        try:
            self.assertEqual(str(ep.default_goldens()).replace("\\", "/"), "/tmp/g")
        finally:
            if old is None:
                del os.environ["FNV_GOLDENS"]
            else:
                os.environ["FNV_GOLDENS"] = old


class Compare(unittest.TestCase):
    def test_readme_table(self):
        exp = ep.readme_expected(README)
        self.assertEqual(exp["len.basic"], "5")
        self.assertIsNone(exp["len.empty"])
        self.assertIsNone(exp["tonumber.bad"])
        self.assertEqual(exp["replace.result"], "[a+b|c]")

    def test_numbers_by_value_text_exactly(self):
        self.assertTrue(ep.same("5", "5.000000"))
        self.assertFalse(ep.same("1", "2"))
        self.assertTrue(ep.same("[ab]", "[ab]"))
        self.assertFalse(ep.same("[ab]", "[AB]"))

    def test_report(self):
        rows, files, _ = ep.parse_lines(LOG.splitlines(True))
        res = ep.compare({"set": "strings", "rows": rows, "files": files}, ep.readme_expected(README))
        self.assertEqual(res["checked"], 4)
        self.assertEqual(res["matched"], 2)                  # len.basic, replace.result
        self.assertEqual(res["mismatches"], [{"id": "find.first", "expected": "1", "got": "2"}])
        self.assertEqual(res["missing"], ["replace.max1"])
        self.assertEqual(res["observe"], [{"id": "len.empty", "got": "0"}, {"id": "tonumber.bad", "got": None}])
        self.assertEqual(res["unknown"], ["extra.id"])
        self.assertEqual(res["files_unfinished"], ["strings/02_edit"])

    def test_main_exit_code(self):
        with tempfile.TemporaryDirectory() as d:
            run, readme = os.path.join(d, "run.json"), os.path.join(d, "README.md")
            rows, files, _ = ep.parse_lines(LOG.splitlines(True))
            with open(run, "w", encoding="utf-8") as f:
                json.dump({"set": "strings", "rows": rows, "files": files}, f)
            with open(readme, "w", encoding="utf-8") as f:
                f.write(README)
            self.assertEqual(ep.main(["compare", run, readme]), 1)


if __name__ == "__main__":
    unittest.main()
