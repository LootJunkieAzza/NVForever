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
| `L4.sim.trace.goodsprings_merchant_restock` | captured | pass 4/4 | save-independent merchant restock |
| `L4.sim.trace.x13_enable_parents` | **missing** | — | backlog item B1 |
| ~~`L4.sim.trace.goodsprings_respawn`~~ | `sim/goodsprings_respawn.trace.json` (kept, unused) | retired | depended on the save's hidden merchant timer (docs/design/layer4.md §7) |

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
| `aa_vendor_period` | merchant restock period and contents | second restock 2.50–3.01 days later; sold items replaced; simulation.md §3 |

## 3. Backlog (captures still to make)

Each item: what it settles, where to stand, and how. "Design" means a job still has to be
written.

- **B1 `x13_enable_parents`** (scenario exists, golden missing). Needs a save inside X-13
  in Old World Blues (cell `NVDLC03X13`). Read-only: disables/enables two root parents and
  reads GetDisabled along the chains. Generate with `capture_steps.py actions
  parity-lab/inputs/layer4/x13_enable_parents.actions.json --data … --start "Inside X-13"`.
- **B2 merchant box without the Respawns flag** (design). Does it restock at all? Easiest:
  Primm, Prospector's Den merchant — container 0x0017409C
  (`VendorContainerPrimmDenMerchantREF`, base `VLootSackVendorWastelandT2RSPros`, stored in
  cell `NVVendorChestsCell`), merchant 0x00173F9B in `SLProspectorsDenInt`. Same pattern as
  `goodsprings_merchant_restock` (empty, wait ≥ 72 h, read; repeat). The other six are
  listed by `fnvtool sim forms` under references.merchant_containers.without_respawns_flag.
- **B3 cell respawn on an ordinary container** (design). The 72 h cell-respawn rule
  (`iHoursToRespawnCell` = 72) is not yet observed on a non-merchant container. Candidate:
  the Goodsprings General Store fridge 0x00105824 (`VLootFridgeClosedTier1WastelandRS`,
  leveled contents, cell `GSGeneralStore`). Leveled contents are random, so observe the
  number of stacks (`GetNumItems`; needs a trace op or a distribution-style check) or use
  a respawning container with fixed contents. Leave the cell between empty and read.
- **B4 merchant restock: on access or on a schedule** (design). All captures fit both.
  Idea: after an observed restock, empty the box and read it only rarely vs every hour
  around the 72 h mark; under "on access" the restock time follows the reads.
- **B5 calendar: is GameMonth 9 October?** Set day 30 of month 10 (November if 0-based,
  30 days) at 20:00 (TimeScale 30!), wait 6 h, read GameDay/GameMonth. Day 1 of month 11
  means 0-based months with Gregorian lengths; day 31 means otherwise.
- **B6 fractional game time** (design). GameHour/GameDaysPassed after waits drift with
  menu time (×30); low TimeScale crashes when the clock moves. Possible: read the clock
  immediately before and after each Wait and compare differences, not absolutes.
- **B7 encumbrance edges**: over-encumbered threshold (weight = capacity vs > capacity),
  readable with `IsPlayerOverencumbered` / `GetIsPlayerOverencumbered` (both on the GECK
  wiki; check which compiles); ammo weight in Hardcore mode (`SetHardcore` / `IsHardcore`
  exist on the wiki; a throwaway save, since Hardcore cannot be undone in a save).
- **B8 leveled lists**: lists with an LVLG global, deep nesting, all-levels lists with
  entries between levels (new cases in a second `.cases.json`, same procedure as
  `leveled_basic`).
- **B9 Doc Mitchell z**: no capture needed; re-run the lab once workstream 2's character
  controller places the actor (the +7.44 offset is in the golden).
- **B10 placed explosives (PGRE)**: are the 297 OWB placed grenades live in game? Needs an
  OWB save near one (`fnvtool sim forms` lists them).
