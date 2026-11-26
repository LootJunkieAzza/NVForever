#!/usr/bin/env python3
"""Damage per hit in the original and in NVForever, side by side (issue #3163).

One matrix (tools/capture/observe/damage_parity.matrix.json: weapons, target actor
records, where each is aimed, how the shooter is set up) makes both sides' runs, so the
two measure the same thing: the player, standing on the Searchlight Airport runway,
shoots a restrained target 300 units north, one shot at a time.

  damage_parity.py campaign [-o FILE]            the original's side: an observer campaign
                                                 (tools/capture/observe.py run ...)
  damage_parity.py original OUT.jsonl ... -o ROWS    its outputs as hit rows
  damage_parity.py scenarios -o FILE             NVForever's side: one smoke scenario per
                                                 weapon (python tools/smoke run --scenarios
                                                 FILE --all --no-merge --out DIR ...)
  damage_parity.py nvforever DIR -o ROWS         the logs of that run as hit rows
  damage_parity.py compare ORIGINAL NVFOREVER [--text] [-o FILE] [--fail-on-diff]

How a hit is read:

- The original: every game loop the observer samples each created actor's Health and
  the last hit JIP LN reports for it (GetHitHealthDamage, GetHitLocation,
  GetHitExtendedFlag). A sample whose Health is below the one before it is a hit; its
  row has both the health lost and JIP's figure. The shots are `player.FireWeapon` with
  the equipped weapon, as in the dismember_odds campaign (TapControl does nothing in an
  unfocused game). FireWeapon's damage is a normal shot's [observed 2026-11-21: on the
  unarmoured wastelander's torso the 9mm pistol did its base damage × (0.5 + 0.5 ×
  skill / 100), at Guns 65 and at Guns 50].
- NVForever: the player's own shots (`NVF_PLAY_TEST_LOOK` at the same heights, then
  `NVF_PLAY_TEST_CONTROL` fire, no spread);
  each hit is the log line `combat: hit <target> on <part> for <dealt> (DT <dt>), health
  <after>/<max>`, one per projectile.

A weapon with several projectiles a shot gives one row per projectile in NVForever and,
in the original, one row per sample (the health lost is every projectile that landed in
that game loop, JIP's figure the last one's); `compare` uses the per-projectile figure
of both.

Rows and comparisons hold damage figures measured in the original: they are private
(never committed). Standard library only. Exit codes: 0 done, 1 bad input, 2 rows
differ (`compare --fail-on-diff`).
"""
import argparse
import json
import math
import os
import re
import statistics
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))

MATRIX_FORMAT = "nvf.damage.matrix/1"
ROWS_FORMAT = "nvf.damage.rows/1"
COMPARE_FORMAT = "nvf.damage.compare/1"
HERE = os.path.dirname(os.path.abspath(__file__))
MATRIX = os.path.join(HERE, "observe", "damage_parity.matrix.json")
CAMPAIGN = os.path.join(HERE, "observe", "damage_parity.campaign.json")

# The original's side: game loops (= samples, the campaign samples every loop).
PLACE_EVERY = 10     # the placing script's period
FIRE_EVERY = 40      # one shot per this many loops: time for the projectile to land
IDLE_CYCLES = 2      # fire cycles after a target's last shot before it is removed
# Game loops a second assumed when turning loops into the session's game hours
# [observed 2026-11-21, the first pistol9mm session: 20,218 loops in 3.3 game hours at
# timescale 30, 51 a second, beside four NVForever smoke games]; a low guess only makes
# the session longer than needed.
ASSUMED_FPS = 50.0
TIMESCALE = 30.0

# NVForever's side: seconds on the world aids' clock.
NVF_START = 6.0      # the first target is placed
NVF_SETUP = 3.0      # from placing a target to its first aim group
NVF_SHOT = 1.5       # between shots
NVF_RELOAD = 4.0     # after each aim group
NVF_AFTER = 1.0      # from a target's removal to the next one
# The aid call that marks the start of an aim group in the log (the target's first and
# second): their results are worth having too (the target's level; that it is alive).
NVF_MARKS = ["GetLevel", "GetDead"]

# A difference in damage below this is rounding: the log prints one decimal, JIP's
# figure is a float [design; not a parity tolerance to be widened to pass].
ABS_TOL = 0.06
REL_TOL = 0.005


# The `rule` check: the body part multipliers the matrix's records showed in the original
# (the head's 2, the sentry bot's shielded part's 0.25, every other part 1) [observed
# 2026-11-21]; a hit on a part of another multiplier shows as a misfit to look at. And
# how far NVForever's log rounds: its hit line prints the damage to one decimal and the
# DT to a whole number.
PART_MULTS = [1.0, 2.0, 0.25]
RULE_TOL = 0.011
NVF_DAMAGE_ROUND = 0.05
NVF_DT_ROUND = 0.5
RULE_FORMAT = "nvf.damage.rule/1"


class InputError(Exception):
    pass


def fid(text):
    """A FormID as an int, from '0x0010CD73', '0010CD73' or a decoded '?…' value."""
    if isinstance(text, (int, float)):
        return int(text)
    if not isinstance(text, str):
        return None
    try:
        return int(text.lstrip("?"), 16)
    except ValueError:
        return None


def load_matrix(path=MATRIX):
    with open(path, encoding="utf-8") as f:
        m = json.load(f)
    if m.get("format") != MATRIX_FORMAT:
        raise InputError(f"{path}: not a {MATRIX_FORMAT} document")
    for kind in ("weapons", "targets"):
        keys = [x.get("key") for x in m.get(kind) or []]
        if not keys or len(set(keys)) != len(keys) or not all(isinstance(k, str) and re.fullmatch(r"[a-z0-9_]+", k) for k in keys):
            raise InputError(f"{path}: {kind} need unique keys of a-z, 0-9 and _")
    for w in m["weapons"]:
        if fid(w.get("weapon")) is None or fid(w.get("ammo")) is None:
            raise InputError(f"{path}: weapon {w['key']} needs weapon and ammo FormIDs")
    for t in m["targets"]:
        aims = t.get("aims") or []
        if fid(t.get("base")) is None or not 1 <= len(aims) <= 2 or not all(
                isinstance(a.get("name"), str) and isinstance(a.get("z"), (int, float)) for a in aims):
            raise InputError(f"{path}: target {t['key']} needs a base FormID and one or two aims (name, z)")
    if not (isinstance(m.get("hits"), int) and m["hits"] >= 1):
        raise InputError(f"{path}: hits must be a whole number, 1 or more")
    return m


def shots_of(m, target):
    return m["hits"] * len(target["aims"])


# --------------------------------------------------------------------------- the original

def session_loops(m):
    return sum(2 * PLACE_EVERY + (shots_of(m, t) + 2 + IDLE_CYCLES + 1) * FIRE_EVERY for t in m["targets"])


def session_hours(m):
    hours = session_loops(m) / ASSUMED_FPS * TIMESCALE / 3600.0 * 1.25
    return math.ceil(hours * 10) / 10


def place_script(m):
    """Every PLACE_EVERY loops: remove dead, finished and left-over targets; set up a
    newly placed one (restrained, its back to the player, 300 units north, Health high
    enough to take every shot); turn a creature's or robot's AI off on the runs after
    that, so it stands still [owner 2026-11-21: "toggle the ai when spawning the
    creatures ... make them static"; observed the same day: with the AI turned off in
    the run that moves it, a sentry bot, a deathclaw and a cazador stayed where
    PlaceAtMe had put them, 700 to 3,000 units off]; place the next record when none is
    left. People keep their AI, restrained: with it off, the shots after the first hit
    at each aim height mostly missed them (11 of 24 shots landed on four people, against
    every shot at the unarmed wastelander with its AI on) [observed 2026-11-21;
    hypothesis: the hit leaves a body that no longer animates out of place].

    Actor values used as variables (as dismember_odds does): the player's Variable07 is
    the number of targets placed; a target's Variable01 is 0 new, 1 set up, then one more
    each fire cycle (shot j leaves when it reads 2 + j); Variable06 its number (1 first),
    Variable03 and Variable04 its two aim heights, Variable05 its shots. No variable is
    called rW: a script declaring `ref rW` did not compile, the same script with another
    name did [observed 2026-11-21].

    A person's weapon is taken away, whichever it holds, every time the script runs: a
    restrained person holds a drawn rifle across the chest and the shots aimed at the
    torso hit the weapon instead (hit location 14, no health lost) [observed 2026-11-21,
    the first pistol9mm session: powder ganger, fiend, NCR trooper and veteran ranger].
    Creatures and robots keep theirs (it is part of the body)."""
    setup, place = [], []
    for k, t in enumerate(m["targets"]):
        aims = t["aims"]
        setup += [f"        {'if' if k == 0 else 'elseif'} fK == {k + 1}",
                  f"          rA.SetAV Variable03 {aims[0]['z']:g}",
                  f"          rA.SetAV Variable04 {aims[-1]['z']:g}",
                  f"          rA.SetAV Variable05 {shots_of(m, t)}"]
        place += [f"  {'if' if k == 0 else 'elseif'} fK == {k}", f"    player.PlaceAtMe {{{k}}} 1 {m['place']['distance']} 0"]
    lines = ["let fL := 0", "let fK := player.GetAV Variable07", "let rA := GetFirstRef 200 1", "while rA != 0",
             "  if rA.GetSourceModIndex == 255", "    if rA.GetDisabled == 0",
             "      let fV := rA.GetAV Variable01", "      let fS := rA.GetAV Variable05", "      let fD := 0",
             "      if rA.GetDead", "        let fD := 1",
             "      elseif fV == 0",
             "        rA.SetRestrained 1", f"        rA.SetAV Health {m['target_health']}",
             f"        rA.MoveTo player 0 {m['place']['distance']} 0", "        rA.SetAngle Z 0",
             "        rA.SetAV Variable06 fK"] + setup + ["        endif", "        rA.SetAV Variable01 1",
             "      elseif rA.GetAV Variable06 != fK", "        let fD := 1",
             f"      elseif fV > fS + {1 + IDLE_CYCLES}", "        let fD := 1",
             "      endif",
             "      if fD", "        rA.Disable", "        rA.MarkForDelete", "      else", "        let fL := fL + 1",
             "        if rA.GetIsCreature",
             "          if fV >= 1", "            if fV <= 2", "              rA.SetActorsAI 0", "            endif", "          endif",
             "        else", "          let rGun := rA.GetEquippedObject 5",
             "          if rGun", "            rA.RemoveItem rGun 1 1", "          endif", "        endif",
             "      endif",
             "    endif", "  endif", "  let rA := GetNextRef", "loop",
             "if fL == 0"] + place + ["  endif", f"  if fK < {len(m['targets'])}", "    player.SetAV Variable07 (fK + 1)", "  endif", "endif"]
    return {"decls": ["ref rA", "ref rGun", "float fL", "float fV", "float fK", "float fD", "float fS"],
            "names": [t["base"] for t in m["targets"]], "lines": lines}


def aim_script(m):
    """Every loop: turn the player to the live target, at its first aim height for its
    first `hits` shots and its second after them (the muzzle is `eye` above the player's
    feet, standing [observed 2026-10-25, dismember_odds])."""
    return {"decls": ["ref rA", "ref rTg", "float fA", "float fD", "float fZ", "float fP", "float fH"],
            "lines": ["let rTg := 0", "let rA := GetFirstRef 200 1", "while rA != 0",
                      "  if rA.GetSourceModIndex == 255", "    if rA.GetDisabled == 0", "      if rA.GetDead == 0",
                      "        if rA.GetAV Variable01 >= 1", "          let rTg := rA", "        endif",
                      "      endif", "    endif", "  endif", "  let rA := GetNextRef", "loop",
                      "if rTg", "  let fH := rTg.GetAV Variable03",
                      f"  if rTg.GetAV Variable01 >= {2 + m['hits']}", "    let fH := rTg.GetAV Variable04", "  endif",
                      "  let fA := player.GetAngle Z + player.GetHeadingAngle rTg", "  player.SetAngle Z fA",
                      "  let fD := player.GetDistance rTg",
                      f"  let fZ := rTg.GetPos Z + fH - player.GetPos Z - {m['shooter']['eye']:g}",
                      "  if fD > 1", "    let fP := 0 - fZ / fD * 57.3", "    player.SetAngle X fP", "  endif", "endif"]}


def fire_script(weapon):
    """Every FIRE_EVERY loops: one shot at a target set up a cycle ago that still has
    shots left, and its cycle count goes up."""
    return {"decls": ["ref rA", "float fV", "float fF", "float fS"], "names": [weapon],
            "lines": ["let fF := 0", "let rA := GetFirstRef 200 1", "while rA != 0",
                      "  if rA.GetSourceModIndex == 255", "    if rA.GetDisabled == 0", "      if rA.GetDead == 0",
                      "        let fV := rA.GetAV Variable01", "        let fS := rA.GetAV Variable05",
                      "        if fV >= 1",
                      "          if fV >= 2", "            if fV < fS + 2", "              let fF := 1",
                      "            endif", "          endif",
                      "          rA.SetAV Variable01 (fV + 1)",
                      "        endif", "      endif", "    endif", "  endif", "  let rA := GetNextRef", "loop",
                      "if fF", "  player.FireWeapon {0}", "endif"]}


def ready_script():
    """Standing, weapon out and mended (every shot wears it [observed 2026-11-21: 99.3 to
    88.7 over one session's shots]), and healed instead of god mode (`tgm` is a toggle: a
    second session of one game load would turn it off again)."""
    return {"lines": ["if player.IsSneaking != 0", "  TogglePlayerSneaking 0", "endif",
                      "if player.IsWeaponOut == 0", "  player.SetWeaponOut 1", "endif",
                      "player.SetWeaponHealthPerc 100", "player.RestoreAV Health 1000"]}


def campaign(m):
    """The fnv.observe.campaign/1 of the matrix: one session per weapon. Each session
    needs a game launch of its own (`observe.py run --only`): in one launch the first
    session's shots landed and most of the later sessions' went wide (4 rows of 56 for
    the third) [observed 2026-11-21]."""
    sh = m["shooter"]
    sessions = []
    for w in m["weapons"]:
        do_first = [["player.MoveTo {0} %g %g %g" % tuple(m["place"]["offset"]), m["place"]["marker"]], {"frames": 120},
                    "player.SetAngle Z 0", "player.SetAV Variable07 0",
                    ["player.AddItem {0} 1", w["weapon"]], ["player.AddItem {0} 400", w["ammo"]],
                    ["player.EquipItem {0}", w["weapon"]], ["player.EquipItem {0}", w["ammo"]],
                    "player.SetWeaponHealthPerc 100",
                    # no spread, as NVForever's run has none (NVF_PLAY_TEST_NO_SPREAD): the
                    # weapon record as the running game holds it, nothing on disk
                    # [observed 2026-11-21: with its spread a 9mm pistol missed one shot
                    # in four at a head 300 units away]
                    ["SetWeaponMinSpread 0 {0}", w["weapon"]], ["SetWeaponSpread 0 {0}", w["weapon"]]]
        # SetAV sets the base value; ForceAV, after every SetAV (Luck is part of a skill),
        # makes the value read the one meant whatever the save's character has on top
        # [observed 2026-11-21: Guns read 65 after SetAV alone and the hits were a skill
        # of 65's; 48 with ForceAV before Luck was set]
        do_first += [f"player.{verb} {name} {value:g}" for verb in ("SetAV", "ForceAV") for name, value in sh["values"].items()]
        do_first += [["player.RemovePerk {0}", p] for p in sh.get("remove_perks") or []]
        sessions.append({
            "name": w["key"], "hours": session_hours(m),
            "console_first": ["CloseAllMenus", f"player.SetLevel {sh['level']}"],
            "do_first": do_first,
            "timed": [{"after": PLACE_EVERY, "repeat": True, "script": ready_script()},
                      {"after": PLACE_EVERY, "repeat": True, "script": place_script(m)},
                      {"after": 1, "repeat": True, "script": aim_script(m)},
                      {"after": FIRE_EVERY, "repeat": True, "script": fire_script(w["weapon"])}],
            "note": w.get("note", "")})
    return {
        "format": "fnv.observe.campaign/1", "name": m["name"],
        "title": (f"Issue #{m['issue']}: damage per hit, weapon by weapon, on {len(m['targets'])} actor records placed one "
                  f"at a time {m['place']['distance']} units north of the standing player on the Searchlight Airport runway "
                  f"(restrained, Health {m['target_health']}); {m['hits']} shots at each aim height (player.FireWeapon with "
                  "the equipped weapon at full condition). Per created actor every game loop: Health, DamageThreshold, "
                  "DamageResist, level, and the last hit as JIP LN reports it. Generated by tools/capture/damage_parity.py "
                  "from damage_parity.matrix.json: change the matrix, not this file. Needs JIP LN NVSE. Run the "
                  "sessions one at a time (--only): each needs its own game launch."),
        "save": m["save"],
        "defaults": {
            "fields": "fight", "every": 1, "depth": 1, "settle_frames": 300, "hour": 12, "close_menus_after": 300,
            "extra": [[["dt", "num", "GetAV DamageThreshold"], ["dr", "num", "GetAV DamageResist"],
                       ["level", "num", "GetLevel"], ["cycle", "num", "GetAV Variable01"],
                       ["hloc", "num", "GetHitLocation"], ["hit_hp", "num", "GetHitHealthDamage"],
                       ["hit_limb", "num", "GetHitLimbDamage"], ["hit_crit", "num", "GetHitExtendedFlag 1"],
                       ["hit_sneak", "num", "GetHitExtendedFlag 7"], ["hit_expl", "num", "GetHitExtendedFlag 8"]]],
            "extra_header": [["difficulty", "num", "GetGameDifficulty"], ["p_level", "num", "player.GetLevel"],
                             ["p_guns", "num", "player.GetAV Guns"], ["p_energy", "num", "player.GetAV EnergyWeapons"],
                             ["p_strength", "num", "player.GetAV Strength"], ["p_luck", "num", "player.GetAV Luck"],
                             ["p_weapon_health", "num", "player.GetWeaponHealthPerc"],
                             ["p_ammo", "ref", "GetPlayerCurrentAmmo"], ["p_sneaking", "num", "player.IsSneaking"]],
        },
        "sessions": sessions,
    }


def original_rows(m, sources):
    """Hit rows from the campaign's output files ([(name, lines)])."""
    import observe  # the plugin's output reader; imported here so the other commands need no capture modules
    bases = {fid(t["base"]): t for t in m["targets"]}
    weapons = sorted((w["key"] for w in m["weapons"]), key=len, reverse=True)
    rows, setups = [], []
    for name, lines in sources:
        mods, obs = observe.read_samples(lines, name)
        for ob in obs:
            label = ob.get("label") or ""
            weapon = next((k for k in weapons if label.endswith("_" + k) or label == k), None)
            if weapon is None:
                continue
            parts = [[tuple(x) for x in p] for p in ob["fields"].get("parts", [])]
            head_names = [h[0] for h in ob["fields"].get("header", [])]
            last = {}   # actor id -> (base, cycle count, health) of its sample before
            setup = None
            for s in ob["samples"]:
                head = dict(zip(head_names, s.get("header") or []))
                if setup is None and head:
                    setup = {k: head.get(k) for k in ("difficulty", "p_level", "p_guns", "p_energy", "p_strength", "p_luck",
                                                      "p_weapon_health", "p_ammo", "p_sneaking")}
                    setup.update(side="original", weapon=weapon, source=name)
                    setups.append(setup)
                for raw in (s.get("groups") or {}).get("nearby", []):
                    if not raw or not isinstance(raw[0], str):
                        continue
                    rec = observe.decode_subject(raw, parts, mods)
                    target = bases.get(fid(rec.get("base")))
                    aid, hp, cycle = raw[0].split("|")[0], rec.get("health"), rec.get("cycle")
                    if target is None or hp is None or not isinstance(cycle, (int, float)):
                        continue
                    if rec.get("dead") == 1:
                        last.pop(aid, None)
                        continue
                    # a removed target's FormID goes to a later one [observed 2026-11-21:
                    # three FormIDs served 14 targets]: another base or a cycle count
                    # that fell is a new actor, and its Health is not the old one's
                    was = last.get(aid)
                    before = was[2] if was is not None and was[0] == rec.get("base") and was[1] <= cycle else None
                    last[aid] = (rec.get("base"), cycle, hp)
                    # no shot has left before the count reads 3
                    # ... and a fall below 0.1 is not a shot [observed 2026-11-21: a powder
                    # ganger's Health fell 0.01 to 0.03 every other loop for 60 loops
                    # after it was set to 5,000; the smallest hit seen was 0.6]
                    if before is not None and cycle >= 3 and hp < before - 0.1:
                        # shot j left when the cycle count read 2 + j and was then raised
                        shot = int(cycle) - 3
                        aims = target["aims"]
                        aim = aims[min(shot // m["hits"], len(aims) - 1)]["name"]
                        rows.append({"side": "original", "weapon": weapon, "target": target["key"], "aim": aim, "shot": shot,
                                     "damage": rec.get("hit_hp"), "health_lost": round(before - hp, 4),
                                     "health_before": before, "health_after": hp, "location": rec.get("hloc"),
                                     "critical": rec.get("hit_crit") == 1, "sneak": rec.get("hit_sneak") == 1,
                                     "explosion": rec.get("hit_expl") == 1, "limb": rec.get("hit_limb"),
                                     "dt": rec.get("dt"), "dr": rec.get("dr"), "level": rec.get("level")})
    return {"format": ROWS_FORMAT, "side": "original", "setups": setups, "rows": rows}


# --------------------------------------------------------------------------- NVForever

def ref_of(target):
    """The target as the test aids name it: the newest reference created from its base
    (`fnv_sim::actions::CREATED_PREFIX`). Its FormID is not known ahead: projectiles and
    dropped items are created references too [observed 2026-11-21: the targets of one run
    were FF000000, FF000009, FF000022, …]."""
    return "created:" + target["base"]


def pitch_of(m, z):
    """The view's pitch in degrees (negative looks down) from NVForever's eye to a point
    `z` above the feet of a target `distance` away on level ground: the original's aim
    script does the same sum from its own muzzle height."""
    return math.degrees(math.atan2(z - m["shooter"]["nvf_eye"], m["place"]["distance"]))


def nvf_plan(m, w):
    """(environment, arguments, run seconds) of one weapon's NVForever run."""
    sh, dist = m["shooter"], m["place"]["distance"]
    script = [f"2:set_av:player:{name}:{value:g}" for name, value in sh["values"].items()]
    script.append(f"2:call:player:SetLevel:{sh['level']}")
    # the actors placed near the spot, dead in the original's save
    script += [f"1:kill:{r}" for r in m["place"].get("nvf_kill") or []]
    control, look = ["3:weapon", "4:condition=100"], []
    t = NVF_START
    for target in m["targets"]:
        r = ref_of(target)
        script += [f"{t:g}:call:player:PlaceAtMe:{target['base']}:1:{dist}:0",
                   f"{t + 1:g}:call:{r}:SetRestrained:1", f"{t + 1:g}:call:{r}:SetActorsAI:0", f"{t + 1:g}:set_av:{r}:Health:{m['target_health']}",
                   f"{t + 1:g}:moveto:{r}:player:0:{dist}:0", f"{t + 1.2:g}:call:{r}:SetAngle:z:0",
                   # a `damage` of 0 logs the value as it stands (`call:` takes no actor value name)
                   f"{t + 2:g}:damage:{r}:DamageThreshold:0", f"{t + 2.2:g}:damage:{r}:DamageResist:0"]
        t += NVF_SETUP
        # a slow action (a bolt to work) names its own seconds between shots: `nvf_shot`
        every = w.get("nvf_shot", NVF_SHOT)
        for g, aim in enumerate(target["aims"]):
            script.append(f"{t:g}:call:{r}:{NVF_MARKS[g]}")
            for i in range(m["hits"]):
                at = t + 0.5 + i * every
                # `fire` presses Use, the second lets it go: one shot of a weapon that is not automatic
                look.append(f"{at:g}:0:{pitch_of(m, aim['z']):.2f}")
                control += [f"{at + 0.3:g}:fire", f"{at + 0.45:g}:fire"]
            t += 0.5 + m["hits"] * every
            control.append(f"{t:g}:reload")
            t += NVF_RELOAD
        script.append(f"{t:g}:call:{r}:Disable")
        t += NVF_AFTER
    env = {"NVF_PLAY_TEST_GOD": "1", "NVF_PLAY_TEST_SCRIPT_MENUS": "1", "NVF_PLAY_TEST_NO_SPREAD": "1",
           "NVF_PLAY_TEST_EQUIP": f"{fid(w['weapon']):08X}", "NVF_PLAY_TEST_GIVE": f"{fid(w['ammo']):08X}:400",
           "NVF_PLAY_TEST_SCRIPT": ",".join(script), "NVF_PLAY_TEST_CONTROL": ",".join(control),
           "NVF_PLAY_TEST_LOOK": ",".join(look)}
    args = list(m["place"]["nvf_args"]) + ["--play", "--hour", "12", "--no-voice", "--no-messages"]
    return env, args, math.ceil(t + 5)


def scenario_name(w):
    return "damage-parity-" + w["key"].replace("_", "-")


def toml_str(s):
    return json.dumps(s, ensure_ascii=False)


def scenarios_toml(m):
    """A scenarios file for `tools/smoke run --scenarios`: the runs take the machine's
    smoke slots like any other game launch. It is not part of tools/smoke/scenarios.toml:
    the runs are long and their result is the hit rows, not a pass."""
    out = ["# Generated by tools/capture/damage_parity.py scenarios from damage_parity.matrix.json (issue #%d)." % m["issue"],
           "# Run: python tools/smoke run --scenarios <this file> --all --no-merge --out <dir> --exe ... --data ...", ""]
    for w in m["weapons"]:
        env, args, seconds = nvf_plan(m, w)
        out += ["[[scenario]]", f"name = {toml_str(scenario_name(w))}",
                f"source = {toml_str('issue #%d: damage per hit with %s on %d actor records, for tools/capture/damage_parity.py nvforever' % (m['issue'], w.get('note') or w['key'], len(m['targets'])))}",
                'areas = ["combat"]', 'paths = ["crates/fnv-combat/src/damage.rs"]',
                "env = { " + ", ".join(f"{k} = {toml_str(v)}" for k, v in env.items()) + " }",
                "args = [" + ", ".join(toml_str(a) for a in args) + "]",
                f"run_seconds = {seconds}",
                "expect = [",
                "  { re = 'combat: hit \\S+ on .* for [\\d.]+ \\(DT ', label = \"at least one shot lands on a target\" },",
                "]", ""]
    return "\n".join(out)


ANSI = re.compile(r"\[[0-9;]*m")
HIT = re.compile(r"combat: hit (?P<target>\S+) on (?P<part>.+?) for (?P<dealt>[\d.]+) \(DT (?P<dt>[\d.]+)(?P<crit>, sneak attack critical|, critical)?\), "
                 r"health (?P<health>-?[\d.]+)/(?P<max>[\d.]+)(?P<immune>, takes no damage)?")
VALUE = re.compile(r"test aid: \S+ (?P<name>DamageThreshold|DamageResist) damaged by 0: now (?P<value>-?[\d.]+)")
CALL = re.compile(r"script: test aid calls (?P<func>\w+)(?: on (?P<on>.+?))?(?: = (?P<value>.+))?$")


def number(text):
    try:
        return float(str(text).strip().strip('"'))
    except ValueError:
        return None


def nvf_rows_of(m, weapon, lines):
    """Hit rows of one weapon's log. The log is read in order: each PlaceAtMe call is the
    next target, the two `damaged by 0` lines after it its DT and DR, each NVF_MARKS call
    the start of an aim group; a hit line belongs to the target and aim group it comes
    after."""
    rows = []
    k, target, group, got = -1, None, None, {}
    for line in lines:
        line = ANSI.sub("", line).rstrip()
        v = VALUE.search(line)
        if v:
            got[v["name"]] = float(v["value"])
            continue
        c = CALL.search(line)
        if c:
            func = c["func"]
            if func == "PlaceAtMe":
                k += 1
                target = m["targets"][k] if k < len(m["targets"]) else None
                group, got = None, {}
            elif target is not None and func in NVF_MARKS:
                group = NVF_MARKS.index(func)
                if func == "GetLevel":
                    got["level"] = number(c["value"])
            elif func == "Disable":
                group = None
            continue
        h = HIT.search(line)
        if h and target is not None and group is not None and group < len(target["aims"]):
            dealt, after = float(h["dealt"]), float(h["health"])
            rows.append({"side": "nvforever", "weapon": weapon, "target": target["key"], "aim": target["aims"][group]["name"],
                         "shot": None, "damage": dealt, "health_lost": dealt, "health_before": round(after + dealt, 4),
                         "health_after": after, "location": h["part"], "critical": bool(h["crit"]),
                         "sneak": "sneak" in (h["crit"] or ""), "explosion": False, "limb": None,
                         # the hit's own DT: what the worn armour and the actor value give together, as
                         # the original's GetAV DamageThreshold reads [observed: combat-rules.md §18]
                         "dt": float(h["dt"]), "dt_value": got.get("DamageThreshold"), "dr": got.get("DamageResist"),
                         "level": got.get("level"), "immune": bool(h["immune"])})
    return rows


def nvforever_rows(m, folder):
    rows, setups = [], []
    for w in m["weapons"]:
        path = os.path.join(folder, scenario_name(w) + ".log")
        if not os.path.isfile(path):
            continue
        with open(path, encoding="utf-8", errors="replace") as f:
            found = nvf_rows_of(m, w["key"], f)
        setups.append({"side": "nvforever", "weapon": w["key"], "source": path, "rows": len(found)})
        rows += found
    if not setups:
        raise InputError(f"{folder}: no {scenario_name(m['weapons'][0])}.log or any other weapon's log")
    return {"format": ROWS_FORMAT, "side": "nvforever", "setups": setups, "rows": rows}


# --------------------------------------------------------------------------- compare

def summary(rows):
    """One side's rows of one (weapon, target, aim): the plain hits' damage (not critical,
    no sneak attack, no blast), and what else the rows say."""
    plain = [r["damage"] for r in rows if isinstance(r.get("damage"), (int, float)) and not (r["critical"] or r["sneak"] or r["explosion"])]
    locs = [r["location"] for r in rows if r.get("location") is not None]

    def common(values):
        values = [v for v in values if v is not None]
        return max(set(values), key=values.count) if values else None

    return {"hits": len(rows), "plain": len(plain), "critical": sum(1 for r in rows if r["critical"]),
            "damage": statistics.median(plain) if plain else None,
            "low": min(plain) if plain else None, "high": max(plain) if plain else None,
            "location": common(locs), "locations": sorted({str(x) for x in locs}),
            "dt": common([r.get("dt") for r in rows]), "dr": common([r.get("dr") for r in rows]),
            "level": common([r.get("level") for r in rows])}


def close(a, b):
    return abs(a - b) <= max(ABS_TOL, REL_TOL * max(abs(a), abs(b)))


def compare(m, original, nvforever):
    for doc in (original, nvforever):
        if doc.get("format") != ROWS_FORMAT:
            raise InputError(f"not a {ROWS_FORMAT} document")
    by = {}
    for side, doc in (("original", original), ("nvforever", nvforever)):
        for r in doc["rows"]:
            by.setdefault((r["weapon"], r["target"], r["aim"]), {}).setdefault(side, []).append(r)
    cells, counts = [], {}
    for w in m["weapons"]:
        for t in m["targets"]:
            for aim in t["aims"]:
                got = by.get((w["key"], t["key"], aim["name"]), {})
                o = summary(got["original"]) if got.get("original") else None
                n = summary(got["nvforever"]) if got.get("nvforever") else None
                notes = []
                if o is None and n is None:
                    status = "not run"
                elif o is None or o["damage"] is None:
                    status = "no original"
                elif n is None or n["damage"] is None:
                    status = "no nvforever"
                else:
                    status = "match" if close(o["damage"], n["damage"]) else "differs"
                    for name in ("dt", "dr", "level"):
                        if o[name] is not None and n[name] is not None and abs(o[name] - n[name]) > 0.01:
                            notes.append(f"the target's {name} differs ({o[name]:g} original, {n[name]:g} nvforever): the set-up, not only the hit")
                for side, s in (("original", o), ("nvforever", n)):
                    if s and s["plain"] and not close(s["low"], s["high"]):
                        notes.append(f"{side}: plain hits range {s['low']:g} to {s['high']:g}")
                    if s and len(s["locations"]) > 1:
                        notes.append(f"{side}: hits landed on {', '.join(s['locations'])}")
                counts[status] = counts.get(status, 0) + 1
                cells.append({"weapon": w["key"], "target": t["key"], "aim": aim["name"], "status": status,
                              "original": o, "nvforever": n, "notes": notes})
    return {"format": COMPARE_FORMAT, "matrix": m["name"], "tolerance": {"absolute": ABS_TOL, "relative": REL_TOL},
            "counts": counts, "setups": {"original": original.get("setups"), "nvforever": nvforever.get("setups")}, "cells": cells}


def compare_text(doc):
    def num(s, key):
        return "-" if not s or s[key] is None else f"{s[key]:g}" if isinstance(s[key], (int, float)) else str(s[key])

    out = [f"{'weapon':14} {'target':16} {'aim':6} {'status':13} {'original':>9} {'nvforever':>9} {'DT o/n':>9} {'DR o/n':>9}  where (original / nvforever)"]
    for c in doc["cells"]:
        if c["status"] == "not run":
            continue
        o, n = c["original"], c["nvforever"]
        out.append(f"{c['weapon']:14} {c['target']:16} {c['aim']:6} {c['status']:13} {num(o, 'damage'):>9} {num(n, 'damage'):>9} "
                   f"{num(o, 'dt') + '/' + num(n, 'dt'):>9} {num(o, 'dr') + '/' + num(n, 'dr'):>9}  {num(o, 'location')} / {num(n, 'location')}")
        out += [f"{'':14} {note}" for note in c["notes"]]
    out.append(", ".join(f"{v} {k}" for k, v in sorted(doc["counts"].items())))
    return "\n".join(out)


# --------------------------------------------------------------------------- rule

def ammo_dt(w, dt):
    """The target's DT after the ammunition's DT Mod (`ammo_dt`: ["multiply", 3])."""
    op, value = w["figures"].get("ammo_dt", ["subtract", 0])
    return dt * value if op == "multiply" else max(dt - value, 0.0)


def rule_damage(w, critical, dt, dr):
    """What docs/spec-notes/damage-armour.md's rule makes of one hit before the body
    part's multiplier; `dt` is after the ammunition's DT Mod, `dr` in percent."""
    f = w["figures"]
    before = f["damage"] + (f["crit"] if critical else 0.0)
    return max((before * f.get("ammo_damage", 1.0) - dt) * (1.0 - dr / 100.0), 0.2 * before)


def rule_check(m, doc):
    """Every row of one side against the rule: whether its damage is the rule's figure for
    its own DT and DR times one of PART_MULTS, and which multiplier each body part showed.
    The original's rows carry the target's DT before the ammunition; NVForever's the hit's
    own DT, after it and rounded."""
    if doc.get("format") != ROWS_FORMAT:
        raise InputError(f"not a {ROWS_FORMAT} document")
    weapons = {w["key"]: w for w in m["weapons"]}
    nvf = doc.get("side") == "nvforever"
    fits, misfits, parts = 0, [], {}
    for r in doc["rows"]:
        w = weapons.get(r["weapon"])
        if w is None or "figures" not in w or r.get("sneak") or r.get("explosion") or not isinstance(r.get("damage"), (int, float)):
            continue
        dt, dr = r.get("dt") or 0.0, r.get("dr") or 0.0
        if nvf:
            low = rule_damage(w, r["critical"], dt + NVF_DT_ROUND, dr)
            high = rule_damage(w, r["critical"], max(dt - NVF_DT_ROUND, 0.0), dr)
            tol = NVF_DAMAGE_ROUND + RULE_TOL
        else:
            low = high = rule_damage(w, r["critical"], ammo_dt(w, dt), dr)
            tol = RULE_TOL
        found = next((k for k in PART_MULTS if low * k - tol - 0.0005 * high * k <= r["damage"] <= high * k + tol + 0.0005 * high * k), None)
        if found is None:
            misfits.append({"weapon": r["weapon"], "target": r["target"], "aim": r["aim"], "location": r.get("location"), "critical": r["critical"],
                            "dt": dt, "dr": dr, "damage": r["damage"], "rule": round(low, 3) if low == high else [round(low, 3), round(high, 3)]})
        else:
            fits += 1
            seen = parts.setdefault(f"{r['target']} / {r.get('location')}", {})
            seen[f"{found:g}"] = seen.get(f"{found:g}", 0) + 1
    return {"format": RULE_FORMAT, "matrix": m["name"], "side": doc.get("side"), "fits": fits, "misfits": misfits, "parts": parts}


def rule_text(doc):
    out = [f"{doc['side']}: {doc['fits']} hits fit the rule, {len(doc['misfits'])} do not"]
    for x in doc["misfits"]:
        out.append(f"  {x['weapon']:14} {x['target']:16} {x['aim']:6} on {x['location']}: {x['damage']:g}{' critical' if x['critical'] else ''}, "
                   f"DT {x['dt']:g} DR {x['dr']:g}, the rule gives {x['rule']} before the part")
    out.append("part multipliers seen (target / where: multiplier x hits):")
    out += [f"  {k}: " + ", ".join(f"{mult} x {n}" for mult, n in sorted(v.items())) for k, v in sorted(doc["parts"].items())]
    return "\n".join(out)


# --------------------------------------------------------------------------- command line

def write(doc, path, text=None):
    body = text if text is not None else json.dumps(doc, indent=1) + "\n"
    if path:
        with open(path, "w", encoding="utf-8", newline="\n") as f:
            f.write(body)
    else:
        sys.stdout.write(body)


def read_json(path):
    with open(path, encoding="utf-8") as f:
        return json.load(f)


def main(argv=None):
    p = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    p.add_argument("--matrix", default=MATRIX)
    sub = p.add_subparsers(dest="cmd", required=True)
    q = sub.add_parser("campaign", help="write the original's observer campaign")
    q.add_argument("-o", "--out", default=CAMPAIGN)
    q = sub.add_parser("original", help="hit rows from the campaign's output files")
    q.add_argument("files", nargs="+")
    q.add_argument("-o", "--out")
    q = sub.add_parser("scenarios", help="write NVForever's runs as a smoke scenarios file")
    q.add_argument("-o", "--out")
    q = sub.add_parser("nvforever", help="hit rows from the logs of the smoke run")
    q.add_argument("folder")
    q.add_argument("-o", "--out")
    q = sub.add_parser("compare", help="line the two sides' rows up")
    q.add_argument("original")
    q.add_argument("nvforever")
    q.add_argument("-o", "--out")
    q.add_argument("--text", action="store_true")
    q.add_argument("--fail-on-diff", action="store_true")
    q = sub.add_parser("rule", help="judge one side's rows, each by its own DT and DR, against the measured rule")
    q.add_argument("rows")
    q.add_argument("-o", "--out")
    q.add_argument("--text", action="store_true")
    q.add_argument("--fail-on-misfit", action="store_true")
    a = p.parse_args(argv)
    try:
        m = load_matrix(a.matrix)
        if a.cmd == "campaign":
            write(campaign(m), a.out)
        elif a.cmd == "original":
            sources = []
            for path in a.files:
                with open(path, encoding="utf-8") as f:
                    sources.append((path, f.readlines()))
            write(original_rows(m, sources), a.out)
        elif a.cmd == "scenarios":
            write(None, a.out, scenarios_toml(m))
        elif a.cmd == "nvforever":
            write(nvforever_rows(m, a.folder), a.out)
        elif a.cmd == "rule":
            doc = rule_check(m, read_json(a.rows))
            if a.out:
                write(doc, a.out)
            if a.text or not a.out:
                print(rule_text(doc))
            if a.fail_on_misfit and doc["misfits"]:
                return 2
        else:
            doc = compare(m, read_json(a.original), read_json(a.nvforever))
            if a.out:
                write(doc, a.out)
            if a.text or not a.out:
                print(compare_text(doc))
            if a.fail_on_diff and doc["counts"].get("differs"):
                return 2
    except (InputError, OSError, json.JSONDecodeError) as e:
        print(f"damage_parity: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
