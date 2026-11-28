#!/usr/bin/env python3
"""Pure-Python clean-room `.nvse` co-save binary parser and A/B differ.

Written strictly from clean-room A/B staging captures documented in
`docs/spec-notes/nvse-cosaves.md` (stdlib only, read-only).
"""

from __future__ import annotations

import argparse
import json
import struct
import sys
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any

MAGIC = b"NVSE"
FORMAT_VERSION = 1

PLUGIN_NVSE = 0x00001400
PLUGIN_JIP = 0x00002200
PLUGIN_JOHNNYGUITAR = 0x00003100
PLUGIN_SHOWOFF = 0x00003C93

KNOWN_CHUNK_KEYS: set[tuple[int, bytes, int]] = {
    (PLUGIN_NVSE, b"SDOM", 0),
    (PLUGIN_NVSE, b"SVRA", 2),
    (PLUGIN_NVSE, b"RVRA", 2),
    (PLUGIN_NVSE, b"EVRA", 2),
    (PLUGIN_NVSE, b"SVTS", 0),
    (PLUGIN_NVSE, b"EVTS", 0),
    (PLUGIN_JIP, b"JPAV", 10),
    (PLUGIN_JIP, b"JPRM", 10),
    (PLUGIN_JIP, b"JPLR", 9),
    (PLUGIN_JIP, b"JPSV", 9),
    (PLUGIN_JOHNNYGUITAR, b"SMGJ", 1),
    (PLUGIN_SHOWOFF, b"OSMS", 10),
    (PLUGIN_SHOWOFF, b"SOAT", 1),
}


class NvseParseError(ValueError):
    """Raised when an `.nvse` container or decoded chunk is malformed."""


class _Cursor:
    def __init__(self, data: bytes, pos: int = 0) -> None:
        self.data = data
        self.pos = pos

    def remaining(self) -> int:
        return len(self.data) - self.pos

    def take(self, n: int) -> bytes:
        if n < 0 or self.pos + n > len(self.data):
            raise NvseParseError(f"truncated: need {n} bytes at {self.pos}, remaining {self.remaining()}")
        out = self.data[self.pos : self.pos + n]
        self.pos += n
        return out

    def u8(self) -> int:
        return self.take(1)[0]

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def i32(self) -> int:
        return struct.unpack("<i", self.take(4))[0]

    def f64(self) -> float:
        return struct.unpack("<d", self.take(8))[0]

    def str_u8(self) -> str:
        n = self.u8()
        return self.take(n).decode("cp1252", errors="replace")

    def str_u16(self) -> str:
        n = self.u16()
        return self.take(n).decode("cp1252", errors="replace")


@dataclass(frozen=True)
class NvseChunk:
    tag: bytes
    version: int
    data: bytes

    @property
    def tag_ascii(self) -> str:
        return self.tag.decode("ascii", errors="replace")


@dataclass(frozen=True)
class NvsePlugin:
    opcode: int
    chunks: tuple[NvseChunk, ...]


@dataclass(frozen=True)
class NvseCosave:
    format_version: int
    nvse_version: int
    runtime_version: int
    plugins: tuple[NvsePlugin, ...]

    @classmethod
    def parse(cls, data: bytes) -> "NvseCosave":
        cur = _Cursor(data)
        magic = cur.take(4)
        if magic != MAGIC:
            raise NvseParseError(f"bad magic: expected {MAGIC!r}, got {magic!r}")
        fmt_ver = cur.u32()
        if fmt_ver != FORMAT_VERSION:
            raise NvseParseError(f"unsupported format version {fmt_ver}")
        nvse_ver = cur.u32()
        runtime_ver = cur.u32()
        num_plugins = cur.u32()
        plugins: list[NvsePlugin] = []
        for _ in range(num_plugins):
            opcode = cur.u32()
            num_chunks = cur.u32()
            plen = cur.u32()
            pbytes = cur.take(plen)
            pcur = _Cursor(pbytes)
            chunks: list[NvseChunk] = []
            for _ in range(num_chunks):
                tag = pcur.take(4)
                cver = pcur.u32()
                clen = pcur.u32()
                cdata = pcur.take(clen)
                chunks.append(NvseChunk(tag=tag, version=cver, data=cdata))
            if pcur.remaining() != 0:
                raise NvseParseError(
                    f"plugin {opcode:#010x} has {pcur.remaining()} trailing byte(s) after {num_chunks} chunks"
                )
            plugins.append(NvsePlugin(opcode=opcode, chunks=tuple(chunks)))
        if cur.remaining() != 0:
            raise NvseParseError(f"file has {cur.remaining()} trailing byte(s) after {num_plugins} plugins")
        return cls(
            format_version=fmt_ver,
            nvse_version=nvse_ver,
            runtime_version=runtime_ver,
            plugins=tuple(plugins),
        )

    def chunks_for(self, opcode: int, tag: bytes, version: int | None = None) -> list[NvseChunk]:
        out: list[NvseChunk] = []
        for p in self.plugins:
            if p.opcode != opcode:
                continue
            for ch in p.chunks:
                if ch.tag == tag and (version is None or ch.version == version):
                    out.append(ch)
        return out

    def mod_list(self) -> list[str]:
        chunks = self.chunks_for(PLUGIN_NVSE, b"SDOM", 0)
        if not chunks:
            return []
        cur = _Cursor(chunks[0].data)
        n = cur.u8()
        mods = [cur.str_u16() for _ in range(n)]
        if cur.remaining() != 0:
            raise NvseParseError(f"SDOM trailing bytes: {cur.remaining()}")
        return mods

    def arrays(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_NVSE, b"RVRA", 2):
            cur = _Cursor(ch.data)
            owning_mod = cur.u8()
            array_id = cur.u32()
            key_type = cur.u8()
            packed = cur.u8()
            num_refs = cur.u32()
            ref_mods = list(cur.take(num_refs))
            num_elems = cur.u32()
            if packed == 1:
                kind = "list"
            elif key_type == 1:
                kind = "map"
            elif key_type == 3:
                kind = "stringmap"
            else:
                raise NvseParseError(f"RVRA unknown key_type={key_type} packed={packed}")
            entries: list[dict[str, Any]] = []
            for idx in range(num_elems):
                if packed == 1:
                    key: Any = float(idx)
                elif key_type == 1:
                    key = cur.f64()
                else:
                    key = cur.str_u16()
                vtype = cur.u8()
                if vtype == 1:
                    val: Any = {"type": "num", "value": cur.f64()}
                elif vtype == 2:
                    val = {"type": "form", "value": cur.u32()}
                elif vtype == 3:
                    val = {"type": "str", "value": cur.str_u16()}
                elif vtype == 4:
                    val = {"type": "array", "value": cur.u32()}
                else:
                    raise NvseParseError(f"RVRA unknown element type {vtype}")
                entries.append({"key": key, "val": val})
            if cur.remaining() != 0:
                raise NvseParseError(f"RVRA trailing bytes: {cur.remaining()}")
            out.append(
                {
                    "owning_mod": owning_mod,
                    "id": array_id,
                    "kind": kind,
                    "ref_mods": ref_mods,
                    "entries": entries,
                }
            )
        return out

    def jip_aux_vars(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_JIP, b"JPAV", 10):
            cur = _Cursor(ch.data)
            num_mods = cur.u16()
            for _ in range(num_mods):
                mod_index = cur.u8()
                num_owners = cur.u16()
                for _ in range(num_owners):
                    owner = cur.u32()
                    num_vars = cur.u16()
                    for _ in range(num_vars):
                        name = cur.str_u8()
                        num_elems = cur.u16()
                        elems = [_read_jip_val(cur) for _ in range(num_elems)]
                        out.append(
                            {
                                "mod_index": mod_index,
                                "owner": owner,
                                "name": name,
                                "elements": elems,
                            }
                        )
            if cur.remaining() != 0:
                raise NvseParseError(f"JPAV trailing bytes: {cur.remaining()}")
        return out

    def jip_refmaps(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_JIP, b"JPRM", 10):
            cur = _Cursor(ch.data)
            num_mods = cur.u16()
            for _ in range(num_mods):
                mod_index = cur.u8()
                num_maps = cur.u16()
                for _ in range(num_maps):
                    name = cur.str_u8()
                    num_entries = cur.u16()
                    entries = []
                    for _ in range(num_entries):
                        key_form = cur.u32()
                        entries.append({"key": key_form, "val": _read_jip_val(cur)})
                    out.append({"mod_index": mod_index, "name": name, "entries": entries})
            if cur.remaining() != 0:
                raise NvseParseError(f"JPRM trailing bytes: {cur.remaining()}")
        return out

    def jip_linked_refs(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_JIP, b"JPLR", 9):
            cur = _Cursor(ch.data)
            n = cur.u16()
            for _ in range(n):
                ref_id = cur.u32()
                linked_id = cur.u32()
                mod_index = cur.u8()
                out.append({"ref": ref_id, "linked": linked_id, "mod_index": mod_index})
            if cur.remaining() != 0:
                raise NvseParseError(f"JPLR trailing bytes: {cur.remaining()}")
        return out

    def jip_added_script_vars(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_JIP, b"JPSV", 9):
            cur = _Cursor(ch.data)
            num_owners = cur.u16()
            for _ in range(num_owners):
                owner = cur.u32()
                num_vars = cur.u16()
                for _ in range(num_vars):
                    mod_index = cur.u8()
                    name = cur.str_u8()
                    raw = cur.take(8)
                    lo, hi = struct.unpack("<II", raw)
                    if hi == 0 and lo != 0:
                        val: dict[str, Any] = {"type": "form", "value": lo}
                    else:
                        val = {"type": "num", "value": struct.unpack("<d", raw)[0]}
                    out.append({"owner": owner, "mod_index": mod_index, "name": name, "val": val})
            if cur.remaining() != 0:
                raise NvseParseError(f"JPSV trailing bytes: {cur.remaining()}")
        return out

    def jg_extra_misc_stats(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_JOHNNYGUITAR, b"SMGJ", 1):
            cur = _Cursor(ch.data)
            n = cur.u16()
            for _ in range(n):
                name = cur.str_u16()
                val = cur.i32()
                out.append({"name": name, "value": val})
            if cur.remaining() != 0:
                raise NvseParseError(f"SMGJ trailing bytes: {cur.remaining()}")
        return out

    def showoff_strmaps(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_SHOWOFF, b"OSMS", 10):
            cur = _Cursor(ch.data)
            num_mods = cur.u16()
            for _ in range(num_mods):
                mod_index = cur.u8()
                num_maps = cur.u16()
                for _ in range(num_maps):
                    name = cur.str_u8()
                    num_entries = cur.u16()
                    entries = []
                    for _ in range(num_entries):
                        k = cur.str_u8()
                        entries.append({"key": k, "val": _read_jip_val(cur)})
                    out.append({"mod_index": mod_index, "name": name, "entries": entries})
            if cur.remaining() != 0:
                raise NvseParseError(f"OSMS trailing bytes: {cur.remaining()}")
        return out

    def showoff_aux_timers(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for ch in self.chunks_for(PLUGIN_SHOWOFF, b"SOAT", 1):
            cur = _Cursor(ch.data)
            num_mods = cur.u16()
            for _ in range(num_mods):
                mod_index = cur.u8()
                num_owners = cur.u16()
                for _ in range(num_owners):
                    owner = cur.u32()
                    num_timers = cur.u16()
                    for _ in range(num_timers):
                        name = cur.str_u8()
                        time_a = cur.f64()
                        time_b = cur.f64()
                        flags = cur.u32()
                        out.append(
                            {
                                "mod_index": mod_index,
                                "owner": owner,
                                "name": name,
                                "time_a": time_a,
                                "time_b": time_b,
                                "flags": flags,
                            }
                        )
            if cur.remaining() != 0:
                raise NvseParseError(f"SOAT trailing bytes: {cur.remaining()}")
        return out

    def unknown_chunks(self) -> list[dict[str, Any]]:
        out: list[dict[str, Any]] = []
        for p in self.plugins:
            for ch in p.chunks:
                if (p.opcode, ch.tag, ch.version) not in KNOWN_CHUNK_KEYS:
                    out.append(
                        {
                            "opcode": f"0x{p.opcode:08X}",
                            "tag": ch.tag_ascii,
                            "version": ch.version,
                            "length": len(ch.data),
                        }
                    )
        return out

    def summary(self) -> dict[str, Any]:
        return {
            "format_version": self.format_version,
            "nvse_version": f"0x{self.nvse_version:08X}",
            "runtime_version": f"0x{self.runtime_version:08X}",
            "plugins": [
                {
                    "opcode": f"0x{p.opcode:08X}",
                    "chunks": [
                        {"tag": c.tag_ascii, "version": c.version, "length": len(c.data)}
                        for c in p.chunks
                    ],
                }
                for p in self.plugins
            ],
            "mods": self.mod_list(),
            "arrays": self.arrays(),
            "jip_aux_vars": self.jip_aux_vars(),
            "jip_refmaps": self.jip_refmaps(),
            "jip_linked_refs": self.jip_linked_refs(),
            "jip_added_script_vars": self.jip_added_script_vars(),
            "jg_extra_misc_stats": self.jg_extra_misc_stats(),
            "showoff_strmaps": self.showoff_strmaps(),
            "showoff_aux_timers": self.showoff_aux_timers(),
            "unknown_chunks": self.unknown_chunks(),
        }


def _read_jip_val(cur: _Cursor) -> dict[str, Any]:
    tag = cur.u8()
    if tag == 1:
        return {"type": "num", "value": cur.f64()}
    if tag == 2:
        return {"type": "form", "value": cur.u32()}
    if tag == 4:
        return {"type": "str", "value": cur.str_u16()}
    raise NvseParseError(f"unknown JIP/ShowOff value tag {tag}")


def diff_cosaves(a: NvseCosave, b: NvseCosave) -> dict[str, Any]:
    """Compare plugin/chunk structure between Save A and Save B."""

    def chunk_map(c: NvseCosave) -> dict[tuple[int, str, int], list[bytes]]:
        m: dict[tuple[int, str, int], list[bytes]] = {}
        for p in c.plugins:
            for ch in p.chunks:
                m.setdefault((p.opcode, ch.tag_ascii, ch.version), []).append(ch.data)
        return m

    ma, mb = chunk_map(a), chunk_map(b)
    all_keys = sorted(set(ma) | set(mb))
    added, removed, changed = [], [], []
    for k in all_keys:
        la = ma.get(k)
        lb = mb.get(k)
        label = f"0x{k[0]:08X}/{k[1]} v{k[2]}"
        if la is None:
            added.append({"chunk": label, "count": len(lb or []), "bytes": sum(len(x) for x in (lb or []))})
        elif lb is None:
            removed.append({"chunk": label, "count": len(la), "bytes": sum(len(x) for x in la)})
        elif la != lb:
            changed.append(
                {
                    "chunk": label,
                    "before_count": len(la),
                    "after_count": len(lb),
                    "before_bytes": sum(len(x) for x in la),
                    "after_bytes": sum(len(x) for x in lb),
                }
            )
    return {"added": added, "removed": removed, "changed": changed}


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Clean-room `.nvse` co-save binary parser and A/B differ.")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p_ins = sub.add_parser("inspect", help="Parse and print JSON summary of an `.nvse` file.")
    p_ins.add_argument("path", type=Path)
    p_dif = sub.add_parser("diff", help="Compare two `.nvse` files chunk-by-chunk.")
    p_dif.add_argument("before", type=Path)
    p_dif.add_argument("after", type=Path)
    args = ap.parse_args(argv)
    if args.cmd == "inspect":
        cosave = NvseCosave.parse(args.path.read_bytes())
        print(json.dumps(cosave.summary(), indent=2))
        return 0
    if args.cmd == "diff":
        a = NvseCosave.parse(args.before.read_bytes())
        b = NvseCosave.parse(args.after.read_bytes())
        print(json.dumps(diff_cosaves(a, b), indent=2))
        return 0
    return 2


if __name__ == "__main__":
    sys.exit(main())
