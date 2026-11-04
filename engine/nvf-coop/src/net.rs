//! The co-op wire: one UDP socket (and Steam, [`crate::steam`], when the game has it), a
//! host and up to [`MAX_GUESTS`] guests. A peer over Steam has a stand-in address
//! ([`crate::steam::addr`]): everything here is the same for it.
//!
//! The host is the hub: each game sends its own player's [`PlayerState`] to the host
//! [`SEND_HZ`] times a second, and the host passes every guest's state on to the other
//! guests along with its own (peer 0). Nothing here knows about Bevy.
//!
//! **Dropouts are not disconnects.** A peer that goes quiet for [`LOST_AFTER`] is only
//! *lost*: its slot, id and player stay, and it is dropped only after [`GRACE`]. A guest
//! that stops hearing the host keeps sending Hello with its session token, and the host
//! gives a known token its old id back, from whatever address it now comes from (a
//! Wi-Fi blip, a router that changed the port). Only a deliberate Bye drops a peer at once.
//!
//! **The host runs the world.** The host's NPCs are the only ones that think: it sends
//! each one it draws ([`NpcState`]) [`NPC_HZ`] times a second, and the guests draw
//! theirs there. A guest's hits on NPCs go to the host ([`Session::send_hit`]), which
//! applies them in its world; what they do (a fight, a death) comes back in the states.
//! The NPCs' hits on a guest go the other way ([`Session::send_hit_to`]). The host's
//! active quest goes to the guests too ([`QuestView`], [`QUEST_EVERY`]), so does its game
//! clock ([`ClockView`], [`CLOCK_EVERY`]: one time of day in every game), and an unlocked
//! door or terminal goes to everyone ([`Session::send_unlock`]). A player another revives
//! is told so through the host ([`Session::send_revive`]).
//!
//! **Loading screens are not dropouts either.** [`Link`] runs the socket on its own
//! thread, so heartbeats keep going while the game's frame is stalled loading a cell; the
//! game only hands it its newest state and takes the events.
//!
//! Packets are little-endian: the magic `NVCO`, the protocol version, a kind byte, then
//! the kind's fields. A packet that does not parse is dropped.

use std::collections::VecDeque;
use std::hash::{BuildHasher, Hasher};
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};

use crate::steam::Steam;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MAGIC: [u8; 4] = *b"NVCO";
pub const VERSION: u8 = 8;
pub const MAX_GUESTS: usize = 3;
pub const SEND_HZ: u32 = 20;
/// A peer that sends nothing for this long is lost (shown frozen, still in its slot).
pub const LOST_AFTER: Duration = Duration::from_secs(4);
/// A lost peer that has not come back after this long is dropped, and a guest stops trying.
pub const GRACE: Duration = Duration::from_secs(120);
/// How often a guest sends Hello while it has no answer.
const HELLO_EVERY: Duration = Duration::from_millis(500);
/// The network thread's tick.
const TICK: Duration = Duration::from_millis(10);
const NAME_MAX: usize = 32;

/// Where a player is: an interior cell, or a worldspace (exterior), by FormID in the
/// game's load order. Both games must run the same load order for the IDs to agree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Interior(u32),
    Exterior(u32),
}

/// Worn items sent with a state, at most.
/// The most a look takes (the game's own encoding, [`Session::set_look`]), and how often it
/// is sent again (it changes rarely; a lost one is made up for by the next).
pub const LOOK_MAX: usize = 1200;
pub const LOOK_EVERY: Duration = Duration::from_secs(2);
/// States this far behind the last are from a game started again (10 s of them).
const RESTARTED: u32 = 200;
pub const MAX_WORN: usize = 10;

/// An NPC of the host's world as the host draws it (module notes).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct NpcState {
    /// The placed reference: the same in every game, as they run the same load order.
    pub reference: u32,
    /// Feet position, game units, Z up.
    pub feet: [f32; 3],
    /// Facing, radians (compass: 0 north, π/2 east).
    pub heading: f32,
    /// Its aim up or down while it fights, radians.
    pub pitch: f32,
    /// [`npc_flag`] bits.
    pub flags: u8,
    /// The weapon it aims (WEAP FormID), 0 for none.
    pub weapon: u32,
    /// Attacks begun (wrapping), counted as [`PlayerState::attacks`] are.
    pub attacks: u8,
}

impl NpcState {
    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }
}

/// [`NpcState::flags`] bits.
pub mod npc_flag {
    pub const MOVING: u8 = 1;
    /// The weapon is out.
    pub const DRAWN: u8 = 2;
    pub const SNEAKING: u8 = 4;
    pub const DEAD: u8 = 8;
    /// Fighting: aiming at someone.
    pub const AIMING: u8 = 16;
    pub const SIGHTS: u8 = 32;
}

/// NPC states in one packet (30 bytes each: under 1210 bytes in all).
pub const NPCS_PER_PACKET: usize = 40;
/// How often the host sends its NPCs.
pub const NPC_HZ: u32 = 10;
/// The host sends its active quest this often, and at once when it changes.
pub const QUEST_EVERY: Duration = Duration::from_secs(2);
/// How often the host sends its game clock.
pub const CLOCK_EVERY: Duration = Duration::from_secs(2);
/// Objectives a [`QuestView`] carries at most.
pub const QUEST_OBJECTIVES: usize = 64;
/// The most a hit takes (the game's own encoding, [`Session::send_hit`]).
pub const HIT_MAX: usize = 64;
/// Each hit is sent this many times (the host takes it once, by its id).
const HIT_COPIES: usize = 3;
/// Hit ids a host remembers per guest, to take each hit once.
const HITS_KEPT: usize = 64;

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
}

impl PlayerState {
    /// A state with nothing in hand and nothing worn.
    pub fn at(place: Place, feet: [f32; 3], yaw: f32) -> Self {
        Self { place, feet, yaw, pitch: 0.0, flags: 0, weapon: 0, attacks: 0, attack_kind: 0, reloads: 0, worn: [0; MAX_WORN], worn_len: 0 }
    }

    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }

    pub fn worn(&self) -> &[u32] {
        &self.worn[..(self.worn_len as usize).min(MAX_WORN)]
    }
}

#[derive(Clone, Debug, PartialEq)]
enum Msg {
    /// `lobby` is the code's [`crate::code::Lobby::key`] (0 for a guest that came by
    /// address): a host answers only its own, as a guest asks its whole network.
    Hello { token: u64, lobby: u32, name: String },
    Welcome { id: u8 },
    Full,
    /// The host does not know the sender: say Hello again.
    Who,
    State { id: u8, seq: u32, state: PlayerState },
    /// "Still here", from a game with no state to send (a guest choosing its save).
    Ping,
    Bye { id: u8 },
    /// How player `id` looks (race, face, hair...; the game's own encoding).
    Look { id: u8, look: Vec<u8> },
    /// Some of the host's NPCs (all of one round share its `seq`).
    Npcs { seq: u32, npcs: Vec<NpcState> },
    /// A guest's hit on an NPC, for the host to apply (the game's own encoding).
    Hit { id: u16, hit: Vec<u8> },
    /// The host's active quest.
    Quest(QuestView),
    /// A door, container or terminal (`terminal`) was unlocked.
    Unlock { reference: u32, terminal: bool },
    /// The host's game clock.
    Clock(ClockView),
    /// Player `by` revived player `target` (who is down).
    Revive { target: u8, by: u8 },
}

/// The host's game clock, as its time globals hold it (GameYear, GameMonth, GameDay,
/// GameHour, GameDaysPassed): the guests set theirs to it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct ClockView {
    pub year: f32,
    pub month: f32,
    pub day: f32,
    pub hour: f32,
    pub days_passed: f32,
}

/// The quest the host has active, as its quest list shows it: the guests show it in place
/// of their own. `quest` 0: none.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct QuestView {
    pub quest: u32,
    pub completed: bool,
    pub failed: bool,
    /// The objectives shown or done, by index, with whether each is done.
    pub objectives: Vec<(i32, bool)>,
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
    /// Some of the host's NPCs, as it draws them (guests only).
    Npcs(Vec<NpcState>),
    /// Guest `id` hit an NPC (host only; each hit once).
    Hit { id: u8, hit: Vec<u8> },
    /// The host's active quest, when first heard or changed (guests only).
    Quest(QuestView),
    /// Player `id` unlocked a door, container or terminal (`terminal`). It may come more
    /// than once.
    Unlocked { id: u8, reference: u32, terminal: bool },
    /// The host's game clock, each time it comes (guests only).
    Clock(ClockView),
    /// Player `by` revived this player. It may come more than once.
    Revived { by: u8 },
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
    /// The ids of a guest's newest hits (host only).
    hits: VecDeque<u16>,
}

impl Peer {
    fn new(id: u8, now: Instant) -> Self {
        Self { id, addr: None, token: 0, last_heard: now, last_seq: None, lost: false, look: None, hits: VecDeque::new() }
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
        /// Where the host may be (its lobby code's address, this PC, the network's
        /// broadcast addresses); Hello goes to each until the host answers, from whatever
        /// address it has, and that one is `host`.
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
    sock: UdpSocket,
    /// Steam, for peers with a stand-in address ([`Session::over_steam`]).
    steam: Option<&'static Steam>,
    role: Role,
    /// The host's guests, or the other peers a guest has heard of (the host among them).
    peers: Vec<Peer>,
    state: Option<PlayerState>,
    seq: u32,
    last_send: Option<Instant>,
    look: Option<Vec<u8>>,
    last_look: Option<Instant>,
    /// A host's lobby key, once it has a code: Hellos for another lobby are not answered.
    lobby: Option<u32>,
    /// A host's NPCs to send, the last time they went and the round's number.
    npcs: Vec<NpcState>,
    last_npcs: Option<Instant>,
    npc_seq: u32,
    /// A guest's newest round of NPCs heard, and its next hit's id.
    heard_npcs: Option<u32>,
    hit_seq: u16,
    /// The ids of the host's newest hits on this guest (guest only).
    heard_hits: VecDeque<u16>,
    /// The host's active quest, and when it was last sent; a guest's last heard.
    quest: Option<QuestView>,
    last_quest: Option<(Instant, QuestView)>,
    heard_quest: Option<QuestView>,
    /// The host's game clock now, and when it was last sent.
    clock: Option<ClockView>,
    last_clock: Option<Instant>,
    events: Vec<Event>,
}

impl Session {
    /// Host on `port` (all interfaces; 0 picks a free one).
    pub fn host(port: u16) -> std::io::Result<Self> {
        Self::with(UdpSocket::bind(("0.0.0.0", port))?, Role::Host)
    }

    /// Join the host at the first of `candidates` that answers; `lobby` is the code's key
    /// (0 for none), `name` is shown to the others. `token` names this guest to the host
    /// across dropouts and restarts ([`new_token`] for a new one).
    pub fn join(candidates: &[SocketAddr], lobby: u32, name: &str, token: u64) -> std::io::Result<Self> {
        let any = if candidates.iter().all(|c| c.is_ipv6()) && !candidates.is_empty() { "[::]:0" } else { "0.0.0.0:0" };
        let role = Role::Guest {
            candidates: candidates.to_vec(),
            host: None,
            token,
            lobby,
            name: truncate(name),
            id: None,
            last_heard: Instant::now(),
            last_hello: None,
            host_lost: false,
            asked: false,
            gone: false,
        };
        let sock = UdpSocket::bind(any)?;
        // A host on the guest's own network is asked by broadcast.
        let _ = sock.set_broadcast(true);
        Self::with(sock, role)
    }

    fn with(sock: UdpSocket, role: Role) -> std::io::Result<Self> {
        sock.set_nonblocking(true)?;
        Ok(Self { sock, steam: None, role, peers: Vec::new(), state: None, seq: 0, last_send: None, look: None, last_look: None, lobby: None, npcs: Vec::new(), last_npcs: None, npc_seq: 0, heard_npcs: None, hit_seq: 0, heard_hits: VecDeque::new(), quest: None, last_quest: None, heard_quest: None, clock: None, last_clock: None, events: Vec::new() })
    }

    /// Also talk over Steam: a host is reached through it by a Steam code, a guest reaches
    /// one.
    pub fn over_steam(mut self, steam: &'static Steam) -> Self {
        self.steam = Some(steam);
        self
    }

    /// The host's NPCs as it draws them now; [`Session::tick`] sends them at [`NPC_HZ`].
    pub fn set_npcs(&mut self, npcs: Vec<NpcState>) {
        self.npcs = npcs;
    }

    /// A guest's hit on an NPC, for the host to apply (at once, [`HIT_COPIES`] times; one
    /// longer than [`HIT_MAX`], or before the host answered, is not sent).
    pub fn send_hit(&mut self, hit: Vec<u8>) {
        let Role::Guest { host: Some(host), .. } = self.role else { return };
        if hit.len() > HIT_MAX {
            return;
        }
        self.hit_seq = self.hit_seq.wrapping_add(1);
        let msg = Msg::Hit { id: self.hit_seq, hit };
        for _ in 0..HIT_COPIES {
            self.sock_send(&msg, host);
        }
    }

    /// The host's active quest now; [`Session::tick`] sends it.
    pub fn set_quest(&mut self, quest: QuestView) {
        self.quest = Some(quest);
    }

    /// The host's game clock now; [`Session::tick`] sends it every [`CLOCK_EVERY`].
    pub fn set_clock(&mut self, clock: ClockView) {
        self.clock = Some(clock);
    }

    /// This player unlocked `reference` (a terminal: hacked): to the host, or from the host
    /// to every guest ([`HIT_COPIES`] times).
    pub fn send_unlock(&mut self, reference: u32, terminal: bool) {
        let msg = Msg::Unlock { reference, terminal };
        let to: Vec<SocketAddr> = match &self.role {
            Role::Host => self.peers.iter().filter_map(|p| p.addr).collect(),
            Role::Guest { host: Some(host), .. } => vec![*host],
            Role::Guest { .. } => Vec::new(),
        };
        for addr in to {
            for _ in 0..HIT_COPIES {
                self.sock_send(&msg, addr);
            }
        }
    }

    /// This player revives player `target` (down): to the host, which passes it on, or
    /// from the host to that guest ([`HIT_COPIES`] times).
    pub fn send_revive(&mut self, target: u8) {
        let Some(by) = self.id() else { return };
        let msg = Msg::Revive { target, by };
        let to = match &self.role {
            Role::Host => self.peers.iter().find(|p| p.id == target).and_then(|p| p.addr),
            Role::Guest { host, .. } => *host,
        };
        if let Some(addr) = to {
            for _ in 0..HIT_COPIES {
                self.sock_send(&msg, addr);
            }
        }
    }

    /// An NPC's hit on guest `id`, for that guest to take (as [`Session::send_hit`]; host
    /// only).
    pub fn send_hit_to(&mut self, id: u8, hit: Vec<u8>) {
        if !self.is_host() || hit.len() > HIT_MAX {
            return;
        }
        let Some(addr) = self.peers.iter().find(|p| p.id == id).and_then(|p| p.addr) else { return };
        self.hit_seq = self.hit_seq.wrapping_add(1);
        let msg = Msg::Hit { id: self.hit_seq, hit };
        for _ in 0..HIT_COPIES {
            self.sock_send(&msg, addr);
        }
    }

    /// A host's lobby key ([`crate::code::Lobby::key`]): from now on only Hellos with it
    /// (or with none) are answered.
    pub fn set_lobby(&mut self, key: u32) {
        self.lobby = Some(key);
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
        self.sock.local_addr()
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
            let silent = now - *last_heard;
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
            if (id.is_none() || *host_lost || *asked) && last_hello.is_none_or(|t| now - t >= HELLO_EVERY) {
                *asked = false;
                let hello = encode(&Msg::Hello { token: *token, lobby: *lobby, name: name.clone() });
                // Until one answers, every address the code gave; then that one.
                for to in host.iter().chain(candidates.iter().filter(|_| host.is_none())) {
                    send_raw(&self.sock, self.steam, &hello, *to);
                }
                *last_hello = Some(now);
            }
        }
        let due = self.last_send.is_none_or(|t| now - t >= Duration::from_secs(1) / SEND_HZ);
        if let (true, Some(id)) = (due, self.id()) {
            let msg = match self.state {
                Some(state) => {
                    self.seq = self.seq.wrapping_add(1);
                    Msg::State { id, seq: self.seq, state }
                }
                None => Msg::Ping,
            };
            match &self.role {
                Role::Host => {
                    for addr in self.peers.iter().filter_map(|p| p.addr) {
                        self.sock_send(&msg, addr);
                    }
                }
                Role::Guest { host: Some(host), .. } => self.sock_send(&msg, *host),
                Role::Guest { .. } => {}
            }
            self.last_send = Some(now);
        }
        let npcs_due = self.last_npcs.is_none_or(|t| now - t >= Duration::from_secs(1) / NPC_HZ);
        if npcs_due && self.is_host() && !self.npcs.is_empty() {
            self.npc_seq = self.npc_seq.wrapping_add(1);
            for chunk in self.npcs.chunks(NPCS_PER_PACKET) {
                let msg = Msg::Npcs { seq: self.npc_seq, npcs: chunk.to_vec() };
                for addr in self.peers.iter().filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
            }
            self.last_npcs = Some(now);
        }
        if let (true, Some(q)) = (self.is_host(), self.quest.clone()) {
            if self.last_quest.as_ref().is_none_or(|(t, sent)| *sent != q || now - *t >= QUEST_EVERY) {
                let msg = Msg::Quest(q.clone());
                for addr in self.peers.iter().filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
                self.last_quest = Some((now, q));
            }
        }
        if let (true, Some(c)) = (self.is_host(), self.clock) {
            if self.last_clock.is_none_or(|t| now - t >= CLOCK_EVERY) {
                let msg = Msg::Clock(c);
                for addr in self.peers.iter().filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
                self.last_clock = Some(now);
            }
        }
        let look_due = self.last_look.is_none_or(|t| now - t >= LOOK_EVERY);
        if let (true, Some(look), Some(id)) = (look_due, self.look.clone(), self.id()) {
            let msg = Msg::Look { id, look };
            match &self.role {
                Role::Host => {
                    for addr in self.peers.iter().filter_map(|p| p.addr) {
                        self.sock_send(&msg, addr);
                    }
                }
                Role::Guest { host: Some(host), .. } => self.sock_send(&msg, *host),
                Role::Guest { .. } => {}
            }
            self.last_look = Some(now);
        }
    }

    fn sock_send(&self, msg: &Msg, to: SocketAddr) {
        send_raw(&self.sock, self.steam, &encode(msg), to);
    }

    fn receive(&mut self, now: Instant) {
        let mut buf = [0u8; 1500];
        loop {
            let (n, from) = match self.sock.recv_from(&mut buf) {
                Ok(r) => r,
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                // Windows reports an ICMP "port unreachable" from an earlier send as an error
                // on a later receive (WSAECONNRESET); it says nothing about any packet.
                Err(e) if e.kind() == ErrorKind::ConnectionReset => continue,
                Err(_) => break,
            };
            self.handle(&buf[..n], from, now);
        }
        let mut steam = Vec::new();
        if let Some(s) = self.steam {
            s.receive(&mut steam);
        }
        for (bytes, from) in steam {
            self.handle(&bytes, from, now);
        }
    }

    fn handle(&mut self, bytes: &[u8], from: SocketAddr, now: Instant) {
        if let Some(msg) = decode(bytes) {
            if self.is_host() {
                self.host_handle(msg, from, now);
            } else {
                self.guest_handle(msg, from, now);
            }
        }
    }

    fn host_handle(&mut self, msg: Msg, from: SocketAddr, now: Instant) {
        match msg {
            Msg::Hello { token, lobby, name } => {
                // Another lobby's guest asking around its network.
                if lobby != 0 && self.lobby.is_some_and(|k| k != lobby) {
                    return;
                }
                // A known token is a guest coming back, from wherever it now is.
                if let Some(p) = self.peers.iter_mut().find(|p| p.token == token) {
                    p.addr = Some(from);
                    p.last_heard = now;
                    // A game started again counts its states from the start.
                    p.last_seq = None;
                    let id = p.id;
                    if std::mem::take(&mut p.lost) {
                        self.events.push(Event::Back(id));
                    }
                    self.sock_send(&Msg::Welcome { id }, from);
                    return;
                }
                let Some(id) = (1..=MAX_GUESTS as u8).find(|i| self.peers.iter().all(|p| p.id != *i)) else {
                    self.sock_send(&Msg::Full, from);
                    return;
                };
                self.peers.push(Peer { addr: Some(from), token, ..Peer::new(id, now) });
                self.sock_send(&Msg::Welcome { id }, from);
                self.events.push(Event::Joined { id, name });
            }
            Msg::State { seq, state, .. } => {
                // The sender is the guest its address belongs to, whatever id it claims.
                let Some(p) = self.peers.iter_mut().find(|p| p.addr == Some(from)) else {
                    self.sock_send(&Msg::Who, from);
                    return;
                };
                p.last_heard = now;
                let id = p.id;
                if std::mem::take(&mut p.lost) {
                    self.events.push(Event::Back(id));
                }
                if p.last_seq.is_some_and(|s| !fresh(seq, s)) {
                    return;
                }
                p.last_seq = Some(seq);
                let relay = Msg::State { id, seq, state };
                for addr in self.peers.iter().filter(|o| o.id != id).filter_map(|o| o.addr) {
                    self.sock_send(&relay, addr);
                }
                self.events.push(Event::State { id, state });
            }
            Msg::Ping => {
                let Some(p) = self.peers.iter_mut().find(|p| p.addr == Some(from)) else {
                    self.sock_send(&Msg::Who, from);
                    return;
                };
                p.last_heard = now;
                let id = p.id;
                if std::mem::take(&mut p.lost) {
                    self.events.push(Event::Back(id));
                }
            }
            Msg::Look { look, .. } => {
                let Some(p) = self.peers.iter_mut().find(|p| p.addr == Some(from)) else { return };
                let id = p.id;
                if p.take_look(look.clone()) {
                    self.events.push(Event::Look { id, look: look.clone() });
                }
                // Each time (it comes again every LOOK_EVERY): a guest who joined since has it.
                let relay = Msg::Look { id, look };
                for addr in self.peers.iter().filter(|o| o.id != id).filter_map(|o| o.addr) {
                    self.sock_send(&relay, addr);
                }
            }
            Msg::Bye { .. } => {
                if let Some(i) = self.peers.iter().position(|p| p.addr == Some(from)) {
                    let id = self.peers.remove(i).id;
                    self.relay_bye(id);
                    self.events.push(Event::Left(id));
                }
            }
            Msg::Hit { id: hit_id, hit } => {
                let Some(p) = self.peers.iter_mut().find(|p| p.addr == Some(from)) else { return };
                if p.hits.contains(&hit_id) {
                    return;
                }
                if p.hits.len() == HITS_KEPT {
                    p.hits.pop_front();
                }
                p.hits.push_back(hit_id);
                self.events.push(Event::Hit { id: p.id, hit });
            }
            // Applied here and passed on to the other guests.
            Msg::Unlock { reference, terminal } => {
                let Some(id) = self.peers.iter().find(|p| p.addr == Some(from)).map(|p| p.id) else { return };
                self.events.push(Event::Unlocked { id, reference, terminal });
                let msg = Msg::Unlock { reference, terminal };
                for addr in self.peers.iter().filter(|p| p.id != id).filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
            }
            Msg::Revive { target, by } => {
                // From a known guest only; for the host, or passed on to its guest.
                if !self.peers.iter().any(|p| p.addr == Some(from) && p.id == by) {
                    return;
                }
                if target == 0 {
                    self.events.push(Event::Revived { by });
                } else if let Some(addr) = self.peers.iter().find(|p| p.id == target).and_then(|p| p.addr) {
                    self.sock_send(&Msg::Revive { target, by }, addr);
                }
            }
            Msg::Welcome { .. } | Msg::Full | Msg::Who | Msg::Npcs { .. } | Msg::Quest(_) | Msg::Clock(_) => {}
        }
    }

    fn guest_handle(&mut self, msg: Msg, from: SocketAddr, now: Instant) {
        let Role::Guest { host, id, last_heard, last_hello, host_lost, asked, gone, .. } = &mut self.role else { return };
        // Before an answer, whoever answers the Hello is the host (a broadcast's answer
        // comes from the host's own address; a host answers only its own lobby's key);
        // then only it.
        let known = match host {
            Some(h) => *h == from,
            None => true,
        };
        if !known || *gone {
            return;
        }
        if host.is_none() {
            *host = Some(from);
        }
        *last_heard = now;
        if std::mem::take(host_lost) {
            self.events.push(Event::HostBack);
        }
        match msg {
            Msg::Welcome { id: given } => {
                if *id != Some(given) {
                    // First welcome, or the host restarted and gave a new id: everyone it
                    // knew is announced again by their states.
                    *id = Some(given);
                    for p in self.peers.drain(..) {
                        self.events.push(Event::Left(p.id));
                    }
                    self.events.push(Event::Welcomed(given));
                }
            }
            Msg::Full => self.events.push(Event::Refused),
            Msg::Who => {
                *asked = true;
                *last_hello = None;
            }
            Msg::State { id: peer, seq, state } => {
                if Some(peer) == *id {
                    return;
                }
                let p = match self.peers.iter().position(|p| p.id == peer) {
                    Some(i) => &mut self.peers[i],
                    None => {
                        self.peers.push(Peer::new(peer, now));
                        self.events.push(Event::Joined { id: peer, name: String::new() });
                        self.peers.last_mut().unwrap()
                    }
                };
                p.last_heard = now;
                if std::mem::take(&mut p.lost) {
                    self.events.push(Event::Back(peer));
                }
                if p.last_seq.is_some_and(|s| !fresh(seq, s)) {
                    return;
                }
                p.last_seq = Some(seq);
                self.events.push(Event::State { id: peer, state });
            }
            Msg::Look { id: peer, look } => {
                if Some(peer) == *id {
                    return;
                }
                let p = match self.peers.iter().position(|p| p.id == peer) {
                    Some(i) => &mut self.peers[i],
                    None => {
                        self.peers.push(Peer::new(peer, now));
                        self.events.push(Event::Joined { id: peer, name: String::new() });
                        self.peers.last_mut().unwrap()
                    }
                };
                if p.take_look(look.clone()) {
                    self.events.push(Event::Look { id: peer, look });
                }
            }
            Msg::Bye { id: 0 } => {
                *gone = true;
                for p in self.peers.drain(..) {
                    self.events.push(Event::Left(p.id));
                }
                self.events.push(Event::HostGone);
            }
            Msg::Bye { id: peer } => {
                if let Some(i) = self.peers.iter().position(|p| p.id == peer) {
                    self.peers.remove(i);
                    self.events.push(Event::Left(peer));
                }
            }
            Msg::Npcs { seq, npcs } => {
                // A round's packets share its number; an older round is dropped.
                if self.heard_npcs.is_some_and(|l| seq != l && !fresh(seq, l)) {
                    return;
                }
                self.heard_npcs = Some(seq);
                self.events.push(Event::Npcs(npcs));
            }
            // A hit on this guest, from the host (player 0).
            Msg::Hit { id: hit_id, hit } => {
                if self.heard_hits.contains(&hit_id) {
                    return;
                }
                if self.heard_hits.len() == HITS_KEPT {
                    self.heard_hits.pop_front();
                }
                self.heard_hits.push_back(hit_id);
                self.events.push(Event::Hit { id: 0, hit });
            }
            Msg::Quest(q) => {
                if self.heard_quest.as_ref() != Some(&q) {
                    self.heard_quest = Some(q.clone());
                    self.events.push(Event::Quest(q));
                }
            }
            Msg::Unlock { reference, terminal } => self.events.push(Event::Unlocked { id: 0, reference, terminal }),
            Msg::Clock(c) => self.events.push(Event::Clock(c)),
            Msg::Revive { target, by } => {
                if Some(target) == *id {
                    self.events.push(Event::Revived { by });
                }
            }
            Msg::Hello { .. } | Msg::Ping => {}
        }
    }

    fn relay_bye(&self, id: u8) {
        if self.is_host() {
            for addr in self.peers.iter().filter_map(|p| p.addr) {
                self.sock_send(&Msg::Bye { id }, addr);
            }
        }
    }

    /// Quiet peers become lost, and lost ones past [`GRACE`] are dropped.
    fn check_peers(&mut self, now: Instant) {
        let mut gone = Vec::new();
        for p in &mut self.peers {
            let silent = now - p.last_heard;
            if silent >= GRACE {
                gone.push(p.id);
            } else if !p.lost && silent >= LOST_AFTER {
                p.lost = true;
                self.events.push(Event::Lost(p.id));
            }
        }
        for id in gone {
            self.peers.retain(|p| p.id != id);
            self.relay_bye(id);
            self.events.push(Event::Left(id));
        }
    }

    /// Say goodbye, so the others drop this player at once rather than after [`GRACE`].
    fn bye(&self) {
        match &self.role {
            Role::Host => self.relay_bye(0),
            Role::Guest { host: Some(host), id: Some(id), gone: false, .. } => self.sock_send(&Msg::Bye { id: *id }, *host),
            Role::Guest { .. } => {}
        }
    }
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
    pub fn new(session: Session) -> Self {
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
                .expect("spawn the co-op network thread")
        };
        Self { session, stop, thread: Some(thread), quiet: false }
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

    pub fn set_state(&self, state: PlayerState) {
        self.with(|s| s.set_state(state));
    }

    pub fn set_look(&self, look: Vec<u8>) {
        self.with(|s| s.set_look(look));
    }

    pub fn set_lobby(&self, key: u32) {
        self.with(|s| s.set_lobby(key));
    }

    pub fn set_npcs(&self, npcs: Vec<NpcState>) {
        self.with(|s| s.set_npcs(npcs));
    }

    pub fn send_hit(&self, hit: Vec<u8>) {
        self.with(|s| s.send_hit(hit));
    }

    pub fn send_hit_to(&self, id: u8, hit: Vec<u8>) {
        self.with(|s| s.send_hit_to(id, hit));
    }

    pub fn send_revive(&self, target: u8) {
        self.with(|s| s.send_revive(target));
    }

    pub fn set_clock(&self, clock: ClockView) {
        self.with(|s| s.set_clock(clock));
    }

    pub fn set_quest(&self, quest: QuestView) {
        self.with(|s| s.set_quest(quest));
    }

    pub fn send_unlock(&self, reference: u32, terminal: bool) {
        self.with(|s| s.send_unlock(reference, terminal));
    }

    pub fn events(&self) -> Vec<Event> {
        self.with(|s| s.take_events())
    }

    /// The host's address once one of the candidates answered (guests only).
    pub fn host_addr(&self) -> Option<SocketAddr> {
        self.with(|s| match &s.role {
            Role::Guest { host, .. } => *host,
            Role::Host => None,
        })
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
/// Send `bytes` to `to`: over Steam for a stand-in address (dropped without Steam), else
/// over UDP.
fn send_raw(sock: &UdpSocket, steam: Option<&Steam>, bytes: &[u8], to: SocketAddr) {
    match crate::steam::peer(to) {
        Some(id) => {
            if let Some(s) = steam {
                s.send(id, bytes);
            }
        }
        None => {
            let _ = sock.send_to(bytes, to);
        }
    }
}

fn newer(a: u32, b: u32) -> bool {
    a != b && a.wrapping_sub(b) < u32::MAX / 2
}

/// A state to take after `last`: newer, or so much older that its game started again
/// (a load restarts it, and its count with it) rather than a packet late on the way.
fn fresh(seq: u32, last: u32) -> bool {
    newer(seq, last) || last.wrapping_sub(seq) > RESTARTED
}

/// A session token no other guest will have: std's per-process random hash keys, mixed
/// with the time.
pub fn new_token() -> u64 {
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos());
    h.write_u32(std::process::id());
    h.finish()
}

fn truncate(name: &str) -> String {
    let mut end = name.len().min(NAME_MAX);
    while !name.is_char_boundary(end) {
        end -= 1;
    }
    name[..end].to_string()
}

fn encode(msg: &Msg) -> Vec<u8> {
    let mut b = Vec::with_capacity(64);
    b.extend_from_slice(&MAGIC);
    b.push(VERSION);
    match msg {
        Msg::Hello { token, lobby, name } => {
            b.push(1);
            b.extend_from_slice(&token.to_le_bytes());
            b.extend_from_slice(&lobby.to_le_bytes());
            b.push(name.len() as u8);
            b.extend_from_slice(name.as_bytes());
        }
        Msg::Welcome { id } => b.extend_from_slice(&[2, *id]),
        Msg::Full => b.push(3),
        Msg::Who => b.push(6),
        Msg::Ping => b.push(7),
        Msg::Look { id, look } => {
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
        }
        Msg::Bye { id } => b.extend_from_slice(&[5, *id]),
        Msg::Npcs { seq, npcs } => {
            b.push(9);
            b.extend_from_slice(&seq.to_le_bytes());
            b.push(npcs.len() as u8);
            for n in npcs {
                b.extend_from_slice(&n.reference.to_le_bytes());
                for v in n.feet.iter().chain([&n.heading, &n.pitch]) {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                b.push(n.flags);
                b.extend_from_slice(&n.weapon.to_le_bytes());
                b.push(n.attacks);
            }
        }
        Msg::Hit { id, hit } => {
            b.push(10);
            b.extend_from_slice(&id.to_le_bytes());
            b.push(hit.len() as u8);
            b.extend_from_slice(hit);
        }
        Msg::Quest(q) => {
            b.push(11);
            b.extend_from_slice(&q.quest.to_le_bytes());
            b.push(u8::from(q.completed) | u8::from(q.failed) << 1);
            let list = &q.objectives[..q.objectives.len().min(QUEST_OBJECTIVES)];
            b.push(list.len() as u8);
            for (i, done) in list {
                b.extend_from_slice(&i.to_le_bytes());
                b.push(u8::from(*done));
            }
        }
        Msg::Unlock { reference, terminal } => {
            b.push(12);
            b.extend_from_slice(&reference.to_le_bytes());
            b.push(u8::from(*terminal));
        }
        Msg::Clock(c) => {
            b.push(13);
            for v in [c.year, c.month, c.day, c.hour, c.days_passed] {
                b.extend_from_slice(&v.to_le_bytes());
            }
        }
        Msg::Revive { target, by } => {
            b.push(14);
            b.push(*target);
            b.push(*by);
        }
    }
    b
}

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
            let token = r.u64()?;
            let lobby = r.u32()?;
            let len = r.u8()? as usize;
            if len > NAME_MAX {
                return None;
            }
            Msg::Hello { token, lobby, name: String::from_utf8_lossy(r.take(len)?).into_owned() }
        }
        2 => Msg::Welcome { id: r.u8()? },
        3 => Msg::Full,
        4 => {
            let id = r.u8()?;
            let seq = r.u32()?;
            let place = match (r.u8()?, r.u32()?) {
                (0, f) => Place::Interior(f),
                (1, f) => Place::Exterior(f),
                _ => return None,
            };
            let feet = [r.f32()?, r.f32()?, r.f32()?];
            let (yaw, pitch) = (r.f32()?, r.f32()?);
            if !feet.iter().chain([&yaw, &pitch]).all(|v| v.is_finite()) {
                return None;
            }
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
            Msg::State { id, seq, state }
        }
        5 => Msg::Bye { id: r.u8()? },
        6 => Msg::Who,
        7 => Msg::Ping,
        8 => {
            let id = r.u8()?;
            let len = u16::from_le_bytes(r.take(2)?.try_into().ok()?) as usize;
            if len > LOOK_MAX {
                return None;
            }
            Msg::Look { id, look: r.take(len)?.to_vec() }
        }
        9 => {
            let seq = r.u32()?;
            let n = r.u8()? as usize;
            if n > NPCS_PER_PACKET {
                return None;
            }
            let mut npcs = Vec::with_capacity(n);
            for _ in 0..n {
                let reference = r.u32()?;
                let feet = [r.f32()?, r.f32()?, r.f32()?];
                let (heading, pitch) = (r.f32()?, r.f32()?);
                if !feet.iter().chain([&heading, &pitch]).all(|v| v.is_finite()) {
                    return None;
                }
                npcs.push(NpcState { reference, feet, heading, pitch, flags: r.u8()?, weapon: r.u32()?, attacks: r.u8()? });
            }
            Msg::Npcs { seq, npcs }
        }
        10 => {
            let id = r.u16()?;
            let len = r.u8()? as usize;
            if len > HIT_MAX {
                return None;
            }
            Msg::Hit { id, hit: r.take(len)?.to_vec() }
        }
        11 => {
            let quest = r.u32()?;
            let flags = r.u8()?;
            let n = r.u8()? as usize;
            if n > QUEST_OBJECTIVES {
                return None;
            }
            let mut objectives = Vec::with_capacity(n);
            for _ in 0..n {
                objectives.push((r.u32()? as i32, r.u8()? != 0));
            }
            Msg::Quest(QuestView { quest, completed: flags & 1 != 0, failed: flags & 2 != 0, objectives })
        }
        12 => Msg::Unlock { reference: r.u32()?, terminal: r.u8()? != 0 },
        14 => Msg::Revive { target: r.u8()?, by: r.u8()? },
        13 => Msg::Clock(ClockView { year: r.f32()?, month: r.f32()?, day: r.f32()?, hour: r.f32()?, days_passed: r.f32()? }),
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
    fn f32(&mut self) -> Option<f32> {
        self.take(4).map(|b| f32::from_le_bytes(b.try_into().unwrap()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(x: f32) -> PlayerState {
        PlayerState::at(Place::Exterior(0xDA726), [x, 2.0, 3.0], 0.5)
    }

    #[test]
    fn messages_round_trip() {
        for msg in [
            Msg::Hello { token: 0xDEAD_BEEF_0123, lobby: 0x1234_5678, name: "Courier".into() },
            Msg::Welcome { id: 2 },
            Msg::Full,
            Msg::Who,
            Msg::Ping,
            Msg::Look { id: 2, look: vec![1, 2, 3, 250] },
            Msg::Hit { id: 65_000, hit: vec![9, 8, 7] },
            Msg::Quest(QuestView { quest: 0x0010_4C1D, completed: false, failed: true, objectives: vec![(10, true), (-1, false), (200, false)] }),
            Msg::Quest(QuestView::default()),
            Msg::Unlock { reference: 0x0001_2345, terminal: true },
            Msg::Clock(ClockView { year: 2281.0, month: 9.0, day: 19.0, hour: 13.25, days_passed: 6.05 }),
            Msg::Revive { target: 2, by: 0 },
            Msg::Npcs {
                seq: 12,
                npcs: vec![
                    NpcState { reference: 0x0010_4C3A, feet: [1.0, -2.5, 300.0], heading: 1.5, pitch: -0.2, flags: npc_flag::DRAWN | npc_flag::AIMING, weapon: 0x0E3778, attacks: 255 },
                    NpcState { reference: 7, feet: [0.0; 3], heading: 0.0, pitch: 0.0, flags: npc_flag::DEAD, weapon: 0, attacks: 0 },
                ],
            },
            Msg::State { id: 1, seq: 7, state: state(-12.5) },
            Msg::State { id: 0, seq: u32::MAX, state: PlayerState { place: Place::Interior(0x104C1C), ..state(0.0) } },
            Msg::State {
                id: 2,
                seq: 9,
                state: PlayerState {
                    pitch: -0.3,
                    flags: flag::DRAWN | flag::SIGHTS,
                    weapon: 0x0E3778,
                    attacks: 65535,
                    attack_kind: attack::POWER_LEFT,
                    reloads: 3,
                    worn: [1, 2, 3, 0, 0, 0, 0, 0, 0, 0],
                    worn_len: 3,
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
        let nan = encode(&Msg::State { id: 1, seq: 1, state: state(f32::NAN) });
        assert!(decode(&nan).is_none(), "not finite");
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

    fn host_and(n: usize) -> (Session, Vec<Session>, SocketAddr) {
        let host = Session::host(0).unwrap();
        let addr: SocketAddr = format!("127.0.0.1:{}", host.local_addr().unwrap().port()).parse().unwrap();
        let guests = (0..n).map(|i| Session::join(&[addr], 0, &format!("G{i}"), new_token()).unwrap()).collect();
        (host, guests, addr)
    }

    /// A guest's revive reaches the other guest through the host, and the host's own.
    #[test]
    fn revives_reach_the_one_revived() {
        let (mut host, mut guests, _) = host_and(2);
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let (ida, idb) = (a.id().unwrap(), b.id().unwrap());
        a.send_revive(idb);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[2].iter().any(|e| matches!(e, Event::Revived { .. })));
        assert!(seen[2].contains(&Event::Revived { by: ida }));
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Revived { .. })));
        b.send_revive(0);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[0].iter().any(|e| matches!(e, Event::Revived { .. })));
        assert!(seen[0].contains(&Event::Revived { by: idb }));
        host.send_revive(ida);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Revived { .. })));
        assert!(seen[1].contains(&Event::Revived { by: 0 }));
    }

    /// Each guest is welcomed, the host relays one guest's state to the other, and a guest
    /// that leaves is dropped at once.
    #[test]
    fn host_relays_between_guests() {
        let (mut host, mut g, _) = host_and(2);
        let mut b = g.pop().unwrap();
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| {
            seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_)))
        });
        let (ida, idb) = (a.id().unwrap(), b.id().unwrap());
        assert_ne!(ida, idb);

        a.set_state(state(10.0));
        host.set_state(state(99.0));
        let seen = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| {
            seen[0].contains(&Event::State { id: ida, state: state(10.0) })
                && seen[2].contains(&Event::State { id: ida, state: state(10.0) })
                && seen[1].contains(&Event::State { id: 0, state: state(99.0) })
        });
        assert!(seen[2].contains(&Event::Joined { id: ida, name: String::new() }));

        a.bye();
        drop(a);
        run(&mut [&mut host, &mut b], &mut clock, step, |seen| seen[0].contains(&Event::Left(ida)) && seen[1].contains(&Event::Left(ida)));
    }

    /// A guest asking its network by broadcast is answered only by its own lobby's host.
    #[test]
    fn a_host_answers_only_its_own_lobby() {
        let mut other = Session::host(0).unwrap();
        other.set_lobby(7);
        let mut mine = Session::host(0).unwrap();
        mine.set_lobby(9);
        let at = |s: &Session| -> SocketAddr { format!("127.0.0.1:{}", s.local_addr().unwrap().port()).parse().unwrap() };
        let mut a = Session::join(&[at(&other), at(&mine)], 9, "A", new_token()).unwrap();
        let mut clock = Instant::now();
        run(&mut [&mut other, &mut mine, &mut a], &mut clock, Duration::from_millis(10), |seen| seen[2].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let Role::Guest { host: chosen, .. } = &a.role else { unreachable!() };
        assert_eq!(*chosen, Some(at(&mine)));
        assert!(other.peers.is_empty());
    }

    /// A code's first address may lead nowhere (the host's local address, seen from
    /// outside its network): the guest joins through the one that answers.
    #[test]
    fn a_guest_joins_through_the_address_that_answers() {
        let mut host = Session::host(0).unwrap();
        let port = host.local_addr().unwrap().port();
        let dead: SocketAddr = "127.0.0.1:9".parse().unwrap();
        let live: SocketAddr = format!("127.0.0.1:{port}").parse().unwrap();
        let mut a = Session::join(&[dead, live], 0, "A", new_token()).unwrap();
        let mut clock = Instant::now();
        run(&mut [&mut host, &mut a], &mut clock, Duration::from_millis(10), |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let Role::Guest { host: chosen, .. } = &a.role else { unreachable!() };
        assert_eq!(*chosen, Some(live));
    }

    /// A guest with no state yet (at the main menu, loading) whose Hello reached the host
    /// by both of the code's addresses, the host keeping one and the guest the other, is
    /// set right by the host asking who its Pings are from: it is never lost.
    #[test]
    fn a_waiting_guest_on_two_addresses_is_not_lost() {
        let Some(lan) = crate::reach::local_ip() else { return };
        let mut host = Session::host(0).unwrap();
        let port = host.local_addr().unwrap().port();
        let both: [SocketAddr; 2] = [format!("{lan}:{port}").parse().unwrap(), format!("127.0.0.1:{port}").parse().unwrap()];
        let mut a = Session::join(&both, 0, "A", new_token()).unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(50);
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        // Past LOST_AFTER twice over, Pings only.
        let mut seen = Vec::new();
        for _ in 0..((LOST_AFTER * 2).as_millis() / step.as_millis()) {
            clock += step;
            host.tick(clock);
            a.tick(clock);
            seen.extend(host.take_events());
            seen.extend(a.take_events());
            std::thread::sleep(Duration::from_millis(2));
        }
        assert!(!seen.iter().any(|e| matches!(e, Event::Lost(_) | Event::HostLost)), "{seen:?}");
    }

    /// Each player's look reaches the others, the host's too, once each until it changes.
    #[test]
    fn npcs_reach_guests_and_hits_reach_the_host_once() {
        let (mut host, mut g, _) = host_and(1);
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        // More than one packet's worth.
        let npcs: Vec<NpcState> = (0..NPCS_PER_PACKET as u32 + 5).map(|i| NpcState { reference: 0x100 + i, feet: [i as f32, 0.0, 0.0], heading: 0.0, pitch: 0.0, flags: 0, weapon: 0, attacks: 0 }).collect();
        host.set_npcs(npcs.clone());
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().filter(|e| matches!(e, Event::Npcs(_))).count() >= 2);
        let mut heard: Vec<NpcState> = Vec::new();
        for e in &seen[1] {
            if let Event::Npcs(n) = e {
                heard.extend(n);
            }
        }
        for n in &npcs {
            assert!(heard.contains(n), "{n:?} not heard");
        }
        a.send_hit(vec![1, 2, 3]);
        a.send_hit(vec![4]);
        let id = a.id().unwrap();
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[0].iter().filter(|e| matches!(e, Event::Hit { .. })).count() >= 2);
        let hits: Vec<&Event> = seen[0].iter().filter(|e| matches!(e, Event::Hit { .. })).collect();
        assert_eq!(hits, [&Event::Hit { id, hit: vec![1, 2, 3] }, &Event::Hit { id, hit: vec![4] }]);
        // And the NPCs' hits on the guest, the other way.
        host.send_hit_to(id, vec![5, 6]);
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Hit { .. })));
        let hits: Vec<&Event> = seen[1].iter().filter(|e| matches!(e, Event::Hit { .. })).collect();
        assert_eq!(hits, [&Event::Hit { id: 0, hit: vec![5, 6] }]);
        // The host's quest, once until it changes; an unlock, to the host.
        let q = QuestView { quest: 0x0010_4C1D, completed: false, failed: false, objectives: vec![(10, false)] };
        host.set_quest(q.clone());
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Quest(_))));
        assert_eq!(seen[1].iter().filter(|e| matches!(e, Event::Quest(_))).collect::<Vec<_>>(), [&Event::Quest(q)]);
        a.send_unlock(0x0001_2345, false);
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[0].iter().any(|e| matches!(e, Event::Unlocked { .. })));
        assert!(seen[0].contains(&Event::Unlocked { id, reference: 0x0001_2345, terminal: false }));
    }

    #[test]
    fn looks_reach_everyone_once() {
        let (mut host, mut g, _) = host_and(2);
        let mut b = g.pop().unwrap();
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| seen[1..].iter().all(|s| s.iter().any(|e| matches!(e, Event::Welcomed(_)))));
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
        let mut back = Session::join(&[addr], 0, "G0", token).unwrap();
        drop(a);
        back.set_state(state(2.0));
        let seen = run(&mut [&mut host, &mut back], &mut clock, step, |seen| {
            seen[0].contains(&Event::Back(id)) && seen[0].contains(&Event::State { id, state: state(2.0) })
        });
        assert_eq!(back.id(), Some(id), "same slot");
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Joined { .. })), "not a new guest: {:?}", seen[0]);
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
