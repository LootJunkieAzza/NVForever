"""Tests for tools/capture/capture_steps.py (job files for nvf_capture, goldens from its
output). Synthetic data only."""
import io
import json
import os
import sys
import tempfile
import unittest
from contextlib import redirect_stdout

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402

LAB_INPUTS = os.path.join(HERE, "..", "..", "..", "parity-lab", "inputs", "layer4")


def actions(*acts):
    return {"format": "fnv.sim.actions/1", "seed": 0, "player_level": 1, "actions": list(acts)}


def out_lines(plugins, results, end=True):
    lines = [{"kind": "start", "job": "t"},
             {"kind": "result", "step": 0, "label": cs.LOAD_ORDER_LABEL, "field": "plugins", "value": "|".join(plugins) + "|"}]
    lines += [{"kind": "result", "step": i + 1, **r} for i, r in enumerate(results)]
    if end:
        lines.append({"kind": "end", "steps": len(lines)})
    return [json.dumps(x) for x in lines]


class JobTests(unittest.TestCase):
    def test_every_lab_input_translates(self):
        for f in sorted(os.listdir(LAB_INPUTS)):
            path = os.path.join(LAB_INPUTS, f)
            with open(path, encoding="utf-8") as fh:
                doc = json.load(fh)
            if f.endswith(".actions.json"):
                job = cs.actions_job(doc, "x", "start")
            else:
                job = cs.cases_job(doc, "x", "start", 3)
            self.assertEqual(job["format"], cs.STEPS_FORMAT)
            self.assertEqual(job["steps"][0]["label"], cs.LOAD_ORDER_LABEL)

    def test_forms_by_plugin_and_local_id(self):
        job = cs.actions_job(actions({"op": "disable", "target": "0x0300DE81"},
                                     {"op": "get_item_count", "of": "player", "item": "Caps001", "label": "caps"}),
                             "x", "s")
        script = job["steps"][1]["script"]
        self.assertIn('let rF1 := BuildRef (GetModIndex "OldWorldBlues.esm") 0x00DE81', script)
        self.assertIn("rF1.Disable", script)
        self.assertTrue(script.startswith("Begin Function { }\n") and script.endswith("End\n"))
        q = job["steps"][2]
        self.assertEqual((q["label"], q["field"]), ("caps", "count"))
        self.assertIn("let fV := player.GetItemCount Caps001", q["query"])

    def test_queries_fields_and_default_labels(self):
        job = cs.actions_job(actions({"op": "get_pos", "of": "player"},
                                     {"op": "get_weight", "of": "player", "label": "w"},
                                     {"op": "get_global", "name": "GameHour", "label": "h"}), "x", "s")
        fields = [(s["label"], s["field"]) for s in job["steps"][1:]]
        self.assertEqual(fields, [("step0", "cell"), ("step0", "pos.x"), ("step0", "pos.y"), ("step0", "pos.z"),
                                  ("w", "weight"), ("w", "capacity"), ("h", "value")])
        self.assertIn("GetFormIDString rC", job["steps"][1]["query"])
        self.assertIn("player.GetAV CarryWeight", job["steps"][6]["query"])

    def test_manual_steps_and_errors(self):
        job = cs.actions_job(actions({"op": "advance_time", "hours": 24}, {"op": "detach_cell", "cell": "GSGasStation"},
                                     {"op": "transfer", "from": "a", "to": "b", "item": "c", "count": 2}), "x", "s")
        self.assertTrue(all("manual" in s for s in job["steps"][1:]))
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "teleport_somewhere"}), "x", "s")
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "get_global", "name": "0x00000038"}), "x", "s")
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "disable", "target": "0x0B000001"}), "x", "s")  # index 11: not a master
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "disable", "target": "Bad Name;"}), "x", "s")
        with self.assertRaises(cs.InputError):
            cs.actions_job({"format": "other", "actions": []}, "x", "s")

    def test_cases_job(self):
        doc = {"format": "fnv.sim.cases/1", "cases": [{"label": "a", "list": "LL1", "level": 10, "count": 3},
                                                       {"label": "b", "list": "0x00177FAF"}]}
        job = cs.cases_job(doc, "x", "s", 4)
        draws = [s for s in job["steps"] if s.get("field") == cs.DRAW_FIELD]
        self.assertEqual(len(draws), 8)
        self.assertIn({"console": "player.setlevel 10"}, job["steps"])
        self.assertIn("player.AddItem LL1 3 1", draws[0]["query"])
        self.assertIn('BuildRef (GetModIndex "FalloutNV.esm") 0x177FAF', draws[4]["query"])
        with self.assertRaises(cs.InputError):
            cs.cases_job(doc, "x", "s", 0)


class OutputTests(unittest.TestCase):
    def test_to_global_follows_the_recorded_load_order(self):
        plugins = ["FalloutNV.esm", "SomeMod.esp", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm"]
        self.assertEqual(cs.to_global("0400DE81", plugins), "0x0300DE81")  # OWB is 03 among the masters
        self.assertEqual(cs.to_global("000846EA", plugins), "0x000846EA")
        self.assertEqual(cs.to_global("FF000801", plugins), "0xFF000801")
        with self.assertRaises(cs.InputError):
            cs.to_global("01000800", plugins)  # SomeMod.esp is not a master
        with self.assertRaises(cs.InputError):
            cs.to_global("09000800", plugins)

    def test_trace_golden(self):
        plugins = cs.MASTERS
        lines = out_lines(plugins, [
            {"label": "caps", "field": "count", "value": 5.0},
            {"label": "dis", "field": "disabled", "value": 1.0},
            {"label": "p", "field": "cell", "value": "000846EA"},
            {"label": "p", "field": "pos.x", "value": 1.5},
            {"label": "p", "field": "pos.y", "value": -2.0},
            {"label": "p", "field": "pos.z", "value": 3.25},
            {"label": "w", "field": "weight", "value": 18.5},
            {"label": "w", "field": "capacity", "value": 200.0},
            {"label": "bad", "field": "value", "error": "script did not compile"},
        ])
        g = cs.trace_golden(*cs.read_output(lines))
        self.assertEqual(g["format"], "fnv.sim.trace/1")
        obs = {o["label"]: o for o in g["observations"]}
        self.assertEqual(obs["caps"], {"label": "caps", "count": 5})
        self.assertIs(obs["dis"]["disabled"], True)
        self.assertEqual(obs["p"], {"label": "p", "cell": "0x000846EA", "pos": [1.5, -2.0, 3.25]})
        self.assertEqual(obs["w"], {"label": "w", "weight": 18.5, "capacity": 200.0})
        self.assertEqual(obs["bad"]["error"], "script did not compile")

    def test_unfinished_or_aborted_output_is_refused(self):
        with self.assertRaises(cs.InputError):
            cs.read_output(out_lines(cs.MASTERS, [], end=False))
        with self.assertRaises(cs.InputError):
            cs.read_output(out_lines(cs.MASTERS, [])[:1] + ['{"kind": "end"}'])  # no load order
        with self.assertRaises(cs.InputError):
            cs.read_output(out_lines(cs.MASTERS, []) + ['{"kind": "aborted"}'])

    def test_samples_golden(self):
        plugins = cs.MASTERS
        pip = "00015038x1;"  # an item that stays in the inventory
        draws = [
            pip + "#" + pip + "00015169x1;",
            pip + "#" + pip + "00015169x1.0000;",
            pip + "#" + pip,
            pip + "#" + pip + "0000000Fx10;00015169x2;",
        ]
        res = [{"label": "a", "field": cs.DRAW_FIELD, "value": v} for v in draws]
        res.append({"label": "a", "field": cs.DRAW_FIELD, "error": "script call failed"})
        cases = {"format": "fnv.sim.cases/1", "cases": [{"label": "a", "list": "LootChemsStimpak50"},
                                                         {"label": "b", "list": "X", "level": 5}]}
        g = cs.samples_golden(*cs.read_output(out_lines(plugins, res)), cases)
        a, b = g["cases"]
        self.assertEqual(a["samples"], 4)
        self.assertEqual(a["outcomes"], {"0x0000000F x10, 0x00015169 x2": 1, "0x00015169 x1": 2, "nothing": 1})
        self.assertEqual(a["failed_draws"], 1)
        self.assertEqual((b["samples"], b["outcomes"], b["level"]), (0, {}, 5))

    def test_cli_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            act = os.path.join(d, "demo.actions.json")
            with open(act, "w", encoding="utf-8") as f:
                json.dump(actions({"op": "get_item_count", "of": "player", "item": "Caps001", "label": "caps"}), f)
            job = os.path.join(d, "job.json")
            self.assertEqual(cs.main(["actions", act, "--start", "Load a save.", "-o", job]), 0)
            with open(job, encoding="utf-8") as f:
                self.assertEqual(json.load(f)["name"], "demo")
            out = os.path.join(d, "out.jsonl")
            with open(out, "w", encoding="utf-8") as f:
                f.write("\n".join(out_lines(cs.MASTERS, [{"label": "caps", "field": "count", "value": 7}])) + "\n")
            buf = io.StringIO()
            with redirect_stdout(buf):
                self.assertEqual(cs.main(["trace", out]), 0)
            self.assertEqual(json.loads(buf.getvalue())["observations"], [{"label": "caps", "count": 7}])
            self.assertEqual(cs.main(["trace", act]), 1)  # not plugin output


if __name__ == "__main__":
    unittest.main()
