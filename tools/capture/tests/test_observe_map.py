"""Tests for tools/capture/observe_map.py and the observer's field sets and scene actions.
No game or game data: plugin output is written by hand."""
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_steps as cs  # noqa: E402
import observe as ob  # noqa: E402
import observe_map as om  # noqa: E402

EDIDS = {"bob": 0x00001001, "ann": 0x00001002, "knife": 0x00004334, "ratbase": 0x00009001}
CTX = (cs.MASTERS, EDIDS)


def output(label, fields_set, samples, ts=30):
    """Plugin output lines: the mod index, one observation of `samples` [(game hour,
    [actor parts...])] with the nearby group only."""
    f = ob.fields_doc([], fields_set)
    lines = [{"kind": "result", "label": cs.MODINDEX_LABEL, "field": "FalloutNV.esm", "value": 0},
             {"kind": "observing", "step": 0, "label": label, "fields": f, "every": 1, "groups": ["nearby"]}]
    for n, (hour, subjects, player) in enumerate(samples):
        head = [1.0 + hour / 24, hour, ts, player[0], player[1], 0.0, 0.0, "00000000", player[2]]
        lines.append({"kind": "sample", "step": 0, "n": n, "header": head, "groups": {"nearby": subjects}, "errors": 0})
    lines.append({"kind": "observed", "step": 0, "label": label, "samples": len(samples), "reason": "until"})
    return [json.dumps(x) for x in lines]


def full(aid, x, y, ptype=36, proc=45, ptgt="00000000", talking=0, dtgt="00000000", sitting=0, pkg="00002001"):
    p0 = "|".join([aid, "00003001", "0000AAAA", str(x), str(y), "0", "0", pkg, str(ptype), str(proc), ptgt])
    p1 = "|".join(["0", "00000000", "0", "0", "0", str(sitting), "0", "0", str(talking), "0", "0", "0", "0", "0", "0", "0",
                   "100", "1", "0", "0"])
    p2 = "|".join([dtgt, "-1", "00000000", "0", "0", "0"])
    return [p0, p1, p2]


def fight(aid, x, y, target="00000000", anim=-1, talking=0, health=100, dead=0, weapon="00000000", creature=0,
          in_combat=1, proc=13, base="00003001"):
    return ["|".join([aid, base, str(x), str(y), "0", "0", str(in_combat), target, str(proc), str(anim), str(talking),
                      "0", str(dead), str(health), weapon, str(creature)])]


class FieldSets(unittest.TestCase):
    def campaign(self, **s):
        session = {"name": "s1", "hours": 0.1}
        session.update(s)
        return {"format": ob.CAMPAIGN_FORMAT, "name": "c", "save": "x", "sessions": [session]}

    def test_fight_set_has_one_part_and_tracked_full_set_two(self):
        (_, (_, job)), = ob.campaign_jobs(self.campaign(fields="fight", every=1, track=["Bob"]), EDIDS).items()
        o = next(s["observe"] for s in job["steps"] if "observe" in s)
        self.assertEqual(o["fields"]["set"], "fight")
        self.assertEqual([len(g["probe"]) for g in o["groups"]], [1, 1])
        self.assertIn("GetAnimAction", o["groups"][0]["probe"][0])
        (_, (_, job)), = ob.campaign_jobs(self.campaign(track=["Bob"]), EDIDS).items()
        o = next(s["observe"] for s in job["steps"] if "observe" in s)
        self.assertEqual([len(g["probe"]) for g in o["groups"]], [3, ob.TRACKED_PARTS],
                         "the third part (animation, dialogue target) only for loaded actors")
        self.assertIn("GetDialogueTarget", o["groups"][0]["probe"][2])

    def test_spawned_actors_are_armed_and_set_on_a_target(self):
        c = self.campaign(place=[{"base": "RatBase", "arm": "Knife", "attack": "player"}])
        (_, (_, job)), = ob.campaign_jobs(c, EDIDS).items()
        texts = [s["script"] for s in job["steps"] if "script" in s]
        scan = next(t for t in texts if "GetSourceModIndex" in t)
        for line in ("rA.RemoveAllItems", "rA.AddItem rF1 1", "rA.EquipItem rF1", "rA.StartCombat player",
                     f"if rA.GetSourceModIndex == {cs.CREATED_MOD_INDEX}"):
            self.assertIn(line, scan)
        self.assertNotIn(":= player.PlaceAtMe", "".join(texts), "PlaceAtMe's result does not compile in game")

    def test_a_spawned_gunman_gets_ammunition_before_it_equips(self):
        c = self.campaign(place=[{"base": "RatBase", "arm": "Knife", "ammo": "Bob", "attack": "player"}])
        (_, (_, job)), = ob.campaign_jobs(c, EDIDS).items()
        scan = next(s["script"] for s in job["steps"] if "script" in s and "GetSourceModIndex" in s["script"])
        self.assertIn(f"rA.AddItem rF2 {ob.PLACE_AMMO}", scan)
        self.assertLess(scan.index(f"rA.AddItem rF2 {ob.PLACE_AMMO}"), scan.index("rA.EquipItem rF1"))
        with self.assertRaises(ob.InputError):
            ob.campaign_jobs(self.campaign(place=[{"base": "RatBase", "ammo": "Bob"}]), EDIDS)

    def test_bad_field_set_is_refused(self):
        with self.assertRaises(ob.InputError):
            ob.campaign_jobs(self.campaign(fields="fast"), EDIDS)

    def test_place_and_do_steps_come_after_arrival(self):
        c = self.campaign(go={"coc": "Bob"}, place=[{"base": "RatBase", "count": 2, "distance": 700, "direction": 1}],
                          do=["killall", ["{0}.AddItem {1} 1", "Bob", "Knife"]])
        (_, (_, job)), = ob.campaign_jobs(c, EDIDS).items()
        texts = [s.get("script") or s.get("console") for s in job["steps"] if "script" in s or "console" in s]
        place = next(i for i, t in enumerate(texts) if "PlaceAtMe" in t)
        self.assertIn("player.PlaceAtMe rF1 2 700 1", texts[place])
        self.assertEqual(texts[place + 1], "killall")
        self.assertIn("rF1.AddItem rF2 1", texts[place + 2])
        with self.assertRaises(ob.InputError):
            ob.campaign_jobs(self.campaign(place=[{"base": "RatBase", "angle": 3}]), EDIDS)


class Map(unittest.TestCase):
    def test_conversation_between_two_npcs(self):
        # Bob walks up to Ann (dialogue package, DIALOGUE_ACTIVATE), both talk, then part.
        dlg, act, talk = 34, ob.PROCEDURES and om.PROC["DIALOGUE_ACTIVATE"], om.PROC["DIALOGUE"]
        samples = []
        for k in range(10):
            hour = 10 + k / 60
            if k < 2:
                subs = [full("00001001", 0, 0), full("00001002", 500, 0)]
            elif k < 4:
                subs = [full("00001001", 100 * k, 0, dlg, act, "00001002"), full("00001002", 500, 0)]
            elif k < 8:
                subs = [full("00001001", 400, 0, dlg, talk, "00001002", talking=k % 2),
                        full("00001002", 500, 0, dlg, talk, "00001001", talking=(k + 1) % 2)]
            else:
                subs = [full("00001001", 400, 0), full("00001002", 500, 0)]
            samples.append((hour, subs, (0, 0, 0)))
        m = om.build([("a.jsonl", output("town", "full", samples))])
        c = m["conversations"]
        self.assertEqual(c["count"], 1)
        conv = c["list"][0]
        self.assertEqual((conv["a"], conv["b"], conv["starter"]), ("0x00001001", "0x00001002", "0x00001001"))
        self.assertEqual(conv["start_distance"], 300.0)
        self.assertEqual(conv["talk_distance"], 100.0)
        self.assertAlmostEqual(conv["minutes"], 5.0, places=3)
        self.assertGreaterEqual(conv["lines"], 3)
        self.assertEqual(m["graph"]["edge_counts"]["talks_to"], 2)

    def test_dialogue_package_on_a_marker_is_no_conversation(self):
        # A dialogue package or dialogue target naming a stool or a marker (not a sampled
        # actor) is no conversation (the original's Novac and Gomorrah records).
        dlg, talk = 34, om.PROC["DIALOGUE"]
        samples = [(10 + k / 60, [full("00001001", 0, 0, dlg, talk, "0000B0B0", talking=1, dtgt="0000B0B1"),
                                  full("00001002", 500, 0)], (0, 0, 0)) for k in range(4)]
        m = om.build([("a.jsonl", output("town", "full", samples))])
        self.assertEqual(m["conversations"]["count"], 0)

    def test_combat_lines_gaps_and_triggers(self):
        # One fighter in combat speaks twice: after a hit, then 10 real s later.
        samples = []
        for k in range(40):
            hour = 12 + k * (0.5 / 3600) * 30  # 0.5 real s per sample at TimeScale 30
            health = 100 if k < 4 else 80
            talking = 1 if k in (5, 6, 25, 26) else 0
            samples.append((hour, [fight("00001001", 0, 0, "00001002", talking=talking, health=health),
                                   fight("00001002", 300, 0, "00001001")], (2000, 0, 0)))
        m = om.build([("a.jsonl", output("fight", "fight", samples))])
        sp = m["speech"]
        self.assertEqual(sp["combat_lines"], 2)
        self.assertEqual(sp["gap_seconds"]["median"], 10.0)
        self.assertEqual(sp["fighters"][0]["triggers"], {"hit": 1, "other": 1})
        self.assertEqual(sp["speaking_at_once"]["fight"]["1"], round(4 * 0.5 / (39 * 0.5), 3))

    def test_melee_swing_distance_and_back_off(self):
        # A dog runs in, stops at 60, swings; steps back to 120 between swings; swings again.
        xs = [400, 300, 200, 100, 60, 60, 60, 60, 60, 80, 120, 120, 90, 60, 60, 60, 60]
        anims = [-1, -1, -1, -1, -1, 2, 2, 4, -1, -1, -1, -1, -1, -1, 2, 2, -1]
        samples = []
        for k, (x, a) in enumerate(zip(xs, anims)):
            hour = 12 + k * (0.1 / 3600) * 30  # 0.1 real s per sample
            samples.append((hour, [fight("00001001", x, 0, "00000014", anim=a, creature=1, base="00009001")], (0, 0, 1)))
        m = om.build([("a.jsonl", output("dog", "fight", samples))], {0x00009001: "RatBase"})
        (cls, v), = m["melee"]["classes"].items()
        self.assertEqual(cls, "creature RatBase")
        self.assertEqual(v["swings"], 2)
        self.assertEqual(v["distance_at_swing"]["min"], 60.0)
        self.assertEqual(v["back_off_between"]["max"], 60.0, "from 60 at the first swing's end out to 120")
        self.assertEqual(m["melee"]["closest_approach"]["0x00001001"]["closest"], 60.0)

    def test_encounter_sides_deaths_and_join(self):
        samples = []
        for k in range(6):
            hour = 12 + k / 600
            a = fight("00001001", 0, 0, "00001002")
            b = fight("00001002", 500, 0, "00001001", dead=1 if k >= 4 else 0, in_combat=0 if k >= 4 else 1)
            c = fight("00001003", 900, 0, "00001001" if k >= 2 else "00000000", in_combat=1 if k >= 2 else 0)
            samples.append((hour, [a, b, c], (5000, 0, 0)))
        m = om.build([("a.jsonl", output("brawl", "fight", samples))])
        enc = m["encounters"]["brawl"]
        self.assertIsNotNone(enc["0x00001002"]["died"])
        self.assertNotEqual(enc["0x00001001"]["side"], enc["0x00001002"]["side"])
        self.assertEqual(enc["0x00001002"]["side"], enc["0x00001003"]["side"])

    def test_package_compared_with_its_record(self):
        samples = [(h, [full("00001001", 0, 0, 36, 45)], (0, 0, 0)) for h in (21.5, 22.5, 23.5)]
        records = {"packages": [{"form": "0x00002001", "type": "Sandbox", "runtime_type_code": 36,
                                 "schedule": {"hour": 22, "duration_hours": 8}, "location": {"kind": "In cell"}}]}
        m = om.build([("a.jsonl", output("night", "full", samples))], records=records)
        p = m["packages"]["0x00002001"]
        self.assertTrue(p["type_matches_record"])
        self.assertAlmostEqual(p["share_outside_schedule"], 0.5, places=2)
        self.assertIn("Conversations between NPCs", om.text(m))


if __name__ == "__main__":
    unittest.main()
