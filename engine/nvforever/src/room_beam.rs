//! Window-ray card replacements [design, #4698; docs/design/room-beams.md].

use crate::emittance::EmittanceState;
#[cfg(any(test, debug_assertions, feature = "dev-aids"))]
use crate::emittance::RoomRayEmittance;
use crate::fly::FlyCam;
use crate::graphics::Graphics;
use crate::rooms::RoomHidden;
use crate::sky::SkyState;
use crate::world::Streamer;
use bevy::camera::primitives::{Aabb, Frustum};
use bevy::camera::visibility::VisibilitySystems;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::ShaderType;
use nvf_assets::room_rays::{ray_shapes, RayBeam, RayShape};
use nvf_assets::GameData;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

/// The fitted prototype look: separate shafts with no brightness boost.
const PEAK: f32 = 0.20;
const PHASE_CAP: f32 = 1.6;
const TUNING: Vec4 = Vec4::new(PEAK, 0.4, PHASE_CAP, 96.0);
const MIN_ASPECT: f32 = 0.35;
const AMBIENT_SHARE: f32 = 0.30;
/// Keep upload and overdraw bounded [design, #4698].
const MAX_SHAFTS: usize = 48;

/// Static ray-card metadata. The fit and the render buffers are prepared on demand.
#[derive(Component)]
pub(crate) struct RaySource {
    #[cfg(any(test, debug_assertions, feature = "dev-aids"))]
    pub reference: u32,
    pub cell: u32,
    pub xemi: Option<u32>,
    pub path: Arc<str>,
    pub block: u32,
}

#[cfg(any(test, debug_assertions, feature = "dev-aids"))]
pub(crate) fn eligible(beam: Result<RayBeam, &'static str>, source: Option<RoomRayEmittance>) -> Result<RayBeam, &'static str> {
    match source {
        Some(RoomRayEmittance::Daylight) => (),
        Some(RoomRayEmittance::OtherRegion) => return Err("not a daylight region"),
        Some(RoomRayEmittance::Light) => return Err("light-backed XEMI"),
        None => return Err("unresolved XEMI"),
    }
    let beam = beam?;
    if !beam.is_long() {
        return Err("too short");
    }
    Ok(beam)
}

/// One beam on the GPU (room_beam.wesl `Beam`), in Bevy world space.
#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct GpuBeam {
    /// xyz: the middle of the bright end; w: the length.
    pub a: Vec4,
    /// xyz: the section's first axis (unit); w: its half width.
    pub u: Vec4,
    /// xyz: the second axis (unit, u x v = axis); w: its half width.
    pub v: Vec4,
    /// xyz: the axis (unit, from the window into the room); w: 1 / the geometric mean of the
    /// section's full widths.
    pub axis: Vec4,
    /// xy: how the end planes lean (distance along the axis per unit of u and v); zw unused.
    pub lean: Vec4,
    /// rgb: the colour (display values) with the ambient hue mixed in; w unused.
    pub colour: Vec4,
}

/// The frame's beams on the main camera (extracted to its view; empty: both passes skip).
#[derive(Component, ExtractComponent, Clone, Debug, Default)]
#[extract_app(bevy::render::RenderApp)]
pub struct RoomBeamCamera {
    pub beams: Vec<GpuBeam>,
    /// Peak display addition, phase asymmetry, phase cap, and near fade in world units.
    pub tune: Vec4,
    pub physical: bool,
    /// Each card owns a contiguous range of shafts in `beams`.
    pub cards: Vec<(Entity, Range<usize>)>,
}

/// A section's half widths with its narrow side raised to `min_aspect` of its wide one.
fn section(h: [f32; 2], min_aspect: f32) -> [f32; 2] {
    let floor = min_aspect.clamp(0.0, 1.0) * h[0].max(h[1]);
    [h[0].max(floor), h[1].max(floor)]
}

/// A beam in world space from its shape's world transform: a prism of the mean of its two ends'
/// sections (widening is a later slice), the narrow side raised by `min_aspect`.
fn gpu_beam(b: &RayBeam, world: &Affine3A, min_aspect: f32, colour: Vec3) -> Option<GpuBeam> {
    let along = world.transform_vector3(b.axis);
    let k = along.length();
    if !(k.is_finite() && k > 1e-6) {
        return None;
    }
    let axis = along / k;
    let u = world.transform_vector3(b.u).normalize_or_zero();
    let v = axis.cross(u).normalize_or_zero();
    if u == Vec3::ZERO || v == Vec3::ZERO {
        return None;
    }
    let mean = [0, 1].map(|i| 0.5 * (b.half_a[i] + b.half_b[i]) * k);
    let h = section(mean, min_aspect);
    if h[0] <= 1e-3 || h[1] <= 1e-3 {
        return None;
    }
    let length = b.length * k;
    Some(GpuBeam {
        a: world.transform_point3(b.a).extend(length),
        u: u.extend(h[0]),
        v: v.extend(h[1]),
        axis: axis.extend(1.0 / (2.0 * (h[0] * h[1]).sqrt())),
        lean: Vec4::new(b.lean[0], b.lean[1], 0.0, 0.0),
        colour: colour.extend(0.0),
    })
}

/// The prism's 8 corners in world space: the bright end's four, then the far one's, each ring
/// counter-clockwise seen from the far end (room_beam.wesl's vertex shader makes the same).
fn gpu_corners(b: &GpuBeam) -> [Vec3; 8] {
    let ring = [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)];
    std::array::from_fn(|i| {
        let (x, y) = (ring[i % 4].0 * b.u.w, ring[i % 4].1 * b.v.w);
        let z = if i < 4 { 0.0 } else { b.a.w } + b.lean.x * x + b.lean.y * y;
        b.a.truncate() + b.u.truncate() * x + b.v.truncate() * y + b.axis.truncate() * z
    })
}

/// The colour moved `cool` toward the hue of the cell's ambient at the colour's own luma (so
/// the strength stays the one PEAK defines) [design: plan §2.5 "Colour"].
fn cooled(colour: Vec3, ambient: Vec3, cool: f32) -> Vec3 {
    let luma = |c: Vec3| c.dot(Vec3::new(0.2126, 0.7152, 0.0722));
    let a = luma(ambient);
    if a <= 1e-4 {
        return colour;
    }
    colour.lerp(ambient / a * luma(colour), cool.clamp(0.0, 1.0))
}


#[derive(Resource, Default)]
struct Beams {
    models: HashMap<Arc<str>, Vec<RayShape>>,
    view: Option<(Entity, u32)>,
    counts: Option<(usize, usize)>,
}

pub(crate) struct RoomBeamPlugin;

impl Plugin for RoomBeamPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Beams>()
            .add_plugins(ExtractComponentPlugin::<RoomBeamCamera>::default())
            .add_systems(PostUpdate, sync
                .after(bevy::transform::TransformSystems::Propagate)
                .after(VisibilitySystems::UpdateFrusta)
                .after(VisibilitySystems::VisibilityPropagate));
        crate::room_beam_pass::build(app);
    }
}

type Cards<'w, 's> = Query<'w, 's, (
    Entity, &'static RaySource, &'static GlobalTransform, &'static InheritedVisibility, Has<RoomHidden>,
)>;

impl Beams {
    fn clear(&mut self, commands: &mut Commands) {
        if let Some((camera, _)) = self.view.take() {
            commands.entity(camera).try_remove::<RoomBeamCamera>();
            self.models.clear();
            if self.counts.is_some_and(|(cards, _)| cards > 0) {
                info!("room beam: replacement cleared");
            }
        }
        self.counts = None;
    }
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn sync(
    mut commands: Commands,
    graphics: Res<Graphics>,
    streamer: Option<Res<Streamer>>,
    sky: Option<Res<SkyState>>,
    data: Option<Res<GameData>>,
    emittance: Res<EmittanceState>,
    mut state: ResMut<Beams>,
    cards: Cards,
    cameras: Query<(Entity, &Camera, &GlobalTransform, &Frustum), With<FlyCam>>,
) {
    if !graphics.interior_beams {
        if state.view.is_some() {
            state.clear(&mut commands);
        }
        return;
    }
    let interior = streamer.and_then(|s| s.interior);
    let (Some(cell), Some(sky), Some(data), Ok((camera, view, transform, frustum))) =
        (interior, sky, data, cameras.single()) else {
            state.clear(&mut commands);
            return;
        };
    if !view.is_active {
        state.clear(&mut commands);
        return;
    }
    let key = (camera, cell);
    if state.view != Some(key) {
        state.clear(&mut commands);
        state.view = Some(key);
    }

    let eye = transform.translation();
    let ambient = sky.interior.map(|l| Vec3::from_array(l.ambient.map(|b| b as f32 / 255.0)));
    let physical = nvf_assets::authored::physical_pbr();
    let mut colours = HashMap::new();
    let mut found = Vec::new();
    for (entity, source, transform, shown, room_hidden) in &cards {
        if source.cell != cell || room_hidden || !shown.get() {
            continue;
        }
        let Some(xemi) = source.xemi else { continue };
        let Some(colour) = *colours.entry(xemi).or_insert_with(|| emittance.room_ray_colour(&sky, xemi)) else { continue };
        let colour = ambient.map_or(colour, |a| cooled(colour, a, AMBIENT_SHARE));
        if colour.max_element() * PEAK * PHASE_CAP < 1.0 / 255.0 {
            continue;
        }
        // The physical-lighting camera composites in linear light.
        let colour = if physical { colour.max(Vec3::ZERO).powf(2.2) } else { colour };
        let fits = state.models.entry(source.path.clone()).or_insert_with(|| ray_shapes(&data, &source.path));
        let Some(fit) = fits.iter().find(|s| s.block == source.block) else { continue };
        let Ok(whole) = fit.beam else { continue };
        if !whole.is_long() {
            continue;
        }
        let mut nearest = f32::MAX;
        let parts = if fit.shafts.is_empty() { std::slice::from_ref(&whole) } else { &fit.shafts };
        let shafts: Vec<_> = parts.iter().filter_map(|part| {
            let box_beam = gpu_beam(part, &transform.affine(), MIN_ASPECT, colour)?;
            let (lo, hi) = gpu_corners(&box_beam).into_iter().fold((Vec3::MAX, Vec3::MIN), |(l, h), c| (l.min(c), h.max(c)));
            if !frustum.intersects_obb(&Aabb::from_min_max(lo, hi), &Affine3A::IDENTITY, true, true) {
                return None;
            }
            nearest = nearest.min((lo - eye).max(eye - hi).max(Vec3::ZERO).length_squared());
            Some(box_beam)
        }).collect();
        if !shafts.is_empty() {
            found.push((nearest, entity, shafts));
        }
    }
    found.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let mut requested = Vec::new();
    let mut beams = Vec::new();
    for (_, entity, shafts) in found {
        if beams.len() + shafts.len() <= MAX_SHAFTS {
            requested.push((entity, beams.len()..beams.len() + shafts.len()));
            beams.extend(shafts);
        }
    }
    let counts = (requested.len(), beams.len());
    if state.counts != Some(counts) {
        info!("room beam: cell 0x{cell:08X}: requested {} ray cards and {} shafts", counts.0, counts.1);
        state.counts = Some(counts);
    }
    if beams.is_empty() {
        commands.entity(camera).try_remove::<RoomBeamCamera>();
    } else {
        commands.entity(camera).insert(RoomBeamCamera { beams, tune: TUNING, physical, cards: requested });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn beam() -> RayBeam {
        RayBeam {
            a: Vec3::ZERO, axis: Vec3::Z, length: 300.0,
            u: Vec3::X, v: Vec3::Y, half_a: [45.0, 28.0], half_b: [45.0, 28.0],
            lean: [0.5, 0.0], share: 1.0,
        }
    }

    #[test]
    fn one_union_curve_bounds_all_overlapping_shafts() {
        let strength = |tau: f32| (1.0 - (-tau).exp()) / (1.0 - (-1.0f32).exp());
        assert_eq!(strength(0.0), 0.0);
        assert_eq!(strength(1.0), 1.0);
        assert!(strength(2.0) < strength(10.0) && strength(10.0) < 1.583);
    }

    // ---- The CPU mirror of room_beam.wesl's clip and quadrature (plan §5 slice 2's checks).

    /// A ray in a beam's own space: origin and direction (X, Y, Z), and `s = Z - lean . XY`.
    struct Ray {
        o: Vec3,
        d: Vec3,
        s0: f32,
        sd: f32,
    }

    fn beam_ray(b: &GpuBeam, eye: Vec3, dir: Vec3) -> Ray {
        let rel = eye - b.a.truncate();
        let o = Vec3::new(rel.dot(b.u.truncate()), rel.dot(b.v.truncate()), rel.dot(b.axis.truncate()));
        let d = Vec3::new(dir.dot(b.u.truncate()), dir.dot(b.v.truncate()), dir.dot(b.axis.truncate()));
        Ray { o, d, s0: o.z - b.lean.x * o.x - b.lean.y * o.y, sd: d.z - b.lean.x * d.x - b.lean.y * d.y }
    }

    fn slab(o: f32, d: f32, lo: f32, hi: f32, r: (f32, f32)) -> (f32, f32) {
        if d.abs() < 1e-8 {
            return if o < lo || o > hi { (1.0, 0.0) } else { r };
        }
        let (ta, tb) = ((lo - o) / d, (hi - o) / d);
        (r.0.max(ta.min(tb)), r.1.min(ta.max(tb)))
    }

    fn clip(b: &GpuBeam, r: &Ray, far: f32) -> (f32, f32) {
        let mut range = (0.0, far);
        range = slab(r.o.x, r.d.x, -b.u.w, b.u.w, range);
        range = slab(r.o.y, r.d.y, -b.v.w, b.v.w, range);
        slab(r.s0, r.sd, 0.0, b.a.w, range)
    }

    fn density(b: &GpuBeam, r: &Ray, t: f32) -> f32 {
        let x = (r.o.x + r.d.x * t) / b.u.w;
        let y = (r.o.y + r.d.y * t) / b.v.w;
        let w = (r.s0 + r.sd * t) / b.a.w;
        let (px, py, pw) = ((1.0 - x * x).max(0.0), (1.0 - y * y).max(0.0), (1.0 - w).clamp(0.0, 1.0));
        px * px * py * py * pw
    }

    const G5: [(f32, f32); 5] = [(0.0, 0.568_888_9), (-0.538_469_3, 0.478_628_67), (0.538_469_3, 0.478_628_67), (-0.906_179_8, 0.236_926_88), (0.906_179_8, 0.236_926_88)];

    fn gauss(b: &GpuBeam, r: &Ray, t0: f32, t1: f32) -> f32 {
        let (mid, half) = (0.5 * (t0 + t1), 0.5 * (t1 - t0));
        let nodes = &G5;
        nodes.iter().map(|(x, w)| w * density(b, r, mid + half * x)).sum::<f32>() * half
    }

    fn steps(b: &GpuBeam, r: &Ray, t0: f32, t1: f32, n: usize) -> f64 {
        let dt = (t1 - t0) as f64 / n as f64;
        (0..n).map(|i| density(b, r, (t0 as f64 + (i as f64 + 0.5) * dt) as f32) as f64).sum::<f64>() * dt
    }

    #[test]
    fn the_quadrature_is_the_integral() {
        let g = gpu_beam(&beam(), &Affine3A::IDENTITY, 0.35, Vec3::ONE).expect("a beam");
        let centre = g.a.truncate() + g.axis.truncate() * (0.3 * g.a.w);
        // Rays from all around through points of the beam, and from inside it.
        let mut checked = 0;
        for i in 0..64 {
            let k = i as f32;
            let dir = Vec3::new((k * 1.37).sin(), (k * 0.71).cos(), (k * 2.13).sin() * 0.8).normalize();
            let target = centre + g.u.truncate() * (g.u.w * 0.6 * (k * 0.9).sin()) + g.v.truncate() * (g.v.w * 0.5 * (k * 1.7).cos());
            let eye = if i % 8 == 0 { target } else { target - dir * 1500.0 };
            {
                let r = beam_ray(&g, eye, dir);
                let (t0, t1) = clip(&g, &r, 1e6);
                if t1 <= t0 {
                    continue;
                }
                let exact = steps(&g, &r, t0, t1, 4096);
                let quad = gauss(&g, &r, t0, t1) as f64;
                if exact > 1e-3 {
                    assert!(((quad - exact) / exact).abs() < 1e-3, "{i}: {quad} against {exact}");
                    checked += 1;
                }
            }
        }
        assert!(checked >= 60, "{checked}");
    }

}
