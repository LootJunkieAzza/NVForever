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

    def test_repeated_drops_run_the_whole_sequence_again(self):
        job = cm.make_job([DROPS], "j", "start here", repeats=3)
        labels = [s["label"] for s in job["steps"] if s.get("field") == "fell"]
        self.assertEqual(labels, ["d a r1", "d b r1", "d a r2", "d b r2", "d a r3", "d b r3"])
        self.assertIn("each drop 3 times", job["title"])
        with self.assertRaises(cm.InputError):
            cm.make_job([DROPS], "j", "start here", repeats=0)

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

    def test_walk_reset_and_pitch(self):
        doc = json.loads(json.dumps(WALK))
        doc["segments"][1]["swim_velocity"] = [0.0, 200.0, -50.0]
        doc["capture"].update(reset_at=[5.0, 6.0, 7.0], pitch_deg=40)
        body = cm.make_job([doc], "j", "s")["steps"][len(cs.MASTERS):]
        self.assertIn("player.SetPos X 5", body[0]["script"])
        self.assertEqual(body[1], {"frames": cm.RESET_FRAMES})
        self.assertEqual(body[2], {"console": "CloseAllMenus"})
        texts = [s.get("script", "") for s in body]
        self.assertTrue(any("player.SetAngle X 40" in t for t in texts))
        self.assertIn("player.SetAngle X 0", texts[-1])
        doc["segments"][1]["swim_velocity"] = [200.0, 0.0, 0.0]
        with self.assertRaises(cm.InputError):
            cm.make_job([doc], "j", "s")

    def test_back_key_moves_against_the_view(self):
        doc = json.loads(json.dumps(WALK))
        doc["segments"][1]["velocity"] = [0.0, -300.0]
        doc["capture"]["keys"] = ["back"]
        body = cm.make_job([doc], "j", "s")["steps"][len(cs.MASTERS):]
        self.assertTrue(any("GetControl 1" in s.get("script", "") for s in body))
        doc["capture"]["keys"] = ["forward"]
        with self.assertRaises(cm.InputError):
            cm.make_job([doc], "j", "s")

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

    def test_drops_median_of_repeats(self):
        def drop(label, fell, z, x=10):
            return [res(label, "fell", fell), res(label, "feet", f"{x} 20 {z + 1}"), res(label, "feet_later", f"{x} 20 {z}")]
        run1 = drop("d a r1", 1.0, 100) + drop("d a r2", 1.0, 108) + drop("d a r3", 1.0, 101, x=24) + \
            drop("d b r1", 0.0, 400) + drop("d b r2", 0.0, 400) + drop("d b r3", 0.0, 400)
        run2 = drop("d a r1", 1.0, 150) + drop("d a r2", 0.0, 300) + drop("d b r1", 0.0, 400) + drop("d b r2", 0.0, 400)
        g = cm.drops_golden(DROPS, run1, run2)
        a = g["observations"][0]
        # Four repeats fell (the one that did not is left out); median per coordinate.
        self.assertEqual(a["repeats"], 4)
        self.assertEqual(a["feet"], [10.0, 20.0, 104.5])
        self.assertEqual(a["spread"], [14.0, 0.0, 50.0])
        self.assertEqual(a["readings"][0], [10.0, 20.0, 100.0])
        self.assertEqual(g["observations"][1], {"label": "b", "not_observed": "did not fall"})

    def test_golden_cli_reads_several_runs(self):
        import tempfile
        with tempfile.TemporaryDirectory() as t:
            paths = []
            for i, z in enumerate((100, 102, 110)):
                path = os.path.join(t, f"r{i}.jsonl")
                with open(path, "w", encoding="utf-8") as f:
                    head = [res(cs.MODINDEX_LABEL, m, float(k)) for k, m in enumerate(cs.MASTERS)]
                    for r in head + [res("d a", "fell", 1.0), res("d a", "feet", f"1 2 {z}"), res("d a", "feet_later", f"1 2 {z}")]:
                        f.write(json.dumps(r) + "\n")
                    f.write(json.dumps({"kind": "end"}) + "\n")
                paths.append(path)
            inp = os.path.join(t, "in.json")
            with open(inp, "w", encoding="utf-8") as f:
                json.dump({"schema": "fnv.phys.drops/1", "name": "d", "drops": [{"label": "a", "at": [1.0, 2.0, 300.0]}]}, f)
            out = os.path.join(t, "g.json")
            self.assertEqual(cm.main(["golden", paths[0], inp, "--also", paths[1], "--also", paths[2], "-o", out]), 0)
            with open(out, encoding="utf-8") as f:
                g = json.load(f)
            self.assertEqual(g["observations"][0]["feet"], [1.0, 2.0, 102.0])
            self.assertEqual(g["observations"][0]["repeats"], 3)

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

    def test_walk_onset_ignores_settling_in_z(self):
        results = []
        for i in range(25):
            t = i * 1000.0 / 60.0
            y = 0.0 if i < 6 else 5.0 * (i - 5)
            z = 104.0 if i < 3 else 103.9
            results.append(res(f"w s{i:03d}", "sample", f"0 {y} {z} {t} 0.016"))
        self.assertEqual(cm.walk_golden(WALK, results)["onset_sample"], 5)

    def test_walk_without_motion_is_an_error(self):
        results = [res(f"w s{i:03d}", "sample", f"0 0 104 {i * 16.0} 0.016") for i in range(10)]
        with self.assertRaises(cm.InputError):
            cm.walk_golden(WALK, results)


if __name__ == "__main__":
    unittest.main()
