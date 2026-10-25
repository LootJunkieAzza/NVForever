"""Tests for tools/capture/capture_run.py (unattended in-game captures). A fake game
stands in for FalloutNV.exe + nvf_capture; no game or game data is used."""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_run as cr  # noqa: E402
import capture_steps as cs  # noqa: E402


def job(name, manual=False):
    steps = [{"query": "Begin Function { }\nSetFunctionValue 1\nEnd\n", "label": cs.MODINDEX_LABEL, "field": "FalloutNV.esm"},
             {"query": "Begin Function { }\nSetFunctionValue 7\nEnd\n", "label": "q", "field": "count"}]
    if manual:
        steps.append({"manual": "Wait 24 hours."})
    return {"format": cs.STEPS_FORMAT, "name": name, "title": name, "start": "", "steps": steps}


class Clock:
    def __init__(self):
        self.t = 1_800_000_000.0

    def __call__(self):
        return self.t


class FakeGame:
    """Plays the plugin: reads auto.json, writes each job's output, then quits.
    `behaviour`: ok, crash (exits after the first job), no_plugin (never reads auto.json),
    no_quit (keeps running), never_starts, hang (never finishes the jobs), frozen (stops
    responding after reading auto.json)."""

    def __init__(self, capture_dir, clock, behaviour="ok"):
        self.dir, self.clock, self.behaviour = capture_dir, clock, behaviour
        self.alive = False
        self.launched = self.killed = False
        self.auto = None
        self.ticks = 0
        self.saves = None

    def check(self):
        return None

    def running(self):
        return self.alive

    def responding(self):
        return not (self.behaviour == "frozen" and self.ticks >= 1)

    def launch(self):
        self.launched = True
        self.alive = self.behaviour != "never_starts"

    def kill(self):
        self.killed, self.alive = True, False

    def sleep(self, seconds):
        self.clock.t += seconds
        if not self.alive:
            return
        self.ticks += 1
        auto = os.path.join(self.dir, "auto.json")
        if self.ticks == 1 and self.behaviour != "no_plugin":
            with open(auto, encoding="utf-8") as f:
                self.auto = json.load(f)
            os.replace(auto, os.path.join(self.dir, "auto.used.json"))
            with open(os.path.join(self.dir, "nvf_capture.log"), "a", encoding="utf-8") as f:
                f.write(f"{int(self.clock.t)} automatic run: 1 menu line(s), 1 quit line(s)\n")
                f.write("1 an old line\n")
                if self.behaviour == "old_plugin":
                    f.write(f"{int(self.clock.t)} skipping C:\\x\\jobs\\a1.steps.json: data did not match any variant\n")
        if self.ticks == 2 and self.auto is not None and self.behaviour not in ("hang", "frozen", "old_plugin"):
            jobs = sorted(os.listdir(os.path.join(self.dir, "jobs")))
            for n, f in enumerate(j for j in jobs if j.endswith(".steps.json")):
                name = f[:-len(".steps.json")]
                out = os.path.join(self.dir, "out")
                os.makedirs(out, exist_ok=True)
                lines = [{"kind": "start", "job": name},
                         {"kind": "result", "step": 0, "label": cs.MODINDEX_LABEL, "field": "FalloutNV.esm", "value": 0.0},
                         {"kind": "result", "step": 1, "label": "q", "field": "count", "value": 7.0},
                         {"kind": "error", "step": 2, "error": "script call failed"}]
                if not (self.behaviour == "crash" and n > 0):
                    lines.append({"kind": "end", "steps": 3})
                with open(os.path.join(out, f"{name}.{int(self.clock.t)}.jsonl"), "w", encoding="utf-8") as fh:
                    fh.write("".join(json.dumps(x) + "\n" for x in lines))
                if self.behaviour == "crash" and n > 0:
                    self.alive = False
                    return
                os.replace(os.path.join(self.dir, "jobs", f), os.path.join(self.dir, "jobs", "done-" + f))
        if self.ticks == 2 and self.behaviour == "autosave" and self.saves:
            # A quest script's Autosave: the game rotates and rewrites the rolling save.
            old = os.path.join(self.saves, "autosave.fos")
            if os.path.exists(old):
                os.replace(old, old + ".bak")
            with open(old, "w", encoding="utf-8") as f:
                f.write("new autosave")
        if self.ticks == 3 and self.behaviour in ("ok", "old_plugin", "autosave"):
            self.alive = False


class Base(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = self.tmp.name
        self.cap = os.path.join(self.root, "capture")
        self.src = os.path.join(self.root, "src")
        os.makedirs(self.src)
        self.clock = Clock()

    def tearDown(self):
        self.tmp.cleanup()

    def write_job(self, name, manual=False, folder=None):
        path = os.path.join(folder or self.src, f"{name}.steps.json")
        os.makedirs(os.path.dirname(path), exist_ok=True)
        with open(path, "w", encoding="utf-8") as f:
            json.dump(job(name, manual), f)
        return path

    def main(self, args, behaviour="ok"):
        game = FakeGame(self.cap, self.clock, behaviour)
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = cr.main(["--game", self.root, "--save", "nvftest", "--capture-dir", self.cap,
                            "--saves-dir", "", "--timeout", "600"] + args,
                           game=game, clock=self.clock, sleep=game.sleep)
        report = json.loads(out.getvalue()) if out.getvalue() else None
        return code, report, err.getvalue(), game


class TestJobStatus(unittest.TestCase):
    def test_a_cut_line_that_parses_to_a_string_is_skipped(self):
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "out.jsonl")
            with open(path, "w", encoding="utf-8") as f:
                f.write('{"kind": "start"}\n"1|0|0"\n{"kind": "end"}\n')
            self.assertEqual(cr.job_status(path), ("ended", 0))


class TestRun(Base):
    def test_ok_run_reports_every_job_and_disarms(self):
        a, b = self.write_job("a1"), self.write_job("b1")
        trace = os.path.join(self.root, "goldens")
        code, report, err, game = self.main([a, b, "--trace-dir", trace])
        self.assertEqual(code, 0, err)
        self.assertEqual(game.auto["format"], cr.AUTO_FORMAT)
        self.assertEqual(game.auto["menu_console"], ["load nvftest"])
        self.assertEqual(game.auto["quit_console"], ["qqq"])
        self.assertGreater(game.auto["expires"], self.clock.t)
        self.assertEqual({n: s["status"] for n, s in report["jobs"].items()}, {"a1": "ended", "b1": "ended"})
        self.assertEqual(report["jobs"]["a1"]["errors"], 1)
        self.assertEqual(report["problems"], [])
        self.assertFalse(report["game_running"])
        self.assertEqual(len(report["log"]), 1, "only lines from this run")
        self.assertFalse(os.path.exists(os.path.join(self.cap, "auto.json")))
        with open(os.path.join(trace, "a1.trace.json"), encoding="utf-8") as f:
            golden = json.load(f)
        self.assertEqual(golden["observations"], [{"label": "q", "count": 7}])

    def test_console_lines_can_be_changed(self):
        code, report, err, game = self.main([self.write_job("a1"), "--menu-console", 'load "my save"',
                                             "--quit-console", "x", "--quit-console", "y"])
        self.assertEqual(code, 0, err)
        self.assertEqual(game.auto["menu_console"], ['load "my save"'])
        self.assertEqual(game.auto["quit_console"], ["x", "y"])

    def test_crash_is_reported(self):
        code, report, _, _ = self.main([self.write_job("a1"), self.write_job("b1")], "crash")
        self.assertEqual(code, 2)
        self.assertEqual(report["jobs"]["a1"]["status"], "ended")
        self.assertEqual(report["jobs"]["b1"]["status"], "running")
        self.assertIn("exited before every job ended", report["problems"][0])

    def test_old_plugin_is_detected_and_auto_json_removed(self):
        code, report, _, _ = self.main([self.write_job("a1")], "no_plugin")
        self.assertEqual(code, 2)
        self.assertIn("did not read auto.json", report["problems"][0])
        self.assertEqual(report["jobs"]["a1"]["status"], "missing")
        self.assertFalse(os.path.exists(os.path.join(self.cap, "auto.json")))

    def test_game_left_running_after_the_jobs(self):
        code, report, _, _ = self.main([self.write_job("a1")], "no_quit")
        self.assertEqual(code, 0, "every job ended")
        self.assertIn("still running", report["problems"][0])

    def test_no_quit_returns_once_the_jobs_end(self):
        code, report, _, game = self.main([self.write_job("a1"), "--no-quit"], "no_quit")
        self.assertEqual(code, 0)
        self.assertEqual(report["problems"], [])
        self.assertEqual(game.auto["quit_console"], [])
        self.assertTrue(report["game_running"])

    def test_job_the_plugin_cannot_read_is_reported(self):
        code, report, _, _ = self.main([self.write_job("a1")], "old_plugin")
        self.assertEqual(code, 2)
        self.assertEqual(report["jobs"]["a1"]["status"], "missing")
        self.assertIn("plugin: skipping", report["problems"][-1])
        self.assertIn("a1.steps.json", report["problems"][-1])

    def test_game_that_never_starts(self):
        code, report, _, _ = self.main([self.write_job("a1")], "never_starts")
        self.assertEqual(code, 2)
        self.assertIn("did not start", report["problems"][0])

    def test_timeout_and_kill(self):
        code, report, _, game = self.main([self.write_job("a1"), "--kill-on-timeout"], "hang")
        self.assertEqual(code, 2)
        self.assertIn("timed out", report["problems"][0])
        self.assertTrue(game.killed)
        self.assertFalse(report["game_running"])


    def test_hung_game_is_reported_and_closed(self):
        code, report, _, game = self.main([self.write_job("a1"), "--kill-on-timeout"], "frozen")
        self.assertEqual(code, 2)
        self.assertIn("stopped responding", report["problems"][0])
        self.assertLess(self.clock.t - report["started"], 600, "well before the timeout")
        self.assertTrue(game.killed)


class TestInput(Base):
    def test_manual_steps_need_attended(self):
        p = self.write_job("m1", manual=True)
        code, _, err, game = self.main([p])
        self.assertEqual(code, 1)
        self.assertIn("--attended", err)
        self.assertFalse(game.launched)
        code, _, err, _ = self.main([p, "--attended"])
        self.assertEqual(code, 0, err)

    def test_other_pending_jobs_are_refused(self):
        self.write_job("zz_old", folder=os.path.join(self.cap, "jobs"))
        code, _, err, _ = self.main([self.write_job("a1")])
        self.assertEqual(code, 1)
        self.assertIn("zz_old", err)
        code, report, err, _ = self.main([self.write_job("a1"), "--include-pending"])
        self.assertEqual(code, 0, err)
        self.assertEqual(sorted(report["jobs"]), ["a1", "zz_old"])

    def test_file_name_must_match_job_name(self):
        p = os.path.join(self.src, "other.steps.json")
        with open(p, "w", encoding="utf-8") as f:
            json.dump(job("a1"), f)
        code, _, err, _ = self.main([p])
        self.assertEqual(code, 1)
        self.assertIn("a1.steps.json", err)

    def test_running_game_is_refused(self):
        game = FakeGame(self.cap, self.clock)
        game.alive = True
        with redirect_stdout(io.StringIO()), redirect_stderr(io.StringIO()) as err:
            code = cr.main(["--game", self.root, "--save", "s", "--capture-dir", self.cap, "--saves-dir", "",
                            self.write_job("a1")], game=game, clock=self.clock, sleep=game.sleep)
        self.assertEqual(code, 1)
        self.assertIn("already running", err.getvalue())

    def test_save_check(self):
        saves = os.path.join(self.root, "Saves")
        os.makedirs(saves)
        self.assertIn("save nvftest", cr.check_save("nvftest", saves))
        open(os.path.join(saves, "nvftest.fos"), "wb").close()
        self.assertIsNone(cr.check_save("nvftest", saves))
        self.assertIsNone(cr.check_save("nvftest", os.path.join(self.root, "nowhere")), "unknown location: not checked")

    def test_real_game_check(self):
        self.assertIn(cr.LOADER_EXE, cr.Game(self.root).check())


if __name__ == "__main__":
    unittest.main()


class TestSavesGuard(Base):
    """The game can write to the Saves folder by itself (a scripted autosave): the run backs
    up the rolling saves first, reports every change and fails, and never writes there."""

    def setUp(self):
        super().setUp()
        self.saves = os.path.join(self.root, "Saves")
        os.makedirs(self.saves)
        for name, text in (("nvftest.fos", "test"), ("autosave.fos", "owner autosave"), ("Save 3.fos", "owner")):
            with open(os.path.join(self.saves, name), "w", encoding="utf-8") as f:
                f.write(text)

    def run_with(self, behaviour):
        game = FakeGame(self.cap, self.clock, behaviour)
        game.saves = self.saves
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = cr.main(["--game", self.root, "--save", "nvftest", "--capture-dir", self.cap,
                            "--saves-dir", self.saves, "--timeout", "600", self.write_job("a1")],
                           game=game, clock=self.clock, sleep=game.sleep)
        return code, json.loads(out.getvalue())

    def test_untouched_saves_pass_and_rolling_saves_are_backed_up(self):
        code, report = self.run_with("ok")
        self.assertEqual(code, 0)
        self.assertNotIn("saves_changed", report)
        backups = os.listdir(os.path.join(self.cap, "saves-backup"))
        self.assertEqual(len(backups), 1)
        self.assertEqual(os.listdir(os.path.join(self.cap, "saves-backup", backups[0])), ["autosave.fos"])

    def test_a_scripted_autosave_fails_the_run_and_keeps_the_owner_copy(self):
        code, report = self.run_with("autosave")
        self.assertEqual(code, 3)
        self.assertEqual(report["saves_changed"], ["changed autosave.fos", "added autosave.fos.bak"])
        self.assertTrue(any("wrote to the Saves folder" in p for p in report["problems"]))
        backup = os.path.join(self.cap, "saves-backup", os.listdir(os.path.join(self.cap, "saves-backup"))[0])
        with open(os.path.join(backup, "autosave.fos"), encoding="utf-8") as f:
            self.assertEqual(f.read(), "owner autosave")
        # Nothing was restored into the Saves folder.
        with open(os.path.join(self.saves, "autosave.fos"), encoding="utf-8") as f:
            self.assertEqual(f.read(), "new autosave")

    def test_listing_helpers(self):
        self.assertIsNone(cr.saves_listing(""))
        self.assertEqual(cr.saves_changes({"a": [1, 1]}, {"a": [1, 2], "b": [1, 1]}), ["changed a", "added b"])
        self.assertEqual(cr.saves_changes({"a": [1, 1]}, {}), ["removed a"])
        self.assertEqual(cr.saves_changes(None, {"a": [1, 1]}), [])
