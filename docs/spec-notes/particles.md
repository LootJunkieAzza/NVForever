# Particle effects: add-on nodes and master particle systems

Status 2026-10-21: placed models' own systems are each reference's own and script-enabled
references emit (§7). 2026-10-11: built for the effects of the player's shots (muzzle sparks and smoke,
a missile's trail, the flamer's flame, impact dust and debris) and, since 2026-10-16, for
the systems inside explosion and impact models (§3 `ModelEmitter`), and since 2026-10-20
for models placed in cells (§5: fires, steam, smoke). Code:
`engine/nvf-assets/src/particles.rs` (the reader), `engine/nvforever/src/particles.rs`
(the simulation and drawing). Claim tags as in CLAUDE.md.

## 1. Records and files

- **ADDN** [documented: xEdit ADDN; schemas/fnv/placed.json]: `DATA` node index, `MODL`
  model, `SNAM` sound, `DNAM` master particle system cap. FalloutNV.esm has 37, indices
  0 to 36, each with a model under `meshes\mps\` (one under `meshes\effects\`)
  [observed: `fnvtool combat fx`]. With the add-on plugins 38 are read.
- **`BSValueNode`** [documented: nif.xml: a `NiNode` with a `value` and flags]. In the
  effect models these nodes are named `AddOnNode<n>` and their `value` is `n` [observed:
  `impactballisticdirt01.nif` 4 and 13, `handgunmuzzleflash01.nif` 22 and 5,
  `mlprojectile.nif` 24 three times, `flamerflame01.nif` 36].
  Reading [hypothesis]: the node stands for the ADDN with that node index.
- **The ADDN models** [observed: `fnvtool nif decode` on `mpsgunsparks01.nif`,
  `mpsmisslesmoketrail.nif`, `mpsflamerflame01.nif`, `mpsdirtdust01.nif`,
  `mpssmokemetalburn01.nif`]: a `BSMasterParticleSystem` root (`max_emitter_objects`
  20) over a node tree with one or two `NiParticleSystem`s (`world_space` true), each
  with:
  - a `NiPSysData` whose vertex count is small (8 for the sparks, 480 for the smoke
    trail, 46 and 40 for the flame's two systems) and, in some, sub-texture rectangles
    (16 for the sparks, a 4 × 4 grid; 8 for one flame system);
  - modifiers `NiPSysAgeDeathModifier`, `NiPSysBoxEmitter`, `NiPSysSpawnModifier`,
    `NiPSysGrowFadeModifier`, `BSPSysSimpleColorModifier`, `NiPSysRotationModifier`,
    `NiPSysGravityModifier`, `NiPSysPositionModifier`, `NiPSysBoundUpdateModifier`,
    and in the flame `NiPSysDragModifier` (three, named for the X, Y and Z axes);
  - controllers `BSPSysMultiTargetEmitterCtlr` (a float interpolator: 90 for the
    sparks, 30 for the smoke trail, 22.5 and 60 for the flame; a bool interpolator
    with keys on at 0 and off at the stop time; `max_emitters` 2, 6, 2),
    `NiPSysUpdateCtlr`, and in some `NiPSysModifierActiveCtlr`;
  - its own `NiAlphaProperty` (flags 4109 for sparks and flame: source alpha, one;
    4333 for smoke: source alpha, inverse source alpha) and
    `BSShaderNoLightingProperty` with the texture.
- **Counts that fit** [observed]: the smoke trail's 480 vertices = 30 a second × 2.67 s
  (life span 2.0 plus its variation 1.33 ÷ 2) × 6 emitters.

## 2. Readings [hypothesis unless tagged]

- **Master system.** The particles belong to the system, not to the node that emitted
  them: they live in the world and go on after the node is gone. Each add-on node in
  the world is an emitter of its ADDN's systems while it exists. For: `world_space`,
  the name, and the vertex count above (rate × longest life × `max_emitters`).
- **Capacity.** A system holds at most its data block's vertex count of particles and
  serves at most `max_emitters` emitters at once.
- **Emitter.** The controller's float interpolator is births a second; its bool keys
  say when it is on, repeating after the stop time (cycle type 0 in the controller's
  flags [documented: nif.xml TimeControllerFlags]). A particle starts in the emitter's
  box about the emitter object, thrown `declination` radians off the emitter object's
  +z, `planar_angle` about it, each ± its variation; speed, life and radius are the
  value ± half the variation (radius ± the whole).
  - The emitter object's frame is the add-on node's place times the emitter node's
    place in the ADDN model.
  - For +z as the axis [observed]: the add-on nodes' own z axis points out of the
    barrel in the handgun flash (along the model's +y), forward in the flamer's flame
    (+y), backward behind the missile (−y) and up off the surface in the dirt impact
    (+z), each the way that effect should throw its particles.
- **Gravity** (`force_type` 0): an acceleration of `strength` units/s² along the
  gravity object's `gravity_axis`, in the world's axes whatever way the emitter
  points. For [observed]: the sparks' gravity object is turned so the axis points down
  (strength 360); the flame's is named `Wind…` and points up (900: fire rises); the
  smoke trail's drifts sideways and up (9). `force_type` 1 pulls towards the object.
- **Drag.** The share `percentage` of the speed along `drag_axis` is lost 30 times a
  second (`DRAG_STEPS_PER_SECOND`). The rate is fitted, not read: the flame is thrown
  at 2,700 units/s with 0.14 on each axis, and at 30 steps a second it travels about
  600 units, near its projectile's range of 640.
- **Grow and fade.** Size goes from `base_scale` to 1 times the radius over
  `grow_time`, and back to `base_scale` over `fade_time` before death.
- **Simple colour.** Three colours over the life: the first until "color 1 end", a
  blend to the second by "color 2 start", the second until "color 2 end", a blend to
  the third by "color 3 start" [documented: the four percentages' names in nif.xml;
  the schema calls them `color1_end_percent`, `color1_start_percent`,
  `color2_end_percent`, `color2_start_percent`, in file order]. Alpha also rises from 0
  over `fade_in_percent` and falls to 0 from `fade_out_percent` to the end. When
  "color 3 start" is not after "color 2 end" the second colour stays: the dirt dust has
  all four at 0 and only its second colour is not blank.
- **Rotation.** A starting angle and a turning speed, each ± its variation, the speed
  with a random sign when the flag says so; drawn as a turn of the square about the
  view direction.
- **Sub-textures.** Each rectangle is (u, width, v, height) [observed: sixteen of
  0.25 × 0.25 covering the texture]; a particle shows one, picked at random at birth.
- **Radius** is half the side of the square drawn.

- **Emitters driven by a model's sequence** [observed, `explosiongrenadefrag.nif`,
  2026-10-16]: an explosion model's systems sit under its own `NiControllerSequence`
  (`SpecialIdle_AreaEffect`, cycle type 2, stop time 5.03 s). Their `NiPSysEmitterCtlr`s
  carry blend interpolators; the sequence's controlled blocks give, per system name,
  interpolator ID `BirthRate` (a float interpolator: 400,000 for the flash, 800,000
  sparks, 2,400,000 smoke, 12,000,000 shrapnel) and `EmitterActive` (bool keys: on at 0,
  off at 0.00001 s). Reading [hypothesis]: a burst, rate × time on = 4, 8, 24 and 120
  particles, within each system's vertex count. Births are the rate times the time the
  emitter was on during the frame (`ParticleSystemDef::on_time`), so a burst is not
  lost or multiplied by the frame length.
- **Particle colours** are display values (the simple colour modifier's and the
  emitter's): converted to linear before they tint the unlit texture [hypothesis, as for
  unlit shapes' vertex colours, nif.md §6; the impact dust's 0.27–0.43 greys drew as
  0.57–0.69 when used as linear values, too pale and too visible on walls (owner,
  playtest list 5 item 48)].

## 3. Engine

- `ModelNode::addon` carries a `BSValueNode`'s value; the shot effects
  (`nvforever::shot_fx`) give each such node an `AddonEmitter` when they spawn a flash,
  projectile or impact model.
- `ModelCtx::particles(path)` reads a file's `NiParticleSystem`s into
  `ParticleSystemDef`s (emitter, rate and keys, modifiers above, material from the
  system's own properties). Block types it meets and does not read are listed
  (`unread`) and logged with the system.
- `nvforever::particles`: one live system per ADDN system; `emit` (births at each
  emitter, the fraction of a particle owed kept by the system, so emitters that live a
  single frame are not rounded away [design]), `simulate`, `draw` (one mesh per system
  rebuilt each frame, a camera-facing square per particle, with texture rectangle and
  vertex colour).
- `ModelEmitter`: a model's own systems emitted from where its emitter objects are in
  the file, under the model's entity; the shot effects put one on each explosion model
  (`shot_fx`, the EXPL's `MODL`) and each impact model (an explosion's dirt impact,
  `ImpactExplosionDirt.nif`, throws debris particles of its own).
- Log: `particles: add-on node <n> <model>: <systems>` (or `particles: model <path>:`)
  on first use, each system with its rate, size and what it has beyond the common
  modifiers (mesh emitter, strips, bombs, colliders, colour keys, keyed fields); at exit
  `particles: <born> born in <n> add-on systems, at most <n> alive at once`, then
  `particles: born in <file>: <system> <count>, …` for each file whose systems emitted.

## 4. Checked [observed 2026-10-11, `--play` at Goodsprings, exit 0, "run finished"; screenshots kept privately]

- Minigun, third person, aimed at the ground 300 units ahead: sparks round the flash,
  a grey cloud of muzzle smoke and impact dust ahead of the barrel; "475 born in 6
  add-on systems, at most 352 alive at once". Systems named in the log: gun sparks
  (22), metal-burn smoke (5), and by surface concrete dust and debris (6, 8), dirt
  dust and debris (4, 13), wood debris (9).
- Missile launcher: a trail of white smoke puffs along the missile's path (add-on 24,
  29 particles in its 0.33 s of flight).
- Flamer: flame particles are born (add-on 36, two systems) and burn about 600 units
  out; near the nozzle the flame is thin (§5).

## 5. Not built, open

- Built 2026-10-19: `NiPSysModifierActiveCtlr` for gravity and drag modifiers
  (explosion-effects.md §3).
- Built 2026-10-20 (§6): mesh emitters, strip systems, the bomb modifier, planar
  colliders, the colour-key modifier, the emitter and gravity float controllers and
  keyed birth rates.
- Built 2026-10-20: models placed in cells emit their own particle systems and their
  add-on nodes' (`world::spawn_emitters`), from a holder at the reference, while the
  reference is shown. Gomorrah's roof fires are two such STATs in TheStripWorldNew:
  ref `0013BA36` (`NVFiregomorrah01`, `effects/nv/nvfiregomorrah01.nif`) and ref
  `00174ECC` (`NVFiregomorrah02`, scale 1.2); each file is one looping box-emitter
  system with `explosionfire01.dds` plus a `BSValueNode` add-on node 14 (ADDN
  `MPSFireSmall01`) [observed, `fnvtool world refs`, `fnvtool nif decode`, 2026-10-20].
  A reference's scale moves and sizes the emitter box but not the particles' radii
  [hypothesis: the game's rule not known]. An add-on node below a `BSDamageStage` node
  emits only while its reference is at one of that node's model damage stages (a
  motorcycle's smoke once it is damaged, never on the Prospector Saloon's static bikes):
  `destruction-stages.md`.
- Not built: turbulence, spawn on death, the ADDN's sound and particle cap (spherical colliders: §7.3),
  the dust devils' path (`fxdustwhirlwind01.nif` emits from a mesh emitter that moves along
  a path). Placed smoke drawn since the 2026-10-20 change (drew nothing before):
  `effects/fxlightsmoke.nif` (10 placed in FalloutNV.esm), `dlc03/effects/
  dlc03fxdarksmoke.nif` (13, Primm), `effects/nv/fxtiresmoke.nif` (16),
  `clutter/ashtray/ashtray03smoke.nif` (19), `effects/fxhelios_steam.nif` (8)
  [observed, `fnvtool world refs`, `fnvtool nif decode`, 2026-10-13]. The more common
  smoke wisps and ground smoke are meshes with scrolling textures (nif.md §6
  "Controllers outside sequences").
- Open: the flamer's flame is a sparse run of puffs in ours. Its emitters exist only
  while a flame projectile is drawn (640 units at 12,000 units/s, 0.05 s a shot), so
  the systems get about a third of their birth rate. How long the game keeps a flame
  projectile's node is not known.
- Open: everything tagged hypothesis in §2 is a reading of field names checked only
  against how the effects look; none of it is compared with the game frame by frame.

## 6. The rest of the weapon effect models [2026-10-20]

A census of the 257 models the PROJ (`MODL`, `NAM1`), EXPL, IPCT and ADDN records of
FalloutNV.esm and the add-on plugins name [observed: the records' model strings,
decoded with `fnvtool nif decode --schema schemas/nif`; two PROJ models name files that
are in no archive, `sonicredprojectile01.nif` and `sonicredmuzzleflash01.nif`] found
these particle blocks unread before this date. Field names and enum values are
documented (nif.xml, `schemas/nif/particles.json`); what they do is a reading of the
names [hypothesis] unless tagged otherwise.

### 6.1 Mesh emitters (`NiPSysMeshEmitter`, 21 models)

- Fields [documented]: the common emitter fields (speed, declination, planar angle,
  colour, radius, life, each with its variation), `emitter_meshes` (geometry blocks of
  the same file), `initial_velocity_type` (0 use normals, 1 random, 2 use direction),
  `emission_type` (0 vertices, 1 face centre, 2 edge centre, 3 face surface, 4 edge
  surface) and `emission_axis`.
- In the census [observed]: every one has velocity type 0 and emission axis (1, 0, 0);
  44 have emission type 3, 1 has type 4 and 5 list no mesh at all. The meshes are
  `NiTriStrips` with normals, most of them hidden (flags bit 0), placed under nodes
  named for what they emit from (`EmitterCenterAll`, `pCloudGroundEmit02`,
  `EmitterShockWave`); 10 to 107 vertices.
- Reading: a particle starts on a mesh picked at random (each mesh as likely), at a
  point the emission type gives: a vertex; a face's or an edge's centre; a point
  anywhere on the faces, a face picked by its area; a point anywhere on the edges, an
  edge picked by its length. Velocity type 0 throws it along the mesh's normal there
  (on a face, the vertex normals blended at the point), turned off it by the
  declination and planar angle as a box emitter's +z is; 1 any way at random; 2 along
  `emission_axis` in the mesh's frame. All in the mesh's place in the file times the
  model's place in the world.
- No meshes [design]: `explosionfiresmall01.nif` (the ADDN FireSmall01) and two DLC
  proton explosions list none. These emit from the system's parent node, which in
  these files and the box emitters' is named `<system>-Emitter` (`Flame-Emitter`)
  [observed], about its +z. In the game such an add-on may emit from the mesh it is
  attached to (a burning body); not known.

### 6.2 Strip systems (`BSStripParticleSystem`, 2 models)

- `projectiles\flamerflame01.nif` (every flamer projectile, FlameProjectileANT,
  Dragon's Breath) and `nvdlc05\effects\nvdlc05cleansinglame01.nif` (Cleansing Flame):
  one system `FireStrips01` each, a `NiParticleSystem` by its fields with a
  `BSStripPSysData` (`bs_max_vertices` 12, `max_point_count` 4, start and end cap size
  0, eight sub-texture rectangles of 0.25 × 0.5) and a `BSPSysStripUpdateModifier`
  (`update_delta_time` 1/30 s) [observed]. Box emitter 3,300 ± 495 units/s along the
  barrel, radius 128 ± 32, life 0.37 s, 30 a second.
- Reading: each particle drags a strip of at most `max_point_count` points behind it:
  where it was, recorded every `update_delta_time`, and where it is. The strip is drawn
  as a ribbon through its points, facing the camera, as wide as the particle's square
  would be (radius × grow/fade scale, twice over), the texture rectangle's width across
  it and its height along it, the oldest point at the rectangle's bottom; colour and
  alpha as the particle's. A particle with fewer than two points is not drawn. At
  3,300 units/s three segments of 1/30 s make a flame about 330 units long, which fits
  a flamer's 640-unit reach.
- The flame model is a projectile model: its own systems are emitted while the
  projectile is drawn (`ModelEmitter` on projectiles, §3), as its add-on node 36 is.

### 6.3 Bomb modifier (`NiPSysBombModifier`, 8 models)

- Fields [documented]: `bomb_object` (a node), `bomb_axis`, `decay`, `delta_v`,
  `decay_type` (0 none, 1 linear, 2 exponential), `symmetry_type` (0 spherical, 1
  cylindrical, 2 planar).
- In the census [observed]: decay type 1 in all; spherical in the explosions and the
  dirt impact (delta-v 4,050 to 11,790, decay 198 to 1,000), cylindrical with axis z in
  the laser, pulse and laser-sting projectiles (13,500; 9,000; 81,000). The dirt
  impact's bomb object is its debris emitter's own node and a modifier-active
  controller has it on for the first 0.2 s.
- Reading: a push of `delta_v` units/s² away from the bomb object (spherical: from its
  point; cylindrical: from the line through it along its axis; planar: from the plane
  through it across its axis), scaled by the distance d: 1 with no decay,
  max(0, 1 − d / decay) linear, e^(−d / decay) exponential. Switched on and off as the
  other modifiers are (§2, explosion-effects.md §3).

### 6.4 Planar colliders (`NiPSysColliderManager` + `NiPSysPlanarCollider`, 4 models)

- Fields [documented]: the manager names its first collider; a collider has `bounce`,
  `spawn_on_collide`, `die_on_collide`, `collider_object`, `next_collider`, and a
  planar one `width`, `height`, `x_axis`, `y_axis`.
- In the census [observed]: one collider each, axes x and y of a node named
  `Deflector01` below the emitter (the dirt impacts: 1,200 × 1,200, bounce 1, die on
  collide; the flash-bang: 1,024 square, bounce 1; the shovel dirt: 385 × 362, bounce
  0.1).
- Reading: the plane through the collider object spanned by its x and y axes, a
  rectangle `width` by `height` about the object. A particle that crosses it within
  the rectangle dies when `die_on_collide`, else it is put back on the plane and the
  part of its velocity across the plane turned round and multiplied by `bounce`.
  Spawning on collision is not built (spawn on death is not either).

### 6.5 Colour keys (`NiPSysColorModifier`, 1 model)

- `fxhelios_charging.nif`: an `NiColorData` with keys at 0, 0.5 and 1 [observed].
  Reading: the keys are over the share of its life a particle has lived; the colour
  replaces the emitter's colour as the simple colour modifier's does.

### 6.6 Float controllers and keyed birth rates

- `NiPSysEmitterSpeedCtlr` (11 models), `NiPSysGravityStrengthCtlr` (8),
  `NiPSysEmitterInitialRadiusCtlr` (4), `NiPSysEmitterDeclinationVarCtlr` and
  `NiPSysEmitterPlanarAngleVarCtlr` (1 each) [documented: nif.xml names;
  observed counts]: each names its modifier (`modifier_name`) and carries a float
  interpolator, keyed (`NiFloatData`) in the gore sprays and the zap impacts, a blend
  interpolator in the explosions, whose sequence gives the keys in a controlled block
  with that controller type and the modifier's name as controller ID [observed:
  `explosiongenericl01.nif` gravity 270 to 45 over 0.17–0.67 s; `bloodspraylimb01.nif`
  speed 190 to 6 over 4 s].
- Emitter birth rates keyed over time [observed: 17 controllers, e.g. the big fiery
  explosion's ground cloud 60, 150, 360 a second over its first 0.3 s, the limb blood
  spray 150 to 210 then 0 by 3 s]: before this date a keyed rate was read as the
  interpolator's lone value (none, so 0 or the sequence's) and those systems emitted
  nothing.
- Reading: the value replaces the field it names, sampled at the emitter's time
  (an emitter's when a particle is born; for gravity strength the time of the emitter
  the particle came from, as modifier-active keys are), brought into the controller's
  start and stop by its cycle type (`nvf_assets::anim_play::OwnCycle`); a sequence's
  keys on the sequence's time and cycle. Births in a frame are the rate at the middle
  of the frame times the time the emitter was on.
- After this [observed 2026-10-20]: every particle block type in the 257 models is
  read (the census's types: the emitters, modifiers, colliders and controllers above
  and those of §1–§3); smoke scenarios `fatman-explosion-particles`,
  `flamer-strip-particles` and `missile-impact-particles` check the log lines and that
  no `not read` is logged for them.
- Code: `nvf_assets::particles` (`EmitterMesh`, `Strip`, `Bomb`, `Collider`,
  `FloatKeys`); `nvforever::particles` (emission from meshes, bomb push, collisions,
  trails, ribbons).
- Not built: spawning on collision; the strip's cap sizes (0 in both files). An emitter
  object moved by the model's sequence: §6.7.

### 6.7 Drawn models whose sequences drive their emitters [2026-10-24]

- **The models** [observed: `fnvtool sim activator-models`, FalloutNV.esm]: 21 activator
  bases have sequences with `NiPSysEmitterCtlr` tracks: REPCONN's rocket bay `repbay.nif`
  (RepconDome; Forward, Right, Left), the force-field glows (`forcefieldrightglow02.nif`,
  35 references, and the left one; Forward loops 25 s, Backward 7.2 s clamped), the
  protectron pods (Open, Close), the vertibird encounters, the B-29s
  (`b29_riseanim.nif`, `b29_bombanim01/02.nif`), the Hoover Dam AA gun and the Fort's
  howitzer. Scripts start every one: the force fields' `OnLoadPlayForwardSCRIPT` plays
  Forward on load and another script Backward to power them down; RepconLaunchSCRIPT
  plays the bay's Forward, then Right (or Left, sabotaged) [observed: `fnvtool sim
  playgroup-calls`].
- **`repbay.nif`** [observed: `fnvtool nif decode`]: 56 systems, each under a node named
  `<system>-Emitter` that hangs from a node the sequences move (`objBase`, `Rocket01`…
  `Rocket21`; the sequences' 26 transform tracks move the doors, the platform, the
  rockets and debris). The systems' own `NiPSysEmitterCtlr`s carry blend interpolators;
  each of the three sequences gives every system a `BirthRate` (a lone float) and an
  `EmitterActive` track. Most `EmitterActive` tracks are a `NiBoolInterpolator` with a
  lone value and no keys: `PFire01` is false all through Forward and Left, and in Right
  has keys off, on at 0.5 s, off at 1 s; `PCloud04` is off in Right and Left, and in
  Forward off then on from 18.3 s to the end; `PCloudRise` on from 10 s to 18.3 s of
  Forward. Bomb and drag modifiers are switched per sequence too
  (`NiPSysModifierActiveCtlr`, `NiBoolTimelineInterpolator`).
- **Before this**: a track without keys was read as no track (always on), and only the
  first sequence's tracks were read, on the emitter's own time from load. The bay's fire
  and smoke burned from the moment it was drawn, at the rockets' resting places inside
  the closed dome; at the launch nothing showed [observed 2026-10-24: the REPCONN launch
  spot check, 2.2 million particles born, none on screen].
- **Reading** [hypothesis, from the data above and as the sequences' other tracks act]:
  - A `NiBoolInterpolator` without keys is its lone value from 0 (an undefined value, as
    a blend interpolator's, is no track).
  - A placed model drawn with its own nodes (`object_anims::GroupRig`) whose sequences
    drive a system emits by the sequence playing on it (`Playing`, the name a script
    plays), on that sequence's time from its start: its on/off and birth-rate tracks,
    and the emitter fields it keys (`NiPSysEmitterSpeedCtlr`…). A sequence that does not
    name the system, or none playing, leaves it off: an emitter whose controller
    leaves everything to sequences has nothing to emit by. A clamped sequence holds its
    end, the last key's state going on (Forward leaves the bay's clouds on until Right
    replaces it).
  - It emits from its emitter object's node as drawn, so the sequence's moves carry it
    (the exhaust rides with the rockets).
  - A modifier switched by the sequences follows the keys of the sequence the particle
    was born under, on that sequence's time.
  - Models not drawn with their own nodes (explosions and shot effects, `shot_fx`) keep
    the first sequence's tracks on the emitter's own time, as before.
- Code: `nvf_assets::particles` (`SequenceEmission`, `ParticleSystemDef::sequences`,
  `ModifierActive::by_sequence`, `Emitter::object`); `nvforever::particles` (`emit`,
  `simulate`). Log: the model's line ends `; <n> driven by the sequences [<names>]`, and
  `particles: <ref> <path> emits as its <sequence> says: <k> of <n> systems` when a
  sequence begins. Smoke `repconn-rocket-launch`.
- Not built: a mesh emitter's meshes are not moved by the sequence; gravity strength keys
  come from the first sequence only.

## 7. Placed particle systems: the survey and what was still missing [2026-10-21]

The survey [observed: `fnvtool world refs` over every worldspace and interior cell of
FalloutNV.esm, joined with `fnvtool nif stats` of `Fallout - Meshes.bsa` and `Update.bsa`,
the base archives only; `fnvtool nif decode` for the fields below]: **1,920 placed
references of 106 model files** hold a particle system or an add-on node. 81 files (1,239
references) have systems of their own (`NiParticleSystem`, `BSStripParticleSystem`), 25
files (681 references) only add-on nodes; 97 of the references start disabled. The most
placed: `barrel02firelight.nif` (278 LIGH, add-on node 14), the ambient dust motes
`fxlightdustparticleswide02.nif` / `wide01.nif` (161 and 129 STAT), `nvbouldersm.nif` (89
MSTT), `fxradiationdust1x1.nif` (80), `fxfiresmall01.nif` (77, add-on 14), the Gomorrah
sconce `neoclassicalsconcefire01.nif` (76), `fxambdust02.nif` (69), `fxsmokemed01.nif`
(68, add-on 2), `fxfiremed01.nif` (63, add-on 0). The "about 66 smokes" of playtest list
4 item 16 (§5: light smoke, Primm's dark smoke, tyre smoke, ashtray smoke, HELIOS steam)
are among them.

Since 2026-10-20 render streaming gives every one of these an emitter (§5). Why many
still drew little or nothing, by group:

### 7.1 One pool for every reference of a file (1,239 references; fixed)

A model's own systems were one live system per file: every placed copy emitted into the
same particles, capped at the file's vertex count. The sconce's flame system holds 14
(`bs_max_vertices`), so Gomorrah's floor of 43 sconces shared 14 flames; the loaded copies
of the wide dust motes shared 60 particles [observed: the data blocks' vertex counts]. These systems are plain `NiParticleSystem`s with an
`NiPSysEmitterCtlr` (no `max_emitters`, no `BSMasterParticleSystem` above them), so they
are read as one system per copy of the model, as any other block of a placed model is
[hypothesis; the master systems of §2 are the ones made to be shared].
Now each placed reference (render streaming's and a script-enabled one's) has its own
live systems, made when it first emits and dropped once its emitter is gone and its last
particle has died. Shot-effect models (flashes, impacts, explosions) keep one pool per
file, as before (their emitters live a frame or two and the system keeps the owed part
of a particle, §3).

### 7.2 A master system serves `max_emitters` emitters (681 add-on references; changed)

An ADDN's master system serves at most its controller's `max_emitters` a frame [§2]:
`mpsfiresmall01.nif` (add-on 14, the barrel fires and `fxfiresmall01`) 10, with 70 and 130
particles; `mpsfiremed01.nif` (add-on 0) 15; `mpssmokemed01.nif` (add-on 2) 9;
`mpssmokesmall01.nif` (add-on 35) 5 [observed]. The emitters were served in no set order,
so with more barrel fires loaded than that (38 in `OVNorthSewers01`) the ones burning
were any ten. Now the emitters nearest the camera are served first [hypothesis: which
emitters the game serves when there are more is not known; nearest-first keeps the fires
in view burning]. The particles of the system stay shared (its vertex count).

### 7.3 Spherical colliders (14 references; built)

The seven snow globes (`clutter\snowglobes\snowglobes_*.nif`, a MISC and a STAT copy
each) have five snow systems each with an `NiPSysSphericalCollider` [observed], whose
fields are the collider's (§6.4) and a `radius` [documented: nif.xml]. Reading
[hypothesis]: a sphere of that radius about the collider object (scaled with it); a
particle crossing its surface either way dies when `die_on_collide`, else is put back on
the surface with the part of its velocity along the normal turned round and multiplied
by `bounce`: the flakes stay in the globe.

### 7.4 References a script enables (97 references; fixed)

References disabled at the start are drawn by nvf-play when a script enables them
(`script_draw.rs`); their particle systems and add-on nodes did not emit. 11 of the 16
tyre-smoke columns, 23 of the 89 small boulders and 20 of the 36 force-field glows are
among them. They now emit as render streaming's do (`world::spawn_emitters` under the
script-drawn root).

### 7.5 Left open

- `NiPSysResetOnLoopCtlr` (1 reference, the Lucky 38 reactor, `nvlucky38reactor.nif`): not
  read; its system loops without the reset.
- Placed items' own systems emit lying in the world: the 44 dynamite sticks
  (`dynamite.nif`, a fuse system of 1,000 particles) [hypothesis: whether the game keeps a
  dynamite's fuse unlit until thrown is not known; not changed].
- `nvbouldersm.nif` (89 MSTT, 5,000 particles, stop time 20 s): a rock-slide effect that
  emits from the start; whether the game holds it until a trigger is not known.
- Unchanged from §5: turbulence, spawn on death, the ADDN's particle cap (`DNAM`), the dust
  devils' path. (An emitter moved by its model's sequence: built, §6.7.)
- Exit log: `particles: placed references emitting: <n> of <m> with particle systems, at
  most <k> placed model instances at once`; smoke `placed-particle-smokes` (Gomorrah's
  floor: 45 of 45, 45 instances [observed 2026-10-21]).
