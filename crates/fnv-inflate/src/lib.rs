//! Dependency-free zlib (RFC 1950) / DEFLATE (RFC 1951) decoder, plus a minimal
//! stored-block encoder.
//!
//! Written from the public RFCs. The decoder uses a 10-bit primary lookup table
//! with a canonical bit-by-bit fallback for longer codes.
//!
//! The encoder only emits *stored* (uncompressed) deflate blocks. That produces a
//! valid zlib stream that every inflater accepts; it exists so modified compressed
//! records/files can be written back. Unmodified data should be written from the
//! original compressed bytes to stay byte-identical.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InflateError {
    Truncated,
    BadZlibHeader,
    PresetDictionary,
    BadBlockType,
    StoredLengthMismatch,
    BadHuffmanTable(&'static str),
    BadSymbol,
    BadDistance,
    ChecksumMismatch { expected: u32, actual: u32 },
    SizeMismatch { expected: usize, actual: usize },
    OutputLimit,
}

impl fmt::Display for InflateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            InflateError::Truncated => write!(f, "compressed stream is truncated"),
            InflateError::BadZlibHeader => write!(f, "invalid zlib header"),
            InflateError::PresetDictionary => write!(f, "zlib preset dictionaries are not supported"),
            InflateError::BadBlockType => write!(f, "invalid deflate block type"),
            InflateError::StoredLengthMismatch => write!(f, "stored block LEN/NLEN mismatch"),
            InflateError::BadHuffmanTable(w) => write!(f, "invalid Huffman table: {w}"),
            InflateError::BadSymbol => write!(f, "invalid Huffman symbol"),
            InflateError::BadDistance => write!(f, "back-reference distance exceeds output"),
            InflateError::ChecksumMismatch { expected, actual } => {
                write!(f, "adler32 mismatch: stored {expected:08x}, computed {actual:08x}")
            }
            InflateError::SizeMismatch { expected, actual } => {
                write!(f, "decompressed size {actual} != expected {expected}")
            }
            InflateError::OutputLimit => write!(f, "decompressed output exceeds limit"),
        }
    }
}

impl std::error::Error for InflateError {}

/// Adler-32 checksum (RFC 1950 section 8.2).
pub fn adler32(data: &[u8]) -> u32 {
    const MOD: u32 = 65521;
    let (mut a, mut b) = (1u32, 0u32);
    for chunk in data.chunks(5552) {
        for &x in chunk {
            a += x as u32;
            b += a;
        }
        a %= MOD;
        b %= MOD;
    }
    (b << 16) | a
}

struct BitReader<'a> {
    data: &'a [u8],
    pos: usize,
    bitbuf: u64,
    bitcnt: u32,
}

impl<'a> BitReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        BitReader { data, pos: 0, bitbuf: 0, bitcnt: 0 }
    }

    #[inline]
    fn refill(&mut self) {
        while self.bitcnt <= 56 {
            if self.pos < self.data.len() {
                self.bitbuf |= (self.data[self.pos] as u64) << self.bitcnt;
                self.pos += 1;
                self.bitcnt += 8;
            } else {
                break;
            }
        }
    }

    #[inline]
    fn need(&mut self, n: u32) -> Result<(), InflateError> {
        if self.bitcnt < n {
            self.refill();
            if self.bitcnt < n {
                return Err(InflateError::Truncated);
            }
        }
        Ok(())
    }

    #[inline]
    fn bits(&mut self, n: u32) -> Result<u32, InflateError> {
        if n == 0 {
            return Ok(0);
        }
        self.need(n)?;
        let v = (self.bitbuf & ((1u64 << n) - 1)) as u32;
        self.bitbuf >>= n;
        self.bitcnt -= n;
        Ok(v)
    }

    /// Peek up to `n` bits without failing at end of input (missing bits read as 0).
    #[inline]
    fn peek(&mut self, n: u32) -> u32 {
        if self.bitcnt < n {
            self.refill();
        }
        (self.bitbuf & ((1u64 << n) - 1)) as u32
    }

    #[inline]
    fn consume(&mut self, n: u32) -> Result<(), InflateError> {
        if self.bitcnt < n {
            return Err(InflateError::Truncated);
        }
        self.bitbuf >>= n;
        self.bitcnt -= n;
        Ok(())
    }

    fn align_byte(&mut self) {
        let r = self.bitcnt % 8;
        self.bitbuf >>= r;
        self.bitcnt -= r;
    }

    /// Byte position of the next unread byte (after aligning).
    fn byte_pos(&self) -> usize {
        self.pos - (self.bitcnt / 8) as usize
    }

    fn take_bytes(&mut self, n: usize) -> Result<&'a [u8], InflateError> {
        // Only valid when aligned. Return buffered bytes to the stream first.
        debug_assert!(self.bitcnt % 8 == 0);
        let start = self.byte_pos();
        if start + n > self.data.len() {
            return Err(InflateError::Truncated);
        }
        self.pos = start + n;
        self.bitbuf = 0;
        self.bitcnt = 0;
        Ok(&self.data[start..start + n])
    }
}

const PRIMARY_BITS: u32 = 10;
const MAX_BITS: usize = 15;

struct Huffman {
    /// Primary table: entry = (symbol << 4) | length, 0 = not in table.
    table: Vec<u16>,
    count: [u16; MAX_BITS + 1],
    symbols: Vec<u16>,
}

impl Huffman {
    fn new(lengths: &[u8]) -> Result<Huffman, InflateError> {
        let mut count = [0u16; MAX_BITS + 1];
        for &l in lengths {
            count[l as usize] += 1;
        }
        count[0] = 0;
        // Check for over-subscribed set (incomplete sets are allowed by RFC for
        // single-code distance trees; we accept them).
        let mut left: i32 = 1;
        for len in 1..=MAX_BITS {
            left <<= 1;
            left -= count[len] as i32;
            if left < 0 {
                return Err(InflateError::BadHuffmanTable("over-subscribed"));
            }
        }
        let mut offs = [0u16; MAX_BITS + 2];
        for len in 1..=MAX_BITS {
            offs[len + 1] = offs[len] + count[len];
        }
        let mut symbols = vec![0u16; lengths.len()];
        for (sym, &l) in lengths.iter().enumerate() {
            if l != 0 {
                symbols[offs[l as usize] as usize] = sym as u16;
                offs[l as usize] += 1;
            }
        }
        // Build primary table from canonical codes.
        let mut table = vec![0u16; 1 << PRIMARY_BITS];
        let mut code: u32 = 0;
        let mut next_code = [0u32; MAX_BITS + 2];
        for len in 1..=MAX_BITS {
            code = (code + count[len - 1] as u32) << 1;
            next_code[len] = code;
        }
        for (sym, &l) in lengths.iter().enumerate() {
            let l = l as u32;
            if l == 0 || l > PRIMARY_BITS {
                if l != 0 {
                    next_code[l as usize] += 1;
                }
                continue;
            }
            let c = next_code[l as usize];
            next_code[l as usize] += 1;
            let rev = reverse_bits(c, l);
            let step = 1u32 << l;
            let mut i = rev;
            while i < (1 << PRIMARY_BITS) {
                table[i as usize] = ((sym as u16) << 4) | l as u16;
                i += step;
            }
        }
        Ok(Huffman { table, count, symbols })
    }

    #[inline]
    fn decode(&self, br: &mut BitReader) -> Result<u16, InflateError> {
        let peek = br.peek(PRIMARY_BITS);
        let e = self.table[peek as usize];
        if e != 0 {
            br.consume((e & 0xF) as u32)?;
            return Ok(e >> 4);
        }
        // Slow path: canonical decode one bit at a time.
        let mut code: i32 = 0;
        let mut first: i32 = 0;
        let mut index: i32 = 0;
        for len in 1..=MAX_BITS {
            code |= br.bits(1)? as i32;
            let count = self.count[len] as i32;
            if code - first < count {
                return Ok(self.symbols[(index + code - first) as usize]);
            }
            index += count;
            first += count;
            first <<= 1;
            code <<= 1;
        }
        Err(InflateError::BadSymbol)
    }
}

#[inline]
fn reverse_bits(mut v: u32, n: u32) -> u32 {
    let mut r = 0;
    for _ in 0..n {
        r = (r << 1) | (v & 1);
        v >>= 1;
    }
    r
}

const LEN_BASE: [u16; 29] = [
    3, 4, 5, 6, 7, 8, 9, 10, 11, 13, 15, 17, 19, 23, 27, 31, 35, 43, 51, 59, 67, 83, 99, 115, 131,
    163, 195, 227, 258,
];
const LEN_EXTRA: [u8; 29] =
    [0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 1, 1, 2, 2, 2, 2, 3, 3, 3, 3, 4, 4, 4, 4, 5, 5, 5, 5, 0];
const DIST_BASE: [u16; 30] = [
    1, 2, 3, 4, 5, 7, 9, 13, 17, 25, 33, 49, 65, 97, 129, 193, 257, 385, 513, 769, 1025, 1537,
    2049, 3073, 4097, 6145, 8193, 12289, 16385, 24577,
];
const DIST_EXTRA: [u8; 30] =
    [0, 0, 0, 0, 1, 1, 2, 2, 3, 3, 4, 4, 5, 5, 6, 6, 7, 7, 8, 8, 9, 9, 10, 10, 11, 11, 12, 12, 13, 13];

fn fixed_tables() -> (Huffman, Huffman) {
    let mut l = [0u8; 288];
    l[..144].fill(8);
    l[144..256].fill(9);
    l[256..280].fill(7);
    l[280..].fill(8);
    let d = [5u8; 30];
    (Huffman::new(&l).unwrap(), Huffman::new(&d).unwrap())
}

fn dynamic_tables(br: &mut BitReader) -> Result<(Huffman, Huffman), InflateError> {
    const ORDER: [usize; 19] = [16, 17, 18, 0, 8, 7, 9, 6, 10, 5, 11, 4, 12, 3, 13, 2, 14, 1, 15];
    let hlit = br.bits(5)? as usize + 257;
    let hdist = br.bits(5)? as usize + 1;
    let hclen = br.bits(4)? as usize + 4;
    if hlit > 286 || hdist > 30 {
        return Err(InflateError::BadHuffmanTable("too many codes"));
    }
    let mut cl = [0u8; 19];
    for &o in ORDER.iter().take(hclen) {
        cl[o] = br.bits(3)? as u8;
    }
    let clh = Huffman::new(&cl)?;
    let mut lens = vec![0u8; hlit + hdist];
    let mut i = 0;
    while i < hlit + hdist {
        let sym = clh.decode(br)?;
        match sym {
            0..=15 => {
                lens[i] = sym as u8;
                i += 1;
            }
            16 => {
                if i == 0 {
                    return Err(InflateError::BadHuffmanTable("repeat with no previous length"));
                }
                let prev = lens[i - 1];
                let n = 3 + br.bits(2)? as usize;
                if i + n > lens.len() {
                    return Err(InflateError::BadHuffmanTable("repeat overflows"));
                }
                lens[i..i + n].fill(prev);
                i += n;
            }
            17 | 18 => {
                let n = if sym == 17 { 3 + br.bits(3)? as usize } else { 11 + br.bits(7)? as usize };
                if i + n > lens.len() {
                    return Err(InflateError::BadHuffmanTable("zero-run overflows"));
                }
                i += n; // already zero
            }
            _ => return Err(InflateError::BadSymbol),
        }
    }
    if lens[256] == 0 {
        return Err(InflateError::BadHuffmanTable("no end-of-block code"));
    }
    Ok((Huffman::new(&lens[..hlit])?, Huffman::new(&lens[hlit..])?))
}

/// Inflate a raw DEFLATE stream. Returns (output, bytes_consumed).
pub fn inflate_raw(input: &[u8], size_hint: usize, limit: usize) -> Result<(Vec<u8>, usize), InflateError> {
    // Never trust a declared size for preallocation: cap it by what the input could
    // plausibly expand to (DEFLATE's maximum ratio is ~1032:1; 64x covers typical data,
    // the Vec grows beyond that if needed).
    let cap = size_hint.min(limit).min(input.len().saturating_mul(64).max(4096));
    let mut out: Vec<u8> = Vec::with_capacity(cap);
    let mut br = BitReader::new(input);
    let mut fixed: Option<(Huffman, Huffman)> = None;
    loop {
        let last = br.bits(1)?;
        let btype = br.bits(2)?;
        match btype {
            0 => {
                br.align_byte();
                let hdr = br.take_bytes(4)?;
                let len = u16::from_le_bytes([hdr[0], hdr[1]]);
                let nlen = u16::from_le_bytes([hdr[2], hdr[3]]);
                if len != !nlen {
                    return Err(InflateError::StoredLengthMismatch);
                }
                let bytes = br.take_bytes(len as usize)?;
                if out.len() + bytes.len() > limit {
                    return Err(InflateError::OutputLimit);
                }
                out.extend_from_slice(bytes);
            }
            1 | 2 => {
                let dynamic;
                let (lit, dist) = if btype == 1 {
                    if fixed.is_none() {
                        fixed = Some(fixed_tables());
                    }
                    let f = fixed.as_ref().unwrap();
                    (&f.0, &f.1)
                } else {
                    dynamic = dynamic_tables(&mut br)?;
                    (&dynamic.0, &dynamic.1)
                };
                loop {
                    let sym = lit.decode(&mut br)?;
                    if sym < 256 {
                        if out.len() >= limit {
                            return Err(InflateError::OutputLimit);
                        }
                        out.push(sym as u8);
                    } else if sym == 256 {
                        break;
                    } else {
                        let li = (sym - 257) as usize;
                        if li >= 29 {
                            return Err(InflateError::BadSymbol);
                        }
                        let len = LEN_BASE[li] as usize + br.bits(LEN_EXTRA[li] as u32)? as usize;
                        let di = dist.decode(&mut br)? as usize;
                        if di >= 30 {
                            return Err(InflateError::BadSymbol);
                        }
                        let d = DIST_BASE[di] as usize + br.bits(DIST_EXTRA[di] as u32)? as usize;
                        if d > out.len() {
                            return Err(InflateError::BadDistance);
                        }
                        if out.len() + len > limit {
                            return Err(InflateError::OutputLimit);
                        }
                        let start = out.len() - d;
                        if d >= len {
                            out.extend_from_within(start..start + len);
                        } else {
                            for k in 0..len {
                                let b = out[start + k];
                                out.push(b);
                            }
                        }
                    }
                }
            }
            _ => return Err(InflateError::BadBlockType),
        }
        if last == 1 {
            break;
        }
    }
    br.align_byte();
    Ok((out, br.byte_pos()))
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
    if cmf & 0x0F != 8 || (cmf >> 4) > 7 || ((cmf as u16) << 8 | flg as u16) % 31 != 0 {
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
    fn detects_truncation() {
        let z = zlib_store(b"abcdef");
        assert!(zlib_decompress(&z[..z.len() - 6], None).is_err());
    }
}
