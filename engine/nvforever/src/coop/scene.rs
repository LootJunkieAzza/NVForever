//! The host's script scenes, shared with the guests (overnight items 66, 80, 83,
//! 2026-11-08) [design].
//!
//! A scene on the host (a DLC's opening slideshow, Elijah's broadcast, the Sink's
//! arrival) holds the host's player with DisablePlayerControls, plays fades with
//! ApplyImageSpaceModifier and moves the host with MoveTo. A guest's game does not run
//! the host's trigger scripts (`nvf_play::script_trigger`, coop-world-sync's gate), so
//! without this it would stand free while the host is held, or (before the gate) run the
//! scene on its own and stay locked after it. The host sends what its scripts do to its
//! player ([`net::SceneView`]) and each guest shares it:
//!
//! - **Held as the host is.** The control groups the host's story scripts hold
//!   (`Game::story_locks`: a quest's, a stage's or dialogue result's, a trigger volume's,
//!   and the NVSE Alt locks; not the host's own menus or what it uses, owner 2026-11-09
//!   "only story scenes", coop-replication.md §7.1) hold the guest too
//!   (`fnv_game::Game::coop_host_locks`), and let go when the host's do. A host that has
//!   not been heard for [`STALE`] seconds holds nothing: a guest is never left locked by a
//!   lost host.
//! - **The same fades.** While the host is held, the image space modifiers on its screen
//!   are put on the guest's (`Game::apply_image_modifier`), again when the host's starts
//!   again, and taken off when the host's goes. Outside a scene they are not sent: the
//!   host's drugs, hits and the like are its own.
//! - **The same slides.** While the host is held, the textures its scripts swapped on
//!   references' shapes (`SwapTexture`: each slideshow line's begin script puts its slide
//!   on the screen) are swapped here too (`TextureSwaps::follow_host`): a guest runs none
//!   of the host's line scripts. They stay after the scene, as the host's do.
//! - **Beside the host.** A guest whose movement the host's scene has held for [`SETTLE`]
//!   seconds and that stands more than [`NEAR`] units from the host in the same place is
//!   brought beside it (the host's right, [`BESIDE`] units off), as the leash brings it
//!   to another place (`super::leash`); so every player watches the scene from the same
//!   spot.
//!
//! - **Dressed as the host.** What the host's scripts put on the host's player
//!   (EquipItem, `Game::script_worn`) and it still wears goes too: each guest wears it,
//!   given one if it has none, locked on if the host's is, and it goes again when the
//!   host's goes or the guest leaves (`Game::follow_story_gear`; overnight items 81 and
//!   84: Dead Money's collar and jumpsuit, Old World Blues' gown). Gear a story takes
//!   (Dead Money's RemoveAllItems) is not followed: the owner's Mojave and DLC
//!   inventories come first (morning-report designs\coop-dlc-inventory.md).
//!
//! - **Bodies hidden.** While a scene holds a player's movement and looking, an idle's
//!   camera node drives its view or a script drives its camera, the other players' bodies
//!   are not drawn on its screen ([`hide_players_in_scene`], items 116 and 214). Not for an
//!   NVForever mini-game's own lock (`Game::scene_locks`): at a drinking contest's table
//!   the other drinkers are seen.
//!
//! Not shared: the player packages its scripts give the host's
//! player (the guests' copies of the host are drawn where the host is, and a guest held
//! still needs no package).
//!
//! Log: `co-op: the host's scene holds this player (…)`, `co-op: the host's scene lets
//! this player go`, `co-op: the host's scene: image space modifier 0x… on this screen`,
//! `co-op: the host's scene: <texture> on 0x… (<shape>)`,
//! `co-op: scene: beside the host at …`.

use super::{place_of, Coop};
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_coop::net::{self, Place, SceneView};
use nvf_play::loading::{Loading, TravelRequest};
use nvf_play::player::Player;
use nvf_play::{Location, Travel};
use std::collections::HashMap;

/// A host's scene not heard for this long (seconds) holds nothing.
const STALE: f32 = 3.0;
/// A guest is brought beside the host once its movement has been held this long
/// (seconds): a lock of a moment does not move anyone.
const SETTLE: f32 = 1.0;
/// Farther than this from the host (game units) in the host's place, a held guest is
/// brought beside it.
const NEAR: f32 = 400.0;
/// A guest further than this (units) from the host's feet while a scene holds the whole
/// view is put on them [design].
const ON_SPOT: f32 = 8.0;
/// How far to the host's right a guest is put (game units).
const BESIDE: f32 = 72.0;
/// A modifier on the host's screen whose seconds went back by more than this started
/// again.
const RESTARTED: f32 = 0.1;
/// After the host was lost, came back or started a new stream, a guest takes off no story
/// gear until the host sends some, or until the host's player has been in its world this
/// long (seconds) since. A restarted host opens its session before its save is loaded
/// and sends an empty list from the main menu (item 203 follow-up: a guest reconnecting
/// to a restarted host lost its Pip-Boy, glove and rifle); it sends its player only from
/// its world (`coop::send`), with the story gear its save keeps.
const GEAR_SETTLE: f32 = 3.0;

/// What co-op keeps of the host's scene ([`Coop::scene`]).
#[derive(Default)]
pub(super) struct Shared {
    /// A guest's: the host's scene as last heard, and when (seconds of `Time`).
    pub(super) heard: Option<(SceneView, f32)>,
    /// The control groups held here for the host, as last set.
    held: [bool; 7],
    /// Since when the host has held this player's movement, and the host's place and yaw
    /// this player was last turned to face as (since the hold began).
    moving_held_since: Option<f32>,
    turned: Option<(Place, f32)>,
    /// The modifiers put on this screen for the host, with the host's seconds for each as
    /// last heard.
    mods: HashMap<u32, f32>,
    /// A guest's: since when its story gear is held on after the host was lost, came back
    /// or started a new stream ([`GEAR_SETTLE`], [`Shared::hold_gear`]).
    gear_hold: Option<f32>,
    /// When the host's player was first heard in its world during the hold.
    host_in_world: Option<f32>,
}

impl Shared {
    /// The host was lost, is back, or sent a new stream (`now`: seconds of `Time`): keep
    /// the story gear on until the host's list is heard again ([`GEAR_SETTLE`]).
    pub(super) fn hold_gear(&mut self, now: f32) {
        if self.gear_hold.is_none() {
            info!("co-op: story gear held on until the host's list comes again (the host was lost or restarted)");
        }
        (self.gear_hold, self.host_in_world) = (Some(now), None);
    }

    /// The host's player was heard in its world (`now`: seconds of `Time`).
    pub(super) fn host_heard(&mut self, now: f32) {
        if self.gear_hold.is_some() && self.host_in_world.is_none() {
            self.host_in_world = Some(now);
        }
    }
}

/// The story gear a guest follows: what the host sends (`heard`), and while held
/// (`hold`) also what it already follows (`followed`), so nothing is taken off.
fn gear_to_follow(heard: &[(FormId, bool)], followed: impl IntoIterator<Item = FormId>, hold: bool) -> Vec<(FormId, bool)> {
    let mut out = heard.to_vec();
    if hold {
        for i in followed {
            if !out.iter().any(|(h, _)| *h == i) {
                out.push((i, false));
            }
        }
    }
    out
}

/// Whether a hold begun at `since` ends with the host's view heard at `at` carrying
/// `worn`: the host sends gear again, or its player has been in its world (from
/// `in_world`) [`GEAR_SETTLE`] seconds by then.
fn gear_hold_ends(since: f32, in_world: Option<f32>, at: f32, worn: usize) -> bool {
    at >= since && (worn > 0 || in_world.is_some_and(|w| at - w >= GEAR_SETTLE))
}

/// The groups held, as [`SceneView::locks`] bits.
pub(super) fn lock_bits(locks: [bool; 7]) -> u8 {
    locks.iter().enumerate().filter(|(_, l)| **l).fold(0, |b, (i, _)| b | 1 << i)
}

fn locks_of(bits: u8) -> [bool; 7] {
    std::array::from_fn(|i| bits & 1 << i != 0)
}

/// A host's scene now: its scripts' locks, and its screen's modifiers while one is held.
/// (A host holds nothing for another: `coop_host_locks` is clear.)
pub(super) fn scene_view(g: &fnv_game::Game) -> SceneView {
    // Story locks only (owner, 2026-11-09): the host's own menus and the things it uses
    // hold the host alone (`Game::story_locks`).
    let locks = lock_bits(g.story_locks());
    // Held: every modifier on its screen; else those its scripts put there (item 180: the
    // REPCONN launch's RepconLaunch, RepRocketLaunch and RepconSwoop flashes, with the
    // players free), which the guests in the host's place show.
    let mods = g.image_modifiers.iter().filter(|m| locks != 0 || g.script_image_modifiers.contains(&m.modifier)).map(|m| (m.modifier.0, m.elapsed as f32)).take(net::SCENE_MODS_MAX).collect();
    let worn = g.story_gear().into_iter().map(|(i, l)| (i.0, l)).take(net::SCENE_MODS_MAX).collect();
    let swaps = if locks == 0 { Vec::new() } else { scene_swaps(&g.texture_swaps) };
    SceneView { locks, mods, worn, swaps, waiting: None }
}

/// A story wait is fresh while the host's talk was asked for this recently (game seconds):
/// the gate asks again every update of the NPC's force greet, and on each try to talk.
const WAIT_FRESH: f64 = 3.0;

/// The host's story wait to send ([`SceneView::waiting`], `fnv_game::Game::story_gate`):
/// the NPC and the ids of the players (by their actors here) not near it.
fn story_waiting(g: &fnv_game::Game, players: impl Iterator<Item = (u8, FormId)>) -> Option<(u32, Vec<u8>)> {
    let w = g.coop_story_wait.as_ref().filter(|w| g.time - w.at <= WAIT_FRESH)?;
    let far: Vec<u8> = players.filter(|(_, a)| w.far.contains(a)).map(|(id, _)| id).collect();
    (!far.is_empty()).then_some((w.actor.0, far))
}

/// The name a story NPC shows (its base's FULL), else its Editor ID.
fn shown_name(g: &fnv_game::Game, actor: FormId) -> String {
    let base = g.world.peek(actor).map(|s| s.base).unwrap_or(actor);
    g.data.full_names.get(&base).cloned().unwrap_or_else(|| g.data.name(actor))
}

/// What the story wait prompts keep ([`story_prompts`]).
#[derive(Default)]
pub(super) struct Prompts {
    /// The host's: when its screen last said who it waits for.
    told: Option<f32>,
    /// A guest's: the NPC (or the host's player) it was last offered a teleport for, and
    /// when.
    offered: Option<(u32, f32)>,
    /// A guest's: the offer held for a menu was logged, for this NPC.
    held_logged: Option<u32>,
}

/// How often the host's log says again whom it waits for (seconds).
const TELL_EVERY: f32 = 8.0;

/// The story wait on the screens, in the co-op invitation's style (owner, 2026-11-09: "It
/// should be a prompt like the duel prompt on the hosts end it'll say waiting for
/// players, and then give guests only the option to teleport while they're not in a menu
/// already and if they are it'll come up once that menu closes"): the host's banner says
/// "WAITING FOR PLAYERS...", with no keys (the host always waits); each far guest's says
/// the host waits for it, with Y to teleport beside the host (refused in combat), and
/// stays until it is near. Neither shows over a menu.
///
/// Log: `co-op: story wait: …`.
#[allow(clippy::too_many_arguments)]
pub(super) fn story_prompts(
    mut coop: ResMut<Coop>,
    time: Res<Time>,
    mut state: Local<Prompts>,
    script: Option<ResMut<nvf_play::script::ScriptState>>,
    mut corner: Option<ResMut<nvf_play::corner_messages::CornerMessages>>,
    loading: Option<Res<Loading>>,
    mut travel: MessageWriter<TravelRequest>,
    social: Option<ResMut<nvf_play::coop_social::Social>>,
    play: Option<Res<State<nvf_play::PlayState>>>,
) {
    use nvf_play::coop_social::StoryPrompt;
    let now = time.elapsed_secs();
    let Some(mut social) = social else { return };
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let g = &mut rt.game;
    // A prompt shows only while this player plays: a menu (the Pip-Boy, a conversation, a
    // container, barter) holds it until it closes [design: owner 2026-11-09, "if they are
    // it'll come up once that menu closes"].
    let playing = play.as_ref().is_none_or(|p| *p.get() == nvf_play::PlayState::Playing) && !g.in_menu() && !loading.as_ref().is_some_and(|l| l.active());
    let mut prompt = None;
    if coop.is_host() {
        let fresh = g.coop_story_wait.as_ref().filter(|w| g.time - w.at <= WAIT_FRESH);
        let names: Vec<String> = match fresh {
            Some(w) => w.far.iter().filter_map(|a| g.coop_actors.get(a).cloned()).collect(),
            // Left behind by a script's move of the host: waited for all the same.
            None => coop.repl.wait_for.iter().map(|id| coop.peers.get(id).map(|p| super::label(*id, &p.name)).unwrap_or_else(|| format!("Player {}", id + 1))).collect(),
        };
        if names.is_empty() {
            state.told = None;
        } else {
            prompt = playing.then_some(StoryPrompt::Host);
            if state.told.is_none_or(|t| now - t >= TELL_EVERY) {
                let text = match fresh.map(|w| shown_name(g, w.actor)) {
                    Some(npc) => format!("Waiting for {} to come near {npc}.", names.join(", ")),
                    None => format!("Waiting for {} to come near.", names.join(", ")),
                };
                info!("co-op: story wait: {text}");
                state.told = Some(now);
            }
        }
    } else if coop.is_guest() {
        let me = coop.link.as_ref().and_then(|l| l.id());
        let heard: Option<(u32, Vec<u8>)> = coop.scene.heard.as_ref().filter(|(_, at): &&(SceneView, f32)| now - *at < STALE).and_then(|(v, _)| v.waiting.clone());
        let host = coop.peers.get(&0).map(|p| super::label(0, &p.name)).unwrap_or_else(|| "The host".into());
        prompt = guest_prompt(heard.as_ref(), me, playing, &host);
        if let Some((npc, _)) = heard.filter(|(_, far)| me.is_some_and(|me| far.contains(&me))) {
            if playing {
                if state.offered.is_none_or(|(n, _)| n != npc) {
                    let why = if npc == FormId::PLAYER_REF.0 { String::new() } else { format!(" to talk to {}", shown_name(g, FormId(npc))) };
                    info!("co-op: story wait: the host waits{why}: offered a teleport to the host");
                    state.offered = Some((npc, now));
                }
            } else if state.offered.is_none_or(|(n, _)| n != npc) && state.held_logged != Some(npc) {
                info!("co-op: story wait: the teleport offer waits for this player's menu to close");
                state.held_logged = Some(npc);
            }
        } else {
            state.offered = None;
            state.held_logged = None;
        }
        if std::mem::take(&mut social.story_accepted) && matches!(prompt, Some(StoryPrompt::Guest { .. })) {
            fast_travel_to_host(&mut coop, g, now, loading.as_deref(), &mut travel, corner.as_deref_mut());
        }
    }
    if social.story != prompt {
        social.story = prompt;
    }
}

/// A guest's story prompt now: the host waits for it (`waiting`, the host's
/// [`SceneView::waiting`], names `me` among the far players), shown only while it plays;
/// under a menu it is held, and it is back as soon as the menu closes, for as long as the
/// host waits for it [design: owner 2026-11-09, "if they are it'll come up once that menu
/// closes"].
fn guest_prompt(waiting: Option<&(u32, Vec<u8>)>, me: Option<u8>, playing: bool, host: &str) -> Option<nvf_play::coop_social::StoryPrompt> {
    let far = waiting.is_some_and(|(_, far)| me.is_some_and(|me| far.contains(&me)));
    (far && playing).then(|| nvf_play::coop_social::StoryPrompt::Guest { host: host.to_string() })
}

/// A guest's fast travel to the host, beside it, as a scene's pull puts it; refused in
/// combat (the game's own rule for fast travel).
fn fast_travel_to_host(coop: &mut Coop, g: &fnv_game::Game, now: f32, loading: Option<&Loading>, travel: &mut MessageWriter<TravelRequest>, corner: Option<&mut nvf_play::corner_messages::CornerMessages>) {
    let Some(host) = coop.host else { return };
    if g.awareness.hunted(FormId::PLAYER_REF) {
        info!("co-op: story wait: no fast travel to the host: this player is in combat");
        if let Some(c) = corner {
            c.push("You cannot fast travel to the host while enemies are after you.".to_string().into());
        }
        return;
    }
    if loading.is_some_and(|l| l.active()) {
        return;
    }
    let feet = beside_slot(host.feet, host.yaw, coop.link.as_ref().and_then(|l| l.id()).unwrap_or(1));
    let (world, interior) = match host.place {
        Place::Interior(c) => (None, Some(c)),
        Place::Exterior(w) => (Some(w), None),
    };
    info!("co-op: story wait: fast travel to the host at {:?} {feet:?}", host.place);
    travel.write(TravelRequest(Travel { world, interior, feet, heading_deg: host.yaw.to_degrees() }));
    coop.last_leash = Some(now);
}

/// The host's texture swaps a held scene sends, at most [`net::SCENE_SWAPS_MAX`]: by
/// reference, each one's last calls first. `TextureSwaps` keeps the last call per shape,
/// so a slideshow's screen, swapped again and again, sends its current slide.
fn scene_swaps(swaps: &fnv_game::functions::misc::TextureSwaps) -> Vec<(u32, String, String)> {
    let mut all: Vec<(u32, String, String)> = swaps.by_ref.iter().flat_map(|(r, l)| l.iter().rev().map(move |(s, t)| (r.0, s.clone(), t.clone()))).collect();
    all.truncate(net::SCENE_SWAPS_MAX);
    all
}

/// What the modifiers on the host's screen ask of this one: the modifiers to put on
/// (new, or started again on the host's) and those to take off (gone from the host's).
/// `given` is updated to the host's list.
fn follow_mods(given: &mut HashMap<u32, f32>, host: &[(u32, f32)]) -> (Vec<u32>, Vec<u32>) {
    let mut on = Vec::new();
    for &(m, at) in host {
        if given.get(&m).is_none_or(|&was| at + RESTARTED < was) {
            on.push(m);
        }
        given.insert(m, at);
    }
    let off: Vec<u32> = given.keys().copied().filter(|m| !host.iter().any(|(h, _)| h == m)).collect();
    for m in &off {
        given.remove(m);
    }
    (on, off)
}

/// How far the host may turn within a scene's hold before a held player is turned to face
/// as the host again (radians; about 10 degrees, past the jitter of a head turn).
const TURN_AGAIN: f32 = 0.17;

/// The smaller angle between two yaws, in radians.
fn angle_apart(a: f32, b: f32) -> f32 {
    let d = (a - b).rem_euclid(std::f32::consts::TAU);
    d.min(std::f32::consts::TAU - d)
}

/// Guest `slot`'s place beside the host (its player number, 1 and up): each its own, so
/// guests summoned together do not land inside each other [playtest 119 item 154: every
/// guest was summoned to one point at the Zion intro]. Slot 1 [`BESIDE`] to the right,
/// 2 as far to the left, 3 twice as far to the right, 4 to the left, and so on (yaw is a
/// compass bearing: 0 north, +Y; π/2 east, +X).
pub(crate) fn beside_slot(feet: [f32; 3], yaw: f32, slot: u8) -> [f32; 3] {
    let slot = slot.max(1);
    let side = if slot.is_multiple_of(2) { -1.0 } else { 1.0 };
    let reach = BESIDE * f32::from(slot.div_ceil(2));
    let right = yaw + std::f32::consts::FRAC_PI_2;
    [feet[0] + right.sin() * reach * side, feet[1] + right.cos() * reach * side, feet[2]]
}

#[allow(clippy::too_many_arguments)]
pub(super) fn share_scene(
    mut coop: ResMut<Coop>,
    time: Res<Time>,
    script: Option<ResMut<nvf_play::script::ScriptState>>,
    player: Option<ResMut<Player>>,
    loc: Option<Res<Location>>,
    loading: Option<Res<Loading>>,
    mut travel: MessageWriter<TravelRequest>,
    solo: Option<Res<nvf_play::save::SoloBase>>,
    mut sim: Option<ResMut<nvf_play::sim::SimState>>,
    mut phys: Option<ResMut<nvf_play::phys::PhysState>>,
) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    // The places' world lives in `SimState` and is lent to the scripts' game only inside
    // `with_world`: the player's inventory (what it wears, the story gear) is read and
    // changed there.
    if coop.is_host() {
        rt.game.coop_host_locks = [false; 7];
        coop.scene = Shared::default();
        if let Some(link) = coop.link.as_ref() {
            // Who is far from the host's player: measured in the world, which the scripts'
            // game has only inside `with_world` (outside it no player was found, so the
            // wait ended at once: laptop run story-far-2, 2026-11-09).
            let measured = sim.is_some();
            let (mut view, far) = match sim.as_deref_mut() {
                Some(sim) => nvf_play::script::with_world(rt, sim, |rt| (scene_view(&rt.game), rt.game.players_far_from(FormId::PLAYER_REF))),
                None => (scene_view(&rt.game), Vec::new()),
            };
            let players: Vec<(u8, FormId)> = coop.peers.iter().filter_map(|(id, p)| p.actor.map(|a| (*id, a))).collect();
            view.waiting = story_waiting(&rt.game, players.iter().copied());
            // The guests a script's move of the host left behind: a wait for the host's
            // own player until each is near it.
            if !coop.repl.wait_for.is_empty() && measured {
                let before = coop.repl.wait_for.len();
                coop.repl.wait_for.retain(|id| players.iter().any(|(i, a)| i == id && far.contains(a)));
                if coop.repl.wait_for.len() < before {
                    let left: Vec<String> = coop.repl.wait_for.iter().map(|i| format!("player {}", i + 1)).collect();
                    info!("co-op: story wait: the host waits for {} now", if left.is_empty() { "no one".to_string() } else { left.join(", ") });
                }
                if view.waiting.is_none() && !coop.repl.wait_for.is_empty() {
                    view.waiting = Some((FormId::PLAYER_REF.0, coop.repl.wait_for.clone()));
                }
            }
            link.set_scene(view);
        }
        return;
    }
    let (guest, lost) = (coop.is_guest(), coop.host_lost);
    // In the host's place (its interior, or its worldspace): a script's modifiers on the
    // host's screen show here too while nobody is held (item 180).
    let with_host = loc.as_deref().map(super::place_of).is_some_and(|p| coop.host_place.is_some_and(|(h, _)| h == p));
    let s = &mut coop.scene;
    // The host's scene now: none when not a guest (left the lobby), the host is lost or
    // not heard lately, or this game is not yet kept as the guest's own.
    let heard = s.heard.as_ref().filter(|(_, at)| guest && !lost && solo.is_some() && now - at < STALE).map(|(v, _)| v.clone());
    if !guest {
        s.heard = None;
    }
    let view = heard.unwrap_or_default();
    // The host's story gear: kept while the host is only lost a moment (a dropout is not
    // a leave), taken off when this game is no guest.
    if let (Some(since), Some((v, at))) = (s.gear_hold, s.heard.as_ref()) {
        if guest && !lost && gear_hold_ends(since, s.host_in_world, *at, v.worn.len()) {
            info!("co-op: story gear follows the host's list again ({} item(s))", v.worn.len());
            s.gear_hold = None;
        }
    }
    if !guest {
        s.gear_hold = None;
    }
    let gear: Option<Vec<(FormId, bool)>> = if guest && solo.is_some() && (s.heard.is_some() || !rt.game.coop_story_gear.is_empty()) {
        let hold = s.gear_hold.is_some();
        s.heard.as_ref().map(|(v, _)| {
            let heard: Vec<(FormId, bool)> = v.worn.iter().map(|&(i, l)| (FormId(i), l)).collect();
            gear_to_follow(&heard, rt.game.coop_story_gear.keys().copied(), hold)
        })
    } else if !rt.game.coop_story_gear.is_empty() {
        Some(Vec::new())
    } else {
        None
    };
    if let (Some(gear), Some(sim)) = (gear, sim.as_deref_mut()) {
        nvf_play::script::with_world(rt, sim, |rt| {
            let g = &mut rt.game;
            let (on, off) = g.follow_story_gear(&gear);
            for i in on {
                info!("co-op: the host's story puts {} ({:#010X}) on this player too", g.data.name(i), i.0);
            }
            for i in off {
                info!("co-op: {} ({:#010X}) taken off: the host's story no longer has it on", g.data.name(i), i.0);
            }
        });
    }
    let g = &mut rt.game;
    let held = locks_of(view.locks);
    if held != s.held {
        if held.iter().any(|h| *h) {
            info!("co-op: the host's scene holds this player (controls {held:?})");
        } else {
            info!("co-op: the host's scene lets this player go");
        }
        s.held = held;
    }
    g.coop_host_locks = held;
    let mods: &[(u32, f32)] = if view.locks != 0 || with_host { &view.mods } else { &[] };
    let (on, off) = follow_mods(&mut s.mods, mods);
    for m in on {
        info!("co-op: the host's scene: image space modifier {m:#010X} on this screen");
        g.apply_image_modifier(FormId(m));
    }
    for m in off {
        g.remove_image_modifier(FormId(m));
    }
    // The host's slides: its line scripts swapped them, which this game does not run.
    for (on, shape, texture) in &view.swaps {
        if g.texture_swaps.follow_host(FormId(*on), shape.clone(), texture.clone()) {
            info!("co-op: the host's scene: {texture} on {on:#010X} ({shape})");
        }
    }
    // Beside the host while its movement is held.
    s.moving_held_since = if held[0] { s.moving_held_since.or(Some(now)) } else { None };
    if s.moving_held_since.is_none() {
        s.turned = None;
    }
    let settled = s.moving_held_since.is_some_and(|t| now - t >= SETTLE);
    let (Some(host), Some(mut player), Some(loc)) = (coop.host, player, loc) else { return };
    // Looking held too (a slideshow, a speech): this player faces the way the host does,
    // once, so it sees what the host is shown.
    // Again whenever the host is moved or turned within the hold (item 153: the guest
    // turned once at Jed's talk, then was summoned into the Honest Hearts slideshow room
    // and kept that heading, so its view never reached the screen the host faced).
    let turn_again = s.turned.is_none_or(|(place, yaw)| place != host.place || angle_apart(yaw, host.yaw) > TURN_AGAIN);
    if settled && held[4] && turn_again && place_of(&loc) == host.place {
        player.heading = host.yaw;
        player.pitch = host.pitch;
        s.turned = Some((host.place, host.yaw));
        info!("co-op: scene: this player faces as the host ({:.0} deg)", host.yaw.to_degrees());
    }
    if !settled || loading.is_some_and(|l| l.active()) || coop.downed_at.is_some() {
        return;
    }
    let (dx, dy) = (player.feet[0] - host.feet[0], player.feet[1] - host.feet[1]);
    // The scene holds the whole view (a slideshow): watched from the host's own spot.
    // From its place beside the host the Honest Hearts slides stood off-centre on a
    // guest's screen, black down one side (desktop run, 2026-11-10; item 153). The
    // players' bodies are hidden and in nobody's way meanwhile
    // ([`hide_players_in_scene`]).
    if scene_holds_view(held) && place_of(&loc) == host.place && dx * dx + dy * dy > ON_SPOT * ON_SPOT {
        coop.last_leash = Some(now);
        // Already in the host's place: put there at once, as the test aids' go-to does,
        // with no loading screen (queue item 185: after the Dead Money summon the guest had
        // a second loading screen for this step of a few hundred units in the same cell).
        if let Some(phys) = phys.as_deref_mut().filter(|p| p.player.is_some() && p.ready_at(host.feet)) {
            phys.place_player(host.feet, 60.0, 200.0);
            let feet = phys.player.as_ref().map_or(host.feet, |c| c.feet);
            player.feet = feet;
            player.placed = Some(feet);
            player.heading = host.yaw;
            info!("co-op: scene: on the host's spot at {:?} {feet:?}: the scene holds the view (put there, same place)", host.place);
            return;
        }
        let (world, interior) = match host.place {
            Place::Interior(c) => (None, Some(c)),
            Place::Exterior(w) => (Some(w), None),
        };
        info!("co-op: scene: on the host's spot at {:?} {:?}: the scene holds the view", host.place, host.feet);
        travel.write(TravelRequest(Travel { world, interior, feet: host.feet, heading_deg: host.yaw.to_degrees() }));
        return;
    }
    if coop.last_leash.is_some_and(|t| now - t < super::LEASH_EVERY) {
        return;
    }
    // Another place is the leash's (`super::leash`).
    if place_of(&loc) != host.place || dx * dx + dy * dy <= NEAR * NEAR {
        return;
    }
    let feet = beside_slot(host.feet, host.yaw, coop.link.as_ref().and_then(|l| l.id()).unwrap_or(1));
    let (world, interior) = match host.place {
        Place::Interior(c) => (None, Some(c)),
        Place::Exterior(w) => (Some(w), None),
    };
    info!("co-op: scene: beside the host at {:?} {feet:?}", host.place);
    travel.write(TravelRequest(Travel { world, interior, feet, heading_deg: host.yaw.to_degrees() }));
    coop.last_leash = Some(now);
}

/// Whether a scene holds this player's view: its movement and its looking are both held
/// (a DLC's opening slideshow, Elijah's broadcast; the host's own locks on the host, the
/// host's shared ones on a guest).
fn scene_holds_view(locks: [bool; 7]) -> bool {
    locks[0] && locks[4]
}

/// Why the other players' bodies are not drawn on this screen, if they are not: while
/// something other than the player drives this player's camera, every other body is
/// hidden. In the original only one player stands in such a shot, and a co-op player in
/// it filled the screen with a face (item 214, the owner watching Dead Money in co-op
/// on 2026-11-14: "players should be invis so they dont see eachothers faces"):
///
/// - a scene holds the view: its movement and its looking held (a DLC's opening or
///   ending slideshow, Elijah's broadcast; every player watches from the scene's spot in
///   first person, and a body on that spot would fill the screen; playtest list 7 item
///   116: in the original the camera stays on the slideshow's screen with no body in view
///   [the owner, 2026-11-09]);
/// - an idle's camera node drives the view (`nvf_play::player::ViewDriven`: the opening's
///   and the Sierra Madre villa's wake-up);
/// - a script places or turns the camera (JohnnyGuitar's SetCameraTranslate,
///   SetCameraRotate, SetCameraRotateToRef: `CameraRequests`' overrides).
///
/// They are drawn again when it ends.
fn hidden_because(locks: [bool; 7], idle_drives_view: bool, script_camera: bool) -> Option<&'static str> {
    if scene_holds_view(locks) {
        Some("the scene holds the view")
    } else if idle_drives_view {
        Some("an idle's camera node drives the view")
    } else if script_camera {
        Some("a script drives the camera")
    } else {
        None
    }
}

/// Whether a script has placed or turned the camera (`CameraRequests`).
fn script_camera(c: &fnv_game::functions::camera::CameraRequests) -> bool {
    c.translate_override.is_some() || c.rotate_override.iter().any(Option::is_some) || c.rotate_track.is_some()
}

/// Hides the other players' bodies on this screen while [`hidden_because`] says so (a
/// scene, an idle's camera, a script's camera), and draws them again after.
pub(super) fn hide_players_in_scene(
    coop: Res<Coop>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    driven: Option<Res<nvf_play::player::ViewDriven>>,
    mut bodies: Query<&mut Visibility, With<super::Puppet>>,
    mut hidden: Local<Option<&'static str>>,
    mut blocks: Option<ResMut<nvf_play::blocking::ActorBlocks>>,
) {
    let game = script.as_ref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game);
    let why = (coop.is_host() || coop.is_guest())
        .then(|| {
            hidden_because(
                game.map_or([false; 7], |g| g.scene_locks()),
                driven.as_deref().is_some_and(|d| d.0),
                game.is_some_and(|g| script_camera(&g.ui.camera)),
            )
        })
        .flatten();
    if why != *hidden {
        let (was, now) = (hidden.is_some(), why.is_some());
        *hidden = why;
        match why {
            Some(w) => info!("co-op: scene: the other players' bodies hidden while {w}"),
            None => info!("co-op: scene: the other players' bodies drawn again"),
        }
        // Nor in anybody's way: the guests stand on the host's spot meanwhile, and a
        // body there pushed the host aside (laptop run story-near, 2026-11-09).
        if was != now {
            if let Some(b) = blocks.as_deref_mut() {
                for r in coop.peers.iter().flat_map(|(&id, p)| p.references(id)) {
                    if now {
                        b.in_nobodys_way.insert(r);
                    } else {
                        b.in_nobodys_way.remove(&r);
                    }
                }
            }
        }
    }
    let want = if why.is_some() { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut bodies {
        v.set_if_neq(want);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reconnect_takes_no_story_gear_off_until_the_host_sends_it_again() {
        // Item 203 follow-up: a restarted host sends an empty list until its save loads.
        let (pipboy, collar) = (FormId(0x15038), FormId(0x0100_F3A4));
        assert_eq!(gear_to_follow(&[], [pipboy], true), vec![(pipboy, false)], "held: kept on");
        assert_eq!(gear_to_follow(&[], [pipboy], false), vec![], "not held: the host's list");
        assert_eq!(gear_to_follow(&[(collar, true)], [pipboy, collar], true), vec![(collar, true), (pipboy, false)]);
        // The hold ends when the host sends gear again, or once its player has been in
        // its world a while; at the main menu (no player heard) it never ends by itself.
        assert!(!gear_hold_ends(10.0, None, 11.0, 0));
        assert!(!gear_hold_ends(10.0, None, 600.0, 0), "the host still at its main menu");
        assert!(gear_hold_ends(10.0, None, 11.0, 2));
        assert!(!gear_hold_ends(10.0, Some(40.0), 41.0, 0), "just loaded: its list may not be in yet");
        assert!(gear_hold_ends(10.0, Some(40.0), 40.0 + GEAR_SETTLE, 0));
        assert!(!gear_hold_ends(10.0, None, 9.0, 2), "a view heard before the hold began");
        // Lost and back again (RelReset can come twice): the hold starts over.
        let mut s = Shared::default();
        s.hold_gear(10.0);
        s.host_heard(12.0);
        s.hold_gear(13.0);
        assert_eq!((s.gear_hold, s.host_in_world), (Some(13.0), None));
        s.host_heard(20.0);
        s.host_heard(21.0);
        assert_eq!(s.host_in_world, Some(20.0));
    }

    #[test]
    fn other_players_are_hidden_while_anything_but_the_player_drives_the_camera() {
        // Item 214: slides, the villa wake-up's idle camera, a script's camera.
        let free = [false; 7];
        assert_eq!(hidden_because(free, false, false), None, "the player's own view: drawn");
        let mut slides = free;
        slides[0] = true;
        slides[4] = true;
        assert_eq!(hidden_because(slides, false, false), Some("the scene holds the view"));
        assert_eq!(hidden_because(free, true, false), Some("an idle's camera node drives the view"));
        assert_eq!(hidden_because(free, false, true), Some("a script drives the camera"));
        // Movement alone held (a talk): the others are seen.
        let mut talk = free;
        talk[0] = true;
        assert_eq!(hidden_because(talk, false, false), None);
        let mut c = fnv_game::functions::camera::CameraRequests::default();
        assert!(!script_camera(&c));
        c.rotate_override[2] = Some(90.0);
        assert!(script_camera(&c));
        c = Default::default();
        c.rotate_track = Some(FormId(0x14));
        assert!(script_camera(&c));
        c = Default::default();
        c.translate_override = Some([0.0, -200.0, 50.0]);
        assert!(script_camera(&c));
    }

    #[test]
    fn lock_bits_round_trip() {
        let l = [true, false, true, false, false, false, true];
        assert_eq!(lock_bits(l), 0b100_0101);
        assert_eq!(locks_of(lock_bits(l)), l);
        assert_eq!(lock_bits([false; 7]), 0);
    }

    #[test]
    fn mods_follow_the_host() {
        let mut given = HashMap::new();
        // A fade starts on the host's screen: on here.
        assert_eq!(follow_mods(&mut given, &[(0x0101_013B, 0.1)]), (vec![0x0101_013B], vec![]));
        // It plays on: nothing to do.
        assert_eq!(follow_mods(&mut given, &[(0x0101_013B, 0.6)]), (vec![], vec![]));
        // It starts again (the next line's fade): on again.
        assert_eq!(follow_mods(&mut given, &[(0x0101_013B, 0.05)]), (vec![0x0101_013B], vec![]));
        // It is gone from the host's: off here.
        assert_eq!(follow_mods(&mut given, &[]), (vec![], vec![0x0101_013B]));
        assert!(given.is_empty());
    }

    #[test]
    fn a_menu_holds_the_teleport_offer_until_it_closes() {
        use nvf_play::coop_social::StoryPrompt;
        let waiting = (FormId::PLAYER_REF.0, vec![1u8]);
        let offer = Some(StoryPrompt::Guest { host: "Host".into() });
        // Far, in the Pip-Boy: held.
        assert_eq!(guest_prompt(Some(&waiting), Some(1), false, "Host"), None);
        // The Pip-Boy closes: shown, and stays while the host waits.
        assert_eq!(guest_prompt(Some(&waiting), Some(1), true, "Host"), offer);
        // Another guest is the far one, or the host waits no more: nothing.
        assert_eq!(guest_prompt(Some(&waiting), Some(2), true, "Host"), None);
        assert_eq!(guest_prompt(None, Some(1), true, "Host"), None);
    }

    #[test]
    fn beside_is_to_the_right() {
        // Facing north, the right is east (+X).
        let f = beside_slot([0.0, 0.0, 10.0], 0.0, 1);
        assert!((f[0] - BESIDE).abs() < 1e-3 && f[1].abs() < 1e-3 && f[2] == 10.0);
        // Facing east, the right is south (-Y).
        let f = beside_slot([0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2, 1);
        assert!(f[0].abs() < 1e-3 && (f[1] + BESIDE).abs() < 1e-3);
    }

    #[test]
    fn each_guest_has_its_own_place_beside_the_host() {
        let spots: Vec<[f32; 3]> = (1..=4).map(|k| beside_slot([0.0, 0.0, 0.0], 0.0, k)).collect();
        assert!((spots[0][0] - BESIDE).abs() < 1e-3);
        assert!((spots[1][0] + BESIDE).abs() < 1e-3);
        assert!((spots[2][0] - 2.0 * BESIDE).abs() < 1e-3);
        assert!((spots[3][0] + 2.0 * BESIDE).abs() < 1e-3);
        for (i, a) in spots.iter().enumerate() {
            for b in &spots[i + 1..] {
                assert!((a[0] - b[0]).hypot(a[1] - b[1]) >= BESIDE - 1e-3);
            }
        }
    }
}
