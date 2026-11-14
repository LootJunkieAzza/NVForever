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
//! is moved there ([`follow_npcs`]). An actor the host's scripts made (a dynamic
//! reference) is made in the guest's world under the host's number, of the host's base,
//! and every NPC's leveled picks are read at the host's level and rolls, so each looks
//! the same on every screen ([`follow_looks`]); the guest's own created references are
//! numbered above the host's ([`GUEST_CREATED`]). The host's world wins whatever the
//! guest's save did: an NPC dead in the guest's game but alive in the host's stands up
//! again ([`place_like_host`]), and an actor the guest's game draws near the host that the
//! host's world does not have (dead or disabled there, not there yet) is disabled for as
//! long as the guest is in the host's world ([`not_in_hosts_world`]). What the host's
//! scripts enabled or disabled that is not an NPC it draws (Sunny's bottles, the markers
//! that are other references' enable parents) goes as its world's presence: every placed
//! reference whose own enabled flag differs from its plugin's ([`presence_of`],
//! `net::Msg::Presence`); a guest takes those flags, and once it has the whole list puts
//! back its plugin's flag on what only its own game changed ([`follow_presence`]). Doors
//! go the same way ([`share_doors`]): one a player opens or closes (the host: or its
//! world, a script's SetOpenState, an NPC) goes to everyone at once (`net::Msg::Door`),
//! and the host's doors whose open flag differs from their plugin's go as a list, for a
//! guest who joins or arrives later; a guest's door swings as the host's does. Lit
//! campfires of Burning Campfire Redone go the same way ([`campfires`]): the host lists
//! those burning in its game, a guest lights and puts out its own to match, and asks the
//! host to light one it lit. The host's script scenes go too ([`scene`]): a guest is held
//! as the host's scripts hold the host, sees the same fades and is brought beside the
//! host. So a
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
//! without the death camera's slow motion (`death_cam::DeathCam::downed_in_coop`) but
//! fading to black, where the downed screen says how to respawn or that someone revives
//! (`nvf_play::coop_downed`, queue item 46). Another player who aims at the body where it
//! lies on their screen (its pelvis, [`revive`]) gets the game's own rollover, "E) Revive"
//! over the name (`nvf_play::coop_social::target`), and pressing Activate revives them
//! with a Stimpak of their own (one is used up): the downed player, healed
//! (`Game::resurrect`), gets up from where the body lies as a duel's loser does: knocked
//! down in that pose, the camera following the body, the get-up clip from the ragdoll's
//! pose (`player_knockdown.rs`, [`crate::player_knockdown::PlayerKnockdown::revived`]);
//! on the others' screens its puppet plays the get-up clip from its own ragdoll's pose
//! ([`revive`]). After [`RESPAWN_AFTER`] a downed player can
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
use crate::world::{Streamer, WorldData, WorldRoot};
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

mod campfires;
mod story;
mod replication;
mod areas;
mod far_combat;
mod leave;
pub use leave::HoldLeave;
mod scene;
mod revive;
mod rejoin;
mod syncing;

/// The port `--join` uses when the address names none (and a code without a port means).
pub const DEFAULT_PORT: u16 = nvf_coop::code::DEFAULT_PORT;
/// How far behind the newest state the others are drawn, so there are two to blend.
const DELAY: f32 = 0.1;
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
/// Env var (a test aid): `1` starts a session with Friendly Fire on.
pub const FRIENDLY_FIRE_ENV: &str = "NVF_COOP_FRIENDLY_FIRE";
/// Another player up within this distance (same place) can come and revive a downed one;
/// farther, a downed guest loads its last save (owner, item 129 Q5) [design].
const ALONE_NEAR: f32 = 8192.0;
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
/// Between two of the host's states of an NPC, a move longer than this (units) and
/// faster than it runs (at most [`NPC_JUMP_SPEED`], units/s) is a script's move, put there at once, not
/// walked [design: no actor of the game runs near 2000 units/s; a sprinting player is
/// a few hundred].
const NPC_JUMP: f32 = 64.0;
const NPC_JUMP_SPEED: f32 = 2000.0;
/// How much faster than its run speed an NPC may move between the host's states before
/// it counts as a jump [design: a run's speed-ups and the states' jitter; the fastest
/// runner seen, a cazador, runs 955 units/s, observed: creature pace logs].
const NPC_RUN_ROOM: f32 = 1.5;
/// States further apart in time than this (seconds) with a move of more than
/// [`NPC_JUMP`] between them: the host did not send it meanwhile (faded out, disabled for
/// the move), so it is put there too. Two rounds of the slowest rate the host sends an
/// actor's pose at (3 Hz, far from the guest: `repl::pose_hz`): at three rounds of
/// [`net::NPC_HZ`] (0.35 s) every pose of a far actor running came after a "gap", and a
/// guest held the Dead Horses still and put them 131-166 units on about every 0.36 s
/// (laptop deadhorse-look 74bac664, item 187 (a)) [observed].
const NPC_GAP: f32 = 2.0 / SLOWEST_POSE_HZ;
/// How long (seconds) a guest carries a moving NPC on past its newest state, waiting for
/// the next: one round of the slowest pose rate [design].
const NPC_CARRY_ON: f32 = 1.0 / SLOWEST_POSE_HZ;
/// The slowest rate (Hz) the host sends an actor's pose to a guest at (`repl::pose_hz`).
const SLOWEST_POSE_HZ: f32 = 3.0;
/// How many of its own rounds behind its newest state a guest draws an NPC whose states
/// come further apart than [`NPC_DELAY`] allows for, up to [`NPC_DELAY_MAX`] seconds: drawn
/// between two states, not held at the newest until the next comes [design].
const NPC_DELAY_ROUNDS: f32 = 1.5;
const NPC_DELAY_MAX: f32 = 0.6;
/// The time (seconds) a guest takes to go over to another delay when an NPC's states come
/// faster or slower (it comes nearer or into view): the drawn copy runs a little slower
/// or faster meanwhile instead of jumping back or on [design].
const NPC_DELAY_EASE: f32 = 1.0;
/// The least time between two of an NPC's states on a guest, in rounds of
/// [`net::NPC_HZ`] [design: under one round, so stamps on time do not drift later than
/// they came; [`NPC_DELAY`] is two rounds, so a spaced state is still blended to].
const NPC_SPACING: f32 = 0.75;
/// The host goes on sending an NPC it stopped drawing this long (seconds), from where its
/// AI has it, longer than a guest's [`NPC_FORGET`] would keep it ([`share_npcs`]).
const NPC_LINGER: f32 = 5.0;
/// For this long after a guest first hears an NPC (seconds), a state with no furniture
/// does not take it off the furniture it is on here (item 204d: a host after a load
/// sends an actor it draws as off for the frame before it seats it) [design].
const NPC_FIRST_HOLD: f32 = 2.0;

/// The furniture a guest has one of the host's NPCs on: the host's (`heard`: furniture
/// and on it), or, when the host says none within [`NPC_FIRST_HOLD`] of its first state
/// (`young`), the one it is on here (`here`) [design, item 204d].
fn furniture_here(heard: Option<(u32, bool)>, young: bool, here: Option<u32>) -> Option<(u32, bool)> {
    heard.or_else(|| here.filter(|_| young).map(|f| (f, true)))
}

/// The furniture the host sends for an NPC it draws: the one it is on (`on`: furniture,
/// seated, getting off), none while it gets off, else the one it is being put on at
/// once (`putting_on`, [`Residents::being_put_on`]) as on it [item 204d].
fn sent_furniture(on: Option<(u32, bool, bool)>, putting_on: Option<u32>) -> (u32, bool) {
    match on {
        Some((_, _, true)) => (0, false),
        Some((f, seated, false)) => (f, seated),
        None => putting_on.map_or((0, false), |f| (f, true)),
    }
}
/// How often every body of the other players is checked for strays (seconds, [`draw`]).
const SWEEP_EVERY: f32 = 1.0;
/// A guest's own actor this near the host (game units, across the floor), which the
/// host's NPCs have not included for [`HIDE_AFTER`] seconds (and the host has been in
/// the place that long), is not in the host's world there ([`not_in_hosts_world`]).
/// Well inside the cells the host draws around it.
const HIDE_REACH: f32 = 3000.0;
const HIDE_AFTER: f32 = 3.0;
/// A guest's own references created at run time are numbered from here, above the
/// host's, which it makes under the host's numbers ([`follow_looks`]).
const GUEST_CREATED: u32 = 0xFF80_0000;

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
            joining_held: false,
            host_quest: None,
            quest_untold: None,
            host_place: None,
            downed_at: None,
            told_respawn: false,
            alone_down: false,
            friendly_fire: std::env::var(FRIENDLY_FIRE_ENV).is_ok_and(|v| v == "1"),
            revive_offer: None,
            revived_by: None,
            strays: Vec::new(),
            swept_at: 0.0,
            unheard: HashMap::new(),
            hidden: HashSet::new(),
            host_disabled: HashSet::new(),
            presence: HostPresence::default(),
            presence_was: HashMap::new(),
            presence_made: None,
            doors_seen: HashMap::new(),
            door_used: HashMap::new(),
            doors_in: Vec::new(),
            doors_heard: HostPresence::default(),
            doors_was: HashMap::new(),
            doors_made: None,
            looks_taken: HashMap::new(),
            numbered: false,
            wear_heard: HashMap::new(),
            wear_taken: HashMap::new(),
            host_lines: None,
            handed_off: false,
            progress_in: Vec::new(),
            campfires: Default::default(),
            story: Default::default(),
            repl: Default::default(),
            areas: Default::default(),
            far: Default::default(),
            sent_npcs: HashMap::new(),
            repl_hud: None,
            ended: false,
            leaving: Default::default(),
            parting: false,
            scene: Default::default(),
        };
        // The main menu's run after a lobby ended neither hosts nor joins, whatever the
        // command line it was started with says (`leave`).
        let ended = env(leave::NO_LOBBY_ENV).is_some();
        std::env::remove_var(leave::NO_LOBBY_ENV);
        if ended {
            info!("co-op: the lobby ended in the run before: not hosting or joining");
        } else if let Some(port) = c.host.or_else(|| env(HOST_ENV).and_then(|p| p.trim().parse().ok())) {
            coop.start_hosting(port);
        } else if let Some(join) = c.join.clone().or_else(|| env(JOIN_ENV)) {
            coop.start_joining(&join);
        }
        app.add_plugins((nvf_play::lobby::LobbyPlugin, nvf_play::coop_progress::CoopProgressPlugin))
            .insert_resource(coop)
            .add_systems(Update, (replication::frame_clock, lobby, send, take_events, share_doors, campfires::share_campfires, tables, social_mates, share_npcs, areas::coop_areas, replication::replicate, follow_looks, follow_npcs, (leash, rejoin::rejoin_host, syncing::syncing).chain(), scene::share_scene, downed, revive::bodies, progress, duel_falls, replication::hud).chain().after(nvf_play::PlaySet::Physics))
            .add_systems(Update, peer_emotes.after(social_mates))
            .add_systems(Update, hold_joining_world.before(nvf_play::PlaySet::Scripts))
            .add_systems(Update, (replication::dismember_out.before(replication::replicate), replication::dismember_in.after(replication::replicate)))
            .add_systems(Update, (replication::corpses_out.before(replication::replicate), replication::corpses_in.after(replication::replicate)))
            .add_systems(Update, far_combat::far_fights.after(areas::coop_areas).before(replication::replicate))
            .add_systems(Update, scene::story_prompts.after(scene::share_scene).before(downed))
            .init_resource::<HoldLeave>()
            .add_systems(Update, (leave::guest_leaves.before(crate::handoff::leave), leave::tell_ended))
            .add_systems(Last, leave::host_ends)
            .add_systems(Update, story::share_story.after(take_events).after(nvf_play::PlaySet::Physics))
            .add_message::<PuppetShot>()
            .add_systems(Update, (stand_ins, draw, scene::hide_players_in_scene, peer_lights, pipboy_share, swimmers).chain().after(nvf_play::PlaySet::Sync).before(crate::locomotion::walk_test));
    }
}

/// The load-order indices of the official masters (`fnv_game::Game::story_plugins`).
static STORY_PLUGINS: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();

/// Note which of `plugins` (the load order) are the official `masters`: their quests are
/// the host's story, which a replication guest does not run.
pub fn set_story_plugins(plugins: &[String], masters: &[&str]) {
    let story = plugins.iter().enumerate().filter(|(_, p)| masters.iter().any(|m| m.eq_ignore_ascii_case(p))).map(|(i, _)| i as u8).collect();
    let _ = STORY_PLUGINS.set(story);
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
pub(crate) struct Coop {
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
    /// A joining guest: its own world logic is held until it is flagged a guest
    /// ([`hold_joining_world`]).
    joining_held: bool,
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
    /// Down with no other player up near ([`ALONE_NEAR`]): a guest loads its last save
    /// (`nvf_play::coop_progress`, item 129).
    alone_down: bool,
    /// The lobby's Friendly Fire rule (item 122): set by the host, sent to the guests
    /// (`replication`); off, players' hits on each other (and their companions') are
    /// dropped by the host, which passes every such hit on (duels still work).
    pub(crate) friendly_fire: bool,
    /// The downed player this one was last told it can revive.
    revive_offer: Option<u8>,
    /// Revived by this player (name), shown on the black until it lifts.
    revived_by: Option<String>,
    /// References whose every body in the world is taken away on the next frame ([`draw`]:
    /// players who left), and when the bodies were last checked for strays.
    strays: Vec<u32>,
    swept_at: f32,
    /// A guest's: its own actors near the host that the host's world does not have, since
    /// when each has not been heard of, and those it disabled ([`not_in_hosts_world`]).
    unheard: HashMap<u32, f32>,
    hidden: HashSet<FormId>,
    /// A guest's: references the host's scripts disabled (and have not enabled since):
    /// an NPC sample of the host's from before the disable does not show it here again
    /// ([`place_like_host`]; item 186).
    host_disabled: HashSet<u32>,
    /// A guest's: the host's world's presence as heard ([`follow_presence`]), and the
    /// enabled flags it changed in this world, as they were (put back on leaving).
    presence: HostPresence,
    presence_was: HashMap<FormId, bool>,
    /// A host's: its world's presence serial when the list was last made, and when.
    presence_made: Option<(u64, f32)>,
    /// Doors ([`share_doors`]): each door's open flag as last seen here, the doors this
    /// player used lately (and when), the doors others opened or closed (who, the door,
    /// open) not yet applied; a guest's: the host's doors' list as heard and the flags it
    /// changed here, as they were; a host's: when its list was last made.
    doors_seen: HashMap<u32, bool>,
    door_used: HashMap<u32, f32>,
    doors_in: Vec<(u8, u32, bool)>,
    doors_heard: HostPresence,
    doors_was: HashMap<FormId, bool>,
    doors_made: Option<f32>,
    /// A guest's: the host's level, all-levels flag and rolls taken for each NPC
    /// ([`follow_looks`]), and whether its own created references are numbered from
    /// [`GUEST_CREATED`] yet.
    looks_taken: HashMap<u32, (u16, bool, u8)>,
    numbered: bool,
    /// A guest: what the host's game draws each of its NPCs wearing and holding
    /// (`net::Msg::Wear`), and what this game last dressed each in ([`follow_looks`]).
    wear_heard: HashMap<u32, Vec<u32>>,
    wear_taken: HashMap<u32, Vec<u32>>,
    /// A guest's: the lines the host's NPCs are saying, and when they came.
    host_lines: Option<(f32, Vec<net::LineView>)>,
    /// A host that let go of its port for a load's new run ([`lobby`]).
    handed_off: bool,
    /// Shared progression heard, for [`progress`].
    progress_in: Vec<(u8, net::Progress)>,
    /// Lit campfires ([`campfires::share_campfires`]).
    campfires: campfires::Shared,
    /// The host's story for the guests ([`story::share_story`]).
    story: story::Shared,
    /// Replication (`replication`, docs/design/coop-replication.md), and its newest perf
    /// line for the corner readout.
    repl: replication::Repl,
    areas: areas::Areas,
    /// The host's fights of its undrawn actors with guests away from it (item 129).
    far: far_combat::FarFights,
    /// The host's: the NPCs it sent last round, and since when each has not been drawn
    /// ([`share_npcs`]: sent on a while from where its AI has it, item 179).
    sent_npcs: HashMap<u32, Option<f32>>,
    repl_hud: Option<String>,
    /// A guest's: the host ended the lobby (told, or gone too long), not yet acted on
    /// ([`leave::guest_leaves`]).
    ended: bool,
    /// A guest's way out while it autosaves ([`leave`]).
    leaving: leave::Leaving,
    /// A guest's way out is done (autosaved, the link stopped) and the main menu is on its
    /// way: this game still stands in the host's world until then ([`follow_npcs`]).
    parting: bool,
    /// The host's script scenes ([`scene::share_scene`]).
    scene: scene::Shared,
}

/// One of the host's NPCs as a guest has heard it.
#[derive(Default)]
struct NpcTrack {
    samples: VecDeque<(f32, NpcState)>,
    /// How far behind its newest state it is drawn (seconds; 0 until first drawn):
    /// [`eased_delay`].
    delay: f32,
    /// When its first state was heard (seconds of `Time`): a new host's first states
    /// do not take it off its furniture here ([`furniture_here`]).
    first_heard: Option<f32>,
    /// The attack counter played up to (None until the first state).
    played: Option<u8>,
    attacked_at: Option<f32>,
}

/// The host's NPCs whose every state heard and pose drawn a guest logs
/// (`NVF_COOP_NPC_TRACE`: hex references, comma-separated), to see how the states of one
/// come in (item 187 (a): the Dead Horses put about 150 units on every 0.36 s on a guest).
fn traced(r: u32) -> bool {
    static LIST: std::sync::OnceLock<Vec<u32>> = std::sync::OnceLock::new();
    LIST.get_or_init(|| std::env::var("NVF_COOP_NPC_TRACE").map(|v| v.split(',').filter_map(|s| u32::from_str_radix(s.trim(), 16).ok()).collect()).unwrap_or_default())
        .contains(&r)
}

/// How far behind its newest state a guest draws an NPC this frame (seconds): from
/// `delay`, eased over [`NPC_DELAY_EASE`] towards [`NPC_DELAY_ROUNDS`] of the time between
/// its states (`spacing`), at least [`NPC_DELAY`] and at most [`NPC_DELAY_MAX`].
fn eased_delay(delay: f32, spacing: f32, dt: f32) -> f32 {
    let target = (spacing * NPC_DELAY_ROUNDS).clamp(NPC_DELAY, NPC_DELAY_MAX);
    if delay <= 0.0 {
        return target;
    }
    delay + (target - delay) * (dt / NPC_DELAY_EASE).clamp(0.0, 1.0)
}

impl NpcTrack {
    /// The time between its states as they came (seconds; [`NPC_DELAY`]'s round when
    /// fewer than two).
    fn spacing(&self) -> f32 {
        match (self.samples.front(), self.samples.back()) {
            (Some((a, _)), Some((b, _))) if self.samples.len() > 1 => (b - a) / (self.samples.len() - 1) as f32,
            _ => 1.0 / net::NPC_HZ as f32,
        }
    }

    /// A state heard at `now`. States heard in one frame (one late, the next on time) are
    /// spaced [`NPC_SPACING`] of a round apart, not given the same time: blended over no
    /// time, the second was put on at once, and the Nipton survivor running about 516
    /// units/s on a guest was "put 48-56 units on in one frame" 13 times (laptop ship run
    /// of s187-nipton) [observed].
    fn heard(&mut self, now: f32, n: NpcState) {
        self.first_heard.get_or_insert(now);
        let at = self.samples.back().map_or(now, |&(last, _)| now.max(last + NPC_SPACING / net::NPC_HZ as f32));
        self.samples.push_back((at, n));
        while self.samples.len() > 8 {
            self.samples.pop_front();
        }
    }

    /// Where to draw it at `t`: feet and heading blended between the states either side.
    /// `top`: the fastest it moves walking or running (units/s); a move faster than that
    /// (and longer than [`NPC_JUMP`]) is a jump.
    /// Past its newest state while it was moving: carried on as it went between its last
    /// two states, for at most [`NPC_CARRY_ON`] seconds, rather than held there until the
    /// next comes. When its states come further apart all at once (from 10 Hz to 3 Hz as
    /// it goes far from the guest), its delay is still short and the newest state is
    /// passed: a guest held 0100B824 and put it 22-65 units on about every 0.35 s for a
    /// second or two (laptop deadhorse-look d7992be2, item 187 (a)) [observed].
    fn carried_on(&self, t: f32, top: f32) -> Option<([f32; 3], f32, bool)> {
        let n = self.samples.len();
        let ((ta, a), (tb, b)) = (*self.samples.get(n.checked_sub(2)?)?, *self.samples.back()?);
        let span = tb - ta;
        let gap = (0..3).map(|k| (b.feet[k] - a.feet[k]).powi(2)).sum::<f32>().sqrt();
        if !b.has(npc_flag::MOVING) || a.space != b.space || span <= 1e-3 || span > NPC_GAP || gap / span > top {
            return None;
        }
        let k = (t - tb).clamp(0.0, NPC_CARRY_ON) / span;
        Some((std::array::from_fn(|i| b.feet[i] + (b.feet[i] - a.feet[i]) * k), b.heading, true))
    }

    fn pose(&self, t: f32, top: f32) -> Option<([f32; 3], f32, bool)> {
        let last = self.samples.back()?.1;
        let (a, b, f) = match self.samples.iter().position(|(at, _)| *at > t) {
            None => return Some(self.carried_on(t, top).unwrap_or((last.feet, last.heading, false))),
            Some(0) => return Some((self.samples[0].1.feet, self.samples[0].1.heading, false)),
            Some(i) => {
                let (ta, a) = self.samples[i - 1];
                let (tb, b) = self.samples[i];
                // Put there, not walked there: a script's move between two states
                // (MoveTo, MoveToMarkerWithFade) or a trip into another space is drawn
                // where it ends once its time comes. Blended, Kimball's fade move at the
                // Hoover Dam slid across on a guest at 1069 units/s (item 187).
                let gap = (0..3).map(|k| (b.feet[k] - a.feet[k]).powi(2)).sum::<f32>().sqrt();
                if a.space != b.space || (gap > NPC_JUMP && (gap / (tb - ta).max(1e-4) > top || tb - ta > NPC_GAP)) {
                    return Some(if t < tb { (a.feet, a.heading, false) } else { (b.feet, b.heading, false) });
                }
                (a, b, ((t - ta) / (tb - ta).max(1e-4)).clamp(0.0, 1.0))
            }
        };
        let feet = std::array::from_fn(|k| a.feet[k] + (b.feet[k] - a.feet[k]) * f);
        let turn = (b.heading - a.heading + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
        Some((feet, a.heading + turn * f, b.has(npc_flag::MOVING)))
    }
}

impl Coop {
    /// The reference player `id`'s puppet is drawn with on this screen (its actor's on
    /// the host, else [`PUPPET_REF`] + its id), for `match_end.rs`.
    pub(crate) fn puppet_reference(&self, id: u8) -> u32 {
        self.peers.get(&id).map_or(PUPPET_REF + id as u32, |p| p.reference(id))
    }

    fn start_hosting(&mut self, port: u16) {
        let Some(link) = host_session(port) else {
            self.note = Some(format!("Cannot open UDP port {port} on this PC: is another game using it?"));
            return;
        };
        // The guests are told the host's name (`net::Msg::Names`).
        link.set_name(&self.name);
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
    (tables, script): (Option<Res<Tables>>, Option<Res<nvf_play::script::ScriptState>>),
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
            // A guest leaves its host's lobby (dropping the link says goodbye) and plays on
            // alone in its own world.
            LobbyRequest::Leave if coop.is_guest() => {
                coop.stop_joining(&mut commands, &mut fighters);
                say("You left the lobby".into());
            }
            LobbyRequest::Leave => {}
            LobbyRequest::FriendlyFire(on) if !coop.is_guest() => {
                let on = *on;
                coop.friendly_fire = on;
                // Kept for the run a load starts (`crate::handoff`).
                std::env::set_var(FRIENDLY_FIRE_ENV, if on { "1" } else { "0" });
                say(format!("Friendly fire {}", if on { "on: players can hurt each other" } else { "off: players can't hurt each other" }));
            }
            LobbyRequest::FriendlyFire(_) => say("Only the host sets friendly fire".into()),
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
                // A first join: the new run holds the syncing screen (item 186).
                std::env::set_var(nvf_play::coop_progress::SYNC_ENV, "1");
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
        // This player by its character's name, as the others call it (item 19).
        let me = script.as_deref().and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.player.name.clone()).filter(|n| !n.trim().is_empty()).unwrap_or_else(|| coop.name.clone());
        players.push(if coop.is_host() { format!("{me} (host, you)") } else { format!("{me} (you)") });
        for (&id, p) in &coop.peers {
            let n = label(id, &p.name);
            // The host is player 0 (`nvf_coop`): marked on a guest's list.
            let n = if id == 0 && n != "The host" { format!("{n} (host)") } else { n };
            // A player at a minigame table says so (the others' tables, module notes).
            let n = match tables.as_deref().and_then(|t| t.at_table(id)) {
                Some(g) => format!("{n} (at {})", nvf_play::coop_tables::game::word(g)),
                None => n,
            };
            players.push(if p.lost { format!("{n} (reconnecting)") } else { n });
        }
    }
    state.set_if_neq(LobbyState { hosting, players, joining: coop.joining.clone(), guest: coop.is_guest(), friendly_fire: coop.friendly_fire });
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
    /// Its character's name, once its look brought one: then its `name`, whatever
    /// co-op name it joined with.
    character: Option<String>,
    /// The head it chose for the others' maps (`nvf_play::coop_heads`), empty for the
    /// plain Vault Boy.
    head: String,
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
    /// The throw its fallen puppet's ragdoll is still to get (game units a second, game
    /// space), and since when it waits for the ragdoll ([`duel_falls`]).
    fall_shove: Option<([f32; 3], f32)>,
    /// Down: where its puppet's body lies here (pelvis, game space; [`revive`]).
    body: Option<[f32; 3]>,
    /// Revived: its puppet getting up from where the body lay ([`revive`]).
    rising: Option<revive::Rising>,
    /// Its own body was seen knocked down (`flag::LYING`) and its puppet fell here for it
    /// ([`duel_falls`]); cleared once the body is up again, so one knockdown is one fall.
    lying_fell: bool,
    /// Fell here for its own game's knockdown, a duel this game took no part in: where
    /// its player's feet were then, and when; its puppet's ragdoll is thrown the way they
    /// go next ([`duel_falls`]).
    fall_from: Option<([f32; 3], f32)>,
    /// When its puppet's ragdoll last got a throw ([`duel_falls`]): the loser's own
    /// velocity (`Msg::Thrown`) arriving after this game threw it already is not a second
    /// throw.
    thrown_at: Option<f32>,
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
    tables.host = link.is_host();
    tables.names = coop.peers.iter().map(|(&id, p)| (id, p.name.clone())).collect();
    // This player's own name too, for lines about a game it plays in (arm wrestling).
    if let Some(me) = tables.me.filter(|_| !coop.name.is_empty()) {
        tables.names.insert(me, coop.name.clone());
    }
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
/// What the other players here emote, for their puppets (`crate::emote_play`).
fn peer_emotes(coop: Res<Coop>, loc: Option<Res<Location>>, out: Option<ResMut<crate::emote_play::PeerEmotes>>) {
    let Some(mut out) = out else { return };
    let here = loc.as_deref().map(place_of);
    let want: Vec<(u32, nvf_coop::net::EmoteState)> = coop
        .peers
        .iter()
        .filter_map(|(&id, p)| {
            let &(_, s) = p.samples.back()?;
            (!p.lost && Some(s.place) == here && s.emote.id != 0).then_some((p.reference(id), s.emote))
        })
        .collect();
    if out.0 != want {
        out.0 = want;
    }
}

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
                        Some(Mate { id, name: label(id, &p.name), reference: p.reference(id), here: s.place == here, feet: s.feet, yaw: s.yaw, down: s.has(flag::DOWN), body: p.body.filter(|_| s.has(flag::DOWN)), lost: p.lost, weapon: s.weapon, attacks: s.attacks, sights: s.has(flag::SIGHTS), pitch: s.pitch })
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

/// The longest the loser of a duel lies on the winner's screen: its puppet gets up when
/// the loser's own body does (`flag::LYING` ends), and is drawn standing if that has not
/// begun by then (seconds) [design: its own knockdown lies about 2.5 s, observed
/// 2026-11-07].
const DUEL_FALL_MAX: f32 = 8.0;

/// How long a fallen puppet's throw waits for its ragdoll before it is let go (seconds).
const DUEL_SHOVE_WAIT: f32 = 2.0;

/// A knocked-down player's feet gone this far (game units) in its own game tell which
/// way its body was thrown [design: above the samples' jitter].
const FALL_TELL: f32 = 12.0;
/// How long after the fall that is looked for (seconds) [design: a throw shows within
/// a few samples; a body that has not moved by then fell in place].
const FALL_TELL_WAIT: f32 = 0.6;

/// The loser of a duel against this player: its puppet falls as a downed player's does
/// ([`downed`]), its ragdoll thrown away from this player as the loser's own game throws
/// it (`nvf_play::coop_social::duel::throw`, the same velocity), and gets up with the
/// loser's own body (`revive::bodies`), at most [`DUEL_FALL_MAX`] after the fall.
#[allow(clippy::too_many_arguments)]
fn duel_falls(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    mut social: ResMut<nvf_play::coop_social::Social>,
    (mates, player): (Res<nvf_play::coop_social::SocialMates>, Option<Res<nvf_play::player::Player>>),
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    mut phys: Option<ResMut<nvf_play::phys::PhysState>>,
    mut ragdolls: ResMut<crate::ragdoll::Ragdolls>,
    (mut hits, mut pushes): (MessageWriter<ActorHit>, MessageWriter<nvf_play::combat::ActorPush>),
    mut corpses: Query<(&RefVisual, &mut crate::ragdoll::Corpse, &GlobalTransform)>,
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
        // Thrown away from this player, as its own game throws it.
        let shove = match (mates.get(id), player.as_deref()) {
            (Some(m), Some(me)) => nvf_play::coop_social::duel::throw(m.feet, me.feet),
            _ => [0.0, 0.0, nvf_play::coop_social::duel::THROW.1],
        };
        p.fall_shove = Some((shove, now));
        // Its body's knockdown came first (below): fallen already, only the throw is new.
        if p.lying_fell {
            continue;
        }
        p.fell_at = Some(now);
        // It gets up when the loser's own body does, with the get-up clip from the
        // ragdoll's pose, as a revived player's puppet does (`revive::bodies`).
        p.rising = Some(revive::Rising::Start { since: now });
        g.extra.entry(FormId(r)).or_default().dead = true;
        hits.write(ActorHit { form_id: r, died: true, knocked_out: false, location: -1, crippled: false, part_crippled: false, torso_crippled: false, by: None, share: 1.0, guard_broken: false, weapon: None });
        info!("co-op: {} lost the duel: the puppet falls, thrown at ({:.0}, {:.0}, {:.0}) units/s", label(id, &p.name), shove[0], shove[1], shove[2]);
        p.lying_fell = true;
    }
    // The loser's own game says how it threw the body (`Msg::Thrown`, sent to every
    // player): every copy is thrown with that velocity. Before, the winner's game threw
    // its copy from where it saw the two stand, and a third player's fell in place and was
    // pushed only once the loser's feet were seen to go (FALL_TELL), so it crumbled first
    // (owner 2026-11-09: "it seems like the loser just crumbles").
    for (id, velocity) in std::mem::take(&mut social.throws) {
        let Some(p) = coop.peers.get_mut(&id).filter(|p| !p.down) else { continue };
        p.fall_from = None;
        if p.thrown_at.is_some_and(|t| now - t < 2.0) {
            // This game's own throw went first (the winner's): the ragdoll has it.
            info!("co-op: {} was thrown in its own game too: its ragdoll here was already thrown", label(id, &p.name));
            continue;
        }
        p.fall_shove = Some((velocity, now));
        if !p.lying_fell && p.fell_at.is_none() {
            let r = p.reference(id);
            p.fell_at = Some(now);
            p.rising = Some(revive::Rising::Start { since: now });
            g.extra.entry(FormId(r)).or_default().dead = true;
            hits.write(ActorHit { form_id: r, died: true, knocked_out: false, location: -1, crippled: false, part_crippled: false, torso_crippled: false, by: None, share: 1.0, guard_broken: false, weapon: None });
            p.lying_fell = true;
        }
        info!("co-op: {} was thrown at ({:.0}, {:.0}, {:.0}) units/s in its own game: its puppet here is thrown the same", label(id, &p.name), velocity[0], velocity[1], velocity[2]);
    }
    // Any other player whose own body lies knocked down (`flag::LYING`, its own
    // `player_knockdown`): a duel this game took no part in, a blast. Its puppet falls
    // here too, in place, and gets up with its body as a duel's loser does. Before, only
    // the two duelists' games had the fall: a third player saw the loser stand, then jump
    // to where its body had lain once it rose (queue 2026-11-08 item 104).
    for (&id, p) in coop.peers.iter_mut() {
        let lies = p.samples.back().is_some_and(|(_, s)| s.has(flag::LYING) && !s.has(flag::DOWN));
        if !lies {
            p.lying_fell = false;
            continue;
        }
        if p.lying_fell || p.down || p.fell_at.is_some() || p.rising.is_some() {
            continue;
        }
        p.lying_fell = true;
        let r = p.reference(id);
        p.fell_at = Some(now);
        p.rising = Some(revive::Rising::Start { since: now });
        g.extra.entry(FormId(r)).or_default().dead = true;
        hits.write(ActorHit { form_id: r, died: true, knocked_out: false, location: -1, crippled: false, part_crippled: false, torso_crippled: false, by: None, share: 1.0, guard_broken: false, weapon: None });
        info!("co-op: {} is knocked down in its own game: the puppet falls here too", label(id, &p.name));
        p.fall_from = p.samples.back().map(|(_, s)| (s.feet, now));
    }
    // A third player's view of a duel's loser (or a blast's): its own game threw its body,
    // and its feet start to go the throw's way. The puppet's ragdoll is thrown the same
    // way, as the duelists' games throw it ([`nvf_play::coop_social::duel::throw`]): before,
    // it fell in place, came to rest far from where its player got up (270 units in the
    // duel3c run, 2026-11-09) and the get-up had to make that up.
    for (&id, p) in coop.peers.iter_mut() {
        let Some((from, since)) = p.fall_from else { continue };
        let Some(&(_, s)) = p.samples.back() else { continue };
        let gone = (s.feet[0] - from[0]).hypot(s.feet[1] - from[1]);
        if gone >= FALL_TELL {
            let shove = nvf_play::coop_social::duel::throw(s.feet, from);
            p.fall_shove = Some((shove, now));
            p.fall_from = None;
            info!("co-op: {}'s body goes ({:.0}, {:.0}) in its own game: the puppet is thrown that way", label(id, &p.name), shove[0], shove[1]);
        } else if now - since > FALL_TELL_WAIT || !p.lying_fell {
            p.fall_from = None;
        }
    }
    // The throw: the ragdoll starts with it, or (already in the world) is pushed by it.
    for (&id, p) in coop.peers.iter_mut() {
        let Some((shove, since)) = p.fall_shove else { continue };
        let r = p.reference(id);
        match corpses.iter_mut().find(|(v, ..)| v.0 == r) {
            Some((_, mut c, _)) if !c.live => {
                c.velocity = shove;
                p.fall_shove = None;
                p.thrown_at = Some(now);
                info!("co-op: {}'s ragdoll starts thrown at ({:.0}, {:.0}, {:.0}) units/s", label(id, &p.name), shove[0], shove[1], shove[2]);
            }
            Some((_, _, gt)) => {
                pushes.write(nvf_play::combat::ActorPush { form_id: r, point: nvf_play::space::to_game(gt.translation()), dv: shove });
                p.fall_shove = None;
                p.thrown_at = Some(now);
                info!("co-op: {}'s ragdoll is pushed by the throw ({:.0}, {:.0}, {:.0}) units/s", label(id, &p.name), shove[0], shove[1], shove[2]);
            }
            None if now - since > DUEL_SHOVE_WAIT => {
                p.fall_shove = None;
                info!("co-op: {}'s puppet has no ragdoll: it falls in place", label(id, &p.name));
            }
            None => {}
        }
    }
    // Up: the get-up has begun (or the puppet was drawn standing for want of a body, a
    // pose or a clip, `revive::bodies`); without either by then, it stands up as it did.
    for (&id, p) in coop.peers.iter_mut() {
        let Some(t) = p.fell_at else { continue };
        let waiting = matches!(p.rising, Some(revive::Rising::Start { .. }));
        if p.down || (waiting && now - t < DUEL_FALL_MAX) {
            if p.down {
                p.fell_at = None;
                p.fall_shove = None;
            }
            continue;
        }
        p.fell_at = None;
        p.fall_shove = None;
        let r = p.reference(id);
        g.extra.entry(FormId(r)).or_default().dead = false;
        if waiting {
            info!("co-op: {}'s get-up did not start: drawn standing", label(id, &p.name));
            p.rising = None;
            if let Some(ph) = phys.as_deref_mut() {
                ph.world.remove_ragdoll(r);
            }
            ragdolls.rest.remove(&r);
            p.redraw = true;
        } else {
            info!("co-op: {} is up again here after the duel, {:.1} s after the fall", label(id, &p.name), now - t);
        }
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
    /// The furniture holding the player (`nvf_play::furniture`).
    seat: Option<Res<'w, nvf_play::furniture::PlayerSeat>>,
    knockdown: Option<Res<'w, crate::player_knockdown::PlayerKnockdown>>,
    /// The battle music's test: in combat no one may fast travel to this player.
    cues: Option<Res<'w, nvf_play::music::MusicCues>>,
    /// The head the others see this player as on their maps (`nvf_play::coop_heads`).
    head: Option<Res<'w, nvf_play::coop_heads::MapHead>>,
    /// The emote this player plays (`crate::emote_play`).
    emote: Option<Res<'w, crate::emote_play::LocalEmote>>,
    /// A slot machine's idle this player plays (`crate::slots_seat`), in the emote's place.
    slot_idle: Option<Res<'w, crate::slots_seat::SlotIdle>>,
}

/// The others' Pip-Boys for this game: each puppet's check (`crate::pipboy_check`), where
/// the others are for the glance's map (`nvf_play::pipboy::glance::CoopMates`), and
/// whether the full Pip-Boy may leave the world running (`nvf_play::menu::LiveMenu`: in a
/// session).
fn pipboy_share(
    coop: Res<Coop>,
    mut checks: ResMut<crate::pipboy_check::PuppetChecks>,
    mut mates: ResMut<nvf_play::pipboy::glance::CoopMates>,
    mut live: ResMut<nvf_play::menu::LiveMenu>,
    mut vats: ResMut<nvf_play::vats_mode::VatsMode>,
    rt: Option<ResMut<nvf_play::rt_vats::RtVats>>,
) {
    use nvf_play::pipboy::glance::{Check, Mate};
    let in_session = coop.link.is_some();
    if live.allowed != in_session {
        live.allowed = in_session;
    }
    // In a session V is the real-time lock (`nvf_play::rt_vats`, queue item 196), for the
    // host and every guest [design: the owner 2026-11-12, "it should be based on the real
    // time vats mod but just using the new limb darwing technique instaead of the
    // diamond"]. VATS mode, should the lock's switch be off, keeps the clock running: the
    // clock is everyone's (`nvf_play::vats_mode::VatsMode::shared_clock`).
    let fellows: Vec<u32> = coop.peers.iter().flat_map(|(&id, p)| p.references(id)).collect();
    if let Some(mut rt) = rt {
        if rt.fellow_players != fellows {
            rt.fellow_players = fellows.clone();
        }
        if rt.allowed != in_session {
            rt.allowed = in_session;
            info!("vats: {}", if in_session { "a co-op session: V is real-time VATS (the lock)" } else { "no session: V is VATS mode as the settings page has it" });
        }
    }
    if vats.shared_clock != in_session {
        vats.shared_clock = in_session;
    }
    // The other players' bodies, which VATS does not offer as targets.
    if vats.fellow_players != fellows {
        vats.fellow_players = fellows;
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
            let more = |f: u8| newest.more & f != 0;
            seen.push(Mate {
                name: label(id, &p.name),
                interior,
                world,
                feet: newest.feet,
                yaw: newest.yaw,
                reference: p.reference(id),
                head: p.head.clone(),
                combat: more(nvf_coop::net::more_flag::COMBAT),
                no_travel: more(nvf_coop::net::more_flag::NO_TRAVEL),
            });
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
fn send(coop: Res<Coop>, player: Option<Res<Player>>, loc: Option<Res<Location>>, state: Option<Res<State<PlayState>>>, main_menu: Option<Res<nvf_play::main_menu::MainMenu>>, doing: Doing, mut was_lying: Local<bool>) {
    let (Some(link), Some(player), Some(loc)) = (coop.link.as_ref(), player, loc) else { return };
    // At the main menu there is no player yet (the link says it is there all the same).
    if at_main_menu(main_menu.as_deref()) {
        return;
    }
    // Behind a loading screen the place is the one being left: keep the last state sent.
    if state.as_ref().is_some_and(|s| *s.get() == PlayState::Loading) {
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
    let lying = doing.knockdown.as_ref().is_some_and(|k| k.lying());
    flags |= if lying { flag::LYING } else { 0 };
    if lying != *was_lying {
        *was_lying = lying;
        // The others' copies of this player get up when this ends (`revive::bodies`).
        info!("co-op: sent: this player's body {}", if lying { "lies" } else { "gets up" });
    }
    s.flags = flags;
    if doing.phys.as_ref().and_then(|p| p.player.as_ref()).is_some_and(|c| c.swimming) {
        s.more |= nvf_coop::net::more_flag::SWIMMING;
    }
    // Whether the others may fast travel to this player (queue item 134): not in combat,
    // and not in a conversation, a scene holding the movement controls, or with fast
    // travel turned off by a script.
    if doing.cues.as_ref().is_some_and(|c| c.combat) {
        s.more |= nvf_coop::net::more_flag::COMBAT;
    }
    let held = doing.script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.player.fast_travel.travel_disabled || rt.game.player.controls_disabled[0]);
    if held || state.as_ref().is_some_and(|s| *s.get() == PlayState::Dialogue) {
        s.more |= nvf_coop::net::more_flag::NO_TRAVEL;
    }
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
    // Getting off is sent as off: the others' puppet starts its getting-off clip with this
    // player's, not once it has ended (3.4 s late on both screens otherwise; ship13's
    // slots-coop-2p-exit, as armwrestle-c found for NPCs).
    s.furniture = doing.seat.as_deref().filter(|seat| !seat.getting_off).and_then(|seat| seat.on).unwrap_or(0);
    // An emote: the others' puppets play it, facing where the body does (the mouse turns
    // the camera meanwhile, not the body).
    if let Some(e) = doing.emote.as_deref() {
        s.emote = e.state;
        if let Some(h) = e.heading {
            s.yaw = h;
        }
    }
    if let Some(e) = doing.slot_idle.as_deref().map(|x| x.state).filter(|e| e.id != 0) {
        s.emote = e;
    }
    if let Some(a) = doing.anim.as_deref() {
        s.attacks = a.attacks as u16;
        s.reloads = a.reloads as u16;
        s.attack_kind = attack_kind(a.swing.as_deref());
    }
    link.set_state(s);
    // The link sends it again only when it changes (and every LOOK_EVERY).
    let rt = doing.script.as_deref().and_then(|s| s.rt.as_ref());
    let look = rt.and_then(|rt| rt.game.player.look.as_ref());
    // The character's own name (the name menu's), for the others to call this player by.
    let name = rt.and_then(|rt| rt.game.player.name.as_deref()).unwrap_or_default();
    let head = doing.head.as_deref().map_or("", |h| h.0.as_str());
    link.set_look(encode_look(look, name, head));
}

/// A look as it goes over the link: 0 for the player record's own; else 1, the race, sex,
/// hair (0 for bald), hair colour (a flag and RGB), eyes (0 for the race's), the head
/// parts, and the three FaceGen lists, each list a count and its items. After it the
/// character's name: its length and its UTF-8 bytes (at most [`NAME_MAX`]; 0 for none).
/// After the name, when the player chose one, the head the others draw this player as on
/// their maps: its length and the icon's path (`nvf_play::coop_heads`, at most
/// [`HEAD_MAX`] bytes; nothing for the plain Vault Boy).
pub fn encode_look(look: Option<&Look>, name: &str, head: &str) -> Vec<u8> {
    let mut b = look_only(look);
    let push = |b: &mut Vec<u8>, s: &str, max: usize| {
        let mut end = s.len().min(max);
        while !s.is_char_boundary(end) {
            end -= 1;
        }
        b.push(end as u8);
        b.extend_from_slice(&s.as_bytes()[..end]);
    };
    push(&mut b, name, NAME_MAX);
    if !head.is_empty() && head.len() <= HEAD_MAX {
        push(&mut b, head, HEAD_MAX);
    }
    b
}

/// The longest map head path sent (bytes) [design: the icons' paths are under 100].
pub const HEAD_MAX: usize = 160;

/// The longest character name sent (bytes) [design: the name menu's names fit].
pub const NAME_MAX: usize = 48;

fn look_only(look: Option<&Look>) -> Vec<u8> {
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

/// [`encode_look`] read back, the look, the name (empty for none, and from a game that
/// sent no name) and the map head (empty for the plain Vault Boy); None for one that is
/// not a look.
pub fn decode_look(b: &[u8]) -> Option<(Option<Look>, String, String)> {
    let mut rd = Reader(b.get(1..)?);
    let look = match *b.first()? {
        0 => None,
        1 => Some(read_look(&mut rd)?),
        _ => return None,
    };
    let text = |rd: &mut Reader| -> Option<String> {
        if rd.0.is_empty() {
            return Some(String::new());
        }
        let n = rd.u8()? as usize;
        let s = rd.0.get(..n)?;
        rd.0 = &rd.0[n..];
        Some(String::from_utf8_lossy(s).into_owned())
    };
    let name = text(&mut rd)?;
    let head = text(&mut rd)?;
    if !rd.0.is_empty() {
        return None;
    }
    Some((look, name, head))
}

fn read_look(rd: &mut Reader) -> Option<Look> {
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
    let (fggs, fgga, fgts) = (list(rd)?, list(rd)?, list(rd)?);
    Some(Look { race, female, hair, hair_color, eyes, head_parts, fggs, fgga, fgts })
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

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
    (solo, mut knock, creator): (Option<Res<nvf_play::save::SoloBase>>, Option<ResMut<crate::player_knockdown::PlayerKnockdown>>, Option<Res<nvf_play::creator::CreatorData>>),
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
                if !name.is_empty() && p.character.is_none() {
                    p.name = name;
                }
                if is_host {
                    notes.push(format!("{} joined", label(id, &p.name)));
                }
            }
            Event::State { id, state } => {
                if id == 0 {
                    coop.host = Some(state);
                    coop.scene.host_heard(now);
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
            Event::Named { id, name } => {
                info!("co-op: player {} is {name}", id + 1);
                coop.peers.entry(id).or_default().name = name;
            }
            Event::Look { id, look } => {
                let Some((look, character, head)) = decode_look(&look) else {
                    warn!("co-op: {} sent a look this game cannot read", label(id, &coop.peers.get(&id).map_or(String::new(), |p| p.name.clone())));
                    continue;
                };
                let reference = coop.peers.get(&id).map_or(PUPPET_REF + id as u32, |p| p.reference(id));
                if let Some(w) = actor_world.as_deref() {
                    if let Ok(mut looks) = w.looks.lock() {
                        match &look {
                            Some(l) => {
                                // Another player's face is no younger than an adult either
                                // (an older version's, queue item 110).
                                let mut a = nvf_play::creator::actor_look(l);
                                if let Some(was) = creator.as_deref().and_then(|c| c.0.adult(&mut a)) {
                                    info!("co-op: player {}'s look read {was:.0} years: drawn at {}", id + 1, fnv_actor::creator::MIN_AGE);
                                }
                                looks.insert(reference, a)
                            }
                            None => looks.remove(&reference),
                        };
                    }
                }
                let p = coop.peers.entry(id).or_default();
                // Called by its character's name from here on (the name menu's), not the
                // co-op name it joined with.
                if !character.is_empty() && p.character.as_deref() != Some(character.as_str()) {
                    info!("co-op: {} is called {character}", label(id, &p.name));
                    p.name = character.clone();
                    p.character = Some(character);
                }
                if p.head != head {
                    info!("co-op: {} shows on the map as {}", label(id, &p.name), if head.is_empty() { "the plain Vault Boy" } else { head.as_str() });
                    p.head = head;
                }
                // The link sends the look again now and then: drawn again only when it changed.
                if p.look.as_ref() != Some(&look) {
                    info!("co-op: {} looks {}", label(id, &p.name), look.as_ref().map_or("as the player record does".to_string(), |l| format!("race {}, {}", l.race, if l.female { "female" } else { "male" })));
                    p.look = Some(look);
                    p.undraw();
                    p.failed = false;
                }
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
                coop.scene.hold_gear(time.elapsed_secs());
                notes.push("Lost the host: reconnecting".into());
            }
            Event::HostBack => {
                coop.host_lost = false;
                coop.scene.hold_gear(time.elapsed_secs());
                notes.push("Reconnected to the host".into());
            }
            Event::HostGone => {
                coop.host = None;
                coop.ended = true;
                notes.push("The host's game has ended".into());
            }
            Event::Npcs(list) => {
                for n in list {
                    let track = coop.npcs.entry(n.reference).or_default();
                    // A state after a pause in the host's (more than three rounds) that
                    // puts it far on: where a guest's copy is then put at once
                    // ([`NpcTrack::pose`]), to tell what the host did meanwhile.
                    if let Some(&(last, was)) = track.samples.back() {
                        let far = (0..3).map(|k| (n.feet[k] - was.feet[k]).powi(2)).sum::<f32>().sqrt();
                        if now - last > NPC_GAP && far > NPC_JUMP {
                            info!("co-op: {:08X} heard again after {:.2} s, {far:.0} units from its last state", n.reference, now - last);
                        } else if far > NPC_JUMP && far > NPC_JUMP_SPEED / 2.0 * (now - last).max(1e-3) {
                            // On time but far on: what the host drew it doing (item 187 (a):
                            // the Dead Horses put 153-160 units on a guest, no pause heard).
                            info!("co-op: {:08X} heard {far:.0} units on from its last state in {:.2} s", n.reference, now - last);
                        }
                    }
                    track.heard(now, n);
                    if traced(n.reference) {
                        let at = track.samples.back().map_or(now, |(at, _)| *at);
                        info!("co-op trace: {:08X} heard at {now:.3} as {at:.3}: ({:.0}, {:.0}, {:.0}) flags {}", n.reference, n.feet[0], n.feet[1], n.feet[2], n.flags);
                    }
                }
            }
            // A guest's, as its actor's in this world (module notes).
            Event::Hit { id, hit } if is_host => match decode_hit(&hit) {
                Some(mut h) => {
                    let by = coop.peers.get(&id).and_then(|p| p.actor).unwrap_or(FormId::PLAYER_REF);
                    // A guest draws the other players as puppets (`PUPPET_REF` + their
                    // id) and its hits on them name the puppet: here it is the player
                    // itself, the host's player or that guest's actor [playtest 119
                    // item 158: a guest's shots hurt no other player, friendly fire on].
                    if is_puppet(h.target.0) {
                        let whose = (h.target.0 - PUPPET_REF) as u8;
                        match if whose == 0 { Some(FormId::PLAYER_REF) } else { coop.peers.get(&whose).and_then(|p| p.actor) } {
                            Some(t) if whose != id => h.target = t,
                            _ => {
                                info!("co-op: player {}'s hit on puppet {} dropped: no such player here", id + 1, h.target);
                                continue;
                            }
                        }
                    }
                    // Friendly Fire off (item 122): a guest's hit on the host or on another
                    // player hurts nobody.
                    let on_player = h.target == FormId::PLAYER_REF || coop.peers.values().any(|p| p.actor == Some(h.target));
                    if on_player && !coop.friendly_fire {
                        info!("co-op: player {}'s hit on player {} dropped: friendly fire is off", id + 1, h.target);
                        continue;
                    }
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
            Event::Revived { by, perks } => {
                if let Some(rt) = rt.as_deref_mut().filter(|rt| rt.game.is_dead(FormId::PLAYER_REF)) {
                    rt.game.resurrect(FormId::PLAYER_REF, false);
                    // The reviver's Healing Hands (`fnv_game::coop_perks`).
                    if perks & net::revive_perk::CURE_RADS != 0 {
                        let rads = fnv_game::coop_perks::cure_rads(&mut rt.game);
                        info!("co-op: revived with Healing Hands: {rads:.0} rads cured");
                        notes.push(format!("Healing Hands: {rads:.0} rads cured"));
                    }
                    // Up from where the body lies, as a duel's loser gets up
                    // (`player_knockdown.rs`).
                    if let Some(k) = knock.as_deref_mut() {
                        k.revived = true;
                    }
                    coop.downed_at = None;
                    let name = label(by, &coop.peers.get(&by).map_or(String::new(), |p| p.name.clone()));
                    coop.revived_by = Some(name.clone());
                    notes.push(format!("{name} revived you"));
                }
            }
            Event::Lines(lines) => coop.host_lines = Some((now, lines)),
            Event::Presence { list: net::LIST_ENABLED, serial, page, pages, refs } => coop.presence.hear(serial, page, pages, refs),
            Event::Presence { list: net::LIST_OPEN, serial, page, pages, refs } => coop.doors_heard.hear(serial, page, pages, refs),
            Event::Presence { list: net::LIST_CAMPFIRE, pages, refs, .. } => coop.campfires.hear(pages, refs),
            Event::Presence { .. } => {}
            Event::Story(entries) => coop.story.heard.extend(entries),
            Event::Wear(npcs) => coop.wear_heard.extend(npcs),
            Event::Campfire { id, reference } => coop.campfires.asked.push((id, reference)),
            Event::Door { id, reference, open } => coop.doors_in.push((id, reference, open)),
            Event::Scene(v) => coop.scene.heard = Some((v, time.elapsed_secs())),
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
            // For `replication::replicate`.
            e @ (Event::Reliable { .. } | Event::RelReset { .. } | Event::Actors { .. }) => {
                // A new stream from the host (it restarted): its story gear comes again.
                if !is_host && matches!(e, Event::RelReset { id: 0 }) {
                    coop.scene.hold_gear(time.elapsed_secs());
                }
                coop.repl.inbox.push(e)
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
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn leash(
    time: Res<Time>,
    mut coop: ResMut<Coop>,
    (mut player, mut phys): (Option<ResMut<Player>>, Option<ResMut<nvf_play::phys::PhysState>>),
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
    if loading.is_some_and(|l| l.active()) {
        return;
    }
    let now = time.elapsed_secs();
    // The replication build: each player goes where they like (their own doors and fast
    // travel move only them, and the host simulates each one's surroundings); only a
    // script that moves the host's player (a DLC's start, MoveTo) takes the guests along
    // (`replication`'s Summon).
    let Some((interior, w, feet, yaw)) = coop.repl.summon.take() else { return };
    // Beside the host, not on it: a guest put on the host's feet pushed the host 40 units
    // aside, out of Jed's 250-unit greeting at Zion, and the greeting never came
    // (laptop run story-near 2026-11-09).
    // Each guest its own place (item 154: every guest landed on one spot at Zion).
    let slot = coop.link.as_ref().and_then(|l| l.id()).unwrap_or(1);
    let feet = scene::beside_slot(feet, yaw, slot);
    coop.last_leash = Some(now);
    // Already in the host's place: put there at once, with no loading screen (queue item
    // 185: the Dead Money summon within the bunker showed a loading screen for a step).
    let there = if interior != 0 { Place::Interior(interior) } else { Place::Exterior(w) };
    if loc.as_deref().map(place_of) == Some(there) {
        if let (Some(player), Some(phys)) = (player.as_deref_mut(), phys.as_deref_mut().filter(|p| p.player.is_some() && p.ready_at(feet))) {
            phys.place_player(feet, 60.0, 200.0);
            let at = phys.player.as_ref().map_or(feet, |c| c.feet);
            player.feet = at;
            player.placed = Some(at);
            player.heading = yaw;
            info!("co-op: summoned: to the host at {there:?} {at:?} (guest place {slot}): put there, same place");
            return;
        }
    }
    let (world, interior) = if interior != 0 { (None, Some(interior)) } else { (Some(w), None) };
    info!("co-op: summoned: to the host at {:?} {:?} {feet:?} (guest place {slot})", world, interior);
    travel.write(TravelRequest(Travel { world, interior, feet, heading_deg: yaw.to_degrees() }));
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
    (sfx, listener, mut sounds, lying, seats, arm): (Option<Res<Sfx>>, Option<Res<Player>>, MessageWriter<PlaySound>, Query<(), Or<(With<Dying>, With<crate::ragdoll::Corpse>)>>, Query<(Has<crate::furniture::SitOn>, Has<crate::furniture::OnFurniture>, Has<crate::furniture::GetUp>)>, Option<Res<nvf_play::armwrestle_coop::ArmBodies>>),
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
    // Bodies despawned this frame are still in the queries until its end: never taken
    // for a puppet's body again (a body found for one frame, then lost, is drawn anew:
    // a puppet spawned again every frame, never dressed or animated).
    let mut gone = despawn_bodies(&mut commands, &roots, world_root, &keep);
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
        // player who is up is never drawn lying (a body that missed getting up); but for
        // the loser of a duel against this player, while it lies ([`duel_falls`]).
        let lies = p.root.is_some_and(|e| lying.contains(e));
        // Getting up after a revive ([`revive`]): its body plays the clip where it lay.
        if p.rising.is_some() && p.root.is_some() && !p.redraw {
            continue;
        }
        if std::mem::take(&mut p.redraw) || (lies && !newest.has(flag::DOWN) && p.fell_at.is_none()) {
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
            gone.extend(despawn_bodies(&mut commands, &roots, world_root, &p.references(id).map(|r| (r, None)).collect()));
        }
        if p.root.is_some_and(|e| gone.contains(&e)) {
            p.root = None;
        }
        let reference = p.reference(id);
        let base = if p.look.is_some() { PLAYER_BASE } else { base };
        let who = Who { reference, base, creature: false, scale: 1.0, strafe: true, air: newest.has(flag::AIR), jumped: false };
        let sim_world = sim.as_deref().map(|s| &s.world);
        if let Err(e) = show_actor(&mut commands, world_root, visible, actor_world.as_deref(), sim_world, &mut ahead, &mut stats, &mut parts, &mut models, &roots, &gone, &mut actors, who, (&mut p.spawned, &mut p.root), (feet, yaw), moving, dt) {
            warn!("co-op: {} cannot be drawn as {base:#010X}: {e}", label(id, &p.name));
            p.failed = true;
        }
        if p.root.is_some_and(|e| gone.contains(&e)) {
            p.root = None;
        }
        if visible && p.spawned {
            p.drawn_in = Some(here);
        }
        let Some(e) = p.root.filter(|_| visible) else {
            fighters.set_puppet(reference, None);
            continue;
        };
        // The furniture holding the player: its puppet gets on, sits or lies there, and
        // gets off, as an NPC does (`crate::furniture`; queue 2026-11-07 item 58: players
        // were seen standing on beds, chairs and couches). Meanwhile the furniture, not
        // the network, places the body (`actors` leaves it out).
        if let Ok((sitting, on, leaving)) = seats.get(e) {
            // At the arm wrestling table: this game's stool of its side, its game's own
            // stool numbered apart (`nvf_play::armwrestle_coop::ArmBodies`).
            match arm.as_deref().map_or(newest.furniture, |a| a.player_furniture(id, newest.furniture)) {
                0 if on && !leaving => {
                    commands.entity(e).insert(crate::furniture::GetUp);
                    info!("co-op: {} gets off its furniture", label(id, &p.name));
                }
                f if f != 0 && !sitting && !on => {
                    commands.entity(e).insert(crate::furniture::SitOn { furniture: f, stay: None, at_once: false });
                    info!("co-op: {} uses furniture {f:#010X}", label(id, &p.name));
                }
                _ => {}
            }
        }
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
            // Friendly Fire off (item 122): a player's hit, or a player's companion's, hurts
            // no other player.
            if !coop.friendly_fire && by.is_some_and(|b| b == FormId::PLAYER_REF || rt.game.is_coop_actor(b) || rt.game.is_teammate(b)) {
                info!("co-op: a hit on player {} by {} dropped: friendly fire is off", id + 1, by.map_or(String::new(), |b| b.to_string()));
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
            // The replication build's change log carries the enabled flags (`replication`).
            let _ = (serial, presence_of);
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
        // In the guest's own cell, where it is (item 109: a guest away from the host is in
        // the world there, for the AI the host runs in its surroundings, `areas`).
        let own_cell = match place {
            Place::Interior(c) => Some(FormId(c)),
            // Outdoors: the grid cell under its feet (far away in the host's worldspace too).
            Place::Exterior(w) => sim.world.placed.world_grid.get(&FormId(w)).and_then(|g| g.get(&fnv_world::cell_of(feet[0], feet[1]))).copied().or(if place == here { cell } else { None }),
        };
        if let Ok(s) = sim.world.get(r) {
            s.pos = feet;
            s.rot = [0.0, 0.0, yaw];
            s.cell = own_cell.or(cell);
        }
        let _ = sim.world.set_enabled(r, (place == here || own_cell.is_some()) && !p.lost && !newest.has(flag::DOWN));
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
    actors: Query<(&RefVisual, &Transform, Option<&Locomotion>, Option<&WeaponHold>, Has<Dying>, &InheritedVisibility, Option<&crate::furniture::OnFurniture>, Option<&crate::outfits::Dressed>), (With<crate::limbs::ActorRig>, Without<Puppet>)>,
    fighters: Res<Fighters>,
    mut attacks: MessageReader<PlayAttack>,
    (mut told, mut sent_at): (Local<usize>, Local<HashMap<u32, [f32; 3]>>),
    (mut sim, actor_world, time): (Option<ResMut<nvf_play::sim::SimState>>, Option<Res<ActorWorld>>, Res<Time>),
    (residents, streamer, world, mut script): (Option<Res<Residents>>, Res<Streamer>, Option<Res<WorldData>>, Option<ResMut<nvf_play::script::ScriptState>>),
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
        coop.repl.host_npcs.clear();
        return;
    }
    // The other players' actors are drawn as puppets here (the puppet only says so once
    // it is first posed, `act`): a guest's own stand-in, sent as an NPC, stood frozen where
    // it was first drawn.
    let players: HashSet<u32> = coop.peers.values().filter_map(|p| p.actor).map(|r| r.0).collect();
    // The residents simulated round a guest away from the host and not drawn (`areas`).
    let undrawn = match (residents.as_deref(), world.as_deref()) {
        (Some(res), Some(world)) => res.simulated_undrawn(world, &streamer),
        _ => Vec::new(),
    };
    // Hostile to the players in this game: fighting one or ready to attack one, as this
    // game's compass judges it for its own player. A guest's game runs none of the
    // host's scripts (StartCombat, SetEnemy, a faction's turn) and has no fights of its
    // own, so its compass shows what the host's world says (Boulder City: the Khans
    // turned on the host and showed 0 hostile on the guest's compass, laptop 2026-11-09).
    let mut hostile: HashSet<u32> = HashSet::new();
    // The weapon a weapon use (a Use Weapon package, the UseWeapon function) has it fire:
    // the combat AI's pose has none for it, so a guest drew its shots as punches with no
    // gun in its hands (Boone's sniper shot at Novac, Novac runs on 9e66, 2026-11-09).
    let mut using: HashMap<u32, FormId> = HashMap::new();
    if let (Some(rt), Some(sim)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim.as_deref_mut()) {
        let refs: Vec<u32> = actors.iter().map(|(v, ..)| v.0).filter(|r| !is_puppet(*r) && !players.contains(r)).chain(undrawn.iter().map(|u| u.0)).collect();
        let others: Vec<FormId> = players.iter().map(|&p| FormId(p)).collect();
        nvf_play::script::with_world(rt, sim, |rt| {
            let g = &mut rt.game;
            for r in refs {
                let id = FormId(r);
                if g.is_dead(id) {
                    continue;
                }
                let fighting = g.awareness.get(id).and_then(|a| a.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF || others.contains(&t));
                if fighting || g.would_attack(id, FormId::PLAYER_REF) {
                    hostile.insert(r);
                }
                if let Some(w) = g.active_weapon_use(id).and_then(|u| u.weapon) {
                    using.insert(r, w);
                }
            }
        });
    }
    // What decides each one's look: its base, and the level and rolls its leveled picks
    // were drawn with ([`follow_looks`]).
    let rolls = actor_world.as_deref().and_then(|w| w.rolls.lock().ok().map(|r| r.clone())).unwrap_or_default();
    let mut npcs = Vec::new();
    let mut wear: Vec<(u32, Vec<u32>)> = Vec::new();
    // Knocked down and lying here: the guests' copies lie too (queue item 104).
    let knocked = |r: u32| script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.knockout(FormId(r)).is_some_and(|k| k.lying()));
    // A knocked-down body lies and gets up as a `Dying` too (`knockouts.rs`): only the
    // dead are sent as dead, or a guest's copy died with it and was raised again when it
    // stood up, without its get-up.
    let out_cold = |r: u32| script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.knockout(FormId(r)).is_some() && !rt.game.is_dead(FormId(r)));
    for (v, t, lo, hold, dying, seen, on, dressed) in &actors {
        // The dead too, out of sight or not: a guest lays its own body down.
        if (!seen.get() && !dying) || is_puppet(v.0) || players.contains(&v.0) || !feet_placed(t.translation.to_array()) {
            continue;
        }
        let r = v.0;
        let pitch = fighters.pitch(r);
        let mut flags = 0;
        flags |= if lo.is_some_and(|l| l.velocity[0].hypot(l.velocity[1]) > 1.0) { npc_flag::MOVING } else { 0 };
        flags |= if hold.is_some_and(|h| h.drawn) { npc_flag::DRAWN } else { 0 };
        flags |= if lo.is_some_and(|l| l.sneaking) { npc_flag::SNEAKING } else { 0 };
        flags |= if dying && !out_cold(r) { npc_flag::DEAD } else { 0 };
        flags |= if knocked(r) { npc_flag::KNOCKED } else { 0 };
        flags |= if pitch.is_some() { npc_flag::AIMING } else { 0 };
        flags |= if fighters.sights(r) { npc_flag::SIGHTS } else { 0 };
        // Getting off, or asked to while still getting on, is sent as off: a guest starts
        // its own getting-off clip with the host's, not once the host's has ended (then it
        // stood up 3.4 s late and was put 187 units on where the host had walked it
        // meanwhile, armwrestle-c's desktop run 2026-11-12; the Dead Horses off their
        // push-up spots got off 8 s late and were put 82-165 units on, laptop
        // deadhorse-spears b015).
        // Drawn but not seated yet while it is put on at once: sent as on (item 204d).
        let (furniture, seated) = sent_furniture(on.map(|f| (f.furniture(), f.seated(), f.leaving())), residents.as_deref().and_then(|res| res.being_put_on(r)));
        flags |= if seated { npc_flag::SEATED } else { 0 };
        // Drawn here further on than last frame than any run goes (item 187 (a): a guest
        // put the Dead Horses 153-160 units on with no pause in these states).
        if let Some(was) = sent_at.insert(r, t.translation.to_array()) {
            let far = (t.translation.x - was[0]).hypot(t.translation.y - was[1]);
            if far > NPC_JUMP {
                info!("co-op: {r:08X} drawn here {far:.0} units on from last frame (sent so){}", if furniture != 0 { ", on furniture" } else { "" });
            }
        }
        let level = actor_world.as_deref().map_or(fnv_sim::zones::LevelAt::plain(0), |w| w.level(r));
        npcs.push(NpcState {
            reference: r,
            feet: t.translation.to_array(),
            heading: crate::nav::tr_heading(t),
            pitch: pitch.unwrap_or(0.0),
            flags,
            // Its fighting pose's weapon, else its weapon use's, else the one drawn on it.
            weapon: fighters.weapon(r).or_else(|| using.get(&r).copied()).or_else(|| dressed.and_then(|d| d.weapon).filter(|_| hold.is_some_and(|h| h.drawn))).map_or(0, |w| w.0),
            attacks: coop.npc_attacks.get(&r).copied().unwrap_or(0),
            furniture,
            base: sim.as_deref().and_then(|s| s.base_of(r)).map_or(0, |b| b.0),
            level: level.level,
            all_levels: level.all_levels,
            rolls: rolls.get(&r).copied().unwrap_or(0).min(127) as u8,
            hostile: hostile.contains(&r),
            // Where its AI has it first: a script's MoveTo of a resident moves the walker,
            // and the reference's cell can still name where it was (Jeannie May, moved
            // out of her house at Novac, was sent as inside it and hidden on the guest).
            space: residents
                .as_deref()
                .and_then(|res| res.walker_of(r))
                .map(|(space, ..)| match space {
                    fnv_nav::Space::Interior(c) | fnv_nav::Space::World(c) => c,
                })
                .or_else(|| sim.as_deref().map(|s| space_of_ref(&s.world, FormId(r))))
                .unwrap_or(0),
        });
        // What it is drawn wearing and holding: each game rolls carried items from its
        // own random numbers (item 71), so the guests dress their copies in this.
        if let Some(d) = dressed {
            wear.push((r, d.worn.iter().map(|i| i.0).chain(d.weapon.map(|w| w.0)).collect()));
        }
    }
    // The residents the host simulates in a guest's surroundings away from it and does
    // not draw (`areas`): where its AI has them (item 109).
    {
        let game = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
        for (r, base, space, feet, heading, walking) in undrawn {
            let dead = game.is_some_and(|g| g.is_dead(FormId(r)));
            let level = actor_world.as_deref().map_or(fnv_sim::zones::LevelAt::plain(0), |w| w.level(r));
            let mut flags = if walking { npc_flag::MOVING } else { 0 };
            flags |= if dead { npc_flag::DEAD } else { 0 };
            flags |= if knocked(r) { npc_flag::KNOCKED } else { 0 };
            flags |= if fighters.weapon(r).is_some() && fighters.pitch(r).is_some() { npc_flag::AIMING } else { 0 };
            // One fighting a guest away from the host (`far_combat`): its weapon out.
            let far = coop.far.fighters.get(&r);
            flags |= if far.is_some_and(|f| f.weapon.is_some()) { npc_flag::AIMING } else { 0 };
            // On the furniture its AI has it on: the crucified at Nipton stay on their
            // crosses on a guest while the host is in the general store (item 204: sent
            // as none, they were taken off and put back on at once when it came out).
            let furniture = residents.as_deref().and_then(|res| res.furniture_of(r)).filter(|_| !dead);
            flags |= if furniture.is_some() { npc_flag::SEATED } else { 0 };
            npcs.push(NpcState {
                reference: r,
                feet,
                heading,
                pitch: 0.0,
                flags,
                weapon: fighters.weapon(r).or(far.and_then(|f| f.weapon)).map_or(0, |w| w.0),
                attacks: coop.npc_attacks.get(&r).copied().unwrap_or(0),
                furniture: furniture.unwrap_or(0),
                base: sim.as_deref().and_then(|s| s.base_of(r)).map_or(base, |b| b.0),
                level: level.level,
                all_levels: level.all_levels,
                rolls: rolls.get(&r).copied().unwrap_or(0).min(127) as u8,
                hostile: hostile.contains(&r),
                space: match space {
                    fnv_nav::Space::Interior(c) | fnv_nav::Space::World(c) => c,
                },
            });
        }
    }
    // One the host stopped drawing (gone through a load door, out of the loaded cells)
    // goes on being sent for a while from where its AI has it, so a guest stops drawing
    // it where it was last seen (item 179: Benny went through the Tops' elevator door on
    // the host and stood at the elevator on the guest for good).
    let now_secs = time.elapsed_secs();
    let sending: HashSet<u32> = npcs.iter().map(|n| n.reference).collect();
    coop.sent_npcs.retain(|r, since| {
        if sending.contains(r) {
            if since.take().is_some() {
                info!("co-op: {r:08X} drawn here again: its own state goes to the guests");
            }
            return true;
        }
        if since.is_none() {
            info!("co-op: {r:08X} not drawn here: sent from where its AI has it for {NPC_LINGER} s");
        }
        now_secs - *since.get_or_insert(now_secs) < NPC_LINGER
    });
    if let Some(res) = residents.as_deref() {
        for (&r, _) in coop.sent_npcs.iter().filter(|(r, _)| !sending.contains(r)) {
            let Some((space, feet, heading)) = res.walker_of(r) else { continue };
            let dead = script.as_deref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| rt.game.is_dead(FormId(r)));
            let level = actor_world.as_deref().map_or(fnv_sim::zones::LevelAt::plain(0), |w| w.level(r));
            // Its furniture as its AI has it, as for the undrawn above (item 204).
            let furniture = res.furniture_of(r).filter(|_| !dead);
            npcs.push(NpcState {
                reference: r,
                feet,
                heading,
                pitch: 0.0,
                flags: if dead { npc_flag::DEAD } else if furniture.is_some() { npc_flag::SEATED } else { 0 },
                weapon: 0,
                attacks: coop.npc_attacks.get(&r).copied().unwrap_or(0),
                furniture: furniture.unwrap_or(0),
                base: sim.as_deref().and_then(|s| s.base_of(r)).map_or(0, |b| b.0),
                level: level.level,
                all_levels: level.all_levels,
                rolls: rolls.get(&r).copied().unwrap_or(0).min(127) as u8,
                hostile: false,
                space: match space {
                    fnv_nav::Space::Interior(c) | fnv_nav::Space::World(c) => c,
                },
            });
        }
    }
    for r in sending {
        coop.sent_npcs.insert(r, None);
    }
    // The replication build dresses the guests' NPCs by the change log's Look (the
    // host's worn list, `replication`); two senders could disagree and the NPC would
    // flip between them, so this one stays quiet here.
    drop(wear);
    link.set_wear(Vec::new());
    if npcs.len().abs_diff(*told) >= 5 || (*told == 0) != npcs.is_empty() {
        *told = npcs.len();
        info!("co-op: sending the guests {} NPCs", npcs.len());
    }
    // The replication build sends them on its compact stream (`replication`).
    coop.repl.host_npcs = npcs;
}

/// A guest's copies of the host's NPCs, made and drawn as the host's game made and draws
/// them (module notes): an actor the host's game created (a dynamic reference: Freeside's
/// thugs, PlaceAtMe'd by a script) is made here under the host's number, of the host's
/// base; and each NPC's leveled picks are read at the host's level with the host's rolls
/// (the picks are seeded by the reference, so the same number, level and rolls pick the
/// same: `fnv_actor::resolve`), and drawn again when that changes what is drawn. Before,
/// a guest rolled its own picks at its own player's level (queue 2026-11-07 item 28,
/// NPCs that looked different) and never drew the host's created actors (item 38).
/// Each is also dressed in what the host's game draws it wearing and holding
/// (`net::Msg::Wear`, `fnv_sim::World::dress_as`): carried items, outfits among them, are
/// rolled from each game's own random numbers, not seeded by the reference, so the same
/// NPC came out in another suit even at the same level (item 71).
#[allow(clippy::type_complexity)]
fn follow_looks(
    mut commands: Commands,
    mut coop: ResMut<Coop>,
    (mut script, mut sim, actor_world, mut residents): (Option<ResMut<nvf_play::script::ScriptState>>, Option<ResMut<nvf_play::sim::SimState>>, Option<Res<ActorWorld>>, Option<ResMut<Residents>>),
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    solo: Option<Res<nvf_play::save::SoloBase>>,
) {
    let coop = &mut *coop;
    let guest = coop.link.as_ref().is_some_and(|l| !l.is_host() && l.id().is_some()) && !at_main_menu(main_menu.as_deref()) && solo.is_some();
    if !guest {
        coop.looks_taken.clear();
        coop.numbered = false;
        coop.wear_heard.clear();
        coop.wear_taken.clear();
        return;
    }
    let (Some(script), Some(sim), Some(aw)) = (script.as_deref_mut(), sim.as_deref_mut(), actor_world.as_deref()) else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    if !coop.numbered {
        sim.world.number_created_from(GUEST_CREATED);
        coop.numbered = true;
        info!("co-op: this game's own created references are numbered from {GUEST_CREATED:#010X}, the host's keep theirs");
    }
    let newest: Vec<NpcState> = coop.npcs.values().filter_map(|t| t.samples.back().map(|&(_, s)| s)).collect();
    let cell = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.cell);
    for s in newest {
        let r = FormId(s.reference);
        let key = (s.level, s.all_levels, s.rolls);
        if s.base == 0 || coop.looks_taken.get(&r.0) == Some(&key) {
            continue;
        }
        let name = rt.game.data.name(FormId(s.base));
        if r.is_dynamic() {
            match sim.world.peek(r).map(|st| st.base) {
                None => {
                    let made = nvf_play::script::with_world(rt, sim, |rt| rt.game.create_as(r, FormId(s.base), cell, s.feet, [0.0, 0.0, s.heading], None));
                    match made {
                        Ok(_) => info!("co-op: {name} {r} made here as the host's game made it"),
                        Err(e) => warn!("co-op: the host's {name} {r} cannot be made here: {e}"),
                    }
                }
                Some(b) if b.0 != s.base => {
                    warn!("co-op: the host's {r} is {name}, this game's own {r} is {}: it is not drawn here", rt.game.data.name(b));
                    continue;
                }
                Some(_) => {}
            }
        }
        // Taken only once it is known here, so one heard before is taken again.
        let Some(base) = sim.base_of(r.0) else { continue };
        coop.looks_taken.insert(r.0, key);
        let drawn = |aw: &ActorWorld| aw.resolve(base.0, r.0).ok().map(|a| (a.npc, a.female, a.race, a.worn));
        let before = drawn(aw);
        let mut changed = false;
        let level = fnv_sim::zones::LevelAt { level: s.level, all_levels: s.all_levels };
        if s.level != 0 && aw.level(r.0) != level {
            // A placed actor's level is the world's (`zones::sync_levels` copies it); a
            // created one is drawn at the display level in both games.
            if !r.is_dynamic() {
                sim.world.set_level(r, level);
            }
            if let Ok(mut l) = aw.levels.lock() {
                l.insert(r.0, level);
            }
            changed = true;
        }
        let rolls = u32::from(s.rolls);
        if aw.rolls.lock().map_or(0, |m| m.get(&r.0).copied().unwrap_or(0)) != rolls {
            if let Ok(mut m) = aw.rolls.lock() {
                m.insert(r.0, rolls);
            }
            rt.game.extra.entry(r).or_default().rolls = rolls;
            changed = true;
        }
        let after = drawn(aw);
        if changed && after != before {
            let again = residents.as_deref_mut().is_some_and(|res| res.redraw(&mut commands, r.0));
            let what = after.as_ref().map_or_else(|| "nothing".to_string(), |a| rt.game.data.name(a.0));
            info!("co-op: {name} {r} takes the host's leveled pick ({what}, level {}, rolls {rolls}){}", s.level, if again { ": drawn again" } else { "" });
        }
    }
    // Dressed as the host's game draws it (item 71): the outfits follow the inventory,
    // so it is drawn again in it (`outfits::follow_inventory`).
    let heard: Vec<(u32, Vec<u32>)> = coop.wear_heard.iter().filter(|(r, w)| coop.wear_taken.get(*r) != Some(*w)).map(|(r, w)| (*r, w.clone())).collect();
    for (r, items) in heard {
        let id = FormId(r);
        // An actor the host's game made, not made here yet: later.
        if sim.world.peek(id).is_none() && sim.world.placed.get(id).is_none() {
            continue;
        }
        let ids: Vec<FormId> = items.iter().map(|&i| FormId(i)).collect();
        match sim.world.dress_as(id, &ids) {
            Ok(true) => {
                let worn: Vec<String> = ids.iter().map(|&i| rt.game.data.name(i)).collect();
                info!("co-op: {} {id} dressed as the host's game has it: {}", rt.game.data.name(id), worn.join(", "));
            }
            Ok(false) => {}
            Err(e) => warn!("co-op: {id} cannot be dressed as the host's game has it: {e}"),
        }
        coop.wear_taken.insert(r, items);
    }
}

/// Whether a joining guest's own world logic is held: from the host's welcome
/// (`joining`, `nvf_play::save::KeepSolo`) until the game is first flagged a guest
/// (`seen_guest`; [`follow_npcs`] then turns it off itself), while the link is a guest's.
fn holds_joining_world(joining: bool, seen_guest: bool, link_guest: bool) -> bool {
    joining && !seen_guest && link_guest
}

/// A joining guest runs none of its own world logic from the host's welcome: it is
/// flagged a guest ([`follow_npcs`]) only once its own game was kept
/// (`nvf_play::save::SoloBase`), after the first frame past the loading screen, and that
/// frame ran its own quests. s182 (desktop two-window run, 2026-11-13): Dead Money's
/// NVDLC01MQ00 moved the joining guest out of the Sierra Madre bunker to the Mojave there
/// (its own bTeleportOut was still 0), so the host's later move to the bunker's trigger
/// left it "too far". Its triggers are held the same way (`script_trigger::held_for_guest`).
fn hold_joining_world(mut coop: ResMut<Coop>, joining: Option<Res<nvf_play::save::KeepSolo>>, mut script: Option<ResMut<nvf_play::script::ScriptState>>, mut seen_guest: Local<bool>) {
    let Some(g) = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game) else { return };
    *seen_guest |= g.coop_guest;
    let hold = holds_joining_world(joining.is_some(), *seen_guest, coop.is_guest());
    if hold {
        if !coop.joining_held {
            info!("replication: joining: this game's own world logic is held until the host's world reaches it");
        }
        coop.joining_held = true;
        g.world_logic_off = true;
        g.story_plugins = STORY_PLUGINS.get().cloned().unwrap_or_else(|| vec![0]);
    } else if coop.joining_held && !g.coop_guest {
        info!("replication: joining ended before the host's world reached this game: its own world logic runs again");
        coop.joining_held = false;
        g.world_logic_off = false;
    }
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
    (mut attacks, mut hits, mut fired, combat): (MessageWriter<PlayAttack>, MessageWriter<ActorHit>, MessageWriter<nvf_play::combat::ShotFired>, Option<Res<nvf_play::combat::Combat>>),
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    mut told: Local<usize>,
    (sfx, listener, mut sounds, mut sim, loc): (Option<Res<Sfx>>, Option<Res<Player>>, MessageWriter<PlaySound>, Option<ResMut<nvf_play::sim::SimState>>, Option<Res<Location>>),
    solo: Option<Res<nvf_play::save::SoloBase>>,
    mut knocked: Local<HashSet<u32>>,
    mut streamed_since: Local<Option<f32>>,
    arm: Option<Res<nvf_play::armwrestle_coop::ArmBodies>>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    // The host's NPCs reach this game once it is kept as the guest's own (module notes).
    let guest = coop.link.as_ref().is_some_and(|l| !l.is_host() && l.id().is_some()) && !at_main_menu(main_menu.as_deref()) && solo.is_some();
    // A guest on its way out (the host ended the lobby, or it chose the main menu) still
    // stands in the host's world while it autosaves and until the main menu is up: it
    // stays a guest's world, running none of its own world logic, scripts or triggers.
    // Before, the link's end made it its own world at once, and its player and the host's
    // ambushers standing in a trigger volume ran the trigger's quest script there (Tops
    // ambush: VMQTops objective 20 set on the guest's game, two-window check 2026-11-09).
    let parting = (coop.ended || !matches!(coop.leaving, leave::Leaving::No) || coop.parting) && !at_main_menu(main_menu.as_deref());
    if !guest && parting && coop.ai_was_off.is_some() {
        return;
    }
    let game = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game);
    if !guest {
        // Left the lobby: this game's world is its own again.
        if let Some(was) = coop.ai_was_off.take() {
            coop.parting = false;
            stats.relay = None;
            if let Some(r) = residents.as_deref_mut() {
                r.remote = None;
            }
            if let Some(g) = game {
                g.toggles.actors_ai_off = was;
                g.coop_guest = false;
                g.world_logic_off = false;
                g.coop_host_quest = None;
                g.coop_host_story = Default::default();
                g.coop_host_name = None;
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
    let Some(g) = game else { return };
    // Where the host's story put the NPCs it draws here, they are here too.
    let here = loc.as_deref().map(place_of);
    let settled = coop.host_place.filter(|(p, _)| Some(*p) == here).map(|(_, since)| since);
    // The host's disables come before its NPC samples (item 186: the Strip crowd the
    // host's scene disabled was enabled here again from the samples sent just before, and
    // 6 of 7 stayed standing): an NPC the host disabled is forgotten here, so the disable
    // below reaches it, and no older sample shows it again until the host enables it.
    for &(r, on) in &coop.presence.fresh {
        if on {
            coop.host_disabled.remove(&r);
        } else {
            coop.host_disabled.insert(r);
            if coop.npcs.remove(&r).is_some() {
                info!("co-op: the host disabled {}: not followed here any more", g.data.name(FormId(r)));
            }
        }
    }
    if let (Some(sim), Some(since)) = (sim.as_deref_mut(), settled) {
        for r in place_like_host(&mut sim.world, g, &coop.npcs, &coop.host_disabled, since + HOST_SETTLE, now) {
            coop.hidden.remove(&r);
            coop.unheard.remove(&r.0);
        }
    }
    // What the host's scripts enabled or disabled, all over its world (module notes); the
    // NPCs the host draws and the actors hidden here are left to the rest.
    if let Some(sim) = sim.as_deref_mut() {
        let (npcs, hidden) = (&coop.npcs, &coop.hidden);
        coop.repl.enabled_heard.retain(|_, at| now - *at < STREAM_WINS);
        let streamed = &coop.repl.enabled_heard;
        let skip = |r: FormId| npcs.contains_key(&r.0) || hidden.contains(&r) || streamed.contains_key(&r.0);
        let fresh = std::mem::take(&mut coop.presence.fresh);
        let n = follow_presence(&mut sim.world, Flag::Enabled, &fresh, &skip, &mut coop.presence_was);
        if n > 0 {
            info!("co-op: {n} references enabled or disabled as in the host's world");
        }
        if let Some(host) = coop.presence.whole.take() {
            let n = undo_own_presence(&mut sim.world, Flag::Enabled, &host, &skip, &mut coop.presence_was);
            if n > 0 {
                info!("co-op: {n} references this game alone enabled or disabled take the plugins' flags again");
            }
        }
    }
    // This game's own actors near the host that the host's world does not have: hidden
    // (module notes). Only once the host's NPCs come, and the host has been here a while.
    // Away from the host too (item 157: a guest's own save kept the Honest Hearts caravan
    // alive at Northern Passage, dead in Zion in the host's world): round this player,
    // once the host has streamed its actors of this place near here for a while (the
    // undrawn residents round away guests, `share_npcs`), so the host simulates this area.
    let mine = listener.as_deref().map(|p| p.feet);
    let my_space = here.map_or(0, |p| match p {
        Place::Interior(c) | Place::Exterior(c) => c,
    });
    let streamed_here = mine.is_some_and(|me| {
        my_space != 0 && coop.npcs.values().filter_map(|t| t.samples.back()).any(|(_, n)| n.space == my_space && (n.feet[0] - me[0]).hypot(n.feet[1] - me[1]) <= HIDE_REACH)
    });
    let away = settled.is_none() && streamed_here;
    if away {
        streamed_since.get_or_insert(now);
    } else {
        *streamed_since = None;
    }
    let centre = match (settled.filter(|s| now - s >= HIDE_AFTER), coop.host, *streamed_since, mine) {
        (Some(_), Some(host), _, _) => Some(host.feet),
        (None, _, Some(since), Some(me)) if now - since >= HIDE_AFTER => Some(me),
        _ => None,
    };
    match (sim.as_deref_mut(), centre) {
        (Some(sim), Some(centre)) if !coop.npcs.is_empty() => {
            let drawn: Vec<(u32, [f32; 3])> = actors.iter().map(|(_, v, _, _, _, _, t)| (v.0, t.translation.to_array())).collect();
            let heard: HashSet<u32> = coop.npcs.keys().copied().collect();
            let missing = not_in_hosts_world(&drawn, &heard, centre, HIDE_REACH);
            coop.unheard.retain(|r, _| missing.contains(r));
            for r in missing {
                let since = *coop.unheard.entry(r).or_insert(now);
                if now - since < HIDE_AFTER || !coop.hidden.insert(FormId(r)) {
                    continue;
                }
                info!("co-op: {} is not in the host's world here: hidden{}", g.data.name(FormId(r)), if away { " (away from the host)" } else { "" });
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
    // Replication (phase 4): the host runs the world's logic, this game draws it (held
    // already while joining, [`hold_joining_world`]).
    if !g.world_logic_off || std::mem::take(&mut coop.joining_held) {
        g.world_logic_off = true;
        g.story_plugins = STORY_PLUGINS.get().cloned().unwrap_or_else(|| vec![0]);
        info!("replication: this guest runs no world logic (story plugins {:?}); the host's world is drawn", g.story_plugins);
    }
    // Test aid `NVF_COOP_TEST_CONSOLE_STORY=1`: the console's lines count as the host's
    // story (`fnv_game::coop_story`), to check what a guest's save keeps (queue item 81).
    g.coop_story.console_counts = std::env::var_os("NVF_COOP_TEST_CONSOLE_STORY").is_some();
    let host_name = coop.peers.get(&0).map(|p| p.name.clone()).filter(|n| !n.is_empty());
    if g.coop_host_name != host_name {
        g.coop_host_name = host_name;
    }
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
    let dt = time.delta_secs();
    let poses: HashMap<u32, RemotePose> = coop
        .npcs
        .iter_mut()
        .filter_map(|(&r, t)| {
            // Its furniture as the host has it now (a chair, a bed, a cross: module notes).
            // Not a dead one's: Chet, killed on the host, sat down on his chair again on a
            // guest (desktop run for item 195).
            let heard = t.samples.back().map(|(_, s)| s).filter(|s| s.furniture != 0 && !s.has(npc_flag::DEAD)).map(|s| (arm.as_deref().map_or(s.furniture, |a| a.npc_furniture(r, s.furniture)), s.has(npc_flag::SEATED)));
            // A new host's first states (after a load) keep it on the furniture it is on
            // here until it says so too (item 204d).
            let dead = t.samples.back().is_some_and(|(_, s)| s.has(npc_flag::DEAD));
            let young = !dead && t.first_heard.is_some_and(|f| now - f < NPC_FIRST_HOLD);
            let furniture = furniture_here(heard, young, residents.as_deref().and_then(|res| res.furniture_of(r)));
            let space = t.samples.back().map_or(0, |(_, s)| s.space);
            // Faster than it runs (with room for a run's speed-ups) is a jump; Kimball's
            // and the Legion sniper's enables and first states at the Hoover Dam were
            // blended from where they were placed at 1165-1399 units/s (laptop 1110d).
            let top = residents.as_deref().and_then(|res| res.top_speed(r)).map_or(NPC_JUMP_SPEED, |run| (run * NPC_RUN_ROOM).min(NPC_JUMP_SPEED));
            t.delay = eased_delay(t.delay, t.spacing(), dt);
            let pose = t.pose(now - t.delay, top);
            if let (true, Some((feet, _, moving))) = (traced(r), pose) {
                info!("co-op trace: {r:08X} drawn at {now:.3} for {:.3} (top {top:.0}): ({:.0}, {:.0}, {:.0}) moving {moving}", now - t.delay, feet[0], feet[1], feet[2]);
            }
            pose.map(|(feet, heading, moving)| (r, RemotePose { feet, heading, moving, furniture, space }))
        })
        .collect();
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
            // The host's NPC sets off when its samples say so, with no warning here: its
            // walk starts at once, fading from a standing fade, as a creature's does
            // (item 187: Vulpes at Nipton slid at 69 units/s on Idle as his walk-up
            // began on the guest) [design].
            if !lo.blend.set_off_at_once {
                lo.blend.set_off_at_once = true;
            }
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
            // And its muzzle flash at its gun's muzzle (`shot_fx`; item 176: Boone's shot
            // at Jeannie May showed nothing on the guest). Only the flash: no shot is
            // fired here, the host's hit comes as the host deals it.
            let projectile = weapon.filter(|_| !melee).and_then(|w| combat.as_deref()?.forms.chain(w, None)?.projectile);
            if let (Some(w), Some(projectile)) = (weapon, projectile) {
                let eye = [s.feet[0], s.feet[1], s.feet[2] + 100.0];
                let (sy, cy) = s.heading.sin_cos();
                let (sp, cp) = s.pitch.sin_cos();
                fired.write(nvf_play::combat::ShotFired { projectile, weapon: w, shooter: FormId(r), at: Some(eye), dir: Some([sy * cp, cy * cp, -sp]), mods: 0 });
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
    // Knockdowns (queue item 104): lying here while the host's lies, held down (this
    // game's own clock would get it up after its time), and getting up when the host's
    // does; `knockouts.rs` draws the fall and the get-up from the game's knockout.
    knocked.retain(|r| coop.npcs.contains_key(r));
    for (&r, t) in &coop.npcs {
        let Some(&(_, s)) = t.samples.back() else { continue };
        let id = FormId(r);
        if g.is_dead(id) {
            knocked.remove(&r);
            continue;
        }
        let lying = g.knockout(id).is_some_and(|k| k.lying());
        if s.has(npc_flag::KNOCKED) {
            if !lying {
                let at = g.time;
                let e = g.extra.entry(id).or_default();
                e.knocked_out = Some(fnv_game::unconscious::KnockOut { cause: fnv_game::unconscious::Cause::KnockedDown, at, facing_up: None, held: false, rising: None, pushed: false, shove: None });
                e.unconscious = true;
                knocked.insert(r);
                info!("co-op: {} is knocked down in the host's world: down here too", g.data.name(id));
            }
        } else if knocked.remove(&r) {
            let at = g.time;
            let e = g.extra.entry(id).or_default();
            e.unconscious = false;
            if let Some(k) = e.knocked_out.as_mut().filter(|k| k.lying()) {
                k.rising = Some(at);
                info!("co-op: {} gets up in the host's world: up here too", g.data.name(id));
            }
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
/// A reference's enabled flag the replication stream set is the host's newest for this
/// long (seconds): the presence list, made up to [`PRESENCE_REMAKE`] before and paged at
/// `net::PRESENCE_EVERY`, does not change it in that time (item 181).
const STREAM_WINS: f32 = 3.0 * PRESENCE_REMAKE;

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
    /// A whole list has come since this was made (the syncing screen waits for one).
    complete: bool,
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
            self.complete = true;
            self.whole = Some(self.refs.clone());
        }
    }
}

/// The flag a host's list carries (`net::LIST_ENABLED`, `net::LIST_OPEN`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Flag {
    /// A reference's own enabled flag (its enable parent's is resolved apart).
    Enabled,
    /// A door's open flag (a door that opens in place: a load door has none).
    Open,
}

impl Flag {
    /// A placed reference's flag as its plugin gives it; `None` for what takes no part
    /// in the list (the player, references made at run time, unknown ones, load doors).
    fn authored(self, world: &fnv_sim::World, r: FormId) -> Option<bool> {
        if r == FormId::PLAYER_REF || r.0 >= 0xFF00_0000 {
            return None;
        }
        let p = world.placed.get(r)?;
        match self {
            Flag::Enabled => Some(!p.initially_disabled()),
            Flag::Open => p.teleport.is_none().then_some(p.open_by_default),
        }
    }

    fn of(self, st: &fnv_sim::world::RefState) -> bool {
        match self {
            Flag::Enabled => st.enabled(),
            Flag::Open => st.open,
        }
    }

    fn set(self, world: &mut fnv_sim::World, r: FormId, on: bool) {
        match self {
            Flag::Enabled => {
                let _ = world.set_enabled(r, on);
            }
            Flag::Open => {
                if let Ok(st) = world.get(r) {
                    st.open = on;
                }
            }
        }
    }
}

/// A host's list (module notes): each placed reference whose `flag` differs from its
/// plugin's, with the flag, in FormID order.
fn presence_of(world: &fnv_sim::World, flag: Flag) -> Vec<(u32, bool)> {
    world.changed().filter(|(r, st)| flag.authored(world, *r).is_some_and(|a| a != flag.of(st))).map(|(r, st)| (r.0, flag.of(st))).collect()
}

/// A guest takes the host's flags (`refs`), except for what `skip` holds; what it changes
/// is kept in `was` as it was first. Returns how many it changed.
fn follow_presence(world: &mut fnv_sim::World, flag: Flag, refs: &[(u32, bool)], skip: &dyn Fn(FormId) -> bool, was: &mut HashMap<FormId, bool>) -> usize {
    let mut n = 0;
    for &(r, on) in refs {
        let r = FormId(r);
        if skip(r) || flag.authored(world, r).is_none() {
            continue;
        }
        let Ok(st) = world.get(r) else { continue };
        let now = flag.of(st);
        if now != on {
            was.entry(r).or_insert(now);
            flag.set(world, r, on);
            n += 1;
        }
    }
    n
}

/// A guest with the host's whole list (`host`): what its own game alone changed (not on
/// the list, not in `skip`) takes its plugin's flag again, kept in `was` as it was first.
/// Returns how many it changed.
fn undo_own_presence(world: &mut fnv_sim::World, flag: Flag, host: &HashSet<u32>, skip: &dyn Fn(FormId) -> bool, was: &mut HashMap<FormId, bool>) -> usize {
    let own: Vec<(FormId, bool)> = world
        .changed()
        .filter(|(r, _)| !host.contains(&r.0) && !skip(*r))
        .filter_map(|(r, st)| flag.authored(world, r).filter(|a| *a != flag.of(st)).map(|a| (r, a)))
        .collect();
    for &(r, authored) in &own {
        was.entry(r).or_insert(!authored);
        flag.set(world, r, authored);
    }
    own.len()
}

/// A host makes its doors' list again this often (seconds); the doors loaded here go at
/// once when they move.
const DOORS_REMAKE: f32 = 0.5;
/// A door this player used is its own this long (seconds): what this game does to it is
/// sent, and the host's list does not undo it before the host has heard.
const DOOR_HELD: f32 = 2.0;

/// A door's open flag here: its state's, or as its plugin places it.
fn door_open(world: &fnv_sim::World, r: FormId) -> Option<bool> {
    world.peek(r).map(|s| s.open).or_else(|| Flag::Open.authored(world, r))
}

/// Doors in co-op (module notes): what others opened or closed is done here; a door
/// loaded here that moved goes to everyone (a guest: only one its player used lately); the
/// host makes its list, a guest follows it.
fn share_doors(
    mut coop: ResMut<Coop>,
    time: Res<Time>,
    doors: Option<Res<nvf_play::doors::Doors>>,
    sim: Option<ResMut<nvf_play::sim::SimState>>,
    mut used: MessageReader<nvf_play::world_sounds::Activated>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let (Some(link), Some(mut sim)) = (coop.link.as_ref(), sim) else {
        used.clear();
        if coop.link.is_none() && !coop.doors_seen.is_empty() {
            coop.doors_seen.clear();
            coop.door_used.clear();
            coop.doors_in.clear();
            coop.doors_heard = HostPresence::default();
            coop.doors_was.clear();
            coop.doors_made = None;
        }
        return;
    };
    let is_host = link.is_host();
    for u in used.read() {
        coop.door_used.insert(u.reference, now);
    }
    coop.door_used.retain(|_, at| now - *at < DOOR_HELD);
    for (id, r, open) in coop.doors_in.drain(..) {
        let r = FormId(r);
        if door_open(&sim.world, r).is_some_and(|was| was != open) {
            if !is_host {
                coop.doors_was.entry(r).or_insert(!open);
            }
            Flag::Open.set(&mut sim.world, r, open);
            info!("co-op: {} {} door {:#010X}", if id == 0 { "the host".to_string() } else { label(id, &coop.peers.get(&id).map_or(String::new(), |p| p.name.clone())) }, if open { "opened" } else { "closed" }, r.0);
        }
        coop.doors_seen.insert(r.0, open);
    }
    if is_host {
        if coop.doors_made.is_none_or(|at| now - at >= DOORS_REMAKE) {
            coop.doors_made = Some(now);
            // The replication build's change log carries the open flags (`replication`).
        }
    } else {
        let used = &coop.door_used;
        let skip = |r: FormId| used.contains_key(&r.0);
        let fresh = std::mem::take(&mut coop.doors_heard.fresh);
        let mut n = follow_presence(&mut sim.world, Flag::Open, &fresh, &skip, &mut coop.doors_was);
        if let Some(host) = coop.doors_heard.whole.take() {
            n += undo_own_presence(&mut sim.world, Flag::Open, &host, &skip, &mut coop.doors_was);
        }
        if n > 0 {
            info!("co-op: {n} doors opened or closed as in the host's world");
        }
    }
    // The doors loaded here that moved: this game's own doing (what came from the others
    // is noted as seen above, and what the host's list did is not this player's).
    let Some(doors) = doors else { return };
    for id in doors.ids() {
        let Some(open) = door_open(&sim.world, FormId(id)) else { continue };
        let moved = coop.doors_seen.insert(id, open).is_some_and(|was| was != open);
        if moved && (is_host || coop.door_used.contains_key(&id)) {
            info!("co-op: this {} {} door {id:#010X}: everyone's does", if is_host { "host's world" } else { "player" }, if open { "opened" } else { "closed" });
            link.send_door(id, open);
        }
    }
}

/// Whether an actor's feet are a place: a body just drawn is at the origin until it is
/// put where it stands, and a sample of it then would put the guest's copy there (item
/// 201, 2026-11-14: GSPG05/06 revived "standing at (0, 0, 0)" on the guest, the host's
/// gangers enabled that frame) [hypothesis: no actor stands at the exact origin].
fn feet_placed(feet: [f32; 3]) -> bool {
    feet != [0.0; 3]
}

/// A guest in the host's place: each NPC the host draws, enabled here and put in this
/// cell when the host's story enabled it or moved it (module notes). Only states heard
/// after `after` and lately: the ones before are the place the host left (the host's NPCs
/// there are forgotten only after [`NPC_FORGET`]). Actors the host's scripts made (`FF`
/// references) are this game's own numbers here: left alone. One dead in this game but
/// alive in the host's stands up again (`Game::resurrect`). Returns those it enabled.
fn place_like_host(world: &mut fnv_sim::World, g: &mut fnv_game::Game, npcs: &HashMap<u32, NpcTrack>, host_disabled: &HashSet<u32>, after: f32, now: f32) -> Vec<FormId> {
    let mut enabled = Vec::new();
    let Some(cell) = world.peek(FormId::PLAYER_REF).and_then(|s| s.cell) else { return enabled };
    let player_space = world.placed.world_of(cell);
    for (&r, t) in npcs {
        let Some(&(at, s)) = t.samples.back() else { continue };
        if at < after || now - at > 1.0 || r >= 0xFF00_0000 || s.has(npc_flag::DEAD) || host_disabled.contains(&r) || !feet_placed(s.feet) {
            continue;
        }
        let id = FormId(r);
        let Ok(st) = world.get(id) else { continue };
        if st.deleted() {
            continue;
        }
        if g.is_dead(id) {
            let was = world.peek(id).map(|st| st.pos);
            info!(
                "co-op: {} is alive in the host's world: up again here (the host's sample: feet ({:.0}, {:.0}, {:.0}), space {:08X}; here {})",
                g.data.name(id),
                s.feet[0],
                s.feet[1],
                s.feet[2],
                s.space,
                was.map_or_else(|| "no state".to_string(), |p| format!("at ({:.0}, {:.0}, {:.0})", p[0], p[1], p[2]))
            );
            g.coop_revive(id);
            // It stands up where the host has it: its place here may be the corpse's own,
            // or none at all (item 201, 2026-11-14: GSPG05/06 0x00104C72/73, dead in the
            // guest's save, revived "standing at (0, 0, 0)"). In this player's space only;
            // another space is left to the rest below.
            let my_space = player_space.map_or(cell.0, |w| w.0);
            if s.space == 0 || s.space == my_space {
                if let Ok(st) = world.get(id) {
                    st.pos = s.feet;
                    st.rot = [0.0, 0.0, s.heading];
                }
            }
        }
        // Shown already, by its own flag or its enable parent's: left as it is. One
        // shown through its enable parent keeps its own flag as the host's does (the
        // parent's change brings it): setting the flag here made every digest find it
        // unlike the host's, which disabled it again and the next sample enabled it
        // (Bitter Springs raid, laptop 2026-11-09: VBSVexillariusREF and the wave-3
        // mongrels and legionaries, children of VBS raid markers, "sent again" every
        // 6-9 s).
        let shown = world.is_enabled(id).unwrap_or(false);
        let by_parent = world.placed.get(id).is_some_and(|p| p.enable_parent.is_some());
        let enable = !shown && !by_parent;
        let Ok(st_cell) = world.get(id).map(|st| st.cell) else { continue };
        // Where the host has it (`NpcState::space`, protocol 129): another interior than
        // this player's is that interior; this player's space (or none told) is the cell
        // here at its feet. One in another worldspace is left to the drawing
        // (`residents::Core::follow`). Putting every one of them in this player's cell
        // fought the drawing every frame: Easy Pete, gone into the Prospector Saloon on
        // the host, was put back in the street and taken in again 120 times a second
        // while the guest stood outside (desktop vspace run, 2026-11-10).
        let my_space = player_space.map_or(cell.0, |w| w.0);
        let target = if s.space == 0 || s.space == my_space {
            world.placed.exterior_cell_at(cell, s.feet).unwrap_or(cell)
        } else if world.placed.world_grid.contains_key(&FormId(s.space)) {
            if enable {
                let _ = world.set_enabled(id, true);
                enabled.push(id);
                info!("co-op: {id} enabled as in the host's world");
            }
            continue;
        } else {
            FormId(s.space)
        };
        // Elsewhere: another interior, or another worldspace, than where it belongs.
        let elsewhere = st_cell.is_none_or(|c| c != target && ((s.space != 0 && s.space != my_space) || player_space.is_none() || world.placed.world_of(c) != player_space));
        if elsewhere {
            let to = target;
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
/// except the one each keeps; the bodies despawned.
fn despawn_bodies(commands: &mut Commands, roots: &Query<(Entity, &RefVisual, &ChildOf)>, world_root: Entity, keep: &HashMap<u32, Option<Entity>>) -> HashSet<Entity> {
    let mut gone = HashSet::new();
    if keep.is_empty() {
        return gone;
    }
    for (e, v, parent) in roots {
        if parent.parent() != world_root {
            continue;
        }
        if let Some(k) = keep.get(&v.0).filter(|k| **k != Some(e)) {
            info!("co-op: a body of {:08X} taken away{}", v.0, if k.is_some() { " (a stray: not its puppet)" } else { "" });
            commands.entity(e).try_despawn();
            gone.insert(e);
        }
    }
    gone
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
        // A player's voice (its set's lines and its own grunts) goes to the others by the
        // voice table (`nvf_play::party_quips`), from the speaker's own game, once: a line
        // here with a player as speaker (the host's, or a guest's stand-in here) would be
        // heard twice or from the wrong body (item 106).
        .filter(|s| s.speaker != FormId::PLAYER_REF && player(s.speaker).is_none())
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
            farewell: false,
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
/// The other players swimming in their own games: their bodies here swim too
/// (`swimming.rs`, item 121).
fn swimmers(coop: Res<Coop>, mut swim: ResMut<crate::swimming::Swimmers>, mut was: Local<HashMap<u8, bool>>) {
    let mut now = std::collections::HashSet::new();
    for (&id, p) in coop.peers.iter() {
        let swims = !p.lost && p.samples.back().is_some_and(|(_, s)| s.more & nvf_coop::net::more_flag::SWIMMING != 0);
        if swims {
            now.extend(p.references(id));
        }
        if was.insert(id, swims).unwrap_or(false) != swims {
            info!("co-op: {} {} in its own game", label(id, &p.name), if swims { "swims" } else { "is out of the water" });
        }
    }
    if swim.0 != now {
        swim.0 = now;
    }
}

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
                let e = commands.spawn((Name::new(format!("Flashlight of player {}", id + 1)), crate::flashlight::remote_spot(), place)).id();
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
    (mut script, mut sim): (Option<ResMut<nvf_play::script::ScriptState>>, Option<ResMut<nvf_play::sim::SimState>>),
    player: Option<Res<Player>>,
    loc: Option<Res<Location>>,
    input: nvf_play::bindings::ControlInput,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    mut travel: MessageWriter<TravelRequest>,
    mut hits: MessageWriter<ActorHit>,
    mut cam: Option<ResMut<crate::death_cam::DeathCam>>,
    main_menu: Option<Res<nvf_play::main_menu::MainMenu>>,
    (target, mut screen, kit): (Res<nvf_play::activate::Target>, ResMut<nvf_play::coop_downed::DownedScreen>, Res<nvf_play::coop_social::ReviveKit>),
) {
    let coop = &mut *coop;
    let in_coop = coop.link.is_some();
    if let Some(c) = cam.as_deref_mut().filter(|c| c.downed_in_coop != in_coop) {
        c.downed_in_coop = in_coop;
    }
    // What the downed screen says this frame (set below while this player is down, or
    // revived while the black lifts).
    let mut shown: Option<nvf_play::coop_downed::Downed> = None;
    if cam.as_deref().is_some_and(|c| c.revived_hold()) {
        shown = coop.revived_by.clone().map(|by| nvf_play::coop_downed::Downed::Reviving { by });
    } else if cam.as_deref().is_none_or(|c| !c.down()) {
        coop.revived_by = None;
    }
    let (Some(rt), Some(sim), Some(player), Some(loc)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim.as_deref_mut(), player, loc) else {
        if screen.0 != shown {
            screen.0 = shown;
        }
        return;
    };
    // The co-op perks can be taken with another player in the session
    // (`fnv_game::coop_perks`).
    rt.game.coop_session = in_coop && !coop.peers.is_empty();
    if !in_coop || at_main_menu(main_menu.as_deref()) {
        coop.downed_at = None;
        if screen.0.is_some() {
            screen.0 = None;
        }
        return;
    }
    let now = time.elapsed_secs();
    downed_body(coop, &mut rt.game, sim, &player, &loc, &input, (&target, &kit), &mut corner, &mut travel, &mut hits, &mut shown, now);
    if screen.0 != shown {
        screen.0 = shown;
    }
}

/// [`downed`]'s work once the game runs in a session.
#[allow(clippy::too_many_arguments)]
fn downed_body(
    coop: &mut Coop,
    g: &mut fnv_game::Game<'static>,
    sim: &mut nvf_play::sim::SimState,
    player: &Player,
    loc: &Location,
    input: &nvf_play::bindings::ControlInput,
    (target, kit): (&nvf_play::activate::Target, &nvf_play::coop_social::ReviveKit),
    corner: &mut Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    travel: &mut MessageWriter<TravelRequest>,
    hits: &mut MessageWriter<ActorHit>,
    shown: &mut Option<nvf_play::coop_downed::Downed>,
    now: f32,
) {
    let here = place_of(loc);
    let mut say = |n: String| {
        info!("co-op: {n}");
        if let Some(c) = corner.as_mut() {
            c.push(n.into());
        }
    };
    // The others' puppets: down, they lie down as the dead do; up, they are drawn anew.
    for (&id, p) in coop.peers.iter_mut() {
        let Some(&(_, s)) = p.samples.back() else { continue };
        let down = s.has(flag::DOWN);
        // A player up is never dead here, whatever happened while it was down; except
        // while it lies after losing a duel to this player ([`duel_falls`] stands it up).
        if !down && !p.down && p.fell_at.is_none() && g.is_dead(FormId(p.reference(id))) {
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
            say(format!("{} is down: go to them and revive them", label(id, &p.name)));
        } else {
            // Revived: its body gets up from where it lies ([`revive`]); a body this game
            // cannot raise is drawn anew standing there.
            p.rising = Some(revive::Rising::Start { since: now });
            p.body = None;
            say(format!("{} is back up", label(id, &p.name)));
        }
    }
    let pressed = input.just_pressed(nvf_play::bindings::Control::Activate);
    // This player.
    if g.is_dead(FormId::PLAYER_REF) {
        let feet = player.feet;
        let alone = !coop.peers.iter().any(|(_, p)| {
            !p.lost && p.samples.back().is_some_and(|&(_, q)| !q.has(flag::DOWN) && q.place == here && (q.feet[0] - feet[0]).hypot(q.feet[1] - feet[1]) <= ALONE_NEAR)
        });
        if alone != coop.alone_down {
            coop.alone_down = alone;
            info!("co-op: down {}", if alone { "with no other player near: a guest goes back to its last save" } else { "with another player near enough to revive" });
        }
        let since = *coop.downed_at.get_or_insert_with(|| {
            info!("co-op: you are down: another player can revive you with a Stimpak; in {RESPAWN_AFTER:.0} seconds you can respawn");
            now
        });
        if now - since < RESPAWN_AFTER {
            *shown = Some(nvf_play::coop_downed::Downed::Waiting { seconds: (RESPAWN_AFTER - (now - since)).ceil().max(1.0) as u32 });
            return;
        }
        // Beside the nearest player up, else anyone (the host first).
        let dist = |q: &PlayerState| if q.place == here { (q.feet[0] - feet[0]).hypot(q.feet[1] - feet[1]) } else { f32::MAX / 2.0 };
        let target = coop
            .peers
            .iter()
            .filter_map(|(&id, p)| p.samples.back().map(|&(_, q)| (id, p, q)))
            .min_by(|a, b| (a.2.has(flag::DOWN) || a.1.lost, dist(&a.2), a.0).partial_cmp(&(b.2.has(flag::DOWN) || b.1.lost, dist(&b.2), b.0)).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(id, p, q)| (label(id, &p.name), q));
        if !coop.told_respawn {
            coop.told_respawn = true;
            info!("co-op: you can respawn{}", target.as_ref().map_or(String::new(), |(n, _)| format!(" by {n}")));
        }
        *shown = Some(nvf_play::coop_downed::Downed::Ready { by: target.as_ref().map(|(n, _)| n.clone()) });
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
            *shown = None;
        }
        return;
    }
    coop.downed_at = None;
    coop.told_respawn = false;
    coop.alone_down = false;
    // A downed player aimed at where the body lies: the crosshair's "Revive" rollover
    // (`nvf_play::coop_social::target`); Activate revives them with a Stimpak.
    let aimed = target.0.as_ref().filter(|t| matches!(t.action, nvf_play::activate::outcome::Action::Revive { .. })).map(|t| t.form_id);
    let near = aimed.and_then(|r| coop.peers.iter().find(|(&id, p)| p.down && !p.lost && p.reference(id) == r)).map(|(&id, p)| (id, label(id, &p.name)));
    let Some((id, name)) = near else {
        coop.revive_offer = None;
        return;
    };
    if coop.revive_offer != Some(id) {
        coop.revive_offer = Some(id);
        info!("co-op: aiming at {name}'s body: Revive");
    }
    if !pressed {
        return;
    }
    // What the revive uses: the crosshair's rollover named it (`ReviveKit`: a Stimpak,
    // or a drink with Quack Surgeon, `fnv_game::coop_perks`).
    let Some((item, item_name)) = kit.0.clone() else {
        say(format!("You need a Stimpak to revive {name}"));
        return;
    };
    let have = sim.world.peek(FormId::PLAYER_REF).and_then(|s| s.inventory.as_ref()).map_or(0, |i| i.count(item));
    if have < 1 {
        say(format!("You need a Stimpak to revive {name}"));
        return;
    }
    let _ = sim.world.remove_item(FormId::PLAYER_REF, item, 1);
    // Healing Hands goes with the revive: the revived is cured of its rads.
    let perks = if fnv_game::coop_perks::has(g, fnv_game::coop_perks::HEALING_HANDS) { net::revive_perk::CURE_RADS } else { 0 };
    if let Some(link) = coop.link.as_ref() {
        link.send_revive(id, perks);
    }
    say(format!("You revived {name} ({item_name} used, {} left){}", have - 1, if perks & net::revive_perk::CURE_RADS != 0 { ": Healing Hands flushes their rads" } else { "" }));
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
    p.alone_down = role == Role::Guest && me_down && coop.alone_down;
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
                net::Progress::Gift { batch, item, count } => Shared::Gift { batch, item, count },
            },
        ));
    }
    let out = std::mem::take(&mut p.outbox);
    let Some(link) = coop.link.as_ref() else { return };
    for what in out {
        link.send_progress(match what {
            Shared::Reward { quest, xp, caps } => net::Progress::Reward { quest, xp, caps },
            Shared::Gift { batch, item, count } => net::Progress::Gift { batch, item, count },
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

/// Where a reference is in this game's world, as [`net::NpcState::space`] tells it: its
/// interior cell, or the worldspace of its exterior cell; 0 when it has no cell.
fn space_of_ref(world: &fnv_sim::World, r: FormId) -> u32 {
    let cell = world.peek(r).and_then(|s| s.cell).or_else(|| world.placed.get(r).and_then(|p| p.cell));
    cell.map_or(0, |c| world.placed.world_of(c).map_or(c.0, |w| w.0))
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

    /// s182: a joining guest's own quests are held from the host's welcome until it is
    /// flagged a guest (Dead Money's MQ00 moved it out of the bunker on that first frame);
    /// a plain game, or one already a guest (the flag's own hold), or one whose link is
    /// gone, holds nothing here.
    #[test]
    fn a_joining_guest_runs_no_world_logic_before_the_guest_flag() {
        use super::holds_joining_world;
        assert!(holds_joining_world(true, false, true), "welcomed, not yet flagged a guest");
        assert!(!holds_joining_world(false, false, true), "not welcomed yet");
        assert!(!holds_joining_world(true, true, true), "flagged a guest once: follow_npcs holds it");
        assert!(!holds_joining_world(true, false, false), "the link is gone or a host's");
    }

    /// Item 204d: a host sends an actor it draws but is still putting on its cross at
    /// once as on it, not as off; one getting off is sent as off at once, as before.
    #[test]
    fn a_host_sends_one_being_put_on_as_on() {
        let cross = 0x0013_75D8;
        assert_eq!(sent_furniture(None, Some(cross)), (cross, true));
        assert_eq!(sent_furniture(None, None), (0, false));
        assert_eq!(sent_furniture(Some((cross, true, false)), None), (cross, true));
        assert_eq!(sent_furniture(Some((cross, false, false)), None), (cross, false));
        assert_eq!(sent_furniture(Some((cross, true, true)), Some(cross)), (0, false));
    }

    /// Item 204d: a new host's first states with no furniture leave a guest's actor on
    /// the furniture it is on here; once that hold is over, or with the host's furniture,
    /// the host's word goes.
    #[test]
    fn a_new_hosts_first_states_keep_a_guest_on_its_furniture() {
        let cross = 0x0013_75D8;
        assert_eq!(furniture_here(None, true, Some(cross)), Some((cross, true)));
        assert_eq!(furniture_here(None, false, Some(cross)), None);
        assert_eq!(furniture_here(None, true, None), None);
        assert_eq!(furniture_here(Some((0x0010_6192, false)), true, Some(cross)), Some((0x0010_6192, false)));
    }

    fn speech(speaker: u32, target: Option<u32>, remaining: f64) -> fnv_game::game::Speech {
        let mut s = lines_in(&[net::LineView { speaker, target: 0, topic: 0x10, info: 0x11, remaining: 1.0, flags: 0, conversation: 0 }], 0.0, None).remove(0);
        s.target = target.map(FormId);
        s.remaining = remaining;
        s.remote = false;
        s
    }

    /// A script's move of one of the host's NPCs is drawn as a jump at its time, not a
    /// slide; a walk is blended (item 187: Kimball's MoveToMarkerWithFade slid on a
    /// guest at 1069 units/s).
    /// Two of an NPC's states heard in one frame (one came late) are blended over time,
    /// not put on at once: the Nipton survivor running about 516 units/s snapped 48-56
    /// units on a guest (laptop ship run of s187-nipton).
    #[test]
    fn npc_states_heard_in_one_frame_are_blended_not_snapped() {
        let state = |x: f32| net::NpcState { reference: 0x0013_49B6, feet: [x, 0.0, 0.0], heading: 0.0, pitch: 0.0, flags: 0, weapon: 0, attacks: 0, furniture: 0, base: 0, level: 0, all_levels: false, rolls: 0, hostile: false, space: 7 };
        let mut t = NpcTrack::default();
        t.heard(0.0, state(0.0));
        // The next round's state is late: it comes with the one after, in one frame.
        t.heard(0.2, state(52.0));
        t.heard(0.2, state(104.0));
        assert!(t.samples[2].0 > t.samples[1].0, "spaced: {:?}", t.samples.iter().map(|s| s.0).collect::<Vec<_>>());
        // Drawn frame by frame (60 fps) across them, no frame's step is longer than the run's.
        let mut last = t.pose(0.0, 516.0 * NPC_RUN_ROOM).unwrap().0[0];
        let mut at = 0.0;
        while at < 0.4 {
            at += 1.0 / 60.0;
            let x = t.pose(at, 516.0 * NPC_RUN_ROOM).unwrap().0[0];
            assert!(x - last < 48.0, "put {} units on at {at}", x - last);
            last = x;
        }
        assert_eq!(last, 104.0);
        // States on time keep the times they came at.
        let mut ok = NpcTrack::default();
        for k in 0..4 {
            ok.heard(k as f32 * 0.1, state(k as f32 * 52.0));
        }
        assert!(ok.samples.iter().enumerate().all(|(k, s)| (s.0 - k as f32 * 0.1).abs() < 1e-6));
    }

    /// A Dead Horse running about 430 units/s far from the guest, its poses sent at 3 Hz
    /// (every 6 or 7 ticks of 0.05 s): drawn frame by frame, it runs on every frame and
    /// is never held still and put 131-166 units on (laptop deadhorse-look 74bac664).
    #[test]
    fn a_far_npc_sent_at_three_hz_runs_smoothly() {
        let state = |x: f32| net::NpcState { reference: 0x0100_B824, feet: [x, 0.0, 0.0], heading: 0.0, pitch: 0.0, flags: npc_flag::MOVING, weapon: 0, attacks: 0, furniture: 0, base: 0, level: 0, all_levels: false, rolls: 0, hostile: false, space: 7 };
        let run = 430.0;
        let top = run * NPC_RUN_ROOM;
        let mut t = NpcTrack::default();
        let (mut now, mut next_sent, mut ticks) = (0.0f32, 0.0f32, 0u32);
        let mut last: Option<f32> = None;
        while now < 6.0 {
            if now >= next_sent {
                // As `replication` pushes a pose: stamped when it came.
                t.samples.push_back((now, state(now * run)));
                while t.samples.len() > 8 {
                    t.samples.pop_front();
                }
                ticks += 1;
                next_sent += if ticks % 2 == 0 { 0.30 } else { 0.35 };
            }
            t.delay = eased_delay(t.delay, t.spacing(), 1.0 / 60.0);
            let x = t.pose(now - t.delay, top).unwrap().0[0];
            if let (Some(l), true) = (last, now > 1.5) {
                let step = x - l;
                assert!(step < 48.0, "put {step:.0} units on at {now:.2} s (delay {:.2})", t.delay);
                assert!(step > 0.0, "held still at {now:.2} s (delay {:.2})", t.delay);
            }
            last = Some(x);
            now += 1.0 / 60.0;
        }
        // Its delay settled at one and a half of its rounds.
        assert!((t.delay - 0.325 * NPC_DELAY_ROUNDS).abs() < 0.05, "delay {}", t.delay);
    }

    /// Running on as it goes far from the guest: its poses come at 10 Hz, then at 3 Hz.
    /// Drawn frame by frame, no frame puts it more than a few frames' run on, and it is
    /// not held still (laptop deadhorse-look d7992be2: 22-65 unit steps every 0.35 s).
    #[test]
    fn an_npc_whose_poses_slow_down_runs_on_smoothly() {
        let state = |x: f32| net::NpcState { reference: 0x0100_B824, feet: [x, 0.0, 0.0], heading: 0.0, pitch: 0.0, flags: npc_flag::MOVING, weapon: 0, attacks: 0, furniture: 0, base: 0, level: 0, all_levels: false, rolls: 0, hostile: false, space: 7 };
        let run = 430.0;
        let mut t = NpcTrack::default();
        let (mut now, mut next_sent) = (0.0f32, 0.0f32);
        let mut last: Option<f32> = None;
        let mut biggest = 0.0f32;
        while now < 8.0 {
            if now >= next_sent {
                t.samples.push_back((now, state(now * run)));
                while t.samples.len() > 8 {
                    t.samples.pop_front();
                }
                next_sent += if now < 3.0 { 0.1 } else { 0.35 };
            }
            t.delay = eased_delay(t.delay, t.spacing(), 1.0 / 60.0);
            let x = t.pose(now - t.delay, run * NPC_RUN_ROOM).unwrap().0[0];
            if let (Some(l), true) = (last, now > 0.5) {
                biggest = biggest.max((x - l).abs());
                assert!(x - l > -1.0, "went back {:.0} units at {now:.2} s", l - x);
            }
            last = Some(x);
            now += 1.0 / 60.0;
        }
        // A frame's run is 7 units; a correction when a pose comes stays far under the
        // 48 units a frame that count as a placement.
        assert!(biggest < 24.0, "largest step {biggest:.0}");
    }

    #[test]
    fn the_delay_eases_to_its_rounds_within_bounds() {
        // First drawn: at once.
        assert_eq!(eased_delay(0.0, 0.1, 1.0 / 60.0), NPC_DELAY);
        assert_eq!(eased_delay(0.0, 2.0, 1.0 / 60.0), NPC_DELAY_MAX);
        // From 10 Hz to 3 Hz: a sixtieth of the way in a frame, all of it in a second.
        let d = eased_delay(NPC_DELAY, 0.35, 1.0 / 60.0);
        assert!(d > NPC_DELAY && d < NPC_DELAY + 0.01, "{d}");
        assert!((eased_delay(NPC_DELAY, 0.35, NPC_DELAY_EASE) - 0.525).abs() < 1e-4);
        // The far actor's 3 Hz rounds are no gap.
        const { assert!(NPC_GAP > 0.35 * 1.5) };
    }

    #[test]
    fn an_npc_moved_by_a_script_jumps_and_a_walking_one_is_blended() {
        let state = |x: f32, space: u32| net::NpcState { reference: 0x0012_FCD2, feet: [x, 0.0, 0.0], heading: 0.0, pitch: 0.0, flags: 0, weapon: 0, attacks: 0, furniture: 0, base: 0, level: 0, all_levels: false, rolls: 0, hostile: false, space };
        let track = |a: (f32, f32, u32), b: (f32, f32, u32)| NpcTrack { samples: [(a.0, state(a.1, a.2)), (b.0, state(b.1, b.2))].into(), ..NpcTrack::default() };
        // A walk: 30 units in a round, half way at half the time.
        let walk = track((0.0, 0.0, 7), (0.1, 30.0, 7));
        assert!((walk.pose(0.05, NPC_JUMP_SPEED).unwrap().0[0] - 15.0).abs() < 0.01);
        // A move of 1000 units in a round: where it was, then where it went.
        let moved = track((0.0, 0.0, 7), (0.1, 1000.0, 7));
        assert_eq!(moved.pose(0.05, NPC_JUMP_SPEED).unwrap().0[0], 0.0);
        // Not sent for a while (faded out) and 1000 units on: no slide either.
        let faded = track((0.0, 0.0, 7), (2.0, 1000.0, 7));
        assert_eq!(faded.pose(1.0, NPC_JUMP_SPEED).unwrap().0[0], 0.0);
        // 140 units in a round (1400 units/s) by one that runs 450: a jump; by one
        // that runs 955 (a cazador): its run, blended.
        let enabled = track((0.0, 0.0, 7), (0.1, 140.0, 7));
        assert_eq!(enabled.pose(0.05, 450.0 * NPC_RUN_ROOM).unwrap().0[0], 0.0);
        assert!((enabled.pose(0.05, 955.0 * NPC_RUN_ROOM).unwrap().0[0] - 70.0).abs() < 0.01);
        // Into another space: no blend across.
        let door = track((0.0, 0.0, 7), (0.1, 10.0, 8));
        assert_eq!(door.pose(0.05, NPC_JUMP_SPEED).unwrap().0[0], 0.0);
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
        // A player's voice goes by the voice table, not here: the host's own lines and its
        // stand-in for a guest are not sent as speakers (item 106).
        let host_grunt = speech(FormId::PLAYER_REF.0, None, 1.0);
        let stand_in_line = speech(stand_in.0, None, 1.0);
        assert!(lines_out(&[host_grunt, stand_in_line], |r| (r == stand_in).then_some(1)).is_empty());
    }

    fn placed(id: u32, disabled: bool) -> fnv_sim::refs::PlacedRef {
        fnv_sim::refs::PlacedRef {
            id: FormId(id), sig: *b"REFR", edid: None, base: FormId(0x1F), cell: None, stored_in: None, plugin: 0,
            flags: if disabled { fnv_sim::refs::FLAG_INITIALLY_DISABLED } else { 0 },
            in_persistent_group: false, pos: [0.0; 3], rot: [0.0; 3], scale: None, count: None, ammo: None,
            health: None, owner: None, lock: None, teleport: None, enable_parent: None, linked: None,
            patrol_idle: None, patrol_topic: None, merchant_container: None, marker_reputation: None,
            open_by_default: false, primitive: None, activate_parents: Vec::new(), parent_activate_only: false,
            prompt: None, radiation: None, radius: None, encounter_zone: None, level_modifier: None, map_marker: None, radio: None,
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
        let list = presence_of(&host, Flag::Enabled);
        assert_eq!(list, vec![(CRATE, false), (PARENT, true)]);

        let mut guest = fnv_sim::World::new(&forms, &table, Default::default());
        guest.set_enabled(FormId(SIGN), true).unwrap();
        guest.set_enabled(FormId(GECKO), true).unwrap();
        assert!(!guest.is_enabled(FormId(BOTTLE)).unwrap());
        let mut heard = HostPresence::default();
        heard.hear(1, 0, 1, list);
        let skip = |r: FormId| r == FormId(GECKO);
        let mut was = HashMap::new();
        assert_eq!(follow_presence(&mut guest, Flag::Enabled, &std::mem::take(&mut heard.fresh), &skip, &mut was), 2);
        assert!(guest.is_enabled(FormId(BOTTLE)).unwrap(), "the bottles come with their parent");
        assert!(!guest.is_enabled(FormId(CRATE)).unwrap());
        assert_eq!(undo_own_presence(&mut guest, Flag::Enabled, &heard.whole.take().unwrap(), &skip, &mut was), 1);
        assert!(!guest.is_enabled(FormId(SIGN)).unwrap());
        assert!(guest.is_enabled(FormId(GECKO)).unwrap(), "skipped");
        // Leaving co-op: as it was.
        for (r, on) in was.drain() {
            guest.set_enabled(r, on).unwrap();
        }
        assert!(guest.is_enabled(FormId(SIGN)).unwrap() && guest.is_enabled(FormId(CRATE)).unwrap() && !guest.is_enabled(FormId(BOTTLE)).unwrap());
    }

    /// Queue item 201, the owner's case: a guest whose own game finished Ghost Town
    /// Gunfight (the Powder Gangers killed, and disabled as the plugin places them) joins
    /// a host mid-quest. The ganger the host has alive and drawn here is up again and
    /// enabled on the guest; one dead on the host is left to the deaths (dead here too);
    /// one the host disabled is not brought back.
    #[test]
    fn a_guests_own_dead_gangers_are_alive_as_in_the_hosts_world() {
        const CELL: u32 = 0x0010_C000;
        const ALIVE: u32 = 0x0010_4C70;
        const KILLED: u32 = 0x0010_4C72;
        const GONE: u32 = 0x0010_4C73;
        const UNPLACED: u32 = 0x0010_4C75;
        let mut table = fnv_sim::RefTable::default();
        for (id, disabled) in [(FormId::PLAYER_REF.0, false), (ALIVE, true), (KILLED, true), (GONE, true), (UNPLACED, true)] {
            let mut p = placed(id, disabled);
            p.cell = Some(FormId(CELL));
            table.refs.insert(p.id, p);
        }
        let forms = fnv_sim::FormStore::default();
        let mut world = fnv_sim::World::new(&forms, &table, Default::default());
        let _ = world.get(FormId::PLAYER_REF).unwrap();
        let empty = crate::outfits::tests::empty_world_data();
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/script/functions.json");
        let json = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let functions = fnv_script::FunctionTable::load(&path).unwrap();
        let no_refs: &'static fnv_sim::RefTable = Box::leak(Box::default());
        let no_forms: &'static fnv_sim::FormStore = Box::leak(Box::default());
        let data: &'static fnv_game::GameData = Box::leak(Box::new(fnv_game::GameData::build(empty.lo, empty.schema, functions, &json, no_refs)));
        let mut g = fnv_game::Game::new(data, fnv_sim::World::new(no_forms, no_refs, Default::default()), Default::default());
        // The guest's own game: every ganger killed.
        for r in [ALIVE, KILLED, GONE, UNPLACED] {
            g.extra.entry(FormId(r)).or_default().dead = true;
        }
        let sample = |r: u32, flags: u8| net::NpcState { reference: r, feet: [10.0, 20.0, 30.0], heading: 0.0, pitch: 0.0, flags, weapon: 0, attacks: 0, furniture: 0, base: 0, level: 0, all_levels: false, rolls: 0, hostile: true, space: 0 };
        let mut npcs = HashMap::new();
        for (r, flags) in [(ALIVE, 0), (KILLED, npc_flag::DEAD), (GONE, 0)] {
            let mut t = NpcTrack::default();
            t.samples.push_back((10.0, sample(r, flags)));
            npcs.insert(r, t);
        }
        // One the host drew this frame, its body not put where it stands yet: at the origin.
        let mut t = NpcTrack::default();
        t.samples.push_back((10.0, net::NpcState { feet: [0.0; 3], ..sample(UNPLACED, 0) }));
        npcs.insert(UNPLACED, t);
        let host_disabled = HashSet::from([GONE]);
        let enabled = place_like_host(&mut world, &mut g, &npcs, &host_disabled, 5.0, 10.2);
        assert_eq!(enabled, vec![FormId(ALIVE)]);
        assert!(world.is_enabled(FormId(ALIVE)).unwrap() && !g.is_dead(FormId(ALIVE)), "alive and drawn as in the host's world");
        assert!(!world.is_enabled(FormId(KILLED)).unwrap() && g.is_dead(FormId(KILLED)), "dead on the host: left to the deaths");
        assert!(!world.is_enabled(FormId(GONE)).unwrap(), "the host disabled it");
        assert!(!world.is_enabled(FormId(UNPLACED)).unwrap() && g.is_dead(FormId(UNPLACED)), "a sample at the origin is no place: left for the next");
        // Revived where the host has it, not where the corpse (or nothing) put it here.
        assert_eq!(world.peek(FormId(ALIVE)).map(|s| s.pos), Some([10.0, 20.0, 30.0]), "stands up at the host's feet");
    }

    /// The host's doors reach a guest's: a door the host opened opens, one only the
    /// guest's game opened closes again, except the one its player just used; a load door
    /// takes no part.
    #[test]
    fn the_hosts_doors_win_on_a_guest() {
        const OPENED: u32 = 0x0010_0D01;
        const OWN: u32 = 0x0010_0D02;
        const USED: u32 = 0x0010_0D03;
        const LOAD: u32 = 0x0010_0D04;
        let mut table = fnv_sim::RefTable::default();
        let mut load = placed(LOAD, false);
        load.teleport = Some(fnv_sim::refs::Teleport { door: FormId(0x0010_0D05), pos: [0.0; 3], rot: [0.0; 3], no_alarm: false });
        for p in [placed(OPENED, false), placed(OWN, false), placed(USED, false), load] {
            table.refs.insert(p.id, p);
        }
        let forms = fnv_sim::FormStore::default();
        let mut host = fnv_sim::World::new(&forms, &table, Default::default());
        host.get(FormId(OPENED)).unwrap().open = true;
        host.get(FormId(LOAD)).unwrap().open = true;
        let list = presence_of(&host, Flag::Open);
        assert_eq!(list, vec![(OPENED, true)]);

        let mut guest = fnv_sim::World::new(&forms, &table, Default::default());
        guest.get(FormId(OWN)).unwrap().open = true;
        guest.get(FormId(USED)).unwrap().open = true;
        let mut heard = HostPresence::default();
        heard.hear(1, 0, 1, list);
        let skip = |r: FormId| r == FormId(USED);
        let mut was = HashMap::new();
        assert_eq!(follow_presence(&mut guest, Flag::Open, &std::mem::take(&mut heard.fresh), &skip, &mut was), 1);
        assert_eq!(undo_own_presence(&mut guest, Flag::Open, &heard.whole.take().unwrap(), &skip, &mut was), 1);
        let open = |r: u32| door_open(&guest, FormId(r)).unwrap();
        assert!(open(OPENED) && !open(OWN) && open(USED));
        assert_eq!(door_open(&guest, FormId(LOAD)), None, "a load door");
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
            race: FormId(0x19),
            female: true,
            hair: Some(FormId(0x0004_2A1B)),
            hair_color: Some([120, 80, 40]),
            eyes: None,
            head_parts: vec![FormId(0x1234)],
            fggs: (0..50).map(|i| i as f32 * 0.1).collect(),
            fgga: vec![-0.5; 30],
            fgts: (0..50).map(|i| -(i as f32)).collect(),
        };
        let name = "Ñ".repeat(40);
        let head = r"interface\icons\message icons\glow_message_vaultboy_ninja.dds";
        let b = encode_look(Some(&l), &name, head);
        assert!(b.len() <= net::LOOK_MAX, "{} bytes", b.len());
        // Cut to whole characters within NAME_MAX bytes.
        assert_eq!(decode_look(&b), Some((Some(l.clone()), "Ñ".repeat(NAME_MAX / 2), head.to_string())));
        assert_eq!(decode_look(&encode_look(None, "", "")), Some((None, String::new(), String::new())));
        assert_eq!(decode_look(&encode_look(None, "Cass", "")), Some((None, "Cass".into(), String::new())));
        // A look from a game that sends no name.
        assert_eq!(decode_look(&look_only(Some(&l))), Some((Some(l), String::new(), String::new())));
        assert_eq!(decode_look(&b[..b.len() - 1]), None);
        assert_eq!(decode_look(&[9]), None);
    }
}
