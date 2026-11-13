# Photo mode

Owner request, 2026-11-09 (overnight queue item 175, with its correction at 23:43): a
simple first photo mode, modelled on how Fallout 76's feels and is laid out (none of its
art). The code is `engine/nvf-play/src/photo_mode.rs`; its module notes are the full
description. This page is the summary.

## What it does

- **Open / close**: Photo Mode (P, rebindable on the Controls tab) or the pause menu's
  Photo Mode entry, which is how a controller opens it: every controller button already
  has a control. Esc, P, or B on a controller close it. The view goes back to the player
  as it was (first or third person, the field of view).
- **The view**: the HUD, the crosshair, quest lines and every other piece of the interface
  hide.
- **The camera** (owner, 2026-11-10: "camera shouldnt be locked to player"): **Free** by
  default. It looks with the mouse and flies anywhere up to 4000 units (about 57 m) from
  the player. That keeps it well inside the cells loaded round the player [design]. Walls
  stop it, and it slides along them. **Orbit** is the first version's camera: it circles
  a point that can move up to 200 units from the player's pivot, 30 to 320 units off.
- **The panel** (right side, in the HUD font and colour over a soft dark backdrop) has
  three tabs (owner, 2026-11-10: "add a few more options commonly found in photo modes"):
  - **Camera**: Camera (Free or Orbit), Zoom (field of view, 10-120 degrees), Roll (-45 to
    45 degrees), and the depth of field: Focus (Auto, which focuses on what is under the
    screen's centre, or a distance up to 8000 units on a curve), Blur (Off to 100%).
  - **Look**: Brightness, Contrast, Saturation, Warmth (-100 cool to +100 warm), Vignette,
    Film Grain.
  - **Frame**: Letterbox (Off, 2.39:1, 1.85:1, 16:9, 4:3, 1:1; the bars are in the photo),
    Grid (rule of thirds; never in the photo), Hide Player (the body goes; its shadow
    stays).

  Everything is drawn by nvforever's image-space grade, so the photo is graded as the
  screen is. Brightness, contrast and saturation multiply the grade. Warmth, vignette,
  grain and the letterbox are a pass at the end of the grade shader. The depth of field
  uses the game's own depth-of-field blur. Motion blur is off while photo mode is open.
  The panel can be hidden for a clean view. Every option resets when photo mode closes.
  Focus distances show in metres at 1.428 cm a unit [hypothesis: the Creation Kit wiki's
  figure for Bethesda's games of this engine family].
- **The photo**: the panel and every piece of the interface are hidden for one frame, and
  the window is captured at full size. The PNG goes to `<NVForever home>\Photos`,
  for example `NVForever 2026-11-10 01-23-45 Goodsprings.png`: the local date and time,
  then the place as the Pip-Boy names it (an interior's name, or outdoors the region's
  map name). Two photos in one second get " (2)". The file is written on its own thread.
  "Photo saved" (or "Photo not saved") is the corner message.

## Solo and co-op

- **Solo**: the game pauses as under a menu (`PlayState::Menu`), and so does the picture
  (the virtual clock, as `tfc 1` does).
- **Co-op**: nothing pauses. The world, NPCs, combat, the other players and the clock go
  on in every game, and enemies can still attack the player taking the photo. Only this
  player's view holds still: each frame, after transforms are propagated, everything that
  moved is drawn where it was when photo mode opened, and the live transform is put back
  at the start of the next frame. So the simulation, the network and the physics only ever
  see the live world. Cameras, the interface and anything spawned later are drawn live.
  Shader time (water, sky), particles and face morphs are not held [known limit].
  A hit on the player shows a warning at the top of the panel for 4 s, and a fight keeps
  a warning there while it lasts. Photo mode stays open.

## Controls [design]

| | Keyboard and mouse | Controller |
|---|---|---|
| Look (Orbit: circle) | mouse | right stick |
| Forward / back (Orbit: nearer / farther) | W / S | left stick up / down |
| Sideways | A / D | left stick left / right |
| Up / down | Space or E / Ctrl or Q | RT / LT |
| Zoom | mouse wheel | the Zoom slider |
| Change the tab | Tab / Shift+Tab | RB / LB |
| Choose / change an option | Up, Down / Left, Right (a slider held repeats) | D-pad |
| Take the photo | Enter or left click | A |
| Hide the panel | H | Y |
| Reset the options | R | X |
| Close | Esc or P | B (Start) |

Shift moves the camera three times faster.

## Not in this version

Frames, filters, poses, time of day, and a co-op mark for players in photo mode.

## Tests

Unit tests in `photo_mode.rs` cover the options (defaults, range, steps, choices going
round, reset, text, tabs, what reaches the grade), the
FOV conversion, the file name and the place's safe form, unique names, the capture's pixel
order, PNG writing, the test aid and solo/co-op. The smoke scenario `photo-mode`
(`NVF_PLAY_TEST_PHOTO`) opens it alone, moves the camera, sets two sliders, saves a photo
and closes it; `photo-mode-options` flies the free camera and turns the new options on.
