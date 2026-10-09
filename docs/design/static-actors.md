# Static actors: NPCs drawn as one assembled entity (workstream 5)

Status 2026-10-08: **parts A, B and C done** (C: geometry and face-texture FaceGen;
the body's skin `.egt` and the race FaceGen data are not used yet); part D (creatures)
not started; part E (animation playback) done as an opt-in: `nvforever --actor-pose idle`
plays `mtidle.kf` on each actor's skeleton, all 60 transform tracks binding to
`skeleton.nif`, with skinned, rigid and FaceGen parts following (actors.md §7); the
default stays the frozen bind pose. Part A: schema
layouts, `fnv-actor` resolution, `fnvtool actor forms | appearance | meshes`, the
independent oracle `tools/capture/oracle_actor.py` and `L3.actors.appearance` (all 1,149
deterministic actors equal). Part B: `nvf_assets::actor` assembles each placed NPC on
one skeleton in the game (`nvforever`, `--no-actors` turns it off); facts and rules in
docs/spec-notes/actors.md. Owner request 2026-10-08: render placed NPCs (no AI, no
movement) with the parts of a humanoid — skeleton, body, hands, head, eyes, mouth, hair,
beard and worn armour — put together as **a single entity** per actor. Creatures follow
as a later part (§4, part D).

Owner decisions 2026-10-08 (answers to §5): a **frozen bind pose** (no idle animation);
**FaceGen is in scope** (part C before the workstream is done); an NPC with two outfits
wears the one with the **best protection by DT and DR**; reference screenshots of a
Goodsprings Settler and Sunny Smiles were provided (kept privately, not committed).

Where it sits: drawing actors is Layer 3 scope (layer3.md §1 lists them as markers only),
but choosing what an NPC wears needs Layer 4 data (`fnv-sim` inventories, leveled lists,
equip). It therefore runs as workstream 5 of the Layer 4 table (layer4.md §7) and changes
Layer 3's renderer only by additions. AI, dialogue, combat and per-frame animation
selection stay Layer 6.

## 1. What the data shows

Checked on the owner's install with `fnvtool esm decode` and `fnvtool nif decode`
(FalloutNV.esm, `Fallout - Meshes.bsa`) [observed 2026-10-08] unless tagged otherwise.

### 1.1 Records

- **NPC_** (e.g. DocMitchell 0x00104C0C): `MODL` names the skeleton
  (`Characters\_Male\Skeleton.NIF`), `RNAM` the race, `HNAM` a HAIR, `LNAM` hair length
  (0.286), `HCLR` hair colour (RGB), `ENAM` an EYES, repeated `PNAM` head parts (HDPT; Doc
  has two), `FGGS`/`FGGA`/`FGTS` FaceGen coefficients, `CNTO` carried items (his outfit is
  among them), `TPLT` a template. ACBS flags bit 0 is Female; bit 8 "Use Template" and the
  template flags (bit 6 "Model/Animation", bit 8 "Inventory") say which parts come from the
  template [documented: xEdit wbDefinitionsFNV.pas, already in `schemas/fnv`]. Doc has a
  `TPLT` but ACBS flags 0, so the template is not used for him.
- **RACE** (CaucasianOld 0x000042C3): `NAM0` opens the head parts and `NAM1` the body
  parts, each with an `MNAM` (male) and an `FNAM` (female) list of `INDX` + `MODL`/`ICON`
  entries. Head indices 0–7 carry the head, ears (texture only), mouth, lower teeth, upper
  teeth, tongue, left eye and right eye (`Characters\Head\HeadHuman.NIF`, `…MouthHuman.NIF`,
  `…EyeLeftHuman.NIF`, …; meanings read from the file names). Body indices 0–2 carry
  `characters\_Male\UpperBody.nif`, `LeftHand.nif`, `RightHand.nif`; index 3 is an `.egt`
  (`UpperBodyHumanMale.egt`). The head's `ICON` is the age-specific skin texture
  (`Characters\Male\HeadHumanOld.dds`). Race `HNAM`/`ENAM` list allowed hairs and eyes;
  `DATA` holds male/female height and weight. `INDX` is not in the schema yet.
- **HAIR** (HairBalding): `MODL` `Characters\Hair\HairBase.NIF`, `ICON`, 1-byte `DATA`.
  **EYES** (EyeBlue): `ICON` only (a texture for the race's eye meshes), 1-byte `DATA`.
  **HDPT** (BeardMustacheCurly): `MODL` `Characters\Hair\BeardMustacheCurly.nif`, 1-byte
  `DATA`. None of the three are in the schema yet.
- **ARMO** (OutfitRepublican02 0x000EF1CC): `BMDT` biped flags (Upper Body), `MODL` male
  worn model, `MOD2` ground model (`…_GO.NIF`), `MOD3` female worn model; `MOD4` is the
  female ground model [documented: xEdit]. These decode today.

### 1.2 Meshes: two ways a part joins the skeleton

- **Skinned parts** (upper body, hands, head, worn armour): the shapes carry a
  `BSDismemberSkinInstance` whose bones are NiNodes *copied into the part's own file*
  (`lefthand.nif` has 18 nodes, `upperbody.nif` 27). HeadHuman binds to Bip01 Spine2, L/R
  Clavicle, Neck1 and Head. To join the actor, each skin bone is rebound **by name** to
  the skeleton's node — the rule the viewer already uses for creatures
  (`engine/nvf-assets/src/nif_spawn.rs`, docs/spec-notes/nif.md §6).
- **Rigid parts** (hair, beards, eyes, presumably hats and glasses): unskinned shapes whose
  root has a `NiStringExtraData` named `Prn` naming the parent bone (`Bip01 Head` for
  hair, beard and eye). They are parented to that bone of the skeleton.
- **Variants and caps inside a part**: `hairbase.nif` holds two shapes, `NoHat` and `Hat`;
  `upperbody.nif` and `republican_02.nif` hold dismemberment caps (`meatneck01`,
  `meathead01`, `bodycaps`, `limbcaps`, `MeatCap*`). An intact actor shows `NoHat` without
  headgear, and no caps [hypothesis; to confirm against in-game screenshots].
- **FaceGen**: next to each head and many hair, beard and hat meshes are `.egm` (geometry
  morphs; 275 in `Fallout - Meshes.bsa`), `.egt` (texture morphs; 36) and `.tri` (49)
  files. There are no pre-generated per-NPC heads in the mesh archive (no `facegen` folder
  besides `hairdefaultfacegen*` and `headfemalefacegen.nif`). Faces are presumably built
  at run time from the race's base head plus the NPC's FGGS/FGGA/FGTS coefficients
  [hypothesis].
- **Pose**: the skeleton's bind pose, frozen (owner decision). The human idle would be
  `meshes\characters\_male\locomotion\mtidle.kf` (the folder also holds 2,024 `.kf`
  files, mostly `idleanims\`).

## 2. Design

### 2.1 One entity per actor

```
ActorRoot (RefVisual(form_id), Transform = ACHR placement × scale)
└── skeleton hierarchy (spawned once from NPC_.MODL)
    ├── Bip01 … Bip01 Head ── rigid parts (hair, beard, eyes) via `Prn`
    └── …
skinned part meshes (body, hands, head, armour): children of ActorRoot,
    SkinnedMesh joints = the skeleton's bone entities, matched by name
```

- The part files' own bone copies are **not** spawned; only their shapes are, bound to the
  one skeleton. A single animation player on the skeleton then moves every part.
- The root carries `RefVisual(form_id)` like any reference, so Part A's hide/show
  (layer4-play.md §3) and cell unloading work unchanged; despawning the root removes the
  whole actor.
- Meshes and materials of a part are cached per model path like static models
  (`nvf-assets` model cache); per-actor state is only the entity tree and texture
  overrides (head skin texture, hair colour, eye texture).
- Scale: ACHR `XSCL` × race height (male/female) [hypothesis: the race height scales the
  actor; to confirm with an in-game height measurement].

### 2.2 Appearance resolution (headless)

A pure function turns a placed actor into a list of parts, with no Bevy types, so it can be
tested and compared against an independent oracle:

```
resolve(ACHR) → ActorAppearance {
  base: NPC_ form (after LVLN resolution and template inheritance),
  female, race, skeleton path, scale,
  parts: [ { path, kind: Skinned | Rigid { bone }, source: Race(INDX) | Hair | Eyes |
             HeadPart | Armor(form, slot), variant: NoHat | Hat | none } ],
  textures: { head_skin, eyes, hair_tint },
  facegen: { fggs[50], fgga[30], fgts[50] },   // used from part C
  issues: [...]                                 // nothing silently dropped
}
```

Rules, all [hypothesis] until captured:
1. **Base**: an ACHR whose base is an LVLN resolves through `fnv-sim`'s leveled lists with
   a per-reference seed (deterministic, not the game's roll). Templates: follow `TPLT`
   only when ACBS "Use Template" is set, and only for the template-flag groups set
   ("Model/Animation" → race, skeleton, hair, eyes, head parts, FaceGen; "Inventory" →
   carried items).
2. **Body**: race body parts for the sex (indices 0–2), head parts (0, 2–7), eye texture
   from EYES, hair from HAIR with `HCLR` tint, each HDPT model.
3. **Worn items**: from the resolved inventory, best protection first (DT, then DR —
   owner statement), each worn unless a better one shares a slot. Its worn model
   (MODL/MOD3 by sex) replaces the race part whose slot it covers (Upper Body → upper
   body; Left/Right Hand → hands; Head → head, head parts and hair). Hat → hair's `Hat`
   shape; Hair without Hat → hair hidden (wigs). Details: actors.md §2.
4. **Weapons** are out of scope for this workstream (no holsters).

### 2.3 Files

| Item | Where | Notes |
|---|---|---|
| Spec note | `docs/spec-notes/actors.md` | all facts above, tagged; capture procedures |
| Schema | `schemas/fnv`: HAIR, EYES, HDPT records; RACE `INDX` (done); CREA `NIFZ` (part D) | from xEdit's public definitions, as in Layer 1 |
| Resolution | `crates/fnv-actor` (new; depends on `fnv-esm`, `fnv-world`, `fnv-sim`) | `resolve`, `ActorAppearance`; unit + synthetic-plugin tests |
| CLI | `fnvtool actor forms \| appearance \| meshes` | contract in actors.md §5; one pointer paragraph in `docs/cli-contract.md` |
| Oracle | `tools/capture/oracle_actor.py` (+ test) | written from the spec note only |
| Scenario | `parity-lab/scenarios/layer3_actors.json`: `L3.actors.appearance` | keyed on reference FormID vs the oracle |
| Assembly | `engine/nvf-assets`: spawn a skeleton once and attach parts to it (skinned by name, rigid by `Prn`) | additive API next to `spawn_nif`; the viewer gets `--npc <edid>` to inspect one actor |
| World | `engine/nvforever/src/actors.rs`: spawn actors during cell streaming | one registration line in `world.rs` |

## 3. Validation and exit criterion

1. `L3.actors.appearance` passes for every placed NPC in the ten masters (parts list vs the
   oracle); every unresolved actor is explained in `issues`.
2. Mesh rules: for every part file referenced by a resolved actor, every skin bone name
   exists in the actor's skeleton and every `Prn` names a skeleton node (count failures;
   expect zero or explain each).
3. Viewer: `nvf-viewer --npc DocMitchell` shows one actor in the idle pose; screenshot.
4. World: `nvforever --data <Data> --cell GSDocMitchellHouse` and `--cell Goodsprings`
   draw the NPCs placed there (Doc Mitchell first) with body, head, hair and outfit; the
   moving-camera smoke test (CLAUDE.md) still exits 0 with actors on; frame time reported
   with and without actors.
5. Owner visual check against in-game screenshots of the same NPCs (§5).

Done when 1–5 hold for humanoids with FaceGen faces (parts A, B and C).

## 4. Parts

| Part | Delivers | Depends on |
|---|---|---|
| A — data | schema additions, `fnv-actor` resolution, CLI, oracle, `L3.actors.appearance`, spec note | `fnv-sim` (leveled lists, inventories) |
| B — assembly | one-skeleton spawning in `nvf-assets`, viewer `--npc`, world streaming of actors, frozen bind pose | A's `ActorAppearance` (stub first) |
| C — FaceGen | spec note for `.egm`/`.egt`/`.tri` from public documentation, head/hair/hat morphs from FGGS/FGGA, skin texture morph from FGTS | B |
| D — creatures | CREA (`MODL` skeleton + `NIFZ` model list), ACRE placement | B |
| E — animation playback | opt-in `--actor-pose idle`: a looping KF idle on each actor's skeleton by bone name (anim_play), accumulation root held at the identity, no per-frame allocation; findings in actors.md §7. Choosing clips by situation stays Layer 6 | B |

A and B can run in parallel once the `ActorAppearance` struct is committed as a stub.

## 5. Open questions for the owner

1. ~~Static pose~~ → Owner 2026-10-08: frozen bind pose.
2. ~~FaceGen~~ → Owner 2026-10-08: do FaceGen (part C).
3. Reference screenshots → Owner 2026-10-08: a Goodsprings Settler and Sunny Smiles
   provided. More would help for the headgear rules (actors.md §6): an NCR trooper with
   helmet, a settler with a cowboy hat, a power armour helmet.
4. ~~Armour choice~~ → Owner 2026-10-08: the best protection by DT and DR (actors.md §2.3).
</content>
</invoke>
