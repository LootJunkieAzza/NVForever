//! Diagnostics of refraction (docs/spec-notes/refraction.md): how many refracting meshes
//! (`nvf_assets::nif_material`, "Refraction": explosions' heat-haze shells, Mister
//! Handy's jet, the Stealth Boy's field) the camera draws. Ten times a second the meshes in
//! view are counted; the log says when some come into view after none were, and at exit
//! the most seen at once.
//!
//! Log lines: `refraction: drawn: <n> refracting meshes in view (<m> with a moving bend)`
//! and at exit `refraction summary: <k> times in view, at most <n> meshes`.

use bevy::prelude::*;
use nvf_assets::NifMaterial;

/// Seconds between counts.
const EVERY: f32 = 0.1;

#[derive(Resource, Default)]
struct Counts {
    since: f32,
    in_view: usize,
    times: u32,
    most: usize,
}

pub struct RefractionPlugin;

impl Plugin for RefractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Counts>().add_systems(PostUpdate, count).add_systems(Last, report);
    }
}

fn count(time: Res<Time>, mut c: ResMut<Counts>, materials: Res<Assets<NifMaterial>>, meshes: Query<(&MeshMaterial3d<NifMaterial>, &ViewVisibility)>) {
    c.since += time.delta_secs();
    if c.since < EVERY {
        return;
    }
    c.since = 0.0;
    let (mut n, mut moving) = (0, 0);
    for (m, seen) in &meshes {
        if !seen.get() {
            continue;
        }
        let Some(m) = materials.get(&m.0) else { continue };
        if m.extension.refract {
            n += 1;
            if m.extension.fire_period != 0.0 {
                moving += 1;
            }
        }
    }
    if n > 0 && c.in_view == 0 {
        c.times += 1;
        info!("refraction: drawn: {n} refracting meshes in view ({moving} with a moving bend)");
    }
    c.in_view = n;
    c.most = c.most.max(n);
}

fn report(mut exit: MessageReader<AppExit>, c: Res<Counts>) {
    if exit.read().next().is_none() {
        return;
    }
    info!("refraction summary: {} times in view, at most {} meshes", c.times, c.most);
}
