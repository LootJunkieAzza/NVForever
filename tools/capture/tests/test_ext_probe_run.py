"""Tests for tools/capture/ext_probe_run.py: job plans and crash handling, no game."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import ext_probe_run as er  # noqa: E402


class Plan(unittest.TestCase):
    def test_jobs_cover_quests_and_selector_numbers(self):
        with tempfile.TemporaryDirectory() as d:
            jobs = er.jobs_for(["strings", "arrays"], d)
        names = [n for n, _, _ in jobs]
        self.assertEqual(names, sorted(names), "job names sort in run order")
        steps = dict((n, s) for n, _, s in jobs)
        first = steps[names[0]]
        self.assertTrue(first[0]["console"].startswith('scof "'))
        self.assertEqual(first[1], {"console": "StartQuest ExtProbeStrings01Q"})
        sel = [n for n in names if "ExtProbeArrays07" in n]
        self.assertGreater(len(sel), 1, "the selector gives one job per probe number")
        self.assertIn({"console": "set ExtProbeArrays07Q.iWhich to 2"}, steps[sel[1]])

    def test_risky_scripts_only_on_request(self):
        with tempfile.TemporaryDirectory() as d:
            plain = [n for n, _, _ in er.jobs_for(["loops", "udf"], d)]
            risky = [n for n, _, _ in er.jobs_for(["loops", "udf"], d, risky=True)]
        self.assertFalse(any("Loops09" in n or "Udf05" in n for n in plain))
        self.assertTrue(any("Loops09" in n for n in risky) and any("Udf05" in n for n in risky))


class Crashes(unittest.TestCase):
    def test_a_crash_drops_the_running_job_and_relaunches(self):
        jobs = [(f"ext_00{i}_X{i}", "strings", [{"frames": 1}]) for i in range(1, 5)]
        crash_on = {"ext_002_X2"}
        calls = []

        def launch(planned, names, capture_dir, game, save, timeout, kill_on_timeout=False):
            calls.append(list(names))
            state, problems = {}, []
            for n in names:
                if n in crash_on:
                    problems.append("the game exited before every job ended (crash? see the log)")
                    break
                state[n] = {"status": "ended"}
            for n in names:
                state.setdefault(n, {"status": "pending"})
            return {"jobs": state, "problems": problems}

        with tempfile.TemporaryDirectory() as d:
            status, launches = er.run_all(jobs, d, d, None, "save", 60, launch=launch)
        self.assertEqual(status, {"ext_001_X1": "ended", "ext_002_X2": "crashed", "ext_003_X3": "ended", "ext_004_X4": "ended"})
        self.assertEqual(launches, 2)
        self.assertEqual(calls[1], ["ext_003_X3", "ext_004_X4"], "the crashed job is not run again")

    def test_collect_writes_a_golden_per_set(self):
        with tempfile.TemporaryDirectory() as d:
            log = os.path.join(d, "ext_001_A.console.txt")
            with open(log, "w", encoding="utf-8") as f:
                f.write("EXTPROBE|strings/ExtProbeStrings01|begin\nEXTPROBE|len.basic|5\n")
            res = er.collect([("ext_001_A", "strings", [])], d, d, "6.4.9")
            with open(res["strings"]["golden"], encoding="utf-8") as f:
                doc = json.load(f)
        self.assertEqual(doc["rows"], [{"id": "len.basic", "value": "5"}])
        self.assertEqual(res["strings"]["compare"]["matched"], 1)


if __name__ == "__main__":
    unittest.main()
