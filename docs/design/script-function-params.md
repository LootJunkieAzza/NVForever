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

## 3. Honoured on 2026-10-18 (branch claude/script-params-honoured)

The list §3 had after the first round, each now read and acted on. Sources: the GECK wiki
export (`C:\fnv-private\geckwiki`, owner's) [documented] unless tagged otherwise; the
shipped calls are counted over FalloutNV.esm with `tools/script_param_audit.py`, the
add-ons' by hand.

| Function | Parameter(s) | Shipped calls | Now |
|---|---|---|---|
| Enable / Disable | FadeIn (default 1) / FadeOut (default 0) | 276 / 268 | the reference fades in or out over 1 s instead of popping (`nvf_play::fade`); enable children fade in unless their XESP "Pop In" flag is set, and pop out [documented: Enable, Disable notes]. GetDisabled read **0** the game loop after `Disable 1` in the original [observed: capture `enable_fade_timing`, 2026-10-18], while NVForever reads 1 at once: whether it stays 0 through the fade (and how long the fade is) is not settled, since the job's screenshots all failed (BitBlt); follow-up job `disable_fade_getdisabled` reads GetDisabled every loop after `Disable 1` and a plain `Disable`. Length and shape [hypothesis] |
| EquipItem / UnequipItem | NoUnequipFlag / NoEquipFlag | 39 / 21 | `fnv_sim::world::EquipLock` on the actor, kept in snapshots: a locked-on item cannot be unequipped (inventory menu: "… can't be unequipped") and nothing else takes its slot; a locked-off one cannot be equipped; the other function takes the lock away |
| SetPlayerTagSkill | iSkillIndex | 23 | four slots (`PlayerState::tag_slots`): the slot's old skill is untagged unless another slot holds it; an index above 3 does nothing |
| Say / SayTo | ForceSubtitles / forceSubtitleFlag | 18 Say (all pass 1, nothing after it); 131 SayTo here, 76 in the add-ons | the subtitle shows even out of hearing (NVForever has no subtitle setting to override) [hypothesis: the flag's meaning] |
| Say | is2D | 0 here; 103 in Old World Blues | heard flat and whole wherever the player is, subtitle included [hypothesis] |
| Say | Actor, isVoice | Actor: 103 in Old World Blues, naming the speaker's own base; isVoice: 0 | **not read**: the wiki gives only the names; the shipped Actor calls name the speaker itself, so what another actor would change is not seen |
| PlaceAtMe | Distance, Direction | 8 | nothing to do: "The Distance and Direction parameters do not work. No matter what is specified for them, the created reference will be placed in safe location nearby" [documented: PlaceAtMe notes] |
| SexChange | resetAppearance | 6 (all pass 1) | **left to the actor-changes workstream** (SexChange and AgeRace for NPCs). Its note: the wiki says that with 1, on a change of sex, the player's look becomes "the default preset for their race+gender" |
| PlaceLeveledActorAtMe | levelMod, encounterZoneID | 0 (the 5 calls pass the actor only) | nothing to do for the shipped scripts: levelMod defaults to 4, "None" [documented]. A level modifier would need leveled actors resolved at the player's level, which they are not (docs/spec-notes/actors.md §2.1) |
| RemoveAllItems | retainOwnershipFlag | 5 | moved items lose their owner unless it is non-zero (`World::transfer_all_owned`) |
| SetTagSkills | addedPoints, showInitialTaggedSkills | 2 (`3`, `3 1`) | the menu's count is addedPoints in all, those tagged before counting; with 0 the tags before are not shown and may be chosen over |
| ShowLoveTesterMenuParams | iPoints | 1 (40) | the seven attributes add up to iPoints, as SetSPECIALPoints |
| SetGlobalTimeMultiplier | keepSoundUnchanged | 1 (`0.5 1`) | without it the sounds the engine's slow motion slows play at the multiplier too (`audio_out::SoundSpeed`) [hypothesis: which sounds, and the factor itself] |
| StartConversation | allowMovement | 1 (`0`) | between actors, 1 lets both go on with their packages while they talk (`Game::conversation_moves`); 0, or left out, keeps them standing as before [hypothesis for the default] |
| AddItemToLeveledList | fHealthPercent100 | 0 here; 11 in Honest Hearts pass 1; 5 in Old World Blues and 4 in Lonesome Road pass 100 | the items the entry gives have that condition, 0–100 % (`LeveledEntry::condition`); Honest Hearts' 1 gives items at 1 % [observed: capture `leveled_list_health`: an entry added at 50 gave Vance's 9mm SMG at 375 of 750 health, at 1 at 7.5] |
| EquipItem / UnequipItem | HideEquipMessage | (see above) | nothing to hide: the engine shows no equip lines |

Another session's unpushed branch `claude/script-params-engine` (2026-10-18) covered the
same rows; from it come the add-ons' call counts above, SayTo's subtitle flag, and keeping
a reference that scripts draw (`nvf_play::script_draw`) until its fade-out ends.

## 4. Still ignored

Rerun of `tools/script_param_audit.py` after this branch, candidates read by hand as in §1:
Say's Actor and isVoice, and SexChange's resetAppearance (left to the actor-changes
workstream). The other rows it prints read their parameters through
a helper (false hits, §1); Look's rotateBody is read as its second argument (the
catalogue lists three parameters: a false hit too).
