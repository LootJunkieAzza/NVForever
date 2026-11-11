//! The arm-wrestling table of the Prospector Saloon (queue item 199): where it stands and
//! who comes to sit across from the player. NVForever's own addition [design: owner
//! 2026-11-10; the original game has no arm wrestling]; the bout itself is
//! `crate::armwrestle`.
//!
//! - **The furniture** ([`ensure`]): a table and two stools, references the game creates
//!   (`fnv_sim::World::place`), not ones moved from the plugins, laid out as the saloon's
//!   own two-stool table: Table02 (STAT) at scale 1.2 with a Stool01L (FURN) 84 units
//!   either side along its x axis, 12 lower, each facing it [observed: FalloutNV.esm
//!   0x00107D1A, 0x00107CED, 0x00107D82 in GSProspectorSaloonInterior]. They are made
//!   when the saloon loads and they are not there; a save keeps them like any created
//!   reference. A co-op guest makes none: the host's are copied to it.
//! - **The spot** ([`TABLE_POS`]): the east end of the back hall to the restrooms, the
//!   spot armwrestle-a chose (nvf-play `armwrestle.rs`, the same base, place, headings and
//!   scale, so whichever side runs first, the other finds its references and none is made
//!   twice): 46 units clear of every navmesh path between the saloon's doors, furniture,
//!   people and idle markers [observed by armwrestle-a: `fnvtool nav path`, 2026-11-11],
//!   and rays along the edges, diagonals and midlines of the table-and-stools footprint
//!   (230 × 84) at 10, 44 and 84 units above the floor hit nothing [observed: `fnvtool
//!   phys cell`, 2026-11-11]. The main room has no such spot: none of its 2,169
//!   footprints that fit the floor is off the navmesh. The hall is navmesh all over, so
//!   the table cuts it ([`TABLE_BOUNDS`], `fnv_nav::NavGraph::add_obstacle`): NPCs walk
//!   round it [design].
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
/// Table02, the saloon's oblong table [observed: STAT 0x00015832, OBND ±46 × ±32 × ±24].
pub const TABLE_EDID: &str = "Table02";
/// Stool01L, the saloon's stool [observed: FURN 0x000157E5].
pub const STOOL_EDID: &str = "Stool01L";

/// The table's centre (game units, the cell's frame), heading and scale [design: the
/// spot, armwrestle-a's; the height and scale those of the cell's Table02, observed].
pub const TABLE_POS: [f32; 3] = [350.0, 300.0, 3486.0];
pub const TABLE_HEADING: f32 = 0.0;
pub const TABLE_SCALE: f32 = 1.2;
/// The stools' distance from the table's centre along its x axis [observed: 84 and 82 for
/// the cell's pair].
pub const STOOL_OFFSET: f32 = 84.0;
/// The stools' height [observed: the cell's pair stand at 3474, 12 under the table].
pub const STOOL_Z: f32 = 3474.0;
/// What the table cuts out of the navmesh, in its own frame before its scale: Table02's
/// object bounds (OBND), the stools left walkable round [design].
pub const TABLE_BOUNDS: ([f32; 3], [f32; 3]) = ([-46.0, -32.0, -24.0], [46.0, 32.0, 24.0]);
/// How near its spot a created reference must be to be the table's (or a stool's): as
/// nvf-play `armwrestle.rs` finds them [design].
const SAME_SPOT: f32 = 6.0;

/// The Goodsprings Settler's reference [observed: ACHR 0x00104F08 of NPC_ GSSettlerCM,
/// placed in the saloon, Strength 5].
pub const OPPONENT: FormId = FormId(0x0010_4F08);
/// How far from the table the opponent may be to be called over (the saloon is about 1000
/// across) [design].
pub const OPPONENT_REACH: f32 = 1600.0;

/// The table and its two stools, as created.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArmTable {
    pub table: FormId,
    /// Seat 0 to the table's west (facing east), seat 1 to its east (facing west).
    pub stools: [FormId; 2],
}

/// Where stool `seat` stands and its heading (radians, the game's: 0 faces +y, π/2 +x):
/// each faces the table [observed: the cell's pair, headings 1.57 at the −x stool and 4.54
/// at the +x one].
pub fn stool_spot(seat: u8) -> ([f32; 3], f32) {
    let side = if seat == 0 { -1.0 } else { 1.0 };
    let (s, c) = TABLE_HEADING.sin_cos();
    // The table's local x axis in the cell's frame: heading 0 has local x along +x.
    let (ax, ay) = (c, -s);
    let pos = [TABLE_POS[0] + side * STOOL_OFFSET * ax, TABLE_POS[1] + side * STOOL_OFFSET * ay, STOOL_Z];
    let face = if seat == 0 { std::f32::consts::FRAC_PI_2 } else { 3.0 * std::f32::consts::FRAC_PI_2 };
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

/// The table and stools, when they are made.
pub fn find(g: &Game) -> Option<ArmTable> {
    let cell = saloon(g)?;
    let table = created_at(g, g.data.by_edid(TABLE_EDID)?, cell, TABLE_POS)?;
    let stool = g.data.by_edid(STOOL_EDID)?;
    let s0 = created_at(g, stool, cell, stool_spot(0).0)?;
    let s1 = created_at(g, stool, cell, stool_spot(1).0)?;
    Some(ArmTable { table, stools: [s0, s1] })
}

/// Which seat of the table `stool` is.
pub fn seat_of(g: &Game, stool: FormId) -> Option<u8> {
    let t = find(g)?;
    t.stools.iter().position(|&s| s == stool).map(|i| i as u8)
}

/// Make the table and stools when the saloon is loaded and they are not there (any one
/// missing is made again). Nothing on a co-op guest, or with a load order lacking the
/// cell or the bases. Returns what was made.
pub fn ensure(g: &mut Game) -> Vec<FormId> {
    let mut made = Vec::new();
    if g.coop_guest {
        return made;
    }
    let (Some(cell), Some(table), Some(stool)) = (saloon(g), g.data.by_edid(TABLE_EDID), g.data.by_edid(STOOL_EDID)) else { return made };
    if !g.loaded_cells.contains(&cell) {
        return made;
    }
    let mut want = vec![(table, TABLE_POS, TABLE_HEADING)];
    for seat in 0..2 {
        let (p, h) = stool_spot(seat);
        want.push((stool, p, h));
    }
    for (base, pos, heading) in want {
        if created_at(g, base, cell, pos).is_some() {
            continue;
        }
        match g.world.place(base, Some(cell), pos, [0.0, 0.0, heading], 1) {
            Ok(r) => {
                if base == table {
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
    fn the_stools_face_the_table_from_either_side() {
        let ((p0, h0), (p1, h1)) = (stool_spot(0), stool_spot(1));
        assert!((p0[0] - (TABLE_POS[0] - STOOL_OFFSET)).abs() < 1e-3 && (p0[1] - TABLE_POS[1]).abs() < 1e-3);
        assert!((p1[0] - (TABLE_POS[0] + STOOL_OFFSET)).abs() < 1e-3 && (p1[1] - TABLE_POS[1]).abs() < 1e-3);
        // Heading h faces (sin h, cos h): seat 0 faces +x, seat 1 faces −x, both to the table.
        let towards = |p: [f32; 3], h: f32| (TABLE_POS[0] - p[0]) * h.sin() + (TABLE_POS[1] - p[1]) * h.cos();
        assert!(towards(p0, h0) > 60.0);
        assert!(towards(p1, h1) > 60.0);
        assert_eq!(p0[2], STOOL_Z);
    }

    #[test]
    fn the_table_cut_is_the_table_only() {
        let (lo, hi) = TABLE_BOUNDS;
        assert!(TABLE_SCALE * (hi[0] - lo[0]) < 2.0 * (STOOL_OFFSET - 15.0), "the stools stay walkable");
    }
}
