"""Tests for tools/capture/jobs/make_b11_followup_jobs.py (the B11 rock and maize follow-up
captures, docs/spec-notes/physics.md section 4.2). No game or game data is used: the drop
files hold only positions and labels."""
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
JOBS = os.path.join(os.path.dirname(HERE), "jobs")
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, JOBS)

import make_b11_followup_jobs as fj  # noqa: E402


class FollowupJobs(unittest.TestCase):
    def test_committed_jobs_match_the_generator(self):
        for drops, name in (("diag_maize.drops.json", "mv_diag_maize"), ("diag_rocks.drops.json", "mv_diag_rocks")):
            with open(os.path.join(JOBS, name + ".steps.json"), encoding="utf-8") as f:
                self.assertEqual(json.load(f)["steps"], fj.make(drops, name, "")["steps"],
                                 "regenerate with: python tools/capture/jobs/make_b11_followup_jobs.py")

    def test_each_drop_is_traced_scanned_and_read_twice(self):
        doc = {"name": "d", "drops": [{"label": "a", "at": [1.0, 2.0, 300.0], "console_before": ["x.disable"],
                                       "console_after": ["x.enable"]}]}
        steps = fj.drop_steps(doc)
        self.assertEqual(steps[0], {"console": "x.disable"})
        self.assertEqual(steps[-1], {"console": "x.enable"})
        fields = [s["field"] for s in steps if "query" in s]
        self.assertEqual(fields[0], "fell")
        self.assertEqual(fields[1:1 + fj.TRACE], [f"s{i:03d}" for i in range(fj.TRACE)])
        self.assertEqual(fields[1 + fj.TRACE:], ["actors", "feet", "feet_later"])
        # The trace's clock starts when the fall has begun.
        i = next(k for k, s in enumerate(steps) if s.get("field") == "fell")
        self.assertEqual(steps[i + 1].get("as"), "t0")

    def test_actor_scan_declares_refs_before_the_block(self):
        # The runtime compiler refuses ref variables declared inside the block
        # (docs/spec-notes/xnvse-capture.md section 5).
        head, _ = fj.ACTOR_SCAN.split("Begin Function", 1)
        self.assertIn("ref rScan", head)
        self.assertIn("ref rNear", head)

    def test_drop_labels_are_unique(self):
        for f in ("diag_maize.drops.json", "diag_rocks.drops.json"):
            with open(os.path.join(JOBS, f), encoding="utf-8") as h:
                labels = [d["label"] for d in json.load(h)["drops"]]
            self.assertEqual(len(labels), len(set(labels)), f)


if __name__ == "__main__":
    unittest.main()
