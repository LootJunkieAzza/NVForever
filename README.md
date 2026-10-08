# NVForever

An open-source, clean-room reimplementation of the engine behaviour of **Fallout: New
Vegas**, in the spirit of OpenMW. It reads the data files of *your own legally purchased
copy*; it ships no Bethesda content and contains no Bethesda or Gamebryo code.

Status: **Layers 0–2 implemented and validated** against FNV 1.4.0.525 (base game + all
DLC). Layer 1: archives and plugins ([docs/design/layer1.md](docs/design/layer1.md)).
Layer 2: every mesh and animation block in the game decodes and re-encodes exactly, an
independent oracle agrees on scene graphs, geometry and all 26,907 textures, and a Bevy
viewer renders meshes with textures, skinning and animation
([docs/design/layer2.md](docs/design/layer2.md)).

| Layer | Scope | State |
|---|---|---|
| 0 | Ground truth, parity lab, capture tools | done (`parity-lab/`, `tools/capture/`) |
| 1 | BSA reader, ESM/ESP parser, load order, schemas | container + load order complete; layouts for 40/107 record types; xEdit cross-check pending |
| 2 | NIF, DDS, KF assets, viewer | 160/160 block types, 1,036,695 blocks exact; 56/56 parity scenarios; viewer (owner visual check pending) |
| 3 | World and rendering | in progress: world data checked on the full load order; free-camera explorer draws terrain and objects (no LOD, sky, water or SpeedTree yet) |
| 4 | Core simulation | started: headless `fnv-sim` (forms, references, inventories, leveled lists, activation, delta state, action traces); container contents match an independent oracle; collision next ([docs/design/layer4.md](docs/design/layer4.md)) |
| 5–7 | Scripting, gameplay, UI/audio/saves | not started |

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

## Getting `fnvtool` (Windows)

`fnvtool` is the project's command-line program (Rust, not Python). Either:

1. **Download** it: after a push, open the repository's *Actions* tab → latest `ci` run →
   artifact `fnvtool-windows-x64`; unzip it into e.g. `C:\Games\NVForever`.
2. **Build** it: install Rust from https://rustup.rs, then in the repository folder run
   `cargo build --release -p fnvtool`; the program is `target\release\fnvtool.exe`.

Example (PowerShell):

```powershell
$D = "F:\SteamLibrary\steamapps\common\Fallout New Vegas\Data"
.\fnvtool.exe bsa verify "$D\Fallout - Meshes.bsa"
```

The Python scripts are separate: `tools\capture\*.py` (reference readers that produce
private goldens) and `parity-lab` (the comparison lab).

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
  fnv-world     worldspaces, cells, references, landscape across a load order
  fnv-sim       Layer 4 core simulation: forms, references, inventories, activation (headless)
  fnvtool       CLI used by the parity lab (JSON on stdout)
engine/         Bevy workspace (its own): nvf-assets, nvf-viewer (asset viewer), nvforever (game)
schemas/fnv/    record layouts as data (fnv.schema/1)
schemas/nif/    NIF block layouts as data (fnv.nif.schema/1)
parity-lab/     oracle-vs-candidate differential testing (Python stdlib)
tools/capture/  oracles, xEdit export script, install inventory
docs/
  spec-notes/   clean-room format/behaviour notes (tagged documented/observed/hypothesis)
  design/       per-layer designs
  adr/          architecture decision records
  cli-contract.md
```

## Build and test

```bash
cargo test --workspace --release
cargo build --release -p fnvtool
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
cd engine && cargo build --release        # Bevy viewer and game (first build takes a while)
```

Dependencies and the reasons for each: [docs/design/dependencies.md](docs/design/dependencies.md)
and [docs/adr/](docs/adr/). Builds with a stock stable Rust toolchain (tested with 1.99).

## Try it on your install

```bash
D="/path/to/Fallout New Vegas/Data"
fnvtool esm stats "$D/FalloutNV.esm"
fnvtool esm verify "$D/FalloutNV.esm"          # byte-identical rewrite + checks
fnvtool bsa list "$D/Update.bsa"
fnvtool esm loadorder "$D" FalloutNV.esm DeadMoney.esm HonestHearts.esm OldWorldBlues.esm \
    LonesomeRoad.esm GunRunnersArsenal.esm --schema schemas/fnv
fnvtool esm decode "$D/FalloutNV.esm" --schema schemas/fnv --edid GSDocMitchellScript
fnvtool nif coverage "$D/Fallout - Meshes.bsa" --schema schemas/nif   # every block, exact?
fnvtool nif decode "$D/Fallout - Meshes.bsa" --path "meshes\clutter\junk\nv\nvnukacolaquartz.nif" --schema schemas/nif
fnvtool dds list "$D/Fallout - Textures.bsa"
```

The viewer (Windows PowerShell; reads your Data folder, never writes to it):

```powershell
$D = "C:\Program Files (x86)\Steam\steamapps\common\Fallout New Vegas\Data"
cd engine
cargo run --release --bin nvf-viewer -- --data $D --filter "clutter\junk\nv"
cargo run --release --bin nvf-viewer -- --data $D --mesh "meshes\creatures\deathclaw\deathclaw.nif" --anim "meshes\creatures\deathclaw\mtidle.kf"
```

Keys: N/P next/previous mesh, A next animation, Space pause, H show hidden objects,
F frame, F12 screenshot, Esc quit; mouse: left drag orbit, right drag pan, wheel zoom.

The world (Layer 3, in progress): terrain and objects streamed around a free camera.

```powershell
cargo run --release --bin nvforever -- --data $D --cell Goodsprings
```

Keys: W/A/S/D move, Q/E down/up, Shift faster, right mouse drag look, wheel speed,
`[`/`]` hour, F12 screenshot, Esc quit. `--world`, `--cell x,y`, `--pos x,y,z`, `--heading`,
`--radius`, `--hour`, `--weather`. An interior cell's Editor ID as `--cell` (e.g.
`GSDocMitchellHouse`) opens that interior with its own lighting.

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
