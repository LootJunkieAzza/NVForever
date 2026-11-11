#!/usr/bin/env python3
"""Compare opt-in game captures with camera-only motion, using Python's standard library.

Capture images stay outside the source/release tree. Results are diagnostic: moving
actors, vertex deformation and first-person geometry differ from camera-only motion.
SPDX-License-Identifier: GPL-3.0-or-later
"""
import argparse
import json
import math
from pathlib import Path
import struct


def matrix(columns):
    return [[columns[c * 4 + r] for c in range(4)] for r in range(4)]


def multiply(a, b):
    return [sum(x * y for x, y in zip(row, b)) for row in a]


def inverse(a):
    rows = [list(row) + [float(i == j) for j in range(4)] for i, row in enumerate(a)]
    for c in range(4):
        pivot = max(range(c, 4), key=lambda r: abs(rows[r][c]))
        rows[c], rows[pivot] = rows[pivot], rows[c]
        scale = rows[c][c]
        if abs(scale) < 1e-15:
            raise ValueError("singular camera matrix")
        rows[c] = [v / scale for v in rows[c]]
        for r in range(4):
            if r != c:
                scale = rows[r][c]
                rows[r] = [x - scale * y for x, y in zip(rows[r], rows[c])]
    return [row[4:] for row in rows]


def percentile(sorted_values, q):
    if not sorted_values:
        return None
    at = (len(sorted_values) - 1) * q
    lo = int(at)
    hi = min(lo + 1, len(sorted_values) - 1)
    return sorted_values[lo] * (hi - at) + sorted_values[hi] * (at - lo) if lo != hi else sorted_values[lo]


def summary(values):
    values = sorted(values)
    return {
        "median": percentile(values, 0.5),
        "p90": percentile(values, 0.9),
        "p99": percentile(values, 0.99),
        "mean": sum(values) / len(values) if values else None,
    }


def pixel(raw, width, x, y):
    return struct.unpack_from("<4e", raw, (y * width + x) * 8)[:3]


def bilinear(raw, width, height, x, y):
    # x/y are centre-based raster coordinates, so texel (0,0) is at (0.5,0.5).
    x -= 0.5
    y -= 0.5
    if not (0 <= x < width - 1 and 0 <= y < height - 1):
        return None
    ix, iy = int(x), int(y)
    fx, fy = x - ix, y - iy
    p = [pixel(raw, width, ix, iy), pixel(raw, width, ix + 1, iy),
         pixel(raw, width, ix, iy + 1), pixel(raw, width, ix + 1, iy + 1)]
    return [p[0][c] * (1 - fx) * (1 - fy) + p[1][c] * fx * (1 - fy)
            + p[2][c] * (1 - fx) * fy + p[3][c] * fx * fy for c in range(3)]


def color_error(a, b):
    if b is None or not all(math.isfinite(c) for c in (*a, *b)):
        return None
    # Bounded HDR comparison without needing a particular game tonemapper.
    def compress(c):
        c = max(0, c)
        return c / (1 + c)
    return sum(abs(compress(x) - compress(y)) for x, y in zip(a, b)) / 3


def image_stats(raw, size):
    assert len(raw) == size[0] * size[1] * 8, "incomplete RGBA16F image"
    nonfinite = 0
    nonblack = 0
    total = [0.0, 0.0, 0.0]
    maximum = 0.0
    for rgba in struct.iter_unpack("<4e", raw):
        if not all(math.isfinite(c) for c in rgba):
            nonfinite += 1
            continue
        nonblack += max(rgba[:3]) > 1e-6
        maximum = max(maximum, *rgba[:3])
        for c in range(3):
            total[c] += rgba[c]
    count = size[0] * size[1]
    return {"nonfinite_pixels": nonfinite, "nonblack_fraction": nonblack / count,
            "mean_rgb": [v / count for v in total], "maximum_rgb": maximum}


def analyze(path, args, previous):
    meta = json.loads(path.read_text())
    width, height = meta["render_size"]
    prefix = path.with_suffix("")
    packed = prefix.with_suffix(".inputs.f32").read_bytes()
    color = prefix.with_suffix(".color.rgba16f").read_bytes()
    output = prefix.with_suffix(".output.rgba16f").read_bytes()
    assert len(packed) == width * height * 16, f"incomplete inputs: {path}"
    assert len(color) == width * height * 8, f"incomplete color: {path}"
    if meta.get("previous_clip_from_world") is None:
        return {"frame": meta["frame"], "skipped": "no previous matrix"}, (meta, color)
    current_inverse = inverse(matrix(meta["current_clip_from_world"]))
    previous_matrix = matrix(meta["previous_clip_from_world"])
    jitter = meta["jitter"]
    errors, reversed_errors, absent_errors, magnitudes = [], [], [], []
    delta_x, delta_y = [], []
    photo = {"captured_motion": [], "camera_motion": [], "reversed_motion": [], "zero_motion": []}
    prior = previous if previous and previous[0]["frame"] + 1 == meta["frame"] else None
    if prior and prior[0]["render_size"] != meta["render_size"]:
        prior = None
    for y in range(2, height - 2, args.step):
        for x in range(2, width - 2, args.step):
            depth, mx, my, reactive = struct.unpack_from("<4f", packed, (y * width + x) * 16)
            if not all(math.isfinite(v) for v in (depth, mx, my, reactive)):
                continue
            if not (1e-7 < depth < args.max_depth) or reactive > args.max_reactive:
                continue
            if 0.3 * width < x < 0.7 * width and y > 0.65 * height:
                continue  # conservative first-person/weapon exclusion
            neighbors = [struct.unpack_from("<f", packed, ((y + dy) * width + x + dx) * 16)[0]
                         for dx, dy in [(-1, 0), (1, 0), (0, -1), (0, 1)]]
            if any(abs(d - depth) > depth * 0.03 for d in neighbors):
                continue  # reject silhouettes and depth discontinuities
            uv = [(x + 0.5 - jitter[0]) / width, (y + 0.5 - jitter[1]) / height]
            world = multiply(current_inverse, [uv[0] * 2 - 1, 1 - uv[1] * 2, depth, 1])
            old = multiply(previous_matrix, world)
            if old[3] <= 0:
                continue
            old_uv = [old[0] / old[3] * 0.5 + 0.5, 0.5 - old[1] / old[3] * 0.5]
            expected = [uv[0] - old_uv[0], uv[1] - old_uv[1]]
            dx, dy = (mx - expected[0]) * width, (my - expected[1]) * height
            errors.append(math.hypot(dx, dy))
            reversed_errors.append(math.hypot((mx + expected[0]) * width, (my + expected[1]) * height))
            absent_errors.append(math.hypot(expected[0] * width, expected[1] * height))
            magnitudes.append(math.hypot(mx * width, my * height))
            delta_x.append(dx)
            delta_y.append(dy)
            if prior:
                pmeta, pcolor = prior
                pj = pmeta["jitter"]
                here = pixel(color, width, x, y)
                for name, vector in [("captured_motion", [mx, my]), ("camera_motion", expected),
                                     ("reversed_motion", [-mx, -my]), ("zero_motion", [0, 0])]:
                    old_color = bilinear(pcolor, width, height,
                                         (uv[0] - vector[0]) * width + pj[0],
                                         (uv[1] - vector[1]) * height + pj[1])
                    error = color_error(here, old_color)
                    if error is not None:
                        photo[name].append(error)
    result = {
        "frame": meta["frame"], "reset": meta["reset"], "samples": len(errors),
        "render_size": meta["render_size"], "units": "render pixels",
        "motion_error": summary(errors), "wrong_sign_error": summary(reversed_errors),
        "zero_motion_error": summary(absent_errors), "captured_motion_magnitude": summary(magnitudes),
        "motion_bias_xy": [summary(delta_x)["median"], summary(delta_y)["median"]],
        "fraction_within_quarter_pixel": sum(e < 0.25 for e in errors) / len(errors) if errors else None,
        "input_color": image_stats(color, meta["render_size"]),
        "output_color": image_stats(output, meta["output_size"]),
        "photometric_error": {name: summary(values) for name, values in photo.items()} if prior else None,
    }
    return result, (meta, color)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("folder", type=Path)
    parser.add_argument("--step", type=int, default=4)
    parser.add_argument("--max-depth", type=float, default=0.05, help="exclude close/first-person geometry")
    parser.add_argument("--max-reactive", type=float, default=0.05)
    parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    if args.step < 1:
        parser.error("--step must be positive")
    paths = sorted(args.folder.glob("frame-*.json"), key=lambda p: int(p.stem.split("-")[-1]))
    if not paths:
        parser.error("no frame metadata found")
    results, previous = [], None
    for path in paths:
        result, previous = analyze(path, args, previous)
        results.append(result)
    report = {"filters": {"step": args.step, "max_depth": args.max_depth, "max_reactive": args.max_reactive}, "frames": results}
    text = json.dumps(report, indent=2, allow_nan=False)
    if args.output:
        args.output.write_text(text + "\n")
    print(text)


if __name__ == "__main__":
    main()
