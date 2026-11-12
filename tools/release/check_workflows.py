"""Check the GitHub workflows' token permissions and action pins (#1999).

Rules, for every file in .github/workflows:

- the workflow's own `permissions:` (before `jobs:`) gives `contents: read` and writes
  nothing, so a job holds a token that can write only where it says so itself;
- a job's `contents: write` is allowed only in a job named in WRITERS (release.yml's
  publish job, which creates the release);
- every action outside GitHub's own `actions/` owner is pinned to a full 40-character
  commit SHA, with its tag or branch in a comment after it (`@<sha> # v2`), which
  Dependabot reads and updates with the SHA.

Standard library only (no YAML parser): the workflows are read line by line by their
indentation, which is all these rules need. Run from the repository root:

    python tools/release/check_workflows.py

It prints one line per problem and exits 1 if there is any.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

# (workflow file name, job) pairs that may hold `contents: write`.
WRITERS = {("release.yml", "publish")}
# Owners whose actions may stay on a tag (GitHub's own).
FIRST_PARTY = {"actions"}

USES = re.compile(r"^\s*(?:-\s+)?uses:\s*([^\s#]+)\s*(?:#\s*(\S.*))?$")
SHA = re.compile(r"^[0-9a-f]{40}$")
KEY = re.compile(r"^( *)([A-Za-z0-9_-]+):\s*(.*?)\s*$")


def problems_in(name: str, text: str) -> list[str]:
    """The rule breaks in workflow `name` (its file name) with contents `text`."""
    out: list[str] = []
    top_perms: dict[str, str] | None = None
    section: str | None = None  # the top-level key the line is under
    job: str | None = None
    perms_at: int | None = None  # indentation of the `permissions:` block being read
    perms: dict[str, str] = {}
    job_perms: dict[str, dict[str, str]] = {}

    def close_perms() -> None:
        nonlocal perms_at, perms, top_perms
        if perms_at is None:
            return
        if perms_at == 0:
            top_perms = perms
        elif job is not None:
            job_perms[job] = perms
        perms_at, perms = None, {}

    for n, raw in enumerate(text.splitlines(), 1):
        line = raw.split(" #")[0] if "uses:" not in raw else raw
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        m = USES.match(raw)
        if m:
            ref, comment = m.group(1), m.group(2)
            if not ref.startswith(("./", "docker://")) and "@" in ref:
                action, _, pin = ref.partition("@")
                owner = action.split("/")[0]
                if owner not in FIRST_PARTY:
                    if not SHA.match(pin):
                        out.append(f"{name}:{n}: {action} is pinned to {pin!r}, not a commit SHA")
                    elif not comment:
                        out.append(f"{name}:{n}: {action}@{pin[:7]}… has no '# <tag>' comment")
            continue
        k = KEY.match(line)
        if not k:
            continue
        indent, key, value = len(k.group(1)), k.group(2), k.group(3)
        if perms_at is not None and indent <= perms_at:
            close_perms()
        if indent == 0:
            section, job = key, None
        elif section == "jobs" and indent == 2:
            job = key
        if key == "permissions" and (indent == 0 or (section == "jobs" and indent == 4)):
            if value:  # `permissions: read-all` / `write-all` / `{}`
                perms_at, perms = indent, {"*": value}
                close_perms()
            else:
                perms_at, perms = indent, {}
            continue
        if perms_at is not None and indent > perms_at:
            perms[key] = value
    close_perms()

    if top_perms is None:
        out.append(f"{name}: no workflow-level permissions (the token would get the repository's default)")
    else:
        writes = sorted(k for k, v in top_perms.items() if "write" in v)
        if writes:
            out.append(f"{name}: the workflow's permissions write {', '.join(writes)}; give write to the job that needs it")
        if top_perms.get("contents") != "read":
            out.append(f"{name}: the workflow's permissions do not give contents: read")
    for j, p in sorted(job_perms.items()):
        for k, v in sorted(p.items()):
            if "write" in v and not (k == "contents" and (name, j) in WRITERS):
                out.append(f"{name}: job {j} writes {k}, and is not listed in WRITERS")
    return out


def main(root: Path) -> int:
    files = sorted((root / ".github" / "workflows").glob("*.yml"))
    if not files:
        print(f"no workflows under {root / '.github' / 'workflows'}")
        return 1
    found = [p for f in files for p in problems_in(f.name, f.read_text(encoding="utf-8"))]
    for p in found:
        print(p)
    print(f"workflows: {len(files)} checked, {len(found)} problem(s)")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(Path(sys.argv[1]) if len(sys.argv) > 1 else Path.cwd()))
