"""battle: NCR troopers against Legion soldiers in Goodsprings, watched by an unseen,
god-mode player (docs/design/stress-tests.md §5).

The fighters are placed by the `NVF_PLAY_TEST_BATTLE` aid (nvf-play stress.rs) from the
leveled lists the AI observer's npc_battles campaign spawned in the original game
(LvlNCRTrooper, LvlLegionRaiderMelee, and VCrTier1CoyoteMed for a third group;
tools/capture/observe/npc_battles.campaign.json), each set on the nearest enemy. What is
measured comes from the engine's own summary lines (battle summary, npc combat summary,
combat, ragdolls: the measures of docs/spec-notes/npc-behaviour-compare.md), the stress
samples (nvforever stress_sample.rs) and the process's memory.
"""
from __future__ import annotations

import re
import statistics

from . import lib, tour

NCR = "LvlNCRTrooper"
LEGION = "LvlLegionRaiderMelee"
THIRD = "VCrTier1CoyoteMed"
# Goodsprings: the death-cam scenario's spot by the town's road, on open ground [observed:
# smoke death-cam-first places the player there].
CENTRE = (-68375.0, 4800.0)

STARTED = re.compile(r"stress battle: (\d+) groups, (\d+) fighters")
PLACED = re.compile(r"stress battle: group (\d+) (\S+) \(([^)]*)\): (\d+) of (\d+) placed")
KILLED = re.compile(r"battle: (\S+) (0x[0-9A-F]+) killed by (\S+) (0x[0-9A-F]+)")
SUMMARY = re.compile(r"battle summary: (.*)$")
SIDE = re.compile(r"(\w+) \((\d+)\): (\d+) standing, (\d+) dead, (\d+) hits, (\d+) kills, (\d+) flights")
NPC_SUMMARY = re.compile(r"npc combat summary: (\d+) actors fought .*?; (\d+) shots .*?, (\d+) blows .*?, (\d+) reloads; (\d+) flee checks, (\d+) fled")
COMBAT = re.compile(r"combat: (\d+) shots .*?, (\d+) projectiles, (\d+) actor hits, (\d+) world hits")
RAGDOLLS = re.compile(r"ragdolls: (\d+) started")
LINE = re.compile(r"combat line: (\S+) says (\w+) line")
COVER = re.compile(r"npc combat: .* takes cover")
FLEES = re.compile(r"npc combat: .* (flees|fled)")
FIGHTS = re.compile(r"npc combat: (?:\S+ )?(0x[0-9A-F]+) fights (.+?) with ")
SWING = re.compile(r"swings at .* from (\d+) units")
STUCK = re.compile(r"actor ([0-9A-F]{8}) stuck at")
TIMING = re.compile(r"(?:INFO|WARN) (\S+?): .*?(\d[\d.]*) (ms per frame|ms a frame|us per frame)")
PANIC = re.compile(r"panicked at")
PROGRESS = re.compile(r".")


def layout(per_side: int, third: int, centre=CENTRE, gap: float = 3000.0) -> tuple[str, tuple[float, float]]:
    """The battle aid's value and where the player stands: the sides `gap` apart across
    the centre (west and east), a third group north of it, the player south of it looking
    north at them."""
    cx, cy = centre
    groups = [f"{NCR}:{per_side}:{cx - gap / 2:g}:{cy:g}", f"{LEGION}:{per_side}:{cx + gap / 2:g}:{cy:g}"]
    if third:
        groups.append(f"{THIRD}:{third}:{cx:g}:{cy + gap / 2:g}")
    return "5;" + ";".join(groups), (cx, cy - gap * 0.6)


def analyse(lines: list[str]) -> dict:
    start = next((lib.stamp_secs(ln) for ln in lines if STARTED.search(ln)), None)
    last = next((lib.stamp_secs(ln) for ln in reversed(lines) if lib.stamp_secs(ln)), None)
    minutes = max(((last or 0) - (start or 0)) / 60.0, 1e-6)
    ttk, kills_by, lines_by, fights, swings, stuck = [], {}, {}, {}, [], set()
    for ln in lines:
        t = lib.stamp_secs(ln)
        if (m := KILLED.search(ln)) and start and t:
            ttk.append(t - start)
            kills_by[m.group(3)] = kills_by.get(m.group(3), 0) + 1
        elif m := LINE.search(ln):
            lines_by[m.group(2)] = lines_by.get(m.group(2), 0) + 1
        elif m := FIGHTS.search(ln):
            fights.setdefault(m.group(1), []).append(m.group(2))
        if m := SWING.search(ln):
            swings.append(int(m.group(1)))
        if m := STUCK.search(ln):
            stuck.add(m.group(1))
    sides = []
    if s := next((SUMMARY.search(ln) for ln in reversed(lines) if SUMMARY.search(ln)), None):
        sides = [{"faction": m.group(1), "actors": int(m.group(2)), "standing": int(m.group(3)), "dead": int(m.group(4)),
                  "hits": int(m.group(5)), "kills": int(m.group(6)), "flights": int(m.group(7))} for m in SIDE.finditer(s.group(1))]
    npc = next((NPC_SUMMARY.search(ln) for ln in reversed(lines) if NPC_SUMMARY.search(ln)), None)
    cmb = next((COMBAT.search(ln) for ln in reversed(lines) if COMBAT.search(ln)), None)
    shots = int(npc.group(2)) if npc else 0
    hits = int(cmb.group(3)) if cmb else 0
    switches = sum(max(len(set(v)) - 1, 0) for v in fights.values())
    placed = [{"group": int(m.group(1)), "base": m.group(2), "placed": int(m.group(4)), "asked": int(m.group(5))}
              for ln in lines if (m := PLACED.search(ln))]
    timings = {}
    for ln in lines:
        if "summary" in ln or "at the end" in ln or "per frame" in ln:
            if m := TIMING.search(ln):
                timings.setdefault(m.group(1), f"{m.group(2)} {m.group(3)}")
    rag = next((RAGDOLLS.search(ln) for ln in reversed(lines) if RAGDOLLS.search(ln)), None)
    return {
        "placed": placed,
        "sides": sides,
        "time_to_kill_s": {"count": len(ttk), "median": round(statistics.median(ttk), 1) if ttk else None,
                           "first": round(min(ttk), 1) if ttk else None, "last": round(max(ttk), 1) if ttk else None},
        "shots": shots,
        "blows": int(npc.group(3)) if npc else 0,
        "actor_hits": hits,
        "hit_rate": round(hits / shots, 3) if shots else None,
        "projectiles": int(cmb.group(2)) if cmb else 0,
        "reloads": int(npc.group(4)) if npc else 0,
        "flee_checks": int(npc.group(5)) if npc else 0,
        "fled": int(npc.group(6)) if npc else 0,
        "cover_taken": sum(1 for ln in lines if COVER.search(ln)),
        "flee_lines": sum(1 for ln in lines if FLEES.search(ln)),
        "target_switches": switches,
        "combat_lines_per_minute": {k: round(v / minutes, 1) for k, v in sorted(lines_by.items())},
        "melee_spacing": {"swings": len(swings), "median": statistics.median(swings) if swings else None,
                          "closest": min(swings) if swings else None},
        "ragdolls_started": int(rag.group(1)) if rag else 0,
        "stuck_actors": len(stuck),
        "panics": sum(1 for ln in lines if PANIC.search(ln)),
        "system_times": timings,
    }


def run(ctx: lib.Ctx, per_side: int, third: int, seconds: float, every: float, centre, gap: float) -> dict:
    aid, player = layout(per_side, third, centre, gap)
    args = ["--cell", "Goodsprings", f"--pos={player[0]:g},{player[1]:g},9000", "--heading", "0", "--hour", "12"]
    launch = lib.Launch(f"battle-{per_side}", args, {"NVF_PLAY_TEST_BATTLE": aid, "NVF_PLAY_TEST_SAMPLE": f"{every:g}"},
                        run_seconds=seconds, progress=PROGRESS, hang_seconds=120, memory_every=every)
    r = lib.run(ctx, launch)
    samples = tour.tag_laps(r.lines)
    tour.add_memory(samples, r.memory)
    tour.write_csv(ctx.out / "samples.csv", samples)
    frames = next((tour.FRAMES.search(ln) for ln in reversed(r.lines) if tour.FRAMES.search(ln)), None)
    peak = lambda k: max((s[k] for s in samples if k in s), default=None)  # noqa: E731
    return {
        "test": "battle",
        "per_side": per_side,
        "third": third,
        "seconds": seconds,
        "status": r.status,
        "frames": None if not frames else {"count": int(frames.group(1)), "mean_ms": float(frames.group(2)),
                                           "low1_ms": float(frames.group(3)), "low1_fps": float(frames.group(4)),
                                           "worst_ms": float(frames.group(5))},
        "peak": {k: peak(k) for k in ("working set", "private bytes", "projectiles", "ragdolls", "actors", "nan actors", "entities")},
        "combat": analyse(r.lines),
        "crash_groups": lib.group_crashes([("battle", c) for c in r.crashes]),
        "log": str(r.log),
        "tail": r.tail(15) if r.status != "ok" else [],
    }


def report(res: dict) -> list[str]:
    c = res["combat"]
    out = ["# Stress test: battle", "",
           f"{res['per_side']} a side" + (f", {res['third']} in a third group" if res["third"] else "")
           + f", {res['seconds']:g} s: status {res['status']}.", ""]
    if res["frames"]:
        f = res["frames"]
        out.append(f"Frames: mean {f['mean_ms']} ms, 1% low {f['low1_ms']} ms ({f['low1_fps']} fps), worst {f['worst_ms']} ms over {f['count']} frames.")
    p = res["peak"]
    out += ["", "Peaks: " + ", ".join(f"{k} {lib.mb(v) if 'set' in k or 'bytes' in k else f'{v:g}'}" for k, v in p.items() if v is not None), ""]
    out += ["## Sides", "", "| faction | actors | standing | dead | hits | kills | flights |", "|---|---|---|---|---|---|---|"]
    out += [f"| {s['faction']} | {s['actors']} | {s['standing']} | {s['dead']} | {s['hits']} | {s['kills']} | {s['flights']} |" for s in c["sides"]]
    out += ["", "## Combat", "",
            f"- placed: " + ", ".join(f"{g['base']} {g['placed']}/{g['asked']}" for g in c["placed"]),
            f"- time to kill (from the placing): {c['time_to_kill_s']}",
            f"- shots {c['shots']}, actor hits {c['actor_hits']} (hit rate {c['hit_rate']}), blows {c['blows']}, reloads {c['reloads']}",
            f"- cover taken {c['cover_taken']}, flee checks {c['flee_checks']}, fled {c['fled']}, target switches {c['target_switches']}",
            f"- combat lines a minute: {c['combat_lines_per_minute']}",
            f"- melee spacing (swing distance, feet to feet): {c['melee_spacing']}",
            f"- ragdolls started {c['ragdolls_started']}, stuck actors {c['stuck_actors']}, panics {c['panics']}",
            "", "## Time per frame by system (the engine's own summaries)", ""]
    out += [f"- {k}: {v}" for k, v in sorted(c["system_times"].items())] or ["- none reported"]
    if res["crash_groups"]:
        out += ["", "## Crashes", ""] + [f"- {g['message']} at {g['location']}" for g in res["crash_groups"]]
    if res["tail"]:
        out += ["", "## Last log lines", "", "```", *res["tail"], "```"]
    return out
