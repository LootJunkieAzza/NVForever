# Script function parameters audit (2026-10-18)

The owner saw "Pip-Boy 3000 removed" at the start of a new game. The opening's script calls
`player.RemoveItem` on the Pip-Boy 3000 (00015038) and the Pip-Boy glove (00025B83) with the
third parameter, HideMessageFlag, set to 1 [observed: FalloutNV.esm scripts, `fnvtool
script decompile`]. The flag is part of the base game [documented: GECK wiki RemoveItem],
but `RemoveItem` never read it. When it was written the engine showed no "added/removed"
lines at all, so the flag had nothing to hide; the lines came later and nobody went back to
the flag. The owner asked for every parameter a function takes to be honoured, and for an
audit of what else is ignored.

## 1. Method

`tools/script_param_audit.py` compares, for every `Kind::Impl` function in
`crates/fnv-game/src/functions/mod.rs`, the parameters the catalogue
(`schemas/script/functions.json`, from the GECK wiki) lists with the highest argument index
its implementation reads (`a.form(i)`, `a.num_or(i, …)`, …). Given a `fnvtool script
decompile` output, it also counts the shipped calls that pass more arguments than are read.
Every candidate was then read by hand: functions that read their arguments through a helper
(`quest(g, a, 0)`, `controls(a, …)`, `rep_axis`, `magic_item`, `request`, the `flag!` macro,
MoveTo's offset loop) are false hits and are left out below.

What it does not catch: a parameter that is read but then not acted on, and the stubs
(`Kind::Stub`/`NoOp`, which ignore every parameter by design; docs/design/script-stubs.md).
Call counts are over FalloutNV.esm only, not the add-ons.

## 2. Fixed here

| Function | Parameter | Shipped calls passing it | Now |
|---|---|---|---|
| AddItem | SilentlyAdd | 82 | non-zero: no "added" line |
| RemoveItem | HideMessageFlag | 17 (the Pip-Boy in the opening) | non-zero: no "removed" line |
| AddItemHealthPercent | MessageHiddenFlag | 31 | as AddItem, also on the leveled-list path |
| RemoveAllItems | hideMessages | 5 | no lines for the moved/destroyed items |
| RemoveAllTypedItems | suppressMessages | — (parsed before, not acted on) | no lines |
| EquipItem | HideEquipMessage | 39 | hides the "removed" line of a consumed aid item [hypothesis] |
| ShowMessage | var1…var9 | 49 | `%.0f`, `%g`, `%e`, width, `+ - 0 space`, `%%` filled from the variables; too few variables: the message does not show [documented: GECK wiki ShowMessage] |
| ShowMessage | Duration | — | does nothing in the original [documented: same page] |

The hide flag is carried on the trace events (`ItemAdded`/`ItemRemoved` `silent`, not
serialised, so traces and goldens are unchanged) and `engine/nvf-play/src/script_events.rs`
skips silent ones.

## 3. Still ignored

| Function | Parameter(s) | Shipped calls passing them | What it needs |
|---|---|---|---|
| Enable / Disable | FadeIn / FadeOut | 276 / 268 | an engine fade of the reference's look (Enable fades by default, Disable does not [documented: catalogue]) |
| EquipItem / UnequipItem | NoUnequipFlag / NoEquipFlag | 39 / 21 | the player's inventory refusing to unequip/equip the item |
| SetPlayerTagSkill | iSkillIndex | 23 | tag skills kept as four slots instead of a set |
| Say | ForceSubtitles, Actor, is2D, isVoice | 18 | subtitles shown regardless of the setting, a stand-in speaker, 2D sound |
| PlaceAtMe | Distance, Direction | 8 | the spawn offset |
| SexChange | resetAppearance | 6 | |
| PlaceLeveledActorAtMe | levelMod, encounterZoneID | 5 | leveled actor levels |
| RemoveAllItems | retainOwnershipFlag | 5 | moved items keep their owner either way now |
| SetTagSkills | addedPoints, showInitialTaggedSkills | 2 | the tag menu's point count |
| ShowLoveTesterMenuParams | iPoints | 1 | |
| SetGlobalTimeMultiplier | keepSoundUnchanged | 1 | |
| StartConversation | allowMovement | 1 | |
| AddItemToLeveledList | fHealthPercent100 | 0 | |
| EquipItem / UnequipItem | HideEquipMessage | (see above) | nothing to hide: the engine shows no equip lines |
