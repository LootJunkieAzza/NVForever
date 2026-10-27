"""markers: fast travel to every map marker of the masters (docs/design/stress-tests.md §3b).

The markers come from fnvtool (`world list`, then `world refs` for each worldspace: the
references of the MapMarker static, 0x00000010). Each launch starts in `--start`, shows
every marker as a fast-travel target (the script aid `call:-:ShowAllMapMarkers:1:1:0`),
then for each marker of its batch opens the Pip-Boy on the DATA page, asks to travel
there (the Pip-Boy aid `travel=`, what a click on the World Map does) and answers Yes
(`key=use`): the trip goes out as a door's does (nvf-play pipboy/data/travel.rs). Between
one marker's ask and the next, the log says whether the trip was refused (and why), went
out, and whether its loading screen came down; the missing or broken files and script
errors logged meanwhile are the marker's (cells.PROBLEMS). A crash or hang ends its batch,
and the next batch starts after the marker it was on.
"""
from __future__ import annotations

import json
import re
import subprocess
from dataclasses import dataclass

from . import cells, lib

MAP_MARKER = "0x00000010"
# The Pip-Boy aid's log of a travel step: `test aid: pipboy Travel(<decimal FormID>) at …`.
ASKED = re.compile(r"test aid: pipboy Travel\((\d+)\)")
REFUSED = re.compile(r"fast travel: to .* refused \((.*)\); the box says")
NOT_NAMED = re.compile(r"fast travel: \S+ is not a named map marker")
NO_REF = re.compile(r"fast travel: \S+ has no placed reference")
GONE = re.compile(r"fast travel: to .*: arrive at")
ARRIVED = re.compile(r"loading screen down after ([\d.]+) s")
PROGRESS = re.compile(r"test aid: pipboy|fast travel:|loading screen")


@dataclass(frozen=True)
class Marker:
    form: str  # 8 hex digits
    world: str
    world_edid: str

    def name(self) -> str:
        return f"{self.world_edid or self.world}:{self.form}"


def markers_from_json(world_list: dict, per_world: dict[str, dict]) -> list[Marker]:
    out = []
    for w in world_list.get("worldspaces", []):
        doc = per_world.get(w["formid"]) or {}
        for r in doc.get("refs", []):
            if str(r.get("base", "")).lower() == MAP_MARKER and not r.get("deleted"):
                out.append(Marker(cells.fid(r["formid"]), cells.fid(w["formid"]), w.get("edid") or ""))
    return sorted(set(out), key=lambda m: (m.world, m.form))


def enumerate_markers(ctx: lib.Ctx) -> list[Marker]:
    def tool(*args) -> dict:
        cmd = [str(ctx.fnvtool), "world", *args[:1], str(ctx.data), *ctx.masters, "--schema", "schemas/fnv", *args[1:]]
        r = subprocess.run(cmd, capture_output=True, text=True, cwd=ctx.repo, check=True)
        return json.loads(r.stdout)

    wl = tool("list")
    return markers_from_json(wl, {w["formid"]: tool("refs", "--world", w["formid"]) for w in wl["worldspaces"]})


def aids(part: list[Marker], every: float, first: float = 8.0) -> dict[str, str]:
    """The script aid that shows every marker, and the Pip-Boy steps: per marker, open
    on DATA, ask, answer Yes, `every` seconds apart."""
    steps = []
    for i, m in enumerate(part):
        t = first + i * every
        steps += [f"{t:g}:open", f"{t + 0.5:g}:page=data", f"{t + 1:g}:travel=0x{m.form}", f"{t + 2:g}:key=use"]
    return {"NVF_PLAY_TEST_SCRIPT": "3:call:-:ShowAllMapMarkers:1:1:0", "NVF_PLAY_TEST_PIPBOY": ",".join(steps)}


def read_batch(lines: list[str], part: list[Marker]) -> dict[int, dict]:
    """Per marker of the batch (0-based), what its stretch of the log says: `asked`,
    `refused` (why), `gone` (the trip went out), `arrived` (its loading screen came down,
    seconds), `problems` (cells.PROBLEMS); `last` is the marker last asked."""
    index = {int(m.form, 16): i for i, m in enumerate(part)}
    out: dict[int, dict] = {}
    at = None
    for ln in lines:
        if m := ASKED.search(ln):
            at = index.get(int(m.group(1)))
            if at is not None:
                out[at] = {"asked": True}
            continue
        if at is None:
            continue
        row = out[at]
        if m := REFUSED.search(ln):
            row["refused"] = m.group(1)
        elif NOT_NAMED.search(ln):
            row["refused"] = "not a named map marker"
        elif NO_REF.search(ln):
            row["refused"] = "no placed reference"
        elif GONE.search(ln):
            row["gone"] = True
        elif (m := ARRIVED.search(ln)) and row.get("gone"):
            row["arrived"] = float(m.group(1))
        else:
            for kind, rx in cells.PROBLEMS.items():
                if m := rx.search(ln):
                    row.setdefault("problems", {}).setdefault(kind, []).append(m.group(1).strip())
                    break
    return out


def status(seen: dict) -> str:
    if "refused" in seen:
        return "refused"
    if "arrived" in seen:
        return "ok"
    if seen.get("gone"):
        return "no arrival"
    return "no trip"


def run(ctx: lib.Ctx, markers: list[Marker], batch: int, every: float, hang: float, start_args: list[str],
        on_batch=None) -> dict:
    rows: list[dict] = []
    crashes: list[tuple[str, lib.Crash]] = []
    todo = list(markers)
    n = 0
    while todo:
        n += 1
        part = todo[:batch]
        name = f"batch-{n:03d}"
        launch = lib.Launch(name, start_args + ["--play", "--no-voice", "--no-messages"], aids(part, every),
                            run_seconds=8 + len(part) * every + 30, progress=PROGRESS, hang_seconds=hang)
        ctx.say(f"markers: {name}: {len(part)} markers ({part[0].name()} ...)")
        r = lib.run(ctx, launch)
        seen = read_batch(r.lines, part)
        last = max(seen) if seen else None
        done = len(part) if r.status == "ok" or last is None else last
        for i in range(done):
            s = seen.get(i, {})
            row = {"marker": part[i].name(), "status": status(s), "batch": name}
            for k in ("refused", "arrived", "problems"):
                if k in s:
                    row[k] = s[k]
            rows.append(row)
        if r.status != "ok" and last is not None:
            rows.append({"marker": part[last].name(), "status": r.status, "batch": name, "log": str(r.log), "tail": r.tail(15)})
            for cr in r.crashes or [lib.Crash(f"no panic report ({r.status}, exit {r.exit})", "", [], "")]:
                crashes.append((part[last].name(), cr))
            todo = todo[last + 1:]
        elif r.status != "ok":
            ctx.say(f"markers: {name} ended before its first marker ({r.status}, exit {r.exit}); stopping")
            break
        else:
            todo = todo[len(part):]
        if on_batch:
            on_batch(summarize(rows, crashes, n, len(todo)))
    return summarize(rows, crashes, n, len(todo))


def summarize(rows: list[dict], crashes: list[tuple[str, lib.Crash]], launches: int, remaining: int = 0) -> dict:
    by = lambda s: [r for r in rows if r["status"] == s]  # noqa: E731
    ok = by("ok")
    refusals: dict[str, int] = {}
    for r in by("refused"):
        refusals[r["refused"]] = refusals.get(r["refused"], 0) + 1
    return {
        "test": "markers",
        "total": len(rows) + remaining,
        "ok": len(ok),
        "refused": len(by("refused")),
        "refusals": refusals,
        "no_trip": len(by("no trip")),
        "no_arrival": len(by("no arrival")),
        "crashed": len(by("crashed")),
        "hung": len(by("hung")),
        "remaining": remaining,
        "launches": launches,
        "slowest": sorted(ok, key=lambda r: -r["arrived"])[:20],
        "crash_groups": lib.group_crashes(crashes),
        "problems": cells.problem_summary([{"cell": r["marker"], **r} for r in rows]),
        "markers": rows,
    }


def report(res: dict) -> list[str]:
    out = ["# Stress test: markers", "",
           f"{res['total']} map markers: {res['ok']} travelled to, {res['refused']} refused, {res['no_trip']} no trip, "
           f"{res['no_arrival']} no arrival, {res['crashed']} crashed, {res['hung']} hung, {res['remaining']} not reached, "
           f"in {res['launches']} launches", ""]
    if res["refusals"]:
        out += ["Refusals: " + ", ".join(f"{k} {v}" for k, v in sorted(res["refusals"].items(), key=lambda kv: -kv[1])), ""]
    if res["crash_groups"]:
        out += ["## Crash groups", ""]
        for i, g in enumerate(res["crash_groups"], 1):
            out += [f"{i}. **{g['message'] or '(no message)'}** at `{g['location']}`: " + ", ".join(g["where"][:20]), ""]
    pr = res["problems"]
    if any(v["cells"] for v in pr.values()):
        out += ["## Missing files and script errors", ""]
        for k, v in pr.items():
            for g in v["top"][:10]:
                out.append(f"- {k} `{g['what']}`: {g['cells']} marker(s), e.g. {', '.join(g['e.g.'])}")
        out.append("")
    out += ["## Slowest arrivals", "", "| marker | seconds |", "|---|---|"]
    out += [f"| {r['marker']} | {r['arrived']:.2f} |" for r in res["slowest"]]
    return out
