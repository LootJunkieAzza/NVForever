# Script function audit, second round (2026-10-19)

The owner found RemoveItem's hide-message flag ignored (fixed 2026-10-18,
docs/design/script-function-params.md) and asked for every script function to be checked
for gaps against the base game. The first round counted parameters: it compared the highest
argument index each implementation reads with the catalogue's parameter count. That misses
the RemoveItem kind of gap once the parameter *is* read: a value read but not acted on, a
default that differs from the wiki, a documented behaviour skipped, a return value that
differs, or a value put on an event or request that nothing in the engine uses.

## 1. Method

- `tools/script_param_audit.py` rerun over the decompiled scripts of all ten masters
  (14,907 units). It now takes a function's `&Args` entry point over a same-named
  `Game` method and reads closures over the arguments (StartCombat and PlayBink were
  false hits). No parameter is newly unread; the rows it prints are the helper false hits
  of script-function-params.md §1 and the two §4 rows.
- `fnvtool script stubs` over the ten masters: 16 stubs (86 calls, 26 condition uses),
  4 no-ops, 1 function used but not registered (GetIsObjectType, 1 condition use).
- By hand, every registered function: the implementation, its catalogue entry
  (`schemas/script/functions.json`) and its GECK wiki page (owner's export,
  `C:\fnv-private\geckwiki`), plus the engine consumer of every event or request field it
  writes. 104 + 100 + 176 + 64 registrations, in four groups (actors/AI/combat/magic;
  items/objects/references/furniture/navigation/camera; misc/player/quests/reputation/
  senses and every `crate::*` entry; console/toggles/UI, stubs and the engine consumers).
  Owner rulings and captures that override the wiki were left alone (ShowCaravanMenu's
  difficulty, IsTalkingActivatorActor, GetDetectionLevel's player bug, Say's Actor and
  isVoice).

About 67 gaps were found: 10 high (shipped scripts show or do the wrong thing), the rest
medium or low. §2 lists what is fixed, §3 what waits for game captures (asked of the
observer session 2026-10-19), §4 what is left and why.

## 2. Fixed (branch claude/script-param-audit)

| Function | Gap | Now | Evidence |
|---|---|---|---|
| RemoveMe | showed "X removed" (88 of 107 shipped calls run in `OnAdd player`, so items that convert on pickup flashed a line) | silent; the copy to a target is silent too [hypothesis] | wiki RemoveMe: "does not display the "xxx has been removed." message" |
| SetObjectiveCompleted, CompleteAllObjectives | "COMPLETED" shown for every objective, hidden ones and those of completed quests included | only for an objective on display of a quest not yet completed (`ObjectiveCompleted.silent`, not serialised) | wiki SetObjectiveCompleted notes |
| Activate (scripted pickup) | two "added" lines: the outcome's and the ItemAdded event's | one, from ItemAdded | code |
| RemoveAllItems | took the player's quest items | kept, moved or destroyed (`World::stays`) | wiki: "If the calling reference is the player, Quest Items" |
| SetDestroyed | destroyed doors and activators still opened and showed a prompt | refused (`Game::activation_refused`), no crosshair prompt; terminals and talking activators count as activators [hypothesis]; NPCs passing an in-place door on their path are not stopped | wiki SetDestroyed |
| DamageObject | script damage set off no stage explosions | the change is queued (`Game::destruction_changes`) for the engine's `queue_destruction`, as a combat hit's | code |
| AddItemHealthPercent | a leveled list kept its own condition | the given health (`World::add_item_at_condition`) | wiki: "overrides the health percentage set by the leveled list" |
| DropMe | kept the item's condition | a fresh item, owner kept (`World::drop_item_fresh`) | wiki DropMe: "does not preserve the health, charges, or script variables" |
| Enable / Disable | on a reference with an enable parent, still faded and redrew it | no effect | wiki Enable and Disable notes |
| Lock | `Lock 0` set level 0 | locks, level unchanged | wiki Lock |
| IsInList | matched the reference itself | base form only | wiki IsInList |
| AddFormToFormList | re-adding did nothing | a script-added form moves to index 0; a record's stays | wiki AddFormToFormList |
| Activate | a disabled actor could be made to activate | does nothing | wiki Disable notes |
| IsOwner | an NPC reference never matched | taken as its base | wiki IsOwner example |
| SetWeaponHealthPerc | no cap | at most 99.83 % | wiki SetWeaponHealthPerc |
| IsWeaponOut | 1 for a weaponless creature in combat | 0 | wiki IsWeaponOut notes |
| SetFactionRank / AddToFaction | rank −2 read back −2 | below −1 reads −1 [hypothesis] | wiki example: −1 removes |
| SetInCharGen | stored, never read | no level-up while set; the earned levels come after | wiki SetInCharGen |
| SetQuestDelay | 0 fell back to the quest's DATA delay | 5 s | wiki: "Setting delay to 0 tells the quest to process at the INI setting default (5 seconds)" |
| EnableFastTravel | a cell change gave waiting back too; `0 0 1` then `0 0 0` overwrote the keep | only travel comes back; a kept ban ignores a later ban without keep (the general rule [hypothesis]) | wiki EnableFastTravel examples |
| ShowQuests | printed "1 running" with none | the running count | code |
| SetGhost | only damage immunity | enemies neither fight nor detect a ghost (held in the Hidden band); shots, melee rays [hypothesis] and blasts pass through ghost NPCs; a ghosted player can be hurt; nobody targets a ghosted player either (the wiki's player exception reads as damage only) | wiki SetGhost notes |
| SetActorFullName | stored, never shown | the crosshair, HUD target, dialogue speaker, subtitles, barter/container titles and caravan opponent take the message's title (`Game::script_name`); kept in saves | wiki SetActorFullName |
| SetUnconscious | on a dead actor only set the flag | brings it back first, no get-up animation | wiki SetUnconscious notes |
| SetCriticalStage | stored only | GooEnd and DisintegrateEnd hide the reference and take its hit volume and ragdoll out (it stays enabled [hypothesis]); GooStart/DisintegrateStart ragdoll effects and static collision of culled non-actors not built | wiki SetCriticalStage |
| GetAngle X on the player | always 0 | the view pitch, −89 looking up, 89 at the feet; things placed at or moved onto the player stay upright [hypothesis] | wiki GetAngle |
| SetSecuritronExpression | lost on load | kept in saves (`GameSnapshot::securitron_faces`, old saves load) | wiki: "persists across saves and game restarts" |
| MoveToMarkerWithFade | a plain MoveTo (273 shipped calls) | fades out, waits `FADE_OUT_HOLD` [hypothesis, capture asked], moves, fades in; the player and disabled references move at once [hypothesis]; pending moves are saved | wiki MoveToMarkerWithFade |

## 3. Waiting for game captures

Asked of the observer session on 2026-10-19 (through "Outstanding items overview"). Each
stays as it is until the answer comes back.

| Function | Question | Severity |
|---|---|---|
| EquipItem / UnequipItem | the wiki says a script's EquipItem does not run OnEquip (nor UnequipItem OnUnequip), ours does: the faction-disguise armours then change factions and show their messages | high |
| AddScriptPackage | the wiki says the package goes at the next evaluation unless Must Complete; ours never drops it (1,192 shipped calls) | high |
| AddNote | the corner line (sComputersAddedNote): text, icon, sound; nothing is shown now | high |
| IncrementScriptedChallenge | the completion reward and message | med |
| CastImmediateOnSelf | effects of duration 0, and effects not ranged Self | med |
| ForceActiveQuest | the compass's quest-target marks (none drawn) | med |
| AddReputation | the "gained" corner line (`ReputationChanged` has no consumer) | med |
| MoveTo on the player | whether the player takes the marker's heading | med |
| AddItem | OnAdd once per item in the count; an NPC re-equipping its best gear | med |
| ShowMessage | two button boxes queued at once (ours loses the first answer) | low |
| ResetQuest, StartCombat (Friend/Ally refusal, search end), DisablePlayerControls (image-space reset), GetSleeping (player), GetSelf (created references) | as in the audit tables | low–med |

## 4. Left, with the reason

- **Low, not done here**: player SetAngle read back into the view, RemoveScriptPackage
  ending a ForceFlee, AddSpell and SetFactionRank on the base of non-unique actors,
  SetItemValue and SetMinimalUse (stored, never read), GetCombatTarget on the player,
  GetLineOfSight from the player to objects, PlayBink `muteAudio 0`, StartConversation's
  walk to the target, PlayMagicShaderVisuals' perspective, ShowMessage's per-FormID text,
  console ToggleDetection/ToggleGodMode (VATS AP)/ToggleActorsAI on one reference,
  ExitGame's credits.
- **Stubs whose reason is stale**: SetRumble (pad rumble exists, `rumble.rs`), SetAllVisible
  (detection built), IsSwimming (player swimming built), PickIdle (the idle tree is walked),
  EnableLoadingMenu (loading screens built); SetVatsTarget and GetForceHitReaction's texts.
  The other stubs' reasons hold (docs/design/script-stubs.md).

The reviewers' full tables, with every function judged clean, were working notes of the
session (not committed); each row above names its evidence.
