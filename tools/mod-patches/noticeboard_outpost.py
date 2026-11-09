"""Writes `NoticeBoard - Classic Mojave Outpost.esp`, a patch plugin for two mods in the
owner's load order (queue item 100, 2026-11-09).

Why [observed: the plugins' records and NVForever screenshots, 2026-11-09]: Notice Board
(NoticeBoard.esm) stands its board, TACT ref 0x01008595 `xxxNoticeBoardTalker`, with three
papers pinned to it (0x01008573-75), by the Mojave Outpost wall at (-84010, -103615). It was
placed beside vanilla FalloutNV.esm crate 0x161CE4, 137 units to the board's side.
brayduck_classic.esm (classic Mojave Outpost), later in the order, moves that crate on its
side to 38 units from the board, so it covers the board's left part. The engine places both
as their records say; in the original game, with this order, they overlap the same way
[hypothesis: not checked in the original].

The patch moves the board and its papers along the board's own width, clear of the crate,
in front of the outpost's garage door, keeping its facing and height. The WRLD and CELL
records are copied from NoticeBoard.esm: they equal the winning ones in the owner's order
(checked 2026-11-09), so no other mod's edits are undone. The patch loads after both mods.

Usage: python -I noticeboard_outpost.py <NoticeBoard.esm> <out.esp> <along_width> <along_facing> [turn_rad]
Item 100 used `250 0`. Reads the owner's plugin; nothing from it is stored here.
"""
import math, struct, sys, zlib

src, out = sys.argv[1], sys.argv[2]
w, n = float(sys.argv[3]), float(sys.argv[4])
turn = float(sys.argv[5]) if len(sys.argv) > 5 else 0.0
data = open(src, 'rb').read()

BOARD = 0x01008595
PAPERS = [0x01008573, 0x01008574, 0x01008575]
WANT = {0x000DA726, 0x000846EA, 0x000E1B72, BOARD, *PAPERS}

found = {}      # fid -> (raw record bytes, group stack of raw headers)
def walk(off, end, stack):
    while off < end:
        sig = data[off:off+4]; size = struct.unpack_from('<I', data, off+4)[0]
        if sig == b'GRUP':
            walk(off+24, off+size, stack + [data[off:off+24]]); off += size; continue
        fid = struct.unpack_from('<I', data, off+12)[0]
        if fid in WANT:
            found[fid] = (data[off:off+24+size], stack)
        off += 24 + size
walk(0, len(data), [])
assert set(found) == WANT, sorted(hex(f) for f in WANT - set(found))

def subs(b):
    o = 0; res = []
    while o < len(b):
        s = b[o:o+4]; k = struct.unpack_from('<H', b, o+4)[0]; res.append([s, b[o+6:o+6+k]]); o += 6 + k
    return res

def ref_data(fid):
    raw = found[fid][0]
    assert not struct.unpack_from('<I', raw, 8)[0] & 0x40000
    return struct.unpack('<6f', dict((s, v) for s, v in subs(raw[24:]))[b'DATA'])

bx, by, bz, _, _, brz = ref_data(BOARD)
# The board's axes, taken from the data: the papers spread along (cos rz', sin rz') with rz' = -rz
# [observed: their offsets from the board, 2026-11-09], and stand +6 units along the
# normal (-sin rz', cos rz').
a = -brz
width = (math.cos(a), math.sin(a)); facing = (-math.sin(a), math.cos(a))
dx = w * width[0] + n * facing[0]; dy = w * width[1] + n * facing[1]
c, s = math.cos(-turn), math.sin(-turn)   # turn: clockwise like rz

def moved(fid):
    raw = bytearray(found[fid][0])
    x, y, z, rx, ry, rz = ref_data(fid)
    ox, oy = x - bx, y - by
    nx = bx + dx + ox * c - oy * s; ny = by + dy + ox * s + oy * c
    out = []
    for sg, v in subs(bytes(raw[24:])):
        if sg == b'DATA':
            v = struct.pack('<6f', nx, ny, z, rx, ry, rz + turn)
        out.append(sg + struct.pack('<H', len(v)) + v)
    body = b''.join(out)
    hdr = raw[:24]; struct.pack_into('<I', hdr, 4, len(body))
    print(f'{fid:#010x}: ({x:.1f}, {y:.1f}) -> ({nx:.1f}, {ny:.1f}), rz {rz:.3f} -> {rz+turn:.3f}')
    return bytes(hdr) + body

def group(hdr, content):
    h = bytearray(hdr); struct.pack_into('<I', h, 4, 24 + len(content)); return bytes(h) + content

wrld, wst = found[0x000DA726]
pcell, pst = found[0x000846EA]
ecell, est = found[0x000E1B72]
bst = found[BOARD][1]; pps = found[PAPERS[0]][1]
assert all(found[p][1] == pps for p in PAPERS)
# stacks: [top WRLD, world children, (6 cell children, 8 persistent)] and
#         [top WRLD, world children, 4 block, 5 sub-block, 6 cell children, 9 temporary]
persistent = group(bst[2], group(bst[3], moved(BOARD)))
temporary = group(pps[2], group(pps[3], ecell + group(pps[4], group(pps[5], b''.join(moved(p) for p in PAPERS)))))
world = group(bst[0], wrld + group(bst[1], pcell + persistent + temporary))

def sub(s, v): return s + struct.pack('<H', len(v)) + v
records = 1 + 2 + 1 + 3   # WRLD, two CELLs, board, papers
hedr = struct.pack('<fiI', 1.34, records, 0x800)
tes4 = (sub(b'HEDR', hedr) + sub(b'CNAM', b'NVForever\0')
        + sub(b'SNAM', b'Moves the Notice Board at the Mojave Outpost clear of the crate Classic Mojave Outpost puts in front of it.\0')
        + sub(b'MAST', b'FalloutNV.esm\0') + sub(b'DATA', bytes(8))
        + sub(b'MAST', b'NoticeBoard.esm\0') + sub(b'DATA', bytes(8)))
header = b'TES4' + struct.pack('<IIIIHH', len(tes4), 0, 0, 0, 15, 0) + tes4
open(out, 'wb').write(header + world)
print('wrote', out, len(header + world), 'bytes; shift', round(dx, 1), round(dy, 1))
