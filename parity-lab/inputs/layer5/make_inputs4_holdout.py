#!/usr/bin/env python3
"""Writes the held-out scenario input of the fourth round of Layer 5 captures
(docs/spec-notes/layer5-captures.md section 10) next to this file:

  python parity-lab/inputs/layer5/make_inputs4_holdout.py

A step of the intro chain from save nvfnew; what it observes is not described here (the
lab's holdout rules, parity-lab/AGENT.md). Forms and givens as make_inputs4.py.
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from make_inputs4 import SKILLS, doc, nvfnew_given, psych_actions  # noqa: E402


def holdout():
    acts = [{"op": "set_quest_var", "quest": "VCG00", "var": "fTimer", "value": 1000},
            {"op": "dismiss_message_boxes", "max": 6, "wait_frames": 600},
            {"op": "get_tag_skills", "skills": SKILLS, "label": "tag skills before"}]
    acts += psych_actions({"nBarter": 3, "nLockpick": 2, "nExplosives": 2, "nEnergyWeapons": 2, "nUnarmed": 1,
                           "nRepair": 1})
    return doc("Layer 5 round 4 held-out intro scenario", nvfnew_given(), acts)


def main():
    path = os.path.join(HERE, "intro_r4_holdout.actions.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(json.dumps(holdout(), indent=1) + "\n")
    print(path)


if __name__ == "__main__":
    main()
