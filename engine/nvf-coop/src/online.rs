//! Joining by an online code (docs/design/coop-networking.md §5-§7, phase 3): the matchmaking
//! service (`coop.nv-forever.com`, [`crate::signal_io`]) introduces host and guest, STUN tells
//! each its public address, and both probe every pair of addresses at once (hole punching,
//! `nvf_nat::ice`). When a pair answers, the guest's Noise handshake ([`crate::noise`]) goes
//! there, and from then on the game's own link ([`crate::net`]) runs as on a LAN. No game
//! traffic goes through the service.
//!
//! [`Online`] is driven from the network thread's tick ([`crate::net::Session::tick`]): it
//! takes the service's frames, STUN answers and probes, and gives packets to send from the
//! game's own sockets (STUN asks and probes must leave from them, so the NAT's mappings are
//! the game's).
//!
//! **Probes** are packets of their own, under the co-op magic and version:
//! `0xE5` probe and `0xE6` answer, each with the guest's number in the lobby (u32) and a
//! check id (u64). They are not authenticated: a forged answer at most sends the guest's
//! handshake to a wrong address, and the guest takes a session only from the host key the
//! service gave it ([`Online::host_key`]). A game answers probes only for a guest it is
//! checking with, so it reflects nothing for strangers.
//!
//! What is not here yet (phase 4): router port mapping for every player (PCP, NAT-PMP,
//! UPnP), a relay through another guest, join approval, and the reasons on the failure
//! screens beyond one line.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::Instant;

use nvf_nat::candidate::{is_lan, Candidate, Kind};
use nvf_nat::ice::{Action, Agent, Config, Role};
use nvf_nat::stun::{self, Gather, Mapping};
use nvf_signal::client::{GuestClient, GuestEvent, HostClient, HostEvent};
use nvf_signal::code::Code;
use nvf_signal::wire::{key_hex, CreateLobby, PeerNo, WIRE_VERSION};

use crate::portmap::{MapEvent, PortMap};
use crate::signal_io::{self, FromIo, SignalIo};

pub const KIND_PROBE: u8 = 0xE5;
pub const KIND_ANSWER: u8 = 0xE6;

/// What the game hears from [`Online`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum OnlineEvent {
    /// The host's lobby code, short form (`K7Q-M2X`), and the secret that keeps it across a
    /// restart.
    Code { code: String, secret: String },
    /// The guest's path to the host: its Hello goes there.
    HostAt(SocketAddr),
    /// What went wrong, for the player.
    Failed(String),
    /// A line for the log.
    Note(String),
}

/// What a host sends the service when it makes its lobby.
#[derive(Clone, Debug)]
pub struct HostInfo {
    pub name: String,
    pub load_order_hash: String,
}

struct Peer {
    key: [u8; 32],
    remotes: Vec<Candidate>,
    agent: Option<Agent>,
}

enum Side {
    Host {
        client: Option<HostClient>,
        peers: HashMap<PeerNo, Peer>,
        secret: Option<String>,
        /// The lobby's code, compact, once the service said it.
        code: Option<String>,
    },
    Guest {
        name: String,
        client: Option<GuestClient>,
        me: Option<PeerNo>,
        host_key: Option<[u8; 32]>,
        remotes: Vec<Candidate>,
        agent: Option<Box<Agent>>,
        load_order_hash: String,
        connected: bool,
    },
}

/// A game's side of the matchmaking service and the checks it starts.
pub struct Online {
    service: String,
    io: Option<SignalIo>,
    side: Side,
    /// This game's own addresses, each with its socket's port: the host candidates, and the
    /// agents' local sockets.
    locals: Vec<SocketAddr>,
    gather: Option<Gather>,
    gathered: bool,
    found: Vec<Candidate>,
    events: Vec<OnlineEvent>,
    /// The socket closed while hosting: when to open it again.
    reopen_at: Option<Instant>,
    /// This player's router asked to open the IPv4 socket's port ([`crate::portmap`]).
    portmap: Option<PortMap>,
}

impl Online {
    /// A host making its lobby, for the sockets at `locals`.
    pub fn host(service: &str, info: HostInfo, key: &[u8; 32], locals: Vec<SocketAddr>) -> Self {
        let body = CreateLobby {
            wire: WIRE_VERSION,
            protocol: crate::net::VERSION as u32,
            build: env!("CARGO_PKG_VERSION").into(),
            load_order_hash: info.load_order_hash.clone(),
            max_players: (crate::net::MAX_GUESTS + 1) as u8,
            host_key: key_hex(key),
            name: info.name.clone(),
        };
        Self::new(service, Some(SignalIo::create(service, body)), Side::Host { client: None, peers: HashMap::new(), secret: None, code: None }, locals)
    }

    /// A host taking its lobby back after a restart: the same code and secret.
    pub fn host_again(service: &str, code: &str, secret: &str, locals: Vec<SocketAddr>) -> Self {
        let mut o = Self::new(service, Some(SignalIo::resume(service, code)), Side::Host { client: None, peers: HashMap::new(), secret: Some(secret.into()), code: Some(code.replace('-', "")) }, locals);
        o.start_stun(&default_stun(), Instant::now());
        o
    }

    /// A guest joining `code`.
    pub fn guest(service: &str, code: Code, name: &str, load_order_hash: &str, locals: Vec<SocketAddr>) -> Self {
        let side = Side::Guest { name: name.into(), client: None, me: None, host_key: None, remotes: Vec::new(), agent: None, load_order_hash: load_order_hash.into(), connected: false };
        let mut o = Self::new(service, Some(SignalIo::lookup(service, code)), side, locals);
        o.start_stun(&default_stun(), Instant::now());
        o
    }

    fn new(service: &str, io: Option<SignalIo>, side: Side, locals: Vec<SocketAddr>) -> Self {
        let found = locals.iter().map(|a| Candidate::new(Kind::Host, *a)).collect();
        let portmap = locals.iter().find(|a| a.is_ipv4()).and_then(|a| PortMap::start(*a, Instant::now()));
        Self { service: service.into(), io, side, locals, gather: None, gathered: false, found, events: Vec::new(), reopen_at: None, portmap }
    }

    /// The host key the service gave a guest: the only key its Noise session may have.
    pub fn host_key(&self) -> Option<[u8; 32]> {
        match &self.side {
            Side::Guest { host_key, .. } => *host_key,
            Side::Host { .. } => None,
        }
    }

    pub fn take_events(&mut self) -> Vec<OnlineEvent> {
        std::mem::take(&mut self.events)
    }

    fn start_stun(&mut self, servers: &[SocketAddr], now: Instant) {
        if self.gather.is_some() || servers.is_empty() {
            return;
        }
        let list: Vec<(SocketAddr, stun::TxId)> = servers.iter().map(|s| (*s, txid())).collect();
        self.gather = Some(Gather::new(&list, now).with_software(concat!("NVForever ", env!("CARGO_PKG_VERSION"))));
    }

    /// A candidate this game found: kept, and given to the service.
    fn found(&mut self, c: Candidate) {
        if self.found.contains(&c) {
            return;
        }
        self.found.push(c);
        match &mut self.side {
            Side::Host { client: Some(cl), .. } => cl.add_local(c),
            Side::Guest { client: Some(cl), .. } => cl.add_local(c),
            _ => {}
        }
    }

    /// The service's socket, STUN and the checks, once a tick: the packets to send from the
    /// game's sockets.
    pub fn tick(&mut self, now: Instant, my_key: &[u8; 32]) -> Vec<(SocketAddr, Vec<u8>)> {
        let mut send = Vec::new();
        if self.reopen_at.is_some_and(|t| now >= t) {
            self.reopen_at = None;
            if let Side::Host { secret: Some(_), code: Some(code), .. } = &self.side {
                self.io = Some(SignalIo::resume(&self.service, code));
            }
        }
        let came = self.io.as_ref().map(SignalIo::take).unwrap_or_default();
        for m in came {
            self.take_io(m, now, my_key);
        }
        if let Some(g) = &mut self.gather {
            send.extend(g.poll(now));
            if !self.gathered && g.done(now) {
                self.gathered = true;
                let r = g.result();
                let mapping = match r.mapping {
                    Mapping::EndpointIndependent => "the same public port for every server (hole punching works)",
                    Mapping::EndpointDependent => "a new public port per server (a hard NAT: joins may fail)",
                    Mapping::Unknown => "unknown (fewer than two servers answered)",
                };
                self.events.push(OnlineEvent::Note(format!("STUN: public address {:?}; mapping {mapping}", r.reflexive())));
                for a in r.reflexive() {
                    self.found(Candidate::new(Kind::ServerReflexive, a));
                }
            }
        }
        if let Some(pm) = &mut self.portmap {
            send.extend(pm.tick(now));
            for e in pm.take_events() {
                match e {
                    MapEvent::Mapped(at) => self.found(Candidate::new(Kind::Mapped, at)),
                    MapEvent::Note(n) => self.events.push(OnlineEvent::Note(n)),
                }
            }
        }
        send.extend(self.poll_agents(now));
        self.flush();
        send
    }

    /// A datagram from `from` that may be the router's answer about the port: whether it was.
    pub fn on_router(&mut self, from: SocketAddr, bytes: &[u8], now: Instant) -> bool {
        self.portmap.as_mut().is_some_and(|pm| pm.on_packet(from, bytes, now))
    }

    fn flush(&mut self) {
        let out = match &mut self.side {
            Side::Host { client: Some(c), .. } => c.take_outgoing(),
            Side::Guest { client: Some(c), .. } => c.take_outgoing(),
            _ => Vec::new(),
        };
        if let Some(io) = &self.io {
            for t in out {
                io.send(t);
            }
        }
    }

    fn take_io(&mut self, m: FromIo, now: Instant, my_key: &[u8; 32]) {
        match m {
            FromIo::Created { code, host_secret, stun } => {
                if let Side::Host { secret, .. } = &mut self.side {
                    *secret = Some(host_secret);
                }
                let servers = if stun.is_empty() { default_stun() } else { stun };
                self.start_stun(&servers, now);
                self.events.push(OnlineEvent::Note(format!("the service made lobby {code}")));
            }
            FromIo::LookedUp(info) => {
                if let Side::Guest { load_order_hash, .. } = &self.side {
                    if info.protocol != crate::net::VERSION as u32 {
                        return self.fail(format!("The host runs another version of NVForever ({}): update both games to the same one.", info.build));
                    }
                    if !info.load_order_hash.is_empty() && !load_order_hash.is_empty() && info.load_order_hash != *load_order_hash {
                        return self.fail("The host runs a different load order: both games need the same plugins in the same order.".into());
                    }
                    if info.full {
                        self.fail("The host's lobby is full.".into());
                    }
                }
            }
            FromIo::Open => {
                let found = self.found.clone();
                match &mut self.side {
                    Side::Host { client, secret: Some(s), .. } => match client {
                        // Reopened after a drop: the same lobby.
                        Some(c) => c.hello(),
                        None => {
                            let mut c = HostClient::new(s);
                            for f in &found {
                                c.add_local(*f);
                            }
                            *client = Some(c);
                        }
                    },
                    Side::Guest { name, client, .. } => {
                        *client = Some(GuestClient::new(name, &key_hex(my_key), &found));
                    }
                    Side::Host { secret: None, .. } => {}
                }
            }
            FromIo::Text(t) => self.on_text(&t, now),
            FromIo::Failed(why) => {
                if matches!(self.side, Side::Host { code: Some(_), .. }) {
                    // The lobby is still there for 120 s: try again soon.
                    self.reopen_at = Some(now + std::time::Duration::from_secs(3));
                    self.events.push(OnlineEvent::Note(format!("the service: {why}; trying again")));
                } else {
                    self.fail(why);
                }
            }
            FromIo::Closed => {
                self.io = None;
                match &self.side {
                    Side::Host { code: Some(_), .. } => self.reopen_at = Some(now + std::time::Duration::from_secs(2)),
                    // Once on the host's link the guest needs the service only to come back.
                    Side::Guest { connected: true, .. } => {}
                    _ => self.fail("The matchmaking service closed the connection.".into()),
                }
            }
        }
    }

    fn fail(&mut self, why: String) {
        self.events.push(OnlineEvent::Failed(why));
        self.io = None;
    }

    fn on_text(&mut self, t: &str, now: Instant) {
        let locals = self.locals.clone();
        match &mut self.side {
            Side::Host { client: Some(c), peers, secret, code: kept, .. } => {
                let Ok(evs) = c.on_text(t) else { return };
                for e in evs {
                    match e {
                        HostEvent::Hosting { code, .. } => {
                            *kept = Some(code.clone());
                            let short = format!("{}-{}", &code[..3.min(code.len())], &code[3.min(code.len())..]);
                            self.events.push(OnlineEvent::Code { code: short, secret: secret.clone().unwrap_or_default() });
                        }
                        HostEvent::JoinRequest { peer, name, key, candidates } => {
                            let Some(key) = unhex32(&key) else {
                                c.decline(peer, "bad_key");
                                continue;
                            };
                            // Join approval is phase 4: every guest is let in, and the game's own
                            // link still turns a full lobby away.
                            c.accept(peer);
                            self.events.push(OnlineEvent::Note(format!("{name} (guest {peer}) asks to join: let in")));
                            peers.insert(peer, Peer { key, remotes: candidates, agent: None });
                        }
                        HostEvent::Candidates { peer, candidates } => {
                            if let Some(p) = peers.get_mut(&peer) {
                                for c in candidates {
                                    if !p.remotes.contains(&c) {
                                        p.remotes.push(c);
                                    }
                                    if let Some(a) = &mut p.agent {
                                        a.add_remote(c);
                                    }
                                }
                            }
                        }
                        HostEvent::Go { peer } | HostEvent::Reconnect { peer } => {
                            if let Some(p) = peers.get_mut(&peer) {
                                let mut a = Agent::new(Role::Host, &locals, Config::default());
                                for c in &p.remotes {
                                    a.add_remote(*c);
                                }
                                if matches!(e, HostEvent::Go { .. }) {
                                    a.start(now);
                                }
                                p.agent = Some(a);
                            }
                        }
                        HostEvent::Connected { peer, path } => self.events.push(OnlineEvent::Note(format!("guest {peer}'s path: {path}"))),
                        HostEvent::Left { peer } => {
                            peers.remove(&peer);
                        }
                        HostEvent::Closed { reason } => self.events.push(OnlineEvent::Failed(format!("The matchmaking service closed the lobby ({reason})."))),
                        HostEvent::Error { error, message } => self.events.push(OnlineEvent::Note(format!("the service: {error}: {message}"))),
                    }
                }
            }
            Side::Guest { client: Some(c), me, host_key, remotes, agent, .. } => {
                let Ok(evs) = c.on_text(t) else { return };
                for e in evs {
                    match e {
                        GuestEvent::Waiting { peer } => {
                            *me = Some(peer);
                            self.events.push(OnlineEvent::Note(format!("waiting for the host to let this game in (guest {peer})")));
                        }
                        GuestEvent::Accepted { host_key: k, host_name, candidates } => {
                            *host_key = unhex32(&k);
                            self.events.push(OnlineEvent::Note(format!("{host_name}'s game let this one in")));
                            for c in candidates {
                                if !remotes.contains(&c) {
                                    remotes.push(c);
                                }
                            }
                        }
                        GuestEvent::Candidates { candidates } => {
                            for c in candidates {
                                if !remotes.contains(&c) {
                                    remotes.push(c);
                                }
                                if let Some(a) = agent {
                                    a.add_remote(c);
                                }
                            }
                        }
                        GuestEvent::Go => {
                            let mut a = Agent::new(Role::Guest, &locals, Config::default());
                            for c in remotes.iter() {
                                a.add_remote(*c);
                            }
                            a.start(now);
                            *agent = Some(Box::new(a));
                        }
                        GuestEvent::Declined { reason } => {
                            self.events.push(OnlineEvent::Failed(format!("The host's game turned this one away ({reason}).")));
                        }
                        GuestEvent::Closed { reason } => {
                            self.events.push(OnlineEvent::Failed(format!("The lobby closed ({reason}).")));
                        }
                        GuestEvent::Error { error, message } => {
                            let why = match error.as_str() {
                                "full" => "The host's lobby is full.".to_string(),
                                "host_away" => "The host's game is reconnecting: try again in a moment.".to_string(),
                                _ => format!("The matchmaking service refused ({error}: {message})."),
                            };
                            self.events.push(OnlineEvent::Failed(why));
                        }
                        // The peer relay is phase 4.
                        GuestEvent::Helpers { .. } | GuestEvent::RelayAsk { .. } | GuestEvent::RelayAnswer { .. } => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn poll_agents(&mut self, now: Instant) -> Vec<(SocketAddr, Vec<u8>)> {
        let mut send = Vec::new();
        match &mut self.side {
            Side::Host { peers, .. } => {
                for (&peer, p) in peers.iter_mut() {
                    let Some(a) = &mut p.agent else { continue };
                    for act in a.poll(now) {
                        if let Action::SendProbe { to, check_id, .. } = act {
                            send.push((to, probe(KIND_PROBE, peer, check_id)));
                        }
                        // The guest names the path, and its handshake comes over it; a host
                        // agent that finds none just stops.
                    }
                }
            }
            Side::Guest { me: Some(me), agent, client, connected, .. } => {
                let mut done = false;
                if let Some(a) = agent {
                    for act in a.poll(now) {
                        match act {
                            Action::SendProbe { to, check_id, .. } => send.push((to, probe(KIND_PROBE, *me, check_id))),
                            Action::Selected { remote, .. } => {
                                let path = if is_lan(remote.ip()) { "lan" } else { "direct" };
                                if let Some(c) = client {
                                    c.connected(path);
                                }
                                *connected = true;
                                self.events.push(OnlineEvent::Note(format!("path to the host: {remote} ({path})")));
                                self.events.push(OnlineEvent::HostAt(remote));
                                done = true;
                            }
                            Action::Failed => {
                                self.events.push(OnlineEvent::Failed("Couldn't reach the host from your network. Ask the host to turn on UPnP on their router or forward UDP port 47960, or join over Tailscale by address.".into()));
                                done = true;
                            }
                            Action::Keepalive { .. } | Action::PathLost => {}
                        }
                    }
                }
                if done {
                    *agent = None;
                }
            }
            Side::Guest { .. } => {}
        }
        send
    }

    /// A guest whose host went quiet: ask the service to start the checks again.
    pub fn path_lost(&mut self) {
        if let Side::Guest { client: Some(c), connected, .. } = &mut self.side {
            if std::mem::take(connected) {
                c.reconnect();
                self.flush();
            }
        }
    }

    /// A Noise session was made with `addr`, whose key is `key`: the host stops probing for
    /// that guest.
    pub fn session_made(&mut self, key: &[u8; 32]) {
        if let Side::Host { peers, .. } = &mut self.side {
            for p in peers.values_mut().filter(|p| p.key == *key) {
                p.agent = None;
            }
        }
    }

    /// A STUN answer that came to the game's socket.
    pub fn on_stun(&mut self, from: SocketAddr, bytes: &[u8]) {
        if let Some(g) = &mut self.gather {
            g.on_packet(from, bytes);
        }
    }

    /// A probe or an answer from `from`: the answer to send back, if any.
    pub fn on_probe(&mut self, bytes: &[u8], from: SocketAddr, now: Instant) -> Option<(SocketAddr, Vec<u8>)> {
        let (kind, peer, check_id) = parse_probe(bytes)?;
        let local = self.local_for(from);
        let agent = match &mut self.side {
            Side::Host { peers, .. } => peers.get_mut(&peer).and_then(|p| p.agent.as_mut()),
            Side::Guest { me: Some(me), agent, .. } if *me == peer => agent.as_deref_mut(),
            Side::Guest { .. } => None,
        };
        // Probes for a check this game is not running are not answered.
        let agent = agent?;
        match kind {
            KIND_PROBE => {
                agent.on_probe_received(local, from, now);
                Some((from, probe(KIND_ANSWER, peer, check_id)))
            }
            _ => {
                agent.on_probe_result(check_id, local, from, now);
                None
            }
        }
    }

    fn local_for(&self, from: SocketAddr) -> SocketAddr {
        self.locals.iter().find(|l| l.is_ipv4() == from.is_ipv4()).copied().unwrap_or(from)
    }

    /// Leaving: the service is told (a host's lobby ends) and the router's port is closed:
    /// the packet that closes it, to send from the game's IPv4 socket.
    pub fn bye(&mut self) -> Option<(SocketAddr, Vec<u8>)> {
        match &mut self.side {
            Side::Host { client: Some(c), .. } => c.bye(),
            Side::Guest { client: Some(c), .. } => c.bye(),
            _ => {}
        }
        self.flush();
        self.portmap.take().and_then(|mut pm| pm.release())
    }
}

fn probe(kind: u8, peer: PeerNo, check_id: u64) -> Vec<u8> {
    let mut b = Vec::with_capacity(18);
    b.extend_from_slice(&crate::net::MAGIC);
    b.push(crate::net::VERSION);
    b.push(kind);
    b.extend_from_slice(&peer.to_le_bytes());
    b.extend_from_slice(&check_id.to_le_bytes());
    b
}

/// A probe or an answer: kind, the guest's number, the check id.
pub fn parse_probe(b: &[u8]) -> Option<(u8, PeerNo, u64)> {
    if b.len() != 18 || b[..4] != crate::net::MAGIC || b[4] != crate::net::VERSION || !matches!(b[5], KIND_PROBE | KIND_ANSWER) {
        return None;
    }
    Some((b[5], u32::from_le_bytes(b[6..10].try_into().ok()?), u64::from_le_bytes(b[10..18].try_into().ok()?)))
}

pub fn is_probe(b: &[u8]) -> bool {
    parse_probe(b).is_some()
}

fn unhex32(s: &str) -> Option<[u8; 32]> {
    if s.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, o) in out.iter_mut().enumerate() {
        *o = u8::from_str_radix(s.get(i * 2..i * 2 + 2)?, 16).ok()?;
    }
    Some(out)
}

fn txid() -> stun::TxId {
    let a = crate::net::new_token().to_le_bytes();
    let b = crate::net::new_token().to_le_bytes();
    let mut t = [0u8; 12];
    t[..8].copy_from_slice(&a);
    t[8..].copy_from_slice(&b[..4]);
    t
}

fn default_stun() -> Vec<SocketAddr> {
    signal_io::resolve_stun(&signal_io::DEFAULT_STUN.map(String::from))
}

/// This PC's addresses the other side may reach directly, with these ports: its LAN IPv4
/// address and its global IPv6 address, each found by asking the system which address it
/// would send from (nothing is sent).
pub fn local_addrs(v4_port: Option<u16>, v6_port: Option<u16>) -> Vec<SocketAddr> {
    let mut out = Vec::new();
    if let Some(p) = v4_port {
        let ip = UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0)).and_then(|s| s.connect(("192.0.2.1", 9)).and(s.local_addr())).ok().map(|a| a.ip());
        if let Some(ip @ IpAddr::V4(v4)) = ip {
            if !v4.is_unspecified() && !v4.is_loopback() {
                out.push(SocketAddr::new(ip, p));
            }
        }
    }
    if let Some(p) = v6_port {
        let ip = UdpSocket::bind((Ipv6Addr::UNSPECIFIED, 0)).and_then(|s| s.connect(("2001:db8::1", 9)).and(s.local_addr())).ok().map(|a| a.ip());
        if let Some(ip @ IpAddr::V6(v6)) = ip {
            if nvf_nat::candidate::is_global_v6(v6) {
                out.push(SocketAddr::new(ip, p));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probes_round_trip_and_nothing_else_is_one() {
        let p = probe(KIND_PROBE, 7, 0xDEAD_BEEF);
        assert_eq!(parse_probe(&p), Some((KIND_PROBE, 7, 0xDEAD_BEEF)));
        assert_eq!(parse_probe(&probe(KIND_ANSWER, 1, 2)), Some((KIND_ANSWER, 1, 2)));
        assert!(!is_probe(&p[..17]));
        assert!(!is_probe(b"NVCO\x8a\xe0................"));
        assert!(!stun::is_stun(&p), "a probe is never taken for STUN");
    }

    #[test]
    fn a_probe_for_no_check_is_not_answered() {
        let mut o = Online::new("http://127.0.0.1:1", None, Side::Host { client: None, peers: HashMap::new(), secret: None, code: None }, vec!["192.168.1.2:47960".parse().unwrap()]);
        assert_eq!(o.on_probe(&probe(KIND_PROBE, 3, 9), "203.0.113.9:5000".parse().unwrap(), Instant::now()), None);
    }

    #[test]
    fn a_host_checks_with_a_guest_it_let_in() {
        let mut o = Online::new("http://127.0.0.1:1", None, Side::Host { client: Some(HostClient::new("s")), peers: HashMap::new(), secret: Some("s".into()), code: None }, vec!["192.168.1.2:47960".parse().unwrap()]);
        let now = Instant::now();
        let key = "ab".repeat(32);
        o.on_text(&format!(r#"{{"type":"join","peer":1,"name":"G","key":"{key}","candidates":[{{"kind":"host","addr":"192.168.1.3:50000"}}]}}"#), now);
        o.on_text(r#"{"type":"go","peer":1}"#, now);
        let sent = o.tick(now, &[0; 32]);
        assert!(sent.iter().any(|(to, b)| to.to_string() == "192.168.1.3:50000" && parse_probe(b).is_some_and(|(k, p, _)| k == KIND_PROBE && p == 1)), "{sent:?}");
        let from: SocketAddr = "192.168.1.3:50000".parse().unwrap();
        let answer = o.on_probe(&probe(KIND_PROBE, 1, 42), from, now);
        assert_eq!(answer, Some((from, probe(KIND_ANSWER, 1, 42))));
        o.session_made(&unhex32(&key).unwrap());
        assert_eq!(o.on_probe(&probe(KIND_PROBE, 1, 43), from, now), None, "checks over once the guest's session is made");
    }
}
