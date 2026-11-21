# Spec notes: death items

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was read from the owner's masters (FNV 1.4.0.525), **[hypothesis]** needs
verification, **[design]** is our own choice. ESM gap audit row 5
(docs/handoff/esm-gap-audit-2026-10-21.md).

Code: `crates/fnv-game/src/death_items.rs` (called from `Game::kill` and
`Game::lay_out_corpses`, `combat.rs`), `fnv_sim::World::add_death_item` (`world.rs`), the
field `ActorDef::death_item` (`data.rs`). Tests `crates/fnv-game/tests/death_items.rs`;
scenario `death-item-radroach`.

## 1. The record

- NPC_ and CREA carry `INAM` "Death item", a FormID of a leveled item list (LVLI)
  [documented: xEdit wbDefinitionsFNV.pas NPC_ and CREA; schema `gameplay.json`].
- It belongs to the **Traits** template group: xEdit hides it when the template flag
  Traits is set [documented: xEdit; docs/spec-notes/actors.md §1.1]. So an actor whose
  base has a template and the Traits flag takes the death item of the actor that supplies
  its traits, through templates and leveled actor lists alike
  (`Game::traits_def`, `templates.rs`).
- The masters agree [observed: FalloutNV.esm, a private survey script]: 1,635 NPC_/CREA
  name a death item (658 NPC_, 977 CREA). Of the actors whose template is an NPC_/CREA
  and that have the Traits flag, 514 creatures carry the same INAM as their template;
  14 differ (e.g. `NVCrV19FireGecko` DeathItemGeckoFire50, its template DeathItemGeckoFire100;
  `NVCrGiantMantisEssential` has none, its template DeathItemMantis100). With the
  Traits rule the template's wins in those 14 [hypothesis: the GECK leaves the hidden
  field as it was].
- Typical lists: `DeathItemRaider15` (360 NPC_), `DeathitemBrotherhood100`,
  `DeathItemNCRDogtag`, `DeathItemGecko100` (Use All of LootGeckoHide80, LootGeckoMeat75,
  LootGeckoMeat25, LootGeckoEgg20), `DeathItemRadroach100` (Radroach Meat, chance none 0)
  [observed]. The number in the name is the chance the list gives something, as its
  chance none says.
- The GECK wiki export has no page on the field itself; it names it only through the JIP
  functions GetDeathItem/SetDeathItem ("the Death Item form of a NPC/Creature form")
  [documented: GECK wiki]. Those functions are not built.

## 2. When it is added

- **As the actor dies**, once: the list is rolled and its items go into the body's
  inventory, where looting finds them [hypothesis: the time; nothing documents whether the
  original rolls at death or at spawn, but the masters put items that only a corpse
  should hold in these lists]. In NVForever: `Game::kill`, before the OnDeath blocks run,
  so a script there sees the items [hypothesis: the order]. No OnAdd block runs, as the
  perk entry point's page says of its lists (§3) [hypothesis: the same for INAM].
- **Level**: the list is read at the reference's leveled-list level, the one its carried
  items were rolled at: its encounter zone's level fixed when its cell first loaded, or
  the player's (docs/spec-notes/encounter-zones.md, PR #554, on main) [hypothesis: the
  same level as its carried items]. Chance none, Use All, "each item in count" and
  LVLG globals work as for any list (simulation.md §4).
- **Once per death**: the dead are not killed again (`Game::kill` refuses), and a saved
  game keeps the inventory, so loading adds nothing. An actor raised with ResurrectActor
  that dies again gets a new roll [hypothesis: each death is a death]. A cell reset that
  brings a corpse back alive rolls its inventory again and drops any death item still
  owed (dead-on-spawn.md §4).
- **Placed corpses** (Base Health 0, `Game::lay_out_corpses`, dead-on-spawn.md) get their
  death item too [hypothesis: the masters give `TCFiend01DEAD` one, dead-on-spawn.md §2].
  Their inventory is not made at a new game, so the item is **owed** (`World::add_death_item`
  keeps it, saved in the sim snapshot as `death_items`) and rolled into the inventory
  when that is first made, at the level then fixed for the reference [design].
- An essential actor knocked out does not die (unconscious.md), so it gets nothing.
- The log line: `death item: <ref> (<base>) <list> at level <n>: <item> x<count>, …`
  (`nothing (chance none)` when the roll gave nothing).

## 3. Perks: "Add Leveled List On Death"

- Perk conditions do not touch INAM: the list has no conditions of its own (a leveled
  item list has chance none and its global only) [documented: xEdit LVLI], and no perk
  of the masters is involved. Animal Friend and Mysterious Stranger-style perks work
  through abilities and AI, not through loot.
- Perks reach a body's loot through their own entry point 21, "Add Leveled List On Death":
  "Adds a leveled list to the target's inventory when the player kills someone … without
  causing OnAdd blocks to run" [documented: GECK wiki Perk, the list of entry points;
  code 21: GetNthPerkEntryFunction]; its argument tab is the Target [documented: GECK wiki
  GetPerkModifier, "21: Target"].
- No perk of FalloutNV.esm has one; Old World Blues' `NVDLC03ThemsGoodEatingPerk` and
  Lonesome Road's `NVDLC04CertifiedTechPerk` do, each with function 8 (Add Leveled List)
  and EPFT 3, a leveled list in EPFD [observed: the add-on masters].
- In NVForever: a kill by the player adds, after the death item, the list of each such
  entry of the player's perks whose conditions pass, tab 1 being the victim (the usual
  entry point rules, perk-entry-points.md §1).

## 4. Not built

- JIP GetDeathItem/SetDeathItem.
- Whether the original rolls a placed corpse's death item at all, and when, is not
  checked in game.
