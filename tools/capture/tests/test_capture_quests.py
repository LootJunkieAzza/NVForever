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


ROUND2 = {
    "format": cq.ACTIONS_FORMAT, "title": "round 2",
    "given": [{"kind": "ref", "ref": "0x00104C0F", "dead": False, "vars": {"PlayerLeft": 1}}],
    "actions": [
        {"op": "set_ref_var", "ref": "0x00104C0F", "var": "PlayerLeft", "value": 0},
        {"op": "get_ref", "ref": "0x00104C0F", "fields": ["var.PlayerLeft", "dead", "disabled"], "label": "doc"},
        {"op": "activate_read", "target": "0x00103E61", "run_on_activate": True,
         "read": {"ref": "0x00104C0F", "field": "var.PlayerLeft"}, "label": "door"},
        {"op": "activate_read", "target": "0x00103E61", "read": {"quest": "TestQuest", "field": "stage"},
         "label": "door plain"},
        {"op": "activate", "target": "0x0012319B", "actor": "player", "run_on_activate": True, "label": "hostage"},
        {"op": "activate", "target": "0x0012319B", "actor": "player", "label": "hostage plain"},
        {"op": "kill", "target": "0x0012319B"},
        {"op": "evaluate_package", "of": "0x00104E85"},
        {"op": "is_current_package", "of": "0x00104E85", "package": "0x0010A21C", "label": "stays"},
        {"op": "add_item_read", "to": "player", "item": "0x0016665D", "count": 1, "read": ["0x0016665D", "0x00140AB2"],
         "label": "added"},
        {"op": "sample_items", "of": "player", "items": ["0x0016665D", "0x00140AB2"], "frames": 3, "label": "items"},
        {"op": "menu_gap", "via": "0x00103E37", "frames": 5, "quest": "TestQuest", "var": "fTimer", "after": 3,
         "label": "gap"},
        {"op": "sample_ref_var", "ref": "0x00104C10", "var": "timer", "frames": 2, "label": "skull"},
        {"op": "sample_var", "quest": "TestQuest", "var": "nCount", "frames": 4, "values": True, "label": "steps"},
    ]}


class Round2(unittest.TestCase):
    def steps(self):
        return cq.actions_job(ROUND2, "q5b_t", "nvfdoc", EDIDS)["steps"][len(cs.MASTERS):]

    def test_steps(self):
        steps = self.steps()
        given = [s for s in steps if s.get("label", "").startswith(cq.GIVEN_PREFIX)]
        self.assertEqual([s["field"] for s in given], ["var.PlayerLeft", "dead"])
        self.assertIn('rF1.GetVariable "PlayerLeft"', given[0]["query"])
        scripts = [s["script"] for s in steps if "script" in s]
        self.assertIn('rF1.SetVariable "PlayerLeft" 0', scripts[0])
        by = {}
        for s in steps:
            if "query" in s:
                by.setdefault(s["label"], []).append(s)
        self.assertEqual([s["field"] for s in by["doc"]], ["var.PlayerLeft", "dead", "disabled"])
        self.assertIn("rF1.GetDead", by["doc"][1]["query"])
        door = by["door"][0]["query"]
        self.assertLess(door.index("rF1.Activate player 1"), door.index('let fV := rF2.GetVariable "PlayerLeft"'))
        self.assertIn("rF1.Activate player\n", by["door plain"][0]["query"])
        self.assertIn("GetStage rF2", by["door plain"][0]["query"])
        self.assertTrue(any("rF1.Activate player 1" in s and "0x" not in s for s in scripts))
        self.assertEqual(by["hostage"][0]["field"], "opened")
        self.assertTrue(any("rF1.Activate player\n" in s for s in scripts))  # the fnv.sim.actions/1 path
        self.assertTrue(any("rF1.Kill" in s for s in scripts))
        self.assertTrue(any("rF1.EvaluatePackage" in s for s in scripts))
        self.assertIn("if rP == rF2", by["stays"][0]["query"])
        added = by["added"][0]["query"]
        self.assertLess(added.index("player.AddItem rF1 1 1"), added.index('($ player.GetItemCount rF2) + "|" + ($ player.GetItemCount rF3)'))
        self.assertEqual([s["field"] for s in by["sample:items"]], ["0", "1", "2"])
        gap = steps.index(by["menu:gap"][0])
        self.assertIn("rF1.Activate player", steps[gap - 2]["script"])
        self.assertEqual(steps[gap - 1], {"frames": 4})
        self.assertEqual(steps[gap + 1], {"console": "CloseAllMenus"})
        self.assertEqual([s["field"] for s in by["sample:gap"]], ["0", "1", "2"])
        self.assertIn('rF1.GetVariable "timer"', by["sample:skull"][0]["query"])

    def test_bad(self):
        for a in ({"op": "activate_read", "target": "0x1", "read": {"field": "stage"}, "label": "x"},
                  {"op": "activate_read", "target": "0x1", "read": {"ref": "0x2", "field": "nope"}, "label": "x"},
                  {"op": "get_ref", "ref": "0x1", "fields": ["stage"], "label": "x"},
                  {"op": "menu_gap", "via": "0x1", "frames": 0, "quest": "Q", "var": "v", "after": 1, "label": "x"},
                  {"op": "sample_items", "of": "player", "items": [], "frames": 2, "label": "x"},
                  {"op": "kill"}):
            with self.assertRaises(cq.InputError):
                cq.actions_job({"format": cq.ACTIONS_FORMAT, "actions": [a]}, "x", "y", EDIDS)

    def test_golden(self):
        rs = [{"label": "__given__:0", "field": "var.PlayerLeft", "value": 1.0},
              {"label": "__given__:0", "field": "dead", "value": 0.0},
              {"label": "doc", "field": "var.PlayerLeft", "value": 0.0},
              {"label": "doc", "field": "dead", "value": 0.0},
              {"label": "doc", "field": "disabled", "value": 1.0},
              {"label": "door", "field": "var.PlayerLeft", "value": 1.0},
              {"label": "hostage", "field": "opened", "value": 1.0},
              {"label": "stays", "field": "current", "value": 0.0},
              {"label": "added", "field": "counts", "value": "1|0"}]
        rs += [{"label": "sample:items", "field": str(k), "value": v} for k, v in enumerate(["1|0", "0|200", "0|200"])]
        rs += [{"label": "menu:gap", "field": "before", "value": 99.5}]
        rs += [{"label": "sample:gap", "field": str(k), "value": v} for k, v in enumerate([99.5, 99.453125, 99.4])]
        rs += [{"label": "sample:steps", "field": str(k), "value": v} for k, v in enumerate([0.0, 0.0, 1.0, 2.0])]
        mods, results = cs.read_output(output_lines(rs))
        doc, problems = cq.golden_document(ROUND2, mods, results)
        self.assertEqual(problems, [])
        obs = {o["label"]: o for o in doc["observations"]}
        self.assertEqual(obs["doc"], {"label": "doc", "step": 1, "var.PlayerLeft": 0.0, "dead": False, "disabled": True})
        self.assertEqual(obs["door"]["var.PlayerLeft"], 1.0)
        self.assertIs(obs["hostage"]["opened"], True)
        self.assertIs(obs["stays"]["current"], False)
        self.assertEqual({k: v for k, v in obs["added"].items() if k.startswith("count.")},
                         {"count.0x0016665D": 1, "count.0x00140AB2": 0})
        self.assertEqual(obs["items"], {"label": "items", "step": 10, "first.0x0016665D": 1, "final.0x0016665D": 0,
                                        "settled.0x0016665D": 1, "first.0x00140AB2": 0, "final.0x00140AB2": 200,
                                        "settled.0x00140AB2": 1})
        self.assertEqual(obs["gap"]["drop"], 0.046875)
        self.assertEqual(obs["steps"]["values"], [0.0, 1.0, 2.0])
        self.assertEqual(obs["steps"]["changes"], 2)

    def test_menu_gap_without_change(self):
        obs = cq.menu_gap_observation({}, {"value": 5.0}, [{"field": "0", "value": 5.0}])
        self.assertIn("error", obs)


ROUND3 = {
    "format": cq.ACTIONS_FORMAT, "title": "round 3",
    "given": [{"kind": "reputation", "rep": "0x00129A79", "fame": 0, "infamy": 0},
              {"kind": "actor_value", "of": "player", "av": "Nighteye", "value": 0}],
    "actions": [
        {"op": "set_reputation", "rep": "0x00129A79", "axis": "fame", "value": 2.5},
        {"op": "add_reputation", "rep": "0x00129A79", "axis": "infamy", "size": 3},
        {"op": "remove_reputation", "rep": "0x00129A79", "axis": "fame", "size": 1},
        {"op": "get_reputation", "rep": "0x00129A79", "fields": ["fame", "pct.infamy", "threshold.good"], "label": "rep"},
        {"op": "set_reputation_read", "rep": "0x00129A79", "fame": 3, "infamy": 10,
         "fields": ["fame", "threshold.mixed", "threshold.bad"], "label": "grid"},
        {"op": "use_item", "item": "0x0007EA1B", "read": [{"av": "Nighteye"}, {"item": "0x0007EA1B"}],
         "stopwatch": True, "label": "used"},
        {"op": "hold_menu", "via": "0x00103E37", "frames": 4},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 100, "label": "finish"},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 5, "label": "again"},
        {"op": "damage_av", "of": "player", "av": "PerceptionCondition", "value": 20},
        {"op": "sample_av_damage", "of": "player", "av": "PerceptionCondition", "damage": 5, "frames": 3, "label": "head"},
        {"op": "add_spell", "of": "player", "spell": "0x0014CCE2"},
        {"op": "sample_av", "of": "player", "av": "Health", "frames": 3, "label": "health"},
        {"op": "go_to_cell", "cell": "Goodsprings"},
        {"op": "sample_jumps", "reads": [{"global": "GameHour"}, {"ref": "0x001771EB", "var": "Time"},
                                         {"quest": "TestQuest", "var": "fTimer"}],
         "frames": 4, "min_step": 0.5, "label": "jumps"},
        {"op": "get_av", "of": "player", "name": "Nighteye", "label": "av"},
    ]}


class Round3(unittest.TestCase):
    def steps(self):
        return cq.actions_job(ROUND3, "q5c_t", "nvfdoc", EDIDS)["steps"][len(cs.MASTERS):]

    def test_steps(self):
        steps = self.steps()
        given = [s for s in steps if s.get("label", "").startswith(cq.GIVEN_PREFIX)]
        self.assertEqual([s["field"] for s in given], ["fame", "infamy", "value"])
        self.assertIn("GetReputation rF1 1", given[0]["query"])
        self.assertIn("player.GetAV Nighteye", given[2]["query"])
        scripts = [s["script"] for s in steps if "script" in s]
        # The stopwatch starts before the first action.
        self.assertIn('SetVariable "fTimer" 100000 rF1', scripts[0])
        self.assertIn("StartQuest rF1", scripts[0])
        self.assertIn("BuildRef fM1 1056823", scripts[0])
        self.assertIn("SetReputation rF1 1 2.5", scripts[1])
        self.assertIn("AddReputation rF1 0 3", scripts[2])
        self.assertIn("RemoveReputation rF1 1 1", scripts[3])
        by = {}
        for s in steps:
            if "query" in s:
                by.setdefault(s["label"], []).append(s)
        self.assertEqual([s["field"] for s in by["rep"]], ["fame", "pct.infamy", "threshold.good"])
        self.assertIn("GetReputationPct rF1 0", by["rep"][1]["query"])
        self.assertIn("GetReputationThreshold rF1 1", by["rep"][2]["query"])
        grid = by["grid"][0]
        self.assertEqual(grid["field"], "fame|threshold.mixed|threshold.bad")
        self.assertLess(grid["query"].index("SetReputation rF1 0 10"), grid["query"].index("GetReputationThreshold rF1 2"))
        used = by["used"][0]
        self.assertEqual(used["field"], "av.Nighteye|count.0x0007EA1B")
        self.assertLess(used["query"].index("player.EquipItem rF1 0 1"), used["query"].index("player.GetAV Nighteye"))
        self.assertIn('GetVariable "fTimer" rF3', used["query"])
        menu = next(k for k, s in enumerate(steps) if "script" in s and "rF1.Activate player" in s["script"])
        self.assertEqual(steps[menu + 1:menu + 4], [{"frames": 4}, {"console": "CloseAllMenus"}, {"frames": 5}])
        until = [s for s in steps if "until" in s]
        self.assertEqual([u["max_frames"] for u in until], [100, 5])
        self.assertIn("if player.GetAV Nighteye == 0", until[0]["until"])
        self.assertEqual(by["clock:finish"][0]["field"], "seconds|value")
        self.assertEqual([s["field"] for s in by["sample:head"]], ["0", "1", "2"])
        self.assertIn("player.DamageAV PerceptionCondition 5", by["sample:head"][0]["query"])
        self.assertTrue(any("player.AddSpell rF1" in s for s in scripts))
        self.assertIn({"console": "coc Goodsprings"}, steps)
        jumps = by["sample:jumps"][0]["query"]
        self.assertIn('($ (GameHour)) + "|" + ($ (rF1.GetVariable "Time")) + "|" + ($ (GetVariable "fTimer" rF2))', jumps)
        self.assertEqual(by["av"][0]["field"], "value")

    def test_bad(self):
        for a in ({"op": "set_reputation", "rep": "0x1", "axis": "good", "value": 1},
                  {"op": "add_reputation", "rep": "0x1", "axis": "fame", "size": 1.5},
                  {"op": "get_reputation", "rep": "0x1", "fields": ["threshold.neutral"], "label": "x"},
                  {"op": "use_item", "item": "0x1", "read": [], "label": "x"},
                  {"op": "use_item", "item": "0x1", "read": [{"av": "Health", "item": "0x2"}], "label": "x"},
                  {"op": "wait_av", "av": "Nighteye", "value": 0, "max_frames": 0, "label": "x"},
                  {"op": "hold_menu", "via": "0x1", "frames": 0},
                  {"op": "sample_av", "av": "Health", "frames": 1, "label": "x"},
                  {"op": "sample_jumps", "reads": [{"global": "GameHour"}], "frames": 3, "min_step": 1, "label": "x"},
                  {"op": "sample_jumps", "reads": [{"global": "GameHour"}, {"ref": "0x1"}], "frames": 3, "min_step": 1,
                   "label": "x"},
                  {"op": "go_to_cell", "cell": "0x0000DAEB"}):
            with self.assertRaises(cq.InputError, msg=a):
                cq.actions_job({"format": cq.ACTIONS_FORMAT, "actions": [a]}, "x", "y", EDIDS)

    def test_golden(self):
        rs = [{"label": "__given__:0", "field": "fame", "value": 0.0},
              {"label": "__given__:0", "field": "infamy", "value": 0.0},
              {"label": "__given__:1", "field": "value", "value": 0.0},
              {"label": "rep", "field": "fame", "value": 1.5},
              {"label": "rep", "field": "pct.infamy", "value": 0.2},
              {"label": "rep", "field": "threshold.good", "value": 4.0},
              {"label": "grid", "field": "fame|threshold.mixed|threshold.bad", "value": "3|0|2"},
              {"label": "used", "field": "av.Nighteye|count.0x0007EA1B", "value": "1|2|18.5156"},
              {"label": "clock:finish", "field": "seconds|value", "value": "138.453|0"},
              {"label": "clock:again", "field": "seconds|value", "value": "200|1"},
              {"label": "av", "field": "value", "value": 1.0}]
        rs += [{"label": "sample:head", "field": str(k), "value": v} for k, v in enumerate([80.0, 75.0, 70.0])]
        rs += [{"label": "sample:health", "field": str(k), "value": v}
               for k, v in enumerate(["160|1", "160.1|2", "160.1|3"])]
        rs += [{"label": "sample:jumps", "field": str(k), "value": v}
               for k, v in enumerate(["9.4|9.4|5", "9.41|9.41|5", "10.41|10.41|5", "10.42|10.42|5"])]
        mods, results = cs.read_output(output_lines(rs))
        doc, problems = cq.golden_document(ROUND3, mods, results)
        self.assertEqual(problems, [])
        obs = {o["label"]: o for o in doc["observations"]}
        self.assertEqual(obs["rep"], {"label": "rep", "step": 3, "fame": 1.5, "pct.infamy": 0.2, "threshold.good": 4})
        self.assertEqual(obs["grid"], {"label": "grid", "step": 4, "fame": 3.0, "threshold.mixed": 0, "threshold.bad": 2})
        self.assertEqual(obs["used"], {"label": "used", "step": 5, "av.Nighteye": 1.0, "count.0x0007EA1B": 2})
        self.assertTrue(obs["finish"]["found"])
        self.assertAlmostEqual(obs["finish"]["seconds"], 119.9374)
        self.assertEqual(obs["again"], {"label": "again", "step": 8, "found": False})
        self.assertEqual(obs["head"]["values"], [80.0, 75.0, 70.0])
        self.assertEqual(obs["health"]["changes"], 1)
        self.assertAlmostEqual(obs["health"]["per_second"], 0.05)
        self.assertEqual(obs["jumps"]["lags"], [0, None])
        self.assertEqual(obs["av"]["value"], 1.0)

    def test_wait_without_stopwatch(self):
        doc = {"format": cq.ACTIONS_FORMAT, "actions": [
            {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 5, "label": "w"}]}
        mods, results = cs.read_output(output_lines([{"label": "clock:w", "field": "seconds|value", "value": "3|0"}]))
        obs = cq.golden_document(doc, mods, results)[0]["observations"]
        self.assertIn("error", obs[0])

    def test_jumps_without_a_first_jump(self):
        obs = cq.jumps_observation({"reads": [{}, {}], "min_step": 1}, [{"field": "0", "value": "1|1"},
                                                                       {"field": "1", "value": "1|5"}])
        self.assertIn("error", obs)


ROUND4 = {
    "format": cq.ACTIONS_FORMAT, "title": "round 4",
    "given": [{"kind": "menu", "menu": 1001, "open": True}],
    "actions": [
        {"op": "set_quest_var", "quest": "TestQuest", "var": "fTimer", "value": 3, "stopwatch": True},
        {"op": "dismiss_message_boxes", "max": 2, "wait_frames": 50, "label": "boxes"},
        {"op": "dismiss_message_boxes", "max": 1, "wait_frames": 50},
        {"op": "wait_quest", "quest": "TestQuest", "field": "done.95", "value": True, "max_frames": 100,
         "stopwatch": True, "label": "95"},
        {"op": "wait_quest", "quest": "TestQuest", "field": "var.bDone", "value": 2, "max_frames": 10, "label": "var"},
        {"op": "wait_menu", "menu": 1051, "max_frames": 10, "label": "name menu"},
        {"op": "start_stopwatch"},
        {"op": "close_menus"},
        {"op": "get_menus", "menus": [1001, 1036], "label": "menus"},
        {"op": "get_tag_skills", "skills": ["Guns", "Sneak"], "label": "tags"},
        {"op": "get_ref", "ref": "0x00103E61", "fields": ["destroyed"], "label": "door"},
    ]}


class Round4(unittest.TestCase):
    def steps(self):
        return cq.actions_job(ROUND4, "q5d_t", "nvfnew", EDIDS)["steps"][len(cs.MASTERS):]

    def test_steps(self):
        steps = self.steps()
        self.assertEqual(steps[0]["label"], "__given__:0")
        self.assertIn("if MenuMode 1001", steps[0]["query"])
        # The variable and the stopwatch in one script.
        self.assertIn('SetVariable "fTimer" 3 rF1', steps[1]["script"])
        self.assertIn('rF2.SetVariable "timer" 100000', steps[1]["script"])
        self.assertIn("BuildRef fM1 1068048", steps[1]["script"])  # the skull, 0x00104C10
        boxes = [s for s in steps if s.get("label", "").startswith("box:")]
        self.assertEqual([s["field"] for s in boxes], ["0", "1"])
        self.assertEqual(sum(1 for s in steps if s.get("console") == "CloseAllMenus"), 4)
        until = [s for s in steps if "until" in s]
        self.assertEqual([u["max_frames"] for u in until], [50, 50, 50, 100, 10, 10])
        self.assertIn("if GetStageDone rF1 95 == 1", until[3]["until"])
        self.assertIn('if GetVariable "bDone" rF1 == 2', until[4]["until"])
        self.assertIn("if MenuMode 1051", until[5]["until"])
        by = {s["label"]: s for s in steps if "query" in s}
        self.assertEqual(by["wait:95"]["field"], "found|seconds")
        self.assertIn('100000 - rF2.GetVariable "timer"', by["wait:95"]["query"])
        self.assertEqual(by["wait:var"]["field"], "found")
        self.assertIn('SetVariable "timer" 100000', next(s["script"] for s in steps[steps.index(by["wait:name menu"]):]
                                                       if "script" in s))
        self.assertIn("IsPlayerTagSkill Sneak", [s for s in steps if s.get("label") == "tags"][1]["query"])
        self.assertIn("rF1.GetDestroyed", by["door"]["query"])

    def test_bad(self):
        for a in ({"op": "dismiss_message_boxes", "max": 0, "wait_frames": 5},
                  {"op": "wait_quest", "quest": "TestQuest", "field": "done.5", "max_frames": 0, "label": "x"},
                  {"op": "wait_quest", "quest": "TestQuest", "field": "nope", "max_frames": 5, "label": "x"},
                  {"op": "get_menus", "menus": [], "label": "x"},
                  {"op": "get_tag_skills", "skills": [], "label": "x"}):
            with self.assertRaises(cq.InputError, msg=a):
                cq.actions_job({"format": cq.ACTIONS_FORMAT, "actions": [a]}, "x", "y", EDIDS)

    def test_golden(self):
        rs = [{"label": "__given__:0", "field": "open", "value": 1.0},
              {"label": "box:boxes", "field": "0", "value": 1.0},
              {"label": "box:boxes", "field": "1", "value": 0.0},
              {"label": "wait:95", "field": "found|seconds", "value": "1|3.03125"},
              {"label": "wait:var", "field": "found", "value": "0"},
              {"label": "wait:name menu", "field": "found", "value": "1"},
              {"label": "menus", "field": "menu.1001", "value": 0.0},
              {"label": "menus", "field": "menu.1036", "value": 1.0},
              {"label": "tags", "field": "tag.Guns", "value": 1.0},
              {"label": "tags", "field": "tag.Sneak", "value": 0.0},
              {"label": "door", "field": "destroyed", "value": 1.0}]
        mods, results = cs.read_output(output_lines(rs))
        doc, problems = cq.golden_document(ROUND4, mods, results)
        self.assertEqual(problems, [])
        obs = {o["label"]: o for o in doc["observations"]}
        self.assertEqual(obs["boxes"], {"label": "boxes", "step": 1, "closed": 1})
        self.assertEqual(obs["95"], {"label": "95", "step": 3, "found": True, "seconds": 3.03125})
        self.assertEqual(obs["var"], {"label": "var", "step": 4, "found": False})
        self.assertEqual(obs["name menu"], {"label": "name menu", "step": 5, "found": True})
        self.assertEqual(obs["menus"], {"label": "menus", "step": 8, "menu.1001": False, "menu.1036": True})
        self.assertEqual(obs["tags"], {"label": "tags", "step": 9, "tag.Guns": True, "tag.Sneak": False})
        self.assertEqual(obs["door"], {"label": "door", "step": 10, "destroyed": True})
        self.assertEqual(len(doc["observations"]), 7)

    def test_given_menu_closed_is_a_mismatch(self):
        mods, results = cs.read_output(output_lines([{"label": "__given__:0", "field": "open", "value": 0.0}]))
        _, problems = cq.golden_document(ROUND4, mods, results)
        self.assertEqual(len(problems), 1)


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
