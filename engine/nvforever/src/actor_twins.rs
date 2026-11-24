//! A check, always on: two drawn bodies of one actor reference (owner's overnight queue
//! items 105 and 124, 2026-11-08/09). Every actor reference is drawn once; a second body
//! of the same reference is a stray left behind by whatever drew it last, and it fools
//! everything that finds an actor by its reference: detection measured both bodies, one
//! in sight and one not, by turns, so a young gecko hunter that was really beside the
//! player never stayed seen long enough to bite (item 105).
//!
//! Once a second, the drawn bodies (`ActorRig`) are counted by reference; a reference
//! with more than one gets a `actor twins:` warning naming where each body is (once per
//! reference and count), and the exit summary says how many references had twins.

use crate::limbs::ActorRig;
use bevy::app::AppExit;
use bevy::prelude::*;
use std::collections::HashMap;

/// How often the bodies are counted, seconds [engineering choice: a stray stays].
const EVERY: f32 = 1.0;
/// At most this many warning lines [engineering choice].
const MAX_LINES: u32 = 40;

pub struct ActorTwinsPlugin;

impl Plugin for ActorTwinsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Twins>().add_systems(PostUpdate, count).add_systems(Last, report);
    }
}

#[derive(Resource, Default)]
struct Twins {
    since: f32,
    /// The count last warned about, by reference.
    warned: HashMap<u32, usize>,
    lines: u32,
}

fn count(mut tw: ResMut<Twins>, time: Res<Time>, bodies: Query<(Entity, &ActorRig, &GlobalTransform)>) {
    tw.since += time.delta_secs();
    if tw.since < EVERY {
        return;
    }
    tw.since = 0.0;
    let mut by_ref: HashMap<u32, Vec<(Entity, Vec3)>> = HashMap::new();
    for (e, rig, gt) in &bodies {
        by_ref.entry(rig.reference).or_default().push((e, Vec3::from(nvf_play::space::to_game(gt.translation()))));
    }
    for (reference, list) in by_ref.into_iter().filter(|(_, l)| l.len() > 1) {
        if tw.warned.get(&reference) == Some(&list.len()) {
            continue;
        }
        tw.warned.insert(reference, list.len());
        if tw.lines < MAX_LINES {
            tw.lines += 1;
            let at: Vec<String> = list.iter().map(|(e, p)| format!("{e} at ({:.0}, {:.0}, {:.0})", p.x, p.y, p.z)).collect();
            warn!("actor twins: {reference:08X} is drawn {} times: {}", list.len(), at.join("; "));
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, tw: Res<Twins>) {
    if exit.read().next().is_some() {
        info!("actor twins summary: {} references were drawn more than once", tw.warned.len());
    }
}
