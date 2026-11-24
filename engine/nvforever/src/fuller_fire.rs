//! Fuller fire [design]: more body and depth for the fires of the world, an NVForever
//! addition beyond the game's data (owner 2026-11-09, after item 103 lit the campfires:
//! "I still want more 3d fire in general").
//!
//! The world's fires (campfires, fire barrels, the fires Burning Campfire Redone places)
//! burn as the particles of their add-on nodes: ADDN 14 `MPS\MPSFireSmall01.NIF`
//! (`pFlames02` 15 a second living about 0.37 s, and `pSparks`) and ADDN 0
//! `MPS\MPSFireMed01.NIF` [observed: the records and the files' systems, logged by
//! `particles`]. About five flame squares per fire are alive at once, each turned to
//! the camera: from most places the fire reads as a flat sprite.
//!
//! An add-on node whose model is one of those fire files (its file name begins
//! `mpsfire`) burns a volumetric flame in place of its flame squares
//! (`fire_volume.rs`; a first try, the squares again denser, looked like a denser top,
//! not 3D: owner 2026-11-09), and also emits systems made here from the game's own
//! (their textures, blending, colours and modifiers):
//! - **Embers**: the sparks (else the first flame system) as small glowing points that
//!   live several times longer and rise well above the flames, drifting apart.
//! - **Smoke**: ADDN 35's `MPS\MPSSmokeSmall01.NIF` systems, fainter and shorter,
//!   starting above the flames: a wisp that gives the fire height in daylight.
//!
//! The file's own systems are untouched, and the added ones are named
//! `fuller fire: …` in the log. The `fuller-fire` switch of the settings page turns
//! them on (`--graphics-only fuller-fire`); off in both presets since 2026-11-14, when a lit
//! campfire's volumetric flames powered off the owner's PC (`fire_volume.rs` "Cost").

use bevy::math::Affine3A;
use bevy::prelude::Vec3;
use nvf_assets::particles::{EmitterParam, ParticleSystemDef};

/// The start of the names of the systems made here.
pub const PREFIX: &str = "fuller fire: ";

/// The smoke the fires gain (ADDN 35 `MPSSmokeSmall01`), as a mesh path.
pub const SMOKE: &str = "meshes\\mps\\mpssmokesmall01.nif";

/// Embers: size, life, births and speed against the file's sparks, and how much of
/// their pull up they keep (so they climb slower and higher, not shoot off).
const EMBER_SIZE: f32 = 0.08;
const EMBER_LIFE: f32 = 3.5;
const EMBER_RATE: f32 = 0.6;
const EMBER_MAX: f32 = 2.5;
const EMBER_SPEED: f32 = 0.6;
const EMBER_LIFT: f32 = 0.5;
/// The least spread of their cone (radians from up), so they drift apart.
const EMBER_SPREAD: f32 = 0.5;

/// Smoke: how far above the add-on node it starts (units), its births, life and
/// opacity against ADDN 35's.
const SMOKE_START: f32 = 48.0;
const SMOKE_RATE: f32 = 0.5;
const SMOKE_LIFE: f32 = 0.6;
const SMOKE_ALPHA: f32 = 0.5;

/// Whether the add-on model at `path` is a fire this gives more body.
pub fn is_fire(path: &str) -> bool {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase();
    name.starts_with("mpsfire")
}

/// Whether `d` is a system made here.
pub fn is_added(d: &ParticleSystemDef) -> bool {
    d.name.starts_with(PREFIX)
}

fn is_spark(d: &&ParticleSystemDef) -> bool {
    d.name.to_ascii_lowercase().contains("spark")
}

/// Whether `d` is one of a fire file's own flame systems (not its sparks), which the
/// volumetric flame stands in for.
pub fn is_flame(d: &ParticleSystemDef) -> bool {
    !is_added(d) && !is_spark(&d)
}

/// The systems a fire whose own are `fire` gains, with the smoke systems `smoke`.
pub fn layers(fire: &[ParticleSystemDef], smoke: &[ParticleSystemDef]) -> Vec<ParticleSystemDef> {
    let mut out = Vec::new();
    if let Some(d) = fire.iter().find(is_spark).or_else(|| fire.iter().find(|d| d.strip.is_none())) {
        let mut m = d.clone();
        m.name = format!("{PREFIX}{} embers", d.name);
        m.rate *= EMBER_RATE;
        m.max = scaled(d.max, EMBER_MAX);
        // Points from birth to death: no growing in.
        m.grow_time = 0.0;
        unkey(&mut m);
        let e = &mut m.emitter;
        e.radius *= EMBER_SIZE;
        e.radius_variation *= EMBER_SIZE;
        e.life *= EMBER_LIFE;
        e.life_variation *= EMBER_LIFE;
        e.speed *= EMBER_SPEED;
        e.speed_variation *= EMBER_SPEED;
        e.declination_variation = e.declination_variation.max(EMBER_SPREAD);
        for g in &mut m.gravity {
            g.strength *= EMBER_LIFT;
            if let Some(k) = g.strength_keys.take() {
                g.strength = k.at(0.0).unwrap_or(g.strength) * EMBER_LIFT;
            }
        }
        out.push(m);
    }
    for d in smoke.iter().filter(|d| d.emitter.mesh.is_none() && d.strip.is_none()) {
        let mut s = d.clone();
        s.name = format!("{PREFIX}{} smoke", d.name);
        s.rate *= SMOKE_RATE;
        unkey(&mut s);
        let e = &mut s.emitter;
        e.frame = Affine3A::from_translation(Vec3::new(0.0, 0.0, SMOKE_START)) * e.frame;
        e.life *= SMOKE_LIFE;
        e.life_variation *= SMOKE_LIFE;
        e.color[3] *= SMOKE_ALPHA;
        out.push(s);
    }
    out
}

/// `max` times `by`, at least one more than `max`.
fn scaled(max: usize, by: f32) -> usize {
    ((max as f32 * by).ceil() as usize).max(max + 1)
}

/// Drops the keys of the fields scaled here (life and size), so the scaled values hold.
/// `MPSFireSmall01` keys neither [observed: its emitters' float controllers are birth
/// rate and on/off only].
fn unkey(d: &mut ParticleSystemDef) {
    d.emitter.keyed.retain(|(p, _)| !matches!(p, EmitterParam::LifeSpan | EmitterParam::Radius));
}
