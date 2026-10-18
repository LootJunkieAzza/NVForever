# Development

Building, running, testing and the layout of the repository. The short project overview is
the [README](../README.md).

## Where things go

NVForever never runs from, or writes into, your Fallout: New Vegas folder, and never
touches `Documents\My Games\FalloutNV`. The game's `Data` folder is read-only input.

| | Location |
|---|---|
| Original game (read-only) | e.g. `F:\SteamLibrary\steamapps\common\Fallout New Vegas` |
| NVForever programs | any folder outside the game, e.g. `C:\Games\NVForever` |
| NVForever home (NVForever.ini, saves, logs, mods) | `Documents\My Games\NVForever` or `NVFOREVER_HOME` (caches: `%LOCALAPPDATA%\NVForever\cache`) |

```
fnvtool setup --game "F:\SteamLibrary\steamapps\common\Fallout New Vegas"
fnvtool paths
```

Details: [design/install-layout.md](design/install-layout.md).

## Build and test

Install Rust from https://rustup.rs (a stock stable toolchain; tested with 1.99) and
Python 3.11+.

```bash
cargo build --release -p fnvtool            # the command-line tool: target/release/fnvtool(.exe)
cargo test --workspace --release
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
python -m unittest discover -s tools/smoke/tests
cd engine && cargo build --release          # Bevy viewer and game (first build takes a while)
```

With the game installed, two more checks run against your own data:

```bash
# Smoke tests: short game runs described as data in tools/smoke/scenarios.toml; picks the
# scenarios your branch's changed files call for and prints one line per scenario.
python tools/smoke plan
python tools/smoke run --exe engine/target/release/nvforever.exe --data "$D" --fnvtool target/release/fnvtool.exe
# The parity lab: every layer against independent readers and in-game captures.
cp parity-lab/lab.local.example.json parity-lab/lab.local.json   # then set your paths
cd parity-lab && python -m fnvlab run                             # summary in runs/latest.md
```

The checks every change runs before it is pushed are in [CLAUDE.md](../CLAUDE.md).
Dependencies and the reasons for each: [design/dependencies.md](design/dependencies.md)
and [adr/](adr/).

## Running

From `engine/`, with `$D` your game's `Data` folder (Windows PowerShell shown):

```powershell
$D = "F:\SteamLibrary\steamapps\common\Fallout New Vegas\Data"
cargo run --release --bin nvforever -- --data $D                               # the main menu (as --play --menu)
cargo run --release --bin nvforever -- --data $D --explore                     # the free-camera explorer
cargo run --release --bin nvforever -- --data $D --play --new-game             # straight into the opening at Doc Mitchell's
cargo run --release --bin nvforever -- --data $D --play --load "Save 3"        # a saved game
cargo run --release --bin nvforever -- --data $D --cell Goodsprings --play     # walk in Goodsprings
cargo run --release --bin nvforever -- --data $D --cell GSDocMitchellHouse --play --no-messages --graphics enhanced
```

`--data` (or `$env:NVFOREVER_DATA`) is for development runs; without it the game folder
comes from `NVForever.ini`, and the first start asks for it in a folder dialog. For
playtests, `tools/play/Play.ps1` starts the game at named places
([tools/play/README.md](../tools/play/README.md)).

- No argument that chooses a world or a mode, or `--play --menu`: the main menu. Continue loads the newest save; New Game plays
  `Video\FNVIntro.bik` and then the opening; Load Game and Settings open the pause menu's
  pages.
- `--play`: you are the player (without it, with `--cell`, `--pos` or `--explore`, the
  camera flies freely: the explorer); NPCs live by their AI
  packages unless `--no-npc-packages` is given.
- `--cell <EditorID or x,y>`: where to start; `--pos x,y,z`, `--heading` and `--hour`
  refine it. `--new-game` (with `--play`) starts the opening instead, `--load <save>` a
  saved game (saves live in `Documents\My Games\NVForever\saves`).
- `--no-messages` (with `--play`): message boxes with only an OK button close at once;
  their text is still logged.
- `--graphics enhanced`: the optional modern look
  ([design/graphics-enhanced.md](design/graphics-enhanced.md)). The default, `vanilla`, is
  the original's look and the one every test compares. `--sun-shadows` turns on sun
  shadows (off by default in both looks).
- `--no-voice`, `--no-sound`, `--no-music` silence parts of the audio.

More options: `--world <WRLD>`, `--pitch`, `--weather`, `--fov`, `--size WxH`,
`--msaa 1|2|4|8`, `--no-actors`, `--actor-pose idle|bind`, `--screenshot <png>`,
`--time-scale` with `--timelapse <dir>`, `--run-seconds <s>`, `--plugins <files…>`,
`--actors-pass-through`. `nvforever --help` lists them all.

Keys in `--play` are the game's defaults (W/A/S/D, E use, Tab Pip-Boy, T wait, F5/F9
quicksave/quickload, Esc pause, `` ` `` console, F12 screenshot); the full list is in
[spec-notes/controls.md](spec-notes/controls.md). In the explorer:
W/A/S/D move, Q/E down/up, Shift faster, right mouse drag look, wheel speed, `[`/`]` hour.

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

Every command and its JSON output: [cli-contract.md](cli-contract.md).

## Checking against the original game

- **Independent readers** (`tools/capture/oracle_*.py`): clean-room Python readers written
  from the spec notes only. Their output is the golden the Rust side must match.
- **In-game captures** (`tools/capture/nvf_capture`, an xNVSE plugin, and
  `capture_run.py`): jobs that run unattended in the original game and record what it
  does as goldens.
- **Reference frames** (`capture_frames.py`): the game's own screenshots at recorded
  cameras, compared with `nvforever` renders of the same camera.
- **AI observer** (`observe.py`): timelines of what NPCs do over game hours.

Private outputs (goldens, screenshots, dumps) stay in a folder outside the repository.
How the lab works: [parity-lab/README.md](../parity-lab/README.md); the capture tools:
[tools/capture/README.md](../tools/capture/README.md).

## Layers

| Layer | Scope | Design |
|---|---|---|
| 0 | Ground truth, parity lab, capture tools, smoke runner | `parity-lab/`, `tools/capture/`, `tools/smoke/` |
| 1 | BSA reader, ESM/ESP parser, load order, schemas | [design/layer1.md](design/layer1.md) |
| 2 | NIF, DDS, KF assets, viewer | [design/layer2.md](design/layer2.md) |
| 3 | World and rendering | [design/layer3.md](design/layer3.md) |
| 4 | Core simulation, collision, physics, play | [design/layer4.md](design/layer4.md) |
| 5 | Scripting | [design/layer5.md](design/layer5.md), [design/script-function-coverage.md](design/script-function-coverage.md) |
| 6 | Gameplay: AI, dialogue, combat, followers, crime, aid, progression | per-system designs in [design/](design/) |
| 7 | Interface, audio, saves | per-system designs in [design/](design/) |

What is done per layer and system: [status.md](status.md). Decisions and questions for the
owner: [owner-review.md](owner-review.md).

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
  fnv-game      game rules: script instances, quests, conditions, events, functions, runner,
                dialogue, detection, combat rules, crime and karma, aid, lockpicking,
                hacking, followers
  fnv-ai        AI packages carried out: destinations, navmesh travel, sandboxing, alert states
  fnv-nav       navmeshes: nav graph of a load order, paths, path following
  fnv-combat    weapons, ammo, projectiles, explosions, body parts, damage rules
  fnv-voice     dialogue voice files: where a line's sound lives and how long it is
  fnv-font      the interface's bitmap fonts (.fnt)
  fnvtool       CLI used by the parity lab (JSON on stdout)
engine/         Bevy workspace (its own):
  nvf-assets    archives, NIF meshes, DDS textures, KF playback for Bevy
  nvf-viewer    asset viewer
  nvforever     the game: world streaming, terrain, LOD, sky, water, trees, actors, AI,
                combat, enhanced graphics, crash reports
  nvf-play      --play: player, physics, activation, menus, HUD, dialogue, audio, scripts
schemas/fnv/    record layouts as data (fnv.schema/1)
schemas/nif/    NIF block layouts as data (fnv.nif.schema/1)
schemas/script/ script function catalogue as data
parity-lab/     oracle-vs-candidate differential testing (Python stdlib)
tools/capture/  independent readers, in-game capture plugin and jobs, xEdit export
tools/smoke/    smoke runner: game-run scenarios as data (scenarios.toml)
docs/
  spec-notes/   clean-room format/behaviour notes (tagged documented/observed/hypothesis)
  design/       per-layer and per-system designs
  status.md     what is complete, partly built and not started, per system
  outstanding.md  everything still to do, ranked for a full playthrough
  handoff/      hand-over notes between coordinating sessions
  adr/          architecture decision records
  development.md  this file
```

## Rules of the project

- Clean room: behaviour and formats are described in plain language in `docs/spec-notes/`
  and implemented from those notes. No leaked source, no DRM circumvention (the Steam
  executable's code section is encrypted and is not unpacked).
- Every factual claim is tagged `[documented]`, `[observed]` or `[hypothesis]`.
- Goldens and any game-derived dumps stay private (`.gitignore` blocks game files).

The full statement of sources, process and trademarks is in [LEGAL.md](../LEGAL.md).
Record layouts in `schemas/` were derived from the MPL-2.0 xEdit definitions as
documentation; see `schemas/README.md`.
