//! zlib (RFC 1950) decoding with NVForever's trailer diagnostics, plus a minimal
//! stored-block encoder.
//!
//! The DEFLATE body is inflated by `flate2` in raw mode with the zlib-rs backend
//! (miniz_oxide, in streaming mode, does not reject back-references that reach before
//! the start of the output). The zlib header and the Adler-32 trailer are checked
//! here, because shipped game data contains streams with a missing or wrong trailer
//! that must be reported rather than rejected (see [`Trailer`]).
//!
//! The encoder only emits *stored* (uncompressed) deflate blocks. That produces a
//! valid zlib stream that every inflater accepts; it exists so modified compressed
//! records/files can be written back. Unmodified data should be written from the
//! original compressed bytes to stay byte-identical.

use flate2::{Decompress, FlushDecompress, Status};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InflateError {
    #[error("compressed stream is truncated")]
    Truncated,
    #[error("invalid zlib header")]
    BadZlibHeader,
    #[error("zlib preset dictionaries are not supported")]
    PresetDictionary,
    #[error("invalid deflate data: {0}")]
    Corrupt(String),
    #[error("adler32 mismatch: stored {expected:08x}, computed {actual:08x}")]
    ChecksumMismatch { expected: u32, actual: u32 },
    #[error("decompressed size {actual} != expected {expected}")]
    SizeMismatch { expected: usize, actual: usize },
    #[error("decompressed output exceeds limit")]
    OutputLimit,
}

/// Adler-32 checksum (RFC 1950 section 8.2).
pub fn adler32(data: &[u8]) -> u32 {
    adler2::adler32_slice(data)
}

fn corrupt(e: flate2::DecompressError) -> InflateError {
    // zlib-rs names the specific fault ("invalid distance too far back", ...) only
    // when it is hit at the start of a call; mid-call it reports its internal state.
    let msg = match e.message() {
        Some(m) if !m.contains("bad state") => m.to_string(),
        _ => "corrupt deflate stream".to_string(),
    };
    InflateError::Corrupt(msg)
}

/// Inflate a raw DEFLATE stream. Returns (output, bytes_consumed); bytes after the
/// final block (e.g. a zlib trailer) are not consumed.
pub fn inflate_raw(input: &[u8], size_hint: usize, limit: usize) -> Result<(Vec<u8>, usize), InflateError> {
    // Never trust a declared size for preallocation: cap it by what the input could
    // plausibly expand to (DEFLATE's maximum ratio is ~1032:1; 64x covers typical data,
    // the Vec grows beyond that if needed).
    let cap = size_hint.min(limit).min(input.len().saturating_mul(64).max(4096));
    // One byte of headroom past `limit` distinguishes "exactly limit" from "more".
    let hard = limit.saturating_add(1);
    let mut out: Vec<u8> = Vec::with_capacity(cap.min(hard));
    let mut d = Decompress::new(false);
    loop {
        if out.len() == out.capacity() {
            let grow = out.capacity().max(4096).min(hard - out.len());
            out.reserve_exact(grow);
        }
        let (in_before, out_before) = (d.total_in(), d.total_out());
        let status = d
            .decompress_vec(&input[d.total_in() as usize..], &mut out, FlushDecompress::None)
            .map_err(corrupt)?;
        if out.len() > limit {
            return Err(InflateError::OutputLimit);
        }
        match status {
            Status::StreamEnd => break,
            Status::Ok | Status::BufError => {
                let stalled = d.total_in() == in_before && d.total_out() == out_before;
                if stalled && out.len() < out.capacity() {
                    return Err(InflateError::Truncated);
                }
            }
        }
    }
    Ok((out, d.total_in() as usize))
}

/// What was found after the deflate data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trailer {
    /// Adler-32 present and correct.
    Verified,
    /// Stream ended right after the final deflate block with no Adler-32. Observed
    /// in at least one shipped archive entry, so readers must accept it.
    Missing,
    /// Adler-32 present but wrong, while the deflate stream itself ended cleanly
    /// and produced exactly the expected size. Observed once in FalloutNV.esm.
    Mismatch { stored: u32, computed: u32 },
}

/// Decompress a zlib stream, verifying the Adler-32 trailer.
/// If `expected_len` is given, the output size must match it exactly.
pub fn zlib_decompress(input: &[u8], expected_len: Option<usize>) -> Result<Vec<u8>, InflateError> {
    match zlib_decompress_ex(input, expected_len)? {
        (out, Trailer::Verified) => Ok(out),
        (_, Trailer::Missing) => Err(InflateError::Truncated),
        (_, Trailer::Mismatch { stored, computed }) => {
            Err(InflateError::ChecksumMismatch { expected: stored, actual: computed })
        }
    }
}

/// Like [`zlib_decompress`], but trailer problems are reported in the returned
/// [`Trailer`] instead of failing: an absent Adler-32 (`Missing`) or a wrong one
/// (`Mismatch`) when the deflate stream itself is complete. A wrong checksum is
/// only tolerated when `expected_len` is given and matches, so a damaged stream
/// is still very unlikely to pass silently. Partial trailers and size mismatches
/// are errors.
pub fn zlib_decompress_ex(input: &[u8], expected_len: Option<usize>) -> Result<(Vec<u8>, Trailer), InflateError> {
    if input.len() < 3 {
        return Err(InflateError::Truncated);
    }
    let cmf = input[0];
    let flg = input[1];
    if cmf & 0x0F != 8 || (cmf >> 4) > 7 || !((cmf as u16) << 8 | flg as u16).is_multiple_of(31) {
        return Err(InflateError::BadZlibHeader);
    }
    if flg & 0x20 != 0 {
        return Err(InflateError::PresetDictionary);
    }
    let limit = expected_len.unwrap_or(1 << 31);
    let (out, used) = inflate_raw(&input[2..], expected_len.unwrap_or(input.len() * 4), limit)?;
    if let Some(e) = expected_len {
        if e != out.len() {
            return Err(InflateError::SizeMismatch { expected: e, actual: out.len() });
        }
    }
    let tpos = 2 + used;
    if tpos == input.len() {
        return Ok((out, Trailer::Missing));
    }
    if tpos + 4 > input.len() {
        return Err(InflateError::Truncated);
    }
    let stored = u32::from_be_bytes([input[tpos], input[tpos + 1], input[tpos + 2], input[tpos + 3]]);
    let actual = adler32(&out);
    if stored != actual {
        if expected_len.is_some() {
            return Ok((out, Trailer::Mismatch { stored, computed: actual }));
        }
        return Err(InflateError::ChecksumMismatch { expected: stored, actual });
    }
    Ok((out, Trailer::Verified))
}

/// Encode `data` as a zlib stream using stored (uncompressed) deflate blocks.
pub fn zlib_store(data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + data.len() / 65535 * 5 + 11);
    out.push(0x78);
    out.push(0x01); // 0x7801 % 31 == 0, fastest-compression level hint
    if data.is_empty() {
        out.extend_from_slice(&[1, 0, 0, 0xFF, 0xFF]);
    } else {
        let mut chunks = data.chunks(65535).peekable();
        while let Some(c) = chunks.next() {
            out.push(if chunks.peek().is_none() { 1 } else { 0 });
            let len = c.len() as u16;
            out.extend_from_slice(&len.to_le_bytes());
            out.extend_from_slice(&(!len).to_le_bytes());
            out.extend_from_slice(c);
        }
    }
    out.extend_from_slice(&adler32(data).to_be_bytes());
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adler_known() {
        // RFC 1950 example value for "Wikipedia" is widely published as 0x11E60398.
        assert_eq!(adler32(b"Wikipedia"), 0x11E6_0398);
        assert_eq!(adler32(b""), 1);
    }

    #[test]
    fn store_roundtrip() {
        for n in [0usize, 1, 100, 65535, 65536, 200_000] {
            let data: Vec<u8> = (0..n).map(|i| (i * 31 % 251) as u8).collect();
            let z = zlib_store(&data);
            assert_eq!(zlib_decompress(&z, Some(n)).unwrap(), data);
        }
    }

    #[test]
    fn fixed_huffman_stream() {
        // zlib.compress(b"hello hello hello hello", 9) from CPython's zlib: a fixed
        // Huffman block with an overlapping back-reference.
        let z: [u8; 16] = [
            0x78, 0xda, 0xcb, 0x48, 0xcd, 0xc9, 0xc9, 0x57, 0xc8, 0x40, 0x27, 0x01, 0x68, 0x03,
            0x08, 0xb1,
        ];
        assert_eq!(zlib_decompress(&z, Some(23)).unwrap(), b"hello hello hello hello");
    }

    #[test]
    fn huge_declared_size_does_not_preallocate() {
        // A tiny stream claiming a 4 GiB output must fail cleanly, not abort on allocation.
        let z = zlib_store(b"abc");
        assert!(matches!(zlib_decompress_ex(&z, Some(u32::MAX as usize)), Err(InflateError::SizeMismatch { .. })));
    }

    #[test]
    fn rejects_bad_header() {
        assert_eq!(zlib_decompress(&[0x78, 0x00, 0, 0, 0, 0], None), Err(InflateError::BadZlibHeader));
    }

    #[test]
    fn detects_checksum_mismatch() {
        let mut z = zlib_store(b"abc");
        let n = z.len();
        z[n - 1] ^= 1;
        assert!(matches!(zlib_decompress(&z, None), Err(InflateError::ChecksumMismatch { .. })));
    }

    #[test]
    fn missing_trailer_is_reported() {
        // Observed shape: deflate data for "@#$%" followed by no Adler-32.
        let z = [0x78, 0x9c, 0x73, 0x50, 0x56, 0x51, 0x05, 0x00];
        assert_eq!(zlib_decompress_ex(&z, Some(4)).unwrap(), (b"@#$%".to_vec(), Trailer::Missing));
        assert!(zlib_decompress(&z, Some(4)).is_err());
    }

    #[test]
    fn rejects_distance_before_output_start() {
        // Fixed-Huffman block whose first symbol copies from distance 1 of an empty output.
        let z = [0x78, 0x01, 0x03, 0x02, 0x00, 0x00];
        assert!(matches!(zlib_decompress_ex(&z, None), Err(InflateError::Corrupt(_))));
        assert!(matches!(zlib_decompress_ex(&z, Some(3)), Err(InflateError::Corrupt(_))));
    }

    #[test]
    fn detects_truncation() {
        let z = zlib_store(b"abcdef");
        assert!(zlib_decompress(&z[..z.len() - 6], None).is_err());
    }
}
