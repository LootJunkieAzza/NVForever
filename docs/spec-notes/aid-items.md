# Spec notes: aid items (chems, food, drink, medicine)

What happens when an actor uses an ingestible (ALCH): which effects apply, what they do
to actor values, how strong they are, how long they last, and addiction. Code:
`crates/fnv-game/src/aid.rs` (the rules), `crates/fnv-game/src/game.rs`
(`apply_magic_item_with`, the effects ticking), `crates/fnv-game/src/functions/actor.rs`
(the temporary pool), `engine/nvf-play/src/aid.rs` (the player's use, the log). Tool:
`fnvtool actor aid` (§8). Builds on actor-values.md (the pools) and scripts.md §7.6 (active
effects and effect scripts).

Sources, and what each tag means here:

- **[documented]**: the owner's GECK wiki export (pages Ingestible, Ingestible Settings,
  Addiction Formula, ActorValue, Equip Type, and the function pages GetSpellUsageNum,
  IsSpellTarget, EquipItem, CastImmediateOnSelf, GetBaseEffectFlag, GetRadiationLevel,
  GetIngestibleFlag) or xEdit's record definitions (through `schemas/fnv`).
- **[observed]**: counted in the owner's FalloutNV.esm with `fnvtool actor aid` (§8) on
  2026-10-12, or captured in game where a capture is named.
- **[hypothesis]**: our reading, to be settled by a capture (§9).

## 1. The record

ALCH, as xEdit defines it [documented: xEdit ALCH, `schemas/fnv/objects.json`]:

| Field | Meaning |
|---|---|
| `ETYP` | equip type: 10 Chems, 11 Stimpak, 12 Food, 13 Alcohol (the hotkey icon) [documented: Equip Type] |
| `DATA` | weight |
| `ENIT` value | the base price ("Potion Value") |
| `ENIT` flags | 1 No Auto-Calc (unused), 2 Food Item, 4 Medicine [documented: GetIngestibleFlag gives 2 Food, 4 Medicine] |
| `ENIT` withdrawal effect | the actor effect (SPEL) an actor gets when it becomes addicted |
| `ENIT` addiction chance | a number folded into the addiction formula (§5) |
| `ENIT` sound - consume | the sound played on use (audio.md §12 plays it) |
| `YNAM`, `ZNAM` | pick-up and drop sounds |
| `EFID` + `EFIT` + `CTDA`… | the effects, each a base effect (MGEF) with magnitude, area, duration (seconds), range type and actor value, and the conditions that follow it |

- Only base effects with "Self" may be used in ingestibles [documented: Ingestible].
- An ingestible all of whose effects are Detrimental and Hostile is a poison [documented:
  Ingestible]; poisons are not modelled here.

The load order (FalloutNV.esm alone) [observed]: 189 ALCH; equip types Food 129, Chems 34,
Alcohol 15, Stimpak 6, and 5 with code 0 ("Big Guns", which reads as a left default); the
Food flag on 120, Medicine on 10; 34 name a withdrawal effect, 33 have a non-zero
addiction chance; 614 effects.

## 2. The base effect

MGEF `DATA` [documented: xEdit MGEF, `schemas/fnv/gameplay.json`; flag numbers also the
GECK wiki's GetBaseEffectFlag]: flags (0 Hostile, 1 Recover, 2 Detrimental, 4 Self, …,
7 No Duration, 8 No Magnitude), the archetype, and an actor value.

The archetypes of the 614 effects [observed]: Value Modifier 564, Script 26, Value And
Parts 4, Limb Condition 3, Chameleon 2, and 15 effects whose `EFID` 0x0000014F has no MGEF
record in any master (§5.1).

- The `EFIT` actor value and the MGEF's never differ on a Value Modifier or Value And Parts
  effect [observed: 0 of 568]. The code reads the `EFIT`'s, else the MGEF's
  [hypothesis: the same either way in the masters].
- Script effects (26: Nuka-Cola's cap, Cateye, Psycho, Fixer's addiction removal, Super
  Stimpak, Doctor's Bag without Hardcore …) run their effect scripts as scripts.md §7.6
  says; nothing here changes that.
- Chameleon (the Stealth Boy) is not modelled (detection would read it).

## 3. What an effect does to an actor value

The ActorValue page's three pools (actor-values.md §1) [documented: ActorValue]:

- A Value Modifier effect **with Recover** puts its magnitude in the **temporary pool**
  while it lasts, and takes it back when it ends (Buffout's Strength +2 for 240 s).
  - Exception: an Ability (SPEL type 4) given to the **player** whose effect has **no
    conditions** puts it in the **permanent pool** [documented: ActorValue, "used for
    example by the Life Giver perk"]. RemoveSpell takes it back [hypothesis].
  - A negative temporary modifier does not take S.P.E.C.I.A.L. or a skill below 0; the
    magnitude is cut when the effect starts and not updated afterwards [documented:
    ActorValue]. The code gives back exactly what it took.
- A Value Modifier effect **without Recover** goes to the **damage pool**: a Detrimental
  one damages the value, the others restore it (never past the permanent value)
  [documented: ActorValue, "base effects … which don't use the Recover flag"].
  - With duration 0 it acts at once [observed: duration-0 effects finish inside the call,
    scripts.md §7.6].
  - With a duration it acts **magnitude per second** while it lasts [hypothesis: the
    Hardcore Stimpak restores 5 for 6 s where the normal one restores 30 at once; Nuka-Cola
    2 for 25 s]. An ability (no duration) without Recover acts once [hypothesis].
- The **sign**: Detrimental lowers, the rest raise or restore [hypothesis: the effect names,
  "Increase …"/"Restore …" without the flag, "Reduce …"/"Damage …" with it].
- **Values that grow** — Rad Level (rads), Dehydration, Hunger, Sleep Deprivation — go the
  other way: a "Damage" effect raises them and a "Restore" effect lowers them, never below
  0 [hypothesis: RadAway's effect is "Restore Rads", Nuka-Cola's "Damage Rads"; the GECK
  wiki's GetRadiationLevel reads the player's rads with GetAV]. The change goes to the
  value's base, where SetActorValue would put it [hypothesis].
- **Value And Parts** ("Restore Health & Conditions", the normal Stimpak and Super
  Stimpak) restores the value and every limb condition by the same amount [hypothesis from
  the effect's name].
- **Limb Condition** ("NVRestoreLimbs": Doctor's Bag at once, Hydra 10 a second for 60 s)
  names no actor value: it restores every limb condition [hypothesis].
- The value changes of the masters' aid items [observed]: 257 at once, 173 modifiers while
  they last, 138 by the second.

What reads the temporary pool now: `GetActorValue` and conditions, the Pip-Boy's STATS page
(values, HP and AP), the HUD's health and AP bars (their maximum includes it). Not yet: the
carry weight rule and the menus, which read S.P.E.C.I.A.L. from the Layer 4 world (Buffout's
+2 Strength does not raise carry weight); combat's limb pools (they read the permanent
Health).

## 4. Potency

The beneficial effects of an ingestible flagged Medicine or Food grow with the user's skill
[documented: Ingestible Settings; Ingestible]:

```
Medicine:  fMagicMedicineSkillBase + fMagicMedicineSkillMult × Medicine / 100   (1, 2)
Food:      fMagicSurvivalSkillBase + fMagicSurvivalSkillMult × Survival / 100   (1, 2)
```

- It multiplies the magnitude of Value Modifier effects "not flagged as Hostile"
  [documented: Ingestible, with a {{check}} on the page]; the code also leaves Detrimental
  ones alone [hypothesis], and the durations [documented: EquipItem, "effects … will
  scale"; hypothesis that only magnitudes do].
- The skill is the user's current value within 0–100 [hypothesis: the wiki says "the
  Player's"; an NPC's own skill is used for NPCs].
- With both flags the two multiply [hypothesis: the page says only that they are "NOT
  mutually exclusive"].
- A new character (Medicine 15) heals 30 × 1.3 = 39 with a Stimpak.

## 5. Usage, addiction and withdrawal

### 5.1 The usage monitor

- Chems carry an effect `EFID` 0x0000014F that no master defines [observed: 15 items;
  magnitude 30 and duration 108,000 s on 12 (Buffout, Jet, Mentats, Psycho, Med-X's
  Morphine, Rocket, Slasher, Rebound, Steady …), 12 / 2,880 on two, 20 / 3,600 on Ant
  Nectar]. It is the "UMON" usage monitor of the Addiction Formula page [hypothesis:
  GetSpellUsageNum's example returns "the number of times Psycho has been used, multiplied
  by 30", the magnitude these effects have].
- GetSpellUsageNum returns the uses while the monitor is active, times its magnitude
  [documented]. Modelled: the monitor is an active effect of the item; a use while it is
  active adds the magnitude to its count and its duration to its time (the rule for the
  same item again, scripts.md §7.6) [hypothesis]; when it ends the count is gone.
  `GetSpellUsageNum` (opcode 0x122B [documented: catalogue]) is implemented.
- Not modelled: monitors "set to decrease with time" [documented: the function's note
  that it does not account for them].

### 5.2 The addiction roll

[documented: Addiction Formula; Ingestible]

- No withdrawal effect, or an Addiction Chance of 0: no addiction.
- Without a usage monitor: the chance is the Addiction Chance (0.1 = 10 %).
- With one: no chance while GetSpellUsageNum is below `fAddictionUsageMonitorThreshold`
  (default 70); from there `(usage − threshold) / 100`. The record's Addiction Chance is
  then ignored. The usage counted is the value after this use [hypothesis], so Buffout's
  third dose within 30 hours (90) addicts with 20 %, the sixth (180) for sure.
- The roll is made on every use while the actor is not already addicted (seeded game RNG).
- Not applied: the "Modify Addiction Chance" perk entry point (Chem Resistant) — perk entry
  points are not modelled yet.

### 5.3 Withdrawal

- Becoming addicted gives the item's withdrawal effect (a SPEL of type Addiction, 10)
  as AddSpell does. `IsSpellTarget Withdrawal…` is 1 from then on, also while the chem is
  active [documented: IsSpellTarget's notes].
- The addiction's value effects are held back while an effect (other than the usage
  monitor) of a chem whose withdrawal it is is active on the actor, and apply when it ends
  [hypothesis: IsSpellTarget's note, an addicted player who has just had the chem "is not
  presently undergoing withdrawal"].
- Withdrawal effects are Value Modifiers with Recover and Detrimental (Buffout Withdrawal:
  Strength −1, Endurance −1) [observed], so they sit in the temporary pool.
- Their conditions (e.g. GetActorValue Ignore Crippled Limbs == 0 on Buffout's) are read
  when the spell is added, as for any effect (scripts.md §7.6); they are not evaluated
  again later [design: the masters' are constant in play].
- Addictions do not wear off with time here [hypothesis: nothing in the export says they
  do].

### 5.4 Cures

- Fixer removes addictions with a script effect (`AddictionRemovalEffectSCRIPT`)
  [observed], so RemoveSpell (scripts.md §7.6) does it.
- The Cure Addiction archetype (31) removes every addiction of the target [hypothesis
  from its name]; no aid item of the masters uses it [observed].

## 6. Effect timers

- Effect durations are seconds of game-mode time: they stop under menus and in
  conversations and do not count under loading screens [observed: scripts.md §7.6,
  Cateye's 120 s].
- The Sleep / Wait menu does not advance them here [hypothesis to check: whether waiting
  hours makes chems wear off; wait-menu.md].
- The same item used again while its effect is active extends that effect (one effect,
  one end) [observed: scripts.md §7.6]; a modifier is therefore not doubled by a second
  dose [hypothesis follows from it].

## 7. The player's use in the engine

- The inventory menu removes one, plays the consume sound (`ui_sounds`), and sends
  `ItemUsed`; the script runtime calls `Game::ingest` for the player (§4–§5).
  `EquipItem` of an aid item by a script does the same [documented: EquipItem, "treated
  as though it were used normally"].
- Log lines (`engine/nvf-play/src/aid.rs`): `aid: the player used <item> (potency p):
  <changes>; <value> before → after, …`, `aid: <item>'s effects on the player ended: …`,
  `aid: the player is addicted to …`, and at exit `aid summary: …`.
- The Pip-Boy's STATS EFF list shows each aid item with active value changes, with the
  amounts applied (potency included).
- Hardcore mode's Stimpak and RadAway effects are chosen by their conditions (IsHardcore,
  HasPerk); the conditions of the masters' aid effects [observed]: IsHardcore 179,
  HasPerk 147, and a few others (§8).
- Not done: NPCs using stimpaks in combat (the combat workstream's), Hardcore's needs
  rising with time (only the items' changes to them are applied), Chameleon, perk entry
  points.

## 8. Tool

```
fnvtool actor aid --schema schemas/fnv <Data> FalloutNV.esm [--item Stimpak …]
    [--use Stimpak --use Buffout --damage Health:60 --seconds 10 --hardcore]
```

`fnv.actor.aid/1`: every ALCH with its equip type, flags, withdrawal, addiction chance,
consume sound, usage monitor and its effects decoded (archetype, flags, actor value, how the
value changes, conditions by function name); the counts of §1–§5 (`summary`); the
withdrawal spells and their effects. With `--use`: the player in the authored game takes
the damage, uses the items in turn, then plays `--seconds`; the values at each step, the
potency, the usage and the addiction roll.

Checked 2026-10-12 (FalloutNV.esm, Medicine 15 on the new player, normal mode): Stimpak
after 60 damage restores 39 at once; Buffout gives Strength +2, Endurance +3 and Health +60
while it lasts, usage 30, no chance of addiction on the first dose.

## 9. To capture

1. A Stimpak at Medicine 15 and at another value: the health restored (§4), and whether
   crippled limbs heal by the same amount (§3, Value And Parts).
2. Rads before and after RadAway and Nuka-Cola, and GetAV RadiationRads (§3, values that
   grow); in Hardcore, H2O after purified water and Nuka-Cola.
3. Buffout three times within a few minutes: GetSpellUsageNum after each, and whether the
   Strength bonus doubles or the effect lengthens (§5.1, §6).
4. An addiction: the withdrawal's Strength while Buffout is active and after it ends
   (§5.3), and after Fixer.
5. A wait of a few hours with Buffout active: whether its effect ends (§6).
6. Health while Buffout is active and when it ends at low health (whether the drop can
   kill).
