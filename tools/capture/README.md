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
  sim/containers.json             oracle_sim.py containers (ten masters)
  sim/items.json                  oracle_sim.py items (ten masters)
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
6. Files are written to the `OutputDir` constant at the top of the script, default
   `C:\fnv-private\goldens\esm\` (the `esm\` folder of the recommended goldens
   directory, exactly where the lab expects them; edit the constant to use another private
   folder outside the repo and the game folder). They are named
   `FalloutNV.xedit.inventory.json` etc. There is no folder dialog: FNVEdit's `InputQuery`
   garbled its default and returned an empty string [observed]. Exporting FalloutNV.esm
   takes several minutes; progress is printed every 50,000 records.
7. Read the Messages tab. For each plugin it prints the FormID check line
   `N own (top byte M), K overrides/injected, J out of range`. Expected: out of range is 0,
   except GunRunnersArsenal.esm which may show 1 (its REFR stored as 0x02000801).
   If a plugin loaded at a load-order index different from its master count (e.g.
   HonestHearts.esm loaded after DeadMoney.esm) shows its new records as "out of range",
   the FormID function returned load-order IDs — switch `FormIDSource` in the script.
8. Convert the exports to UTF-8 JSON (the script writes non-ASCII text in the ANSI code
   page because FNVEdit's interpreter cannot escape it), then they are ready for the lab:
   `python tools\capture\xedit\finish_export.py C:\fnv-private\goldens\esm`
9. If you changed `OutputDir`, copy the JSON files into your private goldens directory
   (`esm/`).

If FNVEdit reports `'unit' expected but ',' found`, you have the first version of the
script, whose header comment contained JSON braces (a `}` ends a Pascal brace comment
early). The current script uses only `//` comments.
If it reports `Undeclared Identifier 'IncludeTrailingPathDelimiter'`, `Type mismatch` on
line 53, shows a garbled (CJK-looking) folder dialog or logs `Writing to` with no folder,
you have an older version; update the script.

What to check on first use:

- File-local FormIDs (`FixedFormID`, not `GetLoadOrderFormID`); expected single
  difference vs. the Python oracle: GRA's 0x02000801 may be normalised to 0x01000801.
- `flags` of a compressed record (any LAND) must include 0x40000.
- `form_version` is not `null` (path `Record Header\Form Version`).
- FalloutNV.esm LAND 0x00150FC0 (bad zlib checksum) may produce an xEdit error and null
  `edid`/`full` for that record.
- After `finish_export.py` the output is pure ASCII (non-ASCII characters escaped as `\uXXXX`).

Known disagreements with xEdit (first full run on the owner's install, 2026-10-06; the
lab compares `type`, `edid`, `full`; fnvtool and `oracle_esm.py` agree on all of them):

- GunRunnersArsenal.esm REFR stored as 0x02000801 (a test-cell placement of the
  Faderator): xEdit reports 0x01000801 [observed]. We report the stored value.
- Records whose EDID/FULL subrecord is absent from the file, where xEdit's view still
  shows the field as an empty, non-greyed row and the script exports `""` instead of
  `null` [observed in FNVEdit; the cause, xEdit materialising fields its definitions mark
  as required, is a hypothesis]: FalloutNV.esm DIAL 0x00152CCA (`edid`), DeadMoney.esm
  RACE 0x01005819 (`full`), OldWorldBlues.esm 0x0101533F (`full`, not inspected).
- These four scenarios therefore fail by one record each. Accepting them is the owner's
  decision (parity-lab/AGENT.md: agents never edit goldens or scenarios).

## 5. `oracle_nif.py` / `oracle_dds.py` — independent mesh and texture readers (Layer 2)

Written only from `docs/spec-notes/nif.md`, `dds.md`, `bsa.md` and the Layer 2 section of the
contract (not from `crates/` or `schemas/`). Standard library only; they import
`oracle_bsa.py` (archive decoding, including the flag 0x100 path prefix of the two texture
archives) and must sit next to it. Archives are memory-mapped, so the 1 GB ones work too.

```
python tools/capture/oracle_nif.py stats "<Data>/Update.bsa" --out "C:/fnv-private/goldens/nif/Update.bsa.nif.stats.json"
python tools/capture/oracle_nif.py scene "<Data>/Update.bsa" --out "C:/fnv-private/goldens/nif/Update.bsa.nif.scene.json"
python tools/capture/oracle_dds.py list  "<Data>/Fallout - Textures.bsa" --out "C:/fnv-private/goldens/dds/Fallout - Textures.bsa.dds.listing.json"
```

- `stats` → `fnv.nif.stats/1`: header fields of every `.nif`/`.kf` (archive record order).
- `scene` → `fnv.nif.scene/1`: files with a block-size table are split into blocks by the
  size table; nodes/geometry objects are read up to their child list, NiTriShapeData /
  NiTriStripsData are decoded completely and must consume their block exactly. A file
  whose container or any analysed block does not decode is listed in `skipped` (reason
  `container: ...` or `block N (Type): ...`) and contributes no objects or shapes.
- `list` → `fnv.dds.listing/1`; errors are exactly `bad magic` / `short header`. Size
  mismatches are printed to stderr.

Goldens layout: `nif/<Archive>.nif.stats.json`, `nif/<Archive>.nif.scene.json`,
`dds/<Archive>.dds.listing.json`.

## 6. `oracle_world.py` — independent world reader (Layer 3)

Written only from `docs/spec-notes/esm.md`, `world.md` and the Layer 3 section of the
contract (not from `crates/`, `engine/` or `schemas/`). Standard library only; imports
`oracle_esm.py` (zlib tolerance, `XXXX` subrecords, cp1252 text) and must sit next to it.
Plugins are memory-mapped and walked once each; only the selected cells, their references,
the references' base objects and (for `heights`) the cells' LAND records are decoded.
`refs --world WastelandNV` over the ten masters takes about 5 s, `refs --interiors` about
6 s, `heights --world WastelandNV` about 14 s.

```
python tools/capture/oracle_world.py refs    "<Data>" FalloutNV.esm DeadMoney.esm ... --world WastelandNV -o "C:/fnv-private/goldens/world/WastelandNV.refs.json"
python tools/capture/oracle_world.py refs    "<Data>" FalloutNV.esm DeadMoney.esm ... --interiors     -o "C:/fnv-private/goldens/world/interiors.refs.json"
python tools/capture/oracle_world.py heights "<Data>" FalloutNV.esm DeadMoney.esm ... --world 0x000DA726 -o "C:/fnv-private/goldens/world/WastelandNV.heights.json"
```

- Plugins are given in load order and echoed as `load_order`. `--world` takes an Editor ID
  (case-insensitive) or a load-order FormID `0x%08X`; `--schema` is accepted and ignored.
  Only the whole-worldspace and `--interiors` selections are implemented (no `--cell`,
  `--x/--y`).
- `refs` → `fnv.world.refs/1`, `heights` → `fnv.world.heights/1`. FormIDs are mapped to the
  load order through each plugin's master list; the last definition wins, including the
  cell a reference sits in and the cell's LAND. A record whose own FormID has a master
  index beyond its plugin's master list (GunRunnersArsenal.esm REFR 0x02000801) is
  dropped and reported.
- `model` follows the contract rule: surrounding whitespace removed (`null` if empty),
  ASCII letters lower-cased, `/` → `\`, leading `\` removed (TREE models are stored as
  `\WastelandShrub01.spt`), then `trees\` (`.spt`) or `meshes\` prefixed unless present.
- Points where the notes leave a choice open are marked `# SPEC-AMBIGUITY` in the source.
- Diagnostics (first 20 per code) go to stderr; exit code 1 when a plugin cannot be read or
  the worldspace does not exist, 2 for usage errors.

## 7. Reference frames from the original game (Layer 3, image scenarios)

Screenshots of the original game at fixed cameras, compared with `nvforever`'s render of
the same camera by the lab's `image` comparator. You take them in your own game; they
stay in the private goldens directory. Console commands below are the game's own
[documented: community console references; verify on your install]; NVForever never
changes your game files or `Documents\My Games\FalloutNV`.

**Setup (once).** Pick how to save screenshots: the game's own key needs
`bAllowScreenShot=1` in the `[Display]` section of your `Fallout.ini` (you edit it
yourself) and writes `ScreenShot<N>.bmp` into the game folder; Steam's F12 also works if
you enable "Save an uncompressed copy" (PNG). Avoid JPEG. Use the same resolution for
every capture; 16:10 or 16:9 both work.

**Per frame.**
1. Load a save, then go to the place: `coc <CellEditorID>` (e.g. `coc Goodsprings`, the
   exterior cell at grid −18, 0) or walk there.
2. Freeze the variables: `set timescale to 1`, `set gamehour to 12` (or another hour),
   `fw <weather FormID>` (e.g. NVWastelandClear; look up its FormID with `help
   NVWastelandClear 4`), wait a few seconds for the weather to settle.
3. Stand still in first person, then record the player's position and angles:
   `player.getpos x`, `player.getpos y`, `player.getpos z`, `player.getangle z`
   (heading), `player.getangle x` (pitch). To set exact values instead:
   `player.setpos x <v>` etc. and `player.setangle z <deg>`, `player.setangle x <deg>`.
4. Note the field of view (`fov` with no argument prints it on some setups; default 75
   [hypothesis]).
5. Hide the HUD and console: `tm`, close the console, take the screenshot, `tm` again.
6. Copy the image to `<goldens>\frames\` and add an entry to
   `<goldens>\frames\manifest.json` (format in `parity-lab/tools/gen_layer3_frames.py`):
   id, image file name, `player_pos`, `heading`, `pitch`, `hour`, `weather`, `fov`, and
   `masks` for regions that cannot match (NPCs or creatures walking by, particles).

A first set of 5-10 frames is plenty: Goodsprings at noon and at 19:00, a view towards
the Strip, Lake Mead shore, a hill with distant LOD in view.

**Then:** `python parity-lab/tools/gen_layer3_frames.py <goldens>` writes the oracle
documents and `<goldens>\frames\scenarios\layer3_frames.json`; add
`"<goldens>/frames/scenarios"` to `scenarios_dirs` in `parity-lab/lab.local.json` and build
the engine (`cd engine && cargo build --release`). `python -m fnvlab run --layer 3` then
renders each camera headless and compares. `min_ssim` in the manifest is provisional
(0.6); it is fixed with you after the first comparisons and then not changed to make
scenarios pass. The camera is placed `eye_height` (default 120 units [hypothesis]) above
the player's position; the first comparisons will tell us the right value.

## 8. `script_functions.py` — script-function catalogue (scripting groundwork)

Builds `schemas/script/functions.json` and `docs/spec-notes/script-functions-catalogue.md`
from a private GECK wiki export (structured template fields only), the masters' compiled
scripts (opcodes and usage by lining up SCDA with SCTX) and, optionally, xEdit's
condition-function table. Unlike the oracles, its outputs **are committed**: they hold only
names, parameter metadata, opcodes, counts and our own paraphrases
(`schemas/script/notes.json`). Details: `docs/spec-notes/script-functions.md`.

```
python tools/capture/script_functions.py --wiki C:/fnv-private/geckwiki/functions.xml ^
  --data "<FNV>/Data" --xedit <path>/wbDefinitionsFNV.pas --notes schemas/script/notes.json
```

## 9. `oracle_sim.py` — independent simulation oracle (Layer 4)

Written only from `docs/spec-notes/simulation.md` (§3, §7) and `docs/spec-notes/esm.md`
(not from `crates/` or `schemas/`). Standard library only; it imports the container
helpers (tolerant zlib, XXXX-aware subrecord iterator, cp1252 strings) from
`oracle_esm.py` and must sit next to it. One linear pass per plugin; only CONT records are
parsed in full and LIGH records for their DATA flags (plus, for `items`, the records of the
inventory item types), so the ten masters take about a second.

```
python tools/capture/oracle_sim.py containers "<Data>" FalloutNV.esm DeadMoney.esm HonestHearts.esm OldWorldBlues.esm LonesomeRoad.esm GunRunnersArsenal.esm ClassicPack.esm MercenaryPack.esm TribalPack.esm CaravanPack.esm -o C:/fnv-private/goldens/sim/containers.json
python tools/capture/oracle_sim.py items "<Data>" FalloutNV.esm DeadMoney.esm HonestHearts.esm OldWorldBlues.esm LonesomeRoad.esm GunRunnersArsenal.esm ClassicPack.esm MercenaryPack.esm TribalPack.esm CaravanPack.esm -o C:/fnv-private/goldens/sim/items.json
```

- `containers` → `fnv.sim.containers/1` (golden `sim/containers.json`): one object per
  winning, non-deleted CONT, ordered by global FormID, with `edid`, `name`, `respawns`,
  `script` and the CNTO `entries` classified as `item` / `leveled_item` / `other:<SIG>` /
  `engine_builtin` / `missing`. Load order = the plugins in the order given; masters are
  matched to the load order by file name, case-insensitively.
- Oracle extensions at top level (the lab compares only `/containers`): `load_order`,
  `summary` (forms defined/deleted, respawning containers, containers without CNTO,
  entries per class, null and unmappable entries) and `warnings` (`{code, detail}`, capped
  at 50 per code). On the ten masters the only warning is `unmappable_formid` for the
  GunRunnersArsenal.esm REFR stored as 0x02000801 (ignored as a form).
- Choices where the contract is silent: a null item is a *stored* FormID of 0; a master
  that is not in the load order makes its FormIDs unmappable (entries omitted, `script`
  null); a SCRI that cannot be globalised gives `script: null`; a CNTO shorter than 8 bytes
  is skipped, a longer one uses its first 8; the first EDID/FULL/SCRI/DATA counts; a LIGH
  without DATA or with DATA shorter than 16 bytes is not carriable; a CONT whose compressed
  data cannot be decoded is still listed, with null fields and no entries; within one
  plugin a later record with the same FormID replaces an earlier one.
- `items` → `fnv.sim.items/1` (golden `sim/items.json`): one object per winning,
  non-deleted inventory item record (LIGH only when carriable) with `sig`, `edid`, `name`,
  `value`, `weight`, `health`, `biped_flags` at the byte offsets of the contract table.
  Top-level extras as for `containers` (`summary` counts per type and null fields;
  warnings `item_field_short`, `item_subrecord_repeated`, `item_data_unreadable`,
  `item_weight_not_finite`). On the ten masters: 3,972 items, no warnings beyond the
  GunRunnersArsenal one above.

## Tests

```
python3 -m unittest discover -s tools/capture/tests -v
```

The tests build tiny synthetic BSA, ESM (single plugins and small load orders), NIF and DDS files (compressed and trailer-less BSA entries,
empty compressed entry, hash mismatches, XXXX subrecord, compressed records including a bad
Adler-32, nested GRUPs, truncated inputs) and, for `oracle_world.py`, a master plus a
dependent plugin (reference override and move between cells, references added to master
cells, LAND override, persistent cell, interior cell, compressed records, XESP, XSCL,
engine-defined base, `.spt` model, unmappable FormID). No game data is needed or used.
