# Loading screens in `--play`

Owner request 2026-10-10: when the player changes cell through a door, pause the game and
show a loading screen while the destination loads; everything is on hold until loading
is done; the picture is one of the game's loading-screen textures, as the plugins list
them. Records and what is known about them: docs/spec-notes/loading-screens.md.

Code: `engine/nvf-play/src/loading.rs` (state, screen), `crates/fnv-world/src/loadscreen.rs`
(records, choice), `engine/nvforever/src/world.rs` (`loading_gate`, faster streaming
behind the screen).

## 1. What happens

1. A door (activation) or a script moving the player sends `TravelRequest` instead of
   `Travel`.
2. The same frame the loading screen is spawned, `PlayState` goes to `Loading` and Bevy's
   virtual clock is paused.
3. Two frames later (`SHOW_FRAMES`), when the screen has been drawn, the request goes on
   as `Travel`. Physics rebuilds its cells and places the player; the renderer drops its
   cells and streams the destination's. These are the long frames, and they now pass
   behind the picture instead of in front of a frozen view of the old place.
4. Behind the screen, streaming is faster than in play: the renderer loads cells for
   200 ms each frame instead of one cell a frame, physics takes up to 8 prepared cells a
   frame instead of 1.
5. The screen comes down when physics has every cell it asked for around the player
   (`PhysState::loading_done`) and the renderer has all the cells around the camera
   (`Streamer::settled`, reported through `Loading::render_busy`), and that has been so
   for 0.25 s and 3 frames (`SETTLE_SECS`, `SETTLE_FRAMES`: time for textures and meshes
   to reach the graphics card). `PlayState` returns to `Playing`, the clock runs again.
6. If loading has not finished after 120 s (`TIMEOUT_SECS`) the screen comes down with a
   warning, so an unattended run cannot hang there.

## 2. What is on hold

- Everything that runs only in `PlayState::Playing`: the player's input, movement and
  physics steps, activation, combat, the game clock, opening menus and the pause menu.
- Scripts: `run_scripts` returns while a loading screen is up, as it does under the
  pause menu. Loading-screen time does not count for scripts in the original either
  [observed: layer5-captures.md §8.3 e]. The two frozen frames after a change of place
  that stood in for a loading screen remain (they now follow the real one).
- Everything driven by `Time`: animations, doors, travelling NPCs, the sky's clock,
  on-screen messages, the `NVF_PLAY_TEST_*` aids and `--run-seconds` (a run lasts its
  seconds of game, plus the loading).
- Not on hold: streaming (render and physics), distant LOD generation, and a voice line
  already playing (as under the pause menu; voice.rs).

## 3. The screen

- The record: `fnv_world::loadscreen::choose` over every LSCR of the load order. Screens
  that belong to the destination (its interior cell, or a grid square within 2 of the
  arrival square in its worldspace) and screens for anywhere are both allowed; when the
  destination has its own, one of them is taken every other time. Both rules are
  hypotheses (spec note §3).
- The picture: the record's ICON, read through the texture cache like any other
  texture. Scaled to cover the window without changing its shape; what does not fit is
  cropped evenly (`loading::cover`). A record without a picture, or one whose texture
  is missing, gives a black screen with the text.
- The text: the record's DESC in a band along the bottom, with "Loading...".
- The layout is NVForever's own. The original's roulette wheel, level progress and text
  box (LSCT) are not drawn.

## 4. Scope and open points

- Only travel shows a loading screen. The start of a run still streams in view, as
  before; a loading screen there would be the same code with the start as the trip
  (owner's call).
- Travel without `--play` (the free camera following `--npc-travel` NPCs) is unchanged:
  no loading screen.
- The game's rules for choosing a screen and its layout are open (spec note §3, §4).
- Test aid: `NVF_PLAY_TEST_LOADING_SHOT=<png>` saves the window when the travel goes out
  (the screen is up); later screens of the run go to `<name>-2.png`, and so on.

## 5. Checks

- `fnv-world`: `loadscreen::tests` (locations, choice).
- `nvf-play`: `loading::tests` (cover, destination, and the sequence: screen up and
  clock paused before the travel goes out, held while the renderer is busy, down and
  playing again after).
- In the game (owner's data): Doc Mitchell's front door, both ways; see the pull request.
