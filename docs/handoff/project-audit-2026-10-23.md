# Project audit: completion, data types, scripts, docs and playthrough blockers (2026-10-23)

The owner asked for "a complete audit of all the data types, scripts, known gaps and where
our documentation is stale … a complete picture on project completion and major playthrough
blockers". Unlike [outstanding-audit-2026-10-23](archive/outstanding-audit-2026-10-23.md), which was
gathered from documents, this one checks the claims against the code on main at 5e803939
(after #618). Six read-only passes ran in parallel: ESM record types, script functions and
events, a main-quest walk, stale documents, test health, and the other systems, formats and
add-ons. Nothing was run in the game. Tags: [observed] = read in code, data or logs;
[documented] = from a named doc; [hypothesis] = a guess.

> **State on 2026-10-25 (main at #799).** Struck through below where later PRs closed an
> item. Closed since this audit: the event blocks and script unlocks (#632), NPC door
> OnActivate (#729), the non-combat perk entry points (#672, #674, #680, #685, #692, #701,
> #715), the embedded creature weapons (esm-audit-2026-10-25 row 14), AI progress in saves
> (patrol points and sandbox days, #730), the credits (#750), NPC grenades (#739),
> dismemberment by blasts and NPC blows (#748), the *Response combat lines (#734), the
> package behaviours (#757), companion perks (#715) and light colour controllers (#658).
> Still open from §7: the headless main-quest walkthroughs (3), the Hoover Dam load test
> (6), melee VATS specials (8, now an owner question: owner-review.md item 31), a recorded
> full parity-lab run (9) and the documentation sweep (10, partly done 2026-10-25). The
> smoke runner has 553 scenarios (431 here).

## 1. The answer in short

- **Every layer has code, and every function the shipped scripts call is registered**
  (6 stubs, 4 no-ops, 1 unregistered opcode left; §3) [observed]. Of 107 record types,
  13 are fully used, 78 partly and 16 not at all (§2) [observed].
- **No full playthrough has ever been attempted.** In the engine, play has been seen
  through the opening, Goodsprings, Primm, Nipton, Novac and REPCONN only. The Strip, the
  Lucky 38, every faction path, Cottonwood Cove, the Fort, Hoover Dam, the end slides and
  every add-on have never run (§4) [observed: no smoke scenario, no handoff report].
  outstanding-audit's "nothing in the engine is known to block a playthrough" is true only
  because nothing past Novac has been looked at.
- **New blockers found by this audit** (not in any earlier list):
  1. **Five script block types never fire** (*fixed 2026-10-23 on
     claude/script-events-door-locks: four now fire; OnSell stays unfired because it never
     ran in the original either, GECK wiki: "never ran prior to xNVSE v6.2.8"*): OnOpen, OnClose, OnSell, OnActorEquip and
     OnMagicEffectHit. OnOpen alone stalls Honest Hearts' *Prisoners of War*
     (NVDLC02MQ05aDoor never sets stage 100) and the Legion's turn in Render Unto Caesar
     (NVCCPlayerStorage, VMQ02 stage 110) [observed: `crates/fnv-game/src/game.rs` event
     table has no constant for them].
  2. **Doors unlocked by scripts stay locked for NPC pathing** (*fixed 2026-10-23 on
     claude/script-events-door-locks: `Ai::apply_door_locks`*): `NavGraph::set_door_lock`
     is never called outside tests. Escort and travel packages through quest-unlocked doors
     may be set aside as unreachable. `VMQYesMan01MoveToTerminal` already shows as
     unreachable [observed].
  3. ~~**About 38 perk entry points are never evaluated**~~ (*built 2026-10-24/25, #672–#715; perk-entry-points.md §7–§14*), so those perks can be taken and do
     nothing. Among them are Light Step, Computer Whiz, Infiltrator, Hoarder, Pack Rat,
     Travel Light, Living Anatomy, Jury Rigging, Mysterious Stranger, Miss Fortune,
     Meltdown, Better Healing, Silent Running and Concentrated Fire (§2.2) [observed].
  4. **Melee and unarmed VATS specials are missing**: WEAP VATS/VANM (Mauler, Lights Out,
     Grand Slam) are unread [observed].
  5. ~~**A creature's built-in weapon survives the limb that carries it being crippled**~~ (*built: esm-audit-2026-10-25 row 14, `fnv-combat` data.rs*):
     WEAP DNAM embedded-weapon actor value (70 weapons) is unread [observed].
- **The verification baseline is stale**: the last full parity-lab run (2026-10-19,
  372/387, 0 regressions) is recorded in no document, 531 commits ago. About a quarter of
  smoke scenarios have no recent log, and `skill-books` fails on main with no mark (§5).
- **About 120 document lines are stale**, worst in status.md, outstanding.md, the design
  notes' "not built" lines and the handoff files (§6).

### 1.1 Completion picture

| Area | State |
|---|---|
| Formats (BSA, ESM, NIF/KF, DDS, SPT, FaceGen, LIP, Bink) | Done: everything decodes. Menu XML is ported by hand and not interpreted, so mods' and add-ons' menu files are ignored |
| World and rendering | Mostly done. Gaps: cloud placement (2/8 exterior frames), environment and cube-map reflections, HDR/eye adaptation, light colour/dimmer controllers, draw-call batching |
| Actors and animation | Mostly done. Gaps: NPC grenade throws, staggers, blast and NPC-melee dismemberment, ragdoll feedback (RGDL), creature KFFZ lists |
| Simulation and physics | Done. Gap: AI progress is not kept in saves |
| Scripting | Done for functions. Gaps: 5 event blocks, NPC door OnActivate, AddPerk/HasPerk on NPCs |
| Gameplay (AI, combat, dialogue, progression) | Built with gaps: about 38 perk entry points, the original's VATS (hit chances, kill cameras, melee specials), package behaviours (Observe Combat, Idle Chatter …), the *Response combat lines, companion perks |
| Interface and audio | Mostly done. Gaps: credits, hotkeys (1, 3–8), audio attenuation curves, loop points, walla |
| Play-tested content | Opening to Novac only. The add-ons: none |

Our judgement [hypothesis]: the engine is feature-complete enough to *attempt* a
playthrough. The remaining risk lies in quest flow that nobody has run, more than in
missing systems.

## 2. Data types (ESM records)

Method: `fnvtool esm stats`/`coverage` over the ten masters with a fnvtool built from main,
then grep of every consumer [observed]. 92 of 107 types have a schema layout (status.md
says 70). 1.90 % of subrecord instances are not decoded by the schema (58,181 of
3,056,241); most of these are read raw in code. Nothing is left over at the end of a
record; the two 2-byte QUST DATA fail to decode, harmlessly (RadioTenpenny,
GenericSupMutBehemoth have no stages or scripts).

### 2.1 Coverage by type

**Full (all fields that matter are used):** ACTI, ADDN, ALCH, AMEF, AMMO, ANIO, ARMA, ARMO,
AVIF, BOOK, CCRD, CDCK, CHAL, CHIP, CLMT, CMNY, CONT, CSNO, CSTY, DEHY, DIAL, ECZN, ENCH,
EYES, FLST, FURN, GLOB, GMST, GRAS, HAIR, HDPT, HUNG, IDLM, IMOD, INFO, IPDS, KEYM, LAND,
LSCR, LTEX, LVLC, LVLI, LVLN, MESG, MICN, MISC, MSET, MSTT, MUSC, NOTE, NPC_, PGRE, QUST,
RADS, RCCT, RCPE, REGN, REPU, SCOL, SCPT, SLPD, SPEL, TACT, TERM, VTYP. (Some of these
have cosmetic fields left; the per-type field notes name them.)

**Unused (no runtime reader):** CAMS and CPTH (VATS kill cameras), RGDL (ragdoll feedback,
pose matching), LSCT (load-screen layout), NAVI (the graph is built from NAVM), INGR (one
nameless record).

**Partial, with the fields that matter:**

| Type | Unused fields → impact |
|---|---|
| PERK | ~38 entry-point codes never evaluated → **noticeable** (§2.2) |
| WEAP | VATS/VANM melee specials (490/86 records); DNAM embedded-weapon actor value (70); grip and power-attack overrides → **noticeable** |
| PACK | Observe Combat, Reaction To Player Actions, Idle Chatter, Avoid Radiation, Guard Remain Near Reference → noticeable |
| IMGS / IMAD | HDR block (eye adaptation, bloom); IMAD HDR tracks; IMAD RDSD/RDSI (21, in no schema or doc) → noticeable look |
| SOUN / ASPC / ALOC | attenuation curve, loop points, Random Location; walla; ALOC delays → noticeable audio |
| CREA | KFFZ (202) resolved but unused; TNAM turn speed; Swims/Walks; Is Ghost → low–noticeable |
| REFR | WMI1 map-marker reputation; XACT, XTRG, XCHG, XLTW, XPWR, XDCR → low |
| WRLD | "Don't Allow NPC Fall Damage" (Dead Money's two west town worldspaces) → low |
| DOOR | Automatic, Minimal Use (decoded, never read), Sliding → low |
| DEBR / CONT DEST | destruction-stage debris never spawned → cosmetic |
| CELL, LGTM, LIGH, WTHR, WATR, PWAT, EFSH, MGEF, PROJ, EXPL, IPCT, TXST, TREE, STAT, RACE, CLAS, FACT, BPTD, IDLE, NAVM, ACHR/ACRE, DOBJ | cosmetic or none (fog power, falloff exponent, BRUS, inverse edge, Off By Default lights, placed TXST decals, NAVM cover values …) |

### 2.2 Perk entry points

Evaluated today: codes 0, 1, 2, 6, 9, 10, 11, 21, 23, 25, 27, 29, 34, 36, 40, 44, 56, 57,
58, 68 [observed: `crates/fnv-game/src/perks.rs`]. Every other code the masters use is not
evaluated. Non-combat: 4 (Light Step, In My Footsteps), 12, 20 (Computer Whiz), 22
(Hoarder), 28 (Mysterious Stranger), 31, 32 (Infiltrator), 41 (Miss Fortune), 42 (Travel
Light), 46 (Meltdown), 47 (Living Anatomy), 48 (Jury Rigging), 51, 55 (Pack Rat), 69–71
(Enhanced Sensors, Spotter, Search and Mark). Combat: 8 (To Hit), 33, 35, 37–39, 43, 52–54
(Run 'n Gun), 59/72, 61–66 (unarmed moves). perk-entry-points.md §2 lists only the combat
leftovers.

## 3. Scripts

### 3.1 Functions

- 401 opcodes are used by the masters' scripts and conditions [observed]. **6 stubs**
  (SetVatsTarget 10 calls, GetVATSTargetHeight 7 conditions, trapUpdate 1, and
  GetForceHitReaction, IsWaterObject, GetPlantedExplosive as conditions), **4 no-ops**
  (PurgeCellBuffers, SetRumble, SetSceneIsComplex, PreloadMagicEffect), and **2
  unregistered**: GetIsObjectType (one condition) and opcode 0x127C (`GetInCharGen` in one
  OWB script; `script-stubs.toml` `unhandled_used` misses it).
- Registered but partial [observed]:

| Function | Uses | What is missing |
|---|---|---|
| AddPerk / RemovePerk / HasPerk on NPCs | 37 / ~100 / ~20 | Fail or answer 0. 25 are companion hire lines (`<companion>.AddPerk CompanionSuite`); the effect is covered by the teammate fix but each hire logs a failed call; Stonewall and Light Step checks on NPCs always fail |
| GetMapMarkerVisible | 41 (VMQ01 among them) | Ignores Hidden and authored-Visible flags |
| SetMinimalUse | 1 | Stored, never read |
| ForceRadioStationUpdate | 3 (add-on MQ00) | Counter only |
| ExitGame | 1 | Main menu, no credits |
| GetCauseofDeath | 1 | Poison and radiation not told apart |
| AddAchievement | 152 | Store only (fine without Steam) |

### 3.2 Events (script blocks)

Fire: GameMode, MenuMode, OnActivate (player and script-driven), OnAdd, OnEquip,
OnUnequip, OnDrop (player drops only), SayToDone, OnHit, OnHitWith, OnDeath,
OnDestructionStageChange, OnFire (player weapons; all 6 shipped scripts are on them),
OnStartCombat, OnCombatEnd, OnPackageStart/Done/Change, OnLoad, OnReset,
OnTrigger/Enter/Leave, OnGrab, OnRelease, ScriptEffectStart/Update/Finish [observed].

**Never fire:**

| Block | Scripts | Lost |
|---|---|---|
| OnOpen | 3 | **Honest Hearts NVDLC02MQ05a stage 100 (quest stalls)**; VMQ02 stage 110 via NVCCPlayerStorage; Goodsprings schoolhouse safe flag |
| OnActorEquip | 1 | The Legate stops healing after his healing powder (Hoover Dam) |
| OnMagicEffectHit | 5 | Vault 22 gas and door-buster traps |
| OnSell | 2 | LR Bowie knife and GRA Esther keep their spell when sold |
| OnClose | 2 | LR eyebot loot container; one empty block |

**NPCs opening doors** never run the door's OnActivate (`engine/nvforever/src/residents.rs`
opens the door directly), and NPC errands activate with a mode that skips OnActivate.
There are 181 door scripts, 67 of which test a non-player action ref, including the
Hoover Dam (VHD*) doors, Lucky38EntranceDoor, the Strip gate, the Tops and the Fort
gates [observed].

**Result scripts** of quest stages, dialogue begin and end, packages, terminals, patrol
points, perk activate choices, magic effects and challenge rewards all run [observed].

## 4. Playthrough walk

Method: all 14,902 script units of FalloutNV.esm, the four story add-ons and GRA were
decompiled and searched per quest, against the function registry and the 431 smoke
scenarios [observed].

| Quest / stage | State | Evidence |
|---|---|---|
| VCG00/01 Doc Mitchell, character creation | Works (the face creator is NVForever's own, awaiting owner) | smoke film-opening, vigor-tester, chargen-menus …; lab L5.intro |
| VCG02 Back in the Saddle | Works, flaky | sunny-wells; shoot-bottles flaky on main |
| VCG03 By a Campfire | Works | crafting, compass-quest-target |
| VMS16 Ghost Town Gunfight | Parts work; Run Goodsprings Run untested | trudy-gunfight-armed |
| VMQ01 They Went That-a-Way | Pieces tested (Primm, Nipton, Novac, REPCONN); Boulder City untested | follower-boone, crucified-stay, intercom-goodbye-line |
| Ring-a-Ding-Ding: Strip gate, Tops, Benny | **Untested** (only the Strip crowd) | strip-crowd-evening |
| Lucky 38, Mr. House, VMQHouse | **Untested** | — |
| Yes Man (VMQYesMan) | **Untested**; MoveToTerminal unreachable in `fnvtool ai reach` | actor-behaviour.md §7 |
| NCR path (Kimball's speech, vertibird) | **Untested** | — |
| Legion path (Cottonwood Cove, the Fort's confiscation, arena) | **Untested** | — |
| Hoover Dam (vHDBattleController, ~300 named actors) | **Untested; performance unmeasured** | — |
| End slides (VEndingScript, NarratorScript, SwapTexture) | Built, never run | — |
| Credits | ~~Missing~~ Built 2026-10-25 (#750), never run at the end of a playthrough | gap-audit S3 |
| Add-ons (DM, HH, OWB, LR, GRA) | Every called function registered; no quest played. HH Prisoners of War stalls (OnOpen) | — |

### 4.1 Blockers, ranked

Hard means the player cannot progress; soft means play degrades.

1. **Untested main quest past Novac** (likely hard, unknown where). The one owner session
   at REPCONN found a real blocker (the intercom); expect more on the Strip, at the Lucky 38
   and the Fort.
2. **OnOpen never fires** (hard for Honest Hearts; Legion branch wrong).
3. **Script unlocks not passed to NPC pathing** (possibly hard: escorts and quest moves
   through unlocked doors).
4. **Hoover Dam scale** (possibly hard: frame rate or stalls). REPCONN with 26 actors
   already takes 15–29 ms a frame; meshes are drawn one draw call each; teleports hitch ~1 s.
5. ~~**NPCs at scripted doors skip OnActivate**~~ (*done, #729*) (soft to hard: 67 scripts, finale doors among
   them).
6. ~~**Saves drop AI progress**~~ (*patrol points and sandbox days kept since #730*) (soft: a save mid-scene, during Kimball's speech or the
   battle, may leave actors out of step). The longest saves on this machine hold under 15
   minutes of play; one frozen load is unexplained (save-load.md §6).
7. **Ending**: slides never run; ~~no credits~~ (credits built, #750) (soft).
8. **Perks that do nothing; no melee VATS specials; no original VATS** (soft but widely
   noticed).
9. **Companions**: perks on NPCs fail, no Hardcore ammo use, ED-E crafts at the player's
   skill, only Boone and Beagle tested (soft).

## 5. Tests and verification

- **Smoke**: 431 scenarios (410 game, 21 tool). The 22 known-failing marks in 9 scenarios
  are unchanged; the ai-packages marks are 12 days old and unexamined. Two
  package-duration-geckos marks encode the pre-10-19 rule. Re-judging the newest log of
  each scenario across all checkouts gave 295 pass, 19 fail and 5 known; **112 have no log
  on this machine**.
- **Failing without a mark**: skill-books (on main), shoot-bottles, combat-sights. Newly
  failing on branches: combat-sights-beyond-reach (1998 vs ≥2001),
  combat-shotgun-reach (597 vs ≥601), actor-creator (facial hair 32 vs 47),
  death-cam-third (ragdoll never rests). Flaky: flamer-burns-npc, incinerator-hurts-player,
  cook-cook-camp, combat-cover, combat-silenced-sound, semi-auto-rate.
- **No scenario selects** `crates/fnv-script` (decode, vm, ir, unit),
  `crates/fnv-game/src/runner.rs`, `fnv-sim/src/inventory.rs`, most of `fnv-voice` and
  `fnv-world`, `engine/nvforever/src/limbs.rs` or `nvf-play/src/dialogue_menu.rs`.
  Pickpocketing has no scenario, and no code.
- **Parity lab**: last full run 2026-10-19 (`NVForever-observer/parity-lab/runs/latest.md`):
  372/387, 13 fail, 2 error, 0 regressions; L1 109/115, L2 56/56, L3 92/94, L4 87/90, L5
  28/32. **L5.script.decode now has 71 mismatches; layer5.md explains 44.** No lab covers
  Layers 6–7.
- **Unit tests**: 2,095 over 287k lines. Thin: fnvtool (8 tests / 15k lines), fnv-nif (24
  / 7k; the lab is its real check), engine/nvforever (229 / 53k), fnv-script (27 / 3.8k).
- **CI**: green (no failure in the last 100 runs). Gaps: no Windows job; `tools/stress/tests`
  is not run though CLAUDE.md lists it.
- **Code markers**: no `todo!`/`unimplemented!`/TODO/FIXME. Stale comments:
  `fnv-game/src/awareness.rs` ("no combat groups yet"), `functions/player.rs:21`
  (teammate perks), `lockpick.rs:510` (crime), `script_hud.rs:119` (character creation),
  `nvf-play/src/ambient.rs:34` (region sounds), `control.rs:27` (QuickSave),
  `fnv-actor/src/records.rs:68` (NAM6).

## 6. Stale documents

| Document | What is wrong |
|---|---|
| `docs/status.md` | Header 2026-10-17/#417; "205 scenarios" (431); "70 of 107 layouts" (92); "160/160 NIF block types" (229 in the schema, 178 concrete); Releases knows only v0.1.0 (tags go to v0.4.0); "Done" for events (5 blocks never fire) and materials (no environment maps); still lists as unbuilt: console multi-statement lines, AvoidThreat, throw phases (built for the player), particle controllers, leveled locks (none in the masters), compass quest markers; omits hotkeys, pickpocketing, perk entry points beyond combat, melee VATS, menu XML; Crafting, Repair and Followers "Partly" with no gap named |
| `docs/outstanding.md` | As of #417; contradicts outstanding-audit (combat-cover, release, smoke process: "workers do not run the game" vs CLAUDE.md) |
| `docs/handoff/archive/outstanding-audit-2026-10-23.md` | "3 of 5 lists merged" (all 5); 426 scenarios; "nothing blocks a playthrough"; lab "no run since 10-09" (one on 10-19); v0.2.0 "to tag" |
| `docs/handoff/esm-gap-audit-2026-10-21.md` | Rows 8, 9, 17, 18 built; FACT XNAM, NPC_ KFFZ (the gap is CREA KFFZ), CDCK wrong; misses WEAP VATS/VANM, embedded weapon, WRLD fall damage, IMAD RDSD |
| `docs/handoff/gap-audit-2026-10-23.md` | "No new ESM gap" (see above); knockdown-player PASS (now marked); shoot-bottles PASS vs outstanding-audit's fail |
| `docs/handoff/handover-2026-10-21/22.md` | Strike through CAMS/CPTH, RGDL, debris, HDR as if built: they were closed, not built |
| `docs/handoff/archive/queue-2026-10-20.md`, `playtest-2026-10-12.md`, `playtest-list-4/5.md`, `site-update-2026-10-16.md` | Finished; still list built work as open |
| `CLAUDE.md` | Points at queue-2026-10-20 as the live queue (finished) and lists playtest lists 1–5 (list 7 is live) |
| `docs/cli-contract.md` | 28 fnvtool commands undocumented; title says Layer 1 |
| `docs/development.md` | Layout lacks fnv-save, tools/stress, tools/play, tools/release; test list lacks stress and clippy |
| `parity-lab/README.md` | Image comparator "open" (built); capture plugin "not yet run in game" (v9, used) |
| `docs/design/script-function-coverage.md` | Totals of 23 and 43 gaps; knockdowns, fast travel, repair, Caravan, radiation, grab, etc. marked missing; `function_systems.toml` not re-run |
| `docs/design/script-stubs.md` | GetKnockedState and IsFacingUp described as missing knockdowns (built); misses 0x127C |
| `docs/design/layer5.md:166-168` | OnSell "runs"; events "from the scripted runner until Layer 6" |
| Design notes' "not built" lines | alert.md (combat groups, crimes), combat-ai.md (fleeing, groups, weapon choice), combat.md:418 (NPCs don't shoot), dialogue-play.md (GetSitting, head tracking), layer3.md:7 (clouds), layer4-play.md:439 (grab, VATS, saves …), layer5-play.md:451 (chargen), player-body.md:60 (Pip-Boy), vats-mode.md:437 (perk AP), layer1.md:19 (licence) |
| Spec notes | nif-node-audit.md (~28 moved line anchors), combat-rules.md, animation.md, controls.md, camera.md, audio.md:533, level-design.md, message-menu.md, grass.md, world.md:375/640, esm.md:202-233, weapon-data-fields.md:7 (145 AMMO is all masters; FalloutNV.esm has 92), skill-books.md:19-25 (27 books, 17 with a skill), effects-animation-audit.md:48 |

**Recommendation** (2026-10-25: the archive move, the CLAUDE.md pointers and most of the
table above done in the docs sweeps): move the finished handoff files to `docs/handoff/archive/`; fold
`outstanding.md` into one list with this file; regenerate the coverage table from an
updated `function_systems.toml`; replace fixed counts in status.md with pointers to the
tools; point CLAUDE.md at this file, gap-audit §7 and playtest list 7.

## 7. What to do next, ranked

1. ~~Fire OnOpen, OnClose, OnActorEquip, OnMagicEffectHit~~ (done 2026-10-23; OnSell
   deliberately not fired, as in the original).
2. ~~Pass script unlocks to NPC pathing~~ (done 2026-10-23, `Ai::apply_door_locks`).
3. **Headless main-quest walkthroughs**: `fnvtool script run` chains like
   `walkthrough/intro.actions.json` for each main-quest chain, then one smoke scenario per
   milestone: the Strip gate, the Lucky 38 elevator, the Fort's confiscation, the Hoover
   Dam panel, VEnding. This turns "untested" into a list.
4. ~~**NPC door OnActivate** (gap-audit S2)~~ (done 2026-10-25, #729).
5. ~~**The non-combat perk entry points** (Light Step, Computer Whiz, Infiltrator, Pack Rat,
   Hoarder, Travel Light, Living Anatomy, Jury Rigging …)~~ (done 2026-10-24/25, #672–#715).
6. **Hoover Dam load test** with the stress tools; renderer batching.
7. ~~**AI progress in saves.**~~ (done 2026-10-25 for patrol points and sandbox days, #730; the rest starts again from the packages, save-load.md §5)
8. **Melee VATS specials** (WEAP VATS/VANM; now owner-review.md item 31) ~~and embedded creature weapons (DNAM)~~ (built).
9. **Run the full parity lab** and record it; explain L5.script.decode 71 vs 44; trace
   skill-books on main.
10. **Documentation sweep** per §6.
