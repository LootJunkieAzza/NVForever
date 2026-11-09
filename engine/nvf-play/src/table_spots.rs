//! Where the players stand after a match at a darts board, pool table or fillet table
//! (owner's item 127): a row of up to four spots beside the table, everyone facing away
//! from it toward open floor, so that the camera standing in front of each player
//! (`nvforever::match_end`) looks back at them with the table behind and no one else in
//! between.
//!
//! - **The row** [design]: four spots [`SPACING`] apart along a line, centred on a point
//!   at an angle `a` and a distance `d` from the table's reference, in the table's own
//!   frame (local +y is the reference's facing: into a darts board, across a pool table);
//!   each faces heading `a` (away from the table). With fewer players the middle spots are
//!   used ([`offsets`]). The seat order is the match's (the owner first).
//! - **Picked by hand** ([`PICKED`]): every board and table the three minigame plugins
//!   place has its row picked: the finder below run in each cell (`NVF_PLAY_TEST_SPOTS`),
//!   its pick looked at from the end camera with four bodies on the spots, and changed
//!   where the picture called for it (results: the night report of 2026-11-09). A key is
//!   the plugin whose form id it is (the load order's file) and the id's low 24 bits.
//! - **The finder** ([`find`]), for a table that is not on the list (another plugin's): it
//!   tries rows around the table every 15 degrees at growing distances and keeps the first
//!   whose spots all stand on the floor, clear of walls and furniture by a body's width,
//!   with nothing overhead, and whose end camera has a clear line to each spot's chest.
//!   Darts boards are on a wall: only rows in front of the board are tried.
//!
//! Log lines start `table spots:`.

use crate::phys::PhysState;
use bevy::prelude::*;
use fnv_phys::Filter;

/// Between two players in the row, game units [design: shoulders about 40 apart].
pub const SPACING: f32 = 50.0;
/// A spot is clear when nothing is within this of its centre at the knees, the waist and
/// the shoulders [design: a body's half width and a little].
const CLEAR: f32 = 24.0;
/// Heights the clearance is looked at, above the feet.
const HEIGHTS: [f32; 3] = [30.0, 70.0, 110.0];
/// The floor under a spot may be this far from the table's floor [design: a step].
const STEP: f32 = 18.0;
/// The end camera's place relative to a player's eye, as `match_end::END_VIEW` puts it:
/// out along the heading turned by `yaw`, at `distance`, `drop` below the eye at the
/// chest, `pitch` up. Copied here so the finder needs no renderer type (kept equal by a
/// test in match_end).
pub const CAM_YAW: f32 = 0.35;
pub const CAM_DISTANCE: f32 = 150.0;
pub const CAM_DROP: f32 = 30.0;
pub const CAM_PITCH: f32 = 0.12;
/// Eye height above the feet the camera check assumes [observed: the player's
/// `eye_height` is about 120 in game units; design: a little lower for shorter bodies].
const EYE: f32 = 115.0;

/// What a table is for: which rows the finder tries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// On a wall: in front of it only (local -y).
    Darts,
    Pool,
    Fillet,
}

/// A table's row: angle (radians, local, 0 along local +y, clockwise as headings turn) and
/// distance of its centre from the reference, game units, and how many players it holds
/// (4, fewer where the floor in front is too narrow: the players past that keep their
/// place).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pick {
    pub a: f32,
    pub d: f32,
    pub max: usize,
}

/// A picked row: (plugin file, form id's low 24 bits, kind, pick).
pub struct Picked {
    pub plugin: &'static str,
    pub id: u32,
    pub kind: Kind,
    pub pick: Pick,
}

const DEG: f32 = std::f32::consts::PI / 180.0;

/// The rows picked for every board and table the minigame plugins place (module notes):
/// Darts Minigame.esp's boards, the tables Pool Table Minigame.esp makes playable (the base
/// game's references, by their FalloutNV.esm ids) and Knife Minigame.esp's knives. Each was
/// looked at from the end camera with four bodies on it (night report 2026-11-09); a
/// reference not here uses the finder.
pub const PICKED: &[Picked] = &[
    Picked { plugin: "Darts Minigame.esp", id: 0x000DBB, kind: Kind::Darts, pick: Pick { a: 165.0 * DEG, d: 130.0, max: 4 } },
    // Between diner booths and a table: room for three in front of it.
    Picked { plugin: "Darts Minigame.esp", id: 0x0010B1, kind: Kind::Darts, pick: Pick { a: 180.0 * DEG, d: 130.0, max: 3 } },
    Picked { plugin: "Darts Minigame.esp", id: 0x0010B2, kind: Kind::Darts, pick: Pick { a: 180.0 * DEG, d: 130.0, max: 4 } },
    Picked { plugin: "Darts Minigame.esp", id: 0x0010B5, kind: Kind::Darts, pick: Pick { a: 180.0 * DEG, d: 130.0, max: 4 } },
    Picked { plugin: "Darts Minigame.esp", id: 0x0010BA, kind: Kind::Darts, pick: Pick { a: 210.0 * DEG, d: 130.0, max: 4 } },
    Picked { plugin: "Knife Minigame.esp", id: 0x000ADE, kind: Kind::Fillet, pick: Pick { a: 105.0 * DEG, d: 70.0, max: 4 } },
    Picked { plugin: "Knife Minigame.esp", id: 0x000DC3, kind: Kind::Fillet, pick: Pick { a: 255.0 * DEG, d: 70.0, max: 4 } },
    Picked { plugin: "Knife Minigame.esp", id: 0x000DC5, kind: Kind::Fillet, pick: Pick { a: 255.0 * DEG, d: 70.0, max: 4 } },
    Picked { plugin: "Knife Minigame.esp", id: 0x000DC7, kind: Kind::Fillet, pick: Pick { a: 195.0 * DEG, d: 85.0, max: 4 } },
    Picked { plugin: "Knife Minigame.esp", id: 0x000DC9, kind: Kind::Fillet, pick: Pick { a: 165.0 * DEG, d: 70.0, max: 4 } },
    // A small room: the end camera's line is clear for 95 of its 150 units.
    Picked { plugin: "FalloutNV.esm", id: 0x0E741A, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x0FBC35, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x10576B, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x10576C, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x107D7B, kind: Kind::Pool, pick: Pick { a: 180.0 * DEG, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x1110C7, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x11249D, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x115C43, kind: Kind::Pool, pick: Pick { a: 180.0 * DEG, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x115D5C, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117942, kind: Kind::Pool, pick: Pick { a: 180.0 * DEG, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117945, kind: Kind::Pool, pick: Pick { a: 180.0 * DEG, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117D21, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117D22, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117D23, kind: Kind::Pool, pick: Pick { a: 180.0 * DEG, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117D24, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x117D25, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x11C663, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x13D856, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x13D858, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x13D859, kind: Kind::Pool, pick: Pick { a: 180.0 * DEG, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x13D85A, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
    Picked { plugin: "FalloutNV.esm", id: 0x1760F3, kind: Kind::Pool, pick: Pick { a: 0.0, d: 110.0, max: 4 } },
];

/// The offsets along the row of `n` players' spots (seat order), centred.
pub fn offsets(n: usize) -> Vec<f32> {
    let n = n.clamp(1, 4);
    (0..n).map(|i| (i as f32 - (n as f32 - 1.0) / 2.0) * SPACING).collect()
}

/// Every offset up to `max` players use: a row is good when all of them are.
fn all_offsets(max: usize) -> Vec<f32> {
    let mut v: Vec<f32> = (1..=max.clamp(1, 4)).flat_map(offsets).collect();
    v.sort_by(f32::total_cmp);
    v.dedup_by(|a, b| (*a - *b).abs() < 0.5);
    v
}

/// The table's frame: its reference's place and Z angle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Frame {
    pub pos: [f32; 3],
    pub theta: f32,
}

impl Frame {
    /// Local (game units) to game space, as the boards' and tables' frames turn.
    pub fn to_world(&self, l: [f32; 2]) -> [f32; 2] {
        let (s, c) = self.theta.sin_cos();
        [self.pos[0] + l[0] * c + l[1] * s, self.pos[1] - l[0] * s + l[1] * c]
    }
}

/// The spots (game space x, y) and the heading all of them face, for a row `pick` and `n`
/// players, in seat order; a row that holds fewer has spots for the first seats only.
pub fn spots(f: Frame, pick: Pick, n: usize) -> (Vec<[f32; 2]>, f32) {
    let n = n.min(pick.max);
    let (s, c) = pick.a.sin_cos();
    let centre = [s * pick.d, c * pick.d];
    // Along the row: the local direction a quarter turn clockwise of the facing.
    let along = [c, -s];
    let v = offsets(n).into_iter().map(|o| f.to_world([centre[0] + along[0] * o, centre[1] + along[1] * o])).collect();
    (v, f.theta + pick.a)
}

/// The end camera for an eye facing `heading` (as `Portrait::spot`): where it stands and
/// the chest point it looks at.
pub fn camera_at(eye: [f32; 3], heading: f32) -> ([f32; 3], [f32; 3]) {
    let target = [eye[0], eye[1], eye[2] - CAM_DROP];
    let out = crate::space::look_dir(heading + CAM_YAW, CAM_PITCH);
    ([target[0] + out[0] * CAM_DISTANCE, target[1] + out[1] * CAM_DISTANCE, target[2] + out[2] * CAM_DISTANCE], target)
}

fn filter() -> Filter {
    Filter { no_actors: true, exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() }
}

/// Why a spot is not good, for the log.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bad {
    NoFloor,
    Step(f32),
    Blocked(f32),
    Overhead,
    Camera(f32),
}

/// Whether a player can stand at `xy` (on a floor near `floor`) facing `heading` with the
/// end camera's line clear: the feet's height when so.
pub fn check(phys: &mut PhysState, xy: [f32; 2], floor: f32, heading: f32) -> Result<f32, Bad> {
    check_with(phys, xy, floor, heading, None)
}

/// [`check`], with the camera's line asked clear only for `cam_min` (a small room: the end
/// camera pulls in from a wall itself) when given.
fn check_with(phys: &mut PhysState, xy: [f32; 2], floor: f32, heading: f32, cam_min: Option<f32>) -> Result<f32, Bad> {
    let hit = phys.ground_below([xy[0], xy[1], floor], 60.0, 80.0).ok_or(Bad::NoFloor)?;
    let z = hit.point[2];
    if (z - floor).abs() > STEP {
        return Err(Bad::Step(z - floor));
    }
    // Nothing over the head (a table top, a shelf) from the feet up.
    if phys.ray([xy[0], xy[1], z + 10.0], [0.0, 0.0, 1.0], 120.0, filter()).is_some() {
        return Err(Bad::Overhead);
    }
    for h in HEIGHTS {
        for i in 0..8 {
            let t = i as f32 * std::f32::consts::FRAC_PI_4;
            if let Some(r) = phys.ray([xy[0], xy[1], z + h], [t.sin(), t.cos(), 0.0], CLEAR, filter()) {
                return Err(Bad::Blocked(r.distance));
            }
        }
    }
    let (cam, chest) = camera_at([xy[0], xy[1], z + EYE], heading);
    let d = [cam[0] - chest[0], cam[1] - chest[1], cam[2] - chest[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    let dir = [d[0] / len, d[1] / len, d[2] / len];
    // The camera's line, and a little room around the camera itself.
    let need = cam_min.map_or(len + 15.0, |m| m.min(len + 15.0));
    if let Some(r) = phys.ray(chest, dir, need, filter()) {
        return Err(Bad::Camera(r.distance));
    }
    Ok(z)
}

/// How good a row is: every spot any number of players uses is good (the spots' feet), or
/// the first that is not.
pub fn check_row(phys: &mut PhysState, f: Frame, pick: Pick, floor: f32) -> Result<Vec<[f32; 3]>, (f32, Bad)> {
    check_row_with(phys, f, pick, floor, None)
}

fn check_row_with(phys: &mut PhysState, f: Frame, pick: Pick, floor: f32, cam_min: Option<f32>) -> Result<Vec<[f32; 3]>, (f32, Bad)> {
    let (s, c) = pick.a.sin_cos();
    let centre = [s * pick.d, c * pick.d];
    let along = [c, -s];
    let heading = f.theta + pick.a;
    let mut out = Vec::new();
    for o in all_offsets(pick.max) {
        let xy = f.to_world([centre[0] + along[0] * o, centre[1] + along[1] * o]);
        let z = check_with(phys, xy, floor, heading, cam_min).map_err(|b| (o, b))?;
        out.push([xy[0], xy[1], z]);
    }
    Ok(out)
}

/// The angles (local) the finder tries for a kind, best first, and the distances.
pub fn candidates(kind: Kind) -> (Vec<f32>, Vec<f32>) {
    use std::f32::consts::PI;
    let step = 15f32.to_radians();
    // (preferred angle, how far either way, the distances)
    let (prefer, span, ds): (f32, f32, Vec<f32>) = match kind {
        // In front of the board, behind the throw line (about 72 out) [design].
        Kind::Darts => (PI, 75f32.to_radians(), (0..8).map(|i| 130.0 + 15.0 * i as f32).collect()),
        // Off a long side of the cloth (local ±y), well clear of the cues [design].
        Kind::Pool => (0.0, PI, (0..9).map(|i| 110.0 + 15.0 * i as f32).collect()),
        Kind::Fillet => (PI, PI, (0..8).map(|i| 70.0 + 15.0 * i as f32).collect()),
    };
    let mut a = vec![prefer];
    let mut k = 1;
    while k as f32 * step <= span + 1e-3 {
        a.push(prefer + k as f32 * step);
        a.push(prefer - k as f32 * step);
        k += 1;
    }
    // A pool table is the same from either long side: try the far one early too.
    if kind == Kind::Pool {
        a.sort_by(|x, y| ang_off(*x, &[0.0, PI]).total_cmp(&ang_off(*y, &[0.0, PI])));
        a.dedup_by(|x, y| ang_off(*x, &[*y]) < 1e-3);
    }
    (a, ds)
}

/// The angle's distance to the nearest of `to`.
fn ang_off(a: f32, to: &[f32]) -> f32 {
    use std::f32::consts::{PI, TAU};
    to.iter().map(|t| ((a - t + PI).rem_euclid(TAU) - PI).abs()).fold(f32::MAX, f32::min)
}

/// The nearest good row: distances first, then angles from the preferred one; in a room
/// too small for the end camera's whole line, the nearest whose line is clear for
/// [`SMALL_ROOM`].
pub fn find(phys: &mut PhysState, f: Frame, kind: Kind, floor: f32) -> Option<Pick> {
    let (angles, ds) = candidates(kind);
    // Four players, then three, then two (a narrow spot in front of a board).
    for max in [4, 3, 2] {
        for cam_min in [None, Some(SMALL_ROOM)] {
            for &d in &ds {
                for &a in &angles {
                    let pick = Pick { a, d, max };
                    if check_row_with(phys, f, pick, floor, cam_min).is_ok() {
                        return Some(pick);
                    }
                }
            }
        }
    }
    None
}

/// The camera's line a small room must leave clear, game units [design: about two thirds
/// of the end view's distance].
const SMALL_ROOM: f32 = 95.0;

/// The picked row of a reference, by its plugin (`load_order`) and id.
pub fn picked(load_order: &[String], r: u32) -> Option<&'static Picked> {
    let file = load_order.get((r >> 24) as usize)?;
    PICKED.iter().find(|p| p.id == r & 0x00FF_FFFF && p.plugin.eq_ignore_ascii_case(file))
}

/// The row for a table: its picked one, else the finder's; `None` when no row fits.
pub fn row(phys: &mut PhysState, load_order: &[String], r: u32, f: Frame, kind: Kind, floor: f32) -> Option<Pick> {
    if let Some(p) = picked(load_order, r) {
        return Some(p.pick);
    }
    let found = find(phys, f, kind, floor);
    match found {
        Some(p) => info!("table spots: {r:08X} not picked; the finder's row a {:.0} deg d {:.0} for {}", p.a.to_degrees(), p.d, p.max),
        None => info!("table spots: {r:08X} not picked and no row fits"),
    }
    found
}

/// This player steps onto a spot (game space x, y on the floor near `floor`), facing
/// `heading`.
pub fn stand_player(phys: &mut PhysState, player: &mut crate::player::Player, xy: [f32; 2], floor: f32, heading: f32) {
    phys.place_player([xy[0], xy[1], floor], 60.0, 200.0);
    player.feet = phys.prev_feet;
    player.heading = heading;
    player.pitch = 0.0;
}

/// The kind of a reference by its base's Editor ID, for the boards and tables the minigame
/// plugins use.
pub fn kind_of(sim: &crate::sim::SimState, r: u32) -> Option<Kind> {
    let base = sim.base_of(r)?;
    let is = |edid: &str| sim.world.forms.by_edid(edid) == Some(base);
    if is(crate::minigames::DARTS_BOARD) {
        Some(Kind::Darts)
    } else if is(crate::pool::TABLE_NEW) || is(crate::pool::TABLE_RUINED) {
        Some(Kind::Pool)
    } else if is(crate::minigames::FILLET_KNIFE) {
        Some(Kind::Fillet)
    } else {
        None
    }
}

/// A reference's frame (its place now, else as placed).
pub fn frame_of(sim: &crate::sim::SimState, r: u32) -> Option<Frame> {
    let id = fnv_sim::FormId(r);
    let (pos, rot) = sim.world.peek(id).map(|x| (x.pos, x.rot)).or_else(|| sim.refs.get(id).map(|x| (x.pos, x.rot)))?;
    Some(Frame { pos, theta: rot[2] })
}

/// The floor around a table: the lowest ground found a little way out from it on every
/// side (a darts board hangs at head height, a knife lies on a table top).
pub fn floor_near(phys: &mut PhysState, f: Frame, kind: Kind) -> Option<f32> {
    // A board: the floor the thrower stands on, under the line.
    if kind == Kind::Darts {
        let xy = f.to_world([0.0, -90.0]);
        if let Some(h) = phys.ground_below([xy[0], xy[1], f.pos[2]], 0.0, 260.0) {
            return Some(h.point[2]);
        }
    }
    let (out, down) = match kind {
        Kind::Darts => (110.0, 260.0),
        Kind::Pool => (130.0, 120.0),
        Kind::Fillet => (90.0, 150.0),
    };
    (0..8)
        .filter_map(|i| {
            let t = i as f32 * std::f32::consts::FRAC_PI_4;
            let xy = f.to_world([t.sin() * out, t.cos() * out]);
            phys.ground_below([xy[0], xy[1], f.pos[2] + 30.0], 0.0, down).map(|h| h.point[2])
        })
        .min_by(f32::total_cmp)
}

/// One board or table the aid looks at: (reference, kind, frame, floor, row).
type AidTable = (u32, Kind, Frame, f32, Option<Pick>);

/// The test aid `NVF_PLAY_TEST_SPOTS=<start>[:<step>]`: from `start` (seconds), every
/// board and table within reach is looked at in turn: its picked row and the finder's are
/// logged, then this player stands on the row's first spot and three stand-ins on the
/// others with the end camera on, `step` seconds (default 4), then on the last spot.
#[derive(Resource, Default)]
struct SpotsAid {
    start: Option<f32>,
    step: f32,
    tables: Option<Vec<AidTable>>,
    stand_ins: Vec<fnv_sim::FormId>,
    shown: Option<usize>,
}

pub struct TableSpotsPlugin;

impl Plugin for TableSpotsPlugin {
    fn build(&self, app: &mut App) {
        let Ok(v) = std::env::var("NVF_PLAY_TEST_SPOTS") else { return };
        let mut it = v.split(':').map(|x| x.trim().parse::<f32>());
        let start = it.next().and_then(Result::ok);
        let step = it.next().and_then(Result::ok).unwrap_or(4.0);
        app.insert_resource(SpotsAid { start, step, ..default() }).add_systems(Update, spots_aid.in_set(crate::PlaySet::Ui));
    }
}

const STAND_INS: &[&str] = &["VStreetSS6Gambler01", "VStreetSS6Traveler01", "VStreetSS6Gambler02"];
/// What each stand-in wears, so the views show the row in all kinds of gear [owner
/// 2026-11-09: test bodies in NCR, Legion and power armor]: armor and helmet Editor IDs.
const STAND_IN_GEAR: [[&str; 2]; 3] = [["ArmorNVCLCenturion", "HelmetNVCLCenturion"], ["ArmorPowerT51b", "ArmorPowerT51bHelmet"], ["ArmorNVNCRTrooper", "HelmetNVNCRTrooper"]];

/// The aid's tables: every board and table within reach of `feet`, with the rows logged.
fn list_tables(sim: &crate::sim::SimState, phys: &mut PhysState, lo: &[String], feet: [f32; 3]) -> Vec<AidTable> {
    let mut list = Vec::new();
    // Interiors share coordinates: only this player's cell's.
    let here = sim.world.peek(fnv_sim::FormId::PLAYER_REF).and_then(|p| p.cell);
    for (id, r) in &sim.refs.refs {
        let d = ((r.pos[0] - feet[0]).powi(2) + (r.pos[1] - feet[1]).powi(2)).sqrt();
        if d > 8000.0 || (r.pos[2] - feet[2]).abs() > 1500.0 || (here.is_some() && r.cell.is_some() && r.cell != here) {
            continue;
        }
        let Some(kind) = kind_of(sim, id.0) else { continue };
        let Some(f) = frame_of(sim, id.0) else { continue };
        let Some(floor) = floor_near(phys, f, kind) else {
            info!("table spots: {:08X} {kind:?}: no floor found near it", id.0);
            continue;
        };
        let file = lo.get((id.0 >> 24) as usize).cloned().unwrap_or_default();
        let found = find(phys, f, kind, floor);
        let pick = picked(lo, id.0).map(|p| p.pick);
        // Why the preferred rows fail, nearest first, for picking by hand.
        let (angles, ds) = candidates(kind);
        for &dd in ds.iter().take(3) {
            let why: Vec<String> = angles
                .iter()
                .take(5)
                .map(|&a| match check_row(phys, f, Pick { a, d: dd, max: 4 }, floor) {
                    Ok(_) => format!("{:.0}:ok", a.to_degrees()),
                    Err((o, b)) => format!("{:.0}:{o:.0}:{b:?}", a.to_degrees()),
                })
                .collect();
            info!("table spots: {:08X} d {dd:.0}: {}", id.0, why.join(" "));
        }
        let show = |p: Option<Pick>| p.map_or("none".into(), |p| format!("a {:.0} d {:.0} max {}", p.a.to_degrees(), p.d, p.max));
        info!(
            "table spots: {:08X} ({file} {:06X}) {kind:?} at ({:.0}, {:.0}, {:.0}) turned {:.0} deg, floor {floor:.0}: finder {} picked {}",
            id.0,
            id.0 & 0xFF_FFFF,
            f.pos[0],
            f.pos[1],
            f.pos[2],
            f.theta.to_degrees(),
            show(found),
            show(pick),
        );
        list.push((id.0, kind, f, floor, pick.or(found)));
    }
    list
}

#[allow(clippy::too_many_arguments)]
fn spots_aid(
    time: crate::pause::AidTime,
    mut aid: ResMut<SpotsAid>,
    mut sim: ResMut<crate::sim::SimState>,
    mut phys: Option<ResMut<PhysState>>,
    mut player: Option<ResMut<crate::player::Player>>,
    mut control: Option<ResMut<crate::control::PlayerControl>>,
    mut script: Option<ResMut<crate::script::ScriptState>>,
) {
    let Some(start) = aid.start else { return };
    let now = time.elapsed_secs();
    if now < start {
        return;
    }
    let (Some(phys), Some(player), Some(rt)) = (phys.as_deref_mut(), player.as_deref_mut(), script.as_mut().and_then(|s| s.rt.as_mut())) else { return };
    let aid = &mut *aid;
    if aid.tables.is_none() {
        let list = list_tables(&sim, phys, &rt.game.data.load_order, player.feet);
        info!("table spots: {} boards and tables near", list.len());
        aid.tables = Some(list);
    }
    let tables = aid.tables.as_ref().expect("listed");
    let i = ((now - start) / aid.step) as usize;
    if aid.shown == Some(i) {
        return;
    }
    aid.shown = Some(i);
    let (table, seat_last) = (i / 2, i % 2 == 1);
    let Some(&(r, kind, f, floor, Some(pick))) = tables.get(table) else {
        if table < tables.len() {
            info!("table spots: {:08X}: no row to show", tables[table].0);
        } else if table == tables.len() && !seat_last {
            info!("table spots: all shown");
        }
        return;
    };
    let (spots, heading) = spots(f, pick, 4);
    let me = if seat_last { spots.len() - 1 } else { 0 };
    let cell = sim.refs.get(fnv_sim::FormId(r)).and_then(|x| x.cell);
    while aid.stand_ins.len() < 3 {
        let base = STAND_INS[aid.stand_ins.len()];
        let Some(b) = sim.world.forms.by_edid(base) else { break };
        let at = [spots[1][0], spots[1][1], floor];
        match crate::script::with_world(rt, &mut sim, |rt| rt.game.create_at(b, cell, at, [0.0, 0.0, heading])) {
            Ok(x) => {
                for edid in STAND_IN_GEAR[aid.stand_ins.len()] {
                    let Some(item) = sim.world.forms.by_edid(edid) else { continue };
                    let worn = sim.world.add_item(x, item, 1).and_then(|_| sim.world.equip(x, item));
                    if let Err(e) = worn {
                        warn!("table spots: stand-in {base} does not wear {edid}: {e}");
                    }
                }
                aid.stand_ins.push(x)
            }
            Err(e) => {
                warn!("table spots: stand-in {base} not made: {e}");
                break;
            }
        }
    }
    let others = (0..spots.len()).filter(|&k| k != me);
    for (k, x) in others.zip(aid.stand_ins.clone()) {
        let at = [spots[k][0], spots[k][1], floor];
        if let Ok(s) = sim.world.get(x) {
            s.pos = at;
            s.rot = [0.0, 0.0, heading];
            s.cell = cell;
        }
        rt.game.teleported.insert(x);
    }
    stand_player(phys, player, spots[me], floor, heading);
    if let Some(c) = control.as_deref_mut() {
        c.portrait = Some(crate::control::Portrait { yaw: CAM_YAW, distance: CAM_DISTANCE, drop: CAM_DROP, pitch: CAM_PITCH, shift: 0.0 });
    }
    info!("table spots: showing {r:08X} {kind:?} seat {me} at {now:.1} (row a {:.0} d {:.0}, feet {:?})", pick.a.to_degrees(), pick.d, player.feet);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_row_is_centred_and_in_seat_order() {
        assert_eq!(offsets(1), vec![0.0]);
        assert_eq!(offsets(2), vec![-25.0, 25.0]);
        assert_eq!(offsets(4), vec![-75.0, -25.0, 25.0, 75.0]);
        assert_eq!(all_offsets(4), vec![-75.0, -50.0, -25.0, 0.0, 25.0, 50.0, 75.0]);
        assert_eq!(all_offsets(2), vec![-25.0, 0.0, 25.0]);
        assert_eq!(spots(Frame { pos: [0.0; 3], theta: 0.0 }, Pick { a: 0.0, d: 100.0, max: 2 }, 4).0.len(), 2);
    }

    #[test]
    fn the_spots_stand_out_from_the_table_facing_away() {
        // A table at (100, 200) turned a quarter; the row straight out along local +y.
        let f = Frame { pos: [100.0, 200.0, 0.0], theta: std::f32::consts::FRAC_PI_2 };
        let (v, heading) = spots(f, Pick { a: 0.0, d: 120.0, max: 4 }, 2);
        // Local +y turned a quarter clockwise is world +x.
        for p in &v {
            assert!((p[0] - 220.0).abs() < 1e-3, "{p:?}");
        }
        assert!((v[0][1] - v[1][1]).abs() - SPACING < 1e-3);
        let dir = crate::space::look_dir(heading, 0.0);
        assert!(dir[0] > 0.99, "faces away from the table: {dir:?}");
    }

    #[test]
    fn the_camera_is_in_front_of_the_spot() {
        let (cam, chest) = camera_at([0.0, 0.0, 100.0], 0.0);
        assert!(cam[1] > 0.9 * CAM_DISTANCE && (chest[2] - 70.0).abs() < 1e-3, "{cam:?}");
    }

    #[test]
    fn darts_rows_are_tried_in_front_of_the_board_first() {
        let (a, ds) = candidates(Kind::Darts);
        assert!((a[0] - std::f32::consts::PI).abs() < 1e-4);
        assert!(a.iter().all(|x| ang_off(*x, &[std::f32::consts::PI]) <= 75f32.to_radians() + 1e-3));
        assert!(ds[0] >= 120.0);
        let (p, _) = candidates(Kind::Pool);
        assert!(ang_off(p[0], &[0.0]) < 1e-4 && ang_off(p[1], &[std::f32::consts::PI]) < 1e-4, "{:?}", &p[..3]);
    }
}
