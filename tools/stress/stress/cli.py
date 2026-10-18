"""`python tools/stress run <test> [options]`: run a stress test and write
target/stress/<test>/<run>/results.json and report.md (docs/design/stress-tests.md).

`python tools/stress list` names the tests. Each test is a module with `run(...)` and
`report(results)`; adding one is a module and an entry in TESTS.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path

from . import battle, cells, lib, tour

TESTS = {
    "cells": "every interior and exterior cell of the masters loads without crashing",
    "tour": "laps of the Mojave, the engine's counts and the process's memory sampled for leaks",
    "battle": "NCR against the Legion in Goodsprings: frame time, memory, combat measures",
}


def common(p: argparse.ArgumentParser) -> None:
    p.add_argument("--exe", type=Path, default=None, help="nvforever (default NVF_EXE or <repo>/engine/target/release)")
    p.add_argument("--data", default=None, help="game Data folder (default NVF_DATA or FNVLAB_DATA_DIR)")
    p.add_argument("--fnvtool", type=Path, default=None, help="fnvtool (default NVF_FNVTOOL or <repo>/target/release)")
    p.add_argument("--name", default=None, help="the run's folder name under target/stress/<test>/ (default: the time)")
    p.add_argument("--no-locks", action="store_true", help="do not take the game locks (a machine of your own)")


def parser() -> argparse.ArgumentParser:
    ap = argparse.ArgumentParser(prog="python tools/stress", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = ap.add_subparsers(dest="cmd", required=True)
    sub.add_parser("list", help="the tests")
    run = sub.add_parser("run", help="run a test")
    tests = run.add_subparsers(dest="test", required=True)

    c = tests.add_parser("cells", help=TESTS["cells"])
    common(c)
    c.add_argument("--kinds", choices=["all", "interior", "exterior"], default="all")
    c.add_argument("--worlds", nargs="*", default=None, help="only these worldspaces (Editor IDs or FormIDs)")
    c.add_argument("--stride", type=int, default=1, help="exteriors on every N-th grid line both ways (1: every cell)")
    c.add_argument("--limit", type=int, default=None, help="at most this many cells")
    c.add_argument("--batch", type=int, default=100, help="cells per launch (50 to 200 is usual)")
    c.add_argument("--dwell", type=float, default=2.0, help="seconds in each cell once it is in")
    c.add_argument("--hang", type=float, default=180.0, help="seconds without progress that count as a hang")
    c.add_argument("--start", default="GSGeneralStore", help="the cell each launch starts in (--cell)")

    t = tests.add_parser("tour", help=TESTS["tour"])
    common(t)
    t.add_argument("--world", default="WastelandNV")
    t.add_argument("--laps", type=int, default=1)
    t.add_argument("--waypoints", type=int, default=12, help="waypoints on the ring (without --route)")
    t.add_argument("--inset", type=float, default=0.3, help="how far in from the grid's edges the ring lies (0..1)")
    t.add_argument("--route", type=Path, default=None, help="a route file of your own (nvf-play stress.rs parse_route)")
    t.add_argument("--step", type=float, default=1024.0, help="units a step")
    t.add_argument("--dwell", type=float, default=1.0, help="seconds between steps")
    t.add_argument("--every", type=float, default=5.0, help="seconds between samples")
    t.add_argument("--hang", type=float, default=180.0)

    b = tests.add_parser("battle", help=TESTS["battle"])
    common(b)
    b.add_argument("--per-side", type=int, default=10, help="fighters a side (10, 25, 50)")
    b.add_argument("--third", type=int, default=0, help="fighters in a third group (creatures), 0 for none")
    b.add_argument("--seconds", type=float, default=180.0, help="how long the run lasts")
    b.add_argument("--every", type=float, default=2.0, help="seconds between samples")
    b.add_argument("--centre", default=f"{battle.CENTRE[0]:g},{battle.CENTRE[1]:g}", help="x,y the battle is round")
    b.add_argument("--gap", type=float, default=3000.0, help="units between the sides")
    return ap


def context(a, test: str) -> lib.Ctx:
    repo = lib.REPO
    exe = a.exe or (Path(os.environ["NVF_EXE"]) if os.environ.get("NVF_EXE") else repo / "engine" / "target" / "release" / lib.EXE_NAME)
    tool = a.fnvtool or (Path(os.environ["NVF_FNVTOOL"]) if os.environ.get("NVF_FNVTOOL") else repo / "target" / "release" / lib.TOOL_NAME)
    data = a.data or os.environ.get("NVF_DATA") or os.environ.get("FNVLAB_DATA_DIR")
    if not data:
        raise SystemExit("no game data: pass --data or set NVF_DATA")
    if not Path(exe).exists():
        raise SystemExit(f"no nvforever at {exe}: build it (engine workspace) or pass --exe / NVF_EXE")
    out = lib.run_folder(repo, test, a.name)
    locks = None if a.no_locks else lib.Locks(lib.default_lock_paths(repo))
    return lib.Ctx(repo, Path(exe), data, Path(tool), out, print, locks, lib.masters(repo))


def main(argv=None) -> int:
    a = parser().parse_args(argv)
    if a.cmd == "list":
        for k, v in TESTS.items():
            print(f"{k:<8} {v}")
        return 0
    ctx = context(a, a.test)
    if a.test == "cells":
        if not ctx.fnvtool.exists():
            raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace) or pass --fnvtool")
        everything = cells.enumerate_cells(ctx)
        chosen = cells.choose(everything, a.kinds, a.stride, a.worlds, a.limit)
        print(f"cells: {len(chosen)} of {len(everything)} cells, {a.batch} a launch")
        res = cells.run(ctx, chosen, a.batch, a.dwell, a.hang, ["--cell", a.start])
        lines = cells.report(res)
        bad = res["crashed"] + res["hung"]
    elif a.test == "tour":
        if a.route:
            route = a.route.read_text(encoding="utf-8")
            npts = sum(1 for ln in route.splitlines() if len(ln.split("#")[0].split()) == 2 and ln.split()[0] not in ("world", "step", "dwell", "laps"))
            perimeter = None
        else:
            if not ctx.fnvtool.exists():
                raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace), or pass --fnvtool or --route")
            wid, lo, hi = tour.world_bounds(ctx, a.world)
            pts = tour.ring(lo, hi, a.waypoints, a.inset)
            route = tour.route_text(wid, pts, a.step, a.dwell, a.laps)
            npts = len(pts)
            perimeter = sum(((pts[i][0] - pts[i - 1][0]) ** 2 + (pts[i][1] - pts[i - 1][1]) ** 2) ** 0.5 for i in range(len(pts)))
        steps = (perimeter / a.step if perimeter else npts * 20) + npts
        est = a.laps * steps * (a.dwell + 3.0) + 300
        # Start outdoors in the tour's worldspace; the first step goes to the first waypoint.
        start = ["--cell", "Goodsprings", "--hour", "12"]
        print(f"tour: {npts} waypoints, about {steps:.0f} steps a lap, {a.laps} laps; run limit {est:.0f} s")
        res = tour.run(ctx, route, a.laps, a.every, a.hang, start, est)
        lines = tour.report(res)
        bad = 0 if res["status"] == "ok" else 1
    else:
        cx, cy = (float(v) for v in a.centre.split(","))
        res = battle.run(ctx, a.per_side, a.third, a.seconds, a.every, (cx, cy), a.gap)
        lines = battle.report(res)
        bad = 0 if res["status"] == "ok" else 1
    lib.write_results(ctx.out, res, lines)
    print("\n".join(lines[:12]))
    print(f"\nresults: {ctx.out / 'results.json'}\nreport:  {ctx.out / 'report.md'}")
    return 1 if bad else 0
