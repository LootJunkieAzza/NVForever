//! World reflections in water: a second camera, the main camera mirrored in the water
//! plane nearest the camera, renders the scene above that plane (an oblique near clip
//! plane cuts away everything below it) into a texture; the water shader samples it at
//! the fragment's screen position, offset by the wave normal. The same technique as
//! Bevy's `mirror` example. Water itself lives on [`WATER_LAYER`], which the reflection
//! camera does not draw.
//!
//! One plane at a time [hypothesis: enough for the Mojave, where lakes do not overlap
//! in height on screen]: water at another height reflects only the sky (water.wgsl).

use crate::water::{DistantWater, WaterCache, WaterMaterial};
use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::camera::{Hdr, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::math::reflection_matrix;
use bevy::pbr::DistanceFog;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::transform::TransformSystems;
use bevy::window::{PrimaryWindow, WindowResized};
use fnv_world::cell_of;

/// Render layer of water surfaces: seen by the main camera, not by the reflection camera
/// (which would otherwise sample the texture it renders into).
pub const WATER_LAYER: usize = 1;

/// The reflection texture's size relative to the window.
const SCALE: u32 = 2;

/// Cells (in grid steps from the camera's cell) searched for the water to reflect in.
const SEARCH: i32 = 2;

/// Below the water plane by this much, so the water surface itself is clipped away.
const CLIP_MARGIN: f32 = 1.0;

#[derive(Component)]
pub struct ReflectionCamera;

#[derive(Resource)]
pub struct Reflection {
    pub image: Handle<Image>,
}

pub struct ReflectionPlugin;

impl Plugin for ReflectionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup_reflection).add_systems(
            PostUpdate,
            (resize_reflection, update_reflection)
                .chain()
                .after(TransformSystems::Propagate)
                .before(VisibilitySystems::UpdateFrusta),
        );
    }
}

fn texture_size(window: &Window) -> Extent3d {
    Extent3d {
        width: (window.physical_width() / SCALE).max(1),
        height: (window.physical_height() / SCALE).max(1),
        depth_or_array_layers: 1,
    }
}

fn setup_reflection(mut commands: Commands, mut images: ResMut<Assets<Image>>, window: Query<&Window, With<PrimaryWindow>>) {
    let size = window.single().map(texture_size).unwrap_or(Extent3d { width: 640, height: 400, depth_or_array_layers: 1 });
    // Linear HDR, not tonemapped: the water shader mixes it with lit colours and
    // tonemaps the result.
    let mut image = Image::new_uninit(
        size,
        TextureDimension::D2,
        TextureFormat::Rgba16Float,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    commands.spawn((
        Name::new("Reflection camera"),
        ReflectionCamera,
        Camera3d::default(),
        Camera { order: -1, invert_culling: true, is_active: false, ..default() },
        RenderTarget::Image(image.clone().into()),
        Hdr,
        Tonemapping::None,
        Msaa::Off,
        Projection::Perspective(PerspectiveProjection::default()),
        crate::sky::initial_fog(),
        RenderLayers::layer(0),
    ));
    commands.insert_resource(Reflection { image });
}

fn resize_reflection(
    mut resized: MessageReader<WindowResized>,
    window: Query<&Window, With<PrimaryWindow>>,
    reflection: Option<Res<Reflection>>,
    mut images: ResMut<Assets<Image>>,
) {
    if resized.read().last().is_none() {
        return;
    }
    let (Ok(window), Some(r)) = (window.single(), reflection) else { return };
    let size = texture_size(window);
    if let Some(mut img) = images.get_mut(&r.image) {
        if img.texture_descriptor.size != size {
            img.resize(size);
        }
    }
}

/// The water surface (Bevy y) to reflect in: the nearest cell around the camera with
/// visible water below the camera.
fn reflection_height(world: &WorldData, dw: &mut DistantWater, cam_game: Vec3) -> Option<f32> {
    let c = cell_of(cam_game.x, cam_game.y);
    (0..=SEARCH).find_map(|r| {
        (-r..=r)
            .flat_map(|dx| (-r..=r).map(move |dy| (dx, dy)))
            .filter(|&(dx, dy)| dx.abs().max(dy.abs()) == r)
            .filter_map(|(dx, dy)| dw.surface(world, (c.0 + dx, c.1 + dy)))
            .map(|(h, _)| h)
            .filter(|&h| h < cam_game.z)
            .min_by(|a, b| (cam_game.z - a).total_cmp(&(cam_game.z - b)))
    })
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_reflection(
    world: Res<WorldData>,
    streamer: Res<Streamer>,
    mut dw: ResMut<DistantWater>,
    main: Query<(&GlobalTransform, &Projection, &DistanceFog), (With<Camera3d>, Without<ReflectionCamera>)>,
    mut refl: Query<
        (&mut Camera, &mut Transform, &mut GlobalTransform, &mut Projection, &mut DistanceFog),
        With<ReflectionCamera>,
    >,
    cache: Res<WaterCache>,
    mut materials: ResMut<Assets<WaterMaterial>>,
) {
    let (Ok((main_gt, main_proj, main_fog)), Ok((mut camera, mut t, mut gt, mut proj, mut fog))) = (main.single(), refl.single_mut())
    else {
        return;
    };
    let cam = main_gt.translation();
    let height = match (streamer.interior, main_proj) {
        (None, Projection::Perspective(_)) => reflection_height(&world, &mut dw, bevy_to_game(cam)),
        _ => None,
    };
    let plane = match height {
        Some(h) => Vec4::new(h, 1.0, 0.0, 0.0),
        None => Vec4::ZERO,
    };
    cache.set_reflection_plane(&mut materials, plane);
    if camera.is_active != height.is_some() {
        camera.is_active = height.is_some();
    }
    let (Some(h), Projection::Perspective(p)) = (height, main_proj) else { return };

    // Mirror the camera in the plane y = h.
    let mirror = Mat4::from_translation(Vec3::Y * h)
        * Mat4::from_mat3a(reflection_matrix(Vec3::Y))
        * Mat4::from_translation(Vec3::NEG_Y * h);
    *t = Transform::from_matrix(mirror * main_gt.to_matrix());
    *gt = GlobalTransform::from(*t);
    // Keep only what lies above the plane: in the main camera's view space the mirrored
    // scene must lie below it (see Bevy's mirror example).
    let normal = (main_gt.compute_transform().rotation.inverse() * Vec3::NEG_Y).normalize();
    *proj = Projection::Perspective(PerspectiveProjection {
        near_clip_plane: normal.extend(h - CLIP_MARGIN - cam.y),
        ..p.clone()
    });
    *fog = main_fog.clone();
}
