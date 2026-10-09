#!/usr/bin/env python3
"""Generate Layer 2 object-frame scenarios (image comparator) from in-game screenshots.

Layer 2 frames look at single placed objects up close (a revolver on a bar, bottles,
a building whose mesh exists in several archives) to catch asset-layer errors:
orientation, texture flips, missing alpha, the wrong archive's copy of a file. They work
like the Layer 3 frames (gen_layer3_frames.py, tools/capture/README.md section 7) but
have their own manifest so the two sets stay apart, and a frame may name an interior
cell. Everything written here is private and goes into the goldens directory:

  <goldens>/frames_l2/manifest.json        written by capture_frames.py manifest (or by hand)
  <goldens>/frames_l2/<image>              the in-game screenshots (BMP or PNG)
  <goldens>/frames_l2/<id>.frame.json      oracle documents (written here)
  <goldens>/frames_l2/scenarios/layer2_frames.json   scenarios (written here)

Add "<goldens>/frames_l2/scenarios" to "scenarios_dirs" in lab.local.json to run them.

Manifest: as gen_layer3_frames.py, plus an optional "cell" per frame (an interior cell's
Editor ID; the camera is then placed in that cell and "world", "hour" and "weather" are
not passed). Paths in the oracle documents are relative to the goldens directory.

usage: gen_layer2_frames.py <goldens dir>
"""
import json
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from gen_layer3_frames import image_size  # noqa: E402

SUBDIR = "frames_l2"


def scenario(fr, eye, min_ssim, size):
    fid = fr["id"]
    w, h = size
    x, y, z = fr["player_pos"]
    frame = f"{{lab_root}}/runs/frames_l2/{fid}"
    argv = ["{engine_candidate}", "--data", "{data_dir}"]
    if fr.get("cell"):
        argv += ["--cell", fr["cell"]]
    else:
        argv += ["--world", fr.get("world", "WastelandNV"), f"--hour={fr.get('hour', 12.0)}"]
    argv += [f"--pos={x},{y},{z}", f"--eye-height={eye}", f"--heading={fr['heading']}",
             f"--pitch={-float(fr['pitch'])}", f"--fov={fr.get('fov', 75)}", f"--size={w}x{h}",
             "--screenshot", f"{frame}.png", "--frame-json", f"{frame}.json"]
    if fr.get("weather") and not fr.get("cell"):
        argv += ["--weather", fr["weather"]]
    s = {"id": f"L2.frame.{fid}", "layer": 2,
         "description": f"Rendered object frame vs the original game at camera '{fid}'"
                        + (f" ({fr['about']})." if fr.get("about") else "."),
         "oracle": {"golden": f"{SUBDIR}/{fid}.frame.json"},
         "candidate": {"argv": argv, "output_file": f"{frame}.json", "timeout_s": 300},
         "comparator": {"kind": "image", "min_ssim": float(fr.get("min_ssim", min_ssim))},
         "tags": ["frame", "render", "asset", "M2"]}
    if fr.get("holdout"):
        s["holdout"] = True
    return s


def main(argv):
    if len(argv) != 2:
        print(__doc__, file=sys.stderr)
        return 2
    fdir = os.path.join(argv[1], SUBDIR)
    with open(os.path.join(fdir, "manifest.json"), encoding="utf-8") as f:
        man = json.load(f)
    eye = float(man.get("eye_height", 120.0))
    min_ssim = float(man.get("min_ssim", 0.6))
    out = []
    for fr in man["frames"]:
        size = image_size(os.path.join(fdir, fr["image"]))
        with open(os.path.join(fdir, f"{fr['id']}.frame.json"), "w", encoding="utf-8") as f:
            json.dump({"schema": "fnv.frame/1", "image": f"{SUBDIR}/{fr['image']}",
                       "masks": fr.get("masks", [])}, f, indent=1)
        out.append(scenario(fr, eye, min_ssim, size))
    sdir = os.path.join(fdir, "scenarios")
    os.makedirs(sdir, exist_ok=True)
    with open(os.path.join(sdir, "layer2_frames.json"), "w", encoding="utf-8") as f:
        json.dump({"scenarios": out}, f, indent=1)
    print(f"{len(out)} frame scenarios -> {sdir}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
