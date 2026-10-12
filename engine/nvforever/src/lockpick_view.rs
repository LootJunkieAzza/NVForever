//! The lock of the lockpicking menu (`nvf_play::lockpick`, docs/spec-notes/lockpicking.md
//! §2): the game's models of the lock and the bobby pin, drawn by a camera of their own
//! into a picture the menu shows behind its text ([`nvf_play::lockpick::LockView`]).
//!
//! - `meshes\terminals\lockinterface01.nif` holds the plate (`frame`), the lock cylinder
//!   (`lock`, a child of the plate) and the screwdriver (`screwdriver`); its `Forward`
//!   sequence (1.33 s) brings the three in, its `Backward` takes them away
//!   [observed: the file's nodes and sequences]. The pose is the `Forward` sequence's,
//!   played from when the menu opens and held at its end [hypothesis: the original plays
//!   it as the menu opens; its sound key `UILockpickingEnter` is played by the menu].
//! - `meshes\terminals\bobbypin01.nif` holds the pin (`bobbypin`, `Pin2`); its `Forward`
//!   (1.0 s) is played as a pin goes in [hypothesis: by its name and length]. The two
//!   files share one space [hypothesis: the pin's last pose ends at the keyhole].
//! - Turning: the cylinder and the screwdriver turn by the lock's angle, the pin by its
//!   own angle from upright plus the lock's, all about the cylinder's axis through the
//!   cylinder's origin [hypothesis: the axis is the files' +Y, the way the camera looks],
//!   clockwise as seen [hypothesis].
//! - The camera looks along +Y at the cylinder from in front [design: the original's
//!   camera is not in the files], with lights of its own on a render layer of their own
//!   [design: the files' two point lights and ambient light stand in, not read].

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_game::lockpick::{Phase, PIN_RANGE};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::{ModelCtx, Rig};
use nvf_play::lockpick::{LockView, LockpickScreen};
use nvf_play::pause::AidTime;
use std::collections::HashMap;

pub const LOCK_MODEL: &str = "meshes\\terminals\\lockinterface01.nif";
pub const PIN_MODEL: &str = "meshes\\terminals\\bobbypin01.nif";
/// The render layer of the lock, its lights and its camera.
pub const LOCK_LAYER: usize = 7;
/// Where the lock is put, far from the world (Bevy space) [design].
const PLACE: Vec3 = Vec3::new(0.0, -200_000.0, 0.0);
/// The camera: this far in front of the cylinder (game units), with this field of view
/// [design: the plate about fills the view].
const CAMERA_DISTANCE: f32 = 45.0;
const FOV_DEG: f32 = 55.0;

pub struct LockpickViewPlugin;

impl Plugin for LockpickViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync));
    }
}

/// One of the two models as drawn: its rig, its nodes' entities by name, the nodes its
/// sequence moves and the node that turns.
struct Model {
    rig: Rig,
    by_name: HashMap<String, Entity>,
    /// Nodes posed each frame: those the `Forward` sequence moves and the turning ones.
    posed: Vec<usize>,
    turned: Vec<usize>,
}

impl Model {
    fn new(rig: Rig, by_name: HashMap<String, Entity>, turned: &[&str]) -> Option<Model> {
        let turned: Vec<usize> = turned.iter().map(|n| node_index(&rig, n)).collect::<Option<_>>()?;
        let mut posed: Vec<usize> = rig.group("Forward").map(|c| c.tracks.iter().filter_map(|(n, _)| node_index(&rig, n)).collect()).unwrap_or_default();
        posed.extend(turned.iter().copied());
        posed.sort_unstable();
        posed.dedup();
        Some(Model { rig, by_name, posed, turned })
    }

    /// Pose the nodes: `Forward` at `t`, the turning nodes turned by `turn` about the
    /// axis through `centre`.
    fn pose(&self, t: f32, centre: Vec3, turn: f32, transforms: &mut Query<&mut Transform>) {
        let clip = self.rig.group("Forward");
        // Played once from its start, then held at its end.
        let t = clip.map_or(0.0, |c| (c.start + t.max(0.0) * c.frequency).min(c.stop));
        let m = turn_about(centre, turn);
        let mut world: Vec<Affine3A> = Vec::with_capacity(self.rig.nodes.len());
        for (i, n) in self.rig.nodes.iter().enumerate() {
            let local = if i == 0 { Affine3A::IDENTITY } else { sampled(&self.rig, clip, i, t).compute_affine() };
            let parent = n.parent.map_or(Affine3A::IDENTITY, |p| world[p]);
            let w = parent * local;
            world.push(if self.turned.contains(&i) { m * w } else { w });
        }
        for &i in &self.posed {
            let Some(e) = self.rig.nodes[i].name.as_ref().and_then(|n| self.by_name.get(n)) else { continue };
            let parent = self.rig.nodes[i].parent.map_or(Affine3A::IDENTITY, |p| world[p]);
            if let Ok(mut tr) = transforms.get_mut(*e) {
                *tr = local_of(world[i], parent);
            }
        }
    }
}

struct Parts {
    lock: Model,
    pin: Model,
    /// The cylinder's place, the centre of every turn.
    centre: Vec3,
    pin_root: Entity,
}

#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    parts: Option<Parts>,
    warned: bool,
}

fn node_index(rig: &Rig, name: &str) -> Option<usize> {
    rig.nodes.iter().position(|n| n.name.as_deref().is_some_and(|m| m.eq_ignore_ascii_case(name)))
}

/// A node's local transform at `t` of `clip`, else its stored one.
fn sampled(rig: &Rig, clip: Option<&Clip>, i: usize, t: f32) -> Transform {
    let mut tr = rig.nodes[i].local;
    let Some(name) = rig.nodes[i].name.as_deref() else { return tr };
    let Some((_, track)) = clip.and_then(|c| c.tracks.iter().find(|(n, _)| n.eq_ignore_ascii_case(name))) else { return tr };
    if let Some(v) = track.translation.sample(t) {
        tr.translation = Vec3::from_array(v);
    }
    if let Some([w, x, y, z]) = track.rotation_at(t) {
        tr.rotation = Quat::from_xyzw(x, y, z, w).normalize();
    }
    if let Some([s]) = track.scale.sample(t) {
        tr.scale = Vec3::splat(s);
    }
    tr
}

/// Every node's transform in the model's space (the root's own left out), posed by
/// `clip` at `t`.
fn model_space(rig: &Rig, clip: Option<&Clip>, t: f32) -> Vec<Affine3A> {
    let mut out: Vec<Affine3A> = Vec::with_capacity(rig.nodes.len());
    for (i, n) in rig.nodes.iter().enumerate() {
        let local = if i == 0 { Affine3A::IDENTITY } else { sampled(rig, clip, i, t).compute_affine() };
        let parent = n.parent.map_or(Affine3A::IDENTITY, |p| out[p]);
        out.push(parent * local);
    }
    out
}

/// A turn of `deg` degrees about the cylinder's axis (+Y) through `centre`, clockwise
/// as seen from in front (looking along +Y).
fn turn_about(centre: Vec3, deg: f32) -> Affine3A {
    Affine3A::from_translation(centre) * Affine3A::from_quat(Quat::from_rotation_y(deg.to_radians())) * Affine3A::from_translation(-centre)
}

fn local_of(world: Affine3A, parent: Affine3A) -> Transform {
    Transform::from_matrix(Mat4::from(parent.inverse() * world))
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    screen: Res<LockpickScreen>,
    view: Option<Res<LockView>>,
    mut models: ModelCtx,
    window: Query<&Window, With<PrimaryWindow>>,
    time: AidTime,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut drawn: Local<Drawn>,
) {
    let Some(s) = screen.0.as_ref() else {
        if !drawn.entities.is_empty() || view.is_some() {
            for e in drawn.entities.drain(..) {
                commands.entity(e).despawn();
            }
            drawn.parts = None;
            commands.remove_resource::<LockView>();
        }
        return;
    };
    if drawn.entities.is_empty() {
        let (Some(lock), Some(pin)) = (models.model(LOCK_MODEL), models.model(PIN_MODEL)) else {
            if !drawn.warned {
                warn!("lockpick view: {LOCK_MODEL} or {PIN_MODEL} not loaded; the menu is drawn without the lock");
                drawn.warned = true;
            }
            return;
        };
        let (Some(lock_rig), Some(pin_rig)) = (lock.rig.clone(), pin.rig.clone()) else {
            if !drawn.warned {
                warn!("lockpick view: the lock's or the pin's model has no sequences");
                drawn.warned = true;
            }
            return;
        };
        let size = window.single().map(|w| Extent3d { width: w.physical_width().max(1), height: w.physical_height().max(1), depth_or_array_layers: 1 }).unwrap_or(Extent3d { width: 1280, height: 720, depth_or_array_layers: 1 });
        let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image = models.images.add(image);
        let layer = RenderLayers::layer(LOCK_LAYER);
        let holder = commands.spawn((Name::new("Lockpicking lock"), Transform::from_translation(PLACE), Visibility::Inherited)).id();
        // Game space (Z up) into Bevy's (Y up), as the world root does.
        let to_bevy = Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        let (_, lock_drawn, lock_names) = lock_rig.spawn_nodes(&mut commands, holder, to_bevy, LOCK_MODEL, 0.0);
        let (pin_root, pin_drawn, pin_names) = pin_rig.spawn_nodes(&mut commands, holder, to_bevy, PIN_MODEL, 0.0);
        for e in lock_drawn.iter().chain(&pin_drawn) {
            commands.entity(*e).insert(layer.clone());
        }
        // Where the cylinder ends up: the camera looks at it.
        let forward = lock_rig.group("Forward");
        let end = model_space(&lock_rig, forward, forward.map_or(0.0, |c| c.stop));
        let centre: Option<Vec3> = node_index(&lock_rig, "lock").map(|i| end[i].translation.into());
        let models = (Model::new(lock_rig, lock_names, &["lock", "screwdriver"]), Model::new(pin_rig, pin_names, &["bobbypin"]));
        let (Some(centre), (Some(lock_model), Some(pin_model))) = (centre, models) else {
            warn!("lockpick view: the models lack the lock, screwdriver or bobbypin node");
            commands.entity(holder).despawn();
            drawn.warned = true;
            return;
        };
        let eye = centre + Vec3::new(0.0, -CAMERA_DISTANCE, 0.0);
        let to = |p: Vec3| PLACE + nvf_play::space::to_bevy(p.to_array());
        let camera = commands
            .spawn((
                Name::new("Lockpicking camera"),
                Camera3d::default(),
                Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(image.clone())),
                Projection::Perspective(PerspectiveProjection { fov: FOV_DEG.to_radians(), near: 1.0, far: 2000.0, ..default() }),
                Tonemapping::None,
                Msaa::Sample4,
                AmbientLight { color: Color::WHITE, brightness: 250.0, ..default() },
                Transform::from_translation(to(eye)).looking_at(to(centre), Vec3::Y),
                layer.clone(),
            ))
            .id();
        // A key light from the upper left in front and a fill from the right [design].
        let key = commands
            .spawn((
                DirectionalLight { illuminance: 6000.0, ..default() },
                Transform::from_translation(to(centre + Vec3::new(-40.0, -60.0, 50.0))).looking_at(to(centre), Vec3::Y),
                layer.clone(),
            ))
            .id();
        let fill = commands
            .spawn((
                DirectionalLight { illuminance: 1500.0, ..default() },
                Transform::from_translation(to(centre + Vec3::new(60.0, -40.0, -10.0))).looking_at(to(centre), Vec3::Y),
                layer.clone(),
            ))
            .id();
        drawn.entities = vec![holder, camera, key, fill];
        drawn.parts = Some(Parts { lock: lock_model, pin: pin_model, centre, pin_root });
        commands.insert_resource(LockView { image });
        info!("lockpick view: the lock drawn ({} and {} shapes), the camera {CAMERA_DISTANCE} in front of the cylinder at {centre:?}", lock_drawn.len(), pin_drawn.len());
    }
    let Some(p) = drawn.parts.as_ref() else { return };
    let now = time.elapsed_secs();
    let g = &s.game;
    p.lock.pose(now - s.opened, p.centre, g.turn, &mut transforms);
    // The pin turns from upright by its own angle and with the cylinder.
    p.pin.pose(now - s.pin_in, p.centre, g.pin - PIN_RANGE / 2.0 + g.turn, &mut transforms);
    let pin_in = matches!(g.phase, Phase::Picking | Phase::Open);
    if let Ok(mut v) = visibility.get_mut(p.pin_root) {
        v.set_if_neq(if pin_in { Visibility::Inherited } else { Visibility::Hidden });
    }
}
