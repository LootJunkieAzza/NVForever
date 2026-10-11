//! Detection in the running game (docs/design/detection.md): every living, drawn actor
//! near the player measures how well it perceives the player and the other actors near
//! it, a few times a second, with `fnv_game::detection`'s formula. This module measures
//! what the formula needs from the world: distance, line of sight and field of view,
//! the light at the target, who is moving, running or sneaking, and the player's shots.
//!
//! The values go to the game (`Game::detection`), where GetDetected and
//! GetDetectionLevel read them; changes of band (hidden, noticed, detected) are logged,
//! and so is the player's highest band, the original's sneak meter (HIDDEN, CAUTION,
//! DANGER). Nothing acts on them yet: combat AI is the next step.
//!
//! What is measured here and how sure it is:
//! - **Line of sight**: a ray through the collision world from the detector's eyes to the
//!   target's chest; the world's solid shapes block it, actors do not [engineering
//!   choice]. Collision exists only around the player, so between actors far from the
//!   player nothing blocks sight [engineering choice].
//! - **Field of view**: about 95° either side of straight ahead for every non-player
//!   actor [documented: catalogue notes, GetLineOfSight]; [hypothesis: detection's
//!   "can see the target" is the same test].
//! - **Light at the target** (0–100) [hypothesis, no source gives the game's measure]:
//!   100 × the luminance of the ambient light, plus the sunlight's times daylight outdoors
//!   or the cell's directional light indoors, plus each point light that reaches the
//!   target with the game's falloff 1 − (d/r)²; capped at 100. Shade from the sun is not
//!   measured. The player's flashlight (flashlight.rs) is not counted [design: it is not
//!   the game's; whether it should show the player is undecided].
//! - **Moving and running**: from how far an actor went since the last measurement
//!   [engineering choice]; the player runs when moving at the run gait (not holding Walk)
//!   [hypothesis: a sneaking run counts as running]. Actors never sneak (packages' Always
//!   Sneak is not acted on yet).
//! - **Action sound**: the player's last shot since the last measurement, at the
//!   weapon's sound level (WEAP VNAM: iSoundLevelLoud, Normal, Silent) [documented:
//!   GECK wiki Category:Detection].
//! - **Detector state**: asleep when lying on a bed; never alert or busy (no combat AI).

use crate::limbs::ActorRig;
use crate::locomotion::Dying;
use crate::ragdoll::Corpse;
use crate::sky::SkyState;
use crate::world::bevy_to_game;
use bevy::prelude::*;
use fnv_game::detection::{Band, Settings, Situation};
use fnv_phys::{layers, Filter, Owner};
use fnv_sim::FormId;
use nvf_play::combat::{Combat, ShotFired};
use nvf_play::control::PlayerControl;
use nvf_play::phys::PhysState;
use nvf_play::player::{MoveIntent, Player, EYE_HEIGHT};
use nvf_play::script::{with_world, ScriptState};
use nvf_play::sim::SimState;
use nvf_play::Location;
use std::collections::{BTreeSet, HashMap};

/// Seconds between two measurements [engineering choice].
pub const INTERVAL: f32 = 0.25;
/// Half the field of view, degrees [documented: catalogue notes, GetLineOfSight].
pub const HALF_FIELD_OF_VIEW: f32 = 95.0;
/// Below this speed (units per second) an actor stands still [engineering choice].
const MOVING_SPEED: f32 = 10.0;
/// Above this speed an actor other than the player runs: halfway between the player's
/// measured walk and run (86.8 and 344 units per second, `MoveSpeeds`) [engineering choice].
const RUNNING_SPEED: f32 = 215.0;
/// The chest, as a fraction of the eye height [engineering choice].
const CHEST: f32 = 0.75;

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
    /// Band changes of pairs with the player as the target, and of pairs between actors,
    /// by the band changed to.
    player_changes: [usize; 3],
    actor_changes: [usize; 3],
    /// Actors that measured the player; those that had it noticed, and detected.
    watchers: BTreeSet<u32>,
    noticed_by: BTreeSet<u32>,
    detected_by: BTreeSet<u32>,
    /// Seconds the player spent in each band (the meter).
    meter_secs: [f32; 3],
    shots_heard: usize,
    /// The lowest and highest value any actor had the player at.
    range: Option<(f64, f64)>,
}

#[derive(Resource, Default)]
pub struct DetectionState {
    settings: Option<Settings>,
    next: f32,
    /// Where each actor was at the last measurement, and when (elapsed seconds).
    last: HashMap<u32, ([f32; 3], f32)>,
    /// The loudest shot the player fired since the last measurement.
    shot: f64,
    /// The player's highest band at the last measurement (the meter).
    meter: Option<Band>,
    stats: Stats,
}

/// An actor taking part in a measurement.
struct Person {
    id: u32,
    label: String,
    feet: [f32; 3],
    eyes: [f32; 3],
    /// Where it looks, horizontal unit vector (game space); `None` for the player (only
    /// actors detect).
    forward: Option<[f32; 2]>,
    moving: bool,
    running: bool,
    sneaking: bool,
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

/// Whether `to` is within the field of view of an actor at `from` looking along
/// `forward` (horizontal angle).
pub fn in_view(from: [f32; 3], forward: [f32; 2], to: [f32; 3]) -> bool {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = (d[0] * d[0] + d[1] * d[1]).sqrt();
    if len < 1.0 {
        return true;
    }
    let cos = (d[0] * forward[0] + d[1] * forward[1]) / len;
    cos >= HALF_FIELD_OF_VIEW.to_radians().cos()
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

/// Whether the collision world lets `from` see `to`.
fn clear_sight(phys: Option<&mut PhysState>, from: [f32; 3], to: [f32; 3]) -> bool {
    let Some(phys) = phys else { return true };
    let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1.0 {
        return true;
    }
    let dir = d.map(|v| v / len);
    let filter = Filter { exclude_layers: layers::character_ignores().union(layers::actor_volumes()), ..Default::default() };
    match phys.ray(from, dir, len, filter) {
        None => true,
        // Other actors' capsules do not hide what is behind them.
        Some(h) => matches!(h.owner, Owner::Actor { .. } | Owner::Character { .. }),
    }
}

type Watched = (&'static ActorRig, &'static GlobalTransform, &'static InheritedVisibility, Has<Dying>, Has<Corpse>, Option<&'static Name>);

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
    actors: Query<Watched>,
    point_lights: Query<(&PointLight, &GlobalTransform, &InheritedVisibility)>,
    spot_lights: Query<(&SpotLight, &GlobalTransform, &InheritedVisibility), Without<crate::flashlight::PlayerFlashlight>>,
) {
    let state = &mut *state;
    // The player's shots, heard at the next measurement.
    for s in shots.read() {
        let level = combat.as_deref().and_then(|c| c.forms.weapons.get(&s.weapon)).and_then(|w| w.sound_level.as_deref());
        let loud = level.map_or(0.0, |l| state.settings.unwrap_or_default().firing_sound(l));
        state.shot = state.shot.max(loud);
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

    // Who takes part: the player and the living actors within reach of the player.
    let motion = |id: u32, feet: [f32; 3], last: &mut HashMap<u32, ([f32; 3], f32)>| -> f32 {
        let speed = match last.get(&id) {
            Some(&(p, t)) if now > t => distance([p[0], p[1], 0.0], [feet[0], feet[1], 0.0]) / (now - t),
            _ => 0.0,
        };
        last.insert(id, (feet, now));
        speed
    };
    let mut people = Vec::new();
    let pspeed = motion(FormId::PLAYER_REF.0, player.feet, &mut state.last);
    let pmoving = pspeed > MOVING_SPEED;
    people.push(Person {
        id: FormId::PLAYER_REF.0,
        label: "the player".into(),
        feet: player.feet,
        eyes: player.eye(),
        forward: None,
        moving: pmoving,
        running: pmoving && !intent.as_deref().is_some_and(|i| i.walk),
        sneaking: control.as_deref().is_some_and(|c| c.sneak),
    });
    for (rig, g, vis, dying, corpse, name) in &actors {
        if !vis.get() || dying || corpse || rt.game.is_dead(FormId(rig.reference)) {
            continue;
        }
        let feet = bevy_to_game(g.translation()).to_array();
        if distance(feet, player.feet) > reach {
            continue;
        }
        let ahead = bevy_to_game(g.affine().transform_vector3(Vec3::Y));
        let flat = Vec2::new(ahead.x, ahead.y).normalize_or_zero();
        let scale = g.affine().transform_vector3(Vec3::Z).length().max(0.1);
        let speed = motion(rig.reference, feet, &mut state.last);
        // "Actor 0010634A EasyPete" → "EasyPete 0010634A".
        let edid = name.and_then(|n| n.as_str().split_whitespace().nth(2)).unwrap_or("");
        let label = format!("{edid} {:08X}", rig.reference).trim().to_string();
        people.push(Person {
            id: rig.reference,
            label,
            feet,
            eyes: [feet[0], feet[1], feet[2] + EYE_HEIGHT * scale],
            forward: Some([flat.x, flat.y]),
            moving: speed > MOVING_SPEED,
            running: speed > RUNNING_SPEED,
            sneaking: false,
        });
    }
    state.last.retain(|_, (_, t)| now - *t < 5.0);

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
    let shot = std::mem::take(&mut state.shot);
    if shot > 0.0 {
        state.stats.shots_heard += 1;
    }
    let mut phys = phys.as_deref_mut();
    let mut changes: Vec<(usize, usize, f64, Option<Band>, fnv_game::detection::Detection)> = Vec::new();
    let mut player_bands: Vec<(usize, fnv_game::detection::Detection)> = Vec::new();
    let mut pairs = 0;
    with_world(rt, sim, |rt| {
        let g = &mut rt.game;
        for (i, d) in people.iter().enumerate() {
            let Some(forward) = d.forward else { continue };
            let asleep = g.asleep(FormId(d.id));
            for (j, t) in people.iter().enumerate() {
                if i == j {
                    continue;
                }
                let dist = distance(d.feet, t.feet);
                if dist > reach {
                    continue;
                }
                let target = t.chest();
                let sit = Situation {
                    distance: dist as f64,
                    exterior,
                    line_of_sight: clear_sight(phys.as_deref_mut(), d.eyes, target),
                    in_view: in_view(d.eyes, forward, target),
                    light: light[j],
                    moving: t.moving,
                    running: t.running,
                    sneaking: t.sneaking,
                    action_sound: if j == 0 { shot } else { 0.0 },
                    detector_alert: false,
                    detector_asleep: asleep,
                    detector_busy: false,
                };
                let (det, before) = g.detect(&settings, FormId(d.id), FormId(t.id), &sit);
                pairs += 1;
                if j == 0 {
                    player_bands.push((i, det));
                }
                if before != Some(det.band) {
                    changes.push((i, j, dist as f64, before, det));
                }
            }
        }
        g.detection.forget_older_than(g.time - 1.0);
    });

    // The log and the counts.
    let st = &mut state.stats;
    st.measurements += 1;
    st.pairs += pairs;
    st.most_pairs = st.most_pairs.max(pairs);
    for (i, j, dist, before, det) in changes {
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
            if det.seen { "in sight" } else { "out of sight" },
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
        } else {
            st.actor_changes[k] += 1;
            debug!("{line}");
        }
    }
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_field_of_view_is_95_degrees_either_side() {
        let at = [0.0, 0.0, 0.0];
        let north = [0.0, 1.0];
        assert!(in_view(at, north, [0.0, 100.0, 0.0]));
        assert!(in_view(at, north, [100.0, 0.0, 0.0]));
        // 94° and 96° off straight ahead.
        let off = |deg: f32| [100.0 * deg.to_radians().sin(), 100.0 * deg.to_radians().cos(), 0.0];
        assert!(in_view(at, north, off(94.0)));
        assert!(!in_view(at, north, off(96.0)));
        assert!(!in_view(at, north, [0.0, -100.0, 0.0]));
    }

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
