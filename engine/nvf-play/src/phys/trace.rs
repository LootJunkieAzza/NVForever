//! Test aid `NVF_PLAY_TEST_PHYS_TRACE=<seconds>[:<every seconds>]`: for that long after
//! each physics cell is loaded, log every placed reference with a dynamic body — where
//! each body is against where it was first seen, how far it has turned, its linear and
//! angular speed and whether it sleeps — every `every` seconds (default 0.25), and, at
//! the first sighting, every contact of its bodies that overlaps by more than
//! [`OVERLAP_LOGGED`] units and with what (a body pushed apart at placement shows there).
//! Lines start with `phys trace:`. Not a game behaviour; no effect unless set.

use super::PhysState;
use bevy::prelude::*;
use fnv_phys::Owner;
use std::collections::BTreeMap;

/// Overlaps deeper than this (game units) are logged at the first sighting.
const OVERLAP_LOGGED: f32 = 0.5;

#[derive(Resource)]
pub(crate) struct PhysTrace {
    seconds: f32,
    every: f32,
    /// Per traced reference: the step it was first seen at, its bodies' first positions
    /// and rotations, and the last step logged.
    seen: BTreeMap<u32, Seen>,
}

struct Seen {
    first_step: u64,
    start: Vec<([f32; 3], [f32; 9])>,
    logged_step: u64,
    done: bool,
    contacts_logged: bool,
}

impl PhysTrace {
    pub(crate) fn from_env() -> Option<PhysTrace> {
        let v = std::env::var("NVF_PLAY_TEST_PHYS_TRACE").ok()?;
        let mut parts = v.split(':');
        let seconds = parts.next()?.trim().parse().ok()?;
        let every = parts.next().and_then(|e| e.trim().parse().ok()).unwrap_or(0.25f32).max(0.0);
        Some(PhysTrace { seconds, every, seen: BTreeMap::new() })
    }
}

fn owner_name(o: Option<Owner>) -> String {
    match o {
        Some(Owner::Ref { form_id, object, layer }) => format!("ref {form_id:08X} object {object} layer {layer}"),
        Some(Owner::Terrain { x, y }) => format!("terrain ({x}, {y})"),
        Some(other) => format!("{other:?}"),
        None => "unknown".into(),
    }
}

/// The angle (degrees) between two row-major rotations.
fn turned(a: &[f32; 9], b: &[f32; 9]) -> f32 {
    // trace(Aᵀ B) = 1 + 2 cos θ
    let tr: f32 = (0..3).map(|i| (0..3).map(|k| a[k * 3 + i] * b[k * 3 + i]).sum::<f32>()).sum();
    ((tr - 1.0) / 2.0).clamp(-1.0, 1.0).acos().to_degrees()
}

pub(crate) fn trace(trace: Option<ResMut<PhysTrace>>, phys: Res<PhysState>) {
    let Some(mut t) = trace else { return };
    let dt = phys.world.params.dt;
    let step = phys.world.steps();
    let refs: Vec<u32> = phys.cells.values().flat_map(|c| c.refs.iter().copied()).collect();
    for r in refs {
        let spins = phys.world.ref_spins(r);
        if spins.iter().all(Option::is_none) {
            continue;
        }
        let bodies = phys.world.ref_bodies(r);
        let (seconds, every) = (t.seconds, t.every);
        let seen = t.seen.entry(r).or_insert_with(|| {
            let start = bodies.iter().map(|b| (b.translation, b.rotation)).collect();
            Seen { first_step: step, start, logged_step: step, done: false, contacts_logged: false }
        });
        // Contacts exist once a step has run since the reference was added.
        if !seen.contacts_logged && step > seen.first_step {
            seen.contacts_logged = true;
            let deep: Vec<String> = phys
                .world
                .ref_contacts(r)
                .iter()
                .filter(|(_, _, d)| *d < -OVERLAP_LOGGED)
                .map(|(i, o, d)| format!("body {i} into {} by {:.1}", owner_name(*o), -d))
                .collect();
            info!(
                "phys trace: {r:08X} first seen at step {}: {} bodies ({} dynamic); overlaps after step {step}: {}",
                seen.first_step,
                bodies.len(),
                spins.iter().filter(|s| s.is_some()).count(),
                if deep.is_empty() { "none".to_string() } else { deep.join("; ") }
            );
        }
        if seen.done {
            continue;
        }
        let since = (step - seen.first_step) as f32 * dt;
        if since > seconds {
            seen.done = true;
            continue;
        }
        if step != seen.first_step && ((step - seen.logged_step) as f32 * dt) + 1e-4 < every {
            continue;
        }
        seen.logged_step = step;
        for (i, (b, spin)) in bodies.iter().zip(&spins).enumerate() {
            let Some(spin) = spin else { continue };
            let Some((p0, r0)) = seen.start.get(i) else { continue };
            let d: Vec<f32> = (0..3).map(|k| b.translation[k] - p0[k]).collect();
            let speed = b.linvel.iter().map(|v| v * v).sum::<f32>().sqrt();
            let spin_speed = spin.iter().map(|v| v * v).sum::<f32>().sqrt().to_degrees();
            info!(
                "phys trace: {r:08X} body {i} at {since:.2} s: moved ({:.1}, {:.1}, {:.1}), turned {:.1}°, speed {speed:.1}, spin {spin_speed:.0}°/s{}",
                d[0],
                d[1],
                d[2],
                turned(r0, &b.rotation),
                if b.sleeping { ", asleep" } else { "" }
            );
        }
    }
}
