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
//! - The ranges are guesses: no game setting for them is in the masters [hypothesis].

use crate::hud::{CompassMarks, COMPASS, STRIP};
use crate::interface::{amber, at, bc_supported, texture};
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
/// The tick (a 32-texel picture whose bar is 18 texels from its left edge) drawn this
/// size, its top this far down the compass window, its bar's middle on the bearing.
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
    /// A map marker the player has not been to, and one it has.
    Place,
    Found,
}

/// Compass units right of the compass's middle at which a thing at `to` stands for a
/// player at `from` facing `heading` (degrees, 0 north, 90 east): the bearing's
/// difference, wrapped to −180…180, at the strip's 1024 units to 360 degrees (`crate::hud`).
pub fn offset(heading: f32, from: [f32; 2], to: [f32; 2]) -> f32 {
    let bearing = (to[0] - from[0]).atan2(to[1] - from[1]).to_degrees();
    let d = (bearing - heading + 540.0).rem_euclid(360.0) - 180.0;
    d * STRIP.x / 360.0
}

/// Whether a mark at `x` (from the compass's middle) is in the compass's window, with
/// its picture's room either side.
pub fn on_compass(x: f32) -> bool {
    x.abs() <= COMPASS.x / 2.0 + LANDMARK_SIZE
}

/// The marks drawn: kind and place across (whole units, so the drawing changes only when
/// a mark moves visibly).
#[derive(Resource, Default, Debug, PartialEq, Clone)]
pub struct CompassView(pub Vec<(Mark, i32)>);

pub struct CompassPlugin;

impl Plugin for CompassPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<CompassView>()
            .add_systems(Update, (discover, gather, draw).chain().in_set(PlaySet::Ui).after(crate::hud::HudSet));
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
pub(crate) fn discover(state: Option<ResMut<DataState>>, mut script: Option<ResMut<ScriptState>>, sim: Res<SimState>, messages: Option<ResMut<QuestMessages>>) {
    let Some(mut state) = state else { return };
    // Found before the game runs (the first frames): kept until it does, for the XP.
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
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
        info!("compass: discovered {name} ({}), +{gained} XP", FormId(form));
        if let (Some(m), false) = (messages.as_deref_mut(), name.trim().is_empty()) {
            m.push(QuestNotice::Discovered(name));
        }
    }
}

/// The NPCs and creatures of the loaded cells (and those a script moved in), with their
/// feet: a moving body's (`PhysState::npcs`) or the reference's place.
fn actors_near(g: &fnv_game::Game, bodies: &std::collections::HashMap<u32, [f32; 3]>) -> Vec<(FormId, [f32; 3])> {
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

/// Who and what is near, ten times a second.
#[allow(clippy::too_many_arguments)]
fn gather(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    location: Option<Res<Location>>,
    phys: Option<Res<PhysState>>,
    markers: Option<Res<DataState>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut poll: Local<f32>,
    mut view: ResMut<CompassView>,
    mut logged: Local<[usize; 4]>,
) {
    *poll += time.delta_secs();
    if *poll < POLL || *state.get() != PlayState::Playing {
        return;
    }
    *poll = 0.0;
    let (Some(player), Some(phys)) = (player, phys) else { return };
    let (feet, heading) = (player.feet, player.heading.to_degrees());
    let here = [feet[0], feet[1]];
    let bodies: std::collections::HashMap<u32, [f32; 3]> = phys.npcs.iter().map(|(&id, b)| (id, b.ch.feet)).collect();
    let mut marks: Vec<(Mark, i32)> = Vec::new();
    let mut perception = 5.0;
    if let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) {
        let found = with_world(rt, &mut sim, |rt| {
            let g = &mut rt.game;
            perception = fnv_game::functions::actor_value_capped(g, FormId::PLAYER_REF, fnv_game::actor_values::PERCEPTION) as f32;
            let range = perception.max(1.0) * ACTOR_RANGE_PER_PERCEPTION;
            let mut out = Vec::new();
            for (r, at) in actors_near(g, &bodies) {
                let d = ((at[0] - feet[0]).powi(2) + (at[1] - feet[1]).powi(2) + (at[2] - feet[2]).powi(2)).sqrt();
                if d > range || g.is_dead(r) || !g.present(r) {
                    continue;
                }
                let x = offset(heading, here, [at[0], at[1]]);
                if !on_compass(x) {
                    continue;
                }
                let fighting = g.awareness.get(r).and_then(|a| a.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
                let hostile = fighting || g.would_attack(r, FormId::PLAYER_REF);
                out.push((if hostile { Mark::Hostile } else { Mark::Actor }, x.round() as i32));
            }
            out
        });
        marks.extend(found);
    }
    if let (Some(loc), Some(st)) = (location.as_deref(), markers.as_deref()) {
        if loc.interior.is_none() {
            let range = perception.max(1.0) * MARKER_RANGE_PER_PERCEPTION;
            for (form, pos, found) in st.markers_of(loc.world) {
                if !sim.present(form) || (pos[0] - feet[0]).hypot(pos[1] - feet[1]) > range {
                    continue;
                }
                let x = offset(heading, here, [pos[0], pos[1]]);
                if on_compass(x) {
                    marks.push((if found { Mark::Found } else { Mark::Place }, x.round() as i32));
                }
            }
        }
    }
    marks.sort();
    let count = |k: &[Mark]| marks.iter().filter(|m| k.contains(&m.0)).count();
    let counts = [count(&[Mark::Actor, Mark::Hostile]), count(&[Mark::Hostile]), count(&[Mark::Place, Mark::Found]), count(&[Mark::Found])];
    if counts != *logged {
        let [actors, hostile, markers, found] = counts;
        info!("compass: {actors} actors ({hostile} hostile), {markers} map markers ({found} found) on the compass, Perception {perception}");
        *logged = counts;
    }
    view.set_if_neq(CompassView(marks));
}

struct Art {
    tick: Option<Handle<Image>>,
    landmark: Option<Handle<Image>>,
    found: Option<Handle<Image>>,
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    view: Res<CompassView>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    layers: Query<Entity, With<CompassMarks>>,
    added: Query<(), Added<CompassMarks>>,
    mut art: Local<Option<Art>>,
) {
    let Ok(layer) = layers.single() else { return };
    if !view.is_changed() && added.get(layer).is_err() {
        return;
    }
    let art = art.get_or_insert_with(|| {
        let bc = bc_supported(formats.as_deref());
        let mut read = |p: &str| data.as_deref().and_then(|d| texture(p, d, &mut images, bc)).map(|t| t.0);
        Art { tick: read(TICK), landmark: read(LANDMARK), found: read(LANDMARK_FOUND) }
    });
    let middle = COMPASS.x / 2.0;
    commands.entity(layer).despawn_related::<Children>().with_children(|p| {
        for &(mark, x) in &view.0 {
            let x = middle + x as f32;
            let (image, node, colour) = match mark {
                Mark::Actor | Mark::Hostile => {
                    let colour = if mark == Mark::Hostile { crime_colour() } else { amber(255.0) };
                    (&art.tick, at(x - TICK_MIDDLE * TICK_SIZE, TICK_TOP, TICK_SIZE, TICK_SIZE), colour)
                }
                Mark::Place | Mark::Found => {
                    let image = if mark == Mark::Found { &art.found } else { &art.landmark };
                    let corner = Vec2::new(x, LANDMARK_Y) - LANDMARK_MIDDLE * LANDMARK_SIZE;
                    (image, at(corner.x, corner.y, LANDMARK_SIZE, LANDMARK_SIZE), amber(255.0))
                }
            };
            match image {
                Some(image) => p.spawn((node, ImageNode { color: colour, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image.clone()) })),
                // Without the game's pictures: a small block.
                None => p.spawn((at(x - 2.0, if matches!(mark, Mark::Actor | Mark::Hostile) { 10.0 } else { 40.0 }, 4.0, 12.0), BackgroundColor(colour))),
            };
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mark_stands_at_its_bearing_from_the_heading() {
        // North of a player facing north: in the middle; east: a quarter strip right.
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
}
