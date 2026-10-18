"""git merge driver for tools/smoke/scenarios.toml: git's own line merge, then a check.

The merge itself is `git merge-file` (the line merge git would do anyway); a conflict
stays a conflict. A merge without conflicts is then checked (smoke/integrity.py): it must
parse, keep the rules a healthy file keeps, and every scenario must equal the side that
changed it. If not, the driver says why and reports a conflict, so the damage git's line
merge does to this file (a lost `]` and `[[scenario]]`, a spliced or doubled block) stops
the merge instead of being committed.

.gitattributes names the driver; each clone turns it on once (worktrees share it):

    git config merge.smoke-scenarios.name "scenarios.toml: line merge, then checked"
    git config merge.smoke-scenarios.driver "python tools/smoke/merge_driver.py %O %A %B %S %X %Y"

Without that configuration git merges the file as before.
"""
from __future__ import annotations

import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

from smoke import integrity  # noqa: E402


def main(base: str, ours: str, theirs: str, *labels: str) -> int:
    texts = [Path(p).read_text(encoding="utf-8") for p in (base, ours, theirs)]
    lb, lo, lt = (list(labels) + ["base", "ours", "theirs"][len(labels):])[:3]
    r = subprocess.run(["git", "merge-file", "-L", lo, "-L", lb, "-L", lt, ours, base, theirs])
    if r.returncode != 0:
        return 1  # conflicts (or an error): git reports them as usual
    result = Path(ours).read_text(encoding="utf-8")
    found = integrity.merge_problems(texts[0], texts[1], texts[2], result)
    if not found:
        try:
            found = integrity.problems(result, HERE)
        except ValueError as e:  # ScenarioError, tomllib.TOMLDecodeError
            found = [str(e)]
    for f in found:
        print(f"tools/smoke/scenarios.toml: the line merge damaged it: {f}", file=sys.stderr)
    if found:
        print("tools/smoke/scenarios.toml: left as the line merge wrote it, marked conflicted; "
              "mend it by hand (each scenario as the side that changed it), then "
              "`python tools/smoke verify` after committing", file=sys.stderr)
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(*sys.argv[1:7]))
