"""Every hand-written job in tools/capture/jobs loads in nvf_capture version 7, the plugin
the owner has installed: its step shapes (tools/capture/nvf_capture/src/runner.rs before
3bfbc9fe, "nvf_capture 8: short scripts") are serde structs with deny_unknown_fields
chosen untagged, so a step with a missing or extra key makes the whole job fail to load
("data did not match any variant of untagged enum Step"). Version 8's `forms` and short
scripts are refused too. No game or game data."""
import json
import os
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
JOBS = os.path.join(os.path.dirname(HERE), "jobs")

# Each step kind: (keys it must have, keys it may have) [observed: runner.rs at 3bfbc9fe~1].
STEPS = {
    "query": ({"query", "label", "field"}, set()),
    "script": ({"script"}, set()),
    "console": ({"console"}, set()),
    "manual": ({"manual"}, set()),
    "frames": ({"frames"}, set()),
    "until": ({"until", "max_frames"}, set()),
    "remember": ({"remember", "as"}, set()),
    "observe": ({"observe"}, set()),
    "screenshot": ({"screenshot"}, set()),
}
OBSERVE = ({"label", "every", "max_samples"},
           {"fields", "header", "groups", "until", "drive", "drive_frames", "drive_end", "hooks"})
JOB = ({"format", "name", "steps"}, {"title", "start"})
# Jobs written for plugin version 9 (installed 2026-10-19, ai-observer.md section 12): their
# observe steps may also carry xNVSE game events and triggered screenshots.
V9_JOBS = {"od_13_assault_faction.steps.json", "od_22_dlg_random_end.steps.json"}
OBSERVE_V9 = (OBSERVE[0], OBSERVE[1] | {"events", "shots"})


def shape_errors(keys, need, may):
    keys = set(keys)
    out = []
    if need - keys:
        out.append(f"missing {sorted(need - keys)}")
    if keys - need - may:
        out.append(f"unknown {sorted(keys - need - may)}")
    return out


def step_errors(step, observe=OBSERVE):
    if not isinstance(step, dict):
        return ["not an object"]
    kinds = [k for k in STEPS if k in step]
    if not kinds:
        return [f"no step key in {sorted(step)}"]
    # serde tries the variants in order; the first key present names the one meant.
    need, may = STEPS[kinds[0]]
    errs = shape_errors(step, need, may)
    for k in need & set(step):
        v = step[k]
        if k in ("frames", "max_frames"):
            if not isinstance(v, int) or isinstance(v, bool) or v < 0:
                errs.append(f"{k} is not a whole number")
        elif k == "observe":
            errs += [f"observe {e}" for e in shape_errors(v, *observe)] if isinstance(v, dict) else ["observe is not an object"]
        elif not isinstance(v, str):
            errs.append(f"{k} is not a string")
    return errs


def short_script(text):
    """A version-8 short script: a text without a Function block (v7 runs only full
    function scripts)."""
    return "Begin Function" not in text


class JobsLoadInVersion7(unittest.TestCase):
    def test_every_job_has_version_7_step_shapes(self):
        bad = []
        names = sorted(f for f in os.listdir(JOBS) if f.endswith(".steps.json"))
        self.assertTrue(names)
        for f in names:
            with open(os.path.join(JOBS, f), encoding="utf-8") as fh:
                job = json.load(fh)
            for e in shape_errors(job, *JOB):
                bad.append(f"{f}: job {e}")
            for i, s in enumerate(job.get("steps", [])):
                for e in step_errors(s, OBSERVE_V9 if f in V9_JOBS else OBSERVE):
                    bad.append(f"{f}: step {i}: {e}")
                for k in ("query", "script", "until", "remember"):
                    if isinstance(s, dict) and isinstance(s.get(k), str) and short_script(s[k]):
                        bad.append(f"{f}: step {i}: {k} is a short script (version 8)")
        self.assertEqual(bad, [], "\n".join(bad[:40]))

    def test_the_checker_refuses_what_version_7_refused(self):
        self.assertEqual(step_errors({"query": "x", "field": "f"}), ["missing ['label']"])
        self.assertEqual(step_errors({"script": "x", "label": "l"}), ["unknown ['label']"])
        self.assertEqual(shape_errors({"format": 1, "name": 1, "steps": 1, "forms": 1}, *JOB), ["unknown ['forms']"])
        self.assertEqual(step_errors({"frames": 2}), [])
        self.assertTrue(short_script("player.GetAV Strength"))


if __name__ == "__main__":
    unittest.main()
