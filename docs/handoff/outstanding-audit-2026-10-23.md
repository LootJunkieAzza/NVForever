# What is outstanding: a concrete audit (2026-10-23)

The owner asked for "a more concrete determination on what's outstanding". This file
answers that from the repository as it stands on main at 0300c36a (after the ESM audit
groups 5–8 and three of the five low-impact lists merged). The sources: `docs/status.md`,
`docs/outstanding.md`, `docs/owner-review.md`, every file in `docs/handoff/`, the ESM gap
audit, W9's gap audit (gap-audit-2026-10-23.md), `tools/smoke/scenarios.toml`, the script
stub table and the parity lab's last recorded runs. Nothing here was re-measured in the
game; where a number comes from a document, the document is named.

## 1. The answer in short

- **Every layer is built.** Layers 1–5 are done and pass their parity labs except for
  accepted, explained residuals; Layers 6–7 are built with gaps. Of the 54 system rows in
  status.md, 40 are Done (20 of them "awaiting owner", 3 a "first version"), 14 are
  Partly (7 awaiting owner), none is Not started.
- **Nothing in the engine is known to block a playthrough**: of outstanding.md §1's 18
  playthrough blockers, 15 are done; the other three are partly built with documented
  leftovers (VATS: NVForever's own VATS mode, two VATS stubs; misc stats: 14 not counted;
  dismemberment: not by blasts or NPC melee). W9's audit ranked no open gap A ("stops a
  quest or companion") after its two A rows were fixed.
- **What is left is of four kinds**, sized below: (§2) about 300 owner checks and about 25
  owner rulings; (§3) a backlog of buildable work, 6 items of it worth doing first; (§4)
  about 15 questions only a capture of the original can answer; (§5) test and document
  upkeep.
- **The single largest unbuilt piece is the original's VATS** (hit chances, kill
  cameras): NVForever ships its own VATS mode instead, by the owner's choice.

## 2. Waiting on the owner (nothing a session can do without you)

### 2.1 Checks in the original game: about 300

`docs/owner-review.md` holds about 300 "Check:" questions, one section per PR (counted
2026-10-23). They are the guesses each PR made. The ones that matter most for
play, by area:

| Area | Checks to do first |
|---|---|
| Combat | Violet's sights/hip range (35 m vs 15 m); Fiends' advance; dogs' bite rate; stagger on every hit; Cook-Cook's flamer reach; Incinerator and frag mine damage; fall damage from a roof |
| AI | settlers joining fights from across town; the Searchlight ghouls following Edwards through a door; a woken sleeper going back to bed; Gomorrah courtesans on their markers |
| Interface | 0.93 menu dim too dark?; quantity prompt in crafting; companion wheel words and behaviour (9 checks); lockpick look against screenshots |
| Gameplay | slot payout ~1.35 a chip (too generous?); Hardcore Stimpak 30 HP over 6 s; addiction doses; difficulty damage (Very Hard vs Very Easy) |
| Look | TAA, SSAO and enhanced mode as the default; shadow flicker in third person; the scope in third person |
| Sound | fire barrels and terminals; the bullet whiz rate; region sounds' frequency |

### 2.2 Rulings: about 25

Decisions a session cannot make (outstanding.md §3, the design notes' open questions):

1. **Lab rules**: the `L5.script.decode` choice, the three `delay_*` inputs, which holdouts
   to demote, where `encodings.json` lives, and the `combat-violet`/`combat-dogs`
   known-failing marks.
2. **Pausing in conversations** ("live world") and whether walking away ends one.
3. **Layer 5 runtime readings** (stage conditions, timers, reputation tolerances, `ref`
   handling, Return) — no capture was designed for them.
4. **Assault and the faction** (karma/crime item 8) — only a sign-off.
5. **AI**: the nine decisions of actor-behaviour.md §6 (choice interval, unreachable
   packages set aside 1 h, whether `--play` runs packages by default …); navmesh.md §7
   (walk speed 86.8, ledge links off, door rules).
6. **Interface**: weapon-wheel.md §10 (slot 2, hold time, slow-motion); player-body.md §5
   (default bindings, rebinding, Pip-Boy light); a loading screen at run start.
7. **Release**: tag v0.2.0 (only the v0.1.0 pre-release exists).
8. **Repository**: old remote branches to delete; figures to place in docs.
9. **combat-cover** smoke: make it robust or mark it known-failing.

### 2.3 Captures and frames only you can make

- **Clouds** (the biggest visual gap: 2 of 8 exterior reference frames pass): add each
  frame's seconds-since-load to `frames/manifest.json`, and capture a still sky.
- **The VCG01 new-game save** (layer5.md §5).
- **Shadow flicker** after leaving an interior: run once with `NVF_PLAY_DEBUG_SHADOWS=1`
  and keep the log. **Crimson Caravan crash**: send the crash log if it happens again.
- **Victor's and Sunny's stop-and-go** (list 4 items 2–3): fixed but never seen in game.

## 3. Buildable without the owner

Ranked by what a player would notice. Each is one PR; the spec reference says where the
rules are written down.

### 3.1 Do first (noticed in normal play)

1. **NPCs at scripted doors** never run the door's OnActivate (W9 S2; 328 GetActionRef and
   578 IsActionRef calls in the masters). Survey the door scripts first.
2. **AI progress in saves** (W9 P2; status.md "Saving and loading"): packages restart on
   load.
3. **NIF light colour and dimmer controllers** (W9 §4): cave lamps flicker, Vault klaxons
   pulse; today they are steady.
4. **Combat lines not said**: AvoidThreat, Yield, KnockedBack, OnFire, Poisoned and the other
   *Response topics; super mutant and robot combat voices (list 7 item 46).
5. **Compass**: the distance figure and quest markers through more than one door (W9 P1
   left).
6. **NPCs throwing grenades**, creature weapons dropped on death (list 7 "left for later").

### 3.2 Then (rare or partial)

- **Teammate perks**: attack-speed and rate-of-fire entries (Old Vaquero, Causeless Rebel).
- **Perk entry points**: reload, equip and attack speed; knockdown; Strength requirement;
  aiming move speed; throw velocity; explosion radius (perk-entry-points.md §2).
- **Dismemberment** by blasts and NPC melee; gore decals; pinned limbs.
- **Swinging traps** (`trapUpdate`) and the tripwire's pellets hitting the player.
- **Script- and sequence-started vertex morphs** (tripwire, Pitt press); lights inside
  models; refraction on other cloaked actors; GooStart/DisintegrateStart ragdolls.
- **AI behaviours**: Idle Chatter, Observe Combat, Reaction To Player Actions, Avoid
  Radiation, Guard "Remain Near Reference", No AI Acquire, DOOR Minimal Use.
- **Combat style**: Acrobatic Dodge, threats out of combat, AvoidThreatResponse.
- **Credits** after ExitGame; GetIsObjectType (needs the form type table); holotape
  subtitles and GetNoteRead; reputation disguises and resets; the console's several
  statements on a line; the scope in third person.
- **Renderer material batching** for heavy interiors (list 7 item 47: REPCONN).
- **Particles**: the reactor's reset-on-loop, dynamite's fuse, the rock-slide boulders, wind
  and velocity modifiers; the Ripper's chain teeth.

### 3.3 Not worth building now (documented reasons)

The low-impact ESM fields left by the audit PRs (#609–#611, #615, #618): each spec note
(item-, actor-, logic-, world-, object-fields.md) names the field, its count in the
master and why it is left — mostly "no master record uses it" or "its effect is not
documented". Also: HDR/eye adaptation, fog power, light falloff exponent, BRUS bush
sounds, walla, ALOC delays, the radio voice filter, the six script stubs that each need a
whole system (two are the original's VATS).

## 4. Questions only a capture of the original answers

From outstanding.md §4, handover-2026-10-21 §2 and the design notes; each is a short
capture job with the capture tools (tools/capture):

- the Incinerator's burn rate (4 HP/s in ours, 1–2 in the original) and its wear per burst;
- the difficulty multipliers `fDiffMultHP*` (GetNumericGameSetting × 10);
- CREA "Is Ghost" (bit 28) — 124 creatures, including wild dogs the player kills;
- explosion knockdown (four explosions in the original knocked nobody down);
- teammates through doors and `Waiting`; ResetAI on a sleeper; SetPackDuration's unit;
  IsInCombat while on Alert; the force-greet Goodbye camera; quest conditions on the
  speaker; actors with no greeting; scripted items in NPC inventories;
- the Strip crowd's patrols ending mid-way; the hits split by attacker;
- by hand (a look, no capture): the generic Goodbye on Nash and Red Lucy; the end of a
  Random topic pool (#120).

## 5. Tests and documents

### 5.1 Smoke (tools/smoke)

- **426 scenarios.** 22 expectations in 9 scenarios are marked known-failing with reasons:
  `ai-packages` (9: counts drifted, two unexamined since 2026-10-16), `combat-violet` (2),
  `combat-dogs`, `anim-sounds`, `house-items` (2), `ui-sounds` (2),
  `package-duration-geckos` (3: two encode the pre-2026-10-19 package rule and need
  rewriting), `vats-mode-melee`, `knockdown-player`.
- **Fail on main without a mark**: `shoot-bottles` (2 glass impacts where 3 are wanted;
  seen on main bd54d636 on 2026-10-23), `combat-sights` (0 hip shots counted),
  `skill-books` (the test aid's two books never reach the inventory: "gave the player 2"
  then "not carried"; seen on main 0300c36a on 2026-10-23, three runs) — the last is
  new and worth tracing first.
- **Flaky** (seen both ways on main's build 2026-10-23): `flamer-burns-npc`,
  `incinerator-hurts-player` (failed 3 of 4 runs on main), `cook-cook-camp`,
  `combat-silenced-sound`, `combat-cover`, `semi-auto-rate`.
- **"now passes" marks to drop after a second confirming run**: `combat-violet` "about 20
  blows", `ui-sounds` wait slider, `vats-mode-melee` "the clip is laid over".

### 5.2 Parity lab

The last **full** run in the documents is 2026-10-09: 376/402 pass, 0 regressions; every
fail is an accepted residual (L1 xEdit normalisations, L3 exterior frames, L4 three
accepted residuals, L5 the four known). **No full run since**: two weeks and several
hundred PRs. Run `python -m fnvlab run` and record the summary.

### 5.3 Documents out of step

- `docs/outstanding.md` (dated 2026-10-18, "as of #417") is stale: its weapon-mod
  remnants (modded models, Decrease Weight, projectile speed, ammo regeneration, modded vs
  plain units) are built per status.md; its VATS line says "left: mines" where status.md
  lists more. **Recommendation:** retire outstanding.md in favour of this file plus
  status.md, or refresh it in one PR.
- `docs/status.md` header says 2026-10-17 / #417; its Releases row knows only v0.1.0;
  Crafting, Repair/casino and Followers are "Partly" with no gap named; the Dialogue and
  Waiting rows have broken sentences. The Audio row listed the region sounds under "Not
  built" (fixed in this PR).
- Items only in outstanding.md (reputation disguises, JIP death items, holotape subtitles,
  the third-person scope, particle remnants, swinging traps, the Incinerator burn rate,
  14 uncounted misc stats) and only in status.md (the *Response combat topics, the
  console's multi-statement lines, kill-camera modifiers, staggers, perk entry points) are
  merged into §3 above.

## 6. Where the ESM audit stands

`docs/handoff/esm-gap-audit-2026-10-21.md`: all 18 priority rows and all 8 medium groups
are built or closed (groups 5–8 on 2026-10-22/23: #589, #596, #602, #607); the five
low-impact sub-lists are done as PRs #609 (Items), #610 (Actors), #611 (Logic), #615
(World) and #618 (Objects), each leaving the rest with reasons in its spec note. The
audit file can be closed once #611, #615 and #618 merge.
