# NVForever status map (2026-10-16)

What is built, what is partly built and what is not started, per system. Built from the
design and spec notes, [owner-review.md](owner-review.md), the script stubs audit
([design/script-stubs.md](design/script-stubs.md)) and the merged PR titles (#1–#302); it
states the project's own state, not facts about the original game (those are tagged in the
linked notes). Update the row when a PR changes a system's state.

- **Done**: built and checked (parity lab, smoke scenario or in-game capture), open points
  are small.
- **Partly**: the core is built; the gaps are named.
- **Not started**: nothing of it is built (script functions that need it are stubs).
- **Awaiting owner**: built with guessed values or a design choice the owner has to confirm;
  every one is listed, per workstream, in [owner-review.md](owner-review.md).

Checks referred to below: the parity lab (`parity-lab/`, scenarios per layer `L1`–`L5`,
compared with independent readers and in-game captures) and the smoke runner
(`tools/smoke`, 123 game-run scenarios in `tools/smoke/scenarios.toml`).

## 1. Foundations

| System | State | Specifics | Notes |
|---|---|---|---|
| Archives (BSA) and plugins (ESM/ESP), load order | Done | Read and rewritten byte for byte; 70 of 107 record types have layouts (`schemas/fnv`), nothing undecoded at the end of a record in the 10 masters; xEdit cross-check agrees on 465,016 records | [layer1](design/layer1.md), [bsa](spec-notes/bsa.md), [esm](spec-notes/esm.md) |
| Assets: NIF, KF, DDS | Done | 160/160 NIF block types, every block of the game decodes and re-encodes exactly; all textures agree with an independent reader; archive precedence settled in game (#101) | [layer2](design/layer2.md), [nif](spec-notes/nif.md), [dds](spec-notes/dds.md) |
| Install layout | Done | NVForever's home is `Documents\My Games\NVForever` (or `NVFOREVER_HOME`; caches in `%LOCALAPPDATA%\NVForever\cache`); `NVForever.ini` keeps the game's folder, asked for in a folder dialog at the first start; its own `plugins.txt` in the home says which plugins load and in what order (`*` turns one on; new plugins in Data are added off); the game's required files are checked before every start; never writes into the game folder or `My Games\FalloutNV` | [install-layout](design/install-layout.md) |
| SpeedTree | Done, awaiting owner | `.spt` trees generated (`fnv-spt`), leaf dimming and wind; field meanings are hypotheses checked against billboards | [speedtree](spec-notes/speedtree.md) |

## 2. World and rendering (Layer 3)

| System | State | Specifics | Notes |
|---|---|---|---|
| World streaming, terrain, LOD | Done | Cells stream around the camera (5×5), terrain with measured texture repeat, distant LOD at four levels with fades, generated distant terrain, object LOD, imposters by region (#206) | [layer3](design/layer3.md), [generated-lod](design/generated-lod.md), [world](spec-notes/world.md) |
| Sky, weather, time of day, water | Partly, awaiting owner | Sky colours, clouds, sun path, moon and stars, weather, water with reflections and underwater fog. All 7 interior reference frames meet the image threshold, 1 of 8 exterior ones: exterior lighting strength and clouds are the known gaps | [layer3](design/layer3.md) §5, [world](spec-notes/world.md) |
| Materials, lights, shadows | Done, awaiting owner | Game-style light falloff, emissive, glass, decals, soft cut-out edges, sun shadows off by default since 2026-10-13 (owner decision; `--sun-shadows` or the settings page turns them on, #274), light rays and glows (#201) | [layer3](design/layer3.md) |
| Grass | Done, awaiting owner | Grown from the landscape textures; placement rule and light weights are ours (fitted) | [grass](spec-notes/grass.md) |
| Image space and modifiers | Partly | Worldspace/weather/interior grade; script screen effects (the opening's white screen and blur, #170). Not built: depth of field, motion blur, screen splatter (TriggerScreenSplatter etc.), SetCellImageSpace | [image-modifiers](spec-notes/image-modifiers.md) |
| Particles and effect shaders | Partly | Particle systems for shot and impact effects (#179); explosions, cave water, dust vortex and SwapTexture (#291); effect shaders' membranes (EFSH fill and edge: PlayMagicShaderVisuals and magic effects), SetActorAlpha, SetSecuritronExpression, Reset3DState (claude/effect-shaders). Not built: effect shaders' particles, holes and add-on models; ash piles | [particles](spec-notes/particles.md), [effect shaders](spec-notes/effect-shaders.md), [script-stubs](design/script-stubs.md) §3.6 |
| Enhanced graphics (optional) | Done | `--graphics enhanced` or the settings page: SSAO, finer and longer shadows, bloom, sharpening, 16× anisotropic filtering, TAA, sun shafts, screen-space water reflections. Vanilla is the default and every test run's look. Not attempted: volumetric fog; contact shadows and CAS tried and dropped | [graphics-enhanced](design/graphics-enhanced.md) |
| Loading screens | Done | Shown during travel between places | [loading-screen](design/loading-screen.md), [loading-screens](spec-notes/loading-screens.md) |
| Developer console | Done | `` ` `` opens it: script functions by name (`player.additem 0000000F 100`), `help`, `prid`, `coc`, `tgm`; a click selects what is under the pointer and shows its details. Not built: `tcl`, `set`, several statements on a line | [console](design/console.md) |

## 3. Actors

| System | State | Specifics | Notes |
|---|---|---|---|
| Appearance | Done, awaiting owner | Skeleton, body, FaceGen (race + NPC values, skin tint, hair morphs), worn armour, creatures, weapons holstered and drawn | [static-actors](design/static-actors.md), [actors](spec-notes/actors.md) |
| Animation | Partly | Idles (PlayIdle, package and dialogue idles), locomotion, turning, head tracking, PlayGroup, weapons' moving parts, melee combos, lip sync from `.lip` files, NPC sneaking (Always Sneak, SetForceSneak; `sneaking.rs`), load-door fades. Not built: actors' jumps, sneaking weapon clips of NPCs, GetAnimAction/IsLeftUp/PickIdle | [animation](spec-notes/animation.md), [idles](spec-notes/idles.md), [lipsync](spec-notes/lipsync.md) |
| Ragdolls | Partly | Corpses fall as jointed bodies from the skeletons' Havok data; stored corpse poses (XRGD, #245); the dead player's body too, with the death camera (slow motion, blur; `death_cam.rs`). Not built: knocking a live actor down and getting up (PushActorAway, GetKnockedState), armour ragdoll templates, IsFacingUp | [ragdoll](spec-notes/ragdoll.md), [dead-on-spawn](spec-notes/dead-on-spawn.md) |
| Actor values | Done, awaiting owner | From the records and the GECK wiki's formulas; `fnvtool actor values` | [actor-values](spec-notes/actor-values.md) |
| Player body and camera | Done, awaiting owner | Animated body, first-person arms rig and optional first-person body, orbit camera, Pip-Boy on the arm, player shadow, lying in beds, sitting | [player-body](design/player-body.md), [camera](spec-notes/camera.md), [furniture](spec-notes/furniture.md) |

## 4. Simulation, physics and play (Layer 4)

| System | State | Specifics | Notes |
|---|---|---|---|
| Collision and physics | Done | Havok shapes as rapier3d bodies; character controller walks, jumps, sneaks and swims like the captures; clutter falls asleep; dropped items; placed statics (STAT, SCOL) never move whatever their model's Havok data says. Three accepted residuals (B11 17/20, portal_wall 5/6, swim_h2o_fresh 8/9: owner said stop) | [layer4](design/layer4.md), [collision](spec-notes/collision.md), [physics](spec-notes/physics.md) |
| Simulation core | Done | Inventories, containers, leveled lists, locks, doors (teleport and in place), containers whose model opens (lockers, safes: Open before the menu, Close after, their sound keys), clock, respawn of cells and corpses, merchants' restock, placed mines, encumbrance | [simulation](spec-notes/simulation.md), [doors](spec-notes/doors.md) |
| `--play` controls | Done, awaiting owner | Keyboard and mouse as the game's, rebindable on the settings page's Controls tab and in `[Controls]` of NVForever.ini, activation, crosshair and prompt, flashlight, aim slows to walk speed and zooms to the weapon's Sight FOV, debug give/equip switches behind a debug flag | [layer4-play](design/layer4-play.md), [controls](spec-notes/controls.md), [camera](spec-notes/camera.md) |
| Far world | Done, awaiting owner | Actors outside the loaded 5×5 cells run a low process (18 s, and the 120 s Low level, #245; the Coming level for actors heading into the loaded cells, #261); actors with Base Health 0 are dead on spawn; crash log and report | [dead-on-spawn](spec-notes/dead-on-spawn.md) |
| Saving and loading games | Done, first version | NVForever's own save files (`crates/fnv-save`, `Documents\My Games\NVForever\saves`): console `save`/`load`/`saves`, F5/F9, autosaves after travel and on `Autosave` (switches under Saving), `nvforever --load`, 512×288 thumbnails, Save and Load on the pause menu (load page newest first with thumbnails). Game, simulation and engine state round-trip (smoke `save-write` / `save-load`). A load while playing starts a new run behind a loading screen, whose window takes over once its world is in (smoke `load-handoff`). Not built: deleting saves from the menu, loading into another load order, AI progress. The original's `.fos` layout is mapped from five saves | [save-load](design/save-load.md), [save-files](spec-notes/save-files.md) |
| Main menu | Done, first version | `nvforever --play --menu`: Continue (newest save), New Game (the opening), Load Game (greyed without saves), Settings, Quit; the plugins' loading screens fade one into the next behind it; the original's logo and main menu theme read from the game's files at run time. New Game plays `Video\FNVIntro.bik` first (any key or click skips it); the mouse pointer is the game's own arrow | `engine/nvf-play/src/main_menu.rs`, `intro.rs`, `cursor.rs` |

## 5. Scripting (Layer 5)

| System | State | Specifics | Notes |
|---|---|---|---|
| Script decoding, interpreter, quests, conditions, events | Done | Every compiled script decodes and decompiles (`fnv-script`); the runtime (`fnv-game`) runs quest, object, effect and result scripts in `--play`; L5 lab 28/32 against in-game captures (4 known fails) | [layer5](design/layer5.md), [layer5-play](design/layer5-play.md), [scripts](spec-notes/scripts.md) |
| Script function coverage | Partly | 59 functions are still stubs (782 uses in the shipped scripts): most need a system not built (VATS, casino and Caravan menus, used items, perks such as Cannibal, dismemberment); 4 deliberate no-ops; GetIsObjectType used but unregistered | [script-stubs](design/script-stubs.md) |

## 6. Gameplay (Layer 6)

| System | State | Specifics | Notes |
|---|---|---|---|
| AI packages and navigation | Done, awaiting owner | Package choice by the owner's selection-order rules (quest priority, stored order), leveled package lists, navmesh paths across spaces, sandboxing, furniture, Follow/Accompany, force greets, residents fast-forwarded through waits, package flags (all but Pretend In Combat, Allow Swimming, Allow Falls, No Warn/Attack; list 5 item 34). Not built: used items (GetIsUsedItem*), EnableNavMesh/DisableNavmesh, Cannibal and Mister Sandman | [actor-behaviour](design/actor-behaviour.md), [navmesh](design/navmesh.md), [packages](spec-notes/packages.md) |
| Dialogue | Done, awaiting owner | DIAL/INFO in full, conversations with the original's selection rules, voices for 97.3 % of lines, the dialogue menu as the original's, NPC hellos and NPC-to-NPC conversations, SayTo facing, distance-based voice volume and subtitles. Not built: disposition, talking activators; rules still to confirm against captures | [dialogue](design/dialogue.md), [dialogue-play](design/dialogue-play.md), [dialogue spec](spec-notes/dialogue.md) |
| Detection and alert states | Done, awaiting owner | Perception of the player and of each other; Normal/Alert/Combat/Lost with search; the light measure is the main guess; detection lines with a group cooldown | [detection](design/detection.md), [alert](design/alert.md) |
| Combat lines | Done, awaiting owner | Combat-tab topics said by the player, NPCs and creatures: Hit, Attack, PowerAttack (the player's; NPC power attacks are not built), Death and DeathResponse, Crippled, HealthHalf/Quarter, KOed, WeaponBroke, NoAmmo, FireExplosive, with the game's timers; responses with a sound of their own (creature and robot grunts) play it instead of a voice. Not built: AvoidThreat, Yield, KnockedBack, OnFire, Poisoned and the other *Response topics | [combat-dialogue](spec-notes/combat-dialogue.md) |
| Combat, player side | Done | Weapons, ammo, projectiles, damage, spread, melee and unarmed, explosives, limbs and crippling, hit reactions, looping reloads, item condition and broken weapons, the player's weapons jamming below 75 % condition, muzzle effects (C1–C8) | [combat](design/combat.md), [combat-data](spec-notes/combat-data.md), [combat-rules](spec-notes/combat-rules.md) |
| Combat AI | Done, awaiting owner | NPCs aim, fire, strike and bite (CA1, CA2), combat styles and confidence, provocation and yielding, allies help allies, fleeing, cover, combat groups, alarms, shared searches, NPC-against-NPC fights (#247), aggro radius warnings, cowards staying out of fights (#264, #267), scripted weapon use (UseWeapon and Use Weapon packages: a spot to stand on, a shot count, always-hit and harmless shots). Not built: VATS, NPCs reloading with animations, melee weapon use | [combat-ai](design/combat-ai.md), [use-weapon](design/use-weapon.md) |
| Followers | Partly, awaiting owner | Companions hired by their own recruit scripts follow through doors, fight the player's foes, ignore friendly fire, OpenTeammateContainer (tested with Boone). Not built: the companion wheel (needs the original's menu files and a screenshot), aggressive/passive, sneaking with the player | [followers](design/followers.md) |
| Crime and karma | Done, awaiting owner | Ownership, theft, opening owned locks, trespass, murder, witnesses, kill karma; `fnvtool actor karma` | [crime-karma](spec-notes/crime-karma.md), [factions](spec-notes/factions.md) |
| Aid items and needs | Done, awaiting owner | Chems, food and meds onto actor values, potency, restore over time, limbs, RadAway, addiction and withdrawal, timers through waits and sleep, Hardcore needs, Stealth Boy, perk entry points, Buffout's carry weight. Not built on purpose: healing in an ordinary bed | [aid-items](spec-notes/aid-items.md) |
| Waiting and sleeping | Done, awaiting owner | T wait menu, beds open the sleep menu, Well Rested, refusals | [wait-menu](spec-notes/wait-menu.md) |
| Lockpicking and hacking | Done, awaiting owner | Lockpicking minigame with help, debug meter, saved broken locks; a lock picked or forced opens its door or container at once; terminals and the hacking minigame from the game's word list | [lockpicking](spec-notes/lockpicking.md), [hacking](spec-notes/hacking.md) |
| Character progression | Partly | Character creation (name, tag skills, traits, Vigor Tester), kill XP, the level-up menu (skill points and the perks page as the original's: every perk, those that can be taken first, the others faint; opened by the console's `advlevel` or `fnv_game::levelup::open`), Intense Training's attributes page (AddSPECIALPoints), perks' Quest + Stage effects. Not built: levelling up by experience (the level stays at SetLevel's or advlevel's), perks that change skill points | [chargen](spec-notes/chargen.md), [vigor-tester](spec-notes/vigor-tester.md), [levelup](spec-notes/levelup.md) |
| VATS | Not started | GetVATSValue (250 conditions) and the other VATS functions are stubs | [script-stubs](design/script-stubs.md) §4 |
| Crafting | Partly, awaiting owner | ShowRecipeMenu opens the recipe menu (workbench, campfire, reloading bench) from the RCPE/RCCT records: list by name, subcategory filter, skill and ingredient counts, making once; no icon, effects line or several at once | [crafting](spec-notes/crafting.md) |
| Repair, Caravan, casino games | Not started | ShowRepairMenu, ShowCaravanMenu, the blackjack/roulette/slot menus are stubs | [script-stubs](design/script-stubs.md) §4 |

## 7. Interface and audio (Layer 7)

| System | State | Specifics | Notes |
|---|---|---|---|
| HUD and messages | Done, awaiting owner | Health, compass, action points, condition, ammo; crosshair and prompt; subtitles; corner messages; quest added/completed and objectives; message boxes in the game's fonts; compass marks for actors (red when hostile) and map markers, "You have discovered"; the crosshair opening with the spread and red over hostiles; reputation boxes and crime infamy; the sneak word ([HIDDEN], [CAUTION], [DANGER], [DETECTED], #289); an enemy's health bar | [hud](spec-notes/hud.md), [reticle](spec-notes/reticle.md), [quest-messages](spec-notes/quest-messages.md), [message-menu](spec-notes/message-menu.md), [reputation](spec-notes/reputation.md) |
| Menus | Partly, awaiting owner | Drawn from the game's menu files: inventory, container, barter, dialogue, Pip-Boy (STATS, ITEMS, DATA with local and world map, radio; the ITEMS page has the item's picture and figures but no DPS, ammo badge or Maintain/Mod hints), "How many?" for stacks of more than 5, wait/sleep, chargen, level up, lockpick, terminals, pause and settings and the main menu in the original's look, crafting. Not built: credits, the companion wheel, repair, barber and plastic surgeon, tutorial, Caravan and casino | [pipboy](spec-notes/pipboy.md), [barter-menu](spec-notes/barter-menu.md), [dialogue-menu](spec-notes/dialogue-menu.md) |
| Audio | Partly, awaiting owner | Dialogue voice, music by place and battle/dungeon sets, sound markers and loops, animation and movement sounds, world and menu sounds, radios and the Pip-Boy radio, distance volume and panning. Not built: reverb | [audio](spec-notes/audio.md), [voice](spec-notes/voice.md), [radio](spec-notes/radio.md) |
| Video | Partly | The intro film (`Video\FNVIntro.bik`) plays before a new game from the main menu, Bink decoded by a GPL decoder (`intro.rs`, on by default since the licence was settled). Not built: the PlayBink script function (the end videos) | [script-stubs](design/script-stubs.md) §4 |

## 8. Tooling

| Tool | State | Specifics | Notes |
|---|---|---|---|
| `fnvtool` | Done | JSON command-line tool over every crate: `bsa`, `esm`, `nif`, `dds`, `spt`, `world`, `phys`, `collide`, `sim`, `script`, `dialogue`, `actor`, `combat`, `nav`, `ai`, `voice`, `setup`, `paths` | [cli-contract](cli-contract.md) |
| Parity lab | Done | Oracle-vs-candidate tests per layer with holdouts and baselines (rules in `parity-lab/AGENT.md`) | [parity-lab/README](../parity-lab/README.md) |
| Capture tools | Done | Independent Python readers (`oracle_*.py`), `nvf_capture` (xNVSE plugin running unattended jobs in the original game), reference frames, AI observer | [tools/capture/README](../tools/capture/README.md), [xnvse-capture](spec-notes/xnvse-capture.md) |
| Smoke runner | Done | `python tools/smoke run` picks the scenarios the changed files call for, shares launches, takes the game locks, one line per scenario; known-failing marks with reasons | `tools/smoke/scenarios.toml` |
| Crash reports | Done | A panic writes `Documents\My Games\NVForever\logs\crash-*.txt` with a backtrace; release builds carry line tables for file:line (`engine/Cargo.toml`) | `engine/nvforever/src/crash.rs` |
| CI | Done | Linux job on pushes to main and on every pull request (#89, #272); a guard against game data in pull requests; PR template with a clean-room checklist; issue forms (bug, crash, differs from the original, build/tool, feature, #273) | `.github/workflows/ci.yml`, `game-data-guard.yml` |
| Releases | Done, first version | Tag-triggered workflow builds Windows and Linux archives with launchers (`nvforever.cmd`/`.sh`) and the schemas; v0.1.0 pre-release 2026-10-13 (before saves, crafting and the main menu) (#270) | `.github/workflows/release.yml`, `tools/release/` |

## 9. Open items needing the owner

- Every guessed constant and design choice, per workstream: [owner-review.md](owner-review.md).
- Known-failing smoke marks that need the owner's word: `combat-violet` (Violet aims down the
  sights at 1,600 units by design) and `combat-dogs` (fewer bites than expected, by the
  combat style's attack chance).
- Not reproduced from the playtest lists: shadow flicker after leaving an interior (debug
  lines kept); the Crimson Caravan crash (the next one leaves a report).
- The companion wheel needs the original's menu files and a screenshot of the wheel.
