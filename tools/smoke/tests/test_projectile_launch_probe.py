"""Actual launcher/judge boundaries for the authored physical aid; no game runs."""
import dataclasses
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))
from smoke.plan import plan  # noqa: E402
from smoke.runner import Env, judge, run_launch  # noqa: E402
from smoke.scenarios import load  # noqa: E402


class ProjectileLaunchProbeTests(unittest.TestCase):
    def scenario(self):
        suite = load(HERE / "scenarios.toml")
        return suite, suite.by_name()["projectile-launch-physical-session-traits"]

    def test_actual_launch_delivers_loaded_editor_id_and_exact_owned_home(self):
        suite, scenario = self.scenario()
        launch, = plan(suite, [scenario])
        with tempfile.TemporaryDirectory() as temporary:
            out = Path(temporary)
            home = out / "b42-launch-probe-home"
            home.mkdir()
            (home / "owned-input.txt").write_text("authored launch boundary", encoding="utf-8")
            child = out / "record.py"
            child.write_text(
                "import json, os\nfrom pathlib import Path\n"
                "home = Path(os.environ['NVFOREVER_HOME']).resolve()\n"
                "print(json.dumps({'aid': os.environ.get('NVF_PLAY_TEST_PROJECTILE_LAUNCH'), "
                "'home': str(home), 'cwd': os.getcwd(), "
                "'input': (home / 'owned-input.txt').read_text(encoding='utf-8')}))\n",
                encoding="utf-8")
            if os.name == "nt":
                exe = out / "record.cmd"
                exe.write_text(f'@"{sys.executable}" "{child}"\n', encoding="utf-8")
            else:
                exe = out / "record.sh"
                exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}"\n', encoding="utf-8")
                exe.chmod(0o755)
            environment = Env(repo=HERE.parent.parent, out=out, exe=exe,
                              data="unused-by-authored-recorder", fnvtool=None)
            code, error, _ = run_launch(launch, environment, None)
            self.assertEqual((code, error), (0, ""))
            actual = json.loads((out / f"{launch.name}.log").read_text(encoding="utf-8"))
            self.assertEqual(actual["aid"], "Weap10mmPistol")
            self.assertEqual(Path(actual["home"]), home.resolve())
            self.assertEqual(Path(actual["cwd"]), out.resolve())
            self.assertEqual(actual["input"], "authored launch boundary")
            self.assertFalse((home / "saves").exists())
            # The inherited aid cannot opt an ordinary launch into weapon use.
            ordinary = dataclasses.replace(launch, name="ordinary-probe-neighbor",
                env={key: value for key, value in launch.env.items()
                     if key != "NVF_PLAY_TEST_PROJECTILE_LAUNCH"})
            with patch.dict(os.environ, {"NVF_PLAY_TEST_PROJECTILE_LAUNCH": "InheritedWrongWeapon"}):
                code, error, _ = run_launch(ordinary, environment, None)
            self.assertEqual((code, error), (0, ""))
            actual = json.loads((out / f"{ordinary.name}.log").read_text(encoding="utf-8"))
            self.assertIsNone(actual["aid"])
            self.assertEqual((home / "owned-input.txt").read_text(encoding="utf-8"),
                             "authored launch boundary")

    def test_judge_needs_two_physical_insertions_and_successful_exact_cleanup(self):
        _, scenario = self.scenario()
        markers = [
            "projectile launch probe: armed loaded=Weap10mmPistol",
            "projectile launch probe: physical stage=FirstPending velocity=500 speed=500 force=7",
            "projectile launch probe: physical stage=SecondPending velocity=1000 speed=1000 force=10",
            "projectile launch probe: PASS stages=2 requests=2 physical_insertions=2 exact_fields_restored=true inventory_restored=true",
            "run finished",
        ]
        self.assertEqual(judge(scenario, markers, 0).status, "PASS")
        for index in range(4):
            self.assertEqual(judge(scenario, markers[:index] + markers[index + 1:], 0).status, "FAIL")
        self.assertEqual(judge(scenario, markers + ["projectile launch probe: FAIL cleanup"], 0).status, "FAIL")
        self.assertEqual(judge(scenario, markers, 1).status, "FAIL")
        self.assertEqual(judge(scenario, [markers[0], markers[2], markers[1], *markers[3:]], 0).status, "FAIL")
        # These synthetic log rows only test the judge. They are not game results.


if __name__ == "__main__":
    unittest.main()
