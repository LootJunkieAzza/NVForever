# Spec note: `.nvse` co-save binary format (xNVSE, JIP LN, JohnnyGuitar, ShowOff)

Clean-room binary specification of the `.nvse` co-save file written beside a `.fos`
save by *Fallout: New Vegas* (`1.4.0.525`) with xNVSE and extender plugins, used by
the offline `fnvtool save inspect-fos` and `fnvtool save convert` utilities
(`crates/fnv-save/src/nvse.rs`, `crates/fnv-game/src/fos_convert.rs`, issue #4747).
The NVForever game runtime remains strictly `.nvfsave`-only (`docs/design/save-load.md`
§5.7, `docs/design/extender-compat.md` §2).

## Clean-room provenance

Every binary structure below tagged `[observed]` was established exclusively by
controlled single-variable A/B `.fos` + `.nvse` captures and byte-level diffing:

- **Date:** 2026-11-26.
- **Game and extender versions:** *Fallout: New Vegas* `1.4.0.525` (`040020D0`,
  English) with only the 10 official base masters (`FalloutNV.esm`, `DeadMoney.esm`,
  `HonestHearts.esm`, `OldWorldBlues.esm`, `LonesomeRoad.esm`, `GunRunnersArsenal.esm`,
  `CaravanPack.esm`, `ClassicPack.esm`, `MercenaryPack.esm`, `TribalPack.esm`),
  `xNVSE 6.4.9`, `JIP LN NVSE 57.30` (`5730`), `JohnnyGuitarNVSE 528`, and
  `ShowOffNVSE Plugin 184` (zero gameplay/UI mods; minimal console-output redirect for
  `PrintToConsole` logs).
- **Staging copy and isolation (`tools/capture/README.md` §10):** Runs executed inside
  an isolated staging copy of the game directory created from clean vanilla masters and
  archives via hardlinks/junctions (never treated as the real install), with
  `SLocalSavePath` pointed to an isolated probe saves subfolder, all host INI and
  load-order files restored byte-for-byte with SHA-256 verification after each run, and
  all probe scripts and temporary probe saves cleaned up afterward.
- **Committed probes and verifier:** Declarative probe steps live in
  `tools/capture/nvse_cosave/campaigns.py`, the independent pure-Python parser/differ in
  `tools/capture/nvse_cosave/parser.py`, and the automated byte-level assertion matrix
  in `tools/capture/nvse_cosave/verify.py`.
- **Clean-room rule (`docs/design/extender-compat.md` §4):** No xNVSE, JIP LN,
  JohnnyGuitar, ShowOff, or other extender C++ source code, headers, or DLL binaries
  were consulted, disassembled, or decompiled. No private playthrough saves were cited.

All integer fields are little-endian unless stated otherwise. All strings are
unterminated ASCII/CP1252 byte sequences prefixed by an explicit byte length.

---

## 1. File container

### 1.1 File header (20 bytes) `[observed: cosave_core_jip c00_base]`

| Offset | Type | Field | Observed value (xNVSE 6.4.9, FNV 1.4.0.525) |
|---:|---|---|---|
| `0x00` | `[u8; 4]` | `magic` | `b"NVSE"` (`4E 56 53 45`) |
| `0x04` | `u32` | `format_version` | `1` |
| `0x08` | `u32` | `nvse_version` | `0x00040006` (xNVSE 6.4.9) |
| `0x0C` | `u32` | `runtime_version` | `0x040020D0` (FNV 1.4.0.525) |
| `0x10` | `u32` | `num_plugins` | `2` (baseline: xNVSE + JIP), `3`–`4` when JG / ShowOff emit chunks |

### 1.2 Plugin block header (12 bytes + `length` bytes) `[observed: c00_base, c10_showoff_strmap, c25_jg_extramisc]`

Repeated `num_plugins` times immediately after offset `0x14`:

| Offset | Type | Field | Notes |
|---:|---|---|---|
| `0x00` | `u32` | `opcode` | Plugin identifier / opcode base |
| `0x04` | `u32` | `num_chunks` | Number of chunk records inside this plugin block |
| `0x08` | `u32` | `length` | Total byte length of the `num_chunks` chunk records (including their 12-byte headers) |

Observed plugin identifiers:

| Opcode (`u32`) | Owner | Observed in |
|---|---|---|
| `0x00001400` | xNVSE core | `c00_base`–`c41_long_and_empty_str` (all 3 committed campaigns) |
| `0x00002200` | JIP LN NVSE | `c00_base`–`c41_long_and_empty_str` (all 3 committed campaigns) |
| `0x00003100` | JohnnyGuitarNVSE | `c25_jg_extramisc`, `c26_showoff_auxtimer` |
| `0x00003C93` | ShowOffNVSE Plugin | `c10_showoff_strmap`, `c12_created_0xff`, `c26_showoff_auxtimer` |

### 1.3 Chunk record header (12 bytes + `length` bytes) `[observed: all captures]`

Repeated `num_chunks` times inside each plugin block:

| Offset | Type | Field | Notes |
|---:|---|---|---|
| `0x00` | `[u8; 4]` | `tag` | 4-byte chunk identifier (raw on-disk byte order; note some plugins write FourCCs little-endian such as `b"SDOM"` for `'MODS'` while others write forward ASCII such as `b"JPAV"`) |
| `0x04` | `u32` | `version` | Per-chunk format version |
| `0x08` | `u32` | `length` | Payload byte length immediately following the 12-byte header |

---

## 2. xNVSE core chunks (`opcode = 0x00001400`)

### 2.1 `b"SDOM"` (`'MODS'`, version `0`) — Load-order plugin list `[observed: c00_base]`

| Field | Type | Notes |
|---|---|---|
| `num_mods` | `u8` | `10` in the 10-master staging captures |
| `mods[num_mods]` | `u16 len` + `[u8; len]` | Plugin filenames in load-order index order (`00 FalloutNV.esm` .. `09 TribalPack.esm`) |

### 2.2 `b"SVRA"` / `b"RVRA"` / `b"EVRA"` (`'ARVS'` / `'ARVR'` / `'ARVE'`, version `2`) — Array variables `[observed: c02_arr_list_inline, c03_arr_map_inline, c04_arr_strmap_nested, c12_created_0xff, c41_long_and_empty_str]`

- `b"SVRA"` (version `2`, length `0`) opens the array section; `b"EVRA"` (version `2`,
  length `0`) closes it.
- Zero or more `b"RVRA"` (version `2`) chunks appear between them, one per live array:

| Field | Type | Notes |
|---|---|---|
| `owning_mod` | `u8` | Owning plugin mod index (`0xFF` for runtime-compiled scripts) |
| `array_id` | `u32` | Non-zero array ID referenced by script variables and nested array elements |
| `key_type` | `u8` | `1` = numeric key (`List` or `Map`), `3` = string key (`StringMap`) |
| `packed` | `u8` | `1` = packed `List` (`array`, implicit `0..N-1` keys), `0` = associative `Map` (`key_type == 1`) or `StringMap` (`key_type == 3`) |
| `num_refs` | `u32` | Reference count (`1` for standalone `aList`/`aMap`/`aSMap`; `2` for `aChild` held by both local var and `aSMap["kArr"]` in `c04_arr_strmap_nested`) |
| `ref_mods` | `[u8; num_refs]` | Mod index of each holder (`[0xFF]`, `[0xFF, 0xFF]` observed) |
| `num_elements` | `u32` | Number of elements following |

Each of the `num_elements` entries consists of:
- **Key** (omitted when `packed == 1`, where keys are implicit `0, 1, ..., num_elements - 1`):
  - `packed == 0, key_type == 1` (`Map`): `f64` key (observed: `-5.5`, `10.0`, `99.0` in `c03_arr_map_inline`).
  - `packed == 0, key_type == 3` (`StringMap`): `u16 len` + `[u8; len]` key (observed: `"kArr"`, `"kNum"`, `"kRef"`, `"kStr"` in `c04_arr_strmap_nested`).
- **Value:** `u8 val_type` + payload:
  - `1` (Number): `f64` (observed: `42.5`, `100.25`, `-12.75`, `777.0`).
  - `2` (FormID): `u32` (observed: `0x00000014`, `0x00104C0F`, `0x000E3778`, and created `0xFF001321` in `c12_created_0xff`).
  - `3` (String): `u16 len` + `[u8; len]` (observed: `"AlphaStr"`, `"MapVal"`, `"NestedHello"`, 270-byte string with `0e 01` = `270`, and empty string `""` with `00 00` in `c41_long_and_empty_str`).
  - `4` (Nested array): `u32` child `array_id` (observed: `1` pointing to `aChild` in `c04_arr_strmap_nested`).

### 2.3 `b"SVTS"` / `b"EVTS"` (`'STVS'` / `'STVE'`, version `0`) — String variable markers `[observed: c00_base, c01_str_inline, c01b_str_after]`

- `b"SVTS"` (version `0`, length `0`) and `b"EVTS"` (version `0`, length `0`) bracket
  the string-variable section.
- **Negative observation (`c01_str_inline`, `c01b_str_after`):** Creating two live
  `string_var`s (`sv_Construct "HelloNVSE"` id `2`, `sv_Construct "Second_12345"` id
  `3`) inside a `0xFF` runtime-compiled UDF and calling `con_Save` synchronously while
  they are in scope emitted **0 inner chunks** between `SVTS` and `EVTS` (byte-identical
  302-byte `.nvse` to `c00_base`), whereas an `array_var` created in the same way in
  `c02_arr_list_inline` emitted an `RVRA` chunk. Because no non-empty inner string
  chunk was observed in the 10-master runtime probes, NVForever's reader does not
  speculate on an unobserved inner string-record layout; any non-empty chunk between
  `SVTS` and `EVTS` is counted truthfully in `skipped`.

### 2.4 Negative proof: `SetModLocalData` `[observed: c20_base vs c22_modlocaldata]`

Setting `SetModLocalData` entries (number, string, ref) produced a 302-byte `.nvse`
byte-for-byte identical to `c20_base`: `ModLocalData` is session-only and is not
written to `.nvse`.

---

## 3. JIP LN NVSE chunks (`opcode = 0x00002200`)

### 3.1 `b"JPAV"` (version `10`) — Permanent Auxiliary Variables `[observed: c05_jip_aux_flt, c06_jip_aux_mix_priv, c12_created_0xff]`

Temporary (`*`-prefixed) auxiliary variables (such as the probe loop's `*cp_*`
variables) are never written. Permanent variables (plain private and `_`-prefixed
public) are encoded as:

```text
u16 num_mods
for _ in 0..num_mods:
    u8  mod_index              // 0xFF for runtime/public-from-runtime, or 0x00..0xFE owning plugin
    u16 num_owners
    for _ in 0..num_owners:
        u32 owner_form_id      // e.g., 0x00000014, 0x00104C0F, or created 0xFF001321
        u16 num_vars
        for _ in 0..num_vars:
            u8  name_len
            [u8; name_len] name
            u16 num_elems
            for _ in 0..num_elems:
                JipVal         // u8 type_tag + payload (below)
```

`JipVal` encoding (shared by `JPAV`, `JPRM`, and ShowOff `OSMS`) `[observed: c05, c06, c07, c10, c12]`:

| `type_tag` (`u8`) | Type | Payload |
|---:|---|---|
| `1` | Number | `f64` (`8` bytes) |
| `2` | FormID | `u32` (`4` bytes; includes `0xFF` created refs as in `c12_created_0xff`) |
| `4` | String | `u16 len` + `[u8; len]` |

### 3.2 `b"JPRM"` (version `10`) — Permanent RefMap Arrays `[observed: c07_jip_refmap, c12_created_0xff]`

```text
u16 num_mods
for _ in 0..num_mods:
    u8  mod_index
    u16 num_maps
    for _ in 0..num_maps:
        u8  name_len
        [u8; name_len] name
        u16 num_entries
        for _ in 0..num_entries:
            u32    key_form_id
            JipVal value
```

**Observed key normalization (`c07_jip_refmap`, `c12_created_0xff`):** When
`RefMapSet*` is called with placed actor/item references, JIP LN stores the resolved
base form ID as `key_form_id`: `Player` (`0x00000014`) was stored as `0x00000007`,
`DocMitchellREF` (`0x00104C0F`) as `0x00104C0C`, and created `PlaceAtMe` pen
(`0xFF001321`) as `0x0005B63C`, while base form `WeapNV9mmPistol` (`0x000E3778`) stayed
`0x000E3778`.

### 3.3 `b"JPLR"` (version `9`) — Linked References (`SetLinkedReference`) `[observed: c23_jip_linkedref, c35_jplr_multi_and_0xff]`

```text
u16 num_links
for _ in 0..num_links:
    u32 ref_form_id            // placed or 0xFF created reference
    u32 linked_ref_form_id     // target linked reference
    u8  mod_index              // owning plugin mod index (0xFF from runtime)
```

Observed: `c23_jip_linkedref` (`1` entry, `11` bytes: `0x00104C0F -> 0x001062AC`,
mod `0xFF`) and `c35_jplr_multi_and_0xff` (`2` entries, `20` bytes: adding created
`0xFF001319 -> 0x00104C0F`, mod `0xFF`).

### 3.4 `b"JPSV"` (version `9`) — Runtime-Added Script Variables (`AddScriptVariable`) `[observed: c24_jip_addscriptvar, c31_jpsv_ref_and_multi]`

```text
u16 num_owners
for _ in 0..num_owners:
    u32 owner_form_id          // quest (e.g. 0x00080664 VMS01) or reference (0x00104C0F)
    u16 num_vars
    for _ in 0..num_vars:
        u8  mod_index          // owning plugin mod index (0xFF from runtime)
        u8  name_len
        [u8; name_len] name
        [u8; 8] raw_value      // 8-byte script-variable union (below)
```

**Observed 8-byte `raw_value` union (`c24_jip_addscriptvar`, `c31_jpsv_ref_and_multi`):**
- Numeric values (`SetVariable`): IEEE 754 `f64` LE (`654.25` -> `00 00 00 00 00 72 84 40`;
  `-12.5` -> `00 00 00 00 00 00 29 c0`; `0.0` -> eight zero bytes).
- Form/reference values (`SetRefVariable`): `u32` FormID LE in bytes `0..4` with bytes
  `4..8` all zero (`0x001062AC` -> `ac 62 10 00 00 00 00 00`).

### 3.5 `b"JPGF"` (version `1`, 8 bytes) — Unmapped baseline chunk `[observed: c00_base]`

Emitted by JIP LN on every save (`00 00 00 00 00 00 80 3f`, `8` bytes). Its semantic
fields were not isolated by an A/B mutation and have no `GameSnapshot` target, so it is
counted in `skipped` as unmapped.

---

## 4. JohnnyGuitarNVSE chunks (`opcode = 0x00003100`)

### 4.1 `b"SMGJ"` (`'JGMS'`, version `1`) — Extra Misc Stats (`InitExtraMiscStat` / `ModExtraMiscStat`) `[observed: c25_jg_extramisc]`

```text
u16 num_stats
for _ in 0..num_stats:
    u16 name_len
    [u8; name_len] name
    i32 value                  // signed 32-bit LE (observed: 17 and -5)
```

---

## 5. ShowOffNVSE Plugin chunks (`opcode = 0x00003C93`)

### 5.1 `b"OSMS"` (`'SMSO'`, version `10`) — Permanent `AuxStringMapArray` `[observed: c10_showoff_strmap, c41_long_and_empty_str]`

```text
u16 num_mods
for _ in 0..num_mods:
    u8  mod_index
    u16 num_maps
    for _ in 0..num_maps:
        u8  name_len
        [u8; name_len] name
        u16 num_entries
        for _ in 0..num_entries:
            u8     key_len
            [u8; key_len] key
            JipVal value       // same type_tag 1/2/4 encoding as §3.1
```

### 5.2 `b"SOAT"` (version `1`) — Persistent `AuxTimer` `[observed: c26_showoff_auxtimer]`

```text
u16 num_mods
for _ in 0..num_mods:
    u8  mod_index
    u16 num_owners
    for _ in 0..num_owners:
        u32 owner_form_id
        u16 num_timers
        for _ in 0..num_timers:
            u8  name_len
            [u8; name_len] name
            f64 time_a         // observed: 45.5, 120.0 (initial countdown / remaining [hypothesis on order])
            f64 time_b         // observed: 45.5, 120.0
            u32 flags          // observed: 0 (flag 0) and 4 (flag 4)
```

*(Note: in `c26_showoff_auxtimer`, both `time_a` and `time_b` held equal values
(`45.5` for `cv_tmr_priv` and `120.0` for `_cv_tmr_pub`), so distinguishing which of
`time_a`/`time_b` is initial countdown vs remaining time stays `[hypothesis]`.
`GameSnapshot` does not yet persist ShowOff aux timers, so `SOAT` is decoded for
`inspect-fos` and counted in `skipped` during `convert`.)*

---

## 6. Offline converter mapping (`[design]`)

The following decisions are NVForever's own offline importer choices (`[design]`, not
original-game constraints):

1. **Mod-list remapping:** Form IDs inside `.nvse` chunks remap through the `.nvse`
   `SDOM` mod list (falling back to the `.fos` plugin list if `SDOM` is absent) onto the
   target load order; `0xFF` created references remap through the `.fos` converter's
   `created_map` (and are skipped if no matching created `.fos` record was placed).
2. **Private vs. public JIP variable keys:** Names starting with `_` map to their
   lower-cased name (capped at 79 chars, matching `crates/fnv-script/src/vm/jip.rs`).
   Private names (not starting with `_` or `*`) with `mod_index == 0xFF` map to their
   lower-cased name; those with `mod_index < 0xFF` resolve the saved plugin filename from
   `SDOM` and, if that plugin is loaded in the target load order, map to
   `"<name>\u{1}<plugin_lower>"` (or are skipped when the owning plugin is absent).
3. **Imported xNVSE array IDs and refcounts:** Imported `RVRA` arrays keep their saved
   `array_id` via `ExtStore::insert_imported_array` (advancing `ExtStore::next` past the
   highest imported ID) with `refs = ref_mods.len().max(1)` so saved script variables
   holding those array IDs stay valid without being swept prematurely; arrays owned by an
   unloaded plugin (`owning_mod < 0xFF`) are skipped.
4. **`JPSV` 8-byte union classification:** Non-zero `low32` with `high32 == 0` is
   decoded as `FormId(low32)` and remapped; all other finite values decode as `f64`.
   Added script variables are attached via `Game::instance_of(owner)` into
   `jip10.added_vars` and `instances` at `ADDED_VAR_BASE + index`.
5. **JohnnyGuitar `SMGJ` keys:** Inserted into `Game::jip2.misc_stats`
   (`GameSnapshot::extra_misc_stats`) under `name.to_ascii_lowercase()`, matching
   `crates/fnv-game/src/nvse_ttw.rs`.
6. **Fail-soft handling:** Corrupt/unparseable adjacent `.nvse` files do not abort `.fos`
   conversion (they set `cosave_ignored: true` and emit a warning). Unknown plugin
   opcodes, unknown chunk tags/versions, `JPGF`, and decoded chunks whose target state
   is not yet persisted in `GameSnapshot` (`JPLR`, `SOAT`) are counted in `skipped`.
