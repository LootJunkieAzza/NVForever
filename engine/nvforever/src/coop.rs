//! Co-op prototype (private fork, not upstream). A lobby is opened from the pause menu
//! (Lobby, Create Lobby) and joined from the main menu (Join Lobby, the code, a save);
//! the screens are `nvf_play::lobby`. `--host <port>` and `--join <code or address>`
//! do the same from the command line.
//!
//! **Lobby codes.** There is no server: the host's game is the lobby. Its code is the
//! host's own address, 8 characters (`nvf_coop::code`); friends on the host's network
//! find it by broadcast. Creating it asks the host's router to let friends in
//! (`nvf_coop::reach`). A load restarts the game (main.rs
//! `restart_args`): the lobby goes with it, in [`HOST_ENV`] and [`CODE_ENV`] for a host
//! and [`JOIN_ENV`] for a guest, which is how a guest's chosen save comes into the host's
//! game.
//!
//! **The host's world.** The host runs the world: its NPCs are the only ones that think
//! (packages, fights, deaths). It sends each NPC it draws (where, facing, weapon out,
//! sneaking, aim, attacks, dead; [`share_npcs`]), and a guest's game turns its own NPCs'
//! AI off and draws them there ([`follow_npcs`], `Residents::remote`). A guest's hits on
//! NPCs are not applied in its game: they go to the host, which applies them as its own
//! player's (`nvf_play::combat::RemoteHit`), so the NPC reacts once, in the one world,
//! and every game sees it.
//!
//! **The other players in the host's world.** On the host each guest is also an actor of
//! the world ([`stand_ins`]): a reference created from the player's record, its AI off,
//! that the network puts where the guest is (disabled while the guest is elsewhere, lost
//! or dead; deleted when it leaves, and never saved, `fnv_game::Game::coop_actors`). The
//! world's actors take it for the player (`Game::as_player`): they detect it, hear its
//! shots ([`PuppetShot`]), attack it whenever they would attack the player, and fight it
//! back when its hits (applied as that actor's) provoke them. Their hits on it do nothing
//! on the host: they go to its guest (`CombatStats::coop_hits`), whose game takes them on
//! its own player, with its own armour.
//!
//! **What a guest does.** A guest leaves the story to the host, as in Dying Light's co-op
//! (`fnv_game::Game::coop_guest`): it talks to NPCs only to trade with them or to play
//! Caravan (their own lines that do that; nobody else offers to talk, and no script
//! starts a conversation with it), and plays at the casino tables. Its quest list,
//! objectives, markers and quest messages are the host's active quest ([`QuestView`],
//! `Game::coop_host_quest`). It picks locks and hacks terminals, and so does the host:
//! what either unlocks is unlocked for everyone (`nvf_play::lockpick::LockOpened`).
//!
//! Every game sends its player's place, feet and heading over `nvf_coop::net` (the host
//! passes them between guests), and draws the other players as NPCs the network moves:
//! engine-only puppets with a reference no load order has, so no script, AI package or
//! physics body ever sees or fights them (the same drawing as the `--npc-travel`
//! travellers, `nav::show_actor`).
//!
//! **The leash.** A guest goes where the host is: when the host is in another cell or
//! worldspace, or outdoors farther than [`LEASH`] away, the guest travels to just behind
//! the host, through the loading screen like any door.
//!
//! **What they do.** Each state carries the weapon in hand, whether it is out, aimed or
//! down the sights, sneaking, in the air, what is worn, and counters of the attacks and
//! reloads begun: the puppet draws and holsters, crouches, aims (as a fighter does,
//! `npc_combat::Fighters::set_puppet`) and plays one attack or reload for each the
//! counter went up, so a lost packet loses none.
//!
//! **How they look.** Each game sends its player's look from the character creator
//! (race, sex, hair, eyes, face; [`encode_look`]) every `net::LOOK_EVERY`; the others draw
//! that player from the player's own record with it (`ActorWorld::looks`), as this game
//! draws its own body. Until a look comes, a player is drawn as [`Config::base`].
//!
//! **Dropouts.** A player whose link goes quiet stays where it was last seen, frozen,
//! while the link reconnects (net.rs); only a player who quit or stayed away past
//! `net::GRACE` is removed. A guest's token is kept in [`TOKEN_ENV`] and in its home, so
//! the game a load restarts (main.rs `restart_args`), or one started again after a
//! crash, comes back as the same player.

use crate::actors::ActorWorld;
use crate::attack_anims::{Move, PlayAttack, Power, SwingPick};
use crate::locomotion::Locomotion;
use crate::nav::{show_actor, Who};
use crate::npc_combat::{Fighters, PuppetPose};
use crate::outfits::{Equipment, PuppetEquipment};
use crate::weapons::WeaponHold;
use crate::world::WorldRoot;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use fnv_actor::weapon::AnimType;
use fnv_game::chargen::Look;
use fnv_sim::FormId;
use nvf_assets::actor::ActorParts;
use nvf_assets::model::ModelCtx;
use nvf_coop::code::Lobby;
use crate::locomotion::Dying;
use crate::residents::{RemotePose, Residents};
use nvf_coop::net::{self, attack, flag, npc_flag, Event, Link, NpcState, Place, PlayerState, QuestView, Session};
use fnv_game::coop::HostQuest;
use nvf_play::lockpick::{LockOpened, OpenedBy};
use nvf_play::script_events::{Objective, QuestNotice};
use nvf_play::combat::{ActorHit, CombatStats, RelayedHit, RemoteHit};
use nvf_coop::reach::{self, Reach};
use nvf_play::loading::{Loading, TravelRequest};
use nvf_play::lobby::{Hosting, Joining, LobbyRequest, LobbyState};
use nvf_play::player::Player;
use nvf_play::{Location, PlayState, RefVisual, Travel};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Duration;

/// The port `--join` uses when the address names none (and a code without a port means).
pub const DEFAULT_PORT: u16 = nvf_coop::code::DEFAULT_PORT;
/// How far behind the newest state the others are drawn, so there are two to blend.
const DELAY: f32 = 0.1;
/// Outdoors, a guest farther than this from the host is brought back (game units; the
/// loaded cells reach about two cells, 8192 units, around a player).
const LEASH: f32 = 6000.0;
/// A leashed guest arrives this far behind the host.
const BEHIND: f32 = 96.0;
/// After a leash trip, the next waits this long (the trip's loading screen comes first).
const LEASH_EVERY: f32 = 5.0;
/// The puppets' references: plugin index FE, which no FNV load order uses.
const PUPPET_REF: u32 = 0xFE00_C000;
/// The record a player with a look is drawn from: the player's own, as this game draws
/// its own body (`player_body.rs`).
const PLAYER_BASE: u32 = FormId::PLAYER_BASE.0;
/// A puppet holds its aim this long after an attack, aimed or not (seconds).
const AIM_AFTER_ATTACK: f32 = 1.5;
/// Where a guest keeps its token across the restarts loads cause.
pub const TOKEN_ENV: &str = "NVF_COOP_TOKEN";
/// ... and in this file in its home, across games started by hand.
const TOKEN_FILE: &str = "coop-token.txt";
/// A host's lobby across the restarts loads cause: its port, and its code.
pub const HOST_ENV: &str = "NVF_COOP_HOST";
pub const CODE_ENV: &str = "NVF_COOP_CODE";
/// A guest's host (its code or address) across them, and the guest's name.
pub const JOIN_ENV: &str = "NVF_COOP_JOIN";
pub const NAME_ENV: &str = "NVF_COOP_NAME";
/// The code is written here in the home too, to be copied.
const CODE_FILE: &str = "lobby-code.txt";
/// A guest looking for its host gives up after this long without an answer (seconds).
const FIND_FOR: f32 = 10.0;

/// What the command line asked for.
#[derive(Clone)]
pub struct Config {
    pub host: Option<u16>,
    pub join: Option<String>,
    pub name: String,
    /// The NPC (base FormID) the other players are drawn as.
    pub base: u32,
}

pub struct CoopPlugin(pub Config);

/// On a co-op puppet's root: the systems that would drive an NPC from the simulation
/// (drawing its weapon in alarm, sneaking) leave it to the network.
#[derive(Component)]
pub struct Puppet;

/// On a guest's NPC that the host draws: as [`Puppet`], the systems that would drive it
/// from this game's simulation leave it to the network.
#[derive(Component)]
pub struct Remote;

/// Another player fired a shot (its puppet's attack with a gun), for the world's actors
/// to hear as they hear the player's (`detection`).
#[derive(Message, Clone, Copy, Debug)]
pub struct PuppetShot {
    pub shooter: FormId,
    pub weapon: FormId,
}

/// How far behind the host's newest NPC states a guest draws them: two rounds.
const NPC_DELAY: f32 = 2.0 / net::NPC_HZ as f32;
/// A guest stops following an NPC the host has not drawn for this long (seconds); it
/// stays where it was last drawn.
const NPC_FORGET: f32 = 3.0;

impl Plugin for CoopPlugin {
    fn build(&self, app: &mut App) {
        let c = &self.0;
        let env = |k: &str| std::env::var(k).ok().filter(|v| !v.trim().is_empty());
        let name = env(NAME_ENV).unwrap_or_else(|| c.name.clone());
        let mut coop = Coop {
            link: None,
            base: c.base,
            name,
            peers: BTreeMap::new(),
            host: None,
            host_lost: false,
            last_leash: None,
            port: DEFAULT_PORT,
            code: None,
            note: None,
            opened: false,
            reaching: None,
            join_code: None,
            joining: Joining::Idle,
            finding_since: None,
            npcs: HashMap::new(),
            npc_attacks: HashMap::new(),
            ai_was_off: None,
            host_quest: None,
        };
        if let Some(port) = c.host.or_else(|| env(HOST_ENV).and_then(|p| p.trim().parse().ok())) {
            coop.start_hosting(port);
        } else if let Some(join) = c.join.clone().or_else(|| env(JOIN_ENV)) {
            coop.start_joining(&join);
        }
        app.add_plugins(nvf_play::lobby::LobbyPlugin)
            .insert_resource(coop)
            .add_systems(Update, (lobby, send, take_events, share_npcs, follow_npcs, leash).chain().after(nvf_play::PlaySet::Physics))
            .add_message::<PuppetShot>()
            .add_systems(Update, (stand_ins, draw).chain().after(nvf_play::PlaySet::Sync).before(crate::locomotion::walk_test));
    }
}

/// The host's session on `port`. A load restarts the game: the run before may hold the
/// port for a moment.
fn host_session(port: u16) -> Option<Link> {
    for attempt in 0..40 {
        match Session::host(port) {
            Ok(s) => {
                info!("co-op: hosting on UDP port {port}");
                return Some(Link::new(s));
            }
            Err(e) if attempt == 39 => error!("co-op: cannot host on UDP port {port}: {e}"),
            Err(_) => std::thread::sleep(Duration::from_millis(250)),
        }
    }
    None
}

/// A guest's session with the host a code or an address names.
fn guest_session(at: &str, name: &str) -> Result<Link, String> {
    let (candidates, key) = match Lobby::decode(at) {
        Some(l) => (l.candidates(reach::local_ip()), l.key()),
        None if at.contains('.') || at.contains(':') => (resolve(at).into_iter().collect(), 0),
        None => (Vec::new(), 0),
    };
    if candidates.is_empty() {
        return Err("That is not a lobby code: check it with the host.".into());
    }
    // The token is kept in the home too, so a game started again by hand (after a crash)
    // is the same player to the host, not a second one beside its frozen self.
    let file = fnv_install::EngineHome::default_root().map(|h| h.join(TOKEN_FILE));
    let parse = |t: &str| u64::from_str_radix(t.trim(), 16).ok();
    let token = std::env::var(TOKEN_ENV)
        .ok()
        .and_then(|t| parse(&t))
        .or_else(|| file.as_ref().and_then(|f| std::fs::read_to_string(f).ok()).and_then(|t| parse(&t)))
        .unwrap_or_else(net::new_token);
    // Inherited by the game a load restarts.
    std::env::set_var(TOKEN_ENV, format!("{token:016x}"));
    if let Some(f) = file {
        if let Err(e) = std::fs::create_dir_all(f.parent().unwrap_or(&f)).and_then(|()| std::fs::write(&f, format!("{token:016x}\n"))) {
            warn!("co-op: cannot keep the session token in {}: {e}", f.display());
        }
    }
    match Session::join(&candidates, key, name, token) {
        Ok(s) => {
            info!("co-op: joining {candidates:?} as {name}");
            Ok(Link::new(s))
        }
        Err(e) => Err(format!("Cannot open a connection: {e}")),
    }
}

fn resolve(at: &str) -> Option<SocketAddr> {
    let with_port = if at.rsplit_once(':').is_some_and(|(_, p)| p.parse::<u16>().is_ok()) { at.to_string() } else { format!("{at}:{DEFAULT_PORT}") };
    with_port.to_socket_addrs().ok()?.find(|a| a.is_ipv4()).or_else(|| with_port.to_socket_addrs().ok()?.next())
}

#[derive(Resource)]
struct Coop {
    /// None while no lobby is open or joined.
    link: Option<Link>,
    base: u32,
    name: String,
    peers: BTreeMap<u8, Peer>,
    /// The host's newest state (guests only).
    host: Option<PlayerState>,
    host_lost: bool,
    /// When the last leash trip was asked for (seconds of `Time`).
    last_leash: Option<f32>,
    /// Hosting: the port, the lobby's code once known and what to tell the host about it,
    /// whether the router opened the port, and the router's answer while it is asked.
    port: u16,
    code: Option<String>,
    note: Option<String>,
    opened: bool,
    reaching: Option<Mutex<Receiver<Reach>>>,
    /// Joining: the code looked for, how the search goes, and since when it has looked
    /// (None: not yet timed; set on the first frame).
    join_code: Option<String>,
    joining: Joining,
    finding_since: Option<f32>,
    /// A guest's: the host's NPCs as heard, by reference.
    npcs: HashMap<u32, NpcTrack>,
    /// A host's: the attacks each of its NPCs began, counted (wrapping).
    npc_attacks: HashMap<u32, u8>,
    /// A guest following the host's world: whether this game's actors' AI was off before.
    ai_was_off: Option<bool>,
    /// A guest's: the host's active quest as last heard.
    host_quest: Option<HostQuest>,
}

/// One of the host's NPCs as a guest has heard it.
#[derive(Default)]
struct NpcTrack {
    samples: VecDeque<(f32, NpcState)>,
    /// The attack counter played up to (None until the first state).
    played: Option<u8>,
    attacked_at: Option<f32>,
}

impl NpcTrack {
    /// Where to draw it at `t`: feet and heading blended between the states either side.
    fn pose(&self, t: f32) -> Option<([f32; 3], f32, bool)> {
        let last = self.samples.back()?.1;
        let (a, b, f) = match self.samples.iter().position(|(at, _)| *at > t) {
            None => return Some((last.feet, last.heading, false)),
            Some(0) => return Some((self.samples[0].1.feet, self.samples[0].1.heading, false)),
            Some(i) => {
                let (ta, a) = self.samples[i - 1];
                let (tb, b) = self.samples[i];
                (a, b, ((t - ta) / (tb - ta).max(1e-4)).clamp(0.0, 1.0))
            }
        };
        let feet = std::array::from_fn(|k| a.feet[k] + (b.feet[k] - a.feet[k]) * f);
        let turn = (b.heading - a.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        Some((feet, a.heading + turn * f, b.has(npc_flag::MOVING)))
    }
}

impl Coop {
    fn start_hosting(&mut self, port: u16) {
        let Some(link) = host_session(port) else {
            self.note = Some(format!("Cannot open UDP port {port} on this PC: is another game using it?"));
            return;
        };
        self.link = Some(link);
        self.port = port;
        std::env::set_var(HOST_ENV, port.to_string());
        // After a restart the lobby's code is the one friends were given.
        if let Some((code, lobby)) = std::env::var(CODE_ENV).ok().and_then(|c| Some((c.clone(), Lobby::decode(&c)?))).filter(|(_, l)| l.port == port) {
            if let Some(link) = &self.link {
                link.set_lobby(lobby.key());
            }
            self.code = Some(code);
            return;
        }
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("coop-reach".into())
            .spawn(move || {
                let _ = tx.send(reach::open(port));
            })
            .ok();
        self.reaching = Some(Mutex::new(rx));
    }

    fn stop_hosting(&mut self, commands: &mut Commands, fighters: &mut Fighters) {
        // Dropping the link says goodbye to the guests.
        self.link = None;
        if self.opened {
            let port = self.port;
            std::thread::Builder::new().name("coop-reach".into()).spawn(move || reach::close(port)).ok();
        }
        self.code = None;
        self.note = None;
        self.opened = false;
        self.reaching = None;
        std::env::remove_var(HOST_ENV);
        std::env::remove_var(CODE_ENV);
        self.forget_peers(commands, fighters);
    }

    fn start_joining(&mut self, at: &str) {
        match guest_session(at, &self.name) {
            Ok(link) => {
                self.link = Some(link);
                self.join_code = Some(at.trim().to_string());
                self.joining = Joining::Finding;
                self.finding_since = None;
            }
            Err(why) => {
                warn!("co-op: {why}");
                self.joining = Joining::Failed(why);
            }
        }
    }

    fn stop_joining(&mut self, commands: &mut Commands, fighters: &mut Fighters) {
        self.link = None;
        self.join_code = None;
        self.joining = Joining::Idle;
        self.host = None;
        self.host_lost = false;
        std::env::remove_var(JOIN_ENV);
        self.forget_peers(commands, fighters);
    }

    fn forget_peers(&mut self, commands: &mut Commands, fighters: &mut Fighters) {
        for (id, p) in std::mem::take(&mut self.peers) {
            fighters.set_puppet(PUPPET_REF + id as u32, None);
            if let Some(e) = p.root {
                commands.entity(e).despawn();
            }
        }
    }

    fn is_host(&self) -> bool {
        self.link.as_ref().is_some_and(|l| l.is_host())
    }

    fn is_guest(&self) -> bool {
        self.link.as_ref().is_some_and(|l| !l.is_host())
    }
}

/// The main menu is up: this game has no player in the world yet.
fn at_main_menu(m: Option<&nvf_play::main_menu::MainMenu>) -> bool {
    m.is_some_and(|m| m.open)
}

/// The lobby screens' asks, the router's answer, and what the screens show.
#[allow(clippy::too_many_arguments)]
fn lobby(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    mut asks: MessageReader<LobbyRequest>,
    mut state: ResMut<LobbyState>,
    mut fighters: ResMut<Fighters>,
    mut loads: MessageWriter<nvf_play::save::LoadRequest>,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    windows: Query<&mut Window, With<PrimaryWindow>>,
    mut titled: Local<Option<String>>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let mut say = |n: String| {
        info!("co-op: {n}");
        if let Some(c) = corner.as_mut() {
            c.push(n.into());
        }
    };
    for ask in asks.read() {
        match ask {
            LobbyRequest::Create if coop.is_guest() => say("You are in another player's lobby: leave it to open your own".into()),
            LobbyRequest::Create if coop.link.is_none() => coop.start_hosting(DEFAULT_PORT),
            LobbyRequest::Create => {}
            LobbyRequest::Close if coop.is_host() => {
                coop.stop_hosting(&mut commands, &mut fighters);
                say("Lobby closed".into());
            }
            LobbyRequest::Close => {}
            LobbyRequest::Find(code) => {
                if coop.is_host() {
                    say("Close your own lobby to join another".into());
                    continue;
                }
                coop.stop_joining(&mut commands, &mut fighters);
                coop.start_joining(code);
            }
            LobbyRequest::Cancel => {
                if coop.is_guest() && at_main_menu(main_menu.as_deref()) {
                    coop.stop_joining(&mut commands, &mut fighters);
                }
                if matches!(coop.joining, Joining::Failed(_)) {
                    coop.joining = Joining::Idle;
                }
            }
            LobbyRequest::Load { path, name } => {
                let Some(code) = coop.join_code.clone().filter(|_| coop.is_guest()) else { continue };
                // The game the load starts joins the same host, as the same player (the
                // token is in TOKEN_ENV); this one leaves without a goodbye, so the host
                // keeps its place.
                std::env::set_var(JOIN_ENV, &code);
                if !name.trim().is_empty() {
                    std::env::set_var(NAME_ENV, name.trim());
                }
                if let Some(link) = coop.link.take() {
                    link.abandon();
                }
                info!("co-op: loading {} to join {code}", path.display());
                loads.write(nvf_play::save::LoadRequest { path: path.clone() });
            }
        }
    }
    // The router's answer: the lobby's code.
    if let Some(r) = coop.reaching.as_ref().and_then(|rx| rx.lock().ok()?.try_recv().ok()) {
        coop.reaching = None;
        let code = r.lobby.encode();
        info!("co-op: lobby code {code} (address {}, this network {}, port {}; router opened it: {})", r.lobby.addr, r.local, r.lobby.port, r.opened);
        if let Some(link) = &coop.link {
            link.set_lobby(r.lobby.key());
        }
        std::env::set_var(CODE_ENV, &code);
        if let Some(f) = fnv_install::EngineHome::default_root().map(|h| h.join(CODE_FILE)) {
            if let Err(e) = std::fs::write(&f, format!("{code}\n")) {
                warn!("co-op: cannot write {}: {e}", f.display());
            }
        }
        if let Some(n) = &r.note {
            warn!("co-op: {n}");
        }
        say(format!("Lobby open: code {code}"));
        coop.code = Some(code);
        coop.note = r.note;
        coop.opened = r.opened;
    }
    // A guest's search: answered, refused (events, `take_events`), or given up.
    if coop.joining == Joining::Finding {
        let welcomed = coop.link.as_ref().is_some_and(|l| l.id().is_some());
        let since = *coop.finding_since.get_or_insert(now);
        if welcomed {
            coop.joining = Joining::Found;
        } else if now - since > FIND_FOR {
            coop.link = None;
            coop.joining = Joining::Failed("No answer from the host: check the code, and that their lobby is open.".into());
        }
    }
    let hosting = match (&coop.code, coop.is_host(), coop.reaching.is_some()) {
        (Some(code), true, _) => Hosting::Open { code: code.clone(), note: coop.note.clone() },
        (_, true, true) => Hosting::Opening,
        _ => Hosting::Off,
    };
    let mut players = Vec::new();
    if coop.link.is_some() {
        players.push(if coop.is_host() { format!("{} (host)", coop.name) } else { coop.name.clone() });
        for (&id, p) in &coop.peers {
            let n = label(id, &p.name);
            players.push(if p.lost { format!("{n} (reconnecting)") } else { n });
        }
    }
    state.set_if_neq(LobbyState { hosting, players, joining: coop.joining.clone() });
    // The window's title says what this game is in co-op.
    let what = match coop.link.as_ref().map(|l| (l.is_host(), l.id())) {
        None => "NVForever".to_string(),
        Some((true, _)) => format!("NVForever co-op (host on port {})", coop.port),
        Some((false, Some(id))) => format!("NVForever co-op (guest, player {})", id + 1),
        Some((false, None)) => "NVForever co-op (joining)".into(),
    };
    if titled.as_deref() != Some(what.as_str()) {
        set_title(&what, windows);
        *titled = Some(what);
    }
}

#[derive(Default)]
struct Peer {
    name: String,
    /// States as they came (seconds of `Time`, state), oldest first.
    samples: VecDeque<(f32, PlayerState)>,
    lost: bool,
    spawned: bool,
    root: Option<Entity>,
    /// The space the puppet was drawn in.
    drawn_in: Option<Place>,
    failed: bool,
    /// The attack and reload counters played up to (None until the first state: those
    /// before it are not played).
    played: Option<(u16, u16)>,
    /// When its last attack was played (seconds of `Time`).
    attacked_at: Option<f32>,
    /// Its look, once heard: `Some(None)` for the player record's own (no character
    /// creator look).
    look: Option<Option<Look>>,
    /// On the host: its actor in this world ([`stand_ins`]).
    actor: Option<FormId>,
}

impl Peer {
    /// The reference its puppet is drawn with: its actor's on the host, else
    /// [`PUPPET_REF`] + its id.
    fn reference(&self, id: u8) -> u32 {
        self.actor.map_or(PUPPET_REF + id as u32, |r| r.0)
    }

    /// The pose to draw at `t`: blended between the states either side of it.
    fn pose(&self, t: f32) -> Option<(Place, [f32; 3], f32, bool)> {
        let last = self.samples.back()?.1;
        let i = self.samples.iter().position(|(at, _)| *at > t);
        let (a, b, f) = match i {
            None => return Some((last.place, last.feet, last.yaw, false)),
            Some(0) => {
                let s = self.samples[0].1;
                return Some((s.place, s.feet, s.yaw, false));
            }
            Some(i) => {
                let (ta, a) = self.samples[i - 1];
                let (tb, b) = self.samples[i];
                (a, b, ((t - ta) / (tb - ta).max(1e-4)).clamp(0.0, 1.0))
            }
        };
        if a.place != b.place {
            return Some((b.place, b.feet, b.yaw, false));
        }
        let feet = std::array::from_fn(|k| a.feet[k] + (b.feet[k] - a.feet[k]) * f);
        let turn = (b.yaw - a.yaw + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        let moved = (0..3).map(|k| (b.feet[k] - a.feet[k]).powi(2)).sum::<f32>() > 0.25;
        Some((b.place, feet, a.yaw + turn * f, moved && !self.lost))
    }
}

fn label(id: u8, name: &str) -> String {
    match (id, name.is_empty()) {
        (0, true) => "The host".into(),
        (_, true) => format!("Player {}", id + 1),
        _ => name.to_string(),
    }
}

fn place_of(loc: &Location) -> Place {
    loc.interior.map_or(Place::Exterior(loc.world), Place::Interior)
}

fn set_title(what: &str, mut windows: Query<&mut Window, With<PrimaryWindow>>) {
    for mut w in &mut windows {
        w.title = what.to_string();
    }
}

/// What the player is doing, as the combat and control systems leave it this frame.
#[derive(bevy::ecs::system::SystemParam)]
struct Doing<'w> {
    control: Option<Res<'w, nvf_play::control::PlayerControl>>,
    anim: Option<Res<'w, nvf_play::combat::WeaponAnim>>,
    combat: Option<Res<'w, nvf_play::combat::Combat>>,
    zoom: Option<Res<'w, nvf_play::aim_zoom::AimZoom>>,
    phys: Option<Res<'w, nvf_play::phys::PhysState>>,
    actor_world: Option<Res<'w, ActorWorld>>,
    sim: Option<Res<'w, nvf_play::sim::SimState>>,
    script: Option<Res<'w, nvf_play::script::ScriptState>>,
}

/// Hand the link this game's newest state (it sends it on its own thread).
fn send(coop: Res<Coop>, player: Option<Res<Player>>, loc: Option<Res<Location>>, state: Option<Res<State<PlayState>>>, main_menu: Option<Res<nvf_play::main_menu::MainMenu>>, doing: Doing) {
    let (Some(link), Some(player), Some(loc)) = (coop.link.as_ref(), player, loc) else { return };
    // At the main menu there is no player yet (the link says it is there all the same).
    if at_main_menu(main_menu.as_deref()) {
        return;
    }
    // Behind a loading screen the place is the one being left: keep the last state sent.
    if state.is_some_and(|s| *s.get() == PlayState::Loading) {
        return;
    }
    let mut s = PlayerState::at(place_of(&loc), player.feet, player.heading);
    s.pitch = player.pitch;
    let control = doing.control.as_deref();
    let out = control.is_some_and(|c| c.weapon_out);
    let aiming = out && control.is_some_and(|c| c.aiming);
    let mut flags = 0;
    flags |= if out { flag::DRAWN } else { 0 };
    flags |= if aiming { flag::AIMING } else { 0 };
    flags |= if control.is_some_and(|c| c.sneak) { flag::SNEAKING } else { 0 };
    flags |= if doing.phys.as_ref().and_then(|p| p.player.as_ref()).is_some_and(|c| !c.grounded) { flag::AIR } else { 0 };
    let sights = match (doing.combat.as_deref(), doing.anim.as_deref()) {
        (Some(c), Some(a)) => c.player_sights_up(a, aiming),
        _ => false,
    } || doing.zoom.as_ref().is_some_and(|z| z.aimed);
    flags |= if sights && out { flag::SIGHTS } else { 0 };
    flags |= if doing.script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(FormId::PLAYER_REF)) { flag::DOWN } else { 0 };
    s.flags = flags;
    let worn = match (doing.actor_world.as_deref(), doing.sim.as_deref()) {
        (Some(w), Some(sim)) => Equipment::of(w, Some(&sim.world), FormId::PLAYER_REF.0),
        _ => None,
    };
    let weapon = doing.combat.as_ref().and_then(|c| c.player_weapon()).or(worn.as_ref().and_then(|e| e.weapon));
    s.weapon = weapon.map_or(0, |w| w.0);
    for (slot, w) in s.worn.iter_mut().zip(worn.iter().flat_map(|e| e.worn.iter())) {
        *slot = w.0;
        s.worn_len += 1;
    }
    if let Some(a) = doing.anim.as_deref() {
        s.attacks = a.attacks as u16;
        s.reloads = a.reloads as u16;
        s.attack_kind = attack_kind(a.swing.as_deref());
    }
    link.set_state(s);
    // The link sends it again only when it changes (and every LOOK_EVERY).
    let look = doing.script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.player.look.as_ref());
    link.set_look(encode_look(look));
}

/// A look as it goes over the link: 0 for the player record's own; else 1, the race, sex,
/// hair (0 for bald), hair colour (a flag and RGB), eyes (0 for the race's), the head
/// parts, and the three FaceGen lists, each list a count and its items.
pub fn encode_look(look: Option<&Look>) -> Vec<u8> {
    let Some(l) = look else { return vec![0] };
    let mut b = vec![1];
    let id = |b: &mut Vec<u8>, f: Option<FormId>| b.extend_from_slice(&f.map_or(0, |f| f.0).to_le_bytes());
    id(&mut b, Some(l.race));
    b.push(l.female as u8);
    id(&mut b, l.hair);
    match l.hair_color {
        Some(c) => b.extend_from_slice(&[1, c[0], c[1], c[2]]),
        None => b.extend_from_slice(&[0, 0, 0, 0]),
    }
    id(&mut b, l.eyes);
    let parts = &l.head_parts[..l.head_parts.len().min(16)];
    b.push(parts.len() as u8);
    for p in parts {
        id(&mut b, Some(*p));
    }
    for list in [&l.fggs, &l.fgga, &l.fgts] {
        let list = &list[..list.len().min(64)];
        b.push(list.len() as u8);
        for v in list {
            b.extend_from_slice(&v.to_le_bytes());
        }
    }
    b
}

/// [`encode_look`] read back; None for one that is not a look.
pub fn decode_look(b: &[u8]) -> Option<Option<Look>> {
    match *b.first()? {
        0 => return Some(None),
        1 => {}
        _ => return None,
    }
    let mut rd = Reader(b.get(1..)?);
    let race = FormId(rd.u32()?);
    let female = rd.u8()? != 0;
    let hair = Some(rd.u32()?).filter(|&h| h != 0).map(FormId);
    let c = [rd.u8()?, rd.u8()?, rd.u8()?, rd.u8()?];
    let hair_color = (c[0] != 0).then_some([c[1], c[2], c[3]]);
    let eyes = Some(rd.u32()?).filter(|&e| e != 0).map(FormId);
    let n = rd.u8()? as usize;
    let head_parts = (0..n).map(|_| rd.u32().map(FormId)).collect::<Option<Vec<_>>>()?;
    let list = |rd: &mut Reader| -> Option<Vec<f32>> {
        let n = rd.u8()? as usize;
        (0..n).map(|_| rd.u32().map(f32::from_bits)).collect()
    };
    let (fggs, fgga, fgts) = (list(&mut rd)?, list(&mut rd)?, list(&mut rd)?);
    rd.0.is_empty().then_some(Some(Look { race, female, hair, hair_color, eyes, head_parts, fggs, fgga, fgts }))
}

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn u8(&mut self) -> Option<u8> {
        let (&v, rest) = self.0.split_first()?;
        self.0 = rest;
        Some(v)
    }

    fn u32(&mut self) -> Option<u32> {
        let v = u32::from_le_bytes(self.0.get(..4)?.try_into().ok()?);
        self.0 = &self.0[4..];
        Some(v)
    }
}

/// What the newest attack was, from the melee swing's clip stem (`attackleft_b`,
/// `attackforwardpower`; none for a shot).
fn attack_kind(swing: Option<&str>) -> u8 {
    let Some(stem) = swing.map(str::to_ascii_lowercase) else { return attack::SHOT };
    if !stem.contains("power") {
        return attack::SWING;
    }
    if stem.contains("forward") {
        attack::POWER_FORWARD
    } else if stem.contains("back") {
        attack::POWER_BACK
    } else if stem.contains("left") {
        attack::POWER_LEFT
    } else if stem.contains("right") {
        attack::POWER_RIGHT
    } else {
        attack::POWER
    }
}

/// A weapon swung rather than fired (fists when none).
fn is_melee(world: Option<&ActorWorld>, weapon: Option<FormId>) -> bool {
    let Some(w) = weapon else { return true };
    let kind = world.and_then(|wd| wd.actors.weapons.get(&w)).and_then(|w| w.anim_type.as_deref()).and_then(AnimType::from_name);
    matches!(kind, Some(AnimType::HandToHand | AnimType::Melee1 | AnimType::Melee2))
}

fn take_events(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    mut fighters: ResMut<Fighters>,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    actor_world: Option<Res<ActorWorld>>,
    mut remote_hits: MessageWriter<RemoteHit>,
    (mut sim, mut script, mut quest_lines): (Option<ResMut<nvf_play::sim::SimState>>, Option<ResMut<nvf_play::script::ScriptState>>, Option<ResMut<nvf_play::quest_messages::QuestMessages>>),
    mut opened: MessageReader<LockOpened>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let Some(link) = coop.link.as_ref() else {
        opened.clear();
        return;
    };
    // What this player unlocked goes to everyone (module notes).
    for o in opened.read() {
        info!("co-op: this player {} {:#010X}: everyone's is unlocked", o.how.word(), o.reference);
        link.send_unlock(o.reference, o.how == OpenedBy::Hacked);
    }
    let mut rt = script.as_deref_mut().and_then(|s| s.rt.as_mut());
    let is_host = link.is_host();
    let mut notes = Vec::new();
    let mut refused = false;
    for e in link.events() {
        match e {
            Event::Welcomed(id) => notes.push(format!("Joined the host's game as player {}", id + 1)),
            Event::Refused => {
                notes.push("The host's game is full".into());
                refused = true;
            }
            Event::Joined { id, name } => {
                let p = coop.peers.entry(id).or_default();
                if !name.is_empty() {
                    p.name = name;
                }
                if is_host {
                    notes.push(format!("{} joined", label(id, &p.name)));
                }
            }
            Event::State { id, state } => {
                if id == 0 {
                    coop.host = Some(state);
                }
                let p = coop.peers.entry(id).or_default();
                p.samples.push_back((now, state));
                while p.samples.len() > 16 {
                    p.samples.pop_front();
                }
            }
            Event::Lost(id) => {
                let p = coop.peers.entry(id).or_default();
                p.lost = true;
                // A guest hears of the host's loss as HostLost too: say it once.
                if id != 0 || is_host {
                    notes.push(format!("{} lost connection, waiting for them", label(id, &p.name)));
                }
            }
            Event::Back(id) => {
                let p = coop.peers.entry(id).or_default();
                p.lost = false;
                if id != 0 || is_host {
                    notes.push(format!("{} reconnected", label(id, &p.name)));
                }
            }
            Event::Look { id, look } => {
                let Some(look) = decode_look(&look) else {
                    warn!("co-op: {} sent a look this game cannot read", label(id, &coop.peers.get(&id).map_or(String::new(), |p| p.name.clone())));
                    continue;
                };
                let reference = coop.peers.get(&id).map_or(PUPPET_REF + id as u32, |p| p.reference(id));
                if let Some(w) = actor_world.as_deref() {
                    if let Ok(mut looks) = w.looks.lock() {
                        match &look {
                            Some(l) => looks.insert(reference, nvf_play::creator::actor_look(l)),
                            None => looks.remove(&reference),
                        };
                    }
                }
                let p = coop.peers.entry(id).or_default();
                info!("co-op: {} looks {}", label(id, &p.name), look.as_ref().map_or("as the player record does".to_string(), |l| format!("race {}, {}", l.race, if l.female { "female" } else { "male" })));
                p.look = Some(look);
                // Drawn again, as it now looks.
                if let Some(e) = p.root.take() {
                    commands.entity(e).despawn();
                }
                p.spawned = false;
                p.drawn_in = None;
                p.failed = false;
            }
            Event::Left(id) => {
                let reference = coop.peers.get(&id).map_or(PUPPET_REF + id as u32, |p| p.reference(id));
                fighters.set_puppet(reference, None);
                if let Some(w) = actor_world.as_deref() {
                    if let Ok(mut looks) = w.looks.lock() {
                        looks.remove(&reference);
                    }
                }
                if let Some(p) = coop.peers.remove(&id) {
                    if let Some(e) = p.root {
                        commands.entity(e).despawn();
                    }
                    if id != 0 {
                        notes.push(format!("{} left", label(id, &p.name)));
                    }
                }
                if id == 0 {
                    coop.host = None;
                }
            }
            Event::HostLost => {
                coop.host_lost = true;
                notes.push("Lost the host: reconnecting".into());
            }
            Event::HostBack => {
                coop.host_lost = false;
                notes.push("Reconnected to the host".into());
            }
            Event::HostGone => {
                coop.host = None;
                notes.push("The host's game has ended".into());
            }
            Event::Npcs(list) => {
                for n in list {
                    let t = coop.npcs.entry(n.reference).or_default();
                    t.samples.push_back((now, n));
                    while t.samples.len() > 8 {
                        t.samples.pop_front();
                    }
                }
            }
            // A guest's, as its actor's in this world (module notes).
            Event::Hit { id, hit } if is_host => match decode_hit(&hit) {
                Some(h) => {
                    let by = coop.peers.get(&id).and_then(|p| p.actor).unwrap_or(FormId::PLAYER_REF);
                    info!("co-op: player {} ({by}) hit {} for {:.1}", id + 1, h.target, h.dealt);
                    remote_hits.write(RemoteHit { by, hit: h });
                }
                None => warn!("co-op: player {} sent a hit this game cannot read", id + 1),
            },
            // One of the host's actors hit this player there.
            Event::Quest(q) => {
                let new = host_quest(&q);
                if let Some(rt) = rt.as_deref() {
                    let notices = host_quest_notices(rt.game.data, coop.host_quest.as_ref(), new.as_ref());
                    for n in notices {
                        info!("co-op: the host's quest: {}", nvf_play::quest_messages::describe(&n));
                        if let Some(l) = quest_lines.as_deref_mut() {
                            l.push(n);
                        }
                    }
                }
                coop.host_quest = new;
            }
            Event::Unlocked { id, reference, terminal } => {
                if let (Some(rt), Some(sim)) = (rt.as_deref_mut(), sim.as_deref_mut()) {
                    if unlock(sim, &mut rt.game, FormId(reference), terminal) {
                        info!("co-op: {} unlocked {reference:#010X}{}", label(id, &coop.peers.get(&id).map_or(String::new(), |p| p.name.clone())), if terminal { " (a terminal)" } else { "" });
                    }
                }
            }
            Event::Hit { hit, .. } => match decode_hurt(&hit) {
                Some((by, h)) => {
                    info!("co-op: {} hit this player for {:.1} in the host's world", by.map_or("something".to_string(), |b| b.to_string()), h.dealt);
                    remote_hits.write(RemoteHit { by: by.unwrap_or(FormId(0)), hit: RelayedHit { target: FormId::PLAYER_REF, ..h } });
                }
                None => warn!("co-op: the host sent a hit this game cannot read"),
            },
        }
    }
    for n in notes {
        info!("co-op: {n}");
        if let Some(c) = corner.as_mut() {
            c.push(n.into());
        }
    }
    if refused {
        coop.link = None;
        coop.joining = Joining::Failed("The host's game is full.".into());
    }
}

/// Guests: go to the host when it is in another space, or too far away outdoors.
fn leash(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    player: Option<Res<Player>>,
    loc: Option<Res<Location>>,
    loading: Option<Res<Loading>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    mut travel: MessageWriter<TravelRequest>,
) {
    if !coop.is_guest() || coop.host_lost || at_main_menu(main_menu.as_deref()) {
        return;
    }
    let (Some(host), Some(player), Some(loc)) = (coop.host, player, loc) else { return };
    if loading.is_some_and(|l| l.active()) {
        return;
    }
    let now = time.elapsed_secs();
    if coop.last_leash.is_some_and(|t| now - t < LEASH_EVERY) {
        return;
    }
    let here = place_of(&loc);
    let (dx, dy) = (player.feet[0] - host.feet[0], player.feet[1] - host.feet[1]);
    let far = matches!(here, Place::Exterior(_)) && dx * dx + dy * dy > LEASH * LEASH;
    if here == host.place && !far {
        return;
    }
    // Behind the host: its heading 0 faces north (+Y), π/2 east (+X).
    let (s, c) = host.yaw.sin_cos();
    let feet = [host.feet[0] - s * BEHIND, host.feet[1] - c * BEHIND, host.feet[2]];
    let (world, interior) = match host.place {
        Place::Interior(c) => (None, Some(c)),
        Place::Exterior(w) => (Some(w), None),
    };
    info!("co-op: leash: to the host at {:?} {feet:?}", host.place);
    travel.write(TravelRequest(Travel { world, interior, feet, heading_deg: host.yaw.to_degrees() }));
    coop.last_leash = Some(now);
}

/// Draw each other player where the link last put it, in this game's space only.
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    loc: Option<Res<Location>>,
    actor_world: Option<Res<ActorWorld>>,
    mut ahead: ResMut<crate::readahead::ReadAhead>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    mut parts: ResMut<ActorParts>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    roots: Query<(Entity, &RefVisual, &ChildOf)>,
    mut actors: Query<(&mut Transform, Option<&mut Locomotion>), Without<crate::furniture::OnFurniture>>,
    mut holds: Query<&mut WeaponHold>,
    dressed: Query<(Option<&PuppetEquipment>, Has<Puppet>)>,
    (mut fighters, mut attacks, main_menu, mut shots): (ResMut<Fighters>, MessageWriter<PlayAttack>, Option<Res<nvf_play::main_menu::MainMenu>>, MessageWriter<PuppetShot>),
) {
    let (Ok(world_root), Some(loc)) = (root.single(), loc) else { return };
    if at_main_menu(main_menu.as_deref()) {
        return;
    }
    let here = place_of(&loc);
    let now = time.elapsed_secs();
    let dt = time.delta_secs().min(0.1);
    let mut stats = crate::actors::ActorStats::default();
    let base = coop.base;
    for (&id, p) in coop.peers.iter_mut() {
        if p.failed {
            continue;
        }
        let Some((place, feet, yaw, moving)) = p.pose(now - DELAY) else { continue };
        // The world's entities go with the space they belong to: after a trip (here or
        // there), or once its entity is gone, the puppet is spawned again.
        if p.root.is_some_and(|e| roots.get(e).is_err()) || p.drawn_in.is_some_and(|d| d != here || d != place) {
            if let Some(e) = p.root.take().filter(|e| roots.get(*e).is_ok()) {
                commands.entity(e).despawn();
            }
            p.spawned = false;
            p.drawn_in = None;
        }
        let visible = place == here;
        let reference = p.reference(id);
        let Some(&(_, newest)) = p.samples.back() else { continue };
        let base = if p.look.is_some() { PLAYER_BASE } else { base };
        let who = Who { reference, base, creature: false, scale: 1.0, strafe: true, air: newest.has(flag::AIR) };
        let sim_world = sim.as_deref().map(|s| &s.world);
        if let Err(e) = show_actor(&mut commands, world_root, visible, actor_world.as_deref(), sim_world, &mut ahead, &mut stats, &mut parts, &mut models, &roots, &mut actors, who, (&mut p.spawned, &mut p.root), (feet, yaw), moving, dt) {
            warn!("co-op: {} cannot be drawn as {base:#010X}: {e}", label(id, &p.name));
            p.failed = true;
        }
        if visible && p.spawned {
            p.drawn_in = Some(here);
        }
        let Some(e) = p.root.filter(|_| visible) else {
            fighters.set_puppet(reference, None);
            continue;
        };
        if let Some(weapon) = act(&mut commands, e, reference, p, newest, now, actor_world.as_deref(), &mut actors, &mut holds, &dressed, &mut fighters, &mut attacks) {
            shots.write(PuppetShot { shooter: FormId(reference), weapon });
        }
    }
}

/// The host: each other player as an actor of this world (module notes), made when it
/// is first seen, put where it is each frame, and enabled only while it is here, in
/// touch and alive; the actors of players who left, or of a game no longer hosted, are
/// deleted. The hits this world's actors land on them go to their games.
#[allow(clippy::too_many_arguments)]
fn stand_ins(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    loc: Option<Res<Location>>,
    actor_world: Option<Res<ActorWorld>>,
    mut sim: Option<ResMut<nvf_play::sim::SimState>>,
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    (mut stats, main_menu, data_state): (ResMut<CombatStats>, Option<Res<nvf_play::main_menu::MainMenu>>, Option<Res<nvf_play::pipboy::data::DataState>>),
) {
    let (Some(sim), Some(rt)) = (sim.as_deref_mut(), script.as_deref_mut().and_then(|s| s.rt.as_mut())) else { return };
    let coop = &mut *coop;
    let host = coop.link.as_ref().is_some_and(|l| l.is_host());
    let kept: HashSet<FormId> = if host { coop.peers.values().filter_map(|p| p.actor).collect() } else { HashSet::new() };
    for r in rt.game.coop_actors.keys().copied().collect::<Vec<_>>() {
        if !kept.contains(&r) {
            let _ = sim.world.set_deleted(r, true);
            rt.game.coop_actors.remove(&r);
            rt.game.extra.remove(&r);
            info!("co-op: {r}, a player's actor, is gone");
        }
    }
    if !host {
        stats.coop_hits.clear();
        return;
    }
    if let Some(link) = coop.link.as_ref() {
        for (by, h) in stats.coop_hits.drain(..) {
            let Some((&id, _)) = coop.peers.iter().find(|(_, p)| p.actor == Some(h.target)) else { continue };
            info!("co-op: {} hit player {} for {:.1}", by.map_or("something".to_string(), |b| b.to_string()), id + 1, h.dealt);
            link.send_hit_to(id, encode_hurt(by, &h));
        }
    }
    let Some(loc) = loc.filter(|_| !at_main_menu(main_menu.as_deref())) else { return };
    if let Some(link) = coop.link.as_ref() {
        link.set_quest(quest_view(data_state.as_deref(), &rt.game));
    }
    let here = place_of(&loc);
    let now = time.elapsed_secs();
    let cell = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.cell);
    for (&id, p) in coop.peers.iter_mut() {
        let (Some((place, feet, yaw, _)), Some(&(_, newest))) = (p.pose(now - DELAY), p.samples.back()) else { continue };
        let r = match p.actor {
            Some(r) => r,
            None => match sim.world.place(FormId(PLAYER_BASE), cell, feet, [0.0, 0.0, yaw], 1) {
                Ok(r) => {
                    rt.game.coop_actors.insert(r, p.name.clone());
                    info!("co-op: {} is {r} in this world", label(id, &p.name));
                    // Its look and its puppet go under the new reference.
                    if let Some(w) = actor_world.as_deref() {
                        if let Ok(mut looks) = w.looks.lock() {
                            if let Some(l) = looks.remove(&(PUPPET_REF + id as u32)) {
                                looks.insert(r.0, l);
                            }
                        }
                    }
                    if let Some(e) = p.root.take() {
                        commands.entity(e).despawn();
                    }
                    p.spawned = false;
                    p.drawn_in = None;
                    p.actor = Some(r);
                    r
                }
                Err(e) => {
                    warn!("co-op: {} cannot be an actor of this world: {e}", label(id, &p.name));
                    continue;
                }
            },
        };
        if let Ok(s) = sim.world.get(r) {
            s.pos = feet;
            s.rot = [0.0, 0.0, yaw];
            s.cell = cell;
        }
        let _ = sim.world.set_enabled(r, place == here && !p.lost && !newest.has(flag::DOWN));
        if let Some(n) = rt.game.coop_actors.get_mut(&r).filter(|n| **n != p.name && !p.name.is_empty()) {
            n.clone_from(&p.name);
        }
        let x = rt.game.extra.entry(r).or_default();
        x.ai_off = true;
        x.force_sneak = newest.has(flag::SNEAKING);
    }
}

/// A host's NPCs as it draws them, for the guests (module notes).
#[allow(clippy::type_complexity)]
fn share_npcs(
    mut coop: ResMut<Coop>,
    actors: Query<(&RefVisual, &Transform, Option<&Locomotion>, Option<&WeaponHold>, Has<Dying>, &InheritedVisibility), (With<crate::limbs::ActorRig>, Without<Puppet>)>,
    fighters: Res<Fighters>,
    mut attacks: MessageReader<PlayAttack>,
    mut told: Local<usize>,
) {
    let coop = &mut *coop;
    let Some(link) = coop.link.as_ref().filter(|l| l.is_host()) else {
        attacks.clear();
        return;
    };
    for a in attacks.read().filter(|a| matches!(a.kind, Move::Attack)) {
        let c = coop.npc_attacks.entry(a.actor).or_default();
        *c = c.wrapping_add(1);
    }
    if coop.peers.is_empty() {
        return;
    }
    let mut npcs = Vec::new();
    for (v, t, lo, hold, dying, seen) in &actors {
        // The dead too, out of sight or not: a guest lays its own body down.
        if (!seen.get() && !dying) || is_puppet(v.0) {
            continue;
        }
        let r = v.0;
        let pitch = fighters.pitch(r);
        let mut flags = 0;
        flags |= if lo.is_some_and(|l| l.velocity[0].hypot(l.velocity[1]) > 1.0) { npc_flag::MOVING } else { 0 };
        flags |= if hold.is_some_and(|h| h.drawn) { npc_flag::DRAWN } else { 0 };
        flags |= if lo.is_some_and(|l| l.sneaking) { npc_flag::SNEAKING } else { 0 };
        flags |= if dying { npc_flag::DEAD } else { 0 };
        flags |= if pitch.is_some() { npc_flag::AIMING } else { 0 };
        flags |= if fighters.sights(r) { npc_flag::SIGHTS } else { 0 };
        npcs.push(NpcState {
            reference: r,
            feet: t.translation.to_array(),
            heading: crate::nav::tr_heading(t),
            pitch: pitch.unwrap_or(0.0),
            flags,
            weapon: fighters.weapon(r).map_or(0, |w| w.0),
            attacks: coop.npc_attacks.get(&r).copied().unwrap_or(0),
        });
    }
    if npcs.len().abs_diff(*told) >= 5 || (*told == 0) != npcs.is_empty() {
        *told = npcs.len();
        info!("co-op: sending the guests {} NPCs", npcs.len());
    }
    link.set_npcs(npcs);
}

/// A guest's game following the host's world (module notes): its actors' AI off, its
/// hits on them sent to the host, and its NPCs drawn as the host draws them.
#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn follow_npcs(
    mut commands: Commands,
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    (mut residents, mut stats, mut script): (Option<ResMut<Residents>>, ResMut<CombatStats>, Option<ResMut<nvf_play::script::ScriptState>>),
    mut actors: Query<(Entity, &RefVisual, Option<&mut WeaponHold>, Option<&mut Locomotion>, Has<Remote>, Has<Dying>), (With<crate::limbs::ActorRig>, Without<Puppet>)>,
    mut fighters: ResMut<Fighters>,
    actor_world: Option<Res<ActorWorld>>,
    (mut attacks, mut hits): (MessageWriter<PlayAttack>, MessageWriter<ActorHit>),
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    mut told: Local<usize>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let guest = coop.link.as_ref().is_some_and(|l| !l.is_host() && l.id().is_some()) && !at_main_menu(main_menu.as_deref());
    let mut game = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game);
    if !guest {
        // Left the lobby: this game's world is its own again.
        if let Some(was) = coop.ai_was_off.take() {
            stats.relay = None;
            if let Some(r) = residents.as_deref_mut() {
                r.remote = None;
            }
            if let Some(g) = game {
                g.toggles.actors_ai_off = was;
                g.coop_guest = false;
                g.coop_host_quest = None;
            }
            coop.host_quest = None;
            for (e, v, _, _, marked, _) in &mut actors {
                if marked {
                    commands.entity(e).remove::<Remote>();
                    fighters.set_puppet(v.0, None);
                }
            }
            coop.npcs.clear();
            info!("co-op: this game runs its own NPCs again");
        }
        return;
    }
    let Some(g) = game.as_deref_mut() else { return };
    if coop.ai_was_off.is_none() {
        coop.ai_was_off = Some(g.toggles.actors_ai_off);
        stats.relay = Some(Vec::new());
        info!("co-op: the host runs the world: this game's NPCs follow the host's");
    }
    // Kept off (a console `tai` would turn it back on).
    g.toggles.actors_ai_off = true;
    g.coop_guest = true;
    if g.coop_host_quest != coop.host_quest {
        g.coop_host_quest.clone_from(&coop.host_quest);
    }
    if let (Some(out), Some(link)) = (stats.relay.as_mut(), coop.link.as_ref()) {
        for h in out.drain(..) {
            info!("co-op: the host gets this game's hit on {} for {:.1}", h.target, h.dealt);
            link.send_hit(encode_hit(&h));
        }
    }
    coop.npcs.retain(|_, t| t.samples.back().is_some_and(|(at, _)| now - at < NPC_FORGET));
    let poses: HashMap<u32, RemotePose> = coop.npcs.iter().filter_map(|(&r, t)| t.pose(now - NPC_DELAY).map(|(feet, heading, moving)| (r, RemotePose { feet, heading, moving }))).collect();
    if poses.len().abs_diff(*told) >= 5 || (*told == 0) != poses.is_empty() {
        *told = poses.len();
        let mut some: Vec<String> = poses.keys().take(8).map(|r| format!("{r:08X}")).collect();
        some.sort();
        info!("co-op: following {} of the host's NPCs ({}...)", poses.len(), some.join(", "));
    }
    if let Some(r) = residents.as_deref_mut() {
        r.remote = Some(poses);
    }
    let mut bodies = HashSet::new();
    for (e, v, hold, lo, marked, dying) in &mut actors {
        let r = v.0;
        let Some(t) = coop.npcs.get_mut(&r) else {
            if marked {
                commands.entity(e).remove::<Remote>();
                fighters.set_puppet(r, None);
            }
            continue;
        };
        let Some(&(_, s)) = t.samples.back() else { continue };
        if !marked {
            commands.entity(e).insert(Remote);
        }
        if !dying {
            bodies.insert(r);
        }
        let drawn = s.has(npc_flag::DRAWN);
        if let Some(mut h) = hold {
            if h.drawn != drawn {
                h.drawn = drawn;
            }
        }
        if let Some(mut lo) = lo {
            lo.set_sneaking(s.has(npc_flag::SNEAKING));
        }
        if s.has(npc_flag::DEAD) {
            fighters.set_puppet(r, None);
            continue;
        }
        let attacked = t.played.is_some_and(|a| a != s.attacks);
        t.played = Some(s.attacks);
        if attacked {
            t.attacked_at = Some(now);
        }
        let weapon = (s.weapon != 0).then_some(FormId(s.weapon));
        let sights = s.has(npc_flag::SIGHTS);
        let fighting = drawn && (s.has(npc_flag::AIMING) || attacked || t.attacked_at.is_some_and(|at| now - at < AIM_AFTER_ATTACK));
        fighters.set_puppet(r, fighting.then_some(PuppetPose { pitch: s.pitch, weapon, sights, crouched: s.has(npc_flag::SNEAKING) }));
        if fighting && attacked {
            let melee = is_melee(actor_world.as_deref(), weapon);
            attacks.write(PlayAttack { actor: r, weapon, melee, sights: sights && !melee, kind: Move::Attack, power: None, swing: melee.then(SwingPick::default) });
        }
    }
    // Deaths: dead here too; a body drawn now falls as the host's did (`deaths`).
    for (&r, t) in &coop.npcs {
        let Some(&(_, s)) = t.samples.back() else { continue };
        if !s.has(npc_flag::DEAD) || g.is_dead(FormId(r)) {
            continue;
        }
        g.extra.entry(FormId(r)).or_default().dead = true;
        if bodies.contains(&r) {
            hits.write(ActorHit { form_id: r, died: true, knocked_out: false, location: -1, crippled: false, part_crippled: false, torso_crippled: false, by: None, share: 1.0, guard_broken: false, weapon: None });
        }
    }
}

/// Whether `reference` is one of the other players' puppets (never one of the world's).
fn is_puppet(reference: u32) -> bool {
    reference & 0xFFFF_FF00 == PUPPET_REF
}

/// The host's active quest as its guests show it ([`QuestView`]).
fn quest_view(st: Option<&nvf_play::pipboy::data::DataState>, g: &fnv_game::Game) -> QuestView {
    let Some(q) = nvf_play::pipboy::data::active_quest(st, g) else { return QuestView::default() };
    let Some(s) = g.quests.get(&q) else { return QuestView::default() };
    let objectives = s.objectives.iter().filter(|(_, o)| o.displayed || o.completed).map(|(i, o)| (*i, o.completed)).collect();
    QuestView { quest: q.0, completed: s.completed, failed: s.failed, objectives }
}

/// What a guest's game shows of a [`QuestView`] (`None`: the host has no quest).
fn host_quest(q: &QuestView) -> Option<HostQuest> {
    (q.quest != 0).then(|| HostQuest { quest: FormId(q.quest), objectives: q.objectives.iter().copied().collect(), completed: q.completed, failed: q.failed })
}

/// The quest messages a guest shows as the host's quest changes, as the host's own game
/// showed them (`nvf_play::script_events`): a new quest is added with its open
/// objectives; on the same quest, each objective shown or done, then its end.
fn host_quest_notices(data: &fnv_game::data::GameData, old: Option<&HostQuest>, new: Option<&HostQuest>) -> Vec<QuestNotice> {
    let Some(n) = new else { return Vec::new() };
    let Some(def) = data.quests.get(&n.quest) else { return Vec::new() };
    let name = def.name.clone().unwrap_or_default();
    let same = old.filter(|o| o.quest == n.quest);
    let mut out = Vec::new();
    if same.is_none() && !n.completed && !n.failed {
        out.push(QuestNotice::Added(name.clone()));
    }
    let mut objectives = Vec::new();
    for (i, &done) in &n.objectives {
        let before = same.and_then(|o| o.objectives.get(i).copied());
        let tell = match before {
            None => same.is_some() || !done,
            Some(was) => done && !was,
        };
        let text = def.objectives.get(i).and_then(|o| o.text.as_deref()).map(str::trim).unwrap_or("");
        if tell && !text.is_empty() {
            objectives.push(Objective { quest: n.quest.0, text: text.to_string(), completed: done });
        }
    }
    if !objectives.is_empty() {
        out.push(QuestNotice::Objectives(objectives));
    }
    if let Some(o) = same {
        if n.completed && !o.completed {
            out.push(QuestNotice::Completed(name.clone()));
        }
        if n.failed && !o.failed {
            out.push(QuestNotice::Failed(name));
        }
    }
    out
}

/// Unlock what another player unlocked: a terminal is hacked, a door or container's lock
/// open. Whether it was still locked here.
fn unlock(sim: &mut nvf_play::sim::SimState, g: &mut fnv_game::Game, r: FormId, terminal: bool) -> bool {
    use fnv_game::terminal::Access;
    if terminal {
        return g.terminals.access.insert(r, Access::Hacked) != Some(Access::Hacked);
    }
    let locked = sim.world.get(r).ok().and_then(|s| s.lock.as_ref()).is_some_and(|l| l.locked);
    let _ = sim.world.unlock(r);
    locked
}

/// A guest's hit as it goes to the host: the target, damage, weapon (0 for none),
/// location, critical, then 0 or 1 and the limb's condition, share, damage and kind.
fn encode_hit(h: &RelayedHit) -> Vec<u8> {
    let mut b = Vec::with_capacity(40);
    b.extend_from_slice(&h.target.0.to_le_bytes());
    b.extend_from_slice(&h.dealt.to_le_bytes());
    b.extend_from_slice(&h.weapon.map_or(0, |w| w.0).to_le_bytes());
    b.extend_from_slice(&h.location.to_le_bytes());
    b.push(h.critical as u8);
    match h.limb {
        Some(l) => {
            b.push(1);
            b.extend_from_slice(&l.condition.to_le_bytes());
            b.extend_from_slice(&l.health_percent.to_le_bytes());
            b.extend_from_slice(&l.damage.to_le_bytes());
            b.push(l.points as u8);
        }
        None => b.push(0),
    }
    b
}

/// [`encode_hit`] read back; None for one that is not a hit.
/// A hit on a guest's actor as it goes to that guest: who dealt it (0 for nobody), then
/// the hit as [`encode_hit`] has it.
fn encode_hurt(by: Option<FormId>, h: &RelayedHit) -> Vec<u8> {
    let mut b = by.map_or(0, |b| b.0).to_le_bytes().to_vec();
    b.extend(encode_hit(h));
    b
}

fn decode_hurt(b: &[u8]) -> Option<(Option<FormId>, RelayedHit)> {
    let by = u32::from_le_bytes(b.get(..4)?.try_into().ok()?);
    Some((Some(FormId(by)).filter(|b| b.0 != 0), decode_hit(&b[4..])?))
}

fn decode_hit(b: &[u8]) -> Option<RelayedHit> {
    let mut rd = Reader(b);
    let target = FormId(rd.u32()?);
    let dealt = f32::from_bits(rd.u32()?);
    let weapon = Some(rd.u32()?).filter(|&w| w != 0).map(FormId);
    let location = rd.u32()? as i32;
    let critical = rd.u8()? != 0;
    let limb = match rd.u8()? {
        0 => None,
        1 => {
            let condition = u16::from_le_bytes([rd.u8()?, rd.u8()?]);
            let health_percent = f64::from_bits(rd.u32()? as u64 | (rd.u32()? as u64) << 32);
            let damage = f64::from_bits(rd.u32()? as u64 | (rd.u32()? as u64) << 32);
            Some(fnv_game::combat::LimbHit { condition, health_percent, damage, points: rd.u8()? != 0 })
        }
        _ => return None,
    };
    (rd.0.is_empty() && dealt.is_finite()).then_some(RelayedHit { target, dealt, weapon, location, critical, limb })
}

/// Make a drawn puppet do what its player's newest state says (module notes). A new
/// attack with a gun gives the gun, for the shot to be heard.
#[allow(clippy::too_many_arguments)]
fn act(
    commands: &mut Commands,
    e: Entity,
    reference: u32,
    p: &mut Peer,
    s: PlayerState,
    now: f32,
    world: Option<&ActorWorld>,
    actors: &mut Query<(&mut Transform, Option<&mut Locomotion>), Without<crate::furniture::OnFurniture>>,
    holds: &mut Query<&mut WeaponHold>,
    dressed: &Query<(Option<&PuppetEquipment>, Has<Puppet>)>,
    fighters: &mut Fighters,
    attacks: &mut MessageWriter<PlayAttack>,
) -> Option<FormId> {
    let weapon = (s.weapon != 0).then_some(FormId(s.weapon));
    let gear = Equipment { worn: s.worn().iter().map(|&w| FormId(w)).collect(), weapon, weapon_mods: 0 };
    if let Ok((worn, marked)) = dressed.get(e) {
        if !marked {
            commands.entity(e).insert(Puppet);
        }
        if worn.is_none_or(|w| w.0 != gear) {
            commands.entity(e).insert(PuppetEquipment(gear));
        }
    }
    let drawn = s.has(flag::DRAWN);
    let sneaking = s.has(flag::SNEAKING);
    if let Ok(mut hold) = holds.get_mut(e) {
        if hold.drawn != drawn {
            hold.drawn = drawn;
        }
    }
    if let Ok((_, Some(mut lo))) = actors.get_mut(e) {
        lo.set_sneaking(sneaking);
    }
    // New attacks and reloads: one each for whatever the counters went up by.
    let (attacked, reloaded) = match p.played {
        Some((a, r)) => (s.attacks != a, s.reloads != r),
        None => (false, false),
    };
    p.played = Some((s.attacks, s.reloads));
    if attacked {
        p.attacked_at = Some(now);
    }
    let fighting = drawn && (s.has(flag::AIMING) || attacked || p.attacked_at.is_some_and(|t| now - t < AIM_AFTER_ATTACK));
    let sights = s.has(flag::SIGHTS);
    // The attack clips play only on an actor with an aim (attack_anims::start).
    fighters.set_puppet(reference, fighting.then_some(PuppetPose { pitch: s.pitch, weapon, sights, crouched: sneaking }));
    if !fighting {
        return None;
    }
    let melee = is_melee(world, weapon);
    if attacked {
        let power = match s.attack_kind {
            attack::POWER => Some(Power::Normal),
            attack::POWER_FORWARD => Some(Power::Forward),
            attack::POWER_BACK => Some(Power::Back),
            attack::POWER_LEFT => Some(Power::Left),
            attack::POWER_RIGHT => Some(Power::Right),
            _ => None,
        };
        let swing = (melee && power.is_none()).then(SwingPick::default);
        attacks.write(PlayAttack { actor: reference, weapon, melee, sights: sights && !melee, kind: Move::Attack, power: power.filter(|_| melee), swing });
    }
    if reloaded && !melee {
        attacks.write(PlayAttack { actor: reference, weapon, melee: false, sights: false, kind: Move::Reload, power: None, swing: None });
    }
    weapon.filter(|_| attacked && !melee)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_hit_round_trips_and_fits_a_packet() {
        let limb = fnv_game::combat::LimbHit { condition: 27, health_percent: 15.5, damage: 33.25, points: true };
        for h in [
            RelayedHit { target: FormId(0x0010_4C3A), dealt: 12.5, weapon: Some(FormId(0x0E3778)), location: 2, critical: true, limb: Some(limb) },
            RelayedHit { target: FormId(7), dealt: 0.0, weapon: None, location: -1, critical: false, limb: None },
        ] {
            let b = encode_hit(&h);
            assert!(b.len() <= net::HIT_MAX);
            let back = decode_hit(&b).unwrap();
            assert_eq!(format!("{back:?}"), format!("{h:?}"));
            // And as it goes to a guest, with who dealt it.
            for by in [Some(FormId(0x0015_A797)), None] {
                let b = encode_hurt(by, &h);
                assert!(b.len() <= net::HIT_MAX);
                let (who, back) = decode_hurt(&b).unwrap();
                assert_eq!((who, format!("{back:?}")), (by, format!("{h:?}")));
            }
        }
        assert!(decode_hit(&[1, 2, 3]).is_none());
        assert!(decode_hurt(&[1, 2]).is_none());
    }

    #[test]
    fn a_look_round_trips_and_fits_a_packet() {
        let l = Look {
            race: FormId(0x0000_19),
            female: true,
            hair: Some(FormId(0x0004_2A1B)),
            hair_color: Some([120, 80, 40]),
            eyes: None,
            head_parts: vec![FormId(0x1234)],
            fggs: (0..50).map(|i| i as f32 * 0.1).collect(),
            fgga: vec![-0.5; 30],
            fgts: (0..50).map(|i| -(i as f32)).collect(),
        };
        let b = encode_look(Some(&l));
        assert!(b.len() <= net::LOOK_MAX, "{} bytes", b.len());
        assert_eq!(decode_look(&b), Some(Some(l)));
        assert_eq!(decode_look(&encode_look(None)), Some(None));
        assert_eq!(decode_look(&b[..b.len() - 1]), None);
        assert_eq!(decode_look(&[9]), None);
    }
}
