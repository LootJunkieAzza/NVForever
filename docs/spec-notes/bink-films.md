# Films from scripts (PlayBink) and cell image spaces (SetCellImageSpace)

Two functions of outstanding.md §1 row 17, built 2026-10-18 (branch claude/screen-video).
Code: `crates/fnv-game/src/functions/misc.rs` (`play_bink`, `set_cell_image_space`),
`functions/ui.rs` (`FilmRequest`, `film_path`), `engine/nvf-play/src/intro.rs` (the film
player), `engine/nvforever/src/world.rs` (`cell_image_space`).

## 1. The calls in the shipped plugins [observed: `fnvtool script decompile`, the ten masters]

PlayBink, 4 calls, all in FalloutNV.esm:

| Where | Call | What follows |
|---|---|---|
| `QUST 0x00102037` (VCG00) stage 0 | `PlayBink "fnvintro.bik" 1 1 0 1` | disables two references, sets stage 90 (the wake-up at Doc Mitchell's) |
| VCG00 stage 7 | `PlayBink "fnvlogo1.bik" 0 0 0 1` | (alone, after a SayTo) |
| VCG00 stage 43 | `PlayBink "fnvlogo2.bik" 0 0 0 1` | a 3 s timer |
| `SCPT 0x0015FF6F` (GameMode) | `PlayBink "vb01.bik" 0 1 0` | moves the player to `0x0015FF70`, stops quest `0x0015FF6E` |

The stages 7 to 50 of VCG00 are an in-engine version of the cemetery scene (Benny's
shots, the screen splatter of screen-splatter.md, the logo films); stage 0 jumps to 90
after the intro film, so a new game never reaches them [observed: the stage scripts].
The owner's `Data\Video` holds only `FNVIntro.bik` [observed 2026-10-18]; no archive holds
a `.bik` [observed: every BSA's listing]. So of the four calls only the intro film's can
play; the others name files that are not there. The "end videos" of the older notes do
not exist as PlayBink calls: the ending is slides (EndingSlides, already built).

SetCellImageSpace, 2 calls, both Old World Blues: `QUST 0x03002FCC` stage 50 sets the
Think Tank (`NVDLC03ThinkTank`, whose XCIM is `NVDLC03ThinkTankImageSpace`) to
`NVDLC03ThinkTankImageSpaceRed`; `SCPT 0x0300D6D2` sets it back while stage 50 is not
done or stage 100 is.

## 2. PlayBink [documented: GECK wiki PlayBink, owner export]

`PlayBink fileName [interruptable=0] [muteAudio=1] [pauseMusic=1] [letterboxed=1]`; the
path is relative to `Data\Video\` (`..\` leaves it), the file must be loose (not in a
BSA), Esc ends an interruptible one, muteAudio silences the game's world sounds,
pauseMusic pauses its music, letterboxed 0 does not letterbox a wide film on a 4:3
screen, and playing a film "appears to stop game time".

Here:

- The call queues the film for the engine (`UiRequests::films`) and the script goes on
  [hypothesis: whether the original's call returns before or after the film is not
  documented; with the game held during the film the scripts' order is the same].
- The film starts once the player is in the world with nothing held (no menu, loading
  screen or other film), full screen over everything with its sound; the game is held
  as under the pause menu (scripts, clock, world sounds); afterwards the game goes on.
- letterboxed: fitted inside the window with black bars; 0: stretched over the window
  [hypothesis: what the original does without letterboxing on a wide screen].
- muteAudio: the game is held under every film [documented: the page, a film "appears
  to stop game time, even if the flags are set to keep ingame noises going"]; with 1 (the
  default) the world's sounds are held with it, with 0 they go on under the film [documented:
  the page, "the game's world sounds will continue to play during the Bink movie"]. Here the
  world's sounds are the placed and background loops and random sounds (`ambient.rs`) and
  the world's radios (`radio.rs`): under a `muteAudio 0` film they are not paused and new
  random sounds are still rolled, by the wall clock, though the world stands still
  [hypothesis: which of the original's sounds count as world sounds is not documented].
  Lines being said stay held with the scripts that time them, else a line's sound and its
  timer would part [hypothesis]. Of the shipped calls only the opening's logo films and
  `vb01.bik` pass 0, and none of those files is in the owner's install (§1), so the
  shipped game never shows it here [observed: §1].
- A file that is not there is skipped at once (log `film: <path>: not a loose file …; the
  game goes on`) [hypothesis: the original's behaviour with a missing file is not known;
  capture idea in owner-review].
- **The intro film twice.** The main menu's New Game plays `FNVIntro.bik` before the
  opening (owner's choice, 2026-10-15), and the opening's stage 0 asks for it again. A
  scripted request for the intro film is not played once the main menu showed it in
  this run [decision]. Started without the main menu (`--play`, a new game), the
  opening's call plays it, as the original does.
- **Unattended runs** (any `NVF_PLAY_TEST_*` set) do not play scripted films unless
  `NVF_PLAY_TEST_FILMS=1` [decision: otherwise every smoke run of a new game would be
  held for the intro's three minutes].

## 3. Log and test aids

`engine/nvf-play/src/intro.rs` module notes: `film: playing <path> (<w>x<h>, <n> frames,
PlayBink[, Esc ends it][, world sounds go on][, music paused], letterboxed|stretched)`, `film: skipped at`,
`film: ended after`, `film: the game goes on`; `NVF_PLAY_TEST_INTRO_SKIP` /
`NVF_PLAY_TEST_INTRO_SHOT` act on every film (`film.png` for a scripted one). `engine/nvf-play/src/ambient.rs`: `ambient: world sounds held (<why>, <n> loops)`, `ambient:
world sounds go on (<n> loops)` when that changes (why: loading, the poker table, a film,
PlayBink muteAudio 1, the game is held), and `ambient: world sounds go on under the film
(PlayBink muteAudio 0, <n> loops)`. Smoke: `film-opening`, `film-console` (muteAudio 1
holds the world's sounds), `film-world-sounds` (muteAudio 0 in Goodsprings: its loops go on
under the film), `main-menu` (the opening's call is not played twice).

## 4. SetCellImageSpace [documented: GECK wiki SetCellImageSpace]

`SetCellImageSpace cell imageSpace` changes the cell's image space. The page's note: on a
loaded cell the new image space is not shown until the cell is loaded again (left and
entered again, or a save made in it loaded); script extenders later changed that.

Here: the change is kept in `FormChanges::cell_image_spaces` (so in the save), and the
engine grades an interior with it whenever the cell loads (every travel into it, and
when a loaded save's scripts are ready), not at the call, as the page says the original
does. Exterior cells take their grade from the worldspace and weather (world.md §6); a
call on one is kept but not drawn [hypothesis: no shipped call names an exterior]. Smoke
`cell-image-space`.
