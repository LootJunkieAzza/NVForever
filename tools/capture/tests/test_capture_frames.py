"""Tests for tools/capture/capture_frames.py (reference-frame jobs, manifest from the
plugin's output). Synthetic data only."""
import json
import os
import sys
import tempfile
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))

import capture_frames as cf  # noqa: E402
import capture_steps as cs  # noqa: E402

CAMERAS = {"format": cf.CAMERAS_FORMAT, "start": "Load a save.", "console_first": ["tgm"],
           "cameras": [{"id": "gs_noon", "pos": [-70500, 1200, 8300], "heading": 45, "pitch": -5, "hour": 12,
                        "weather": "NVWastelandClear", "fov": 75},
                       {"id": "lake", "pos": [55500.5, 37536, 2960], "heading": 90}]}
EDIDS = {"nvwastelandclear": 0x000FFC88}


def out_lines(skip=(), stop_early=False):
    lines = [{"kind": "start", "job": "fr"},
             {"kind": "result", "step": 0, "label": cs.MODINDEX_LABEL, "field": "FalloutNV.esm", "value": 0}]
    for cid, mean, fg in (("gs_noon", 0.41, True), ("lake", 0.005, False)):
        if cid in skip:
            continue
        lines.append({"kind": "screenshot", "step": 9, "label": cid, "file": f"shots/fr.009.{cid}.1.png",
                      "width": 4, "height": 2, "mean": mean, "foreground": fg})
        for f, v in (("x", 1.5), ("y", 2.0), ("z", 3.0), ("heading", 45.0), ("pitch", -5.0), ("hour", 12.123456),
                     ("first_person", 1.0 if cid == "gs_noon" else 0.0)):
            lines.append({"kind": "result", "step": 10, "label": cid, "field": f, "value": v})
    if not stop_early:
        lines.append({"kind": "end", "steps": 30})
    return [json.dumps(x) for x in lines]


class JobTests(unittest.TestCase):
    def test_steps_per_camera(self):
        job = cf.frames_job(CAMERAS, "fr", EDIDS)
        self.assertEqual(job["format"], cs.STEPS_FORMAT)
        steps = job["steps"][len(cs.MASTERS):]
        consoles = [s["console"] for s in steps if "console" in s]
        self.assertEqual(consoles[:6], ["tgm", "cow WastelandNV -18 0", "CloseAllMenus", "set GameHour to 12", "fw 000FFC88",
                                        "CloseAllMenus"])
        self.assertEqual(consoles.count("tm"), 4, "HUD off and on again for each camera")
        shots = [s["screenshot"] for s in steps if "screenshot" in s]
        self.assertEqual(shots, ["gs_noon", "lake"])
        i = next(k for k, s in enumerate(steps) if s.get("screenshot") == "gs_noon")
        self.assertEqual(steps[i - 2], {"console": "tm"})
        self.assertEqual([s["field"] for s in steps[i + 1:i + 8]], ["x", "y", "z", "heading", "pitch", "hour", "first_person"])
        self.assertTrue(all(s["label"] == "gs_noon" for s in steps[i + 1:i + 8]))
        text = json.dumps(steps)
        self.assertLess(text.index("DisablePlayerControls 0 0 0 1 0 0 0"), text.index('"screenshot"'))
        self.assertGreater(text.index("EnablePlayerControls"), text.index('"screenshot"'))
        self.assertIn("player.SetPos X 55500.5", json.dumps(job))
        self.assertIn("cow WastelandNV 13 9", consoles, "lake camera's own grid square")
        lake = json.dumps(steps[steps.index({"console": "cow WastelandNV 13 9"}):])
        self.assertNotIn("set GameHour", lake, "no hour asked, clock left alone")
        self.assertNotIn("fw ", lake)

    def test_bad_cameras(self):
        with self.assertRaises(cf.InputError):
            cf.frames_job({"cameras": [{"id": "a b", "pos": [0, 0, 0]}]}, "fr")
        with self.assertRaises(cf.InputError):
            cf.frames_job({"cameras": [{"id": "a", "pos": [0, 0]}]}, "fr")  # no z, no --data
        with self.assertRaises(cf.InputError):
            cf.frames_job({"cameras": [{"id": "a", "pos": [0, 0, 0], "weather": "Clear"}]}, "fr")  # edid without --data
        self.assertEqual(cf.weather_formid("0xffc88", None), "000FFC88")

    def test_missing_z_comes_from_the_terrain(self):
        class Flat:
            def height(self, world, x, y):
                return 100.25
        job = cf.frames_job({"cameras": [{"id": "a", "pos": [10, 20]}]}, "fr", terrain=Flat())
        self.assertIn("player.SetPos Z 109", json.dumps(job), "terrain + clearance, rounded up to whole units")


class ManifestTests(unittest.TestCase):
    def test_read_frames(self):
        fr = cf.read_frames(out_lines())
        self.assertEqual(fr["gs_noon"]["file"], "shots/fr.009.gs_noon.1.png")
        self.assertEqual(fr["gs_noon"]["hour"], 12.123456)
        with self.assertRaises(cf.InputError):
            cf.read_frames(out_lines(stop_early=True))

    def test_merge_keeps_owner_fields_and_warns(self):
        old = {"eye_height": 110, "frames": [
            {"id": "other", "image": "other.bmp", "player_pos": [0, 0, 0], "heading": 0, "pitch": 0},
            {"id": "gs_noon", "image": "old.bmp", "masks": [[0, 0, 10, 10]], "min_ssim": 0.7, "holdout": True}]}
        man, warnings = cf.merge_manifest(old, CAMERAS, cf.read_frames(out_lines()))
        self.assertEqual(man["eye_height"], 110)
        self.assertEqual(man["min_ssim"], 0.6)
        self.assertEqual([e["id"] for e in man["frames"]], ["other", "gs_noon", "lake"])
        gs = man["frames"][1]
        self.assertEqual(gs["image"], "gs_noon.png")
        self.assertEqual(gs["player_pos"], [1.5, 2.0, 3.0])
        self.assertEqual((gs["heading"], gs["pitch"], gs["hour"]), (45.0, -5.0, 12.1235))
        self.assertEqual((gs["masks"], gs["min_ssim"], gs["holdout"]), ([[0, 0, 10, 10]], 0.7, True))
        self.assertEqual(gs["weather"], "NVWastelandClear")
        self.assertNotIn("weather", man["frames"][2])
        self.assertEqual(len(warnings), 3, warnings)  # lake: third person, not in front, black

    def test_manifest_command_copies_pictures(self):
        with tempfile.TemporaryDirectory() as tmp:
            cap = os.path.join(tmp, "capture")
            os.makedirs(os.path.join(cap, "shots"))
            os.makedirs(os.path.join(cap, "out"))
            with open(os.path.join(cap, "shots", "fr.009.gs_noon.1.png"), "wb") as f:
                f.write(b"png")
            out = os.path.join(cap, "out", "fr.1.jsonl")
            with open(out, "w") as f:
                f.write("\n".join(out_lines(skip=("lake",))) + "\n")
            cams = os.path.join(tmp, "cameras.json")
            with open(cams, "w") as f:
                json.dump(CAMERAS, f)
            gold = os.path.join(tmp, "goldens")
            self.assertEqual(cf.main(["manifest", out, "--cameras", cams, "--goldens", gold]), 0)
            with open(os.path.join(gold, "frames", "gs_noon.png"), "rb") as f:
                self.assertEqual(f.read(), b"png")
            with open(os.path.join(gold, "frames", "manifest.json")) as f:
                self.assertEqual([e["id"] for e in json.load(f)["frames"]], ["gs_noon"])


if __name__ == "__main__":
    unittest.main()
