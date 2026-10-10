# NVForever

An open-source, clean-room reimplementation of the engine behaviour of **Fallout: New
Vegas**, in the spirit of OpenMW. It reads the data files of *your own legally purchased
copy*; it ships no Bethesda content and contains no Bethesda or Gamebryo code.

Status (2026-10-09, FNV 1.4.0.525, base game + all DLC): the archive and plugin readers,
the asset formats, the world renderer, the core simulation, physics and scripting run on
the whole game. `nvforever --play` lets you walk the Mojave with collision, open doors and
containers, manage the inventory, and see quest and object scripts run. Every layer is
checked by the parity lab against independent readers and against captures from the
original game. Of Layers 6–7, the player's side of combat is built and you can talk to
NPCs with their recorded voices; AI packages, the game's menus, the rest of the audio and
saves are not started.

| Layer | Scope | State |
|---|---|---|
| 0 | Ground truth, parity lab, capture tools | done: `parity-lab/` (402 scenarios; 376 pass on `main`, the rest are listed in the layer designs and [docs/owner-review.md](docs/owner-review.md)), independent Python readers, an xEdit export, and `nvf_capture`, an xNVSE plugin that runs unattended jobs in the original game (actions, traces, screenshots, AI observation) |
| 1 | BSA reader, ESM/ESP parser, load order, schemas | done: archives and plugins read and rewrite byte for byte; 70 of 107 record types have layouts, with nothing left undecoded at the end of a record in any of the 10 masters; the xEdit cross-check matches 465,016 records of FalloutNV.esm except one, and the 4 rows that differ across the masters are xEdit normalising the file (layer1.md) — 111/115 scenarios |
| 2 | NIF, DDS, KF assets, viewer | done: 160/160 block types, 1,036,695 blocks decode and re-encode exactly; an independent reader agrees on all 26,907 textures; archive precedence (`Update.bsa` and DLC archives over the base ones) confirmed in the game — 51/51 scenarios outside the holdouts |
| 3 | World and rendering | in progress: `nvforever` streams the world with terrain, objects, distant LOD (four levels) and distant tree billboards, SpeedTree trees, water, sky, weather and time of day, interiors with their own lighting, and NPCs and creatures with FaceGen and idle animation. Compared with the game's own screenshots: all 7 interior reference frames and 1 of 8 exterior ones meet the image threshold; exterior lighting (sunlight strength, shaded faces) and clouds are the known gaps ([docs/design/layer3.md](docs/design/layer3.md)) — 99/109 scenarios |
| 4 | Core simulation, collision, physics, play | done apart from Layer 6 systems: headless `fnv-sim` (inventories, containers, leveled lists, locks, doors, clock, respawn, placed mines), Havok collision and a rapier3d character controller that walks, jumps, sneaks and swims like the game's captures, and `--play` (walking, doors including other worldspaces, containers, inventory, dropping, pushing clutter), played by hand with keyboard and mouse. 87/90 scenarios; the 3 misses are accepted physics residuals ([docs/design/layer4.md](docs/design/layer4.md)) |
| 5 | Scripting | in progress: every compiled script decodes (`fnv-script`); interpreter, quests, conditions and events (`fnv-game`, 28/32 `L5` scenarios against in-game captures); scripts run in `nvforever --play`; the intro chain advances headless with stand-ins for Layer 6 ([docs/design/layer5.md](docs/design/layer5.md)) |
| 6–7 | Gameplay (dialogue, AI, combat), UI/audio/saves | started: the player's side of combat ([docs/design/combat.md](docs/design/combat.md)), dead actors drop as ragdolls built from their skeletons' Havok bodies and constraints ([docs/spec-notes/ragdoll.md](docs/spec-notes/ragdoll.md)); dialogue: topics and lines read in full, conversations with the original's selection rules (headless and in `--play` with a plain dialogue screen), voice lines found for 97.3 % of responses and played; rules still to confirm against in-game captures ([docs/design/dialogue.md](docs/design/dialogue.md)); AI packages, the game's menus, the rest of the audio and saves are not started |

Decisions taken by working sessions while the owner was away, and questions for the
owner, are listed in [docs/owner-review.md](docs/owner-review.md).

## Where things go (separate from the game)

NVForever never runs from, or writes into, your Fallout: New Vegas folder, and never
touches `Documents\My Games\FalloutNV`. The game's `Data` folder is read-only input.

| | Location |
|---|---|
| Original game (read-only) | e.g. `F:\SteamLibrary\steamapps\common\Fallout New Vegas` |
| NVForever programs | any folder outside the game, e.g. `C:\Games\NVForever` |
| NVForever home (config, saves, logs, cache, mods) | `%LOCALAPPDATA%\NVForever` or `NVFOREVER_HOME` |

```
fnvtool setup --game "F:\SteamLibrary\steamapps\common\Fallout New Vegas"
fnvtool paths
```

Details: [docs/design/install-layout.md](docs/design/install-layout.md).

## Build and test

Install Rust from https://rustup.rs (a stock stable toolchain; tested with 1.99) and
Python 3.11+.

```bash
cargo build --release -p fnvtool            # the command-line tool: target/release/fnvtool(.exe)
cargo test --workspace --release
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
cd engine && cargo build --release          # Bevy viewer and game (first build takes a while)
```

The checks every change runs before it is pushed are in [CLAUDE.md](CLAUDE.md).
Dependencies and the reasons for each: [docs/design/dependencies.md](docs/design/dependencies.md)
and [docs/adr/](docs/adr/).

## Play and explore

From `engine/`, with `$D` your game's `Data` folder (Windows PowerShell shown):

```powershell
$D = "F:\SteamLibrary\steamapps\common\Fallout New Vegas\Data"
cargo run --release --bin nvforever -- --data $D --cell Goodsprings --play
```

Keys in `--play`: W/A/S/D move, Shift walk, Caps Lock always run, Ctrl sneak, Q auto
move, Space jump, mouse look, E use (doors, containers, items), Tab inventory, Esc pause
menu, `[`/`]` hour, F12 screenshot. Going through a door to another place shows one of
the game's loading screens while the destination loads; the game holds still meanwhile
([docs/design/loading-screen.md](docs/design/loading-screen.md)).

Without `--play` the camera flies freely: W/A/S/D move, Q/E down/up, Shift faster, right
mouse drag look, wheel speed, `[`/`]` hour, F12 screenshot, Esc quit.

Useful options: `--world <WRLD>`, `--cell <EditorID or x,y>` (an interior's Editor ID such
as `GSDocMitchellHouse` opens that interior), `--pos x,y,z`, `--heading`, `--pitch`,
`--hour`, `--weather`, `--fov` (the game's setting, default 75), `--size WxH`,
`--no-actors`, `--actor-pose idle|bind`, `--screenshot <png>` (render once and exit),
`--time-scale <game hours per second>` with `--timelapse <dir>` (day/night sequences).
`nvforever --help` lists them all.

The asset viewer:

```powershell
cargo run --release --bin nvf-viewer -- --data $D --filter "clutter\junk\nv"
cargo run --release --bin nvf-viewer -- --data $D --mesh "meshes\creatures\deathclaw\deathclaw.nif" --anim "meshes\creatures\deathclaw\mtidle.kf"
```

Keys: N/P next/previous mesh, A next animation, Space pause, H show hidden objects,
F frame, F12 screenshot, Esc quit; mouse: left drag orbit, right drag pan, wheel zoom.

## The command-line tool

`fnvtool` prints JSON for the parity lab and for inspection:

```bash
D="/path/to/Fallout New Vegas/Data"
M="FalloutNV.esm DeadMoney.esm HonestHearts.esm OldWorldBlues.esm LonesomeRoad.esm GunRunnersArsenal.esm"
fnvtool esm stats "$D/FalloutNV.esm"
fnvtool esm verify "$D/FalloutNV.esm"          # byte-identical rewrite + checks
fnvtool esm decode "$D/FalloutNV.esm" --schema schemas/fnv --edid GSDocMitchellScript
fnvtool esm loadorder "$D" $M --schema schemas/fnv
fnvtool bsa list "$D/Update.bsa"
fnvtool nif coverage "$D/Fallout - Meshes.bsa" --schema schemas/nif   # every block, exact?
fnvtool world list --schema schemas/fnv "$D" $M
fnvtool sim doors --schema schemas/fnv "$D" $M  # every teleport door and where it leads
fnvtool script decompile --schema schemas/fnv "$D" $M --form 0x00107230   # GSDocMitchellScript
```

Every command and its JSON output: [docs/cli-contract.md](docs/cli-contract.md).

## Checking against the original game

- **Independent readers** (`tools/capture/oracle_*.py`): clean-room Python readers for
  archives, plugins, meshes, textures, world data, collision, terrain heights, simulation,
  actors and scripts, written from the spec notes only. Their output is the golden the
  Rust side must match.
- **In-game captures** (`tools/capture/nvf_capture`, an xNVSE plugin, and
  `capture_run.py`): jobs that run unattended in the original game from a save and record
  what it does — stages, inventories, positions over time, clock and timers — as goldens.
- **Reference frames** (`capture_frames.py`): the game's own screenshots at recorded
  cameras, compared with `nvforever` renders of the same camera by the lab's image
  comparator.
- **AI observer** (`observe.py`): timelines of what NPCs do over game hours, for Layer 6.

Private outputs (goldens, screenshots, dumps) stay in a folder outside the repository.
How the lab works: [parity-lab/README.md](parity-lab/README.md); the capture tools:
[tools/capture/README.md](tools/capture/README.md).

## Layout

```
crates/
  fnv-install   separation of NVForever's home from the original game
  fnv-util      Windows-1252 (encoding_rs), SHA-256 (sha2), byte cursor
  fnv-inflate   zlib decoding with trailer diagnostics (flate2) + stored-block encoder
  fnv-bsa       BSA v104 archives
  fnv-esm       plugins: container, load order, schema-driven decoding, coverage
  fnv-nif       NIF/KF: container, schema interpreter, scene graph, geometry, animation
  fnv-dds       DDS: header, formats, mip layout
  fnv-vfs       read-only virtual file system (archives in precedence order + loose files)
  fnv-world     worldspaces, cells, references, landscape, weather and lighting records
  fnv-spt       SpeedTree .spt parameter files and the tree generator
  fnv-actor     actor appearance: skeleton, body, head, hair, FaceGen, worn armour
  fnv-collide   Havok collision blocks as engine-neutral shapes
  fnv-phys      physics: rapier3d world, character controller, swimming, queries
  fnv-sim       core simulation: forms, references, inventories, activation, clock (headless)
  fnv-script    compiled scripts: bytecode to IR, function table, decompiler, interpreter
  fnv-game      game rules: script instances, quests, conditions, events, functions, runner
  fnvtool       CLI used by the parity lab (JSON on stdout)
engine/         Bevy workspace (its own):
  nvf-assets    archives, NIF meshes, DDS textures, KF playback for Bevy
  nvf-viewer    asset viewer
  nvforever     the game: world streaming, terrain, LOD, sky, water, trees, actors
  nvf-play      --play: player, physics, activation, menus, scripts in the game
schemas/fnv/    record layouts as data (fnv.schema/1)
schemas/nif/    NIF block layouts as data (fnv.nif.schema/1)
schemas/script/ script function catalogue as data
parity-lab/     oracle-vs-candidate differential testing (Python stdlib)
tools/capture/  independent readers, in-game capture plugin and jobs, xEdit export
docs/
  spec-notes/   clean-room format/behaviour notes (tagged documented/observed/hypothesis)
  design/       per-layer designs and status
  adr/          architecture decision records
  cli-contract.md
  owner-review.md  decisions and questions for the owner
```

## Rules of the project

- Clean room: behaviour and formats are described in plain language in `docs/spec-notes/`
  and implemented from those notes. No leaked source, no DRM circumvention (the Steam
  executable's code section is encrypted and is not unpacked).
- Every factual claim is tagged `[documented]`, `[observed]` or `[hypothesis]`.
- Goldens and any game-derived dumps stay private (`.gitignore` blocks game files).

## Licence

GPL-3.0-or-later (placeholder pending the project owner's decision; see `LICENSE`).
Record layouts in `schemas/` were derived from the MPL-2.0 xEdit definitions as
documentation; see `schemas/README.md`.
