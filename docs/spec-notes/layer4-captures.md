# Layer 4 in-game captures: log and backlog

Captures made with the `nvf_capture` xNVSE plugin on the owner's install (FNV 1.4.0.525,
Steam, xNVSE 6.4.9, the ten masters in the order of docs/spec-notes/simulation.md).
How to run a capture: tools/capture/README.md §10; what the in-game script compiler
accepts and the crashes met: docs/spec-notes/xnvse-capture.md §5. Facts learned are
recorded with their tags in docs/spec-notes/simulation.md; this file is the index.

Private files (never committed): jobs and output in `C:\fnv-private\capture\`
(`jobs\`, `jobs\done\`, `out\`, `nvf_capture.log`), goldens in `C:\fnv-private\goldens\sim\`.

## 1. Scenario goldens (status 2026-10-07)

| Scenario | Golden | Candidate | Notes |
|---|---|---|---|
| `L4.sim.containers` | `sim/containers.json` (oracle) | pass | independent oracle, not in-game |
| `L4.sim.items` | `sim/items.json` (oracle) | pass | independent oracle, 3,972 items |
| `L4.sim.trace.goodsprings_weight_and_time` | captured | pass 9/9 | GameHour/GameDaysPassed fractions not observed (clock runs in menus) |
| `L4.sim.leveled.basic` | captured (2,100 draws) | pass 7/7 | 300 `AddItem` draws per case |
| `L4.sim.trace.goodsprings_doc_mitchell` | captured | 5/6 | fails only on z after the door (+7.44): needs the character controller (workstream 2); do not tune |
| `L4.sim.trace.goodsprings_merchant_restock` | captured | pass 4/4 | merchant restock; needs no hidden restock time (the start's place in the 7-day cycle matters, simulation.md §3) |
| `L4.sim.trace.goodsprings_merchant_midnight_restock` | captured 2026-10-08 unattended (`nvftest`, `--time hour`) | pass 11/11 | restock days (GameDaysPassed 0 or 3 mod 7); it refuted the midnight rule it was designed for |
| `L4.sim.trace.x13_enable_parents` | captured 2026-10-08 (coc into X-13, god mode) | pass 13/13 | B1 done |
| ~~`L4.sim.trace.goodsprings_respawn`~~ | `sim/goodsprings_respawn.trace.json` (kept, unused) | retired | depended on the save's hidden merchant timer (docs/design/layer4.md §7) |

Substitute checks (xnvse-capture.md §7): the same scenarios regenerated with
`--time hour|timescale` or `--settle auto` (job names `cmp_<scenario>_<mode>`, kept with
the private jobs) and compared with the golden made by hand:
`capture_steps.py compare <golden> <substitute trace>`. Done 2026-10-08: restock (hour,
timescale), weight and time (hour, timescale) and Doc Mitchell (`--settle auto`) all agree
with their goldens (xnvse-capture.md §7), so these scenarios can be recaptured unattended:

```
python tools/capture/capture_steps.py actions parity-lab/inputs/layer4/<name>.actions.json --data "<Data>" --time hour --settle auto [--confirm-after N] --start "<save>" -o <dir>/<name>.steps.json
python tools/capture/capture_run.py --game "<FNV folder>" --save <save> --kill-on-timeout --trace-dir <dir> <dir>/<name>.steps.json
```
Saves: `nvftest` (inside the Goodsprings gas station) for the restock scenario, `nvfdoc`
(Doc Mitchell's house, items in place) for Doc Mitchell and weight and time. Write new
traces somewhere other than the goldens directory and compare them; never overwrite a
golden.

Regenerating a scenario job (Editor IDs resolved from the masters, read-only):

```
python tools/capture/capture_steps.py actions parity-lab/inputs/layer4/<name>.actions.json --data "<Data>" --start "<where to load>" [--confirm-after N] -o C:/fnv-private/capture/jobs/<name>.steps.json
python tools/capture/capture_steps.py cases parity-lab/inputs/layer4/leveled_basic.cases.json --data "<Data>" --draws 300 --start "<...>" -o C:/fnv-private/capture/jobs/leveled_basic.steps.json
python tools/capture/capture_steps.py trace C:/fnv-private/capture/out/<name>.<time>.jsonl -o C:/fnv-private/goldens/sim/<name>.trace.json
python tools/capture/capture_steps.py samples C:/fnv-private/capture/out/leveled_basic.<time>.jsonl --cases parity-lab/inputs/layer4/leveled_basic.cases.json -o C:/fnv-private/goldens/sim/leveled_basic.samples.json
```

`goodsprings_doc_mitchell` needs `--confirm-after 18` (the front door's loading screen).

## 2. Diagnostic and follow-up captures (hand-written jobs)

Kept in `tools/capture/jobs/` so they can be re-run (copy one into the capture
directory's `jobs\`). Results are in the notes named.

| Job | Purpose | Result (where recorded) |
|---|---|---|
| `aa_diag`, `aa_diag2`, `aa_diag3` | what `CompileScript` accepts | rules in xnvse-capture.md §5 |
| `aa_crash_test`, `aa_crash_test2` (and an unsaved test 3) | isolate the FalloutNV.exe+0x477C6D crash | low TimeScale + clock moving crashes; xnvse-capture.md §5 |
| `aa_pickup_followup`, `aa_pickup_followup2` | scripted `Activate` on items; disabled refs; exterior cell asked indoors | simulation.md §5 |
| `aa_exterior_cell` | `GetParentCell` of a persistent exterior ref, outdoors | grid cell containing the position; simulation.md §5 |
| `aa_respawn_delay` | `iHoursToRespawnCell`; 1/6/12 h away | 72 in game; merchant box separate rule; simulation.md §3 |
| `aa_vendor_restock` | merchant box: no-leave 24 h, 5 h across midnight, 10 h | restock without leaving; not midnight; simulation.md §3 |
| `aa_auto_trial` | read-only; first unattended run (`capture_run.py`): menu load, auto start, quit | works (second try; the first hung the game, fixed in the plugin); xnvse-capture.md §6 |
| `aa_time_substitutes` | how each scripted substitute for the Wait menu moves the calendar | `GameHour` past 24 and TimeScale 3600 both carry the calendar; `GameDaysPassed` alone does not; xnvse-capture.md §7 |
| `bl_calendar` (B5) | month lengths, year end, leap year | months 0-based, Gregorian lengths, no leap day; simulation.md §5b |
| `bl_encumbrance` (B7) | over-encumbered threshold, Hardcore ammo weight | ammo 0.029 each in Hardcore; no over-encumbered function compiles; simulation.md §3 |
| `bl_restock_timing`, `_ts` (B4) | restock on access or on a schedule | refilled 24 h after an access-time refill: not access time; simulation.md §3 |
| `bl_restock_schedule` (B4) | refill times, 2-hourly reads over 292 h | refills just after midnights 14, 21, 24; midnight rule; simulation.md §3 |
| `bl_primm_restock`, `_ts` (B2) | merchant box without the Respawns flag | never refilled in 7 days; simulation.md §3 |
| `bl_container_respawn`, `_ts` (B3) | 72 h cell respawn, fridge (Respawns) and toolbox | fridge: empty at 24/70 h, refilled at 74 h; toolbox never; simulation.md §3 |
| `bl_placed_mines` (B10) | are placed mines live; scripted activation | inert for 600 loops; scripted Activate does nothing; simulation.md §1 |
| `mr_diag_reads`, `mr_diag_calendar`, `mr_diag_month_short`, `mr_diag_long_untouched`, `mr_diag_ringo`, `mr_diag_untouched_3d`, `_4d` (`make_restock_jobs.py`) | why the midnight rule failed: refill times with 2- or 6-hourly reads, the save's calendar, Ringo's state | restocks only on days 0 or 3 mod 7; simulation.md §3 |
| `mr_predict_days` (`make_restock_jobs.py`) | held-out check of the restock days, made after the rule was stated | refilled on 31, 35, 38, 42 and no day between; simulation.md §3 |
| `aa_vendor_period` | merchant restock period and contents | second restock 2.50–3.01 days later; sold items replaced; simulation.md §3 |

## 3. Backlog (captures still to make)

Each item: what it settles, where to stand, and how. "Design" means a job still has to be
written.

- **B1 `x13_enable_parents`** — **done 2026-10-08** (golden captured unattended with `--go-to-cell NVDLC03X13`, god mode and AI detection off; the scenario passes 13/13). Was: scenario exists, golden missing. Needs a save inside X-13
  in Old World Blues (cell `NVDLC03X13`). Read-only: disables/enables two root parents and
  reads GetDisabled along the chains. Generate with `capture_steps.py actions
  parity-lab/inputs/layer4/x13_enable_parents.actions.json --data … --start "Inside X-13"`.
- **B2 merchant box without the Respawns flag** — **done 2026-10-08** (`bl_primm_restock`: never refilled in 7 days). Was: design. Does it restock at all? Easiest:
  Primm, Prospector's Den merchant — container 0x0017409C
  (`VendorContainerPrimmDenMerchantREF`, base `VLootSackVendorWastelandT2RSPros`, stored in
  cell `NVVendorChestsCell`), merchant 0x00173F9B in `SLProspectorsDenInt`. Same pattern as
  `goodsprings_merchant_restock` (empty, wait ≥ 72 h, read; repeat). The other six are
  listed by `fnvtool sim forms` under references.merchant_containers.without_respawns_flag.
- **B3 cell respawn on an ordinary container** — **done 2026-10-08** (`bl_container_respawn`: Respawns fridge refilled after 74 h away, not after 70 h; toolbox never). Open: the exact boundary between 70 and 74 h. Was: design. The 72 h cell-respawn rule
  (`iHoursToRespawnCell` = 72) is not yet observed on a non-merchant container. Candidate:
  the Goodsprings General Store fridge 0x00105824 (`VLootFridgeClosedTier1WastelandRS`,
  leveled contents, cell `GSGeneralStore`). Leveled contents are random, so observe the
  number of stacks (`GetNumItems`; needs a trace op or a distribution-style check) or use
  a respawning container with fixed contents. Leave the cell between empty and read.
- **B4 merchant restock: on access or on a schedule** — **done 2026-10-08**: neither as stated; restocks happen at midnights, at least 3 days after the last one, skipped while the box is untouched (`bl_restock_timing`, `bl_restock_schedule`; simulation.md §3). **Superseded later on 2026-10-08**: restocks come on fixed days, GameDaysPassed 0 or 3 modulo 7, whatever the last restock (`make_restock_jobs.py`; simulation.md §3); the "changed since" condition is dropped. Open: other merchants (is the schedule global?), and whether an untouched box with leveled stock is re-rolled. Was: design. All captures fit both.
  Idea: after an observed restock, empty the box and read it only rarely vs every hour
  around the 72 h mark; under "on access" the restock time follows the reads.
- **B5 calendar** — **done 2026-10-08** (`bl_calendar`: 0-based Gregorian months, no leap day). Was: is GameMonth 9 October? Set day 30 of month 10 (November if 0-based,
  30 days) at 20:00 (TimeScale 30!), wait 6 h, read GameDay/GameMonth. Day 1 of month 11
  means 0-based months with Gregorian lengths; day 31 means otherwise.
- **B6 fractional game time** (design). GameHour/GameDaysPassed after waits drift with
  menu time (×30); low TimeScale crashes when the clock moves. Possible: read the clock
  immediately before and after each Wait and compare differences, not absolutes.
- **B7 encumbrance edges** — **partly done 2026-10-08** (`bl_encumbrance`): Hardcore ammunition weight captured; the threshold is still open because neither over-encumbered function compiles in the runtime (an observable alternative is needed, e.g. whether fast travel or running is refused, which needs someone at the game). Was: over-encumbered threshold (weight = capacity vs > capacity),
  readable with `IsPlayerOverencumbered` / `GetIsPlayerOverencumbered` (both on the GECK
  wiki; check which compiles); ammo weight in Hardcore mode (`SetHardcore` / `IsHardcore`
  exist on the wiki; a throwaway save, since Hardcore cannot be undone in a save).
- **B8 leveled lists**: lists with an LVLG global, deep nesting, all-levels lists with
  entries between levels (new cases in a second `.cases.json`, same procedure as
  `leveled_basic`).
- **B9 Doc Mitchell z**: no capture needed; re-run the lab once workstream 2's character
  controller places the actor (the +7.44 offset is in the golden).
- **B10 placed explosives (PGRE)** — **partly done 2026-10-08** (`bl_placed_mines`): inert while the player stands in the cell; scripted `Activate` does nothing. Open: the player's own activation (attended). The 297 PGRE are spread over five masters, not only Old World Blues (simulation.md §1). Was: are the 297 OWB placed grenades live in game? Needs an
  OWB save near one (`fnvtool sim forms` lists them).
- **B11 collision layer matrix** (design; unattended). Settles which Havok layers stop the
  player and which stop dropped items (physics.md §4 and §6 question 4; today
  `layers::character_ignores` is chosen from the layer names), and doubles as a check of
  the Havok scale (collision.md §6). Method: a **drop test**, which needs no one at the
  controls. For each layer, pick a few references of that layer with a flat top in open
  ground (`fnvtool phys cell` lists every collider with its layer and bounds; prefer
  exterior Goodsprings and interiors already used by other jobs). Then, with god mode on:
  1. *Player*: `player.setpos x/y` over the object's centre and `player.setpos z` to
     200 units above its top; wait about 3 s of game loops; read `player.GetPos z`.
     Resting near the top (as for the rug, within a couple of units) = the layer blocks
     the player; resting on what lies below = it does not. Repeat with the same drop over
     bare ground next to it as the control.
  2. *Clutter*: `player.placeatme` a light clutter item (a tin can, MISC with Clutter
     collision) and move it over the object with `<ref>.setpos`; read its `GetPos z`
     after a few seconds. This fills the clutter row of the matrix.
  Layers to cover first: Static, AnimStatic, Transparent, Clutter, Trigger, Water,
  Non Collidable, Invisible Wall, Collision Box, Debris Small/Large, Prop, trees
  (SpeedTree, no Havok blocks: does the player fall through or rest on a hidden shape?)
  and CollisionMarker boxes. Candidate side: `fnvtool phys cell --drop x,y,z` gives the
  same rest height per probe, so each case can become a keyed `L4.phys` scenario
  (rest z with a tolerance of a few units). Risk: an object that moves when hit (dynamic
  clutter) falls with the probe; use statics for the player row.

## 4. Owner answers (2026-10-08, in game, attended)

Recorded where the rule lives, tagged [observed by the owner in game, 2026-10-08]:

- A locked container with its key in the inventory opens directly on activation, no
  prompt (simulation.md §5).
- NPC carried-item entries with count 0 give nothing (simulation.md §3).
- Notes, casino chips, caravan money and caravan cards weigh nothing, like caps
  (simulation.md §3; `Inventory::totals` counts them as 0).
- Disabled references are not drawn, have no collision and cannot be interacted with
  (simulation.md §1, physics.md §4; `fnvtool phys cell` now also leaves out references
  disabled through their enable parent).
- `GetPos Z` is the feet: the player standing on a rug at z 7360 in Doc Mitchell's house
  reads 7361.05 (physics.md §6).

