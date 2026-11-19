//! Small zip archives written for the tests, by hand after APPNOTE.TXT (the records `zip.rs`
//! reads): no file of NVIDIA's is ever used.

use std::io::Write;

use flate2::Crc;
use flate2::write::DeflateEncoder;

/// One entry to write.
pub(super) struct TestEntry<'a> {
    pub(super) name: &'a str,
    pub(super) data: &'a [u8],
    pub(super) deflate: bool,
    /// An extra field length the local header claims without the bytes being there (a damaged
    /// or hostile archive); 0 for none.
    pub(super) claimed_extra: u16,
}

impl<'a> TestEntry<'a> {
    pub(super) fn stored(name: &'a str, data: &'a [u8]) -> Self {
        Self { name, data, deflate: false, claimed_extra: 0 }
    }

    pub(super) fn deflated(name: &'a str, data: &'a [u8]) -> Self {
        Self { name, data, deflate: true, claimed_extra: 0 }
    }

    pub(super) fn claiming_extra(self, claimed_extra: u16) -> Self {
        Self { claimed_extra, ..self }
    }
}

/// How to write the archive.
#[derive(Default)]
pub(super) struct Options {
    /// The archive comment after the end record.
    pub(super) comment: Vec<u8>,
    /// ZIP64 end records, and every central entry's sizes and offset in a ZIP64 field.
    pub(super) zip64: bool,
    /// Sizes and CRC zero in the local headers, in a data descriptor after each entry's data.
    pub(super) descriptor: bool,
}

/// `data` deflated (raw deflate, as zip's method 8).
pub(super) fn deflate(data: &[u8]) -> Vec<u8> {
    let mut encoder = DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

pub(super) fn crc32(data: &[u8]) -> u32 {
    let mut crc = Crc::new();
    crc.update(data);
    crc.sum()
}

fn u16le(out: &mut Vec<u8>, v: u16) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u32le(out: &mut Vec<u8>, v: u32) {
    out.extend_from_slice(&v.to_le_bytes());
}

fn u64le(out: &mut Vec<u8>, v: u64) {
    out.extend_from_slice(&v.to_le_bytes());
}

/// The archive's bytes.
pub(super) fn build(entries: &[TestEntry<'_>], options: &Options) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    let flags: u16 = if options.descriptor { 1 << 3 } else { 0 };
    for entry in entries {
        let packed = if entry.deflate { deflate(entry.data) } else { entry.data.to_vec() };
        let method: u16 = if entry.deflate { 8 } else { 0 };
        let crc = crc32(entry.data);
        let offset = out.len() as u64;
        let (local_crc, local_packed, local_size) = if options.descriptor { (0, 0, 0) } else { (crc, packed.len() as u32, entry.data.len() as u32) };
        u32le(&mut out, 0x0403_4b50);
        u16le(&mut out, 20);
        u16le(&mut out, flags);
        u16le(&mut out, method);
        u16le(&mut out, 0);
        u16le(&mut out, 0);
        u32le(&mut out, local_crc);
        u32le(&mut out, local_packed);
        u32le(&mut out, local_size);
        u16le(&mut out, entry.name.len() as u16);
        u16le(&mut out, entry.claimed_extra);
        out.extend_from_slice(entry.name.as_bytes());
        out.extend_from_slice(&packed);
        if options.descriptor {
            u32le(&mut out, 0x0807_4b50);
            u32le(&mut out, crc);
            u32le(&mut out, packed.len() as u32);
            u32le(&mut out, entry.data.len() as u32);
        }
        let mut extra = Vec::new();
        let (c_packed, c_size, c_offset) = if options.zip64 {
            u16le(&mut extra, 0x0001);
            u16le(&mut extra, 24);
            u64le(&mut extra, entry.data.len() as u64);
            u64le(&mut extra, packed.len() as u64);
            u64le(&mut extra, offset);
            (u32::MAX, u32::MAX, u32::MAX)
        } else {
            (packed.len() as u32, entry.data.len() as u32, offset as u32)
        };
        u32le(&mut central, 0x0201_4b50);
        u16le(&mut central, 45);
        u16le(&mut central, 45);
        u16le(&mut central, flags);
        u16le(&mut central, method);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u32le(&mut central, crc);
        u32le(&mut central, c_packed);
        u32le(&mut central, c_size);
        u16le(&mut central, entry.name.len() as u16);
        u16le(&mut central, extra.len() as u16);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u16le(&mut central, 0);
        u32le(&mut central, 0);
        u32le(&mut central, c_offset);
        central.extend_from_slice(entry.name.as_bytes());
        central.extend_from_slice(&extra);
    }
    let directory_at = out.len() as u64;
    let directory_len = central.len() as u64;
    out.extend_from_slice(&central);
    let count = entries.len() as u64;
    if options.zip64 {
        let record_at = out.len() as u64;
        u32le(&mut out, 0x0606_4b50);
        u64le(&mut out, 44);
        u16le(&mut out, 45);
        u16le(&mut out, 45);
        u32le(&mut out, 0);
        u32le(&mut out, 0);
        u64le(&mut out, count);
        u64le(&mut out, count);
        u64le(&mut out, directory_len);
        u64le(&mut out, directory_at);
        u32le(&mut out, 0x0706_4b50);
        u32le(&mut out, 0);
        u64le(&mut out, record_at);
        u32le(&mut out, 1);
    }
    u32le(&mut out, 0x0605_4b50);
    u16le(&mut out, 0);
    u16le(&mut out, 0);
    let (n, len, at) = if options.zip64 { (0xFFFF, u32::MAX, u32::MAX) } else { (count as u16, directory_len as u32, directory_at as u32) };
    u16le(&mut out, n);
    u16le(&mut out, n);
    u32le(&mut out, len);
    u32le(&mut out, at);
    u16le(&mut out, options.comment.len() as u16);
    out.extend_from_slice(&options.comment);
    out
}
