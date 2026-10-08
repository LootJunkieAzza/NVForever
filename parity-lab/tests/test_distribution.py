"""Self-tests for the distribution comparator (random outcomes, spec section 8.1).
Synthetic data only.

Run:  python -m unittest discover -s tests -v     (from parity-lab/)
"""
import math
import random
import sys
import unittest
from pathlib import Path

LAB = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(LAB))

from fnvlab import comparators as C  # noqa: E402


def doc(*cases):
    return {"cases": list(cases)}


def case(label, **kw):
    return {"label": label, **kw}


CFG = {"kind": "distribution", "select": "/cases"}


class ChiSquareTests(unittest.TestCase):
    def test_known_quantiles(self):
        # Textbook 5% critical values of the chi-square distribution.
        for stat, df in ((3.841459, 1), (5.991465, 2), (9.487729, 4), (18.307038, 10)):
            self.assertAlmostEqual(C.chi2_sf(stat, df), 0.05, places=5)
        self.assertAlmostEqual(C.chi2_sf(6.634897, 1), 0.01, places=5)
        self.assertAlmostEqual(C.chi2_sf(2.0, 2), math.exp(-1.0), places=10)  # df 2: exp(-x/2)

    def test_edges(self):
        self.assertEqual(C.chi2_sf(0.0, 3), 1.0)
        self.assertEqual(C.chi2_sf(5.0, 0), 1.0)
        self.assertLess(C.chi2_sf(500.0, 3), 1e-100)
        prev = 1.0
        for x in range(0, 60):
            p = C.chi2_sf(float(x), 7)
            self.assertLessEqual(p, prev + 1e-15)
            prev = p


class DistributionTests(unittest.TestCase):
    def test_registered(self):
        self.assertIs(C.COMPARATORS["distribution"], C.compare_distribution)

    def test_identical_samples_pass(self):
        h = {"a": 50, "b": 30, "nothing": 20}
        r = C.compare_distribution(doc(case("x", outcomes=h)), doc(case("x", outcomes=dict(h))), CFG)
        self.assertEqual(r.score, 1.0)
        self.assertEqual(r.total_failures, 0)
        self.assertAlmostEqual(r.stats["cases"]["x"]["p"], 1.0)
        self.assertEqual(r.stats["cases"]["x"]["mode"], "homogeneity")

    def test_same_law_different_sizes_pass(self):
        rng = random.Random(4)
        law = [("a", 0.5), ("b", 0.3), ("c", 0.2)]

        def sample(n):
            out = {}
            for _ in range(n):
                u, acc = rng.random(), 0.0
                for k, p in law:
                    acc += p
                    if u < acc:
                        out[k] = out.get(k, 0) + 1
                        break
            return out

        r = C.compare_distribution(doc(case("x", outcomes=sample(200))), doc(case("x", outcomes=sample(20000))), CFG)
        self.assertEqual(r.score, 1.0, r.stats)

    def test_different_law_fails(self):
        r = C.compare_distribution(doc(case("x", outcomes={"a": 90, "b": 10})),
                                   doc(case("x", outcomes={"a": 5000, "b": 5000})), CFG)
        self.assertEqual(r.score, 0.0)
        self.assertEqual([f.kind for f in r.failures], ["distribution"])
        self.assertLess(r.stats["cases"]["x"]["p"], 1e-6)
        self.assertAlmostEqual(r.stats["cases"]["x"]["tv"], 0.4)

    def test_goodness_of_fit(self):
        exact = {"a": 0.25, "b": 0.75}
        ok = C.compare_distribution(doc(case("x", outcomes={"a": 26, "b": 74})),
                                    doc(case("x", probabilities=exact)), CFG)
        self.assertEqual(ok.score, 1.0)
        self.assertEqual(ok.stats["cases"]["x"]["mode"], "fit")
        bad = C.compare_distribution(doc(case("x", outcomes={"a": 60, "b": 40})),
                                     doc(case("x", probabilities=exact)), CFG)
        self.assertEqual(bad.score, 0.0)

    def test_impossible_outcome_fails_outright(self):
        r = C.compare_distribution(doc(case("x", outcomes={"a": 99, "z": 1})),
                                   doc(case("x", probabilities={"a": 1.0})), CFG)
        self.assertEqual(r.score, 0.0)
        self.assertEqual(r.failures[0].kind, "impossible_outcome")
        self.assertEqual(r.failures[0].expected, ["z"])

    def test_rare_categories_are_pooled(self):
        # Many rare outcomes: without pooling the expected counts would be < 1.
        oracle = {f"r{i}": 1 for i in range(10)}
        oracle["common"] = 90
        cand = {f"r{i}": 100 for i in range(10)}
        cand["common"] = 9000
        r = C.compare_distribution(doc(case("x", outcomes=oracle)), doc(case("x", outcomes=cand)), CFG)
        self.assertEqual(r.score, 1.0)
        self.assertEqual(r.stats["cases"]["x"]["df"], 1)

    def test_single_outcome_both_sides(self):
        r = C.compare_distribution(doc(case("x", outcomes={"a": 10})), doc(case("x", outcomes={"a": 1000})), CFG)
        self.assertEqual(r.score, 1.0)
        self.assertEqual(r.stats["cases"]["x"]["df"], 0)

    def test_alpha_and_custom_key_field(self):
        cfg = {"select": "/rows", "key": "id", "field": "hist", "alpha": 0.5}
        e = {"rows": [{"id": 1, "hist": {"a": 55, "b": 45}}]}
        a = {"rows": [{"id": 1, "hist": {"a": 5000, "b": 5000}}]}
        self.assertEqual(C.compare_distribution(e, a, cfg).score, 0.0)  # p ~ 0.32 < 0.5
        self.assertEqual(C.compare_distribution(e, a, {**cfg, "alpha": 0.01}).score, 1.0)

    def test_missing_extra_and_malformed(self):
        e = doc(case("x", outcomes={"a": 10}), case("y", outcomes={"a": 10}), case("v", outcomes={"a": 10}))
        a = doc(case("x", outcomes={"a": 10}), case("y", outcomes="bad"), case("z", outcomes={"a": 10}))
        r = C.compare_distribution(e, a, CFG)
        kinds = sorted(f.kind for f in r.failures)
        self.assertEqual(kinds, ["extra", "malformed", "missing"])
        self.assertEqual(r.stats["passed"], 1)
        self.assertAlmostEqual(r.score, 1 / 4)  # x passes; y malformed; v missing; z extra

    def test_candidate_without_collection(self):
        r = C.compare_distribution(doc(case("x", outcomes={"a": 1})), {"other": 1}, CFG)
        self.assertEqual(r.score, 0.0)
        self.assertEqual(r.failures[0].kind, "missing")

    def test_bad_oracle_is_a_configuration_error(self):
        with self.assertRaises(C.ComparatorError):
            C.compare_distribution(doc(case("x", outcomes={})), doc(case("x", outcomes={"a": 1})), CFG)
        with self.assertRaises(C.ComparatorError):
            C.compare_distribution(doc(case("x", outcomes={"a": -1})), doc(case("x", outcomes={"a": 1})), CFG)
        with self.assertRaises(C.ComparatorError):
            C.compare_distribution(doc(case("x", outcomes={"a": 1}), case("x", outcomes={"a": 2})),
                                   doc(case("x", outcomes={"a": 1})), CFG)

    def test_empty_candidate_sample_fails(self):
        r = C.compare_distribution(doc(case("x", outcomes={"a": 5})), doc(case("x", outcomes={})), CFG)
        self.assertEqual(r.score, 0.0)


if __name__ == "__main__":
    unittest.main()
