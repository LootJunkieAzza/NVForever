//! The lobby protocol end to end: clients and the stand-in service, and a whole join through
//! simulated NATs (#3795 section 12, "Simulated NAT" and "Service").

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::{Duration, Instant};

use nvf_nat::candidate::{Candidate, Kind};
use nvf_nat::ice::{Action, Agent, Config as IceConfig, Role};
use nvf_nat::natsim::{NatKind, Sim};
use nvf_nat::stun::Gather;
use nvf_signal::client::{GuestClient, GuestEvent, HostClient, HostEvent};
use nvf_signal::standin::{Config, ConnId, StandIn};
use nvf_signal::wire::{key_hex, CreateLobby, LobbyCreated, LobbyInfo, WIRE_VERSION};

const PORT: u16 = 47960;

fn key(n: u8) -> String {
    key_hex(&[n; 32])
}

fn create(si: &mut StandIn, max: u8, now: Instant) -> LobbyCreated {
    let req = CreateLobby {
        wire: WIRE_VERSION,
        protocol: 138,
        build: "0.8.1".into(),
        load_order_hash: "abc123".into(),
        max_players: max,
        host_key: key(1),
        name: "Courier".into(),
    };
    let (status, body) = si.create(&serde_json::to_string(&req).unwrap(), now, 1_000);
    assert_eq!(status, 200, "{body}");
    serde_json::from_str(&body).unwrap()
}

/// Frames waiting for each socket, and the sockets the service closed.
#[derive(Default)]
struct Wires {
    inbox: HashMap<ConnId, Vec<String>>,
    closed: Vec<ConnId>,
}

impl Wires {
    fn send(&mut self, si: &mut StandIn, conn: ConnId, frames: Vec<String>, now: Instant) {
        for f in frames {
            for o in si.message(conn, &f, now) {
                self.inbox.entry(o.conn).or_default().push(o.text);
                if o.close {
                    self.closed.push(o.conn);
                }
            }
        }
    }
    fn take(&mut self, conn: ConnId) -> Vec<String> {
        self.inbox.remove(&conn).unwrap_or_default()
    }
}

fn host_events(h: &mut HostClient, w: &mut Wires, conn: ConnId) -> Vec<HostEvent> {
    w.take(conn).iter().flat_map(|f| h.on_text(f).unwrap()).collect()
}

fn guest_events(g: &mut GuestClient, w: &mut Wires, conn: ConnId) -> Vec<GuestEvent> {
    w.take(conn).iter().flat_map(|f| g.on_text(f).unwrap()).collect()
}

#[test]
fn info_and_gates() {
    let t0 = Instant::now();
    let mut si = StandIn::new(Config { min_protocol: 138, ..Config::default() });
    let lobby = create(&mut si, 6, t0);
    assert_eq!(lobby.code.len(), 6);
    assert_eq!(lobby.expires, 1_120);
    let (status, body) = si.info(&nvf_signal::code::Code::parse(&lobby.code).unwrap().to_string());
    assert_eq!(status, 200);
    let info: LobbyInfo = serde_json::from_str(&body).unwrap();
    assert_eq!((info.protocol, info.players, info.full, info.load_order_hash.as_str()), (138, 1, false, "abc123"));
    assert_eq!(si.info("ZZZ-ZZZ").0, 404);
    let old = r#"{"wire":1,"protocol":137,"build":"0.7","load_order_hash":"x","max_players":6,"host_key":"KEY","name":"n"}"#;
    assert_eq!(si.create(&old.replace("KEY", &key(2)), t0, 0).0, 403);
    assert_eq!(si.create("{}", t0, 0).0, 400);
}

#[test]
fn host_secret_checked() {
    let t0 = Instant::now();
    let mut si = StandIn::new(Config::default());
    let lobby = create(&mut si, 6, t0);
    let conn = si.open(&lobby.code).unwrap();
    let mut w = Wires::default();
    let mut h = HostClient::new("wrong");
    w.send(&mut si, conn, h.take_outgoing(), t0);
    let ev = host_events(&mut h, &mut w, conn);
    assert!(matches!(&ev[0], HostEvent::Error { error, .. } if error == "bad_secret"), "{ev:?}");
    assert_eq!(w.closed, vec![conn]);
}

#[test]
fn accept_decline_full_and_held_candidates() {
    let t0 = Instant::now();
    let mut si = StandIn::new(Config::default());
    let lobby = create(&mut si, 3, t0);
    let mut w = Wires::default();
    let hc = si.open(&lobby.code).unwrap();
    let mut h = HostClient::new(&lobby.host_secret);
    w.send(&mut si, hc, h.take_outgoing(), t0);
    assert!(matches!(&host_events(&mut h, &mut w, hc)[0], HostEvent::Hosting { code, .. } if *code == lobby.code));
    let hcand = Candidate::new(Kind::ServerReflexive, "203.0.113.1:40000".parse().unwrap());
    h.add_local(hcand);
    assert!(h.take_outgoing().is_empty(), "no guest to tell yet");

    // Guest A joins; a candidate found before the host answers is held.
    let ac = si.open(&lobby.code).unwrap();
    let a_lan = Candidate::new(Kind::Host, "192.168.2.2:47960".parse().unwrap());
    let mut a = GuestClient::new("A", &key(10), &[a_lan]);
    w.send(&mut si, ac, a.take_outgoing(), t0);
    assert_eq!(guest_events(&mut a, &mut w, ac), vec![GuestEvent::Waiting { peer: 1 }]);
    let ev = host_events(&mut h, &mut w, hc);
    assert_eq!(ev, vec![HostEvent::JoinRequest { peer: 1, name: "A".into(), key: key(10), candidates: vec![a_lan] }]);
    let a_srflx = Candidate::new(Kind::ServerReflexive, "203.0.113.2:40000".parse().unwrap());
    a.add_local(a_srflx);
    assert!(a.take_outgoing().is_empty(), "held until accepted");

    h.accept(1);
    w.send(&mut si, hc, h.take_outgoing(), t0);
    assert_eq!(host_events(&mut h, &mut w, hc), vec![HostEvent::Go { peer: 1 }]);
    let ev = guest_events(&mut a, &mut w, ac);
    assert_eq!(
        ev,
        vec![
            GuestEvent::Accepted { host_key: key(1), host_name: "Courier".into(), candidates: vec![hcand] },
            GuestEvent::Go,
            GuestEvent::Helpers { peers: vec![] }
        ]
    );
    // The held candidate goes now, and reaches the host.
    w.send(&mut si, ac, a.take_outgoing(), t0);
    assert_eq!(host_events(&mut h, &mut w, hc), vec![HostEvent::Candidates { peer: 1, candidates: vec![a_srflx] }]);
    // A host candidate found later reaches the accepted guest.
    let hmapped = Candidate::new(Kind::Mapped, "203.0.113.1:47960".parse().unwrap());
    h.add_local(hmapped);
    w.send(&mut si, hc, h.take_outgoing(), t0);
    assert_eq!(guest_events(&mut a, &mut w, ac), vec![GuestEvent::Candidates { candidates: vec![hmapped] }]);

    // Guest B is declined.
    let bc = si.open(&lobby.code).unwrap();
    let mut b = GuestClient::new("B", &key(11), &[]);
    w.send(&mut si, bc, b.take_outgoing(), t0);
    guest_events(&mut b, &mut w, bc);
    host_events(&mut h, &mut w, hc);
    h.decline(2, "Not now");
    w.send(&mut si, hc, h.take_outgoing(), t0);
    assert_eq!(guest_events(&mut b, &mut w, bc), vec![GuestEvent::Declined { reason: "Not now".into() }]);
    assert!(w.closed.contains(&bc));

    // C fills the lobby (host, A, C = 3); D finds it full.
    let cc = si.open(&lobby.code).unwrap();
    let mut c = GuestClient::new("C", &key(12), &[]);
    w.send(&mut si, cc, c.take_outgoing(), t0);
    assert_eq!(guest_events(&mut c, &mut w, cc), vec![GuestEvent::Waiting { peer: 3 }]);
    let info: LobbyInfo = serde_json::from_str(&si.info(&lobby.code).1).unwrap();
    assert!(info.full);
    let dc = si.open(&lobby.code).unwrap();
    let mut d = GuestClient::new("D", &key(13), &[]);
    w.send(&mut si, dc, d.take_outgoing(), t0);
    assert!(matches!(&guest_events(&mut d, &mut w, dc)[0], GuestEvent::Error { error, .. } if error == "full"));

    // A reports its path, then leaves.
    a.connected("direct");
    w.send(&mut si, ac, a.take_outgoing(), t0);
    let ev = host_events(&mut h, &mut w, hc);
    assert!(matches!(&ev[0], HostEvent::JoinRequest { peer: 3, .. }), "C's request: {ev:?}");
    assert_eq!(ev[1..], [HostEvent::Connected { peer: 1, path: "direct".into() }]);
    a.bye();
    w.send(&mut si, ac, a.take_outgoing(), t0);
    let ev = host_events(&mut h, &mut w, hc);
    assert!(ev.contains(&HostEvent::Left { peer: 1 }), "{ev:?}");
    assert!(w.closed.contains(&ac));

    // The host leaves: C is told and the code is free.
    h.bye();
    w.send(&mut si, hc, h.take_outgoing(), t0);
    assert_eq!(guest_events(&mut c, &mut w, cc), vec![GuestEvent::Closed { reason: "host_left".into() }]);
    assert_eq!(si.lobbies(), 0);
    assert_eq!(si.info(&lobby.code).0, 404);
}

#[test]
fn host_reconnects_within_grace_or_lobby_expires() {
    let t0 = Instant::now();
    let mut si = StandIn::new(Config::default());
    let lobby = create(&mut si, 6, t0);
    let mut w = Wires::default();
    let hc = si.open(&lobby.code).unwrap();
    let mut h = HostClient::new(&lobby.host_secret);
    w.send(&mut si, hc, h.take_outgoing(), t0);
    host_events(&mut h, &mut w, hc);
    let gc = si.open(&lobby.code).unwrap();
    let mut g = GuestClient::new("G", &key(5), &[]);
    w.send(&mut si, gc, g.take_outgoing(), t0);
    guest_events(&mut g, &mut w, gc);
    host_events(&mut h, &mut w, hc);

    // The host's socket drops; a new one within the grace finds its guest still there.
    si.close(hc, t0 + Duration::from_secs(1));
    assert!(si.tick(t0 + Duration::from_secs(100)).is_empty());
    let hc2 = si.open(&lobby.code).unwrap();
    h.hello();
    w.send(&mut si, hc2, h.take_outgoing(), t0 + Duration::from_secs(100));
    assert_eq!(host_events(&mut h, &mut w, hc2), vec![HostEvent::Hosting { code: lobby.code.clone(), peers: vec![1] }]);

    // It drops again and stays away: the lobby ends after 2 minutes.
    si.close(hc2, t0 + Duration::from_secs(200));
    assert!(si.tick(t0 + Duration::from_secs(319)).is_empty());
    let outs = si.tick(t0 + Duration::from_secs(320));
    assert!(outs.iter().any(|o| o.conn == gc && o.close && o.text.contains("expired")), "{outs:?}");
    assert_eq!(si.lobbies(), 0);

    // An unclaimed lobby (no host socket ever) expires too.
    let unclaimed = create(&mut si, 6, t0 + Duration::from_secs(400));
    assert_eq!(si.tick(t0 + Duration::from_secs(520)).len(), 0);
    assert_eq!(si.info(&unclaimed.code).0, 404);
}

fn gather(sim: &mut Sim, socket: SocketAddr, now: &mut Instant, seed: u8) -> Vec<Candidate> {
    let servers = sim.stun_servers().to_vec();
    let mut g = Gather::new(&[(servers[0], [seed; 12]), (servers[1], [seed + 1; 12])], *now);
    while !g.done(*now) {
        for (to, b) in g.poll(*now) {
            sim.send(socket, to, b);
        }
        for p in sim.deliver() {
            g.on_packet(p.from, &p.data);
        }
        *now += Duration::from_millis(10);
    }
    let mut c = vec![Candidate::new(Kind::Host, socket)];
    c.extend(g.result().reflexive().into_iter().map(|a| Candidate::new(Kind::ServerReflexive, a)));
    c
}

/// A whole join: the host creates a lobby, a guest behind a symmetric NAT joins by code, the
/// service passes keys and candidates, both start at `go`, and the checks find the path.
#[test]
fn join_by_code_through_nats() {
    let mut now = Instant::now();
    let mut sim = Sim::new();
    let (hl, gl) = (sim.add_lan(NatKind::Restricted), sim.add_lan(NatKind::Symmetric));
    let (hs, gs) = (sim.add_socket(hl, PORT), sim.add_socket(gl, PORT));

    let mut si = StandIn::new(Config::default());
    let lobby = create(&mut si, 6, now);
    let mut w = Wires::default();
    let hc = si.open(&lobby.code).unwrap();
    let mut host = HostClient::new(&lobby.host_secret);
    w.send(&mut si, hc, host.take_outgoing(), now);
    host_events(&mut host, &mut w, hc);
    for c in gather(&mut sim, hs, &mut now, 1) {
        host.add_local(c);
    }

    // The guest types the code as a player would.
    let typed = nvf_signal::code::Code::parse(&lobby.code).unwrap().to_string().to_lowercase();
    assert_eq!(si.info(&typed).0, 200);
    let gcands = gather(&mut sim, gs, &mut now, 3);
    let gc = si.open(&typed).unwrap();
    let mut guest = GuestClient::new("Six", &key(6), &gcands[..1]);
    w.send(&mut si, gc, guest.take_outgoing(), now);
    for c in &gcands[1..] {
        guest.add_local(*c);
    }

    let mut agents = [Agent::new(Role::Host, &[hs], IceConfig::default()), Agent::new(Role::Guest, &[gs], IceConfig::default())];
    let sockets = [hs, gs];
    let mut selected = [false, false];
    let mut host_heard_connected = false;
    let end = now + Duration::from_secs(12);
    while now < end && !(selected == [true, true] && host_heard_connected) {
        // Lobby traffic.
        for ev in host_events(&mut host, &mut w, hc) {
            match ev {
                HostEvent::JoinRequest { peer, key: k, candidates, .. } => {
                    assert_eq!(k, key(6));
                    candidates.into_iter().for_each(|c| agents[0].add_remote(c));
                    host.accept(peer);
                }
                HostEvent::Candidates { candidates, .. } => candidates.into_iter().for_each(|c| agents[0].add_remote(c)),
                HostEvent::Go { .. } => agents[0].start(now),
                HostEvent::Connected { path, .. } => {
                    assert_eq!(path, "direct");
                    host_heard_connected = true;
                }
                other => panic!("host: {other:?}"),
            }
        }
        w.send(&mut si, hc, host.take_outgoing(), now);
        for ev in guest_events(&mut guest, &mut w, gc) {
            match ev {
                GuestEvent::Waiting { .. } => {}
                GuestEvent::Accepted { host_key, candidates, .. } => {
                    assert_eq!(host_key, key(1));
                    candidates.into_iter().for_each(|c| agents[1].add_remote(c));
                }
                GuestEvent::Candidates { candidates } => candidates.into_iter().for_each(|c| agents[1].add_remote(c)),
                GuestEvent::Go => agents[1].start(now),
                GuestEvent::Helpers { .. } => {}
                other => panic!("guest: {other:?}"),
            }
        }
        w.send(&mut si, gc, guest.take_outgoing(), now);

        // Game traffic: probes, straight between the players through their NATs.
        for (i, a) in agents.iter_mut().enumerate() {
            for act in a.poll(now) {
                match act {
                    Action::SendProbe { from, to, check_id, .. } => {
                        let mut d = vec![b'P'];
                        d.extend_from_slice(&check_id.to_be_bytes());
                        sim.send(from, to, d);
                    }
                    Action::Selected { local, remote, .. } => {
                        selected[i] = true;
                        if i == 1 {
                            sim.send(local, remote, vec![b'N']);
                            guest.connected("direct");
                        }
                    }
                    Action::Failed => panic!("no path"),
                    _ => {}
                }
            }
        }
        for p in sim.deliver() {
            let Some(i) = sockets.iter().position(|&s| s == p.to) else { continue };
            match p.data[0] {
                b'P' => {
                    agents[i].on_probe_received(p.to, p.from, now);
                    let mut d = p.data.clone();
                    d[0] = b'R';
                    sim.send(p.to, p.from, d);
                }
                b'R' => {
                    agents[i].on_probe_result(u64::from_be_bytes(p.data[1..9].try_into().unwrap()), p.to, p.from, now);
                }
                b'N' => agents[i].on_peer_selected(p.to, p.from, now),
                _ => {}
            }
        }
        now += Duration::from_millis(10);
    }
    assert_eq!(selected, [true, true]);
    assert!(host_heard_connected);
    // The host talks to the guest's per-destination port, which only the checks could find.
    let (_, to_guest) = agents[0].selected().unwrap();
    assert_eq!(agents[0].selected_candidate().unwrap().kind, Kind::PeerReflexive);
    assert_eq!(to_guest.ip(), "203.0.113.2".parse::<std::net::IpAddr>().unwrap());
}

/// Section 8: guest C, with no path to the host, finds a helper and asks it through the
/// service, guest to guest.
#[test]
fn relay_exchange_between_guests() {
    let t0 = Instant::now();
    let mut si = StandIn::new(Config::default());
    let lobby = create(&mut si, 6, t0);
    let mut w = Wires::default();
    let hc = si.open(&lobby.code).unwrap();
    let mut h = HostClient::new(&lobby.host_secret);
    w.send(&mut si, hc, h.take_outgoing(), t0);
    host_events(&mut h, &mut w, hc);

    let join = |w: &mut Wires, si: &mut StandIn, h: &mut HostClient, name: &str, n: u8| {
        let c = si.open(&lobby.code).unwrap();
        let mut g = GuestClient::new(name, &key(n), &[]);
        w.send(si, c, g.take_outgoing(), t0);
        let peer = match guest_events(&mut g, w, c)[0] {
            GuestEvent::Waiting { peer } => peer,
            ref e => panic!("{e:?}"),
        };
        host_events(h, w, hc);
        h.accept(peer);
        w.send(si, hc, h.take_outgoing(), t0);
        host_events(h, w, hc);
        guest_events(&mut g, w, c);
        (c, g, peer)
    };
    let (ac, mut a, ap) = join(&mut w, &mut si, &mut h, "A", 10);
    let (bc, mut b, bp) = join(&mut w, &mut si, &mut h, "B", 11);
    let (cc, mut c, cp) = join(&mut w, &mut si, &mut h, "C", 12);

    // Asking before anyone is a helper still routes (C may know A from earlier).
    c.relay_ask(99, &[]);
    w.send(&mut si, cc, c.take_outgoing(), t0);
    assert!(matches!(&guest_events(&mut c, &mut w, cc)[0], GuestEvent::Error { error, .. } if error == "unknown_peer"));

    // A and B report direct paths: everyone hears who could relay.
    a.connected("direct");
    w.send(&mut si, ac, a.take_outgoing(), t0);
    b.connected("lan");
    w.send(&mut si, bc, b.take_outgoing(), t0);
    let last_helpers = |ev: Vec<GuestEvent>| {
        ev.into_iter()
            .rev()
            .find_map(|e| match e {
                GuestEvent::Helpers { peers } => Some(peers),
                _ => None,
            })
            .unwrap()
    };
    assert_eq!(last_helpers(guest_events(&mut c, &mut w, cc)), vec![ap, bp]);
    assert_eq!(last_helpers(guest_events(&mut a, &mut w, ac)), vec![bp], "not itself");
    host_events(&mut h, &mut w, hc);
    guest_events(&mut b, &mut w, bc);

    // C asks A, which refuses; then B, which accepts with its candidates.
    let c_cand = Candidate::new(Kind::ServerReflexive, "198.51.100.7:50000".parse().unwrap());
    c.relay_ask(ap, &[c_cand]);
    w.send(&mut si, cc, c.take_outgoing(), t0);
    assert_eq!(guest_events(&mut a, &mut w, ac), vec![GuestEvent::RelayAsk { from: cp, candidates: vec![c_cand] }]);
    assert!(host_events(&mut h, &mut w, hc).is_empty(), "the host is not involved");
    a.relay_answer(cp, false, "busy", &[]);
    w.send(&mut si, ac, a.take_outgoing(), t0);
    assert_eq!(
        guest_events(&mut c, &mut w, cc),
        vec![GuestEvent::RelayAnswer { helper: ap, accept: false, reason: "busy".into(), candidates: vec![] }]
    );
    c.relay_ask(bp, &[c_cand]);
    w.send(&mut si, cc, c.take_outgoing(), t0);
    guest_events(&mut b, &mut w, bc);
    let b_cand = Candidate::new(Kind::ServerReflexive, "203.0.113.9:47960".parse().unwrap());
    b.relay_answer(cp, true, "", &[b_cand]);
    w.send(&mut si, bc, b.take_outgoing(), t0);
    assert_eq!(
        guest_events(&mut c, &mut w, cc),
        vec![GuestEvent::RelayAnswer { helper: bp, accept: true, reason: String::new(), candidates: vec![b_cand] }]
    );

    // B loses its path: it is no longer offered.
    b.reconnect();
    w.send(&mut si, bc, b.take_outgoing(), t0);
    assert_eq!(last_helpers(guest_events(&mut c, &mut w, cc)), vec![ap]);

    // D is accepted after the set last changed and still learns it (found live, lobby Q5Y-C1C).
    let dc = si.open(&lobby.code).unwrap();
    let mut d = GuestClient::new("D", &key(13), &[]);
    w.send(&mut si, dc, d.take_outgoing(), t0);
    let dp = match guest_events(&mut d, &mut w, dc)[0] {
        GuestEvent::Waiting { peer } => peer,
        ref e => panic!("{e:?}"),
    };
    host_events(&mut h, &mut w, hc);
    h.accept(dp);
    w.send(&mut si, hc, h.take_outgoing(), t0);
    host_events(&mut h, &mut w, hc);
    assert_eq!(last_helpers(guest_events(&mut d, &mut w, dc)), vec![ap]);
    // A reconnect that changes no helper still tells the reconnecting guest the set.
    c.reconnect();
    w.send(&mut si, cc, c.take_outgoing(), t0);
    assert_eq!(last_helpers(guest_events(&mut c, &mut w, cc)), vec![ap]);
    assert!(guest_events(&mut d, &mut w, dc).is_empty(), "the others hear nothing new");
    host_events(&mut h, &mut w, hc);

    // A leaves: the list empties.
    a.bye();
    w.send(&mut si, ac, a.take_outgoing(), t0);
    assert_eq!(last_helpers(guest_events(&mut c, &mut w, cc)), Vec::<u32>::new());
}
