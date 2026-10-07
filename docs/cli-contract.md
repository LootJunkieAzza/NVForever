# Candidate CLI contract (Layer 1)

The parity lab runs `fnvtool` and compares its stdout (one JSON document) with oracle
goldens of the **same schema**. Any oracle tool (xEdit export script, independent
reader) must emit these shapes. Exit code 0 = success, 1 = input could not be processed,
2 = usage error (`--help` exits 0); diagnostics go to stderr. `fnvtool --help` and
`fnvtool <group> <command> --help` describe every option.

Conventions: FormIDs are strings `"0x%08X"` (upper-case hex) and are **file-local**
(exactly as stored in the plugin, not load-order adjusted). Text is decoded from
Windows-1252 to UTF-8. Missing optional values are `null`.

## `fnvtool esm inventory <plugin>` → `fnv.esm.inventory/1`

```json
{
  "schema": "fnv.esm.inventory/1",
  "plugin": "FalloutNV.esm",
  "header": {"plugin": "...", "is_master": true, "flags": 1, "version": 1.34,
             "num_records": 542016, "next_object_id": "0xFF17BA48", "author": "...",
             "description": null, "masters": [], "overridden_forms": 0,
             "other_subrecords": [], "file_size": 245650747},
  "errors": [],
  "records": [
    {"formid": "0x0017B37C", "type": "GMST", "edid": "sPoisonNoWeaponMessage",
     "full": null, "flags": 0, "form_version": 15}
  ]
}
```

`records` lists every record except TES4, in file order. `header.version` is the HEDR
f32 widened to a double without rounding (e.g. 1.340000033378601). `overridden_forms` is
the number of u32 entries in ONAM (0 when absent). `errors` entries are
`{"code": ..., "detail": ...}`; they are diagnostics and not compared. `edid`/`full` are the first
`EDID`/`FULL` subrecord read as NUL-terminated text. `flags` is the raw record-header
flags dword (including the compression bit 0x40000).

Lab comparator: **keyed** on `formid`, `select: "/records"`, fields
`["type", "edid", "full", "flags", "form_version"]` (the xEdit cross-check compares only
`type`, `edid`, `full`). Header compared separately with **exact** on `/header`,
including `file_size`.

## `fnvtool esm stats <plugin>` → `fnv.esm.stats/1`

```json
{"schema": "fnv.esm.stats/1", "plugin": "...", "records": 465017, "groups": 77000,
 "compressed_records": 33179, "deleted_records": 0, "hedr_num_records": 542016,
 "hedr_matches_records_plus_groups_minus_one": true, "top_groups": ["GMST", "..."],
 "by_type": {"ACHR": 3386, "...": 0}, "by_group_type": {"top": 101, "...": 0}}
```

`records` and `by_type` include TES4. `by_group_type` always lists all 11 group-type
names, in the order below, including zero counts; an out-of-range type N is counted under `unknown_N`. Group-type names: `top, world_children, interior_cell_block,
interior_cell_subblock, exterior_cell_block, exterior_cell_subblock, cell_children,
topic_children, cell_persistent_children, cell_temporary_children,
cell_visible_distant_children`.

## `fnvtool bsa list <archive> [--no-content-hash]` → `fnv.bsa.listing/1`

```json
{
  "schema": "fnv.bsa.listing/1",
  "archive": "Update.bsa",
  "header": {"version": 104, "archive_flags": 131, "file_flags": 283, "folder_count": 22,
             "file_count": 86, "total_folder_name_length": 645, "total_file_name_length": 1900},
  "warnings": [{"code": "...", "detail": "..."}],
  "errors": [],
  "files": [
    {"path": "meshes\\creatures\\nvsecuritron\\death.kf", "name_hash": "0x1776fd25640574e8",
     "folder_hash": "0x...", "stored_size": 2199, "compressed": false,
     "size": 2199, "sha256": "..."}
  ]
}
```

`path` is `folder\name` as stored (lower case, backslashes); when the folder name is
empty the path is the bare file name. `header.file_flags` is the low u16 only (the two
following bytes are not part of the contract). `size`/`sha256` describe the
decoded (decompressed) contents and are omitted with `--no-content-hash`. Lab comparator:
**keyed** on `path`, fields `["stored_size", "compressed", "size", "sha256", "name_hash"]`.

## Other commands (not yet scenarios)

| Command | Schema | Purpose |
|---|---|---|
| `bsa verify <archive>` | `fnv.bsa.verify/1` | byte-identical rebuild + decode every file |
| `bsa extract <archive> <path> <out>` | `fnv.bsa.extract/1` | write one decoded file |
| `esm header <plugin>` | (header object above) | TES4 summary |
| `esm verify <plugin>` | `fnv.esm.verify/1` | byte-identical rebuild, decompression, subrecord round-trip, zlib trailer anomalies |
| `esm coverage <plugin> --schema <dir>` | `fnv.esm.coverage/1` | schema decode coverage + ranked work queue |
| `esm decode <plugin> --schema <dir> --formid X \| --edid E` | `fnv.esm.decode/1` | schema-decoded record(s) |
| `esm loadorder <data-dir> <plugins…> [--schema <dir>]` | `fnv.esm.loadorder/1` | masters, overrides, injected forms, ONAM check, FormID-reference resolution |
| `esm survey <data-dir> <plugins…> [--plugin P]` | `fnv.esm.survey/1` | per (record, subrecord) size/string/FormID statistics |

### Top-level keys of the other documents

- `fnv.bsa.verify/1`: `archive`, `files`, `roundtrip_identical` (bool), `original_size`,
  `rewritten_size`, `first_difference` (byte offset or null), `decode_errors` [string],
  `warnings` [{code, detail}] (includes `zlib_trailer_missing`, `zlib_checksum_mismatch`,
  `data_not_contiguous`, `trailing_bytes`, `noncanonical_folder_name`, hash mismatches).
- `fnv.esm.verify/1`: `plugin`, `roundtrip_identical`, `first_difference`, `records`,
  `compressed_records`, `extended_size_subrecords`, `decompress_errors` [string],
  `zlib_trailer_anomalies` [string], `subrecord_errors` [string] (max 50),
  `payload_roundtrip_mismatches` [string] (max 50).
- `fnv.esm.coverage/1`: `plugin`; `summary` {subrecords, ok, unknown, trailing, error,
  ok_percent, opaque_bytes, distinct_pairs, pairs_fully_decoded}; `work_queue` [{record,
  sub, total, ok, unknown, trailing, error, top_sizes [{size, count}], first_error?}]
  sorted by undecoded instances; `record_errors` [string].
- `fnv.esm.decode/1`: `plugin`, `records` [{formid, type, type_name?, flags,
  form_version, subrecords [{sig, name?, status, value | raw (hex), error?}]}].
- `fnv.esm.loadorder/1`: `plugins` [{plugin, load_index, records, new_forms, overrides,
  unmappable_formids [formid], onam_entries, onam_entries_that_are_overrides,
  temporary_group_overrides, temporary_group_overrides_in_onam, injected_forms_by_type,
  overrides_by_type, overrides_not_in_onam_by_type}], `unique_global_forms`,
  `definitions_per_form` {count: forms}; with `--schema` also `references` {formid_fields,
  null, resolved, bad_master_index, unresolved_by_kind, unresolved_examples}.
- `fnv.esm.survey/1`: `records` {type: count}, `pairs` [{record, sub, count,
  distinct_sizes, top_sizes, zstring_like, formid_array_like, first_u32_formid,
  max_per_record}], `errors`.

# Layer 2 (meshes, animations, textures)

Inputs are BSA archives; every `.nif` / `.kf` (or `.dds`) file inside is reported, in
archive order. Paths are as stored in the archive (lower case, backslashes). Floats are
f32 values widened to JSON numbers without rounding; NaN and infinities are `null`
(two node translations in the game are NaN). -0.0 and 0.0 compare equal. Block references
are block indices. `error` / `reason` texts are diagnostics: tools should agree on whether
one is present, and the lab compares them, but none occur in the game data.

## `fnvtool nif stats <archive>` → `fnv.nif.stats/1`

```json
{
  "schema": "fnv.nif.stats/1",
  "archive": "Fallout - Meshes.bsa",
  "files": [
    {"path": "meshes\clutter\x.nif", "version": "20.2.0.7", "user_version": 11,
     "bs_version": 34, "num_blocks": 12, "strings": 9,
     "block_types": {"BSFadeNode": 1, "NiTriStrips": 2, "...": 0}, "error": null}
  ]
}
```

Everything here comes from the file header (docs/spec-notes/nif.md §2), so it is defined
for every version. `strings` is the string-table entry count (`null` for versions without
a string table). `block_types` counts blocks per type name (keys sorted). `error` is a
message when the header cannot be read, and then the other fields are `null`. Lab
comparator: **keyed** on `path`, fields `["version", "user_version", "bs_version",
"num_blocks", "strings", "block_types", "error"]`.

## `fnvtool nif scene <archive>` → `fnv.nif.scene/1`

```json
{
  "schema": "fnv.nif.scene/1",
  "archive": "Fallout - Meshes.bsa",
  "skipped": [{"path": "...", "reason": "no block-size table (version 20.0.0.4)"}],
  "objects": [
    {"key": "meshes\clutter\x.nif#0", "type": "BSFadeNode", "name": "x", "parent": null,
     "translation": [0.0, 0.0, 0.0], "rotation": [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0],
     "scale": 1.0}
  ],
  "shapes": [
    {"key": "meshes\clutter\x.nif#3", "type": "NiTriStripsData", "vertices": 24,
     "triangles": 30, "has_normals": true, "has_tangents": true, "uv_sets": 1,
     "has_vertex_colors": false, "bound_center": [0.0, 0.0, 1.5], "bound_radius": 3.2,
     "aabb_min": [-1.0, -1.0, 0.0], "aabb_max": [1.0, 1.0, 3.0]}
  ]
}
```

Only files with a block-size table are analysed; the others are listed in `skipped` (and
also files whose container cannot be read). `key` is `path#block`.

- `objects`: every block whose type is NiNode, a type derived from it (spec notes §3), or
  NiTriShape, NiTriStrips, BSSegmentedTriShape, in block order. `name` is the object name
  (`null` for "no string"). `parent` is the index of the first node (lowest block index)
  whose child list contains this block, else `null`. `rotation` is row-major.
- A file in which any block cannot be decoded is listed in `skipped` and contributes no
  objects or shapes.
- `shapes`: every NiTriShapeData and NiTriStripsData block. `vertices` is the stored vertex
  count. `has_tangents` means tangents are actually stored (normals present and the
  tangent bit set). `triangles` = triangle count
  if the has-triangles byte is set (lists), or the sum over strips of
  max(length − 2, 0) if the has-points byte is set (strips), else 0. `uv_sets` = the
  number of UV sets actually stored. `aabb_min`/`aabb_max` = component-wise min/max over
  the stored vertices (`null` when there are none).

Lab comparator: **keyed** on `key` for `/objects` and for `/shapes` (all fields), plus
**exact** on `/skipped`.

## `fnvtool dds list <archive>` → `fnv.dds.listing/1`

Defined in docs/spec-notes/dds.md together with the texture notes.

# Layer 3 (world data)

All `world` commands take the Data folder, the plugins in load order, and
`--schema <schemas/fnv>`; they print the load order back as `load_order`. FormIDs are
**load-order (global) FormIDs** as `"0x%08X"`: the top byte is the index of the defining
plugin in the given load order (docs/spec-notes/esm.md). Floats are f32 values widened to
JSON numbers. Facts behind the fields: docs/spec-notes/world.md.

A cell's references are the records in persistent (8), temporary (9) and
visible-distant (10) cell-children groups labelled with that cell, except LAND and NAVM.
When several plugins define the same reference FormID, the last one in load order wins,
including which cell's group it is in. The same rule picks the cell's LAND.

## `fnvtool world refs ... (--cell <edid|formid> | --world <edid|formid> --x X --y Y)` → `fnv.world.refs/1`

```json
{
  "schema": "fnv.world.refs/1",
  "cell": {"formid": "0x0000ABCD", "edid": "Goodsprings", "name": null, "world": "0x000DA726",
           "grid": [-18, 0], "interior": false, "water_height": 3.4028234663852886e38,
           "refs": 159, "land": "0x0000ABCE"},
  "refs": [
    {"formid": "0x00012345", "type": "REFR", "base": "0x00012300", "base_type": "STAT",
     "base_edid": "Rock01", "model": "meshes\rocks\rock01.nif",
     "position": [1.0, 2.0, 3.0], "rotation": [0.0, 0.0, 1.5707964], "scale": 1.0,
     "flags": 0, "persistent": false, "initially_disabled": false, "deleted": false,
     "enable_parent": null}
  ]
}
```

- `refs` sorted by `formid`. `type` = record signature of the winning definition.
- `base` = NAME mapped to the load order (`null` if absent or 0). `base_type`, `base_edid`
  come from the base's winning record; `null` when the base is not defined in the load
  order (engine-defined forms). `model` = the base's MODL, lower case, `/` → `\`,
  prefixed with `meshes\` unless already present (`trees\` for `.spt` SpeedTree
  models); `null` if absent or blank.
- `position`, `rotation` = DATA (rotation in radians, as stored); `[0,0,0]` if DATA is
  absent. `scale` = XSCL, else 1.0. `flags` = record header flags of the winning
  definition; `initially_disabled` = bit 0x800, `deleted` = bit 0x20. `persistent` = the
  winning definition is in a type 8 group.
- `enable_parent` = XESP as `{"ref": formid, "opposite": bool (flag bit 0)}`, else `null`.
- Cell fields: `water_height` = XCLW (`null` if absent), `grid` = XCLC (x, y) or `null`,
  `interior` = DATA bit 0, `refs` = count, `land` = the cell's LAND FormID or `null`.

Lab comparator: **keyed** on `formid` for `/refs` (all fields), **exact** on `/cell`.

## `fnvtool world heights ... --world <edid|formid>` → `fnv.world.heights/1`

```json
{
  "schema": "fnv.world.heights/1",
  "world": "0x000DA726",
  "cells": [
    {"grid": [-18, 0], "min": 4012.0, "max": 4410.0, "mean": 4201.37,
     "corners": [4012.0, 4100.0, 4300.0, 4410.0], "centre": 4200.0}
  ]
}
```

One entry per exterior grid cell of the worldspace whose LAND has VHGT, sorted by
(x, y). The worldspace's persistent cell (the CELL directly in the world-children group)
is not a grid cell. Heights are the 33 × 33 integrated VHGT values in world units
(world.md §3); `corners` = [south-west, south-east, north-west, north-east] =
h[0][0], h[0][32], h[32][0], h[32][32] with h[row][col], rows south → north;
`centre` = h[16][16]; `mean` is the mean of all 1089 values.

Lab comparator: **keyed** on `grid` for `/cells` (all fields, float tolerance 1e-3).

## Other world commands (not yet scenarios)

- `world list` → `fnv.world.list/1`: per worldspace FormID, EDID, name, parent, persistent
  cell, cell / grid-cell / land-cell / reference counts, grid bounds, default heights;
  interior cell count; orphan groups; unreadable records.
- `world cells [--world W]` → `fnv.world.cells/1`: the cell objects above, for one
  worldspace or (without `--world`) all interior cells.
- `world land --world W --x X --y Y [--full]` → `fnv.world.land/1`: the 33 × 33 heights
  (rows south → north); with `--full` also normals (signed), colours and texture layers.
- `world check [--world W]` → `fnv.world.check/1`: the observational checks behind
  world.md (cell membership of references, shared LAND edges, normals against heights,
  base object types).
