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

## 3. Done in the engine follow-up (branch claude/script-params-engine)

The rows that needed engine behaviour. Calls are FalloutNV.esm's unless an add-on is named.

| Function | Parameter | Shipped calls passing it | Now |
|---|---|---|---|
| Enable / Disable | FadeIn / FadeOut | 276 / 268 | Enable fades in unless its flag is 0, Disable fades out only with 1 [documented: GECK wiki Enable, Disable]; enable children fade in with their parent unless their XESP says Pop In, and pop out [documented: same pages]. `nvf_play::ref_fade`: linear over 1 s of game time [hypothesis], drawn as SetActorAlpha draws (nvforever `effect_shaders.rs`); the simulation changes at once. The `fade` flag rides on the trace events, not serialised |
| EquipItem / UnequipItem | NoUnequipFlag / NoEquipFlag | 39 / 21 | `fnv_sim::EquipLocks` on the actor, saved: the player's inventory menu cannot take off an item kept on, nor put on one kept off; an item kept on blocks its slot for every equip, EquipItem's too [documented: GECK wiki EquipItem]. EquipItem lifts NoEquip, UnequipItem lifts NoUnequip [documented] |
| SetPlayerTagSkill | iSkillIndex | 23 (Doc Mitchell's test: slots 0, 1, 2) | four slots (`PlayerState::tag_slots`): a skill put in a slot replaces the one there; above 3 does nothing [documented: GECK wiki SetPlayerTagSkill]; the tag menu's Done clears the slots |
| SetTagSkills | addedPoints, showInitialTaggedSkills | 2 | the total to tag; with 0 the old tags are not on the sheet, so the picks replace them [documented: GECK wiki SetTagSkills] |
| ShowLoveTesterMenuParams | iPoints | 1 (40) | the attributes add up to it, as SetSPECIALPoints [documented: GECK wiki ShowLoveTesterMenuParams] |
| Say / SayTo | ForceSubtitles / forceSubtitleFlag | 18 | the subtitle shows even with the new setting "Subtitles of lines said in the world" off (`script_hud::SubtitleSettings`, on by default) [documented flag; hypothesis: the flag is about that setting] |
| Say | is2D | 0 here; 103 in OldWorldBlues.esm | the line is said flat and heard (and subtitled) wherever the player is [hypothesis from the name: the stealth suit's voice, a creature reference that says its lines with is2D 1] |
| SexChange | resetAppearance | 6 (Fallout 3's CG00 scripts, not run in New Vegas) | the player's look becomes the race's first starting face for the new sex, from the creator's data (`nvf_play::creator::reset_look`) [documented: GECK wiki SexChange; hypothesis: "default preset" = the first]; without it a chosen look takes the new sex and keeps its face [hypothesis] |
| RemoveAllItems | retainOwnershipFlag | 5 | 0 (the default) clears the moved items' owner [documented: GECK wiki RemoveAllItems] (`World::transfer_all_owned`) |
| SetGlobalTimeMultiplier | keepSoundUnchanged | 1 (passes 1) | 0 (the default) moves sound with the clock, as the engine's own slow motion does, faster too up to 2× [documented flag; hypothesis: the speed] (`audio_out::clock_sound`) |
| StartConversation | allowMovement | 1 (passes 0) | 1 lets the two actors walk on while they talk (`Game::held_by_conversation`) [documented: GECK wiki StartConversation]; left out they stand, as before [hypothesis] |
| AddItemToLeveledList | fHealthPercent100 | 0 here; 11 in HonestHearts.esm (1), 9 in OldWorldBlues / LonesomeRoad (100) | the weapons and armour the entry gives come at that percentage (`LeveledEntry::condition`) [documented: 0–100 in New Vegas]. Honest Hearts adds its vendor lists at 1, so those weapons come at 1 %: owner to check in game (docs/owner-review.md) |

## 4. Still not read, and why

| Function | Parameter(s) | Shipped calls passing them | Why |
|---|---|---|---|
| PlaceAtMe | Distance, Direction | 8 | they have no effect in the original: "the object always lands in a safe spot near the caller" [documented: GECK wiki PlaceAtMe, Bugs] |
| PlaceLeveledActorAtMe | levelMod, encounterZoneID | 0 | leveled actor lists are read at one fixed level for every placement (`fnv_game::templates::LEVELED_ACTOR_LEVEL`, docs/spec-notes/actors.md §2.1); there is no actor level for the difficulty or the zone to change until actor levelling exists |
| Say | Actor | 0 here; 103 in OldWorldBlues.esm | the wiki gives only the name [documented: GECK wiki Say]; every shipped call names the speaker's own base (NVDLC03StealthSuitVoiceCr), so what another actor would change is not seen |
| Say | isVoice | 0 | name only, and passed by no shipped script |
| EquipItem / UnequipItem | HideEquipMessage | 39 / 21 | nothing to hide: the engine shows no equip lines (EquipItem's hides a consumed aid item's line, §2) |

`python -I tools/script_param_audit.py . <decompile.json>` on 2026-10-18 lists, of these
functions, only the §4 rows (and the false hits of §1).
