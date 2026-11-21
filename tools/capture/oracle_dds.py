#!/usr/bin/env python3
"""Independent reference reader ("oracle") for DDS textures inside BSA archives.

Written only from docs/spec-notes/dds.md (sections 1-3 and the output format in section 5)
and docs/spec-notes/bsa.md, without looking at the Rust implementation. Standard library
only; needs oracle_bsa.py and oracle_nif.py (archive walker) next to it.

Layout in brief: "DDS " magic, 124-byte header (31 u32: size, flags, height, width,
pitch/linear size, depth, mip count, 11 reserved, 32-byte pixel format {size, flags, FourCC,
RGB bit count, R/G/B/A masks}, caps1..caps4, reserved), then pixel data: for each face every
mip level from largest to smallest.

usage: oracle_dds.py list <archive.bsa> [--out FILE]   -> fnv.dds.listing/1
Exit codes: 0 ok, 1 archive unreadable, 2 usage error.
"""
import argparse
import mmap
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_nif  # noqa: E402

SCHEMA = "fnv.dds.listing/1"
HEADER_BYTES = 128  # magic + 124-byte header

_HEADER = struct.Struct("<4s31I")

DDPF_FOURCC = 0x4
CAPS2_CUBEMAP = 0x200
CAPS2_FACES = 0xFC00  # +X, -X, +Y, -Y, +Z, -Z

# dds.md section 2: (pixel-format flags, FourCC, bit count, (R, G, B, A masks)) -> name.
BLOCK_FORMATS = {b"DXT1": 8, b"DXT3": 16, b"DXT5": 16}
UNCOMPRESSED_FORMATS = {
    (0x41, 32, (0x00FF0000, 0x0000FF00, 0x000000FF, 0xFF000000)): "A8R8G8B8",
    (0x40, 24, (0x00FF0000, 0x0000FF00, 0x000000FF, 0x00000000)): "R8G8B8",
    (0x41, 16, (0x0F00, 0x00F0, 0x000F, 0xF000)): "A4R4G4B4",
}


def _fourcc_text(fourcc: bytes) -> str:
    if all(0x20 <= b < 0x7F for b in fourcc):
        return fourcc.decode("ascii")
    return "0x" + fourcc.hex()


def classify(pf_flags: int, fourcc: bytes, bits: int, masks):
    """Return (format name, block bytes or None, bits per texel or None)."""
    if pf_flags == DDPF_FOURCC and fourcc in BLOCK_FORMATS:
        return fourcc.decode("ascii"), BLOCK_FORMATS[fourcc], None
    name = UNCOMPRESSED_FORMATS.get((pf_flags, bits, tuple(masks)))
    if name is not None:
        return name, None, bits
    text = (f"unknown(flags=0x{pf_flags:x},fourcc={_fourcc_text(fourcc)},bits={bits},"
            f"masks=0x{masks[0]:08x}/0x{masks[1]:08x}/0x{masks[2]:08x}/0x{masks[3]:08x})")
    return text, None, None


def level_bytes(w: int, h: int, block_bytes, bits) -> int:
    if block_bytes is not None:
        return max(1, (w + 3) // 4) * max(1, (h + 3) // 4) * block_bytes
    return w * h * bits // 8


def dds_entry(path: str, data, archive_error: str = None) -> dict:
    entry = {"path": path, "width": None, "height": None, "mip_count": None, "levels": None,
             "format": None, "cubemap": None, "faces": None, "expected_size": None,
             "actual_size": None, "error": None}
    if archive_error is not None:
        entry["error"] = archive_error
        return entry
    if len(data) < 4 or data[:4] != b"DDS ":
        entry["error"] = "bad magic"
        return entry
    if len(data) < HEADER_BYTES:
        entry["error"] = "short header"
        return entry
    v = _HEADER.unpack_from(data, 0)[1:]
    height, width, mip_count = v[2], v[3], v[6]
    pf_flags, fourcc_u32, bits = v[19], v[20], v[21]
    masks = v[22:26]
    caps2 = v[27]
    fourcc = struct.pack("<I", fourcc_u32)
    fmt, block_bytes, texel_bits = classify(pf_flags, fourcc, bits, masks)
    levels = mip_count if mip_count else 1
    cubemap = bool(caps2 & CAPS2_CUBEMAP)
    faces = bin(caps2 & CAPS2_FACES).count("1") if cubemap else 1
    expected = None
    if block_bytes is not None or texel_bits is not None:
        per_face = 0
        w, h = width, height
        for _ in range(levels):
            per_face += level_bytes(w, h, block_bytes, texel_bits)
            w, h = max(1, w // 2), max(1, h // 2)
        expected = per_face * faces
    entry.update({
        "width": width, "height": height, "mip_count": mip_count, "levels": levels,
        "format": fmt, "cubemap": cubemap, "faces": faces, "expected_size": expected,
        "actual_size": len(data) - HEADER_BYTES,
    })
    return entry


def listing_document(buf, archive_name: str) -> dict:
    files = [dds_entry(path, data, err)
             for path, data, err in oracle_nif.iter_archive(buf, (".dds",))]
    return {"schema": SCHEMA, "archive": archive_name, "files": files}


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="List the DDS textures of a BSA archive.")
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("list", help=f"emit {SCHEMA}")
    p.add_argument("archive")
    p.add_argument("-o", "--out", "--output", dest="out", help="write JSON here instead of stdout")
    args = parser.parse_args(argv)
    try:
        f, buf = oracle_nif.open_archive(args.archive)
    except OSError as exc:
        print(f"oracle_dds: cannot read {args.archive}: {exc}", file=sys.stderr)
        return 1
    try:
        doc = listing_document(buf, os.path.basename(args.archive))
    except ValueError as exc:
        print(f"oracle_dds: {args.archive}: {exc}", file=sys.stderr)
        return 1
    finally:
        if isinstance(buf, mmap.mmap):
            buf.close()
        f.close()
    oracle_nif.write_output(oracle_nif.render(doc, ("files",)), args.out)
    for e in doc["files"]:
        if e["error"]:
            print(f"oracle_dds: {e['path']}: {e['error']}", file=sys.stderr)
        elif e["expected_size"] != e["actual_size"]:
            print(f"oracle_dds: {e['path']}: expected {e['expected_size']} bytes, "
                  f"found {e['actual_size']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
