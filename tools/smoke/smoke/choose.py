"""Which scenarios and push checks a change calls for."""
from __future__ import annotations

import re
import subprocess
from functools import lru_cache
from pathlib import Path

from .scenarios import Check, Scenario


@lru_cache(maxsize=None)
def glob_regex(pattern: str) -> re.Pattern:
    """A glob over repo-relative paths with `/`: `**` any depth, `*` and `?` within a part.
    A pattern ending in `/` matches everything under that folder."""
    if pattern.endswith("/"):
        pattern += "**"
    out, i = [], 0
    while i < len(pattern):
        c = pattern[i]
        if pattern.startswith("**/", i):
            out.append("(?:.*/)?")
            i += 3
        elif pattern.startswith("**", i):
            out.append(".*")
            i += 2
        elif c == "*":
            out.append("[^/]*")
            i += 1
        elif c == "?":
            out.append("[^/]")
            i += 1
        else:
            out.append(re.escape(c))
            i += 1
    return re.compile("".join(out) + r"\Z")


def matches(path: str, globs: list[str]) -> bool:
    path = path.replace("\\", "/")
    return any(glob_regex(g).match(path) for g in globs)


def git(repo: Path, *args: str) -> str:
    r = subprocess.run(["git", "-C", str(repo), *args], capture_output=True, text=True)
    if r.returncode != 0:
        raise RuntimeError(f"git {' '.join(args)}: {r.stderr.strip()}")
    return r.stdout


def changed_files(repo: Path, base: str) -> list[str]:
    """Files that differ from where this branch left `base`: commits, staged and
    unstaged edits, and new untracked files."""
    mb = git(repo, "merge-base", base, "HEAD").strip()
    files = set(git(repo, "diff", "--name-only", mb).split())
    files |= set(git(repo, "ls-files", "--others", "--exclude-standard").split())
    return sorted(f.replace("\\", "/") for f in files if f)


def by_changes(scenarios: list[Scenario], files: list[str]) -> list[tuple[Scenario, list[str]]]:
    """Scenarios whose `paths` match a changed file, with the files that called them."""
    out = []
    for s in scenarios:
        hit = [f for f in files if matches(f, s.paths)]
        if hit:
            out.append((s, hit))
    return out


def by_area(scenarios: list[Scenario], areas: list[str]) -> list[Scenario]:
    want = {a.lower() for a in areas}
    return [s for s in scenarios if want & {a.lower() for a in s.areas}]


def checks_for(checks: list[Check], files: list[str]) -> list[Check]:
    return [c for c in checks if any(matches(f, c.paths) for f in files)]
