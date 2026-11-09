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
//! - **Held as the host is.** The control groups the host's scripts hold (every lock
//!   `Game::controls_locked` reads, the NVSE Alt locks included) hold the guest too
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
//! - **Bodies hidden.** While a scene holds a player's movement and looking, the other
//!   players' bodies are not drawn ([`hide_players_in_scene`], item 116).
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
/// How far to the host's right a guest is put (game units).
const BESIDE: f32 = 72.0;
/// A modifier on the host's screen whose seconds went back by more than this started
/// again.
const RESTARTED: f32 = 0.1;

/// What co-op keeps of the host's scene ([`Coop::scene`]).
#[derive(Default)]
pub(super) struct Shared {
    /// A guest's: the host's scene as last heard, and when (seconds of `Time`).
    pub(super) heard: Option<(SceneView, f32)>,
    /// The control groups held here for the host, as last set.
    held: [bool; 7],
    /// Since when the host has held this player's movement, and whether this player was
    /// turned to face as the host since.
    moving_held_since: Option<f32>,
    turned: bool,
    /// The modifiers put on this screen for the host, with the host's seconds for each as
    /// last heard.
    mods: HashMap<u32, f32>,
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
    let locks = lock_bits(g.controls_locked());
    let mods = if locks == 0 { Vec::new() } else { g.image_modifiers.iter().map(|m| (m.modifier.0, m.elapsed as f32)).take(net::SCENE_MODS_MAX).collect() };
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
    /// A guest's: the NPC it was last offered a fast travel for, and when.
    offered: Option<(u32, f32)>,
    /// A guest's: the owner of the box it showed, while it is open.
    open_box: Option<Option<String>>,
}

/// How often the host's screen says again whom it waits for, and how soon a guest who
/// said "Not now" is offered the fast travel again (seconds).
const TELL_EVERY: f32 = 8.0;
const OFFER_AGAIN: f32 = 45.0;

/// The story wait on the screens (owner, 2026-11-09): the host's corner says which
/// players it waits for and for whom; each far guest gets a box offering a fast travel to
/// the host ("Give an option for players to fast travel when host is waiting"), which
/// puts it beside the host as a scene's pull does, unless it is in combat.
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
) {
    let now = time.elapsed_secs();
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let g = &mut rt.game;
    if coop.is_host() {
        let names: Vec<String> = match g.coop_story_wait.as_ref().filter(|w| g.time - w.at <= WAIT_FRESH) {
            Some(w) => w.far.iter().filter_map(|a| g.coop_actors.get(a).cloned()).collect(),
            None => Vec::new(),
        };
        if names.is_empty() {
            state.told = None;
            return;
        }
        if state.told.is_some_and(|t| now - t < TELL_EVERY) {
            return;
        }
        let npc = g.coop_story_wait.as_ref().map(|w| shown_name(g, w.actor)).unwrap_or_default();
        let text = format!("Waiting for {} to come near {npc}.", names.join(", "));
        info!("co-op: story wait: {text}");
        if let Some(c) = corner.as_mut() {
            c.push(text.into());
        }
        state.told = Some(now);
        return;
    }
    if !coop.is_guest() {
        return;
    }
    // The box's answer.
    if let Some(owner) = state.open_box.clone() {
        match g.message_box.as_ref() {
            Some(m) if m.owner == owner => {
                if let Some(b) = m.pressed {
                    g.message_box = None;
                    state.open_box = None;
                    if b == 0 {
                        fast_travel_to_host(&mut coop, g, now, loading.as_deref(), &mut travel, corner.as_deref_mut());
                    } else {
                        info!("co-op: story wait: this player stays where it is");
                    }
                }
            }
            // Closed some other way.
            _ => state.open_box = None,
        }
        return;
    }
    let me = coop.link.as_ref().and_then(|l| l.id());
    let heard: Option<(u32, Vec<u8>)> = coop.scene.heard.as_ref().filter(|(_, at): &&(SceneView, f32)| now - *at < STALE).and_then(|(v, _)| v.waiting.clone());
    let Some((npc, _)) = heard.filter(|(_, far)| me.is_some_and(|me| far.contains(&me))) else { return };
    if state.offered.is_some_and(|(n, at)| n == npc && now - at < OFFER_AGAIN) || g.message_box.is_some() || g.in_menu() || loading.is_some_and(|l| l.active()) {
        return;
    }
    let name = shown_name(g, FormId(npc)).replace(['"', '|', '%'], "");
    let line = format!("MessageBoxEx \"The host is waiting to talk to {name}, and you are too far away.|Fast travel to the host|Not now\"");
    match g.run_source(&line, Default::default()) {
        Ok(_) => {
            info!("co-op: story wait: the host waits to talk to {name}: offered a fast travel to the host");
            state.open_box = Some(g.message_box.as_ref().and_then(|m| m.owner.clone()));
        }
        Err(e) => warn!("co-op: story wait: no box for the fast travel: {e}"),
    }
    state.offered = Some((npc, now));
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
    let feet = beside(host.feet, host.yaw);
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

/// Where a guest stands beside the host: [`BESIDE`] units to the host's right (yaw is a
/// compass bearing: 0 north, +Y; π/2 east, +X).
fn beside(feet: [f32; 3], yaw: f32) -> [f32; 3] {
    let right = yaw + std::f32::consts::FRAC_PI_2;
    [feet[0] + right.sin() * BESIDE, feet[1] + right.cos() * BESIDE, feet[2]]
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
            let mut view = match sim.as_deref_mut() {
                Some(sim) => nvf_play::script::with_world(rt, sim, |rt| scene_view(&rt.game)),
                None => scene_view(&rt.game),
            };
            view.waiting = story_waiting(&rt.game, coop.peers.iter().filter_map(|(id, p)| p.actor.map(|a| (*id, a))));
            link.set_scene(view);
        }
        return;
    }
    let (guest, lost) = (coop.is_guest(), coop.host_lost);
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
    let gear: Option<Vec<(FormId, bool)>> = if guest && solo.is_some() && (s.heard.is_some() || !rt.game.coop_story_gear.is_empty()) {
        s.heard.as_ref().map(|(v, _)| v.worn.iter().map(|&(i, l)| (FormId(i), l)).collect())
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
    let (on, off) = follow_mods(&mut s.mods, &view.mods);
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
        s.turned = false;
    }
    let settled = s.moving_held_since.is_some_and(|t| now - t >= SETTLE);
    let (Some(host), Some(mut player), Some(loc)) = (coop.host, player, loc) else { return };
    // Looking held too (a slideshow, a speech): this player faces the way the host does,
    // once, so it sees what the host is shown.
    if settled && held[4] && !s.turned && place_of(&loc) == host.place {
        player.heading = host.yaw;
        player.pitch = host.pitch;
        s.turned = true;
        info!("co-op: scene: this player faces as the host ({:.0} deg)", host.yaw.to_degrees());
    }
    if !settled || loading.is_some_and(|l| l.active()) || coop.downed_at.is_some() || coop.last_leash.is_some_and(|t| now - t < super::LEASH_EVERY) {
        return;
    }
    let (dx, dy) = (player.feet[0] - host.feet[0], player.feet[1] - host.feet[1]);
    // Another place is the leash's (`super::leash`).
    if place_of(&loc) != host.place || dx * dx + dy * dy <= NEAR * NEAR {
        return;
    }
    let feet = beside(host.feet, host.yaw);
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

/// While a scene holds the view, the other players' bodies are not drawn: every player
/// watches from the scene's spot in first person (the POV lock), and a body standing on
/// that spot would fill the screen (playtest list 7 item 116: in the original the camera
/// stays on the slideshow's screen with no body in view [the owner, 2026-11-09]). They are
/// drawn again when the scene lets go.
pub(super) fn hide_players_in_scene(
    coop: Res<Coop>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    mut bodies: Query<&mut Visibility, With<super::Puppet>>,
    mut hidden: Local<bool>,
) {
    let held = (coop.is_host() || coop.is_guest()) && script.as_ref().and_then(|s| s.rt.as_ref()).is_some_and(|rt| scene_holds_view(rt.game.controls_locked()));
    if held != *hidden {
        *hidden = held;
        info!("co-op: scene: the other players' bodies {}", if held { "hidden while the scene holds the view" } else { "drawn again" });
    }
    let want = if held { Visibility::Hidden } else { Visibility::Inherited };
    for mut v in &mut bodies {
        v.set_if_neq(want);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn beside_is_to_the_right() {
        // Facing north, the right is east (+X).
        let f = beside([0.0, 0.0, 10.0], 0.0);
        assert!((f[0] - BESIDE).abs() < 1e-3 && f[1].abs() < 1e-3 && f[2] == 10.0);
        // Facing east, the right is south (-Y).
        let f = beside([0.0, 0.0, 0.0], std::f32::consts::FRAC_PI_2);
        assert!(f[0].abs() < 1e-3 && (f[1] + BESIDE).abs() < 1e-3);
    }
}
