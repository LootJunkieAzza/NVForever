#!/usr/bin/env python3
"""Generate rendered-frame scenarios (Layer 3, image comparator) from the owner's captures.

Frames are private (screenshots of the original game), so everything this script writes
goes into the goldens directory, never into the repository:

  <goldens>/frames/manifest.json         written by the owner (format below)
  <goldens>/frames/<image>               the in-game screenshots (BMP or PNG)
  <goldens>/frames/<id>.frame.json       oracle documents (written here)
  <goldens>/frames/scenarios/layer3_frames.json   scenarios (written here)

Add "<goldens>/frames/scenarios" to "scenarios_dirs" in lab.local.json to run them.

Manifest:
{
  "eye_height": 120,                # added to player z for the camera [hypothesis]
  "min_ssim": 0.6,                  # provisional; frozen with the owner after the first runs
  "frames": [
    {"id": "goodsprings_noon", "image": "goodsprings_noon.bmp",
     "player_pos": [-71680.0, 2048.0, 8248.0], "heading": 10.0, "pitch": -5.0,
     "hour": 12.0, "weather": "NVWastelandClear", "fov": 75,
     "world": "WastelandNV", "masks": [[0, 0, 300, 60]], "holdout": false}
  ]
}
heading: the player's z angle in degrees (getangle z: 0 = north, clockwise);
pitch: the player's x angle in degrees as the game reports it (positive = looking down);
the candidate's pitch is the negation. masks: rectangles [x0, y0, x1, y1) in the
screenshot's pixels to ignore (HUD remnants, actors, particles).

usage: gen_layer3_frames.py <goldens dir>
"""
import json
import os
import struct
import sys


def image_size(path):
    with open(path, "rb") as f:
        head = f.read(32)
    if head[:8] == b"\x89PNG\r\n\x1a\n":
        return struct.unpack(">II", head[16:24])
    if head[:2] == b"BM":
        w, h = struct.unpack("<ii", head[18:26])
        return w, abs(h)
    raise ValueError(f"{path}: not a PNG or BMP")


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    gold = argv[1]
    fdir = os.path.join(gold, "frames")
    with open(os.path.join(fdir, "manifest.json"), encoding="utf-8") as f:
        man = json.load(f)
    eye = float(man.get("eye_height", 120.0))
    min_ssim = float(man.get("min_ssim", 0.6))
    out = []
    for fr in man["frames"]:
        fid = fr["id"]
        w, h = image_size(os.path.join(fdir, fr["image"]))
        with open(os.path.join(fdir, f"{fid}.frame.json"), "w", encoding="utf-8") as f:
            json.dump({"schema": "fnv.frame/1", "image": f"frames/{fr['image']}", "masks": fr.get("masks", [])}, f, indent=1)
        x, y, z = fr["player_pos"]
        frame = f"{{lab_root}}/runs/frames/{fid}"
        argv_c = ["{engine_candidate}", "--data", "{data_dir}",
                  "--world", fr.get("world", "WastelandNV"),
                  f"--pos={x},{y},{z}", f"--eye-height={eye}",
                  f"--heading={fr['heading']}", f"--pitch={-float(fr['pitch'])}",
                  f"--hour={fr.get('hour', 12.0)}", f"--fov={fr.get('fov', 75)}",
                  f"--size={w}x{h}", "--screenshot", f"{frame}.png", "--frame-json", f"{frame}.json"]
        if fr.get("weather"):
            argv_c += ["--weather", fr["weather"]]
        s = {"id": f"L3.frame.{fid}", "layer": 3,
             "description": f"Rendered frame vs the original game at camera '{fid}'.",
             "oracle": {"golden": f"frames/{fid}.frame.json"},
             "candidate": {"argv": argv_c, "output_file": f"{frame}.json", "timeout_s": 300},
             "comparator": {"kind": "image", "min_ssim": float(fr.get("min_ssim", min_ssim))},
             "tags": ["frame", "render", "M3"]}
        if fr.get("holdout"):
            s["holdout"] = True
        out.append(s)
    sdir = os.path.join(fdir, "scenarios")
    os.makedirs(sdir, exist_ok=True)
    with open(os.path.join(sdir, "layer3_frames.json"), "w", encoding="utf-8") as f:
        json.dump({"scenarios": out}, f, indent=1)
    print(f"{len(out)} frame scenarios -> {sdir}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
