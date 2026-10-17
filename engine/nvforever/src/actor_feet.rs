//! Test aid `NVF_TEST_ACTOR_FEET=<seconds>[:<range>]`: once, after that many seconds,
//! log for every drawn actor within `range` game units of the camera (default 3000) where
//! its root stands, where its lowest foot bone is, and the drawn landscape height under
//! it, so a floating actor shows in the log (docs/spec-notes/floating-actors.md).

use crate::limbs::ActorRig;
use crate::world::{bevy_to_game, WorldData};
use bevy::prelude::*;

/// A foot bone this far above the actor's root is off the ground: a standing actor's
/// ankles are about 7 units up, a walking one's lift a little more [observed].
const LIFTED: f32 = 30.0;

#[derive(Resource)]
pub struct FeetProbe {
    at: f32,
    range: f32,
    done: bool,
}

/// The probe asked for by the environment, if any.
pub fn from_env() -> Option<FeetProbe> {
    let v = std::env::var("NVF_TEST_ACTOR_FEET").ok()?;
    let mut it = v.split(':');
    let at = it.next()?.trim().parse().ok()?;
    let range = it.next().and_then(|r| r.trim().parse().ok()).unwrap_or(3000.0);
    Some(FeetProbe { at, range, done: false })
}

#[allow(clippy::type_complexity)]
pub fn probe(
    time: Res<Time>,
    mut p: ResMut<FeetProbe>,
    cams: Query<&GlobalTransform, With<Camera3d>>,
    roots: Query<(&Name, &GlobalTransform, &InheritedVisibility, &ActorRig)>,
    nodes: Query<&GlobalTransform>,
    bodies: Query<(&Name, &GlobalTransform), Without<ActorRig>>,
    world: Option<Res<WorldData>>,
) {
    if p.done || time.elapsed_secs() < p.at {
        return;
    }
    p.done = true;
    let Some(cam) = cams.iter().next().map(|c| bevy_to_game(c.translation())) else { return };
    let (mut n, mut lifted) = (0, 0);
    for (name, gt, vis, rig) in &roots {
        if !name.as_str().starts_with("Actor ") || !vis.get() {
            continue;
        }
        let root = bevy_to_game(gt.translation());
        if root.truncate().distance(cam.truncate()) > p.range {
            continue;
        }
        let foot = (rig.skel.names.iter().zip(&rig.nodes))
            .filter(|(n, _)| n.as_deref().is_some_and(|n| n.contains("Foot")))
            .filter_map(|(_, e)| nodes.get(*e).ok())
            .map(|t| bevy_to_game(t.translation()).z)
            .reduce(f32::min);
        let ground = world.as_deref().and_then(|w| land_height(w, root.x, root.y));
        let fmt = |v: Option<f32>| v.map_or("-".to_string(), |v| format!("{v:.0}"));
        let fmt1 = |v: Option<f32>| v.map_or("-".to_string(), |v| format!("{v:.1}"));
        info!(
            "actor feet: {} at ({:.0}, {:.0}, {:.0}): lowest foot bone z {}, drawn landscape z {}, root {} above it",
            name.as_str().trim_start_matches("Actor "),
            root.x,
            root.y,
            root.z,
            fmt(foot),
            fmt(ground),
            fmt1(ground.map(|g| root.z - g)),
        );
        n += 1;
        lifted += usize::from(foot.is_some_and(|f| f - root.z > LIFTED));
    }
    // The player's own body (third-person rig), where it is drawn over the landscape.
    for (name, gt) in &bodies {
        if name.as_str() == "Player body (Third person)" {
            let root = bevy_to_game(gt.translation());
            let ground = world.as_deref().and_then(|w| land_height(w, root.x, root.y));
            let above = ground.map_or("-".to_string(), |g| format!("{:.1}", root.z - g));
            info!("actor feet: player body at ({:.0}, {:.0}, {:.1}): drawn landscape z {}, root {above} above it", root.x, root.y, root.z, ground.map_or("-".to_string(), |g| format!("{g:.1}")));
        }
    }
    info!("actor feet: {n} actors within {:.0} units of the camera, {lifted} with a foot bone over {LIFTED} units above the root", p.range);
}

/// The full-detail landscape height at game (x, y), as the terrain mesh draws it.
fn land_height(world: &WorldData, x: f32, y: f32) -> Option<f32> {
    let g = fnv_world::cell_of(x, y);
    let land = world.grid_cell(g)?.land?;
    let land = fnv_world::land::load_land(world.lo, world.schema, land, fnv_world::land::LandParts::HEIGHTS)?;
    let (ox, oy) = (g.0 as f32 * fnv_world::CELL_SIZE, g.1 as f32 * fnv_world::CELL_SIZE);
    Some(fnv_world::grass::height_at(land.heights.as_ref()?, x - ox, y - oy))
}
