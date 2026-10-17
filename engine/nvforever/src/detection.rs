//! Detection in the running game (docs/design/detection.md): every living, drawn actor
//! (the loaded area) measures how well it perceives the player and the other actors within
//! its reach, a few times a second, with `fnv_game::detection`'s formula; so does the
//! player, from the camera. This module measures
//! what the formula needs from the world: distance, line of sight and field of view,
//! the light at the target, who is moving, running or sneaking, and the player's shots.
//!
//! The values go to the game (`Game::detection`), where GetDetected and
//! GetDetectionLevel read them; changes of band (hidden, noticed, detected) are logged,
//! and so is the player's highest band, the original's sneak meter (HIDDEN, CAUTION,
//! DANGER). Nothing acts on them yet: combat AI is the next step.
//!
//! What is measured here and how sure it is:
//! - **Who**: every two actors of the loaded area within detection reach of each other,
//!   near the player or not (`fnv_game::sight::pairs_within`) [observed: a Frenzied Chet
//!   6,682 units from the player fought Ringo within 1 s, capture detection-1 det2,
//!   2026-10-17]; the player measures every drawn actor, out of reach too (hidden there,
//!   but GetLineOfSight can read 1: 1 at 6,682 units, same capture). Actors out of the
//!   loaded area (the low process) do not detect anyone [observed: with the player in the
//!   gas station, a Frenzied Chet outdoors never fought Ringo in 155 s, same capture
//!   det3].
//! - **Line of sight**: a ray through the collision world from the detector's eyes to the
//!   target's chest; the world's solid shapes block it, actors do not [engineering
//!   choice]. Collision exists only around the player (3 × 3 cells): where either end is
//!   out of it, only the landscape blocks sight (`fnv_game::sight::over_land`, the
//!   heights read from the cells' LAND records) [engineering choice].
//! - **Field of view**: 95° either side of straight ahead, for every actor and for the
//!   player, whose view is the camera's (where it is, where it looks, first or third
//!   person) [documented: GECK wiki GetLineOfSight]; detection's "can see the target" is
//!   the same test [observed for the player: Ringo 263 units away read GetDetected and
//!   GetLineOfSight 1 at ±45° off the view and 0 at ±135°, third person, capture
//!   detection-1 det1].
//! - **The player as a detector**: measured like an actor, its Perception against the
//!   target's Sneak [observed: player.GetDetected Ringo and Chet 1 in the original,
//!   function probes fp1/fp2 2026-10-17, also with `tdetect` on, so the toggle and
//!   `NVF_PLAY_TEST_UNSEEN` leave it measuring; 0 for Ringo out of view and at 6,682
//!   units, capture detection-1; hypothesis: the same formula]. It feeds
//!   GetDetected / GetLineOfSight only: no detection state.
//! - **Light at the target** (0–100) [hypothesis, no source gives the game's measure]:
//!   100 × the luminance of the ambient light, plus the sunlight's times daylight outdoors
//!   or the cell's directional light indoors, plus each point light that reaches the
//!   target with the game's falloff 1 − (d/r)²; capped at 100. Shade from the sun is not
//!   measured. The player's flashlight (flashlight.rs) is not counted [design: it is not
//!   the game's; whether it should show the player is undecided].
//! - **Moving and running**: from how far an actor went since the last measurement
//!   [engineering choice]; the player runs when moving at the run gait (not holding Walk)
//!   [hypothesis: a sneaking run counts as running]. Other actors sneak while
//!   `Game::sneaks` says so (SetForceSneak, a package's Always Sneak; `sneaking.rs`).
//! - **Turning** (IsTurning): the heading changed by more than [`TURN_RATE`] per second
//!   since the last measurement, left or right [hypothesis: the game's threshold is not
//!   documented].
//! - **AI off** (SetActorsAI 0): the actor does not detect anyone [documented: GECK wiki
//!   SetActorsAI, "will not process … detection"]; it can still be detected.
//! - **Action sound**: an actor's (or the player's) loudest shot since the last
//!   measurement, at the
//!   weapon's sound level (WEAP VNAM: iSoundLevelLoud, Normal, Silent) [documented:
//!   GECK wiki Category:Detection].
//! - **Detector state**: asleep when lying on a bed; alert in a combat state, busy when
//!   in Combat with someone other than the target (`fnv_game::awareness`).
//!
//! **Detection states** (docs/design/alert.md): each measurement goes to
//! `Game::observe_threat`, which keeps those of actors the detector would attack; the
//! impacts of shots and CreateDetectionEvent are detection events, owned by the shooter,
//! measured like a target that makes the event's sound and cannot be seen
//! (`Game::hear_event`); an actor that is hit is alarmed about whoever hit it
//! (`Game::hurt_by`); then `Game::settle_awareness` changes states. Changes and
//! Detection topic lines are logged here (`alert:` lines); how alerted actors look is
//! `alert.rs`. [hypothesis: an impact sounds at the firing weapon's sound level; the
//! original's is not documented.]

use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::ragdoll::Corpse;
use crate::sky::SkyState;
use crate::world::{bevy_to_game, WorldData};
use bevy::prelude::*;
use fnv_game::awareness::{Change, DetectionEvent, EventType, State};
use fnv_game::detection::{Band, Settings, Situation};
use fnv_game::sight::in_view;
use fnv_phys::{layers, Filter, Owner};
use fnv_game::functions::senses::Motion;
use fnv_sim::FormId;
use nvf_play::combat::{ActorHit, Combat, ImpactFx, ShotFired};
use nvf_play::control::PlayerControl;
use nvf_play::phys::PhysState;
use nvf_play::player::{MoveIntent, Player, EYE_HEIGHT};
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use nvf_play::{Location, PlayerCamera};
use std::collections::{BTreeSet, HashMap};

/// Seconds between two measurements [engineering choice].
pub const INTERVAL: f32 = 0.25;
/// An actor turning faster than this (radians per second, about 30°/s) is turning for
/// IsTurning [hypothesis].
pub const TURN_RATE: f32 = 0.5;
/// Below this speed (units per second) an actor stands still [engineering choice].
const MOVING_SPEED: f32 = 10.0;
/// Above this speed an actor other than the player runs: halfway between the player's
/// measured walk and run (86.8 and 344 units per second, `MoveSpeeds`) [engineering choice].
const RUNNING_SPEED: f32 = 215.0;
/// The chest, as a fraction of the eye height [engineering choice].
const CHEST: f32 = 0.75;
/// A line of sight passes at most this many actors' capsules.
const PAST_ACTORS: usize = 8;

pub struct DetectionPlugin;

impl Plugin for DetectionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<DetectionState>()
            .add_systems(Update, measure.after(nvf_play::PlaySet::Scripts))
            .add_systems(Last, report);
    }
}

#[derive(Debug, Default)]
struct Stats {
    measurements: usize,
    pairs: usize,
    most_pairs: usize,
    /// Band changes of pairs with the player as the target, of pairs between actors, and
    /// of the player's own (the player as the detector), by the band changed to.
    player_changes: [usize; 3],
    actor_changes: [usize; 3],
    own_changes: [usize; 3],
    /// Pairs measured between actors both out of the player's reach; those detected.
    far_pairs: usize,
    far_detected: usize,
    /// Actors the player ever had detected.
    player_saw: BTreeSet<u32>,
    /// Actors that measured the player; those that had it noticed, and detected.
    watchers: BTreeSet<u32>,
    noticed_by: BTreeSet<u32>,
    detected_by: BTreeSet<u32>,
    /// Seconds the player spent in each band (the meter).
    meter_secs: [f32; 3],
    shots_heard: usize,
    /// The lowest and highest value any actor had the player at.
    range: Option<(f64, f64)>,
    /// State changes by (from, to); actors that were ever Alert; Detection topic lines
    /// said, and asked for without a line; detection events raised and actors alarmed
    /// by them; hits that alarmed.
    changes: std::collections::BTreeMap<(State, State), usize>,
    alerted: BTreeSet<u32>,
    barks: usize,
    barks_silent: usize,
    /// Detection lines held back by the group cooldown.
    barks_held: usize,
    events: usize,
    events_heard: usize,
    hurt_alarms: usize,
}

#[derive(Resource, Default)]
pub struct DetectionState {
    settings: Option<Settings>,
    next: f32,
    /// Where each actor was at the last measurement, and when (elapsed seconds).
    last: HashMap<u32, ([f32; 3], f32)>,
    /// Each actor's compass heading at the last measurement, and when.
    headings: HashMap<u32, (f32, f32)>,
    /// The loudest shot each shooter fired since the last measurement.
    shots: HashMap<u32, f64>,
    /// The sound level of each shooter's last shot: its impacts make that sound.
    shot_levels: HashMap<u32, f64>,
    /// Impacts of shots (where, whose), and actors hit (who, by whom), since the last
    /// measurement.
    impacts: Vec<([f32; 3], u32)>,
    hits: Vec<(u32, u32)>,
    /// The player's highest band at the last measurement (the meter).
    meter: Option<Band>,
    /// Landscape heights of the exterior cells, for sight out of the collision world.
    land: LandCache,
    stats: Stats,
}

/// The heights of each exterior cell's LAND record, read when first needed (worldspace,
/// grid square → heights; `None` for a square without land).
#[derive(Default)]
struct LandCache {
    cells: HashMap<(u32, i32, i32), Option<Vec<f32>>>,
}

impl LandCache {
    /// The land's height at game (x, y) in worldspace `ws`.
    fn height(&mut self, world: &WorldData, ws: u32, x: f32, y: f32) -> Option<f32> {
        let g = fnv_world::cell_of(x, y);
        let h = self.cells.entry((ws, g.0, g.1)).or_insert_with(|| {
            let w = world.index.worldspaces.get(&ws)?;
            let land = world.index.cells.get(w.grid.get(&g)?)?.land?;
            fnv_world::land::load_land(world.lo, world.schema, land, fnv_world::land::LandParts::HEIGHTS)?.heights
        });
        let (ox, oy) = (g.0 as f32 * fnv_world::CELL_SIZE, g.1 as f32 * fnv_world::CELL_SIZE);
        h.as_deref().map(|h| fnv_world::grass::height_at(h, x - ox, y - oy))
    }
}

/// An actor taking part in a measurement.
struct Person {
    id: u32,
    label: String,
    feet: [f32; 3],
    eyes: [f32; 3],
    /// Where it looks, horizontal unit vector (game space): the body's facing, or for the
    /// player the camera's.
    forward: [f32; 2],
    /// Where it sees from: the eyes, or for the player the camera.
    view: [f32; 3],
    moving: bool,
    running: bool,
    sneaking: bool,
    /// IsTurning's code: 0 not turning, 1 left, 2 right.
    turning: u8,
}

impl Person {
    fn chest(&self) -> [f32; 3] {
        let h = self.eyes[2] - self.feet[2];
        [self.feet[0], self.feet[1], self.feet[2] + h * CHEST]
    }
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn luminance(c: [f32; 3]) -> f32 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// The light level at a point, 0–100 (see the module notes).
fn light_at(sky: Option<&SkyState>, lights: &[([f32; 3], f32, f32)], at: [f32; 3]) -> f64 {
    let mut l = match sky {
        Some(s) => match &s.interior {
            Some(c) => {
                let u = |c: [u8; 3]| c.map(|v| v as f32 / 255.0);
                luminance(u(c.ambient)) + luminance(u(c.directional))
            }
            None => luminance(s.color(fnv_world::sky::ColorType::Ambient)) + luminance(s.color(fnv_world::sky::ColorType::Sunlight)) * s.daylight(),
        },
        None => 0.5,
    };
    for &(p, radius, lum) in lights {
        let d = distance(p, at);
        if d < radius {
            l += lum * (1.0 - (d / radius).powi(2));
        }
    }
    (l as f64 * 100.0).clamp(0.0, 100.0)
}

/// Layers sight passes through: the transparent layers, invisible walls and small loose
/// things [hypothesis: by the layers' names; a "Transparent Small" fence beside a Fiend's
/// path hid the player from it in a test, 2026-10-11].
fn see_through() -> layers::LayerSet {
    layers::LayerSet::from_layers(&[
        layers::TRANSPARENT,
        layers::TRANSPARENT_SMALL,
        layers::TRANSPARENT_SMALL_ANIM,
        layers::INVISIBLE_WALL,
        layers::CLUTTER,
        layers::DEBRIS_SMALL,
        layers::SHELL_CASING,
    ])
}

/// Test aid `NVF_PLAY_TEST_UNSEEN=1`: nobody perceives the player (its pairs and its
/// detection events are not measured), so fights between actors can be watched from
/// close by [design: as the original's console toggle for detection].
fn unseen() -> bool {
    static UNSEEN: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *UNSEEN.get_or_init(|| std::env::var("NVF_PLAY_TEST_UNSEEN").is_ok_and(|v| v == "1"))
}

/// Whether the collision world lets `from` see `to` (also the combat AI's cover test,
/// `combat_tactics`).
pub(crate) fn clear_sight(phys: Option<&mut PhysState>, from: [f32; 3], to: [f32; 3]) -> bool {
    let Some(phys) = phys else { return true };
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1.0 {
        return true;
    }
    let dir = d.map(|v| v / len);
    let filter = Filter { exclude_layers: layers::character_ignores().union(layers::actor_volumes()).union(see_through()), ..Default::default() };
    // Actors' capsules (the detector's own included: the ray starts inside it) do not
    // hide what is behind them: the ray goes on past them.
    let mut at = 0.0;
    for _ in 0..PAST_ACTORS {
        let start = [from[0] + dir[0] * at, from[1] + dir[1] * at, from[2] + dir[2] * at];
        match phys.ray(start, dir, len - at, filter.clone()) {
            None => return true,
            Some(h) if matches!(h.owner, Owner::Actor { .. } | Owner::Character { .. }) => at += h.distance + 1.0,
            Some(_) => return false,
        }
        if at >= len {
            return true;
        }
    }
    true
}

/// Whether `from` sees `to` for detection: through the collision world where both are
/// in it, else over the landscape alone (the module notes).
fn sight(phys: Option<&mut PhysState>, land: &mut LandCache, world: Option<&WorldData>, exterior: bool, from: [f32; 3], to: [f32; 3]) -> bool {
    let Some(phys) = phys else { return true };
    if phys.ready_at(from) && phys.ready_at(to) {
        return clear_sight(Some(phys), from, to);
    }
    let (Some(world), true) = (world, exterior) else { return true };
    let ws = phys.worldspace;
    fnv_game::sight::over_land(from, to, |x, y| land.height(world, ws, x, y))
}

/// A band change: detector and target (indices into the people), distance, the band
/// before, the detection, and how the target was in sight.
type BandChange = (usize, usize, f64, Option<Band>, fnv_game::detection::Detection, &'static str);

type Watched = (&'static ActorRig, &'static GlobalTransform, &'static InheritedVisibility, Has<Dying>, Has<Corpse>, Option<&'static Name>);

/// The light sources, the camera and the worldspace's records.
type Scene<'w, 's> = (
    Query<'w, 's, (&'static PointLight, &'static GlobalTransform, &'static InheritedVisibility)>,
    Query<'w, 's, (&'static SpotLight, &'static GlobalTransform, &'static InheritedVisibility), Without<crate::flashlight::PlayerFlashlight>>,
    Query<'w, 's, &'static GlobalTransform, With<PlayerCamera>>,
    Option<Res<'w, WorldData>>,
);

/// What the player is doing and where, the player's weapons and the sky.
type Inputs<'w> = (
    Option<Res<'w, Player>>,
    Option<Res<'w, MoveIntent>>,
    Option<Res<'w, PlayerControl>>,
    Option<Res<'w, Location>>,
    Option<Res<'w, Combat>>,
    Option<Res<'w, SkyState>>,
);

/// Measure every pair (the module notes).
#[allow(clippy::too_many_arguments)]
fn measure(
    time: Res<Time>,
    mut state: ResMut<DetectionState>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: Option<ResMut<SimState>>,
    mut phys: Option<ResMut<PhysState>>,
    (player, intent, control, location, combat, sky): Inputs,
    mut shots: MessageReader<ShotFired>,
    (mut impacts, mut hits): (MessageReader<ImpactFx>, MessageReader<ActorHit>),
    actors: Query<Watched>,
    (point_lights, spot_lights, camera, world): Scene,
) {
    let state = &mut *state;
    // Shots, heard at the next measurement.
    for s in shots.read() {
        let level = combat.as_deref().and_then(|c| c.forms.weapons.get(&s.weapon)).and_then(|w| w.sound_level.as_deref());
        let loud = level.map_or(0.0, |l| state.settings.unwrap_or_default().firing_sound(l));
        let heard = state.shots.entry(s.shooter.0).or_default();
        *heard = heard.max(loud);
        state.shot_levels.insert(s.shooter.0, loud);
    }
    for i in impacts.read() {
        state.impacts.push((i.point, i.shooter.0));
    }
    for h in hits.read() {
        if let (false, Some(by)) = (h.died, h.by) {
            state.hits.push((h.form_id, by.0));
        }
    }
    let now = time.elapsed_secs();
    if now < state.next {
        return;
    }
    state.next = now + INTERVAL;
    let (Some(player), Some(sim)) = (player, sim.as_deref_mut()) else { return };
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let settings = *state.settings.get_or_insert_with(|| {
        let s = Settings::read(&rt.game);
        info!(
            "detection: base {}, skill × {}, reach {} ({} outdoors), noticed above {}, detected above 0",
            s.base_value,
            s.skill_mult,
            s.max_distance(false),
            s.max_distance(true),
            s.noticed_min
        );
        s
    });
    let exterior = location.as_deref().is_none_or(|l| l.interior.is_none());
    let reach = settings.max_distance(exterior) as f32;

    // Who takes part: the player and the living actors of the loaded area (drawn).
    let motion = |id: u32, feet: [f32; 3], last: &mut HashMap<u32, ([f32; 3], f32)>| -> f32 {
        let speed = match last.get(&id) {
            Some(&(p, t)) if now > t => distance([p[0], p[1], 0.0], [feet[0], feet[1], 0.0]) / (now - t),
            _ => 0.0,
        };
        last.insert(id, (feet, now));
        speed
    };
    // Compass heading (0 north, rising clockwise): rising is turning right.
    let turning = |id: u32, heading: f32, headings: &mut HashMap<u32, (f32, f32)>| -> u8 {
        let code = match headings.get(&id) {
            Some(&(h, t)) if now > t => {
                let d = (heading - h + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
                let rate = d / (now - t);
                if rate.abs() <= TURN_RATE {
                    0
                } else if rate > 0.0 {
                    2
                } else {
                    1
                }
            }
            _ => 0,
        };
        headings.insert(id, (heading, now));
        code
    };
    let mut people = Vec::new();
    let player_run_mode = !intent.as_deref().is_some_and(|i| i.walk);
    let pspeed = motion(FormId::PLAYER_REF.0, player.feet, &mut state.last);
    let pmoving = pspeed > MOVING_SPEED;
    // The player sees with the camera, first or third person [documented: GECK wiki
    // GetLineOfSight]; without one, from the eyes along the heading.
    let (view, forward) = match camera.iter().next() {
        Some(c) => {
            let ahead = bevy_to_game(c.forward().as_vec3());
            (bevy_to_game(c.translation()).to_array(), Vec2::new(ahead.x, ahead.y).normalize_or_zero())
        }
        None => (player.eye(), Vec2::new(player.heading.sin(), player.heading.cos())),
    };
    people.push(Person {
        id: FormId::PLAYER_REF.0,
        label: "the player".into(),
        feet: player.feet,
        eyes: player.eye(),
        forward: [forward.x, forward.y],
        view,
        moving: pmoving,
        running: pmoving && !intent.as_deref().is_some_and(|i| i.walk),
        sneaking: control.as_deref().is_some_and(|c| c.sneak),
        turning: turning(FormId::PLAYER_REF.0, player.heading, &mut state.headings),
    });
    for (rig, g, vis, dying, corpse, name) in &actors {
        if !vis.get() || dying || corpse || rt.game.is_dead(FormId(rig.reference)) {
            continue;
        }
        let feet = bevy_to_game(g.translation()).to_array();
        let ahead = bevy_to_game(g.affine().transform_vector3(Vec3::Y));
        let flat = Vec2::new(ahead.x, ahead.y).normalize_or_zero();
        let scale = g.affine().transform_vector3(Vec3::Z).length().max(0.1);
        let speed = motion(rig.reference, feet, &mut state.last);
        // "Actor 0010634A EasyPete" → "EasyPete 0010634A".
        let edid = name.and_then(|n| n.as_str().split_whitespace().nth(2)).unwrap_or("");
        let label = format!("{edid} {:08X}", rig.reference).trim().to_string();
        let eyes = [feet[0], feet[1], feet[2] + EYE_HEIGHT * scale];
        people.push(Person {
            id: rig.reference,
            label,
            feet,
            eyes,
            forward: [flat.x, flat.y],
            view: eyes,
            moving: speed > MOVING_SPEED,
            running: speed > RUNNING_SPEED,
            sneaking: rt.game.sneaks(FormId(rig.reference)),
            turning: turning(rig.reference, flat.x.atan2(flat.y), &mut state.headings),
        });
    }
    state.last.retain(|_, (_, t)| now - *t < 5.0);
    state.headings.retain(|_, (_, t)| now - *t < 5.0);

    // The light sources that are on: game-space centre, radius, luminance.
    let mut lights = Vec::new();
    for (l, g, v) in &point_lights {
        if v.get() {
            lights.push((bevy_to_game(g.translation()).to_array(), l.range, luminance(l.color.to_srgba().to_f32_array_no_alpha())));
        }
    }
    for (l, g, v) in &spot_lights {
        if v.get() {
            lights.push((bevy_to_game(g.translation()).to_array(), l.range, luminance(l.color.to_srgba().to_f32_array_no_alpha())));
        }
    }
    let light: Vec<f64> = people.iter().map(|p| light_at(sky.as_deref(), &lights, p.chest())).collect();

    // Every detector against every target within reach.
    let shots = std::mem::take(&mut state.shots);
    if shots.get(&FormId::PLAYER_REF.0).is_some_and(|s| *s > 0.0) {
        state.stats.shots_heard += 1;
    }
    let mut phys = phys.as_deref_mut();
    let mut changes: Vec<BandChange> = Vec::new();
    let mut player_bands: Vec<(usize, fnv_game::detection::Detection)> = Vec::new();
    let (mut pairs, mut far_pairs, mut far_detected) = (0, 0, 0);
    let land = &mut state.land;
    let world = world.as_deref();
    let impacts = std::mem::take(&mut state.impacts);
    let hits = std::mem::take(&mut state.hits);
    let shot_levels = state.shot_levels.clone();
    let (mut events, mut hurt) = (0, 0);
    let mut heard: Vec<(String, DetectionEvent, f64, f32)> = Vec::new();
    let (alert_changes, barks, held) = with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        // What the scripts read of movement and sneaking (IsMoving, IsRunning,
        // IsSneaking): the people measured now; the others are forgotten.
        g.senses.motion.retain(|k, _| people.iter().any(|p| p.id == k.0));
        for p in &people {
            // The player's speed mode is the Walk toggle, moving or not (IsRunning).
            let run_mode = if p.id == FormId::PLAYER_REF.0 { player_run_mode } else { p.running };
            g.set_motion(FormId(p.id), Motion { moving: p.moving, running: p.running, sneaking: p.sneaking, turning: p.turning, run_mode });
        }
        // Every two within reach of each other, both ways; the player detects too.
        let feet: Vec<[f32; 3]> = people.iter().map(|p| p.feet).collect();
        let near_player: Vec<bool> = feet.iter().map(|f| distance(*f, player.feet) <= reach).collect();
        let within = fnv_game::sight::pairs_within(&feet, reach);
        // The player sees every drawn actor, whatever the distance (out of reach it is
        // hidden): player.GetLineOfSight Ringo read 1 at 6,682 units [observed: capture
        // detection-1 det2, 2026-10-17].
        let beyond: Vec<(usize, usize)> = (1..people.len()).filter(|&j| !near_player[j]).map(|j| (0, j)).collect();
        let ordered = within.into_iter().flat_map(|(a, b)| [(a, b), (b, a)]).chain(beyond);
        for (i, j) in ordered {
            let (d, t) = (&people[i], &people[j]);
            let by_player = i == 0;
            if (j == 0 && (unseen() || g.toggles.detection_off)) || (!by_player && g.ai_off(FormId(d.id))) {
                continue;
            }
            let aware = g.awareness.get(FormId(d.id));
            let alert = aware.is_some_and(|a| a.state.in_combat());
            let fighting = aware.filter(|a| a.state == State::Combat).and_then(|a| a.combat_target()).map(|(t, _)| t);
            let dist = distance(d.feet, t.feet);
            let target = t.chest();
            let sit = Situation {
                distance: dist as f64,
                exterior,
                line_of_sight: sight(phys.as_deref_mut(), land, world, exterior, d.view, target),
                in_view: in_view(d.view, d.forward, target),
                light: light[j],
                moving: t.moving,
                running: t.running,
                sneaking: t.sneaking,
                action_sound: shots.get(&t.id).copied().unwrap_or(0.0),
                detector_alert: alert,
                detector_asleep: !by_player && g.asleep(FormId(d.id)),
                detector_busy: fighting.is_some_and(|f| f != FormId(t.id)),
            };
            let (det, before) = g.detect(&settings, FormId(d.id), FormId(t.id), &sit);
            // The player's own detection feeds the script functions only.
            if !by_player {
                g.observe_threat(FormId(d.id), FormId(t.id), &det, t.feet);
            }
            pairs += 1;
            if !near_player[i] && !near_player[j] {
                far_pairs += 1;
                far_detected += usize::from(det.band == Band::Detected);
            }
            if j == 0 {
                player_bands.push((i, det));
            }
            if before != Some(det.band) {
                let sight = match (sit.line_of_sight, sit.in_view) {
                    (true, true) => "in sight",
                    (false, _) => "out of sight (blocked)",
                    (true, false) => "out of sight (out of view)",
                };
                changes.push((i, j, dist as f64, before, det, sight));
            }
        }
        g.detection.forget_older_than(g.time - 1.0);

        // Detection events: the impacts of shots, CreateDetectionEvent.
        for (at, by) in impacts {
            let level = shot_levels.get(&by).copied().unwrap_or(0.0);
            g.queue_detection_event(DetectionEvent { owner: FormId(by), at, level, kind: EventType::Bullet });
        }
        for ev in g.take_detection_events() {
            if (unseen() || g.toggles.detection_off) && ev.owner == FormId::PLAYER_REF {
                continue;
            }
            events += 1;
            // Actors hear events; the player (first) does not take part.
            for d in people.iter().skip(1) {
                let dist = distance(d.feet, ev.at);
                if FormId(d.id) == ev.owner || dist > reach {
                    continue;
                }
                let sit = Situation {
                    distance: dist as f64,
                    exterior,
                    line_of_sight: sight(phys.as_deref_mut(), land, world, exterior, d.eyes, ev.at),
                    // An event is heard, not seen [hypothesis].
                    in_view: false,
                    sneaking: ev.owner == FormId::PLAYER_REF && people[0].sneaking,
                    action_sound: ev.level,
                    detector_alert: g.awareness.state(FormId(d.id)).in_combat(),
                    detector_asleep: g.asleep(FormId(d.id)),
                    ..Default::default()
                };
                let pair = g.detection_pair(FormId(d.id), ev.owner);
                let det = settings.evaluate(&pair, &sit);
                if g.hear_event(FormId(d.id), &ev, &det) {
                    heard.push((d.label.clone(), ev, det.value, dist));
                }
            }
        }
        for (id, by) in hits {
            // Hit by someone it would leave alone: it may turn on them (assault,
            // friendly fire; `fnv_game::provocation`).
            g.attacked_by(FormId(id), FormId(by));
            let Some(at) = people.iter().find(|p| p.id == by).map(|p| p.feet) else { continue };
            if g.hurt_by(FormId(id), FormId(by), at) {
                hurt += 1;
            }
        }
        // Allies that see one of their own fighting take up the fight (Assistance).
        g.call_allies();
        g.settle_awareness();
        // Who started fighting or noticed a hostile alarms its comrades near it
        // (`fnv_game::combat_group`); only those measured here: drawn (the loaded area).
        let near: std::collections::BTreeMap<FormId, [f32; 3]> = people.iter().map(|p| (FormId(p.id), p.feet)).collect();
        g.raise_alarms(&near);
        let held = std::mem::take(&mut g.awareness.held_back);
        for (topic, who, by) in &held {
            debug!("alert: {} does not say {topic}: {} said a line of that kind {:.0} s ago (group cooldown)", g.data.name(*who), g.data.name(*by), g.awareness.said.iter().rev().find(|s| s.speaker == *by).map_or(0.0, |s| g.time - s.time));
        }
        (g.awareness.take_changes(), std::mem::take(&mut g.awareness.barks), held.len())
    });

    // The log and the counts.
    let st = &mut state.stats;
    st.measurements += 1;
    st.barks_held += held;
    st.pairs += pairs;
    st.far_pairs += far_pairs;
    st.far_detected += far_detected;
    st.most_pairs = st.most_pairs.max(pairs);
    for (i, j, dist, before, det, sight) in changes {
        let (d, t) = (&people[i], &people[j]);
        let line = format!(
            "detection: {} → {}: {} → {} (value {:.1}: sound {:.1}, visual {:.1}, skills {:.1} against {:.1}; {:.0} units, {}, light {:.0}{}{})",
            d.label,
            t.label,
            before.map_or("unmeasured", Band::name),
            det.band.name(),
            det.value,
            det.sound,
            det.visual,
            det.detector_skill,
            det.target_skill,
            dist,
            sight,
            light[j],
            if t.sneaking { ", sneaking" } else { "" },
            if t.running {
                ", running"
            } else if t.moving {
                ", moving"
            } else {
                ""
            },
        );
        let k = det.band as usize;
        if j == 0 {
            st.player_changes[k] += 1;
            info!("{line}");
        } else if i == 0 {
            st.own_changes[k] += 1;
            if det.band == Band::Detected {
                st.player_saw.insert(t.id);
            }
            debug!("{line}");
        } else {
            st.actor_changes[k] += 1;
            debug!("{line}");
        }
    }
    log_alerts(st, &people, &alert_changes, &barks, &heard, events, hurt);
    for (i, det) in &player_bands {
        let id = people[*i].id;
        st.watchers.insert(id);
        match det.band {
            Band::Noticed => {
                st.noticed_by.insert(id);
            }
            Band::Detected => {
                st.detected_by.insert(id);
            }
            Band::Hidden => {}
        }
        st.range = Some(st.range.map_or((det.value, det.value), |(lo, hi)| (lo.min(det.value), hi.max(det.value))));
    }
    let top = player_bands.iter().max_by(|a, b| a.1.value.total_cmp(&b.1.value));
    let meter = top.map_or(Band::Hidden, |(_, d)| d.band);
    st.meter_secs[meter as usize] += INTERVAL;
    if state.meter != Some(meter) {
        match top {
            Some((i, d)) => info!("detection: the player is {} ({}: {:.1})", meter.meter(), people[*i].label, d.value),
            None => info!("detection: the player is {} (no actor within reach)", meter.meter()),
        }
        state.meter = Some(meter);
    }
}

type Bark = (&'static str, FormId, Option<FormId>);

/// The `alert:` log lines and their counts.
fn log_alerts(st: &mut Stats, people: &[Person], changes: &[Change], barks: &[Bark], heard: &[(String, DetectionEvent, f64, f32)], events: usize, hurt: usize) {
    let label = |id: FormId| people.iter().find(|p| p.id == id.0).map_or_else(|| format!("{:08X}", id.0), |p| p.label.clone());
    st.events += events;
    st.events_heard += heard.len();
    st.hurt_alarms += hurt;
    for (who, ev, value, dist) in heard {
        info!("alert: {who} heard a {} event of {} (level {:.0}, value {value:.1}, {dist:.0} units away)", ev.kind.name(), label(ev.owner), ev.level);
    }
    for c in changes {
        *st.changes.entry((c.from, c.to)).or_default() += 1;
        if c.to == State::Alert {
            st.alerted.insert(c.actor.0);
        }
        let what = match (c.about, c.cause) {
            (Some(o), Some(cause)) => format!(": {} {}", cause.describe(), label(o)),
            (Some(o), None) => format!(": detects {}", label(o)),
            _ => String::new(),
        };
        let me = people.iter().find(|p| p.id == c.actor.0).map(|p| p.feet);
        let away = match (me, c.at) {
            (Some(f), Some(at)) => format!(", {:.0} units away", distance(f, at)),
            _ => String::new(),
        };
        let value = c.value.map_or(String::new(), |v| format!(", value {v:.1}"));
        info!("alert: {} {} → {}{what}{value}{away}", label(c.actor), c.from.name(), c.to.name());
    }
    for (topic, who, line) in barks {
        match line {
            Some(l) => {
                st.barks += 1;
                info!("alert: {} says {topic} line {:08X}", label(*who), l.0);
            }
            None => {
                st.barks_silent += 1;
                debug!("alert: {} has no {topic} line", label(*who));
            }
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, state: Res<DetectionState>) {
    if exit.read().next().is_none() {
        return;
    }
    let s = &state.stats;
    let bands = |c: [usize; 3]| format!("{} to hidden, {} to noticed, {} to detected", c[0], c[1], c[2]);
    info!(
        "detection summary: {} measurements, {} pairs ({} at most at once), {} actors measured the player, {} had it noticed, {} detected; \
         the player's changes: {}; between actors: {}; the player was hidden {:.1} s, CAUTION {:.1} s, DANGER {:.1} s; {} shots heard; values {}",
        s.measurements,
        s.pairs,
        s.most_pairs,
        s.watchers.len(),
        s.noticed_by.len(),
        s.detected_by.len(),
        bands(s.player_changes),
        bands(s.actor_changes),
        s.meter_secs[0],
        s.meter_secs[1],
        s.meter_secs[2],
        s.shots_heard,
        s.range.map_or("none".into(), |(lo, hi)| format!("{lo:.1} to {hi:.1}")),
    );
    info!(
        "detection summary, out of the player's reach: {} pairs, {} measured detected; the player as a detector: {}; {} actors detected by the player",
        s.far_pairs,
        s.far_detected,
        bands(s.own_changes),
        s.player_saw.len(),
    );
    let changes: Vec<String> = s.changes.iter().map(|((f, t), n)| format!("{} → {} {n}", f.name(), t.name())).collect();
    info!(
        "alert summary: {} actors became Alert; changes: {}; {} detection events, heard {} times; {} hits alarmed; {} Detection topic lines said, {} asked for without a line, {} held back by the group cooldown",
        s.alerted.len(),
        if changes.is_empty() { "none".into() } else { changes.join(", ") },
        s.events,
        s.events_heard,
        s.hurt_alarms,
        s.barks,
        s.barks_silent,
        s.barks_held,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn light_adds_point_lights_with_the_game_falloff_and_caps_at_100() {
        let lamp = ([0.0, 0.0, 0.0], 200.0, 1.0);
        assert!((light_at(None, &[], [0.0; 3]) - 50.0).abs() < 1e-6);
        // Half the radius away: 1 − 0.25 of the lamp's luminance.
        assert!((light_at(None, &[lamp], [100.0, 0.0, 0.0]) - 100.0).abs() < 1e-6);
        let dim = ([0.0, 0.0, 0.0], 200.0, 0.2);
        assert!((light_at(None, &[dim], [100.0, 0.0, 0.0]) - 65.0).abs() < 1e-4);
        assert!((light_at(None, &[dim], [300.0, 0.0, 0.0]) - 50.0).abs() < 1e-6);
    }
}
