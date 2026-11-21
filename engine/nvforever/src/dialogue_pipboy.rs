//! A dialogue-owned view of the actual resolved Pip-Boy model.
//! No inventory, first-person arm, raise animation or PipBoy gameplay state is changed.
use crate::actors::ActorWorld;
use crate::pipboy_arm::{self, PipBoyPart, Posed};
use crate::player_body::PlayerBody;
use crate::terminal_view::projection::GlassMap;
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::camera::{visibility::RenderLayers, CameraUpdateSystems, ImageRenderTarget, RenderTarget};
use bevy::math::{Affine2, Affine3A};
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::transform::TransformSystems;
use fnv_actor::resolve::slot;
use fnv_sim::FormId;
use nvf_assets::{model::{ModelCtx, ModelCache}, nif_material::NifMaterial,
    texture::TextureCache, GameData};
use nvf_play::dialogue_device::{DeviceKind, DeviceView, Request};
use std::sync::Arc;

const LAYER: usize = 17;
const PLACE: Vec3 = Vec3::new(16384.0, 16384.0, 16384.0);
const FOV: f32 = 0.5;
const UV_ATTEMPTS: i32 = 16;

type ProjectionKey = (Mat4, Affine3A, Option<Rect>, Option<UVec2>);

#[derive(SystemParam)]
struct Sources<'w> {
    world: Option<Res<'w, ActorWorld>>,
    data: Option<Res<'w, GameData>>,
    body: Option<Res<'w, PlayerBody>>,
    script: Option<Res<'w, nvf_play::script::ScriptState>>,
    #[cfg(feature = "dialogue-model-capture")]
    capture: Option<Res<'w, crate::dialogue_capture::CaptureWorn>>,
}

impl Sources<'_> {
    fn worn(&self) -> Option<&[FormId]> {
        if let Some(body) = self.body.as_deref() { return Some(body.worn()); }
        #[cfg(feature = "dialogue-model-capture")]
        if let Some(capture) = self.capture.as_deref() { return Some(&capture.0); }
        None
    }

    fn look(&self) -> Option<&fnv_game::chargen::Look> {
        self.body.as_ref().and(self.script.as_deref())
            .and_then(|s| s.rt.as_ref()).and_then(|rt| rt.game.player.look.as_ref())
    }
}

/// The admitted source, not merely the last worn vector. A real empty body is
/// authoritative and must never fall through to capture equipment.
#[derive(Clone, PartialEq)]
struct Provider {
    body: bool,
    worn: Vec<FormId>,
    look: Option<fnv_game::chargen::Look>,
    change: fnv_actor::Change,
}

impl Provider {
    fn matches(&self, sources: &Sources<'_>) -> bool {
        sources.world.as_ref().is_some_and(|world|
            self.body == sources.body.is_some() && Some(self.worn.as_slice()) == sources.worn()
                && self.look.as_ref() == sources.look()
                && self.change == world.change(FormId::PLAYER_REF.0))
    }

    fn read(sources: &Sources<'_>) -> Option<Self> {
        let world = sources.world.as_deref()?;
        Some(Self {body:sources.body.is_some(), worn:sources.worn()?.to_vec(),
            look:sources.look().cloned(),change:world.change(FormId::PLAYER_REF.0)})
    }
}

#[derive(Resource, Default)]
struct Owned {
    request: Option<Request>,
    attempted: bool,
    provider: Option<Provider>,
    model: Option<String>,
    scene: Option<Scene>,
}

struct Scene {
    root: Entity,
    camera: Entity,
    image: Handle<Image>,
    materials: Vec<Handle<NifMaterial>>,
    screen_material: Handle<NifMaterial>,
    glass: Arc<Posed>,
    blockers: Vec<Arc<Posed>>,
    uv: Rect,
    projection: Option<ProjectionKey>,
    rejected: Option<ProjectionKey>,
}

fn same(a: &Request, b: &Request) -> bool {
    a.stamp == b.stamp && a.image == b.image && a.camera == b.camera
        && a.target == b.target && a.canvas == b.canvas
}

pub(crate) fn install(app: &mut App) {
    use bevy::ecs::schedule::common_conditions::resource_exists;
    app.init_resource::<Owned>().init_resource::<ModelCache>().init_resource::<TextureCache>()
        // Retire a changed provider before the sole dialogue raw reader runs.
        .add_systems(Update, (retire, ApplyDeferred).chain().before(nvf_play::PlaySet::Scripts))
        .add_systems(Update, prepare
            .run_if(resource_exists::<GameData>)
            .run_if(resource_exists::<ActorWorld>)
            .run_if(resource_exists::<Assets<NifMaterial>>)
            .after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync))
        .add_systems(PostUpdate, publish.run_if(resource_exists::<Assets<NifMaterial>>).after(CameraUpdateSystems)
            .after(TransformSystems::Propagate));
}

fn remove_view(commands: &mut Commands, request: &Request) {
    let stamp = request.stamp;
    // Check again when commands apply, so another kind's newly published view survives.
    commands.queue(move |world: &mut World| {
        if world.get_resource::<DeviceView>().is_some_and(|v|
            v.stamp.kind == DeviceKind::PipBoy && v.stamp == stamp) {
            world.remove_resource::<DeviceView>();
        }
    });
}

type RetireAssets<'w> = (Option<ResMut<'w, Assets<Image>>>, Option<ResMut<'w, Assets<NifMaterial>>>);

fn retire(
    mut commands: Commands, request: Option<Res<Request>>, sources: Sources,
    mut owned: ResMut<Owned>,
    cameras: Query<(&Camera,&GlobalTransform,&RenderTarget)>, windows: Query<&Window>,
    (mut images, mut materials): RetireAssets<'_>,
) {
    let current = request.as_deref().filter(|r| r.valid() && r.stamp.kind == DeviceKind::PipBoy);
    let changed = match (owned.request.as_ref(), current) {
        (Some(old), Some(new)) => !same(old, new), (None, None) => false, _ => true,
    };
    if current.is_none() && owned.request.is_none() && owned.scene.is_none() { return; }
    let inputs_changed = match &owned.provider {
        Some(provider) => !provider.matches(&sources),
        None => sources.world.is_some() && sources.worn().is_some(),
    };
    let refresh = inputs_changed || sources.world.as_ref().is_some_and(|w| w.is_changed());
    let provider = if refresh { Provider::read(&sources) } else { None };
    let model = if refresh {
        provider.as_ref().filter(|p| !p.worn.is_empty()).and_then(|p| {
            let look = p.look.as_ref().map(nvf_play::creator::actor_look);
            let world = sources.world.as_deref()?;
            if let Some(look) = look.as_ref() { resolved_model_path(world,&p.worn,Some(look)) }
            else { model_path(world,&p.worn) }
        })
    } else { None };
    let provider_changed = inputs_changed || (refresh && model != owned.model);
    let missing_assets = owned.scene.as_ref().is_some_and(|scene|
        images.as_deref().is_none_or(|a| a.get(&scene.image).is_none())
        || materials.as_deref().is_none_or(|a| scene.materials.iter().any(|h| a.get(h).is_none())));
    if changed || provider_changed || missing_assets {
        if let Some(old) = owned.request.as_ref() { remove_view(&mut commands, old); }
        if let Some(scene) = owned.scene.take() {
            commands.queue(move |world: &mut World| {
                if let Ok(entity) = world.get_entity_mut(scene.root) { entity.despawn(); }
            });
            if let Some(images) = images.as_deref_mut() { images.remove(scene.image.id()); }
            if let Some(materials) = materials.as_deref_mut() {
                for material in scene.materials { materials.remove(material.id()); }
            }
        }
        owned.attempted = false;
    }
    if changed { owned.request = current.cloned(); }
    if refresh { owned.provider = provider; owned.model = model; }
    // Retry a stable missing asset only when its actual GameData provider changes.
    if sources.data.as_ref().is_some_and(|data| data.is_changed()) { owned.attempted = false; }
    if let (Some(scene), Some(request)) = (owned.scene.as_mut(), current) {
        let signature = camera_signature(scene, request, &cameras, &windows);
        if signature.is_none() || scene.projection.is_some_and(|old| Some(old) != signature) {
            scene.projection = None;
            scene.rejected = None;
            remove_view(&mut commands,request);
        }
    }

}

/// This is the same actor-part resolver the player body uses, without changing it.
pub(crate) fn model_path(world: &ActorWorld, worn: &[FormId]) -> Option<String> {
    resolved_model_path(world, worn, None)
}

fn resolved_model_path(world: &ActorWorld, worn: &[FormId], look: Option<&fnv_actor::Look>) -> Option<String> {
    let appearance = world.resolve_as(FormId::PLAYER_BASE.0, FormId::PLAYER_REF.0,
        Some(worn), look).ok()?;
    appearance.parts.into_iter().find(|p| p.role == "armor"
        && world.actors.armors.get(&p.form).is_some_and(|a| a.slots & slot::PIPBOY != 0))
        .map(|p| p.model)
}

/// Fit all actual housing vertices, not just the glass, in either display aspect.
fn fit(glass: &Posed, shapes: &[Arc<Posed>], aspect: f32) -> Option<(Transform, f32, f32)> {
    let center = glass.middle()?;
    let normal = glass.triangles.iter().find_map(|&[a,b,c]|
        (glass.points[b]-glass.points[a]).cross(glass.points[c]-glass.points[a]).try_normalize())?;
    let uv_center = glass.uvs.iter().copied().sum::<Vec2>() / glass.uvs.len() as f32;
    let here = glass.at_uv(uv_center)?;
    let down = glass.at_uv(uv_center + Vec2::new(0.0, 0.01))? - here;
    let up = -down.try_normalize()?;
    let orientation = Transform::from_translation(normal).looking_at(Vec3::ZERO, up);
    if !orientation.rotation.is_finite() { return None; }
    let right = orientation.rotation * Vec3::X;
    let up = orientation.rotation * Vec3::Y;
    let mut width: f32 = 0.0;
    let mut height: f32 = 0.0;
    let mut depth: f32 = 0.0;
    for shape in shapes {
        for point in &shape.points {
            let p = *point-center;
            width = width.max(p.dot(right).abs());
            height = height.max(p.dot(up).abs());
            depth = depth.max(p.dot(normal).abs());
        }
    }
    let distance = depth + (height.max(width/aspect) / (FOV*0.5).tan())*1.08;
    if !distance.is_finite() || distance <= depth || !aspect.is_finite() || aspect <= 0.0 { return None; }
    let transform = Transform::from_translation(center+normal*distance).looking_at(center, up);
    if !transform.translation.is_finite() || !transform.rotation.is_finite() { return None; }
    Some((transform, ((distance-depth)*0.1).max(0.001), distance+depth+(width+height).max(1.0)))
}

fn prepare(
    mut commands: Commands, sources: Sources,
    mut models: ModelCtx, mut owned: ResMut<Owned>,
    windows: Query<&Window>,
) {
    if owned.scene.is_some() { return; }
    let Some(request) = owned.request.clone() else { return; };
    // The early provider observer is the admission authority; publish checks again.
    if !owned.provider.as_ref().is_some_and(|p| p.matches(&sources)) { return; }
    if owned.attempted { return; }
    let Ok(window) = windows.get(request.stamp.window) else { return; };
    let physical = Rect::from_corners(Vec2::ZERO, window.resolution.physical_size().as_vec2());
    if !window.focused || !physical.contains(request.stamp.display.min)
        || !physical.contains(request.stamp.display.max)
        || request.target != nvf_play::pipboy::TARGET { return; }
    owned.attempted = true;
    let Some(path) = owned.model.clone() else {
        // A character who has not been given a Pip-Boy wears none; the Wrist
        // Terminal theme then keeps the conversation on Normal.
        info!("dialogue Pip-Boy: the player wears no Pip-Boy model; Wrist Terminal dialogue uses Normal");
        return;
    };
    let Some(model) = models.model_with_cpu(&path) else {
        warn!("dialogue Pip-Boy: actual model {path} not loaded"); return;
    };
    // Static prepared models already contain skinned shapes in their actual NIF bind pose.
    let place = Affine3A::from_translation(PLACE)
        * Affine3A::from_rotation_x(-std::f32::consts::FRAC_PI_2);
    let mut prepared = Vec::new();
    for shape in &model.shapes {
        let part = shape.name.as_deref().and_then(pipboy_arm::part);
        if part == Some(PipBoyPart::ScreenLit) { continue; }
        let transform = place * model.root * shape.transform;
        let Some(mesh) = shape.cpu.as_deref() else { return; };
        let Some(pose) = pipboy_arm::pointer_pose(mesh, None, Mat4::from(transform)) else { return; };
        prepared.push((shape, part, transform, Arc::new(pose)));
    }
    let screens = prepared.iter().filter(|(_,p,_,_)| *p == Some(PipBoyPart::Screen)).collect::<Vec<_>>();
    if screens.len() != 1 { return; }
    let glass = screens[0].3.clone();
    let bounds = glass.uvs.iter().copied().fold(None, |bounds: Option<Rect>, uv|
        Some(bounds.map_or(Rect::from_corners(uv,uv), |r| Rect::from_corners(r.min.min(uv),r.max.max(uv)))));
    let Some(uv) = bounds.filter(|r| r.min.is_finite() && r.max.is_finite() && r.width()>0.0 && r.height()>0.0) else { return; };
    let all = prepared.iter().map(|(_,_,_,p)| p.clone()).collect::<Vec<_>>();
    let display = request.stamp.display.size();
    let Some((camera_transform, near, far)) = fit(&glass, &all, display.x/display.y) else { return; };
    let size = display.ceil().as_uvec2();
    if size.x == 0 || size.y == 0 || size.max_element() > 8192 { return; }
    let mut image = Image::new_fill(Extent3d {width:size.x,height:size.y,depth_or_array_layers:1},
        TextureDimension::D2, &[0,0,0,0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
    image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
    let image = models.images.add(image);
    let layer = RenderLayers::layer(LAYER);
    let root = commands.spawn((Name::new("Dialogue-owned actual Pip-Boy"), Transform::IDENTITY, Visibility::Inherited)).id();
    let mut materials = Vec::new();
    let mut screen_material = None;
    let mut blockers = Vec::new();
    for (shape, part, transform, pose) in prepared {
        let mut material = shape.material.clone();
        if let Some(source) = models.materials.get(&shape.material) {
            let copy = match part {
                Some(PipBoyPart::Screen) => Some(pipboy_arm::menus_material(source, request.image.clone())),
                Some(PipBoyPart::Glare) => Some(pipboy_arm::glare_material(source)),
                _ => None,
            };
            if let Some(copy) = copy {
                material = models.materials.add(copy);
                materials.push(material.clone());
                if part == Some(PipBoyPart::Screen) { screen_material = Some(material.clone()); }
            }
        }
        if !matches!(part,Some(PipBoyPart::Screen|PipBoyPart::Glare)) { blockers.push(pose); }
        commands.spawn((Name::new(format!("Dialogue Pip-Boy {}",shape.name.as_deref().unwrap_or("shape"))),
            Mesh3d(shape.mesh.clone()), MeshMaterial3d(material), Transform::from_matrix(Mat4::from(transform)),
            layer.clone(), ChildOf(root)));
    }
    let camera = commands.spawn((Name::new("Dialogue Pip-Boy camera"), Camera3d::default(),
        Camera {order:-3,clear_color:ClearColorConfig::Custom(Color::NONE),..default()},
        RenderTarget::Image(ImageRenderTarget::from(image.clone())),
        Projection::Perspective(PerspectiveProjection {fov:FOV,near,far,..default()}),
        bevy::core_pipeline::tonemapping::Tonemapping::Linear, Msaa::Sample4,
        AmbientLight {color:Color::WHITE,brightness:150.0,..default()}, camera_transform,
        layer.clone(), ChildOf(root))).id();
    commands.spawn((DirectionalLight {illuminance:3000.0,shadow_maps_enabled:false,..default()},
        Transform::from_translation(camera_transform.translation).looking_at(glass.middle().unwrap_or(PLACE),Vec3::Y),
        layer, ChildOf(root)));
    let Some(screen_material) = screen_material else {
        commands.entity(root).despawn();
        models.images.remove(image.id());
        for material in materials { models.materials.remove(material.id()); }
        return;
    };
    owned.scene = Some(Scene {root,camera,image,materials,screen_material,glass,blockers,uv,
        projection:None,rejected:None});
    info!("dialogue Pip-Boy: resolved {path}, actual NIF bind pose, independent layer17");
}

fn camera_signature(
    scene: &Scene, request: &Request,
    cameras: &Query<(&Camera,&GlobalTransform,&RenderTarget)>, windows: &Query<&Window>,
) -> Option<ProjectionKey> {
    let (camera,transform,target) = cameras.get(scene.camera).ok()?;
    let window = windows.get(request.stamp.window).ok()?;
    let physical = Rect::from_corners(Vec2::ZERO,window.physical_size().as_vec2());
    if !camera.is_active || !window.focused || !physical.contains(request.stamp.display.min)
        || !physical.contains(request.stamp.display.max)
        || !matches!(target,RenderTarget::Image(image) if image.handle == scene.image) { return None; }
    let logical = camera.logical_viewport_rect()?;
    Some((camera.clip_from_view(),transform.affine(),Some(logical),camera.physical_target_size()))
}

fn publish(
    mut commands: Commands, request: Option<Res<Request>>, sources: Sources, mut owned: ResMut<Owned>,
    cameras: Query<(&Camera,&GlobalTransform,&RenderTarget)>, windows: Query<&Window>,
    mut materials: ResMut<Assets<NifMaterial>>,
) {
    let Some(current) = request.as_deref() else { return; };
    let Some(expected) = owned.request.as_ref() else { return; };
    if !same(current,expected) { return; }
    if !owned.provider.as_ref().is_some_and(|p| p.matches(&sources)) {
        remove_view(&mut commands,current); return;
    }
    let Some(scene) = owned.scene.as_mut() else { return; };
    let Some(signature) = camera_signature(scene,current,&cameras,&windows) else {
        scene.projection=None; scene.rejected=None;
        remove_view(&mut commands,current); return;
    };
    if scene.projection == Some(signature) || scene.rejected == Some(signature) { return; }
    if scene.projection.take().is_some() {
        // Give the raw reader a complete frame without a projection before rebinding.
        remove_view(&mut commands,current); return;
    }
    let Ok((camera,transform,_)) = cameras.get(scene.camera) else { return; };
    for step in 0..UV_ATTEMPTS {
        let uv_size = scene.uv.size() * (0.95_f32 * 0.9_f32.powi(step));
        let uv = Rect::from_center_size(scene.uv.center(),uv_size);
        let scale = current.canvas.size()/uv.size();
        let uv_to_canvas = Affine2::from_scale_angle_translation(scale,0.0,current.canvas.min-uv.min*scale);
        let Some((map,scale)) = GlassMap::new(scene.glass.clone(),scene.blockers.clone(),camera,transform,
            current.stamp.display,current.canvas,uv_to_canvas) else { continue; };
        let Some(mut material) = materials.get_mut(&scene.screen_material) else { break; };
        material.base.uv_transform = Affine2::from_scale(current.target.as_vec2().recip()) * uv_to_canvas;
        let stamp = current.stamp;
        let image = scene.image.clone();
        let camera = scene.camera;
        let root = scene.root;
        let expected = (*current).clone();
        commands.queue(move |world: &mut World| {
            let live = world.get_resource::<Owned>().is_some_and(|o|
                o.scene.as_ref().is_some_and(|s| s.root == root && s.projection.is_some()));
            if live && world.get_resource::<Request>().is_some_and(|r| same(r,&expected)) {
                world.insert_resource(DeviceView {stamp,image,camera,scale,map:Arc::new(map)});
            }
        });
        scene.projection=Some(signature);
        info!("dialogue Pip-Boy: calibrated actual glass UV {uv:?}, display {:?}, conservative pixels/canvas {scale}",current.stamp.display);
        return;
    }
    scene.rejected=Some(signature);
    remove_view(&mut commands,current);
    warn!("dialogue Pip-Boy: no fully covered calibrated glass projection; no view published");
}

#[cfg(test)]
mod tests;
