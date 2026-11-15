"""Game environment delivery through the real runner and an owned Python child."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path
from unittest import mock

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke import cli
from smoke.plan import plan
from smoke.runner import Env, run_launch
from smoke.scenarios import load_text


class LaunchEnvironmentTests(unittest.TestCase):
    def launch(self, out, values, requires=()):
        table = ", ".join(f"{key} = {json.dumps(value)}" for key, value in values.items())
        suite = load_text(f'''
[[scenario]]
name = "environment"
paths = ["tools/smoke/"]
args = ["--play"]
run_seconds = 1
env = {{ {table} }}
requires = {json.dumps(list(requires))}
''')
        child = out / "child.py"
        child.write_text('''import json, os, re
from pathlib import Path
Path("started").write_text("owned child", encoding="utf-8")
values = {key: os.environ.get(key) for key in
          ("NVF_PLAY_TEST_CONSOLE", "NVFOREVER_HOME", "NVF_SMOKE_SCALAR")}
console = values["NVF_PLAY_TEST_CONSOLE"] or ""
filename = re.search(r"<filename>(.*?)</filename>", console)
if filename:
    path = Path(filename[1])
    values["texture"] = path.read_text(encoding="utf-8") if path.is_file() else None
home = values["NVFOREVER_HOME"]
if home:
    udf = Path(home) / "mods/NVSE/user_defined_functions/NVFInlineOperands/probe.txt"
    values["udf"] = udf.read_text(encoding="utf-8") if udf.is_file() else None
print(json.dumps(values))
''', encoding="utf-8")
        if os.name == "nt":
            exe = out / "child.cmd"
            exe.write_text(f'@"{sys.executable}" "{child}"\n')
        else:
            exe = out / "child.sh"
            exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}"\n')
            exe.chmod(0o755)
        env = Env(repo=out, out=out, exe=exe, data="unused authored data", fnvtool=None)
        return plan(suite, suite.scenarios)[0], env

    def delivered(self, out):
        return json.loads((out / "environment.log").read_text(encoding="utf-8"))

    def test_texture_filename_reaches_the_child_inside_console_source(self):
        variable = "NVF_GLOBAL_IMAGE_METRICS_TEXTURE"
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            texture = out / "authored texture.dds"
            texture.write_text("authored file sentinel", encoding="utf-8")
            source = '3:SetUIString "HUDMainMenu/probe" "<image><filename>{env:' + variable + '}</filename></image>"'
            launch, env = self.launch(out, {"NVF_PLAY_TEST_CONSOLE": source}, [variable])
            with mock.patch.dict(os.environ, {variable: str(texture)}):
                self.assertEqual(run_launch(launch, env, None)[:2], (0, ""))
            actual = self.delivered(out)
            self.assertEqual(actual["NVF_PLAY_TEST_CONSOLE"], source.replace("{env:" + variable + "}", str(texture)))
            self.assertEqual(actual["texture"], "authored file sentinel")
            self.assertIn(str(texture), (out / "environment.cmd.txt").read_text())

    def test_seeded_home_udf_is_read_from_the_selected_home(self):
        variable = "NVF_PLAY_TEST_INLINE_SOURCE_HOME"
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            home = out / "seeded home"
            udf = home / "mods/NVSE/user_defined_functions/NVFInlineOperands/probe.txt"
            udf.parent.mkdir(parents=True)
            udf.write_text("authored UDF sentinel", encoding="utf-8")
            launch, env = self.launch(out, {"NVFOREVER_HOME": "{env:" + variable + "}"}, [variable])
            with mock.patch.dict(os.environ, {variable: str(home)}):
                self.assertEqual(run_launch(launch, env, None)[:2], (0, ""))
            self.assertEqual(self.delivered(out)["NVFOREVER_HOME"], str(home))
            self.assertEqual(self.delivered(out)["udf"], "authored UDF sentinel")

    def test_absent_or_empty_opt_ins_skip_normally_and_direct_launch_starts_nothing(self):
        import argparse
        for variable, key in (("NVF_GLOBAL_IMAGE_METRICS_TEXTURE", "NVF_PLAY_TEST_CONSOLE"),
                              ("NVF_PLAY_TEST_INLINE_SOURCE_HOME", "NVFOREVER_HOME")):
            for value in (None, ""):
                with self.subTest(variable=variable, value=value), tempfile.TemporaryDirectory() as tmp:
                    out = Path(tmp)
                    launch, env = self.launch(out, {key: "{env:" + variable + "}"}, [variable])
                    with mock.patch.dict(os.environ):
                        os.environ.pop(variable, None)
                        if value is not None:
                            os.environ[variable] = value
                        verdicts = cli.run_launches([launch], env, None, 1, argparse.Namespace(context=3), lambda _: None)
                        self.assertEqual([(v.status, v.reason) for v in verdicts], [("SKIP", "set " + variable)])
                        self.assertFalse((out / "started").exists())
                        code, error, _ = run_launch(launch, env, None)
                    self.assertIsNone(code)
                    self.assertIn(variable, error)
                    self.assertFalse((out / "started").exists())

    def test_environment_values_are_scalar_and_inserted_text_is_not_reexpanded(self):
        variable = "NVF_SMOKE_SELFTEST_SCALAR"
        literal = 'a*?[x] "two words"; {env:NVF_SMOKE_MUST_NOT_BE_READ}'
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            launch, env = self.launch(out, {"NVF_SMOKE_SCALAR": "prefix {env:" + variable + "} suffix"})
            with mock.patch.dict(os.environ, {variable: literal, "NVF_SMOKE_MUST_NOT_BE_READ": "wrong"}):
                self.assertEqual(run_launch(launch, env, None)[:2], (0, ""))
            self.assertEqual(self.delivered(out)["NVF_SMOKE_SCALAR"], "prefix " + literal + " suffix")

    def test_repo_substitution_still_reaches_the_child(self):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            launch, env = self.launch(out, {"NVF_SMOKE_SCALAR": "{repo}/authored"})
            self.assertEqual(run_launch(launch, env, None)[:2], (0, ""))
            self.assertEqual(self.delivered(out)["NVF_SMOKE_SCALAR"], str(out) + "/authored")
