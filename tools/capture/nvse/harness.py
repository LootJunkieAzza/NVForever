#!/usr/bin/env python3
"""Read-only clean-room black-box NVSE/extender probe harness.

Strict read-only guarantees (aligned with `tools/capture/README.md` §10):
- NEVER writes, creates, modifies, renames, or deletes any file or directory
  inside `--game-dir`, `Data\\`, `NVSE\\`, `Documents\\My Games\\FalloutNV`,
  `Fallout*.ini`, or any game Saves directory.
- NEVER launches `FalloutNV.exe` or `nvse_loader.exe` automatically against a
  live install.
- `emit` writes generated console-only probe scripts and `MANUAL_SETUP.txt`
  strictly inside an explicit `--out-dir` outside git, and refuses any
  `--out-dir` that equals, sits inside, or contains `--game-dir` / `--saves-dir`
  (checked on resolved real paths) or contains live game-install sentinels.
- All emitted probe scripts log exclusively via `PrintToConsole` and are
  validated to contain zero game-folder or Saves-folder file-writing commands
  (no `WriteStringToFile`, `con_Save`, `WriteToJSON`, `SetINI*`,
  `SaveCachedIniFile`, `ExcelWrite*`, `ExcelErase*`, `TextToQRCode`,
  `DumpLoadOrder`, `ModLogPrint`, etc.). `emit` refuses persistence catalogs
  because two-boot capture requires `con_Save` into the Saves folder.
- `verify` and `cite` read captured `PROBE|<id>|<result>` and two-boot
  `P2|<BEFORE|SAMERUN|COLD>|<key>|<value>` logs strictly read-only, classify
  three-tier save/load persistence (`SAVED` / `SESSION_ONLY` /
  `CLEARED_ON_LOAD`), and enforce an automated privacy leak gate.
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass
import json
import os
from pathlib import Path
import re
import sys
from typing import Any

PROBE_CATALOG_SCHEMA = "nvf.nvse.probe.catalog/1"
PERSISTENCE_CATALOG_SCHEMA = "nvf.nvse.persistence.catalog/1"
SCOREBOARD_SCHEMA = "nvf.nvse.scoreboard/1"

SUPPORTED_CATALOG_SCHEMAS = (PROBE_CATALOG_SCHEMA, PERSISTENCE_CATALOG_SCHEMA)

OFFICIAL_MASTERS: tuple[str, ...] = (
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "CaravanPack.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
)

DEFAULT_CATALOG_DIR = Path(__file__).resolve().parent / "catalogs"
STAGED_STEP_PREFIX = "nvseprobe_s_"
STAGED_LOOP_FILENAME = "nvseprobe_loop.gek"
STAGED_GR_FILENAME = "gr_nvseprobe.txt"
MANUAL_SETUP_FILENAME = "MANUAL_SETUP.txt"

LIVE_INSTALL_SENTINELS = (
    "FalloutNV.exe",
    "nvse_loader.exe",
    "Fallout.ini",
    "FalloutPrefs.ini",
    "plugins.txt",
    "loadorder.txt",
)

_SAFE_ID_RE = re.compile(r"^[a-zA-Z][a-zA-Z0-9_]*$")
_SAFE_FILENAME_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
_CTRL_CHAR_RE = re.compile(r"[\x00-\x08\x0b\x0c\x0e-\x1f\x7f]")
_WIN_DRIVE_PATH_RE = re.compile(
    r"(?<![A-Za-z0-9_])[A-Za-z]:(?:\\(?![\"'\\])|/)[A-Za-z0-9_.\-~]"
)
_POSIX_HOME_PATH_RE = re.compile(r"(?:/Users/|/home/|/mnt/[a-z]/)[A-Za-z0-9_.\-]+")
_UNC_PATH_RE = re.compile(r"\\\\[A-Za-z0-9_.\-]+\\[A-Za-z0-9_.\-]+")
_PRIVATE_MARKER_RE = re.compile(r"(?:\\Users\\|\bAppData\b|\bOneDrive\b)", re.IGNORECASE)
_PDB_RE = re.compile(r"\b[A-Za-z0-9_.-]+\.pdb\b", re.IGNORECASE)
_SAVE_FILE_RE = re.compile(
    r"(?<![<A-Za-z0-9_])([A-Za-z0-9_ .-]+?\.(?:fos|nvse))(?!\.[A-Za-z0-9_])\b",
    re.IGNORECASE,
)

# Unquoted bare EditorIDs that must not appear in runtime-compiled probe scripts
# (forms must be resolved via GetFormFromMod "FalloutNV.esm" "<hex>" or BuildRef).
_BARE_EDITORID_RE = re.compile(
    r"\b("
    r"Weap[A-Z][A-Za-z0-9_]+|"
    r"Armor[A-Z][A-Za-z0-9_]+|"
    r"Ammo[A-Z][A-Za-z0-9_]+|"
    r"GS[A-Z][A-Za-z0-9_]+|"
    r"DocMitchell[A-Za-z0-9_]*|"
    r"SunnySmiles[A-Za-z0-9_]*|"
    r"Vault\d+[A-Za-z0-9_]+|"
    r"WastelandNV|"
    r"Goodsprings[A-Za-z0-9_]*|"
    r"Stimpak|"
    r"Caps001|"
    r"NukaCola[A-Za-z0-9_]*|"
    r"RadAway|"
    r"VMS\d+[A-Za-z0-9_]*|"
    r"VCG\d+[A-Za-z0-9_]*|"
    r"NVWasteland[A-Za-z0-9_]+|"
    r"PlayerFaction|"
    r"DefaultWater|"
    r"IdleMarker[A-Za-z0-9_]+|"
    r"AnimObject[A-Za-z0-9_]+"
    r")\b"
)

_PROBE_LINE_RE = re.compile(r"(?:^|[\s:\]])PROBE\|([a-zA-Z0-9_]+)\|(.*)$")
_COMPILE_FAIL_RE = re.compile(
    r"(?:^|[\s:\]])(?:PROBE_COMPILE_FAIL|COMPILE_FAIL)\|([a-zA-Z0-9_]+)(?:\|(.*))?$"
)
_P2_LINE_RE = re.compile(r"(?:^|[\s:\]])P2\|(BEFORE|SAMERUN|COLD)\|([a-zA-Z0-9_]+)\|(.*)$")
_SR_LINE_RE = re.compile(r"(?:^|[\s:\]])SR\|(boot1|boot2)\|([a-zA-Z0-9_]+)$")

# Game-folder and Saves-folder file-writing commands forbidden in committed
# catalogs and emitted probe scripts (tools/capture/README.md §10).
FORBIDDEN_WRITER_COMMANDS: tuple[str, ...] = (
    # Console / save writers
    "con_Save",
    "con_SaveINI",
    "con_SCOF",
    "con_Screenshot",
    "SaveGame",
    "SaveINI",
    "Autosave",
    "DeleteSave",
    "SetConsoleOutputFile",
    "CaptureScreenshot",
    "TakeScreenshot",
    # General file / log / dump / QR writers
    "WriteStringToFile",
    "WriteArrayToFile",
    "WriteToFile",
    "WriteFile",
    "WriteTextFile",
    "WriteBinaryFile",
    "AppendToFile",
    "StringToFile",
    "StringToTextFile",
    "ArrayToFile",
    "FileFromString",
    "SaveToFile",
    "DeleteFile",
    "CopyFile",
    "RenameFile",
    "MoveFile",
    "CreateFile",
    "CreateDirectory",
    "DeleteDirectory",
    "RemoveDirectory",
    "ModLogPrint",
    "PrintToFile",
    "LogToFile",
    "SetLogFile",
    "DebugPrintToFile",
    "DumpLoadOrder",
    "TextToQRCode",
    # JSON writers
    "WriteToJSON",
    "WriteJSON",
    "JSONWrite",
    "JSONSave",
    "JSONAppend",
    "JSONRemove",
    "JSONSetValue",
    "JSONSetValueFloat",
    "JSONSetValueString",
    "JSONSetValueArray",
    "JSONCreateFile",
    # INI file writers / mutators (Data\Config or game folder)
    "SetINIFloat",
    "SetINIString",
    "SetINISection",
    "SetINIFloat_Cached",
    "SetINIString_Cached",
    "SetINISetting_Cached",
    "SaveCachedIniFile",
    "CloseFileSO",
    "RemoveINIKey",
    "RemoveINISection",
    "RemoveINIKey_Cached",
    "RemoveINISection_Cached",
    "ClearINISection",
    "WriteINIFloat",
    "WriteINIString",
    "WriteINISection",
    "SetINIValue",
    "SaveINIFile",
    # SUP Excel file writers / mutators
    "ExcelWriteFloat",
    "ExcelWriteString",
    "ExcelArrayToFile",
    "ExcelEraseSheet",
    "ExcelEraseCell",
    "ExcelSetSheetName",
    "ExcelWrite",
    "ExcelSave",
    "ExcelCreate",
)

_FORBIDDEN_WRITER_RE = re.compile(
    r"\b(" + "|".join(re.escape(cmd) for cmd in FORBIDDEN_WRITER_COMMANDS) + r")\b",
    re.IGNORECASE,
)


class ReadOnlyBoundaryError(PermissionError):
    """Raised when an operation attempts to write into a protected game or Saves directory."""


class PrivacyLeakError(ValueError):
    """Raised when text fails the automated clean-room privacy leak gate."""


@dataclass(frozen=True)
class ParsedLogs:
    probes: dict[str, str]
    compile_fails: dict[str, str]
    persistence: dict[str, dict[str, str]]
    script_runner: dict[str, list[str]]
    source_files: list[str]


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
    """Verify `out_dir` does not overlap `--game-dir`, `--saves-dir`, or a live FNV install.

    All comparisons use resolved real paths (`os.path.realpath`) normalized for
    platform case rules so symlinks, junctions, and `..` segments cannot bypass
    the boundary.
    """
    raw_out = Path(out_dir)
    real_out = _norm_real(raw_out)

    protected: list[tuple[str, str | os.PathLike[str]]] = []
    eff_game = game_dir if game_dir is not None else os.environ.get("FNV_GAME_DIR")
    eff_saves = saves_dir if saves_dir is not None else os.environ.get("FNV_SAVES_DIR")
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

    # Defense in depth: refuse if out_dir or any existing ancestor looks like a
    # live FNV install or My Games\FalloutNV INI/Saves directory.
    for ancestor in (real_out, *real_out.parents):
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


def _assert_child_dest(dest: Path, root: Path, label: str) -> Path:
    """Verify `dest` resolves strictly inside `root` before writing."""
    real_dest = _norm_real(dest)
    real_root = _norm_real(root)
    if not _is_same_or_child(real_dest, real_root) or real_dest == real_root:
        raise ValueError(f"Refusing to write {label} outside staging root ({root}): {dest}")
    return dest


def _validate_safe_rel_posix_path(rel: Any, label: str) -> str:
    """Validate that `rel` is a clean relative POSIX path with no traversal or drive prefix."""
    if not isinstance(rel, str) or not rel:
        raise ValueError(f"Unsafe {label} path: {rel!r}")
    if "\\" in rel or rel.startswith("/") or ":" in rel:
        raise ValueError(f"Unsafe {label} path (must be relative POSIX): {rel!r}")
    parts = rel.split("/")
    if any(part in ("", ".", "..") for part in parts):
        raise ValueError(f"Unsafe {label} path (traversal/empty segment): {rel!r}")
    return rel


def redact_save_names(text: str) -> str:
    """Replace concrete `.fos` / `.nvse` save filenames with `<save>.fos` / `<save>.nvse`."""
    def _repl(match: re.Match[str]) -> str:
        name = match.group(1)
        ext = ".fos" if name.lower().endswith(".fos") else ".nvse"
        return f"<save>{ext}"

    return _SAVE_FILE_RE.sub(_repl, text)


def check_privacy_leaks(text: str, *, allow_save_placeholder: bool = True) -> list[str]:
    """Return a list of privacy or hygiene violations found in `text`."""
    issues: list[str] = []
    if _CTRL_CHAR_RE.search(text):
        issues.append("contains disallowed ASCII control character")
    for m in _WIN_DRIVE_PATH_RE.finditer(text):
        issues.append(f"contains Windows drive path: {m.group(0)!r}")
    for m in _POSIX_HOME_PATH_RE.finditer(text):
        issues.append(f"contains POSIX home/mount path: {m.group(0)!r}")
    for m in _UNC_PATH_RE.finditer(text):
        issues.append(f"contains UNC path: {m.group(0)!r}")
    for m in _PRIVATE_MARKER_RE.finditer(text):
        issues.append(f"contains private user profile marker: {m.group(0)!r}")
    for m in _PDB_RE.finditer(text):
        issues.append(f"contains debug symbol filename: {m.group(0)!r}")
    for m in _SAVE_FILE_RE.finditer(text):
        token = m.group(1)
        if allow_save_placeholder and token.lower() in ("<save>.fos", "<save>.nvse"):
            continue
        issues.append(f"contains unredacted save filename: {token!r}")
    return issues


def strip_script_strings_and_comments(line: str) -> str:
    """Remove quoted literals and `;` comments from a GECK/NVSE script line."""
    out: list[str] = []
    in_quote = False
    i = 0
    while i < len(line):
        ch = line[i]
        if ch == '"':
            in_quote = not in_quote
            out.append(" ")
        elif ch == ";" and not in_quote:
            break
        elif not in_quote:
            out.append(ch)
        i += 1
    return "".join(out)


def find_bare_editorids(script_lines: list[str]) -> list[str]:
    """Return any unquoted bare vanilla EditorIDs found in `script_lines`."""
    hits: list[str] = []
    for line in script_lines:
        code = strip_script_strings_and_comments(line)
        for m in _BARE_EDITORID_RE.finditer(code):
            hits.append(m.group(1))
    return hits


def find_forbidden_writer_commands(script_lines: list[str]) -> list[str]:
    """Return any forbidden game/Saves file-writing commands found in `script_lines`."""
    hits: list[str] = []
    for line in script_lines:
        code = strip_script_strings_and_comments(line)
        for m in _FORBIDDEN_WRITER_RE.finditer(code):
            hits.append(m.group(1))
    return hits


def _detect_repo_root(start: Path | None = None) -> Path | None:
    cur = (start or Path(__file__).resolve()).resolve()
    if cur.is_file():
        cur = cur.parent
    for parent in (cur, *cur.parents):
        if (parent / "docs" / "spec-notes").is_dir():
            return parent
    return None


def load_catalog(path: str | os.PathLike[str]) -> dict[str, Any]:
    """Load a declarative catalog JSON file strictly read-only."""
    p = Path(path)
    return json.loads(p.read_text(encoding="utf-8"))


def iter_catalog_paths(
    *,
    catalog: str | os.PathLike[str] | None = None,
    catalog_dir: str | os.PathLike[str] | None = None,
) -> list[Path]:
    """Return sorted catalog JSON paths from `--catalog` or `--catalog-dir`."""
    if catalog is not None:
        p = Path(catalog)
        if not p.is_file():
            raise FileNotFoundError(f"Catalog file not found: {p}")
        return [p]
    cdir = Path(catalog_dir) if catalog_dir is not None else DEFAULT_CATALOG_DIR
    if not cdir.is_dir():
        raise FileNotFoundError(f"Catalog directory not found: {cdir}")
    paths = sorted(p for p in cdir.iterdir() if p.is_file() and p.suffix.lower() == ".json")
    if not paths:
        raise FileNotFoundError(f"No catalog JSON files found in: {cdir}")
    return paths


def _check_spec_file(spec_file: Any, repo_root: Path | None, prefix: str, errors: list[str]) -> None:
    if not isinstance(spec_file, str) or not spec_file:
        errors.append(f"{prefix}: missing or non-string spec_file")
        return
    if "\\" in spec_file or spec_file.startswith("/") or ".." in spec_file.split("/"):
        errors.append(f"{prefix}: spec_file must be a clean relative POSIX path: {spec_file!r}")
        return
    if not spec_file.startswith("docs/spec-notes/"):
        errors.append(f"{prefix}: spec_file must live under docs/spec-notes/: {spec_file!r}")
        return
    if repo_root is not None and not (repo_root / spec_file).is_file():
        errors.append(f"{prefix}: spec_file does not exist in repo: {spec_file}")


def _scan_strings_for_privacy(obj: Any, path_label: str, errors: list[str]) -> None:
    if isinstance(obj, str):
        for issue in check_privacy_leaks(obj):
            errors.append(f"{path_label}: privacy leak: {issue}")
    elif isinstance(obj, list):
        for idx, item in enumerate(obj):
            _scan_strings_for_privacy(item, f"{path_label}[{idx}]", errors)
    elif isinstance(obj, dict):
        for k, v in obj.items():
            _scan_strings_for_privacy(v, f"{path_label}.{k}", errors)


def validate_catalog_data(
    data: dict[str, Any],
    *,
    repo_root: Path | None = None,
    seen_ids: set[str] | None = None,
    source_name: str = "<catalog>",
) -> list[str]:
    """Validate one parsed catalog dict and return a list of error strings (empty = valid)."""
    errors: list[str] = []
    if not isinstance(data, dict):
        return [f"{source_name}: catalog root must be a JSON object"]

    schema = data.get("schema")
    if schema not in SUPPORTED_CATALOG_SCHEMAS:
        errors.append(f"{source_name}: unsupported schema {schema!r}")
        return errors

    for req_key in ("catalog_id", "description", "game_version", "xnvse_version", "official_masters", "extender_versions"):
        if req_key not in data or not data[req_key]:
            errors.append(f"{source_name}: missing required top-level field {req_key!r}")

    masters = data.get("official_masters")
    if isinstance(masters, list) and tuple(masters) != OFFICIAL_MASTERS:
        errors.append(f"{source_name}: official_masters must match the 10 canonical base masters")

    _scan_strings_for_privacy(data, source_name, errors)

    local_ids: set[str] = set()
    global_ids = seen_ids if seen_ids is not None else set()

    if schema == PROBE_CATALOG_SCHEMA:
        batches = data.get("batches")
        if not isinstance(batches, list) or not batches:
            errors.append(f"{source_name}: 'batches' must be a non-empty list")
            return errors
        batch_ids: set[str] = set()
        for b_idx, batch in enumerate(batches):
            b_prefix = f"{source_name}.batches[{b_idx}]"
            if not isinstance(batch, dict):
                errors.append(f"{b_prefix}: batch entry must be an object")
                continue
            bid = batch.get("id")
            if not isinstance(bid, str) or not _SAFE_ID_RE.match(bid):
                errors.append(f"{b_prefix}: invalid batch id {bid!r}")
            elif bid in batch_ids:
                errors.append(f"{b_prefix}: duplicate batch id {bid!r}")
            else:
                batch_ids.add(bid)

            probes = batch.get("probes")
            if not isinstance(probes, list) or not probes:
                errors.append(f"{b_prefix}: 'probes' must be a non-empty list")
                continue

            for p_idx, probe in enumerate(probes):
                p_prefix = f"{b_prefix}.probes[{p_idx}]"
                if not isinstance(probe, dict):
                    errors.append(f"{p_prefix}: probe entry must be an object")
                    continue
                pid = probe.get("id")
                if not isinstance(pid, str) or not _SAFE_ID_RE.match(pid):
                    errors.append(f"{p_prefix}: invalid probe id {pid!r}")
                else:
                    if pid in local_ids:
                        errors.append(f"{p_prefix}: duplicate probe id {pid!r}")
                    if pid in global_ids:
                        errors.append(f"{p_prefix}: probe id {pid!r} duplicates another catalog")
                    local_ids.add(pid)
                    global_ids.add(pid)
                    p_prefix = f"{source_name}:{pid}"

                _check_spec_file(probe.get("spec_file"), repo_root, p_prefix, errors)
                spec_line = probe.get("spec_line")
                if not isinstance(spec_line, int) or spec_line < 1:
                    errors.append(f"{p_prefix}: spec_line must be a positive integer")
                funcs = probe.get("functions_tested")
                if not isinstance(funcs, list) or not funcs or not all(isinstance(f, str) and f for f in funcs):
                    errors.append(f"{p_prefix}: functions_tested must be a non-empty list of strings")

                lines = probe.get("script_lines")
                if not isinstance(lines, list) or not lines or not all(isinstance(ln, str) for ln in lines):
                    errors.append(f"{p_prefix}: script_lines must be a non-empty list of strings")
                    continue

                all_probe_lines = list(lines)
                followup = probe.get("followup_script_lines", [])
                if followup:
                    if not isinstance(followup, list) or not all(isinstance(ln, str) for ln in followup):
                        errors.append(f"{p_prefix}: followup_script_lines must be a list of strings")
                    else:
                        all_probe_lines.extend(followup)
                extra_steps = probe.get("extra_steps", [])
                if extra_steps:
                    if not isinstance(extra_steps, list):
                        errors.append(f"{p_prefix}: extra_steps must be a list")
                    else:
                        seen_suffixes: set[str] = set()
                        for s_idx, step in enumerate(extra_steps):
                            if not isinstance(step, dict):
                                errors.append(f"{p_prefix}.extra_steps[{s_idx}]: step must be an object")
                                continue
                            sfx = step.get("suffix")
                            if not isinstance(sfx, str) or not _SAFE_ID_RE.match(sfx):
                                errors.append(f"{p_prefix}.extra_steps[{s_idx}]: invalid suffix {sfx!r}")
                            elif sfx in seen_suffixes:
                                errors.append(f"{p_prefix}.extra_steps[{s_idx}]: duplicate suffix {sfx!r}")
                            else:
                                seen_suffixes.add(sfx)
                            wt = step.get("wait_ticks", 0)
                            if not isinstance(wt, int) or isinstance(wt, bool) or wt < 0:
                                errors.append(f"{p_prefix}.extra_steps[{s_idx}]: wait_ticks must be a non-negative int")
                            slines = step.get("script_lines")
                            if not isinstance(slines, list) or not slines or not all(isinstance(ln, str) for ln in slines):
                                errors.append(f"{p_prefix}.extra_steps[{s_idx}]: script_lines must be a non-empty list of strings")
                            else:
                                all_probe_lines.extend(slines)
                helpers = probe.get("helper_scripts", {})
                if isinstance(helpers, dict):
                    for hname, hlines in helpers.items():
                        if not _SAFE_FILENAME_RE.match(hname):
                            errors.append(f"{p_prefix}: unsafe helper script name {hname!r}")
                        if isinstance(hlines, list):
                            all_probe_lines.extend(hlines)

                writers = find_forbidden_writer_commands(all_probe_lines)
                if writers:
                    errors.append(
                        f"{p_prefix}: forbidden game/Saves file-writing command(s) in script_lines: {sorted(set(writers))} "
                        "(committed catalogs must be strictly read-only console probes)"
                    )
                bare = find_bare_editorids(all_probe_lines)
                if bare:
                    errors.append(f"{p_prefix}: bare EditorID(s) in script_lines: {sorted(set(bare))}")

                if isinstance(pid, str):
                    marker = f"PROBE|{pid}|"
                    if not any(marker in ln for ln in all_probe_lines):
                        errors.append(f"{p_prefix}: script_lines must emit {marker!r}")

                fixtures = probe.get("fixtures", [])
                if fixtures:
                    if not isinstance(fixtures, list):
                        errors.append(f"{p_prefix}: fixtures must be a list")
                    else:
                        for f_idx, fx in enumerate(fixtures):
                            rel = fx.get("rel_path") if isinstance(fx, dict) else None
                            if (
                                not isinstance(rel, str)
                                or not rel
                                or "\\" in rel
                                or rel.startswith("/")
                                or ".." in rel.split("/")
                            ):
                                errors.append(f"{p_prefix}.fixtures[{f_idx}]: unsafe rel_path {rel!r}")

    elif schema == PERSISTENCE_CATALOG_SCHEMA:
        items = data.get("items")
        if not isinstance(items, list) or not items:
            errors.append(f"{source_name}: 'items' must be a non-empty list")
            return errors
        valid_tiers = {"SAVED", "SESSION_ONLY", "CLEARED_ON_LOAD", "UNCHANGED", "OBSERVE_ONLY"}
        for idx, item in enumerate(items):
            ipref = f"{source_name}.items[{idx}]"
            if not isinstance(item, dict):
                errors.append(f"{ipref}: item must be an object")
                continue
            iid = item.get("id")
            if not isinstance(iid, str) or not _SAFE_ID_RE.match(iid):
                errors.append(f"{ipref}: invalid item id {iid!r}")
            else:
                if iid in local_ids or iid in global_ids:
                    errors.append(f"{ipref}: duplicate id {iid!r}")
                local_ids.add(iid)
                global_ids.add(iid)
                ipref = f"{source_name}:{iid}"

            key = item.get("key")
            if not isinstance(key, str) or not _SAFE_ID_RE.match(key):
                errors.append(f"{ipref}: invalid persistence key {key!r}")
            tier = item.get("expected_tier")
            if tier not in valid_tiers:
                errors.append(f"{ipref}: invalid expected_tier {tier!r}")
            _check_spec_file(item.get("spec_file"), repo_root, ipref, errors)
            if not isinstance(item.get("spec_line"), int) or item["spec_line"] < 1:
                errors.append(f"{ipref}: spec_line must be a positive integer")
            funcs = item.get("functions_tested")
            if not isinstance(funcs, list) or not funcs:
                errors.append(f"{ipref}: functions_tested must be a non-empty list")

        for block_name in ("mutate_script_lines", "readback_script_lines"):
            lines = data.get(block_name)
            if not isinstance(lines, list) or not lines:
                errors.append(f"{source_name}: missing {block_name}")
                continue
            writers = find_forbidden_writer_commands(lines)
            if writers:
                errors.append(
                    f"{source_name}.{block_name}: forbidden game/Saves file-writing command(s): {sorted(set(writers))}"
                )
            bare = find_bare_editorids(lines)
            if bare:
                errors.append(f"{source_name}.{block_name}: bare EditorID(s): {sorted(set(bare))}")

        sr = data.get("script_runner_lifecycle")
        if sr is not None:
            if not isinstance(sr, dict):
                errors.append(f"{source_name}: script_runner_lifecycle must be an object")
            else:
                srid = sr.get("id")
                if not isinstance(srid, str) or not _SAFE_ID_RE.match(srid):
                    errors.append(f"{source_name}: invalid script_runner_lifecycle id {srid!r}")
                elif srid in local_ids or srid in global_ids:
                    errors.append(f"{source_name}: duplicate id {srid!r}")
                else:
                    local_ids.add(srid)
                    global_ids.add(srid)
                _check_spec_file(sr.get("spec_file"), repo_root, f"{source_name}:script_runner_lifecycle", errors)

    return errors


def validate_catalogs(
    paths: list[Path],
    *,
    repo_root: Path | None = None,
) -> tuple[list[str], dict[str, int]]:
    """Validate one or more catalog files and return `(errors, summary_counts)`."""
    effective_root = repo_root if repo_root is not None else _detect_repo_root()
    errors: list[str] = []
    seen_ids: set[str] = set()
    counts = {"catalogs": 0, "probe_batches": 0, "probes": 0, "persistence_items": 0}
    for path in paths:
        counts["catalogs"] += 1
        try:
            data = load_catalog(path)
        except Exception as exc:
            errors.append(f"{path.name}: failed to parse JSON: {exc}")
            continue
        errors.extend(
            validate_catalog_data(
                data,
                repo_root=effective_root,
                seen_ids=seen_ids,
                source_name=path.name,
            )
        )
        if data.get("schema") == PROBE_CATALOG_SCHEMA:
            for batch in data.get("batches", []):
                counts["probe_batches"] += 1
                counts["probes"] += len(batch.get("probes", []))
        elif data.get("schema") == PERSISTENCE_CATALOG_SCHEMA:
            counts["persistence_items"] += len(data.get("items", []))
            if data.get("script_runner_lifecycle"):
                counts["persistence_items"] += 1
    return errors, counts


def _render_udf(lines: list[str], *, header: str = "begin Function {}") -> bytes:
    rendered = [header]
    for ln in lines:
        rendered.append("  " + ln)
    rendered.append("end")
    return ("\r\n".join(rendered) + "\r\n").encode("ascii")


def _manual_setup_guide(
    *,
    catalog_id: str,
    stage_root: Path,
    probe_count: int,
    required_plugins: list[str],
    fixture_paths: list[str],
) -> str:
    lines = [
        f"Catalog: {catalog_id} ({probe_count} probes)",
        "Output mode: PrintToConsole (console-only; emitted probe scripts write zero files to the game or Saves folder)",
        f"Staged directory: {stage_root}",
        "",
        "STRICT READ-ONLY GUARANTEE (tools/capture/README.md §10):",
        "- This tool NEVER writes into your Fallout: New Vegas install directory,",
        "  Data\\, NVSE\\, Fallout.ini, or Saves folder, and NEVER launches the game.",
        "- All emitted probe scripts log strictly via PrintToConsole and contain zero",
        "  file-writing or con_Save commands (validated before emission).",
        "- Copying the staged scripts (and any optional read-only fixtures below) into",
        "  an isolated staging copy is a manual one-time step performed by you; standard",
        "  extender startup logs such as nvse.log are produced by the extenders themselves.",
        "",
        "One-time manual setup (performed by you in an isolated staging copy or backed-up install):",
        "1. Prepare Fallout: New Vegas 1.4.0.525 (English) with ONLY the 10 official base masters:",
        "   " + ", ".join(OFFICIAL_MASTERS),
        "2. Install xNVSE 6.4.9+ and the required public extender DLLs for your selected batch:",
        "   " + (", ".join(required_plugins) if required_plugins else "(xNVSE 6.4.9 + JIP LN NVSE)"),
        "3. Back up Documents\\My Games\\FalloutNV\\Fallout.ini and your Saves folder by hand.",
        "4. Inspect the generated console-only scripts under Data\\NVSE\\ inside this staged folder,",
        "   then copy them into <staging-FNV>\\Data\\NVSE\\ by hand:",
        f"   - Data\\NVSE\\Plugins\\Scripts\\{STAGED_GR_FILENAME}",
        f"   - Data\\NVSE\\user_defined_functions\\{STAGED_LOOP_FILENAME}",
        f"   - Data\\NVSE\\user_defined_functions\\{STAGED_STEP_PREFIX}*.gek",
    ]
    if fixture_paths:
        lines.append("   Optional read-only input fixtures (copy into <staging-FNV>\\ only if running those probes):")
        for fp in fixture_paths:
            lines.append(f"   - fixtures/{fp}")
    lines.extend(
        [
            "5. Run the game manually and capture console output into a private directory outside git,",
            "   then verify read-only:",
            "   python tools/capture/nvse/harness.py verify --log-file <private-log>",
            "6. When finished, delete all copied gr_nvseprobe.txt / nvseprobe_*.gek files",
            "   and any staged input fixtures by hand, and restore your Fallout.ini backup.",
            "",
        ]
    )
    return "\n".join(lines)


def emit_catalog(
    catalog_path: str | os.PathLike[str],
    out_dir: str | os.PathLike[str],
    *,
    game_dir: str | os.PathLike[str] | None = None,
    saves_dir: str | os.PathLike[str] | None = None,
    batches: list[str] | None = None,
    probe_ids: list[str] | None = None,
    interval: int = 10,
    exclude_optional: bool = False,
    exclude_staging_only: bool = False,
) -> dict[str, Any]:
    """Emit console-only clean-room NVSE probe scripts and `MANUAL_SETUP.txt` strictly into `out_dir`."""
    odir = assert_safe_out_dir(out_dir, game_dir=game_dir, saves_dir=saves_dir)
    data = load_catalog(catalog_path)
    schema = data.get("schema")

    if schema == PERSISTENCE_CATALOG_SCHEMA:
        raise ReadOnlyBoundaryError(
            "Refusing to emit persistence catalog scripts because two-boot capture requires "
            "con_Save into the Saves folder (tools/capture/README.md §10); use 'verify' and "
            "'cite' strictly read-only on pre-captured logs."
        )

    if schema == PROBE_CATALOG_SCHEMA:
        batch_filter = set(batches) if batches else None
        probe_filter = set(probe_ids) if probe_ids else None
        selected: list[dict[str, Any]] = []
        req_plugins: set[str] = set()
        for batch in data["batches"]:
            if batch_filter is not None and batch["id"] not in batch_filter:
                continue
            for probe in batch["probes"]:
                if probe_filter is not None and probe["id"] not in probe_filter:
                    continue
                if exclude_optional and probe.get("optional_extender", False):
                    continue
                if exclude_staging_only and (
                    probe.get("requires_fixture", False) or probe.get("requires_isolated_staging", False)
                ):
                    continue
                selected.append(probe)
                for pl in probe.get("required_plugins", batch.get("required_plugins", [])):
                    req_plugins.add(pl)

        if not selected:
            raise ValueError("No probes matched the requested filters.")

        # Pre-validate all probe IDs, helper filenames, extra steps, fixture relative
        # paths, and forbidden file-writing commands before creating any files.
        for probe in selected:
            pid = probe.get("id")
            if not isinstance(pid, str) or not _SAFE_ID_RE.match(pid):
                raise ValueError(f"Unsafe probe id: {pid!r}")
            all_lines = list(probe.get("script_lines", []))
            followup = probe.get("followup_script_lines", [])
            if followup:
                all_lines.extend(followup)
            seen_sfx: set[str] = set()
            for step in probe.get("extra_steps", []):
                sfx = step.get("suffix") if isinstance(step, dict) else None
                if not isinstance(sfx, str) or not _SAFE_ID_RE.match(sfx) or sfx in seen_sfx:
                    raise ValueError(f"Unsafe or duplicate extra_steps suffix for {pid}: {sfx!r}")
                seen_sfx.add(sfx)
                wt = step.get("wait_ticks", 0)
                if not isinstance(wt, int) or isinstance(wt, bool) or wt < 0:
                    raise ValueError(f"Invalid wait_ticks for {pid}.{sfx}: {wt!r}")
                slines = step.get("script_lines")
                if not isinstance(slines, list) or not slines:
                    raise ValueError(f"Empty extra_steps script_lines for {pid}.{sfx}")
                all_lines.extend(slines)
            for hname, hlines in probe.get("helper_scripts", {}).items():
                if not isinstance(hname, str) or not _SAFE_FILENAME_RE.match(hname):
                    raise ValueError(f"Unsafe helper script filename: {hname!r}")
                if isinstance(hlines, list):
                    all_lines.extend(hlines)
            writers = find_forbidden_writer_commands(all_lines)
            if writers:
                raise ReadOnlyBoundaryError(
                    f"Refusing to emit probe {pid!r} containing forbidden game/Saves file-writing "
                    f"command(s): {sorted(set(writers))}"
                )
            for fx in probe.get("fixtures", []):
                _validate_safe_rel_posix_path(
                    fx.get("rel_path") if isinstance(fx, dict) else None,
                    f"fixture for {pid}",
                )

        scripts_dir = odir / "Data" / "NVSE" / "Plugins" / "Scripts"
        udf_dir = odir / "Data" / "NVSE" / "user_defined_functions"
        scripts_dir.mkdir(parents=True, exist_ok=True)
        udf_dir.mkdir(parents=True, exist_ok=True)

        # Clean prior harness-owned files strictly inside out_dir.
        gr_path = _assert_child_dest(scripts_dir / STAGED_GR_FILENAME, odir, "gr script")
        if gr_path.is_file():
            gr_path.unlink()
        for item in udf_dir.iterdir():
            if item.is_file() and (
                item.name.startswith(STAGED_STEP_PREFIX) or item.name == STAGED_LOOP_FILENAME
            ):
                item.unlink()

        emitted_scripts: list[Path] = []
        fixture_paths: list[str] = []
        loop_steps: list[tuple[str, str, int]] = []
        for probe in selected:
            pid = probe["id"]
            fn = f"{STAGED_STEP_PREFIX}{pid}.gek"
            sp = _assert_child_dest(udf_dir / fn, odir, "probe script")
            sp.write_bytes(_render_udf(probe["script_lines"]))
            emitted_scripts.append(sp)
            init_wait = int(probe.get("wait_ticks_after", 0))
            loop_steps.append((pid, fn, init_wait))
            for hname, hlines in probe.get("helper_scripts", {}).items():
                hp = _assert_child_dest(udf_dir / hname, odir, "helper script")
                has_header = bool(hlines and hlines[0].lower().startswith("begin function"))
                header = hlines[0] if has_header else "begin Function {}"
                body = hlines[1:] if has_header else hlines
                hp.write_bytes(_render_udf(body, header=header))
                emitted_scripts.append(hp)
            followup = probe.get("followup_script_lines")
            if followup:
                ffn = f"{STAGED_STEP_PREFIX}{pid}_post.gek"
                fsp = _assert_child_dest(udf_dir / ffn, odir, "followup script")
                fsp.write_bytes(_render_udf(followup))
                emitted_scripts.append(fsp)
                loop_steps.append((f"{pid}_post", ffn, 0))
            for step in probe.get("extra_steps", []):
                sfx = step["suffix"]
                efn = f"{STAGED_STEP_PREFIX}{pid}_{sfx}.gek"
                esp = _assert_child_dest(udf_dir / efn, odir, "extra step script")
                esp.write_bytes(_render_udf(step["script_lines"]))
                emitted_scripts.append(esp)
                loop_steps.append((f"{pid}_{sfx}", efn, int(step.get("wait_ticks", 0))))
            for fx in probe.get("fixtures", []):
                rel = _validate_safe_rel_posix_path(fx["rel_path"], f"fixture for {pid}")
                fdest = _assert_child_dest(odir / "fixtures" / rel, odir, "fixture file")
                fdest.parent.mkdir(parents=True, exist_ok=True)
                fdest.write_text(fx["content"], encoding="utf-8", newline="\r\n")
                fixture_paths.append(rel)

        # Build main-loop runner UDF (strictly PrintToConsole).
        loop_lines = [
            'int st = Player.AuxVarGetFlt "*np_st"',
            "int mm = GetActiveMenuMode",
            'Player.AuxVarSetFlt "*np_st" (st + 1)',
            "if mm == 1013",
            "  if st == 4",
            "    CenterOnCell GSDocMitchellHouse",
            "  endif",
            "  return",
            'elseif mm != 0 && (Player.AuxVarGetFlt "*np_keepmenu") == 0',
            "  CloseActiveMenu",
            '  ClickMenuButton "MessageMenu#1"',
            "  return",
            "endif",
            'int wt = Player.AuxVarGetFlt "*np_wt"',
            "if wt > 0",
            '  Player.AuxVarSetFlt "*np_wt" (wt - 1)',
            "  return",
            "endif",
            'int ph = Player.AuxVarGetFlt "*np_ph"',
            'Player.AuxVarSetFlt "*np_ph" (ph + 1)',
            "ref rFn",
            "if ph == 0",
            "  EnablePlayerControls 1 1 1 1 1 1 1",
            '  Player.AuxVarSetFlt "*np_wt" 4',
        ]
        for idx, (step_id, fn, wait_after) in enumerate(loop_steps, 1):
            loop_lines.extend(
                [
                    f"elseif ph == {idx}",
                    f'  let rFn := CompileScript "{fn}"',
                    "  if rFn",
                    "    Call rFn",
                    "  else",
                    f'    PrintToConsole "PROBE_COMPILE_FAIL|{step_id}"',
                    "  endif",
                ]
            )
            if wait_after > 0:
                loop_lines.append(f'  Player.AuxVarSetFlt "*np_wt" {wait_after}')
        loop_lines.extend(
            [
                f"elseif ph == {len(loop_steps) + 1}",
                '  PrintToConsole "PROBE_DONE"',
                "  con_QuitGame",
                "endif",
            ]
        )
        loop_path = _assert_child_dest(udf_dir / STAGED_LOOP_FILENAME, odir, "loop script")
        loop_path.write_bytes(
            ("begin Function {}\r\n" + "\r\n".join(f"  {ln}" for ln in loop_lines) + "\r\nend\r\n").encode("ascii")
        )

        gr_lines = [
            f'PrintToConsole "NVSEPROBE START {data["catalog_id"]}"',
            'Player.AuxVarSetFlt "*np_st" 0',
            'Player.AuxVarSetFlt "*np_ph" 0',
            'Player.AuxVarSetFlt "*np_wt" 0',
            'Player.AuxVarSetFlt "*np_keepmenu" 0',
            f'SetGameMainLoopCallback (CompileScript "{STAGED_LOOP_FILENAME}") 1 {interval} 3',
        ]
        gr_path.write_bytes(("\r\n".join(gr_lines) + "\r\n").encode("ascii"))

        setup_path = _assert_child_dest(odir / MANUAL_SETUP_FILENAME, odir, "setup guide")
        setup_path.write_text(
            _manual_setup_guide(
                catalog_id=data["catalog_id"],
                stage_root=odir,
                probe_count=len(selected),
                required_plugins=sorted(req_plugins),
                fixture_paths=fixture_paths,
            ),
            encoding="utf-8",
            newline="\n",
        )
        return {
            "catalog_id": data["catalog_id"],
            "out_dir": str(odir),
            "emitted_probes": len(selected),
            "output_mode": "console",
            "gr_path": str(gr_path),
            "loop_path": str(loop_path),
            "setup_path": str(setup_path),
            "fixtures": fixture_paths,
        }

    raise ValueError(f"Unsupported catalog schema: {schema!r}")


def parse_probe_logs(
    *,
    log_files: list[str | os.PathLike[str]] | None = None,
    log_dir: str | os.PathLike[str] | None = None,
) -> ParsedLogs:
    """Read captured `PROBE|`, `P2|`, and `SR|` lines strictly read-only."""
    paths: list[Path] = []
    if log_files:
        for lf in log_files:
            p = Path(lf)
            if not p.is_file():
                raise FileNotFoundError(f"Log file not found: {p}")
            paths.append(p)
    if log_dir is not None:
        ldir = Path(log_dir)
        if not ldir.is_dir():
            raise FileNotFoundError(f"Log directory not found: {ldir}")
        for entry in sorted(ldir.rglob("*")):
            if entry.is_file() and entry.suffix.lower() in (".log", ".txt"):
                paths.append(entry)
    if not paths:
        raise ValueError("Provide at least one --log-file or --log-dir containing .log/.txt files.")

    probes: dict[str, str] = {}
    compile_fails: dict[str, str] = {}
    persistence: dict[str, dict[str, str]] = {}
    script_runner: dict[str, list[str]] = {"boot1": [], "boot2": []}
    seen_sources: list[str] = []

    for path in paths:
        seen_sources.append(path.name)
        text = path.read_text(encoding="utf-8", errors="replace")
        for raw_line in text.splitlines():
            line = raw_line.strip()
            if not line:
                continue
            m_p2 = _P2_LINE_RE.search(line)
            if m_p2:
                phase, key, val = m_p2.groups()
                persistence.setdefault(key, {})[phase] = val.strip()
                continue
            m_sr = _SR_LINE_RE.search(line)
            if m_sr:
                boot, event = m_sr.groups()
                script_runner.setdefault(boot, []).append(event)
                continue
            m_cf = _COMPILE_FAIL_RE.search(line)
            if m_cf:
                pid, reason = m_cf.groups()
                compile_fails[pid] = (reason or "COMPILE_FAIL").strip()
                continue
            m_pr = _PROBE_LINE_RE.search(line)
            if m_pr:
                pid, result = m_pr.groups()
                probes[pid] = result.strip()

    return ParsedLogs(
        probes=probes,
        compile_fails=compile_fails,
        persistence=persistence,
        script_runner=script_runner,
        source_files=seen_sources,
    )


def _extract_field(value: str | None, field: str | None) -> str | None:
    if value is None or not field:
        return value
    for part in value.split(","):
        if "=" in part:
            k, v = part.split("=", 1)
            if k.strip() == field:
                return v.strip()
    return None


def classify_persistence_tier(
    before: str | None,
    samerun: str | None,
    cold: str | None,
    *,
    baseline: str | None = None,
    field: str | None = None,
) -> str:
    """Classify two-boot save/load persistence into SAVED / SESSION_ONLY / CLEARED_ON_LOAD.

    - `SAVED`: survives both same-run reload (`SAMERUN == BEFORE`) and fresh
      cold-boot load (`COLD == BEFORE`), and differs from `baseline` if given.
    - `SESSION_ONLY`: survives same-run reload (`SAMERUN == BEFORE`), resets on
      fresh cold-boot load (`COLD != BEFORE`).
    - `CLEARED_ON_LOAD`: resets immediately on same-run reload (`SAMERUN != BEFORE`)
      and remains reset on cold-boot load (`COLD != BEFORE`).
    - `UNCHANGED`: all three phases equal `baseline` (mutation had no readable effect).
    - `INCOMPLETE`: one or more of `BEFORE`, `SAMERUN`, `COLD` is missing.
    - `ANOMALOUS`: `SAMERUN != BEFORE` while `COLD == BEFORE`.
    """
    b = _extract_field(before, field)
    s = _extract_field(samerun, field)
    c = _extract_field(cold, field)
    if b is None or s is None or c is None:
        return "INCOMPLETE"
    if baseline is not None and b == baseline and s == baseline and c == baseline:
        return "UNCHANGED"
    if s == b and c == b:
        return "SAVED"
    if s == b and c != b:
        return "SESSION_ONLY"
    if s != b and c != b:
        return "CLEARED_ON_LOAD"
    return "ANOMALOUS"


def _evaluate_match(observed: str, rule: dict[str, Any] | None) -> str:
    if not rule:
        return "OBSERVED"
    kind = rule.get("type")
    val = rule.get("value", "")
    if kind == "exact":
        return "MATCH" if observed == val else "MISMATCH"
    if kind == "regex":
        return "MATCH" if re.search(val, observed) else "MISMATCH"
    if kind == "contains":
        return "MATCH" if val in observed else "MISMATCH"
    if kind == "prefix":
        return "MATCH" if observed.startswith(val) else "MISMATCH"
    return "OBSERVED"


def verify_logs(
    catalog_paths: list[Path],
    *,
    log_files: list[str | os.PathLike[str]] | None = None,
    log_dir: str | os.PathLike[str] | None = None,
    redact_saves: bool = False,
) -> dict[str, Any]:
    """Compare read-only captured logs against declarative catalogs and run the privacy gate."""
    parsed = parse_probe_logs(log_files=log_files, log_dir=log_dir)

    entries: list[dict[str, Any]] = []
    persistence_entries: list[dict[str, Any]] = []
    privacy_violations: list[str] = []
    counts = {
        "total_probes": 0,
        "observed": 0,
        "match": 0,
        "mismatch": 0,
        "compile_fail": 0,
        "missing": 0,
        "persistence_total": 0,
        "persistence_match": 0,
        "persistence_mismatch": 0,
        "persistence_incomplete": 0,
    }
    metadata: dict[str, Any] = {
        "game_version": "",
        "xnvse_version": "",
        "official_masters": list(OFFICIAL_MASTERS),
        "extender_versions": {},
    }

    for cpath in catalog_paths:
        data = load_catalog(cpath)
        if not metadata["game_version"]:
            metadata["game_version"] = data.get("game_version", "")
            metadata["xnvse_version"] = data.get("xnvse_version", "")
        metadata["extender_versions"].update(data.get("extender_versions", {}))
        schema = data.get("schema")

        if schema == PROBE_CATALOG_SCHEMA:
            for batch in data["batches"]:
                for probe in batch["probes"]:
                    counts["total_probes"] += 1
                    pid = probe["id"]
                    raw_obs = parsed.probes.get(pid)
                    cf_reason = parsed.compile_fails.get(pid)
                    if raw_obs is not None:
                        obs = redact_save_names(raw_obs) if redact_saves else raw_obs
                        for leak in check_privacy_leaks(obs):
                            privacy_violations.append(f"{pid}: {leak}")
                        status = _evaluate_match(obs, probe.get("match"))
                        if status == "MATCH":
                            counts["match"] += 1
                            counts["observed"] += 1
                        elif status == "MISMATCH":
                            counts["mismatch"] += 1
                            counts["observed"] += 1
                        else:
                            counts["observed"] += 1
                    elif cf_reason is not None:
                        obs = redact_save_names(cf_reason) if redact_saves else cf_reason
                        for leak in check_privacy_leaks(obs):
                            privacy_violations.append(f"{pid}: {leak}")
                        status = "COMPILE_FAIL"
                        counts["compile_fail"] += 1
                    else:
                        obs = ""
                        status = "MISSING"
                        counts["missing"] += 1

                    entries.append(
                        {
                            "id": pid,
                            "catalog": data["catalog_id"],
                            "batch": batch["id"],
                            "extender": probe.get("extender", batch.get("extender", "")),
                            "spec_file": probe["spec_file"],
                            "spec_line": probe["spec_line"],
                            "functions_tested": probe["functions_tested"],
                            "status": status,
                            "observed": obs,
                        }
                    )

        elif schema == PERSISTENCE_CATALOG_SCHEMA:
            for item in data["items"]:
                counts["persistence_total"] += 1
                key = item["key"]
                phases = parsed.persistence.get(key, {})
                b = phases.get("BEFORE")
                s = phases.get("SAMERUN")
                c = phases.get("COLD")
                if redact_saves:
                    b = redact_save_names(b) if b is not None else None
                    s = redact_save_names(s) if s is not None else None
                    c = redact_save_names(c) if c is not None else None
                for tag, val in (("BEFORE", b), ("SAMERUN", s), ("COLD", c)):
                    if val is not None:
                        for leak in check_privacy_leaks(val):
                            privacy_violations.append(f"{item['id']} ({tag}): {leak}")

                observed_tier = classify_persistence_tier(
                    b,
                    s,
                    c,
                    baseline=item.get("baseline"),
                    field=item.get("field"),
                )
                exp_tier = item["expected_tier"]
                if observed_tier == "INCOMPLETE":
                    verdict = "INCOMPLETE"
                    counts["persistence_incomplete"] += 1
                elif exp_tier == "OBSERVE_ONLY":
                    verdict = "OBSERVED"
                    counts["persistence_match"] += 1
                elif observed_tier == exp_tier:
                    verdict = "MATCH"
                    counts["persistence_match"] += 1
                else:
                    verdict = "MISMATCH"
                    counts["persistence_mismatch"] += 1

                persistence_entries.append(
                    {
                        "id": item["id"],
                        "key": key,
                        "extender": item.get("extender", ""),
                        "spec_file": item["spec_file"],
                        "spec_line": item["spec_line"],
                        "functions_tested": item["functions_tested"],
                        "before": b or "",
                        "samerun": s or "",
                        "cold": c or "",
                        "expected_tier": exp_tier,
                        "observed_tier": observed_tier,
                        "verdict": verdict,
                    }
                )

    if privacy_violations:
        raise PrivacyLeakError(
            "Privacy gate rejected captured log output:\n  - " + "\n  - ".join(privacy_violations)
        )

    return {
        "schema": SCOREBOARD_SCHEMA,
        "metadata": metadata,
        "log_sources": parsed.source_files,
        "counts": counts,
        "probes": entries,
        "persistence": persistence_entries,
        "script_runner": parsed.script_runner,
    }


def format_citation(
    entry: dict[str, Any],
    metadata: dict[str, Any],
    *,
    is_persistence: bool = False,
) -> str:
    """Format a clean-room `[observed: ...]` citation and enforce the privacy gate."""
    game_ver = metadata.get("game_version") or "Fallout: New Vegas 1.4.0.525"
    xnvse_ver = metadata.get("xnvse_version") or "6.4.9"
    extender = entry.get("extender") or "xNVSE core"
    funcs = "/".join(entry.get("functions_tested") or [])
    pid = entry["id"]

    if is_persistence:
        detail = (
            f"BEFORE={entry['before']}, SAMERUN={entry['samerun']}, "
            f"COLD={entry['cold']} -> {entry['observed_tier']}"
        )
        citation = (
            f"[observed: {game_ver} + xNVSE {xnvse_ver} + {extender} with the 10 official base masters, "
            f"clean-room two-boot persistence probe `{pid}` ({funcs}): {detail}; "
            f"no binary, DLL, or source consulted]"
        )
    else:
        obs = entry.get("observed", "").replace("|", " ; ")
        citation = (
            f"[observed: {game_ver} + xNVSE {xnvse_ver} + {extender} with the 10 official base masters, "
            f"clean-room black-box script probe `{pid}` ({funcs}): {obs}; "
            f"no binary, DLL, or source consulted]"
        )

    leaks = check_privacy_leaks(citation)
    if leaks:
        raise PrivacyLeakError(f"Citation for {pid} failed privacy gate: {'; '.join(leaks)}")
    return citation


def render_markdown_scoreboard(scoreboard: dict[str, Any]) -> str:
    """Render a clean-room Markdown scoreboard from a verified scoreboard dict."""
    c = scoreboard["counts"]
    lines = [
        "# NVSE Clean-Room Black-Box Probe Scoreboard",
        "",
        f"- **Game:** {scoreboard['metadata'].get('game_version', 'Fallout: New Vegas 1.4.0.525')}",
        f"- **xNVSE:** {scoreboard['metadata'].get('xnvse_version', '6.4.9')}",
        f"- **Probes:** total={c['total_probes']}, observed={c['observed']}, "
        f"match={c['match']}, mismatch={c['mismatch']}, "
        f"compile_fail={c['compile_fail']}, missing={c['missing']}",
    ]
    if c["persistence_total"]:
        lines.append(
            f"- **Persistence (3-tier):** total={c['persistence_total']}, "
            f"match={c['persistence_match']}, mismatch={c['persistence_mismatch']}, "
            f"incomplete={c['persistence_incomplete']}"
        )
    lines.append("")

    observed_probes = [p for p in scoreboard["probes"] if p["status"] != "MISSING"]
    if observed_probes:
        lines.extend(
            [
                "## Probe Observations",
                "",
                "| Probe ID | Spec-Note | Status | Observed |",
                "|---|---|---|---|",
            ]
        )
        for p in observed_probes:
            obs_safe = p["observed"].replace("|", "\\|")
            lines.append(
                f"| `{p['id']}` | `{p['spec_file']}:{p['spec_line']}` | **{p['status']}** | `{obs_safe}` |"
            )
        lines.append("")

    if scoreboard["persistence"]:
        lines.extend(
            [
                "## Save/Load Persistence Matrix (3-Tier)",
                "",
                "| Item ID | Spec-Note | Before | Same-Run | Cold | Observed Tier | Expected | Verdict |",
                "|---|---|---|---|---|---|---|---|",
            ]
        )
        for p in scoreboard["persistence"]:
            lines.append(
                f"| `{p['id']}` | `{p['spec_file']}:{p['spec_line']}` | "
                f"`{p['before']}` | `{p['samerun']}` | `{p['cold']}` | "
                f"**{p['observed_tier']}** | `{p['expected_tier']}` | **{p['verdict']}** |"
            )
        lines.append("")

    md = "\n".join(lines)
    leaks = check_privacy_leaks(md)
    if leaks:
        raise PrivacyLeakError("Markdown scoreboard failed privacy gate: " + "; ".join(leaks))
    return md


def build_arg_parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(
        description=(
            "Strictly read-only clean-room NVSE/extender black-box probe harness. "
            "Never writes into --game-dir, Data\\, NVSE\\, Fallout.ini, or Saves."
        )
    )
    sub = ap.add_subparsers(dest="command", required=True)

    p_list = sub.add_parser("list", help="List declarative catalogs, batches, and probe counts.")
    p_list.add_argument("--catalog", default=None, help="Single catalog JSON file.")
    p_list.add_argument("--catalog-dir", default=None, help="Directory of catalog JSON files.")
    p_list.add_argument("--json", action="store_true", help="Emit machine-readable JSON summary.")

    p_val = sub.add_parser("validate", help="Validate catalog schemas, unique IDs, spec paths, EditorIDs, and privacy.")
    p_val.add_argument("--catalog", default=None, help="Single catalog JSON file.")
    p_val.add_argument("--catalog-dir", default=None, help="Directory of catalog JSON files.")
    p_val.add_argument("--repo-root", default=None, help="Repository root for verifying spec_file existence.")

    p_emit = sub.add_parser("emit", help="Emit console-only probe scripts + MANUAL_SETUP.txt strictly into --out-dir.")
    p_emit.add_argument("--catalog", required=True, help="Catalog JSON file to emit.")
    p_emit.add_argument("--out-dir", required=True, help="Explicit output directory outside git and --game-dir.")
    p_emit.add_argument("--game-dir", default=None, help="Protected FNV install directory (read-only boundary check).")
    p_emit.add_argument("--saves-dir", default=None, help="Protected Saves directory (read-only boundary check).")
    p_emit.add_argument("--batch", action="append", default=None, help="Filter to specific batch ID(s).")
    p_emit.add_argument("--probe", action="append", default=None, help="Filter to specific probe ID(s).")
    p_emit.add_argument("--interval", type=int, default=10, help="Main-loop tick interval (default: 10).")
    p_emit.add_argument("--exclude-optional", action="store_true", help="Skip probes requiring optional extender DLLs.")
    p_emit.add_argument(
        "--exclude-staging-only",
        action="store_true",
        help="Skip probes that require optional read-only input fixtures.",
    )

    p_ver = sub.add_parser("verify", help="Verify captured PROBE| and P2| logs read-only against catalogs.")
    p_ver.add_argument("--catalog", default=None, help="Single catalog JSON file.")
    p_ver.add_argument("--catalog-dir", default=None, help="Directory of catalog JSON files.")
    p_ver.add_argument("--log-file", action="append", default=None, help="Captured log file (repeatable).")
    p_ver.add_argument("--log-dir", default=None, help="Directory of captured .log/.txt files (read-only).")
    p_ver.add_argument("--json-out", default=None, help="Write JSON scoreboard to this path (outside --game-dir).")
    p_ver.add_argument("--md-out", default=None, help="Write Markdown scoreboard to this path (outside --game-dir).")
    p_ver.add_argument("--game-dir", default=None, help="Protected FNV install directory (boundary check for outputs).")
    p_ver.add_argument("--saves-dir", default=None, help="Protected Saves directory (boundary check for outputs).")
    p_ver.add_argument(
        "--redact-save-names",
        action="store_true",
        help="Anonymize concrete .fos/.nvse save filenames to <save>.fos before privacy gate.",
    )
    p_ver.add_argument(
        "--strict",
        action="store_true",
        help="Exit non-zero if any mismatch, compile failure, or incomplete persistence item is present.",
    )

    p_cite = sub.add_parser("cite", help="Format clean-room [observed: ...] citations with privacy gate.")
    p_cite.add_argument("--scoreboard", default=None, help="Verified scoreboard JSON from `verify --json-out`.")
    p_cite.add_argument("--catalog", default=None, help="Catalog JSON file (when citing directly from logs).")
    p_cite.add_argument("--catalog-dir", default=None, help="Catalog directory (when citing directly from logs).")
    p_cite.add_argument("--log-file", action="append", default=None, help="Captured log file (repeatable).")
    p_cite.add_argument("--log-dir", default=None, help="Directory of captured logs (read-only).")
    p_cite.add_argument("--spec-file", default=None, help="Filter citations to one spec-note relative path.")
    p_cite.add_argument("--probe", action="append", default=None, help="Filter citations to specific probe/item ID(s).")
    p_cite.add_argument("--out", default=None, help="Write citations Markdown to this path (outside --game-dir).")
    p_cite.add_argument("--game-dir", default=None, help="Protected FNV install directory.")
    p_cite.add_argument("--saves-dir", default=None, help="Protected Saves directory.")
    p_cite.add_argument("--redact-save-names", action="store_true", help="Anonymize .fos/.nvse filenames to <save>.fos.")

    return ap


def main(argv: list[str] | None = None) -> int:
    ap = build_arg_parser()
    args = ap.parse_args(argv)

    if args.command == "list":
        paths = iter_catalog_paths(catalog=args.catalog, catalog_dir=args.catalog_dir)
        summary: list[dict[str, Any]] = []
        for p in paths:
            data = load_catalog(p)
            schema = data.get("schema")
            if schema == PROBE_CATALOG_SCHEMA:
                batches = [
                    {
                        "id": b["id"],
                        "title": b.get("title", ""),
                        "probe_count": len(b.get("probes", [])),
                        "required_plugins": b.get("required_plugins", []),
                    }
                    for b in data.get("batches", [])
                ]
                summary.append(
                    {
                        "file": p.name,
                        "catalog_id": data.get("catalog_id"),
                        "schema": schema,
                        "total_probes": sum(b["probe_count"] for b in batches),
                        "batches": batches,
                    }
                )
            else:
                summary.append(
                    {
                        "file": p.name,
                        "catalog_id": data.get("catalog_id"),
                        "schema": schema,
                        "total_items": len(data.get("items", [])),
                        "has_script_runner_lifecycle": bool(data.get("script_runner_lifecycle")),
                    }
                )
        if args.json:
            print(json.dumps(summary, indent=2))
        else:
            for item in summary:
                if "batches" in item:
                    print(f"{item['file']} ({item['catalog_id']}): {item['total_probes']} probes")
                    for b in item["batches"]:
                        print(f"  - {b['id']:18s} ({b['probe_count']:3d} probes) {b['title']}")
                else:
                    print(
                        f"{item['file']} ({item['catalog_id']}): {item['total_items']} persistence items"
                        + (" + script_runner_lifecycle" if item["has_script_runner_lifecycle"] else "")
                    )
        return 0

    if args.command == "validate":
        paths = iter_catalog_paths(catalog=args.catalog, catalog_dir=args.catalog_dir)
        repo_root = Path(args.repo_root) if args.repo_root else None
        errors, counts = validate_catalogs(paths, repo_root=repo_root)
        if errors:
            for err in errors:
                print(f"ERROR: {err}", file=sys.stderr)
            print(f"Validation FAILED with {len(errors)} error(s).", file=sys.stderr)
            return 1
        print(
            f"OK: validated {counts['catalogs']} catalog(s), "
            f"{counts['probe_batches']} batch(es), {counts['probes']} probe(s), "
            f"{counts['persistence_items']} persistence item(s)."
        )
        return 0

    if args.command == "emit":
        info = emit_catalog(
            args.catalog,
            args.out_dir,
            game_dir=args.game_dir,
            saves_dir=args.saves_dir,
            batches=args.batch,
            probe_ids=args.probe,
            interval=args.interval,
            exclude_optional=args.exclude_optional,
            exclude_staging_only=args.exclude_staging_only,
        )
        print(json.dumps(info, indent=2))
        return 0

    if args.command == "verify":
        paths = iter_catalog_paths(catalog=args.catalog, catalog_dir=args.catalog_dir)
        scoreboard = verify_logs(
            paths,
            log_files=args.log_file,
            log_dir=args.log_dir,
            redact_saves=args.redact_save_names,
        )
        md = render_markdown_scoreboard(scoreboard)
        if args.json_out:
            jpath = Path(args.json_out)
            assert_safe_out_dir(jpath.parent, game_dir=args.game_dir, saves_dir=args.saves_dir)
            jpath.parent.mkdir(parents=True, exist_ok=True)
            jpath.write_text(json.dumps(scoreboard, indent=2) + "\n", encoding="utf-8", newline="\n")
        if args.md_out:
            mpath = Path(args.md_out)
            assert_safe_out_dir(mpath.parent, game_dir=args.game_dir, saves_dir=args.saves_dir)
            mpath.parent.mkdir(parents=True, exist_ok=True)
            mpath.write_text(md + "\n", encoding="utf-8", newline="\n")
        else:
            print(md)
        if args.strict:
            c = scoreboard["counts"]
            if c["mismatch"] or c["compile_fail"] or c["persistence_mismatch"] or c["persistence_incomplete"]:
                return 2
        return 0

    if args.command == "cite":
        if args.scoreboard:
            scoreboard = json.loads(Path(args.scoreboard).read_text(encoding="utf-8"))
        else:
            paths = iter_catalog_paths(catalog=args.catalog, catalog_dir=args.catalog_dir)
            scoreboard = verify_logs(
                paths,
                log_files=args.log_file,
                log_dir=args.log_dir,
                redact_saves=args.redact_save_names,
            )
        probe_filter = set(args.probe) if args.probe else None
        out_lines: list[str] = []
        meta = scoreboard.get("metadata", {})
        for entry in scoreboard.get("probes", []):
            if entry["status"] in ("MISSING", "COMPILE_FAIL"):
                continue
            if args.spec_file and entry["spec_file"] != args.spec_file:
                continue
            if probe_filter and entry["id"] not in probe_filter:
                continue
            out_lines.append(
                f"- `{entry['spec_file']}:{entry['spec_line']}` (`{entry['id']}`): "
                + format_citation(entry, meta, is_persistence=False)
            )
        for entry in scoreboard.get("persistence", []):
            if entry["verdict"] == "INCOMPLETE":
                continue
            if args.spec_file and entry["spec_file"] != args.spec_file:
                continue
            if probe_filter and entry["id"] not in probe_filter:
                continue
            out_lines.append(
                f"- `{entry['spec_file']}:{entry['spec_line']}` (`{entry['id']}`): "
                + format_citation(entry, meta, is_persistence=True)
            )
        rendered = "\n".join(out_lines) + ("\n" if out_lines else "")
        if args.out:
            opath = Path(args.out)
            assert_safe_out_dir(opath.parent, game_dir=args.game_dir, saves_dir=args.saves_dir)
            opath.parent.mkdir(parents=True, exist_ok=True)
            opath.write_text(rendered, encoding="utf-8", newline="\n")
        else:
            sys.stdout.write(rendered)
        return 0

    return 2


if __name__ == "__main__":
    raise SystemExit(main())
