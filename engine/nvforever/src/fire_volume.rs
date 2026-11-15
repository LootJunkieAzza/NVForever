//! Volumetric flames [design], part of the fuller fire (`fuller_fire.rs`, the
//! `fuller-fire` switch): each fire add-on node (ADDN model `mpsfire*`) burns a flame
//! drawn by marching rays through a box above it, not by camera-facing squares, so the
//! fire has depth from every side and from above (owner 2026-11-09: the denser squares
//! looked "not 3d or real").
//!
//! - **Shape**: a teardrop of rising, swirling 3D noise, widest near the base and
//!   flickering to a tip; the noise rises and twists faster higher up
//!   (`fire_volume.wesl`).
//! - **Colour**: by heat, deep red at the cool edges through orange to a pale yellow
//!   core, bright enough there for the light glow to take it; added to what is behind.
//! - **Hidden by the scene**: the march stops at the depth the scene drew (the logs,
//!   the ground, a wall), so the flame sits among the logs and never shows through a
//!   wall; without a depth prepass (the water's mirror camera) the box's own depth test
//!   stands in.
//! - While it burns, the add-on's own flame squares are not born (`particles.rs`); its
//!   sparks, the embers and the smoke of `fuller_fire` stay.
//!
//! The common technique of real-time volumetric fire (noise-shaped density, a heat to
//! colour ramp, emission summed along the ray); all values set by eye on the campfire
//! shots of 2026-11-09.
//!
//! ## Cost
//!
//! Every pixel a flame's box covers runs the march: up to [`MAX_STEPS`] steps of three
//! noise sums each, pure arithmetic. On 2026-11-14 the owner's PC powered off when a
//! campfire was lit [observed]: a modded campfire stacks five fire nodes in one spot, so
//! eight boxes burned there, close up, at 2560x1440 with no frame cap: the GPU went from
//! light load to a full arithmetic load in one frame [hypothesis: the power supply's
//! protection tripped on that step]. So the cost is planned each frame ([`plan_flames`]):
//!
//! - **One flame per spot**: nodes within half a radius of each other burn one flame
//!   (the largest); at most [`MAX_FLAMES`] flames, the nearest.
//! - **Steps by screen share**: a flame's steps times the share of the screen its box
//!   covers stays under [`FLAME_BUDGET`]; a flame across the room marches many steps, one
//!   filling the view marches few (the eye can't tell the steps apart there).
//! - **A frame budget**: the flames' steps times shares sum to at most [`FRAME_BUDGET`]
//!   (six full screens of one step); past that the steps shrink to [`MIN_STEPS`] and then
//!   the farthest flames go.
//! - **The shader holds the bounds too** (`fire_volume.wesl`): a NaN, a huge step count or
//!   a ray longer than the box's diagonal marches nothing past them.
//!
//! Off by default (the `fuller-fire` switch) until the capped flame is measured safe.

use bevy::asset::embedded_asset;
use bevy::camera::visibility::NoFrustumCulling;
use bevy::light::NotShadowCaster;
use bevy::mesh::MeshVertexBufferLayoutRef;
use bevy::pbr::{MaterialPipeline, MaterialPipelineKey, MeshPipelineKey};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, CompareFunction, Face, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use bevy::transform::TransformSystems;
use nvf_play::sfx::Sfx;

use crate::particles::AddonEmitter;

/// A fire's flame: (radius at the base, height), units, at the node's scale 1.
pub fn flame_size(path: &str) -> (f32, f32) {
    let name = path.rsplit(['\\', '/']).next().unwrap_or(path).to_ascii_lowercase();
    if name.contains("med") || name.contains("large") {
        (48.0, 165.0)
    } else {
        (30.0, 100.0)
    }
}

/// How far below the add-on node the flame starts (units), so it rises out of the fuel.
const SINK: f32 = 4.0;
/// Most ray steps through a flame (`fire_volume.wesl` MAX_STEPS).
pub const MAX_STEPS: f32 = 40.0;
/// Fewest ray steps through a drawn flame (`fire_volume.wesl` MIN_STEPS).
pub const MIN_STEPS: f32 = 4.0;
/// Most flames drawn at once.
pub const MAX_FLAMES: usize = 4;
/// One flame's steps times its share of the screen, at most.
pub const FLAME_BUDGET: f32 = 4.0;
/// All flames' steps times their shares of the screen, at most.
pub const FRAME_BUDGET: f32 = 6.0;

/// The camera a plan is made for.
#[derive(Clone, Copy, Debug)]
pub struct View {
    pub eye: Vec3,
    /// Vertical field of view, radians.
    pub fov: f32,
    /// Width over height.
    pub aspect: f32,
}

/// A flame's box: its base (bottom centre), radius and height, world units.
#[derive(Clone, Copy, Debug)]
pub struct FlameBox {
    pub base: Vec3,
    pub radius: f32,
    pub height: f32,
}

/// The share of the screen (0..1) a flame's box can cover seen from `view`: its height
/// and width over the view's at the box's nearest point; 1 from inside it.
pub fn screen_share(b: &FlameBox, view: &View) -> f32 {
    let lo = b.base - Vec3::new(b.radius, 0.0, b.radius);
    let hi = b.base + Vec3::new(b.radius, b.height, b.radius);
    let near = view.eye.clamp(lo, hi);
    let d = near.distance(view.eye);
    let half = (view.fov.clamp(0.1, 3.0) * 0.5).tan();
    let aspect = view.aspect.clamp(0.2, 10.0);
    let tall = (b.height / (2.0 * d * half)).min(1.0);
    let wide = (2.0 * b.radius / (2.0 * d * half * aspect)).min(1.0);
    let share = tall * wide;
    if share.is_finite() {
        share.clamp(0.0, 1.0)
    } else {
        1.0
    }
}

/// Which flames draw this frame and with how many steps ([`plan_flames`]).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    /// Per flame given: its steps, or `None` when it is not drawn.
    pub steps: Vec<Option<f32>>,
    /// Flames drawn.
    pub shown: usize,
    /// Flames not drawn because another burns in the same spot.
    pub stacked: usize,
    /// Flames not drawn because of [`MAX_FLAMES`] or [`FRAME_BUDGET`].
    pub over: usize,
    /// The drawn flames' steps times their screen shares, summed (at most [`FRAME_BUDGET`]).
    pub load: f32,
}

/// Plans the flames' cost for one frame (the module's "Cost"). With no camera the flames
/// are taken to fill the screen: [`MIN_STEPS`] each, as many as [`FRAME_BUDGET`] holds.
pub fn plan_flames(flames: &[FlameBox], view: Option<View>) -> Plan {
    let mut plan = Plan { steps: vec![None; flames.len()], ..Plan::default() };
    let sane = |b: &FlameBox| b.base.is_finite() && b.radius.is_finite() && b.height.is_finite() && b.radius > 0.0 && b.height > 0.0;
    // One per spot: the largest of the nodes stacked there.
    let mut order: Vec<usize> = (0..flames.len()).filter(|&i| sane(&flames[i])).collect();
    order.sort_by(|&a, &b| flames[b].radius.total_cmp(&flames[a].radius).then(a.cmp(&b)));
    let mut spots: Vec<usize> = Vec::new();
    for i in order {
        let f = &flames[i];
        let same = spots.iter().any(|&j| {
            let g = &flames[j];
            let across = Vec2::new(f.base.x - g.base.x, f.base.z - g.base.z).length();
            across <= 0.5 * f.radius.max(g.radius) && (f.base.y - g.base.y).abs() <= 0.5 * f.height.max(g.height)
        });
        if same {
            plan.stacked += 1;
        } else {
            spots.push(i);
        }
    }
    let view = view.filter(|v| v.eye.is_finite() && v.fov.is_finite() && v.aspect.is_finite());
    let Some(view) = view else {
        // Each flame taken to fill the screen: as many as the frame's budget holds.
        for &i in spots.iter().take(((FRAME_BUDGET / MIN_STEPS) as usize).clamp(1, MAX_FLAMES)) {
            plan.steps[i] = Some(MIN_STEPS);
            plan.shown += 1;
            plan.load += MIN_STEPS;
        }
        plan.over = spots.len() - plan.shown;
        return plan;
    };
    // The nearest first.
    let dist = |i: usize| {
        let b = &flames[i];
        (b.base + Vec3::Y * b.height * 0.5).distance(view.eye)
    };
    spots.sort_by(|&a, &b| dist(a).total_cmp(&dist(b)).then(a.cmp(&b)));
    plan.over += spots.len().saturating_sub(MAX_FLAMES);
    spots.truncate(MAX_FLAMES);
    let share: Vec<f32> = spots.iter().map(|&i| screen_share(&flames[i], &view)).collect();
    let mut steps: Vec<f32> = share.iter().map(|&s| (FLAME_BUDGET / s.max(1e-6)).clamp(MIN_STEPS, MAX_STEPS)).collect();
    // Over the frame's budget: shrink every flame's steps, then drop the farthest.
    let load = |steps: &[f32], n: usize| steps.iter().zip(&share).take(n).map(|(k, s)| k * s).sum::<f32>();
    let mut n = spots.len();
    let total = load(&steps, n);
    if total > FRAME_BUDGET {
        let k = FRAME_BUDGET / total;
        for s in &mut steps {
            *s = (*s * k).max(MIN_STEPS);
        }
    }
    while n > 0 && load(&steps, n) > FRAME_BUDGET {
        n -= 1;
    }
    plan.over += spots.len() - n;
    for (j, &i) in spots.iter().take(n).enumerate() {
        plan.steps[i] = Some(steps[j].round());
        plan.shown += 1;
    }
    plan.load = load(&steps, n);
    plan
}
/// Brightness of the summed light.
const BRIGHTNESS: f32 = 1.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, ShaderType)]
pub struct FireParams {
    /// xyz: the flame's base in Bevy world space; w: its height.
    pub base: Vec4,
    /// x: radius at the base, y: brightness, z: seed, w: steps.
    pub shape: Vec4,
    /// x: noise frequency across, y: density gain, z: edge (the body's share of the
    /// density; less gives ragged tongues), w: swirl.
    pub look: Vec4,
}

/// The look's default ([`FireParams::look`]).
const LOOK: Vec4 = Vec4::new(6.0, 1.5, 0.9, 0.8);

/// Test aid: `NVF_FIRE_VOLUME=radius,height,brightness,frequency,gain,edge,swirl`
/// overrides the small fire's size and every flame's look (each field optional).
fn tuned() -> Option<[Option<f32>; 7]> {
    let v = std::env::var("NVF_FIRE_VOLUME").ok()?;
    let mut out = [None; 7];
    for (slot, f) in out.iter_mut().zip(v.split(',')) {
        *slot = f.trim().parse().ok();
    }
    Some(out)
}

#[derive(Asset, TypePath, AsBindGroup, Debug, Clone)]
pub struct FireVolumeMaterial {
    #[uniform(0)]
    pub params: FireParams,
}

impl Material for FireVolumeMaterial {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvforever/fire_volume.wesl".into()
    }

    fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Add
    }

    fn enable_prepass() -> bool {
        false
    }

    fn enable_shadows() -> bool {
        false
    }

    fn specialize(
        _pipeline: &MaterialPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _layout: &MeshVertexBufferLayoutRef,
        key: MaterialPipelineKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // The box's far side: drawn whether the camera is outside it or in it, once.
        descriptor.primitive.cull_mode = Some(Face::Front);
        // With the scene's depth the shader stops the march itself; the far side would
        // fail the depth test behind a log the flame stands in front of.
        if key.mesh_key.contains(MeshPipelineKey::DEPTH_PREPASS) {
            if let Some(ds) = descriptor.depth_stencil.as_mut() {
                ds.depth_compare = Some(CompareFunction::Always);
                ds.depth_write_enabled = Some(false);
            }
        }
        Ok(())
    }
}

/// A flame drawn for the add-on node `emitter`.
#[derive(Component)]
pub struct FireVolume {
    emitter: Entity,
    radius: f32,
    height: f32,
}

/// On an add-on node: its flame was looked for (a [`FireVolume`] names it when it has one).
#[derive(Component)]
pub struct FlameLooked;

/// Flames drawn, for the log.
#[derive(Resource, Default)]
struct FlamesMade {
    count: usize,
    cube: Option<Handle<Mesh>>,
}

pub struct FireVolumePlugin;

impl Plugin for FireVolumePlugin {
    fn build(&self, app: &mut App) {
        embedded_asset!(app, "fire_volume.wesl");
        app.add_plugins(MaterialPlugin::<FireVolumeMaterial>::default())
            .init_resource::<FlamesMade>()
            .add_systems(Update, spawn_flames)
            .add_systems(PostUpdate, follow_emitters.after(TransformSystems::Propagate));
    }
}

/// Whether the add-on node of ADDN `index` is a fire, and its model path.
fn fire_model(sfx: &Sfx, index: i32) -> Option<String> {
    let path = sfx.forms.addons.get(&index).and_then(|a| a.model.as_deref()).map(fnv_actor::resolve::mesh_path)?;
    crate::fuller_fire::is_fire(&path).then_some(path)
}

/// Makes the flame of each fire add-on node, only while the `fuller-fire` switch is on
/// (switched on later, the nodes not yet looked at get theirs then).
fn spawn_flames(
    mut commands: Commands,
    graphics: Option<Res<crate::graphics::Graphics>>,
    sfx: Option<Res<Sfx>>,
    emitters: Query<(Entity, &AddonEmitter), Without<FlameLooked>>,
    mut made: ResMut<FlamesMade>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FireVolumeMaterial>>,
) {
    let Some(sfx) = sfx else { return };
    if !graphics.as_ref().is_some_and(|g| g.fuller_fire) {
        return;
    }
    for (e, em) in &emitters {
        let Some(path) = fire_model(&sfx, em.index) else {
            commands.entity(e).insert(FlameLooked);
            continue;
        };
        let (mut radius, mut height) = flame_size(&path);
        let mut brightness = BRIGHTNESS;
        let mut look = LOOK;
        if let Some(t) = tuned() {
            radius = t[0].unwrap_or(radius);
            height = t[1].unwrap_or(height);
            brightness = t[2].unwrap_or(brightness);
            look = Vec4::new(t[3].unwrap_or(look.x), t[4].unwrap_or(look.y), t[5].unwrap_or(look.z), t[6].unwrap_or(look.w));
        }
        let cube = made.cube.get_or_insert_with(|| meshes.add(Cuboid::new(1.0, 1.0, 1.0))).clone();
        let seed = (e.to_bits() % 997) as f32 * 0.6180339;
        let material = materials.add(FireVolumeMaterial { params: FireParams { base: Vec4::ZERO, shape: Vec4::new(radius, brightness, seed, MIN_STEPS), look } });
        commands.spawn((Mesh3d(cube), MeshMaterial3d(material), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, FireVolume { emitter: e, radius, height }));
        commands.entity(e).insert(FlameLooked);
        made.count += 1;
        if made.count.is_power_of_two() {
            info!("fire volume: {} flame(s) made ({path}: {radius} across the base, {height} high); drawn as the frame's plan allows", made.count);
        }
    }
}

/// The most the plan drew over a stretch of frames, for the log.
#[derive(Default)]
struct PlanSeen {
    since: f32,
    shown: usize,
    stacked: usize,
    over: usize,
    load: f32,
}

/// Each flame stands on its add-on node (the node's place and scale, upright whatever
/// the node's turn: flames rise), shows while the node shows, the switch is on and the
/// frame's plan draws it ([`plan_flames`]), and goes with the node.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn follow_emitters(
    mut commands: Commands,
    graphics: Option<Res<crate::graphics::Graphics>>,
    time: Res<Time>,
    camera: Query<(&GlobalTransform, &Projection), crate::fly::WorldCamera>,
    nodes: Query<(&GlobalTransform, Option<&InheritedVisibility>), With<AddonEmitter>>,
    mut flames: Query<(Entity, &FireVolume, &mut Transform, &mut Visibility, &MeshMaterial3d<FireVolumeMaterial>)>,
    mut materials: ResMut<Assets<FireVolumeMaterial>>,
    mut seen: Local<PlanSeen>,
) {
    let on = graphics.as_ref().is_some_and(|g| g.fuller_fire);
    // Where each flame would burn this frame, and which could show.
    let mut lit: Vec<(Entity, Transform, Vec4, f32)> = Vec::new();
    let mut boxes: Vec<FlameBox> = Vec::new();
    for (e, f, _, mut v, _) in &mut flames {
        let Ok((g, shown)) = nodes.get(f.emitter) else {
            commands.entity(e).despawn();
            continue;
        };
        if !(on && shown.is_none_or(|s| s.get())) {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
            continue;
        }
        let (scale, _, at) = g.to_scale_rotation_translation();
        let (place, params, r) = flame_place(at, scale, f.radius, f.height);
        boxes.push(FlameBox { base: params.truncate(), radius: r, height: params.w });
        lit.push((e, place, params, r));
    }
    let view = camera.iter().next().and_then(|(g, p)| match p {
        Projection::Perspective(p) => Some(View { eye: g.translation(), fov: p.fov, aspect: p.aspect_ratio }),
        _ => None,
    });
    let plan = plan_flames(&boxes, view);
    for ((e, place, params, r), steps) in lit.into_iter().zip(&plan.steps) {
        let Ok((_, _, mut t, mut v, m)) = flames.get_mut(e) else { continue };
        let Some(steps) = *steps else {
            if *v != Visibility::Hidden {
                *v = Visibility::Hidden;
            }
            continue;
        };
        if *t != place {
            *t = place;
        }
        if materials.get(&m.0).is_some_and(|mat| mat.params.base != params || mat.params.shape.x != r || mat.params.shape.w != steps) {
            if let Some(mut mat) = materials.get_mut(&m.0) {
                mat.params.base = params;
                mat.params.shape.x = r;
                mat.params.shape.w = steps;
            }
        }
        if *v != Visibility::Inherited {
            *v = Visibility::Inherited;
        }
    }
    seen.shown = seen.shown.max(plan.shown);
    seen.stacked = seen.stacked.max(plan.stacked);
    seen.over = seen.over.max(plan.over);
    seen.load = seen.load.max(plan.load);
    seen.since += time.delta_secs();
    if seen.since >= 10.0 {
        if seen.shown + seen.stacked + seen.over > 0 {
            info!(
                "fire volume: at most {} flame(s) drawn in the last 10 s ({} stacked in a drawn flame's spot, {} over the budget); steps x screen share at most {:.2} of {FRAME_BUDGET}",
                seen.shown, seen.stacked, seen.over, seen.load
            );
        }
        *seen = PlanSeen::default();
    }
}

/// A flame's box and its shader's base for an add-on node at `at` with world `scale`:
/// the box (a unit cube scaled) stands upright on the node, its base [`SINK`] below it
/// so the flame rises out of the fuel, its middle over the node; and the radius at the
/// node's scale.
fn flame_place(at: Vec3, scale: Vec3, radius: f32, height: f32) -> (Transform, Vec4, f32) {
    let s = scale.abs().max_element().clamp(0.1, 10.0);
    let (r, h) = (radius * s, height * s);
    let base = at - Vec3::Y * SINK * s;
    let place = Transform::from_translation(base + Vec3::Y * h * 0.5).with_scale(Vec3::new(2.0 * r, h, 2.0 * r) * 1.02);
    (place, Vec4::new(base.x, base.y, base.z, h), r)
}

/// Whether ADDN `index` burns a volumetric flame (its own flame squares then stay
/// unborn).
pub fn burns(sfx: Option<&Sfx>, index: i32) -> bool {
    sfx.is_some_and(|s| fire_model(s, index).is_some())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn medium_fires_burn_larger_flames() {
        let small = flame_size(r"meshes\mps\mpsfiresmall01.nif");
        let med = flame_size(r"meshes\mps\mpsfiremed01.nif");
        assert!(med.0 > small.0 && med.1 > small.1);
        assert!(small.1 > 2.0 * small.0, "taller than wide");
    }

    #[test]
    fn the_flame_stands_on_its_fire_and_rises_from_it() {
        // A campfire's add-on node somewhere in the world, at the node's scale 1 and 2.
        let at = Vec3::new(-6352.0, 637.0, 2004.0);
        let (r0, h0) = flame_size(r"meshes\mps\mpsfiresmall01.nif");
        for k in [1.0, 2.0] {
            let (place, base, r) = flame_place(at, Vec3::splat(k), r0, h0);
            // Centred over the node across the ground: the flame is in the fire's spot.
            assert!((place.translation.x - at.x).abs() < 1e-3 && (place.translation.z - at.z).abs() < 1e-3, "{place:?}");
            // Its base just under the node, the box from there to the tip.
            assert!((base.y - (at.y - SINK * k)).abs() < 1e-3);
            assert!((base.w - h0 * k).abs() < 1e-3 && (r - r0 * k).abs() < 1e-3);
            let (bottom, top) = (place.translation.y - place.scale.y * 0.5, place.translation.y + place.scale.y * 0.5);
            assert!(bottom <= base.y && top >= base.y + base.w, "the box holds the whole flame");
            assert!(place.scale.x >= 2.0 * r && place.scale.z >= 2.0 * r, "and its width");
        }
        // A node turned upside down or mirrored still burns upright at its size.
        let (place, base, _) = flame_place(at, Vec3::new(-1.0, 1.0, -1.0), r0, h0);
        assert!(base.y < at.y && place.translation.y > at.y);
    }

    fn campfire(x: f32) -> FlameBox {
        FlameBox { base: Vec3::new(x, 0.0, 0.0), radius: 30.0, height: 100.0 }
    }

    fn eye_at(x: f32, z: f32) -> Option<View> {
        Some(View { eye: Vec3::new(x, 60.0, z), fov: 75f32.to_radians(), aspect: 16.0 / 9.0 })
    }

    fn drawn(plan: &Plan) -> Vec<f32> {
        plan.steps.iter().flatten().copied().collect()
    }

    #[test]
    fn five_flames_stacked_in_one_spot_draw_once() {
        // The Burning Campfire of 2026-11-14: five fire nodes on one spot, seen from 110 units.
        let mut fires: Vec<FlameBox> = (0..5).map(|i| campfire(i as f32 * 2.0)).collect();
        fires[3].radius = 48.0;
        fires[3].height = 165.0;
        let plan = plan_flames(&fires, eye_at(0.0, 110.0));
        assert_eq!((plan.shown, plan.stacked, plan.over), (1, 4, 0), "{plan:?}");
        assert!(plan.steps[3].is_some(), "the largest burns for the spot");
        // A second campfire across the camp burns its own.
        fires.push(campfire(600.0));
        assert_eq!(plan_flames(&fires, eye_at(0.0, 110.0)).shown, 2);
    }

    #[test]
    fn a_flame_at_reach_marches_fewer_steps_than_one_across_the_room() {
        let fire = [campfire(0.0)];
        let near = drawn(&plan_flames(&fire, eye_at(0.0, 80.0)))[0];
        let far = drawn(&plan_flames(&fire, eye_at(0.0, 2000.0)))[0];
        assert!(near < far, "{near} {far}");
        assert_eq!(far, MAX_STEPS);
        let at_reach = plan_flames(&fire, eye_at(0.0, 80.0));
        assert!(at_reach.load <= FLAME_BUDGET + 1e-3, "{at_reach:?}");
    }

    #[test]
    fn a_camera_inside_the_flame_marches_the_fewest_steps() {
        let plan = plan_flames(&[campfire(0.0)], eye_at(0.0, 0.0));
        assert_eq!(drawn(&plan), vec![MIN_STEPS]);
        assert!((plan.load - MIN_STEPS).abs() < 1e-3);
    }

    #[test]
    fn the_frame_budget_holds_however_many_fires_crowd_the_view() {
        for (count, gap, eye) in [(12, 70.0, 60.0), (12, 300.0, 200.0), (40, 10.0, 0.0), (3, 0.0, 1.0), (8, 2000.0, 5.0)] {
            let fires: Vec<FlameBox> = (0..count).map(|i| campfire(i as f32 * gap)).collect();
            for z in [eye, eye + 100.0, eye + 1000.0] {
                let plan = plan_flames(&fires, eye_at(0.0, z));
                assert!(plan.shown >= 1 && plan.shown <= MAX_FLAMES, "{plan:?}");
                assert!(plan.load <= FRAME_BUDGET + 1e-3, "{count} fires {gap} apart from {z}: {plan:?}");
                assert_eq!(plan.shown + plan.stacked + plan.over, count, "every flame is counted once");
                assert!(drawn(&plan).iter().all(|&s| (MIN_STEPS..=MAX_STEPS).contains(&s)));
            }
        }
    }

    #[test]
    fn bad_numbers_draw_nothing_or_the_fewest_steps() {
        let bad = [
            FlameBox { base: Vec3::NAN, ..campfire(0.0) },
            FlameBox { radius: f32::INFINITY, ..campfire(0.0) },
            FlameBox { height: 0.0, ..campfire(0.0) },
            FlameBox { radius: -5.0, ..campfire(0.0) },
        ];
        let plan = plan_flames(&bad, eye_at(0.0, 100.0));
        assert_eq!(plan.shown, 0, "{plan:?}");
        // A camera with no sane numbers: one flame, the fewest steps.
        for view in [None, Some(View { eye: Vec3::NAN, fov: 1.0, aspect: 1.0 }), Some(View { eye: Vec3::ZERO, fov: f32::NAN, aspect: 0.0 })] {
            let fires: Vec<FlameBox> = (0..6).map(|i| campfire(i as f32 * 500.0)).collect();
            let plan = plan_flames(&fires, view);
            assert_eq!(drawn(&plan), vec![MIN_STEPS], "{view:?}");
            assert!(plan.load <= FRAME_BUDGET);
        }
        // A zero field of view or aspect is clamped, not divided by.
        let s = screen_share(&campfire(0.0), &View { eye: Vec3::new(0.0, 60.0, 200.0), fov: 0.0, aspect: 0.0 });
        assert!((0.0..=1.0).contains(&s));
    }

    #[test]
    fn the_shader_bounds_are_the_plans() {
        let wesl = include_str!("fire_volume.wesl");
        assert!(wesl.contains(&format!("const MIN_STEPS: i32 = {};", MIN_STEPS as i32)));
        assert!(wesl.contains(&format!("const MAX_STEPS: i32 = {};", MAX_STEPS as i32)));
        assert!(wesl.contains("clamp(select(MIN_STEPS, i32(wanted)"), "the shader clamps the uniform's steps");
    }
}
