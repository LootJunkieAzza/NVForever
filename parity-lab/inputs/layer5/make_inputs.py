#!/usr/bin/env python3
"""Writes the Layer 5 quest scenario inputs (fnv.script.actions/1, contract in
docs/spec-notes/layer5-captures.md section 4) into this folder. The same file drives the
in-game capture (tools/capture/capture_quests.py job) and the candidate
(`fnvtool script run --actions`). Forms are global FormIDs of FalloutNV.esm, quests and
globals Editor IDs (looked up read-only, 2026-10-09).

  python parity-lab/inputs/layer5/make_inputs.py
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
FRAME = 1 / 60

HEALING_POWDER = "0x00136A1D"   # NVHealingPowder
XANDER_ROOT = "0x0015D982"      # VCG03XanderRootREF
BROC_FLOWER = "0x0015D981"      # VCG03BrocFlowerREF
CRITTER_1 = "0x00168C25"        # VERT1SurvivalSkillCritter01REF
DOC_MITCHELL = "0x00104C0F"     # DocMitchellREF
DOC_DRESSER = "0x00103E37"      # DocMitchellDresserRef
MEDICAL_SUPPLIES = "0x0015EF9C"  # VMS16bMedicalSupplies


def write(name, doc):
    doc = {"format": "fnv.script.actions/1", "seed": 0, "frame_seconds": FRAME, **doc}
    with open(os.path.join(HERE, f"{name}.actions.json"), "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(doc, indent=2) + "\n")


def get(quest, label, fields):
    return {"op": "get_quest", "quest": quest, "label": label, "fields": fields}


def stage(quest, n):
    return {"op": "set_stage", "quest": quest, "stage": n}


def stage_order():
    f = ["running", "completed", "stage", "done.5", "done.10", "done.15", "done.50",
         "objective.5.displayed", "objective.5.completed", "objective.10.displayed", "objective.10.completed",
         "objective.50.displayed"]
    write("quest_stage_order", {
        "title": "VMS16: a higher stage, a lower one, the same one again, an undefined one, the fail stage",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "VMS16", "running": False, "completed": False, "stage": 0,
                   "not_done": [5, 10, 50, 110],
                   "objectives": [{"index": 5, "displayed": False}, {"index": 10, "displayed": False},
                                  {"index": 50, "displayed": False}]}],
        "actions": [
            get("VMS16", "VMS16 before", f),
            stage("VMS16", 50), get("VMS16", "VMS16 after stage 50", f),
            stage("VMS16", 10), get("VMS16", "VMS16 after the lower stage 10", f),
            {"op": "set_objective", "quest": "VMS16", "index": 10, "displayed": False},
            get("VMS16", "VMS16 objective 10 hidden again", ["objective.10.displayed"]),
            stage("VMS16", 10), get("VMS16", "VMS16 after stage 10 again", f),
            stage("VMS16", 15), get("VMS16", "VMS16 after the undefined stage 15", f),
            stage("VMS16", 5), get("VMS16", "VMS16 after the lower stage 5", f),
            stage("VMS16", 110), get("VMS16", "VMS16 after the fail stage 110", f + ["done.110"])]})


def repeated_stage():
    f = ["running", "stage", "done.60", "done.70"]
    write("quest_repeated_stage", {
        "title": "Generic (allow repeated stages): stage 70 sets ScroungerOdds, set twice; then the lower stage 60",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "Generic", "running": True, "not_done": [60, 70]},
                  {"kind": "global", "name": "ScroungerOdds", "value": 100},
                  {"kind": "global", "name": "FortuneFinderOdds", "value": 100},
                  {"kind": "global", "name": "FortuneFinderChangeOdds", "value": 100}],
        "actions": [
            get("Generic", "Generic before", f),
            {"op": "set_global", "name": "ScroungerOdds", "value": 0},
            stage("Generic", 70),
            {"op": "get_global", "name": "ScroungerOdds", "label": "ScroungerOdds after stage 70"},
            get("Generic", "Generic after stage 70", f),
            {"op": "set_global", "name": "ScroungerOdds", "value": 0},
            stage("Generic", 70),
            {"op": "get_global", "name": "ScroungerOdds", "label": "ScroungerOdds after stage 70 again"},
            get("Generic", "Generic after stage 70 again", f),
            stage("Generic", 60),
            {"op": "get_global", "name": "FortuneFinderOdds", "label": "FortuneFinderOdds after the lower stage 60"},
            get("Generic", "Generic after the lower stage 60", f),
            {"op": "set_global", "name": "ScroungerOdds", "value": 100},
            {"op": "set_global", "name": "FortuneFinderOdds", "value": 100},
            {"op": "set_global", "name": "FortuneFinderChangeOdds", "value": 100}]})


def result_effects():
    f = ["running", "completed", "stage", "done.10", "var.nHealingPowdersAtStart"]
    write("quest_result_effects", {
        "title": "VCG03 stage 10: a quest variable set from a function call, references enabled; "
                 "the stage set again without the repeat flag",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "VCG03", "running": False, "completed": False, "stage": 0,
                   "not_done": [10], "vars": {"nHealingPowdersAtStart": 0}},
                  {"kind": "item_count", "of": "player", "item": HEALING_POWDER, "count": 0},
                  {"kind": "disabled", "of": XANDER_ROOT, "disabled": True},
                  {"kind": "disabled", "of": BROC_FLOWER, "disabled": True}],
        "actions": [
            {"op": "add_item", "to": "player", "item": HEALING_POWDER, "count": 2},
            {"op": "set_stage_read", "quest": "VCG03", "stage": 10, "field": "var.nHealingPowdersAtStart",
             "label": "VCG03 variable read in the script that set stage 10"},
            get("VCG03", "VCG03 after stage 10", f),
            {"op": "get_disabled", "of": XANDER_ROOT, "label": "Xander root after stage 10"},
            {"op": "get_disabled", "of": BROC_FLOWER, "label": "broc flower after stage 10"},
            {"op": "get_disabled", "of": CRITTER_1, "label": "survival critter 1 after stage 10"},
            {"op": "add_item", "to": "player", "item": HEALING_POWDER, "count": 1},
            {"op": "disable", "target": XANDER_ROOT},
            stage("VCG03", 10),
            get("VCG03", "VCG03 after stage 10 again", f),
            {"op": "get_disabled", "of": XANDER_ROOT, "label": "Xander root after stage 10 again"},
            {"op": "remove_item", "from": "player", "item": HEALING_POWDER, "count": 3}]})


def result_additem():
    f = ["running", "completed", "stage", "done.30", "objective.20.completed", "objective.30.displayed",
         "objective.40.displayed", "objective.60.displayed"]
    write("quest_result_additem", {
        "title": "VMS16b stage 30: a result script with if/else that gives an actor an item and shows objectives",
        "save": "nvfdoc",
        "given": [{"kind": "quest", "quest": "VMS16b", "running": False, "stage": 0, "not_done": [30]},
                  {"kind": "item_count", "of": DOC_MITCHELL, "item": MEDICAL_SUPPLIES, "count": 0},
                  {"kind": "item_count", "of": DOC_DRESSER, "item": MEDICAL_SUPPLIES, "count": 0}],
        "actions": [
            stage("VMS16b", 30),
            {"op": "get_item_count", "of": DOC_MITCHELL, "item": MEDICAL_SUPPLIES, "label": "actor's item after stage 30"},
            {"op": "get_item_count", "of": DOC_DRESSER, "item": MEDICAL_SUPPLIES, "label": "container's item after stage 30"},
            get("VMS16b", "VMS16b after stage 30", f),
            {"op": "remove_item", "from": DOC_MITCHELL, "item": MEDICAL_SUPPLIES, "count": 1}]})


def complete_flag():
    f = ["running", "completed", "stage", "done.90", "done.100"]
    write("quest_complete_flag", {
        "title": "VMS22: the Complete Quest stage 90 (empty script), then the Fail Quest stage 100 (empty script)",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "VMS22", "running": True, "completed": False, "not_done": [90, 100]}],
        "actions": [stage("VMS22", 90), get("VMS22", "VMS22 after the complete stage 90", f),
                    stage("VMS22", 100), get("VMS22", "VMS22 after the fail stage 100", f)]})


def var_storage():
    """A Set statement with a numeric literal into a local of each declared type, read
    back. Captured with the job script's own locals (quest variables cannot be written
    by a Set statement from a job script, docs/spec-notes/layer5-captures.md section 1);
    the compiled form keeps only Integer vs FloatOrRef (SLSD), so a quest script's
    locals take the same path [hypothesis]."""
    values = {"short": [32767, 32768, 40000, 65535, 65536, 70000, -32768, -32769, -40000, 2147483647, 2147483648,
                        3000000000, 2.7, -2.7, 2.5, -2.5, 0.5, -0.5, 0.3333333333333333, 40000.5, 2147483647.5,
                        3000000000.5, -3000000000.5],
              "long": [40000, 2147483647, 2147483648, 3000000000, -3000000000, 2.7, -2.7],
              "int": [40000, 3000000000, 2.7, -2.7],
              "float": [0.3333333333333333, 0.1, 16777217, 0.00000001, 123456789.123, 40000, -2.7, 3000000000,
                        3000000000.5, -3000000000, -3000000000.5, 4294967296.5]}
    acts = [{"op": "set_local", "type": t, "value": v, "label": f"{t} set to {v!r}"}
            for t, vs in values.items() for v in vs]
    write("quest_var_storage", {
        "title": "What a Set statement stores into short, long, int and float variables",
        "save": "nvftest",
        "given": [],
        "actions": acts})


def delay(name, quest, title, runs, frames, given_vars, timer="fTimer", run_var="bRunTimer", running=False):
    """A GetSecondsPassed countdown in the quest's own GameMode block: each script run
    lowers the timer by the time since the previous run, so the drops are the quest's
    script delay. A stopped quest is sampled first, then started; a running one is
    sampled as it is. The timer starts far above zero so the countdown never ends."""
    f = ["running", "stage"]
    acts = [{"op": "set_quest_var", "quest": quest, "var": timer, "value": 100000}]
    if run_var:
        acts.append({"op": "set_quest_var", "quest": quest, "var": run_var, "value": 1})
    if not running:
        acts += [{"op": "sample_var", "quest": quest, "var": timer, "frames": 120, "label": f"{quest} timer while stopped"},
                 get(quest, f"{quest} before StartQuest", f),
                 {"op": "start_quest", "quest": quest}]
    # The first change after the start (or after the timer was set) depends on where the
    # quest's delay stood (docs/spec-notes/layer5-captures.md section 5.4); it is skipped.
    acts += [{"op": "sample_var", "quest": quest, "var": timer, "frames": frames, "runs": runs, "skip": 1,
              "label": f"{quest} timer while running"},
             get(quest, f"{quest} while running", f)]
    if not running:
        acts.append({"op": "stop_quest", "quest": quest})
    acts += [{"op": "set_quest_var", "quest": quest, "var": v, "value": given_vars.get(v, 0)}
             for v in (run_var, timer) if v]
    write(name, {
        "title": title,
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": quest, "running": running, "vars": given_vars}],
        "actions": acts})


def main():
    stage_order()
    repeated_stage()
    result_effects()
    result_additem()
    complete_flag()
    var_storage()
    delay("quest_delay_data", "VMQ03aTimer", "Quest script delay, flag clear, DATA delay 0.5 s (VMQ03aTimer)",
          3, 300, {"bRunTimer": 0, "fTimer": 0})
    delay("quest_delay_flagged_short", "VCG01", "Quest script delay, 'Default Script Processing Delay' flag set, "
          "DATA delay 0.01 s (VCG01)", 6, 1800, {"bRunTimer": 1, "fTimer": -0.015999460592865944})
    delay("quest_delay_frame", "VCG00", "Quest script delay, flag clear, DATA delay 0.01 s (VCG00)",
          6, 120, {"bRunTimer": 1, "fTimer": -0.014002632349729538})
    delay("quest_delay_flagged_1s", "CGTutorial", "Quest script delay, flag set, DATA delay 1.0 s, a running "
          "quest (CGTutorial)", 3, 1500, {"runTimer": 0, "timer": 0}, timer="timer", run_var="runTimer", running=True)
    delay("quest_delay_zero", "VMS40Timer", "Quest script delay, flag clear, DATA delay 0 (VMS40Timer)",
          3, 1500, {"bRunTimer": 0, "fTimer": 0})
    delay("quest_delay_flagged_zero", "L38TimerQuest", "Quest script delay, flag set, DATA delay 0 (L38TimerQuest)",
          3, 1500, {"Timer": 0, "open": 1}, timer="Timer", run_var=None)


if __name__ == "__main__":
    main()
