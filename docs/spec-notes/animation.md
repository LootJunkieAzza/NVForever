# Spec notes: animation data (workstream 5, part F) — IDLE records and `.kf` files

Clean-room description in plain language: headless groundwork for animation *selection*
(what an actor plays when, Layer 6). Nothing here decides what to play; it records what
the data holds. Tags: **[documented]** cites a public source, **[observed]** was measured
on the owner's install (FNV 1.4.0.525, the ten masters in the load order of
docs/spec-notes/simulation.md, the archives `fnv_vfs::Vfs::open_default` loads from
`Data`) with `fnvtool actor anims` (§6) unless another command is named,
**[hypothesis]** needs verification against the original game.

Record layouts come from the Layer 1 schema (`schemas/fnv`), re-expressed from xEdit's
public definitions (`Core/wbDefinitionsFNV.pas`, `Core/wbDefinitionsCommon.pas`, branch
`dev-4.1.6`, MPL-2.0) as documentation only; no xEdit code is used. `.kf` decoding is
`fnv-nif` (docs/spec-notes/nif.md). Code: `crates/fnv-actor/src/idle.rs` (IDLE reader),
`crates/fnv-actor/src/anims.rs` (folders, skeletons, bindings, KFFZ/NIFZ lists),
`crates/fnvtool/src/actor_anims.rs` (the catalogue).

## 1. Records

### 1.1 IDLE

Subrecords in order [documented: xEdit IDLE]: `EDID`; a model (`MODL` file name, optional
`MODB`; `MODT`/`MODS`/`MODD` are allowed by xEdit and absent in the masters [observed]);
any number of `CTDA` conditions; `ANAM` (required); `DATA` (required).

- **ANAM** (8 bytes): parent IDLE, previous IDLE (FormIDs; each may be null).
- **DATA** (6 or 8 bytes): animation group section (u8), loop minimum (u8), loop maximum
  (u8), unused (u8), replay delay (i16); in the 8-byte form also flags (u8; bit 0 "No
  attacking") and an unused byte [documented: xEdit, flags optional from the fifth field].
  Instances: 896 × 8 bytes, 849 × 6 bytes [observed: `fnvtool esm survey`].
- **Animation group section byte**: the low six bits name the section — 0 Idle,
  1 Movement, 2 Left Arm, 3 Left Hand, 4 Weapon, 5 Weapon Up, 6 Weapon Down,
  7 Special Idle, 20 Whole Body, 21 Upper Body [documented: xEdit wbIdleAnam]. xEdit names
  bit 0x40 "Loose Idle" and prints "Must return a file" when bit 0x80 is clear
  [documented]; what the engine does with either bit is not known (§5). The reader keeps
  the byte raw (`Idle::group_raw`) and exposes `section()` and `group_high_bits()`.
- **MODL** is relative to `meshes\` [observed: every name resolves under `meshes\`, §2.2].

Counts [observed]: 1,742 winning IDLE records (FalloutNV 1,594, OldWorldBlues 48,
HonestHearts 45, LonesomeRoad 28, DeadMoney 27; 1,745 instances with overrides); none is
deleted. By section: Special Idle 1,299, Whole Body 178, Movement 125, Weapon 70, Idle 49,
Upper Body 21; no record uses Left Arm, Left Hand, Weapon Up or Weapon Down. High bits:
none 1,203, 0x40 441, 0x80 93, both 5. Flags: absent 849, 0 828, "No attacking" 65. Loop
(min, max): (0, 0) 1,462, (255, 255) 93, other small ranges the rest. Replay delay: 0 in
1,696 records.

### 1.2 Conditions (CTDA)

Layout [documented: xEdit wbConditions, already in `schemas/fnv/common.json`]: type (u8),
3 unused bytes, comparison value (4 bytes), function index (u16), 2 unused bytes,
parameter 1 and 2 (4 bytes each), then optionally run-on (u32) and reference (4 bytes).
Instances on IDLE: 1,807 × 28 bytes, 98 × 20 bytes, 2 × 24 bytes [observed: survey].

- **Type byte** [documented: xEdit wbConditionTypeToStr]: top three bits are the
  comparison — 0 `==`, 1 `!=`, 2 `>`, 3 `>=`, 4 `<`, 5 `<=` (6 and 7 undefined); bit 0
  "Or"; bit 2 "Use Global" (the comparison value is then a GLOB FormID, otherwise an f32
  [documented: wbConditionCompValueDecider]). Bit 1 is named "Run On Target" for
  Oblivion/Fallout 3 and "Use Aliases" for later games in xEdit; which applies to FNV is
  not settled here, so the reader exposes only `or()` and `use_global()`.
- **Function**: index into the condition function table; the script opcode is
  0x1000 + index (docs/spec-notes/script-functions.md §4). The catalogue names functions
  from `schemas/script/functions.json`.
- **Parameters, run-on and reference stay raw** (`Condition::param1_raw` …): their types
  depend on the function, and they are file-local values of the IDLE's defining plugin
  (`Idle::plugin`), so a FormID parameter must be mapped through that plugin's master
  list by whoever evaluates it (Layer 5/6).
- xEdit reads run-on as an animation group, not an actor, for `IsFacingUp` and `IsLeftUp`
  (indices 106 and 285) [documented: wbConditionRunOnDecider].

Observed on IDLE [observed]: 1,903 conditions on 1,161 records (at most 13 on one), 62
distinct functions. Most used: GetRandomPercent 362, GetHitLocation 305, GetActorValue
178, IsLastIdlePlayed 86, GetFurnitureMarkerID 79, GetSitting 70, GetCurrentAIProcedure
64, IsCurrentFurnitureObj 56, IsWeaponOut 51, IsMoving 48, IsInCombat 47. Three indices
have no opcode in the script catalogue: 451 (86 uses), 258 (2), 123 (1); xEdit names
them IsLastIdlePlayed (parameter: an IDLE), GetUsedItemLevel and IsGreetingPlayer
[documented: xEdit wbCTDAFunctions]. Run-on values: 0 (subject) 1,797, absent 98,
20 × 5 (all IsLeftUp), 1 × 2 (GetIsID), 7 × 1 (IsInCombat — outside xEdit's 0–4 actor
list; §5).

### 1.3 Actor bases: KFFZ, NIFZ, NIFT

- **KFFZ** (NPC_ and CREA): a list of `.kf` file names; **NIFZ** (CREA): a list of model
  file names; both shown by xEdit only when the actor does not take Model/Animation from
  its template [documented: xEdit]. Stored as NUL-terminated names with one extra NUL
  after the last [observed: all instances in FalloutNV.esm]; decoded with the schema type
  `zstring_array` (added for this note; `crates/fnv-esm/src/schema.rs`). No NPC_ in the
  ten masters has a KFFZ; 202 CREA do (2,719 names); 1,572 CREA have a NIFZ (3,454 names)
  [observed].
- **NIFT** (CREA, 2,238 instances): per NIFZ model, a count-prefixed list of 24-byte
  texture hashes [documented: xEdit wbModelInfos]. Schema v1 cannot express nested
  count-prefixed arrays, so NIFT stays undecoded (docs/design/layer1.md); 1,751 instances
  are 4 bytes [observed], presumably a zero count [hypothesis].

## 2. The IDLE tree

### 2.1 Structure [observed]

- Every parent and previous link resolves to a loaded IDLE; there are no parent cycles.
  597 roots (no parent); depth up to 9 (by depth: 597, 400, 112, 195, 272, 96, 32, 27, 9, 2).
- IDLEs whose `MODL` names a folder (411, e.g. `Characters\_Male\IdleAnims`) are the
  inner nodes: 362 of them have children. IDLEs naming a `.kf` (1,331) are the leaves:
  1,330 have no children. 1,322 of the found `.kf` names are inside an `idleanims\`
  folder; 170 `.kf` files are named by more than one IDLE (1,132 distinct).
- Siblings (same parent) chain through `previous`: in 357 of 364 sibling groups the links
  form exactly one chain from a first sibling (previous null or outside the group)
  through all of them. `Idles::children` returns siblings in that chain order.
- Conditions sit on both levels: 331 of 363 inner nodes and 830 of 1,379 leaves have some.
- 452 roots name a `.kf` and have no children; 244 of their Editor IDs start with `Loose`.

### 2.2 Reading of the structure [hypothesis]

The tree reads like a decision tree for Layer 6: starting from the roots in chain order,
an actor descends into the first node whose conditions pass and plays the first passing
leaf; the section byte says which body part or slot the result drives. Childless roots
would then be idles played on request (scripts, packages, dialogue) rather than chosen
by the tree. None of this is verified; see §5 and §7.

## 3. Animation files

### 3.1 Where `.kf` files sit [observed]

- 4,765 `.kf` files in the archives, all readable, 4,810 sequences (33 files hold more or
  fewer than one). Cycle type (nif.xml CycleType) 2 "clamp" 3,497, 0 "loop" 1,313.
- **Skeleton folder rule** (`anims::SkeletonIndex`): a `.kf` belongs to the nearest folder
  above it that holds a `skeleton*.nif` (prefer `skeleton.nif`), ignoring
  `meshes\clutter\` (bone-pile props). 4,498 files resolve that way in 51 folders.
  43 more are in folders whose actor folder ends like exactly one skeleton folder
  (`meshes\update\characters\_male` and `meshes\nvdlc01\characters\_male` →
  `characters\_male`; `creatures\yaoguai` → `nvdlc02\creatures\yaoguai`;
  `dlcanch\creatures\spidermine` → `nvdlc03\creatures\spidermine`) [rule: hypothesis].
  224 are in Fallout 3 DLC creature folders with no skeleton in the archives
  (`dlc04\creatures\hillfolk*`, `dlc05\creatures\abomination|combatdroid|maintenancerobot`,
  `dlcanch\creatures\chimera`).
- **Layout inside a skeleton folder**: files directly in it (weapon/movement sets,
  1,007 in `characters\_male`), `idleanims\` (982 in `_male`), `locomotion\` with
  `hurt\`, `male\`, `female\`, `child\` (and `toddler\` in `_1stperson`), and in some
  creature folders `specialanims\` (KFFZ targets, §4).
- Groups: `characters\_male` 2,114 files (666 named by an IDLE), `characters\_1stperson`
  442 (none named by an IDLE), 57 creature folders 2,209 files.

### 3.2 Names [observed]

- A sequence name is rarely the file name (494 of 4,810). File stems are mostly a prefix
  plus the sequence name (`1hpattack5down.kf` → `Attack5Down`). Prefixes by files: `mt`
  567, `h2h` 448, `1hp` 346, `2hr` 287, `2ha` 152, `2hl` 150, `2hm` 133, `2hh` 128, `1hm`
  123, `sneak2hr` 108, `sneak1hp` 97, `1gt` 74, `sneak2ha` 64, `swimmt` 56, then `sneak*`,
  `pa*`, `vats*`, `1lm`, `1md`; 1,181 files do not end in their sequence name
  (1,007 of them in `idleanims\`).
- The weapon prefixes line up with xEdit's 14 WEAP animation types (Hand to Hand, Melee
  1/2 Hand, Pistol Ballistic/Energy, Rifle Ballistic/Automatic/Energy, Handle, Launcher,
  Grenade Throw, Land Mine, Mine Drop, Thrown) [documented: xEdit wbWeaponAnimTypeEnum];
  the mapping (`h2h`, `1hm`, `2hm`, `1hp`, `2hr`, `2ha`, `2hh`, `2hl`, `1gt`, `1lm`,
  `1md`; no separate energy prefixes) and the `mt`/`sneak`/`swim`/`pa`/`vats` stance
  prefixes are [hypothesis].
- Sequence names (270 kinds, `SpecialIdle_…` collapsed): `SpecialIdle_*` 1,525, Equip 138,
  Unequip 135, Aim 132, Forward 115, FastForward 96, Holster 92, Idle 86, AttackRight 81,
  Backward 77, TurnRight 75, TurnLeft 74, … — the animation group names Layer 6 will
  select by [hypothesis].
- Text keys, by first word: start and end 4,848 each, enum 3,754, sound 3,071, blend
  1,918, hit 931, a 734, blendin 708, m 563, endloop 435, startloop 426, prn 376, attach
  183, decal 147, detach 141, eject 88, blendout 72, fire 62, loop 62, release 40, hold
  32; misspellings occur (`blemd`, `blen`, `eum`). Their meanings are Layer 6 work.

### 3.3 Binding to the skeleton [observed]

A track binds when its target node name is a node name of the folder's skeleton (exact
match). 222,219 of 235,204 tracks (94.5 %) bind; 29 of 53 groups with a skeleton bind
every track. The rest name nodes that are not in the skeleton: `##`-prefixed nodes
(weapon and prop parts such as `##SpeedLoader`, `##Trigger`; 2,634 among the per-group
top names), `HeadAnims:0` (1,108 in `characters\_male`, a target on the head mesh
[hypothesis: facial morph controller]) and creature props (Mr. House's bed panels,
Securitron screens). `characters\_male`: 111,189 of 119,031 bound; `_1stperson`: 16,559
of 18,881; creatures: 94,471 of 97,292.

## 4. Lookups

- **IDLE `.kf`** [observed]: 1,322 of 1,331 found; 9 missing (e.g. a roach wings idle and
  three Securitron screen idles; listed by `idle_missing`). 408 of the 411 folder models name
  a folder that holds `.kf` files; the other 3 name `idleanims` folders of creatures with
  no `.kf` there (a roach and a Zax eye).
- **CREA KFFZ** names are bare file names looked up as `<skeleton folder>\specialanims\
  <name>`, then `<skeleton folder>\<name>` [rule: hypothesis; observed: 2,694 names found
  in `specialanims\`, none needed the second place]. 25 names are missing (19 distinct,
  on Mr. Gutsy and Robobrain records).
- **CREA NIFZ** names are found as `<skeleton folder>\<name>` 3,372 times; 82 are missing
  (21 distinct; some on records whose skeleton is `meshes\marker_creature.nif`).

## 5. Open questions for the owner

1. **Bits 0x40 / 0x80 of the IDLE group byte**: xEdit calls 0x40 "Loose Idle" and links
   0x80 to "must return a file". 441 records set 0x40 (414 Special Idles); 93 set 0x80.
   Do you know (GECK UI, wiki) what they do?
2. **Tree walk**: is the reading in §2.2 right — roots in previous-chain order, first
   passing node, first passing leaf? A capture could settle it (§7).
3. **Childless roots** (452, 244 named `Loose…`): played only on request (scripts,
   packages, dialogue)?
4. **Seven sibling groups** whose previous links do not form one chain: data slips, or a
   rule (several chains, evaluated by FormID)?
5. **Run-on 7** on one IsInCombat condition (outside 0–4): ignore, or does it mean
   something?
6. **Condition functions without an opcode** in `schemas/script/functions.json` (451
   IsLastIdlePlayed, 258, 123): should the generated catalogue take xEdit's index for
   them, as it does for other unused vanilla functions?
7. **File prefixes** (§3.2): is the mapping to WEAP animation types right, and where do
   energy pistols and rifles take their animations?
8. **Fallout 3 DLC folders without skeletons** (224 files): leftovers to ignore?

## 6. CLI contract

`fnvtool actor anims <Data> <plugins…> --schema <dir> [--nif-schema <dir>]
[--functions <file>] [--summary-only]` → `fnv.actor.anims/1`. `--nif-schema` defaults to
`nif` next to `--schema`, `--functions` to `script/functions.json` next to it (condition
names are left out when that file is absent). Paths are VFS paths (lower case,
backslashes); FormIDs print as `0x%08X`; f32 values are widened to f64.

Top level: `schema`, `summary`, `groups`, `idle_missing`, `kffz_missing`, `nifz_missing`,
`issues` (IDLE loader issues; `actor_bases` for the KFFZ/NIFZ reader when not empty),
`load_order`, `archives`, `condition_function_names` (how many indices have a name);
unless `--summary-only`, also `files` and `idles`.

- `summary`: `kf_files`, `unreadable`, `sequences`, `files_not_one_sequence`,
  `sequences_named_like_file`, `groups`, `files_by_skeleton_rule` {`ancestor`,
  `sametail`, `none`}, `tracks_checked`, `tracks_bound` (only files with a readable
  skeleton), `text_key_kinds` (first word, lower case; `-options` for exporter option
  strings), `sequence_name_kinds` (names with `_` collapsed to `Head_*`),
  `file_prefixes` (file stem minus its first sequence name that it ends with; `(named
  like file)`, `(no suffix match)`), `idles` {`records`, `conditions`, `by_section`
  {section name or `unknown N` or `no DATA`: {`records`, `high_bits_0xNN`, `kf_found`,
  `kf_missing`, `folder_with_kf`, `other_model`, `no_model`}}, `found_kf_by_group`,
  `tree` (`fnv_actor::idle::TreeStats`: `idles`, `roots`, `dangling_parent`,
  `dangling_previous`, `previous_other_parent`, `sibling_groups`,
  `sibling_groups_one_chain`, `in_parent_cycle`, `by_depth`), `condition_functions`
  (name or `index N`: count), `condition_run_on` (value or `absent`: count)}, `kffz` and
  `nifz` (counts: `CREA with KFFZ`, `names found in specialanims`, `names found in
  skeleton folder`, `names missing`, `lists without MODL`).
- `groups[]`, ordered by folder: {`folder`, `skeleton`, `skeleton_rule` (`ancestor` |
  `same_tail` | `none`), `skeleton_error`, `skeleton_nodes`, `skeletons_in_folder`,
  `files`, `unreadable`, `sequences`, `tracks`, `bound`, `subfolders` {relative folder:
  files}, `named_by_idle`, `named_by_kffz`, `unbound_names`, `unbound_top` [[name,
  count] × ≤ 12]}.
- `files[]`, ordered by path: {`path`, `group`, `sequences` [{`name`, `start_time`,
  `stop_time`, `cycle_type`, `text_keys` (count), `tracks`, `bound` (null without a
  skeleton)}]} or {`path`, `group`, `error`}.
- `idles[]`, ordered by FormID: {`form`, `edid`, `plugin`, `model`, `model_kind` (`kf` |
  `other` | `none`), `found` (kf: file exists; other: a folder holding `.kf` files; null
  without a model), `group_raw`, `section`, `loop` [min, max], `replay_delay`, `flags`,
  `parent`, `previous`, `conditions` [{`function`, `name`, `operator`, `or`, `type_raw`,
  `comparison` (f32, null when it is a global), `comparison_raw`, `param1_raw`,
  `param2_raw`, `run_on`, `reference_raw`}]}.
- `idle_missing[]` {`form`, `edid`, `section`, `model`}; `kffz_missing[]` and
  `nifz_missing[]` {`form`, `edid`, `skeleton`, `name`}.

Library: `fnv_actor::idle::{Idles, Idle, Condition}` (`Idles::build(lo, schema)`,
`children`, `depth`, `tree_stats`) and `fnv_actor::anims::{SkeletonIndex, bind,
kffz_candidates, actor_anim_lists}`.

## 7. Captures wanted

- With `nvf_capture`: an NPC standing idle near furniture and in combat, logging the
  idle played (`IsLastIdlePlayed` / the console's `GetCurrentAIProcedure`) over time, to
  check the tree walk of §2.2.
- `PlayIdle` on a childless `Loose…` root and on an inner node, to see which leaf plays.

## 8. "Random Anim Start" (placed models)

- **Fact** [documented: xEdit wbDefinitionsFNV.pas, record flag lists]: record header
  flag bit 16 (`0x00010000`) is "Random Anim Start" on ACTI, CONT, DOOR, FURN, LIGH,
  MSTT, TACT and TERM; on other types bit 16 means something else (REFR: "High Priority
  LOD") or nothing. The GECK wiki's "World Objects" page (owner's export
  `objects.xml`) describes it: the object starts its idle animation at a random time, so
  several copies in one scene do not animate in step.
- **Data** [observed, ten masters]: 124 base records set it — ACTI 38 of 1,837, CONT 10 of
  3,417, DOOR 1 of 474, FURN 1 of 256, LIGH 9 of 728, MSTT 65 of 300, TACT 0, TERM 0.
- **Engine**: a placed model that plays its embedded idle (`nvf_assets::model::Rig`,
  docs/spec-notes/nif.md §6) starts at a phase spread by the reference FormID when its
  base has the flag (`Rig::spawn_at_phase`, `fnv_actor::anims::random_anim_start`), the
  same spread as actors' idles (actors.md §7.3) — repeatable, not the game's random
  roll. Bases without the flag start at the clip's start, so their copies stay in step
  [hypothesis: the game starts unflagged objects at time 0]. The run summary counts
  `refs_random_start`.
