#!/usr/bin/env python3
"""Independent reference reader ("oracle") for NIF meshes / KF animations inside BSA archives.

Written only from docs/spec-notes/nif.md, docs/spec-notes/bsa.md and the "Layer 2" section of
docs/cli-contract.md, deliberately without looking at the Rust implementation, so the parity
lab can compare two independent readers. Standard library only; needs oracle_bsa.py next to it.

Container in brief (nif.md section 2, all little-endian):

  "Gamebryo File Format, Version x.x.x.x\\n"
  version u32 (0x14020007 = 20.2.0.7), endianness u8, user version u32, block count u32
  Bethesda export block: stream version u32 + three u8-length strings (author, process
                         script, export script)
  block types: u16 count + (u32 length + bytes) names
  block type index: u16 per block
  block sizes: u32 per block                        (version >= 20.2.0.5)
  string table: u32 count, u32 max length, strings (version >= 20.1.0.1)
  groups: u32 count + u32 per group
  blocks, back to back; footer: u32 root count + i32 roots

Modes (both read every .nif / .kf in the archive, in archive record order):

  stats <archive>  -> fnv.nif.stats/1   (header only, every version)
  scene <archive>  -> fnv.nif.scene/1   (files with a block-size table; others "skipped")

Exit codes: 0 ok, 1 archive unreadable, 2 usage error. Diagnostics go to stderr.
"""
import argparse
import json
import mmap
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_bsa  # noqa: E402

STATS_SCHEMA = "fnv.nif.stats/1"
SCENE_SCHEMA = "fnv.nif.scene/1"

V20_0_0_4 = 0x14000004
V20_1_0_1 = 0x14010001
V20_2_0_5 = 0x14020005
V20_2_0_7 = 0x14020007

HEADER_PREFIX = b"Gamebryo File Format, Version "
NO_STRING = 0xFFFFFFFF

# nif.md section 3: NiNode and the NiNode-derived types listed there.
NODE_TYPES = frozenset({
    "NiNode", "BSFadeNode", "BSMultiBoundNode", "BSValueNode", "NiBillboardNode",
    "BSOrderedNode", "BSMasterParticleSystem", "BSRangeNode", "BSBlastNode",
    "BSDamageStage", "BSDebrisNode",
})
GEOMETRY_TYPES = frozenset({"NiTriShape", "NiTriStrips", "BSSegmentedTriShape"})
SHAPE_DATA_TYPES = frozenset({"NiTriShapeData", "NiTriStripsData"})

MESH_EXTENSIONS = (".nif", ".kf")


class NifError(Exception):
    """The bytes do not match the expected layout."""


def version_text(v: int) -> str:
    return f"{(v >> 24) & 0xFF}.{(v >> 16) & 0xFF}.{(v >> 8) & 0xFF}.{v & 0xFF}"


# --------------------------------------------------------------------------- byte reader

class Reader:
    """Bounded little-endian reader over buf[pos:end]."""

    __slots__ = ("buf", "pos", "end")

    def __init__(self, buf, pos: int = 0, end: int = None):
        self.buf = buf
        self.pos = pos
        self.end = len(buf) if end is None else end

    def need(self, n: int, what: str) -> None:
        if n < 0 or self.pos + n > self.end:
            raise NifError(f"{what}: needs {n} bytes at offset {self.pos}, "
                           f"only {max(self.end - self.pos, 0)} left")

    def unpack(self, fmt: str, what: str):
        s = struct.Struct("<" + fmt)
        self.need(s.size, what)
        values = s.unpack_from(self.buf, self.pos)
        self.pos += s.size
        return values

    def u8(self, what):
        return self.unpack("B", what)[0]

    def u16(self, what):
        return self.unpack("H", what)[0]

    def u32(self, what):
        return self.unpack("I", what)[0]

    def i32(self, what):
        return self.unpack("i", what)[0]

    def f32s(self, n: int, what):
        return self.unpack(f"{n}f", what) if n else ()

    def raw(self, n: int, what) -> bytes:
        self.need(n, what)
        out = bytes(self.buf[self.pos:self.pos + n])
        self.pos += n
        return out

    def skip(self, n: int, what) -> None:
        self.need(n, what)
        self.pos += n


# --------------------------------------------------------------------------- header

class Header:
    __slots__ = ("version", "user_version", "bs_version", "num_blocks", "type_names",
                 "block_type_index", "block_sizes", "strings", "data_start")


def read_header(buf) -> Header:
    """Parse the container header (nif.md section 2, items 1-10). Raises NifError."""
    r = Reader(buf)
    nl = bytes(buf[:256]).find(b"\n")
    if nl < 0:
        raise NifError("header text line not terminated by a line feed within 256 bytes")
    line = bytes(buf[:nl])
    if not line.startswith(HEADER_PREFIX):
        raise NifError(f"header text line {line[:40]!r} is not a Gamebryo header")
    r.pos = nl + 1
    h = Header()
    h.version = r.u32("version")
    if h.version < V20_0_0_4:
        raise NifError(f"unsupported version {version_text(h.version)}")
    endian = r.u8("endianness")
    if endian != 1:
        raise NifError(f"endianness byte {endian}, only 1 (little-endian) is supported")
    h.user_version = r.u32("user version")
    h.num_blocks = r.u32("block count")
    # Bethesda export block (nif.md: "present for these versions").
    h.bs_version = r.u32("stream version")
    for what in ("author", "process script", "export script"):
        n = r.u8(f"{what} length")
        r.skip(n, what)
    num_types = r.u16("block type count")
    h.type_names = []
    for i in range(num_types):
        n = r.u32(f"block type name {i} length")
        h.type_names.append(oracle_bsa.decode_cp1252(r.raw(n, f"block type name {i}")))
    h.block_type_index = list(r.unpack(f"{h.num_blocks}H", "block type indices")) \
        if h.num_blocks else []
    for i, t in enumerate(h.block_type_index):
        if t >= num_types:
            raise NifError(f"block {i}: type index {t} out of range ({num_types} types)")
    h.block_sizes = None
    if h.version >= V20_2_0_5:
        h.block_sizes = list(r.unpack(f"{h.num_blocks}I", "block sizes")) if h.num_blocks else []
    h.strings = None
    if h.version >= V20_1_0_1:
        count = r.u32("string count")
        r.u32("maximum string length")
        h.strings = []
        for i in range(count):
            n = r.u32(f"string {i} length")
            h.strings.append(oracle_bsa.decode_cp1252(r.raw(n, f"string {i}")))
    groups = r.u32("group count")
    r.skip(4 * groups, "group sizes")
    h.data_start = r.pos
    return h


def stats_entry(path: str, buf, archive_error: str = None) -> dict:
    entry = {"path": path, "version": None, "user_version": None, "bs_version": None,
             "num_blocks": None, "strings": None, "block_types": None, "error": None}
    if archive_error is not None:
        entry["error"] = archive_error
        return entry
    try:
        h = read_header(buf)
    except NifError as exc:
        entry["error"] = str(exc)
        return entry
    counts = {}
    for t in h.block_type_index:
        name = h.type_names[t]
        counts[name] = counts.get(name, 0) + 1
    entry.update({
        "version": version_text(h.version),
        "user_version": h.user_version,
        "bs_version": h.bs_version,
        "num_blocks": h.num_blocks,
        "strings": None if h.strings is None else len(h.strings),
        "block_types": dict(sorted(counts.items())),
    })
    return entry


# --------------------------------------------------------------------------- scene

def _string(h: Header, index: int, what: str):
    if index == NO_STRING:
        return None
    if index >= len(h.strings):
        raise NifError(f"{what}: string index {index} out of range ({len(h.strings)} strings)")
    return h.strings[index]


def _ref_list(r: Reader, what: str):
    n = r.u32(f"{what} count")
    return r.unpack(f"{n}i", what) if n else ()


def read_scene_object(r: Reader, h: Header):
    """Named-object + scene-object leading fields (nif.md section 3)."""
    name = _string(h, r.u32("name"), "name")
    _ref_list(r, "extra data")
    r.i32("controller")
    if h.bs_version > 26:
        r.u32("flags")
    else:
        r.u16("flags")
    translation = list(r.f32s(3, "translation"))
    rotation = list(r.f32s(9, "rotation"))
    scale = r.f32s(1, "scale")[0]
    _ref_list(r, "properties")
    r.i32("collision object")
    return name, translation, rotation, scale


def read_shape_data(r: Reader, h: Header, type_name: str) -> dict:
    """NiTriShapeData / NiTriStripsData, complete (nif.md section 3, items 1-11)."""
    r.i32("group id")
    n = r.u16("vertex count")
    r.skip(2, "keep/compress flags")
    positions = ()
    if r.u8("has vertices"):
        positions = r.f32s(3 * n, "vertices")
    flags = r.u16("vector flags")
    if h.version == V20_2_0_7:
        uv_sets = flags & 0x1
    else:
        uv_sets = flags & 0x3F
    tangent_bit = bool(flags & 0x1000)
    has_normals = bool(r.u8("has normals"))
    has_tangents = False
    if has_normals:
        r.skip(12 * n, "normals")
        if tangent_bit:
            r.skip(24 * n, "tangents and bitangents")
            has_tangents = True
    center = list(r.f32s(3, "bound centre"))
    radius = r.f32s(1, "bound radius")[0]
    has_colors = bool(r.u8("has vertex colours"))
    if has_colors:
        r.skip(16 * n, "vertex colours")
    r.skip(8 * n * uv_sets, "uv sets")
    r.u16("consistency")
    r.i32("additional data")
    tri_count = r.u16("triangle count")
    if type_name == "NiTriShapeData":
        r.u32("triangle point count")
        triangles = 0
        if r.u8("has triangles"):
            r.skip(6 * tri_count, "triangles")
            triangles = tri_count
        groups = r.u16("match group count")
        for i in range(groups):
            k = r.u16(f"match group {i} count")
            r.skip(2 * k, f"match group {i}")
    else:
        strips = r.u16("strip count")
        lengths = r.unpack(f"{strips}H", "strip lengths") if strips else ()
        triangles = 0
        if r.u8("has points"):
            r.skip(2 * sum(lengths), "strip points")
            triangles = sum(max(k - 2, 0) for k in lengths)
    if r.pos != r.end:
        raise NifError(f"{r.end - r.pos} bytes left over at end of block")
    if positions:
        xs, ys, zs = positions[0::3], positions[1::3], positions[2::3]
        aabb_min = [min(xs), min(ys), min(zs)]
        aabb_max = [max(xs), max(ys), max(zs)]
    else:
        aabb_min = aabb_max = None
    return {
        "type": type_name,
        "vertices": n,
        "triangles": triangles,
        "has_normals": has_normals,
        "has_tangents": has_tangents,
        "uv_sets": uv_sets,
        "has_vertex_colors": has_colors,
        "bound_center": center,
        "bound_radius": radius,
        "aabb_min": aabb_min,
        "aabb_max": aabb_max,
    }


def scene_file(path: str, buf):
    """Analyse one file. Returns ("skipped", reason) or ("ok", objects, shapes)."""
    try:
        h = read_header(buf)
    except NifError as exc:
        return "skipped", f"container: {exc}"
    if h.block_sizes is None:
        return "skipped", f"no block-size table (version {version_text(h.version)})"
    # Block boundaries from the size table, then the footer.
    starts = []
    pos = h.data_start
    for i, size in enumerate(h.block_sizes):
        starts.append(pos)
        pos += size
    if pos > len(buf):
        return "skipped", (f"container: block sizes end at {pos}, beyond the file end "
                           f"{len(buf)}")
    try:
        footer = Reader(buf, pos)
        roots = footer.u32("footer root count")
        footer.skip(4 * roots, "footer roots")
    except NifError as exc:
        return "skipped", f"container: {exc}"
    objects = []
    shapes = []
    children = []  # (node index, child refs) in block order
    try:
        for i, t in enumerate(h.block_type_index):
            type_name = h.type_names[t]
            if type_name in NODE_TYPES or type_name in GEOMETRY_TYPES:
                r = Reader(buf, starts[i], starts[i] + h.block_sizes[i])
                try:
                    name, translation, rotation, scale = read_scene_object(r, h)
                    if type_name in NODE_TYPES:
                        children.append((i, _ref_list(r, "children")))
                        _ref_list(r, "effects")
                except NifError as exc:
                    raise NifError(f"block {i} ({type_name}): {exc}") from None
                objects.append({"key": f"{path}#{i}", "type": type_name, "name": name,
                                "parent": None, "translation": translation,
                                "rotation": rotation, "scale": scale})
            elif type_name in SHAPE_DATA_TYPES:
                r = Reader(buf, starts[i], starts[i] + h.block_sizes[i])
                try:
                    shape = read_shape_data(r, h, type_name)
                except NifError as exc:
                    raise NifError(f"block {i} ({type_name}): {exc}") from None
                shapes.append({"key": f"{path}#{i}", **shape})
    except NifError as exc:
        return "skipped", str(exc)
    parent = {}
    for node, refs in children:  # block order: first assignment is the lowest index
        for c in refs:
            if c >= 0 and c not in parent:
                parent[c] = node
    for obj in objects:
        index = int(obj["key"].rsplit("#", 1)[1])
        obj["parent"] = parent.get(index)
    return "ok", objects, shapes


# --------------------------------------------------------------------------- archive

def iter_archive(buf, extensions):
    """Yield (path, data_bytes_or_None, error_or_None) for every file whose path ends with
    one of `extensions`, in archive record order (folder records, then file records).

    Walks the BSA layout as described in docs/spec-notes/bsa.md and decodes with
    oracle_bsa._decode_entry (which handles compression and the archive flag 0x100
    embedded path prefix). Raises ValueError if the archive header is unreadable."""
    size = len(buf)
    if size < oracle_bsa.HEADER_SIZE:
        raise ValueError(f"file is {size} bytes, BSA header needs {oracle_bsa.HEADER_SIZE}")
    (magic, version, folder_offset, archive_flags, folder_count, file_count,
     _folder_names_len, file_names_len, _file_flags, _pad) = oracle_bsa._HEADER.unpack_from(buf, 0)
    if magic != b"BSA\0":
        raise ValueError(f"bad BSA magic {magic!r}")
    if archive_flags & oracle_bsa.ARCHIVE_XBOX:
        raise ValueError("Xbox 360 archive (flag 0x40) not supported")
    has_folder_names = bool(archive_flags & oracle_bsa.ARCHIVE_FOLDER_NAMES)
    has_file_names = bool(archive_flags & oracle_bsa.ARCHIVE_FILE_NAMES)
    compressed_default = bool(archive_flags & oracle_bsa.ARCHIVE_COMPRESSED_DEFAULT)
    embedded = bool(archive_flags & oracle_bsa.ARCHIVE_EMBEDDED_NAMES)
    pos = folder_offset
    folders = []
    for _ in range(folder_count):
        if pos + 16 > size:
            raise ValueError("folder records truncated")
        folders.append(oracle_bsa._FOLDER_RECORD.unpack_from(buf, pos))
        pos += 16
    entries = []
    for _hash, count, _offset in folders:
        folder_name = b""
        if has_folder_names:
            if pos >= size:
                raise ValueError("folder name truncated")
            length = buf[pos]
            folder_name = bytes(buf[pos + 1:pos + 1 + length]).split(b"\0", 1)[0]
            pos += 1 + length
        for _ in range(count):
            if pos + 16 > size:
                raise ValueError("file records truncated")
            file_hash, raw_size, data_offset = oracle_bsa._FILE_RECORD.unpack_from(buf, pos)
            pos += 16
            entries.append((folder_name, file_hash, raw_size, data_offset))
    names = []
    if has_file_names:
        names = bytes(buf[pos:pos + file_names_len]).split(b"\0")
    listing = oracle_bsa.Listing("")
    for k, (folder_name, file_hash, raw_size, data_offset) in enumerate(entries):
        if k < len(names) and has_file_names:
            name_text = oracle_bsa.decode_cp1252(names[k])
        else:
            name_text = f"#0x{file_hash:016x}"
        path = (f"{oracle_bsa.decode_cp1252(folder_name)}\\{name_text}"
                if has_folder_names and folder_name else name_text)
        if not path.lower().endswith(extensions):
            continue
        stored = raw_size & oracle_bsa.SIZE_MASK
        compressed = compressed_default != bool(raw_size & oracle_bsa.SIZE_COMPRESSION_TOGGLE)
        before = len(listing.errors)
        data = oracle_bsa._decode_entry(buf, listing, path, data_offset, stored, compressed,
                                        embedded)
        error = None
        if len(listing.errors) > before:
            error = "archive: " + "; ".join(e["detail"] for e in listing.errors[before:])
            data = None
        yield path, (None if data is None else bytes(data)), error


def open_archive(path):
    """Return (file object, mmap) for an archive; caller closes both."""
    f = open(path, "rb")
    try:
        if os.fstat(f.fileno()).st_size == 0:
            return f, b""
        return f, mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
    except Exception:
        f.close()
        raise


# --------------------------------------------------------------------------- output

def render(doc: dict, list_keys=()) -> str:
    """JSON with one list element per line for the keys in `list_keys` (as oracle_bsa)."""
    lines = ["{"]
    keys = list(doc)
    for i, key in enumerate(keys):
        value = doc[key]
        comma = "," if i < len(keys) - 1 else ""
        if key in list_keys and value:
            lines.append(f" {json.dumps(key)}: [")
            lines.append(",\n".join(" " + json.dumps(item, ensure_ascii=False) for item in value))
            lines.append(f" ]{comma}")
        else:
            lines.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{comma}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def stats_document(buf, archive_name: str) -> dict:
    files = [stats_entry(path, data, err)
             for path, data, err in iter_archive(buf, MESH_EXTENSIONS)]
    return {"schema": STATS_SCHEMA, "archive": archive_name, "files": files}


def scene_document(buf, archive_name: str) -> dict:
    skipped, objects, shapes = [], [], []
    for path, data, err in iter_archive(buf, MESH_EXTENSIONS):
        if err is not None:
            skipped.append({"path": path, "reason": err})
            continue
        result = scene_file(path, data)
        if result[0] == "skipped":
            skipped.append({"path": path, "reason": result[1]})
        else:
            objects.extend(result[1])
            shapes.extend(result[2])
    return {"schema": SCENE_SCHEMA, "archive": archive_name, "skipped": skipped,
            "objects": objects, "shapes": shapes}


def write_output(text: str, out) -> None:
    if out:
        with open(out, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="NIF/KF statistics and scene summary of a BSA.")
    sub = parser.add_subparsers(dest="command", required=True)
    for name, text in (("stats", STATS_SCHEMA), ("scene", SCENE_SCHEMA)):
        p = sub.add_parser(name, help=f"emit {text}")
        p.add_argument("archive")
        p.add_argument("-o", "--out", "--output", dest="out",
                       help="write JSON here instead of stdout")
    args = parser.parse_args(argv)
    name = os.path.basename(args.archive)
    try:
        f, buf = open_archive(args.archive)
    except OSError as exc:
        print(f"oracle_nif: cannot read {args.archive}: {exc}", file=sys.stderr)
        return 1
    try:
        if args.command == "stats":
            doc = stats_document(buf, name)
            text = render(doc, ("files",))
            problems = [f"{e['path']}: {e['error']}" for e in doc["files"] if e["error"]]
        else:
            doc = scene_document(buf, name)
            text = render(doc, ("skipped", "objects", "shapes"))
            problems = [f"skipped {e['path']}: {e['reason']}" for e in doc["skipped"]]
    except ValueError as exc:
        print(f"oracle_nif: {args.archive}: {exc}", file=sys.stderr)
        return 1
    finally:
        if isinstance(buf, mmap.mmap):
            buf.close()
        f.close()
    write_output(text, args.out)
    for line in problems:
        print(f"oracle_nif: {line}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
