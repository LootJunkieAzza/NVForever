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
//!   (1.0 s) is played as a pin goes in [hypothesis: by its name and length], backwards
//!   as the player leaves with a pin in [design]; a broken pin plays `Backward` (0.97 s,
//!   it twists) then `Left` (0.47 s, `Pin2` flies off, hidden at the end), the short
//!   piece staying in the keyhole until the next pin [hypothesis: the keys of the two
//!   sequences meet; observed: a short piece in the keyhole after a break on the owner's
//!   screenshot] (`nvf_play::lockpick::PinClip`). The two files share one space
//!   [hypothesis: the pin's last pose ends at the keyhole].
//! - Turning: once the lock is in, its `Backward` sequence (2.97 s: the cylinder turns a
//!   quarter turn while the screwdriver follows with a twist of the wrist) is posed at the
//!   time the lock's turn has reached (half turned, half way through) [hypothesis: the
//!   sequence turns the cylinder by exactly the full turn; observed: on the owner's
//!   screenshots the screwdriver is still nearly level with the lock 40° round and points
//!   down with it all the way round, as the sequence has it]. The pin turns by its own
//!   angle from upright only (it stays where the player put it while the lock turns
//!   [observed: the same screenshots]), about the cylinder's axis through the cylinder's
//!   origin [hypothesis: the axis is the files' +Y, the way the camera looks], clockwise as
//!   seen [hypothesis].
//! - The camera looks along +Y at the cylinder from in front [design: the original's
//!   camera is not in the files; its distance and field of view are fitted by eye to the
//!   owner's screenshots, where the cylinder fills about 0.72 of the view's height and
//!   sits a little above the middle], with lights of its own on a render layer of their
//!   own [design: the files' two point lights and ambient light stand in, not read]. The
//!   picture is clear where nothing is drawn: the room shows dimly behind the lock
//!   [observed: the owner's screenshots].

use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_game::lockpick::{FULL_TURN, PIN_RANGE};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::{ModelCtx, Rig};
use nvf_play::lockpick::{LockView, LockpickScreen, PinClip, LEAVE_SECONDS};
use nvf_play::pause::AidTime;
use std::collections::HashMap;

pub const LOCK_MODEL: &str = "meshes\\terminals\\lockinterface01.nif";
pub const PIN_MODEL: &str = "meshes\\terminals\\bobbypin01.nif";
/// The render layer of the lock, its lights and its camera.
pub const LOCK_LAYER: usize = 7;
/// Where the lock is put, far from the world (Bevy space) [design].
const PLACE: Vec3 = Vec3::new(0.0, -200_000.0, 0.0);
/// The camera: this far in front of the cylinder (game units), with this field of view,
/// aimed this far under the cylinder's centre [design: fitted by eye to the owner's
/// screenshots].
const CAMERA_DISTANCE: f32 = 70.0;
const FOV_DEG: f32 = 17.0;
const AIM_BELOW: f32 = 0.7;
/// The pin model's sequences.
const PIN_CLIPS: [&str; 3] = ["Forward", "Backward", "Left"];

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
        let mut posed: Vec<usize> =
            PIN_CLIPS.iter().filter_map(|c| rig.group(c)).flat_map(|c| c.tracks.iter().filter_map(|(n, _)| node_index(&rig, n))).collect();
        posed.extend(turned.iter().copied());
        posed.sort_unstable();
        posed.dedup();
        Some(Model { rig, by_name, posed, turned })
    }

    /// Pose the nodes: the sequence `clip` at `t`, the turning nodes turned by `turn`
    /// about the axis through `centre`.
    fn pose(&self, clip: &str, t: f32, centre: Vec3, turn: f32, transforms: &mut Query<&mut Transform>) {
        let clip = self.rig.group(clip);
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

    /// A sequence's length, seconds (0 without it).
    fn length(&self, clip: &str) -> f32 {
        self.rig.group(clip).map_or(0.0, |c| (c.stop - c.start) / c.frequency.max(f32::EPSILON))
    }

    fn entity(&self, name: &str) -> Option<Entity> {
        self.by_name.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|(_, e)| *e)
    }
}

/// The pin's sequence, the time in it, and whether the long piece is still drawn.
fn pin_pose(pin: &Model, clip: PinClip, t: f32) -> (&'static str, f32, bool) {
    match clip {
        PinClip::In => ("Forward", t, true),
        PinClip::Out => {
            let len = pin.length("Forward");
            ("Forward", (len - t * len / LEAVE_SECONDS).max(0.0), true)
        }
        PinClip::Break => {
            let twist = pin.length("Backward");
            if t < twist {
                ("Backward", t, true)
            } else {
                ("Left", t - twist, t - twist < pin.length("Left"))
            }
        }
    }
}

struct Parts {
    lock: Model,
    pin: Model,
    /// The cylinder's place, the centre of every turn.
    centre: Vec3,
    /// The pin's long piece (`Pin2`), hidden once it has flown off.
    pin_long: Option<Entity>,
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
        let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 0], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image = models.images.add(image);
        let layer = RenderLayers::layer(LOCK_LAYER);
        let holder = commands.spawn((Name::new("Lockpicking lock"), Transform::from_translation(PLACE), Visibility::Inherited)).id();
        // Game space (Z up) into Bevy's (Y up), as the world root does.
        let to_bevy = Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        let (_, lock_drawn, lock_names) = lock_rig.spawn_nodes(&mut commands, holder, to_bevy, LOCK_MODEL, 0.0);
        let (_, pin_drawn, pin_names) = pin_rig.spawn_nodes(&mut commands, holder, to_bevy, PIN_MODEL, 0.0);
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
        let aim = centre - Vec3::new(0.0, 0.0, AIM_BELOW);
        let to = |p: Vec3| PLACE + nvf_play::space::to_bevy(p.to_array());
        let camera = commands
            .spawn((
                Name::new("Lockpicking camera"),
                Camera3d::default(),
                Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::NONE), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(image.clone())),
                Projection::Perspective(PerspectiveProjection { fov: FOV_DEG.to_radians(), near: 1.0, far: 2000.0, ..default() }),
                Tonemapping::None,
                Msaa::Sample4,
                AmbientLight { color: Color::WHITE, brightness: 250.0, ..default() },
                Transform::from_translation(to(eye)).looking_at(to(aim), Vec3::Y),
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
        let pin_long = pin_model.entity("Pin2");
        drawn.parts = Some(Parts { lock: lock_model, pin: pin_model, centre, pin_long });
        commands.insert_resource(LockView { image });
        info!("lockpick view: the lock drawn ({} and {} shapes), the camera {CAMERA_DISTANCE} in front of the cylinder at {centre:?}", lock_drawn.len(), pin_drawn.len());
    }
    let Some(p) = drawn.parts.as_ref() else { return };
    let now = time.elapsed_secs();
    let g = &s.game;
    // In, the lock's `Forward`; then its `Backward`, which turns the cylinder a quarter turn
    // and moves the screwdriver with it, at the time the lock's turn has reached.
    let since = now - s.opened;
    if since < p.lock.length("Forward") {
        p.lock.pose("Forward", since, p.centre, g.turn, &mut transforms);
    } else {
        p.lock.pose("Backward", g.turn / FULL_TURN * p.lock.length("Backward"), p.centre, 0.0, &mut transforms);
    }
    // The pin turns from upright by its own angle only: it stays put as the lock turns.
    let (clip, t, long) = pin_pose(&p.pin, s.pin_clip, now - s.pin_since);
    p.pin.pose(clip, t, p.centre, g.pin - PIN_RANGE / 2.0, &mut transforms);
    if let Some(v) = p.pin_long.and_then(|e| visibility.get_mut(e).ok()) {
        let mut v = v;
        v.set_if_neq(if long { Visibility::Inherited } else { Visibility::Hidden });
    }
}
