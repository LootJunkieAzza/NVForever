# Save files (`.fos`): layout as observed

Clean-room notes on the original game's save file, written to plan NVForever's own save
and load system ([design/save-load.md](../design/save-load.md)). The game runtime itself
does not read or write `.fos` files; this note says what the original keeps, so our save
keeps the same state. `fnv_save::fos` reads the observed/documented sections below for the
extender lab (`fnvtool ext queries --save` and `ext probes --save`, §6.4–§6.5, issue #1023)
and the offline converter (`fnvtool save inspect-fos` and `fnvtool save convert`, §6.6). Tags: [documented] (public page named), [observed] (bytes in the saves below),
[hypothesis] (a reading that fits the bytes but is not proven). Offsets are byte offsets
in the file, little-endian unless stated.

No save bytes, dumps or screenshots are committed. The working copies and the parser used
for this note are in `C:\fnv-private\save-research\` (private).

## 1. The saves looked at

`C:\fnv-private\capture\saves-backup\` holds 27 folders, named by the Unix time at which
`tools/capture/capture_run.py` backed up the game's rolling saves (`autosave.fos`,
`autosave.fos.bak`, `autosave.nvse`) before a capture run (tools/capture/README.md). The
27 folders hold only **four different `.fos` files** [observed, MD5 of every file]:

| Name here | File (first folder holding it) | Size | Written by | Header place / play time |
|---|---|---|---|---|
| A | `1791540675\autosave.fos.bak` | 1,860,200 | the owner's game, 2026-10-09 05:43 (owner-review.md, "Read first") | Goodsprings, 0:11:58, save number 15 |
| B | `1791540675\autosave.fos` | 1,776,091 | VCG01 stage 55's scripted `Autosave`, capture round 4 from save `nvfnew` (layer5-captures.md §10) | Doc Mitchell's House, 0:01:50, save number 16 |
| C | `1791556130\autosave.fos` | 1,854,167 | capture `pc_drop_fall`, loaded `nvfdoc`, walked out of Doc Mitchell's door (owner-review.md; `run_pc_drop_fall.json`) | Goodsprings, 0:11:50, save number 16 |
| D | `1791556382\autosave.fos` | 1,854,167 | capture `pc_push`, loaded `nvfdoc`, same door (`run_pc_push.json`) | Goodsprings, 0:11:51, save number 16 |

A fifth save, the owner's **"max save"** (`saves-backup\Max Save\`, called M here), is a
late full playthrough: save number 292, level 50, 61:01:36 play time, 8,050,402 bytes,
file dated 2013 [observed, header and file time]. It is used for the range of record types and sizes (§6.3).

C and D start from the same save and leave by the same door a few real minutes apart, so
their difference shows what moves with a little game time (§7). A and C share the same
form ID array and changed-form list (§6), so A is from the same line of play [observed].
B is a new game, before the player ever entered an exterior worldspace (§5.3).

The xNVSE co-save (`autosave.nvse`) is ignored: NVForever has no use for it.

Controlled A/B save-pair observations (`[observed: vanilla 1.4.0.525 A/B save pairs]` and
`[observed: vanilla 1.4.0.525 + clean-room test-plugin A/B save pairs]`) come from a
separate 261-save clean-room probe set:

| Field | Details |
|---|---|
| Install, owner, and date | Contributor's own clean English Steam/GOG `Fallout: New Vegas` `1.4.0.525` install; 261 A/B probe saves captured `2026-11-23` |
| Load order | Only the 10 official base masters (`FalloutNV.esm`, `DeadMoney.esm`, `HonestHearts.esm`, `OldWorldBlues.esm`, `LonesomeRoad.esm`, `GunRunnersArsenal.esm`, `CaravanPack.esm`, `ClassicPack.esm`, `MercenaryPack.esm`, `TribalPack.esm`); zero gameplay, UI, or tweak mods |
| Script runner and versions | `xNVSE 6.4.9` + `JIP LN NVSE 57.30`, used solely as an in-process script runner to dispatch single-variable vanilla console/script commands and `con_Save` immediately before and after each change (pure black-box observation; no binary or DLL inspection) |
| Temporary clean-room synthetic test plugin | A temporary `.esp` constructed strictly from public TES4/FNV binary record format specifications to test form shapes absent from `FalloutNV.esm` (`XGLB` ownership global `0x22`, carryable `LIGH` `0x26`, charged `WEAP` `0x28`, dual-slot `ARMO` `0x17`, and persistent/scaled `REFR` `0x1C`/`0x30`), loaded only for those isolated probes and removed immediately after capture |
| Tools that read the saves | `fnvtool save inspect-fos` and an offline Python standard-library `.fos` parser and byte-diff verifier |

## 2. Top-level layout

The file is a sequence of sections; a table after the plugin list gives the absolute
offsets of the later ones [observed, all four saves; offsets for C]:

| Section | C offset | Notes |
|---|---|---|
| Magic `FO3SAVEGAME` (11 bytes, no terminator) | 0 | [observed]; the `.fos` extension is shared with Fallout 3 [documented: fileinfo.com ".FOS File Extension", no layout given] |
| Header size (u32) | 0x0B | counts the bytes from 0x0F to the start of the screenshot |
| Header (§3) | 0x0F | fields each followed by 0x7C |
| Screenshot (§3.2) | 0x0F + header size | width × height × 3 bytes |
| Form version byte, plugin block (§4) | after the screenshot | |
| File location table (§5) | after the plugins | 8 × u32 then 78 zero bytes |
| Global data table 1 (§5.1) | table[2] | 12 typed entries |
| Changed forms (§6) | table[3] | `changeFormCount` records |
| Global data table 2 (§5.2) | table[4] | 1 entry |
| Form ID array (§5.3) | table[0] | u32 count, then u32 form IDs |
| Worldspace array (§5.3) | right after the form ID array | u32 count, then u32 form IDs |
| Tail (9 bytes) | table[1] | `05 00 00 00 00 00 00 00 7C` in all four saves; meaning unknown |

The file ends exactly after the tail [observed: table[1] + 9 = file size in all five].

**The `|` byte.** Nearly every field in the header, the globals and the changed-form data
is followed by `0x7C` (`|`) [observed]. It carries no data: a reader skips it after each
field. The Oblivion (`TES4SAVEGAME`) and Skyrim layouts on UESP have no such separators
[documented: UESP "Tes4Mod:Save File Format", "Skyrim Mod:Save File Format"]; the rest of
the structure is close to Skyrim's (§5), which is the closest public description.

## 3. Header

### 3.1 Fields [observed, all four saves]

From 0x0F, each field followed by a 0x7C byte:

| Field | Encoding | Values seen |
|---|---|---|
| unknown | u32 | 0x30 in all four [hypothesis: header/format version] |
| language | 64 bytes, NUL-padded ASCII | `ENGLISH` |
| screenshot width | u32 | 512 |
| screenshot height | u32 | 288 |
| save number | u32 | 15 (A), 16 (B, C, D) |
| player name | u16 length, 0x7C, bytes | empty in B (a new game; the name is set in VCG01) [observed] |
| karma title | u16 length, 0x7C, bytes | "Drifter" |
| level | u32 | 1 |
| location name | u16 length, 0x7C, bytes | the cell's or worldspace region's display name |
| play time | u16 length, 0x7C, ASCII `HHH.MM.SS` | `000.11.50` |

Strings are length-prefixed (u16), a `|`, then the bytes without a terminator; non-empty
strings are followed by a trailing `|`, while zero-length strings (such as an unnamed
new-game player, empty in B) are read as `00 00 7C` with no second `|` [hypothesis]. The header
size therefore varies with the strings (137 bytes in A/C/D, 141 in B) [observed]. The
header holds no game date and no plugin list; the load menu's line is built from it alone
[hypothesis].

### 3.2 Screenshot

Width × height × 3 bytes, uncompressed, no padding [observed: 512 × 288 × 3 = 442,368
bytes ends exactly where the form-version byte starts]. Channel order RGB [observed: vanilla
1.4.0.525 A/B save pairs: warm amber interior mean (68.4, 50.5, 29.6) and Mojave exterior
mean (126.0, 111.4, 83.3) show channel 0 > channel 1 > channel 2].

## 4. Form version and plugins [observed]

- u8 form version: 0x1B (27). Every changed form carries the same version byte (§6).
- u32 size of the plugin block (199 in all four), then:
- u8 plugin count (10), `|`, then per plugin: u16 length, `|`, name, `|`.

M lists nine (no `GunRunnersArsenal.esm`) in a different order (`FalloutNV.esm`,
`HonestHearts.esm`, `TribalPack.esm`, …) [observed], so the list is the load order the
save was made with, not a fixed list. The ten names in A–D are the base game masters in load order (`FalloutNV.esm`, the four story
DLCs, `GunRunnersArsenal.esm`, the four packs). A plugin's position is its mod index in
the form IDs that follow [hypothesis, consistent with every form ID resolving to a record
of the expected type in that master, §6].

## 5. File location table and global data

Right after the plugin block: eight u32 values, then 78 zero bytes up to the first global
entry [observed]. Matched against Skyrim's table [documented, UESP "Skyrim Mod:Save File
Format": six offsets, four counts, `unused[15]`], the New Vegas table is shorter:

| # | Meaning | C value | How it was matched |
|---|---|---|---|
| 0 | form ID array offset | 1,818,058 | points at a u32 count followed by that many form IDs [observed] |
| 1 | tail offset | 1,854,158 | equals the end of the worldspace array [observed] |
| 2 | global data table 1 offset | 442,834 | first typed entry starts there [observed] |
| 3 | changed forms offset | 445,396 | = end of table 1; the record walk starts there [observed] |
| 4 | global data table 2 offset | 1,818,050 | = end of the record walk [observed] |
| 5 | table 1 count | 12 | 12 entries tile table 1 exactly [observed] |
| 6 | table 2 count | 1 | |
| 7 | changed form count | 4,220 (A, C, D), 3,885 (B) | walking this many records lands exactly on offset 4 [observed] |

A global entry is u32 type, u32 length, then `length` bytes [observed, same as Skyrim's
documented layout].

### 5.1 Global data table 1 (types 0–11) [observed unless tagged]

| Type | Length (C) | Contents |
|---|---|---|
| 0 | 220 | (every value below is followed by 0x7C) u32 count 43, 43 × (u32). All zero in B (new game); first two are 1 in A/C/D. The misc statistics by code: the values of 31 saves fit the names (misc-stats.md §1) [observed; also observed: vanilla 1.4.0.525 A/B save pairs: all 43 zero at start; ingesting chems increments indices 7 and 9; entering Goodsprings increments index 1; killing an NPC increments indices 2 and 35] |
| 1 | 57 | **Player location and created-form counter**: u32 next created form ID (0xFF001313 in C, 0xFF001309 in B, 0xFF001317 in A); ref of the current worldspace (WastelandNV in C, a dummy worldspace FFEncounterWorld in B, which is indoors); two i32 cell grid coordinates (−18, 0 in C); ref of the worldspace or interior cell the player is in (GSDocMitchellHouse in B); three f32 position (no separator between them); ref of the active quest shown in the Pip-Boy (VCG01 in B, VCG02 in C) [observed: vanilla 1.4.0.525 A/B save pairs: `None` at start; `StartQuest 0x0010A214` (`VCG02`) + `SetObjectiveDisplayed` sets it to `0x0010A214`; `SetCurrentQuest 0x00080664` (`VMS01`) updates it to `0x00080664`]; u32, f32, u32 unknown |
| 2 | 107 | `vsval` count, `\|`, then `count × (ref, \|, u16, \|)`, followed by 105 unmapped trailing bytes (`count = 0` in A–D: `00 7C` + 105 bytes = 107; `count = 603` in M, §6.3 [observed: A–D, M]; per-base-form death counts [observed: vanilla 1.4.0.525 A/B save pairs: killing Doc Mitchell (base 0x00104C0C) transitions count 0 -> 1 with (0x00104C0C, 1); killing a spawned Powder Ganger (base 0x0015F312) records (0x0015F312, 1)]; the 105 unmapped trailing bytes remain `[hypothesis]`/unmapped) |
| 3 | 1803 | **Global variables**: vsval count (200), then per global: ref and f32, each followed by 0x7C. 199 resolve to GLOB records, one to form 0x63 (no record in the masters [hypothesis: a hard-coded global]). Game time is here: GameHour, GameDaysPassed, GameDay/Month/Year, TimeScale (30) |
| 4 | 30 | three f32 and six bytes; the third float moved between C and D (10.388 → 10.402); meaning unknown |
| 5 | 36 | u32 1, flags, floats; two floats moved between C and D; meaning unknown |
| 6 | 33 | u32 1003, small values; identical in all four |
| 7 | 2 | one byte (0) |
| 8 | 71 | **Sky and weather**: ref current weather, ref second weather (NVWastelandGS and NVWastelandClear in C; WastelandClear twice in B), then f32 values; two of them equal GameHour exactly (9.4697 in C) [observed: matches the GameHour global; also observed: vanilla 1.4.0.525 A/B save pairs: `ForceWeather 0x000FFC88` changes the first ref from null to `0x000FFC88`]; the rest unknown |
| 9 | 7 | u32 0 and a byte |
| 10 | 96 (C), 80 (B) | a vsval-counted list of refs (18 in C, 14 in B) [observed]; Pip-Boy radio and music playback state: opening `u32, \|, u8, \|, string song_path, \|, vsval n1, \|, n1 × (ref, \|)` (remains zeroed when Pip-Boy radio is toggled; non-zero header fallback `[hypothesis]`), immediately followed by a second `vsval`-counted active-station list holding `ref3 station, \|, u32 playback_ms, \|, u32, \|, u8, \|, u8, \|, u32, \|, u8, \|, u8, \|` [observed: vanilla 1.4.0.525 A/B save pairs: `PipBoyRadio On 0x0014DF03` adds 27 bytes at the second `vsval` list with `ref3 0x0014DF03` and `playback_ms` 9740 advancing to 29077 twenty-five ticks later while the first four header fields stay 0] |
| 11 | 4 | one ref (resolves to a SOUN record in C) [hypothesis: unknown; the resolution may be meaningless] |

### 5.2 Global data table 2

One entry, type 1000, length 0, in all four saves [observed].

### 5.3 Form ID array and worldspace array

- Form ID array: u32 count, then count × u32 form IDs (9,022 in A/C/D, 7,237 in B). Most
  are references (ACHR, REFR, ACRE) and their bases (NPC_, CREA), packages, quests,
  scripts, navmeshes, globals, cells [observed: resolving each against the masters].
  No entry has mod index 0xFF in any of the four.
- Worldspace array: u32 count, then form IDs. C has one (WastelandNV, 0x000DA726); B has
  none: the new-game player had not been outdoors [observed]. [hypothesis: the worldspaces
  the player has visited; Oblivion's page calls its equivalent of unknown purpose,
  documented, UESP Tes4Mod].

### 5.4 Refs and vsval

- **Ref** (3 bytes, big-endian): the top 2 bits are a kind, the low 22 bits a value.
  Kind 0: value is a 1-based index into the form ID array (0 = none); kind 1: a form ID in
  `FalloutNV.esm` directly; kind 2: a created form (0xFF000000 | value) [documented for
  Skyrim, UESP "Skyrim Mod:Save File Format"; observed for New Vegas: index − 1 makes every
  changed form resolve to a record of the type its record type says, §6, while the plain
  index does not]. Kind 1 was seen (e.g. world ref `00 01 95`), kind 2 was not.
- **vsval** (variable-size count): the low 2 bits of the first byte give the width (0 = 1
  byte, 1 = 2 bytes, 2 = 4 bytes), the value is the little-endian number shifted right by
  2 [hypothesis: fits every count seen: `21 03` = 200 globals, `98` = 38 quest stages,
  `04` = 1 item, `1C` = 7 variables].

## 6. Changed forms

Each record [observed: the walk tiles the section exactly in all four saves]:

| Field | Size | Notes |
|---|---|---|
| ref | 3 | §5.4 |
| change flags | u32 | which kinds of change follow |
| type | u8 | low 6 bits: record type; high 2 bits: width of the length field (0 = u8, 1 = u16, 2 = u32) |
| version | u8 | 27 in every record (= the form version byte, §4) |
| length | 1/2/4 | data size |
| data | length | the changes, in a fixed order; fields are each followed by 0x7C |

Skyrim's documented header has a second length (uncompressed size); New Vegas has only
one and no compression [observed].

The data has no per-field tags: which fields are present follows from the change flags and
the record type, as in Oblivion [documented, UESP Tes4Mod: "Bit order does not match the
order of subrecords", flags say which subrecords are present]. This note decodes the parts
needed to plan our own save; the rest is listed as unknown.

### 6.1 Types seen [observed, C; B has the same types]

Type numbers are matched by resolving each record's ref against the masters:

| Type | Record | Count C (B) | Change flag bits seen |
|---|---|---|---|
| 0 | REFR (placed objects) | 317 | 0, 1, 2, 4, 5, 12, 27, 28, 29, 31 |
| 1 | ACHR (placed NPCs, and the player ref 0x14) | 2,418 | 0, 1, 4, 5, 11, 18, 20, 21, 27, 28, 29, 31 |
| 2 | ACRE (placed creatures) | 900 | 0, 1, 5, 10, 11, 18, 20, 27, 28, 29, 31 |
| 7 | CELL | 15 | 1, 29, 30, 31 |
| 8 | INFO (dialogue responses) | 52 (13) | 31, always with no data |
| 9 | QUST | 466 | 1, 2, 29, 30, 31 |
| 10 | NPC_ (only the player base, 0x7) | 1 | 2, 5 |
| 11 | CREA | 1 | 1 |
| 33 | CLAS (PlayerClass) | 1 | 1 |
| 34 | FACT | 4 | 2 |
| 37 | FLST (form lists) | 41 | 31 |
| 40 | LVLI (leveled items) | 1 | 31 |
| 50 | CHAL (challenges) | 3 | 1 |

Not seen in A–D: created forms (kind-2 refs); M has them (§6.3).

Not every actor is listed: the masters hold 3,942 ACHR and 3,739 ACRE, the saves 2,418 /
900 (C) and 2,406 / 893 (B) [observed], so the original keeps a record for a large fixed
set of actors (most of them untouched by the player) [hypothesis: the persistent ones].

### 6.2 What is decoded per type

**QUST** [observed: VCG01 in B is at stage 55 per its capture; its record shows stages up
to 55 done and 60 not done]:

- flag bit 1: one byte of quest flags (`0x31` in B where VCG01 is running, `0x32` in C; `0x21`
  when running) [observed: vanilla 1.4.0.525 A/B save pairs on `VCG02` `0x0010A214` (`flags =
  0xE0000006`) and `VMS01` `0x00080664` (`flags = 0xE0000002`): bit 0 running, bit 1 completed].
- bit 2: an f32 first (9.0, 5.0, 0.05 seen) [observed: vanilla 1.4.0.525 A/B save pairs:
  bit 2 prepends `00 00 a0 40 7c` (`5.0f`) / `2.5f` before bit 30 variables: quest script
  processing delay, `SetQuestDelay`].
- bit 31: **stages**: vsval count, then per stage: u8 stage index, `|`, u8 done, `|`, vsval
  log-entry count, then per entry: u8 entry index, `|`, u8 done, `|`, and when done 4
  bytes `23 01 E9 08` / `24 01 E9 08` [observed: vanilla 1.4.0.525 A/B save pairs: 4-byte
  date stamp on completed entries; exact day sub-field encoding remains `[hypothesis]`].
- bit 30 (465 of 466 quests): **script variables** (vsval count, then per variable: u32
  index, `|`, f64 value, `|`), then two more vsval-counted lists that were empty in every
  quest looked at. A quest with no changes but bit 30 stores `00|00|00`.
- bit 29: **objectives**: vsval count, then per objective: u32 objective index, `|`, u32
  state, `|` (`1` = displayed, `3` = displayed and completed) [observed: vanilla 1.4.0.525
  A/B save pairs on `VCG02` `0x0010A214` (`[(10, 3), (20, 1)]`) and `VMS01` `0x00080664`].
  B: VCG01 objective 10 = 1; C: objectives 10, 30, 40, 50 = 3.

Variables are stored as f64 even though scripts declare short/int/float/ref [observed:
`00 00 00 00 00 00 F0 3F` = 1.0]. A **reference variable** has the top bit of its index
set (`41 00 00 80` = variable 0x41) and holds a 3-byte ref instead of the f64 (`00 14 19`)
[observed: nvfairport, VStreetCombat, VFactionSquad, NVDLC02RainQuest; every quest record
of that save then walks to its end].

The flags byte (bit 1), across nvfairport's 32 quests that have it: 0x21 VMQ01 and 0x31
on the five story DLCs' MQ00 (running), 0x32 VCG01 (done), 0x20 VCG02, 0x00 VCG04 and
VGenericTimer [observed] [hypothesis: bit 0 running, bit 1 completed; the other bits
unknown].

**REFR** [observed bytes; meanings tagged per item below]:

- bit 0: u32 that looks like the record header's flag word (0x409, 0x29, 0x800809 seen)
  [observed: vanilla 1.4.0.525 A/B save pairs: `Disable` on door `0x001062AC` sets bit 0
  and prepends `0b 08 00 00 7c` (`0x080B`, `FORM_DISABLED`); three of its bits are read by
  the lab, §6.5].
- bit 2: worldspace/cell ref, then six f32 with no separators (position, rotation), `|`.
  Seen on refs moved by physics (tumbleweeds, items) [hypothesis: "moved"].
- bit 4: one f32 (`1.0` seen; `SetScale 1.5` writes `00 00 c0 3f 7c`) [observed: vanilla
  1.4.0.525 A/B save pairs: reference scale (`SetScale`)].
- bit 5 and bit 27: **item lists**: reuses the bar-delimited inventory framing of
  §6.4 (`vsval count, |`, then per item `ref, |, i32 count_delta, |, vsval lists, |`, per
  list `vsval extras, |`, per extra `type, |, payload, |`) [observed: vanilla 1.4.0.525 A/B
  save pairs: container inventory deltas against the base `CONT` stock on unlocked, owned
  (bit 6), picked-up (bit 10), and locked (bit 12) containers; observed item extras include
  `0x1C`, `0x21`, `0x22`, `0x23`, `0x24`, `0x25`, `0x2F` (`u32, |, u8, |`), `0x30`, `0x3E`,
  and `0x8D`; pass-through item extra `0x0D` remains `[hypothesis]`, while `0x26`/`0x28` are
  defensive `[design]` pass-throughs]. Vendor containers and Doc Mitchell's dresser have them.
- bit 6 (`0x40`), bit 10 (`0x400`), bit 12 (`0x1000`), bit 29, and bit 31: `vsval`-counted extra-data list:
  - bit 6 (`0x40`): `0x21` owner (`ref, |`), emitted by `SetOwnership` even without bits
    12/29/31 (e.g. `flags = 0x18000050`, bits 4 + 6 + 27 + 28, where `0x21` precedes the
    container inventory) [observed: vanilla 1.4.0.525 A/B save pairs].
  - bit 10 (`0x400`): `0x1C` original/holder reference (`ref, |`) on picked-up world `REFR`s
    (e.g. `flags = 0x00000403` / `0x00000401`, `04 7c 1c 7c <ref3 holder> 7c`) [observed:
    vanilla 1.4.0.525 + clean-room test-plugin A/B save pairs: picking up placed `REFR`s writes
    `0x1C` `ref3` in player & container inventories and `flags 0x403` + `04 7c 1c 7c ref3(holder) 7c`
    on the world `REFR`].
  - bit 12 (`0x1000`): lock entry `0x2A` (`u8 level, |, u8 flags, |, ref key, |, u32 tries,
    |, u32 unlocked, |`; bit 0 of `flags` = locked) on doors and containers [observed:
    vanilla 1.4.0.525 A/B save pairs: `Lock 50` and `Lock 75`].
  - on placed `MapMarker` `REFR`s (`NAME == 0x00000010`), bit 31 (optionally with bit 29)
    stores a `vsval`-counted extra-data list containing type `0x2C` (`u8, |`; `0x01`
    visible, `0x03` visible + fast-travelable) and optional bit-29 `0x74` (`ref, |`), ending
    at `c.at_end()` [observed: vanilla 1.4.0.525 A/B save pairs: `ShowMap ... 0` -> `0x01`,
    `ShowMap ... 1` -> `0x03`].
  - on other bit-6/10/12/29/31 references: `0x0D` script variables (`ref script, |, vsval nv, |,
    nv × (u32 index, |, ref if index & 0x80000000 else f64, |), vsval 0, |, vsval 0, |`),
    `0x21` owner (`ref, |`), `0x2A` lock (above), `0x1C` (`ref, |`), `0x22` ownership global
    (`ref, |`), `0x23` owner rank (`i32, |`), `0x25` condition (`f32, |`), `0x30` item scale
    (`f32, |`), `0x39` dropped-from actor (`ref, |`), and `0x6E` loaded weapon magazine
    (`ref ammo, |, u32 rounds, |`) [observed: vanilla 1.4.0.525 A/B save pairs], plus auxiliary
    reference extras skipped while walking the list (`0x18`, `0x1B`, `0x2C`, `0x2E`, `0x2F`,
    `0x46`, `0x56`, `0x60`, `0x73`, `0x74`, `0x75`, `0x7C`, `0x8B` [hypothesis: reference `0x0D`
    script variables and auxiliary extra data]; `0x26`/`0x28` defensive `[design]` pass-throughs).
- bit 23 and bit 28: animation state (`0x28 | u16 len | bytes |` / `vsval | u16 len | bytes |`
  string such as `"Open"`, `"Close"`, `"Backward"`, `"SpecialIdle"`) [observed: vanilla
  1.4.0.525 A/B save pairs: `SetOpenState 1` on door `0x001062AC` changes `"Close"` to
  `"Open"`].
- bit 29: a short ref list ending `74 | ref` [unknown].

**ACHR / ACRE**: flag 0x40000 (bit 18) on 2,020 of 2,418 ACHR records, data 297 bytes for
most [observed]. These include an f32 near 17,634 that moved with game time between C and
D and a u32 that grew by 765 [observed]; meanings unknown. Non-player `ACHR`/`ACRE` records
(without bit-2 ragdoll layout) begin with optional placement (bit 1: `ref, 6 × f32, |`),
`u8 process_level, |`, optional form flag word (bit 0: `u32, |`, where bit 16 = `0x0001_0000`
is actor dead state [observed: vanilla 1.4.0.525 A/B save pairs: `KillActor` on placed NPC
`0x00104C0F` sets `0x00010409`]), optional scale (bit 4: `f32, |` [observed: vanilla 1.4.0.525
A/B save pairs: `SetScale 1.35`]), optional `vsval`-counted actor extra-data list (when bits
11/17/18/29/31 are present: `0x5E` actor faction ranks [observed: vanilla 1.4.0.525 A/B save
pairs: `SetFactionRank`], `0x0D` actor script variables [observed framing; non-empty vars
`[hypothesis]`], plus auxiliary actor extras `0x04`, `0x18`, `0x19`, `0x1B`, `0x1D`, `0x1F`,
`0x2E`, `0x46`, `0x56`, `0x60`, `0x73`, `0x74`, `0x75`, `0x7C`, `0x8B` [hypothesis]), and
optional inventory item list (bits 5/27, with `0x16` worn, `0x1C` original ref, `0x21` item
owner, `0x22` ownership global, `0x23` owner rank, `0x24` stack count, `0x25` condition,
`0x2F` container extra, `0x30` item scale, `0x3E` no-unequip flag, and `0x8D` weapon mods
[observed: vanilla 1.4.0.525 A/B save pairs]; `0x17` secondary worn is a defensive `[design]`
pass-through not emitted by vanilla `FalloutNV.exe 1.4.0.525`). The player
ref (0x14) record is 4,429 bytes in C (flags 0xB0000832) and starts with a ref and six f32
like REFR bit 2 [observed]; see §6.4 for the decoded player layout.

**NPC_ 0x7 (player base)**: fields follow in fixed change-flag order (`|`-delimited)
`bit 1 (ACBS) -> bit 4 (spells) -> bit 2 (SPECIAL) -> [bit 3 -> bit 9] -> bit 5 (name) -> bit 11 (appearance) -> bit 24 (SexChange)`
[observed: vanilla 1.4.0.525 A/B save pairs: `NPC_ 0x00104C0C` `flags 0x22` emits 24B ACBS `|`
then name `|`; `NPC_ 0x7` `flags 0x01000034` emits bit 4 spells, bit 2 SPECIAL, bit 5 name `|`,
and trailing bit 24 `01 |`; bits 3 and 9 remain `[hypothesis]`]:
- bit 1: 24-byte base stats (ACBS) block `|` [observed: vanilla 1.4.0.525 A/B save pairs:
  24-byte ACBS block; `u32` flags at offset 0 bit 1 `0x02` = Essential on `SetEssential`,
  bit 0 = female `[hypothesis]`],
- bit 4: two `vsval`-counted spell/ability ref lists (`vsval n, |, n × (ref, |)`)
  [observed: vanilla 1.4.0.525 A/B save pairs: `Player.AddSpell 0x1768BC` writes
  `flags = 0x10`, `04 7c <ref3> 7c 00 7c`],
- bit 2: seven `u8` SPECIAL values (`S, P, E, C, I, A, L`) followed by `|` [observed:
  vanilla 1.4.0.525 A/B save pairs: `ModPCAttribute Strength 2` writes `NPC_ 0x7`
  `flags = 0x4`, `07 05 05 05 05 05 05 7c`],
- bit 3: 20-byte AI block `|`,
- bit 9: 28-byte skills block `|`,
- bit 5: display name (`u16 len, |, string, |`) written by `SetActorFullName <MESG>` on
  `NPC_`, `CREA`, and player base `NPC_ 0x7` (which also updates the header player name)
  [observed: vanilla 1.4.0.525 A/B save pairs: `SetActorFullName 0x00171DBF` on DocMitchellREF
  and Player writes `flags 0x20` and `u16(12)|"Orris's Thug"|` on `NPC_ 0x00104C0C` and `NPC_ 0x7`;
  `SetActorFullName 0x001717D3` on ED-E writes `flags 0x20` and `u16(24)|"Damaged Duraframe Eyebot"|`
  on `CREA 0x0010C769`],
- bit 11: FaceGen appearance (`u8 (1), |`, 50 `f32, |` geometry-symmetric morphs `fggs`,
  30 `f32, |` geometry-asymmetric morphs `fgga`, 50 `f32, |` texture-symmetric morphs `fgts`,
  `ref hair, |`, `ref eyes, |`, `f32 hair_length, |`, `u32 hair_color_rgba_le (R, G, B, 0), |`,
  `vsval n, |, n × (ref head_part, |)`) [observed: vanilla 1.4.0.525 A/B save pairs:
  `Player.MatchFaceGeometry 0x00104C0F 100` writes `NPC_ 0x7` `flags 0x00000800`, 676 bytes,
  parsing cleanly to EOF with eyes `0x00004253` and head part `0x000B5BEA` matching
  `NPC_ 0x00104C0C` `ENAM`/`PNAM` in `FalloutNV.esm`],
- bit 24 (`0x0100_0000`): `u8 female, |` [observed: vanilla 1.4.0.525 A/B save pairs:
  `Player.SexChange` writes `NPC_ 0x7` with `flags = 0x01000000`, `01 7c`].

**CELL**: fields follow in fixed change-flag order [observed: vanilla 1.4.0.525 A/B save
pairs for bits 1, 3, 30, 31; bit 29 remains `[hypothesis]`]: bit 29 (4-byte exterior cell
grid), bit 30 (`u32, |` detach hours, e.g. `0x129` = 297 [observed: 297 = GameDaysPassed
(12.39) × 24; also observed: vanilla 1.4.0.525 A/B save pairs: leaving `0x103DF9` via
`CenterOnCell` turns on bit 30 and prepends `u32 132 |`], converted to days as `hours / 24.0`),
bit 1 (`u8, |` cell flags, `0x20` = public area [observed: vanilla 1.4.0.525 A/B save pairs:
`SetCellPublicFlag 1`]), bit 31 (local-map fog-of-war data; implies cell visited [observed:
vanilla 1.4.0.525 A/B save pairs: interior 76B and newly visited exterior 33B]), and **bit 3
(`ref, |` cell owner `NPC_`/`FACT`) serialized AFTER bit-31 fog** [observed: vanilla
1.4.0.525 A/B save pairs: `SetCellPublicFlag 1` + `SetCellOwnership DocMitchell` on
`0x00103DF9` yields `flags = 0x8000000a` with `20 7c`, 76B fog, and trailing `ref3 |` at
bytes `78..82`].

**INFO**: bit 31, no data. 13 in B (new game), 52 in C [observed: vanilla 1.4.0.525 A/B
save pairs: `INFO 0x001751E9 flags = 0x80000000 len = 0` — "this response has been said",
the say-once flag].

**FACT** (PlayerFaction and three others): bit 2: vsval count, then per entry: ref, `|`,
i32 modifier, `|`, u32 reaction, `|` [observed: vanilla 1.4.0.525 A/B save pairs: `SetEnemy`
-> reaction `1`, `SetAlly` -> reaction `2`; read by the lab, §6.5].

**FLST** bit 31: `u32` count, `|`, then `count × (ref, |)` [observed: vanilla 1.4.0.525 A/B
save pairs: `AddFormToFormList` writes `01 00 00 00 7c <ref3> 7c` (9B) and 15-entry lists
write `0f 00 00 00 7c ...` (65B), confirming `u32` count width]. **LVLI** bit 31: `vsval`
count, `|`, then entries of `ref, |, u16 level, |, u16 count, |, f32 condition, |` [observed:
vanilla 1.4.0.525 A/B save pairs: `AddItemToLeveledList ... 3 2 0.75` writes
`vsval(1) | ref3 | u16(3) | u16(2) | f32(0.0075) |`]. **CHAL** bit 1: `u32 progress, |,
u32 flags, |` (1 and 0 seen in A–D) [observed: vanilla 1.4.0.525 A/B save pairs across 5
vanilla challenges]. **CLAS** bit 1 (PlayerClass): four u32, tag-skill AV codes and −1
[observed: vanilla 1.4.0.525 A/B save pairs: `SetPlayerTagSkill Lockpick 1` writes
`[-1, 36, -1, -1]`].

### 6.3 The max save (M) [observed unless tagged]

M parses with the same rules: the record walk ends exactly at table offset 4 after
42,784 changed forms, the form ID array holds 60,872 IDs, and the file ends after the same
9-byte tail. Form IDs resolve against the masters **in M's own load order** (resolving them
with A–D's order gives wrong record types), confirming that a form ID's mod index is the
plugin's position in the save's list.

- **Worldspace array**: 23 worldspaces, including the four story DLCs' (Zion, the Villa and
  its town worldspaces, Big MT, the Divide) and the Strip, Freeside, Boulder City.
  Together with B (none, never outdoors) this supports "worldspaces visited" [hypothesis].
- **Created forms** (kind-2 refs, 0xFF…): 574 records in M (556 REFR, 15 ACRE, 3 type 3)
  [observed: M]. Created `REFR`s begin with a 32-byte header ending with `|` at byte 31:
  `cell_or_world_ref` (3 bytes), six `f32` LE (position and rotation, no internal `|`),
  `u8` header flag (`0x01` or `0x03`), `base_form_ref` (3 bytes), and `|` [observed: vanilla
  1.4.0.525 A/B save pairs: `Player.PlaceAtMe Pencil01 1` creates `0xFF` `REFR` with this
  exact 32-byte placement header + optional bit-2 Havok bytes].
  The next created form ID in M is 0xFF00155E.
- **Global table 1 grows with play**: type 2 is 4,329 bytes: vsval count 603, then
  (ref, `|`, u16, `|`) entries [observed: vanilla 1.4.0.525 A/B save pairs: per-base-form
  death counts]; type 9 is 221,569 bytes (7 in
  A–D; contents not decoded); type 10 is 2,793 bytes and holds a radio song's file path
  (`data\sound\songs\radionv\…mp3`) [observed: radio and music playback state, §5.1]; type 0
  (43 u32) holds large counts (up to 147,591) [observed: misc statistics, §5.1].
- **Record types beyond A–D**: 4 PGRE (placed grenades, 205), 13 TACT (talking
  activators, 21), 31 NOTE (16, no data) [hypothesis: opened/read notes; newly acquired
  notes are stored in the player `ACHR 0x14` tail notes list, §6.4], 32 ECZN (encounter
  zones, 20, 17 bytes each: four little-endian `u32` followed by `|`: `[detach_hours,
  visit_hours, 0, locked_level]`) [observed: vanilla 1.4.0.525 A/B save pairs: entering
  `BisonSteve01` stamps `ECZN 0x000D7034` with `(0, 132, 0, 2)|`, leaving updates
  `stamps[0]` to `132`; entering `Vault11c` stamps `ECZN 0x000E9081` with `(0, 132, 0, 6)|`],
  35 PACK (1),
  43 REPU (reputations, 9: bit 1, two f32, e.g. 2.0 and 0.0) [observed: vanilla 1.4.0.525
  A/B save pairs: `AddReputationExact 1 15` / `0 7` writes `f32 15.0 | f32 7.0 |` (fame and
  infamy)], and many more NPC_ (573, bits 2, 3, 9: seven SPECIAL-like bytes, then a block of
  14 skill-like bytes) [hypothesis: base stats of levelled NPCs fixed at first meeting].
- **Counts and sizes** (data bytes, min / median / max): REFR 26,683 (0 / 19 / 4,856);
  ACHR 3,560 (285 / 528 / 2,675); ACRE 2,861 (281 / 954 / 3,693); CELL 4,208
  (2 / 42 / 858); INFO 3,332 (all 0); QUST 483 (2 / 20 / 695); CHAL 130; FACT 46
  (5 / 44 / 870); FLST 36 (9 / 25 / 405). The player ref record is **108,445 bytes**
  (4,429 in C) [observed]; its contents were not decoded.

Size guide for our own format: a 61-hour game keeps state for about 42,800 forms; most of
it is per-reference (REFR, ACHR, ACRE: 33,000) and per-cell (4,200) [observed].

### 6.4 The player record, as far as `fnv_save::fos` reads it [observed unless tagged]

Read in the 19 `nvf*.fos` saves of the owner's `Saves` folder (2026-11-01; flags
0xB0000832, 0x90000832, 0xB0400832 and 0xB0400C32). The data starts the same in all of them:

| Part | Encoding | Notes |
|---|---|---|
| place | ref, six f32, `\|` | the worldspace outdoors (WastelandNV, TheStripWorldNew) or the interior cell (GSDocMitchellHouse), position, rotation in radians (nvfairport: z 1.309 = 75°) |
| actor values | 232 × (f32, `\|`) | three arrays of 77 (the actor value count) and one more entry (`index 231`). First array (`0..77`): 2.0 at codes 38 (Melee Weapons) and 43 (Speech) in 17 saves, 2.0 at code 5 (Strength) in the two level-50 saves (nvfLanius, nvfSlides). Second array (`77..154`): code 24 holds 10 in nvfairport, 143 to 3,419 in mid-game saves and 186,200 in the level-50 ones, code 23 holds 50 to 1,000 [observed: vanilla 1.4.0.525 A/B save pairs: permanent `ModAV` modifiers (Strength 5, Speech 43), Experience (24), Karma (23), and `ModAV`-applied radiation (54) / hardcore needs (73–75)]. The first is not skill points spent, as read on 2026-11-01: nvfairport's +2 Melee Weapons and +2 Speech are exactly the enchantment of the Vault 21 Jumpsuit it wears (VaultSuit21's EITM EnchClothingVaultSuitNormal, Speech 2 and Melee Weapons 2) [observed, 2026-11-11; also observed: vanilla 1.4.0.525 A/B save pairs: Buffout/Rad-X and SleepDeprivation Stage 1 `-1` AGI write to AV1]: giving them as skill points and putting the suit on counted them twice (Melee 36 against the probe's 34). Active effects' modifiers; not applied as base points to avoid double-counting worn gear. nvfairport's Melee is 34 = 2 + 2 × 5 + ⌈10 × 0.5⌉ + 15 tag + 2 (the suit), which matches its probe. Third array (`154..231`): current damage deltas — negative deltas on AP (code 12) and limb conditions (codes 25–31) [observed: vanilla 1.4.0.525 A/B save pairs: `DamageAV ActionPoints 15`, `PerceptionCondition 40`, `LeftAttackCondition 30`; positive AV3 accumulation on rads/needs remains `[hypothesis]`]. Entry `231` (the 232nd float): current health damage delta, negative when injured [observed: vanilla 1.4.0.525 A/B save pairs: `DamageAV Health 40` -> `-40.0`] |
| post-AV float | u8 (0x00), `\|`, optional f32, `\|` | `u8 process_level (0x00), \|`, followed by `f32 scale, \|` **only when change flag bit 4 (`0x10`) is set** (`1.0` in the `nvf*.fos` saves [observed: `nvf*.fos`]; absent when bit 4 is `0`, `1.25` when `Player.SetScale 1.25` flips bit 4 [observed: vanilla 1.4.0.525 A/B save pairs]) |
| actor extra-data list | vsval count, `\|`, then `count` entries: type byte, `\|`, payload, `\|` | Present when actor-extra or inventory change flags are set (omitted on `0x10000002` saves without those flags [observed: vanilla 1.4.0.525 A/B save pairs]). Actor extra-data types: `0x5E` (`vsval n, \|, n × (ref, \|, i8 rank, \|)` [observed: vanilla 1.4.0.525 A/B save pairs: `Player.SetFactionRank`]), `0x1D` (`vsval n, \|, n × (ref, \|)` gated on `ACHR 0x14` bit 11 `0x800` [observed: vanilla 1.4.0.525 A/B save pairs: recruiting Boone via `VNPCFollowers.bBooneHired = 1` + `SetPlayerTeammate 1` + `EvaluatePackage` flips bit 11 and emits `0x1D` with `[0x00096BCE]`]), `0x18` (`ref, \|, 12 bytes, \|, f32, \|`, seen in early `nvf*.fos` saves), and `0x74` (`ref, \|`, seen with change flag 29) [observed: `nvf*.fos`]; `0x60` (`u32, \|`), `0x75` (`ref, \|, ref, \|, u8, \|`), and 5-entry lists with `0x18` absent [hypothesis]. An unknown type stops inventory parsing rather than guessing |
| inventory | vsval count, `\|`, then per item: ref, `\|`, i32 count, `\|`, vsval lists, `\|`, per list: vsval extras, `\|`, per extra: type byte, `\|`, payload, `\|` | nvfairport: 27 items, the same 27 kinds its probes counted [observed: nvfairport] |
| post-inventory tail | bit-22 base-AV overrides, notes, perks, and teammate perks | When change flag bit 22 is set, `vsval n, \|, n × (u8 av_code, \|, f32 val, \|)` for any `SetAV` AV code `< 77` appears immediately before `FF FF \| FF FF \| 00 \| 01 01 00 00 \|` [observed: vanilla 1.4.0.525 A/B save pairs: `Player.SetAV Luck 9` flips bit 22 and inserts `04 7c 0b 7c 00 00 10 41 7c`]. In the High-Process block starting at byte `0` of the post-inventory tail (immediately after `parse_item_list` ends), counting 0-based across its `0x7C` (`\|`)-terminated fields (`tok[0]` = first `\|`-terminated field after the inventory section; exact 77-field width table `PROCESS_PREFIX_WIDTHS` in `crates/fnv-save/src/fos.rs::parse_player_active_magazine`, 286 bytes `0x11E` total including every `\|` separator), **`tok[70]`** (byte offset `+258` / `0x102`, `u32, \|`) is `0xFFFFFFFF` when no weapon is equipped and non-`0xFFFFFFFF` when a weapon is equipped, followed by `tok[71]` (`u8, \|`), `tok[72..=75]` (four `ref3, \|` fields), and **`tok[76]`** (byte offset `+281` / `0x119`, `u32, \|`), which records the active equipped weapon's current magazine round count (`13 -> 12 -> 1 -> 13 -> 12`; `11`, `6`, `2`, `30`; `0` when unequipped) even when item extra `0x6E` is absent [observed: vanilla 1.4.0.525 A/B save pairs]. Later in the tail (after `AD DE AD DE \|` and the 11-slot `ref3 \|` array), the player's acquired notes list `vsval n, \|, n × (ref3 note, \|)` precedes known actors and the player's perk list `vsval n, \|, n × (ref perk, \|, u8 rank, \|)` before the 5-zero-`u32` boundary [observed: vanilla 1.4.0.525 A/B save pairs: `AddNote 0x00175AB0`, `AddNote 0x00177875`, and world pickup of `0x000E8893` -> `0x000E88A0` each grow `ACHR 0x14` by +4 bytes with `04 7c <ref3> 7c` and zero `TYPE_NOTE` records; `AddPerk`], and the teammate-perk list `vsval n, \|, n × (ref perk, \|, u8 rank, \|)` immediately precedes the fixed 54-byte end trailer (`00 \| 01 \| 01 \| 8 × (u32 \|) \| 4 × (u8 \|)`) [observed: vanilla 1.4.0.525 A/B save pairs: `Player.AddPerk ... 1`] |

Seventeen item extra-data types are handled in inventory item lists (tags per type below):

- `0x0D` (`ref, \|, vsval nv, \|, nv × (u32 idx, \|, ref if idx & 0x80000000 else f64, \|), vsval 0, \|, vsval 0, \|`): same script-variable framing as `QUST` bit 30 [hypothesis: layout and item script instance]; non-empty trailing lists fail soft.
- `0x16` (0 payload bytes): worn armour, Pip-Boy, glove, and equipped weapons [observed: `nvf*.fos` and vanilla 1.4.0.525 A/B save pairs: tracks weapon & armor equip swaps].
- `0x17` (0 payload bytes): defensive pass-through guard `[design]` — not emitted by `FalloutNV.exe 1.4.0.525` even under direct clean-room test-plugin stimulation (dual-slot ring `BMDT=0x180`, left/right ring & hand slots, equipped individually, simultaneously, twice, and locked with `noUnequip=1` on Player and NPCs all emit `0x16` only) `[observed negative proof]`.
- `0x1C` (`ref, \|`): original world `REFR` on picked-up inventory items (and current holder `REFR` on the picked-up world `REFR` itself, gated by `REFR` change flag bit 10 `0x400`) [observed: vanilla 1.4.0.525 + clean-room test-plugin A/B save pairs: picking up placed `REFR`s writes `0x1C ref3` in player & container inventories and `flags 0x403` + `04 7c 1c 7c ref3(holder) 7c` on the world `REFR`].
- `0x21` (`ref, \|`): owner [observed: vanilla 1.4.0.525 A/B save pairs on `REFR` and inside `parse_item_list` entries when picking up owned items (Doc Mitchell-owned Plunger emits `0x21 = 0x00104C0C`)].
- `0x22` (`ref, \|`): ownership global (`XGLB`) carried from placed `REFR`s into player inventory, container inventory, and re-dropped `REFR`s [observed: vanilla 1.4.0.525 + clean-room test-plugin A/B save pairs: `REFR` with `XGLB=0x00000038` (`GameHour`) writes `0x22 ref3(0x38)|` in player inv, container inv, and re-dropped `REFR`].
- `0x23` (`i32, \|`): owner faction rank [observed: vanilla 1.4.0.525 A/B save pairs: picking up Gomorrah `ShotGlass01` with `XRNK = 1` emits `0x23 = 1`].
- `0x24` (`i16, \|`): stack count [observed: vanilla 1.4.0.525 A/B save pairs].
- `0x25` (`f32, \|`): condition HP on weapons and armour [observed: `nvf*.fos` and vanilla 1.4.0.525 A/B save pairs: `AddItemHealthPercent` 0.50/0.60/0.75 writes 75.0/48.0/112.5].
- `0x26` (`f32, \|`): defensive pass-through guard `[design]` — not emitted by `FalloutNV.exe 1.4.0.525` even with a synthetic carryable `LIGH` (`CanBeCarried=0x02`, `Time=120s`, `GetEquipped=0` on Player & NPC) `[observed negative proof]`.
- `0x28` (`f32, \|`): defensive pass-through guard `[design]` — not emitted by `FalloutNV.exe 1.4.0.525` even under live combat hits with synthetic `EITM`/`EAMT=500` charged weapons (`0/57` vanilla weapon `ENCH`es carry charge) `[observed negative proof]`.
- `0x2F` (`u32, \|, u8, \|`): container extra data [observed: vanilla 1.4.0.525 A/B save pairs].
- `0x30` (`f32, \|`): item scale (`XSCL`) carried from scaled world `REFR`s into player and container inventories [observed: vanilla 1.4.0.525 + clean-room test-plugin A/B save pairs: `REFR`s with `XSCL=1.5` and `1.75` write `0x30 f32(1.5)|` and `f32(1.75)|` in player & container inventories].
- `0x3E` (0 payload bytes): no-unequip item flag written alongside `0x16` when an item is equipped with `EquipItem <item> 1` (`noUnequip = 1`) [observed: vanilla 1.4.0.525 A/B save pairs: `EquipItem 0x000E3778 1` on `CraigBooneREF` writes `3e 7c` alongside `16 7c`].
- `0x4A` (`u8, \|`): hotkey quick-slot index (0-based, `0..=7`) [observed: vanilla 1.4.0.525 A/B save pairs: `SetHotkey 3` -> `2`, `SetHotkey 5` -> `4`; not persisted in `NVFSAVE` v1].
- `0x6E` (`ref, \|, u32, \|`): loaded magazine ammo form and round count — framing `[observed: vanilla 1.4.0.525 A/B save pairs: killing armed NPCs in combat drops 0xFF WEAP REFRs with 0x6E=(0x0008ED02, 5) and (0x0008ED03, 4) plus 0x39=ref3(actor)]`, whereas vanilla player inventory weapons emit zero `0x6E` and track the active equipped weapon's round count in `ACHR 0x14` process-block `tok[76]` (`u32, \|`, gated on `tok[70] != 0xFFFFFFFF`) `[observed: vanilla 1.4.0.525 A/B save pairs]`.
- `0x8D` (`u8, \|`): weapon mods, low 3 bits for slots 1–3 [observed: vanilla 1.4.0.525 A/B save pairs: `SetEquippedWeaponModFlags 3` writes `03 7c` both equipped and unequipped].

Any other item extra-data type fails soft so no inventory is guessed out of misaligned bytes.

The Pip-Boy and its glove are listed worn with count 0 [hypothesis: the count is the
change against the base form's inventory, which holds them].

The player's base form (NPC_ 0x7, bit 2) holds SPECIAL: `05 05 05 05 05 05 0A` in
nvfairport, Luck 10 as its probes read. PlayerClass (CLAS, bit 1) holds the three tag
skills as actor value codes and −1 (38, 43, 44: Melee Weapons, Speech, Survival).

Global table 1 type 1, decoded further: next created ID `|`, world ref `|`, grid x (i32)
`|`, grid y (i32) `|`, world or cell ref `|`, three f32 `|`, quest ref `|`, u32 `|`, f32 `|`,
u32 `|`. nvfairport: grid (9, −29), which holds the player's position (37000, −117160)
[observed]. Its quest ref is 0x0010A214, the quest GetActiveQuest gave in nvfairport's
gaps4 probe [observed], so it is the active quest.

`fnvtool ext queries --save` gives the runner, in this order: the globals, each quest's
running/completed bits, stages done and not done, objectives and variables (by the
script's variable names), the active quest (global type 1, §6.4), the player's name
(the header's), SPECIAL as the player's actor values, the tag skills, the inventory as changes on top
of the new game's, the items it marks worn (extra 0x16) put on, the place, and the references'
and factions' changes of §6.5. What it leaves out is counted in its output
(`save.skipped`, by reason): a quest record the reader does not understand (bytes left
over, a non-empty list after the variables), a form of a plugin not loaded, a form the save
created (0xFF...), a reference past the form ID array. The pre-inventory `0x00 | f32 |` marker and every actor and item extra-data type byte and `|`
separator are verified: an unknown actor or item extra-data type gives no inventory rather
than guessing. The tag skills are read only from PlayerClass's record (0x00057E6A).

### 6.5 References and factions, as `fnv_save::fos` reads them (2026-11-11, #1023)

Read in nvfairport [observed unless tagged]; the B, C and M saves were used to check
the meanings.

**Which layouts are read.** Among nvfairport's 4,051 REFR, ACHR and ACRE records, the
start of the data follows from change flags 0-2 in these shapes, each checked by its `|`
markers on every record of that shape:

| Record | Flags 0-2 | Data starts with | Records |
|---|---|---|---|
| REFR | 0 alone | u32 form flag word, `\|` | 106 |
| REFR | 1 or 2, not 0 | place: ref, six f32, `\|` | 40 (4 more with flag 1 do not fit, and 1 with flags 0 and 2, both left out) |
| ACHR, ACRE | 1, not 2 | place, then a byte and `\|` (0, 3 seen; meaning unknown), then the flag word and `\|` when flag 0 is set | 851 |
| ACRE | 0, 1 and 2 | place, then not the byte | 16, left out |
| ACHR, ACRE | 0 alone | not the flag word | 10, left out |

The place is the same encoding as the player's (§6.4): the cell (interiors) or the
worldspace (exteriors), position, rotation in radians. Doc Mitchell (0x00104C0F) stands
at (1087.0, 2198.5, 7362.0) in GSDocMitchellHouse in nvfairport, against (2288.8,
2244.4, 7360.0) in the ESM's placement.

**The form flag word.** The values seen are combinations of 0x1, 0x2, 0x8, 0x20, 0x40,
0x400, 0x800, 0x20000, 0x40000, 0x100000, 0x800000 and 0x1000000; 0x1 and 0x8 are on
every one. 0x400 matched the ESM's Persistent bit on the nine references checked against
the ESM [observed]. Three bits are read by `ext queries --save` and a fourth (`0x10000`) by `fnvtool save convert` [hypothesis for each meaning, from the references that carry it]:

- **0x800, disabled**: the ESM's "Initially Disabled" header bit. Reference 0x001475B8 is
  initially disabled in the ESM and saved without it (enabled since); VCG00PropEnabler
  (0x0010B3A8), not initially disabled, is saved with it; the Strip's travellers
  (VStreetTraveler*REF) carry it as in the ESM.
- **0x20, removed from the world**: VCG01CasualHardcoreMessageREF (0x0011F293), the note
  the opening quest has the player take (issue #1041: the original's reference walk in
  Doc Mitchell's house counts it only with taken references included), saved as 0x429;
  B, made before it is taken, has no flag word for it. DocHouseTriggerREF also has it.
- **0x40, talked to the player**: on DocMitchellREF and RingoRef (0x449), both spoken to
  in the opening, and on VMQTopsSecMessengerREF; not on any of the 28 other actors that
  have a flag word. B (before the player speaks to Doc) has no flag word for Doc.
- **0x10000, dead** (`FORM_DEAD`, bit 16): on killed placed `ACHR` and `ACRE` records
  [observed: vanilla 1.4.0.525 A/B save pairs: `KillActor` on `0x00104C0F` writes
  `0x00010409`].

`fnvtool ext queries --save` gives each reference with a flag word its disabled and
removed state (and, for actors, talked to), and each place, to the run.

**Factions** (FACT, change flag 2): vsval count, `|`, then per relation: ref, `|`, i32,
`|`, u32, `|`; the record ends after the last [observed: nvfairport's four FACT
records]. PlayerFaction (0x0001B2A4) lists 48 relations, the three DLC factions one each
(towards PlayerFaction). The u32 is 0 to 3, the i32 0, 30, 50, 100 or -100
[hypothesis: the ESM's XNAM fields, the modifier and the group combat reaction 0
neutral, 1 enemy, 2 ally, 3 friend]. `ext queries --save` sets each reaction as
SetEnemy/SetAlly would; the modifier is not used.

**Reputations**: nvfairport has no REPU record (M has nine, §6.3), so every reputation
is at its start [observed].

### 6.6 Offline `.fos` converter (`fnvtool save inspect-fos` / `convert`) [design; converted fields tagged below]

`crates/fnv-game/src/fos_convert.rs` and `crates/fnvtool/src/save_cmd.rs` build an offline
converter on top of `fnv_save::fos`. It populates a `World` and `Game` from the active load
order plus the decoded sections of §§3–6.5, then emits a standard `NVFSAVE\0` v1 file through
`Game::checked_snapshot` and `fnv_save::write`.

- **Observed / documented structure** (§§3–6.5): container header strings, dimensions, and
  RGB screenshot channel order; plugin load order; form ID array; global table 1 types 0
  (`misc_stats`), 1 (`active_quest` & location), 2 prefix (`dead_counts`), 3 (`GLOB` values),
  8 (`weather`), and 10 active-station list 2 (`radio_state`/`radio_station`); player and
  reference positions/rotations; `QUST` flags (bit 1), delay (bit 2), objectives (bit 29),
  script variables (bit 30), and stages (bit 31); player AV1/AV2 (permanent `ModAV`, XP 24,
  Karma 23, `ModAV` radiation 54 and hardcore needs 73–75), AV3 damage deltas (AP 12, limbs
  25–31), AV entry `231` health damage, bit-4-gated player scale, `0x5E` faction ranks,
  `0x1D` active teammates/companions, bit-22 `SetAV` base-AV overrides, `ACHR 0x14` process-block
  `tok[76]` active equipped weapon magazine round count, `ACHR 0x14` tail acquired notes list,
  player & teammate perks, `NPC_ 0x7` multi-flag order (`1 -> 4 -> 2 -> 5 -> 11 -> 24`), bit-2
  SPECIAL, bit-4 spells, bit-5 name, bit-11 FaceGen appearance morphs/hair/eyes/head parts, and
  bit-24 `SexChange` female flag, `CLAS` bit-1 tag skills, and inventory item extras `0x16`
  worn, `0x1C` original ref, `0x21` owner, `0x22` ownership global, `0x23` owner faction rank,
  `0x24` stack count, `0x25` condition, `0x2F` container extra, `0x30` item scale, `0x3E`
  no-unequip flag, `0x4A` hotkey, `0x6E` weapon magazine framing (NPC-dropped `REFR`s), and
  `0x8D` weapon mods; non-player `ACHR`/`ACRE` `FORM_DEAD` (`0x10000`), scale (bit 4), `0x5E`
  factions, and bits 5/27 inventories; `REFR` bit-0 `FORM_DISABLED`, bit-4 scale, bit-6 `0x21`
  owner, bit-10 `0x1C` original/holder ref, bit-12 `0x2A` lock, `0x39` dropped-from actor,
  bit-28 Open/Close door state, bit-31 `0x2C` map markers, bits 5/27 container inventories, and
  created `0xFF` `REFR` 32-byte headers; `ECZN` bit 31 locked levels & timestamps; `CELL`
  bits 1/3/30/31 (bit 3 owner after bit-31 fog); `FACT` bit 2 relations; `FLST` bit 31 (`u32`
  count); `LVLI` bit 31; `REPU` bit 1; `CHAL` bit 1; and `INFO` bit 31 said-once (`[observed]`
  in named saves A–D, M, the `nvf*.fos` set, and vanilla 1.4.0.525 A/B save pairs).
- **Remaining hypothesised fields written into `.nvfsave` `[hypothesis]`**:
  - `NPC_ 0x7` bit 1 ACBS bit-0 female fallback, bit 3 AI block, and bit 9 skills block (§6.2),
  - player AV3 positive growing meters fallback (radiation 54, dehydration 73, hunger 74,
    sleep deprivation 75) when present in AV3 rather than AV2 (§6.4),
  - global table 1 type 10 opening-header radio fallback when the active-station second list
    is absent (§5.1),
  - empty-payload `NOTE` bit 31 changed forms (§6.3; opened/read notes unioned with observed
    `ACHR 0x14` tail notes),
  - `REFR`/`ACHR` `0x0D` script variables on non-created forms and auxiliary skipped
    reference/actor extras (§6.2),
  - player-inventory `0x6E` worn-weapon magazine ammo/rounds when present on a player item
    (§6.4; framing is `[observed]` on NPC-dropped `REFR`s, while vanilla player inventory weapons
    emit zero `0x6E` and track active rounds in `ACHR 0x14 tok[76]`; `0x17`, `0x26`, `0x28` are
    retained only as defensive `[design]` pass-throughs proven unemittable by vanilla 1.4.0.525,
    and `0x4A` hotkeys remain counted in `skipped` because `NVFSAVE` v1 does not persist hotkeys).

Every `[hypothesis]` field actually written during a conversion is counted explicitly in
`ConversionReport::hypothesis_fields` (alongside a summary entry in `warnings`) so callers
and users can see exactly which hypothesised `.fos` readings entered the `.nvfsave`. Every
unmapped global table, created `0xFF` form, missing-plugin form, unmapped or
non-empty-payload changed-form record, overflowed inventory delta, non-finite IEEE-754
float (`NaN` / `±Infinity` in quest variables, globals, transforms, item conditions,
reputations, delays, or actor values, which would otherwise serialize as JSON `null`),
and sibling `.nvse` co-save is counted in `skipped` rather than guessed. After writing,
`fnvtool save convert` verifies both `fnv_save::read_header` and full typed
`fnv_save::read_body::<GameSnapshot>` + `GameSnapshot::check` round-trip. The game
runtime itself remains `.nvfsave`-only.

## 7. What moves between C and D [observed]

C and D: same starting save, same door, about a second of game time apart (GameHour
9.46967 → 9.47007). File size, form ID array, worldspace array and the set of changed
forms are identical. What differs:

- header: play time 0:11:50 → 0:11:51;
- globals: GameHour and GameDaysPassed (table 1 type 3), the two GameHour copies in the
  weather entry (type 8), and floats in types 4 and 5;
- every ACHR and ACRE record (3,318) changes data without changing size: in the common 297-byte ACHR
  layout, an f32 at data offset 54 (17,634.11 → 17,634.16) and a u16/u32 at 240
  (11,688 → 12,453) [observed]; other layouts change the same kind of fields at other
  offsets; the player record changes;
- 38 REFR records, all with flag bit 28 (with or without 31 and 27): the animation/script
  data of activators [hypothesis: animation timers];
- one QUST record (vDialogueGomorrah): script variable 24 holds a copy of GameDaysPassed
  (12.39…) and moved with it;
- no stage, objective, INFO, CELL or FLST record changes.

So almost every actor record carries values that move with time even when nothing
happens to the actor [observed]. A versus C (the same line of play, different session)
differs in 82 REFR records (scripted animation/state data, the moved items), 7 QUST
records (script variables), the player and every actor.

## 8. Open questions

- The meaning of global types 2 (105 unmapped trailing bytes after the death-count prefix),
  4, 5, 6, 9, 10 (trailing playback bytes after the station list), 11 and of the 9-byte tail.
- Non-player `ACHR`/`ACRE` AI packages, active magic effects, bit-2 ragdoll layouts, and
  complex variable-length actor extras (such as `0x5F` dismembered-limb blocks), plus the
  player record of nvfLanius and nvfSlides (flag 10, a different shape after the actor
  values), and any actor or item extra-data types beyond those listed in §§6.2 and 6.4.
- The trailing fields past offset 32 on created `0xFF` `REFR`s (when change flags beyond
  bit 1 are set), the full layout of created `ACRE`/`ACHR`/projectile forms, and global
  type 9, which grows with play.
- The QUST date stamp's day encoding.

None of these is needed for NVForever's own format. The offline `fnvtool save convert`
importer (§6.6, `crates/fnv-game/src/fos_convert.rs`) converts only the decoded sections
documented above, explicitly tags and counts every `[hypothesis]` field written into
`.nvfsave` in `ConversionReport::hypothesis_fields`, and truthfully counts every unmapped
global table (or unmapped trailing bytes of type 2), unpersisted item hotkey (`0x4A`),
unsupported created `0xFF` kind (`ACRE`/`ACHR`/etc.), missing-plugin form, unmapped
changed-form record, and ignored `.nvse` co-save in `skipped` [design; converted field
readings `[hypothesis]` where tagged in §§3–6.6]. The game runtime itself remains
`.nvfsave`-only.
