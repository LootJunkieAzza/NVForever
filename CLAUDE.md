# CLAUDE.md — standing instructions for AI sessions on NVForever

NVForever is an open-source, clean-room reimplementation of the Fallout: New Vegas engine
(OpenMW-style). The owner wants sessions to work autonomously, including pushing.

## Charter (non-negotiable)

- Clean room: describe formats/behaviour in plain language in `docs/spec-notes/`, implement
  from those notes. No leaked Bethesda/Gamebryo source. No DRM circumvention (the Steam
  `FalloutNV.exe` code section is encrypted; do not unpack it).
- Tag every factual claim `[documented]` (cite page/file), `[observed]` or `[hypothesis]`.
  Never invent offsets, opcodes or field meanings.
- Ship no game data. Goldens and game-derived dumps stay private and are never committed
  (`.gitignore` blocks game files).
- Work in layer order (docs/design/layer1.md, layer2.md, layer3.md; Layer 3 = world/rendering is in progress).
- nv-rs (slaterain/nv-rs) is a source of format facts only: never copy its code (its
  policy admits leaked Xbox symbols and translated decompiled code; owner decision
  2026-10-07). nif.xml is documentation only (no codegen) to keep the licence open.
- NVForever never writes into the game folder or `Documents\My Games\FalloutNV`
  (`crates/fnv-install`, docs/design/install-layout.md).
- Use the Rust ecosystem fully (owner decision); plan in docs/design/dependencies.md.

## Parity lab rules (parity-lab/AGENT.md)

Never edit goldens, scenarios, tolerances or comparators to make tests pass; never
special-case inputs; never read holdouts or pass `--reveal`; never run `fnvlab baseline`.
Fix or explain every regression.

## Checks before every push

```
cargo test --workspace --release
cargo clippy --workspace --all-targets --release -- -D warnings
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
(cd engine && cargo clippy --release --all-targets -- -D warnings)   # Bevy workspace (viewer + game)
```

With game data available (owner's install, read-only): build `fnvtool`, set
`parity-lab/lab.local.json`, run `python -m fnvlab run` and report the summary.
After renderer changes, also run the moving-camera smoke test (cell streaming and LOD
rebuilds; fixed-camera screenshots do not exercise them):
`engine/target/release/nvforever --data <Data> --cell Goodsprings --auto-fly 4000 --run-seconds 40`
(expect exit 0 and "run finished").
After sky or lighting changes, also run the day/night time-lapse (a full day in about
20 s; one "time-lapse" log line per game hour; `--timelapse <dir>` saves a frame per hour,
add `--pos -69500,2500,13400 --pitch -89.5` for the aerial view):
`engine/target/release/nvforever --data <Data> --cell Goodsprings --play --hour 17 --time-scale 1.2 --run-seconds 26`
(expect exit 0, "run finished" and 24+ hourly samples).
After image space modifier changes (`fnv_world::image_modifier`, the `fnv-game` functions,
`imagespace.rs` and its shaders; a sleep's fade to black is drawn there too), run the opening for half a minute:
`NVF_PLAY_TEST_SCRIPT_MENUS=1 engine/target/release/nvforever --data <Data> --play --new-game --run-seconds 32`
(expect exit 0 and the "image space modifiers on the screen:" lines in this order: WhiteISFX,
MQ08AllBlackedOutISFX; VCG01FadeInFromBlackISFX; that with VCG01PlayerStrainISFX;
VCG01PlayerStrainISFX; none. docs/spec-notes/image-modifiers.md §6).
After navmesh, pathing or travelling-NPC changes, run the headless NPC walks in
docs/design/navmesh.md §5 (`fnvtool nav walk`, all must arrive) and the travel smoke test:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --npc-travel DocMitchellREF=GSChetRef --npc-travel SunnyRef=EasyPeteRef --follow-npc --run-seconds 105`
(expect exit 0 and "nav summary: 2 of 2 travellers arrived").
After AI package changes (`fnv-game` `ai`/`package`, `fnv-ai`), run `fnvtool ai packages`
(expect `all_agree` true), `fnvtool ai reach` and, with the owner's observer captures,
`fnvtool ai observed` and `fnvtool ai run --samples`; compare with the figures in
docs/design/actor-behaviour.md §4 and §5.3 and explain any change. After changes to
`fnv-ai` or the engine's residents, also run the residents smoke test:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --npc-packages EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 21.9 --follow-npc --run-seconds 150`
(expect exit 0, "ai summary … 3 residents, 6 packages chosen, 4 trips, 4 arrivals, 0 unreachable"
and Easy Pete sitting on 0010634A, getting off it, and on his bed 00176ACE in the "furniture:" lines).
After changes to Dialogue packages (`fnv-ai` `goal::talk`, `carry` `talk_step`;
docs/design/actor-behaviour.md §5.6), also run the force-greet smoke test, with
`NVF_PLAY_TEST_SCRIPT_MENUS=1` and `NVF_PLAY_TEST_SCRIPT=14:activate:ProspectorSaloonDoor` set:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --play --npc-packages SunnyRef --no-voice --run-seconds 45`
(expect exit 0, "SunnyRef: walks up to 0x00000014 to talk", "dialogue: conversation with SunnyRef started
(topic GREETING)" and "1 talks started by dialogue packages" in the ai summary).
After actor value changes (`fnv-game` `actor_values`, `functions::actor`), run
`fnvtool actor values --schema schemas/fnv <Data> FalloutNV.esm --survey --actor player --actor SunnyRef`
and compare with docs/spec-notes/actor-values.md §3.1 (the fit table) and §7 (Sunny Smiles:
Guns 35, Health 100; the player: every skill 15, Action Points 80); explain any change.
After changes to the Wait and Sleep menu, to what sleeping does or to the people during
a wait (`nvf-play` `wait`, `fnv-game` `rest`, `fnv-ai` `pass_time`, the engine's residents;
docs/spec-notes/wait-menu.md §11), run the sleep smoke test, with
`NVF_PLAY_TEST_NEAR=000CEB03`, `NVF_PLAY_TEST_ACTIVATE=16` and
`NVF_PLAY_TEST_WAIT=24:hours=3,25:accept` set:
`engine/target/release/nvforever --data <Data> --cell NovacPlayersRoom --play --hour 22 --run-seconds 36`
(expect exit 0, "wait menu: open as sleep", three "sleep: hour" lines, "on screen: You are now
Well Rested!" and "1 sleeps told to a bed's script" in the wait summary), and the wait with
residents, with `NVF_PLAY_TEST_WAIT=30:open,31:hours=4,32:accept` set:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --play --npc-packages EasyPeteRef,GSChetRef,GoodspringsSettler04Ref --hour 18.5 --run-seconds 50`
(expect exit 0, "3 there, 0 on their way" and "4 hours passed in the wait menu (2 walks in them)"
in the ai summary).
After detection changes (`fnv_game::detection`, the engine's `detection.rs`; docs/design/detection.md),
run `fnvtool actor detect --schema schemas/fnv <Data> FalloutNV.esm --detector SunnyRef` and compare
with the table in detection.md §4 (sneaking, lit: detected out to 1,840; sneaking, dark: never);
explain any change. Then the in-game check of detection.md §5 (expect exit 0, a "hidden → noticed"
line for GSSettlerAM 00104F0A while sneaking, the shot taking it to detected, and a "detection summary").
After detection state changes (`fnv_game::awareness`, `fnv_ai::alert`, the engine's `alert.rs`
and `detection.rs`; docs/design/alert.md), run the Fiend check of alert.md §8:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --pos=-43900,64896,7000 --play --hour 12 --no-voice --npc-packages FiendVioletRef --run-seconds 45`
(expect exit 0, "1EFiendViolet 000F570B Normal → Alert: noticed the player", her NormalToAlert
line, "FiendVioletRef: searches for the player", "Alert → Combat: detects the player" at about
2,650 units with no Combat ↔ Lost flicker after it, and "alert summary: 1 actors became Alert").
After combat AI changes (nvforever `npc_combat.rs`, `nvf_play::combat` `ActorFire`/`ActorStrike`/`By`,
`fnv_ai::alert` closing in; docs/design/combat-ai.md), run the two checks of combat-ai.md §7:
`NVF_PLAY_TEST_GOD=1 NVF_PLAY_TEST_SCRIPT_MENUS=1 engine/target/release/nvforever --data <Data> --cell Goodsprings --pos=-43900,64896,7000 --play --hour 12 --no-voice --npc-packages FiendVioletRef --run-seconds 45`
(expect exit 0, "fights the player with WeapHuntingRifle (gun)", "stops and faces the player" at about
1,600 units, "shots are drawn from its weapon's muzzle", "2hrattack3.kf" in the "attack anims summary"
and a dozen "Violet hits the player for 44.7"), and the dogs, with
`NVF_PLAY_TEST_SCRIPT=3:combat:0x000F573F:player,3:combat:0x0013E5BB:player` added and
`--pos=-45600,65000,7000 --npc-packages 000F573F,0013E5BB --run-seconds 30` (expect about 20 blows in the
"npc combat summary", the dog's `h2hattack…` clips in the "attack anims summary", and only a few
"runs at"/"stops and faces" lines: they stop at biting range, not against the player).
After changes to actor templates, leveled actor lists or faction functions (`fnv-game`
`templates`, `functions::actor`), run
`fnvtool actor factions --schema schemas/fnv <Data> FalloutNV.esm --survey --actor player --actor FiendVioletRef --actor 00157B36`
and compare with docs/spec-notes/factions.md §2–§5 (the coyote 00157B36: factions source
CrCoyote, four factions; the pick against the actor drawn: 3,003 same, 0 differ); explain any change.
After furniture changes (docs/spec-notes/furniture.md §7), run the sitting smoke test:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --npc-sit EasyPeteRef=0010634A:8 --follow-npc --run-seconds 45`
(expect exit 0 and "furniture summary: 1 of 1 actors sat down, 1 got up again").
After music, placed sound or background loop changes (docs/spec-notes/audio.md), run
`fnvtool world audio <Data> FalloutNV.esm --schema schemas/fnv` (compare the counts with
audio.md) and the music smoke test:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --play --pos=-69000,3500,8900 --hour 12 --run-seconds 40`
(expect exit 0, "music: musSetLOCDesExplBothABC day started, layers outer+middle+inner",
"music summary: 1 started, 0 changes of layers, 0 files not played" and "ambient summary:
2 placed loops started, … 1 background loops started"; the count of random sounds varies).
After radio changes (`fnv_world::radio`, `fnv_game::radio`, `nvf-play` `radio`;
docs/spec-notes/radio.md §6), run the saloon jukebox, switched off at 20 s and on at 28 s, with
`NVF_PLAY_TEST_SCRIPT_MENUS=0.3`, `NVF_PLAY_TEST_NEAR=00107D73` and `NVF_PLAY_TEST_ACTIVATE=20,28` set:
`engine/target/release/nvforever --data <Data> --cell GSProspectorSaloonInterior --play --no-voice --run-seconds 40`
(expect exit 0, "00109087 (vCountryRadio) is destroyed: silent", "00107D73 tuned to vCountryRadio
playing sound\songs\radionv\…_mono.ogg", "switched off", the same file again "from <s> s" (20–25 s, by frame rate) and
"radio summary: 1 receivers heard, 1 songs started, 0 DJ lines, 0 lines without a file; receivers
switched 3 times, 1 on at exit").
After changes to the player's controls or to what scripts lock of them (`nvf-play`
`control::ControlLocks`, `script::sync_locks`; docs/spec-notes/controls.md §5), run the
held-player smoke test, with `NVF_PLAY_TEST_SCRIPT_MENUS=1`, `NVF_PLAY_TEST_MOVE=1:1:0`,
`NVF_PLAY_TEST_LOOK=0.1:90:0`, `NVF_PLAY_TEST_ACTIVATE=14` and
`NVF_PLAY_TEST_CONTROL=16:sneak,17:weapon,18:view` set:
`engine/target/release/nvforever --data <Data> --play --new-game --no-voice --run-seconds 30`
(expect exit 0, "look heading 90.0 pitch 0.0 ignored", "activate: ignored" and "controls summary:
scripts changed the player's controls 2 times; disabled at exit: movement, Pip-Boy, fighting, POV,
sneaking; held back: movement <frames>, fighting 1, POV 1, looking 1, sneaking 1").
After actor collision or pushing changes (`nvf-play` `blocking`, docs/spec-notes/physics.md §6.8),
run the push smoke test (the player stands on Easy Pete's noon walk):
`NVF_PLAY_TEST_SCRIPT_MENUS=1 NVF_PLAY_TEST_GOTO=1:-67790:3450:8425 engine/target/release/nvforever --data <Data> --cell Goodsprings --play --npc-packages EasyPeteRef --hour 12 --run-seconds 30`
(expect exit 0, Easy Pete's "used door 0x0010636F", and in the "actors pushed aside:" line
"one the player in" about 200 steps: he pushes the player ahead of him and goes on).
After idle animation changes (docs/spec-notes/idles.md §7: `fnv-game` `idles`, `fnv-actor`
`idle_play`, the engine's `idles.rs`, the player body's idles or view), run the opening:
`NVF_PLAY_TEST_SCRIPT_MENUS=0.05 engine/target/release/nvforever --data <Data> --play --new-game --no-voice --run-seconds 90`
(expect exit 0, "run finished", "VCG01 stage 55 set", "idle summary: 10 idles asked for (1 by scripts,
4 by packages' begin, end and change, 1 from collections, 4 by dialogue lines); 5 started on actors,
5 on the player; 0 for actors not drawn, 0 without a usable clip", and Doc Mitchell getting off his
chair "by clip SpecialIdle_DocSitLeftExit" in the "furniture:" lines).

After changes to the wait and sleep menu (`nvf-play` `wait`, docs/spec-notes/wait-menu.md
§11) or to the play clock it advances (`fnv_sim::World::advance_hours`, `sky_follows_game`),
run the sleep smoke test, with `NVF_PLAY_TEST_NEAR=00176ACE`, `NVF_PLAY_TEST_ACTIVATE=16`
and `NVF_PLAY_TEST_WAIT=22:open,23.5:hours=24,24.5:accept` set:
`engine/target/release/nvforever --data <Data> --cell GSHouseInterior03 --play --hour 22 --run-seconds 40`
(expect exit 0, "wait menu: open as sleep (the player lies on a bed)", 24 "sleep: hour N of 24
passed:" lines ending one date later at the time of day the menu opened at, and "wait summary:
the menu was opened 1 times (1 as sleep), 24 hours passed, 0 requests held by scripts").

After changes to the messages at the top left (`nvf-play` `corner_messages`, `interface`,
`script_events` `Note`; docs/spec-notes/corner-messages.md §7), run two of them, with
`NVF_PLAY_TEST_SCRIPT_MENUS=0.3` and
`NVF_PLAY_TEST_SCRIPT=3:message:FactionReminderNCR,3.1:message:AbilityWellRestedStartMsg` set:
`engine/target/release/nvforever --data <Data> --cell GSHouseInterior03 --play --no-voice --run-seconds 20`
(expect exit 0 and, in this order, `corner message: "You are still dressed as a member of the NCR."
for 2 s, icon glow_message_vaultboy_ncr.dds` and `corner message: "You are now Well Rested!" for
10 s, icon default`, about 2.25 s apart).

After changes to the container or barter menus (`nvf-play` `menu`, `menu::look`, `menu::barter`,
`fnv-game` ShowBarterMenu; docs/spec-notes/barter-menu.md §6), run the barter from Chet's
dialogue, with `NVF_PLAY_TEST_TALK=10:GSChetRef=for sale|leave`, `NVF_PLAY_TEST_BARTER=*` and
`NVF_PLAY_TEST_SCRIPT_MENUS=0.3` set:
`engine/target/release/nvforever --data <Data> --cell GSGeneralStore --play --no-voice --run-seconds 50`
(expect exit 0, "menu: a script opens the barter menu with 0x00104C79", "stock in 0x00104C7A",
"menu: Accept: Traded", "menu: close" and then the conversation's "Goodbye." choice).

After changes to the quest lines at the left ("Quest added", objectives; `nvf-play`
`quest_messages`, `script_events` `QuestNotice`; docs/spec-notes/quest-messages.md §7), run them,
with `NVF_PLAY_TEST_SCRIPT_MENUS=0.3` and
`NVF_PLAY_TEST_SCRIPT=3:objective:VCG02:3,9:objective_done:VCG02:3,9.1:objective:VCG02:5` set:
`engine/target/release/nvforever --data <Data> --cell GSHouseInterior03 --play --no-voice --run-seconds 45`
(expect exit 0 and, in this order, `quest message on screen: Quest added: BACK IN THE SADDLE`,
`quest message on screen: Talk to Sunny Smiles in the Prospector Saloon.` and
`quest message on screen: COMPLETED: Talk to Sunny Smiles in the Prospector Saloon. | Meet Sunny
Smiles behind the Prospector Saloon.`).

After changes to the sounds animations and doors name (nvforever `anim_sounds.rs`,
`fnv_actor::anim_sounds`, `nvf_play::doors` sounds; docs/spec-notes/audio.md §10), run the
footsteps, reload and doors check, with `NVF_PLAY_TEST_EQUIP=0000434F`,
`NVF_PLAY_TEST_GIVE=00004241:50`, `NVF_PLAY_TEST_MOVE=11:1:0,14:0:0` and
`NVF_PLAY_TEST_CONTROL=15:weapon,17:fire,17.2:fire,18:reload` set:
`engine/target/release/nvforever --data <Data> --cell Goodsprings --play --no-voice --open-doors --run-seconds 24`
(expect exit 0 and an "anim sounds summary:" line with about 5 footsteps by the player,
"3 reload keys", "5 door sounds (2 from door records)" and "0 keys without a SOUN"; the
actors' footsteps and rustles vary).

After changes to who turns to whom while a line is said (`fnv_game::game::Game::facings`,
nvforever `speech_look.rs`, `fnv_ai::social` `face_listeners`; docs/design/dialogue-play.md §9),
run Sunny's lines in the saloon, with `NVF_PLAY_TEST_SCRIPT_MENUS=0.3` and
`NVF_PLAY_TEST_SCRIPT=14:say_to:0x00104E85:0x0010588E:GREETING,20:say_to:SunnyRef:player:GREETING,26:say_to:SunnyRef:0x0010588E:GREETING:1` set:
`engine/target/release/nvforever --data <Data> --cell GSProspectorSaloonInterior --play --no-voice --run-seconds 34`
(expect exit 0, "CheyenneREF listens to SunnyRef", each "line … over: SunnyRef has … off its front"
within a few degrees of 0, no "listens" line for the last one, and "speech look summary: 3 lines";
add `--no-npc-packages` to see "the body turns" by speech_look.rs itself).

## Commits

End commit messages with the session's attribution lines. Keep commits focused; write
what changed and what was verified.
