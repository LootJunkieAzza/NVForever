# Stress tests

The smoke tests (`tools/smoke`) check one behaviour each in a short run. The stress tests
(`tools/stress`) push the engine the way long play does: every cell, long trips, big
fights. They look for crashes, hangs, memory that is never given back, and slow frames.
They are not parity tests. Nothing in them is compared with the original game, except
the combat measures that docs/spec-notes/npc-behaviour-compare.md compares.

Run them on a machine with the game installed (read-only, as every run). They take the
game locks as smoke runs do, so a stress run and a smoke run wait for each other.

```
python tools/stress list
python tools/stress run cells  --exe <engine target>/release/nvforever.exe --data <Data> --fnvtool <root target>/release/fnvtool.exe [options]
python tools/stress run tour   ... [--laps N]
python tools/stress run battle ... [--per-side 10|25|50] [--third N] [--seconds S]
```

`NVF_EXE`, `NVF_DATA` and `NVF_FNVTOOL` stand in for the three paths, as for smoke.
`--no-locks` skips the locks on a machine of your own. `--name` names the run's folder.

## 1. How a run is made (tools/stress/stress/lib.py)

- **One launch.** `lib.run` starts nvforever with `--play --no-messages` and these test
  aids: `NVF_PLAY_TEST_GOD`, `NVF_PLAY_TEST_UNSEEN`, `NVF_PLAY_TEST_SCRIPT_MENUS=0.05`
  and `NVF_PLAY_TEST_STRESS=1` (§2). The test adds its own aids on top.
- **Its own home.** Each launch gets its own NVForever home, `<run>/<launch>.home`
  (`NVFOREVER_HOME`). Its run log and any crash reports
  (`logs/crash-*.txt`, engine/nvforever/src/crash.rs) stay with it.
- **Watched for progress.** Each test says which log lines count as progress. When no
  such line comes for `--hang` seconds, the run counts as hung: every thread's stack goes
  to `<launch>.stacks.txt` (tools/smoke `stacks.py`), and the process tree is killed.
- **Memory.** On Windows, the runner reads the process's working set and private bytes
  every few seconds (`<launch>.memory.csv`).
- **Status.** A launch ends `ok`, `crashed` (a crash report was written), `hung`, or
  `failed` (a non-zero exit with no report: a native crash, which no Rust hook sees).
- **Shared code.** The locks (`Locks`, `default_lock_paths`), the aid-free environment
  and the stack dump are imported from tools/smoke, not copied.
- **Crash grouping.** Crashes are grouped by cause (`lib.group_crashes`). The key is the
  panic message with its numbers blanked, the source file it was raised in, and the top
  five backtrace frames that are the engine's own (std, core and the crash hook left
  out). Each group lists the cells or runs that hit it.

Every run writes `target/stress/<test>/<run>/results.json` (everything) and `report.md`
(the summary to read first). The launch logs and command lines sit beside them.

## 2. The engine's aids (engine/nvf-play/src/stress.rs, engine/nvforever/src/stress_sample.rs)

- **`NVF_PLAY_TEST_STRESS=1`.** Nothing holds an unattended run up, and scripts still run.
  - A conversation that starts (a force-greet, StartConversation, a package's dialogue)
    is ended at once.
  - A script's move of the player (MoveTo player, a trigger's travel) is left out.
  - Both are logged; the conversations ended are counted in `stress summary:`.
- **`NVF_PLAY_TEST_CELLS=<list>[;<dwell>]`.** Visits the cells of a list file in turn:
  `interior <cell>` and `exterior <world> <x> <y> <cell>`, FormIDs in hex.
  - Each visit is a travel behind the loading screen, as a door or `coc` makes.
  - An exterior's ground is found once the cell is in.
  - Each visit logs `stress cell: i/n start …` before it goes, and `stress cell: i/n
    loaded in <s> s …` when the loading screen comes down.
  - An interior with no door or marker is entered by the placed reference nearest the
    middle of its references (its origin when it has none), so it is load-tested too.
  - A grid cell that is not the listed one (a list made with another load order) logs
    `skipped:` with the reason.
- **`NVF_PLAY_TEST_TOUR=<route>`.** Steps the player along a closed route. The route file
  has `world`, `step`, `dwell` and `laps` lines, and `<x> <y>` waypoints.
  - A step within the loaded ground puts the player on it at once; a step beyond it goes
    by a travel.
  - Logs each lap's start and end, and each waypoint reached.
- **`NVF_PLAY_TEST_BATTLE=<at>;<base>:<count>:<x>:<y>;…`.** At `<at>` seconds, puts each
  group's fighters in a block on the ground round (x, y), facing the other groups
  (`Game::create_at`, made as PlaceAtMe makes references).
  - Each fighter is set on the nearest fighter of another group (StartCombat).
  - The placing line ends with each group's FormIDs (`0xFF000000..0xFF000009`): created
    actors have no Editor ID, and the runner tells the sides' deaths apart by them.
  - Created actors become residents and fight as placed ones do.
- **`NVF_PLAY_TEST_SAMPLE=<seconds>`.** Every that many seconds, logs one `stress sample:`
  line with what the engine holds and where the camera is:
  - frame time over the window: mean and worst;
  - entities, drawn references, meshes, images and materials;
  - audio sinks, physics cells, physics bodies (fixed/kinematic/dynamic);
  - actors, actors with a non-finite position, ragdolls, projectiles in flight.

  At the exit, `stress frames:` gives the whole run's frame-time mean, 1 % low and worst.

## 3. cells

Every cell of the ten masters (tools/smoke's `masters` list, the engine's load order) is
visited, so a crash anywhere in loading shows up.

- **Where the cells come from.** fnvtool's `world list`, then `world cells` for each
  worldspace and for the interiors. Persistent cells (no grid) are left out.
- **Batches.** `--batch` cells per launch, 100 by default. A crash or a hang ends its
  batch. The next batch starts after the cell it was on, so one bad cell costs one
  launch, not the sweep.
- **Choosing cells.** `--kinds interior|exterior`, `--worlds`, `--stride N` (exteriors
  on every N-th grid line both ways) and `--limit` pick a sample.
- **Long sweeps.** All 44,517 cells take about 30 hours at 50 a launch; batches of 200
  are fine. After every batch the runner saves `state.json` and rewrites
  `results.json` and `report.md`, so a sweep can be read while it runs.
  `--resume <run>` goes on with a stopped sweep in the same folder: cells it already has
  a row for are skipped, and its rows and crash groups stay in the results.
- **Whose crash.** A crash or hang counts against the cell the launch last went to,
  whether it came while that cell loaded or after.
- **Results.** Totals (OK, crashed, hung, skipped, not reached), the mean and slowest
  loads, the crash groups with their cells, and each bad cell's last log lines.

## 4. tour

Long play's memory: what the engine holds, sampled along laps of the Mojave.

- **The route.** By default `--waypoints` points on an ellipse `--inset` of the way in
  from the worldspace's grid edges (fnvtool `world list`). `--route` takes a file of
  your own.
- **The pace.** The player moves `--step` units every `--dwell` seconds. The defaults
  (1024 units a second) stream cells about as fast as running would load them.
- **The samples.** Each sample is tagged with its lap and the waypoint it is on. The
  process's memory is added from the runner's readings. All of them go to `samples.csv`.
- **Possible leaks.** A count that ends each lap higher than the last ("rising every
  lap"), or that is higher at the same waypoint on a later lap than on the first ("did
  not fall back on coming back"). Each is listed with where it grew (the sample's
  place). The tolerance is 5 %.

  A count that settles after the first lap is caching. One that keeps rising is the one
  to look into.

This does not walk on the navmesh as an actor would: it steps over the ground. That
streams the same cells, but finds no path or collision trouble. A walking tour, with the
residents' walker driving the player, is left for later.

## 5. battle

NCR troopers against Legion soldiers in Goodsprings, with an unseen, god-mode player
watching from the south.

- **The fighters.** The leveled lists the AI observer's npc_battles campaign spawned in
  the original game: `LvlNCRTrooper`, `LvlLegionRaiderMelee`, and `VCrTier1CoyoteMed`
  for `--third` [observed: tools/capture/observe/npc_battles.campaign.json].
- **The layout.** The sides stand `--gap` units apart, west and east of `--centre`;
  `--per-side` sets their size (10, 25, 50).
- **Frames and memory.** Frame-time mean, 1 % low and worst; the peaks of memory,
  projectiles, ragdolls and actors; actors with a non-finite position.
- **Per system.** The engine's own per-frame timings, from the summaries that report one
  (actor animation, scripts, foot IK, head tracking …).
- **Combat**, for AI tuning:
  - the sides' standing, dead, hits, kills and flights (`battle summary:`);
  - time to kill, counted from the placing;
  - guns: shots against projectile hits on actors, and reloads; melee: blows and impacts
    (the sides' hits in `battle summary:` count both);
  - deaths by group, with the median and last time to kill;
  - flee checks and flights, cover taken, target switches;
  - combat lines a minute by kind;
  - melee spacing (swing distances), and stuck actors.

## 6. Reading results

Start with `report.md`.

- **cells.** Each crash group is one bug: its message, where it was raised, and the cells
  that hit it. The launch logs (`batch-NNN.log`) and homes (`batch-NNN.home/logs`) hold
  the full reports.
- **tour.** "Possible leaks" names the counts to look at; `samples.csv` plots them over
  time.
- **battle.** Numbers to compare between builds and with npc-behaviour-compare.md's rows.

A hung launch's `.stacks.txt` shows where every thread stood.

## 7. Adding a test

1. Write a module in `tools/stress/stress/` with `run(ctx, …) -> dict` and
   `report(results) -> list[str]`.
   - Launch with `lib.run(ctx, lib.Launch(...))`, giving the aids, the run's length and
     the progress pattern.
   - Read what you need from `Result.lines`. Give `lib.group_crashes` the crashes.
2. Add it to `TESTS` and to the parser and dispatch in `cli.py`.
3. When the engine has to do something new, add a test aid in nvf-play `stress.rs` (or a
   counter in nvforever `stress_sample.rs`). Keep its parsing a pure function with a unit
   test, and log one line per step for the runner to read.
4. Add unit tests in `tools/stress/tests/` that run without the game: feed the module
   made-up log lines, and use a fake game (a script behind a `.cmd`) for the launcher.

The runner's own tests are a push check (`stress runner tests` in
tools/smoke/scenarios.toml): `python -m unittest discover -s tools/stress/tests`.
