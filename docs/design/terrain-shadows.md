# Terrain shadows beyond the sun's cascades (#2671)

An enhanced-look feature, off by default, not a parity item: the landscape shades itself
where the sun's shadow cascades no longer reach (`--shadow-distance`, 8,000 units by
default). A page of its own rather than a section of `generated-lod.md`: that page is about
the distant meshes' bake, while this feature has its own height source, a GPU producer and
a lighting term, and draws on the generated LOD only as one possible height source.

Stages: T0 the height source (`engine/nvforever/src/sun_heightfield.rs`), T1 the
shadow-height producer (`engine/nvforever/src/terrain_shadows.rs`, `terrain_shadows.wesl`),
T2 the receivers (`engine/nvforever/src/sun_visibility.wesl`, through the sun visibility of
[sun-visibility.md](sun-visibility.md)) and the settings row Terrain shadows (experimental)
(`terrain-shadows`, Graphics › Shadows, off by default and in every preset).

## T0: the height source

A world-aligned map of 512 × 512 texels of 256 units (131,072 units, 32 × 32 cells) around
the camera's cell, each texel the highest stored LAND vertex on its square, with a validity
mask (no LAND, not loaded, outside the worldspace: invalid, height −1e30). Published as an
`Rg32Float` image (R height, G validity), row 0 the southernmost, with its origin, texel
size, side, valid height range and a version. Details and bounds in the module notes of
`sun_heightfield.rs`. Its limit of 64 loader results a frame bounds only the taking of
results, not the frame's whole cost: a recentring copies the grid, a publication copies the
grid and uploads its image, and T1 then dispatches the whole map in one frame [design: the
code; the frame cost is not measured].

## T1: the shadow-height map

**What it computes.** For each texel, the shadow height
`S(x, z) = max over t > 0 of [H((x, z) + t·L) − t·tan(e)]`, L the horizontal unit direction
towards the light, e its elevation. A receiver at height y over that texel is in the
landscape's shadow when y < S. One value per texel serves every receiver height (ground,
roofs, actors, leaves, water), which a horizon angle stored at ground level would not
[engineering choice, #2671 plan].

**Published basis.** Storing per-texel light occlusion of a height field along a light
direction is the idea of horizon mapping [documented: N. Max, "Horizon mapping: shadows for
bump-mapped surfaces", The Visual Computer 4(2), 1988]. A max-height mipmap lets a march
over a height field skip space without stepping over a ridge [documented: A. Tevs, I. Ihrke,
H.-P. Seidel, "Maximum mipmaps for fast, accurate, and scalable dynamic height field
rendering", I3D 2008]; T1 does not build one yet (below). The shadow-height form, the step
schedule and the cadence rule are engineering choices for this engine, not taken from
those papers.

**The pass** (`terrain_shadows.rs` module notes): a compute pass, one invocation per texel
(8 × 8 workgroups), reading T0's image with `textureLoad` and writing an `Rg32Float` storage
texture (`Rgba32Float` since T2): R = S, G = the horizontal distance to the texel that gave
S (T2's penumbra), −1e30 / 0 where nothing casts, B = the ground's relief around the texel
(T2's bias). `Rgba32Float` write-only storage is a guaranteed format in wgpu 30 without
optional features [documented: wgpu-types 30.0.2
`TextureFormat::guaranteed_format_features`]. It runs in the render graph's `Begin` set,
only on a frame with a new job; the result is published when the render world has recorded
the whole dispatch, into one of two targets (the other stays published meanwhile).

**The march** [engineering choice]. From the receiver texel's centre, at most 96 samples at
`t_i = 0.5·i + b·i²` texels, b set so the last sample lands on the ray's reach: the nearer
of the map's edge and `(max − min) / tan(e)`, past which nothing on the map can rise above
its lowest height. The samples are held 1e-3 texel short of the reach, so the last one lies
in the last texel the ray crosses inside the map, never on the edge itself (until
2026-11-17 it fell on or past the edge and ended the march, so a caster in the last step
before the edge cast nothing). High sun therefore takes few samples (cheap [hypothesis]: the
pass's time is not measured), and low sun covers the whole map
with steps that grow as the square root of the distance. Every sampled texel is scored with
the ray's exact entry distance into it, as the brute-force CPU reference
(`sun_heightfield::shadow_height_reference`) scores it, and the march stops once a ray
falling from the map's highest height cannot beat the best so far. The receiver's own texel
is left out (its maximum is the receiver's own ground), as in the reference.

**Bounds.**
- The march visits a subset of the texels the reference visits and scores them alike: its
  raw shadow height S is never above the reference's (tested:
  `march_matches_reference_on_ridge`), within this domain only: S at or above the map's
  lowest valid height (the reach leaves out what falls below it, so a constant-height map
  gives no shadow where the reference gives a finite S under that height; the tests compare
  above ground level). It says nothing about the receivers' filtered, soft T: the band
  grows with the winning caster's distance G, so a march that misses a narrow farther caster
  and keeps a nearer one can come out darker (review of f8d532cca, finding 4: (S, G) =
  (90, 1,000) against the reference's (100, 20,000) gives about 0.08 against 0.69 for a
  receiver at 16 under tan e = 0.2), and the bilinear filter has no pointwise reference.
  Ordering rendered visibility would need a receiver-level reference.
- It is lighter only where it steps over a texel. The longest step is 0.5 texel while the
  reach is under 48 texels and 14.5 texels (3,700 units) at the full diagonal of 724 texels
  (`max_step`, test `steps_grow_with_reach`). A caster at least as long along the ray as the
  local step is always sampled, and for a flat-topped caster S is then within
  `tan(e) · step` of the reference (tested on synthetic ridges); a narrower caster can be
  missed. A caster the map's edge cuts shorter than the step is still sampled, by the last
  sample (only a sliver under 1e-3 texel is missed; tests
  `march_reaches_casters_near_the_map_edge`, `march_matches_reference_at_low_sun_from_every_side`).
  A receiver standing on a caster can see the caster's rest as narrower than the step, when
  its ray leaves it sideways right after the receiver's own texel. Replacing the texel read with a max-pyramid read at the step's level would make
  the march conservative instead; that is the planned refinement if pictures show leaks.
- The heightfield keeps each texel's highest point, so on a slope facing the light S can
  stand up to one texel's relief above the receiver's own ground (test
  `slope_overshoot_is_within_a_texel_of_relief`): T2 needs a slope-scaled bias of about
  `texel · |slope|` (as built: "The bias" under T2).
- Rays end at the map's edge: a ridge beyond it casts nothing. With T0's recentring the
  camera stays at least 11 cells (45,056 units) from the edge.

**Cadence** [engineering choice, #2671 plan]. A new map only when T0's version changes or
when the predicted shadow-edge displacement since the published map's light exceeds half a
texel (128 units), never sooner than 0.1 s after the last job was asked for (counted from
the request, not the publication, so a job's own latency is not added to every interval;
`update_reason`). For a blocker of height h (the map's `max − min`, a bound for every blocker
on it) the shadow's length `h·cot(e)` changes by `h·|cot(e0) − cot(e1)|` (to first order
`h·csc²(e)·Δe`), not capped at the map: a shorter blocker's shadow can lie inside the map
while the tallest one's runs off it (until the review of f8d532cca both lengths were capped
at the diagonal before subtracting, which called a movement of tens of thousands of units
zero at a grazing sun; test `low_sun_movement_is_not_capped_away`). Its tip swings by
`length · Δazimuth`, the length capped at the map's diagonal. The function is only asked
above the horizon cut-off below, where cot(e) is at most about 100; under it, it answers
"always update". The light is the sun entity's own transform, which turns every
frame. For h = 2,000 units the elevation alone allows 0.028° at 5°, 0.25° at 15° and 1.8°
at 45° before a new map (test `low_sun_updates_more_often`); the sun climbs about 0.21° a
game minute near the horizon (14 hours from 06:00 to 20:00 over 180°, `fnv_world::sky::sun_direction`),
so about every 8 game seconds at 5°, every 70 at 15° and every 9 game minutes at 45°, the
first held to the 0.1 s floor [derived from the formula, not measured]. Below the horizon
fade where the sun's own shadow maps stop (sine of the elevation under 0.1 · sin 0.1,
`sky.rs` `HORIZON_FADE`, `graphics.rs` `SHADOW_FADED`) nothing is computed and "no terrain
shadow" is published; at night the light is the moon and is handled alike.

**Another worldspace** [engineering choice, after the review of f8d532cca]. T0's
heightfield carries a discontinuity generation (`SunHeightfield::generation`), bumped on
every emptying and on every worldspace change even with nothing published. T1's job in
flight and its publication carry the generation of their heights; when T0's differs, both
are dropped and no terrain shadow is shown (the neutral input, T = 1) until a map of the new
heights is complete, and a job the render world finishes is published only if it is of
T0's current generation. T0 can empty and publish the new world's first heights in one
update, so T1 cannot rely on seeing an empty heightfield in between (tests
`world_change_drops_the_published_map`, `stale_world_job_is_not_published`).

**Off path.** Everything sits behind `TerrainShadowsWanted` (false by default; the settings
row or the debug override sets it, T2), which turns T0 on with it. While false no job is made, no target allocated, no pipeline compiled, and
the render system's run condition fails.

**Not measured yet.** The pass's GPU time (it carries a `terrain_shadows` time span) and its
spikes, how often low sun asks for a new map in play, whether the march's misses show on
difficult ground (terraces, narrow channels; the pictures of "What has been run" under T2
show none at the places taken), and the producer on the Steam Deck class of GPU. The cost model
in the plan (about 25 million height reads for a full map at 96 samples) is an estimate
[hypothesis].

**Debug aids.** `NVF_PLAY_DEBUG_TERRAIN_SHADOWS=1` turns the producer (and T0) on;
`NVF_PLAY_DEBUG_TERRAIN_SHADOW_DUMP=<path.png>` writes the first map published 3 s into the
run, its heightfield and a shaded-ground mask as greyscale PNGs, north up.

## T2: the receivers

**Where.** sun_visibility.wesl `nvf_terrain_visibility` gives T for a receiver at world
position P lit from L; the sun visibility's composition (docs/design/sun-visibility.md §2)
uses it: `V = K · Vsolid`, `Vsolid = min(C, T)` at every distance (beyond the last
cascade Bevy's C is 1, so T alone). It runs in the directional light loop of the patched `apply_pbr_lighting`, on
the outdoor sun (the moon at night) only, for receivers and non-receivers alike (terrain,
objects, actors, grass, leaves, tree cards, water). The CPU mirrors in sun_visibility.rs
(`terrain_visibility`, `terrain_lit`, `solid_visibility`) follow the shader line for line;
the constants are in both, a test compares them [design: the files].

**The map as read** (`terrain_shadows.rs` module notes): `Rgba32Float`, R = S, G = the
caster's horizontal distance, B = the ground's relief around the texel (below), row 0 the
southernmost; `uv = P.xz · uv_scale + uv_offset`, texel (i, j) spans uv `[i, i+1]/side`.
Where nothing casts, and on texels with no terrain, R = −1e30 and B = 0.

**The filter.** The texel-space position `x = uv · side` is taken half a texel back
(`c = x − 0.5`); the four texels around it (`floor(c)` and `+1`, indices clamped to the map,
since the shared sampler repeats and the format is not filterable) are read with
`textureLoad`, each is *compared* with the receiver, and the four comparisons are weighted
bilinearly by `fract(c)`. S itself is never interpolated: it jumps by hundreds of units at a
ridge's shadow end, and an interpolated S would shade the ground in front of a ridge's
shadow [engineering choice; the percentage-closer idea of filtering comparisons, not
depths: W. Reeves, D. Salesin, R. Cook, "Rendering antialiased shadows with depth maps",
SIGGRAPH 1987].

**The soft edge** [engineering choice]. Each comparison is
`lit = smoothstep(0, 1, saturate((y′ − S) / band + 1))`: fully lit at and above S, fully
shaded `band` under it, where

`band = max(G · 0.01, 0.25 · texel) + 0.5 · texel · min(tan e, 1)`

- `G · 0.01`: the penumbra grows with the caster's distance; 0.01 rad stands for the sun's
  apparent diameter (about 0.53°, 0.0093 rad, rounded). At G = 20,000 units: 200 units of
  height.
- `0.25 · texel` (64 units): a floor, so a near caster's edge is not razor sharp at the
  256-unit grid.
- `0.5 · texel · tan e`: half of what S falls along the light over one texel, so the four
  comparisons change smoothly across a texel and the grid does not show as stair steps;
  `tan e` (from L) is held to at most 1, as a high sun shades little and would widen the band
  past a texel's fall.

The band lies *under* S, not around it: a receiver at S is lit. That makes every shadow
shorter by about half a band (`band / (2 · tan e)` horizontally) and keeps the comparison
from shading a lit slope [engineering choice: the brief prefers too little shadow to
speckle]. The figure of about 200 units (0.8 texel) is the near-caster case at about 13°
(07:00) only; the bias's constant 16 units adds about 0.27 texel there (`16 / tan e`), and
the relief term and a distant caster's wider band shorten shadows further [derived from the
formulas, not measured].

**The bias** [engineering choice]. Before comparing, the receiver is lifted:

`y′ = P.y + 16 + 2 · relief`, `relief` the bilinear mix of the four texels' B.

Why: T0 keeps each texel's *highest* vertex, so on a slope S over a texel stands up to about
a texel's relief above the ground there (test `slope_overshoot_is_within_a_texel_of_relief`);
unbiased, lit slopes speckle and stripe (the main session's dump at Goodsprings 07:00). For a
plane rising g per unit towards the light (lit while g < tan e), the sunward neighbour's
maximum gives `S − y ≤ (1.5 − f)·g·τ − 0.5·τ·tan e` at the receiver's own texel (f the
receiver's fraction towards the next centre, τ the texel) and `(2.5 − f)·g·τ − 0.5·τ·tan e
< (2 − f)·g·τ` at the next one; a plane facing the light at slope s gives at most
`(f − 0.5)·s·τ`; a slope across the light adds half a texel's cross relief. With relief = g·τ
on an axis-aligned slope, `2 · relief` covers every case; 1.5 did not (a prototype of the march
and the receiver left T at 0.991 on a lit slope; test `lit_slope_has_no_acne`: planes up to
45° from eight directions under suns at 5°, 13° and 30°, T > 0.9999 everywhere, while the
unbiased comparison shades the ground at over 10,000 of the samples). The 16 units cover
f32 noise and the rendered triangles lying under their highest vertex.

What the bias is not: an error bound. The plane cases above are all the tests cover, and
two failures remain (review of f8d532cca, finding 3, derived, not seen in a picture):
- On terraced ground it can lift a receiver out of a real shadow. Sunward texel maxima
  0, 128, 256, 384, 384, constant across rows, under tan e = 0.2: at the centre of the texel
  whose maximum is 128 the relief is 128 and the lift `2 · 128 + 16 = 272`, so a receiver
  on a flat terrace at 128 is compared at 400, above S = 384 − 384 · 0.2 = 307.2 from the
  uphill 384s: lit, though under S.
- B sees only differences between texel maxima: relief inside a texel whose neighbouring
  maxima are equal gives B = 0, so a narrow sunlit channel along the light gets no bias
  while the neighbours' maxima may shade it.
The bias and the filter are kept as built: with them the pictures of the main session run
2026-11-17 (fork-notes; "What has been run" below) show no speckle on sunlit slopes, at
Goodsprings 07:00 and 19:00 among others. Lowering the multiplier would trade the first
failure for speckle; keeping terrace shadows without acne needs better information about
the ground's error inside a texel than B.

What **relief** is (`terrain_shadows.wesl` `relief`, mirrored by `texel_relief`): per axis
(east-west, north-south) the *smaller* of the two neighbours' height differences (one side
alone at the map's edge or beside an invalid texel, 0 when both are missing), the two axes
summed; 0 for an invalid texel. A uniform slope gives `τ·(|g_x| + |g_z|)`; the flat foot (or
flat top) of a cliff gives 0, so the bias does not lift a cliff's own shadow off the ground
right at its foot (test `valley_behind_ridge_is_shaded`: shaded from 0.2 texel past the
ridge). Why not the alternatives the plan named [engineering choice]: the hook passes no
normal (`nvf_sun_visibility(P, L, C, view_z)` in three callers), and a receiver's own normal
is not the ground's (a wall facing the sun would get an endless bias and never take a ridge's
shadow); S's own gradient along the light is tan e inside every real shadow, so a bias from it
would shorten every shadow by its factor times a texel. The relief costs the producer four
more loads a texel per map and widens the output from `Rg32Float` to `Rgba32Float` (4 MB a
target at 512 × 512, two targets).

**Edges.** T fades to 1 over the map's last 4 texels (1,024 units; smoothstep of the
distance to the nearest edge) and is 1 outside the map, before any texture read
[engineering choice: marches end at the edge, so near it nothing is known]. Texels with no
terrain hold S = −1e30 and shade nothing (test `invalid_texels_shade_nothing`); the bilinear
filter fades a shadow into such a hole over one texel. Test `map_edge_fades_out`.

**Only terrain-scale shadows; the darker of the two.** T is the landscape's shadow only
(256-unit texels): near the camera the cascades shade objects and the terrain alike, and
`Vsolid = min(C, T)` counts a ridge already in both once, never `C · T` [design: the
maintainer's decision on #2920, 2026-11-18: "the plain darker of the two, with no fade-out
of object shadows beyond the sun shadows' range"]. There is no blend by distance: an
object's cascade shadow stays whole up to the cascades' cut-off line, exactly as with the
row off, and past the last cascade Bevy's C is 1, so T alone shades (test
`terrain_combine_is_the_darker_of_the_two`). With Sun shadows off, C = 1 and Vsolid = T at
every distance.

An earlier version of this branch blended Vsolid from `min(C, T)` to T over the last fifth
of the last cascade, so that object shadows ended in a fade instead of at the cut-off line;
with the row on, about 0.3-0.5 % of the pixels came out slightly brighter in three aerial
views [observed: main session run 2026-11-17, fork-notes]. The maintainer chose the plain
combine instead, so the row changes nothing inside the cascades' range. Separate object and
terrain shadow terms would allow a more selective transition; not taken.

**Activation** (`terrain_shadows.rs` `feed_sun_visibility`, `follow_graphics`): the settings
row Terrain shadows (or `NVF_PLAY_DEBUG_TERRAIN_SHADOWS=1`, which wins) sets
`TerrainShadowsWanted`, which turns T0 and T1 on and `SunVisibility::terrain_enabled`, which
alone decides the views' terrain variant, markers and pipeline key bits. The publication only
fills `SunVisibility::terrain` (the uniform and the texture). Switched on with nothing
published (indoors, no heightfield yet, the light under the horizon fade, the first map still
computing) the uniform's terrain bit is clear: T = 1 and Vsolid = C before any read, and
binding 41 holds a one-texel stand-in that casts nothing (S = −1e30; Bevy's white fallback
would read S = 1 and shade every receiver below y = 1). A map published but not uploaded
yet is treated alike: `prepare_sun_visibility` binds the stand-in and clears the written
uniform's terrain bit (`usable_uniform`), so the shader never reads the stand-in as a
512-texel map and Vsolid stays C without a usable map (test
`terrain_bit_needs_an_uploaded_map`; review of f8d532cca, finding 5). So doors, the sun setting and new
maps never rebuild pipelines; only the switch does (test `terrain_bindings_follow_the_switch`,
smoke `terrain-shadows`).

**Freshness.** The image is bound by handle: T1's two targets alternate, and
`prepare_sun_visibility` compares the bound texture view with the uploaded one every frame,
so each publication is read from the frame it is extracted on. Nothing relies on a new
texture per version: T0's new versions are written into the same GPU texture and view
(the vendored `GpuImage` preparation reuses a texture of the same descriptor with
`COPY_DST`; earlier notes said every version made a new one, which was wrong), and T1 makes
a bind group per dispatch; a kept view whose contents change needs nothing rebuilt.
The combine reads nothing of the cascades' configuration, so a cascade change needs no
update of the uniform. Between two publications the
map stands still while the sun moves: the step is accepted, not blended [engineering choice:
the simple one]. T1's cadence asks for a new map once the predicted shadow-edge movement for
the map's whole relief (a bound for every blocker on it) exceeds half a texel (128 units),
at most every 0.1 s. That is meant to keep the jump when a new map lands near 128 units
[hypothesis: derived from the cadence formula, not measured], and it is not a bound: the
light moves further while a job waits to be published, and nothing bounds a publication's
age (a job waits for its pipeline to compile and for T0's upload, and it keeps the light it
was asked for; only new heights refresh a pending job's light), and a faster game clock
moves the edge more than 128 units within the 0.1 s floor. The floor holds maps back only
when the edge would move 128 units in under 0.1 s: at 5° and 2,000 units of relief that is
0.028° of sun, about 0.27 s of real time at the game's default TimeScale of 30 (sky.rs,
citing the GECK wiki), so at 5° it binds only with a faster clock [derived from T1's cadence
formula]. Nearer the horizon it binds at the default clock too: in a run from 06:02 the map
was published about 9 times a second for the first minutes (main session run 2026-11-17,
below). The floor spaces the requests, not the dispatches: a delayed job can be dispatched
and published and the next one, already due, dispatched at once.

**Debug view.** `NVF_PLAY_DEBUG_SUN_VISIBILITY=terrain` draws T alone (white lit, black
shaded) in place of the lit colour; `combined` draws V = K · Vsolid. Without the row (or the
override) T is 1 everywhere: the view turns the plumbing on but not the terrain variant.

**Cost**: four 16-byte texel loads, four smoothsteps and a few multiplies per fragment of
the sun's light loop, twice where the transmitted lobe is drawn; the producer's four extra
loads per texel per map. Whole-frame times on one machine (main session run 2026-11-17,
fork-notes: Goodsprings standing still, 2560x1440, an RTX 5070 Ti, the clock running, a
quiet machine, two rounds): main 3.15-3.16 ms at 09:00, the branch with the row off
3.15-3.16, on 3.20-3.22, on with Cloud shadows 3.22-3.25; at 07:00 main 3.33-3.34 and the
row on 3.39-3.40. These are frame averages: the producer's own GPU time, spikes on a
publication and a lower-end GPU are not measured.

**What has been run** (main session runs 2026-11-17, fork-notes). On 4b1cdf3fa, with the
review fixes: the build, the engine tests (nvforever 1,312 passed), engine and root clippy,
the root tests, and the smoke scenarios `terrain-shadows`, `cloud-shadows`, `shadows-sun`,
`sun-shadows-performance`, `graphics-settings` and `settings-headings` passed. Through
Freeside's gate to the Strip with the row on, the log shows the heightfield started over,
`terrain shadows: none (heightfield started over)`, then a first publication for the new
worldspace, and one `sun visibility: ... terrain on` line in the whole run. From 06:02 with
the clock running, a first publication at 0.57° of sun, then `sun moved: edge 15311 units`,
and version 424 after 45 seconds, with no error. The pictures are from f8d532cca, before
those fixes (which change no receiver or march arithmetic). Pictures at Goodsprings 06:15,
07:00 and 12:00 and Novac 19:00, and from the air above Goodsprings at 07:00, 12:00 and
19:00: valleys shaded on the side away from the sun with soft edges, no speckle on sunlit
slopes, nothing at noon. With Sun shadows on, ground inside the cascades' range does not
change with the row (the cascades already shade it) and ground beyond it does; with Sun
shadows off a shaded flat area darkens. In three aerial views about 0.3-0.5 % of the pixels
were slightly brighter with the row on: that was the end band of an earlier version
(above), since replaced by the plain `min(C, T)`; the views were not taken again.

**Not run or not shown yet**: the producer's own GPU time, spikes and a lower-end GPU;
upscalers and MSAA with the row on; how a publication looks while the sun moves (the run
above shows the log, not the picture); save/load with the row on; the layouts and pipelines of the reflection and scope views
and the first-person arms with the terrain variant; difficult ground (terraces, narrow
channels; the bias's limits above); whether the 256-unit grid shows anywhere else.

**Open for tuning** (sun_visibility.wesl and sun_visibility.rs `TERRAIN_*`, changed together):
- the penumbra per unit of caster distance (0.01) and its floor (0.25 texel);
- the grid term (0.5 texel times tan e, capped at tan e = 1);
- the bias (2 × relief + 16 units) against how short it makes shadows on slopes;
- the band's place under S (lit at S) against a band centred on S (truer edges, more risk
  of stripes on slopes);
- the edge fade (4 texels);
- whether the moon should cast terrain shadows at night at the same strength.
