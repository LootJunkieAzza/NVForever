# Clean-Room `.fos` Save-Pair Capture, Parser & Verification Harness

This package provides a clean-room black-box `.fos` A/B capture
staging/collection tool, pure-Python `.fos` binary parser/differ, and
verification suite for validating Fallout: New Vegas (`FO3SAVEGAME`) save
layouts without ever committing game assets, saves, plugins, or private machine
paths. The Python tools themselves are strictly read-only on `<game-dir>` and
`Documents\My Games\FalloutNV`; live capture relies on one-time manual setup so
the manually installed `fosprobe_loop.gek` can call `con_Save <step.name>`
inside `FalloutNV.exe` to produce `.fos` saves in a user-isolated `Saves`
subfolder.

## Clean-Room & Read-Only Charter

- **Black-box runtime observation only:** Every format check here operates on
  `.fos` save pairs produced by running unmodified *Fallout: New Vegas*
  `1.4.0.525` (English) and diffing Save A (before) vs Save B (after) a single
  state mutation. No disassembly, decompilation, binary string dumping, or
  third-party DLL inspection is used.
- **Strictly read-only Python tools (`tools/capture/README.md` §10) + explicit
  2026-11-24 §10 exception for `Saves\fosprobe\`:** No Python tool in
  `tools/capture/fos/` (`harness.py`, `parser.py`, `verify.py`) ever writes,
  modifies, creates, or deletes files in `<game-dir>`, `<game-dir>\Data\`, or
  `Documents\My Games\FalloutNV` (`Fallout.ini`, `FalloutPrefs.ini`,
  `FalloutCustom.ini`, `plugins.txt`, `loadorder.txt`, `Saves\`, or
  `fosprobe_trace.txt`). No tool copies bytes out of `FalloutNV.esm` or
  generates `.esp` plugins, and the generated probe scripts write zero files
  inside `<game-dir>` (`PrintToConsole` only). All Python tool outputs go
  strictly into the user-chosen `--out-dir` outside git, and `harness.py`
  enforces an `--out-dir` guard (`assert_safe_out_dir`, mirroring
  `tools/capture/nvse/harness.py`) that refuses any `--out-dir` equal to,
  inside, or an ancestor of `--game-dir` / `--saves-dir`, or inside a live FNV
  install / `FalloutNV\Saves` directory tree. Under the **2026-11-24 maintainer
  exception to `tools/capture/README.md` §10** granted for this harness, when
  manually installed and run in `FalloutNV.exe`, `fosprobe_loop.gek`
  intentionally calls `con_Save <step.name>` to create `.fos` save files in the
  isolated `Saves\fosprobe\` subfolder (e.g.,
  `Documents\My Games\FalloutNV\Saves\fosprobe\`), which the user cleans up by
  hand after capture.
- **Zero game data in git:** Never commit `.fos` saves, `.esm`/`.esp` plugins,
  screenshots, or raw dumps. Always point `--out-dir` and `--saves-dir` at a
  private directory outside the repository.
- **Zero hardcoded paths or dependencies:** Standard library Python 3.11+ only.
  All directories (`--game-dir`, `--saves-dir`, `--out-dir`) come from CLI flags
  or environment variables (`FNV_GAME_DIR`, `FNV_FOS_SAVES_DIR`,
  `FNV_FOS_OUT_DIR`).

## Layout

- `parser.py` — stdlib-only `.fos` binary parser (Header, Screenshot RGB mean
  check, Global Tables 1 & 2, FormID/worldspace tables, Changed Forms across
  `ACHR`/`ACRE`, `REFR`, `PGRE`, `CELL`, `QUST`, `NPC_`/`CREA`, `ECZN`, `TACT`,
  `MISC`, `FACT`, `FLST`, `LVLI`, `REPU`, `CHAL`, `CLAS`, `INFO`, and `0xFF`
  created refs) plus A/B save-pair diffing.
- `campaigns.py` — 16 declarative pure-vanilla 10-master single-variable A/B
  probe campaigns across Rounds 1–3 using `BuildRef 0 <id>` and vanilla
  commands.
- `harness.py` — strictly read-only campaign helper with four subcommands:
  - `list` — list all pure-vanilla declarative campaigns,
  - `stage` — write generated probe scripts (`gr_fosprobe.txt`,
    `fosprobe_loop.gek`, `fosprobe_s_*.gek`) and `MANUAL_SETUP.txt` strictly
    into `--out-dir` (never into `<game-dir>`),
  - `collect` — copy captured `.fos` files read-only from `--saves-dir` into
    `--out-dir`,
  - `run` — launch a manually prepared game read-only (`nvse_loader.exe` /
    `FalloutNV.exe`) with a running-process collision guard and PID-tree-only
    timeout cleanup, poll `--saves-dir` read-only, and copy finished `.fos`
    files into `--out-dir`.
- `verify.py` — read-only CLI verifier (`python -m tools.capture.fos.verify
  --saves-dir <dir>` or `python tools/capture/fos/verify.py --saves-dir <dir>`)
  validating all 61 pure-vanilla 10-master assertions (plus 7 optional
  manual-GECK-plugin assertions when those captures are present in
  `--saves-dir`, 68 total) against the format facts documented in
  `docs/spec-notes/save-files.md`.
- `tests/test_parser_synthetic.py` — offline pure-synthetic unit test suite
  building minimal `FO3SAVEGAME` buffers in memory and proving zero writes to
  any game/INI/Saves directory.

## One-Time Manual Setup (owner, by hand — tools never write into the game folder)

Offline synthetic tests (`tests/test_parser_synthetic.py`) require **no game
install and no `.fos` files**. For live A/B capture, mirror
`tools/capture/README.md` §10 by setting up the game folder and
`Documents\My Games\FalloutNV` once by hand:

1. Use an unmodified install of *Fallout: New Vegas* `1.4.0.525` (English) with
   only the 10 official base masters loaded (`FalloutNV.esm`, the four story
   DLCs, `GunRunnersArsenal.esm`, and the four courier packs; zero gameplay/UI
   mods), plus xNVSE `6.4.9+` and JIP LN NVSE (for `CompileScript`,
   `SetGameMainLoopCallback`, `con_Save`, and `con_QuitGame`).
2. Back up your `Documents\My Games\FalloutNV\Fallout.ini` and
   `FalloutPrefs.ini` by hand. Optionally point `[General]`
   `SLocalSavePath=Saves\fosprobe\` (and create `Saves\fosprobe\` by hand), set
   `bAllowConsole=1`, clear `SIntroSequence=` / `SMainMenuMovieIntro=` /
   `sIntroMovie=`, and set `SCharGenQuest=00102037` (or empty for campaigns
   that start directly in-world).
3. Stage the desired campaign's probe scripts into a private directory outside
   git:
   ```bash
   python tools/capture/fos/harness.py stage \
     --campaign run1_player \
     --out-dir "<private-dir>/fos-stage/run1_player"
   ```
4. Inspect the generated scripts under `<private-dir>/fos-stage/run1_player/Data/`
   and copy them into `<FNV>\Data\` by hand:
   - `Data\NVSE\Plugins\Scripts\gr_fosprobe.txt`
   - `Data\NVSE\user_defined_functions\fosprobe_loop.gek`
   - `Data\NVSE\user_defined_functions\fosprobe_s_*.gek`
5. When finished capturing, delete the copied `gr_fosprobe.txt` and
   `fosprobe_*.gek` files from `<FNV>\Data\NVSE\`, remove the captured `.fos`
   files / `Saves\fosprobe\` subfolder written by `con_Save`, and restore your
   INI backup by hand.

## Optional Manual GECK Test Plugin (for rare extras absent from vanilla masters)

Vanilla `FalloutNV.esm` contains zero carryable `LIGH` forms (`0x26`), zero
charged weapon `ENCH` records (`0x28`), zero dual-slot ring `ARMO` forms
(`0x17`), and zero world `REFR`s carrying `XGLB` (`0x22`). Because capture
tools never copy record bytes out of `FalloutNV.esm` or write into `Data\`,
contributors who want to re-verify those 7 optional assertions can create a
small test `.esp` manually in the G.E.C.K. containing:
- a `LIGH` with `CanBeCarried` (`0x02`) and `Time = 120` placed in
  `GSDocMitchellHouse`,
- an `ARMO` ring using both ring biped slots (`BMDT` bits 7 & 8),
- a weapon `ENCH` with non-zero charge (`500`, cost `25`) attached to a test
  `WEAP` (`EITM` + `EAMT = 500`), and
- placed `REFR`s in `GSDocMitchellHouse` with `XGLB` (`0x00000038` `GameHour`)
  and `XSCL` (`1.5` / `1.75`).

Enable that plugin manually in your load order, capture the `r21_*` / `r23_*`
saves into your private `--saves-dir`, and `verify.py` will automatically run
those 7 additional assertions alongside the 61 pure-vanilla assertions.

## Usage

### 1. Run offline synthetic tests (no game required)

```bash
python -m unittest discover -s tools/capture/fos/tests -v
```

### 2. List, stage, run (read-only), and collect pure-vanilla campaigns

```bash
python tools/capture/fos/harness.py list

python tools/capture/fos/harness.py stage \
  --campaign run1_player \
  --out-dir "<private-dir>/fos-stage/run1_player"

# After copying staged scripts into <FNV>\Data\ by hand:
python tools/capture/fos/harness.py run \
  --campaign run1_player \
  --game-dir "<path-to-fnv>" \
  --saves-dir "<path-to-My-Games-FalloutNV>/Saves/fosprobe" \
  --out-dir "<private-dir>/fos-saves"

# Or copy already-captured saves read-only without launching the game:
python tools/capture/fos/harness.py collect \
  --campaign run1_player \
  --saves-dir "<path-to-My-Games-FalloutNV>/Saves/fosprobe" \
  --out-dir "<private-dir>/fos-saves"
```

### 3. Diff or scan captured `.fos` pairs (read-only)

```bash
python tools/capture/fos/parser.py diff "<private-dir>/p00_base.fos" "<private-dir>/p01_scale.fos"
python tools/capture/fos/parser.py scan --saves-dir "<private-dir>/fos-saves"
```

### 4. Run the verification matrix (read-only)

```bash
python -m tools.capture.fos.verify --saves-dir "<private-dir>/fos-saves"
# or:
python tools/capture/fos/verify.py --saves-dir "<private-dir>/fos-saves"
```

## `[design]` Harness Defaults

The following parameters are NVForever harness design choices `[design]` (not
original-game format constraints):

- Default script loop tick interval `15`, per-campaign `max_wait` timeouts
  (`90`–`180` seconds), and staged script naming (`gr_fosprobe.txt`,
  `fosprobe_loop.gek`, `fosprobe_s_*.gek`).
- Optional defensive pass-through parsing of unemittable item extras `0x17`,
  `0x26`, and `0x28` in `parser.py`.
