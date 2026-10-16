# Script stubs: the audit of 2026-10-12

The owner's list 2 asked to "go through the placeholder stubs on scripting functions
against everything we have added up till this point and check/add any core engine
functionality missing" (workstream K, docs/handoff/playtest-2026-10-12.md §3). This note
is that audit: every stubbed function, how often the shipped scripts use it, the system it
needs, its status, and what this round did.

## 1. What a stub is

`crates/fnv-game/src/functions/mod.rs` registers every script function by opcode:

- `Kind::Impl`: implemented;
- `Kind::Stub(system)`: recorded in the trace (`stub` events, `Game::stubbed`), no effect,
  returns 0;
- `Kind::NoOp(why)` (new): deliberately nothing to do on this engine; returns 0, not traced;
- not registered: traced once per unit as `unhandled`, returns 0.

`functions::stubs` (new) gives each stub its status: **assigned** to another workstream
(`ASSIGNED`), **implementable** (its system exists; left for later, `IMPLEMENTABLE` says
what is missing), else **needs a system** not built yet (the `Stub` text names it).

| | before | after |
|---|---|---|
| `Kind::Stub` | 123 | 94 (23 assigned, 10 implementable, 61 need a system); 88 (17 assigned) after karma and crime (§3.1); 81 (17 assigned, 1 implementable, 63 need a system) after the second round (§3.2); 80 (16 assigned) after the followers (§3.3); 77 (16 assigned, 1 implementable, 60 need a system) after Autosave, ShowRecipeMenu and SwapTexture (§3.4); 76 (16 assigned, 1 implementable, 59 need a system) after NPC sneaking (§3.5); 75 (15 assigned) after UseWeapon (§3.6) |
| deliberate no-ops | 4 (`misc::zero` / `one`: IsXBox, IsPS3, IsWin32, IsGoreDisabled) | those 4 + 4 `Kind::NoOp` |
| uses in the shipped scripts that reach a stub | 4,185 (calls + condition uses) | 2,207 (1,552 calls, 655 condition uses); 2,075 (1,452 calls, 623 condition uses) after karma and crime; 2,032 (1,418 calls, 614 condition uses) after the second round; 2,006 (1,392 calls, 614 condition uses) after the followers; 1,534 (920 calls, 614 condition uses) after §3.4; 1,531 (917 calls, 614 condition uses) after NPC sneaking (§3.5); 1,518 (904 calls, 614 condition uses) after UseWeapon (§3.6) |

22 stubs were implemented here (§3); 4 became `NoOp` (§5); 3 more were implemented by the
combat workstream while this audit ran (StopCombatAlarmOnActor, GetAttacked, GetFriendHit,
PR #235), merged before this branch's PR.

## 2. How it is counted

`fnvtool script stubs <Data> <the ten masters> --schema schemas/fnv` (fnv.script.stubs/1)
decodes every script unit of the load order (object, quest and effect scripts; quest stage,
dialogue, package and terminal result scripts: 14,907 units over the ten masters) and
counts each stub's calls; condition uses (CTDA) are the catalogue's
(`schemas/script/functions.json`, from xEdit's table and the masters' scan). It lists every
stub and no-op with its status, sorted by use, and the functions used but not registered at
all (§6). The smoke scenario `script-stubs` (tools/smoke/scenarios.toml) checks the counts
of §1.

[observed] The decoded call counts can differ from the catalogue's `script_calls` (e.g.
IsLimbGone: 51 decoded calls against 12 in the catalogue): the catalogue counted compiled
SCPT records, the survey also decodes result scripts.

## 3. Implemented this round

Ordered by use (calls + condition uses over the ten masters). The behaviour notes and their
evidence tags are on each function (`functions/ai_state.rs`, `senses.rs`, `ui.rs`,
`crate::weather`); the engine hooks are the visible part.

| Function | Opcode | Calls | Conditions | Over | Engine hook |
|---|---|---|---|---|---|
| ResetAI | 0x11FA | 711 | 0 | packages (`crate::ai`): the script package dropped, goal and unreachable packages forgotten, a Look let go, packages chosen again | the residents take the new package |
| StopLook / Look | 0x105D / 0x105C | 242 / 159 | 0 | `Game::script_look` | `nvforever` `script_look.rs`: a `LookOverride` over every other look (the player, an actor, or a marker's position); `rotateBody` kept, not acted on |
| IncrementScriptedChallenge | 0x1275 | 134 | 0 | `PlayerState::challenges` progress against the CHAL DATA threshold; a recurring one starts again from 0 | none (no challenge page or message yet) |
| GetCurrentAIProcedure | 0x108F | 32 | 64 | package type, goal met (17 DONE), furniture (8 SLEEP, 21 GET_UP, 48 SITTING), combat (13), search (36), conversation (4) | none |
| ForceActiveQuest | 0x11DD | 82 | 0 | `UiRequests::active_quest` | the Pip-Boy DATA page takes it (`pipboy data: active quest … (ForceActiveQuest)`) |
| ForceTerminalBack | 0x1223 | 80 | 0 | `Terminals::back`, handed on by `terminal::choose` | the terminal goes back one level or closes; counted in the terminal summary |
| IsSneaking | 0x111E | 35 | 33 | `Senses::motion` | `control::tell_scripts` (the player, every frame), `detection.rs` (everyone measured) |
| GetLineOfSight | 0x101B | 49 | 0 | the detection last measured for the pair (`Detection::seen`: line of sight within ±95°) | `detection.rs` (already) |
| IsMoving | 0x1019 | 0 | 49 | `Senses::motion` (moving reads 1, forward) | `detection.rs` |
| GetCurrentAIPackage | 0x106E | 16 | 15 | package type codes; 5 combat, 29 search, 6 conversation | none |
| AddSPECIALPoints | 0x1219 | 21 | 0 | `SpecialSheet::for_menu`: more points, the old values the floor; opens 1074 | the Vigor Tester's menu opens with those rules |
| AlwaysShowActorSubtitles | 0x1277 | 16 | 0 | `Senses::always_subtitles` | `script_hud.rs` shows the subtitle out of hearing too |
| SetWeather / ForceWeather | 0x1104 / 0x112D | 14 / 13 | 0 | `WeatherState::request` (a fade or at once, held or not) | `main.rs` `script_weather` → `SkyState::script_weather` |
| IsPC1stPerson | 0x1188 | 2 | 9 | `Senses::first_person` | `control::tell_scripts` |
| ReleaseWeatherOverride | 0x1158 | 11 | 0 | `WeatherState::release` | the climate chooses again |
| IsRaining | 0x103E | 6 | 3 | the sky's weather is Rainy (WTHR DATA) and fully faded in | `script_weather` tells the game |
| IsRunning | 0x111F | 0 | 4 | `Senses::motion` | `detection.rs` |
| AddTagSkills | 0x121B | 3 | 0 | `TagSheet::for_menu`: the count raised, the old tags locked; opens 1048 | the tag-skill menu ("SKILLS: n/4 Selected") |
| SetSPECIALPoints | 0x1218 | 2 | 0 | `SpecialSheet::for_menu`: the total; opens 1074 | as AddSPECIALPoints |
| GetIsCurrentWeather | 0x1095 | 2 | 0 | the weather shown, or one just asked for | `script_weather` |

In all, 1,807 uses moved from stubs to implementations.

The test aid `NVF_PLAY_TEST_SCRIPT` gained `call:<reference or ->:<function>[:<argument>…]`,
which calls any function by name as a script would; the `script-stubs-play` scenario uses
it for Look, StopLook, SetWeather and ReleaseWeatherOverride.

### 3.1 Implemented by karma and crime (workstream J, claude/karma-crime)

Taken off the assigned list (docs/spec-notes/crime-karma.md §7): SendAssaultAlarm (0x105F,
93 calls; it was assigned to E2, the assault rules are provocation's and the alarm is a
crime), SendTrespassAlarm (0x115C, 6), GetTrespassWarningLevel (0x1090, 29 condition
uses), IsTrespassing (0x1091, 1), GetAlarmed (0x103D, 1 call), GetMajorCrimeCount
(0x1075, 2); and GetIsAlignment (0x11DA, 3 condition uses), no longer unregistered.
GetOwnerLastTarget (0x124E) stays assigned to J: it is the weapon owner's combat target,
not built. Also registered there: GetMinorCrimeCount, GetActorCrimePlayerEnemy,
IsActorAVictim, IsPCAMurderer, IsCellOwner, IsInMyOwnedCell, and RewardKarma with its
corner message.

### 3.2 Second round (claude/script-stubs-2, 2026-10-12)

The implementable stubs of §4 and the unregistered functions of §6. Evidence tags are on
each function; call sites are from `fnvtool script decompile` over the ten masters
[observed].

| Function | Opcode | Calls | Conditions | Over | Engine hook |
|---|---|---|---|---|---|
| SetPackDuration | 0x1122 | 12 | 0 | `RefExtra::package_ends`: the current package ends that many game minutes after the call (counts as done, end script, then the actor chooses again) [hypothesis: from the call; what it ends into] | none (`Game::end_timed_packages` in the periodic choice) |
| IsPlayerMovingIntoNewSpace | 0x1166 | 6 | 8 | `Game::loading` (the travel's loading screen) | `nvf_play::script` sets it already |
| SetGlobalTimeMultiplier | 0x1186 | 7 | 0 | `UiRequests::time_multiplier`; ≤ 0 refused | `nvf_play::script::time_multiplier`: `Time<Virtual>` relative speed; sound speed unchanged |
| Rotate | 0x1004 | 4 | 0 | one call turns the reference for one frame (speed × frame time), the frame's last call counts [hypothesis, from the wiki's note and the GameMode callers] | `moved.rs`, `script_draw.rs` redraw `Game::turned` |
| SetActorsAI | 0x118B | 3 | 0 | `RefExtra::ai_off`: no periodic package choice | `fnv_ai` does not step the actor; `detection.rs` skips it as a detector; animation and collision not stopped |
| ShowAllMapMarkers | 0x11F8 | 2 | 0 | `PlayerState::all_markers` (show, travel kept, skip hidden); `Game::map_marker_shown` | the Pip-Boy world map (FNAM bit 4 "Show All" Hidden) |
| IsTurning | 0x101A | 0 | 1 | `Motion::turning` | `detection.rs`: heading change over 0.5 rad/s (left 1, right 2) [hypothesis: threshold] |
| AddItemToLeveledList | 0x1236 | 20 | 0 | `fnv_sim::World::add_to_leveled_list`: entries added after the authored ones, in snapshots; health not kept | containers and AddItem through `leveled::resolve_with`; actors' drawn equipment (`fnv-actor`) does not see them |
| IsDLCInstalled | 0x126F | 8 | 0 | a load-order plugin whose name matches, spaces, case and extension ignored [hypothesis: "tribal pack" ↔ TribalPack.esm] | none |
| GetXPForNextLevel | 0x1215 | 3 | 0 | the next level's total, (n − 1)((n − 2)/2 · iXPBumpBase + iXPBase) [documented: GECK wiki Experience Settings], less the player's XP [hypothesis: remaining, from the Here and Now perk's stage] | none (levelling up is not built) |

Left, with the reason (§4): PushActorAway (ragdolls are for corpses only; a live
knockdown and getting up is not built, with GetKnockedState), SetForceSneak (since built,
§3.5), AutoDisplayObjectives
(objectives already show as scripts set them; what the switch adds is not documented
beyond the summary), GetIsObjectType (its condition parameter is a form type code, and
the code table is not in the local GECK wiki export; one condition use).

The test aid `call:` also takes `x`/`y`/`z` (an axis) and `"text"` (a string). Smoke:
`script-stubs-2-play` (the time multiplier and ShowAllMapMarkers on the Pip-Boy map).

### 3.3 The followers (H, 2026-10-12)

OpenTeammateContainer (0x11C8, 26 calls) is implemented (docs/design/followers.md §5): the
container menu on the companion's inventory, once the conversation is over. No stub is
assigned to H any more. Wait, StopWaiting and IsWaiting are not registered and used by no
script of the masters [observed]; the companions wait by their `Waiting` variable and Guard
packages.

### 3.4 Saving, crafting and rendering (2026-10-15 and 16)

Three stubs that needed a system are implemented, all without condition uses:
Autosave (0x115E, 74 calls; saving, 2026-10-15), ShowRecipeMenu (0x1250, 15 calls; the
crafting menus, claude/l5-crafting, PR #292) and SwapTexture (0x1269, 383 calls; the
texture swap on a reference's shape, claude/l5-render-fx, §4 notes). With the followers'
OpenTeammateContainer (§3.3, 26 calls, the last stub assigned to H) the audit's counts went
from 81 stubs and 1,418 calls to 77 and 920; the smoke scenario `script-stubs` checks these.

### 3.5 NPC sneaking (claude/l5-npc-ai, 2026-10-16)

SetForceSneak (0x10D3, 3 calls) is implemented: `RefExtra::force_sneak`, read by
`Game::sneaks` together with the current package's Always Sneak; the engine crouches the
actor in its `sneakmt…` locomotion clips, walks it at fMoveSneakMult and measures it as
sneaking (nvforever `sneaking.rs`; owner's playtest list 5 items 36–37). GetForceSneak has
no opcode in the catalogue and stays unregistered.

### 3.6 UseWeapon (claude/use-weapon, 2026-10-16)

UseWeapon (0x119C, 13 calls), assigned to E2 combat AI, is implemented with the Use Weapon
package (docs/design/use-weapon.md): the actor walks to where it is to stand, turns to its
target and fires the number of shots, with the always-hit, no-damage, crouch-to-reload
and hold-fire flags. 76 → 75 stubs (15 assigned, 14 of them to E2), 917 → 904 calls.

## 4. Every remaining stub

Statuses: **needs a system not built yet**; **assigned to workstream** (another agent is
building it now or next: E2 combat AI, H followers, J karma and crime; the aid workstream's
functions, AddSpell, RemoveSpell, CastImmediateOnSelf and Dispel, were implementations
before this audit and are not stubs); **implementable, left for later** (the system
exists; the note says what is missing); **deliberately no-op**.

| Function | Opcode | Calls | Conditions | Status | System / note |
|---|---|---|---|---|---|
| SwapTexture | 0x1269 | 383 | 0 | implemented 2026-10-16 (claude/l5-render-fx) | rendering |
| GetVATSValue | 0x1198 | 0 | 250 | needs a system not built yet | combat (VATS) |
| PlayMagicShaderVisuals | 0x1123 | 205 | 0 | needs a system not built yet | rendering (effect shaders) |
| SetActorAlpha | 0x1129 | 127 | 0 | needs a system not built yet | rendering |
| SetCombatStyle | 0x10B1 | 92 | 0 | assigned to workstream | assigned to workstream E2 combat AI |
| IsActorTalkingThroughActivator | 0x11AE | 0 | 92 | needs a system not built yet | dialogue (talking activators) |
| StopMagicShaderVisuals | 0x1125 | 79 | 0 | needs a system not built yet | rendering (effect shaders) |
| Autosave | 0x115E | 74 | 0 | implemented 2026-10-15 | saving: asks the engine for an autosave (`nvf_play::save`) |
| GetGroupMemberCount | 0x11A0 | 0 | 66 | assigned to workstream | assigned to workstream E2 combat AI |
| IsLimbGone | 0x118E | 51 | 0 | assigned to workstream | assigned to workstream E2 combat AI |
| GetAnimAction | 0x10DB | 44 | 1 | needs a system not built yet | animation |
| IsFacingUp | 0x106A | 0 | 41 | needs a system not built yet | animation (ragdoll) |
| GetKnockedState | 0x106B | 5 | 35 | assigned to workstream | assigned to workstream E2 combat AI |
| SetSecuritronExpression | 0x1248 | 37 | 0 | needs a system not built yet | rendering |
| PushActorAway | 0x118A | 35 | 0 | needs a system not built yet | physics: ragdolls are for corpses only; a live actor knocked down and getting up is not built |
| PurgeCellBuffers | 0x1189 | 32 | 0 | deliberately no-op | memory: the original's cell buffers; cells here load and unload by distance |
| ShowCaravanMenu | 0x125A | 29 | 0 | needs a system not built yet | menus (Caravan) |
| FireWeapon | 0x11E2 | 24 | 0 | assigned to workstream | assigned to workstream E2 combat AI |
| IsLeftUp | 0x111D | 0 | 22 | needs a system not built yet | animation |
| Exists | 0x119F | 0 | 21 | assigned to workstream | assigned to workstream E2 combat AI |
| SetRumble | 0x11FB | 17 | 0 | deliberately no-op | controller rumble: no controller rumble on the PC version |
| GetIsUsedItem | 0x10F6 | 0 | 16 | needs a system not built yet | AI (used items) |
| GetDisposition | 0x104C | 0 | 15 | needs a system not built yet | dialogue (disposition) |
| ShowRepairMenu | 0x117A | 15 | 0 | needs a system not built yet | menus (repair) |
| ShowRecipeMenu | 0x1250 | 15 | 0 | implemented 2026-10-16 (claude/l5-crafting) | menus (crafting) |
| UseWeapon | 0x119C | 13 | 0 | implemented 2026-10-16 (claude/use-weapon) | combat AI: docs/design/use-weapon.md |
| Reset3DState | 0x1146 | 12 | 0 | needs a system not built yet | rendering |
| GetShouldAttack | 0x1042 | 7 | 4 | assigned to workstream | assigned to workstream E2 combat AI |
| HasBeenEaten | 0x107F | 6 | 4 | needs a system not built yet | AI (Cannibal perk) |
| SetVatsTarget | 0x11EB | 10 | 0 | needs a system not built yet | combat (VATS) |
| ForceFlee | 0x10F5 | 8 | 0 | assigned to workstream | assigned to workstream E2 combat AI |
| TriggerScreenSplatter | 0x1240 | 8 | 0 | needs a system not built yet | image space |
| IsSwimming | 0x10B9 | 0 | 7 | needs a system not built yet | movement (swimming) |
| GetVATSTargetHeight | 0x10EB | 0 | 7 | needs a system not built yet | combat (VATS) |
| GetRadiationLevel | 0x11F7 | 0 | 7 | needs a system not built yet | radiation (environment) |
| IsCombatTarget | 0x1203 | 6 | 1 | assigned to workstream | assigned to workstream E2 combat AI |
| GetIsUsedItemType | 0x10F7 | 0 | 6 | needs a system not built yet | AI (used items) |
| ShowBlackJackMenuParams | 0x1254 | 6 | 0 | needs a system not built yet | menus (casino) |
| ShowRouletteMenuParams | 0x1256 | 6 | 0 | needs a system not built yet | menus (casino) |
| SetAllVisible | 0x115A | 5 | 0 | needs a system not built yet | detection |
| EnableNavMesh | 0x11C0 | 5 | 0 | needs a system not built yet | navigation |
| GetOwnerLastTarget | 0x124E | 5 | 0 | assigned to workstream | assigned to workstream J karma and crime |
| ShowSlotMachineMenuParams | 0x1252 | 5 | 0 | needs a system not built yet | menus (casino) |
| SystemSave | 0x127B | 5 | 0 | needs a system not built yet | saving |
| StartCannibal | 0x107E | 4 | 0 | needs a system not built yet | AI (Cannibal perk) |
| GetVATSMode | 0x10E0 | 2 | 2 | needs a system not built yet | combat (VATS) |
| PlayBink | 0x1114 | 4 | 0 | needs a system not built yet | video |
| IsPlayerGrabbedRef | 0x11D0 | 3 | 1 | needs a system not built yet | physics (grab) |
| ShowBarberMenu | 0x11F1 | 4 | 0 | needs a system not built yet | menus (appearance) |
| ResetPipboyManager | 0x1201 | 4 | 0 | needs a system not built yet | Pip-Boy |
| AttachAshPile | 0x1211 | 4 | 0 | needs a system not built yet | rendering (death effects) |
| AddCardToPlayer | 0x125B | 4 | 0 | needs a system not built yet | Caravan |
| GetUsedItemActivate | 0x1103 | 0 | 3 | needs a system not built yet | AI (used items) |
| SetNoAvoidance | 0x115B | 3 | 0 | needs a system not built yet | AI |
| GetGroupTargetCount | 0x11A1 | 0 | 3 | assigned to workstream | assigned to workstream E2 combat AI |
| PlaceAtReticle | 0x1263 | 3 | 0 | needs a system not built yet | HUD (reticle) |
| GetArmorRating | 0x1051 | 1 | 1 | assigned to workstream | assigned to workstream E2 combat AI |
| PickIdle | 0x1064 | 2 | 0 | needs a system not built yet | animation |
| GetSandman | 0x10E2 | 0 | 2 | needs a system not built yet | AI (Mister Sandman perk) |
| setAllReachable | 0x1159 | 2 | 0 | needs a system not built yet | navigation |
| SetSceneIsComplex | 0x115D | 2 | 0 | deliberately no-op | performance switch of the original's AI |
| IsPlayerActionActive | 0x1170 | 2 | 0 | needs a system not built yet | player input |
| SetDisposition | 0x1184 | 2 | 0 | needs a system not built yet | dialogue (disposition) |
| GetCauseofDeath | 0x118D | 1 | 1 | assigned to workstream | assigned to workstream E2 combat AI |
| SetCellImageSpace | 0x1196 | 2 | 0 | needs a system not built yet | image space |
| DisableNavmesh | 0x11BF | 2 | 0 | needs a system not built yet | navigation |
| ShowTutorialMenu | 0x11E3 | 2 | 0 | needs a system not built yet | menus (tutorial) |
| AgeRace | 0x11E4 | 2 | 0 | needs a system not built yet | character appearance |
| ShowPlasticSurgeonMenu | 0x11F2 | 2 | 0 | needs a system not built yet | menus (appearance) |
| ForceSave | 0x1217 | 2 | 0 | needs a system not built yet | saving |
| AutoDisplayObjectives | 0x1225 | 2 | 0 | implementable, left for later | objectives already show at the left as scripts set them; what the switch adds is not documented beyond the summary |
| SetScreenSplatterFade | 0x1243 | 2 | 0 | needs a system not built yet | image space |
| GetForceHitReaction | 0x1259 | 0 | 2 | assigned to workstream | assigned to workstream E2 combat AI |
| EnableLoadingMenu | 0x1270 | 2 | 0 | needs a system not built yet | menus (loading) |
| StartMisterSandman | 0x107C | 1 | 0 | needs a system not built yet | AI (Mister Sandman perk) |
| GetCannibal | 0x10E3 | 0 | 1 | needs a system not built yet | AI (Cannibal perk) |
| trapUpdate | 0x110C | 1 | 0 | needs a system not built yet | physics (traps) |
| IsWaterObject | 0x1130 | 0 | 1 | needs a system not built yet | AI (drinking) |
| PreloadMagicEffect | 0x1163 | 1 | 0 | deliberately no-op | loading: effect particles load when drawn |
| GetPlantedExplosive | 0x11AC | 0 | 1 | assigned to workstream | assigned to workstream E2 combat AI |
| ExitGame | 0x11B5 | 1 | 0 | needs a system not built yet | menus (credits, main menu) |
| GetIsUsedItemEquipType | 0x11E0 | 0 | 1 | needs a system not built yet | AI (used items) |
| ClearScreenSplatter | 0x1241 | 1 | 0 | needs a system not built yet | image space |

Notes on the most used:

- **SwapTexture** (383 calls) [observed: decoded scripts]: `ref.SwapTexture "##<node>:<n>"
  "<texture>"`, mostly the end slides (`interface\endgame\endscrn_*` on `##gs01`), the
  Rorschach cards of Doc Mitchell's test (`pipboycard01:0`) and the Euclid's C-Finder's
  screen. No GECK wiki page. Implemented 2026-10-16: the game keeps the last texture per
  shape of each reference (`fnv_game::functions::misc::swap_texture`), the engine puts
  `textures\<texture>.dds` on the drawn shape of that name (nvforever `texture_swap.rs`);
  `##` is dropped [hypothesis]. Not part of saves.
- **GetVATSValue** (250 conditions): VATS is not built; outside VATS 0 is what the
  conditions read anyway [hypothesis].
- **PlayMagicShaderVisuals / StopMagicShaderVisuals** (284 calls): effect shaders (EFSH)
  are not drawn yet. A rendering system, not the aid workstream's actor effects.
- **SetActorAlpha** (127): see-through actors (Dead Money's holograms) need a per-actor
  blended material; not built.
- **IsActorTalkingThroughActivator** (92 conditions): conversations through talking
  activators (intercoms, radios) are not built; with none, 0 is the right answer.
- **SystemSave / ForceSave** (7): saves exist now (docs/design/save-load.md) and Autosave
  is implemented; these two are not.
- **PushActorAway** (35): the engine's ragdolls (`ragdoll.rs`) are for corpses; a pushed
  live actor has to be knocked down and get up again (GetKnockedState), not built.

## 5. Deliberately no-op

| Function | Why |
|---|---|
| SetRumble | controller rumble: the PC version has none |
| PurgeCellBuffers | the original's cell buffers; cells here load and unload by distance |
| PreloadMagicEffect | effect particles load when drawn |
| SetSceneIsComplex | a performance switch of the original's AI |
| IsXBox, IsPS3 (0), IsWin32 (1), IsGoreDisabled (0) | already `Impl`: the PC version, gore on |

## 6. Used but not registered

| Function | Opcode | Calls | Conditions | Note |
|---|---|---|---|---|
| GetIsObjectType | 0x11B1 | 0 | 1 | a form type by code (xEdit `ptFormType`); the Form Type IDs table is not in the local GECK wiki export |

AddItemToLeveledList, IsDLCInstalled and GetXPForNextLevel were registered in §3.2.

## 7. Checks

- `cargo test -p fnv-game --release`: `functions.rs`
  `audited_stubs_answer_from_the_engine_state_and_leave_requests`, `package_choice.rs`
  `reset_ai_drops_the_script_package_and_the_codes_follow_the_package`, the `chargen.rs`
  unit tests for added points and tags, and the `functions::stubs` tests (the tables name
  stubs only; every stub has a status); for §3.2 `functions.rs` `second_round_stubs`,
  `package_choice.rs` `set_pack_duration_ends_the_package_and_set_actors_ai_stops_the_choice`
  and `fnv-sim` `synthetic.rs` `entries_added_to_a_leveled_list_count_and_persist`.
- `python tools/smoke run`: `script-stubs` (the survey's counts) and `script-stubs-play`
  (Look and the weather in Goodsprings), `script-stubs-2-play` (§3.2).
