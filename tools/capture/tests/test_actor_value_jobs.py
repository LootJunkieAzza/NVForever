"""Tests for tools/capture/jobs/make_actor_value_jobs.py (the actor value capture,
docs/spec-notes/actor-values.md section 8). No game or game data is used: the job holds
only script text and FormIDs."""
import json
import os
import re
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
JOBS = os.path.join(os.path.dirname(HERE), "jobs")
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, JOBS)

import make_actor_value_jobs as av  # noqa: E402


class ActorValueJob(unittest.TestCase):
    def setUp(self):
        self.job = av.actor_values()
        self.queries = [s for s in self.job["steps"] if "query" in s]
        self.texts = [s.get("query") or s.get("script") for s in self.job["steps"] if "query" in s or "script" in s]

    def test_committed_job_matches_the_generator(self):
        with open(os.path.join(JOBS, "av_actor_values.steps.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f)["steps"], self.job["steps"],
                             "regenerate with: python tools/capture/jobs/make_actor_value_jobs.py")

    def test_no_manual_steps_and_unique_results(self):
        # capture_run.py refuses jobs with manual steps unless someone attends.
        self.assertFalse([s for s in self.job["steps"] if "manual" in s])
        keys = [(s["label"], s["field"]) for s in self.queries if s["label"] != "__modindex__"]
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
            used = set(re.findall(r"\b([rfi][A-Z]\w*)\b", body))
            self.assertLessEqual(used, declared, t)

    def test_every_actor_has_every_code_once_for_both_readings(self):
        for label, _ in av.ACTORS:
            self.assertEqual(sum(1 for s in self.queries if s["label"] == label and s["field"] == "level"), 1)
            for kind, function in (("current", "GetActorValue"), ("base", "GetBaseActorValue")):
                codes = []
                for s in self.queries:
                    if s["label"] == f"{label} {kind}":
                        got = [int(c) for c in re.findall(rf"\.{function} (\d+)$", s["query"], re.M)]
                        first, last = (int(x) for x in s["field"].split("-"))
                        self.assertEqual(got, list(range(first, last + 1)))
                        # One number per code comes back.
                        self.assertEqual(s["query"].count("($ fV"), len(got))
                        codes += got
                self.assertEqual(codes, list(range(av.CODES)), f"{label} {kind}")

    def test_the_actors_are_built_from_their_form_ids_and_the_player_by_name(self):
        for label, who in av.ACTORS:
            s = next(s for s in self.queries if s["label"] == label and s["field"] == "level")
            if who == "player":
                self.assertIn("player.GetLevel", s["query"])
            else:
                self.assertIn(f"BuildRef fM1 {int(who, 16) & 0xFFFFFF}", s["query"])

    def test_what_is_changed_on_the_player_is_put_back_where_it_can_be(self):
        scripts = [s["script"] for s in self.job["steps"] if "script" in s]
        self.assertIn(f"player.DamageActorValue {av.CHARISMA} 2", scripts[0])
        self.assertIn(f"player.RestoreActorValue {av.CHARISMA} 2", scripts[1])
        mods = [m for t in scripts for m in re.findall(r"player\.ModActorValue (\d+) (-?\d+)", t)]
        for code in {c for c, _ in mods}:
            self.assertEqual(sum(int(d) for c, d in mods if c == code), 0)
        # SetActorValue comes last: the run ends without saving.
        self.assertIn("SetActorValue", scripts[-1])


if __name__ == "__main__":
    unittest.main()
