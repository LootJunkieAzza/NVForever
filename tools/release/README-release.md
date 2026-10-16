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

Unpack anywhere outside the game folder (e.g. `C:\Games\NVForever`) and start it. At the
first start NVForever asks for your Fallout: New Vegas folder (the one holding `Data`, e.g.
`F:\SteamLibrary\steamapps\common\Fallout New Vegas`), checks that the game files it needs
are there and remembers the folder in `Documents\My Games\NVForever\NVForever.ini`
(edit or delete that file to change it; `--data` overrides it for one run). Then:

```
nvforever.cmd --play                     # Goodsprings, playing
nvforever.cmd --play --cell Primm
nvforever.cmd --help                     # every option
nvf-viewer.cmd --mesh meshes\clutter\...
```

On Linux use the `.sh` launchers and pass `--data` (or set `Path=` under `[Game]` in the
home's `NVForever.ini`). Settings, saves, logs and crash reports go to
`Documents\My Games\NVForever` (or `NVFOREVER_HOME`); caches to `%LOCALAPPDATA%\NVForever\cache`.

Run the programs through the launchers (or pass `--esm-schema`, `--nif-schema` and set
`NVF_PLAY_SCRIPT_FUNCTIONS` yourself): the built-in schema defaults point at the build
machine's checkout.

What works and what is not started yet: `docs/status.md` in the repository. Licence:
GPL-3.0-or-later (`LICENSE`); legal notes in `LEGAL.md`.
