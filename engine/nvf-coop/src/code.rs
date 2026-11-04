//! Lobby codes: the host's address, written for a person to read out.
//!
//! There is no server between the players: a code is the host's own address (its internet
//! one when it is known, else its local network's) and a check byte against typos: 5 bytes
//! in Crockford's base32 make 8 characters, two groups of four (`K7QM-2X9D`). A host not on
//! [`DEFAULT_PORT`] adds the port: 7 bytes, 12 characters (`K7QM-2X9D-8RTC`). Friends on
//! the host's own network need nothing more: a guest also asks its network for the code's
//! host ([`Lobby::candidates`]), which answers only to its own code ([`Lobby::key`]).
//! Reading forgives case, spaces, dashes and the look-alikes O (0), I and L (1).

use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
/// The port a code without one means.
pub const DEFAULT_PORT: u16 = 47_960;
/// Characters in a code on the default port, and with its own port.
const SHORT: usize = 8;
const LONG: usize = 12;

/// What a code says.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lobby {
    /// Where the host can be reached: its internet address, or its local network's.
    pub addr: Ipv4Addr,
    pub port: u16,
}

impl Lobby {
    /// Where a guest tries the host, in turn: the code's address, this PC (a host in
    /// another window), and the guest's own network by broadcast (`local` is the guest's
    /// address there; its /24's broadcast address goes first, as Windows sends the
    /// all-ones one out of a single adapter only).
    pub fn candidates(&self, local: Option<Ipv4Addr>) -> Vec<SocketAddr> {
        let mut ips = vec![self.addr, Ipv4Addr::LOCALHOST];
        if let Some(l) = local.filter(|l| l.is_private()) {
            let [a, b, c, _] = l.octets();
            ips.push(Ipv4Addr::new(a, b, c, 255));
        }
        ips.push(Ipv4Addr::BROADCAST);
        let mut out: Vec<SocketAddr> = Vec::new();
        for ip in ips {
            let a = SocketAddr::V4(SocketAddrV4::new(ip, self.port));
            if !out.contains(&a) {
                out.push(a);
            }
        }
        out
    }

    /// The number a guest's Hello carries so that only this code's host answers it (never
    /// 0, which is a guest that came by address, not by code).
    pub fn key(&self) -> u32 {
        let mut b = self.addr.octets().to_vec();
        b.extend_from_slice(&self.port.to_be_bytes());
        fnv(&b).max(1)
    }

    pub fn encode(&self) -> String {
        let mut b = self.addr.octets().to_vec();
        if self.port != DEFAULT_PORT {
            b.extend_from_slice(&self.port.to_be_bytes());
        }
        b.push(check(&b));
        // 5 bits to a character, the last character's spare bits zero.
        let mut chars = String::new();
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
        let groups: Vec<&str> = (0..chars.len()).step_by(4).map(|i| &chars[i..(i + 4).min(chars.len())]).collect();
        groups.join("-")
    }

    /// Read a code as typed; None for one that is mistyped (or not a code).
    pub fn decode(text: &str) -> Option<Lobby> {
        let mut values = Vec::with_capacity(LONG);
        for c in text.chars().filter(|c| !matches!(c, '-' | ' ')) {
            let c = match c.to_ascii_uppercase() {
                'O' => '0',
                'I' | 'L' => '1',
                c => c,
            };
            values.push(ALPHABET.iter().position(|&a| a as char == c)? as u32);
        }
        let bytes = match values.len() {
            SHORT => 5,
            LONG => 7,
            _ => return None,
        };
        let mut b = Vec::with_capacity(bytes);
        let (mut acc, mut bits) = (0u32, 0);
        for v in values {
            acc = acc << 5 | v;
            bits += 5;
            if bits >= 8 {
                bits -= 8;
                b.push((acc >> bits & 0xFF) as u8);
            }
        }
        b.truncate(bytes);
        if b.len() != bytes {
            return None;
        }
        let (body, sum) = b.split_at(bytes - 1);
        if sum[0] != check(body) {
            return None;
        }
        let addr = Ipv4Addr::new(body[0], body[1], body[2], body[3]);
        let port = if bytes == 7 { u16::from_be_bytes([body[4], body[5]]) } else { DEFAULT_PORT };
        (port != 0 && !addr.is_unspecified()).then_some(Lobby { addr, port })
    }
}

fn fnv(bytes: &[u8]) -> u32 {
    bytes.iter().fold(0x811C_9DC5u32, |h, &b| (h ^ b as u32).wrapping_mul(0x0100_0193))
}

/// A check byte that changes with any one character (an FNV-1a hash folded to a byte).
fn check(bytes: &[u8]) -> u8 {
    let h = fnv(bytes);
    (h ^ h >> 8 ^ h >> 16 ^ h >> 24) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lobby() -> Lobby {
        Lobby { addr: Ipv4Addr::new(203, 0, 113, 7), port: DEFAULT_PORT }
    }

    #[test]
    fn round_trips_and_reads_forgivingly() {
        let code = lobby().encode();
        assert_eq!(code.len(), SHORT + 1, "{code}");
        assert_eq!(code.matches('-').count(), 1);
        assert_eq!(Lobby::decode(&code), Some(lobby()));
        let typed = code.to_lowercase().replace('-', " ").replace('0', "o").replace('1', "l");
        assert_eq!(Lobby::decode(&typed), Some(lobby()), "{typed}");
        let other = Lobby { port: 50_000, ..lobby() };
        let long = other.encode();
        assert_eq!(long.len(), LONG + 2, "{long}");
        assert_eq!(Lobby::decode(&long), Some(other));
    }

    #[test]
    fn a_typo_is_caught() {
        for l in [lobby(), Lobby { port: 50_000, ..lobby() }] {
            let code = l.encode();
            let n = code.chars().filter(|c| *c != '-').count();
            let mut caught = 0;
            for (i, c) in code.char_indices().filter(|(_, c)| *c != '-') {
                let other = if c == 'A' { 'B' } else { 'A' };
                let typo = format!("{}{other}{}", &code[..i], &code[i + 1..]);
                caught += (Lobby::decode(&typo) != Some(l)) as usize;
            }
            // The last character's spare bits carry nothing, so a change there can still read.
            assert!(caught >= n - 1, "{caught} of {n} typos caught in {code}");
        }
        assert_eq!(Lobby::decode("hello"), None);
        assert_eq!(Lobby::decode(&lobby().encode()[..7]), None);
    }

    #[test]
    fn candidates_try_the_code_then_this_pc_then_the_network() {
        let c = lobby().candidates(Some(Ipv4Addr::new(192, 168, 1, 20)));
        let ips: Vec<String> = c.iter().map(|a| a.ip().to_string()).collect();
        assert_eq!(ips, ["203.0.113.7", "127.0.0.1", "192.168.1.255", "255.255.255.255"]);
        assert!(c.iter().all(|a| a.port() == DEFAULT_PORT));
        assert_eq!(lobby().candidates(None).len(), 3);
    }

    #[test]
    fn keys_tell_lobbies_apart() {
        let other = Lobby { addr: Ipv4Addr::new(203, 0, 113, 8), ..lobby() };
        assert_ne!(lobby().key(), other.key());
        assert_ne!(lobby().key(), 0);
    }
}
