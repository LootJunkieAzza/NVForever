# Authored PBR material maps (`textures\pbr\`)

NVForever's own file format for texture packs that give a surface physically based material
maps: occlusion, roughness and metallic, a normal map and a height map, beside the game's
diffuse texture (#2981). It is **NVForever's own format**: it describes no behaviour of the
original game, and no file the base game or its add-ons ship is known to lie under
`textures\pbr\` [hypothesis: not checked against the archives]. Every rule below is our
choice, marked **[design]**; facts about the texture formats cite their source
**[documented]**; what the engine does today cites its code.

Version 1. Pack authors may rely on the layout, member names, channels and scalar keys
below; a later version adds to them without changing their meaning.

## Decided (maintainer, 2026-11-20, #3104)

These choices are settled; pack authors and later versions can rely on them.

- [design, owner 2026-11-20, #3104] **Member names**: `<stem>_orm.dds`, `<stem>_n.dds`,
  `<stem>_p.dds` and `<stem>.toml`, with `<stem>_albedo.dds` reserved; the extension is not
  part of the stem, so `foo.dds` and `foo.tga` in one folder share one set (Layout, below).
- [design, owner 2026-11-20, #3104] **Normal map green points down**, the game's own
  convention, so a pack can start from the game's normal maps (Channels, below).
- [design, owner 2026-11-20, #3104] **DDS only**: members are DDS files; KTX2 is not read.
- [design, owner 2026-11-20, #3104] `GameData.vfs` stays a public field
  (`engine/nvf-assets/src/data.rs`); code that replaces the virtual file system goes
  through `GameData::set_vfs`, which drops the cached index of sets so it is rebuilt from
  the new one.

## What the engine implements at this stage

| Part | State |
|---|---|
| Path rules, member names, the index of sets a load order supplies | **implemented** (`crates/fnv-pbr`; `GameData::pbr_index` in `engine/nvf-assets/src/data.rs` builds it on first use and logs one line of counts; materials ask for it only while the Authored materials setting is on) |
| `fnvtool pbr list` (which sets a load order has, and which archive or folder supplies each file) | **implemented** (`crates/fnvtool/src/pbr_cmd.rs`, docs/cli-contract.md) |
| Scalar file reading (`<stem>.toml`) | **implemented**; read once per mount, by the first shape that asks (stage 3) [documented: `engine/nvf-assets/src/data.rs` `GameData::pbr_scalars`] |
| Reading members' pixels | `_orm` (linear) and `_n` (two-channel normal role, `TextureUsage::NormalTwoChannel`) **implemented** (stage 3); `_p` **planned** (stage 4); `_albedo` stays reserved |
| Reading BC7 and BC4 files (DDS with the DX10 header, or legacy `BC4U` / `BC4S`) | stage 1 of #2981 (`crates/fnv-dds`) |
| Drawing a surface with its set (ORM, normal) | **implemented** in source (stage 3) [documented: `engine/nvf-assets/src/nif_spawn.rs` `shape_material`, `authored.rs` `retexture`]; the setting Authored materials (`authored-materials`, on by default, read at the start; `--graphics-only no-authored-materials`). Its check is the field test `pbr-authored-field` (below). A sample pack was drawn in the game with the setting off and on by the main session as of head `bebd7841a` (pictures not committed; they show the pack's own values, and no reference look exists to judge them by); those pictures are owed again after the fixes that followed that head |
| Height maps (`_p`, parallax) | **planned** (stage 4, with #2979) |
| Metallic | read with the ORM but **held at 0** (stage 3, implemented) until the engine has a reflection source (stage 5) |
| A pack validator (`fnvtool pbr check`, `fnvtool pbr selftest`) | **implemented** (`crates/fnvtool/src/pbr_check.rs`, docs/cli-contract.md `fnv.pbr.check/1`) |

Sections marked "planned" describe how the engine will use a set once those stages land.
Wherever this note says a member "will be read" or "will be accepted", stage 3 does so for
`_orm` and `_n`; `_p` waits for stage 4.

## Layout

[design] Every file of a pack lives under `textures\pbr\`, in the same folders as the game
diffuse it belongs to. The set's **stem** is the diffuse's path with its extension removed
(whatever it is), placed under `textures\pbr\`:

| Diffuse the game draws | Stem |
|---|---|
| `textures\architecture\urban\sidewalkquad01.dds` | `textures\pbr\architecture\urban\sidewalkquad01` |
| `textures\clutter\food\tincan.tga` | `textures\pbr\clutter\food\tincan` |

A set's members are the stem plus one suffix:

| File | Role | Required |
|---|---|---|
| `<stem>_orm.dds` | red: ambient occlusion; green: perceptual roughness; blue: metallic | makes the set **authored** |
| `<stem>_n.dds` | tangent-space normal, two channels (X in red, Y in green), green pointing down like the game's normal maps [observed: docs/spec-notes/nif.md] | optional |
| `<stem>_p.dds` | height, brighter is higher | optional; alone it makes a **height-only** set |
| `<stem>.toml` | scalars (below) | optional |
| `<stem>_albedo.dds` | **reserved** for a later version: found and counted, never activates a set, will not be read in version 1 | — |

- [design] A set with `_orm` is authored. A set with `_p` and no `_orm` is valid: the game's
  own material with height only (when height maps land). A set with only `_n`, `.toml` or
  `_albedo` does nothing; the engine counts it as incomplete.
- [design] There is **no plain-name file**: `<stem>.dds` is not a member. In version 1 the
  base colour is always the game's own diffuse (or whatever diffuse replacer wins in the
  load order).
- [design] Paths are matched without regard to slash direction or ASCII case, as every
  game path is (`fnv_vfs::normalize`): only `A`-`Z` fold to `a`-`z`; other letters (accented
  or non-Latin) must match exactly. A diffuse path a mesh writes with a leading `data\` or an
  absolute folder is cut at `textures\` first, as for the diffuse itself
  (`fnv_vfs::texture_path`).
- [design] The extension is not part of the stem, so `foo.dds` and `foo.tga` in one folder
  share one set. That is accepted: a pack cannot give the two different sets.
- [design] The key is the diffuse **as the engine resolves it**: after record texture swaps,
  after the first-person twin is chosen, and after a console path (`\xbox\`) is replaced by
  the PC copy, which happens only when the PC diffuse exists (`resolved_path`,
  `engine/nvf-assets/src/texture.rs`). A set under `\pc\` therefore applies only where the PC
  diffuse is drawn.
- [design] **First person**: when the diffuse's file name starts with `1stperson` and there is
  no active set for it, the set of the same name without the prefix is used. One set
  serves a weapon in both views; a pack can still give the first-person texture its own.

### Reading a file name back: one suffix, once

[design] A file under `textures\pbr\` is assigned to a set by removing **exactly one**
suffix from the end of its file name (the last path component; folder names are never
peeled): `_orm.dds`, `_n.dds`, `_p.dds`, `_albedo.dds` or `.toml`, compared after ASCII case
folding. What is left must be a non-empty name; it is the stem. The suffix is never
removed twice:

| File under `textures\pbr\walls\` | Set (diffuse) | Member |
|---|---|---|
| `foo_n.dds` | `foo` | normal |
| `foo_n_orm.dds` | `foo_n` (a diffuse named `foo_n.dds`) | ORM |
| `foo_n_n.dds` | `foo_n` | normal |
| `foo.dds` | none: **unmatched** | — |
| `foo_orm.png`, `readme.txt` | none: unmatched | — |

Unmatched files are ignored and counted (`fnvtool pbr list` names them).

## Channels and colour spaces

[design] The colour space follows the member's **role**, not the DDS header: every member
of version 1 (`_orm`, `_n`, `_p`) will be read as linear data, even when a DX10 header names
an `_SRGB` format. Authors should still export them as linear (non-sRGB) so other tools show
them correctly.

- `_orm`: red occlusion (1 = unoccluded), green perceptual roughness (0 smooth, 1 rough),
  blue metallic (0 dielectric, 1 metal). The alpha channel will not be read.
- `_n`: X in red, Y in green, each 0..1 standing for -1..1; Z is rebuilt from them. Green
  follows the game's convention, +G pointing down the image [observed: docs/spec-notes/nif.md,
  "Normal maps", from the game's normal and height map pairs], which the engine follows by
  flipping Y for every NIF normal map (`flip_normal_map_y` in
  `engine/nvf-assets/src/nif_spawn.rs`), so a pack can start from the game's normal maps.
  A three-channel normal map (BC7, BC1, BC3) will be accepted and read as X, Y, Z
  (stage 3); its alpha will not be read.
- `_p`: one channel, red; brighter is higher.

## Compression and sizes

- Recommended formats: `_orm` **BC7** (or BC1 when its quality is enough), `_n` **BC5**
  (unsigned), `_p` **BC4** (unsigned). BC1 and BC3 will be accepted for `_orm` and `_n`
  from stage 3, and for `_p` when height maps land (stage 4); `_albedo` stays reserved.
  [documented: Microsoft, "Texture Block Compression in Direct3D 11": BC4 stores one channel
  and BC5 two in 4x4-texel blocks of 8 and 16 bytes; BC7 stores RGB or RGBA in 16-byte
  blocks]
  https://learn.microsoft.com/en-us/windows/win32/direct3d11/texture-block-compression-in-direct3d-11
- BC7 files carry the DDS DX10 extension header [documented: Microsoft,
  "DDS_HEADER_DXT10"]. BC4 and BC5 files may carry it too, or a legacy header with the
  FourCC `BC4U` / `BC4S` (BC4) or `ATI2` / `BC5S` (BC5) [documented: Microsoft,
  "Programming Guide for DDS"; docs/spec-notes/dds.md, docs/spec-notes/dds-bc5.md]. The
  engine reads the DX10 header and the BC4 FourCCs from stage 1 of #2981 on.
- [design] The **top level's width and height must be multiples of 4**. The engine has no CPU
  decoder for BC7, so a file it cannot upload whole will be refused rather than decoded.
- [design] Ship **full mip chains**, down to 1x1. Smaller mip levels need not be multiples of 4.
- [design] Each member's size is its own: an ORM at 2048 beside a 1024 diffuse and a 4096
  normal map is fine. Nothing ties the sizes together.
- Before shipping, run `fnvtool pbr check <folder>` on the pack (the mod folder, its
  `textures` folder or the `pbr` folder). It reads each file's DDS headers and size and the
  scalar files: formats per member, sides that are multiples of 4, mip chains, cut-off or
  padded files, `_SRGB` codes, incomplete sets and unmatched files, and it estimates the
  pack's GPU memory; it exits non-zero on an error. It does not decode or judge the maps'
  content (whether the roughness or normals look right): view them in the engine.

## Scalars: `<stem>.toml`

A TOML file of plain keys, all optional:

| Key | Meaning | Range | Default |
|---|---|---|---|
| `parallax_depth` | depth of the height map's darkest point below the surface, in UV units (planned, stage 4) | 0 to 0.25 | the engine's own depth |
| `roughness_scale` | multiplies the ORM's roughness; the product is held to 0..1 when drawn, and to 0.089..1 for the highlight's shape (below) | 0 to 2 | 1.0 |
| `reflectance` | the highlight strength of non-metals; F0 = 0.16 x reflectance² (0 to 16 %) | 0 to 1 | 0.5 (F0 4 %) |

```toml
# textures\pbr\walls\brick07.toml
roughness_scale = 0.9
reflectance = 0.35
```

[design] Reading is lenient (`fnv_pbr::parse_scalars`): an integer counts as a number; an
unknown key, a table, or a value that is not a finite number is ignored with a warning; a
number outside its range is clamped into it with a warning; a file that is not UTF-8 or
not TOML is ignored whole with a warning. A UTF-8 byte order mark is accepted.
`normal_strength` is not a key: bake the strength into the normal map.

## What a set changes on a surface (stage 3)

[design] When the engine draws a surface whose diffuse has an authored set:

- **Switched off**: the shape's glossiness, the normal map's alpha as a highlight mask, and the
  shape's Specular flag. Roughness comes from the ORM (times `roughness_scale`), the highlight
  strength from `reflectance`. A surface the game drew without highlights therefore gets the
  default 4 % F0 once authored; `reflectance = 0` keeps it matte.
- **Replaced**: the normal map, when the set has `_n`; otherwise the shape's own normal map
  stays, without its alpha mask.
- **Kept**: the base colour (the diffuse), the glow map, alpha blending and alpha testing (from
  the diffuse's alpha, as before), the decal flag, and environment maps once the engine draws
  them (it does not yet: `engine/nvf-assets/src/nif_material.wesl` header).
- **Not covered in version 1**: actor skin, hair and FaceGen heads; terrain layers; distant
  LOD; effects (unlit, additive, refraction, particles); the Pip-Boy and interface textures.

## How the engine lights a set

Maps tuned for other renderers need these notes:

- The diffuse light follows the engine's model of the original's: lights are summed in
  display values and the diffuse is multiplied by that sum raised to 2.2, not by a linear
  light sum (`engine/nvforever/src/lights/display_shader.rs`). That the original lights
  surfaces this way is itself a [hypothesis, fitted to the original's frames]
  (docs/spec-notes/world.md, "Light on surfaces in display values").
- Ambient occlusion (ORM red) darkens only the ambient part of that sum, not the sun or
  lamps (`display_shader.rs`, `diffuse_occlusion`).
- The highlight is GGX with the Fresnel factor held at F0 in world views (no brightening at
  grazing angles; `FRESNEL_EDIT`, `engine/nvforever/src/lights.rs`).
- Roughness is perceptual: the ORM's green is multiplied by `roughness_scale`, and the
  surface shader holds the product to 0..1 right after sampling
  (`engine/nvf-assets/src/nif_material.wesl`): a scale above 1 on a rough texel passed 1,
  and the environment term's lookup (`F_AB`, `pbr_functions.wesl`), which takes the value
  as it is, then left its range. [observed: 2026-11-20, the set `offrmfloor01` in the Vault
  21 gift shop with its roughness scaled 2 and more drew white patches before the cap and
  none after, pictures at scales 2, 2.5 and 3.] For the highlight's shape the product is
  further raised to at least 0.089 and then squared (Bevy's
  `perceptualRoughnessToRoughness`, `engine/vendor/bevy_pbr/src/render/pbr_lighting.wesl`),
  so a product below 0.089 draws a highlight as narrow as 0.089's. A raw green below 0.089
  can still end above that floor with a scale above 1.
- **Metallic** will be read but **held at 0** in version 1: with no reflection source in the
  engine, a metal would draw black except for lamp highlights. Until a later version adds
  one, a metal surface draws as a dielectric of its diffuse colour with the authored
  roughness. Author the blue channel correctly anyway; it will be used.

## Where a set does not apply, and checking one (stage 3)

What the engine's source does with a set, as written (not yet compared with a running game
beyond the field test below):

- [design] Which shapes take a set [documented: `engine/nvf-assets/src/authored.rs`
  `shape_eligible`, `key_eligible`; `engine/nvforever/src/actors.rs`]: lit NIF shapes with
  texture coordinates. Never: unlit, additive, multiplied or refracting shapes, particles,
  hair (the hair shader, and the hair model's shapes once an actor wears it), skin (the skin
  shader, and any shape whose diffuse is the actor's race body or hand texture or the head's
  face texture, from the moment that is known, tinted or not), landscape LOD
  (`meshes\landscape\lod\`), and diffuses under `textures\pipboy3000\` or
  `textures\interface\`. Unlit copies made later (the Pip-Boy's and the terminal's screens)
  drop any set.
- [design] The normal map [documented: `authored.rs` `apply`]: the set's `_n` when it has one
  and the drawn mesh has tangents; otherwise the game's own normal map, whose alpha is then
  no specular mask. Tangents are made for a shape whose first normal map is the `_n` when its
  material is made; where a later change of diffuse (a script's texture swap) or a copy of a
  drawn mesh would bring the first normal map to a mesh without them, the `_n` is not applied
  (logged once per set) and the rest of the set is.
- [design] A swapped diffuse takes its own set, or none, from the shape's legacy look; it
  never keeps the previous diffuse's maps [documented: `authored.rs` `retexture`]. A script's
  change of a shape's glossiness goes to the legacy look while a set is on it, so the set's
  roughness stays [documented: `engine/nvforever/src/nif_mirror.rs` `set_material`]. A
  specular colour the script set itself is drawn at once and kept as an override that a set
  put on later (a swap's) keeps drawing; the stored colour the mirror writes back with an
  edit of another value never scales a set's highlight [documented: `authored.rs`
  `set_legacy_specular_tint`, `apply`, `LegacyLook::scripted_tint`]. Several edits of one
  shape in one mirror update go to one private copy [documented: `nif_mirror.rs`,
  `SceneEdit::Material`].
- The setting Authored materials (Graphics > Detail and Distance) is read once at the start.
  Off, the material path builds no index, loads no file under `textures\pbr\` and keeps no
  legacy look [documented: `authored.rs` `find`, `engine/nvf-assets/src/nif_spawn.rs`].
- A set's scalar file is read and its warnings logged once per mount: by the first shape
  that asks, on whichever thread; a new mount of the files reads it afresh [documented:
  `engine/nvf-assets/src/data.rs` `GameData::pbr_scalars`].
- The smoke scenarios `pbr-authored-field` and `pbr-authored-field-off`
  (`NVF_PLAY_TEST_PBR_FIELD=1`, `engine/nvf-assets/src/pbr_field.rs`) are the check of the
  drawn result: generated sets through the real material path on five panels (plain; lit,
  with `_orm` and a two-channel `_n` decoded on a worker; dark, as lit but for the ORM's
  red; reference, lit's `_orm` with the same normal as a three-channel game map; flat,
  `_orm` without a normal map), two GPU readbacks judged by mean luminance. [observed: on
  `7e2de4be1`, the main session's runs, two readbacks each, identical: on, plain 103, lit 67,
  dark 38, reference 67, flat 103; off, plain, lit, dark and flat 103, reference 60.] The
  comparator's thresholds were set from those readbacks with margins [engineering policy:
  occlusion at least 15, lit and reference within 3, reference at least 15 darker than flat
  (on) or plain (off), the off panels within 2]. [observed: the main session's smoke run
  at `c512cd3c8`, the head with the final review's fixes bar the last change to the swap's
  source lookup: `pbr-authored-field` and `pbr-authored-field-off` pass with those
  thresholds, and `rorschach-cards`.]
- Calibration aids, off by default, read at the start [documented: `authored.rs` `PbrView`,
  `metallic_debug`; `engine/nvf-assets/src/nif_material.wesl`]: `NVF_PBR_VIEW` =
  `roughness`, `metallic`, `occlusion`, `normal` or `authored` draws that input in place of
  the lit colour of forward-drawn lit NIF surfaces. Refracting shapes return before the
  view; additive draws add the view's colour by their alpha and multiplied draws stay
  neutral, so effects fade out of it; depth and coverage are unchanged.
  `NVF_PBR_METALLIC=1` builds authored materials metallic by the ORM's blue channel and keeps
  a metal's ambient highlight, a preview of a later metallic stage [design].
- On the app's exit the log has one line `authored materials summary: ...` (or
  `authored materials summary: off (setting)`): counts of build events, not of visible
  shapes: material builds that applied a set, the distinct set stems seen at those builds,
  builds that skipped a set for one of the recorded reasons (other misses are not counted),
  `_orm` load failures (at a build or a swap alike), role copies made legacy, and calls of
  `retexture` on a shape with a saved legacy look [documented: `authored.rs` `Stats`, `report`].
  Rebuilt or discarded materials count again.

### Known limits (stage 3)

From the source reviews of 2026-11-19 and 2026-11-20 [documented: the files named]. Two
are not fixed:

- **A script's material edit during a fade.** The property mirror
  (`engine/nvforever/src/nif_mirror.rs`) makes the shape's private material from the
  material it wears, which during a fade is the fade's copy, while the fade
  (`engine/nvf-play/src/fade.rs` `Fading`, `engine/nvforever/src/effect_shaders.rs` `Faded`)
  keeps the older original and puts it back when it ends. The edit is then lost: on an
  authored shape the set's roughness first looks right, and a later swap to a diffuse
  without a set brings back the gloss stored before the edit. Fixing it means the fades
  rebuilding their copies from a per-shape material, a change to the fade system itself.
- **A placed shape the mirror gave a private material, then swapped.** The swap matches
  shapes by their original material, and a placed shape without a shape name no longer
  matches once the mirror replaced it, so its old diffuse stays drawn with its authored
  fields (set, mark, metallic). This predates stage 3 (`texture_swap.rs`, `nif_mirror.rs`).

What holds for a texture swap while the shape fades, and what does not:

- **SetActorAlpha and a fade on one mesh, then a swap.** Whichever of the two started
  second copied what the mesh wore then, so its stored original can be the other's
  see-through copy. The swap matches and copies the mesh's own material beneath both
  (`engine/nvforever/src/effect_shaders.rs` `persistent_material`, `texture_swap.rs`), so
  the swapped material carries the new diffuse with the mesh's own alpha, blend mode and
  depth write, in both start orders [documented: test `texture_swap.rs`
  `a_swap_under_both_fades_copies_the_mesh_own_material_in_either_start_order`, through
  `Swapping::sync`, the fade's step and `AlphaDrawing::sync` on a named shape, with the
  fade stepped before the actor's alpha in that fixture; not seen in the game]. An unnamed
  shape is matched by the same handle, its model's material [hypothesis, from the source:
  the test has no model-backed unnamed shape]. When the actor's alpha outlasts one fade and
  a second fade runs, the alpha's stored original is the first fade's removed copy; the
  swap then takes the second fade's original [documented: test
  `a_swap_during_a_second_fade_under_one_actor_alpha_copies_the_mesh_own_material`]. Two
  sequences still give the swap a wrong source [hypothesis, from the source; neither system
  keeps the mesh's own handle apart from its copies]: the alpha outliving a fade with no
  second fade running (its stored original is a removed copy, so the swap is not made), and
  the alpha released while a fade that started second still runs (the fade's original is
  the alpha's copy, so the swapped material is see-through). Both go with the issue below.
- **Not changed, and not whole: how the two see-through copies hand the mesh back.** With
  no swap, two orders leave the mesh wrong after both have ended: the actor's alpha started
  first and released first (the fade's end puts the alpha's copy back, the mesh stays
  see-through), and the fade started first and ended first (the alpha's release puts back
  the fade's copy, which the fade removed). The other two orders, and all four after a swap,
  end on the mesh's own or swapped material, opaque [documented: the same test's `HELD`
  list, each change in its own update, in the fixture's order of the two systems]. While
  both run the two factors are not consistently composed: a fade that starts under the
  actor's alpha multiplies the alpha copy's opacity, but an alpha that starts or changes
  under a fade replaces what is drawn. The game declares no order between the fade's step
  and the actor's alpha, and two ends in one update are not covered by the test
  [hypothesis, from reading `fade.rs` `step` and `effect_shaders.rs` `AlphaDrawing::sync`].
  This predates stage 3 and needs one
  owner of the drawn material (base opacity x actor alpha x fade); it is left for its own
  issue.

## Load order: per file, so ship complete sets

[design] Each file resolves on its own through the virtual file system, like every other
texture: plugin archives in load order, then `Data`'s loose files, then NVForever's mods
folder or a Mod Organizer 2 profile's mods (docs/design/mod-folders.md). Two packs can
therefore mix, one's `_orm` with another's `_n`. **Ship complete sets**, and use
`fnvtool pbr list` to see which pack supplies each file.

What is true of archives today (`Vfs::archive_order_in`, `Vfs::archive_files_layered`,
`crates/fnv-vfs/src/lib.rs`):

- Loose files always load, from `Data`, the mods folder or a Mod Organizer 2 mod.
- A pack in a `.bsa` is mounted only when the archive belongs to an **active plugin** by
  name (`<plugin>.bsa`, `<plugin> - <anything>.bsa`, or its numbered pieces), lying in
  `Data`, in NVForever's mods folder or in a Mod Organizer 2 mod; when the archive list
  names it (`[Archive] SArchiveList` in NVForever.ini, for an archive in `Data`; or a Mod
  Organizer 2 profile's ticked archives, in any of its layers); or when it is `Update.bsa`,
  which is mounted after the base list (from `Data`, or under Mod Organizer 2 from the
  highest layer that has it) unless the base list names it, which keeps it at that place,
  or the `NVF_ARCHIVE_ORDER` override sets the whole order without it. A plugin that cannot
  be read or lacks a master is left out, and the archives it owns by name with it; an
  archive list that names one of those archives still mounts it. A lone `.bsa` that none
  of these covers is not mounted.

## Log line and listing

The first time the index is asked for, the engine logs one line, e.g.
`authored materials: 12 sets under textures\pbr\ (11 with _orm, 1 height only; files: 11 _orm,
9 _n, 3 _p, 2 .toml, 0 _albedo reserved), 1 incomplete sets and 2 unmatched files ignored`.
`fnvtool pbr list --data <Data> [plugins…] [--mods <folder> | --mo2 <instance>]` prints
every set with its state and, per member, the archive or loose folder that wins
(`fnv.pbr.listing/1`, docs/cli-contract.md).
