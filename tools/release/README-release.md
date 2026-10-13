# NVForever release build

NVForever is a clean-room reimplementation of the Fallout: New Vegas engine. **It ships no
game content**: you need your own copy of Fallout: New Vegas (1.4.0.525, base game and DLC).
NVForever only reads the game's `Data` folder and never writes into the game folder or
`Documents\My Games\FalloutNV`.

## What is in this archive

| File | What it is |
|---|---|
| `nvforever(.exe)` | the game: `--play` for the playable slice, without it a free-camera explorer |
| `nvf-viewer(.exe)` | the asset viewer: meshes, textures and animations from your data |
| `fnvtool(.exe)` | the command-line tool: setup, paths, archive and plugin inspection as JSON |
| `nvforever.cmd` / `.sh`, `nvf-viewer.cmd` / `.sh` | launchers that point the programs at `schemas/` beside them |
| `schemas/` | record, NIF and script-function schemas the programs read at start-up |

## Running

Unpack anywhere outside the game folder (e.g. `C:\Games\NVForever`), then:

```
fnvtool setup --game "F:\SteamLibrary\steamapps\common\Fallout New Vegas"
set NVFOREVER_DATA=F:\SteamLibrary\steamapps\common\Fallout New Vegas\Data
nvforever.cmd --play                     # Goodsprings, playing
nvforever.cmd --play --cell Primm
nvforever.cmd --help                     # every option
nvf-viewer.cmd --mesh meshes\clutter\...
```

On Linux use the `.sh` launchers and `export NVFOREVER_DATA=...`. Config, logs and
crash reports go to `%LOCALAPPDATA%\NVForever` (or `NVFOREVER_HOME`).

Run the programs through the launchers (or pass `--esm-schema`, `--nif-schema` and set
`NVF_PLAY_SCRIPT_FUNCTIONS` yourself): the built-in schema defaults point at the build
machine's checkout.

What works and what is not started yet: `docs/status.md` in the repository. Licence:
GPL-3.0-or-later (`LICENSE`); legal notes in `LEGAL.md`.
