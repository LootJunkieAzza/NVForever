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
# Editor IDs the lab inputs use, with made-up FormIDs (no game data here).
EDIDS = {e.lower(): 0x00010000 + i for i, e in enumerate([
    "Stimpak", "Caps001", "GSDocMitchellHouseIntDoorREF", "WeapNV9mmPistol", "ArmorLeather",
    "OutfitWasteland01", "Ammo9mm", "LootChemsStimpak50", "LL0NPCWastelandCaps35", "VFactionSquadStimpaks",
    "VendorChestStimpaksBoomer", "ClutterHouseholdJunkNV", "AmmoMissilelauncherScrounger", "Ammo10mmScrounger",
    "Ammo20GaScrounger", "CapsFortuneFinder", "NVDLC01ChipsChangeFortuneFinder", "VendorFlakShrapnelMiniNukes",
    "VendorArmorRare75", "VendorBasicMedicalSupplies100", "LootChemsNVAll50", "LL2Tier4ExplosivesMixed50", "VZKeyDirtyGSHome2SafeKey"])}


def actions(*acts):
    return {"format": "fnv.sim.actions/1", "seed": 0, "player_level": 1, "actions": list(acts)}


def body(job):
    """The steps after the mod-index header, without settle pauses."""
    return [s for s in job["steps"][len(cs.MASTERS):] if "frames" not in s]


def head(mods):
    return [{"kind": "result", "label": cs.MODINDEX_LABEL, "field": m, "value": float(i)} for m, i in mods]


def out_lines(results, mods=None, end=True):
    mods = mods if mods is not None else [(m, i) for i, m in enumerate(cs.MASTERS)]
    lines = [{"kind": "start", "job": "t"}] + head(mods)
    lines += [{"kind": "result", "step": i, **r} for i, r in enumerate(results)]
    if end:
        lines.append({"kind": "end", "steps": len(lines)})
    return [json.dumps(x) for x in lines]


class JobTests(unittest.TestCase):
    def test_every_lab_input_translates(self):
        for f in sorted(os.listdir(LAB_INPUTS)):
            with open(os.path.join(LAB_INPUTS, f), encoding="utf-8") as fh:
                doc = json.load(fh)
            if f.endswith(".actions.json"):
                job = cs.actions_job(doc, "x", "start", EDIDS)
            else:
                job = cs.cases_job(doc, "x", "start", 2, EDIDS)
            self.assertEqual(job["format"], cs.STEPS_FORMAT)
            self.assertEqual([s["field"] for s in job["steps"][:len(cs.MASTERS)]], cs.MASTERS)

    def test_forms_by_mod_index_and_decimal_local_id(self):
        job = cs.actions_job(actions({"op": "disable", "target": "0x0300DE81"},
                                     {"op": "get_item_count", "of": "player", "item": "Caps001", "label": "caps"}),
                             "x", "s", EDIDS)
        script = body(job)[0]["script"]
        self.assertEqual(script, 'float fM1\nref rF1\nBegin Function { }\nlet fM1 := GetModIndex "OldWorldBlues.esm"\n'
                                 'let rF1 := BuildRef fM1 56961\nrF1.Disable\nSetFunctionValue 1\nEnd\n')
        q = body(job)[1]
        self.assertEqual((q["label"], q["field"]), ("caps", "count"))
        self.assertIn(f"let rF1 := BuildRef fM1 {0x10001}", q["query"])
        self.assertIn("let fV := player.GetItemCount rF1", q["query"])
        self.assertTrue(q["query"].index("float fV") < q["query"].index("Begin Function"))

    def test_actions_return_a_value_and_avoid_string_variables(self):
        job = cs.actions_job(actions({"op": "set_global", "name": "GameHour", "value": 8},
                                     {"op": "remove_all_items", "from": "player"},
                                     {"op": "get_pos", "of": "player"}), "x", "s", EDIDS)
        for s in job["steps"]:
            text = s.get("script") or s.get("query") or ""
            self.assertNotIn("string_var", text)
            if "script" in s:
                self.assertIn("SetFunctionValue 1", text)
        self.assertIn("SetFunctionValue (GetFormIDString rC)", body(job)[2]["query"])

    def test_whole_numbers_have_no_decimal_point(self):
        job = cs.actions_job(actions({"op": "set_av", "of": "player", "name": "Strength", "value": 5.0},
                                     {"op": "set_global", "name": "GameHour", "value": 8},
                                     {"op": "set_global", "name": "GameHour", "value": 8.5}), "x", "s")
        steps = body(job)
        self.assertIn("player.SetAV Strength 5\n", steps[0]["script"])
        self.assertEqual(steps[1], {"console": "set GameHour to 8"})
        self.assertEqual(steps[2], {"console": "set GameHour to 8.5"})

    def test_actions_are_followed_by_a_pause(self):
        job = cs.actions_job(actions({"op": "equip_item", "actor": "player", "item": "0x00020423"},
                                     {"op": "add_item", "to": "player", "item": "0x0000000F", "count": 1},
                                     {"op": "get_weight", "of": "player"},
                                     {"op": "advance_time", "hours": 1}), "x", "s")
        steps = job["steps"][len(cs.MASTERS):]
        kinds = [next(iter(st)) for st in steps]
        self.assertEqual(kinds, ["script", "frames", "script", "frames", "query", "query", "manual"])
        self.assertEqual(steps[1]["frames"], cs.SETTLE_FRAMES_LONG)
        self.assertEqual(steps[3]["frames"], cs.SETTLE_FRAMES)

    def test_queries_fields_and_default_labels(self):
        job = cs.actions_job(actions({"op": "get_pos", "of": "player"},
                                     {"op": "get_weight", "of": "player", "label": "w"},
                                     {"op": "get_global", "name": "GameHour", "label": "h"}), "x", "s")
        fields = [(s["label"], s["field"]) for s in job["steps"][len(cs.MASTERS):]]
        self.assertEqual(fields, [("step0", "cell"), ("step0", "pos.x"), ("step0", "pos.y"), ("step0", "pos.z"),
                                  ("w", "weight"), ("w", "capacity"), ("h", "value")])

    def test_locks_and_labelled_activations(self):
        job = cs.actions_job(actions({"op": "get_locked", "of": "0x0010B2D4", "label": "safe"},
                                     {"op": "activate", "actor": "player", "target": "0x0010B2D4", "label": "open"},
                                     {"op": "activate", "actor": "player", "target": "0x0010B2D4"}), "x", "s")
        steps = job["steps"][len(cs.MASTERS):]
        self.assertEqual((steps[0]["label"], steps[0]["field"]), ("safe", "locked"))
        self.assertIn("let fV := rF1.GetLocked", steps[0]["query"])
        kinds = [next(iter(st)) for st in steps[1:]]
        # A labelled activation: the action, a pause, MenuMode 1008 as `opened`, then
        # every menu closed again; an unlabelled one is only followed by a pause.
        self.assertEqual(kinds, ["script", "frames", "query", "console", "frames", "script", "frames"])
        self.assertEqual((steps[3]["label"], steps[3]["field"]), ("open", "opened"))
        self.assertIn(f"MenuMode {cs.CONTAINER_MENU}", steps[3]["query"])
        self.assertEqual(steps[4], {"console": "CloseAllMenus"})

    def test_created_references_are_found_by_scanning_the_cell(self):
        job = cs.actions_job(actions({"op": "drop_item", "from": "player", "item": "Stimpak", "count": 2},
                                     {"op": "activate", "actor": "player", "target": "created:Stimpak"}),
                             "x", "s", EDIDS)
        steps = job["steps"][len(cs.MASTERS):]
        self.assertIn("player.Drop rF1 2", steps[0]["script"])
        self.assertEqual(steps[1], {"frames": cs.DROP_FRAMES})
        script = steps[2]["script"]
        self.assertIn(f"let rF1 := BuildRef fM1 {0x10000}", script)
        self.assertIn(f"let rScan := GetFirstRef {cs.INVENTORY_ITEM_REFS} 0 0", script)
        self.assertIn("if rScanBase == rF1", script)
        self.assertIn(f"if rScan.GetSourceModIndex == {cs.CREATED_MOD_INDEX}", script)
        self.assertIn("let rF2 := rScan", script)
        self.assertIn("rF2.Activate player", script)
        self.assertTrue(script.index("ref rScan") < script.index("Begin Function"))
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "activate", "actor": "player", "target": "created:NoSuchItem"}),
                           "x", "s", EDIDS)

    def test_errors(self):
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "teleport_somewhere"}), "x", "s", EDIDS)
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "get_global", "name": "0x00000038"}), "x", "s", EDIDS)
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "disable", "target": "0x0B000001"}), "x", "s", EDIDS)
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "disable", "target": "NoSuchRef"}), "x", "s", EDIDS)
        with self.assertRaises(cs.InputError) as e:
            cs.actions_job(actions({"op": "disable", "target": "Caps001"}), "x", "s", None)
        self.assertIn("--data", str(e.exception))
        with self.assertRaises(cs.InputError):
            cs.actions_job({"format": "other", "actions": []}, "x", "s")

    def test_confirm_after(self):
        acts = actions({"op": "activate", "actor": "player", "target": "0x00010000"},
                       {"op": "get_pos", "of": "player"})
        steps = body(cs.actions_job(acts, "x", "s", confirm_after={0}))
        self.assertIn("activate", steps[1]["manual"])
        self.assertEqual(steps[2]["field"], "cell")
        with self.assertRaises(cs.InputError):
            cs.actions_job(acts, "x", "s", confirm_after={5})

    def test_manual_steps(self):
        job = cs.actions_job(actions({"op": "advance_time", "hours": 24}, {"op": "detach_cell", "cell": "GSGasStation"},
                                     {"op": "transfer", "from": "a", "to": "b", "item": "c", "count": 2}), "x", "s")
        self.assertTrue(all("manual" in s for s in job["steps"][len(cs.MASTERS):]))

    def test_time_substitutes(self):
        acts = actions({"op": "advance_time", "hours": 30})
        hour = cs.actions_job(acts, "x", "s", time_mode="hour")["steps"][len(cs.MASTERS):]
        scripts = [s["script"] for s in hour if "script" in s]
        self.assertEqual(len(scripts), 2, "at most 24 hours at a time")
        self.assertIn("let fH := GameHour + 24\nset GameHour to fH", scripts[0])
        self.assertIn("GameHour + 6\n", scripts[1])
        self.assertTrue(all("frames" in s for s in hour if "script" not in s))
        ts = cs.actions_job(acts, "x", "s", time_mode="timescale")["steps"][len(cs.MASTERS):]
        self.assertEqual([s.get("as") for s in ts[:2]], ["days0", "scale0"])
        self.assertIn("GameDaysPassed", ts[0]["remember"])
        self.assertIn("TimeScale", ts[1]["remember"])
        self.assertEqual(ts[2], {"console": f"set TimeScale to {cs.FAST_TIMESCALE}"})
        self.assertIn("if GameDaysPassed >= %{days0} + 1.25\n", ts[3]["until"])
        self.assertEqual(ts[3]["max_frames"], 30 * cs.LOOPS_PER_HOUR)
        self.assertEqual(ts[4], {"console": "set TimeScale to %{scale0}"})
        self.assertIn("frames", ts[5])
        with self.assertRaises(cs.InputError):
            cs.actions_job(actions({"op": "advance_time", "hours": 0}), "x", "s", time_mode="hour")
        with self.assertRaises(cs.InputError):
            cs.actions_job(acts, "x", "s", time_mode="sleep")

    def test_settle_auto_waits_for_the_cell_change(self):
        acts = actions({"op": "activate", "actor": "player", "target": "0x00010000"},
                       {"op": "add_item", "to": "player", "item": "0x0000000F", "count": 1},
                       {"op": "get_pos", "of": "player"})
        steps = cs.actions_job(acts, "x", "s", confirm_after={0, 1}, settle_mode="auto")["steps"][len(cs.MASTERS):]
        self.assertFalse(any("manual" in s for s in steps))
        until = [s for s in steps if "until" in s]
        self.assertEqual(len(until), 1, "only an activation waits for a cell change")
        self.assertIn("let rD := rF1.GetParentCell", until[0]["until"])
        self.assertIn("BuildRef fM1 65536", until[0]["until"])
        self.assertEqual(until[0]["max_frames"], cs.DOOR_LOOPS)
        self.assertEqual(sum(1 for s in steps if s.get("frames") == cs.AFTER_DOOR_FRAMES), 2)

    def test_console_first_and_go_to_cell(self):
        acts = actions({"op": "get_disabled", "of": "0x00010000"})
        edids = {"nvdlc03x13": 0x0300328C}
        steps = cs.actions_job(acts, "x", "s", edids, console_first=["tgm", "tdetect"],
                               go_to_cell="NVDLC03X13")["steps"][len(cs.MASTERS):]
        self.assertEqual(steps[:3], [{"console": "tgm"}, {"console": "tdetect"}, {"console": "coc NVDLC03X13"}])
        self.assertIn('let fM1 := GetModIndex "OldWorldBlues.esm"', steps[3]["until"])
        self.assertIn("BuildRef fM1 12940", steps[3]["until"])
        self.assertIn("if rP == rF1", steps[3]["until"])
        self.assertEqual(steps[4], {"frames": cs.AFTER_DOOR_FRAMES})
        self.assertIn("query", steps[5])
        with self.assertRaises(cs.InputError):
            cs.actions_job(acts, "x", "s", edids, go_to_cell="NVDLC03X13; qqq")
        with self.assertRaises(cs.InputError):
            cs.actions_job(acts, "x", "s", edids, go_to_cell="NoSuchCell")

    def test_compare_traces(self):
        def trace(*obs):
            return {"format": "fnv.sim.trace/1", "observations": list(obs)}
        a = trace({"label": "box", "count": 3}, {"label": "p", "cell": "0x1", "pos": [1.0, 2.0, 3.0]}, {"label": "d", "disabled": True})
        b = trace({"label": "box", "count": 3}, {"label": "p", "cell": "0x1", "pos": [1.0, 2.0, 3.4]}, {"label": "d", "disabled": True})
        self.assertEqual(cs.compare_traces(a, a), [])
        self.assertEqual(cs.compare_traces(a, b), ["p: pos [1.0, 2.0, 3.0] vs [1.0, 2.0, 3.4]"])
        self.assertEqual(cs.compare_traces(a, b, tolerance=0.5), [])
        c = trace({"label": "box", "count": 0}, {"label": "new", "count": 1}, {"label": "d", "disabled": 1})
        diffs = cs.compare_traces(a, c)
        self.assertIn("box: count 3 vs 0", diffs)
        self.assertIn("p: only in first", diffs)
        self.assertIn("new: only in second", diffs)
        self.assertIn("d: disabled true vs 1", diffs, "a boolean is not a number")
        c2 = trace({"label": "box", "count": 3, "error": "script call failed"})
        self.assertIn('box: error null vs "script call failed"', cs.compare_traces(trace({"label": "box", "count": 3}), c2))
        with self.assertRaises(cs.InputError):
            cs.compare_traces(a, {"format": "fnv.sim.samples/1"})
        with tempfile.TemporaryDirectory() as tmp:
            pa, pb = os.path.join(tmp, "a.json"), os.path.join(tmp, "b.json")
            cs.write(a, pa)
            cs.write(b, pb)
            with redirect_stdout(io.StringIO()) as out:
                self.assertEqual(cs.main(["compare", pa, pb]), 2)
                self.assertEqual(cs.main(["compare", pa, pb, "--tolerance", "0.5"]), 0)
            self.assertIn("1 difference(s)", out.getvalue())

    def test_cases_job(self):
        doc = {"format": "fnv.sim.cases/1", "cases": [{"label": "a", "list": "LootChemsStimpak50", "level": 10, "count": 3},
                                                       {"label": "b", "list": "0x00177FAF"}]}
        job = cs.cases_job(doc, "x", "s", 4, EDIDS)
        fields = [s.get("field") for s in job["steps"] if s.get("label") == "a"]
        self.assertEqual(fields.count("draw3.num"), 1)
        self.assertEqual(len([f for f in fields if f.startswith("base.slot")]), cs.SLOTS)
        self.assertIn({"console": "player.setlevel 10"}, job["steps"])
        draws = [s["script"] for s in job["steps"] if "AddItem" in s.get("script", "")]
        self.assertEqual(len(draws), 8)
        self.assertIn("player.AddItem rF1 3 1", draws[0])
        self.assertIn(f"BuildRef fM1 {0x177FAF}", draws[4])
        with self.assertRaises(cs.InputError):
            cs.cases_job(doc, "x", "s", 0, EDIDS)


    def test_cases_job_sets_reads_and_restores_globals(self):
        doc = {"format": "fnv.sim.cases/1", "cases": [{"label": "a", "list": "Ammo10mmScrounger", "count": 4,
                                                       "globals": {"ScroungerOdds": 50}},
                                                      {"label": "b", "list": "LootChemsStimpak50"}]}
        steps = cs.cases_job(doc, "x", "s", 2, EDIDS)["steps"]
        i_rem = next(i for i, s in enumerate(steps) if s.get("as") == "case0g0")
        self.assertIn("let fV := ScroungerOdds", steps[i_rem]["remember"])
        self.assertEqual(steps[i_rem + 1], {"console": "set ScroungerOdds to 50"})
        fields = [(s.get("label"), s.get("field")) for s in steps if "query" in s]
        i_set, i_base = fields.index(("a", "global.ScroungerOdds")), fields.index(("a", "base.slot0"))
        i_after, i_last = fields.index(("a", "global_after.ScroungerOdds")), fields.index(("a", "draw1.num"))
        self.assertTrue(i_set < i_base and i_last < i_after)
        i_restore = steps.index({"console": "set ScroungerOdds to %{case0g0}"})
        self.assertTrue(i_restore < next(i for i, s in enumerate(steps) if s.get("label") == "b"))
        self.assertFalse(any(s.get("field", "").startswith("global") for s in steps if s.get("label") == "b"))
        with self.assertRaises(cs.InputError):
            cs.cases_job({"format": "fnv.sim.cases/1", "cases": [{"label": "c", "list": "X",
                                                                  "globals": {"0x000645CD": 5}}]}, "x", "s", 1, EDIDS)


class OutputTests(unittest.TestCase):
    def test_to_global_follows_the_recorded_mod_indices(self):
        mods = {0: "FalloutNV.esm", 2: "DeadMoney.esm", 4: "OldWorldBlues.esm"}  # a mod sits at 01
        self.assertEqual(cs.to_global("0400DE81", mods), "0x0300DE81")
        self.assertEqual(cs.to_global("000846EA", mods), "0x000846EA")
        self.assertEqual(cs.to_global("FF000801", mods), "0xFF000801")
        with self.assertRaises(cs.InputError):
            cs.to_global("01000800", mods)

    def test_not_loaded_masters_are_left_out(self):
        mods, _ = cs.read_output(out_lines([], mods=[("FalloutNV.esm", 0), ("DeadMoney.esm", 255)]))
        self.assertEqual(mods, {0: "FalloutNV.esm"})

    def test_trace_golden(self):
        lines = out_lines([
            {"label": "caps", "field": "count", "value": 5.0},
            {"label": "dis", "field": "disabled", "value": 1.0},
            {"label": "p", "field": "cell", "value": "00103DF9"},
            {"label": "p", "field": "pos.x", "value": 1.5},
            {"label": "p", "field": "pos.y", "value": -2.0},
            {"label": "p", "field": "pos.z", "value": 3.25},
            {"label": "w", "field": "weight", "value": 18.5},
            {"label": "w", "field": "capacity", "value": 200.0},
            {"label": "bad", "field": "value", "error": "script did not compile"},
            {"label": "lk", "field": "locked", "value": 1.0},
            {"label": "op", "field": "opened", "value": 0.0},
        ])
        g = cs.trace_golden(*cs.read_output(lines))
        obs = {o["label"]: o for o in g["observations"]}
        self.assertEqual(obs["caps"], {"label": "caps", "count": 5})
        self.assertIs(obs["dis"]["disabled"], True)
        self.assertEqual(obs["p"], {"label": "p", "cell": "0x00103DF9", "pos": [1.5, -2.0, 3.25]})
        self.assertEqual(obs["w"], {"label": "w", "weight": 18.5, "capacity": 200.0})
        self.assertEqual(obs["bad"]["error"], "script did not compile")
        self.assertEqual(obs["lk"], {"label": "lk", "locked": 1})
        self.assertIs(obs["op"]["opened"], False)

    def test_unfinished_or_aborted_output_is_refused(self):
        with self.assertRaises(cs.InputError):
            cs.read_output(out_lines([], end=False))
        with self.assertRaises(cs.InputError):
            cs.read_output(['{"kind": "start"}', '{"kind": "end"}'])  # no mod indices
        with self.assertRaises(cs.InputError):
            cs.read_output(out_lines([]) + ['{"kind": "aborted"}'])

    def test_samples_golden(self):
        def dump(prefix, items, num=None):
            slots = [f"{fid}x{n}" for fid, n in items] + ["00000000x0"] * (cs.SLOTS - len(items))
            rows = [{"label": "a", "field": f"{prefix}.slot{i}", "value": v} for i, v in enumerate(slots)]
            return rows + [{"label": "a", "field": f"{prefix}.num", "value": float(len(items) if num is None else num)}]
        pip = ("00015038", 1)  # stays in the inventory
        res = dump("base", [pip])
        res += dump("draw0", [pip, ("00015169", "1")])
        res += dump("draw1", [pip, ("00015169", "1.0000")])
        res += dump("draw2", [pip])
        res += dump("draw3", [pip, ("0000000F", 10), ("00015169", 2)])
        res += dump("draw4", [pip], num=cs.SLOTS + 1)  # overflow: more stacks than slots read
        res += dump("draw5", [pip])
        res[-2] = {"label": "a", "field": "draw5.slot11", "error": "script call failed"}
        cases = {"format": "fnv.sim.cases/1", "cases": [{"label": "a", "list": "LootChemsStimpak50"},
                                                         {"label": "b", "list": "X", "level": 5}]}
        g = cs.samples_golden(*cs.read_output(out_lines(res)), cases)
        a, b = g["cases"]
        self.assertEqual(a["outcomes"], {"0x0000000F x10, 0x00015169 x2": 1, "0x00015169 x1": 2, "nothing": 1})
        self.assertEqual((a["samples"], a["failed_draws"]), (4, 2))
        self.assertEqual((b["samples"], b["outcomes"], b["level"]), (0, {}, 5))

    def test_samples_golden_records_the_globals_read(self):
        slots = lambda prefix: [{"label": "a", "field": f"{prefix}.slot{i}", "value": "00000000x0"}  # noqa: E731
                                for i in range(cs.SLOTS)] + [{"label": "a", "field": f"{prefix}.num", "value": 0.0}]
        res = [{"label": "a", "field": "global.ScroungerOdds", "value": 50.0}] + slots("base") + slots("draw0")
        res += [{"label": "a", "field": "global_after.ScroungerOdds", "error": "script call failed"}]
        cases = {"format": "fnv.sim.cases/1", "cases": [{"label": "a", "list": "X", "globals": {"ScroungerOdds": 50}}]}
        (a,) = cs.samples_golden(*cs.read_output(out_lines(res)), cases)["cases"]
        self.assertEqual(a["outcomes"], {"nothing": 1})
        self.assertEqual(a["globals"], {"ScroungerOdds": 50})
        self.assertEqual(a["globals_read"], {"ScroungerOdds": 50.0})
        self.assertEqual(a["globals_read_after"], {"ScroungerOdds": None})

    def test_cli_round_trip(self):
        with tempfile.TemporaryDirectory() as d:
            act = os.path.join(d, "demo.actions.json")
            with open(act, "w", encoding="utf-8") as f:
                json.dump(actions({"op": "get_item_count", "of": "player", "item": "0x0000000F", "label": "caps"}), f)
            job = os.path.join(d, "job.json")
            self.assertEqual(cs.main(["actions", act, "--start", "Load a save.", "-o", job]), 0)
            with open(job, encoding="utf-8") as f:
                self.assertEqual(json.load(f)["name"], "demo")
            out = os.path.join(d, "out.jsonl")
            with open(out, "w", encoding="utf-8") as f:
                f.write("\n".join(out_lines([{"label": "caps", "field": "count", "value": 7}])) + "\n")
            buf = io.StringIO()
            with redirect_stdout(buf):
                self.assertEqual(cs.main(["trace", out]), 0)
            self.assertEqual(json.loads(buf.getvalue())["observations"], [{"label": "caps", "count": 7}])
            self.assertEqual(cs.main(["trace", act]), 1)  # not plugin output


if __name__ == "__main__":
    unittest.main()
