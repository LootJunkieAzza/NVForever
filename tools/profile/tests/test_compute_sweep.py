"""The compute-pool sweep's generated campaigns (no game, no game data)."""
import sys
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


if __name__ == "__main__":
    unittest.main()
