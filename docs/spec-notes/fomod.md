# Spec note: FOMOD installers and mod archives, as the launcher reads them

What the launcher's Mods page does when a mod archive or a FOMOD installer is dropped on it
(`crates/nvf-launcher/src/fomod.rs`). FOMOD is the mod community's packaging format, not a
format of the original game, and nothing here comes from Bethesda's code. Where the public
sources leave a rule open, the launcher's choice is tagged `[design]`.

## Sources

- **[S1] "The fomod Documentation"** (fomod-docs), "written for the `5.x` major releases":
  https://fomod-docs.readthedocs.io/en/latest/ , with its pages "Specification"
  (`specs.html`), "A fomod Tutorial" (`tutorial.html`) and the schema reference "XML Schema
  Documentation" for `ModuleConfig.xml`, schema version 5.1 (`_static/ModuleConfig.html`; the
  schema is `ModuleConfig.xsd` in GandaG/fomod-schema, tag `5.1`). Read 2026-11-23.
- **[S2] The Nexus Mods App's developer note "About FOMOD"**: `docs/developers/misc/AboutFomod.md`
  in Nexus-Mods/NexusMods.App, as of commit `60cb48066236` (2024-06-18). Its old web address,
  `https://nexus-mods.github.io/NexusMods.App/misc/AboutFomod/`, answered 404 on 2026-11-23.
  This is the "Nexus Mods app note" the code's comments name.

## Packaging

- FOMOD mods are packed as `.zip`, `.rar` or `.7z`, and "Sometimes a file with the `.fomod`
  extension is actually just a `.zip` file" [documented: S2, "Intro"].
- An archive can hold another: "the outer archive could be a .rar file containing a single
  `.zip` file with `.fomod` extension which then contains the actual mod content"
  [documented: S2, "Packaging"]. The launcher opens one such inner archive, the only file of the
  outer one [design; `unwrap_nested`].
- The launcher also reads `.tar` and gzip (`.tar.gz`, `.tgz`) [design]. Its own zip and tar
  readers come first. An outside program (7z, bsdtar, unar, unrar) is used only for a format
  they do not read (`.7z`, `.rar`) or a file they cannot read as that format at all. An entry
  they refuse (one that would land outside the mod's folder, a link, a broken entry) ends the
  install, and no outside program gets the archive after it [design; `extract_archive`].
- The installer sits in a `fomod` folder holding `ModuleConfig.xml` (the installer) and
  `info.xml`, which has "no defined schema" [documented: S1, "Specification"].
- A C# script installer (`fomod/script.cs`) is code "that gets compiled on the user's system and
  run in the context of the mod manager" [documented: S2, "FOMOD C#"]. The launcher never
  compiles or runs one. An archive with only a script is refused; with a `ModuleConfig.xml` too,
  the XML is used and the page says the script was not run [design; `stage_tree`].

## Groups

- A group's `type` is one of `SelectAtLeastOne`, `SelectAtMostOne`, `SelectExactlyOne`,
  `SelectAll`, `SelectAny`: "The group type - affects which options can be chosen"
  [documented: S1, schema reference, `group`]. The tutorial calls them "self-explanatory" and
  defines none [documented: S1, "A fomod Tutorial"].
- The launcher reads them by their names [design]: at least one, at most one or exactly one
  usable option chosen; every option chosen; any number. An exactly-one group with more than one
  pick keeps the first. An exactly-one or at-least-one group with none gets one: the first
  `Recommended` option, else the first `Required`, else the first usable one
  [`Wizard::normalize`, `default_pick`].

## Option types

- An option's type is one of `Required`, `Optional`, `Recommended`, `NotUsable`,
  `CouldBeUsable` [documented: S1, schema reference, `pluginTypeEnum`]. The tutorial describes
  three of them, Optional ("where the option is... optional"), Required ("where the user doesn't
  really have a choice") and Recommended ("where the option is usually pre-selected. Be careful
  as implementation of this varies"), and calls the other two "useless" [documented: S1, "A fomod
  Tutorial"].
- The launcher [design]: a `Required` option is always chosen; a `NotUsable` one is never chosen
  and its files are not copied; `Recommended` is the default pick (above); `CouldBeUsable` is
  chosen freely, as `Optional` is.

## Flags and conditions

- A chosen option sets flags: "A flag to set if the plugin is selected" [documented: S1, schema
  reference, `flag`]. A step is shown when its `visible` conditions match: "Otherwise, the
  install step is skipped" [documented: S1, schema reference, `visible`]. For conditional
  installs, "All matching patterns will have their files installed" [documented: S1, schema
  reference, `conditionalFileInstalls`]. A `flagDependency` "Specifies that a condition flag
  must have a specific value" [documented: S1, schema reference].
- The launcher [design]: flag names and values match without regard to case. A condition list
  with `operator="Or"` holds when any part holds, otherwise when all do. A `fileDependency` is
  judged from the load order and the files found: a plugin is `Active` when it is on,
  `Inactive` when it is present but off, `Missing` when it is not there; another file is
  present when its path is found [`file_state`]. A `gameDependency` is not checked and always
  holds.

## Priority

- A file or folder's `priority` defaults to 0, and "A higher number indicates the file or
  folder should be installed after the items with lower numbers. This value does not have to be
  unique" [documented: S1, schema reference, `priority`]. Which copy is kept when two land on the
  same path, and what equal numbers do, is not said.
- "this format is not a fully specified standard, especially on the topic of priorities
  between multiple options containing the same files, the existing spec is fuzzy such that
  different managers, all following the spec correctly, can produce different results"
  [documented: S2, "FOMOD XML"].
- The launcher [design]: files are copied lowest priority first, so where two land on the same
  path the higher priority is copied last and kept. With equal priority the later one in this
  order is kept: the required files (`requiredInstallFiles`), then the chosen options' files in
  step, group and option order, then the conditional installs in pattern order; within one list,
  the order of the file [`Wizard::plan`, `copy_items`].
