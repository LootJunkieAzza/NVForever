"""Tests for tools/capture/observe_compare.py: timelines written by hand, no game."""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import observe_compare as oc  # noqa: E402


def seg(session, t0, t1, package, proc="SANDBOX", path=10.0, cell="TownCell"):
    return {"session": session, "from": t0, "to": t1, "path": path,
            "state": {"package_edid": package, "procedure_name": proc, "cell_edid": cell}}


def tl(actors):
    return {"format": "fnv.observe.timeline/1",
            "actors": {aid: {"edid": edid, "segments": segs} for aid, (edid, segs) in actors.items()}}


def bob(sleep_until, extra=()):
    return ("BobREF", [seg("day", 0, sleep_until, "BobSleep", "SLEEP"), seg("day", sleep_until, 24, "BobSandbox")]
            + list(extra))


class Compare(unittest.TestCase):
    def test_shares_segments_and_the_base_range(self):
        base = [oc.run_metrics(tl({"1": bob(6.0)})), oc.run_metrics(tl({"1": bob(6.5)}))]
        m = base[0][("day", "BobREF")]
        self.assertAlmostEqual(m["package:BobSleep"], 0.25)
        self.assertAlmostEqual(m["procedure:SANDBOX"], 0.75)
        self.assertEqual((m["segments"], m["path"], m["_cells"]), (2.0, 20.0, ["TownCell"]))
        same = oc.compare(base, [oc.run_metrics(tl({"1": bob(6.2)}))])
        self.assertEqual(same["outside"], [], "inside the base runs' range")
        self.assertAlmostEqual(same["max_base_share_spread"], 0.5 / 24)
        moved = oc.compare(base, [oc.run_metrics(tl({"1": bob(10.0)}))])
        self.assertEqual({r["metric"] for r in moved["outside"]},
                         {"package:BobSleep", "package:BobSandbox", "procedure:SLEEP", "procedure:SANDBOX"})
        r = [x for x in moved["outside"] if x["metric"] == "package:BobSleep"][0]
        self.assertAlmostEqual(r["beyond"], 10 / 24 - 6.5 / 24)
        # a slightly later bedtime than both base runs, within the share tolerance
        self.assertEqual(oc.compare(base, [oc.run_metrics(tl({"1": bob(7.0)}))])["outside"], [])

    def test_new_cells_new_packages_and_partial_actors(self):
        base = [oc.run_metrics(tl({"1": bob(6.0)}))] * 2
        other = [oc.run_metrics(tl({"1": ("BobREF", [seg("day", 0, 12, "BobWander", "WANDER", cell="Saloon"),
                                                      seg("day", 12, 24, "BobSandbox")]),
                                    "2": ("AnnREF", [seg("day", 0, 24, "AnnSandbox")])}))]
        doc = oc.compare(base, other)
        self.assertIn("package:BobWander", {r["metric"] for r in doc["outside"]}, "a package the base runs never ran")
        self.assertEqual(doc["cells"], [{"session": "day", "actor": "BobREF", "cells_only_with": ["Saloon"]}])
        self.assertEqual(doc["partial"], [["day", "AnnREF"]])
        self.assertIn("seen in some runs only (1): day/AnnREF", oc.text_report(doc))

    def test_cli(self):
        with tempfile.TemporaryDirectory() as d:
            paths = []
            for k, until in enumerate((6.0, 6.5, 12.0)):
                p = os.path.join(d, f"{k}.json")
                with open(p, "w", encoding="utf-8") as f:
                    json.dump(tl({"1": bob(until)}), f)
                paths.append(p)
            out = io.StringIO()
            with redirect_stdout(out):
                self.assertEqual(oc.main(["--base", paths[0], paths[1], "--with", paths[2],
                                          "-o", os.path.join(d, "c.json"), "--text"]), 0)
            self.assertIn("outside the base range by more than the tolerance: 4", out.getvalue())
            with open(os.path.join(d, "c.json"), encoding="utf-8") as f:
                self.assertEqual(json.load(f)["format"], oc.COMPARE_FORMAT)
            bad = os.path.join(d, "bad.json")
            with open(bad, "w") as f:
                json.dump({"format": "x"}, f)
            with redirect_stderr(io.StringIO()):
                self.assertEqual(oc.main(["--base", bad, "--with", paths[0]]), 1)


if __name__ == "__main__":
    unittest.main()
