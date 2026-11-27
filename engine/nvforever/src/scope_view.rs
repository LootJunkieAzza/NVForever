//! A weapon's scope while it is up (`nvf_play::aim_zoom::Scope`,
//! docs/spec-notes/weapon-scopes.md): its picture, its night vision and its sway.
//!
//! - **The picture** (§3): the weapon's MOD3 model (`interface\HUD\*.nif`) drawn by an
//!   orthographic camera of its own looking along the model's +Y, [`VIEW_HEIGHT`] model
//!   units high [hypothesis: the black mask's rectangle, 1480 × 920, is the screen at
//!   16:10], on a render layer of its own, into a picture shown full-screen under the HUD
//!   [design]. Black bars stand beyond the mask's sides on wider screens [design]. The
//!   materials are the model's own.
//! - **The picture's memory** (#4781): the picture does not change while the scope is up
//!   (the sway turns the main view), so its camera renders it once and is then despawned
//!   with the shapes it drew: it stays on until the render world has drawn every shape of
//!   it ([`PictureCheck`]: mesh, material and pipeline ready for its view) on
//!   [`PICTURE_READY_FRAMES`] frames in a row.
//!   A camera that is on gets its own window-sized view targets; at 4x MSAA, 3440x1440,
//!   they were about 217 MB beside the picture's 20 MB [observed: `NVF_PLAY_TEST_SAMPLE`
//!   allocator labels, upstream main 6c393607a, #4781]. It renders without MSAA or
//!   transmission. The picture is kept, hidden, when the scope goes down and shown again
//!   at the next scope-up with the same model and window size; another model, the 3D
//!   lens or another window size makes it anew.
//! - **Night vision** (§4): a night-vision scope holds `NightScopeISFX` on the screen at its
//!   first keys in an interior or at night [documented: GECK wiki "Weapons"; hypothesis:
//!   the record, and night as sunset's end to sunrise's start].
//! - **Sway** (§5): `scopewobble.nif`'s 5 s loop of the `Camera1st` node turns the view,
//!   scaled so its widest point is half of PSp × 5.955° [documented: GECK wiki "Gun Spread
//!   Formula (New Vegas)"] × `fGunWobbleMultScope`, × `fNonAttackGunWobbleMult` while the
//!   player is not attacking [documented: the same page; hypothesis: the curve's meaning
//!   and the axes, X up and down, Z sideways]. The shots follow the view.
//! - **3D scopes** (`Scope::lens_3d`, a test version; docs/design/scope-3d-lens.md): the
//!   picture is not shown and the arms stay. A second world camera, under the player's
//!   camera, renders the zoomed world into a square image; a disc on the weapon's
//!   eyepiece (`scope_lens::LensMount`) shows it, with the MOD3 reticle drawn over it
//!   without its black mask. The camera's angle keeps the full zoom's magnification for
//!   the lens's size on screen [design].

use crate::imagespace::ScreenModifiers;
use crate::sky::SkyState;
use crate::world::WorldData;
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::{RenderLayers, ViewVisibility};
use bevy::camera::{ImageRenderTarget, RenderTarget, ScalingMode};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_nif::anim::TransformTrack;
use nvf_assets::data::GameData;
use nvf_assets::model::ModelCtx;
use nvf_play::aim_zoom::Scope;
use nvf_play::combat::Combat;
use nvf_play::player::{Player, MAX_PITCH};
use nvf_play::PlayerCamera;
use crate::scope_lens::LensMount;
use bevy::asset::UntypedAssetId;
use bevy::pbr::{DistanceFog, PreparedMaterial, RenderMaterialInstances, ScreenSpaceTransmission, SpecializedMaterialPipelineCache};
use bevy::render::erased_render_asset::ErasedRenderAssets;
use bevy::render::material_bind_groups::MaterialBindGroupAllocators;
use bevy::render::mesh::RenderMesh;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::PipelineCache;
use bevy::render::sync_world::MainEntity;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::sync::atomic::{AtomicU32, Ordering::Relaxed};
use std::sync::Arc;

/// The render layer of the scope's picture and its camera.
pub const SCOPE_LAYER: usize = 13;
/// Where the picture is put, far from the world and the other menus' models (Bevy space)
/// [design].
const PLACE: Vec3 = Vec3::new(0.0, -220_000.0, 0.0);
/// Model units across the screen's height [hypothesis, module docs].
pub const VIEW_HEIGHT: f32 = 920.0;
/// The mask's half width: black beyond it [observed: `keepScope01:0` spans ±740].
const MASK_HALF_WIDTH: f32 = 740.0;
/// The night-vision scope's image space modifier, by Editor ID [hypothesis, §4]. No DOBJ
/// place names a scope image space (default-objects.md §1), so it stays an Editor ID.
pub const NIGHT_SCOPE_IMAD: &str = "NightScopeISFX";
/// The scope's wobble [observed, §5].
pub const WOBBLE_MODEL: &str = "meshes\\characters\\weaponwobbles\\scopewobble.nif";
/// Spread units to degrees of the sway circle's diameter [documented, §5].
pub const SPREAD_TO_DEG: f32 = 5.955;
/// A shot within this many seconds counts as attacking (full sway) [hypothesis].
const ATTACK_SECONDS: f32 = 0.5;
/// 3D scopes: the mask's inner circle (model units) the reticle is cut to [observed:
/// `keepScope01:0`'s ring, weapon-scopes.md §3].
const LENS_CIRCLE: f32 = 361.0;
/// 3D scopes: the reticle's and the zoomed world's images (pixels, square) [design].
const RETICLE_PX: u32 = 512;
const LENS_WORLD_PX: u32 = 768;
/// 3D scopes: the reticle drawn this far in front of the world disc (model units).
const RETICLE_LIFT: f32 = 0.02;

/// The second world camera of a 3D scope (it takes the sun visibility too:
/// sun_visibility.rs).
#[derive(Component)]
pub(crate) struct LensCamera;

pub struct ScopeViewPlugin;

impl Plugin for ScopeViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync),
                night_vision.after(nvf_play::PlaySet::Sync),
                sway.in_set(nvf_play::PlaySet::Activation).run_if(resource_exists::<Combat>),
                // After the desert haze and the aerial perspective (fog.rs FogWriters): the
                // lens copies the main camera's fog.
                lens_camera.after(show).after(crate::fog::FogWriters),
                settle_picture.after(show),
            ),
        )
        .init_resource::<PictureCheck>()
        // After underwater_fog in PostUpdate so the 3D scope lens receives the current
        // frame's underwater fog or freshly restored weather/interior fog.
        .add_systems(PostUpdate, sync_lens_fog.after(crate::underwater::underwater_fog));
    }

    fn finish(&self, app: &mut App) {
        let Some(check) = app.world().get_resource::<PictureCheck>().cloned() else { return };
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .insert_resource(check)
                .add_systems(ExtractSchedule, extract_picture_check)
                .add_systems(Render, observe_picture.in_set(RenderSystems::Cleanup));
        }
    }
}

/// Render frames in a row with the whole picture drawn before its camera is turned off
/// [engineering choice: the first frame drawn plus one, for a pipeline compiled during it].
pub const PICTURE_READY_FRAMES: u32 = 2;

/// The 2D picture's camera and shapes as the render world checks them (#4781): set by
/// [`show`] while a picture's camera renders, empty otherwise. `ready` counts the render
/// frames in a row on which every shape was drawable for the camera's view; shared by
/// both worlds.
#[derive(Resource, Clone, Default)]
struct PictureCheck {
    camera: Option<MainEntity>,
    /// The camera renders this frame (it is on).
    active: bool,
    shapes: Vec<(MainEntity, AssetId<Mesh>, UntypedAssetId)>,
    ready: Arc<AtomicU32>,
}

/// The shapes the camera sees, only: one outside its view is culled and never drawn (the
/// side bars on a screen of 16:10 or narrower) [observed: 1280x800, the bars never queued,
/// the picture waited for them]. The shapes are on the picture's own layer, so a shape's
/// `ViewVisibility` is its visibility in this camera.
fn extract_picture_check(main: Extract<Res<PictureCheck>>, seen: Extract<Query<&ViewVisibility>>, mut render: ResMut<PictureCheck>) {
    render.camera = main.camera;
    render.active = main.active;
    render.shapes.clear();
    render.shapes.extend(main.shapes.iter().filter(|(e, _, _)| seen.get(e.id()).is_ok_and(|v| v.get())).copied());
}

/// Count the frame as drawn when every shape's mesh and material are on the GPU (the
/// material only prepares once its textures are) and its pipeline for the picture
/// camera's view is compiled [design: as android_render_admission.rs `Observe::flags`].
#[allow(clippy::too_many_arguments)]
fn observe_picture(
    check: Res<PictureCheck>,
    instances: Res<RenderMaterialInstances>,
    materials: Res<ErasedRenderAssets<PreparedMaterial>>,
    allocators: Res<MaterialBindGroupAllocators>,
    specialized: Res<SpecializedMaterialPipelineCache>,
    pipelines: Res<PipelineCache>,
    meshes: Res<RenderAssets<RenderMesh>>,
    mut waited: Local<u32>,
) {
    let Some(camera) = check.camera.filter(|_| check.active) else {
        check.ready.store(0, Relaxed);
        *waited = 0;
        return;
    };
    let entries: Vec<_> = specialized.iter().filter(|(view, _)| view.main_entity == camera).map(|(_, e)| e).collect();
    // Each test in turn: mesh, material instance, material bound, pipeline asked, pipeline ready.
    let flags = |(entity, mesh, material): &(MainEntity, AssetId<Mesh>, UntypedAssetId)| {
        let bound = materials.get(*material).is_some_and(|m| allocators.get(&material.type_id()).and_then(|a| a.get(m.binding.group)).is_some_and(|g| g.bind_group().is_some()));
        let ids: Vec<_> = entries.iter().filter_map(|e| e.get(entity)).collect();
        [meshes.get(*mesh).is_some(), instances.instances.get(entity).is_some_and(|i| i.asset_id == *material), bound, !ids.is_empty(), ids.iter().any(|id| pipelines.get_render_pipeline(**id).is_some())]
    };
    if !check.shapes.is_empty() && check.shapes.iter().all(|s| flags(s).iter().all(|f| *f)) {
        check.ready.fetch_add(1, Relaxed);
        return;
    }
    check.ready.store(0, Relaxed);
    *waited += 1;
    // Once, should the picture take long: which test holds it.
    if *waited == PICTURE_WAIT_LOG_FRAMES {
        let held: Vec<String> = check.shapes.iter().map(|s| flags(s).iter().map(|f| if *f { '1' } else { '0' }).collect()).collect();
        info!("scope view: the picture not drawn after {PICTURE_WAIT_LOG_FRAMES} frames; per shape mesh/instance/bound/asked/ready: {}; views with pipelines for the camera: {}", held.join(" "), entries.len());
    }
}

/// Render frames of waiting after which [`observe_picture`] says what holds the picture.
const PICTURE_WAIT_LOG_FRAMES: u32 = 60;

/// The picture camera on while the scope is up and the picture is not yet drawn (off while
/// it is down). Once drawn, the camera and the shapes it drew are despawned and only the
/// picture's image is kept: a camera turned off keeps its view targets on its render-world
/// entity [observed: 60 MB still allocated at 3440x1440 with the camera off; vendored
/// bevy_render `extract_cameras` takes away only the extracted components].
fn settle_picture(mut commands: Commands, scope: Res<Scope>, mut check: ResMut<PictureCheck>, mut cameras: Query<(Entity, &mut Camera, &mut PictureCamera)>) {
    for (entity, mut camera, mut pic) in &mut cameras {
        let checked = check.camera == Some(MainEntity::from(entity));
        if checked && check.ready.load(Relaxed) >= PICTURE_READY_FRAMES {
            info!("scope view: picture drawn in {} frames; its camera and shapes are gone, the picture kept", pic.frames);
            commands.entity(entity).despawn();
            for (shape, _, _) in check.shapes.drain(..) {
                commands.entity(shape.id()).try_despawn();
            }
            check.camera = None;
            check.active = false;
            check.ready.store(0, Relaxed);
            continue;
        }
        let on = scope.on && !scope.lens_3d;
        if on {
            pic.frames += 1;
        }
        if camera.is_active != on {
            camera.is_active = on;
        }
        if checked && check.active != on {
            check.active = on;
        }
    }
}

/// The 2D picture's camera, while it has not drawn the picture yet.
#[derive(Component, Default)]
struct PictureCamera {
    frames: u32,
}

/// The game path of a MOD3 model (stored without `meshes\`).
pub fn model_path(mod3: &str) -> String {
    let p = mod3.replace('/', "\\");
    if p.to_ascii_lowercase().starts_with("meshes\\") {
        p
    } else {
        format!("meshes\\{p}")
    }
}

/// The time of day counts as night: from the end of sunset to the start of sunrise
/// (hours) [hypothesis, §4].
pub fn is_night(hour: f32, sunrise_begin: f32, sunset_end: f32) -> bool {
    let h = hour.rem_euclid(24.0);
    h >= sunset_end || h < sunrise_begin
}

/// The picture as drawn: its entities, the model and the window size it was built for,
/// and (the 2D picture) its full-screen node, kept hidden while the scope is down.
#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    model: Option<String>,
    lens: bool,
    size: UVec2,
    picture: Option<Entity>,
    hidden: bool,
    warned: Option<String>,
}

/// What [`show`] does with what is drawn, for the scope wanted (`want`: its picture's
/// model path while up) [design, #4781].
#[derive(Debug, PartialEq, Eq)]
enum Keep {
    /// Leave it as it is.
    Leave,
    /// The 2D picture hidden (the scope went down) or shown again (back up, the same).
    Hide,
    Show,
    /// Despawn it (and build what is wanted, if anything).
    Remake,
}

fn keep(drawn_model: Option<&str>, drawn_lens: bool, drawn_size: UVec2, hidden: bool, want: Option<&str>, lens: bool, size: UVec2) -> Keep {
    let Some(model) = drawn_model else { return Keep::Leave };
    match want {
        // The 3D lens goes with its scope; the 2D picture is kept for the next scope-up.
        None if drawn_lens => Keep::Remake,
        None => if hidden { Keep::Leave } else { Keep::Hide },
        Some(w) if w != model || lens != drawn_lens || (!lens && drawn_size != size) => Keep::Remake,
        Some(_) if hidden => Keep::Show,
        Some(_) => Keep::Leave,
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn show(
    mut commands: Commands,
    scope: Res<Scope>,
    mut models: ModelCtx,
    mut plain: ResMut<Assets<StandardMaterial>>,
    window: Query<&Window, With<PrimaryWindow>>,
    (mounts, main_cam): (Query<(Entity, &LensMount)>, Query<Entity, With<PlayerCamera>>),
    (mut check, mut visibility): (ResMut<PictureCheck>, Query<&mut Visibility>),
    mut drawn: Local<Drawn>,
) {
    let size = window.single().map(|w| UVec2::new(w.physical_width().max(1), w.physical_height().max(1))).unwrap_or(UVec2::new(1280, 720));
    let want = scope.model.as_deref().filter(|_| scope.on).map(model_path);
    let lens = scope.lens_3d;
    match keep(drawn.model.as_deref(), drawn.lens, drawn.size, drawn.hidden, want.as_deref(), lens, size) {
        Keep::Leave => {}
        k @ (Keep::Hide | Keep::Show) => {
            drawn.hidden = k == Keep::Hide;
            if let Some(mut v) = drawn.picture.and_then(|p| visibility.get_mut(p).ok()) {
                *v = if drawn.hidden { Visibility::Hidden } else { Visibility::Inherited };
            }
            if !drawn.hidden {
                info!("scope view: {} shown again (kept)", drawn.model.as_deref().unwrap_or(""));
            }
        }
        Keep::Remake => {
            // The 2D picture's camera and shapes may be gone already (`settle_picture`).
            for e in drawn.entities.drain(..) {
                commands.entity(e).try_despawn();
            }
            drawn.model = None;
            drawn.picture = None;
            drawn.hidden = false;
            *check = PictureCheck { ready: check.ready.clone(), ..default() };
            check.ready.store(0, Relaxed);
        }
    }
    let Some(path) = want else { return };
    if drawn.model.is_some() {
        return;
    }
    let Some(model) = models.model(&path) else {
        if drawn.warned.as_deref() != Some(&path) {
            warn!("scope view: {path} not loaded; the scope is shown without its picture");
            drawn.warned = Some(path);
        }
        return;
    };
    if lens {
        let (Some((mount, m)), Ok(cam)) = (mounts.iter().next(), main_cam.single()) else { return };
        drawn.entities = draw_lens(&mut commands, &model, &mut models, mount, m.radius, cam);
        info!("scope view: {path} in the lens (3D scope), radius {:.2}, reticle cut to {LENS_CIRCLE} units", m.radius);
        drawn.model = Some(path);
        drawn.lens = true;
        return;
    }
    let mut image = Image::new_fill(Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 }, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = models.images.add(image);
    let layer = RenderLayers::layer(SCOPE_LAYER);
    // Game space (Z up) into Bevy's (Y up), as the world root does.
    let place = Affine3A::from_translation(PLACE) * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let to = |p: Vec3| place.transform_point3(p);
    let mut entities = Vec::new();
    // Each shape the render world must have drawn before the camera is turned off.
    let mut shapes = Vec::new();
    for s in &model.shapes {
        let world = place * model.root * s.transform;
        let name = s.name.as_deref().unwrap_or("");
        let e = commands.spawn((Name::new(format!("Scope {name}")), Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(world)), layer.clone())).id();
        shapes.push((MainEntity::from(e), s.mesh.id(), s.material.id().untyped()));
        entities.push(e);
    }
    // Black beyond the mask's sides, for screens wider than 16:10.
    let bar = models.meshes.add(Rectangle::new(4.0 * VIEW_HEIGHT, 2.0 * VIEW_HEIGHT));
    let black = plain.add(StandardMaterial { base_color: Color::BLACK, unlit: true, ..default() });
    for side in [-1.0f32, 1.0] {
        let at = to(Vec3::new(side * (MASK_HALF_WIDTH + 2.0 * VIEW_HEIGHT - 1.0), 0.0, 0.0));
        let e = commands.spawn((Name::new("Scope side bar"), Mesh3d(bar.clone()), MeshMaterial3d(black.clone()), Transform::from_translation(at), layer.clone())).id();
        shapes.push((MainEntity::from(e), bar.id(), black.id().untyped()));
        entities.push(e);
    }
    let eye = Vec3::new(0.0, -1000.0, 0.0);
    // No MSAA and no transmission: each would add window-sized targets while the camera
    // is on (module notes, #4781).
    let camera = commands
        .spawn((
            Name::new("Scope camera"),
            PictureCamera::default(),
            Camera3d::default(),
            Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
            RenderTarget::Image(ImageRenderTarget::from(image.clone())),
            Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::FixedVertical { viewport_height: VIEW_HEIGHT }, near: 0.0, far: 3000.0, ..OrthographicProjection::default_3d() }),
            Tonemapping::Linear,
            Msaa::Off,
            ScreenSpaceTransmission { steps: 0, ..default() },
            AmbientLight { color: Color::WHITE, brightness: 500.0, ..default() },
            Transform::from_translation(to(eye)).looking_at(to(Vec3::ZERO), Vec3::Y),
            layer.clone(),
        ))
        .id();
    entities.push(camera);
    check.camera = Some(MainEntity::from(camera));
    check.shapes = shapes;
    check.ready.store(0, Relaxed);
    // Under the HUD (whose nodes sit at the default z index), over the world.
    let picture = commands
        .spawn((
            Name::new("Scope picture"),
            Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), ..default() },
            ImageNode::new(image),
            GlobalZIndex(-1),
            Pickable::IGNORE,
        ))
        .id();
    entities.push(picture);
    info!("scope view: {path} drawn ({} shapes), {VIEW_HEIGHT} units high", model.shapes.len());
    drawn.entities = entities;
    drawn.picture = Some(picture);
    drawn.model = Some(path);
    drawn.lens = false;
    drawn.size = size;
}

/// A square image to render into: `hdr` linear (the world, tonemapped by the main
/// camera when drawn on the lens), else sRGB with alpha (the reticle).
fn target_image(images: &mut Assets<Image>, px: u32, hdr: bool) -> Handle<Image> {
    let format = if hdr { TextureFormat::Rgba16Float } else { TextureFormat::Bgra8UnormSrgb };
    let mut image = Image::new_uninit(Extent3d { width: px, height: px, depth_or_array_layers: 1 }, TextureDimension::D2, format, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    images.add(image)
}

/// The 3D scope: the reticle (the MOD3 model without its mask) and the zoomed world, each
/// rendered into an image, shown on two discs at the eyepiece `mount`.
fn draw_lens(commands: &mut Commands, model: &nvf_assets::model::PreparedModel, models: &mut ModelCtx, mount: Entity, radius: f32, cam: Entity) -> Vec<Entity> {
    let mut entities = Vec::new();
    let reticle = target_image(&mut models.images, RETICLE_PX, false);
    let layer = RenderLayers::layer(SCOPE_LAYER);
    let place = Affine3A::from_translation(PLACE) * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    for s in &model.shapes {
        let name = s.name.as_deref().unwrap_or("").to_ascii_lowercase();
        // The reticle only: not the black mask (vanilla `keepScope01:0`; `MaskRing*` in
        // the other layouts), nor the tint ring, the glare (`Additive01:5`) or the inner
        // shading, which over the lens drew as a dark veil (drawn into a clear image they
        // keep their alpha) [observed: scope-3d-lens-interior, the lens much darker than
        // the door behind it until they were left out].
        let veil = ["keepscope", "maskring", "tintring", "additive", "inner lens shading"];
        if veil.iter().any(|v| name.starts_with(v)) {
            continue;
        }
        let world = place * model.root * s.transform;
        entities.push(commands.spawn((Name::new(format!("Scope lens reticle {name}")), Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), Transform::from_matrix(Mat4::from(world)), layer.clone())).id());
    }
    let to = |p: Vec3| place.transform_point3(p);
    entities.push(
        commands
            .spawn((
                Name::new("Scope lens reticle camera"),
                Camera3d::default(),
                Camera { order: -5, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(reticle.clone())),
                Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::FixedVertical { viewport_height: 2.0 * LENS_CIRCLE }, near: 0.0, far: 3000.0, ..OrthographicProjection::default_3d() }),
                Tonemapping::Linear,
                Msaa::Sample4,
                AmbientLight { color: Color::WHITE, brightness: 500.0, ..default() },
                Transform::from_translation(to(Vec3::new(0.0, -1000.0, 0.0))).looking_at(to(Vec3::ZERO), Vec3::Y),
                layer,
            ))
            .id(),
    );
    // The zoomed world: the main camera's view without the arms and body and the scope's
    // own layer; the impact decals too (they need a depth prepass, which it has). Its own
    // grade holds only what belongs in the lens (a night-vision scope's modifier).
    let world_image = target_image(&mut models.images, LENS_WORLD_PX, true);
    let lens_cam = commands
        .spawn((
            Name::new("Scope lens camera"),
            LensCamera,
            Camera3d::default(),
            Camera { order: -4, ..default() },
            RenderTarget::Image(ImageRenderTarget::from(world_image.clone())),
            bevy::camera::Hdr,
            Tonemapping::Linear,
            Msaa::Sample4,
            Projection::Perspective(PerspectiveProjection { fov: 0.1, near: 5.0, far: crate::sky::OUTDOOR_FAR, aspect_ratio: 1.0, ..default() }),
            crate::sky::initial_fog(),
            bevy::core_pipeline::prepass::DepthPrepass,
            RenderLayers::from_layers(&[0, crate::reflection::WATER_LAYER, crate::decals::DECAL_LAYER, crate::volumetric_clouds::FLAT_CLOUD_LAYER]),
            (crate::imagespace::ImageSpaceGrade::default(), crate::imagespace::LensGrade),
            Transform::IDENTITY,
        ))
        .id();
    commands.entity(cam).add_child(lens_cam);
    entities.push(lens_cam);
    let disc = models.meshes.add(Circle::new(radius));
    let arms = RenderLayers::layer(crate::sky::ARMS_LAYER);
    // NIF materials drawn as first-person shapes, as the rifle and its scope around them
    // are (docs/design/first-person-depth.md): at world depth the squeezed scope tube hid
    // the discs inside it.
    let first_person = |base: StandardMaterial| {
        let mut m = nvf_assets::nif_material::nif_material(base);
        m.extension.first_person = true;
        m
    };
    let world_mat = models.materials.add(first_person(StandardMaterial { base_color_texture: Some(world_image), unlit: true, fog_enabled: false, ..default() }));
    let reticle_mat = models.materials.add(first_person(StandardMaterial { base_color_texture: Some(reticle), unlit: true, fog_enabled: false, alpha_mode: AlphaMode::Blend, ..default() }));
    for (name, mat, z) in [("Scope lens world", world_mat, 0.0), ("Scope lens reticle", reticle_mat, RETICLE_LIFT)] {
        let e = commands.spawn((Name::new(name), Mesh3d(disc.clone()), MeshMaterial3d(mat), Transform::from_xyz(0.0, 0.0, z), arms.clone(), bevy::light::NotShadowCaster)).id();
        commands.entity(mount).add_child(e);
        entities.push(e);
    }
    entities
}

/// Keep the 3D scope's camera on the main view's fog and far plane, and its angle such
/// that the lens magnifies as the full zoom does: the lens's angular radius on screen
/// (tan a) over the zoom's magnification tan(base/2) / tan(sight/2).
#[allow(clippy::type_complexity)]
fn lens_camera(
    zoom: Option<Res<nvf_play::aim_zoom::AimZoom>>,
    (player, sway): (Option<Res<Player>>, Res<nvf_play::aim_zoom::LensSway>),
    main: Query<(&GlobalTransform, &Projection, &DistanceFog), (With<PlayerCamera>, Without<LensCamera>)>,
    mut lens: Query<(&mut Projection, &mut DistanceFog, &mut Transform), With<LensCamera>>,
    mounts: Query<(&GlobalTransform, &LensMount)>,
    mut told: Local<Option<i32>>,
) {
    let Ok((mut proj, mut fog, mut turn)) = lens.single_mut() else {
        *told = None;
        return;
    };
    let (Ok((cam, main_proj, main_fog)), Some((mount, m)), Some(z)) = (main.single(), mounts.iter().next(), zoom) else { return };
    let (Some(base), Some(sight), Projection::Perspective(mp)) = (z.base(), z.sight, main_proj) else { return };
    let centre = mount.translation();
    let edge = mount.transform_point(Vec3::X * m.radius);
    let tan_a = (edge - centre).length() / (centre - cam.translation()).length().max(1e-3);
    let magnification = (base / 2.0).tan() / (sight / 2.0).tan().max(1e-4);
    let fov = 2.0 * (tan_a / magnification.max(1e-3)).atan();
    if let Projection::Perspective(p) = &mut *proj {
        if (p.fov - fov).abs() > 1e-5 || p.far != mp.far {
            p.fov = fov;
            p.far = mp.far;
            p.aspect_ratio = 1.0;
        }
    }
    *fog = main_fog.clone();
    // The sway turns the lens's view with the gun, off the main view's (§5): the turn
    // from the view's direction to the swayed one, in the main camera's frame.
    let local = match player.as_deref() {
        Some(p) if *sway != nvf_play::aim_zoom::LensSway::default() => {
            let dir = |d: [f32; 3]| nvf_play::space::to_bevy(d) - nvf_play::space::to_bevy([0.0; 3]);
            let (from, to) = (dir(nvf_play::space::look_dir(p.heading, p.pitch)), dir(sway.look_dir(p.heading, p.pitch)));
            let r = cam.compute_transform().rotation;
            r.inverse() * Quat::from_rotation_arc(from.normalize_or(Vec3::NEG_Z), to.normalize_or(Vec3::NEG_Z)) * r
        }
        _ => Quat::IDENTITY,
    };
    if turn.rotation != local {
        turn.rotation = local;
    }
    let deg = (fov.to_degrees() * 10.0).round() as i32;
    if told.is_none_or(|t| (t - deg).abs() > 5) {
        *told = Some(deg);
        info!("scope lens camera: FOV {:.1}°, the lens {:.1}° across on screen, magnification {magnification:.2}", fov.to_degrees(), 2.0 * tan_a.atan().to_degrees());
    }
}

/// Keep the 3D scope lens's fog synchronized with the main camera after `underwater_fog`
/// runs in `PostUpdate`, avoiding 1-frame stale underwater/above-water fog in the lens.
#[allow(clippy::type_complexity)]
pub(crate) fn sync_lens_fog(
    main: Query<&DistanceFog, (crate::fly::WorldCamera, Without<LensCamera>)>,
    mut lens: Query<&mut DistanceFog, With<LensCamera>>,
) {
    let Ok(mut fog) = lens.single_mut() else { return };
    let Ok(main_fog) = main.single() else { return };
    *fog = main_fog.clone();
}

/// Hold the night-vision scope's modifier on the screen while it gives night vision.
fn night_vision(scope: Res<Scope>, sky: Option<Res<SkyState>>, world: Option<Res<WorldData>>, mut mods: ResMut<ScreenModifiers>, mut state: Local<(Option<Option<u32>>, bool, bool)>, tweaks: Option<Res<nvf_play::tweaks::TweakSettings>>) {
    // The player's Night Vision Toggle tweak's By Day Too [design: docs/design/tweaks.md].
    let by_day = tweaks.as_ref().is_some_and(|t| t.night_vision_toggle && t.night_vision_by_day);
    let night = by_day || sky.as_deref().is_some_and(|s| s.interior.is_some() || is_night(s.hour, s.climate.sunrise.0, s.climate.sunset.1));
    let on = scope.on && scope.night_vision && night;
    // A 3D scope gives its night vision in the lens only [design].
    let lens = on && scope.lens_3d;
    if (on, lens) == (state.1, state.2) {
        return;
    }
    (state.1, state.2) = (on, lens);
    let Some(world) = world else { return };
    let form = *state.0.get_or_insert_with(|| fnv_world::image_modifier::find_image_modifier(world.lo, world.schema, NIGHT_SCOPE_IMAD));
    match form {
        Some(f) => {
            mods.hold(f, on && !lens);
            mods.hold_lens(f, lens);
            info!("scope: night vision {NIGHT_SCOPE_IMAD} {}", if lens { "on in the lens" } else if on { "on" } else { "off" });
        }
        None if on => warn!("scope: no image space modifier {NIGHT_SCOPE_IMAD}; the night-vision scope shows no night vision"),
        None => {}
    }
}

/// The wobble's curves: the track, its span (seconds) and its widest point (radians).
struct Wobble {
    track: Arc<TransformTrack>,
    start: f32,
    stop: f32,
    peak: f32,
}

impl Wobble {
    fn load(data: &GameData) -> Option<Wobble> {
        let bytes = data.read(WOBBLE_MODEL)?;
        let file = fnv_nif::NifFile::parse(&bytes, Some(&data.schema)).ok()?;
        let dec = fnv_nif::scene::Decoded::new(&file, &bytes, &data.schema);
        let (_, track, cycle) = nvf_assets::model::free_transform_tracks(&file, &dec).into_iter().find(|(n, ..)| n.eq_ignore_ascii_case("Camera1st"))?;
        let mut w = Wobble { track, start: cycle.start, stop: cycle.stop, peak: 0.0 };
        if !(w.start.is_finite() && w.stop.is_finite()) {
            return None;
        }
        let widths: Vec<f32> = (0..=200).map(|k| w.angles(k as f32 / 200.0)).map(|[x, z]| x.hypot(z)).collect();
        // A broken file's NaN or infinite turn would reach the player's view: no sway.
        if !widths.iter().all(|v| v.is_finite()) {
            return None;
        }
        w.peak = widths.into_iter().fold(0.0, f32::max);
        (w.peak > 0.0).then_some(w)
    }

    /// The node's turn about X and Z (radians) at `share` (0–1) of the loop.
    fn angles(&self, share: f32) -> [f32; 2] {
        let t = self.start + (self.stop - self.start) * share.rem_euclid(1.0);
        if let Some(e) = &self.track.euler {
            let at = |k: usize| e[k].sample(t).map_or(0.0, |v| v[0]);
            return [at(0), at(2)];
        }
        let [w, x, y, z] = self.track.rotation_at(t).unwrap_or([1.0, 0.0, 0.0, 0.0]);
        let (ex, _, ez) = Quat::from_xyzw(x, y, z, w).to_euler(EulerRot::XYZ);
        [ex, ez]
    }

    /// The view's turn (yaw, pitch; degrees) `t` seconds into the loop, scaled so the
    /// widest point is `radius` degrees.
    fn turn(&self, t: f32, radius: f32) -> [f32; 2] {
        let span = (self.stop - self.start).max(0.01);
        let [x, z] = self.angles(t / span);
        [z / self.peak * radius, x / self.peak * radius]
    }
}

#[derive(Default)]
struct SwayState {
    wobble: Option<Option<Wobble>>,
    t: f32,
    last: [f32; 2],
    told: bool,
}

/// Sway the view while the scope is up (§5).
#[allow(clippy::too_many_arguments)]
fn sway(
    time: Res<Time>,
    scope: Res<Scope>,
    combat: Res<Combat>,
    sim: Option<Res<nvf_play::sim::SimState>>,
    data: Option<Res<GameData>>,
    mut player: ResMut<Player>,
    mut lens_sway: ResMut<nvf_play::aim_zoom::LensSway>,
    mut st: Local<SwayState>,
    tweaks: Option<Res<nvf_play::tweaks::TweakSettings>>,
) {
    let want = if scope.on {
        if st.wobble.is_none() {
            // A panic reading a broken file counts as no wobble.
            let w = data.as_deref().and_then(|d| nvf_assets::guard::guarded(|| Ok(Wobble::load(d))).unwrap_or_else(|e| {
                warn!("scope sway: {WOBBLE_MODEL} not read: {e}");
                None
            }));
            if w.is_none() {
                warn!("scope sway: {WOBBLE_MODEL} has no Camera1st rotation; scopes do not sway");
            }
            st.wobble = Some(w);
        }
        let setting = |name: &str, or: f32| sim.as_deref().and_then(|s| s.forms.setting_f32(name)).unwrap_or(or);
        let spread = combat.player_spread.unwrap_or(0.0);
        let attacking = combat.since_last_shot() < ATTACK_SECONDS;
        let radius = spread * SPREAD_TO_DEG / 2.0 * setting("fGunWobbleMultScope", 1.0) * if attacking { 1.0 } else { setting("fNonAttackGunWobbleMult", 0.5) };
        // The player's Hold Breath tweak steadies it (docs/design/tweaks.md).
        let held = nvf_play::tweaks::BREATH_HELD.load(std::sync::atomic::Ordering::Relaxed);
        let radius = if held { radius * tweaks.as_ref().map_or(1.0, |t| t.breath_sway) } else { radius };
        if !st.told {
            st.told = true;
            info!("scope sway: PSp {spread:.3}, radius {radius:.2}°");
        }
        st.t += time.delta_secs();
        let t = st.t;
        st.wobble.as_ref().and_then(Option::as_ref).map_or([0.0, 0.0], |w| w.turn(t, radius))
    } else {
        st.t = 0.0;
        st.told = false;
        [0.0, 0.0]
    };
    // A 3D scope sways the gun and its lens (`LensSway`); the picture sways the view.
    let (view, lens) = if scope.lens_3d { ([0.0, 0.0], want) } else { (want, [0.0, 0.0]) };
    let turn = nvf_play::aim_zoom::LensSway { yaw: lens[0].to_radians(), pitch: lens[1].to_radians() };
    if *lens_sway != turn {
        *lens_sway = turn;
    }
    if view == st.last {
        return;
    }
    player.heading += (view[0] - st.last[0]).to_radians();
    player.pitch = (player.pitch + (view[1] - st.last[1]).to_radians()).clamp(-MAX_PITCH, MAX_PITCH);
    st.last = view;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn night_runs_from_sunset_end_to_sunrise_begin() {
        assert!(is_night(23.0, 6.0, 20.0));
        assert!(is_night(2.0, 6.0, 20.0));
        assert!(is_night(20.0, 6.0, 20.0));
        assert!(!is_night(6.0, 6.0, 20.0));
        assert!(!is_night(12.0, 6.0, 20.0));
        assert!(is_night(26.0, 6.0, 20.0));
    }

    #[test]
    fn mod3_paths_gain_the_meshes_folder_once() {
        assert_eq!(model_path("interface\\HUD\\mildotscope.NIF"), "meshes\\interface\\HUD\\mildotscope.NIF");
        assert_eq!(model_path("Meshes\\interface\\hud\\x.nif"), "Meshes\\interface\\hud\\x.nif");
    }

    /// #4781: the 2D picture is kept hidden across a scope-down and shown again for the
    /// same model and window size; anything else makes it anew; the 3D lens goes with it.
    #[test]
    fn the_2d_picture_is_kept_between_scope_ups() {
        let (a, b, s, t) = (Some("meshes\\a.nif"), Some("meshes\\b.nif"), UVec2::new(3440, 1440), UVec2::new(1280, 720));
        assert_eq!(keep(None, false, s, false, a, false, s), Keep::Leave, "nothing drawn: build");
        assert_eq!(keep(a, false, s, false, a, false, s), Keep::Leave, "up and drawn");
        assert_eq!(keep(a, false, s, false, None, false, s), Keep::Hide, "down: hidden, kept");
        assert_eq!(keep(a, false, s, true, None, false, s), Keep::Leave, "down and hidden");
        assert_eq!(keep(a, false, s, true, a, false, s), Keep::Show, "up again, the same");
        assert_eq!(keep(a, false, s, true, None, false, t), Keep::Leave, "a resize while down waits for the next scope-up");
        assert_eq!(keep(a, false, s, true, a, false, t), Keep::Remake, "up again at another window size");
        assert_eq!(keep(a, false, s, false, a, false, t), Keep::Remake, "a resize while up");
        assert_eq!(keep(a, false, s, true, b, false, s), Keep::Remake, "another scope");
        assert_eq!(keep(a, false, s, true, a, true, s), Keep::Remake, "the 3D lens instead");
        assert_eq!(keep(a, true, s, false, None, true, s), Keep::Remake, "the 3D lens goes down with its scope");
        assert_eq!(keep(a, true, s, false, a, true, t), Keep::Leave, "the 3D lens does not follow the window size");
    }
}
