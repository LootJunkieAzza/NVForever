#!/usr/bin/env python3
"""Writes the actor value capture job (docs/spec-notes/actor-values.md section 8: what
each actor value of the player, of NPCs and of creatures is in the game, to settle that
note's hypotheses) into this folder. It runs unattended (capture_run.py, save `nvfdoc`;
no manual steps). No game data is stored here: only script text and FormIDs.

  python tools/capture/jobs/make_actor_value_jobs.py

`av_actor_values` reads, for the player and seven Goodsprings actors, the level and all
77 actor values twice: the current value (GetActorValue) and the base value
(GetBaseActorValue). Compare with

  fnvtool actor values --schema schemas/fnv <Data> FalloutNV.esm --actor player \\
      --actor SunnyRef --actor DocMitchellREF --actor EasyPeteRef --actor GSChetRef \\
      --actor RingoRef --actor CheyenneREF --actor GSVictorRef

The actors and what each settles (their stored bytes are in that tool's output):

- the player: skills from the formula; Action Points, the resistances, Fatigue;
- Sunny Smiles (auto-calculated, level 2) and Chet, Ringo (auto-calculated);
- Doc Mitchell and Easy Pete (not auto-calculated, each with one skill offset). Easy
  Pete's stored Big Guns is not what the formula gives, and his Explosives has the
  offset: together they tell "stored value + offset" from "formula + offset";
- Cheyenne (a creature) and Victor (a creature whose stats come from a template): the
  attributes, the Combat Skill as the weapon skills, the skills without a field.

Then, on the player (the run ends without saving):

- Charisma damaged by 2: do Barter and Speech follow?
- Barter modified by 5: the current, base and permanent values;
- Speech set to 50: is the number the value, or added to the formula's?

Script functions used, from the GECK wiki [documented]: GetActorValue,
GetBaseActorValue, GetPermanentActorValue, DamageActorValue, RestoreActorValue,
ModActorValue, SetActorValue, GetLevel. Actor values are given by code, which xNVSE's
compiler accepts in place of the name [documented: GECK wiki GetActorValueAlt, "xNVSE
allows using Actor Value codes"].
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import make_movement_jobs as mm  # noqa: E402

START = "nvfdoc (Doc Mitchell's house); stays inside, nothing is moved."

# Label, reference (global FormID in FalloutNV.esm, from `fnvtool actor values`).
ACTORS = [
    ("player", "player"),
    ("sunny", "0x00104E85"),       # SunnyRef
    ("doc", "0x00104C0F"),         # DocMitchellREF
    ("easy pete", "0x00104C80"),   # EasyPeteRef
    ("chet", "0x00104C79"),        # GSChetRef
    ("ringo", "0x00104C7D"),       # RingoRef
    ("cheyenne", "0x0010588E"),    # CheyenneREF (CREA)
    ("victor", "0x001073E8"),      # GSVictorRef (CREA, stats from its template)
]
CODES = 77          # actor value codes 0 to 76 [documented: xEdit ActorValue]
CHUNK = 11          # values per query; a string of eleven numbers compiled in pc_actor_push
STRENGTH, CHARISMA, BARTER, SPEECH = 5, 8, 32, 43


def values(label, who, function, kind):
    """All 77 values of `who` with `function`, eleven per query, as "v v v …"; the field
    names the codes ("0-10")."""
    out = []
    for first in range(0, CODES, CHUNK):
        codes = list(range(first, min(first + CHUNK, CODES)))
        n = cs.Names(cs.MASTERS, None)
        v = n.operand(who)
        names = [f"fV{i}" for i in range(len(codes))]
        lines = n.setup + [f"let {name} := {v}.{function} {code}" for name, code in zip(names, codes)]
        lines.append("SetFunctionValue (" + ' + " " + '.join(f"($ {name})" for name in names) + ")")
        out.append(cs.query(cs.function(n.decls + [f"float {name}" for name in names], lines), f"{label} {kind}", f"{codes[0]}-{codes[-1]}"))
    return out


def number(label, who, expr, field):
    """`expr` (with `{0}` the actor) as a number."""
    n = cs.Names(cs.MASTERS, None)
    v = n.operand(who)
    return cs.query(cs.function(n.decls + ["float fV"], n.setup + [f"let fV := {expr.format(v)}", "SetFunctionValue fV"]), label, field)


def three(label, code):
    """The current, base and permanent value of one of the player's actor values."""
    return [mm.num(f"player.GetActorValue {code}", label, "current"), mm.num(f"player.GetBaseActorValue {code}", label, "base"),
            mm.num(f"player.GetPermanentActorValue {code}", label, "permanent")]


def actor_values():
    steps = []
    for label, who in ACTORS:
        steps.append(number(label, who, "{0}.GetLevel", "level"))
        steps += values(label, who, "GetActorValue", "current")
        steps += values(label, who, "GetBaseActorValue", "base")
    # Do the formulas follow a damaged attribute?
    steps += three("charisma before", CHARISMA) + three("barter before", BARTER) + three("speech before", SPEECH)
    steps += [mm.script(f"player.DamageActorValue {CHARISMA} 2"), {"frames": 30}]
    steps += three("charisma damaged 2", CHARISMA) + three("barter, charisma damaged 2", BARTER) + three("speech, charisma damaged 2", SPEECH)
    steps += [mm.script(f"player.RestoreActorValue {CHARISMA} 2"), {"frames": 30}]
    steps += three("barter, charisma restored", BARTER)
    # The pools.
    steps += [mm.script(f"player.ModActorValue {BARTER} 5"), {"frames": 30}] + three("barter modified 5", BARTER)
    steps += [mm.script(f"player.ModActorValue {BARTER} -5"), {"frames": 30}] + three("barter modified back", BARTER)
    steps += [mm.script(f"player.ModActorValue {STRENGTH} 1"), {"frames": 30}] + three("strength modified 1", STRENGTH)
    steps += [mm.num("player.GetActorValue 13", "strength modified 1", "carry weight")]
    steps += [mm.script(f"player.ModActorValue {STRENGTH} -1"), {"frames": 30}]
    # SetActorValue on a skill of the player: the value, or added to the formula's?
    steps += [mm.script(f"player.SetActorValue {SPEECH} 50"), {"frames": 30}] + three("speech set 50", SPEECH)
    j = cs.job("av_actor_values", START, steps, cs.MASTERS)
    j["title"] = "Actor values of the player, five NPCs and two creatures: current and base, and the pools on the player (under a minute, no input)"
    return j


def main():
    j = actor_values()
    path = os.path.join(HERE, j["name"] + ".steps.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(j, f, indent=1)
        f.write("\n")
    print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
