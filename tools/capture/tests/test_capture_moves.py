"""Tests for tools/capture/capture_moves.py (movement jobs from the candidate's inputs,
goldens from the plugin's output). Synthetic data only."""
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_moves as cm  # noqa: E402
import capture_steps as cs  # noqa: E402

LAB_INPUTS = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")

DROPS = {"schema": "fnv.phys.drops/1", "name": "d", "drops": [
    {"label": "a", "at": [10.0, 20.0, 300.0]}, {"label": "b", "at": [1.0, 2.0, 400.0]}]}
WALK = {"schema": "fnv.phys.walk-script/1", "start": [0.0, 0.0], "hz": 60,
        "segments": [{"steps": 30}, {"steps": 60, "velocity": [0.0, 300.0]}],
        "observe": {"from_step": 30, "every": 15, "count": 4},
        "capture": {"name": "w", "set_z": 104, "heading_deg": 0, "keys": ["forward"], "samples": 20}}


def res(label, field, value):
    return {"kind": "result", "step": 0, "label": label, "field": field, "value": value}


class Jobs(unittest.TestCase):
    def test_drop_steps(self):
        job = cm.make_job([DROPS], "j", "start here", console_first=["tgm"])
        body = job["steps"][len(cs.MASTERS):]
        self.assertEqual(body[0], {"console": "tgm"})
        self.assertIn("player.SetPos Z 300", body[1]["script"])
        self.assertEqual(body[2]["max_frames"], cm.UNTIL_FALL)
        self.assertIn("player.GetPos Z < 299", body[2]["until"])
        labels = [(s["label"], s["field"]) for s in body if "query" in s]
        self.assertEqual(labels[:3], [("d a", "fell"), ("d a", "feet"), ("d a", "feet_later")])

    def test_walk_steps_hold_and_release_keys(self):
        doc = json.loads(json.dumps(WALK))
        doc["capture"].update(keys=["forward", "run"], jump=True)
        body = cm.make_job([doc], "j", "s")["steps"][len(cs.MASTERS):]
        texts = [s.get("script", "") for s in body]
        down = next(t for t in texts if "HoldKey" in t)
        self.assertLess(down.index("GetControl 9"), down.index("GetControl 0"), "run goes down before forward")
        self.assertIn("TapControl 12", down)
        self.assertTrue(any("ReleaseKey" in t for t in texts))
        self.assertIn("player.SetAngle Z 0", texts[1])
        self.assertEqual(sum(1 for s in body if s.get("field") == "sample"), 25)
        self.assertTrue(any("remember" in s for s in body))

    def test_heading_must_match_the_segments(self):
        doc = json.loads(json.dumps(WALK))
        doc["capture"]["heading_deg"] = 90
        with self.assertRaises(cm.InputError):
            cm.make_job([doc], "j", "s")

    def test_lab_inputs_make_jobs(self):
        docs = [cs.load_json(os.path.join(LAB_INPUTS, f)) for f in sorted(os.listdir(LAB_INPUTS))
                if f.endswith((".drops.json", ".walk.json"))]
        self.assertGreaterEqual(len(docs), 5)
        job = cm.make_job(docs, "j", "s", leave_door="0x00103E61")
        self.assertEqual(job["format"], cs.STEPS_FORMAT)


class Goldens(unittest.TestCase):
    def test_drops(self):
        results = [res("d a", "fell", 1.0), res("d a", "feet", "10 20 104"), res("d a", "feet_later", "10 20 103.5"),
                   res("d b", "fell", 0.0), res("d b", "feet", "1 2 400"), res("d b", "feet_later", "1 2 400")]
        g = cm.drops_golden(DROPS, results)
        self.assertEqual(g["observations"][0], {"label": "a", "feet": [10.0, 20.0, 103.5], "feet_first": [10.0, 20.0, 104.0]})
        self.assertEqual(g["observations"][1], {"label": "b", "not_observed": "did not fall"})
        self.assertEqual(g["not_observed"][0]["label"], "b")

    def test_walk_aligns_on_onset_with_one_loop_lag(self):
        # The clock runs 1/60 s per loop; the player stands for 6 samples, then moves
        # 5 units per loop, and each position lags the clock by one loop.
        results = []
        for i in range(25):
            t = i * 1000.0 / 60.0
            y = 0.0 if i < 6 else 5.0 * (i - 5)
            results.append(res(f"w s{i:03d}", "sample", f"0 {y} 104 {t} 0.016"))
        g = cm.walk_golden(WALK, results)
        # Pairs (t[i-1], y[i]): the last pair before y changes has t = 4/60 s.
        self.assertEqual(g["onset_sample"], 5)
        self.assertEqual([o["label"] for o in g["observations"]], ["t0.25"])
        self.assertAlmostEqual(g["observations"][0]["feet"][1], 75.0)

    def test_walk_without_motion_is_an_error(self):
        results = [res(f"w s{i:03d}", "sample", f"0 0 104 {i * 16.0} 0.016") for i in range(10)]
        with self.assertRaises(cm.InputError):
            cm.walk_golden(WALK, results)


if __name__ == "__main__":
    unittest.main()
