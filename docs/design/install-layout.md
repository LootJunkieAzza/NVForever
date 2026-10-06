# Install layout: NVForever runs apart from the original game

Owner requirement: NVForever must not run from the game's folder or interact with it
beyond reading the game data.

## Three separate places

| Place | Example (Windows) | Who writes | Contents |
|---|---|---|---|
| **Original game** | `F:\SteamLibrary\steamapps\common\Fallout New Vegas` | Steam only | `Data\*.esm`, `*.bsa`, loose files: **read-only input** for NVForever |
| **NVForever program** | `C:\Games\NVForever\` (any folder outside the game) | installer / you | `nvforever.exe`, `fnvtool.exe`, `schemas\` |
| **NVForever home** | `%LOCALAPPDATA%\NVForever\` (or `NVFOREVER_HOME`) | NVForever | `nvforever.json`, `saves\`, `logs\`, `cache\`, `mods\` |

Never touched at all: the original game's user folder
`%USERPROFILE%\Documents\My Games\FalloutNV` (its `Fallout.ini`, `FalloutPrefs.ini`, saves
and `plugins.txt`). NVForever keeps its own settings, saves and load order, so playing
either game never changes the other.

## Rules enforced in code (`crates/fnv-install`)

- `GameInstall::open` accepts the install root or its `Data` folder and requires
  `Data\FalloutNV.esm`. Game files are only ever opened for reading.
- `check_separation` fails if: the home is inside the game install or the reverse; the
  NVForever executable is inside the game install; the home overlaps the original game's
  user folder.
- `guard_write` is the gatekeeper for every file NVForever writes; it refuses paths inside
  the game install or the original user folder. `fnvtool bsa extract` already uses it.
- `fnvtool setup --game <folder>` creates the home and records the game location in
  `nvforever.json`, refusing overlapping layouts; `fnvtool paths` prints the resolved
  layout and the separation check.

## Mods

Mods for NVForever go into `<home>\mods\` and are layered over the read-only game Data
by NVForever's virtual file system (planned with Layer 2/3). Nothing is ever installed
into the game's `Data` folder, so the original game keeps running exactly as before.

Paths come from the `directories` crate: the home default is the Local AppData known
folder, and the original game's user folder is located through the Documents known
folder, so a redirected Documents folder (e.g. OneDrive) is followed. Both are untested
on the owner's PC so far.

## Not yet handled

- Symlinks/junctions are resolved by canonicalization only for paths that exist.
