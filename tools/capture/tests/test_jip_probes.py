"""Tests for tools/capture/jip_probes.py: the job, collection and comparison, no game."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import jip_probes as jp  # noqa: E402


class Job(unittest.TestCase):
    def test_every_probe_is_a_function_script_query(self):
        job = jp.job()
        self.assertEqual(job["format"], "fnv.capture.steps/1")
        self.assertEqual(len(job["steps"]), len(jp.PROBES))
        ids = [s["label"] for s in job["steps"]]
        self.assertEqual(len(ids), len(set(ids)), "probe ids are unique")
        for s in job["steps"]:
            q = s["query"]
            self.assertIn("Begin Function { }", q)
            self.assertTrue(q.endswith("End\n"))
            self.assertLess(q.index("Begin Function"), q.index("SetFunctionValue"))

    def test_ref_variables_come_before_begin(self):
        q = next(s["query"] for s in jp.job()["steps"] if s["label"] == "formfrommod.player")
        self.assertTrue(q.startswith("ref rF\nBegin Function { }\n"))

    def test_form_set(self):
        job = jp.job("jip-forms")
        self.assertEqual(job["name"], "jip_forms")
        self.assertEqual(len(job["steps"]), len(jp.FORM_PROBES))
        q = next(s["query"] for s in job["steps"] if s["label"] == "reftostring.base")
        self.assertTrue(q.startswith('ref rF\nBegin Function { }\nlet rF := GetFormFromMod "FalloutNV.esm" "104C0C"\n'))
        ids = [pid for probes in jp.SETS.values() for pid, *_ in probes]
        self.assertEqual(len(ids), len(set(ids)), "probe ids are unique across sets")

    def test_store_probes_clean_up(self):
        for pid, body, _, _, _ in jp.STORE_PROBES:
            for verb, undo in (("AuxVarSet", "AuxVarErase"), ("RefMapSet", "RefMapDestroy")):
                if verb in body:
                    self.assertIn(undo, body, pid)

    def test_event_set_undoes_its_damage(self):
        ids = [pid for pid, *_ in jp.EVENT_PROBES]
        self.assertLess(ids.index("ev.damage"), ids.index("ev.amount"))
        self.assertEqual(ids[-1], "ev.cleanup")
        self.assertIn("RestoreAV Health 1", jp.EVENT_PROBES[-1][1])

    def test_hit_set_undoes_its_health_boost(self):
        ids = [pid for pid, *_ in jp.HIT_PROBES]
        self.assertEqual(ids[0], "hit.boost")
        self.assertLess(ids.index("hit.boom"), ids.index("hit.damage.0"))
        self.assertIn("ModAV Health -1000", jp.HIT_PROBES[-1][1])
        for name, _ in jp.HIT_VALUES:
            self.assertIn(f'AuxVarErase "*nvfH.{name}"', jp.HIT_PROBES[-1][1])

    def test_only_getters(self):
        for pid, body, _, _, _ in [p for k, probes in jp.SETS.items() if k not in jp.WRITING_SETS for p in probes]:
            words = body.replace("(", " ").split()
            self.assertFalse(any(w.startswith(("Set", "Toggle", "Clear", "Write")) and w != "SetFunctionValue" for w in words), pid)


class Collect(unittest.TestCase):
    def test_output_to_rows_and_compare(self):
        lines = [{"kind": "start"},
                 {"kind": "result", "label": "fsin.90", "field": "value", "value": 1.0},
                 {"kind": "result", "label": "modname.0", "field": "value", "value": "FalloutNV"},
                 {"kind": "result", "label": "av.strength", "field": "value", "value": 5.0},
                 {"kind": "result", "label": "fsqrt.16", "field": "value", "error": "did not compile"},
                 {"kind": "end"}]
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "out.jsonl")
            with open(p, "w", encoding="utf-8") as f:
                f.write("\n".join(json.dumps(x) for x in lines) + "\n")
            doc = jp.collect(p)
            self.assertEqual(jp.collect(p, "jip-forms")["set"], "jip-forms")
        self.assertEqual(doc["rows"][:2], [{"id": "fsin.90", "value": "1"}, {"id": "modname.0", "value": "FalloutNV"}])
        self.assertEqual(doc["errors"], [{"id": "fsqrt.16", "error": "did not compile"}])
        res = jp.compare(doc)
        self.assertEqual(res["matched"], 2)
        self.assertIn("fsqrt.16", res["missing"])
        self.assertIn({"id": "av.strength", "got": "5"}, res["observe"])


if __name__ == "__main__":
    unittest.main()
