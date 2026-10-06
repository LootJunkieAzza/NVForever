#!/usr/bin/env python3
"""Independent reference reader ("oracle") for Fallout: New Vegas plugins (ESM/ESP).

Written only from docs/spec-notes/esm.md and docs/cli-contract.md, deliberately without
looking at the Rust implementation, so the parity lab can compare two independent readers.
Standard library only; the file is self-contained so it can be copied to another machine.

Format in brief (all integers little-endian):

  A plugin is a flat sequence of records and groups; groups nest.
  record header (24 bytes)  type (4 chars), data size u32, flags u32, FormID u32,
                            version control u32, form version u16, version control u16;
                            followed by `data size` bytes of data.
                            Flag 0x00040000: data = u32 decoded size + zlib stream.
                            Flag 0x20: deleted.  Flag 0x1 on TES4: master file.
  group header (24 bytes)   "GRUP", total size u32 (header included), label (4 bytes),
                            group type i32, 4 x u16 (stamp etc., not interpreted)
  subrecords (record data)  type (4 chars), size u16, data.  "XXXX" with size 4 holds the
                            real u32 size of the next subrecord (whose own size field is 0).
  TES4 (first record)       HEDR (version f32, record count i32, next object id u32),
                            CNAM author, SNAM description, MAST/DATA pairs, ONAM u32 list.
  Text is Windows-1252, NUL-terminated.

Groups are walked with an explicit stack of group end offsets (no recursion), so the
245 MB FalloutNV.esm (~540k records and groups) is handled in one linear pass.

The reader never raises on odd data: structural problems go to "errors", irregular but
usable data (e.g. a zlib stream whose Adler-32 is wrong but whose length matches) goes to
"warnings", each as {"code", "detail"}.

usage: oracle_esm.py inventory <plugin> [-o out.json]   -> fnv.esm.inventory/1
       oracle_esm.py stats <plugin> [-o out.json]       -> fnv.esm.stats/1
Exit codes: 0 ok, 1 input unreadable, 2 usage error.
"""
import argparse
import json
import os
import struct
import sys
import zlib

INVENTORY_SCHEMA = "fnv.esm.inventory/1"
STATS_SCHEMA = "fnv.esm.stats/1"

HEADER_SIZE = 24
FLAG_MASTER = 0x1
FLAG_DELETED = 0x20
FLAG_COMPRESSED = 0x00040000

GROUP_TYPE_NAMES = [
    "top",
    "world_children",
    "interior_cell_block",
    "interior_cell_subblock",
    "exterior_cell_block",
    "exterior_cell_subblock",
    "cell_children",
    "topic_children",
    "cell_persistent_children",
    "cell_temporary_children",
    "cell_visible_distant_children",
]
# TES4 subrecords with a dedicated header field; anything else is listed in other_subrecords.
KNOWN_TES4_SUBRECORDS = {b"HEDR", b"CNAM", b"SNAM", b"MAST", b"DATA", b"ONAM"}

_RECORD_HEADER = struct.Struct("<4sIIIIHH")
_GROUP_HEADER = struct.Struct("<4sI4siHHHH")
_SUBRECORD_HEADER = struct.Struct("<4sH")
_U32 = struct.Struct("<I")
_HEDR = struct.Struct("<fiI")


# --------------------------------------------------------------------------- helpers

def decode_cp1252(raw: bytes) -> str:
    """Windows-1252 to str. The five bytes cp1252 leaves undefined (0x81, 0x8D, 0x8F, 0x90,
    0x9D) map to the same-numbered C1 control characters, as Windows itself does."""
    try:
        return raw.decode("cp1252")
    except UnicodeDecodeError:
        return "".join(
            bytes([b]).decode("cp1252") if b not in (0x81, 0x8D, 0x8F, 0x90, 0x9D) else chr(b)
            for b in raw
        )


def zstring(raw) -> str:
    """Text up to the first NUL (or the whole field if there is none)."""
    return decode_cp1252(bytes(raw).split(b"\0", 1)[0])


def signature_text(sig: bytes) -> str:
    return sig.decode("latin-1")


def inflate_zlib(stream, expected_size: int):
    """Decode a zlib stream tolerantly.

    The 2-byte zlib header is checked by hand and the deflate data is inflated raw
    (wbits=-15), so a missing, short or wrong Adler-32 trailer does not prevent decoding.
    Returns (data_or_None, problems) where problems is a list of (severity, code, detail)
    with severity "warning" (data usable) or "error" (data missing or wrong length).
    """
    problems = []
    stream = bytes(stream)
    if len(stream) < 2:
        return None, [("error", "zlib_truncated", f"stream is {len(stream)} bytes, no zlib header")]
    cmf, flg = stream[0], stream[1]
    if (cmf & 0x0F) != 8 or ((cmf << 8) | flg) % 31 != 0:
        return None, [("error", "zlib_bad_header", f"zlib header bytes 0x{cmf:02x} 0x{flg:02x}")]
    if flg & 0x20:
        return None, [("error", "zlib_preset_dictionary", "stream requires a preset dictionary")]
    inflater = zlib.decompressobj(-15)
    try:
        out = inflater.decompress(stream[2:])
        out += inflater.flush()
    except zlib.error as exc:
        return None, [("error", "zlib_error", str(exc))]
    if not inflater.eof:
        problems.append(("error", "zlib_truncated",
                         f"deflate data ends before the final block ({len(out)} bytes decoded)"))
    else:
        rest = inflater.unused_data
        computed = zlib.adler32(out) & 0xFFFFFFFF
        if len(rest) == 0:
            problems.append(("warning", "zlib_trailer_missing",
                             "no Adler-32 trailer after the deflate data"))
        elif len(rest) < 4:
            problems.append(("warning", "zlib_trailer_truncated",
                             f"only {len(rest)} of 4 Adler-32 trailer bytes present"))
        else:
            stored = struct.unpack(">I", rest[:4])[0]
            if stored != computed:
                problems.append(("warning", "zlib_checksum_mismatch",
                                 f"stored Adler-32 0x{stored:08x}, computed 0x{computed:08x}"))
            if len(rest) > 4:
                problems.append(("warning", "zlib_trailing_bytes",
                                 f"{len(rest) - 4} bytes after the Adler-32 trailer"))
    if len(out) != expected_size:
        problems.append(("error", "size_mismatch",
                         f"declared {expected_size} bytes, decoded {len(out)}"))
    return out, problems


def iter_subrecords(data, start: int, end: int, problems: list, where: str):
    """Yield (signature, data_offset, size) for the subrecords in data[start:end].

    Handles XXXX: a 4-byte XXXX subrecord gives the real size of the next subrecord.
    Structural problems are appended to `problems` as (severity, code, detail).
    """
    pos = start
    pending_size = None
    unpack = _SUBRECORD_HEADER.unpack_from
    while pos < end:
        if end - pos < 6:
            problems.append(("error", "subrecord_header_truncated",
                             f"{where}: {end - pos} stray bytes at the end of the record data"))
            return
        sig, size = unpack(data, pos)
        pos += 6
        if pending_size is not None:
            if size != 0:
                problems.append(("warning", "xxxx_size_field_nonzero",
                                 f"{where}: {signature_text(sig)} after XXXX has size field {size}"))
            size, pending_size = pending_size, None
        elif sig == b"XXXX" and size == 4:
            if pos + 4 > end:
                problems.append(("error", "subrecord_overrun", f"{where}: XXXX runs past record end"))
                return
            pending_size = _U32.unpack_from(data, pos)[0]
            pos += 4
            continue
        if pos + size > end:
            problems.append(("error", "subrecord_overrun",
                             f"{where}: {signature_text(sig)} of {size} bytes runs past record end"))
            return
        yield sig, pos, size
        pos += size
    if pending_size is not None:
        problems.append(("warning", "xxxx_dangling", f"{where}: XXXX is the last subrecord"))


def first_edid_full(data, start: int, end: int, problems: list, where: str):
    """Return (edid_raw, full_raw) of the first EDID/FULL subrecords (None when absent).

    The hot path of the whole tool; equivalent to scanning iter_subrecords() but inlined.
    """
    edid = full = None
    pos = start
    pending_size = None
    unpack = _SUBRECORD_HEADER.unpack_from
    while pos < end:
        if end - pos < 6:
            problems.append(("error", "subrecord_header_truncated",
                             f"{where}: {end - pos} stray bytes at the end of the record data"))
            break
        sig, size = unpack(data, pos)
        pos += 6
        if pending_size is not None:
            if size != 0:
                problems.append(("warning", "xxxx_size_field_nonzero",
                                 f"{where}: {signature_text(sig)} after XXXX has size field {size}"))
            size, pending_size = pending_size, None
        elif sig == b"XXXX" and size == 4:
            if pos + 4 > end:
                problems.append(("error", "subrecord_overrun", f"{where}: XXXX runs past record end"))
                break
            pending_size = _U32.unpack_from(data, pos)[0]
            pos += 4
            continue
        nxt = pos + size
        if nxt > end:
            problems.append(("error", "subrecord_overrun",
                             f"{where}: {signature_text(sig)} of {size} bytes runs past record end"))
            break
        if sig == b"EDID":
            if edid is None:
                edid = data[pos:nxt]
        elif sig == b"FULL":
            if full is None:
                full = data[pos:nxt]
        pos = nxt
    if pending_size is not None:
        problems.append(("warning", "xxxx_dangling", f"{where}: XXXX is the last subrecord"))
    return edid, full


# --------------------------------------------------------------------------- scanner

class PluginScan:
    """Result of one linear pass over a plugin."""

    def __init__(self, plugin: str, file_size: int):
        self.plugin = plugin
        self.file_size = file_size
        self.header = None
        self.records = []           # inventory rows (TES4 excluded)
        self.errors = []
        self.warnings = []
        self.record_count = 0       # includes TES4
        self.group_count = 0
        self.compressed_records = 0
        self.deleted_records = 0
        self.hedr_num_records = None
        self.top_groups = []
        self.by_type = {}
        self.by_group_type = {name: 0 for name in GROUP_TYPE_NAMES}
        self.fatal = False

    def report(self, severity: str, code: str, detail: str) -> None:
        (self.errors if severity == "error" else self.warnings).append({"code": code, "detail": detail})


def _record_data(buf, scan: PluginScan, pos: int, end: int, flags: int, where: str):
    """Return (data, start, end) for a record's subrecord area, decompressing if needed.
    Returns (None, 0, 0) if the data cannot be obtained."""
    if not flags & FLAG_COMPRESSED:
        return buf, pos, end
    if end - pos < 4:
        scan.report("error", "compressed_record_too_small",
                    f"{where}: compressed data is {end - pos} bytes, needs a 4-byte size")
        return None, 0, 0
    declared = _U32.unpack_from(buf, pos)[0]
    out, problems = inflate_zlib(buf[pos + 4:end], declared)
    fatal = False
    for severity, code, detail in problems:
        scan.report(severity, code, f"{where}: {detail}")
        fatal = fatal or severity == "error"
    if out is None or fatal:
        return None, 0, 0
    return out, 0, len(out)


def _parse_tes4(buf, scan: PluginScan, flags: int, data, start: int, end: int) -> None:
    problems = []
    version = num_records = next_object_id = None
    author = description = None
    masters = []
    onam_count = 0
    other = []
    hedr_seen = False
    for sig, off, size in iter_subrecords(data, start, end, problems, "TES4"):
        if sig == b"HEDR":
            if hedr_seen:
                problems.append(("warning", "duplicate_hedr", "TES4 has more than one HEDR; using the first"))
                continue
            hedr_seen = True
            if size < _HEDR.size:
                problems.append(("error", "hedr_too_small", f"HEDR is {size} bytes, expected 12"))
                continue
            if size != _HEDR.size:
                problems.append(("warning", "hedr_size", f"HEDR is {size} bytes, expected 12"))
            version, num_records, next_object_id = _HEDR.unpack_from(data, off)
        elif sig == b"CNAM":
            if author is None:
                author = zstring(data[off:off + size])
        elif sig == b"SNAM":
            if description is None:
                description = zstring(data[off:off + size])
        elif sig == b"MAST":
            masters.append(zstring(data[off:off + size]))
        elif sig == b"DATA":
            pass  # u64 per master, informational only
        elif sig == b"ONAM":
            if size % 4:
                problems.append(("warning", "onam_size", f"ONAM is {size} bytes, not a multiple of 4"))
            onam_count += size // 4
        else:
            other.append(signature_text(sig))
    if not hedr_seen:
        problems.append(("error", "missing_hedr", "TES4 has no HEDR subrecord"))
    for severity, code, detail in problems:
        scan.report(severity, code, detail)
    scan.hedr_num_records = num_records
    scan.header = {
        "plugin": scan.plugin,
        "is_master": bool(flags & FLAG_MASTER),
        "flags": flags,
        "version": version,  # the f32 widened to a Python float, unrounded
        "num_records": num_records,
        "next_object_id": None if next_object_id is None else f"0x{next_object_id:08X}",
        "author": author,
        "description": description,
        "masters": masters,
        "overridden_forms": onam_count,
        "other_subrecords": other,
        "file_size": scan.file_size,
    }


def scan_plugin(buf, plugin_name: str, want_records: bool = True) -> PluginScan:
    """Walk the whole plugin once, collecting inventory rows and statistics."""
    size = len(buf)
    scan = PluginScan(plugin_name, size)
    unpack_record = _RECORD_HEADER.unpack_from
    unpack_group = _GROUP_HEADER.unpack_from
    by_type = scan.by_type
    group_names = GROUP_TYPE_NAMES
    records = scan.records
    problems = []

    stack = []          # end offsets of the open groups, innermost last
    pos = 0
    first = True
    while True:
        while stack and pos >= stack[-1]:
            stack.pop()
        limit = stack[-1] if stack else size
        if pos >= limit:
            break  # only reachable with an empty stack: end of file
        if limit - pos < HEADER_SIZE:
            scan.report("error", "header_truncated",
                        f"offset 0x{pos:X}: {limit - pos} bytes left in "
                        f"{'group' if stack else 'file'}, a header needs 24")
            pos = limit
            continue
        sig = buf[pos:pos + 4]

        if sig == b"GRUP":
            _, group_size, label, group_type, _, _, _, _ = unpack_group(buf, pos)
            if first:
                scan.report("error", "missing_tes4", "plugin starts with a GRUP instead of TES4")
                first = False
            if group_size < HEADER_SIZE:
                scan.report("error", "group_size_invalid",
                            f"offset 0x{pos:X}: GRUP size {group_size} < 24; skipping rest of "
                            f"{'enclosing group' if stack else 'file'}")
                pos = limit
                continue
            group_end = pos + group_size
            if group_end > limit:
                scan.report("error", "group_overrun",
                            f"offset 0x{pos:X}: GRUP of {group_size} bytes ends at 0x{group_end:X}, "
                            f"beyond its container end 0x{limit:X}")
                group_end = limit
            scan.group_count += 1
            if 0 <= group_type < len(group_names):
                scan.by_group_type[group_names[group_type]] += 1
            else:
                key = f"unknown_{group_type}"
                scan.by_group_type[key] = scan.by_group_type.get(key, 0) + 1
                scan.report("warning", "unknown_group_type", f"offset 0x{pos:X}: group type {group_type}")
            if not stack:
                if group_type != 0:
                    scan.report("warning", "top_level_group_type",
                                f"offset 0x{pos:X}: top-level group has type {group_type}")
                scan.top_groups.append(signature_text(label))
            stack.append(group_end)
            pos += HEADER_SIZE
            continue

        # A record.
        _, data_size, flags, formid, _, form_version, _ = unpack_record(buf, pos)
        data_start = pos + HEADER_SIZE
        data_end = data_start + data_size
        type_text = signature_text(sig)
        where = f"{type_text} 0x{formid:08X} at 0x{pos:X}"
        if data_end > limit:
            scan.report("error", "record_overrun",
                        f"{where}: data of {data_size} bytes runs past its container end 0x{limit:X}")
            data_end = limit
            truncated = True
        else:
            truncated = False
        scan.record_count += 1
        by_type[type_text] = by_type.get(type_text, 0) + 1
        if flags & FLAG_COMPRESSED:
            scan.compressed_records += 1
        if flags & FLAG_DELETED:
            scan.deleted_records += 1

        if first:
            first = False
            if sig != b"TES4":
                scan.report("error", "missing_tes4", f"first record is {type_text}, not TES4")
            else:
                data, start, end = _record_data(buf, scan, data_start, data_end, flags, where)
                if data is not None:
                    _parse_tes4(buf, scan, flags, data, start, end)
                pos = data_end
                continue
        elif sig == b"TES4":
            scan.report("warning", "extra_tes4", f"{where}: TES4 record after the start of the file")

        if want_records:
            edid = full = None
            if not truncated or not flags & FLAG_COMPRESSED:
                data, start, end = _record_data(buf, scan, data_start, data_end, flags, where)
                if data is not None:
                    edid, full = first_edid_full(data, start, end, problems, where)
                    if problems:
                        for severity, code, detail in problems:
                            scan.report(severity, code, detail)
                        problems.clear()
            records.append({
                "formid": f"0x{formid:08X}",
                "type": type_text,
                "edid": None if edid is None else zstring(edid),
                "full": None if full is None else zstring(full),
                "flags": flags,
                "form_version": form_version,
            })
        pos = data_end

    if scan.header is None and not scan.errors:
        scan.report("error", "missing_tes4", "plugin contains no TES4 record")
    if scan.header is None:
        scan.fatal = size == 0 or not scan.record_count
    return scan


# --------------------------------------------------------------------------- documents

def inventory_document(scan: PluginScan) -> dict:
    # "warnings" is an oracle extension (not in the contract example); the lab compares
    # only /header and /records.
    return {
        "schema": INVENTORY_SCHEMA,
        "plugin": scan.plugin,
        "header": scan.header,
        "errors": scan.errors,
        "warnings": scan.warnings,
        "records": scan.records,
    }


def stats_document(scan: PluginScan) -> dict:
    hedr = scan.hedr_num_records
    return {
        "schema": STATS_SCHEMA,
        "plugin": scan.plugin,
        "records": scan.record_count,
        "groups": scan.group_count,
        "compressed_records": scan.compressed_records,
        "deleted_records": scan.deleted_records,
        "hedr_num_records": hedr,
        "hedr_matches_records_plus_groups_minus_one":
            None if hedr is None else hedr == scan.record_count + scan.group_count - 1,
        "top_groups": scan.top_groups,
        "by_type": dict(sorted(scan.by_type.items())),
        "by_group_type": scan.by_group_type,
    }


def render(doc: dict, list_key=None) -> str:
    """JSON with one element of `list_key` per line (diff-friendly, fast for big lists)."""
    lines = ["{"]
    keys = list(doc)
    for i, key in enumerate(keys):
        value = doc[key]
        comma = "," if i < len(keys) - 1 else ""
        if key == list_key and value:
            lines.append(f" {json.dumps(key)}: [")
            lines.append(",\n".join(" " + json.dumps(item, ensure_ascii=False) for item in value))
            lines.append(f" ]{comma}")
        else:
            lines.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{comma}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Inventory / statistics of an FNV plugin.")
    sub = parser.add_subparsers(dest="command", required=True)
    for name, text in (("inventory", "fnv.esm.inventory/1"), ("stats", "fnv.esm.stats/1")):
        p = sub.add_parser(name, help=f"emit {text}")
        p.add_argument("plugin")
        p.add_argument("-o", "--output", help="write JSON here instead of stdout")
    args = parser.parse_args(argv)
    try:
        with open(args.plugin, "rb") as f:
            buf = f.read()
    except OSError as exc:
        print(f"oracle_esm: cannot read {args.plugin}: {exc}", file=sys.stderr)
        return 1
    name = os.path.basename(args.plugin)
    if args.command == "inventory":
        scan = scan_plugin(buf, name, want_records=True)
        text = render(inventory_document(scan), "records")
    else:
        scan = scan_plugin(buf, name, want_records=False)
        text = render(stats_document(scan))
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    for item in scan.errors:
        print(f"oracle_esm: error {item['code']}: {item['detail']}", file=sys.stderr)
    for item in scan.warnings:
        print(f"oracle_esm: warning {item['code']}: {item['detail']}", file=sys.stderr)
    return 1 if scan.fatal else 0


if __name__ == "__main__":
    sys.exit(main())
