"""Tests for tools/capture/jobs/make_controls_jobs.py (the player-controls capture,
docs/spec-notes/controls.md section 5.4). No game or game data is used: the job holds
only script text, a FormID and positions."""
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
JOBS = os.path.join(os.path.dirname(HERE), "jobs")
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, JOBS)

import make_controls_jobs as cj  # noqa: E402


def text_of(step):
    return next((step[k] for k in ("script", "query", "remember", "until", "console") if k in step), "")


class ControlsJob(unittest.TestCase):
    def setUp(self):
        self.job = cj.controls_job()
        self.steps = self.job["steps"]

    def test_committed_job_matches_the_generator(self):
        with open(os.path.join(JOBS, "pc_controls.steps.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f), self.job, "regenerate with: python tools/capture/jobs/make_controls_jobs.py")

    def test_flags_follow_the_parameter_order(self):
        self.assertEqual(cj.flags(), "0 0 0 0 0 0 0")
        self.assertEqual(cj.flags("movement"), "1 0 0 0 0 0 0")
        self.assertEqual(cj.flags("sneaking", "pov"), "0 0 0 1 0 0 1")
        self.assertEqual(cj.flags(*cj.GROUPS), "1 1 1 1 1 1 1")
        with self.assertRaises(ValueError):
            cj.flags("jumping")

    def test_probes_ask_each_group_and_any(self):
        steps = cj.probes("p")
        self.assertEqual([s["field"] for s in steps], [f"disabled.{g}" for g in cj.GROUPS] + ["disabled.any"])
        self.assertIn("GetPlayerControlsDisabled 0 0 0 0 1 0 0\n", steps[4]["query"])
        self.assertIn("let fV := GetPlayerControlsDisabled\n", steps[7]["query"])

    def test_every_disable_is_given_back_before_the_job_ends(self):
        calls = [t for t in map(text_of, self.steps) if "DisablePlayerControls" in t or "EnablePlayerControls" in t]
        calls = [t for t in calls if "GetPlayerControlsDisabled" not in t]
        self.assertIn("DisablePlayerControls\n", calls[0], "the form without parameters comes first")
        self.assertIn("EnablePlayerControls\n", calls[-1], "the job leaves every control on")
        # No two disables in a row: each one is given back before the next.
        for a, b in zip(calls, calls[1:]):
            if "DisablePlayerControls" in a and "DisablePlayerControls" in b:
                self.fail(f"two disables in a row: {a!r} {b!r}")

    def test_short_parameter_lists_are_probed_from_a_known_state(self):
        labels = {s["label"] for s in self.steps if s.get("field") == "disabled.fighting"}
        for form in cj.SHORT_DISABLES:
            self.assertIn(f"disable {form}", labels)
        for form in cj.SHORT_ENABLES:
            self.assertIn(f"all off then enable {form}", labels)
        # The opening's own call (VCG00 stage 0).
        self.assertIn("1 1", cj.SHORT_DISABLES)
        texts = list(map(text_of, self.steps))
        i = next(k for k, t in enumerate(texts) if "DisablePlayerControls 1 1\n" in t)
        before = [t for t in texts[:i] if "PlayerControls" in t and "GetPlayerControlsDisabled" not in t][-1]
        self.assertIn("EnablePlayerControls\n", before)

    def test_held_keys_are_released(self):
        held = 0
        for t in map(text_of, self.steps):
            held += t.count("HoldKey iK") - t.count("ReleaseKey iK")
            self.assertIn(held, (0, 1))
        self.assertEqual(held, 0)

    def test_the_door_comes_last_and_uses_the_remembered_pose(self):
        names = [s["as"] for s in self.steps if "remember" in s]
        self.assertEqual(names[:4], ["door_x", "door_y", "door_z", "door_h"])
        first_door = next(i for i, s in enumerate(self.steps) if "%{door_x}" in text_of(s))
        last_home = max(i for i, s in enumerate(self.steps) if f"player.SetPos X {cj.cs.number(cj.mv.RUN_START[0])}" in text_of(s))
        self.assertGreater(first_door, last_home, "a working Activate takes the player indoors")
        labels = [s["label"] for s in self.steps if s.get("field") == "crosshair"]
        self.assertEqual(labels[:2], ["door controls on, as arrived", "door controls on, turned round"])
        self.assertEqual(len(labels), 8)
        shots = [s["screenshot"] for s in self.steps if "screenshot" in s]
        self.assertEqual(len(shots), len(set(shots)))
        self.assertIn("door_movement_off_turned_round", shots)
        # The control case: Activate tapped with every control on, after the locked one.
        acts = [s["label"] for s in self.steps if s.get("field") == "x" and s["label"].startswith("door activate")]
        self.assertEqual(acts, ["door activate with movement off, as arrived", "door activate with movement off, turned round",
                                "door activate with controls on, as arrived", "door activate with controls on, turned round"])

    def test_labels_are_unique_per_field(self):
        seen = set()
        for s in self.steps:
            if "query" in s and s["label"] != cj.cs.MODINDEX_LABEL:
                key = (s["label"], s["field"])
                self.assertNotIn(key, seen)
                seen.add(key)


if __name__ == "__main__":
    unittest.main()
