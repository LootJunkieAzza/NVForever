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
Taken here as the vanilla call's first four parameters [hypothesis].

SetScreenSplatterFade's `0.5 1`: the share of a splat's time after which it fades, and
the fade switch [hypothesis: from the setting `fBloodSplatterFadeStart` = 0.4 and the Ex
variant's int `fade`]. Without a call, splats fade from `fBloodSplatterFadeStart`.

## 3. Game data [observed: FalloutNV.esm GMST, Fallout - Textures2.bsa, Fallout_default.ini]

| Setting | Value | Use here [hypothesis] |
|---|---|---|
| fBloodSplatterMinSize / MaxSize | 0.12 / 0.18 | splat side, share of the screen height, × size factor |
| fBloodSplatterMinOpacity / MaxOpacity | 0.5 / 1.0 | opacity range, × opacity factor, at most 1 |
| fBloodSplatterOpacityChance | 0.3 | chance of the second range |
| fBloodSplatterMinOpacity2 / MaxOpacity2 | 0.3 / 0.6 | the second opacity range |
| fBloodSplatterDuration | 20 | seconds, when the call gives none |
| fBloodSplatterFadeStart | 0.4 | fade start share without SetScreenSplatterFade |
| iBloodSplatterMaxCount | 25 | splats on the screen at most; new ones replace the oldest |
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

- The flare (its settings above; texture and drawing unknown).
- **Blood the engine puts on the screen.** ClearScreenBlood's page: a console command
  that "clears blood overlaying the screen after the player took damage" [documented:
  the catalogue summary]. So the original puts blood on the screen when the player is
  hurt, but nothing says when (close hits only? every hit?) or how much, so it is not
  built. The capture job below takes a picture after damage to see.

## 5. Checks

Unit tests: `nvf_play::screen_splatter` (ranges, cap, fade, settings, atlas cells),
`fnv-game` `functions.rs` `films_splatter_and_cell_image_space`. Smoke
`screen-splatter`. Capture job `tools/capture/jobs/fx_screen_splatter_look.steps.json`
(save nvfstrip), one call per step with a picture after each: counts 1 and 10, stage
36's call (50 25 2 2), count 100, stage 50's call (100 20 2 2) through its 20 s, the size
and opacity factors, ClearScreenSplatter, the screen after `player.DamageActorValue
Health 40`, and last SetScreenSplatterFade: once with splats already on the screen,
then with the fade switch off.

**SetScreenSplatterFade quits the original** [observed 2026-10-18, three runs]: as the
first splatter call of a session, made from a capture job's function script, the game
ended with nothing logged, before the step returned. That happened twice with
`SetScreenSplatterFade 0.5 1` alone (the split job's step 7, after only the "clean"
picture). The first run also ended, inside a step that began with the same call and then
made `TriggerScreenSplatter 100 20 2 2`. TriggerScreenSplatter has not been seen to
crash. Not known: whether the call crashes only with no splatter set up yet (the
reordered job calls it after a trigger to see), only from a function script, or always.
The shipped calls (VCG00 stages 36 and 50) come before that stage's triggers, but a new
game never reaches those stages (§1), so no player meets this. Here the call only
records its two values and is safe with any input (`misc::set_screen_splatter_fade`).

Bounds here [engineering choice]: count at least 0 and at most iBloodSplatterMaxCount
on the screen (that setting itself at most 1,000), duration at most an hour, the size
and opacity factors 0 to 10, a value that is not a number taken as missing.
