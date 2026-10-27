"""`python tools/stress run <test> [options]`: run a stress test and write
target/stress/<test>/<run>/results.json and report.md (docs/design/stress-tests.md).

`python tools/stress list` names the tests. Each test is a module with `run(...)` and
`report(results)`; adding one is a module and an entry in TESTS.
"""
from __future__ import annotations

import argparse
import os
from pathlib import Path

from . import battle, cells, lib, soak, tour

TESTS = {
    "cells": "every interior cell of the masters loads without crashing (exteriors: tour)",
    "tour": "laps of the Mojave, the engine's counts and the process's memory sampled for leaks",
    "battle": "NCR against the Legion in Goodsprings: frame time, memory, combat measures",
    "soak": "the tour's lap with containers, doors and fights mixed in: hitches put down to their cause",
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
    c.add_argument("--kinds", choices=["all", "interior", "exterior"], default="interior",
                   help="interior by default: the tour streams the exteriors (owner, 2026-10-21)")
    c.add_argument("--worlds", nargs="*", default=None, help="only these worldspaces (Editor IDs or FormIDs)")
    c.add_argument("--stride", type=int, default=1, help="exteriors on every N-th grid line both ways (1: every cell)")
    c.add_argument("--limit", type=int, default=None, help="at most this many cells")
    c.add_argument("--batch", type=int, default=100, help="cells per launch (50 to 200 is usual)")
    c.add_argument("--dwell", type=float, default=2.0, help="seconds in each cell once it is in")
    c.add_argument("--hang", type=float, default=180.0, help="seconds without progress that count as a hang")
    c.add_argument("--start", default="GSGeneralStore", help="the cell each launch starts in (--cell)")
    c.add_argument("--abort-after", type=int, default=cells.ABORT_AFTER,
                   help="stop after this many launches in a row end before their first cell (the game does not start)")
    c.add_argument("--resume", default=None, metavar="RUN",
                   help="go on with an earlier run (its folder name under target/stress/cells/): cells it has done are skipped")

    t = tests.add_parser("tour", help=TESTS["tour"])
    common(t)
    t.add_argument("--world", default="WastelandNV")
    t.add_argument("--path", choices=["towns", "ring"], default="towns",
                   help="towns: Goodsprings, Primm, Mojave Outpost, Nipton, Novac, Camp McCarran, Nellis, Jacobstown "
                        "and back (--towns to change them); ring: a ring on the walkable cells")
    t.add_argument("--towns", nargs="+", default=None, metavar="CELL",
                   help="exterior cell Editor IDs to go through in order, back to the first (with --path towns)")
    t.add_argument("--laps", type=int, default=1)
    t.add_argument("--waypoints", type=int, default=12, help="waypoints on the ring (without --route)")
    t.add_argument("--inset", type=float, default=0.3,
                   help="how far in from the walkable world's edge (its largest navmesh area) the ring lies (0..1)")
    t.add_argument("--route", type=Path, default=None, help="a route file of your own (nvf-play stress.rs parse_route)")
    t.add_argument("--step", type=float, default=1024.0, help="units a step")
    t.add_argument("--dwell", type=float, default=1.0, help="seconds between steps")
    t.add_argument("--every", type=float, default=5.0, help="seconds between samples")
    t.add_argument("--slice", default=None, metavar="START:END",
                   help="only waypoints START to END (1-based) of the ring, to go over one stretch again "
                        "(the waypoint numbers are the tour log's `at waypoint i/n`)")
    t.add_argument("--hang", type=float, default=180.0)
    t.add_argument("--aid", action="append", default=[], metavar="NAME=VALUE",
                   help="a further test aid for the game, e.g. NVF_PLAY_TEST_NO_READ_AHEAD=1 to compare a run without "
                        "the actor read-ahead (repeatable)")

    s = tests.add_parser("soak", help=TESTS["soak"])
    common(s)
    s.add_argument("--world", default="WastelandNV")
    s.add_argument("--towns", nargs="+", default=None, metavar="CELL", help="the towns of the lap (default: the tour's)")
    s.add_argument("--laps", type=int, default=1)
    s.add_argument("--step", type=float, default=1024.0, help="units a step")
    s.add_argument("--dwell", type=float, default=1.0, help="seconds between steps")
    s.add_argument("--every", type=float, default=5.0, help="seconds between samples")
    s.add_argument("--event-every", type=float, default=20.0, help="seconds between events (container, door, fight in turn)")
    s.add_argument("--hang", type=float, default=180.0)
    s.add_argument("--aid", action="append", default=[], metavar="NAME=VALUE",
                   help="a further test aid for the game, e.g. NVF_PLAY_TEST_PROFILE=5 with a profile build (repeatable)")
    b = tests.add_parser("battle", help=TESTS["battle"])
    common(b)
    b.add_argument("--per-side", type=int, default=10, help="fighters a side (10, 25, 50)")
    b.add_argument("--aid", action="append", default=[], metavar="NAME=VALUE",
                   help="a further test aid for the game, e.g. NVF_PLAY_TEST_SIGHT_CHECK=1 to check the detection's "
                        "shared and kept lines of sight (repeatable)")
    b.add_argument("--third", type=int, default=0, help="fighters in a third group (creatures), 0 for none")
    b.add_argument("--seconds", type=float, default=180.0, help="how long the run lasts")
    b.add_argument("--every", type=float, default=2.0, help="seconds between samples")
    b.add_argument("--centre", default=f"{battle.CENTRE[0]:g},{battle.CENTRE[1]:g}", help="x,y the battle is round")
    b.add_argument("--gap", type=float, default=3000.0, help="units between the sides")
    for p in (t, b, s):
        g = p.add_argument_group("budget", "frame-time limits after start-up; a run over any of them fails (exit 1)")
        g.add_argument("--budget-mean-ms", type=float, default=None, help="largest mean frame")
        g.add_argument("--budget-low1-ms", type=float, default=None, help="largest 1%% low frame")
        g.add_argument("--budget-worst-ms", type=float, default=None, help="largest single frame")
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
    resume = getattr(a, "resume", None)
    if resume and not (repo / "target" / "stress" / test / resume).is_dir():
        raise SystemExit(f"no run {resume} to resume under target/stress/{test}/")
    out = lib.run_folder(repo, test, resume or a.name)
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
        # The results and report are rewritten after every batch, so a long sweep can be
        # read while it runs and nothing is lost if the runner stops.
        res = cells.run(ctx, chosen, a.batch, a.dwell, a.hang, ["--cell", a.start],
                        on_batch=lambda r: lib.write_results(ctx.out, r, cells.report(r)), abort_after=a.abort_after)
        lines = cells.report(res)
        bad = res["crashed"] + res["hung"] + (1 if res["status"] == "aborted" else 0)
    elif a.test == "tour":
        if a.route:
            route = a.route.read_text(encoding="utf-8")
            if a.slice:
                route = tour.slice_route_text(route, a.slice)
            npts = sum(1 for ln in route.splitlines() if tour.is_route_point(ln))
            perimeter = None
        else:
            if not ctx.fnvtool.exists():
                raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace), or pass --fnvtool or --route")
            wid, _, _ = tour.world_bounds(ctx, a.world)
            # On the walkable world (the largest navmesh area), not the grid's bounds: the
            # grid runs on into empty landscape beyond the playable area.
            nav = tour.nav_cells(ctx, a.world)
            if a.path == "towns":
                pts, stops = tour.town_route(tour.world_cells(ctx, a.world), nav, a.towns or tour.TOWNS)
                print("tour: through " + ", ".join(stops))
            else:
                pts = tour.ring_on_cells(nav, a.waypoints, a.inset)
            if a.slice:
                pts = tour.slice_points(pts, a.slice)
            route = tour.route_text(wid, pts, a.step, a.dwell, a.laps)
            npts = len(pts)
            perimeter = sum(((pts[i][0] - pts[i - 1][0]) ** 2 + (pts[i][1] - pts[i - 1][1]) ** 2) ** 0.5 for i in range(len(pts)))
        steps = (perimeter / a.step if perimeter else npts * 20) + npts
        est = a.laps * steps * (a.dwell + 3.0) + 300
        # Start outdoors in the tour's worldspace; the first step goes to the first waypoint.
        start = ["--cell", "Goodsprings", "--hour", "12"]
        print(f"tour: {npts} waypoints, about {steps:.0f} steps a lap, {a.laps} laps; run limit {est:.0f} s")
        aids = dict(kv.split("=", 1) for kv in a.aid)
        res = tour.run(ctx, route, a.laps, a.every, a.hang, start, est, aids)
        lines = tour.report(res)
        bad = 0 if res["status"] == "ok" else 1
    elif a.test == "soak":
        if not ctx.fnvtool.exists():
            raise SystemExit(f"no fnvtool at {ctx.fnvtool}: build it (root workspace) or pass --fnvtool")
        wid, _, _ = tour.world_bounds(ctx, a.world)
        pts, stops = tour.town_route(tour.world_cells(ctx, a.world), tour.nav_cells(ctx, a.world), a.towns or tour.TOWNS)
        print("soak: through " + ", ".join(stops))
        route = tour.route_text(wid, pts, a.step, a.dwell, a.laps)
        perimeter = sum(((pts[i][0] - pts[i - 1][0]) ** 2 + (pts[i][1] - pts[i - 1][1]) ** 2) ** 0.5 for i in range(len(pts)))
        steps = perimeter / a.step + len(pts)
        tour_seconds = a.laps * steps * a.dwell
        est = a.laps * steps * (a.dwell + 3.0) + 600
        res = soak.run(ctx, route, pts, a.world, a.laps, a.every, a.hang, ["--cell", "Goodsprings", "--hour", "12"], est,
                       tour_seconds, a.event_every, dict(kv.split("=", 1) for kv in a.aid))
        lines = soak.report(res)
        bad = 0 if res["status"] == "ok" else 1
    else:
        cx, cy = (float(v) for v in a.centre.split(","))
        res = battle.run(ctx, a.per_side, a.third, a.seconds, a.every, (cx, cy), a.gap, dict(kv.split("=", 1) for kv in a.aid))
        lines = battle.report(res)
        bad = 0 if res["status"] == "ok" else 1
    if a.test in ("tour", "battle", "soak"):
        budget = lib.Budget(a.budget_mean_ms, a.budget_low1_ms, a.budget_worst_ms)
        failed = lib.over_budget(res.get("frames"), budget)
        res["budget"] = {"mean_ms": budget.mean_ms, "low1_ms": budget.low1_ms, "worst_ms": budget.worst_ms, "failed": failed}
        # Right after the status line, so the first lines printed carry the verdict.
        lines[3:3] = lib.budget_report(budget, failed)
        bad += len(failed)
    lib.write_results(ctx.out, res, lines)
    print("\n".join(lines[:12]))
    print(f"\nresults: {ctx.out / 'results.json'}\nreport:  {ctx.out / 'report.md'}")
    return 1 if bad else 0
