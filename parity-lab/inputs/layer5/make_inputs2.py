#!/usr/bin/env python3
"""Writes the second round of Layer 5 scenario inputs (fnv.script.actions/1 with the
round 2 additions, docs/spec-notes/layer5-captures.md sections 4 and 8) into this
folder. The same file drives the in-game capture (tools/capture/capture_quests.py job)
and the candidate (`fnvtool script run --actions`). Quests, globals and object-script
variables are named by Editor ID / name, forms by global FormID of FalloutNV.esm
(looked up read-only, 2026-10-09).

  python parity-lab/inputs/layer5/make_inputs2.py

Every `given` states the facts of the save the scenario depends on (quest running
state, stage, stages done, objectives, variables, item counts, dead and disabled
states), checked against the save when the golden is built.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
FRAME = 1 / 60

CAPS = "0x0000000F"                    # Caps001
JAR = "0x0016665D"                     # JarPistolPowder (OnAdd + GameMode script)
POWDER = "0x00140AB2"                  # PowderPistol
DOC = "0x00104C0F"                     # DocMitchellREF
DOC_DOOR = "0x00103E61"                # GSDocMitchellHouseIntDoorREF
DRESSER = "0x00103E37"                 # DocMitchellDresserRef
SETTLERS = ["0x00104F08", "0x00104F0A", "0x00104F03"]  # GoodspringsSettler04Ref, 03Ref, 01Ref
SUNNY = "0x00104E85"                   # SunnyRef
FLEE_PACKAGE = "0x00107480"            # GoodspringsFleePackage
SUNNY_STAY_PACKAGE = "0x0010A21C"      # SunnySmilesStayAtCurrentLocation
EDE_DISABLED = "0x001572E6"            # NVCompanionEDEDisabledREF
LEGIONARY = "0x0011F654"               # TecLegionaryRaider5REF
HOSTAGE_1 = "0x0012319B"               # TechMineNCRHostage1REF
HOSTAGE_2 = "0x0012319C"               # TechMineNCRHostage2REF
RENOLDS = "0x00134B9C"                 # NVTecNCRRenoldsREF
RINGO = "0x00104C7D"                   # RingoRef
GANG_MARKER = "0x00105D4C"             # GoodspringsPowderGangMarker


def write(name, doc):
    doc = {"format": "fnv.script.actions/1", "seed": 0, "frame_seconds": FRAME, **doc}
    with open(os.path.join(HERE, f"{name}.actions.json"), "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(doc, indent=2) + "\n")


def get(quest, label, fields):
    return {"op": "get_quest", "quest": quest, "label": label, "fields": fields}


def stage(quest, n):
    return {"op": "set_stage", "quest": quest, "stage": n}


def qvar(quest, var, value):
    return {"op": "set_quest_var", "quest": quest, "var": var, "value": value}


RUMBLE_VARS = ["Stage1Fired", "Stage1FireOnTimerValue", "stage1RumbleLeft", "Stage1Duration", "timerMaxValue"]


def multi_entry_stage():
    """ControlRumble stage 0 has five log entries without conditions; entry 0 resets
    Stage1Fired…, 1 Stage1FireOnTimerValue…, 2 stage1RumbleLeft…, 3 Stage1Duration… and
    timerMaxValue (to 10), 4 calls StopQuest. Allow repeated stages is set."""
    f = ["running", "stage", "done.0"] + [f"var.{v}" for v in RUMBLE_VARS]
    write("quest2_multi_entry_stage", {
        "title": "ControlRumble stage 0: five log entries without conditions, each with a result script",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "ControlRumble", "running": False, "completed": False, "stage": 0,
                   "done": [0], "not_done": [1],
                   "vars": {"initDefaults": 1, "RumbleNow": 0, "Stage1Fired": 0, "Stage1FireOnTimerValue": 0,
                            "stage1RumbleLeft": 0, "Stage1Duration": 0, "timerMaxValue": 10}}],
        "actions": [qvar("ControlRumble", v, 7) for v in RUMBLE_VARS] + [
            get("ControlRumble", "ControlRumble before stage 0", f),
            stage("ControlRumble", 0),
            get("ControlRumble", "ControlRumble after stage 0", f)]})


def condition_groups():
    """Package conditions that mix AND and OR. GoodspringsFleePackage, the first
    package of the Goodsprings settlers: VMS16.bGunFightStart == 1 (OR) /
    VMS16b.bPowderGangAttacking == 1 / VMS16.bTrudyHelp == 0. SunnySmilesStayAtCurrentLocation
    (Sunny's third package): GetObjectiveDisplayed VCG02 70 == 0 / GetQuestCompleted VCG02
    == 0 / GetStage VCG02 == 45 (OR) / == 40 (OR) / == 20; Sunny's two packages before it
    need bMetSunny 0 and VCG02 objective 10 completed."""
    acts = []
    for name, gunfight, trudy, gang in (("gunfight, Trudy not helping", 1, 0, 0), ("gunfight, Trudy helping", 1, 1, 0),
                                        ("gang attacking, Trudy not helping", 0, 0, 1),
                                        ("gang attacking, Trudy helping", 0, 1, 1), ("nothing", 0, 0, 0)):
        acts += [qvar("VMS16", "bGunFightStart", gunfight), qvar("VMS16", "bTrudyHelp", trudy),
                 qvar("VMS16b", "bPowderGangAttacking", gang)]
        acts += [{"op": "evaluate_package", "of": s} for s in SETTLERS]
        acts += [{"op": "is_current_package", "of": s, "package": FLEE_PACKAGE, "label": f"{name}: settler {s} flees"}
                 for s in SETTLERS]
    acts += [qvar("VFreeformGoodsprings", "bMetSunny", 1), stage("VCG02", 20)]

    def sunny(label):
        return [{"op": "evaluate_package", "of": SUNNY},
                {"op": "is_current_package", "of": SUNNY, "package": SUNNY_STAY_PACKAGE, "label": label}]

    def objective70(shown):
        return {"op": "set_objective", "quest": "VCG02", "index": 70, "displayed": shown}

    acts += sunny("Sunny stays: VCG02 stage 20")
    acts += [objective70(True)] + sunny("Sunny stays: VCG02 stage 20, objective 70 shown")
    acts += [objective70(False)] + sunny("Sunny stays: VCG02 stage 20, objective 70 hidden")
    acts += [stage("VCG02", 45)] + sunny("Sunny stays: VCG02 stage 45")
    acts += [objective70(True)] + sunny("Sunny stays: VCG02 stage 45, objective 70 shown")
    write("quest2_condition_groups", {
        "title": "AND/OR condition lists: which AI package the Goodsprings settlers and Sunny Smiles choose",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "VMS16", "running": False, "completed": False, "stage": 0,
                   "vars": {"bGunFightStart": 0, "bTrudyHelp": 0}},
                  {"kind": "quest", "quest": "VMS16b", "running": False, "completed": False, "stage": 0,
                   "vars": {"bPowderGangAttacking": 0}},
                  {"kind": "quest", "quest": "VFreeformGoodsprings", "running": True, "vars": {"bMetSunny": 0}},
                  {"kind": "quest", "quest": "VCG02", "running": False, "completed": False, "stage": 5, "done": [5],
                   "not_done": [20, 40, 45],
                   "objectives": [{"index": 10, "displayed": False, "completed": False},
                                  {"index": 20, "displayed": False}, {"index": 70, "displayed": False}]}]
                 + [{"kind": "ref", "ref": r, "disabled": False, "dead": False} for r in SETTLERS + [SUNNY]],
        "actions": acts})


def stray_elseif():
    """BeatrixFX's quest script (delay 0.1 s) counts `timer` up by GetSecondsPassed;
    after an If that is closed by EndIf come six ElseIf…EndIf pairs that set `bump` to
    2…7 at increasing times; at timer > 9 the script stops the quest."""
    write("quest2_stray_elseif", {
        "title": "BeatrixFX: ElseIf markers after their If chain was closed, in a quest script",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "BeatrixFX", "running": False, "completed": False, "stage": 0,
                   "vars": {"reset": 0, "bump": 0, "timer": 0}}],
        "actions": [{"op": "start_quest", "quest": "BeatrixFX"},
                    {"op": "sample_var", "quest": "BeatrixFX", "var": "bump", "frames": 900, "values": True,
                     "label": "BeatrixFX bump while running"},
                    get("BeatrixFX", "BeatrixFX after it stopped", ["running", "var.reset", "var.bump"])]})


def dead_activation():
    """Scripted activation of dead actors with and without the run-OnActivate flag:
    ED-E's disabled body (OnActivate sets vDialogueEDE.bEDEExamined and shows a message;
    never calls Activate), a legionary without an OnActivate block, and the two
    Techatticup hostages (FalloutNV SCPT 0x00123191: the only Activate of the OnActivate
    block follows an ElseIf after its chain was closed)."""
    write("quest2_dead_activation", {
        "title": "OnActivate of dead and living actors run by a scripted Activate, with and without the flag",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "vDialogueEDE", "vars": {"bEDEExamined": 0}},
                  {"kind": "ref", "ref": EDE_DISABLED, "dead": True},
                  {"kind": "ref", "ref": LEGIONARY, "dead": False},
                  {"kind": "ref", "ref": HOSTAGE_1, "dead": False, "vars": {"Freed": 0}},
                  {"kind": "ref", "ref": HOSTAGE_2, "dead": False, "vars": {"Freed": 0}}],
        "actions": [
            {"op": "activate", "target": EDE_DISABLED, "actor": "player", "label": "dead ED-E activated"},
            get("vDialogueEDE", "bEDEExamined after the plain activation", ["var.bEDEExamined"]),
            {"op": "activate", "target": EDE_DISABLED, "actor": "player", "run_on_activate": True,
             "label": "dead ED-E activated with the flag"},
            get("vDialogueEDE", "bEDEExamined after the flagged activation", ["var.bEDEExamined"]),
            {"op": "kill", "target": LEGIONARY},
            {"op": "get_ref", "ref": LEGIONARY, "fields": ["dead"], "label": "legionary killed"},
            {"op": "activate", "target": LEGIONARY, "actor": "player", "run_on_activate": True,
             "label": "dead legionary activated with the flag"},
            {"op": "kill", "target": HOSTAGE_1},
            {"op": "get_ref", "ref": HOSTAGE_1, "fields": ["dead"], "label": "hostage 1 killed"},
            {"op": "activate", "target": HOSTAGE_1, "actor": "player", "run_on_activate": True,
             "label": "dead hostage activated with the flag"},
            {"op": "set_ref_var", "ref": HOSTAGE_2, "var": "Freed", "value": 1},
            {"op": "activate", "target": HOSTAGE_2, "actor": "player", "run_on_activate": True,
             "label": "living freed hostage activated with the flag"}]})


def ref_activation():
    """GSDocMitchellDoorScript: OnActivate sets DocMitchellREF.PlayerLeft to 1, then
    Activate (the door's default: the player goes outside)."""
    write("quest2_ref_activation", {
        "title": "A door's OnActivate writes another reference's variable: scripted Activate with and without the flag",
        "save": "nvfdoc",
        "given": [{"kind": "ref", "ref": DOC, "dead": False, "vars": {"PlayerLeft": 1}}],
        "actions": [
            {"op": "set_ref_var", "ref": DOC, "var": "PlayerLeft", "value": 0},
            {"op": "get_ref", "ref": DOC, "fields": ["var.PlayerLeft"], "label": "PlayerLeft reset"},
            {"op": "activate_read", "target": DOC_DOOR, "actor": "player", "read": {"ref": DOC, "field": "var.PlayerLeft"},
             "label": "PlayerLeft read in the script that activated the door"},
            {"op": "get_ref", "ref": DOC, "fields": ["var.PlayerLeft"], "label": "PlayerLeft after the plain activation"},
            {"op": "activate_read", "target": DOC_DOOR, "actor": "player", "run_on_activate": True,
             "read": {"ref": DOC, "field": "var.PlayerLeft"},
             "label": "PlayerLeft read in the script that activated the door with the flag"},
            {"op": "get_ref", "ref": DOC, "fields": ["var.PlayerLeft"], "label": "PlayerLeft after the flagged activation"}]})


def onadd_gamemode():
    """JarPistolPowderAddScript: OnAdd Player sets killme; GameMode then adds 200
    PowderPistol to the player and removes the jar (RemoveMe)."""
    write("quest2_onadd_gamemode", {
        "title": "An item's OnAdd and GameMode blocks: a jar of pistol powder added to the player",
        "save": "nvftest",
        "given": [{"kind": "item_count", "of": "player", "item": JAR, "count": 0},
                  {"kind": "item_count", "of": "player", "item": POWDER, "count": 0}],
        "actions": [
            {"op": "add_item_read", "to": "player", "item": JAR, "count": 1, "read": [JAR, POWDER],
             "label": "counts in the script that added the jar"},
            {"op": "sample_items", "of": "player", "items": [JAR, POWDER], "frames": 20, "label": "counts after the jar"},
            {"op": "remove_item", "from": "player", "item": POWDER, "count": 200}]})


def seconds_after_menu():
    """GetSecondsPassed countdowns after a container menu held open for 900 frames:
    VCG00 (delay 0.01: every frame; has a MenuMode block) and VMQ03aTimer (delay 0.5;
    GameMode only)."""
    write("quest2_seconds_after_menu", {
        "title": "GetSecondsPassed in quest scripts after a long menu",
        "save": "nvfdoc",
        "given": [{"kind": "quest", "quest": "VCG00", "running": False, "completed": False, "stage": 100,
                   "done": [100], "vars": {"bRunTimer": 1}},
                  {"kind": "quest", "quest": "VMQ03aTimer", "running": False, "completed": False, "stage": 0,
                   "vars": {"bRunTimer": 0, "fTimer": 0, "nEvent": 0}}],
        "actions": [
            qvar("VCG00", "fTimer", 100000), {"op": "start_quest", "quest": "VCG00"}, {"op": "wait_frames", "frames": 30},
            {"op": "menu_gap", "via": DRESSER, "frames": 900, "quest": "VCG00", "var": "fTimer", "after": 30,
             "label": "VCG00 countdown: first drop after the menu"},
            qvar("VMQ03aTimer", "fTimer", 100000), qvar("VMQ03aTimer", "bRunTimer", 1),
            {"op": "start_quest", "quest": "VMQ03aTimer"}, {"op": "wait_frames", "frames": 60},
            {"op": "menu_gap", "via": DRESSER, "frames": 900, "quest": "VMQ03aTimer", "var": "fTimer", "after": 90,
             "label": "VMQ03aTimer countdown: first drop after the menu"}]})


def holdout_a():
    write("quest2_holdout_a", {
        "title": "Layer 5 round 2 scenario",
        "save": "nvftest",
        "given": [{"kind": "quest", "quest": "VTechatticup", "running": True, "completed": False, "stage": 0,
                   "not_done": [10, 20, 100, 110],
                   "objectives": [{"index": 10, "displayed": False}, {"index": 20, "displayed": False}],
                   "vars": {"NumHostages": 0, "HostagesFreed": 0, "HostagesDead": 0, "DoOnce": 0}},
                  {"kind": "ref", "ref": RENOLDS, "dead": False}],
        "actions": [
            stage("VTechatticup", 10),
            get("VTechatticup", "after the first stage", ["stage", "objective.10.displayed"]),
            qvar("VTechatticup", "NumHostages", 2),
            {"op": "wait_frames", "frames": 1200},
            get("VTechatticup", "after the wait", ["running", "completed", "stage", "done.20", "objective.10.completed",
                                                   "objective.20.displayed", "var.NumHostages", "var.HostagesFreed",
                                                   "var.DoOnce"])]})


def holdout_b():
    write("quest2_holdout_b", {
        "title": "Layer 5 round 2 scenario",
        "save": "nvftest",
        "given": [{"kind": "ref", "ref": RINGO, "dead": False, "disabled": False},
                  {"kind": "quest", "quest": "VMS16", "running": False, "completed": False, "stage": 0,
                   "not_done": [70, 110]},
                  {"kind": "quest", "quest": "VMS16b", "running": False, "completed": False, "stage": 0,
                   "objectives": [{"index": 10, "displayed": False, "completed": False}, {"index": 15, "displayed": False}]},
                  {"kind": "disabled", "of": GANG_MARKER, "disabled": True}],
        "actions": [
            {"op": "set_objective", "quest": "VMS16b", "index": 10, "displayed": True},
            {"op": "remove_all_items", "from": RINGO},
            {"op": "get_item_count", "of": RINGO, "item": CAPS, "label": "before"},
            {"op": "kill", "target": RINGO},
            {"op": "get_ref", "ref": RINGO, "fields": ["dead"], "label": "after"},
            {"op": "get_item_count", "of": RINGO, "item": CAPS, "label": "count after"},
            get("VMS16", "first quest after", ["running", "completed", "stage", "done.110"]),
            get("VMS16b", "second quest after", ["running", "objective.10.displayed", "objective.10.completed",
                                                 "objective.15.displayed"]),
            {"op": "get_disabled", "of": GANG_MARKER, "label": "reference after"}]})


def main():
    for f in (multi_entry_stage, condition_groups, stray_elseif, dead_activation, ref_activation, onadd_gamemode,
              seconds_after_menu, holdout_a, holdout_b):
        f()


if __name__ == "__main__":
    main()
