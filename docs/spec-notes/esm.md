# Spec note: plugin files (ESM/ESP), Fallout: New Vegas

Clean-room description in plain language; `crates/fnv-esm` is implemented from it.
Tags: **[documented]** (public source named), **[observed]** (checked against the
user's FNV 1.4.0.525 masters: FalloutNV.esm, the 4 story DLCs, GunRunnersArsenal and
the 4 pre-order packs), **[hypothesis]**.

Sources: UESP "Tes4Mod:Mod File Format" (record/group/subrecord container, shared by
the Oblivion/Fallout 3/New Vegas generation); xEdit `wbDefinitionsFNV.pas`
(record layouts; cited per entry in `schemas/`).

## Container structure

A plugin is a sequence of **records** and **groups**. All integers little-endian.

**Record header, 24 bytes** [documented][observed]:
type (4 ASCII chars), data size u32, flags u32, FormID u32, version-control u32,
form version u16, version-control u16. The data that follows is `data size` bytes.
- Form version is 15 for 99% of FalloutNV.esm records; older values (1–14) occur on
  records last saved by older tool versions [observed].
- Flag 0x00040000: the data is a u32 uncompressed size followed by a zlib stream
  [documented][observed: 33,179 compressed records in FalloutNV.esm].
- Flag 0x1 on the first record (TES4) marks a master file [documented][observed].

**Group header, 24 bytes** [documented][observed]: `GRUP`, total size u32 (including
this header), label (4 bytes), group type i32, then four u16s (stamp, unknown,
version, unknown) that are preserved verbatim. Group types:

| Type | Label | Contents |
|---|---|---|
| 0 | record type | all top-level records of that type |
| 1 | parent WRLD FormID | worldspace children |
| 2 / 3 | block / sub-block number (i32) | interior cell blocks |
| 4 / 5 | grid y, x (two i16) | exterior cell blocks |
| 6 | parent CELL FormID | cell children |
| 7 | parent DIAL FormID | topic children (INFO) |
| 8 / 9 | parent CELL FormID | persistent / temporary references |
| 10 | parent CELL FormID | visible-distant children; documented, **absent** in all shipped FNV masters [observed] |

**Subrecords** inside record data: type (4 chars), size u16, data. A subrecord of type
`XXXX` with size 4 carries a u32 that is the real size of the *next* subrecord, whose own
size field is then 0 [documented][observed: one such subrecord per large master, in the
WRLD offset table]. 

**Text** is Windows-1252, usually NUL-terminated (`zstring`) [observed]. FNV has no
localized-string tables [observed: no TES4 localized flag, strings are inline].

## TES4 header record

Subrecords [documented][observed]:
- `HEDR` (12 bytes): version f32 (1.32–1.34 across masters), record count i32, next
  object ID u32. **The record count equals (records + groups − 1)**, i.e. every record
  and group except TES4 itself, in all 10 masters [observed]. This gives a free
  self-check that does not depend on any third-party tool.
- `CNAM` author, `SNAM` description (optional zstrings).
- `MAST` zstring + `DATA` u64 pairs, one per master, in order. DATA is 0 in every
  shipped plugin [observed]; described as master file size in community docs.
- `ONAM`: array of u32 FormIDs. In every shipped DLC, ONAM lists **exactly** the
  overridden records that sit in cell temporary-children groups (type 9) and nothing
  else [observed: 179/109/202/159/94 entries for DM/HH/OWB/LR/GRA, set-equal]. Those are
  REFR, ACRE, ACHR and NAVM records, plus 7 LAND records in LonesomeRoad.esm [observed]. Purpose believed to be letting the engine find overridden
  temporary references without loading the cell [hypothesis].

## FormIDs and load order

- A stored FormID's top byte is an index into **that file's** master list; the value
  equal to the master count means "this file" [documented][observed].
- At load, the top byte is replaced by the load-order index of the referenced file;
  later files override earlier definitions of the same global FormID [documented].
- **Injected forms** [observed]: the four pre-order packs (Caravan/Classic/Mercenary/
  Tribal) define *all* their records with top byte 00 — FalloutNV.esm's space — although
  those FormIDs do not exist in FalloutNV.esm. LonesomeRoad injects one AVIF the same
  way. The engine must accept new forms in a master's ID space.
- **Out-of-range index** [observed]: GunRunnersArsenal.esm has one REFR with FormID
  0x02000801 although it has only one master (valid indices 0 and 1). What the game does
  with it is unknown [hypothesis: treated as the file's own index, or dropped]. The
  loader reports it as `unmappable`.
- FormIDs below 0x800 in the first master are reserved for engine built-ins
  [hypothesis, community knowledge]; references into that range need not resolve to a
  record.

## Data irregularities in the user's files

- FalloutNV.esm, LAND 0x00150FC0 (file offset 0xB0CFF04): the zlib stream decodes
  completely to exactly the declared 4,385 bytes and parses into valid subrecords, but the
  stored Adler-32 is wrong (stored 0x6E704ECC, computed 0x6C404EC4). Python's zlib
  rejects it. Arithmetic on the two checksums says the authored data had one byte, at
  offset 4,315 (inside VHGT terrain heights), larger by 8 than what decodes [observed;
  the single-byte explanation is a hypothesis]. Either the shipped file has this defect
  or the user's copy is damaged — **verify the SHA-256 against a second copy or after a
  Steam "Verify integrity of game files"**. The reader accepts such streams when the
  output size matches, and reports them. Re-hashed read-only on 2026-10-09: the owner's
  current `Data\FalloutNV.esm` (245,650,747 bytes) still has SHA-256
  `50991d36804b7d1e70df1afd7471b72f0e29d1b456ee2516a9717c002564e7c1`, the value the
  anomaly was first found in [observed]; telling a shipped defect from local damage still
  needs a Steam "verify" (owner-only) or a second copy.

## Cross-check against xEdit (M1)

The owner's FNVEdit export (`tools/capture/xedit/export_inventory.pas`, private goldens
`esm/<Plugin>.xedit.inventory.json`) covers all 10 masters, 629,788 records. Type,
EditorID and FULL agree with `fnvtool` and with the independent `oracle_esm.py` on every
record except **four rows** (2026-10-09) [observed]. In all four, both of our readers
report what the file holds and the export shows xEdit's own normalisation:

| Plugin | Record | File holds | xEdit export | Ours = oracle |
|---|---|---|---|---|
| FalloutNV.esm | DIAL 0x00152CCA | PNAM, DATA only; **no EDID** | `edid: ""` | `edid: null` |
| DeadMoney.esm | RACE 0x01005819 `Christine` | EDID, DESC, DATA, ...; **no FULL** | `full: ""` | `full: null` |
| OldWorldBlues.esm | CLAS 0x0101533F `NVDLC03SinkVendorClass` | EDID, DESC, DATA, ATTR; **no FULL** | `full: ""` | `full: null` |
| GunRunnersArsenal.esm | REFR stored as **0x02000801** | that FormID | `0x01000801` | `0x02000801` |

- **xEdit adds missing required subrecords as empty ones** [documented: xEdit
  `Core/wbImplementation.pas`, branch dev-4.1.6, the record-loading code right after
  `mrDef.AfterLoad`: for a record that is neither deleted nor a partial form, every member
  the record definition marks *required* and the file lacks is added as an empty element,
  logged as "Adding missing record"]. In `wbDefinitionsFNV.pas` DIAL starts with
  `wbEDIDReqKC`, RACE and CLAS with `wbEDIDReq, wbFULLReq` [documented]. The export script
  tests presence (`ElementExists` / `ElementBySignature`), finds the placeholder and writes
  `""`. Check over all 10 masters [observed]: these three are the *only* records that lack
  an EDID or FULL which xEdit's definition marks as required; all 1,126,037 other absent
  EDID/FULL values (optional in xEdit) are `null` in the export, as in ours. The file has
  no such subrecord, so `null` (absent) is the faithful reading; the difference is xEdit
  normalisation, not data.
- **xEdit folds an out-of-range master index into the file's own index** [documented:
  xEdit `wbImplementation.pas`, `TwbMainRecord.DoGetFixedFormID`: a file index greater
  than or equal to the plugin's master count is replaced by the plugin's own index]. GRA
  has one master, so 0x02000801 is shown as 0x01000801. We keep the stored value and the
  load order reports it as `unmappable` [observed]. Details of that REFR [observed]: its
  base (NAME) is 0x02000800, also out of range (GRA's own 0x01000800 is the WEAP
  `WeapNVDLC00Faderator`, "The Faderator"); it sits in the temporary children of
  `TestMentats` (0x00108BED, FalloutNV.esm's "Mentats Test Level"); DATA position
  (2341.27, 2058.89, 6991.75), rotation 0; no other subrecords. A developer test
  placement saved while the file had one more master is the likely origin [hypothesis].
  What the game does with it: Unknowns 4.

Consequence: `L1.esm.inventory_xedit.{FalloutNV,DeadMoney,OldWorldBlues,GunRunnersArsenal}`
fail by exactly these rows (scores 0.999998, 0.999975, 0.999981, 0.997433) and the other
six pass with every row equal. Copying xEdit's behaviour into our reader would make the
inventory say something the file does not, so the reader stays as it is; the comparator,
scenarios and goldens are untouched (parity-lab/AGENT.md).

## Round-trip guarantees (tested)

- Re-serializing the record/group tree reproduces all 10 masters byte-for-byte
  (FalloutNV.esm, 245,650,747 bytes, in ~3 s) [observed].
- Every record payload re-serializes identically from its parsed subrecords, including
  XXXX handling [observed].

## Unknowns (need analysis or the original game)

1. Field-level layouts for the (record, subrecord) pairs not yet in `schemas/fnv`
   (FalloutNV.esm, 2026-10-09: 183 of 1,155 pairs, 5.3% of subrecord instances; biggest:
   PACK, NAVM, NAVI, DIAL, TERM) — tracked by `fnvtool esm coverage` as the work queue.
2. Order-dependent subrecords (same signature meaning different things depending on what
   precedes it, e.g. condition blocks, model sub-structures) — schema format v1 has no
   context rules yet.
3. GMST `DATA` type depends on the first letter of the Editor ID (s/i/f) [documented in
   xEdit as a union]; schema v1 has only size-decided unions (`{"union": [...]}`, used for
   WATR DATA), not EditorID-decided ones, so GMST DATA stays undecoded.
4. Engine handling of the out-of-range FormID in GRA, of the bad-checksum LAND, and of
   the dangling reference DeadMoney INFO 0x01011B23 → 0x010108DD (no such record in any
   loaded file) [observed].
5. Version-control fields and group stamp/unknown fields — preserved, not interpreted.
6. Two QUST `DATA` subrecords in FalloutNV.esm (e.g. 0x0001E5CA) are 2 bytes long, while
   the documented layout needs at least 4 [observed]; engine behaviour unknown.
