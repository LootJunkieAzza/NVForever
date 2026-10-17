# NVForever

An open-source, clean-room reimplementation of the engine behaviour of **Fallout: New
Vegas**, in the spirit of OpenMW. It reads the data files of *your own legally purchased
copy*; it ships no Bethesda content and contains no Bethesda or Gamebryo code.
NVForever is an independent project, not affiliated with or endorsed by Bethesda
Softworks, ZeniMax Media, Microsoft or Obsidian Entertainment; *Fallout* and *Fallout:
New Vegas* are their trademarks. See [LEGAL.md](LEGAL.md).

Status (2026-10-17, FNV 1.4.0.525, base game + all DLC): the archive and plugin readers,
the asset formats, the world renderer, physics, the simulation core and the script runtime
run on the whole game. `nvforever --play --menu` opens on the original's main menu
(Continue, New Game, Load Game, Settings, Quit); New Game plays the intro film and the
opening at Doc Mitchell's, with NVForever's own character creator for the face. From there
it is a playable slice of the game: walk the Mojave with collision and fast travel from the
Pip-Boy's map; talk to NPCs in the original's dialogue menu with their recorded voices;
NPCs live by their AI packages, talk among themselves, detect you, fight, flee and hide,
take cover and limp when crippled; shoot, strike and throw with the player's side of
combat; companions such as Boone follow you through doors and fight with you; gain
experience, level up and pick perks; pick locks, hack terminals, use aid items, craft at
workbenches and campfires, play blackjack and roulette, wait and sleep; crime, karma and
reputation are tracked; the HUD, Pip-Boy, inventory, barter, container, pause, settings
and main menus are drawn from the game's own menu files; music, sounds and radio play;
save and load games (F5/F9, the pause menu, autosaves). Not built yet: VATS, knockdowns,
the repair menu, Caravan and the slot machines, the companion wheel.

**What is complete, partly built and not started, per system: [docs/status.md](docs/status.md).
Everything still to do, ranked by what blocks a full playthrough:
[docs/outstanding.md](docs/outstanding.md).**

| Layer | Scope | State |
|---|---|---|
| 0 | Ground truth, parity lab, capture tools, smoke runner | done: `parity-lab/` (scenarios per layer against independent readers and captures from the original game), `nvf_capture` (an xNVSE plugin that runs unattended jobs in the original game), `tools/smoke` (game-run scenarios) |
| 1 | BSA reader, ESM/ESP parser, load order, schemas | done ([docs/design/layer1.md](docs/design/layer1.md)) |
| 2 | NIF, DDS, KF assets, viewer | done ([docs/design/layer2.md](docs/design/layer2.md)) |
| 3 | World and rendering | done, with known gaps: exterior lighting and clouds against the game's frames, effect shaders, depth of field; an optional enhanced look (`--graphics enhanced`) ([docs/design/layer3.md](docs/design/layer3.md)) |
| 4 | Core simulation, collision, physics, play | done; 3 accepted physics residuals ([docs/design/layer4.md](docs/design/layer4.md)) |
| 5 | Scripting | done; every function the shipped scripts use probed against the original, about 55 still stubs that need systems not built yet ([docs/design/layer5.md](docs/design/layer5.md), [docs/design/script-function-coverage.md](docs/design/script-function-coverage.md)) |
| 6 | Gameplay: AI, dialogue, combat, followers, crime, aid, progression | mostly done; levelling up and perks, blackjack and roulette built; crafting first version; VATS, knockdowns, repair and Caravan not started |
| 7 | Interface, audio, saves | mostly done; saved games and the main menu first versions; the companion wheel, credits, repair, barber and surgeon, Caravan and slot menus not started |

Decisions taken by working sessions while the owner was away, and questions for the
owner, are listed in [docs/owner-review.md](docs/owner-review.md).

## Where things go (separate from the game)

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

Details: [docs/design/install-layout.md](docs/design/install-layout.md).

## Download

Pre-release builds for Windows and Linux are on the
[releases page](https://github.com/LootJunkieAzza/NVForever/releases) (v0.1.0, 2026-10-13,
built before saved games, crafting, the main menu, levelling up and the casino games). Unpack outside the game folder and
start through the launchers beside the programs, which point them at the shipped schemas:
`nvforever.cmd` (Windows) or `./nvforever.sh` (Linux); `README-release.md` in the archive
explains the rest. v0.1.0 needs `NVFOREVER_DATA` set to your game's `Data` folder and has
no `--menu`: start it with `--play` or `--play --new-game`. From the next release on, the
first start asks for the game folder and remembers it, and `--play --menu` opens the main menu.

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

The checks every change runs before it is pushed are in [CLAUDE.md](CLAUDE.md).
Dependencies and the reasons for each: [docs/design/dependencies.md](docs/design/dependencies.md)
and [docs/adr/](docs/adr/).

## Play and explore

From `engine/`, with `$D` your game's `Data` folder (Windows PowerShell shown):

```powershell
$D = "F:\SteamLibrary\steamapps\common\Fallout New Vegas\Data"
cargo run --release --bin nvforever -- --data $D --play --menu                 # the main menu
cargo run --release --bin nvforever -- --data $D --play --new-game             # straight into the opening at Doc Mitchell's
cargo run --release --bin nvforever -- --data $D --play --load "Save 3"        # a saved game
cargo run --release --bin nvforever -- --data $D --cell Goodsprings --play     # walk in Goodsprings
cargo run --release --bin nvforever -- --data $D --cell GSDocMitchellHouse --play --no-messages --graphics enhanced
```

`--data` (or `$env:NVFOREVER_DATA`) is for development runs; without it the game folder
comes from `NVForever.ini`, and the first start asks for it in a folder dialog
([docs/design/install-layout.md](docs/design/install-layout.md)). For playtests, `tools/play/Play.ps1` starts
the game at named places ([tools/play/README.md](tools/play/README.md)).

- `--play --menu`: the main menu over the plugins' loading screens, with the original's
  logo and theme. Continue loads the newest save; New Game plays `Video\FNVIntro.bik` (any
  key or click skips it) and then the opening; Load Game and Settings open the pause
  menu's pages.
- `--play`: you are the player (without it the camera flies freely); NPCs live by their AI
  packages unless `--no-npc-packages` is given.
- `--cell <EditorID or x,y>`: where to start (an interior's Editor ID such as
  `GSDocMitchellHouse` opens that interior); `--pos x,y,z`, `--heading` and `--hour`
  refine it. `--new-game` (with `--play`) starts the opening instead, `--load <save>` a
  saved game (saves live in `Documents\My Games\NVForever\saves`).
- `--no-messages` (with `--play`): message boxes with only an OK button (such as the
  add-on packs' start-up boxes) close at once; their text is still logged.
- `--graphics enhanced`: the optional modern look (ambient occlusion, finer shadows,
  bloom, sharpening, anisotropic filtering, sun shafts, water reflections;
  [docs/design/graphics-enhanced.md](docs/design/graphics-enhanced.md)). The default,
  `vanilla`, is the original's look and the one every test compares. Each improvement is
  also on the settings page of the Esc menu. Sun shadows are off by default in both looks;
  `--sun-shadows` (or the settings page) turns them on.
- `--no-voice`, `--no-sound`, `--no-music` silence parts of the audio.

Keys in `--play` (the game's defaults): W/A/S/D move, Shift walk/run, Caps Lock always run,
Ctrl sneak, Q auto move, Space jump, mouse look, left mouse attack, right mouse aim, R
reload, E use (doors, containers, items, people), F first/third person, Tab Pip-Boy (hold:
flashlight), T wait, F5 quicksave, F9 quickload, Esc pause menu (Continue, Save, Load,
Settings, Quit), `` ` `` developer console, `[`/`]` hour, F12 screenshot. Going through a door to
another place shows one of the game's loading screens while the destination loads
([docs/design/loading-screen.md](docs/design/loading-screen.md)). The bindings:
[docs/spec-notes/controls.md](docs/spec-notes/controls.md).

A crash leaves a report with a backtrace in `Documents\My Games\NVForever\logs`. Report bugs,
crashes and differences from the original with the
[issue forms](https://github.com/LootJunkieAzza/NVForever/issues/new/choose); never
attach game files.

Without `--play` the camera flies freely: W/A/S/D move, Q/E down/up, Shift faster, right
mouse drag look, wheel speed, `[`/`]` hour, F12 screenshot, Esc quit.

More options: `--world <WRLD>`, `--pitch`, `--weather`, `--fov` (the game's setting,
default 75), `--size WxH`, `--msaa 1|2|4|8`, `--no-actors`, `--actor-pose idle|bind`,
`--screenshot <png>` (render once and exit), `--time-scale <game hours per second>` with
`--timelapse <dir>` (day/night sequences), `--run-seconds <s>` (exit after that long, for
tests), `--plugins <files…>` (the load order, mods included; their archives follow),
`--actors-pass-through` (actors no longer kept apart from each other). `nvforever --help` lists them all.

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
  cli-contract.md
  owner-review.md  decisions and questions for the owner
```

## Rules of the project

- Clean room: behaviour and formats are described in plain language in `docs/spec-notes/`
  and implemented from those notes. No leaked source, no DRM circumvention (the Steam
  executable's code section is encrypted and is not unpacked).
- Every factual claim is tagged `[documented]`, `[observed]` or `[hypothesis]`.
- Goldens and any game-derived dumps stay private (`.gitignore` blocks game files).

The full statement of sources, process and trademarks is in [LEGAL.md](LEGAL.md).

## Support the project

NVForever is free and stays free. If you would like to support the work, you can do so on
Ko-fi: [ko-fi.com/lootjunkieazza](https://ko-fi.com/lootjunkieazza). Donations are
voluntary and pay for nothing in return: no game content, no early access, no extra
features. You still need your own copy of *Fallout: New Vegas*.

## Licence

GPL-3.0-or-later; see [LICENSE](LICENSE) and [LEGAL.md](LEGAL.md#licence).
Record layouts in `schemas/` were derived from the MPL-2.0 xEdit definitions as
documentation; see `schemas/README.md`.
