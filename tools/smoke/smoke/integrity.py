"""Checks that a text merge of scenarios.toml did not quietly damage it.

git's line merge of the file has dropped the `]` and `[[scenario]]` between two scenarios,
spliced one scenario's expectations into another's, and doubled blocks (2026-10-16 to
-18). A damaged file that still parses is caught here:

- `problems(text)`: rules every healthy file keeps. A tool scenario expects JSON (fnvtool
  prints JSON), a game scenario expects log lines; no run of three expectation lines
  appears twice (a splice copies lines); every `[[scenario]]` header starts a table with
  its own `name`.
- `merge_problems(base, ours, theirs, result)`: scenario by scenario, the merge result
  must equal the side that changed it (or both sides, when both changed it the same way);
  none may be lost unless one side removed it. A scenario both sides changed differently
  is left to the person who merged.

`python tools/smoke verify [REV]` runs both (the second when REV, default HEAD, is a merge).
"""
from __future__ import annotations

import re
import subprocess
import tomllib
from pathlib import Path

from .scenarios import GAME, TOOL, load_text

# Expectation lines in a row that must not appear twice: two tables (`{ re = …, label = … }`
# lines are long and particular), four of any kind (short text lines repeat in sequences).
RUNS = ((2, re.compile(r"^\s*\{")), (4, re.compile(r"""^\s*[{"']""")))
_TABLE = re.compile(r"^\[\[?[A-Za-z]")  # a table header


def problems(text: str, base: Path | None = None) -> list[str]:
    """What is wrong with a scenario file that parses (a parse error raises)."""
    suite = load_text(text, base)
    out = []
    for s in suite.scenarios:
        json_kinds = {e.json_path is not None for e in s.all_expectations() if not (s.kind == GAME and e.label == s.ends)}
        if s.kind == TOOL and False in json_kinds:
            out.append(f"{s.name}: a tool scenario holds log-line expectations (spliced in from a game scenario?)")
        if s.kind == GAME and True in json_kinds:
            out.append(f"{s.name}: a game scenario holds json expectations (spliced in from a tool scenario?)")
    lines = text.splitlines()
    for n, entry in RUNS:
        seen: dict[tuple[str, ...], int] = {}
        for i in range(len(lines) - n + 1):
            run = tuple(x.strip() for x in lines[i:i + n])
            if not all(entry.match(x) for x in lines[i:i + n]):
                continue
            if run in seen:
                out.append(f"lines {seen[run]}-{seen[run] + n - 1} appear again at {i + 1}-{i + n} "
                           f"(a block doubled by a merge?): {run[0][:80]}")
            else:
                seen[run] = i + 1
    tables = [i for i, x in enumerate(lines) if _TABLE.match(x)]
    headers = [i for i in tables if lines[i].strip() == "[[scenario]]"]
    if len(headers) != len(suite.scenarios):
        out.append(f"{len(headers)} [[scenario]] headers for {len(suite.scenarios)} scenarios")
    for h in headers:
        end = next((i for i in tables if i > h), len(lines))
        names = [x for x in lines[h + 1:end] if x.startswith("name = ")]
        if len(names) != 1:
            out.append(f"the [[scenario]] at line {h + 1} has {len(names)} `name =` lines before the next header")
    return out


def _by_name(text: str | None) -> dict[str, dict]:
    if text is None:
        return {}
    return {d.get("name"): d for d in tomllib.loads(text).get("scenario", [])}


def merge_problems(base: str | None, ours: str, theirs: str, result: str) -> list[str]:
    """Scenarios the merge result got wrong, judged against both parents and their base."""
    try:
        r = _by_name(result)
    except tomllib.TOMLDecodeError as e:
        return [f"the merge result does not parse: {e}"]
    try:
        b, o, t = (_by_name(x) for x in (base, ours, theirs))
    except tomllib.TOMLDecodeError as e:
        return [f"a parent of the merge does not parse ({e}): nothing to compare with"]
    out = []
    for name in sorted(set(o) | set(t) | set(r), key=str):
        bo, oo, to, ro = b.get(name), o.get(name), t.get(name), r.get(name)
        if oo == to:
            want = [oo]
        elif oo == bo:
            want = [to]
        elif to == bo:
            want = [oo]
        else:
            if ro is None and oo is not None and to is not None:
                out.append(f"{name}: changed on both sides and lost in the merge")
            continue  # changed differently on both sides: the merger decided
        if ro not in want:
            if ro is None:
                out.append(f"{name}: lost in the merge")
            elif want == [None]:
                out.append(f"{name}: in the merge result but removed (or never there) on both sides")
            else:
                keys = sorted(set(ro) ^ set(want[0]) | {k for k in set(ro) & set(want[0]) if ro[k] != want[0][k]})
                out.append(f"{name}: differs from the side that changed it ({', '.join(keys)})")
    return out


def _show(repo: Path, rev: str, path: str) -> str | None:
    r = subprocess.run(["git", "-C", str(repo), "show", f"{rev}:{path}"], capture_output=True, text=True, encoding="utf-8")
    return r.stdout if r.returncode == 0 else None


def _git(repo: Path, *args) -> str | None:
    r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
    return r.stdout.strip() if r.returncode == 0 else None


def merge_check(repo: Path, rev: str, path: str) -> list[str] | None:
    """`merge_problems` for the merge commit `rev`; None when it is no merge or its parents
    are not in this clone (a shallow CI checkout)."""
    parents = (_git(repo, "rev-list", "--parents", "-n", "1", rev) or "").split()[1:]
    if len(parents) != 2:
        return None
    ours, theirs, result = (_show(repo, p, path) for p in (*parents, rev))
    if ours is None or theirs is None or result is None:
        return None
    base = _git(repo, "merge-base", *parents)
    return merge_problems(_show(repo, base, path) if base else None, ours, theirs, result)
