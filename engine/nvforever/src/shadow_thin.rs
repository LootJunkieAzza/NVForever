//! A test aid: small objects far from the camera cast no sun shadows (`--shadow-thin`,
//! off by default). [engineering choices throughout; the original casts no sun shadows
//! from the world at all, docs/spec-notes/world.md §6]
//!
//! Built to cut the sun shadows' frame cost (2026-10-23) and measured not to pay
//! (docs/design/graphics-enhanced.md §5.5): the cost is mostly per shadow view, not per
//! caster. At Goodsprings the farthest cascade's ~2,000 casters are mostly mid-sized and
//! large objects; halving them (ratio 0.04, which drops visible shadows) saved 1-2 ms of
//! about 6.5, and the ratio 0.006 thinned 65 meshes and saved nothing measurable. It stays
//! for measuring: the frame-time log breaks the sun's last cascade down by angular size.
//!
//! The rule: a placed reference (all the meshes of one [`RefVisual`] together, so a
//! building made of many small pieces counts as the whole building) casts while its
//! nearest point is within [`NEAR`] of the camera, and beyond that while its bounding
//! radius is at least `ratio` × its distance (its angular size). Below that every one of
//! its meshes gets `NotShadowCaster`, so it casts no shadow from any light (the sun, the
//! arms' shade light, the flashlight beyond `NEAR`). It casts again once its radius is
//! [`HYSTERESIS`] × that, so an object at the edge does not flick on and off. Checked every
//! [`EVERY`] seconds while the sun casts shadows; all comes back when they go off.
//!
//! Left alone: meshes that are not placed references (terrain, trees, LOD, effects),
//! see-through ones, and every mesh some other rule already took the shadow from (a
//! `NotShadowCaster` this module did not put there is never removed).
//!
//! `--shadow-thin <ratio>` sets the ratio (0 turns the rule off); `--graphics-cycle` sets
//! it per set with `shadow-thin=<ratio>`.

use crate::fly::FlyCam;
use crate::sky::SkyState;
use bevy::camera::primitives::Aabb;
use bevy::light::NotShadowCaster;
use bevy::prelude::*;
use nvf_assets::NifMaterial;
use nvf_play::RefVisual;
use std::collections::HashMap;

/// Within this many units of the camera everything casts: the sun's first cascade
/// (a twelfth of the 8,000-unit default reach) and the arms' shade light lie inside it.
pub const NEAR: f32 = 1000.0;

/// The default ratio of bounding radius to distance below which a far object casts
/// nothing: 0, off (module notes). For scale: 0.006 rad is a radius of about 3 pixels at
/// 720 lines and a 75° field of view, objects under 12 units at 2,000 and under 48 at
/// 8,000.
pub const DEFAULT_RATIO: f32 = 0.0;

/// An object that casts nothing casts again at this many times the ratio.
pub const HYSTERESIS: f32 = 1.15;

/// Seconds between checks.
pub const EVERY: f32 = 0.25;

/// The rule's ratio; 0 or less: off.
#[derive(Resource, Clone, Debug)]
pub struct ShadowThin {
    pub ratio: f32,
    /// Meshes that cast nothing because of the rule at the last check.
    pub thinned: usize,
    /// References judged at the last check, and how many of them cast nothing.
    pub refs: (usize, usize),
    /// Each judged reference's bounding radius over its distance, `None` within [`NEAR`]
    /// (for the frame-time log's breakdown of what the cascades draw).
    pub sizes: HashMap<u32, Option<f32>>,
}

/// The angular sizes the frame-time log groups the casters of a cascade by.
pub const SIZE_BUCKETS: [f32; 6] = [0.003, 0.006, 0.01, 0.02, 0.04, 0.08];

/// Marks the meshes whose `NotShadowCaster` this module put there.
#[derive(Component)]
pub struct FarNoShadow;

/// Whether a reference with bounding radius `radius` whose centre is `distance` from the
/// camera casts, given whether it casts now.
pub fn casts(radius: f32, distance: f32, ratio: f32, casting_now: bool) -> bool {
    if ratio <= 0.0 || distance - radius <= NEAR {
        return true;
    }
    let need = ratio * distance * if casting_now { 1.0 } else { HYSTERESIS };
    radius >= need
}

pub struct ShadowThinPlugin(pub f32);

impl Plugin for ShadowThinPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ShadowThin { ratio: self.0, thinned: 0, refs: (0, 0), sizes: HashMap::new() }).add_systems(Update, thin);
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn thin(
    mut commands: Commands,
    time: Res<Time<Real>>,
    sky: Res<SkyState>,
    mut rule: ResMut<ShadowThin>,
    materials: Res<Assets<NifMaterial>>,
    cams: Query<&GlobalTransform, With<FlyCam>>,
    meshes: Query<(Entity, &RefVisual, &GlobalTransform, Option<&Aabb>, &MeshMaterial3d<NifMaterial>, Has<NotShadowCaster>, Has<FarNoShadow>)>,
    ours: Query<Entity, With<FarNoShadow>>,
    mut since: Local<f32>,
    mut last_ratio: Local<f32>,
    mut by_ref: Local<HashMap<u32, (Vec3, Vec3, bool)>>,
) {
    let on = sky.shadows && rule.ratio > 0.0;
    if !on {
        // Everything casts again (once: nothing is ours afterwards).
        if !ours.is_empty() {
            for e in &ours {
                commands.entity(e).try_remove::<(NotShadowCaster, FarNoShadow)>();
            }
            rule.thinned = 0;
            rule.refs = (0, 0);
            rule.sizes.clear();
        }
        *last_ratio = 0.0;
        return;
    }
    *since += time.delta_secs();
    if *since < EVERY && *last_ratio == rule.ratio {
        return;
    }
    *since = 0.0;
    *last_ratio = rule.ratio;
    let Some(cam) = cams.iter().next().map(|t| t.translation()) else { return };
    // Each reference's bounds (world space) and whether any of its meshes is ours now.
    by_ref.clear();
    for (_, r, t, aabb, _, _, mine) in &meshes {
        let Some(aabb) = aabb else { continue };
        let (lo, hi) = world_bounds(t, aabb);
        let b = by_ref.entry(r.0).or_insert((lo, hi, false));
        b.0 = b.0.min(lo);
        b.1 = b.1.max(hi);
        b.2 |= mine;
    }
    let mut sizes = HashMap::with_capacity(by_ref.len());
    let judged: HashMap<u32, bool> = by_ref
        .iter()
        .map(|(&r, &(lo, hi, thinned))| {
            let centre = (lo + hi) * 0.5;
            let radius = (hi - lo).length() * 0.5;
            let d = centre.distance(cam);
            sizes.insert(r, (d - radius > NEAR).then(|| radius / d.max(1.0)));
            (r, casts(radius, d, rule.ratio, !thinned))
        })
        .collect();
    rule.sizes = sizes;
    let mut thinned = 0;
    for (e, r, _, aabb, material, no_shadow, mine) in &meshes {
        if aabb.is_none() {
            continue;
        }
        let cast = judged.get(&r.0).copied().unwrap_or(true);
        if mine {
            if cast {
                commands.entity(e).try_remove::<(NotShadowCaster, FarNoShadow)>();
            } else {
                thinned += 1;
            }
        } else if !cast && !no_shadow && materials.get(&material.0).is_some_and(|m| !nvf_assets::nif_material::see_through(m)) {
            commands.entity(e).try_insert((NotShadowCaster, FarNoShadow));
            thinned += 1;
        }
    }
    let refs = (judged.len(), judged.values().filter(|c| !**c).count());
    if rule.thinned != thinned || rule.refs != refs {
        rule.thinned = thinned;
        rule.refs = refs;
    }
}

/// A mesh's bounds in world space (the corners of its box, transformed).
fn world_bounds(t: &GlobalTransform, aabb: &Aabb) -> (Vec3, Vec3) {
    let m = t.affine();
    let c = Vec3::from(aabb.center);
    let h = Vec3::from(aabb.half_extents);
    let centre = m.transform_point3(c);
    // The box's half extents along the world axes: |M| × h.
    let x = m.matrix3.x_axis.abs() * h.x;
    let y = m.matrix3.y_axis.abs() * h.y;
    let z = m.matrix3.z_axis.abs() * h.z;
    let e = Vec3::from(x + y + z);
    (centre - e, centre + e)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn near_everything_casts_far_only_the_large() {
        assert_eq!(DEFAULT_RATIO, 0.0, "off by default (module notes)");
        assert!(casts(0.01, 9000.0, DEFAULT_RATIO, true), "the default lets everything cast");
        let r = 0.006;
        assert!(casts(1.0, 500.0, r, true), "within NEAR");
        assert!(casts(5.0, NEAR + 5.0, r, true), "nearest point within NEAR");
        assert!(!casts(10.0, 2000.0, r, true), "a 10-unit box at 2,000");
        assert!(casts(15.0, 2000.0, r, true));
        assert!(!casts(40.0, 8000.0, r, true));
        assert!(casts(400.0, 8000.0, r, true), "a building at 8,000");
        assert!(casts(0.1, 9000.0, 0.0, false), "ratio 0: off");
    }

    #[test]
    fn an_object_at_the_edge_does_not_flick() {
        let r = 0.006;
        let d = 4000.0;
        let edge = r * d;
        assert!(casts(edge * 1.05, d, r, true), "casting: stays on just above the edge");
        assert!(!casts(edge * 1.05, d, r, false), "not casting: needs the hysteresis");
        assert!(casts(edge * 1.2, d, r, false));
    }

    #[test]
    fn a_rotated_box_bounds_in_world_space() {
        let t = GlobalTransform::from(Transform::from_xyz(100.0, 0.0, 0.0).with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).with_scale(Vec3::splat(2.0)));
        let aabb = Aabb { center: Vec3::ZERO.into(), half_extents: Vec3::new(10.0, 1.0, 1.0).into() };
        let (lo, hi) = world_bounds(&t, &aabb);
        assert!((lo - Vec3::new(98.0, -20.0, -2.0)).length() < 1e-3 && (hi - Vec3::new(102.0, 20.0, 2.0)).length() < 1e-3, "{lo} {hi}");
    }
}
