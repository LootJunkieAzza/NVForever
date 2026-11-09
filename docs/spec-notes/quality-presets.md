# Graphics quality presets (#1966)

What the original's install holds for its quality presets, and how NVForever's settings
page reads them. Implemented in `engine/nvforever/src/quality.rs`; the page's layout in
`engine/nvf-play/src/settings_layout.rs` (docs/design/settings-page.md).

## The files

- The install carries four preset files in the game folder, beside `Fallout_default.ini`:
  `low.ini`, `medium.ini`, `high.ini` and `VeryHigh.ini` [documented: the install's files;
  also cited in impact-decals.md and world.md]. Each is a plain INI of `[Section]` headings
  and `key=value` lines.
- Between them they set **41 keys** in seven sections: `[Display]` 17, `[Water]` 11,
  `[TerrainManager]` 5, `[LOD]` 3, `[Grass]` 2, `[Imagespace]` 2, `[BlurShaderHDR]` 1
  [documented: the four files, read 2026-11-09]. Every file sets the same 41 keys.
- A key name's first letter gives its kind (`b` a switch 0/1, `i` a whole number, `f` a
  number) [hypothesis: by the names; it holds for all 41].
- What the original does with each number is not documented here, only what each name says
  [hypothesis]; the mapping below is NVForever's own [design].

**No value from the files is in NVForever.** The page reads them from the player's install
at run time, only reading them (`fnv_install::GameInstall::open` on `--data`, the root
folder, names matched without case). The unit tests and the smoke scenarios use made-up
files (`tools/smoke/fixtures/quality-presets*`) [design: LEGAL.md].

## Which key sets which row [design: #1966]

Nine keys set a row. A key a file lacks leaves its row out of that preset.

| Section | Key | Row (`[Settings]` key) | How |
| --- | --- | --- | --- |
| `[Display]` | `fShadowLODStartFade` | Shadow distance (`shadow-distance`) | part, see below |
| `[Display]` | `bDrawShadows` | Sun shadows (`sun-shadows`) | on when not 0 |
| `[Display]` | `iShadowFilter` | Shadow filtering (`shadow-filter`) | 0 is Fast, any other number Smooth (the vanilla filter); Temporal is NVForever's own and no preset takes it |
| `[LOD]` | `fLODFadeOutMultActors` | Actor distance (`actor-distance`) | part |
| `[LOD]` | `fLODFadeOutMultObjects` | Object distance (`object-distance`) | part |
| `[LOD]` | `fLODFadeOutMultItems` | Item distance (`item-distance`) | part |
| `[Grass]` | `fGrassStartFadeDistance` | Grass distance (`grass-distance`) | part |
| `[Water]` | `bUseWaterReflections` | Mirrored water reflections (`water-mirror`) | on when not 0 |
| `[BlurShaderHDR]` | `bDoHighDynamicRange` | Light glow (HDR) (`glow`) | on when not 0 |

**A part** (the distance sliders are parts of their full distance, `draw_distance.rs`): the
file's number divided by the largest number the preset files found give that key, so the
fullest preset draws at 100% and the others in proportion; the row then brings it onto its
own range and steps (at least 10%, in 5% steps). The original's own distances are not
copied: the sliders keep NVForever's full distances (`--radius`, `--shadow-distance`,
`--grass-distance`). When only some files are found, the largest of those found is the 100%.
A key that is 0 in every file found sets nothing (no 100% to scale by) and the log says so.

The shadow distance row follows `fShadowLODStartFade` as the nearest counterpart: NVForever's
sun shadows have no other quality key, and the original casts no sun shadows from the world (the Sun shadows row's
note, `engine/nvforever/src/settings.rs`) [design].

## Keys not used [design: #1966]

No NVForever row counts for these, so a preset leaves them unused:

- `[Display] iMultiSample`, `iWaterMultisamples`, `bTransparencyMultisampling`: the
  multisampling. Left to the Anti-aliasing row until the maintainer decides (#1966's
  question: whether presets should set anti-aliasing and FSR).
- `[Display] iMaxAnisotropy`: the anisotropic filtering is one of the enhanced effects
  (16x on, 8x off), and the presets never touch those.
- `[Display] iMaxDecalsPerFrame`, `fDecalLOD1`, `fDecalLOD2`: the impact decals have no
  settings row (impact-decals.md).
- `[Display] iTexMipMapSkip`, `iTexMipMapMinimum`: no row sets a texture size.
- `[Display] iActorShadowCountInt`, `iActorShadowCountExt`, `iShadowMapResolution`: the
  sun's shadow map size is fixed (`graphics.rs` `SHADOW_MAP`); no row counts actors'
  shadows.
- `[Display] fSpecularLODStartFade`, `fLightLODStartFade`: no row fades specular light or
  lights by distance.
- `[TerrainManager] fTreeLoadDistance`, `fBlockLoadDistanceLow`, `fBlockLoadDistance`,
  `fSplitDistanceMult`, `fHighBlockLoadDistanceLow`: distant land and trees are one switch
  (Distant terrain and objects), not a distance.
- `[Grass] b30GrassVS`: no counterpart.
- `[Imagespace] bDoDepthOfField`, `iRadialBlurLevel`: no row.
- `[Water] bUseWaterRefractions`, `bUseWaterReflectionBlur`, `bUseWaterDisplacements`,
  `bUseWaterDepth`, `iWaterReflectWidth`, `iWaterReflectHeight`, `bReflectExplosions`,
  `iWaterBlurAmount`, `bAutoWaterSilhouetteReflections`, `bForceHighDetailReflections`: the
  water has only the mirrored and the screen-space reflections as rows.

A key that is in neither list is logged as not known and left unused.

## The Quality row [design: #1966]

- Under its own heading, Graphics > Quality, the first row of the Graphics tab:
  `Low`, `Medium`, `High`, `Very High` (one per file found) and `Custom`. The names are our
  own words: the load order has no game setting for them [documented: FalloutNV.esm holds
  no "Very High" string, searched 2026-11-09].
- Taking a preset sets each row it maps through that row's own setter, as a change on the
  page would (each logged `settings: <key> <value>`, kept in the INI).
- The row **reads** the first preset whose values every mapped row (of those registered)
  holds, else Custom. Custom is what the row reads, not a choice to take: the list offers it
  only while it is read. Any change to a mapped row shows Custom; moving it back shows the
  preset again.
- A missing or unreadable file leaves its preset out of the list and says so once in the
  log (`quality presets: Medium is not offered: medium.ini not found in …`). With none of
  the four, the row is not registered at all (`… the Quality row is not shown`).
- Kept as `quality=<name>` in `[Settings]` of `NVForever.ini`. The stored name sets
  nothing at the start: the rows are kept on their own and are the truth, so a test run's
  command line still wins over the INI row by row (#1382).
- Defaults unchanged: nothing changes until the player takes a preset. With the owner's
  install, today's defaults (sun shadows on, Smooth filtering, light glow and mirrored water
  on, every distance at 100%) read as `Very High`.
- It never touches the enhanced effects, the anti-aliasing or FSR.

## The Enhanced effects row [design: #1966]

Under Graphics > Picture, its first row: `On` sets every switch `--graphics enhanced` turns
on and `vanilla` leaves off (ambient occlusion, longer sun shadows, bloom, sharpening,
anisotropic filtering 16x, sun shafts, screen-space water reflections; taken from
`graphics::Graphics::preset`, so a new effect joins by itself), `Off` clears them all, and
it reads `Custom` when they are mixed (then offered as the one chosen). The effects keep
their own rows. The light glow, eye adaptation, neon glow and mirrored water are on in both
looks, so they are not effects of it, and the two rows never set the same row. Kept as
`enhanced=On|Off|Custom`; a stored On or Off is set again at the start (its note names
`--graphics`, so a test run keeps the command line's look), and the effects' own stored
states, read after it, have the last word.

## Test aid and log

- `NVF_PLAY_TEST_QUALITY_PRESETS=<folder>` reads the preset files from that folder instead
  of the game folder.
- `quality presets: Low, Medium, High, Very High from <folder>; 9 key(s) set rows, 32 not
  used` at the start; `quality presets: <name> from <folder>: <n> row(s) set` when one is
  taken; `quality presets: the Quality row reads <name>` and `… the Enhanced effects row
  reads <name>` at the start and whenever it changes while the pause menu is open.
- Smoke: `settings-quality-presets`, `settings-quality-presets-missing`,
  `settings-quality-presets-part`, `settings-enhanced-effects`.
