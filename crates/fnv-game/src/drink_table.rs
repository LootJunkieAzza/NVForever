//! The drinking contest's prop (NVForever's own mini-game [design: owner 2026-11-11; the
//! original game has no drinking contest]): a bottle of whiskey standing on a $100 NCR
//! bill on a bar table, and up to four places to stand around it. Activating the bottle
//! starts the contest (nvf-play `drink_world`); the contest itself is
//! `crate::drink_contest` (drink-a's). In the Prospector Saloon they stand on the bar
//! counter's short end by the side wall [design: owner, ship17 play, 2026-11-12: "Put the
//! dollar/bottle here and get rid of the table it was on"; the spot circled in the
//! owner's shot `drink-bar-spot.webp`].
//!
//! - **The pieces** [observed: FalloutNV.esm]: the bottle is the `Whiskey` ALCH
//!   (0x00032C75, `Clutter\LiquorBottles\WhiskeyBottle01.NIF`, bounds -4..4 by -1..1 and
//!   0..19 high, its origin at its base), the bill the `NCRMoney100` MISC (0x0013A4D8,
//!   "$100 NCR", `Clutter\Money\NCRMoney100.nif`, bounds -8..8 by -4..3 and 0 high: a flat
//!   sheet at its origin). Both are references the game creates
//!   (`fnv_sim::World::place_as`), not items moved from the plugins. They are never picked
//!   up: activating the bottle is the contest's (nvf-play `activate`), the bill is not
//!   offered, and their collision is fixed where they stand (nvf-play `phys`), so a body
//!   falling against the table does not knock them over.
//! - **One entry per bar** ([`BARS`]): the cell, the table (one the game makes, or none
//!   when the bar's own counter or table is used), where the bill lies on its top, the
//!   stand marks and the win stage. The first bar is the Prospector Saloon; another bar is
//!   one more entry.
//! - **Saves from before the move** ([`ensure`]): a piece made under a bar's number at
//!   another spot is moved to its spot, and a table made under the table number of a bar
//!   that has none now is taken away (marked deleted), so a game saved with the saloon's
//!   old round table loads with the props on the counter and no table.
//! - **Fixed numbers** ([`Bar::refs`]): every piece is made under the same number in
//!   every game, as arm wrestling's table (`crate::armwrestle_table::REFS`), so that in
//!   co-op the bottle a player activates is the same reference on every screen. A number
//!   already taken is not replaced by another: the piece is then not made (logged).
//! - **The stand marks** ([`mark`]): slot `i` of the contest stands at mark `i`, on the
//!   floor at the counter, facing it [design: up to four drinkers; standing, as at a bar].
//!
//! Log lines start `drink table:`.

use crate::game::Game;
use fnv_sim::FormId;

/// The bottle [observed: FalloutNV.esm ALCH 0x00032C75 `Whiskey`].
pub const BOTTLE_EDID: &str = "Whiskey";
/// The bill under it [observed: FalloutNV.esm MISC 0x0013A4D8 `NCRMoney100`, "$100 NCR"].
pub const BILL_EDID: &str = "NCRMoney100";
/// How far above the table's top the bill's sheet lies, so that it is drawn over the
/// table and not in it [design: a sheet of paper; the model is flat at its origin,
/// bounds 0 high].
pub const BILL_LIFT: f32 = 0.15;
/// How far above the bill's sheet the bottle's base stands [design: on the paper].
pub const BOTTLE_LIFT: f32 = 0.1;
/// The most drinkers (and marks) at one table [design: owner, up to four].
pub const MARKS: usize = 4;

/// A table the game makes for a bar.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TableDef {
    /// The table's base, by Editor ID.
    pub edid: &'static str,
    /// Its origin (game units, the cell's frame), heading (radians) and scale.
    pub pos: [f32; 3],
    pub heading: f32,
    pub scale: f32,
}

/// One bar with a drinking contest.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bar {
    /// A short name for the logs.
    pub key: &'static str,
    /// The interior cell, by Editor ID.
    pub cell: &'static str,
    /// The table the game makes, or `None` when the bill goes on one of the bar's own.
    pub table: Option<TableDef>,
    /// The point of the table's top the bill lies on (game units), its z the top
    /// surface.
    pub top: [f32; 3],
    /// The bill's and the bottle's heading (radians, the game's: 0 faces +y).
    pub heading: f32,
    /// The floor the drinkers stand on (z).
    pub floor: f32,
    /// The stand marks, slot 0 first: the feet's x and y on `floor`, and the heading the
    /// drinker faces (radians, the game's: 0 faces +y).
    pub marks: [[f32; 3]; MARKS],
    /// The numbers the table, the bill and the bottle are made under.
    pub refs: [FormId; 3],
    /// Where a player who wins dances ([`stage`]): the feet's x and y, on `floor`.
    pub stage: [f32; 2],
    /// The bearing from the stage to the win camera (radians, the game's); the winner
    /// faces it.
    pub stage_bearing: f32,
    /// The contest camera ([`contest_shot`]): x and y, and its height over `floor`, on the
    /// far side of the counter from the marks, so the drinkers are seen from the front.
    pub contest_camera: [f32; 3],
}

/// Which piece of a bar's prop a reference is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Piece {
    Table,
    Bill,
    Bottle,
}

/// The Prospector Saloon's floor [observed: `fnvtool phys cell`, rays down in the main
/// room hit 3456; the saloon's own tables and stools stand on 3456].
const SALOON_FLOOR: f32 = 3456.0;
/// The saloon's bar counter top, where the bill lies: on the counter's short end by the
/// side wall, a hand's width in from the wall [observed: FalloutNV.esm, the counter is
/// `OfficeReceptionDeskMid01` 0x00107D6E with `OfficeReceptionDeskCurveR01` 0x00107D6F at
/// (-216, -408, 3450), its end against `NVGSWallONLY` 0x00107D89 at (0, -528);
/// `fnvtool phys cell`, 2026-11-12: rays down on the end hit the curve's collision at
/// 3528.1 from x -80 to -25 and y -470 to -560, its wall-side edge is the wall's face at x
/// -19.8, its front (customers') face at y -562.4, the inner (barkeep's) edge near y -465;
/// design: the owner's circle].
const SALOON_COUNTER_TOP: [f32; 3] = [-38.0, -515.0, 3528.1];

/// The bars, in the order their fixed numbers are given.
pub const BARS: &[Bar] = &[Bar {
    key: "goodsprings",
    cell: crate::armwrestle_table::SALOON_EDID,
    table: None,
    top: SALOON_COUNTER_TOP,
    heading: 0.6,
    floor: SALOON_FLOOR,
    // A step back from the counter's front, facing across it toward the bottle, along
    // the counter from its end round its curve: slot 0 west of the stool 0x00106195 at
    // (-134, -578) by the straight run, slots 1 to 3 round the curve, slot 3 clear of the
    // stool 0x00107D6C at about (-277, -428) [design: parity, 2026-11-13, "like standing
    // at a bar: about a step back from the counter, facing across it"; the marks of
    // 2026-11-12 stood 36 from the counter's end by the side wall, so the third-person
    // camera, 40 right of the shoulder (`fOverShoulderPosX`), was pulled in against the
    // wall 0x00107D89 and the view ran along it, drink-b's frame drink_tick_640].
    // [observed: `fnvtool phys cell`, 2026-11-13: a character dropped on each mark stands
    // on the floor at 3456.7; the third-person camera's legs from each mark (40 right,
    // then 170 back, at 107 up) meet nothing; the level ray from each such camera along
    // the heading first meets a wall 339 to 468 away, and its line to the bottle is clear;
    // the wall's face (x -19.8) ends at y -656, the counter's outline at the waist runs
    // from (-20, -562) by (-129, -550) and (-219, -470) to x -265 on its long run.]
    marks: [[-125.0, -620.0, 0.42], [-205.0, -570.0, 0.89], [-245.0, -510.0, 1.40], [-300.0, -470.0, 1.05]],
    // The first number was the round table's (`RETIRED_TABLE_EDID`).
    refs: [FormId(0xFF7F_A100), FormId(0xFF7F_A101), FormId(0xFF7F_A102)],
    // South-east of the marks, off where every passed-out body falls (backwards, away from
    // the counter), the camera south-south-west of it looking up the room, so the counter's
    // end with the bottle, the back bar and the side wall's end are behind the dancer, the
    // wall's end hidden by the dancer [observed: `fnvtool phys cell`, 2026-11-13: a
    // character dropped at the stage stands on the floor at 3456.7, nothing within 60 at
    // the waist but the side wall's end 0x00107D89 at 59 to the north-east, the line from
    // the chest to the camera is clear and nothing is within 60 of the camera round it,
    // the wall NVGSWallDoor 0x00107D81 97 over it; the bar room's light
    // BasementLightKeyNeutral 0x00106C17 hangs over the counter's end at (-96, -530,
    // 3614); design: a dancer seen against the windows was a silhouette in drink-b's co-op
    // frames, 2026-11-12].
    stage: [-60.0, -700.0],
    stage_bearing: std::f32::consts::PI + 0.35,
    // Behind the counter's curve on the barkeep's side, about 35 in from its inner edge,
    // above the counter top, looking back across it at the marks' fronts [design: the
    // owner, 2026-11-13, "The game camera shouldn't be on their backs it should be facing
    // them so you can see everyone drinking"; from here slots 0 to 2 stand within 30 deg
    // of the view's centre; the bar room's lamp at (-96, -530, 3614) is 30 off the line to
    // slot 0]. Pulled in by a ray when something stands between (nvf-play drink_world).
    contest_camera: [-130.0, -420.0, 150.0],
}];

/// The win camera's distance from the winner's feet, flat (game units) [design: the whole
/// body in the lower two thirds of the frame at the play camera's field of view, under
/// the contest screen's banner and board; fitted to drink-b's 2026-11-12 frames, where at
/// 120 out, 140 up and the waist (70) the head reached the top sixth].
pub const STAGE_CAMERA_DISTANCE: f32 = 150.0;
/// ... its height over the floor [design: above the head, looking down, so the room's
/// floor and not its windows is behind the dancer].
pub const STAGE_CAMERA_UP: f32 = 160.0;
/// The point it looks at, over the winner's feet [design: the chest, so the body sits low
/// in the frame].
pub const STAGE_LOOK_UP: f32 = 100.0;

/// Where a player who wins stands and dances, and the one camera every drinker at the
/// table sees the dance through [design: one fixed shot, so that every screen shows the
/// same frame at the same moment, the light falling on the winner's front].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stage {
    /// The winner's feet, on the floor, and the heading that faces the camera.
    pub feet: [f32; 3],
    pub heading: f32,
    /// The camera and the point it looks at.
    pub camera: [f32; 3],
    pub look_at: [f32; 3],
}

/// The point the contest camera looks at, over the drinkers' centre [design: the chest].
pub const CONTEST_LOOK_UP: f32 = 100.0;

/// The contest camera of `bar` with `drinkers` at its marks: the camera across the counter
/// and the point it looks at, the drinkers' centre at the chest [design: the owner,
/// 2026-11-13, every drinker seen from the front].
pub fn contest_shot(bar: &Bar, drinkers: usize) -> ([f32; 3], [f32; 3]) {
    let n = drinkers.clamp(1, MARKS);
    let (mut x, mut y) = (0.0, 0.0);
    for slot in 0..n {
        let (p, _) = mark(bar, slot);
        x += p[0];
        y += p[1];
    }
    let [cx, cy, up] = bar.contest_camera;
    ([cx, cy, bar.floor + up], [x / n as f32, y / n as f32, bar.floor + CONTEST_LOOK_UP])
}

/// The win stage of `bar`.
pub fn stage(bar: &Bar) -> Stage {
    let (s, c) = bar.stage_bearing.sin_cos();
    let [x, y] = bar.stage;
    Stage {
        feet: [x, y, bar.floor],
        heading: wrap(bar.stage_bearing),
        camera: [x + STAGE_CAMERA_DISTANCE * s, y + STAGE_CAMERA_DISTANCE * c, bar.floor + STAGE_CAMERA_UP],
        look_at: [x, y, bar.floor + STAGE_LOOK_UP],
    }
}

/// How near its spot a created reference must be to be a piece of the prop [design].
const SAME_SPOT: f32 = 6.0;
/// The base of a bar's table of before: the one made under a bar's table number is taken
/// away when a save with it loads and the bar has no table now [observed: the saloon's
/// round table, a `Table01` at (215, -725, 3486) and scale 1.2, until 2026-11-12].
pub const RETIRED_TABLE_EDID: &str = "Table01";

impl Bar {
    /// Where the bill lies and the bottle stands (their origins).
    pub fn bill_pos(&self) -> [f32; 3] {
        [self.top[0], self.top[1], self.top[2] + BILL_LIFT]
    }

    pub fn bottle_pos(&self) -> [f32; 3] {
        [self.top[0], self.top[1], self.top[2] + BILL_LIFT + BOTTLE_LIFT]
    }

    /// The piece made under `r`, when it is one of this bar's numbers.
    pub fn piece(&self, r: FormId) -> Option<Piece> {
        match self.refs.iter().position(|&x| x == r)? {
            0 => Some(Piece::Table),
            1 => Some(Piece::Bill),
            _ => Some(Piece::Bottle),
        }
    }
}

/// Where mark `slot` of `bar` is (the feet, on the floor) and the heading that faces the
/// counter (or table).
pub fn mark(bar: &Bar, slot: usize) -> ([f32; 3], f32) {
    let [x, y, h] = bar.marks[slot % MARKS];
    ([x, y, bar.floor], wrap(h))
}

fn wrap(a: f32) -> f32 {
    a.rem_euclid(std::f32::consts::TAU)
}

/// The bar and piece a reference is, if it is one of the props' fixed numbers.
pub fn piece_of(r: FormId) -> Option<(&'static Bar, Piece)> {
    BARS.iter().find_map(|b| b.piece(r).map(|p| (b, p)))
}

/// Whether `r` is a piece whose collision stays where it was made (the bill and the
/// bottle; a table is a static anyway).
pub fn fixed_in_place(r: FormId) -> bool {
    piece_of(r).is_some_and(|(_, p)| p != Piece::Table)
}

/// The bar whose bottle `r` is.
pub fn bar_of_bottle(r: FormId) -> Option<&'static Bar> {
    piece_of(r).filter(|(_, p)| *p == Piece::Bottle).map(|(b, _)| b)
}

/// The created reference of `base` in `cell` standing at `pos`.
fn created_at(g: &Game, base: FormId, cell: FormId, pos: [f32; 3]) -> Option<FormId> {
    g.world
        .created()
        .find(|(_, s)| s.base == base && s.cell == Some(cell) && !s.deleted() && (0..3).all(|i| (s.pos[i] - pos[i]).abs() <= SAME_SPOT))
        .map(|(id, _)| id)
}

/// What a bar's pieces are to be: (number, base, position, heading, scale).
fn wanted(g: &Game, bar: &Bar) -> Vec<(FormId, FormId, [f32; 3], f32, f32)> {
    let mut want = Vec::new();
    if let Some(t) = bar.table {
        match g.data.by_edid(t.edid) {
            Some(base) => want.push((bar.refs[0], base, t.pos, t.heading, t.scale)),
            None => return Vec::new(),
        }
    }
    let (Some(bill), Some(bottle)) = (g.data.by_edid(BILL_EDID), g.data.by_edid(BOTTLE_EDID)) else { return Vec::new() };
    want.push((bar.refs[1], bill, bar.bill_pos(), bar.heading, 1.0));
    want.push((bar.refs[2], bottle, bar.bottle_pos(), bar.heading, 1.0));
    want
}

/// Make the pieces of every bar whose cell is loaded and that lack them (any one missing
/// is made again), under the bar's fixed numbers: a co-op guest makes its own under the
/// same numbers as the host's. Nothing with a load order lacking the cell or the bases.
/// Returns what was made.
pub fn ensure(g: &mut Game) -> Vec<FormId> {
    let mut made = Vec::new();
    for bar in BARS {
        let Some(cell) = g.data.by_edid(bar.cell) else { continue };
        if !g.loaded_cells.contains(&cell) {
            continue;
        }
        if bar.table.is_none() {
            retire_table(g, bar, cell);
        }
        for (id, base, pos, heading, scale) in wanted(g, bar) {
            if created_at(g, base, cell, pos).is_some() {
                continue;
            }
            if let Ok(s) = g.world.get(id) {
                if s.base == base && s.cell == Some(cell) && !s.deleted() {
                    // Made at the spot of before (a save from then): moved to this one.
                    let was = s.pos;
                    s.pos = pos;
                    s.rot = [0.0, 0.0, heading];
                    tracing::info!(
                        "drink table: {}: moved {} {id} from ({:.1}, {:.1}, {:.2}) to ({:.1}, {:.1}, {:.2})",
                        bar.key,
                        g.data.name(base),
                        was[0],
                        was[1],
                        was[2],
                        pos[0],
                        pos[1],
                        pos[2]
                    );
                    continue;
                }
            }
            match g.world.place_as(id, base, Some(cell), pos, [0.0, 0.0, heading]) {
                Ok(r) => {
                    if scale != 1.0 {
                        if let Ok(s) = g.world.get(r) {
                            s.scale = Some(scale);
                        }
                    }
                    tracing::info!("drink table: {}: made {} {r} at ({:.1}, {:.1}, {:.2})", bar.key, g.data.name(base), pos[0], pos[1], pos[2]);
                    made.push(r);
                }
                Err(e) => tracing::warn!("drink table: {}: {} not made under {id}: {e}", bar.key, g.data.name(base)),
            }
        }
    }
    made
}

/// Take away the table made under `bar`'s table number, in a save from before the bar
/// lost its table.
fn retire_table(g: &mut Game, bar: &Bar, cell: FormId) {
    let id = bar.refs[0];
    let Some(table) = g.data.by_edid(RETIRED_TABLE_EDID) else { return };
    if !g.world.get(id).is_ok_and(|s| s.base == table && s.cell == Some(cell) && !s.deleted()) {
        return;
    }
    if g.world.set_deleted(id, true).is_ok() {
        tracing::info!("drink table: {}: the table {id} of before is taken away", bar.key);
    }
}

/// The bottle of `bar`, when it is made and still there.
pub fn bottle(g: &Game, bar: &Bar) -> Option<FormId> {
    let cell = g.data.by_edid(bar.cell)?;
    let base = g.data.by_edid(BOTTLE_EDID)?;
    created_at(g, base, cell, bar.bottle_pos()).filter(|&r| r == bar.refs[2])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_marks_stand_apart_at_the_counter_facing_it() {
        let bar = &BARS[0];
        let near = |slot| {
            let (p, _) = mark(bar, slot);
            (bar.top[0] - p[0]).hypot(bar.top[1] - p[1])
        };
        for slot in 0..MARKS {
            let (p, h) = mark(bar, slot);
            assert_eq!(p[2], bar.floor);
            // Heading h faces (sin h, cos h): towards the counter, where the bottle is.
            let (dx, dy) = (bar.top[0] - p[0], bar.top[1] - p[1]);
            assert!(dx * h.sin() + dy * h.cos() > 0.0, "slot {slot} faces away from the counter");
            // Two bodies (radius 22) never overlap.
            for other in slot + 1..MARKS {
                let (q, _) = mark(bar, other);
                assert!((p[0] - q[0]).hypot(p[1] - q[1]) > 44.0, "marks {slot} and {other} overlap");
            }
            // The first drinker stands nearest the bottle.
            assert!(near(0) <= near(slot));
        }
        // The saloon's marks are on the customers' side of the counter's front (y -562 on
        // its straight run) and off the side wall (x -20).
        for slot in 0..MARKS {
            let (p, _) = mark(bar, slot);
            assert!(p[0] < -20.0 - 22.0, "slot {slot} is in the wall");
        }
        // Slot 0 by the straight run; slot 1 by the curve, whose face at x -204 is at y -490
        // [observed: `fnvtool phys cell` rays at the waist, 2026-11-13].
        assert!(mark(bar, 0).0[1] < -562.4 - 22.0 && mark(bar, 1).0[1] < -490.0 - 22.0);
    }

    /// The contest camera sees the drinkers from the front: it stands ahead of each one
    /// (within 45 deg of where it faces), across the counter's front from the marks and
    /// above its top, and with two or three drinkers all of them stand within 32 deg of
    /// the view's centre (the play camera's field of view is wider).
    #[test]
    fn the_contest_camera_faces_the_drinkers() {
        let bar = &BARS[0];
        for n in 1..=MARKS {
            let (cam, look) = contest_shot(bar, n);
            assert!(cam[2] > bar.top[2] + 40.0, "above the counter top");
            assert!(look[2] < cam[2]);
            let centre = (look[0] - cam[0]).atan2(look[1] - cam[1]);
            for slot in 0..n {
                let (p, h) = mark(bar, slot);
                let (dx, dy) = (cam[0] - p[0], cam[1] - p[1]);
                let off = angle(dx.atan2(dy) - h).abs().to_degrees();
                assert!(off < 45.0, "{n} drinkers: slot {slot} has the camera {off:.0} deg off its front");
                if n <= 3 {
                    let at = angle((p[0] - cam[0]).atan2(p[1] - cam[1]) - centre).abs().to_degrees();
                    assert!(at < 32.0, "{n} drinkers: slot {slot} is {at:.0} deg off the view's centre");
                }
            }
            // Across the counter's front (y -562.4 on its straight run, -550 by the curve)
            // from the marks: behind it, on the barkeep's side.
            assert!(cam[1] > -465.0 && cam[0] < -20.0, "{cam:?} is not behind the counter");
        }
    }

    fn angle(a: f32) -> f32 {
        (a + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI
    }

    #[test]
    fn the_winner_faces_the_win_camera_away_from_the_counter_and_the_marks() {
        let bar = &BARS[0];
        let st = stage(bar);
        // Facing the camera: heading h faces (sin h, cos h).
        let (dx, dy) = (st.camera[0] - st.feet[0], st.camera[1] - st.feet[1]);
        let flat = dx.hypot(dy);
        assert!((flat - STAGE_CAMERA_DISTANCE).abs() < 1e-3);
        assert!((dx * st.heading.sin() + dy * st.heading.cos()) / flat > 0.999);
        // Off the counter, a body's length from every mark and off where each body falls:
        // backwards from its mark, a body's height (130) long.
        assert!((st.feet[0] - bar.top[0]).hypot(st.feet[1] - bar.top[1]) > 100.0);
        for slot in 0..MARKS {
            let (m, h) = mark(bar, slot);
            assert!((st.feet[0] - m[0]).hypot(st.feet[1] - m[1]) > 60.0, "the stage is on mark {slot}");
            let (s, c) = h.sin_cos();
            let near = (0..=13)
                .map(|k| k as f32 * 10.0)
                .map(|t| (st.feet[0] - (m[0] - s * t)).hypot(st.feet[1] - (m[1] - c * t)))
                .fold(f32::MAX, f32::min);
            assert!(near > 60.0, "the stage is where slot {slot} falls ({near:.0})");
        }
        // The camera in the bar room (its side wall at x -20, its far wall about y -880,
        // open floor west to x -496), on the customers' side, above the head.
        assert!(st.camera[0] < -40.0 && st.camera[0] > -480.0 && st.camera[1] > -870.0 && st.camera[1] < st.feet[1]);
        assert!(st.camera[2] > st.look_at[2] && st.look_at[2] > st.feet[2]);
    }

    #[test]
    fn the_bottle_stands_on_the_bill_on_the_counter_top() {
        for bar in BARS {
            let (bill, bottle) = (bar.bill_pos(), bar.bottle_pos());
            assert!(bill[2] > bar.top[2] && bill[2] - bar.top[2] < 0.5);
            assert!(bottle[2] > bill[2] && bottle[2] - bill[2] < 0.5);
            assert_eq!((bill[0], bill[1]), (bottle[0], bottle[1]));
        }
        // The saloon's: on the counter's collision top, inside its end, off the wall.
        let bar = &BARS[0];
        assert!(bar.table.is_none());
        assert!((bar.top[2] - 3528.1).abs() < 1e-3);
        assert!(bar.top[0] < -19.8 - 8.0 && bar.top[0] > -80.0 && bar.top[1] < -470.0 && bar.top[1] > -562.4 + 8.0);
    }

    #[test]
    fn the_fixed_numbers_are_dynamic_distinct_and_apart_from_arm_wrestling() {
        let mut all: Vec<FormId> = BARS.iter().flat_map(|b| b.refs).collect();
        all.extend(crate::armwrestle_table::REFS);
        let n = all.len();
        all.sort();
        all.dedup();
        assert_eq!(all.len(), n);
        assert!(all.iter().all(|r| r.is_dynamic()));
        let bar = &BARS[0];
        assert_eq!(piece_of(bar.refs[2]).map(|(_, p)| p), Some(Piece::Bottle));
        assert!(fixed_in_place(bar.refs[1]) && fixed_in_place(bar.refs[2]) && !fixed_in_place(bar.refs[0]));
        assert!(bar_of_bottle(bar.refs[2]).is_some() && bar_of_bottle(bar.refs[1]).is_none());
    }
}
