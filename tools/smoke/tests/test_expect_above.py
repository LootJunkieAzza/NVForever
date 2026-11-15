"""[design, #2573] Paired comparisons with synthetic logs, without game data."""
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from smoke.expect import Expect, ScenarioError, check_one  # noqa: E402
from smoke.scenarios import load  # noqa: E402


class AboveTests(unittest.TestCase):
    def compare(self, lines, **extra):
        return check_one(Expect.parse(dict(re=r"fighter (\S+)", above=r"watcher (\S+)", **extra)), lines)

    def test_pass_pairs_in_matching_line_order(self):
        result = self.compare(["watcher 1,200", "fighter 1,201", "unrelated", "fighter -2.5", "watcher -3"])
        self.assertTrue(result.ok, result.detail)

    def test_named_n_takes_precedence_over_first_group(self):
        e = Expect.parse({"re": r"fighter (\d+) (?P<n>\d+)", "above": r"watcher (\d+) (?P<n>\d+)"})
        self.assertTrue(check_one(e, ["fighter 0 10", "watcher 99 9"]).ok)

    def test_every_failing_pair_is_listed(self):
        result = self.compare(["fighter 8", "watcher 7", "fighter 5", "watcher 5", "fighter 2", "watcher 3"])
        self.assertFalse(result.ok)
        self.assertEqual(result.what, "above")
        self.assertNotIn("pair 1:", result.detail)
        self.assertIn("pair 2: 5 > 5 failed", result.detail)
        self.assertIn("pair 3: 2 > 3 failed", result.detail)
        self.assertEqual(result.context, ["fighter 5", "watcher 5", "fighter 2", "watcher 3"])

    def test_unequal_counts_name_both_counts(self):
        for lines, counts in [(["fighter 2", "fighter 3", "watcher 1"], (2, 1)),
                              (["fighter 2", "watcher 1", "watcher 1"], (1, 2)),
                              (["watcher 1"], (0, 1)), (["fighter 2"], (1, 0))]:
            with self.subTest(counts=counts):
                result = self.compare(lines)
                self.assertFalse(result.ok)
                self.assertIn(f"re matched {counts[0]} lines; above matched {counts[1]} lines", result.detail)

    def test_no_lines_fails_even_with_count_zero(self):
        result = self.compare([], count=0)
        self.assertFalse(result.ok)
        self.assertIn("re matched 0 lines; above matched 0 lines", result.detail)

    def test_count_does_not_skip_comparison(self):
        self.assertFalse(self.compare(["fighter 1", "watcher 2"], count=1).ok)
        self.assertFalse(self.compare(["fighter 2", "watcher 1"], count=2).ok)
        self.assertTrue(self.compare(["fighter 2", "watcher 1"], count=1).ok)

    def test_invalid_number_is_a_failing_pair(self):
        for a, b in [("bad", "1"), ("2", "bad"), ("nan", "1")]:
            with self.subTest(a=a, b=b):
                self.assertFalse(self.compare([f"fighter {a}", f"watcher {b}"]).ok)

    def test_optional_capture_without_a_number_fails(self):
        e = Expect.parse({"re": r"fighter(?: (\d+))?", "above": r"watcher (\d+)"})
        self.assertFalse(check_one(e, ["fighter", "watcher 1"]).ok)

    def test_parse_requires_re_and_both_capture_groups(self):
        for item in [{"text": "fighter", "above": r"watcher (\d+)"},
                     {"re": "fighter", "above": r"watcher (\d+)"},
                     {"re": r"fighter (\d+)", "above": "watcher"},
                     {"re": r"fighter (\d+)", "above": "("}]:
            with self.subTest(item=item), self.assertRaises(ScenarioError):
                Expect.parse(item)

    def test_scenario_matches_call_log_formats(self):
        suite = load(Path(__file__).resolve().parent.parent / "scenarios.toml")
        scenario = next(s for s in suite.scenarios if s.name == "detection-combat-target-sneak")
        lines = ["run finished", "detection summary",
                 "alert: GoodspringsSettler01Ref 00104F03 Alert → Combat: detects the player, value 12.0, 690 units away"]
        for _ in range(5):
            for actor, value in [("GoodspringsSettler01Ref", 12.0), ("GoodspringsSettler03Ref", 10.0)]:
                lines.append(f"script: test aid calls GetDetectionValue on {actor} = Num({value})")
                lines.append(f"script: test aid calls GetDistance on {actor} = 690")
        for e in scenario.expect:
            with self.subTest(label=e.label):
                result = check_one(e, lines)
                self.assertTrue(result.ok, result.detail)
        comparison = next(e for e in scenario.expect if e.above is not None)
        self.assertFalse(check_one(comparison, [ln.replace("Num(12.0)", "Num(9.0)") for ln in lines]).ok)
