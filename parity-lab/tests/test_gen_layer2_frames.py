"""Self-tests for tools/gen_layer2_frames.py (synthetic manifest and image only)."""
import json
import os
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))
sys.path.insert(0, os.path.join(os.path.dirname(__file__), "..", "tools"))

from fnvlab import image as im  # noqa: E402
import gen_layer2_frames as g2  # noqa: E402


class GenLayer2Frames(unittest.TestCase):
    def test_exterior_and_interior(self):
        with tempfile.TemporaryDirectory() as gold:
            fdir = os.path.join(gold, "frames_l2")
            os.makedirs(fdir)
            png = im.encode_png(im.Image(4, 2, [(0, 0, 0)] * 8))
            for n in ("a.png", "b.png"):
                with open(os.path.join(fdir, n), "wb") as f:
                    f.write(png)
            man = {"eye_height": 100, "min_ssim": 0.5, "frames": [
                {"id": "ext", "image": "a.png", "player_pos": [1.0, 2.0, 3.0], "heading": 90.0, "pitch": 5.0,
                 "hour": 12.0, "weather": "W", "world": "WastelandNV", "masks": [[0, 0, 1, 1]]},
                {"id": "int", "image": "b.png", "player_pos": [4.0, 5.0, 6.0], "heading": 0.0, "pitch": -2.0,
                 "cell": "SomeInterior", "weather": "W", "min_ssim": 0.7, "about": "a revolver"}]}
            with open(os.path.join(fdir, "manifest.json"), "w") as f:
                json.dump(man, f)
            self.assertEqual(g2.main(["x", gold]), 0)
            with open(os.path.join(fdir, "scenarios", "layer2_frames.json")) as f:
                sc = {s["id"]: s for s in json.load(f)["scenarios"]}
            ext, inn = sc["L2.frame.ext"], sc["L2.frame.int"]
            self.assertEqual(ext["layer"], 2)
            self.assertIn("--world", ext["candidate"]["argv"])
            self.assertIn("--pitch=-5.0", ext["candidate"]["argv"])
            self.assertIn("--size=4x2", ext["candidate"]["argv"])
            self.assertIn("--weather", ext["candidate"]["argv"])
            self.assertEqual(ext["comparator"]["min_ssim"], 0.5)
            argv = inn["candidate"]["argv"]
            self.assertEqual(argv[argv.index("--cell") + 1], "SomeInterior")
            self.assertNotIn("--world", argv)
            self.assertNotIn("--weather", argv)
            self.assertEqual(inn["comparator"]["min_ssim"], 0.7)
            self.assertIn("a revolver", inn["description"])
            with open(os.path.join(fdir, "ext.frame.json")) as f:
                doc = json.load(f)
            self.assertEqual(doc["image"], "frames_l2/a.png")
            self.assertEqual(doc["masks"], [[0, 0, 1, 1]])


if __name__ == "__main__":
    unittest.main()
