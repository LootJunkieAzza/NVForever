# Crafting: recipes and the recipe menu

The workbench, the campfire and the reloading bench open one menu, the recipe menu
(playtest list 5 item 72). Code: `crates/fnv-game/src/crafting.rs` (records, the check,
`ShowRecipeMenu`), `engine/nvf-play/src/crafting.rs` (the menu).

## 1. The records

**RCPE, a recipe** [documented: GECK wiki "Recipe Entry" (owner's export); xEdit
`wbDefinitionsFNV` RCPE for the layout]:

| Subrecord | Meaning |
|---|---|
| `EDID` | Editor ID |
| `FULL` | The name shown in the menu |
| `CTDA` (any number) | Visibility conditions: a recipe whose conditions fail is not listed |
| `DATA`, 16 bytes | Skill (an actor value code, i32, −1 for none), level (u32), category (FormID), subcategory (FormID) |
| `RCIL` + `RCQY` | An ingredient (FormID) and its quantity (u32) |
| `RCOD` + `RCQY` | An output (FormID) and its quantity (u32) |

[observed: FalloutNV.esm has 105 RCPE; every DATA is 16 bytes; 263 RCIL and 144 RCOD,
and 407 RCQY, one after each. With every add-on loaded there are 291 recipes.]

**RCCT, a recipe category** [documented: GECK wiki "Recipe Entry"; xEdit RCCT]: `EDID`,
`FULL`, `DATA` (one byte of flags, bit 0 set for a subcategory). FalloutNV.esm has 10
[observed]: the categories Workbench, Campfire and Reloading Bench, and the subcategories
Aid, Ammo, Breakdown, Chems, Food, Misc and Weapons.

The wiki says that the skill NONE means the recipe can be crafted at any skill and the
menu shows no skill requirement. It also says a recipe "will only become usable once the
player has all the listed ingredients" [documented].

## 2. Opening the menu

`station.ShowRecipeMenu category` (alias `Craft`) takes a category or a subcategory
[documented: GECK wiki ShowRecipeMenu, whose example is `EDE1REF.Craft AmmoSubRecipes`].
The menu lists the recipes whose category or subcategory matches. The scripts of the
benches' activators call it. The recipe menu is MenuMode 1077 [documented: GECK wiki
GetMenuTargetRef, "1077 The actor whose skills are used for crafting"]. It is an engine
menu: scripts see it as open until the player leaves it.

## 3. The menu

[observed: the owner's screenshots in C:\fnv-private\Crafting; documented: the layout in
`menus\recipe_menu.xml`, see the module notes]

- The list: the recipes sorted by name. The label is the name, with the output count in
  brackets after it when more than one is made ("Antivenom (2)"). NVForever's campfire
  list starts "Antivenom (2), Atomic Cocktail (4), Battle Brew, Bighorner Steak, Black
  Blood Sausage, Black Coffee", the same as the original's [observed, smoke `crafting`].
- The arrows beside "RECIPES" filter the list by subcategory [hypothesis].
- For the chosen recipe, at the right: "Made at ..." with its category's name; "Skill
  Requirement" as "<skill> (<player's value>/<level>)"; "Ingredients" as
  "<name> (<have>/<need>)".
- Under the list: the first output's WG and VAL.
- Accept A) and Exit X).
- Making a recipe once removes its ingredients from the player and adds its outputs.
  Each output gets an "added" line in the corner.

Not built:
- the output's icon and its effects line;
- making a recipe several times at once;
- the "Made at" category for a recipe whose category differs from the bench's;
- crafting experience (whether the original gives any is not known).
