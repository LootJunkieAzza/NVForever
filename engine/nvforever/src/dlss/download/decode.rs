//! One entry's data unpacked as it arrives: stored bytes or raw deflate
//! (`flate2::read::DeflateDecoder`) into the staging file, its SHA-256, CRC-32 and size taken
//! on the way, never more than the size the directory declares written, and the cancel flag
//! looked at for every 64 KiB.

use std::io::{self, Read, Write};

use flate2::Crc;
use flate2::read::DeflateDecoder;

use super::zip::{DEFLATED, STORED};
use super::{DownloadFile, Failure};

/// The unit the cancel flag is looked at.
pub(super) const CHUNK: usize = 64 * 1024;

/// What the unpacked bytes were.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct Decoded {
    pub(super) sha256: [u8; 32],
    pub(super) crc32: u32,
    pub(super) size: u64,
}

/// Why unpacking stopped.
#[derive(Debug)]
pub(super) enum DecodeError {
    /// Reading the packed bytes, or inflating them, failed (the source may know more).
    Source(io::Error),
    /// The data unpacks to more than the declared size.
    Oversize,
    /// Writing the staging file failed.
    Write(io::Error),
    /// The cancel flag was set.
    Cancelled,
}

/// `packed` (only the entry's packed bytes) unpacked by `method` into `out`, refusing anything
/// past `declared` bytes.
pub(super) fn decode<R: Read, W: Write>(packed: R, method: u16, declared: u64, out: &mut W, cancelled: &dyn Fn() -> bool) -> Result<Decoded, DecodeError> {
    match method {
        STORED => pump(packed, declared, out, cancelled),
        DEFLATED => pump(DeflateDecoder::new(packed), declared, out, cancelled),
        other => Err(DecodeError::Source(io::Error::other(format!("method {other}")))),
    }
}

fn pump<R: Read, W: Write>(mut reader: R, declared: u64, out: &mut W, cancelled: &dyn Fn() -> bool) -> Result<Decoded, DecodeError> {
    let mut buf = vec![0u8; CHUNK];
    let mut hash = fnv_util::sha256::Stream::new();
    let mut crc = Crc::new();
    let mut size = 0u64;
    loop {
        if cancelled() {
            return Err(DecodeError::Cancelled);
        }
        let n = match reader.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(DecodeError::Source(e)),
        };
        size += n as u64;
        if size > declared {
            return Err(DecodeError::Oversize);
        }
        hash.update(&buf[..n]);
        crc.update(&buf[..n]);
        out.write_all(&buf[..n]).map_err(DecodeError::Write)?;
    }
    out.flush().map_err(DecodeError::Write)?;
    Ok(Decoded { sha256: hash.finish(), crc32: crc.sum(), size })
}

/// The unpacked bytes checked against the directory (size, CRC-32) and the file's pin (SHA-256).
pub(super) fn verify(decoded: &Decoded, crc32: u32, file: &DownloadFile) -> Result<(), Failure> {
    if decoded.size != file.size {
        return Err(Failure::NotAZip(format!("{} unpacks to {} bytes, not {}", file.zip_path, decoded.size, file.size)));
    }
    if decoded.crc32 != crc32 {
        return Err(Failure::NotAZip(format!("{}: its CRC-32 {:08x} is not the directory's {crc32:08x}", file.zip_path, decoded.crc32)));
    }
    if !pin_matches(&decoded.sha256, file) {
        return Err(Failure::Mismatch { name: file.name, sha256: fnv_util::sha256::to_hex(&decoded.sha256) });
    }
    Ok(())
}

/// Whether `digest` is the file's pinned SHA-256.
pub(super) fn pin_matches(digest: &[u8; 32], file: &DownloadFile) -> bool {
    super::super::preflight::parse_sha256(file.sha256).as_ref() == Some(digest)
}

#[cfg(test)]
mod tests {
    use super::super::test_zip::{crc32, deflate};
    use super::*;

    fn data() -> Vec<u8> {
        (0..300_000u32).map(|i| ((i * 31) ^ (i >> 5)) as u8).collect()
    }

    fn file(data: &[u8], sha256: &'static str) -> DownloadFile {
        DownloadFile { name: "x.dll", zip_path: "bin/x64/x.dll", size: data.len() as u64, sha256 }
    }

    fn leak_hex(data: &[u8]) -> &'static str {
        Box::leak(fnv_util::sha256::hex(data).into_boxed_str())
    }

    #[test]
    fn stored_and_deflated_entries_round_trip_and_verify() {
        let data = data();
        for (method, packed) in [(STORED, data.clone()), (DEFLATED, deflate(&data))] {
            let mut out = Vec::new();
            let decoded = decode(packed.as_slice(), method, data.len() as u64, &mut out, &|| false).unwrap();
            assert_eq!(out, data, "method {method}");
            assert_eq!(decoded.size, data.len() as u64);
            assert_eq!(decoded.crc32, crc32(&data));
            assert_eq!(decoded.sha256, fnv_util::sha256::digest(&data));
            verify(&decoded, crc32(&data), &file(&data, leak_hex(&data))).unwrap();
        }
    }

    #[test]
    fn a_crc_or_hash_or_size_other_than_expected_is_refused() {
        let data = data();
        let mut out = Vec::new();
        let decoded = decode(deflate(&data).as_slice(), DEFLATED, data.len() as u64, &mut out, &|| false).unwrap();
        let good = file(&data, leak_hex(&data));
        assert!(matches!(verify(&decoded, crc32(&data) ^ 1, &good), Err(Failure::NotAZip(why)) if why.contains("CRC-32")));
        let other = file(&data, leak_hex(b"other bytes"));
        assert_eq!(verify(&decoded, crc32(&data), &other), Err(Failure::Mismatch { name: "x.dll", sha256: fnv_util::sha256::hex(&data) }));
        let longer = DownloadFile { size: data.len() as u64 + 1, ..good };
        assert!(matches!(verify(&decoded, crc32(&data), &longer), Err(Failure::NotAZip(why)) if why.contains("unpacks to")));
    }

    #[test]
    fn output_past_the_declared_size_stops_at_once() {
        // A deflate bomb: 8 MiB of zeros packed into a few KiB, declared as 1000 bytes.
        let zeros = vec![0u8; 8 << 20];
        let packed = deflate(&zeros);
        assert!(packed.len() < 64 * 1024);
        let mut out = Vec::new();
        assert!(matches!(decode(packed.as_slice(), DEFLATED, 1000, &mut out, &|| false), Err(DecodeError::Oversize)));
        assert!(out.len() <= 1000, "nothing past the declared size written ({} bytes)", out.len());
        let mut out = Vec::new();
        assert!(matches!(decode(&b"12345"[..], STORED, 4, &mut out, &|| false), Err(DecodeError::Oversize)));
    }

    #[test]
    fn damaged_deflate_data_and_cancel_stop_the_unpacking() {
        let data = data();
        let mut packed = deflate(&data);
        packed.truncate(packed.len() / 2);
        packed.extend([0xFF; 64]);
        let mut out = Vec::new();
        let result = decode(packed.as_slice(), DEFLATED, data.len() as u64, &mut out, &|| false);
        // Either inflating fails, or it ends short and the check refuses it.
        if let Ok(decoded) = result {
            assert!(verify(&decoded, crc32(&data), &file(&data, leak_hex(&data))).is_err());
        } else {
            assert!(matches!(result, Err(DecodeError::Source(_))));
        }
        let mut out = Vec::new();
        assert!(matches!(decode(data.as_slice(), STORED, data.len() as u64, &mut out, &|| true), Err(DecodeError::Cancelled)));
        assert!(out.is_empty());
        assert!(matches!(decode(data.as_slice(), 12, data.len() as u64, &mut out, &|| false), Err(DecodeError::Source(_))));
    }
}
