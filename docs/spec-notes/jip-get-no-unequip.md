# JIP LN `GetNoUnequip`

## Documented contract

- [documented: GECK wiki, [GetNoUnequip](https://geckwiki.com/index.php?title=GetNoUnequip)]
  `reference.GetNoUnequip` is called on the inventory reference of a currently
  equipped item and returns `1` if the item cannot be unequipped through normal
  means (locked via `EquipItem` `NoUnequipFlag` or `SetNoUnequip 1`), or `0`
  otherwise.
- [observed: `data2` capture, `crates/fnv-game/src/nvse_plugins.rs`]
  An unlocked pistol inventory reference returns `0`.

## NVForever mapping and open questions

- [design] `nvse_plugins` (`GetNoUnequip`) and `nvse_jip11` (`SetNoUnequip`)
  resolve the calling inventory reference via `Game::temp_item(r)` to
  `(actor, item)` and check whether `item` is currently equipped on `actor` via
  `RefState::inventory.is_equipped(item)` directly (matching `GetEquipped`,
  `IsEquipped`, `GetEquippedItemRefForItem`, and `World::unequip_check`, and
  avoiding `World::equipment`'s pre-outfitting `default_choice` preview).
  `GetNoUnequip` returns `1` iff equipped and
  `World::equip_lock(actor, item) == Some(EquipLock::NoUnequip)`, else `0`.
  Non-inventory references, omitted receivers, unequipped items, and
  `EquipLock::NoEquip` items return `0`.
- [observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9 + JIP LN 57.30 with the 10
  official base masters, clean-room script probe jip_38_get_set_no_unequip_invref
  (GetNoUnequip/SetNoUnequip): nR=1,0,0,0; no DLL or source consulted] Calling
  `GetNoUnequip` / `SetNoUnequip` on a `GetInvRefsForItem` inventory reference in the
  same frame returned `0` before and after `SetNoUnequip 1` / `0`.
- [hypothesis] Per-stack/per-instance NoUnequip differentiation when an actor
  carries multiple identical units of the same base form has not been measured
  in an original-game capture (`World::equip_locks` is currently keyed by base
  `FormId`).
