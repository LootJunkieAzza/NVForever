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
//! **A guest's own game.** As a guest is welcomed, its own game is kept as it is then
//! (`nvf_play::save::KeepSolo`, `SoloBase`), and nothing of the host's world reaches it
//! (the leash, the host's NPCs and clock) until it is: each save it makes in the host's
//! world is its own game where it left it, with the character as it is now. So a player
//! at Big MT who helps a friend in Primm loads back in at Big MT, with the XP, items and
//! perks of the visit.
//!
//! **What a guest does.** A guest leaves the story to the host, as in Dying Light's co-op
//! (`fnv_game::Game::coop_guest`): no script of its game starts a conversation with it,
//! but it talks to any NPC it goes up to (a casino's cashier changes its own caps for
//! chips), in its own game, and plays at the casino tables. Its quest list,
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
//! worldspace, or outdoors farther than [`LEASH`] away, the guest travels to the host's
//! own spot, through the loading screen like any door, and the host's body lets it out
//! as any actor's does (`nvf_play::blocking`): a spot behind the host could be inside a
//! wall.
//!
//! **The host's clock.** The host sends its game clock ([`net::ClockView`]); a guest sets
//! its own to it when they differ, so the time of day, and what NPCs do by it, is the
//! same in every game.
//!
//! **The host's world on a guest.** An NPC the host draws where the guest is, the guest
//! draws there too: one the host's story enabled (the Powder Gangers coming to
//! Goodsprings) is enabled in the guest's world, and one it moved (Trudy to her saloon)
//! is moved there ([`follow_npcs`]). An actor the host's scripts made has no reference
//! the guest's world knows, and is not drawn there. The host's world wins whatever the
//! guest's save did: an NPC dead in the guest's game but alive in the host's stands up
//! again ([`place_like_host`]), and an actor the guest's game draws near the host that the
//! host's world does not have (dead or disabled there, not there yet) is disabled for as
//! long as the guest is in the host's world ([`not_in_hosts_world`]). What the host's
//! scripts enabled or disabled that is not an NPC it draws (Sunny's bottles, the markers
//! that are other references' enable parents) goes as its world's presence: every placed
//! reference whose own enabled flag differs from its plugin's ([`presence_of`],
//! `net::Msg::Presence`); a guest takes those flags, and once it has the whole list puts
//! back its plugin's flag on what only its own game changed ([`follow_presence`]). So a
//! guest whose save has done everything joins a host who is just starting and sees the
//! host's Goodsprings. None of it reaches the guest's own saves (its own game is kept, below).
//!
//! **What the host's NPCs say.** The lines the host's NPCs are saying (a scene between
//! two of them, a scripted Say or SayTo, a hello; `Game::speech`), and the NPC's lines
//! of the host's own conversation ([`conversation_line`]), go to the guests ([`net::LineView`]),
//! whose games say them as the host's does, voice and subtitles, timed by the host and
//! without their scripts (`Game::set_remote_speech`); a line of a guest's own by one of
//! the host's NPCs stops.
//!
//! **What they do.** Each state carries the weapon in hand, whether it is out, aimed or
//! down the sights, sneaking, in the air, the flashlight, what is worn, and counters of
//! the attacks and reloads begun: the puppet draws and holsters, crouches, aims (as a
//! fighter does, `npc_combat::Fighters::set_puppet`) and plays one attack or reload for
//! each the counter went up, so a lost packet loses none. A shot is heard where it is
//! fired (the gun's sound, near or distant, as an NPC's: [`shot_sound`]), and so is a
//! shot of an NPC a guest follows. A puppet's flashlight is lit on it ([`peer_lights`]).
//! A puppet's head is left to its animation: it does not turn to look at the player.
//!
//! **How they look.** Each game sends its player's look from the character creator
//! (race, sex, hair, eyes, face; [`encode_look`]) every `net::LOOK_EVERY`; the others draw
//! that player from the player's own record with it (`ActorWorld::looks`), as this game
//! draws its own body. Until a look comes, a player is drawn as [`Config::base`].
//!
//! **Down, not dead.** A player whose health runs out in co-op is down ([`downed`]): the
//! body drops as a ragdoll (here and on the others' screens) and the view stays with it,
//! without the death camera's slow motion or fade (`death_cam::DeathCam::downed_in_coop`).
//! Another player who comes within [`REVIVE_REACH`] and presses Activate revives them
//! with a Stimpak of their own (one is used up): the downed player stands up where the
//! body lies, healed (`Game::resurrect`). After [`RESPAWN_AFTER`] a downed player can
//! press Activate to stand up again beside another player (the nearest one up, else the
//! host) instead.
//!
//! **Loading a save while hosting.** A load hands over to a new run of the game, which
//! starts while this one still shows the loading screen (`crate::handoff`). The host
//! lets go of its port then, without a goodbye ([`lobby`]), so the new run opens the lobby
//! on it at once and the guests come back to it; if the new run fails, this one opens
//! the lobby again.
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
use nvf_coop::code::{Host, Lobby};
use nvf_coop::steam;
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
use nvf_play::coop_tables::Tables;
use nvf_play::player::Player;
use nvf_play::sfx::{PlaySound, Sfx};
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
/// After a leash trip, the next waits this long (the trip's loading screen comes first).
const LEASH_EVERY: f32 = 5.0;
/// The puppets' references: plugin index FE, which no FNV load order uses.
const PUPPET_REF: u32 = 0xFE00_C000;
/// The record a player with a look is drawn from: the player's own, as this game draws
/// its own body (`player_body.rs`).
const PLAYER_BASE: u32 = FormId::PLAYER_BASE.0;
/// A guest takes the host's NPCs as being where it is only this long after the host
/// came there (seconds): before, they may be the place it left.
const HOST_SETTLE: f32 = 1.0;
/// Where a shot is heard from, above the shooter's feet (about the gun's height).
const GUN_HEIGHT: f32 = 100.0;
/// A guest takes the host's clock when they differ by more than this (game hours: three
/// game minutes, six real seconds at TimeScale 30).
const CLOCK_SLACK: f32 = 0.05;
/// A downed player can respawn after this long (seconds).
const RESPAWN_AFTER: f32 = 60.0;
/// How near a downed player another must be to revive them (game units, about 2 m).
const REVIVE_REACH: f32 = 150.0;
/// What reviving uses up: a Stimpak.
const STIMPAK: FormId = FormId(0x0001_5169);
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
/// A guest looking for its host gives up after this long without an answer (seconds; a
/// Steam route can take a few to set up).
const FIND_FOR: f32 = 20.0;

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
/// How often every body of the other players is checked for strays (seconds, [`draw`]).
const SWEEP_EVERY: f32 = 1.0;
/// A guest's own actor this near the host (game units, across the floor), which the
/// host's NPCs have not included for [`HIDE_AFTER`] seconds (and the host has been in
/// the place that long), is not in the host's world there ([`not_in_hosts_world`]).
/// Well inside the cells the host draws around it.
const HIDE_REACH: f32 = 3000.0;
const HIDE_AFTER: f32 = 3.0;

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
            quest_untold: None,
            host_place: None,
            downed_at: None,
            told_respawn: false,
            revive_offer: None,
            strays: Vec::new(),
            swept_at: 0.0,
            unheard: HashMap::new(),
            hidden: HashSet::new(),
            presence: HostPresence::default(),
            presence_was: HashMap::new(),
            presence_made: None,
            host_lines: None,
            handed_off: false,
            progress_in: Vec::new(),
        };
        if let Some(port) = c.host.or_else(|| env(HOST_ENV).and_then(|p| p.trim().parse().ok())) {
            coop.start_hosting(port);
        } else if let Some(join) = c.join.clone().or_else(|| env(JOIN_ENV)) {
            coop.start_joining(&join);
        }
        app.add_plugins((nvf_play::lobby::LobbyPlugin, nvf_play::coop_progress::CoopProgressPlugin))
            .insert_resource(coop)
            .add_systems(Update, (lobby, send, take_events, tables, social_mates, share_npcs, follow_npcs, leash, downed, progress, duel_falls).chain().after(nvf_play::PlaySet::Physics))
            .add_message::<PuppetShot>()
            .add_systems(Update, (stand_ins, draw, peer_lights, pipboy_share).chain().after(nvf_play::PlaySet::Sync).before(crate::locomotion::walk_test));
    }
}

/// The host's session on `port`. A load restarts the game: the run before may hold the
/// port for a moment.
fn host_session(port: u16) -> Option<Link> {
    for attempt in 0..40 {
        match Session::host(port) {
            Ok(s) => {
                info!("co-op: hosting on UDP port {port}");
                return Some(Link::new(with_steam(s)));
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
    if steam_code(at) && steam::steam().is_none() {
        warn!("co-op: {at} is a Steam code and Steam is not running: only a host on this network can answer");
    }
    match Session::join(&candidates, key, name, token) {
        Ok(s) => {
            info!("co-op: joining {candidates:?} as {name}");
            Ok(Link::new(with_steam(s)))
        }
        Err(e) => Err(format!("Cannot open a connection: {e}")),
    }
}

/// A session that also talks over Steam when this game has it.
fn with_steam(s: Session) -> Session {
    match steam::steam() {
        Some(st) => s.over_steam(st),
        None => s,
    }
}

/// Whether `at` is a code for a host on Steam.
fn steam_code(at: &str) -> bool {
    Lobby::decode(at).is_some_and(|l| matches!(l.host, Host::Steam(_)))
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
    /// A guest's: the host's quest as its messages last told it, while messages for a
    /// newer one wait for this game's scripts (a guest joins while its game still loads,
    /// and the host sends its quest again only when it changes).
    quest_untold: Option<Option<HostQuest>>,
    /// A guest's: the host's place, and since when it has been there (seconds of `Time`).
    host_place: Option<(Place, f32)>,
    /// This player is down: since when (seconds of `Time`), and whether it was told it
    /// can respawn.
    downed_at: Option<f32>,
    told_respawn: bool,
    /// The downed player this one was last told it can revive.
    revive_offer: Option<u8>,
    /// References whose every body in the world is taken away on the next frame ([`draw`]:
    /// players who left), and when the bodies were last checked for strays.
    strays: Vec<u32>,
    swept_at: f32,
    /// A guest's: its own actors near the host that the host's world does not have, since
    /// when each has not been heard of, and those it disabled ([`not_in_hosts_world`]).
    unheard: HashMap<u32, f32>,
    hidden: HashSet<FormId>,
    /// A guest's: the host's world's presence as heard ([`follow_presence`]), and the
    /// enabled flags it changed in this world, as they were (put back on leaving).
    presence: HostPresence,
    presence_was: HashMap<FormId, bool>,
    /// A host's: its world's presence serial when the list was last made, and when.
    presence_made: Option<(u64, f32)>,
    /// A guest's: the lines the host's NPCs are saying, and when they came.
    host_lines: Option<(f32, Vec<net::LineView>)>,
    /// A host that let go of its port for a load's new run ([`lobby`]).
    handed_off: bool,
    /// Shared progression heard, for [`progress`].
    progress_in: Vec<(u8, net::Progress)>,
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
        if let Some(st) = steam::steam() {
            // Steam's relays let anyone in: no router to ask. Friends on this network still
            // come by broadcast, to this port.
            info!("co-op: hosting over Steam too (relays: {})", st.relays());
            let local = reach::local_ip().unwrap_or(std::net::Ipv4Addr::LOCALHOST);
            let _ = tx.send(Reach { lobby: Lobby::steam(st.account(), port), local, opened: true, note: None });
        } else {
            std::thread::Builder::new()
                .name("coop-reach".into())
                .spawn(move || {
                    let _ = tx.send(reach::open(port));
                })
                .ok();
        }
        self.reaching = Some(Mutex::new(rx));
    }

    fn stop_hosting(&mut self, commands: &mut Commands, fighters: &mut Fighters) {
        // Dropping the link says goodbye to the guests.
        self.link = None;
        if self.opened && !self.code.as_deref().is_some_and(steam_code) {
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
            self.strays.extend(p.references(id));
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
    tables: Option<Res<Tables>>,
    handoff: Option<Res<crate::handoff::Handoff>>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let mut say = |n: String| {
        info!("co-op: {n}");
        if let Some(c) = corner.as_mut() {
            c.push(n.into());
        }
    };
    // A load's new run takes over (module notes): it hosts on this port, so this run lets
    // go of it now, without a goodbye (the guests wait for the host to come back), rather
    // than holding it until the new run is up, which waits on it. If the new run failed,
    // this one hosts again.
    let leaving = handoff.as_deref().is_some_and(|h| h.leaving());
    if leaving && coop.is_host() {
        if let Some(link) = coop.link.take() {
            info!("co-op: a new run takes over: port {} handed over to it", coop.port);
            link.abandon();
            coop.handed_off = true;
        }
    } else if !leaving && coop.handed_off {
        coop.handed_off = false;
        info!("co-op: the new run did not take over: hosting again");
        coop.start_hosting(coop.port);
    }
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
        info!("co-op: lobby code {code} (host {}, this network {}, port {}; open to the internet: {})", r.lobby.host, r.local, r.lobby.port, r.opened);
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
            let why = if coop.join_code.as_deref().is_some_and(steam_code) && steam::steam().is_none() {
                "This code goes through Steam: start Steam, then join again."
            } else {
                "No answer from the host: check the code, and that their lobby is open."
            };
            coop.joining = Joining::Failed(why.into());
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
            // A player at a minigame table says so (the others' tables, module notes).
            let n = match tables.as_deref().and_then(|t| t.at_table(id)) {
                Some(g) => format!("{n} (at {})", nvf_play::coop_tables::game::word(g)),
                None => n,
            };
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
    /// Down, as last seen ([`downed`]); drawn anew (standing) on the next frame.
    down: bool,
    redraw: bool,
    /// Every body of this player in the world is taken away before it is drawn again
    /// ([`Peer::undraw`]).
    purge: bool,
    /// Lost a duel to this player: its puppet lies since then (seconds of `Time`;
    /// [`duel_falls`]).
    fell_at: Option<f32>,
}

impl Peer {
    /// The reference its puppet is drawn with: its actor's on the host, else
    /// [`PUPPET_REF`] + its id.
    fn reference(&self, id: u8) -> u32 {
        self.actor.map_or(PUPPET_REF + id as u32, |r| r.0)
    }

    /// The references its bodies can have: its puppet's, and its actor's on the host.
    fn references(&self, id: u8) -> impl Iterator<Item = u32> {
        std::iter::once(PUPPET_REF + id as u32).chain(self.actor.map(|r| r.0))
    }

    /// Take its body away, to be drawn anew. Not only the root it knows: a body spawned
    /// in the frames before its root is found (or one left lying when it got up) would
    /// stay behind, frozen where it was; [`draw`] takes every body of its references away
    /// first ([`Peer::purge`]).
    fn undraw(&mut self) {
        self.root = None;
        self.spawned = false;
        self.drawn_in = None;
        self.purge = true;
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

/// The minigame tables between this game and the link: who is on, who is near, this
/// player's table view out and its actions for the others' tables (nvf-play
/// `coop_tables`).
fn tables(coop: Res<Coop>, player: Option<Res<Player>>, loc: Option<Res<Location>>, tables: Option<ResMut<Tables>>) {
    let Some(mut tables) = tables else { return };
    let Some(link) = coop.link.as_ref() else {
        if tables.on {
            *tables = Default::default();
        }
        return;
    };
    tables.on = true;
    tables.me = link.id();
    tables.names = coop.peers.iter().map(|(&id, p)| (id, p.name.clone())).collect();
    let here = loc.as_deref().map(place_of);
    tables.near = coop
        .peers
        .iter()
        .filter(|(_, p)| !p.lost)
        .filter_map(|(&id, p)| {
            let s = p.samples.back()?.1;
            let feet = player.as_deref()?.feet;
            let d2 = (0..3).map(|k| (s.feet[k] - feet[k]).powi(2)).sum::<f32>();
            (Some(s.place) == here && d2 < nvf_play::coop_tables::WATCH_RANGE.powi(2)).then_some(id)
        })
        .collect();
    link.set_table(tables.out.clone().map(|(table, game, blob)| net::TableView { table, game, blob }));
    for (table, game, blob) in std::mem::take(&mut tables.actions_out) {
        link.send_table_action(table, game, blob);
    }
}

/// The other players for the interaction wheel (`nvf_play::coop_social`): who they are,
/// where, and whether this player is indoors.
fn social_mates(coop: Res<Coop>, loc: Option<Res<Location>>, mut mates: ResMut<nvf_play::coop_social::SocialMates>) {
    use nvf_play::coop_social::{Mate, SocialMates};
    let want = match (coop.link.as_ref(), loc.as_deref()) {
        (Some(link), Some(loc)) => {
            let here = place_of(loc);
            SocialMates {
                me: link.id(),
                interior: loc.interior.is_some(),
                mates: coop
                    .peers
                    .iter()
                    .filter_map(|(&id, p)| {
                        let &(_, s) = p.samples.back()?;
                        Some(Mate { id, name: label(id, &p.name), reference: p.reference(id), here: s.place == here, feet: s.feet, yaw: s.yaw, down: s.has(flag::DOWN), lost: p.lost })
                    })
                    .collect(),
            }
        }
        _ => SocialMates::default(),
    };
    if *mates != want {
        *mates = want;
    }
}

/// How long the loser of a duel lies on the winner's screen before its puppet stands
/// again (seconds) [design: its own game's knockdown and get-up take about as long].
const DUEL_FALL: f32 = 3.5;

/// The loser of a duel against this player: its puppet falls as a downed player's does
/// ([`downed`]), and stands again after [`DUEL_FALL`].
#[allow(clippy::too_many_arguments)]
fn duel_falls(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    mut social: ResMut<nvf_play::coop_social::Social>,
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    mut phys: Option<ResMut<nvf_play::phys::PhysState>>,
    mut ragdolls: ResMut<crate::ragdoll::Ragdolls>,
    mut hits: MessageWriter<ActorHit>,
) {
    let now = time.elapsed_secs();
    let mut falls: Vec<u8> = Vec::new();
    social.falls.retain(|&f| {
        if f == u8::MAX {
            return true;
        }
        falls.push(f);
        false
    });
    let Some(g) = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game) else { return };
    for id in falls {
        let Some(p) = coop.peers.get_mut(&id).filter(|p| !p.down) else { continue };
        let r = p.reference(id);
        p.fell_at = Some(now);
        g.extra.entry(FormId(r)).or_default().dead = true;
        hits.write(ActorHit { form_id: r, died: true, knocked_out: false, location: -1, crippled: false, part_crippled: false, torso_crippled: false, by: None, share: 1.0, guard_broken: false, weapon: None });
        info!("co-op: {} lost the duel: the puppet falls", label(id, &p.name));
    }
    for (&id, p) in coop.peers.iter_mut() {
        if p.fell_at.is_none_or(|t| now - t < DUEL_FALL) {
            continue;
        }
        p.fell_at = None;
        if p.down {
            continue;
        }
        let r = p.reference(id);
        g.extra.entry(FormId(r)).or_default().dead = false;
        if let Some(ph) = phys.as_deref_mut() {
            ph.world.remove_ragdoll(r);
        }
        ragdolls.rest.remove(&r);
        p.redraw = true;
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
    flashlight: Option<Res<'w, nvf_play::flashlight::Flashlight>>,
    /// The Pip-Boy up in full or for a glance (`crate::pipboy_check`).
    pipboy: (Option<Res<'w, nvf_play::pipboy::PipBoy>>, Option<Res<'w, nvf_play::pipboy::glance::Glance>>),
}

/// The others' Pip-Boys for this game: each puppet's check (`crate::pipboy_check`), where
/// the others are for the glance's map (`nvf_play::pipboy::glance::CoopMates`), and
/// whether the full Pip-Boy may leave the world running (`nvf_play::menu::LiveMenu`: in a
/// session).
fn pipboy_share(coop: Res<Coop>, mut checks: ResMut<crate::pipboy_check::PuppetChecks>, mut mates: ResMut<nvf_play::pipboy::glance::CoopMates>, mut live: ResMut<nvf_play::menu::LiveMenu>) {
    use nvf_play::pipboy::glance::{Check, Mate};
    let in_session = coop.link.is_some();
    if live.allowed != in_session {
        live.allowed = in_session;
    }
    let mut want = std::collections::HashMap::new();
    let mut seen = Vec::new();
    for (&id, p) in &coop.peers {
        let Some(&(_, newest)) = p.samples.back() else { continue };
        let check = Check::from_code(newest.pipboy);
        if check != Check::None && !p.lost {
            want.insert(p.reference(id), check);
        }
        if !p.lost {
            let (interior, world) = match newest.place {
                Place::Interior(c) => (Some(c), 0),
                Place::Exterior(w) => (None, w),
            };
            seen.push(Mate { name: label(id, &p.name), interior, world, feet: newest.feet });
        }
    }
    if checks.0 != want {
        checks.0 = want;
    }
    if mates.0 != seen {
        mates.0 = seen;
    }
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
    flags |= if doing.flashlight.as_ref().is_some_and(|f| f.on) { flag::LIGHT } else { 0 };
    s.flags = flags;
    if let (Some(pip), Some(glance)) = (doing.pipboy.0.as_deref(), doing.pipboy.1.as_deref()) {
        s.pipboy = nvf_play::pipboy::glance::Check::of(pip, glance).code();
    }
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
    (mut opened, real, mut tables, mut anims_out, mut anims_in): (MessageReader<LockOpened>, Res<Time<Real>>, Option<ResMut<Tables>>, MessageReader<crate::consume_anims::SendAnim>, MessageWriter<crate::consume_anims::PeerAnim>),
    solo: Option<Res<nvf_play::save::SoloBase>>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let Some(link) = coop.link.as_ref() else {
        opened.clear();
        anims_out.clear();
        return;
    };
    // This player's item animations go to everyone (consume_anims.rs).
    for a in anims_out.read() {
        link.send_anim(a.0.clone());
    }
    // What this player unlocked goes to everyone (module notes).
    for o in opened.read() {
        info!("co-op: this player {} {:#010X}: everyone's is unlocked", o.how.word(), o.reference);
        link.send_unlock(o.reference, o.how == OpenedBy::Hacked);
    }
    let mut rt = script.as_deref_mut().and_then(|s| s.rt.as_mut());
    let is_host = link.is_host();
    let mut notes = Vec::new();
    let mut refused = false;
    // The host's quest messages, once this game's scripts are there to name them (a quest
    // heard while the game still loaded is told then).
    if let (Some(told), Some(rt)) = (coop.quest_untold.as_ref(), rt.as_deref()) {
        for n in host_quest_notices(rt.game.data, told.as_ref(), coop.host_quest.as_ref()) {
            info!("co-op: the host's quest: {}", nvf_play::quest_messages::describe(&n));
            if let Some(l) = quest_lines.as_deref_mut() {
                l.push(n);
            }
        }
        coop.quest_untold = None;
    }
    for e in link.events() {
        match e {
            Event::Welcomed(id) => {
                // Keep this game as the guest's own before the host's world reaches it.
                if !is_host {
                    commands.insert_resource(nvf_play::save::KeepSolo);
                }
                notes.push(format!("Joined the host's game as player {}", id + 1));
            }
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
                    if coop.host_place.is_none_or(|(p, _)| p != state.place) {
                        coop.host_place = Some((state.place, now));
                    }
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
                p.undraw();
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
                    coop.strays.extend(p.references(id));
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
                // Told from the quest last told, even when messages for a later one wait.
                let told = coop.quest_untold.take().unwrap_or_else(|| coop.host_quest.clone());
                coop.host_quest = host_quest(&q);
                coop.quest_untold = Some(told);
            }
            Event::Revived { by } => {
                if let Some(rt) = rt.as_deref_mut().filter(|rt| rt.game.is_dead(FormId::PLAYER_REF)) {
                    rt.game.resurrect(FormId::PLAYER_REF, false);
                    coop.downed_at = None;
                    notes.push(format!("{} revived you", label(by, &coop.peers.get(&by).map_or(String::new(), |p| p.name.clone()))));
                }
            }
            Event::Lines(lines) => coop.host_lines = Some((now, lines)),
            Event::Presence { serial, page, pages, refs } => coop.presence.hear(serial, page, pages, refs),
            Event::Clock(c) => {
                // Not before this game is kept as the guest's own.
                if let Some(sim) = sim.as_deref_mut().filter(|_| solo.is_some()) {
                    take_clock(&mut sim.world, c);
                }
            }
            Event::Unlocked { id, reference, terminal } => {
                if let (Some(rt), Some(sim)) = (rt.as_deref_mut(), sim.as_deref_mut()) {
                    if unlock(sim, &mut rt.game, FormId(reference), terminal) {
                        info!("co-op: {} unlocked {reference:#010X}{}", label(id, &coop.peers.get(&id).map_or(String::new(), |p| p.name.clone())), if terminal { " (a terminal)" } else { "" });
                    }
                }
            }
            Event::Table { id, table, game, blob } => {
                let Some(t) = tables.as_deref_mut() else { continue };
                if game & net::TABLE_ACTION != 0 {
                    t.actions_in.push((id, table, game & !net::TABLE_ACTION, blob));
                } else if game == net::TABLE_LEFT {
                    t.seen.remove(&id);
                } else {
                    let new = t.seen.get(&id).is_none_or(|v| v.1 != game);
                    t.seen.insert(id, (table, game, blob, real.elapsed_secs()));
                    if new {
                        info!("co-op: {} is at {} ({table:#010X})", label(id, &coop.peers.get(&id).map_or(String::new(), |p| p.name.clone())), nvf_play::coop_tables::game::word(game));
                    }
                }
            }
            Event::Anim { id, cue } => {
                let reference = coop.peers.get(&id).map_or(PUPPET_REF + id as u32, |p| p.reference(id));
                anims_in.write(crate::consume_anims::PeerAnim { reference, cue });
            }
            // For `progress` (crate::coop_progress).
            Event::Progress { id, what } => coop.progress_in.push((id, what)),
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
    solo: Option<Res<nvf_play::save::SoloBase>>,
) {
    // A downed guest stays down where it fell (it respawns by the host, `downed`). Not
    // moved before its own game is kept (module notes).
    if !coop.is_guest() || coop.host_lost || at_main_menu(main_menu.as_deref()) || coop.downed_at.is_some() || solo.is_none() {
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
    // On the host's own spot: the host's body lets the guest out (module notes).
    let feet = host.feet;
    let (world, interior) = match host.place {
        Place::Interior(c) => (None, Some(c)),
        Place::Exterior(w) => (Some(w), None),
    };
    info!("co-op: leash: to the host at {:?} {feet:?}", host.place);
    travel.write(TravelRequest(Travel { world, interior, feet, heading_deg: host.yaw.to_degrees() }));
    coop.last_leash = Some(now);
}

/// Draw each other player where the link last put it, in this game's space only.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
    (sfx, listener, mut sounds, lying): (Option<Res<Sfx>>, Option<Res<Player>>, MessageWriter<PlaySound>, Query<(), Or<(With<Dying>, With<crate::ragdoll::Corpse>)>>),
) {
    let (Ok(world_root), Some(loc)) = (root.single(), loc) else { return };
    if at_main_menu(main_menu.as_deref()) {
        return;
    }
    let now = time.elapsed_secs();
    // Bodies to take away: of players who left, of players drawn anew, and (once a
    // second) any body of a player that is not its puppet: a stray left behind.
    let sweep = now - coop.swept_at >= SWEEP_EVERY;
    if sweep {
        coop.swept_at = now;
    }
    let mut keep: HashMap<u32, Option<Entity>> = coop.strays.drain(..).map(|r| (r, None)).collect();
    for (&id, p) in coop.peers.iter_mut() {
        if std::mem::take(&mut p.purge) {
            keep.extend(p.references(id).map(|r| (r, None)));
        } else if sweep && p.root.is_some() {
            keep.extend(p.references(id).map(|r| (r, p.root)));
        }
    }
    despawn_bodies(&mut commands, &roots, world_root, &keep);
    let here = place_of(&loc);
    let dt = time.delta_secs().min(0.1);
    let mut stats = crate::actors::ActorStats::default();
    let base = coop.base;
    for (&id, p) in coop.peers.iter_mut() {
        if p.failed {
            continue;
        }
        let Some((place, feet, yaw, moving)) = p.pose(now - DELAY) else { continue };
        let Some(&(_, newest)) = p.samples.back() else { continue };
        // Up again after being down: its body was a ragdoll. And whatever the reason, a
        // player who is up is never drawn lying (a body that missed getting up).
        let lies = p.root.is_some_and(|e| lying.contains(e));
        if std::mem::take(&mut p.redraw) || (lies && !newest.has(flag::DOWN)) {
            if lies {
                info!("co-op: {} is up: drawn standing", label(id, &p.name));
            }
            p.undraw();
        }
        // The world's entities go with the space they belong to: after a trip (here or
        // there), or once its entity is gone, the puppet is spawned again.
        if p.root.is_some_and(|e| roots.get(e).is_err()) {
            p.root = None;
            p.undraw();
        } else if p.drawn_in.is_some_and(|d| d != here || d != place) {
            p.undraw();
        }
        let visible = place == here;
        // Not here: no body of it here either (one spawned but not yet found as its root
        // would stay frozen where it was).
        if !visible && (p.spawned || p.root.is_some()) {
            p.undraw();
        }
        // Taken away now, before it is spawned again below.
        if std::mem::take(&mut p.purge) {
            despawn_bodies(&mut commands, &roots, world_root, &p.references(id).map(|r| (r, None)).collect());
        }
        let reference = p.reference(id);
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
            if let Some(s) = shot_sound(sfx.as_deref(), listener.as_deref(), weapon, feet) {
                debug!("co-op: {} fires {weapon}: sound {}", label(id, &p.name), s.sound);
                sounds.write(s);
            }
        }
    }
}

/// The host: each other player as an actor of this world (module notes), made when it
/// is first seen, put where it is each frame, and enabled only while it is here, in
/// touch and alive; the actors of players who left, or of a game no longer hosted, are
/// deleted. The hits this world's actors land on them go to their games.
#[allow(clippy::too_many_arguments)]
fn stand_ins(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    loc: Option<Res<Location>>,
    actor_world: Option<Res<ActorWorld>>,
    mut sim: Option<ResMut<nvf_play::sim::SimState>>,
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    (mut stats, main_menu, data_state): (ResMut<CombatStats>, Option<Res<nvf_play::main_menu::MainMenu>>, Option<Res<nvf_play::pipboy::data::DataState>>),
    social: Option<Res<nvf_play::coop_social::Social>>,
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
            // A duel does no damage (`nvf_play::coop_social`): this player's shots at its
            // opponent are dropped.
            if by == Some(FormId::PLAYER_REF) && social.as_deref().is_some_and(|s| s.spares(id)) {
                info!("co-op: a hit on player {} dropped: they are dueling", id + 1);
                continue;
            }
            info!("co-op: {} hit player {} for {:.1}", by.map_or("something".to_string(), |b| b.to_string()), id + 1, h.dealt);
            link.send_hit_to(id, encode_hurt(by, &h));
        }
    }
    let Some(loc) = loc.filter(|_| !at_main_menu(main_menu.as_deref())) else { return };
    if let Some(link) = coop.link.as_ref() {
        link.set_quest(quest_view(data_state.as_deref(), &rt.game));
        if let Ok(c) = sim.world.calendar() {
            link.set_clock(net::ClockView { year: c.year, month: c.month, day: c.day, hour: c.hour, days_passed: c.days_passed });
        }
        let players: HashMap<FormId, u8> = coop.peers.iter().filter_map(|(&id, p)| Some((p.actor?, id))).collect();
        let said: Vec<fnv_game::game::Speech> = rt.game.speech.iter().cloned().chain(conversation_line(&rt.game)).collect();
        link.set_lines(lines_out(&said, |r| players.get(&r).copied()));
        // The host's world's presence, made again when it changed (or now and then: a
        // load makes a new world).
        let now = time.elapsed_secs();
        let serial = sim.world.presence_serial();
        if coop.presence_made.is_none_or(|(s, at)| s != serial || now - at >= PRESENCE_REMAKE) {
            coop.presence_made = Some((serial, now));
            link.set_presence(presence_of(&sim.world));
        }
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
                    p.undraw();
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
    // The other players' actors are drawn as puppets here (the puppet only says so once
    // it is first posed, `act`): a guest's own stand-in, sent as an NPC, stood frozen where
    // it was first drawn.
    let players: HashSet<u32> = coop.peers.values().filter_map(|p| p.actor).map(|r| r.0).collect();
    let mut npcs = Vec::new();
    for (v, t, lo, hold, dying, seen) in &actors {
        // The dead too, out of sight or not: a guest lays its own body down.
        if (!seen.get() && !dying) || is_puppet(v.0) || players.contains(&v.0) {
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
    mut actors: Query<(Entity, &RefVisual, Option<&mut WeaponHold>, Option<&mut Locomotion>, Has<Remote>, Has<Dying>, &Transform), (With<crate::limbs::ActorRig>, Without<Puppet>)>,
    mut fighters: ResMut<Fighters>,
    actor_world: Option<Res<ActorWorld>>,
    (mut attacks, mut hits): (MessageWriter<PlayAttack>, MessageWriter<ActorHit>),
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    mut told: Local<usize>,
    (sfx, listener, mut sounds, mut sim, loc): (Option<Res<Sfx>>, Option<Res<Player>>, MessageWriter<PlaySound>, Option<ResMut<nvf_play::sim::SimState>>, Option<Res<Location>>),
    solo: Option<Res<nvf_play::save::SoloBase>>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    // The host's NPCs reach this game once it is kept as the guest's own (module notes).
    let guest = coop.link.as_ref().is_some_and(|l| !l.is_host() && l.id().is_some()) && !at_main_menu(main_menu.as_deref()) && solo.is_some();
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
                g.set_remote_speech(Vec::new(), |_| false);
            }
            // What the host's world hid of this one's is back.
            if let Some(sim) = sim.as_deref_mut() {
                for r in coop.hidden.drain() {
                    let _ = sim.world.set_enabled(r, true);
                }
                for (r, was) in coop.presence_was.drain() {
                    let _ = sim.world.set_enabled(r, was);
                }
            }
            coop.presence_was.clear();
            coop.presence = HostPresence::default();
            coop.unheard.clear();
            coop.host_lines = None;
            coop.host_quest = None;
            coop.quest_untold = None;
            for (e, v, _, _, marked, _, _) in &mut actors {
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
    // Where the host's story put the NPCs it draws here, they are here too.
    let here = loc.as_deref().map(place_of);
    let settled = coop.host_place.filter(|(p, _)| Some(*p) == here).map(|(_, since)| since);
    if let (Some(sim), Some(since)) = (sim.as_deref_mut(), settled) {
        for r in place_like_host(&mut sim.world, g, &coop.npcs, since + HOST_SETTLE, now) {
            coop.hidden.remove(&r);
            coop.unheard.remove(&r.0);
        }
    }
    // What the host's scripts enabled or disabled, all over its world (module notes); the
    // NPCs the host draws and the actors hidden here are left to the rest.
    if let Some(sim) = sim.as_deref_mut() {
        let (npcs, hidden) = (&coop.npcs, &coop.hidden);
        let skip = |r: FormId| npcs.contains_key(&r.0) || hidden.contains(&r);
        let fresh = std::mem::take(&mut coop.presence.fresh);
        let n = follow_presence(&mut sim.world, &fresh, &skip, &mut coop.presence_was);
        if n > 0 {
            info!("co-op: {n} references enabled or disabled as in the host's world");
        }
        if let Some(host) = coop.presence.whole.take() {
            let n = undo_own_presence(&mut sim.world, &host, &skip, &mut coop.presence_was);
            if n > 0 {
                info!("co-op: {n} references this game alone enabled or disabled take the plugins' flags again");
            }
        }
    }
    // This game's own actors near the host that the host's world does not have: hidden
    // (module notes). Only once the host's NPCs come, and the host has been here a while.
    match (sim.as_deref_mut(), settled.filter(|s| now - s >= HIDE_AFTER), coop.host) {
        (Some(sim), Some(_), Some(host)) if !coop.npcs.is_empty() => {
            let drawn: Vec<(u32, [f32; 3])> = actors.iter().map(|(_, v, _, _, _, _, t)| (v.0, t.translation.to_array())).collect();
            let heard: HashSet<u32> = coop.npcs.keys().copied().collect();
            let missing = not_in_hosts_world(&drawn, &heard, host.feet, HIDE_REACH);
            coop.unheard.retain(|r, _| missing.contains(r));
            for r in missing {
                let since = *coop.unheard.entry(r).or_insert(now);
                if now - since < HIDE_AFTER || !coop.hidden.insert(FormId(r)) {
                    continue;
                }
                info!("co-op: {} is not in the host's world here: hidden", g.data.name(FormId(r)));
                let _ = sim.world.set_enabled(FormId(r), false);
            }
        }
        _ => coop.unheard.clear(),
    }
    // The lines the host's NPCs are saying, as long as each lasts.
    let me = coop.link.as_ref().and_then(|l| l.id());
    let lines = coop.host_lines.as_ref().map_or_else(Vec::new, |(at, lines)| lines_in(lines, now - at, me));
    let npcs = &coop.npcs;
    g.set_remote_speech(lines, |r| npcs.contains_key(&r.0));
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
    for (e, v, hold, lo, marked, dying, _) in &mut actors {
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
            // Its shot, heard as the host's NPC's is there (no shot is fired here).
            if let Some(snd) = weapon.filter(|_| !melee).and_then(|w| shot_sound(sfx.as_deref(), listener.as_deref(), w, s.feet)) {
                sounds.write(snd);
            }
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

/// A shot of `weapon` fired by someone standing at `feet`, heard here: its 3D shoot sound,
/// or its distant one beyond the 3D sound's reach, as `nvf_play::combat` plays an NPC's
/// (a puppet's gun mods are not known: the plain sound).
fn shot_sound(sfx: Option<&Sfx>, listener: Option<&Player>, weapon: FormId, feet: [f32; 3]) -> Option<PlaySound> {
    let sfx = sfx?;
    let shoot = sfx.forms.weapons.get(&weapon)?.shoot_sounds(false);
    let near_max = shoot.near.and_then(|n| sfx.forms.sounds.get(&n)).map_or(0.0, |n| n.max_distance);
    let at = [feet[0], feet[1], feet[2] + GUN_HEIGHT];
    let distance = listener.map_or(0.0, |p| {
        let e = p.eye();
        ((e[0] - at[0]).powi(2) + (e[1] - at[1]).powi(2) + (e[2] - at[2]).powi(2)).sqrt()
    });
    let (sound, _) = shoot.heard_at(distance, near_max);
    Some(PlaySound { sound: sound?, at: Some(at) })
}

/// A guest takes the host's clock when they differ by more than [`CLOCK_SLACK`].
fn take_clock(world: &mut fnv_sim::World, c: net::ClockView) {
    let Ok(own) = world.calendar() else { return };
    // By the date and hour, not GameDaysPassed alone: a script (or the console) can set
    // GameHour without it.
    let same_day = (own.year, own.month, own.day) == (c.year, c.month, c.day);
    let off = if same_day { c.hour - own.hour } else { (c.days_passed - own.days_passed) * 24.0 };
    if same_day && off.abs() <= CLOCK_SLACK && (c.days_passed - own.days_passed).abs() * 24.0 <= CLOCK_SLACK {
        return;
    }
    if off.abs() >= 0.5 {
        info!("co-op: the host's clock: day {} {:02}:{:02} (this game was {off:+.1} h off)", c.day, c.hour as u32, (c.hour.fract() * 60.0) as u32);
    }
    let cal = fnv_sim::clock::Calendar { year: c.year, month: c.month, day: c.day, hour: c.hour, days_passed: c.days_passed };
    if let Err(e) = world.set_calendar(cal) {
        warn_once!("co-op: cannot take the host's clock: {e}");
    }
}

/// A host makes its presence list again at least this often (seconds).
const PRESENCE_REMAKE: f32 = 2.0;

/// A guest's view of the host's world's presence (`net::Msg::Presence`), page by page.
#[derive(Default)]
struct HostPresence {
    serial: u32,
    pages: u16,
    got: HashSet<u16>,
    refs: HashSet<u32>,
    /// Flags heard and not yet applied.
    fresh: Vec<(u32, bool)>,
    /// The whole list (every page of one serial), once, for [`undo_own_presence`].
    whole: Option<HashSet<u32>>,
}

impl HostPresence {
    fn hear(&mut self, serial: u32, page: u16, pages: u16, refs: Vec<(u32, bool)>) {
        if serial != self.serial || pages != self.pages {
            (self.serial, self.pages) = (serial, pages);
            self.got.clear();
            self.refs.clear();
        }
        self.refs.extend(refs.iter().map(|r| r.0));
        self.fresh.extend(refs);
        if self.got.insert(page) && self.got.len() == pages as usize {
            self.whole = Some(self.refs.clone());
        }
    }
}

/// A placed reference's own enabled flag as its plugin gives it; `None` for what takes no
/// part in presence (the player, references made at run time, unknown ones).
fn authored_enabled(world: &fnv_sim::World, r: FormId) -> Option<bool> {
    if r == FormId::PLAYER_REF || r.0 >= 0xFF00_0000 {
        return None;
    }
    world.placed.get(r).map(|p| !p.initially_disabled())
}

/// A host's world's presence (module notes): each placed reference whose own enabled flag
/// differs from its plugin's, with the flag, in FormID order.
fn presence_of(world: &fnv_sim::World) -> Vec<(u32, bool)> {
    world.changed().filter(|(r, st)| authored_enabled(world, *r).is_some_and(|a| a != st.enabled)).map(|(r, st)| (r.0, st.enabled)).collect()
}

/// A guest takes the host's flags (`refs`), except for what `skip` holds; what it changes
/// is kept in `was` as it was first. Returns how many it changed.
fn follow_presence(world: &mut fnv_sim::World, refs: &[(u32, bool)], skip: &dyn Fn(FormId) -> bool, was: &mut HashMap<FormId, bool>) -> usize {
    let mut n = 0;
    for &(r, on) in refs {
        let r = FormId(r);
        if skip(r) || authored_enabled(world, r).is_none() {
            continue;
        }
        let Ok(st) = world.get(r) else { continue };
        if st.enabled != on {
            was.entry(r).or_insert(st.enabled);
            let _ = world.set_enabled(r, on);
            n += 1;
        }
    }
    n
}

/// A guest with the host's whole list (`host`): what its own game alone enabled or
/// disabled (not on the list, not in `skip`) takes its plugin's flag again, kept in `was`
/// as it was first. Returns how many it changed.
fn undo_own_presence(world: &mut fnv_sim::World, host: &HashSet<u32>, skip: &dyn Fn(FormId) -> bool, was: &mut HashMap<FormId, bool>) -> usize {
    let own: Vec<(FormId, bool)> = world
        .changed()
        .filter(|(r, _)| !host.contains(&r.0) && !skip(*r))
        .filter_map(|(r, st)| authored_enabled(world, r).filter(|a| *a != st.enabled).map(|a| (r, a)))
        .collect();
    for &(r, authored) in &own {
        was.entry(r).or_insert(!authored);
        let _ = world.set_enabled(r, authored);
    }
    own.len()
}

/// A guest in the host's place: each NPC the host draws, enabled here and put in this
/// cell when the host's story enabled it or moved it (module notes). Only states heard
/// after `after` and lately: the ones before are the place the host left (the host's NPCs
/// there are forgotten only after [`NPC_FORGET`]). Actors the host's scripts made (`FF`
/// references) are this game's own numbers here: left alone. One dead in this game but
/// alive in the host's stands up again (`Game::resurrect`). Returns those it enabled.
fn place_like_host(world: &mut fnv_sim::World, g: &mut fnv_game::Game, npcs: &HashMap<u32, NpcTrack>, after: f32, now: f32) -> Vec<FormId> {
    let mut enabled = Vec::new();
    let Some(cell) = world.peek(FormId::PLAYER_REF).and_then(|s| s.cell) else { return enabled };
    let player_space = world.placed.world_of(cell);
    for (&r, t) in npcs {
        let Some(&(at, s)) = t.samples.back() else { continue };
        if at < after || now - at > 1.0 || r >= 0xFF00_0000 || s.has(npc_flag::DEAD) {
            continue;
        }
        let id = FormId(r);
        let Ok(st) = world.get(id) else { continue };
        if st.deleted {
            continue;
        }
        if g.is_dead(id) {
            info!("co-op: {} is alive in the host's world: up again here", g.data.name(id));
            g.resurrect(id, false);
        }
        let Ok(st) = world.get(id) else { continue };
        let enable = !st.enabled;
        // Elsewhere: another interior, or another worldspace, than this player's.
        let elsewhere = st.cell.is_none_or(|c| c != cell && (player_space.is_none() || world.placed.world_of(c) != player_space));
        if elsewhere {
            let to = world.placed.exterior_cell_at(cell, s.feet).unwrap_or(cell);
            if let Ok(st) = world.get(id) {
                st.cell = Some(to);
                st.pos = s.feet;
                st.rot = [0.0, 0.0, s.heading];
            }
            g.teleported.insert(id);
        }
        if enable {
            let _ = world.set_enabled(id, true);
            enabled.push(id);
        }
        if enable || elsewhere {
            info!("co-op: {id} {} as in the host's world", match (enable, elsewhere) { (true, true) => "enabled and brought here", (true, false) => "enabled", _ => "brought here" });
        }
    }
    enabled
}

/// Despawn the bodies (actor roots under the world root) of the references in `keep`,
/// except the one each keeps.
fn despawn_bodies(commands: &mut Commands, roots: &Query<(Entity, &RefVisual, &ChildOf)>, world_root: Entity, keep: &HashMap<u32, Option<Entity>>) {
    if keep.is_empty() {
        return;
    }
    for (e, v, parent) in roots {
        if parent.parent() != world_root {
            continue;
        }
        if let Some(k) = keep.get(&v.0).filter(|k| **k != Some(e)) {
            info!("co-op: a body of {:08X} taken away{}", v.0, if k.is_some() { " (a stray: not its puppet)" } else { "" });
            commands.entity(e).try_despawn();
        }
    }
}

/// A guest's own actors drawn within `reach` of the host's feet (across the floor) that
/// the host's NPCs (`heard`) do not include: not in the host's world there (module
/// notes). The other players' puppets are not actors of either world.
fn not_in_hosts_world(drawn: &[(u32, [f32; 3])], heard: &HashSet<u32>, host: [f32; 3], reach: f32) -> HashSet<u32> {
    drawn
        .iter()
        .filter(|(r, feet)| !is_puppet(*r) && !heard.contains(r) && (feet[0] - host[0]).hypot(feet[1] - host[1]) <= reach)
        .map(|(r, _)| *r)
        .collect()
}

/// The line the NPC is saying in the host's own conversation, as a line said to the host
/// (the guests hear it from where the NPC stands): the time left is that of the response
/// it is at and of those after it, so a guest's game finds the same response.
fn conversation_line(g: &fnv_game::Game) -> Option<fnv_game::game::Speech> {
    let c = g.dialogue.as_ref()?;
    let line = c.line.as_ref()?;
    let order = g.data.infos.get(&line.info)?.response_order();
    let lengths = g.response_lengths(c.actor, line.info);
    let after: f64 = order.iter().skip(line.response + 1).map(|&r| lengths.get(r).copied().unwrap_or(0.0)).sum();
    let mut s = lines_in(&[net::LineView { speaker: c.actor.0, target: 0, topic: line.topic.0, info: line.info.0, remaining: 1.0, flags: 0, conversation: 0 }], 0.0, None).pop()?;
    s.target = Some(FormId::PLAYER_REF);
    s.remaining = line.remaining.max(0.0) + after;
    s.remote = false;
    Some(s)
}

/// The host's lines as they go to the guests: its player as [`net::LineView::HOST`], a
/// guest's actor (`player`) as that guest; a line by an actor the host's scripts made
/// (a reference no other game has) is not sent, nor a listener of that kind.
fn lines_out(speech: &[fnv_game::game::Speech], player: impl Fn(FormId) -> Option<u8>) -> Vec<net::LineView> {
    let map = |r: FormId| match (r, player(r)) {
        (FormId::PLAYER_REF, _) => Some(net::LineView::HOST),
        (_, Some(id)) => Some(net::LineView::guest(id)),
        (r, None) if r.0 >= 0xFF00_0000 => None,
        (r, None) => Some(r.0),
    };
    use net::LineView as L;
    speech
        .iter()
        .filter(|s| !s.remote)
        .filter_map(|s| {
            let speaker = map(s.speaker).filter(|r| !is_puppet(*r))?;
            let mut flags = 0;
            for (on, f) in [(s.say_to, L::SAY_TO), (s.greeting, L::GREETING), (s.hello, L::HELLO), (s.force_subtitles, L::FORCE_SUBTITLES), (s.two_d, L::TWO_D)] {
                flags |= if on { f } else { 0 };
            }
            Some(L { speaker, target: s.target.and_then(map).unwrap_or(0), topic: s.topic.0, info: s.info.0, remaining: s.remaining as f32, flags, conversation: s.conversation.unwrap_or(0).min(u16::MAX as u32) as u16 })
        })
        .take(net::LINES_MAX)
        .collect()
}

/// The host's lines as a guest says them, `ago` seconds after they came: each with the
/// time it has left (those over are gone), this player (`me`) for its own id, the
/// others' puppets for theirs.
fn lines_in(lines: &[net::LineView], ago: f32, me: Option<u8>) -> Vec<fnv_game::game::Speech> {
    use net::LineView as L;
    let map = |r: u32| if me.is_some_and(|m| r == L::guest(m)) { FormId::PLAYER_REF } else { FormId(r) };
    lines
        .iter()
        .filter(|l| l.remaining - ago > 0.0)
        .map(|l| fnv_game::game::Speech {
            speaker: map(l.speaker),
            target: (l.target != 0).then(|| map(l.target)),
            topic: FormId(l.topic),
            info: FormId(l.info),
            remaining: (l.remaining - ago) as f64,
            say_to: l.has(L::SAY_TO),
            greeting: l.has(L::GREETING),
            conversation: (l.conversation != 0).then_some(l.conversation as u32),
            earlier_topics: Vec::new(),
            hello: l.has(L::HELLO),
            look: fnv_game::game::LineLook::Both,
            response: 0,
            force_subtitles: l.has(L::FORCE_SUBTITLES),
            two_d: l.has(L::TWO_D),
            end_ran: false,
            remote: true,
        })
        .collect()
}

/// Each other player's flashlight, on their puppet where its eyes are (`flashlight.rs`),
/// while it is on and the player is in this place.
fn peer_lights(mut commands: Commands, time: Res<Time>, coop: Res<Coop>, loc: Option<Res<Location>>, mut lights: Query<&mut Transform>, mut lit: Local<HashMap<u8, Entity>>) {
    let here = loc.as_deref().map(place_of);
    let now = time.elapsed_secs();
    let mut want = HashMap::new();
    for (&id, p) in &coop.peers {
        let (Some((place, feet, yaw, _)), Some(&(_, s))) = (p.pose(now - DELAY), p.samples.back()) else { continue };
        if Some(place) != here || p.lost || !s.has(flag::LIGHT) || s.has(flag::DOWN) {
            continue;
        }
        let eye = [feet[0], feet[1], feet[2] + if s.has(flag::SNEAKING) { nvf_play::player::EYE_HEIGHT * 0.7 } else { nvf_play::player::EYE_HEIGHT }];
        want.insert(id, crate::flashlight::placement(eye, yaw, s.pitch));
    }
    lit.retain(|id, e| {
        let keep = want.contains_key(id);
        if !keep {
            commands.entity(*e).despawn();
        }
        keep
    });
    for (id, place) in want {
        match lit.get(&id).copied() {
            Some(e) => {
                if let Ok(mut t) = lights.get_mut(e) {
                    *t = place;
                }
            }
            None => {
                debug!("co-op: player {}'s flashlight is on", id + 1);
                let e = commands.spawn((Name::new(format!("Flashlight of player {}", id + 1)), crate::flashlight::spot(), place)).id();
                lit.insert(id, e);
            }
        }
    }
}

/// Down, revived and respawned (module notes): this player down, the others' puppets
/// lying down while they are, and reviving one in reach.
#[allow(clippy::too_many_arguments)]
fn downed(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    (mut script, mut sim, mut phys, mut ragdolls): (Option<ResMut<nvf_play::script::ScriptState>>, Option<ResMut<nvf_play::sim::SimState>>, Option<ResMut<nvf_play::phys::PhysState>>, ResMut<crate::ragdoll::Ragdolls>),
    player: Option<Res<Player>>,
    loc: Option<Res<Location>>,
    input: nvf_play::bindings::ControlInput,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    mut travel: MessageWriter<TravelRequest>,
    mut hits: MessageWriter<ActorHit>,
    mut cam: Option<ResMut<crate::death_cam::DeathCam>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
) {
    let coop = &mut *coop;
    let in_coop = coop.link.is_some();
    if let Some(c) = cam.as_deref_mut().filter(|c| c.downed_in_coop != in_coop) {
        c.downed_in_coop = in_coop;
    }
    let (Some(rt), Some(sim), Some(player), Some(loc)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim.as_deref_mut(), player, loc) else { return };
    if !in_coop || at_main_menu(main_menu.as_deref()) {
        coop.downed_at = None;
        return;
    }
    let now = time.elapsed_secs();
    let here = place_of(&loc);
    let mut say = |n: String| {
        info!("co-op: {n}");
        if let Some(c) = corner.as_mut() {
            c.push(n.into());
        }
    };
    let g = &mut rt.game;
    // The others' puppets: down, they lie down as the dead do; up, they are drawn anew.
    for (&id, p) in coop.peers.iter_mut() {
        let Some(&(_, s)) = p.samples.back() else { continue };
        let down = s.has(flag::DOWN);
        // A player up is never dead here, whatever happened while it was down.
        if !down && !p.down && g.is_dead(FormId(p.reference(id))) {
            g.extra.entry(FormId(p.reference(id))).or_default().dead = false;
        }
        if down == p.down {
            continue;
        }
        p.down = down;
        let r = p.reference(id);
        g.extra.entry(FormId(r)).or_default().dead = down;
        if down {
            hits.write(ActorHit { form_id: r, died: true, knocked_out: false, location: -1, crippled: false, part_crippled: false, torso_crippled: false, by: None, share: 1.0, guard_broken: false, weapon: None });
            say(format!("{} is down: revive them with a Stimpak", label(id, &p.name)));
        } else {
            if let Some(ph) = phys.as_deref_mut() {
                ph.world.remove_ragdoll(r);
            }
            ragdolls.rest.remove(&r);
            p.redraw = true;
            say(format!("{} is back up", label(id, &p.name)));
        }
    }
    let key = input.bindings.get(nvf_play::bindings::Control::Activate).key.map_or("Activate".to_string(), |k| format!("{k:?}").trim_start_matches("Key").to_string());
    let pressed = input.just_pressed(nvf_play::bindings::Control::Activate);
    // This player.
    if g.is_dead(FormId::PLAYER_REF) {
        let since = *coop.downed_at.get_or_insert_with(|| {
            say(format!("You are down: another player can revive you with a Stimpak. In {RESPAWN_AFTER:.0} seconds you can respawn"));
            now
        });
        if now - since < RESPAWN_AFTER {
            return;
        }
        // Beside the nearest player up, else anyone (the host first).
        let feet = player.feet;
        let dist = |q: &PlayerState| if q.place == here { (q.feet[0] - feet[0]).hypot(q.feet[1] - feet[1]) } else { f32::MAX / 2.0 };
        let target = coop
            .peers
            .iter()
            .filter_map(|(&id, p)| p.samples.back().map(|&(_, q)| (id, p, q)))
            .min_by(|a, b| (a.2.has(flag::DOWN) || a.1.lost, dist(&a.2), a.0).partial_cmp(&(b.2.has(flag::DOWN) || b.1.lost, dist(&b.2), b.0)).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, p, q)| (label(id, &p.name), q));
        if !coop.told_respawn {
            coop.told_respawn = true;
            say(format!("Press {key} to respawn{}", target.as_ref().map_or(String::new(), |(n, _)| format!(" by {n}"))));
        }
        if pressed {
            g.resurrect(FormId::PLAYER_REF, false);
            if let Some((name, q)) = target {
                let (world, interior) = match q.place {
                    Place::Interior(c) => (None, Some(c)),
                    Place::Exterior(w) => (Some(w), None),
                };
                travel.write(TravelRequest(Travel { world, interior, feet: q.feet, heading_deg: q.yaw.to_degrees() }));
                say(format!("You respawned by {name}"));
            } else {
                say("You respawned".into());
            }
            coop.downed_at = None;
            coop.told_respawn = false;
        }
        return;
    }
    coop.downed_at = None;
    coop.told_respawn = false;
    // A downed player in reach: revive them with a Stimpak.
    let feet = player.feet;
    let near = coop
        .peers
        .iter()
        .filter(|(_, p)| p.down && !p.lost)
        .filter_map(|(&id, p)| {
            let &(_, q) = p.samples.back()?;
            let d = (q.feet[0] - feet[0]).hypot(q.feet[1] - feet[1]);
            (q.place == here && d <= REVIVE_REACH && (q.feet[2] - feet[2]).abs() < REVIVE_REACH).then(|| (d, id, label(id, &p.name)))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0));
    let Some((_, id, name)) = near else {
        coop.revive_offer = None;
        return;
    };
    if coop.revive_offer != Some(id) {
        coop.revive_offer = Some(id);
        say(format!("Press {key} to revive {name} (uses a Stimpak)"));
    }
    if !pressed {
        return;
    }
    let have = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map_or(0, |i| i.count(STIMPAK));
    if have < 1 {
        say(format!("You need a Stimpak to revive {name}"));
        return;
    }
    let _ = sim.world.remove_item(FormId::PLAYER_REF, STIMPAK, 1);
    if let Some(link) = coop.link.as_ref() {
        link.send_revive(id);
    }
    say(format!("You revived {name} (Stimpak used, {} left)", have - 1));
}

/// Shared progression between the link and `nvf_play::coop_progress` (quest rewards, a
/// save by anyone saving everyone, everyone down loading the last shared save): this
/// game's role, whether every player is down (the host's view), what was heard and what
/// to send.
fn progress(mut coop: ResMut<Coop>, mut p: ResMut<nvf_play::coop_progress::CoopProgress>, script: Option<Res<nvf_play::script::ScriptState>>, main_menu: Option<Res<nvf_play::main_menu::MainMenu>>) {
    use nvf_play::coop_progress::{Role, Shared};
    let coop = &mut *coop;
    let role = match coop.link.as_ref() {
        None => Role::Solo,
        Some(l) if l.is_host() => Role::Host,
        Some(_) => Role::Guest,
    };
    p.role = role;
    let me_down = !at_main_menu(main_menu.as_deref()) && script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(FormId::PLAYER_REF));
    p.everyone_down = role == Role::Host && nvf_play::coop_progress::everyone_down(me_down, coop.peers.values().map(|q| (q.down, q.lost)));
    let kind_in = |k: u8| match k {
        net::Progress::QUICK => fnv_save::SaveKind::Quick,
        net::Progress::AUTO => fnv_save::SaveKind::Auto,
        net::Progress::SYSTEM => fnv_save::SaveKind::System,
        _ => fnv_save::SaveKind::Manual,
    };
    for (id, what) in coop.progress_in.drain(..) {
        p.inbox.push((
            id,
            match what {
                net::Progress::Reward { quest, xp, caps } => Shared::Reward { quest, xp, caps },
                net::Progress::Save { stamp, kind } => Shared::Save { stamp, kind: kind_in(kind) },
                net::Progress::Reload { stamp } => Shared::Reload { stamp },
            },
        ));
    }
    let out = std::mem::take(&mut p.outbox);
    let Some(link) = coop.link.as_ref() else { return };
    for what in out {
        link.send_progress(match what {
            Shared::Reward { quest, xp, caps } => net::Progress::Reward { quest, xp, caps },
            Shared::Save { stamp, kind } => net::Progress::Save {
                stamp,
                kind: match kind {
                    fnv_save::SaveKind::Quick => net::Progress::QUICK,
                    fnv_save::SaveKind::Auto => net::Progress::AUTO,
                    fnv_save::SaveKind::System => net::Progress::SYSTEM,
                    fnv_save::SaveKind::Manual => net::Progress::MANUAL,
                },
            },
            Shared::Reload { stamp } => net::Progress::Reload { stamp },
        });
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
            // Its head is the other player's: it does not turn to look at this one.
            commands.entity(e).insert((Puppet, crate::head_look::NoHeadLook));
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

    fn speech(speaker: u32, target: Option<u32>, remaining: f64) -> fnv_game::game::Speech {
        let mut s = lines_in(&[net::LineView { speaker, target: 0, topic: 0x10, info: 0x11, remaining: 1.0, flags: 0, conversation: 0 }], 0.0, None).remove(0);
        s.target = target.map(FormId);
        s.remaining = remaining;
        s.remote = false;
        s
    }

    /// The host's lines reach a guest as the host says them: its player is the host's
    /// puppet there, a guest's actor that guest's player (or its puppet on another
    /// guest); a line by an actor the host's scripts made is not sent; the time left
    /// counts down from when they came.
    #[test]
    fn the_hosts_lines_reach_a_guest_with_its_people() {
        let stand_in = FormId(0xFF00_0812);
        let mut scene = speech(0x0010_4C3A, Some(0x0010_4C1D), 4.0);
        scene.conversation = Some(3);
        scene.say_to = true;
        let hello = speech(0x0010_4C3A, Some(FormId::PLAYER_REF.0), 2.0);
        let to_guest = speech(0x0010_4C1D, Some(stand_in.0), 1.0);
        let made = speech(0xFF00_0900, None, 5.0);
        let mut own_remote = speech(0x0010_4C1D, None, 5.0);
        own_remote.remote = true;
        let out = lines_out(&[scene.clone(), hello, to_guest, made, own_remote], |r| (r == stand_in).then_some(1));
        assert_eq!(out.len(), 3, "{out:?}");
        assert_eq!((out[1].target, out[2].target), (net::LineView::HOST, net::LineView::guest(1)));
        // On guest 1, half a second later.
        let heard = lines_in(&out, 0.5, Some(1));
        assert_eq!(heard.len(), 3);
        assert!(heard.iter().all(|s| s.remote));
        assert_eq!((heard[0].speaker, heard[0].target, heard[0].conversation, heard[0].say_to), (scene.speaker, scene.target, Some(3), true));
        assert!((heard[0].remaining - 3.5).abs() < 1e-6);
        assert_eq!(heard[1].target, Some(FormId(PUPPET_REF)));
        assert_eq!(heard[2].target, Some(FormId::PLAYER_REF));
        // On guest 2, the line to guest 1 turns to guest 1's puppet; past its end, gone.
        let other = lines_in(&out, 0.5, Some(2));
        assert_eq!(other[2].target, Some(FormId(PUPPET_REF + 1)));
        assert_eq!(lines_in(&out, 1.5, Some(2)).len(), 2);
        assert!(lines_in(&out, 4.5, Some(2)).is_empty());
    }

    fn placed(id: u32, disabled: bool) -> fnv_sim::refs::PlacedRef {
        fnv_sim::refs::PlacedRef {
            id: FormId(id), sig: *b"REFR", edid: None, base: FormId(0x1F), cell: None, stored_in: None, plugin: 0,
            flags: if disabled { fnv_sim::refs::FLAG_INITIALLY_DISABLED } else { 0 },
            in_persistent_group: false, pos: [0.0; 3], rot: [0.0; 3], scale: None, count: None, ammo: None,
            health: None, owner: None, lock: None, teleport: None, enable_parent: None, linked: None,
            patrol_idle: None, patrol_topic: None, merchant_container: None, marker_reputation: None,
            open_by_default: false, primitive: None, activate_parents: Vec::new(), parent_activate_only: false,
            prompt: None, radiation: None, radius: None, encounter_zone: None, level_modifier: None,
        }
    }

    /// The host's world's presence reaches a guest's (module notes): the host's own flags
    /// that differ from the plugins' (Sunny's bottles, enabled by their disabled parent
    /// marker; a crate it disabled) are taken; once the whole list is in, what only the
    /// guest's game changed (its own done quest's sign) takes the plugins' flag again,
    /// except what is skipped (the NPCs the host draws); leaving puts it all back.
    #[test]
    fn the_hosts_world_presence_wins_on_a_guest() {
        const BOTTLE: u32 = 0x0010_A202;
        const PARENT: u32 = 0x0016_8A92;
        const CRATE: u32 = 0x0010_0001;
        const SIGN: u32 = 0x0010_0002;
        const GECKO: u32 = 0x0010_0003;
        let mut table = fnv_sim::RefTable::default();
        let mut bottle = placed(BOTTLE, false);
        bottle.enable_parent = Some(fnv_sim::refs::EnableParent { reference: FormId(PARENT), opposite: false, pop_in: false });
        for p in [bottle, placed(PARENT, true), placed(CRATE, false), placed(SIGN, true), placed(GECKO, true)] {
            table.refs.insert(p.id, p);
        }
        let forms = fnv_sim::FormStore::default();
        let mut host = fnv_sim::World::new(&forms, &table, Default::default());
        host.set_enabled(FormId(PARENT), true).unwrap();
        host.set_enabled(FormId(CRATE), false).unwrap();
        let _ = host.get(FormId(BOTTLE)).unwrap();
        let list = presence_of(&host);
        assert_eq!(list, vec![(CRATE, false), (PARENT, true)]);

        let mut guest = fnv_sim::World::new(&forms, &table, Default::default());
        guest.set_enabled(FormId(SIGN), true).unwrap();
        guest.set_enabled(FormId(GECKO), true).unwrap();
        assert!(!guest.is_enabled(FormId(BOTTLE)).unwrap());
        let mut heard = HostPresence::default();
        heard.hear(1, 0, 1, list);
        let skip = |r: FormId| r == FormId(GECKO);
        let mut was = HashMap::new();
        assert_eq!(follow_presence(&mut guest, &std::mem::take(&mut heard.fresh), &skip, &mut was), 2);
        assert!(guest.is_enabled(FormId(BOTTLE)).unwrap(), "the bottles come with their parent");
        assert!(!guest.is_enabled(FormId(CRATE)).unwrap());
        assert_eq!(undo_own_presence(&mut guest, &heard.whole.take().unwrap(), &skip, &mut was), 1);
        assert!(!guest.is_enabled(FormId(SIGN)).unwrap());
        assert!(guest.is_enabled(FormId(GECKO)).unwrap(), "skipped");
        // Leaving co-op: as it was.
        for (r, on) in was.drain() {
            guest.set_enabled(r, on).unwrap();
        }
        assert!(guest.is_enabled(FormId(SIGN)).unwrap() && guest.is_enabled(FormId(CRATE)).unwrap() && !guest.is_enabled(FormId(BOTTLE)).unwrap());
    }

    /// A guest has the host's whole list only once every page of one serial came; a new
    /// serial starts again.
    #[test]
    fn the_whole_presence_list_is_every_page_of_one_serial() {
        let mut p = HostPresence::default();
        p.hear(4, 0, 2, vec![(1, true)]);
        assert!(p.whole.is_none());
        p.hear(5, 1, 2, vec![(2, true)]);
        assert!(p.whole.is_none(), "another serial");
        p.hear(5, 0, 2, vec![(3, false)]);
        assert_eq!(p.whole.take(), Some(HashSet::from([2, 3])));
        assert_eq!(p.fresh.len(), 3);
    }

    /// A guest hides its own actors near the host that the host's world does not draw;
    /// not the host's NPCs, not those far off, not the other players' puppets.
    #[test]
    fn what_the_host_does_not_have_near_it_is_hidden() {
        let host = [1000.0, 1000.0, 50.0];
        let drawn = [
            (0x0010_4C3A, [1100.0, 900.0, 0.0]),
            (0x0010_4C1D, [1200.0, 1000.0, 0.0]),
            (0x0010_5000, [9000.0, 1000.0, 0.0]),
            (PUPPET_REF, [1000.0, 1000.0, 50.0]),
            (0xFF00_0123, [1000.0, 1500.0, 0.0]),
        ];
        let heard: HashSet<u32> = [0x0010_4C3A].into();
        let hidden = not_in_hosts_world(&drawn, &heard, host, HIDE_REACH);
        assert_eq!(hidden, [0x0010_4C1D, 0xFF00_0123].into());
    }

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
