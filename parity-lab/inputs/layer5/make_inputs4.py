#!/usr/bin/env python3
"""Writes the visible scenario inputs of the fourth round of Layer 5 captures: the intro
quest VCG01 ("Ain't That a Kick in the Head") from the owner's new-game save `nvfnew`
(fnv.script.actions/1 with the round 4 additions, docs/spec-notes/layer5-captures.md
section 10) next to this file:

  python parity-lab/inputs/layer5/make_inputs4.py

Quests are named by Editor ID, other forms by FalloutNV.esm FormID (looked up read-only
with `fnvtool esm decode`, 2026-10-09):
- DocMitchellREF 0x00104C0F, GSDocMitchellHouseIntDoorREF 0x00103E61,
  VCG01VigorTesterREF 0x00104C13, VictorREF 0x00103DFE, GoodspringsCemeteryMapMarker
  0x001088F6; Pipboy 0x00015038;
- quests by Editor ID, including the pack plugins' VPreorderTribal (0x00172A1F),
  VPreorderCaravan (0x00172A1E), VPreorderClassic (0x001674D4) and VPreorderMercenary
  (0x00172A1D), records the pack plugins add under FalloutNV.esm's index.
Every `given` is a fact of nvfnew read by the round 4 trials (q5d_trial_open,
q5d_trial_drive1, q5d_trial_drive2) and checked again when each golden is built.

No input passes VCG01 stage 55: its result script calls `autosave`, which wrote
autosave.fos in trial q5d_trial_drive1 although the INI has bAllowScriptedAutosave=0;
captures must never save. `opening` ends at stage 50 and disables Doc Mitchell so that
his last two lines (whose result sets 55) cannot finish before the game quits.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
FRAME = 1 / 60

DOC, DOC_DOOR, TESTER, VICTOR, CEMETERY_MARKER = "0x00104C0F", "0x00103E61", "0x00104C13", "0x00103DFE", "0x001088F6"
PIPBOY = "0x00015038"
PACK_QUESTS = ["VPreorderTribal", "VPreorderCaravan", "VPreorderClassic", "VPreorderMercenary"]
VCG01_STAGES = [0, 1, 3, 5, 7, 8, 9, 10, 15, 17, 20, 25, 30, 35, 36, 37, 40, 45, 50, 55, 60, 65, 70, 75, 76, 78, 79, 80,
                85, 90, 95, 98, 100, 102, 105, 110, 115, 200]
VCG01_VARS = ["bChooseSex", "nButton", "bRunTimer", "bGiveTest", "fTimer", "bBeganTest", "nDocReaction",
              "nMostExtremeStat", "bLow", "fMaxDeviation", "fCurrentValue", "fAbsCurrentValue"]
TEST_SCORES = ["nBarter", "nBigGuns", "nEnergyWeapons", "nExplosives", "nLockpick", "nMedicine", "nMelee", "nRepair",
               "nScience", "nSmallGuns", "nSneak", "nSpeech", "nSurvival", "nUnarmed"]
TEST_RESULTS = ["nMaxStatValue", "nBestSpecialStat", "nSpecialBias", "nTopCombat", "nTopGateOpener", "nTopOverall",
                "nSkillsLeftToBeat", "nSneakBias", "bApplySneakBias", "nMaxValue", "bDoFinalCalculation"]
SKILLS = ["Barter", "EnergyWeapons", "Explosives", "Guns", "Lockpick", "Medicine", "MeleeWeapons", "Repair", "Science",
          "Sneak", "Speech", "Survival", "Unarmed"]
SPECIAL = ["Strength", "Perception", "Endurance", "Charisma", "Intelligence", "Agility", "Luck"]
# Menu codes seen in the trials (MenuMode <code>): 1001 message box, 1036 race menu
# (ShowRaceMenu), 1048 tag-skill menu (SetTagSkills), 1051 name menu (VCG01 stage 10),
# 1074 the vigor tester's SPECIAL menu (ShowLoveTesterMenuParams), 1084 trait menu
# (ShowTraitMenu).
MESSAGE, RACE, TAGS, NAME, SPECIAL_MENU, TRAITS = 1001, 1036, 1048, 1051, 1074, 1084
WAIT = 2400  # frames a wait may take: Doc's longest run of lines was about 15 s


def doc(title, given, actions):
    return {"format": "fnv.script.actions/1", "seed": 0, "frame_seconds": FRAME, "title": title, "save": "nvfnew",
            "given": given, "actions": actions}


def nvfnew_given(stopped=()):
    """The facts of nvfnew every round 4 input depends on [observed: q5_state_nvfnew and
    the round 4 trials]: a pack plugin's message box is open at load; VCG00 at stage 90
    with its countdown running (fTimer is not given: it was 3.0 or less at the start of
    the jobs, depending on how soon the box opened, so the inputs set it); VCG01 running
    with nothing done; VCG01Test and CGTutorial running untouched; the player's SPECIAL
    all 5. Which pack plugin's box is open at the start varies between sessions (Tribal
    Pack in six, Classic Pack in four of the round 4 sessions where it was identified),
    so the packs' item flags are not given and the number of boxes closed is not
    observed."""
    g = [{"kind": "menu", "menu": MESSAGE, "open": True},
         {"kind": "quest", "quest": "VCG00", "running": True, "completed": False, "stage": 90, "done": [0, 90],
          "not_done": [5, 85, 95, 100], "vars": {"bRunTimer": 1}},
         {"kind": "quest", "quest": "VCG01", "running": True, "completed": False, "stage": 0, "not_done": VCG01_STAGES,
          "objectives": [{"index": o, "displayed": False, "completed": False} for o in (10, 30, 40, 50)],
          "vars": {v: 0 for v in VCG01_VARS}},
         {"kind": "quest", "quest": "VCG01Test", "running": True, "completed": False,
          "vars": {v: 0 for v in TEST_SCORES + ["bDoFinalCalculation"]}},
         {"kind": "quest", "quest": "CGTutorial", "running": True, "stage": 0, "not_done": [2]},
         {"kind": "quest", "quest": "VDLCPackQuest", "running": True}]
    g += [{"kind": "quest", "quest": q, "running": True} for q in PACK_QUESTS]
    g += [{"kind": "actor_value", "of": "player", "av": s, "value": 5} for s in SPECIAL]
    g += [{"kind": "quest", "quest": q, "running": False, "completed": False, "stage": 0} for q in stopped]
    return g


def wait(quest, stage, label, stopwatch=False, max_frames=WAIT):
    a = {"op": "wait_quest", "quest": quest, "field": f"done.{stage}", "value": True, "max_frames": max_frames,
         "label": label}
    if stopwatch:
        a["stopwatch"] = True
    return a


def menu(code, label, max_frames=WAIT):
    return {"op": "wait_menu", "menu": code, "max_frames": max_frames, "label": label}


def opening():
    """From the load to VCG01 stage 50, every step as the game takes it by itself; the
    player's part is closing the menus (four pack message boxes, the name menu, the race
    menu). VCG00's countdown is held (fTimer 1000) while the boxes are closed and then
    restarted at 3 s together with the stopwatch, so every timed step is measured from
    one start."""
    v0, v1 = "VCG00", "VCG01"
    acts = [
        {"op": "set_quest_var", "quest": v0, "var": "fTimer", "value": 1000},
        {"op": "get_quest", "quest": v0, "fields": ["running", "stage", "var.fTimer"],
         "label": "VCG00 while the first box is open"},
        {"op": "dismiss_message_boxes", "max": 6, "wait_frames": 600},
        {"op": "get_quest", "quest": "VDLCPackQuest",
         "fields": ["var.bTribalItemsGiven", "var.bCaravanItemsGiven", "var.bClassicItemsGiven", "var.bMercenaryItemsGiven"],
         "label": "pack items given"},
        {"op": "get_quest", "quest": v1, "fields": ["stage", "done.0", "var.bRunTimer"], "label": "VCG01 after the boxes"},
        {"op": "set_quest_var", "quest": v0, "var": "fTimer", "value": 3, "stopwatch": True},
        wait(v0, 95, "VCG00 95", True),
        wait(v0, 100, "VCG00 100", True),
        wait(v1, 0, "VCG01 0", True),
        {"op": "get_quest", "quest": v0, "fields": ["running", "completed", "stage", "done.95", "done.100"],
         "label": "VCG00 after VCG01 0"},
        {"op": "get_ref", "ref": VICTOR, "fields": ["disabled"], "label": "Victor after VCG01 0"},
        {"op": "get_ref", "ref": DOC_DOOR, "fields": ["destroyed"], "label": "house door after VCG01 0"},
        {"op": "get_item_count", "of": "player", "item": PIPBOY, "label": "Pip-Boy after VCG01 0"},
        wait(v1, 1, "VCG01 1", True),
        wait(v1, 3, "VCG01 3", True),
        wait(v1, 5, "VCG01 5 (Doc's first line ended)"),
        {"op": "start_stopwatch"},
        wait(v1, 7, "VCG01 7", True),
        wait(v1, 8, "VCG01 8"),
        wait(v1, 10, "VCG01 10 (Doc's lines ended)"),
        menu(NAME, "name menu open at 10"),
        {"op": "start_stopwatch"},
        {"op": "wait_frames", "frames": 120},
        wait(v1, 10, "script time in the name menu", True),
        {"op": "get_quest", "quest": v1, "fields": ["stage", "done.15"], "label": "VCG01 in the name menu"},
        {"op": "close_menus"},
        {"op": "start_stopwatch"},
        wait(v1, 15, "VCG01 15", True),
        wait(v1, 25, "VCG01 25"),
        wait(v1, 30, "VCG01 30"),
        {"op": "start_stopwatch"},
        wait(v1, 35, "VCG01 35", True),
        wait(v1, 36, "VCG01 36"),
        menu(RACE, "race menu open at 36"),
        {"op": "get_quest", "quest": v1, "fields": ["stage", "done.36", "done.37", "done.40"],
         "label": "VCG01 in the race menu"},
        {"op": "close_menus"},
        wait(v1, 45, "VCG01 45"),
        {"op": "start_stopwatch"},
        wait(v1, 50, "VCG01 50", True),
        {"op": "get_quest", "quest": v1,
         "fields": ["running", "completed", "stage"] + [f"done.{s}" for s in VCG01_STAGES if s <= 55]
                   + [f"objective.{o}.{k}" for o in (10, 30) for k in ("displayed", "completed")]
                   + ["var.bRunTimer", "var.bChooseSex", "var.bGiveTest", "var.bBeganTest"],
         "label": "VCG01 at 50"},
        {"op": "get_quest", "quest": "CGTutorial", "fields": ["stage", "done.2"], "label": "CGTutorial at 50"},
        {"op": "get_menus", "menus": [MESSAGE, RACE, NAME], "label": "menus at 50"},
        # Safety (not an observation): Doc's next two lines end in VCG01 55, whose result
        # script autosaves; disabled, he cannot finish them before the game quits.
        {"op": "disable", "target": DOC},
    ]
    return doc("VCG00 90 to VCG01 50 from the new-game save: pack message boxes, the opening countdowns, Doc "
               "Mitchell's first lines, the name and race menus (closed), stages set by timers and by dialogue",
               nvfnew_given(), acts)


def tester():
    """VCG01 from the vigor tester to stage 80, reached without stage 55: while the first
    pack box holds GameMode, VCG01 0 and 60 are set and objective 30 is shown (in the
    natural order Doc's line at 60 shows it); then the boxes are closed."""
    v1 = "VCG01"
    acts = [
        {"op": "set_stage", "quest": v1, "stage": 0},
        {"op": "get_quest", "quest": "VCG00", "fields": ["running", "stage", "done.95", "done.100"],
         "label": "VCG00 after VCG01 0"},
        {"op": "set_stage", "quest": v1, "stage": 60},
        {"op": "set_objective", "quest": v1, "index": 30, "displayed": True},
        {"op": "get_quest", "quest": v1,
         "fields": ["stage", "done.0", "done.1", "done.60", "objective.10.displayed", "objective.10.completed",
                    "objective.30.displayed", "var.bRunTimer", "var.fTimer"],
         "label": "VCG01 at 60 with the box open"},
        {"op": "dismiss_message_boxes", "max": 6, "wait_frames": 600},
        wait(v1, 5, "VCG01 5 (Doc's first line ended)"),
        {"op": "get_quest", "quest": v1, "fields": ["stage", "done.1", "done.3", "done.7", "done.55"],
         "label": "VCG01 after Doc's first line"},
        {"op": "activate_read", "target": TESTER, "actor": "player", "run_on_activate": True,
         "read": {"quest": v1, "field": "stage"}, "label": "vigor tester activated: in the same script"},
        {"op": "get_menus", "menus": [SPECIAL_MENU], "label": "SPECIAL menu after the tester"},
        {"op": "get_ref", "ref": TESTER, "fields": ["destroyed"], "label": "tester after its activation"},
        {"op": "get_quest", "quest": v1, "fields": ["objective.30.completed", "var.nDocReaction"],
         "label": "VCG01 in the SPECIAL menu"},
        {"op": "close_menus"},
        {"op": "start_stopwatch"},
        wait(v1, 70, "VCG01 70", True),
        {"op": "get_quest", "quest": v1,
         "fields": ["var.nDocReaction", "var.nMostExtremeStat", "var.bLow", "var.fMaxDeviation", "var.fCurrentValue",
                    "var.fAbsCurrentValue"], "label": "Doc's reaction"},
        wait(v1, 75, "VCG01 75 (Doc's reaction line ended)"),
        {"op": "get_quest", "quest": v1, "fields": ["stage", "done.75", "done.76", "done.78", "done.79"],
         "label": "VCG01 after 75"},
        wait(v1, 80, "VCG01 80 (psych intro lines ended)"),
        {"op": "get_quest", "quest": v1,
         "fields": ["running", "stage", "done.80", "objective.30.completed", "objective.40.displayed",
                    "objective.40.completed"], "label": "VCG01 at 80"},
    ]
    return doc("VCG01 60 to 80 without stage 55: the vigor tester's OnActivate (SPECIAL menu, stage 65), Doc's "
               "reaction to the SPECIAL stats (stage 65's timer), 70 to 80 through Doc's lines",
               nvfnew_given(), acts)


def psych_actions(scores):
    """The psych test's answers: each answer's result script adds to VCG01Test's scores;
    the last sets bDoFinalCalculation 1 (INFOs of topics 0x00105515-0x00105560)."""
    acts = [{"op": "set_quest_var", "quest": "VCG01Test", "var": k, "value": v} for k, v in scores.items()]
    acts += [{"op": "set_quest_var", "quest": "VCG01Test", "var": "bDoFinalCalculation", "value": 1},
             {"op": "wait_quest", "quest": "VCG01Test", "field": "var.bDoFinalCalculation", "value": 2,
              "max_frames": 600, "label": "VCG01Test calculation done"},
             {"op": "get_quest", "quest": "VCG01Test",
              "fields": ["running"] + [f"var.{v}" for v in TEST_SCORES + TEST_RESULTS], "label": "VCG01Test after"},
             {"op": "get_tag_skills", "skills": SKILLS, "label": "tag skills after the psych test"}]
    return acts


def complete():
    """VCG01 80 to 200 without stage 55: the psych test's effect, 85 to 110 (tag-skill and
    trait menus closed), 115 (the exit trigger's SetStage), and the farewell dialogue's
    VGenericTimer event 3, which sets VCG01 200 and VCG02 5 (the Pip-Boy it also gives is
    left out: equipping it from a job crashed the game in trial q5d_trial_drive2)."""
    v1 = "VCG01"
    acts = [{"op": "set_stage", "quest": v1, "stage": 0}, {"op": "set_stage", "quest": v1, "stage": 80},
            {"op": "dismiss_message_boxes", "max": 6, "wait_frames": 600},
            {"op": "get_tag_skills", "skills": SKILLS, "label": "tag skills before the psych test"}]
    acts += psych_actions({"nSneak": 2, "nScience": 3, "nSpeech": 1, "nSmallGuns": 2, "nMedicine": 1, "nSurvival": 1})
    acts += [
        {"op": "set_quest_var", "quest": v1, "var": "bBeganTest", "value": 1},
        {"op": "set_objective", "quest": v1, "index": 40, "completed": True},
        {"op": "set_stage", "quest": v1, "stage": 85},
        wait(v1, 90, "VCG01 90"),
        menu(TAGS, "tag-skill menu open at 90"),
        {"op": "close_menus"},
        {"op": "start_stopwatch"},
        wait(v1, 95, "VCG01 95", True),
        wait(v1, 98, "VCG01 98 (Doc's trait lines ended)"),
        {"op": "start_stopwatch"},
        wait(v1, 100, "VCG01 100", True),
        wait(v1, 102, "VCG01 102", True),
        menu(TRAITS, "trait menu open at 102"),
        {"op": "close_menus"},
        {"op": "start_stopwatch"},
        wait(v1, 105, "VCG01 105", True),
        wait(v1, 110, "VCG01 110 (Doc's farewell lines ended)"),
        {"op": "get_quest", "quest": v1,
         "fields": ["stage"] + [f"done.{s}" for s in (85, 90, 95, 98, 100, 102, 105, 110)]
                   + [f"objective.{o}.{k}" for o in (40, 50) for k in ("displayed", "completed")],
         "label": "VCG01 at 110"},
        {"op": "set_stage", "quest": v1, "stage": 115},
        {"op": "get_quest", "quest": v1, "fields": ["stage", "done.115", "done.200"], "label": "VCG01 at 115"},
        {"op": "set_quest_var", "quest": "VGenericTimer", "var": "fTimer", "value": 0.1},
        {"op": "set_quest_var", "quest": "VGenericTimer", "var": "nEvent", "value": 3},
        {"op": "start_quest", "quest": "VGenericTimer"},
        wait(v1, 200, "VCG01 200"),
        {"op": "get_quest", "quest": v1,
         "fields": ["running", "completed", "stage", "done.200"]
                   + [f"objective.{o}.{k}" for o in (10, 30, 40, 50) for k in ("displayed", "completed")],
         "label": "VCG01 at 200"},
        {"op": "get_quest", "quest": "VMQ01",
         "fields": ["running", "completed", "stage", "done.10", "done.15"]
                   + [f"objective.{o}.displayed" for o in (10, 20, 25, 30)], "label": "VMQ01 after VCG01 200"},
        {"op": "get_quest", "quest": "VCG02",
         "fields": ["running", "completed", "stage", "done.5", "done.10", "objective.3.displayed",
                    "objective.5.displayed"], "label": "VCG02 after VCG01 200"},
        {"op": "get_quest", "quest": "VCG04", "fields": ["running", "completed"], "label": "VCG04 after VCG01 200"},
        {"op": "get_quest", "quest": "VGenericTimer", "fields": ["running", "var.nEvent"],
         "label": "VGenericTimer after VCG01 200"},
        {"op": "get_disabled", "of": CEMETERY_MARKER, "label": "cemetery map marker after VCG01 200"},
        {"op": "get_global", "name": "GameHour", "label": "GameHour after VCG01 200"},
    ]
    given = nvfnew_given(("VCG02", "VCG03", "VCG04", "VMQ01", "VGenericTimer"))
    given += [{"kind": "disabled", "of": CEMETERY_MARKER, "disabled": True}]
    return doc("VCG01 80 to 200 without stage 55: the psych test's tag skills, 85 to 115 with the tag-skill and trait "
               "menus closed, and VGenericTimer's event 3 completing VCG01 and starting VMQ01, VCG02 and VCG04",
               given, acts)


def main():
    for name, d in (("intro_opening", opening()), ("intro_tester", tester()), ("intro_complete", complete())):
        path = os.path.join(HERE, f"{name}.actions.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(d, indent=1) + "\n")
        print(path)


if __name__ == "__main__":
    main()
