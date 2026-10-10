"""Tests for tools/capture/jobs/make_actor_push_jobs.py (the actor collision capture,
docs/spec-notes/physics.md section 6.8). No game or game data is used: the job holds only
script text, FormIDs and positions."""
import json
import os
import re
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
JOBS = os.path.join(os.path.dirname(HERE), "jobs")
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, JOBS)

import make_actor_push_jobs as ap  # noqa: E402


class ActorPushJob(unittest.TestCase):
    def setUp(self):
        self.job = ap.actor_push()
        self.texts = [s.get("query") or s.get("script") or s.get("remember") for s in self.job["steps"]]
        self.texts = [t for t in self.texts if t]

    def test_committed_job_matches_the_generator(self):
        with open(os.path.join(JOBS, "pc_actor_push.steps.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f)["steps"], self.job["steps"],
                             "regenerate with: python tools/capture/jobs/make_actor_push_jobs.py")

    def test_no_manual_steps_and_unique_results(self):
        # capture_run.py refuses jobs with manual steps unless someone attends.
        self.assertFalse([s for s in self.job["steps"] if "manual" in s])
        keys = [(s["label"], s["field"]) for s in self.job["steps"] if "query" in s]
        self.assertEqual(len(keys), len(set(keys)))

    def test_scripts_follow_the_runtime_compiler_rules(self):
        # docs/spec-notes/xnvse-capture.md section 5: variables are declared before the
        # block, forms are built with BuildRef (no Editor IDs, no hex literals), and the
        # names fW, rK and rS did not compile.
        for t in self.texts:
            head, body = t.split("Begin Function { }", 1)
            self.assertNotRegex(body, r"(?m)^\s*(ref|float|int)\s")
            self.assertNotRegex(t, r"0x[0-9A-Fa-f]+")
            declared = set(re.findall(r"(?m)^(?:ref|float|int) (\w+)$", head))
            self.assertFalse(declared & {"fW", "rK", "rS"})
            used = set(re.findall(r"\b([rfi][A-Z]\w*)\b", body)) - {"fMoveBaseSpeed", "fMoveRunMult"}
            self.assertLessEqual(used, declared, t)

    def test_every_trial_has_its_own_clock_and_both_actors_are_run_into(self):
        steps = self.job["steps"]
        for who in ("doc", "settler"):
            for trial in ("A run 1", "A walk 1", "B run side", "C run long", "D run restrained", "E overlap 1", "F free", "F blocked"):
                first = next(i for i, s in enumerate(steps) if s.get("label", "").startswith(f"{who} {trial} s000"))
                self.assertEqual(steps[first - 1].get("as"), "t0", f"{who} {trial}")
        # The player is put on the remembered spot of the actor's own walk.
        self.assertEqual(sum(1 for t in self.texts if "%{bx}" in t and "%{by}" in t), 2)
        # The settler is removed at the end; Doc Mitchell is restrained and released.
        self.assertIn("rH.MarkForDelete", self.texts[-1])
        self.assertEqual(sum(1 for t in self.texts if "SetRestrained 1" in t), sum(1 for t in self.texts if "SetRestrained 0" in t))

    def test_samples_thin_out_without_losing_the_loop_count(self):
        s = ap.samples(ap.doc, "x", 3, 12, 3)
        self.assertEqual([q["label"] for q in s if "query" in q], ["x s003", "x s006", "x s009", "x s012"])
        self.assertEqual(sum(q.get("frames", 1) for q in s), 12)


if __name__ == "__main__":
    unittest.main()
