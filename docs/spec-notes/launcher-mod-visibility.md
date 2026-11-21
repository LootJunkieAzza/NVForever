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
process does not read it [documented: `engine/nvforever/src/main.rs` opens
`GameData::open_for_plugins_in` when `--mo2` is not set, and
`plugins::plugin_path` does not open `mod-order.txt`].

## What the game process reads

The launcher starts NVForever. NVForever indexes files itself. It does not
ask the original executable to look in a second folder.

[documented: `plugin_path` in `crates/fnv-install/src/plugins.rs`, and
`engine/nvforever/src/main.rs` when `--mo2` is not set]

- A plugin is opened from the mods folder when that folder has it, including a
  file nested in a subfolder. The same name in `Data` is used when the mods
  folder does not have it. A mod installed in `Data` by hand is still read.
- Loose files and the base archives in `Data` are still read
  [documented: `Vfs::open_for_plugins_in`].
- Archives that sit beside a plugin in the mods folder are mounted with that
  plugin [documented: `Vfs::archive_order_in`].
- [documented: `Layout::mod_roots` and `Vfs::open_for_plugins_layered`] A start
  with `--mo2` is unchanged: each enabled mod folder of that instance, then its
  overwrite folder. That start only reads.

## Links

Nothing in the launcher links or deletes files in the game `Data` folder.

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
