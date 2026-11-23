"""Unit tests for the `flashlight-aimed-weapon` smoke scenario (#3121 / #3405).

No game data required.
Run:  python -m unittest discover -s tools/smoke/tests     (from the repo root)
"""
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke import choose, integrity  # noqa: E402
from smoke.expect import check_forbid, check_one, check_order  # noqa: E402
from smoke.scenarios import load  # noqa: E402

SAMPLE_LOG = [
    "2026-11-22T19:00:02.373572Z  INFO nvf_play::test_aid: test aid: the player equips 0x00129A44 (unequipped [])",
    "2026-11-22T19:00:30.917010Z  INFO nvf_play::aim_zoom: aim zoom: WeapNVRangerSequoia (0x00129A44) sight FOV 45.0° (vertical 34.5°, unzoomed 59.8°)",
    "2026-11-22T19:00:32.665537Z  INFO nvf_play::test_aid: test aid: screenshot flashlight_aimed_off.png",
    "2026-11-22T19:00:33.676006Z  INFO nvf_play::flashlight: flashlight: on",
    "2026-11-22T19:00:35.669468Z  INFO nvf_play::test_aid: test aid: screenshot flashlight_aimed_on.png",
    "2026-11-22T19:00:36.678698Z  INFO nvf_play::flashlight: flashlight: off",
    "2026-11-22T19:00:37.907162Z  INFO nvf_play::aim_zoom: aim zoom: back to the unzoomed view (vertical 59.8°)",
    "2026-11-22T19:00:39.672290Z  INFO nvforever: run finished after 14.0s: 1 cells loaded in total, 1 loaded now",
]


class FlashlightAimedWeaponScenarioTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scenarios_path = HERE / "scenarios.toml"
        cls.suite = load(cls.scenarios_path)
        cls.scenario = cls.suite.by_name()["flashlight-aimed-weapon"]

    def test_scenarios_file_integrity_is_clean(self):
        self.assertEqual(integrity.problems(self.scenarios_path.read_text(encoding="utf-8"), HERE), [])

    def test_scenario_shape_and_controls(self):
        s = self.scenario
        self.assertIn("flashlight", s.areas)
        self.assertIn("lights", s.areas)
        self.assertIn("player-body", s.areas)
        self.assertIn("--play", s.args)
        self.assertIn("1280x720", s.args)
        self.assertEqual(s.env.get("NVF_PLAY_TEST_EQUIP"), "00129A44")
        control = s.env.get("NVF_PLAY_TEST_CONTROL", "")
        self.assertIn(":weapon", control)
        self.assertIn(":aim", control)
        self.assertIn(":flashlight", control)
        shots = s.env.get("NVF_PLAY_TEST_SHOTS", "")
        self.assertIn("flashlight_aimed_off.png", shots)
        self.assertIn("flashlight_aimed_on.png", shots)
        self.assertTrue(s.manual)

    def test_selected_for_flashlight_and_display_shader_paths(self):
        for changed in (
            "engine/nvforever/src/flashlight.rs",
            "engine/nvforever/src/lights.rs",
            "engine/nvforever/src/lights/display_shader.rs",
            "engine/nvf-play/src/flashlight.rs",
        ):
            with self.subTest(changed=changed):
                picked = choose.by_changes(self.suite.scenarios, [changed])
                self.assertIn("flashlight-aimed-weapon", [s.name for s, _ in picked])
        by_area = choose.by_area(self.suite.scenarios, ["flashlight"])
        self.assertIn("flashlight-aimed-weapon", [s.name for s in by_area])

    def test_expectations_order_and_forbid_match_sample_log(self):
        for e in self.scenario.expect:
            with self.subTest(expect=e.label):
                res = check_one(e, SAMPLE_LOG)
                self.assertTrue(res.ok, res.detail)
        for seq in self.scenario.order:
            outcomes = check_order(seq, SAMPLE_LOG)
            self.assertTrue(all(o.ok for o in outcomes), [o.detail for o in outcomes if not o.ok])
        for f in self.scenario.forbid:
            res = check_forbid(f, SAMPLE_LOG)
            self.assertTrue(res.ok, res.detail)

    def test_forbid_catches_rendering_error_and_order_catches_missing_flashlight_on(self):
        bad_render = SAMPLE_LOG + ["2026-11-22T19:00:34.000000Z ERROR wgpu: Caught rendering error: test"]
        self.assertFalse(all(check_forbid(f, bad_render).ok for f in self.scenario.forbid))
        no_light = [ln for ln in SAMPLE_LOG if "flashlight: on" not in ln]
        outcomes = check_order(self.scenario.order[0], no_light)
        self.assertFalse(all(o.ok for o in outcomes))


if __name__ == "__main__":
    unittest.main()
