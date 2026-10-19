# Screen splatter (TriggerScreenSplatter, SetScreenSplatterFade, ClearScreenSplatter)

Built 2026-10-18 (branch claude/screen-video; outstanding.md §1 row 18). Code:
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
| iBloodSplatterMaxCount | 25 | not a cap on a call: 100 darkens far more than 25 would [observed, §5]; its use not known |
| fBloodSplatterFlareSize / FlareMult / FlareOffsetScale | 0.065 / 0.95 / 0.45 | the flare: not built |

`[ScreenSplatter] bScreenSplatterEnabled=1` is the INI's only splatter key.

Textures under `textures\gore\`: `screenbloodalpha01opt.dds` (256², DXT5, four soft blob
shapes in its alpha, 2×2) and `screenbloodcolor01opt.dds` (256², DXT1, four red blobs,
2×2); single-splat versions `screenbloodalpha01`–`03.dds` and `screenbloodcolor01.dds`
(128²); `screenblooddot01.dds` (DXT5) and `screenblooddot02.dds` (DXT1), single round
dots, perhaps the flare. A splat here is one atlas cell: the colour cell's RGB with the
shape cell's alpha [hypothesis: the "opt" atlases are the ones drawn].

Placement: uniform over the screen; no rotation [hypothesis]. The splats lie under the
HUD and every menu [hypothesis].

## 4. Not built

- The flare (its settings above; texture and drawing unknown; the small bright glints at
  the cores of the original's splats may be it).
- **Blood the engine puts on the screen.** ClearScreenBlood's page: a console command
  that "clears blood overlaying the screen after the player took damage" [documented:
  the catalogue summary]. So the original puts blood on the screen when the player is
  hurt, but nothing says when (close hits only? every hit?) or how much, so it is not
  built. `player.DamageActorValue Health 40` put no blood on the screen (pictures 0.2 s
  and 1.2 s after) [observed, §5]; a weapon's hit is not captured.

## 5. Checks

Unit tests: `nvf_play::screen_splatter` (ranges, no cap, fade, alpha gain, settings, atlas cells),
`fnv-game` `functions.rs` `films_splatter_and_cell_image_space`. Smoke
`screen-splatter`. Capture job `tools/capture/jobs/fx_screen_splatter_look.steps.json`
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
