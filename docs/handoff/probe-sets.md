# Extender probe sets: what each covers and what the lab can match (2026-10-27)

The in-game probe sets of `tools/capture/jip_probes.py`: runtime-compiled scripts run in
the original game (xNVSE 6.4.9, JIP LN 57.30, JohnnyGuitar 528, ShowOff 184, kNVSE 37,
MCM; the owner's save `nvfairport`, at the airport), one value per row [observed]. Each
set's golden is private (`C:\fnv-private\goldens\jip\<set>.json`); each `L5.ext.*`
scenario runs the same steps in `fnvtool ext queries` and compares by row id.

How to run a set in the game, collect it and check it on a second capture:
`docs/handoff/xnvse-remaining.md` §4 and the comments at the top of `jip_probes.py`; the
step files are `tools/capture/jip-probes/<set>.steps.json` (`jip_probes.py steps`).

## The sets

Lab: rows matching on claude/ext-compiler e02b071f (merged into claude/jip-lab).
"Engine" rows need 3D, AI, combat or animation ticks that `ext queries` does not run;
"save" rows depend on state in the owner's save; neither can match in the lab, and
nothing is marked or loosened for them.

| Set | Rows | Scenario | Lab | Covers; rows the lab cannot match |
|---|---|---|---|---|
| jip-getters | 42 | L5.ext.jip.getters | 38/42 | JIP base-form getters |
| jip-forms | 24 | L5.ext.jip.forms | 21/24 | JIP form utilities |
| jip-stores | 32 | L5.ext.jip.stores | 32/32 | aux variables, ref maps |
| jip-events | 17 | L5.ext.jip.events | 17/17 | health-damage handler, main-loop callback |
| jip-hits | 49 | L5.ext.jip.hits | 27/49 | hit data in OnHit handlers; the hits are engine rows |
| jip-records | 89 | L5.ext.jip.records | 89/89 | record getters |
| nvse-forms | 110 | L5.ext.nvse.forms | 110/110 | xNVSE form functions |
| nvse-vars | 18 | L5.ext.nvse.vars | 18/18 | script and ref variables |
| nvse-refs | 12 | L5.ext.nvse.refs | 11/12 | reference walking |
| nvse-controls | 90 | L5.ext.nvse.controls | 90/90 | controls |
| nvse-actors | 64 | L5.ext.nvse.actors | 64/64 | doors, factions, NPC appearance |
| nvse-setters | 10 | L5.ext.nvse.setters | 10/10 | base-form setters |
| nvse-misc | 34 | L5.ext.nvse.misc | 33/34 | misc |
| nvse-math | 15 | L5.ext.nvse.math | 15/15 | math |
| nvse-data | 20 | L5.ext.nvse.data | 20/20 | data |
| nvse-inv | 21 | L5.ext.nvse.inv | 21/21 | inventory |
| nvse-second … nvse-eighth | 89, 25, 77, 77, 41, 44, 59 | L5.ext.nvse.second … eighth | 87, 25, 72, 76, 41, 38, 54 | the xNVSE core families (xnvse-remaining.md §3); a few save rows (player's name, inventory counts, Doc's talked-to flag) |
| nvse-keynames | 266 | L5.ext.nvse.keynames | 266/266 | key names |
| jip-fifth | 70 | L5.ext.jip.fifth | 62/70 | JIP fifth batch |
| lu-leveled, lu-leveled2 | 94, 40 | L5.ext.lu.leveled, …leveled2 | 91, 33 | N1 leveled lists: entry kinds, the player-level rule, containers, AddItem |
| nvse-flags | 42 | L5.ext.nvse.flags | 30/42 | N14 form flag 0x40 = talked to the player; save rows |
| ini-files | 39 | L5.ext.ini.files | 39/39 | N3 INI files (`--config tools/capture/jip-probes/config`) |
| jip-n7 | 89 | L5.ext.jip.n7 | 84/89 | N7 JIP actor and game odds |
| player-state | 31 | L5.ext.jip.playerstate | 31/31 | sweep-ranked getters, N8 pad |
| mcm | 13 | L5.ext.mcm | 12/13 | N2 outside the pause menu (load order + MCM's ESP); in-menu rows not probed |
| plugin-odds | 42 | L5.ext.plugin.odds | 37/42 | N9 JohnnyGuitar / ShowOff / ITR leftovers |
| projectile | 33 | L5.ext.jip.projectile | 14/33 | impact handlers on a fired round; engine rows |
| sky-sound | 193 | L5.ext.jip.skysound | 191/193 | N4 sound, weather (00171285 by name), image space; 2 save rows (current weather) |
| knvse-anim | 32 | L5.ext.knvse.anim | 18/32 | N5 kNVSE; engine rows (what an actor plays) |
| nif-blocks | 60 | L5.ext.jip.nifblocks | 26/60 | N6 NIF blocks on an NPC and a placed item; engine rows (3D) |
| nvse-events | 100 | L5.ext.nvse.events | 77/100 | N11 which events fire and with which arguments; engine rows |
| jip-events2 | 43 | L5.ext.jip.events2 | 33/43 | N11 JIP / JohnnyGuitar handlers; engine rows |
| nvse-leftovers | 16 | L5.ext.nvse.leftovers | 13/16 | N13 leftovers |
| save-set, save-read | 46, 33 | none | – | N12 state across a save (one save `nvfprobe`, removed after; owner-review) |
| rgb-ui | 34 | L5.ext.jip.rgbui | 24/34 | SetWeatherRGBColor overflow rule; HUD traits wait on UI T3 |
| gaps1 | 25 | L5.ext.gaps1 | not run yet | the scan's most used functions no other set called |
| gaps2-gaps6 | – | L5.ext.gaps2-6 | – | the counts' next functions; the extender core's questions; DPS on the skill curve |
| gaps7 | 110 | L5.ext.gaps7 | not run yet | DPS critical part; explosion, perk and package flags; dimensions, terrain, item kinds |
| gaps8 | 48 | L5.ext.gaps8 | not run yet | worldspace climate and flags, climate textures, armour audio and model paths, setters read back |
| gaps9 | 103 | L5.ext.gaps9 | not run yet | melee and unarmed DPS: five weapons' record fields, crit damage 0, CritChance +10, skill +16 |
| gaps10 | 57 | L5.ext.gaps10 | not run yet | ref map strings, RunScriptSnippet, array files (written in the game folder, removed after), impact data, ray casts, SetPosEx, sneaking, first person |
| gaps11 | 47 | L5.ext.gaps11 | not run yet | SetPosEx translate frame (Doc, the player at heading 90 and 0); item-ref health, mod flags, ammo rounds |
| plugins1 | 71 | L5.ext.plugins1 | not run yet | the most used unprobed LU, JohnnyGuitar and ShowOff functions (idles, recipes, ingestible and race flags, easing, DLL checks) |
| plugins2 | 67 | L5.ext.plugins2 | not run yet | ingestible flags on food, drink and poison; ApplyEasing's other curves; IsDLLLoaded names; map markers, pickup sounds, recipe subcategory, RemoveFormFromFormList |
| plugins3 | 21 | L5.ext.plugins3 | not run yet | GetName on 521 references near the player (none reads its own name); JohnnyGuitar perk events; SetGlobalValue; GetAvailableRadios |
| engine1 | 58 | none (engine side) | – | goldens for engine-built functions: P2P ray casts from the player, PushObject on a placed stimpak, toggle timing per frame, GetAvailableRadios forms |
| engine2 | 30 | none (engine side) | – | P2P ray cast collision flags 0-4 down and north; PlayMusic and IsMusicPlaying |
| engine3 | 10 | none (engine side) | – | straight-down ray casts 200 units from the player, with terrain height |
| glta-order | 28 | none | – | GetLoadedTypeArray element order (weapons of FalloutNV.esm and of all plugins, ingestibles) |
| data1 | 75 | none yet | – | record setters still unbuilt in the counts, each set, read back and put back; ActorHasBaseFlag |
| data2 | 54 | none yet | – | actor base flags (low) and ActorHasBaseFlag on four NPCs, NPC height, equipped data, activator water, ar_Cat |
| data3 | 32 | none yet | – | map marker names, SortFormsByType order, ListAddArray/ListAddList at index 0, unique items, RewardXPExact, JohnnyGuitar AV-change event |
| anim1 | 53 | none (engine side) | – | player idles in third person: IsAnimPlaying/IsAnimPlayingEx, ForcePlayIdle, ForceStopIdle, StopIdle, GetPlayedIdle per frame |
| anim2 | 607 | none (engine side) | – | GetPlayedIdle 30 frames after a stop; IsAnimPlayingEx on placed NPCs standing, following, and attacking the player (god mode on) |
| reload1 | 946 | none (engine side) | – | IsAnimPlayingEx 4 (reload) on the player (clip set to 1, TapControl 7 twice) and on an NPC fighting the player; DamageActorValue with god mode off and on (LU Console "tgm") |
| sound1 | 181 | none (engine side) | – | IsSoundPlaying frame by frame after PlaySound, PlaySound3D, StopSound and StopSoundAlt; GetSoundPlayers; IsSoundPlayingFromPath |

## Coverage

Of the extender functions in `schemas/script/functions.json` whose plugin is installed in
the probe game, a probe set calls (by name or alias): xNVSE 227 of 398, JIP 161 of 768,
LU 71 of 283, ShowOff 13 of 173, JohnnyGuitar 13 of 359, kNVSE 14 of 28, MCM 5 of 5.
The 40 most widely used unbuilt functions of `modlist-scan-2026-10-26.md` are all called
now (gaps1 took the last six). What to probe next is best ranked by the scan's full
per-function counts.

## Rules seen while writing probes (the original game's runtime compiler) [observed]

- Variables (ref, float, string_var, array_var) are declared before `Begin Function`.
- A variable named like a command alias does not compile: rK, rW, rJ, rN, fW seen.
- Hex literals do not compile; neither does a fractional literal for SetBaseKarma.
- Lambdas: typed parameters inside the braces; GetSelf, not `this`; a lambda of seven
  lines inside the call did not compile where four did; a handler declared with more
  parameters than the event passes crashed the game (JohnnyGuitar's AV change).
- Array elements cannot be arguments: `sv_Construct "%g" aA[0]` and
  `GetFormIDString aA[0]` fail; copy the element into a variable first. `ar_Sort` of a
  string array does not compile.
- A string_var passed to a plain string parameter, or through `%z` in a formatted
  string, is taken as its name (SetStringIniSetting, SetUIStringAlt); `$sText` does not
  compile there (SetSoundSourceFile).
- A function whose outputs are script variables (GetAngleEx, GetAnglesBetweenPoints)
  does not compile in a probe body. A function's returned array or ref goes into a
  variable before it is passed on (`ar_Size (GetFollowers)` read -1; JIP list functions
  return no array when there is nothing to list).
- One action per step: inventory changes show a frame later; a placed actor's 3D
  (blocks, animations) takes up to five frames.
- What an actor animates or how many times an event fires depends on frame timing:
  sets keep only what repeats on a second capture (`stability.py` in the session notes).
