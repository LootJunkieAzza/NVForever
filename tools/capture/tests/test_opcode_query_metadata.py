"""Registration queries establish command IDs, not command execution or support."""
import collections
import json
import os
import sys
import unittest

CAPTURE_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
sys.path.insert(0, CAPTURE_DIR)
import script_functions as sf


INVENTORY_QUERY_IDS = {
    "getcurrenthealth": 0x1451, "setcurrenthealth": 0x1452,
    "getrefcount": 0x1476, "getowner": 0x14D7,
    "isequipped": 0x15B9, "getinvrefsforitem": 0x15BA,
    "getweaponrefmodflags": 0x270C, "setweaponrefmodflags": 0x270D,
    "getitemrefcurrenthealth": 0x270F, "setitemrefcurrenthealth": 0x2710,
    "getallitemrefs": 0x2767, "removemeiralt": 0x27E2,
}

MAGIC_PIPBOY_REFRACTION_QUERY_IDS = {
    "PlayMagicEffectVisuals": 0x1124, "StopMagicEffectVisuals": 0x1126,
    "EvaluateSpellConditions": 0x119D, "GetPipBoyMode": 0x3168,
    "SetActorRefraction": 0x1154,
}

PARENT_BATCH_QUERY_IDS = {
    "ModScale": (0x113D, "GECK1"), "SetSize": (0x10A4, "GECK1"),
    "AddNPCToLeveledList": (0x1234, "GECK1"),
    "AddCreatureToLeveledList": (0x1235, "GECK1"),
    "PlaceAtMeHealthPercent": (0x11BD, "GECK1"),
    "SetRadioToDialogueOnlyMode": (0x1278, "VEGAS1"),
    "DisableHardcoreTracking": (0x126A, "VEGAS1"),
    "ToggleDemigodMode": (0x124C, "VEGAS1"),
    "GetFollowers": (0x2637, "JIP"), "GetDroppedRefs": (0x2780, "JIP"),
}


def empty_scan():
    return {key: {} for key in
            ("top", "expr", "calls", "scripts", "ctda", "begin", "statements", "stats")}


def wiki_row(name, origin="NVSE"):
    return {"name": name, "origin": origin, "page": name, "categories": []}


class RegistrationQueryMetadata(unittest.TestCase):
    def test_parent_batch_ids_match_capture_and_render_as_queries(self):
        root = os.path.dirname(os.path.dirname(CAPTURE_DIR))
        with open(os.path.join(root, "schemas/script/command_opcodes.json"), encoding="utf-8") as f:
            commands = json.load(f)["opcodes"]
        wiki = [wiki_row(n, origin) for n, (_, origin) in PARENT_BATCH_QUERY_IDS.items()]
        aliases = {"SetSize": ["CSize"], "DisableHardcoreTracking": ["babby"],
                   "ToggleDemigodMode": ["TDM"]}
        for row in wiki:
            if row["name"] in aliases:
                row["aliases"] = aliases[row["name"]]
        doc = sf.build(wiki, empty_scan(), {}, {})
        output = json.loads(sf.render_json(doc))
        rows = {f["name"]: f for f in output["functions"]}
        md = sf.render_md(output)
        for name, (opcode, origin) in PARENT_BATCH_QUERY_IDS.items():
            with self.subTest(name=name):
                self.assertEqual(commands[name], opcode)
                self.assertEqual((rows[name]["opcodes"], rows[name]["opcode_evidence"]),
                                 ([f"0x{opcode:04X}"], "GetCommandOpcode"))
                self.assertNotIn("script_calls", rows[name])
                self.assertNotIn("scripts_using", rows[name])
                self.assertNotIn(name.lower(), sf.DOCUMENTED_OPCODES,
                                 "retained query evidence must not be relabeled as documentation")
                if name in aliases:
                    self.assertEqual(rows[name]["aliases"], aliases[name])
                if origin == "JIP":
                    self.assertNotIn(name, md)
                else:
                    self.assertIn(f"0x{opcode:04X} query |  |", md)

    def test_parent_batch_scan_and_admitted_xedit_precedence(self):
        for name, (opcode, origin) in PARENT_BATCH_QUERY_IDS.items():
            with self.subTest(name=name):
                wiki = [wiki_row(name, origin)]
                xedit = {0x234: (name, [])}
                indexed = sf.build(wiki, empty_scan(), xedit, {})
                row = next(f for f in indexed["functions"] if f["name"] == name)
                expected = ([f"0x{opcode:04X}"], "GetCommandOpcode") if origin == "JIP" else (["0x1234"], "xedit_index")
                self.assertEqual((row["opcodes"], row["opcode_evidence"]), expected)
                scan = empty_scan()
                scan.update(top={0x1321: collections.Counter({name: 2})},
                            calls={0x1321: 2}, scripts={0x1321: 1})
                observed = sf.build(wiki, scan, xedit, {})
                row = next(f for f in observed["functions"] if f["name"] == name)
                self.assertEqual((row["opcodes"], row["opcode_evidence"], row["script_calls"]),
                                 (["0x1321"], "observed", 2))
                if origin != "JIP":
                    self.assertIn("0x1321 obs", sf.render_md(observed))

    def test_setting_collision_preserves_both_rows_and_native_opcode_owner(self):
        # The console page's collected name collides with the two-string setter.
        console = {"name": "SetStringSetting", "page": "ShowFullSceneGraph",
                   "origin": "GECK1", "console_only": True,
                   "returns": {"type": "void", "name": None},
                   "categories": ["Console Functions", "Designer Debug Tools"],
                   "summary": "Displays the selected scene graph."}
        setter = {"name": "SetStringSetting", "page": "SetStringSetting", "origin": "LU",
                  "params": [{"name": "settingName", "type": "string", "kind": "string"},
                             {"name": "newValue", "type": "string", "kind": "string"}],
                  "categories": ["Functions (JIP)"]}
        for inputs in ([console, setter], [setter, console]):
            with self.subTest(first_page=inputs[0]["page"]):
                doc = sf.build(inputs, empty_scan(), {}, {})
                output = json.loads(sf.render_json(doc))
                rows = [r for r in output["functions"] if r["name"] == "SetStringSetting"]
                self.assertEqual(len(rows), 2)
                false = next(r for r in rows if r["page"] == "ShowFullSceneGraph")
                native = next(r for r in rows if r["page"] == "SetStringSetting")
                self.assertEqual((false["origin"], false["family"], false["console_only"]),
                                 ("GECK1", "vanilla", True))
                self.assertEqual(false["summary"], console["summary"])
                self.assertNotIn("params", false)
                self.assertNotIn("opcodes", false)
                self.assertNotIn("opcode_evidence", false)
                self.assertEqual((native["origin"], native["family"], native["opcodes"], native["opcode_evidence"]),
                                 ("LU", "extender", ["0x2284"], "GetCommandOpcode"))
                self.assertEqual(native["params"], setter["params"])
                first_by_opcode = next(r for r in output["functions"] if "0x2284" in r.get("opcodes", []))
                self.assertEqual(first_by_opcode["page"], "SetStringSetting")
                md = sf.render_md(output)
                line = next(l for l in md.splitlines() if "| [SetStringSetting]" in l)
                self.assertIn("title=ShowFullSceneGraph) |  |", line)
                self.assertIn(console["summary"], line)
                self.assertNotIn("0x2284", md)
                self.assertNotIn("settingName", line)
                self.assertNotIn("title=SetStringSetting)", md)

    def test_setting_query_requires_both_origin_and_own_page(self):
        for origin, page in [("GECK1", "SetStringSetting"), ("LU", "ShowFullSceneGraph"),
                             ("NVSE", "SetStringSetting"), ("LU", "")]:
            with self.subTest(origin=origin, page=page):
                rec = dict(wiki_row("SetStringSetting", origin), page=page)
                row = next(r for r in sf.build([rec], empty_scan(), {}, {})["functions"]
                           if r["name"] == "SetStringSetting")
                self.assertNotIn("opcodes", row)

    def test_settings_casino_default_query_capture_and_xedit_precedence(self):
        expected = {"SetStringSetting": 0x2284, "SetCasinoWinningsLevel": 0x125D,
                    "SetDefaultOpen": 0x10D8, "GetDefaultOpen": 0x10D7}
        root = os.path.dirname(os.path.dirname(CAPTURE_DIR))
        with open(os.path.join(root, "schemas/script/command_opcodes.json"), encoding="utf-8") as f:
            commands = json.load(f)["opcodes"]
        wiki = [wiki_row(n, "LU" if n == "SetStringSetting" else "GECK1") for n in expected]
        doc = sf.build(wiki, empty_scan(), {}, {})
        rows = {f["name"]: f for f in doc["functions"]}
        md = sf.render_md(doc)
        for name, opcode in expected.items():
            with self.subTest(name=name):
                self.assertEqual(commands[name], opcode)
                self.assertEqual((rows[name]["opcodes"], rows[name]["opcode_evidence"]),
                                 ([f"0x{opcode:04X}"], "GetCommandOpcode"))
                self.assertNotIn("script_calls", rows[name])
                if name != "SetStringSetting":
                    self.assertIn(f"0x{opcode:04X} query |  |", md)
        indexed = sf.build(wiki, empty_scan(), {0xD7: ("GetDefaultOpen", [])}, {})
        row = next(f for f in indexed["functions"] if f["name"] == "GetDefaultOpen")
        self.assertEqual((row["opcodes"], row["opcode_evidence"]), (["0x10D7"], "xedit_index"))
        self.assertIn("0x10D7 xe", sf.render_md(indexed))
        self.assertNotIn("getstringsetting", sf.REGISTRATION_QUERY_OPCODES,
                         "the getter is a separate followup")

    def test_magic_pipboy_refraction_query_ids_and_actual_markdown(self):
        root = os.path.dirname(os.path.dirname(CAPTURE_DIR))
        with open(os.path.join(root, "schemas/script/command_opcodes.json"), encoding="utf-8") as f:
            commands = json.load(f)["opcodes"]
        wiki = [wiki_row(n, "JohnnyGuitar" if n == "GetPipBoyMode" else "GECK1")
                for n in MAGIC_PIPBOY_REFRACTION_QUERY_IDS]
        doc = sf.build(wiki, empty_scan(), {}, {})
        rows = {f["name"]: f for f in doc["functions"]}
        md = sf.render_md(doc)
        for name, opcode in MAGIC_PIPBOY_REFRACTION_QUERY_IDS.items():
            with self.subTest(name=name):
                self.assertEqual(opcode, commands[name])
                self.assertEqual((rows[name]["opcodes"], rows[name]["opcode_evidence"]),
                                 ([f"0x{opcode:04X}"], "GetCommandOpcode"))
                self.assertNotIn("script_calls", rows[name])
                self.assertNotIn("scripts_using", rows[name])
                self.assertNotIn("condition_index", rows[name])
                if name == "GetPipBoyMode":
                    self.assertNotIn(name, md, "extender rows stay out of vanilla Markdown")
                else:
                    self.assertIn(f"| [{name}]", md)
                    self.assertIn(f"0x{opcode:04X} query |  |", md)
        self.assertIn("registration query, not execution", md)

    def test_twelve_query_ids_survive_without_inventing_executions(self):
        root = os.path.dirname(os.path.dirname(CAPTURE_DIR))
        with open(os.path.join(root, "schemas/script/command_opcodes.json"), encoding="utf-8") as f:
            commands = {n.lower(): op for n, op in json.load(f)["opcodes"].items()}
        doc = sf.build([wiki_row(n) for n in INVENTORY_QUERY_IDS], empty_scan(), {}, {})
        rows = {f["name"]: f for f in doc["functions"]}
        for name, opcode in INVENTORY_QUERY_IDS.items():
            with self.subTest(name=name):
                self.assertEqual(opcode, commands[name])
                self.assertEqual(rows[name]["opcodes"], [f"0x{opcode:04X}"])
                self.assertEqual(rows[name]["opcode_evidence"], "GetCommandOpcode")
                self.assertNotIn("script_calls", rows[name])
                self.assertNotIn("scripts_using", rows[name])
                self.assertNotIn("condition_index", rows[name])
                self.assertNotIn(name, sf.render_md(doc), "the Markdown catalogue remains vanilla-only")

    def test_observed_scan_overrides_query_with_actual_usage_counts(self):
        scan = empty_scan()
        scan.update(top={0x1234: collections.Counter({"GetCurrentHealth": 2})},
                    calls={0x1234: 2}, scripts={0x1234: 1})
        doc = sf.build([wiki_row("GetCurrentHealth")], scan, {}, {})
        row = next(f for f in doc["functions"] if f["name"] == "GetCurrentHealth")
        self.assertEqual((row["opcodes"], row["opcode_evidence"], row["script_calls"], row["scripts_using"]),
                         (["0x1234"], "observed", 2, 1))

    def test_existing_xedit_admission_and_precedence_are_preserved(self):
        xedit = {0x234: ("GetCurrentHealth", [])}
        for origin, opcode, evidence in [("GECK1", "0x1234", "xedit_index"),
                                          ("NVSE", "0x1451", "GetCommandOpcode")]:
            with self.subTest(origin=origin):
                doc = sf.build([wiki_row("GetCurrentHealth", origin)], empty_scan(), xedit, {})
                row = next(f for f in doc["functions"] if f["name"] == "GetCurrentHealth")
                self.assertEqual((row["opcodes"], row["opcode_evidence"]), ([opcode], evidence))
                self.assertEqual("condition_index" in row, origin == "GECK1")

    def test_real_markdown_renderer_distinguishes_all_four_evidence_sources(self):
        # Synthetic vanilla rows exercise the renderer's query branch even though the
        # twelve currently retained query commands belong to the extender catalogue.
        rows = [{"name": name, "family": "vanilla", "opcodes": [opcode],
                 "opcode_evidence": evidence}
                for name, opcode, evidence in [("Scanned", "0x1001", "observed"),
                    ("Indexed", "0x1002", "xedit_index"), ("Documented", "0x1003", "documented"),
                    ("Queried", "0x1004", "GetCommandOpcode")]]
        md = sf.render_md({"functions": rows})
        for opcode, tag in [("0x1001", "obs"), ("0x1002", "xe"),
                            ("0x1003", "doc"), ("0x1004", "query")]:
            self.assertIn(opcode + " " + tag, md)
        self.assertIn("| Queried | 0x1004 query |  |", md)
        self.assertIn("registration query", md)
        self.assertIn("not execution", md)
        self.assertNotIn("0x1004 xe", md)


if __name__ == "__main__":
    unittest.main()
