"""Tests for tools/capture/jobs/make_owner_decision_jobs.py and the observer campaign
owner_decisions.campaign.json (docs/spec-notes/owner-decision-captures.md). No game or game
data is used: the jobs hold only script text and FormIDs."""
import glob
import json
import os
import re
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
CAPTURE = os.path.dirname(HERE)
JOBS = os.path.join(CAPTURE, "jobs")
sys.path.insert(0, CAPTURE)
sys.path.insert(0, JOBS)

import make_owner_decision_jobs as od  # noqa: E402
import observe  # noqa: E402

SCRIPT_KEYS = ("script", "query", "remember", "until")
CAMPAIGN = os.path.join(CAPTURE, "observe", "owner_decisions.campaign.json")


def scripts(steps):
    for s in steps:
        for k in SCRIPT_KEYS:
            if k in s:
                yield k, s[k]
        if "observe" in s:
            o = s["observe"]
            yield from (("header", t) for t in o.get("header", []))
            for g in o.get("groups", []):
                yield "count", g["count"]
                yield from (("probe", t) for t in g["probe"])
            for k in ("until", "drive", "drive_end"):
                if k in o:
                    yield k, o[k]
            for h in o.get("hooks", []):
                yield "when", h["when"]
                yield "then", h["then"]


class Rules:
    """docs/spec-notes/xnvse-capture.md section 5, checked on every script of a job."""

    def check_job(self, j):
        for kind, text in scripts(j["steps"]):
            lines = text.rstrip("\n").split("\n")
            self.assertIn("Begin Function { }", lines, text)
            begin = lines.index("Begin Function { }")
            self.assertEqual(lines[-1], "End", text)
            for line in lines[begin + 1:]:
                self.assertNotRegex(line, r"^(ref|float|int|short|long|string_var) ", f"declaration inside the block: {text}")
            for line in lines[:begin]:
                self.assertRegex(line, r"^(ref|float|int) \w+$", text)
            self.assertNotRegex(text, r"0x[0-9A-Fa-f]", f"hex literal in script text: {text}")
            for m in re.finditer(r"BuildRef (\S+) (\S+)", text):
                self.assertRegex(m.group(1), r"^fM\d+$")
                self.assertRegex(m.group(2), r"^\d+$", "BuildRef takes a decimal local FormID")
            self.assertNotRegex(text, r"(?i)(set|let)\s+TimeScale", "TimeScale is left alone (reading it is fine)")
            if kind in ("script", "then", "drive_end"):
                self.assertTrue(text.endswith("SetFunctionValue 1\nEnd\n"), f"action script without SetFunctionValue 1: {text}")
            # Every name a script calls a function on is player, a ref variable or rA (the scan).
            for m in re.finditer(r"\b([A-Za-z_]\w*)\.\w+", re.sub(r'"[^"]*"', '""', text)):
                self.assertRegex(m.group(1), r"^(player|r[A-Z]\w*)$", f"an Editor ID in script text? {text}")
        for s in j["steps"]:
            if "console" in s:
                self.assertNotRegex(s["console"], r"(?i)TimeScale")


class OwnerDecisionJobs(Rules, unittest.TestCase):
    def setUp(self):
        self.jobs = od.all_jobs()

    def test_committed_jobs_match_the_generator(self):
        committed = sorted(os.path.basename(p) for p in glob.glob(os.path.join(JOBS, od.PREFIX + "*.steps.json")))
        self.assertEqual(committed, sorted(j["name"] + ".steps.json" for j in self.jobs),
                         "regenerate with: python tools/capture/jobs/make_owner_decision_jobs.py")
        for j in self.jobs:
            with open(os.path.join(JOBS, j["name"] + ".steps.json"), encoding="utf-8") as f:
                self.assertEqual(json.load(f), j, j["name"])

    def test_scripts_follow_the_runtime_compilers_rules(self):
        for j in self.jobs:
            with self.subTest(job=j["name"]):
                self.check_job(j)

    def test_god_mode_is_switched_back(self):
        for j in self.jobs:
            toggles = sum(1 for s in j["steps"] if s.get("console") == "tgm")
            self.assertEqual(toggles % 2, 0, j["name"])

    def test_every_job_moves_the_player_first(self):
        for j in self.jobs:
            first = next(s for s in j["steps"] if s.get("label") != od.cs.MODINDEX_LABEL and s.get("console") != "tgm")
            moves = first.get("console", "").startswith(("coc ", "cow ")) or "player.MoveTo" in first.get("script", "") \
                or "GameHour" in first.get("script", "")
            self.assertTrue(moves or j["name"] == "od_10_stray_markers", f"{j['name']} starts with {first}")

    def test_labels_are_unique_per_field(self):
        for j in self.jobs:
            seen = set()
            for s in j["steps"]:
                if "query" in s and s["label"] != od.cs.MODINDEX_LABEL:
                    key = (s["label"], s["field"])
                    self.assertNotIn(key, seen, j["name"])
                    seen.add(key)

    def test_screenshots_are_unique(self):
        for j in self.jobs:
            shots = [s["screenshot"] for s in j["steps"] if "screenshot" in s]
            self.assertEqual(len(shots), len(set(shots)), j["name"])
            for name in shots:
                self.assertRegex(name, r"^[A-Za-z0-9_]+$")

    def test_menus_are_closed_after_dialogue(self):
        for j in self.jobs:
            opened = False
            for s in j["steps"]:
                if "Activate player" in s.get("script", "") or "MenuMode 1009" in s.get("until", ""):
                    opened = True
                if s.get("console") == "CloseAllMenus":
                    opened = False
            self.assertFalse(opened and "dlg" in j["name"], f"{j['name']} leaves a menu open")

    def test_record_joins_values(self):
        q = od.record([("num", "{0}.GetPos X"), ("ref", "{0}.GetCurrentPackage")], "l", "f", [od.RINGO])["query"]
        self.assertIn("ref rV1", q.split("Begin Function")[0])
        self.assertIn('SetFunctionValue (($ fV0) + " " + (GetFormIDString rV1))', q)
        self.assertIn(f"BuildRef fM1 {int(od.RINGO, 16)}", q)


class OwnerDecisionCampaign(Rules, unittest.TestCase):
    def setUp(self):
        with open(CAMPAIGN, encoding="utf-8") as f:
            self.campaign = json.load(f)

    def test_jobs_are_written_without_game_data(self):
        jobs = observe.campaign_jobs(self.campaign)  # forms by FormID only: no --data needed
        self.assertEqual(len(jobs), len(self.campaign["sessions"]))
        for name, (_, j) in jobs.items():
            with self.subTest(job=name):
                self.check_job(j)

    def test_god_mode_only_in_the_first_session(self):
        lines = [s.get("console_first", []) for s in self.campaign["sessions"]]
        self.assertIn("tgm", lines[0])
        self.assertFalse(any("tgm" in ls for ls in lines[1:]))
        self.assertNotIn("timescale", json.dumps(self.campaign).lower().replace('"timescale"', ""))


if __name__ == "__main__":
    unittest.main()
