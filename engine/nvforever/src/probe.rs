//! The remaster's local light probe (docs/design/graphics-remaster.md §2 "Light probe"):
//! six small cameras at the main camera's position, one per cube face, render the world
//! around it, off the screen too, one face every [`EVERY`] frames. gi.rs copies the faces into a 2D
//! array each frame and (1) lights surfaces by the probe's light from each direction
//! (the Strip's signs behind the camera still light the street) and (2) gives the faces
//! to the lighting as the reflection environment of environment-mapped shapes
//! (nif_material.wesl `env_reflection`). An engineering choice, not the original game's.
//!
//! Only while the main camera carries [`crate::gi::GlobalIllumination`].

use crate::fly::FlyCam;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::RenderTarget;
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::extract_resource::{ExtractResource, ExtractResourcePlugin};
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::asset::RenderAssetUsages;
use bevy::transform::TransformSystems;

/// A face is drawn every this many frames (the whole probe every 6 x this).
const EVERY: usize = 4;

/// The side of a face, in texels.
pub const FACE_SIZE: u32 = 128;

/// The faces' looking direction and up, in the order of the array's layers (gi.wesl and
/// nif_material.wesl `probe_uv` use the same table).
pub const FACES: [(Vec3, Vec3); 6] = [
    (Vec3::X, Vec3::Y),
    (Vec3::NEG_X, Vec3::Y),
    (Vec3::Y, Vec3::NEG_Z),
    (Vec3::NEG_Y, Vec3::Z),
    (Vec3::Z, Vec3::Y),
    (Vec3::NEG_Z, Vec3::Y),
];

/// The faces' images (HDR, linear, after exposure).
#[derive(Resource, ExtractResource, Clone)]
#[extract_app(bevy::render::RenderApp)]
pub struct ProbeImages {
    pub faces: [Handle<Image>; 6],
}

#[derive(Component)]
struct ProbeFace(usize);

pub struct ProbePlugin;

impl Plugin for ProbePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(ExtractResourcePlugin::<ProbeImages>::default())
            .add_systems(Startup, setup)
            .add_systems(
                PostUpdate,
                follow
                    .after(TransformSystems::Propagate)
                    .before(bevy::camera::visibility::VisibilitySystems::UpdateFrusta)
                    .before(bevy::light::SimulationLightSystems::UpdateDirectionalLightCascades),
            );
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let faces = std::array::from_fn(|i| {
        let mut image = Image::new_uninit(
            Extent3d { width: FACE_SIZE, height: FACE_SIZE, depth_or_array_layers: 1 },
            TextureDimension::D2,
            TextureFormat::Rgba16Float,
            RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
        );
        image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_SRC | TextureUsages::RENDER_ATTACHMENT;
        let image = images.add(image);
        commands.spawn((
            Name::new(format!("Light probe face {i}")),
            ProbeFace(i),
            Camera3d::default(),
            Camera { order: -2, is_active: false, ..default() },
            RenderTarget::Image(image.clone().into()),
            bevy::camera::Hdr,
            Tonemapping::Linear,
            Msaa::Off,
            Projection::Perspective(PerspectiveProjection { fov: std::f32::consts::FRAC_PI_2, aspect_ratio: 1.0, near: 8.0, ..default() }),
            crate::sky::initial_fog(),
            RenderLayers::from_layers(&[0]),
            Transform::default(),
        ));
        image
    });
    commands.insert_resource(ProbeImages { faces });
}

/// The main camera, for [`follow`].
type MainCamera<'w, 's> = Query<'w, 's, (&'static GlobalTransform, &'static DistanceFog, Has<crate::gi::GlobalIllumination>), (With<FlyCam>, Without<ProbeFace>)>;

/// One face every [`EVERY`] frames, at the main camera, while it has global illumination; the fog as the
/// main camera's.
fn follow(
    main: MainCamera,
    mut faces: Query<(&ProbeFace, &mut Camera, &mut Transform, &mut GlobalTransform, &mut DistanceFog), Without<FlyCam>>,
    mut frame: Local<usize>,
) {
    let Ok((at, fog, gi)) = main.single() else {
        for (_, mut cam, ..) in &mut faces {
            cam.is_active = false;
        }
        return;
    };
    *frame = frame.wrapping_add(1);
    let pos = at.translation();
    for (face, mut cam, mut t, mut gt, mut f) in &mut faces {
        // One face every EVERY-th frame: each probe view costs a view's culling, queueing
        // and its own sun cascades on the render thread (about 2 ms a view at Goodsprings).
        let on = gi && frame.is_multiple_of(EVERY) && face.0 == (*frame / EVERY) % 6;
        if cam.is_active != on {
            cam.is_active = on;
        }
        if on {
            let (dir, up) = FACES[face.0];
            *t = Transform::from_translation(pos).looking_to(dir, up);
            *gt = GlobalTransform::from(*t);
            *f = fog.clone();
        }
    }
}
