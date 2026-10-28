"""Synthetic HUD trait plans and jobs; no game files or running game."""
import copy
from contextlib import redirect_stderr, redirect_stdout
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_run as cr  # noqa: E402
import hud_traits as ht  # noqa: E402


def fixture():
    return {"schema": "fnv.ui.tree/1", "path": "HUDMainMenu", "warnings": ["PRIVATE_WARNING"],
            "tree": {"kind": "menu", "name": "HUDMainMenu", "templates": ["UnusedStencil"],
                     "traits": {"_InventedZero": 0, "_InventedText": "PRIVATE_VALUE",
                                "_InventedSymbol": "&invented;"},
                     "children": [{"kind": "rect", "name": "FictionalPanel", "templates": [],
                                   "traits": {"_InventedFraction": 1.25, "_InventedNegative": -8,
                                              "_InventedBool": True, "_InventedDigits": "12"},
                                   "children": [{"kind": "text", "name": "FictionalLabel", "templates": [],
                                                 "traits": {"_InventedLabel": "PRIVATE_LABEL"}, "children": []}]}]}}


def observe(job):
    return next(s["observe"] for s in job["steps"] if "observe" in s)


def scripts(obs):
    return obs["header"] + [s for g in obs["groups"] for s in [g["count"]] + g["probe"]]


class HudTraits(unittest.TestCase):
    def test_paths_and_numeric_filter(self):
        doc = fixture()
        paths = ht.list_tiles(doc)
        self.assertEqual([p for p, _ in paths], ["HUDMainMenu", "HUDMainMenu/FictionalPanel",
                                               "HUDMainMenu/FictionalPanel/FictionalLabel"])
        obs = observe(ht.make_job(doc, tiles=()))
        rows = obs["fields"]["tiles"]
        self.assertEqual([r["file_probes"] for r in rows], [2, 4, 0])
        self.assertEqual(rows[0]["traits"], list(doc["tree"]["traits"]))
        self.assertEqual(rows[1]["numeric_traits"], ["_InventedFraction", "_InventedNegative"])
        for g in obs["groups"]:
            self.assertEqual(g["count"], ht.number_script("1"))
            self.assertEqual(g["probe"][0], ht.number_script("1"))
        text = json.dumps(obs)
        for private in ("PRIVATE_VALUE", "PRIVATE_WARNING", "PRIVATE_LABEL", "UnusedStencil"):
            self.assertNotIn(private, text)

    def test_explicit_expression_metadata_is_selected_but_never_embedded(self):
        doc = fixture()
        doc["tree"]["traits"].update({"_InventedExpr": {"value": "text", "expr": "PRIVATE_EXPRESSION"},
                                      "_InventedWrapped": {"value": 5, "expr": None},
                                      "_InventedString": {"value": "text", "expr": None}})
        obs = observe(ht.make_job(doc, tiles=()))
        row = obs["fields"]["tiles"][0]
        self.assertEqual(row["numeric_traits"], ["_InventedZero", "_InventedExpr", "_InventedWrapped"])
        self.assertEqual(row["file_probes"], 6)
        self.assertNotIn("PRIVATE_EXPRESSION", json.dumps(obs))
        self.assertFalse(ht.is_numeric(float("nan")))
        self.assertFalse(ht.is_numeric(False))

    def test_runtime_children_and_counts(self):
        obs = observe(ht.make_job(fixture(), tiles=("FictionalPanel",), children=4,
                                  traits=("_InventedX", "_InventedY")))
        group = obs["groups"][-1]
        self.assertEqual((group["count"], group["max"]), (ht.number_script("4"), 4))
        self.assertEqual(group["probe"][1], ht.read_script("GetUIFloatAlt", "HUDMainMenu/FictionalPanel/*:%{i}/_InventedX"))
        for i in range(4):
            expanded = group["probe"][1].replace("%{i}", str(i))
            self.assertIn(f"/*:{i}/_InventedX", expanded)
        self.assertNotIn("GetUIFloatInherited", "".join(scripts(obs)))
        self.assertEqual(obs["fields"]["counts"]["trait_reads"], 6 + 8)
        self.assertEqual(obs["fields"]["tiles"][1]["child_probes"], 8)
        defaults = observe(ht.make_job(fixture()))
        self.assertEqual(sum(t["child_probes"] for t in defaults["fields"]["tiles"]), 9 * 12 * (7 + 3 * 2))

    def test_inherited_reads_x_y_depth_both_ways(self):
        doc = fixture()
        doc["tree"]["children"][0]["traits"].update({"x": 4, "Depth": 2})
        panel = "HUDMainMenu/FictionalPanel"
        obs = observe(ht.make_job(doc, tiles=("FictionalPanel",), children=2, traits=("y", "_InventedX")))
        text = "".join(scripts(obs))
        self.assertIn(ht.read_script("GetUIFloatInherited", f"{panel}/x"), text)
        self.assertIn(ht.read_script("GetUIFloatInherited:menu", f"{panel}/Depth"), text)
        self.assertTrue(ht.read_script("GetUIFloatInherited:menu", "a/x").endswith('"a/x" 1\nSetFunctionValue fV\nEnd\n'))
        self.assertNotIn(f'GetUIFloatInherited "{panel}/_InventedFraction"', text)
        self.assertIn(ht.read_script("GetUIFloatInherited", f"{panel}/*:%{{i}}/y"), text)
        self.assertNotIn(f'GetUIFloatInherited "{panel}/*:%{{i}}/_InventedX"', text)
        # The menu's 2 reads; the panel's 2 + 2 plus x and Depth at 4 each; children:
        # 2 x (y with 3 readers, _InventedX with 1).
        self.assertEqual(obs["fields"]["counts"]["trait_reads"], 2 + 4 + 8 + 2 * 4)
        self.assertIn("ExcludeMenuRoot", obs["fields"]["inherited"])
        off = observe(ht.make_job(doc, tiles=(), inherited=False))
        self.assertNotIn("GetUIFloatInherited", "".join(scripts(off)))
        self.assertNotIn("inherited", off["fields"])

    def test_documented_inherited_control(self):
        doc = fixture()
        doc["tree"]["children"].append({"kind": "rect", "name": "RadiationMeter", "templates": [], "traits": {},
                                        "children": [{"kind": "text", "name": "radiation_text", "templates": [],
                                                      "traits": {}, "children": []}]})
        obs = observe(ht.make_job(doc, tiles=()))
        rows = [h for h in obs["fields"]["header"] if isinstance(h, dict) and h["control"] == "documented"]
        self.assertEqual([(h["reader"], h["expected"]) for h in rows],
                         [("GetUIFloatInherited", -50), ("GetUIFloatInherited:menu", 0)])
        self.assertEqual(obs["fields"]["counts"]["control_reads"], 6)
        self.assertFalse(any(isinstance(h, dict) and h["control"] == "documented"
                             for h in observe(ht.make_job(fixture(), tiles=()))["fields"]["header"]))

    def test_controls_header_and_screenshots(self):
        job = ht.make_job(fixture(), tiles=(), every=11, shots=4, shot_gap=7, start="Invented save in invented room")
        obs = observe(job)
        self.assertEqual(job["start"], "Invented save in invented room")
        self.assertEqual(job["steps"][0]["max_frames"], 3600)
        self.assertIn("MenuMode == 0", job["steps"][0]["until"])
        self.assertEqual(job["steps"][1], {"frames": 300})
        self.assertEqual(sum("observe" in step for step in job["steps"]), 1)
        for expr in ("player.GetAV Health", "player.GetAV ActionPoints", "player.GetAngle Z"):
            self.assertTrue(any(expr in s for s in obs["header"]))
        controls = obs["fields"]["header"][3:]
        self.assertEqual([c["control"] for c in controls], ["exists", "exists", "missing", "missing"])
        self.assertTrue(all(c["path"] == "HUDMainMenu/_InventedZero" for c in controls[:2]))
        self.assertTrue(all(c["expected"] == -999 for c in controls[2:]))
        for c, script in zip(controls, obs["header"][3:]):
            self.assertEqual(script, ht.read_script(c["reader"], c["path"]))
        self.assertEqual(obs["fields"]["clock"]["relative_loops"], "n * 11")
        self.assertEqual(obs["shots"], {"label": "hud", "when": ht.number_script("1"), "gap": 7, "max": 4})
        self.assertFalse(any("console" in s for s in job["steps"]))
        self.assertNotIn("GetScreenTrait", "".join(scripts(obs)))
        self.assertNotIn("shots", observe(ht.make_job(fixture(), shots=0)))

    def test_escaping_and_full_numeric_scripts(self):
        doc = fixture()
        doc["tree"]["children"][0]["name"] = 'Fictional\\Nested"@Panel'
        doc["tree"]["children"][0]["traits"] = {'_Invented"Quote': 0}
        obs = observe(ht.make_job(doc, tiles=()))
        text = "".join(scripts(obs))
        self.assertIn('GetUIFloat "HUDMainMenu/Fictional/Nested%q@Panel/_Invented%qQuote"', text)
        self.assertNotIn('\\"', text)
        round_trip = json.loads(json.dumps(obs))
        self.assertEqual(round_trip, obs)
        for script in scripts(obs):
            self.assertTrue(script.startswith("float fV\nBegin Function { }\n"))
            self.assertIn("SetFunctionValue fV", script)
            self.assertTrue(script.endswith("End\n"))
        for bad in ("Injected\nEnd", "Injected\x00", "Injected%{i}", "Injected%q"):
            with self.subTest(bad=bad), self.assertRaises(ht.InputError):
                ht.make_job(fixture(), tiles=(bad,))

    def test_duplicate_and_unnamed_tiles_use_absolute_indices(self):
        doc = fixture()
        child = doc["tree"]["children"][0]
        second = copy.deepcopy(child)
        second["name"] = child["name"].lower()
        unnamed = copy.deepcopy(child)
        unnamed["name"] = ""
        doc["tree"]["children"] += [second, unnamed]
        paths = [p for p, _ in ht.list_tiles(doc)]
        for i in range(3):
            self.assertIn(f"HUDMainMenu/*:{i}", paths)

    def test_wide_tiles_split_and_account_for_every_call(self):
        doc = fixture()
        doc["tree"]["traits"] = {f"_Invented{n}": n for n in range(70)}
        obs = observe(ht.make_job(doc, tiles=("FictionalPanel",), children=2,
                                  traits=tuple(f"_InventedChild{n}" for n in range(70))))
        self.assertTrue(all(len(g["probe"]) <= ht.GROUP_READS + 1 for g in obs["groups"]))
        counts = obs["fields"]["counts"]
        self.assertEqual(counts["trait_reads"], 144 + 140)
        calls = len(obs["header"]) + sum(1 + g["max"] * len(g["probe"]) for g in obs["groups"]) + 1
        self.assertEqual(counts["max_calls_per_sample"], calls)

    def test_cli_list_does_not_write_and_job_passes_capture_loader(self):
        with tempfile.TemporaryDirectory() as tmp:
            tree = Path(tmp) / "invented-tree.json"
            tree.write_text(json.dumps(fixture()), encoding="utf-8")
            out = Path(tmp) / "invented_hud.steps.json"
            stdout = io.StringIO()
            with redirect_stdout(stdout):
                self.assertEqual(ht.main([str(tree), "--out", str(out), "--list"]), 0)
            self.assertFalse(out.exists())
            plan = json.loads(stdout.getvalue())
            self.assertEqual(plan["counts"]["trait_reads"], 6 + 9 * 12 * (7 + 3 * 2))
            tree.write_text(json.dumps(fixture()), encoding="utf-16")
            with redirect_stdout(io.StringIO()):
                self.assertEqual(ht.main([str(tree), "--out", str(out), "--start", "InventedSave"]), 0)
            loaded = cr.load_job(out)
            self.assertEqual(loaded["name"], "invented_hud")
            self.assertEqual(loaded["format"], "fnv.capture.steps/1")
            self.assertEqual(loaded["start"], "InventedSave")
            self.assertEqual(cr.manual_steps(loaded), [])
            # [design] The loader checks format and basename; the numeric script
            # and observe shape checks above cover what it does not validate.
            self.assertEqual(loaded, ht.make_job(fixture(), name="invented_hud", start="InventedSave"))
            with redirect_stderr(io.StringIO()):
                self.assertEqual(ht.main([str(tree), "--out", str(Path(tmp) / "bad.json")]), 1)

    def test_bad_input_is_rejected(self):
        for change in ({"schema": "wrong"}, {"path": "HUDMainMenu/FictionalPanel"}):
            doc = fixture()
            doc.update(change)
            with self.assertRaises(ht.InputError):
                ht.make_job(doc)
        doc = fixture()
        doc["tree"]["traits"] = {}
        doc["tree"]["children"] = []
        with self.assertRaises(ht.InputError):
            ht.make_job(doc)
        for kwargs in ({"children": -1}, {"every": 0}, {"shots": -1}, {"shot_gap": 0}, {"name": "bad name"}):
            with self.subTest(kwargs=kwargs), self.assertRaises(ht.InputError):
                ht.make_job(fixture(), **kwargs)


if __name__ == "__main__":
    unittest.main()
