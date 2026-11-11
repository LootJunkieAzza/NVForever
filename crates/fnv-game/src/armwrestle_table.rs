//! The arm-wrestling table of the Prospector Saloon (queue item 199): where it stands and
//! who comes to sit across from the player. NVForever's own addition [design: owner
//! 2026-11-10; the original game has no arm wrestling]; the bout itself is
//! `crate::armwrestle`.
//!
//! - **The furniture** ([`ensure`]): a table and two chairs, references the game creates
//!   (`fnv_sim::World::place`), not ones moved from the plugins, laid out as the game
//!   seats two people facing each other at a table: the Fort's plank workbench
//!   `NVGrinderWorkbench` (STAT, a top 19 from its centre across and 40 along, on X legs
//!   at its two ends), at scale 1.1, with a `Chair01TableR` and a `Chair01TableL` (FURN)
//!   across its narrow side, 26 above the floor it stands on, each facing it [observed:
//!   FalloutNV.esm, the game's own pairs of chairs at a table, e.g. in cell 0x00130B8C a
//!   Chair01TableR at (24, -57) of a dinertable01NV facing it and a Chair01TableL at
//!   (23, 60) turned 175 degrees, both 27 higher than the table]. The table is the
//!   narrowest vanilla table with room for the knees under it, so that the wrestlers'
//!   forearms stand upright (owner, 2026-11-11: the grip over the diner table, whose top
//!   reaches 28.3 across its narrow side, looked bad; the owner's pick of five shot in
//!   the saloon, 2026-11-11). Every STAT, FURN, MSTT, ACTI and CONT record of FalloutNV.esm
//!   was measured by its model's vertices, and every mesh of the base and DLC archives:
//!   the only narrower tops at that height, SlotMachineTable01 to 03 (18.3 from the
//!   centre, 49.5 high), are closed cabinets the knees would go into, and so are the
//!   barrels (NVWoodBarrel, top 22.3, bulging to 28.4 at knee height) [observed:
//!   Fallout - Meshes.bsa, 2026-11-11]. The
//!   chairs are entered from the side, each enabling one side marker (MNAM 0x40000002
//!   the right one, 0x40000001 the left [observed]); R on one side and L on the other, as
//!   the game pairs them, are both entered from the same side of the table, here the open
//!   hall to its south. The seated body lands on the chair's origin facing the table
//!   (docs/spec-notes/furniture.md section 4). The chairs stand nearer the table than the
//!   game sets them, 35 from its centre instead of 55 to 68, so that the wrestlers'
//!   arms can meet over it ([`SEAT_OFFSET`]). The first layout, Table02 at scale 1.2 with a Stool01L at either long end, sat
//!   the wrestlers too far apart to reach across, and the stool's sitting pose leans
//!   into the table's edge (owner's run 5, 2026-11-11). They are made when the saloon
//!   loads and they are not there; a save keeps them like any created reference. They
//!   are made under fixed numbers ([`REFS`]), the same in every game, so that in co-op
//!   the host's and each guest's table are one reference: the furniture a player or the
//!   opponent sits on is named alike on every screen (`fnv_sim` numbers a guest's own
//!   created references apart from the host's) [design: armwrestle-b]. The first
//!   layout's table and stools, in a game saved with them, are taken away
//!   ([`retire_first_layout`]).
//! - **The spot** ([`TABLE_POS`]): the east end of the back hall to the restrooms, the
//!   spot armwrestle-a chose, the table turned so that the chairs sit west and east of it,
//!   its north end against the hall's north wall (owner, 2026-11-11). That wall has a
//!   wooden ledge from 52 to 110 above the floor standing out to y 361, and under it the
//!   wall is at y 382.5 [observed: `fnvtool phys cell`, rays north at x 300 to 410 and
//!   down at x 350, 2026-11-11]; the table's top (51.7 at scale 1.1) goes under the ledge
//!   and its end stands half a unit from the wall below it. The footprint of table,
//!   chairs and their entry points (x 267 to 433, y 236 to 358, checked with the diner
//!   table and the chairs at 59; the chairs at 35 and their entry points stand inside it,
//!   the table's north half reaches the wall, where no path goes)
//!   is clear: rays along its edges, diagonals and midlines at 10, 44 and 84 units above
//!   the floor hit nothing, the floor under the table and both chairs is at 3456, and the
//!   navmesh paths between the saloon's doors, its stools, bar, idle markers and the
//!   settler keep at least 27 units from it, besides the 24 units the paths keep from
//!   walls [observed: `fnvtool phys cell` and `fnvtool nav path`, 2026-11-11; the back
//!   door 0x00108BD6 has no path]. The table does not cut the navmesh: the hall's east
//!   end is four large triangles (149994, 149995, 150007, 150009), which a cut under its
//!   bounds takes whole, the floor at both seats with them, so the opponent could not
//!   reach his seat [observed: armwrestle-b's run 4, "the goal cannot be reached (0
//!   triangles searched)"]; no path crosses the spot.
//! - **The opponent** ([`OPPONENT`]): the Goodsprings Settler (GSSettlerCM), whose package
//!   GoodspringsSettlerSaloonPackage (Travel, near the bar stool 0x00106192) keeps him on
//!   a stool at the bar all day; he sleeps from 22:00 for 8 hours
//!   (GSSettler04SleepPackage22x8) [observed: the NPC_ and PACK records]. He has no
//!   quest of his own, so a minute at the table takes nothing from the game [design]. The
//!   offer is the table's stools: a player who sits on one with nobody across is joined
//!   by him, when he is about ([`opponent_available`]) [design: lighter than a dialogue
//!   topic]. He walks there on the navmesh and sits through his AI
//!   ([`call_opponent`]: `RefExtra::seat_override`, read by `fnv_ai`), and gets up and
//!   goes back to his package after the bout ([`release_opponent`]).

use crate::game::Game;
use fnv_sim::FormId;

/// GSProspectorSaloonInterior [observed: FalloutNV.esm CELL 0x00106185].
pub const SALOON_EDID: &str = "GSProspectorSaloonInterior";
/// The Fort's plank workbench [observed: STAT 0x00127B85 NVGrinderWorkbench,
/// architecture/TheFort/NV_Grinder-Workbench.NIF: a top 40 by 19 from its centre and 47
/// above its origin on the floor; five references in FalloutNV.esm].
pub const TABLE_EDID: &str = "NVGrinderWorkbench";
/// The two chairs, seat 0 and seat 1: the game's pair of diner chairs [observed: FURN
/// 0x0006F8C3 Chair01TableR, 0x0006F8C2 Chair01TableL], kept from the diner-table layout
/// [design: the saloon's own kind of chair].
pub const SEAT_EDIDS: [&str; 2] = ["Chair01TableR", "Chair01TableL"];

/// The floor at the spot [observed: `fnvtool phys cell`, rays down at the table and both
/// chairs hit 3456].
pub const FLOOR_Z: f32 = 3456.0;
/// The table's centre (game units, the cell's frame; its origin is on the floor), heading
/// and scale [design: the spot, armwrestle-a's; heading pi/2 turns its narrow side (the
/// mesh's y) to face west and east]. At scale 1 the battens under its top (37 to 42 above
/// the floor, 8 to 14 from the centre across [observed: nv_grinder-workbench.nif]) are
/// at the height of a seated body's thighs (the chair's seat about 32 above the floor,
/// chair01.nif, and a thigh about 10 thick [hypothesis]); at 1.1 they are 40.7 to 46.2,
/// the top 51.7 (under the ledge of the wall behind it, at 52) and 20.9 from the centre
/// across, 44.3 along. The centre is 44.8 from the wall [observed: the wall at y 382.5].
pub const TABLE_POS: [f32; 3] = [350.0, 337.7, FLOOR_Z];
pub const TABLE_HEADING: f32 = std::f32::consts::FRAC_PI_2;
pub const TABLE_SCALE: f32 = 1.1;
/// How high the table's top is above its origin, at scale 1 [observed:
/// nv_grinder-workbench.nif, the top's vertices at 42 to 47].
pub const TABLE_TOP: f32 = 47.0;
/// The game z of the table's top surface (the clasped hands go above it, over the table's
/// x and y) [observed: the floor and the mesh, times the scale].
pub const TABLE_TOP_Z: f32 = FLOOR_Z + TABLE_TOP * TABLE_SCALE;
/// The chairs' distance from the table's centre across its narrow side: as near as the
/// seated body allows [design]. The game sets Chair01 at 57 to 60 from this table's
/// centre [observed], where the wrestlers' shoulders are about 130 apart and their arms,
/// about 35 long each, cannot meet [observed by armwrestle-a: the shoulders at x 286
/// and 415, the run of 2026-11-11 14:10, about 5 units behind each chair's origin].
/// The workbench's top, at scale 1.1, reaches 20.9 from its centre across and is 46.2
/// to 51.7 above the floor; its legs cross at its two ends, 39 and more along it from the
/// centre, out of the knees' way [observed: nv_grinder-workbench.nif, times 1.1]. At 35
/// the seated chest (taken as about 8 in front of the chair's origin [hypothesis], as
/// measured against the diner table at 42) stays about 6 clear of the edge, the chair's
/// seat front (20 in front of its origin, chair01.nif) goes 6 under the top, and the
/// knees (about 30 in front of the chair's origin [hypothesis: a seated body]) come to
/// about 5 from the centre, short of the other wrestler's; the shoulders (about 5 behind
/// the chair's origin [observed by armwrestle-a]) are about 60 apart.
pub const SEAT_OFFSET: f32 = 35.0;
/// The chairs' height [observed: 26 and 27 over the table's origin for Chair01 at diner
/// tables; the chair's markers are 26 under its origin].
pub const SEAT_Z: f32 = FLOOR_Z + 26.0;
/// Pieces of earlier layouts, made by games before 2026-11-11 15:00: base and place
/// [design: retired, see the module notes]. The first layout (Table02 at scale 1.2, a
/// Stool01L at either long end), then the diner table with the chairs at 59 from its
/// centre, then at 42 and 38 by the round casino table, all at y 300 (one entry takes
/// the chairs at 308 and 312, another those at 388 and 392).
const FIRST_LAYOUT: [(&str, [f32; 3]); 9] = [
    ("Table02", [350.0, 300.0, 3486.0]),
    ("Stool01L", [266.0, 300.0, 3474.0]),
    ("Stool01L", [434.0, 300.0, 3474.0]),
    ("Chair01TableR", [291.0, 300.0, 3482.0]),
    ("Chair01TableL", [409.0, 300.0, 3482.0]),
    ("dinertable01NV", [350.0, 300.0, 3456.0]),
    ("NVCasinoTableG", [350.0, 300.0, 3456.0]),
    ("Chair01TableR", [310.0, 300.0, 3482.0]),
    ("Chair01TableL", [390.0, 300.0, 3482.0]),
];
/// The numbers the table and its two seats are made under, in every game [design: in
/// the dynamic range, below a co-op guest's own (from 0xFF800000) and far above any count
/// the host's game reaches; armwrestle-b]. One already taken (never seen) falls back to
/// the next free number.
pub const REFS: [FormId; 3] = [FormId(0xFF7F_A000), FormId(0xFF7F_A001), FormId(0xFF7F_A002)];

/// How near its spot a created reference must be to be the table's (or a seat's) [design].
const SAME_SPOT: f32 = 6.0;

/// The Goodsprings Settler's reference [observed: ACHR 0x00104F08 of NPC_ GSSettlerCM,
/// placed in the saloon, Strength 5].
pub const OPPONENT: FormId = FormId(0x0010_4F08);
/// How far from the table the opponent may be to be called over (the saloon is about 1000
/// across) [design].
pub const OPPONENT_REACH: f32 = 1600.0;

/// The table and its two seats, as created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmTable {
    pub table: FormId,
    /// The seats (chairs; the name is the first layout's): seat 0 to the table's west
    /// (facing east), seat 1 to its east (facing west).
    pub stools: [FormId; 2],
}

/// Where seat `seat` stands and its heading (radians, the game's: 0 faces +y, pi/2 +x):
/// across the table's narrow side (its local y axis), each facing the table [observed:
/// the game's pairs, the chair at local -y turned as the table, the one at +y half
/// round].
pub fn stool_spot(seat: u8) -> ([f32; 3], f32) {
    let side = if seat == 0 { -1.0 } else { 1.0 };
    let (s, c) = TABLE_HEADING.sin_cos();
    // The table's local y axis in the cell's frame: heading h turns +y to (sin h, cos h).
    let (ax, ay) = (s, c);
    let pos = [TABLE_POS[0] + side * SEAT_OFFSET * ax, TABLE_POS[1] + side * SEAT_OFFSET * ay, SEAT_Z];
    let face = if seat == 0 { 0.0 } else { std::f32::consts::PI };
    (pos, wrap(TABLE_HEADING + face))
}

fn wrap(a: f32) -> f32 {
    a.rem_euclid(std::f32::consts::TAU)
}

fn saloon(g: &Game) -> Option<FormId> {
    g.data.by_edid(SALOON_EDID)
}

/// The created reference of `base` in `cell` standing at `pos`, if there is one.
fn created_at(g: &Game, base: FormId, cell: FormId, pos: [f32; 3]) -> Option<FormId> {
    g.world
        .created()
        .find(|(_, s)| s.base == base && s.cell == Some(cell) && !s.deleted() && (0..3).all(|i| (s.pos[i] - pos[i]).abs() <= SAME_SPOT))
        .map(|(id, _)| id)
}

/// The test aid naming another table base to make in the table's place, for comparison
/// shots (`NVF_PLAY_TEST_ARMWRESTLE_TABLE=<editor ID>[:<yaw in degrees>]`, the yaw
/// added to [`TABLE_HEADING`]): only how it looks; the arms still take [`TABLE_TOP_Z`]
/// and the chairs stay where they are [design].
pub const TABLE_AID: &str = "NVF_PLAY_TEST_ARMWRESTLE_TABLE";

/// The table's base: [`TABLE_EDID`], or the one [`TABLE_AID`] names.
fn table_base(g: &Game) -> Option<FormId> {
    match std::env::var(TABLE_AID) {
        Ok(e) if !e.trim().is_empty() => g.data.by_edid(e.split(':').next().unwrap_or("").trim()),
        _ => g.data.by_edid(TABLE_EDID),
    }
}

/// The table's heading: [`TABLE_HEADING`], turned by the yaw [`TABLE_AID`] gives.
fn table_heading() -> f32 {
    let turn = std::env::var(TABLE_AID).ok().and_then(|e| e.split(':').nth(1).and_then(|y| y.trim().parse::<f32>().ok())).unwrap_or(0.0);
    wrap(TABLE_HEADING + turn.to_radians())
}

/// The table and stools, when they are made.
pub fn find(g: &Game) -> Option<ArmTable> {
    let cell = saloon(g)?;
    let table = created_at(g, table_base(g)?, cell, TABLE_POS)?;
    let s0 = created_at(g, g.data.by_edid(SEAT_EDIDS[0])?, cell, stool_spot(0).0)?;
    let s1 = created_at(g, g.data.by_edid(SEAT_EDIDS[1])?, cell, stool_spot(1).0)?;
    Some(ArmTable { table, stools: [s0, s1] })
}

/// Which seat of the table `stool` is.
pub fn seat_of(g: &Game, stool: FormId) -> Option<u8> {
    let t = find(g)?;
    t.stools.iter().position(|&s| s == stool).map(|i| i as u8)
}

/// Make the table and stools when the saloon is loaded and they are not there (any one
/// missing is made again), under [`REFS`]: a co-op guest makes its own under the same
/// numbers as the host's. Nothing with a load order lacking the cell or the bases.
/// Returns what was made.
pub fn ensure(g: &mut Game) -> Vec<FormId> {
    let mut made = Vec::new();
    let (Some(cell), Some(table), Some(seat0), Some(seat1)) = (saloon(g), table_base(g), g.data.by_edid(SEAT_EDIDS[0]), g.data.by_edid(SEAT_EDIDS[1])) else {
        return made;
    };
    if !g.loaded_cells.contains(&cell) {
        return made;
    }
    retire_first_layout(g, cell);
    let mut want = vec![(table, TABLE_POS, table_heading())];
    for (seat, base) in [(0, seat0), (1, seat1)] {
        let (p, h) = stool_spot(seat);
        want.push((base, p, h));
    }
    for ((base, pos, heading), id) in want.into_iter().zip(REFS) {
        if created_at(g, base, cell, pos).is_some() {
            continue;
        }
        let at = [0.0, 0.0, heading];
        match g.world.place_as(id, base, Some(cell), pos, at).or_else(|_| g.world.place(base, Some(cell), pos, at, 1)) {
            Ok(r) => {
                if base == table && TABLE_SCALE != 1.0 {
                    if let Ok(s) = g.world.get(r) {
                        s.scale = Some(TABLE_SCALE);
                    }
                }
                tracing::info!("armwrestle: made {} {r} at ({:.0}, {:.0}, {:.0})", g.data.name(base), pos[0], pos[1], pos[2]);
                made.push(r);
            }
            Err(e) => tracing::warn!("armwrestle: {} not made: {e}", g.data.name(base)),
        }
    }
    made
}

/// Take away the earlier layouts' pieces where a game made them: they stood where the
/// table and chairs stand now.
fn retire_first_layout(g: &mut Game, cell: FormId) {
    for (edid, pos) in FIRST_LAYOUT {
        let Some(base) = g.data.by_edid(edid) else { continue };
        if let Some(r) = created_at(g, base, cell, pos) {
            if g.world.set_deleted(r, true).is_ok() {
                tracing::info!("armwrestle: took away an earlier layout's {edid} {r}");
            }
        }
    }
}

/// The opponent, when he can come to the table now: present, alive, conscious, not in
/// combat, in the saloon within [`OPPONENT_REACH`] of the table, and not already sent to
/// a seat elsewhere.
pub fn opponent_available(g: &Game) -> Option<FormId> {
    let cell = saloon(g)?;
    let a = OPPONENT;
    if !g.present(a) || g.cell_of(a) != Some(cell) {
        return None;
    }
    if g.extra.get(&a).is_some_and(|e| e.dead || e.out_cold()) || g.awareness.state(a).in_combat() {
        return None;
    }
    let p = g.world.peek(a).map(|s| s.pos).or_else(|| g.world.placed.get(a).map(|p| p.pos))?;
    if (p[0] - TABLE_POS[0]).hypot(p[1] - TABLE_POS[1]) > OPPONENT_REACH {
        return None;
    }
    Some(a)
}

/// The opponent's Strength now (1..=10), for his tapping (`crate::armwrestle::NpcTapper`).
pub fn opponent_strength(g: &mut Game, actor: FormId) -> f32 {
    crate::functions::actor_value(g, actor, crate::actor_values::STRENGTH).unwrap_or(5.0).clamp(1.0, 10.0) as f32
}

/// Send `actor` to sit on the table's stool `seat`: its AI walks it there on the navmesh
/// and sits it down (`fnv_ai`, which reads `RefExtra::seat_override`). False when the
/// table is not made.
pub fn call_opponent(g: &mut Game, actor: FormId, seat: u8) -> bool {
    let Some(t) = find(g) else { return false };
    let stool = t.stools[usize::from(seat.min(1))];
    g.extra.entry(actor).or_default().seat_override = Some(stool);
    tracing::info!("armwrestle: {} is called to seat {seat} ({stool})", g.data.name(crate::functions::base_of(g, actor).unwrap_or(actor)));
    true
}

/// The stool `actor` was sent to, if it was.
pub fn called_to(g: &Game, actor: FormId) -> Option<FormId> {
    g.extra.get(&actor).and_then(|e| e.seat_override)
}

/// Let `actor` go: it gets up and takes up its own package again.
pub fn release_opponent(g: &mut Game, actor: FormId) {
    if let Some(e) = g.extra.get_mut(&actor) {
        if e.seat_override.take().is_some() {
            tracing::info!("armwrestle: {actor} goes back to its package");
        }
    }
}

/// Whether `actor` sits on the stool it was called to (GetSitting 3 on it).
pub fn opponent_seated(g: &Game, actor: FormId) -> bool {
    let Some(stool) = called_to(g, actor) else { return false };
    g.extra.get(&actor).and_then(|e| e.furniture).is_some_and(|f| f.reference == stool && f.state == 3)
}

/// The seat of the table a player sits on (getting on or on it), when one does: the local
/// player, or another the game made (a co-op player's body).
fn player_seat(g: &Game, t: &ArmTable) -> Option<(FormId, u8)> {
    g.extra.iter().find_map(|(who, e)| {
        let u = e.furniture?;
        let seat = t.stools.iter().position(|&s| s == u.reference)? as u8;
        // A co-op player's body is a created reference [design: armwrestle-b's].
        let player = *who == FormId::PLAYER_REF || (who.is_dynamic() && *who != OPPONENT);
        (player && matches!(u.state, 2 | 3)).then_some((*who, seat))
    })
}

/// Once a frame: a player sitting at the table with nobody on the other stool has the
/// opponent called to it, when he is about; when no player sits there any more, he is let
/// go [design].
pub fn tick(g: &mut Game) {
    // A co-op guest's NPCs follow the host's game, which calls him.
    if g.coop_guest {
        return;
    }
    let called = called_to(g, OPPONENT);
    let Some(t) = find(g) else {
        if called.is_some() {
            release_opponent(g, OPPONENT);
        }
        return;
    };
    match (player_seat(g, &t), called) {
        (Some((who, seat)), None) => {
            let other = t.stools[usize::from(1 - seat)];
            if !g.furniture_used_by_other(who, other) && opponent_available(g).is_some() {
                call_opponent(g, OPPONENT, 1 - seat);
            }
        }
        (None, Some(_)) => release_opponent(g, OPPONENT),
        // Someone else took the other stool first (another player, or the stranger
        // nvf-play seats there): he goes back.
        (Some((who, seat)), Some(_)) => {
            let other = t.stools[usize::from(1 - seat)];
            if g.extra.iter().any(|(a, e)| *a != who && *a != OPPONENT && e.furniture.is_some_and(|u| u.reference == other)) {
                release_opponent(g, OPPONENT);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_chairs_face_the_table_across_its_narrow_side() {
        let ((p0, h0), (p1, h1)) = (stool_spot(0), stool_spot(1));
        assert!((p0[0] - (TABLE_POS[0] - SEAT_OFFSET)).abs() < 1e-3 && (p0[1] - TABLE_POS[1]).abs() < 1e-3);
        assert!((p1[0] - (TABLE_POS[0] + SEAT_OFFSET)).abs() < 1e-3 && (p1[1] - TABLE_POS[1]).abs() < 1e-3);
        // Heading h faces (sin h, cos h): seat 0 faces +x, seat 1 faces −x, both to the table.
        let towards = |p: [f32; 3], h: f32| (TABLE_POS[0] - p[0]) * h.sin() + (TABLE_POS[1] - p[1]) * h.cos();
        assert!(towards(p0, h0) > 30.0);
        assert!(towards(p1, h1) > 30.0);
        assert_eq!(p0[2], SEAT_Z);
    }
}
