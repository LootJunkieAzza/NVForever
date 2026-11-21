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

[hypothesis] NVForever accepts integral levels 1 through 65535 and ignores
invalid forms, the wrong actor family, missing arguments, nonfinite numbers
and levels outside that range. A capture must establish the original's
invalid-input behavior and duplicate-entry ordering. Appending preserves
the order of all existing entries and permits duplicate additions.

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
