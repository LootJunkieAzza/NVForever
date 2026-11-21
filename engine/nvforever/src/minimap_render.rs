//! Modern Minimap's map picture, as its plugin (ModernMinimap.dll) draws it for the HUD's
//! `ModernMinimap` window (docs/spec-notes/menu-xml.md §11; the tiles' side is
//! `nvf_play::tiles::modern_minimap`). Without it the window's `MapRender` image stayed the
//! file's placeholder (`Interface\Shared\solid.dds`), a plain square under the icons.
//!
//! A camera of its own renders into a texture [design: as `crate::reflection`], active
//! only in the frames the window asks for a render: orthographic, straight down over the
//! player, north up, `_FrustumSize` game units each way, the world layer only (not the
//! player's body or arms, not the water's own layer), no fog, black where nothing is. The
//! file's classic tint (`systemcolor`, `brightness`) is applied by the tiles themselves.
//! The picture is 1024 texels square [hypothesis: the plugin's INI default,
//! `NVSE\Plugins\ModernMinimap.ini` `textureSize=1024`].

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, VisibilitySystems};
use bevy::camera::{RenderTarget, ScalingMode};
use bevy::light::SimulationLightSystems;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::transform::TransformSystems;
use nvf_play::tiles::modern_minimap::{rendered, request, Request};

/// The runtime picture's name (`nvf_play::tiles::runtime_images`).
const IMAGE: &str = "modern_minimap";
/// The picture's size, texels [hypothesis: see the module notes].
const SIZE: u32 = 1024;
/// How far below the camera it draws beyond its height over the player, game units
/// [design: the file's camera stands 10,000 over the player].
const DEPTH: f32 = 20000.0;

#[derive(Component)]
struct MinimapCamera;

#[derive(Resource)]
struct MinimapImage(Handle<Image>);

/// The camera's place and view for a request: over the player, looking straight down,
/// north (game +y, Bevy -z) up, the frustum's width each way.
fn camera_for(r: &Request) -> (Transform, OrthographicProjection) {
    let [x, y, z] = r.centre;
    let t = Transform::from_xyz(x, z + r.offset, -y).looking_to(Vec3::NEG_Y, Vec3::NEG_Z);
    let p = OrthographicProjection {
        scaling_mode: ScalingMode::Fixed { width: 2.0 * r.half, height: 2.0 * r.half },
        near: 0.0,
        far: r.offset.abs() + DEPTH,
        ..OrthographicProjection::default_3d()
    };
    (t, p)
}

pub struct MinimapRenderPlugin;

impl Plugin for MinimapRenderPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, setup).add_systems(
            PostUpdate,
            update
                .after(TransformSystems::Propagate)
                .before(VisibilitySystems::UpdateFrusta)
                // An active camera needs its sun cascades (as `crate::reflection`).
                .before(SimulationLightSystems::UpdateDirectionalLightCascades),
        );
    }
}

fn setup(mut commands: Commands, mut images: ResMut<Assets<Image>>) {
    let mut image = Image::new_uninit(
        Extent3d { width: SIZE, height: SIZE, depth_or_array_layers: 1 },
        TextureDimension::D2,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::MAIN_WORLD | RenderAssetUsages::RENDER_WORLD,
    );
    image.texture_descriptor.usage |= TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = images.add(image);
    commands.spawn((
        Name::new("Modern Minimap camera"),
        MinimapCamera,
        Camera3d::default(),
        Camera { order: -2, is_active: false, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(image.clone().into()),
        Msaa::Off,
        Projection::Orthographic(OrthographicProjection::default_3d()),
        RenderLayers::layer(0),
    ));
    commands.insert_resource(MinimapImage(image));
}

/// Render when the window asks (`request`), then write the render's centre and the
/// picture's name to the window (`rendered`); otherwise the camera stays off.
fn update(
    mut script: Option<ResMut<nvf_play::script::ScriptState>>,
    image: Option<Res<MinimapImage>>,
    runtime: Option<ResMut<nvf_play::tiles::runtime_images::RuntimeImages>>,
    mut camera: Query<(&mut Camera, &mut Transform, &mut GlobalTransform, &mut Projection), With<MinimapCamera>>,
) {
    let Ok((mut cam, mut t, mut gt, mut proj)) = camera.single_mut() else { return };
    let rt = script.as_deref_mut().and_then(|s| s.rt.as_mut());
    let (Some(rt), Some(image), Some(mut runtime)) = (rt, image, runtime) else {
        cam.is_active = false;
        return;
    };
    let Some(r) = request(&rt.game.ui_tree) else {
        cam.is_active = false;
        return;
    };
    let (place, view) = camera_for(&r);
    *t = place;
    *gt = GlobalTransform::from(place);
    *proj = Projection::Orthographic(view);
    if !cam.is_active {
        info!("modern minimap: map picture rendered round ({:.0}, {:.0}), {:.0} units each way", r.centre[0], r.centre[1], r.half);
    }
    cam.is_active = true;
    let name = runtime.set(IMAGE, image.0.clone(), Vec2::splat(SIZE as f32));
    rendered(&mut rt.game.ui_tree, &r, &name);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_camera_looks_down_with_north_up_over_the_whole_frustum() {
        let r = Request { centre: [1000.0, -500.0, 300.0], half: 2048.0, offset: 10000.0 };
        let (t, p) = camera_for(&r);
        assert_eq!(t.translation, Vec3::new(1000.0, 10300.0, 500.0));
        assert!(t.forward().as_vec3().abs_diff_eq(Vec3::NEG_Y, 1e-5));
        assert!(t.up().as_vec3().abs_diff_eq(Vec3::NEG_Z, 1e-5), "north (game +y) is Bevy -z");
        assert!(matches!(p.scaling_mode, ScalingMode::Fixed { width, height } if width == 4096.0 && height == 4096.0));
        assert!(p.far >= 10300.0);
    }
}
