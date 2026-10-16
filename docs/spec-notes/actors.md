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
- A rigid part on the head has its vertices in the **skeleton's axes** with the origin
  at the parent bone: hair spans z −0.7…15.3 and a cowboy hat z 3.8…17.5 above
  `Bip01 Head` (bind position (0, 0.4, 112.8) in `skeleton.nif`), while the head bone's
  own x axis points up. Node rotations of the 206 rigid shapes placed NPCs use: all
  identity 112 (hats, helmets, beards); root and shape rotations that cancel 66 (most
  hair, e.g. `hairbun.nif`); a root equal to the inverse of `Bip01 Head`'s bind rotation
  with identity below 16 (eyes and the other race head parts); identity root with real
  offsets below 12 (Pip-Boy arm parts). Rule for head parts [observed, those three kinds
  render in place]: a part whose root stores the bone's inverse bind rotation is an
  ordinary child of the bone; any other part gets that inverse rotation from the
  attachment; node transforms are kept in both cases.
- A rigid part on **another bone is in that bone's own axes**: an ordinary child, node
  transforms kept, no rotation added. Over the worn models of every ARMO in the ten
  masters (498 models, placed or not; `fnvtool actor meshes`, `armor_rigid_bones`), 61
  have unskinned shapes: 35 name `Bip01 Head`, 20 have no `Prn`, and 6 name
  `Bip01 L ForeTwist`, all of them Pip-Boys (`pipboy3000\pipboyarm.nif`,
  `pipboyarmfemale.nif`, `pipboyarmnpc.nif`, `pipboyarmfemalenpc.nif`,
  `pimpboy3billion\pimpboyarm.nif`, `pimpboynpc.nif`) [observed]. `pipboyarm.nif`: the
  root is the identity; the casing shape sits 8.09 along x, turned 11° about x, and its
  vertices then reach x 4.4…17.1. `Bip01 L ForeTwist` starts at the elbow with its x
  axis down the forearm, and `Bip01 L Hand` is 18.13 along it. As a plain child the
  casing is a cuff round the forearm from 4.4 to 17.1 units below the elbow, 1.8…6.1
  from the bone's line; with the head rule's rotation it lands 1.9…15.8 units *above*
  the elbow, 0.2…11.4 from the line, which is what `--play` drew until 2026-10-10 (the
  Pip-Boy on the back of the upper arm, askew) [observed: computed from the two files,
  and in screenshots before and after]. Why the head differs is open [hypothesis: the
  game builds the head, with hair and headgear, in the skeleton's axes as one unit, as
  the skinned head model is].
- The player's Pip-Boy models (`pipboyarm.nif`, `pipboyarmfemale.nif`, `pimpboyarm.nif`:
  19 shapes; the NPC ones have 6) hold a shape `PipboyLightEffect:0`: an unlit sheet
  textured `effects\fxwhite.dds`, 11.5 × 9.8 units over the 5.9 × 4.5 screen, emissive
  white, alpha-blended, with vertex colours (0.10, 1.0, 0.35) whose alpha runs 0…0.125
  [observed]. It is the glow of the Pip-Boy light and shows only while that light is on
  [owner statement 2026-10-10]. NVForever has no Pip-Boy light yet, so the shape is not
  drawn (`fnv_actor::mesh::is_light_effect_shape`); drawn, it was a flat bright green
  square over the screen. The three orange button lights (`StatsGlow`, `ItemsGlow`,
  `DataGlow`) are always lit [owner statement 2026-10-10] and are drawn. Drawn as they
  are, and open: the `glare` sheet, and `ScreenLit` beside `pipboyscreen` [hypothesis:
  one of the two screens is for the light or the menu].
- Outfits hold two left sleeves: 170 of the 498 worn models have shapes named
  `PipBoyOff…`, 159 of them also `PipBoyOn…` (`PipBoyOn`, `PipBoyOn:0`, `PipBoyOn1:0`,
  `PipBoy0n:0`, `PipBoyOnLegate:0`, …) [observed: `armor_pipboy_sleeves`]. In
  `vaultsuitarmor\m\outfitm.nif` `PipBoyOff` (175 vertices) covers the left forearm from
  3 to 18 units below the elbow and the hand's cuff; `PipBoyOn` (111 vertices) leaves
  out 3 to 15, where the Pip-Boy sits [observed]. Rule: an actor wearing an item in the
  PipBoy biped slot (bit 6) draws the `PipBoyOn…` shapes [owner statement 2026-10-10:
  "we see a sleeve on the arm with a pipboy, it is rendered over the outfit using the
  pipboy on node"; two in-game first-person screenshots: a jumpsuit's blue sleeve from
  the elbow to the Pip-Boy, and another outfit's brown cuff between the Pip-Boy and the
  glove], any other actor the `PipBoyOff…` shapes [hypothesis, from the names]
  (`fnv_actor::mesh::pipboy_sleeve`). The same holds in first person, where the sleeve
  is drawn beside the `Arms…` shapes (`fnv_actor::first_person::is_first_person_shape`).
  Before 2026-10-10 both sleeves were drawn on everyone, and none in first person.
- Hair-shader shapes (BSShaderFlags bit 18) carry a mask in their vertex colours, not a
  colour: red 0.04–0.39 with green and blue at 1 in `hairbase.nif` and
  `beardmustachecurly.nif` [observed]; multiplied in, hair turns cyan. They are not
  applied [hypothesis: what the mask does in the game's hair shader is open].
- Hair colour (`HCLR`) tints the hair model and, since 2026-10-16, every hair-shader shape
  of the head parts (HDPT models: beards such as `beardmustachecurly.nif`) the same way
  [hypothesis: the owner saw Sgt Kilborn's and Joe Cobb's beards white in ours, unlike
  their hair; an untinted beard texture is near grey-white; playtest list 5 item 26].
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
else `Bip01 Head`; the head rule's rotation on `Bip01 Head` only). Hair shows `NoHat` or
`Hat` (§2.4); outfits show the left sleeve that matches whether a Pip-Boy is worn, and
the Pip-Boy light's glow is not drawn (§1.2); dismemberment caps — shapes none
of whose partitions sets part flag bit 0 ("editor visible", nif.xml BSPartFlag) — are
hidden [hypothesis: an intact actor shows none]. Pose: the skeleton's bind pose, frozen
(default), or the idle clip on the skeleton's bones (§7).
The race's height scales the actor [hypothesis]. Texture overrides: the race part ICONs,
the EYES texture and the HAIR texture replace the model's diffuse; a skin shape also
takes the normal map beside its new diffuse (§3.1, "Normal maps"). Hair colour: texture ×
2 × HCLR in gamma space [hypothesis: a plain multiply leaves Sunny Smiles' hair dark
brown where the owner's screenshot shows auburn; doubled, it matches by eye].

### 3.1 Skin shapes, and which shapes a part's texture replaces

Survey of the visible shapes drawn by a BSShaderPPLightingProperty in
`Fallout - Meshes.bsa` (`crates/fnv-nif/examples/skin_survey.rs`) [observed]:

- 255 shapes have shader type 14 (`SHADER_SKIN` [documented: nif.xml BSShaderType]).
  They are exactly the shapes with the "FaceGen" shader flag (bit 10 [documented:
  nif.xml BSShaderFlags]): no shape has one without the other. Their diffuse textures:
  172 an upper body texture (`textures\characters\<kind>\upperbody….dds`), 71 a hand
  texture, 10 a head texture, 2 none. They sit in the race part models and in worn
  models (skin showing inside outfits and gloves).
- 28 shapes name such a skin texture without the skin type (shader type 1): corpses
  and severed parts in clutter, the chargen models `m01….nif`, one outfit
  (`armor\caesar\caesar_f.nif`).
- The race upper bodies hold two drawn shapes: the skin (`upperbody.nif` `Arms:1`,
  `femaleupperbody.nif` `Arms01`; type 14, the upper body texture) and the underwear
  (`Arms:0`, `UpperBody`; type 1, `textures\armor\underwear\underwearm.dds` /
  `underwearf.dds`: a grey T-shirt and dark shorts for men). The child bodies hold the
  skin shape only. The Pip-Boy glove hands (`lefthandpipboyglove.nif`) hold a skin
  shape (hand texture) and the glove (type 1, its own texture).
- Head and hand race parts hold one skin shape. Mouth, teeth, tongue and eye models hold
  one shape of type 1; the child mouth, teeth and tongue models name no texture of
  their own, so they depend on the part's texture.

Rule used [hypothesis; the owner's in-game picture of the unclothed male body shows the
grey T-shirt and dark shorts over light skin, 2026-10-10]: a texture given for a whole
part (a race part's ICON, the EYES texture, the HAIR texture) replaces the diffuse of
the part's skin shapes when it has any, else of every shape; never of a dismemberment
cap (`nvf_assets::actor::texture_targets`). Before 2026-10-10 the engine put it on every
shape, which painted the underwear with the skin texture (and the body tint of §4.1
over that): the whole body was one skin colour.

**Worn models** (built 2026-10-10). The skin shapes of worn models (role `armor`:
outfits, gloves, the Pip-Boy glove), from the same survey, which also lists the skin
shapes by texture and the files by the kinds of skin shape they hold [observed,
`Fallout - Meshes.bsa`]:

- 187 files under `meshes\armor\` hold skin shapes: 141 only upper body textures, 43
  only hand textures, 1 an upper body and a head texture, 2 only a shape without a
  texture. No file mixes hand and upper body skin.
- The texture a shape names is fixed in the file, whoever wears the model, and several
  are plainly not the wearer's: 13 shapes in six adult women's outfits
  (`leatherarmor\f\outfitf.nif`, `republicans\republicanf_01…04.nif`,
  `wastelandclothing03\outfitf.nif`) name the child's
  `characters\childfemale\upperbodyfemale.dds`, a different file from the
  adult `characters\female\upperbodyfemale.dds` (48 placed adult NPCs wear them), and
  the child outfits name the adult texture; `combatarmor\m\outfitm.nif` `Upperbody`
  names the head texture `characters\male\headhuman.dds` (its twin in the women's
  model, `Upperbody02`, names the upper body texture; 11 placed NPCs wear the men's
  model); `eulogyjones\outfitf.nif` `upperbody` and `bigbattle\encsciencebattlearmor.nif`
  `UpperBody:0` name none (no placed NPC wears them). The one model that names a
  ghoul or raider texture is Raul's `raoul\raoul_armorjumpsuit.nif`
  (`characters\ghoul\upperbodymale.dds`).
- Nine races have skin textures of their own kind (body index 0 and 1 ICON): Ghoul
  (`characters\ghoul\…`, `ghoulfemale\…`), the four raider races (`characters\raider\…`, `raiderfemale\…`),
  and the four child races (`characters\child\…`, `childfemale\…`). Of the 3,386 placed
  NPCs of FalloutNV.esm that resolve, 21 ghouls, 350 of the raider races and 30
  children wear a model with skin shapes (`fnvtool actor appearance --all`).

Rule used [hypothesis: the game gives a worn model's skin shapes the wearer's race
texture; the files above fit that, and no in-game picture was compared]:
a skin shape of a worn model takes the race's upper body texture (body index 0 ICON)
or hand texture (index 1 ICON) in place of the one it names, then the tint of §4.1.
Which of the two is decided by the file name of the texture the shape names, when it
lies in a `characters` folder: `hand…` → hand; `upperbody…` or `head…` → upper body; a
shape that names no texture → upper body; a shape that names any other texture keeps
it (none in the base game). Dismemberment caps are left alone
(`fnv_actor::resolve::worn_skin`, `Appearance::worn_skin_texture`; engine
`actors.rs` `part_uses`). Choosing by the slot the worn item covers would give the same
result for every file above, since no file mixes the two.

Checked on screen, before and after (2026-10-10, owner's install, pictures private): a
ghoul in `prostitutes\prosmale02.nif` (reference 0x0011A2FB, Gomorrah) had pale human
arms, chest and legs under a ghoul head and hands, and now has ghoul skin throughout;
three Great Khans of the raider races at Quarry Junction (QJMelissa, QJGreatKhanAAM,
QJGreatKhanHM) had light untinted arms and legs under dark faces, and now arms and legs
of the faces' colour.

**Normal maps** (built 2026-10-10). Until then a replaced diffuse texture left the
normal map (texture slot 1) the one the model names. Survey [observed: `fnvtool bsa list`
and `fnvtool dds list` on `Fallout - Textures.bsa`; texture slots of the part models of
every placed NPC (`fnvtool actor appearance --all`, `fnvtool nif decode`); which meshes
of `Fallout - Meshes.bsa` name a texture, and in which slot,
`crates/fnv-nif/examples/texture_refs.rs`]:

- Skin. The skin folders under `textures\characters\` (`male`, `female`, `ghoul`,
  `ghoulfemale`, `raider`, `raiderfemale`, `child`, `childfemale`, and `old` /
  `oldfemale` for heads only) hold, beside each diffuse (`upperbody….dds`, `hand….dds`,
  `headhuman.dds`, and `headhumanold.dds` in `male` and `female`), a file of the same
  name with `_n` (1024 × 1024 DXT5 for bodies and heads): 30 files, no two with the
  same content (SHA-256).
- All 139 skin shader properties in the models of placed NPCs name in slot 1 the `_n`
  file beside their slot 0. But the models name the adult human's maps whoever uses
  them: all eight head models (`headhuman`, `headfemale`, `headghoul`,
  `headghoulfemale`, `headold`, `headoldfemale`, `headchild`, `headchildfemale`) name
  `characters\male\headhuman.dds` and `characters\male\headhuman_n.dds`, the women's
  ones included; the hand and body models name the `male` or `female` files.
- No mesh names the `_n` files of `ghoul` (but the upper body one, by Raul's jumpsuit),
  `ghoulfemale`, `raider`, `raiderfemale`, `child`, `old`, `oldfemale`, the head and
  hand ones of `childfemale`, or `female\headhumanold_n.dds`; `female\headhuman_n.dds`
  is named by one mesh, `characters\head\headfemalefacegen.nif`, which is no placed
  NPC's head part (theirs is `headfemale.nif`). The game can reach these files only by
  their names.
- The `_sk` maps (slot 2) need no switch: the head and body ones are one file in every
  folder (128 × 128; the first-person hands' too), the hand ones another (32 × 32).
- Not skin (shader type 1). Eye textures have no `_n` file. The mouth, teeth and tongue
  ones have one for the human textures only, a 1 × 1 pixel stand-in (the same content as
  `characters\head\earshuman.dds`) that no mesh names; those models name
  `textures\shared\flat_n.dds`. Hair: 34 of the 37 diffuse textures in
  `characters\hair\` have an `_n` file, and a mesh names every one of them but three
  whose diffuse no placed NPC uses (`hairf`, `shades`, `shades01`); the models do not
  always name the file beside their own diffuse (`hairfemaleb.nif`: `hairf02.dds` with
  `hairf01_n.dds`, there being no `hairf02_n.dds`).

Rule used [hypothesis: the game finds the normal map of a skin texture it puts in by its
name; the unnamed files above fit that, and no in-game picture was compared]: when the
diffuse of a skin shape (shader type 14) is replaced — a race part's ICON on the race's
head, hand and body models, or the race's texture on a worn model's skin — its normal
map becomes the file named like the new diffuse with `_n` before the extension, when
the virtual file system has that file; otherwise the model's own stays
(`fnv_vfs::normal_map_beside`, `nvf_assets::actor::skin_normal_map`, applied in
`ActorCtx::material_with_texture`). Shapes that are not skin keep the normal map their
model names: for eyes and mouth parts the files above speak against a switch, for hair
they do not decide (below). FaceGen face and body tints replace only the base colour
image afterwards (§4), so they keep the switched normal map.

What it changes among the 3,386 placed NPCs of FalloutNV.esm that resolve: 1,107 heads
(489 adult women, who had the men's head relief; 131 old men and women; 384 of the
raider races; 74 ghouls; 29 children), the hands of 491 (raider races, ghouls,
children), and the skin inside outfits of the wearers counted under "Worn models". A man
of a young adult human race (`male\headhuman.dds`, `male\handmale.dds`,
`male\upperbodymale.dds`) keeps every map: for him the file beside the texture is the
one the model names.

Checked on screen, before and after (2026-10-10, owner's install, pictures private,
`C:\fnv-private\frames\skin-normal-maps\`): the Gomorrah ghoul above gains the ghoul
relief on face and neck (sunken nose, bared jaw, sinews) and a rougher surface on arms,
chest and legs; pixels change on him only. Of the three Great Khans, the woman's face
(QJMelissa) loses the men's heavy brow and jaw relief; the raider maps differ little
from the human ones at that distance (about 1,000 pixels of 1,440,000 differ by more
than 6/255). A settler of the Caucasian race in the Prospector Saloon (GSSettlerCM):
the two pictures are the same file, byte for byte.

Open:

- Hair. 216 placed NPCs have a HAIR texture that is not their hair model's own and has
  an `_n` file of other content than the one the model names (`hairbase.nif` and
  `hairbaseold.nif`, which name `hairbase_n.dds`, with `hairafricanamericanbase.dds`
  (187) or `hairbalding.dds` (29)). They keep the model's normal map [hypothesis: the
  files do not show which the game uses; a picture of such a head from the original,
  close up, would]. The hair's `_hl` map (slot 2) is in the same position.
- The hand-to-hand weapon models (`weapons\hand2hand\*.nif`, 12 skin shapes: a hand
  inside boxing tape, knuckles and the like) are not worn models; a hand in them keeps
  the texture it names, untinted.
- Skin-coloured shapes without the skin type (`armor\caesar\caesar_f.nif` `Arms:0`,
  above) keep their texture.

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
  16-bit deltas). Read by `fnv_actor::tri` since dialogue step D6: the morphs are what
  lip sync moves (docs/spec-notes/lipsync.md §4, which also corrects the vertex claim
  below: the model's vertices are the TRI's in the same order — every triangle joins
  the same vertex numbers — but `headhuman.nif`'s positions differ from the TRI's by up
  to 0.53 units, so "nearest vertex" does not hold for all of them).
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

How the game combines them [hypothesis; compared with the owner's in-game picture of
Sunny Smiles, §4.3]: a morphed vertex = base vertex + Σ coefficientᵢ × scaleᵢ × deltaᵢ
over the symmetric (FGGS) and asymmetric (FGGA) modes, the coefficients being the NPC's
plus its race's (§4.3, since 2026-10-10; before that the NPC's alone), for every part
with an `.egm` beside its model whose vertex count covers the shape, and for the hair
shapes that have an `.egm` each (§4.3); the face texture = the head's texture (the
race ICON) + Σ cᵢ × scaleᵢ × EGT modeᵢ, upsampled bilinearly and flipped vertically,
with cᵢ = the NPC's FGTSᵢ + its race's FGTSᵢ (§4.2, since 2026-10-10; before that the
NPC's alone). The RACE "FaceGen - Main clamp" / "Face clamp" values are not used. In the
Goodsprings flight (`--auto-fly 4000`, 40 s) 267 shapes were morphed
and 34 faces textured, with no EGM mismatching its shape.

### 4.1 Body skin

- The race's body index 3 ("Upper Body Texture", §1.1) names an `.egt`:
  `characters\_male\upperbodyhumanmale.egt` for every adult human male race, and
  `…female`, `…child`, `…old` variants [observed: `fnvtool actor forms`]. These files
  have the documented EGT layout with 32 × 32 images, 50 symmetric and 0 asymmetric
  modes, 153,864 bytes each — the size the header implies [observed: 5 files checked],
  so FGTS (50 values) fits them as it fits the head's.
- Rule [hypothesis: the face rule of §4 applied to the body; not checked against the
  game]: the body skin texture = the race's upper body ICON
  (`characters\male\upperbodymale.dds`, 1024 × 1024 [observed]) + Σ cᵢ × scaleᵢ × EGT
  modeᵢ (cᵢ as for the face: the NPC's FGTS plus its race's, §4.2; the hands likewise),
  upsampled bilinearly and flipped vertically. The engine gives it to every shape
  whose diffuse is that texture — the race's upper body and the body skin shapes inside
  outfits, which take that texture (§3.1, worn models) — and halves it to 512 × 512 to
  keep one copy per actor small
  (`FaceGenCache::skin_texture`). `--no-facegen` turns it off. Only the skin shape of
  the race's upper body takes it; the underwear shape keeps its own texture (§3.1).
- The body file holds one colour per mode [observed, 2026-10-10]: within a mode of
  `upperbodyhumanmale.egt` all 1,024 texels are equal (spread below 0.001 colour units
  per unit of coefficient), so the offset is one colour for the whole texture and
  nothing in the file spares the painted briefs of the skin texture. Those colours are
  the head file's at the neck: over all 50 modes and 3 channels the body's values differ
  from `headhuman.egt`'s texels in the bottom rows of the head texture (rows 0–46 of
  256 from the bottom, middle columns) by 0.02 units rms, against 1.6 for the median
  texel. For the player's record the body offset and the head offset at the neck are
  both (−56, −46, −48). This supports the rule above: body and neck get the same colour
  shift [hypothesis for the game, as before].
- Hands [hypothesis, built 2026-10-10]: the hand `.egt` files (`lefthandmale.egt`,
  `righthandold.egt`, …, 16 files, also 32 × 32 × 50) sit in `characters\_male\`; no
  record names them. Observed: they are four distinct files (left and right, each in a
  male-kind and a female-kind version); each mode is nearly one colour (spread at most
  0.47 units per unit of coefficient); the four differ from each other by at most 0.02
  units in a mode's mean colour, and from the body file by at most 1.8. For the
  player's record the hand offset is (−47, −38, −41). The engine takes the file named
  like the race's body `.egt` (`upperbody[human]<kind>.egt` → `lefthand<kind>.egt`,
  `fnv_actor::resolve::hand_egt`; the left file serves both hands, the right one
  differing by 0.02 units) and adds its offset to the race's hand texture (body index 1
  ICON), for every shape with that texture: the race hands and hand skin inside gloves
  (the Pip-Boy glove's fingers and wrist; they take the race's texture, §3.1). Untinted, the hands stayed the light colour of
  the texture beside arms darkened by the body offset.
- The player's first-person arms and hands take the same tint (the first-person
  appearance keeps FGTS; `fnv_actor::first_person`).
- The player's record (NPC_ 7) has a dark offset: body (−56, −46, −48), where the median
  male Caucasian NPC has (−25, −24, −28) and one in ten is beyond −45 in red [observed,
  557 NPCs]. With the body texture's median colour (159, 125, 96) that gives a brown
  skin, clearly darker than the untinted texture (`--no-facegen` shows the latter).
  Whether the original draws its default courier that dark is not checked [open; an
  in-game picture of the unclothed default character would settle it, and with it the
  strength of the rule].

### 4.2 Race FaceGen data (added to the NPC's: texture values here, geometry values in §4.3)

Each RACE carries FGGS/FGGA/FGTS per sex (§1.1). The GECK's race form calls them a
baseline: its "FaceGen Race" and "FaceGen Data" tabs "specify a racial face baseline"
[documented: GECK wiki, "Race"]. Over the 3,386 placed NPCs with 50 FGGS values
[observed, `fnvtool actor appearance --all`, FalloutNV.esm]: none has the race's values;
the median length of an NPC's FGGS vector is 6.0 and of the race's 13.2, and the median
distance between the two is 17.2 — larger than either. So NPC values are not scattered
around the race's, as absolute coordinates with a race default would be.

**What the values hold** [observed, 2026-10-10; the 1,722 distinct NPC_ records among
those placed NPCs, colours from the body `.egt` of §4.1 for the sex (one colour per
mode), in colour units (red, green, blue)]:

| Race, sex (NPC_ records) | NPCs' own FGTS, median | The race's FGTS | Sum, median |
|---|---|---|---|
| Caucasian, male (547) | −25, −24, −28 | +1, −1, −2 | −24, −25, −31 |
| Caucasian, female (200) | 0, −11, −17 | 0, +4, +12 | 0, −7, −6 |
| Hispanic, male (148) | −16, −19, −24 | −9, −10, −4 | −24, −29, −28 |
| Asian, male (88) | −15, −17, −22 | +8, +1, −7 | −7, −17, −28 |
| AfricanAmerican, male (223) | +8, −3, −10 | −91, −71, −49 | −83, −74, −59 |
| AfricanAmerican, female (60) | +6, −5, −13 | −82, −64, −43 | −76, −68, −57 |
| AfricanAmericanOld, male (23) | +4, −5, −13 | −92, −72, −54 | −88, −77, −67 |
| AfricanAmericanRaider, male (18) | +7, +1, −4 | −48, −35, −32 | −41, −34, −36 |
| AfricanAmericanRaider, female (20) | +8, +9, +8 | −48, −34, −21 | −40, −25, −12 |

- The Caucasian, Hispanic, Asian and AfricanAmerican races of one kind and sex name the
  same textures and `.egt` files: `characters\male\headhuman.dds` and
  `upperbodymale.dds` for the plain male races; the Old races `headhumanold.dds`, the
  OldAged races `characters\old\headhuman.dds` (both with the plain body texture), the
  Raider and Child races their own folder's head and body textures [observed: `fnvtool
  actor forms`, `actor appearance`]. So between a Caucasian and an AfricanAmerican NPC
  only the FaceGen values can differ.
- From their own values the AfricanAmerican NPCs come out lighter than the Caucasian
  ones. Their own FGTS hold none of the race's colour: along the direction from the
  Caucasian race's FGTS to the AfricanAmerican race's (9.5 units long for men) the
  AfricanAmerican men's own values lie at −0.02 (median; quartiles −1.15 and +1.21),
  the Caucasian men's at +0.28, the Hispanic men's at +0.15. Absolute coordinates would
  put the AfricanAmerican NPCs near +9.5.
- The race colour is large only for the AfricanAmerican races (10 race and sex pairs, 357
  NPC_ records; offsets from (−31, −31, −26) to (−92, −72, −54)). For every other race
  and sex the race's offset is within 25 units per channel; CaucasianOld and
  CaucasianOldAged men have all-zero race FGTS.
- Geometry shows the same pattern: along the direction from the Caucasian race's FGGS to
  the AfricanAmerican race's (17.1 units long for men) the AfricanAmerican men's own
  FGGS lie at −0.59 (median; quartiles −6.03 and +0.08), the Caucasian men's at +0.16.
  Over all NPCs the cosine between an NPC's FGGS and its race's has a median of −0.10
  (69% negative); for FGTS −0.07 (67% negative).
- Every race's FGGA is all zeros (41 race and sex pairs); 2% of the NPC_ records have a
  non-zero FGGA. Adding the race's FGGA would change nothing.

**Rule for textures** [hypothesis, built 2026-10-10]: the game adds the race's FGTS for
the actor's sex to the NPC's, value by value, and the sum drives the face, body and hand
`.egt` (`Appearance::skin_fgts`, `fnv_actor::facegen::add_coefficients`). Reasons: the
GECK wiki's "baseline"; the NPCs' own values hold none of the race's colour (above), and
with shared textures nothing else could give AfricanAmerican NPCs a darker skin than
Caucasian ones. Not yet compared with an in-game picture [open: the owner's picture of
Joe Cobb and of Easy Pete would settle the rule and its strength]. An actor without
FaceGen data of its own still gets no tint. `nvforever --no-race-facegen` draws the
skin from the NPC's values alone, for comparison.

Pictures (our renderer, 2026-10-10, with and without `--no-race-facegen`; kept privately
in `C:\fnv-private\references\race_facegen\`):

- Joe Cobb (GSJoeCobbRef, AfricanAmerican; body offset own (+15, −2, −20), race (−91,
  −71, −49), sum (−76, −73, −69)): pale skin from his own values, the lightest face in
  the saloon; dark brown with the race's added. His reference is initially disabled, so
  the pictures need `--show-disabled-actors` (§11).
- Sunny Smiles (SunnyRef; her record's race is Hispanic, female race offset (+4, +2,
  −2)): no visible change in skin colour. (Her face shape did not match the game from
  her own values alone, §4.3.)
- Easy Pete (EasyPeteRef; his record's race is AfricanAmericanOldAged, race offset (−75,
  −62, −53), sum (−99, −91, −72); his head texture is `characters\old\headhuman.dds`): a
  pale grey-white face from his own values, a dark red-brown one with the race's added —
  the largest change among the three, and the one most worth checking against the game,
  since the sum is darker than Joe Cobb's.
- The player's record (NPC_ 7, Caucasian male, race offset (+1, −1, −2)) is unaffected,
  as is the skin tone question of §4.1.

**Geometry**: the numbers above suggest the race's FGGS is a baseline in the same way
(the NPCs' own FGGS hold none of the race's shape). Built 2026-10-10 after the owner's
in-game picture of Sunny Smiles showed that her own values alone give the wrong face:
§4.3.

### 4.3 Face shape: the race's geometry values, and the hair's morph files

Owner's report (2026-10-10): Sunny Smiles has "a strange face shape, almost more
masculine", with an in-game dialogue close-up of her. Pictures (the game's, ours before
and after) are kept privately in `C:\fnv-private\references\face_shape\`.

**The camera matters** [observed]: in the game's picture her head is 0.31 of the frame
height and her shoulders 0.39 of its width, which at the default field of view puts the
camera about 46 units from her, looking down (the frame's centre is on her throat). A
long lens from further away flattens a face and widens its jaw, so shape comparisons use
that camera: `nvforever --cell 0x00106185 --pos 277,-492.7,3574 --heading 270 --pitch -17
--fov 75 --size 1280x720 --actor-pose bind` (the Prospector Saloon; she stands at
(230.9, −492.7, 3456) facing east, scale 0.95).

**Rule for geometry** [hypothesis, built]: the coefficients that morph the head and every
part fitted to it (eyes, mouth, brows, beards, hair, hats) are the NPC's FGGS plus its
race's FGGS for its sex, value by value (`Appearance::shape_facegen`; FGGA likewise,
though every race's is zero, §4.2). The lip rig starts from the same shape
(`lips.rs`). `nvforever --no-race-facegen` draws faces from the NPC's values alone.

- Sunny Smiles [observed: NPC_ GSSunnySmiles, race Hispanic, female, head
  `headfemale.nif`]: her own FGGS has length 17.1, the race's 22.5, the sum 19.4 — the
  two partly cancel (mode 39: +7.18 and −8.07; mode 48: +6.77 and −9.78).
- From her own values: a long lower face, a heavy square jaw, small eyes. From the sum:
  a shorter lower face, a jaw tapering to a small chin, larger eyes — the game's face.
  With the pictures scaled to the same distance between the eyes, the eye line to the
  chin is about 1.6 eye distances in the game's picture, 1.85 from her own values and
  1.7 from the sum [observed, read off the pictures by eye, ±0.05].
- Still different [open]: in the game's picture her eyes are more open and her lips
  fuller, and the chin is a little shorter still. There she stands in her idle pose
  looking up at the player; ours is the bind pose. Whether the rest is pose, expression
  or the rule's strength is not known.
- The authored values agree [observed, `fnvtool actor appearance --all`]: in each of the
  23 race and sex groups with 8 or more distinct NPC_ records, the mean of the NPCs' own
  FGGS points away from the race's FGGS (cosine −0.04 to −0.68) — what faces edited on
  top of the race's shape and pulled part of the way back to a common taste would give,
  and not what unused race values would give.
- Other faces in the saloon, ours before and after, not compared with the game: Trudy
  (Caucasian, female; own 7.0, race 19.5) turns from a long narrow face into a round
  one; Joe Cobb (AfricanAmerican, male; own 26.5, race 16.1) gets a broader nose and
  jaw; a male Caucasian settler (own 6.3, race 13.2) changes least.

**Hair morph files are per shape** [observed, `Fallout - Meshes.bsa`]: of its 275 `.egm`
files, 121 sit beside a `.nif` of the same name; 134 are named after a hair model plus
one of its two shapes — `hairbunnohat.egm` (V 947) and `hairbunhat.egm` (V 472) for the
`NoHat` and `Hat` shapes of `hairbun.nif` (§2.4). Of the 105 models in
`characters\hair\`, 67 have that pair and no `.egm` of their own, 30 (brows, beards, glasses, an eye patch, a wig)
have their own, and 8 (`glasses…go.nif`, `powderedwiggo.nif`, `shadesgo.nif`)
have none. The engine looked only beside the model, so those 67 hair models were never
morphed: the hair kept the base head's shape and the scalp came through it once the
race's shape was added. Now a shape of a model without an `.egm` takes
`<model stem><shape name>.egm` when the file has exactly the shape's vertex count
(`FaceGenCache::morph`, `shape_sibling`). In the saloon 85 shapes are morphed and no
`.egm` mismatches its shape. The other 20 `.egm` files (headgear, e.g.
`armor\headgear\beanie\hatbeanie.egm`, `dlc05\armor\cowboyoutfit\cowboyhatm.egm`)
have no `.nif` of their name; which model each belongs to is [open].

Hair shine is a material question: docs/spec-notes/nif.md §6, "Hair".

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
  `problems`}]. Over the worn models (MODL, MOD3) of every ARMO, placed or not:
  `armor_models` (how many), `armor_rigid_bones` (models with unskinned shapes by the
  `Prn` they name, or `(no Prn)`), `armor_rigid_off_head` [{`model`, `prn`,
  `skinned_shapes`, `rigid_shapes`}] (those naming another bone than `Bip01 Head`),
  `armor_pipboy_sleeves` (models with `PipBoyOn…` and `PipBoyOff…` shapes: `on and off`,
  `on only`, `off only`), `armor_light_effect` (models with the Pip-Boy light's glow).

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
  [hypothesis: the game may pick other idles per sex, package or furniture]. Since part
  G the idle comes from the actor's locomotion clip set (§8.3), found by the same rule.
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

### 7.6 The head and neck bones, and the head tracking data (dialogue look)

What `nvforever` relies on to turn an actor's head to the player in a conversation
(docs/design/dialogue-play.md §8; `engine/nvforever/src/dialogue_look.rs`), 2026-10-10:

- **The chain.** In `characters\_male\skeleton.nif` the head hangs from two neck bones:
  `Bip01 Neck` → `Bip01 Neck1` → `Bip01 Head` [observed: the engine walks up from the
  head through its parents whose names contain "Neck" and logs the chain:
  `dialogue look: … on Bip01 Neck, Bip01 Neck1, Bip01 Head`].
- **Axes.** In the bind pose an actor faces +y of its own space with z up (its root is
  turned about z by its reference's heading, §3) [observed: actors face their
  reference's heading]. The head bone's own "forward" and "up" are therefore read from
  its bind-pose rotation (the inverse of it applied to +y and +z), with no assumption
  about how the bone's axes are named. A head turned with these axes looks at the
  player's eye in the screenshots, level [observed].
- **Body part data.** `DefaultBodyPartData` (BPTD 0x0000001D, model
  `Characters\_Male\skeleton.NIF`) has six parts. Its `Head` part: part node `Bip01
  Neck1`, VATS target `Bip01 Head`, IK start node (`BPNI`) `Bip01 Head`; flags
  Severable, IK Data, Explodable, IK Data - Is Head, IK Data - Headtracking; Tracking
  Max Angle 60.0. The other five parts (torso, arms, legs) have Tracking Max Angle 0;
  the two leg parts carry IK Data - Biped Data [observed: `fnvtool esm decode --edid
  DefaultBodyPartData` on FalloutNV.esm]. The fields' meanings: "Headtracking": the head
  will turn towards its target; "Tracking Max Angle": the largest angle the actor will
  turn its head to look at something; "Start Node": the base node of the part in the
  skeleton [documented: GECK wiki "Body Part", owner export]. [hypothesis: the angle is
  in degrees and is the largest turn to either side; whether the original turns only
  the start node or spreads the turn over the neck is not known.]
- **The `HeadTrack` float.** The human idle's `NiFloatExtraDataController` on `Bip01
  Head` (§7.1) fits what the wiki says of skeletons: a float extra data named
  `HeadTrack` marks the bone animated for head tracking, and a clip can animate that
  float through such a controller [documented: GECK wiki "NIF Block Types" and "NIF
  Animation Controllers", owner export]. [hypothesis: the idle's track is that float,
  and a clip uses it to weaken or switch off head tracking while it plays.] Not played.
- **What follows a turned head** [observed: screenshots of Easy Pete and Sunny Smiles,
  the head at up to 60° to a side]: the skinned head and neck (no gap at the collar),
  the rigid parts under `Bip01 Head` (hair, eyes, beard, hat), and the face while its
  lip sync runs (the morphed mesh is skinned like the mesh it replaces).
- **Turn clips.** `locomotion\mtturnleft.kf` and `mtturnright.kf` (§8.1) play while a
  standing actor's body is turned on the spot to the player, through
  `Locomotion::turn` as for the walk test and travellers.

## 8. Locomotion playback (part G)

`nvforever` plays an actor's idle, walk, run and turn clips according to its velocity and
turn rate, with a short cross-fade between clips and a playback rate matched to ground
speed. There is no AI yet (Layer 6): only the test aid `--actor-walk-test <speed>` moves
actors (§8.4). `--actor-pose bind|idle` keep their meaning: `idle` is the same selector
with every actor standing still. Plan and status: docs/design/static-actors.md part G.

### 8.1 Clips of `characters\_male` (survey)

`fnvtool actor anims` and `fnvtool actor locomotion` (§8.5) on the owner's install,
2026-10-09 [observed]. `characters\_male\locomotion\` holds 125 `.kf` files (133
sequences): 70 directly in it, 9 in `male\`, 9 in `female\`, 8 in `child\` and 29 in
`hurt\`. Every sequence loops (cycle type 0) and names `Bip01` as its accumulation root.

- **Stance prefixes** (file stem minus the sequence name): `mt` (no weapon), `pamt`
  (power armour: only `pamtidle.kf` and `male|female\pamtforward.kf`), and the weapon
  prefixes `h2h`, `1hm`, `1hp`, `2ha`, `2hh`, `2hl`, `2hm`, `2hr`, `1gt`, `1lm`, `1md`
  (animation.md §3.2 for their reading).
- **Unarmed set** (the one the engine plays; sequence name = move):

| Move | File (below `characters\_male\locomotion\`) | Length (s) | Root motion per cycle (x, y) | Speed (units/s) |
|---|---|---|---|---|
| Idle | `mtidle.kf` | 4.000 | none (no `Bip01` track) | 0 |
| Forward | `male\mtforward.kf` | 1.200 | (0, +102.31) | 85.26 |
| Backward | `male\mtbackward.kf` | 1.133 | (0, −76.43) | 67.44 |
| Left | `male\mtleft.kf` | 1.200 | (−103.84, 0) | 86.53 |
| Right | `male\mtright.kf` | 1.200 | (+102.82, 0) | 85.68 |
| FastForward | `male\mtfastforward.kf` | 0.800 | (0, +283.11) | 353.88 |
| FastBackward | `male\mtfastbackward.kf` | 0.800 | (0, −226.00) | 282.49 |
| FastLeft | `male\mtfastleft.kf` | 0.800 | (−283.00, 0) | 353.75 |
| FastRight | `male\mtfastright.kf` | 0.800 | (+283.00, 0) | 353.75 |
| TurnLeft | `mtturnleft.kf` | 1.000 | none (no `Bip01` track) | 0 |
| TurnRight | `mtturnright.kf` | 1.000 | none (no `Bip01` track) | 0 |

  `female\` has the same eight walk and run files with the same lengths and nearly the
  same root motion (Forward 102.08). `child\` too, except Left and Right (±81, 67.5
  units/s) and FastLeft and FastRight (0.867 s, ±306.70, 353.88 units/s). Idle and
  turns exist once, in `locomotion\`.
- **Weapon stances** in `locomotion\`: `h2h`, `1hm`, `1hp`, `2hh`, `2hl` have all eight
  walk and run moves; `2ha`, `2hr` have Forward and the four fast moves; `2hm` and `1gt`
  only the four fast moves; `1gt`, `1lm`, `1md`, `2hh` have TurnLeft and TurnRight.
  Weapon turns of `1hm`, `1hp`, `h2h` and weapon idles (`1hmidle.kf`, `h2hidle.kf`:
  0.333 s, 3–7 tracks) sit directly in `characters\_male\`, beside jumps, block idles
  and the `sneak*` (all moves, 0.867–1.733 s) and `swim*` sets. Several weapon walks have
  46 tracks against 60 in `mt` (`1hm`, `h2h`), and weapon turns 11–16 [hypothesis:
  weapon clips are partial and layered over the `mt` clips by priority; Layer 6].
- Eight `*fastbackward.kf` files (`1gt`, `1hm`, `1hp`, `2ha`, `2hh`, `2hl`, `2hr`,
  `h2h`) hold a second sequence `FastRight` (0.733 s) after `FastBackward`; the engine
  takes the first sequence of a file.
- `hurt\` holds `mt`, `1hm`, `1hp` and `h2h` walk and run variants with a trailing `_`
  (e.g. `mtforward_.kf`), plus `mtidle`, `pamtidle` and the `mt` turns [hypothesis:
  crippled-leg locomotion; not used].

### 8.2 Root motion

- 121 of the 133 sequences drive `Bip01`; in all 121 its track has a translation channel
  (B-spline or keys), a **static identity rotation and scale 1** [observed]. The 12 that
  do not drive it are the four idles (`mtidle`, `pamtidle` and their `hurt\` copies) and
  the turns of `mt`, `1gt`, `1lm` and `1md` [observed].
- `Bip01`'s translation runs from (0, 0, 0) at the start to the per-cycle offset at the
  stop, with z = 0 throughout; forward moves go along +y, backward along −y, Left along −x,
  Right along +x. The pace is close to even (Forward: 12.2 units in the first 0.15 s,
  14.5 in the next) and strafes sway up to ±1.5 units across the line (`mtleft` y)
  [observed: `--samples 9`].
- **Actor axes**: +y is the actor's forward, +x its right, z up [hypothesis, from the
  direction of each move's root motion].
- The pelvis height lives on `Bip01 NonAccum` in every clip: 59–66 units up, bobbing by
  2–7 units per step, and turned about 90° about z (as in `mtidle.kf`, §7.2) [observed].
- Turn clips carry no root rotation, so their turn speed is not in the clip [observed:
  no `Bip01` track, or a static one in `2hhturn*`; hypothesis: the game sets it].
- **Playback**: the engine extracts the root motion and moves the actor itself. While a
  clip with a `Bip01` track plays, `Bip01` takes the sampled translation minus the
  linear accumulation `(t − start) / length × per-cycle offset`, which keeps the small
  in-cycle deviations and removes the travel [engine rule; hypothesis that the game
  moves the actor at the clip's average speed]. A clip without a `Bip01` track holds it
  at the identity (§7.2), so all locomotion clips blend on the same root.
- The root's **height** counts from the track's start: sampling also removes the
  track's start z. Only the 2ha/2hh `Holster` clips (and their power-armour and sneak
  copies) start `Bip01` off height 0, at a constant 67–68 with no `Bip01 NonAccum`
  track; taken as they are they lifted NPCs 66 units off the ground
  [observed; rule hypothesis, docs/spec-notes/floating-actors.md].

### 8.3 Selection, blending, phase and speed (engine)

All rules here are engine choices unless tagged otherwise. Layer 6 replaces the inputs
(AI packages, paths) and should keep the playback.

- **Inputs** per actor and frame: velocity in the actor's axes (§8.2) and turn rate
  (radians per second, positive = to the left).
- **Choice** (`fnv_actor::locomotion::choose`):
  - ground speed below 5 units/s: standing. Turning faster than 15°/s → TurnLeft or
    TurnRight by sign; else Idle.
  - otherwise the direction by the velocity's angle from forward: within 45° Forward,
    beyond 135° Backward, else Left or Right.
  - walk or run: run when the speed is above the geometric mean of the direction's walk
    and run clip speeds (Forward: √(85.26 × 353.88) ≈ 173.7 units/s).
  - playback rate = ground speed ÷ the clip's root-motion speed, clamped to 0.4–2.5
    (outside that range the feet slide); idle and turns play at rate 1 (§8.2).
- **Cross-fade**: when the choice changes, the new clip fades in over its blend time while
  the old one fades out (weights sum to 1, smoothstep). Blend time = the `Blend: N` text
  key of the incoming clip read as N frames at 30 per second (`Blend: 6` → 0.2 s,
  `Blend: 9` → 0.3 s, `Blend: 3` → 0.1 s, `Blend: 15` → 0.5 s), else 0.2 s [hypothesis:
  N counts 30 fps frames; every text key time in these files is a multiple of 1/30 s;
  `mtidle.kf` has no such key]. A change requested during a fade waits for the fade to
  end, so weights never jump.
- **Pose blend**: per skeleton node, translation and scale are lerped and rotations
  nlerped along the shorter arc by weight; a node that only one clip drives blends
  towards its bind pose by the missing weight.
- **Phase sync**: between two walk/run clips, the incoming clip starts at the same
  footfall phase as the outgoing one, using the time of each clip's first `Enum: Left`
  text key as its left-footfall mark [hypothesis: `Enum: Left`/`Enum: Right` are footstep
  sound events; the `m:L`/`m:R` keys of some files do not agree with them
  (`male\mtfastforward.kf` puts `m:R` at 0.40 s, between `Enum: Left` at 0.17 s and
  `Enum: Right` at 0.57 s), so they are not used]. During such a fade both clips advance
  at the weight-blended cycle rate so the feet stay in step. Idle and turns start at
  phase 0, except an actor's first idle, which starts at its spread phase (§7.3).
- **Body**: female actors use `locomotion\female\`, others `locomotion\male\`
  (`fnv_actor::locomotion::clip_candidates`; a missing file falls back to `male\`).
  Children (`child\`) need the race's child flag, which the records reader does not take
  yet [open].

### 8.4 Test aid `--actor-walk-test <speed>`

Every actor within `--actor-walk-test-radius` (default: unlimited) of the start position
walks a fixed, repeating script along its own forward axis, in the plane of its placement
(it does not follow the ground, so on slopes it floats or sinks): forward 300 units at
`<speed>`, stand 1 s, turn left 180° at 90°/s, forward back to the start, stand 1 s,
turn right 180°, forward out again, stand 1 s, walk back **backward** at 0.75 ×
`<speed>`, stand 1 s; repeat. Each actor starts at a spread offset in the script (the
§7.3 phase). At 85 units/s the forward legs play at rate 1.0; at 350 they run. With
`--actor-pose-time t`, every walking actor instead runs the script and its blend from the
start to `t` in 1/60 s steps and holds there, so screenshots repeat exactly. Speeds and
the line length are in the actor's own units (its scale applies on top).

### 8.5 CLI contract

`fnvtool actor locomotion <Data> --nif-schema <dir> [--folder <vfs folder>]
[--samples N]` → `fnv.actor.locomotion/1`. `--folder` defaults to
`meshes\characters\_male`. Top level: `schema`, `folder`, `files`, `prefixes` (stance
prefixes of the stems in `<folder>\locomotion\…`), `sets`, `archives`.

- `files[]`, by path, for every `.kf` below `<folder>\locomotion\`: {`path`, `sequences`
  [{`name`, `start_time`, `stop_time`, `cycle_type`, `frequency`, `accum_root`,
  `root_driven`, `root_translation` [x, y, z] (per cycle), `root_yaw_deg` (summed over 64
  steps), `speed` (units/s, horizontal), `turn_deg_per_s`, `text_keys` [[time, text]],
  `root_channels`? {`translation`, `rotation`, `scale`: `none` | `static` | `keys` |
  `bspline` | `euler`}, `root_start`? {`translation`, `rotation` (w, x, y, z), `scale`},
  `samples`? {node: [{`t`, `translation`, `yaw_deg`}]} (with `--samples N` ≥ 2: the
  accumulation root and `Bip01 NonAccum` at N even times)}]} or {`path`, `error`}.
- `sets[]`: for each body (`Male`, `Female`, `Child`) and prefix: {`body`, `prefix`,
  `clips` {move: path}, `missing` [move]}, by the lookup of §8.3.

Library: `fnv_actor::locomotion` (`Move`, `Body`, `clip_candidates`, `LocomotionSet`,
`RootMotion`, `root_motion`, `foot_phase`, `blend_time`, `choose`, `Blend`).

### 8.6 Performance

Owner's machine, 2026-10-09, release build [observed].

- **Per pose** (`nvf-assets` example `blend_bench`: `BlendPlayer::sample` over 20,000
  poses, no Bevy transform writes; three runs):

| Pose | Tracks | µs per pose |
|---|---|---|
| Idle (`mtidle.kf`), one clip | 60 | 18.8–25.4 |
| Walk (`male\mtforward.kf`), one clip | 59 | 6.0–7.1 |
| Idle → walk, two clips | 119 | 25.6–26.9 |
| Walk → run (`male\mtfastforward.kf`), two clips | 117 | 21.1–21.9 |

  A blended pose costs about the sum of its two clips: rotation channels set the price.
  `mtforward.kf` has 56 keyed rotation channels (3 static), `mtidle.kf` 51 B-spline and
  `mtfastforward.kf` 52 B-spline [observed: `kf_bind_check`]. Part E measured 20.2 µs per
  idle pose with `kf_bind_check` (§7.5).
- **In the engine** (the `actor animation` line of the `--run-seconds` summary: choosing,
  blending, sampling and writing the bones' local transforms; not Bevy's transform
  propagation or skin upload):

| Run (40 s, `--cell Goodsprings`) | Exit, "run finished" | Actors (walking) | µs per actor and frame | Clips per actor and frame | Frames blending two | fps |
|---|---|---|---|---|---|---|
| `--auto-fly 4000` (all standing) | 0, yes | 34 (0) | 37.1 | 1.00 | 0 % | 25.5 |
| `--auto-fly 4000 --actor-walk-test 120` | 0, yes | 33 (33) | 37.8 | 1.12 | 12.5 % | 25.9 |
| fixed camera at the saloon, `--actor-walk-test 120` | 0, yes | 3 (3) | 41.1 | 1.12 | 11.9 % | 19.5 |

  Part E's figure (§7.5) was 41.6 µs, later 29.3 µs, per clip and frame for the single
  idle clip; part G stays in that range with blending on (up to 17 actors posed at once,
  0.17 ms per frame). The frame rate is set by other work.

### 8.7 Visual check

Screenshots held at fixed script times (`--actor-walk-test 85 --actor-pose-time t`, and
`350` for the run), kept privately (`C:\fnv-private\references\partg\`), Easy Pete on
the Prospector Saloon porch, Goodsprings [observed]:

- Walking out (0.3 s, 0.9 s): side view, facing the way he moves, legs at different
  points of the stride, feet on the porch boards.
- Walking back (8.0 s): facing west, mid-stride. Walking backward (19.5 s): facing east
  and moving west with a backward stride. Turning (5.3 s): turned away from the camera,
  part way through the left turn from east to north.
- Running (350 units/s, 0.3 s): a running stride with arms bent.
- Doc Mitchell in his house (1.5 s): walking away from the camera, back turned, so the
  clips' forward (+y) is the way an actor faces [observed; confirms §8.2's axes].
- Not checked yet: foot sliding over time (needs video or a sequence of frames per step),
  and how the game itself blends and phases these clips (an in-game capture of an NPC
  walking, stopping and turning would settle §8.3's hypotheses).

### 8.8 Notes for later work

- **Layer 6**: AI sets `Locomotion::velocity` and `turn` (`engine/nvforever/src/locomotion.rs`)
  instead of the walk test. Weapon stances need a second, partial layer over the `mt`
  clips (weapon walks have 46 of 60 tracks; priorities are in the controlled blocks,
  `fnv_nif::anim::Track::priority`) and a stance prefix per equipped weapon type
  (animation.md §3.2). Sneak and swim sets sit directly in `characters\_male\`
  (`sneakmt*`, `swimmt*`) and would need `clip_candidates` to look there too.
- **Turn speed**: turn clips carry no root rotation (§8.2); the game's turn speed and
  whether it scales the turn clip's rate are open. The walk test turns at 90°/s; actors
  turning on the spot turn at 120°/s (§8.9).
- **Ground following**: the walk test moves in the placement's plane; a real mover needs
  the ground height (Layer 4 physics) per step.
- **`nvf-viewer --anim`** (done 2026-10-09): the viewer's single-clip player
  (`AnimState::start`) holds an undriven accumulation root at the identity (§7.2) and
  removes a driven root's per-cycle travel in proportion to the cycle phase
  (`Playing::in_place`, the rule of §8.2), so walk and run clips play in place there
  too. World objects' clips keep their travel.
- **Creatures (part D)**: done for the idle (§10.3); creature walk and turn clips are
  not surveyed and not played.

### 8.9 Turning on the spot

Added 2026-10-10. The turn clips were already chosen for a standing actor that turns
(§8.3), but nothing made an actor stand and turn: travellers swung to their path's
direction at 720°/s while already walking, so `mtturnleft.kf` and `mtturnright.kf` played
only in the walk test and on the player's body. All rules here are engine choices
[design]; none is a measured game value.

- **What the data says.** The turn clips are 1 s loops with no accumulation-root track
  and a pelvis (`Bip01 NonAccum`) that stays at yaw 90° and between 64.0 and 66.9 units
  up for the whole loop; text keys `Blend: 3`, `Enum: Left` at 0.5 s and
  `Enum: Right` at 0.93 s in `mtturnleft.kf`, `Enum: Left` at 0.37 s in `mtturnright.kf`
  [observed: `fnvtool actor locomotion --samples 9`, owner's install, 2026-10-10]. So
  the clip holds no turn angle or speed (§8.2). `FalloutNV.esm` has no game setting
  whose Editor ID contains "turn", and only `fVATSTargetRotateMult` with "rotat"
  [observed: `fnvtool esm inventory`]; the speed is not in the plugin data.
- **Turn speed**: 120°/s standing (`fnv_nav::follow::SPOT_TURN_RATE`) [design: half a
  turn in 1.5 s, three footfalls]. The clip plays at rate 1 whatever the angle. The AI
  observer captures record each actor's heading per sample (ai-observer.md), so a
  capture of an NPC turning to a new package target could replace this number [open].
- **Before walking** (`fnv_nav::Walker`): a walker that stands (it did not move in its
  last update: at the start of a path, after waiting at a door, after arriving through a
  teleport door) and whose way lies more than 20° off its heading
  (`SPOT_TURN_START`) turns on the spot until it faces the way, then sets off; its
  updates return `WalkEvent::Turned` meanwhile. Smaller differences, and every corner
  met while walking, are steered at the walking turn rate (720°/s then; 180°/s since
  2026-10-11, below). The feet
  do not move during the turn, so path following is unchanged; a trip takes up to 1.5 s
  longer per standing start.
- **Standing with nowhere to go**: `Walker::face(heading, dt)` turns a walker-moved
  actor (travellers; the AI packages' agents) towards a compass heading at the same
  speed and says when it faces it. For an actor nothing else moves, the component
  `TurnTo { heading }` on its root (`engine/nvforever/src/locomotion.rs`) does the
  same: the system `turn_to` turns the root about z, sets `Locomotion::turn` so the turn
  clip plays, and removes the component when the actor faces the heading (a component
  taken away mid-turn stops the turn). Headings are compass headings in game space
  (0 = +y north, π/2 = +x east), as `Walker::heading`. The conversation partner's body
  turn (`engine/nvforever/src/dialogue_look.rs`, `BodyTurn`) is separate: it eases its
  own turn and sets `Locomotion::turn` itself; `TurnTo` is for turns at a steady speed.
- **Clip choice** is §8.3's, unchanged: both movers give the actor zero velocity and a
  turn rate of ±120°/s, above the 15°/s threshold; the turn clip fades in over its
  `Blend: 3` (0.1 s) and the idle or walk that follows over its own blend time.
- **Test aid `--actor-turn-test <radius>`**: standing NPCs within `<radius>` game units
  of the camera get a `TurnTo` towards it whenever it is more than 20° off their
  heading. On exit the log says `turn test: N turns on the spot begun, M finished, D
  degrees in all`. Not with `--actor-walk-test`; `--npc-travel` travellers are turned by
  their walker, which overrides it. The `--run-seconds` summary gains the line `actor
  turns: N actor frames playing a turn-on-the-spot clip`.
- **Turning while walking** (added 2026-10-11, owner's playtest: "they are able to make
  a complete 180 degree turn in a frame or two") [design]. The walking turn rate is
  180°/s (`fnv_nav::follow::WALK_TURN_RATE`, was 720°/s). The game's characters turn
  at the setting `fCharacterDefaultTurningSpeed` [documented: GECK wiki, Settings
  list]; it is not in FalloutNV.esm, so its value is the executable's and unknown.
  Creatures carry their own, CREA `TNAM` "Turning Speed" [documented: xEdit CREA]: 100
  (mole rat), 90 (mantis), 75 (sentry bot), 50 (Protectron), 45 (giant ant), 40
  (bighorner), 0 on templated ones [observed, a sample of 12 of 1,578 records]; read as
  degrees per second [hypothesis]; not used yet. While its heading catches up with the
  way to go the actor slows down: it keeps `0.3 + 0.7 × max(cos off, 0)` of its speed
  (`turn_slowdown`; a right-angle corner costs about half a second). Actors moved by
  the character controller (near the player, `residents.rs`) walk along their heading
  while it is within a quarter turn of the way, so they round a corner in an arc
  (`arc_step`); on the navmesh (`Walker`, off-screen and the last steps to a place) the
  feet keep to the path. Actors moved by the character controller now also turn on
  the spot before setting off, as the walker does (§8.9 above); they did not.
- **Other turns made gradual** (2026-10-11) [design]: an actor arriving at an idle
  marker or a sandbox spot turns to its heading on the spot at 120°/s (`fnv_ai::Agent::
  face_to`; was set at once); an actor walking the last steps to furniture takes at
  least as long as turning to the entry heading at 120°/s needs (`furniture.rs`; was
  as little as 0.15 s); an actor getting off furniture keeps the heading the getting-off
  clip left it with (its back to the furniture) when its walk goes on (`residents.rs`;
  the walker kept the heading it sat down with and the body snapped half about in one
  frame). The conversation turn (`dialogue_look.rs`, `speech_look.rs`) is unchanged
  (eased, about 150°/s).
- **Measured** [observed in NVForever, the residents smoke test, `turn summary` line]:
  the fastest turn of a drawn resident between two frames was 720°/s with 2 frames over
  270°/s before, 180°/s with none after.
- **Not done**: weapon-stance turn clips (`1hpturnleft.kf`, …; the player's body picks
  them, `fnv_actor::player_clips::turn_clips`, NPCs play `mt` only); creature turns
  (§10.3: their turn clips sit in the skeleton's folder and are found by
  `clip_candidates`, not checked on screen); scaling the clip's rate with the turn
  speed; an actor facing its target on arrival (left to AI packages).

### 8.10 Creature speeds and the alert stance

Added 2026-10-16 (owner's playtest: cazadors moved far too slowly and their movement
stopped and started, as Victor's walk had before #258). Code:
`engine/nvforever/src/creature_pace.rs`, `fnv_ai::Ai::speeds`.

- **Cause in NVForever** [observed in the code]: every actor walked at 86.8 and ran at
  344 units/s (`fnv_ai::AiConfig`, the player's speeds); in combat a creature could only
  be slowed below 344 (`npc_combat::pace`, list 5 item 25). The cazador's `mtforward.kf`
  moves 216.73 units/s and `mtfastforward.kf` 545.54 [observed: `fnvtool actor
  locomotion --folder meshes\creatures\nvcazadores`], so its walk played at the slowest
  clip rate (0.4) and 344 sat on the walk/run switch (the geometric mean of the two
  clips' speeds, 343.9, §8.3): any slow-down swapped the clips back and forth.
- **Measured in the original game** [observed: AI observer (ai-observer.md), save
  `nvftest`, 2026-10-16; outputs in `C:\fnv-private\capture\out\obs_cazador_*`, not
  committed]. Positions every 5 game loops (about 0.16 s), horizontal speed between
  samples.
  - Calm: non-hostile cazadors at the Tribal Village (cell -19, 7; `tgm`, `tdetect`; the
    owner's tip), sandbox package, not running, 1,665 samples over a game hour: young
    cazadors (VCrTier3CazadorSmall: base scale 0.5, Speed Mult 300) held 224–233
    units/s (median and 90th percentile), the medium ones (scale 1.0, Speed Mult 250)
    373–386.
  - Charging: NVCrCazador and VCrTier3CazadorSmall spawned in Goodsprings by
    `PlaceAtMe`, god mode, in Combat and running (IsRunning, IsMoving forward): the
    medium ones up to 920–974 units/s, the young up to 576. With combat AI off (`tcai`)
    they hovered at 75–120.
- **Rule** [observed fit; hypothesis for other creatures]: a creature moves at its clip's
  root-motion speed × its scale × Speed Mult / 100 × 0.70 (`CREATURE_SPEED_FIT`): the
  walk 216.73 × 0.5 × 3.0 × 0.70 = 228 and 216.73 × 1 × 2.5 × 0.70 = 379; the run
  545.54 × 0.5 × 3.0 × 0.70 = 573 and 545.54 × 2.5 × 0.70 = 955, all within the
  measurements. The share is the same for both kinds (0.701 and 0.701 when walking), so it
  is proportional to Speed Mult, not an offset. No game setting gives 0.70
  (`fMoveBaseSpeed` 77, `fMoveRunMult` 4, `fMoveSneakMult` 0.57 are the only movement
  ones [observed: `fnvtool esm inventory`]); people at Speed Mult 100 walk at their clip's
  speed (§8.3, physics.md §6.3), so why creatures keep 0.70 is open. Other creatures
  are not measured.
- **Alert stance** [observed: the owner, 2026-10-16: "the cazadors scurry along the
  ground when not alert and fly when they are alert/in combat"]: the cazador's folder
  has `h2hforward.kf` (298.15 units/s), `h2hfastforward.kf` (545.54) and `h2hidle.kf`
  beside its `mt` clips; the dog has `h2hforward` and `h2hfastforward`, the mirelurk king
  `h2hforward`, no other creature folder of the base archives has `h2h` movement clips
  [observed: the meshes and Update archive listings]. Rule [design on the owner's
  observation]: outside the Normal detection state (Alert, Combat, Lost) such a creature
  moves on its `h2h` clips (missing moves taken from the `mt` set) at their speeds by the
  rule above; the change of set is not cross-faded.
- **Clip rate** [hypothesis: the movement comes from the clips, §8.2]: the clips play
  Speed Mult × 0.70 faster (`Locomotion::pace`) and the walk or run is chosen for the
  velocity divided by that, so a medium cazador walking at 379 units/s plays its walk
  clip at 1.75, not its run clip at 0.7.
- **Turning** [observed: the calm session's headings, 0.16 s apart]: medium cazadors
  turned at up to 304°/s, young ones up to 137°/s, both plateaus; 85–89% of their
  turns faster than 60°/s happened while they moved (over 20 units/s). Rule
  [hypothesis: proportional to scale, fitted on these two kinds; CREA `TNAM`, 720 and
  90 on these records, does not match]: a creature turns at 300°/s × its scale walking
  and standing (`CREATURE_TURN_RATE`), and turns on the spot before setting off only
  when its way is more than 60° off (`CREATURE_SPOT_TURN_START`, design; people: 20°,
  §8.9). With the people's 120°/s and 20°, a fast cazador stopped to turn at the start
  of nearly every short sandbox walk.
- **Walk/run changes** [design]: the gait changes only when a speed smoothed over 0.2 s
  (`Locomotion::gait_speed`) is 15% past the switch point (`GAIT_HYSTERESIS`). In
  NVForever a creature's drawn speed jumped to two or three times its walk for a single
  frame (455 → 1,363 → 455 units/s, logged at the Tribal Village), and each jump began a
  0.2 s cross-fade to the run clip and back: a hitch in the walk, the stop-and-start the
  owner saw. Where the jumps come from is not found.
- **Deathclaw's walk**: its walk clip is `locomotion\mtfoward.kf` (sequence Forward, 120
  units/s) [observed]; `clip_candidates` looks for that spelling last, so deathclaws now
  have a walk.
- **Alert look** (`engine/nvforever/src/creature_stance.rs`) [design on the owner's
  description, 2026-10-16: "it's like they are drawing a weapon, i.e. the hands"]: a
  creature that is alerted (Alert, Combat, Lost, or SetAlert; as people draw their
  weapons, alert.md §5) plays its `h2hequip.kf` once, then holds `h2haim.kf` over its
  locomotion by track priority; calm again, `h2hunequip.kf` (else `h2hholster.kf`) once,
  then its locomotion alone. The draw cycle is the weapons' (`WeaponAnim`, §9.4) on blend
  layers 2 and 3; a creature needs no `Weapon` node and gets no `WeaponHold`, so its
  attacks are found as before (`attack_anims.rs`) and play over the aim (layers 4 and 5).
  At rest none of its stance plays: the creatures' holster clips are whole-body poses.
  Creatures with an `h2h` movement set (cazador, dog, mirelurk king, above) are left
  out: that set is their alert look [design]. With the stance a dog that stopped at the
  player (body gap 5–17 units, measured from the posed torso, `npc_combat`) was 27–42
  away moments later, past the 24-unit hit gap, and ran in again: 7 blows instead of 15
  in smoke `combat-dogs` [observed in NVForever, 2026-10-16; hypothesis: the dog's aim
  pose holds its torso further back than its idle].
  - Which creatures [observed: `Fallout - Meshes.bsa` listing]: the 25 creature folders
    with `h2haim.kf` at their root, all with `h2hequip.kf` and `h2hunequip.kf` except
    `nvsporeplant` (aim only): brahmin, centaur, deathclaw, dog, ghoul, giantant,
    mirelurk, mirelurkking, molerat, nightstalker, nvbighorner, nvcazadores, nvgecko,
    nvgiantrat, nvmantis, nvsecuritron, nvsporecarrier, nvsporeplant, protectron,
    queenant, radroach, radscorpion, smbehemoth, smspinebreaker, yaoguai.
  - Priorities [observed: `kf_layer_survey`]: the aim clips sit under the walk and run
    clips (priority 30 on every bone): deathclaw aim legs and tail 20, spine and upper
    arms 30, forearms and hands 35 (its `mtidle.kf` is 20 throughout); ghoul legs 25,
    spine and upper arms 30, hands 35 (idle 20); dog 25, cazador 21, mirelurk king 20.
    So a moving creature keeps its legs (and shares its spine and upper arms with the
    aim at equal priority, §9.4 blending) and raises its forearms and hands; a standing
    one takes the aim's pose (the deathclaw's legs half its idle's, both at 20).
    Equip and unequip: deathclaw legs 25, upper body 55; ghoul equip (3.0 s) 55 on
    every bone, legs included, so a ghoul that runs at once slides with still legs for
    the equip's length; cazador equip 33
    throughout [observed; whether the original holds a creature still meanwhile is
    open].
  - Seen in NVForever [observed: smoke `creature-alert-stance` and
    `creature-alert-stance-ghoul`, screenshots not committed, 2026-10-16]: calm, the
    deathclaw's arms hang and the ghoul stands upright; alerted, the deathclaw raises
    its claws over its shoulders and the ghoul hunches with its hands raised; both go
    back after the unequip clip. Deathclaws fighting a feral ghoul play their attacks
    with the stance drawn.
- **Not done**: the aim's up and down forms for creatures that have them (ghoul,
  protectron, smbehemoth, smspinebreaker: `h2haimup.kf`, `h2haimdown.kf`; people's are
  in `attack_anims.rs`); the creature alert look is not compared with the original game
  (when the equip plays, whether a creature stops for it); speeds of a creature not yet
  drawn (it moves at the people's speeds until its clips load); creature turn speed
  (CREA `TNAM`, §8.9).

## 9. Weapons: holstering and drawing (part H)

An NPC that carries a weapon shows it holstered and can draw and holster it with the
game's own equip clips, layered over its locomotion by track priority. Nothing decides
yet *when* to draw (Layer 6: combat, packages); the test aid `--actor-weapon-test <s>`
toggles it. Plan and status: docs/design/static-actors.md part H.

### 9.1 Clips (survey)

`fnvtool actor anims` and `fnvtool actor weapons` (§9.5) on the owner's install,
2026-10-09 [observed]. For each stance prefix (animation.md §3.2) `characters\_male\`
holds four clips, named `<prefix><clip>.kf`:

| Clip | Sequence | Cycle | Length | What it does |
|---|---|---|---|---|
| `holster` | Holster | clamp | 0.1–0.167 s | pose of the holstered weapon: drives `Weapon` (priority 55; 5 in `2hm`, 20 in `1gt`, `1md`, `2hh`), sometimes weapon parts, and one or two body nodes at priority 5 |
| `equip` | Equip | clamp | 0.367–2.0 s | draws: legs at priority 25–27, upper body at 55 |
| `aim` | Aim | loop | 2.0–13.3 s | weapon at the ready: legs 25 (`2hm` 15), spine 30, arms 35–45 |
| `unequip` | Unequip | clamp | 0.367–2.0 s | holsters: priorities as `equip` |

Per prefix (length of equip / unequip in seconds; where the weapon hangs, §9.2):

| Prefix | Types (§9.3) | Equip / unequip | Holstered at | Drawn at |
|---|---|---|---|---|
| `h2h` | Hand to Hand | 0.40 / 0.37 | `Bip01 R ForeTwist` | `Bip01 R ForeTwist` |
| `1hm` | Melee (1 Hand) | 0.90 / 1.10 | `Bip01 Pelvis` | `Bip01 R Hand` |
| `2hm` | Melee (2 Hand) | 1.20 / 1.20 | `Bip01 Spine2` | `Bip01 R Hand` (its aim clip names `Bip01 L Hand`, animation.md §9.10) |
| `1hp` | Pistol (Ballistic, Energy) | 0.37 / 0.47 | `Bip01 Pelvis` | `Bip01 R Hand` |
| `2hr` | Rifle (Ballistic, Energy) | 0.67 / 0.83 | `Bip01 Spine2` | `Bip01 R Hand` |
| `2ha` | Rifle - Automatic | 1.00 / 0.83 | `Bip01 Spine2` | `Bip01 R Hand` |
| `2hh` | Handle | 1.00 / 1.00 | `Bip01 R Hand` | `Bip01 R Hand` |
| `2hl` | Launcher | 2.00 / 2.00 | `Bip01 Spine2` | `Bip01 R Hand` |
| `1gt` | Grenade Throw, Thrown | 1.00 / 1.00 | `Bip01 Pelvis` | `Bip01 R Hand` |
| `1lm` | Land Mine | 1.00 / 1.00 | `Bip01 Pelvis` | `Bip01 R Hand` |
| `1md` | Mine Drop | 1.00 / 1.00 | `Bip01 Pelvis` | `Bip01 R Hand` |

- The stance clips also drive the weapon model's own moving parts, `##`-named nodes
  (`##357Cylinder`, `##MinigunBarrel`, …), mostly at priority 0 [observed: the tracks the
  skeleton lacks].
- The unarmed locomotion clips are priority 30 (walk, run) and 10 (`mtidle.kf`), and none
  of them drives `Weapon` [observed: `kf_bind_check`].
- `pa*` (power armour) and `sneak*` variants exist for several prefixes; not used.

### 9.2 Where the weapon hangs

- `skeleton.nif` has a node `Weapon` under `Bip01 R Hand`, at (6.17, 1.99, 0.76)
  [observed]. A weapon model (e.g. `weapons\1handpistol\9mm.nif`) is a `BSFadeNode` at the
  identity with its shapes and `##` part nodes below [observed].
- The stance clips carry text keys `prn: <bone>` (also `Prn:`, and once `prn: Bip01 R
  Hand  -at none`) and `Attach`/`Detach` [observed]. Reading [hypothesis]: from a `prn:`
  key on, the `Weapon` node hangs from the named bone, and the clip's `Weapon` track
  gives its transform relative to that bone. Equip clips end on the hand (`1hpequip`:
  `prn: Bip01 R Hand` at 0.13 s), unequip clips on the holster bone (`1hpunequip`:
  `prn: Bip01 Pelvis` at 0.20 s), holster clips name the holster bone. `Attach`/`Detach`
  come close to the `prn:` keys but not always in the same order (`2haequip`: `Attach`
  0.37 s, `prn: Bip01 R Hand` 0.93 s). The clips' `Weapon` track jumps from the holster
  pose to the hand pose at `Attach` and back at `Detach`, not at the `prn:` keys
  [observed: animation.md §9.10].
- Engine rule (`fnv_actor::weapon::mount_bones`, `WeaponAnim::parent`): holstered, the
  weapon hangs from the holster clip's last `prn:` bone (else the unequip clip's); drawn,
  from the equip clip's (else the aim clip's, else `Weapon`'s skeleton parent), as the
  player body; while equipping or unequipping, it changes from one to the other at the
  clip's `Attach` / `Detach` key (a clip without one: the last `prn:` bone it has
  passed, starting from where it was). Until 2026-10-16 the drawn bone was the aim
  clip's and the change came at the `prn:` keys: NPCs held the baseball bat end over
  end (hung from the left hand, owner's playtest 2026-10-16), and every stance's weapon
  sat in the wrong place between the `prn:` key and `Attach` / `Detach`.
- That NPCs show holstered weapons at the pelvis or on the back is [hypothesis, from the
  holster clips]; an in-game screenshot of an NPC with a holstered pistol and one with a
  rifle would settle it.

### 9.3 Which weapon, which stance

- WEAP: `MODL` third-person model, DNAM "Animation Type" (14 values), DNAM flags
  "Embedded Weapon" and "Non-Playable", DATA base damage [documented: xEdit WEAP;
  `schemas/fnv`]. 496 WEAP records, 372 drawable (a model, not embedded, a known type)
  [observed].
- An actor's weapon (`fnv_actor::weapon::choose_weapon`): among its carried items (the
  same leveled-list results as its worn armour, §2.3), the drawable weapon with the
  highest base damage, the first carried on ties [engine choice standing in for Layer 6's
  equip logic].
- Type → prefix (`AnimType::prefix`). The ESM gives only the type (DNAM); no field names
  the clips, so the link is read from two kinds of evidence in the data [observed,
  2026-10-09]:
  - **Weapon parts.** A weapon model's moving parts are `##`-named nodes, and the stance
    clips animate them by name. Matching the parts of the weapons placed NPCs carry
    against the `##` tracks of every `<prefix>*.kf` in `characters\_male\` (all clips of
    a prefix, not only the four of §9.1): the laser pistol's `##LPSideLatch`,
    `##LPSmallEnergyCell`, `##LPTrigger` occur only in `1hp` clips, as do the parts of the
    plasma pistol, Euclid's C-Finder, Plasma Defender and flare gun (Pistol - Energy →
    `1hp`); the laser rifle's `##LR_CellReleaseSpring` occurs only in `2hr` clips (Rifle -
    Energy → `2hr`; its `##MicroFusionCell` is also in `2ha` and `2hh` clips); the frag
    grenade's `##GFPin`, the plasma grenade's slides and the dynamite's `##Lighter` only in
    `1gt` clips (Grenade Throw → `1gt`); likewise `##MinigunBarrel` (`2hh`, Handle),
    `##127SmgClip` (`2ha`, Automatic), `##mlIndicators` (`2hl`, Launcher), and every
    ballistic pistol's and rifle's parts in `1hp` and `2hr`. Generic part names
    (`##Trigger`, `##Clip`) occur under several prefixes and decide nothing; the recharger
    pistol's parts occur in no clip.
  - **Attack animations.** The thrown weapons (throwing knife, hatchet, spear) have no
    moving parts, but their DNAM "Attack Animation" is AttackThrow6, 7 or 8, and the only
    files of those names are `1gtattackthrow6.kf`, `…7.kf`, `…8.kf` (and their `sneak1gt`
    copies) (Thrown → `1gt`). `1gtaim.kf` also animates `##Throwingkinfe` (sic), the
    throwing knife's node.

  So the energy types have no prefix of their own and share the ballistic ones, and
  Thrown shares Grenade Throw's. Land Mine (`1lm`) and Mine Drop (`1md`) are matched by
  name only (no NPC carries one).
- Placed NPCs in the ten masters (3,942 resolved): 3,139 have a weapon (826 by fixed
  data, 2,313 through a leveled roll), 803 none. By type: Pistol - Ballistic 690, Rifle -
  Ballistic 559, Melee (1 Hand) 546, Rifle - Automatic 536, Melee (2 Hand) 471, Thrown 123,
  Pistol - Energy 89, Rifle - Energy 53, Hand to Hand 26, Handle 20, Launcher 20, Grenade
  Throw 6. Most common: the service rifle (262), sledgehammer (212), combat knife (202).
  Every chosen weapon's model exists [observed].

### 9.4 Engine

- **Priority blending** (`nvf_assets::anim_play::BlendPlayer`, up to four clips): per
  node, the clips that drive it are taken from the highest priority down; a priority
  group takes its summed weight (at most what is left of 1), shared by weight within the
  group; whatever is left goes to the next lower group, and finally to the bind pose
  [hypothesis: the blending rule of the game's animation system; nif.xml documents the
  per-track "Priority" field]. With only locomotion clips (weights summing to 1) this is
  the plain weighted blend of part G. A node a clip stops driving goes back to its bind
  transform.
- **Draw cycle** (`fnv_actor::weapon::WeaponAnim`): holstered (holster clip, held at its
  end) → equipping (equip clip) → drawn (aim clip, looping) → unequipping (unequip clip)
  → holstered. Each change cross-fades over the incoming clip's `Blend: N` time (§8.3),
  else 0.2 s; equip and unequip play to their end before the next change. The weapon
  clips are layered over the locomotion clips: with the priorities of §9.1 the equip
  clip takes the upper body (55) and, standing, the legs (25–27 > idle 10), while a
  walk's legs (30) win.
- **Weapon model**: drawn as a rigid part under the actor's `Weapon` node, which is moved
  to the bone of §9.2 when it changes. The `##` part tracks are not played yet (the parts
  stay at their model pose).
- **When the body plays a clip of its own** (on furniture, getting on and off it, and a
  death pose), only the `Weapon` track of the weapon clips is laid over it
  (`WeaponHold::weapon_layers`), so the weapon keeps its place under the bone it hangs
  from and follows that bone. The seat clips have no `Weapon` track [observed:
  `dynamicidle_chairsit.kf`, `dynamicidle_sitstool.kf`, `chair_forwardenter.kf`]: before,
  the node went back to its bind transform (its place under the right hand) while hung
  from the pelvis or spine, and a seated settler's holstered pistol was off his hip at a
  wrong angle [observed in NVForever, owner's playtest 2026-10-11; fixed: the pistol is
  at the hip on the stool 00106192].
- **Flags**: `--actor-weapons none|holstered|drawn` (default `holstered`) and the test aid
  `--actor-weapon-test <s>` (draw, hold `<s>` seconds, holster, hold, repeat; each actor
  starts at its spread phase). With `--actor-pose-time t` the cycle is run from its start
  to `t` and held, like the walk test (§8.4).

### 9.5 CLI contract

`fnvtool actor weapons <Data> <plugins…> --schema <dir> [--nif-schema <dir>] [--folder
<vfs folder>]` → `fnv.actor.weapons/1`. Top level: `schema`, `folder`, `stances`,
`summary`, `actors`, `archives`, `load_order`.

- `stances[]`, by prefix: {`prefix`, `anim_types` [names], `holster_bone`, `drawn_bone`
  (§9.2), `clips` {`holster`|`equip`|`aim`|`unequip`: {`path`, `name`, `start_time`,
  `stop_time`, `cycle_type`, `blend_time`, `prn` [[phase, bone]], `text_keys` [[time,
  text]], `transform_priorities` {priority: tracks}, `weapon_track_priority`} or {`path`,
  `error`}}}.
- `summary`: `weap_records`, `drawable`, `actors` (counts: `no weapon`, `weapon
  (deterministic)`, `weapon (random)`, `unresolved`), `by_anim_type`, `top_weapons`
  [[edid, actors] × ≤ 25], `missing_models` {path: actors}.
- `actors[]`, by reference FormID, those with a weapon: {`reference`, `edid`, `weapon`,
  `weapon_edid`, `anim_type`, `prefix`, `model`, `random`}.

Library: `fnv_actor::weapon` (`AnimType`, `choose_weapon`, `EquipClip`, `clip_path`,
`prn_bone`, `ClipFacts`, `mount_bones`, `Stage`, `WeaponAnim`); `Appearance::weapon`
(not serialised, so `fnv.actor.appearance/1` is unchanged).

### 9.6 Visual check

Screenshots kept privately (`C:\fnv-private\references\partg\weapons\`), Goodsprings, held with
`--actor-pose-time` [observed]:

- Easy Pete (.357 revolver, `1hp`): holstered, the revolver hangs at his right hip,
  grip up, barrel down and back; drawn, it is in his right hand pointing forward at the
  ready; at 0.10 s into the draw his hand reaches the hip, at 0.25 s the revolver is in
  the hand coming up (the `prn: Bip01 R Hand` key is at 0.13 s).
- A Goodsprings settler (varmint rifle, `2hr`): holstered, the rifle is slung diagonally
  across his back; drawn, he holds it low at the ready in both hands; walking with it
  drawn (`--actor-walk-test 85 --actor-weapons drawn`), his legs walk while his arms keep
  the rifle at the ready (walk legs at priority 30 over the aim clip's 25, aim arms at 35
  over the walk's 30).
- This matches how the game shows NPCs' weapons as far as remembered [hypothesis]; an
  in-game screenshot of the same NPCs would settle the exact holster placement.

### 9.7 Performance

Owner's machine, 2026-10-09, release build [observed].

- Per pose (`blend_bench`, which binds every track name, weapon parts included, so it
  overstates the engine a little; two runs): idle alone 19.8–20.5 µs; idle with a
  holstered rifle (`2hrholster.kf`) 21.5–23.9 µs; walk with a drawn rifle (`2hraim.kf`)
  23.0–26.1 µs; a walk → run fade with an equip → aim fade (four clips, 265 tracks)
  53.3–55.6 µs. The two-clip locomotion figures of §8.6 are unchanged by the priority
  rule (idle → walk 26.8–27.7 µs, walk → run 23.4–24.5 µs).
- In the engine (`--cell Goodsprings --auto-fly 4000 --run-seconds 40`; exit 0 and "run
  finished" in all three; 26 of 34 actors armed, no weapon model or stance missing):

| Run | µs per actor and frame | Clips per actor and frame | Tracks per actor and frame | fps |
|---|---|---|---|---|
| `--actor-weapons none` | 45.1 | 1.00 | 60.0 | 25.8 |
| default (holstered) | 46.9 | 1.79 | 62.3 | 23.8 |
| `--actor-weapon-test 1.5 --actor-walk-test 120` | 63.2 | 2.13 | 107.9 | 24.7 |

  Holstered weapons cost about 2 µs per actor and frame (the holster clips have 2–4
  tracks); drawing, holding and walking together about 18 µs more. Up to 17 actors were
  posed at once, 0.21–0.28 ms per frame.

## 10. Creatures (part D)

Placed creatures (ACRE) are drawn like NPCs: one skeleton per creature, its body models
skinned to it, the skeleton's idle playing. Plan and status: docs/design/static-actors.md
part D.

### 10.1 Records and resolution (`Resolver::resolve_creature`)

- **CREA** [documented: xEdit wbDefinitionsFNV.pas CREA; layouts in
  `schemas/fnv/gameplay.json`]: `MODL` the skeleton (relative to `meshes\`), `NIFZ`
  "Model List" (model file names, NUL-separated, animation.md §1.3), `KFFZ`
  "Animations" (`.kf` file names), `BNAM` "Base Scale" (f32), `ACBS` (as NPC_: template
  flags u16 at byte 22), `TPLT` template, `DATA` type byte (0 Animal, 1 Mutated Animal, 2
  Mutated Insect, 3 Abomination, 4 Super Mutant, 5 Feral Ghoul, 6 Robot, 7 Giant).
  xEdit hides `MODL`, `NIFZ`, `NIFT` and `KFFZ` when the Model/Animation template flag
  (bit 6) is set and `BNAM` when the Stats flag (bit 1) is set [documented].
- **LVLC** has the LVLN layout [documented: xEdit]; it is picked like an LVLN (§2.1:
  entries with level ≤ 1, pool by "Calculate from all levels", uniform pick seeded by the
  reference FormID, each list once per creature, chance none not applied). A pool with
  more than one distinct entry makes the creature `random`.
- **Base**: the ACRE's `NAME` (a CREA, or an LVLC resolved as above; anything else is an
  error: an NPC_ or LVLN base is "not a creature base").
- **Templates**: for Model/Animation (bit 6) and Stats (bit 1), the data comes from the
  first CREA along the `TPLT` chain whose flag for that group is clear, starting at the
  base; a `TPLT` naming an LVLC is resolved as above. A flag without a `TPLT` ends the
  chain at that creature.
- **Skeleton**: the Model/Animation source's `MODL` as an archive path (§2 path rule);
  absent or an editor placeholder (file name starting with `marker`) is an error ("names
  no skeleton").
- **Parts**: each `NIFZ` name of the Model/Animation source, in record order, trimmed,
  `/` → `\`, ASCII lower case, leading `\` removed, joined to the skeleton's folder:
  `<skeleton folder>\<name>` [observed: animation.md §4]. Role `body`, `form` the
  Model/Animation source. Empty names are skipped.
- **Anims**: the source's `KFFZ` names, trimmed, `/` → `\`, ASCII lower case, empty ones
  skipped (not played yet; Layer 6).
- **Base scale**: the Stats source's `BNAM`, 1 when absent. The engine multiplies the
  reference's scale by it [hypothesis: values match the creatures' sizes in the game by
  eye — bark scorpions 0.3–0.4 of the radscorpion model, young bighorners 0.8–1.5,
  geckos 0.55–1.0; `nvvoid` placeholders 0.1].

Observed on the ten masters (`fnvtool actor creatures --all --check-meshes`,
2026-10-09): 3,739 placed ACRE, every base a CREA (none an LVLC; LVLC appear only as
templates), all resolved: 2,346 deterministic, 1,393 random. 46 skeletons. NIFZ models
per creature: 1 (3,026), 2 (273), 3–10 (440). Base scale 1 for 1,738; 0.1 for 440 (431
`nvvoid`); 0.15–1.5 otherwise. DATA types: Mutated Animal 845, Mutated Insect 804,
Animal 688, Abomination 605, Robot 460, Feral Ghoul 244, Super Mutant 93.

### 10.2 Meshes and assembly

Over the 230 (skeleton, model) pairs placed creatures use [observed]: 138 models are
skinned, 81 rigid (no skin; robots' screens, saws and tracks, Mr. House's pod parts, dog
skull caps), 8 have no shapes (`nvvoid.nif` and similar placeholders), 3 files are
missing (`ghoul\mmm_ghoulvariant_v1.nif` for 26 creatures,
`nvpenthouemaincomputer\nvpenthousemaincomputer_housescreen.nif` for 2 and
`radscorpion\nvdlc03superlaserstingenergize.nif` for 51). Every skin bone of every
skinned model is a node of its skeleton.

Where the unskinned shapes hang [observed 2026-10-10, FalloutNV.esm, the four add-on
masters and GunRunnersArsenal.esm: 266 unskinned shapes in the 81 rigid pairs;
this replaces an earlier statement here that every such shape has an ancestor named
like a skeleton node, which counted the part's root]:
- All 266 are in a part whose root names a `Prn` bone that the skeleton has. None has
  a node between the root and the shape that is named like a skeleton node, so the
  nearest-ancestor rule below is used by none of them, and none falls to the root.
- The part's root is the identity for all but the parts below. 57 shapes have a root
  that is itself named like a skeleton node (the Securitron's `Screen01Root`, the
  Protectron's `DomeRoot`, the dog's `EyesBlue` and `EyesOneBlue`); 209 do not (the
  dog's `EyesVicious` and `EyesSetYellow` among them), so the name is not what
  attaches a part.
- Parts on `Bip01 Head` whose root is not the identity: the four dog eye models
  (`eyessetblue.nif`, `eyessetoneblue.nif`, `eyessetvicious.nif` and
  `eyessetyellow.nif` in `creatures\dog`; root a half turn, rows (0.40, 0.92, 0),
  (0.92, −0.40, 0), (0, 0, −1), no translation) and the Protectron's
  `blowawaydome.nif`, `blowawaydomebos.nif` and `cowboyhat.nif`. Parts on `Bip01 Head`
  with an identity root: the cyberdog skull caps (`cmskullcap.nif`, `cpskullcap.nif`
  and `rex_skullcap.nif` in `creatures\dog`). Off the head, only
  `nvdlc01_ghostbreathe.nif` (`Bip01 Neck1`), `fxmrhandyjetaddon.nif` (`Bip01 Pelvis`)
  and `nvdlc03needlesaddon.nif` (`Bip01`) have a root that is not the identity.
- **These head parts are ordinary children of the bone: bone ∘ root ∘ shape** (not in
  the skeleton's axes as NPC head parts are, §1.2) [observed, bind pose, positions in
  the skeleton's space, where the dog faces +y and x = 0 is its plane of symmetry]:
  - Dog eyes: as a child, the two eyes are centred at (±3.46, 52.96, 59.53); the skin's
    vertices weighted to `Bip01 Head2 Eyeleds` (the eyelids) are centred at
    (±3.75, 54.00, 59.95), 1.2 units in front of them. In the skeleton's axes at the
    bone (the NPC rule) both eyes are at x = 6.94, one at z = 53.2 and one at z = 60.1:
    on one side of the head, one above the other. With the root dropped they are 7.4
    units from the eyelids.
  - Skull caps (identity root): as a child the cap spans x −7.0 to 7.0 on top of the
    skull (z 63 to 72); in the skeleton's axes it is centred at x = 13, beside the head.
  - Protectron: the skeleton has a node `DomeRoot` under `Bip01 Head` with the same
    local rotation as the root of `blowawaydome.nif` and no translation, so the part's
    root is that node's transform relative to the bone. As a child the dome spans
    x −6.4 to 6.4 and the hat is centred on the head bone's x; in the skeleton's axes
    they are centred 6 and 10 units to one side.
  - The dog skeleton's `EyesBlue`, `Eyes` and `EyesOneBlue` nodes (children of
    `Bip01 Head`) store the inverse of the head's bind rotation instead, which is not
    the eye models' root rotation; hanging the eyes from them gives the one-sided
    placement too. The eye models' root rotation is the local rotation of the
    skeleton's `Bip01 Head2 Eyeleds` [hypothesis: the eyes were modelled in the eyelid
    bone's axes; those three nodes are leftovers of the export].
- Dogs without an eye model: coyotes (`coyote.nif`, 75 placed) and Legion mongrels
  (`nv_mongrel.nif`, 57) are one skinned model each, eyes included. Placed creatures
  with one of the head parts above: 135 (75 with dog eyes, 28 with a skull cap, 60 with
  a Protectron dome, 1 with the hat).
- The dog eye shapes use the body's texture (`textures\creatures\dog\dog.dds`, a patch
  at u 0.45–0.54, v 0.92–1.0), no alpha property, and BSShaderFlags 0x82000181
  (Specular, Environment Mapping, Alpha Texture, Remappable Textures, ZBuffer Test
  [documented: nif.xml BSShaderFlags]) with an empty environment slot in the texture
  set; they are drawn as plain textured shapes.

Rules (engine, `nvf_assets::actor`, `PartUse::by_ancestor`):
- Skinned shapes bind to the skeleton's bones by name, as NPC parts (§3).
- An unskinned shape hangs from the skeleton node named like its **nearest ancestor**
  in the part file that the skeleton has, keeping the transforms of the nodes between
  (its own included) [hypothesis: the part file's copy of the bone has the skeleton's
  bind transform, so the shape lands where the file puts it]; else from the part's `Prn`
  bone as an ordinary child, bone ∘ root ∘ shape, on every bone including `Bip01 Head`
  [observed above; until 2026-10-10 the NPC head rule of §1.2 was applied here, which
  put dog eyes, skull caps and the Protectron's dome and hat beside the head]; else from
  the skeleton's root with the whole transform below the part's root.
- Hidden nodes, editor markers and dismemberment caps are left out as for NPCs.
- Textures are the models' own (NIFT texture hashes are not read).

### 10.3 Idle

Every one of the 46 skeleton folders has `mtidle.kf` directly in it (41 of the 42
creature folders with a skeleton in `Fallout - Meshes.bsa`; `zaxeye` has none), and
walks, where present, in `locomotion\` (`nvbighorner\locomotion\mtforward.kf`) [observed].
The clip lookup (§8.3, `clip_candidates`) therefore ends with `<folder>\<prefix><move>.kf`;
creatures use the male body's lookup. Each creature plays its idle at the spread phase
of §7.3 with the accumulation-root rule of §7.2; other moves wait for Layer 6.

### 10.4 CLI contract

`fnvtool actor creatures <Data> <plugins…> --schema <dir> (--ref <formid> | --cell
<edid|formid> | --all) [--deterministic] [--check-meshes [--nif-schema <dir>]]` →
`fnv.actor.creatures/1`. The creatures are the given reference, the ACRE of the cell, or
every ACRE of the load order (a record of type ACRE inside a cell-children group; winning
definition), skipping deleted winners. With `--deterministic`, creatures whose `random`
is true are left out.
Top level: `schema`, `summary` {`outcomes` (`resolved deterministic`, `resolved random`,
`deleted`, errors by kind), `base_kinds` (base record type per creature)}, `issues`,
`creatures`, `load_order`, and with `--check-meshes` `meshes` {`pairs`, `skeletons`,
`part_kinds`, `idle_clips` (per skeleton, the idle found relative to its folder),
`rigid_at_root` [{`skeleton`, `model`, `shapes`, `of`, `creatures`}], `problems`
[{`skeleton`, `model`, `creatures`, `problems`}]}.
Each creature, ordered by reference FormID: {`reference`, `base`, `scale` (XSCL, default
1, f32 widened), `creature` (the CREA after leveled resolution), `edid` (its EDID),
`kind`? (DATA type name of the Model/Animation source), `skeleton`, `base_scale` (f32
widened), `parts` [{`role`, `form`, `model`}], `anims` [name], `random`}; or
{`reference`, `base`, `error`}. FormIDs print as `0x%08X`.

### 10.5 Checks against the game and cost

- **Reference frames** (2026-10-09, `tools/capture/frames/creatures_cameras.json`, save
  `nvftest`, noon, clear weather; screenshots kept privately in
  `C:\fnv-private\references\partd\`) against `nvforever` at the recorded poses
  (`--fov 75`, eye height 120) [observed]:
  - Goodsprings bighorner pen: the young bighorners stand on the ground with the game's
    shape, horns and fur colour and about the same size (XSCL 0.7–1.0 × BNAM 1.5); the
    game's had walked together, ours stand at their placements. SSIM 0.39 (160 px
    wide; sky, grass and lighting dominate as for the frames of Layer 3).
  - Coyotes east of Goodsprings: feet on the slope, size and coat match the game's
    coyote by eye; SSIM 0.43.
  - Victor's shack: the game's Victor had moved away; ours stands at his placement with
    body, wheel and screen frame in place (the screen face, an animated texture, shows
    blank). SSIM 0.36.
- **Moving-camera smoke test** (`--cell Goodsprings --auto-fly 4000 --run-seconds 40`,
  after merging main with part H weapons): exit 0, "run finished", 50.0 fps; 115
  creatures drawn and animated, none unresolved or without an idle; 47 NPCs drawn and
  animated (39 armed); 31 actors and creatures spawned hidden; 26 bodies skin-tinted;
  posing 16.3 µs per actor and frame (up to 79 posed at once, 0.41 ms per frame).
  Creature clip sets loaded: `nvraven`, `nvbighorner`, `dog`, `radscorpion`,
  `nvsecuritron`, `blowfly`, `nvvoid`, `nvcazadores`, `nvmantis`, each with its idle
  (1.7–16.7 s) and every idle track bound except two Securitron prop nodes
  (`Screen01`, `##VoiceBox_talk`).
- **Head parts** (2026-10-10, `nvforever` screenshots before and after the `Prn` rule
  of §10.2 changed; no game screenshot was compared) [observed]: Cheyenne (Prospector
  Saloon, `--cell 0x00106185 --pos 156,-590,3520 --heading 0 --pitch -4 --fov 30`) had
  two eyeballs on one side of her head and empty sockets, and now has a pale blue eye
  in each socket; Rex (`--cell 0x0010E03B --pos 290,-1790,545 --heading 221 --pitch -17
  --fov 30`) had his eyes beside his head and his skull cap beside his neck, and now
  has both in place; Primm Slim (`--cell 0x000D7845 --pos 2052,3300,7360 --heading 201
  --pitch -3 --fov 35`) had his hat on edge beside his head and now wears it.
- **Rex's skull cap** (2026-10-10, same Rex pose; `nvforever` screenshots before and
  after, brightened five times to judge the dark room; no game screenshot was compared)
  [observed]: the cap was drawn black, every pixel over it 0, and the brain could not be
  seen. Its `##SkullCap:0` shape has NiAlphaProperty flags 4163 (blend, source factor
  ZERO, destination factor SRC_COLOR [documented: nif.xml AlphaFlags]: the frame behind
  it is multiplied by its colour), a flat blue texture and NiMaterialProperty emissive
  0.19 × 12. The emissive light was added as light, on the linear texture, and the
  result was fogged and tonemapped like a lit surface, which left a factor too dark for
  the room. A multiply-blended shape's colour is now formed as a factor on display
  values and limited to 0..1 (docs/spec-notes/nif.md §6, "Multiply blend"
  [hypothesis]): Rex's cap leaves (0.44, 0.78, 1) of what lies behind it, more where
  the cap is lit. After the change the brain shows through a blue tint (mean frame
  value over the brain (4.1, 2.4, 1.4) of 255, from 0). The rule is not specific to Rex:
  it is the only lit multiply shape of 584 in `Fallout - Meshes.bsa`; the police
  cyberdog's cap comes out as red glass by the same rule and the military one, unlit,
  as yellow glass (not looked at in the engine). The caps' emissive colours are keyed
  (a 4 s pulse) and their environment map is not drawn: both are open.

## 11. Actors not enabled at the start

A placed NPC or creature whose reference is not enabled at the start of a game
(initially disabled, or following a disabled enable parent; world.rs
`WorldData::enabled`) and not deleted is spawned **hidden** when its cell loads: the
whole actor is built as usual (parts, FaceGen, idle) with `Visibility::Hidden` on its
root and a `SpawnedDisabled(reference)` marker (`engine/nvforever/src/actors.rs`). Its
entities carry `RefVisual(reference)` like any actor, so when a script enables it
(`ref_enabled`, or an enable parent's), nvf-play's visual sync shows it
(docs/design/layer5-play.md §4). If the simulation already has the reference enabled
when its cell loads (enabled while the cell was away), a component hook on
`SpawnedDisabled` shows it at once. Without `--play` such actors stay hidden (the test aid `--show-disabled-actors` draws
them, e.g. Joe Cobb in the saloon). Light
sources of such references (LIGH) are spawned the same way under a hidden `RefVisual`
holder; their models stay with nvf-play's `script_draw.rs` [engine rules]. The run
summary's `ActorStats` counts hidden actors (`spawned_hidden`); `StreamStats::
lights_hidden` the lights.

Observed (`--cell Goodsprings --auto-fly 4000`, 40 s, 2026-10-09): 31 actors and
creatures spawned hidden. Not yet checked in a `--play` run that enables one (the
showing goes through the existing `RefVisual` sync).

## 12. Head tracking (NVForever design)

An actor's head turns to look at the player character or at another NPC near it (owner
request 2026-10-10). This is an **NVForever design feature, not a parity target**: how
far, how fast and at whom the original's actors look is not captured, so every number
and rule in §12.2 is a design choice, tagged **[design]**. The facts the feature leans
on are in §12.1 with the usual tags. Pure rules and their tests:
`crates/fnv-actor/src/head_look.rs`; the engine side: `engine/nvforever/src/head_look.rs`.

### 12.1 Facts used

- **The neck of the human skeleton** (`characters\_male\skeleton.nif`): `Bip01 Head` is
  the child of `Bip01 Neck1`, which is the child of `Bip01 Neck`. `Bip01 L Clavicle` and
  `Bip01 R Clavicle` are children of `Bip01 Neck` too, so that bone carries the
  shoulders and arms [observed: nvforever's `head tracking:` log line, 2026-10-10].
  Turning `Bip01 Neck` swings the shoulders and arms with the head [observed: the first
  build turned it; screenshot kept privately].
- **Other skeletons with a node named `Bip01 Head`** [observed: the same log lines, cells
  around Goodsprings]: `creatures\nvbighorner` (head under a `Bip01 Neck` that carries
  the clavicles), `creatures\nvsecuritron` (head under `Bip01 Spin02`, no neck bone),
  `creatures\nvmantis` (head under `Bip01 Neck`, under `Bip01 Thorax01`),
  `creatures\dog` (`Bip01 Neck` → `Bip01 Neck1` → `Bip01 Neck2` → `Bip01 Head`),
  `creatures\centaur\skeletonevolved.nif` (head under `Bip01 Neck3`; `Bip01 Neck2`
  carries other bones). Without one: `creatures\nvraven`, `creatures\radscorpion`.
- **Body part data** (BPTD, docs/spec-notes/combat-data.md §1.5): a part has a flag
  "IK Data - Headtracking", an IK start node (`BPNI`) and a tracking max angle (an f32
  in `BPND`) [documented: xEdit, as schemas/fnv/combat.json]. For the body part data of
  `characters\_male\skeleton.nif` the flagged part is `Head`, its IK start node
  `Bip01 Head` and its angle 60; the dog's body part data has no part with the flag
  [observed: the log line of a `--play` run in GSProspectorSaloonInterior, 2026-10-10].
  That the angle is in degrees and bounds the turn to either side is a **[hypothesis]**;
  what the original does with the IK start node (which bones it turns, and how) is not
  known.
- **What follows the head bone**: face, eyes, ears, hair, beard and hat stay on the head
  as it turns, and the neck joins the body without a gap at a 60° turn [observed:
  screenshots of GSSettlerAM and of Easy Pete with Sunny Smiles next to him, with and
  without `--no-head-tracking`, a close-up of Easy Pete at the limit, and Sunny Smiles
  in the saloon; kept privately]. This agrees with §7.4.
- **The bind pose faces +y**: with no clip playing (`--actor-pose bind`) the head's
  bind rotation taken as "facing the actor's +y, z up" gives a head that turns to the
  camera as expected [observed: screenshot]; this supports the actor axes of §8.2
  [hypothesis there] for the head.
- **Not captured**: the original's notice distance, field of view, turn limits in
  pitch, turn speed, whom an actor prefers to look at, and whether creatures track.
  A capture would be needed before any of §12.2 is called parity.

### 12.2 Rules [design]

All of this section is **[design]**. Angles are measured in the frame of the bone the
turn is made against (§12.3): yaw to either side of straight ahead, pitch up and down.

| What | Value |
|---|---|
| Notice a target within | 350 units (about 5 m) and 100° to either side |
| Keep the target looked at out to | 440 units and 120° to either side |
| Another actor takes over when nearer than | 0.75 × the distance to the one looked at |
| Turn to either side, at most | the body part data's tracking angle (60° for humans, §12.1), 60° without data, never more than 80° |
| Tilt, at most | 30° up, 25° down |
| Target nearly straight behind (beyond 150°) | keeps the side the head was turned to |
| Share of the turn on the neck bones above the head | 0.4, spread evenly; the head bone has the whole turn |
| Easing | critically damped, time constant 0.25 s (nine tenths of the way after 0.5 s), never faster than 240°/s |
| Heads are turned only within | 2500 units of the camera |
| Eye point of an actor | 9 units in front of and 7 above its head bone (the looker measures from 7 above its own) |

- **Who looks**: every drawn, living actor with a head bone to turn (§12.3). Dead actors
  (`locomotion::Dying`, `ragdoll::Corpse`, dead in the simulation) and hidden ones (§11)
  neither look nor are looked at; a head that was turned when its actor dies is let go
  at once (the death pose or the ragdoll owns the bones).
- **At whom**: the player's eyes (`Player::eye`), or the eye point of another living,
  drawn **NPC**. Creatures are looked at by nobody (the request names NPCs and the
  player); creatures with a head bone do look at NPCs and the player.
- **Choice** (`fnv_actor::head_look::choose`): only targets within the notice distance
  and angle count (the keep values for the one already looked at); the player, when it
  counts, before any actor; otherwise the nearest, the one looked at staying until
  another is clearly nearer; equal distances go to the lower reference FormID. Nobody
  to look at: the head eases back onto its animated pose.
- **Limits** apply to where the head faces, not to how far it is turned away from the
  animation: a head the idle already holds 9° to one side turns 69° to face 60° to the
  other.
- **A forced target** (§12.3) is looked at whatever its distance and direction; the
  turn limits and the easing still hold.
- **No stare breaking**: an actor looks at its target for as long as the target counts.
  Glancing away, blinking and eye movement are not built.

### 12.3 Engine

- **Bones** (`head_rig`): the head bone is the IK start node of the body part with the
  head-tracking flag when the actor's body part data is read (`--play`:
  `nvf_play::combat::Combat`, `CombatForms::body_parts_for`); an actor whose data has
  no such part does not track. Without that data (no `--play`) it is the node named
  `Bip01 Head`. A skeleton without the bone is skipped silently (a `debug` line). Up to
  two bones right above the head whose name contains `neck` share the turn, but only
  bones that carry nothing except the next bone of the chain: on the human skeleton
  that is `Bip01 Neck1` and `Bip01 Head`, measured against `Bip01 Neck` (§12.1).
- **Frame**: the turn is measured against the bone above the chain (how that bone is
  turned away from its bind pose), not against the actor's root, so the limits are the
  neck's own whatever the spine does.
- **On top of the animation**: each frame, after `locomotion::drive` has posed the
  skeleton and before transforms are propagated, the animated direction of the head is
  read, the wanted direction (the target's, limited) gives the turn wanted on top of it,
  the turn is eased, and the chain's local rotations are replaced by rotations that put
  the head exactly at "animated direction + eased turn" (`turned_locals`), the neck
  bones at their share. The animation's own head movement and roll stay under the turn;
  a zero turn leaves the animated rotations untouched.
- **Bones no clip poses** keep what was last written to them, so the animated rotation
  of a bone that still holds last frame's written value is the remembered one
  (`animated_rotation`); it is given back when the head is at rest [observed: with
  `--actor-pose bind` the head turns once and holds, it does not wind up].
- **State and API** (components on the actor's root, the entity with `ActorRig`):
  - `HeadLook`: inserted by the module on every drawn actor; `target()` (what the head
    is turned to: `LookAt::Player`, `LookAt::Actor(reference)` or `LookAt::Point`),
    `turn()` and `facing()`.
  - `LookOverride(LookAt)`: inserted by another system to force a target (a
    conversation's speaker looking at the player, a script's `Look`); it comes before
    the proximity choice and ignores distance, direction and the camera radius. Remove
    it to give the head back. When the target cannot be found (no player; the actor is
    dead, hidden or not drawn near the camera) the proximity choice applies.
  - `NoHeadLook`: the actor's head is left to its animation (it eases back first).
- **Without `--play`** there is no player: only actor-to-actor tracking. The free camera
  is not looked at, unless the test aid `--head-track-camera` makes it stand in for
  the player.
- **Pausing**: the easing runs on the virtual clock, so the Esc pause menu and
  `--menu-pause original` hold it; the turn is written every frame all the same (the
  frame's animation is posed under it).
- **Cost**: the choice and the turn run only for actors within 2500 units of the camera
  (or with a forced target); the rest cost a distance test. `--no-head-tracking` adds
  no system.

### 12.4 CLI contract

- `--no-head-tracking`: heads stay on their animation (the module is not added).
- `--head-track-camera` (test aid, not with `--play`): the free camera stands in for the
  player.
- `--head-track-point x,y,z` (test aid): every actor near the camera gets a
  `LookOverride` to that game position, the way another system forces a target.
- Log (`head tracking:`): once per skeleton, the bones turned, their shares, the bone
  they are measured against, the yaw limit and where bone and angle come from; at the
  end of a run `head tracking summary: N of M actors with a head to turn looked at
  something (… at the player, … at another actor, … at a target forced on them), up to
  … at once, … at the end; largest turn on top of the animation …°; … us per frame on
  average`, then one `head tracking at the end:` line for up to twelve actors still
  looking (who, where, at whom, how far the head is turned). `debug` lines name every
  change of target.

### 12.5 Checks (2026-10-10, owner's install)

- Unit tests: `fnv_actor::head_look` (choice, hysteresis, the player first, limits, the
  side kept behind, shares, easing in and out without a snap, determinism, a paused
  clock) and `nvforever::head_look` (the head ends exactly at the animated direction
  plus the turn for a chain with arbitrary bind rotations, the neck turns less than the
  head, the animation's movement stays under the turn, no double turn of an unposed
  bone).
- NPC to NPC: `--npc-travel SunnyRef=EasyPeteRef` (Sunny Smiles stops 56 units from Easy
  Pete): both look at each other, Easy Pete's head at the 60° limit, Sunny's 8° up on
  top of her idle; the travel smoke test of CLAUDE.md ends with Doc Mitchell and Chet
  looking at each other in the store, 58 units apart, and both travellers arrived.
- The player: `--play` in GSProspectorSaloonInterior, the player 130 units in front of
  Sunny Smiles: her head turns 31° to the player. Standing behind her she does not
  notice the player (outside the 100°).
- Forced: `--head-track-point` below the floor of the saloon: all three drawn actors
  look down at the 25° limit.
- Cost: 5.1 µs per frame on average over the moving-camera run (`--cell Goodsprings
  --auto-fly 4000`, 40 s; 151 actors given a head to turn in all, up to 79 posed at
  once, actor posing itself 395 µs per frame); 2 actors looked at another on the way.

### 12.6 Open

- **Conversations** (dialogue step D7, `engine/nvforever/src/dialogue_look.rs`, merged
  2026-10-10): a conversation turns its actor's head itself, and head tracking hands the
  head over so the two never write the same bones. While `dialogue_look::Looks` holds
  the actor (facing the player, or easing back) `track` leaves it alone and forgets its
  own turn; it runs after `turn_speaker`. A head that tracking already had on the
  player starts the conversation fully turned (weight 1) instead of easing in from the
  animation, and `HeadLook::animated` gives the conversation the animation's rotation of
  a bone tracking wrote and no clip posed since [design]. Checked 2026-10-10: Easy Pete
  with the player 53° off his front (`NVF_PLAY_TEST_TALK`, head only): head on the
  player before the talk, still on the player 0.15 s into it and during the line, one
  turn and no kink at the neck (screenshots kept privately). Rough edges: after the
  conversation the head eases back to the animation and then tracking turns it to the
  player again; a head mid-turn when the talk starts jumps to the full turn; the two
  systems' limits and bone chains differ (the conversation also turns `Bip01 Neck`, which
  the clavicles hang from, §12.1). One system driving the head through
  `LookOverride(LookAt::Player)` would remove all three [open].
- Sitting, sleeping and other furniture poses: nothing marks such actors yet;
  `NoHeadLook` is there for it. The frame of §12.3 keeps the limits sensible in such a
  pose, but that is not checked.
- Creatures: only the skeletons of §12.1 were seen; none was checked by eye. Without
  `--play` a creature with a `Bip01 Head` tracks even when its body part data has no
  head-tracking part (the data is not read then).
- Eyes do not move; a target beyond the turn limit is looked at with the head at the
  limit.
- The original's values (§12.1, "Not captured").
