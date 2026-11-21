# Launcher mods: a named profile, and where new files go

The launcher's own manager keeps each mod as one folder under the home's `mods`
directory. This note is the design for saving that page as a named profile, and
for a later output folder. It is not an import of another program's code. Reading
an existing instance with `--mo2` stays a separate, read-only path
([mod-folders.md](https://github.com/LootJunkieAzza/NVForever/blob/main/docs/design/mod-folders.md),
[#1526](https://github.com/LootJunkieAzza/NVForever/issues/1526)).
The file layout that reader already understands is written up in
[mod-organizer-profile.md](https://github.com/LootJunkieAzza/NVForever/blob/main/docs/spec-notes/mod-organizer-profile.md).
The files this page writes are in
[launcher-profile.md](../spec-notes/launcher-profile.md).
This note does not add facts about the original game.

No settings row is added. If one is added later it follows
[settings-page.md](https://github.com/LootJunkieAzza/NVForever/blob/main/docs/design/settings-page.md) §0.

## A profile is a name and two lists

The player types a name. **Add** and **Save backup** write the same profile:
the folder order on the left and the plugin order on the right. The name is one
plain folder name, not a path. An empty name is refused.

The profile is a folder under the launcher home, `profiles/<name>/`. Saving
writes only these three text files:

| File | What it holds |
|---|---|
| `modlist.txt` | One mod folder per line. `+` means on, `-` means off. The winning folder is first in the file. The page shows that folder last, which is the order the page already uses when it reads a list. |
| `plugins.txt` | The plugins that are on, one per line, a `*` before the name. |
| `loadorder.txt` | Every plugin on the page, first loaded first. |

Saving the same name again replaces those three files and leaves any other file
in the folder. Nothing is copied. The mods folder is not the place a profile is
written.

A dropped folder is only for reading an older list. The drop fills the path
line. The path cannot be typed. **Read** lists profiles in that folder, and
**Load** puts the chosen lists on the page. A save does not write into the
dropped folder, so an older instance is left as it was.

**Load** of a profile this page saved puts both lists back. The plugin backup
list (the dated copies made when the live list is saved) is a different store.
**Save backup** used to write `plugins.txt.bak.1` beside the live list, which
that dated list never shows. It now writes the named profile instead.

## What is not written

The profile save does not write the game folder, `Documents\My Games\FalloutNV`,
or the original game's plugin list. That is the install rule
([install-layout.md](https://github.com/LootJunkieAzza/NVForever/blob/main/docs/design/install-layout.md)).

The page's own **Save**, and **Load** of a profile, still make symlinks in the
game `Data` folder for a turned-on mod. That is older behaviour on this branch
and it breaks the same rule. This design does not add more of those writes.
Taking them out is a later change: the mods have to be layered by the program
itself, and this launcher-only branch does not build that layer. Until then,
removing the links would leave the mods unloaded.

## Output folder, not built yet

Files the game or a tool creates while running, other than the saves the
launcher already keeps, should land in one folder under the home, `overwrite`.
It is the last layer: the same relative path in `overwrite` wins over a mod
folder. It is not a profile, and it is not a copy of `Data`. It is not created
by the named-profile save.

## What stays read-only

`--mo2` still only reads another program's instance and profile. The launcher
does not write that instance's profile folders, its output folder, or its INI
files.
