#!/usr/bin/env python3
"""Layer 0: file inventory of a game install (stdlib only).

Writes JSON: {"root": ..., "files": [{"path", "size", "sha256"}...]} sorted by path.
Paths are relative, forward-slash, original case. Output is PRIVATE ground truth
(it fingerprints copyrighted files) — keep it in the goldens directory, never commit.

usage: inventory.py <install_or_data_dir> [--no-hash] [-o out.json]
"""
import argparse
import hashlib
import json
import os
import sys


def sha256(path: str) -> str:
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def main() -> int:
    ap = argparse.ArgumentParser()
    ap.add_argument("root")
    ap.add_argument("--no-hash", action="store_true")
    ap.add_argument("-o", "--output")
    a = ap.parse_args()
    files = []
    for dp, _dn, fn in os.walk(a.root):
        for n in fn:
            p = os.path.join(dp, n)
            rel = os.path.relpath(p, a.root).replace(os.sep, "/")
            row = {"path": rel, "size": os.path.getsize(p)}
            if not a.no_hash:
                row["sha256"] = sha256(p)
            files.append(row)
    files.sort(key=lambda r: r["path"].lower())
    out = json.dumps({"schema": "fnv.inventory/1", "root": os.path.basename(os.path.abspath(a.root)),
                      "files": files}, indent=1)
    if a.output:
        with open(a.output, "w", encoding="utf-8") as f:
            f.write(out)
    else:
        sys.stdout.write(out)
    return 0


if __name__ == "__main__":
    sys.exit(main())
