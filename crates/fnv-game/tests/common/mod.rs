//! Builders for synthetic plugins and hand-written bytecode (docs/spec-notes/scripts.md
//! §2), shared by the function tests. No game data.
#![allow(dead_code)]

use fnv_esm::raw::{write_subrecords, Subrecord};

pub fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord {
    Subrecord { sig: *sig, data, extended: false }
}

pub fn z(s: &str) -> Vec<u8> {
    [s.as_bytes(), &[0]].concat()
}

pub fn u32s(v: &[u32]) -> Vec<u8> {
    v.iter().flat_map(|x| x.to_le_bytes()).collect()
}

pub fn record(sig: &[u8; 4], flags: u32, fid: u32, subs: &[Subrecord]) -> Vec<u8> {
    let payload = write_subrecords(subs);
    [&sig[..], &(payload.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &fid.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &payload]
        .concat()
}

pub fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &label, &gtype.to_le_bytes(), &[0; 8], body].concat()
}

pub fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

pub fn tes4() -> Vec<u8> {
    let hedr = [1.34f32.to_le_bytes(), 0i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
    record(b"TES4", 1, 0, &[sub(b"HEDR", hedr)])
}

/// SCHR + SCDA + variables (`(index, name, 1 = Integer / 0 = FloatOrRef)`) + SCRO list.
pub fn unit(kind: u16, scda: &[u8], vars: &[(u32, &str, u8)], refs: &[u32]) -> Vec<Subrecord> {
    let mut schr = vec![0u8; 4];
    schr.extend((refs.len() as u32).to_le_bytes());
    schr.extend((scda.len() as u32).to_le_bytes());
    schr.extend((vars.len() as u32).to_le_bytes());
    schr.extend(kind.to_le_bytes());
    schr.extend(1u16.to_le_bytes());
    let mut out = vec![sub(b"SCHR", schr)];
    if !scda.is_empty() {
        out.push(sub(b"SCDA", scda.to_vec()));
    }
    for (i, n, t) in vars {
        let mut slsd = i.to_le_bytes().to_vec();
        slsd.extend([0u8; 12]);
        slsd.push(*t);
        slsd.extend([0u8; 7]);
        out.push(sub(b"SLSD", slsd));
        out.push(sub(b"SCVR", z(n)));
    }
    for r in refs {
        out.push(sub(b"SCRO", r.to_le_bytes().to_vec()));
    }
    out
}

pub fn le16(v: u16) -> Vec<u8> {
    v.to_le_bytes().to_vec()
}
pub fn st(op: u16, body: &[u8]) -> Vec<u8> {
    [le16(op), le16(body.len() as u16), body.to_vec()].concat()
}
pub fn ex(tokens: &[Vec<u8>]) -> Vec<u8> {
    tokens.iter().flat_map(|t| [vec![b' '], t.clone()].concat()).collect()
}
pub fn a(s: &str) -> Vec<u8> {
    s.as_bytes().to_vec()
}
pub fn var(letter: u8, i: u16) -> Vec<u8> {
    [vec![letter], le16(i)].concat()
}
pub fn set(target: Vec<u8>, e: Vec<u8>) -> Vec<u8> {
    st(0x15, &[target, le16(e.len() as u16), e].concat())
}
pub fn xcall(op: u16, blob: &[u8]) -> Vec<u8> {
    [vec![b'X'], le16(op), le16(blob.len() as u16), blob.to_vec()].concat()
}

/// One block: event code, parameter block, body.
pub type BlockBytes = (u16, Option<Vec<u8>>, Vec<u8>);

/// ScriptName, then `Begin <event> [params]` … `End` around each body.
pub fn script(blocks: &[BlockBytes]) -> Vec<u8> {
    let mut out = st(0x1D, &[]);
    for (event, params, body) in blocks {
        let end = st(0x11, &[]);
        let len = (body.len() + end.len()) as u32;
        let mut b = [le16(*event), len.to_le_bytes().to_vec()].concat();
        if let Some(p) = params {
            b.extend(p);
        }
        out.extend(st(0x10, &b));
        out.extend(body);
        out.extend(end);
    }
    out
}
