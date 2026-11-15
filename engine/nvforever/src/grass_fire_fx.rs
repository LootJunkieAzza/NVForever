//! Grass fires on screen (`nvf_play::grass_fire`, docs/design/grass-fire.md): the burn
//! map copied to the grass materials' texture when it changes (`crate::grass::GrassBurn`),
//! the cells that unloaded forgotten so their grass grows back whole, the weather's wind
//! handed to the fires, and the flames, smoke and light of each burning patch.
//!
//! Each patch that catches gets [design, owner 2026-11-14: "a bit sparse", "starts small
//! and gets larger"]:
//!
//! - two small fires (`mps\mpsfiresmall01.nif`, the particle fire effects add on as an
//!   add-on node) at random spots in it, and on about a third of patches a larger one
//!   (`mps\mpsfiremed01.nif`);
//! - each starts at [`START_SIZE`] of its size and grows to it over [`GROW_SECS`] (its
//!   particles' size, `ModelEmitter::size`, and its spray, the holder's scale), and dies
//!   down over [`SHRINK_SECS`] when the patch burns out;
//! - on about one patch in six, an orange point light that flickers and grows and dies
//!   with the flames, at most [`MAX_LIGHTS`];
//! - when it burns out, on half the patches, smoke (`mps\mpssmokesmall01.nif`) that
//!   smoulders for [`SMOULDER_SECS`].
//!
//! GPU safety caps [design, gpu-device-lost 2026-11-14, after a power-off from stacked
//! fire filling the screen close up: fill rate near the camera matters most]:
//!
//! - at most [`MAX_EFFECTS`] flame and smoke models at once; past that a patch gets none
//!   (the grass still glows and chars);
//! - at most [`MAX_NEAR`] of them within [`NEAR`] units of the camera, every stacked model
//!   counted; past that the nearest are taken out, not the farthest;
//! - at most [`MAX_LIGHTS`] lights;
//! - a ramp: at most [`RAMP_PER_SEC`] new models a second, so a big burn cannot go from
//!   nothing to full in one frame. Patches waiting for the ramp queue (at most
//!   [`MAX_WAITING`], the oldest dropped) and get their flames when it allows, if they
//!   still burn.
//!
//! Log: `grass fire fx: …` once per model when it is loaded (or not); while any fire
//! effect lives, `grass fire fx: … drawn, … near …` every [`LOG_EVERY`] s; at exit
//! `grass fire fx summary: …`.

use crate::grass::GrassBurn;
use crate::particles::ModelEmitter;
use crate::world::{Streamer, WorldRoot};
use bevy::prelude::*;
use nvf_assets::model::{ModelCtx, PreparedModel};
use nvf_play::grass_fire::{centre, FireEvent, GrassFires, Key};
use std::collections::HashMap;
use std::sync::Arc;

pub const SMALL_FIRE: &str = r"mps\mpsfiresmall01.nif";
pub const MEDIUM_FIRE: &str = r"mps\mpsfiremed01.nif";
pub const SMOKE: &str = r"mps\mpssmokesmall01.nif";
/// The most flame and smoke models at once [design: frame cost].
pub const MAX_EFFECTS: usize = 200;
/// The most flame and smoke models within [`NEAR`] of the camera [design: fill rate].
pub const MAX_NEAR: usize = 24;
/// What counts as near the camera, in game units [design].
pub const NEAR: f32 = 400.0;
/// The most fire lights at once [design: frame cost].
pub const MAX_LIGHTS: usize = 8;
/// New flame and smoke models a second, at most [design: no sudden jump in GPU load].
pub const RAMP_PER_SEC: f32 = 20.0;
/// Burning patches waiting for the ramp, at most [chosen].
pub const MAX_WAITING: usize = 256;
/// Seconds between the running counts in the log [chosen].
pub const LOG_EVERY: f32 = 10.0;
// The GPU-safe caps (gpu-device-lost, 2026-11-14): raising one past these fails the build.
const _: () = assert!(MAX_EFFECTS <= 200 && MAX_NEAR <= 24 && MAX_LIGHTS <= 8 && RAMP_PER_SEC <= 20.0);
/// A flame's size when its patch catches, as a share of its full size [chosen].
pub const START_SIZE: f32 = 0.15;
/// Seconds a flame takes to grow to its full size [chosen].
pub const GROW_SECS: f32 = 2.0;
/// Seconds a flame takes to die down once its patch burnt out [chosen].
pub const SHRINK_SECS: f32 = 1.2;
/// Seconds smoke rises from a burnt-out patch [chosen].
pub const SMOULDER_SECS: f32 = 5.0;
/// A fire light's brightness (a LIGH fade) and reach [chosen].
const LIGHT_FADE: f32 = 1.4;
const LIGHT_RANGE: f32 = 420.0;

#[derive(Resource, Default)]
struct FlameFx {
    /// Each burning patch's ground height and its flames (and light).
    patches: HashMap<Key, (f32, Vec<Entity>)>,
    /// Models by path, once tried (`None` inside: it did not load).
    models: HashMap<&'static str, Option<Arc<PreparedModel>>>,
    /// Flame and smoke models alive, lights alive.
    effects: usize,
    lights: usize,
    spawned: u32,
    smoke: u32,
    lit: u32,
    /// Models not drawn: past [`MAX_EFFECTS`], waiting patches dropped or burnt out
    /// before the ramp reached them, and the near ones taken out.
    skipped: u32,
    waited_out: u32,
    near_removed: u32,
    /// Patches waiting for the ramp, oldest first, with their ground height.
    waiting: std::collections::VecDeque<(Key, f32)>,
    /// New models the ramp allows now (refills at [`RAMP_PER_SEC`]).
    budget: f32,
    /// The near count last frame, and seconds since the last log line.
    near: usize,
    since_log: f32,
    uploads: u32,
    cells_forgotten: u32,
}

/// A flame: it grows, burns, and dies down once `out` is set.
#[derive(Component)]
struct Flame {
    age: f32,
    /// Seconds since its patch burnt out.
    out: Option<f32>,
    /// Its full particle size and spray scale.
    size: f32,
    scale: f32,
}

/// Smoke over a burnt-out patch.
#[derive(Component)]
struct Smoulder {
    age: f32,
    size: f32,
}

/// A fire light: it grows and dies with its patch's flames, and flickers.
#[derive(Component)]
struct FireLight {
    age: f32,
    out: Option<f32>,
    full: f32,
    phase: f32,
}

pub struct GrassFireFxPlugin;

impl Plugin for GrassFireFxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<FlameFx>()
            .add_systems(Update, (sync, animate, near_cap).chain().after(nvf_play::PlaySet::Activation).after(crate::world::stream).run_if(resource_exists::<GrassFires>))
            .add_systems(Last, report);
    }
}

fn model(fx: &mut FlameFx, models: &mut ModelCtx, path: &'static str) -> Option<Arc<PreparedModel>> {
    fx.models
        .entry(path)
        .or_insert_with(|| {
            let full = fnv_actor::resolve::mesh_path(path);
            let pm = models.model(&full);
            match &pm {
                Some(pm) => info!("grass fire fx: model {full}: {} shapes, {} nodes", pm.shapes.len(), pm.nodes.len()),
                None => warn!("grass fire fx: model {full} not loaded; fires show without it"),
            }
            pm
        })
        .clone()
}

/// Spawn one of the effect models at `place`; `None` past the cap or without the model.
fn spawn(commands: &mut Commands, fx: &mut FlameFx, models: &mut ModelCtx, root: Entity, path: &'static str, place: Transform, size: f32) -> Option<Entity> {
    if fx.effects >= MAX_EFFECTS {
        fx.skipped += 1;
        return None;
    }
    let pm = model(fx, models, path)?;
    fx.budget -= 1.0;
    let full = fnv_actor::resolve::mesh_path(path);
    let (holder, _) = crate::shot_fx::spawn_playing_itself(commands, root, &pm, place, &full);
    let mut emitter = ModelEmitter::new(full);
    emitter.size = size;
    commands.entity(holder).insert(emitter);
    fx.effects += 1;
    Some(holder)
}

/// See the module notes.
#[allow(clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    mut gf: ResMut<GrassFires>,
    mut fx: ResMut<FlameFx>,
    streamer: Res<Streamer>,
    burn: Res<GrassBurn>,
    sky: Option<Res<crate::sky::SkyState>>,
    time: Res<Time>,
    mut models: ModelCtx,
    root: Query<Entity, With<WorldRoot>>,
    mut flames: Query<&mut Flame>,
    mut lights: Query<&mut FireLight>,
) {
    let fx = &mut *fx;
    let fires = &mut gf.fires;
    // The weather's wind: game (x, y) is Bevy (x, -z) (`crate::tree::wind_vector`).
    if let Some(sky) = sky.as_deref() {
        let w = crate::tree::wind_vector(sky);
        fires.wind = [w.x * w.w, -w.z * w.w];
    }
    // Cells no longer loaded (nor being spawned): forgotten, so they grow back.
    let gone: Vec<(i32, i32)> = fires.cells().filter(|c| streamer.interior.is_some() || !(streamer.loaded.contains_key(c) || streamer.spawning.contains_key(c))).collect();
    for c in gone {
        fires.forget_cell(c);
        fx.cells_forgotten += 1;
    }
    if fires.dirty {
        if let Some(mut image) = models.images.get_mut(&burn.0) {
            image.data = Some(fires.map.clone());
            fires.dirty = false;
            fx.uploads += 1;
        }
    }
    let events = std::mem::take(&mut fires.events);
    let Ok(world_root) = root.single() else { return };
    fx.budget = (fx.budget + RAMP_PER_SEC * time.delta_secs()).min(RAMP_PER_SEC);
    for e in events {
        match e {
            FireEvent::Lit { key, z } => {
                fx.waiting.push_back((key, z));
                if fx.waiting.len() > MAX_WAITING {
                    fx.waiting.pop_front();
                    fx.waited_out += 1;
                }
            }
            FireEvent::Out { key } => {
                if let Some(i) = fx.waiting.iter().position(|w| w.0 == key) {
                    // Burnt out before the ramp reached it: no flames, no smoke.
                    fx.waiting.remove(i);
                    fx.waited_out += 1;
                    continue;
                }
                let Some((z, made)) = fx.patches.remove(&key) else { continue };
                for e in made {
                    if let Ok(mut f) = flames.get_mut(e) {
                        f.out.get_or_insert(0.0);
                    } else if let Ok(mut l) = lights.get_mut(e) {
                        l.out.get_or_insert(0.0);
                    }
                }
                // Smoke on half the burnt-out patches, as the ramp allows.
                let r = hash(key);
                if (r >> 16).is_multiple_of(2) && fx.budget >= 1.0 {
                    let c = centre(key);
                    let place = Transform::from_translation(Vec3::new(c[0], c[1], z));
                    if let Some(h) = spawn(&mut commands, fx, &mut models, world_root, SMOKE, place, 1.0) {
                        commands.entity(h).insert(Smoulder { age: 0.0, size: 1.0 });
                        fx.smoke += 1;
                    }
                }
            }
            _ => {}
        }
    }
    // Waiting patches get their flames as the ramp allows (up to three models each).
    while fx.budget >= 3.0 {
        let Some((key, z)) = fx.waiting.pop_front() else { break };
        let c = centre(key);
        let r = hash(key);
        let bits = |shift: u32, n: u32| ((r >> shift) & ((1 << n) - 1)) as f32 / ((1u32 << n) - 1) as f32;
        let mut made = Vec::new();
        // Two small fires at random spots in the patch, sized at random [chosen].
        for (i, shift) in [(0u32, 0u32), (1, 12)] {
            let at = Vec3::new(c[0] + (bits(shift, 4) - 0.5) * 48.0, c[1] + (bits(shift + 4, 4) - 0.5) * 48.0, z);
            let size = 0.9 + 0.5 * bits(shift + 8, 4);
            let place = Transform::from_translation(at).with_rotation(Quat::from_rotation_z(i as f32 * 2.1 + bits(24, 4) * std::f32::consts::TAU)).with_scale(Vec3::splat(START_SIZE));
            if let Some(h) = spawn(&mut commands, fx, &mut models, world_root, SMALL_FIRE, place, size * START_SIZE) {
                commands.entity(h).insert(Flame { age: 0.0, out: None, size, scale: 1.0 });
                made.push(h);
            }
        }
        // A larger fire on about a third of the patches.
        if r.is_multiple_of(3) {
            let size = 0.8 + 0.3 * bits(28, 4);
            let place = Transform::from_translation(Vec3::new(c[0], c[1], z)).with_scale(Vec3::splat(START_SIZE));
            if let Some(h) = spawn(&mut commands, fx, &mut models, world_root, MEDIUM_FIRE, place, size * START_SIZE) {
                commands.entity(h).insert(Flame { age: 0.0, out: None, size, scale: 1.0 });
                made.push(h);
            }
        }
        // A light on about one patch in six, above the flames.
        if (r >> 20).is_multiple_of(6) && fx.lights < MAX_LIGHTS {
            let full = crate::lights::lumens(LIGHT_FADE, crate::sky::light_unit());
            let light = commands
                .spawn((
                    PointLight { color: crate::lights::display_color([1.0, 0.45, 0.12]), intensity: 0.0, range: LIGHT_RANGE, shadow_maps_enabled: false, ..default() },
                    FireLight { age: 0.0, out: None, full, phase: bits(4, 8) * std::f32::consts::TAU },
                    Transform::from_translation(Vec3::new(c[0], c[1], z + 48.0)),
                    ChildOf(world_root),
                ))
                .id();
            fx.lights += 1;
            fx.lit += 1;
            made.push(light);
        }
        fx.spawned += made.len() as u32;
        fx.patches.insert(key, (z, made));
    }
}

/// Flames grow, burn and die down; smoke rises and thins; lights flicker.
#[allow(clippy::type_complexity)]
fn animate(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FlameFx>,
    mut flames: Query<(Entity, &mut Flame, &mut Transform, &mut ModelEmitter), (Without<Smoulder>, Without<FireLight>)>,
    mut smoke: Query<(Entity, &mut Smoulder, &mut ModelEmitter), Without<Flame>>,
    mut lights: Query<(Entity, &mut FireLight, &mut PointLight)>,
) {
    let dt = time.delta_secs();
    let now = time.elapsed_secs();
    for (e, mut f, mut t, mut em) in &mut flames {
        f.age += dt;
        let out = f.out.map(|o| o + dt);
        f.out = out;
        let share = match out {
            Some(out) if out >= SHRINK_SECS => {
                commands.entity(e).try_despawn();
                fx.effects = fx.effects.saturating_sub(1);
                continue;
            }
            // From the size it had when its patch burnt out.
            Some(out) => grown(f.age - out) * (1.0 - out / SHRINK_SECS),
            None => grown(f.age),
        };
        em.size = f.size * share;
        t.scale = Vec3::splat(f.scale * (0.35 + 0.65 * share));
    }
    for (e, mut s, mut em) in &mut smoke {
        s.age += dt;
        if s.age >= SMOULDER_SECS {
            commands.entity(e).try_despawn();
            fx.effects = fx.effects.saturating_sub(1);
            continue;
        }
        // Thinning over its last 1.5 s.
        em.size = s.size * ((SMOULDER_SECS - s.age) / 1.5).clamp(0.0, 1.0);
    }
    for (e, mut l, mut light) in &mut lights {
        l.age += dt;
        let share = match l.out.as_mut() {
            Some(out) => {
                *out += dt;
                if *out >= SHRINK_SECS {
                    commands.entity(e).try_despawn();
                    fx.lights = fx.lights.saturating_sub(1);
                    continue;
                }
                1.0 - *out / SHRINK_SECS
            }
            None => grown(l.age),
        };
        let flicker = 0.8 + 0.12 * (now * 11.0 + l.phase).sin() + 0.08 * (now * 23.0 + 2.0 * l.phase).sin();
        light.intensity = l.full * share * flicker;
    }
}

/// At most [`MAX_NEAR`] flame and smoke models within [`NEAR`] of the camera: past that
/// the nearest are taken out (they cost the most fill rate). Also the running log line.
#[allow(clippy::type_complexity)]
fn near_cap(
    mut commands: Commands,
    time: Res<Time>,
    mut fx: ResMut<FlameFx>,
    camera: Query<&GlobalTransform, With<nvf_play::PlayerCamera>>,
    effects: Query<(Entity, &GlobalTransform), Or<(With<Flame>, With<Smoulder>)>>,
) {
    let Some(eye) = camera.iter().next().map(GlobalTransform::translation) else { return };
    let mut near: Vec<(f32, Entity)> = effects.iter().map(|(e, t)| (t.translation().distance_squared(eye), e)).filter(|(d, _)| *d <= NEAR * NEAR).collect();
    let over = take_out(&mut near);
    for &(_, e) in &near[..over] {
        commands.entity(e).try_despawn();
    }
    fx.effects = fx.effects.saturating_sub(over);
    fx.near_removed += over as u32;
    fx.near = near.len() - over;
    fx.since_log += time.delta_secs();
    if fx.since_log >= LOG_EVERY && (fx.effects > 0 || fx.lights > 0 || !fx.waiting.is_empty()) {
        fx.since_log = 0.0;
        info!(
            "grass fire fx: {} drawn (cap {MAX_EFFECTS}), {} near (cap {MAX_NEAR} within {NEAR}), {} lights (cap {MAX_LIGHTS}), {} patches waiting for the ramp; not drawn so far: {} past the cap, {} waited out, {} near taken out",
            fx.effects, fx.near, fx.lights, fx.waiting.len(), fx.skipped, fx.waited_out, fx.near_removed
        );
    }
}

/// Sort the near effects nearest first and return how many of the nearest to take out so
/// that [`MAX_NEAR`] remain.
fn take_out<T: Copy>(near: &mut [(f32, T)]) -> usize {
    near.sort_by(|a, b| a.0.total_cmp(&b.0));
    near.len().saturating_sub(MAX_NEAR)
}

/// How far grown a flame is `age` seconds after it caught: [`START_SIZE`] to 1 over
/// [`GROW_SECS`], fast at first [chosen].
fn grown(age: f32) -> f32 {
    let k = (age / GROW_SECS).clamp(0.0, 1.0);
    let ease = 1.0 - (1.0 - k) * (1.0 - k);
    START_SIZE + (1.0 - START_SIZE) * ease
}

/// A patch's fixed random bits.
fn hash(k: Key) -> u32 {
    let mut h = (k.0 as u32).wrapping_mul(0x9E37_79B1) ^ (k.1 as u32).wrapping_mul(0x85EB_CA77);
    h ^= h >> 15;
    h = h.wrapping_mul(0xC2B2_AE3D);
    h ^ (h >> 13)
}

fn report(mut exit: MessageReader<AppExit>, fx: Res<FlameFx>) {
    if exit.read().next().is_some() {
        info!(
            "grass fire fx summary: {} flames drawn ({} past the cap of {MAX_EFFECTS} not drawn, {} patches waited out the ramp, {} near taken out), {} smoke, {} lights, {} burn map uploads, {} cells forgotten (their grass grows back)",
            fx.spawned, fx.skipped, fx.waited_out, fx.near_removed, fx.smoke, fx.lit, fx.uploads, fx.cells_forgotten
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flames_start_small_and_grow() {
        assert_eq!(grown(0.0), START_SIZE);
        assert!(grown(GROW_SECS * 0.5) > 0.6);
        assert_eq!(grown(GROW_SECS), 1.0);
        assert_eq!(grown(100.0), 1.0);
    }

    #[test]
    fn the_nearest_go_when_too_many_are_near() {
        // 30 stacked at various distances: the 6 nearest go, the 24 farthest stay.
        let mut near: Vec<(f32, u32)> = (0..30u32).rev().map(|i| (i as f32 * 10.0, i)).collect();
        let over = take_out(&mut near);
        assert_eq!(over, 6);
        let gone: Vec<u32> = near[..over].iter().map(|n| n.1).collect();
        assert_eq!(gone, vec![0, 1, 2, 3, 4, 5]);
        let mut few: Vec<(f32, u32)> = (0..MAX_NEAR as u32).map(|i| (i as f32, i)).collect();
        assert_eq!(take_out(&mut few), 0);
    }
}
