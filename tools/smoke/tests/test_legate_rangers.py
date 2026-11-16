"""[design, #2532] Require both rangers' descent using synthetic logs, no game data."""
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke.runner import judge  # noqa: E402
from smoke.scenarios import load  # noqa: E402


def ranger_lines(actor, names=True):
    marker01 = "VLegateCampRangerRunMarker01REF" if names else "0x001438D3"
    marker02 = "VLegateCampRangerRunMarker02REF" if names else "0x001438D4"
    return [
        f"{actor}: package VLegateRangerRunToCamp01",
        f"{actor}: runs to (10, 20, 30) (patrol point {marker01})",
        f"{actor}: arrived after 1.0s, 100 units (patrol point {marker01})",
        f"{actor}: runs to (40, 50, 60) (patrol point {marker02})",
        f"actor jumps: {actor} is in the air on JumpLoopForward",
        f"{actor}: arrived after 2.0s, 200 units (patrol point {marker02})",
    ]


class LegateRangerTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.scenario = load(HERE / "scenarios.toml").by_name()["legate-rangers-showdown"]

    def verdict(self, first, second):
        return judge(self.scenario, first + second + ["run finished"], 0).status

    def test_both_descend_then_arrive_with_names_or_ids(self):
        for names in (True, False):
            with self.subTest(names=names):
                first = ranger_lines("001438DA", names)
                second = ranger_lines("001438DB", names)
                # [design] Interleaved actors still have separate ordered chains.
                lines = [line for pair in zip(first, second) for line in pair]
                self.assertEqual(self.verdict(lines, []), "PASS")

    def test_marker01_only_fails_even_without_cannot_reach(self):
        self.assertEqual(self.verdict(ranger_lines("001438DA")[:3],
                                      ranger_lines("001438DB")[:3]), "FAIL")

    def test_each_ranger_requires_every_descent_step_in_order(self):
        for actor in ("001438DA", "001438DB"):
            for missing in (3, 4, 5):
                with self.subTest(actor=actor, missing=missing):
                    logs = {a: ranger_lines(a) for a in ("001438DA", "001438DB")}
                    del logs[actor][missing]
                    self.assertEqual(self.verdict(logs["001438DA"], logs["001438DB"]), "FAIL")
            for wrong in ("actor jumps: 00000001 is in the air on JumpLoopForward",
                          f"actor jumps: {actor} jumps up a ledge at (1, 2, 3)"):
                with self.subTest(actor=actor, wrong=wrong):
                    logs = {a: ranger_lines(a) for a in ("001438DA", "001438DB")}
                    logs[actor][4] = wrong
                    self.assertEqual(self.verdict(logs["001438DA"], logs["001438DB"]), "FAIL")
            with self.subTest(actor=actor, order="arrival before drop"):
                logs = {a: ranger_lines(a) for a in ("001438DA", "001438DB")}
                logs[actor][4], logs[actor][5] = logs[actor][5], logs[actor][4]
                self.assertEqual(self.verdict(logs["001438DA"], logs["001438DB"]), "FAIL")

    def test_cannot_reach_still_fails(self):
        for actor in ("001438DA", "001438DB"):
            with self.subTest(actor=actor):
                lines = ranger_lines("001438DA") + ranger_lines("001438DB")
                lines.append(f"{actor}: cannot reach goal")
                self.assertEqual(self.verdict(lines, []), "FAIL")


if __name__ == "__main__":
    unittest.main()
