# Old World Blues look: comparison with the original (2026-10-23)

Owner's request (through the effects audit session): compare Old World Blues' worldspaces
with the original game the way `fx-lighting-audit-2026-10-23.md` did for the Mojave and
Dead Money, and list everything completely off. Survey only; no engine change on this
branch (nothing found was small and clear-cut enough to fix in passing; see "Follow-ups").

## How it was checked

- **Data survey** (`fnvtool world list/refs`, `esm decode`, `nif decode`, `FalloutNV.esm
  OldWorldBlues.esm` only; HonestHearts data not read). `NVDLC03BigMT` (64 × 64 cells,
  16,488 references, climate `NVDLC03BigMTClimate`, image space `NVDefaultExterior`) and
  `NVDLC03Higgs` (child world, Mojave climate `NVDefaultClimate`) [observed]. The Big MT
  climate has one weather, `NVDLC03BigMTWeather` (fog day 4,000-50,000 power 0.5, night
  3,000-37,500 power 0.5; clouds `NVDLC02_WhispyClouds.dds`; sun, glare and star dome from
  `NVDLC02\sky\`, which `OldWorldBlues - Main.bsa` ships itself) [observed]. Its four
  adapters `NVDLC03BigMT{SunRise,HighNoon,Sunset,Night}ISFX` all tint towards
  (0.33, 0.33, 0.72) at alpha 0.05-0.29, multiply cinematic 0x12 by 2.0 and saturation by
  0.5-0.675 [observed; world.md §6 already covers them].
- **Original-game frames**: 14 cameras (12 kept) from the owner's `nvfstrip` save, `cow`
  to the Big MT (the save has not been there; `cow` worked), `tgm` + `tdetect`, by
  `tools/capture/capture_frames.py` jobs with 200 frames of pause after each screenshot
  step and the game window grabbed from the desktop (the plugin's BitBlt fails, as in the
  effects audit). Private, `C:\fnv-private\references\owb\game`.
- **Our frames**: the same cameras with `nvforever --play --no-voice --no-messages`
  (`--god` where creatures attack; free camera for the Sink balcony), `NVFOREVER_HOME`
  empty, 1280 × 720, built from **main at af7fca7e** (`ours_main`), and again from
  **`claude/beam-blend` at 8d770059** (PR #636, not merged yet; `ours_bb`). Side by side
  in `C:\fnv-private\references\owb\sbs`. #636 changed none of the findings below (frame
  means within a few grey levels, p99 up to +30 at night from the glow).
- Pixel numbers below are medians of 8-bit sRGB regions of the 1280 × 720 frames.

## Spots

Severity: **A** the place looks wrong at a glance, **B** clearly different on comparison,
**C** detail.

| # | Camera (world, pos, heading, pitch, hour) | What differs | Likely cause | Sev. |
|---|---|---|---|---|
| 1 | Think Tank, day — BigMT 1888,-23500,-400, 0°, -8, 12:00 | Original: concrete and ground sunlit warm (ground 162,122,92), sky mauve (97,81,92). Ours: ground half as bright (82,72,56), sky grey-blue (92,88,93). | Sunlight Dimmer not applied (F1); adapter tint drops the base amber (F2). | A |
| 2 | Think Tank, night — same, 22:00 | Original: five broad bright cyan beams over the dome, inner dome glow cyan, metal spheres sheen in moonlight. Ours: beams barely visible (also with #636), spheres black, stars dense and clipped white. | Beam blend (F3); moonlight strength (F1); stars (F4). | A |
| 3 | Sink balcony, night — 1888,-15200,2970, 0°, 0, 22:00 | Original: the ring of 12 balcony force fields is a solid bright cyan band. Ours: the same force fields as faint translucent bluish panels. (The original's camera ended up above the ring, ours at the intended spot; compare by content.) | Additive/emissive strength of `nvdlc03x13fxforcefield01.nif` (emissive ×1.2-2.0, animated alpha) (F3). | A |
| 4 | Sink balcony, day — 1888,-14500,2970, 180°, 5, 10:00 | Original looks over the crater in warm haze, mauve sky. Ours: cooler, bluer, flatter. | F1, F2. | B |
| 5 | X-13, night — -19800,-15616,-304, 270°, 0, 22:00 | Original darker and purple; the door's two lamp beams (`fxlightbeambrt03`) visible. Ours: lighter blue sky with many white stars, door beams faint. | Stars and night sky (F4, F6); beams (F3). | B |
| 6 | X-8, day — 15200,-24770,-144, 90°, -5, 12:00 | Close camera on a fence and a sunlit dirt mound: original's mound warm and contrasty, ours dark greenish-grey and flat. | F1, F2. | B |
| 7 | Z-38 hologram site, night — -16800,-23252,-520, 270°, 0, 22:00 | Same content. Original: faint haze, sparse dim stars (peak 108 grey). Ours: far more stars clipped at 255, sky lighter (23,24,44 vs 15,13,26), facades facing us lit, which the original leaves dark. | F4, F6, F5. | B |
| 8 | Securitron / "MOUNT..." letters, night — 12600,-5600,-688, 90°, -5, 22:00 | Original: the building's west facade glows white in the moonlight, ground grey (frame mean 34). Ours: facade black, ground dark (mean 17). Doubling our sun did not light it. | Moonlight from the wrong side (F5) and too weak (F1). | A |
| 9 | Forbidden Zone dome door, night — 6672,4300,-2173, 14°, -12, 22:00 | Original: concrete wall lit grey-white from the upper right, red only in the ceiling grid. Ours: whole wall magenta and dark; the red lamps dominate. | F1 (moonlight ×2.0 missing), F5. | B |
| 10 | X-20 satellite station, night — 400,-34300,-866, 180°, -5, 22:00 | Same content; ours lighter blue sky, too many stars, ground less moonlit. Heading in `--play` differs slightly (as noted in the effects audit). | F4, F6, F1. | C |
| 11 | Little Yangtze, sunrise — -11000,-9300,776, 0°, 0, 07:00 | Original: whole sky warm peach and very bright (top 233,201,179). Ours: teal-grey sky with pink clouds (147,148,142), the weather's own sunrise sky colour (131,147,148). | Unknown; the adapters tint blue. Candidates: sunrise Sunlight Dimmer ×3.0 reaching the sky/glare, HDR eye adaptation. | A |
| 12 | X-12, sunset — -6200,-17216,-628, 90°, 0, 19:00 | Original: dome and buildings in strong warm sunset light, garden lamp bulb white. Ours: same layout, buildings dull grey, lamp bulb dim blue. Without `--god` cyber dogs killed our player before the picture. | F1 (sunset ×2.0), F2. | A |
| 13 | Higgs Village, day — NVDLC03Higgs 2600,-600,8, 315°, 0, 12:00 | Close camera under a girder frame. Original: the sun blows the sky out white (bloom). Ours: pale yellow sky; ground and grass match. Uses the Mojave's climate, so only the sun glare/bloom differs. | Bloom/HDR (fx audit item 3 and #636). | C |
| – | Forbidden Zone by day (5200,3000), dropped | Both cameras faced a rock face. | Camera choice. | – |

Not seen off anywhere: models (nothing missing or misplaced in 12 spots), LOD (Big MT ships
its own terrain LOD; distant crater and mountains match), textures, clouds (the whispy
layer is the same in both), fog distances. No water in these views (Big MT has no LOD
water; `default_water_height` -10,000).

## Findings (causes, with evidence)

**F1. The image space's Sunlight Dimmer is never read** [observed]. IMGS DNAM has a
"sunlight dimmer" (NVDefaultExterior 1.1) and every IMAD multiplies/adds to it (track
`\x0bIAD` / `KIAD`). Time-0 values: NVWastelandIS ×1.1, NVWastelandSunriseIS ×1.5,
NVWastelandSunsetIS +0.2, NVWastelandNightIS ×1.0, NVWastelandHeavyHazeIS ×1.1; Big MT
**×2.25 day, ×3.0 sunrise, ×2.0 sunset, ×2.0 night**; Dead Money's NVDLC01WestTownIS
**×0.4 + 0.1**. `grep` finds no use in `crates/` or `engine/`. So the Big MT's sun and
moon are 2-2.5 × the Mojave's in the original, and ours are the same as the Mojave's.
Spots 1, 6, 8, 9 and 12 all show the original's directly lit surfaces about 2 × brighter
(ground 162 vs 82 at spot 1, frame mean 34 vs 17 at spot 8). A test build with the
directional light × 2.05 (Big MT day ÷ Mojave day, 2.475 / 1.21) raised spot 1's ground
from 82 to 112 (original 162) [observed];
the rest of the gap is the grade (F2). [hypothesis: the dimmer scales the directional
light; the Mojave's lighting was fitted with its 1.21 built in, so a rule over all worlds
is directional × dimmer(hour) / 1.21, or the Mojave's fit re-based on it.]

**F2. The Big MT grade loses the base amber tint** [observed]. Our grade sets the tint
colour to the adapter's and scales the base amount (NVDefaultExterior amber
(0.98, 0.57, 0) at 0.33) by the adapter's tint alpha (`imagespace.rs` `Cinematic::adapt`).
The Big MT adapters' alphas are 0.05-0.1 by day, so the tint all but vanishes and our sky
keeps the weather's grey-blue (92,88,93); the original's is mauve with red above green
(97,81,92), and its sunlit ground is warmer (R/B 1.76 vs ours 1.45). Tested: keeping the
base amount and moving the tint colour towards the adapter's by its alpha gave a brown sky
(90,75,61): right in red against green, but the blue is lost [observed]. So neither rule
fits; the original keeps both a warm and a blue component. Candidates for the follow-up:
apply the base tint and then the adapter tint (two steps), or read the adapter's Fade
Color (NAM3, mauve (0.52,0.37,0.67) at 0.06 by day, (0.72,0.33,0.72) at sunrise), which we
do not apply to the weather grade at all.

**F3. Bright additive effects are far too faint** (Think Tank beams, Sink force fields,
X-13 door beams). `nvdlc03imposterbeamblue.nif` (the five sky beams): two crossed planes,
additive (NiAlphaProperty 4109), vertex-alpha flag set, vertex alpha **0.15 on the
centre line and 0 at the edges**, emissive white × **10**, view-angle falloff
[observed: `fnvtool nif decode`]. The original draws them as wide bright cyan beams, which
fits colour × emissive 10 × 0.15 ≈ 1.5 at the core with linear alpha. Ours raises alpha to
`ADD_ALPHA_POWER` 1.5 (0.15^1.5 = 0.058) and main does not apply the emissive multiplier,
so the beam is ~0.06 of white; #636's emissive × 10 brings it to ~0.6, still visibly
fainter than the original [observed: spot 2 frames]. This is the opposite of the Strip
lamp beams being too strong at night (fx audit item 4): a low-alpha, high-multiplier shape
loses most of its light to the power. The force fields (emissive ×1.2-2.0, animated alpha
and texture scroll) show the same. Evidence that the blend power should not apply after the
emissive multiplier, or should apply to vertex alpha × texture alpha differently.

**F4. Big MT stars too many and too bright.** The climate's star dome is
`NVDLC02\sky\stars.nif`: texture `nvdlc02\sky\Skystars.dds` 1024², **alpha 255 in every
texel**, colour in 39,882 texels, shader flags without Alpha_Texture (0x80000000), emissive
× 10. The Mojave's `Sky\Stars.nif` has `SkyStars.dds` 512², alpha in only 436 texels,
Alpha_Texture set (0x80000100), emissive × 1 [observed]. We weight star colour by texture
alpha for every star dome (sky.rs, hypothesis fitted on the Mojave), so the Big MT gets
every coloured texel at full strength. Original: sparse dim stars, peak 108-147 grey; ours
4-35 × more bright pixels, clipped to 255 [observed, spots 2, 5, 7]. [hypothesis: the
original draws this dome's colour much dimmer, e.g. a threshold or the weather's star colour
scaled further; needs a look at the texture's colour histogram against the frames.]

**F5. Night light comes from the wrong side** [hypothesis]. At 22:00 our moonlight comes
from opposite the sun (`sky.rs`: below the horizon the light is `-sun_dir`), i.e. from the
east, and lights east-facing walls (spot 7's facades). The original lights west-facing
walls (spot 8's facade, white) and leaves east-facing ones dark (spot 7), as if the night
light kept the sun's side. Two spots; check against a Mojave night frame before changing.

**F6. Night sky background too light and blue**. Night sky medians: original (15,13,27),
ours (23-24, 24, 44-50) at spots 5 and 7 [observed]. The Big MT night adapter adds 0.45 to
cinematic brightness (`TIAD`, read by us as the brightness add) and multiplies 0x12 by 2.0;
whether we read these right at night is open (world.md §6 "IMAD cinematic track order").

## Follow-ups (proposed as sessions)

1. **Sunlight Dimmer** (F1): read IMGS + adapter sunlight dimmer, scale the directional
   light by it relative to the Mojave's, check that the Mojave frames are unchanged
   (`C:\fnv-private\l3sky`) and that the Big MT and Dead Money frames move towards the
   original. Touches every world: one rule, no per-world values.
2. **Weather grade tint and fade colour** (F2, F6, spot 11): find a rule that keeps the
   Mojave fits and gives the Big MT's mauve sky and peach sunrise; candidates above. Do
   it after 1 (the sun changes what the grade sees). The Dead Money session works on the
   same grade code: coordinate (notes left in the shared notes folder).
3. **Bright additive effects** (F3): the blend power and the emissive multiplier for
   low-alpha, high-multiplier shapes, judged on the Think Tank beams, the Sink force
   fields and the Strip lamps (fx audit item 4) together, so one rule serves both.
4. **Star domes and night light** (F4, F5): per-dome star weighting from the shader flags
   and texture, and the moon's direction, checked on Mojave and Big MT night frames.

## Capture notes (for the next session)

- `cow NVDLC03BigMT x y` from the Mojave save works [observed]; the original crashed after
  2 cameras once in four launches (3-4 cameras per launch otherwise).
- SetPos to the Sink balcony (z 2,850) left the player falling: the balcony's collision
  markers are enable-parented (opposite) to an enabled XMarker, so there is nothing to
  stand on; the picture caught the player above the force-field ring. Take that view with
  a free camera only.
- `--play` renders in the Big MT need `--god`: cyber dogs and robots attack the player at
  once (X-12 frame was the death screen; at the Think Tank the screen splatter showed as
  red blobs).
