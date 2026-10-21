# Screen splatter (TriggerScreenSplatter, SetScreenSplatterFade, ClearScreenSplatter)

Built 2026-10-18 (branch claude/screen-video; outstanding.md §1 row 18); the hurt
blood and the flare 2026-10-20 (queue item 16, branch claude/q16-hurt-splatter, §4). Code:
`crates/fnv-game/src/functions/misc.rs`, `functions/ui.rs` (`SplatterRequests`),
`engine/nvf-play/src/screen_splatter.rs`.

## 1. Callers [observed: decoded scripts of the ten masters]

Only the opening's cut cemetery scene (`QUST 0x00102037`, VCG00; bink-films.md §1):

| Stage | Calls |
|---|---|
| 36 | `SetScreenSplatterFade 0.5 1`, `TriggerScreenSplatter 50 25 2 2` |
| 37 | `TriggerScreenSplatter 50 21 2 2` |
| 38 | `TriggerScreenSplatter 100 17 2 2` |
| 39 | `TriggerScreenSplatter 100 12 3 2` |
| 50 | `SetScreenSplatterFade 0.5 1`, `TriggerScreenSplatter 100 20 2 2`, `50 16 2 2`, `100 13 2.5 2`, `50 9 2 2` |
| 100 | `ClearScreenSplatter` |

The stages are 4 s apart (each sets a 4 s timer) and the durations fall by 4 (25, 21,
17), so the splats of stages 36–38 would end together [observed: the stage scripts].

## 2. Parameters

The vanilla wiki pages are empty. The JohnnyGuitar NVSE extension documents
`TriggerScreenSplatterEx splatterCount splatterDuration sizeMultiplier opacityMultiplier
alphaTexture colorTexture flareTexture fade` [documented: GECK wiki, owner export]: the
alpha texture an atlas of splat shapes, the colour texture an atlas of their colours,
the flare texture "the lens flare when vision is blocked/blurred by the splatter".
Taken here as the vanilla call's first four parameters [hypothesis]; the original's
pictures (§5) agree: the darkening grows with the count, about as the square of the size
factor and with the opacity factor.

SetScreenSplatterFade's `0.5 1`: the share of a splat's time after which it fades, and
the fade switch [hypothesis: from the setting `fBloodSplatterFadeStart` = 0.4 and the Ex
variant's int `fade`]. Its real effect cannot be observed: the call quits the original
(§5). Without a call, splats fade from their start (§5) [observed].

## 3. Game data [observed: FalloutNV.esm GMST, Fallout - Textures2.bsa, Fallout_default.ini]

| Setting | Value | Use here |
|---|---|---|
| fBloodSplatterMinSize / MaxSize | 0.12 / 0.18 | splat side, share of the screen **width**, × size factor [fitted, §5] |
| fBloodSplatterMinOpacity / MaxOpacity | 0.5 / 1.0 | opacity range, × opacity factor with no cap at 1 [hypothesis; the factor fitted, §5] |
| fBloodSplatterOpacityChance | 0.3 | chance of the second range [hypothesis] |
| fBloodSplatterMinOpacity2 / MaxOpacity2 | 0.3 / 0.6 | the second opacity range [hypothesis] |
| fBloodSplatterDuration | 20 | seconds, when the call gives none [hypothesis; a call's own duration observed, §5] |
| fBloodSplatterFadeStart | 0.4 | not used: the original's splats fade from their start [observed, §5] |
| iBloodSplatterMaxCount | 25 | not a cap on a call: 100 darkens far more than 25 would [observed, §5]; here the hurt blood's count and cap (§4) [hypothesis] |
| fBloodSplatterFlareSize / FlareMult / FlareOffsetScale | 0.065 / 0.95 / 0.45 | the flare's side, strength threshold and offset (§4) [hypothesis] |

`[ScreenSplatter] bScreenSplatterEnabled=1` is the INI's only splatter key.

Textures under `textures\gore\`: `screenbloodalpha01opt.dds` (256², DXT5, four soft blob
shapes in its alpha, 2×2) and `screenbloodcolor01opt.dds` (256², DXT1, four red blobs,
2×2); single-splat versions `screenbloodalpha01`–`03.dds` and `screenbloodcolor01.dds`
(128²); `screenblooddot01.dds` (DXT5) and `screenblooddot02.dds` (DXT1), single round
dots, perhaps the flare. A splat here is one atlas cell: the colour cell's RGB with the
shape cell's alpha [hypothesis: the "opt" atlases are the ones drawn].

Placement: uniform over the screen; no rotation [hypothesis]. The splats lie under the
HUD and every menu [hypothesis].

## 4. Blood when the player is hurt, and the flare (built 2026-10-20, queue item 16)

What is known [documented: GECK wiki, owner export]: ClearScreenBlood (console only)
"clears all blood present on the screen after taking damage", so the original does put
blood there when the player is hurt; Fallout 3's TriggerScreenBlood shows "the blood
effect on screen which will slowly fade away", its count 1 to 100; ApplyImageSpaceModifier's
page says the BloodISFX modifier is unused, renamed, and that TriggerScreenBlood plays "the
actual blood effect". No game setting names the hurt blood: the masters' settings with
`blood` or `splat` in their names are the `fBloodSplatter*`/`iBloodSplatterMaxCount` of
§3 and `sBloodParticleDefault` (an impact model) [observed: GMST dump of the ten
masters 2026-10-20]; the INI has only `bScreenSplatterEnabled` (and `bUseThreadedBlood`).
So the hurt blood is taken to be the same splats [hypothesis], and `player.DamageActorValue
Health 40` putting none there [observed, §5] makes it a hit's, not a Health loss's.

The rule built [hypothesis throughout; nothing captured]:

- **When**: a hit that takes Health from the player (a weapon's, a blow's, an explosion's,
  the hit test aid's: everything `combat::hit_player` sees), not DamageActorValue, not
  falls, not god mode (no Health lost).
- **How much**: `ceil(lost / maximum Health × iBloodSplatterMaxCount)` splats, at least
  one, counting a loss beyond the maximum as all of it: a tenth of the health is 3
  splats, a near-kill most of 25. The splats are a call's with the factors 1 (size
  0.12–0.18 of the width, opacity from the two ranges) and the duration
  `fBloodSplatterDuration` (20 s), fading from their start as the scripts' do.
- **Cap**: a hurt adds splats only while the screen holds fewer than
  `iBloodSplatterMaxCount` (25), up to it. That is the setting's use here: §5 showed it
  does not cap a script's call. The scripts' calls are not capped by it.
- ClearScreenSplatter clears them too (they are the same splats). ClearScreenBlood and
  TriggerScreenBlood are not in the function table: no opcode is known for either, and
  no shipped script calls them [observed: the catalogue has no opcode or calls].

**The flare** [hypothesis, fitted where said]: the JohnnyGuitar page's flare texture is
"the lens flare when vision is blocked/blurred by the splatter" (§2), and the original's
splats showed bright red-white glints at their cores only with the opacity factor 2
(§5). Built: each splat carries a flare, the dot `textures\gore\screenblooddot02.dds`
(128², DXT1: a red glow peaking at its centre on black [observed]; the other dot,
`screenblooddot01`, a dark red with soft alpha, is not a glow);

- side `fBloodSplatterFlareSize` (0.065) of the screen's width times the call's size
  factor;
- centre off the splat's by a random direction and a random length up to
  `fBloodSplatterFlareOffsetScale` (0.45) of the splat's half-side (near the core);
- strength the splat's opacity now beyond `fBloodSplatterFlareMult` (0.95), at most 1:
  nothing at the factor 1 (opacity at most 1, so at most 0.05), up to full at factor 2
  [fitted to "glints only at factor 2"]; it fades with its splat and is gone before it.
- drawn over all splats as the dot's red with the glow as alpha (over the dark splat
  that is about the dot added) [engineering choice: the UI draws by alpha]. The white in
  the original's glints is not reproduced.

Log: `screen splatter: <n> splats on the screen (hurt: <lost> of <max> health)`. Smoke
`screen-splatter-hurt`: the hit aid hurts the player for 30 and 8 of the starting 105
(or whatever the save has) Health.

### Not built

- A hurt splat's real look and count: no weapon hit on the player has been captured
  (no capture job has an NPC shoot the player).

## 5. Checks

Unit tests: `nvf_play::screen_splatter` (ranges, no cap, fade, alpha gain, settings, atlas cells),
`fnv-game` `functions.rs` `films_splatter_and_cell_image_space`; hurt counts and the
flare's strength, size and offset. Smokes `screen-splatter`, `screen-splatter-hurt`. Capture job `tools/capture/jobs/fx_screen_splatter_look.steps.json`
(save nvfstrip), one call per step with a picture after each: counts 1 and 10, stage
36's call (50 25 2 2), count 100, stage 50's call (100 20 2 2) through its 20 s, the size
and opacity factors, ClearScreenSplatter, the screen after `player.DamageActorValue
Health 40`, and last SetScreenSplatterFade: once with splats already on the screen,
then with the fade switch off.

**The original's pictures** (run 2026-10-18 from nvfstrip, Freeside's north gate at
night, 1920×1080; output `C:\fnv-private\capture\out\fx_screen_splatter_look.1792356085.jsonl`,
pictures `C:\fnv-private\capture\shots\fx_screen_splatter_look.*.png`) [observed]. Each
picture against the clean one, darkening = 1 − brightness ratio averaged over the screen
(the scene's own change between clean pictures is about 0.013):

| Call | Darkening | Seen |
|---|---|---|
| 1 20 1 1 | 0.00 | nothing |
| 10 20 1 1 | 0.02 | four or five soft dark blobs where the scene is bright, about a quarter of the screen height across |
| 100 1 1 1 | 0.13 | many soft blobs all over |
| 10 20 3 1 | 0.23 | larger blobs |
| 10 20 1 2 | 0.07 | the same blobs, darker, with bright red-white glints at the cores |
| 10 20 2 2 | 0.19 | |
| 50 25 2 2 | 0.61 | most of the screen dark |
| 100 20 2 2 at 0.5 / 5.5 / 10.5 / 15.5 / 21 s | 0.87 / 0.82 / 0.72 / 0.49 / 0.06 (means of the screen) | nearly black, lighter at each step, gone at 21 s |

What follows [fitted]: the splats darken what is under them (black-red over-paint, dark
soft blobs with a red core, the textures' look); the count is the number of splats
(10 shows several blobs; 100 darkens about seven times as much as 10 and 50 about five
times as much, with overlap taken out as −ln(1 − darkening), where a cap of 25 would
give 2.5), so iBloodSplatterMaxCount does not cap it; the size factor works about as
its square (3 → 12.7× the darkening of 1) and the opacity factor about linearly with
no cap at 1 (2 → 2.5–3.6×), so a texel's alpha is the shape's alpha times the opacity,
at most 1; the splat side fits a share of the screen's width better than of its height
(the blobs are about a quarter of the height across at factor 1); splats fade from
their start, all gone at their duration (the 0.4 setting would leave 5.5 s as dark as
0.5 s). ClearScreenSplatter leaves the screen clean.

**SetScreenSplatterFade quits the original** [observed, three runs]: the first run ended
inside a step that began with `SetScreenSplatterFade 0.5 1`; twice more the split job's
step with `SetScreenSplatterFade 0.5 1` alone never returned, once as the first
splatter call of the session, once with ten splats on the screen. The game ended with
nothing logged. So the call's effect cannot be observed from scripts, and the values
here stay a hypothesis. No player meets it: the opening stages that call it are never
reached (§1).

Our SetScreenSplatterFade only records its two values and is safe with any input
(`misc::set_screen_splatter_fade`).

Bounds here [engineering choice]: count at least 0 and at most iBloodSplatterMaxCount
on the screen (that setting itself at most 1,000), duration at most an hour, the size
and opacity factors 0 to 10, a value that is not a number taken as missing.
