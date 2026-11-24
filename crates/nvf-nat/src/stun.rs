//! STUN Binding (RFC 8489), the part a client needs to learn its public address, plus the
//! server's answer for `natsim`. Format notes: `docs/spec-notes/nat-traversal.md`.
//!
//! No authentication: a Binding to a public STUN server carries none (RFC 8489 section 6), and
//! co-op's probes between players are Noise messages, not STUN.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

/// The magic cookie, bytes 4..8 of every message [documented: RFC 8489 section 5].
pub const MAGIC_COOKIE: u32 = 0x2112_A442;
/// Message types [documented: RFC 8489 section 5; the error value from the class bits].
pub const BINDING_REQUEST: u16 = 0x0001;
pub const BINDING_SUCCESS: u16 = 0x0101;
pub const BINDING_ERROR: u16 = 0x0111;

/// Attribute types [documented: RFC 8489 section 18.3].
const ATTR_MAPPED_ADDRESS: u16 = 0x0001;
const ATTR_ERROR_CODE: u16 = 0x0009;
const ATTR_XOR_MAPPED_ADDRESS: u16 = 0x0020;
const ATTR_SOFTWARE: u16 = 0x8022;
const ATTR_FINGERPRINT: u16 = 0x8028;
/// FINGERPRINT is the CRC-32 XOR this [documented: RFC 8489 section 14.7].
const FINGERPRINT_XOR: u32 = 0x5354_554E;

const HEADER: usize = 20;

/// The 96-bit transaction id. The caller draws it at random (RFC 8489 section 5 asks for a
/// uniformly random one); this crate has no random source of its own.
pub type TxId = [u8; 12];

/// Whether a datagram is a STUN message, so a socket shared with game traffic can tell them
/// apart: the two top bits zero, the cookie in place, and a length that matches the datagram
/// and is a multiple of 4 [documented: RFC 8489 sections 5 and 7].
pub fn is_stun(b: &[u8]) -> bool {
    if b.len() < HEADER || b[0] & 0xc0 != 0 {
        return false;
    }
    let len = u16::from_be_bytes([b[2], b[3]]) as usize;
    let cookie = u32::from_be_bytes([b[4], b[5], b[6], b[7]]);
    cookie == MAGIC_COOKIE && len.is_multiple_of(4) && len == b.len() - HEADER
}

/// A Binding request, with SOFTWARE when given (RFC 8489 section 6.1 asks clients for it) and a
/// FINGERPRINT last.
pub fn binding_request(tx: TxId, software: Option<&str>) -> Vec<u8> {
    let mut m = header(BINDING_REQUEST, tx);
    if let Some(s) = software {
        put_attr(&mut m, ATTR_SOFTWARE, s.as_bytes());
    }
    finish_with_fingerprint(m)
}

/// A Binding success response naming `mapped` in XOR-MAPPED-ADDRESS, with a FINGERPRINT. Used
/// by `natsim`'s servers and by tests.
pub fn binding_success(tx: TxId, mapped: SocketAddr) -> Vec<u8> {
    let mut m = header(BINDING_SUCCESS, tx);
    let value = xor_address(mapped, &tx);
    put_attr(&mut m, ATTR_XOR_MAPPED_ADDRESS, &value);
    finish_with_fingerprint(m)
}

/// What a decoded message says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub kind: u16,
    pub tx: TxId,
    /// XOR-MAPPED-ADDRESS, or MAPPED-ADDRESS from an old server when there is no XOR form.
    pub mapped: Option<SocketAddr>,
    /// ERROR-CODE as class × 100 + number [documented: RFC 8489 section 14.8].
    pub error: Option<u16>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    NotStun,
    Truncated,
    BadFingerprint,
    /// FINGERPRINT not last [documented: RFC 8489 section 14.7].
    AttributeAfterFingerprint,
    BadAddress,
}

/// Decodes a message. A FINGERPRINT, when present, is checked. Unknown attributes are skipped
/// [design: the only thing read from a server is its address].
pub fn decode(b: &[u8]) -> Result<Message, DecodeError> {
    if !is_stun(b) {
        return Err(DecodeError::NotStun);
    }
    let kind = u16::from_be_bytes([b[0], b[1]]);
    let mut tx = [0u8; 12];
    tx.copy_from_slice(&b[8..20]);
    let mut msg = Message { kind, tx, mapped: None, error: None };
    let mut xor_mapped = None;
    let mut plain_mapped = None;
    let mut at = HEADER;
    let mut seen_fingerprint = false;
    while at < b.len() {
        if seen_fingerprint {
            return Err(DecodeError::AttributeAfterFingerprint);
        }
        if at + 4 > b.len() {
            return Err(DecodeError::Truncated);
        }
        let ty = u16::from_be_bytes([b[at], b[at + 1]]);
        let len = u16::from_be_bytes([b[at + 2], b[at + 3]]) as usize;
        let start = at + 4;
        let end = start + len;
        if end > b.len() {
            return Err(DecodeError::Truncated);
        }
        let value = &b[start..end];
        match ty {
            ATTR_XOR_MAPPED_ADDRESS => xor_mapped = Some(read_xor_address(value, &tx)?),
            ATTR_MAPPED_ADDRESS => plain_mapped = Some(read_address(value)?),
            ATTR_ERROR_CODE => {
                if value.len() < 4 {
                    return Err(DecodeError::Truncated);
                }
                let class = (value[2] & 0x07) as u16;
                msg.error = Some(class * 100 + value[3] as u16);
            }
            ATTR_FINGERPRINT => {
                if len != 4 {
                    return Err(DecodeError::Truncated);
                }
                let want = u32::from_be_bytes([value[0], value[1], value[2], value[3]]);
                if crc32(&b[..at]) ^ FINGERPRINT_XOR != want {
                    return Err(DecodeError::BadFingerprint);
                }
                seen_fingerprint = true;
            }
            _ => {}
        }
        at = end + pad(len);
    }
    msg.mapped = xor_mapped.or(plain_mapped);
    Ok(msg)
}

fn header(kind: u16, tx: TxId) -> Vec<u8> {
    let mut m = Vec::with_capacity(64);
    m.extend_from_slice(&kind.to_be_bytes());
    m.extend_from_slice(&0u16.to_be_bytes());
    m.extend_from_slice(&MAGIC_COOKIE.to_be_bytes());
    m.extend_from_slice(&tx);
    m
}

fn pad(len: usize) -> usize {
    (4 - len % 4) % 4
}

fn put_attr(m: &mut Vec<u8>, ty: u16, value: &[u8]) {
    m.extend_from_slice(&ty.to_be_bytes());
    m.extend_from_slice(&(value.len() as u16).to_be_bytes());
    m.extend_from_slice(value);
    m.resize(m.len() + pad(value.len()), 0);
    set_len(m);
}

fn set_len(m: &mut [u8]) {
    let len = (m.len() - HEADER) as u16;
    m[2..4].copy_from_slice(&len.to_be_bytes());
}

/// The header length counts the 8-byte FINGERPRINT before the CRC is taken over everything
/// ahead of the attribute [documented: RFC 8489 section 14.7].
fn finish_with_fingerprint(mut m: Vec<u8>) -> Vec<u8> {
    let before = m.len();
    put_attr(&mut m, ATTR_FINGERPRINT, &[0; 4]);
    let crc = crc32(&m[..before]) ^ FINGERPRINT_XOR;
    m[before + 4..before + 8].copy_from_slice(&crc.to_be_bytes());
    m
}

/// The XOR key: the cookie, then for IPv6 the transaction id [documented: RFC 8489 section 14.2].
fn xor_key(tx: &TxId) -> [u8; 16] {
    let mut k = [0u8; 16];
    k[..4].copy_from_slice(&MAGIC_COOKIE.to_be_bytes());
    k[4..].copy_from_slice(tx);
    k
}

fn xor_address(addr: SocketAddr, tx: &TxId) -> Vec<u8> {
    let key = xor_key(tx);
    let port = addr.port() ^ (MAGIC_COOKIE >> 16) as u16;
    let mut v = vec![0u8];
    match addr.ip() {
        IpAddr::V4(ip) => {
            v.push(0x01);
            v.extend_from_slice(&port.to_be_bytes());
            v.extend(ip.octets().iter().zip(&key[..4]).map(|(a, k)| a ^ k));
        }
        IpAddr::V6(ip) => {
            v.push(0x02);
            v.extend_from_slice(&port.to_be_bytes());
            v.extend(ip.octets().iter().zip(&key).map(|(a, k)| a ^ k));
        }
    }
    v
}

fn read_address(v: &[u8]) -> Result<SocketAddr, DecodeError> {
    if v.len() < 4 {
        return Err(DecodeError::Truncated);
    }
    let port = u16::from_be_bytes([v[2], v[3]]);
    match (v[1], v.len()) {
        (0x01, 8) => Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(v[4], v[5], v[6], v[7])), port)),
        (0x02, 20) => {
            let mut o = [0u8; 16];
            o.copy_from_slice(&v[4..20]);
            Ok(SocketAddr::new(IpAddr::V6(Ipv6Addr::from(o)), port))
        }
        _ => Err(DecodeError::BadAddress),
    }
}

fn read_xor_address(v: &[u8], tx: &TxId) -> Result<SocketAddr, DecodeError> {
    let plain = read_address(v)?;
    let key = xor_key(tx);
    let port = plain.port() ^ (MAGIC_COOKIE >> 16) as u16;
    let ip = match plain.ip() {
        IpAddr::V4(ip) => {
            let mut o = ip.octets();
            o.iter_mut().zip(&key[..4]).for_each(|(a, k)| *a ^= k);
            IpAddr::V4(Ipv4Addr::from(o))
        }
        IpAddr::V6(ip) => {
            let mut o = ip.octets();
            o.iter_mut().zip(&key).for_each(|(a, k)| *a ^= k);
            IpAddr::V6(Ipv6Addr::from(o))
        }
    };
    Ok(SocketAddr::new(ip, port))
}

/// CRC-32 as gzip uses it (reflected polynomial 0xEDB88320, starting and ending inverted),
/// the one FINGERPRINT names [documented: RFC 8489 section 14.7, pointing to RFC 1952 section 8].
pub fn crc32(data: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            let mask = (crc & 1).wrapping_neg();
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

/// How the NAT in front of a socket picks its public port, from two STUN servers' answers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mapping {
    /// Both servers saw the same address: one public port for every destination. Hole
    /// punching works against it (unless both sides filter strictly).
    EndpointIndependent,
    /// The servers saw different ports: a new public port per destination (a "symmetric" NAT).
    EndpointDependent,
    /// Fewer than two answers.
    Unknown,
}

/// What [`Gather`] found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Gathered {
    /// Each server that answered, and the address it saw.
    pub answers: Vec<(SocketAddr, SocketAddr)>,
    pub mapping: Mapping,
}

impl Gathered {
    /// The distinct addresses the servers saw, as server-reflexive candidates.
    pub fn reflexive(&self) -> Vec<SocketAddr> {
        let mut out: Vec<SocketAddr> = Vec::new();
        for (_, seen) in &self.answers {
            if !out.contains(seen) {
                out.push(*seen);
            }
        }
        out
    }
}

/// Asks STUN servers (two, for [`Mapping`]) for this socket's public address. Sans-IO: call
/// [`Gather::poll`] every tick and send what it returns from the socket being asked about;
/// hand it every STUN datagram that arrives with [`Gather::on_packet`].
///
/// Resends follow RFC 8489 section 6.2.1 (500 ms, doubling, same transaction id) [documented],
/// cut off at `deadline` [design: a join should not wait the RFC's 39.5 s for a server that
/// is down; the default 3 s covers four sends].
pub struct Gather {
    servers: Vec<Server>,
    started: Instant,
    deadline: Duration,
    software: Option<String>,
}

struct Server {
    addr: SocketAddr,
    tx: TxId,
    sends: u32,
    next: Instant,
    rto: Duration,
    seen: Option<SocketAddr>,
}

impl Gather {
    pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(3);
    const RTO: Duration = Duration::from_millis(500);
    const MAX_SENDS: u32 = 7;

    /// `servers` with one random transaction id each.
    pub fn new(servers: &[(SocketAddr, TxId)], now: Instant) -> Self {
        Gather {
            servers: servers
                .iter()
                .map(|&(addr, tx)| Server { addr, tx, sends: 0, next: now, rto: Self::RTO, seen: None })
                .collect(),
            started: now,
            deadline: Self::DEFAULT_DEADLINE,
            software: None,
        }
    }

    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    /// The SOFTWARE attribute to send, e.g. "NVForever 0.7.0".
    pub fn with_software(mut self, software: &str) -> Self {
        self.software = Some(software.to_owned());
        self
    }

    /// Datagrams due now: (server, bytes).
    pub fn poll(&mut self, now: Instant) -> Vec<(SocketAddr, Vec<u8>)> {
        if self.done(now) {
            return Vec::new();
        }
        let software = self.software.as_deref();
        let mut out = Vec::new();
        for s in &mut self.servers {
            if s.seen.is_none() && s.sends < Self::MAX_SENDS && now >= s.next {
                out.push((s.addr, binding_request(s.tx, software)));
                s.sends += 1;
                s.next = now + s.rto;
                s.rto *= 2;
            }
        }
        out
    }

    /// A datagram from `from`. Returns whether it was an answer to this gather.
    pub fn on_packet(&mut self, from: SocketAddr, b: &[u8]) -> bool {
        let Ok(msg) = decode(b) else { return false };
        let from = crate::candidate::canonical(from);
        for s in &mut self.servers {
            if s.tx == msg.tx && crate::candidate::canonical(s.addr) == from {
                if msg.kind == BINDING_SUCCESS {
                    if let Some(seen) = msg.mapped {
                        s.seen = Some(crate::candidate::canonical(seen));
                    }
                }
                return true;
            }
        }
        false
    }

    /// Finished: every server answered, or the deadline passed.
    pub fn done(&self, now: Instant) -> bool {
        self.servers.iter().all(|s| s.seen.is_some()) || now.duration_since(self.started) >= self.deadline
    }

    /// The result so far (final once [`Gather::done`]).
    pub fn result(&self) -> Gathered {
        let answers: Vec<_> = self.servers.iter().filter_map(|s| s.seen.map(|seen| (s.addr, seen))).collect();
        let mapping = match answers.as_slice() {
            [a, b, ..] if a.1 == b.1 => Mapping::EndpointIndependent,
            [_, _, ..] => Mapping::EndpointDependent,
            _ => Mapping::Unknown,
        };
        Gathered { answers, mapping }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The transaction id of RFC 5769's samples [documented: RFC 5769 section 2.1].
    const RFC5769_TX: TxId = [0xb7, 0xe7, 0xa7, 0x01, 0xbc, 0x34, 0xd6, 0x86, 0xfa, 0x87, 0xdf, 0xae];

    #[test]
    fn crc32_check_value() {
        // The CRC-32 (ISO-HDLC, gzip's) check value of "123456789" [documented: the CRC
        // catalogue's CRC-32/ISO-HDLC entry].
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn xor_mapped_ipv4_matches_rfc5769() {
        // RFC 5769 section 2.2: 192.0.2.1 port 32853 is sent as family 01, X-Port a1 47,
        // X-Address e1 12 a6 43 [documented].
        let v = xor_address("192.0.2.1:32853".parse().unwrap(), &RFC5769_TX);
        assert_eq!(v, [0x00, 0x01, 0xa1, 0x47, 0xe1, 0x12, 0xa6, 0x43]);
        assert_eq!(read_xor_address(&v, &RFC5769_TX).unwrap(), "192.0.2.1:32853".parse().unwrap());
    }

    #[test]
    fn xor_mapped_ipv6_matches_rfc5769() {
        // RFC 5769 section 2.3 [documented].
        let addr: SocketAddr = "[2001:db8:1234:5678:11:2233:4455:6677]:32853".parse().unwrap();
        let v = xor_address(addr, &RFC5769_TX);
        let want = [
            0x00, 0x02, 0xa1, 0x47, 0x01, 0x13, 0xa9, 0xfa, 0xa5, 0xd3, 0xf1, 0x79, 0xbc, 0x25, 0xf4, 0xb5, 0xbe, 0xd2,
            0xb9, 0xd9,
        ];
        assert_eq!(v, want);
        assert_eq!(read_xor_address(&v, &RFC5769_TX).unwrap(), addr);
    }

    #[test]
    fn request_round_trip() {
        let req = binding_request(RFC5769_TX, Some("NVForever test"));
        assert!(is_stun(&req));
        assert_eq!(req.len() % 4, 0);
        let m = decode(&req).unwrap();
        assert_eq!(m.kind, BINDING_REQUEST);
        assert_eq!(m.tx, RFC5769_TX);
        assert_eq!(m.mapped, None);
    }

    #[test]
    fn response_round_trip_and_fingerprint_checked() {
        let addr: SocketAddr = "203.0.113.5:40000".parse().unwrap();
        let mut resp = binding_success(RFC5769_TX, addr);
        let m = decode(&resp).unwrap();
        assert_eq!((m.kind, m.mapped), (BINDING_SUCCESS, Some(addr)));
        // Flip one bit of the address: the fingerprint no longer matches.
        resp[HEADER + 6] ^= 1;
        assert_eq!(decode(&resp), Err(DecodeError::BadFingerprint));
    }

    #[test]
    fn not_stun() {
        assert!(!is_stun(b"NVCO\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00"));
        assert!(!is_stun(&[0; 8]));
        let mut req = binding_request(RFC5769_TX, None);
        req.push(0);
        assert!(!is_stun(&req), "length must match the datagram");
    }

    #[test]
    fn truncated_attribute_is_an_error() {
        let mut m = header(BINDING_SUCCESS, RFC5769_TX);
        m.extend_from_slice(&[0x00, 0x20, 0x00, 0x08, 0, 1, 0, 0]);
        set_len(&mut m);
        assert_eq!(decode(&m), Err(DecodeError::Truncated));
    }

    #[test]
    fn gather_resends_then_classifies() {
        let t0 = Instant::now();
        let s1: SocketAddr = "198.51.100.1:3478".parse().unwrap();
        let s2: SocketAddr = "198.51.100.2:3478".parse().unwrap();
        let mut g = Gather::new(&[(s1, [1; 12]), (s2, [2; 12])], t0);
        assert_eq!(g.poll(t0).len(), 2);
        assert!(g.poll(t0 + Duration::from_millis(499)).is_empty());
        assert_eq!(g.poll(t0 + Duration::from_millis(500)).len(), 2, "first resend at 500 ms");
        assert!(g.poll(t0 + Duration::from_millis(1499)).is_empty());
        assert_eq!(g.poll(t0 + Duration::from_millis(1500)).len(), 2, "then 1 s later");
        let seen: SocketAddr = "203.0.113.9:5000".parse().unwrap();
        assert!(g.on_packet(s1, &binding_success([1; 12], seen)));
        assert!(!g.on_packet(s1, &binding_success([9; 12], seen)), "unknown transaction");
        assert!(!g.on_packet(s2, &binding_success([1; 12], seen)), "wrong server");
        assert!(!g.done(t0 + Duration::from_secs(2)));
        assert_eq!(g.poll(t0 + Duration::from_millis(3499)).len(), 0, "deadline");
        assert!(g.on_packet(s2, &binding_success([2; 12], "203.0.113.9:5001".parse().unwrap())));
        let r = g.result();
        assert_eq!(r.mapping, Mapping::EndpointDependent);
        assert_eq!(r.reflexive().len(), 2);
    }
}
