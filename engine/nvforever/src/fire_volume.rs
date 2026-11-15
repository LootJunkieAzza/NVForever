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
/// Ray steps through a flame.
const STEPS: f32 = 40.0;
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
            .add_systems(PostUpdate, follow_emitters.after(TransformSystems::Propagate).after(nvf_play::photo_mode::HeldViewSet));
    }
}

/// Whether the add-on node of ADDN `index` is a fire, and its model path.
fn fire_model(sfx: &Sfx, index: i32) -> Option<String> {
    let path = sfx.forms.addons.get(&index).and_then(|a| a.model.as_deref()).map(fnv_actor::resolve::mesh_path)?;
    crate::fuller_fire::is_fire(&path).then_some(path)
}

fn spawn_flames(
    mut commands: Commands,
    sfx: Option<Res<Sfx>>,
    emitters: Query<(Entity, &AddonEmitter), Without<FlameLooked>>,
    mut made: ResMut<FlamesMade>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<FireVolumeMaterial>>,
) {
    let Some(sfx) = sfx else { return };
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
        let material = materials.add(FireVolumeMaterial { params: FireParams { base: Vec4::ZERO, shape: Vec4::new(radius, brightness, seed, STEPS), look } });
        commands.spawn((Mesh3d(cube), MeshMaterial3d(material), Transform::IDENTITY, Visibility::Hidden, NotShadowCaster, NoFrustumCulling, FireVolume { emitter: e, radius, height }));
        commands.entity(e).insert(FlameLooked);
        made.count += 1;
        if made.count.is_power_of_two() {
            info!("fire volume: {} flame(s) drawn ({path}: {radius} across the base, {height} high)", made.count);
        }
    }
}

/// Each flame stands on its add-on node (the node's place and scale, upright whatever
/// the node's turn: flames rise), shows while the node shows and the switch is on, and
/// goes with the node.
fn follow_emitters(
    mut commands: Commands,
    graphics: Option<Res<crate::graphics::Graphics>>,
    nodes: Query<(&GlobalTransform, Option<&InheritedVisibility>), With<AddonEmitter>>,
    mut flames: Query<(Entity, &FireVolume, &mut Transform, &mut Visibility, &MeshMaterial3d<FireVolumeMaterial>)>,
    mut materials: ResMut<Assets<FireVolumeMaterial>>,
) {
    let on = graphics.as_ref().is_none_or(|g| g.fuller_fire);
    for (e, f, mut t, mut v, m) in &mut flames {
        let Ok((g, shown)) = nodes.get(f.emitter) else {
            commands.entity(e).despawn();
            continue;
        };
        let want = if on && shown.is_none_or(|s| s.get()) { Visibility::Inherited } else { Visibility::Hidden };
        if *v != want {
            *v = want;
        }
        if want == Visibility::Hidden {
            continue;
        }
        let (scale, _, at) = g.to_scale_rotation_translation();
        let (place, params, r) = flame_place(at, scale, f.radius, f.height);
        if *t != place {
            *t = place;
        }
        if materials.get(&m.0).is_some_and(|mat| mat.params.base != params || mat.params.shape.x != r) {
            if let Some(mut mat) = materials.get_mut(&m.0) {
                mat.params.base = params;
                mat.params.shape.x = r;
            }
        }
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
}
