# Particle effects: add-on nodes and master particle systems

Status 2026-10-11: built for the effects of the player's shots (muzzle sparks and smoke,
a missile's trail, the flamer's flame, impact dust and debris) and, since 2026-10-16, for
the systems inside explosion and impact models (§3 `ModelEmitter`). Not built for particle
systems of models placed in cells (fires, steam, dust devils). Code:
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
- Log: `particles: add-on node <n> <model>: <systems>` on first use; at exit
  `particles: <born> born in <n> add-on systems, at most <n> alive at once`.

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
- Not built: turbulence, spawn on death, the bomb modifier, strip and mesh particle systems, other emitter shapes than
  box, sphere and cylinder, colour-key modifiers, the ADDN's sound and particle cap,
  particle systems of models placed in cells (the dust devils `fxdustwhirlwind01.nif`
  emit from a mesh emitter that moves along a path). Placed smoke that is built so (not drawn at
  all): `effects/fxlightsmoke.nif` (10 placed in FalloutNV.esm), `dlc03/effects/
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
