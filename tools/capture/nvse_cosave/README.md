# Clean-Room `.nvse` Co-Save Capture, Parser & Verification Harness

This package provides a clean-room black-box `.nvse` co-save A/B capture staging/collection
tool, pure-Python `.nvse` binary parser/differ, and verification suite for validating
*Fallout: New Vegas* (`NVSE` v1) co-save layouts documented in
`docs/spec-notes/nvse-cosaves.md` without committing game assets, saves, plugins, or
private machine paths.

## Clean-Room & Read-Only Charter (`tools/capture/README.md` §10)

- **100% black-box runtime observation:** Every format rule is established strictly from
  single-variable A/B `.fos` + `.nvse` save pairs captured in an isolated staging copy of
  *Fallout: New Vegas* `1.4.0.525` (`040020D0`) with only the 10 official base masters
  and named public extender releases (`xNVSE 6.4.9`, `JIP LN NVSE 57.30`,
  `JohnnyGuitarNVSE 528`, `ShowOffNVSE Plugin 184`). Zero extender C++ source code,
  headers, or DLL binaries are consulted or disassembled.
- **Strictly read-only Python tools:** No Python tool in `tools/capture/nvse_cosave/`
  ever writes, modifies, creates, or deletes files inside `--game-dir`, `--saves-dir`, or
  `Documents\My Games\FalloutNV`. All staged scripts and collected copies go strictly
  inside `--out-dir` outside git, enforced by `assert_safe_out_dir` (refusing any
  `--out-dir` overlapping `--game-dir`, `--saves-dir`, or a live FNV install/Saves tree).
- **Staging-copy isolation for file-writing probes (`tools/capture/README.md` §10):**
  Because `con_Save` writes `.fos` and `.nvse` files, live capture may run **only** in a
  separate staging copy of the game folder (never treated as the real install) with
  `SLocalSavePath` redirected to an isolated probe saves subfolder, restoring all INI and
  load-order files byte-for-byte afterward.
- **Zero external dependencies:** Pure Python 3.11+ standard library only. All paths come
  from CLI flags or environment variables (`FNV_GAME_DIR`, `FNV_COSAVE_SAVES_DIR`,
  `FNV_COSAVE_OUT_DIR`).

## Layout

- `campaigns.py` — 3 declarative single-variable A/B `.nvse` capture campaigns using
  `GetFormFromMod "FalloutNV.esm" "<hex>"` (zero bare EditorIDs, zero custom plugins).
- `parser.py` — stdlib-only `.nvse` binary parser (`NVSE` v1 header, plugin blocks,
  `SDOM`, `SVRA`/`RVRA`/`EVRA`, `SVTS`/`EVTS`, `JPAV`, `JPRM`, `JPLR`, `JPSV`, `SMGJ`,
  `OSMS`, `SOAT`, plus raw pass-through for unknown plugins/chunks) and A/B differ.
- `harness.py` — strictly read-only campaign helper (`list`, `stage`, `collect`) with
  `assert_safe_out_dir` boundary protection.
- `verify.py` — read-only CLI verifier checking 27 byte-level A/B assertions across all
  captured `.nvse` files against `docs/spec-notes/nvse-cosaves.md`.
- `tests/test_nvse_cosave_synthetic.py` — offline pure-synthetic `unittest` suite
  requiring no game install and no captured saves.

## Usage

### 1. Run offline synthetic tests (no game required)

```bash
python -m unittest discover -s tools/capture/nvse_cosave/tests -v
```

### 2. List, stage, and collect campaigns (read-only on game/saves)

```bash
python tools/capture/nvse_cosave/harness.py list

python tools/capture/nvse_cosave/harness.py stage \
  --campaign cosave_core_jip \
  --out-dir "<private-dir>/cosave-stage/cosave_core_jip"

python tools/capture/nvse_cosave/harness.py collect \
  --campaign cosave_core_jip \
  --saves-dir "<staging-saves-dir>" \
  --out-dir "<private-dir>/cosave-captures"
```

### 3. Inspect, diff, or verify captured `.nvse` files (read-only)

```bash
python tools/capture/nvse_cosave/parser.py inspect "<private-dir>/c04_arr_strmap_nested.nvse"
python tools/capture/nvse_cosave/parser.py diff "<private-dir>/c00_base.nvse" "<private-dir>/c05_jip_aux_flt.nvse"
python tools/capture/nvse_cosave/verify.py --saves-dir "<private-dir>/cosave-captures"
```

## `[design]` Harness Defaults

- Default script loop tick interval `10`, staged script naming (`gr_cosaveprobe.txt`,
  `cosaveprobe_loop.gek`, `cosaveprobe_s_*.gek`), and `MANUAL_SETUP.txt` instructions.
- Temporary loop-state variable prefix `*cp_` (chosen so the loop itself writes zero
  persistent entries into `JPAV`).
