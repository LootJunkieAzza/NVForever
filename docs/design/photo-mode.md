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
- **The zoom is a layer on top** (#202, owner playtest of ship18 d2b501ce6: the view
  stayed narrow after photo mode). Each frame photo mode takes its zoom off the camera
  before the game's own writers of the field of view run (the sights' zoom, a
  conversation's zoom), and lays it on again after them. Those writers take any field of
  view they did not write for the unzoomed view, so before this they took photo mode's
  zoom as the gameplay view [observed: log 030136, a conversation's zoom eased out to
  vertical 22.7° under photo mode, and the close put back that zoomed angle]. The close
  leaves what the game's writers have on the camera: the view it opened on, or the
  gameplay view if a conversation ended meanwhile, or a field of view the settings set
  while open. The sights' zoom holds where it was while photo mode is open, so a view
  down the sights comes back down the sights. Photo mode never writes the field-of-view
  setting.
- **The view**: the HUD, the crosshair, quest lines and every other piece of the interface
  hide.
- **The camera** (owner, 2026-11-10: "camera shouldnt be locked to player"): **Free** by
  default. It looks with the mouse and flies anywhere up to 4000 units (about 57 m) from
  the player. That keeps it well inside the cells loaded round the player [design]. Walls
  stop it, and it slides along them. **Orbit** is the first version's camera: it circles
  a point that can move up to 200 units from the player's pivot, 30 to 320 units off.
- **The panel** (right side, in the HUD font and colour over a soft dark backdrop as dark as the game's menus over the world, 0.93) has
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
  is a lens blur of its own (owner, 2026-11-14: bokeh; `engine/nvforever/src/bokeh.rs`):
  each point is drawn as a disc as large as its circle of confusion through a thin lens
  focused at Focus and opened to Blur, near-white points spread as round bright discs,
  and a blurred background never spreads over a sharp subject in front of it. A blurred
  foreground spreads soft over what is behind it, as a layer laid over by how much of
  each point its discs cover, and in front of the focus a disc grows up to three times
  the largest one behind it, so a wall by the lens melts wide. It runs only while photo mode is open with
  Blur above Off; gameplay's depth of field (VATS, conversations, the modifiers) is the
  game's Gaussian one as before. Motion blur is off while photo mode is open.
  The panel can be hidden for a clean view. Every option resets when photo mode closes.
  Focus distances show in metres at 1.428 cm a unit [hypothesis: the Creation Kit wiki's
  figure for Bethesda's games of this engine family].
- **The photo**: the panel and every piece of the interface are hidden for one frame, and
  the window is captured at full size. The PNG goes to `<NVForever home>\Photos`,
  for example `NVForever 2026-11-10 01-23-45 Goodsprings.png`: the local date and time,
  then the place as the Pip-Boy names it (an interior's name, or outdoors the region's
  map name). Two photos in one second get " (2)". The file is written on its own thread.
  "Photo saved" (or "Photo not saved") is photo mode's own notice, low in the middle of
  the view for 2.5 s. The corner messages (items added, quests) wait, hidden and with
  their clocks stopped, until photo mode closes, as behind a menu.

## Solo and co-op

- **Solo**: the game pauses as under a menu (`PlayState::Menu`), and so does the picture
  (the virtual clock, as `tfc 1` does).
- **Co-op**: nothing pauses. The world, NPCs, combat, the other players and the clock go
  on in every game, and enemies can still attack the player taking the photo. Only this
  player's view holds still: each frame, after transforms are propagated, everything that
  moved is drawn where it was when photo mode opened, and the live transform is put back
  at the start of the next frame. So the simulation, the network and the physics only ever
  see the live world. Anything spawned later is held where it is first drawn; the cameras
  and the interface are drawn live. The rest of the picture holds too (owner, 2026-11-14:
  "a true frozen snapshot you fly around in"; `engine/nvforever/src/view_hold.rs`): the
  shaders' clock stands at the moment photo mode opened (water, grass and tree wind,
  fire), the sky, clouds, stars and moon stay as they were while following the camera
  (the hour and the weather go on underneath), and what only moves the picture waits:
  particles, rain and snow, faces and lip sync, vertex morphs, light flicker, material
  clips and ripples (`nvf_play::photo_mode::view_live`). On close the view goes live at
  once. Known limits: the colour grade's own weather and hour changes, and the screen's
  per-frame noise, keep running. The test aid's `open-coop` opens it this way in one
  window (smoke `photo-mode-coop-hold`).
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

## Lights (owner request 2026-11-19)

The **Lights** tab places up to 4 coloured lights for the shot. The mechanics follow GTA VI's
photo mode lights [design: the owner named it as the model; nothing of its art or assets is
used]. The settings and state are in `nvf-play/src/photo_lights.rs`; nvforever draws them as
the engine's dynamic point and spot lights, with the game's attenuation
(`nvforever/src/photo_lights.rs`, `lights.rs`).

- **Rows** (they scroll, six at a time): Light (which one the rows set), Add Light, Move &
  Aim, On, Type (Point or Spot), Hue (0-360), Saturation, Brightness, Range, Width (the
  spot's full cone, 6° tight spot to 170° flood), Direction and Tilt (heading and pitch),
  Shadows, Pin to Camera, Remove Light. Left/Right change a row or press an action.
- **Colour**: the Hue slider goes round the colour circle and Saturation mixes it to white
  (HSV with the value at full). There is no colour wheel (owner).
- **Placing**: Add Light drops a new light where the camera is, aimed along the view, and
  starts editing it. Nothing points a light at the camera on its own afterwards; the
  player sets its direction.
- **Editing** (Move & Aim, or a new light): the photo camera stands still, and its inputs
  drive the light instead. The mouse or right stick aims it, WASD or the left stick move
  it (level with the ground along the camera's heading), Space/E and Ctrl/Q or the
  triggers raise and lower it, and the wheel, [ ] or LB/RB narrow and widen it. Esc, B or
  the right mouse button (or Move & Aim again) end editing. The camera is then put back
  exactly where it stood when editing began, from a kept copy of its place and view.
- **Pin to Camera** is an option, off by default. A pinned light rides at the camera, and
  its direction is kept relative to the view. Unpinning sets it down at the camera,
  pointing where it pointed.
- **Markers**: a ball in the light's colour and an arrow along its beam for every light,
  and the cone's rim for the chosen spot. They are drawn through walls while the Lights tab
  is shown or a light is edited, and never in the photo.
- **Shadows** use Bevy's shadow maps (a cube map per point light, one map per spot), and
  carry the sun's render layers so the player's body casts its shadow.
- The lights exist only while photo mode is open. They are removed when it closes and are
  never saved.

## Not in this version

Frames, filters, poses, time of day, and a co-op mark for players in photo mode.

## Tests

Unit tests in `photo_mode.rs` cover the options (defaults, range, steps, choices going
round, reset, text, tabs, what reaches the grade), the
FOV conversion, the file name and the place's safe form, unique names, the capture's pixel
order, PNG writing, the test aid and solo/co-op. The smoke scenario `photo-mode`
(`NVF_PLAY_TEST_PHOTO`) opens it alone, moves the camera, sets two sliders, saves a photo
and closes it; `photo-mode-options` flies the free camera and turns the new options on;
`photo-mode-lights` adds, colours, moves, aims and widens two lights with the camera held,
saves a photo and checks the lights go on close. Unit tests in `nvf-play/src/photo_lights.rs`
cover hue/saturation to colour, ranges, the light limit, input routing while editing, the
camera put back, pinning and clearing.
