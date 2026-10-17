//! The roulette table of the roulette menu (`nvf_play::roulette`,
//! docs/spec-notes/roulette.md §6): the game's models drawn by a camera of their own into
//! a picture the menu shows behind its text ([`nvf_play::roulette::RouletteView`]).
//!
//! - The table is the casino's roulette model (CSNO MOD4, `NV_Roulette-Table<x>.NIF`): the
//!   felt, the wheel (`Roulette_Wheel`) in its housing, the ball (`Ball`, under the turning
//!   `Point01`) and a camera (`Camera01`, aimed at `Camera01.Target`); its `Spin_1`–`Spin_3`
//!   sequences (9.83 s each) turn the wheel, run the ball round and down into a pocket, and
//!   fly the camera from above the felt to the wheel [observed: the file's nodes and
//!   sequences]. Betting, the model stands in its stored pose; a spin plays one of the three
//!   (`nvf_play::roulette::Phase::Spinning`), held at its end while the result shows.
//! - The ball comes to rest at the same place on the wheel in a given sequence; the wheel is
//!   turned about its axis by the angle that brings the spun pocket under it
//!   [hypothesis: the original's way of matching the result is not known]. A pocket's angle
//!   on the wheel: the number strip (`nv_roulette03.dds`, `fnv_game::roulette::WHEEL_STRIP`)
//!   wraps the wheel's shape once, its left edge at −15.33° about the wheel's Z axis from
//!   its X axis, counter-clockwise as the picture runs, 38 equal cells [observed:
//!   `fnvtool nif decode` of the wheel's shape: texture U against vertex angle].
//! - The camera: the model's own, from `Camera01` towards `Camera01.Target` (a look-at
//!   sequence), its view 34.5° high [observed: the NiCamera's frustum, ±0.311 at a near
//!   plane of 1; hypothesis: the height is kept on wider screens].
//! - The bet points: the nodes of `nv_roulette-points.nif`, placed with the table's origin
//!   [hypothesis: checked by eye, the points fall on the felt's numbers]. The marker is
//!   `nv_roulette-bet.nif` at the point under it (`Bet_Selector`, or `Bet_Invalid` where the
//!   current bet cannot go [hypothesis: by the names]); each bet a stack of
//!   `nv_roulette-chip.nif`, as tall as the chips it takes in 100s, 25s, 10s, 5s and 1s, a
//!   tenth of the model's height a chip, at most the whole model [design].
//! - Lights: a key light from above the player's side and ambient light, on the render
//!   layer of its own [design: the file's three point lights (`Omni01`–`03`, riding with
//!   the camera) are not read].
//! - The pointer: a ray from the camera through the pointer meets the felt's plane; the
//!   nearest bet point within reach is the one under the pointer
//!   ([`nvf_play::roulette::RoulettePointer`]).

use crate::lockpick_view::{local_of, node_index, sampled};
use bevy::asset::RenderAssetUsages;
use bevy::camera::visibility::RenderLayers;
use bevy::camera::{ImageRenderTarget, RenderTarget};
use bevy::core_pipeline::tonemapping::Tonemapping;
use bevy::light::AmbientLight;
use bevy::math::Affine3A;
use bevy::prelude::*;
use bevy::render::render_resource::{Extent3d, TextureDimension, TextureFormat, TextureUsages};
use bevy::window::PrimaryWindow;
use fnv_game::roulette::{self as rules, WHEEL_STRIP};
use nvf_assets::anim_play::Clip;
use nvf_assets::model::{ModelCtx, PreparedModel, Rig};
use nvf_play::pause::AidTime;
use nvf_play::roulette::{Phase, RoulettePointer, RouletteScreen, RouletteView};
use std::collections::HashMap;
use std::sync::Arc;

pub const DEFAULT_TABLE: &str = "meshes\\terminals\\nv_roulette\\nv_roulette-table.nif";
pub const POINTS_MODEL: &str = "meshes\\terminals\\nv_roulette\\nv_roulette-points.nif";
pub const BET_MODEL: &str = "meshes\\terminals\\nv_roulette\\nv_roulette-bet.nif";
pub const CHIP_MODEL: &str = "meshes\\terminals\\nv_roulette\\nv_roulette-chip.nif";
/// The render layer of the table, its lights and its camera (lockpicking has 7, poker 8).
pub const ROULETTE_LAYER: usize = 10;
/// Where the table is put, far from the world and from the lock (Bevy space) [design].
const PLACE: Vec3 = Vec3::new(0.0, -200_000.0, 40_000.0);
/// The camera's view height, degrees: 2 atan(0.31066) [observed: the NiCamera's frustum].
const VFOV_DEG: f32 = 34.5;
/// Where the wheel's number strip starts, degrees about the wheel's Z axis (see above).
const STRIP_START_DEG: f32 = -15.33;
/// How near the pointer must come to a bet point, game units: the grid's (two across) and
/// the outside places' [design].
const REACH_GRID: f32 = 1.3;
const REACH_OUTSIDE: f32 = 4.5;
/// The chip model's height in chips [design].
const CHIPS_PER_MODEL: f32 = 10.0;

pub struct RouletteViewPlugin;

impl Plugin for RouletteViewPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, show.after(nvf_play::PlaySet::Ui).before(nvf_play::PlaySet::Sync));
    }
}

struct Table {
    rig: Rig,
    by_name: HashMap<String, Entity>,
    wheel: usize,
    camera: usize,
    target: usize,
    /// For each spin sequence (1–3): the ball's angle on the wheel at its end, degrees.
    ball_end: [Option<f32>; 3],
}

impl Table {
    fn clip(&self, n: u8) -> Option<&Clip> {
        self.rig.group(&format!("Spin_{n}"))
    }

    /// Every node in model space, posed by `clip` at `t`, the wheel turned by `turn`
    /// degrees about its own Z axis.
    fn space(&self, clip: Option<&Clip>, t: f32, turn: f32) -> Vec<Affine3A> {
        let mut out: Vec<Affine3A> = Vec::with_capacity(self.rig.nodes.len());
        for (i, n) in self.rig.nodes.iter().enumerate() {
            let local = if i == 0 { Affine3A::IDENTITY } else { sampled(&self.rig, clip, i, t).compute_affine() };
            let parent = n.parent.map_or(Affine3A::IDENTITY, |p| out[p]);
            let mut w = parent * local;
            if i == self.wheel && turn != 0.0 {
                w *= Affine3A::from_rotation_z(turn.to_radians());
            }
            out.push(w);
        }
        out
    }

    fn pose(&self, world: &[Affine3A], transforms: &mut Query<&mut Transform>) {
        for (i, n) in self.rig.nodes.iter().enumerate().skip(1) {
            let Some(e) = n.name.as_ref().and_then(|name| self.by_name.get(name)) else { continue };
            let parent = n.parent.map_or(Affine3A::IDENTITY, |p| world[p]);
            if let Ok(mut tr) = transforms.get_mut(*e) {
                *tr = local_of(world[i], parent);
            }
        }
    }
}

/// A clip's time `elapsed` seconds in, held at its end.
fn clip_time(c: Option<&Clip>, elapsed: f32) -> f32 {
    c.map_or(0.0, |c| (c.start + elapsed.max(0.0) * c.frequency).min(c.stop))
}

/// A pocket's angle on the wheel, degrees.
pub fn pocket_angle(pocket: u8) -> f32 {
    let k = WHEEL_STRIP.iter().position(|&p| p == pocket).unwrap_or(0) as f32;
    STRIP_START_DEG + 360.0 * (k + 0.5) / WHEEL_STRIP.len() as f32
}

/// The pocket at an angle on the wheel.
pub fn pocket_at(angle: f32) -> u8 {
    let n = WHEEL_STRIP.len() as f32;
    let k = ((angle - STRIP_START_DEG).rem_euclid(360.0) / 360.0 * n).floor() as usize;
    WHEEL_STRIP[k.min(WHEEL_STRIP.len() - 1)]
}

/// How far the chip model reaches under its origin (its height).
fn chip_depth(m: &PreparedModel) -> f32 {
    m.bounds.map_or(2.0, |(lo, hi)| (hi.z - lo.z).max(0.01))
}

/// The chips a bet's stack shows: the amount in 100s, 25s, 10s, 5s and 1s [design].
pub fn stack_chips(amount: u32) -> u32 {
    let mut left = amount;
    let mut n = 0;
    for d in [100, 25, 10, 5, 1] {
        n += left / d;
        left %= d;
    }
    n
}

#[derive(Default)]
struct Drawn {
    entities: Vec<Entity>,
    table: Option<Table>,
    /// Bet points by name, game space (model space of the table).
    points: HashMap<String, Vec3>,
    /// The parent of the chips and the marker (game space).
    space: Option<Entity>,
    /// The marker's two parts: the selector's shapes and the invalid sign's.
    marker: Option<Entity>,
    selector: Vec<Entity>,
    invalid: Vec<Entity>,
    chips: Vec<Entity>,
    chips_key: String,
    chip_model: Option<Arc<PreparedModel>>,
    camera: Option<Entity>,
    /// The camera in game space: eye and target, for the pointer's ray.
    eye: (Vec3, Vec3),
    cursor: Option<Vec2>,
    warned: bool,
}

/// Spawn a model's shapes (no sequences) under `parent` at `at` (game space); each entity
/// with its shape's name and the names of the nodes above it.
fn spawn_shapes(commands: &mut Commands, m: &PreparedModel, parent: Entity, layer: &RenderLayers) -> Vec<(Entity, Vec<String>)> {
    m.shapes
        .iter()
        .map(|s| {
            let names: Vec<String> = s.name.iter().cloned().chain(s.ancestors.iter().cloned()).collect();
            let t = Transform::from_matrix(Mat4::from(m.root * s.transform));
            let e = commands.spawn((Mesh3d(s.mesh.clone()), MeshMaterial3d(s.material.clone()), t, Visibility::Inherited, layer.clone(), ChildOf(parent))).id();
            (e, names)
        })
        .collect()
}

#[allow(clippy::too_many_arguments)]
fn show(
    mut commands: Commands,
    screen: Res<RouletteScreen>,
    view: Option<Res<RouletteView>>,
    mut models: ModelCtx,
    window: Query<&Window, With<PrimaryWindow>>,
    time: AidTime,
    mut pointer: ResMut<RoulettePointer>,
    mut transforms: Query<&mut Transform>,
    mut visibility: Query<&mut Visibility>,
    mut drawn: Local<Drawn>,
) {
    let Some(s) = screen.0.as_ref() else {
        if !drawn.entities.is_empty() || view.is_some() {
            for e in drawn.entities.drain(..) {
                commands.entity(e).despawn();
            }
            *drawn = Drawn::default();
            commands.remove_resource::<RouletteView>();
            pointer.point = None;
        }
        return;
    };
    let layer = RenderLayers::layer(ROULETTE_LAYER);
    if drawn.entities.is_empty() {
        let path = s.model.as_deref().map_or_else(|| DEFAULT_TABLE.to_string(), |m| format!("meshes\\{m}"));
        let loaded = (models.model(&path), models.model(POINTS_MODEL), models.model(BET_MODEL), models.model(CHIP_MODEL));
        let (Some(table), Some(points), Some(bet), Some(chip)) = loaded else {
            if !drawn.warned {
                warn!("roulette view: {path}, {POINTS_MODEL}, {BET_MODEL} or {CHIP_MODEL} not loaded; the menu is drawn without the table");
                drawn.warned = true;
            }
            return;
        };
        let (Some(rig), Some(point_rig)) = (table.rig.clone(), points.rig.clone()) else {
            if !drawn.warned {
                warn!("roulette view: the table's or the points' model has no sequences");
                drawn.warned = true;
            }
            return;
        };
        let names = ["Roulette_Wheel", "Ball", "Camera01", "Camera01.Target"].map(|n| node_index(&rig, n));
        let [Some(wheel), Some(ball), Some(camera_node), Some(target)] = names else {
            warn!("roulette view: {path} lacks the wheel, the ball or the camera");
            drawn.warned = true;
            return;
        };
        let size = window.single().map(|w| Extent3d { width: w.physical_width().max(1), height: w.physical_height().max(1), depth_or_array_layers: 1 }).unwrap_or(Extent3d { width: 1280, height: 720, depth_or_array_layers: 1 });
        let mut image = Image::new_fill(size, TextureDimension::D2, &[0, 0, 0, 255], TextureFormat::Bgra8UnormSrgb, RenderAssetUsages::default());
        image.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST | TextureUsages::RENDER_ATTACHMENT;
        let image = models.images.add(image);
        let holder = commands.spawn((Name::new("Roulette table"), Transform::from_translation(PLACE), Visibility::Inherited)).id();
        // Game space (Z up) into Bevy's (Y up), as the world root does.
        let to_bevy = Transform::from_rotation(Quat::from_rotation_x(-std::f32::consts::FRAC_PI_2));
        let (_, table_drawn, by_name) = rig.spawn_nodes(&mut commands, holder, to_bevy, &path, 0.0);
        for e in &table_drawn {
            commands.entity(*e).insert(layer.clone());
        }
        let space = commands.spawn((to_bevy, Visibility::Inherited, ChildOf(holder))).id();
        // The marker.
        let marker = commands.spawn((Transform::default(), Visibility::Hidden, ChildOf(space))).id();
        let parts = spawn_shapes(&mut commands, &bet, marker, &layer);
        let is_invalid = |names: &[String]| names.iter().any(|n| n.to_ascii_lowercase().starts_with("bet_invalid"));
        let selector: Vec<Entity> = parts.iter().filter(|(_, n)| !is_invalid(n)).map(|(e, _)| *e).collect();
        let invalid: Vec<Entity> = parts.iter().filter(|(_, n)| is_invalid(n)).map(|(e, _)| *e).collect();
        // The bet points.
        let point_space = crate::lockpick_view::model_space(&point_rig, None, 0.0);
        let points: HashMap<String, Vec3> = point_rig
            .nodes
            .iter()
            .zip(&point_space)
            .filter_map(|(n, w)| {
                let name = n.name.as_ref()?.to_ascii_uppercase();
                rules::spot(&name).map(|_| (name, Vec3::from(w.translation)))
            })
            .collect();
        let camera = commands
            .spawn((
                Name::new("Roulette camera"),
                Camera3d::default(),
                Camera { order: -3, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
                RenderTarget::Image(ImageRenderTarget::from(image.clone())),
                Projection::Perspective(PerspectiveProjection { fov: VFOV_DEG.to_radians(), near: 1.0, far: 4000.0, ..default() }),
                Tonemapping::None,
                Msaa::Sample4,
                AmbientLight { color: Color::WHITE, brightness: 350.0, ..default() },
                Transform::from_translation(PLACE),
                layer.clone(),
            ))
            .id();
        let to = |p: Vec3| PLACE + nvf_play::space::to_bevy(p.to_array());
        let key = commands
            .spawn((DirectionalLight { illuminance: 5000.0, ..default() }, Transform::from_translation(to(Vec3::new(10.0, -60.0, 90.0))).looking_at(to(Vec3::ZERO), Vec3::Y), layer.clone()))
            .id();
        let mut t = Table { rig, by_name, wheel, camera: camera_node, target, ball_end: [None; 3] };
        for n in 1..=3u8 {
            let Some(c) = t.clip(n) else { continue };
            let end = t.space(Some(c), c.stop, 0.0);
            let local = end[wheel].inverse().transform_point3(end[ball].translation.into());
            t.ball_end[n as usize - 1] = Some(local.y.atan2(local.x).to_degrees());
        }
        info!(
            "roulette view: {path} drawn ({} shapes), {} bet points, chip model {} shapes (bounds {:?}), ball at the spins' ends: {}",
            table_drawn.len(),
            points.len(),
            chip.shapes.len(),
            chip.bounds,
            t.ball_end.iter().map(|a| a.map_or("none".to_string(), |a| format!("{a:.1}° (pocket {})", rules::pocket_name(pocket_at(a))))).collect::<Vec<_>>().join(", ")
        );
        drawn.entities = vec![holder, camera, key];
        drawn.table = Some(t);
        drawn.points = points;
        drawn.space = Some(space);
        drawn.marker = Some(marker);
        drawn.selector = selector;
        drawn.invalid = invalid;
        drawn.chip_model = Some(chip);
        drawn.camera = Some(camera);
        commands.insert_resource(RouletteView { image });
    }
    let now = time.elapsed_secs();
    let Some(t) = drawn.table.as_ref() else { return };
    // The table's pose.
    let world = match s.phase {
        Phase::Betting => t.space(None, 0.0, 0.0),
        Phase::Spinning { since, clip, pocket } | Phase::Paid { since, clip, pocket, .. } => {
            let c = t.clip(clip);
            let turn = t.ball_end[clip as usize - 1].map_or(0.0, |b| b - pocket_angle(pocket));
            t.space(c, clip_time(c, now - since), turn)
        }
    };
    t.pose(&world, &mut transforms);
    // The camera: from Camera01 towards its target.
    let eye = Vec3::from(world[t.camera].translation);
    let aim = Vec3::from(world[t.target].translation);
    let to = |p: Vec3| PLACE + nvf_play::space::to_bevy(p.to_array());
    if let Some(mut tr) = drawn.camera.and_then(|c| transforms.get_mut(c).ok()) {
        *tr = Transform::from_translation(to(eye)).looking_at(to(aim), Vec3::Y);
    }
    drawn.eye = (eye, aim);
    // The chips.
    let key: String = s.table.bets.iter().map(|(b, a)| format!("{}={a};", b.point)).collect();
    if key != drawn.chips_key {
        for e in drawn.chips.drain(..) {
            commands.entity(e).despawn();
        }
        if let (Some(space), Some(m)) = (drawn.space, drawn.chip_model.clone()) {
            for (b, amount) in &s.table.bets {
                let Some(&p) = drawn.points.get(&b.point) else { continue };
                let height = (stack_chips(*amount) as f32 / CHIPS_PER_MODEL).clamp(1.0 / CHIPS_PER_MODEL, 1.0);
                // The model hangs under its origin [observed: its bounds, z −2.04 to 0]: the
                // stack is lifted so its foot stands on the felt.
                let lift = chip_depth(&m) * height;
                let stack = commands.spawn((Transform::from_translation(p + Vec3::Z * lift).with_scale(Vec3::new(1.0, 1.0, height)), Visibility::Inherited, ChildOf(space))).id();
                spawn_shapes(&mut commands, &m, stack, &layer);
                drawn.chips.push(stack);
            }
        }
        drawn.chips_key = key;
    }
    // The marker over the bet point under it, above any chips there.
    let betting = s.phase == Phase::Betting;
    if let Some(marker) = drawn.marker {
        let at = drawn.points.get(&s.cursor).copied();
        if let (Some(p), Ok(mut tr)) = (at, transforms.get_mut(marker)) {
            let on = s.table.on(&s.cursor);
            let depth = drawn.chip_model.as_deref().map_or(2.0, chip_depth);
            let lift = if on > 0 { depth * (stack_chips(on) as f32 / CHIPS_PER_MODEL).clamp(1.0 / CHIPS_PER_MODEL, 1.0) } else { 0.0 };
            tr.translation = p + Vec3::Z * lift;
        }
        if let Ok(mut v) = visibility.get_mut(marker) {
            v.set_if_neq(if betting && at.is_some() && s.help.is_none() { Visibility::Inherited } else { Visibility::Hidden });
        }
        let valid = s.can_place();
        for (list, show) in [(&drawn.selector, valid), (&drawn.invalid, !valid)] {
            for e in list {
                if let Ok(mut v) = visibility.get_mut(*e) {
                    v.set_if_neq(if show { Visibility::Inherited } else { Visibility::Hidden });
                }
            }
        }
    }
    // The pointer over the felt.
    let Ok(w) = window.single() else { return };
    let cursor = w.cursor_position();
    if cursor == drawn.cursor || !betting || s.help.is_some() {
        return;
    }
    drawn.cursor = cursor;
    let Some(c) = cursor else { return };
    let size = Vec2::new(w.width(), w.height());
    let felt_z = drawn.points.values().next().map_or(0.0, |p| p.z);
    let found = pick(drawn.eye, c, size, felt_z, &drawn.points);
    if found != pointer.point {
        pointer.point = found;
        pointer.moves += 1;
    }
}

/// The bet point under the pointer at `c` (logical pixels in a window of `size`), the
/// camera at `eye` looking at `aim` (game space).
fn pick((eye, aim): (Vec3, Vec3), c: Vec2, size: Vec2, felt_z: f32, points: &HashMap<String, Vec3>) -> Option<String> {
    if size.x <= 0.0 || size.y <= 0.0 {
        return None;
    }
    let f = (aim - eye).normalize_or_zero();
    let r = f.cross(Vec3::Z).normalize_or_zero();
    let up = r.cross(f);
    let tan = (VFOV_DEG.to_radians() / 2.0).tan();
    let ndc = Vec2::new(c.x / size.x * 2.0 - 1.0, 1.0 - c.y / size.y * 2.0);
    let dir = f + r * ndc.x * tan * size.x / size.y + up * ndc.y * tan;
    if dir.z.abs() < 1e-6 {
        return None;
    }
    let k = (felt_z - eye.z) / dir.z;
    if k <= 0.0 {
        return None;
    }
    let hit = eye + dir * k;
    points
        .iter()
        .map(|(n, p)| (n, Vec2::new(p.x - hit.x, p.y - hit.y).length()))
        .filter(|(n, d)| *d <= if n.starts_with('S') { REACH_OUTSIDE } else { REACH_GRID })
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map(|(n, _)| n.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pockets_and_angles_agree() {
        for p in 0..rules::POCKETS as u8 {
            assert_eq!(pocket_at(pocket_angle(p)), p);
        }
        // The strip's first cell, 35, runs from −15.33° on.
        assert_eq!(pocket_at(-15.0), 35);
        assert_eq!(pocket_at(-16.0), 14);
    }

    #[test]
    fn stacks_count_chips_by_denomination() {
        assert_eq!(stack_chips(1), 1);
        assert_eq!(stack_chips(35), 2);
        assert_eq!(stack_chips(200), 2);
        assert_eq!(stack_chips(49), 7);
    }

    #[test]
    fn the_pointer_finds_the_point_it_looks_at() {
        let points: HashMap<String, Vec3> = [("D12".to_string(), Vec3::new(0.0, 5.33, 0.0)), ("S08".to_string(), Vec3::new(-4.0, -10.67, 0.0))].into_iter().collect();
        // Looking straight down at D12 from above, the pointer in the middle.
        let eye = (Vec3::new(0.0, 5.0, 60.0), Vec3::new(0.0, 5.33, 0.0));
        assert_eq!(pick(eye, Vec2::new(640.0, 360.0), Vec2::new(1280.0, 720.0), 0.0, &points).as_deref(), Some("D12"));
        assert_eq!(pick(eye, Vec2::new(0.0, 0.0), Vec2::new(1280.0, 720.0), 0.0, &points), None);
    }
}
