#!/usr/bin/env python3
"""Read-only clean-room black-box `.fos` A/B save-pair capture harness.

Strict read-only guarantees for the Python tools (aligned with
`tools/capture/README.md` §10 and `tools/capture/capture_run.py`):
- The Python tools (`harness.py`, `parser.py`, `verify.py`) NEVER write,
  create, modify, or delete any file or directory inside `--game-dir`,
  `--saves-dir`, or `Documents\\My Games\\FalloutNV` (zero tool writes to
  `Data\\`, `Fallout*.ini`, `plugins.txt`, `loadorder.txt`, `Saves\\`, or
  `fosprobe_trace.txt`).
- NEVER copies record bytes out of `FalloutNV.esm` or generates `.esp` plugins.
- All generated probe scripts (`gr_fosprobe.txt`, `fosprobe_loop.gek`,
  `fosprobe_s_*.gek`, and `MANUAL_SETUP.txt`) and collected `.fos` copies are
  written strictly inside the user-chosen `--out-dir` outside git, and
  `assert_safe_out_dir` refuses any `--out-dir` that equals, sits inside, or
  contains `--game-dir` / `--saves-dir` (checked on resolved real paths) or
  sits inside a live FNV install / `FalloutNV\\Saves` directory tree.
- Maintainer §10 exception (2026-11-24): the generated probe scripts write zero
  files inside `<game-dir>` (console logging uses `PrintToConsole` only); when
  manually installed and executed inside `FalloutNV.exe`, `fosprobe_loop.gek`
  is explicitly granted an exception to `tools/capture/README.md` §10 to call
  `con_Save <step.name>` and write `.fos` save captures into the isolated
  `Saves\\fosprobe\\` subfolder (e.g.,
  `Documents\\My Games\\FalloutNV\\Saves\\fosprobe\\`), cleaned up by hand.
- Refuses to collide with an already-running `FalloutNV.exe` process.
- Terminates ONLY its own spawned PID tree (`taskkill /PID <pid> /T /F` on
  Windows, `SIGTERM` on POSIX) — never kills unrelated instances.
"""

from __future__ import annotations

import argparse
import os
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path
from typing import Callable

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import campaigns as fos_campaigns  # type: ignore[import-not-found]
else:
    from . import campaigns as fos_campaigns

STAGED_STEP_PREFIX = "fosprobe_s_"
STAGED_LOOP_FILENAME = "fosprobe_loop.gek"
STAGED_GR_FILENAME = "gr_fosprobe.txt"
MANUAL_SETUP_FILENAME = "MANUAL_SETUP.txt"

LIVE_INSTALL_SENTINELS = (
    "FalloutNV.exe",
    "nvse_loader.exe",
    "Fallout.ini",
    "FalloutPrefs.ini",
    "plugins.txt",
    "loadorder.txt",
)


class ReadOnlyBoundaryError(PermissionError):
    """Raised when an operation attempts to write into a protected game or Saves directory."""


def _norm_real(path: str | os.PathLike[str]) -> Path:
    return Path(os.path.normcase(os.path.realpath(os.fspath(path))))


def _is_same_or_child(candidate: Path, root: Path) -> bool:
    try:
        candidate.relative_to(root)
        return True
    except ValueError:
        return False


def assert_safe_out_dir(
    out_dir: str | os.PathLike[str],
    *,
    game_dir: str | os.PathLike[str] | None = None,
    saves_dir: str | os.PathLike[str] | None = None,
) -> Path:
    """Verify `out_dir` does not overlap `--game-dir`, `--saves-dir`, or a live FNV install/Saves tree.

    Mirrors `tools/capture/nvse/harness.py`: all comparisons use resolved real
    paths (`os.path.realpath`) normalized for platform case rules so symlinks,
    junctions, and `..` segments cannot bypass the boundary.
    """
    raw_out = Path(out_dir)
    real_out = _norm_real(raw_out)

    protected: list[tuple[str, str | os.PathLike[str]]] = []
    eff_game = game_dir if game_dir is not None else os.environ.get("FNV_GAME_DIR")
    eff_saves = (
        saves_dir
        if saves_dir is not None
        else (os.environ.get("FNV_FOS_SAVES_DIR") or os.environ.get("FNV_SAVES_DIR"))
    )
    if eff_game:
        protected.append(("--game-dir", eff_game))
    if eff_saves:
        protected.append(("--saves-dir", eff_saves))

    for label, p in protected:
        real_p = _norm_real(p)
        if _is_same_or_child(real_out, real_p):
            raise ReadOnlyBoundaryError(
                f"Refusing to write inside or equal to protected {label}: {raw_out}"
            )
        if _is_same_or_child(real_p, real_out):
            raise ReadOnlyBoundaryError(
                f"Refusing to use an ancestor of protected {label} as --out-dir: {raw_out}"
            )

    # Defense in depth: refuse if out_dir or any ancestor sits inside a live FNV
    # install, My Games\FalloutNV INI/config tree, or FalloutNV\Saves folder.
    for ancestor in (real_out, *real_out.parents):
        if ancestor.name.lower() == "saves" and ancestor.parent.name.lower() == "falloutnv":
            raise ReadOnlyBoundaryError(
                f"Refusing to write inside FalloutNV Saves folder ({ancestor}): {raw_out}"
            )
        if not ancestor.exists():
            continue
        for sentinel in LIVE_INSTALL_SENTINELS:
            if (ancestor / sentinel).exists():
                raise ReadOnlyBoundaryError(
                    f"Refusing to write inside directory tree containing live game/config file "
                    f"'{sentinel}' ({ancestor}): {raw_out}"
                )
        if (ancestor / "Data" / "FalloutNV.esm").exists():
            raise ReadOnlyBoundaryError(
                f"Refusing to write inside directory tree containing Data/FalloutNV.esm "
                f"({ancestor}): {raw_out}"
            )
    return raw_out


def default_is_fnv_running() -> bool:
    """Return True if any `FalloutNV.exe` process is currently running."""
    if os.name == "nt":
        res = subprocess.run(
            ["tasklist", "/FI", "IMAGENAME eq FalloutNV.exe", "/NH"],
            capture_output=True,
            text=True,
            check=False,
        )
        return "falloutnv.exe" in res.stdout.lower()
    res = subprocess.run(["pgrep", "-i", "FalloutNV"], capture_output=True, text=True, check=False)
    return res.returncode == 0


def check_no_running_fnv(
    *,
    is_running: Callable[[], bool] = default_is_fnv_running,
    wait_seconds: float = 0.0,
    poll_interval: float = 5.0,
) -> None:
    """Refuse to launch if another `FalloutNV.exe` instance is already active."""
    deadline = time.monotonic() + max(0.0, wait_seconds)
    while is_running():
        if time.monotonic() >= deadline:
            raise RuntimeError(
                "Another FalloutNV.exe process is already running; refusing to collide."
            )
        time.sleep(min(poll_interval, max(0.1, deadline - time.monotonic())))


def terminate_pid_tree(pid: int | None) -> None:
    """Terminate ONLY the specified spawned PID (and its children)."""
    if not pid:
        return
    if os.name == "nt":
        subprocess.run(["taskkill", "/PID", str(pid), "/T", "/F"], capture_output=True, check=False)
    else:
        try:
            os.kill(pid, signal.SIGTERM)
        except ProcessLookupError:
            return


def _manual_setup_guide(campaign: fos_campaigns.Campaign, stage_root: Path) -> str:
    """Render concise one-time manual setup instructions for a staged campaign."""
    return "\n".join(
        [
            f"Campaign: {campaign.name} (Round {campaign.round_num}, {len(campaign.steps)} steps)",
            f"Staged directory: {stage_root}",
            "",
            "One-time manual setup (the Python capture tools never write into the game",
            "folder or Documents\\My Games\\FalloutNV, and the staged scripts write zero",
            "files inside <game-dir>; under the 2026-11-24 maintainer §10 exception to",
            "tools/capture/README.md, fosprobe_loop.gek intentionally calls",
            "`con_Save <step.name>` inside FalloutNV.exe to write `.fos` saves into",
            "the isolated Saves\\fosprobe\\ subfolder):",
            "",
            "1. Ensure Fallout: New Vegas 1.4.0.525 (10 official base masters only) has",
            "   xNVSE 6.4.9+ and JIP LN NVSE installed.",
            "2. Optional INI setup in Documents\\My Games\\FalloutNV\\Fallout.ini",
            "   (edited by hand, backed up by you first):",
            "     [General]",
            f"     SCharGenQuest={campaign.start_chargen_quest}",
            "     SLocalSavePath=Saves\\fosprobe\\",
            "     bAllowConsole=1",
            "     SIntroSequence=",
            "     SMainMenuMovieIntro=",
            "     sIntroMovie=",
            "   Create Documents\\My Games\\FalloutNV\\Saves\\fosprobe\\ by hand.",
            "3. Copy the staged Data\\ tree from this output directory into <FNV>\\Data\\:",
            f"   - Data\\NVSE\\Plugins\\Scripts\\{STAGED_GR_FILENAME}",
            f"   - Data\\NVSE\\user_defined_functions\\{STAGED_LOOP_FILENAME}",
            f"   - Data\\NVSE\\user_defined_functions\\{STAGED_STEP_PREFIX}*.gek",
            "4. Run the game (manually or via `harness.py run --campaign ...`) so",
            "   fosprobe_loop.gek writes the campaign's `.fos` captures via `con_Save`",
            "   into your configured Saves subfolder, then copy them out read-only with",
            "   `harness.py collect` and verify with `verify.py --saves-dir <private-dir>`.",
            "5. When finished, remove the copied `gr_fosprobe.txt` and `fosprobe_*.gek`",
            "   files from <FNV>\\Data\\NVSE\\, delete the captured `.fos` files /",
            "   `Saves\\fosprobe\\` subfolder, and restore your Fallout.ini by hand.",
            "",
        ]
    )


def stage_campaign_scripts(
    out_dir: str | os.PathLike[str],
    campaign: fos_campaigns.Campaign,
    *,
    interval: int | None = None,
    game_dir: str | os.PathLike[str] | None = None,
    saves_dir: str | os.PathLike[str] | None = None,
) -> tuple[Path, Path, Path]:
    """Write generated probe scripts strictly inside `out_dir` (never into `<game-dir>`).

    Creates:
      - `<out-dir>/Data/NVSE/user_defined_functions/fosprobe_s_*.gek`
      - `<out-dir>/Data/NVSE/user_defined_functions/fosprobe_loop.gek`
      - `<out-dir>/Data/NVSE/Plugins/Scripts/gr_fosprobe.txt`
      - `<out-dir>/MANUAL_SETUP.txt`

    Refuses any `out_dir` overlapping `game_dir`, `saves_dir`, or a live FNV
    install / `FalloutNV\\Saves` directory.

    Returns `(loop_path, gr_path, setup_guide_path)`.
    """
    odir = assert_safe_out_dir(out_dir, game_dir=game_dir, saves_dir=saves_dir)
    scripts_dir = odir / "Data" / "NVSE" / "Plugins" / "Scripts"
    udf_dir = odir / "Data" / "NVSE" / "user_defined_functions"
    scripts_dir.mkdir(parents=True, exist_ok=True)
    udf_dir.mkdir(parents=True, exist_ok=True)

    # Clean prior harness-owned generated files inside out_dir only.
    for item in scripts_dir.iterdir():
        if item.is_file() and item.name == STAGED_GR_FILENAME:
            item.unlink()
    for item in udf_dir.iterdir():
        if item.is_file() and (
            item.name.startswith(STAGED_STEP_PREFIX) or item.name == STAGED_LOOP_FILENAME
        ):
            item.unlink()

    for idx, step in enumerate(campaign.steps, 1):
        sp = udf_dir / f"{STAGED_STEP_PREFIX}{idx:02d}_{step.name}.gek"
        lines = ["begin Function {}"] + [f"  {cmd}" for cmd in step.commands] + ["end"]
        sp.write_bytes(("\r\n".join(lines) + "\r\n").encode("ascii"))

    tick_interval = campaign.interval if interval is None else interval
    gek = [
        "begin Function {}",
        '  int st = Player.AuxVarGetFlt "*fp_st"',
        "  int mm = GetActiveMenuMode",
        '  Player.AuxVarSetFlt "*fp_st" (st + 1)',
        "  if mm == 1013",
        "    if st == 4",
        '      PrintToConsole "FOSPROBE COC_START"',
        "      CenterOnCell GSDocMitchellHouse",
        "    endif",
        "    return",
        "  elseif mm != 0",
        "    CloseActiveMenu",
        '    ClickMenuButton "MessageMenu#1"',
        "    return",
        "  endif",
        '  int wt = Player.AuxVarGetFlt "*fp_wt"',
        "  if wt > 0",
        '    Player.AuxVarSetFlt "*fp_wt" (wt - 1)',
        "    return",
        "  endif",
        '  int ph = Player.AuxVarGetFlt "*fp_ph"',
        '  int sub = Player.AuxVarGetFlt "*fp_sub"',
        "  ref rFn",
        "  if ph == 0",
        '    Player.AuxVarSetFlt "*fp_ph" 1',
        '    Player.AuxVarSetFlt "*fp_wt" 8',
        "    return",
    ]
    for idx, step in enumerate(campaign.steps, 1):
        step_filename = f"{STAGED_STEP_PREFIX}{idx:02d}_{step.name}.gek"
        gek.append(f"  elseif ph == {idx}")
        gek.append("    if sub == 0")
        gek.append(f'      let rFn := CompileScript "{step_filename}"')
        gek.append("      if rFn")
        gek.append(f'        PrintToConsole "FOSPROBE RUN {idx} {step.name} OK"')
        gek.append("        Call rFn")
        gek.append("      else")
        gek.append(f'        PrintToConsole "FOSPROBE RUN {idx} {step.name} COMPILE_FAIL"')
        gek.append("      endif")
        gek.append('      Player.AuxVarSetFlt "*fp_sub" 1')
        if step.wait_ticks > 0:
            gek.append(f'      Player.AuxVarSetFlt "*fp_wt" {step.wait_ticks}')
        gek.append("    else")
        gek.append(f'      PrintToConsole "FOSPROBE SAVE {idx} {step.name}"')
        gek.append(f"      con_Save {step.name}")
        gek.append('      Player.AuxVarSetFlt "*fp_sub" 0')
        gek.append(f'      Player.AuxVarSetFlt "*fp_ph" {idx + 1}')
        gek.append('      Player.AuxVarSetFlt "*fp_wt" 1')
        gek.append("    endif")

    final_ph = len(campaign.steps) + 1
    gek.extend(
        [
            f"  elseif ph == {final_ph}",
            '    PrintToConsole "FOSPROBE CAMPAIGN_DONE_QUIT"',
            f'    Player.AuxVarSetFlt "*fp_ph" {final_ph + 1}',
            "    con_QuitGame",
            "  endif",
            "end",
        ]
    )

    loop_path = udf_dir / STAGED_LOOP_FILENAME
    loop_path.write_bytes(("\r\n".join(gek) + "\r\n").encode("ascii"))

    gr_path = scripts_dir / STAGED_GR_FILENAME
    gr_lines = [
        f'PrintToConsole "FOSPROBE GR_START {campaign.name}"',
        'Player.AuxVarSetFlt "*fp_st" 0',
        'Player.AuxVarSetFlt "*fp_ph" 0',
        'Player.AuxVarSetFlt "*fp_sub" 0',
        'Player.AuxVarSetFlt "*fp_wt" 0',
        f'SetGameMainLoopCallback (CompileScript "{STAGED_LOOP_FILENAME}") 1 {tick_interval} 3',
        'PrintToConsole "FOSPROBE GR_DONE"',
    ]
    gr_path.write_bytes(("\r\n".join(gr_lines) + "\r\n").encode("ascii"))

    setup_path = odir / MANUAL_SETUP_FILENAME
    setup_path.write_text(_manual_setup_guide(campaign, odir), encoding="utf-8", newline="\n")
    return loop_path, gr_path, setup_path


def collect_saves(
    saves_dir: str | os.PathLike[str],
    out_dir: str | os.PathLike[str],
    *,
    campaign: fos_campaigns.Campaign | None = None,
    game_dir: str | os.PathLike[str] | None = None,
) -> list[Path]:
    """Read-only copy of `.fos` files from `saves_dir` into `out_dir`.

    Never creates, modifies, or deletes anything inside `saves_dir`, and refuses
    any `out_dir` overlapping `saves_dir`, `game_dir`, or a live FNV install.
    When `campaign` is given, copies only `<step.name>.fos` files belonging to
    that campaign; otherwise copies all `*.fos` files found in `saves_dir`.
    """
    odir = assert_safe_out_dir(out_dir, game_dir=game_dir, saves_dir=saves_dir)
    sdir = Path(saves_dir)
    if not sdir.is_dir():
        raise FileNotFoundError(f"Saves directory does not exist: {sdir}")
    odir.mkdir(parents=True, exist_ok=True)

    wanted: set[str] | None = (
        {f"{step.name}.fos".lower() for step in campaign.steps} if campaign is not None else None
    )
    copied: list[Path] = []
    for entry in sorted(sdir.iterdir(), key=lambda p: p.name.lower()):
        if not entry.is_file() or not entry.name.lower().endswith(".fos"):
            continue
        if wanted is not None and entry.name.lower() not in wanted:
            continue
        dst = odir / entry.name
        shutil.copy2(entry, dst)
        copied.append(dst)
    return copied


def run_campaign(
    *,
    campaign: fos_campaigns.Campaign,
    game_dir: str | os.PathLike[str],
    saves_dir: str | os.PathLike[str],
    out_dir: str | os.PathLike[str],
    loader_exe: str = "nvse_loader.exe",
    max_wait: int | None = None,
    wait_fnv_free_seconds: float = 0.0,
    dry_run: bool = False,
    is_running_fn: Callable[[], bool] = default_is_fnv_running,
) -> list[Path]:
    """Launch a manually set-up game read-only and copy finished `.fos` captures to `out_dir`.

    Strictly read-only on `game_dir` and `saves_dir`: never writes scripts,
    plugins, INIs, load-order files, or deletes/creates saves inside those
    folders. The user must have performed the one-time manual setup from
    `stage_campaign_scripts` (`MANUAL_SETUP.txt`) beforehand.
    """
    odir = assert_safe_out_dir(out_dir, game_dir=game_dir, saves_dir=saves_dir)
    gdir = Path(game_dir)
    sdir = Path(saves_dir)

    if not gdir.is_dir():
        raise FileNotFoundError(f"Game directory does not exist: {gdir}")
    if not sdir.is_dir():
        raise FileNotFoundError(
            f"Saves directory does not exist: {sdir} (create it by hand during one-time setup)"
        )
    if not dry_run:
        check_no_running_fnv(is_running=is_running_fn, wait_seconds=wait_fnv_free_seconds)

    exe_path = gdir / loader_exe
    if not exe_path.is_file():
        fallback = gdir / "FalloutNV.exe"
        if fallback.is_file():
            exe_path = fallback
        else:
            raise FileNotFoundError(
                f"Neither {loader_exe} nor FalloutNV.exe found in --game-dir ({gdir})"
            )

    odir.mkdir(parents=True, exist_ok=True)
    if dry_run:
        return collect_saves(sdir, odir, campaign=campaign, game_dir=gdir)

    wanted = {f"{step.name}.fos".lower() for step in campaign.steps}
    timeout = campaign.max_wait if max_wait is None else max_wait
    print(f"=== Starting campaign '{campaign.name}' ({len(campaign.steps)} saves, read-only) ===")
    t0 = time.monotonic()
    proc: subprocess.Popen[bytes] | None = None
    try:
        proc = subprocess.Popen([str(exe_path)], cwd=str(gdir))
        last_count = -1
        while time.monotonic() - t0 < timeout:
            time.sleep(2.0)
            rc = proc.poll()
            fos_files = sorted(
                fn for fn in os.listdir(sdir) if fn.lower() in wanted and (sdir / fn).is_file()
            )
            if len(fos_files) != last_count or rc is not None:
                latest = fos_files[-1] if fos_files else None
                print(
                    f"  t={time.monotonic()-t0:.1f}s rc={rc} "
                    f"saves={len(fos_files)}/{len(campaign.steps)} latest={latest}"
                )
                last_count = len(fos_files)
            if rc is not None:
                break
            if len(fos_files) >= len(campaign.steps):
                time.sleep(2.5)
                break
        if proc.poll() is None:
            terminate_pid_tree(proc.pid)
        time.sleep(0.5)
    finally:
        if proc is not None and proc.poll() is None:
            terminate_pid_tree(proc.pid)

    copied = collect_saves(sdir, odir, campaign=campaign, game_dir=gdir)
    print(f"=== Campaign '{campaign.name}' copied {len(copied)}/{len(campaign.steps)} .fos files ===")
    return copied


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description=(
            "Read-only clean-room `.fos` A/B save-pair capture harness. "
            "The Python tools never write into --game-dir or Documents\\My Games\\FalloutNV "
            "(and refuse any --out-dir overlapping --game-dir, --saves-dir, or a live install); "
            "under the 2026-11-24 maintainer §10 exception to tools/capture/README.md, "
            "fosprobe_loop.gek calls `con_Save` inside FalloutNV.exe to write `.fos` captures "
            "into the isolated Saves\\fosprobe\\ subfolder."
        )
    )
    sub = ap.add_subparsers(dest="command", required=True)

    sub.add_parser("list", help="List all pure-vanilla declarative capture campaigns.")

    p_stage = sub.add_parser(
        "stage",
        help="Generate probe scripts (`gr_fosprobe.txt`, `fosprobe_*.gek`) strictly into --out-dir.",
    )
    p_stage.add_argument("--campaign", required=True, help="Campaign name from `list`.")
    p_stage.add_argument(
        "--out-dir",
        default=os.environ.get("FNV_FOS_OUT_DIR"),
        help="Private output directory outside git for staged scripts (or env FNV_FOS_OUT_DIR).",
    )
    p_stage.add_argument(
        "--game-dir",
        default=os.environ.get("FNV_GAME_DIR"),
        help="Optional game directory to guard --out-dir against (or env FNV_GAME_DIR).",
    )
    p_stage.add_argument(
        "--saves-dir",
        default=os.environ.get("FNV_FOS_SAVES_DIR"),
        help="Optional Saves directory to guard --out-dir against (or env FNV_FOS_SAVES_DIR).",
    )
    p_stage.add_argument("--interval", type=int, default=None, help="Override script loop tick interval.")

    p_collect = sub.add_parser(
        "collect",
        help="Copy captured `.fos` files read-only from --saves-dir into --out-dir.",
    )
    p_collect.add_argument(
        "--campaign",
        default=None,
        help="Optional campaign name to filter only that campaign's `.fos` files.",
    )
    p_collect.add_argument(
        "--game-dir",
        default=os.environ.get("FNV_GAME_DIR"),
        help="Optional game directory to guard --out-dir against (or env FNV_GAME_DIR).",
    )
    p_collect.add_argument(
        "--saves-dir",
        default=os.environ.get("FNV_FOS_SAVES_DIR"),
        help="Game Saves folder to read from (read-only; or env FNV_FOS_SAVES_DIR).",
    )
    p_collect.add_argument(
        "--out-dir",
        default=os.environ.get("FNV_FOS_OUT_DIR"),
        help="Private output directory outside git for copied `.fos` files (or env FNV_FOS_OUT_DIR).",
    )

    p_run = sub.add_parser(
        "run",
        help="Launch a manually prepared game (read-only) and copy captured `.fos` files to --out-dir.",
    )
    p_run.add_argument("--campaign", required=True, help="Campaign name from `list`.")
    p_run.add_argument(
        "--game-dir",
        default=os.environ.get("FNV_GAME_DIR"),
        help="Game folder containing nvse_loader.exe / FalloutNV.exe (read-only; or env FNV_GAME_DIR).",
    )
    p_run.add_argument(
        "--saves-dir",
        default=os.environ.get("FNV_FOS_SAVES_DIR"),
        help="Game Saves folder to poll and read from (read-only; or env FNV_FOS_SAVES_DIR).",
    )
    p_run.add_argument(
        "--out-dir",
        default=os.environ.get("FNV_FOS_OUT_DIR"),
        help="Private output directory outside git for captured `.fos` files (or env FNV_FOS_OUT_DIR).",
    )
    p_run.add_argument(
        "--loader-exe",
        default="nvse_loader.exe",
        help="Executable inside --game-dir to launch (default nvse_loader.exe, falls back to FalloutNV.exe).",
    )
    p_run.add_argument("--max-wait", type=int, default=None, help="Override max wait seconds.")
    p_run.add_argument(
        "--wait-fnv-free",
        type=float,
        default=0.0,
        help="Seconds to wait for an existing FalloutNV.exe to exit before failing (default 0).",
    )
    p_run.add_argument(
        "--dry-run",
        action="store_true",
        help="Validate paths and collect any existing matching saves without launching the game.",
    )

    args = ap.parse_args(argv)

    if args.command == "list":
        for camp in fos_campaigns.ALL_CAMPAIGNS:
            print(
                f"  R{camp.round_num} {camp.name:28s} ({len(camp.steps):2d} steps) — "
                f"{camp.description}"
            )
        return 0

    if args.command == "stage":
        if args.campaign not in fos_campaigns.CAMPAIGNS_BY_NAME:
            ap.error(f"Unknown campaign {args.campaign!r}; run `harness.py list` to see names.")
        if not args.out_dir:
            ap.error("--out-dir (or FNV_FOS_OUT_DIR) is required.")
        loop_path, gr_path, setup_path = stage_campaign_scripts(
            args.out_dir,
            fos_campaigns.CAMPAIGNS_BY_NAME[args.campaign],
            interval=args.interval,
            game_dir=args.game_dir,
            saves_dir=args.saves_dir,
        )
        print(f"Staged scripts into {Path(args.out_dir)}:")
        print(f"  {gr_path}")
        print(f"  {loop_path}")
        print(f"  {setup_path}")
        return 0

    if args.command == "collect":
        if not args.saves_dir or not args.out_dir:
            ap.error("--saves-dir and --out-dir (or env vars) are required.")
        camp = None
        if args.campaign is not None:
            if args.campaign not in fos_campaigns.CAMPAIGNS_BY_NAME:
                ap.error(f"Unknown campaign {args.campaign!r}; run `harness.py list` to see names.")
            camp = fos_campaigns.CAMPAIGNS_BY_NAME[args.campaign]
        copied = collect_saves(args.saves_dir, args.out_dir, campaign=camp, game_dir=args.game_dir)
        print(f"Copied {len(copied)} .fos file(s) read-only into {Path(args.out_dir)}.")
        return 0

    if args.command == "run":
        if args.campaign not in fos_campaigns.CAMPAIGNS_BY_NAME:
            ap.error(f"Unknown campaign {args.campaign!r}; run `harness.py list` to see names.")
        missing = [
            flag
            for flag, val in (
                ("--game-dir", args.game_dir),
                ("--saves-dir", args.saves_dir),
                ("--out-dir", args.out_dir),
            )
            if not val
        ]
        if missing:
            ap.error(f"Missing required path arguments (or env vars): {', '.join(missing)}")
        run_campaign(
            campaign=fos_campaigns.CAMPAIGNS_BY_NAME[args.campaign],
            game_dir=args.game_dir,
            saves_dir=args.saves_dir,
            out_dir=args.out_dir,
            loader_exe=args.loader_exe,
            max_wait=args.max_wait,
            wait_fnv_free_seconds=args.wait_fnv_free,
            dry_run=args.dry_run,
        )
        return 0

    return 2


if __name__ == "__main__":
    sys.exit(main())
