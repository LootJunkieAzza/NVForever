"""The "game" candidate kind: a run of the engine under a smoke slot, its answer read
from output_file (fnvlab/runner.py). Synthetic: a small Python script stands in for
nvforever. No game data.

Run:  python -m unittest discover -s tests -v     (from parity-lab/)
"""
import contextlib
import io
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
sys.path.insert(0, str(LAB.parent / "tools" / "smoke"))

from fnvlab.config import ConfigError, LabConfig, Scenario  # noqa: E402
from fnvlab import runner  # noqa: E402

# Stands in for `nvforever --play` with NVF_PLAY_TEST_QUERIES: writes the answer where
# NVF_PLAY_TEST_QUERIES_OUT says, with what it saw of its environment and the locks.
FAKE_GAME = r'''
import json, os, sys
out = os.environ["NVF_PLAY_TEST_QUERIES_OUT"]
locks = os.environ.get("NVF_SMOKE_LOCKS", "")
shared = locks.split(os.pathsep)[-1] if locks else ""
print("fake game: running", flush=True)
if sys.argv[1:] == ["--fail"]:
    print("fake game: something broke", flush=True)
    sys.exit(3)
if sys.argv[1:] == ["--no-answer"]:
    sys.exit(0)
doc = {"format": "fnv.ext.probes/1", "set": "x",
       "rows": [{"id": "steps", "value": os.environ["NVF_PLAY_TEST_QUERIES"]},
                {"id": "stray_aid", "value": os.environ.get("NVF_PLAY_TEST_CONSOLE", "")},
                {"id": "slot_held", "value": str(bool(shared) and os.path.isdir(shared))}]}
with open(out, "w", encoding="utf-8") as f:
    json.dump(doc, f)
'''


def scenario(argv_tail=(), **cand):
    c = {"kind": "game",
         "argv": ["{python}", "{fake}", *argv_tail],
         "env": {"NVF_PLAY_TEST_QUERIES": "{lab_root}/job.steps.json",
                 "NVF_PLAY_TEST_QUERIES_OUT": "{out}"},
         "output_file": "{out}", "timeout_s": 60}
    c.update(cand)
    return Scenario.from_json({"id": "L5.ext.fake", "layer": 5, "oracle": {"inline": {}},
                               "candidate": c, "comparator": {"kind": "exact"}}, "test")


class GameCandidateTests(unittest.TestCase):
    def setUp(self):
        self.tmp = Path(tempfile.mkdtemp(prefix="fnvlab-game-"))
        fake = self.tmp / "fake_game.py"
        fake.write_text(FAKE_GAME, encoding="utf-8")
        self.out = self.tmp / "answers" / "x.probes.json"
        # The real lab folder as the root: the smoke runner's module is found beside it.
        self.cfg = LabConfig(root=LAB, vars={"python": sys.executable, "fake": str(fake),
                                             "out": str(self.out), "lab_root": str(self.tmp)},
                             goldens_dir=self.tmp, scenarios_dirs=[], runs_dir=self.tmp,
                             baseline_path=self.tmp / "b.json")
        self.saved = {k: os.environ.get(k) for k in ("NVF_SMOKE_LOCKS", "NVF_SMOKE_STAGGER", "NVF_PLAY_TEST_CONSOLE")}
        # Locks of this test only, never the machine's.
        os.environ["NVF_SMOKE_LOCKS"] = os.pathsep.join([str(self.tmp / "locks" / "run.lock"),
                                                         str(self.tmp / "locks" / "smoke.lock")])
        os.environ["NVF_SMOKE_STAGGER"] = "0"
        os.environ["NVF_PLAY_TEST_CONSOLE"] = "1:tgm"  # a stray aid of the lab's shell

    def tearDown(self):
        for k, v in self.saved.items():
            if v is None:
                os.environ.pop(k, None)
            else:
                os.environ[k] = v
        shutil.rmtree(self.tmp, ignore_errors=True)

    def run_quiet(self, s):
        with contextlib.redirect_stderr(io.StringIO()):
            return runner._run_candidate(s, self.cfg)

    def test_runs_under_a_smoke_slot_and_reads_the_answer(self):
        self.out.parent.mkdir(parents=True)
        self.out.write_text('{"stale": true}')  # a previous run's answer is never read
        got = self.run_quiet(scenario())
        rows = {r["id"]: r["value"] for r in got["rows"]}
        self.assertEqual(rows["steps"], f"{self.tmp}/job.steps.json")
        self.assertEqual(rows["stray_aid"], "", "the lab's own test aids are not passed on")
        self.assertEqual(rows["slot_held"], "True", "the shared smoke lock is held during the run")
        self.assertFalse((self.tmp / "locks" / "smoke.lock").exists(), "and gone after it")
        self.assertIn("fake game: running", (self.out.parent / "x.probes.json.log").read_text())

    def test_waits_for_the_capture_lock(self):
        # The original game's capture lock is there: no slot, the run waits (a gate that
        # gives up at once stands in for the wait).
        (self.tmp / "locks" / "run.lock").mkdir(parents=True)
        calls = []

        @contextlib.contextmanager
        def gate(cfg):
            calls.append("in")
            yield
            calls.append("out")
        got = runner._run_candidate(scenario(), self.cfg, gate=gate)
        self.assertEqual(calls, ["in", "out"])
        self.assertEqual(got["format"], "fnv.ext.probes/1")
        from smoke import slots  # noqa: PLC0415 - the gate's module
        pool = slots.Slots(self.tmp / "locks" / "smoke.lock", [self.tmp / "locks" / "run.lock"], want=1, say=lambda m: None)
        self.assertIsInstance(pool.try_take(), str, "a held capture lock leaves no slot")
        pool.finish()

    def test_failures_become_errors(self):
        with self.assertRaisesRegex(RuntimeError, "exited with code 3(.|\n)*something broke"):
            self.run_quiet(scenario(["--fail"]))
        with self.assertRaisesRegex(RuntimeError, "wrote no answer"):
            self.run_quiet(scenario(["--no-answer"]))
        with contextlib.redirect_stderr(io.StringIO()):
            r = runner.run_scenario(scenario(["--fail"]), self.cfg)
        self.assertEqual(r.status, "error")
        self.assertIn("exited with code 3", r.error)

    def test_a_game_candidate_needs_an_output_file(self):
        with self.assertRaisesRegex(ConfigError, "output_file"):
            Scenario.from_json({"id": "x", "layer": 5, "oracle": {"inline": {}},
                                "candidate": {"kind": "game", "argv": ["a"]}, "comparator": {"kind": "exact"}}, "t")
        with self.assertRaisesRegex(ConfigError, "candidate kind"):
            Scenario.from_json({"id": "x", "layer": 5, "oracle": {"inline": {}},
                                "candidate": {"kind": "film", "argv": ["a"]}, "comparator": {"kind": "exact"}}, "t")


if __name__ == "__main__":
    unittest.main()
