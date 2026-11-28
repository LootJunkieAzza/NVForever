#!/usr/bin/env python3
"""Strictly read-only clean-room `.nvse` co-save A/B capture staging & collection helper.

Mirrors `tools/capture/fos/harness.py` and `tools/capture/nvse/harness.py`:
- NEVER writes, creates, modifies, or deletes any file or directory inside
  `--game-dir`, `--saves-dir`, or `Documents\\My Games\\FalloutNV`.
- Enforces `assert_safe_out_dir` using resolved real paths so symlinks, junctions,
  and `..` segments cannot bypass the read-only boundary.
- Live `con_Save` execution may run ONLY in a separate staging copy of the game
  folder (`tools/capture/README.md` §10).
"""

from __future__ import annotations

import argparse
import os
import shutil
import sys
from pathlib import Path

if __package__ in (None, ""):
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import campaigns as cosave_campaigns  # type: ignore[import-not-found]
else:
    from . import campaigns as cosave_campaigns

STAGED_STEP_PREFIX = "cosaveprobe_s_"
STAGED_LOOP_FILENAME = "cosaveprobe_loop.gek"
STAGED_GR_FILENAME = "gr_cosaveprobe.txt"
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
    """Refuse any `out_dir` overlapping `--game-dir`, `--saves-dir`, or a live FNV install/Saves tree."""
    raw_out = Path(out_dir)
    real_out = _norm_real(raw_out)

    protected: list[tuple[str, str | os.PathLike[str]]] = []
    eff_game = game_dir if game_dir is not None else os.environ.get("FNV_GAME_DIR")
    eff_saves = (
        saves_dir
        if saves_dir is not None
        else (os.environ.get("FNV_COSAVE_SAVES_DIR") or os.environ.get("FNV_SAVES_DIR"))
    )
    if eff_game:
        protected.append(("--game-dir", eff_game))
    if eff_saves:
        protected.append(("--saves-dir", eff_saves))

    for label, p in protected:
        real_p = _norm_real(p)
        if _is_same_or_child(real_out, real_p):
            raise ReadOnlyBoundaryError(f"Refusing to write inside or equal to protected {label}: {raw_out}")
        if _is_same_or_child(real_p, real_out):
            raise ReadOnlyBoundaryError(f"Refusing to use an ancestor of protected {label} as --out-dir: {raw_out}")

    for ancestor in (real_out, *real_out.parents):
        if ancestor.name.lower() == "saves" and ancestor.parent.name.lower() == "falloutnv":
            raise ReadOnlyBoundaryError(f"Refusing to write inside FalloutNV Saves folder ({ancestor}): {raw_out}")
        if not ancestor.exists():
            continue
        for sentinel in LIVE_INSTALL_SENTINELS:
            if (ancestor / sentinel).exists():
                raise ReadOnlyBoundaryError(
                    f"Refusing to write inside directory tree containing '{sentinel}' ({ancestor}): {raw_out}"
                )
        if (ancestor / "Data" / "FalloutNV.esm").exists():
            raise ReadOnlyBoundaryError(
                f"Refusing to write inside directory tree containing Data/FalloutNV.esm ({ancestor}): {raw_out}"
            )
    return raw_out


def _manual_setup_guide(campaign: cosave_campaigns.Campaign, stage_root: Path) -> str:
    return "\n".join(
        [
            f"Campaign: {campaign.name} ({len(campaign.steps)} steps)",
            f"Required extenders: {', '.join(campaign.required_extenders)}",
            f"Staged directory: {stage_root}",
            "",
            "Isolated staging-copy setup (`tools/capture/README.md` §10):",
            "1. Create a separate staging copy of Fallout: New Vegas 1.4.0.525 (10 official",
            "   masters only, hardlinks/junctions to vanilla BSAs/ESMs) with xNVSE 6.4.9,",
            "   JIP LN NVSE 57.30, JohnnyGuitarNVSE 528, and ShowOffNVSE Plugin 184.",
            "2. Back up your host INI and load-order files, and redirect SLocalSavePath to",
            "   an isolated probe saves subfolder (never your real Saves directory).",
            "3. Copy the staged Data\\NVSE\\ files into the staging copy by hand, run the",
            "   staging FalloutNV.exe, then copy the resulting .fos + .nvse files out via",
            "   `harness.py collect` and verify with `verify.py --saves-dir <private-dir>`.",
            "4. Remove the staged scripts and probe saves and restore your INI/load-order",
            "   backups (SHA-256 verified) afterward.",
            "",
        ]
    )


def stage_campaign_scripts(
    out_dir: str | os.PathLike[str],
    campaign: cosave_campaigns.Campaign,
    *,
    interval: int | None = None,
    game_dir: str | os.PathLike[str] | None = None,
    saves_dir: str | os.PathLike[str] | None = None,
) -> tuple[Path, Path, Path]:
    """Generate clean-room `.nvse` co-save probe scripts strictly inside `out_dir`."""
    odir = assert_safe_out_dir(out_dir, game_dir=game_dir, saves_dir=saves_dir)
    scripts_dir = odir / "Data" / "NVSE" / "Plugins" / "Scripts"
    udf_dir = odir / "Data" / "NVSE" / "user_defined_functions"
    scripts_dir.mkdir(parents=True, exist_ok=True)
    udf_dir.mkdir(parents=True, exist_ok=True)

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
        '  int st = Player.AuxVarGetFlt "*cp_st"',
        "  int mm = GetActiveMenuMode",
        '  Player.AuxVarSetFlt "*cp_st" (st + 1)',
        "  if mm == 1013",
        "    if st == 3",
        "      CenterOnCell GSDocMitchellHouse",
        "    endif",
        "    return",
        "  elseif mm != 0",
        "    CloseActiveMenu",
        '    ClickMenuButton "MessageMenu#1"',
        "    return",
        "  endif",
        '  int wt = Player.AuxVarGetFlt "*cp_wt"',
        "  if wt > 0",
        '    Player.AuxVarSetFlt "*cp_wt" (wt - 1)',
        "    return",
        "  endif",
        '  int ph = Player.AuxVarGetFlt "*cp_ph"',
        '  int sub = Player.AuxVarGetFlt "*cp_sub"',
        "  ref rFn",
        "  if ph == 0",
        '    Player.AuxVarSetFlt "*cp_ph" 1',
        '    Player.AuxVarSetFlt "*cp_wt" 6',
        "    return",
    ]
    for idx, step in enumerate(campaign.steps, 1):
        step_filename = f"{STAGED_STEP_PREFIX}{idx:02d}_{step.name}.gek"
        gek.append(f"  elseif ph == {idx}")
        gek.append("    if sub == 0")
        gek.append(f'      let rFn := CompileScript "{step_filename}"')
        gek.append("      if rFn")
        gek.append("        Call rFn")
        gek.append("      else")
        gek.append(f'        PrintToConsole "COMPILE_FAIL|{step.name}"')
        gek.append("      endif")
        if step.inline_save:
            gek.append(f'      Player.AuxVarSetFlt "*cp_ph" {idx + 1}')
            gek.append('      Player.AuxVarSetFlt "*cp_wt" 2')
        else:
            gek.append('      Player.AuxVarSetFlt "*cp_sub" 1')
            gek.append(f'      Player.AuxVarSetFlt "*cp_wt" {step.wait_ticks}')
        gek.append("    else")
        gek.append(f"      con_Save {step.name}")
        gek.append('      Player.AuxVarSetFlt "*cp_sub" 0')
        gek.append(f'      Player.AuxVarSetFlt "*cp_ph" {idx + 1}')
        gek.append('      Player.AuxVarSetFlt "*cp_wt" 2')
        gek.append("    endif")

    final_ph = len(campaign.steps) + 1
    gek.extend(
        [
            f"  elseif ph == {final_ph}",
            '    PrintToConsole "PROBE_DONE"',
            f'    Player.AuxVarSetFlt "*cp_ph" {final_ph + 1}',
            "    con_QuitGame",
            "  endif",
            "end",
        ]
    )

    loop_path = udf_dir / STAGED_LOOP_FILENAME
    loop_path.write_bytes(("\r\n".join(gek) + "\r\n").encode("ascii"))

    gr_path = scripts_dir / STAGED_GR_FILENAME
    gr_lines = [
        f'PrintToConsole "COSAVEPROBE GR_START {campaign.name}"',
        'Player.AuxVarSetFlt "*cp_st" 0',
        'Player.AuxVarSetFlt "*cp_ph" 0',
        'Player.AuxVarSetFlt "*cp_sub" 0',
        'Player.AuxVarSetFlt "*cp_wt" 0',
        f'SetGameMainLoopCallback (CompileScript "{STAGED_LOOP_FILENAME}") 1 {tick_interval} 3',
    ]
    gr_path.write_bytes(("\r\n".join(gr_lines) + "\r\n").encode("ascii"))

    setup_path = odir / MANUAL_SETUP_FILENAME
    setup_path.write_text(_manual_setup_guide(campaign, odir), encoding="utf-8", newline="\n")
    return loop_path, gr_path, setup_path


def collect_cosaves(
    saves_dir: str | os.PathLike[str],
    out_dir: str | os.PathLike[str],
    *,
    campaign: cosave_campaigns.Campaign | None = None,
    game_dir: str | os.PathLike[str] | None = None,
) -> list[Path]:
    """Read-only copy of `.fos` + `.nvse` pairs from `saves_dir` into `out_dir`."""
    odir = assert_safe_out_dir(out_dir, game_dir=game_dir, saves_dir=saves_dir)
    sdir = Path(saves_dir)
    if not sdir.is_dir():
        raise FileNotFoundError(f"Saves directory does not exist: {sdir}")
    odir.mkdir(parents=True, exist_ok=True)

    wanted: set[str] | None = (
        {
            f"{step.name}{ext}".lower()
            for step in campaign.steps
            for ext in (".fos", ".nvse")
        }
        if campaign is not None
        else None
    )
    copied: list[Path] = []
    for entry in sorted(sdir.iterdir(), key=lambda p: p.name.lower()):
        if not entry.is_file() or entry.suffix.lower() not in (".fos", ".nvse"):
            continue
        if wanted is not None and entry.name.lower() not in wanted:
            continue
        dst = odir / entry.name
        shutil.copy2(entry, dst)
        copied.append(dst)
    return copied


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Read-only clean-room `.nvse` co-save A/B capture staging & collection helper."
    )
    sub = ap.add_subparsers(dest="command", required=True)
    sub.add_parser("list", help="List all declarative `.nvse` co-save campaigns.")

    p_stage = sub.add_parser("stage", help="Stage probe scripts strictly into --out-dir.")
    p_stage.add_argument("--campaign", required=True)
    p_stage.add_argument("--out-dir", default=os.environ.get("FNV_COSAVE_OUT_DIR"))
    p_stage.add_argument("--game-dir", default=os.environ.get("FNV_GAME_DIR"))
    p_stage.add_argument("--saves-dir", default=os.environ.get("FNV_COSAVE_SAVES_DIR"))
    p_stage.add_argument("--interval", type=int, default=None)

    p_collect = sub.add_parser("collect", help="Copy `.fos` + `.nvse` files read-only into --out-dir.")
    p_collect.add_argument("--campaign", default=None)
    p_collect.add_argument("--game-dir", default=os.environ.get("FNV_GAME_DIR"))
    p_collect.add_argument("--saves-dir", default=os.environ.get("FNV_COSAVE_SAVES_DIR"))
    p_collect.add_argument("--out-dir", default=os.environ.get("FNV_COSAVE_OUT_DIR"))

    args = ap.parse_args(argv)
    if args.command == "list":
        for c in cosave_campaigns.ALL_CAMPAIGNS:
            print(f"  {c.name:24s} ({len(c.steps):2d} steps) — {c.description}")
        return 0
    if args.command == "stage":
        if args.campaign not in cosave_campaigns.CAMPAIGNS_BY_NAME:
            ap.error(f"Unknown campaign {args.campaign!r}")
        if not args.out_dir:
            ap.error("--out-dir (or FNV_COSAVE_OUT_DIR) is required.")
        loop_p, gr_p, guide_p = stage_campaign_scripts(
            args.out_dir,
            cosave_campaigns.CAMPAIGNS_BY_NAME[args.campaign],
            interval=args.interval,
            game_dir=args.game_dir,
            saves_dir=args.saves_dir,
        )
        print(f"Staged scripts into {Path(args.out_dir)}:\n  {gr_p}\n  {loop_p}\n  {guide_p}")
        return 0
    if args.command == "collect":
        if not args.saves_dir or not args.out_dir:
            ap.error("--saves-dir and --out-dir are required.")
        camp = cosave_campaigns.CAMPAIGNS_BY_NAME[args.campaign] if args.campaign else None
        copied = collect_cosaves(args.saves_dir, args.out_dir, campaign=camp, game_dir=args.game_dir)
        print(f"Copied {len(copied)} file(s) read-only into {Path(args.out_dir)}.")
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
