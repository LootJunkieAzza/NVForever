# Vertex morphs (`NiGeomMorpherController`, `NiMorphData`)

Waving flags, the Big MT's holograms and a few wires and tarps change the shape of their
mesh over time by blending stored vertex sets. This note describes the blocks, what the
base game uses them for and how NVForever plays them (no-owner queue item 17, 2026-10-20).

**Sources.** niftools nif.xml (read as documentation only; the layouts are in
`schemas/nif/anim.json`); `crates/fnv-nif/examples/morph_survey.rs` over the base
archives `Fallout - Meshes.bsa`, `Update.bsa` and `Fallout - Misc.bsa` (no add-on
archive read) [observed]; `fnvtool nif decode` for single files; `fnvtool world refs
--world WastelandNV` over `FalloutNV.esm` for the placed references.

## 1. The blocks

- **`NiMorphData`**: a morph count, a vertex count, a "relative targets" byte ("always
  1 in all official files") and the morphs; each **`Morph`** has a frame name and one
  vector per vertex [documented: nif.xml NiMorphData, Morph]. (For bs_version 14-34 there
  is no legacy weight field [documented: nif.xml `vercond`].)
- **`NiGeomMorpherController`** (a `NiInterpController`, so it has the NiTimeController
  flags, frequency, phase, start and stop time and a target, the shape): morpher flags
  (0 UPDATE_NORMALS_DISABLED, 1 UPDATE_NORMALS_ENABLED), the data, an "always update"
  byte, and in 20.2.0.7 files one **`MorphWeight`** per morph: an interpolator reference
  and a float weight [documented: nif.xml NiGeomMorpherController, MorphWeight,
  GeomMorpherFlags]. Morph `i` takes its weight from the `i`-th entry [hypothesis: nif.xml
  does not say how they pair].
- **Sequences**: a `NiControllerSequence` drives a manager-controlled morpher with one
  controlled block per morph, controller type `NiGeomMorpherController`, the shape's
  name as node name, no controller ID, and the morph's frame name as **interpolator ID**
  [observed: `utilitypolewsmall01nv.nif`'s `Idle` lists `Base`,
  `UtilityPoleWSmall01WireM01` and `…M02`, the frame names of its three morphs]. The
  controller's own `MorphWeight` interpolators are then `NiBlendFloatInterpolator`s
  without a value [observed]. `Track::interpolator_id` (fnv-nif) carries the ID.

## 2. What the data looks like [observed, `morph_survey.rs`]

Every controller in a base-archive `.nif` (64 blocks in 39 files):

- has relative targets 1 and morpher flags 0 (normals not updated);
- has morph 0 named `Base`, equal, to the bit, to the shape's
  stored vertices, and later morphs that are offsets (up to 5.3 units on the NCR-style
  flags, 78.5 on the Legion and Great Khan flags, 18-30 on the wires; effects up to 2,470);
- weighs its morphs with `NiFloatInterpolator`s (free ones) or by a sequence (manager
  controlled ones).

Hence the vertex position is morph 0 plus the sum of every later morph times its weight
[hypothesis: the formula is not written in nif.xml; it is the reading that leaves the
shape as stored when every weight is 0 and that keeps the offsets small].

| How it runs | Files (blocks) | Examples |
|---|---|---|
| Own controller (Active, not manager controlled), moving | 19 (36) | `clutter\flags\nv_ncr_flag.nif`, `nv_ncr_flag_s.nif`, `nv_ncr_flag_s_flipped.nif`, `nv_nevadaflag.nif`, `nv_foa_flag.nif` (skinned, 7 morphs, clamped -2 s…22.8 or 34.7 s); `furniture\nv_legionflag.nif`, `nv_legionflag_largnopole.nif`, `clutter\nvgreatkhanflag\nvgreatkhanflag.nif` (14 morphs, looping 9.73 s); 10 Big MT holograms `dlc05\dungeons\mz\misc\dlc05holo*.nif` (looping 1.3-3 s, a jump at the loop); `animobjects\hchamberfxlso.nif` (clamped 66.7 s); `weapons\1handpistol\dartgun.nif` (the band, 0.03 s) |
| Manager controlled (a sequence) | 18 (26) | the wires of `utilitypolewsingle01nv.nif`, `…single02nv`, `…small01nv`, `…small02nv` (Idle, looping 10.67 s); `greenhousenvint.nif` tarp; `nv_ncr_hangfromtop.nif`, `…02`; `traps\tripwire01.nif`; `dlcpitt\clutter\press\dlcpittconveyorpress*.nif`; `effects\fxharoldheart01.nif`; `b29_riseanim.nif`; `nvnellisartillery.nif`; `effects\nv\…` fog, blood, shimmer |
| Neither | 1 (1) | `effects\explosionsplash.nif` |
| Own controller, still | 1 (1) | `characters\_male\skeleton.nif` (3 vertices, face-bone helper) |

`.kf` files: 1,170 sequence tracks with `NiFloatInterpolator` and 23 with
`NiBSplineCompFloatInterpolator` drive morphers (the 1st-person `##dgRubberBand:0` of the
dart gun's clips and the like); not played here (weapons).

The wire poles placed in the Mojave (`utilitypolewsingle01.nif` 72 references,
`…single02` 37, `…small01` 27, `…small02` 22, `utilitypolewirenv.nif` 32) are the models
without `nv` and have **no** morphs; the `…nv.nif` ones with morphing wires are not placed
in WastelandNV [observed]. So the Mojave's wires do not swing in the original either,
as far as the data shows [hypothesis]. Placed morphing models in WastelandNV
[observed]: 27 `nv_legionflag.nif` (The Fort), 2 `nv_legionflag_largnopole.nif`, 2
`nvgreatkhanflag.nif`, 1 `nv_nevadaflag.nif` (Goodsprings, ref 0x0010A18D), 13
`greenhousenv.nif` (not the `…int` file with the tarp).

## 3. What NVForever plays (`nvf-assets` `model::shape_morphs`, `morph.rs`)

Taken when a model is prepared, per shape:

1. an own controller (Active, not manager controlled), or a manager-controlled one fed
   by the file's `Idle` (else `SpecialIdle`) sequence, its timing then the sequence's
   (start, stop, frequency, cycle type);
2. relative targets, at least two morphs, every morph with the shape's vertex count, and
   a weight that changes over the span;
3. clamped controllers only when every weight ends where it started (the NCR, Nevada and
   Followers flags: they are played looping) [hypothesis: those are cycles meant to
   repeat; a world object's clamped controller on the game clock would have ended before
   anyone sees it]. Other clamped ones (the tripwire's snap, the dart gun's band, the
   vault chamber's 66.7 s shock) are left for whatever starts them.

The shape's mesh is kept readable in the main world (`RenderAssetUsages::all()`), and
`morph::play_vertex_morphs` rewrites its positions whenever the weights at the game's
clock (`Time::elapsed_secs`, paused with the game) differ from the last ones written;
Bevy then recomputes the mesh's bounds. Normals are left as stored (morpher flags 0
everywhere). A skinned shape (the NCR-style flags) morphs its rig copy too, before
skinning.

Every placed copy of a model shares the mesh, so they wave in step [hypothesis: in the
original a controller runs on the application clock unless something sets its phase;
not compared in a capture]. Log lines: `<model> <shape>: vertex morphs, N morph(s) over a
S s cycle (own controller | idle sequence)` when read, `vertex morphs: <model> <shape>
running (…)` the first time its weights change (smoke `vertex-morphs`).

## 4. Left open

- Morphs a script or a non-idle sequence starts (the tripwire, the Pitt press, the
  B-29's rise, Harold's heart, the explosion splash) and the 1st-person `.kf` morphs.
- Whether the original's flags are out of step with each other (per-reference phase).
- Updating normals (no base-game file asks for it).
