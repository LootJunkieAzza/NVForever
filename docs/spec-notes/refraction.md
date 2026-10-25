# Refraction (heat shimmer) [2026-10-21]

Queue item 18. What in the data asks for refraction, and how NVForever draws it. The first
part (explosions' heat-haze shells drawn as refraction instead of solid domes) was built
on 2026-10-20 and is described in [explosion-effects.md §7](explosion-effects.md); this
note covers the whole census, fire refraction and actors.

## 1. Where the data asks for it

- **Shader flags** [documented: nif.xml `BSShaderFlags`]: bit 15 "Refraction", bit 16
  "Fire_Refraction". `BSShaderPPLightingProperty` stores "Refraction Strength" (float) and
  "Refraction Fire Period" (integer) [documented: nif.xml].
- **Controllers** [documented: GECK wiki export, "NIF Animation Controllers"]:
  `BSRefractionStrengthController` animates the refraction strength and
  `BSRefractionFirePeriodController` the fire period, of a `BSShaderPPLightingProperty` or
  `Lighting30ShaderProperty`. The page says nothing of what the period means.
- **Script functions**: `SetActorRefraction` (alias `sar`, 0–10, the player when no
  reference) and `ToggleRefractionDebug` are console/debug functions [documented: GECK
  wiki, category Debugging]; no script of the base game or the add-ons calls
  `SetActorRefraction` [observed: the name is in none of the ten `.esm` files], so its
  opcode is not known and it is not built. `SetTargetRefractionFire` has an empty page.
- **Effects**: the Stealth Boy's MGEF `StealthBoyInvis` ("Stealth Field") has archetype
  and actor value Chameleon and **no** effect shader or object display shader
  [observed: `fnvtool esm decode --edid StealthBoyInvis`]. Whatever the original draws
  on a cloaked actor is the engine's own, not data.

## 2. Census of the base archives [observed, 2026-10-21]

`effect_anim_survey` (crates/fnv-nif/examples) over the base archives
`Fallout - Meshes.bsa` and `Update.bsa` (no add-on archives):

| What | Files |
|---|---|
| Refraction flag on `BSShaderPPLightingProperty` | 75 |
| … of which also Fire Refraction | 66 (no file has Fire Refraction without Refraction on this shader) |
| Refraction flag on `BSShaderNoLightingProperty` | 1 (`effects\granttestfire.nif`, a test file) |
| Fire Refraction flag on `BSShaderNoLightingProperty` | 3 (`fortfirefx.nif`, `secanimfx.nif` without Refraction; the test fire) |
| `BSRefractionStrengthController` in model sequences | 51 files; free-running in 2 (Mister Handy's jet), in `.kf` clips 3 |
| `BSRefractionFirePeriodController` | 13 files, all Novac windows and the like |

- By folder, the 75: `effects` 29, `dlc03` 15, `dlc04` 12, `dlc05` 8, `dlcanch` 7,
  `projectiles` 2, `traps` 1, `creatures` 1. Outside effects and projectiles only
  Mister Handy's jet (`fxmrhandyjetaddon.nif`), the Zeta force-field door
  (`dlc05mzdoorsmfieldeffect01.nif`) and the fire gas trap (`trapgasfire01.nif`).
- Stored strengths run 0 to 1.0 (0 on many shapes whose sequence keys the strength;
  0.04 Mister Handy's jet, 0.0556 the Incinerator's burst, 0.25 the big explosions,
  1.0 the Helios laser and the displacer impact).
- Fire periods: 0, 1, 2, 3, 5, 10 (the most), 18, 40, 200, and negative
  −5, −10, −22.
- The Novac windows (`bungalow_windowext.nif` and the rest) carry both refraction
  controllers but **not** the Refraction flag (flags 0x82280181) [observed], so they do
  not refract; an earlier note (effects-animation-audit.md §1) listed them as examples.

## 3. Fire refraction

- **Rule** [hypothesis]: a refracting shape with Fire Refraction set moves its bend: the
  normal map is read a second time with its texture coordinates scrolled up the texture
  (v grows) by one whole repeat every `period × 0.1` seconds, and that tangent-space tilt
  (red and green, −1..1) is added to the view-space tilt of the shape's own (vertex)
  normal before it is multiplied by the strength. The unscrolled normal map is not used
  then (until 2026-10-25 both were, so the map counted twice). A negative period scrolls the other way. A period of 0
  leaves the bend still. Reasons: the name and its use on fires and explosions point at
  rising heat; the most common period, 10, then gives one repeat a second, which shows
  over an explosion's 0.3 s shell; tenths of a second keep 200 (a roof shock effect) at
  20 s and 1 (the mesmetron and sonic projectiles) at a fast flicker. Not checked against
  the game.
- `BSRefractionFirePeriodController` is not played (it drives only the Novac windows,
  which do not refract).
- Built: `nvf_assets::nif_spawn::fire_period`, `NifSurface::fire_period`, the `REFRACT`
  branch of `nif_material.wgsl`.

## 4. Cut-outs

- The models' refracting shapes bend their whole area, as before (their alpha test, if
  any, is not applied) [hypothesis: no census of refracting shapes with an alpha test].
- A material made refracting by the engine (the Stealth Boy's field, §5) keeps its
  outline: an alpha-tested part keeps its cut, a blended part (hair) is cut at alpha 0.5
  [hypothesis].

## 5. The Stealth Boy's field on the player

- While the player's Chameleon is 1 or more, every mesh of the player's body and
  first-person arms is drawn refracting, a screen shift of 0.03 per unit of tilt (strength
  0.3 in the unit of §6), normal map scrolling once every
  2 s [hypothesis: chosen between the game's own refracting effects, 0.04 for Mister
  Handy's jet and 0.0556 for the Incinerator; the original's look is the engine's (§1)
  and has not been compared with game frames]. Nothing of the body's colour is drawn:
  the body shows as a bent outline of what lies behind it. This replaces the earlier
  blended copy at 30 % opacity tinted blue (aid-items.md §2).
- Built: `stealth_field.rs`. Not built: other actors under Chameleon (Nightkin cast
  `StealthBoyInvisibilitySpell`): their meshes are not changed.

## 6. How it is drawn

**Strength to screen shift** [hypothesis, 2026-10-25]: the shift in screen UV (a share of
the screen's width or height) is the view-space tilt × the strength × 0.1
(`nif_material::REFRACTION_SCREEN`). Until then the strength was taken as screen UV whole,
and the owner found the Incinerator's burst "really really strong and noticeable": keyed to
0.156 (explosion-effects.md §7), its shell moved the picture by up to about a third of the
screen and drew the boulder behind it as a magnified dome. Reasons for a tenth: the stored
strengths run up to 1.0 (§2: the HELIOS One beam, the displacer impact), which as screen UV
would move the picture by a whole screen; with 0.1 the Incinerator's peak is about 2 % of
the screen, Mister Handy's jet 0.4 % (8 pixels at 1920 wide), the Ghoul body glow 0.15 %, and
1.0 a tenth of the screen. Not compared with game frames. The Stealth Boy's field (§5) keeps
its shift (0.03 screen UV per unit of tilt): its material strength is 0.3.

As in explosion-effects.md §7: after the opaque pass from Bevy's copy of it (the view
transmission texture), no depth written, nothing in the shadow maps; particles and other
blended shapes are drawn later and are not bent; a refracting shape behind another
refracting shape is not seen through it (Bevy's single transmission step).

Log lines (`nvforever/src/refraction.rs`): `refraction: drawn: <n> refracting meshes in
view (<m> with a moving bend)` when refracting meshes come into view after none were
(counted ten times a second), and at exit `refraction summary: …`. Smoke:
`stealth-boy-refraction`, `incinerator-heat-haze`.
