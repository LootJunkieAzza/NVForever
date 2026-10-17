# Spec note: repair

Tags as in `nif.md`: `[documented]` cites a page or file, `[observed]` says what was
looked at, `[hypothesis]` is a reading that nothing has confirmed yet. `[design]` marks
what is NVForever's own, `[owner]` a rule the owner set.

Two ways to repair a weapon or a piece of armour: a vendor does it for caps (the repair
services menu, §2–§4), or the player takes parts from a like item in the Pip-Boy (§5,
not built yet). Code: `crates/fnv-game/src/repair.rs` (the rules, `ShowRepairMenu`),
`engine/nvf-play/src/repair.rs` (the menu), `crates/fnv-sim/src/inventory.rs`
(`Inventory::set_condition_one`).

## 1. Sources

- **The owner's screenshots** of the original, 2026-10-17, kept privately: eight of the
  repair services menu (C:\fnv-private\repairmenu, 1920 x 1080, a vendor with Repair
  100, the player with 18 caps, nine weapons listed) and eight of the Pip-Boy's repair
  (C:\fnv-private\repairmenuPlayer).
- **The game's interface files** in `Fallout - Misc.bsa`: `menus\repair_services_menu.xml`
  (the vendor's menu), `menus\repair_menu.xml` (the Pip-Boy's) and the inventory's
  `menus\main\inventory_menu.xml` (its Repair button), with the shared pieces they
  include (`list_box.xml`, `list_box_template.xml`, `card_info.xml`, `meter.xml`,
  `text_box.xml`, `bottom_bracket.xml`, `vertical_fade_line.xml`, `line.xml`). Described
  here in plain words; nothing of them is shipped.
- **The GECK wiki** (owner's export): ShowRepairMenu, GetMenuTargetRef, Armor, "Item
  Repair Formula", "Item Condition And Value", GetRepairList, Settings.
- **FalloutNV.esm**: the settings (GMST) and the in-game help (MESG `HelpRepair`,
  `HelpWeapons`, `HelpApparel`, `HelpPipBoyItems`), read with a private script.

## 2. Opening the vendor's menu

- `actor.ShowRepairMenu` (alias `srm`) "opens the repair menu so that the player may
  repair their equipment by paying caps. The repair skill of the calling actor will be
  used, and any caps spent will be added to that actor's inventory." Called without a
  reference (a quest script, the console) "nothing will happen". Called on the player,
  the caps spent come back to the player, "but they must have enough caps in their
  inventory before they can repair an item" [documented: GECK wiki ShowRepairMenu].
- The menu is MenuMode 1058 [documented: GECK wiki GetMenuTargetRef, "1058 The actor
  (vendor) whose skills are used for repairing"]. Scripts see it open until the player
  leaves it.
- Ten quests' scripts or dialogue call it, Raul's (`VDialogueRaul`) among them
  [observed: docs/design/script-function-coverage.md].

## 3. The vendor's menu

**What the file says** [documented: repair_services_menu.xml], in its units (960 to the
screen's height):

- The menu is a rectangle 1020 x 680 centred on the screen; behind it a black picture
  (`interface\shared\background\solid_black.dds`) 55 left of it and 115 above, 1134 x
  910, at brightness 128 and the shared background opacity.
- The headline: a short side fading down at the left, a line 50 long, the title in font 6
  at x 70, 13 above the line ("Repair Services", the string `sRepairServicesTitle`), the
  line again from 20 after the title to a second short side; at the right, two labels
  in font 2 right-aligned 20 down: the vendor's skill, ending 15 before the second side,
  and the player's caps ending at 1005. The second side stands 35 left of the caps
  label (but not left of 860), a third side at 1020.
- The list: 15 left of the menu and 60 down, 480 x 580, the shared list box. A row (the
  template) is 32 in and 440 wide: the name in font 2 at 40, 20 down, wrapped at 260; a
  bar 60 x 16 at 370, 20 down, its back at alpha 64, its fill as wide as the condition
  (set in code); a filled square 25 x 25 at (15, 17.5) for an equipped item. The rows
  click silently (`&nosound;`); their alpha is set in code, "true if we have enough
  money and vendor skill to repair this item". With no rows, the text `sNoItemsToRepair`.
- The item's picture: 400 x 400 at (500, 0).
- The figures from (485, 380): the condition (`sInventoryCondition`) as a card 263 x 60
  with a meter 150 x 20 at 85; beside it at 273 the item's figure (set in code); an
  arrow `interface\shared\arrow\repair_arrow_down.dds` 80 x 80 at (223, 50); the gains in
  font 4 centred at 120 and 400, 90 down; the condition and figure after the repair at
  140 down; the cost in font 6 at 230 down. An error text in font 2 centred at (250,
  160) shows instead of the cards after the repair, "when an item can't be repaired,
  like if the vendor's skill is too low" (set in code).
- The buttons, right-aligned at 1010 from 75 down, 47 apart: Repair (on the Xbox only),
  Repair All (`sRepairAllItems`, PC key A, drawn at alpha 128 unless it can be used) and
  Done (`sDone`, PC key E).
- A bottom bracket at 680, 1020 wide: a line with short sides fading up.

**What the screenshots add** [observed]:

- The words: "Repair Services", "Repair Skill  100", "Caps  18", "Repair All (285)  A)",
  "Done  E)", "CND", "DAM", "+66 %" (the percent sign small), "+3", "Cost: 98 caps".
- The rows the player can have repaired come first and bright (Minigun, Sturdy Caravan
  Shotgun, Weathered 10mm Pistol, Mercenary's Grenade Rifle at 11, 11, ?, 1 caps), the
  others after them, dim at about two thirds (Straight Razor 32, Varmint Rifle 98, 9mm
  Pistol 130 caps with 18 caps carried; Broad Machete and Binoculars at full condition,
  "Cost: 0 caps"). Within each part the order is not by name or by cost; we sort by name
  [design].
- The Varmint Rifle is marked equipped (the filled square).
- "Repair All (285)" is dim with 18 caps; its figure is the sum of the rows' costs.
- The cost line is dim when the player cannot pay it.
- At vendor skill 100 every item's condition after the repair is full.
- The item's figure is DAM for a weapon (Varmint Rifle 8 → 11, 9mm Pistol 7 → 9).

## 4. The vendor's rules

- **What is listed**: the player's weapons and armour worn below full condition, one row
  per group of units at one condition ("Name (count)" for more than one, each repaired
  one at a time) [design for the grouping]. An item at full condition is not listed
  [owner: "an item at 100% condition can't be repaired, so it doesn't appear in the
  list"; the original lists it, dim, at "Cost: 0 caps", §3]. Armour needs a DT of at
  least 1 [documented: GECK wiki Armor, "In Fallout: New Vegas, the item must have at
  least 1 DT to be repaired by a vendor"]. Items without a health (DATA health 0) are
  not listed [design]; neither is non-playable armour (the Pip-Boy) [design].
- **How far**: up to the vendor's Repair skill as a percentage of full condition
  [hypothesis: the in-game help says vendors "require no special gear to perform the
  repair, they are limited by their own skill. Extremely talented repair vendors may be
  able to restore a weapon or piece of apparel to almost perfect condition" (MESG
  `HelpRepair`); at skill 100 the screenshots restore everything to full]. An item at or
  above that limit is beyond the vendor: its row is dim and the error text reads "The
  vendor's skill is too low to repair this." [design: the words; the original's are set
  in its code]. FalloutNV.esm has `fRepairSkillMax` 10.0 [observed]; its role is not
  documented and it is not read.
- **The cost**: the item's value × `fItemRepairCostMult` × the condition gained (a
  fraction), rounded to whole caps, halves up, at least 1 [hypothesis, fitted to the
  screenshots: Varmint Rifle (value 75) from 34 % costs 98 (75 × 2 × 0.655 = 98.3); 9mm
  Pistol (100) from 35 % 130; Straight Razor (35) from 55 % 32 (31.5); Minigun (5500)
  just short of full 11; `fItemRepairCostMult` is 2.0 in FalloutNV.esm, observed]. The
  value is the record's (WEAP/ARMO DATA), not the worn item's lower one; the player's
  Barter skill plays no part [hypothesis: the same fit].
- **Paying**: the caps leave the player and go to the vendor; when the vendor is the
  player nothing changes hands [documented: §2]. A repair the player cannot pay is
  refused.
- **Repair All**: every row the player can have repaired, each unit, for the sum of their
  costs; refused as a whole when the player cannot pay the total [hypothesis].
- **The figure after the repair**: our own damage and armour rules at the new condition
  (combat-rules.md §17: damage from 100 % at full to 50 % at zero, DT the same), rounded
  [design: the original's numbers (8 → 11 for the Varmint Rifle) do not follow our rule].
- **The condition shown**: whole percent, rounded [observed: "+66 %" from 34 %].
- **Sounds**: opening `UIMenuMode`, the pointer or keys on a row `UIMenuFocus` (the list
  box's), a repair `UIRepairWeapon` [observed: the SOUN exists in FalloutNV.esm;
  hypothesis: that it is the repair's sound, armour too], a refusal and Done
  `UIMenuCancel` [design].
- **Keys**: a click on a row or Enter repairs it, A Repair All, E (and Esc, Tab) Done; up
  and down or the wheel move the choice [documented: the file's `_PCButton_A`,
  `_PCButton_E`; design: Enter, the others].

## 5. Repair from the inventory (not built yet)

- The in-game help: "In your Pip-Boy, the Repair menu shows any equipment you currently
  have that can be used to repair the selected piece of equipment. Highlight the piece of
  equipment you would like to sacrifice to make the repair. The efficiency of your
  repair is affected by your character's Repair skill." "Whatever item you choose to
  repair with will be scrapped completely." [documented: FalloutNV.esm MESG `HelpRepair`,
  `HelpPipBoyItems`]
- The formula [documented: GECK wiki "Item Repair Formula"]: with TIC the target's and SIC
  the source's condition on 0 to 10, the gain in percent is `(max(TIC, SIC) + fRepairMin
  + (fRepairMax − fRepairMin) × Repair / 100 + min(TIC, SIC) × fRepairScavengeMult − TIC)
  × 10`, the result capped at 100 % with a minimum of 5 %, "in Fallout: New Vegas, the
  final condition is never rounded up". The settings' defaults: `fRepairMax` 2.0,
  `fRepairMin` 0.5, `fRepairScavengeMult` 0.05 [documented]; FalloutNV.esm sets none of
  them [observed].
- Which items: those on the target's repair list (WEAP/ARMO `REPL`, a form list), and
  items of the same kind always [documented: GECK wiki Armor, "Repair Item List: Form list
  of items that can be used to repair this object. Can be NONE, objects can always be
  repaired by items of the same type"].
- The ITEMS page's hint reads "Repair R)" or, above 75 % for a weapon and 50 % for
  apparel, "Maintain R)" [observed: the owner's screenshots; documented: the help text
  "At 75% Condition or higher, a weapon does maximum damage and only needs to be
  maintained", "Apparel above 50% CND provides maximum protection and only needs to be
  maintained"].
