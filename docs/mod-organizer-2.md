# Playing NVForever with Mod Organizer 2

Mod Organizer 2 (MO2) can start NVForever with the mods of its profile, the same way it
starts Fallout: New Vegas. Nothing needs installing or copying: add NVForever to MO2 once,
then pick it beside **Run**.

## Add NVForever to MO2 (once)

1. In MO2, open the executables list (the drop-down beside **Run**) and choose
   **<Edit…>**.
2. Press **+**, then **Add from file…**, and choose `nvforever.exe`.
3. Leave **Start in** as its folder and the arguments empty, then **OK**.

NVForever must be told where the game is as usual (the folder dialog at its first start,
or `Path=` under `[Game]` in `Documents\My Games\NVForever\NVForever.ini`): the same game
folder MO2 manages. It can be installed anywhere outside the game folder.

## What it uses from MO2

- **Mods** (left pane): the active mods' files appear in the game's `Data` folder for
  NVForever, overriding each other by MO2's priority, as they do for the game.
- **Plugins** (right pane): the plugins ticked there load, in that order. NVForever's own
  plugin list (`Documents\My Games\NVForever\plugins.txt`) is not used or changed by an
  MO2 start; started any other way, NVForever uses its own list as before.
- The run log says which list was used: `setup: Mod Organizer 2 start: plugins from its
  profile (...)`.

## What it does not use

- MO2's profile INIs (`Fallout.ini`, `FalloutPrefs.ini`): NVForever keeps its own settings
  in `NVForever.ini`.
- MO2's profile saves: NVForever's saves stay in `Documents\My Games\NVForever\saves`.
- NVSE, its plugins (`.dll`) and other programs hooking the original game do not run in
  NVForever. Mods that need NVSE script functions NVForever has not built yet may not
  work.

## How it works

MO2 starts programs with its file system hook (usvfs). NVForever sees the hook in its own
process and reads the plugin list MO2 shows it at the original game's list path
(`%LOCALAPPDATA%\FalloutNV\plugins.txt`), recognised by MO2's header line. Details and
sources: `crates/fnv-install/src/mo2.rs`, docs/design/install-layout.md.
