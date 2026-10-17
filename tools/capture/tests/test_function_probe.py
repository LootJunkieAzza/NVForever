"""Tests for tools/capture/function_probe.py (script-function probes). No game, no game
data: the probe file is written here, the game's and our answers are faked."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402
import function_probe as fp  # noqa: E402

FORMS = {"sunny": "FalloutNV.esm:104E85", "item": "FalloutNV.esm:020423", "gas": "FalloutNV.esm:109038",
         "dm": "DeadMoney.esm:000801"}
FNAMES = {"getdisposition": "GetDisposition", "getequipped": "GetEquipped", "setalert": "SetAlert",
          "hasloaded3d": "HasLoaded3D", "moveto": "MoveTo", "getcombattarget": "GetCombatTarget",
          "getpcmiscstat": "GetPCMiscStat"}


def batch(steps, **kw):
    return dict({"name": "t1", "title": "t", "start": "nvftest", "steps": steps}, **kw)


class GameScripts(unittest.TestCase):
    def test_a_query_is_the_form_that_compiled_in_game(self):
        # Character for character the fa_batch1_queries script that ran (2026-10-16).
        job, _, _ = fp.build_batch(batch([{"ask": "@sunny.GetEquipped @item"}]), FORMS, cs.MASTERS, FNAMES)
        q = job["steps"][len(cs.MASTERS)]
        self.assertEqual(q["query"], "float fM1\nref rF1\nref rF2\nfloat fV\nBegin Function { }\n"
                         "let fM1 := GetModIndex \"FalloutNV.esm\"\nlet rF1 := BuildRef fM1 1068677\n"
                         "let rF2 := BuildRef fM1 132131\nlet fV := rF1.GetEquipped rF2\nSetFunctionValue fV\nEnd\n")
        self.assertEqual((q["label"], q["field"]), ("@sunny.GetEquipped @item", "v"))

    def test_every_job_starts_with_the_mod_indices(self):
        job, _, _ = fp.build_batch(batch([]), FORMS, cs.MASTERS, FNAMES)
        self.assertEqual([s["field"] for s in job["steps"]], cs.MASTERS)
        self.assertTrue(all(s["label"] == cs.MODINDEX_LABEL for s in job["steps"]))

    def test_statements_end_with_a_value_and_a_pause(self):
        job, actions, plan = fp.build_batch(batch([{"do": "@sunny.SetAlert 1"}]), FORMS, cs.MASTERS, FNAMES)
        s, pause = job["steps"][len(cs.MASTERS):]
        self.assertIn("rF1.SetAlert 1\nSetFunctionValue 1\nEnd\n", s["script"])
        self.assertEqual(pause, {"frames": fp.SETTLE})
        self.assertEqual(actions["actions"], [{"op": "call", "line": "0x00104E85.SetAlert 1"}])
        self.assertEqual(plan, [])

    def test_until_declares_its_refs(self):
        job, actions, plan = fp.build_batch(batch([{"until": "@sunny.HasLoaded3D", "max_frames": 99, "label": "u"}]),
                                            FORMS, cs.MASTERS, FNAMES)
        u = job["steps"][len(cs.MASTERS)]
        self.assertTrue(u["until"].startswith("float fM1\nref rF1\nfloat fV\nBegin Function { }\n"))
        self.assertIn("if rF1.HasLoaded3D\n  let fV := 1\nendif\n", u["until"])
        self.assertEqual(u["max_frames"], 99)
        self.assertEqual(actions["actions"], [{"op": "call_until", "line": "0x00104E85.HasLoaded3D", "max_frames": 99, "label": "u"}])
        self.assertEqual(plan[0]["kind"], "until")

    def test_a_reference_answer_is_its_formid_text(self):
        job, _, plan = fp.build_batch(batch([{"ask": "@sunny.GetCombatTarget", "ref": True}]), FORMS, cs.MASTERS, FNAMES)
        q = job["steps"][len(cs.MASTERS)]["query"]
        self.assertIn("ref rScan\nBegin Function", q)
        self.assertIn("SetFunctionValue (GetFormIDString rScan)", q)
        self.assertTrue(plan[0]["ref"])

    def test_coc_waits_for_the_cell_and_moveto_for_the_3d(self):
        job, actions, _ = fp.build_batch(batch([{"coc": "GSGasStation", "cell": "@gas"}, {"moveto": "@sunny"}]),
                                         FORMS, cs.MASTERS, FNAMES)
        steps = job["steps"][len(cs.MASTERS):]
        self.assertEqual(steps[0], {"console": "coc GSGasStation"})
        self.assertIn("let rP := player.GetParentCell\nif rP == rF1", steps[1]["until"])
        self.assertEqual(steps[2], {"frames": fp.AFTER_COC})
        self.assertIn("player.MoveTo rF1", steps[3]["script"])
        self.assertIn("if rF1.HasLoaded3D", steps[4]["until"])
        self.assertEqual(actions["actions"][0], {"op": "go_to_cell", "cell": "GSGasStation"})
        self.assertEqual(actions["actions"][2], {"op": "call", "line": "player.MoveTo 0x00104E85"})

    def test_a_screenshot_hides_the_hud_in_game_only(self):
        job, actions, _ = fp.build_batch(batch([{"screenshot": "ringo-up"}]), FORMS, cs.MASTERS, FNAMES)
        steps = job["steps"][len(cs.MASTERS):]
        self.assertEqual([next(iter(s)) for s in steps], ["console", "frames", "screenshot", "console", "frames"])
        self.assertEqual(steps[2]["screenshot"], "ringo-up")
        self.assertEqual(actions["actions"], [])
        with self.assertRaises(fp.ProbeError):
            fp.build_batch(batch([{"screenshot": "two words"}]), FORMS, cs.MASTERS, FNAMES)

    def test_sides_and_per_side_queries(self):
        job, actions, plan = fp.build_batch(batch([
            {"ask": "@sunny.GetDisposition player", "side": "game"},
            {"game": "GameDay", "ours": {"op": "get_global", "name": "GameDay"}, "label": "day"},
            {"console": "tgm"},
        ], ours_start=[{"op": "go_to_cell", "cell": "GSGasStation"}]), FORMS, cs.MASTERS, FNAMES)
        self.assertNotIn("ours_step", plan[0])
        self.assertEqual(actions["actions"], [{"op": "go_to_cell", "cell": "GSGasStation"},
                                              {"op": "get_global", "name": "GameDay", "label": "day"}])
        self.assertEqual(job["steps"][-2:], [{"console": "tgm"}, {"frames": fp.SETTLE}])

    def test_our_formids_follow_the_masters_order(self):
        self.assertEqual(fp.our_text("@dm.GetDisposition @sunny", FORMS, cs.MASTERS), "0x01000801.GetDisposition 0x00104E85")

    def test_mistakes_are_refused(self):
        with self.assertRaisesRegex(fp.ProbeError, "not in \\[forms\\]"):
            fp.build_batch(batch([{"ask": "@nobody.GetDisposition player"}]), FORMS, cs.MASTERS, FNAMES)
        with self.assertRaisesRegex(fp.ProbeError, "not a function"):
            fp.build_batch(batch([{"ask": "player.GetNothing"}]), FORMS, cs.MASTERS, FNAMES)
        with self.assertRaisesRegex(fp.ProbeError, "used twice"):
            fp.build_batch(batch([{"ask": "player.GetDisposition player"}] * 2), FORMS, cs.MASTERS, FNAMES)
        with self.assertRaisesRegex(fp.ProbeError, "exactly one"):
            fp.build_batch(batch([{"ask": "x", "do": "y"}]), FORMS, cs.MASTERS, FNAMES)
        with self.assertRaisesRegex(fp.ProbeError, "not one of the masters"):
            fp.parse_form("Mod.esp:000800", cs.MASTERS)
        with self.assertRaisesRegex(fp.ProbeError, "batch name"):
            fp.build_batch(batch([], name="bad name"), FORMS, cs.MASTERS, FNAMES)

    def test_quoted_arguments_stay_one_word(self):
        self.assertEqual(fp.split_call('GetPCMiscStat "Locations Discovered"'), (None, "GetPCMiscStat", ['"Locations Discovered"']))
        self.assertEqual(fp.split_call("@sunny.GetDisposition player"), ("@sunny", "GetDisposition", ["player"]))


class Comparing(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = self.tmp.name
        doc = {"format": fp.PROBES_FORMAT, "forms": FORMS, "batch": [batch([
            {"ask": "@sunny.GetDisposition player"},
            {"ask": "@sunny.GetCombatTarget", "ref": True},
            {"ask": 'GetPCMiscStat "Locations Discovered"', "label": "loc"},
            {"until": "@sunny.HasLoaded3D", "label": "wait"},
            {"ask": "player.GetDisposition player", "side": "game", "label": "game only"},
        ])]}
        self.plan = fp.write_jobs(doc, self.dir, FNAMES)
        steps = {p["label"]: p.get("game_step") for p in self.plan["batches"][0]["probes"]}
        lines = [{"kind": "start"}] + [{"kind": "result", "step": i, "label": cs.MODINDEX_LABEL, "field": m, "value": i}
                                       for i, m in enumerate(cs.MASTERS)]
        lines += [{"kind": "result", "step": steps["@sunny.GetDisposition player"], "label": "x", "field": "v", "value": 35},
                  {"kind": "result", "step": steps["@sunny.GetCombatTarget"], "label": "x", "field": "v", "value": "00000014"},
                  {"kind": "result", "step": steps["loc"], "label": "loc", "field": "v", "value": 4},
                  {"kind": "waited", "step": steps["wait"], "frames": 17},
                  {"kind": "result", "step": steps["game only"], "label": "g", "field": "v", "error": "compile failed"},
                  {"kind": "end", "steps": 20}]
        with open(os.path.join(self.dir, "t1.game.jsonl"), "w", encoding="utf-8") as f:
            f.write("\n".join(json.dumps(x) for x in lines) + "\n")
        trace = {"observations": [
            {"label": "@sunny.GetDisposition player", "step": 0, "value": 0, "stub": "disposition not built"},
            {"label": "@sunny.GetCombatTarget", "step": 1, "value": "0x00000014"},
            {"label": "loc", "step": 2, "value": 4},
            {"label": "wait", "step": 3, "frames": 0}]}
        with open(os.path.join(self.dir, "t1.trace.json"), "w", encoding="utf-8") as f:
            json.dump(trace, f)

    def tearDown(self):
        self.tmp.cleanup()

    def test_rows_line_up_by_label(self):
        rows = {r["label"]: r for r in fp.compare(self.dir)}
        self.assertEqual(rows["@sunny.GetDisposition player"]["verdict"], "differs")
        self.assertEqual(rows["@sunny.GetDisposition player"]["stub"], "disposition not built")
        self.assertEqual(rows["@sunny.GetCombatTarget"]["game"], "FalloutNV.esm:000014")
        self.assertEqual(rows["@sunny.GetCombatTarget"]["ours"], "FalloutNV.esm:000014")
        self.assertEqual(rows["@sunny.GetCombatTarget"]["verdict"], "same")
        self.assertEqual(rows["loc"]["verdict"], "same")
        self.assertEqual((rows["wait"]["game"], rows["wait"]["ours"], rows["wait"]["verdict"]), (17, 0, "differs"))
        self.assertEqual(rows["game only"]["verdict"], "game error")

    def test_the_table_has_a_row_per_probe_and_totals(self):
        text = fp.markdown(fp.compare(self.dir))
        self.assertEqual(text.count("\n| t1 |"), 5)
        self.assertIn("0 (stub)", text)
        self.assertIn("Totals: differs 2, game error 1, same 2", text)

    def test_missing_answers(self):
        os.remove(os.path.join(self.dir, "t1.game.jsonl"))
        rows = fp.compare(self.dir)
        self.assertEqual({r["verdict"] for r in rows}, {"ours only", "no answer"})


class Files(unittest.TestCase):
    def test_the_committed_probes_build(self):
        path = os.path.join(os.path.dirname(HERE), "probes", "audit2.toml")
        with tempfile.TemporaryDirectory() as d:
            plan = fp.write_jobs(fp.load_probes(path), d, fp.function_names())
            self.assertGreater(sum(len(b["probes"]) for b in plan["batches"]), 100)
            names = [b["name"] for b in plan["batches"]]
            self.assertEqual(names, sorted(names), "the plugin runs jobs in name order")
            for n in names:
                with open(os.path.join(d, f"{n}.steps.json"), encoding="utf-8") as f:
                    job = json.load(f)
                self.assertFalse([s for s in job["steps"] if "manual" in s], "unattended jobs only")


if __name__ == "__main__":
    unittest.main()
