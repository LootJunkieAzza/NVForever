//! Co-op prototype (private fork, not upstream). A lobby is opened from the pause menu
//! (Lobby, Create Lobby) and joined from the main menu (Join Lobby, the code, a save);
//! the screens are `nvf_play::lobby`. `--host <port>` and `--join <code or address>`
//! do the same from the command line.
//!
//! **Lobby codes.** There is no server: the host's game is the lobby. Its code is the
//! host's own addresses and port (`nvf_coop::code`); creating it asks the host's router
//! to let friends in (`nvf_coop::reach`). A load restarts the game (main.rs
//! `restart_args`): the lobby goes with it, in [`HOST_ENV`] and [`CODE_ENV`] for a host
//! and [`JOIN_ENV`] for a guest, which is how a guest's chosen save comes into the host's
//! game.
//!
//! Each game plays its own world. Every game sends its player's place, feet and heading
//! over `nvf_coop::net` (the host passes them between guests), and draws the other
//! players as NPCs the network moves: engine-only puppets with a reference no load order
//! has, so no script, AI package or physics body ever sees or fights them (the same
//! drawing as the `--npc-travel` travellers, `nav::show_actor`).
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
use nvf_coop::net::{self, attack, flag, Event, Link, Place, PlayerState, Session};
use nvf_coop::reach::{self, Reach};
use nvf_play::loading::{Loading, TravelRequest};
use nvf_play::lobby::{Hosting, Joining, LobbyRequest, LobbyState};
use nvf_play::player::Player;
use nvf_play::{Location, PlayState, RefVisual, Travel};
use std::collections::{BTreeMap, VecDeque};
use std::net::{SocketAddr, ToSocketAddrs};
use std::sync::mpsc::{self, Receiver};
use std::sync::Mutex;
use std::time::Duration;

/// The port `--join` uses when the address names none.
pub const DEFAULT_PORT: u16 = 47_960;
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
        };
        if let Some(port) = c.host.or_else(|| env(HOST_ENV).and_then(|p| p.trim().parse().ok())) {
            coop.start_hosting(port);
        } else if let Some(join) = c.join.clone().or_else(|| env(JOIN_ENV)) {
            coop.start_joining(&join);
        }
        app.add_plugins(nvf_play::lobby::LobbyPlugin)
            .insert_resource(coop)
            .add_systems(Update, (lobby, send, take_events, leash).chain().after(nvf_play::PlaySet::Physics))
            .add_systems(Update, draw.after(nvf_play::PlaySet::Sync).before(crate::locomotion::walk_test));
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
    let candidates = match Lobby::decode(at) {
        Some(l) => l.candidates(),
        None if at.contains('.') || at.contains(':') => resolve(at).into_iter().collect(),
        None => Vec::new(),
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
    match Session::join(&candidates, name, token) {
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
        if let Some(code) = std::env::var(CODE_ENV).ok().filter(|c| Lobby::decode(c).is_some_and(|l| l.port == port)) {
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
        info!("co-op: lobby code {code} (this network {}, internet {}, port {}; router opened it: {})", r.lobby.local, r.lobby.public, r.lobby.port, r.opened);
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
}

impl Peer {
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
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let Some(link) = coop.link.as_ref() else { return };
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
                let reference = PUPPET_REF + id as u32;
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
                fighters.set_puppet(PUPPET_REF + id as u32, None);
                if let Some(w) = actor_world.as_deref() {
                    if let Ok(mut looks) = w.looks.lock() {
                        looks.remove(&(PUPPET_REF + id as u32));
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
    (mut fighters, mut attacks, main_menu): (ResMut<Fighters>, MessageWriter<PlayAttack>, Option<Res<nvf_play::main_menu::MainMenu>>),
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
        let reference = PUPPET_REF + id as u32;
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
        act(&mut commands, e, reference, p, newest, now, actor_world.as_deref(), &mut actors, &mut holds, &dressed, &mut fighters, &mut attacks);
    }
}

/// Make a drawn puppet do what its player's newest state says (module notes).
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
) {
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
        return;
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
