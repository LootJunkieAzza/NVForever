//! The sockets under the co-op wire (docs/design/coop-networking.md §3, phase 1): every
//! packet [`crate::net`] sends or takes goes through a [`Transport`], which picks the
//! way to the address it is given.
//!
//! - An IPv4 address goes over the IPv4 socket.
//! - An IPv6 address goes over the IPv6 socket. An IPv4-mapped IPv6 address
//!   (`::ffff:a.b.c.d`) is taken as the IPv4 address it holds.
//! - A Steam peer's stand-in address ([`crate::steam::addr`]) goes over Steam, when this
//!   game talks over Steam; without Steam it is dropped.
//!
//! A host listens on both families on the same port: IPv4 must bind, IPv6 is added when the
//! PC has it. The IPv6 socket is IPv6-only, so it never takes the IPv4 port from the
//! other socket (Linux binds `[::]` dual-stack by default). A guest opens a socket of each
//! family its host's candidate addresses need. Nothing above this module knows how many
//! sockets there are.
//!
//! The sockets are non-blocking. [`Transport::recv`] drains everything waiting, on every
//! socket and Steam.

use std::io::{self, ErrorKind};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};

use socket2::{Domain, Protocol, Socket, Type};

use crate::steam::Steam;

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
    steam: Option<&'static Steam>,
}

impl Transport {
    /// A host's sockets on `port` (0 picks a free one; IPv6 then takes the same port when
    /// it is free there). IPv4 must bind, IPv6 is best effort.
    pub fn host(port: u16, scope: Scope) -> io::Result<Self> {
        match scope {
            Scope::Loopback => Ok(Self { v4: Some(bind_v4(Ipv4Addr::LOCALHOST, port)?), v6: None, steam: None }),
            Scope::All => {
                let v4 = bind_v4(Ipv4Addr::UNSPECIFIED, port)?;
                let port = v4.local_addr()?.port();
                let v6 = bind_v6(Ipv6Addr::UNSPECIFIED, port).ok();
                Ok(Self { v4: Some(v4), v6, steam: None })
            }
        }
    }

    /// A guest's sockets for reaching `candidates`, on free ports: loopback only when every
    /// candidate is this PC (no firewall prompt), else every address, with broadcast on the
    /// IPv4 socket for a host on the guest's own network. IPv6 is opened when a candidate
    /// is IPv6 (a Steam stand-in does not count) and is needed only then; IPv4 is always
    /// opened unless every candidate is IPv6.
    pub fn guest(candidates: &[SocketAddr]) -> io::Result<Self> {
        let real: Vec<SocketAddr> = candidates.iter().map(|c| canonical(*c)).filter(|c| crate::steam::peer(*c).is_none()).collect();
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
        Ok(Self { v4, v6, steam: None })
    }

    /// Talk over Steam too from now on, or not.
    pub fn set_steam(&mut self, steam: Option<&'static Steam>) {
        self.steam = steam;
    }

    pub fn steam(&self) -> Option<&'static Steam> {
        self.steam
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
    /// and the error returned. Steam is kept for [`Scope::All`] and dropped for
    /// [`Scope::Loopback`].
    pub fn rebind(&mut self, scope: Scope) -> io::Result<()> {
        let port = self.local_addr()?.port();
        let was = if self.local_only() { Scope::Loopback } else { Scope::All };
        self.v4 = None;
        self.v6 = None;
        match Self::host(port, scope) {
            Ok(t) => {
                self.v4 = t.v4;
                self.v6 = t.v6;
                if scope == Scope::Loopback {
                    self.steam = None;
                }
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

    /// Send `bytes` to `to` the way its address says. A packet with no way to go (no
    /// socket of that family, no Steam) is dropped, as UDP drops any.
    pub fn send(&self, bytes: &[u8], to: SocketAddr) {
        if let Some(id) = crate::steam::peer(to) {
            if let Some(s) = self.steam {
                s.send(id, bytes);
            }
            return;
        }
        let to = canonical(to);
        let sock = if to.is_ipv4() { &self.v4 } else { &self.v6 };
        if let Some(s) = sock {
            let _ = s.send_to(bytes, to);
        }
    }

    /// Everything waiting, on every socket and Steam, each packet handed to `take` with
    /// the address it came from (an IPv4-mapped IPv6 address as its IPv4 one).
    pub fn recv(&self, mut take: impl FnMut(&[u8], SocketAddr)) {
        let mut buf = [0u8; RECV_MAX];
        for sock in self.v4.iter().chain(self.v6.iter()) {
            loop {
                match sock.recv_from(&mut buf) {
                    Ok((n, from)) => take(&buf[..n], canonical(from)),
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    // Windows reports an ICMP "port unreachable" from an earlier send as an
                    // error on a later receive (WSAECONNRESET); it says nothing about any
                    // packet.
                    Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                    Err(_) => break,
                }
            }
        }
        if let Some(s) = self.steam {
            let mut got = Vec::new();
            s.receive(&mut got);
            for (bytes, from) in got {
                take(&bytes, from);
            }
        }
    }
}

/// The IP and UDP header bytes of a packet to or from `addr`, for the traffic lines.
pub fn overhead(addr: SocketAddr) -> u64 {
    match canonical(addr) {
        SocketAddr::V6(a) if crate::steam::peer(SocketAddr::V6(a)).is_none() => UDP_OVERHEAD_V6,
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
        // No IPv6 socket, no Steam: nothing happens, nothing panics.
        t.send(b"x", "[2001:db8::1]:1".parse().unwrap());
        t.send(b"x", crate::steam::addr(crate::steam::steam_id(1)));
    }
}
