"""soak: the tour's lap of the Mojave with play mixed in, to find hitches and leaks
where they come from (docs/design/stress-tests.md §4.1).

Along the lap, every `event_every` seconds, one event in turn:

- **container**: the player is moved to a placed container near the route, opens it
  (`activate`, the container menu as E opens it) and closes it with Tab three seconds on;
- **door**: the player is moved to a teleport door near the route and goes through it
  (a load behind the loading screen; the tour's next step brings them back outdoors);
- **fight**: a hostile NPC near the route (a Powder Ganger, Fiend or Viper) is moved
  beside the player and set on them (StartCombat); 15 s on it is killed (death, ragdoll,
  dropped weapon).

The events are `NVF_PLAY_TEST_SCRIPT` steps and `NVF_PLAY_TEST_KEYS` presses, both on the
clock that starts when the scripts first run (`NVF_PLAY_AIDS_FROM_SCRIPTS=1`), whose start
the log names; so each hitch is put down to the event before it (within
[`WINDOW`] seconds) or to walking. `NVF_PLAY_TEST_STRESS=0`: scripts may move the player
and conversations are answered as an unattended run answers them (the moves are the
point here), where the tour ends them.
"""
from __future__ import annotations

import json
import re
import subprocess
from pathlib import Path

from . import lib, tour

KINDS = ("container", "door", "fight")
# Hostile people placed persistently in the Mojave, by their base's Editor ID.
FIGHTERS = re.compile(r"^(LvlPowderG|LvlFiend|VViperGang|PrimmPowde)")
# A candidate is within this of a route point (game units).
NEAR = 3 * tour.CELL
# A hitch this soon after an event (seconds) is put down to it.
WINDOW = 8.0
CLOCK = re.compile(r"test aids: clock starts when the scripts first run")
OUTCOMES = {
    "containers opened": re.compile(r"menu: open Container\("),
    "loads (doors, steps beyond the loaded ground)": re.compile(r"loading screen down after"),
    "kills": re.compile(r"test aid: the player killed"),
    "aid steps that failed": re.compile(r"NVF_PLAY_TEST_SCRIPT.*(?:failed|error|not found|cannot)", re.I),
}


def fnvtool_json(ctx: lib.Ctx, args: list[str]) -> dict:
    cmd = [str(ctx.fnvtool), *args, str(ctx.data), *ctx.masters]
    out = subprocess.run(cmd, cwd=ctx.repo, capture_output=True, check=True).stdout
    return json.loads(out)


def teleport_bases(models: dict) -> set[str]:
    """DOOR bases every placed reference of which is a teleport (sim door-models)."""
    return {d["door"] for d in models["doors"] if d.get("teleport_refs") and d["teleport_refs"] == d.get("refs")}


def candidates(refs: list[dict], doors: set[str], points: list[tuple[float, float]]) -> dict[str, list[dict]]:
    """The references of each kind near the route, in route order."""
    out: dict[str, list[dict]] = {k: [] for k in KINDS}
    for r in refs:
        if r.get("deleted") or r.get("initially_disabled") or r.get("enable_parent"):
            continue
        bt, edid = r.get("base_type"), r.get("base_edid") or ""
        kind = ("container" if bt == "CONT"
                else "door" if bt == "DOOR" and r["base"] in doors
                else "fight" if bt in ("NPC_", "CREA") and r.get("persistent") and FIGHTERS.match(edid)
                else None)
        if not kind:
            continue
        x, y = r["position"][0], r["position"][1]
        d, i = min(((x - p[0]) ** 2 + (y - p[1]) ** 2, i) for i, p in enumerate(points))
        if d <= NEAR * NEAR:
            out[kind].append({"ref": r["formid"], "base": edid, "pos": r["position"][:3], "route_point": i})
    for v in out.values():
        v.sort(key=lambda c: c["route_point"])
    return out


def spread(items: list, n: int) -> list:
    """`n` of `items` evenly spaced (all of them when there are fewer)."""
    if n >= len(items):
        return list(items)
    return [items[round(i * (len(items) - 1) / max(n - 1, 1))] for i in range(n)]


def schedule(cands: dict[str, list[dict]], start: float, every: float, until: float) -> tuple[list[str], list[str], list[dict]]:
    """The script aid steps, the key presses and the events, one event every `every`
    seconds from `start` while before `until`, the kinds in turn (a kind with no
    candidates left out)."""
    kinds = [k for k in KINDS if cands.get(k)]
    if not kinds:
        return [], [], []
    slots = max(int((until - start) // every), 0)
    per = {k: spread(cands[k], -(-slots // len(kinds))) for k in kinds}
    used = {k: 0 for k in kinds}
    steps, keys, events = [], [], []
    for n in range(slots):
        kind = kinds[n % len(kinds)]
        if used[kind] >= len(per[kind]):
            continue
        c = per[kind][used[kind]]
        used[kind] += 1
        t, ref = start + n * every, c["ref"]
        if kind == "container":
            steps += [f"{t:g}:moveto:player:{ref}", f"{t + 3:g}:activate:{ref}"]
            keys.append(f"{t + 6:g}:tab")
        elif kind == "door":
            steps += [f"{t:g}:moveto:player:{ref}", f"{t + 3:g}:activate:{ref}"]
        else:
            steps += [f"{t:g}:moveto:{ref}:player:300:0:0", f"{t + 1:g}:combat:{ref}:player", f"{t + 15:g}:kill:{ref}"]
        events.append({"at": t, "kind": kind, "ref": ref, "base": c["base"]})
    return steps, keys, events


def attribute(lines: list[str], events: list[dict], window: float = WINDOW) -> dict:
    """Each hitch (`stress hitch`, `profile hitch`) put down to the event before it within
    `window` seconds, or to `walking`; per cause the count, the worst and the hitches."""
    t0 = next((lib.stamp_secs(lib.clean(ln)) for ln in lines if CLOCK.search(ln)), None)
    by: dict[str, dict] = {}
    for ln in lines:
        c = lib.clean(ln)
        m = tour.HITCH.search(c) or lib.PROFILE_HITCH.search(c)
        at = lib.stamp_secs(c)
        if not m or at is None:
            continue
        ms = float(m.group(1))
        cause = "walking"
        if t0 is not None:
            since = [(at - (t0 + e["at"]), e) for e in events if 0 <= at - (t0 + e["at"]) <= window]
            if since:
                e = min(since, key=lambda s: s[0])[1]
                cause = e["kind"]
        kind = "profile" if "profile hitch" in c else "stress"
        b = by.setdefault(cause, {"stress": 0, "profile": 0, "worst_ms": 0.0, "hitches": []})
        b[kind] += 1
        b["worst_ms"] = max(b["worst_ms"], ms)
        b["hitches"].append({"ms": ms, "log_s": round(at - t0, 1) if t0 is not None else None, "line": c.split(": ", 1)[-1][:240]})
    for b in by.values():
        b["hitches"].sort(key=lambda h: -h["ms"])
        b["hitches"] = b["hitches"][:5]
    return by


def outcomes(lines: list[str]) -> dict[str, int]:
    return {k: sum(1 for ln in lines if r.search(ln)) for k, r in OUTCOMES.items()}


def run(ctx: lib.Ctx, route: str, points: list[tuple[float, float]], world: str, laps: int, every: float, hang: float,
        start_args: list[str], est_seconds: float, tour_seconds: float, event_every: float, aids: dict[str, str] | None = None) -> dict:
    refs = fnvtool_json(ctx, ["world", "refs", "--schema", "schemas/fnv", "--world", world])["refs"]
    doors = teleport_bases(fnvtool_json(ctx, ["sim", "door-models", "--schema", "schemas/fnv"]))
    cands = candidates(refs, doors, points)
    steps, keys, events = schedule(cands, 20.0, event_every, tour_seconds)
    ctx.say(f"soak: {len(events)} events ({', '.join(f'{k} {sum(1 for e in events if e['kind'] == k)}' for k in KINDS)}) "
            f"from {', '.join(f'{len(v)} {k} candidates' for k, v in cands.items())}")
    (ctx.out / "events.json").write_text(json.dumps(events, indent=1), encoding="utf-8")
    soak_aids = {"NVF_PLAY_AIDS_FROM_SCRIPTS": "1", "NVF_PLAY_TEST_STRESS": "0"}
    if steps:
        soak_aids["NVF_PLAY_TEST_SCRIPT"] = ",".join(steps)
    if keys:
        soak_aids["NVF_PLAY_TEST_KEYS"] = ",".join(keys)
    res = tour.run(ctx, route, laps, every, hang, start_args, est_seconds, {**soak_aids, **(aids or {})})
    lines = Path(res["log"]).read_text(encoding="utf-8", errors="replace").splitlines()
    res.update({"test": "soak", "events": events, "outcomes": outcomes(lines), "by_cause": attribute(lines, events)})
    return res


def report(res: dict) -> list[str]:
    out = tour.report(res)
    out[0] = "# Stress test: soak"
    extra = ["", "## Events", "", f"{len(res['events'])} scheduled: "
             + ", ".join(f"{k} {sum(1 for e in res['events'] if e['kind'] == k)}" for k in KINDS) + "."]
    extra += [f"- {k}: {v}" for k, v in res["outcomes"].items()]
    extra += ["", "## Hitches by cause", "",
              f"A hitch within {WINDOW:g} s after an event is put down to it; the rest to walking.", "",
              "| cause | stress hitches (≥250 ms) | profile hitches (≥100 ms) | worst ms |", "|---|---|---|---|"]
    for k, b in sorted(res["by_cause"].items(), key=lambda kv: -kv[1]["worst_ms"]):
        extra.append(f"| {k} | {b['stress']} | {b['profile']} | {b['worst_ms']:g} |")
    for k, b in res["by_cause"].items():
        extra += ["", f"### {k}", ""] + [f"- {h['ms']:g} ms at {h['log_s']} s: `{h['line']}`" for h in b["hitches"]]
    # After the frame figures, before the tour's own tables.
    at = next((i for i, ln in enumerate(out) if ln.startswith("## ")), len(out))
    return out[:at] + extra + [""] + out[at:]
