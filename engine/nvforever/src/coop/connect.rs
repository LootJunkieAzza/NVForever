//! Co-op's connections (docs/design/coop-networking.md): a host's lobby and a guest's join
//! over the internet transport (`nvf_coop::stack`), this game's key pair, the lobby's code
//! (an online code from the matchmaking service, else a code of the host's address with
//! the router asked to open the port), and what the service says.
//!
//! **Off by default.** Nothing here runs until a lobby is created or joined (the Co-op
//! switch, or `--host` / `--join`), so single player opens no socket [design].
//!
//! Log: `co-op: hosting on UDP port <p> (encrypted; this game's key <fp>)`, `co-op: lobby
//! code <code> ...`, `co-op: joining ...`, `co-op: online: <note>`, `co-op: <name> asks to
//! join ...`, and the online checks' own `co-op connect: ...` line (`nvf_coop::online`).

use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;

use bevy::prelude::*;
use fnv_util::words::{tr, trf};
use nvf_coop::code::Lobby;
use nvf_coop::net::{Link, Session};
use nvf_coop::noise::Identity;
use nvf_coop::online::{HostInfo, OnlineEvent};
use nvf_coop::reach::{self, Reach};
use nvf_coop::sockets::{Scope, Sockets};
use nvf_coop::stack::{self, Control, Stack};
use nvf_play::lobby::Joining;

use super::{Coop, DEFAULT_PORT};

/// The file in a game's home that keeps its co-op key pair: the private key, then the
/// public key, in hex. The host knows a returning player by its key.
const KEY_FILE: &str = "coop-identity.key";
/// A test aid: this run's key pair (as the file keeps it), so two games on one PC are two
/// players.
pub const KEY_ENV: &str = "NVF_COOP_KEY";

/// This game's co-op key pair: [`KEY_ENV`]'s, else the one its home keeps
/// ([`KEY_FILE`]), else a new one, kept there.
pub(super) fn identity() -> Identity {
    if let Some(id) = std::env::var(KEY_ENV).ok().and_then(|t| Identity::from_text(&t)) {
        return id;
    }
    let file = fnv_install::EngineHome::default_root().map(|h| h.join(KEY_FILE));
    if let Some(id) = file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| Identity::from_text(&t)) {
        return id;
    }
    let id = Identity::generate();
    if let Some(f) = &file {
        if let Err(e) = std::fs::create_dir_all(f.parent().unwrap_or(f)).and_then(|()| std::fs::write(f, format!("{}\n", id.to_text()))) {
            warn!("co-op: cannot keep this game's key in {}: {e} (a new one each run)", f.display());
        }
    }
    id
}

/// A host's link on `port`, every network, and its control.
pub(super) fn host_link(port: u16, name: &str) -> Result<(Link, Control), String> {
    let socks = Sockets::host(port, Scope::All).map_err(|e| trf("Cannot open UDP port {port} on this PC ({e}): is another game using it?", &[("port", &port), ("e", &e)]).to_string())?;
    let (mut stack, control) = Stack::host(socks, identity(), None);
    if let Some(service) = nvf_coop::signal_io::service() {
        stack.go_online(&service, HostInfo { name: name.into(), ..HostInfo::default() });
        info!("co-op: asking {service} for an online code");
    }
    info!("co-op: hosting on UDP port {port} (encrypted; this game's key {})", stack.identity().fingerprint());
    let admission = stack.admission();
    let link = Link::new(Session::host_on(Box::new(stack), Box::new(admission))).map_err(|e| e.to_string())?;
    link.set_name(name);
    Ok((link, control))
}

/// A guest's link to the host `at` names (an online code, an address code, or an address).
pub(super) fn guest_link(at: &str, name: &str) -> Result<(Link, Control), String> {
    let me = identity();
    let token = stack::token_of(&me.public());
    if let Some(nvf_signal::code::JoinForm::Online(code)) = nvf_signal::code::JoinForm::parse(at) {
        let Some(service) = nvf_coop::signal_io::service() else {
            return Err(tr("That is an online code, and Co-op connection is set to LAN only: change it in Settings to join.").into());
        };
        let fp = me.fingerprint();
        let (stack, control) = Stack::join_online(&service, code, name, "", "", me).map_err(|e| trf("Cannot open a connection: {e}", &[("e", &e)]).to_string())?;
        info!("co-op: joining online code {at} through {service} as {name} (encrypted; this game's key {fp})");
        let admission = stack.admission();
        let s = Session::join_on(Box::new(stack), Box::new(admission), &[stack::ONLINE_HOST], 0, name, token);
        return Ok((Link::new(s).map_err(|e| e.to_string())?, control));
    }
    let (candidates, key) = match Lobby::decode(at) {
        Some(l) => (l.candidates(reach::local_ip()), l.key()),
        None if at.contains('.') || at.contains(':') => (resolve(at).into_iter().collect(), 0),
        None => (Vec::new(), 0),
    };
    if candidates.is_empty() {
        return Err(tr("That is not a lobby code: check it with the host.").into());
    }
    let socks = Sockets::guest(&candidates).map_err(|e| trf("Cannot open a connection: {e}", &[("e", &e)]).to_string())?;
    info!("co-op: joining {candidates:?} as {name} (encrypted; this game's key {})", me.fingerprint());
    let (stack, control) = Stack::guest(socks, me, key);
    let admission = stack.admission();
    let s = Session::join_on(Box::new(stack), Box::new(admission), &candidates, key, name, token);
    Ok((Link::new(s).map_err(|e| e.to_string())?, control))
}

/// An address typed in (a host name or IP, with or without the port).
fn resolve(at: &str) -> Option<SocketAddr> {
    let at = at.trim();
    let with_port = if at.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { at.to_string() } else { format!("{at}:{DEFAULT_PORT}") };
    let all: Vec<SocketAddr> = with_port.to_socket_addrs().ok()?.collect();
    all.iter().find(|a| a.is_ipv4()).or(all.first()).copied()
}

/// The address code: the router asked to open `port`, on a thread (it takes seconds).
pub(super) fn reach_out(port: u16) -> Mutex<Receiver<Reach>> {
    let (tx, rx) = mpsc::channel();
    std::thread::Builder::new()
        .name("coop-reach".into())
        .spawn(move || {
            let _ = tx.send(reach::open(port));
        })
        .ok();
    Mutex::new(rx)
}

/// The router's port, closed again (a host that opened it leaves).
pub(super) fn close_router(port: u16) {
    std::thread::Builder::new().name("coop-reach".into()).spawn(move || reach::close(port)).ok();
}

/// The file in the game's home the lobby's code is written to, for the host to copy
/// (the host's Co-op screen says so).
const CODE_FILE: &str = "lobby-code.txt";

/// The lobby's code, kept in [`CODE_FILE`].
fn keep_code(code: &str) {
    if let Some(f) = fnv_install::EngineHome::default_root().map(|h| h.join(CODE_FILE)) {
        if let Err(e) = std::fs::write(&f, format!("{code}
")) {
            warn!("co-op: cannot write {}: {e}", f.display());
        }
    }
}

/// The router's answer, once it came: the lobby's address code.
pub(super) fn take_reach(coop: &mut Coop) {
    let Some(r) = coop.reaching.as_ref().and_then(|rx| rx.lock().ok()?.try_recv().ok()) else { return };
    coop.reaching = None;
    let code = r.lobby.encode();
    info!("co-op: lobby code {code} (host {}, this network {}, port {}; open to the internet: {})", r.lobby.host, r.local, r.lobby.port, r.opened);
    if let Some(link) = &coop.link {
        link.set_lobby(r.lobby.key());
    }
    if let Some(n) = &r.note {
        warn!("co-op: {n}");
    }
    keep_code(&code);
    coop.code = Some(code);
    coop.note = r.note;
    coop.opened = r.opened;
}

/// What the matchmaking service and its checks said (`nvf_coop::online`).
pub(super) fn online_event(coop: &mut Coop, e: OnlineEvent, notes: &mut Vec<String>) {
    match e {
        OnlineEvent::Code { code, .. } => {
            info!("co-op: lobby code {code} (online: the matchmaking service introduces players; the game's traffic goes between the games)");
            keep_code(&code);
            coop.code = Some(code);
            coop.online_pending = false;
            coop.note = None;
        }
        OnlineEvent::Failed(why) if coop.is_host() => {
            warn!("co-op: online code: {why}");
            if std::mem::take(&mut coop.online_pending) {
                info!("co-op: no online code: a code of this PC's address instead");
                coop.reaching = Some(reach_out(coop.port));
            }
        }
        OnlineEvent::Failed(why) => {
            coop.link = None;
            coop.control = None;
            coop.fail(&why);
        }
        OnlineEvent::HostAt(at) => info!("co-op: the host is at {at}: saying hello there"),
        OnlineEvent::HostVia(at) => info!("co-op: the host is reached through another player at {at} (relayed)"),
        OnlineEvent::RelayFor(peer) => info!("co-op: relaying for guest {peer}, who cannot reach the host"),
        OnlineEvent::RelayAt { peer, addr } => info!("co-op: guest {peer}, relayed, is at {addr}"),
        OnlineEvent::RelayEnd(peer) => info!("co-op: no longer relaying for guest {peer}"),
        OnlineEvent::JoinAsk { peer, name } => {
            info!("co-op: {name} asks to join (guest {peer}): the host decides in the Co-op Lobby");
            notes.push(trf("{name} wants to join: open the Co-op Lobby to let them in", &[("name", &name)]).to_string());
            coop.asking.retain(|(p, _)| *p != peer);
            coop.asking.push((peer, name));
        }
        OnlineEvent::JoinAskOver(peer) => coop.asking.retain(|(p, _)| *p != peer),
        OnlineEvent::AwaitingHost => {
            info!("co-op: waiting for the host to let this game in");
            coop.awaiting_host = true;
            coop.finding_since = None;
        }
        OnlineEvent::LetIn => {
            info!("co-op: the host let this game in");
            coop.awaiting_host = false;
            coop.finding_since = None;
        }
        OnlineEvent::HostedOpen { relay, side, .. } => info!("co-op: a session on the hosted relay {relay} ({side:?} side)"),
        OnlineEvent::Note(n) => info!("co-op: online: {n}"),
    }
}

/// The service's events for this game, and the router's answer (once a frame).
pub(super) fn poll(mut coop: ResMut<Coop>, mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>) {
    let coop = &mut *coop;
    take_reach(coop);
    let Some(events) = coop.control.as_ref().map(Control::events) else { return };
    let mut notes = Vec::new();
    for e in events {
        online_event(coop, e, &mut notes);
    }
    if let Some(c) = corner.as_mut() {
        for n in notes {
            c.push(n.into());
        }
    }
}

/// What the lobby shows for a failed join because the host said so.
pub(super) fn kicked_note(banned: bool) -> String {
    if banned {
        tr("The host removed you from the lobby and will not let you back in.").into()
    } else {
        tr("The host removed you from the lobby.").into()
    }
}

/// A guest's search gives up after `FIND_FOR`, or longer while the host is asked.
pub(super) fn waited_too_long(coop: &Coop, since: f32, now: f32) -> bool {
    let limit = if coop.awaiting_host { nvf_coop::online::ASK_FOR.as_secs_f32() + super::FIND_FOR } else { super::FIND_FOR };
    coop.joining == Joining::Finding && now - since > limit
}
