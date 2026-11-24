//! Asking the home router for a port: PCP (RFC 6887) and NAT-PMP (RFC 6886). Format notes:
//! `docs/spec-notes/nat-traversal.md`. UPnP IGD is a different protocol (HTTP and SOAP); it
//! stays with the `igd-next` crate in the caller, and its result is a [`Mapping`] like these.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};
use std::time::{Duration, Instant};

/// The router's PCP and NAT-PMP port [documented: RFC 6886 sections 3.1 and 3.3; RFC 6887
/// section 19.1].
pub const SERVER_PORT: u16 = 5351;

const PCP_VERSION: u8 = 2;
const PCP_OP_MAP: u8 = 1;
const PROTO_UDP: u8 = 17;
const NATPMP_OP_EXTERNAL: u8 = 0;
const NATPMP_OP_MAP_UDP: u8 = 1;
/// PCP and NAT-PMP both use result 1 for an unsupported version [documented: RFC 6886
/// section 3.5; RFC 6887 section 7.4].
const UNSUPP_VERSION: u16 = 1;

/// A PCP MAP request for UDP [documented: RFC 6887 sections 7.1 and 11.1]: 24-byte header
/// (version 2, opcode 1, lifetime, client address as IPv6) and 36 bytes of MAP data (nonce,
/// protocol 17, internal port, suggested external port and address). A suggested address of
/// all zeros means "any" (the unspecified address of the client's family).
pub fn pcp_map_request(nonce: [u8; 12], client: IpAddr, internal_port: u16, suggested: Option<SocketAddr>, lifetime: u32) -> Vec<u8> {
    let mut m = Vec::with_capacity(60);
    m.push(PCP_VERSION);
    m.push(PCP_OP_MAP);
    m.extend_from_slice(&[0, 0]);
    m.extend_from_slice(&lifetime.to_be_bytes());
    m.extend_from_slice(&as_v6(client));
    m.extend_from_slice(&nonce);
    m.push(PROTO_UDP);
    m.extend_from_slice(&[0, 0, 0]);
    m.extend_from_slice(&internal_port.to_be_bytes());
    let (port, ip) = match suggested {
        Some(s) => (s.port(), s.ip()),
        None if client.is_ipv4() => (0, IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
        None => (0, IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
    };
    m.extend_from_slice(&port.to_be_bytes());
    m.extend_from_slice(&as_v6(ip));
    m
}

/// NAT-PMP's external address request: version 0, opcode 0 [documented: RFC 6886 section 3.2].
pub fn natpmp_external_request() -> [u8; 2] {
    [0, NATPMP_OP_EXTERNAL]
}

/// NAT-PMP's UDP map request [documented: RFC 6886 section 3.3]. Lifetime 0 removes it
/// (section 3.4).
pub fn natpmp_map_request(internal_port: u16, suggested_port: u16, lifetime: u32) -> [u8; 12] {
    let mut m = [0u8; 12];
    m[1] = NATPMP_OP_MAP_UDP;
    m[4..6].copy_from_slice(&internal_port.to_be_bytes());
    m[6..8].copy_from_slice(&suggested_port.to_be_bytes());
    m[8..12].copy_from_slice(&lifetime.to_be_bytes());
    m
}

/// IPv4 as an IPv4-mapped IPv6 address [documented: RFC 6887 section 5].
fn as_v6(ip: IpAddr) -> [u8; 16] {
    match ip {
        IpAddr::V4(v4) => v4.to_ipv6_mapped().octets(),
        IpAddr::V6(v6) => v6.octets(),
    }
}

fn from_v6(o: [u8; 16]) -> IpAddr {
    let v6 = Ipv6Addr::from(o);
    match v6.to_ipv4_mapped() {
        Some(v4) => IpAddr::V4(v4),
        None => IpAddr::V6(v6),
    }
}

/// A decoded answer from the router.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Response {
    /// A PCP MAP response [documented: RFC 6887 sections 7.2 and 11.1].
    PcpMap { result: u8, lifetime: u32, nonce: [u8; 12], internal_port: u16, external: SocketAddr },
    /// Any other PCP response (another opcode, or too short to carry MAP data).
    PcpOther { result: u8 },
    /// NAT-PMP's external address response [documented: RFC 6886 section 3.2].
    NatPmpExternal { result: u16, ip: Ipv4Addr },
    /// NAT-PMP's UDP map response [documented: RFC 6886 section 3.3].
    NatPmpMap { result: u16, internal_port: u16, external_port: u16, lifetime: u32 },
    /// A version-0 reply with only a result, as a NAT-PMP router sends to a request it does not
    /// understand, PCP's included [documented: RFC 6886 section 3.5].
    NatPmpResult { result: u16 },
}

/// Decodes a datagram from the router. `None` for anything that is not a well-formed answer.
pub fn decode(b: &[u8]) -> Option<Response> {
    match *b.first()? {
        PCP_VERSION => decode_pcp(b),
        0 => decode_natpmp(b),
        _ => None,
    }
}

fn decode_pcp(b: &[u8]) -> Option<Response> {
    // A client ignores replies under 24 bytes, over 1100 or not a multiple of 4, and the R bit
    // must be set on a response [documented: RFC 6887 sections 7.2 and 8.3].
    if b.len() < 24 || b.len() > 1100 || !b.len().is_multiple_of(4) || b[1] & 0x80 == 0 {
        return None;
    }
    let result = b[3];
    let lifetime = u32::from_be_bytes([b[4], b[5], b[6], b[7]]);
    if b[1] & 0x7f != PCP_OP_MAP || b.len() < 60 {
        return Some(Response::PcpOther { result });
    }
    let mut nonce = [0u8; 12];
    nonce.copy_from_slice(&b[24..36]);
    let internal_port = u16::from_be_bytes([b[40], b[41]]);
    let port = u16::from_be_bytes([b[42], b[43]]);
    let mut ip = [0u8; 16];
    ip.copy_from_slice(&b[44..60]);
    Some(Response::PcpMap { result, lifetime, nonce, internal_port, external: SocketAddr::new(from_v6(ip), port) })
}

fn decode_natpmp(b: &[u8]) -> Option<Response> {
    if b.len() < 4 {
        return None;
    }
    let op = b[1];
    let result = u16::from_be_bytes([b[2], b[3]]);
    match (op, b.len()) {
        (128, 12..) => Some(Response::NatPmpExternal { result, ip: Ipv4Addr::new(b[8], b[9], b[10], b[11]) }),
        (129, 16..) => Some(Response::NatPmpMap {
            result,
            internal_port: u16::from_be_bytes([b[8], b[9]]),
            external_port: u16::from_be_bytes([b[10], b[11]]),
            lifetime: u32::from_be_bytes([b[12], b[13], b[14], b[15]]),
        }),
        _ => Some(Response::NatPmpResult { result }),
    }
}

/// Which protocol made a mapping.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    Pcp,
    NatPmp,
    /// For the caller's UPnP IGD mappings, so all three look the same.
    Upnp,
}

/// A port the router opened.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mapping {
    pub protocol: Protocol,
    pub external: SocketAddr,
    pub lifetime: Duration,
    /// When it was granted; renew from half its lifetime (RFC 6887 section 11.2.1 says 1/2 to
    /// 5/8) [documented].
    pub granted: Instant,
}

impl Mapping {
    pub fn renew_at(&self) -> Instant {
        self.granted + self.lifetime / 2
    }
}

/// Why no mapping was made.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// Nothing answered by the deadline: no PCP or NAT-PMP on this router (or it is off).
    NoAnswer,
    /// The router refused with this result code (PCP's or NAT-PMP's numbering, by `protocol`).
    Refused { protocol: Protocol, result: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Step {
    Pcp,
    NatPmpExternal,
    NatPmpMap { ip: Ipv4Addr },
    Done,
}

/// Asks the gateway for a UDP port, sans-IO: PCP first, as RFC 6886 section 1.1 advises, then
/// NAT-PMP when the router answers in version 0 [documented: RFC 6887 section 9, RFC 6886
/// section 3.5]. Call [`Mapper::poll`] every tick and send what it returns to
/// `gateway:5351` from the socket being mapped; hand it every datagram from the gateway with
/// [`Mapper::on_packet`]; read [`Mapper::outcome`].
///
/// Resends start at 250 ms and double, NAT-PMP's schedule (RFC 6886 section 3.1), for PCP too,
/// until `deadline` [design: RFC 6887 section 8.1.1 asks PCP for 3 s first, which would hold a
/// join for seconds on a router without PCP; the default 2 s deadline covers four sends].
pub struct Mapper {
    gateway: SocketAddr,
    client: IpAddr,
    internal_port: u16,
    lifetime: u32,
    nonce: [u8; 12],
    step: Step,
    started: Instant,
    deadline: Duration,
    next: Instant,
    rto: Duration,
    outcome: Option<Result<Mapping, Failure>>,
}

impl Mapper {
    pub const DEFAULT_DEADLINE: Duration = Duration::from_secs(2);
    /// RFC 6886 section 3.3 recommends 7200 s [documented].
    pub const DEFAULT_LIFETIME: u32 = 7200;
    const RTO: Duration = Duration::from_millis(250);

    /// `client` is the socket's own LAN address (PCP checks it against the packet's source);
    /// `nonce` a random value of the caller's (RFC 6887 section 11.1).
    pub fn new(gateway: IpAddr, client: IpAddr, internal_port: u16, nonce: [u8; 12], now: Instant) -> Self {
        Mapper {
            gateway: SocketAddr::new(gateway, SERVER_PORT),
            client,
            internal_port,
            lifetime: Self::DEFAULT_LIFETIME,
            nonce,
            step: Step::Pcp,
            started: now,
            deadline: Self::DEFAULT_DEADLINE,
            next: now,
            rto: Self::RTO,
            outcome: None,
        }
    }

    pub fn with_deadline(mut self, deadline: Duration) -> Self {
        self.deadline = deadline;
        self
    }

    pub fn with_lifetime(mut self, seconds: u32) -> Self {
        self.lifetime = seconds;
        self
    }

    pub fn gateway(&self) -> SocketAddr {
        self.gateway
    }

    fn request(&self) -> Vec<u8> {
        match self.step {
            Step::Pcp => pcp_map_request(self.nonce, self.client, self.internal_port, None, self.lifetime),
            Step::NatPmpExternal => natpmp_external_request().to_vec(),
            Step::NatPmpMap { .. } => natpmp_map_request(self.internal_port, self.internal_port, self.lifetime).to_vec(),
            Step::Done => Vec::new(),
        }
    }

    /// The datagram due now for the gateway, if any.
    pub fn poll(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.outcome.is_some() {
            return None;
        }
        if now.duration_since(self.started) >= self.deadline {
            self.finish(Err(Failure::NoAnswer));
            return None;
        }
        if now < self.next {
            return None;
        }
        self.next = now + self.rto;
        self.rto *= 2;
        Some(self.request())
    }

    fn advance(&mut self, step: Step, now: Instant) {
        self.step = step;
        self.next = now;
        self.rto = Self::RTO;
    }

    fn finish(&mut self, outcome: Result<Mapping, Failure>) {
        self.step = Step::Done;
        self.outcome = Some(outcome);
    }

    /// A datagram from `from`. Returns whether it was the gateway's answer to this mapper.
    /// Packets from anyone else are dropped (RFC 6886 section 3.2 asks clients to) [documented].
    pub fn on_packet(&mut self, from: SocketAddr, b: &[u8], now: Instant) -> bool {
        if crate::candidate::canonical(from) != crate::candidate::canonical(self.gateway) || self.outcome.is_some() {
            return false;
        }
        let Some(resp) = decode(b) else { return false };
        match (self.step, resp) {
            (Step::Pcp, Response::PcpMap { result, lifetime, nonce, internal_port, external }) => {
                if nonce != self.nonce || internal_port != self.internal_port {
                    return false;
                }
                if result == 0 {
                    let lifetime = Duration::from_secs(lifetime as u64);
                    self.finish(Ok(Mapping { protocol: Protocol::Pcp, external, lifetime, granted: now }));
                } else if result as u16 == UNSUPP_VERSION {
                    self.advance(Step::NatPmpExternal, now);
                } else {
                    self.finish(Err(Failure::Refused { protocol: Protocol::Pcp, result: result as u16 }));
                }
            }
            (Step::Pcp, Response::PcpOther { result }) if result as u16 == UNSUPP_VERSION => {
                self.advance(Step::NatPmpExternal, now)
            }
            // A version-0 answer to PCP: the router speaks NAT-PMP.
            (Step::Pcp, Response::NatPmpResult { .. } | Response::NatPmpExternal { .. } | Response::NatPmpMap { .. }) => {
                self.advance(Step::NatPmpExternal, now)
            }
            (Step::NatPmpExternal, Response::NatPmpExternal { result: 0, ip }) => self.advance(Step::NatPmpMap { ip }, now),
            (Step::NatPmpMap { ip }, Response::NatPmpMap { result: 0, internal_port, external_port, lifetime }) => {
                if internal_port != self.internal_port {
                    return false;
                }
                let external = SocketAddr::new(IpAddr::V4(ip), external_port);
                let lifetime = Duration::from_secs(lifetime as u64);
                self.finish(Ok(Mapping { protocol: Protocol::NatPmp, external, lifetime, granted: now }));
            }
            (
                Step::NatPmpExternal | Step::NatPmpMap { .. },
                Response::NatPmpExternal { result, .. } | Response::NatPmpMap { result, .. } | Response::NatPmpResult { result },
            ) => self.finish(Err(Failure::Refused { protocol: Protocol::NatPmp, result })),
            _ => return false,
        }
        true
    }

    pub fn outcome(&self) -> Option<Result<Mapping, Failure>> {
        self.outcome
    }

    /// The request that removes the mapping (lifetime 0), to send to [`Mapper::gateway`] on
    /// leaving [documented: RFC 6886 section 3.4; RFC 6887 section 11.1].
    pub fn release(&self) -> Option<Vec<u8>> {
        match self.outcome? {
            Ok(m) if m.protocol == Protocol::Pcp => Some(pcp_map_request(self.nonce, self.client, self.internal_port, None, 0)),
            Ok(m) if m.protocol == Protocol::NatPmp => Some(natpmp_map_request(self.internal_port, 0, 0).to_vec()),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NONCE: [u8; 12] = [7; 12];

    fn gw() -> IpAddr {
        "192.168.1.1".parse().unwrap()
    }
    fn me() -> IpAddr {
        "192.168.1.20".parse().unwrap()
    }
    fn gw_addr() -> SocketAddr {
        SocketAddr::new(gw(), SERVER_PORT)
    }

    /// A PCP router's answer, built from the request as RFC 6887 section 11.1 describes.
    fn pcp_answer(req: &[u8], result: u8, external: SocketAddr, lifetime: u32) -> Vec<u8> {
        let mut r = vec![0u8; 60];
        r[0] = 2;
        r[1] = 0x80 | PCP_OP_MAP;
        r[3] = result;
        r[4..8].copy_from_slice(&lifetime.to_be_bytes());
        r[24..42].copy_from_slice(&req[24..42]);
        r[42..44].copy_from_slice(&external.port().to_be_bytes());
        r[44..60].copy_from_slice(&as_v6(external.ip()));
        r
    }

    #[test]
    fn pcp_request_layout() {
        let req = pcp_map_request(NONCE, me(), 47960, None, 7200);
        assert_eq!(req.len(), 60);
        assert_eq!(&req[..2], &[2, 1]);
        assert_eq!(&req[4..8], &7200u32.to_be_bytes());
        // 192.168.1.20 as ::ffff:192.168.1.20.
        assert_eq!(&req[8..24], &[0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0xff, 0xff, 192, 168, 1, 20]);
        assert_eq!(&req[24..36], &NONCE);
        assert_eq!(req[36], 17);
        assert_eq!(&req[40..42], &47960u16.to_be_bytes());
        assert_eq!(&req[42..44], &[0, 0]);
        assert_eq!(&req[44..60], &as_v6(IpAddr::V4(Ipv4Addr::UNSPECIFIED)));
    }

    #[test]
    fn natpmp_request_layout() {
        assert_eq!(natpmp_external_request(), [0, 0]);
        let m = natpmp_map_request(47960, 47960, 7200);
        assert_eq!(m[..4], [0, 1, 0, 0]);
        assert_eq!(m[4..6], 47960u16.to_be_bytes());
        assert_eq!(m[8..], 7200u32.to_be_bytes());
    }

    #[test]
    fn pcp_success() {
        let t0 = Instant::now();
        let mut m = Mapper::new(gw(), me(), 47960, NONCE, t0);
        let req = m.poll(t0).unwrap();
        let ext: SocketAddr = "203.0.113.4:50000".parse().unwrap();
        let other: SocketAddr = "192.168.1.99:5351".parse().unwrap();
        assert!(!m.on_packet(other, &pcp_answer(&req, 0, ext, 3600), t0), "not the gateway");
        assert!(m.on_packet(gw_addr(), &pcp_answer(&req, 0, ext, 3600), t0));
        let got = m.outcome().unwrap().unwrap();
        assert_eq!((got.protocol, got.external, got.lifetime), (Protocol::Pcp, ext, Duration::from_secs(3600)));
        assert_eq!(got.renew_at(), t0 + Duration::from_secs(1800));
        assert_eq!(m.release().unwrap()[4..8], [0, 0, 0, 0]);
    }

    #[test]
    fn pcp_wrong_nonce_ignored() {
        let t0 = Instant::now();
        let mut m = Mapper::new(gw(), me(), 47960, NONCE, t0);
        let mut req = m.poll(t0).unwrap();
        req[24] ^= 1;
        assert!(!m.on_packet(gw_addr(), &pcp_answer(&req, 0, "203.0.113.4:1".parse().unwrap(), 60), t0));
        assert_eq!(m.outcome(), None);
    }

    #[test]
    fn falls_back_to_natpmp() {
        let t0 = Instant::now();
        let mut m = Mapper::new(gw(), me(), 47960, NONCE, t0);
        m.poll(t0).unwrap();
        // NAT-PMP router: an 8-byte "unsupported version" reply (RFC 6886 section 3.5).
        assert!(m.on_packet(gw_addr(), &[0, 0, 0, 1, 0, 0, 0, 9], t0));
        assert_eq!(m.poll(t0).unwrap(), vec![0, 0]);
        let mut ext = [0u8; 12];
        ext[1] = 128;
        ext[8..].copy_from_slice(&[203, 0, 113, 4]);
        assert!(m.on_packet(gw_addr(), &ext, t0));
        assert_eq!(m.poll(t0).unwrap()[1], 1, "map request next");
        let mut map = [0u8; 16];
        map[1] = 129;
        map[8..10].copy_from_slice(&47960u16.to_be_bytes());
        map[10..12].copy_from_slice(&40001u16.to_be_bytes());
        map[12..].copy_from_slice(&7200u32.to_be_bytes());
        assert!(m.on_packet(gw_addr(), &map, t0));
        let got = m.outcome().unwrap().unwrap();
        assert_eq!((got.protocol, got.external), (Protocol::NatPmp, "203.0.113.4:40001".parse().unwrap()));
        assert_eq!(m.release().unwrap(), natpmp_map_request(47960, 0, 0).to_vec());
    }

    #[test]
    fn refusal_and_timeout() {
        let t0 = Instant::now();
        let mut m = Mapper::new(gw(), me(), 47960, NONCE, t0);
        let req = m.poll(t0).unwrap();
        // NOT_AUTHORIZED is 2 [documented: RFC 6887 section 7.4].
        assert!(m.on_packet(gw_addr(), &pcp_answer(&req, 2, "0.0.0.0:0".parse().unwrap(), 0), t0));
        assert_eq!(m.outcome(), Some(Err(Failure::Refused { protocol: Protocol::Pcp, result: 2 })));

        let mut m = Mapper::new(gw(), me(), 47960, NONCE, t0);
        let sends: usize = (0..2000).filter_map(|ms| m.poll(t0 + Duration::from_millis(ms))).count();
        // Sends at 0, 250, 750 and 1750 ms before the 2 s deadline.
        assert_eq!(sends, 4);
        assert_eq!(m.poll(t0 + Duration::from_secs(2)), None);
        assert_eq!(m.outcome(), Some(Err(Failure::NoAnswer)));
    }
}
