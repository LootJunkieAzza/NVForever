# Spec note: BSA archives (version 104, Fallout 3 / New Vegas)

Clean-room description in plain language. Implementation (`crates/fnv-bsa`) is written
from this note. Confidence tags: **[documented]** cites a public source, **[observed]**
was checked against the user's own install (FNV 1.4.0.525, Steam), **[hypothesis]**
needs verification.

Sources: UESP "Oblivion Mod:BSA File Format" and "Skyrim Mod:Archive File Format"
(content2.uesp.net mirror). Observations: all 15 archives under 400 MB in the user's
`Data` folder (15,072 files).

## Layout

All integers little-endian.

1. **Header, 36 bytes** [documented][observed]
   - magic `BSA\0`
   - version u32 = 104 in every FNV archive [observed]
   - offset u32 = 36 (start of folder records) [observed]
   - archive flags u32 (see below)
   - folder count u32, file count u32
   - total folder name length u32: sum over folders of (name length + 1) — the +1 is the
     terminating NUL; the length *byte* of each folder name is not counted [observed]
   - total file name length u32: size of the file-name block [observed]
   - file flags u16 (content type bits) + 2 bytes that are **not always zero**
     (`DeadMoney - Sounds.bsa` has 0x0378, `LonesomeRoad - Sounds.bsa` 0x02F9) —
     preserve them [observed]
2. **Folder records**, 16 bytes each, sorted ascending by hash [observed]:
   hash u64, file count u32, offset u32. The offset equals the absolute position of this
   folder's file-record block **plus the total file name length** [observed: true for
   every folder in every shipped archive].
3. **File-record blocks**, one per folder, in folder order. Each starts (if archive flag
   0x1) with the folder path as a length-prefixed string: one length byte that *includes*
   the trailing NUL, then the bytes, then NUL [observed]. Then `count` file records of
   16 bytes, sorted ascending by hash: hash u64, size u32, offset u32 (absolute) [observed].
   - size bit 30 (0x40000000) inverts the archive's default compression for that file
     [documented]; bits 0–29 are the stored size. Bit 31 never set in the corpus [observed].
4. **File-name block** (if archive flag 0x2): every file name (no directory), each
   NUL-terminated, in the same order as the file records [observed].
5. **File data**, immediately after the name block, in record order with no gaps
   [observed: true for 18 of the 21 archives in the owner's install; the exceptions are
   listed under Anomalies]. The writer packs blocks in record order; `rewrite` keeps an
   archive's own block order and name-block tail so the exceptions still rebuild.

## Archive flags

| Bit | Meaning | Status |
|---|---|---|
| 0x1 | folder names present | [documented][observed] |
| 0x2 | file names present | [documented][observed] |
| 0x4 | files compressed by default | [documented][observed: Misc archive] |
| 0x8, 0x10, 0x20 | "retain names/offsets" hints | [documented], no layout effect seen |
| 0x40 | Xbox 360 (big-endian) | [documented]; not supported |
| 0x80 | set on all mesh/texture-type archives (`0x83`) | meaning unknown; **not** "embedded names": data in those archives starts directly with file contents [observed] |
| 0x100 | file data prefixed by the file's full path as a u8-length string (no NUL), before any compression header | [documented]; set (flags 0x107) on `Fallout - Textures.bsa` and `Fallout - Textures2.bsa`, where all 22,054 files carry their own path [observed 2026-10-07 on the full install; an earlier note said "never set", written before those two archives were available] |

## Compressed file data

A u32 uncompressed size, then a zlib stream [observed]. Two tolerated irregularities:
- `Fallout - Misc.bsa`, `menus\s.txt`: the zlib stream has **no Adler-32 trailer**
  (stored size 12 = 4 + 8). The deflate data is complete and yields the declared 4 bytes.
  The original game must therefore not require the trailer [observed; game behaviour
  hypothesis].
- `menus\do_not_delete.txt`: compressed, declared size 0, no zlib data at all (block is
  just the 4-byte size) [observed].

## Name hash

64-bit value from a lower-cased, backslash-normalised name [documented: UESP BSA page
describes the algorithm; verified on every file and folder name in the corpus, observed]:

- For files, split off the extension at the last `.`; folders have no extension.
- Low 32 bits: last char of the stem | (second-to-last char << 8, only if stem length > 2)
  | (stem length << 16) | (first char << 24). Then OR a marker for four extensions:
  `.kf` 0x80, `.nif` 0x8000, `.dds` 0x8080, `.wav` 0x80000000.
- High 32 bits: rolling value `h = h * 0x1003F + c` (mod 2^32) over the stem's characters
  from index 1 up to (length − 2) exclusive, plus the same rolling value computed over the
  extension (including the dot), summed mod 2^32.

## Anomalies

- `Fallout - Invalidation.bsa` (83 bytes): its folder name has length byte 0 (no NUL,
  while the header's total folder name length is 1), its only file's data offset points 3
  bytes past the end of the name block, and 1 byte follows the (zero-length) data
  [observed]. It is probably generated by a mod manager for archive invalidation rather
  than shipped by Bethesda [hypothesis: its modification time differs from the shipped
  archives']. It parses, the reader reports `noncanonical_folder_name`,
  `data_not_contiguous` and `trailing_bytes`, and it cannot be rebuilt byte-identically.

- `Fallout - Textures.bsa` (shipped, 1,196,767,196 bytes, 11,470 files): the data blocks
  are contiguous and fill the file exactly, but are **not stored in record order** (the
  first record's block is at 324,286,810) [observed: `fnvtool bsa verify` on the owner's
  install, 2026-10-06]. The order actually used is unknown [hypothesis: the packing
  tool's input order]. The reader reports `data_not_in_record_order`.
- `Fallout - Voices1.bsa` (shipped, 1,602,331,889 bytes, 105,517 files): the file-name
  block is **105 bytes longer** than the names it holds; `total_file_name_length` and
  every folder offset count those bytes, and the data starts after them [observed: same
  run; the rebuilt header differed at offset 28 and the rebuild was 105 bytes short].
  Their content is not yet known (the reader's `name_block_slack` warning says whether
  they are all NUL).

## Unknowns (need the user's help)

- Archives over 400 MB (`Fallout - Meshes/Textures/Textures2/Sound/Voices1`,
  `LonesomeRoad - Main`, `OldWorldBlues - Main`) could not be transferred. Their listings
  were validated on the owner's machine on 2026-10-06 (all match `oracle_bsa.py`); see
  Anomalies for the two that did not rebuild with the canonical rule.
- Loose-file precedence (see Precedence below) and the order of `Update.bsa` against the
  DLC archives.
- Meaning of archive flag 0x80 and of file-flag padding bytes.

## Precedence (the same path in several archives)

Settings [observed]: `Fallout_default.ini`, the owner's `Fallout.ini` and
`FalloutPrefs.ini` all have `SArchiveList=Fallout - Textures.bsa, Fallout - Textures2.bsa,
Fallout - Meshes.bsa, Fallout - Voices1.bsa, Fallout - Sound.bsa, Fallout - Misc.bsa`,
`bInvalidateOlderFiles=1`, `SInvalidationFile=ArchiveInvalidation.txt`, plus
`bUseArchives=1`, `bCheckRuntimeCollisions=0` and three `iRetain…Table=1` lines. `Update.bsa` is
not in the list, and no plugin is named `Update`; the game uses it all the same (below).

Survey [observed 2026-10-09, `fnvtool bsa list` of all 22 archives with content hashes]:
420 paths are present in more than one archive, 300 of them with different contents.
`Update.bsa` replaces 35 files of `Fallout - Meshes.bsa` and 2 of `Fallout - Textures2.bsa`;
the DLC `<Plugin> - Main.bsa` archives replace base meshes too (e.g. `OldWorldBlues -
Main.bsa` has its own `architecture\nellishangar\nvnellishangar.nif`, using
`nellis afb\nellishanger_d.dds` instead of the base copy's Citadel wall textures and
window band). Many differences are collision-only (Havok blocks) and invisible.

Test [observed 2026-10-09, plugin v7 screenshots from save `nvftest`, 1280 × 720, noon,
NVWastelandClear; `nvforever` rendered each recorded camera twice with the archive
order replaced through `NVF_ARCHIVE_ORDER`; compared over the pixels where the two
renders differ (mean absolute sRGB difference to the game, lower is closer; SSIM of that
region, higher is closer)]:

| Camera (object) | Hypothesis A (game order below) | Hypothesis B | A | B |
|---|---|---|---|---|
| Nellis hangar, `nvnellishangar.nif` | `OldWorldBlues - Main` over `Fallout - Meshes` | base over DLC | 15.7 / 0.716 | 24.4 / 0.680 |
| Camp Forlorn Hope tower, `ncr\ncr_guardtower01.nif` | `Update` over `Fallout - Meshes` | base over `Update` | 29.7 / 0.577 | 60.6 / 0.561 |
| Westside, `freeside\atomicwranglerext.nif` | `Update` over `Fallout - Meshes` | base over `Update` | 6.8 / 0.777 | 21.1 / 0.741 |

All three favour A. Rule used by `crates/fnv-vfs` (lowest precedence first):
1. the `SArchiveList` archives in list order [the order among them never matters for
   the shipped data: the six share no path, observed];
2. `Update.bsa` — **overrides the base archives** [observed, two objects above];
3. each active plugin's own archives (`<plugin stem> - *.bsa`, `<plugin stem>.bsa`) in
   load order — a **DLC archive overrides the base archives** [observed, Nellis hangar];
4. loose files [hypothesis: no loose mesh or texture exists in the owner's `Data`
   (only music, radio songs, shader packages and the intro video), and the project never
   writes into the game folder, so this stays untested; `bInvalidateOlderFiles=1` is
   commonly said to make loose files win [hypothesis]].

File timestamps do not decide it: `Update.bsa` and the DLC archives are dated July 2025,
older than the base archives' modification times (6 October 2026) [observed],
and still override them. Still [hypothesis]: `Update.bsa` before the DLC archives (they
share only `characters\_1stperson\1hpaim.kf` with different contents, not testable by a
screenshot) and DLC against DLC in load order (only DLC-worldspace files differ).
`Fallout - Invalidation.bsa` is in no list and named after no plugin; the VFS ignores it
[hypothesis that the game does too].
