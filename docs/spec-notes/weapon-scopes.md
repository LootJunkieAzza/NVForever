# Spec notes: weapon scopes

The scope a weapon carries: when it has one, what the player sees through it, its night
vision and its sway. ESM gap audit row 10 (docs/handoff/esm-gap-audit-2026-10-21.md);
built in claude/e10-weapon-scopes. Aiming and the zoom itself are older:
`nvf_play::aim_zoom` (camera.md, combat-data.md §1.1), true iron sights
(`nvforever/src/iron_sights.rs`).
A 3D scope on the gun's lens instead of the overlay: a test version behind a switch
(docs/design/scope-3d-lens.md).

## 1. The record's fields

- **DNAM Flags 1 bit 2 "Has Scope"** [documented: xEdit wbDefinitionsFNV.pas WEAP,
  `schemas/fnv/objects.json`; GECK wiki "GetWeaponHasScope": the flag of the Art and Sound
  tab].
- **DNAM Flags 2 bit 12 "Scope has NightVision"** and **bit 13 "Scope from Mod"**
  [documented: the same].
- **MOD3** (on WEAP, "Model 3 FileName"): the scope's picture [documented: GECK wiki
  "Weapons", Has Scope: "Target NIF: The file containing the overlay effect used in iron
  sights mode"].
- **EFSD** "Scope Effect", an effect shader (EFSH) [documented: xEdit; GECK wiki
  "GetWeaponScopeEffectShader" returns an EffectShader]. "It appears that this value is
  ignored by the game engine" [documented: GECK wiki "Weapons"]. Only Dead Money's
  Holorifle names one [observed: a scan of the six masters' WEAP records, 2026-10-21].
  **Not used** here, as in the original.
- **Has Night Vision**: "when aiming down the weapon's scope, the player is granted night
  vision if they are in an interior or if the time is considered to be night"
  [documented: GECK wiki "Weapons"].
- **Comes from Weapon Mod**: "the weapon's scope and scope effects will only appear once
  a weapon mod with the 'Increase Zoom' effect is applied to the weapon" [documented:
  the same].

What the masters hold [observed: the same scan]: 53 WEAP records (by Editor ID) have Has Scope; every
one names a MOD3 under `interface\HUD\` (or an add-on's `...\interface\hud\`). Those with
Scope from Mod set Has Scope too (the hunting rifle, the 9mm pistol, the .44 Magnum, the
Varmint Rifle, the laser rifle, the trail carbine). The sniper rifle, anti-materiel rifle,
hunting revolver, marksman carbine, gauss rifle, binoculars and the camera are scoped
outright. Night vision: the Varmint Rifle (with its scope mod) and Ratslayer, the
Holorifle.

## 2. When a scope is up

- A weapon **is scoped** when it has Has Scope, and, if it has Scope from Mod, a mod
  whose effect is Increase Zoom is on it [documented, §1] (`WeaponStats::scope_model`).
- The scope's picture is shown once the player aims (Block) a scoped gun and the zoom
  has reached the weapon's Sight FOV (`AimZoom::blend` 1), in first person
  (`nvf_play::aim_zoom::Scope`). It goes as soon as the player lets go of Block, or puts
  the weapon away [hypothesis: at full zoom; the original's timing is not measured].
- **Third person** [design]: no scope picture; the view zooms as before. The original's
  behaviour there is not recorded.
- While the scope is up the **first-person arms and weapon are hidden** and the
  **crosshair is not drawn** (the scope's reticle is the aim) [hypothesis: the overlay
  covers the view; the original's screenshots show no gun through a scope].
- The zoom is the weapon's Sight FOV, as for iron sights (Increase Zoom mods lower it;
  `fnv_combat::data::WeaponStats::modded`).

## 3. The picture (MOD3)

Base-game four-shape scopes share one layout [observed: `fnvtool nif decode` of
`mildotscope.nif`, `varmitscope.nif`, `duplexscope.nif`, `biggamescope.nif`,
`germanscope.nif`, `laserscope.nif`, `scope3.nif`, `binoculars.nif` in
`Fallout - Meshes.bsa`]: under `keepScope01`, in the model's X (right) and Z (up) plane,
looked at along +Y:

| Shape | What | Extent |
|---|---|---|
| `keepAdditive03:0` | the reticle (`textures\weapons\scope\*_o.dds`), alpha tested / blended (`NiAlphaProperty` flags 4333, threshold 40), emissive white | ±372 × ±348 |
| `keepScope01:0` | the black mask: a ring from a circle of radius ~361 out to a rectangle | ±740 × ±460 |
| `TintRing:0` | a faint coloured ring of vertex colours (additive, flags 4109; alpha 0–0.05 in `mildotscope.nif`, up to 0.60 in `laserscope.nif`) | ±373 |
| `Additive01:5` | the lens glare (`scope01.dds`), additive (flags 4109) | ±435 |

Other base-game and add-on scope models in the six masters (`FalloutNV.esm`,
`DeadMoney.esm`, `HonestHearts.esm`, `OldWorldBlues.esm`, `LonesomeRoad.esm`,
`GunRunnersArsenal.esm`; Dead Money, Honest Hearts and GRA add no new scope NIFs and reuse
the base files) use different layouts, including multiply-blended and untextured
full-window tint/vignette shapes [observed: `fnvtool bsa list` and `fnvtool nif decode`
across `Fallout - Meshes.bsa`, `OldWorldBlues - Main.bsa`, `LonesomeRoad - Main.bsa`]:

| File (archive) | Weapons | Layout & notable multiply / untextured full-window shapes |
|---|---|---|
| `interface\hud\scope01.nif` (`Fallout - Meshes.bsa`) | Boone's scoped rifle, That Gun / 5.56mm pistol, Van Graff laser rifle | 6 shapes: opaque ring mask `Scope01:0` (±740 × ±460), opaque `crosshair_new:0`, blended `MaskRing:4`, additive `Additive:5` & `TintRing:0`, and **multiply** untextured `Inner Lens Shading:3` (`NiAlphaProperty` flags 4323, `ZERO`/`INV_SRC_ALPHA`, vertex alpha 0–1) |
| `dlcanch\interface\hud\dlcanchgaussscope01.nif` (`Fallout - Meshes.bsa`) | Gauss Rifle, YCS/186, Elijah's Gauss Rifle, Faderator | 9 shapes: opaque ring mask `DLCAnchGaussScope01:0` (±779 × ±485), opaque `Red Dot:0`, blended `MaskRing:4`, additive `Additive:5` & `TintRing:0`, and four **multiply** shapes: untextured `Inner Lens Shading:3` (flags 4323) plus textured reticle `Reticule:2`, `RtNumbers:2`, `RtNumbers01:2` (`DLCAnchGaussScope01.dds`, flags 4163 `ZERO`/`SRC_COLOR`) |
| `nvdlc03\interface\hud\nvdlc03circletechscope.nif` (`OldWorldBlues - Main.bsa`) | unused untinted variant | 6 shapes: rectangular frame mask `Scope:0` (±674.5 × ±461.7, opaque, rectangular window), blended `Plane01:0` (`NVDLC03CircleTechScope.dds`, flags 4333), blended `MaskRing01:4`, additive `Additive:5` & `TintRing:0`, and **multiply** untextured vignette `Edge:0` (flags 4163) |
| `nvdlc03\interface\hud\nvdlc03circletechscopegreen.nif` (`OldWorldBlues - Main.bsa`) | K9000 Cyberdog Gun (`NVDLC03WeapCyberdogGun`) | 7 shapes: as `nvdlc03circletechscope.nif`, with **multiply** untextured `BlueEdge:0` (flags 4163) and **additive** untextured full-window quad `GreenFilter:0` (flags 4097 `ONE`/`ONE`, vertex colour (0, 0.16, 0.12, 1.0)) |
| `nvdlc03\interface\hud\nvdlc03circletechscopered.nif` (`OldWorldBlues - Main.bsa`) | FIDO (`NVDLC03WeapCyberdogGunFido`) | 7 shapes: as `nvdlc03circletechscope.nif`, with **multiply** untextured `RedEdge:0` (flags 4163) and **additive** untextured full-window quad `RedFilter:0` (flags 4097 `ONE`/`ONE`, vertex colour (0.16, 0, 0, 1.0)) |
| `nvdlc04\interface\hud\nvdlc04redglarescope.nif` (`LonesomeRoad - Main.bsa`) | Red Glare (`NVDLC04WeapRedGlare`) | 5 shapes: rectangular frame mask `Scope:0` (±674.5 × ±461.7, opaque, inner window ±359.4 × ±359.7), blended reticle `Plane01:0` (`Laserscope_o.dds`, flags 4333), additive `Additive:5` & `TintRing:0`, and **multiply** untextured full-window quad `SuperRedFilter:0` (±498.9 × ±378.7, flags 4163 `ZERO`/`SRC_COLOR`, vertex colours R 0.73–1.00, G 0.33–0.63, B 0.24–0.41, alpha 1.0) |

- **Drawn** [design: how the original draws it is not documented beyond "overlay"; #3225]:
  the base mask's rectangle is 1480 × 920, a 16:10 frame; orthographic cameras looking
  along +Y show **920 model units of height** across the screen's height [hypothesis: the
  mask's rectangle is the screen at 16:10], so the base circle is 0.78 of the screen's
  height. Shapes are partitioned by `AlphaMode` onto dedicated render layers:
  - `SCOPE_LAYER` (`over`, cleared to transparent black): opaque masks (`keepScope01:0`,
    `Scope:0`, `DLCAnchGaussScope01:0`), alpha-tested/alpha-blended reticles and rings
    (`keepAdditive03:0`, `Plane01:0`, `MaskRing*:4`), plus opaque black side bars placed at
    the model's horizontal mask extent (±740 for base scopes, ±674.5 for Red Glare / OWB,
    ±779 for Gauss Rifle) so wider screens stay black without gaps [design].
  - `SCOPE_MULTIPLY_LAYER` (`multiply`, cleared to opaque white, spawned only when the
    model contains `AlphaMode::Multiply` shapes): `SuperRedFilter:0`, `Edge:0`/`BlueEdge:0`/
    `RedEdge:0`, `Reticule:2`/`RtNumbers*:2`, `Inner Lens Shading:3`. Rendering multiply
    shapes onto white preserves their exact per-channel tint factor instead of multiplying
    against transparent black `(0, 0, 0, 0)` and turning the window opaque black (#3225).
  - `SCOPE_ADD_LAYER` (`add`, cleared to black, spawned only when the model contains
    `AlphaMode::Add` shapes, with `add_linear = true` on private material copies):
    `Additive*:5`, `TintRing:0`, `GreenFilter:0`, `RedFilter:0`.
  - A full-screen post-process pass (`scope_composite_pass` / `scope_composite.wesl`) runs
    after `ImageSpaceGrade` and before Bevy's UI pass (`ui_pass`), computing
    `over.rgb + (max(world.rgb, 0) * mul + add) * (1 - over.a)` so multiply filters tint
    the zoomed world, additive glare/filters add cleanly, opaque masks cover everything
    outside the window, and the HUD stays on top [design].
- The materials are the loader's (the NIF's own shaders, alpha and vertex colours).

## 4. Night vision

- A scope with Scope has NightVision gives night vision while it is up, in an interior or
  at night [documented, §1]. **Night** is from the climate's sunset end to its sunrise
  begin (the sky's night colours) [hypothesis].
- **The look**: the image space modifier `NightScopeISFX` (FalloutNV.esm 0x0008CD8C),
  held at its first keys while the scope is up: brightness ×2.5, a green tint
  (0.21, 0.50, 0.21) at full alpha, a slight radial blur and motion blur 0.2 [observed:
  `imad_dump NightScopeISFX`; hypothesis: nothing in the masters plays this record
  (image-modifiers.md §9.2), its name fits, so the engine plays it for night-vision
  scopes]. Found by Editor ID; without it, no night vision is shown.

## 5. Sway

- "The sway you experience while using a weapon's scope is exactly the same as your hand
  sway"; the player's spread (PSp, before the weapon's own) sets it: the sway is a circle
  whose diameter is PSp × 5.955° [documented: GECK wiki "Gun Spread Formula (New
  Vegas)", testing methods]. `fGunWobbleMultScope` (1.0) multiplies the sway seen through
  a scope; `fNonAttackGunWobbleMult` (0.5) multiplies it while the player is not
  attacking [documented: the same page's settings].
- **The motion** [observed: `meshes\characters\weaponwobbles\scopewobble.nif`: a
  `Camera1st` node with a 5 s looping Euler rotation, X keys −6.2° to +0.8°, Z keys −5.2°
  to +6.5°; hypothesis: this is the scope's wobble, X turning the view up and down and Z
  sideways]: the file's two curves, scaled so their widest point from the middle is the
  documented circle's radius × the two multipliers, read in a loop from when the scope
  comes up. "Attacking" is a shot in the last 0.5 s [hypothesis].
- The shots follow the view as it sways, as with crippled arms (crippled-limbs.md §3);
  NVForever's spread cone around the crosshair stays as it is [design: combat-rules.md,
  the cone stands for the hand's sway elsewhere; through a scope both count].

## 6. Logs (smoke)

- `scope: <EDID> (<form>) up: <MOD3>, sight FOV <deg>°, a plain scope` (or `a night-vision
  scope`) when the scope comes up; `scope: down` when it goes; `crosshair: hidden by the
  scope`.
- `scope view: <MOD3> drawn (<n> shapes), 920 units high` once the picture is built.
- `scope: night vision NightScopeISFX on` / `off`.
- `scope sway: PSp <x>, radius <deg>°` when the scope comes up.

Smoke: `scope-overlay` (the sniper rifle at noon), `scope-night-vision` (Ratslayer at
night). Both passed on 2026-10-21; the sniper rifle's frame at 1280 × 800 showed the mil-dot
reticle in a black-masked circle about 0.75 of the screen's height, the HUD over it, no
arms or weapon [observed: `play-14s.png`, not committed]. The scenario's player had PSp
0.671 (a new character's skill under the rifle's requirement), a sway radius of 1.0°.
