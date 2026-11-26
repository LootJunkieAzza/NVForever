//! The co-op session: a host and up to [`MAX_GUESTS`] guests, over a
//! [`Transport`] ([`Session::host_on`], [`Session::join_on`]; no real transport is in this
//! part, see [`crate::seam`]). Peers are known by their [`SocketAddr`].
//!
//! The host is the hub: each game sends its own player's [`PlayerState`] to the host
//! [`SEND_HZ`] times a second, and the host passes every guest's state on to the other
//! guests along with its own (peer 0); so it does each player's look, and every player's
//! name ([`Msg::Names`]). A reliable, ordered stream runs between the host and each guest
//! ([`crate::rel`], [`Session::send_reliable`]) for the layer above. Nothing of the world
//! is shared here, and nothing here knows about Bevy.
//!
//! **Who may join.** A guest's Hello carries the lobby's proof ([`Admission`]; the
//! lobby's secret, [`crate::seam::SharedSecret`]); the host drops a Hello without it
//! before anything else, a returning guest's token included. The host's answer proves the
//! secret back, and a guest takes as its host only an address whose answer does.
//!
//! **The host checks every guest's message** ([`crate::guard`]) before acting on it or
//! passing it on: a rate per sender and kind, bounds, movement, forms, and the rules a
//! layer above adds ([`Session::add_rule`]). The sender is the guest its address belongs
//! to, never an id the message names.
//!
//! **Dropouts are not disconnects.** A peer that goes quiet for [`LOST_AFTER`] is only
//! *lost*: its slot, id and player stay, and it is dropped only after [`GRACE`]. A guest
//! that stops hearing the host keeps sending Hello with its session token, and the host
//! gives a known token its old id back, from whatever address it now comes from (a
//! Wi-Fi blip, a router that changed the port). Only a deliberate Bye drops a peer at once.
//!
//! **Loading screens are not dropouts either.** [`Link`] runs the session on its own
//! thread, so heartbeats keep going while the game's frame is stalled loading a cell; the
//! game only hands it its newest state and takes the events.
//!
//! **Everything a peer can make grow is bounded**: the peers ([`MAX_GUESTS`]), the
//! per-address byte counts ([`TRAFFIC_ADDRS`]), the events waiting for the game
//! ([`EVENTS_MAX`]; a state or a look replaces the one before it), each reliable stream's
//! queue ([`crate::rel::MAX_UNACKED`]) and every list on the wire.
//!
//! Packets are little-endian: the magic `NVCO`, the protocol version, a kind byte, then
//! the kind's fields. A packet that does not parse is dropped.

use std::cell::RefCell;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::guard::{FormCheck, Guard, Kind, Rate, Reason, Rule, Stats, Verdict};
use crate::rel::{Frame, Reliable, ITEM_MAX, PACKET_BUDGET};
use crate::seam::{Admission, Transport};

pub const MAGIC: [u8; 4] = *b"NVCO";
/// The protocol's version: a packet of another is dropped. 138 is the core protocol on
/// its own (players and the reliable stream; the Hello and the host's answers carry the
/// lobby's proof), so no game of an earlier protocol joins it, nor it them.
pub const VERSION: u8 = 138;
pub const MAX_GUESTS: usize = 5;
pub const SEND_HZ: u32 = 20;
/// A peer that sends nothing for this long is lost (shown frozen, still in its slot).
pub const LOST_AFTER: Duration = Duration::from_secs(4);
/// A lost peer that has not come back after this long is dropped, and a guest stops trying.
pub const GRACE: Duration = Duration::from_secs(120);
/// How often a guest sends Hello while it has no answer.
const HELLO_EVERY: Duration = Duration::from_millis(500);
/// The network thread's tick.
pub const TICK: Duration = Duration::from_millis(10);
/// The longest name, in bytes.
pub const NAME_MAX: usize = 32;
/// IP and UDP headers, counted with each packet's bytes in [`Session::traffic`].
const UDP_OVERHEAD: u64 = 28;
/// The most reliable packets one stream sends per network tick ([`TICK`]): about 110 KB/s
/// while a join's snapshot goes, far less after.
pub const REL_PACKETS: usize = 4;
/// Items one reliable frame carries at most on the wire (each takes at least its 2-byte
/// length within [`PACKET_BUDGET`]).
pub const REL_ITEMS_MAX: usize = PACKET_BUDGET / 2;
/// How often the host sends every player's name ([`Msg::Names`]), and at once after a
/// guest joins.
pub const NAMES_EVERY: Duration = Duration::from_secs(2);
/// A goodbye is sent this many times ([`Session::bye`]) [design: one burst of loss does
/// not take them all].
pub const BYE_COPIES: usize = 3;
/// Between the copies of a goodbye [design: apart enough that one burst of loss does not
/// take them all, short enough that quitting does not wait].
const BYE_GAP: Duration = Duration::from_millis(15);
/// Addresses whose bytes are counted ([`Session::traffic`]), at most: past it, those not
/// of a current peer are forgotten [design: the peers and a few the guest still tries].
pub const TRAFFIC_ADDRS: usize = MAX_GUESTS + 4;
/// Events waiting for the game, at most [design: a game takes them every frame; a stalled
/// one still has room for a long load]. A state or a look replaces the same player's one
/// still waiting; past the bound other events are dropped (and counted), and reliable
/// frames are not taken (nor acknowledged) until the game takes its events, so the
/// sender keeps them.
pub const EVENTS_MAX: usize = 4096;

/// Where a player is: an interior cell, or a worldspace (exterior), by FormID in the
/// game's load order. Both games must run the same load order for the IDs to agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Interior(u32),
    Exterior(u32),
}

/// The most a look takes (the game's own encoding, [`Session::set_look`]), and how often it
/// is sent again (it changes rarely; a lost one is made up for by the next).
pub const LOOK_MAX: usize = 1200;
pub const LOOK_EVERY: Duration = Duration::from_secs(2);
/// States this far behind the last are from a game started again (10 s of them).
const RESTARTED: u32 = 200;
/// A reliable frame of a stream this few behind the guest's is one late on the way (each
/// Hello the host hears starts one more); any other number is a new stream [design].
const EPOCHS_LATE: u16 = 8;
/// Worn items sent with a state, at most.
pub const MAX_WORN: usize = 10;

/// [`PlayerState::flags`] bits.
pub mod flag {
    /// The weapon is out (drawn, not holstered).
    pub const DRAWN: u8 = 1;
    /// Aiming (the Block/aim button held).
    pub const AIMING: u8 = 2;
    pub const SNEAKING: u8 = 4;
    /// Off the ground (jumping, falling).
    pub const AIR: u8 = 8;
    /// Looking down the sights or a scope.
    pub const SIGHTS: u8 = 16;
    /// Dead: the world's actors leave this player alone.
    pub const DOWN: u8 = 32;
    /// The flashlight is on.
    pub const LIGHT: u8 = 64;
    /// Revived, the body still lies before getting up (the others' copies of it start
    /// their get-up when this clears).
    pub const LYING: u8 = 128;
}

/// [`PlayerState::attack_kind`]: what the newest attack was.
pub mod attack {
    pub const SHOT: u8 = 0;
    pub const SWING: u8 = 1;
    /// Power attacks: standing, then moving forward, back, left, right.
    pub const POWER: u8 = 2;
    pub const POWER_FORWARD: u8 = 3;
    pub const POWER_BACK: u8 = 4;
    pub const POWER_LEFT: u8 = 5;
    pub const POWER_RIGHT: u8 = 6;
}

/// One player's state, as its own game sees it.
///
/// Attacks and reloads are counters, not events: a lost packet loses nothing, the next
/// state's counter is still higher, and the other side plays one more of them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PlayerState {
    pub place: Place,
    /// Feet position, game units, Z up.
    pub feet: [f32; 3],
    /// Facing, radians (compass: 0 north, π/2 east).
    pub yaw: f32,
    /// Looking up or down, radians (negative down).
    pub pitch: f32,
    /// [`flag`] bits.
    pub flags: u8,
    /// The weapon in hand (WEAP FormID), 0 for none (fists).
    pub weapon: u32,
    /// Attacks begun so far (shots, swings), and what the newest was ([`attack`]).
    pub attacks: u16,
    pub attack_kind: u8,
    /// Reloads begun so far.
    pub reloads: u16,
    /// Worn armour and clothing (ARMO FormIDs), the first `worn_len`.
    pub worn: [u32; MAX_WORN],
    pub worn_len: u8,
    /// The Pip-Boy: 0 down, 1 the glance, 2 the full Pip-Boy.
    pub pipboy: u8,
    /// The furniture reference holding the player (a chair, a couch, a bed), 0 for none:
    /// the others draw the player getting on, sitting or lying there, and getting off.
    pub furniture: u32,
    /// [`more_flag`] bits: the states [`PlayerState::flags`] has no room for.
    pub more: u8,
    /// The emote the player plays, for the others to draw on its puppet.
    pub emote: EmoteState,
}

/// An emote as it goes to the others: which (`id` 0 for none, else the emote's number
/// plus one), which of its clips (`variant`), its phase ([`emote`]), which of its loop's
/// clips (`step`) and how far into the clip it is (`t`, seconds), so one who joins or
/// comes near late sees it where it is.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EmoteState {
    pub id: u8,
    pub variant: u8,
    pub phase: u8,
    pub step: u8,
    pub t: f32,
}

/// [`EmoteState::phase`] values.
pub mod emote {
    pub const ENTER: u8 = 0;
    pub const STAY: u8 = 1;
    pub const EXIT: u8 = 2;
    /// A one-shot emote's only clip.
    pub const ONCE: u8 = 3;
}

/// [`PlayerState::more`] bits.
pub mod more_flag {
    /// In deep water: the others draw the player swimming.
    pub const SWIMMING: u8 = 1;
    /// In combat: the others cannot fast travel to this player.
    pub const COMBAT: u8 = 2;
    /// Somewhere no one may fast travel to this player: in a conversation or a scene that
    /// holds the controls, or with fast travel turned off by a script.
    pub const NO_TRAVEL: u8 = 4;
}

impl PlayerState {
    /// A state with nothing in hand and nothing worn.
    pub fn at(place: Place, feet: [f32; 3], yaw: f32) -> Self {
        Self { place, feet, yaw, pitch: 0.0, flags: 0, weapon: 0, attacks: 0, attack_kind: 0, reloads: 0, worn: [0; MAX_WORN], worn_len: 0, pipboy: 0, furniture: 0, more: 0, emote: EmoteState::default() }
    }

    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }

    pub fn worn(&self) -> &[u32] {
        &self.worn[..(self.worn_len as usize).min(MAX_WORN)]
    }
}

/// A message on the wire. Public so the host's rules ([`crate::guard::Rule`]) can read
/// what they judge; only the session encodes and sends them.
#[derive(Clone, Debug, PartialEq)]
pub enum Msg {
    /// `lobby` names the lobby the guest asks for (0 for none): a host that was given a
    /// lobby number ([`Session::set_lobby`]) answers only Hellos naming it, or none. `proof`
    /// is the guest's [`Admission::hello`].
    Hello { token: u64, lobby: u32, proof: u64, name: String },
    /// The host lets the guest in as `id`; `proof` is the host's [`Admission::answer`].
    Welcome { id: u8, proof: u64 },
    /// The host has no free slot; `proof` as a Welcome's.
    Full { proof: u64 },
    /// The host does not know the sender: say Hello again.
    Who,
    State { id: u8, seq: u32, state: PlayerState },
    /// "Still here", from a game with no state to send (a guest choosing its save).
    Ping,
    Bye { id: u8 },
    /// Every player's name, from the host (the host is id 0): a guest learns the host's
    /// and the other guests' names from it.
    Names { names: Vec<(u8, String)> },
    /// How player `id` looks (race, face, hair...; the game's own encoding).
    Look { id: u8, look: Vec<u8> },
    /// The reliable stream between the host and one guest ([`crate::rel`]), both ways;
    /// `epoch` is the host's number for the stream, new each time the guest says Hello.
    Rel { epoch: u16, frame: Frame },
}

impl Msg {
    /// Its kind, as the guard counts and limits it.
    pub fn kind(&self) -> Kind {
        match self {
            Msg::Hello { .. } => Kind::Hello,
            Msg::Welcome { .. } => Kind::Welcome,
            Msg::Full { .. } => Kind::Full,
            Msg::Who => Kind::Who,
            Msg::State { .. } => Kind::State,
            Msg::Ping => Kind::Ping,
            Msg::Bye { .. } => Kind::Bye,
            Msg::Names { .. } => Kind::Names,
            Msg::Look { .. } => Kind::Look,
            Msg::Rel { .. } => Kind::Rel,
        }
    }
}

/// What [`Link::events`] reports.
#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    /// This guest was let in as peer `id` (guests only; again after the host restarted).
    Welcomed(u8),
    /// The host had no free slot (guests only).
    Refused,
    /// A peer joined (`name` is empty when only its state has been seen).
    Joined { id: u8, name: String },
    State { id: u8, state: PlayerState },
    /// A peer went quiet; it keeps its slot for [`GRACE`].
    Lost(u8),
    /// A lost peer is back.
    Back(u8),
    /// A peer left, or was lost for longer than [`GRACE`].
    Left(u8),
    /// The host went quiet (guests only); trying to reconnect.
    HostLost,
    /// The host answered again (guests only).
    HostBack,
    /// The host left, or did not answer for [`GRACE`] (guests only).
    HostGone,
    /// A peer's look, when first heard or changed.
    Look { id: u8, look: Vec<u8> },
    /// A player's name, when first heard or changed (guests only; [`Msg::Names`]).
    Named { id: u8, name: String },
    /// An item of the reliable stream from player `id` (the host: 0), once and in order.
    Reliable { id: u8, item: Vec<u8> },
    /// The reliable stream with player `id` started again (a guest's Hello on the host;
    /// a new stream from the host on a guest, id 0): what was sent on it before may be
    /// lost, so the layer above sends its whole state again.
    RelReset { id: u8 },
}

impl Event {
    /// The player an event is about, if one.
    fn about(&self) -> Option<u8> {
        match self {
            Event::Welcomed(id) | Event::Lost(id) | Event::Back(id) | Event::Left(id) => Some(*id),
            Event::Joined { id, .. } | Event::State { id, .. } | Event::Look { id, .. } | Event::Named { id, .. } | Event::Reliable { id, .. } | Event::RelReset { id } => Some(*id),
            Event::Refused | Event::HostLost | Event::HostBack | Event::HostGone => Some(0),
        }
    }
}

/// What [`Session::traffic`] reports for one player.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Traffic {
    pub id: u8,
    pub sent: u64,
    pub heard: u64,
    pub pending: usize,
    pub resent: u64,
}

/// A peer as one side sees it.
struct Peer {
    id: u8,
    /// The host's record of a guest: its address and token. Unused for a guest's peers.
    addr: Option<SocketAddr>,
    token: u64,
    last_heard: Instant,
    last_seq: Option<u32>,
    lost: bool,
    look: Option<Vec<u8>>,
    /// Its name: the host's record of a guest's (its Hello), a guest's of another player's
    /// (the host's [`Msg::Names`]).
    name: String,
    /// The host's reliable stream with this guest, and its number (host only).
    rel: Reliable,
    epoch: u16,
}

impl Peer {
    fn new(id: u8, now: Instant) -> Self {
        Self { id, addr: None, token: 0, last_heard: now, last_seq: None, lost: false, look: None, name: String::new(), rel: Reliable::new(), epoch: 0 }
    }

    /// Keep `look`; whether it is new.
    fn take_look(&mut self, look: Vec<u8>) -> bool {
        if self.look.as_ref() == Some(&look) {
            return false;
        }
        self.look = Some(look);
        true
    }
}

enum Role {
    Host,
    Guest {
        /// Where the host may be (the addresses the transport's join was given); Hello goes to each until the host answers, from whatever address
        /// it has, and that one is `host`.
        candidates: Vec<SocketAddr>,
        host: Option<SocketAddr>,
        token: u64,
        lobby: u32,
        name: String,
        id: Option<u8>,
        last_heard: Instant,
        last_hello: Option<Instant>,
        host_lost: bool,
        /// The host asked who this is (it knows this guest by another address): say
        /// Hello again.
        asked: bool,
        gone: bool,
    },
}

/// One side of the link, ticked by [`Link`]'s thread (or by hand in tests).
pub struct Session {
    transport: Box<dyn Transport>,
    admission: Box<dyn Admission>,
    role: Role,
    /// The host's guests, or the other peers a guest has heard of (the host among them).
    peers: Vec<Peer>,
    state: Option<PlayerState>,
    seq: u32,
    last_send: Option<Instant>,
    look: Option<Vec<u8>>,
    last_look: Option<Instant>,
    /// A host's lobby number, once it has one: Hellos for another lobby are not answered.
    lobby: Option<u32>,
    /// The host's own name, and when every player's names last went.
    own_name: String,
    last_names: Option<Instant>,
    events: Vec<Event>,
    /// Events dropped past [`EVENTS_MAX`].
    events_dropped: u64,
    /// A guest's reliable stream with the host, and the host's number for it once heard.
    rel: Reliable,
    host_epoch: Option<u16>,
    /// Bytes sent to and heard from each address (the perf lines), at most
    /// [`TRAFFIC_ADDRS`] addresses.
    sent: RefCell<HashMap<SocketAddr, u64>>,
    heard: HashMap<SocketAddr, u64>,
    /// The host's checks on its guests' messages.
    guard: Guard,
}

impl Session {
    /// Host over `transport`, letting in the guests `admission` admits.
    pub fn host_on(transport: Box<dyn Transport>, admission: Box<dyn Admission>) -> Self {
        Self::with(transport, admission, Role::Host)
    }

    /// Join over `transport` the host at the first of `candidates` that answers, proving
    /// the lobby with `admission`. `lobby` names the lobby asked for (0 for none), `name` is
    /// shown to the others. `token` names this guest to the host across dropouts and
    /// restarts ([`new_token`] for a new one).
    pub fn join_on(transport: Box<dyn Transport>, admission: Box<dyn Admission>, candidates: &[SocketAddr], lobby: u32, name: &str, token: u64) -> Self {
        let role = Role::Guest {
            candidates: candidates.to_vec(),
            host: None,
            token,
            lobby,
            name: crate::guard::clean_name(name),
            id: None,
            last_heard: Instant::now(),
            last_hello: None,
            host_lost: false,
            asked: false,
            gone: false,
        };
        Self::with(transport, admission, role)
    }

    fn with(transport: Box<dyn Transport>, admission: Box<dyn Admission>, role: Role) -> Self {
        Self {
            transport,
            admission,
            role,
            peers: Vec::new(),
            state: None,
            seq: 0,
            last_send: None,
            look: None,
            last_look: None,
            lobby: None,
            own_name: String::new(),
            last_names: None,
            events: Vec::new(),
            events_dropped: 0,
            rel: Reliable::new(),
            host_epoch: None,
            sent: RefCell::new(HashMap::new()),
            heard: HashMap::new(),
            guard: Guard::default(),
        }
    }

    /// A guest's host is now at `to` (the host's game came back on another port): Hello
    /// goes there at once, with this guest's token, so the host knows it again. Nothing
    /// for a host, nor when `to` is the host already. True when the host moved.
    pub fn move_host(&mut self, to: SocketAddr) -> bool {
        let Role::Guest { candidates, host, last_hello, asked, gone, .. } = &mut self.role else { return false };
        if *host == Some(to) || (host.is_none() && candidates.as_slice() == [to]) || *gone {
            return false;
        }
        *candidates = vec![to];
        *host = Some(to);
        *asked = true;
        *last_hello = None;
        true
    }

    /// The host's own name, for the guests ([`Msg::Names`]).
    pub fn set_name(&mut self, name: &str) {
        let name = crate::guard::clean_name(name);
        if name != self.own_name {
            self.own_name = name;
            self.last_names = None;
        }
    }

    /// A host's lobby number: from now on only Hellos naming it (or none) are answered.
    pub fn set_lobby(&mut self, key: u32) {
        self.lobby = Some(key);
    }

    /// What the host's game knows of forms ([`FormCheck`]), for its checks on the guests'
    /// states; None: only the numeric checks.
    pub fn set_forms(&mut self, forms: Option<Arc<dyn FormCheck>>) {
        self.guard.set_forms(forms);
    }

    /// A check of the host's on guests' messages of `kind` ([`crate::guard::Rule`]).
    pub fn add_rule(&mut self, kind: Kind, rule: Box<dyn Rule>) {
        self.guard.add_rule(kind, rule);
    }

    /// The rate the host lets each guest send messages of `kind` at.
    pub fn set_rate(&mut self, kind: Kind, rate: Rate) {
        self.guard.set_rate(kind, rate);
    }

    /// What the host's checks dropped, by kind and reason, and each guest's bad messages.
    pub fn guard_stats(&self) -> Stats {
        self.guard.stats()
    }

    /// Guest `id`'s bad messages so far (dropped by a check other than the rate, or not
    /// parsing), host only: for a kick policy.
    pub fn bad_messages(&self, id: u8) -> u64 {
        self.guard.bad(id)
    }

    /// Events dropped because the game did not take them ([`EVENTS_MAX`]).
    pub fn events_dropped(&self) -> u64 {
        self.events_dropped
    }

    pub fn is_host(&self) -> bool {
        matches!(self.role, Role::Host)
    }

    /// This game's peer id: 0 for the host, the host's answer for a guest.
    pub fn id(&self) -> Option<u8> {
        match &self.role {
            Role::Host => Some(0),
            Role::Guest { id, .. } => *id,
        }
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.transport.local_addr()
    }

    /// The host's address once one of the candidates answered (guests only).
    pub fn host_addr(&self) -> Option<SocketAddr> {
        match &self.role {
            Role::Guest { host, .. } => *host,
            Role::Host => None,
        }
    }

    /// This game's newest player state; [`Session::tick`] sends it at [`SEND_HZ`].
    pub fn set_state(&mut self, state: PlayerState) {
        self.state = Some(state);
    }

    /// This game's player's look, sent every [`LOOK_EVERY`] (and at once when it changes);
    /// one longer than [`LOOK_MAX`] is not sent.
    pub fn set_look(&mut self, look: Vec<u8>) {
        if look.len() > LOOK_MAX {
            return;
        }
        if self.look.as_ref() != Some(&look) {
            self.last_look = None;
        }
        self.look = Some(look);
    }

    /// Queue `item` on the reliable stream: a host's to guest `to` (None: every guest), a
    /// guest's to the host (`to` is ignored). False when a stream refused it, full
    /// ([`crate::rel::MAX_UNACKED`]): the other side is not acknowledging, and the layer
    /// above sends its whole state again when the stream starts again
    /// ([`Event::RelReset`]).
    #[must_use = "a full stream refuses the item"]
    pub fn send_reliable(&mut self, to: Option<u8>, item: Vec<u8>) -> bool {
        if self.is_host() {
            let mut all = true;
            for p in self.peers.iter_mut().filter(|p| to.is_none_or(|t| t == p.id)) {
                all &= p.rel.push(item.clone());
            }
            all
        } else {
            self.rel.push(item)
        }
    }

    /// Bytes sent to and heard from each player (the host: 0) so far, with the items
    /// waiting on its reliable stream and those sent again.
    pub fn traffic(&self) -> Vec<Traffic> {
        let sent = self.sent.borrow();
        let of = |a: Option<SocketAddr>| a.map_or((0, 0), |a| (sent.get(&a).copied().unwrap_or(0), self.heard.get(&a).copied().unwrap_or(0)));
        match &self.role {
            Role::Host => self
                .peers
                .iter()
                .map(|p| {
                    let (out, inb) = of(p.addr);
                    Traffic { id: p.id, sent: out, heard: inb, pending: p.rel.pending(), resent: p.rel.resent }
                })
                .collect(),
            Role::Guest { host, .. } => {
                let (out, inb) = of(*host);
                vec![Traffic { id: 0, sent: out, heard: inb, pending: self.rel.pending(), resent: self.rel.resent }]
            }
        }
    }

    pub fn take_events(&mut self) -> Vec<Event> {
        std::mem::take(&mut self.events)
    }

    /// Read every waiting packet, send this game's state when it is due, say Hello while
    /// a guest has no answer, and mark quiet peers lost or gone.
    pub fn tick(&mut self, now: Instant) {
        self.receive(now);
        self.check_peers(now);
        if let Role::Guest { candidates, host, token, lobby, name, id, last_heard, last_hello, host_lost, asked, gone } = &mut self.role {
            if *gone {
                return;
            }
            let silent = now.saturating_duration_since(*last_heard);
            if id.is_some() && !*host_lost && silent >= LOST_AFTER {
                *host_lost = true;
                self.events.push(Event::HostLost);
            }
            if silent >= GRACE {
                *gone = true;
                self.events.push(Event::HostGone);
                return;
            }
            // Hello until welcomed, and again while the host is lost or asks who this is:
            // the host may have dropped this address, or restarted and forgotten everyone.
            if (id.is_none() || *host_lost || *asked) && last_hello.is_none_or(|t| now.saturating_duration_since(t) >= HELLO_EVERY) {
                *asked = false;
                let hello = encode(&Msg::Hello { token: *token, lobby: *lobby, proof: self.admission.hello(), name: name.clone() });
                // Until one answers, every address the lobby gave; then that one.
                for to in host.iter().chain(candidates.iter().filter(|_| host.is_none())) {
                    let _ = self.transport.send_to(&hello, *to);
                }
                *last_hello = Some(now);
            }
        }
        let due = self.last_send.is_none_or(|t| now.saturating_duration_since(t) >= Duration::from_secs(1) / SEND_HZ);
        if let (true, Some(id)) = (due, self.id()) {
            let msg = match self.state {
                Some(state) => {
                    self.seq = self.seq.wrapping_add(1);
                    Msg::State { id, seq: self.seq, state }
                }
                None => Msg::Ping,
            };
            self.to_all(&msg);
            self.last_send = Some(now);
        }
        if self.is_host() && !self.peers.is_empty() && self.last_names.is_none_or(|t| now.saturating_duration_since(t) >= NAMES_EVERY) {
            let names = std::iter::once((0, self.own_name.clone())).chain(self.peers.iter().map(|p| (p.id, p.name.clone()))).collect();
            self.to_all(&Msg::Names { names });
            self.last_names = Some(now);
        }
        // The reliable streams: what is due, and acknowledgements.
        if self.is_host() {
            let mut out = Vec::new();
            for p in self.peers.iter_mut() {
                if let Some(addr) = p.addr.filter(|_| !p.lost) {
                    for frame in p.rel.frames(now, REL_PACKETS) {
                        out.push((Msg::Rel { epoch: p.epoch, frame }, addr));
                    }
                }
            }
            for (msg, addr) in out {
                self.sock_send(&msg, addr);
            }
        } else if let (Some(epoch), Role::Guest { host: Some(host), id: Some(_), .. }) = (self.host_epoch, &self.role) {
            let host = *host;
            for frame in self.rel.frames(now, REL_PACKETS) {
                self.sock_send(&Msg::Rel { epoch, frame }, host);
            }
        }
        let look_due = self.last_look.is_none_or(|t| now.saturating_duration_since(t) >= LOOK_EVERY);
        if let (true, Some(look), Some(id)) = (look_due, self.look.clone(), self.id()) {
            self.to_all(&Msg::Look { id, look });
            self.last_look = Some(now);
        }
    }

    /// To every guest (the host), or to the host (a guest).
    fn to_all(&self, msg: &Msg) {
        match &self.role {
            Role::Host => {
                for addr in self.peers.iter().filter_map(|p| p.addr) {
                    self.sock_send(msg, addr);
                }
            }
            Role::Guest { host: Some(host), .. } => self.sock_send(msg, *host),
            Role::Guest { .. } => {}
        }
    }

    fn sock_send(&self, msg: &Msg, to: SocketAddr) {
        let bytes = encode(msg);
        let known = |a: SocketAddr| known_addr(&self.role, &self.peers, a);
        tally(&mut self.sent.borrow_mut(), to, bytes.len() as u64 + UDP_OVERHEAD, known);
        let _ = self.transport.send_to(&bytes, to);
    }

    fn receive(&mut self, now: Instant) {
        let mut buf = [0u8; 1500];
        // A transport error other than "nothing waiting" ends this tick's reading.
        while let Ok(Some((n, from))) = self.transport.recv_from(&mut buf) {
            self.handle(&buf[..n], from, now);
        }
    }

    fn handle(&mut self, bytes: &[u8], from: SocketAddr, now: Instant) {
        let known = |a: SocketAddr| known_addr(&self.role, &self.peers, a);
        tally(&mut self.heard, from, bytes.len() as u64 + UDP_OVERHEAD, known);
        let Some(msg) = decode(bytes) else {
            if let (true, Some(id)) = (self.is_host(), self.peer_at(from)) {
                self.guard.note_bad(id);
            }
            return;
        };
        if self.is_host() {
            self.host_handle(msg, from, now);
        } else {
            self.guest_handle(msg, from, now);
        }
    }

    /// The guest at address `from` (host only).
    fn peer_at(&self, from: SocketAddr) -> Option<u8> {
        self.peers.iter().find(|p| p.addr == Some(from)).map(|p| p.id)
    }

    /// Note an event for the game (module notes: bounded).
    fn push_event(&mut self, e: Event) {
        if let Event::State { id, .. } | Event::Look { id, .. } = &e {
            let (id, state) = (*id, matches!(e, Event::State { .. }));
            // The same player's last one still waiting, unless something else about the
            // player came after it.
            for old in self.events.iter_mut().rev() {
                let same = match old {
                    Event::State { id: i, .. } => state && *i == id,
                    Event::Look { id: i, .. } => !state && *i == id,
                    _ => false,
                };
                if same {
                    *old = e;
                    return;
                }
                if old.about() == Some(id) && !matches!(old, Event::State { .. } | Event::Look { .. }) {
                    break;
                }
            }
        }
        // A reliable item is never dropped: frames are not taken while the events are full.
        if self.events.len() >= EVENTS_MAX && !matches!(e, Event::Reliable { .. }) {
            self.events_dropped += 1;
            return;
        }
        self.events.push(e);
    }

    fn host_handle(&mut self, mut msg: Msg, from: SocketAddr, now: Instant) {
        // The sender is the guest its address belongs to, whatever id the message names.
        let peer = self.peer_at(from);
        if self.guard.check(from, peer, &mut msg, now) == Verdict::Accept {
            self.host_take(msg, peer, from, now);
        }
    }

    /// A guest's message the guard accepted.
    fn host_take(&mut self, msg: Msg, peer: Option<u8>, from: SocketAddr, now: Instant) {
        match msg {
            Msg::Hello { token, lobby, proof, name } => {
                // Another lobby's guest asking around its network.
                if lobby != 0 && self.lobby.is_some_and(|k| k != lobby) {
                    return;
                }
                // The lobby's proof before anything else, a known token's too.
                if !self.admission.admits(proof) {
                    self.guard.note(Kind::Hello, Reason::Secret, peer);
                    return;
                }
                // A known token is a guest coming back, from wherever it now is; a guest
                // already at this address with a new token is the same guest started again
                // (it keeps its slot: one address never holds two).
                let slot = self.peers.iter().position(|p| p.token == token).or_else(|| peer.and_then(|id| self.peers.iter().position(|p| p.id == id)));
                if let Some(p) = slot.map(|i| &mut self.peers[i]) {
                    p.token = token;
                    p.addr = Some(from);
                    p.last_heard = now;
                    // A game started again counts its states from the start.
                    p.last_seq = None;
                    p.name = name;
                    let id = p.id;
                    // A Hello starts the reliable stream again (the guest may have started
                    // again, or lost what was on the way).
                    p.epoch = p.epoch.wrapping_add(1);
                    p.rel = Reliable::new();
                    let back = std::mem::take(&mut p.lost);
                    self.last_names = None;
                    self.guard.forget(id);
                    self.push_event(Event::RelReset { id });
                    if back {
                        self.push_event(Event::Back(id));
                    }
                    self.sock_send(&Msg::Welcome { id, proof: self.admission.answer(token, Some(id)) }, from);
                    return;
                }
                let Some(id) = (1..=MAX_GUESTS as u8).find(|i| self.peers.iter().all(|p| p.id != *i)) else {
                    self.sock_send(&Msg::Full { proof: self.admission.answer(token, None) }, from);
                    return;
                };
                // A stream number unlike a guest's last one in this slot, and unlike the
                // one a host game before this one gave it (a guest coming back to a host
                // started again kept that one).
                let epoch = ((token as u16) ^ (new_token() as u16)) | 1;
                self.guard.drop_guest(id);
                self.peers.push(Peer { addr: Some(from), token, name: name.clone(), epoch, ..Peer::new(id, now) });
                self.last_names = None;
                self.sock_send(&Msg::Welcome { id, proof: self.admission.answer(token, Some(id)) }, from);
                self.push_event(Event::Joined { id, name });
                self.push_event(Event::RelReset { id });
            }
            Msg::State { seq, state, .. } => {
                let Some(p) = self.peers.iter_mut().find(|p| Some(p.id) == peer) else {
                    self.sock_send(&Msg::Who, from);
                    return;
                };
                p.last_heard = now;
                let id = p.id;
                let back = std::mem::take(&mut p.lost);
                let stale = p.last_seq.is_some_and(|s| !fresh(seq, s));
                if !stale {
                    p.last_seq = Some(seq);
                }
                if back {
                    self.push_event(Event::Back(id));
                }
                if stale {
                    return;
                }
                let relay = Msg::State { id, seq, state };
                for addr in self.peers.iter().filter(|o| o.id != id).filter_map(|o| o.addr) {
                    self.sock_send(&relay, addr);
                }
                self.push_event(Event::State { id, state });
            }
            Msg::Ping => {
                let Some(p) = self.peers.iter_mut().find(|p| Some(p.id) == peer) else {
                    self.sock_send(&Msg::Who, from);
                    return;
                };
                p.last_heard = now;
                let id = p.id;
                if std::mem::take(&mut p.lost) {
                    self.push_event(Event::Back(id));
                }
            }
            Msg::Look { look, .. } => {
                let Some(p) = self.peers.iter_mut().find(|p| Some(p.id) == peer) else { return };
                let id = p.id;
                if p.take_look(look.clone()) {
                    self.push_event(Event::Look { id, look: look.clone() });
                }
                // Each time (it comes again every LOOK_EVERY): a guest who joined since has it.
                let relay = Msg::Look { id, look };
                for addr in self.peers.iter().filter(|o| o.id != id).filter_map(|o| o.addr) {
                    self.sock_send(&relay, addr);
                }
            }
            Msg::Bye { .. } => {
                if let Some(id) = peer {
                    self.drop_peer(id);
                }
            }
            Msg::Rel { epoch, frame } => {
                // Not taken while the game is not taking its events: unacknowledged, the
                // guest sends it again.
                if self.events.len() >= EVENTS_MAX {
                    return;
                }
                let Some(p) = self.peers.iter_mut().find(|p| Some(p.id) == peer) else { return };
                // A frame of an older stream.
                if epoch != p.epoch {
                    return;
                }
                p.last_heard = now;
                let id = p.id;
                for item in p.rel.receive(frame) {
                    // Each item passes the host's rules for its kind before the game has it.
                    if self.guard.admit_item(id, &item, now) == Verdict::Accept {
                        self.push_event(Event::Reliable { id, item });
                    }
                }
            }
            // Dropped by the guard: only the host sends these.
            Msg::Welcome { .. } | Msg::Full { .. } | Msg::Who | Msg::Names { .. } => {}
        }
    }

    /// The host drops guest `id`: the others are told, and the game.
    fn drop_peer(&mut self, id: u8) {
        self.peers.retain(|p| p.id != id);
        self.guard.drop_guest(id);
        self.relay_bye(id);
        self.push_event(Event::Left(id));
    }

    /// A guest welcomed (again): what it heard from the host so far counts from the start
    /// again: the reliable stream both ways (the host's end is new too) and the host's
    /// epoch.
    fn hear_anew(&mut self) {
        // Before any stream was heard (the first Welcome), what is queued keeps (as the
        // first stream keeps it, `Msg::Rel`).
        if self.host_epoch.take().is_some() {
            self.rel = Reliable::new();
        }
    }

    fn guest_handle(&mut self, msg: Msg, from: SocketAddr, now: Instant) {
        let Role::Guest { host, token, id, last_heard, host_lost, gone, .. } = &mut self.role else { return };
        if *gone {
            return;
        }
        // An answer must prove the lobby's secret: one that does not is forged.
        let proven = match &msg {
            Msg::Welcome { id: given, proof } => self.admission.answered(*token, Some(*given), *proof),
            Msg::Full { proof } => self.admission.answered(*token, None, *proof),
            _ => true,
        };
        if !proven {
            return;
        }
        // Before an answer, the address whose answer proves the secret is the host (an
        // answer may come from another address than the one the Hello went to); then only it.
        match *host {
            Some(h) if h != from => return,
            Some(_) => {}
            None if matches!(msg, Msg::Welcome { .. } | Msg::Full { .. }) => *host = Some(from),
            None => return,
        }
        *last_heard = now;
        let back = std::mem::take(host_lost);
        let me = *id;
        if back {
            self.push_event(Event::HostBack);
        }
        match msg {
            Msg::Welcome { id: given, .. } => {
                let Role::Guest { id, .. } = &mut self.role else { return };
                if *id != Some(given) {
                    // First welcome, or the host restarted and gave a new id: everyone it
                    // knew is announced again by their states.
                    *id = Some(given);
                    for p in std::mem::take(&mut self.peers) {
                        self.push_event(Event::Left(p.id));
                    }
                    self.push_event(Event::Welcomed(given));
                }
                // A Welcome answers a Hello, and the host starts this guest's stream again
                // for each one (a known token), or never had it (a host started again): so
                // does this game.
                self.hear_anew();
            }
            Msg::Full { .. } => self.push_event(Event::Refused),
            Msg::Who => {
                let Role::Guest { asked, last_hello, .. } = &mut self.role else { return };
                *asked = true;
                *last_hello = None;
            }
            Msg::State { id: peer, seq, state } => {
                if Some(peer) == me || peer as usize > MAX_GUESTS {
                    return;
                }
                let joined = self.guest_peer(peer, now);
                let p = self.peers.iter_mut().find(|p| p.id == peer).expect("just made");
                p.last_heard = now;
                let back = std::mem::take(&mut p.lost);
                let stale = p.last_seq.is_some_and(|s| !fresh(seq, s));
                if !stale {
                    p.last_seq = Some(seq);
                }
                if joined {
                    self.push_event(Event::Joined { id: peer, name: String::new() });
                }
                if back {
                    self.push_event(Event::Back(peer));
                }
                if !stale {
                    self.push_event(Event::State { id: peer, state });
                }
            }
            Msg::Look { id: peer, look } => {
                if Some(peer) == me || peer as usize > MAX_GUESTS {
                    return;
                }
                let joined = self.guest_peer(peer, now);
                let p = self.peers.iter_mut().find(|p| p.id == peer).expect("just made");
                let new = p.take_look(look.clone());
                if joined {
                    self.push_event(Event::Joined { id: peer, name: String::new() });
                }
                if new {
                    self.push_event(Event::Look { id: peer, look });
                }
            }
            Msg::Bye { id: 0 } => {
                let Role::Guest { gone, .. } = &mut self.role else { return };
                *gone = true;
                for p in std::mem::take(&mut self.peers) {
                    self.push_event(Event::Left(p.id));
                }
                self.push_event(Event::HostGone);
            }
            Msg::Bye { id: peer } => {
                if let Some(i) = self.peers.iter().position(|p| p.id == peer) {
                    self.peers.remove(i);
                    self.push_event(Event::Left(peer));
                }
            }
            Msg::Names { names } => {
                for (peer, name) in names {
                    if Some(peer) == me || name.is_empty() || peer as usize > MAX_GUESTS {
                        continue;
                    }
                    if self.guest_peer(peer, now) {
                        self.push_event(Event::Joined { id: peer, name: String::new() });
                    }
                    let p = self.peers.iter_mut().find(|p| p.id == peer).expect("just made");
                    if p.name != name {
                        p.name = name.clone();
                        self.push_event(Event::Named { id: peer, name });
                    }
                }
            }
            Msg::Rel { epoch, frame } => {
                // Not taken while the game is not taking its events (module notes).
                if self.events.len() >= EVENTS_MAX {
                    return;
                }
                // A new stream (the host heard this guest's Hello): what was on the old
                // one is gone; a frame of one of the last few streams, late on the way, is
                // dropped. Any other number is a new stream (a host game started again
                // numbers its own), so a Welcome lost on the way does not leave this guest
                // on the old one.
                match self.host_epoch {
                    Some(e) if e == epoch => {}
                    Some(e) if (1..=EPOCHS_LATE).contains(&e.wrapping_sub(epoch)) => return,
                    was => {
                        // The first stream keeps what was queued before it was heard
                        // (nothing of it went yet).
                        if was.is_some() {
                            self.rel = Reliable::new();
                        }
                        self.host_epoch = Some(epoch);
                        self.push_event(Event::RelReset { id: 0 });
                    }
                }
                for item in self.rel.receive(frame) {
                    self.push_event(Event::Reliable { id: 0, item });
                }
            }
            // A guest's messages, for the host only.
            Msg::Hello { .. } | Msg::Ping => {}
        }
    }

    /// A guest's record of player `peer`, made if new (true).
    fn guest_peer(&mut self, peer: u8, now: Instant) -> bool {
        if self.peers.iter().any(|p| p.id == peer) {
            return false;
        }
        self.peers.push(Peer::new(peer, now));
        true
    }

    fn relay_bye(&self, id: u8) {
        if self.is_host() {
            self.to_all(&Msg::Bye { id });
        }
    }

    /// Quiet peers become lost, and lost ones past [`GRACE`] are dropped.
    fn check_peers(&mut self, now: Instant) {
        let mut gone = Vec::new();
        let mut lost = Vec::new();
        for p in &mut self.peers {
            let silent = now.saturating_duration_since(p.last_heard);
            if silent >= GRACE {
                gone.push(p.id);
            } else if !p.lost && silent >= LOST_AFTER {
                p.lost = true;
                lost.push(p.id);
            }
        }
        for id in lost {
            self.push_event(Event::Lost(id));
        }
        for id in gone {
            if self.is_host() {
                self.drop_peer(id);
            } else {
                self.peers.retain(|p| p.id != id);
                self.push_event(Event::Left(id));
            }
        }
    }

    /// Say goodbye, so the others drop this player at once rather than after [`GRACE`]:
    /// [`BYE_COPIES`] times, [`BYE_GAP`] apart, so one lost packet does not leave the
    /// others waiting (a host that ends the lobby must reach every guest).
    fn bye(&self) {
        for i in 0..BYE_COPIES {
            if i > 0 {
                std::thread::sleep(BYE_GAP);
            }
            match &self.role {
                Role::Host => self.relay_bye(0),
                Role::Guest { host: Some(host), id: Some(id), gone: false, .. } => self.sock_send(&Msg::Bye { id: *id }, *host),
                Role::Guest { .. } => return,
            }
        }
    }
}

/// Whether `a` is a current peer's address: a host's guest, a guest's host.
fn known_addr(role: &Role, peers: &[Peer], a: SocketAddr) -> bool {
    match role {
        Role::Host => peers.iter().any(|p| p.addr == Some(a)),
        Role::Guest { host, .. } => *host == Some(a),
    }
}

/// Count `n` bytes for `to`, keeping at most [`TRAFFIC_ADDRS`] addresses: when a new one
/// comes to a full map, those not of a current peer (`known`) are forgotten first, and
/// if none can be, the new one is not counted.
fn tally(map: &mut HashMap<SocketAddr, u64>, to: SocketAddr, n: u64, known: impl Fn(SocketAddr) -> bool) {
    if !map.contains_key(&to) && map.len() >= TRAFFIC_ADDRS {
        map.retain(|a, _| known(*a));
        if map.len() >= TRAFFIC_ADDRS {
            return;
        }
    }
    *map.entry(to).or_default() += n;
}

/// A [`Session`] on its own thread, so the link keeps beating while the game's frame
/// stalls (a cell loading, a save). Dropping it says Bye and stops the thread.
pub struct Link {
    session: Arc<Mutex<Session>>,
    stop: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
    /// Stop without a Bye ([`Link::abandon`]).
    quiet: bool,
}

impl Link {
    /// Run `session` on a thread of its own; an error if the thread cannot be started.
    pub fn new(session: Session) -> std::io::Result<Self> {
        let session = Arc::new(Mutex::new(session));
        let stop = Arc::new(AtomicBool::new(false));
        let thread = {
            let (session, stop) = (session.clone(), stop.clone());
            std::thread::Builder::new()
                .name("coop-net".into())
                .spawn(move || {
                    while !stop.load(Ordering::Relaxed) {
                        if let Ok(mut s) = session.lock() {
                            s.tick(Instant::now());
                        }
                        std::thread::sleep(TICK);
                    }
                })
                ?
        };
        Ok(Self { session, stop, thread: Some(thread), quiet: false })
    }

    fn with<R>(&self, f: impl FnOnce(&mut Session) -> R) -> R {
        let mut s = self.session.lock().unwrap_or_else(|e| e.into_inner());
        f(&mut s)
    }

    pub fn is_host(&self) -> bool {
        self.with(|s| s.is_host())
    }

    pub fn id(&self) -> Option<u8> {
        self.with(|s| s.id())
    }

    pub fn local_addr(&self) -> std::io::Result<SocketAddr> {
        self.with(|s| s.local_addr())
    }

    /// [`Session::move_host`].
    pub fn move_host(&self, to: SocketAddr) -> bool {
        self.with(|s| s.move_host(to))
    }

    pub fn set_state(&self, state: PlayerState) {
        self.with(|s| s.set_state(state));
    }

    pub fn set_look(&self, look: Vec<u8>) {
        self.with(|s| s.set_look(look));
    }

    pub fn set_lobby(&self, key: u32) {
        self.with(|s| s.set_lobby(key));
    }

    pub fn set_name(&self, name: &str) {
        self.with(|s| s.set_name(name));
    }

    /// [`Session::set_forms`].
    pub fn set_forms(&self, forms: Option<Arc<dyn FormCheck>>) {
        self.with(|s| s.set_forms(forms));
    }

    /// [`Session::add_rule`].
    pub fn add_rule(&self, kind: Kind, rule: Box<dyn Rule>) {
        self.with(|s| s.add_rule(kind, rule));
    }

    /// [`Session::set_rate`].
    pub fn set_rate(&self, kind: Kind, rate: Rate) {
        self.with(|s| s.set_rate(kind, rate));
    }

    /// [`Session::guard_stats`].
    pub fn guard_stats(&self) -> Stats {
        self.with(|s| s.guard_stats())
    }

    /// [`Session::bad_messages`].
    pub fn bad_messages(&self, id: u8) -> u64 {
        self.with(|s| s.bad_messages(id))
    }

    /// [`Session::send_reliable`].
    #[must_use = "a full stream refuses the item"]
    pub fn send_reliable(&self, to: Option<u8>, item: Vec<u8>) -> bool {
        self.with(|s| s.send_reliable(to, item))
    }

    pub fn traffic(&self) -> Vec<Traffic> {
        self.with(|s| s.traffic())
    }

    pub fn events(&self) -> Vec<Event> {
        self.with(|s| s.take_events())
    }

    /// The host's address once one of the candidates answered (guests only).
    pub fn host_addr(&self) -> Option<SocketAddr> {
        self.with(|s| s.host_addr())
    }

    /// Stop without saying Bye: the game starts again (to load a save) and comes back
    /// with the same token before the host even marks it lost.
    pub fn abandon(mut self) {
        self.quiet = true;
    }
}

impl Drop for Link {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
        if !self.quiet {
            self.with(|s| s.bye());
        }
    }
}

/// `a` is newer than `b`, allowing for the counter wrapping.
fn newer(a: u32, b: u32) -> bool {
    a != b && a.wrapping_sub(b) < u32::MAX / 2
}

/// A state to take after `last`: newer, or so much older that its game started again
/// (a load restarts it, and its count with it) rather than a packet late on the way.
fn fresh(seq: u32, last: u32) -> bool {
    newer(seq, last) || last.wrapping_sub(seq) > RESTARTED
}

/// A session token no other guest will have: from the operating system's random source,
/// else std's per-process random hash keys mixed with the time. Never 0.
pub fn new_token() -> u64 {
    if let Some(t) = getrandom::u64().ok().filter(|&t| t != 0) {
        return t;
    }
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    h.write_u32(std::process::id());
    h.finish().max(1)
}

fn put_name(b: &mut Vec<u8>, name: &str) {
    let name = crate::guard::clean_name(name);
    b.push(name.len() as u8);
    b.extend_from_slice(name.as_bytes());
}

fn encode(msg: &Msg) -> Vec<u8> {
    let mut b = Vec::with_capacity(64);
    b.extend_from_slice(&MAGIC);
    b.push(VERSION);
    match msg {
        Msg::Hello { token, lobby, proof, name } => {
            b.push(1);
            b.extend_from_slice(&token.to_le_bytes());
            b.extend_from_slice(&lobby.to_le_bytes());
            b.extend_from_slice(&proof.to_le_bytes());
            put_name(&mut b, name);
        }
        Msg::Welcome { id, proof } => {
            b.extend_from_slice(&[2, *id]);
            b.extend_from_slice(&proof.to_le_bytes());
        }
        Msg::Full { proof } => {
            b.push(3);
            b.extend_from_slice(&proof.to_le_bytes());
        }
        Msg::Who => b.push(6),
        Msg::Ping => b.push(7),
        Msg::Names { names } => {
            let names: Vec<_> = names.iter().take(MAX_GUESTS + 1).collect();
            b.extend_from_slice(&[21, names.len() as u8]);
            for (id, name) in names {
                b.push(*id);
                put_name(&mut b, name);
            }
        }
        Msg::Look { id, look } => {
            let look = &look[..look.len().min(LOOK_MAX)];
            b.extend_from_slice(&[8, *id]);
            b.extend_from_slice(&(look.len() as u16).to_le_bytes());
            b.extend_from_slice(look);
        }
        Msg::State { id, seq, state } => {
            b.extend_from_slice(&[4, *id]);
            b.extend_from_slice(&seq.to_le_bytes());
            let (kind, form) = match state.place {
                Place::Interior(f) => (0u8, f),
                Place::Exterior(f) => (1u8, f),
            };
            b.push(kind);
            b.extend_from_slice(&form.to_le_bytes());
            for v in state.feet.iter().chain([&state.yaw, &state.pitch]) {
                b.extend_from_slice(&v.to_le_bytes());
            }
            b.push(state.flags);
            b.extend_from_slice(&state.weapon.to_le_bytes());
            b.extend_from_slice(&state.attacks.to_le_bytes());
            b.push(state.attack_kind);
            b.extend_from_slice(&state.reloads.to_le_bytes());
            let worn = state.worn();
            b.push(worn.len() as u8);
            for w in worn {
                b.extend_from_slice(&w.to_le_bytes());
            }
            b.push(state.pipboy);
            b.extend_from_slice(&state.furniture.to_le_bytes());
            b.push(state.more);
            let e = &state.emote;
            b.extend_from_slice(&[e.id, e.variant, e.phase, e.step]);
            b.extend_from_slice(&e.t.to_le_bytes());
        }
        Msg::Bye { id } => b.extend_from_slice(&[5, *id]),
        Msg::Rel { epoch, frame } => {
            b.push(30);
            b.extend_from_slice(&epoch.to_le_bytes());
            b.extend_from_slice(&frame.ack.to_le_bytes());
            b.extend_from_slice(&frame.first.to_le_bytes());
            b.extend_from_slice(&(frame.items.len() as u16).to_le_bytes());
            for item in &frame.items {
                b.extend_from_slice(&(item.len() as u16).to_le_bytes());
                b.extend_from_slice(item);
            }
        }
    }
    b
}

/// A message as [`encode`] wrote it; None for anything else. Every list has its bound,
/// and every float read must be finite.
fn decode(b: &[u8]) -> Option<Msg> {
    let rest = b.strip_prefix(&MAGIC)?;
    let (&version, rest) = rest.split_first()?;
    if version != VERSION {
        return None;
    }
    let (&kind, rest) = rest.split_first()?;
    let mut r = Reader(rest);
    let msg = match kind {
        1 => {
            let (token, lobby, proof) = (r.u64()?, r.u32()?, r.u64()?);
            Msg::Hello { token, lobby, proof, name: r.name()? }
        }
        2 => Msg::Welcome { id: r.u8()?, proof: r.u64()? },
        3 => Msg::Full { proof: r.u64()? },
        4 => {
            let id = r.u8()?;
            let seq = r.u32()?;
            let place = match (r.u8()?, r.u32()?) {
                (0, f) => Place::Interior(f),
                (1, f) => Place::Exterior(f),
                _ => return None,
            };
            let feet = [r.finite()?, r.finite()?, r.finite()?];
            let (yaw, pitch) = (r.finite()?, r.finite()?);
            let mut state = PlayerState::at(place, feet, yaw);
            state.pitch = pitch;
            state.flags = r.u8()?;
            state.weapon = r.u32()?;
            state.attacks = r.u16()?;
            state.attack_kind = r.u8()?;
            state.reloads = r.u16()?;
            let n = r.u8()? as usize;
            if n > MAX_WORN {
                return None;
            }
            for w in &mut state.worn[..n] {
                *w = r.u32()?;
            }
            state.worn_len = n as u8;
            state.pipboy = r.u8()?;
            state.furniture = r.u32()?;
            state.more = r.u8()?;
            state.emote = EmoteState { id: r.u8()?, variant: r.u8()?, phase: r.u8()?, step: r.u8()?, t: r.finite()? };
            Msg::State { id, seq, state }
        }
        5 => Msg::Bye { id: r.u8()? },
        6 => Msg::Who,
        7 => Msg::Ping,
        8 => {
            let id = r.u8()?;
            let len = r.u16()? as usize;
            if len > LOOK_MAX {
                return None;
            }
            Msg::Look { id, look: r.take(len)?.to_vec() }
        }
        21 => {
            let n = r.u8()? as usize;
            if n > MAX_GUESTS + 1 {
                return None;
            }
            let mut names = Vec::with_capacity(n);
            for _ in 0..n {
                let id = r.u8()?;
                names.push((id, r.name()?));
            }
            Msg::Names { names }
        }
        30 => {
            let epoch = r.u16()?;
            let ack = r.u32()?;
            let first = r.u32()?;
            let n = r.u16()? as usize;
            if n > REL_ITEMS_MAX {
                return None;
            }
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                let len = r.u16()? as usize;
                if len > ITEM_MAX {
                    return None;
                }
                items.push(r.take(len)?.to_vec());
            }
            Msg::Rel { epoch, frame: Frame { ack, first, items } }
        }
        _ => return None,
    };
    r.0.is_empty().then_some(msg)
}

struct Reader<'a>(&'a [u8]);

impl<'a> Reader<'a> {
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        if self.0.len() < n {
            return None;
        }
        let (head, tail) = self.0.split_at(n);
        self.0 = tail;
        Some(head)
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|b| b[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|b| u16::from_le_bytes(b.try_into().unwrap()))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|b| u32::from_le_bytes(b.try_into().unwrap()))
    }
    fn u64(&mut self) -> Option<u64> {
        self.take(8).map(|b| u64::from_le_bytes(b.try_into().unwrap()))
    }
    /// A float, None unless finite.
    fn finite(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).filter(|v| v.is_finite())
    }
    /// A name: its length byte (at most [`NAME_MAX`]), then its bytes.
    fn name(&mut self) -> Option<String> {
        let len = self.u8()? as usize;
        if len > NAME_MAX {
            return None;
        }
        Some(String::from_utf8_lossy(self.take(len)?).into_owned())
    }
}

#[cfg(test)]
impl Session {
    /// A host on loopback UDP `port` (0 picks a free one), for tests.
    fn host(port: u16, secret: crate::seam::SharedSecret) -> std::io::Result<Self> {
        Ok(Self::host_on(Box::new(crate::seam::loopback::Udp::host(port)?), Box::new(secret)))
    }

    /// A guest on loopback UDP, for tests.
    fn join(candidates: &[SocketAddr], lobby: u32, secret: crate::seam::SharedSecret, name: &str, token: u64) -> std::io::Result<Self> {
        Ok(Self::join_on(Box::new(crate::seam::loopback::Udp::guest()?), Box::new(secret), candidates, lobby, name, token))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seam::memory::{pair, Hub};
    use crate::seam::SharedSecret;

    fn state(x: f32) -> PlayerState {
        PlayerState::at(Place::Exterior(0xA001), [x, 2.0, 3.0], 0.5)
    }

    fn secret() -> SharedSecret {
        SharedSecret::new(0x5EC2_E7AB_CDEF_0123).unwrap()
    }

    #[test]
    fn messages_round_trip() {
        for msg in [
            Msg::Hello { token: 0xDEAD_BEEF_0123, lobby: 0x1234_5678, proof: 0x0123_4567_89AB_CDEF, name: "Courier".into() },
            Msg::Welcome { id: 2, proof: u64::MAX },
            Msg::Full { proof: 7 },
            Msg::Names { names: vec![(0, "Alpha".into()), (2, "Bravo".into()), (3, String::new())] },
            Msg::Who,
            Msg::Ping,
            Msg::Look { id: 2, look: vec![1, 2, 3, 250] },
            Msg::Rel { epoch: 3, frame: Frame { ack: 9, first: u32::MAX, items: vec![vec![1, 2], Vec::new(), vec![7; ITEM_MAX]] } },
            Msg::State { id: 1, seq: 7, state: state(-12.5) },
            Msg::State { id: 0, seq: u32::MAX, state: PlayerState { place: Place::Interior(0xA002), ..state(0.0) } },
            Msg::State {
                id: 2,
                seq: 9,
                state: PlayerState {
                    pitch: -0.3,
                    flags: flag::DRAWN | flag::SIGHTS,
                    weapon: 0xA003,
                    attacks: 65535,
                    attack_kind: attack::POWER_LEFT,
                    reloads: 3,
                    worn: [1, 2, 3, 0, 0, 0, 0, 0, 0, 0],
                    worn_len: 3,
                    pipboy: 2,
                    furniture: 0x0011_C601,
                    more: more_flag::SWIMMING,
                    emote: EmoteState { id: 5, variant: 1, phase: emote::STAY, step: 2, t: 3.25 },
                    ..state(4.0)
                },
            },
            Msg::Bye { id: 3 },
        ] {
            assert_eq!(decode(&encode(&msg)), Some(msg));
        }
    }

    #[test]
    fn bad_packets_are_dropped() {
        let good = encode(&Msg::State { id: 1, seq: 1, state: state(1.0) });
        assert!(decode(&good[..good.len() - 1]).is_none(), "short");
        let mut long = good.clone();
        long.push(0);
        assert!(decode(&long).is_none(), "trailing bytes");
        let mut wrong = good.clone();
        wrong[4] = VERSION + 1;
        assert!(decode(&wrong).is_none(), "other version");
        for bad in [f32::NAN, f32::INFINITY] {
            assert!(decode(&encode(&Msg::State { id: 1, seq: 1, state: state(bad) })).is_none(), "feet not finite");
            assert!(decode(&encode(&Msg::State { id: 1, seq: 1, state: PlayerState { yaw: bad, ..state(0.0) } })).is_none(), "yaw not finite");
            assert!(decode(&encode(&Msg::State { id: 1, seq: 1, state: PlayerState { pitch: bad, ..state(0.0) } })).is_none(), "pitch not finite");
        }
        // The kinds this protocol dropped (the world's) do not parse.
        let mut other = encode(&Msg::Ping);
        other[5] = 9;
        assert!(decode(&other).is_none(), "an unknown kind");
    }

    /// Every list on the wire has its bound: one past it does not parse.
    #[test]
    fn lists_past_their_bounds_do_not_parse() {
        let header = |kind: u8| [&MAGIC[..], &[VERSION, kind]].concat();
        // A name one byte too long.
        let mut hello = header(1);
        hello.extend_from_slice(&[0; 20]);
        hello.push(NAME_MAX as u8 + 1);
        hello.extend_from_slice(&[b'a'; NAME_MAX + 1]);
        assert!(decode(&hello).is_none());
        // Too many names.
        let mut names = header(21);
        names.push(MAX_GUESTS as u8 + 2);
        for i in 0..MAX_GUESTS as u8 + 2 {
            names.extend_from_slice(&[i, 0]);
        }
        assert!(decode(&names).is_none());
        // A look too long.
        let mut look = header(8);
        look.push(1);
        look.extend_from_slice(&(LOOK_MAX as u16 + 1).to_le_bytes());
        look.extend_from_slice(&vec![0; LOOK_MAX + 1]);
        assert!(decode(&look).is_none());
        // Too many worn items.
        let mut s = encode(&Msg::State { id: 1, seq: 1, state: state(1.0) });
        let worn_at = 4 + 1 + 1 + 1 + 4 + 1 + 4 + 20 + 1 + 4 + 2 + 1 + 2;
        assert_eq!(s[worn_at], 0);
        s[worn_at] = MAX_WORN as u8 + 1;
        assert!(decode(&s).is_none());
        // A reliable frame of too many items, or an item too long.
        let rel = |items: Vec<Vec<u8>>| encode(&Msg::Rel { epoch: 0, frame: Frame { ack: 0, first: 0, items } });
        assert!(decode(&rel(vec![Vec::new(); REL_ITEMS_MAX])).is_some());
        assert!(decode(&rel(vec![Vec::new(); REL_ITEMS_MAX + 1])).is_none());
        assert!(decode(&rel(vec![vec![0; ITEM_MAX + 1]])).is_none());
    }

    #[test]
    fn sequence_wraps() {
        assert!(newer(1, 0));
        assert!(newer(0, u32::MAX));
        assert!(!newer(5, 5));
        assert!(!newer(4, 5));
    }

    /// Ticks every session with a shared fake clock until `done` holds (real sockets on
    /// this machine, so packets take a moment to arrive).
    fn run(sessions: &mut [&mut Session], clock: &mut Instant, step: Duration, mut done: impl FnMut(&[Vec<Event>]) -> bool) -> Vec<Vec<Event>> {
        let mut seen: Vec<Vec<Event>> = vec![Vec::new(); sessions.len()];
        for _ in 0..400 {
            *clock += step;
            for (s, seen) in sessions.iter_mut().zip(&mut seen) {
                s.tick(*clock);
                seen.extend(s.take_events());
            }
            if done(&seen) {
                return seen;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        panic!("never settled: {seen:?}");
    }

    fn welcomed(seen: &[Event]) -> bool {
        seen.iter().any(|e| matches!(e, Event::Welcomed(_)))
    }

    fn host_and(n: usize) -> (Session, Vec<Session>, SocketAddr) {
        let host = Session::host(0, secret()).unwrap();
        let addr: SocketAddr = format!("127.0.0.1:{}", host.local_addr().unwrap().port()).parse().unwrap();
        let guests = (0..n).map(|i| Session::join(&[addr], 0, secret(), &format!("G{i}"), new_token()).unwrap()).collect();
        (host, guests, addr)
    }

    fn addr(n: u8) -> SocketAddr {
        SocketAddr::from(([10, 0, 0, n], 47_960))
    }

    /// A host and a guest over the in-memory transport: the guest is welcomed, and each
    /// sees the other's state and name.
    #[test]
    fn a_guest_joins_over_a_transport_of_its_own() {
        let (h, g) = pair(addr(1), addr(2));
        let mut host = Session::host_on(Box::new(h), Box::new(secret()));
        let mut guest = Session::join_on(Box::new(g), Box::new(secret()), &[addr(1)], 0, "Guest", 42);
        host.set_name("Host");
        host.set_state(state(1.0));
        guest.set_state(state(2.0));
        let mut clock = Instant::now();
        let seen = run(&mut [&mut host, &mut guest], &mut clock, Duration::from_millis(10), |seen| {
            seen[0].iter().any(|e| matches!(e, Event::State { id: 1, .. })) && seen[1].iter().any(|e| matches!(e, Event::State { id: 0, .. })) && seen[1].iter().any(|e| matches!(e, Event::Named { id: 0, .. }))
        });
        assert!(seen[0].contains(&Event::Joined { id: 1, name: "Guest".into() }));
        assert!(seen[1].contains(&Event::Welcomed(1)));
        assert!(seen[1].contains(&Event::Named { id: 0, name: "Host".into() }));
        assert_eq!(guest.host_addr(), Some(addr(1)));
    }

    /// A Hello with the wrong secret, or none, is never answered.
    #[test]
    fn a_wrong_or_zero_secret_is_not_let_in() {
        struct Zero;
        impl Admission for Zero {
            fn hello(&self) -> u64 {
                0
            }
            fn admits(&self, _: u64) -> bool {
                false
            }
            fn answer(&self, _: u64, _: Option<u8>) -> u64 {
                0
            }
        }
        let hub = Hub::new();
        let mut host = Session::host_on(Box::new(hub.endpoint(addr(1))), Box::new(secret()));
        let wrong = SharedSecret::new(secret().value() ^ 1).unwrap();
        let mut a = Session::join_on(Box::new(hub.endpoint(addr(2))), Box::new(wrong), &[addr(1)], 0, "A", 1);
        let mut b = Session::join_on(Box::new(hub.endpoint(addr(3))), Box::new(Zero), &[addr(1)], 0, "B", 2);
        let mut clock = Instant::now();
        for _ in 0..200 {
            clock += Duration::from_millis(10);
            for s in [&mut host, &mut a, &mut b] {
                s.tick(clock);
            }
        }
        assert!(host.peers.is_empty());
        assert!(host.take_events().is_empty());
        assert_eq!(a.id(), None);
        assert_eq!(b.id(), None);
        assert!(host.guard_stats().dropped.iter().any(|&(k, r, n)| k == Kind::Hello && r == Reason::Secret && n > 0));
    }

    /// A Welcome that does not prove the secret (from a stranger, or a host of another
    /// lobby) is dropped; the guest stays unanswered and takes no host.
    #[test]
    fn a_forged_welcome_is_dropped() {
        let hub = Hub::new();
        let forger = hub.endpoint(addr(9));
        let mut guest = Session::join_on(Box::new(hub.endpoint(addr(2))), Box::new(secret()), &[addr(1)], 0, "A", 77);
        let other = SharedSecret::new(1).unwrap();
        let mut clock = Instant::now();
        for proof in [0, 12345, other.answer(77, Some(1)), secret().answer(78, Some(1))] {
            forger.send_to(&encode(&Msg::Welcome { id: 1, proof }), addr(2)).unwrap();
            forger.send_to(&encode(&Msg::Full { proof }), addr(2)).unwrap();
            clock += Duration::from_millis(10);
            guest.tick(clock);
        }
        assert_eq!(guest.id(), None);
        assert_eq!(guest.host_addr(), None);
        assert!(guest.take_events().is_empty());
        // The real proof is taken.
        forger.send_to(&encode(&Msg::Welcome { id: 1, proof: secret().answer(77, Some(1)) }), addr(2)).unwrap();
        guest.tick(clock + Duration::from_millis(10));
        assert_eq!(guest.id(), Some(1));
        // A known host's forged Welcome later is dropped too.
        forger.send_to(&encode(&Msg::Welcome { id: 3, proof: 5 }), addr(2)).unwrap();
        guest.tick(clock + Duration::from_millis(20));
        assert_eq!(guest.id(), Some(1));
    }

    /// One address never holds two slots: Hellos with new tokens from the same address take
    /// the one slot over, they do not fill the lobby.
    #[test]
    fn hellos_with_new_tokens_from_one_address_keep_one_slot() {
        let hub = Hub::new();
        let mut host = Session::host_on(Box::new(hub.endpoint(addr(1))), Box::new(secret()));
        let raw = hub.endpoint(addr(2));
        let mut clock = Instant::now();
        for token in 1..=(MAX_GUESTS as u64 + 3) {
            raw.send_to(&encode(&Msg::Hello { token, lobby: 0, proof: secret().hello(), name: "A".into() }), addr(1)).unwrap();
            clock += Duration::from_millis(10);
            host.tick(clock);
        }
        assert_eq!(host.peers.len(), 1);
        // Another address is another guest.
        let other = hub.endpoint(addr(3));
        other.send_to(&encode(&Msg::Hello { token: 99, lobby: 0, proof: secret().hello(), name: "B".into() }), addr(1)).unwrap();
        host.tick(clock + Duration::from_millis(10));
        assert_eq!(host.peers.len(), 2);
    }

    /// A guest's token is honoured only with the lobby's secret: a stranger holding a
    /// known token but the wrong secret does not take the guest's slot or address.
    #[test]
    fn a_known_token_with_the_wrong_secret_is_not_let_back() {
        let hub = Hub::new();
        let mut host = Session::host_on(Box::new(hub.endpoint(addr(1))), Box::new(secret()));
        let mut a = Session::join_on(Box::new(hub.endpoint(addr(2))), Box::new(secret()), &[addr(1)], 0, "A", 555);
        let mut clock = Instant::now();
        run(&mut [&mut host, &mut a], &mut clock, Duration::from_millis(10), |seen| welcomed(&seen[1]));
        let wrong = SharedSecret::new(9).unwrap();
        let mut thief = Session::join_on(Box::new(hub.endpoint(addr(3))), Box::new(wrong), &[addr(1)], 0, "A", 555);
        for _ in 0..100 {
            clock += Duration::from_millis(10);
            host.tick(clock);
            thief.tick(clock);
        }
        assert_eq!(thief.id(), None);
        assert_eq!(host.peers.len(), 1);
        assert_eq!(host.peers[0].addr, Some(addr(2)));
        assert!(!host.take_events().iter().any(|e| matches!(e, Event::RelReset { .. } | Event::Back(_))));
    }

    /// The host's checks run before it acts: a guest's host-only messages are dropped
    /// and counted against it, a teleporting state is not passed on, and the sender is
    /// known by its address, not by the id its message names.
    #[test]
    fn the_host_checks_its_guests_messages() {
        let hub = Hub::new();
        let mut host = Session::host_on(Box::new(hub.endpoint(addr(1))), Box::new(secret()));
        let mut a = Session::join_on(Box::new(hub.endpoint(addr(2))), Box::new(secret()), &[addr(1)], 0, "A", 1);
        let mut b = Session::join_on(Box::new(hub.endpoint(addr(3))), Box::new(secret()), &[addr(1)], 0, "B", 2);
        let mut clock = Instant::now();
        a.set_state(state(1.0));
        run(&mut [&mut host, &mut a, &mut b], &mut clock, Duration::from_millis(10), |seen| welcomed(&seen[1]) && welcomed(&seen[2]) && seen[2].iter().any(|e| matches!(e, Event::State { id: 1, .. })));
        // Guest 1's address now leads to this raw endpoint.
        let raw = hub.endpoint(addr(2));
        for msg in [Msg::Welcome { id: 1, proof: 0 }, Msg::Full { proof: 0 }, Msg::Who, Msg::Names { names: vec![(0, "Me".into())] }] {
            raw.send_to(&encode(&msg), addr(1)).unwrap();
        }
        raw.send_to(b"not a packet", addr(1)).unwrap();
        // A state naming guest 2, far away in the same place.
        raw.send_to(&encode(&Msg::State { id: 2, seq: 1000, state: state(90_000.0) }), addr(1)).unwrap();
        // A plausible one naming guest 2: it is guest 1's.
        raw.send_to(&encode(&Msg::State { id: 2, seq: 1001, state: state(5.0) }), addr(1)).unwrap();
        clock += Duration::from_millis(10);
        host.tick(clock);
        let events = host.take_events();
        assert!(events.contains(&Event::State { id: 1, state: state(5.0) }), "{events:?}");
        assert!(!events.iter().any(|e| matches!(e, Event::State { id, state } if *id == 2 || state.feet[0] > 1000.0)), "{events:?}");
        assert_eq!(host.bad_messages(1), 6, "{:?}", host.guard_stats());
        assert_eq!(host.bad_messages(2), 0);
        clock += Duration::from_millis(10);
        b.tick(clock);
        let at_b = b.take_events();
        assert!(!at_b.iter().any(|e| matches!(e, Event::State { state, .. } if state.feet[0] > 1000.0)), "{at_b:?}");
    }

    /// A rule a layer above adds runs on each reliable item by its kind (its first byte),
    /// after the stream delivered it and before the game has it.
    #[test]
    fn rules_judge_each_reliable_item_by_its_kind() {
        struct NoSevens;
        impl Rule for NoSevens {
            fn check(&self, _: &crate::guard::Ctx, subject: &crate::guard::Subject) -> Verdict {
                match subject {
                    crate::guard::Subject::Item(item) if item.get(1) == Some(&7) => Verdict::Drop(Reason::Rule("no sevens")),
                    _ => Verdict::Accept,
                }
            }
        }
        let (h, g) = pair(addr(1), addr(2));
        let mut host = Session::host_on(Box::new(h), Box::new(secret()));
        host.add_rule(Kind::Item(5), Box::new(NoSevens));
        let mut a = Session::join_on(Box::new(g), Box::new(secret()), &[addr(1)], 0, "A", 1);
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| welcomed(&seen[1]));
        // The guest's stream goes once it has heard the host's.
        assert!(host.send_reliable(Some(1), vec![1]));
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Reliable { .. })));
        for item in [vec![5, 1], vec![5, 7], vec![6, 7], vec![5, 2]] {
            assert!(a.send_reliable(None, item));
        }
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[0].iter().filter(|e| matches!(e, Event::Reliable { .. })).count() == 3);
        let items: Vec<Vec<u8>> = seen[0].iter().filter_map(|e| if let Event::Reliable { item, .. } = e { Some(item.clone()) } else { None }).collect();
        assert_eq!(items, vec![vec![5, 1], vec![6, 7], vec![5, 2]]);
        assert!(host.guard_stats().dropped.contains(&(Kind::Item(5), Reason::Rule("no sevens"), 1)));
        assert_eq!(host.bad_messages(1), 1);
    }

    /// The per-address byte counts keep to [`TRAFFIC_ADDRS`] however many addresses send,
    /// keeping the guests'.
    #[test]
    fn traffic_counts_stay_bounded() {
        let hub = Hub::new();
        let mut host = Session::host_on(Box::new(hub.endpoint(addr(1))), Box::new(secret()));
        let mut a = Session::join_on(Box::new(hub.endpoint(addr(2))), Box::new(secret()), &[addr(1)], 0, "A", 1);
        let mut clock = Instant::now();
        run(&mut [&mut host, &mut a], &mut clock, Duration::from_millis(10), |seen| welcomed(&seen[1]));
        for i in 0..500u32 {
            let from = hub.endpoint(SocketAddr::from(([10, 9, (i >> 8) as u8, i as u8], 1)));
            from.send_to(b"noise", addr(1)).unwrap();
            from.send_to(&encode(&Msg::Ping), addr(1)).unwrap();
        }
        clock += Duration::from_millis(10);
        host.tick(clock);
        a.tick(clock);
        host.tick(clock);
        assert!(host.heard.len() <= TRAFFIC_ADDRS && host.sent.borrow().len() <= TRAFFIC_ADDRS);
        assert!(host.heard.contains_key(&addr(2)));
        assert!(host.traffic().iter().any(|t| t.id == 1 && t.heard > 0 && t.sent > 0));
    }

    /// Waiting events stay bounded when the game takes none: a player's state replaces
    /// the one before it.
    #[test]
    fn a_waiting_state_is_replaced_by_the_newer() {
        let (h, _) = pair(addr(1), addr(2));
        let mut s = Session::host_on(Box::new(h), Box::new(secret()));
        s.push_event(Event::Joined { id: 1, name: String::new() });
        for i in 0..10_000 {
            s.push_event(Event::State { id: 1, state: state(i as f32) });
            s.push_event(Event::Look { id: 1, look: vec![i as u8] });
        }
        assert_eq!(s.events.len(), 3);
        assert_eq!(s.events[1], Event::State { id: 1, state: state(9_999.0) });
        // After the player left and came back, a new state does not jump ahead of that.
        s.push_event(Event::Left(1));
        s.push_event(Event::State { id: 1, state: state(1.0) });
        assert_eq!(s.events.last(), Some(&Event::State { id: 1, state: state(1.0) }));
        for _ in 0..EVENTS_MAX * 2 {
            s.push_event(Event::Lost(2));
        }
        assert_eq!(s.events.len(), EVENTS_MAX);
        assert!(s.events_dropped() > 0);
    }

    /// The reliable streams: the host's items reach a guest once and in order, the
    /// guest's reach the host, and the traffic is counted.
    #[test]
    fn the_reliable_stream_reaches_its_guest() {
        let (mut host, mut guests, _) = host_and(2);
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| welcomed(&seen[1]) && welcomed(&seen[2]));
        assert!(seen[0].iter().filter(|e| matches!(e, Event::RelReset { .. })).count() >= 2);
        let ida = a.id().unwrap();
        for i in 0..50u8 {
            assert!(host.send_reliable(Some(ida), vec![7, i]));
        }
        assert!(host.send_reliable(None, vec![8]));
        assert!(a.send_reliable(None, vec![9, 9]));
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| {
            seen[1].iter().filter(|e| matches!(e, Event::Reliable { .. })).count() == 51 && seen[0].iter().any(|e| matches!(e, Event::Reliable { .. })) && seen[2].iter().any(|e| matches!(e, Event::Reliable { .. }))
        });
        let at_a: Vec<Vec<u8>> = seen[1].iter().filter_map(|e| if let Event::Reliable { id: 0, item } = e { Some(item.clone()) } else { None }).collect();
        let mut want: Vec<Vec<u8>> = (0..50u8).map(|i| vec![7, i]).collect();
        want.push(vec![8]);
        assert_eq!(at_a, want);
        assert!(seen[0].iter().any(|e| *e == Event::Reliable { id: ida, item: vec![9, 9] }));
        assert!(seen[2].iter().any(|e| *e == Event::Reliable { id: 0, item: vec![8] }));
        let t = host.traffic();
        assert!(t.iter().any(|t| t.id == ida && t.sent > 0 && t.heard > 0));
        // Everything is acknowledged in the end.
        for _ in 0..40 {
            clock += step;
            for s in [&mut host, &mut *a, &mut *b] {
                s.tick(clock);
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(host.traffic().iter().all(|t| t.pending == 0), "{:?}", host.traffic());
    }

    #[test]
    fn guests_learn_every_players_name() {
        let (mut host, mut guests, _) = host_and(2);
        host.set_name("Alpha");
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        let named = |v: &[Event]| v.iter().filter_map(|e| if let Event::Named { id, name } = e { Some((*id, name.clone())) } else { None }).collect::<Vec<_>>();
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| named(&seen[1]).len() >= 2 && named(&seen[2]).len() >= 2);
        let (ida, idb) = (a.id().unwrap(), b.id().unwrap());
        let sorted = |mut v: Vec<(u8, String)>| {
            v.sort();
            v
        };
        assert_eq!(sorted(named(&seen[1])), sorted(vec![(0, "Alpha".into()), (idb, "G1".into())]));
        assert_eq!(sorted(named(&seen[2])), sorted(vec![(0, "Alpha".into()), (ida, "G0".into())]));
        assert!(named(&seen[0]).is_empty(), "the host has the names from the guests' Hello");
    }

    #[test]
    fn a_host_answers_only_its_own_lobby() {
        let mut other = Session::host(0, secret()).unwrap();
        other.set_lobby(7);
        let mut mine = Session::host(0, secret()).unwrap();
        mine.set_lobby(9);
        let at = |s: &Session| -> SocketAddr { format!("127.0.0.1:{}", s.local_addr().unwrap().port()).parse().unwrap() };
        let mut a = Session::join(&[at(&other), at(&mine)], 9, secret(), "A", new_token()).unwrap();
        let mut clock = Instant::now();
        run(&mut [&mut other, &mut mine, &mut a], &mut clock, Duration::from_millis(10), |seen| welcomed(&seen[2]));
        assert_eq!(a.host_addr(), Some(at(&mine)));
        assert!(other.peers.is_empty());
    }

    /// A lobby's first address may lead nowhere: the guest joins through the one that
    /// answers.
    #[test]
    fn a_guest_joins_through_the_address_that_answers() {
        let mut host = Session::host(0, secret()).unwrap();
        let port = host.local_addr().unwrap().port();
        let dead: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let live: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut a = Session::join(&[dead, live], 0, secret(), "A", new_token()).unwrap();
        let mut clock = Instant::now();
        run(&mut [&mut host, &mut a], &mut clock, Duration::from_millis(10), |seen| welcomed(&seen[1]));
        assert_eq!(a.host_addr(), Some(live));
    }

    /// Each player's look reaches the others, the host's too, once each until it changes.
    #[test]
    fn looks_reach_everyone_once() {
        let (mut host, mut g, _) = host_and(2);
        let mut b = g.pop().unwrap();
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| seen[1..].iter().all(|s| welcomed(s)));
        let ida = a.id().unwrap();
        host.set_look(vec![0, 7]);
        a.set_look(vec![1, 2, 3]);
        let seen = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| {
            seen[0].contains(&Event::Look { id: ida, look: vec![1, 2, 3] })
                && seen[1].contains(&Event::Look { id: 0, look: vec![0, 7] })
                && seen[2].contains(&Event::Look { id: ida, look: vec![1, 2, 3] })
                && seen[2].contains(&Event::Look { id: 0, look: vec![0, 7] })
        });
        // Sent again every LOOK_EVERY, reported once.
        let mut more = seen;
        for _ in 0..3 {
            clock += LOOK_EVERY;
            for s in [&mut host, &mut a, &mut b] {
                s.tick(clock);
            }
            std::thread::sleep(Duration::from_millis(5));
            for (s, seen) in [&mut host, &mut a, &mut b].into_iter().zip(more.iter_mut()) {
                s.tick(clock);
                seen.extend(s.take_events());
            }
        }
        let looks = more[2].iter().filter(|e| matches!(e, Event::Look { .. })).count();
        assert_eq!(looks, 2, "{:?}", more[2]);
    }

    /// A game started again counts its states from 1: they are taken, not dropped as late.
    #[test]
    fn a_restarted_count_is_fresh() {
        assert!(fresh(8, 7) && !fresh(7, 8) && !fresh(7, 7));
        assert!(fresh(1, 5000));
        assert!(fresh(0, u32::MAX));
    }

    /// A guest whose packets stop is lost, not dropped, and gets its own id back when it
    /// returns from a new address (a new socket, as after a router changed its port).
    #[test]
    fn a_quiet_guest_keeps_its_slot_and_comes_back() {
        let (mut host, mut g, addr) = host_and(1);
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        a.set_state(state(1.0));
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[0].iter().any(|e| matches!(e, Event::State { .. })));
        let id = a.id().unwrap();

        // The guest goes silent past LOST_AFTER: only the host ticks.
        clock += LOST_AFTER + Duration::from_secs(1);
        host.tick(clock);
        let events = host.take_events();
        assert!(events.contains(&Event::Lost(id)), "{events:?}");
        assert!(!events.contains(&Event::Left(id)), "dropped too soon: {events:?}");

        // It comes back on a fresh socket with the same token.
        let Role::Guest { token, .. } = a.role else { unreachable!() };
        let mut back = Session::join(&[addr], 0, secret(), "G0", token).unwrap();
        drop(a);
        back.set_state(state(2.0));
        let seen = run(&mut [&mut host, &mut back], &mut clock, step, |seen| seen[0].contains(&Event::Back(id)) && seen[0].contains(&Event::State { id, state: state(2.0) }));
        assert_eq!(back.id(), Some(id), "same slot");
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Joined { .. })), "not a new guest: {:?}", seen[0]);
    }

    /// The host's game closed and started again (a new session on the same port, with
    /// the same lobby, knowing nobody) while a guest stayed in. The guest says Hello, is
    /// welcomed as the same player, and hears the new host's reliable items, and the new
    /// host hears its own.
    #[test]
    fn a_guest_back_with_a_host_started_again_hears_its_stream() {
        let (mut host, mut g, addr) = host_and(1);
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| welcomed(&seen[1]));
        let id = a.id().unwrap();
        // The old host's stream, well along.
        for i in 0..30u8 {
            assert!(host.send_reliable(Some(id), vec![1, i]));
        }
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().filter(|e| matches!(e, Event::Reliable { .. })).count() == 30);

        // The host's game closes; another starts on the same port.
        drop(host);
        let mut host = Session::host(addr.port(), secret()).unwrap();
        clock += LOST_AFTER + Duration::from_secs(1);
        a.tick(clock);
        assert!(a.take_events().contains(&Event::HostLost));
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].contains(&Event::HostBack) && seen[0].iter().any(|e| matches!(e, Event::Joined { .. })));
        assert_eq!(a.id(), Some(id), "the same player again");

        for i in 0..5u8 {
            assert!(host.send_reliable(Some(id), vec![2, i]));
        }
        assert!(a.send_reliable(None, vec![9]));
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| {
            seen[1].iter().filter(|e| matches!(e, Event::Reliable { item, .. } if item[0] == 2)).count() == 5 && seen[0].iter().any(|e| *e == Event::Reliable { id, item: vec![9] })
        });
        let items: Vec<Vec<u8>> = seen[1].iter().filter_map(|e| if let Event::Reliable { item, .. } = e { Some(item.clone()) } else { None }).collect();
        assert_eq!(items, (0..5u8).map(|i| vec![2, i]).collect::<Vec<_>>(), "the new host's items, once and in order");
        assert!(seen[1].iter().any(|e| matches!(e, Event::RelReset { id: 0 })), "the guest starts the host's stream again");
    }

    /// A guest that stops hearing the host reports it lost, keeps saying Hello, and
    /// reports it back when the host answers; past GRACE it gives up.
    #[test]
    fn a_guest_rides_out_a_quiet_host() {
        let (mut host, mut g, _) = host_and(1);
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        host.set_state(state(5.0));
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::State { .. })));

        clock += LOST_AFTER + Duration::from_secs(1);
        a.tick(clock);
        let events = a.take_events();
        assert!(events.contains(&Event::HostLost) && events.contains(&Event::Lost(0)), "{events:?}");
        assert!(!events.contains(&Event::HostGone), "{events:?}");

        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].contains(&Event::HostBack) && seen[1].contains(&Event::Back(0)));

        clock += GRACE + Duration::from_secs(1);
        a.tick(clock);
        assert!(a.take_events().contains(&Event::HostGone));
    }
}
