# Audit: S.P.E.C.I.A.L., skills and derived statistics against the Fallout Wiki

2026-10-23. The owner saved two Fallout Wiki (Fandom) pages, "Fallout: New Vegas SPECIAL"
and "Fallout: New Vegas skills" (kept privately in `C:\fnv-private\geckwiki`, not
committed), and asked whether NVForever covers everything they describe, derived
statistics included.

**Rule (owner, 2026-10-23):** where the Fandom pages disagree with the game data (the
masters' records and settings) or with the GECK wiki, the Fandom claim is wrong and is
dropped. Tags below: `[fandom]` for the pages' claims, `[observed]` for the masters (read
with `fnvtool actor values --survey` and `fnvtool esm decode`), `[documented]` for the
GECK wiki export.

## 1. Summary

- **Matches (no work):** the attribute rules (40 points, 1 to 10, values outside ignored),
  the skill formula and the attribute of every skill, tag skills, skill points per level
  (GECK wiki form), perks every 2 levels, the level cap, and the derived statistics Action
  Points, Carry Weight, Critical Chance, Hit Points, Melee Damage, Unarmed Damage, Poison
  Resistance and Rad Resistance. A new character's numbers are identical to the page's.
- **Gaps** (section 4): Companion Nerve, Reload Speed from Agility, the low-Intelligence
  dialogue lines, VATS hit chance (with the skill and Strength requirement penalty), the
  Modify Weapon Strength Req entry point, Heal Rate and Fatigue formulas.
- **Disagreements that need a capture** (section 5): the order of DR and DT, and whether
  Old World Blues' Spineless counts towards perk requirements.
- **Fandom claims dropped** (section 6): skill points for odd Intelligence (the GECK wiki
  differs).

## 2. Derived statistics

Base values for a new character with every attribute at 5, from `fnvtool actor values
--actor player` on FalloutNV.esm [observed] and the Fandom table's formula [fandom].

| Statistic | Fandom formula | Masters' settings [observed] | NVForever | Status |
|---|---|---|---|---|
| Action Points | AGL × 3 + 65 | `fAVDActionPointsBase` 65, `…Mult` 3 | 80 | matches |
| Carry Weight | STR × 10 + 150 | `fAVDCarryWeightsBase` 150, `…Mult` 10 | 200 | matches |
| Critical Chance | LCK % | `fAVDCritLuckBase` 0, `…Mult` default 1 | 5 | matches |
| Hit Points | 95 + END × 20 + level × 5 | `fAVDHealthEnduranceOffset` 0, `fAVDHealthEnduranceMult` 20, `fAVDHealthLevelMult` 5 | 200 (100 + END × 20 + (level − 1) × 5 with Base Health 100) | matches (same value at every level) |
| Melee Damage | STR × 0.5 | `fAVDMeleeDamageStrengthMult` 0.5, `…Offset` 0 | 2.5 | matches |
| Unarmed Damage | Unarmed / 20 + 0.5 | `fAVDUnarmedDamageBase` 0.5, `…Mult` 0.05 | 0.5 + 0.05 × Unarmed | matches |
| Poison Resistance | END × 5 − 5 | none | 20 | matches; the note's [hypothesis] now has the page behind it |
| Rad Resistance | END × 2 − 2 | none | 8 | matches, as above |
| Skill Rate | INT × 0.5 + 10 | `iLevelUp…` present but unused [documented] | GECK wiki form | see section 6 |
| Perk Rate | every even level | `iLevelsPerPerk` absent, default 2 [documented] | 2, 4, 6 … | matches |
| Implant Limit | END − Endurance implant | — | Dr. Usanagi's dialogue and scripts (data) | not checked: runs through the dialogue conditions; no engine rule |
| Companion Nerve | CHA × 5 % to companions' damage and defence | none | — | **gap** (4.1) |
| Reload Speed | 10 / (AGL + 5) | none | reload takes the clip's length | **gap** (4.2) |
| Heal Rate | END, perks, Monocyte Breeder (no formula) | `fAVDHealRateEndurance6…10Bonus` 5, 5, 5, 10, 10 | 0; Monocyte Breeder's script heals | **gap**, small (4.6) |
| Fatigue | END and level (no formula) | none | the record's ACBS Fatigue (200 on the player) | **gap**, small (4.6) |
| Addiction Resistance | perks | — | perk entry points and the Addiction Formula (aid-items.md §5) | matches |
| Damage Threshold, Damage Resistance | armour, perks, chems | — | worn armour, ammo effects, perk hooks (combat-rules.md §2) | matches, but see 5.1 |
| Energy, Fire Resistance | armour, Atomic Cocktail | — | the weapon's damage type picks the resistance (`fnv-combat` damage.rs) | matches |
| Dehydration, Starvation, Sleep | Hardcore meters | — | `crates/fnv-game/src/needs.rs` (status: Hardcore needs done) | matches |

## 3. S.P.E.C.I.A.L. and skills

| Fandom claim | Where NVForever has it | Status |
|---|---|---|
| 5 in each attribute, 5 more to give, 40 in all, 1 to 10 each | vigor-tester.md, `fnv_game::chargen` | matches |
| Bonuses beyond 1 to 10 ignored | actor-values.md §1, `actor_values::capped` [documented: GetThresholdedActorValue] | matches |
| Base stats (no chems or gear) feed perk requirements and skill points | levelup.md §4 reads permanent Intelligence; requirements are the records' own GetPermanentActorValue conditions; implants and traits are unconditional abilities and go to the permanent pool (`aid.rs`, `permanent_pool`) | matches; Spineless see 5.2 |
| Intense Training, implants, Lonesome Road, Four Eyes, Small Frame change base stats | AddSPECIALPoints (`functions::ui`), perk abilities (levelup.md §9) | matches |
| The rank names (Wet Noodle … Two-Headed Coin Flip) | the Vigor Tester's label boards, `vitomatic_special-values1/2.dds` (vigor-tester.md) | matches (game textures) |
| Level cap 30, +5 per add-on | experience.md §3: `iMaxCharacterLevel` 30 [observed] + 5 per add-on | matches |
| Up to two traits or none | `iTraitMenuMaxNumTraits` 2 (chargen.md §1) | matches |
| Three tag skills, +15 each | `fAVDTagSkillBonus` 15 (chargen.md §1) | matches |
| Skill = 2 + 2 × attribute + ⌈Luck / 2⌉ | actor-values.md §3.1 (the GECK wiki's form, the masters' 2.0 bases) | matches |
| Each skill's attribute (Barter CHA, Energy Weapons PER, Explosives PER, Guns AGL, Lockpick PER, Medicine INT, Melee STR, Repair INT, Science INT, Sneak AGL, Speech CHA, Survival END, Unarmed END) | actor-values.md §3.1 | matches all 13. Survival on Endurance was a [hypothesis]; the tag-skill screenshot (END 7, INT 5, Survival tagged, 35) shows it [observed: chargen.md §2], now written there |
| Weapon damage × (100 + skill) / 200 for non-Unarmed weapons | `fDamageSkillBase` 0.5 + `fDamageSkillMult` 0.5 × skill / 100 (combat-rules.md §2) | matches |
| Unarmed skill adds damage instead | UnarmedDamage actor value (combat-rules.md §2) | matches |
| Unmet requirements penalise accuracy and increase wobble | spread: Strength and skill shortfall terms (combat-rules.md §3) | matches outside VATS; VATS see 4.4 |
| Skill checks are pass/fail on a number, failing choices shown in red | dialogue-menu.md ("[Sneak 17/25]", the red frame) | matches |
| XP for passed checks | the lines' scripts' RewardXP (experience.md §2) | matches |
| Skill magazines +10, +20 with Comprehension | the ALCH's own conditioned effects (aid-items.md, skill-books.md) | matches |
| Survival raises the benefit of food | `fMagicSurvivalSkillBase/Mult` (aid-items.md §4) | matches |
| Good Natured: −5 combat, +5 non-combat | its ability (levelup.md §9; smoke `perk-skill-points`) | matches |
| Educated +2 per level; Logan's Loophole | `ADJUST_GAINED_SKILL_POINTS` (levelup.md §9) | matches |
| Low-Intelligence characters (Intelligence under 4) | dialogue.md §5: Low/High Intelligence lines and the Dumb Response not modelled | **gap** (4.3) |

## 4. Gaps

1. **Companion Nerve.** A companion's damage and defence go up by 5 % per point of the
   player's Charisma [fandom]. Nothing in the masters names it [observed: no GMST]. Not
   built; the teammates' combat would read it.
2. **Reload Speed from Agility.** 10 / (AGL + 5) [fandom], so 1.0 at Agility 5. No
   setting in the masters. Reload takes the clip's length (combat-rules.md §2); the perk
   entry points Reload Speed and Equip Speed are waiting on the clips' playback rate
   (perk-entry-points.md). One change would do both.
3. **Low Intelligence.** The INFO flags Low/High Intelligence (dialogue.md §2) and the
   Dumb Response are not used, so a character with Intelligence under 4 sees the normal
   lines.
4. **VATS hit chance.** No hit chances yet (owner: 100 % for now, vats-mode.md), so the
   page's "drastically penalises your VATS accuracy" for unmet requirements is not there.
5. **Modify Weapon Strength Req** (entry point 53, Weapon Handling): not read by the
   Strength term of the spread (perk-entry-points.md).
6. **Heal Rate and Fatigue.** The page names their inputs but gives no formula. Heal
   Rate's base is 0 and the masters' `fAVDHealRateEndurance6…10Bonus` are unread; Fatigue
   is the record's ACBS value. Both stay [hypothesis] until the capture in
   actor-values.md §8 (item 3) runs.

## 5. Disagreements to capture

1. **DR before DT.** The page says damage resistance is applied before damage threshold.
   NVForever takes DT off first and DR from the rest (combat-rules.md §2 step 7,
   [design]). The GECK wiki's Weapon Damage Formula does not give the order. A capture
   that settles it: a hit of known damage on an actor with both DT and DR (an NPC in
   armour given `ModAV DamageResistance 50`), the Health lost compared with both orders.
2. **Spineless and perk requirements.** The page says Old World Blues' Spineless (+1
   Strength) and Reinforced Spine (+2) do not count for perk requirements. NVForever
   puts every unconditional ability of the player in the permanent pool, which
   requirements read. Either their abilities carry a condition, or the original keeps
   them out some other way. Check the records (`fnvtool esm decode` on the two PERKs and
   their SPELs), then `GetPermanentActorValue Strength` with Spineless in a capture.

## 6. Fandom claims dropped

- **Skill points for odd Intelligence.** The page: "For odd Intelligence scores, the
  extra skill point is given on even levels", so Intelligence 1 gets 11 at level 2, and
  its totals (304 for Intelligence 1 without add-ons, 735 for Intelligence 10 with all)
  are 10 + Intelligence / 2 per level. The GECK wiki: Floor(Min(permanent Intelligence,
  9) × 0.5 + 10), plus one when the level and permanent Intelligence are both even
  [documented: Derived Skill Settings]. The GECK wiki wins (owner rule); levelup.md §4
  and `levelup::points_for` already follow it. The owner's level-3 screenshot (12
  points) fits both readings.
- Nothing else on the two pages disagrees with the masters or the GECK wiki.

## 7. What changed with this audit

- actor-values.md §3.1: Survival on Endurance is now [observed] (the tag-skill
  screenshot), not a hypothesis.
- actor-values.md §4: Poison and Rad Resistance now cite the Fandom table as well.
- Nothing in code changed.
