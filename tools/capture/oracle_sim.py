#!/usr/bin/env python3
"""Independent reference implementation ("oracle") for the Layer 4 simulation documents.

Written only from docs/spec-notes/simulation.md (§3 and §7 "CLI contract") and
docs/spec-notes/esm.md, deliberately without looking at the Rust implementation, so the
parity lab can compare two independent implementations. Standard library only. It imports
the container helpers (tolerant zlib inflate, subrecord iterator with XXXX handling,
cp1252 zstring decoding) from oracle_esm.py, which must sit in the same folder.

usage: oracle_sim.py containers <data_dir> <plugin>... [-o out.json]
       -> fnv.sim.containers/1
Exit codes: 0 ok, 1 input unreadable, 2 usage error.

What it does, in brief:

  Load order    the plugins in the order given; plugin i has load-order index i.
  FormIDs       a stored FormID's top byte indexes the plugin's own MAST list; the value
                equal to the master count means "this plugin". The top byte is replaced by
                the load-order index of the file it designates (masters matched by file
                name, case-insensitively). An index beyond the master count, or naming a
                master that is not in the load order, is unmappable.
  Winners       for every global FormID the last definition in load order (and, within one
                plugin, in file order) wins. A winner with record flag 0x20 (Deleted) means
                "not defined".
  One pass      each plugin is walked once with an explicit stack of group end offsets.
                Only CONT records are parsed in full (EDID, FULL, SCRI, DATA, CNTO); LIGH
                records are parsed for DATA (flags u32 at byte offset 12, bit 1 "Can be
                Carried"); every other record contributes only its type, FormID and flags,
                so compressed non-CONT/LIGH records (e.g. LAND) are never decoded.
  Compression   flag 0x00040000: u32 decoded size + zlib stream; inflated raw after the
                2-byte zlib header so a wrong Adler-32 trailer is tolerated (reported).

Document (fnv.sim.containers/1): "format", "count", "containers" (one object per winning,
non-deleted CONT, ordered by global FormID: formid, edid, name, respawns, script, entries
[{item, count, class}]). Extra top-level keys of this oracle (not compared by the lab):
"load_order", "summary" (counts for cross-checking the spec note) and "warnings"
([{code, detail}], irregular data that did not stop the run).
"""
import argparse
import json
import os
import struct
import sys

CAPTURE_DIR = os.path.dirname(os.path.abspath(__file__))
if CAPTURE_DIR not in sys.path:
    sys.path.insert(0, CAPTURE_DIR)

from oracle_esm import inflate_zlib, iter_subrecords, signature_text, zstring  # noqa: E402

CONTAINERS_FORMAT = "fnv.sim.containers/1"

HEADER_SIZE = 24
FLAG_DELETED = 0x20
FLAG_COMPRESSED = 0x00040000

ITEM_TYPES = frozenset(s.encode("ascii") for s in (
    "WEAP", "ARMO", "AMMO", "ALCH", "BOOK", "KEYM", "MISC", "NOTE", "IMOD", "CHIP", "CCRD", "CMNY",
))
LIGH_CAN_BE_CARRIED = 0x2      # bit 1 of the LIGH DATA flags u32 at byte offset 12
CONT_RESPAWNS = 0x2            # bit 1 of the first CONT DATA byte
ENGINE_BUILTIN_LIMIT = 0x800

_RECORD_HEADER = struct.Struct("<4sIII")       # type, data size, flags, FormID
_GROUP_SIZE = struct.Struct("<I")
_U32 = struct.Struct("<I")
_CNTO = struct.Struct("<Ii")

MAX_WARNINGS_PER_CODE = 50


class Warnings:
    """{code, detail} list with a per-code cap; suppressed totals are reported at the end."""

    def __init__(self):
        self.items = []
        self.counts = {}

    def add(self, code: str, detail: str) -> None:
        n = self.counts.get(code, 0) + 1
        self.counts[code] = n
        if n <= MAX_WARNINGS_PER_CODE:
            self.items.append({"code": code, "detail": detail})

    def finish(self) -> list:
        out = list(self.items)
        for code, n in self.counts.items():
            if n > MAX_WARNINGS_PER_CODE:
                out.append({"code": "warnings_suppressed",
                            "detail": f"{code}: {n - MAX_WARNINGS_PER_CODE} more not listed ({n} total)"})
        return out


class InputError(Exception):
    """A plugin cannot be read at all (exit code 1)."""


# --------------------------------------------------------------------------- helpers

def fid_text(value: int) -> str:
    return f"0x{value:08X}"


def record_data(buf, pos: int, end: int, flags: int, where: str, warnings: Warnings):
    """(data, start, end) of a record's subrecord area, decompressing if needed;
    (None, 0, 0) when the data cannot be obtained."""
    if not flags & FLAG_COMPRESSED:
        return buf, pos, end
    if end - pos < 4:
        warnings.add("compressed_record_too_small", f"{where}: {end - pos} bytes, needs a 4-byte size")
        return None, 0, 0
    declared = _U32.unpack_from(buf, pos)[0]
    out, problems = inflate_zlib(buf[pos + 4:end], declared)
    fatal = out is None
    for severity, code, detail in problems:
        warnings.add(code, f"{where}: {detail}")
        fatal = fatal or severity == "error"
    if fatal:
        return None, 0, 0
    return out, 0, len(out)


def subrecords(data, start: int, end: int, where: str, warnings: Warnings):
    problems = []
    items = list(iter_subrecords(data, start, end, problems, where))
    for _, code, detail in problems:
        warnings.add(code, detail)
    return items


def parse_tes4_masters(buf, flags: int, data_end: int, plugin: str, warnings: Warnings):
    data, start, end = record_data(buf, HEADER_SIZE, data_end, flags, f"{plugin} TES4", warnings)
    if data is None:
        raise InputError(f"{plugin}: TES4 header data cannot be decoded")
    return [zstring(data[off:off + size])
            for sig, off, size in subrecords(data, start, end, f"{plugin} TES4", warnings)
            if sig == b"MAST"]


# --------------------------------------------------------------------------- load order

class Plugin:
    def __init__(self, name: str, index: int, buf, masters: list, remap: list):
        self.name = name
        self.index = index
        self.buf = buf
        self.masters = masters
        self.remap = remap          # top byte -> load-order index (None = unmappable)

    def globalise(self, local: int):
        """Global FormID for a stored FormID of this plugin, or None if unmappable."""
        top = local >> 24
        if top >= len(self.remap):
            return None
        target = self.remap[top]
        if target is None:
            return None
        return (target << 24) | (local & 0x00FFFFFF)


def find_plugin_file(data_dir: str, name: str):
    path = os.path.join(data_dir, name)
    if os.path.isfile(path):
        return path
    try:  # case-insensitive fallback for case-sensitive file systems
        lowered = name.lower()
        for entry in os.listdir(data_dir):
            if entry.lower() == lowered:
                return os.path.join(data_dir, entry)
    except OSError:
        pass
    return path


def open_plugin(data_dir: str, name: str, index: int, load_order_lower: list, warnings: Warnings) -> Plugin:
    path = find_plugin_file(data_dir, name)
    try:
        with open(path, "rb") as f:
            buf = f.read()
    except OSError as exc:
        raise InputError(f"cannot read {path}: {exc}") from exc
    if len(buf) < HEADER_SIZE or buf[:4] != b"TES4":
        raise InputError(f"{name}: does not start with a TES4 record")
    _, data_size, flags, _ = _RECORD_HEADER.unpack_from(buf, 0)
    data_end = HEADER_SIZE + data_size
    if data_end > len(buf):
        raise InputError(f"{name}: TES4 record runs past the end of the file")
    masters = parse_tes4_masters(buf, flags, data_end, name, warnings)
    remap = []
    for master in masters:
        lowered = master.lower()
        if lowered in load_order_lower:
            target = load_order_lower.index(lowered)
            if target >= index:
                warnings.add("master_loaded_after",
                             f"{name}: master {master} is at load-order index {target}, not before {index}")
            remap.append(target)
        else:
            warnings.add("master_not_loaded", f"{name}: master {master} is not in the load order; "
                                              f"FormIDs with top byte {len(remap)} are unmappable")
            remap.append(None)
    remap.append(index)
    return Plugin(name, index, buf, masters, remap)


# --------------------------------------------------------------------------- scanning

def scan_plugin(plugin: Plugin, forms: dict, warnings: Warnings) -> None:
    """Walk one plugin and record every record as the current winner of its global FormID.

    forms[global] = (sig, deleted, extra); extra is the "Can be Carried" bool for LIGH,
    (plugin, data, start, end) for CONT (data None if it could not be decoded), else None.
    """
    buf = plugin.buf
    size = len(buf)
    unpack_record = _RECORD_HEADER.unpack_from
    unpack_size = _GROUP_SIZE.unpack_from
    remap = plugin.remap
    nremap = len(remap)
    sig_cache = {}
    name = plugin.name

    stack = []
    # Skip the TES4 record (already parsed); its FormID 0 is not a form.
    pos = HEADER_SIZE + unpack_record(buf, 0)[1]
    while True:
        while stack and pos >= stack[-1]:
            stack.pop()
        limit = stack[-1] if stack else size
        if pos >= limit:
            break
        if limit - pos < HEADER_SIZE:
            warnings.add("header_truncated", f"{name} offset 0x{pos:X}: {limit - pos} bytes left, "
                                             f"a header needs 24")
            pos = limit
            continue
        sig = buf[pos:pos + 4]
        if sig == b"GRUP":
            group_size = unpack_size(buf, pos + 4)[0]
            if group_size < HEADER_SIZE:
                warnings.add("group_size_invalid", f"{name} offset 0x{pos:X}: GRUP size {group_size}; "
                                                   f"skipping the rest of the enclosing container")
                pos = limit
                continue
            group_end = pos + group_size
            if group_end > limit:
                warnings.add("group_overrun", f"{name} offset 0x{pos:X}: GRUP ends at 0x{group_end:X}, "
                                              f"beyond its container end 0x{limit:X}")
                group_end = limit
            stack.append(group_end)
            pos += HEADER_SIZE
            continue

        _, data_size, flags, local = unpack_record(buf, pos)
        data_start = pos + HEADER_SIZE
        data_end = data_start + data_size
        if data_end > limit:
            warnings.add("record_overrun", f"{name} {signature_text(sig)} 0x{local:08X} at 0x{pos:X}: "
                                           f"data runs past its container end")
            data_end = limit
        pos = data_end
        sig = sig_cache.setdefault(sig, sig)

        top = local >> 24
        target = remap[top] if top < nremap else None
        if target is None:
            warnings.add("unmappable_formid", f"{name} {signature_text(sig)} stored as 0x{local:08X}: "
                                              f"master index {top} is outside the plugin's "
                                              f"{nremap - 1} master(s) or not loaded; record ignored")
            continue
        formid = (target << 24) | (local & 0x00FFFFFF)
        deleted = bool(flags & FLAG_DELETED)
        extra = None
        if not deleted:
            if sig == b"CONT":
                where = f"{name} CONT 0x{local:08X}"
                data, start, end = record_data(buf, data_start, data_end, flags, where, warnings)
                if data is buf:  # copy the few CONT payloads so the file buffer can be freed
                    data, start, end = bytes(buf[start:end]), 0, end - start
                extra = (plugin, data, start, end)
            elif sig == b"LIGH":
                extra = light_can_be_carried(buf, data_start, data_end, flags,
                                             f"{name} LIGH 0x{local:08X}", warnings)
        forms[formid] = (sig, deleted, extra)


def light_can_be_carried(buf, start: int, end: int, flags: int, where: str, warnings: Warnings) -> bool:
    data, start, end = record_data(buf, start, end, flags, where, warnings)
    if data is None:
        return False
    for sig, off, size in subrecords(data, start, end, where, warnings):
        if sig == b"DATA":
            if size < 16:
                warnings.add("ligh_data_short", f"{where}: DATA is {size} bytes, flags need 16; not carried")
                return False
            return bool(_U32.unpack_from(data, off + 12)[0] & LIGH_CAN_BE_CARRIED)
    warnings.add("ligh_no_data", f"{where}: no DATA subrecord; not carried")
    return False


# --------------------------------------------------------------------------- containers

def classify(item: int, forms: dict) -> str:
    winner = forms.get(item)
    if winner is None or winner[1]:
        return "engine_builtin" if item < ENGINE_BUILTIN_LIMIT else "missing"
    sig, _, extra = winner
    if sig in ITEM_TYPES:
        return "item"
    if sig == b"LIGH":
        return "item" if extra else "other:LIGH"
    if sig == b"LVLI":
        return "leveled_item"
    return "other:" + signature_text(sig)


def container_object(formid: int, extra, forms: dict, warnings: Warnings, stats: dict) -> dict:
    plugin, data, start, end = extra
    obj = {"formid": fid_text(formid), "edid": None, "name": None, "respawns": False,
           "script": None, "entries": []}
    where = f"{plugin.name} CONT {fid_text(formid)}"
    if data is None:
        warnings.add("container_data_unreadable", f"{where}: data could not be decoded; "
                                                  f"listed with null fields and no entries")
        return obj
    seen_data = seen_script = False
    cnto = 0
    entries = obj["entries"]
    for sig, off, size in subrecords(data, start, end, where, warnings):
        if sig == b"EDID":
            if obj["edid"] is None:
                obj["edid"] = zstring(data[off:off + size])
        elif sig == b"FULL":
            if obj["name"] is None:
                obj["name"] = zstring(data[off:off + size])
        elif sig == b"SCRI":
            if seen_script:
                continue
            seen_script = True
            if size < 4:
                warnings.add("scri_short", f"{where}: SCRI is {size} bytes; script null")
                continue
            local = _U32.unpack_from(data, off)[0]
            script = plugin.globalise(local)
            if script is None:
                warnings.add("scri_unmappable", f"{where}: SCRI 0x{local:08X} unmappable; script null")
            else:
                if local == 0:
                    warnings.add("scri_null", f"{where}: SCRI present with FormID 0")
                obj["script"] = fid_text(script)
        elif sig == b"DATA":
            if seen_data:
                continue
            seen_data = True
            if size >= 1:
                obj["respawns"] = bool(data[off] & CONT_RESPAWNS)
        elif sig == b"CNTO":
            cnto += 1
            if size < 8:
                warnings.add("cnto_short", f"{where}: CNTO is {size} bytes, needs 8; entry skipped")
                continue
            if size != 8:
                warnings.add("cnto_size", f"{where}: CNTO is {size} bytes, using the first 8")
            local, count = _CNTO.unpack_from(data, off)
            if local == 0:
                stats["null_items"] += 1
                continue
            item = plugin.globalise(local)
            if item is None:
                stats["unmappable_items"] += 1
                warnings.add("cnto_unmappable", f"{where}: item 0x{local:08X} unmappable; entry omitted")
                continue
            cls = classify(item, forms)
            stats["by_class"][cls] = stats["by_class"].get(cls, 0) + 1
            entries.append({"item": fid_text(item), "count": count, "class": cls})
    if not cnto:
        stats["without_cnto"] += 1
    if obj["respawns"]:
        stats["respawns"] += 1
    stats["entries"] += len(entries)
    return obj


def build_containers(data_dir: str, plugins: list, warnings: Warnings) -> dict:
    load_order_lower = [p.lower() for p in plugins]
    forms = {}
    for index, name in enumerate(plugins):
        plugin = open_plugin(data_dir, name, index, load_order_lower, warnings)
        scan_plugin(plugin, forms, warnings)
        plugin.buf = None  # CONT payloads were copied; free the file buffer
    stats = {"forms_defined": sum(1 for v in forms.values() if not v[1]),
             "forms_deleted": sum(1 for v in forms.values() if v[1]),
             "respawns": 0, "without_cnto": 0, "entries": 0, "null_items": 0,
             "unmappable_items": 0, "by_class": {}}
    containers = []
    for formid in sorted(forms):
        sig, deleted, extra = forms[formid]
        if sig == b"CONT" and not deleted:
            containers.append(container_object(formid, extra, forms, warnings, stats))
    stats["by_class"] = dict(sorted(stats["by_class"].items()))
    return {
        "format": CONTAINERS_FORMAT,
        "count": len(containers),
        "load_order": list(plugins),
        "summary": stats,
        "warnings": warnings.finish(),
        "containers": containers,
    }


# --------------------------------------------------------------------------- output / CLI

def render(doc: dict, list_key=None) -> str:
    """JSON with one element of `list_key` per line (as oracle_esm.render)."""
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
    parser = argparse.ArgumentParser(description="Layer 4 simulation oracle.")
    sub = parser.add_subparsers(dest="command", required=True)
    p = sub.add_parser("containers", help=f"emit {CONTAINERS_FORMAT}")
    p.add_argument("data_dir")
    p.add_argument("plugins", nargs="+", metavar="plugin")
    p.add_argument("-o", "--output", help="write JSON here instead of stdout")
    args = parser.parse_args(argv)

    lowered = [name.lower() for name in args.plugins]
    if len(set(lowered)) != len(lowered):
        print("oracle_sim: a plugin is listed twice in the load order", file=sys.stderr)
        return 2
    if len(args.plugins) > 255:
        print("oracle_sim: more than 255 plugins", file=sys.stderr)
        return 2
    if not os.path.isdir(args.data_dir):
        print(f"oracle_sim: data directory not found: {args.data_dir}", file=sys.stderr)
        return 1
    warnings = Warnings()
    try:
        doc = build_containers(args.data_dir, args.plugins, warnings)
    except InputError as exc:
        print(f"oracle_sim: {exc}", file=sys.stderr)
        return 1
    text = render(doc, "containers")
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    for item in doc["warnings"]:
        print(f"oracle_sim: warning {item['code']}: {item['detail']}", file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
