"""Self-tests for the parity lab. Synthetic data only; no game data.

Run:  python -m unittest discover -s tests -v     (from parity-lab/)
"""
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
LAB = HERE.parent
sys.path.insert(0, str(LAB))

from fnvlab import comparators as C  # noqa: E402
from fnvlab.cli import main  # noqa: E402


class ComparatorTests(unittest.TestCase):
    def test_exact_identical(self):
        r = C.compare_exact({"a": [1, 2, {"b": "x"}]}, {"a": [1, 2, {"b": "x"}]}, {})
        self.assertEqual(r.score, 1.0)
        self.assertEqual(r.total_failures, 0)

    def test_exact_float_tolerance(self):
        self.assertEqual(C.compare_exact([1.0], [1.0000001], {"abs_tol": 1e-6}).total_failures, 0)
        self.assertEqual(C.compare_exact([1.0], [1.01], {"abs_tol": 1e-6}).total_failures, 1)
        self.assertEqual(C.compare_exact([100.0], [100.5], {"rel_tol": 0.01}).total_failures, 0)

    def test_exact_int_vs_float_and_bool(self):
        self.assertEqual(C.compare_exact({"v": 3}, {"v": 3.0}, {}).total_failures, 0)
        # bool is not a number here: True must not equal 1
        self.assertEqual(C.compare_exact({"v": 1}, {"v": True}, {}).total_failures, 1)

    def test_exact_missing_extra_type(self):
        r = C.compare_exact({"a": 1, "b": 2}, {"a": 1, "c": 3}, {})
        kinds = sorted(f.kind for f in r.failures)
        self.assertEqual(kinds, ["extra", "missing"])
        r = C.compare_exact({"a": [1]}, {"a": "x"}, {})
        self.assertEqual(r.failures[0].kind, "type")

    def test_exact_subset(self):
        r = C.compare_exact({"a": {"b": 1}}, {"a": {"b": 1, "c": 2}, "d": 3}, {"subset": True})
        self.assertEqual(r.total_failures, 0)
        r = C.compare_exact({"a": {"b": 1}}, {"a": {"b": 2, "c": 2}}, {"subset": True})
        self.assertEqual(r.total_failures, 1)
        r = C.compare_exact({"a": [1, 2]}, {"a": [1, 2, 3]}, {"subset": True})
        self.assertEqual(r.total_failures, 1)

    def test_exact_ignore_paths(self):
        r = C.compare_exact({"a": 1, "t": 5}, {"a": 1, "t": 9}, {"ignore_paths": ["/t"]})
        self.assertEqual(r.total_failures, 0)

    def test_keyed(self):
        e = [{"k": 1, "v": "a"}, {"k": 2, "v": "b"}, {"k": 3, "v": "c"}]
        a = [{"k": 1, "v": "a"}, {"k": 2, "v": "X"}, {"k": 4, "v": "d"}]
        r = C.compare_keyed(e, a, {"key": "k"})
        self.assertEqual(r.stats["equal"], 1)
        self.assertEqual(r.stats["changed"], 1)
        self.assertEqual(r.stats["missing"], 1)
        self.assertEqual(r.stats["extra"], 1)
        self.assertAlmostEqual(r.score, 1 / 4)

    def test_keyed_fields_subset_and_composite_key(self):
        e = [{"p": "A", "id": 1, "x": 1, "y": 2}]
        a = [{"p": "A", "id": 1, "x": 1, "y": 99}]
        self.assertEqual(C.compare_keyed(e, a, {"key": ["p", "id"], "fields": ["x"]}).score, 1.0)
        self.assertLess(C.compare_keyed(e, a, {"key": ["p", "id"]}).score, 1.0)

    def test_keyed_duplicate_in_oracle_is_config_error(self):
        with self.assertRaises(C.ComparatorError):
            C.compare_keyed([{"k": 1}, {"k": 1}], [], {"key": "k"})

    def test_keyed_candidate_duplicates_are_failures(self):
        r = C.compare_keyed([{"k": 1}], [{"k": 1}, {"k": 1}], {"key": "k"})
        self.assertTrue(any(f.kind == "duplicate_key" for f in r.failures))
        self.assertLess(r.score, 1.0)

    def test_sequence(self):
        e = [{"s": 1}, {"s": 2}, {"s": 3}, {"s": 4}]
        r = C.compare_sequence(e, [{"s": 1}, {"s": 2}, {"s": 9}, {"s": 4}], {})
        self.assertEqual(r.stats["matching_prefix"], 2)
        self.assertEqual(r.failures[0].kind, "divergence")
        self.assertEqual(r.failures[0].path, "/2")
        self.assertAlmostEqual(r.score, 0.5)
        r = C.compare_sequence(e, e + [{"s": 5}], {})
        self.assertLess(r.score, 1.0)
        self.assertTrue(any(f.kind == "length" for f in r.failures))
        r = C.compare_sequence(e, e[:2], {})
        self.assertAlmostEqual(r.score, 0.5)

    def test_failure_cap(self):
        e = list(range(1000))
        a = [x + 1 for x in e]
        r = C.compare_exact(e, a, {})
        self.assertEqual(r.total_failures, 1000)
        self.assertEqual(len(r.failures), C.MAX_RECORDED_FAILURES)


class EndToEndTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="fnvlab-test-"))
        shutil.copytree(LAB / "selftest", self.tmp / "selftest",
                        ignore=shutil.ignore_patterns("runs"))
        self.cfg = str(self.tmp / "selftest" / "lab.json")
        os.environ["FNVLAB_PYTHON"] = sys.executable

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)
        os.environ.pop("FNVLAB_PYTHON", None)

    def _run(self, *args):
        import contextlib
        import io
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            code = main(["--config", self.cfg, *args])
        return code, out.getvalue(), err.getvalue()

    def _latest(self):
        return json.loads((self.tmp / "selftest" / "runs" / "latest.json").read_text())

    def test_demo_run_statuses(self):
        code, _, _ = self._run("run")
        self.assertEqual(code, 0)  # no baseline yet, not strict
        rep = self._latest()
        st = {r["id"]: r["status"] for r in rep["results"]}
        self.assertEqual(st["L1.synthetic.inventory.ok"], "pass")
        self.assertEqual(st["L2.synthetic.mesh.tolerance"], "pass")
        self.assertEqual(st["L1.synthetic.inventory.buggy"], "fail")
        self.assertEqual(st["L2.synthetic.mesh.bad"], "fail")
        self.assertEqual(st["L5.synthetic.trace"], "fail")
        for e in ("L1.synthetic.crash", "L1.synthetic.garbage", "L1.synthetic.missing_golden",
                  "L3.synthetic.image_planned"):
            self.assertEqual(st[e], "error", e)
        self.assertEqual(st["L6.synthetic.holdout"], "fail")

    def test_holdout_redacted_everywhere(self):
        self._run("run")
        runs = self.tmp / "selftest" / "runs"
        for f in [runs / "latest.json", runs / "latest.md", *(runs / "history").glob("*.json")]:
            text = f.read_text()
            self.assertNotIn("SECRET", text, f)
            self.assertNotIn("VMS01\", \"stage\": 30", text.split("L6.synthetic.holdout")[-1][:2000]
                             if "L6.synthetic.holdout" in text else "", f)
        rep = self._latest()
        h = [r for r in rep["results"] if r["id"] == "L6.synthetic.holdout"][0]
        self.assertTrue(h["redacted"])
        self.assertNotIn("failures", h)
        self.assertNotIn("L6.synthetic.holdout", [w["id"] for w in rep["next_work"]])
        # list must not leak the description either
        code, out, _ = self._run("list", "--json")
        self.assertNotIn("SECRET", out)

    def test_reveal_refused_without_tty(self):
        code, _, err = self._run("run", "--reveal")
        self.assertEqual(code, 3)
        self.assertIn("interactive terminal", err)

    def test_next_work_ordering(self):
        self._run("run")
        nw = self._latest()["next_work"]
        keys = [(w["layer"], w["score"]) for w in nw]
        self.assertEqual(keys, sorted(keys))

    def test_failure_ids_stable(self):
        self._run("run")
        a = {f["id"] for r in self._latest()["results"] for f in r.get("failures", [])}
        self._run("run")
        b = {f["id"] for r in self._latest()["results"] for f in r.get("failures", [])}
        self.assertEqual(a, b)
        self.assertTrue(a)

    def test_regression_exit_code(self):
        self._run("run")
        self.assertEqual(self._run("baseline")[0], 0)
        self.assertEqual(self._run("run")[0], 0)  # same results, no regression
        # Break the previously passing scenario by changing the fake candidate mode.
        sp = self.tmp / "selftest" / "scenarios" / "selftest.json"
        data = json.loads(sp.read_text())
        for s in data["scenarios"]:
            if s["id"] == "L1.synthetic.inventory.ok":
                s["candidate"]["argv"][-1] = "inventory_buggy"
        sp.write_text(json.dumps(data))
        code, out, _ = self._run("run")
        self.assertEqual(code, 1)
        regs = self._latest()["regressions"]
        self.assertEqual([r["id"] for r in regs], ["L1.synthetic.inventory.ok"])

    def test_holdout_regression_is_redacted(self):
        self._run("run")
        self._run("baseline")
        sp = self.tmp / "selftest" / "scenarios" / "selftest.json"
        data = json.loads(sp.read_text())
        for s in data["scenarios"]:
            if s["id"] == "L6.synthetic.holdout":
                s["candidate"]["argv"][-1] = "crash"
        sp.write_text(json.dumps(data))
        code, _, _ = self._run("run")
        self.assertEqual(code, 1)
        reg = self._latest()["regressions"][0]
        self.assertTrue(reg["redacted"])
        self.assertNotIn("status", reg)

    def test_require_baseline(self):
        self.assertEqual(self._run("run", "--require-baseline")[0], 4)
        self._run("run")
        self._run("baseline")
        self.assertEqual(self._run("run", "--require-baseline")[0], 0)

    def test_history_files_unique(self):
        self._run("run", "--layer", "2")
        self._run("run", "--layer", "2")
        self._run("run", "--layer", "2")
        hist = list((self.tmp / "selftest" / "runs" / "history").glob("*.json"))
        self.assertEqual(len(hist), 3)

    def test_list_json_hides_holdout_tags(self):
        code, out, _ = self._run("list", "--json")
        h = [s for s in json.loads(out) if s["holdout"]][0]
        self.assertEqual(h["tags"], [])
        self.assertEqual(h["description"], "")

    def test_strict(self):
        code, _, _ = self._run("run", "--strict")
        self.assertEqual(code, 2)
        code, _, _ = self._run("run", "--strict", "--scenario", "L1.synthetic.inventory.ok")
        self.assertEqual(code, 0)

    def test_filters(self):
        code, out, _ = self._run("list", "--json", "--layer", "2")
        ids = [s["id"] for s in json.loads(out)]
        self.assertEqual(ids, ["L2.synthetic.mesh.bad", "L2.synthetic.mesh.tolerance"])
        code, out, _ = self._run("list", "--json", "--tag", "script")
        self.assertEqual([s["id"] for s in json.loads(out)], ["L5.synthetic.trace"])
        code, out, _ = self._run("list", "--json", "--scenario", "L1.synthetic.inv*")
        self.assertEqual(len(json.loads(out)), 2)

    def test_bad_scenario_file_is_config_error(self):
        (self.tmp / "selftest" / "scenarios" / "broken.json").write_text('{"scenarios": [{"id": "x"}]}')
        code, _, err = self._run("list")
        self.assertEqual(code, 3)
        self.assertIn("missing", err)

    def test_json_flag(self):
        code, out, _ = self._run("run", "--json", "--layer", "2")
        rep = json.loads(out)
        self.assertEqual(rep["summary"]["total"], 2)


if __name__ == "__main__":
    unittest.main()
