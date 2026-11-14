"""Latency routes and DLSS-G exit checks, using synthetic logs only."""
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke.expect import check_one  # noqa: E402
from smoke.runner import judge  # noqa: E402
from smoke.scenarios import load  # noqa: E402


class DlssRouteTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scenarios = load(HERE / "scenarios.toml").by_name()

    def test_latency_names_each_supported_route_explicitly(self):
        scenario = self.scenarios["latency-reduction"]
        for available, technique in [
            ("AMD Anti-Lag (VK_AMD_anti_lag) available", "AMD Anti-Lag"),
            ("NVIDIA low latency (VK_NV_low_latency2) available", "NVIDIA low latency"),
            ("NVIDIA Reflex through Streamline available", "NVIDIA Reflex (Streamline)"),
            ("not available", "not available on this GPU"),
        ]:
            with self.subTest(technique=technique):
                lines = ["run finished", f"latency reduction: {available} on GPU",
                         f"latency reduction: on ({technique})"]
                self.assertEqual(judge(scenario, lines, 0).status, "PASS")
                self.assertEqual(judge(scenario, lines[:-1], 0).status, "FAIL")
        unknown = ["run finished", "latency reduction: some new technique available on GPU",
                   "latency reduction: on (some new technique)"]
        self.assertEqual(judge(scenario, unknown, 0).status, "FAIL")

    def test_frame_generation_requires_latency_on_with_frame_generation(self):
        expectation = next(e for e in self.scenarios["frame-generation"].expect
                           if e.label == "frame generation turns latency reduction on")
        for technique in ("AMD Anti-Lag", "NVIDIA low latency", "NVIDIA Reflex (Streamline)",
                          "not available on this GPU"):
            self.assertTrue(check_one(expectation, [f"latency reduction: on ({technique}, with frame generation)"]).ok)
            self.assertFalse(check_one(expectation, [f"latency reduction: on ({technique})"]).ok)
        self.assertFalse(check_one(expectation, ["latency reduction: off (NVIDIA Reflex (Streamline), with frame generation)"]).ok)

    def test_direct_variant_excludes_streamline_even_with_direct_messages(self):
        scenario = self.scenarios["latency-reduction-direct"]
        self.assertEqual(scenario.env["NVF_DLSS_PRELOAD"], "0")
        self.assertIn("dlss-empty", scenario.env["NVF_DLSS_DIR"])
        self.assertEqual(scenario.args[scenario.args.index("--upscaler") + 1], "off")
        lines = ["run finished", "latency reduction: NVIDIA low latency (VK_NV_low_latency2) available on GPU",
                 "latency reduction: on (NVIDIA low latency)"]
        self.assertEqual(judge(scenario, lines, 0).status, "PASS")
        for extra in ("NVIDIA Reflex through Streamline available", "DLSS: slInit: eOk", "DLSS: route: interposer"):
            self.assertEqual(judge(scenario, lines + [extra], 0).status, "FAIL")

    def exit_lines(self, notification):
        return [
            "run finished", "DLSS-G: options: mode On, ", notification,
            "DLSS-G: shutdown: options mode eOff: eOk",
            "DLSS-G: shutdown: null tags (frame 42): eOk",
            "DLSS: shutdown: slFreeResources(DLSS, viewport 0) eOk",
            "DLSS-G: shutdown: slFreeResources(DLSS-G, viewport 0) eOk",
            "DLSS: shutdown: slShutdown eOk",
            "DLSS: shutdown done: Streamline down before the Vulkan device and instance were released",
        ]

    def test_exit_requires_mode_off_before_ordered_cleanup_on_either_path(self):
        scenario = self.scenarios["dlssg-exit"]
        for notification in ("DLSS-G: exit: DLSS-G's mode off for the last frame (AppExit)",
                             "DLSS-G: exit: the window closes: DLSS-G's mode off now: eOk"):
            lines = self.exit_lines(notification)
            self.assertEqual(judge(scenario, lines, 0).status, "PASS")
            self.assertEqual(judge(scenario, lines[:2] + lines[3:], 0).status, "FAIL")
            self.assertEqual(judge(scenario, lines[:2] + list(reversed(lines[2:])), 0).status, "FAIL")
            self.assertEqual(judge(scenario, lines[:5] + lines[6:], 0).status, "FAIL")
        failed_off = self.exit_lines("DLSS-G: exit: the window closes: DLSS-G's mode off now: eError")
        self.assertEqual(judge(scenario, failed_off, 0).status, "FAIL")
