"""Read a first-person motion trace (NVF_PLAY_TEST_MOTION_TRACE, engine/nvforever/src/player_body.rs
`MotionTrace`) and report what looks unsmooth: frame-time spikes, jerks in the clips' hand motion,
in the sway and in the eye, per time window.

    python tools/smoke/motion_trace.py target/smoke/movement-inertia-trace.csv [--window A:B:name ...]

Jerk is the change of acceleration per frame; a hitch shows as a jerk far above the window's
median (a velocity that kinks). Numbers only: no game data is read or written.
"""
from __future__ import annotations

import argparse
import csv
import math
import statistics
import sys
from pathlib import Path

DEFAULT_WINDOWS = [
    (6.0, 8.0, "draw"),
    (8.5, 11.5, "idle, weapon out"),
    (11.8, 14.0, "aim in"),
    (14.0, 16.0, "aim out"),
    (19.8, 21.8, "flick right"),
    (21.8, 23.8, "flick back"),
    (23.8, 27.0, "run"),
    (27.0, 29.0, "stop"),
    (28.8, 31.0, "crouch"),
    (31.0, 33.0, "crouched jump"),
    (33.0, 35.0, "stand"),
    (35.0, 37.0, "jump"),
    (37.0, 40.0, "holster"),
]


def rows(path: Path) -> list[dict[str, float]]:
    with path.open(newline="") as f:
        return [{k: float(v) for k, v in r.items()} for r in csv.DictReader(f)]


def derivative(t: list[float], v: list[float]) -> list[float]:
    out = [0.0]
    for i in range(1, len(v)):
        dt = t[i] - t[i - 1]
        out.append((v[i] - v[i - 1]) / dt if dt > 1e-6 else 0.0)
    return out


def report(name: str, rs: list[dict[str, float]], channels: list[str]) -> list[str]:
    lines = []
    if len(rs) < 8:
        return [f"{name}: too few frames ({len(rs)})"]
    t = [r["t"] for r in rs]
    dts = [r["dt"] for r in rs]
    med_dt = statistics.median(dts)
    spikes = [(r["t"], r["dt"]) for r in rs if r["dt"] > 2.0 * med_dt]
    lines.append(f"{name}: {len(rs)} frames, median frame {med_dt * 1000:.1f} ms, {len(spikes)} frame spikes"
                 + (f" (worst {max(s[1] for s in spikes) * 1000:.1f} ms at {max(spikes, key=lambda s: s[1])[0]:.2f}s)" if spikes else ""))
    for ch in channels:
        v = [r[ch] for r in rs]
        if any(math.isnan(x) for x in v):
            continue
        vel = derivative(t, v)
        acc = derivative(t, vel)
        jerk = [abs(a - b) for a, b in zip(acc[2:], acc[1:-1])]
        if not jerk:
            continue
        travel = max(v) - min(v)
        med = statistics.median(jerk) or 1e-9
        worst_i = max(range(len(jerk)), key=jerk.__getitem__)
        ratio = jerk[worst_i] / med
        reversals = sum(1 for a, b in zip(acc[2:], acc[3:]) if a * b < 0 and abs(a) > 1e-3 and abs(b) > 1e-3)
        flag = "  <-- hitch?" if ratio > 25 and travel > 0.05 else ""
        lines.append(f"  {ch:10s} travel {travel:8.4f}  peak speed {max(map(abs, vel[1:])):9.4f}/s  "
                      f"accel reversals {reversals:3d}  worst jerk {ratio:6.1f}x median at {t[worst_i + 2]:.2f}s{flag}")
    return lines


def main(argv: list[str]) -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("trace", type=Path)
    p.add_argument("--window", action="append", default=[], help="A:B:name (seconds of the test aids' clock)")
    p.add_argument("--lead", type=float, default=0.0,
                   help="seconds the test aids' clock ran before the trace's first line (default: 0)")
    a = p.parse_args(argv)
    rs = rows(a.trace)
    if not rs:
        print("empty trace")
        return 1
    t0 = rs[0]["t"]
    for r in rs:
        r["t"] -= t0 - a.lead
    windows = [(float(x), float(y), n) for x, y, n in (w.split(":", 2) for w in a.window)] or DEFAULT_WINDOWS
    channels = ["hand_x", "hand_y", "hand_z", "hand_rx", "hand_ry", "hand_rz", "sway_x", "sway_y", "sway_z", "sway_yaw", "sway_pitch", "sway_roll", "eye_z"]
    for lo, hi, name in windows:
        print("\n".join(report(f"{name} [{lo:g}-{hi:g}s]", [r for r in rs if lo <= r["t"] < hi], channels)))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
