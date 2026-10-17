#!/usr/bin/env python3
"""The script-function coverage tables of docs/design/script-function-coverage.md.

  function_coverage.py --coverage cov.json --uses uses.json [--probes compare.json]
                       [--systems function_systems.toml] [--audit script-function-audit.md]
                       [--doc docs/design/script-function-coverage.md]

Inputs: `fnvtool script coverage` (every function the ten masters' scripts call or their
conditions use, with the catalogue's call and condition counts), `fnvtool script uses`
(per quest, the functions its scripts and conditions use), the per-function status of
docs/design/script-function-audit.md §6, the curated `function_systems.toml` (the
functions that do not match: their status and the system they need) and, optionally,
the probes' comparison rows (`function_probe.py compare --json`).

Every function gets one status: `match` (behaves as the original, as far as documented
or observed), `wrong` (wrong or a stub while the system behind it exists), `no-system`
(the system behind it is not built; it is named) or `open` (not settled; a capture
would). A function the audit does not call "matches" / "fixed here" must be in the
systems file, so nothing is classified by default.

The doc's text between `<!-- generated:begin -->` and `<!-- generated:end -->` is
replaced; the rest is written by hand. Without --doc the tables go to stdout.
Standard library only.
"""
import argparse
import json
import os
import re
import sys
import tomllib

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.dirname(os.path.dirname(HERE))
SYSTEMS = os.path.join(HERE, "function_systems.toml")
AUDIT = os.path.join(REPO, "docs", "design", "script-function-audit.md")
BEGIN, END = "<!-- generated:begin -->", "<!-- generated:end -->"
STATUSES = ("match", "wrong", "no-system", "open")
MATCHING_AUDIT = ("matches", "fixed here", "implemented")


class CoverageError(Exception):
    pass


def audit_statuses(path):
    """{function name (lower case): (audit status, note)} from the audit's §6 table; a row
    naming several functions ("A / B") gives each the row's status."""
    out = {}
    with open(path, encoding="utf-8") as f:
        for line in f:
            if not line.startswith("| ") or line.startswith(("| Function", "|---")):
                continue
            cells = [c.strip() for c in line.strip().strip("|").split(" | ")]
            if len(cells) < 6 or not re.search(r"0x[0-9A-Fa-f]{4}|console", cells[1]):
                continue
            status = cells[5]
            note = cells[6] if len(cells) > 6 else ""
            for name in re.split(r"\s*/\s*", cells[0]):
                name = name.replace("(console)", "").strip()
                if name and not name.startswith("..."):
                    out[name.lower()] = (status, note)
    return out


def load_systems(path):
    with open(path, "rb") as f:
        doc = tomllib.load(f)
    fns = {k.lower(): dict(v, name=k) for k, v in doc.get("function", {}).items()}
    for k, v in fns.items():
        if v.get("status") not in STATUSES:
            raise CoverageError(f"{path}: {v['name']}: status must be one of {', '.join(STATUSES)}")
        if v["status"] in ("wrong", "no-system") and not v.get("system"):
            raise CoverageError(f"{path}: {v['name']}: a {v['status']} function names its system")
    return doc, fns


def probe_counts(rows):
    """{function (lower case): {"same": n, "differs": n, ...}} from the probes' rows."""
    out = {}
    for r in rows or []:
        fn = (r.get("fn") or "").lower()
        if fn:
            d = out.setdefault(fn, {})
            d[r["verdict"]] = d.get(r["verdict"], 0) + 1
    return out


def classify(cov, audit, systems):
    """Rows: name, opcode, calls, conditions, status, system, note, audit."""
    rows = []
    missing = []
    for o in cov["opcodes"]:
        name = o.get("name")
        if not name:
            continue
        key = name.lower()
        a = audit.get(key)
        s = systems.get(key)
        calls = o.get("script_calls") or 0
        conds = o.get("condition_uses") or 0
        row = {"name": name, "opcode": o["opcode"], "calls": calls, "conditions": conds, "decoded_calls": o.get("calls", 0),
               "audit": a[0] if a else "", "system": "", "note": ""}
        if s:
            row.update(status=s["status"], system=s.get("system", ""), note=s.get("note", ""))
        elif a and a[0].split()[0] in [m.split()[0] for m in MATCHING_AUDIT]:
            row.update(status="match", note="")
        elif o.get("stub"):
            missing.append(name)
            continue
        elif a is None and o.get("implemented"):
            row.update(status="match", note="not in the audit's table")
        else:
            missing.append(name)
            continue
        rows.append(row)
    if missing:
        raise CoverageError("not classified (add them to function_systems.toml): " + ", ".join(sorted(missing)))
    return rows


def blockers(rows, uses, groups):
    """Missing systems ranked by the quests that depend on them: [{system, functions,
    calls, conditions, quests, main, companions}]."""
    by_system = {}
    for r in rows:
        if r["status"] in ("no-system", "wrong") and r["system"]:
            by_system.setdefault(r["system"], []).append(r)
    main = [re.compile(p) for p in groups.get("main", [])]
    companions = set(groups.get("companions", []))
    out = []
    for system, fns in by_system.items():
        names = {f["name"] for f in fns}
        quests = []
        for q in uses["quests"]:
            c = sum(v for k, v in q.get("calls", {}).items() if k in names)
            k = sum(v for k, v in q.get("condition_uses", {}).items() if k in names)
            if c or k:
                quests.append((q, c, k))
        label = lambda q: f"{q['edid']} ({q['name']})" if q.get("name") else q["edid"] or q["form"]
        out.append({
            "system": system,
            "status": "no-system" if any(f["status"] == "no-system" for f in fns) else "wrong",
            "functions": sorted(names),
            "calls": sum(f["calls"] for f in fns),
            "conditions": sum(f["conditions"] for f in fns),
            "quests": len(quests),
            "quest_calls": sum(c for _, c, _ in quests),
            "quest_conditions": sum(k for _, _, k in quests),
            "main": sorted(label(q) for q, _, _ in quests if q.get("edid") and any(p.match(q["edid"]) for p in main)),
            "companions": sorted(label(q) for q, _, _ in quests if q.get("edid") in companions),
        })
    out.sort(key=lambda b: (-b["quests"], -(b["quest_calls"] + b["quest_conditions"]), -(b["calls"] + b["conditions"]), b["system"]))
    return out


def cell(v):
    return str(v).replace("|", "\\|").replace("\n", " ")


def tables(rows, ranked, probes):
    out = []
    counts = {s: [0, 0, 0] for s in STATUSES}
    for r in rows:
        c = counts[r["status"]]
        c[0] += 1
        c[1] += r["calls"]
        c[2] += r["conditions"]
    total = [sum(c[i] for c in counts.values()) for i in range(3)]
    out += ["### Totals", "",
            "| Status | Functions | Calls | Condition uses |", "|---|---|---|---|"]
    names = {"match": "matches the original", "wrong": "wrong or stub, system exists", "no-system": "no system yet", "open": "open (capture needed)"}
    for s in STATUSES:
        out.append(f"| {names[s]} | {counts[s][0]} | {counts[s][1]} | {counts[s][2]} |")
    out.append(f"| all used | {total[0]} | {total[1]} | {total[2]} |")
    out += ["", "### Playthrough blockers", "",
            "Missing or wrong systems ranked by the quests whose scripts or conditions use them "
            "(fnvtool script uses: a quest's scripts, stages, objectives, dialogue lines and one "
            "level of what they name). Calls and condition uses are the catalogue's, over the ten masters.", "",
            "| # | System | Kind | Quests | Calls / cond. in them | All calls / cond. | Functions | Main quest | Companions |",
            "|---|---|---|---|---|---|---|---|---|"]
    for i, b in enumerate(ranked, 1):
        main = ", ".join(b["main"][:6]) + (f" and {len(b['main']) - 6} more" if len(b["main"]) > 6 else "")
        comp = ", ".join(b["companions"][:4]) + (f" and {len(b['companions']) - 4} more" if len(b["companions"]) > 4 else "")
        out.append(f"| {i} | {cell(b['system'])} | {b['status']} | {b['quests']} | {b['quest_calls']} / {b['quest_conditions']} | "
                   f"{b['calls']} / {b['conditions']} | {', '.join(b['functions'])} | {cell(main) or '-'} | {cell(comp) or '-'} |")
    out += ["", "### Every function", "",
            "Calls / cond.: the catalogue's script calls and condition uses over the ten masters. "
            "Probes: the 2026-10-17 probes' verdicts (same / differs) where there were any.", "",
            "| Function | Opcode | Calls / cond. | Status | System it needs | Probes | Note |", "|---|---|---|---|---|---|---|"]
    for r in sorted(rows, key=lambda r: r["name"].lower()):
        p = probes.get(r["name"].lower(), {})
        ptext = ", ".join(f"{k} {v}" for k, v in sorted(p.items()) if k in ("same", "differs", "game error", "ours error"))
        out.append(f"| {r['name']} | {r['opcode']} | {r['calls']} / {r['conditions']} | {r['status']} | {cell(r['system']) or '-'} | "
                   f"{ptext or '-'} | {cell(r['note'])} |")
    return "\n".join(out) + "\n"


def write_doc(path, text):
    with open(path, encoding="utf-8") as f:
        doc = f.read()
    if BEGIN not in doc or END not in doc:
        raise CoverageError(f"{path}: no {BEGIN} … {END} region")
    a, b = doc.index(BEGIN) + len(BEGIN), doc.index(END)
    doc = doc[:a] + "\n" + text + doc[b:]
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write(doc)


def main(argv=None):
    ap = argparse.ArgumentParser(prog="function_coverage.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--coverage", required=True, help="fnvtool script coverage output (fnv.script.coverage/1)")
    ap.add_argument("--uses", required=True, help="fnvtool script uses output (fnv.script.uses/1)")
    ap.add_argument("--probes", help="function_probe.py compare --json rows")
    ap.add_argument("--systems", default=SYSTEMS)
    ap.add_argument("--audit", default=AUDIT)
    ap.add_argument("--doc", help="the doc whose generated region is replaced (default: stdout)")
    args = ap.parse_args(argv)
    try:
        with open(args.coverage, encoding="utf-8") as f:
            cov = json.load(f)
        with open(args.uses, encoding="utf-8") as f:
            uses = json.load(f)
        probes = []
        if args.probes:
            with open(args.probes, encoding="utf-8") as f:
                probes = json.load(f)
        doc, systems = load_systems(args.systems)
        rows = classify(cov, audit_statuses(args.audit), systems)
        text = tables(rows, blockers(rows, uses, doc.get("groups", {})), probe_counts(probes))
        if args.doc:
            write_doc(args.doc, text)
        else:
            sys.stdout.write(text)
    except (CoverageError, OSError, json.JSONDecodeError, tomllib.TOMLDecodeError) as e:
        print(f"function_coverage: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
