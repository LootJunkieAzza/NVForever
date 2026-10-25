#!/usr/bin/env python3
"""Compare AI observer timelines of the same campaign from two set-ups: does the change
(a plugin installed, e.g. JIP LN NVSE) move what NPCs do by more than runs of the same
set-up differ from each other (docs/spec-notes/ai-observer-jip.md section 4).

  observe_compare.py --base A1.timeline.json A2.timeline.json --with B1.timeline.json [...]
                     [--frac-tol F] [--rel-tol R] [-o compare.json] [--text]

For every session and actor seen in all runs: the share of observed game time on each
package and each procedure, the number of segments (spans of unchanged AI state), the
distance walked and the cells visited. A `--with` value is "outside" when it falls
beyond the range of the `--base` values by more than a tolerance: `--frac-tol` for the
shares (default 0.05 of the session), `--rel-tol` for segments and distance (default
0.25 of the base range's top). Actors seen in some runs only are listed apart. With one
base run the range is that run's value, which says nothing about run-to-run noise: give
two or more.

Standard library only. Exit codes: 0 compared, 1 bad input.
"""
import argparse
import json
import sys

COMPARE_FORMAT = "fnv.observe.compare/1"
# Report thresholds, not parity tolerances: they only sort what to look at first
# [design]. A share is of the actor's observed hours in that session.
FRAC_TOL = 0.05
REL_TOL = 0.25


def run_metrics(tl):
    """{(session, actor label): {metric: value}} of one fnv.observe.timeline/1."""
    out = {}
    for aid, a in tl.get("actors", {}).items():
        label = a.get("edid") or aid
        by_session = {}
        for seg in a.get("segments", []):
            by_session.setdefault(seg["session"], []).append(seg)
        for session, segs in by_session.items():
            hours = sum(s["to"] - s["from"] for s in segs)
            m = {"segments": float(len(segs)), "path": sum(s.get("path") or 0.0 for s in segs)}
            cells = set()
            for s in segs:
                st = s["state"]
                span = (s["to"] - s["from"]) / hours if hours > 0 else 0.0
                pkg = st.get("package_edid") or st.get("package") or "none"
                if isinstance(pkg, str) and pkg.upper().startswith("0XFF"):
                    # a package made at run time: its FormID depends on how many forms were
                    # made before it (Sunny's dialogue package in the saloon was FF001313
                    # without plugins and FF001323 with JIP and JohnnyGuitar, same type,
                    # procedure and target [observed 2026-10-25]), so it is compared by type
                    pkg = f"runtime {st.get('package_type_name', st.get('package_type'))}"
                proc = st.get("procedure_name", st.get("procedure"))
                m[f"package:{pkg}"] = m.get(f"package:{pkg}", 0.0) + span
                m[f"procedure:{proc if proc is not None else 'none'}"] = \
                    m.get(f"procedure:{proc if proc is not None else 'none'}", 0.0) + span
                cell = st.get("cell_edid") or st.get("cell")
                if cell:
                    cells.add(str(cell))
            m["_cells"] = sorted(cells)
            m["_hours"] = hours
            out[(session, label)] = m
    return out


def compare(base, other, frac_tol=FRAC_TOL, rel_tol=REL_TOL):
    """base, other: lists of run_metrics(). Returns the fnv.observe.compare/1 document."""
    keys_all = [set(r) for r in base + other]
    common = set.intersection(*keys_all) if keys_all else set()
    partial = sorted(set.union(*keys_all) - common) if keys_all else []
    rows, cells = [], []
    for key in sorted(common):
        runs_b = [r[key] for r in base]
        runs_o = [r[key] for r in other]
        metrics = sorted({k for r in runs_b + runs_o for k in r if not k.startswith("_")})
        for metric in metrics:
            vb = [r.get(metric, 0.0) for r in runs_b]
            vo = [r.get(metric, 0.0) for r in runs_o]
            lo, hi = min(vb), max(vb)
            share = metric.startswith(("package:", "procedure:"))
            tol = frac_tol if share else rel_tol * max(abs(hi), 1.0)
            beyond = max(max(0.0, v - hi, lo - v) for v in vo)
            rows.append({"session": key[0], "actor": key[1], "metric": metric, "base": vb, "with": vo,
                         "base_spread": hi - lo, "beyond": beyond, "tol": tol, "outside": beyond > tol})
        cb = [set(r["_cells"]) for r in runs_b]
        co = [set(r["_cells"]) for r in runs_o]
        known = set.union(*cb)
        new = sorted(set.union(*co) - known)
        if new:
            cells.append({"session": key[0], "actor": key[1], "cells_only_with": new})
    outside = [r for r in rows if r["outside"]]
    outside.sort(key=lambda r: -(r["beyond"] / r["tol"] if r["tol"] else r["beyond"]))
    spread = [r for r in rows if r["metric"].startswith(("package:", "procedure:"))]
    return {"format": COMPARE_FORMAT, "base_runs": len(base), "with_runs": len(other),
            "frac_tol": frac_tol, "rel_tol": rel_tol, "compared": len(common), "metrics": len(rows),
            "outside": outside, "cells": cells, "partial": [list(k) for k in partial],
            "max_base_share_spread": max((r["base_spread"] for r in spread), default=0.0),
            "rows": rows}


def text_report(doc):
    out = [f"{doc['compared']} (session, actor) pairs in every run, {doc['metrics']} metrics; "
           f"base runs {doc['base_runs']}, with runs {doc['with_runs']}",
           f"largest spread of a share between base runs: {doc['max_base_share_spread']:.3f}",
           f"outside the base range by more than the tolerance: {len(doc['outside'])}"]
    for r in doc["outside"]:
        out.append(f"  {r['session']:<18} {r['actor']:<26} {r['metric']:<40} base "
                   f"{', '.join(f'{v:.3f}' for v in r['base'])}  with {', '.join(f'{v:.3f}' for v in r['with'])}")
    for c in doc["cells"]:
        out.append(f"  cells only in the with runs: {c['session']} {c['actor']} {c['cells_only_with']}")
    if doc["partial"]:
        out.append(f"seen in some runs only ({len(doc['partial'])}): "
                   + ", ".join(f"{s}/{a}" for s, a in doc["partial"][:40])
                   + (" ..." if len(doc["partial"]) > 40 else ""))
    return "\n".join(out) + "\n"


def main(argv=None):
    ap = argparse.ArgumentParser(prog="observe_compare.py", description=__doc__.split("\n\n")[0])
    ap.add_argument("--base", nargs="+", required=True, metavar="TIMELINE", help="runs of the reference set-up")
    ap.add_argument("--with", dest="other", nargs="+", required=True, metavar="TIMELINE", help="runs of the changed set-up")
    ap.add_argument("--frac-tol", type=float, default=FRAC_TOL)
    ap.add_argument("--rel-tol", type=float, default=REL_TOL)
    ap.add_argument("--text", action="store_true", help="print a readable summary (also with -o)")
    ap.add_argument("-o", "--output", help="write the comparison as JSON here")
    args = ap.parse_args(argv)
    try:
        runs = []
        for paths in (args.base, args.other):
            group = []
            for p in paths:
                with open(p, encoding="utf-8") as f:
                    tl = json.load(f)
                if tl.get("format") != "fnv.observe.timeline/1":
                    raise ValueError(f"{p}: not a fnv.observe.timeline/1 document")
                group.append(run_metrics(tl))
            runs.append(group)
    except (OSError, ValueError) as e:
        print(f"observe_compare: {e}", file=sys.stderr)
        return 1
    doc = compare(runs[0], runs[1], args.frac_tol, args.rel_tol)
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            json.dump(doc, f, indent=1)
            f.write("\n")
    if args.text or not args.output:
        sys.stdout.write(text_report(doc))
    return 0


if __name__ == "__main__":
    sys.exit(main())
