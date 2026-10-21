# Spent ammo items: cases, hulls and drained cells

ESM gap audit row 13 ([esm-gap-audit-2026-10-21](../handoff/esm-gap-audit-2026-10-21.md)),
built in `claude/e13-spent-casings`. Code: `nvf-play` `combat.rs` (`spent_ammo_item`,
called from `fire` where a shot takes its rounds from the inventory); the record is read
by `fnv_combat::data` (`AmmoStats::consumed`).

## 1. The record

- AMMO DAT2 holds Consumed Ammo (a FormID) and Consumed Percentage (a float)
  [documented: the schema `schemas/fnv/objects.json` AMMO DAT2 `consumed_ammo`,
  `consumed_percentage`; GECK wiki Ammo, the dialog's "Ammo Consumed (Percentage)"].
- The percentage is 0–100 [observed: FalloutNV.esm: Ammo10mm gives Case10mm 00140AA7 at
  20, Ammo12Ga Hull12Ga 00140AEB at 40, AmmoSmallEnergyCell DrainedSmallEnergyCell
  0012115E at 40, AmmoFlamerFuel DrainedFlamerFuelTank 0015C631 at 25 (`fnvtool combat
  form`)]. The items are MISC records ("Case, 10mm", "Hull, 12 Gauge", "Drained Small
  Energy Cell" …) [observed].
- Meaning: "The object type, and the percentage odds, that will be randomly created in
  the user's inventory every time the weapon is fired"; the field name means items added
  when ammo is consumed [documented: GECK wiki Ammo, "Ammo Consumed (Percentage)"].

## 2. When it is given

- On every shot of the player's that spends ammo from the inventory, one roll against the
  percentage; on a pass one item goes into the player's inventory, silently
  [documented: the wiki's "every time the weapon is fired"; hypothesis: silently, no
  corner message].
- One roll per shot, also for a weapon that spends several rounds a shot [hypothesis: the
  wiki says per firing; per round spent would be the other reading].
- Not per round reloaded [documented: the wiki ties it to firing].
- Not tied to Hardcore mode [hypothesis: the wiki names no condition; only ammo weight is
  Hardcore's, by the same page].
- Only the player [hypothesis: the wiki says "the user's inventory"; NVForever's NPCs carry
  no inventory ammo (npc-reload), so they would have nothing to roll for]. God mode
  spends no ammo, so it gives nothing; a thrown weapon (it spends itself) has no AMMO.
- The rolls have their own seeded generator (`spent_rng`), so the shots' spread, tracer and
  jam rolls are what they were.

## 3. Perks

- Entry point 57 Modify Chance for Ammo Item, argument tab Weapon [documented: GECK wiki
  GetNthPerkEntryFunction (the code), GetPerkModifier (the tab), Entry Point ("Modifies
  the chance of receiving casings or drained ammunition")].
- Hand Loader: Multiply 2.0 with IsWeaponSkillType Guns on the Weapon tab; Vigilant
  Recycler: Multiply 2.0 with IsWeaponSkillType Energy Weapons [observed: the PERK
  records' PRKE/DATA 0x39 function 3, EPFD 2.0, CTDA function 109 on tab 1].
- The chance after the perks is the percentage the roll is made against (so Hand Loader
  makes a 10mm case 40 %) [hypothesis: the entry modifies the percentage itself, not the
  odds of a second roll].
- Hand Loader's recipe unlock and Vigilant Recycler's workbench recipes are the crafting
  menu's (conditions on the recipes), not this note's.

## 4. What is logged

- `combat: spent ammo item 0x00140AA7 from Ammo10mm given to the player (40 % chance after
  HandLoader)` for each item given; at exit `combat: spent ammo items: N shots rolled, M
  given [00140AA7 x k, …]`. Smoke scenario `spent-ammo-items`.
