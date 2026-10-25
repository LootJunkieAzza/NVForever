# Spec notes: the logic records' minor fields (ESM audit low-impact list, "Logic")

From the low-impact list of docs/handoff/esm-gap-audit-2026-10-21.md, "Logic". Field
names [documented: xEdit wbDefinitionsFNV.pas QUST, INFO, DIAL, SCPT, GLOB, MESG, MGEF,
SPEL, ENCH, AMEF]. Counts are FalloutNV.esm's [observed 2026-10-23, private survey, not
committed].

## 1. GLOB type: not built, and why

Every global stores a float (FLTV); FNAM types 200 as Short, 4 Long, 14 Float. The type
cannot decide what a global holds: the clock's globals are typed Short (`GameHour` s 12,
`TimeScale` s 30, `GameDay`, `GameMonth`, `GameYear`) and GameHour must hold fractions
[observed]. Globals keep their fractions, as before.

## 2. SCPT "Enabled" (built)

SCHR flag bit 0; one master script has it clear, `l38upgradeGunRack01SCRIPT`, on an
unplaced MISC (`L38UpgradeGunRack1`) [observed]. A script with the flag clear is not
attached to its forms, so it does not run [documented: the flag; hypothesis: what a
disabled script does] (`fnv_game::data`, the SCPT arm).

## 3. AMEF Fatigue Mod (built)

AMEF type 5 is Fatigue Mod [documented: GECK wiki GetAmmoEffectTraitNumeric, "0 Damage
Mod … 5 Fatigue Mod"]. One master effect: `AmmoEffectBeanBagFatigue`, Add 250, on
`Ammo12GaBeanBag` [observed]. A hit by a projectile whose ammo carries it takes the
effect's value (from 0) off the target's Fatigue, as DamageAV Fatigue does, so the target
may be knocked out (unconscious.md) [hypothesis: once a hit; the bean bag fires one
projectile a shot]. `Game::damage_fatigue`; log `combat: <target> loses 250 fatigue to
the ammo`.

## 4. Not built, and why

- **QUST "Allow Repeated Conversation Topics"** (94 of 436 quests): "If this box is
  unchecked, conversations will never use the same topic twice (in the same
  conversation)" [documented: GECK wiki Quest]. The player's dialogue never removes a
  topic once said (said lines darken, dialogue.md), so there is nothing for the flag to
  allow; NPC-to-NPC conversations are where it would apply.
- **INFO Low / High Intelligence** (28 and 20 lines): applied since 2026-10-25 (dialogue.md §2: a dim player, Intelligence at or below `iDialogueDummySpeakThisIntOrBelow`). Was: only listed as INFO flags
  [documented: GECK wiki GetDialogueInfoFlags]; the threshold is not documented, and New
  Vegas gates intelligence lines with conditions.
- **INFO speech challenge** (3 lines in the masters): the roll is not documented, but its
  settings are in FalloutNV.esm: `fSpeechChallenge*`, `iSpeechChallengeDifficulty*`
  (Average 55), `fSpeechAutoSuccessThreshold` (0.9) [observed: `fnvtool esm decode`]. The
  lines always pass.
- **INFO PNAM/TPIC and DIAL INOM/INOA order**: file order matches the masters (it
  matters for mods only).
- **QUST CNAM log entries**: there is no journal page that shows them.
- **MESG "Auto Display"** (31 messages): the help messages it marks are shown by each
  menu already.
- **MGEF Painless, No Hit Effect, Gory Visuals; SPEL SPIT and ENCH ENIT flags**: what
  each changes in play is not documented beyond the names.

## 5. Checks

Smoke `bean-bag-fatigue` (a gecko shot with bean bags loses 250 fatigue a hit and is
knocked out).
