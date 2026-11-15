"""Actual registry -> planner -> run_launch -> owned child, without a game.

This checks launch delivery only. The Rust recovery and successor tests exercise
the actual save policy, CLI and typed save consumer separately.
"""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
from smoke import runner
from smoke.plan import plan
from smoke.scenarios import load_text


class DeathRecoveryLaunchTests(unittest.TestCase):
    def deliver(self, name):
        registry = Path(os.environ.get("NVF_DEATH_BOUNDARY_REGISTRY", Path(__file__).resolve().parent.parent / "scenarios.toml"))
        suite = load_text(registry.read_bytes().decode("utf-8"), base=registry.parent)
        selected = [s for s in suite.scenarios if s.name == name]
        self.assertEqual(len(selected), 1)
        launch, = plan(suite, selected, no_merge=True)
        with tempfile.TemporaryDirectory(prefix="nvf owned death boundary ") as temporary:
            root = Path(temporary)
            child = root / "recorder.py"
            child.write_text('''import json, os, sys
from pathlib import Path
home = os.environ.get("NVFOREVER_HOME")
resolved = Path(home).resolve() if home else None
if resolved:
    assert resolved.parent == Path.cwd(), "only an owned home is admitted by this recorder"
    (resolved / "saves").mkdir(parents=True)
    (resolved / "profile.marker").write_text("authored owned profile", encoding="utf8")
print("DEATH_BOUNDARY " + json.dumps({"argv":sys.argv[1:], "cwd":str(Path.cwd()),
    "home":home, "resolved_home":str(resolved) if resolved else None,
    "opt_in":os.environ.get("NVF_PLAY_TEST_DEATH_RECOVERY"),
    "clock":os.environ.get("NVF_PLAY_AIDS_FROM_SCRIPTS"),
    "console":os.environ.get("NVF_PLAY_TEST_CONSOLE"),
    "hit":os.environ.get("NVF_PLAY_TEST_HIT"),
    "marker":(resolved / "profile.marker").read_text(encoding="utf8") if resolved else None}))
''', encoding="utf8")
            if os.name == "nt":
                executable = root / "recorder.cmd"
                executable.write_text(f'@"{sys.executable}" "{child}" %*\n', encoding="utf8")
            else:
                executable = root / "recorder.sh"
                executable.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}" "$@"\n', encoding="utf8")
                executable.chmod(0o755)
            environment = runner.Env(repo=registry.parents[2], out=root, exe=executable, data="authored-read-only-data", fnvtool=None)
            code, error, _ = runner.run_launch(launch, environment, None)
            self.assertEqual((code, error), (0, ""))
            lines = (root / f"{launch.name}.log").read_text(encoding="utf8").splitlines()
            receipts = [json.loads(line.removeprefix("DEATH_BOUNDARY ")) for line in lines if line.startswith("DEATH_BOUNDARY ")]
            self.assertEqual(len(receipts), 1, lines)
            receipt = receipts[0]
            self.assertEqual(receipt["argv"], ["--data", "authored-read-only-data", *launch.args, "--run-seconds", runner.fmt(launch.run_seconds)])
            self.assertEqual(receipt["clock"], "1")
            self.assertEqual(receipt["console"], launch.env.get("NVF_PLAY_TEST_CONSOLE"))
            self.assertEqual(receipt["hit"], launch.env.get("NVF_PLAY_TEST_HIT"))
            if receipt["home"]:
                self.assertEqual(Path(receipt["resolved_home"]).parent, root)
                self.assertEqual(receipt["marker"], "authored owned profile")
            return receipt, launch

    def test_recovery_positive_delivers_explicit_opt_in_to_the_owned_home(self):
        receipt, launch = self.deliver("death-recovery-handoff")
        self.assertEqual(receipt["opt_in"], "1", "no silent assisted recovery fallback")
        self.assertEqual(receipt["home"], "death-recovery-home")
        self.assertIn("save death-recovery-living", receipt["console"])
        self.assertIn("player.killactor", receipt["console"])
        self.assertEqual(launch.run_seconds, 90)

    def test_own_hit_delivers_no_automatic_recovery_opt_in(self):
        receipt, _ = self.deliver("player-own-hit-no-credit")
        self.assertIsNone(receipt["opt_in"])
        self.assertEqual(receipt["hit"], "8:player:500:0x0007EA25")

    def test_grenade_self_kill_delivers_no_automatic_recovery_opt_in(self):
        receipt, _ = self.deliver("g5-self-kill-1")
        self.assertIsNone(receipt["opt_in"])

    def test_dead_save_refusal_keeps_its_owned_home_without_recovery_opt_in(self):
        receipt, _ = self.deliver("save-refused-when-dead")
        self.assertIsNone(receipt["opt_in"])
        self.assertEqual(receipt["home"], "save-dead-home")
        self.assertIn("19:save dead-save", receipt["console"])


if __name__ == "__main__":
    unittest.main()
