//! Actor hit volumes in play (docs/design/combat.md §5.2, step C3): every placed actor
//! (ACHR, ACRE) that is drawn gets a capsule in the physics world (`fnv_phys::actors`),
//! so shots, melee rays and the crosshair's collision ray can hit it.
//!
//! The capsule is sized and placed from the actor's render bounds each frame [design]:
//! the union of the world-space bounding boxes of the actor's drawn parts; feet at the
//! bottom centre, height the box's height, radius the smaller horizontal half-extent
//! (clamped to [`MIN_RADIUS`]..[`MAX_RADIUS`]). An actor that is hidden, removed by the
//! simulation or no longer drawn loses its volume. Dead actors move to the DEAD_BIP layer
//! once the simulation has a dead state (step C5).

use bevy::camera::primitives::Aabb;
use bevy::prelude::*;
use fnv_phys::HitVolume;
use std::collections::BTreeMap;

use crate::phys::{PhysState, PhysSystems};
use crate::sim::SimState;
use crate::{space, PlayWorld, RefVisual};

/// Radius limits, game units [design: a thin limb pose or a T-pose bind box should not
/// make a needle or a wall].
pub const MIN_RADIUS: f32 = 10.0;
pub const MAX_RADIUS: f32 = 150.0;

/// Volumes in the world now, and the most at once (for the exit report).
#[derive(Resource, Default)]
pub struct HitVolumes {
    pub count: usize,
    pub most: usize,
}

pub struct HitVolumePlugin;

impl Plugin for HitVolumePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HitVolumes>()
            .add_systems(Update, sync_volumes.after(PhysSystems).in_set(crate::PlaySet::Physics))
            .add_systems(Last, report);
    }
}

/// The game-space box of a part: the eight corners of its local bounds through its
/// transform, then into game space.
fn game_box(aabb: &Aabb, gt: &GlobalTransform) -> ([f32; 3], [f32; 3]) {
    let t = gt.affine();
    let (c, h) = (Vec3::from(aabb.center), Vec3::from(aabb.half_extents));
    let mut lo = [f32::INFINITY; 3];
    let mut hi = [f32::NEG_INFINITY; 3];
    for i in 0..8 {
        let s = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
        let p = space::to_game(t.transform_point3(c + h * s));
        for k in 0..3 {
            lo[k] = lo[k].min(p[k]);
            hi[k] = hi[k].max(p[k]);
        }
    }
    (lo, hi)
}

/// The capsule for an actor's game-space bounds.
pub fn volume_for(lo: [f32; 3], hi: [f32; 3], dead: bool) -> HitVolume {
    let half_x = (hi[0] - lo[0]) / 2.0;
    let half_y = (hi[1] - lo[1]) / 2.0;
    HitVolume {
        feet: [(lo[0] + hi[0]) / 2.0, (lo[1] + hi[1]) / 2.0, lo[2]],
        radius: half_x.min(half_y).clamp(MIN_RADIUS, MAX_RADIUS),
        height: (hi[2] - lo[2]).max(2.0 * MIN_RADIUS),
        dead,
    }
}

fn is_actor(play: &PlayWorld, form_id: u32) -> bool {
    play.index.refs.get(&form_id).is_some_and(|e| &e.sig == b"ACHR" || &e.sig == b"ACRE")
}

fn sync_volumes(
    play: Res<PlayWorld>,
    sim: Res<SimState>,
    parts: Query<(&RefVisual, &Aabb, &GlobalTransform, &InheritedVisibility)>,
    mut phys: ResMut<PhysState>,
    mut vols: ResMut<HitVolumes>,
) {
    let mut bounds: BTreeMap<u32, ([f32; 3], [f32; 3])> = BTreeMap::new();
    for (r, aabb, gt, vis) in &parts {
        if !vis.get() || !is_actor(&play, r.0) {
            continue;
        }
        let (lo, hi) = game_box(aabb, gt);
        let e = bounds.entry(r.0).or_insert((lo, hi));
        for k in 0..3 {
            e.0[k] = e.0[k].min(lo[k]);
            e.1[k] = e.1[k].max(hi[k]);
        }
    }
    bounds.retain(|&id, _| sim.present(id));
    let stale: Vec<u32> = phys.world.actor_volumes().filter(|id| !bounds.contains_key(id)).collect();
    for id in stale {
        phys.world.remove_actor_volume(id);
    }
    for (&id, &(lo, hi)) in &bounds {
        phys.world.set_actor_volume(id, volume_for(lo, hi, false));
    }
    if bounds.len() != vols.count {
        debug!("actor hit volumes: {}", bounds.len());
    }
    vols.count = bounds.len();
    vols.most = vols.most.max(vols.count);
}

/// At exit: the counts, and a probe of each volume: a level ray from 150 units away
/// (from the side the player is on) at the capsule's middle should hit that actor first,
/// unless something stands in between.
fn report(mut exit: MessageReader<AppExit>, vols: Res<HitVolumes>, mut phys: ResMut<PhysState>) {
    if exit.read().last().is_none() {
        return;
    }
    let ids: Vec<u32> = phys.world.actor_volumes().collect();
    let player = phys.player.as_ref().map(|c| c.feet);
    let (mut hit, mut other, mut none) = (0, 0, 0);
    for id in &ids {
        let Some(v) = phys.world.actor_volume(*id) else { continue };
        let c = v.center();
        let (dx, dy) = player.map_or((1.0, 0.0), |p| {
            let (dx, dy) = (p[0] - c[0], p[1] - c[1]);
            let l = (dx * dx + dy * dy).sqrt();
            if l > 1e-3 { (dx / l, dy / l) } else { (1.0, 0.0) }
        });
        let from = [c[0] + dx * 150.0, c[1] + dy * 150.0, c[2]];
        match phys.ray(from, [-dx, -dy, 0.0], 300.0, fnv_phys::Filter::default()) {
            Some(h) if h.owner == (fnv_phys::Owner::Actor { form_id: *id, layer: fnv_phys::layers::BIPED }) => hit += 1,
            Some(_) => other += 1,
            None => none += 1,
        }
    }
    info!(
        "actor hit volumes: {} at exit, at most {} at once; probe rays: {hit} hit their actor, {other} hit something else first, {none} hit nothing",
        vols.count, vols.most
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_from_bounds() {
        // A person about 128 tall, 60 wide (arms) and 30 deep.
        let v = volume_for([-30.0, -15.0, 0.0], [30.0, 15.0, 128.0], false);
        assert_eq!(v.feet, [0.0, 0.0, 0.0]);
        assert_eq!((v.radius, v.height), (15.0, 128.0));
        // A flat thing gets the minimum radius and height.
        let v = volume_for([0.0, 0.0, 5.0], [2.0, 2.0, 6.0], true);
        assert_eq!((v.radius, v.height, v.dead), (MIN_RADIUS, 2.0 * MIN_RADIUS, true));
    }
}
