#!/usr/bin/env python3
"""Collect extender probe output (tools/capture/ext-probes/) into goldens, and compare.

The probe scripts print `EXTPROBE|<id>|<value>` lines to the console of the original game
(tools/capture/ext-probes/*/README.md). This tool reads a game log or a saved console
dump, keeps those lines and writes one `fnv.ext.probes/1` document per probe set:

  ext_probes.py collect LOG [LOG ...] --set strings [--version 6.4.9] [-o OUT.json]
  ext_probes.py compare RUN.json tools/capture/ext-probes/strings/README.md [--json R]

- **collect** writes `<goldens>/ext/<set>.json` unless `-o` is given; `<goldens>` is
  `FNV_GOLDENS` when set, else `C:\\fnv-private\\goldens` on Windows and
  `~/fnv-private/goldens` elsewhere (the layout in tools/capture/README.md). The output is
  a golden: private, never committed.
- A probe line may sit anywhere in a log line (a console prefix is ignored); the value is
  everything after the second `|`, so it may contain more `|`. Begin and end markers
  (`EXTPROBE|<set>/<file>|begin` and `…|end`) are kept under `files`, so a script that
  stopped half way shows. An id printed twice keeps its last value, and both are listed
  under `duplicates`.
- **compare** checks a collected run against the README's probe table: the `Id` and
  `Expected` columns. Rows whose Expected starts with "to observe" or "observe" are
  listed with what the run printed, not judged. Numbers are compared by value
  (`5` equals `5.000000`); everything else as text. Exit 1 when a value differs or an id
  with an expected value is missing.

Standard library only; logs and READMEs are only read.
"""
import argparse
import json
import os
import re
import sys
from pathlib import Path

FORMAT = "fnv.ext.probes/1"
TAG = "EXTPROBE|"
MARKERS = ("begin", "end")


def default_goldens():
    env = os.environ.get("FNV_GOLDENS")
    if env:
        return Path(env)
    return Path("C:/fnv-private/goldens") if os.name == "nt" else Path.home() / "fnv-private" / "goldens"


def parse_lines(lines):
    """(rows, files, duplicates) from log lines: rows [{id, value}] in first-seen order."""
    values, order, dups, files = {}, [], [], {}
    for raw in lines:
        i = raw.find(TAG)
        if i < 0:
            continue
        rest = raw[i + len(TAG):].rstrip("\r\n")
        if "|" not in rest:
            continue
        pid, value = rest.split("|", 1)
        if "/" in pid and value in MARKERS:
            files.setdefault(pid, {"name": pid, "begun": False, "ended": False})
            files[pid]["begun" if value == "begin" else "ended"] = True
            continue
        if pid in values:
            if values[pid] != value:
                dups.append({"id": pid, "values": [values[pid], value]})
        else:
            order.append(pid)
        values[pid] = value
    rows = [{"id": p, "value": values[p]} for p in order]
    return rows, list(files.values()), dups


def collect(paths, probe_set, version=None):
    lines = []
    for p in paths:
        with open(p, encoding="utf-8", errors="replace") as f:
            lines.extend(f)
    rows, files, dups = parse_lines(lines)
    return {"format": FORMAT, "set": probe_set, "version": version,
            "sources": [os.path.basename(p) for p in paths],
            "rows": rows, "files": files, "duplicates": dups}


def _cells(line):
    """Cells of a Markdown table row, honouring `\\|` inside cells."""
    parts = re.split(r"(?<!\\)\|", line.strip().strip("|"))
    return [c.strip().replace("\\|", "|") for c in parts]


def _unquote(cell):
    cell = cell.strip()
    if len(cell) >= 2 and cell[0] == "`" and cell[-1] == "`":
        return cell[1:-1]
    return cell


def readme_expected(text):
    """{id: expected or None (observe)} from the first table with Id and Expected columns."""
    out, cols = {}, None
    for line in text.splitlines():
        if not line.lstrip().startswith("|"):
            cols = None if cols and not line.strip() else cols
            continue
        cells = _cells(line)
        if cols is None:
            if "Id" in cells and "Expected" in cells:
                cols = (cells.index("Id"), cells.index("Expected"))
            continue
        if set("".join(cells)) <= set("-: "):
            continue
        if len(cells) <= max(cols):
            continue
        pid, exp = _unquote(cells[cols[0]]), cells[cols[1]]
        low = exp.lower()
        out[pid] = None if low.startswith("to observe") or low.startswith("observe") else _unquote(exp)
    return out


def _num(s):
    try:
        return float(s)
    except (TypeError, ValueError):
        return None


def same(expected, got):
    a, b = _num(expected), _num(got)
    if a is not None and b is not None:
        return abs(a - b) <= 1e-4 * max(1.0, abs(a))
    return expected == got


def compare(run, expected):
    got = {r["id"]: r["value"] for r in run["rows"]}
    res = {"set": run.get("set"), "checked": 0, "matched": 0, "mismatches": [], "missing": [],
           "observe": [], "unknown": sorted(set(got) - set(expected))}
    for pid, exp in expected.items():
        if exp is None:
            res["observe"].append({"id": pid, "got": got.get(pid)})
            continue
        res["checked"] += 1
        if pid not in got:
            res["missing"].append(pid)
        elif same(exp, got[pid]):
            res["matched"] += 1
        else:
            res["mismatches"].append({"id": pid, "expected": exp, "got": got[pid]})
    res["files_unfinished"] = [f["name"] for f in run.get("files", []) if not f.get("ended")]
    return res


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    c = sub.add_parser("collect", help="probe lines of logs -> fnv.ext.probes/1")
    c.add_argument("logs", nargs="+")
    c.add_argument("--set", required=True, help="probe set name (strings, arrays, …)")
    c.add_argument("--version", help="the xNVSE version the run used")
    c.add_argument("-o", "--out", help="output file (default <goldens>/ext/<set>.json)")
    k = sub.add_parser("compare", help="a collected run against a README's expected values")
    k.add_argument("run")
    k.add_argument("readme")
    k.add_argument("--json", help="also write the report here")
    a = ap.parse_args(argv)
    if a.cmd == "collect":
        doc = collect(a.logs, a.set, a.version)
        out = Path(a.out) if a.out else default_goldens() / "ext" / f"{a.set}.json"
        out.parent.mkdir(parents=True, exist_ok=True)
        out.write_text(json.dumps(doc, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
        print(json.dumps({"out": str(out), "rows": len(doc["rows"]), "files": len(doc["files"]),
                          "duplicates": len(doc["duplicates"])}))
        return 0
    with open(a.run, encoding="utf-8") as f:
        run = json.load(f)
    with open(a.readme, encoding="utf-8") as f:
        expected = readme_expected(f.read())
    res = compare(run, expected)
    text = json.dumps(res, indent=1, ensure_ascii=False)
    if a.json:
        Path(a.json).write_text(text + "\n", encoding="utf-8")
    print(text)
    return 1 if res["mismatches"] or res["missing"] else 0


if __name__ == "__main__":
    sys.exit(main())
