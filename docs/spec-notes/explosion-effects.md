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
- **Not played**: `NiGeomMorpherController` (`explosionsplash.nif`), `NiPSysGravityStrengthCtlr`,
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
- The bomb modifier, particle colliders, spawn on death, mesh
  emitters (7 explosion files emit from meshes: their systems are not drawn).
- Everything in §2–§5 tagged hypothesis is checked only against how it looks; to compare
  with frames of the game: a frag grenade and a missile going off at a known distance and
  hour (compare-frames rules in owner-review.md).

## 7. Heat haze (refraction) [2026-10-20]

The owner saw the Incinerator's fire burst as "an untextured dome shape that expands".

- **The file** [observed, `fnvtool nif decode` of `meshes\dlc03\effects\dlc03flamerexplosion.nif`,
  the model of EXPLs `NVIncineratorFireBurst` and `DLC03FlamerFireBurst`]: under its root a
  billboard node `Refract` with one shape `Refract:0` (a `BSShaderPPLightingProperty` with
  shader flags 0x82018101: bits 15 "Refraction" and 16 "Fire_Refraction" [documented:
  nif.xml BSShaderFlags], refraction strength 0.0556, fire period 10, textures
  `FXPartTinyGlowy.dds` and `FXNormalDistort_n.dds`, no `NiAlphaProperty`); a billboard
  `Core` (an additive glow, stored alpha 0, faded in and out by its alpha controller);
  add-on nodes `AddOnNode00` (ADDN MPSFireMed01, the flames) and `AddOnNode35`
  (MPSSmokeSmall01). Its sequence grows `Refract` and `Core` (transform tracks), shows
  `Refract` for 0.3 s (visibility keys), and keys the refraction strength 0.056 → 0.156 at
  0.033 s → 0.056 at 0.23 s (`BSRefractionStrengthController` on the shape's shader
  property).
- Drawn as an ordinary lit shape, `Refract:0` is opaque and shows its glow texture as a
  grey dome over the flames: the owner's report.
- **Census** [observed, 2026-10-20]: of the 257 models that PROJ (model, muzzle flash),
  EXPL, IPCT and ADDN records of FalloutNV.esm and the add-on plugins name, 31 have a
  shape with the Refraction flag, all on `BSShaderPPLightingProperty` (28 with Fire
  Refraction too, 28 with a refraction strength controller): explosion shells of the
  missile (`explosiongenericl01.nif`), the nukes (`explosionlarge01.nif`,
  `explosionbigfiry.nif`), the gas trap and flamer crit burst (`fxgastrapblast.nif`), the
  Nuka grenade, the Gauss rifle (`dlcanchgaussexplosion01.nif`), the orbital strike. Outside
  weapons, Mister Handy's jet (`creatures\mistergutsy\fxmrhandyjetaddon.nif`) has three
  refracting cloud meshes whose diffuse is plain white (`FXWHITE.dds`) and whose normal map
  is flat (`FXFlatNormal_n.dds`), strength 0.04.
- **Rule** [hypothesis]: a shape with the Refraction flag is not drawn as a surface; it
  shows what lies behind it shifted on the screen by its normal (normal map applied) in
  view space times its refraction strength, in screen UV. Reasons: the jet's white,
  flat-normal clouds only make sense as distortion, and with a flat normal map the bend
  must come from the mesh's own normals; a strength of 0.05 as a share of the screen is a
  visible but small shift. The diffuse texture is not used. Fire Refraction (bit 16) and
  the fire period are not used: what they change is not known (perhaps a scroll of the
  normal map; `BSRefractionFirePeriodController` exists in 13 files).
- Built (`nvf_assets::nif_material`, "Refraction"): drawn after the opaque pass from
  Bevy's copy of it, as the water is; no depth written, so the flames and smoke inside
  the shell show; nothing drawn into shadow maps. Particles and blended shapes are drawn
  later and are not bent by it. The strength keys play with the effect clip
  (`MaterialTarget::Refraction`), and placed objects' free strength controllers too.
- Checked by eye only (smoke `incinerator-heat-haze`, frames 0.1–2 s after the burst):
  flames and smoke, no dome. Not compared with game frames.

## 8. Audit of weapon effect models [2026-10-20]

The census in §7 also listed, per model, what the engine reads or plays [observed,
script over `fnvtool nif decode`; supported types taken from the code of
`nvf_assets::particles`, `model.rs` effect clips and `material_anim.rs`]:

| Gap | Models | Examples | Where |
|---|---|---|---|
| Refraction shells drawn as solid shapes | 31 | Incinerator, missile, nukes, Gauss | fixed here (§7) |
| `NiPSysMeshEmitter`: systems not drawn | 21 | `fxexplosionplasma.nif`, `fxgastrapblast.nif`, `explosionfiresmall01.nif` | particles session |
| `BSStripParticleSystem` + `BSPSysStripUpdateModifier` | 2 | `projectiles\flamerflame01.nif` (every flamer), Cleansing Flame | particles session |
| `NiPSysEmitterSpeedCtlr`, `NiPSysGravityStrengthCtlr`, `NiPSysEmitterInitialRadiusCtlr`, declination/planar angle controllers | 11, 8, 4, 1, 1 | `bloodspraylimb01.nif`, `explosiongenericl01.nif` | particles session |
| `NiPSysBombModifier`; `NiPSysColliderManager`/`NiPSysPlanarCollider` | 8; 4 | `impactexplosiondirt.nif`, laser projectiles | particles session |
| Impact decals (IPCT DODT, texture sets) | all impacts | flamer scorch, bullet holes | decals session |
| NPCs' muzzle flashes at the shot's start, unturned | — | every NPC gun | NPC muzzle session |
| Model files absent from the data | 2 | `sonicredprojectile01.nif`, `sonicredmuzzleflash01.nif` (Old World Blues) | the game's data |
| Texture absent | 1 | `nvdlc04flashbanggrenade_n.dds` | the game's data |

Texture paths written absolute in five Old World Blues projectiles and a Honest Hearts
muzzle flash (`d:\projects\fallout\game\data\textures\...`) resolve: `fnv_vfs::texture_path`
keeps what follows `\textures\`.
