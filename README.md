# NVForever

An open-source, clean-room reimplementation of the engine behaviour of **Fallout: New
Vegas**, in the spirit of OpenMW. It reads the data files of *your own legally purchased
copy*; it ships no Bethesda content and contains no Bethesda or Gamebryo code.

Status: **Layer 0 (tooling) done; Layer 1 (archives and plugins) implemented and
validated** against FNV 1.4.0.525 (base game + all DLC), with two items open: the FNVEdit
cross-check (run on your PC) and record-specific layouts for the 67 record types that so
far decode only their shared subrecords. See [docs/design/layer1.md](docs/design/layer1.md).

| Layer | Scope | State |
|---|---|---|
| 0 | Ground truth, parity lab, capture tools | done (`parity-lab/`, `tools/capture/`) |
| 1 | BSA reader, ESM/ESP parser, load order, schemas | container + load order complete; layouts for 40/107 record types; xEdit cross-check pending |
| 2 | NIF, DDS, KF assets | next |
| 3–7 | World, simulation, scripting, gameplay, UI/audio/saves | not started |

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
  fnv-util      Windows-1252, SHA-256, byte cursor
  fnv-inflate   zlib/DEFLATE decoder (+ stored-block encoder)
  fnv-json      order-preserving JSON
  fnv-bsa       BSA v104 archives
  fnv-esm       plugins: container, load order, schema-driven decoding, coverage
  fnvtool       CLI used by the parity lab (JSON on stdout)
schemas/fnv/    record layouts as data (fnv.schema/1)
parity-lab/     oracle-vs-candidate differential testing (Python stdlib)
tools/capture/  oracles, xEdit export script, install inventory
docs/
  spec-notes/   clean-room format/behaviour notes (tagged documented/observed/hypothesis)
  design/       per-layer designs
  cli-contract.md
```

## Build and test

```bash
cargo test --workspace --release
cargo build --release -p fnvtool
python -m unittest discover -s parity-lab/tests
python -m unittest discover -s tools/capture/tests
```

No external crates are used *yet*: the build sandbox had no access to crates.io. The
project will move to the standard ecosystem (serde, flate2, clap, memmap2, rayon, wgpu,
…) as soon as that access exists; see [docs/design/dependencies.md](docs/design/dependencies.md).
Builds with a stock stable Rust toolchain (tested with 1.97).

## Try it on your install

```bash
D="/path/to/Fallout New Vegas/Data"
fnvtool esm stats "$D/FalloutNV.esm"
fnvtool esm verify "$D/FalloutNV.esm"          # byte-identical rewrite + checks
fnvtool bsa list "$D/Update.bsa"
fnvtool esm loadorder "$D" FalloutNV.esm DeadMoney.esm HonestHearts.esm OldWorldBlues.esm \
    LonesomeRoad.esm GunRunnersArsenal.esm --schema schemas/fnv
fnvtool esm decode "$D/FalloutNV.esm" --schema schemas/fnv --edid GSDocMitchellScript
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
