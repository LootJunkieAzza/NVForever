"""Game env placeholders at run_launch's real subprocess boundary; no game assets."""
import argparse
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from smoke import cli, runner  # noqa: E402
from smoke.plan import plan  # noqa: E402
from smoke.scenarios import load_text  # noqa: E402


TEXTURE = "NVF_GLOBAL_IMAGE_METRICS_TEXTURE"
HOME = "NVF_PLAY_TEST_INLINE_SOURCE_HOME"
UDF = Path("mods/NVSE/user_defined_functions/NVFInlineOperands/owned_self_distance.gek")
CONSOLE = ('10:PrintC "owned injected %g", (InjectUIComponent "globals()" '
           '"<image name=%qOwnedImage%q><visible>0</visible>'
           '<filename>{env:NVF_GLOBAL_IMAGE_METRICS_TEXTURE}</filename></image>")')
QUERY = "{repo}/tools/smoke/fixtures/vanilla-inline-call-operands.steps.json"
OWNED_UDF = "Begin Function {}\nSetFunctionValue 1\nEnd\n"


class GameEnvTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="nvf game env ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.out = self.root / "out"
        self.out.mkdir()
        self.repo = self.root / "owned repo"
        self.repo.mkdir()
        # The stand-in child reads what Popen actually delivered. It performs no
        # parsing, engine work or graphics; only the authored file-path boundary.
        child = self.root / "child.py"
        child.write_text('''import json, os, re
from pathlib import Path
env = dict(os.environ)
console = env.get("NVF_PLAY_TEST_CONSOLE", "")
match = re.search(r"<filename>(.*?)</filename>", console)
filename = match[1] if match else None
texture = Path(env["NVF_SMOKE_SELFTEST_TEXTURE_ROOT"]) / filename if filename else None
udf = Path(env.get("NVFOREVER_HOME", "")) / "mods/NVSE/user_defined_functions/NVFInlineOperands/owned_self_distance.gek"
print("child receipt " + json.dumps({
    "console": console, "filename": filename,
    "texture": texture.read_text(encoding="utf-8") if texture and texture.is_file() else None,
    "home": env.get("NVFOREVER_HOME"),
    "udf": udf.read_text(encoding="utf-8") if udf.is_file() else None,
    "queries": env.get("NVF_PLAY_TEST_QUERIES"),
    "literal": env.get("NVF_SMOKE_SELFTEST_LITERAL"),
    "empty": env.get("NVF_PLAY_TEST_TILE_MENUS"),
    "repo": env.get("NVF_SMOKE_SELFTEST_REPO"),
    "serve": env.get("NVF_SMOKE_SELFTEST_SERVE"),
    "from_scripts": env.get("NVF_PLAY_AIDS_FROM_SCRIPTS"),
    "unrelated_aid": env.get("NVF_PLAY_TEST_UNRELATED_ENV_TEST"),
    "source_home_optin": env.get("NVF_PLAY_TEST_INLINE_SOURCE_HOME")
}, ensure_ascii=True))
''', encoding="utf-8")
        if os.name == "nt":
            self.exe = self.root / "child.cmd"
            self.exe.write_text(f'@"{sys.executable}" "{child}"\n', encoding="utf-8")
        else:
            self.exe = self.root / "child.sh"
            self.exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}"\n', encoding="utf-8")
            self.exe.chmod(0o755)
        self.env = runner.Env(repo=self.repo, out=self.out, exe=self.exe, data="owned data", fnvtool=None)

    def launch(self, env, requires=(), serve=False):
        values = dict(env, NVF_SMOKE_SELFTEST_TEXTURE_ROOT=str(self.root))
        text = '\n'.join([
            '[[scenario]]', 'name = "game-env"', 'paths = ["tools/smoke/"]', 'args = ["--play"]',
            'run_seconds = 1',
            'env = { ' + ', '.join(json.dumps(k) + ' = ' + json.dumps(v) for k, v in values.items()) + ' }',
            'requires = ' + json.dumps(list(requires)),
            'serve = { status = 200, body = "owned" }' if serve else '',
        ])
        suite = load_text(text)
        return plan(suite, suite.scenarios)[0]

    def run_child(self, launch):
        code, err, _ = runner.run_launch(launch, self.env, None)
        self.assertEqual((code, err), (0, ""))
        lines = (self.out / "game-env.log").read_text(encoding="utf-8").splitlines()
        rows = [line[len("child receipt "):] for line in lines if line.startswith("child receipt ")]
        self.assertEqual(len(rows), 1, lines)
        return json.loads(rows[0])

    def test_console_filename_reaches_child_and_owned_texture(self):
        name = "owned texture with spaces.dds"
        (self.root / name).write_text("authored texture boundary sentinel", encoding="utf-8")
        with mock.patch.dict(os.environ, {TEXTURE: name}):
            launch = self.launch({"NVF_PLAY_TEST_CONSOLE": CONSOLE}, [TEXTURE])
            self.assertEqual(runner.missing_requirements(launch.scenarios[0]), [])
            receipt = self.run_child(launch)
        self.assertEqual(receipt["filename"], name)
        self.assertEqual(receipt["console"], CONSOLE.replace("{env:" + TEXTURE + "}", name))
        self.assertEqual(receipt["texture"], "authored texture boundary sentinel")
        self.assertIn("<filename>" + name + "</filename>", (self.out / "game-env.cmd.txt").read_text())

    def test_seeded_home_reaches_child_and_owned_udf(self):
        home = self.root / "seeded writable home"
        udf = home / UDF
        udf.parent.mkdir(parents=True)
        udf.write_text(OWNED_UDF, encoding="utf-8")
        with mock.patch.dict(os.environ, {HOME: str(home)}):
            launch = self.launch({"NVFOREVER_HOME": "{env:" + HOME + "}", "NVF_PLAY_TEST_QUERIES": QUERY}, [HOME])
            self.assertEqual(runner.missing_requirements(launch.scenarios[0]), [])
            receipt = self.run_child(launch)
        self.assertEqual(receipt["home"], str(home))
        self.assertEqual(receipt["udf"], OWNED_UDF)
        self.assertEqual(receipt["queries"], QUERY.replace("{repo}", str(self.repo)))
        # Expansion reads the parent's opt-in before inherited test aids are stripped.
        self.assertIsNone(receipt["source_home_optin"])

    def assert_refused(self, name, value):
        with mock.patch.dict(os.environ):
            if value is None:
                os.environ.pop(name, None)
            else:
                os.environ[name] = value
            values = {"NVF_PLAY_TEST_CONSOLE": CONSOLE} if name == TEXTURE else {"NVFOREVER_HOME": "{env:" + HOME + "}"}
            # Direct run_launch must also refuse; it cannot rely on the CLI's requires filter.
            launch = self.launch(values)
            with mock.patch.object(runner.subprocess, "Popen", side_effect=AssertionError("missing env placeholder reached Popen")) as start:
                code, err, _ = runner.run_launch(launch, self.env, None)
            start.assert_not_called()
        self.assertIsNone(code)
        self.assertIn(name, err)
        self.assertFalse((self.out / "game-env.log").exists())

    def test_unset_texture_refuses_before_popen(self):
        self.assert_refused(TEXTURE, None)

    def test_empty_texture_refuses_before_popen(self):
        self.assert_refused(TEXTURE, "")

    def test_unset_home_refuses_before_popen(self):
        self.assert_refused(HOME, None)

    def test_empty_home_refuses_before_popen(self):
        self.assert_refused(HOME, "")

    def test_cli_requires_skip_without_popen(self):
        for name, values in [(TEXTURE, {"NVF_PLAY_TEST_CONSOLE": CONSOLE}), (HOME, {"NVFOREVER_HOME": "{env:" + HOME + "}"})]:
            with self.subTest(name=name), mock.patch.dict(os.environ):
                os.environ.pop(name, None)
                launch = self.launch(values, [name])
                with mock.patch.object(runner.subprocess, "Popen", side_effect=AssertionError("missing opt-in reached Popen")) as start:
                    verdicts = cli.run_launches([launch], self.env, None, 1, argparse.Namespace(context=3), lambda _: None)
                start.assert_not_called()
                self.assertEqual([v.status for v in verdicts], ["SKIP"])
                self.assertIn(name, verdicts[0].reason)

    def test_context_empty_and_literal_values_reach_child_unchanged(self):
        # This must not borrow expand()'s tool-only glob/list behavior.
        (self.out / "a.dds").write_text("not a glob expansion", encoding="utf-8")
        values = {"NVF_SMOKE_SELFTEST_LITERAL": "keep *.dds [7] with spaces", "NVF_PLAY_TEST_TILE_MENUS": "", "NVF_SMOKE_SELFTEST_REPO": "{repo}/fixture", "NVF_SMOKE_SELFTEST_SERVE": "{serve}/owned"}
        with mock.patch.dict(os.environ, {"NVF_PLAY_TEST_UNRELATED_ENV_TEST": "do not inherit"}):
            receipt = self.run_child(self.launch(values, serve=True))
        self.assertEqual(receipt["literal"], values["NVF_SMOKE_SELFTEST_LITERAL"])
        self.assertEqual(receipt["empty"], "")
        self.assertEqual(receipt["repo"], str(self.repo) + "/fixture")
        self.assertRegex(receipt["serve"], r"^http://127\.0\.0\.1:\d+/owned$")
        self.assertEqual(receipt["from_scripts"], "1")
        self.assertIsNone(receipt["unrelated_aid"])


if __name__ == "__main__":
    unittest.main()
