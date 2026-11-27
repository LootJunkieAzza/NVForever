#!/usr/bin/env python3
"""Clean-room stdlib-only `.fos` binary parser and A/B byte-diff analyzer.

Parses Fallout: New Vegas 1.4.0.525 (`FO3SAVEGAME`) save files produced by
controlled single-variable black-box A/B capture runs, decodes headers,
Global Tables 1 & 2, FormID/worldspace tables, and Changed Forms across all
verified record types/flags/extras, and diffs save pairs cleanly.

Pure standard library Python 3 only — no third-party dependencies, no game
assets, and no hardcoded machine paths.
"""

from __future__ import annotations

import argparse
import glob
import json
import os
import struct
import sys
from pathlib import Path
from typing import Any

BAR = 0x7C
AV_COUNT = 77
AV_ENTRIES = 232

# Anchor immediately following the player bit-22 and non-player bit-23 AV list
# (`FF FF | FF FF | 00 | 01 01 00 00 |`).
PROCESS_AV_LIST_ANCHOR = b"\xff\xff\x7c\xff\xff\x7c\x00\x7c\x01\x01\x00\x00\x7c"

KIND_NAMES: dict[int, str] = {
    0: "REFR",
    1: "ACHR",
    2: "ACRE",
    3: "PMIS",
    4: "PGRE",
    7: "CELL",
    8: "INFO",
    9: "QUST",
    10: "NPC_",
    11: "CREA",
    13: "TACT",
    22: "MISC",
    31: "NOTE",
    32: "ECZN",
    33: "CLAS",
    34: "FACT",
    35: "PACK",
    37: "FLST",
    40: "LVLI",
    43: "REPU",
    50: "CHAL",
}


class ParseError(Exception):
    """Raised when a `.fos` buffer violates expected binary framing."""


def encode_vsval(value: int) -> bytes:
    """Encode an unsigned integer in `.fos` variable-size (`vsval`) format."""
    if value < 0:
        raise ValueError(f"vsval cannot encode negative value {value}")
    if value < 0x40:
        return bytes([value << 2])
    if value < 0x4000:
        return struct.pack("<H", (value << 2) | 1)
    if value < 0x40000000:
        return struct.pack("<I", (value << 2) | 2)
    raise ValueError(f"vsval value too large: {value}")


def encode_ref3(kind: int, value: int) -> bytes:
    """Encode a 3-byte big-endian `.fos` RefID (`kind` in 0..=2, 22-bit value)."""
    if kind not in (0, 1, 2):
        raise ValueError(f"ref3 kind must be 0, 1, or 2; got {kind}")
    if not (0 <= value <= 0x3FFFFF):
        raise ValueError(f"ref3 value out of 22-bit range: {value:#x}")
    packed = (kind << 22) | value
    return bytes([(packed >> 16) & 0xFF, (packed >> 8) & 0xFF, packed & 0xFF])


class Cursor:
    """Bounds-checked little-endian cursor for `0x7C`-delimited `.fos` payloads."""

    def __init__(self, buf: bytes, pos: int = 0) -> None:
        self.b = buf
        self.p = pos

    def clone(self) -> "Cursor":
        return Cursor(self.b, self.p)

    def at_end(self) -> bool:
        return self.p == len(self.b)

    def rem(self) -> int:
        return len(self.b) - self.p

    def take(self, n: int) -> bytes:
        if n < 0 or self.p + n > len(self.b):
            raise ParseError(f"eof at {self.p:#x} needing {n} (len {len(self.b):#x})")
        chunk = self.b[self.p : self.p + n]
        self.p += n
        return chunk

    def u8(self) -> int:
        return self.take(1)[0]

    def i8(self) -> int:
        return struct.unpack("<b", self.take(1))[0]

    def u16(self) -> int:
        return struct.unpack("<H", self.take(2))[0]

    def i16(self) -> int:
        return struct.unpack("<h", self.take(2))[0]

    def u32(self) -> int:
        return struct.unpack("<I", self.take(4))[0]

    def i32(self) -> int:
        return struct.unpack("<i", self.take(4))[0]

    def f32(self) -> float:
        return struct.unpack("<f", self.take(4))[0]

    def f64(self) -> float:
        return struct.unpack("<d", self.take(8))[0]

    def ref3(self) -> bytes:
        return self.take(3)

    def bar(self) -> None:
        byte = self.u8()
        if byte != BAR:
            raise ParseError(f"expected '|' (0x7C) at {self.p - 1:#x}, got {byte:#04x}")

    def vsval(self) -> int:
        b0 = self.u8()
        kind = b0 & 3
        if kind == 0:
            return b0 >> 2
        if kind == 1:
            b1 = self.u8()
            return ((b1 << 8) | b0) >> 2
        if kind == 2:
            b1, b2, b3 = self.take(3)
            return ((b3 << 24) | (b2 << 16) | (b1 << 8) | b0) >> 2
        raise ParseError(f"invalid vsval tag {kind} at {self.p - 1:#x}")

    def string_field(self, trailing_bar_on_nonempty: bool = True) -> str:
        n = self.u16()
        self.bar()
        if n == 0:
            return ""
        text = self.take(n).decode("latin-1")
        if trailing_bar_on_nonempty:
            self.bar()
        return text


# Backwards-friendly alias.
Cur = Cursor


def _decode_pre_anchor_av_list(
    buf: bytes, *, restrict_skills: bool = False
) -> list[tuple[int, float]] | None:
    """Decode a `vsval n | n * (u8 av_code | f32 val |)` list immediately before
    `PROCESS_AV_LIST_ANCHOR` (used by player `ACHR 0x14` bit 22 and non-player
    `ACHR` bit 23).
    """
    idx = buf.find(PROCESS_AV_LIST_ANCHOR)
    if idx < 2:
        return None
    max_entries = 14 if restrict_skills else AV_COUNT
    for k in range(1, max_entries + 1):
        start = idx - k * 7
        if start < 2:
            break
        if k < 64:
            hdr_ok = buf[start - 2] == (k << 2) and buf[start - 1] == BAR
        else:
            v = (k << 2) | 1
            hdr_ok = (
                start >= 3
                and buf[start - 3] == (v & 0xFF)
                and buf[start - 2] == (v >> 8)
                and buf[start - 1] == BAR
            )
        if not hdr_ok:
            continue
        c = Cursor(buf, start)
        entries: list[tuple[int, float]] = []
        valid = True
        try:
            for _ in range(k):
                code = c.u8()
                c.bar()
                val = c.f32()
                c.bar()
                if restrict_skills and not (32 <= code <= 45 and 0.0 <= val <= 100.0):
                    valid = False
                    break
                if code >= AV_COUNT or val != val:
                    valid = False
                    break
                entries.append((code, val))
        except ParseError:
            valid = False
        if valid and c.p == idx:
            return entries
    return None


def decode_player_tok76(tail: bytes) -> int | None:
    """Extract active equipped magazine round count from `ACHR 0x14` post-inventory
    process block `tok[76]` (`u32 |`).
    """
    toks: list[bytes] = []
    pos = 0
    for i, byte in enumerate(tail):
        if byte == BAR:
            toks.append(tail[pos:i])
            pos = i + 1
            if len(toks) > 76:
                break
    if len(toks) <= 76 or len(toks[76]) != 4:
        return None
    return struct.unpack("<I", toks[76])[0]


class FosFile:
    """Parsed `.fos` save image (from disk path or in-memory bytes)."""

    def __init__(self, path: str | os.PathLike[str] | None = None, *, raw: bytes | None = None, name: str = "<memory>") -> None:
        if path is not None:
            self.path = str(path)
            self.raw = Path(path).read_bytes()
        elif raw is not None:
            self.path = name
            self.raw = raw
        else:
            raise ValueError("Provide either path or raw bytes")
        self._parse()

    @classmethod
    def from_bytes(cls, raw: bytes, name: str = "<memory>") -> "FosFile":
        return cls(path=None, raw=raw, name=name)

    def _parse(self) -> None:
        c = Cursor(self.raw, 0)
        magic = c.take(11)
        if magic != b"FO3SAVEGAME":
            raise ParseError(f"bad magic {magic!r}")
        hdr_size = c.u32()
        hdr_start = c.p
        self.hdr_version = c.u32()
        c.bar()
        self.language = c.take(64).split(b"\x00", 1)[0].decode("latin-1")
        c.bar()
        self.shot_w = c.u32()
        c.bar()
        self.shot_h = c.u32()
        c.bar()
        self.save_num = c.u32()
        c.bar()
        self.player_name = c.string_field(True)
        self.karma_title = c.string_field(True)
        self.level = c.u32()
        c.bar()
        self.location = c.string_field(True)
        self.play_time = c.string_field(True)
        c.p = hdr_start + hdr_size

        self.screenshot = c.take(self.shot_w * self.shot_h * 3)
        self.form_version = c.u8()
        plugin_block_sz = c.u32()
        pb_end = c.p + plugin_block_sz
        pcount = c.u8()
        c.bar()
        self.plugins: list[str] = []
        for _ in range(pcount):
            self.plugins.append(c.string_field(True))
        c.p = pb_end

        # [0]=formid_off, [1]=tail_off, [2]=g1_off, [3]=changed_off,
        # [4]=g2_off, [5]=g1_cnt, [6]=g2_cnt, [7]=changed_cnt
        self.table = [c.u32() for _ in range(8)]

        # Form ID array & worldspace array
        fc = Cursor(self.raw, self.table[0])
        fid_count = fc.u32()
        if fc.p + fid_count * 4 > len(self.raw):
            raise ParseError("form_ids table truncated")
        self.form_ids = list(struct.unpack_from(f"<{fid_count}I", self.raw, fc.p))
        fc.p += fid_count * 4
        ws_count = fc.u32()
        if fc.p + ws_count * 4 > len(self.raw):
            raise ParseError("worldspaces table truncated")
        self.worldspaces = list(struct.unpack_from(f"<{ws_count}I", self.raw, fc.p))
        fc.p += ws_count * 4
        if fc.p != self.table[1]:
            raise ParseError(f"worldspace end {fc.p} != tail {self.table[1]}")
        self.tail = self.raw[self.table[1] :]

        # Global data table 1
        self.globals1: dict[int, bytes] = {}
        gc = Cursor(self.raw, self.table[2])
        for _ in range(self.table[5]):
            gtype = gc.u32()
            glen = gc.u32()
            self.globals1[gtype] = gc.take(glen)
        if gc.p != self.table[3]:
            raise ParseError(f"g1 end {gc.p} != changed_off {self.table[3]}")

        # Changed forms
        self.changed: list[dict[str, Any]] = []
        self.by_key: dict[tuple[int, int | None], dict[str, Any]] = {}
        cc = Cursor(self.raw, self.table[3])
        for _ in range(self.table[7]):
            r3 = cc.ref3()
            flags = cc.u32()
            tb = cc.u8()
            kind = tb & 0x3F
            lwidth = tb >> 6
            ver = cc.u8()
            if lwidth == 0:
                dlen = cc.u8()
            elif lwidth == 1:
                dlen = cc.u16()
            elif lwidth == 2:
                dlen = cc.u32()
            else:
                raise ParseError(f"bad lwidth {lwidth}")
            data = cc.take(dlen)
            fid = self.resolve(r3)
            rec = {
                "ref3": r3,
                "form": fid,
                "flags": flags,
                "kind": kind,
                "ver": ver,
                "data": data,
            }
            self.changed.append(rec)
            self.by_key[(kind, fid)] = rec
        if cc.p != self.table[4]:
            raise ParseError(f"changed end {cc.p} != g2_off {self.table[4]}")

        # Global data table 2
        self.globals2: dict[int, bytes] = {}
        g2c = Cursor(self.raw, self.table[4])
        for _ in range(self.table[6]):
            gtype = g2c.u32()
            glen = g2c.u32()
            self.globals2[gtype] = g2c.take(glen)
        if g2c.p != self.table[0]:
            raise ParseError(f"g2 end {g2c.p} != formid_off {self.table[0]}")

    def resolve(self, r3: bytes) -> int | None:
        v = (r3[0] << 16) | (r3[1] << 8) | r3[2]
        kind = v >> 22
        val = v & 0x3FFFFF
        if kind == 0:
            if val == 0:
                return None
            if 1 <= val <= len(self.form_ids):
                return self.form_ids[val - 1]
            return None
        if kind == 1:
            return val
        if kind == 2:
            return 0xFF000000 | val
        return None

    def form_id_to_ref3(self, form_id: int) -> bytes | None:
        """Look up the kind-0 `ref3` encoding of `form_id` in this save's FormID table."""
        try:
            idx = self.form_ids.index(form_id) + 1
        except ValueError:
            return None
        return encode_ref3(0, idx)

    def get_rec(self, kind: int, fid: int) -> dict[str, Any] | None:
        return self.by_key.get((kind, fid))

    def screenshot_rgb_means(self) -> tuple[float, float, float] | None:
        """Return per-channel `(ch0, ch1, ch2)` mean across the RGB thumbnail."""
        pixels = self.shot_w * self.shot_h
        if pixels <= 0 or len(self.screenshot) < pixels * 3:
            return None
        c0 = sum(self.screenshot[0::3]) / pixels
        c1 = sum(self.screenshot[1::3]) / pixels
        c2 = sum(self.screenshot[2::3]) / pixels
        return (c0, c1, c2)

    # --- Global Table 1 Decoders ---

    def decode_misc_stats(self) -> list[int] | None:
        """Decode Global[0]: `u32 count | count * (u32 |)`."""
        d = self.globals1.get(0)
        if not d:
            return None
        c = Cursor(d, 0)
        count = c.u32()
        c.bar()
        stats: list[int] = []
        for _ in range(count):
            stats.append(c.u32())
            c.bar()
        return stats

    def decode_global1(self) -> dict[str, Any] | None:
        """Decode Global[1]: player world/cell location and active quest ref."""
        d = self.globals1.get(1)
        if not d:
            return None
        c = Cursor(d, 0)
        next_id = c.u32()
        c.bar()
        world = self.resolve(c.ref3())
        c.bar()
        gx = c.i32()
        c.bar()
        gy = c.i32()
        c.bar()
        cell = self.resolve(c.ref3())
        c.bar()
        pos = [c.f32() for _ in range(3)]
        c.bar()
        active_quest = self.resolve(c.ref3())
        if not c.at_end():
            c.bar()
        return {
            "next_form_id": next_id,
            "world": world,
            "grid": (gx, gy),
            "cell": cell,
            "pos": pos,
            "active_quest": active_quest,
        }

    def decode_active_quest(self) -> int | None:
        g1 = self.decode_global1()
        return None if g1 is None else g1["active_quest"]

    def decode_dead_counts(self) -> dict[str, Any] | None:
        """Decode Global[2]: `vsval n | n * (ref3 base_form | u16 count |)` + tail."""
        d = self.globals1.get(2)
        if not d:
            return None
        c = Cursor(d, 0)
        n = c.vsval()
        c.bar()
        out: list[tuple[int | None, int]] = []
        for _ in range(n):
            ref = self.resolve(c.ref3())
            c.bar()
            cnt = c.u16()
            c.bar()
            out.append((ref, cnt))
        return {
            "count": n,
            "entries": out,
            "tail_len": c.rem(),
            "tail_head": d[c.p : c.p + 24].hex(),
        }

    def decode_weather(self) -> dict[str, Any] | None:
        """Decode Global[8]: current and secondary weather refs."""
        d = self.globals1.get(8)
        if not d:
            return None
        c = Cursor(d, 0)
        w0 = self.resolve(c.ref3())
        c.bar()
        w1 = self.resolve(c.ref3())
        c.bar()
        return {"current": w0, "second": w1, "len": len(d)}

    def decode_radio(self) -> dict[str, Any] | None:
        """Decode Global[10]: opening ambient/header block + active Pip-Boy radio list2."""
        d = self.globals1.get(10)
        if not d:
            return None
        c = Cursor(d, 0)
        try:
            u0 = c.u32()
            c.bar()
            on = c.u8()
            c.bar()
            song = c.string_field(True)
            cnt1 = c.vsval()
            c.bar()
            refs1: list[int | None] = []
            for _ in range(cnt1):
                refs1.append(self.resolve(c.ref3()))
                c.bar()

            # Observed active Pip-Boy radio station list (BUG #3 fix).
            active_stations: list[dict[str, Any]] = []
            if not c.at_end():
                cnt2 = c.vsval()
                c.bar()
                for _ in range(cnt2):
                    station = self.resolve(c.ref3())
                    c.bar()
                    playback_ms = c.u32()
                    c.bar()
                    f1 = c.u32()
                    c.bar()
                    b1 = c.u8()
                    c.bar()
                    b2 = c.u8()
                    c.bar()
                    f2 = c.u32()
                    c.bar()
                    b3 = c.u8()
                    c.bar()
                    b4 = c.u8()
                    c.bar()
                    active_stations.append(
                        {
                            "station": station,
                            "playback_ms": playback_ms,
                            "u32_a": f1,
                            "u8_a": b1,
                            "u8_b": b2,
                            "u32_b": f2,
                            "u8_c": b3,
                            "u8_d": b4,
                        }
                    )

            return {
                "ok": True,
                "u0": u0,
                "on": on,
                "song": song,
                "count": cnt1,
                "refs": refs1,
                "active_stations": active_stations,
                "pipboy_on": len(active_stations) > 0,
                "pipboy_station": active_stations[0]["station"] if active_stations else None,
                "rem_bytes": d[c.p :].hex(),
            }
        except Exception as exc:
            return {"ok": False, "err": str(exc), "raw_head": d[:64].hex(), "len": len(d)}

    # --- Shared Sub-Decoders ---

    def parse_script_extra(self, c: Cursor) -> dict[str, Any]:
        scr = self.resolve(c.ref3())
        c.bar()
        nv = c.vsval()
        c.bar()
        vs: list[tuple[int, tuple[str, Any]]] = []
        for _ in range(nv):
            idx = c.u32()
            c.bar()
            if idx & 0x80000000:
                val: tuple[str, Any] = ("ref", self.resolve(c.ref3()))
            else:
                val = ("num", c.f64())
            c.bar()
            vs.append((idx & 0x7FFFFFFF, val))
        trail = c.vsval()
        c.bar()
        init_f = c.u8()
        c.bar()
        return {"script": scr, "vars": vs, "trail": trail, "init": init_f}

    def parse_item_list(
        self, c: Cursor, *, strict_legacy_0x3e: bool = False
    ) -> list[dict[str, Any]]:
        """Parse an inventory item list (`vsval n | n * (ref3 | i32 | entries)`)."""
        n = c.vsval()
        c.bar()
        items: list[dict[str, Any]] = []
        for _ in range(n):
            item = self.resolve(c.ref3())
            c.bar()
            cnt = c.i32()
            c.bar()
            lists = c.vsval()
            c.bar()
            entries: list[dict[str, Any]] = []
            for _ in range(lists):
                extras = c.vsval()
                c.bar()
                entry: dict[str, Any] = {}
                for _ in range(extras):
                    ty = c.u8()
                    c.bar()
                    if ty == 0x0D:
                        entry["0x0D_script"] = self.parse_script_extra(c)
                    elif ty in (0x16, 0x17):
                        entry[f"0x{ty:02X}_worn"] = True
                    elif ty == 0x3E:
                        if strict_legacy_0x3e:
                            raise ParseError(
                                f"unknown item extra 0x3E at {c.p:#x} next={c.b[c.p:c.p+16].hex()}"
                            )
                        entry["0x3E_cannot_wear_or_unequip"] = True
                    elif ty in (0x1C, 0x22):
                        entry[f"0x{ty:02X}"] = self.resolve(c.ref3())
                        c.bar()
                    elif ty == 0x21:
                        entry["0x21_owner"] = self.resolve(c.ref3())
                        c.bar()
                    elif ty == 0x23:
                        entry["0x23_rank"] = c.i32()
                        c.bar()
                    elif ty == 0x24:
                        entry["0x24_count"] = c.i16()
                        c.bar()
                    elif ty == 0x25:
                        entry["0x25_cond"] = c.f32()
                        c.bar()
                    elif ty in (0x26, 0x28, 0x30):
                        entry[f"0x{ty:02X}"] = c.f32()
                        c.bar()
                    elif ty == 0x2F:
                        u32v = c.u32()
                        c.bar()
                        u8v = c.u8()
                        c.bar()
                        entry["0x2F"] = (u32v, u8v)
                    elif ty == 0x4A:
                        entry["0x4A_hotkey"] = c.u8()
                        c.bar()
                    elif ty == 0x6E:
                        am = self.resolve(c.ref3())
                        c.bar()
                        rd = c.u32()
                        c.bar()
                        entry["0x6E_ammo"] = (am, rd)
                    elif ty == 0x8D:
                        entry["0x8D_mods"] = c.u8()
                        c.bar()
                    else:
                        raise ParseError(
                            f"unknown item extra 0x{ty:02X} at {c.p:#x} next={c.b[c.p:c.p+16].hex()}"
                        )
                entries.append(entry)
            items.append({"item": item, "count": cnt, "entries": entries})
        return items

    def parse_ref_prefix(self, rec: dict[str, Any], c: Cursor) -> dict[str, Any] | None:
        fl = rec["flags"]
        if (fl & 0b1110) == 0:
            return None
        at = self.resolve(c.ref3())
        v = [c.f32() for _ in range(6)]
        cc_suf = None
        if fl & (1 << 3):
            cc_suf = c.take(7)
        c.bar()
        havok_len = None
        if fl & (1 << 2):
            havok_len = c.vsval()
            c.bar()
            if havok_len > 0:
                c.take(havok_len)
        return {
            "at": at,
            "pos": v[:3],
            "rot": v[3:],
            "cell_change": cc_suf.hex() if cc_suf else None,
            "havok_len": havok_len,
        }

    # --- Changed Form Decoders ---

    def decode_npc_or_crea(self, kind: int, fid: int) -> dict[str, Any] | None:
        """Decode `NPC_` (kind 10) or `CREA` (kind 11) in observed multi-bit order:
        `bit 1 (ACBS) -> bit 4 (spells) -> bit 2 (SPECIAL) -> bit 3 -> bit 9 ->
         bit 5 (name) -> bit 11 (FaceGen appearance) -> bit 24 (SexChange u8 |)`.
        """
        rec = self.get_rec(kind, fid)
        if not rec:
            return None
        c = Cursor(rec["data"], 0)
        fl = rec["flags"]
        pb: dict[str, Any] = {
            "form": fid,
            "kind": KIND_NAMES.get(kind, str(kind)),
            "flags": fl,
            "len": len(rec["data"]),
        }
        try:
            if fl & (1 << 1):
                acbs = c.take(24)
                c.bar()
                af = struct.unpack_from("<I", acbs, 0)[0]
                pb["acbs_flags"] = af
                pb["female"] = bool(af & 1)
                pb["essential"] = bool(af & 2)
            if fl & (1 << 4):
                n1 = c.vsval()
                c.bar()
                s1 = []
                for _ in range(n1):
                    s1.append(self.resolve(c.ref3()))
                    c.bar()
                n2 = c.vsval()
                c.bar()
                s2 = []
                for _ in range(n2):
                    s2.append(self.resolve(c.ref3()))
                    c.bar()
                pb["spells"] = s1
                pb["spells2"] = s2
            if fl & (1 << 2):
                pb["special"] = list(c.take(7))
                c.bar()
            if fl & (1 << 3):
                c.take(20)
                c.bar()
            if fl & (1 << 9):
                c.take(28)
                c.bar()
            if fl & (1 << 5):
                pb["name"] = c.string_field(True)
            if fl & (1 << 11):
                hdr = c.u8()
                c.bar()
                fggs = []
                for _ in range(50):
                    fggs.append(c.f32())
                    c.bar()
                fgga = []
                for _ in range(30):
                    fgga.append(c.f32())
                    c.bar()
                fgts = []
                for _ in range(50):
                    fgts.append(c.f32())
                    c.bar()
                hair = self.resolve(c.ref3())
                c.bar()
                eyes = self.resolve(c.ref3())
                c.bar()
                hlen = c.f32()
                c.bar()
                hcol = c.u32()
                c.bar()
                np = c.vsval()
                c.bar()
                parts = []
                for _ in range(np):
                    parts.append(self.resolve(c.ref3()))
                    c.bar()
                pb["appearance"] = {
                    "hdr": hdr,
                    "fggs": fggs,
                    "fgga": fgga,
                    "fgts": fgts,
                    "hair": hair,
                    "eyes": eyes,
                    "hlen": hlen,
                    "hcol": hcol,
                    "parts": parts,
                }
            if fl & (1 << 24):
                sex_byte = c.u8()
                c.bar()
                pb["sex_change_bit24"] = bool(sex_byte)
                pb["female"] = bool(sex_byte)
            pb["rem"] = c.rem()
        except Exception as exc:
            pb["err"] = str(exc)
        return pb

    def decode_player(
        self,
        *,
        gate_scale_bit4: bool = True,
        strict_legacy_0x3e: bool = False,
        restrict_bit22_skills: bool = False,
    ) -> dict[str, Any]:
        """Decode player `NPC_ 0x7` + `ACHR 0x14` (AV arrays, bit-4 gated scale,
        extras, inventory, `tok[76]` mag rounds, bit-22 AV overrides, tail notes,
        perks, teammate perks, and active magic effects).
        """
        out: dict[str, Any] = {}
        npc_base = self.decode_npc_or_crea(10, 7)
        if npc_base is not None:
            out["npc_base"] = npc_base

        r = self.get_rec(1, 0x14)
        if not r:
            return out
        d = r["data"]
        fl = r["flags"]
        out["flags"] = fl
        out["achr_flags"] = f"{fl:#010x}"
        out["achr_len"] = len(d)
        c = Cursor(d, 0)
        at = self.resolve(c.ref3())
        pos_rot = [c.f32() for _ in range(6)]
        c.bar()
        out["place"] = (at, pos_rot[:3], pos_rot[3:])
        avs: list[float] = []
        for _ in range(AV_ENTRIES):
            avs.append(c.f32())
            c.bar()
        out["av1_mod"] = {i: avs[i] for i in range(77) if avs[i] != 0.0}
        out["av2_base"] = {i - 77: avs[i] for i in range(77, 154) if avs[i] != 0.0}
        out["av3_dmg"] = {i - 154: avs[i] for i in range(154, 231) if avs[i] != 0.0}
        out["av231_hp_dmg"] = avs[231]

        proc_level = c.u8()
        c.bar()
        out["post_av_u8"] = proc_level

        # BUG #1: `f32 scale |` is only serialized when change flag bit 4 (0x10) is set.
        if (not gate_scale_bit4) or (fl & (1 << 4)):
            out["scale"] = c.f32()
            c.bar()
        else:
            out["scale"] = None

        ae_cnt = c.vsval()
        c.bar()
        aex: list[tuple[str, Any]] = []
        ae_ok = True
        try:
            for _ in range(ae_cnt):
                ty = c.u8()
                c.bar()
                if ty == 0x5E:
                    n = c.vsval()
                    c.bar()
                    facs = []
                    for _ in range(n):
                        fac = self.resolve(c.ref3())
                        c.bar()
                        rk = c.i8()
                        c.bar()
                        facs.append((fac, rk))
                    aex.append(("0x5E_factions", facs))
                elif ty == 0x0D:
                    aex.append(("0x0D_script", self.parse_script_extra(c)))
                elif ty == 0x18:
                    rf = self.resolve(c.ref3())
                    c.bar()
                    c.take(12)
                    c.bar()
                    flt = c.f32()
                    c.bar()
                    aex.append(("0x18", (rf, flt)))
                elif ty == 0x74:
                    rf = self.resolve(c.ref3())
                    c.bar()
                    aex.append(("0x74", rf))
                elif ty == 0x1D:
                    n = c.vsval()
                    c.bar()
                    tms = []
                    for _ in range(n):
                        tms.append(self.resolve(c.ref3()))
                        c.bar()
                    aex.append(("0x1D_teammates", tms))
                elif ty == 0x60:
                    v = c.u32()
                    c.bar()
                    aex.append(("0x60", v))
                elif ty == 0x75:
                    r1 = self.resolve(c.ref3())
                    c.bar()
                    r2 = self.resolve(c.ref3())
                    c.bar()
                    u = c.u8()
                    c.bar()
                    aex.append(("0x75", (r1, r2, u)))
                else:
                    raise ParseError(
                        f"unknown player actor extra 0x{ty:02X} at {c.p:#x} next={d[c.p:c.p+24].hex()}"
                    )
        except Exception as exc:
            ae_ok = False
            out["actor_extras_err"] = str(exc)
        out["actor_extras"] = aex

        if ae_ok:
            try:
                out["inventory"] = self.parse_item_list(
                    c, strict_legacy_0x3e=strict_legacy_0x3e
                )
                tail = d[c.p :]
                out["tail"] = tail
                out["tail_len"] = len(tail)
                out["tok76_mag_rounds"] = decode_player_tok76(tail)
                if fl & (1 << 22):
                    out["bit22_av_overrides"] = _decode_pre_anchor_av_list(
                        tail, restrict_skills=restrict_bit22_skills
                    )
                else:
                    out["bit22_av_overrides"] = []
                out["tail_notes"] = self._parse_player_tail_notes(tail)
                out["perks"] = self._parse_player_perks(tail)
                tm_res = self._parse_teammate_perks_and_anchor(tail)
                if tm_res is not None:
                    ae_end, tm_perks = tm_res
                    out["teammate_perks"] = tm_perks
                    out["ae_end"] = ae_end
                    out["active_effects"] = self._parse_active_effects(tail, ae_end)
                else:
                    out["teammate_perks"] = None
                    out["active_effects"] = []
                    out["tail_end64"] = tail[-64:].hex()
            except Exception as exc:
                out["inv_err"] = str(exc)
        return out

    def _parse_player_tail_notes(self, tail: bytes) -> list[int]:
        """Parse the observed `ACHR 0x14` tail notes list (BUG #9):
        `10 * (00 00 00 |) -> vsval(0) | -> vsval n_notes | n_notes * (ref3 |)`.
        """
        ten_zeros = b"\x00\x00\x00\x7c" * 10
        idx = tail.find(ten_zeros)
        if idx < 0:
            return []
        c = Cursor(tail, idx + len(ten_zeros))
        try:
            lead = c.vsval()
            c.bar()
            if lead != 0:
                return []
            n_notes = c.vsval()
            c.bar()
            notes: list[int] = []
            for _ in range(n_notes):
                fid = self.resolve(c.ref3())
                c.bar()
                if fid is not None:
                    notes.append(fid)
            return notes
        except ParseError:
            return []

    @staticmethod
    def _match_vsval_hdr(buf: bytes, st: int, k: int) -> int | None:
        if k < 64:
            if st >= 2 and buf[st - 2] == (k << 2) and buf[st - 1] == BAR:
                return st - 2
        elif k < 16384:
            v = (k << 2) | 1
            if (
                st >= 3
                and buf[st - 3] == (v & 0xFF)
                and buf[st - 2] == (v >> 8)
                and buf[st - 1] == BAR
            ):
                return st - 3
        return None

    def _parse_player_perks(self, tail: bytes) -> list[tuple[int, int]]:
        dead_sig = b"\xad\xde\xad\xde\x7c"
        five_zeros = b"\x00\x00\x00\x00\x7c" * 5
        if len(tail) < 85:
            return []
        ss = tail.find(dead_sig)
        if ss < 0:
            ss = 0
        se = len(tail) - 60
        ce = se
        while ce >= ss + len(five_zeros):
            rel = tail.rfind(five_zeros, ss, ce)
            if rel < 0:
                break
            z5 = rel
            ce = z5 + len(five_zeros) - 1
            cands: list[int] = []
            if z5 >= 8 and tail[z5 - 8 : z5] == b"\x00\x7c\x00\x7c\x00\x7c\x00\x7c":
                cands.append(z5 - 8)
            if z5 >= 4 and tail[z5 - 4 : z5] == b"\x00\x7c\x00\x7c":
                for m in range(1, 17):
                    mst = (z5 - 4) - m * 13
                    if mst >= 4 and tail[mst - 4 : mst] == bytes([0, BAR, m << 2, BAR]):
                        if all(
                            tail[mst + j * 13 + 4] == BAR
                            and tail[mst + j * 13 + 9] == BAR
                            and tail[mst + j * 13 + 12] == BAR
                            for j in range(m)
                        ):
                            cands.append(mst - 4)
            for pe in cands:
                for k in range(1, 513):
                    st = pe - k * 6
                    if st < 0:
                        break
                    hs = self._match_vsval_hdr(tail, st, k)
                    if hs is None:
                        continue
                    if hs >= 5 and (
                        tail[hs - 1] != BAR or tail[hs - 3] != BAR or tail[hs - 5] != BAR
                    ):
                        continue
                    res: list[tuple[int, int]] = []
                    ok = True
                    for j in range(k):
                        off = st + j * 6
                        if tail[off + 3] != BAR or tail[off + 5] != BAR:
                            ok = False
                            break
                        fid = self.resolve(tail[off : off + 3])
                        rk = tail[off + 4]
                        if fid is None or rk == 0 or rk > 20:
                            ok = False
                            break
                        res.append((fid, rk))
                    if ok:
                        return res
        return []

    def _parse_teammate_perks_and_anchor(
        self, tail: bytes
    ) -> tuple[int, list[tuple[int, int]]] | None:
        if len(tail) < 56:
            return None
        e54 = len(tail) - 54
        if tail[e54 : e54 + 6] != b"\x00\x7c\x01\x7c\x01\x7c":
            return None
        for j in range(8):
            if tail[e54 + 6 + j * 5 + 4] != BAR:
                return None
        for j in range(4):
            if tail[e54 + 46 + j * 2 + 1] != BAR:
                return None
        if (
            e54 >= 17
            and tail[e54 - 2] == 0
            and tail[e54 - 1] == BAR
            and tail[e54 - 3] == BAR
            and tail[e54 - 8] == BAR
            and tail[e54 - 13] == BAR
        ):
            return (e54 - 17, [])
        for k in range(1, 65):
            st = e54 - k * 6
            if st < 0:
                break
            hs = self._match_vsval_hdr(tail, st, k)
            if hs is None or hs < 15:
                continue
            if tail[hs - 1] != BAR or tail[hs - 6] != BAR or tail[hs - 11] != BAR:
                continue
            res: list[tuple[int, int]] = []
            ok = True
            for j in range(k):
                off = st + j * 6
                if tail[off + 3] != BAR or tail[off + 5] != BAR:
                    ok = False
                    break
                fid = self.resolve(tail[off : off + 3])
                rk = tail[off + 4]
                if fid is None or rk == 0 or rk > 20:
                    ok = False
                    break
                res.append((fid, rk))
            if ok:
                return (hs - 15, res)
        return None

    def _parse_active_effects(self, tail: bytes, ae_end: int) -> list[dict[str, Any]]:
        if ae_end < 2 or ae_end > len(tail):
            return []
        ss = max(0, ae_end - 32768)
        for start in range(ss, ae_end):
            if (start + 1 >= ae_end or tail[start + 1] != BAR) and (
                start + 2 >= ae_end or tail[start + 2] != BAR
            ):
                continue
            c = Cursor(tail[:ae_end], start)
            try:
                cnt = c.vsval()
                c.bar()
                if cnt == 0 or cnt > 512:
                    continue
                out: list[dict[str, Any]] = []
                for _ in range(cnt):
                    src = self.resolve(c.ref3())
                    c.bar()
                    eidx = c.u8()
                    c.bar()
                    sub = c.vsval()
                    c.bar()
                    plen = c.vsval()
                    c.bar()
                    if not (46 <= plen <= 4096):
                        raise ParseError("plen")
                    pl = c.take(plen)
                    sc = Cursor(pl, 0)
                    elapsed = sc.f32()
                    sc.bar()
                    u8s = []
                    for _ in range(5):
                        u8s.append(sc.u8())
                        sc.bar()
                    mag = sc.f32()
                    sc.bar()
                    dur = sc.f32()
                    sc.bar()
                    fl = sc.u32()
                    sc.bar()
                    caster = self.resolve(sc.ref3())
                    sc.bar()
                    target = self.resolve(sc.ref3())
                    sc.bar()
                    extra_r = self.resolve(sc.ref3())
                    sc.bar()
                    worn = self.resolve(sc.ref3())
                    sc.bar()
                    out.append(
                        {
                            "source": src,
                            "effect_index": eidx,
                            "subtype": sub,
                            "plen": plen,
                            "elapsed": elapsed,
                            "u8s": u8s,
                            "magnitude": mag,
                            "duration": dur,
                            "flags": fl,
                            "caster": caster,
                            "target": target,
                            "extra_ref": extra_r,
                            "worn": worn,
                            "rem_bytes": pl[sc.p :].hex(),
                        }
                    )
                if c.at_end():
                    return out
            except Exception:
                continue
        return []

    def decode_actor(
        self, kind: int, fid: int, *, strict_legacy_0x3e: bool = False
    ) -> dict[str, Any] | None:
        """Decode non-player `ACHR` (1) or `ACRE` (2), including created `0xFF` actors
        and bit-23 `ModAV` permanent modifiers (BUG #6).
        """
        rec = self.get_rec(kind, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {
            "form": fid,
            "kind": KIND_NAMES.get(kind, str(kind)),
            "flags": fl,
            "len": len(rec["data"]),
        }
        try:
            if (fid >> 24) == 0xFF:
                at = self.resolve(c.ref3())
                v = [c.f32() for _ in range(6)]
                hf = c.u8()
                base = self.resolve(c.ref3())
                c.bar()
                res["created_hdr"] = {
                    "at": at,
                    "pos": v[:3],
                    "rot": v[3:],
                    "hflag": hf,
                    "base": base,
                }
                if fl & (1 << 2):
                    hl = c.vsval()
                    c.bar()
                    if hl > 0:
                        c.take(hl)
                    res["havok_len"] = hl
            else:
                res["place"] = self.parse_ref_prefix(rec, c)
            res["process_level"] = c.u8()
            c.bar()
            if fl & 1:
                res["form_flags"] = c.u32()
                c.bar()
            if fl & (1 << 4):
                res["scale"] = c.f32()
                c.bar()
            extra_bits = (1 << 11) | (1 << 17) | (1 << 18) | (1 << 29) | (1 << 31)
            if fl & extra_bits:
                cnt = c.vsval()
                c.bar()
                ex: list[tuple[str, Any]] = []
                for _ in range(cnt):
                    ty = c.u8()
                    c.bar()
                    if ty == 0x04:
                        r = self.resolve(c.ref3())
                        c.bar()
                        u = c.u8()
                        c.bar()
                        ex.append(("0x04", (r, u)))
                    elif ty == 0x0D:
                        ex.append(("0x0D_script", self.parse_script_extra(c)))
                    elif ty == 0x18:
                        r = self.resolve(c.ref3())
                        c.bar()
                        c.take(12)
                        c.bar()
                        f = c.f32()
                        c.bar()
                        ex.append(("0x18", (r, f)))
                    elif ty == 0x19:
                        r1 = self.resolve(c.ref3())
                        c.bar()
                        r2 = self.resolve(c.ref3())
                        c.bar()
                        c.u32()
                        c.bar()
                        c.u8()
                        c.bar()
                        c.u8()
                        c.bar()
                        c.u8()
                        c.bar()
                        ex.append(("0x19", (r1, r2)))
                    elif ty in (0x1B, 0x1D, 0x7C):
                        n = c.vsval()
                        c.bar()
                        rs = []
                        for _ in range(n):
                            rs.append(self.resolve(c.ref3()))
                            c.bar()
                        ex.append((f"0x{ty:02X}", rs))
                    elif ty == 0x1F:
                        ex.append(("0x1F", True))
                    elif ty in (0x21, 0x46, 0x74):
                        r = self.resolve(c.ref3())
                        c.bar()
                        ex.append((f"0x{ty:02X}", r))
                    elif ty == 0x2E:
                        r1 = self.resolve(c.ref3())
                        c.bar()
                        r2 = self.resolve(c.ref3())
                        c.bar()
                        tf = c.u32()
                        c.bar()
                        if tf & 0x10:
                            for _ in range(2):
                                n = c.vsval()
                                c.bar()
                                for _ in range(n):
                                    c.ref3()
                                    c.bar()
                        ex.append(("0x2E", (r1, r2, tf)))
                    elif ty in (0x35, 0x73):
                        u = c.u8()
                        c.bar()
                        ex.append((f"0x{ty:02X}", u))
                    elif ty in (0x56, 0x60):
                        u = c.u32()
                        c.bar()
                        ex.append((f"0x{ty:02X}", u))
                    elif ty == 0x5E:
                        n = c.vsval()
                        c.bar()
                        facs = []
                        for _ in range(n):
                            fa = self.resolve(c.ref3())
                            c.bar()
                            rk = c.i8()
                            c.bar()
                            facs.append((fa, rk))
                        ex.append(("0x5E_factions", facs))
                    elif ty == 0x5F:
                        msk = c.u16()
                        c.bar()
                        c.u32()
                        c.bar()
                        c.u32()
                        c.bar()
                        c.u8()
                        c.bar()
                        c.ref3()
                        c.bar()
                        nl = c.vsval()
                        c.bar()
                        limbs = []
                        for _ in range(nl):
                            bp = c.u8()
                            c.bar()
                            exp = c.u8()
                            c.bar()
                            c.u8()
                            c.bar()
                            c.u8()
                            c.bar()
                            no = c.vsval()
                            c.bar()
                            for _ in range(no):
                                c.ref3()
                                c.bar()
                            limbs.append((bp, bool(exp)))
                        ex.append(("0x5F_limbs", (msk, limbs)))
                    elif ty == 0x75:
                        r1 = self.resolve(c.ref3())
                        c.bar()
                        r2 = self.resolve(c.ref3())
                        c.bar()
                        u = c.u8()
                        c.bar()
                        ex.append(("0x75", (r1, r2, u)))
                    elif ty == 0x89:
                        r = self.resolve(c.ref3())
                        c.bar()
                        ex.append(("0x89_ashpile", r))
                    elif ty == 0x8B:
                        c.take(12)
                        c.bar()
                        c.ref3()
                        c.bar()
                        c.u32()
                        c.bar()
                        c.u8()
                        c.bar()
                        ex.append(("0x8B", True))
                    else:
                        raise ParseError(
                            f"unknown actor extra 0x{ty:02X} at {c.p:#x} next={rec['data'][c.p:c.p+24].hex()}"
                        )
                res["extras"] = ex
            if fl & ((1 << 5) | (1 << 27)):
                res["inventory"] = self.parse_item_list(
                    c, strict_legacy_0x3e=strict_legacy_0x3e
                )
            # Process block
            if not c.at_end():
                if fl & (1 << 23):
                    res["av_bit23_mod"] = _decode_pre_anchor_av_list(rec["data"][c.p :])
                pc = c.clone()
                try:
                    for _ in range(8):
                        pc.u8()
                        pc.bar()
                    pc.f32()
                    pc.bar()
                    pc.f32()
                    pc.bar()
                    pc.u8()
                    pc.bar()
                    pc.u8()
                    pc.bar()
                    pc.ref3()
                    pc.bar()
                    pc.ref3()
                    pc.bar()
                    pc.u32()
                    pc.bar()
                    pc.f32()
                    pc.bar()
                    pc.u32()
                    pc.bar()
                    pkg = self.resolve(pc.ref3())
                    pc.bar()
                    res["current_package"] = pkg
                    has_b20 = bool(fl & (1 << 20))
                    has_b21 = bool(fl & (1 << 21))
                    legacy = (
                        res["process_level"] == 0
                        and pkg is not None
                        and not has_b20
                        and not has_b21
                    )
                    want_dmg = res["process_level"] <= 3 and (has_b21 or legacy)
                    want_perm = res["process_level"] <= 3 and (has_b20 or legacy)
                    if want_dmg or want_perm:
                        tok18 = pc.u8()
                        pc.bar()
                        res["tok18"] = tok18
                        if pkg is not None:
                            if tok18 in (0x00, 0xFF):
                                pass
                            elif tok18 == 0x0E:
                                for _ in range(3):
                                    pc.u32()
                                    pc.bar()
                                pc.ref3()
                                pc.bar()
                            elif tok18 == 0x0D:
                                for _ in range(3):
                                    pc.u32()
                                    pc.bar()
                                pc.u8()
                                pc.bar()
                                pc.u8()
                                pc.bar()
                                pc.u32()
                                pc.bar()
                                pc.ref3()
                                pc.bar()
                                n = pc.vsval()
                                pc.bar()
                                for _ in range(n):
                                    pc.ref3()
                                    pc.bar()
                            elif tok18 == 0x0C:
                                for _ in range(8):
                                    pc.u32()
                                    pc.bar()
                                for _ in range(4):
                                    pc.u8()
                                    pc.bar()
                                pc.u32()
                                pc.bar()
                                pc.u8()
                                pc.bar()
                                pc.u32()
                                pc.bar()
                                for _ in range(4):
                                    pc.u32()
                                    pc.bar()
                                pc.take(6)
                                pc.bar()
                                for _ in range(3):
                                    pc.ref3()
                                    pc.bar()
                                n = pc.vsval()
                                pc.bar()
                                for _ in range(n):
                                    for _ in range(3):
                                        pc.u32()
                                        pc.bar()
                                    pc.ref3()
                                    pc.bar()
                            else:
                                raise ParseError(
                                    f"unknown tok18 {tok18:#04x} next={rec['data'][pc.p:pc.p+32].hex()}"
                                )
                            for _ in range(3):
                                pc.f32()
                                pc.bar()
                            pc.ref3()
                            pc.bar()
                            pc.u8()
                            pc.bar()
                        pc.f32()
                        pc.bar()
                        pc.ref3()
                        pc.bar()
                        for _ in range(3):
                            pc.f32()
                            pc.bar()
                        pc.u8()
                        pc.bar()
                        pc.u16()
                        pc.bar()
                        pc.f32()
                        pc.bar()
                        pc.f32()
                        pc.bar()
                        for _ in range(5):
                            pc.ref3()
                            pc.bar()
                        pc.u8()
                        pc.bar()
                        if want_dmg:
                            n1 = pc.vsval()
                            pc.bar()
                            dmg = []
                            for _ in range(n1):
                                cd = pc.u8()
                                pc.bar()
                                vl = pc.f32()
                                pc.bar()
                                dmg.append((cd, vl))
                            res["av_damage"] = dmg
                        if want_perm:
                            u32p = pc.u32()
                            pc.bar()
                            n2 = pc.vsval()
                            pc.bar()
                            perm = []
                            for _ in range(n2):
                                cd = pc.u8()
                                pc.bar()
                                vl = pc.f32()
                                pc.bar()
                                perm.append((cd, vl))
                            res["av_permanent_u32"] = u32p
                            res["av_permanent"] = perm
                except Exception as exc:
                    res["proc_err"] = str(exc)
        except Exception as exc:
            res["err"] = str(exc)
        return res

    def decode_refr(
        self,
        fid: int,
        *,
        include_bit6_bit10: bool = True,
        include_0x6e_0x39: bool = True,
        strict_legacy_0x3e: bool = False,
    ) -> dict[str, Any] | None:
        """Decode `REFR` (kind 0), including created `0xFF` refs, bits 6 & 10
        extra lists (BUG #4 / BUG #11), destructible `0x56`/`0x5B` (BUG #10),
        and dropped weapon `0x6E`/`0x39` (BUG #13).
        """
        rec = self.get_rec(0, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {
            "form": fid,
            "flags": fl,
            "len": len(rec["data"]),
            "raw_head": rec["data"][:48].hex(),
        }
        try:
            if (fid >> 24) == 0xFF:
                at = self.resolve(c.ref3())
                v = [c.f32() for _ in range(6)]
                hf = c.u8()
                base = self.resolve(c.ref3())
                c.bar()
                res["created_hdr"] = {
                    "at": at,
                    "pos": v[:3],
                    "rot": v[3:],
                    "hflag": hf,
                    "base": base,
                }
                if fl & (1 << 2):
                    hl = c.vsval()
                    c.bar()
                    if hl > 0:
                        c.take(hl)
                    res["havok_len"] = hl
            else:
                res["place"] = self.parse_ref_prefix(rec, c)

            if fl & 1:
                res["form_flags"] = c.u32()
                c.bar()
            if fl & (1 << 4):
                res["scale"] = c.f32()
                c.bar()

            extra_bits = (1 << 12) | (1 << 29) | (1 << 31)
            if include_bit6_bit10:
                extra_bits |= (1 << 6) | (1 << 10)
            inv_bits = (1 << 5) | (1 << 27)

            if (fl & extra_bits) and not c.at_end():
                cnt = c.vsval()
                c.bar()
                ex: list[tuple[str, Any]] = []
                for _ in range(cnt):
                    ty = c.u8()
                    c.bar()
                    if ty == 0x0D:
                        ex.append(("0x0D_script", self.parse_script_extra(c)))
                    elif ty == 0x21:
                        r = self.resolve(c.ref3())
                        c.bar()
                        ex.append(("0x21_owner", r))
                    elif ty == 0x2A:
                        lvl = c.u8()
                        c.bar()
                        lfl = c.u8()
                        c.bar()
                        key = self.resolve(c.ref3())
                        c.bar()
                        tr = c.u32()
                        c.bar()
                        un = c.u32()
                        c.bar()
                        ex.append(
                            (
                                "0x2A_lock",
                                {
                                    "level": lvl,
                                    "flags": lfl,
                                    "key": key,
                                    "tries": tr,
                                    "unlocked": un,
                                },
                            )
                        )
                    elif ty == 0x2C:
                        mf = c.u8()
                        c.bar()
                        ex.append(("0x2C_mapmarker", mf))
                    elif ty == 0x50:
                        tf = c.u8()
                        c.bar()
                        tl = c.u8()
                        c.bar()
                        ex.append(("0x50_terminal", {"flags": tf, "lock_level": tl}))
                    elif ty == 0x56:
                        hp = c.f32()
                        c.bar()
                        ex.append(("0x56_hp", hp))
                    elif ty == 0x5B:
                        r = self.resolve(c.ref3())
                        c.bar()
                        stg = c.u32()
                        c.bar()
                        ex.append(("0x5B_dest", (r, stg)))
                    elif ty == 0x6E and include_0x6e_0x39:
                        am = self.resolve(c.ref3())
                        c.bar()
                        rd = c.u32()
                        c.bar()
                        ex.append(("0x6E_ammo", (am, rd)))
                    elif ty == 0x39 and include_0x6e_0x39:
                        r = self.resolve(c.ref3())
                        c.bar()
                        ex.append(("0x39_dropped_from", r))
                    elif ty in (0x1C, 0x22, 0x46, 0x49, 0x6C, 0x74):
                        r = self.resolve(c.ref3())
                        c.bar()
                        ex.append((f"0x{ty:02X}", r))
                    elif ty == 0x30:
                        sc = c.f32()
                        c.bar()
                        ex.append(("0x30_scale", sc))
                    elif ty == 0x73:
                        u = c.u8()
                        c.bar()
                        ex.append(("0x73", u))
                    elif ty == 0x60:
                        u = c.u32()
                        c.bar()
                        ex.append(("0x60", u))
                    elif ty == 0x2F:
                        u = c.u32()
                        c.bar()
                        b = c.u8()
                        c.bar()
                        ex.append(("0x2F", (u, b)))
                    elif ty == 0x25:
                        f = c.f32()
                        c.bar()
                        ex.append(("0x25_cond", f))
                    elif ty == 0x24:
                        h = c.i16()
                        c.bar()
                        ex.append(("0x24_count", h))
                    elif ty == 0x18:
                        r = self.resolve(c.ref3())
                        c.bar()
                        c.take(12)
                        c.bar()
                        f = c.f32()
                        c.bar()
                        ex.append(("0x18", (r, f)))
                    elif ty == 0x75:
                        r1 = self.resolve(c.ref3())
                        c.bar()
                        r2 = self.resolve(c.ref3())
                        c.bar()
                        u = c.u8()
                        c.bar()
                        ex.append(("0x75", (r1, r2, u)))
                    elif ty == 0x8B:
                        c.take(12)
                        c.bar()
                        c.ref3()
                        c.bar()
                        c.u32()
                        c.bar()
                        c.u8()
                        c.bar()
                        ex.append(("0x8B", True))
                    elif ty == 0x92:
                        u1 = c.u32()
                        c.bar()
                        u2 = c.u32()
                        c.bar()
                        ex.append(("0x92", (u1, u2)))
                    else:
                        raise ParseError(
                            f"unknown refr extra 0x{ty:02X} at {c.p:#x} next={rec['data'][c.p:c.p+24].hex()}"
                        )
                res["extras"] = ex

            if (fl & inv_bits) and not c.at_end():
                res["inventory"] = self.parse_item_list(
                    c, strict_legacy_0x3e=strict_legacy_0x3e
                )
            if (fl & ((1 << 23) | (1 << 28))) and not c.at_end():
                blen = c.vsval()
                c.bar()
                anim = c.string_field(True)
                res["anim"] = {"blen": blen, "name": anim}
            res["rem"] = c.rem()
        except Exception as exc:
            res["err"] = str(exc)
        return res

    def decode_pgre(self, fid: int) -> dict[str, Any] | None:
        """Decode `PGRE` (kind 4): placement/Havok (bits 1-3) + form_flags (bit 0)."""
        rec = self.get_rec(4, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & 0b1110:
            res["place"] = self.parse_ref_prefix(rec, c)
        if fl & 1:
            res["lead_u8"] = c.u8()
            c.bar()
            res["form_flags"] = c.u32()
            c.bar()
        res["rem"] = c.rem()
        return res

    def decode_cell(self, fid: int) -> dict[str, Any] | None:
        """Decode `CELL` (kind 7): bit 30 detach hours, bit 1 flags, bit 31 fog,
        and trailing bit 3 owner AFTER bit-31 fog (BUG #5).
        """
        rec = self.get_rec(7, fid)
        if not rec:
            return None
        fl = rec["flags"]
        d = rec["data"]
        c = Cursor(d, 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(d)}
        if fl & (1 << 30):
            res["detach_hours"] = c.u32()
            c.bar()
        if fl & (1 << 1):
            res["cell_flags"] = c.u8()
            c.bar()
        if fl & (1 << 3):
            # Bit 3 owner ref3| is serialized at the end after bit-31 fog.
            if fl & (1 << 31):
                if c.rem() >= 4 and d[-1] == BAR:
                    res["fog_len"] = (len(d) - 4) - c.p
                    res["owner"] = self.resolve(d[-4:-1])
                else:
                    raise ParseError("CELL bit 3+31 missing trailing ref3|")
            else:
                res["owner"] = self.resolve(c.ref3())
                c.bar()
        elif fl & (1 << 31):
            res["fog_len"] = c.rem()
        return res

    def decode_eczn(self, fid: int) -> dict[str, Any] | None:
        """Decode `ECZN` (kind 32): bit 1 flags (`u8 |`) and bit 31 stamps (`4 * u32 |`)."""
        rec = self.get_rec(32, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 1):
            res["zone_flags"] = c.u8()
            c.bar()
        if fl & (1 << 31):
            res["stamps"] = tuple(c.u32() for _ in range(4))
            c.bar()
        res["rem"] = c.rem()
        return res

    def decode_tact(self, fid: int) -> dict[str, Any] | None:
        """Decode `TACT` (kind 13): bit 2 name (`u16|str|`) before bit 23 actor (`ref3|`)."""
        rec = self.get_rec(13, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 2):
            res["name"] = c.string_field(True)
        if fl & (1 << 23):
            res["talking_actor"] = self.resolve(c.ref3())
            c.bar()
        res["rem"] = c.rem()
        return res

    def decode_misc(self, fid: int) -> dict[str, Any] | None:
        """Decode `MISC` (kind 22): bit 0 form_flags (`u32 |`) and bit 1 cap value (`i32 |`)."""
        rec = self.get_rec(22, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 0):
            res["form_flags"] = c.u32()
            c.bar()
        if fl & (1 << 1):
            res["value"] = c.i32()
            c.bar()
        res["rem"] = c.rem()
        return res

    def decode_qust(self, fid: int) -> dict[str, Any] | None:
        """Decode `QUST` (kind 9): bit 1 flags, bit 2 delay, bit 31 stages, bit 30 vars, bit 29 objectives."""
        rec = self.get_rec(9, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 1):
            res["quest_flags"] = c.u8()
            c.bar()
        if fl & (1 << 2):
            res["delay"] = c.f32()
            c.bar()
        if fl & (1 << 29):
            # Objectives sit at the tail: `vsval n | n * (u32 idx | u32 state |)`.
            # Try largest non-zero k first so a trailing `00 7c` inside a u32 state
            # cannot falsely match k == 0.
            d = rec["data"]
            matched = False
            for k in range(128, 0, -1):
                st = len(d) - k * 10
                if st < 2:
                    continue
                hs = self._match_vsval_hdr(d, st, k)
                if hs is None:
                    continue
                if hs > 0 and d[hs - 1] != BAR:
                    continue
                oc = Cursor(d, st)
                objs: list[tuple[int, int]] = []
                try:
                    for _ in range(k):
                        oi = oc.u32()
                        oc.bar()
                        os_val = oc.u32()
                        oc.bar()
                        objs.append((oi, os_val))
                    if oc.at_end():
                        res["objectives"] = objs
                        matched = True
                        break
                except ParseError:
                    continue
            if not matched and len(d) >= 2 and d[-2:] == b"\x00\x7c":
                res["objectives"] = []
        return res

    def decode_fact(self, fid: int) -> dict[str, Any] | None:
        """Decode `FACT` (kind 34): bit 1 flags (`u32 |`) and bit 2 relations."""
        rec = self.get_rec(34, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 1):
            res["faction_flags"] = c.u32()
            c.bar()
        if fl & (1 << 2):
            n = c.vsval()
            c.bar()
            rels = []
            for _ in range(n):
                target = self.resolve(c.ref3())
                c.bar()
                mod = c.i32()
                c.bar()
                reaction = c.u32()
                c.bar()
                rels.append((target, mod, reaction))
            res["relations"] = rels
        res["rem"] = c.rem()
        return res

    def decode_flst(self, fid: int) -> dict[str, Any] | None:
        """Decode `FLST` (kind 37) bit 31: `u32 count | count * (ref3 |)`."""
        rec = self.get_rec(37, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 31):
            count = c.u32()
            c.bar()
            forms = []
            for _ in range(count):
                forms.append(self.resolve(c.ref3()))
                c.bar()
            res["forms"] = forms
        res["rem"] = c.rem()
        return res

    def decode_lvli(self, fid: int) -> dict[str, Any] | None:
        """Decode `LVLI` (kind 40) bit 31: `vsval n | n * (ref3 | u16 lvl | u16 cnt | f32 cond |)`."""
        rec = self.get_rec(40, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 31):
            n = c.vsval()
            c.bar()
            entries = []
            for _ in range(n):
                form = self.resolve(c.ref3())
                c.bar()
                lvl = c.u16()
                c.bar()
                cnt = c.u16()
                c.bar()
                cond = c.f32()
                c.bar()
                entries.append((form, lvl, cnt, cond))
            res["entries"] = entries
        res["rem"] = c.rem()
        return res

    def decode_repu(self, fid: int) -> dict[str, Any] | None:
        """Decode `REPU` (kind 43) bit 1: `f32 fame | f32 infamy |`."""
        rec = self.get_rec(43, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 1):
            res["fame"] = c.f32()
            c.bar()
            res["infamy"] = c.f32()
            c.bar()
        res["rem"] = c.rem()
        return res

    def decode_chal(self, fid: int) -> dict[str, Any] | None:
        """Decode `CHAL` (kind 50) bit 1: `u32 progress | u32 flags |`."""
        rec = self.get_rec(50, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 1):
            res["progress"] = c.u32()
            c.bar()
            res["chal_flags"] = c.u32()
            c.bar()
        res["rem"] = c.rem()
        return res

    def decode_clas(self, fid: int) -> dict[str, Any] | None:
        """Decode `CLAS` (kind 33) bit 1: `4 * (u32 tag_skill |)`."""
        rec = self.get_rec(33, fid)
        if not rec:
            return None
        fl = rec["flags"]
        c = Cursor(rec["data"], 0)
        res: dict[str, Any] = {"form": fid, "flags": fl, "len": len(rec["data"])}
        if fl & (1 << 1):
            tags = []
            for _ in range(4):
                tags.append(c.u32())
                c.bar()
            res["tag_skills"] = tags
        res["rem"] = c.rem()
        return res


def diff_saves(
    save_a: FosFile | str | os.PathLike[str],
    save_b: FosFile | str | os.PathLike[str],
    *,
    ignore_globals: tuple[int, ...] = (3, 4, 5),
) -> dict[str, Any]:
    """Compare two `.fos` saves and return a structured diff summary."""
    a = save_a if isinstance(save_a, FosFile) else FosFile(save_a)
    b = save_b if isinstance(save_b, FosFile) else FosFile(save_b)

    global_diffs: dict[int, dict[str, Any]] = {}
    for g in sorted(set(a.globals1) | set(b.globals1)):
        if g in ignore_globals:
            continue
        da, db = a.globals1.get(g, b""), b.globals1.get(g, b"")
        if da != db:
            entry: dict[str, Any] = {"len_a": len(da), "len_b": len(db)}
            if g == 1:
                entry["a"] = a.decode_global1()
                entry["b"] = b.decode_global1()
            elif g == 2:
                entry["a"] = a.decode_dead_counts()
                entry["b"] = b.decode_dead_counts()
            elif g == 8:
                entry["a"] = a.decode_weather()
                entry["b"] = b.decode_weather()
            elif g == 10:
                entry["a"] = a.decode_radio()
                entry["b"] = b.decode_radio()
            global_diffs[g] = entry

    keys_a = set(a.by_key.keys())
    keys_b = set(b.by_key.keys())
    new_forms: list[dict[str, Any]] = []
    for k, fid in sorted(keys_b - keys_a, key=lambda x: (x[0], -1 if x[1] is None else x[1])):
        rb = b.by_key[(k, fid)]
        new_forms.append(
            {
                "kind": k,
                "kind_name": KIND_NAMES.get(k, str(k)),
                "form": fid,
                "flags": rb["flags"],
                "len": len(rb["data"]),
                "hex_head": rb["data"][:40].hex(),
            }
        )

    modified_forms: list[dict[str, Any]] = []
    for k, fid in sorted(keys_a & keys_b, key=lambda x: (x[0], -1 if x[1] is None else x[1])):
        ra, rb = a.by_key[(k, fid)], b.by_key[(k, fid)]
        if (
            ra["flags"] != rb["flags"]
            or len(ra["data"]) != len(rb["data"])
            or (k not in (1, 2) and ra["data"] != rb["data"])
        ):
            modified_forms.append(
                {
                    "kind": k,
                    "kind_name": KIND_NAMES.get(k, str(k)),
                    "form": fid,
                    "flags_a": ra["flags"],
                    "flags_b": rb["flags"],
                    "len_a": len(ra["data"]),
                    "len_b": len(rb["data"]),
                }
            )

    return {
        "a": os.path.basename(a.path),
        "b": os.path.basename(b.path),
        "plugins": (len(a.plugins), len(b.plugins)),
        "changed_count": (len(a.changed), len(b.changed)),
        "worldspaces": (a.worldspaces, b.worldspaces),
        "global_diffs": global_diffs,
        "new_forms": new_forms,
        "modified_forms": modified_forms,
    }


def format_diff(diff: dict[str, Any]) -> str:
    """Format a `diff_saves` result as readable text."""
    lines = [
        f"=== DIFF {diff['a']} -> {diff['b']} ===",
        f"Plugins: {diff['plugins'][0]} -> {diff['plugins'][1]} | "
        f"ChangedForms: {diff['changed_count'][0]} -> {diff['changed_count'][1]}",
    ]
    for g, info in diff["global_diffs"].items():
        lines.append(f"  Global[{g}]: len {info['len_a']} -> {info['len_b']}")
    if diff["new_forms"]:
        lines.append(f"  NEW changed forms ({len(diff['new_forms'])}):")
        for item in diff["new_forms"][:30]:
            fid_s = f"{item['form']:#010x}" if item["form"] is not None else "None"
            lines.append(
                f"    + {item['kind_name']} {fid_s} flags={item['flags']:#010x} "
                f"len={item['len']} hex={item['hex_head']}"
            )
    if diff["modified_forms"]:
        lines.append(f"  MODIFIED changed forms ({len(diff['modified_forms'])}):")
        for item in diff["modified_forms"][:30]:
            fid_s = f"{item['form']:#010x}" if item["form"] is not None else "None"
            lines.append(
                f"    ~ {item['kind_name']} {fid_s} "
                f"flags={item['flags_a']:#010x}->{item['flags_b']:#010x} "
                f"len={item['len_a']}->{item['len_b']}"
            )
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(
        description="Clean-room `.fos` binary parser and A/B save-pair diff tool."
    )
    sub = ap.add_subparsers(dest="cmd", required=True)

    p_diff = sub.add_parser("diff", help="Diff two `.fos` save files byte-by-byte.")
    p_diff.add_argument("save_a", help="Baseline Save A (.fos)")
    p_diff.add_argument("save_b", help="Mutated Save B (.fos)")
    p_diff.add_argument("--json", action="store_true", help="Emit JSON output.")

    p_scan = sub.add_parser("scan", help="Parse and summarize every `.fos` file in a directory.")
    p_scan.add_argument("--saves-dir", required=True, help="Directory containing `.fos` captures.")

    args = ap.parse_args(argv)
    if args.cmd == "diff":
        diff = diff_saves(args.save_a, args.save_b)
        if args.json:
            print(json.dumps(diff, indent=2, default=str))
        else:
            print(format_diff(diff))
        return 0
    if args.cmd == "scan":
        files = sorted(glob.glob(os.path.join(args.saves_dir, "*.fos")))
        print(f"Found {len(files)} .fos files in {args.saves_dir}")
        for path in files:
            ff = FosFile(path)
            print(
                f"  {os.path.basename(path)}: ver={ff.form_version} "
                f"plugins={len(ff.plugins)} changed={len(ff.changed)}"
            )
        return 0
    return 1


if __name__ == "__main__":
    sys.exit(main())
