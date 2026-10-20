"""Tests for tools/capture/fight_stats.py: plugin output written by hand, no game data."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import fight_stats as fs  # noqa: E402
import observe as ob  # noqa: E402


def output(rows, events):
    """rows: [(n, hour, [(id, x, in_combat, target, anim, dead, health)])]."""
    lines = [{"kind": "observing", "step": 1, "label": "f", "every": 1, "groups": ["nearby"],
              "fields": ob.fields_doc([], "fight", events=["OnHit", "OnDeath"])}]
    for n, hour, actors in rows:
        subj = [[f"{a}|00000001|{x}|0|0|0|{c}|{t}|13|{anim}|0|0|{dead}|{hp}|00000002|0"]
                for a, x, c, t, anim, dead, hp in actors]
        lines.append({"kind": "sample", "step": 1, "n": n, "header": [10.0, hour, 30.0, 0, 0, 0, 0, "00000000", 0],
                      "groups": {"nearby": subj}, "errors": 0, "us": 1})
    for e in events:
        lines.append({"kind": "event", "step": 1, **e})
    return "\n".join(json.dumps(x) for x in lines) + "\n"


class FightStats(unittest.TestCase):
    def test_shots_hits_damage_and_deaths(self):
        rows = [
            (0, 6.0, [("0000000A", 0, 1, "0000000B", -1, 0, 100), ("0000000B", 900, 1, "0000000A", -1, 0, 50)]),
            (1, 6.001, [("0000000A", 0, 1, "0000000B", 2, 0, 100), ("0000000B", 900, 1, "0000000A", -1, 0, 50)]),
            (2, 6.002, [("0000000A", 0, 1, "0000000B", 3, 0, 100), ("0000000B", 900, 1, "0000000A", -1, 0, 30)]),
            (3, 6.003, [("0000000A", 0, 1, "0000000B", 4, 0, 100), ("0000000B", 900, 1, "0000000A", -1, 0, 30)]),
            (4, 6.004, [("0000000A", 0, 1, "0000000B", 2, 0, 100), ("0000000B", 900, 1, "0000000A", -1, 0, 30)]),
            (5, 6.005, [("0000000A", 0, 0, "00000000", -1, 0, 100), ("0000000B", 900, 0, "00000000", -1, 1, 0)]),
        ]
        events = [{"n": 1, "name": "OnHit", "value": "0000000B|0000000A|10|6.001|50|0|0"},
                  {"n": 4, "name": "OnHit", "value": "0000000B|0000000A|10|6.004|30|0|0"},
                  {"n": 4, "name": "OnHit", "value": "0000000B|0|10|6.004|30|0|0"}]
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "o.jsonl")
            with open(p, "w", encoding="utf-8") as f:
                f.write(output(rows, events))
            res = fs.stats(*fs.read(p))
        a = res["fighters"]["0000000A"]
        self.assertEqual((a["shots"], a["hits"], a["damage"]), (2, 2, 50.0), "2 then 3 is one shot; the later 2 another")
        self.assertEqual(a["shot_range"]["median"], 900.0)
        self.assertEqual(a["damage_per_hit"]["max"], 30.0, "the killing hit counts what was left")
        self.assertEqual(res["fighters"]["0000000B"]["died"], round(0.005 * 3600 / 30, 1))
        self.assertEqual(res["summary"]["hits_per_shot"], 1.0, "a hit with no attacker is left out")

    def test_weapon_wear_split_by_cause(self):
        doc = ob.fields_doc([], "fight", extra_parts=[[("weapon_health", "num", "GetEquippedCurrentHealth 5")]],
                            events=["OnHit", "OnDeath"])
        lines = [{"kind": "observing", "step": 1, "label": "f", "every": 1, "groups": ["nearby"], "fields": doc}]
        # (n, anim, health, weapon health): a burst wears it 1.0, a hit on it 2.5 later.
        rows = [(0, -1, 100, 110.0), (1, 2, 100, 110.0), (2, 2, 100, 109.0), (3, -1, 100, 109.0),
                (4, -1, 90, 106.5), (5, -1, 90, 106.5)]
        for n, anim, hp, wh in rows:
            subj = [[f"0000000A|00000001|0|0|0|0|1|0000000B|13|{anim}|0|0|0|{hp}|00000002|0", f"{wh}"]]
            lines.append({"kind": "sample", "step": 1, "n": n, "header": [10.0, 6.0 + n / 1000, 30.0, 0, 0, 0, 0, "00000000", 0],
                          "groups": {"nearby": subj}, "errors": 0, "us": 1})
        lines.append({"kind": "event", "step": 1, "n": 3, "name": "OnHit", "value": "0000000A|0000000B|10|6.003|100|0|0"})
        with tempfile.TemporaryDirectory() as d:
            p = os.path.join(d, "o.jsonl")
            with open(p, "w", encoding="utf-8") as f:
                f.write("\n".join(json.dumps(x) for x in lines) + "\n")
            res = fs.stats(*fs.read(p))
        w = res["fighters"]["0000000A"]["weapon_wear"]
        self.assertEqual((w["firing"], w["firing_falls"], w["per_shot"]), (1.0, 1, 1.0))
        self.assertEqual((w["hit"], w["hit_falls"]["n"], w["hit_damage"]["median"], w["other"]), (2.5, 1, 10.0, 0))


if __name__ == "__main__":
    unittest.main()
