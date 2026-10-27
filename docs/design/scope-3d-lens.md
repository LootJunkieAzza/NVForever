# Design: a 3D scope (the zoomed world on the gun's lens)

Feasibility study, 2026-10-26; a test version is built behind a switch (§7). The scope as it stands (the MOD3 picture drawn
full-screen with a black border, night vision, sway) is in
[weapon-scopes.md](../spec-notes/weapon-scopes.md); the aim zoom in
`nvf_play::aim_zoom`, the picture in `nvforever/src/scope_view.rs`, true iron sights in
`nvforever/src/iron_sights.rs`.

**The look asked about** [observed: the owner's reference picture, 2026-10-26, a mod for
the original game]: aiming a scoped rifle keeps the first-person gun on screen; the scope's
body is drawn around a round lens; inside the lens the world is magnified, with a red
mil-dot reticle; around the scope the world stays unzoomed.

**The original game** draws the 2D overlay and hides the gun [observed: weapon-scopes.md
§2-3, the smoke frame of 2026-10-21; documented: GECK wiki "Weapons", MOD3 "the overlay
effect used in iron sights mode"]. So a 3D scope is a look NVForever would add, not a
parity fix.

## 1. Answer

**Partly yes.** It can be built for every scoped weapon with the game's own assets, but
only a few first-person models carry a lens shape to draw on. For the rest the lens has to
be placed by NVForever, from a short table of measured eyepieces (or a fallback disc).
The reticle can come from the MOD3 models as they are. The engine side is a second
camera rendering the world into a texture (the water reflection already does this), a
lens material, and a new gun pose for scoped aim. Cost: an extra world render while the
scope is up (§5). It is a look the original does not have, so it should be a setting.
The default is the owner's call (§6).

## 2. Lens survey (question 1)

How it was made [observed, 2026-10-26]: every WEAP record of the ten masters that names a
MOD3 (57 records; the 53 with Has Scope of weapon-scopes.md §1 among them) was read with a
small ESM reader. For each record the survey took its first-person model (WNAM's STAT,
else MODL) and, when one of WMI1-3 is a scope mod, the matching model of each mod
combination (WNMn/MWDn, n = 1, 2, 1+2, 3, 1+3, 2+3, all). The object names were then
taken from `fnvtool nif scene` over the base and add-on archives. Shapes were measured
with `fnvtool nif decode`.

| Model (first person) | Weapons | Lens shape | Notes |
|---|---|---|---|
| `2handrifle\sniperrifle.nif`, `sniperrifleunique.nif`, `sniperriflesuppressor.nif` | Sniper Rifle, Gobi Campaign Scout Rifle, Christine's CoS Silencer Rifle | `##ScopeLens:0` | Both glass caps in one shape (below) |
| `2handrifle\trailcarbinescope.nif` | Trail Carbine with its scope mod | `Lens:0` | Both caps in one shape; the tube is `Scope:0` |
| `nvdlc03\...\nvdlc03cyberdoggun.nif`, `nvdlc03fidogun.nif` | K9000 Cyberdog Gun, Fido | `##K9000Lens:0` | Not measured |
| `nvdlc04\...\nvdlc04redglare.nif` | Red Glare | `RG_eyeglass` | Not measured; it also has scope doors |
| `1handpistol\44magnumrevolverheavyframe.nif` | .44 Magnum with scope + heavy frame | node `Lens2` only | No shape named for a lens |
| `2handrifle\huntingriflescope.nif` (+ `...scopeextmag`, `...scopecustaction`, `huntingrifleallmods`) | Hunting Rifle with scope, Boone's rifle (`WeapBooneSniperRifle`) | none | `HuntingRifleScope*:0` tube only |
| `2handrifle\varmintriflescp*.nif`, `varmintrifleunique.nif` | Varmint Rifle with the night scope, Ratslayer | none | `VarmintScope:0` tube only |
| `1handpistol\9mmscp.nif`, `9mmextclipscp.nif` | 9mm Pistol with scope | none | `Scope:0` tube and mount |
| `2handrifle\laserriflescope*.nif` | Laser Rifle with scope | none | `Scope:0`, or no scope shape at all in the Focus Optics models |
| `nvdlc05\...\plasmadefender*.nif` | Plasma Defender (GRA) with its scope mod | none | `Scope` shape |
| `2handrifle\antimaterialrifle.nif`, `2handautomatic\marksman.nif`, `marksmanaa.nif`, `2handrifle\gaussrifle.nif`, `1stpersongaussrifleunique.nif`, `cowboyrepeaterunique.nif`, `1handpistol\huntingrevolver.nif`, `thatgun.nif`, `nvdlc05\...\556thatgununique.nif`, `nvdlc05\...\nvdlc05bozarmachinegun.nif`, `nvdlc05\...\nvdlc051stpersonweapuniquehuntingrifle.nif`, `laserrifle.nif`, `laserrifleunique.nif`, `nvdlc01weapholorifle.nif` | Anti-Materiel Rifle, Marksman Carbine, Gauss Rifle, Medicine Stick, Hunting Revolver, That Gun, Bozar, Paciencia, ... | none | The scope is part of the gun's one shape, or the model has no scope at all |
| `1handmelee\binoculars.nif`, `1handpistol\camera.nif` | Binoculars, the camera | none | |

**The sniper rifle's lens** [observed: `##ScopeLens:0` in `sniperrifle.nif`, 144
vertices]: the shape holds the two glass surfaces of the scope, not one lens. Along the
gun's axis (+X) it has rings at x = 8.3-8.7 in the shape's frame (radius up to 1.38 units,
domed: radius 0.3-0.5 at 8.3, 1.37 at 8.7) and at x = 28.4-28.6 (radius 1.58 to 0.25).
Both are centred at (y 5.61, z 0) in the shape's frame. The shape is moved by
(-9.70, 6.23, 0.01). So the **eyepiece** is the cap with the lower x: model
x = -1.4 to -1.0, height 11.84, radius 1.38. The objective is at the front, radius 1.58.
It uses the gun's own textures (`SniperRifle.dds`, environment map `ShinyBright_e.dds`).
[hypothesis] That makes it a glass sheen on the gun's texture, not a picture.

**The trail carbine's lens** [observed: `Lens:0` in `trailcarbinescope.nif`, 128
vertices]: also two domed caps, at x = -6.6 to -6.1 (radius up to 0.87) and
x = 6.7 to 7.1 (0.93), under node `Lens` at (8.86, 7.39, 0). Its texture is the scope's
(`TrailCarbineScope.dds`).

**Conclusion** [observed: the table]: in vanilla content only the sniper-rifle family and
the trail carbine scope give a lens surface to draw on, and both name it differently
(`##ScopeLens`, `Lens`). The K9000 and Red Glare have lens-named shapes too, not
measured here. Everything else (hunting rifle, varmint rifle/Ratslayer, anti-materiel
rifle, marksman carbine, gauss rifle, 9mm, laser rifle, binoculars) has either a scope
tube shape with no lens, or a scope merged into the gun's single shape. None of these
models has a named eyepiece node either. `##SightingNode` (iron_sights.rs) is present on
the varmint rifle and 9mm scoped models, but not on the sniper rifle, hunting rifle
scope, anti-materiel rifle or marksman carbine models.

So **a lens disc made by NVForever** is needed for most weapons. Two ways [design]:

- **Measured eyepieces**: a small data table in the engine source with, per first-person
  model, the eyepiece's centre, axis and radius in model units. The numbers would come
  from measuring each model's scope tube, as the sniper rifle's were measured above (the
  ring of vertices at the tube's rear end). These are measurements, like the extents in
  weapon-scopes.md §3; no game data ships. About 20 models.
- **Derived**: where a model has a lens-named shape (`##ScopeLens`, `Lens`, `##K9000Lens`)
  the eyepiece is taken from it at load: its rearmost ring along the barrel's axis
  (`ProjectileNode`'s direction). Everything else falls back to the table, and a model in
  neither gets a disc at the view's centre (§4.2).

## 3. The reticle from the MOD3 models (question 2)

**Yes.** [observed: `fnvtool nif scene` names; weapon-scopes.md §3 layout] The eight
vanilla scope pictures (`mildotscope`, `varmitscope`, `duplexscope`, `germanscope`,
`biggamescope`, `laserscope`, `scope3`, `binoculars`) all have the same four shapes.
`keepAdditive03:0` is the reticle (±372 × ±348, texture `textures\weapons\scope\*_o.dds`,
alpha tested). `keepScope01:0` is the black mask (a ring from radius ~361 out to the
1480 × 920 rectangle). `TintRing:0` is a faint ring and `Additive01:5` the glare.

- The reticle alone is the picture **without `keepScope01:0`**, cut to a circle of radius
  ~361 units, the mask's inner edge. `scope_view.rs` already draws the MOD3 model into an
  image with its own orthographic camera. Hiding the mask shape by name, and making that
  image square (722 × 722 units around the middle) instead of 920 high, gives a reticle
  image that maps onto the lens disc as it is. `TintRing` and the glare can stay; they sit
  inside the circle.
- **The other layouts** [observed: names only, not measured]: `scope01.nif` (Boone's
  rifle, the Van Graff laser rifle, That Gun) and the Gauss scope
  (`dlcanchgaussscope01.nif`) have `MaskRing:4`, `Inner Lens Shading:3`, a reticle
  shape (`crosshair_new:0`, `Reticule:2`) and a body shape (`Scope01:0`,
  `DLCAnchGaussScope01:0`). The K9000 scopes (`nvdlc03circletechscope*.nif`) have
  `MaskRing01:4`, a filter and an edge; Red Glare's has `SuperRedFilter:0` and `Scope:0`.
  [hypothesis] There the mask is `MaskRing*` and/or the body shape; which shapes to hide
  must be checked per file before those weapons get a 3D lens. Until then they keep the 2D
  overlay.
- The reticle's colour is the texture's own. The red mil-dots in the reference picture
  are the mod's; vanilla's mil-dot reticle is drawn as the texture is (smoke frame of
  2026-10-21: black on the world) [observed: weapon-scopes.md §6].

## 4. Engine design (question 3)

### 4.1 Rendering: a scope camera into a texture, drawn on the lens

- **A second world camera** [design], like `reflection.rs`. It has the main camera's
  render layers minus the arms and body (`sky::ARMS_LAYER`, `sky::BODY_LAYER`), and it
  renders into an image of its own (`RenderTarget::Image`, as `scope_view.rs` and the
  water do). It sits at the eyepiece and looks along the scope's axis, so the world in
  the lens moves when the gun moves (sway, recoil). It needs the main camera's look
  components too: `DistanceFog` (or the `#ifdef DISTANCE_FOG` guard of
  claude/scope-picture-fog, without which a fog-less camera draws nothing),
  tonemapping, exposure and the image-space modifiers.
- **Its field of view** [design]: today's magnification is kept. The full-screen zoom
  magnifies by M = tan(base/2) / tan(sight/2), base being the unzoomed vertical FOV and
  sight the weapon's Sight FOV (`aim_zoom::zoomed_fov`). The lens covers an angle 2a of
  the main view, where a = atan(eyepiece radius / eye distance). The scope camera's
  vertical FOV is then 2·atan(tan(a) / M). With it, a distant target looks the same size
  through the lens as through today's overlay.
- **The image's size** [design]: square, about the lens's size on screen (the lens's
  angular diameter over the vertical FOV, times the window height). That is around
  0.5 of the window height (§4.3), e.g. 540 px at 1080p. No more pixels than the lens
  shows are rendered.
- **The lens material** [design]: an unlit material on the lens shape (or the made disc).
  It shows the scope image by the lens's own planar coordinates (the disc's centre is the
  image's centre), lays the reticle image (§3) over it, and darkens the edge into a
  vignette. Outside the circle of the eyepiece's radius it is clear, so the gun's own
  glass cap still shows there. On the sniper rifle the eyepiece cap of `##ScopeLens:0`
  takes this material. The objective cap and the other shapes keep theirs.
- **The main view** stays unzoomed [design], or zooms a little (e.g. a quarter of the
  way to the Sight FOV) so the gun seems to come to the eye. `AimZoom::blend` still runs.
  It sets the scope camera's FOV instead of the main camera's (a setting would choose,
  §6). The gun keeps being drawn on `ARMS_LAYER`, by the main camera.
- **Rejected: a screen-space circle** [design]. Drawing the zoomed render in a circle at
  the screen's middle, with a stencil, is simpler. But it does not follow the gun (sway,
  recoil), and the circle would float in front of models whose scope is off-centre.
  Kept only as the fallback for models with no lens at all (§2): the disc is then placed
  on the view axis, eye-distance in front of the camera.
- **Rejected: magnifying the main frame** [design]. Showing a crop of the already-drawn
  frame in the lens costs nothing. But the vanilla Sight FOVs zoom ~2-4×, so the lens
  would show the frame's pixels 2-4× too big: a blur, and no more detail than without a
  scope.

### 4.2 The gun's pose (true iron sights)

- `iron_sights.rs` turns and moves `Bip01 Translate` so `##SightingNode` lands on the
  eye. For a 3D scope the point is instead **the eyepiece's centre, moved back along the
  scope's axis by the eye relief** [design]. The eye relief is a constant chosen so the
  lens fills about half the screen's height. A 75° setting is ~60° vertical
  (`vertical_fov`, camera.md §3), so a lens half the height high has tan a = tan 30° / 2,
  a ≈ 16°; with the sniper rifle's radius of 1.38 that is 1.38 / 0.289 ≈ 4.8 units
  [hypothesis: arithmetic, no frame measured].
- Models without a sighting node (the sniper rifle, the anti-materiel rifle, ...) then
  get a pose from the lens too. Today they keep the shipped aim clip (iron_sights.rs,
  "Which weapons").
- The scope's mount sits between the eye and the lens on some models. Whether the near
  plane or the mount cuts into the lens must be looked at per model [hypothesis].

### 4.3 What else changes

- **The crosshair** stays hidden while the scope is up, as now (the reticle is the aim).
- **Arms and gun**: today hidden while the scope is up (weapon-scopes.md §2). In 3D mode
  they stay drawn; the overlay picture (`scope_view::show`) is not shown.
- **Sway** (weapon-scopes.md §5): today `scopewobble.nif`'s curve turns the view. In 3D
  mode it turns the gun (the same node the view sway moves, `view_sway.rs`). The scope
  camera follows the lens, so the reticle and the zoomed world drift together in the
  lens while the unzoomed world around stays still. The shots follow the scope's axis
  (they already follow the swayed view).
- **Night vision** (weapon-scopes.md §4): `NightScopeISFX` is now laid over the whole
  screen. In 3D mode it belongs on the scope camera only (green in the lens, normal
  around it). That needs the image-space modifiers to work per camera; how
  `imagespace.rs` attaches them was not checked [hypothesis: the main camera only].
- **LOD**: distant objects drawn through the lens use the LOD the main camera picks by
  distance (`lod.rs`), the same as through today's zoom. Nothing new.
- **Third person**: unchanged (no scope picture; the view zooms).
- **Binoculars and the camera** (scoped "weapons" with no barrel to look down): keep the
  2D overlay [design].

## 5. Cost

- **Frame time** [hypothesis: not measured]: a second world view while the scope is up.
  In Bevy every camera prepares, culls and encodes its own draws. With the narrow FOV
  (a few degrees) culling leaves only what is in the lens, and the image is about a
  quarter of the screen's pixels. So the extra cost is mostly CPU-side preparation per
  view plus the sun's shadow cascades, which Bevy makes per view (the reflection camera
  gets its own, graphics-enhanced.md §5.3; the sun shadow cost of the main view,
  sun-shadow work 2026-10-23). Turning shadows off for the scope camera (or giving it a
  single cascade) is the main saving. A recorded smoke frame time with the scope up, on
  the Strip and in the open desert, would settle it. Today's overlay costs nothing extra
  (the world is drawn once).
- **Build** [design]: a scope camera and its image (~150 lines, from `reflection.rs` and
  `scope_view.rs`), a lens material (WGSL, ~60 lines), the lens table and lens finding
  (~100 lines + ~20 measured rows), the scoped pose in `iron_sights.rs`, sway on the gun
  instead of the view, night vision on the scope camera, a setting, and smoke scenarios
  (`scope-3d-lens`: the sniper rifle at noon, with log lines such as `scope lens:
  <model> <shape or table>, radius <r>, scope camera FOV <deg>°`). Two to three working
  sessions. No prototype was built in this study: the smallest one that shows anything
  needs the camera, the material and the pose together.
- **Coverage on day one** [design]: the sniper-rifle family and the trail carbine
  (lens shapes) plus whatever the measured table covers. Weapons with neither keep the 2D
  overlay, so nothing gets worse.

## 6. Setting (question 4)

The 3D scope differs from the original's look, and it changes play: the screen around
the lens is unzoomed, the target is smaller, and the gun hides part of the view. So it
should be **a switch on the settings page's Gameplay tab**, like true iron sights:
"3D scopes" on or off, defined in NVForever.ini [Settings]. Off is the original's
overlay. Whether it is on by default is the owner's call ([owner-review.md](../owner-review.md)
item 33).

## 7. Test version (built 2026-10-26)

At the owner's request ("a test version with a gameplay switch"), a first cut is built.
It is off by default: the **"3D scopes (test)"** switch is on the settings page next to
True iron sights, and `--scope-3d` on the command line turns it on.

- **Lens table** (`nvforever/src/scope_lens.rs`) [observed: measured as in §2]: the
  sniper rifles (3 models), the trail carbine scope, the hunting rifle scope (4 models),
  the varmint rifle scope and Ratslayer (5), the 9mm scope (2) and the laser rifle scope
  (2). Other scoped guns keep the full-screen picture.
- **The gun** is turned and moved so the point `EYE_RELIEF` (14 units) behind the
  eyepiece lands on the eye (`player_body.rs` `line_up_sights`, the true-iron-sights
  correction). This works whether or not the model has a sighting node.
- **The lens** (`scope_view.rs`, `draw_lens`): two discs on an entity at the eyepiece,
  drawn on the arms' layer. The first shows a 768² HDR image rendered by a second world
  camera under the player's camera. The second shows the MOD3 reticle rendered without
  `keepScope01`/`MaskRing*`, cut to radius 361. `lens_camera` sets the camera's angle
  each frame from the lens's size on screen and the full zoom's magnification, and
  copies the main view's fog and far plane.
- **The main view** goes `LENS_MAIN_SHARE` (0.2) of the way to the Sight FOV
  (`aim_zoom.rs`). The arms stay drawn and the crosshair stays hidden.
- **Night vision** (2026-10-26): a night-vision scope's `NightScopeISFX` is held in the
  lens only (`ScreenModifiers::hold_lens`). The lens camera has an image-space grade of
  its own (`imagespace::LensGrade`) that holds only the lens's modifiers on a neutral
  grade. The main camera then grades the lens with the rest of the screen [design].
  [observed: `scope_3d_night.png`, Ratslayer at 23:00: the lens green and bright, the
  night around it as without the scope.]
- **Sway** (2026-10-26): with a 3D scope the wobble of weapon-scopes.md §5 no longer
  turns the view. It turns the gun (the sights correction's direction), the lens camera
  (turned off the main camera by the same angles) and the shots
  (`nvf_play::aim_zoom::LensSway`, `combat::fire`). The world around the scope stays
  still [design].
- **Brightness** (2026-10-26): the lens shows the zoomed world as lit, and the main
  camera's tonemapping applies the eye adaptation to the lens with the rest of the
  screen. Bevy's auto exposure adds its value to the view's colour-grading exposure,
  which the tonemapping pass multiplies in [observed: bevy_post_process 0.19.1
  `auto_exposure.wgsl`, bevy_core_pipeline `tonemapping_shared.wgsl`]. In the first cut
  the lens was much darker than the room in an interior. The cause was the MOD3's glare
  (`Additive01`), tint ring and inner shading: drawn into a clear image they kept their
  alpha and lay over the lens as a dark veil. Now only the reticle is drawn
  [observed: `scope_3d_interior.png` with them, without the reticle disc, and with the
  reticle alone; the lens matches the door panel behind it]. Light clustering was ruled
  out: one cluster changed nothing.
- **Impact decals** (2026-10-26): the lens camera draws `decals::DECAL_LAYER` (it has a
  depth prepass) [observed: `scope_3d_shots.png` against the same frame without the
  shot, the sniper rifle at the rock by the Prospector Saloon: the hole's mark under the
  reticle].
- **Cost** (2026-10-26) [observed: `--log-frame-times 1`, Goodsprings at noon, the
  sniper rifle aimed for ~28 s, light glow on, 1280 × 800, this machine, other sessions'
  work possibly running]: the overlay 5.6-5.8 ms a frame while aimed, the 3D lens
  7.0-7.9 ms (about +1.7 ms, +30 %). Unaimed, both 5.6 ms. The lens camera's own sun
  shadow cascades are the likeliest share (Bevy makes them per view); not split further.
- **Still not done**: the lens camera has no motion blur or depth of field of its own.
  Binoculars and the camera keep the picture.
- **Smoke**: `scope-3d-lens` (sniper rifle), `scope-3d-lens-tube` (Ratslayer),
  `scope-3d-lens-night`, `scope-3d-lens-shots` and `scope-3d-lens-interior`. The first two passed on 2026-10-26 [observed: `play-14s.png` / `play-15s.png` at 1280 × 800,
  not committed]. The zoomed bushes and the red mil-dot / black duplex reticle show
  inside the scope's rear ring, the house and the yard unzoomed around it, the arms
  holding the gun. Logged: lens camera FOV ~6° for the sniper rifle (Sight FOV 25°,
  magnification 3.46).
