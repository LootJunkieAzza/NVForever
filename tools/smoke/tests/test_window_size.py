"""`run`/`plan --size WxH` and `--extra-args`: every nvforever launch of a run in one window
size, with more arguments (smoke/plan.py with_window, smoke/cli.py); fnvtool runs untouched."""

import contextlib
import io
import json
import os
import stat
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))

from smoke import cli  # noqa: E402
from smoke.plan import plan, split_args, window_size, with_window  # noqa: E402
from smoke.scenarios import GAME, load_text  # noqa: E402

BASE = ["--cell", "Goodsprings", "--hour", "9"]  # no --play: that adds --no-messages


def game(name, args, extra=""):
    return (f'[[scenario]]\nname = "{name}"\npaths = ["engine/"]\nargs = {json.dumps(args)}\n'
            f"run_seconds = 5\n{extra}\n")


def tool(name):
    return f'[[scenario]]\nname = "{name}"\nkind = "tool"\npaths = ["crates/"]\ncommands = [["info", "--size", "7x7"], ["ext"]]\n'


class WithWindowTests(unittest.TestCase):
    def test_replaces_a_size_in_either_form(self):
        self.assertEqual(with_window(BASE + ["--size", "1280x720", "--hour", "12"], "3840x2160"),
                         BASE + ["--size", "3840x2160", "--hour", "12"])
        self.assertEqual(with_window(BASE + ["--size=1280x720"], "3840x2160"), BASE + ["--size=3840x2160"])

    def test_appends_when_there_is_none(self):
        self.assertEqual(with_window(BASE, "3840x2160"), BASE + ["--size", "3840x2160"])
        self.assertEqual(with_window(BASE), BASE)

    def test_extra_args_follow_and_the_size_wins_over_theirs(self):
        self.assertEqual(with_window(BASE, None, ["--graphics", "enhanced"]), BASE + ["--graphics", "enhanced"])
        self.assertEqual(with_window(BASE + ["--size", "1280x720"], "3840x2160", ["--size=640x480"]),
                         BASE + ["--size", "3840x2160", "--size=3840x2160"])

    def test_the_value_is_checked(self):
        self.assertEqual(window_size("3840x2160"), "3840x2160")
        for bad in ("4k", "3840", "3840x", "x2160", "3840X2160", "3840x2160x1", "0x2160", "-1x5", " 3840x2160"):
            with self.subTest(bad=bad), self.assertRaises(ValueError):
                window_size(bad)

    def test_split_args(self):
        self.assertEqual(split_args("--graphics enhanced"), ["--graphics", "enhanced"])
        self.assertEqual(split_args('--home "C:\\my dir\\h" --no-music'), ["--home", "C:\\my dir\\h", "--no-music"])


class PlanTests(unittest.TestCase):
    def test_merged_launch_gets_it_once(self):
        args = BASE + ["--size", "1280x720"]
        suite = load_text(game("a", args, 'launch = "L"') + game("b", args, 'launch = "L"') + game("c", BASE) + tool("t"))
        launches = plan(suite, suite.scenarios, size="3840x2160", extra=["--graphics", "enhanced"])
        by = {x.name: x for x in launches}
        self.assertEqual([s.name for s in by["L"].scenarios], ["a", "b"])
        self.assertEqual(by["L"].args, BASE + ["--size", "3840x2160", "--graphics", "enhanced"])
        self.assertEqual(by["c"].args, BASE + ["--graphics", "enhanced", "--size", "3840x2160"])
        self.assertEqual(by["t"].args, [])  # fnvtool: untouched
        self.assertEqual(by["t"].scenarios[0].commands, [["info", "--size", "7x7"], ["ext"]])
        # The scenarios themselves keep their own arguments.
        self.assertEqual(suite.scenarios[0].args, args)
        # --no-merge: each launch on its own, each once.
        alone = plan(suite, suite.scenarios, True, "3840x2160")
        self.assertEqual([x.args.count("3840x2160") for x in alone if x.kind == GAME], [1, 1, 1])

    def test_nothing_given_changes_nothing(self):
        suite = load_text(game("a", BASE + ["--size", "1280x720"]))
        self.assertEqual(plan(suite, suite.scenarios)[0].args, BASE + ["--size", "1280x720"])


def fake_program(d: Path, name: str) -> Path:
    """A program that prints its arguments and the line a game run ends with."""
    script = d / f"{name}.py"
    script.write_text("import sys\nprint('argv', sys.argv[1:])\n"
                      "print('2026-10-11T10:00:10.000000Z  INFO nvforever: run finished after 5.0s: 1 cells loaded')\n")
    if os.name == "nt":
        exe = d / f"{name}.cmd"
        exe.write_text(f'@"{sys.executable}" "{script}" %*\n')
    else:
        exe = d / name
        exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{script}" "$@"\n')
        exe.chmod(exe.stat().st_mode | stat.S_IEXEC)
    return exe


class CommandLineTests(unittest.TestCase):
    def files(self, d: Path) -> Path:
        sc = d / "s.toml"
        sc.write_text(game("a", BASE + ["--size", "1280x720"], 'launch = "L"')
                      + game("b", BASE + ["--size", "1280x720"], 'launch = "L"')
                      + game("c", BASE + ["--size=1280x720"]) + tool("t"), encoding="utf-8")
        return sc

    def main(self, argv) -> tuple[int, str, str]:
        out, err = io.StringIO(), io.StringIO()
        with contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
            try:
                code = cli.main(argv)
            except SystemExit as e:
                code = e.code
        return code, out.getvalue(), err.getvalue()

    def test_plan_shows_it(self):
        with tempfile.TemporaryDirectory() as d:
            sc = self.files(Path(d))
            code, text, _ = self.main(["plan", "--all", "--scenarios", str(sc), "--out", str(Path(d) / "o"),
                                       "--size", "3840x2160", "--extra-args", "--graphics enhanced"])
        self.assertEqual(code, 0)
        self.assertIn("every game launch with: --size 3840x2160 --graphics enhanced", text)
        self.assertIn("L: a b | --cell Goodsprings --hour 9 --size 3840x2160 --graphics enhanced --run-seconds 5", text)
        self.assertIn("c: c | --cell Goodsprings --hour 9 --size=3840x2160 --graphics enhanced --run-seconds 5", text)
        self.assertNotIn("1280x720", text)

    def test_bad_size_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            sc = self.files(Path(d))
            for cmd in ("plan", "run"):
                code, _, err = self.main([cmd, "--all", "--scenarios", str(sc), "--size", "4k"])
                self.assertEqual(code, 2)
                self.assertIn("argument --size: expected WxH", err)
                self.assertIn("'4k'", err)

    def test_run_launches_with_it_and_records_it(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            sc, out = self.files(d), d / "out"
            exe, fnvtool = fake_program(d, "nvforever"), fake_program(d, "fnvtool")
            code, text, _ = self.main(["run", "--all", "--scenarios", str(sc), "--out", str(out), "--no-locks",
                                       "--exe", str(exe), "--fnvtool", str(fnvtool), "--data", "D",
                                       "--size", "3840x2160", "--extra-args", "--graphics enhanced",
                                       "--extra-args=--no-music"])
            self.assertEqual(code, 0, text)
            self.assertIn("smoke: 4 passed, 0 failed, 0 known failing, 0 skipped", text)
            self.assertIn("at 3840x2160 with --graphics enhanced --no-music", text)
            shared = (out / "L.cmd.txt").read_text()
            self.assertIn("--size 3840x2160 --graphics enhanced --no-music --run-seconds 5", shared)
            self.assertNotIn("1280x720", shared)
            self.assertIn("--size=3840x2160 --graphics enhanced --no-music", (out / "c.cmd.txt").read_text())
            self.assertIn("'--size', '3840x2160'", (out / "L.log").read_text())
            tool_cmd = (out / "t.cmd.txt").read_text()
            self.assertIn("info --size 7x7", tool_cmd)
            self.assertNotIn("3840x2160", tool_cmd)
            self.assertNotIn("--graphics", tool_cmd)
            rows = {r["scenario"]: r for r in json.loads((out / "summary.json").read_text())}
            for n in ("a", "b", "c"):
                self.assertEqual(rows[n]["size"], "3840x2160")
                self.assertEqual(rows[n]["extra_args"], ["--graphics", "enhanced", "--no-music"])
            self.assertNotIn("size", rows["t"])
            self.assertNotIn("extra_args", rows["t"])
            # --failed still reads the rows.
            self.assertEqual(cli.last_failed(load_text(sc.read_text()).scenarios, out / "summary.json", lambda _: None), [])

    def test_a_run_without_them_records_nothing(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            sc, out = self.files(d), d / "out"
            exe = fake_program(d, "nvforever")
            code, text, _ = self.main(["run", "--only", "c", "--scenarios", str(sc), "--out", str(out), "--no-locks",
                                       "--exe", str(exe), "--data", "D"])
            self.assertEqual(code, 0, text)
            self.assertNotIn("every game launch with", text)
            self.assertIn("--size=1280x720", (out / "c.cmd.txt").read_text())
            [row] = json.loads((out / "summary.json").read_text())
            self.assertEqual(set(row), {"scenario", "status", "failures"})


if __name__ == "__main__":
    unittest.main()
