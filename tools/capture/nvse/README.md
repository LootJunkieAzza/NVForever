# Clean-Room NVSE / Extender Black-Box Probe Harness

This package provides a strictly read-only black-box NVSE/extender probe script
emitter, declarative console-only clean-room probe catalogs, read-only log
verifier, offline three-tier save/load persistence classifier (`SAVED` /
`SESSION_ONLY` / `CLEARED_ON_LOAD`), and clean-room `[observed: ...]` citation
formatter for verifying *Fallout: New Vegas* script extender behavior without
ever committing game assets, saves, logs, or private machine paths, and without
writing into the game folder or `Documents\My Games\FalloutNV`.

## Clean-Room & Read-Only Charter (`tools/capture/README.md` §10)

- **100% black-box runtime observation:** Every probe executes public GECK/NVSE
  script commands via `CompileScript` isolated user-defined functions (UDFs)
  against an unmodified *Fallout: New Vegas* `1.4.0.525` (`040020D0`) install
  with only the 10 official base masters and named public extender releases. No
  binary, DLL, PDB, disassembly, decompilation, or third-party extender source
  code is consulted.
- **Strictly read-only on game, INI, and Saves folders:** No tool in
  `tools/capture/nvse/` ever writes, creates, modifies, renames, or deletes any
  file or directory inside `--game-dir`, `Data\`, `NVSE\`, `Fallout*.ini`,
  `plugins.txt`, `loadorder.txt`, or any Saves folder, and no tool launches
  `FalloutNV.exe` automatically.
- **Resolved real-path boundary guard:** `emit` (and `--json-out` / `--md-out`
  / `--out`) resolves real paths (`os.path.realpath`) and refuses any output
  path that equals, sits inside, or contains `--game-dir` or `--saves-dir`, or
  that contains live game/config sentinels (`FalloutNV.exe`, `nvse_loader.exe`,
  `Data/FalloutNV.esm`, `Fallout.ini`, `plugins.txt`).
- **Console-only probe scripts (zero script-driven file writes):** Every emitted
  probe script logs `PROBE|<id>|<result>` strictly via `PrintToConsole` (there
  is no file-output mode). `validate` and `emit` reject all game-folder and
  Saves-folder file-writing commands (`WriteStringToFile`, `WriteArrayToFile`,
  `con_Save`, `WriteToJSON`, `SetINI*`, `RemoveINI*`, `SaveCachedIniFile`,
  `CloseFileSO`, `ExcelWrite*`, `ExcelArrayToFile`, `ExcelErase*`,
  `ExcelSetSheetName`, `TextToQRCode`, `DumpLoadOrder`, `ModLogPrint`, etc.).
  Manual staging still requires copying the emitted scripts (and any optional
  read-only input fixtures under `fixtures/`) into an isolated staging copy by
  hand; when run, the probe scripts themselves perform zero file or save writes
  (normal xNVSE/extender plugin startup logs such as `nvse.log` are produced by
  the extenders themselves).
- **Automated privacy leak gate:** `validate`, `verify`, and `cite` reject
  absolute Windows drive paths, POSIX `/home/` or `/Users/` paths, UNC paths,
  Windows profile markers, `.pdb` names, unredacted `.fos` / `.nvse` save
  filenames, and ASCII control characters.
- **Zero external dependencies:** Pure Python 3.11+ standard library only.

## Reference Environment

Probes in `catalogs/` target:

- **Base game:** *Fallout: New Vegas* `1.4.0.525` (`040020D0`) with the 10
  official base masters only:
  `FalloutNV.esm`, `DeadMoney.esm`, `HonestHearts.esm`, `OldWorldBlues.esm`,
  `LonesomeRoad.esm`, `GunRunnersArsenal.esm`, `CaravanPack.esm`,
  `ClassicPack.esm`, `MercenaryPack.esm`, `TribalPack.esm`.
- **Public script extenders:** `xNVSE 6.4.9`, `JIP LN NVSE 57.30` (`5730`),
  `ShowOffNVSE Plugin 184`, `JohnnyGuitarNVSE 528`, `kNVSE 37`,
  `SUP NVSE Plugin 855`, `yUI 140` (v1.4d), `UI Organizer Plugin 230`,
  `hNVSE 120`, `AnhNVSE 131`, and `lStewieAl's Tweaks 1005`.
- **No bare EditorIDs:** Every vanilla form in runtime-compiled probe scripts is
  resolved by plugin + local FormID (`GetFormFromMod "FalloutNV.esm" "<hex>"`),
  enforced automatically by `harness.py validate`.

## Layout

- `harness.py` — stdlib-only CLI with five subcommands:
  - `list` — list catalogs, batches, probe counts, and required extenders.
  - `validate` — verify catalog schemas, global probe ID uniqueness, relative
    `docs/spec-notes/` paths, zero bare EditorIDs, zero forbidden game/Saves
    file-writing commands, and zero privacy leaks.
  - `emit` — render console-only (`PrintToConsole`) isolated `.gek` UDFs, loop
    runner, `gr_nvseprobe.txt`, any declarative read-only input fixtures, and
    `MANUAL_SETUP.txt` strictly into `--out-dir` (refuses persistence catalogs
    because `con_Save` writes into the Saves folder).
  - `verify` — parse captured `PROBE|<id>|<result>`, `COMPILE_FAIL|<id>`,
    `P2|<BEFORE|SAMERUN|COLD>|<key>|<value>`, and `SR|<boot>|<event>` logs
    strictly read-only; classify offline 3-tier save/load persistence logs;
    write JSON and Markdown scoreboards.
  - `cite` — format copy-paste-ready clean-room `[observed: ...]` citations with
    the automated privacy gate.
- `catalogs/` — 10 sanitized declarative console-only JSON catalogs (28 batches,
  542 read-only probes total; all file-writing and `con_Save` probes excluded):
  - `wave1_parity_probes.json` — Wave 1/2 parity probes covering xNVSE compound
    assignments & `dATan2`, JIP hit 3D direction/base damage/reticle limbs,
    ShowOff `OnFireWeapon` & `ForceWeaponJamAnim`, ShowOff objective/unholster
    & xNVSE `OnApplyIMOD`/`OnRemoveIMOD` events, and `SetCritDam` alias.
  - `wave4a_extenders.json` — Wave 4A read-only extender hypothesis catalog
    across 6 families (`w4_showoff`, `w4_jip_lutana`, `w4_jg`, `w4_sup`,
    `w4_knvse_misc`, `w4_itr_pp`).
  - `wave4b_extenders.json` — Wave 4B remaining read-only Bucket A & B extender
    catalog across JIP/Lutana, ShowOff/Core/kNVSE/AnhNVSE/Stewie, JohnnyGuitar,
    and SUP.
  - `wave4c_extenders.json` — Wave 4C read-only console-only discrepancy
    follow-ups and remaining Bucket A & B extender sweep (5 batches, 100
    probes: `w4c_phase_a`, `w4c_b_jip_lutana`, `w4c_b_showoff_core`,
    `w4c_b_jg`, `w4c_b_sup_knvse_opt`).
  - `wave4d_extenders.json` — Wave 4D read-only console-only open-PR and
    merged-PR NVSE/extender/vanilla edge-case probes, including 12 expanded
    maintainer-requested follow-ups (4 batches, 43 probes:
    `w4d_01_open_pr_quickwins`, `w4d_02_jip_lutana_edges`,
    `w4d_03_jg_sup_anh_showoff_edges`, `w4d_04_xnvse_compiler_and_vanilla_cmds`).
  - `wave4e_mods.json` — Wave 4E read-only console-only mod-ecosystem and
    vanilla runtime probes covering yUI 140/ySI, kNVSE 37, UI Organizer 230,
    MCM XML bitwise math, BaseObjectSwapper, SUP LockPickMenu, JIP Script
    Runner `gx_`, and vanilla combat/AI behavior (3 batches, 17 probes:
    `w4e_01_yui_ysi_and_knvse`, `w4e_02_uio_xml_mcm_bos`,
    `w4e_03_vanilla_runtime_and_script_runner`).
  - `wave4g_f2_extenders.json` — Wave 4G read-only console-only Bucket (f2)
    extender and runtime hypothesis probes (1 batch, 13 probes:
    `w4g_01_bucket_f2_readonly`).
  - `wave4h_reconcile.json` — Wave 4H read-only console-only maintainer-requested
    reconciliation probes covering XML parenting, EXPL slot 4 null vs omit,
    UIO `<ln>` nonzero start, float crit damage, multi-tick
    `SetCombatDisabled`, hex `DisableMuzzleFlashLights`, indoor moon/days/climate,
    `StewieGetValue` whitespace, `ApplyWeaponPoison` Stimpak vs poison,
    `SetSystemColor` truncation, and SUP `GetLockPickMenuValue` tiers (1 batch,
    11 probes: `w4h_01_maintainer_reconcile`).
  - `wave6_point_raycast.json` — Wave 6 read-only console-only JIP LN
    `GetPointRayCastPos` parity probes covering direction-angle signs, pre-seeded
    no-hit output coordinates, `maxRange <= 0` / omitted default reach, and
    `collisionFlag` filtering across exterior terrain, actor, and interior floor
    geometry (1 batch, 3 probes: `w6_point_raycast`).
  - `wave6d4_camera_movement.json` — Wave 6D-4 JIP LN `GetCameraMovement` two-float
    out-var signature and integer `0` return (1 batch, 1 probe: `w6d4_camera_movement`).
- `tests/test_nvse_harness.py` — offline stdlib `unittest` suite (no game
  install or captured logs required).

## One-Time Manual Staging & Cleanup Workflow

1. Back up your `Documents\My Games\FalloutNV\Fallout.ini` and Saves folder by
   hand (or prepare an isolated staging copy with only the 10 official masters).
2. Emit the desired catalog or batch into a private directory outside git:
   ```bash
   python tools/capture/nvse/harness.py emit \
     --catalog tools/capture/nvse/catalogs/wave4a_extenders.json \
     --batch w4_showoff \
     --game-dir "<path-to-fnv>" \
     --out-dir "<private-dir>/nvse-stage/w4_showoff"
   ```
3. Review the emitted scripts and `<private-dir>/nvse-stage/w4_showoff/MANUAL_SETUP.txt`,
   then copy `Data\NVSE\...` (and any optional read-only input fixtures under
   `fixtures/` if you opt in to fixture-backed read probes) into your isolated
   staging game folder by hand and run the game manually.
4. Capture the game console output into a private directory outside git (for
   example via `nvf_capture` or console output capture) and run `verify`
   read-only.
5. Remove all copied `gr_nvseprobe.txt`, `nvseprobe_*.gek`, and any staged
   read-only input fixtures from your staging copy by hand, and restore your
   INI backup if needed.

## Usage

### 1. Run offline tests and catalog validation (no game required)

```bash
python -m unittest discover -s tools/capture/nvse/tests -v
python tools/capture/nvse/harness.py validate --catalog-dir tools/capture/nvse/catalogs
python tools/capture/nvse/harness.py list
```

### 2. Verify captured logs read-only and generate scoreboards

```bash
python tools/capture/nvse/harness.py verify \
  --catalog-dir tools/capture/nvse/catalogs \
  --log-dir "<private-dir>/nvse-logs" \
  --redact-save-names \
  --json-out "<private-dir>/scoreboard.json" \
  --md-out "<private-dir>/scoreboard.md"
```

### 3. Format clean-room `[observed: ...]` citations

```bash
python tools/capture/nvse/harness.py cite \
  --scoreboard "<private-dir>/scoreboard.json" \
  --spec-file docs/spec-notes/ext-showoff.md
```

## `[design]` Harness Defaults

The following parameters are NVForever harness design choices `[design]` (not
original-game behavior constraints):

- `PROBE|<id>|<result>`, `P2|<BEFORE|SAMERUN|COLD>|<key>|<value>`, and
  `SR|<boot>|<event>` log line formats and the offline 3-tier persistence
  classifier labels (`SAVED`, `SESSION_ONLY`, `CLEARED_ON_LOAD`, `UNCHANGED`,
  `INCOMPLETE`, `ANOMALOUS`).
- Default main-loop tick interval `10`, console-only output (`PrintToConsole`),
  and staged filenames (`gr_nvseprobe.txt`, `nvseprobe_loop.gek`,
  `nvseprobe_s_<id>.gek`, `MANUAL_SETUP.txt`).
- Automated forbidden-writer validator list and privacy leak gate patterns
  rejecting machine-specific paths, unredacted save filenames, `.pdb` names,
  and control characters.
