"""fnvlab command line: list, run, baseline.

Exit codes:
  0  run completed, no regressions (and, with --strict, every scenario passed)
  1  regression(s) against the saved baseline
  2  --strict and at least one scenario is not passing
  3  configuration error (bad config / scenario file)
  4  --require-baseline given but no baseline exists (CI must not pass silently)
"""
from __future__ import annotations

import argparse
import datetime as _dt
import json
import os
import sys

from .config import ConfigError, LabConfig, filter_scenarios, load_scenarios, unset_vars
from .report import build_report, render_markdown, write_reports
from .runner import find_regressions, load_baseline, make_baseline, run_all

DEFAULT_CONFIG = os.path.join(os.path.dirname(os.path.dirname(os.path.abspath(__file__))), "lab.json")


def _confirm_reveal() -> bool:
    """Holdout reveal requires a human at an interactive terminal."""
    if not (sys.stdin.isatty() and sys.stderr.isatty()):
        print("--reveal requires an interactive terminal; refusing.", file=sys.stderr)
        return False
    print("Holdout details will be shown. Agents must never do this.\n"
          "Type 'reveal holdouts' to continue: ", end="", file=sys.stderr, flush=True)
    return sys.stdin.readline().strip() == "reveal holdouts"


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(prog="fnvlab", description="FNV parity lab (oracle vs candidate)")
    ap.add_argument("--config", default=os.environ.get("FNVLAB_CONFIG", DEFAULT_CONFIG))
    sub = ap.add_subparsers(dest="cmd", required=True)

    def add_filters(p):
        p.add_argument("--layer", type=int, action="append", help="repeatable")
        p.add_argument("--scenario", action="append", help="id or glob; repeatable")
        p.add_argument("--tag", action="append", help="repeatable; any match")

    pl = sub.add_parser("list", help="list scenarios")
    add_filters(pl)
    pl.add_argument("--json", action="store_true")

    pr = sub.add_parser("run", help="run scenarios and write runs/latest.{json,md}")
    add_filters(pr)
    pr.add_argument("--json", action="store_true", help="print the JSON report to stdout")
    pr.add_argument("--reveal", action="store_true", help="HUMAN ONLY: show holdout details")
    pr.add_argument("--strict", action="store_true", help="non-zero exit unless everything passes")
    pr.add_argument("--require-baseline", action="store_true",
                    help="exit 4 if no baseline exists (use in CI so regressions cannot go unnoticed)")

    pb = sub.add_parser("baseline", help="HUMAN ONLY: save runs/latest.json as the baseline")
    pb.add_argument("--from", dest="src", help="run JSON to use (default runs/latest.json)")

    a = ap.parse_args(argv)
    try:
        cfg = LabConfig.load(a.config)
        if a.cmd in ("list", "run"):
            scen = filter_scenarios(load_scenarios(cfg), a.layer, a.scenario, a.tag)
    except ConfigError as e:
        print(f"config error: {e}", file=sys.stderr)
        return 3

    if a.cmd == "list":
        if a.json:
            print(json.dumps([{"id": s.id, "layer": s.layer, "tags": [] if s.holdout else s.tags,
                               "holdout": s.holdout,
                               "comparator": s.comparator["kind"],
                               "description": "" if s.holdout else s.description} for s in scen], indent=2))
        else:
            for s in scen:
                flag = " [holdout]" if s.holdout else ""
                desc = "" if s.holdout else f" — {s.description}"
                print(f"L{s.layer}  {s.id}  ({s.comparator['kind']}){flag}{desc}")
            print(f"{len(scen)} scenario(s)")
        return 0

    if a.cmd == "baseline":
        src = a.src or str(cfg.runs_dir / "latest.json")
        try:
            latest = json.loads(open(src, encoding="utf-8").read())
        except (OSError, json.JSONDecodeError) as e:
            print(f"cannot read run {src}: {e}", file=sys.stderr)
            return 3
        if latest.get("filters") and any(latest["filters"].values()):
            print("warning: baseline is being taken from a filtered run", file=sys.stderr)
        cfg.baseline_path.parent.mkdir(parents=True, exist_ok=True)
        cfg.baseline_path.write_text(json.dumps(make_baseline(latest), indent=2), encoding="utf-8")
        print(f"baseline written: {cfg.baseline_path}")
        return 0

    # run
    unset = unset_vars(cfg)
    if unset:
        local = cfg.root / "lab.local.json"
        print(f"config error: not set: {', '.join(unset)}. Put them in {local} "
              f"(\"vars\": {{...}}, see lab.local.example.json) or set "
              f"{', '.join('FNVLAB_' + k.upper() for k in unset)}. "
              f"{local} {'exists' if local.exists() else 'does not exist'}.", file=sys.stderr)
        return 3
    if a.require_baseline and load_baseline(cfg) is None:
        print(f"no baseline at {cfg.baseline_path}; refusing to run with --require-baseline",
              file=sys.stderr)
        return 4
    reveal = bool(a.reveal) and _confirm_reveal()
    if a.reveal and not reveal:
        return 3
    started = _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    quiet = a.json

    def progress(s):
        if not quiet:
            print(f"  running L{s.layer} {s.id} ...", file=sys.stderr, flush=True)

    results = run_all(scen, cfg, progress)
    regs = find_regressions(results, load_baseline(cfg))
    filters = {"layer": a.layer, "scenario": a.scenario, "tag": a.tag}
    rep = build_report(results, regs, started, reveal, filters)
    if reveal:
        # Revealed details go to the human's terminal only; nothing unredacted is
        # written to disk, where an agent could read it later.
        sys.stderr.write(render_markdown(rep))
        safe = build_report(results, regs, started, False, filters)
        write_reports(safe, cfg.runs_dir)
        rep = safe
    else:
        jp, mp = write_reports(rep, cfg.runs_dir)
    if a.json:
        print(json.dumps(rep, indent=2, ensure_ascii=False))
    else:
        s = rep["summary"]
        print(f"{s['pass']}/{s['total']} pass, {s['fail']} fail, {s['error']} error, "
              f"{s['regressions']} regression(s). Report: {cfg.runs_dir / 'latest.md'}")
    if regs:
        return 1
    if a.strict and rep["summary"]["pass"] != rep["summary"]["total"]:
        return 2
    return 0


if __name__ == "__main__":
    sys.exit(main())
