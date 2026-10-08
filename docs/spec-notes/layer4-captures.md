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
| `L4.sim.leveled.b8` | captured 2026-10-08 unattended (`nvftest`, 3,000 draws) | pass 10/10 | B8: LVLG globals set from the console, all-levels between/at entry levels, count without each item, nesting 3–5 deep; 300 draws per case, no failed draw, globals read back unchanged |
| `L4.sim.trace.goodsprings_doc_mitchell` | captured | pass 6/6 | z after the door (+7.44 over the XTEL z) is the arrival rest height on the terrain's contact shell (physics.md §6.2); the candidate's 8693.53 is 0.34 under the golden, inside the band the game itself spreads over |
| `L4.sim.trace.goodsprings_merchant_restock` | captured | pass 4/4 | merchant restock; needs no hidden restock time (the start's place in the 7-day cycle matters, simulation.md §3) |
| `L4.sim.trace.goodsprings_merchant_midnight_restock` | captured 2026-10-08 unattended (`nvftest`, `--time hour`) | pass 11/11 | restock days (GameDaysPassed 0 or 3 mod 7); it refuted the midnight rule it was designed for |
| `L4.sim.trace.x13_enable_parents` | captured 2026-10-08 (coc into X-13, god mode) | pass 13/13 | B1 done |
| `L4.sim.trace.goodsprings_locked_safe_key` | captured 2026-10-08 unattended (`nvftest`, `--go-to-cell GSHouseInterior02`, tgm, tdetect) | pass 13/13 | key opens the lock for good (GetLocked 0) |
| `L4.sim.trace.goodsprings_locked_safe_no_key` | captured 2026-10-08 unattended (`nvftest`, player in the gas station, tgm, tdetect) | pass 13/13 after the GetLocked fix (12/13 before: level 255 reads 1, not 2) | a first run made after the key job, with the player in another cell, is not used (activation of a container outside the player's cell opens nothing) |
| `L4.sim.trace.goodsprings_drop_and_retake` | captured 2026-10-08 unattended (`nvfdoc`, `--confirm-after 2 --settle auto`, tgm, tdetect) | 4/6 after the fixes (2/6 before: scripted pick-up of a stack, drop position); the remaining 2 fail only on z, +7.44 from the player's own settled height after the door (B9, workstream 2); do not tune | dropped refs found with `created:<item>` (GetFirstRef scan) |
| `L4.phys.drops.b11_goodsprings` | captured 2026-10-08 (`mv_b11_drops`, coordinator, re-made `nvfdoc`) | 12/20 at capture, 15/20 after the fixes, 14/20 since Portal Box markers are solid again (physics.md §4.2) | B11 player row; trees open (jump-trees workstream); rock b and the maize push are the game's own spread, rock a a landing on a step edge (follow-up `mv_diag_rocks`, `mv_diag_maize`); owner asked whether drop goldens should come from repeated drops |
| `L4.phys.walk.fence_slide`, `.fence_stop` | captured 2026-10-08 (`mv_traces`) | 6/6, 6/6 | held out, passed as predicted |
| `L4.phys.walk.porch_steps` | captured 2026-10-08 (`mv_traces`) | 5/6 → 6/6 | speed kept along uphill slopes (physics.md §6.3) |
| `L4.phys.walk.running_jump` | captured 2026-10-08 (`mv_traces`) | 0/6 | air control and landing open (physics.md §6.3) |
| `L4.phys.walk.hill_run`, `.portal_wall` | captured 2026-10-08 (`mv_followup_traces`, this workstream); goldens **replaced** 2026-10-08 by a rebuild from the same raw capture with the corrected onset (owner decision; log below) | 6/6 and 5/6 (before the replacement, same candidate: 3/6 and 4/6) | no longer independent held-out evidence: their results against the corrected onset were seen before the rebuild; portal_wall's open row is the airborne z at t0.25 (physics.md §6.4) |
| `L4.phys.walk.swim_cove_dive`, `.swim_lake_shore` | captured 2026-10-08 (`sw_scenarios`, this workstream) | 3/6 and 3/8 | held out; constant swim speed refuted (stroke cycle), steep-bank exit open; inputs left as designed (physics.md §6.6). Follow-up: the inputs cannot express the game (no stroke, 348 against 339; with the stroke at 339 the dive is 6/6 in a scratch copy), and `swim_lake_shore`'s bank rows were captured with its cell not loaded (the job travelled by long `SetPos`); retire and redesign? (question to the owner, physics.md §6.6) |
| `L4.phys.walk.swim_dam_forward`, `.swim_bay_back` | captured 2026-10-08 (`sw_heldout2`) | 10/10, 8/8 | held out after the stroke cycle; passed as predicted |
| `L4.phys.drops.swim_h2o` | captured 2026-10-08 (`sw_h2o`) | fails (interior water reverted) | held out; interior water is a placed water object at −328, not XCLW (physics.md §6.6). Follow-up: placed water modelled (Water-layer phantom boxes): deep_a and shallow_a pass (2/5); deep_b, mid_a, mid_b were placed while the capture's player still swam from deep_a (SetPos keeps the state) and the lab's fresh probes cannot carry it (a scratch walk script with `place` segments reproduces them within 2.7); redesign? (question to the owner) |
| `L4.phys.drops.swim_guardian` | captured 2026-10-08 (`sw_guardian`, this workstream) | 5/6 | held out; placed water as predicted (z within 4.7, edge of the box confirmed, XCLW not water); `out_far` slides 10 units on landing (physics.md §6.6) |
| ~~`L4.sim.trace.goodsprings_respawn`~~ | `sim/goodsprings_respawn.trace.json` (kept, unused) | retired | depended on the save's hidden merchant timer (docs/design/layer4.md §7) |

**Golden replacement log.**

- 2026-10-08, `phys/move_hill_run.walk.json` and `phys/move_portal_wall.walk.json`
  (owner decision in chat, 2026-10-08; this is the only sanctioned edit of goldens so far).
  What was wrong [observed]: `capture_moves.py golden` took the movement onset at the first
  change of any coordinate. In both runs of `mv_followup_traces` the player settled 0.08–0.1
  in z one sample before the first horizontal step, so the onset, and with it every
  observation time, was one sample (0.031 s of game clock) early. The builder was fixed
  the same day (commit 774f699: the first change of horizontal position), but these two
  goldens were built before the fix (15:41, fix 15:50) and were left as built.
  What was done: the installed files were backed up privately
  (`C:\fnv-private\goldens-backup\2026-10-08-onset-fix\phys\`) and replaced by goldens
  rebuilt with the current builder from the original raw capture
  (`out\mv_followup_traces.1791470143.jsonl`, unchanged). Onset sample 5 → 6; the rows
  move by 8–10 units in x and 4–6 in z (hill_run), and by 7–8 in x/y and 4–9 in z on
  portal_wall's first two rows, its later rows stand still and barely change.
  Check against the game's own motion [observed]: in hill_run the horizontal speed over
  the first 0.25 s window is 273 units/s with the old onset and 311 with the new one,
  against 273–319 (four of five 294–319) over the five later windows. With the early onset the
  first window included 31 ms of standing still.
  A fresh re-capture with the same job (regenerated by `make_movement_jobs.py`; identical
  to the job that ran, apart from line endings) was **not** made: the capture
  directory's `jobs\` held another session's pending `aa_auto_trial`, and moving it
  aside for the run was not permitted in this session. So the comparison of a fresh
  build with the rebuild, meant to confirm the onset fix within the game's spread, is
  still open. Re-run `mv_followup_traces` (save `nvfdoc`) and compare when the capture
  directory is free.
  Pass counts (lab, same candidate, origin/main 9a57111): hill_run 3/6 → 6/6,
  portal_wall 4/6 → 5/6; no other scenario changed.
  Consequence: both scenarios were designed as held-out checks, but their results
  against the corrected onset (6/6, 5/6) were known before the rebuild was installed. They
  stay as regression scenarios and no longer count as independent held-out evidence.
- Other goldens built by the old onset rule (before commit 774f699, 2026-10-08 15:50):
  `phys/move_fence_slide`, `move_fence_stop`, `move_running_jump` and
  `move_porch_steps.walk.json` (`mv_traces`, built 15:18). On their raw capture
  (`out\mv_traces.1791469079.jsonl`) the first change of any coordinate and the first
  horizontal change fall on the same sample (onset sample 6 in all four), so they would
  not change [observed]; not rebuilt, listed for the owner. The swim walk goldens
  (16:48 and 17:07) were built with the corrected builder; drop goldens
  (`b11_goodsprings`, `swim_h2o_drops`) have no onset.

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

`leveled_b8` (unattended; 42,196 steps, about 12 minutes): `capture_steps.py cases
parity-lab/inputs/layer4/leveled_b8.cases.json --data "<Data>" --draws 300 --start "…"`,
then `capture_run.py --game "<FNV folder>" --save nvftest --kill-on-timeout <job>`, then
`samples` into a file outside the goldens directory, checked (no `failed_draws`;
`globals_read` and `globals_read_after` equal to the values asked for) before it was
copied to the new golden `sim/leveled_b8.samples.json`. Cases with `globals` set them
from the console and restore them afterwards.

`goodsprings_doc_mitchell` needs `--confirm-after 18` (the front door's loading screen),
`goodsprings_drop_and_retake` `--confirm-after 2`; `goodsprings_locked_safe_key` travels with
`--go-to-cell GSHouseInterior02`. The lock and drop jobs were made with `--time hour
--settle auto --console-first tgm --console-first tdetect`. Run each lock job in its own
`capture_run.py` session: jobs run alphabetically in one session, and the key job's
travel would leave the player outside the gas station for the no-key job.

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
| `leveled_b8_smoke` (generated: the `leveled_b8` cases with `--draws 3`) | does setting, reading back and restoring a case's global work unattended | yes: values read back as set, restored to the save's 100 (`ScroungerOdds`, `FortuneFinderOdds`); no errors |
| `ld_lock_levels` (`make_lock_drop_jobs.py`) | GetLocked / GetLockLevel at level 255 (scripted `Lock 255`, authored trunk with key), menus after scripted activation | GetLocked 1 at 255; no menu without the key, container menu with it; simulation.md §5 |
| `ld_drop_refs` (`make_lock_drop_jobs.py`) | references a drop creates, what a scripted pick-up leaves | one ref per drop (0xFF00xxxx), 50 ahead / 30 up, taken refs left out of `GetFirstRef … 0`; simulation.md §5 |
| `ld_drop_stack` (`make_lock_drop_jobs.py`) | scripted Activate on dropped stacks of 2 and 3 | gives 1 item, the stack is taken; simulation.md §5 |
| `ld_placed_stack` (`make_lock_drop_jobs.py`) | scripted Activate on a placed XCNT 12 stack (BB rounds, GSHouseInterior03) | gives 1 round, the stack then counts 1; a second activation gives 1 more; simulation.md §5 |
| `dt_teleport_rest` | rest height after Doc Mitchell's door over time; `SetPos` placements and drops at the destination, on flat and sloped terrain | 3.84 above the LAND surface, a stable height; triangulation confirmed; physics.md §6.2 |
| `dt_rest_detail` | fall traces, repeated drops, drop grids, Doc Mitchell's porch | `SetPos` drops end 3.5–11 above terrain (unreliable); porch 1.04 above the floor; physics.md §6.2 |
| `dt_door_arrivals` | eleven door teleports around Goodsprings (predictions of a first rule, shell + 0.35, written before the run; 0x00105228 ran late and 0x0010903F not at all) | rest 0.54–1.05 above statics, 3.32–3.85 above terrain; physics.md §6.2 |
| `mv_b11_clutter` | B11 clutter row | bottles bounce 50–300 units; only Trees and Terrain readable (they stop clutter); physics.md §4.2 |
| `mv_values` | eye height, reach, other speeds, long fall | eye about 123 (pick-sphere model), reach 120–123 feet-to-face at 30°, back 272.8, sneak 197.7, g 686.0, no terminal speed to 1,841; physics.md §6 |
| `mv_air` | air control calibration | in-air momentum kept, decays 0.84/loop when released, 15 units/s from a mid-air press; physics.md §6.3 |
| `mv_diag_trees_rocks` | ±30 drop grids on the B11 tree and rock | rock agrees within 4 at 7 of 9; tree 43–303 higher everywhere; physics.md §6.4 |
| `mv_diag_maize` (`make_b11_followup_jobs.py`, drops `diag_maize.drops.json`) | what moved the B11 maize drop: repeats, other heights, ±30, bare slope, agave pickable, maize disabled; every loop traced, actors within 600 counted | a landing slide straight downhill on 5° LAND in 3 of 14 drops, one loop long, 30 units from the maize too; no actor near; physics.md §4.2 |
| `mv_diag_rocks` (`make_b11_followup_jobs.py`, drops `diag_rocks.drops.json`) | spread of the B11 rock rows: 4 repeats each on rock a, rock b, open ground (interleaved), ±30 grid on rock a | open ground 0 spread; rock b 7.8 over six drops, candidate inside; rock a 9.1 over five, candidate 3.5 below all (lands on a step edge and slides down it); grid 7 of 8 within 3; physics.md §4.2 |
| `mv_trial` (`make_movement_jobs.py`) | what works unattended for movement: GetSecondsPassed, `$` of floats, bound keys, settings, held keys, a tapped jump, a fall | all of it; gravity 684–686, jump apex 61.6, run 344, walk 86.8, settings; physics.md §6.3 |
| `sw_diag`, `sw_diag2` (`make_swim_jobs.py`) | which sample scripts compile; the clock after `coc` to Boulder Beach | `fW` does not compile; a menu opens on arrival (MenuMode 1, clock stopped) until `CloseAllMenus`; xnvse-capture.md §5 |
| `sw_trial` (`make_swim_jobs.py`; a first run from `nvftest` recorded nothing) | swimming at Boulder Beach: walking in, falling in, floating, speeds, diving, jump | walking in never swims; fall stops 100 below the surface, floats 102.5 below; stroke cycle; physics.md §6.6 |
| `sw_fall_breath` | jump at the floating height; falls into water and onto land (health) | no fall damage in water; land damage k (h − 600)^1.65; died at the 3000 land fall (job aborted on reload); physics.md §6.6 |
| `sw_breath` | two minutes held under water | drowning after 10.0 s at 40 Health/s; physics.md §6.6 |
| `sw_scenarios` | held-out `L4.phys.walk.swim_cove_dive`, `.swim_lake_shore` | constant swim speed refuted (stroke cycle); steep-bank exit open; physics.md §6.6 |
| `sw_heldout2`, `sw_h2o` | held-out `L4.phys.walk.swim_dam_forward`, `.swim_bay_back`, `L4.phys.drops.swim_h2o` | physics.md §6.6 |
| `sw_banks` (`make_swim_jobs.py`; first run by long `SetPos` only, kept privately as `swim/sw_banks.run1_setpos.jsonl`) | leaving the water up banks of 12–58°, the Lake Las Vegas and Boulder Beach lines again | first run unusable: land drawn as distant LOD (owner's screenshot), frozen trials, a crash; with `cow` to each bank's cell the swimmer meets the bed with its capsule and walks out (Lake Las Vegas 27°, Boulder Beach 28–34°, Lake Mead 12.5°): the `swim_lake_shore` saw-tooth was an unloaded cell; the second run hung after four trials when the game lost the focus; physics.md §6.6 |
| `sw_banks_b`, `sw_banks_c` | the other banks (16–58°) | swimming within 2 everywhere; walks out up to 45.1°, stands at the foot of 49.3° and 58.3° (steepest walkable slope 45.1–49.3°); five trials of `sw_banks_b` had the clock stopped in the water, rerun in `sw_banks_c` with menus closed before the keys; physics.md §6.6 |
| `sw_guardian` | held-out `L4.phys.drops.swim_guardian` (placed water, `CampGuardianCaves`) | as predicted; physics.md §6.6 |
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
- **B8 leveled lists** — **done 2026-10-08** (`L4.sim.leveled.b8`, 10 cases, 300 draws
  each, unattended; the candidate passes 10/10; facts in simulation.md §4): the LVLG
  global replaces chance none at its current value, all-levels lists with the level
  between or equal to entry levels, a count without "each item", nesting 3–5 deep. The
  candidate's `World` now reads LVLG globals at their run-time values. Open: fractional
  or out-of-range global values; leveled lists resolved in containers and on actors
  rather than by `AddItem` (still [hypothesis] that they resolve the same way). Was:
  lists with an LVLG global, deep nesting, all-levels lists with entries between levels.
- **B9 Doc Mitchell z** — **done 2026-10-08** (`dt_teleport_rest`, `dt_rest_detail`, `dt_door_arrivals`: the player rests on a contact shell, 0.7 above statics and 3.5 above the terrain, within −0.18…+0.35; physics.md §6.2). Open: what picks the height inside that band; why `SetPos` drops sometimes hang 7–11 units above the terrain. Was: no capture needed; re-run the lab once workstream 2's character
  controller places the actor (the +7.44 offset is in the golden).
- **B10 placed explosives (PGRE)** — **partly done 2026-10-08** (`bl_placed_mines`): inert while the player stands in the cell; scripted `Activate` does nothing. Open: the player's own activation (attended). The 297 PGRE are spread over five masters, not only Old World Blues (simulation.md §1). Was: are the 297 OWB placed grenades live in game? Needs an
  OWB save near one (`fnvtool sim forms` lists them).
- **B11 collision layer matrix** — **designed 2026-10-08, jobs ready, not run** (the game
  was needed elsewhere): `mv_b11_drops` (player row, 20 drops, scenario
  `L4.phys.drops.b11_goodsprings`, predictions physics.md §4.2) and `mv_b11_clutter`
  (clutter row, a whiskey bottle on 11 targets, no scenario). Was: (design; unattended). Settles which Havok layers stop the
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

- **B11 follow-up (rocks, maize)** — **done 2026-10-08** (`mv_diag_rocks`, `mv_diag_maize`,
  physics.md §4.2): rock b and the maize push are the game's own spread; rock a is open
  (step edge, B13). Not settled: what triggers the landing slide (seen only on touchdown
  loops of 31 ms or more, which are not enough); a drop with the maize disabled hung in
  mid-air, so the console `Disable` + drop was only seen once.
- **B13 the player's shape near the feet** (design; overlaps the jump-trees workstream's
  tree rows). Rock a showed the game's player passing, while falling, a step edge 12.7
  units from its axis and 18 above its feet, where a radius-18 capsule would rest
  (physics.md §4.2); fence stops say 21.9 at the waist (§6.3). Drops beside one straight
  vertical edge of a flat-topped static with clear ground below, the axis at horizontal
  distances −4, 0, 4, 8, 12, 16, 20 and 24 outside the edge, three repeats each: where the
  rest height changes from the top (plus the shell) to the ground below, and the heights
  in between, give the shape's half-width against height. Prediction for a capsule of
  radius 18: feet above the top by sqrt(18² − d²) − 18 (−0.45 at 4, −1.9 at 8, −4.6 at 12,
  −9.8 at 16) and on the ground from 18. Target not chosen yet: the CollisionMarker
  0x00176AD0 of B11 is a thin wall between other collision; `fnvtool phys cell --ray`
  line scans find the edge.
- **B12 step and slope limits** (design). The highest ledge walked onto and the steepest
  LAND square run up: held keys work unattended (physics.md §6.3), so a run at ledges of
  graded heights (e.g. stacked crates, porch and road edges measured with `fnvtool phys
  cell --ray`) and up LAND squares of known steepness, positions sampled per loop, would
  settle both; targets not chosen yet.

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

## 5. Movement jobs ready to run (2026-10-08)

Made by `python tools/capture/jobs/make_movement_jobs.py` (the scenario jobs from the lab
inputs through `tools/capture/capture_moves.py job`); all start from save `nvfdoc`, leave
Doc Mitchell's house through the front door, and set god mode and AI detection off. Run
each in its own session (they all start inside the house):

```
python tools/capture/capture_run.py --game "<FNV folder>" --save nvfdoc --kill-on-timeout -o C:/fnv-private/capture/run_<job>.json tools/capture/jobs/<job>.steps.json
```

| Job | Steps | What | After the run |
|---|---|---|---|
| `mv_b11_drops` | 155 | B11 player row: 20 drops (scenario `L4.phys.drops.b11_goodsprings`) | `python tools/capture/capture_moves.py golden C:/fnv-private/capture/out/mv_b11_drops.<time>.jsonl parity-lab/inputs/layer4/b11_goodsprings.drops.json -o C:/fnv-private/goldens/phys/b11_goodsprings.drops.json` |
| `mv_traces` | 663 | four runs with held keys (scenarios `L4.phys.walk.*`) | `capture_moves.py golden <jsonl> parity-lab/inputs/layer4/move_<name>.walk.json -o C:/fnv-private/goldens/phys/move_<name>.walk.json` for `fence_slide`, `fence_stop`, `running_jump`, `porch_steps` |
| `mv_b11_clutter` | 72 | B11 clutter row (a bottle on 11 targets) | read the `bottle <target>` results; no golden |
| `mv_values` | 1,231 | eye height and reach (crosshair against the yard fence: pitch 0–70° at 40 and 80 units, distance 90–180 at 30° down), speeds backwards, sideways, diagonal and sneaking, a 2,500-unit fall | analysis into physics.md §6.3; no golden |

Predictions for the scenario jobs are in physics.md §4.2 and §6.3 (written before the
runs). For `mv_values` [hypothesis, from the candidate's placeholders]: with the eye 112
above the feet (feet 8355.85) the crosshair would leave the fence top (8422.4) at about
48.7° down from 40 units and 29.6° from 80; the reach would be 150 from the eye.
