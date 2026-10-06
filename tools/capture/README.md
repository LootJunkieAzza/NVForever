# Capture tools (ground truth for the parity lab)

These tools produce **goldens**: JSON documents describing the user's own game files, which
the parity lab (`parity-lab/`) compares against the candidate `fnvtool` output. All JSON
shapes are defined in [`docs/cli-contract.md`](../../docs/cli-contract.md).

## Where outputs go: a private goldens directory, never the repo

Goldens are derived from copyrighted game data (record names, file lists, content hashes).
They must live in a **private directory outside the repository** and must **never be
committed**. Recommended layout:

```
~/fnv-private/goldens/            (Windows: C:\fnv-private\goldens\)
  install-inventory.json          inventory.py
  esm/<Plugin>.inventory.json     oracle_esm.py inventory
  esm/<Plugin>.stats.json         oracle_esm.py stats
  esm/<Plugin>.xedit.inventory.json   xedit/export_inventory.pas
  bsa/<Archive>.listing.json      oracle_bsa.py   (e.g. "DeadMoney - Main.bsa.listing.json")
```

Point the lab at it through its `goldens_dir` setting in an uncommitted local overlay or an
`FNVLAB_*` environment variable (see `parity-lab/fnvlab/config.py`), not in committed config.

## 1. `inventory.py` — install file inventory (Layer 0)

Lists every file under an install or `Data` folder with size and SHA-256 (`fnv.inventory/1`).

```
python3 tools/capture/inventory.py "<...>/Fallout New Vegas" -o ~/fnv-private/goldens/install-inventory.json
python3 tools/capture/inventory.py "<...>/Data" --no-hash      # sizes only, fast
```

## 2. `oracle_bsa.py` — independent BSA reader

A second, independent implementation of the BSA v104 reader, written only from
`docs/spec-notes/bsa.md` and the contract (not from `crates/`). Python 3.11+, standard
library only, single self-contained file — copy it anywhere.

```
python3 tools/capture/oracle_bsa.py "<Data>/Fallout - Misc.bsa" -o "~/fnv-private/goldens/bsa/Fallout - Misc.bsa.listing.json"
python3 tools/capture/oracle_bsa.py "<Data>/Update.bsa" --no-content-hash     # no decoding
```

Emits `fnv.bsa.listing/1`. `size`/`sha256` describe the decoded contents. It tolerates the
irregularities listed in the spec note (zlib stream without Adler-32 trailer, compressed
entry with declared size 0 and no zlib data), verifies every stored name hash against its
own hash implementation, and reports irregularities in `warnings` (data usable:
`file_hash_mismatch`, `folder_hash_mismatch`, `zlib_trailer_missing`,
`zlib_checksum_mismatch`, `compressed_block_empty`, `file_flags_padding_nonzero`,
`noncanonical_folder_name`, `data_not_contiguous`, `trailing_bytes`, ...) or `errors`
(something could not be read: `data_out_of_bounds`, `zlib_truncated`, `size_mismatch`, ...).
Exit code 1 only if the file is not a readable BSA at all.

Archives over 400 MB (`Fallout - Meshes/Textures/Textures2/Sound/Voices1`,
`LonesomeRoad - Main`, `OldWorldBlues - Main`) have to be captured on the machine that
holds them: copy `oracle_bsa.py` there and run it for each archive.

```
for f in "<Data>"/*.bsa; do python3 oracle_bsa.py "$f" -o "$HOME/fnv-private/goldens/bsa/$(basename "$f").listing.json"; done
```

## 3. `oracle_esm.py` — independent plugin reader

Independent ESM/ESP reader written only from `docs/spec-notes/esm.md` and the contract.
Single self-contained stdlib file. Walks groups with an explicit stack; FalloutNV.esm
(245 MB) takes about 6 s and ~0.7 GB RAM for `inventory`, under 1 s for `stats`.

```
python3 tools/capture/oracle_esm.py inventory "<Data>/FalloutNV.esm" -o ~/fnv-private/goldens/esm/FalloutNV.inventory.json
python3 tools/capture/oracle_esm.py stats     "<Data>/FalloutNV.esm" -o ~/fnv-private/goldens/esm/FalloutNV.stats.json
```

- `inventory` → `fnv.esm.inventory/1`: TES4 header summary plus every other record in file
  order (`formid` file-local `"0x%08X"`, `type`, first `EDID`/`FULL` as cp1252 text up to
  the first NUL or `null`, raw `flags`, `form_version`). Compressed records are decoded; a
  stream whose Adler-32 is wrong but which ends cleanly with the declared length is
  accepted and reported as `zlib_checksum_mismatch` (FalloutNV.esm LAND 0x00150FC0).
  `XXXX` size-extension subrecords are honoured. In addition to the contract's `errors`
  the document carries a `warnings` array (an oracle extension; the lab compares only
  `/header` and `/records`).
- `stats` → `fnv.esm.stats/1`: record/group counts, compressed/deleted counts, the
  HEDR self-check (`num_records == records + groups - 1`), top-level group order,
  per-type and per-group-type counts. Diagnostics go to stderr.

Header conventions: `version` is the HEDR f32 widened to a double without rounding (e.g.
`1.340000033378601`); `overridden_forms` is the number of u32s in ONAM (0 when there is no
ONAM); `other_subrecords` lists, in file order, every TES4 subrecord signature other than
HEDR/CNAM/SNAM/MAST/DATA/ONAM.

## 4. `xedit/export_inventory.pas` — xEdit (FNVEdit) export

A third, fully independent oracle: xEdit's own parser. Produces
`<Plugin>.xedit.inventory.json` in the `fnv.esm.inventory/1` schema. **Compare these on
`/records` only** — the header is best effort (see the script's header comment).

Step by step:

1. Install FNVEdit (xEdit 4.x, run as `FNVEdit.exe` or `xEdit.exe -fnv`).
2. Copy `tools/capture/xedit/export_inventory.pas` into xEdit's `Edit Scripts` folder.
3. Start FNVEdit. In the plugin selection dialog, tick the masters you want
   (e.g. FalloutNV.esm, DeadMoney.esm, HonestHearts.esm, ...) and click OK. Wait until the
   Messages tab says "Background Loader: finished".
4. In the left pane, select the plugin(s) to export (Ctrl+click for several).
5. Right-click → **Apply Script...**, choose `export_inventory`, click OK.
6. Enter the output directory when asked — a **private** folder outside the repo and
   outside the game folder; the default is `C:\NVForever-private\goldens\esm\`, which is
   exactly where the lab expects them (files are named `FalloutNV.xedit.inventory.json`
   etc.). Exporting FalloutNV.esm takes several minutes; progress
   is printed every 50,000 records.
7. Read the Messages tab. For each plugin it prints the FormID check line
   `N own (top byte M), K overrides/injected, J out of range`. Expected: out of range is 0,
   except GunRunnersArsenal.esm which may show 1 (its REFR stored as 0x02000801).
   If a plugin loaded at a load-order index different from its master count (e.g.
   HonestHearts.esm loaded after DeadMoney.esm) shows its new records as "out of range",
   the FormID function returned load-order IDs — switch `FormIDSource` in the script.
8. If you chose another folder, copy the JSON files into your private goldens directory
   (`esm/`).

If FNVEdit reports `'unit' expected but ',' found`, you have the first version of the
script, whose header comment contained JSON braces (a `}` ends a Pascal brace comment
early). The current script uses only `//` comments.

What to check on first use:

- File-local FormIDs (`FixedFormID`, not `GetLoadOrderFormID`); expected single
  difference vs. the Python oracle: GRA's 0x02000801 may be normalised to 0x01000801.
- `flags` of a compressed record (any LAND) must include 0x40000.
- `form_version` is not `null` (path `Record Header\Form Version`).
- FalloutNV.esm LAND 0x00150FC0 (bad zlib checksum) may produce an xEdit error and null
  `edid`/`full` for that record.
- The output is pure ASCII (non-ASCII characters escaped as `\uXXXX`).

## Tests

```
python3 -m unittest discover -s tools/capture/tests -v
```

The tests build tiny synthetic BSA and ESM files (compressed and trailer-less BSA entries,
empty compressed entry, hash mismatches, XXXX subrecord, compressed records including a bad
Adler-32, nested GRUPs, truncated inputs). No game data is needed or used.
