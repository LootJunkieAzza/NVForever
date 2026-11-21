#!/usr/bin/env python3
"""Writes the two held-out scenario inputs of the third round of Layer 5 captures
(docs/spec-notes/layer5-captures.md section 9) next to this file:

  python parity-lab/inputs/layer5/make_inputs3_holdout.py

Kept apart from make_inputs3.py so that the visible generator does not show them.
Forms by FalloutNV.esm FormID (looked up read-only, 2026-10-09): RepNVGreatKhans
0x0011989B (scale 30), RepNVFollowers 0x00124AD1 (scale 50); NVNukaColaQuartz
0x000E2F6D, NVCateye 0x0007EA1B, MS05IceNukaCola 0x00040DCB, Caps001 0x0000000F.
Every `given` is a fact of save nvfdoc read by the round 3 trials and checked again when
the golden is built.
"""
import json
import os

HERE = os.path.dirname(os.path.abspath(__file__))
FRAME = 1 / 60
KHANS, FOLLOWERS = "0x0011989B", "0x00124AD1"
QUARTZ, CATEYE, ICE_NUKA, CAPS = "0x000E2F6D", "0x0007EA1B", "0x00040DCB", "0x0000000F"
ALL_FIELDS = ["fame", "infamy", "pct.fame", "pct.infamy", "threshold.mixed", "threshold.good", "threshold.bad"]


def doc(title, given, actions):
    return {"format": "fnv.script.actions/1", "seed": 0, "frame_seconds": FRAME, "title": title, "save": "nvfdoc",
            "given": given, "actions": actions}


def holdout_a():
    acts = []
    steps = [("add", "fame", 5), ("add", "fame", 5), ("add", "fame", 5), ("add", "infamy", 4), ("add", "infamy", 4),
             ("add", "infamy", 3), ("remove", "fame", 5), ("remove", "fame", 4), ("add", "infamy", 5),
             ("add", "infamy", 5), ("remove", "infamy", 2), ("remove", "fame", 5), ("remove", "fame", 5)]
    for n, (kind, axis, size) in enumerate(steps):
        acts += [{"op": f"{kind}_reputation", "rep": KHANS, "axis": axis, "size": size},
                 {"op": "get_reputation", "rep": KHANS, "fields": ALL_FIELDS,
                  "label": f"Great Khans step {n + 1}: {kind} {axis} size {size}"}]
    acts += [{"op": "set_reputation", "rep": KHANS, "axis": "infamy", "value": 3},
             {"op": "remove_reputation", "rep": KHANS, "axis": "infamy", "size": 3},
             {"op": "get_reputation", "rep": KHANS, "fields": ["infamy", "threshold.mixed", "threshold.good", "threshold.bad"],
              "label": "Great Khans: RemoveReputation infamy size 3 from 3"}]
    for f, i in ((7.5, 0), (7.4, 0), (0, 7.5), (25, 7.5), (7.5, 25), (50, 25), (25, 50), (24.9, 49.9), (60, 60),
                 (12.5, 37.5), (50, 50), (49.9, 7.5), (0, 50), (50, 0), (-5, 30)):
        acts.append({"op": "set_reputation_read", "rep": FOLLOWERS, "fame": f, "infamy": i, "fields": ALL_FIELDS,
                     "label": f"Followers: fame {f:g}, infamy {i:g}"})
    for rep in (KHANS, FOLLOWERS):
        acts += [{"op": "set_reputation", "rep": rep, "axis": "fame", "value": 0},
                 {"op": "set_reputation", "rep": rep, "axis": "infamy", "value": 0}]
    given = [{"kind": "reputation", "rep": r, "fame": 0, "infamy": 0} for r in (KHANS, FOLLOWERS)]
    return doc("Layer 5 round 3 held-out scenario A", given, acts)


def holdout_b():
    acts = [
        {"op": "add_item", "to": "player", "item": ICE_NUKA, "count": 1},
        {"op": "add_item", "to": "player", "item": QUARTZ, "count": 1},
        {"op": "add_item", "to": "player", "item": CATEYE, "count": 1},
        {"op": "use_item", "actor": "player", "item": ICE_NUKA, "read": [{"item": CAPS}, {"item": ICE_NUKA}],
         "label": "item 1 used"},
        {"op": "use_item", "actor": "player", "item": QUARTZ, "read": [{"av": "Nighteye"}, {"item": QUARTZ}],
         "stopwatch": True, "label": "item 2 used"},
        {"op": "wait_frames", "frames": 600},
        {"op": "use_item", "actor": "player", "item": CATEYE, "read": [{"av": "Nighteye"}], "label": "item 3 used"},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 20000, "label": "first end"},
        {"op": "set_av", "of": "player", "name": "Nighteye", "value": 1},
        {"op": "wait_av", "of": "player", "av": "Nighteye", "value": 0, "max_frames": 3000, "label": "second end"},
    ]
    given = [{"kind": "item_count", "of": "player", "item": ICE_NUKA, "count": 0},
             {"kind": "item_count", "of": "player", "item": QUARTZ, "count": 0},
             {"kind": "item_count", "of": "player", "item": CATEYE, "count": 0},
             {"kind": "item_count", "of": "player", "item": CAPS, "count": 18},
             {"kind": "actor_value", "of": "player", "av": "Nighteye", "value": 0}]
    return doc("Layer 5 round 3 held-out scenario B", given, acts)


def main():
    for name, d in (("holdout_a", holdout_a()), ("holdout_b", holdout_b())):
        path = os.path.join(HERE, f"quest3_{name}.actions.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(d, indent=2) + "\n")
        print(f"{path}: {len(d['actions'])} actions")


if __name__ == "__main__":
    main()
