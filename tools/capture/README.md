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

## 9. `oracle_collide.py` — independent collision reader (Layer 4)

Written only from `docs/spec-notes/collision.md` (section 5 is the contract), `nif.md`,
`bsa.md` and the niftools `nif.xml` read as documentation for the Havok block layouts (not
from `crates/`, `schemas/` or `engine/`). Standard library only; imports `oracle_nif.py`
(container header, archive walking) and through it `oracle_bsa.py`, so all three must sit
together. `Fallout - Meshes.bsa` takes about 10 s.

```
python tools/capture/oracle_collide.py objects "<Data>/Fallout - Meshes.bsa" -o "C:/fnv-private/goldens/collide/Fallout - Meshes.bsa.collide.objects.json"
python tools/capture/oracle_collide.py objects "<Data>/Update.bsa" --filter clutter     # paths containing "clutter"
```

- `objects` → `fnv.collide.objects/1`: every `.nif` in archive record order. Files with a
  block-size table are split into blocks by it; scene objects are read up to their
  collision reference (nodes also their child list), and every collision object, body and
  shape block that is followed is decoded completely and must consume its stored size
  exactly. A file is listed in `skipped` (and contributes no objects or phantoms) when its
  container does not decode (`container: ...`), it has no size table (`no block-size
  table ...`), a followed block does not decode (`block N (Type): ...`), a reference is
  missing (`missing block: ...`), a shape/body/collision-object type is unsupported
  (`unsupported ...`), packed vertices are compressed, or a transform matrix is not a
  rotation times a uniform scale. Reasons are free text; the lab compares paths only.
- Bounds are in the mesh root's frame: the root node's own transform is not applied
  (collision.md §4); scene objects below it compose as usual.
- `--filter` keeps files whose path contains the text (any case); `--schema` is accepted
  and ignored.
- Points where the spec leaves a choice open are marked `# SPEC-AMBIGUITY` in the source.

Goldens layout: `collide/<Archive>.collide.objects.json`, e.g.
`collide/Fallout - Meshes.bsa.collide.objects.json`.

## 10. `oracle_phys.py` — independent terrain heights (Layer 4 physics)

Written only from `docs/spec-notes/physics.md` (§3 and the `phys heights` contract in §7),
`world.md`, `esm.md` and `docs/cli-contract.md` (not from `crates/`, `engine/`, `schemas/`
or `parity-lab/`). Standard library only; imports `oracle_world.py` (plugin walking,
load-order FormIDs, cell and LAND selection, VHGT integration) and through it
`oracle_esm.py`, so all three must sit together. `heights --world WastelandNV` over the ten
masters takes about 3 s (warm file cache).

```
python tools/capture/oracle_phys.py heights "<Data>" FalloutNV.esm DeadMoney.esm ... --world WastelandNV -o "C:/fnv-private/goldens/phys/WastelandNV.heights.json"
```

- `heights` → `fnv.phys.heights/1`: one entry per exterior grid cell of the worldspace
  whose CELL has a LAND record (the persistent cell is not a grid cell), sorted by (x, y);
  `heights` holds the terrain height under the eight fixed points of the contract, or
  `null` when the LAND has no (valid) VHGT. Unlike `oracle_world.py heights`, cells without
  heights are listed.
- The surface is stated analytically from the 33 × 33 integrated VHGT grid: each grid
  square is split along its south-west → north-east diagonal (`fu ≥ fv` south-east
  triangle, else north-west); at a vertex the height is the vertex height. Computed in
  Python floats; the lab compares with an absolute tolerance of 0.5 units, keyed on `grid`.
- Plugins are given in load order and echoed as `load_order`; `--world` takes an Editor ID
  (case-insensitive) or a load-order FormID `0x%08X`; `--schema` is accepted and ignored.
- Points where the notes leave a choice open are marked `# SPEC-AMBIGUITY` in the source.
- Diagnostics go to stderr; exit code 1 when a plugin cannot be read or the worldspace does
  not exist, 2 for usage errors.

Goldens layout: `phys/<World>.heights.json`, e.g. `phys/WastelandNV.heights.json`.

## 11. `oracle_sim.py` — independent simulation oracle (Layer 4)

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

## 10. `nvf_capture` + `capture_steps.py` — in-game captures (Layer 4+)

An xNVSE plugin for the **original game** (not part of NVForever) that runs a scenario's
steps inside FNV and records what the game reports, so trace and leveled-list goldens need
no hand-typed console sessions. Interface, protocol and file formats:
`docs/spec-notes/xnvse-capture.md`. The plugin reads no game memory (only xNVSE's plugin
interface and the game's script language) and writes only to the capture directory.

**One-time setup (owner, by hand — tools never write into the game folder):**
1. Install xNVSE 6.4.9 or later following its own instructions (github.com/xNVSE/NVSE,
   Steam version). The plugin declines to load on older xNVSE or other game versions.
2. Build the plugin (32-bit):
   ```
   rustup target add i686-pc-windows-msvc
   cd tools/capture/nvf_capture && cargo build --release
   ```
   and copy `target/i686-pc-windows-msvc/release/nvf_capture.dll` to
   `<FNV>\Data\NVSE\Plugins\`. Optional: `nvf_capture.ini` there with `dir=<path>` to move
   the capture directory away from `C:\fnv-private\capture\`.
3. Use a **test character** (and back up saves): jobs remove items, change levels,
   attributes and globals.

**Per scenario:**
```
python tools/capture/capture_steps.py actions parity-lab/inputs/layer4/goodsprings_weight_and_time.actions.json --data "<Data>" --start "Load the pinned save in Doc Mitchell's house (new level-1 character, no perks)." -o C:/fnv-private/capture/jobs/goodsprings_weight_and_time.steps.json
python tools/capture/capture_steps.py cases parity-lab/inputs/layer4/leveled_basic.cases.json --data "<Data>" --draws 300 --start "Load a test save in a quiet interior." -o C:/fnv-private/capture/jobs/leveled_basic.steps.json
```
Start the game, load the save named in `--start`; the plugin announces the first pending
job (top left). **End** starts it, **Home** skips it; manual steps (waiting, walking
through a door) show their instruction and continue on **End**. Output:
`C:\fnv-private\capture\out\<job>.<time>.jsonl`; finished jobs move to `jobs\done\`; a log
is in `nvf_capture.log`. Then build the golden:
```
python tools/capture/capture_steps.py trace   C:/fnv-private/capture/out/goodsprings_weight_and_time.<time>.jsonl -o C:/fnv-private/goldens/sim/goodsprings_weight_and_time.trace.json
python tools/capture/capture_steps.py samples C:/fnv-private/capture/out/leveled_basic.<time>.jsonl --cases parity-lab/inputs/layer4/leveled_basic.cases.json -o C:/fnv-private/goldens/sim/leveled_basic.samples.json
```
`--data` (the game's Data folder, read-only) resolves Editor IDs to FormIDs: the game does
not know Editor IDs at run time. Forms in jobs are named by plugin file and local FormID,
so the game's own load order does not matter; output FormIDs are mapped back through the masters' mod indices,
which every job records first (`GetModIndex`). A step that fails (e.g. a script that does not compile) is recorded
as an error and the job continues; check the golden for `error` fields before using it.
**Unattended runs (`capture_run.py`, plugin version 2):** no one needs to be at the game
for jobs without manual steps. One-time: make a save for captures with a plain name from
the console (`save nvftest`), and copy the rebuilt DLL as in step 2. Then:
```
python tools/capture/capture_run.py --game "<FNV folder>" --save nvftest C:/path/to/<job>.steps.json [...] -o C:/fnv-private/capture/run.json
```
It copies the jobs into `jobs\`, writes `auto.json` (the plugin's instruction to load the
save from the main menu, start jobs by itself and quit when done; xnvse-capture.md §4),
starts `nvse_loader.exe` (Steam must be running), waits, and writes a report: each job's
status (`ended`, `aborted`, `running` = cut off, `missing`), its output file and number of
failed steps, problems (game did not start, crashed, old plugin, timeout) and the
plugin's log lines from this run. `--trace-dir C:/fnv-private/goldens/sim` also builds the
trace goldens of ended jobs. Jobs with manual steps are refused unless `--attended`
(someone presses End after each); other jobs already pending in `jobs\` are refused
unless `--include-pending`. Exit code 0 means every job ended. The default console lines
for loading and quitting (`load <save>`, `qqq`) worked on the owner's install
(xnvse-capture.md §6); `--menu-console` / `--quit-console` change them. The read-only
trial, to check a new setup:
```
python tools/capture/capture_run.py --game "<FNV folder>" --save nvftest tools/capture/jobs/aa_auto_trial.steps.json
```
**Scripted substitutes** for manual steps (xnvse-capture.md §7, plugin version 3):
`capture_steps.py actions … --time hour|timescale` replaces the Wait menu and `--settle
auto` replaces the End confirmation after a door. They are used for a scenario only once
a capture made with them agrees with the golden made by hand:
```
python tools/capture/capture_steps.py compare C:/fnv-private/goldens/sim/<name>.trace.json <substitute>.trace.json --tolerance <t>
```
**AI observer (`observe.py`, plugin version 6):** a bot that watches NPCs for spans of
game time and records their AI state (package, procedure, targets, combat, detection,
sitting/sleeping, position, …) for the actors near the player and for named actors
wherever they are; it moves the player only by teleport and by holding Forward through
xNVSE. Design, fields and their sources: `docs/spec-notes/ai-observer.md`. First the
diagnostic job (after copying the current DLL), then a campaign:
```
python tools/capture/capture_run.py --game "<FNV folder>" --save nvftest tools/capture/jobs/aa_observe_diag.steps.json
python tools/capture/observe.py run tools/capture/observe/goodsprings.campaign.json --game "<FNV folder>" --data "<Data>" --timeline C:/fnv-private/capture/observe/goodsprings.timeline.json
python tools/capture/observe.py report C:/fnv-private/capture/out/obs_goodsprings_town_day.<time>.jsonl --data "<Data>" --text
```
`run` restarts the game after a crash or hang and retries the session that was cut off
(`--retries`, default 2); its report goes to `C:\fnv-private\capture\observe\<campaign>\`.

Self-tests: `cargo test --release` in `nvf_capture` (32-bit; step runner, automatic mode,
observe step, layouts), `tests/test_capture_steps.py`, `tests/test_capture_run.py` and
`tests/test_observe.py`.

## Tests

```
python3 -m unittest discover -s tools/capture/tests -v
```

The tests build tiny synthetic BSA, ESM (single plugins and small load orders), NIF and DDS files (compressed and trailer-less BSA entries,
empty compressed entry, hash mismatches, XXXX subrecord, compressed records including a bad
Adler-32, nested GRUPs, truncated inputs) and, for `oracle_world.py`, a master plus a
dependent plugin (reference override and move between cells, references added to master
cells, LAND override, persistent cell, interior cell, compressed records, XESP, XSCL,
engine-defined base, `.spt` model, unmappable FormID). For `oracle_collide.py` they build
NIFs with collision on the root, on a transformed and scaled child node and on a geometry
object, bhkRigidBody vs bhkRigidBodyT, every supported shape (box, sphere, capsule, convex
vertices, multi-sphere, list, transform, convex transform, MOPP over packed strips with
sub-parts, NiTriStrips shape), phantoms, and each skip reason. No game data is needed or
used.
