"""Tests for tools/capture/damage_parity.py: plugin output and engine log lines written by
hand, no game data."""
import json
import os
import sys
import tempfile
import unittest
from pathlib import Path

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, os.path.join(os.path.dirname(os.path.dirname(HERE)), "smoke"))

import damage_parity as dp  # noqa: E402
import observe as ob  # noqa: E402


def small():
    """A matrix of two weapons and two records (a person with two aims, a creature with one)."""
    m = dp.load_matrix()
    m["weapons"] = m["weapons"][:2]
    m["targets"] = [m["targets"][0], next(t for t in m["targets"] if len(t["aims"]) == 1)]
    m["hits"] = 2
    return m


def output(m, session, samples):
    """Plugin output of one session. samples: [[(id, base, health, dt, dr, level, cycle,
    hloc, hit_hp, crit)]], one list per game loop."""
    c = dp.campaign(m)
    parts, header = ob.session_extras(ob.merged(c, c["sessions"][0]))
    doc = ob.fields_doc([], "fight", extra_parts=parts, extra_header=header)
    lines = [{"kind": "observing", "step": 1, "label": f"obs_{m['name']}_01_{session}", "every": 1, "groups": ["nearby"], "fields": doc}]
    head = [10.0, 12.0, 30.0, 0, 0, 0, 0, "00000000", 0] + [1, 50, 50, 50, 5, 5, 100, "0008ED03", 0]
    for n, actors in enumerate(samples):
        subj = [[f"{a}|{base}|0|300|0|0|0|00000000|0|0|0|0|0|{hp}|00000000|0", f"{dt}|{dr}|{lvl}|{cycle}|{hloc}|{hit}|0|{crit}|0|0"]
                for a, base, hp, dt, dr, lvl, cycle, hloc, hit, crit in actors]
        lines.append({"kind": "sample", "step": 1, "n": n, "header": head, "groups": {"nearby": subj}, "errors": 0, "us": 1})
    return [json.dumps(x) for x in lines]


LOG = """\
\x1b[2m2026-11-21T00:49:34Z\x1b[0m \x1b[32m INFO\x1b[0m nvf_play::script: script: test aid calls PlaceAtMe on 0x00000014 = FF000000
INFO nvf_play::script: test aid: 0xFF000000 DamageThreshold damaged by 0: now 2.0
INFO nvf_play::script: test aid: 0xFF000000 DamageResist damaged by 0: now 0.0
INFO nvf_play::combat: combat: hit 0xFF000000 on Torso for 99.0 (DT 2), health 4901/5000
INFO nvf_play::script: script: test aid calls GetLevel on 0xFF000000 = 1
INFO nvf_play::combat: combat: hit 0xFF000000 on Torso for 17.9 (DT 2), health 4982/5000
INFO nvf_play::combat: combat: hit 0xFF000000 on Torso for 37.9 (DT 2, critical), health 4944/5000
INFO nvf_play::script: script: test aid calls GetDead on 0xFF000000 = 0
INFO nvf_play::combat: combat: hit 0xFF000000 on Head for 35.8 (DT 2), health 4908/5000
INFO nvf_play::script: script: test aid calls Disable on 0xFF000000 = 0
INFO nvf_play::combat: combat: hit 0xFF000000 on Head for 35.8 (DT 2), health 4872/5000
INFO nvf_play::script: script: test aid calls PlaceAtMe on 0x00000014 = FF000001
INFO nvf_play::script: test aid: 0xFF000001 DamageThreshold damaged by 0: now 0.0
INFO nvf_play::script: script: test aid calls GetLevel on 0xFF000001 = 4
INFO nvf_play::combat: combat: hit 0xFF000001 on the body for 19.5 (DT 0), health 4980/5000
"""


class Matrix(unittest.TestCase):
    def test_the_committed_campaign_is_the_matrix_s(self):
        with open(dp.CAMPAIGN, encoding="utf-8") as f:
            self.assertEqual(json.load(f), dp.campaign(dp.load_matrix()),
                             "run `python tools/capture/damage_parity.py campaign` after changing the matrix")

    def test_the_campaign_builds_jobs(self):
        m = dp.load_matrix()
        c = dp.campaign(m)
        ob.check_campaign(c)
        jobs = ob.campaign_jobs(c)
        self.assertEqual(len(jobs), len(m["weapons"]))
        self.assertEqual([s["name"] for s in c["sessions"]], [w["key"] for w in m["weapons"]])
        # no console toggle: a second session of one game load would undo it
        self.assertFalse(any("tgm" in s["console_first"] for s in c["sessions"]))

    def test_each_session_fires_its_own_weapon_at_every_record(self):
        m = dp.load_matrix()
        for s, w in zip(dp.campaign(m)["sessions"], m["weapons"]):
            scripts = [t["script"] for t in s["timed"]]
            self.assertEqual([x["names"] for x in scripts if any("FireWeapon" in line for line in x["lines"])], [[w["weapon"]]])
            placing = next(x for x in scripts if any("PlaceAtMe" in line for line in x["lines"]))
            self.assertEqual(placing["names"], [t["base"] for t in m["targets"]])
            self.assertEqual(sum("PlaceAtMe" in line for line in placing["lines"]), len(m["targets"]))

    def test_a_bad_matrix_is_refused(self):
        m = dp.load_matrix()
        m["targets"][0]["aims"] = []
        with tempfile.TemporaryDirectory() as d:
            path = os.path.join(d, "m.json")
            with open(path, "w", encoding="utf-8") as f:
                json.dump(m, f)
            with self.assertRaises(dp.InputError):
                dp.load_matrix(path)


class Original(unittest.TestCase):
    def test_a_fall_in_health_is_a_hit_with_its_aim(self):
        m = small()
        person, creature = m["targets"]
        a, b = "FF000001", "FF000002"
        pb, cb = person["base"][2:], creature["base"][2:]
        samples = [
            [(a, pb, 5000, 2, 0, 1, 1, -1, 0, 0)],
            [(a, pb, 5000, 2, 0, 1, 3, -1, 0, 0)],            # shot 0 left (cycle 2 -> 3)
            [(a, pb, 4982.5, 2, 0, 1, 3, 0, 17.5, 0)],        # and landed: torso
            [(a, pb, 4982.5, 2, 0, 1, 4, 0, 17.5, 0)],
            [(a, pb, 4965, 2, 0, 1, 4, 0, 17.5, 0)],
            [(a, pb, 4965, 2, 0, 1, 5, 0, 17.5, 0)],          # shot 2: the second aim
            [(a, pb, 4930, 2, 0, 1, 5, 1, 35, 0)],
            [(b, cb, 5000, 0, 10, 4, 1, -1, 0, 0)],
            [(b, cb, 4970, 0, 10, 4, 3, 0, 30, 1)],           # a critical
        ]
        doc = dp.original_rows(m, [("x.jsonl", output(m, m["weapons"][1]["key"], samples))])
        rows = doc["rows"]
        self.assertEqual([(r["target"], r["aim"], r["shot"], r["damage"], r["health_lost"], r["location"], r["critical"]) for r in rows],
                         [(person["key"], "torso", 0, 17.5, 17.5, 0.0, False), (person["key"], "torso", 1, 17.5, 17.5, 0.0, False),
                          (person["key"], "head", 2, 35.0, 35.0, 1.0, False), (creature["key"], "body", 0, 30.0, 30.0, 0.0, True)])
        self.assertTrue(all(r["weapon"] == m["weapons"][1]["key"] for r in rows))
        self.assertEqual((rows[0]["dt"], rows[3]["dr"], rows[3]["level"]), (2.0, 10.0, 4.0))
        self.assertEqual(doc["setups"][0]["difficulty"], 1)
        self.assertEqual(doc["setups"][0]["p_weapon_health"], 100)

    def test_other_actors_and_other_campaigns_are_left_out(self):
        m = small()
        stray = [[("FF000009", "00000007", 100, 0, 0, 1, 0, -1, 0, 0)], [("FF000009", "00000007", 50, 0, 0, 1, 0, 0, 50, 0)]]
        self.assertEqual(dp.original_rows(m, [("x", output(m, m["weapons"][0]["key"], stray))])["rows"], [])
        self.assertEqual(dp.original_rows(m, [("x", output(m, "elsewhere", stray))])["rows"], [])


class NVForever(unittest.TestCase):
    def test_the_plan_shoots_every_record(self):
        m = dp.load_matrix()
        env, args, seconds = dp.nvf_plan(m, m["weapons"][0])
        shots = sum(dp.shots_of(m, t) for t in m["targets"])
        control = env["NVF_PLAY_TEST_CONTROL"].split(",")
        self.assertEqual(sum(c.endswith(":fire") for c in control), 2 * shots)  # pressed and let go
        script = env["NVF_PLAY_TEST_SCRIPT"].split(",")
        self.assertEqual(sum(":PlaceAtMe:" in s for s in script), len(m["targets"]))
        self.assertIn(f":call:created:{m['targets'][-1]['base']}:Disable", script[-1])
        self.assertEqual(sum(":kill:" in s for s in script), len(m["place"]["nvf_kill"]))
        times = [float(s.split(":")[0]) for s in script if ":kill:" not in s][len(m["shooter"]["values"]) + 1:]
        self.assertEqual(times, sorted(times))
        self.assertGreater(seconds, times[-1])
        self.assertIn("--play", args)
        self.assertEqual(env["NVF_PLAY_TEST_EQUIP"], "000E3778")
        look = [x.split(":") for x in env["NVF_PLAY_TEST_LOOK"].split(",")]
        self.assertEqual(len(look), shots)
        # the wastelander: the torso well below the eye, the head about level with it
        self.assertLess(float(look[0][2]), -3)
        self.assertLess(abs(float(look[m["hits"]][2])), 2)
        self.assertEqual({x[1] for x in look}, {"0"})

    def test_the_scenarios_file_is_one_the_smoke_runner_loads(self):
        from smoke import scenarios
        m = dp.load_matrix()
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / "damage.toml"
            path.write_text(dp.scenarios_toml(m), encoding="utf-8")
            suite = scenarios.load(path)
        names = [s.name for s in suite.scenarios]
        self.assertEqual(names, [dp.scenario_name(w) for w in m["weapons"]])

    def test_log_lines_become_rows(self):
        m = small()
        rows = dp.nvf_rows_of(m, "w", LOG.splitlines())
        self.assertEqual([(r["target"], r["aim"], r["damage"], r["location"], r["critical"], r["dt"], r["dr"], r["level"]) for r in rows],
                         [(m["targets"][0]["key"], "torso", 17.9, "Torso", False, 2.0, 0.0, 1.0),
                          (m["targets"][0]["key"], "torso", 37.9, "Torso", True, 2.0, 0.0, 1.0),
                          (m["targets"][0]["key"], "head", 35.8, "Head", False, 2.0, 0.0, 1.0),
                          (m["targets"][1]["key"], "body", 19.5, "the body", False, 0.0, None, 4.0)])
        self.assertEqual(rows[0]["health_before"], 4999.9)

    def test_the_logs_of_a_folder(self):
        m = small()
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(dp.InputError):
                dp.nvforever_rows(m, d)
            with open(os.path.join(d, dp.scenario_name(m["weapons"][1]) + ".log"), "w", encoding="utf-8") as f:
                f.write(LOG)
            doc = dp.nvforever_rows(m, d)
        self.assertEqual({r["weapon"] for r in doc["rows"]}, {m["weapons"][1]["key"]})
        self.assertEqual(doc["setups"][0]["rows"], 4)


class Compare(unittest.TestCase):
    def rows(self, side, items):
        return {"format": dp.ROWS_FORMAT, "side": side, "setups": [],
                "rows": [{"side": side, "weapon": w, "target": t, "aim": aim, "damage": dmg, "critical": crit, "sneak": False,
                          "explosion": False, "location": loc, "dt": dt, "dr": 0.0, "level": 1.0}
                         for w, t, aim, dmg, crit, loc, dt in items]}

    def test_match_differs_and_missing(self):
        m = small()
        w0, w1 = (w["key"] for w in m["weapons"])
        person, creature = (t["key"] for t in m["targets"])
        original = self.rows("original", [(w0, person, "torso", 17.52, False, 0.0, 2.0), (w0, person, "torso", 37.5, True, 0.0, 2.0),
                                          (w0, person, "head", 35.0, False, 1.0, 2.0), (w0, creature, "body", 12.0, False, 0.0, 0.0),
                                          (w1, person, "torso", 9.0, False, 0.0, 2.0)])
        nvforever = self.rows("nvforever", [(w0, person, "torso", 17.5, False, "Torso", 2.0), (w0, person, "head", 39.0, False, "Head", 4.0),
                                            (w0, creature, "body", 12.0, True, "the body", 0.0)])
        doc = dp.compare(m, original, nvforever)
        by = {(c["weapon"], c["target"], c["aim"]): c for c in doc["cells"]}
        self.assertEqual(by[(w0, person, "torso")]["status"], "match")       # the critical is left out of the figure
        self.assertEqual(by[(w0, person, "torso")]["original"]["critical"], 1)
        self.assertEqual(by[(w0, person, "head")]["status"], "differs")
        self.assertTrue(any("dt differs" in n for n in by[(w0, person, "head")]["notes"]))
        self.assertEqual(by[(w0, creature, "body")]["status"], "no nvforever")  # only a critical there
        self.assertEqual(by[(w1, person, "torso")]["status"], "no nvforever")
        self.assertEqual(by[(w1, creature, "body")]["status"], "not run")
        self.assertEqual(doc["counts"], {"match": 1, "differs": 1, "no nvforever": 2, "not run": 2})
        text = dp.compare_text(doc)
        self.assertIn("differs", text)
        self.assertNotIn("not run ", text.splitlines()[1])

    def test_the_command_line(self):
        m = small()
        w0 = m["weapons"][0]["key"]
        person = m["targets"][0]["key"]
        with tempfile.TemporaryDirectory() as d:
            a, b, out = (os.path.join(d, n) for n in ("a.json", "b.json", "c.json"))
            with open(a, "w") as f:
                json.dump(self.rows("original", [(w0, person, "torso", 17.5, False, 0.0, 2.0)]), f)
            with open(b, "w") as f:
                json.dump(self.rows("nvforever", [(w0, person, "torso", 20.0, False, "Torso", 2.0)]), f)
            self.assertEqual(dp.main(["compare", a, b, "-o", out]), 0)
            self.assertEqual(dp.main(["compare", a, b, "-o", out, "--fail-on-diff"]), 2)
            with open(out) as f:
                self.assertEqual(json.load(f)["format"], dp.COMPARE_FORMAT)
            self.assertEqual(dp.main(["compare", a, os.path.join(d, "missing.json")]), 1)


class Rule(unittest.TestCase):
    """The measured rule (docs/spec-notes/damage-armour.md) on rows of either side."""

    def rows(self, side, items):
        return {"format": dp.ROWS_FORMAT, "side": side, "setups": [],
                "rows": [{"side": side, "weapon": w, "target": "t", "aim": "torso", "damage": dmg, "critical": crit, "sneak": False,
                          "explosion": False, "location": loc, "dt": dt, "dr": dr} for w, dmg, crit, loc, dt, dr in items]}

    def test_the_original_s_rows_carry_the_dt_before_the_ammunition(self):
        m = dp.load_matrix()
        doc = dp.rule_check(m, self.rows("original", [
            ("pistol9mm", 8.0, False, 0.0, 4.0, 0.0),          # 12 - 4
            ("pistol9mm", 4.8, False, 1.0, 11.0, 0.0),         # the floor's 2.4, the head's x2
            ("pistol9mm", 2.4, False, 0.0, 21.0, 30.0),        # DR does not cut the floor
            ("huntrifle_hp", 7.8, False, 0.0, 25.0, 0.0),      # a fifth of 39, not of 68.25
            ("huntrifle_hp", 61.075, True, 0.0, 24.0, 30.0),   # (91 x 1.75 - 72) x 0.7
            ("huntshotgun", 16.0, True, 0.0, 5.0, 0.0),        # (7.5 + 10) x 1.2 - 5
            ("plasma", 9.275, False, 0.0, 24.0, 30.0),         # (35.25 - 22) x 0.7
            ("pistol9mm", 13.5, False, 1.0, 11.0, 0.0),        # the part's x2 before DT: not the rule
        ]))
        self.assertEqual((doc["fits"], len(doc["misfits"])), (7, 1))
        self.assertEqual(doc["misfits"][0]["rule"], 2.4)
        self.assertEqual(doc["parts"]["t / 1.0"], {"2": 1})

    def test_nvforever_s_rows_carry_the_hit_s_own_dt_rounded(self):
        m = dp.load_matrix()
        doc = dp.rule_check(m, self.rows("nvforever", [
            ("huntrifle_hp", 23.2, False, "Torso", 45.0, 0.0),  # 68.25 - 45: the DT is after the ammunition's x3
            ("magnum357", 9.0, False, "Torso", 10.0, 0.0),      # a DT of 10.5 printed as 10
            ("magnum357", 28.5, False, "Head", 10.0, 0.0),      # 19.5 x 2 - 10.5
        ]))
        self.assertEqual((doc["fits"], [x["damage"] for x in doc["misfits"]]), (2, [28.5]))

    def test_the_command_line(self):
        with tempfile.TemporaryDirectory() as d:
            a = os.path.join(d, "o.json")
            with open(a, "w") as f:
                json.dump(self.rows("original", [("pistol9mm", 13.5, False, 1.0, 11.0, 0.0)]), f)
            self.assertEqual(dp.main(["rule", a, "-o", os.path.join(d, "r.json")]), 0)
            self.assertEqual(dp.main(["rule", a, "-o", os.path.join(d, "r.json"), "--fail-on-misfit"]), 2)

    def test_every_weapon_has_its_figures(self):
        for w in dp.load_matrix()["weapons"]:
            self.assertGreater(w["figures"]["damage"], 0, w["key"])


if __name__ == "__main__":
    unittest.main()
