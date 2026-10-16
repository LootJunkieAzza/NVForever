# Install layout: NVForever runs apart from the original game

Owner requirement: NVForever must not run from the game's folder or interact with it
beyond reading the game data.

## Three separate places

| Place | Example (Windows) | Who writes | Contents |
|---|---|---|---|
| **Original game** | `F:\SteamLibrary\steamapps\common\Fallout New Vegas` | Steam only | `Data\*.esm`, `*.bsa`, loose files: **read-only input** for NVForever |
| **NVForever program** | `C:\Games\NVForever\` (any folder outside the game) | installer / you | `nvforever.exe`, `fnvtool.exe`, `schemas\` |
| **NVForever home** | `Documents\My Games\NVForever\` (or `NVFOREVER_HOME`) | NVForever | `NVForever.ini`, `saves\`, `logs\` (run logs, crash reports), `mods\` |
| NVForever cache | `%LOCALAPPDATA%\NVForever\cache\` (`<home>\cache` with `NVFOREVER_HOME`) | NVForever | generated LOD; can be deleted |

The home is beside the original game's own `My Games\FalloutNV` (owner request
2026-10-16), never inside it. The cache stays in Local AppData [our choice]: it can grow to
gigabytes and Documents is often synced (OneDrive).

Never touched at all: the original game's user folder
`%USERPROFILE%\Documents\My Games\FalloutNV` (its `Fallout.ini`, `FalloutPrefs.ini`, saves
and `plugins.txt`). NVForever keeps its own settings, saves and load order, so playing
either game never changes the other.

## First start (`engine/nvforever/src/setup.rs`)

The game's folder comes from, in order:

1. `--data` / `NVFOREVER_DATA` (developers and the smoke tests): never stored, never a
   dialog, so a scripted run fails instead of waiting.
2. `NVForever.ini` in the home:
   ```ini
   [Game]
   Path=F:\SteamLibrary\steamapps\common\Fallout New Vegas
   ```
3. Otherwise NVForever asks: a welcome box, then the system's folder picker (opened at
   Steam's or GOG's default folder when one exists). The player picks the **game's root
   folder** (the one holding `FalloutNV.exe` and `Data`; picking `Data` itself is accepted
   and stored as its parent). A folder that cannot be used is refused with the reason and
   the picker comes back; Cancel ends the start. The folder is then stored in the INI.

A stored folder that no longer works (moved, files missing) brings back the picker with the
reason. Saving the INI changes only `Path` and keeps other lines and comments.

The settings page's switches (pause menu and main menu) are kept in the same INI, one
`key=1` / `key=0` line each under `[Settings]` (`nvf_play::settings`). Every flip writes
all of them; each start, including the restart that loads a save, sets the listed
switches to their stored values, so the INI wins over the command line. Unattended runs
(test aids) neither read nor write the INI of the player's own home.

**Before every start** the folder must pass (`fnv_install::GameInstall`):

- `Data\FalloutNV.esm` exists (the folder is a game install at all);
- every file of `REQUIRED_DATA_FILES` is in `Data` and not empty: `FalloutNV.esm`, the six
  archives of the game's `SArchiveList` (`fnv_vfs::DEFAULT_ARCHIVE_LIST`, a test keeps the
  two lists together) and `Update.bsa` [observed in the owner's 1.4.0.525 install]. Names
  match without case. The message names each missing file;
- the separation rules below.

The add-on masters (Dead Money … Caravan Pack) are optional: without `--plugins` the game
loads the ones present and says which are missing.

On the first start of the new home, the settings and saves of the earlier home
(`%LOCALAPPDATA%\NVForever`, builds before 2026-10-16) are **copied** over once (the old
folder is left alone; saves already in the new home are not replaced). `nvforever.json`
there is still read when no INI exists.

## Rules enforced in code (`crates/fnv-install`)

- `GameInstall::open` accepts the install root or its `Data` folder and requires
  `Data\FalloutNV.esm`. Game files are only ever opened for reading.
- `check_separation` fails if: the home is inside the game install or the reverse; the
  NVForever executable is inside the game install; the home overlaps the original game's
  user folder.
- `guard_write` is the gatekeeper for every file NVForever writes; it refuses paths inside
  the game install or the original user folder. `fnvtool bsa extract` already uses it.
- `fnvtool setup --game <folder>` creates the home and records the game location in
  `NVForever.ini`, refusing overlapping layouts and installs that lack required files;
  `fnvtool paths` prints the resolved layout, the separation check, missing files and the
  add-ons present.

## Mods

Mods for NVForever go into `<home>\mods\` and are layered over the read-only game Data
by NVForever's virtual file system (planned with Layer 2/3). Nothing is ever installed
into the game's `Data` folder, so the original game keeps running exactly as before.

Paths come from the `directories` crate: the home is under the Documents known folder and
the cache under the Local AppData known folder, so a redirected Documents folder (e.g.
OneDrive) is followed. Off Windows the home is the platform data directory (no dialog
there yet: pass `--data` or write the INI).

## Not yet handled

- Symlinks/junctions are resolved by canonicalization only for paths that exist.
- No in-game way to change the game folder; edit or delete `NVForever.ini`.
