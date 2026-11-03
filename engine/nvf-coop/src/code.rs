//! Lobby codes: the host's addresses, written for a person to read out.
//!
//! There is no server between the players: a code is the host's own address, its
//! local-network one (for friends in the same house) and its internet one (for the rest),
//! with the port and a check byte against typos. 11 bytes in Crockford's base32 make 18
//! characters, shown as three groups of six (`K7QM2X-9DPL4H-8RTCAB`). Reading forgives
//! case, spaces, dashes and the look-alikes O (0), I and L (1).

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
const BYTES: usize = 11;
const CHARS: usize = 18;

/// What a code says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lobby {
    /// The host's address on its own network.
    pub local: Ipv4Addr,
    /// The host's address on the internet (unspecified when it is not known).
    pub public: Ipv4Addr,
    pub port: u16,
}

impl Lobby {
    /// Where a guest tries the host, in turn: the local address, then the internet one.
    pub fn candidates(&self) -> Vec<SocketAddr> {
        let mut out = vec![SocketAddr::V4(SocketAddrV4::new(self.local, self.port))];
        if !self.public.is_unspecified() && self.public != self.local {
            out.push(SocketAddr::V4(SocketAddrV4::new(self.public, self.port)));
        }
        out
    }

    pub fn encode(&self) -> String {
        let mut b = [0u8; BYTES];
        b[..4].copy_from_slice(&self.local.octets());
        b[4..8].copy_from_slice(&self.public.octets());
        b[8..10].copy_from_slice(&self.port.to_be_bytes());
        b[10] = check(&b[..10]);
        // 88 bits, 5 to a character, the last character's spare bits zero.
        let mut chars = String::with_capacity(CHARS + 2);
        let (mut acc, mut bits) = (0u32, 0);
        for byte in b {
            acc = acc << 8 | byte as u32;
            bits += 8;
            while bits >= 5 {
                bits -= 5;
                chars.push(ALPHABET[(acc >> bits & 31) as usize] as char);
            }
        }
        if bits > 0 {
            chars.push(ALPHABET[(acc << (5 - bits) & 31) as usize] as char);
        }
        chars.insert(12, '-');
        chars.insert(6, '-');
        chars
    }

    /// Read a code as typed; None for one that is mistyped (or not a code).
    pub fn decode(text: &str) -> Option<Lobby> {
        let mut values = Vec::with_capacity(CHARS);
        for c in text.chars().filter(|c| !matches!(c, '-' | ' ')) {
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            values.push(ALPHABET.iter().position(|&a| a as char == c)? as u32);
        }
        if values.len() != CHARS {
            return None;
        }
        let mut b = Vec::with_capacity(BYTES);
        let (mut acc, mut bits) = (0u32, 0);
        for v in values {
            acc = acc << 5 | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                b.push((acc >> bits & 0xFF) as u8);
            }
        }
        b.truncate(BYTES);
        if b.len() != BYTES || b[10] != check(&b[..10]) {
            return None;
        }
        let ip = |s: &[u8]| Ipv4Addr::new(s[0], s[1], s[2], s[3]);
        let port = u16::from_be_bytes([b[8], b[9]]);
        (port != 0).then(|| Lobby { local: ip(&b[..4]), public: ip(&b[4..8]), port })
    }
}

/// A check byte that changes with any one character (an FNV-1a hash folded to a byte).
fn check(bytes: &[u8]) -> u8 {
    let h = bytes.iter().fold(0x811C_9DC5u32, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193));
    (h ^ h >> 8 ^ h >> 16 ^ h >> 24) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobby() -> Lobby {
        Lobby { local: Ipv4Addr::new(192, 168, 1, 20), public: Ipv4Addr::new(203, 0, 113, 7), port: 47960 }
    }

    #[test]
    fn round_trips_and_reads_forgivingly() {
        let code = lobby().encode();
        assert_eq!(code.len(), CHARS + 2, "{code}");
        assert_eq!(code.matches('-').count(), 2);
        assert_eq!(Lobby::decode(&code), Some(lobby()));
        let typed = code.to_lowercase().replace('-', " ").replace('0', "o").replace('1', "l");
        assert_eq!(Lobby::decode(&typed), Some(lobby()), "{typed}");
    }

    #[test]
    fn a_typo_is_caught() {
        let code = lobby().encode();
        let mut caught = 0;
        for (i, c) in code.char_indices().filter(|(_, c)| *c != '-') {
            let other = if c == 'A' { 'B' } else { 'A' };
            let typo = format!("{}{other}{}", &code[..i], &code[i + 1..]);
            caught += Lobby::decode(&typo).is_none() as usize;
        }
        // The last character's spare bits carry nothing, so a change there can still read.
        assert!(caught >= CHARS - 1, "{caught} of {CHARS} typos caught");
        assert_eq!(Lobby::decode("hello"), None);
        assert_eq!(Lobby::decode(&code[..code.len() - 1]), None);
    }

    #[test]
    fn candidates_try_the_local_address_first() {
        let c = lobby().candidates();
        assert_eq!(c.len(), 2);
        assert_eq!(c[0].ip().to_string(), "192.168.1.20");
        let lan_only = Lobby { public: Ipv4Addr::UNSPECIFIED, ..lobby() };
        assert_eq!(lan_only.candidates().len(), 1);
    }
}
