"""The compute-pool sweep's generated campaigns (no game, no game data)."""
import json
import sys
import tempfile
import tomllib
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

import compute_sweep  # noqa: E402

SRC = {
    "defaults": {"seconds": 60, "size": "1920x1080", "graphics": "enhanced"},
    "scenario": [
        {"name": "stand", "group": "stand", "args": ["--cell", "Somewhere", "--hour", "12"]},
        {"name": "walk", "group": "move", "args": ["--cell", "Somewhere"], "aids": {"NVF_PLAY_TEST_MOVE": "3:1:0"}, "seconds": 70},
        # A scenario without `args` (as the stress battles are).
        {"name": "battle", "group": "combat", "seconds": 90, "battle": {"per_side": 10, "third": 0}},
    ],
}


class SweepCampaign(unittest.TestCase):
    def test_every_scenario_gets_the_pool_size_and_keeps_its_own_fields(self):
        doc = tomllib.loads(compute_sweep.sweep_campaign(SRC, ["stand", "walk", "battle"], 12, False))
        self.assertEqual(doc["defaults"], SRC["defaults"])
        by = {s["name"]: s for s in doc["scenario"]}
        self.assertEqual([s["name"] for s in doc["scenario"]], ["stand", "walk", "battle"])
        for s in by.values():
            self.assertEqual(s["aids"]["NVF_PLAY_TEST_COMPUTE_THREADS"], "12")
        self.assertEqual(by["walk"]["aids"]["NVF_PLAY_TEST_MOVE"], "3:1:0")
        self.assertEqual(by["walk"]["seconds"], 70)
        self.assertEqual(by["battle"]["battle"], {"per_side": 10, "third": 0})
        self.assertEqual(by["stand"]["args"], ["--cell", "Somewhere", "--hour", "12"])
        self.assertNotIn("args", by["battle"])

    def test_placement_off_reaches_every_scenario_even_without_args(self):
        doc = tomllib.loads(compute_sweep.sweep_campaign(SRC, ["stand", "battle"], 8, True))
        for s in doc["scenario"]:
            self.assertEqual(s["args"][:2], ["--cpu-cache-placement", "off"])
        self.assertEqual(doc["scenario"][0]["args"][2:], ["--cell", "Somewhere", "--hour", "12"])

    def test_unknown_scenario_is_refused(self):
        with self.assertRaises(SystemExit):
            compute_sweep.sweep_campaign(SRC, ["stand", "nowhere"], 8, False)


class SweepResults(unittest.TestCase):
    def test_round_order_alternates(self):
        self.assertEqual(compute_sweep.round_order([4, 8, 12], 1), [4, 8, 12])
        self.assertEqual(compute_sweep.round_order([4, 8, 12], 2), [12, 8, 4])

    def test_summary_uses_only_available_values_in_successful_rounds(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            first, second, missing = [root / str(i) for i in range(3)]
            for folder, frames in [(first, {"frame_mean": 4.0, "cpu_ms": None, "hitches": []}),
                                   (second, {"frame_mean": None, "cpu_ms": 6.0, "hitches": [1, 2]})]:
                folder.mkdir()
                (folder / "results.json").write_text(json.dumps({"scenarios": [
                    {"name": "stand", "status": "ok", "slowest_frame": None, **frames},
                    {"name": "stand", "status": "failed", "frame_mean": 100.0},
                ]}), encoding="utf-8")
            lines = compute_sweep.summarise({4: [first, second, missing]}, ["stand", "walk"]).splitlines()
            self.assertEqual(lines[1].split(), ["4", "stand", "4.00", "-", "1.0", "6.0"])
            self.assertIn("(no finished run)", lines[2])


if __name__ == "__main__":
    unittest.main()
