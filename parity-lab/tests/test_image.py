"""Self-tests for the image decoder and comparator (synthetic images only)."""
import os
import struct
import sys
import tempfile
import unittest
import zlib

sys.path.insert(0, os.path.join(os.path.dirname(__file__), ".."))

from fnvlab import image as im  # noqa: E402
from fnvlab.comparators import COMPARATORS, ComparatorError  # noqa: E402


def gradient(w, h, shift=0):
    return im.Image(w, h, [((x * 7 + shift) % 256, (y * 5) % 256, (x * y + shift) % 256) for y in range(h) for x in range(w)])


def png_with_filter(img: im.Image, f: int) -> bytes:
    """Encode RGB rows with one PNG filter type (exercises the decoder's unfiltering)."""
    bpp, stride = 3, img.width * 3
    rows = [bytes(v for x in range(img.width) for v in img.at(x, y)) for y in range(img.height)]
    raw, prev = bytearray(), bytes(stride)
    for row in rows:
        out = bytearray()
        for i in range(stride):
            a = row[i - bpp] if i >= bpp else 0
            b = prev[i]
            c = prev[i - bpp] if i >= bpp else 0
            pred = {0: 0, 1: a, 2: b, 3: (a + b) >> 1, 4: im._paeth(a, b, c)}[f]
            out.append((row[i] - pred) & 255)
        raw.append(f)
        raw += out
        prev = row

    def chunk(t, d):
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", img.width, img.height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b""))


def bmp(img: im.Image, top_down=False) -> bytes:
    stride = (img.width * 3 + 3) & ~3
    body = bytearray()
    ys = range(img.height) if top_down else range(img.height - 1, -1, -1)
    for y in ys:
        row = bytearray()
        for x in range(img.width):
            r, g, b = img.at(x, y)
            row += bytes((b, g, r))
        row += bytes(stride - len(row))
        body += row
    h = -img.height if top_down else img.height
    header = struct.pack("<2sIHHI", b"BM", 54 + len(body), 0, 0, 54)
    info = struct.pack("<IiiHHIIiiII", 40, img.width, h, 1, 24, 0, len(body), 2835, 2835, 0, 0)
    return header + info + bytes(body)


class DecodeTests(unittest.TestCase):
    def test_png_filters(self):
        img = gradient(13, 9)
        for f in range(5):
            self.assertEqual(im.decode_png(png_with_filter(img, f)).pixels, img.pixels, f"filter {f}")

    def test_png_roundtrip_writer(self):
        img = gradient(5, 4)
        self.assertEqual(im.decode_png(im.encode_png(img)).pixels, img.pixels)

    def test_bmp_both_orientations(self):
        img = gradient(7, 5)
        self.assertEqual(im.decode_bmp(bmp(img)).pixels, img.pixels)
        self.assertEqual(im.decode_bmp(bmp(img, top_down=True)).pixels, img.pixels)

    def test_rejects_garbage(self):
        with self.assertRaises(im.ImageError):
            im.decode_png(b"nope")
        with self.assertRaises(im.ImageError):
            im.decode_bmp(b"BM" + bytes(10))


class CompareTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()

    def tearDown(self):
        self.tmp.cleanup()

    def write(self, name, data):
        p = os.path.join(self.tmp.name, name)
        with open(p, "wb") as f:
            f.write(data)
        return p

    def run_cmp(self, exp_img, act_img, cfg, masks=None):
        e = self.write("e.png", im.encode_png(exp_img))
        a = self.write("a.bmp", bmp(act_img))
        return COMPARATORS["image"]({"image": "e.png", "masks": masks or []}, {"image": a},
                                    {"_goldens_dir": self.tmp.name, "width": 32, **cfg})

    def test_identical_passes(self):
        img = gradient(64, 40)
        r = self.run_cmp(img, img, {"min_ssim": 0.99, "max_mae": 0.001})
        self.assertEqual(r.score, 1.0)
        self.assertEqual(r.total_failures, 0)
        self.assertAlmostEqual(r.stats["ssim"], 1.0, places=6)

    def test_different_fails_with_score(self):
        a = gradient(64, 40)
        b = im.Image(64, 40, [(255 - p[0], p[2], p[1]) for p in a.pixels])
        r = self.run_cmp(a, b, {"min_ssim": 0.9})
        self.assertLess(r.score, 0.9)
        self.assertEqual(r.failures[0].kind, "ssim")
        self.assertEqual(len(r.stats["tiles"]), 16)

    def test_mask_excludes_region(self):
        a = gradient(64, 40)
        b = im.Image(64, 40, [p if x < 32 else (0, 0, 0) for i, p in enumerate(a.pixels) for x in [i % 64]])
        unmasked = self.run_cmp(a, b, {"min_ssim": 0.99})
        masked = self.run_cmp(a, b, {"min_ssim": 0.99}, masks=[[30, 0, 64, 40]])
        self.assertGreater(unmasked.total_failures, 0)
        self.assertEqual(masked.total_failures, 0)

    def test_scaled_candidate_is_compared(self):
        big = gradient(128, 80)
        small = im.Image(64, 40, [big.at(x * 2, y * 2) for y in range(40) for x in range(64)])
        r = self.run_cmp(big, small, {"min_ssim": 0.5})
        self.assertEqual(r.stats["working_size"], [32, 20])

    def test_aspect_mismatch(self):
        r = self.run_cmp(gradient(64, 40), gradient(64, 64), {"min_ssim": 0.5})
        self.assertEqual(r.score, 0.0)
        self.assertEqual(r.failures[0].kind, "aspect")

    def test_misconfiguration_is_an_error(self):
        img = gradient(8, 8)
        with self.assertRaises(ComparatorError):
            self.run_cmp(img, img, {})
        with self.assertRaises(ComparatorError):
            COMPARATORS["image"]({"image": "missing.png"}, {"image": "missing.png"}, {"min_ssim": 0.5})


if __name__ == "__main__":
    unittest.main()
