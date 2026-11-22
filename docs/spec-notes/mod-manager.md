# Spec note: the launcher's mod manager

NVForever's own rules for the Mods page and for what a start without `--mo2`
reads. This is not a claim about the original game, and it is not another
program's deploy step. No code was copied. The game `Data` folder is not
written.

The smaller notes are
[launcher-mod-visibility.md](launcher-mod-visibility.md) (what the game
process reads) and [launcher-profile.md](launcher-profile.md) (a named
profile). `--mo2` is a different, read-only path
([mod-organizer-profile.md](mod-organizer-profile.md)).

## Where a mod lives

[design] Each mod is one folder under the launcher home's `mods` directory.
The home is `~/.local/share/nvforever` on Linux (or `$XDG_DATA_HOME/nvforever`)
and `Documents\My Games\NVForever` on Windows, unless `NVFOREVER_HOME` is set
[documented: `EngineHome::default_root` in `crates/fnv-install/src/lib.rs`].

A mod's meshes, textures, and archives sit in that folder, or in a `Data`
folder inside it when the mod has one [documented: `plugins::mod_content_root`].
They are not copied, hard-linked, or symlinked into the game `Data` folder.

A plugin, archive, or loose file that is already in `Data` is still read. The
mods folder does not hide the base game or a mod that was installed there by
hand [documented: `plugin_path`, `Vfs::open_launcher_mods`].

## The two home lists

[documented: `Launcher::save_mods` and `Launcher::remember_lists`] The launcher
keeps two text files in the home:

- `plugins.txt`: which plugins are on, and their load order. A `*` turns a
  line on. The format is NVForever's own
  [documented: `crates/fnv-install/src/plugins.rs`].
- `mod-order.txt`: which mod folders are on, and their order, in Mod Organizer
  2's `modlist.txt` form, so MO2 can read it [design: owner 2026-11-22, every
  list file stays readable by MO2]. One line a row, the **highest priority
  first**: `+name` on, `-name` off, a separator as `-<name>_separator`, CRLF
  line ends, no header [documented: docs/spec-notes/mod-organizer-profile.md,
  `modlist.txt`; `fnv_install::mod_order`]. The first on-line wins a loose-file
  overlap [documented: `plugins::mod_content_layers`]. The page shows that
  winner at the bottom of the list.
- `mod-locks.txt`: the locked mods, one name a line. MO2's list has no lock
  mark, so locks are kept beside it [design].

A `mod-order.txt` in the launcher's earlier form is still read: lowest priority
first, `*` on, a bare name off, `=` locked, `% name` a separator, `;` a comment.
The `;` comment, a `*` line or a bare name marks that form. The next write
turns it into MO2's form [documented: `mod_order::read_mod_order` and its
tests].

Both files are written when the list changes, with no profile open and without
pressing **Save**. Closing the launcher and opening it again restores that
list. A profile is not required for this.

Writing either file does not make a backup. A dated copy is made only when
**Save backup** is pressed [documented: `Launcher::save_dated_backup`].
Lowering the number of backups kept asks before the oldest copies are deleted.
The mods themselves are not deleted.

## What the game is given

The game process does not read `mod-order.txt`. When the launcher starts the
game it passes each enabled folder as `--mod-layer`, lowest priority first
[documented: `launch::spawn`]. The last layer wins.

A start that was not given `--mod-layer` still layers every mod folder, in
name order. Folder on/off is not applied on that path
[documented: `plugins::mod_folder_layers`].

`--mo2` is not this path. It still only reads another program's instance.
The launcher does not write that instance.

## A named profile

[documented: docs/spec-notes/launcher-profile.md] **Save** writes
`<home>/profiles/<name>/` and only `modlist.txt`, `plugins.txt`, and
`loadorder.txt`. **Load** puts those lists back on the page. An open profile
overrides the home lists the next time that profile is opened. The home lists
still exist on their own.

## Not in this manager

- The game `Data` folder is not written. Neither is
  `Documents\My Games\FalloutNV`, except the one case already documented for
  an external instance ([documented: `crates/fnv-install` module notes]).
- An output folder, `overwrite`, is not created. The same relative path there
  would win over a mod folder. That folder is not built yet
  [design: docs/design/launcher-mods.md].
- Nexus and ModPub downloads are not part of the running launcher.
