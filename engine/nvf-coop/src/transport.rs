//! The sockets under the co-op wire (docs/design/coop-networking.md §3, phase 1): every
//! packet [`crate::net`] sends or takes goes through a [`Transport`], which picks the
//! way to the address it is given.
//!
//! - An IPv4 address goes over the IPv4 socket.
//! - An IPv6 address goes over the IPv6 socket. An IPv4-mapped IPv6 address
//!   (`::ffff:a.b.c.d`) is taken as the IPv4 address it holds.
//!
//! A host listens on both families on the same port: IPv4 must bind, IPv6 is added when the
//! PC has it. The IPv6 socket is IPv6-only, so it never takes the IPv4 port from the
//! other socket (Linux binds `[::]` dual-stack by default). A guest opens a socket of each
//! family its host's candidate addresses need. Nothing above this module knows how many
//! sockets there are.
//!
//! The sockets are non-blocking. [`Transport::recv`] drains everything waiting, on every
//! socket.

use std::cell::RefCell;
use std::io::{self, ErrorKind};
use std::time::Instant;

use nvf_nat::hosted::{self, Client, Received, SessionId, Side};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};

use socket2::{Domain, Protocol, Socket, Type};

/// The largest datagram taken: Ethernet's MTU, as before.
const RECV_MAX: usize = 1500;
/// IP and UDP headers, counted with each packet's bytes in the traffic lines.
pub const UDP_OVERHEAD_V4: u64 = 28;
pub const UDP_OVERHEAD_V6: u64 = 48;

/// Where a host listens.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Every network: IPv4 on all addresses, and IPv6 on all addresses when the PC has it.
    All,
    /// This PC only (IPv4 loopback): split screen's player 2, no firewall prompt.
    Loopback,
}

/// The co-op sockets of one game.
pub struct Transport {
    v4: Option<UdpSocket>,
    v6: Option<UdpSocket>,
    /// Sessions on relays players host ([`Transport::hosted_open`]).
    hosted: RefCell<Vec<Hosted>>,
}

/// A session on a relay a player hosts (`nvf-relay`, `nvf_nat::hosted`,
/// docs/design/coop-networking.md §9): its client, and the stand-in address the game knows
/// the other side by ([`hosted_standin`]). What goes to the stand-in is wrapped and sent to
/// the relay; what comes from the relay for this session is unwrapped and taken as from the
/// stand-in, so the Noise session and the checks see one plain address.
struct Hosted {
    client: Client,
    standin: SocketAddr,
}

/// The stand-in address of hosted-relay session `s`: an address in 240.0.0.0/4 (reserved,
/// never a real host [documented: RFC 1112 §4]) holding 28 bits of the session id, and 16
/// more as the port.
pub fn hosted_standin(s: &SessionId) -> SocketAddr {
    let a = (u32::from_be_bytes([s[0], s[1], s[2], s[3]]) & 0x0FFF_FFFF) | 0xF000_0000;
    let port = u16::from_be_bytes([s[4], s[5]]).max(1);
    SocketAddr::new(IpAddr::V4(Ipv4Addr::from(a)), port)
}

/// Whether `a` is a hosted relay's stand-in ([`hosted_standin`]).
pub fn is_hosted_standin(a: SocketAddr) -> bool {
    matches!(canonical(a), SocketAddr::V4(v) if (240..255).contains(&v.ip().octets()[0]))
}

impl Transport {
    /// A host's sockets on `port` (0 picks a free one; IPv6 then takes the same port when
    /// it is free there). IPv4 must bind, IPv6 is best effort.
    pub fn host(port: u16, scope: Scope) -> io::Result<Self> {
        match scope {
            Scope::Loopback => Ok(Self { v4: Some(bind_v4(Ipv4Addr::LOCALHOST, port)?), v6: None, hosted: RefCell::default() }),
            Scope::All => {
                let v4 = bind_v4(Ipv4Addr::UNSPECIFIED, port)?;
                let port = v4.local_addr()?.port();
                let v6 = bind_v6(Ipv6Addr::UNSPECIFIED, port).ok();
                Ok(Self { v4: Some(v4), v6, hosted: RefCell::default() })
            }
        }
    }

    /// A guest's sockets for reaching `candidates`, on free ports: loopback only when every
    /// candidate is this PC (no firewall prompt), else every address, with broadcast on the
    /// IPv4 socket for a host on the guest's own network. IPv6 is opened when a candidate
    /// is IPv6 and is needed only then; IPv4 is always
    /// opened unless every candidate is IPv6.
    pub fn guest(candidates: &[SocketAddr]) -> io::Result<Self> {
        let real: Vec<SocketAddr> = candidates.iter().map(|c| canonical(*c)).collect();
        let want_v6 = real.iter().any(SocketAddr::is_ipv6);
        let want_v4 = real.iter().any(SocketAddr::is_ipv4) || !want_v6;
        let local = !real.is_empty() && real.iter().all(|c| c.ip().is_loopback());
        let v4 = if want_v4 {
            let s = bind_v4(if local { Ipv4Addr::LOCALHOST } else { Ipv4Addr::UNSPECIFIED }, 0)?;
            let _ = s.set_broadcast(true);
            Some(s)
        } else {
            None
        };
        let v6 = if want_v6 {
            match bind_v6(if local { Ipv6Addr::LOCALHOST } else { Ipv6Addr::UNSPECIFIED }, 0) {
                Ok(s) => Some(s),
                // With IPv4 to fall back on, a PC without IPv6 still joins.
                Err(_) if v4.is_some() => None,
                Err(e) => return Err(e),
            }
        } else {
            None
        };
        Ok(Self { v4, v6, hosted: RefCell::default() })
    }

    /// A guest's sockets before it knows where its host is (an online code): IPv4 on every
    /// address, and IPv6 when the PC has it.
    pub fn guest_anywhere() -> io::Result<Self> {
        let v4 = bind_v4(Ipv4Addr::UNSPECIFIED, 0)?;
        let _ = v4.set_broadcast(true);
        Ok(Self { v4: Some(v4), v6: bind_v6(Ipv6Addr::UNSPECIFIED, 0).ok(), hosted: RefCell::default() })
    }

    /// The address shown for this game: the IPv4 socket's, else the IPv6 one's.
    pub fn local_addr(&self) -> io::Result<SocketAddr> {
        match (&self.v4, &self.v6) {
            (Some(s), _) | (None, Some(s)) => s.local_addr(),
            (None, None) => Err(io::Error::new(ErrorKind::NotConnected, "no co-op socket")),
        }
    }

    /// The IPv6 socket's address, when there is one.
    pub fn local_addr_v6(&self) -> Option<SocketAddr> {
        self.v6.as_ref().and_then(|s| s.local_addr().ok())
    }

    /// Whether this game listens on this PC's loopback only.
    pub fn local_only(&self) -> bool {
        let lo = |s: &Option<UdpSocket>| s.as_ref().is_none_or(|s| s.local_addr().is_ok_and(|a| a.ip().is_loopback()));
        (self.v4.is_some() || self.v6.is_some()) && lo(&self.v4) && lo(&self.v6)
    }

    /// A host moves to `scope` on the port it has: the old sockets are let go first, then
    /// the port is bound again. If that fails, the old scope is bound again (best effort)
    /// and the error returned.
    pub fn rebind(&mut self, scope: Scope) -> io::Result<()> {
        let port = self.local_addr()?.port();
        let was = if self.local_only() { Scope::Loopback } else { Scope::All };
        self.v4 = None;
        self.v6 = None;
        match Self::host(port, scope) {
            Ok(t) => {
                self.v4 = t.v4;
                self.v6 = t.v6;
                Ok(())
            }
            Err(e) => {
                if let Ok(t) = Self::host(port, was) {
                    self.v4 = t.v4;
                    self.v6 = t.v6;
                }
                Err(e)
            }
        }
    }

    /// Send `bytes` to `to` over the socket of its family. A packet with no way to go (no
    /// socket of that family) is dropped, as UDP drops any.
    pub fn send(&self, bytes: &[u8], to: SocketAddr) {
        let to = canonical(to);
        if is_hosted_standin(to) {
            let wrapped = self.hosted.borrow().iter().find(|h| h.standin == to).map(|h| (h.client.wrap(bytes), h.client.relay()));
            if let Some((b, relay)) = wrapped {
                self.send_raw(&b, relay);
            }
            return;
        }
        self.send_raw(bytes, to);
    }

    fn send_raw(&self, bytes: &[u8], to: SocketAddr) {
        let to = canonical(to);
        let sock = if to.is_ipv4() { &self.v4 } else { &self.v6 };
        if let Some(s) = sock {
            let _ = s.send_to(bytes, to);
        }
    }

    /// Everything waiting, on every socket, each packet handed to `take` with
    /// the address it came from (an IPv4-mapped IPv6 address as its IPv4 one).
    pub fn recv(&self, mut take: impl FnMut(&[u8], SocketAddr)) {
        let mut buf = [0u8; RECV_MAX];
        for sock in self.v4.iter().chain(self.v6.iter()) {
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) => {
                        let from = canonical(from);
                        match self.unwrap_hosted(&buf[..n], from) {
                            Some(Some((payload, standin))) => take(&payload, standin),
                            Some(None) => {}
                            None => take(&buf[..n], from),
                        }
                    }
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    // Windows reports an ICMP "port unreachable" from an earlier send as an
                    // error on a later receive (WSAECONNRESET); it says nothing about any
                    // packet.
                    Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                    Err(_) => break,
                }
            }
        }
    }
}

impl Transport {
    /// A session on the relay at `relay` for the pair `session`, as `side`: its stand-in
    /// address ([`hosted_standin`]). The same session again is the one already open.
    pub fn hosted_open(&self, relay: SocketAddr, session: SessionId, side: Side) -> SocketAddr {
        let standin = hosted_standin(&session);
        let mut h = self.hosted.borrow_mut();
        if !h.iter().any(|x| x.client.session() == session) {
            h.push(Hosted { client: Client::new(canonical(relay), session, side), standin });
        }
        standin
    }

    /// The relays' Binds that are due, sent.
    pub fn hosted_poll(&self, now: Instant) {
        let due: Vec<(Vec<u8>, SocketAddr)> = self.hosted.borrow_mut().iter_mut().filter_map(|h| h.client.poll(now).map(|b| (b, h.client.relay()))).collect();
        for (b, relay) in due {
            self.send_raw(&b, relay);
        }
    }

    /// Whether the relay took this side of the session whose stand-in is `standin`.
    pub fn hosted_bound(&self, standin: SocketAddr) -> bool {
        self.hosted.borrow().iter().any(|h| h.standin == standin && h.client.bound())
    }

    /// A datagram from `from`, when it is a hosted relay's for one of this game's sessions:
    /// `Some(Some(payload, stand-in))` for the other side's data, `Some(None)` for the relay's
    /// own answer; `None`: not a relay's.
    fn unwrap_hosted(&self, bytes: &[u8], from: SocketAddr) -> Option<Option<(Vec<u8>, SocketAddr)>> {
        let session = hosted::peek_session(bytes)?;
        let mut hs = self.hosted.borrow_mut();
        let h = hs.iter_mut().find(|h| h.client.session() == session && h.client.relay() == from)?;
        match h.client.on_datagram(bytes) {
            Received::Data(p) => Some(Some((p.to_vec(), h.standin))),
            Received::Control => Some(None),
            Received::NotMine => None,
        }
    }
}

/// The IP and UDP header bytes of a packet to or from `addr`, for the traffic lines (a
/// hosted relay's stand-in: its wrapping too).
pub fn overhead(addr: SocketAddr) -> u64 {
    if is_hosted_standin(addr) {
        return UDP_OVERHEAD_V4 + hosted::HEADER as u64;
    }
    match canonical(addr) {
        SocketAddr::V6(_) => UDP_OVERHEAD_V6,
        _ => UDP_OVERHEAD_V4,
    }
}

/// An IPv4-mapped IPv6 address as the IPv4 address it holds; anything else as it is.
pub fn canonical(addr: SocketAddr) -> SocketAddr {
    match addr {
        SocketAddr::V6(a) => match a.ip().to_ipv4_mapped() {
            Some(v4) => SocketAddr::new(IpAddr::V4(v4), a.port()),
            None => addr,
        },
        v4 => v4,
    }
}

fn bind_v4(ip: Ipv4Addr, port: u16) -> io::Result<UdpSocket> {
    let s = UdpSocket::bind((ip, port))?;
    s.set_nonblocking(true)?;
    Ok(s)
}

/// An IPv6-only UDP socket, so it never holds the IPv4 port too.
fn bind_v6(ip: Ipv6Addr, port: u16) -> io::Result<UdpSocket> {
    let s = Socket::new(Domain::IPV6, Type::DGRAM, Some(Protocol::UDP))?;
    s.set_only_v6(true)?;
    s.bind(&SocketAddr::new(IpAddr::V6(ip), port).into())?;
    s.set_nonblocking(true)?;
    Ok(s.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    fn drain(t: &Transport, want: usize) -> Vec<(Vec<u8>, SocketAddr)> {
        let mut got = Vec::new();
        let until = Instant::now() + Duration::from_secs(2);
        while got.len() < want && Instant::now() < until {
            t.recv(|b, from| got.push((b.to_vec(), from)));
            std::thread::sleep(Duration::from_millis(5));
        }
        got
    }

    fn has_v6_loopback() -> bool {
        bind_v6(Ipv6Addr::LOCALHOST, 0).is_ok()
    }

    #[test]
    fn mapped_addresses_are_taken_as_ipv4() {
        let mapped: SocketAddr = "[::ffff:192.168.1.5]:47960".parse().unwrap();
        assert_eq!(canonical(mapped), "192.168.1.5:47960".parse().unwrap());
        let v6: SocketAddr = "[2001:db8::5]:47960".parse().unwrap();
        assert_eq!(canonical(v6), v6);
        assert_eq!(overhead(v6), UDP_OVERHEAD_V6);
        assert_eq!(overhead("10.0.0.1:1".parse().unwrap()), UDP_OVERHEAD_V4);
    }

    #[test]
    fn loopback_host_is_local_only() {
        let t = Transport::host(0, Scope::Loopback).unwrap();
        assert!(t.local_only());
        assert!(t.local_addr().unwrap().ip().is_loopback());
        assert_eq!(t.local_addr_v6(), None);
    }

    #[test]
    fn all_networks_host_takes_one_port_on_both_families() {
        let t = Transport::host(0, Scope::All).unwrap();
        assert!(!t.local_only());
        let port = t.local_addr().unwrap().port();
        if let Some(v6) = t.local_addr_v6() {
            assert_eq!(v6.port(), port, "IPv6 listens on the IPv4 port");
        }
    }

    #[test]
    fn guest_reaches_host_over_ipv4() {
        let host = Transport::host(0, Scope::Loopback).unwrap();
        let at = host.local_addr().unwrap();
        let guest = Transport::guest(&[at]).unwrap();
        assert!(guest.local_only(), "a host on this PC asks nothing of the firewall");
        guest.send(b"hello", at);
        let got = drain(&host, 1);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].0, b"hello");
        host.send(b"welcome", got[0].1);
        assert_eq!(drain(&guest, 1)[0].0, b"welcome");
    }

    #[test]
    fn guest_reaches_host_over_ipv6() {
        if !has_v6_loopback() {
            eprintln!("no IPv6 loopback on this machine: skipped");
            return;
        }
        let host = Transport::host(0, Scope::All).unwrap();
        let Some(v6) = host.local_addr_v6() else {
            eprintln!("host could not bind IPv6: skipped");
            return;
        };
        let at = SocketAddr::new(IpAddr::V6(Ipv6Addr::LOCALHOST), v6.port());
        let guest = Transport::guest(&[at]).unwrap();
        assert!(guest.v4.is_none(), "an IPv6-only host needs no IPv4 socket");
        guest.send(b"hello6", at);
        let got = drain(&host, 1);
        assert_eq!(got.len(), 1);
        assert!(got[0].1.is_ipv6());
        host.send(b"welcome6", got[0].1);
        assert_eq!(drain(&guest, 1)[0].0, b"welcome6");
    }

    #[test]
    fn rebind_keeps_the_port() {
        let mut t = Transport::host(0, Scope::Loopback).unwrap();
        let port = t.local_addr().unwrap().port();
        t.rebind(Scope::All).unwrap();
        assert!(!t.local_only());
        assert_eq!(t.local_addr().unwrap().port(), port);
        t.rebind(Scope::Loopback).unwrap();
        assert!(t.local_only());
        assert_eq!(t.local_addr().unwrap().port(), port);
    }

    #[test]
    fn a_packet_with_no_way_is_dropped() {
        let t = Transport::host(0, Scope::Loopback).unwrap();
        // No IPv6 socket: nothing happens, nothing panics.
        t.send(b"x", "[2001:db8::1]:1".parse().unwrap());
    }
}
