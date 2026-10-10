# Static actors: NPCs drawn as one assembled entity (workstream 5)

Since 2026-10-09 the same assembly also draws the player's own body in `--play`,
animated from the controls with layered clips (docs/design/player-body.md).

Status 2026-10-08: **parts A, B and C done** (C: geometry and face-texture FaceGen;
since 2026-10-09 the body skin `.egt` too, actors.md §4.1; since 2026-10-10 the
race's FGTS is added to the NPC's for the skin tint, its FGGS stays unused, §4.2); part
D (creatures) done 2026-10-09 (§8 below); part E (animation
playback) done: `nvforever` (default `--actor-pose idle`) plays `mtidle.kf` on each actor's skeleton, all 60 transform tracks binding to
`skeleton.nif`, with skinned, rigid and FaceGen parts following (actors.md §7); since
2026-10-09 this is the default (owner decision: the owner asked for working NPCs; set by
the desktop coordinator, reversible), and `--actor-pose bind` keeps the frozen bind pose.
Part G (locomotion playback) done 2026-10-09: idle, walk, run and turn clips chosen from
each actor's velocity and turn rate, two-clip cross-fades, rate and phase matched to the
clips' root motion; the test aid `--actor-walk-test` moves actors (actors.md §8).
Part H (weapons) done 2026-10-09: armed NPCs show their weapon holstered (at the hip or on
the back) by default and draw and holster it with the game's equip clips, layered over
locomotion by track priority; `--actor-weapons none|holstered|drawn`,
`--actor-weapon-test` (actors.md §9). Part A: schema
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
| Schema | `schemas/fnv`: HAIR, EYES, HDPT records; RACE `INDX` (done); CREA `NIFZ`, `KFFZ`, IDLE (done, part F) | from xEdit's public definitions, as in Layer 1 |
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
| D — creatures (done 2026-10-09) | CREA (`MODL` skeleton + `NIFZ` model list), LVLC and template resolution, ACRE placement, idle; `fnvtool actor creatures`, oracle, `L3.actors.creatures`; actors.md §10; §8 below | B |
| E — animation playback | `--actor-pose idle` (the default since 2026-10-09): a looping KF idle on each actor's skeleton by bone name (anim_play), accumulation root held at the identity, no per-frame allocation; findings in actors.md §7. Choosing clips by situation stays Layer 6 | B |
| F — animation data (headless) | IDLE schema and reader (`fnv_actor::idle`), `.kf` catalogue by skeleton folder (`fnvtool actor anims`), spec note docs/spec-notes/animation.md; groundwork for Layer 6 animation selection | A |
| H — weapons: holster and draw | the weapon an NPC carries drawn as a model on the skeleton's `Weapon` node, holstered by default; the stance's holster, equip, aim and unequip clips layered over locomotion by track priority; `prn:` keys move the weapon between holster bone and hand; test aid `--actor-weapon-test`; `fnvtool actor weapons`; actors.md §9; plan in §7 below | G |
| G — locomotion playback | idle, walk, run and turn clips chosen from velocity and turn rate, cross-faded (two-clip weighted blend), playback rate synced to ground speed from the clips' root motion; test aid `--actor-walk-test`; `fnvtool actor locomotion`; actors.md §8; plan in §6 below | E, F |

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

## 6. Part G — locomotion playback (plan, 2026-10-09)

Goal: actors that move play the game's own locomotion clips with feet that do not slide,
ready for Layer 6 to drive them. Facts and rules: docs/spec-notes/actors.md §8 (survey
§8.1, root motion §8.2, engine rules §8.3).

1. **Survey** (done before code): `fnvtool actor anims` lists the clips;
   `fnvtool actor locomotion` (new, `fnv.actor.locomotion/1`) measures each clip's root
   motion and lists the clip set per body and stance prefix.
2. **Headless** (`crates/fnv-actor/src/locomotion.rs`, unit tests on synthetic
   sequences): moves and clip lookup by skeleton folder, body and prefix
   (`LocomotionSet::find` over the archive listing, no hard-coded file names beyond the
   observed naming rule); `root_motion`; `foot_phase` and `blend_time` from text keys;
   `choose` (velocity and turn rate → move and rate); `Blend` (current and previous
   clip, weights, phases, phase sync, fade timing).
3. **Engine sampling** (`engine/nvf-assets/src/anim_play.rs`): a `BlendPlayer` component
   that poses a skeleton from up to two weighted clips per frame (lerp/nlerp per node,
   bind pose for missing weight), with the accumulation root rule of actors.md §8.2.
   Tracks are bound to skeleton node indices once per (skeleton, clip) and shared;
   scratch buffers are allocated at spawn, so sampling allocates nothing per frame.
   The single-clip `Playing` stays for animated world objects and the viewer.
4. **Engine control** (`engine/nvforever/src/actors.rs`): with `--actor-pose idle` (the
   default) every human actor gets a `Locomotion` component (its clip set, `Blend` state,
   velocity and turn rate) and a `BlendPlayer`; a system feeds `choose` and `Blend` and
   writes the layers. A standing actor plays Idle at its spread phase as in part E, and
   `--actor-pose-time` still holds it. `--actor-pose bind` is unchanged. The hard-coded
   `idle_clip()` goes; the idle comes from the clip set.
5. **Test aid**: `--actor-walk-test <speed>` (and `--actor-walk-test-radius`) moves
   actors along a fixed script (actors.md §8.4) that exercises walk or run, stop, turns
   both ways and backward walking. The script is a pure function of time with unit
   tests.
6. **Checks**: unit tests for clip lookup, root motion, choice, blend weights and phase
   sync on synthetic clips; screenshots of Doc Mitchell walking at several held times
   (kept privately); the moving-camera smoke test and a run with the walk test aid;
   sampling cost per actor and frame against §7.5 (part E).

Out of scope (Layer 6 and part D): AI packages, pathfinding, situation idles from IDLE
conditions, weapon stances (no weapons drawn yet), creatures, and the `nvf-viewer --anim`
accumulation-root follow-up. Notes for them: actors.md §8.8.

Results (2026-10-09): all of 1–6 done. The unarmed set of `characters\_male` has every
move for male, female and child bodies (actors.md §8.1); walk and run clips move `Bip01`
only by translation (forward walk 85.3 units/s, run 353.9), idle and turns not at all
(§8.2). Screenshots show walking, turning, walking backward and running facing the right
way with the feet on the floor (§8.7). Cost: 6–25 µs per one-clip pose and 21–27 µs per
two-clip pose; in the engine 37–41 µs per actor and frame, the same range as part E
(§8.6).

## 7. Part H — weapons: holster and draw (plan, 2026-10-09)

Owner request 2026-10-09: start on weapon equipping animations for NPCs. Facts and rules:
docs/spec-notes/actors.md §9 (survey §9.1–9.3, engine §9.4).

1. **Survey** (done before code): `fnvtool actor anims` lists the stance clips; the new
   `fnvtool actor weapons` (`fnv.actor.weapons/1`) reports each stance's clips (`prn:`
   keys, track priorities, holster and hand bones) and the weapon each placed NPC draws.
2. **Headless** (`crates/fnv-actor`): WEAP in `records` (model, animation type, damage,
   embedded); `Appearance::weapon` from the carried items (not serialised, so
   `L3.actors.appearance` is unaffected); `weapon` module: type → stance prefix, clip
   paths, `prn:` parsing, holster/hand bones, the draw-cycle state machine. Unit tests.
3. **Engine sampling** (`nvf-assets`): `BlendPlayer` takes up to four clips and blends by
   per-track priority (actors.md §9.4); a node no clip drives any more returns to its
   bind transform.
4. **Engine control** (`nvforever`): a `WeaponHold` component per armed actor (its stance
   clips bound to the skeleton, the draw cycle, the holster and hand bones); the weapon
   model spawned under the `Weapon` node and re-parented when the cycle passes a `prn:`
   key; `--actor-weapons none|holstered|drawn`, `--actor-weapon-test <s>`.
5. **Checks**: unit tests (stance mapping, weapon choice, `prn:` parsing, draw cycle,
   priority blending); screenshots of armed NPCs holstered, drawing and drawn (kept
   privately); the moving-camera smoke test with weapons and with the test aid; cost per
   actor and frame against part G (§8.6).

Out of scope: when to draw (Layer 6: combat, packages, the player's presence), attacks,
reloads and firing, the `##` weapon-part tracks, stance-specific walk clips
(`1hpforward.kf`, …), power-armour and sneak variants, creatures' weapons.

Results (2026-10-09): 1–5 done. 3,139 of 3,942 placed NPCs carry a drawable weapon
(actors.md §9.3); every stance's clips load and give a holster and a hand bone (§9.1).
Screenshots show a revolver at the hip and a rifle on the back, both drawn into the hands,
and a rifle held at the ready while walking (§9.6). Holstered weapons cost about 2 µs per
actor and frame, drawing while walking about 18 µs more (§9.7). Open for the owner: the
exact holster placement against in-game screenshots, and the default (`holstered`; set
`--actor-weapons none` for the part G look).

## 8. Part D — creatures (2026-10-09)

Facts and rules: docs/spec-notes/actors.md §10 (records, resolution, meshes, idle, CLI).

- **Headless**: `fnv_actor::records::Creature` (CREA) and LVLC lists;
  `Resolver::resolve_creature` → `CreatureAppearance` (creature after LVLC and template
  resolution, skeleton, NIFZ parts beside it, base scale, KFFZ names, `random`).
- **CLI and oracle**: `fnvtool actor creatures` (`fnv.actor.creatures/1`, with
  `--check-meshes`); `tools/capture/oracle_actor.py creatures`, written from the spec
  note; `L3.actors.creatures` compares the 2,346 deterministic placed creatures of the
  ten masters (all equal).
- **Engine**: `actors::spawn_creature` assembles each placed creature with
  `nvf_assets::actor::spawn_actor` (`PartUse::by_ancestor`: unskinned shapes hang from
  their nearest ancestor that the skeleton has, else from their `Prn` bone as ordinary
  children, actors.md §10.2), scales it by XSCL × BNAM, and plays the
  skeleton's `mtidle.kf` through the locomotion player (male body lookup, spread phase).
  `world.rs` calls it for CREA bases next to the NPC call.
- **Also in this round**: actors and creatures that are not enabled at the start are
  spawned hidden so a script's Enable shows them (actors.md §11; owner-review item 19);
  light sources of such lights likewise; "Random Anim Start" spreads placed models'
  idle phases (animation.md §8); `nvf-viewer --anim` plays walks in place (actors.md
  §8.8); the body skin `.egt` tints the body texture (actors.md §4.1).
- **Not done**: creature walk/turn/attack clips and KFFZ special idles (Layer 6); NIFT
  texture variants; the race's FaceGen geometry values (§4.2 of actors.md; its texture
  values are added since 2026-10-10). The hand `.egt` followed on
  2026-10-10 (actors.md §4.1).

Results (2026-10-09): `L3.actors.creatures` 2,346 of 2,346 equal; every skin bone of the
230 (skeleton, model) pairs is in its skeleton and every rigid shape finds a bone
(actors.md §10.2); in-game reference frames of the Goodsprings bighorners, coyotes and
Victor match by eye in place, size and look (SSIM 0.36–0.43, sky and grass dominated);
the moving-camera smoke test draws and animates 115 creatures with no idle missing, at
50 fps (actors.md §10.5).
