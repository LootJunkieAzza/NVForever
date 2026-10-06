//! Shared helpers:
//! - Windows-1252 decoding/encoding (the game's 8-bit text encoding), via `encoding_rs`
//! - SHA-256 for corpus fingerprints, via `sha2`
//! - `Cursor`, a bounds-checked little-endian byte reader

pub mod cp1252 {
    use encoding_rs::WINDOWS_1252;

    /// Decode. Undefined slots (0x81, 0x8D, 0x8F, 0x90, 0x9D) map to the C1 control
    /// of the same value (WHATWG "windows-1252"), so decoding is lossless.
    pub fn decode(bytes: &[u8]) -> String {
        WINDOWS_1252.decode_without_bom_handling(bytes).0.into_owned()
    }

    /// Encode; returns None if a character has no Windows-1252 representation.
    pub fn encode(s: &str) -> Option<Vec<u8>> {
        let (out, _, unmappable) = WINDOWS_1252.encode(s);
        (!unmappable).then(|| out.into_owned())
    }

    /// Decode a zero-terminated string: stops at the first NUL.
    pub fn decode_z(bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        decode(&bytes[..end])
    }
}

pub mod sha256 {
    use sha2::{Digest, Sha256};

    pub fn digest(data: &[u8]) -> [u8; 32] {
        Sha256::digest(data).into()
    }

    pub fn hex(data: &[u8]) -> String {
        to_hex(&digest(data))
    }

    pub fn to_hex(b: &[u8]) -> String {
        hex::encode(b)
    }
}

/// Bounds-checked little-endian reader over a byte slice.
#[derive(Clone)]
pub struct Cursor<'a> {
    pub data: &'a [u8],
    pub pos: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutOfBounds {
    pub pos: usize,
    pub wanted: usize,
    pub len: usize,
}

impl std::fmt::Display for OutOfBounds {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "read of {} bytes at offset {} exceeds length {}", self.wanted, self.pos, self.len)
    }
}

impl std::error::Error for OutOfBounds {}

macro_rules! rd {
    ($name:ident, $t:ty, $n:expr) => {
        pub fn $name(&mut self) -> Result<$t, OutOfBounds> {
            let b = self.bytes($n)?;
            Ok(<$t>::from_le_bytes(b.try_into().unwrap()))
        }
    };
}

impl<'a> Cursor<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Cursor { data, pos: 0 }
    }
    pub fn remaining(&self) -> usize {
        self.data.len().saturating_sub(self.pos)
    }
    pub fn is_empty(&self) -> bool {
        self.remaining() == 0
    }
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], OutOfBounds> {
        if self.pos.checked_add(n).is_none_or(|e| e > self.data.len()) {
            return Err(OutOfBounds { pos: self.pos, wanted: n, len: self.data.len() });
        }
        let s = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(s)
    }
    pub fn tag(&mut self) -> Result<[u8; 4], OutOfBounds> {
        Ok(self.bytes(4)?.try_into().unwrap())
    }
    rd!(u8, u8, 1);
    rd!(i8, i8, 1);
    rd!(u16, u16, 2);
    rd!(i16, i16, 2);
    rd!(u32, u32, 4);
    rd!(i32, i32, 4);
    rd!(u64, u64, 8);
    rd!(i64, i64, 8);
    rd!(f32, f32, 4);
    rd!(f64, f64, 8);
}

/// Printable form of a 4-byte signature; non-printable bytes are hex-escaped.
pub fn tag_str(t: &[u8; 4]) -> String {
    let mut s = String::new();
    for &b in t {
        if (0x20..0x7F).contains(&b) {
            s.push(b as char);
        } else {
            s.push_str(&format!("\\x{b:02x}"));
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sha256_vectors() {
        // FIPS 180-4 / NIST example vectors.
        assert_eq!(sha256::hex(b""), "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855");
        assert_eq!(sha256::hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
        assert_eq!(
            sha256::hex(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256::hex(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn cp1252_roundtrip() {
        let all: Vec<u8> = (0u8..=255).collect();
        let s = cp1252::decode(&all);
        assert_eq!(cp1252::encode(&s).unwrap(), all);
        assert_eq!(cp1252::decode(&[0x80, 0x92, 0xE9]), "\u{20AC}\u{2019}\u{E9}");
        assert_eq!(cp1252::decode_z(b"abc\0def"), "abc");
    }

    #[test]
    fn cursor_bounds() {
        let mut c = Cursor::new(&[1, 0, 0, 0, 2]);
        assert_eq!(c.u32().unwrap(), 1);
        assert!(c.u32().is_err());
        assert_eq!(c.u8().unwrap(), 2);
    }
}
