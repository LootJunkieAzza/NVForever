"""cells: every cell of the masters loads without crashing (docs/design/stress-tests.md §3).

The cells come from fnvtool (`world list`, then `world cells` for each worldspace and
for the interiors) over the same ten masters in the same order as the engine's own
load order, so the FormIDs agree; the engine checks each exterior against its own grid
and skips one that does not fit (the log says so). Visits go in batches, one launch
each, through the `NVF_PLAY_TEST_CELLS` aid (nvf-play stress.rs): a crash or hang ends
its batch, and the next batch starts after the cell it was on.
"""
from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass
from pathlib import Path

from . import lib

START = re.compile(r"stress cell: (\d+)/(\d+) start (.*)$")
LOADED = re.compile(r"stress cell: (\d+)/(\d+) loaded in ([\d.]+) s (.*)$")
SKIPPED = re.compile(r"stress cell: (\d+)/(\d+) skipped: (.*)$")
PROGRESS = re.compile(r"stress cell")


@dataclass(frozen=True)
class Cell:
    kind: str  # interior | exterior
    cell: str  # FormID, 8 hex digits
    label: str
    world: str = ""
    x: int = 0
    y: int = 0

    def line(self) -> str:
        if self.kind == "interior":
            return f"interior {self.cell} {self.label}".rstrip()
        return f"exterior {self.world} {self.x} {self.y} {self.cell} {self.label}".rstrip()

    def name(self) -> str:
        if self.kind == "interior":
            return f"{self.label or self.cell} ({self.cell})"
        return f"{self.label or ''} {self.world}:{self.x},{self.y} ({self.cell})".strip()


def fid(s) -> str:
    return f"{int(str(s), 16):08X}"


def cells_from_json(world_list: dict, per_world: dict[str, dict], interiors: dict) -> list[Cell]:
    """Cells from fnvtool's fnv.world.list/1 and fnv.world.cells/1 documents: the interiors,
    then each worldspace's grid cells (its persistent cell has no grid and is left out)."""
    out = [Cell("interior", fid(c["formid"]), c.get("edid") or "") for c in interiors.get("cells", [])]
    for w in world_list.get("worldspaces", []):
        doc = per_world.get(w["formid"])
        if not doc:
            continue
        wname = w.get("edid") or ""
        for c in doc.get("cells", []):
            g = c.get("grid")
            if not g:
                continue
            out.append(Cell("exterior", fid(c["formid"]), c.get("edid") or wname, fid(w["formid"]), int(g[0]), int(g[1])))
    return out


def choose(cells: list[Cell], kinds: str = "all", stride: int = 1, worlds: list[str] | None = None,
           limit: int | None = None) -> list[Cell]:
    """The cells to visit: by kind, exteriors on every `stride`-th grid line both ways, only
    the named worldspaces (Editor IDs or FormIDs), at most `limit`."""
    want = {w.lower() for w in worlds or []}
    out = []
    for c in cells:
        if kinds != "all" and c.kind != kinds:
            continue
        if c.kind == "exterior":
            if stride > 1 and (c.x % stride or c.y % stride):
                continue
            if want and c.world.lower() not in want and c.label.lower() not in want:
                continue
        out.append(c)
    return out[:limit] if limit else out


def batches(cells: list[Cell], size: int) -> list[list[Cell]]:
    return [cells[i:i + size] for i in range(0, len(cells), size)]


def read_batch(lines: list[str]) -> dict:
    """What one launch's log says: loaded cells (1-based index -> seconds), skipped ones,
    and the last cell started (the one a crash or hang was on)."""
    loaded, skipped, last = {}, {}, None
    for ln in lines:
        if m := LOADED.search(ln):
            loaded[int(m.group(1))] = float(m.group(3))
        elif m := SKIPPED.search(ln):
            skipped[int(m.group(1))] = m.group(3)
        elif m := START.search(ln):
            last = int(m.group(1))
    return {"loaded": loaded, "skipped": skipped, "last_started": last}


def enumerate_cells(ctx: lib.Ctx) -> list[Cell]:
    def tool(*args) -> dict:
        cmd = [str(ctx.fnvtool), "world", *args[:1], str(ctx.data), *ctx.masters, "--schema", "schemas/fnv", *args[1:]]
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=ctx.repo, check=True)
        return json.loads(r.stdout)

    wl = tool("list")
    per = {w["formid"]: tool("cells", "--world", w["formid"]) for w in wl["worldspaces"]}
    return cells_from_json(wl, per, tool("cells"))


def run(ctx: lib.Ctx, cells: list[Cell], batch: int, dwell: float, hang: float, start_args: list[str]) -> dict:
    todo = list(cells)
    rows: list[dict] = []
    crashes: list[tuple[str, lib.Crash]] = []
    n_launch = 0
    while todo:
        n_launch += 1
        part = todo[:batch]
        name = f"batch-{n_launch:03d}"
        listing = ctx.out / f"{name}.cells.txt"
        listing.write_text("# tools/stress cells\n" + "".join(c.line() + "\n" for c in part), encoding="utf-8")
        launch = lib.Launch(name, start_args, {"NVF_PLAY_TEST_CELLS": f"{listing};{dwell:g}"},
                            run_seconds=len(part) * (dwell + 40) + 120, progress=PROGRESS, hang_seconds=hang)
        ctx.say(f"cells: {name}: {len(part)} cells ({part[0].name()} ...)")
        r = lib.run(ctx, launch)
        seen = read_batch(r.lines)
        for i, c in enumerate(part, 1):
            if i in seen["loaded"]:
                rows.append({"cell": c.name(), "status": "ok", "seconds": seen["loaded"][i], "batch": name})
            elif i in seen["skipped"]:
                rows.append({"cell": c.name(), "status": "skipped", "why": seen["skipped"][i], "batch": name})
        stop = seen["last_started"]
        broken = r.status != "ok" and stop is not None and stop not in seen["loaded"]
        if broken:
            c = part[stop - 1]
            row = {"cell": c.name(), "status": "crashed" if r.status == "crashed" else "hung", "batch": name,
                   "log": str(r.log), "tail": r.tail(15)}
            if r.stacks:
                row["stacks"] = str(r.stacks)
            rows.append(row)
            for cr in r.crashes or [lib.Crash(f"no panic report ({r.status}, exit {r.exit})", "", [], "")]:
                crashes.append((c.name(), cr))
            todo = todo[stop:]
        elif r.status != "ok" and not broken:
            # Ended badly between cells (or before the first): the rest of the batch again,
            # from the first cell not yet visited.
            done = max([*seen["loaded"], *seen["skipped"], 0])
            crashes.append((f"{name} after cell {done}", (r.crashes or [lib.Crash(f"{r.status}, exit {r.exit}", "", [], "")])[0]))
            if done == 0:
                rows.append({"cell": part[0].name(), "status": r.status, "batch": name, "log": str(r.log), "tail": r.tail(15)})
                done = 1
            todo = todo[done:]
        else:
            todo = todo[len(part):]
            missing = [c for i, c in enumerate(part, 1) if i not in seen["loaded"] and i not in seen["skipped"]]
            for c in missing:
                rows.append({"cell": c.name(), "status": "not reached", "batch": name})
    return summarize(rows, crashes, n_launch)


def summarize(rows: list[dict], crashes: list[tuple[str, lib.Crash]], launches: int) -> dict:
    by = lambda s: [r for r in rows if r["status"] == s]  # noqa: E731
    ok = by("ok")
    return {
        "test": "cells",
        "total": len(rows),
        "ok": len(ok),
        "crashed": len(by("crashed")),
        "hung": len(by("hung")),
        "skipped": len(by("skipped")),
        "not_reached": len(by("not reached")),
        "launches": launches,
        "slowest": sorted(ok, key=lambda r: -r["seconds"])[:20],
        "mean_load_seconds": round(sum(r["seconds"] for r in ok) / len(ok), 3) if ok else None,
        "crash_groups": lib.group_crashes(crashes),
        "cells": rows,
    }


def report(res: dict) -> list[str]:
    out = [f"# Stress test: cells", "",
           f"{res['total']} cells: {res['ok']} OK, {res['crashed']} crashed, {res['hung']} hung, "
           f"{res['skipped']} skipped, {res['not_reached']} not reached, in {res['launches']} launches; "
           f"mean load {res['mean_load_seconds']} s", ""]
    if res["crash_groups"]:
        out += ["## Crash groups", ""]
        for i, g in enumerate(res["crash_groups"], 1):
            out += [f"{i}. **{g['message'] or '(no message)'}** at `{g['location']}`: {len(g['where'])} cell(s)",
                    *(f"   - `{f}`" for f in g["frames"]),
                    "   - cells: " + ", ".join(g["where"][:20]) + (" ..." if len(g["where"]) > 20 else ""), ""]
    out += ["## Slowest loads", "", "| cell | seconds |", "|---|---|"]
    out += [f"| {r['cell']} | {r['seconds']:.2f} |" for r in res["slowest"]]
    bad = [r for r in res["cells"] if r["status"] in ("crashed", "hung", "failed")]
    if bad:
        out += ["", "## Crashed or hung cells", ""]
        for r in bad:
            out += [f"- {r['cell']}: {r['status']} ({r['batch']}, log {r.get('log', '')})"]
    return out
