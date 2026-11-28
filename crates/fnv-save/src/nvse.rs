//! Clean-room offline `.nvse` co-save binary reader (`docs/spec-notes/nvse-cosaves.md`).
//!
//! The NVForever runtime never reads or writes `.nvse` co-saves directly (the runtime
//! stays `.nvfsave`-only). This module parses the `NVSE` v1 co-save container and the
//! `[observed]` chunk layouts established by controlled single-variable A/B captures
//! (`tools/capture/nvse_cosave/`) for `fnvtool save inspect-fos` and `fnvtool save convert`.
//! Unknown plugin opcodes, unknown chunk tags, and unsupported chunk versions fail soft
//! and are reported via [`NvseCosave::unknown_chunks`].

use std::fmt;

/// Container magic at offset 0 (`docs/spec-notes/nvse-cosaves.md` §1.1).
pub const MAGIC: &[u8; 4] = b"NVSE";
/// Supported co-save container format version (`docs/spec-notes/nvse-cosaves.md` §1.1).
pub const FORMAT_VERSION: u32 = 1;

/// Observed plugin opcode bases (`docs/spec-notes/nvse-cosaves.md` §1.2).
pub const PLUGIN_NVSE: u32 = 0x0000_1400;
pub const PLUGIN_JIP: u32 = 0x0000_2200;
pub const PLUGIN_JOHNNYGUITAR: u32 = 0x0000_3100;
pub const PLUGIN_SHOWOFF: u32 = 0x0000_3C93;

/// Error returned when an `.nvse` container or a decoded chunk payload is malformed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvseError(pub String);

impl fmt::Display for NvseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for NvseError {}

/// Bounds-checked little-endian byte cursor.
struct Cursor<'a> {
    b: &'a [u8],
    at: usize,
}

impl<'a> Cursor<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, at: 0 }
    }

    fn remaining(&self) -> usize {
        self.b.len().saturating_sub(self.at)
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], NvseError> {
        let end = self
            .at
            .checked_add(n)
            .ok_or_else(|| NvseError(format!("length overflow ({n}) at offset {}", self.at)))?;
        let slice = self.b.get(self.at..end).ok_or_else(|| {
            NvseError(format!(
                "truncated: need {n} byte(s) at offset {}, only {} remaining",
                self.at,
                self.remaining()
            ))
        })?;
        self.at = end;
        Ok(slice)
    }

    fn u8(&mut self) -> Result<u8, NvseError> {
        Ok(self.take(1)?[0])
    }

    fn u16(&mut self) -> Result<u16, NvseError> {
        let s = self.take(2)?;
        Ok(u16::from_le_bytes([s[0], s[1]]))
    }

    fn u32(&mut self) -> Result<u32, NvseError> {
        let s = self.take(4)?;
        Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn i32(&mut self) -> Result<i32, NvseError> {
        let s = self.take(4)?;
        Ok(i32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }

    fn f64(&mut self) -> Result<f64, NvseError> {
        let s = self.take(8)?;
        Ok(f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
    }

    fn str_u8(&mut self) -> Result<String, NvseError> {
        let n = usize::from(self.u8()?);
        Ok(decode_cp1252(self.take(n)?))
    }

    fn str_u16(&mut self) -> Result<String, NvseError> {
        let n = usize::from(self.u16()?);
        Ok(decode_cp1252(self.take(n)?))
    }

    fn expect_eof(&self, what: &str) -> Result<(), NvseError> {
        if self.remaining() == 0 {
            Ok(())
        } else {
            Err(NvseError(format!("{what}: {} unexpected trailing byte(s)", self.remaining())))
        }
    }
}

/// Decode ASCII / Windows-1252 string bytes without panicking.
fn decode_cp1252(bytes: &[u8]) -> String {
    if let Ok(s) = std::str::from_utf8(bytes) {
        return s.to_string();
    }
    const CP1252_HIGH: [char; 32] = [
        '\u{20AC}', '\u{FFFD}', '\u{201A}', '\u{0192}', '\u{201E}', '\u{2026}', '\u{2020}',
        '\u{2021}', '\u{02C6}', '\u{2030}', '\u{0160}', '\u{2039}', '\u{0152}', '\u{FFFD}',
        '\u{017D}', '\u{FFFD}', '\u{FFFD}', '\u{2018}', '\u{2019}', '\u{201C}', '\u{201D}',
        '\u{2022}', '\u{2013}', '\u{2014}', '\u{02DC}', '\u{2122}', '\u{0161}', '\u{203A}',
        '\u{0153}', '\u{FFFD}', '\u{017E}', '\u{0178}',
    ];
    bytes
        .iter()
        .map(|&b| match b {
            0x00..=0x7F => b as char,
            0x80..=0x9F => CP1252_HIGH[(b - 0x80) as usize],
            0xA0..=0xFF => char::from(b),
        })
        .collect()
}

/// Format a 4-byte chunk tag as printable ASCII (or hex escapes for non-ASCII bytes).
pub fn format_tag(tag: [u8; 4]) -> String {
    if tag.iter().all(|b| b.is_ascii_graphic()) {
        String::from_utf8_lossy(&tag).into_owned()
    } else {
        format!("0x{:02X}{:02X}{:02X}{:02X}", tag[0], tag[1], tag[2], tag[3])
    }
}

/// 20-byte `.nvse` container header (`docs/spec-notes/nvse-cosaves.md` §1.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvseHeader {
    pub format_version: u32,
    pub nvse_version: u32,
    pub runtime_version: u32,
    pub plugin_count: u32,
}

/// One raw chunk inside a plugin block (`docs/spec-notes/nvse-cosaves.md` §1.3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvseChunk {
    pub tag: [u8; 4],
    pub version: u32,
    pub data: Vec<u8>,
}

/// One plugin block inside an `.nvse` co-save (`docs/spec-notes/nvse-cosaves.md` §1.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvsePlugin {
    pub opcode: u32,
    pub chunks: Vec<NvseChunk>,
}

/// Parsed `.nvse` co-save container.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NvseCosave {
    pub header: NvseHeader,
    pub plugins: Vec<NvsePlugin>,
}

/// xNVSE array kind (`docs/spec-notes/nvse-cosaves.md` §2.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NvseArrayKind {
    List,
    Map,
    StringMap,
}

/// Key inside an xNVSE `RVRA` (`'ARVR'`) array record.
#[derive(Debug, Clone, PartialEq)]
pub enum NvseArrayKey {
    Num(f64),
    Str(String),
}

/// Value inside an xNVSE `RVRA` (`'ARVR'`) array record.
#[derive(Debug, Clone, PartialEq)]
pub enum NvseArrayVal {
    Num(f64),
    Form(u32),
    Str(String),
    Array(u32),
}

/// Decoded xNVSE `RVRA` (`'ARVR'` v2) array record (`docs/spec-notes/nvse-cosaves.md` §2.2).
#[derive(Debug, Clone, PartialEq)]
pub struct NvseArray {
    pub owning_mod: u8,
    pub id: u32,
    pub kind: NvseArrayKind,
    pub ref_mods: Vec<u8>,
    pub entries: Vec<(NvseArrayKey, NvseArrayVal)>,
}

/// Shared value encoding for JIP `JPAV`/`JPRM` and ShowOff `OSMS`
/// (`docs/spec-notes/nvse-cosaves.md` §3.1).
#[derive(Debug, Clone, PartialEq)]
pub enum JipVal {
    Num(f64),
    Form(u32),
    Str(String),
}

/// Decoded JIP LN `JPAV` v10 permanent auxiliary variable (`docs/spec-notes/nvse-cosaves.md` §3.1).
#[derive(Debug, Clone, PartialEq)]
pub struct JipAuxVar {
    pub mod_index: u8,
    pub owner: u32,
    pub name: String,
    pub elements: Vec<JipVal>,
}

/// Decoded JIP LN `JPRM` v10 permanent RefMap (`docs/spec-notes/nvse-cosaves.md` §3.2).
#[derive(Debug, Clone, PartialEq)]
pub struct JipRefMap {
    pub mod_index: u8,
    pub name: String,
    pub entries: Vec<(u32, JipVal)>,
}

/// Decoded JIP LN `JPLR` v9 linked reference (`docs/spec-notes/nvse-cosaves.md` §3.3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct JipLinkedRef {
    pub reference: u32,
    pub linked: u32,
    pub mod_index: u8,
}

/// Decoded 8-byte value union in JIP LN `JPSV` v9 (`docs/spec-notes/nvse-cosaves.md` §3.4).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum JipAddedVarVal {
    Num(f64),
    Form(u32),
}

/// Decoded JIP LN `JPSV` v9 runtime-added script variable (`docs/spec-notes/nvse-cosaves.md` §3.4).
#[derive(Debug, Clone, PartialEq)]
pub struct JipAddedScriptVar {
    pub owner: u32,
    pub mod_index: u8,
    pub name: String,
    pub value: JipAddedVarVal,
}

/// Decoded JohnnyGuitar `SMGJ` (`'JGMS'` v1) extra misc stat (`docs/spec-notes/nvse-cosaves.md` §4.1).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JgExtraMiscStat {
    pub name: String,
    pub value: i32,
}

/// Decoded ShowOff `OSMS` (`'SMSO'` v10) permanent `AuxStringMapArray`
/// (`docs/spec-notes/nvse-cosaves.md` §5.1).
#[derive(Debug, Clone, PartialEq)]
pub struct ShowOffStrMap {
    pub mod_index: u8,
    pub name: String,
    pub entries: Vec<(String, JipVal)>,
}

/// Decoded ShowOff `SOAT` v1 persistent aux timer (`docs/spec-notes/nvse-cosaves.md` §5.2).
#[derive(Debug, Clone, PartialEq)]
pub struct ShowOffAuxTimer {
    pub mod_index: u8,
    pub owner: u32,
    pub name: String,
    pub time_a: f64,
    pub time_b: f64,
    pub flags: u32,
}

/// Summary of an unmapped/unsupported chunk preserved for truthful reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownChunk {
    pub opcode: u32,
    pub tag: [u8; 4],
    pub version: u32,
    pub length: usize,
}

fn read_jip_val(cur: &mut Cursor<'_>) -> Result<JipVal, NvseError> {
    match cur.u8()? {
        1 => Ok(JipVal::Num(cur.f64()?)),
        2 => Ok(JipVal::Form(cur.u32()?)),
        4 => Ok(JipVal::Str(cur.str_u16()?)),
        other => Err(NvseError(format!("unknown JIP/ShowOff value type tag {other}"))),
    }
}

fn is_known_chunk(opcode: u32, tag: [u8; 4], version: u32) -> bool {
    matches!(
        (opcode, &tag, version),
        (PLUGIN_NVSE, b"SDOM", 0)
            | (PLUGIN_NVSE, b"SVRA" | b"RVRA" | b"EVRA", 2)
            | (PLUGIN_NVSE, b"SVTS" | b"EVTS", 0)
            | (PLUGIN_JIP, b"JPAV" | b"JPRM", 10)
            | (PLUGIN_JIP, b"JPLR" | b"JPSV", 9)
            | (PLUGIN_JOHNNYGUITAR, b"SMGJ", 1)
            | (PLUGIN_SHOWOFF, b"OSMS", 10)
            | (PLUGIN_SHOWOFF, b"SOAT", 1)
    )
}

impl NvseCosave {
    /// Parse an `.nvse` v1 co-save container with strict bounds validation.
    pub fn parse(bytes: &[u8]) -> Result<Self, NvseError> {
        let mut cur = Cursor::new(bytes);
        let magic = cur.take(4)?;
        if magic != MAGIC {
            return Err(NvseError("not an NVSE co-save (bad magic)".into()));
        }
        let format_version = cur.u32()?;
        if format_version != FORMAT_VERSION {
            return Err(NvseError(format!(
                "unsupported NVSE co-save format version {format_version} (expected {FORMAT_VERSION})"
            )));
        }
        let nvse_version = cur.u32()?;
        let runtime_version = cur.u32()?;
        let plugin_count = cur.u32()?;
        let min_bytes = (plugin_count as usize)
            .checked_mul(12)
            .ok_or_else(|| NvseError("plugin_count overflow".into()))?;
        if min_bytes > cur.remaining() {
            return Err(NvseError(format!(
                "plugin_count {plugin_count} exceeds remaining {} byte(s)",
                cur.remaining()
            )));
        }

        let mut plugins = Vec::with_capacity(plugin_count as usize);
        for _ in 0..plugin_count {
            let opcode = cur.u32()?;
            let chunk_count = cur.u32()?;
            let plen = cur.u32()? as usize;
            let pbytes = cur.take(plen)?;
            let mut pcur = Cursor::new(pbytes);
            let min_cbytes = (chunk_count as usize)
                .checked_mul(12)
                .ok_or_else(|| NvseError("chunk_count overflow".into()))?;
            if min_cbytes > pcur.remaining() {
                return Err(NvseError(format!(
                    "plugin 0x{opcode:08X} chunk_count {chunk_count} exceeds plugin length {plen}"
                )));
            }
            let mut chunks = Vec::with_capacity(chunk_count as usize);
            for _ in 0..chunk_count {
                let raw_tag = pcur.take(4)?;
                let tag = [raw_tag[0], raw_tag[1], raw_tag[2], raw_tag[3]];
                let version = pcur.u32()?;
                let clen = pcur.u32()? as usize;
                let data = pcur.take(clen)?.to_vec();
                chunks.push(NvseChunk { tag, version, data });
            }
            pcur.expect_eof(&format!("plugin 0x{opcode:08X}"))?;
            plugins.push(NvsePlugin { opcode, chunks });
        }
        cur.expect_eof("nvse container")?;

        Ok(Self {
            header: NvseHeader {
                format_version,
                nvse_version,
                runtime_version,
                plugin_count,
            },
            plugins,
        })
    }

    fn chunks_for(&self, opcode: u32, tag: [u8; 4], version: u32) -> impl Iterator<Item = &NvseChunk> {
        self.plugins
            .iter()
            .filter(move |p| p.opcode == opcode)
            .flat_map(|p| p.chunks.iter())
            .filter(move |c| c.tag == tag && c.version == version)
    }

    /// Decode xNVSE `SDOM` (`'MODS'` v0) load-order plugin list.
    pub fn mod_list(&self) -> Result<Option<Vec<String>>, NvseError> {
        let Some(chunk) = self.chunks_for(PLUGIN_NVSE, *b"SDOM", 0).next() else {
            return Ok(None);
        };
        let mut cur = Cursor::new(&chunk.data);
        let count = usize::from(cur.u8()?);
        let mut mods = Vec::with_capacity(count);
        for _ in 0..count {
            mods.push(cur.str_u16()?);
        }
        cur.expect_eof("SDOM")?;
        Ok(Some(mods))
    }

    /// Decode every xNVSE `RVRA` (`'ARVR'` v2) array record.
    pub fn arrays(&self) -> Vec<Result<NvseArray, NvseError>> {
        self.chunks_for(PLUGIN_NVSE, *b"RVRA", 2)
            .map(|ch| parse_rvra(&ch.data))
            .collect()
    }

    /// Decode JIP LN `JPAV` v10 permanent auxiliary variables.
    pub fn jip_aux_vars(&self) -> Result<Vec<JipAuxVar>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_JIP, *b"JPAV", 10) {
            let mut cur = Cursor::new(&ch.data);
            let num_mods = usize::from(cur.u16()?);
            for _ in 0..num_mods {
                let mod_index = cur.u8()?;
                let num_owners = usize::from(cur.u16()?);
                for _ in 0..num_owners {
                    let owner = cur.u32()?;
                    let num_vars = usize::from(cur.u16()?);
                    for _ in 0..num_vars {
                        let name = cur.str_u8()?;
                        let num_elems = usize::from(cur.u16()?);
                        let mut elements = Vec::with_capacity(num_elems.min(cur.remaining()));
                        for _ in 0..num_elems {
                            elements.push(read_jip_val(&mut cur)?);
                        }
                        out.push(JipAuxVar { mod_index, owner, name, elements });
                    }
                }
            }
            cur.expect_eof("JPAV")?;
        }
        Ok(out)
    }

    /// Decode JIP LN `JPRM` v10 permanent RefMap arrays.
    pub fn jip_refmaps(&self) -> Result<Vec<JipRefMap>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_JIP, *b"JPRM", 10) {
            let mut cur = Cursor::new(&ch.data);
            let num_mods = usize::from(cur.u16()?);
            for _ in 0..num_mods {
                let mod_index = cur.u8()?;
                let num_maps = usize::from(cur.u16()?);
                for _ in 0..num_maps {
                    let name = cur.str_u8()?;
                    let num_entries = usize::from(cur.u16()?);
                    let mut entries = Vec::with_capacity(num_entries.min(cur.remaining() / 5));
                    for _ in 0..num_entries {
                        let key_form = cur.u32()?;
                        let val = read_jip_val(&mut cur)?;
                        entries.push((key_form, val));
                    }
                    out.push(JipRefMap { mod_index, name, entries });
                }
            }
            cur.expect_eof("JPRM")?;
        }
        Ok(out)
    }

    /// Decode JIP LN `JPLR` v9 linked references.
    pub fn jip_linked_refs(&self) -> Result<Vec<JipLinkedRef>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_JIP, *b"JPLR", 9) {
            let mut cur = Cursor::new(&ch.data);
            let count = usize::from(cur.u16()?);
            if count.saturating_mul(9) > cur.remaining() {
                return Err(NvseError(format!("JPLR count {count} exceeds payload length")));
            }
            for _ in 0..count {
                let reference = cur.u32()?;
                let linked = cur.u32()?;
                let mod_index = cur.u8()?;
                out.push(JipLinkedRef { reference, linked, mod_index });
            }
            cur.expect_eof("JPLR")?;
        }
        Ok(out)
    }

    /// Decode JIP LN `JPSV` v9 runtime-added script variables.
    pub fn jip_added_script_vars(&self) -> Result<Vec<JipAddedScriptVar>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_JIP, *b"JPSV", 9) {
            let mut cur = Cursor::new(&ch.data);
            let num_owners = usize::from(cur.u16()?);
            for _ in 0..num_owners {
                let owner = cur.u32()?;
                let num_vars = usize::from(cur.u16()?);
                for _ in 0..num_vars {
                    let mod_index = cur.u8()?;
                    let name = cur.str_u8()?;
                    let raw = cur.take(8)?;
                    let lo = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]);
                    let hi = u32::from_le_bytes([raw[4], raw[5], raw[6], raw[7]]);
                    let value = if hi == 0 && lo != 0 {
                        JipAddedVarVal::Form(lo)
                    } else {
                        JipAddedVarVal::Num(f64::from_le_bytes([
                            raw[0], raw[1], raw[2], raw[3], raw[4], raw[5], raw[6], raw[7],
                        ]))
                    };
                    out.push(JipAddedScriptVar { owner, mod_index, name, value });
                }
            }
            cur.expect_eof("JPSV")?;
        }
        Ok(out)
    }

    /// Decode JohnnyGuitar `SMGJ` (`'JGMS'` v1) extra misc stats.
    pub fn jg_extra_misc_stats(&self) -> Result<Vec<JgExtraMiscStat>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_JOHNNYGUITAR, *b"SMGJ", 1) {
            let mut cur = Cursor::new(&ch.data);
            let count = usize::from(cur.u16()?);
            for _ in 0..count {
                let name = cur.str_u16()?;
                let value = cur.i32()?;
                out.push(JgExtraMiscStat { name, value });
            }
            cur.expect_eof("SMGJ")?;
        }
        Ok(out)
    }

    /// Decode ShowOff `OSMS` (`'SMSO'` v10) permanent `AuxStringMapArray` entries.
    pub fn showoff_strmaps(&self) -> Result<Vec<ShowOffStrMap>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_SHOWOFF, *b"OSMS", 10) {
            let mut cur = Cursor::new(&ch.data);
            let num_mods = usize::from(cur.u16()?);
            for _ in 0..num_mods {
                let mod_index = cur.u8()?;
                let num_maps = usize::from(cur.u16()?);
                for _ in 0..num_maps {
                    let name = cur.str_u8()?;
                    let num_entries = usize::from(cur.u16()?);
                    let mut entries = Vec::with_capacity(num_entries.min(cur.remaining() / 2));
                    for _ in 0..num_entries {
                        let key = cur.str_u8()?;
                        let val = read_jip_val(&mut cur)?;
                        entries.push((key, val));
                    }
                    out.push(ShowOffStrMap { mod_index, name, entries });
                }
            }
            cur.expect_eof("OSMS")?;
        }
        Ok(out)
    }

    /// Decode ShowOff `SOAT` v1 persistent aux timers.
    pub fn showoff_aux_timers(&self) -> Result<Vec<ShowOffAuxTimer>, NvseError> {
        let mut out = Vec::new();
        for ch in self.chunks_for(PLUGIN_SHOWOFF, *b"SOAT", 1) {
            let mut cur = Cursor::new(&ch.data);
            let num_mods = usize::from(cur.u16()?);
            for _ in 0..num_mods {
                let mod_index = cur.u8()?;
                let num_owners = usize::from(cur.u16()?);
                for _ in 0..num_owners {
                    let owner = cur.u32()?;
                    let num_timers = usize::from(cur.u16()?);
                    for _ in 0..num_timers {
                        let name = cur.str_u8()?;
                        let time_a = cur.f64()?;
                        let time_b = cur.f64()?;
                        let flags = cur.u32()?;
                        out.push(ShowOffAuxTimer { mod_index, owner, name, time_a, time_b, flags });
                    }
                }
            }
            cur.expect_eof("SOAT")?;
        }
        Ok(out)
    }

    /// List all chunks whose `(opcode, tag, version)` is not one of the observed/decoded formats.
    pub fn unknown_chunks(&self) -> Vec<UnknownChunk> {
        let mut out = Vec::new();
        for p in &self.plugins {
            for ch in &p.chunks {
                if !is_known_chunk(p.opcode, ch.tag, ch.version) {
                    out.push(UnknownChunk {
                        opcode: p.opcode,
                        tag: ch.tag,
                        version: ch.version,
                        length: ch.data.len(),
                    });
                }
            }
        }
        out
    }
}

fn parse_rvra(data: &[u8]) -> Result<NvseArray, NvseError> {
    let mut cur = Cursor::new(data);
    let owning_mod = cur.u8()?;
    let id = cur.u32()?;
    let key_type = cur.u8()?;
    let packed = cur.u8()?;
    let num_refs = cur.u32()? as usize;
    let ref_mods = cur.take(num_refs)?.to_vec();
    let num_elems = cur.u32()? as usize;

    let kind = match (packed, key_type) {
        (1, 1) => NvseArrayKind::List,
        (0, 1) => NvseArrayKind::Map,
        (0, 3) => NvseArrayKind::StringMap,
        _ => {
            return Err(NvseError(format!(
                "RVRA unsupported array kind (packed={packed}, key_type={key_type})"
            )));
        }
    };

    let mut entries = Vec::with_capacity(num_elems.min(cur.remaining()));
    for idx in 0..num_elems {
        let key = match kind {
            NvseArrayKind::List => NvseArrayKey::Num(idx as f64),
            NvseArrayKind::Map => NvseArrayKey::Num(cur.f64()?),
            NvseArrayKind::StringMap => NvseArrayKey::Str(cur.str_u16()?),
        };
        let val = match cur.u8()? {
            1 => NvseArrayVal::Num(cur.f64()?),
            2 => NvseArrayVal::Form(cur.u32()?),
            3 => NvseArrayVal::Str(cur.str_u16()?),
            4 => NvseArrayVal::Array(cur.u32()?),
            other => return Err(NvseError(format!("RVRA unknown value type {other}"))),
        };
        entries.push((key, val));
    }
    cur.expect_eof("RVRA")?;
    Ok(NvseArray { owning_mod, id, kind, ref_mods, entries })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chunk(tag: &[u8; 4], ver: u32, payload: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + payload.len());
        out.extend_from_slice(tag);
        out.extend_from_slice(&ver.to_le_bytes());
        out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        out.extend_from_slice(payload);
        out
    }

    fn plugin(opcode: u32, chunks: &[Vec<u8>]) -> Vec<u8> {
        let total: usize = chunks.iter().map(Vec::len).sum();
        let mut out = Vec::with_capacity(12 + total);
        out.extend_from_slice(&opcode.to_le_bytes());
        out.extend_from_slice(&(chunks.len() as u32).to_le_bytes());
        out.extend_from_slice(&(total as u32).to_le_bytes());
        for c in chunks {
            out.extend_from_slice(c);
        }
        out
    }

    fn container(plugins: &[Vec<u8>]) -> Vec<u8> {
        let mut out = Vec::new();
        out.extend_from_slice(MAGIC);
        out.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
        out.extend_from_slice(&0x0004_0006u32.to_le_bytes());
        out.extend_from_slice(&0x0400_20D0u32.to_le_bytes());
        out.extend_from_slice(&(plugins.len() as u32).to_le_bytes());
        for p in plugins {
            out.extend_from_slice(p);
        }
        out
    }

    #[test]
    fn parses_all_observed_chunks_and_reports_unknown_softly() {
        let mut sdom = vec![2u8];
        for name in ["FalloutNV.esm", "TestMod.esp"] {
            sdom.extend_from_slice(&(name.len() as u16).to_le_bytes());
            sdom.extend_from_slice(name.as_bytes());
        }

        // RVRA list (id=1) + stringmap (id=2) referencing id=1
        let mut list = vec![0xFFu8];
        list.extend_from_slice(&1u32.to_le_bytes());
        list.extend_from_slice(&[1, 1]);
        list.extend_from_slice(&2u32.to_le_bytes());
        list.extend_from_slice(&[0xFF, 0xFF]);
        list.extend_from_slice(&2u32.to_le_bytes());
        list.push(1);
        list.extend_from_slice(&42.5f64.to_le_bytes());
        list.push(2);
        list.extend_from_slice(&0x0010_4C0Fu32.to_le_bytes());

        let mut smap = vec![0x01u8];
        smap.extend_from_slice(&2u32.to_le_bytes());
        smap.extend_from_slice(&[3, 0]);
        smap.extend_from_slice(&1u32.to_le_bytes());
        smap.push(0x01);
        smap.extend_from_slice(&2u32.to_le_bytes());
        smap.extend_from_slice(&4u16.to_le_bytes());
        smap.extend_from_slice(b"kArr");
        smap.push(4);
        smap.extend_from_slice(&1u32.to_le_bytes());
        smap.extend_from_slice(&4u16.to_le_bytes());
        smap.extend_from_slice(b"kStr");
        smap.push(3);
        let long = "A".repeat(270);
        smap.extend_from_slice(&(long.len() as u16).to_le_bytes());
        smap.extend_from_slice(long.as_bytes());

        // JPAV
        let mut jpav = Vec::new();
        jpav.extend_from_slice(&1u16.to_le_bytes());
        jpav.push(0x01);
        jpav.extend_from_slice(&1u16.to_le_bytes());
        jpav.extend_from_slice(&0x0000_0014u32.to_le_bytes());
        jpav.extend_from_slice(&1u16.to_le_bytes());
        jpav.push(7);
        jpav.extend_from_slice(b"privVar");
        jpav.extend_from_slice(&2u16.to_le_bytes());
        jpav.push(1);
        jpav.extend_from_slice(&(-9.5f64).to_le_bytes());
        jpav.push(4);
        jpav.extend_from_slice(&3u16.to_le_bytes());
        jpav.extend_from_slice(b"abc");

        // JPRM
        let mut jprm = Vec::new();
        jprm.extend_from_slice(&1u16.to_le_bytes());
        jprm.push(0xFF);
        jprm.extend_from_slice(&1u16.to_le_bytes());
        jprm.push(5);
        jprm.extend_from_slice(b"_rmap");
        jprm.extend_from_slice(&1u16.to_le_bytes());
        jprm.extend_from_slice(&0x0000_0007u32.to_le_bytes());
        jprm.push(2);
        jprm.extend_from_slice(&0x000E_3778u32.to_le_bytes());

        // JPLR
        let mut jplr = Vec::new();
        jplr.extend_from_slice(&1u16.to_le_bytes());
        jplr.extend_from_slice(&0x0010_4C0Fu32.to_le_bytes());
        jplr.extend_from_slice(&0x0010_62ACu32.to_le_bytes());
        jplr.push(0xFF);

        // JPSV
        let mut jpsv = Vec::new();
        jpsv.extend_from_slice(&1u16.to_le_bytes());
        jpsv.extend_from_slice(&0x0008_0664u32.to_le_bytes());
        jpsv.extend_from_slice(&2u16.to_le_bytes());
        jpsv.push(0xFF);
        jpsv.push(3);
        jpsv.extend_from_slice(b"vFl");
        jpsv.extend_from_slice(&12.5f64.to_le_bytes());
        jpsv.push(0xFF);
        jpsv.push(3);
        jpsv.extend_from_slice(b"vRf");
        jpsv.extend_from_slice(&0x0010_62ACu32.to_le_bytes());
        jpsv.extend_from_slice(&0u32.to_le_bytes());

        // SMGJ
        let mut smgj = Vec::new();
        smgj.extend_from_slice(&1u16.to_le_bytes());
        smgj.extend_from_slice(&6u16.to_le_bytes());
        smgj.extend_from_slice(b"MyStat");
        smgj.extend_from_slice(&(-5i32).to_le_bytes());

        // OSMS
        let mut osms = Vec::new();
        osms.extend_from_slice(&1u16.to_le_bytes());
        osms.push(0xFF);
        osms.extend_from_slice(&1u16.to_le_bytes());
        osms.push(5);
        osms.extend_from_slice(b"_smap");
        osms.extend_from_slice(&1u16.to_le_bytes());
        osms.push(2);
        osms.extend_from_slice(b"k1");
        osms.push(1);
        osms.extend_from_slice(&7.25f64.to_le_bytes());

        // SOAT
        let mut soat = Vec::new();
        soat.extend_from_slice(&1u16.to_le_bytes());
        soat.push(0xFF);
        soat.extend_from_slice(&1u16.to_le_bytes());
        soat.extend_from_slice(&0x14u32.to_le_bytes());
        soat.extend_from_slice(&1u16.to_le_bytes());
        soat.push(4);
        soat.extend_from_slice(b"_tmr");
        soat.extend_from_slice(&60.0f64.to_le_bytes());
        soat.extend_from_slice(&60.0f64.to_le_bytes());
        soat.extend_from_slice(&4u32.to_le_bytes());

        let raw = container(&[
            plugin(
                PLUGIN_NVSE,
                &[
                    chunk(b"SDOM", 0, &sdom),
                    chunk(b"SVRA", 2, &[]),
                    chunk(b"RVRA", 2, &list),
                    chunk(b"RVRA", 2, &smap),
                    chunk(b"EVRA", 2, &[]),
                    chunk(b"SVTS", 0, &[]),
                    chunk(b"EVTS", 0, &[]),
                ],
            ),
            plugin(
                PLUGIN_JIP,
                &[
                    chunk(b"JPAV", 10, &jpav),
                    chunk(b"JPRM", 10, &jprm),
                    chunk(b"JPLR", 9, &jplr),
                    chunk(b"JPSV", 9, &jpsv),
                    chunk(b"JPGF", 1, &[0, 0, 0, 0, 0, 0, 0x80, 0x3F]),
                ],
            ),
            plugin(PLUGIN_JOHNNYGUITAR, &[chunk(b"SMGJ", 1, &smgj)]),
            plugin(PLUGIN_SHOWOFF, &[chunk(b"OSMS", 10, &osms), chunk(b"SOAT", 1, &soat)]),
            plugin(0x0000_8888, &[chunk(b"TEST", 1, &[1, 2, 3])]),
        ]);

        let parsed = NvseCosave::parse(&raw).unwrap();
        assert_eq!(parsed.header.format_version, 1);
        assert_eq!(parsed.mod_list().unwrap(), Some(vec!["FalloutNV.esm".into(), "TestMod.esp".into()]));

        let arrs: Vec<NvseArray> = parsed.arrays().into_iter().map(Result::unwrap).collect();
        assert_eq!(arrs.len(), 2);
        assert_eq!(arrs[0].kind, NvseArrayKind::List);
        assert_eq!(arrs[1].kind, NvseArrayKind::StringMap);
        assert_eq!(arrs[1].entries[1].1, NvseArrayVal::Str(long));

        assert_eq!(parsed.jip_aux_vars().unwrap().len(), 1);
        assert_eq!(parsed.jip_refmaps().unwrap().len(), 1);
        assert_eq!(parsed.jip_linked_refs().unwrap().len(), 1);
        let jsv = parsed.jip_added_script_vars().unwrap();
        assert_eq!(jsv.len(), 2);
        assert_eq!(jsv[0].value, JipAddedVarVal::Num(12.5));
        assert_eq!(jsv[1].value, JipAddedVarVal::Form(0x0010_62AC));
        assert_eq!(parsed.jg_extra_misc_stats().unwrap(), vec![JgExtraMiscStat { name: "MyStat".into(), value: -5 }]);
        assert_eq!(parsed.showoff_strmaps().unwrap().len(), 1);
        assert_eq!(parsed.showoff_aux_timers().unwrap()[0].flags, 4);

        let unk = parsed.unknown_chunks();
        assert_eq!(unk.len(), 2);
        assert_eq!(unk[0].tag, *b"JPGF");
        assert_eq!(unk[1].opcode, 0x0000_8888);
    }

    #[test]
    fn rejects_bad_magic_version_truncation_and_absurd_counts() {
        assert!(NvseCosave::parse(b"NOPE").is_err());
        let mut bad_ver = MAGIC.to_vec();
        bad_ver.extend_from_slice(&99u32.to_le_bytes());
        bad_ver.extend_from_slice(&[0u8; 12]);
        assert!(NvseCosave::parse(&bad_ver).is_err());

        let mut huge_count = MAGIC.to_vec();
        huge_count.extend_from_slice(&1u32.to_le_bytes());
        huge_count.extend_from_slice(&0u32.to_le_bytes());
        huge_count.extend_from_slice(&0u32.to_le_bytes());
        huge_count.extend_from_slice(&u32::MAX.to_le_bytes());
        assert!(NvseCosave::parse(&huge_count).is_err());
    }
}
