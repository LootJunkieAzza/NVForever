"""Tests for tools/capture/capture_quests.py (Layer 5 quest captures). Synthetic decoded
records and plugin output only; no game data."""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_quests as cq  # noqa: E402
import capture_steps as cs  # noqa: E402


def sub(sig, value):
    return {"sig": sig, "status": "ok", "value": value}


QUEST = {"formid": "0x01000ABC", "type": "QUST", "subrecords": [
    sub("EDID", "TestQuest"), sub("SCRI", "0x01000ABD"), sub("FULL", "Test"),
    sub("DATA", {"flags": {"value": 9, "set": ["Start game enabled", "Allow repeated stages"]}, "priority": 10,
                 "unused": [0, 0], "quest_delay": 0.5}),
    sub("INDX", 10), sub("QSDT", {"value": 0, "set": []}), sub("CTDA", {}), sub("CNAM", "log"),
    sub("SCHR", {"compiled_size": 16}), sub("SCDA", "00"), sub("SCTX", "set TestQuest.n to 1"),
    sub("QSDT", {"value": 1, "set": ["Complete Quest"]}), sub("SCHR", {"compiled_size": 0}),
    sub("INDX", 20),
    sub("QOBJ", 5), sub("NNAM", "Do it."), sub("QSTA", {"target": "0x00000014"}), sub("CTDA", {}), sub("CTDA", {}),
    sub("QOBJ", 7), sub("NNAM", "Then this."),
]}

SCRIPT = {"formid": "0x01000ABD", "type": "SCPT", "subrecords": [
    sub("EDID", "TestQuestScript"),
    sub("SLSD", {"index": 2, "type": "Integer"}), sub("SCVR", "nCount"),
    sub("SLSD", {"index": 1, "type": "FloatOrRef"}), sub("SCVR", "fTimer"),
]}


def defs():
    return cq.defs_document([cq.quest_defs(QUEST, SCRIPT)], "Test.esm")


def output_lines(results, end=True):
    lines = [{"kind": "start", "job": "j"},
             {"kind": "result", "step": 0, "label": cs.MODINDEX_LABEL, "field": "FalloutNV.esm", "value": 0}]
    lines += [{"kind": "result", "step": i + 1, **r} for i, r in enumerate(results)]
    if end:
        lines.append({"kind": "end", "steps": len(lines)})
    return [json.dumps(x) for x in lines]


class Defs(unittest.TestCase):
    def test_stages_entries_objectives_variables(self):
        q = cq.quest_defs(QUEST, SCRIPT)
        self.assertEqual(q["edid"], "TestQuest")
        self.assertEqual(q["flags"], ["Start game enabled", "Allow repeated stages"])
        self.assertEqual(q["flag_bits"], 9)
        self.assertEqual(q["delay"], 0.5)
        self.assertEqual([s["index"] for s in q["stages"]], [10, 20])
        e0, e1 = q["stages"][0]["entries"]
        self.assertEqual((e0["conditions"], e0["log"], e0["compiled_size"]), (1, True, 16))
        self.assertEqual((e1["flags"], e1["compiled_size"]), (["Complete Quest"], 0))
        self.assertEqual(q["stages"][1]["entries"], [])
        self.assertEqual([(o["index"], o["targets"], o["conditions"]) for o in q["objectives"]], [(5, 1, 2), (7, 0, 0)])
        self.assertEqual(q["variables"], [{"index": 2, "name": "nCount", "type": "Integer"},
                                          {"index": 1, "name": "fTimer", "type": "FloatOrRef"}])
        self.assertEqual(q["script_edid"], "TestQuestScript")
        self.assertNotIn("Do it.", json.dumps(q))  # no texts kept

    def test_quest_without_script_or_edid(self):
        q = cq.quest_defs({"formid": "0x1", "subrecords": [sub("EDID", "Bare")]})
        self.assertEqual((q["variables"], q["stages"]), ([], []))
        with self.assertRaises(cq.InputError):
            cq.quest_defs({"formid": "0x2", "subrecords": []})


class StateJob(unittest.TestCase):
    def test_fields_and_expressions(self):
        q = defs()["quests"][0]
        self.assertEqual(cq.quest_fields(q), [
            "running", "completed", "stage", "done.10", "done.20",
            "objective.5.displayed", "objective.5.completed", "objective.7.displayed", "objective.7.completed",
            "var.nCount", "var.fTimer"])
        self.assertEqual(cq.field_expr("rQ", "done.10"), "GetStageDone rQ 10")
        self.assertEqual(cq.field_expr("rQ", "objective.5.completed"), "GetObjectiveCompleted rQ 5")
        self.assertEqual(cq.field_expr("rQ", "var.fTimer"), 'GetVariable "fTimer" rQ')
        self.assertEqual(cq.field_expr("rQ", "running"), "GetQuestRunning rQ")
        for bad in ("done", "objective.5.shown", "var.a.b", "nonsense", "var.0x12"):
            with self.assertRaises(cq.InputError):
                cq.field_expr("rQ", bad)

    def test_job_has_mod_indices_then_one_query_per_field(self):
        j = cq.state_job(defs(), "q5_state", "Any save.")
        self.assertEqual(j["format"], cs.STEPS_FORMAT)
        head = [s for s in j["steps"] if s.get("label") == cs.MODINDEX_LABEL]
        self.assertEqual(len(head), len(cs.MASTERS))
        body = j["steps"][len(head):]
        self.assertEqual([s["field"] for s in body], cq.quest_fields(defs()["quests"][0]))
        self.assertIn("let fV := GetStage rF1", body[2]["query"])
        self.assertIn("let rF1 := BuildRef fM1 2748", body[2]["query"])  # 0x01000ABC: DeadMoney.esm, local 0xABC
        self.assertTrue(all(s["label"] == "TestQuest" for s in body))


class State(unittest.TestCase):
    def results(self, **over):
        vals = {"running": 1.0, "completed": 0.0, "stage": 20.0, "done.10": 1.0, "done.20": 0.0,
                "objective.5.displayed": 1.0, "objective.5.completed": 1.0, "objective.7.displayed": 0.0,
                "objective.7.completed": 0.0, "var.nCount": 3.0, "var.fTimer": 0.25}
        vals.update(over)
        return [{"label": "TestQuest", "field": f, "value": v} for f, v in vals.items()]

    def test_state_document(self):
        mods, results = cs.read_output(output_lines(self.results()))
        doc = cq.state_document(defs(), mods, results)
        self.assertEqual(doc["format"], cq.STATE_FORMAT)
        q = doc["quests"][0]
        self.assertEqual((q["running"], q["completed"], q["stage"]), (True, False, 20))
        self.assertEqual((q["stages_done"], q["stages_not_done"]), ([10], [20]))
        self.assertEqual(q["objectives"], [{"index": 5, "displayed": True, "completed": True},
                                           {"index": 7, "displayed": False, "completed": False}])
        self.assertEqual(q["variables"], {"nCount": 3.0, "fTimer": 0.25})
        self.assertNotIn("errors", q)

    def test_errors_and_missing_fields_are_kept(self):
        rs = [r for r in self.results() if r["field"] != "var.fTimer"]
        rs[0] = {"label": "TestQuest", "field": "running", "error": "compile failed"}
        mods, results = cs.read_output(output_lines(rs))
        q = cq.state_document(defs(), mods, results)["quests"][0]
        self.assertEqual(q["errors"], {"running": "compile failed", "var.fTimer": "not recorded"})
        self.assertNotIn("running", q)

    def test_diff(self):
        mods, results = cs.read_output(output_lines(self.results()))
        a = cq.state_document(defs(), mods, results)
        mods, results = cs.read_output(output_lines(self.results(**{"stage": 10.0, "var.nCount": 4.0, "done.20": 1.0})))
        b = cq.state_document(defs(), mods, results)
        d = cq.diff_states(a, b)
        self.assertIn(("TestQuest", "stage", 20, 10), d)
        self.assertIn(("TestQuest", "stages_done", [10], [10, 20]), d)
        self.assertIn(("TestQuest", "var nCount", 3.0, 4.0), d)
        self.assertEqual(cq.diff_states(a, a), [])


EDIDS = {"testquest": 0x00104C1C, "q": 0x00104C1D}

ACTIONS = {
    "format": cq.ACTIONS_FORMAT, "title": "t",
    "given": [{"kind": "quest", "quest": "TestQuest", "running": False, "stage": 0, "not_done": [10],
               "objectives": [{"index": 5, "displayed": False}], "vars": {"nCount": 0}},
              {"kind": "global", "name": "ScroungerOdds", "value": 100}],
    "actions": [
        {"op": "set_stage", "quest": "TestQuest", "stage": 20},
        {"op": "get_quest", "quest": "TestQuest", "label": "after 20", "fields": ["stage", "running", "done.20"]},
        {"op": "set_stage_read", "quest": "TestQuest", "stage": 10, "field": "var.nCount", "label": "same script"},
        {"op": "set_quest_var", "quest": "TestQuest", "var": "fTimer", "value": 1e-08},
        {"op": "set_objective", "quest": "TestQuest", "index": 5, "displayed": False},
        {"op": "start_quest", "quest": "TestQuest"},
        {"op": "sample_var", "quest": "TestQuest", "var": "fTimer", "frames": 4, "runs": 2, "label": "timer"},
        {"op": "sample_var", "quest": "TestQuest", "var": "fTimer", "frames": 3, "label": "still"},
        {"op": "stop_quest", "quest": "TestQuest"},
        {"op": "set_global", "name": "ScroungerOdds", "value": 0},
        {"op": "get_global", "name": "ScroungerOdds", "label": "odds"},
        {"op": "wait_frames", "frames": 5},
        {"op": "set_local", "type": "short", "value": 40000, "label": "short"},
    ]}


class Job(unittest.TestCase):
    def test_actions_to_steps(self):
        j = cq.actions_job(ACTIONS, "q5_t", "nvftest", EDIDS)
        steps = j["steps"][len(cs.MASTERS):]
        given = [s for s in steps if s.get("label", "").startswith(cq.GIVEN_PREFIX)]
        self.assertEqual([s["field"] for s in given], ["running", "stage", "done.10", "objective.5.displayed",
                                                       "var.nCount", "value"])
        scripts = [s["script"] for s in steps if "script" in s]
        self.assertIn("SetStage rF1 20", scripts[0])
        self.assertIn('let fM1 := GetModIndex "FalloutNV.esm"', scripts[0])
        self.assertTrue(any('SetVariable "fTimer" 0.00000001 rF1' in s for s in scripts))
        self.assertTrue(any("SetObjectiveDisplayed rF1 5 0" in s for s in scripts))
        self.assertTrue(any("StartQuest rF1" in s for s in scripts))
        self.assertTrue(any("StopQuest rF1" in s for s in scripts))
        self.assertEqual([s["console"] for s in steps if "console" in s], ["set ScroungerOdds to 0"])
        local = [s for s in steps if s.get("label") == "short"][0]
        self.assertIn("short xV" + chr(10), local["query"])
        self.assertIn("set xV to 40000" + chr(10) + "SetFunctionValue xV", local["query"])
        same = [s for s in steps if s.get("label") == "same script"][0]
        self.assertLess(same["query"].index("SetStage rF1 10"), same["query"].index('let fV := GetVariable "nCount" rF1'))
        samples = [s for s in steps if s.get("label") == "sample:timer"]
        self.assertEqual([s["field"] for s in samples], ["0", "1", "2", "3"])
        self.assertIn({"frames": 5}, steps)

    def test_bad_actions(self):
        for a in ({"op": "teleport"}, {"op": "get_quest", "quest": "Q", "label": "x"},
                  {"op": "get_quest", "quest": "Q", "fields": ["stage"]},
                  {"op": "set_objective", "quest": "Q", "index": 1},
                  {"op": "sample_var", "quest": "Q", "var": "v", "frames": 0, "label": "x"},
                  {"op": "set_stage", "quest": "Unknown", "stage": 1},
                  {"op": "set_local", "type": "double", "value": 1, "label": "x"}):
            with self.assertRaises((cq.InputError, KeyError)):
                cq.actions_job({"format": cq.ACTIONS_FORMAT, "actions": [a]}, "x", "y", EDIDS)
        with self.assertRaises(cq.InputError):
            cq.actions_job({"format": "fnv.sim.actions/1"}, "x", "y")

    def test_literal(self):
        self.assertEqual(cq.literal(40000), "40000")
        self.assertEqual(cq.literal(-2.5), "-2.5")
        self.assertEqual(cq.literal(1e-08), "0.00000001")
        self.assertEqual(cq.literal(0.3333333333333333), "0.3333333333333333")
        with self.assertRaises(cq.InputError):
            cq.literal(float("nan"))


class Golden(unittest.TestCase):
    def capture(self, given_running=0.0):
        rs = [{"label": "__given__:0", "field": "running", "value": given_running},
              {"label": "__given__:0", "field": "stage", "value": 0.0},
              {"label": "__given__:0", "field": "done.10", "value": 0.0},
              {"label": "__given__:0", "field": "objective.5.displayed", "value": 0.0},
              {"label": "__given__:0", "field": "var.nCount", "value": 0.0},
              {"label": "__given__:1", "field": "value", "value": 100.0},
              {"label": "after 20", "field": "stage", "value": 20.0},
              {"label": "after 20", "field": "running", "value": 1.0},
              {"label": "after 20", "field": "done.20", "value": 1.0},
              {"label": "same script", "field": "var.nCount", "value": 1.0}]
        rs += [{"label": "sample:timer", "field": str(k), "value": v} for k, v in enumerate([100.0, 99.5, 99.5, 99.0])]
        rs += [{"label": "sample:still", "field": str(k), "value": 7.0} for k in range(3)]
        rs += [{"label": "odds", "field": "value", "value": 0.0}]
        return cs.read_output(output_lines(rs))

    def test_golden(self):
        mods, results = self.capture()
        doc, problems = cq.golden_document(ACTIONS, mods, results)
        self.assertEqual(problems, [])
        self.assertEqual(doc["format"], cq.TRACE_FORMAT)
        obs = {o["label"]: o for o in doc["observations"]}
        self.assertEqual(obs["after 20"], {"label": "after 20", "step": 1, "stage": 20, "running": True, "done.20": True})
        self.assertEqual(obs["same script"]["var.nCount"], 1.0)
        self.assertEqual(obs["timer"]["drops"], [0.5, 0.5])
        self.assertEqual(obs["still"], {"label": "still", "step": 7, "changes": 0})
        self.assertEqual(obs["odds"]["value"], 0.0)
        self.assertNotIn(cq.GIVEN_PREFIX, json.dumps(doc["observations"]))
        diag = {d["label"]: d for d in doc["diagnostics"]}
        self.assertEqual(diag["timer"]["loops_between"], [1, 2])

    def test_given_mismatch_reported(self):
        mods, results = self.capture(given_running=1.0)
        _, problems = cq.golden_document(ACTIONS, mods, results)
        self.assertEqual(problems, ["given 0 running: expected False, the save has True"])

    def test_skip(self):
        values = [(0, 10.0), (1, 9.0), (2, 9.0), (3, 8.5), (4, 8.0)]
        obs, _ = cq.sample_observation({"label": "x", "runs": 2, "skip": 1}, values)
        self.assertEqual(obs["drops"], [0.5, 0.5])
        obs, _ = cq.sample_observation({"label": "x", "runs": 3, "skip": 1}, values)
        self.assertIn("error", obs)

    def test_too_few_runs(self):
        obs, diag = cq.sample_observation({"label": "x", "runs": 3}, [(0, 5.0), (1, 4.0)])
        self.assertIn("error", obs)
        self.assertEqual(diag["all_drops"], [1.0])

    def test_cli_golden_refuses_mismatch(self):
        with tempfile.TemporaryDirectory() as d:
            mods_lines = output_lines([{"label": "__given__:0", "field": "value", "value": 5.0}])
            out = os.path.join(d, "o.jsonl")
            with open(out, "w") as f:
                f.write("\n".join(mods_lines) + "\n")
            acts = os.path.join(d, "a.json")
            with open(acts, "w") as f:
                json.dump({**ACTIONS, "given": ACTIONS["given"][1:], "actions": []}, f)
            err = io.StringIO()
            with redirect_stderr(err):
                self.assertEqual(cq.main(["golden", out, "--actions", acts, "-o", os.path.join(d, "g.json")]), 1)
            self.assertIn("expected 100", err.getvalue())
            with redirect_stderr(io.StringIO()), redirect_stdout(io.StringIO()):
                self.assertEqual(cq.main(["golden", out, "--actions", acts, "--allow-given-mismatch",
                                          "-o", os.path.join(d, "g.json")]), 0)
            self.assertTrue(os.path.exists(os.path.join(d, "g.json")))


class Inputs(unittest.TestCase):
    """The committed scenario inputs convert to jobs (no game data needed: quests are
    named by Editor ID, forms by FormID)."""

    def test_layer5_inputs(self):
        root = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer5")
        if not os.path.isdir(root):
            self.skipTest("no layer5 inputs")
        names = sorted(n for n in os.listdir(root) if n.endswith(".actions.json"))
        self.assertTrue(names)
        for n in names:
            with open(os.path.join(root, n), encoding="utf-8") as f:
                doc = json.load(f)
            quests = {x["quest"] for x in doc.get("given", []) + doc["actions"] if "quest" in x}
            j = cq.actions_job(doc, n.split(".")[0], "save", {q.lower(): 0x00100000 + k for k, q in enumerate(sorted(quests))})
            self.assertTrue(j["steps"], n)
            labels = [a["label"] for a in doc["actions"] if a.get("label")]
            self.assertEqual(len(labels), len(set(labels)), f"{n}: duplicate labels")


if __name__ == "__main__":
    unittest.main()
