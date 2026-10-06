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
