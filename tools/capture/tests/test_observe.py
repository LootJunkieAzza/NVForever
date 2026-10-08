"""Tests for tools/capture/observe.py (the AI observer). No game or game data is used:
Editor IDs come from a small made-up index, plugin output is written by hand, and the
game runs are played by a fake capture_run."""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402
import observe as ob  # noqa: E402

# Made-up Editor IDs -> global FormIDs (FalloutNV.esm is index 0).
EDIDS = {"towncell": 0x0000AAAA, "saloon": 0x0000BBBB, "bob": 0x00001001, "ann": 0x00001002}
CTX = (cs.MASTERS, EDIDS)


def campaign(**session):
    s = {"name": "s1", "go": {"coc": "TownCell"}, "hour": 6, "hours": 2}
    s.update(session)
    return {"format": ob.CAMPAIGN_FORMAT, "name": "c1", "save": "nvftest",
            "defaults": {"every": 30, "track": ["Bob", "Ann"]}, "sessions": [s]}


def scripts_of(job):
    out = []
    for s in job["steps"]:
        for k in ("query", "script", "until", "remember"):
            if k in s:
                out.append(s[k])
        if "observe" in s:
            o = s["observe"]
            out += o["header"] + [o.get("until") or "", o.get("drive") or "", o.get("drive_end") or ""]
            for g in o["groups"]:
                out += [g["count"]] + g["probe"]
    return [x for x in out if x]


class ScriptRules(unittest.TestCase):
    """What the game's runtime compiler accepted (xnvse-capture.md section 5)."""

    def check(self, job):
        for text in scripts_of(job):
            head, _, body = text.partition("Begin Function { }")
            self.assertTrue(body, text)
            self.assertNotIn("0x", text)
            self.assertNotIn("string_var", text)
            for line in body.splitlines():
                self.assertFalse(line.strip().startswith(("ref ", "float ", "int ")), f"declaration inside the block: {line}")
            self.assertIn("SetFunctionValue", body, "every script returns a value")
            self.assertTrue(text.rstrip().endswith("End"))

    def test_diag_job(self):
        job = ob.diag_job()
        self.check(job)
        self.assertEqual(job["format"], cs.STEPS_FORMAT)
        fields = [s.get("field") for s in job["steps"] if s.get("label") == "field"]
        self.assertEqual(fields, [f[0] for p in ob.PARTS for f in p], "every field is checked on its own")
        self.assertEqual(sum("observe" in s for s in job["steps"]), 1)

    def test_committed_diag_job_is_current(self):
        with open(os.path.join(os.path.dirname(HERE), "jobs", "aa_observe_diag.steps.json"), encoding="utf-8") as f:
            self.assertEqual(json.load(f), ob.diag_job(), "regenerate with: observe.py diag -o jobs/aa_observe_diag.steps.json")

    def test_campaign_job(self):
        jobs = ob.campaign_jobs(campaign(walk={"to": "Ann", "within": 100}, timescale=60), EDIDS)
        (name, (session, job)), = jobs.items()
        self.assertEqual(name, "obs_c1_s1")
        self.check(job)
        kinds = [next(iter(s)) for s in job["steps"] if next(iter(s)) != "query"]
        self.assertEqual(kinds, ["script", "frames", "console", "until", "frames", "frames", "remember", "console",
                                 "remember", "observe", "script", "console", "frames"])
        o = [s["observe"] for s in job["steps"] if "observe" in s][0]
        self.assertEqual([g["name"] for g in o["groups"]], ["nearby", "tracked"])
        self.assertEqual(o["fields"]["tracked"], ["Bob", "Ann"])
        self.assertEqual(o["every"], 30)
        self.assertIn("%{t0} + 0.0833", o["until"])
        self.assertIn("player.GetHeadingAngle rF1", o["drive"])
        self.assertIn("BuildRef fM1 4098", o["drive"], "Ann = 0x1002")
        tracked = o["groups"][1]
        self.assertIn("BuildRef fM1 4097", tracked["probe"][0])
        self.assertIn("elseif %{i} == 1", tracked["probe"][0])
        self.assertEqual(tracked["count"].count("let fV := 2"), 1)
        self.assertNotIn("GetDistance", "".join(o["groups"][0]["probe"]), "GetDistance ends scripts for far targets")
        self.assertEqual(session["hours"], 2)

    def test_hour_and_bad_campaigns(self):
        self.assertIn("let fH := 6", ob.set_hour_script(6))
        for bad in (dict(hour=24), dict(hours=0), dict(every=0), dict(go={"teleport": "x"}), dict(color=1)):
            with self.assertRaises((ob.InputError, cs.InputError), msg=bad):
                ob.campaign_jobs(campaign(**bad), EDIDS)
        with self.assertRaises(cs.InputError):
            ob.campaign_jobs(campaign(), None)  # tracked Editor IDs need --data
        c = campaign()
        c["sessions"].append(dict(c["sessions"][0]))
        with self.assertRaises(ob.InputError):
            ob.campaign_jobs(c, EDIDS)

    def test_plugin_parses_generated_jobs(self):
        # The plugin's own parser is checked against the committed diag job by its Rust
        # tests; here only the step keys the plugin knows are used.
        known = {"query", "script", "console", "manual", "frames", "until", "remember", "observe"}
        for _, job in ob.campaign_jobs(campaign(), EDIDS).values():
            for s in job["steps"]:
                self.assertIn(next(iter(s)), known)


def output(samples, end=True):
    """Plugin output lines for one observation; samples: [(days, nearby, tracked)]."""
    lines = [{"kind": "start", "job": "j"},
             {"kind": "result", "label": cs.MODINDEX_LABEL, "field": "FalloutNV.esm", "value": 0},
             {"kind": "result", "label": cs.MODINDEX_LABEL, "field": "DeadMoney.esm", "value": 1},
             {"kind": "observing", "step": 3, "label": "s1", "fields": ob.fields_doc(["Bob"]), "every": 30,
              "groups": ["nearby", "tracked"]}]
    for n, (days, near, tracked) in enumerate(samples):
        header = [days, (days * 24) % 24, 30, 0.0, 0.0, 0.0, 0.0, "0000AAAA", 0]
        lines.append({"kind": "sample", "step": 3, "n": n, "header": header,
                      "groups": {"nearby": near, "tracked": tracked}, "errors": 0, "us": 100})
    if end:
        lines += [{"kind": "observed", "step": 3, "label": "s1", "samples": len(samples), "reason": "until"},
                  {"kind": "end", "steps": 4}]
    return [json.dumps(x) for x in lines]


def actor(fid, x=0.0, package="00002001", ptype=36, proc=45, combat=0, sitting=0):
    p0 = f"{fid}|00003001|0000AAAA|{x}|0|0|90|{package}|{ptype}|{proc}|00000000"
    p1 = f"{combat}|00000000|1|0|0|{sitting}|0|0|0|0|0|0|0|0|0|0|100|1|0|0"
    return [p0, p1]


class Timeline(unittest.TestCase):
    def test_segments_events_and_merging(self):
        bob, ann, spawned = "00001001", "00001002", "FF000801"
        samples = [
            (1.0, [actor(bob), actor(ann)], [actor(bob)]),
            (1.0 + 1 / 24, [actor(bob, x=30), actor(ann, sitting=3, proc=48)], [actor(bob, x=30)]),
            (1.0 + 2 / 24, [actor(ann, sitting=3, proc=48), actor(spawned, combat=1)], [actor(bob, x=500)]),
        ]
        names = {0x1001: "BobREF", 0x2001: "BobSandbox", 0xAAAA: "TownCell"}
        tl = ob.timeline([("out.jsonl", output(samples))], names)
        self.assertEqual(tl["format"], ob.TIMELINE_FORMAT)
        self.assertEqual(tl["problems"], [])
        b = tl["actors"]["0x00001001"]
        self.assertEqual(b["edid"], "BobREF")
        self.assertEqual(b["groups"], ["nearby", "tracked"])
        self.assertEqual(b["samples"], 3, "one record per sample even when in both groups")
        self.assertEqual(len(b["segments"]), 2, "leaving the loaded area (seen) starts a new segment")
        s0 = b["segments"][0]
        self.assertAlmostEqual(s0["from"], 24.0)
        self.assertAlmostEqual(s0["to"], 25.0)
        self.assertEqual(s0["state"]["package_edid"], "BobSandbox")
        self.assertEqual(s0["state"]["package_type_name"], "SANDBOX")
        self.assertEqual(s0["state"]["procedure_name"], "SANDBOX")
        self.assertEqual(s0["path"], 30.0)
        self.assertEqual(b["segments"][1]["state"]["seen"], None)
        self.assertAlmostEqual(b["summary"]["hours_by_package"]["BobSandbox"], 1.0)
        a = tl["actors"]["0x00001002"]
        self.assertEqual([s["state"]["sitting"] for s in a["segments"]], [0.0, 3.0])
        ev = [(e["actor"], e["field"], e["to"]) for e in tl["events"]]
        self.assertIn(("0x00001002", "sitting", 3.0), ev)
        self.assertIn(("0x00001002", "procedure", 48.0), ev)
        self.assertIn("0xFF000801", tl["actors"], "actors created at run time keep their FF FormID")
        self.assertEqual(tl["actors"]["0xFF000801"]["segments"][0]["state"]["in_combat"], 1.0)
        self.assertIn("BobREF", ob.text_summary(tl))

    def test_crashed_output_is_read_up_to_its_last_line(self):
        lines = output([(1.0, [actor("00001001")], []), (1.1, [actor("00001001")], [])], end=False)
        lines.append('{"kind": "sample", "step": 3, "n": 2, "head')
        tl = ob.timeline([("crash.jsonl", lines)])
        self.assertEqual(tl["sessions"][0]["end"], None)
        self.assertEqual(tl["actors"]["0x00001001"]["samples"], 2)
        self.assertEqual(ob.timeline([("x", ['{"kind": "start"}'])])["problems"][0], "x: no mod indices (not an nvf_capture output?)")


class Supervise(unittest.TestCase):
    """The supervisor over a fake capture_run.run."""

    class Game:
        def __init__(self):
            self.alive, self.kills = False, 0

        def running(self):
            return self.alive

        def kill(self):
            self.kills += 1
            self.alive = False

    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = self.tmp.name
        os.makedirs(os.path.join(self.dir, "jobs"))
        self.jobs = ob.campaign_jobs({"format": ob.CAMPAIGN_FORMAT, "name": "c", "sessions": [
            {"name": "a", "hours": 1}, {"name": "b", "hours": 1}]})

    def tearDown(self):
        self.tmp.cleanup()

    def fake_run(self, plan):
        """plan: per round, {job: status}; jobs missing from the dict are 'missing'."""
        rounds = iter(plan)
        calls = []

        def run(jobs, names, capture_dir, game, save, timeout, **kw):
            calls.append((sorted(jobs), timeout, kw.get("kill_on_timeout")))
            for n, (path, _) in jobs.items():
                with open(path, encoding="utf-8") as src, open(os.path.join(capture_dir, "jobs", f"{n}.steps.json"), "w") as dst:
                    dst.write(src.read())
            r = next(rounds)
            problems = r.pop("_problems", [])
            return {"problems": problems, "jobs": {n: {"status": r.get(n, "missing"), "output": f"{n}.out" if n in r else None,
                                                       "errors": 0} for n in names}}
        return run, calls

    def supervise(self, plan, retries=1):
        run, calls = self.fake_run(plan)
        rep = ob.supervise(self.jobs, self.dir, self.Game(), "nvftest", retries, os.path.join(self.dir, "stage"), run=run)
        return rep, calls

    def test_a_crash_is_retried_and_the_rest_continue(self):
        rep, calls = self.supervise([{"obs_c_a": "running"}, {"obs_c_a": "ended", "obs_c_b": "ended"}])
        self.assertEqual([c[0] for c in calls], [["obs_c_a", "obs_c_b"], ["obs_c_a", "obs_c_b"]])
        self.assertTrue(all(c[2] for c in calls), "hung games are closed")
        self.assertEqual(calls[0][1], int(2 * 3600 / 30 * 1.5) + 2 * ob.SESSION_MARGIN)
        s = rep["sessions"]
        self.assertEqual((s["obs_c_a"]["status"], s["obs_c_a"]["cut_off"]), ("ended", 1))
        self.assertEqual(s["obs_c_a"]["outputs"], ["obs_c_a.out", "obs_c_a.out"])
        self.assertEqual(s["obs_c_b"]["status"], "ended")
        self.assertEqual(rep["problems"], [])

    def test_a_session_that_keeps_crashing_is_given_up(self):
        rep, calls = self.supervise([{"obs_c_a": "running"}, {"obs_c_a": "running"}, {"obs_c_b": "ended"}], retries=1)
        self.assertEqual(calls[2][0], ["obs_c_b"])
        self.assertFalse(os.path.exists(os.path.join(self.dir, "jobs", "obs_c_a.steps.json")),
                         "a given-up job is taken out of jobs/ so the plugin does not run it again")
        self.assertEqual(rep["sessions"]["obs_c_a"]["status"], "given up")
        self.assertEqual(rep["sessions"]["obs_c_b"]["status"], "ended")

    def test_fatal_problems_and_no_progress_stop(self):
        rep, calls = self.supervise([{"_problems": ["plugin: skipping obs_c_a.steps.json: unknown step"]}])
        self.assertEqual(len(calls), 1)
        self.assertTrue(rep["problems"][0].startswith("stopped: plugin:"))
        rep, calls = self.supervise([{}])
        self.assertEqual(len(calls), 1)
        self.assertIn("no progress", rep["problems"][0])

    def test_cli_rejects_bad_input(self):
        path = os.path.join(self.dir, "c.json")
        with open(path, "w") as f:
            json.dump({"format": "x"}, f)
        err = io.StringIO()
        with redirect_stderr(err), redirect_stdout(io.StringIO()):
            self.assertEqual(ob.main(["jobs", path]), 1)
        self.assertIn("not a fnv.observe.campaign/1", err.getvalue())


if __name__ == "__main__":
    unittest.main()
