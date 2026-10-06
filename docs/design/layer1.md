# Layer 1 design: BSA reader and ESM/ESP parser

Spec section 12 deliverable, in the section 9 format. Status as of 2026-10-06: **implemented
and validated against the user's install** (FNV 1.4.0.525); the remaining exit-criterion
item is the cross-check against an FNVEdit export run on the user's machine.

## 1. Summary and assumptions

Task: design (and, given autonomy, build) the archive and plugin layer so that
FalloutNV.esm and every DLC master load, every FormID resolves, and a complete record
inventory matches a trusted reference.

Assumptions (defaults chosen while the user was away; easy to revisit):
- **Rust**, standard library only. The sandbox has no crates.io access, so zlib inflate,
  JSON and SHA-256 are implemented in-tree (`fnv-inflate`, `fnv-json`, `fnv-util`). Swapping
  in `miniz_oxide`/`serde_json` later is a local change.
- Licence placeholder **GPL-3.0-or-later** (OpenMW's choice). Not yet decided by the user.
- Ground truth = an independent Python reader written by a separate agent from the spec
  notes only, plus the HEDR record count authored by Bethesda's own tool, plus (pending)
  FNVEdit exports.

## 2. Findings and design

### 2.1 Crates (format libraries have no engine dependencies, spec section 7)

| Crate | Role |
|---|---|
| `fnv-util` | Windows-1252 text, SHA-256, bounds-checked LE cursor |
| `fnv-inflate` | zlib/DEFLATE decoder (RFC 1950/1951) with trailer diagnostics; stored-block encoder |
| `fnv-json` | order-preserving JSON value, strict parser, writer |
| `fnv-bsa` | BSA v104 reader, name hash, canonical writer, byte-identical rewrite |
| `fnv-esm` | `raw` container layer → `plugin` → `loadorder` → `schema`/`decode` → `inventory`/`survey` |
| `fnvtool` | candidate CLI; every command prints one JSON document (`docs/cli-contract.md`) |

### 2.2 BSA (details and tags in `docs/spec-notes/bsa.md`)

- Header, folder records, file-record blocks, name block and data layout: [documented]
  by UESP, every rule **[observed]**: a writer using only these rules rebuilds 14 of 15
  staged archives byte-for-byte; the 15th is a mod-manager file.
- Name hash: [documented], verified on all 15,072 files and every folder [observed].
- Archive flag 0x80 does **not** mean embedded names in FNV [observed]; 0x100 never set.
- Two shipped zlib irregularities (missing Adler-32; size-0 compressed entry) must be
  accepted [observed].
- Data is read lazily by offset, and `bsa verify` streams its rewrite into a byte
  comparator, so even the 1.6 GB archives need only the directory plus one file in
  memory [observed on the staged archives].

### 2.3 Plugins (details in `docs/spec-notes/esm.md`)

- Record/group/subrecord container, compression, XXXX: [documented], round-trip
  **[observed]** on all 10 masters (FalloutNV.esm 245 MB in ~3 s).
- HEDR record count = records + groups − 1 in every master [observed] — a free oracle.
- ONAM = exactly the overridden records in cell temporary-children groups (REFR, ACRE,
  ACHR, NAVM; also 7 LAND in LonesomeRoad) in every DLC [observed].
- Pre-order packs inject new forms into FalloutNV.esm's FormID space [observed].
- Irregular data in the user's masters [observed], each tolerated and reported:
  one LAND with a wrong zlib checksum (FalloutNV.esm; shipped defect *or* local damage —
  [hypothesis], see §4); one REFR whose FormID uses a master index beyond the master list
  (GunRunnersArsenal.esm); one dangling reference (DeadMoney INFO 0x01011B23 →
  0x010108DD); two QUST DATA subrecords only 2 bytes long where the documented layout
  needs at least 4 (FalloutNV.esm 0x0001E5CA and 0x0002242A — both have old form versions
  1 and 2, so an older legacy layout is the likely explanation [hypothesis]).

Tolerance policy (spec section 7): structural overruns are hard errors with offsets;
everything else (unknown signatures, mis-sized data, checksum anomalies, unmappable
FormIDs) is preserved and reported. Nothing is silently dropped: records keep their raw
bytes, so a rewrite is byte-identical even when the schema knows nothing about them.

### 2.4 Load order and FormIDs

`LoadOrder::new(&[Plugin])` checks that each master is present and loads earlier, builds
per-plugin master-index → load-index maps, and indexes every record by global FormID
(`Vec<DefRef>` per FormID, last = winner). API: `to_global`, `to_local`, `winner`,
`exists`. Results on base + 9 DLC/packs [observed]: 628,463 unique global forms; 780 forms with
2–6 definitions.

**Reference resolution** [observed] (every FormID-typed field the schema knows): 1,002,308 fields;
102,646 null; 893,586 resolve; 6,074 point into the engine built-in range below 0x800
(4,566 of them 0x14 [hypothesis: the player reference]); **1 dangling**; **1** with an
invalid master index (the GRA REFR). So every FormID the schema can see resolves, except
those two irregularities.

### 2.5 Schema-as-data strategy

- Layouts live in `schemas/fnv/*.json` (`fnv.schema/1`), loaded and validated at runtime;
  correcting a layout needs no recompile.
- Type system: primitives, `formid`, `zstring`, named types, structs with trailing
  optional fields (form-version growth), fixed or fill-to-end arrays, enums, flags.
- Lookup: (record type, signature) first, then `common`.
- Every entry carries `confidence` and a `note` citing its source (currently xEdit
  `wbDefinitionsFNV.pas` / `wbDefinitionsCommon.pas`, MPL-2.0, read as documentation and
  re-expressed in our own format).
- Coverage is measured, not assumed: `fnvtool esm coverage` decodes every subrecord and
  ranks undecoded (record, signature) pairs by instance count. That list is the work
  queue (spec section 7: logging of unhandled data drives the work).
- `fnvtool esm survey` gives purely observational statistics per pair (sizes, string-like,
  FormID-array-like) to check or derive layouts.

Current coverage [observed] (subrecord instances decoded exactly by a documented layout;
about 0.5% of FalloutNV.esm instances are fields the layout itself marks as opaque bytes,
e.g. compiled script data, counted separately as `opaque_bytes`):

| Plugin | Decoded | Pairs fully decoded | Trailing | Errors |
|---|---|---|---|---|
| FalloutNV.esm | 93.66 % of 2,321,456 | 833 / 1,155 | 0 | 2 |
| DeadMoney.esm | 94.32 % | 624 / 889 | 0 | 0 |
| HonestHearts.esm | 92.93 % | 602 / 848 | 0 | 0 |
| OldWorldBlues.esm | 95.76 % | 603 / 854 | 0 | 0 |
| LonesomeRoad.esm | 94.76 % | 578 / 810 | 0 | 0 |
| GunRunnersArsenal.esm | 92.51 % | 224 / 259 | 0 | 0 |

Largest undecoded record types by instances: PACK (AI packages), NAVM/NAVI (navmesh),
IMAD (image-space modifiers, 12,002), TERM (3,925), IDLE (3,194); plus CREA NIFT/NIFZ,
DIAL INFC/INFX, REFR TNAM. **Record-specific layouts exist for 40 of the 107 record types
present**; the other 67 decode only their shared subrecords (EDID, FULL, OBND, models,
scripts, …). Adding them is the main remaining Layer 1 work item. Schema v1 limits behind several gaps: no context rules
(the same signature meaning different things by position), no length-prefixed strings or
string arrays, no unions (GMST DATA depends on the Editor ID's first letter). Schema v2
should add `union` (with a decider: size, or a sibling field, or EDID prefix),
`lstring`/`string_array`, and ordered subrecord groups.

## 3. Code and schema

- Code: `crates/*` (tests: `cargo test --workspace --release`, 36 unit and integration tests).
- Schemas: `schemas/fnv/{common,world,objects,gameplay}.json` (40 record types, 420
  subrecord definitions, 40 shared types).
- Lab: `parity-lab/` (Python stdlib; 26 self-tests), scenarios in
  `parity-lab/scenarios/layer1_*.json`.
- Capture: `tools/capture/` (oracles, xEdit script, inventory; 16 self-tests).

Parity result on the cloud-staged data [observed]: **84/84 capturable Layer 1 scenarios pass** (10
plugin inventories incl. all 465,016 FalloutNV.esm records, headers, stats, byte-identical
round-trips; 15 archive listings with SHA-256 of every decoded file, headers,
round-trips). The other 31 scenarios need data that only exists on the user's machine
(7 archives over 400 MB; 10 FNVEdit exports) and report as `error` until captured.

## 4. Step-by-step validation plan against FalloutNV.esm

Each step is a lab scenario or a command; all steps 1–6 pass today on the staged copy.

1. **Container integrity**: `fnvtool esm verify FalloutNV.esm` → byte-identical rewrite,
   0 decompression errors, 0 subrecord parse errors, payload round-trip identical
   (scenario `L1.esm.roundtrip.FalloutNV`).
2. **Bethesda self-check**: `fnvtool esm stats` → HEDR count = records + groups − 1
   (`L1.esm.stats.FalloutNV`, which also compares per-type and per-group-type counts with
   the independent oracle).
3. **Header**: masters, version, next object ID, ONAM size vs oracle (`L1.esm.header.*`).
4. **Record inventory**: all 465,016 records keyed by FormID; type, EditorID, FULL,
   flags, form version vs the independent Python oracle (`L1.esm.inventory.FalloutNV`).
5. **Load order**: `fnvtool esm loadorder` over base + DLC: no missing masters, every
   record FormID mappable (one known exception), overrides/injected forms/ONAM reported.
6. **Field decoding**: `fnvtool esm coverage --schema schemas/fnv` → zero `trailing`,
   errors only where noted; reference resolution via `loadorder --schema`.
7. **Third-party cross-check (pending)**: FNVEdit export vs candidate
   (`L1.esm.inventory_xedit.*`) — the M1 exit criterion.
8. **Repeat on DLC masters** (steps 1–7 per plugin; scenarios exist for all 10).

## 5. Open questions and verification steps for the user

1. **Run the FNVEdit export** (`tools/capture/xedit/export_inventory.pas`, steps in
   `tools/capture/README.md`) for all 10 masters and drop the outputs into the private
   goldens directory as `esm/<Plugin>.xedit.inventory.json`. This closes the M1 exit
   criterion ("matching a trusted third-party tool"). Expected difference: the GRA REFR
   0x02000801 may be displayed as 0x01000801.
2. **Big archives**: on your PC run `fnvtool bsa verify` and
   `python tools/capture/oracle_bsa.py` on `Fallout - Meshes/Textures/Textures2/Sound/
   Voices1.bsa`, `LonesomeRoad - Main.bsa`, `OldWorldBlues - Main.bsa`, or move the lab
   run to your machine (copy `lab.local.example.json` to `lab.local.json`).
3. **Checksum anomaly**: compare the SHA-256 of your `FalloutNV.esm`
   (`50991d36804b7d1e70df1afd7471b72f0e29d1b456ee2516a9717c002564e7c1`) after Steam
   "Verify integrity of game files", or with another copy, to tell a shipped defect from
   local damage (LAND 0x00150FC0).
4. **In-game behaviour checks** (later, via xNVSE logging): what the engine does with the
   out-of-range FormID 0x02000801 in GRA, and with the dangling DM INFO reference.
5. Your live `Documents\My Games\FalloutNV\Fallout.ini` / `FalloutPrefs.ini` `[Archive]`
   section (archive list and invalidation settings) — needed for archive precedence.
6. Decisions still yours: language (Rust assumed), licence (GPL-3.0 assumed).

## 6. Updated spec notes

Written to `docs/spec-notes/bsa.md`, `docs/spec-notes/esm.md` and
`docs/spec-notes/layer0-install.md` (plain-language, tagged).
