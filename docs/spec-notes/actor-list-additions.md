# Persistent actor-list additions

[documented] `AddNPCToLeveledList list actor level` accepts a leveled
character base and an NPC or another leveled character base.
`AddCreatureToLeveledList list actor level` accepts the corresponding
creature forms. Both changes survive saving and loading. Sources:
[NPC](https://geckwiki.com/index.php?title=AddNPCToLeveledList),
[creature](https://geckwiki.com/index.php?title=AddCreatureToLeveledList).

[documented] The added entry becomes eligible at its supplied list level;
actor-list counts are one. Existing list selection rules still apply.
Source: [LeveledCreature](https://geckwiki.com/index.php?title=LeveledCreature).

[observed: Fallout: New Vegas 1.4.0.525 + xNVSE 6.4.9, 2026-11-24, `GSDocMitchellHouse`
(`0x00103DF9`), 10 official base masters, isolated clean-room staging copy (read-only
`PrintToConsole` probe; zero writes to game install or Saves),
`w4d_41_vanilla_player_and_lists` (`tools/capture/nvse/catalogs/wave4d_extenders.json`, PR #3803 / #3802):
`PROBE|w4d_41_vanilla_player_and_lists|xp(0->25)|modAttr(5->7)|lvli(n0=4,l0=5:0,l1=6:1,l65k=7:-1,l70k=8:4464)|pkg(doc=14,pl=0)|rmMerch=ok`;
zero DLL, PDB, or third-party source consulted] For leveled item lists
(`AddItemToLeveledList`), the original game accepts level `0` and levels above `65535` (such
as `70000`), growing `GetNumLevItems` on `0x00000AA1` from `4` to `8`, and `GetNthLevItemLevel`
readback confirms level `0` is stored verbatim as `0`, level `1` as `1`, level `65535` as
`0xFFFF` (read back as `-1`), and level `70000` wraps modulo 65536 (`u16`) to `4464`.
NVForever's `AddItemToLeveledList` converts finite levels via `(level as i64) as u16`;
non-finite inputs map to `0` [design], and `count` below `1` remains clamped to `1`
[hypothesis].

For actor lists (`AddNPCToLeveledList` / `AddCreatureToLeveledList`), finite level clamping
(`0 -> 1`) and 16-bit wrapping (`65535`/`-1 -> 65535`, `70000 -> 4464`) are now [observed]
below (`### Level clamping and 16-bit wrapping (w6d7_35)`); rejecting nonfinite level numbers
remains a [design] guard. [hypothesis] Invalid forms, the wrong actor family, missing arguments,
and duplicate-entry ordering remain unprobed on `LVLN`/`LVLC`; NVForever ignores invalid forms,
wrong-family actors, and missing arguments, while appending preserves the order of all existing
entries and permits duplicate additions.

[hypothesis] Existing generated inventories keep their contents; subsequent
actor template selection and newly generated inventories see additions.
No original-game or installed-mod result is claimed here.

[hypothesis] Source calls are supported by their documented names only.
Native opcode/argument admission remains open for a GECK compilation probe.
No numeric encoding is inferred from registration or documentation identity.

[hypothesis] Persistent additions belong to saved base-form changes; session
list edits remain session edits. Runtime readers combine them with accepted
list selection, encounter levels and reference seeds. The renderer must read
the same effective lists as gameplay, including after loading a save.

[design] An edited actor list changes resurrection inventory regeneration only
when the list is reachable through that actor's Inventory template chain. An
ordinary actor with its own inventory retains carried extras after an unrelated
NPC-list or creature-list edit, as it does without edits. Existing static-list
regeneration rules remain in force. Synthetic controls cover that preservation
and relevant edited NPC/creature lists selecting their new inventories.
[hypothesis] Whether the original game's resurrection follows the same rule
after a list edit has not been observed; this extends NVForever's existing
regeneration rule.

[design] When resurrection selects another inventory source, created references
update their saved source and placed references update their reconstructed source.
ResetInventory must read that same selection immediately and after saving/loading;
clearing a created source returns it to its own base. This changes the selected
source, not the inventory-preservation rule above. Synthetic controls cover both
reference lifetimes and clearing an override. [hypothesis] The original game's
source after a reroll and a save/load has not been observed.

[design] Restoring a game reconstructs effective source metadata for every placed
actor after saved list additions, actor rolls and player level are restored, even
when the actor already has generated contents. This metadata refresh leaves those
contents intact. A later ResetInventory follows the reconstructed source. Created
actors retain their saved source until an actual reroll replaces it.
[hypothesis] The original game's source after restoring a save with additions
has not been observed.

[hypothesis] Both functions return 0, whether or not an entry was added; no
cited source or capture establishes their return value.

[design] Engine choices, not claims about the game: additions are stored per
list as (level, form) pairs in the saved form changes, a level held as 16 bits;
the Inventory-chain walk used by resurrection stops after 32 steps and at a
repeated form; the renderer's actor-list copy is updated only for lists whose
effective definition changed, including removal after a load.

### Level clamping and 16-bit wrapping (`w6d7_35`)
- **Level edge values accepted** [observed 2026-11-25, Fallout: New Vegas 1.4.0.525 (`040020D0`) + `xNVSE 6.4.9` + `JIP LN NVSE 57.30 (5730)`, 10 official base masters, `GSDocMitchellHouse` (`0x00103DF9`), isolated clean-room staging, committed probe `w6d7_35_add_npc_and_creature_to_leveled_list_edges` in `tools/capture/nvse/catalogs/wave6d7_actor_leveled_list_levels.json`]: both `AddNPCToLeveledList` and `AddCreatureToLeveledList` accept `0` (stored as `1`), `65535` (`0xFFFF`, read as `-1` by `GetNthLevItemLevel`), `70000` (wrapped mod `65536` to `4464`), and `-1` (`0xFFFF`, `-1`) (`PROBE|w6d7_35_add_npc_and_creature_to_leveled_list_edges|lvln(n=3->4/5/6/7,lv=1,4464,-1,-1)|lvlc(n=1->2/3/4,lv=1,4464,-1)`).
