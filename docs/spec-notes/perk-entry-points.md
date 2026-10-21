# Perk entry points in combat

ESM gap audit row 2 ([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)),
built in `claude/e2-perk-combat-entry-points`. The general rules of entry points (ranks,
order, the Set/Add/Multiply functions) were first written for aid items
([aid-items](aid-items.md) §10), experience ([experience](experience.md) §2) and skill
points ([levelup](levelup.md) §9); this note adds the argument tabs, the other functions
and the combat entry points. Code: `fnv_game::perks` (evaluation),
`fnv_combat::damage::PerkHooks` / `EntryOps` (the hit), `nvf-play` `combat.rs`
(`hit_perks`, `player_weapon_perk`) and `vats_mode.rs` (AP cost).

## 1. Entries, functions and ranks

- A PERK effect of type 2 (entry point) has DATA `point, function, parameter type`,
  conditions in tabs (PRKC before the CTDA), EPFT (the kind of EPFD) and EPFD
  [documented: xEdit wbDefinitionsFNV.pas PERK; observed: the layouts, aid-items.md §10].
- The entry point codes 0–73 and their names [documented: GECK wiki
  GetNthPerkEntryFunction, "Entry Function Codes"]; what each modifies [documented: GECK
  wiki Entry Point, its table].
- Functions [observed: every PERK of FalloutNV.esm and the five add-ons]: only 1, 2, 3
  are used on numbers, always with EPFT 1 and one float (Set 28 times, Add 14, Multiply
  96 in FalloutNV.esm); 8 with EPFT 3 (a leveled list) and 9 with EPFT 4 (a script, the
  Activate choices). The names Set Value, Add Value, Multiply Value [hypothesis: from the
  values: 0/1 flags with 1, bonuses with 2, factors with 3].
- The GECK's other number functions, numbered by its Function list's order, which puts
  Add Leveled List eighth and Add Activate Choice ninth as the data shows [hypothesis for
  every code and parameter layout below: no record of the masters uses them]:
  4 Add Range To Value (EPFD two floats, low and high: a number rolled between them is
  added), 5 Add Actor Value Mult (EPFD the actor value code as a 4-byte integer, then a
  float factor: the owner's actor value × the factor is added), 6 Absolute Value, 7
  Negative Absolute Value, 12 Set to Actor Value Mult, 13 Multiply Actor Value Mult, 14
  Multiply 1 + Actor Value Mult. `fnv_game::perks::resolve` turns each into an `Op`
  (Set, Add, Mul, Abs, NegAbs) when it is used; Add Range rolls on the game's RNG.
- Of a perk with several ranks only the highest owned rank's entries apply; entries apply
  by ascending priority, then perk FormID [hypothesis, as aid-items.md §10].
- Only the player has perks (AddPerk on another actor is refused), so only the player's
  hits, the hits the player takes and the player's weapon figures change [design: as
  before; the GECK wiki GetPerkModifier says companions answer by the player's companion
  perks, not built].

## 2. The combat entry points

Tabs [documented: GECK wiki GetPerkModifier, the list of each entry function's
additional arguments; observed: the masters' entries put their conditions on these
tabs, below]. Tab 0 is always the perk owner. "Perks" are FalloutNV.esm's entries
[observed].

| Code | Entry point | Tabs 1, 2 | FalloutNV.esm perks | NVForever |
|---|---|---|---|---|
| 0 | Calculate Weapon Damage | Weapon, Target | 40 entries: Bloody Mess × 1.05, Cowboy × 1.25 (IsInList on tab 1), Demolition Expert × 1.2/1.4/1.6 (IsWeaponSkillType Explosives on tab 1), Lady Killer / Confirmed Bachelor / Black Widow / Cherchez La Femme × 1.1 (GetIsSex on tab 2), Animal Control and the other challenge perks (GetIsCreatureType on tab 2), Center of Mass × 1.15 (GetVATSValue Targeted Part torso on tab 0), Purifier, Pyromaniac, Laser Commander, Heavy Handed, Unstoppable Force, Spray and Pray × 0.25 (GetInFaction on tab 2) … | the damage before critical damage, every hit by the player (`damage::hit`, `weapon_damage`) |
| 1 | Calculate My Critical Hit Chance | Weapon, Target | Laser Commander + 10, Set Lasers for Fun + 2/+ 4, Ninja × 1.15 | the chance after the formula, before VATS's bonus |
| 2 | Calculate My Critical Hit Damage | Weapon, Target | Better Criticals × 1.5, Hunter × 1.75, Ninja × 1.25 and Stealth Girl × 1.1 and The Professional × 1.2 (GetDetected on tab 0: unseen), Heavy Handed × 0.4 | the weapon's critical damage |
| 34 | Calculate Gun Spread | Weapon | Fast Shot × 1.2, Trigger Discipline × 0.8, Steady Chem Set 0, Gun Strength Fail + 1 | the player's actor spread, not the weapon's minimum spread [documented: GECK wiki Entry Point, "Has no effect on a Weapon's Min Spread in New Vegas"]; the crosshair's cone too |
| 36 | Modify Enemy Critical Hit Chance | Weapon, Target | Dream Crusher × 0.5 | the attacker's chance when the player is hit; tab 2 taken as the attacker [hypothesis: the page says "Target"; no entry has conditions] |
| 40 | Action Point Cost | Weapon | Fast Shot × 0.8, Loose Cannon × 0.7, Math Wrath × 0.9, Plasma Spaz × 0.8, Trigger Discipline × 1.2, Beautiful Beatdown × 0.9 | VATS mode's AP a mark, set on entering it (logged `vats mode: perks change the AP cost`) |
| 56 | Modify Damage Threshold (defender) | Attacker, Attacker's weapon | Toughness + 3/+ 6, Sub-Dermal Armor + 4, Kamikaze − 2, Hit the Deck + 25 (Explosives on tab 2), Stonewall + 5 (melee/unarmed on tab 2), Bonds of Steel + 4 | the player's own DT when hit |
| 58 | Modify Damage Threshold (attacker) | Weapon, Target | Shotgun Surgeon Set 10, Piercing Strike Set 15, Companion Suite Set 10 | the DT the player's hit ignores: from 0, taken off the target's DT, never below 0 [documented: GECK wiki Entry Point, "read as 'How much DT to subtract from the targeted actor'"] |
| 68 | Modify Item Damage | — | Regular Maintenance × 0.5, Full Maintenance × 0.25, Built to Destroy × 1.15 | the wear of the player's weapon by its own shots and blows and by hits on it [hypothesis: armour wear too? not applied] |

Order on a hit [hypothesis where the formula pages are silent]: weapon damage after the
ammo's Damage Mod; critical chance after the formula, then VATS's bonus, then the
target's Modify Enemy Critical Hit Chance; DT: the target's DT, the ammo's DT Mod, the
target's defender entries, less the attacker's entry 58.

Not wired yet, with why:

| Code | Entry point | Perks | Why not |
|---|---|---|---|
| 8 | Calculate To Hit Chance | Commando, Fast Shot, Gunslinger, Wired Reflexes, Sniper, Trigger Discipline | VATS mode never misses (owner, vats-mode.md); no hit chance to change |
| 37, 38, 43 | Reload Speed, Equip Speed, Modify Attack Speed | Rapid Reload, Quick Draw, Fast Shot, Loose Cannon, Slayer, Melee Hacker, Old Vaquero, Causeless Rebel, Trigger Discipline | the player body plays its clips at their own speed; changing only the cycle's timing would fire or end a reload before the clip does. Needs the clips' playback rate (one follow-up) |
| 52 | Knockdown Chance | Super Slam (Set 0.15 / 0.30, IsWeaponInList + GetWeaponAnimType on tab 0) | melee knockdown not built (knockdowns are explosions' only) |
| 53 | Modify Weapon Strength Req | Weapon Handling Set 2 | the Strength requirement's spread penalty is not read from it yet |
| 54 | Modify Aiming Move Speed | Run 'n Gun × 2 | what the factor scales is unclear (the moving spread penalty, by the wiki) [hypothesis needed] |
| 59, 72 | Modify Throwing Velocity, Adjust Explosion Radius | Heave Ho!, Loose Cannon, Splash Damage | not asked for yet; easy follow-ups at the throw and the explosion |
| 35, 39, 57, 61–66 | Player Kill AP Reward, Action Point Regen, Modify Chance for Ammo Item, the unarmed power attacks' Has … | Grim Reaper's Sprint, Nerves of Steel, Hand Loader, Vigilant Recycler, the unarmed moves | the systems they change are not built or are NVForever's own |

## 3. Conditions by tab

Each tab's conditions run on that tab's form, and all tabs' lists must pass (within a
list the OR flags combine as anywhere else) [hypothesis: the GECK keeps one list per
tab]:

- tab 0 on the perk owner, with the other actor of the hit (the target, or the attacker
  for the defender's entries) as the conditions' Target;
- tabs 1 and 2 on the form the entry point gives there (a weapon's base form, an actor's
  reference), with the owner as their Target [hypothesis].

An entry with conditions on a tab the caller has no form for (a shot on an object has no
target; the aid, experience and skill point entry points give no tabs) is not applied
[hypothesis]. A condition on a weapon tab runs on the weapon's base form: IsInList and
IsWeaponSkillType take a WEAP as its own base (`fnv_game::functions::base_of`)
[hypothesis].

## 4. GetVATSValue's query codes

Center of Mass and Sniper test GetVATSValue on tab 0, query 5 with the condition actor
values 26 (torso) and 25 / 31 (head, brain) [observed], so query 5 is Targeted Part, not
the page's sixth entry. The query codes are therefore the GECK wiki page's order with
Weapon Type Is moved last [hypothesis; observed: across FalloutNV.esm's 248 GetVATSValue
conditions codes 0, 2 and 3 take a form, 4, 7, 12–14, 16 and 17 none, 5 a condition
actor value, 6 the numbers 15 or 18, 15 a small number 1–13 like an animation type]: 0
WeaponIs, 1 Weapon In List, 2 Target Is, 3 Target In List, 4 Target Distance, 5 Targeted
Part, 6 VATS Action, 7 Is Success, 8 Is Critical, 9 Crit Effect Is, 10 Crit Effect In
List, 11 Is Fatal, 12 Explode Part, 13 Dismember Part, 14 Cripple Part, 15 Weapon Type
Is. (`fnv_game::vats`; vats-mode.md §12.) GetVATSValue answers only while VATS mode's
shot is leaving, so Center of Mass applies to VATS mode's shots that land while it fires
[hypothesis].

## 5. Checks

- `fnv-combat` `tests/rules.rs` `perk_entry_points_change_the_hit`: each damage entry
  point with the masters' numbers.
- `fnv-game` `tests/perk_combat.rs`: tabs 1 and 2 by weapon and target, the owner's tab,
  ranks, the defender's tabs, every function.
- Smoke `perk-combat-entry-points`: Bloody Mess and Confirmed Bachelor change the
  player's 10mm shot on Easy Pete (12.65 → 14.61, × 1.05 × 1.1), Math Wrath the VATS AP
  cost (17 → 15.3) [observed: run 2026-10-21].
