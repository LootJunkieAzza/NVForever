"""BC5 field scenario setup and outcome boundaries; no game or GPU is launched."""
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

HERE = Path(__file__).resolve().parent.parent
sys.path.insert(0, str(HERE))

from smoke.plan import plan  # noqa: E402
from smoke.runner import Env, judge, run_launch  # noqa: E402
from smoke.scenarios import load  # noqa: E402


class Bc5FieldSetupTests(unittest.TestCase):
    def scenario(self):
        suite = load(HERE / "scenarios.toml")
        return suite, suite.by_name()["bc5-signed-normal-owned-field"]

    def test_actual_game_launch_supplies_explicit_aid_and_owned_relative_home(self):
        suite, scenario = self.scenario()
        launch, = plan(suite, [scenario])
        with tempfile.TemporaryDirectory() as d:
            out = Path(d)
            child = out / "record.py"
            child.write_text(
                "import json, os\nfrom pathlib import Path\n"
                "print(json.dumps({'aid': os.environ.get('NVF_PLAY_TEST_BC5_FIELD'), "
                "'home': str(Path(os.environ['NVFOREVER_HOME']).resolve()), 'cwd': os.getcwd()}))\n",
                encoding="utf-8")
            if os.name == "nt":
                exe = out / "record.cmd"
                exe.write_text(f'@"{sys.executable}" "{child}"\n', encoding="utf-8")
            else:
                exe = out / "record.sh"
                exe.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}"\n', encoding="utf-8")
                exe.chmod(0o755)
            code, error, _ = run_launch(launch, Env(repo=HERE.parent.parent, out=out, exe=exe, data="unused-authored-bootstrap-placeholder", fnvtool=None), None)
            self.assertEqual((code, error), (0, ""))
            actual = json.loads((out / f"{launch.name}.log").read_text(encoding="utf-8"))
            self.assertEqual(actual["aid"], "1")
            self.assertEqual(Path(actual["home"]), out / "bc5-field-home")
            self.assertEqual(Path(actual["cwd"]), out)
            # The recorder only observes the actual process environment. It does
            # not claim that the Rust aid initialized or any frame was rendered.
            self.assertFalse((out / "bc5-field-home").exists())

    def test_success_needs_both_readbacks_and_rejects_shader_errors(self):
        _, scenario = self.scenario()
        markers = [
            "bc5 field: actual NIF material source, 3 authored panels, 4 quadrants",
            "bc5 field: first rendered readback PASS",
            "bc5 field: second rendered readback PASS",
            "bc5 field: rendered comparison PASS",
            "run finished",
        ]
        self.assertEqual(judge(scenario, markers, 0).status, "PASS")
        for i in range(4):
            self.assertEqual(judge(scenario, markers[:i] + markers[i + 1:], 0).status, "FAIL")
        self.assertEqual(judge(scenario, markers + ["wgpu error: validation error"], 0).status, "FAIL")
        self.assertEqual(judge(scenario, markers + ["failed to create shader module: malformed WGSL"], 0).status, "FAIL")
        self.assertEqual(judge(scenario, markers + ["bc5 field: FAIL readback 2"], 0).status, "FAIL")
        self.assertEqual(judge(scenario, markers, 1).status, "FAIL")
        self.assertEqual(judge(scenario, markers[:1] + [markers[2], markers[1]] + markers[3:], 0).status, "FAIL")


def repeated_rust_initialization(exe: Path, out: Path):
    """Called by the Rust parent test; actual runner/Popen, no game or GPU."""
    suite, scenario = Bc5FieldSetupTests().scenario()
    launch, = plan(suite, [scenario])
    child = out / "rust-child.py"
    child.write_text(
        "import subprocess, sys\n"
        f"result = subprocess.run([{str(exe)!r}, '--exact', "
        "'bc5_field::tests::registered_launcher_child_initializes_actual_field', "
        "'--ignored', '--nocapture'], timeout=20, check=False)\n"
        "sys.exit(result.returncode)\n", encoding="utf-8")
    if os.name == "nt":
        wrapper = out / "rust-child.cmd"
        wrapper.write_text(f'@"{sys.executable}" "{child}"\n', encoding="utf-8")
    else:
        wrapper = out / "rust-child.sh"
        wrapper.write_text(f'#!/bin/sh\nexec "{sys.executable}" "{child}"\n', encoding="utf-8")
        wrapper.chmod(0o755)
    env = Env(repo=HERE.parent.parent, out=out, exe=wrapper,
              data="unused-authored-bootstrap-placeholder", fnvtool=None)
    import re
    records = []
    outputs = []
    for ordinal in (1, 2):
        code, error, _ = run_launch(launch, env, None)
        raw = (out / f"{launch.name}.log").read_text(encoding="utf-8")
        (out / f"launch-{ordinal}.log").write_text(raw, encoding="utf-8")
        print(f"registered launch {ordinal}: {code=} {error=}\n{raw}", flush=True)
        if (code, error) != (0, ""):
            raise AssertionError(f"actual Rust initialization {ordinal}: {code=} {error=}\n{raw}")
        if len(re.findall(r"test result: ok\. 1 passed; 0 failed; 0 ignored; 0 measured; \d+ filtered out;", raw)) != 1:
            raise AssertionError(f"Rust child did not execute its exact ignored entrypoint: {raw}")
        lines = [line.removeprefix("BC5_FIELD_CHILD_OUTPUT=") for line in raw.splitlines()
                 if line.startswith("BC5_FIELD_CHILD_OUTPUT=")]
        if len(lines) != 1:
            raise AssertionError(f"missing/duplicate actual child output: {raw}")
        output = Path(lines[0])
        if not output.is_relative_to(out / "bc5-field-home" / "bc5-field-output"):
            raise AssertionError(f"field escaped the delivered home: {output}")
        if judge(scenario, raw.splitlines(), code).status != "FAIL":
            raise AssertionError("CPU initialization must not certify rendered success")
        files = {str(p.relative_to(output)): p.read_bytes()
                 for p in output.rglob("*") if p.is_file()}
        if len(files) != 6 or any(p.startswith("readback-") for p in files):
            raise AssertionError(f"wrong initialization-only evidence: {files.keys()}")
        outputs.append(output)
        records.append(files)
    if outputs[0] == outputs[1]:
        raise AssertionError("two launches reused one evidence recipient")
    for output, files in zip(outputs, records):
        if {str(p.relative_to(output)): p.read_bytes() for p in output.rglob("*") if p.is_file()} != files:
            raise AssertionError("a later launch replaced earlier evidence")
    if records[0] != records[1]:
        raise AssertionError("authored inputs differed across the repeated launches")
    print("two actual registered launches initialized distinct field recipients; no GPU/readback credit")


if __name__ == "__main__":
    if len(sys.argv) == 4 and sys.argv[1] == "--rust-child":
        repeated_rust_initialization(Path(sys.argv[2]), Path(sys.argv[3]))
    else:
        unittest.main()
