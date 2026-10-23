//! Room and portal culling in interiors (docs/spec-notes/level-design.md §3.1;
//! docs/design/frame-rate-audit.md §3): with the camera in a room, only the rooms seen
//! through its portals, and through theirs, are drawn.
//!
//! The rule [documented: level-design.md §3.1, the editor's Optimization and Occlusion
//! pages]: room markers are boxes; portals join two rooms; the space outside every room
//! (the unbounded space) acts as one more room; a portal in view makes the room behind it
//! visible, seen through a narrower frustum (the portal's projection), and the step repeats
//! from there. What is done here [engineering choices, each a simplification of that rule]:
//! - **Data**: room markers are references of base `0x1F` with a box `XPRM` (half
//!   extents); portals are references of base `0x20` with an `XPRM` one unit thick and
//!   `XPOD` naming the two rooms, a null entry the unbounded space [observed:
//!   level-design.md §3.1 "Data"].
//! - **Membership**: room membership is not stored per object (level-design.md §3.1), so
//!   a placed object's mesh belongs to every room whose box, grown by [`MARGIN`], its
//!   bounds touch, and to the unbounded space when it touches none. A mesh is drawn when
//!   one of its rooms is. Walls and floors on a room's edge belong to both sides. Only the
//!   meshes of placed objects and items are culled: actors, the player's body, dropped
//!   items, lights, particles and effects are always drawn (actors walk between rooms;
//!   the editor's pages say objects moved by Havok are drawn while they move).
//! - **The narrower frustum** is a screen rectangle: a portal's corners projected, their
//!   bounding rectangle cut by the rectangle the room was seen through. A portal with a
//!   corner behind the camera passes the whole rectangle on (the camera is in or near it).
//! - **No room**: with the camera in no room (outside the boxes, in a cell without rooms,
//!   or with no player camera), everything is drawn: never the void.
//! - Occlusion planes and multibounds (§3.2) are not used.
//!
//! A culled mesh is moved to render layer [`CULL_LAYER`], which no camera or light sees;
//! the meshes this module hides are marked [`RoomHidden`] and given back the default
//! layer when their room is in view again (placed objects' meshes have no layers of
//! their own). `--no-room-culling` turns it off; the test aid
//! `NVF_PLAY_TEST_ROOM_CULLING_CYCLE=<seconds>` turns it off and on again every that many
//! seconds of real time (`rooms: culling off|on (test aid)`), to compare frame times in
//! one run.
//!
//! Logged: `rooms: <cell>: <n> rooms, <p> portals` when an interior with rooms loads, and
//! `rooms: <cell>: camera in room <id>, <v> of <n> rooms in view, <h> meshes hidden`
//! when that changes (at most a line a second), and at the exit
//! `rooms summary: …` with the frames culled and the most meshes hidden.

use crate::draw_distance::DrawClass;
use crate::world::{bevy_to_game, Streamer, WorldData};
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use fnv_world::fields::{formid_of, global, num, winner, Fields};
use fnv_world::placed::placement;
use fnv_world::transform::ref_rotation;
use nvf_play::RefVisual;
use std::collections::{HashMap, HashSet, VecDeque};

/// The render layer culled meshes are moved to: seen by no camera or light.
pub const CULL_LAYER: usize = 29;
/// Room boxes are grown by this much for membership, game units [engineering choice: a
/// wall's thickness, so the walls and floors on a room's edge count on both sides].
pub const MARGIN: f32 = 32.0;
/// Base forms of room markers and portals [observed: level-design.md §3.1].
pub const ROOM_MARKER: u32 = 0x1F;
pub const PORTAL: u32 = 0x20;
/// Rooms followed at most per frame (a guard against a loop of portals).
const MAX_STEPS: usize = 4096;

/// A room box: centre, rotation (game space, `v' = M v`) and half extents.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RoomBox {
    pub id: u32,
    pub center: Vec3,
    pub rot: Mat3,
    pub half: Vec3,
}

impl RoomBox {
    /// Whether `p` (game space) is inside, the box grown by `grow`.
    pub fn contains(&self, p: Vec3, grow: f32) -> bool {
        let l = self.rot.transpose() * (p - self.center);
        l.x.abs() <= self.half.x + grow && l.y.abs() <= self.half.y + grow && l.z.abs() <= self.half.z + grow
    }

    /// Whether the box grown by `grow` touches the world box `min`–`max` (game space):
    /// the oriented box against the axis-aligned one, by their separating axes along
    /// the world's and the box's own axes [a conservative test: boxes it calls touching
    /// may not, never the other way].
    pub fn touches(&self, min: Vec3, max: Vec3, grow: f32) -> bool {
        let half = self.half + Vec3::splat(grow);
        let c2 = (min + max) / 2.0;
        let h2 = (max - min) / 2.0;
        let d = c2 - self.center;
        let axes = [self.rot.x_axis, self.rot.y_axis, self.rot.z_axis];
        // The world's axes.
        for i in 0..3 {
            let r1: f32 = (0..3).map(|k| half[k] * axes[k][i].abs()).sum();
            if d[i].abs() > r1 + h2[i] {
                return false;
            }
        }
        // The box's axes.
        for (k, a) in axes.iter().enumerate() {
            let r2 = h2.x * a.x.abs() + h2.y * a.y.abs() + h2.z * a.z.abs();
            if d.dot(*a).abs() > half[k] + r2 {
                return false;
            }
        }
        true
    }
}

/// A portal: its corners (game space) and the two rooms it joins (indices into the
/// rooms; `None` the unbounded space).
#[derive(Debug, Clone, PartialEq)]
pub struct Portal {
    pub id: u32,
    pub corners: Vec<Vec3>,
    pub rooms: [Option<usize>; 2],
}

/// A space a portal leads to: a room, or the unbounded space.
type Space = Option<usize>;

/// The rooms and portals of the interior drawn now.
#[derive(Resource, Default)]
pub struct Rooms {
    pub off: bool,
    /// `NVF_PLAY_TEST_ROOM_CULLING_CYCLE`: seconds on, then as many off.
    cycle: Option<f32>,
    cycled_off: bool,
    pub cell: Option<u32>,
    pub rooms: Vec<RoomBox>,
    pub portals: Vec<Portal>,
    /// The spaces in view this frame (`None` in it: the unbounded space); `None`: all.
    pub visible: Option<HashSet<Space>>,
    pub camera_room: Option<usize>,
    last_log: f32,
    logged: Option<(Option<usize>, usize, usize)>,
    pub frames_culled: u64,
    pub most_hidden: usize,
}

/// The spaces a mesh belongs to ([`MARGIN`]); an empty list is the unbounded space.
#[derive(Component, Debug, Clone, Default)]
pub struct InRooms(pub Vec<usize>);

/// A placed reference's mesh that is never culled (an actor's, a dropped item's).
#[derive(Component)]
pub struct Unroomed;

/// A mesh this module moved to [`CULL_LAYER`].
#[derive(Component)]
pub struct RoomHidden;

pub struct RoomsPlugin {
    pub off: bool,
}

impl Plugin for RoomsPlugin {
    fn build(&self, app: &mut App) {
        if self.off {
            info!("rooms: --no-room-culling: every room of an interior is drawn");
        }
        let cycle = std::env::var("NVF_PLAY_TEST_ROOM_CULLING_CYCLE").ok().and_then(|v| v.trim().parse::<f32>().ok()).filter(|s| *s > 0.0);
        if let Some(c) = cycle {
            info!("test aid NVF_PLAY_TEST_ROOM_CULLING_CYCLE: room culling off and on every {c} s");
        }
        app.insert_resource(Rooms { off: self.off, cycle, ..default() }).add_systems(Update, (load, assign, cull).chain().after(crate::draw_distance::apply_ranges)).add_systems(Last, report);
    }
}

/// A portal's corners from its centre, rotation and half extents: the box's corners on
/// its two widest axes, at the middle of the thin one.
pub fn portal_corners(center: Vec3, rot: Mat3, half: Vec3) -> Vec<Vec3> {
    let mut axes = [(half.x, rot.x_axis), (half.y, rot.y_axis), (half.z, rot.z_axis)];
    axes.sort_by(|a, b| b.0.total_cmp(&a.0));
    let (a, b) = (axes[0].1 * axes[0].0, axes[1].1 * axes[1].0);
    vec![center + a + b, center + a - b, center - a - b, center - a + b]
}

/// Read the rooms and portals of interior `cell` (module notes).
fn read(world: &WorldData, cell: u32) -> (Vec<RoomBox>, Vec<Portal>) {
    let Some(c) = world.index.cells.get(&cell) else { return (Vec::new(), Vec::new()) };
    let mut rooms = Vec::new();
    let mut raw_portals = Vec::new();
    for &r in &c.refs {
        let Some(p) = placement(world.lo, world.schema, r) else { continue };
        let base = p.base.map(|b| b & 0x00FF_FFFF);
        if base != Some(ROOM_MARKER) && base != Some(PORTAL) {
            continue;
        }
        let Some((pi, rec)) = winner(world.lo, r) else { continue };
        let Ok(f) = Fields::new(rec, pi, world.schema) else { continue };
        let Some(bounds) = f.get(b"XPRM").and_then(|v| v.get("bounds").cloned()) else { continue };
        let Some(half) = (|| Some(Vec3::new(num(&bounds, "x")? as f32, num(&bounds, "y")? as f32, num(&bounds, "z")? as f32).abs()))() else { continue };
        let center = Vec3::from_array(p.position);
        let rot = Mat3::from_cols_array(&ref_rotation(p.rotation)).transpose();
        if base == Some(ROOM_MARKER) {
            rooms.push(RoomBox { id: r, center, rot, half });
        } else {
            let pair: Vec<Option<u32>> = f
                .get(b"XPOD")
                .and_then(|v| v.as_array().cloned())
                .map(|a| a.iter().map(|x| formid_of(x).and_then(|l| global(world.lo, pi, l))).collect())
                .unwrap_or_default();
            raw_portals.push((r, portal_corners(center, rot, half), pair));
        }
    }
    let index: HashMap<u32, usize> = rooms.iter().enumerate().map(|(i, r)| (r.id, i)).collect();
    let portals = raw_portals
        .into_iter()
        .filter_map(|(id, corners, pair)| {
            if pair.len() != 2 {
                return None;
            }
            // A null entry is the unbounded space; a room not among the cell's markers
            // cannot be followed (the portal is dropped).
            let to = |x: &Option<u32>| -> Option<Space> {
                match x {
                    None => Some(None),
                    Some(r) => index.get(r).map(|i| Some(*i)),
                }
            };
            Some(Portal { id, corners, rooms: [to(&pair[0])?, to(&pair[1])?] })
        })
        .collect();
    (rooms, portals)
}

fn load(mut rooms: ResMut<Rooms>, streamer: Option<Res<Streamer>>, world: Option<Res<WorldData>>) {
    let interior = streamer.and_then(|s| s.interior);
    if rooms.cell == interior {
        return;
    }
    rooms.cell = interior;
    rooms.rooms.clear();
    rooms.portals.clear();
    rooms.visible = None;
    rooms.logged = None;
    let (Some(cell), Some(world)) = (interior, world) else { return };
    let (r, p) = read(&world, cell);
    if !r.is_empty() {
        info!("rooms: {cell:08X}: {} rooms, {} portals", r.len(), p.len());
    }
    rooms.rooms = r;
    rooms.portals = p;
}

/// Give new meshes of placed objects and items their rooms.
#[allow(clippy::type_complexity)]
fn assign(
    mut commands: Commands,
    rooms: Res<Rooms>,
    new: Query<(Entity, &DrawClass, &GlobalTransform, &Aabb), (With<RefVisual>, Without<InRooms>, Without<Unroomed>)>,
) {
    if rooms.rooms.is_empty() {
        return;
    }
    for (e, class, gt, aabb) in &new {
        if !matches!(class, DrawClass::Object | DrawClass::Item) {
            commands.entity(e).insert(Unroomed);
            continue;
        }
        let (min, max) = game_bounds(gt, aabb);
        let in_rooms: Vec<usize> = rooms.rooms.iter().enumerate().filter(|(_, r)| r.touches(min, max, MARGIN)).map(|(i, _)| i).collect();
        commands.entity(e).insert(InRooms(in_rooms));
    }
}

/// A mesh's bounds in game space (axis-aligned).
fn game_bounds(gt: &GlobalTransform, aabb: &Aabb) -> (Vec3, Vec3) {
    let c = Vec3::from(aabb.center);
    let h = Vec3::from(aabb.half_extents);
    let m = gt.affine();
    let mut min = Vec3::splat(f32::INFINITY);
    let mut max = Vec3::splat(f32::NEG_INFINITY);
    for i in 0..8 {
        let s = Vec3::new(if i & 1 == 0 { -1.0 } else { 1.0 }, if i & 2 == 0 { -1.0 } else { 1.0 }, if i & 4 == 0 { -1.0 } else { 1.0 });
        let p = bevy_to_game(m.transform_point3(c + h * s));
        min = min.min(p);
        max = max.max(p);
    }
    (min, max)
}

/// A screen rectangle in normalised device coordinates: (min x, min y, max x, max y).
type Rect2 = [f32; 4];
const FULL: Rect2 = [-1.0, -1.0, 1.0, 1.0];

fn cut(a: Rect2, b: Rect2) -> Option<Rect2> {
    let r = [a[0].max(b[0]), a[1].max(b[1]), a[2].min(b[2]), a[3].min(b[3])];
    (r[0] < r[2] && r[1] < r[3]).then_some(r)
}

fn covers(a: Rect2, b: Rect2) -> bool {
    a[0] <= b[0] && a[1] <= b[1] && a[2] >= b[2] && a[3] >= b[3]
}

fn union(a: Rect2, b: Rect2) -> Rect2 {
    [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]
}

/// The screen rectangle of a portal's corners (game space) through `clip_from_game`; the
/// whole screen when a corner is behind the camera; `None` when all are.
fn portal_rect(corners: &[Vec3], clip_from_game: &Mat4) -> Option<Rect2> {
    let mut r = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    let mut behind = 0;
    for c in corners {
        let p = *clip_from_game * c.extend(1.0);
        if p.w <= 1e-3 {
            behind += 1;
            continue;
        }
        let (x, y) = (p.x / p.w, p.y / p.w);
        r = [r[0].min(x), r[1].min(y), r[2].max(x), r[3].max(y)];
    }
    match behind {
        0 => Some(r),
        n if n == corners.len() => None,
        _ => Some(FULL),
    }
}

/// The spaces seen from `start` through the portals (module notes).
pub fn visible_spaces(portals: &[Portal], start: Space, clip_from_game: &Mat4) -> HashSet<Space> {
    let mut seen: HashMap<Space, Rect2> = HashMap::from([(start, FULL)]);
    let mut queue: VecDeque<(Space, Rect2)> = VecDeque::from([(start, FULL)]);
    let mut steps = 0;
    while let Some((space, rect)) = queue.pop_front() {
        steps += 1;
        if steps > MAX_STEPS {
            break;
        }
        for p in portals {
            let other = if p.rooms[0] == space {
                p.rooms[1]
            } else if p.rooms[1] == space {
                p.rooms[0]
            } else {
                continue;
            };
            let Some(pr) = portal_rect(&p.corners, clip_from_game) else { continue };
            let Some(through) = cut(pr, rect) else { continue };
            match seen.get_mut(&other) {
                Some(had) if covers(*had, through) => {}
                Some(had) => {
                    *had = union(*had, through);
                    queue.push_back((other, through));
                }
                None => {
                    seen.insert(other, through);
                    queue.push_back((other, through));
                }
            }
        }
    }
    seen.into_keys().collect()
}

#[allow(clippy::type_complexity)]
fn cull(
    mut commands: Commands,
    time: Res<Time<Real>>,
    mut rooms: ResMut<Rooms>,
    camera: Query<(&Camera, &GlobalTransform), With<nvf_play::PlayerCamera>>,
    meshes: Query<(Entity, &InRooms, Has<RoomHidden>, Option<&RenderLayers>)>,
) {
    let rooms = &mut *rooms;
    if rooms.rooms.is_empty() {
        return;
    }
    let now = time.elapsed_secs();
    if let Some(c) = rooms.cycle {
        let off = (now / c) as u64 % 2 == 1;
        if off != rooms.cycled_off {
            rooms.cycled_off = off;
            info!("rooms: culling {} (test aid)", if off { "off" } else { "on" });
        }
    }
    let cam = camera.iter().find(|(c, _)| c.is_active);
    let eye = cam.map(|(_, gt)| bevy_to_game(gt.translation()));
    let start = eye.and_then(|p| rooms.rooms.iter().position(|r| r.contains(p, 0.0)));
    let visible = match (rooms.off || rooms.cycled_off, cam, start) {
        (false, Some((c, gt)), Some(start)) => {
            // Game space → Bevy world space is the world root's rotation (world.rs
            // `game_to_bevy`; an interior's cell entity sits at the origin).
            let bevy_from_game = crate::world::game_to_bevy().to_matrix();
            let clip_from_game = c.clip_from_view() * gt.to_matrix().inverse() * bevy_from_game;
            Some(visible_spaces(&rooms.portals, Some(start), &clip_from_game))
        }
        _ => None,
    };
    let mut hidden = 0usize;
    for (e, in_rooms, was_hidden, layers) in &meshes {
        let show = visible.as_ref().is_none_or(|v| if in_rooms.0.is_empty() { v.contains(&None) } else { in_rooms.0.iter().any(|i| v.contains(&Some(*i))) });
        if !show {
            hidden += 1;
        }
        match (show, was_hidden) {
            (false, false) if layers.is_none() => {
                commands.entity(e).insert((RenderLayers::layer(CULL_LAYER), RoomHidden));
            }
            (true, true) => {
                commands.entity(e).remove::<(RenderLayers, RoomHidden)>();
            }
            _ => {}
        }
    }
    rooms.camera_room = start;
    if visible.is_some() {
        rooms.frames_culled += 1;
        rooms.most_hidden = rooms.most_hidden.max(hidden);
    }
    let state = (start, visible.as_ref().map_or(rooms.rooms.len() + 1, |v| v.len()), hidden);
    if rooms.logged != Some(state) && now - rooms.last_log >= 1.0 {
        rooms.logged = Some(state);
        rooms.last_log = now;
        let cell = rooms.cell.unwrap_or(0);
        match (visible.as_ref(), start) {
            (Some(v), Some(s)) => info!(
                "rooms: {cell:08X}: camera in room {:08X}, {} of {} rooms in view{}, {hidden} meshes hidden",
                rooms.rooms[s].id,
                v.iter().filter(|x| x.is_some()).count(),
                rooms.rooms.len(),
                if v.contains(&None) { " and the unbounded space" } else { "" }
            ),
            _ => info!("rooms: {cell:08X}: camera in no room, every room drawn"),
        }
    }
    rooms.visible = visible;
}

fn report(mut exit: MessageReader<AppExit>, rooms: Res<Rooms>) {
    if exit.read().last().is_some() && rooms.frames_culled > 0 {
        info!("rooms summary: {} frames culled by rooms and portals, at most {} meshes hidden", rooms.frames_culled, rooms.most_hidden);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn room(id: u32, center: [f32; 3], half: [f32; 3]) -> RoomBox {
        RoomBox { id, center: Vec3::from_array(center), rot: Mat3::IDENTITY, half: Vec3::from_array(half) }
    }

    #[test]
    fn rooms_contain_and_touch() {
        let r = room(1, [0.0, 0.0, 0.0], [100.0, 50.0, 20.0]);
        assert!(r.contains(Vec3::new(99.0, -49.0, 0.0), 0.0));
        assert!(!r.contains(Vec3::new(101.0, 0.0, 0.0), 0.0));
        assert!(r.contains(Vec3::new(101.0, 0.0, 0.0), 2.0));
        assert!(r.touches(Vec3::new(90.0, 0.0, 0.0), Vec3::new(130.0, 10.0, 10.0), 0.0));
        assert!(!r.touches(Vec3::new(140.0, 0.0, 0.0), Vec3::new(150.0, 10.0, 10.0), 32.0));
        assert!(r.touches(Vec3::new(120.0, 0.0, 0.0), Vec3::new(150.0, 10.0, 10.0), 32.0), "within the margin");
        // A box turned a quarter: its long side along y.
        let t = RoomBox { rot: Mat3::from_rotation_z(std::f32::consts::FRAC_PI_2), ..r };
        assert!(t.contains(Vec3::new(0.0, 99.0, 0.0), 0.0) && !t.contains(Vec3::new(99.0, 0.0, 0.0), 0.0));
        assert!(!t.touches(Vec3::new(60.0, 0.0, 0.0), Vec3::new(70.0, 1.0, 1.0), 0.0));
    }

    #[test]
    fn portals_span_their_two_widest_axes() {
        let c = portal_corners(Vec3::ZERO, Mat3::IDENTITY, Vec3::new(64.0, 0.5, 96.0));
        assert_eq!(c.len(), 4);
        assert!(c.iter().all(|p| p.y == 0.0 && p.x.abs() == 64.0 && p.z.abs() == 96.0));
    }

    /// A camera at the origin looking along +x (game space), 90° wide.
    fn camera() -> Mat4 {
        let proj = Mat4::perspective_rh(std::f32::consts::FRAC_PI_2, 1.0, 1.0, 10_000.0);
        // View: game +x forward, +z up.
        let view = Mat4::look_at_rh(Vec3::ZERO, Vec3::X, Vec3::Z);
        proj * view
    }

    #[test]
    fn rooms_are_seen_through_portals_in_view() {
        // Room 0 around the camera; room 1 beyond a portal ahead; room 2 beyond a portal
        // behind the camera's back, far off; room 3 behind room 1's far portal.
        let p = |id, x: f32, a, b| Portal { id, corners: portal_corners(Vec3::new(x, 0.0, 0.0), Mat3::IDENTITY, Vec3::new(0.5, 50.0, 50.0)), rooms: [a, b] };
        let portals = vec![p(10, 200.0, Some(0), Some(1)), p(11, -300.0, Some(0), Some(2)), p(12, 600.0, Some(1), Some(3)), p(13, 900.0, Some(3), None)];
        let v = visible_spaces(&portals, Some(0), &camera());
        assert!(v.contains(&Some(0)) && v.contains(&Some(1)) && v.contains(&Some(3)) && v.contains(&None));
        assert!(!v.contains(&Some(2)), "the room behind the camera is not seen");
        // A portal off to the side, outside the first portal's rectangle, is not followed.
        let side = vec![p(10, 200.0, Some(0), Some(1)), Portal { id: 14, corners: portal_corners(Vec3::new(600.0, 2000.0, 0.0), Mat3::IDENTITY, Vec3::new(0.5, 50.0, 50.0)), rooms: [Some(1), Some(4)] }];
        let v = visible_spaces(&side, Some(0), &camera());
        assert!(v.contains(&Some(1)) && !v.contains(&Some(4)));
    }

    #[test]
    fn a_portal_the_camera_stands_in_passes_the_whole_view() {
        let corners = vec![Vec3::new(-10.0, -50.0, -50.0), Vec3::new(10.0, 50.0, -50.0), Vec3::new(10.0, 50.0, 50.0), Vec3::new(-10.0, -50.0, 50.0)];
        assert_eq!(portal_rect(&corners, &camera()), Some(FULL));
        let behind: Vec<Vec3> = corners.iter().map(|c| Vec3::new(-100.0 - c.x.abs(), c.y, c.z)).collect();
        assert_eq!(portal_rect(&behind, &camera()), None);
    }
}
