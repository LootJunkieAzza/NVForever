#!/usr/bin/env python3
"""Independent reference reader ("oracle") for BSA archives, version 104 (Fallout 3 / New Vegas).

Written only from docs/spec-notes/bsa.md and docs/cli-contract.md, deliberately without
looking at the Rust implementation, so the parity lab can compare two independent readers.
Standard library only; the file is self-contained so it can be copied to another machine.

Format in brief (all integers little-endian):

  header (36 bytes)   "BSA\\0", version (104), folder-record offset (36), archive flags,
                      folder count, file count, total folder name length, total file name
                      length, file flags (u16) + 2 padding bytes
  folder records      16 bytes each: name hash u64, file count u32, offset u32
                      (offset = position of the folder's file-record block + total file
                      name length)
  file-record blocks  one per folder, in folder order: [length-prefixed folder path if
                      archive flag 0x1] then 16-byte file records: name hash u64,
                      size u32 (bit 30 toggles the archive default compression, bits 0-29
                      are the stored size), absolute data offset u32
  file-name block     [if archive flag 0x2] NUL-terminated file names, record order
  file data           compressed entries are a u32 decoded size followed by a zlib stream

Name hash: see bsa_hash() below.

The reader is tolerant: it never raises on odd data. Anything irregular goes into the
"warnings" (data still usable) or "errors" (something could not be read) arrays as
{"code", "detail"} objects.

usage: oracle_bsa.py <archive.bsa> [--no-content-hash] [-o out.json]
Output schema: fnv.bsa.listing/1. Exit codes: 0 ok, 1 input unreadable, 2 usage error.
"""
import argparse
import hashlib
import json
import os
import struct
import sys
import zlib

SCHEMA = "fnv.bsa.listing/1"

HEADER_SIZE = 36
FOLDER_RECORD_SIZE = 16
FILE_RECORD_SIZE = 16

ARCHIVE_FOLDER_NAMES = 0x1
ARCHIVE_FILE_NAMES = 0x2
ARCHIVE_COMPRESSED_DEFAULT = 0x4
ARCHIVE_XBOX = 0x40
ARCHIVE_EMBEDDED_NAMES = 0x100

SIZE_COMPRESSION_TOGGLE = 0x40000000
SIZE_BIT31 = 0x80000000
SIZE_MASK = 0x3FFFFFFF

# Extension markers OR-ed into the low 32 bits of a file-name hash.
EXTENSION_MARKERS = {
    b".kf": 0x80,
    b".nif": 0x8000,
    b".dds": 0x8080,
    b".wav": 0x80000000,
}

_HEADER = struct.Struct("<4sIIIIIIIHH")
_FOLDER_RECORD = struct.Struct("<QII")
_FILE_RECORD = struct.Struct("<QII")
_U32 = struct.Struct("<I")


# --------------------------------------------------------------------------- text / hash

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


def _rolling_hash(data: bytes) -> int:
    h = 0
    for c in data:
        h = (h * 0x1003F + c) & 0xFFFFFFFF
    return h


def bsa_hash(name: bytes, is_folder: bool) -> int:
    """64-bit BSA name hash (spec-notes/bsa.md, "Name hash").

    The name is lower-cased (ASCII) and '/' is normalised to '\\'. Files are split into
    stem + extension at the last '.', folders have no extension.
      low 32 bits  = last stem char | second-to-last << 8 (only if stem length > 2)
                     | stem length << 16 | first char << 24 | extension marker
      high 32 bits = rolling(stem[1 : len-2]) + rolling(extension incl. dot)  (mod 2^32)
    where rolling(s) iterates h = h * 0x1003F + c (mod 2^32) from h = 0.
    """
    n = name.replace(b"/", b"\\").lower()
    if is_folder:
        stem, ext = n, b""
    else:
        dot = n.rfind(b".")
        stem, ext = (n, b"") if dot < 0 else (n[:dot], n[dot:])
    low = 0
    if stem:
        low = stem[-1] | (len(stem) << 16) | (stem[0] << 24)
        if len(stem) > 2:
            low |= stem[-2] << 8
    low = (low | EXTENSION_MARKERS.get(ext, 0)) & 0xFFFFFFFF
    high = (_rolling_hash(stem[1:len(stem) - 2]) + _rolling_hash(ext)) & 0xFFFFFFFF
    return (high << 32) | low


# --------------------------------------------------------------------------- zlib

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


# --------------------------------------------------------------------------- reader

class Listing:
    """Accumulates the listing document while parsing."""

    def __init__(self, archive_name: str):
        self.archive = archive_name
        self.header = None
        self.warnings = []
        self.errors = []
        self.files = []
        self.fatal = False

    def warn(self, code: str, detail: str) -> None:
        self.warnings.append({"code": code, "detail": detail})

    def error(self, code: str, detail: str) -> None:
        self.errors.append({"code": code, "detail": detail})

    def document(self) -> dict:
        return {
            "schema": SCHEMA,
            "archive": self.archive,
            "header": self.header,
            "warnings": self.warnings,
            "errors": self.errors,
            "files": self.files,
        }


def _decode_entry(buf, listing: Listing, path: str, offset: int, stored: int,
                  compressed: bool, embedded_names: bool):
    """Return the decoded contents of one file (bytes) or None if unreadable."""
    end = offset + stored
    if offset > len(buf) or end > len(buf):
        listing.error("data_out_of_bounds",
                      f"{path}: data {offset}..{end} beyond archive end {len(buf)}")
        return None
    data = memoryview(buf)[offset:end]
    if embedded_names:
        # Flag 0x100 (set on the two texture archives): data begins with a length-prefixed full path.
        if len(data) < 1 or 1 + data[0] > len(data):
            listing.error("embedded_name_truncated", f"{path}: embedded name runs past data")
            return None
        data = data[1 + data[0]:]
    if not compressed:
        return data
    if len(data) < 4:
        listing.error("compressed_block_too_small",
                      f"{path}: compressed block is {len(data)} bytes, needs a 4-byte size")
        return None
    declared = _U32.unpack_from(data, 0)[0]
    stream = data[4:]
    if declared == 0 and len(stream) == 0:
        listing.warn("compressed_block_empty",
                     f"{path}: compressed entry with declared size 0 and no zlib data")
        return b""
    out, problems = inflate_zlib(stream, declared)
    for severity, code, detail in problems:
        (listing.error if severity == "error" else listing.warn)(code, f"{path}: {detail}")
    return out


def read_listing(buf, archive_name: str, content_hash: bool = True) -> Listing:
    """Parse a whole archive held in `buf` (bytes-like) and build the listing."""
    listing = Listing(archive_name)
    size = len(buf)
    if size < HEADER_SIZE:
        listing.error("truncated_header", f"file is {size} bytes, header needs {HEADER_SIZE}")
        listing.fatal = True
        return listing
    (magic, version, folder_offset, archive_flags, folder_count, file_count,
     total_folder_name_length, total_file_name_length, file_flags, file_flags_pad) = \
        _HEADER.unpack_from(buf, 0)
    if magic != b"BSA\0":
        listing.error("bad_magic", f"magic {magic!r}, expected b'BSA\\x00'")
        listing.fatal = True
        return listing
    listing.header = {
        "version": version,
        "archive_flags": archive_flags,
        "file_flags": file_flags,
        "folder_count": folder_count,
        "file_count": file_count,
        "total_folder_name_length": total_folder_name_length,
        "total_file_name_length": total_file_name_length,
    }
    if version != 104:
        listing.warn("unexpected_version", f"version {version}, expected 104")
    if folder_offset != HEADER_SIZE:
        listing.warn("unexpected_folder_offset", f"folder record offset {folder_offset}, expected 36")
    if file_flags_pad != 0:
        listing.warn("file_flags_padding_nonzero",
                     f"the 2 bytes after file flags are 0x{file_flags_pad:04x}")
    if archive_flags & ARCHIVE_XBOX:
        listing.error("xbox_archive", "archive flag 0x40 (Xbox 360, big-endian) is not supported; "
                                      "values below were read as little-endian")
    has_folder_names = bool(archive_flags & ARCHIVE_FOLDER_NAMES)
    has_file_names = bool(archive_flags & ARCHIVE_FILE_NAMES)
    compressed_default = bool(archive_flags & ARCHIVE_COMPRESSED_DEFAULT)
    embedded_names = bool(archive_flags & ARCHIVE_EMBEDDED_NAMES)
    if embedded_names:
        listing.warn("embedded_names_flag", "archive flag 0x100 set (never seen in FNV); "
                                            "skipping a length-prefixed path before each file's data")

    # Folder records.
    folders = []
    pos = folder_offset
    for i in range(folder_count):
        if pos + FOLDER_RECORD_SIZE > size:
            listing.error("folder_records_truncated",
                          f"folder record {i} of {folder_count} at {pos} runs past end of file")
            break
        folders.append(_FOLDER_RECORD.unpack_from(buf, pos))
        pos += FOLDER_RECORD_SIZE
    for a, b in zip(folders, folders[1:]):
        if b[0] <= a[0]:
            listing.warn("folders_not_sorted", f"folder hash 0x{b[0]:016x} follows 0x{a[0]:016x}")
            break

    # File-record blocks (read sequentially in folder order; stored offsets cross-checked).
    entries = []  # (folder_name_bytes, folder_hash, file_hash, raw_size, data_offset)
    folder_name_bytes_total = 0
    for index, (folder_hash, count, stored_offset) in enumerate(folders):
        block_start = pos
        if stored_offset - total_file_name_length != block_start:
            listing.warn("folder_offset_mismatch",
                         f"folder {index} (hash 0x{folder_hash:016x}): stored offset "
                         f"{stored_offset} - total file name length {total_file_name_length} "
                         f"!= block position {block_start}")
        folder_name = b""
        if has_folder_names:
            if pos >= size:
                listing.error("folder_name_truncated", f"folder {index}: name at {pos} past end of file")
                break
            length = buf[pos]
            raw = bytes(buf[pos + 1:pos + 1 + length])
            if len(raw) < length:
                listing.error("folder_name_truncated", f"folder {index}: name runs past end of file")
                break
            pos += 1 + length
            if length == 0 or raw[-1] != 0 or raw.find(b"\0") != length - 1:
                listing.warn("noncanonical_folder_name",
                             f"folder {index}: length byte {length}, bytes {raw!r} "
                             f"(expected a NUL-terminated name counted by the length byte)")
            folder_name = raw.split(b"\0", 1)[0]
            # The spec defines the header total as sum(name length + 1), i.e. counting
            # one NUL per folder whether or not the stored string actually has it.
            folder_name_bytes_total += len(folder_name) + 1
            computed = bsa_hash(folder_name, is_folder=True)
            if computed != folder_hash:
                listing.warn("folder_hash_mismatch",
                             f"folder {decode_cp1252(folder_name)!r}: stored 0x{folder_hash:016x}, "
                             f"computed 0x{computed:016x}")
        previous_hash = None
        for j in range(count):
            if pos + FILE_RECORD_SIZE > size:
                listing.error("file_records_truncated",
                              f"folder {index}: file record {j} of {count} runs past end of file")
                break
            file_hash, raw_size, data_offset = _FILE_RECORD.unpack_from(buf, pos)
            pos += FILE_RECORD_SIZE
            if previous_hash is not None and file_hash <= previous_hash:
                listing.warn("files_not_sorted",
                             f"folder {index}: file hash 0x{file_hash:016x} follows 0x{previous_hash:016x}")
            previous_hash = file_hash
            entries.append((folder_name, folder_hash, file_hash, raw_size, data_offset))
        else:
            continue
        break  # inner loop hit truncation

    if has_folder_names and folder_name_bytes_total != total_folder_name_length:
        listing.warn("total_folder_name_length_mismatch",
                     f"header says {total_folder_name_length}, folder name bytes sum to "
                     f"{folder_name_bytes_total}")
    if sum(f[1] for f in folders) != file_count:
        listing.warn("file_count_mismatch",
                     f"header file count {file_count}, folder records sum to {sum(f[1] for f in folders)}")

    # File-name block.
    names = []
    if has_file_names:
        block = bytes(buf[pos:pos + total_file_name_length])
        if len(block) < total_file_name_length:
            listing.error("file_names_truncated",
                          f"name block of {total_file_name_length} bytes at {pos} runs past end of file")
        names = block.split(b"\0")
        if names and names[-1] == b"":
            names.pop()
        else:
            listing.warn("file_names_unterminated", "file-name block does not end with NUL")
        if len(names) != len(entries):
            listing.warn("file_name_count_mismatch",
                         f"{len(names)} names in the name block, {len(entries)} file records")
        pos += total_file_name_length
    else:
        listing.warn("file_names_absent", "archive flag 0x2 clear; paths use the hash as name")

    # Files.
    names_end = pos
    expected_offset = pos  # canonical layout: data follows the name block, no gaps
    gaps = 0
    last_data_end = pos
    for k, (folder_name, folder_hash, file_hash, raw_size, data_offset) in enumerate(entries):
        if k < len(names):
            name = names[k]
            computed = bsa_hash(name, is_folder=False)
            if computed != file_hash:
                listing.warn("file_hash_mismatch",
                             f"{decode_cp1252(folder_name)}\\{decode_cp1252(name)}: stored "
                             f"0x{file_hash:016x}, computed 0x{computed:016x}")
            name_text = decode_cp1252(name)
        else:
            name_text = f"#0x{file_hash:016x}"
        # Contract: an empty folder name yields the bare file name (no leading '\\').
        path = (f"{decode_cp1252(folder_name)}\\{name_text}"
                if has_folder_names and folder_name else name_text)
        stored = raw_size & SIZE_MASK
        compressed = compressed_default != bool(raw_size & SIZE_COMPRESSION_TOGGLE)
        if raw_size & SIZE_BIT31:
            listing.warn("size_bit31_set", f"{path}: size field 0x{raw_size:08x} has bit 31 set")
        entry = {
            "path": path,
            "name_hash": f"0x{file_hash:016x}",
            "folder_hash": f"0x{folder_hash:016x}",
            "stored_size": stored,
            "compressed": compressed,
        }
        if content_hash:
            data = _decode_entry(buf, listing, path, data_offset, stored, compressed, embedded_names)
            entry["size"] = None if data is None else len(data)
            entry["sha256"] = None if data is None else hashlib.sha256(data).hexdigest()
        elif data_offset + stored > size:
            listing.error("data_out_of_bounds",
                          f"{path}: data {data_offset}..{data_offset + stored} beyond archive end {size}")
        if data_offset != expected_offset:
            gaps += 1
            if gaps == 1:
                listing.warn("data_not_contiguous",
                             f"{path}: data at {data_offset}, canonical layout expects "
                             f"{expected_offset} (name block ends at {names_end})")
        expected_offset = data_offset + stored
        last_data_end = max(last_data_end, data_offset + stored)
        listing.files.append(entry)

    if gaps > 1:
        listing.warn("data_not_contiguous", f"{gaps} files in total are not at their canonical offset")
    if last_data_end < size:
        listing.warn("trailing_bytes", f"{size - last_data_end} bytes after the last file's data")
    return listing


# --------------------------------------------------------------------------- output

def render(doc: dict, list_key: str) -> str:
    """JSON with one list element per line (diff-friendly, fast for big listings)."""
    lines = ["{"]
    keys = list(doc)
    for i, key in enumerate(keys):
        value = doc[key]
        comma = "," if i < len(keys) - 1 else ""
        if key == list_key and value:
            lines.append(f" {json.dumps(key)}: [")
            body = [" " + json.dumps(item, ensure_ascii=False) for item in value]
            lines.append(",\n".join(body))
            lines.append(f" ]{comma}")
        else:
            lines.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False)}{comma}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="List a BSA archive as fnv.bsa.listing/1 JSON.")
    parser.add_argument("archive")
    parser.add_argument("--no-content-hash", action="store_true",
                        help="skip decoding; omit size and sha256")
    parser.add_argument("-o", "--output", help="write JSON here instead of stdout")
    args = parser.parse_args(argv)
    try:
        with open(args.archive, "rb") as f:
            buf = f.read()
    except OSError as exc:
        print(f"oracle_bsa: cannot read {args.archive}: {exc}", file=sys.stderr)
        return 1
    listing = read_listing(buf, os.path.basename(args.archive), not args.no_content_hash)
    text = render(listing.document(), "files")
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    for item in listing.errors:
        print(f"oracle_bsa: error {item['code']}: {item['detail']}", file=sys.stderr)
    return 1 if listing.fatal else 0


if __name__ == "__main__":
    sys.exit(main())
