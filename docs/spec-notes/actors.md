# Spec notes: actor appearance (workstream 5) — what makes up a placed NPC

Clean-room description in plain language. Tags: **[documented]** cites a public source,
**[observed]** was measured on the owner's install (FNV 1.4.0.525, the ten masters as in
docs/spec-notes/simulation.md) with `fnvtool actor forms | appearance | meshes` (§5),
**[hypothesis]** needs verification against the original game. Owner statements are
marked as such with their date.

Record layouts come from the Layer 1 schema (`schemas/fnv`, re-expressed from xEdit
`wbDefinitionsFNV.pas` / `wbDefinitionsCommon.pas`, branch `dev-4.1.6`, as
documentation). FaceGen file layouts come from the FaceGen SDK manual's public file
format page (§4).

## 1. Data

### 1.1 Records

- **NPC_** [documented: xEdit]: `MODL` skeleton, `RNAM` race, `HNAM` hair (HAIR),
  `LNAM` hair length, `HCLR` hair colour (RGB + unused byte), `ENAM` eyes (EYES), repeated
  `PNAM` head parts (HDPT), `FGGS`/`FGGA`/`FGTS` FaceGen coefficients (50/30/50 floats),
  `NAM6` height, `CNTO` carried items, `TPLT` template (NPC_ or LVLN). ACBS flags bit 0
  is Female; ACBS template flags select which groups come from the template: bit 0
  Traits, bit 6 Model/Animation, bit 8 Inventory (others: stats, factions, …).
- Which fields each template group governs [documented: xEdit hides these fields when
  the group's flag is set]: **Traits** — race, the Female flag, voice, height, weight,
  karma, death item; **Model/Animation** — `MODL`, head parts, hair, hair length, eyes,
  hair colour, FaceGen data, impact material; **Inventory** — `CNTO`.
- 4,220 NPC_ records [observed]. `TPLT` names an NPC_ in 2,204 and an LVLN in 703.
  Template use by group: none 1,313; a `TPLT` but no group flag 482; Inventory only
  1,295; Traits+Model+Inventory 712; Traits+Model 190; Model only 91; Model+Inventory 48;
  Traits+Inventory 77; Traits only 12 [observed].
- Skeletons named by NPC_ `MODL`: `characters\_male\skeleton.nif` 3,534,
  `marker_creature.nif` 678 (an editor placeholder), none 8 [observed]. Of the 89 NPC_
  with Traits but not Model/Animation from a template, 22 name the placeholder; 21 of
  those still have their own FaceGen data [observed].
- **RACE** [documented: xEdit]: `DATA` (male/female height and weight, flags), `DNAM`
  default hair per sex, then positional sections: `NAM0` opens the head data and `NAM1`
  the body data; in each, `MNAM` then `FNAM` start the male and female lists of parts,
  each part an `INDX` followed by `MODL` and/or `ICON`. After the body data come `HNAM`
  (allowed hairs) and `ENAM` (allowed eyes), then `MNAM`/`FNAM` again, each followed by
  that sex's FaceGen data (`FGGS`, `FGGA`, `FGTS`, `SNAM`).
  - Head part indices: 0 Head, 1 Ears, 2 Mouth, 3 Teeth Lower, 4 Teeth Upper, 5 Tongue,
    6 Left Eye, 7 Right Eye [documented: xEdit wbHeadPartIndexEnum].
  - Body part indices: 0 Upper Body, 1 Left Hand, 2 Right Hand, 3 Upper Body Texture
    [documented: xEdit wbBodyPartIndexEnum, non-Oblivion branch].
  - CaucasianOld [observed]: head `Characters\Head\HeadHuman.NIF` with ICON
    `Characters\Male\HeadHumanOld.dds` (the age-specific face texture); ears have an
    ICON but no model; mouth, teeth, tongue and eyes have models; body 0–2
    `characters\_Male\UpperBody.nif`, `LeftHand.nif`, `RightHand.nif` with ICONs; body 3
    has a `MODL` that is an `.egt` (`Characters\_Male\UpperBodyHumanMale.egt`).
  - 31 races [observed].
- **HAIR** [documented: xEdit]: `MODL`, `ICON` ("Texture"), `DATA` flags (Playable, Not
  Male, Not Female, Fixed). 67 records [observed].
- **EYES** [documented: xEdit]: `ICON` ("Texture"), `DATA` flags (Playable, Not Male,
  Not Female). An eye texture: the eye meshes come from the race. 12 records [observed].
- **HDPT** [documented: xEdit]: `MODL`, `DATA` playable, repeated `HNAM` extra parts
  (HDPT). Beards and eyebrows (`BeardMustacheCurly.nif`, `EyebrowM.nif`). 61 records
  [observed].
- **ARMO** [documented: xEdit]: `BMDT` biped flags (bit 0 Head, 1 Hair, 2 Upper Body,
  3 Left Hand, 4 Right Hand, …, 9 Headband, 10 Hat, 11 Eye Glasses, 14 Mask, …) and
  general flags; worn (biped) models `MODL` male and `MOD3` female, ground models `MOD2`
  and `MOD4`; `DNAM` DR (stored as an integer that xEdit shows divided by 100) and DT.
  Leather, Metal, Combat, T-51b and the Field Hand outfit all store DR 0 and DT 6, 12,
  15, 25 and 2 [observed]. `MODS`/`MO2S`/`MO3S` (alternate textures) do not decode with
  schema v1 yet (length-prefixed strings; layer1.md).
- All 3,942 placed ACHR in the ten masters have an NPC_ base (none an LVLN) [observed].

### 1.2 Meshes

Measured over the 427 (skeleton, part) pairs that placed NPCs use [observed, `fnvtool
actor meshes`]:

- **Skinned parts** — upper bodies, hands, heads, 271 worn-armour models: shapes with a
  `BSDismemberSkinInstance` whose bones are NiNodes copied into the part's own file
  (`lefthand.nif` holds 18 nodes, `upperbody.nif` 27). Every bone name a skin uses exists
  in the actor's skeleton.
- **Rigid parts** — hair (58 models), head parts (21), eyes, mouth parts, 41 armour
  models (hats, helmets, glasses): unskinned. Most have a root `NiStringExtraData` named
  `Prn` whose string names the parent bone (`Bip01 Head` for hair, beards and eyes); 19
  headgear models (cowboy hats, fiend helmets, NCR trooper helmets, berets) and the
  child-female mouth parts have none.
- A rigid part's vertices end up in the **skeleton's axes** with the origin at the
  parent bone: hair spans z −0.7…15.3 and a cowboy hat z 3.8…17.5 above `Bip01 Head`
  (bind position (0, 0.4, 112.8) in `skeleton.nif`), while the head bone's own x axis
  points up. Node rotations of the 206 rigid shapes placed NPCs use: all identity 112
  (hats, helmets, beards); root and shape rotations that cancel 66 (most hair, e.g.
  `hairbun.nif`); a root equal to the inverse of `Bip01 Head`'s bind rotation with
  identity below 16 (eyes and the other race head parts); identity root with real
  offsets below 12 (Pip-Boy arm parts). Rule [observed, all four kinds render in place]:
  a part whose root stores the bone's inverse bind rotation is an ordinary child of the
  bone; any other part gets that inverse rotation from the attachment; node transforms
  are kept in both cases.
- Hair-shader shapes (BSShaderFlags bit 18) carry a mask in their vertex colours, not a
  colour: red 0.04–0.39 with green and blue at 1 in `hairbase.nif` and
  `beardmustachecurly.nif` [observed]; multiplied in, hair turns cyan. They are not
  applied [hypothesis: what the mask does in the game's hair shader is open].
- Every hair model has a `NoHat` and a `Hat` shape (58 of 58). Upper bodies and armour
  carry dismemberment caps (`meatneck01`, `meathead01`, `bodycaps`, `limbcaps`,
  `MeatCap*`).
- Every model and every texture override named by the resolver exists in the archives
  (80 textures).
- FaceGen companions: `.egm` (275), `.egt` (36), `.tri` (49) files in
  `Fallout - Meshes.bsa`, next to heads, hair, beards and many hats; no pre-generated
  per-NPC heads.
- The human idle is `meshes\characters\_male\locomotion\mtidle.kf`. `nvforever` plays
  the idle by default (owner decision 2026-10-09; it was the frozen bind pose before);
  `--actor-pose bind` keeps the bind pose (§7).

## 2. Resolution rules (`fnv_actor::resolve`)

FormIDs are global (load-order) FormIDs as in simulation.md §1; paths are **archive
paths**: trimmed, `/` → `\`, lower case, leading `\` removed, prefixed with `meshes\`
(models, `.egt`) or `textures\` (textures) unless already starting with it.

### 2.1 Actor base and leveled NPCs

- The placed ACHR's `NAME` is the base. An LVLN is resolved to one of its entries like a
  leveled item (simulation.md §4): entries with level ≤ the display level 1; the pool is
  all of them with "Calculate from all levels", else those of the highest eligible
  level; nested LVLN are followed. Chance none is not applied: a static display always
  shows an actor (a display choice, not a claim about the game).
- The pick is uniform with a random stream seeded by the reference's FormID (repeatable,
  not the game's roll). Each LVLN is picked **once per actor**, so every template group
  that reaches it sees the same NPC.
- The actor base after this resolution is the appearance's `npc` (and `edid`).
- **`random`** is true when (a) an LVLN pick had a pool with more than one distinct
  entry, or (b) the inventory source (§2.2) carries an entry with count > 0 that names a
  leveled item list which is *armour-random*: it can yield armour at level 1 (it is
  armour, or a list whose reachable entries — the pool, or all eligible entries with
  "Use All" — can yield armour) and it has chance none > 0, or an LVLG global, or (without
  "Use All") a pool of more than one distinct item, or a reachable entry that is itself
  armour-random. Actors with `random` false are fully determined by the data.

### 2.2 Templates and the skeleton

- For each group — Traits (template flag bit 0), Model/Animation (bit 6), Inventory
  (bit 8) — the data comes from the first NPC along the `TPLT` chain whose flag for that
  group is clear, starting at the actor base; a `TPLT` naming an LVLN is resolved as in
  §2.1 [hypothesis, following xEdit's field grouping]. `ACBS` "Use Template" is not
  consulted.
- From the **Traits** source: race (`RNAM`), sex (ACBS bit 0 Female).
- From the **Model/Animation** source: skeleton, hair, eyes, head parts, hair colour
  (`HCLR`, first three bytes), FaceGen arrays.
- From the **Inventory** source: carried items (`CNTO` in record order).
- Skeleton: the Model/Animation source's `MODL`; if it is absent or names an editor
  placeholder (file name starting with `marker`), the Traits source's; else
  `meshes\characters\_male\skeleton.nif` [hypothesis: the placeholder cannot be what the
  game draws, and every NPC_ naming a real skeleton names this one].

### 2.3 Worn armour

Owner statement 2026-10-08: an NPC carrying two outfits wears the one with the better
protection by DT and DR. Rule as implemented [hypothesis for the details]:

1. Resolve the carried items: each entry in order (count ≤ 0 gives nothing; a leveled
   item list resolves at level 1 per simulation.md §4).
2. Candidates: the ARMO items among them, each once, in order of first appearance.
3. Order by DT descending, then DR (stored value) descending, then first appearance.
4. Walk that order: wear an item unless it has no body slot or an already worn item
   shares one of its slots. `worn` lists the worn items in this order.

Placed NPCs wear 0 items: 44, 1: 1,309, 2: 2,080, 3: 508, 4: 1 [observed with these
rules].

### 2.4 Parts

`covered` = the union of the worn items' biped flags. Parts, in this order:

1. **Body**: the race's body parts for the sex in record order, indices 0 (role
   `body:upper_body`), 1 (`body:left_hand`), 2 (`body:right_hand`), each only if it has a
   `MODL` and `covered` lacks its slot (Upper Body, Left Hand, Right Hand); `texture` is
   the part's ICON. Body index 3 (an `.egt`, FaceGen texture data for the body skin) is
   not a part: its `MODL` is `body_egt`.
2. **Head** (only when `covered` lacks Head [hypothesis]): the race's head parts for the
   sex in record order that have a `MODL`, role `head:<name>` with names head, ears,
   mouth, teeth_lower, teeth_upper, tongue, left_eye, right_eye (indices 0–7);
   `texture` is the EYES record's ICON for indices 6 and 7 when the Model/Animation
   source names an EYES with one, else the part's ICON. Ears have no model and do not
   appear [observed].
3. **Head parts** (only when `covered` lacks Head): for each `PNAM` of the
   Model/Animation source in order, the HDPT's `MODL` (role `head_part`), followed by its
   `HNAM` extra parts, depth first.
4. **Hair** (only when `covered` lacks Head, and either covers Hat or lacks Hair): the
   Model/Animation source's HAIR, else the race's default hair (`DNAM`) for the sex;
   role `hair`, `texture` the HAIR's ICON. `hat` is true when `covered` has Hat: the
   hair model's `Hat` shape is shown instead of `NoHat` [hypothesis]. Covered Hair
   without Hat (a wig, a bandana) hides the hair.
5. **Worn armour**, in `worn` order: the item's `MOD3` for a female actor when present,
   else `MODL`; role `armor`, no `texture`.

`form` is the race for race parts, else the HAIR, HDPT or ARMO record.

Headgear worn by placed NPCs: Hair+Headband+Hat 1,166 (NCR trooper helmets),
Hair+Headband+Mask 364, Headband+Hat 159, Head with other slots (power armour and
similar helmets) 132, Hair alone (a wig) 2 [observed].

## 3. Assembly (engine, part B)

One entity per actor: the skeleton's node hierarchy is spawned once; skinned part shapes
bind their skins to the skeleton's bones by name (the part's own bone copies are not
spawned); rigid parts are parented to their attachment bone by the rule of §1.2 (`Prn`,
else `Bip01 Head`). Hair shows `NoHat` or `Hat` (§2.4); dismemberment caps — shapes none
of whose partitions sets part flag bit 0 ("editor visible", nif.xml BSPartFlag) — are
hidden [hypothesis: an intact actor shows none]. Pose: the skeleton's bind pose, frozen
(default), or the idle clip on the skeleton's bones (§7).
The race's height scales the actor [hypothesis]. Texture overrides: the race part ICONs,
the EYES texture and the HAIR texture replace the model's diffuse. Hair colour: texture ×
2 × HCLR in gamma space [hypothesis: a plain multiply leaves Sunny Smiles' hair dark
brown where the owner's screenshot shows auburn; doubled, it matches by eye].

## 4. FaceGen (part C)

File layouts [documented: FaceGen SDK manual, "File Formats",
https://facegen.com/dl/sdk/doc/manual/fileformats.html; all values little-endian]:

- **EGM** (geometry morphs): 64-byte header — magic `FREGM002`, V (vertex count), S
  (symmetric modes), A (asymmetric modes), geometry basis version (u32 each), 40 reserved
  bytes; then S symmetric modes followed by A asymmetric modes, each a f32 scale and V ×
  3 signed 16-bit deltas; a mode's delta for a vertex is the stored value × scale.
- **EGT** (texture morphs): 64-byte header — magic `FREGT003`, rows R, columns C, S, A,
  texture basis version (u32 each), 36 reserved bytes; then each mode: a f32 scale and
  three R × C images of signed bytes (red, green, blue), rows top to bottom; the value is
  the stored byte × scale.
- **TRI**: magic `FRTRI003`, counts of vertices, triangles, quads, labelled vertices and
  surface points, texture coordinates, extension flags, difference and statistical morph
  counts; vertices, faces, labels, UVs, then morphs (label, scale, per-vertex signed
  16-bit deltas).
- A face coordinate is 50 symmetric shape, 30 asymmetric shape and 50 texture values in
  standard deviations, usually within ±5 [documented: FaceGen SDK manual, "Overview"].
  This matches NPC_ FGGS/FGGA/FGTS (4,218 of 4,220 NPC_ have 50/30/50 [observed]).

Observed on the files [observed]: `headhuman.egm` has V 1,449, S 50, A 30 and
`headhuman.egt` 256 × 256, S 50, A 0, both of exactly the documented sizes; the EGM's V
equals `headhuman.tri`'s V + K (1,211 + 238), and `headhuman.nif`'s 1,211 vertices are
the TRI's first 1,211 in the same order (nearest-vertex index identical). For Doc
Mitchell's FGTS the EGT offset spans about −60…+20 colour units; one EGM mode moves a
vertex by at most 0.29 units per unit of coefficient. The EGT's first row is the face
texture's bottom row: taken top-down, the offset puts the mouth above the eyes.

How the game combines them [hypothesis; matches the owner's screenshot of Sunny Smiles
by eye]: a morphed vertex = base vertex + Σ coefficientᵢ × scaleᵢ × deltaᵢ over the
symmetric (FGGS) and asymmetric (FGGA) modes, for every part with an `.egm` beside its
model whose vertex count covers the shape; the face texture = the head's texture (the
race ICON) + Σ FGTSᵢ × scaleᵢ × EGT modeᵢ, upsampled bilinearly and flipped vertically.
The race FaceGen data, the RACE "FaceGen - Main clamp" / "Face clamp" values and the
body's "Upper Body Texture" `.egt` are not used yet. In the Goodsprings flight
(`--auto-fly 4000`, 40 s) 267 shapes were morphed and 34 faces textured, with no EGM
mismatching its shape.

## 5. CLI contract

- `fnvtool actor forms <Data> <plugins…> --schema <dir>` → `fnv.actor.forms/1`:
  `counts`, `templates` (NPC_ per template-group combination), `template_targets`,
  `skeletons`, `traits_templated_own_model`, `facegen_lengths`, `races` (per race:
  height, head/body part counts, male body parts, FaceGen presence), `issues`.
- `fnvtool actor appearance <Data> <plugins…> --schema <dir> (--ref <formid> | --cell
  <edid|formid> | --all) [--deterministic]` → `fnv.actor.appearance/1`. The actors are
  the given reference, the ACHR of the cell, or every ACHR of the load order (a record of
  type ACHR inside a cell-children group, world.md §1; winning definition), skipping
  those whose winning record is deleted. With `--deterministic`, actors whose `random`
  is true are left out (resolution errors stay).
  Top level: `schema`, `summary` {`outcomes` (counts: `resolved deterministic`,
  `resolved random`, `deleted`, errors by kind), `base_kinds` (base record type per
  actor), `worn_items` (actors per number of worn items, among those listed)}, `issues`
  (loader issues), `actors`, `load_order`.
  Each actor, ordered by reference FormID: {`reference`, `base`, `scale` (XSCL, default
  1, f32 widened), `npc`, `edid`, `female`, `race`, `skeleton`, `race_height` (RACE DATA
  height for the sex, f32 widened), `parts` [{`role`, `form`, `model`, `texture`?}],
  `worn` [FormID], `hat`, `hair_color`? [r, g, b], `facegen`? {`fggs`, `fgga`, `fgts`}
  (f32 widened), `race_facegen`? (same shape, the race's for the sex), `body_egt`?,
  `random`}; or {`reference`, `base`, `error`} when the actor cannot be resolved.
  FormIDs print as `0x%08X`; absent optional values are left out.
- `fnvtool actor meshes <Data> <plugins…> --schema <dir> [--nif-schema <dir>]` →
  `fnv.actor.meshes/1`: `pairs`, `part_kinds` (role × skinned/rigid/+Prn), `hair_variants`,
  `textures`, `missing_textures`, `problems` [{`skeleton`, `model`, `role`, `actors`,
  `problems`}].

## 6. Captures wanted

- Screenshots in game of NPCs with a cowboy hat, an NCR trooper helmet, a power armour
  helmet and a wig, to check §2.4 (the owner's Goodsprings Settler and Sunny Smiles
  screenshots, 2026-10-08, are kept privately as references).
- `GetEquipped` on NPCs that carry two items for one slot (§2.3).
- An NPC whose record names `marker_creature.nif` (e.g. `PrimmNCRDeputyRanger`), to
  confirm it is drawn with the human skeleton (§2.2).
- The idle in game for comparison with §7: a screenshot of Doc Mitchell standing in his
  house, and whether actors placed side by side breathe in step (§7.3 spreads them).

## 7. Animation playback (part E)

`nvforever --actor-pose idle` (the default since 2026-10-09, owner decision) plays a
looping KF clip on each actor's skeleton; `--actor-pose bind` (the default before) keeps
the frozen bind pose.
`--actor-pose-time <s>` holds every actor at one clip time (screenshots). Choosing a clip
per situation (AI, packages, movement) is Layer 6 and not done here.

### 7.1 The human idle

`meshes\characters\_male\locomotion\mtidle.kf` against
`meshes\characters\_male\skeleton.nif` [observed: `kf_bind_check` example and the
`actor pose` log line of `nvforever`]:

- One controller sequence `Idle`, 0–4 s, cycle type 0 (loop), frequency 1, with 64
  controlled blocks.
- 60 are transform tracks, and **all 60 name a node of `skeleton.nif`** (66 nodes); none
  is left unbound. By rotation channel: 51 B-spline (compressed), 2 keyed (`Bip01
  Spine`, `Bip01 L ForeTwist`), 7 static (`Bip01 L Toe0`, `Bip01 Neck`, both clavicles,
  both pauldrons, `Bip01 NonAccum`).
- The other 4 are not transforms and are not played: an `NiFloatExtraDataController` on
  `Bip01 Head` and an `NiGeomMorpherController` on `HeadAnims:0` (both with an
  `NiFloatInterpolator`), and `NiVisController`s on `HeadAnims` and `##VisCtrl` (with an
  `NiBoolInterpolator`) [hypothesis: facial animation and visibility switches; what they
  drive is open].
- 6 skeleton nodes have no track, among them the root `Scene Root` and `Bip01`.
- At its start the clip is up to 47° away from the bind pose (`Bip01 R UpperArm` 46.8°,
  `Bip01 L UpperArm` 41.6°): the arms hang down instead of the T-pose.

### 7.2 Accumulation root

- The sequence names `Bip01` as its accumulation root (NiControllerSequence "Accum Root
  Name", nif.xml) [observed].
- `skeleton.nif` puts the body's offset on `Bip01`: translation (0, 0.657, 67.771) and
  90° about z, with `Bip01 NonAccum` below it at the identity [observed].
- `mtidle.kf` has no `Bip01` track; its static `Bip01 NonAccum` pose is (0, −0.296,
  66.330) and 90° about z, nearly the skeleton's `Bip01` offset [observed]. Played over
  the bind `Bip01`, the offset applies twice: the pelvis would sit about 134 units up,
  turned 180° [observed: arithmetic on these values].
- Rule [hypothesis: the accumulated motion of a clip without a root track is none]: while
  such a clip plays, its accumulation root is held at the identity
  (`Clip::undriven_accum_root`). Rendered, Doc Mitchell then stands with his feet on the
  floor at the height of the bind pose [observed, screenshot].
- nvf-viewer follows the same rule (`AnimState::start`), and puts the root back to its
  bind transform when the next sequence drives it or `AnimState::stop` is called. With
  `characters\_male\upperbody.nif` and `mtidle.kf`, the skeleton's `Bip01 Pelvis` is
  67.77 units up in the bind pose, 134.02 with the root left at its bind transform and
  66.25 with the root held. The feet are 7.07 up in the bind pose, 74.1 without the
  hold and 6.4 with it [observed: world positions logged by a temporary viewer patch].

### 7.3 Engine

- Clip choice: `locomotion\mtidle.kf` in the skeleton's folder [observed for
  `characters\_male\skeleton.nif`; hypothesis: the same rule for other skeletons]. Every
  human actor, female ones included (they use the same skeleton, §1.1), plays it
  [hypothesis: the game may pick other idles per sex, package or furniture].
- Tracks drive the skeleton's bone entities by node name, except the actor's root, whose
  transform is the placement. Skinned shapes are bound to those bones and rigid parts are
  their children (§3), so one player moves every part.
- Each actor starts at a phase spread by its reference FormID, so neighbours do not move
  in step [engine choice, not a game rule].
- Sampling allocates nothing per frame: B-spline channels are evaluated into fixed-size
  arrays (`BSplineChannel::eval_array`); clips are loaded once per path and their tracks
  shared between actors.

### 7.4 What follows the animation

Doc Mitchell in GSDocMitchellHouse, held at clip times 0 s and 2 s (screenshots kept
privately) [observed]:

- Skinned parts (upper body with outfit, hands, head) follow: the arms hang at the sides
  and change between the two times; the neck joins the body without a gap.
- Rigid parts under `Bip01 Head` (hair, eyes, moustache) stay in place on the head as it
  turns and tilts (about 2 units of head movement between 0 s and 2 s).
- The FaceGen-morphed head and its face texture are kept (the morph replaces the mesh, not
  the skin binding).

### 7.5 Performance

The CLAUDE.md moving-camera smoke test (`--cell Goodsprings --auto-fly 4000
--run-seconds 40`), 2026-10-08, owner's machine [observed]:

| | `--actor-pose bind` | `--actor-pose idle` |
|---|---|---|
| exit, "run finished" | 0, yes (8 runs) | 0, yes (8 runs) |
| actors drawn / playing a clip | 33–34 / 0 | 33–34 / 33–34, no clip missing |
| clips playing at once (peak) | 5 (animated models) | 19 (5 models + actors in loaded cells) |
| sampling cost per clip and frame | 10.5 µs | 41.6 µs |
| sampling cost per frame, run average | 0.004–0.006 ms | 0.12–0.22 ms |
| frame rate, mean of 8 runs (range) | 13.7 fps (9.4–19.5) | 13.7 fps (9.3–23.1) |

- The frame rate cannot separate the two modes on this machine at the time: another
  session's `nvforever` and builds ran alongside (CPU at 66–79 %), and runs of the same
  mode differ by up to 2×. The sampling cost, timed inside the engine
  (`advance_animations`, printed in the run summary), is the reliable figure: about
  0.8 ms per frame with 19 clips. It covers sampling and writing the bones' local
  transforms, not Bevy's transform propagation or skin upload.
- On a quieter machine (2026-10-09, after merging main with the cell fades of #50), one
  run each: bind 27.5 fps, idle 26.7 fps (33 actors animated, up to 24 clips at once,
  29.3 µs per clip and frame) [observed].
- Sampling the 60 tracks of `mtidle.kf` alone takes 20.2 µs per pose (`kf_bind_check`
  with `KF_BENCH=1`, 20,000 poses).
- No per-frame allocation in sampling (§7.3).
