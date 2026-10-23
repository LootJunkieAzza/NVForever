# Effects and lighting audit (2026-10-23)

Owner's request: the Strip's street lamp beams and Primm's wall lights end in a hard,
square edge; light beams, glows and smoke look artificial next to the original. "Check
literally everything", fix by rules that cover every model (no per-beam tweaks), and aim
for natural lighting rather than a 1:1 copy. Branch `claude/beam-blend`.

## How it was checked

- **Survey of every placed effect shape** (base game only; the add-ons stay out because
  `HonestHearts - Main.bsa` is a lab holdout): `crates/fnv-nif/examples/fx_audit.rs` over
  `Fallout - Meshes.bsa` and `Update.bsa` (22,105 shapes and particle systems that blend,
  alpha-test or use an effect shader), joined with `fnvtool world refs` of every
  worldspace and interior of `FalloutNV.esm` (7,499 placed models, 296,122 references).
  Effect groups among placed models: additive (316 shapes, 145 models, 7,624 references),
  unlit alpha-blended (1,129 shapes, 819 models, editor markers included), multiply
  (325 shapes, 314 models), unlit opaque (206 shapes), particle systems (1,262 in the
  archives).
- **Original-game frames** at 16 spots chosen where these cluster, at 22:00, from the
  owner's `nvfstrip` save: the Strip (2), Primm (3), Hoover Dam's lower level, the 2C
  golf resort, Jacobstown lodge, Vault 19, Lake Mead cave, HELIOS One tower, Vault 22,
  and Dead Money (4, owner's request). Private, in
  `C:\fnv-private\references\fx_audit\game`; the cameras in the session's scratch
  (`capture_cams.json` layout of `tools/capture/capture_frames.py`).
- **The same spots in nvforever** with `--play` (scripts switch lamps on at night).

### Capture problems met (for the next session)

- The installed nvf_capture's own screenshot step failed with "BitBlt failed" on every
  shot, although the game was windowed, in front and focused (the mine-grip session met
  it earlier the same day). Worked around by grabbing the game window's client area from
  the desktop (PIL `ImageGrab`) when the job reports its screenshot step, with 200 game
  loops of pause added after each step.
- The original crashed after 2-5 `coc` trips between interiors in one session; the
  interiors were captured three per launch. 2EOVault21, TOPSCasino, RocketLabBasement,
  OVWestSewers02 and Gomorrah00 crashed it before their picture and have no frame.
- NPCs shooting at the player blur the frame (hit blur): use `tdetect` for frame-only jobs.
- `--play` renders read the player's own `NVForever.ini` unless `NVFOREVER_HOME` points
  elsewhere (resolution, enhanced Bloom); and the player's heading in `--play` did not
  always match `--heading` (Dead Money frames face another way), so compare those by
  content, not pixel for pixel.

## Fixed on this branch

1. **Soft intersections** (nif.md §6): every unlit shape that adds or blends (beams,
   glows, smoke and dust sheets, particles) fades out over 160 units in front of the
   surface behind it, unless it lies along that surface. The Strip lamps' crossed beam
   panels no longer cut a "+" into the road.
2. **Light glow** (graphics-enhanced.md §3.11, world.md §6 "HDR and bloom"): HDR main
   view with Bevy's natural bloom, on in vanilla, strength from the image space's Bright
   Scale; the image space's HDR and bloom values are now read.
3. **Emissive multiplier of unlit shapes** (nif.md §6): in the HDR view unlit shapes are
   drawn at emissive colour × multiplier, so neon authored at 25 (Gomorrah) burns.
4. **Script-enabled references take their emittance colour** (world.md §8): the Strip's
   street lamp beams (enabled by script at night) drew white; they take the
   `stripStreetLight` colour now (owner: "we render beams white but the strip poles are
   more warm").

## Open, in order of how much they show

1. **Dead Money looks far darker and wrong-coloured** (owner: "most likely going to be an
   issue in ours"; frames `dm_*`). In the original the red sky and haze fill every
   street and buildings read clearly; in ours they are near-black silhouettes, and
   Puesta del Sol is greenish where the original is warm yellow.
   - Its weathers' fog: NVDLC01VillaWeather FNAM near 0, far 75,000, **power 0.35** day
     and night [observed]. Our fog is linear and ignores the power (world.md §6 "Fog
     power"), so there is next to no fog within a few thousand units; if the fog amount
     were (distance / far)^power, a wall 1,000 units away would be 22 % fogged, as the
     frames look. Needs a fog curve of our own in every world shader (Bevy's
     DistanceFog has none), checked against the Mojave frames (power 0.5-0.6 there).
   - Its image spaces and adapters (the yellow/red grades of the districts) need
     checking against the frames the same way NVDefaultExterior was (world.md §6).
2. **Dead Money's wall lantern looks far worse than the original's** (owner, frame
   `dm_westtown_beams`, "make a note that it needs looking at, don't skip it").
   `nvdlc01_sconceon.nif`: the glass (`SconceON:0`) is a lit shape with emissive
   (0.76, 0.55, 0.20) × multiplier 10, a cut-out cage, and an additive glow card
   (`fxsoftglowspot01.dds`, falloff); its light `NVDLC01ExtSconceKeyWT` has radius 200,
   colour (224, 255, 117), fade 2.5. In the original the glass burns warm white and the
   wall around it is washed warm yellow; ours is a dull lantern on a greenish wall. Check
   after item 1's grade: whether the lit-shape emissive × 10 reaches the HDR view as it
   should, and whether a 200-unit light should light that much wall.
3. **Interiors are darker than the original's** (Vault 19, Hoover Dam): the original's HDR
   eye adaptation (Eye Adapt Speed, Target LUM, LUM ramps) brightens dark rooms and its
   ceiling tubes glow harder. Not built; Bevy's auto exposure on the HDR view is the
   candidate, driven by those values.
4. **Lamp beams near the camera are too strong at night** (Strip; the original shows a
   faint haze). The additive power (alpha^p, `ADD_ALPHA_POWER`) is 1.5, fitted in the
   saloon's lit interior (nif.md §6 "Additive blend"); on a black night sky the
   original's display-value sum is alpha^2.2, so a faint beam is about 2.7 times too
   bright there. Power 2 was tried and left out: by the saloon fit's own numbers it would
   leave the window rays at about 0.4 grey levels against a target of 2.8. A fixed blend
   cannot serve both; candidates are a power that follows the brightness behind the
   shape (read from Bevy's copy of the opaque frame, which the refracting shapes and the
   water already use) or the light glow carrying more of the beams' look.
   **Done on branch `claude/additive-strength`:** additive shapes add colour × alpha in
   display values over the opaque frame behind them, with the emissive multiplier in
   display values (nif.md §6 "Additive blend"); the Strip's beams are now the faint haze,
   the Big MT's sky beams bright. Found on the way: the emissive multiplier of unlit
   shapes (item "Fixed" 3) never reached the shader (Bevy fills emissive light in only
   for lit materials); fixed there too. Left open: overlapping additive shapes sum short.
5. **Gomorrah's exterior neon** (`GomorrahLights` 0x0013BA74, `GomorrahLightsAlwaysOn`,
   persistent references in TheStripWorldNew) showed nothing in our Strip frames where the
   original has a bright orange tube along the roof line; whether they are drawn at all
   needs checking.
6. **Hoover Dam's work lamp beams** were off in our `--play` render (quest state of a new
   game); compare from a save where they are on.
