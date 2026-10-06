//! Shared helpers with no dependencies:
//! - Windows-1252 decoding/encoding (the game's 8-bit text encoding)
//! - SHA-256 (FIPS 180-4) for corpus fingerprints
//! - `Cursor`, a bounds-checked little-endian byte reader

pub mod cp1252 {
    /// Code points for bytes 0x80..=0x9F in Windows-1252. Undefined slots
    /// (0x81, 0x8D, 0x8F, 0x90, 0x9D) map to the C1 control of the same value,
    /// matching the WHATWG "windows-1252" decoder, so decoding is lossless.
    const HIGH: [u16; 32] = [
        0x20AC, 0x0081, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0x008D, 0x017D, 0x008F, 0x0090, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0x009D, 0x017E, 0x0178,
    ];

    pub fn decode(bytes: &[u8]) -> String {
        let mut s = String::with_capacity(bytes.len());
        for &b in bytes {
            let c = match b {
                0x80..=0x9F => char::from_u32(HIGH[(b - 0x80) as usize] as u32).unwrap(),
                _ => b as char,
            };
            s.push(c);
        }
        s
    }

    /// Encode; returns None if a character has no Windows-1252 representation.
    pub fn encode(s: &str) -> Option<Vec<u8>> {
        let mut out = Vec::with_capacity(s.len());
        for c in s.chars() {
            let cp = c as u32;
            if cp < 0x80 || (0xA0..=0xFF).contains(&cp) {
                out.push(cp as u8);
            } else if let Some(i) = HIGH.iter().position(|&h| h as u32 == cp) {
                out.push(0x80 + i as u8);
            } else {
                return None;
            }
        }
        Some(out)
    }

    /// Decode a zero-terminated string: stops at the first NUL.
    pub fn decode_z(bytes: &[u8]) -> String {
        let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        decode(&bytes[..end])
    }
}

pub mod sha256 {
    const K: [u32; 64] = [
        0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4,
        0xab1c5ed5, 0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe,
        0x9bdc06a7, 0xc19bf174, 0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f,
        0x4a7484aa, 0x5cb0a9dc, 0x76f988da, 0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7,
        0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967, 0x27b70a85, 0x2e1b2138, 0x4d2c6dfc,
        0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85, 0xa2bfe8a1, 0xa81a664b,
        0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070, 0x19a4c116,
        0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
        0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7,
        0xc67178f2,
    ];

    #[derive(Clone)]
    pub struct Sha256 {
        h: [u32; 8],
        buf: [u8; 64],
        buflen: usize,
        total: u64,
    }

    impl Default for Sha256 {
        fn default() -> Self {
            Self::new()
        }
    }

    impl Sha256 {
        pub fn new() -> Self {
            Sha256 {
                h: [
                    0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c,
                    0x1f83d9ab, 0x5be0cd19,
                ],
                buf: [0; 64],
                buflen: 0,
                total: 0,
            }
        }

        fn block(&mut self, b: &[u8]) {
            let mut w = [0u32; 64];
            for i in 0..16 {
                w[i] = u32::from_be_bytes([b[4 * i], b[4 * i + 1], b[4 * i + 2], b[4 * i + 3]]);
            }
            for i in 16..64 {
                let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
                let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
                w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
            }
            let [mut a, mut b_, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
            for i in 0..64 {
                let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
                let ch = (e & f) ^ (!e & g);
                let t1 = h.wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
                let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
                let maj = (a & b_) ^ (a & c) ^ (b_ & c);
                let t2 = s0.wrapping_add(maj);
                h = g;
                g = f;
                f = e;
                e = d.wrapping_add(t1);
                d = c;
                c = b_;
                b_ = a;
                a = t1.wrapping_add(t2);
            }
            for (x, y) in self.h.iter_mut().zip([a, b_, c, d, e, f, g, h]) {
                *x = x.wrapping_add(y);
            }
        }

        pub fn update(&mut self, mut data: &[u8]) {
            self.total += data.len() as u64;
            if self.buflen > 0 {
                let take = (64 - self.buflen).min(data.len());
                self.buf[self.buflen..self.buflen + take].copy_from_slice(&data[..take]);
                self.buflen += take;
                data = &data[take..];
                if self.buflen == 64 {
                    let b = self.buf;
                    self.block(&b);
                    self.buflen = 0;
                }
            }
            while data.len() >= 64 {
                self.block(&data[..64]);
                data = &data[64..];
            }
            if !data.is_empty() {
                self.buf[..data.len()].copy_from_slice(data);
                self.buflen = data.len();
            }
        }

        pub fn finish(mut self) -> [u8; 32] {
            let bits = self.total.wrapping_mul(8);
            self.update(&[0x80]);
            while self.buflen != 56 {
                self.update(&[0]);
            }
            self.update(&bits.to_be_bytes());
            let mut out = [0u8; 32];
            for (i, v) in self.h.iter().enumerate() {
                out[4 * i..4 * i + 4].copy_from_slice(&v.to_be_bytes());
            }
            out
        }
    }

    pub fn digest(data: &[u8]) -> [u8; 32] {
        let mut s = Sha256::new();
        s.update(data);
        s.finish()
    }

    pub fn hex(data: &[u8]) -> String {
        to_hex(&digest(data))
    }

    pub fn to_hex(b: &[u8]) -> String {
        const H: &[u8; 16] = b"0123456789abcdef";
        let mut s = String::with_capacity(b.len() * 2);
        for &x in b {
            s.push(H[(x >> 4) as usize] as char);
            s.push(H[(x & 15) as usize] as char);
        }
        s
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
        if self.pos.checked_add(n).map_or(true, |e| e > self.data.len()) {
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
        let mut s = sha256::Sha256::new();
        for _ in 0..1000 {
            s.update(&[b'a'; 1000]);
        }
        assert_eq!(sha256::to_hex(&s.finish()), "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0");
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
