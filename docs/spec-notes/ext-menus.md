# Menu getters: GetMenuTargetRef and GetSelectedItemRef (#983)

JIP's two functions that tell a script what an open menu is about and what it has
highlighted. Code: `crates/fnv-game/src/nvse_jip3.rs` (the functions),
`crates/fnv-game/src/game.rs` (`MenuPick`, `MenuPicks`, `Game::set_menu_picks`),
`engine/nvf-play/src/menu.rs` (`publish_menu_picks`, what the engine reports).

## 1. The pages

- **GetMenuTargetRef** `menuCode`: "the reference from which the specified menu is
  derived (if currently open)", by menu: 1008 the container or actor whose inventory is
  open, 1009 the actor spoken to, 1014 the door or container picked, 1016 the source
  item stack, 1023 the map marker under the mouse, 1027 and 1084 the perk under the
  mouse, 1035 the item repaired, 1053 the actor bartered with, 1055 and 1057 the
  terminal, 1056 the VATS target, 1058 the repair vendor, 1061 the weapon modded, 1075
  the companion, 1077 the crafting actor, 1083 the Caravan opponent [documented:
  GetMenuTargetRef]. 1016, 1035 and 1061 are inventory references.
- **GetSelectedItemRef**: "the form currently highlighted in the active menu": an
  inventory reference in the inventory (1002), container (1008), Pip-Boy repair (1035),
  barter (1053), vendor repair (1058) and item mod (1061) menus; a base form on the DATA
  page (1023: a quest, note or challenge) and the STATS page (1003: a perk)
  [documented: GetSelectedItemRef].

## 2. What NVForever does

- The engine reports, each frame, each item menu's target and highlighted row
  (`Game::set_menu_picks`): the container menu (target the container, the row of the
  focused column), the inventory (no target, the player's row), barter (the merchant),
  the Pip-Boy repair list (the item repaired; the part highlighted, or the item for the
  first row) and the mod menu (the weapon; the mod highlighted, or the weapon). An item
  is given to scripts as an inventory reference of what holds it: the player, the
  container, or in barter the merchant's container (its stock, as VendorContainerChet
  holds Chet's goods; the target is still the merchant) [documented: §1].
- An inventory reference names the item's base form and its holder only, so two of
  one weapon at different condition, or the weapon being modded and another of it, are
  one reference to scripts [hypothesis: the original's reference is to the one stack;
  NVForever's inventory references have no stack of their own yet].
- A pick counts only while its menu is open (`Game::menu_open`); GetSelectedItemRef
  reads the menu on top (`Game::top_menu`: the menu given, else the one under it, else
  the conversation).
- GetMenuTargetRef 1009 is the conversation's speaker, as before.
- Not built [hypothesis: nothing keeps them yet]: the DATA and STATS pages' base forms,
  1014, 1016, 1023, 1027/1084, 1055-1058, 1075, 1077, 1083 and the VATS target; they read
  0.

## 3. Checks

- `cargo test --release -p fnv-game --test synthetic menu_getters` (closed menus read 0;
  target and inventory reference while open; the menu on top answers).
