# Spec note: NVForever's mod manager

Clean-room description of the launcher's Mods page and of how a start uses
`<home>/mod-order.txt`. These are NVForever's own rules. Nothing here is a claim
about what the original game does. Tags: **[documented]** cites a file in this
branch, **[hypothesis]** is not checked on a running game.

Sources: `crates/nvf-launcher/src/app.rs` (`load_mod_rows`, `write_mod_order`,
Apply), `engine/nvforever/src/main.rs` (`enabled_mod_dirs`, `mod_loose_roots`,
`mod_plugin_roots`), `crates/fnv-install/src/plugins.rs` (`plugin_path_in`,
`link_into_mods`), `crates/fnv-vfs/src/lib.rs` (`open_for_plugins_layered`),
`crates/fnv-vfs/src/casefold.rs`. The home and the read-only game Data folder
are already described in `docs/design/install-layout.md`.

## The list

[documented: `load_mod_rows`, `write_mod_order`]

- The folders are the real directories in `<home>/mods/`. A symbolic link is
  skipped. The page also skips a name that starts with `.`.
- `<home>/mod-order.txt` is one folder name per line. A leading `*` means the
  folder is on. A blank line, or a line starting with `;` or `#`, is a comment.
  The first line the page writes is `; A star means this folder is on.`
- Names match without case. A folder that is not in the file is added at the
  end, on. Dragging a row changes this order. A check changes the star.
- Apply writes this file and the plugin list. The page says the files stay in
  their folders. It does not copy them into the game's Data folder.
- A folder, a loose `.esp`/`.esm`/`.bsa`, or an archive dropped on the launcher
  (or typed into Add) is copied into `<home>/mods/` under its own name. Archives
  are `.zip`, `.7z`, `.rar`, `.fomod` (a zip), `.tar`, and `.gz`, and the same
  formats when the file's bytes say so. An existing folder is not replaced.
- The archive is opened first. One archive whose only content is another archive
  is opened too. A `Data` folder, or a single wrapper folder, is the mod. A
  plugin that sits only in an installer folder such as `optional`, beside a
  readme, is copied out so it is a real plugin. A folder with no plugin and no
  game files is not added.
- `fomod/ModuleConfig.xml` asks, one step at a time, when there is a choice.
  The window is one step. A group is choose one, at most one, at least one, all,
  or any. An option is required, optional, recommended, not usable, or could be
  usable. The chosen files are what land in the mod folder. A higher file
  `priority` wins. A `fomod/script.cs` installer is not run.
- Plugins from the new folder are added to the list. A new folder is on, so
  those plugins are on. Save keeps the list. Dropping used to add the folder
  and leave the plugin list unchanged.

## Which copy is used

[documented: `mod_loose_roots`, `open_for_plugins_layered`, `mod_plugin_roots`,
`plugin_path_in`]

- Loose files are layered over the game's Data folder, lowest first. The mods
  folder itself is first, then each checked folder from the top of the list to
  the bottom. A later layer replaces an earlier one, so the lower checked
  folder's copy of the same file is the one used. The page says the same thing,
  and it marks the used copy green and the skipped copy red.
- A checked folder that contains a `Data` directory contributes loose files
  from that directory. Otherwise it contributes loose files from the folder
  itself.
- A plugin is opened from the first root that has it. Those roots are the
  checked folders from the bottom of the list upward, then the mods folder,
  then the game's Data folder. The lower checked folder's plugin is therefore
  the one opened.
- A folder with no star is not a layer.

[hypothesis: not watched on a running game] The green and red notes follow this
order. No run recorded here compared two real mod folders.

## The game folder is not written

[documented: `link_into_mods`, `casefold.rs`]

- A plugin that is not in the mods folder or in Data is searched only when
  `NVFOREVER_PLUGIN_SEARCH` names directories. The link is made in the mods
  folder: a hard link, or a symbolic link when the file is on another disk.
  A copy is made only when `NVFOREVER_PLUGIN_COPY` is set. If the destination
  would be inside the game's Data folder, the link and the copy are refused
  and the file is read from where it already is.
- `NVFOREVER_NO_PLUGIN_SEARCH` turns that search off.
- Directories inside the NVForever home whose names differ only by case can be
  joined. The preferred spelling, when the code lists one, is `meshes`,
  `textures`, `sound`, `music`, `menus`, `shaders`, `video`, `NVSE`, `Config`,
  `uio`, `facegen`, `distantlod`, `LSData`, `Plugins`, or `Scripts`. Two copies
  with the same bytes collapse to one. Two copies that differ keep the one
  already in the surviving directory and rename the other to `name.case-conflict`.
  This returns immediately unless the directory is under the NVForever home, so
  the game's Data folder is not entered. `NVFOREVER_NO_CASEFOLD` turns it off.

## What this note does not say

The page counts active plugins as `n / 255`, including the base game
[documented: the plugins column]. That is the page's counter. It is not a new
measurement of the original game. No words shown by the original game were
copied. The sentences on the Mods page are NVForever's own.
