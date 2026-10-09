#!/usr/bin/env python3
"""Reference frames from the original game for the Layer 3 image scenarios.

The capture plugin (tools/capture/nvf_capture, version 7) takes screenshots of the game
window (`screenshot` steps, docs/spec-notes/xnvse-capture.md section 4). This script
turns a list of cameras into a capture job that places the player at each camera, sets
the hour and weather, hides the HUD and takes the picture, and turns the plugin's output
into the frames manifest that parity-lab/tools/gen_layer3_frames.py reads. The lab then
renders the same cameras with `nvforever` and compares (README section 7).

  capture_frames.py job <cameras.json> [--data <Data>] [--name NAME] [-o job.steps.json]
      cameras -> fnv.capture.steps/1 job. --data (read-only) resolves weather Editor IDs
      to FormIDs and fills a camera's missing z with the terrain height there.
  capture_frames.py manifest <out.jsonl> --cameras <cameras.json> --goldens <dir>
                             [--capture-dir DIR]
      plugin output -> <goldens>/frames/<id>.png and entries in
      <goldens>/frames/manifest.json. The position, angles and hour are the ones the game
      reported just after the picture, not the requested ones. Existing entries keep their
      masks, min_ssim and holdout; other entries are left alone.

Cameras (`fnv.capture.cameras/1`; positions are coordinates we choose, not game data):
{
  "format": "fnv.capture.cameras/1",
  "start": "Load a save outdoors.",
  "console_first": ["tgm", "tdetect"],
  "cameras": [
    {"id": "goodsprings_noon", "world": "WastelandNV", "pos": [-70500, 1200],
     "heading": 45, "pitch": 0, "hour": 12, "weather": "NVWastelandClear", "fov": 75}
  ]
}
pos is the player's feet [x, y] or [x, y, z]; heading is GetAngle Z (degrees, 0 = north,
clockwise), pitch GetAngle X (positive looks down). weather (optional) is an Editor ID
(needs --data) or a FormID "0x...". console_first (optional) runs once before the first
camera: the trial's lake camera without `tgm` showed the player killed by a creature
[observed 2026-10-09]. Each camera forces the first-person view with
`DisablePlayerControls 0 0 0 1 0 0 0` (the fourth flag, POV: "if currently in 3rd person,
the view is forced into first person" [documented: GECK wiki DisablePlayerControls]),
records IsPC1stPerson with the picture and gives the controls back afterwards.

Travel follows what worked in the swimming captures [observed 2026-10-08, physics.md
6.6]: the console's `cow` loads the camera's own grid square first and only a short
SetPos follows (a long SetPos alone left the land drawn as distant LOD), then
CloseAllMenus. Standard library only. Exit codes: 0 ok, 1 bad input.
"""
import argparse
import json
import math
import os
import shutil
import sys

CAPTURE_DIR = os.path.dirname(os.path.abspath(__file__))
if CAPTURE_DIR not in sys.path:
    sys.path.insert(0, CAPTURE_DIR)

import capture_steps as cs  # noqa: E402

CAMERAS_FORMAT = "fnv.capture.cameras/1"
CELL = 4096  # exterior grid square, world units (world.md)
LAND_STEP = 128  # spacing of the 33 x 33 LAND heights, world units (world.md)
# Game loops after `fw` before the picture: weather changes blend in over a few seconds
# [hypothesis; the first frames will show whether 10 s at 60 loops per second is enough].
WEATHER_FRAMES = 600
# Game loops after `tm` (HUD off) before the picture, and after the player is placed again.
HUD_FRAMES = 30
PLACE_FRAMES = 60
# Feet above the terrain when z comes from --data, so the player is not placed into the
# ground; the character then settles onto it.
FEET_CLEARANCE = 8.0
# Fields recorded per camera, in this order, with the expressions that read them.
RECORDED = [("x", "player.GetPos X"), ("y", "player.GetPos Y"), ("z", "player.GetPos Z"),
            ("heading", "player.GetAngle Z"), ("pitch", "player.GetAngle X"), ("hour", "GameHour"),
            ("first_person", "IsPC1stPerson")]
# DisablePlayerControls flags: movement, Pip-Boy, fighting, POV, looking, rollover text,
# sneaking [documented: GECK wiki DisablePlayerControls]; only POV is set.
FIRST_PERSON = "DisablePlayerControls 0 0 0 1 0 0 0"
MANIFEST_DEFAULTS = {"eye_height": 120, "min_ssim": 0.6}


class InputError(Exception):
    pass


def num(expr, label, field):
    return cs.query(cs.function(["float fV"], [f"let fV := {expr}", "SetFunctionValue fV"]), label, field)


def script(*lines):
    return {"script": cs.function([], list(lines), returns_value=False)}


def place(x, y, z, heading, pitch):
    return script(f"player.SetPos X {cs.number(x)}", f"player.SetPos Y {cs.number(y)}", f"player.SetPos Z {cs.number(z)}",
                  f"player.SetAngle Z {cs.number(heading)}", f"player.SetAngle X {cs.number(pitch)}")


def in_square_until(gx, gy):
    """1 once the player stands in exterior grid square (gx, gy)."""
    x0, y0 = gx * CELL, gy * CELL
    return cs.function(["float fV"], [f"if player.GetPos X >= {x0}", f"  if player.GetPos X < {x0 + CELL}",
                                      f"    if player.GetPos Y >= {y0}", f"      if player.GetPos Y < {y0 + CELL}",
                                      "        let fV := 1", "      endif", "    endif", "  endif", "endif",
                                      "SetFunctionValue fV"])


def check_id(cid):
    if not isinstance(cid, str) or not cid or not all(c.isalnum() or c in "_-" for c in cid):
        raise InputError(f"camera id {cid!r} must be non-empty [A-Za-z0-9_-]")
    return cid


def weather_formid(weather, edids):
    if weather is None:
        return None
    if not isinstance(weather, str) or not weather:
        raise InputError(f"bad weather {weather!r}")
    if weather[:2].lower() == "0x":
        return f"{int(weather, 16):08X}"
    if edids is None:
        raise InputError(f"weather {weather!r} is an Editor ID: give --data to resolve it, or a FormID 0x...")
    fid = edids.get(weather.lower())
    if fid is None:
        raise InputError(f"weather {weather!r}: no such Editor ID in the masters")
    return f"{fid:08X}"


class Terrain:
    """Terrain height under a point, from the masters' LAND records (read-only)."""

    def __init__(self, data_dir, masters=cs.MASTERS):
        import oracle_world
        self.ow = oracle_world
        self.diag = oracle_world.Diagnostics()
        self.index, err = oracle_world.load(data_dir, masters, self.diag)
        if err:
            raise InputError(err)
        self.grids = {}

    def _grid(self, world):
        if world not in self.grids:
            gid, err = self.index.resolve_world(world)
            if err:
                raise InputError(err)
            cells = {}
            for cell in self.ow.select_cells(self.index, gid):
                land = self.index.cell_land.get(cell)
                if land is None:
                    continue
                _, _, subs = self.index.subrecords(cell, (b"XCLC",))
                xclc = subs.get(b"XCLC")
                if xclc is not None and len(xclc) in (8, 12):
                    cells[tuple(self.ow._XCLC.unpack_from(xclc))] = land[1]
            self.grids[world] = cells
        return self.grids[world]

    def height(self, world, x, y):
        import oracle_phys
        gx, gy = math.floor(x / CELL), math.floor(y / CELL)
        land = self._grid(world).get((gx, gy))
        if land is None:
            raise InputError(f"{world} has no LAND at grid ({gx}, {gy}); give the camera a z")
        _, _, subs = self.index.subrecords(land, (b"VHGT",))
        vhgt = subs.get(b"VHGT")
        if vhgt is None or len(vhgt) != self.ow.VHGT_SIZE:
            raise InputError(f"{world} grid ({gx}, {gy}) has no heights; give the camera a z")
        h = self.ow.integrate_vhgt(vhgt)
        z = oracle_phys.height_at(h, (x - gx * CELL) / LAND_STEP, (y - gy * CELL) / LAND_STEP)
        if z is None:
            raise InputError(f"no terrain height at ({x}, {y})")
        return z


def camera_steps(cam, edids, terrain):
    cid = check_id(cam.get("id"))
    world = cam.get("world", "WastelandNV")
    cs.plain_name(world, f"camera {cid}: world")
    pos = cam.get("pos")
    if not isinstance(pos, list) or len(pos) not in (2, 3):
        raise InputError(f"camera {cid}: pos must be [x, y] or [x, y, z]")
    x, y = float(pos[0]), float(pos[1])
    if len(pos) == 3:
        z = float(pos[2])
    elif terrain is not None:
        # Whole units: the swimming jobs' SetPos only ever used whole numbers [observed].
        z = float(math.ceil(terrain.height(world, x, y) + FEET_CLEARANCE))
    else:
        raise InputError(f"camera {cid}: pos has no z; give one or --data to use the terrain height")
    heading, pitch = float(cam.get("heading", 0.0)), float(cam.get("pitch", 0.0))
    gx, gy = math.floor(x / CELL), math.floor(y / CELL)
    steps = [{"console": f"cow {world} {gx} {gy}"}, {"until": in_square_until(gx, gy), "max_frames": cs.DOOR_LOOPS},
             {"frames": cs.AFTER_DOOR_FRAMES}, {"console": "CloseAllMenus"}, {"frames": PLACE_FRAMES},
             script(FIRST_PERSON), place(x, y, z, heading, pitch)]
    if cam.get("hour") is not None:
        # GameHour is set at the save's TimeScale; only moving the clock at a low TimeScale
        # crashed the game (xnvse-capture.md section 5).
        steps.append({"console": f"set GameHour to {cs.number(cam['hour'])}"})
    fw = weather_formid(cam.get("weather"), edids)
    if fw is not None:
        steps.append({"console": f"fw {fw}"})
    steps += [{"frames": WEATHER_FRAMES}, place(x, y, z, heading, pitch), {"console": "CloseAllMenus"},
              {"frames": PLACE_FRAMES}, {"console": "tm"}, {"frames": HUD_FRAMES}, {"screenshot": cid}]
    steps += [num(expr, cid, field) for field, expr in RECORDED]
    steps += [{"console": "tm"}, script("EnablePlayerControls"), {"frames": HUD_FRAMES}]
    return steps


def load_cameras(path):
    doc = cs.load_json(path)
    if doc.get("format") != CAMERAS_FORMAT:
        raise InputError(f"{path}: format must be {CAMERAS_FORMAT}")
    cams = doc.get("cameras")
    if not isinstance(cams, list) or not cams:
        raise InputError(f"{path}: no cameras")
    ids = [check_id(c.get("id")) for c in cams]
    if len(set(ids)) != len(ids):
        raise InputError(f"{path}: camera ids repeat")
    return doc


def frames_job(doc, name, edids=None, terrain=None):
    first = doc.get("console_first", [])
    if not isinstance(first, list) or not all(isinstance(c, str) and c for c in first):
        raise InputError("console_first must be a list of console lines")
    steps = [{"console": c} for c in first]
    for cam in doc["cameras"]:
        steps += camera_steps(cam, edids, terrain)
    start = doc.get("start") or "Load a save; the job travels to each camera."
    return cs.job(name, start, steps, cs.MASTERS)


def read_frames(lines):
    """{camera id: {"file", "width", "height", "mean", "foreground", <recorded fields>}}
    from the plugin's output; errors of a camera's steps are collected under "errors"."""
    shots, results, ended = {}, {}, False
    for n, line in enumerate(lines, 1):
        line = line.strip()
        if not line:
            continue
        try:
            rec = json.loads(line)
        except json.JSONDecodeError as e:
            raise InputError(f"line {n}: {e}") from e
        kind = rec.get("kind")
        if kind == "screenshot":
            shots[rec["label"]] = {k: rec.get(k) for k in ("file", "width", "height", "mean", "foreground")}
        elif kind == "result" and rec.get("label") != cs.MODINDEX_LABEL:
            results.setdefault(rec["label"], {})[rec["field"]] = rec.get("value")
        elif kind == "end":
            ended = True
        elif kind == "aborted":
            raise InputError("the job was aborted (a save was loaded while it ran)")
    if not ended:
        raise InputError("the output has no end line (job unfinished)")
    out = {}
    for cid, shot in shots.items():
        rec = dict(shot)
        rec.update(results.get(cid, {}))
        out[cid] = rec
    return out


def merge_manifest(manifest, cameras, frames):
    """Manifest with an entry per captured camera; returns (manifest, warnings)."""
    man = dict(MANIFEST_DEFAULTS)
    man.update(manifest or {})
    entries = {e["id"]: e for e in man.get("frames", [])}
    order = [e["id"] for e in man.get("frames", [])]
    warnings = []
    for cam in cameras["cameras"]:
        cid = cam["id"]
        fr = frames.get(cid)
        if fr is None:
            warnings.append(f"{cid}: no screenshot in the output")
            continue
        missing = [f for f, _ in RECORDED if f != "first_person" and not isinstance(fr.get(f), (int, float))]
        if missing:
            warnings.append(f"{cid}: not recorded: {', '.join(missing)}; left out")
            continue
        if fr.get("first_person") != 1:
            warnings.append(f"{cid}: not a first-person view (IsPC1stPerson {fr.get('first_person')})")
        if not fr.get("foreground"):
            warnings.append(f"{cid}: the game window was not in front; another window may be in the picture")
        if (fr.get("mean") or 0.0) < 0.02:
            warnings.append(f"{cid}: the picture is (nearly) black (mean {fr.get('mean')})")
        e = dict(entries.get(cid, {}))
        e.update({"id": cid, "image": f"{cid}.png", "player_pos": [fr["x"], fr["y"], fr["z"]],
                  "heading": fr["heading"], "pitch": fr["pitch"], "hour": round(fr["hour"], 4),
                  "world": cam.get("world", "WastelandNV"), "fov": cam.get("fov", 75)})
        if cam.get("weather"):
            e["weather"] = cam["weather"]
        else:
            e.pop("weather", None)
        e.setdefault("masks", [])
        if cid not in entries:
            order.append(cid)
        entries[cid] = e
    man["frames"] = [entries[i] for i in order]
    return man, warnings


def write_manifest(out_path, cameras, goldens, capture_dir):
    with open(out_path, encoding="utf-8") as f:
        frames = read_frames(f)
    fdir = os.path.join(goldens, "frames")
    os.makedirs(fdir, exist_ok=True)
    mpath = os.path.join(fdir, "manifest.json")
    old = cs.load_json(mpath) if os.path.exists(mpath) else None
    man, warnings = merge_manifest(old, cameras, frames)
    for e in man["frames"]:
        fr = frames.get(e["id"])
        if fr is not None:
            shutil.copyfile(os.path.join(capture_dir, fr["file"]), os.path.join(fdir, e["image"]))
    cs.write(man, mpath)
    return man, warnings


def main(argv=None):
    ap = argparse.ArgumentParser(prog="capture_frames.py", description=__doc__.split("\n\n")[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    j = sub.add_parser("job")
    j.add_argument("cameras")
    j.add_argument("--data", help="game Data folder (read-only): weather Editor IDs, terrain heights")
    j.add_argument("--name")
    j.add_argument("-o", "--output")
    m = sub.add_parser("manifest")
    m.add_argument("output_jsonl")
    m.add_argument("--cameras", required=True)
    m.add_argument("--goldens", required=True, help="private goldens directory")
    m.add_argument("--capture-dir", default=None,
                   help="capture directory the plugin wrote to (default: the output file's parent's parent)")
    args = ap.parse_args(argv)
    try:
        cams = load_cameras(args.cameras)
        if args.cmd == "job":
            name = args.name or os.path.basename(args.cameras).split(".")[0]
            edids = cs.edid_index(args.data) if args.data else None
            terrain = Terrain(args.data) if args.data else None
            cs.write(frames_job(cams, name, edids, terrain), args.output)
        else:
            cdir = args.capture_dir or os.path.dirname(os.path.dirname(os.path.abspath(args.output_jsonl)))
            man, warnings = write_manifest(args.output_jsonl, cams, args.goldens, cdir)
            for w in warnings:
                print(f"capture_frames: warning: {w}", file=sys.stderr)
            print(f"{len(man['frames'])} frame(s) in {os.path.join(args.goldens, 'frames', 'manifest.json')}")
    except (OSError, json.JSONDecodeError, InputError, cs.InputError, KeyError, ValueError) as e:
        print(f"capture_frames: {e}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
