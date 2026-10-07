#!/usr/bin/env python3
"""Independent reference implementation ("oracle") of the Layer 3 world commands.

Written only from docs/spec-notes/esm.md, docs/spec-notes/world.md and the "Layer 3
(world data)" section of docs/cli-contract.md, deliberately without looking at the Rust
implementation, so the parity lab can compare two independent readers. Standard library
only; imports oracle_esm.py (container helpers: zlib tolerance, XXXX subrecords, cp1252
text) and must sit next to it.

What it does, in brief:

  1. Every plugin is memory-mapped and walked once (explicit group stack, no recursion).
     Each record's FormID is mapped to the load order: the stored top byte indexes the
     plugin's own master list, the value equal to the master count means "this plugin"
     (esm.md "FormIDs and load order"). The last definition in load order wins.
  2. For each winning definition the walk remembers where it sits:
       CELL       -> the worldspace whose world-children group (type 1) holds it (or none)
                     and whether it is *directly* in that group (the persistent cell);
       other      -> the cell named by the enclosing persistent / temporary /
                     visible-distant children group (types 8 / 9 / 10) and that type.
     A cell's references are the winning records in its 8/9/10 groups except LAND and
     NAVM; the cell's LAND is picked by the same last-definition-wins rule.
  3. Only the records needed for the requested output are decoded (and decompressed):
     the selected cells, their references, the references' base objects, and for
     `heights` the selected cells' LAND records.

usage: oracle_world.py refs <Data> <plugin>... (--world <edid|0xFORMID> | --interiors) [-o out.json]
       oracle_world.py heights <Data> <plugin>... --world <edid|0xFORMID> [-o out.json]
Plugins are given in load order. `--schema DIR` is accepted for command-line
compatibility with fnvtool and ignored. Exit codes: 0 ok, 1 input unreadable,
2 usage error.
"""
import argparse
import json
import math
import mmap
import os
import struct
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import oracle_esm  # noqa: E402  (same directory)

REFS_SCHEMA = "fnv.world.refs/1"
HEIGHTS_SCHEMA = "fnv.world.heights/1"

HEADER_SIZE = 24
FLAG_DELETED = 0x20
FLAG_INITIALLY_DISABLED = 0x800
FLAG_COMPRESSED = 0x00040000

GT_TOP, GT_WORLD = 0, 1
GT_CELL_CHILDREN = 6
GT_PERSISTENT, GT_TEMPORARY, GT_DISTANT = 8, 9, 10
REF_GROUP_TYPES = (GT_PERSISTENT, GT_TEMPORARY, GT_DISTANT)
NOT_REFERENCES = (b"LAND", b"NAVM")
# world.md §5: NAME / DATA layouts are given for these record types only.
PLACED_TYPES = (b"REFR", b"ACHR", b"ACRE", b"PGRE")

VHGT_SIZE = 4 + 33 * 33 + 3  # 1096
GRID = 33

_RECORD_HEADER = struct.Struct("<4sIII")
_GROUP_HEADER = struct.Struct("<4sII i")
_U32 = struct.Struct("<I")
_F32 = struct.Struct("<f")
_DATA6 = struct.Struct("<6f")
_XCLC = struct.Struct("<ii")
_XESP = struct.Struct("<IB")

# Sentinel world for a world-children group whose label cannot be mapped.
UNMAPPABLE_WORLD = -1


# --------------------------------------------------------------------------- diagnostics

class Diagnostics:
    """Collects (code, detail) problems; prints the first few per code to stderr."""

    MAX_PER_CODE = 20

    def __init__(self):
        self.counts = {}
        self.examples = {}

    def add(self, code: str, detail: str) -> None:
        n = self.counts.get(code, 0)
        self.counts[code] = n + 1
        if n < self.MAX_PER_CODE:
            self.examples.setdefault(code, []).append(detail)

    def emit(self, stream=None) -> None:
        stream = stream or sys.stderr
        for code in sorted(self.counts):
            for detail in self.examples.get(code, []):
                print(f"oracle_world: {code}: {detail}", file=stream)
            extra = self.counts[code] - len(self.examples.get(code, []))
            if extra > 0:
                print(f"oracle_world: {code}: ... and {extra} more", file=stream)


def fid_text(fid):
    return None if fid is None else f"0x{fid:08X}"


def finite(value):
    """f32 widened to a Python float; NaN and infinities become None (JSON null)."""
    return value if math.isfinite(value) else None


# --------------------------------------------------------------------------- plugins

class Plugin:
    def __init__(self, name: str, index: int, buf):
        self.name = name
        self.index = index
        self.buf = buf
        self.masters = []
        self.tes4_end = 0
        # 256 entries: stored top byte -> load-order index << 24, or None (unmappable).
        self.map_table = [None] * 256

    def map_fid(self, fid: int):
        """Stored (file-local) FormID -> load-order FormID, or None if unmappable."""
        hi = self.map_table[fid >> 24]
        if hi is None:
            return None
        return hi | (fid & 0x00FFFFFF)


def open_plugin(path: str, name: str, index: int):
    with open(path, "rb") as f:
        size = os.fstat(f.fileno()).st_size
        if size == 0:
            return Plugin(name, index, b"")
        buf = mmap.mmap(f.fileno(), 0, access=mmap.ACCESS_READ)
    return Plugin(name, index, buf)


def record_data(plugin: Plugin, pos: int, diag: Diagnostics):
    """(data, start, end, flags, sig, stored_fid) of the record at `pos`; data is None when
    the payload cannot be obtained (it is then treated as having no subrecords)."""
    buf = plugin.buf
    sig, size, flags, fid = _RECORD_HEADER.unpack_from(buf, pos)
    start = pos + HEADER_SIZE
    end = start + size
    if end > len(buf):
        end = len(buf)
    if not flags & FLAG_COMPRESSED:
        return buf, start, end, flags, sig, fid
    where = f"{plugin.name} {oracle_esm.signature_text(sig)} 0x{fid:08X} at 0x{pos:X}"
    if end - start < 4:
        diag.add("compressed_record_too_small", where)
        return None, 0, 0, flags, sig, fid
    declared = _U32.unpack_from(buf, start)[0]
    out, problems = oracle_esm.inflate_zlib(buf[start + 4:end], declared)
    fatal = False
    for severity, code, detail in problems:
        diag.add(code, f"{where}: {detail}")
        fatal = fatal or severity == "error"
    if out is None or fatal:
        return None, 0, 0, flags, sig, fid
    return out, 0, len(out), flags, sig, fid


def first_subrecords(data, start, end, wanted, diag: Diagnostics, where: str) -> dict:
    """{sig: bytes} of the first subrecord of each wanted signature (world.md §5: when a
    record has several subrecords with the same signature, the first one counts)."""
    # SPEC-AMBIGUITY: "the first one counts" is applied before the size check, so a
    # first occurrence with the wrong size makes the field absent even when a later
    # occurrence has the right size.
    found = {}
    if data is None:
        return found
    problems = []
    for sig, off, size in oracle_esm.iter_subrecords(data, start, end, problems, where):
        if sig in wanted and sig not in found:
            found[sig] = bytes(data[off:off + size])
            if len(found) == len(wanted):
                break
    for _, code, detail in problems:
        diag.add(code, detail)
    return found


def parse_tes4(plugin: Plugin, diag: Diagnostics) -> bool:
    buf = plugin.buf
    if len(buf) < HEADER_SIZE or buf[0:4] != b"TES4":
        diag.add("missing_tes4", f"{plugin.name}: file does not start with a TES4 record")
        return False
    data, start, end, _, _, _ = record_data(plugin, 0, diag)
    size = _RECORD_HEADER.unpack_from(buf, 0)[1]
    plugin.tes4_end = min(HEADER_SIZE + size, len(buf))
    if data is None:
        return False
    problems = []
    for sig, off, sz in oracle_esm.iter_subrecords(data, start, end, problems, f"{plugin.name} TES4"):
        if sig == b"MAST":
            plugin.masters.append(oracle_esm.zstring(data[off:off + sz]))
    for _, code, detail in problems:
        diag.add(code, detail)
    return True


def build_map_tables(plugins, diag: Diagnostics) -> None:
    by_name = {}
    for p in plugins:
        by_name.setdefault(p.name.lower(), p.index)
    for p in plugins:
        for i, master in enumerate(p.masters):
            if i > 255:
                break
            idx = by_name.get(master.lower())
            if idx is None:
                # SPEC-AMBIGUITY: a master missing from the given load order is not covered
                # by the notes; FormIDs pointing into it are treated as unmappable.
                diag.add("master_not_loaded", f"{p.name}: master {master!r} is not in the load order")
                continue
            if idx >= p.index:
                diag.add("master_loaded_after", f"{p.name}: master {master!r} is loaded after it")
            p.map_table[i] = idx << 24
        own = len(p.masters)
        if own <= 255:
            p.map_table[own] = p.index << 24
        # Top bytes above the master count stay None: esm.md reports such FormIDs as
        # unmappable (e.g. GunRunnersArsenal.esm REFR 0x02000801).


# --------------------------------------------------------------------------- index

class WorldIndex:
    """Winning definition of every load-order FormID, with its group context."""

    def __init__(self, plugins, diag: Diagnostics):
        self.plugins = plugins
        self.diag = diag
        # gid -> (plugin index, header offset, sig, ctx)
        #   CELL:   ctx = (world gid or None, directly_in_world_group: bool)
        #   others: ctx = (cell gid, group type 8/9/10) or None outside reference groups
        self.winners = {}

    # ----------------------------------------------------------------- pass 1
    def index_plugin(self, plugin: Plugin) -> None:
        buf = plugin.buf
        size = len(buf)
        diag = self.diag
        winners = self.winners
        map_table = plugin.map_table
        pidx = plugin.index
        unpack_record = _RECORD_HEADER.unpack_from
        unpack_group = _GROUP_HEADER.unpack_from
        interned = {}
        # state: (world gid or None, innermost group type or None, ref ctx or None)
        root = (None, None, None)
        stack = []  # (end offset, state)
        pos = plugin.tes4_end
        while True:
            while stack and pos >= stack[-1][0]:
                stack.pop()
            if stack:
                limit, state = stack[-1]
            else:
                limit, state = size, root
            if pos >= limit:
                break
            if limit - pos < HEADER_SIZE:
                diag.add("header_truncated", f"{plugin.name} offset 0x{pos:X}: {limit - pos} bytes left")
                pos = limit
                continue
            sig, gsize, label, gtype = unpack_group(buf, pos)
            if sig == b"GRUP":
                if gsize < HEADER_SIZE:
                    diag.add("group_size_invalid", f"{plugin.name} offset 0x{pos:X}: GRUP size {gsize}")
                    pos = limit
                    continue
                gend = pos + gsize
                if gend > limit:
                    diag.add("group_overrun", f"{plugin.name} offset 0x{pos:X}: group ends past its container")
                    gend = limit
                world, _, ref = state
                if gtype == GT_TOP:
                    world, ref = None, None
                elif gtype == GT_WORLD:
                    hi = map_table[label >> 24]
                    if hi is None:
                        diag.add("unmappable_group_label",
                                 f"{plugin.name} offset 0x{pos:X}: world-children group 0x{label:08X}")
                        world = UNMAPPABLE_WORLD
                    else:
                        world = hi | (label & 0xFFFFFF)
                    ref = None
                elif gtype in REF_GROUP_TYPES:
                    hi = map_table[label >> 24]
                    if hi is None:
                        diag.add("unmappable_group_label",
                                 f"{plugin.name} offset 0x{pos:X}: cell group 0x{label:08X}")
                        ref = (None, gtype)
                    else:
                        ref = (hi | (label & 0xFFFFFF), gtype)
                else:
                    ref = None
                stack.append((gend, (world, gtype, ref)))
                pos += HEADER_SIZE
                continue

            # A record.
            _, data_size, _, fid = unpack_record(buf, pos)
            end = pos + HEADER_SIZE + data_size
            if end > limit:
                diag.add("record_overrun", f"{plugin.name} 0x{fid:08X} at 0x{pos:X}: data runs past its container")
                end = limit
            hi = map_table[fid >> 24]
            if hi is None:
                # SPEC-AMBIGUITY: esm.md only says the loader reports such FormIDs as
                # unmappable; this oracle drops the record (it neither defines nor
                # overrides anything).
                diag.add("unmappable_formid",
                         f"{plugin.name} {oracle_esm.signature_text(sig)} 0x{fid:08X} at 0x{pos:X}: dropped")
            else:
                sig = interned.setdefault(sig, sig)
                world, gtype_in, ref = state
                if sig == b"CELL":
                    ctx = (world, gtype_in == GT_WORLD)
                elif ref is not None:
                    if ref[0] is None:
                        ctx = None  # cell label unmappable (already reported)
                    else:
                        ctx = ref
                else:
                    if gtype_in == GT_CELL_CHILDREN:
                        diag.add("record_directly_in_cell_children",
                                 f"{plugin.name} 0x{fid:08X} at 0x{pos:X}: not in a type 8/9/10 group")
                    ctx = None
                # SPEC-AMBIGUITY: a FormID defined twice in one plugin: the later one in
                # file order wins. A reference's cell is the label of its 8/9/10 group
                # (not of the enclosing type 6 group).
                winners[hi | (fid & 0xFFFFFF)] = (pidx, pos, sig, ctx)
            pos = end

    # ----------------------------------------------------------------- derived tables
    def derive(self) -> None:
        """Cells, references per cell, LAND per cell, worldspaces."""
        self.cells = {}          # cell gid -> (world gid or None, direct)
        self.cell_refs = {}      # cell gid -> [ref gid]
        self.cell_land = {}      # cell gid -> (order key, land gid)
        self.worlds = []         # [wrld gid]
        for gid, (pidx, pos, sig, ctx) in self.winners.items():
            if sig == b"CELL":
                self.cells[gid] = ctx
            elif sig == b"WRLD":
                self.worlds.append(gid)
            elif ctx is not None:
                cell = ctx[0]
                if sig == b"LAND":
                    # SPEC-AMBIGUITY: several distinct LAND FormIDs in one cell. "The same
                    # rule picks the cell's LAND" read as: the definition latest in load
                    # order (then latest in file order) wins.
                    key = (pidx, pos)
                    old = self.cell_land.get(cell)
                    if old is not None:
                        self.diag.add("several_lands", f"cell 0x{cell:08X}: LAND 0x{old[1]:08X} and 0x{gid:08X}")
                    if old is None or key > old[0]:
                        self.cell_land[cell] = (key, gid)
                elif sig == b"NAVM":
                    pass
                else:
                    self.cell_refs.setdefault(cell, []).append(gid)
        for cell in self.cell_refs:
            if cell not in self.cells:
                self.diag.add("orphan_cell_group",
                              f"references in groups of cell 0x{cell:08X}, which is not a CELL in the load order")

    # ----------------------------------------------------------------- record access
    def subrecords(self, gid: int, wanted):
        """(sig, flags, {sig: bytes}) for the winning record of `gid`."""
        pidx, pos, _, _ = self.winners[gid]
        plugin = self.plugins[pidx]
        data, start, end, flags, sig, fid = record_data(plugin, pos, self.diag)
        where = f"{plugin.name} {oracle_esm.signature_text(sig)} 0x{fid:08X}"
        return sig, flags, first_subrecords(data, start, end, wanted, self.diag, where)

    def close(self) -> None:
        for plugin in self.plugins:
            if isinstance(plugin.buf, mmap.mmap):
                plugin.buf.close()

    def plugin_of(self, gid: int) -> Plugin:
        return self.plugins[self.winners[gid][0]]

    def resolve_world(self, text: str):
        """Load-order FormID of the worldspace named by EDID (case-insensitive) or 0xFORMID."""
        t = text.strip()
        if t[:2].lower() == "0x":
            try:
                gid = int(t, 16)
            except ValueError:
                return None, f"--world {text!r}: not a hexadecimal FormID"
            w = self.winners.get(gid)
            if w is None or w[2] != b"WRLD":
                return None, f"--world {text}: no WRLD with that FormID in the load order"
            return gid, None
        want = t.lower()
        hits = []
        for gid in self.worlds:
            _, _, subs = self.subrecords(gid, (b"EDID",))
            edid = subs.get(b"EDID")
            if edid is not None and oracle_esm.zstring(edid).lower() == want:
                hits.append(gid)
        if not hits:
            return None, f"--world {text!r}: no WRLD with that Editor ID in the load order"
        if len(hits) > 1:
            # SPEC-AMBIGUITY: duplicate worldspace Editor IDs are not covered; refuse.
            return None, f"--world {text!r}: several WRLD records match: " + ", ".join(fid_text(h) for h in sorted(hits))
        return hits[0], None


# --------------------------------------------------------------------------- decoding

def map_field(index: WorldIndex, owner_gid: int, stored: int, what: str):
    """A FormID field of the record `owner_gid` -> load-order FormID; None for 0 or when
    unmappable (reported)."""
    if stored == 0:
        return None
    plugin = index.plugin_of(owner_gid)
    gid = plugin.map_fid(stored)
    if gid is None:
        # SPEC-AMBIGUITY: an unmappable FormID in a field is not covered; emitted as null.
        index.diag.add("unmappable_field",
                       f"{plugin.name} 0x{owner_gid:08X} {what} 0x{stored:08X}: emitted as null")
        return None
    return gid if gid != 0 else None


# SPEC-AMBIGUITY: "whitespace" is not defined further; read as the Unicode White_Space
# characters that Windows-1252 text can contain (TAB, LF, VT, FF, CR, space, U+0085 NEL,
# U+00A0 NBSP). Python's str.strip() would also remove U+001C..U+001F, so it is not used.
_MODEL_WHITESPACE = "\t\n\x0b\x0c\r \x85\xa0"
_ASCII_LOWER = str.maketrans("ABCDEFGHIJKLMNOPQRSTUVWXYZ", "abcdefghijklmnopqrstuvwxyz")


def normalise_model(path: str):
    """Contract (Layer 3, `world refs`, `model`): surrounding whitespace removed, null if
    that leaves it empty; otherwise ASCII letters lower-cased, '/' -> '\\', leading '\\'
    removed, then prefixed with trees\\ if it ends in .spt, else meshes\\, unless it already
    starts with that prefix."""
    p = path.strip(_MODEL_WHITESPACE)
    if p == "":
        return None
    p = p.translate(_ASCII_LOWER).replace("/", "\\")
    # SPEC-AMBIGUITY: "leading \ removed" read as removing the whole run of leading
    # backslashes (the shipped TREE models have exactly one). A path that consists only of
    # separators is not turned into null (the null rule applies after whitespace removal
    # only), so it becomes the bare prefix.
    p = p.lstrip("\\")
    prefix = "trees\\" if p.endswith(".spt") else "meshes\\"
    if not p.startswith(prefix):
        p = prefix + p
    return p


class BaseCache:
    def __init__(self, index: WorldIndex):
        self.index = index
        self.cache = {}

    def get(self, gid):
        if gid is None:
            return None, None, None
        hit = self.cache.get(gid)
        if hit is not None:
            return hit
        if gid not in self.index.winners:
            out = (None, None, None)   # engine-defined / undefined base
        else:
            sig, _, subs = self.index.subrecords(gid, (b"EDID", b"MODL"))
            edid = subs.get(b"EDID")
            modl = subs.get(b"MODL")
            out = (oracle_esm.signature_text(sig),
                   None if edid is None else oracle_esm.zstring(edid),
                   None if modl is None else normalise_model(oracle_esm.zstring(modl)))
        self.cache[gid] = out
        return out


REF_WANTED = (b"NAME", b"DATA", b"XSCL", b"XESP")
CELL_WANTED = (b"EDID", b"FULL", b"DATA", b"XCLC", b"XCLW")


def decode_ref(index: WorldIndex, bases: BaseCache, gid: int) -> dict:
    _, _, _, ctx = index.winners[gid]
    sig, flags, subs = index.subrecords(gid, REF_WANTED)
    placed = sig in PLACED_TYPES
    # SPEC-AMBIGUITY: world.md §5 gives NAME / DATA layouts for REFR, ACHR, ACRE and PGRE
    # only; for any other record type found in a reference group, base is null and
    # position / rotation are [0, 0, 0]. XSCL / XESP apply to "any reference".
    base = None
    name = subs.get(b"NAME") if placed else None
    if name is not None and len(name) == 4:
        base = map_field(index, gid, _U32.unpack(name)[0], "NAME")
    base_type, base_edid, model = bases.get(base)
    position = [0.0, 0.0, 0.0]
    rotation = [0.0, 0.0, 0.0]
    data = subs.get(b"DATA") if placed else None
    if data is not None and len(data) == 24:
        v = [finite(x) for x in _DATA6.unpack(data)]
        position, rotation = v[:3], v[3:]
    scale = 1.0
    xscl = subs.get(b"XSCL")
    if xscl is not None and len(xscl) == 4:
        scale = finite(_F32.unpack(xscl)[0])
    enable_parent = None
    xesp = subs.get(b"XESP")
    if xesp is not None and len(xesp) == 8:
        ref, eflags = _XESP.unpack_from(xesp)
        parent = map_field(index, gid, ref, "XESP")
        if parent is not None:
            enable_parent = {"ref": fid_text(parent), "opposite": bool(eflags & 1)}
    return {
        "formid": fid_text(gid),
        "type": oracle_esm.signature_text(sig),
        "cell": fid_text(ctx[0]),
        "base": fid_text(base),
        "base_type": base_type,
        "base_edid": base_edid,
        "model": model,
        "position": position,
        "rotation": rotation,
        "scale": scale,
        "flags": flags,
        "persistent": ctx[1] == GT_PERSISTENT,
        "initially_disabled": bool(flags & FLAG_INITIALLY_DISABLED),
        "deleted": bool(flags & FLAG_DELETED),
        "enable_parent": enable_parent,
    }


def decode_cell(index: WorldIndex, gid: int) -> dict:
    world, _ = index.cells[gid]
    _, _, subs = index.subrecords(gid, CELL_WANTED)
    edid = subs.get(b"EDID")
    full = subs.get(b"FULL")
    data = subs.get(b"DATA")
    # SPEC-AMBIGUITY: a CELL without a 1-byte DATA reports interior = false.
    interior = bool(data[0] & 1) if data is not None and len(data) == 1 else False
    xclc = subs.get(b"XCLC")
    grid = list(_XCLC.unpack_from(xclc)) if xclc is not None and len(xclc) in (8, 12) else None
    xclw = subs.get(b"XCLW")
    water = finite(_F32.unpack(xclw)[0]) if xclw is not None and len(xclw) == 4 else None
    land = index.cell_land.get(gid)
    return {
        "formid": fid_text(gid),
        "edid": None if edid is None else oracle_esm.zstring(edid),
        "name": None if full is None else oracle_esm.zstring(full),
        "world": fid_text(world) if world not in (None, UNMAPPABLE_WORLD) else None,
        "grid": grid,
        "interior": interior,
        "water_height": water,
        "refs": len(index.cell_refs.get(gid, ())),
        "land": None if land is None else fid_text(land[1]),
    }


def integrate_vhgt(vhgt: bytes):
    """33 x 33 heights in world units, h[row][col], rows south -> north (world.md §3)."""
    offset = _F32.unpack_from(vhgt, 0)[0]
    steps = struct.unpack_from("<1089b", vhgt, 4)
    rows = []
    row_start = offset
    for r in range(GRID):
        base = r * GRID
        row_start += steps[base]
        value = row_start
        row = [value]
        for c in range(1, GRID):
            value += steps[base + c]
            row.append(value)
        # SPEC-AMBIGUITY: "Multiply by 8 for world units" applied to the integrated value
        # including the offset, i.e. height = 8 * (offset + sum of steps).
        rows.append([8.0 * v for v in row])
    return rows


# --------------------------------------------------------------------------- commands

def select_cells(index: WorldIndex, world):
    """Cells whose winning CELL record sits in `world`'s world-children group, or (world
    None) in no world-children group (interior cells per the contract)."""
    # SPEC-AMBIGUITY: --interiors selects by membership (in no world-children group), as
    # the contract defines interior cells; the DATA interior bit is only reported.
    return sorted(gid for gid, (w, _) in index.cells.items() if w == world)


def refs_document(index: WorldIndex, world, load_order) -> dict:
    cells = select_cells(index, world)
    bases = BaseCache(index)
    cell_docs = [decode_cell(index, gid) for gid in cells]
    ref_ids = []
    for gid in cells:
        ref_ids.extend(index.cell_refs.get(gid, ()))
    ref_ids.sort()
    ref_docs = [decode_ref(index, bases, gid) for gid in ref_ids]
    return {"schema": REFS_SCHEMA, "load_order": list(load_order), "cells": cell_docs, "refs": ref_docs}


def heights_document(index: WorldIndex, world, load_order) -> dict:
    out = []
    for gid in select_cells(index, world):
        _, direct = index.cells[gid]
        if direct:
            continue  # the persistent cell is not a grid cell
        land = index.cell_land.get(gid)
        if land is None:
            continue
        _, _, subs = index.subrecords(gid, (b"XCLC",))
        xclc = subs.get(b"XCLC")
        if xclc is None or len(xclc) not in (8, 12):
            # SPEC-AMBIGUITY: a cell of the worldspace without a grid position cannot be
            # keyed by grid; skipped and reported.
            index.diag.add("cell_without_grid", f"cell 0x{gid:08X} has LAND but no XCLC; skipped")
            continue
        grid = list(_XCLC.unpack_from(xclc))
        _, _, lsubs = index.subrecords(land[1], (b"VHGT",))
        vhgt = lsubs.get(b"VHGT")
        if vhgt is None or len(vhgt) != VHGT_SIZE:
            continue
        h = integrate_vhgt(vhgt)
        flat = [v for row in h for v in row]
        out.append({
            "grid": grid,
            "min": finite(min(flat)) if all(map(math.isfinite, flat)) else None,
            "max": finite(max(flat)) if all(map(math.isfinite, flat)) else None,
            "mean": finite(math.fsum(flat) / len(flat)) if all(map(math.isfinite, flat)) else None,
            "corners": [finite(h[0][0]), finite(h[0][32]), finite(h[32][0]), finite(h[32][32])],
            "centre": finite(h[16][16]),
        })
    out.sort(key=lambda c: (c["grid"][0], c["grid"][1]))
    seen = set()
    for c in out:
        key = tuple(c["grid"])
        if key in seen:
            index.diag.add("duplicate_grid", f"world 0x{world:08X}: grid {list(key)} appears twice")
        seen.add(key)
    return {"schema": HEIGHTS_SCHEMA, "load_order": list(load_order), "world": fid_text(world), "cells": out}


def render(doc: dict) -> str:
    """JSON with one element per line for the top-level lists (diff-friendly)."""
    lines = ["{"]
    keys = list(doc)
    for i, key in enumerate(keys):
        value = doc[key]
        comma = "," if i < len(keys) - 1 else ""
        if isinstance(value, list) and value and isinstance(value[0], dict):
            lines.append(f" {json.dumps(key)}: [")
            lines.append(",\n".join(" " + json.dumps(item, ensure_ascii=False, allow_nan=False)
                                    for item in value))
            lines.append(f" ]{comma}")
        else:
            lines.append(f" {json.dumps(key)}: {json.dumps(value, ensure_ascii=False, allow_nan=False)}{comma}")
    lines.append("}")
    return "\n".join(lines) + "\n"


def load(data_dir: str, plugin_names, diag: Diagnostics):
    """Open, parse and index every plugin. Returns (index, error_message)."""
    plugins = []
    for i, name in enumerate(plugin_names):
        path = os.path.join(data_dir, name)
        try:
            plugin = open_plugin(path, name, i)
        except (OSError, ValueError) as exc:
            return None, f"cannot read {path}: {exc}"
        if not parse_tes4(plugin, diag):
            return None, f"{path}: not a readable plugin (no TES4 header)"
        plugins.append(plugin)
    build_map_tables(plugins, diag)
    index = WorldIndex(plugins, diag)
    for plugin in plugins:
        index.index_plugin(plugin)
    index.derive()
    return index, None


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description="Independent oracle for fnvtool world refs / heights.")
    sub = parser.add_subparsers(dest="command", required=True)
    p_refs = sub.add_parser("refs", help=f"emit {REFS_SCHEMA}")
    p_heights = sub.add_parser("heights", help=f"emit {HEIGHTS_SCHEMA}")
    for p in (p_refs, p_heights):
        p.add_argument("data_dir")
        p.add_argument("plugins", nargs="+")
        p.add_argument("-o", "--output", help="write JSON here instead of stdout")
        p.add_argument("--schema", help="accepted for fnvtool compatibility; ignored")
    sel = p_refs.add_mutually_exclusive_group(required=True)
    sel.add_argument("--world")
    sel.add_argument("--interiors", action="store_true")
    p_heights.add_argument("--world", required=True)
    args = parser.parse_args(argv)

    diag = Diagnostics()
    index, error = load(args.data_dir, args.plugins, diag)
    if index is None:
        diag.emit()
        print(f"oracle_world: {error}", file=sys.stderr)
        return 1
    try:
        return run(args, index, diag)
    finally:
        index.close()


def run(args, index: WorldIndex, diag: Diagnostics) -> int:
    world = None
    if args.world is not None:
        world, error = index.resolve_world(args.world)
        if world is None:
            diag.emit()
            # SPEC-AMBIGUITY: an unknown worldspace is reported with exit code 1 (input
            # could not be processed), not 2 (the command line itself is well formed).
            print(f"oracle_world: {error}", file=sys.stderr)
            return 1
    if args.command == "refs":
        doc = refs_document(index, world, args.plugins)
    else:
        doc = heights_document(index, world, args.plugins)
    text = render(doc)
    if args.output:
        with open(args.output, "w", encoding="utf-8", newline="\n") as f:
            f.write(text)
    else:
        sys.stdout.buffer.write(text.encode("utf-8"))
    diag.emit()
    return 0


if __name__ == "__main__":
    sys.exit(main())
