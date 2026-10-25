#!/usr/bin/env python3
"""Writes the drowning capture job (docs/spec-notes/drowning.md) into this folder: the
rules `sw_breath` (2026-10-08; physics.md section 6.6) left open, measured at the same
Lake Mead spot. It runs unattended (capture_run.py, save `nvfdoc`: the job leaves Doc
Mitchell's house by its door and travels with `coc`, as the swimming jobs do). No game data
is stored here: only script text, the positions of make_swim_jobs.py and two FormIDs of
FalloutNV.esm named below.

  python tools/capture/jobs/make_drown_jobs.py

Trials, each from the surface after a rest long enough to have the whole breath back (god
mode off, Health raised by 5,000 as in sw_breath; one health sample every 3 game loops
unless said otherwise):

A dive is a SetPos onto the lake bed (no keys), the surface a SetPos to the water's height.

- full: a dive of 16 s, health on every game loop (when the damage starts, and whether it
  falls smoothly or by steps).
- recover 0.5 / 2 / 5: a 6 s dive, back at the surface for 0.5, 2 or 5 s, then a dive of
  16 s (how much breath the time at the surface gives back).
- after drowning: a 13 s dive (3 s of damage), 1 s at the surface, an 8 s dive.
- endurance 1 / 10: the full dive with Endurance set to 1, then to 10.
- rebreather: the full dive with the Rebreather's ability `VMS15WaterBreathingActual`
  (SPEL 0x00100630, Water Breathing with magnitude 0) added.
- water breathing value: the full dive with only the actor value Water Breathing set to 1
  by the console, and last its value read back (`GetAV WaterBreathing`; the name follows
  the game's scheme, `NightEye`, `RadiationRads` [hypothesis], so this step is last).

Script functions used, from the GECK wiki [documented]: as make_swim_jobs.py, and
AddSpell / RemoveSpell / SetAV (console).
"""
import json
import os
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
sys.path.insert(0, HERE)

import capture_steps as cs  # noqa: E402
import make_swim_jobs as sw  # noqa: E402

REBREATHER_SPELL = "00100630"   # SPEL VMS15WaterBreathingActual [observed: FalloutNV.esm]
# Game loops: the game runs at about 60 a second [observed: sw_breath's samples].
SECOND = 60
REST = 15 * SECOND


# A message box (GetActiveMenuMode 1001) opens about 0.2 s after the player is first put in
# the water and stops the clock until CloseAllMenus [observed: dr_diag2, 2026-10-25; which
# message is not read]. Menus are closed after every placement; the clock stands while one
# is open, so the samples' game time is unaffected.
CLOSE = [{"frames": 15}, {"console": "CloseAllMenus"}, {"frames": 2}]


def surface():
    dx, dy = sw.DEEP
    return [sw.setpos(dx, dy, sw.WATER_Z), sw.angles(90)] + CLOSE


def rest():
    return surface() + [{"frames": REST}]


# Under water: put on the bed (about 800 below the surface there, make_swim_jobs.py), where
# a player released stays [observed: sw_breath]. Keys are not used: in this job's trials of
# 2026-10-25 a held forward key with the view turned down left the player floating.
BED_Z = sw.WATER_Z - 790.0


def dive(label, seconds, every=3):
    dx, dy = sw.DEEP
    return [sw.setpos(dx, dy, BED_Z)] + CLOSE + sw.samples(label, seconds * SECOND // every, every=every, health=True)


def at_surface(label, frames, every=3):
    return surface() + sw.samples(label, max(1, frames // every), every=every, health=True)


def trial(label, *parts):
    """Menus closed again before the keys and the clock checked: in the first run of this
    job the clock stood still at the surface the whole time although MenuMode read 0 after
    travel, as in sw_walkin's first run and sw_banks_b [observed, 2026-10-25]."""
    clock = [{"console": "CloseAllMenus"}, {"frames": 60}, {"console": "CloseAllMenus"}, {"frames": 30}, sw.num("MenuMode", f"{label} menumode", "value"), sw.remember_t0(),
             {"frames": 30}, sw.num("(GetCurrentTime - %{t0}) * 120000", f"{label} clock", "ms")]
    return rest() + clock + [sw.remember_t0()] + [s for p in parts for s in p]


def setup():
    steps = list(sw.PROTECT) + sw.go_there()
    steps += rest() + [sw.num("player.GetAV Health", "before tgm", "health"), {"console": "tgm"}, {"frames": 10},
                       sw.script("player.ModAV Health 5000"), {"frames": 30}, sw.num("player.GetAV Health", "raised", "health"),
                       sw.num("player.GetAV Endurance", "endurance", "value")]
    return steps


def short():
    """A first try of the setup and the full dive alone (a minute)."""
    steps = setup() + trial("full", dive("full", 16, every=1))
    steps += surface() + [{"frames": 60}, {"console": "tgm"}, {"frames": 10}, sw.num("player.GetAV Health", "end", "health")]
    j = cs.job("dr_short", sw.START, steps, cs.MASTERS)
    j["title"] = "Drowning: one dive of 16 s, health each game loop (about 1.5 min, no input)"
    return j


# (Endurance, dives): the first run (2026-10-25) took 1 twice, 3 and 7; the second 6, 8, 9.
ENDURANCE_DIVES = ((1, 2), (3, 1), (7, 1))
ENDURANCE_DIVES_2 = ((6, 1), (8, 1), (9, 1))


def endurance(name="dr_endurance", dives=ENDURANCE_DIVES):
    """Breath by Endurance: dr_rules held 10 s at Endurance 5 and at 1 (right after 5), and
    14.9 s at 10. Two dives in a row at 1 (does the second hold less?), then at 3 and 7."""
    steps = setup()
    for e, n in dives:
        steps += [{"console": f"player.setav endurance {e}"}, {"frames": 10}, sw.num("player.GetAV Endurance", f"endurance {e}", "value")]
        for i in range(n):
            steps += trial(f"endurance {e} dive {i + 1}", dive(f"endurance {e} dive {i + 1}", 15))
    steps += surface() + [{"frames": 60}, {"console": "tgm"}, {"frames": 10}, sw.num("player.GetAV Health", "end", "health")]
    j = cs.job(name, sw.START, steps, cs.MASTERS)
    j["title"] = "Drowning: breath at Endurance " + ", ".join(f"{e}" + (f" ({n} dives)" if n > 1 else "") for e, n in dives) + " (about 3 min, no input)"
    return j


def rules():
    steps = setup()
    steps += trial("full", dive("full", 16, every=1))
    for label, frames in (("recover 0.5", SECOND // 2), ("recover 2", 2 * SECOND), ("recover 5", 5 * SECOND)):
        steps += trial(label, dive(f"{label} first", 6), at_surface(f"{label} surface", frames), dive(f"{label} second", 16))
    steps += trial("after drowning", dive("after drowning first", 13), at_surface("after drowning surface", SECOND),
                   dive("after drowning second", 8))
    for e in (1, 10):
        steps += [{"console": f"player.setav endurance {e}"}, {"frames": 10}, sw.num("player.GetAV Endurance", f"endurance {e}", "value")]
        steps += trial(f"endurance {e}", dive(f"endurance {e}", 16))
    steps += [{"console": f"player.addspell {REBREATHER_SPELL}"}, {"frames": 30}]
    steps += trial("rebreather", dive("rebreather", 16))
    steps += [{"console": f"player.removespell {REBREATHER_SPELL}"}, {"frames": 30}]
    steps += [{"console": "player.setav waterbreathing 1"}, {"frames": 10}]
    steps += trial("water breathing value", dive("water breathing value", 16))
    steps += surface() + [{"frames": 60}, {"console": "tgm"}, {"frames": 10}, sw.num("player.GetAV Health", "end", "health"),
                          sw.num("player.GetAV WaterBreathing", "water breathing value", "value")]
    j = cs.job("dr_rules", sw.START, steps, cs.MASTERS)
    j["title"] = "Drowning: breath after surfacing, Endurance, Water Breathing, the damage's steps (about 7 min, no input)"
    return j


def main():
    for j in (short(), rules(), endurance(), endurance("dr_endurance2", ENDURANCE_DIVES_2)):
        path = os.path.join(HERE, j["name"] + ".steps.json")
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(j, f, indent=1)
            f.write("\n")
        print(path, len(j["steps"]), "steps")


if __name__ == "__main__":
    main()
