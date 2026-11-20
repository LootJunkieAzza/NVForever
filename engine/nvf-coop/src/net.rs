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
//! door or terminal goes to everyone ([`Session::send_unlock`]), and so does a door one
//! opens or closes ([`Session::send_door`]); the host's world's presence and its doors'
//! open flags go to the guests as lists ([`Msg::Presence`]). A player another revives
//! is told so through the host ([`Session::send_revive`]). An animation a player plays
//! for an item (eating, a stimpak, skinning) goes to everyone through the host
//! ([`Session::send_anim`], [`AnimCue`]); an older game drops the kind it does not know.
//! The lines the host's NPCs are saying (a scene between two of them, a Say, a hello) go
//! to the guests ([`LineView`], every [`LINES_EVERY`] and at once when they change), who
//! hear them and see their subtitles as the host does.
//! What a script scene does to the host's player goes to the guests ([`SceneView`],
//! every [`SCENE_EVERY`] and at once when it changes): the control groups the host's
//! scripts hold and the image space modifiers on the host's screen, so a guest is held
//! and sees the same fades while the host's scene plays.
//! Shared progression goes the same way ([`Session::send_progress`], [`Progress`]): a
//! quest the host completed pays each guest its XP and caps, a save by anyone saves every
//! game, and the host tells everyone to load the last shared save when all are down.
//!
//! **Minigame tables.** A player at a table (blackjack, roulette, slots, darts, five
//! finger fillet) sends what it shows ([`Session::set_table`], a view: the newest wins,
//! again every [`TABLE_EVERY`]) so the others can watch, and a table another player owns
//! (a shared blackjack dealer, a darts match) takes actions from the others
//! ([`Session::send_table_action`]: three copies, each taken once). The host passes both
//! on. The blobs are the game side's (nvf-play `coop_tables`).
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

use crate::rel::{Frame, Reliable};
use crate::repl::ActorPose;
use crate::steam::Steam;
use crate::story::{self, StoryEntry, StoryPager, StoryValue};
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

pub const MAGIC: [u8; 4] = *b"NVCO";
/// The replication test build (docs/design/coop-replication.md) speaks its own protocol:
/// 100 above the normal co-op build's, so the two never join each other. 130 (0.7.1, one bump
/// for both): `Effect::Sync` (effect code 8, the syncing screen) and the change log's
/// `repl::Value::Pose` (comp 9, a dead actor's resting ragdoll pose), which 129 cannot read.
/// 132 (ship24.2): the lobby takes 6 players (`MAX_GUESTS` 5), so a 131 game (4 players) and
/// a 132 game never join each other.
pub const VERSION: u8 = 132;
pub const MAX_GUESTS: usize = 5;
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
/// IP and UDP headers, counted with each packet's bytes in [`Session::traffic`].
const UDP_OVERHEAD: u64 = 28;
/// The most reliable packets one stream sends per network tick (10 ms): about 110 KB/s
/// while a join's snapshot goes, far less after.
const REL_PACKETS: usize = 4;

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
/// A reliable frame of a stream this few behind the guest's is one late on the way (each
/// Hello the host hears starts one more); any other number is a new stream [design].
const EPOCHS_LATE: u16 = 8;
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
    /// The furniture reference it is using or walking up to (a chair, a bed, a cross), 0
    /// for none; [`npc_flag::SEATED`] once it is on it.
    pub furniture: u32,
    /// Its base form as the host's world has it (an NPC, a creature or a leveled list of
    /// them): what a guest makes it of when the host's game created it (a dynamic
    /// reference the guest's world does not have).
    pub base: u32,
    /// The level its leveled lists are read at in the host's game, and whether every
    /// entry at or under it is eligible (`fnv_sim::zones::LevelAt`); level 0: none told.
    pub level: u16,
    pub all_levels: bool,
    /// How many times its leveled picks were rolled again (a resurrection: the seed,
    /// `fnv_sim::leveled::roll_seed`), up to 127. With [`Self::level`], so a guest draws
    /// the leveled pick the host draws.
    pub rolls: u8,
    /// Hostile to the players in the host's game: fighting one or ready to attack one
    /// (`Game::would_attack`, the host's scripts' StartCombat and faction changes
    /// included), for a guest's compass (Boulder City, laptop 2026-11-09). Sent in the
    /// top bit of [`Self::level`] (levels stay under 32768).
    pub hostile: bool,
    /// Where it is in the host's world: its interior cell, or the worldspace of the
    /// exterior it stands in (0: not told). A guest draws it only there [until protocol
    /// 129 a guest put every NPC the host drew in its own place: the NCR Embassy's MPs,
    /// drawn and killed inside on the host, hung in the air over the Strip at the
    /// interior's coordinates on a guest outside, playtest 119 item 151].
    pub space: u32,
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
    /// On its furniture ([`NpcState::furniture`]), not walking up to it.
    pub const SEATED: u8 = 64;
    /// Knocked down or out and lying (`fnv_game::unconscious`): the others' copies lie
    /// until it clears, then get up (queue item 104).
    pub const KNOCKED: u8 = 128;
}

/// NPC states in one packet (45 bytes each: under 1200 bytes in all).
pub const NPCS_PER_PACKET: usize = 26;
/// How often the host sends its NPCs.
pub const NPC_HZ: u32 = 10;
/// The host sends its active quest this often, and at once when it changes.
pub const QUEST_EVERY: Duration = Duration::from_secs(2);
/// How often the host sends the lines its NPCs are saying (and at once when they change).
pub const LINES_EVERY: Duration = Duration::from_millis(250);
/// Lines sent at most.
pub const LINES_MAX: usize = 24;
/// How often the host sends a page of its world's presence ([`Msg::Presence`]).
pub const PRESENCE_EVERY: Duration = Duration::from_millis(200);
/// References one presence page carries at most (5 bytes each: 1000 bytes).
pub const PRESENCE_PAGE: usize = 200;
/// The host's lists ([`Msg::Presence`]): each placed reference whose own enabled flag
/// differs from its plugin's ...
pub const LIST_ENABLED: u8 = 0;
/// ... and each whose open flag (a door) differs from its plugin's.
pub const LIST_OPEN: u8 = 1;
/// ... and each campfire of Burning Campfire Redone burning in the host's game, with
/// whether its silent fires still burn beside the one with the sound
/// (`fnv_game::campfire`).
pub const LIST_CAMPFIRE: u8 = 2;
/// How many lists there are.
pub const LISTS: usize = 3;
/// How often the host sends a page of its story ([`Msg::Story`], `crate::story`).
pub const STORY_EVERY: Duration = Duration::from_millis(200);
/// How often the host sends a page of what its NPCs wear ([`Msg::Wear`]).
pub const WEAR_EVERY: Duration = Duration::from_millis(250);
/// NPCs one wear page carries at most.
pub const WEAR_PAGE: usize = 32;
/// Items one NPC's entry carries at most (its worn armour and its weapon).
pub const WEAR_ITEMS: usize = 16;
/// How often the host sends its game clock.
pub const CLOCK_EVERY: Duration = Duration::from_secs(2);
/// How often the host sends its [`SceneView`] when it has not changed.
pub const SCENE_EVERY: Duration = Duration::from_millis(500);
/// Image space modifiers a [`SceneView`] carries at most.
pub const SCENE_MODS_MAX: usize = 16;
/// Texture swaps a [`SceneView`] carries at most, and the longest shape name or texture
/// path one may have (bytes); a longer one is not sent.
pub const SCENE_SWAPS_MAX: usize = 8;
pub const SWAP_TEXT_MAX: usize = 200;
/// How often the host sends every player's name ([`Msg::Names`]), and at once after a
/// guest joins.
pub const NAMES_EVERY: Duration = Duration::from_secs(2);
/// Objectives a [`QuestView`] carries at most.
pub const QUEST_OBJECTIVES: usize = 64;
/// The most a hit takes (the game's own encoding, [`Session::send_hit`]).
pub const HIT_MAX: usize = 64;
/// Each hit is sent this many times (the host takes it once, by its id).
const HIT_COPIES: usize = 3;
/// A goodbye is sent this many times ([`Session::bye`]) [design: as [`HIT_COPIES`]].
pub const BYE_COPIES: usize = HIT_COPIES;
/// Between the copies of a goodbye [design: apart enough that one burst of loss does not
/// take them all, short enough that quitting does not wait].
const BYE_GAP: Duration = Duration::from_millis(15);
/// Hit ids a host remembers per guest, to take each hit once.
const HITS_KEPT: usize = 64;
/// The most a table's blob takes ([`Session::set_table`]): one packet, with room to spare.
pub const TABLE_MAX: usize = 1100;
/// How often a table view is sent again while it stays the same (a lost one is made up
/// for by the next; a changed one goes at once).
pub const TABLE_EVERY: Duration = Duration::from_millis(250);
/// The `game` bit of a table action ([`Session::send_table_action`]): sent
/// [`HIT_COPIES`] times, each taken once. Without it the message is a view: the newest
/// wins.
pub const TABLE_ACTION: u8 = 0x80;
/// The `game` of a view that says the player left the table.
pub const TABLE_LEFT: u8 = 0;
/// Table actions remembered per player, to take each once.
const ACTIONS_KEPT: usize = 32;

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

/// [`Msg::Revive`]'s `perks` bits: the reviver's co-op perks that act on the revived
/// (`fnv_game::coop_perks`).
pub mod revive_perk {
    /// Healing Hands: the revived is cured of all radiation.
    pub const CURE_RADS: u8 = 1;
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
    /// The Pip-Boy: 0 down, 1 the glance, 2 the full Pip-Boy (`nvf_play::pipboy::glance::Check`).
    pub pipboy: u8,
    /// The furniture reference holding the player (a chair, a couch, a bed), 0 for none:
    /// the others draw the player getting on, sitting or lying there, and getting off.
    pub furniture: u32,
    /// [`more_flag`] bits: the states [`PlayerState::flags`] has no room for.
    pub more: u8,
    /// The emote the player plays (queue item 137), for the others to draw on its puppet.
    pub emote: EmoteState,
}

/// An emote as it goes to the others: which (`id` 0 for none, else the emote's number
/// plus one, `nvf_play::emotes`), which of its clips (`variant`), its phase ([`emote`]),
/// which of its loop's clips (`step`) and how far into the clip it is (`t`, seconds), so
/// one who joins or comes near late sees it where it is.
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
    /// In combat (the battle music's test): the others cannot fast travel to this player.
    pub const COMBAT: u8 = 2;
    /// Somewhere no one may fast travel to this player: in a conversation or a scene that
    /// holds the controls, or with fast travel turned off by a script (queue item 134).
    pub const NO_TRAVEL: u8 = 4;
}

impl PlayerState {
    /// A state with nothing in hand and nothing worn.
    pub fn at(place: Place, feet: [f32; 3], yaw: f32) -> Self {
        Self { place, feet, yaw, pitch: 0.0, flags: 0, weapon: 0, attacks: 0, attack_kind: 0, reloads: 0, worn: [0; MAX_WORN], worn_len: 0, pipboy: 0, furniture: 0, more: 0, emote: EmoteState { id: 0, variant: 0, phase: 0, step: 0, t: 0.0 } }
    }

    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
    }

    pub fn worn(&self) -> &[u32] {
        &self.worn[..(self.worn_len as usize).min(MAX_WORN)]
    }
}

/// An animation a player plays for an item, for the others to show on its puppet
/// (`consume_anims.rs` in the game). `clip` and `edid` are at most [`ANIM_TEXT_MAX`]
/// bytes each.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AnimCue {
    /// [`AnimCue::CONSUME`], [`AnimCue::INTERACT`] or [`AnimCue::CRAFT`].
    pub kind: u8,
    /// How it looks without a third-person clip (`fnv_actor::b42::Style` code).
    pub style: u8,
    /// Worn in power armour.
    pub power_armor: bool,
    /// The item (or recipe), by FormID in the load order every game runs.
    pub item: u32,
    /// Its EditorID: the folder the clips are in.
    pub edid: String,
    /// The clip the player played (its archive path).
    pub clip: String,
}

impl AnimCue {
    pub const CONSUME: u8 = 0;
    pub const INTERACT: u8 = 1;
    pub const CRAFT: u8 = 2;
    /// A clip of the game's own, played on the puppet as named (`clip`): a darts throw.
    pub const GAME: u8 = 3;
}

pub const ANIM_TEXT_MAX: usize = 160;

/// What a game keeps of the animations it sent and heard.
#[derive(Default)]
struct AnimWire {
    seq: u16,
    /// (player, seq) of the newest heard, so the copies count once.
    heard: VecDeque<(u8, u16)>,
}

impl AnimWire {
    /// Whether (`id`, `seq`) is new; noted.
    fn first(&mut self, id: u8, seq: u16) -> bool {
        if self.heard.contains(&(id, seq)) {
            return false;
        }
        if self.heard.len() == HITS_KEPT {
            self.heard.pop_front();
        }
        self.heard.push_back((id, seq));
        true
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
    /// Every player's name, from the host (the host is id 0): a guest learns the host's
    /// and the other guests' names from it.
    Names { names: Vec<(u8, String)> },
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
    /// What the host's scripts do to the host's player now ([`SceneView`]).
    Scene(SceneView),
    /// The lines the host's NPCs are saying now (the newest `seq` wins).
    Lines { seq: u32, lines: Vec<LineView> },
    /// Page `page` of `pages` of one of the host's lists (`list`: [`LIST_ENABLED`], each
    /// placed reference whose own enabled flag differs from the one its plugin gives it,
    /// with the flag; [`LIST_OPEN`], the same for the open flag). `serial` changes when
    /// the list does; the pages go round, one at a time.
    Presence { list: u8, serial: u32, page: u16, pages: u16, refs: Vec<(u32, bool)> },
    /// A page of the host's story: quests' states and quest script variables
    /// (`crate::story`).
    Story { entries: Vec<StoryEntry> },
    /// What some of the host's NPCs wear and hold, as its game draws them: each NPC's
    /// reference and its worn armour and weapon (the whole set, so each entry stands
    /// alone). The host sends a page every [`WEAR_EVERY`], round and round.
    Wear { npcs: Vec<(u32, Vec<u32>)> },
    /// A door was opened (`open`) or closed by a player (to the host, and from it to
    /// everyone) or in the host's world.
    Door { reference: u32, open: bool },
    /// A guest lit campfire `reference` (or kept it burning): to the host, which lights it
    /// and lists it ([`LIST_CAMPFIRE`]).
    Campfire { reference: u32 },
    /// Player `by` revived player `target` (who is down); `perks`: the reviver's co-op
    /// perks that act on the revived ([`revive_perk`]).
    Revive { target: u8, by: u8, perks: u8 },
    /// Player `id` at a minigame table: what it shows (a view, the newest wins) or an
    /// action for the table's owner (`game` with [`TABLE_ACTION`]). `table` names the
    /// table (a casino's FormID, a dartboard's reference), `blob` is the game's own
    /// encoding. The host passes each on to the other guests.
    Table { id: u8, table: u32, game: u8, seq: u32, blob: Vec<u8> },
    /// Player `id` plays an animation for an item ([`AnimCue`]).
    Anim { id: u8, seq: u16, cue: AnimCue },
    /// Shared progression from player `id` ([`Progress`]).
    Progress { id: u8, seq: u16, what: Progress },
    /// The reliable stream between the host and one guest ([`crate::rel`]), both ways;
    /// `epoch` is the host's number for the stream, new each time the guest says Hello.
    Rel { epoch: u16, frame: Frame },
    /// The host's moving actors, for one guest (the compact stream, [`ActorPose`]); all
    /// packets of one round share `seq`.
    /// `ms`: the host's clock when the round went (its session's milliseconds), so a
    /// guest can tell how late each round is.
    Actors { seq: u32, ms: u32, poses: Vec<ActorPose> },
}

/// Shared progression (co-op progression, [`Session::send_progress`]). Each is sent
/// [`HIT_COPIES`] times and taken once.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Progress {
    /// The host completed `quest` (FormID in the load order every game runs): each guest
    /// gets its `xp` (as `RewardXP` gives it) and `caps`. Only the host's are taken.
    Reward { quest: u32, xp: u32, caps: u32 },
    /// The sender saved: every game saves now, so nobody keeps a save the others lack.
    /// `stamp` names this shared save (the sender's choice, for the logs); `kind` is
    /// the sender's save kind ([`Progress::MANUAL`], [`Progress::QUICK`],
    /// [`Progress::AUTO`], [`Progress::SYSTEM`]): each game saves the same kind.
    Save { stamp: u32, kind: u8 },
    /// Every player is down with nobody left to revive: every game loads its last shared
    /// save. Only the host's are taken.
    Reload { stamp: u32 },
    /// A hand-out of the host's result scripts (`count` of `item`; `batch` groups one
    /// script's): every guest takes it as its rules say (`fnv_game::coop_rewards`, items
    /// 112 and 114). Only the host's are taken. `quest` (0: none) and `stage`
    /// ([`Progress::NO_STAGE`]: none) name the result script's quest stage, so a guest
    /// that did it in its own game keeps its own (owner, 2026-11-14).
    Gift { batch: u32, item: u32, count: u32, quest: u32, stage: u32 },
    /// The host's `quest` ended: completed, or `failed` (a Fail Quest stage, FailQuest).
    /// Every guest shows its line and lists it as the host does, in the order they came
    /// (a quest completed and failed in one frame: both; item 205). Only the host's are
    /// taken.
    QuestEnded { quest: u32, failed: bool },
    /// A guest's join choice for joint progression and its character's stable ID
    /// (`fnv_game::coop_joint`; shared-progression plan §3.7). Guest to host.
    JointChoice { character: u64, on: bool },
    /// The host set `stage` of a claimable `quest` (`fnv_game::coop_joint::StageOffer`):
    /// for the players whose bit is set in `to` (bit = player id). `line` is the INFO
    /// whose result script set it (0: none), its end script when `line_end`; `prior`
    /// the host's earlier stages of the quest. Only the host's are taken.
    Stage { quest: u32, stage: i32, line: u32, line_end: bool, prior: Vec<i32>, to: u8 },
    /// A guest took or turned down a [`Progress::Stage`] (for the host's log).
    StageResult { quest: u32, stage: i32, taken: bool },
    /// Kill credit for player `to` (shared-progression plan §4.2): `xp` of the kill of
    /// `target`, its `karma`, and whether `to` was the killer (kill count, challenges).
    /// Only the host's are taken.
    /// `infamy`: the guest's own seen-murder infamy by reputation (whole points).
    KillCredit { to: u8, target: u32, target_base: u32, npc: bool, xp: u32, karma: i32, killer: bool, infamy: Vec<(u32, i32)> },
    /// A guest's own hit, for its hit and damage challenges (`weapon`/`part` 0 and
    /// `u32::MAX` for none). Only the host's are taken.
    HitCredit { to: u8, target: u32, weapon: u32, part: u32, dealt: u32, critical: bool, crippled: bool },
}

impl Progress {
    pub const MANUAL: u8 = 0;
    pub const QUICK: u8 = 1;
    pub const AUTO: u8 = 2;
    pub const SYSTEM: u8 = 3;
    /// [`Progress::Gift`]'s stage when the script was no quest stage's.
    pub const NO_STAGE: u32 = u32::MAX;

    /// Only the host may send it (a guest's is dropped).
    pub fn host_only(&self) -> bool {
        !matches!(self, Progress::Save { .. } | Progress::JointChoice { .. } | Progress::StageResult { .. })
    }
}

/// What a player's table shows now ([`Session::set_table`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TableView {
    pub table: u32,
    /// The game (the game side's numbering; never [`TABLE_LEFT`] or with [`TABLE_ACTION`]).
    pub game: u8,
    pub blob: Vec<u8>,
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

/// What the host's scripts do to the host's player now, for the guests to share: the
/// control groups held (`locks`, bit `i` for group `i` in the order of
/// `fnv_game::Player::controls_disabled`: movement, Pip-Boy, fighting, POV, looking,
/// rollover, sneaking) and the image space modifiers on the screen (IMAD FormID, seconds
/// since applied), at most [`SCENE_MODS_MAX`]; and what the host's scripts put on the host's
/// player and it still wears (`worn`: item FormID, locked on), at most [`SCENE_MODS_MAX`];
/// and, while a group is held, the textures its scripts swapped on references' shapes
/// (`swaps`: reference FormID, shape name, texture path; a slideshow's slides), at most
/// [`SCENE_SWAPS_MAX`], each string at most [`SWAP_TEXT_MAX`] bytes.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SceneView {
    pub locks: u8,
    pub mods: Vec<(u32, f32)>,
    pub worn: Vec<(u32, bool)>,
    pub swaps: Vec<(u32, String, String)>,
    /// The story NPC the host waits to talk to until every player is near it, and the
    /// players (ids) not near it yet (`fnv_game::Game::story_gate`): each of those is
    /// offered a fast travel to the host. `None`: no wait.
    pub waiting: Option<(u32, Vec<u8>)>,
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

/// A line one of the host's NPCs is saying, as the host's game has it (`fnv_game`'s
/// `Speech`): the guests say it too. References are the load order's, which every game
/// runs; the host's player is [`LineView::HOST`], a guest's actor in the host's world is
/// [`LineView::guest`] of its id.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LineView {
    pub speaker: u32,
    /// The listener, 0 for none.
    pub target: u32,
    pub topic: u32,
    pub info: u32,
    /// Seconds (game time) left until it ends, as the host sent it.
    pub remaining: f32,
    /// [`LineView::SAY_TO`] and the others.
    pub flags: u8,
    /// A line of a conversation between two actors: its number in it, 0 for none.
    pub conversation: u16,
}

impl LineView {
    pub const SAY_TO: u8 = 1;
    pub const GREETING: u8 = 2;
    pub const HELLO: u8 = 4;
    pub const FORCE_SUBTITLES: u8 = 8;
    pub const TWO_D: u8 = 16;
    /// The host's player as a speaker or listener.
    pub const HOST: u32 = 0xFE00_C000;

    /// Player `id` as a speaker or listener (the host is id 0: [`LineView::HOST`]).
    pub fn guest(id: u8) -> u32 {
        Self::HOST + id as u32
    }

    pub fn has(&self, f: u8) -> bool {
        self.flags & f != 0
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
    /// The host's [`SceneView`], each time it comes (guests only).
    Scene(SceneView),
    /// The lines the host's NPCs are saying now, each time they come (guests only).
    Lines(Vec<LineView>),
    /// A page of one of the host's lists, each time it comes (guests only;
    /// [`Msg::Presence`]).
    Presence { list: u8, serial: u32, page: u16, pages: u16, refs: Vec<(u32, bool)> },
    /// A page of the host's story, each time it comes (guests only; [`Msg::Story`]).
    Story(Vec<StoryEntry>),
    /// What some of the host's NPCs wear and hold ([`Msg::Wear`]).
    Wear(Vec<(u32, Vec<u32>)>),
    /// Player `id` (0: the host's world) opened (`open`) or closed a door. It may come
    /// more than once.
    Door { id: u8, reference: u32, open: bool },
    /// Guest `id` lit campfire `reference` (host only). It may come more than once.
    Campfire { id: u8, reference: u32 },
    /// Player `by` revived this player, with its co-op perks ([`revive_perk`]). It may
    /// come more than once.
    Revived { by: u8, perks: u8 },
    /// Player `id` at a table: a view (newest only; `game` [`TABLE_LEFT`]: it left), or
    /// an action (`game` with [`TABLE_ACTION`], each once).
    Table { id: u8, table: u32, game: u8, blob: Vec<u8> },
    /// Player `id` plays an animation for an item (each once).
    Anim { id: u8, cue: AnimCue },
    /// Shared progression from player `id` (each once; [`Progress::host_only`] ones only
    /// from the host).
    Progress { id: u8, what: Progress },
    /// An item of the reliable stream from player `id` (the host: 0), once and in order.
    Reliable { id: u8, item: Vec<u8> },
    /// The reliable stream with player `id` started again (a guest's Hello on the host;
    /// a new stream from the host on a guest, id 0): what was sent on it before may be
    /// lost, so the layer above sends its whole state again.
    RelReset { id: u8 },
    /// Some of the host's moving actors (guests only), with the host's clock when they
    /// went (milliseconds) and this game's when they came.
    Actors { ms: u32, here_ms: u32, poses: Vec<ActorPose> },
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
    /// The ids of a guest's newest hits (host only).
    hits: VecDeque<u16>,
    /// The host's reliable stream with this guest, and its number (host only).
    rel: Reliable,
    epoch: u16,
}

impl Peer {
    fn new(id: u8, now: Instant) -> Self {
        Self { id, addr: None, token: 0, last_heard: now, last_seq: None, lost: false, look: None, name: String::new(), hits: VecDeque::new(), rel: Reliable::new(), epoch: 0 }
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

/// One of the host's paged lists ([`Msg::Presence`]): the list, its serial and the next
/// page to send.
#[derive(Default)]
struct Paged {
    refs: Vec<(u32, bool)>,
    serial: u32,
    page: usize,
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
    /// The host's scene now, and when and what it last sent.
    scene: SceneView,
    last_scene: Option<(Instant, SceneView)>,
    /// The host's own name, and when every player's names last went.
    own_name: String,
    last_names: Option<Instant>,
    /// The host's NPCs' lines, and the last sent (when, which); a guest's newest heard.
    lines: Vec<LineView>,
    last_lines: Option<(Instant, Vec<(u32, u32)>)>,
    lines_seq: u32,
    heard_lines: Option<u32>,
    /// The host's lists ([`Msg::Presence`]), and when the last pages went.
    lists: [Paged; LISTS],
    last_presence: Option<Instant>,
    /// The host's story ([`Msg::Story`]) and when its last page went.
    story: StoryPager,
    last_story: Option<Instant>,
    /// What the host's NPCs wear ([`Msg::Wear`]), the next page to send and when the
    /// last went.
    wear: Vec<(u32, Vec<u32>)>,
    wear_page: usize,
    last_wear: Option<Instant>,
    /// This player's table view, and when it last went.
    table: Option<TableView>,
    last_table: Option<Instant>,
    table_seq: u32,
    /// Table messages heard, by sender: the newest view's seq and the actions' seqs.
    table_heard: Vec<(u8, Option<u32>, VecDeque<u32>)>,
    anims: AnimWire,
    /// Shared progression sent and heard (the same bookkeeping as the animations').
    progress: AnimWire,
    events: Vec<Event>,
    /// A guest's reliable stream with the host, and the host's number for it once heard.
    rel: Reliable,
    host_epoch: Option<u16>,
    /// The host's actor rounds sent, a guest's newest heard.
    actor_seq: u32,
    heard_actors: Option<u32>,
    /// Bytes sent to and heard from each address (the perf lines).
    sent: RefCell<HashMap<SocketAddr, u64>>,
    heard: HashMap<SocketAddr, u64>,
    /// When this session began (the clock of [`Msg::Actors`]).
    started: Instant,
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
        Ok(Self { sock, steam: None, role, peers: Vec::new(), state: None, seq: 0, last_send: None, look: None, last_look: None, lobby: None, npcs: Vec::new(), last_npcs: None, npc_seq: 0, heard_npcs: None, hit_seq: 0, heard_hits: VecDeque::new(), quest: None, last_quest: None, heard_quest: None, clock: None, last_clock: None, scene: SceneView::default(), last_scene: None, own_name: String::new(), last_names: None, lines: Vec::new(), last_lines: None, lines_seq: 0, heard_lines: None, lists: Default::default(), last_presence: None, story: StoryPager::default(), last_story: None, table: None, last_table: None, table_seq: 0, table_heard: Vec::new(), anims: AnimWire::default(), progress: AnimWire::default(), wear: Vec::new(), last_wear: None, wear_page: 0, events: Vec::new(), rel: Reliable::new(), host_epoch: None, actor_seq: 0, heard_actors: None, sent: RefCell::new(HashMap::new()), heard: HashMap::new(), started: Instant::now() })
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

    /// The host's scene now; [`Session::tick`] sends it when it changes and every
    /// [`SCENE_EVERY`].
    pub fn set_scene(&mut self, mut scene: SceneView) {
        scene.mods.truncate(SCENE_MODS_MAX);
        scene.worn.truncate(SCENE_MODS_MAX);
        scene.swaps.retain(|(_, s, t)| s.len() <= SWAP_TEXT_MAX && t.len() <= SWAP_TEXT_MAX);
        scene.swaps.truncate(SCENE_SWAPS_MAX);
        self.scene = scene;
    }

    /// The host's own name, for the guests ([`Msg::Names`]).
    pub fn set_name(&mut self, name: &str) {
        let name = truncate(name);
        if name != self.own_name {
            self.own_name = name;
            self.last_names = None;
        }
    }

    /// The lines the host's NPCs are saying now; [`Session::tick`] sends them every
    /// [`LINES_EVERY`], and at once when a line starts or ends. At most [`LINES_MAX`].
    pub fn set_lines(&mut self, mut lines: Vec<LineView>) {
        lines.truncate(LINES_MAX);
        self.lines = lines;
    }

    /// One of the host's lists now (`list`, [`Msg::Presence`]); [`Session::tick`] sends a
    /// page of each every [`PRESENCE_EVERY`], from the first page again when it changed.
    pub fn set_presence(&mut self, list: u8, refs: Vec<(u32, bool)>) {
        let Some(l) = self.lists.get_mut(list as usize) else { return };
        if refs != l.refs {
            l.refs = refs;
            l.serial = l.serial.wrapping_add(1);
            l.page = 0;
        }
    }

    /// The host's story now ([`Msg::Story`]); [`Session::tick`] sends a page every
    /// [`STORY_EVERY`] (`crate::story`).
    pub fn set_story(&mut self, entries: Vec<StoryEntry>) {
        self.story.set(entries);
    }

    /// What the host's NPCs wear and hold now ([`Msg::Wear`]): each one's reference and
    /// its worn armour and weapon. [`Session::tick`] sends a page every [`WEAR_EVERY`],
    /// round and round (a lost page comes again); an entry has at most [`WEAR_ITEMS`].
    pub fn set_wear(&mut self, mut npcs: Vec<(u32, Vec<u32>)>) {
        for (_, items) in &mut npcs {
            items.truncate(WEAR_ITEMS);
        }
        self.wear = npcs;
    }

    /// This player opened (`open`) or closed door `reference` (the host: or its world
    /// did): to the host, or from the host to every guest ([`HIT_COPIES`] times).
    pub fn send_door(&mut self, reference: u32, open: bool) {
        let msg = Msg::Door { reference, open };
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

    /// This guest lit campfire `reference`: to the host ([`HIT_COPIES`] times). A host
    /// sends nothing: its list says it ([`LIST_CAMPFIRE`]).
    pub fn send_campfire(&mut self, reference: u32) {
        let Role::Guest { host: Some(host), .. } = &self.role else { return };
        let (host, msg) = (*host, Msg::Campfire { reference });
        for _ in 0..HIT_COPIES {
            self.sock_send(&msg, host);
        }
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

    /// What this player's table shows now (`None`: not at one). [`Session::tick`] sends it
    /// every [`TABLE_EVERY`], and at once when it changes; leaving says so
    /// ([`TABLE_LEFT`], [`HIT_COPIES`] times). One longer than [`TABLE_MAX`] is not sent.
    pub fn set_table(&mut self, view: Option<TableView>) {
        if view.as_ref().is_some_and(|v| v.blob.len() > TABLE_MAX || v.game == TABLE_LEFT || v.game & TABLE_ACTION != 0) {
            return;
        }
        if view == self.table {
            return;
        }
        let Some(view) = view else {
            if let Some(old) = self.table.take() {
                for _ in 0..HIT_COPIES {
                    self.send_table(old.table, TABLE_LEFT, Vec::new());
                }
            }
            return;
        };
        self.table = Some(view);
        self.last_table = None;
    }

    /// An action for a table's owner (its game's own encoding; `game` without
    /// [`TABLE_ACTION`]): to everyone through the host, [`HIT_COPIES`] times, taken once.
    pub fn send_table_action(&mut self, table: u32, game: u8, blob: Vec<u8>) {
        if blob.len() > TABLE_MAX {
            return;
        }
        let msg = self.table_msg(table, game | TABLE_ACTION, blob);
        for _ in 0..HIT_COPIES {
            self.to_all(msg.as_ref());
        }
    }

    fn send_table(&mut self, table: u32, game: u8, blob: Vec<u8>) {
        let msg = self.table_msg(table, game, blob);
        self.to_all(msg.as_ref());
    }

    fn table_msg(&mut self, table: u32, game: u8, blob: Vec<u8>) -> Option<Msg> {
        let id = self.id()?;
        self.table_seq = self.table_seq.wrapping_add(1);
        Some(Msg::Table { id, table, game, seq: self.table_seq, blob })
    }

    /// To every guest (the host), or to the host (a guest).
    fn to_all(&self, msg: Option<&Msg>) {
        let Some(msg) = msg else { return };
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

    /// Whether a table message from `id` is new: a view newer than the last, an action
    /// not yet taken.
    fn table_fresh(&mut self, id: u8, game: u8, seq: u32) -> bool {
        let i = match self.table_heard.iter().position(|h| h.0 == id) {
            Some(i) => i,
            None => {
                self.table_heard.push((id, None, VecDeque::new()));
                self.table_heard.len() - 1
            }
        };
        let h = &mut self.table_heard[i];
        if game & TABLE_ACTION != 0 {
            if h.2.contains(&seq) {
                return false;
            }
            if h.2.len() == ACTIONS_KEPT {
                h.2.pop_front();
            }
            h.2.push_back(seq);
            return true;
        }
        if h.1.is_some_and(|l| !fresh(seq, l)) {
            return false;
        }
        h.1 = Some(seq);
        true
    }

    /// This player revives player `target` (down): to the host, which passes it on, or
    /// from the host to that guest ([`HIT_COPIES`] times).
    pub fn send_revive(&mut self, target: u8, perks: u8) {
        let Some(by) = self.id() else { return };
        let msg = Msg::Revive { target, by, perks };
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

    /// This player plays an animation for an item: to the host, which passes it on, or
    /// from the host to every guest ([`HIT_COPIES`] times). Text longer than
    /// [`ANIM_TEXT_MAX`] is not sent.
    pub fn send_anim(&mut self, cue: AnimCue) {
        let Some(id) = self.id() else { return };
        if cue.edid.len() > ANIM_TEXT_MAX || cue.clip.len() > ANIM_TEXT_MAX {
            return;
        }
        self.anims.seq = self.anims.seq.wrapping_add(1);
        let msg = Msg::Anim { id, seq: self.anims.seq, cue };
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

    /// Shared progression ([`Progress`]): to the host, which passes it on, or from the
    /// host to every guest ([`HIT_COPIES`] times). A guest's [`Progress::host_only`] one is
    /// not sent.
    pub fn send_progress(&mut self, what: Progress) {
        let Some(id) = self.id() else { return };
        if what.host_only() && id != 0 {
            return;
        }
        self.progress.seq = self.progress.seq.wrapping_add(1);
        let msg = Msg::Progress { id, seq: self.progress.seq, what };
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

    /// Queue `item` on the reliable stream: a host's to guest `to` (None: every guest), a
    /// guest's to the host (`to` is ignored).
    pub fn send_reliable(&mut self, to: Option<u8>, item: Vec<u8>) {
        if self.is_host() {
            for p in self.peers.iter_mut().filter(|p| to.is_none_or(|t| t == p.id)) {
                p.rel.push(item.clone());
            }
        } else {
            self.rel.push(item);
        }
    }

    /// The host's moving actors for guest `to`, sent now.
    pub fn send_actors(&mut self, to: u8, poses: &[ActorPose]) {
        let Some(addr) = self.peers.iter().find(|p| p.id == to).and_then(|p| p.addr) else { return };
        self.actor_seq = self.actor_seq.wrapping_add(1);
        for chunk in poses.chunks(crate::repl::ACTORS_PER_PACKET) {
            self.sock_send(&Msg::Actors { seq: self.actor_seq, ms: self.started.elapsed().as_millis() as u32, poses: chunk.to_vec() }, addr);
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
        if self.is_host() && !self.peers.is_empty() && self.last_scene.as_ref().is_none_or(|(t, sent)| *sent != self.scene || now - *t >= SCENE_EVERY) {
            let msg = Msg::Scene(self.scene.clone());
            for addr in self.peers.iter().filter_map(|p| p.addr) {
                self.sock_send(&msg, addr);
            }
            self.last_scene = Some((now, self.scene.clone()));
        }
        if self.is_host() && !self.peers.is_empty() && self.last_names.is_none_or(|t| now - t >= NAMES_EVERY) {
            let names = std::iter::once((0, self.own_name.clone())).chain(self.peers.iter().map(|p| (p.id, p.name.clone()))).collect();
            let msg = Msg::Names { names };
            for addr in self.peers.iter().filter_map(|p| p.addr) {
                self.sock_send(&msg, addr);
            }
            self.last_names = Some(now);
        }
        if self.is_host() && !self.peers.is_empty() && self.last_presence.is_none_or(|t| now - t >= PRESENCE_EVERY) {
            for list in 0..LISTS {
                let l = &self.lists[list];
                let pages = l.refs.len().div_ceil(PRESENCE_PAGE).max(1);
                let page = l.page % pages;
                let refs = l.refs.iter().skip(page * PRESENCE_PAGE).take(PRESENCE_PAGE).copied().collect();
                let msg = Msg::Presence { list: list as u8, serial: l.serial, page: page as u16, pages: pages as u16, refs };
                for addr in self.peers.iter().filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
                self.lists[list].page = (page + 1) % pages;
            }
            self.last_presence = Some(now);
        }
        if self.is_host() && !self.peers.is_empty() && !self.story.is_empty() && self.last_story.is_none_or(|t| now - t >= STORY_EVERY) {
            let msg = Msg::Story { entries: self.story.page() };
            for addr in self.peers.iter().filter_map(|p| p.addr) {
                self.sock_send(&msg, addr);
            }
            self.last_story = Some(now);
        }
        if self.is_host() && !self.peers.is_empty() && !self.wear.is_empty() && self.last_wear.is_none_or(|t| now - t >= WEAR_EVERY) {
            let pages = self.wear.len().div_ceil(WEAR_PAGE);
            let page = self.wear_page % pages;
            let npcs = self.wear.iter().skip(page * WEAR_PAGE).take(WEAR_PAGE).cloned().collect();
            let msg = Msg::Wear { npcs };
            for addr in self.peers.iter().filter_map(|p| p.addr) {
                self.sock_send(&msg, addr);
            }
            self.wear_page = (page + 1) % pages;
            self.last_wear = Some(now);
        }
        if self.is_host() {
            let said: Vec<(u32, u32)> = self.lines.iter().map(|l| (l.speaker, l.info)).collect();
            let due = match &self.last_lines {
                None => !said.is_empty(),
                Some((t, sent)) => *sent != said || (!said.is_empty() && now - *t >= LINES_EVERY),
            };
            if due {
                self.lines_seq = self.lines_seq.wrapping_add(1);
                let msg = Msg::Lines { seq: self.lines_seq, lines: self.lines.clone() };
                for addr in self.peers.iter().filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
                self.last_lines = Some((now, said));
            }
        }
        if let Some(v) = self.table.clone() {
            if self.last_table.is_none_or(|t| now - t >= TABLE_EVERY) && self.id().is_some() {
                self.send_table(v.table, v.game, v.blob);
                self.last_table = Some(now);
            }
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
        let bytes = encode(msg);
        *self.sent.borrow_mut().entry(to).or_default() += bytes.len() as u64 + UDP_OVERHEAD;
        send_raw(&self.sock, self.steam, &bytes, to);
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
        *self.heard.entry(from).or_default() += bytes.len() as u64 + UDP_OVERHEAD;
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
                    p.name = truncate(&name);
                    self.last_names = None;
                    let id = p.id;
                    // A Hello starts the reliable stream again (the guest may have started
                    // again, or lost what was on the way).
                    p.epoch = p.epoch.wrapping_add(1);
                    p.rel = Reliable::new();
                    self.events.push(Event::RelReset { id });
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
                // A stream number unlike a guest's last one in this slot, and unlike the
                // one a host game before this one gave it (a guest coming back to a host
                // started again kept that one: queue item 201).
                let epoch = ((token as u16) ^ (new_token() as u16)) | 1;
                self.peers.push(Peer { addr: Some(from), token, name: truncate(&name), epoch, ..Peer::new(id, now) });
                self.last_names = None;
                self.sock_send(&Msg::Welcome { id }, from);
                self.events.push(Event::Joined { id, name });
                self.events.push(Event::RelReset { id });
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
            Msg::Campfire { reference } => {
                let Some(id) = self.peers.iter().find(|p| p.addr == Some(from)).map(|p| p.id) else { return };
                self.events.push(Event::Campfire { id, reference });
            }
            Msg::Door { reference, open } => {
                let Some(id) = self.peers.iter().find(|p| p.addr == Some(from)).map(|p| p.id) else { return };
                self.events.push(Event::Door { id, reference, open });
                let msg = Msg::Door { reference, open };
                for addr in self.peers.iter().filter(|p| p.id != id).filter_map(|p| p.addr) {
                    self.sock_send(&msg, addr);
                }
            }
            Msg::Revive { target, by, perks } => {
                // From a known guest only; for the host, or passed on to its guest.
                if !self.peers.iter().any(|p| p.addr == Some(from) && p.id == by) {
                    return;
                }
                if target == 0 {
                    self.events.push(Event::Revived { by, perks });
                } else if let Some(addr) = self.peers.iter().find(|p| p.id == target).and_then(|p| p.addr) {
                    self.sock_send(&Msg::Revive { target, by, perks }, addr);
                }
            }
            Msg::Table { table, game, seq, blob, .. } => {
                // The sender is the guest its address belongs to, whatever id it claims.
                let Some(id) = self.peers.iter().find(|p| p.addr == Some(from)).map(|p| p.id) else { return };
                if !self.table_fresh(id, game, seq) {
                    return;
                }
                let relay = Msg::Table { id, table, game, seq, blob: blob.clone() };
                for addr in self.peers.iter().filter(|p| p.id != id).filter_map(|p| p.addr) {
                    self.sock_send(&relay, addr);
                }
                self.events.push(Event::Table { id, table, game, blob });
            }
            Msg::Anim { id, seq, cue } => {
                // From the guest it names only; shown here and passed on to the others.
                if !self.peers.iter().any(|p| p.addr == Some(from) && p.id == id) || !self.anims.first(id, seq) {
                    return;
                }
                let msg = Msg::Anim { id, seq, cue: cue.clone() };
                let to: Vec<SocketAddr> = self.peers.iter().filter(|p| p.id != id).filter_map(|p| p.addr).collect();
                for addr in to {
                    for _ in 0..HIT_COPIES {
                        self.sock_send(&msg, addr);
                    }
                }
                self.events.push(Event::Anim { id, cue });
            }
            Msg::Progress { id, seq, what } => {
                // From the guest it names only, never a host's kind; taken here and passed
                // on to the others.
                if what.host_only() || !self.peers.iter().any(|p| p.addr == Some(from) && p.id == id) || !self.progress.first(id, seq) {
                    return;
                }
                let msg = Msg::Progress { id, seq, what: what.clone() };
                let to: Vec<SocketAddr> = self.peers.iter().filter(|p| p.id != id).filter_map(|p| p.addr).collect();
                for addr in to {
                    for _ in 0..HIT_COPIES {
                        self.sock_send(&msg, addr);
                    }
                }
                self.events.push(Event::Progress { id, what });
            }
            Msg::Rel { epoch, frame } => {
                let Some(p) = self.peers.iter_mut().find(|p| p.addr == Some(from)) else { return };
                // A frame of an older stream.
                if epoch != p.epoch {
                    return;
                }
                p.last_heard = now;
                let id = p.id;
                for item in p.rel.receive(frame) {
                    self.events.push(Event::Reliable { id, item });
                }
            }
            Msg::Welcome { .. } | Msg::Full | Msg::Who | Msg::Npcs { .. } | Msg::Quest(_) | Msg::Clock(_) | Msg::Scene(_) | Msg::Lines { .. } | Msg::Presence { .. } | Msg::Story { .. } | Msg::Wear { .. } | Msg::Names { .. } | Msg::Actors { .. } => {}
        }
    }

    /// A guest welcomed (again): what it heard from the host so far counts from the start
    /// again. The reliable stream both ways (the host's end is new too), the host's epoch,
    /// the newest rounds of actors, NPCs and lines, and the copies of hits, animations,
    /// progression and table actions taken (the copies go out at once, so none of an
    /// older stream is still on the way after the silence a Hello follows).
    fn hear_anew(&mut self) {
        // Before any stream was heard (the first Welcome), what is queued keeps (as the
        // first stream keeps it, `Msg::Rel`).
        if self.host_epoch.take().is_some() {
            self.rel = Reliable::new();
        }
        self.heard_actors = None;
        self.heard_npcs = None;
        self.heard_lines = None;
        self.heard_quest = None;
        self.heard_hits.clear();
        self.anims.heard.clear();
        self.progress.heard.clear();
        self.table_heard.clear();
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
                // A Welcome answers a Hello, and the host starts this guest's streams
                // again for each one (a known token), or never had them (a host started
                // again): so does this game (queue item 201: after the owner's host game
                // was closed and started again, the guest kept its old reliable stream and
                // its newest actor round, took nothing from the new host for 33 minutes,
                // and drew its own game's NPCs).
                self.hear_anew();
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
            // Every copy of the host's goodbye after the first finds it gone already.
            Msg::Bye { id: 0 } if *gone => {}
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
            Msg::Rel { epoch, frame } => {
                // A new stream (the host heard this guest's Hello): what was on the old
                // one is gone; a frame of one of the last few streams, late on the way, is
                // dropped. Any other number is a new stream (a host game started again
                // numbers its own, `host_handle`), so a Welcome lost on the way does not
                // leave this guest on the old one.
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
                        self.events.push(Event::RelReset { id: 0 });
                    }
                }
                for item in self.rel.receive(frame) {
                    self.events.push(Event::Reliable { id: 0, item });
                }
            }
            Msg::Actors { seq, ms, poses } => {
                // An older round is dropped; one far behind the last is a host started
                // again (as the NPC rounds and states take it, `fresh`).
                if self.heard_actors.is_some_and(|h| !fresh(seq, h) && seq != h) {
                    return;
                }
                self.heard_actors = Some(seq);
                self.events.push(Event::Actors { ms, here_ms: self.started.elapsed().as_millis() as u32, poses });
            }
            Msg::Unlock { reference, terminal } => self.events.push(Event::Unlocked { id: 0, reference, terminal }),
            Msg::Clock(c) => self.events.push(Event::Clock(c)),
            Msg::Scene(v) => self.events.push(Event::Scene(v)),
            Msg::Presence { list, serial, page, pages, refs } => self.events.push(Event::Presence { list, serial, page, pages, refs }),
            Msg::Story { entries } => self.events.push(Event::Story(entries)),
            Msg::Wear { npcs } => self.events.push(Event::Wear(npcs)),
            Msg::Door { reference, open } => self.events.push(Event::Door { id: 0, reference, open }),
            Msg::Names { names } => {
                for (peer, name) in names {
                    if Some(peer) == *id || name.is_empty() {
                        continue;
                    }
                    let p = match self.peers.iter().position(|p| p.id == peer) {
                        Some(i) => &mut self.peers[i],
                        None => {
                            self.peers.push(Peer::new(peer, now));
                            self.events.push(Event::Joined { id: peer, name: String::new() });
                            self.peers.last_mut().unwrap()
                        }
                    };
                    if p.name != name {
                        p.name = name.clone();
                        self.events.push(Event::Named { id: peer, name });
                    }
                }
            }
            Msg::Lines { seq, lines } => {
                if self.heard_lines.is_some_and(|l| !fresh(seq, l)) {
                    return;
                }
                self.heard_lines = Some(seq);
                self.events.push(Event::Lines(lines));
            }
            Msg::Revive { target, by, perks } => {
                if Some(target) == *id {
                    self.events.push(Event::Revived { by, perks });
                }
            }
            Msg::Table { id: peer, table, game, seq, blob } => {
                if Some(peer) == *id || !self.table_fresh(peer, game, seq) {
                    return;
                }
                self.events.push(Event::Table { id: peer, table, game, blob });
            }
            Msg::Anim { id: player, seq, cue } => {
                if Some(player) != *id && self.anims.first(player, seq) {
                    self.events.push(Event::Anim { id: player, cue });
                }
            }
            Msg::Progress { id: player, seq, what } => {
                if Some(player) != *id && (player == 0 || !what.host_only()) && self.progress.first(player, seq) {
                    self.events.push(Event::Progress { id: player, what });
                }
            }
            // A guest's request, for the host only.
            Msg::Hello { .. } | Msg::Ping | Msg::Campfire { .. } => {}
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

    /// Say goodbye, so the others drop this player at once rather than after [`GRACE`]:
    /// [`BYE_COPIES`] times, [`BYE_GAP`] apart, as a hit is sent more than once, so one
    /// lost packet does not leave the others waiting (queue item 90: a host that ends
    /// the lobby must reach every guest).
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

    pub fn send_revive(&self, target: u8, perks: u8) {
        self.with(|s| s.send_revive(target, perks));
    }

    pub fn set_clock(&self, clock: ClockView) {
        self.with(|s| s.set_clock(clock));
    }

    pub fn set_scene(&self, scene: SceneView) {
        self.with(|s| s.set_scene(scene));
    }

    pub fn set_name(&self, name: &str) {
        self.with(|s| s.set_name(name));
    }

    pub fn set_lines(&self, lines: Vec<LineView>) {
        self.with(|s| s.set_lines(lines));
    }

    pub fn set_presence(&self, list: u8, refs: Vec<(u32, bool)>) {
        self.with(|s| s.set_presence(list, refs));
    }

    /// [`Session::set_story`].
    pub fn set_story(&self, entries: Vec<StoryEntry>) {
        self.with(|s| s.set_story(entries));
    }

    /// [`Session::set_wear`].
    pub fn set_wear(&self, npcs: Vec<(u32, Vec<u32>)>) {
        self.with(|s| s.set_wear(npcs));
    }

    pub fn send_door(&self, reference: u32, open: bool) {
        self.with(|s| s.send_door(reference, open));
    }

    pub fn set_quest(&self, quest: QuestView) {
        self.with(|s| s.set_quest(quest));
    }

    pub fn send_unlock(&self, reference: u32, terminal: bool) {
        self.with(|s| s.send_unlock(reference, terminal));
    }

    pub fn send_campfire(&self, reference: u32) {
        self.with(|s| s.send_campfire(reference));
    }

    pub fn set_table(&self, view: Option<TableView>) {
        self.with(|s| s.set_table(view));
    }

    pub fn send_table_action(&self, table: u32, game: u8, blob: Vec<u8>) {
        self.with(|s| s.send_table_action(table, game, blob));
    }

    pub fn send_anim(&self, cue: AnimCue) {
        self.with(|s| s.send_anim(cue));
    }

    pub fn send_reliable(&self, to: Option<u8>, item: Vec<u8>) {
        self.with(|s| s.send_reliable(to, item))
    }

    pub fn send_actors(&self, to: u8, poses: &[ActorPose]) {
        self.with(|s| s.send_actors(to, poses))
    }

    pub fn traffic(&self) -> Vec<Traffic> {
        self.with(|s| s.traffic())
    }

    pub fn send_progress(&self, what: Progress) {
        self.with(|s| s.send_progress(what));
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
        Msg::Names { names } => {
            let names: Vec<_> = names.iter().take(MAX_GUESTS + 1).collect();
            b.extend_from_slice(&[21, names.len() as u8]);
            for (id, name) in names {
                let name = truncate(name);
                b.extend_from_slice(&[*id, name.len() as u8]);
                b.extend_from_slice(name.as_bytes());
            }
        }
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
            b.push(state.pipboy);
            b.extend_from_slice(&state.furniture.to_le_bytes());
            b.push(state.more);
            let e = &state.emote;
            b.extend_from_slice(&[e.id, e.variant, e.phase, e.step]);
            b.extend_from_slice(&e.t.to_le_bytes());
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
                b.extend_from_slice(&n.furniture.to_le_bytes());
                b.extend_from_slice(&n.base.to_le_bytes());
                b.extend_from_slice(&((n.level & 0x7FFF) | if n.hostile { 0x8000 } else { 0 }).to_le_bytes());
                b.push(n.rolls.min(127) | if n.all_levels { 128 } else { 0 });
                b.extend_from_slice(&n.space.to_le_bytes());
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
        Msg::Scene(v) => {
            let mods = &v.mods[..v.mods.len().min(SCENE_MODS_MAX)];
            b.extend_from_slice(&[24, v.locks, mods.len() as u8]);
            for (m, at) in mods {
                b.extend_from_slice(&m.to_le_bytes());
                b.extend_from_slice(&at.to_le_bytes());
            }
            let worn = &v.worn[..v.worn.len().min(SCENE_MODS_MAX)];
            b.push(worn.len() as u8);
            for (item, locked) in worn {
                b.extend_from_slice(&item.to_le_bytes());
                b.push(u8::from(*locked));
            }
            let swaps: Vec<_> = v.swaps.iter().filter(|(_, s, t)| s.len() <= SWAP_TEXT_MAX && t.len() <= SWAP_TEXT_MAX).take(SCENE_SWAPS_MAX).collect();
            b.push(swaps.len() as u8);
            for (on, shape, texture) in swaps {
                b.extend_from_slice(&on.to_le_bytes());
                for s in [shape, texture] {
                    b.push(s.len() as u8);
                    b.extend_from_slice(s.as_bytes());
                }
            }
            match &v.waiting {
                Some((npc, far)) => {
                    b.extend_from_slice(&npc.to_le_bytes());
                    let far = &far[..far.len().min(MAX_GUESTS)];
                    b.push(far.len() as u8);
                    b.extend_from_slice(far);
                }
                None => b.extend_from_slice(&0u32.to_le_bytes()),
            }
        }
        Msg::Lines { seq, lines } => {
            b.push(17);
            b.extend_from_slice(&seq.to_le_bytes());
            b.push(lines.len() as u8);
            for l in lines {
                for v in [l.speaker, l.target, l.topic, l.info] {
                    b.extend_from_slice(&v.to_le_bytes());
                }
                b.extend_from_slice(&l.remaining.to_le_bytes());
                b.push(l.flags);
                b.extend_from_slice(&l.conversation.to_le_bytes());
            }
        }
        Msg::Presence { list, serial, page, pages, refs } => {
            b.push(19);
            b.push(*list);
            b.extend_from_slice(&serial.to_le_bytes());
            b.extend_from_slice(&page.to_le_bytes());
            b.extend_from_slice(&pages.to_le_bytes());
            b.extend_from_slice(&(refs.len() as u16).to_le_bytes());
            for (r, on) in refs {
                b.extend_from_slice(&r.to_le_bytes());
                b.push(u8::from(*on));
            }
        }
        Msg::Story { entries } => {
            b.push(25);
            b.extend_from_slice(&(entries.len() as u16).to_le_bytes());
            for e in entries {
                match e {
                    StoryEntry::Quest { quest, stage, running, ended, done, failed, objectives, seen } => {
                        b.push(0);
                        b.extend_from_slice(&quest.to_le_bytes());
                        b.extend_from_slice(&stage.to_le_bytes());
                        b.push(u8::from(*running) | (u8::from(*ended) << 1) | (u8::from(*failed) << 2));
                        let done = &done[done.len().saturating_sub(story::STAGES)..];
                        b.push(done.len() as u8);
                        for d in done {
                            b.extend_from_slice(&d.to_le_bytes());
                        }
                        let objectives = &objectives[..objectives.len().min(story::OBJECTIVES)];
                        b.push(objectives.len() as u8);
                        for (i, done) in objectives {
                            b.extend_from_slice(&i.to_le_bytes());
                            b.push(u8::from(*done));
                        }
                        b.extend_from_slice(&seen.to_le_bytes());
                    }
                    StoryEntry::Var { quest, index, value } => {
                        b.push(1);
                        b.extend_from_slice(&quest.to_le_bytes());
                        b.extend_from_slice(&index.to_le_bytes());
                        match value {
                            StoryValue::Num(v) => {
                                b.push(0);
                                b.extend_from_slice(&v.to_bits().to_le_bytes());
                            }
                            StoryValue::Ref(r) => {
                                b.push(1);
                                b.extend_from_slice(&r.to_le_bytes());
                            }
                        }
                    }
                    StoryEntry::Swap { reference, shape, texture } => {
                        b.push(2);
                        b.extend_from_slice(&reference.to_le_bytes());
                        for t in [story::swap_text(shape), story::swap_text(texture)] {
                            b.push(t.len() as u8);
                            b.extend_from_slice(t.as_bytes());
                        }
                    }
                }
            }
        }
        Msg::Campfire { reference } => {
            b.push(22);
            b.extend_from_slice(&reference.to_le_bytes());
        }
        Msg::Wear { npcs } => {
            b.push(23);
            b.push(npcs.len().min(WEAR_PAGE) as u8);
            for (r, items) in npcs.iter().take(WEAR_PAGE) {
                b.extend_from_slice(&r.to_le_bytes());
                b.push(items.len().min(WEAR_ITEMS) as u8);
                for i in items.iter().take(WEAR_ITEMS) {
                    b.extend_from_slice(&i.to_le_bytes());
                }
            }
        }
        Msg::Door { reference, open } => {
            b.push(20);
            b.extend_from_slice(&reference.to_le_bytes());
            b.push(u8::from(*open));
        }
        Msg::Revive { target, by, perks } => {
            b.push(14);
            b.push(*target);
            b.push(*by);
            b.push(*perks);
        }
        Msg::Table { id, table, game, seq, blob } => {
            b.extend_from_slice(&[15, *id]);
            b.extend_from_slice(&table.to_le_bytes());
            b.push(*game);
            b.extend_from_slice(&seq.to_le_bytes());
            b.extend_from_slice(&(blob.len() as u16).to_le_bytes());
            b.extend_from_slice(blob);
        }
        Msg::Anim { id, seq, cue } => {
            b.extend_from_slice(&[16, *id]);
            b.extend_from_slice(&seq.to_le_bytes());
            b.extend_from_slice(&[cue.kind, cue.style, u8::from(cue.power_armor)]);
            b.extend_from_slice(&cue.item.to_le_bytes());
            for t in [&cue.edid, &cue.clip] {
                let n = t.len().min(ANIM_TEXT_MAX);
                b.push(n as u8);
                b.extend_from_slice(&t.as_bytes()[..n]);
            }
        }
        Msg::Progress { id, seq, what } => {
            b.extend_from_slice(&[18, *id]);
            b.extend_from_slice(&seq.to_le_bytes());
            match what {
                Progress::Reward { quest, xp, caps } => {
                    b.push(0);
                    for v in [quest, xp, caps] {
                        b.extend_from_slice(&v.to_le_bytes());
                    }
                }
                Progress::Save { stamp, kind } => {
                    b.push(1);
                    b.extend_from_slice(&stamp.to_le_bytes());
                    b.push(*kind);
                }
                Progress::Reload { stamp } => {
                    b.push(2);
                    b.extend_from_slice(&stamp.to_le_bytes());
                }
                Progress::Gift { batch, item, count, quest, stage } => {
                    b.push(4);
                    for v in [batch, item, count, quest, stage] {
                        b.extend_from_slice(&v.to_le_bytes());
                    }
                }
                Progress::QuestEnded { quest, failed } => {
                    b.push(5);
                    b.extend_from_slice(&quest.to_le_bytes());
                    b.push(*failed as u8);
                }
                Progress::JointChoice { character, on } => {
                    b.push(6);
                    b.extend_from_slice(&character.to_le_bytes());
                    b.push(*on as u8);
                }
                Progress::Stage { quest, stage, line, line_end, prior, to } => {
                    b.push(7);
                    b.extend_from_slice(&quest.to_le_bytes());
                    b.extend_from_slice(&stage.to_le_bytes());
                    b.extend_from_slice(&line.to_le_bytes());
                    b.push(*line_end as u8);
                    b.push(*to);
                    let n = prior.len().min(255);
                    b.push(n as u8);
                    for s in &prior[..n] {
                        b.extend_from_slice(&s.to_le_bytes());
                    }
                }
                Progress::StageResult { quest, stage, taken } => {
                    b.push(8);
                    b.extend_from_slice(&quest.to_le_bytes());
                    b.extend_from_slice(&stage.to_le_bytes());
                    b.push(*taken as u8);
                }
                Progress::KillCredit { to, target, target_base, npc, xp, karma, killer, infamy } => {
                    b.push(9);
                    b.push(*to);
                    b.extend_from_slice(&target.to_le_bytes());
                    b.extend_from_slice(&target_base.to_le_bytes());
                    b.push(*npc as u8);
                    b.extend_from_slice(&xp.to_le_bytes());
                    b.extend_from_slice(&karma.to_le_bytes());
                    b.push(*killer as u8);
                    let n = infamy.len().min(255);
                    b.push(n as u8);
                    for (r, d) in &infamy[..n] {
                        b.extend_from_slice(&r.to_le_bytes());
                        b.extend_from_slice(&d.to_le_bytes());
                    }
                }
                Progress::HitCredit { to, target, weapon, part, dealt, critical, crippled } => {
                    b.push(10);
                    b.push(*to);
                    for v in [target, weapon, part, dealt] {
                        b.extend_from_slice(&v.to_le_bytes());
                    }
                    b.push(*critical as u8);
                    b.push(*crippled as u8);
                }
            }
        }
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
        Msg::Actors { seq, ms, poses } => {
            b.push(31);
            b.extend_from_slice(&seq.to_le_bytes());
            b.extend_from_slice(&ms.to_le_bytes());
            b.push(poses.len().min(crate::repl::ACTORS_PER_PACKET) as u8);
            for p in poses.iter().take(crate::repl::ACTORS_PER_PACKET) {
                p.write(&mut b);
            }
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
        30 => {
            let epoch = r.u16()?;
            let ack = r.u32()?;
            let first = r.u32()?;
            let n = r.u16()? as usize;
            if n > 1500 {
                return None;
            }
            let mut items = Vec::with_capacity(n);
            for _ in 0..n {
                let len = r.u16()? as usize;
                items.push(r.take(len)?.to_vec());
            }
            Msg::Rel { epoch, frame: Frame { ack, first, items } }
        }
        31 => {
            let seq = r.u32()?;
            let ms = r.u32()?;
            let n = r.u8()? as usize;
            if n > crate::repl::ACTORS_PER_PACKET {
                return None;
            }
            let bytes = r.take(n * crate::repl::ACTOR_BYTES)?;
            let mut pr = crate::repl::Reader(bytes);
            let poses = (0..n).map(|_| ActorPose::read(&mut pr)).collect::<Option<Vec<_>>>()?;
            Msg::Actors { seq, ms, poses }
        }
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
            state.pipboy = r.u8()?;
            state.furniture = r.u32()?;
            state.more = r.u8()?;
            state.emote = EmoteState { id: r.u8()?, variant: r.u8()?, phase: r.u8()?, step: r.u8()?, t: r.f32()? };
            if !state.emote.t.is_finite() {
                return None;
            }
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
                let (flags, weapon, attacks, furniture, base, level, look, space) = (r.u8()?, r.u32()?, r.u8()?, r.u32()?, r.u32()?, r.u16()?, r.u8()?, r.u32()?);
                npcs.push(NpcState { reference, feet, heading, pitch, flags, weapon, attacks, furniture, base, level: level & 0x7FFF, all_levels: look & 128 != 0, rolls: look & 127, hostile: level & 0x8000 != 0, space });
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
        // A build before the perks byte sends two bytes.
        14 => Msg::Revive { target: r.u8()?, by: r.u8()?, perks: r.u8().unwrap_or(0) },
        15 => {
            let (id, table, game, seq) = (r.u8()?, r.u32()?, r.u8()?, r.u32()?);
            let len = r.u16()? as usize;
            if len > TABLE_MAX {
                return None;
            }
            Msg::Table { id, table, game, seq, blob: r.take(len)?.to_vec() }
        }
        16 => {
            let (id, seq) = (r.u8()?, r.u16()?);
            let (kind, style, power_armor, item) = (r.u8()?, r.u8()?, r.u8()? != 0, r.u32()?);
            let mut text = || -> Option<String> {
                let len = r.u8()? as usize;
                if len > ANIM_TEXT_MAX {
                    return None;
                }
                Some(String::from_utf8_lossy(r.take(len)?).into_owned())
            };
            let (edid, clip) = (text()?, text()?);
            Msg::Anim { id, seq, cue: AnimCue { kind, style, power_armor, item, edid, clip } }
        }
        18 => {
            let (id, seq) = (r.u8()?, r.u16()?);
            let what = match r.u8()? {
                0 => Progress::Reward { quest: r.u32()?, xp: r.u32()?, caps: r.u32()? },
                1 => Progress::Save { stamp: r.u32()?, kind: r.u8()? },
                2 => Progress::Reload { stamp: r.u32()? },
                // Before quest and stage were sent (a build of 2026-11-13 or older).
                3 => Progress::Gift { batch: r.u32()?, item: r.u32()?, count: r.u32()?, quest: 0, stage: Progress::NO_STAGE },
                4 => Progress::Gift { batch: r.u32()?, item: r.u32()?, count: r.u32()?, quest: r.u32()?, stage: r.u32()? },
                5 => Progress::QuestEnded { quest: r.u32()?, failed: r.u8()? != 0 },
                6 => Progress::JointChoice { character: u64::from(r.u32()?) | (u64::from(r.u32()?) << 32), on: r.u8()? != 0 },
                7 => {
                    let (quest, stage, line, line_end, to) = (r.u32()?, r.u32()? as i32, r.u32()?, r.u8()? != 0, r.u8()?);
                    let n = r.u8()?;
                    let prior = (0..n).map(|_| r.u32().map(|s| s as i32)).collect::<Option<Vec<_>>>()?;
                    Progress::Stage { quest, stage, line, line_end, prior, to }
                }
                8 => Progress::StageResult { quest: r.u32()?, stage: r.u32()? as i32, taken: r.u8()? != 0 },
                9 => {
                    let (to, target, target_base, npc, xp, karma, killer) = (r.u8()?, r.u32()?, r.u32()?, r.u8()? != 0, r.u32()?, r.u32()? as i32, r.u8()? != 0);
                    let n = r.u8()?;
                    let infamy = (0..n).map(|_| Some((r.u32()?, r.u32()? as i32))).collect::<Option<Vec<_>>>()?;
                    Progress::KillCredit { to, target, target_base, npc, xp, karma, killer, infamy }
                }
                10 => Progress::HitCredit { to: r.u8()?, target: r.u32()?, weapon: r.u32()?, part: r.u32()?, dealt: r.u32()?, critical: r.u8()? != 0, crippled: r.u8()? != 0 },
                _ => return None,
            };
            Msg::Progress { id, seq, what }
        }
        13 => Msg::Clock(ClockView { year: r.f32()?, month: r.f32()?, day: r.f32()?, hour: r.f32()?, days_passed: r.f32()? }),
        24 => {
            let locks = r.u8()?;
            let n = r.u8()? as usize;
            if n > SCENE_MODS_MAX {
                return None;
            }
            let mut mods = Vec::with_capacity(n);
            for _ in 0..n {
                mods.push((r.u32()?, r.f32()?));
            }
            let n = r.u8()? as usize;
            if n > SCENE_MODS_MAX {
                return None;
            }
            let mut worn = Vec::with_capacity(n);
            for _ in 0..n {
                worn.push((r.u32()?, r.u8()? != 0));
            }
            let n = r.u8()? as usize;
            if n > SCENE_SWAPS_MAX {
                return None;
            }
            let mut swaps = Vec::with_capacity(n);
            for _ in 0..n {
                let on = r.u32()?;
                let mut text = || -> Option<String> {
                    let len = r.u8()? as usize;
                    if len > SWAP_TEXT_MAX {
                        return None;
                    }
                    Some(String::from_utf8_lossy(r.take(len)?).into_owned())
                };
                let (shape, texture) = (text()?, text()?);
                swaps.push((on, shape, texture));
            }
            let npc = r.u32()?;
            let waiting = if npc == 0 {
                None
            } else {
                let n = r.u8()? as usize;
                if n > MAX_GUESTS {
                    return None;
                }
                Some((npc, r.take(n)?.to_vec()))
            };
            Msg::Scene(SceneView { locks, mods, worn, swaps, waiting })
        }
        17 => {
            let seq = r.u32()?;
            let n = r.u8()? as usize;
            if n > LINES_MAX {
                return None;
            }
            let mut lines = Vec::with_capacity(n);
            for _ in 0..n {
                let (speaker, target, topic, info) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
                let remaining = r.f32()?;
                if !remaining.is_finite() {
                    return None;
                }
                lines.push(LineView { speaker, target, topic, info, remaining, flags: r.u8()?, conversation: r.u16()? });
            }
            Msg::Lines { seq, lines }
        }
        19 => {
            let (list, serial, page, pages, n) = (r.u8()?, r.u32()?, r.u16()?, r.u16()?, r.u16()? as usize);
            if n > PRESENCE_PAGE || page >= pages || list as usize >= LISTS {
                return None;
            }
            let mut refs = Vec::with_capacity(n);
            for _ in 0..n {
                refs.push((r.u32()?, r.u8()? != 0));
            }
            Msg::Presence { list, serial, page, pages, refs }
        }
        20 => Msg::Door { reference: r.u32()?, open: r.u8()? != 0 },
        25 => {
            let n = r.u16()? as usize;
            if n > story::PAGE_BYTES {
                return None;
            }
            let mut entries = Vec::with_capacity(n);
            for _ in 0..n {
                entries.push(match r.u8()? {
                    0 => {
                        let (quest, stage, flags, k) = (r.u32()?, r.u16()? as i16, r.u8()?, r.u8()? as usize);
                        if k > story::STAGES {
                            return None;
                        }
                        let mut done = Vec::with_capacity(k);
                        for _ in 0..k {
                            done.push(r.u16()? as i16);
                        }
                        let o = r.u8()? as usize;
                        if o > story::OBJECTIVES {
                            return None;
                        }
                        let mut objectives = Vec::with_capacity(o);
                        for _ in 0..o {
                            objectives.push((r.u16()? as i16, r.u8()? != 0));
                        }
                        StoryEntry::Quest { quest, stage, running: flags & 1 != 0, ended: flags & 2 != 0, done, failed: flags & 4 != 0, objectives, seen: r.u32()? }
                    }
                    1 => {
                        let (quest, index) = (r.u32()?, r.u16()?);
                        let value = match r.u8()? {
                            0 => StoryValue::Num(f64::from_bits(r.u64()?)),
                            1 => StoryValue::Ref(r.u32()?),
                            _ => return None,
                        };
                        StoryEntry::Var { quest, index, value }
                    }
                    2 => {
                        let reference = r.u32()?;
                        let n = r.u8()? as usize;
                        let shape = String::from_utf8_lossy(r.take(n)?).into_owned();
                        let n = r.u8()? as usize;
                        let texture = String::from_utf8_lossy(r.take(n)?).into_owned();
                        StoryEntry::Swap { reference, shape, texture }
                    }
                    _ => return None,
                });
            }
            Msg::Story { entries }
        }
        22 => Msg::Campfire { reference: r.u32()? },
        23 => {
            let n = r.u8()? as usize;
            if n > WEAR_PAGE {
                return None;
            }
            let mut npcs = Vec::with_capacity(n);
            for _ in 0..n {
                let reference = r.u32()?;
                let k = r.u8()? as usize;
                if k > WEAR_ITEMS {
                    return None;
                }
                let mut items = Vec::with_capacity(k);
                for _ in 0..k {
                    items.push(r.u32()?);
                }
                npcs.push((reference, items));
            }
            Msg::Wear { npcs }
        }
        21 => {
            let n = r.u8()? as usize;
            if n > MAX_GUESTS + 1 {
                return None;
            }
            let mut names = Vec::with_capacity(n);
            for _ in 0..n {
                let id = r.u8()?;
                let len = r.u8()? as usize;
                if len > NAME_MAX {
                    return None;
                }
                names.push((id, String::from_utf8_lossy(r.take(len)?).into_owned()));
            }
            Msg::Names { names }
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
            Msg::Names { names: vec![(0, "Alpha".into()), (2, "Bravo".into()), (3, String::new())] },
            Msg::Full,
            Msg::Who,
            Msg::Ping,
            Msg::Look { id: 2, look: vec![1, 2, 3, 250] },
            Msg::Hit { id: 65_000, hit: vec![9, 8, 7] },
            Msg::Quest(QuestView { quest: 0x0010_4C1D, completed: false, failed: true, objectives: vec![(10, true), (-1, false), (200, false)] }),
            Msg::Quest(QuestView::default()),
            Msg::Unlock { reference: 0x0001_2345, terminal: true },
            Msg::Clock(ClockView { year: 2281.0, month: 9.0, day: 19.0, hour: 13.25, days_passed: 6.05 }),
            Msg::Scene(SceneView::default()),
            Msg::Scene(SceneView { locks: 0x7F, mods: vec![(0x0101_013B, 0.25), (0x0001_5D2B, 3.5)], worn: vec![(0x0100_F3A4, true), (0x0100_6E8F, false)], swaps: vec![(0x0101_38B2, "nvdlc01_intromovie00:0".into(), "textures/nvdlc01/slide03.dds".into())], waiting: None }),
            Msg::Scene(SceneView { locks: 0, mods: vec![], worn: vec![], swaps: vec![], waiting: Some((0x0100_9E7C, vec![1, 3])) }),
            Msg::Revive { target: 2, by: 0, perks: revive_perk::CURE_RADS },
            Msg::Presence { list: LIST_ENABLED, serial: 7, page: 0, pages: 1, refs: Vec::new() },
            Msg::Presence { list: LIST_OPEN, serial: 8, page: 2, pages: 3, refs: vec![(0x0010_A202, true), (0x0016_8A92, false)] },
            Msg::Door { reference: 0x0010_C0DE, open: true },
            Msg::Story { entries: Vec::new() },
            Msg::Story {
                entries: vec![
                    StoryEntry::Quest { quest: 0x1E00_2B11, stage: 20, running: true, ended: false, done: vec![10, 20], failed: false, objectives: vec![(10, true), (20, false)], seen: 7 },
                    StoryEntry::Quest { quest: 0x0010_4C1D, stage: -1, running: false, ended: true, done: Vec::new(), failed: true, objectives: Vec::new(), seen: 0 },
                    StoryEntry::Var { quest: 0x1E00_2B11, index: 1, value: StoryValue::Num(1.0) },
                    StoryEntry::Var { quest: 0x1E00_2B11, index: 2, value: StoryValue::Num(-0.125) },
                    StoryEntry::Var { quest: 0x1E00_2B11, index: 900, value: StoryValue::Ref(0x0010_C0DE) },
                    StoryEntry::Swap { reference: 0x0010_3DFE, shape: "screen01:0".into(), texture: r"textures\creatures\securitron\victor_alert.dds".into() },
                ],
            },
            Msg::Campfire { reference: 0x000C_DB59 },
            Msg::Presence { list: LIST_CAMPFIRE, serial: 1, page: 0, pages: 1, refs: vec![(0x000C_DB59, true)] },
            Msg::Wear { npcs: vec![(0x0010_C0DE, vec![0x0001_5EB6, 0x0008_F2E1]), (0xFF00_0643, Vec::new())] },
            Msg::Lines { seq: 3, lines: Vec::new() },
            Msg::Lines {
                seq: u32::MAX,
                lines: vec![
                    LineView { speaker: 0x0010_4C3A, target: LineView::HOST, topic: 0x0011_2233, info: 0x0011_2244, remaining: 3.25, flags: LineView::SAY_TO | LineView::TWO_D, conversation: 0 },
                    LineView { speaker: 0x0010_4C1D, target: 0x0010_4C3A, topic: 1, info: 2, remaining: 0.5, flags: 0, conversation: 4 },
                ],
            },
            Msg::Table { id: 1, table: 0x0010_C1DB, game: 3, seq: 77, blob: vec![1, 2, 3] },
            Msg::Table { id: 0, table: 7, game: 2 | TABLE_ACTION, seq: u32::MAX, blob: Vec::new() },
            Msg::Table { id: 3, table: 7, game: 1, seq: 1, blob: vec![0xAB; TABLE_MAX] },
            Msg::Anim { id: 1, seq: 65_535, cue: AnimCue { kind: AnimCue::CONSUME, style: 2, power_armor: false, item: 0x0001_5169, edid: "Stimpak".into(), clip: "meshes\\characters\\_male\\idleanims\\b42inject\\0\\1\\stimpak\\stimpak.kf".into() } },
            Msg::Anim { id: 0, seq: 0, cue: AnimCue::default() },
            Msg::Progress { id: 0, seq: 9, what: Progress::Reward { quest: 0x0010_4C1D, xp: 250, caps: 100 } },
            Msg::Progress { id: 0, seq: 10, what: Progress::Gift { batch: 3, item: 0x0011_A208, count: 24, quest: 0x0010_4C1E, stage: 50 } },
            Msg::Progress { id: 0, seq: 11, what: Progress::Gift { batch: 4, item: 0x0015_9E04, count: 1, quest: 0, stage: Progress::NO_STAGE } },
            Msg::Progress { id: 0, seq: 12, what: Progress::QuestEnded { quest: 0x0010_4C1F, failed: false } },
            Msg::Progress { id: 0, seq: 13, what: Progress::QuestEnded { quest: 0x0010_4C1F, failed: true } },
            Msg::Progress { id: 2, seq: 14, what: Progress::JointChoice { character: 0x0123_4567_89AB_CDEF, on: true } },
            Msg::Progress { id: 0, seq: 15, what: Progress::Stage { quest: 0x0011_76B8, stage: 100, line: 0x0013_E51E, line_end: true, prior: vec![1, 10, 20, 50], to: 0b0110 } },
            Msg::Progress { id: 1, seq: 16, what: Progress::StageResult { quest: 0x0011_76B8, stage: 100, taken: false } },
            Msg::Progress { id: 0, seq: 17, what: Progress::KillCredit { to: 1, target: 0x0010_4C1D, target_base: 0x0010_4C1E, npc: true, xp: 10, karma: -5, killer: true, infamy: vec![(0x0010_4C2A, 30)] } },
            Msg::Progress { id: 0, seq: 18, what: Progress::HitCredit { to: 2, target: 0x0010_4C1D, weapon: 0x0E3778, part: u32::MAX, dealt: 12, critical: true, crippled: false } },
            Msg::Progress { id: 2, seq: 65_535, what: Progress::Save { stamp: u32::MAX, kind: Progress::SYSTEM } },
            Msg::Progress { id: 0, seq: 0, what: Progress::Reload { stamp: 7 } },
            Msg::Npcs {
                seq: 12,
                npcs: vec![
                    NpcState { reference: 0x0010_4C3A, feet: [1.0, -2.5, 300.0], heading: 1.5, pitch: -0.2, flags: npc_flag::DRAWN | npc_flag::AIMING, weapon: 0x0E3778, attacks: 255, furniture: 0, base: 0x0010_4C39, level: 12, all_levels: true, rolls: 3, hostile: true, space: 0x0010_4C00 },
                    NpcState { reference: 7, feet: [0.0; 3], heading: 0.0, pitch: 0.0, flags: npc_flag::DEAD | npc_flag::SEATED, weapon: 0, attacks: 0, furniture: 0x0010_C055, base: 0, level: 0, all_levels: false, rolls: 0, hostile: false, space: 0 },
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

    /// Replication's streams (docs/design/coop-replication.md): the host's reliable items
    /// reach a guest once and in order, the guest's reach the host, the compact actor
    /// stream goes to the guest it is meant for, and the traffic is counted.
    #[test]
    fn the_reliable_stream_and_the_actor_stream_reach_their_guest() {
        let (mut host, mut guests, _) = host_and(2);
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_))));
        assert!(seen[0].iter().filter(|e| matches!(e, Event::RelReset { .. })).count() >= 2);
        let ida = a.id().unwrap();
        for i in 0..50u8 {
            host.send_reliable(Some(ida), vec![7, i]);
        }
        host.send_reliable(None, vec![8]);
        let pose = ActorPose { reference: 0x1234, feet: [10.0, 20.0, 30.0], heading: 1.0, pitch: 0.0, flags: 1, attacks: 0 };
        host.send_actors(ida, &[pose]);
        a.send_reliable(None, vec![9, 9]);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| {
            seen[1].iter().filter(|e| matches!(e, Event::Reliable { .. })).count() == 51 && seen[0].iter().any(|e| matches!(e, Event::Reliable { .. })) && seen[2].iter().any(|e| matches!(e, Event::Reliable { .. }))
        });
        let at_a: Vec<Vec<u8>> = seen[1].iter().filter_map(|e| if let Event::Reliable { id: 0, item } = e { Some(item.clone()) } else { None }).collect();
        let mut want: Vec<Vec<u8>> = (0..50u8).map(|i| vec![7, i]).collect();
        want.push(vec![8]);
        assert_eq!(at_a, want);
        assert!(seen[0].iter().any(|e| *e == Event::Reliable { id: ida, item: vec![9, 9] }));
        assert!(seen[1].iter().any(|e| matches!(e, Event::Actors { poses, .. } if poses[0].reference == 0x1234)));
        assert!(!seen[2].iter().any(|e| matches!(e, Event::Actors { .. })));
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

    /// A guest's save reaches the host and the other guest once; the host's reward and
    /// reload reach every guest; a guest's reward is not sent.
    #[test]
    fn progress_reaches_everyone_once() {
        let (mut host, mut guests, _) = host_and(2);
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let ida = a.id().unwrap();
        let progress = |v: &[Event]| v.iter().filter(|e| matches!(e, Event::Progress { .. })).cloned().collect::<Vec<_>>();
        // A guest may not pay the others.
        a.send_progress(Progress::Reward { quest: 1, xp: 1, caps: 1 });
        a.send_progress(Progress::Save { stamp: 42, kind: Progress::QUICK });
        let mut seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| !progress(&seen[0]).is_empty() && !progress(&seen[2]).is_empty());
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(3));
            clock += step;
            for (i, s) in [&mut host, &mut *a, &mut *b].into_iter().enumerate() {
                s.tick(clock);
                seen[i].extend(s.take_events());
            }
        }
        let saved = Event::Progress { id: ida, what: Progress::Save { stamp: 42, kind: Progress::QUICK } };
        assert_eq!(progress(&seen[0]), std::slice::from_ref(&saved));
        assert_eq!(progress(&seen[2]), [saved]);
        assert!(progress(&seen[1]).is_empty(), "not back to its sender");
        let reward = Progress::Reward { quest: 0x0010_4C1D, xp: 250, caps: 100 };
        host.send_progress(reward.clone());
        host.send_progress(Progress::Reload { stamp: 9 });
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| progress(&seen[1]).len() == 2 && progress(&seen[2]).len() == 2);
        assert_eq!(progress(&seen[1]), [Event::Progress { id: 0, what: reward }, Event::Progress { id: 0, what: Progress::Reload { stamp: 9 } }]);
    }

    /// Each guest learns the host's name and the other guest's, never its own.
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
        a.send_revive(idb, revive_perk::CURE_RADS);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[2].iter().any(|e| matches!(e, Event::Revived { .. })));
        assert!(seen[2].contains(&Event::Revived { by: ida, perks: revive_perk::CURE_RADS }));
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Revived { .. })));
        b.send_revive(0, 0);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[0].iter().any(|e| matches!(e, Event::Revived { .. })));
        assert!(seen[0].contains(&Event::Revived { by: idb, perks: 0 }));
        host.send_revive(ida, 0);
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Revived { .. })));
        assert!(seen[1].contains(&Event::Revived { by: 0, perks: 0 }));
    }

    /// The host's NPCs' lines reach every guest, again while they last and at once when
    /// they change; an empty list goes once, so the lines end on the guests too.
    #[test]
    fn the_hosts_lines_reach_the_guests() {
        let (mut host, mut guests, _) = host_and(2);
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let line = LineView { speaker: 0x0010_4C3A, target: 0x0010_4C1D, topic: 1, info: 2, remaining: 4.0, flags: 0, conversation: 1 };
        host.set_lines(vec![line]);
        let heard = |seen: &[Event], want: &[LineView]| seen.iter().any(|e| *e == Event::Lines(want.to_vec()));
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| heard(&seen[1], &[line]) && heard(&seen[2], &[line]));
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Lines(_))), "the host does not hear its own");
        // Sent again while it lasts.
        run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().filter(|e| matches!(e, Event::Lines(_))).count() >= 2);
        host.set_lines(Vec::new());
        run(&mut [&mut host, a, b], &mut clock, step, |seen| heard(&seen[1], &[]) && heard(&seen[2], &[]));
        // Once only: nothing is said now.
        let mut ticks = 0;
        let quiet = run(&mut [&mut host, a, b], &mut clock, step, |_| {
            ticks += 1;
            ticks > 40
        });
        assert!(!quiet[1].iter().any(|e| matches!(e, Event::Lines(_))), "{:?}", quiet[1]);
    }

    /// The host's world's presence reaches the guests page by page, round and round, a
    /// new serial when it changes; an empty one goes too (a guest then knows the host
    /// changed nothing).
    #[test]
    fn the_hosts_presence_reaches_the_guests_page_by_page() {
        type Page = (u16, u16, Vec<(u32, bool)>);
        let (mut host, mut guests, _) = host_and(1);
        let a = &mut guests[0];
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        run(&mut [&mut host, a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let refs: Vec<(u32, bool)> = (0..PRESENCE_PAGE as u32 + 5).map(|i| (0x0010_0000 + i, i % 2 == 0)).collect();
        host.set_presence(LIST_ENABLED, refs.clone());
        let pages = |seen: &[Event], serial: u32| -> Vec<Page> {
            seen.iter()
                .filter_map(|e| match e {
                    Event::Presence { list: LIST_ENABLED, serial: s, page, pages, refs } if *s == serial => Some((*page, *pages, refs.clone())),
                    _ => None,
                })
                .collect()
        };
        let seen = run(&mut [&mut host, a], &mut clock, step, |seen| pages(&seen[1], 1).len() >= 3);
        let got = pages(&seen[1], 1);
        assert_eq!((got[0].0, got[0].1, got[1].0, got[2].0), (0, 2, 1, 0), "round and round");
        assert_eq!([got[0].2.clone(), got[1].2.clone()].concat(), refs);
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Presence { .. })), "not to the host");
        host.set_presence(LIST_ENABLED, Vec::new());
        let seen = run(&mut [&mut host, a], &mut clock, step, |seen| !pages(&seen[1], 2).is_empty());
        assert_eq!(pages(&seen[1], 2)[0], (0, 1, Vec::new()));
        // The doors' list goes beside it, with its own serial.
        host.set_presence(LIST_OPEN, vec![(0x0010_C0DE, true)]);
        let seen = run(&mut [&mut host, a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Presence { list: LIST_OPEN, serial: 1, .. })));
        assert!(seen[1].contains(&Event::Presence { list: LIST_OPEN, serial: 1, page: 0, pages: 1, refs: vec![(0x0010_C0DE, true)] }));
    }

    /// What the host's NPCs wear reaches the guests page by page, round and round, each
    /// NPC's whole set in one entry (item 71: the same NPC dressed alike in both games).
    #[test]
    fn what_the_hosts_npcs_wear_reaches_the_guests() {
        let (mut host, mut guests, _) = host_and(1);
        let a = &mut guests[0];
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        run(&mut [&mut host, a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let npcs: Vec<(u32, Vec<u32>)> = (0..WEAR_PAGE as u32 + 3).map(|i| (0x0010_0000 + i, vec![0x0001_0000 + i, 0x0002_0000])).collect();
        host.set_wear(npcs.clone());
        let pages = |seen: &[Event]| -> Vec<Vec<(u32, Vec<u32>)>> {
            seen.iter()
                .filter_map(|e| match e {
                    Event::Wear(n) => Some(n.clone()),
                    _ => None,
                })
                .collect()
        };
        let seen = run(&mut [&mut host, a], &mut clock, step, |seen| pages(&seen[1]).len() >= 3);
        let got = pages(&seen[1]);
        assert_eq!((got[0].len(), got[1].len()), (WEAR_PAGE, 3));
        assert_eq!([got[0].clone(), got[1].clone()].concat(), npcs);
        assert_eq!(got[2], got[0], "round and round");
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Wear(_))), "not to the host");
        // An entry carries at most WEAR_ITEMS items.
        host.set_wear(vec![(0x0010_C0DE, (0..40).collect())]);
        let seen = run(&mut [&mut host, a], &mut clock, step, |seen| pages(&seen[1]).iter().any(|p| p[0].0 == 0x0010_C0DE));
        assert!(pages(&seen[1]).iter().any(|p| p[0] == (0x0010_C0DE, (0..WEAR_ITEMS as u32).collect())));
    }

    /// A guest's animation reaches the host and the other guest once each (it goes three
    /// times), naming the guest, and not back to it; the host's reaches the guests.
    #[test]
    fn animations_reach_everyone_once() {
        let (mut host, mut guests, _) = host_and(2);
        let (a, b) = guests.split_at_mut(1);
        let (a, b) = (&mut a[0], &mut b[0]);
        let mut clock = Instant::now();
        let step = Duration::from_millis(20);
        run(&mut [&mut host, a, b], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let ida = a.id().unwrap();
        let cue = AnimCue { kind: AnimCue::CONSUME, style: 0, power_armor: false, item: 0x0002_2D1C, edid: "BrahminSteak".into(), clip: "x.kf".into() };
        let anims = |v: &[Event]| v.iter().filter(|e| matches!(e, Event::Anim { .. })).cloned().collect::<Vec<_>>();
        a.send_anim(cue.clone());
        let mut seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| !anims(&seen[0]).is_empty() && !anims(&seen[2]).is_empty());
        // The copies still on their way.
        for _ in 0..20 {
            std::thread::sleep(Duration::from_millis(3));
            clock += step;
            for (i, s) in [&mut host, &mut *a, &mut *b].into_iter().enumerate() {
                s.tick(clock);
                seen[i].extend(s.take_events());
            }
        }
        assert_eq!(anims(&seen[0]), [Event::Anim { id: ida, cue: cue.clone() }]);
        assert_eq!(anims(&seen[2]), [Event::Anim { id: ida, cue: cue.clone() }]);
        assert!(anims(&seen[1]).is_empty(), "not back to its sender");
        host.send_anim(cue.clone());
        let seen = run(&mut [&mut host, a, b], &mut clock, step, |seen| !anims(&seen[1]).is_empty() && !anims(&seen[2]).is_empty());
        assert_eq!(anims(&seen[1])[0], Event::Anim { id: 0, cue });
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
        let welcomed = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| {
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
        // From its state, or earlier from the host's names (`Msg::Names`).
        let joined = Event::Joined { id: ida, name: String::new() };
        assert!(seen[2].contains(&joined) || welcomed[2].contains(&joined));

        a.bye();
        drop(a);
        run(&mut [&mut host, &mut b], &mut clock, step, |seen| seen[0].contains(&Event::Left(ida)) && seen[1].contains(&Event::Left(ida)));
    }

    /// A guest's table view reaches the host and the other guest, again while it stays;
    /// leaving says so; an action sent three times is taken once everywhere.
    #[test]
    fn tables_are_relayed_and_actions_taken_once() {
        let (mut host, mut g, _) = host_and(2);
        let mut b = g.pop().unwrap();
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| {
            seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))) && seen[2].iter().any(|e| matches!(e, Event::Welcomed(_)))
        });
        let ida = a.id().unwrap();
        let view = TableView { table: 0x0010_C1DB, game: 1, blob: vec![4, 5, 6] };
        a.set_table(Some(view.clone()));
        let shown = Event::Table { id: ida, table: view.table, game: 1, blob: vec![4, 5, 6] };
        let seen = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| seen[0].contains(&shown) && seen[2].contains(&shown));
        assert!(!seen[1].iter().any(|e| matches!(e, Event::Table { .. })), "not back to its sender");
        // Sent again while it stays: a guest that joins later sees it.
        let again = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| seen[2].contains(&shown));
        assert!(again[2].contains(&shown));

        // The host's action, to every guest, once each.
        host.send_table_action(view.table, 1, vec![9]);
        let act = Event::Table { id: 0, table: view.table, game: 1 | TABLE_ACTION, blob: vec![9] };
        let seen = run(&mut [&mut host, &mut a, &mut b], &mut clock, Duration::from_millis(20), |seen| seen[1].contains(&act) && seen[2].contains(&act));
        // A few more ticks for the copies to come in.
        let more = run(&mut [&mut host, &mut a, &mut b], &mut clock, Duration::from_millis(20), |seen| seen[2].iter().filter(|e| **e == shown).count() >= 2);
        for side in [1, 2] {
            let n = seen[side].iter().chain(&more[side]).filter(|e| **e == act).count();
            assert_eq!(n, 1, "taken once by {side}");
        }

        a.set_table(None);
        let left = Event::Table { id: ida, table: view.table, game: TABLE_LEFT, blob: Vec::new() };
        run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| seen[0].contains(&left) && seen[2].contains(&left));
    }

    #[test]
    fn a_table_view_too_long_is_not_sent() {
        let mut host = Session::host(0).unwrap();
        host.set_table(Some(TableView { table: 1, game: 1, blob: vec![0; TABLE_MAX + 1] }));
        assert!(host.table.is_none());
        host.set_table(Some(TableView { table: 1, game: TABLE_LEFT, blob: Vec::new() }));
        assert!(host.table.is_none(), "left is not a game");
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
        let npcs: Vec<NpcState> = (0..NPCS_PER_PACKET as u32 + 5).map(|i| NpcState { reference: 0x100 + i, feet: [i as f32, 0.0, 0.0], heading: 0.0, pitch: 0.0, flags: 0, weapon: 0, attacks: 0, furniture: 0, base: 0, level: 0, all_levels: false, rolls: 0, hostile: false, space: 0 }).collect();
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

    /// A door a guest opens reaches the host and the other guest, naming the guest, and
    /// not back to it; one the host closes reaches the guests.
    #[test]
    fn doors_reach_everyone() {
        let (mut host, mut g, _) = host_and(2);
        let mut b = g.pop().unwrap();
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| seen[1..].iter().all(|s| s.iter().any(|e| matches!(e, Event::Welcomed(_)))));
        let ida = a.id().unwrap();
        a.send_door(0x0001_D002, true);
        let door = |seen: &[Event]| seen.iter().any(|e| matches!(e, Event::Door { .. }));
        let seen = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| door(&seen[0]) && door(&seen[2]));
        assert!(seen[0].contains(&Event::Door { id: ida, reference: 0x0001_D002, open: true }));
        assert!(seen[2].contains(&Event::Door { id: 0, reference: 0x0001_D002, open: true }));
        assert!(!door(&seen[1]), "not back to the guest");
        host.send_door(0x0001_D002, false);
        let seen = run(&mut [&mut host, &mut a, &mut b], &mut clock, step, |seen| door(&seen[1]) && door(&seen[2]));
        assert!(seen[1..].iter().all(|s| s.contains(&Event::Door { id: 0, reference: 0x0001_D002, open: false })));
    }

    #[test]
    fn the_hosts_scene_reaches_the_guests_and_ends() {
        let (mut host, mut g, _) = host_and(1);
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let held = SceneView { locks: 0x7F, mods: vec![(0x0101_013B, 0.1)], worn: vec![(0x0100_F3A4, true)], swaps: vec![(0x0101_38B2, "nvdlc01_intromovie00:0".into(), "textures/x.dds".into())], waiting: None };
        host.set_scene(held.clone());
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].contains(&Event::Scene(held.clone())));
        assert!(!seen[0].iter().any(|e| matches!(e, Event::Scene(_))), "not back to the host");
        // Let go: the empty scene comes at once, not after SCENE_EVERY.
        host.set_scene(SceneView::default());
        let mut waited = Duration::ZERO;
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| {
            waited += step;
            seen[1].contains(&Event::Scene(SceneView::default()))
        });
        assert!(seen[1].contains(&Event::Scene(SceneView::default())));
        assert!(waited < SCENE_EVERY, "a change goes at once ({waited:?})");
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

    /// Queue item 201: the host's game closed and started again (a new session on the same
    /// port, knowing nobody) while a guest stayed in. The guest says Hello, is welcomed as
    /// the same player, and hears the new host's reliable items and actor rounds, and the
    /// new host hears its own: before, the guest kept the old stream (the new one came with
    /// the same number) and the old host's newest actor round, and took nothing more.
    #[test]
    fn a_guest_back_with_a_host_started_again_hears_its_streams() {
        let (mut host, mut g, addr) = host_and(1);
        let mut a = g.pop().unwrap();
        let mut clock = Instant::now();
        let step = Duration::from_millis(10);
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().any(|e| matches!(e, Event::Welcomed(_))));
        let id = a.id().unwrap();
        // The old host's streams, well along: reliable items, and many actor rounds.
        let pose = |r: u32| ActorPose { reference: r, feet: [1.0, 2.0, 3.0], heading: 0.0, pitch: 0.0, flags: 0, attacks: 0 };
        for i in 0..30u8 {
            host.send_reliable(Some(id), vec![1, i]);
        }
        for _ in 0..500 {
            host.send_actors(id, &[pose(0x10)]);
        }
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].iter().filter(|e| matches!(e, Event::Reliable { .. })).count() == 30);

        // The host's game closes; another starts on the same port.
        drop(host);
        let mut host = Session::host(addr.port()).unwrap();
        clock += LOST_AFTER + Duration::from_secs(1);
        a.tick(clock);
        assert!(a.take_events().contains(&Event::HostLost));
        run(&mut [&mut host, &mut a], &mut clock, step, |seen| seen[1].contains(&Event::HostBack) && seen[0].iter().any(|e| matches!(e, Event::Joined { .. })));
        assert_eq!(a.id(), Some(id), "the same player again");

        for i in 0..5u8 {
            host.send_reliable(Some(id), vec![2, i]);
        }
        host.send_actors(id, &[pose(0x20)]);
        a.send_reliable(None, vec![9]);
        let seen = run(&mut [&mut host, &mut a], &mut clock, step, |seen| {
            seen[1].iter().filter(|e| matches!(e, Event::Reliable { item, .. } if item[0] == 2)).count() == 5
                && seen[1].iter().any(|e| matches!(e, Event::Actors { poses, .. } if poses[0].reference == 0x20))
                && seen[0].iter().any(|e| *e == Event::Reliable { id, item: vec![9] })
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
