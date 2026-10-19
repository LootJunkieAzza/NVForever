# Spec notes: what an explosion looks like (effect models, screen effect)

Clean-room description in plain language. Tags as in CLAUDE.md: **[documented]** cites a
public source, **[observed]** was measured on the owner's install, **[hypothesis]** needs
verification, **[design]** is our choice where the game's rule is not known.

An explosion (EXPL) is drawn with the game's own files: its model (`MODL`), its light,
the impact of its impact data set on what it went off against, and its image space
modifier. Nothing here is authored by us; the work is playing what those files hold.
Code: `engine/nvforever/src/shot_fx.rs` (spawning), `engine/nvf-assets/src/model.rs`
(`effect_clip`, `Rig::effect`), `engine/nvf-assets/src/particles.rs` and
`engine/nvforever/src/particles.rs` (particle systems), `engine/nvforever/src/imagespace.rs`
(`ScreenModifiers::play`). Particles in general: particles.md; modifiers: image-modifiers.md.

## 1. What the files hold [observed, `fnvtool nif decode` over `Fallout - Meshes.bsa`, 2026-10-19]

Survey of the 76 files under `meshes\effects\` whose names hold "explosion", "impact" or
"muzzleflash":

- **Explosion models** (22 of them) carry a `NiControllerManager` with one sequence,
  `SpecialIdle_AreaEffect`, cycle type 2 (clamp), stop time from 1.67 s
  (`fxexplosionplasma.nif`) to 19.2 s (`fxravenrockexplosion01.nif`); 5.03 s for the frag
  grenade (`explosiongrenadefrag.nif`), 4.5 s for the missile (`explosiongenericl01.nif`),
  12.33 s for the nukes (`explosionlarge01.nif`, `explosionbigfiry.nif`). Every
  controller in them has the "manager controlled" flag (flags 108 or 104). What the
  sequences drive, by controller type: particle emitters (`NiPSysEmitterCtlr`) and
  modifier switches (`NiPSysModifierActiveCtlr`) in all; in 15 also node transforms
  (`NiTransformController`: the fireball and shock-wave meshes grow and move), alpha
  (`NiAlphaController`), texture scrolling (`NiTextureTransformController`),
  visibility (`NiVisController`), glow (`NiMaterialColorController` with target colour
  3, TC_SELF_ILLUM [documented: nif.xml MaterialColor], and
  `BSMaterialEmittanceMultController`), and a heat-haze strength
  (`BSRefractionStrengthController`, 11 files).
- The frag grenade's own model (`explosiongrenadefrag.nif`, also the 40 mm grenade's and
  the frag mines') holds particle systems only: flash, long sparks, smoke (a fireball:
  orange to grey over its life, radius 256 ± 64) and shrapnel. Its sequence bursts their
  emitters at time 0 and switches the smoke's three drag modifiers off at 0.33 s.
- **Impact models** hold no sequence. Their controllers stand alone on nodes and
  properties, and none has the Active flag: flags 69 = bit 0 (AnimType "APP_INIT"),
  cycle type 2 (clamp) [documented: nif.xml TimeControllerFlags]. 59 transform, 25
  alpha, 24 emitter controllers over 27 files.
  `impactexplosiondirt.nif` (the explosion data set's dirt impact): two debris-textured
  cone meshes `FastMesh01`/`FastMesh02` that sink 50 units and grow to 1.9–2.6 times
  their size over 0.33 s, and a third, `FastMesh03`, doing the same from 0.133 s to
  0.467 s; `FastMesh02` and `FastMesh03` store material alpha 0 and fade in over
  0.03 s, hold, and fade out by 0.30 s and 0.43 s; a debris particle system bursting
  for 0.17 s with a bomb modifier switched off at 0.2 s.
- **Muzzle flash models** (13): `NiTextureTransformController`s and
  `NiTransformController`s with flags 72 (active, loop).

## 2. Playing a model as an effect

- **The effect clip** (`nvf_assets::model::effect_clip`): the model's `Idle` sequence,
  else a `SpecialIdle…` one, plus every controller of the file that has no "manager
  controlled" flag, whether or not it has the Active flag: transform, visibility,
  alpha, texture scroll, self-illumination colour and emittance multiplier. It is
  played once from when the effect appears, clamped at its end.
- **Inactive controllers are played** [hypothesis]: an impact model's shapes that store
  alpha 0 (`FastMesh02`, `FastMesh03`) are seen only through their alpha controllers,
  and none of those controllers has the Active flag, so the game must start them when
  it shows the effect. Placed world objects still play only active ones (nif.md §6).
- **A controller's time is the effect's time from 0** [hypothesis], brought into its
  start and stop times by its cycle type (clamped: the first key holds until the start
  time): `FastMesh03`'s controller and keys start at 0.133 s, after `FastMesh01`'s at
  0 s, which reads as a second splash after the first. Read with the start time added
  (start + time), as the scrolling smoke was before, both splashes would play together.
  `nvf_assets::anim_play::OwnCycle::time`; it changes nothing for the controllers
  surveyed in nif.md §6, whose start times are 0.
- Transform and visibility tracks of stand-alone controllers are sampled at the clip's
  own time; their keys span their controllers' start and stop times, so this equals
  clamping each to its own times. A looping one (the muzzle flashes') plays once.
- **How long the model stays** [hypothesis]: an explosion's model stays for its clip's
  length (5.03 s for the frag grenade), else 5 s [design]; its particles live on in the
  world (particles.md §2). An impact's model stays for its IPCT's effect duration
  [documented: xEdit IPCT "Effect - Duration"].
- **Not played**: `BSRefractionStrengthController` (the renderer has no refraction),
  `NiGeomMorpherController` (`explosionsplash.nif`), `NiPSysGravityStrengthCtlr`,
  `NiPSysEmitterSpeedCtlr`, `NiPSysEmitterInitialRadiusCtlr` (`fxhelios_laserimpact.nif`),
  the bomb modifier (`NiPSysBombModifier`, 4 files), particle colliders.

## 3. Particle modifiers switched on and off

- `NiPSysModifierActiveCtlr` [documented: nif.xml]: names a modifier of its particle
  system (`modifier_name`) and carries a bool interpolator; under a sequence the
  interpolator is a blend one and the sequence's controlled block (controller ID = the
  modifier's name) gives the keys [observed: `explosiongrenadefrag.nif`].
- Reading [hypothesis]: the keys are on the emitter's time, as the emitter's own on/off
  keys are; a particle feels a gravity or drag modifier while the emitter it came from
  is at an "on" time (`nvf_assets::particles::ModifierActive`). For the frag grenade's
  fireball this is drag for its first 0.33 s, then free flight.

## 4. The smoke's fade [observed, 2026-10-19]

The owner found the frag smoke fading too soon (owner-review.md, playtest list 5
item 10): its simple colour modifier has fade-in 0.01 and fade-out 0.15. Over all 406
`BSPSysSimpleColorModifier`s of the `effects` and `mps` meshes, fade-out is never below
fade-in when it is above 0, and pairs such as (0.5, 0.51), (0.2, 0.21) and (0.05, 0.05)
are common (12, 11 and 71 of them). That fits fade-out being the point of the life
where the fade starts, as particles.md §2 reads it; read as "the last share of the
life", (0.5, 0.51) would fade in over the first half and out over an overlapping last
half. So the 15 % stays: the frag fireball is opaque only briefly and thins out for the
rest of its 2.8 s, by its data.

## 5. The image space modifier

- EXPL `MNAM` is the image space modifier and `DATA` holds the image-space radius
  [documented: xEdit EXPL; combat-data.md §1.4]. "IS Radius: Radius of the area that will
  see the ImageSpace effect"; "IS Mod: The ImageSpace Modifier that plays when explosion
  occurs" [documented: GECK wiki Explosion].
- In FalloutNV.esm [observed, `fnvtool combat form` over the 154 EXPLs]: 97 name a
  modifier, most `0x00019CED` (43) and `0x0001512D` (26, the nukes); the IS radius is
  mostly twice the blast radius (frag grenade 1800 against 900, missile 2000 against
  1000); 2 with a modifier have IS radius 0. 130 have "Ignore Image Space Swap".
- Rule here: when the eye (the player camera) is within the IS radius of the point, the
  modifier plays once from the start, at full strength [hypothesis: no fall-off with
  distance; the record has no field for one], through the same player as scripts'
  modifiers (image-modifiers.md §3–§5). A record that is not animatable is not shown
  [design: it would stay until removed]. A second explosion restarts it.
- "Ignore Image Space Swap" — "Overrides close proximity explosion optimization (which
  removes the blinding flash from explosions)" [documented: GECK wiki Explosion]. What the
  swap is is not known; not used.

## 6. Not built, open

- **Impact decals** (scorch marks): IPCT `DODT` decal data and the texture set; impact
  decals are not built for any impact. A step of its own (projected decals on
  terrain and statics).
- Heat haze (refraction), the bomb modifier, particle colliders, spawn on death, mesh
  emitters (7 explosion files emit from meshes: their systems are not drawn).
- Everything in §2–§5 tagged hypothesis is checked only against how it looks; to compare
  with frames of the game: a frag grenade and a missile going off at a known distance and
  hour (compare-frames rules in owner-review.md).
