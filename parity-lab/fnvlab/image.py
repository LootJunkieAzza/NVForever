"""Image decoding and comparison for rendered-frame scenarios (standard library only).

Decoders: PNG (8-bit greyscale, RGB, RGBA, grey+alpha; non-interlaced; all five row
filters) and uncompressed BMP (24/32-bit, bottom-up or top-down). That covers Bevy's
screenshots (PNG RGBA8) and the original game's screenshots (BMP).

Metrics, computed on images box-downsampled to a common working size:
- ``ssim``: mean structural similarity of luminance over 8x8 windows (stride 4),
  standard constants (K1 = 0.01, K2 = 0.03, L = 1);
- ``mae``: mean absolute difference of sRGB channels in [0, 1];
- ``mean_color_delta``: difference of the average colours (lighting / tint shifts).
Masked rectangles (source pixel coordinates) are excluded from all three.
"""
from __future__ import annotations

import struct
import zlib
from dataclasses import dataclass


class ImageError(Exception):
    pass


@dataclass
class Image:
    width: int
    height: int
    # Row-major RGB triples, 0..255.
    pixels: list[tuple[int, int, int]]

    def at(self, x: int, y: int) -> tuple[int, int, int]:
        return self.pixels[y * self.width + x]


def _paeth(a: int, b: int, c: int) -> int:
    p = a + b - c
    pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
    if pa <= pb and pa <= pc:
        return a
    return b if pb <= pc else c


def decode_png(data: bytes) -> Image:
    if data[:8] != b"\x89PNG\r\n\x1a\n":
        raise ImageError("not a PNG")
    pos, idat, ihdr = 8, bytearray(), None
    while pos + 8 <= len(data):
        (length,) = struct.unpack(">I", data[pos:pos + 4])
        ctype = data[pos + 4:pos + 8]
        chunk = data[pos + 8:pos + 8 + length]
        pos += 12 + length
        if ctype == b"IHDR":
            ihdr = struct.unpack(">IIBBBBB", chunk)
        elif ctype == b"IDAT":
            idat += chunk
        elif ctype == b"IEND":
            break
    if ihdr is None:
        raise ImageError("PNG without IHDR")
    w, h, depth, ctype, _comp, _filt, interlace = ihdr
    channels = {0: 1, 2: 3, 4: 2, 6: 4}.get(ctype)
    if depth != 8 or channels is None or interlace != 0:
        raise ImageError(f"unsupported PNG (depth {depth}, colour type {ctype}, interlace {interlace})")
    raw = zlib.decompress(bytes(idat))
    stride = w * channels
    if len(raw) < h * (stride + 1):
        raise ImageError("PNG data too short")
    prev = bytearray(stride)
    pixels: list[tuple[int, int, int]] = []
    for y in range(h):
        f = raw[y * (stride + 1)]
        line = bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        for i in range(stride):
            a = line[i - channels] if i >= channels else 0
            b = prev[i]
            c = prev[i - channels] if i >= channels else 0
            if f == 1:
                line[i] = (line[i] + a) & 255
            elif f == 2:
                line[i] = (line[i] + b) & 255
            elif f == 3:
                line[i] = (line[i] + ((a + b) >> 1)) & 255
            elif f == 4:
                line[i] = (line[i] + _paeth(a, b, c)) & 255
            elif f != 0:
                raise ImageError(f"bad PNG filter {f}")
        for x in range(w):
            o = x * channels
            if channels <= 2:
                v = line[o]
                pixels.append((v, v, v))
            else:
                pixels.append((line[o], line[o + 1], line[o + 2]))
        prev = line
    return Image(w, h, pixels)


def decode_bmp(data: bytes) -> Image:
    if data[:2] != b"BM" or len(data) < 54:
        raise ImageError("not a BMP")
    (offset,) = struct.unpack("<I", data[10:14])
    w, h, _planes, bpp, comp = struct.unpack("<iiHHI", data[18:34])
    if bpp not in (24, 32) or comp not in (0, 3):
        raise ImageError(f"unsupported BMP ({bpp} bits, compression {comp})")
    top_down = h < 0
    h = abs(h)
    bytes_pp = bpp // 8
    stride = (w * bytes_pp + 3) & ~3
    if len(data) < offset + stride * h:
        raise ImageError("BMP data too short")
    pixels: list[tuple[int, int, int]] = [(0, 0, 0)] * (w * h)
    for row in range(h):
        y = row if top_down else h - 1 - row
        base = offset + row * stride
        for x in range(w):
            o = base + x * bytes_pp
            pixels[y * w + x] = (data[o + 2], data[o + 1], data[o])
    return Image(w, h, pixels)


def load(path: str) -> Image:
    with open(path, "rb") as f:
        data = f.read()
    if data[:8] == b"\x89PNG\r\n\x1a\n":
        return decode_png(data)
    if data[:2] == b"BM":
        return decode_bmp(data)
    raise ImageError(f"{path}: not a PNG or BMP")


def encode_png(img: Image) -> bytes:
    """Minimal RGB PNG writer (for tests and diff images)."""
    raw = bytearray()
    for y in range(img.height):
        raw.append(0)
        for x in range(img.width):
            raw.extend(img.at(x, y))

    def chunk(t: bytes, d: bytes) -> bytes:
        return struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d) & 0xFFFFFFFF)

    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", img.width, img.height, 8, 2, 0, 0, 0))
            + chunk(b"IDAT", zlib.compress(bytes(raw))) + chunk(b"IEND", b""))


@dataclass
class Plane:
    """A downsampled image: RGB floats in [0, 1] plus a validity mask."""
    width: int
    height: int
    rgb: list[tuple[float, float, float]]
    valid: list[bool]


def downsample(img: Image, width: int, masks: list[list[float]] | None = None) -> Plane:
    """Box-filter to `width` columns (height keeps the aspect ratio). A working pixel is
    invalid if any source pixel under it lies in a mask rectangle [x0, y0, x1, y1)."""
    width = max(1, min(width, img.width))
    height = max(1, round(img.height * width / img.width))
    sx, sy = img.width / width, img.height / height
    masks = masks or []
    rgb, valid = [], []
    for y in range(height):
        y0, y1 = int(y * sy), max(int(y * sy) + 1, int((y + 1) * sy))
        for x in range(width):
            x0, x1 = int(x * sx), max(int(x * sx) + 1, int((x + 1) * sx))
            ok = not any(x0 < m[2] and x1 > m[0] and y0 < m[3] and y1 > m[1] for m in masks)
            r = g = b = 0
            n = 0
            for yy in range(y0, min(y1, img.height)):
                row = yy * img.width
                for xx in range(x0, min(x1, img.width)):
                    p = img.pixels[row + xx]
                    r += p[0]
                    g += p[1]
                    b += p[2]
                    n += 1
            n = max(n, 1)
            rgb.append((r / n / 255.0, g / n / 255.0, b / n / 255.0))
            valid.append(ok)
    return Plane(width, height, rgb, valid)


def _luma(c: tuple[float, float, float]) -> float:
    return 0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]


@dataclass
class Metrics:
    ssim: float
    mae: float
    mean_color_delta: float
    tiles: list[dict]
    width: int
    height: int


def compare(a: Plane, b: Plane, tiles: int = 4) -> Metrics:
    if (a.width, a.height) != (b.width, b.height):
        raise ImageError(f"working sizes differ: {a.width}x{a.height} vs {b.width}x{b.height}")
    w, h = a.width, a.height
    valid = [x and y for x, y in zip(a.valid, b.valid)]
    la = [_luma(c) for c in a.rgb]
    lb = [_luma(c) for c in b.rgb]
    c1, c2 = 0.01 ** 2, 0.03 ** 2
    win, step = 8, 4
    ssims: list[tuple[int, int, float]] = []
    for y0 in range(0, max(h - win, 0) + 1, step):
        for x0 in range(0, max(w - win, 0) + 1, step):
            idx = [(y0 + dy) * w + x0 + dx for dy in range(min(win, h)) for dx in range(min(win, w))]
            if not all(valid[i] for i in idx):
                continue
            n = len(idx)
            ma = sum(la[i] for i in idx) / n
            mb = sum(lb[i] for i in idx) / n
            va = sum((la[i] - ma) ** 2 for i in idx) / n
            vb = sum((lb[i] - mb) ** 2 for i in idx) / n
            cov = sum((la[i] - ma) * (lb[i] - mb) for i in idx) / n
            s = ((2 * ma * mb + c1) * (2 * cov + c2)) / ((ma * ma + mb * mb + c1) * (va + vb + c2))
            ssims.append((x0 + win // 2, y0 + win // 2, s))
    ssim = sum(s for _, _, s in ssims) / len(ssims) if ssims else 0.0
    vi = [i for i in range(w * h) if valid[i]]
    if vi:
        mae = sum(abs(a.rgb[i][k] - b.rgb[i][k]) for i in vi for k in range(3)) / (3 * len(vi))
        mean_a = [sum(a.rgb[i][k] for i in vi) / len(vi) for k in range(3)]
        mean_b = [sum(b.rgb[i][k] for i in vi) / len(vi) for k in range(3)]
        mcd = max(abs(mean_a[k] - mean_b[k]) for k in range(3))
    else:
        mae, mcd = 1.0, 1.0
    out_tiles = []
    for ty in range(tiles):
        for tx in range(tiles):
            xs, xe = tx * w // tiles, (tx + 1) * w // tiles
            ys, ye = ty * h // tiles, (ty + 1) * h // tiles
            sel = [s for (x, y, s) in ssims if xs <= x < xe and ys <= y < ye]
            ti = [y * w + x for y in range(ys, ye) for x in range(xs, xe) if valid[y * w + x]]
            t_mae = sum(abs(a.rgb[i][k] - b.rgb[i][k]) for i in ti for k in range(3)) / (3 * len(ti)) if ti else None
            out_tiles.append({"tile": [tx, ty], "ssim": round(sum(sel) / len(sel), 4) if sel else None,
                              "mae": round(t_mae, 4) if t_mae is not None else None})
    return Metrics(ssim, mae, mcd, out_tiles, w, h)
