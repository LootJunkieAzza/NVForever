"""Tests for tools/capture/function_coverage.py (the coverage tables). Inputs are written
here; no game data."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import function_coverage as fc  # noqa: E402

AUDIT = """# audit

| Function | Opcode | Calls / cond | Reads | Written by | Status | Note |
|---|---|---|---|---|---|---|
| Alpha | 0x1001 | 1/1 | x | y | matches |  |
| Beta / Gamma | 0x1002 / 0x1003 | 1/1 | x | y | stub | no system |
| Delta | 0x1004 | 1/1 | x | y | fixed here | done |
| Eps | 0x1005 | 1/1 | x | y | gap left | wrong |
"""

SYSTEMS = """format = "fnv.function.systems/1"
[groups]
main = ["^MQ"]
companions = ["Buddy"]
[function.Beta]
status = "no-system"
system = "flying"
note = "stub"
[function.Gamma]
status = "no-system"
system = "flying"
[function.Eps]
status = "wrong"
system = "walking"
note = "half built"
"""


def cov(*names):
    return {"opcodes": [{"opcode": f"0x{0x1001 + i:04X}", "name": n, "calls": 3, "script_calls": 2, "condition_uses": 5,
                         "implemented": True, "stub": n in ("Beta", "Gamma")} for i, n in enumerate(names)]}


USES = {"quests": [
    {"form": "0x1", "edid": "MQ01", "name": "Main One", "calls": {"Beta": 2}, "condition_uses": {}},
    {"form": "0x2", "edid": "Buddy", "name": "Companion", "calls": {"Eps": 1}, "condition_uses": {"Gamma": 4}},
    {"form": "0x3", "edid": "Side", "name": "", "calls": {"Gamma": 1}, "condition_uses": {}},
    {"form": "0x4", "edid": "Other", "name": "x", "calls": {"Alpha": 9}, "condition_uses": {}},
]}


class Coverage(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        d = self.tmp.name
        self.audit = os.path.join(d, "audit.md")
        self.systems = os.path.join(d, "systems.toml")
        with open(self.audit, "w", encoding="utf-8") as f:
            f.write(AUDIT)
        with open(self.systems, "w", encoding="utf-8") as f:
            f.write(SYSTEMS)

    def tearDown(self):
        self.tmp.cleanup()

    def rows(self, *names):
        _, systems = fc.load_systems(self.systems)
        return fc.classify(cov(*names), fc.audit_statuses(self.audit), systems)

    def test_rows_split_and_statuses_come_from_audit_or_systems(self):
        a = fc.audit_statuses(self.audit)
        self.assertEqual(a["gamma"][0], "stub")
        rows = {r["name"]: r for r in self.rows("Alpha", "Beta", "Gamma", "Delta", "Eps")}
        self.assertEqual({k: v["status"] for k, v in rows.items()},
                         {"Alpha": "match", "Beta": "no-system", "Gamma": "no-system", "Delta": "match", "Eps": "wrong"})
        self.assertEqual(rows["Eps"]["system"], "walking")

    def test_nothing_unclassified_passes(self):
        with open(self.systems, "w", encoding="utf-8") as f:
            f.write('format = "fnv.function.systems/1"\n')
        with self.assertRaisesRegex(fc.CoverageError, "Beta, Eps, Gamma"):
            self.rows("Alpha", "Beta", "Gamma", "Eps")

    def test_a_wrong_function_names_its_system(self):
        with open(self.systems, "w", encoding="utf-8") as f:
            f.write('format = "fnv.function.systems/1"\n[function.Eps]\nstatus = "wrong"\n')
        with self.assertRaisesRegex(fc.CoverageError, "names its system"):
            fc.load_systems(self.systems)

    def test_blockers_rank_by_quests_and_name_main_and_companions(self):
        doc, _ = fc.load_systems(self.systems)
        ranked = fc.blockers(self.rows("Alpha", "Beta", "Gamma", "Delta", "Eps"), USES, doc["groups"])
        self.assertEqual([b["system"] for b in ranked], ["flying", "walking"])
        fly = ranked[0]
        self.assertEqual((fly["quests"], fly["quest_calls"], fly["quest_conditions"]), (3, 3, 4))
        self.assertEqual(fly["main"], ["MQ01 (Main One)"])
        self.assertEqual(fly["companions"], ["Buddy (Companion)"])
        self.assertEqual((fly["calls"], fly["conditions"], fly["status"]), (4, 10, "no-system"))

    def test_the_doc_region_is_replaced(self):
        doc = os.path.join(self.tmp.name, "doc.md")
        with open(doc, "w", encoding="utf-8") as f:
            f.write(f"# Doc\n\nkept\n\n{fc.BEGIN}\nold\n{fc.END}\n\nkept too\n")
        rows = self.rows("Alpha", "Beta", "Gamma", "Delta", "Eps")
        d, _ = fc.load_systems(self.systems)
        text = fc.tables(rows, fc.blockers(rows, USES, d["groups"]), fc.probe_counts([{"fn": "Eps", "verdict": "same"}]))
        fc.write_doc(doc, text)
        with open(doc, encoding="utf-8") as f:
            out = f.read()
        self.assertNotIn("old", out)
        self.assertIn("kept\n", out)
        self.assertIn("kept too", out)
        self.assertIn("| Eps | 0x1005 | 2 / 5 | wrong | walking | same 1 | half built |", out)
        self.assertIn("| matches the original | 2 | 4 | 10 |", out)

    def test_the_committed_systems_file_reads(self):
        doc, systems = fc.load_systems(fc.SYSTEMS)
        self.assertIn("main", doc["groups"])
        self.assertGreater(len(systems), 50)


if __name__ == "__main__":
    unittest.main()
