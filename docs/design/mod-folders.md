# Mods folders: an MO2 profile's merge without MO2 running

Mod Organizer 2 shows a program the game's `Data` folder with the profile's mods merged into
it, through its file system hook, usvfs [documented: MO2 source, `OrganizerCore::beforeRun`
and `fileMapping`]. On Windows NVForever only has to notice the hook and read the plugin list
MO2 maps in (`crates/fnv-install/src/mo2.rs`). On Linux there is no MO2 and no hook, so the
engine does the merge itself, inside its own virtual file system (#1526): point it at an MO2
instance and a profile, and the profile's enabled mods' loose files are read as if they sat in
`Data`, the overwrite folder last, and the profile's `plugins.txt` decides the load order.

The instance and the profile stay MO2's: installing mods, ordering them and ticking plugins is
still done in MO2 (on Windows, or in a Windows install shared with a Linux one). NVForever
only reads the profile. Nothing is written into the game folder or into MO2's folders.
Formats and their public sources: [spec note](../spec-notes/mod-organizer-profile.md).

## Using it

```
nvforever --mo2 "<instance folder>" [--profile <name>]
fnvtool vfs mods "<instance folder>" [--profile <name>] [--data <Data> --path <game path>]
```

The instance folder is the one holding `ModOrganizer.ini` (a portable instance, or a global
one under `%LOCALAPPDATA%\ModOrganizer\<name>` copied over). `--profile` defaults to the
instance's `selected_profile`. The game's `Data` folder still comes from `--data` /
`NVForever.ini`, so a profile copied from Windows may keep its Windows `gamePath`.

`fnvtool vfs mods` prints the profile as the merge it means (`fnv.vfs.mods/1`): the mods in
list order with their enabled state, the folders from the lowest priority up, the active
plugins, and any `.bsa` a mod holds that is not mounted yet. With `--data` and `--path` it
resolves one game path through the same layers the engine reads, and names the layer that wins.

## What is merged, and in which order

Precedence, lowest first:

| Layer | Source | Notes |
|---|---|---|
| 1 | the game's `Data` loose files | as before; archived copies under them |
| 2 | each enabled mod's folder | from the lowest priority up, so a higher priority mod replaces a lower one's file |
| 3 | the overwrite folder | always last [documented: MO2 `Profile::doWriteModlist`, `overwrite` priority] |

Folders come from `ModOrganizer.ini` [documented: MO2 `PathSettings`]: `mod_directory`,
`profiles_directory`, `overwrite_directory` under `[Settings]`, each defaulting to
`%BASE_DIR%/mods`, `%BASE_DIR%/profiles`, `%BASE_DIR%/overwrite`, and `%BASE_DIR%` defaulting
to `[Settings] base_directory`, itself defaulting to the instance's folder. The profile is
`[General] selected_profile`, or `--profile`.

`modlist.txt` is read as MO2 writes it [documented: `Profile::doWriteModlist`]: a header line,
then `+Name` (on), `-Name` (off) and `*Name` (the game's own files, on), **highest priority
first**; the file is the profile's list without the automatic rows (separators, backups). A
mod the list names whose folder is gone is skipped rather than becoming an empty layer.
Disabled mods are not read at all.

`plugins.txt` in the profile is read as MO2 writes it [documented:
`GamebryoGamePlugins::writeList`]: the header line, then the **active** plugins only, one per
line, in load order, in the system's ANSI code page. `loadorder.txt` is not needed: the same
order is in `plugins.txt`, and a profile MO2 has not written yet has neither. A visited plugin
is opened from the highest layer that has it (overwrite, then the mods from the highest
priority down), else from `Data`.

## What this does not do (yet)

- **Archives in mods.** A `.bsa` inside a mod folder is not mounted: `Vfs` mounts the `Data`
  folder's archives, `Update.bsa` and the active plugins' archives as the game does. A mod
  that ships its files in an archive therefore contributes nothing yet; the engine warns at
  start with the number of such archives and up to five names. Phase 2 of #1526.
- **Profile INIs.** `Fallout.ini`, `FalloutPrefs.ini`, `initweaks.ini` are not read; NVForever
  keeps its own settings and does not merge MO2's tweaks.
- **Saves.** The profile's saves are not used, and a save NVForever writes does not go into
  the profile (the editor's nothing-to-do-with path uses `Layout::save_target` for the
  original game's plugin saves, not for this).
- **Files read outside `Vfs`.** Anything that opens `Data` with plain file calls instead of
  through the virtual file system will not see the mods. The engine's assets go through
  `GameData`/`Vfs`; this is the honest limit of an in-process merge, and worth naming in a
  bug report that looks like "the mod file is there but the game does not see it".
- **usvfs itself.** No file-system hook, no copy-on-write into overwrite, no per-file
  shadowing rules beyond path precedence. Only the paths above are merged.

## Code

- `crates/fnv-install/src/mo2.rs`: `Layout::open` (instance + profile), `mod_roots`
  (lowest first, overwrite last), `profile_plugins`, `mod_archives` (the warning's list).
- `crates/fnv-vfs/src/lib.rs`: `Vfs::open_for_plugins_layered` indexes each root over `Data`
  in order, later roots replacing earlier files by normalised path (case- and
  slash-insensitive); `loose_roots` reports the layers.
- `crates/fnv-install/src/plugins.rs`: `plugin_path_in` looks a plugin up through the roots,
  highest first.
- `engine/nvforever/src/main.rs`: `--mo2`, `--profile`; the profile's plugins take the place
  of the home `plugins.txt` when `--plugins` is not given.
- `crates/fnvtool/src/bsa_cmd.rs`: `vfs mods`, the verification command.

## Tests

- `crates/fnv-install/src/mo2.rs` tests: the roots' order (lowest priority, overwrite last),
  a gone mod folder skipped, `ModOrganizer.ini` folders and `--profile`, archives named once.
- `crates/fnv-vfs/tests/layered.rs`: a higher layer replaces a lower one's file regardless of
  case or slash; a lower layer's untouched files stay readable; absent layers are skipped.
- Verified on this host against the GOG install (2026-11-17): a synthetic instance with
  `Base`, `Late` and `overwrite` all holding `textures\test\shared.dds` resolved to
  `overwrite`'s copy through `fnvtool vfs mods --path`, and an engine run with the same
  instance read a corrupt `FalloutNV.esm` placed in `overwrite` ("plugin FalloutNV.esm left
  out: cannot be read"), proving the plugin lookup goes through the layers.
