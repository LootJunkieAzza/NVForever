# Spec note: how a launcher mod is seen

NVForever's own rules for the launcher's mods folder. This is not a claim about
the original game, and it is not another program's deploy step. No code was
copied. The game `Data` folder is not written.

## What Apply does

[documented: `Launcher::apply_mods` and `Launcher::save_mods` in
`crates/nvf-launcher/src/app.rs`] **Apply** writes two text files in the
launcher home: `plugins.txt` (which plugins are on) and `mod-order.txt` (which
mod folders are on, and their order). It does not copy, hard-link, or symlink
mod files anywhere. A profile **Save** or **Load** writes or reads
`profiles/<name>/` only.

`mod-order.txt` is read by the launcher when it draws the page. The game
process does not read it.

## What the game process reads

The launcher starts NVForever. NVForever indexes files itself. It does not
ask the original executable to look in a second folder.

[documented: `plugin_path` and `is_base_game_plugin` in `crates/fnv-install/src/plugins.rs`,
and `engine/nvforever/src/main.rs` when `--mo2` is not set]

- The base game is read from `Data`: `FalloutNV.esm`, the add-on masters, their
  `<master>_lang.esp` files, the base archives, and the loose files that ship there.
- Any other plugin is a mod. It is opened only from the mods folder. The walk
  enters subfolders. A copy of that plugin in `Data` is not opened.
- A folder turned off in `mod-order.txt` is not entered. A folder the file does
  not mention is on. The last on-line wins.
- [documented: `Vfs::open_launcher_mods`] Each enabled mod folder is its own
  layer, lowest priority first. If the folder contains `Data`, loose files are
  read from that directory. Archives are read from the mod folder and from
  folders nested inside it. A mod archive or loose file is not read from the
  game `Data` folder.
- [documented: `Layout::mod_roots` and `Vfs::open_for_plugins_layered`] A start
  with `--mo2` is unchanged: each enabled mod folder of that instance, then its
  overwrite folder. That start only reads.

## Links, and Windows

[documented: `link_on_rows_into_data`, `symlink_mod_into_data`, and
`ensure_mod_link` in `crates/nvf-launcher/src/app.rs`] Those functions are not
called by **Apply** or **Load**. They call `std::os::unix::fs::symlink` and
would write into the game `Data` folder. There is no Windows version of them.
Leaving them unused is intentional.

[documented: `link_into_mods` and `symlink_file` in
`crates/fnv-install/src/plugins.rs`] The only link still made is into the mods
folder, and only when `NVFOREVER_PLUGIN_SEARCH` finds a plugin that is in
neither the mods folder nor `Data`. The destination is refused if it would be
inside the game folder.

The order is the same on both platforms:

1. A hard link of that one file into the mods folder. This works when the
   source and the mods folder are on the same volume. It does not need the
   privilege Windows asks for a symbolic link.
2. If the hard link fails because the paths are on different disks, a symlink:
   `std::os::unix::fs::symlink` on Unix, `std::os::windows::fs::symlink_file`
   on Windows. A directory junction is not created. On Windows that file
   symlink fails unless the account is allowed to create symbolic links.
3. A copy, only when `NVFOREVER_PLUGIN_COPY` is set and the symlink failed.
   The copy is still into the mods folder, not into `Data`.

If every link fails, the plugin is read from the path where it was found.

## A mods folder as an extra Data folder

[documented: `Vfs::open_launcher_mods` and `plugins::mod_content_layers`] This is
what a start without `--mo2` does now. Each enabled mod folder is a root over
`Data`, lowest priority first, on Linux and Windows the same way. The index
only reads. Nothing is written into `Data`.

The base game is still read from `Data`. A mod file is not. The original game's
executable is not given a second Data path.
