//! The drinking contest's prop (NVForever's own mini-game [design: owner 2026-11-11; the
//! original game has no drinking contest]): a bottle of whiskey standing on a $100 NCR
//! bill on a bar table, and up to four places to stand around it. Activating the bottle
//! starts the contest (nvf-play `drink_world`); the contest itself is
//! `crate::drink_contest` (drink-a's).
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
//!   when the bar's own table is used), where the bill lies on its top, and the stand
//!   marks. The first bar is the Prospector Saloon; another bar is one more entry.
//! - **Fixed numbers** ([`Bar::refs`]): every piece is made under the same number in
//!   every game, as arm wrestling's table (`crate::armwrestle_table::REFS`), so that in
//!   co-op the bottle a player activates is the same reference on every screen. A number
//!   already taken is not replaced by another: the piece is then not made (logged).
//! - **The stand marks** ([`mark`]): slot `i` of the contest stands at mark `i`, on the
//!   floor around the table, facing its centre [design: up to four drinkers; standing,
//!   as the saloon's bar tables are stood at].
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
    /// The marks' distance from the table's centre (`top`'s x and y).
    pub mark_radius: f32,
    /// The marks' bearings from the centre (radians, the game's), slot 0 first.
    pub mark_bearings: [f32; MARKS],
    /// The numbers the table, the bill and the bottle are made under.
    pub refs: [FormId; 3],
    /// Where a player who wins dances ([`stage`]): the feet's x and y, on `floor`.
    pub stage: [f32; 2],
    /// The bearing from the stage to the win camera (radians, the game's); the winner
    /// faces it.
    pub stage_bearing: f32,
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
/// The saloon's table: a `Table01` at scale 1.2, as the saloon's own bar tables
/// [observed: FalloutNV.esm, Table01 0x0010663B, 0x00107D17 and 0x00127633 in the saloon,
/// each at scale 1.2 and 30 above the floor; Table01's bounds are 31 from its centre and
/// 24 above and below its origin].
const SALOON_TABLE: TableDef = TableDef { edid: "Table01", pos: [215.0, -725.0, SALOON_FLOOR + 30.0], heading: 0.0, scale: 1.2 };
/// Table01's top above its origin at scale 1 [observed: the top of the collision mesh of
/// `meshes\furniture\table01.nif`, 30.99 above the origin (`fnvtool collide objects`), its
/// foot 25.35 below; the record's bounds say 24 and put the bill inside the top, seen in
/// the first shot].
pub const TABLE01_TOP: f32 = 31.0;

/// The bars, in the order their fixed numbers are given.
pub const BARS: &[Bar] = &[Bar {
    key: "goodsprings",
    cell: crate::armwrestle_table::SALOON_EDID,
    table: Some(SALOON_TABLE),
    top: [SALOON_TABLE.pos[0], SALOON_TABLE.pos[1], SALOON_TABLE.pos[2] + TABLE01_TOP * SALOON_TABLE.scale],
    heading: 0.6,
    floor: SALOON_FLOOR,
    mark_radius: 62.0,
    // North, east, south and west of the table: the first two drinkers face each other.
    mark_bearings: [0.0, std::f32::consts::PI, std::f32::consts::FRAC_PI_2, 3.0 * std::f32::consts::FRAC_PI_2],
    refs: [FormId(0xFF7F_A100), FormId(0xFF7F_A101), FormId(0xFF7F_A102)],
    // North-east of the table, on open floor under the saloon's lamp 0x0016CD72 at (281,
    // -603), out of the way of the bodies passed out at marks 0 (falling north) and 2
    // (falling east); the camera south-east of it, between the winner and the windows,
    // looks north-west into the room, an inner wall behind the dancer [observed:
    // FalloutNV.esm, the saloon's windows NVGSRmWindowSeparate stand in the south wall at
    // y -872 and the east wall at x 488, its tables Table01 0x00127633 at (396, -629) and
    // Table02 0x00107D18 at (368, -770); `fnvtool phys cell`: the floor at (300, -620) is
    // 3456 with nothing within 60 at the waist, the line to the camera is clear, and the
    // line on north-west past the dancer meets the wall NVGSWallONLY 0x00107D8A at 398;
    // design: a dancer seen against the windows was a silhouette in drink-b's co-op
    // frames, 2026-11-12].
    stage: [300.0, -620.0],
    stage_bearing: 3.0 * std::f32::consts::FRAC_PI_4,
}];

/// The win camera's distance from the winner's feet, flat (game units) [design: the whole
/// body in the frame at the play camera's field of view].
pub const STAGE_CAMERA_DISTANCE: f32 = 120.0;
/// ... its height over the floor [design: above the head, looking down, so the room's
/// floor and not its windows is behind the dancer].
pub const STAGE_CAMERA_UP: f32 = 140.0;
/// The point it looks at, over the winner's feet [design: about the waist].
pub const STAGE_LOOK_UP: f32 = 70.0;

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
/// table's centre.
pub fn mark(bar: &Bar, slot: usize) -> ([f32; 3], f32) {
    let b = bar.mark_bearings[slot % MARKS];
    // Heading h faces (sin h, cos h).
    let (s, c) = b.sin_cos();
    let pos = [bar.top[0] + bar.mark_radius * s, bar.top[1] + bar.mark_radius * c, bar.floor];
    (pos, wrap(b + std::f32::consts::PI))
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
        for (id, base, pos, heading, scale) in wanted(g, bar) {
            if created_at(g, base, cell, pos).is_some() {
                continue;
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
    fn the_marks_stand_around_the_table_facing_its_centre() {
        let bar = &BARS[0];
        for slot in 0..MARKS {
            let (p, h) = mark(bar, slot);
            let (dx, dy) = (bar.top[0] - p[0], bar.top[1] - p[1]);
            assert!(((dx * dx + dy * dy).sqrt() - bar.mark_radius).abs() < 1e-3);
            // Heading h faces (sin h, cos h): towards the centre.
            let towards = (dx * h.sin() + dy * h.cos()) / bar.mark_radius;
            assert!(towards > 0.999, "slot {slot} faces away ({towards})");
            assert_eq!(p[2], bar.floor);
        }
        // Slots 0 and 1 face each other across the table.
        let ((a, _), (b, _)) = (mark(bar, 0), mark(bar, 1));
        assert!(((a[0] - b[0]).hypot(a[1] - b[1]) - 2.0 * bar.mark_radius).abs() < 1e-3);
    }

    #[test]
    fn the_winner_faces_the_win_camera_away_from_the_table_and_the_marks() {
        let bar = &BARS[0];
        let st = stage(bar);
        // Facing the camera: heading h faces (sin h, cos h).
        let (dx, dy) = (st.camera[0] - st.feet[0], st.camera[1] - st.feet[1]);
        let flat = dx.hypot(dy);
        assert!((flat - STAGE_CAMERA_DISTANCE).abs() < 1e-3);
        assert!((dx * st.heading.sin() + dy * st.heading.cos()) / flat > 0.999);
        // Off the table (Table01 at 1.2 reaches 37 from its centre) and a body's length
        // from every mark.
        assert!((st.feet[0] - bar.top[0]).hypot(st.feet[1] - bar.top[1]) > 100.0);
        for slot in 0..MARKS {
            let (m, _) = mark(bar, slot);
            assert!((st.feet[0] - m[0]).hypot(st.feet[1] - m[1]) > 60.0, "the stage is on mark {slot}");
        }
        // The camera inside the saloon's window walls (x 488, y -872), above the head.
        assert!(st.camera[0] < 470.0 && st.camera[1] > -860.0);
        assert!(st.camera[2] > st.look_at[2] && st.look_at[2] > st.feet[2]);
    }

    #[test]
    fn the_bottle_stands_on_the_bill_on_the_table_top() {
        for bar in BARS {
            let (bill, bottle) = (bar.bill_pos(), bar.bottle_pos());
            assert!(bill[2] > bar.top[2] && bill[2] - bar.top[2] < 0.5);
            assert!(bottle[2] > bill[2] && bottle[2] - bill[2] < 0.5);
            assert_eq!((bill[0], bill[1]), (bottle[0], bottle[1]));
            if let Some(t) = bar.table {
                assert!((bar.top[2] - (t.pos[2] + TABLE01_TOP * t.scale)).abs() < 1e-3);
            }
        }
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
