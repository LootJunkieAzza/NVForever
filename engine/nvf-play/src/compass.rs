//! The marks on the HUD's compass and the discovery of map markers
//! (docs/spec-notes/hud.md §11; owner's playtest list 5, items 12 and 57).
//!
//! - **Actors**: every living NPC and creature of the loaded cells within the player's
//!   Perception × [`ACTOR_RANGE_PER_PERCEPTION`] gets a tick on the compass at its
//!   bearing: amber (the HUD's colour) for the others, red (the crime colour) for those
//!   hostile to the player — fighting the player or ready to attack it, as the enemy
//!   health bar decides (`crate::hud_target`) [observed: the owner's screenshots in
//!   C:\fnv-private\Creature Markers, a red tick over a gecko, an amber one over an NCR
//!   trooper; the pictures are the HUD file's `compass_npc_icon`, `glow_hud_tick_mark.dds`].
//!   An actor that sneaks (as IsSneaking reads it) gets none (`fnv_game::Game::compass_shows`)
//!   [observed: the owner, 2026-10-23].
//! - **Map markers**: every map marker of the worldspace within Perception ×
//!   [`MARKER_RANGE_PER_PERCEPTION`], while the player is outdoors: a hollow triangle
//!   until the player has been there, a filled one after [observed: the owner's
//!   screenshots in C:\fnv-private\Map Markers; the pictures `glow_hud_compass_landmark`
//!   and `…_landmark_discovered.dds`].
//! - **Discovery**: the Pip-Boy's DATA page notes the markers the player comes within the
//!   radius of (`crate::pipboy::data`); each new one shows "You have discovered" over its
//!   name where quests are announced (`crate::quest_messages`), and gives the player
//!   `iXPRewardDiscoverMapMarker` experience [observed: 10 in FalloutNV.esm, and "+10 XP"
//!   on the owner's screenshot]. A save keeps the markers found.
//! - **Quest targets**: the active quest's marked targets (the Pip-Boy's, whose
//!   conditions pass: `crate::pipboy::data::refresh_quest_targets`) at any distance, as
//!   `glow_hud_compass_objective_marker.dds` drawn over the others [documented:
//!   `menus/prefabs/hudtemplates.xml`, `template_compass_icon_quest`: that picture,
//!   40 × 50 at y 5 as the map marker's, depth 5 over the map marker's 3]. A target in
//!   another place (an interior, or another worldspace) is marked at the door of the
//!   player's place that starts the way to it through the fewest load doors, avoiding
//!   locked ones unless the target ignores locks (`fnv_game::quest_targets::DoorGraph`)
//!   [hypothesis: the shortest way; the GECK wiki says the marker "will attempt to avoid
//!   locked doors if possible"] (docs/spec-notes/hud.md §11). The original's compass
//!   shows no distance: its icon file sets `_Distance` and nothing reads it [documented:
//!   `menus/prefabs/compassicon.xml`].
//! - The ranges are guesses: no game setting for them is in the masters [hypothesis].
//! - Who and what is near is looked at ten times a second ([`gather`], [`CompassNear`]);
//!   where each mark stands on the compass is worked out every frame from the camera's
//!   heading, the player's place and the actors' moving bodies ([`place`]), unrounded,
//!   so the marks move as smoothly as the strip (T12d) [design: at ten places a second,
//!   and in whole units, they stepped while the player turned or the actors walked,
//!   where the original's glide, the owner's play of 2026-10-29].

use crate::hud::{CompassMarks, COMPASS_HEADING, STRIP};
use fnv_game::quest_targets::{DoorGraph, Place as QuestPlace};
use crate::interface::{amber, at, bc_supported, texture, u};
use crate::phys::PhysState;
use crate::pipboy::data::DataState;
use crate::player::Player;
use crate::quest_messages::QuestMessages;
use crate::reticle::crime_colour;
use crate::save::SaveHook;
use crate::script::{with_world, ScriptState};
use crate::script_events::QuestNotice;
use crate::sim::SimState;
use crate::{Location, PlaySet, PlayState};
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::GameData;
use std::collections::BTreeSet;

/// Game units of range per point of Perception for actors and for map markers
/// [hypothesis: Perception 5 shows actors within about 21 m and markers within about
/// 146 m (1 unit ≈ 1.43 cm); the owner asks for actors "in a close proximity"].
pub const ACTOR_RANGE_PER_PERCEPTION: f32 = 300.0;
pub const MARKER_RANGE_PER_PERCEPTION: f32 = 2048.0;
/// The experience for finding a marker without the game setting [observed: its value].
const DISCOVER_XP: f64 = 10.0;

/// Seconds between looks at who is near [design].
const POLL: f32 = 0.1;

// ---- the pictures and where they stand [observed: fitted to the owner's screenshots] ----

const TICK: &str = "textures\\interface\\hud\\glow_hud_tick_mark.dds";
const LANDMARK: &str = "textures\\interface\\hud\\glow_hud_compass_landmark.dds";
const LANDMARK_FOUND: &str = "textures\\interface\\hud\\glow_hud_compass_landmark_discovered.dds";
const QUEST: &str = "textures\\interface\\hud\\glow_hud_compass_objective_marker.dds";
/// The tick (a 32-texel picture, its bar at texel 9 [observed: hud.md §14.5]) drawn this
/// size, its top this far down the compass window, its bar's middle on the bearing
/// [design: fitted to the owner's screenshots, hud.md §11].
const TICK_SIZE: f32 = 26.0;
const TICK_TOP: f32 = 9.0;
const TICK_MIDDLE: f32 = 9.0 / 32.0;
/// The triangle (a 64-texel picture, the triangle's middle at texel 19, 15) drawn this
/// size, its middle this far down the window.
const LANDMARK_SIZE: f32 = 38.0;
const LANDMARK_MIDDLE: Vec2 = Vec2::new(19.0 / 64.0, 15.0 / 64.0);
const LANDMARK_Y: f32 = 46.7;

/// A mark's kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Mark {
    /// An actor not hostile to the player.
    Actor,
    Hostile,
    /// A companion, under the player's Companion Pip Colour tweak [design: docs/design/tweaks.md].
    Companion,
    /// A map marker the player has not been to, and one it has.
    Place,
    Found,
    /// A target of the active quest (drawn last, over the others).
    Quest,
}

/// Compass units right of the heading point at which a thing at `to` stands for a
/// player at `from` facing `heading` (degrees, 0 north, 90 east): the bearing's
/// difference, wrapped to −180…180, at the strip's 1024 units to 360 degrees
/// [observed: hud.md §14.5].
pub fn offset(heading: f32, from: [f32; 2], to: [f32; 2]) -> f32 {
    let bearing = (to[0] - from[0]).atan2(to[1] - from[1]).to_degrees();
    let d = (bearing - heading + 540.0).rem_euclid(360.0) - 180.0;
    d * STRIP.x / 360.0
}

/// One of eight directions (0 front, 1 front right … 4 back … 7 front left) for a mark
/// `x` compass units right of the heading point (GetNearestCompassHostileDirection,
/// `fnv_game::nvse_jg9`) [hypothesis: eight equal sectors about the heading].
pub fn direction_of(x: f32) -> u8 {
    let degrees = x * 360.0 / STRIP.x;
    ((degrees / 45.0).round() as i32).rem_euclid(8) as u8
}

/// The visible distance either side of the heading point
/// [observed: hud.md §14.5 for actor ticks and one place mark;
/// hypothesis: the same for quest marks].
const COMPASS_VISIBLE: f32 = 139.0;

/// Whether a mark at `x` from the heading point is in the visible band
/// [observed: hud.md §14.5 for actor ticks and one place mark;
/// hypothesis: the same for quest marks].
pub fn on_compass(x: f32) -> bool {
    x.abs() <= COMPASS_VISIBLE
}

/// Where a quest target's mark stands for a target `x` right of the heading point: there
/// inside the visible band, else at the band's end on its side [observed: hud_compare
/// hud-bars-partial, hud-ammo-label, perk-meltdown-anatomy and xp-meter at 1280 x 720, the
/// original's objective marker held under the line's left end, its middle at x 53.5-54
/// pixels (the band's end, its `_Heading` 0 with the icon's 40-unit box), where the
/// in-band one stands at its bearing (needs_limbs, at SE); hypothesis: the right end for a
/// target to the right, all four were at the left]. Until T12e a target off the band had
/// no mark [hypothesis: the band of actor ticks and places, hud.md §14.5].
pub fn pinned(x: f32) -> f32 {
    x.clamp(-COMPASS_VISIBLE, COMPASS_VISIBLE)
}

/// The marks drawn: kind and place across in compass units right of the heading point,
/// unrounded (T12d), worked out every frame by [`place`].
#[derive(Resource, Default, Debug, PartialEq, Clone)]
pub struct CompassView(pub Vec<(Mark, f32)>);

/// A mark the last look found ([`gather`]): its kind, where it stood (game x, y) and the
/// actor whose moving body ([`PhysState::npcs`]) it follows between looks.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Near {
    pub mark: Mark,
    pub at: [f32; 2],
    pub body: Option<u32>,
}

/// The marks in range at the last look, whatever their bearing.
#[derive(Resource, Default, Debug, Clone)]
pub struct CompassNear(pub Vec<Near>);

/// The marks of `near` on the compass for a player at `here` facing `heading`
/// (degrees), each actor at its body's feet in `bodies` when it has one, sorted by kind
/// then place [design]. A quest target outside the visible band stays on the compass,
/// pinned at the band's end on its side ([`pinned`]).
pub fn marks(near: &[Near], heading: f32, here: [f32; 2], bodies: &dyn Fn(u32) -> Option<[f32; 2]>) -> Vec<(Mark, f32)> {
    marks_far(near, heading, here, bodies).into_iter().map(|(m, x, _)| (m, x)).collect()
}

/// [`marks`] with each mark's distance from the player (game units).
pub fn marks_far(near: &[Near], heading: f32, here: [f32; 2], bodies: &dyn Fn(u32) -> Option<[f32; 2]>) -> Vec<(Mark, f32, f32)> {
    let mut out: Vec<(Mark, f32, f32)> = near
        .iter()
        .filter_map(|n| {
            let at = n.body.and_then(bodies).unwrap_or(n.at);
            let x = offset(heading, here, at);
            let d = (at[0] - here[0]).hypot(at[1] - here[1]);
            match n.mark {
                Mark::Quest => Some((n.mark, pinned(x), d)),
                _ => on_compass(x).then_some((n.mark, x, d)),
            }
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0).then(a.1.total_cmp(&b.1)));
    out
}

/// Each mark's strength in [`CompassView`]'s order: 1, or under the player's
/// Fade Pips by Distance tweak an actor's less the further it is within the sneak
/// range [design: docs/design/tweaks.md].
#[derive(Resource, Default, Debug, PartialEq, Clone)]
pub struct CompassAlpha(pub Vec<f32>);

/// The newly dead the compass has seen: since when, and whether the crosshair has been
/// on them (the Recently Dead tweak) [design: docs/design/tweaks.md].
type DeadSeen = std::collections::HashMap<u32, (f32, bool)>;

/// How faint an actor's pip at the sneak range's end gets (the tweak) [design].
const FARTHEST_PIP: f32 = 0.25;

/// The reach places show on the compass at the last look (game units, as `f32` bits),
/// for the Fade Places by Distance tweak [design: docs/design/tweaks.md].
static PLACE_RANGE: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

pub struct CompassPlugin;

impl Plugin for CompassPlugin {
    fn build(&self, app: &mut App) {
        // The marks are placed after the player and the camera move and before the tile
        // HUD's writes (`crate::tiles`), so both drawings put them with this frame's strip.
        app.init_resource::<CompassView>()
            .init_resource::<CompassAlpha>()
            .init_resource::<CompassNear>()
            .add_systems(Update, place.after(PlaySet::Physics).before(PlaySet::Activation))
            .add_systems(Update, (discover, crate::challenges::notices, gather, draw).chain().in_set(PlaySet::Ui).after(crate::hud::HudSet))
            .add_systems(Update, crate::pipboy::data::refresh_quest_targets.in_set(PlaySet::Ui));
        crate::save::add_save_hook(app, SAVE_HOOK);
    }
}

const SAVE_HOOK: SaveHook = SaveHook {
    key: "map_markers_found",
    save: |w| serde_json::to_value(w.get_resource::<DataState>()?.discovered()).ok(),
    load: |w, v| {
        let found: BTreeSet<u32> = serde_json::from_value(v.clone()).unwrap_or_default();
        DataState::restore_discovered(w, found);
    },
    // Marker references by FormID (numbers): moved to the current load order, those of
    // plugins no longer loaded dropped.
    remap: Some(|v, r| {
        if let Some(a) = v.as_array_mut() {
            *a = a.iter().filter_map(|x| x.as_u64().and_then(|n| r.map(n as u32)).map(serde_json::Value::from)).collect();
        }
    }),
};

/// Markers found since the last frame: the message, the experience, the log.
#[allow(clippy::too_many_arguments)]
pub(crate) fn discover(state: Option<ResMut<DataState>>, mut script: Option<ResMut<ScriptState>>, mut sim: ResMut<SimState>, messages: Option<ResMut<QuestMessages>>, loading_save: Option<Res<crate::save::PendingEngine>>, mut corner: Option<ResMut<crate::corner_messages::CornerMessages>>, tweaks: Option<Res<crate::tweaks::TweakSettings>>, (player, location): (Option<Res<Player>>, Option<Res<Location>>)) {
    let in_corner = tweaks.is_some_and(|t| t.discovered_in_corner);
    let Some(mut state) = state else { return };
    // Found before the game runs (the first frames): kept until it does, for the XP.
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    // A save is loading: its found markers are not back yet and drop from the queue
    // what the save already holds (`DataState::restore_discovered`; #1571).
    if loading_save.is_some() {
        return;
    }
    // The player's tweak marks the map markers near the player on the map [design: docs/design/tweaks.md].
    let reach = rt.game.tweaks.mark_nearby_places;
    if let (true, Some(p), Some(loc)) = (reach > 0.0, player.as_deref(), location.as_deref()) {
        if loc.interior.is_none() {
            for (form, pos, _) in state.markers_of(loc.world) {
                let near = (pos[0] - p.feet[0]).hypot(pos[1] - p.feet[1]) <= reach;
                if near && rt.game.player.map_markers.insert(FormId(form)) {
                    info!("compass: {} marked on the map, within {reach:.0} units (tweak)", FormId(form));
                }
            }
        }
    }
    // The rules hear which markers are found (GetMapMarkerVisible), also after a load.
    if rt.game.player.found_markers.len() != state.discovered().len() {
        rt.game.set_found_markers(state.discovered().iter().map(|&f| FormId(f)));
    }
    let found = state.take_newly_found();
    if found.is_empty() {
        return;
    }
    let mut messages = messages;
    let xp = sim.forms.setting_f32("iXPRewardDiscoverMapMarker").map_or(DISCOVER_XP, |v| v as f64);
    for (form, name) in found {
        let gained = fnv_game::experience::award(&mut rt.game, fnv_game::experience::Source::Discovery, xp);
        // Discovery challenges and Locations Discovered ones (`fnv_game::challenges`), and
        // JIP's location-discover event, whose handlers see the world.
        with_world(rt, &mut sim, |rt| rt.game.challenge_marker_found(FormId(form)));
        info!("compass: discovered {name} ({}), +{gained} XP", FormId(form));
        // The player's tweak puts it in the corner instead of the banner [design: docs/design/tweaks.md].
        if in_corner && !name.trim().is_empty() {
            if let Some(c) = corner.as_deref_mut() {
                c.push(format!("Discovered: {name}").into());
            }
        } else if let (Some(m), false) = (messages.as_deref_mut(), name.trim().is_empty()) {
            m.push(QuestNotice::Discovered(name));
        }
    }
}

/// The NPCs and creatures of the loaded cells (and those a script moved in), with their
/// feet: a moving body's (`PhysState::npcs`) or the reference's place.
pub(crate) fn actors_near(g: &fnv_game::Game, bodies: &std::collections::HashMap<u32, [f32; 3]>) -> Vec<(FormId, [f32; 3])> {
    let placed = g.world.placed;
    let is_actor = |r: &FormId| placed.get(*r).is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE"));
    let mut ids: BTreeSet<FormId> = g.loaded_cells.iter().filter_map(|c| placed.by_cell.get(c)).flatten().copied().filter(is_actor).collect();
    ids.extend(g.moved.iter().copied().filter(is_actor));
    ids.into_iter()
        .filter(|r| g.cell_of(*r).is_some_and(|c| g.loaded_cells.contains(&c)))
        .filter_map(|r| {
            let feet = bodies.get(&r.0).copied().or_else(|| g.world.peek(r).map(|s| s.pos)).or_else(|| placed.get(r).map(|p| p.pos))?;
            Some((r, feet))
        })
        .collect()
}

/// How far the compass shows actors: the player's Perception × [`ACTOR_RANGE_PER_PERCEPTION`],
/// or its most, Perception 10's, with Enhanced Sensors (entry point 69 Has Improved
/// Detection, "Max compass range" [documented: GECK wiki Perk entry points];
/// perk-entry-points.md §13).
pub(crate) fn actor_range(g: &mut fnv_game::Game) -> f32 {
    let perception = fnv_game::functions::actor_value_capped(g, FormId::PLAYER_REF, fnv_game::actor_values::PERCEPTION) as f32;
    let sensors = g.player_has_entry(fnv_game::perks::HAS_IMPROVED_DETECTION);
    let p = if sensors { MAX_PERCEPTION } else { perception.max(1.0) };
    p * ACTOR_RANGE_PER_PERCEPTION
}

/// The most a SPECIAL reaches [documented: GECK wiki Actor Value Indices, 1 to 10].
const MAX_PERCEPTION: f32 = 10.0;

/// Who and what is near, ten times a second.
#[allow(clippy::too_many_arguments)]
fn gather(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    control: Option<Res<crate::control::PlayerControl>>,
    location: Option<Res<Location>>,
    phys: Option<Res<PhysState>>,
    markers: Option<Res<DataState>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut poll: Local<f32>,
    mut near: ResMut<CompassNear>,
    mut logged: Local<[usize; 5]>,
    mut graph: Local<Option<DoorGraph>>,
    mut logged_quest: Local<Option<(usize, usize)>>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    (mut dead_seen, target): (Local<DeadSeen>, Option<Res<crate::activate::Target>>),
) {
    // The player's tweak leaves hostile actors off the compass [design: docs/design/tweaks.md].
    let hide_foes = tweaks.as_ref().is_some_and(|t| t.hide_enemy_markers);
    let quest_reach = tweaks.as_ref().map_or(0.0, |t| t.quest_marker_reach);
    let pips_in_danger_only = tweaks.as_ref().is_some_and(|t| t.compass_pips_in_danger_only);
    let neutral_foes = tweaks.as_ref().is_some_and(|t| t.neutral_foes_on_compass);
    let companion_pips = tweaks.as_ref().is_some_and(|t| t.companion_pip > 0);
    let nearest_unfound = tweaks.as_ref().is_some_and(|t| t.nearest_unfound_on_compass);
    // The player's tweak keeps the newly dead on the compass for a while, until the
    // crosshair has been on them [design: docs/design/tweaks.md].
    let dead_secs = tweaks.as_ref().map_or(0.0, |t| t.recent_dead_secs);
    let now = time.elapsed_secs();
    let looked = target.as_ref().and_then(|t| t.0.as_ref()).map(|t| t.form_id);
    *poll += time.delta_secs();
    if *poll < POLL || *state.get() != PlayState::Playing {
        return;
    }
    *poll = 0.0;
    let (Some(player), Some(phys)) = (player, phys) else { return };
    // Marks stand at their bearing from the camera's heading point, shared with the
    // strip [observed: hud.md §14.5].
    let heading = control.as_deref().map_or(player.heading, |c| c.view_heading(player.heading)).to_degrees();
    let feet = player.feet;
    let here = [feet[0], feet[1]];
    let bodies: std::collections::HashMap<u32, [f32; 3]> = phys.npcs.iter().map(|(&id, b)| (id, b.ch.feet)).collect();
    // Every mark in range, and the kinds of those in the compass's window now (the log).
    let mut found_near: Vec<Near> = Vec::new();
    let mut marks: Vec<Mark> = Vec::new();
    let mut perception = 5.0;
    // Actors in range and in the window left off because they sneak.
    let mut sneakers = 0;
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        let found = with_world(rt, &mut sim, |rt| {
            let g = &mut rt.game;
            perception = fnv_game::functions::actor_value_capped(g, FormId::PLAYER_REF, fnv_game::actor_values::PERCEPTION) as f32;
            let range = actor_range(g);
            let mut out = Vec::new();
            // The nearest hostile mark's distance and offset, of all and of those not
            // invisible (Invisibility or Chameleon).
            let mut nearest: [Option<(f32, f32)>; 2] = [None; 2];
            for (r, at) in actors_near(g, &bodies) {
                let d = ((at[0] - feet[0]).powi(2) + (at[1] - feet[1]).powi(2) + (at[2] - feet[2]).powi(2)).sqrt();
                if d > range || !g.present(r) {
                    continue;
                }
                if g.is_dead(r) {
                    if dead_secs > 0.0 {
                        let (since, seen) = dead_seen.entry(r.0).or_insert((now, false));
                        *seen |= looked == Some(r.0);
                        if !*seen && now - *since < dead_secs {
                            let shown = on_compass(offset(heading, here, [at[0], at[1]]));
                            out.push((Near { mark: Mark::Actor, at: [at[0], at[1]], body: None }, shown));
                        }
                    }
                    continue;
                }
                let shown = on_compass(offset(heading, here, [at[0], at[1]]));
                if !g.compass_shows(r) {
                    sneakers += usize::from(shown);
                    continue;
                }
                let fighting = g.awareness.get(r).and_then(|a| a.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
                let hostile = fighting || g.would_attack(r, FormId::PLAYER_REF);
                if hostile {
                    let x = offset(heading, here, [at[0], at[1]]);
                    let unseen = ["Invisibility", "Chameleon"].iter().filter_map(|n| fnv_game::functions::av_by_name(n)).any(|c| fnv_game::functions::actor_value_capped(g, r, c) > 0.0);
                    for k in [0, 1] {
                        if (k == 0 || !unseen) && nearest[k].is_none_or(|(nd, _)| d < nd) {
                            nearest[k] = Some((d, x));
                        }
                    }
                }
                if hostile && hide_foes {
                    continue;
                }
                // The player's tweaks: no pips unless in danger, and foes in the usual colour [design: docs/design/tweaks.md].
                if pips_in_danger_only && crate::hud::sneak_now(g) != crate::hud::Sneak::Danger {
                    continue;
                }
                let mark = if hostile && !neutral_foes {
                    Mark::Hostile
                } else if companion_pips && g.is_teammate(r) {
                    Mark::Companion
                } else {
                    Mark::Actor
                };
                out.push((Near { mark, at: [at[0], at[1]], body: bodies.contains_key(&r.0).then_some(r.0) }, shown));
            }
            g.other.jg9.compass_hostile = nearest.map(|n| n.map(|(_, x)| direction_of(x)));
            out
        });
        for (n, shown) in found {
            if shown {
                marks.push(n.mark);
            }
            found_near.push(n);
        }
    }
    if let (Some(loc), Some(st)) = (location.as_deref(), markers.as_deref()) {
        if loc.interior.is_none() {
            let range = perception.max(1.0) * MARKER_RANGE_PER_PERCEPTION;
            PLACE_RANGE.store(range.to_bits(), std::sync::atomic::Ordering::Relaxed);
            // The player's tweak: a place not found yet shows only when the map shows it
            // [design: docs/design/tweaks.md].
            let unmapped = if tweaks.as_ref().is_some_and(|t| t.hide_unmapped_places) {
                st.unmapped_of(loc.world, script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game))
            } else {
                Default::default()
            };
            for (form, pos, found) in st.markers_of(loc.world) {
                if !sim.present(form) || unmapped.contains(&form) || (pos[0] - feet[0]).hypot(pos[1] - feet[1]) > range {
                    continue;
                }
                let mark = if found { Mark::Found } else { Mark::Place };
                if on_compass(offset(heading, here, [pos[0], pos[1]])) {
                    marks.push(mark);
                }
                found_near.push(Near { mark, at: [pos[0], pos[1]], body: None });
            }
            // The player's tweak: the nearest place not yet found shows at any distance
            // [design: docs/design/tweaks.md].
            if nearest_unfound {
                let away = |pos: [f32; 3]| (pos[0] - feet[0]).hypot(pos[1] - feet[1]);
                let nearest = st.markers_of(loc.world).into_iter().filter(|(form, _, found)| !found && sim.present(*form) && !unmapped.contains(form)).min_by(|a, b| away(a.1).total_cmp(&away(b.1)));
                if let Some((_, pos, _)) = nearest.filter(|n| away(n.1) > range) {
                    if on_compass(offset(heading, here, [pos[0], pos[1]])) {
                        marks.push(Mark::Place);
                    }
                    found_near.push(Near { mark: Mark::Place, at: [pos[0], pos[1]], body: None });
                }
            }
        }
    }
    // The active quest's targets: how many have a place (themselves or a door), and how
    // many of those are in the compass's window now.
    let (mut placed, mut shown) = (0, 0);
    if let (Some(loc), Some(st)) = (location.as_deref(), markers.as_deref()) {
        for c in st.marked_targets() {
            let graph = graph.get_or_insert_with(|| DoorGraph::build(sim.refs));
            let Some((pos, itself)) = quest_mark_at(&sim, loc, graph, c.target, c.ignores_locks) else { continue };
            // The player's tweak: quest targets past this far are left off [design: docs/design/tweaks.md].
            if quest_reach > 0.0 && (pos[0] - feet[0]).hypot(pos[1] - feet[1]) > quest_reach {
                continue;
            }
            placed += 1;
            if on_compass(offset(heading, here, [pos[0], pos[1]])) {
                marks.push(Mark::Quest);
                shown += 1;
            }
            // A target that walks is followed by its body; a door stays.
            let body = (itself && bodies.contains_key(&c.target.0)).then_some(c.target.0);
            found_near.push(Near { mark: Mark::Quest, at: [pos[0], pos[1]], body });
        }
    }
    if *logged_quest != Some((placed, shown)) {
        // The others are held at the compass's ends ([`pinned`]).
        info!("compass: {placed} quest target(s) placed, {shown} on the compass, {} held at an end", placed - shown);
        *logged_quest = Some((placed, shown));
    }
    let count = |k: &[Mark]| marks.iter().filter(|m| k.contains(m)).count();
    let counts = [count(&[Mark::Actor, Mark::Hostile, Mark::Companion]), count(&[Mark::Hostile]), count(&[Mark::Place, Mark::Found]), count(&[Mark::Found]), sneakers];
    if counts != *logged {
        let [actors, hostile, markers, found, sneakers] = counts;
        info!("compass: {actors} actors ({hostile} hostile), {markers} map markers ({found} found) on the compass, Perception {perception}, {sneakers} sneaking left off");
        *logged = counts;
    }
    near.0 = found_near;
}

/// Where the marks stand this frame (see the module notes): from the last look's
/// marks, the camera's heading (shared with the strip [observed: hud.md §14.5]) and the
/// player's and the actors' places now.
pub(crate) fn place(
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    control: Option<Res<crate::control::PlayerControl>>,
    phys: Option<Res<PhysState>>,
    near: Res<CompassNear>,
    mut view: ResMut<CompassView>,
    (mut alpha, sim, tweaks): (ResMut<CompassAlpha>, Option<Res<SimState>>, Option<Res<crate::tweaks::TweakSettings>>),
) {
    if *state.get() != PlayState::Playing {
        return;
    }
    let Some(player) = player else { return };
    let heading = control.as_deref().map_or(player.heading, |c| c.view_heading(player.heading)).to_degrees();
    let here = [player.feet[0], player.feet[1]];
    let body = |id: u32| phys.as_deref().and_then(|p| p.npcs.get(&id)).map(|b| [b.ch.feet[0], b.ch.feet[1]]);
    let far = marks_far(&near.0, heading, here, &body);
    view.set_if_neq(CompassView(far.iter().map(|&(m, x, _)| (m, x)).collect()));
    // The player's tweak: the sneak range, fSneakMaxDistance (times
    // fSneakExteriorDistanceMult outdoors), fades actors' pips [design: docs/design/tweaks.md].
    let fade = tweaks.as_ref().is_some_and(|t| t.compass_pips_fade);
    let range = if fade {
        let d = fnv_game::detection::Settings::default();
        let setting = |name: &str, or: f64| sim.as_deref().and_then(|s| s.forms.setting_f32(name)).unwrap_or(or as f32);
        let outdoors = phys.as_deref().is_some_and(|p| p.space == crate::phys::cells::Space::Exterior);
        setting("fSneakMaxDistance", d.max_distance) * if outdoors { setting("fSneakExteriorDistanceMult", d.exterior_distance_mult) } else { 1.0 }
    } else {
        0.0
    };
    // And its other tweak fades places over the reach they show at.
    let places = if tweaks.as_ref().is_some_and(|t| t.compass_places_fade) { f32::from_bits(PLACE_RANGE.load(std::sync::atomic::Ordering::Relaxed)) } else { 0.0 };
    let shade = |&(m, _, d): &(Mark, f32, f32)| match m {
        Mark::Actor | Mark::Hostile | Mark::Companion if range > 0.0 => (1.0 - d / range).clamp(FARTHEST_PIP, 1.0),
        Mark::Place | Mark::Found if places > 0.0 => (1.0 - d / places).clamp(FARTHEST_PIP, 1.0),
        _ => 1.0,
    };
    alpha.set_if_neq(CompassAlpha(far.iter().map(shade).collect()));
}

/// Where the compass marks quest target `target` for a player at `loc` (see the module
/// notes): the target itself in the player's cell or worldspace, else the door of the
/// player's place that starts the way to it (`fnv_game::quest_targets::DoorGraph`);
/// `None` when no way through doors is known. True with the target itself, false with a
/// door.
fn quest_mark_at(sim: &SimState, loc: &Location, graph: &DoorGraph, target: FormId, ignores_locks: bool) -> Option<([f32; 3], bool)> {
    let refs = sim.refs;
    let (cell, pos) = match sim.world.peek(target) {
        Some(s) => (s.cell?, s.pos),
        None => {
            let p = refs.get(target)?;
            (p.cell?, p.pos)
        }
    };
    let here = match loc.interior {
        Some(c) => QuestPlace::Interior(FormId(c)),
        None => QuestPlace::World(FormId(loc.world)),
    };
    let there = QuestPlace::of_cell(refs, cell);
    if here == there {
        return Some((pos, true));
    }
    let door = graph.first_door(&sim.world, here, there, ignores_locks)?;
    sim.world.peek(door).map(|s| s.pos).or_else(|| refs.get(door).map(|d| d.pos)).map(|p| (p, false))
}

struct Art {
    tick: Option<Handle<Image>>,
    landmark: Option<Handle<Image>>,
    found: Option<Handle<Image>>,
    quest: Option<Handle<Image>>,
}

/// The Compass Fade Left Side tweak's shade of a mark `x` right of the heading point:
/// whole from the middle rightwards, down to a fifth at the left end (ours) [design: docs/design/tweaks.md].
fn left_fade(x: f32) -> f32 {
    if x >= 0.0 {
        1.0
    } else {
        (1.0 + x / COMPASS_VISIBLE).clamp(0.2, 1.0)
    }
}

/// A mark's node's left edge in the compass window for a mark `x` right of the heading
/// point: the tick's bar's middle, the triangle's middle on the bearing; without the
/// game's pictures a 4-unit block's middle.
fn mark_left(mark: Mark, x: f32, picture: bool) -> f32 {
    let x = COMPASS_HEADING + x;
    match (mark, picture) {
        (_, false) => x - 2.0,
        (Mark::Actor | Mark::Hostile | Mark::Companion, true) => x - TICK_MIDDLE * TICK_SIZE,
        _ => x - LANDMARK_MIDDLE.x * LANDMARK_SIZE,
    }
}

/// The marks' nodes as spawned: each mark's kind and node, in [`CompassView`]'s order.
#[derive(Default)]
struct Spawned(Vec<(Mark, Entity)>);

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    view: Res<CompassView>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    layers: Query<Entity, With<CompassMarks>>,
    added: Query<(), Added<CompassMarks>>,
    mut nodes: Query<&mut Node>,
    mut art: Local<Option<Art>>,
    mut spawned: Local<Spawned>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    alpha: Option<Res<CompassAlpha>>,
) {
    let companion = crate::tweaks::companion_colour(tweaks.as_deref());
    // The HUD's lengths at the UI scale (`crate::interface::hud_units`, #1631).
    let _hud = crate::interface::hud_units();
    let Ok(layer) = layers.single() else { return };
    let fresh = added.get(layer).is_ok();
    let alpha_changed = alpha.as_ref().is_some_and(|a| a.is_changed());
    if !view.is_changed() && !fresh && !alpha_changed {
        return;
    }
    let art = art.get_or_insert_with(|| {
        let bc = bc_supported(formats.as_deref());
        let mut read = |p: &str| data.as_deref().and_then(|d| texture(p, d, &mut images, bc)).map(|t| t.0);
        Art { tick: read(TICK), landmark: read(LANDMARK), found: read(LANDMARK_FOUND), quest: read(QUEST) }
    });
    let image_of = |mark: Mark| match mark {
        Mark::Actor | Mark::Hostile | Mark::Companion => &art.tick,
        Mark::Found => &art.found,
        Mark::Quest => &art.quest,
        Mark::Place => &art.landmark,
    };
    // The same kinds in the same order: the nodes move, every frame while the player
    // turns, without being spawned again (T12d) [design].
    // The player's tweak: marks fade toward the compass's left end; drawn again each
    // frame for their shade [design: docs/design/tweaks.md].
    let fade_left = tweaks.as_ref().is_some_and(|t| t.compass_fade_left);
    if !fade_left && !alpha_changed && !fresh && spawned.0.len() == view.0.len() && spawned.0.iter().zip(&view.0).all(|(a, b)| a.0 == b.0) {
        for (&(mark, entity), &(_, x)) in spawned.0.iter().zip(&view.0) {
            if let Ok(mut node) = nodes.get_mut(entity) {
                node.left = u(mark_left(mark, x, image_of(mark).is_some()));
            }
        }
        return;
    }
    let mut out = Vec::with_capacity(view.0.len());
    commands.entity(layer).despawn_related::<Children>().with_children(|p| {
        for (k, &(mark, x)) in view.0.iter().enumerate() {
            let image = image_of(mark);
            let left = mark_left(mark, x, image.is_some());
            let (node, colour) = match mark {
                Mark::Actor | Mark::Hostile | Mark::Companion => {
                    let colour = match mark {
                        Mark::Hostile => crime_colour(),
                        Mark::Companion => companion.unwrap_or_else(|| amber(255.0)),
                        _ => amber(255.0),
                    };
                    (at(left, TICK_TOP, TICK_SIZE, TICK_SIZE), colour)
                }
                // The quest marker in the map marker's box (the same 40 x 50 at y 5 in the
                // HUD file) [hypothesis: its picture is laid out as the landmark's].
                Mark::Place | Mark::Found | Mark::Quest => (at(left, LANDMARK_Y - LANDMARK_MIDDLE.y * LANDMARK_SIZE, LANDMARK_SIZE, LANDMARK_SIZE), amber(255.0)),
            };
            let colour = if fade_left && mark != Mark::Quest { colour.with_alpha(left_fade(x)) } else { colour };
            // The Fade Pips by Distance tweak (`CompassAlpha`).
            let colour = match alpha.as_ref().and_then(|a| a.0.get(k)).copied().filter(|a| *a < 1.0) {
                Some(a) => colour.with_alpha(colour.alpha() * a),
                None => colour,
            };
            let id = match image {
                Some(image) => p.spawn((node, ImageNode { color: colour, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) })).id(),
                // Without the game's pictures: a small block.
                None => p.spawn((at(left, if matches!(mark, Mark::Actor | Mark::Hostile | Mark::Companion) { 10.0 } else { 40.0 }, 4.0, 12.0), BackgroundColor(colour))).id(),
            };
            out.push((mark, id));
        }
    });
    spawned.0 = out;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_stands_at_its_bearing_from_the_heading() {
        // North of a player facing north: at the heading point; east: a quarter strip
        // right [observed: hud.md §14.5].
        assert_eq!(offset(0.0, [0.0, 0.0], [0.0, 100.0]), 0.0);
        assert!((offset(0.0, [0.0, 0.0], [100.0, 0.0]) - 256.0).abs() < 1e-3);
        // Facing east, a thing to the north is a quarter strip left.
        assert!((offset(90.0, [0.0, 0.0], [0.0, 100.0]) + 256.0).abs() < 1e-3);
        // The difference wraps: facing 350, a thing at 10 is 20 degrees right.
        let ten = [10f32.to_radians().sin(), 10f32.to_radians().cos()];
        assert!((offset(350.0, [0.0, 0.0], ten) - 20.0 * 1024.0 / 360.0).abs() < 1e-2);
        // Behind the player: off the compass.
        assert!(!on_compass(offset(0.0, [0.0, 0.0], [0.0, -100.0])));
        assert!(on_compass(offset(0.0, [0.0, 0.0], [30.0, 100.0])));
    }

    /// T12d: the marks keep their place to a fraction of a unit, move with the heading
    /// every frame, and an actor's mark follows its body between looks.
    #[test]
    fn the_marks_move_smoothly_with_the_heading_and_the_bodies() {
        let near = [
            Near { mark: Mark::Actor, at: [0.0, 100.0], body: Some(7) },
            Near { mark: Mark::Place, at: [0.0, 100.0], body: None },
            Near { mark: Mark::Quest, at: [0.0, -100.0], body: None },
        ];
        let none = |_: u32| None;
        // Turning a tenth of a degree moves the marks 1024 / 3600 units, not 0 or 1.
        let a = marks(&near, 0.0, [0.0, 0.0], &none);
        let b = marks(&near, 0.1, [0.0, 0.0], &none);
        assert_eq!(a.iter().map(|m| m.0).collect::<Vec<_>>(), [Mark::Actor, Mark::Place, Mark::Quest]);
        // (The quest target straight behind is held at an end, the left at 180 degrees off
        // and the right just under: not compared.)
        for (a, b) in a.iter().zip(&b).take(2) {
            assert!((a.1 - b.1 - 1024.0 / 3600.0).abs() < 1e-3, "{a:?} {b:?}");
        }
        // The actor's body has walked east: its mark with it, the place's stays.
        let walked = |id: u32| (id == 7).then_some([10.0, 100.0]);
        let c = marks(&near, 0.0, [0.0, 0.0], &walked);
        assert!((c[0].1 - offset(0.0, [0.0, 0.0], [10.0, 100.0])).abs() < 1e-4 && c[0].1 > 1.0);
        // An actor behind the player has none.
        let behind = [Near { mark: Mark::Actor, at: [0.0, -100.0], body: None }];
        assert!(marks(&behind, 0.0, [0.0, 0.0], &none).is_empty());
        assert_eq!(c[1].1, a[1].1);
        // The quest target behind the player is held at the band's left end (T12e).
        assert!(c.iter().any(|m| *m == (Mark::Quest, -COMPASS_VISIBLE)));
        // A mark's node keeps the fraction.
        assert_eq!(mark_left(Mark::Place, 0.25, false), COMPASS_HEADING + 0.25 - 2.0);
    }

    /// T12e: a quest target off the band is held at the end on its side; inside, at its
    /// bearing.
    #[test]
    fn a_quest_target_off_the_compass_is_held_at_its_end() {
        let none = |_: u32| None;
        let at = |to: [f32; 2]| marks(&[Near { mark: Mark::Quest, at: to, body: None }], 0.0, [0.0, 0.0], &none);
        // Due east (256 right) and due west: held at the right and the left end.
        assert_eq!(at([100.0, 0.0]), [(Mark::Quest, COMPASS_VISIBLE)]);
        assert_eq!(at([-100.0, 0.0]), [(Mark::Quest, -COMPASS_VISIBLE)]);
        // 30 degrees right: at its bearing.
        let thirty = [30f32.to_radians().sin(), 30f32.to_radians().cos()];
        assert!((at(thirty)[0].1 - 30.0 * 1024.0 / 360.0).abs() < 1e-2);
        assert_eq!(pinned(-140.0), -139.0);
        assert_eq!(pinned(12.5), 12.5);
    }

    #[test]
    fn due_north_of_a_player_facing_north_stands_at_148() {
        assert_eq!(COMPASS_HEADING + offset(0.0, [0.0, 0.0], [0.0, 100.0]), 148.0);
    }

    #[test]
    fn a_mark_shows_in_the_visible_band() {
        // The sweep showed the tick at ±45 degrees and hid it at ±50
        // [observed: hud.md §14.5].
        for bearing in [-45.0, 45.0] {
            assert!(on_compass(offset(-bearing, [0.0, 0.0], [0.0, 100.0])));
        }
        for bearing in [-50.0, 50.0] {
            assert!(!on_compass(offset(-bearing, [0.0, 0.0], [0.0, 100.0])));
        }
        for x in [-139.0, 139.0] {
            assert!(on_compass(x));
            assert!(!on_compass(x.signum() * 140.0));
        }
    }
}
