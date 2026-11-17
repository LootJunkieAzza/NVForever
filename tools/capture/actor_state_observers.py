#!/usr/bin/env python3
"""Generate read-only actor snapshots for pending compatibility captures.

The operator prepares a disposable GECK fixture/save and takes separate snapshots;
this generator neither changes game state nor launches a process. Never save over an
existing player save. Use the same actor, inventory, target, time window, difficulty,
weapon, and observed number of shots for each ammo comparison. Record projectile or
hit events externally to distinguish no firing from firing without expenditure.

Pending experiments (all outcomes remain unknown):
* SetTeammateUsingAmmo: teammate on/off x flag 0/1, ordinary and NPCs Use Ammo weapons;
  record ammo before/after actual combat, not only getter readback. Include no ammo,
  one round, multi-round Ammo Use, and an ordinary nonteammate. Reload the same fixture
  between trials, and record the weapon flags and initial inventory privately.
* LockEquipment: unlocked/locked/unlocked trials of AddItem automatic equipment review
  and combat rearming, with identical weak/strong weapons and armor. Observe actual
  equipped items after settling; include explicit EquipItem and UnequipItem controls,
  which accepted captures establish must remain possible while locked.
* SetClass: Auto-calc stats off/on and two classes with different attributes/tag skills
  at the same level; snapshot both a destination and another ref of its base before,
  after, after save/load, and after restarting. Include a Traits template and placed
  leveled NPC. Establish class-sharing and recalculated SPECIAL/skills/Health.
* DuplicateNPCStats: source/destination Auto-calc stats off/on independently, distinct
  classes and levels; snapshot both refs and a sibling of the destination base. Compare
  after the command, settling, save/load and restart. Include damaged/modified AVs to
  determine whether current values or recalculated base values are copied.

Example (IDs belong to an operator-authored fixture, not an original-game record):
  actor_state_observers.py --plugin Probe.esp --actor 300 --ammo 920 \
      --item 910 --item 911 --label ammo.flag0.before -o snapshot.steps.json

Every query is game-level. No raw memory, numeric extender opcode, or binary layout
is probed. Compiling generated queries in the original game is still required.
"""

import argparse
import json
import re


ACTOR_VALUES = (
    "Strength", "Perception", "Endurance", "Charisma", "Intelligence", "Agility", "Luck",
    "Barter", "EnergyWeapons", "Explosives", "Guns", "Lockpick", "Medicine",
    "MeleeWeapons", "Repair", "Science", "Sneak", "Speech", "Survival", "Unarmed", "Health",
)


def form_id(text):
    if not re.fullmatch(r"(?:0[xX])?[0-9a-fA-F]{1,6}", text):
        raise argparse.ArgumentTypeError("expected a nonzero local FormID in hex (up to six digits)")
    value = int(text, 16)
    if not value:
        raise argparse.ArgumentTypeError("FormID must be nonzero")
    return f"{value:X}"


def plugin_name(text):
    if not re.fullmatch(r"[A-Za-z0-9 _.-]+\.(?:esm|esp)", text, re.IGNORECASE):
        raise argparse.ArgumentTypeError("expected a plugin filename without a path or quotes")
    return text


def job(plugin, actor, ammo, items, label):
    head = f'ref rActor\nref rItem\nBegin Function {{ }}\nlet rActor := GetFormFromMod "{plugin}" "{actor}"\n'
    steps = []

    def query(key, expression, item=None, text=False):
        lookup = f'let rItem := GetFormFromMod "{plugin}" "{item}"\n' if item else ""
        steps.append({"query": head + lookup + f"SetFunctionValue ({expression})\nEnd\n",
                      "label": f"{label}.{key}", "field": "value", **({"text": True} if text else {})})

    query("actor", "GetFormIDString rActor", text=True)
    query("valid", "IsFormValid rActor")
    query("class", "GetFormIDString (GetClass rActor)", text=True)
    query("level", "rActor.GetLevel")
    query("teammate", "rActor.GetPlayerTeammate")
    query("uses_ammo", "rActor.GetTeammateUsingAmmo")
    query("dead", "rActor.GetDead")
    query("in_combat", "rActor.IsInCombat")
    if ammo:
        query("ammo_count", "rActor.GetItemCount rItem", ammo)
    for item in dict.fromkeys(items):
        query(f"item.{item}.count", "rActor.GetItemCount rItem", item)
        query(f"item.{item}.equipped", "rActor.GetEquipped rItem", item)
    for actor_value in ACTOR_VALUES:
        query(f"av.{actor_value}.base", f"rActor.GetBaseActorValue {actor_value}")
        query(f"av.{actor_value}.current", f"rActor.GetActorValue {actor_value}")
    return {"format": "fnv.capture.steps/1", "name": label, "title": "Read-only actor state snapshot",
            "start": "Operator-prepared disposable fixture; see generator help for comparison protocol",
            "steps": steps}


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument("--plugin", required=True, type=plugin_name)
    parser.add_argument("--actor", required=True, type=form_id)
    parser.add_argument("--ammo", type=form_id)
    parser.add_argument("--item", action="append", default=[], type=form_id)
    parser.add_argument("--label", required=True)
    parser.add_argument("-o", "--out", required=True)
    args = parser.parse_args()
    if not re.fullmatch(r"[A-Za-z0-9_.-]+", args.label):
        parser.error("label must contain only letters, numbers, underscore, dot and hyphen")
    with open(args.out, "w", encoding="utf-8", newline="\n") as output:
        json.dump(job(args.plugin, args.actor, args.ammo, args.item, args.label), output, indent=2)
        output.write("\n")


if __name__ == "__main__":
    main()
