# Perk entry points in combat

ESM gap audit row 2 ([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)),
built in `claude/e2-perk-combat-entry-points`. The general rules of entry points (ranks,
order, the Set/Add/Multiply functions) were first written for aid items
([aid-items](aid-items.md) §10), experience ([experience](experience.md) §2) and skill
points ([levelup](levelup.md) §9); this note adds the argument tabs, the other functions
and the combat entry points. Code: `fnv_game::perks` (evaluation),
`fnv_combat::damage::PerkHooks` / `EntryOps` (the hit), `nvf-play` `combat.rs`
(`hit_perks`, `player_weapon_perk`) and `vats_mode.rs` (AP cost).
Every perk of the masters and whether NVForever acts on it: [perk-audit-2026-10-23](../handoff/perk-audit-2026-10-23.md).

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
| 57 | Modify Chance for Ammo Item | Weapon | Hand Loader × 2 (IsWeaponSkillType Guns on tab 1), Vigilant Recycler × 2 (Energy Weapons) | the chance of a shot's spent item, the ammo's DAT2 percentage (claude/e13-spent-casings, [spent-ammo-items](spent-ammo-items.md)) |
| 8 | Calculate To Hit Chance | Weapon, Target | Commando × 1.25, Gunslinger × 1.25, Sniper × 1.25 (GetVATSValue Targeted Part head on tab 0), Hobbler × 1.25 (legs), Wired Reflexes × 1.1, Fast Shot × 0.8, Trigger Discipline × 1.2 (IsWeaponSkillType on tab 1), Sneering Imperialist × 1.25 (GetInFaction on tab 2), Camarader-E, Lonesome Road | VATS mode's chance of each mark, read when the mark is made with GetVATSValue answering for it (since 2026-10-23, vats-mode.md §17.2) |
| 33 | Has Concentrated Fire | — | Concentrated Fire Set 1 | + `iVATSConcentratedFireBonus` for each earlier mark on the same part (vats-mode.md §17.2) |
| 28, 41 | Mysterious Stranger, Miss Fortune | — | Mysterious Stranger Set 1, Miss Fortune Set 1 | a helper may come for a VATS mode mark (vats-mode.md §18.2, since 2026-10-24) |
| 67 | VATS HelperChance | — | Friendly Help × 2 | the helpers' odds |
| 35 | Player Kill AP Reward | — | Grim Reaper's Sprint + 20 | AP back after a VATS mode sequence that killed (§18.3) |
| 39 | Action Point Regen | — | Nerves of Steel × 1.2, Atomic! (by rads), Ain't Like That Now × 1.25 | the AP's regeneration outside VATS mode (§18.4) |
| 68 | Modify Item Damage | — | Regular Maintenance × 0.5, Full Maintenance × 0.25, Built to Destroy × 1.15 | the wear of the player's weapon by its own shots and blows and by hits on it [hypothesis: armour wear too? not applied] |

Order on a hit [hypothesis where the formula pages are silent]: weapon damage after the
ammo's Damage Mod; critical chance after the formula, then VATS's bonus, then the
target's Modify Enemy Critical Hit Chance; DT: the target's DT, the ammo's DT Mod, the
target's defender entries, less the attacker's entry 58.

Not wired yet, with why:

| Code | Entry point | Perks | Why not |
|---|---|---|---|
| 37 | Reload Speed | Rapid Reload and the others with this entry point | built: the player's reload rate, Agility's 0.5 + 0.1 × Agility through this entry with the weapon's tab; the cycle and the body's reload clips both run at it (combat-rules.md §2, Reload speed) |
| 38, 43 | Equip Speed, Modify Attack Speed | Quick Draw, Fast Shot, Loose Cannon, Slayer, Melee Hacker, Old Vaquero, Causeless Rebel, Trigger Discipline | the player body plays its clips at their own speed; changing only the cycle's timing would fire before the clip does. The reload's layer rate (`WeaponAnim::reload_rate`) is the way to do it for the draw and attack clips too |
| 52 | Knockdown Chance | Super Slam (Set 0.15 / 0.30, IsWeaponInList + GetWeaponAnimType on tab 0) | melee knockdown not built (knockdowns are explosions' only) |
| 53 | Modify Weapon Strength Req | Weapon Handling Set 2 | the Strength requirement's spread penalty is not read from it yet |
| 54 | Modify Aiming Move Speed | Run 'n Gun × 2 | what the factor scales is unclear (the moving spread penalty, by the wiki) [hypothesis needed] |
| 61–66 | the unarmed power attacks' Has … | the unarmed moves | the special moves are not built |

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

## 6. Teammate perks (2026-10-23, playtest list 7 item 44)

- [documented: GECK wiki AddPerk, HasPerk] `player.AddPerk perk 1` (forTeammates)
  gives the perk to the player's teammates instead of the player.
- [observed: the masters' scripts and PERK records] The shipped calls are
  VCG01RemoveDocCarebearModeTriggerSCRIPT's `player.AddPerk CompanionSuite 1` on leaving
  Doc Mitchell's, and the companions' quest rewards in dialogue result scripts (Old
  Vaquero, Hand of Vengeance, Calm Heart, Bonds of Steel, Causeless Rebel, three in the
  add-ons). Each has one rank; their entry points test `GetIsID` of the companion they are
  for on the owner's tab ("Cass does 15% more damage with all Guns"), Calm Heart and
  Companion Suite also carry an Ability. P04CompanionFireQuestSCRIPT removes Companion
  Suite from each companion it fires (`CraigBooneREF.RemovePerk CompanionSuite`).
- Built: `Game::held_perks` gives a teammate (SetPlayerTeammate 1, alive) the teammate
  perks at rank 1, so their entry points apply to its hits, its DT and the rest as the
  player's own do (`Game::entry_point_entries`; combat asks for a hit's perks when the
  attacker or the target is the player or a teammate), and their abilities join its spell
  list while it is a teammate and leave when it is not (`Game::sync_perk_abilities_of`,
  log `perks: <actor>: <perk>'s ability <spell> on` and `perks: <actor>'s ability
  <spell> off`) [hypothesis: held only while a teammate, one rank; a companion's
  RemovePerk needs nothing, it is no longer a teammate when it is called].
- Not built: entry points the engine asks only of the player (attack speed, rate of
  fire: Old Vaquero's and Causeless Rebel's), so those two do nothing yet. An ability of a
  teammate perk the player loses (`player.RemovePerk perk 1`) stays on the teammates [design:
  no shipped script does that].
- Checks: `fnv-game` `tests/perk_combat.rs` `teammate_perks_apply_to_teammates`; smoke
  `teammate-perk`.

## 7. Locks, terminals, sneaking, weight and travel (2026-10-24)

Owner 2026-10-24: "Build the next batch: Infiltrator, Computer Whiz, Silent Running, Pack
Rat, Heavyweight, Hoarder's carry weight and Long Haul" (perk-audit-2026-10-23 §3.3,
§3.4). What each entry point does [documented: GECK wiki Entry Point]; the perks'
entries [observed: FalloutNV.esm, DeadMoney.esm, OldWorldBlues.esm PERK]. None of them has
conditions, and all are the player's (`perks::player_has_entry`, `Game::sync_perk_weights`).

| Code | Entry point | Perk | NVForever |
|---|---|---|---|
| 32 | Ignore Broken Lock: "run when the player attempts to open a broken lock; if non-zero, another chance will be granted" | Infiltrator, Set 1 ("can make one more attempt to pick a broken lock") | a broken lock opens the lockpicking menu once more (`lockpick::opening`); breaking it again spends the try (`Locks::retried`, saved), and it stays broken for good |
| 20 | Ignore Locked Terminal: "run when the player attempts to activate a locked terminal; if non-zero, another chance will be granted" | Computer Whiz, Set 1 ("one extra attempt to hack a locked-down terminal") | a locked-out terminal can be hacked once more (`terminal::opening`); locked out again, the try is spent (`Terminals::retried`, saved as `terminal_retries`) |
| 31 | Ignore Running During Detection: "if non-zero, running will be ignored" | Silent Running, Set 1 | the player's running does not raise the noise it makes to a detector (`fSneakRunningMult` not applied, `detection::Pair::ignore_running`) |
| 55 | Modify Light Items: "if set to a non-zero value, modifies the weight of 'Light' items in the reference's inventory according to fPackRatModifier" | Pack Rat, Set 1 ("items with a weight of 2 or less weigh half as much") | each unit of at most `fPackRatThreshold` weighs × `fPackRatModifier` in the player's load; FalloutNV.esm sets neither setting [observed], so the defaults 2 and 0.5 come from the perk's text [observed] |
| 73 | Adjust Heavy Weapon Weight: "modifies the weight of a Weapon which weighs 10.0 lbs or more (after the DecreaseWeight Weapon Mod effect is applied)" | Heavyweight, Multiply 0.5 | each weapon unit of 10 or more (its mods applied) weighs × the value |
| 22 | Get Max Carry Weight: "run when the actor's max carry weight is calculated" | Hoarder, Add 25 | the player's capacity (`functions::actor::sync_carry`; its −1 SPECIAL ability below 160 lbs was already an ability) |
| 51 | Has Fast Travel Always: "access Fast Travel even when overencumbered" | Long Haul, Set 1 | the Pip-Boy's fast travel takes an over-encumbered player (`pipboy::data::travel`) |

- The weights [design where the wiki is silent]: the rules (`fnv_sim::inventory::
  WeightRules`) are applied to each unit's weight with its mods, in the load the world
  counts (encumbrance, the menus' weight total); the inventory rows still show each item's
  own weight [hypothesis: the original's rows are not checked]. Both rules at once: a
  weapon of 10 or more takes Heavyweight's, a lighter unit Pack Rat's. Kept in step every
  frame with the abilities (`Game::sync_perk_weights`), logged `perks: the player's item
  weights: …` when they change.
- The extra tries [design]: "another chance" is taken as one more menu after the lock
  broke or the terminal locked out, until it breaks or locks out again; leaving the
  menu without failing keeps the try.
- Checks: `fnv-sim` `perk_weight_rules_halve_light_items_and_heavy_weapons`; `fnv-game`
  `perk_combat` `weight_perks_change_the_players_load`,
  `silent_running_ignores_the_players_running`, `a_second_failure_spends_the_extra_try`;
  smoke `perk-infiltrator`, `perk-computer-whiz`, `perk-long-haul`, `perk-weights`.

## 8. Mines, explosions, throws and movement (2026-10-24)

Owner 2026-10-24: "Build the next batch: Splash Damage, Heave Ho!, Light Step, and the
run-speed perks (Travel Light, Tunnel Runner, Implant M-5)" (perk-audit-2026-10-23 §3.2,
§3.5). What each entry point does [documented: GECK wiki Entry Point]; the entries
[observed: the PERK records].

| Code | Entry point | Perks | NVForever |
|---|---|---|---|
| 4 | Calculate Mine Explode Chance: "run when the actor comes within proximity of an armed mine; modifies the chance of an armed mine detonating" | Light Step Set 0; In My Footsteps (Dead Money) Set 0; ED-E's Light Step (Lonesome Road, NVDLC04EDENoMines) Set 0 under GetIsID ED-E | an actor whose chance is 0 does not start a mine's fuse (`nvf-play` `mines::tick`), logged once a mine `mines: <actor> does not set off mine <ref> … (… Light Step)`; the player's own, and a teammate's with the teammate perks. Chances between 0 and 1 are not rolled [design: the masters set only 0] |
| 72 | Adjust Explosion Radius | Splash Damage × 1.25 | the radius of every explosion the player (or a teammate with the perk) causes, before its hits, the pushed bodies and the destructibles are found (`combat::owner_weapon_perk` with the cause's weapon on tab 1); the explosion's look is not scaled [design] |
| 59 | Modify Throwing Velocity: "run when ANY projectile is created; modifies the velocity of ANY projectile spawned by the perk owner (filtered by conditions); use GetWeaponAnimType == 9 to restrain the effect to throwing-type weapons" | Heave, Ho! × 1.5 (no conditions); Loose Cannon × 0.75 (its condition) | every projectile the player fires leaves that much faster, as the page says (Heave Ho!'s entry has no condition, so the player's bullets too); VATS mode's throws aim their arc at that speed |
| 42 | Modify Run Speed: "changes the actor's movement speed" | Travel Light × 1.1 (GetEquipped NVTravelLightBadArmorList, the armours it does not allow), Tunnel Runner × 1.25 (sneaking, the same list), Implant M-5 × 1.2 (sneaking), Atomic! × 1.25, Stealth Suit Mk II × 1.2 | the player's movement, walking, running and sneaking alike (`player::PerkSpeed`, worked out every frame by `combat::perk_speed`), logged `movement: the Modify Run Speed perks make the player move at <p>% (<perks>)`. NPCs' speeds (the Lobotomites' × 0.75) are not changed [not built] |

Checks: `fnv-game` `perk_combat` `mine_blast_throw_and_speed_perks`; smoke
`perk-light-step`, `perk-heave-ho-splash`, `perk-run-speed`.
