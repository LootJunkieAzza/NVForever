# Spec note: a launcher profile

The launcher's own profile, written from the Mods page. This is not a description
of another program's files. Those, and the public sources they were read from,
are already in
[mod-organizer-profile.md](https://github.com/LootJunkieAzza/NVForever/blob/main/docs/spec-notes/mod-organizer-profile.md).
No code was copied. Nothing here was taken from the original game.

## Where it is written

These are the launcher's own choices, written up before the code in
[launcher-mods.md](../design/launcher-mods.md).

- The folder is `<home>/profiles/<name>/`. `<home>` is the launcher home. `<name>`
  is the name the player typed: one folder name, not a path.
- A dropped folder is read only. The save does not write there.
- The game folder and `Documents\My Games\FalloutNV` are not written
  [documented: docs/design/install-layout.md on NVForever main].

## The three files

The launcher's own text files, not a format read out of the original game:

- `modlist.txt`: one mod folder per line. `+` is on, `-` is off. Lines that are
  empty or start with `#` are skipped. The winning folder is the first line.
  The page stores that folder last, and reverses the file when it reads it.
- `plugins.txt`: plugins that are on, one per line. A leading `*` marks one on
  and is not part of the file name.
- `loadorder.txt`: every plugin on the page, one per line, the first loaded
  first. A leading `*` is ignored if it is present.

Saving the same name again replaces these three files. Any other file in the
folder is left in place.
