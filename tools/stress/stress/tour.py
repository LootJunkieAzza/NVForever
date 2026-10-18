"""tour: laps of the Mojave with the engine's counts and the process's memory sampled
(docs/design/stress-tests.md §4).

The route is a ring of waypoints inside the worldspace's grid (fnvtool `world list`),
or a route file of your own; the `NVF_PLAY_TEST_TOUR` aid (nvf-play stress.rs) steps
the player along it, and `NVF_PLAY_TEST_SAMPLE` (nvforever stress_sample.rs) logs what
the engine holds every few seconds. The runner adds the process's working set and
private bytes, and reports counts that grow lap over lap or stay up when the tour
comes back to a waypoint.
"""
from __future__ import annotations

import json
import math
import re
import subprocess

from . import lib

CELL = 4096.0
SAMPLE = re.compile(r"stress sample: t ([\d.]+) s, (.*), at (.*)$")
FIELD = re.compile(r"^(?P<name>[a-z %]+?) (?P<value>-?[\d.]+(?:/[\d.]+)*)(?: ms)?$")
LAP_START = re.compile(r"stress tour: lap (\d+) starts")
WAYPOINT = re.compile(r"stress tour: lap (\d+) at waypoint (\d+)/(\d+)")
FRAMES = re.compile(r"stress frames: (\d+) frames, mean ([\d.]+) ms, 1% low ([\d.]+) ms \(([\d.]+) fps\), worst ([\d.]+) ms")
STARTUP = re.compile(r"stress start-up: (\d+) frames, ([\d.]+) ms, slowest ([\d.]+) ms")
HITCH = re.compile(r"stress hitch: ([\d.]+) ms at t ([\d.]+) s, (\d+) cells loaded in it \(([\d.]+) ms\), at (.*)$")
PROGRESS = re.compile(r"stress (tour|sample)")
# Counts a leak would show in (the engine's sample fields, and the process's memory).
WATCHED = ["entities", "refs drawn", "meshes", "images", "materials", "models cached", "textures cached", "audio",
           "cells", "bodies", "actors", "ragdolls", "working set", "private bytes"]
# GPU assets and the caches that hold them: back near where they were once the tour has
# left a place behind (the caches keep what nothing uses for a minute, nvf_assets::release).
ASSETS = ["meshes", "images", "models cached", "textures cached"]


def ring(grid_min, grid_max, n: int = 12, inset: float = 0.3) -> list[tuple[float, float]]:
    """`n` waypoints on an ellipse inside the grid bounds (cells), `inset` of the way in
    from each edge, game units, counter-clockwise from the east."""
    (x0, y0), (x1, y1) = grid_min, grid_max
    cx, cy = (x0 + x1 + 1) / 2 * CELL, (y0 + y1 + 1) / 2 * CELL
    rx, ry = (x1 + 1 - x0) * CELL / 2 * (1 - inset), (y1 + 1 - y0) * CELL / 2 * (1 - inset)
    return [(round(cx + rx * math.cos(2 * math.pi * k / n)), round(cy + ry * math.sin(2 * math.pi * k / n))) for k in range(n)]


def route_text(world: str, points, step: float, dwell: float, laps: int) -> str:
    return (f"# tools/stress tour\nworld {world}\nstep {step:g}\ndwell {dwell:g}\nlaps {laps}\n"
            + "".join(f"{x:g} {y:g}\n" for x, y in points))


def parse_sample(line: str) -> dict | None:
    m = SAMPLE.search(line)
    if not m:
        return None
    s = {"t": float(m.group(1)), "at": m.group(3)}
    for part in m.group(2).split(", "):
        f = FIELD.match(part.strip())
        if not f:
            continue
        name, value = f.group("name").strip(), f.group("value")
        if "/" in value:
            s[name] = sum(float(v) for v in value.split("/"))
        else:
            s[name] = float(value)
    return s


def tag_laps(lines: list[str]) -> list[dict]:
    """The samples of a log, each with the lap and waypoint the tour was on."""
    lap, wp, out = 0, 0, []
    for ln in lines:
        if m := LAP_START.search(ln):
            lap, wp = int(m.group(1)), 0
        elif m := WAYPOINT.search(ln):
            lap, wp = int(m.group(1)), int(m.group(2))
        elif s := parse_sample(ln):
            s["lap"], s["waypoint"] = lap, wp
            out.append(s)
    return out


def add_memory(samples: list[dict], memory: list[tuple[float, int, int]], started_at: float = 0.0) -> None:
    """The nearest memory reading (by run seconds) on each sample."""
    if not memory:
        return
    for s in samples:
        t = s["t"] + started_at
        _, ws, pb = min(memory, key=lambda m: abs(m[0] - t))
        s["working set"], s["private bytes"] = ws, pb


def growth(samples: list[dict], fields=WATCHED, tolerance: float = 0.05) -> dict:
    """Possible leaks: for each count, its value at each waypoint lap after lap. A count
    that is higher at the same waypoint on every later lap than on the first (by more than
    `tolerance` of it) is reported with the waypoints where it grew, and its lap-end values."""
    laps = sorted({s["lap"] for s in samples if s["lap"] > 0})
    out = {}
    for f in fields:
        at: dict[tuple[int, int], tuple[float, str]] = {}
        for s in samples:
            if f in s and s["lap"] > 0:
                at[(s["lap"], s["waypoint"])] = (s[f], s.get("at", ""))  # the last sample there
        ends = []
        for lap in laps:
            vals = [v for (l, _), (v, _) in sorted(at.items()) if l == lap]
            if vals:
                ends.append(vals[-1])
        grew = []
        for (l, wp), (v, place) in sorted(at.items()):
            if not laps or l == laps[0]:
                continue
            first = at.get((laps[0], wp))
            if first is not None and v > first[0] * (1 + tolerance) + 1:
                grew.append({"lap": l, "waypoint": wp, "first": first[0], "now": v, "at": place})
        rising = len(ends) >= 2 and all(b > a * (1 + tolerance) for a, b in zip(ends, ends[1:]))
        if grew or rising:
            out[f] = {"lap_ends": ends, "rising_every_lap": rising, "grew_at": grew[:30]}
    return out


def not_released(samples: list[dict], fields=ASSETS, part: float = 0.1, tolerance: float = 0.25, slack: float = 100) -> dict:
    """Assets not let go within a lap: a lap ends where it began (the ring closes), so a
    count whose mean over the lap's last `part` of samples is above its mean over the
    first `part` by more than `tolerance` of it plus `slack` was kept after the places
    that loaded it were left. Per count: the laps with (start, end, peak)."""
    out = {}
    for f in fields:
        bad = []
        for lap in sorted({s["lap"] for s in samples if s["lap"] > 0}):
            vals = [s[f] for s in samples if s["lap"] == lap and f in s]
            if len(vals) < 4:
                continue
            n = max(1, round(len(vals) * part))
            start, end = sum(vals[:n]) / n, sum(vals[-n:]) / n
            if end > start * (1 + tolerance) + slack:
                bad.append({"lap": lap, "start": round(start, 1), "end": round(end, 1), "peak": max(vals)})
        if bad:
            out[f] = bad
    return out


def startup_and_hitches(lines: list[str], top: int = 10) -> tuple[dict | None, list[dict]]:
    """The engine's start-up line, and its slowest `top` hitches after start-up."""
    startup = None
    hitches = []
    for ln in lines:
        if m := STARTUP.search(ln):
            startup = {"frames": int(m.group(1)), "ms": float(m.group(2)), "slowest_ms": float(m.group(3))}
        elif m := HITCH.search(ln):
            hitches.append({"ms": float(m.group(1)), "t": float(m.group(2)), "cells": int(m.group(3)),
                            "cell_ms": float(m.group(4)), "at": m.group(5)})
    hitches.sort(key=lambda h: -h["ms"])
    return startup, hitches[:top]


def write_csv(path, samples: list[dict]) -> None:
    keys = ["t", "lap", "waypoint"] + sorted({k for s in samples for k in s} - {"t", "lap", "waypoint", "at"}) + ["at"]
    with open(path, "w", encoding="utf-8") as f:
        f.write(",".join(keys) + "\n")
        for s in samples:
            f.write(",".join('"' + str(s.get(k, "")).replace('"', "'") + '"' if k == "at" else str(s.get(k, "")) for k in keys) + "\n")


def world_bounds(ctx: lib.Ctx, world: str):
    cmd = [str(ctx.fnvtool), "world", "list", str(ctx.data), *ctx.masters, "--schema", "schemas/fnv"]
    wl = json.loads(subprocess.run(cmd, capture_output=True, text=True, cwd=ctx.repo, check=True).stdout)
    for w in wl["worldspaces"]:
        if (w.get("edid") or "").lower() == world.lower() or w["formid"].lower() == world.lower():
            return w["formid"], w["grid_min"], w["grid_max"]
    raise SystemExit(f"no worldspace {world} in fnvtool's world list")


def run(ctx: lib.Ctx, route: str, laps: int, every: float, hang: float, start_args: list[str], est_seconds: float) -> dict:
    path = ctx.out / "route.txt"
    path.write_text(route, encoding="utf-8")
    launch = lib.Launch("tour", start_args, {"NVF_PLAY_TEST_TOUR": str(path), "NVF_PLAY_TEST_SAMPLE": f"{every:g}"},
                        run_seconds=est_seconds, progress=PROGRESS, hang_seconds=hang, memory_every=every)
    r = lib.run(ctx, launch)
    samples = tag_laps(r.lines)
    # The sample's t is the engine's real clock since its start; the memory's seconds count
    # from the launch: the first sample's line time against the launch's start aligns them.
    first = next((ln for ln in r.lines if "stress sample:" in ln), None)
    start_line = next((ln for ln in r.lines if lib.stamp_secs(ln) is not None), None)
    offset = 0.0
    if first and start_line:
        offset = (lib.stamp_secs(first) - lib.stamp_secs(start_line)) - (parse_sample(first) or {"t": 0})["t"]
    add_memory(samples, r.memory, max(offset, 0.0))
    write_csv(ctx.out / "samples.csv", samples)
    frames = next((FRAMES.search(ln) for ln in reversed(r.lines) if FRAMES.search(ln)), None)
    startup, hitches = startup_and_hitches(r.lines)
    laps_done = max((int(m.group(1)) for ln in r.lines if (m := re.search(r"stress tour: lap (\d+) done", ln))), default=0)
    return {
        "test": "tour",
        "status": r.status,
        "laps_asked": laps,
        "laps_done": laps_done,
        "samples": len(samples),
        "frames": None if not frames else {"count": int(frames.group(1)), "mean_ms": float(frames.group(2)),
                                           "low1_ms": float(frames.group(3)), "low1_fps": float(frames.group(4)),
                                           "worst_ms": float(frames.group(5))},
        "peak": {f: max((s[f] for s in samples if f in s), default=None) for f in WATCHED},
        "possible_leaks": growth(samples),
        "not_released": not_released(samples),
        "startup": startup,
        "hitches": hitches,
        "crash_groups": lib.group_crashes([("tour", c) for c in r.crashes]),
        "log": str(r.log),
        "tail": r.tail(15) if r.status != "ok" else [],
    }


def report(res: dict) -> list[str]:
    out = ["# Stress test: tour", "", f"Status {res['status']}: {res['laps_done']} of {res['laps_asked']} laps, "
           f"{res['samples']} samples (samples.csv)."]
    if res["frames"]:
        f = res["frames"]
        out += ["", f"Frames: {f['count']}, mean {f['mean_ms']} ms, 1% low {f['low1_ms']} ms ({f['low1_fps']} fps), worst {f['worst_ms']} ms"
                " (after start-up)"]
    if res.get("startup"):
        st = res["startup"]
        out.append(f"Start-up: {st['frames']} frames, {st['ms']:g} ms, slowest {st['slowest_ms']:g} ms (left out of the frames above)")
    if res.get("hitches"):
        out += ["", "## Slowest hitches", "", "| ms | t | cells loaded in it | at |", "|---|---|---|---|"]
        out += [f"| {h['ms']:g} | {h['t']:g} | {h['cells']} ({h['cell_ms']:g} ms) | {h['at']} |" for h in res["hitches"]]
    out += ["", "## Peaks", "", "| count | peak |", "|---|---|"]
    for k, v in res["peak"].items():
        if v is not None:
            out.append(f"| {k} | {lib.mb(v) if k in ('working set', 'private bytes') else f'{v:g}'} |")
    out += ["", "## Possible leaks", ""]
    if not res["possible_leaks"]:
        out.append("None: no count grew lap over lap or stayed up on coming back to a waypoint.")
    for k, v in res["possible_leaks"].items():
        out.append(f"- **{k}**: lap ends {v['lap_ends']}" + (" (rising every lap)" if v["rising_every_lap"] else ""))
        for g in v["grew_at"][:10]:
            out.append(f"  - lap {g['lap']} waypoint {g['waypoint']} ({g['at']}): {g['first']:g} on lap 1, {g['now']:g} now")
    out += ["", "## Assets not released", ""]
    if not res.get("not_released"):
        out.append("None: meshes, images and the caches came back near their lap-start counts by each lap's end.")
    for k, laps in res.get("not_released", {}).items():
        for b in laps:
            out.append(f"- **{k}**: lap {b['lap']} started near {b['start']:g}, ended near {b['end']:g} (peak {b['peak']:g})")
    if res["crash_groups"]:
        out += ["", "## Crashes", ""] + [f"- {g['message']} at {g['location']}" for g in res["crash_groups"]]
    if res["tail"]:
        out += ["", "## Last log lines", "", "```", *res["tail"], "```"]
    return out
