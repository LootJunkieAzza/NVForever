# Launcher (`nvforever-launcher`, #2708)

A small window beside `nvforever.exe` (`crates/nvf-launcher`, egui/eframe on OpenGL) that
shows the project's news, starts the game and changes the settings in `NVForever.ini` without
starting the game first (owner request, 2026-11-15). The game does not need it: a start
without the launcher is the same as before.

## Pages

- **Home**: news posts. First the site's `https://nv-forever.com/news.json`
  (`{"posts":[{"title","date","author","image","body" (Markdown),"url"}]}`, see
  `src/news.rs`), and while the site has none, this repository's GitHub releases (title, date,
  notes). The last list is kept in the home's cache folder (`launcher-news.json`) for offline
  starts. "Fetch news" (`[Launcher] News=0/1`) turns every request off.
- **Settings**: the `[Settings]` rows by the in-game page's tabs and headings, with its labels
  and notes, read from `docs/settings-reference.md` (built in). The reference does not say what
  kind a row is, so the launcher goes by the value in the INI: `1`/`0` a switch, `0.50` a
  slider (as a percentage, or the step's words for Difficulty), anything else text; Kill camera
  and V.A.T.S. style get their choices as a list (a test checks the engine still has them). A
  row the game has not written yet shows "game default". Rows the reference does not list go
  under Other. Save writes only the changed keys over the file as it is on disk then.
- **Mods**: NVForever's own `plugins.txt` (turn on/off, move). Opening the page runs
  `fnv_install::plugins::sync`, as the game's start does, so new plugins get their lines.
- **Game folder**: `[Game] Path` with the required-files check, the program to start
  (`[Launcher] Exe`, else `nvforever(.exe)` beside the launcher, else this checkout's
  `engine/target/release`), and buttons that open the saves, logs, screenshots and settings
  folders.
- **Website / GitHub**: open nv-forever.com and the repository in the browser.

## Top and bottom bars

- The site's next-release countdown (the `nextRelease` block of the site's
  `assets/js/config.js`, the same that drives the site's own bar), in the middle of the top
  bar; nothing when the site announces none or the moment has passed.
- "Update to vX": when GitHub's newest release (the release list's first one, drafts left out;
  `releases/latest` skips pre-releases) is later than the launcher's version, a button that
  opens its page. Nothing is downloaded or installed (an updater is a later task).
- Continue (the newest save, with its picture, name, level and place: `--play --load`),
  Main menu (`--play --menu`), and More: New game (`--play --new-game`), the explorer
  (`--explore`) and "Close the launcher when the game starts" (`[Launcher] CloseOnPlay`).
  No `--data` is passed: the game reads its folder from the INI, so a launcher start is a
  player's start in every way (reports included, by the player's own choice).

## Fonts

The system's own interface font, read from its font folder at start and never shipped (Windows:
Segoe UI, Semibold for titles; Linux: Noto Sans or DejaVu Sans when installed), else egui's
built-in fonts. Their taller lines, a 16-point news body and wider paragraph spacing answer the
contributors' note that the news read "very squished together" (2026-11-15). egui has no
line-height setting for Markdown text, so the room comes from the font and its size.

## Rules it keeps

- Writes only NVForever's own files (the INI, `plugins.txt`), through
  `fnv_install::guard_write`; never the game folder or `Documents\My Games\FalloutNV`.
- Sends nothing but GET requests, and only while "Fetch news" is on (it is on at first, so the
  first start asks before the player has seen the switch): the site's `news.json` and
  `assets/js/config.js`, GitHub's release list (at most once a minute, shared by the news and
  the update check; GitHub allows 60 an hour without a sign-in), and each `https://` image a post names, on
  whatever host it names, side by side, at most 8 MiB and 15 s each (20 posts at most). Nothing fetched is
  saved but the news cache, and nothing is run.
- Opens only `https://` pages in the browser (`launch::open_url`): the news, release and
  update links come from the network, and the system's opener would also run a program, a
  `\\host\share` path or a `file:` link; only URL characters pass (no spaces, quotes,
  backslashes or commas). Links inside a post's text go the same way: each frame ends by taking
  them out of egui's output (eframe is built without its own link opener). Its own
  folders open through `launch::open_folder`.
- Test aid: `NVF_LAUNCHER_SHOT=<file.png>` saves the window once the news has arrived and
  closes; `NVF_LAUNCHER_PAGE=home|settings|mods|game` opens that page first.

## Not yet

- What kind a row is, its default and a list's choices come from the INI and a short table, not
  from the game. Better: the game writes its settings catalogue (kinds, steps' words, choices,
  defaults) to the home at each start, and the launcher reads it.
- Screen readers: eframe's `accesskit` brings zbus on Linux (unmaintained, RustSec), so it is off.
- No folder picker on Linux (rfd's portal backend brings zbus too): the folder is typed.
