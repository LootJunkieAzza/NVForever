#!/usr/bin/env python3
"""Writes the visible scenario inputs of the third round of Layer 5 captures
(fnv.script.actions/1 with the round 3 additions, docs/spec-notes/layer5-captures.md
section 9) next to this file:

  python parity-lab/inputs/layer5/make_inputs3.py

Forms are named by FalloutNV.esm FormID (looked up read-only with `fnvtool esm
inventory` / `esm decode`, 2026-10-09):
- reputations (REPU, DATA value = the scale): RepNVWhiteGloveSociety 0x00116F16 (10),
  RepNVNovac 0x00129A79 (20), RepNVFreeside 0x00129A7A (70);
- aid items: NukaCola 0x0001519E, NVContrerasMedicalSupplies 0x000F1865, NVCateye
  0x0007EA1B; Caps001 0x0000000F; the ability ImplantRegenBoost 0x0014CCE2;
- Doc Mitchell's dresser 0x00103E37; two Goodsprings windows 0x001771EB, 0x001771EA
  (WastelandWindowScript copies GetCurrentTime into `Time` every run).
Every `given` states a fact of the save the observations depend on; the values were
read from the saves by the round 3 trials (q5c_trial_rep, q5c_trial_fx,
q5c_trial_order) and are checked again when each golden is built.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
FRAME = 1 / 60

WHITE_GLOVE, NOVAC, FREESIDE = "0x00116F16", "0x00129A79", "0x00129A7A"
SCALE = {WHITE_GLOVE: 10, NOVAC: 20, FREESIDE: 70}
NAME = {WHITE_GLOVE: "White Glove Society", NOVAC: "Novac", FREESIDE: "Freeside"}
NUKA_COLA, CONTRERAS, CATEYE, CAPS = "0x0001519E", "0x000F1865", "0x0007EA1B", "0x0000000F"
REGEN = "0x0014CCE2"
DRESSER = "0x00103E37"
WINDOWS = ["0x001771EB", "0x001771EA"]
ALL_FIELDS = ["fame", "infamy", "pct.fame", "pct.infamy", "threshold.mixed", "threshold.good", "threshold.bad"]


def doc(title, save, given, actions):
    return {"format": "fnv.script.actions/1", "seed": 0, "frame_seconds": FRAME, "title": title, "save": save,
            "given": given, "actions": actions}


def zero(rep):
    return [{"op": "set_reputation", "rep": rep, "axis": "fame", "value": 0},
            {"op": "set_reputation", "rep": rep, "axis": "infamy", "value": 0}]


def reputation():
    acts = []
    for rep in (WHITE_GLOVE, FREESIDE):
        n, s = NAME[rep], SCALE[rep]
        for axis in ("fame", "infamy"):
            for size in range(1, 6):
                acts += zero(rep) + [{"op": "add_reputation", "rep": rep, "axis": axis, "size": size},
                                     {"op": "get_reputation", "rep": rep, "fields": ALL_FIELDS,
                                      "label": f"{n}: AddReputation {axis} size {size} from 0"}]
            for size in range(1, 6):
                acts += zero(rep) + [{"op": "set_reputation", "rep": rep, "axis": axis, "value": 20},
                                     {"op": "remove_reputation", "rep": rep, "axis": axis, "size": size},
                                     {"op": "get_reputation", "rep": rep, "fields": [axis],
                                      "label": f"{n}: RemoveReputation {axis} size {size} from 20"}]
        edges = [("AddReputation size 5 one below the scale", [("set", s - 1), ("add", 5)]),
                 ("AddReputation size 1 above the scale", [("set", 2 * s), ("add", 1)]),
                 ("SetReputation twice the scale", [("set", 2 * s)]),
                 ("SetReputation -3", [("set", -3)]),
                 ("SetReputation 2.5", [("set", 2.5)]),
                 ("AddReputation size 0", [("add", 0)]),
                 ("AddReputation size 6", [("add", 6)]),
                 ("AddReputation size -2 from 10", [("set", 10), ("add", -2)]),
                 ("RemoveReputation size 5 from 1", [("set", 1), ("remove", 5)])]
        for what, ops in edges:
            acts += zero(rep)
            for kind, v in ops:
                if kind == "set":
                    acts.append({"op": "set_reputation", "rep": rep, "axis": "fame", "value": v})
                else:
                    acts.append({"op": f"{kind}_reputation", "rep": rep, "axis": "fame", "size": v})
            acts.append({"op": "get_reputation", "rep": rep, "fields": ["fame", "pct.fame", "threshold.mixed",
                                                                        "threshold.good", "threshold.bad"],
                         "label": f"{n}: {what}"})
        acts += zero(rep)
    # The level boundaries and the tier grid (Novac, scale 20), both axes set and read in
    # one script.
    fracs = [0, 0.1, 0.149, 0.15, 0.3, 0.499, 0.5, 0.75, 0.999, 1, 1.5]
    s = SCALE[NOVAC]
    points = [(f, 0) for f in fracs] + [(0, f) for f in fracs[1:]]
    points += [(f, i) for f in (0.15, 0.5, 1) for i in (0.15, 0.5, 1)]
    points += [(0.149, 0.5), (0.5, 0.149), (0.3, 0.999), (0.999, 0.3), (1, 0.499), (0.499, 1), (0.1, 0.1)]
    for f, i in points:
        acts.append({"op": "set_reputation_read", "rep": NOVAC, "fame": round(f * s, 6), "infamy": round(i * s, 6),
                     "fields": ALL_FIELDS, "label": f"Novac: fame {f:g} x scale, infamy {i:g} x scale"})
    acts += zero(NOVAC)
    given = [{"kind": "reputation", "rep": r, "fame": 0, "infamy": 0} for r in (WHITE_GLOVE, NOVAC, FREESIDE)]
    return doc("Reputation: AddReputation and RemoveReputation per deed size, SetReputation edges, and "
               "GetReputationThreshold over a fame x infamy grid (White Glove Society, Novac, Freeside)",
               "nvfdoc", given, acts)


def effect_start():
    acts = [
        # The regeneration ability first (no other healing effect active).
        {"op": "damage_av", "of": "player", "av": "Health", "value": 40},
        {"op": "sample_av", "of": "player", "av": "Health", "frames": 30, "label": "health without the ability"},
        {"op": "add_spell", "of": "player", "spell": REGEN},
        {"op": "sample_av", "of": "player", "av": "Health", "frames": 60, "label": "health with the ability"},
        {"op": "remove_spell", "of": "player", "spell": REGEN},
        {"op": "sample_av", "of": "player", "av": "Health", "frames": 30, "label": "health after RemoveSpell"},
        {"op": "restore_av", "of": "player", "av": "Health", "value": 100},
        # Nuka-Cola: ScriptEffectStart adds a cap (no duration).
        {"op": "add_item", "to": "player", "item": NUKA_COLA, "count": 1},
        {"op": "use_item", "actor": "player", "item": NUKA_COLA, "read": [{"item": CAPS}, {"item": NUKA_COLA}],
         "label": "Nuka-Cola used: in the same script"},
        {"op": "get_item_count", "of": "player", "item": CAPS, "label": "caps after the Nuka-Cola"},
        # Contreras' supplies (duration 0): Start, Update and Finish restore the limbs.
        {"op": "add_item", "to": "player", "item": CONTRERAS, "count": 1},
        {"op": "damage_av", "of": "player", "av": "PerceptionCondition", "value": 40},
        {"op": "get_av", "of": "player", "name": "PerceptionCondition", "label": "head before the supplies"},
        {"op": "use_item", "actor": "player", "item": CONTRERAS,
         "read": [{"av": "PerceptionCondition"}, {"item": CONTRERAS}], "label": "supplies used: in the same script"},
        {"op": "damage_av", "of": "player", "av": "PerceptionCondition", "value": 20},
        {"op": "sample_av_damage", "of": "player", "av": "PerceptionCondition", "damage": 5, "frames": 10,
         "label": "head each frame after the supplies (read, then damaged by 5)"},
        {"op": "restore_av", "of": "player", "av": "PerceptionCondition", "value": 100},
        # Cateye (120 s): Start sets Nighteye to 1.
        {"op": "add_item", "to": "player", "item": CATEYE, "count": 1},
        {"op": "use_item", "actor": "player", "item": CATEYE, "read": [{"av": "Nighteye"}, {"item": CATEYE}],
         "label": "Cateye used: in the same script"},
        {"op": "get_av", "of": "player", "name": "Nighteye", "label": "Nighteye after the Cateye"},
    ]
    given = [{"kind": "item_count", "of": "player", "item": CAPS, "count": 18},
             {"kind": "item_count", "of": "player", "item": NUKA_COLA, "count": 0},
             {"kind": "item_count", "of": "player", "item": CONTRERAS, "count": 0},
             {"kind": "item_count", "of": "player", "item": CATEYE, "count": 0},
             {"kind": "actor_value", "of": "player", "av": "Nighteye", "value": 0},
             {"kind": "actor_value", "of": "player", "av": "PerceptionCondition", "value": 100},
             {"kind": "actor_value", "of": "player", "av": "Health", "value": 200}]
    return doc("Effect scripts of aid items: ScriptEffectStart inside the use, a zero-duration effect after the "
               "use, ScriptEffectUpdate of an ability per frame", "nvfdoc", given, acts)


def effect_duration():
    acts = [
        {"op": "add_item", "to": "player", "item": CATEYE, "count": 4},
        {"op": "use_item", "actor": "player", "item": CATEYE, "read": [{"av": "Nighteye"}], "stopwatch": True,
         "label": "Cateye 1 used"},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 20000,
         "label": "Cateye 1: ScriptEffectFinish"},
        {"op": "use_item", "actor": "player", "item": CATEYE, "read": [{"av": "Nighteye"}], "stopwatch": True,
         "label": "Cateye 2 used"},
        {"op": "wait_frames", "frames": 300},
        {"op": "hold_menu", "via": DRESSER, "frames": 900},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 20000,
         "label": "Cateye 2 (container menu open for 900 frames): ScriptEffectFinish"},
        {"op": "use_item", "actor": "player", "item": CATEYE, "read": [{"av": "Nighteye"}], "stopwatch": True,
         "label": "Cateye 3 used"},
        {"op": "wait_frames", "frames": 600},
        {"op": "use_item", "actor": "player", "item": CATEYE, "read": [{"av": "Nighteye"}],
         "label": "Cateye 4 used while 3 is active"},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 20000,
         "label": "Cateye 3 and 4: first ScriptEffectFinish"},
        {"op": "set_av", "of": "player", "name": "Nighteye", "value": 1},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 3000,
         "label": "Cateye 3 and 4: a second ScriptEffectFinish"},
    ]
    given = [{"kind": "item_count", "of": "player", "item": CATEYE, "count": 0},
             {"kind": "actor_value", "of": "player", "av": "Nighteye", "value": 0}]
    return doc("Effect script duration: Cateye's ScriptEffectFinish after 120 s, with a container menu held open, "
               "and with a second Cateye taken while the first is active", "nvfdoc", given, acts)


def script_order():
    acts = [
        {"op": "go_to_cell", "cell": "Goodsprings"},
        {"op": "start_quest", "quest": "BeatrixFX"},
        {"op": "sample_jumps", "reads": [{"global": "GameHour"}, {"ref": WINDOWS[0], "var": "Time"},
                                         {"ref": WINDOWS[1], "var": "Time"}],
         "frames": 900, "min_step": 0.5, "label": "the hour BeatrixFX adds, seen by the windows' scripts"},
        {"op": "get_quest", "quest": "BeatrixFX", "fields": ["running", "var.bump", "var.reset"],
         "label": "BeatrixFX after it ended"},
    ]
    given = [{"kind": "quest", "quest": "BeatrixFX", "running": False, "vars": {"reset": 0, "bump": 0}}]
    return doc("Quest scripts against object scripts in one frame: BeatrixFX's GameMode adds an hour to GameHour; "
               "two Goodsprings windows copy GetCurrentTime into a variable in their GameMode", "nvftest", given, acts)


def main():
    for name, d in (("reputation", reputation()), ("effect_start", effect_start()),
                    ("effect_duration", effect_duration()), ("script_order", script_order())):
        path = os.path.join(HERE, f"quest3_{name}.actions.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(d, indent=2) + "\n")
        print(f"{path}: {len(d['actions'])} actions")


if __name__ == "__main__":
    main()
