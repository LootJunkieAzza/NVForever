# Handoff: bring the website (LootJunkieAzza/NVForever-Site) up to date (2026-10-16)

For the NVForever-Site session. The site's text was written against the state of
2026-10-12 and the v0.1.0 release notes; main has moved a lot since (PRs #252–#302 and the
main-menu commits of 2026-10-15). The repository's own docs were brought up to date in the
same change as this note: **`README.md` and `docs/status.md` are the source of truth**. Take
facts from them, not from this summary, if the two ever differ.

## 1. What changed that the site must reflect

- **The game now starts at the main menu.** `nvforever --play --menu` opens on the original's
  main menu over the plugins' loading screens, with the original's logo and theme read from
  the user's own game files: Continue (newest save), New Game, Load Game, Settings, Quit.
  New Game plays the intro film (`Video\FNVIntro.bik`, any key or click skips it), then the
  opening at Doc Mitchell's. (`--play --new-game` still goes straight to the opening,
  `--play --load "<save>"` to a saved game, `--cell X --play` to a place.)
- **Saved games exist** (first version): F5 quicksave, F9 quickload, Save/Load on the pause
  menu with thumbnails, autosaves after travel. Saves live in `Documents\My Games\NVForever\saves`.
- **Pause menu** in the original's look: Continue, Save, Load, Settings, Quit (Quit asks
  Main Menu / Exit Game / Cancel).
- **Crafting** (first version): workbench, campfire and reloading bench recipe menus.
- **HUD** closer to the original: HP/AP/CND bars, compass marks, location discovery, sneak
  word ([HIDDEN]/[CAUTION]/[DANGER]), enemy health bar, reputation boxes.
- **Sun shadows are off by default** since 2026-10-13 (`--sun-shadows` or the settings page
  turns them on). The site's "finer shadows" wording for `--graphics enhanced` should not
  imply sun shadows are on.
- Mods: `--plugins <files…>` loads a load order with mods; their archives follow.
- Program icon on the exe and window; the game's own mouse pointer.
- **First start (#303)**: NVForever's home (`NVForever.ini`, saves, logs, mods) is now
  `Documents\My Games\NVForever` (not `%LOCALAPPDATA%\NVForever`; caches stay there). With
  no `--data` and no stored folder, the first start shows a welcome box and a folder picker
  (Windows) for the game's **root** folder, checks the required game files and stores it in
  `NVForever.ini`. `--data` only overrides it for one run. On Linux: `--data`, or `Path=`
  under `[Game]` in the INI. `fnvtool setup` writes the INI too. Still never touches
  `My Games\FalloutNV`.
- Still **not started**: VATS, levelling up and perks, repair, the Caravan and casino games,
  the companion wheel, credits.

## 2. The release caveat (important)

The only release is **v0.1.0, a pre-release of 2026-10-13**. It predates saves, crafting
and the main menu, so **v0.1.0 has no `--menu`**. Until a new release is cut, the site must
not tell people the download opens on a main menu. Two honest options:
1. describe the main menu as "on main, in the next release", keeping v0.1.0's launch line; or
2. wait for the owner to tag v0.2.0 (the release workflow runs on a `v*` tag) and then update
   `assets/js/config.js` (version, date, `downloadUrl`, `fileName`, `fileSize`, `sha256`).

## 3. Specific places on the site that are wrong now (index.html)

- **Status section** ("as of 12 October 2026"): the "Not started" row lists saved games,
  crafting and the main menu. Move saved games and the main menu to done/first version,
  crafting to partly; date it 16 October 2026. Mirror `docs/status.md`.
- **FAQ "Can I play the whole game with it?"**: says saved games and crafting are still to
  come. Same fix.
- **"What it does today" cards**: add saves and the main menu with the intro; crafting in
  "Wasteland life"; the pause and main menus in "The game's own interface".
- **Install → Launch**: it runs `nvforever --data ... --play` directly. A release build must
  be started through the launchers beside the programs (`nvforever.cmd` on Windows,
  `./nvforever.sh` on Linux): the built-in schema paths point at the build machine
  (`tools/release/README-release.md`). For v0.1.0, `--data` or `NVFOREVER_DATA` (the
  game's `Data` folder) is required. From the next release the steps shrink to: unpack,
  run `nvforever.cmd --play --menu`, pick the game folder once; drop the `fnvtool setup`
  step or mark it optional.
- **FAQ / uninstall / crash answers**: NVForever's files move from `%LOCALAPPDATA%\NVForever`
  to `Documents\My Games\NVForever` (caches stay in `%LOCALAPPDATA%\NVForever\cache`) in
  the next release; v0.1.0 still uses `%LOCALAPPDATA%`. Linux paths: see install-layout.md.
- **Controls**: add F5 quicksave, F9 quickload, `` ` `` developer console; Esc is the pause
  menu (Continue, Save, Load, Settings, Quit).
- **Bug reports**: GitHub now has issue forms (bug, crash, differs from the original,
  build/tool, feature): link `https://github.com/LootJunkieAzza/NVForever/issues/new/choose`.

## 4. Rules

- The site is a separate private repo; open a PR there, as usual for that repo.
- No game content on the site: no screenshots or frames rendered from game data (the legal
  section already says so). Keep the YouTube embeds as they are.
- Don't claim features the downloadable build doesn't have (section 2).
