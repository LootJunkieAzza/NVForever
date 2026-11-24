//! The darts opponents (`minigames.rs`): each board's own regulars, who walk up to the
//! throw line for a match, throw from there and walk off again [design: owner 2026-11-05,
//! "each board should have its own set of characters that come up to compete with you";
//! the mod's opponent is a voice on the screen with no body, docs/spec-notes/darts.md].
//!
//! - **Who plays at a board** ([`roster`]): the people of the board's own cell, the
//!   actors placed there that are about now, alive, on speaking terms with the player,
//!   not essential, selling nothing and not children, on the floor of the line; up to
//!   [`MAX_REGULARS`] of them, in an order fixed by the board and the actors' FormIDs (the
//!   same in every save and with any mod that leaves the cell's people alone). A board
//!   with fewer than two (a mod's board in an empty room) gets strangers made from the
//!   original game's generic records for the area ([`CITY`], [`WASTELAND`]): made for the
//!   match and removed after it [design].
//! - **The walk** ([`follow`]): the opponent walks from where it was to the line on its
//!   turn, to [`OPP_WAIT`] beside it on the player's, to its seat in the end view's row
//!   beside the player once the match is won ([`where_to`], the seat given by `nvforever`
//!   `match_end.rs`), and back to where it came from when the player leaves; its AI does
//!   not walk it meanwhile
//!   (`nvforever` `residents.rs` `Residents::hold_pose`, applied by `darts_world.rs`).
//!   It goes the navmesh's way ([`Opponent::route`], planned by `darts_world.rs`), round
//!   the pool tables, stools and counters between, not straight through them; one sitting
//!   down gets up first and sets off from where it stood up ([`Opponent::seated`]). Only
//!   when no way is found in [`ROUTE_WAIT`] does it walk straight [design: item 92, the
//!   Prospector Saloon's settler, seated at a table, was drawn on his chair while the
//!   game had him throwing, then jumped to the line].
//! - **Skill** ([`own_skill`], [`throw_gap`]): an opponent's Agility and Perception make
//!   half its skill, the bet the other half (the mod's opponent plays better for a
//!   bigger bet [observed: darts.md]). A skilled one throws with a steady rhythm and a
//!   tight spread, a poor one hesitates, at uneven pauses, and scatters [design].

use crate::minigames::{BoardFrame, LINE, UP};
use bevy::prelude::*;
use fnv_game::Game;
use fnv_sim::FormId;

/// Most regulars a board keeps [design].
pub const MAX_REGULARS: usize = 4;
/// Strangers are made until the board has this many [design].
pub const MIN_PLAYERS: usize = 3;
/// How fast an opponent walks, units a second [hypothesis: about the original's walk].
pub const WALK: f32 = 110.0;
/// Further than this from where it is going, an opponent runs, this fast (game units a
/// second) [design: the people's running clip moves about 350 a second].
pub const RUN_BEYOND: f32 = 400.0;
pub const RUN: f32 = 330.0;
/// From the start of the throw to the dart leaving the hand, seconds: the arm draws back
/// meanwhile (`darts_arm.rs`) [design].
pub const RELEASE: f32 = 0.42;
/// How long the walk waits for its way to be planned before it goes straight, seconds
/// (no navmesh, or none planned: a run without the engine) [design].
pub const ROUTE_WAIT: f32 = 0.5;
/// A corner of the way counts as reached this near, game units [design].
const CORNER: f32 = 6.0;
/// Where the opponent waits on the player's turn, local: beside the line on the left,
/// out of the view [design].
pub const OPP_WAIT: [f32; 3] = [-64.0, LINE[1] - 12.0, 0.0];
/// How far from the line a regular may be found outdoors (game units, across the floor;
/// indoors the whole room is the board's: the Boomers' mess hall at Nellis is 4000 across,
/// owner 2026-11-05), and how far below and above the bull its feet may be: the line's
/// floor is about 110 below the bull [observed: the Atomic Wrangler's board, bull at
/// 13705, floor 13593; design].
pub const REACH: f32 = 1400.0;
pub const BELOW: f32 = 200.0;
pub const ABOVE: f32 = 60.0;

/// Generic people of the original game for a board in the city and in the wasteland
/// (FalloutNV.esm Editor IDs; one a load order lacks is passed over) [design].
pub const CITY: &[&str] = &["VStreetSS6Gambler01", "VStreetSS6Gambler02", "VStreetSS6Gambler03", "VStreetSS6Traveler01", "VStreetSS6Traveler02"];
pub const WASTELAND: &[&str] = &["NVProspectorMaleU", "NVProspectorFemaleU", "NVProspectorMaleV", "NVProspectorFemaleV", "NVProspectorMaleW", "NVProspectorFemaleW"];
/// A cell whose Editor ID or name has one of these is in the city [design].
const CITY_WORDS: &[&str] = &["freeside", "strip", "vegas", "casino", "gomorrah", "tops", "ultraluxe", "lucky38", "wrangler", "westside", "northvegas"];

/// One who may play at a board.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    /// One of the cell's own people, walked over and given back.
    Regular(FormId),
    /// A stranger of this base, made for the match.
    Stranger(FormId),
}

/// A fixed shuffle key: the same board and form give the same place in every save.
fn mix(a: u32, b: u32) -> u64 {
    let mut z = (u64::from(a) << 32 | u64::from(b)).wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// How far actor `r` (a placed actor of `cell`) is from a board whose line is at `line`,
/// across the floor from where the data places it (the same in every save), when it can
/// come over to play: still in the cell, on the line's floor, within `reach`, and
/// [`fits`].
fn regular(g: &mut Game, r: FormId, cell: FormId, line: [f32; 3], reach: f32) -> Option<f32> {
    let p = g.world.placed.get(r).filter(|p| p.sig == *b"ACHR")?;
    let placed = p.pos;
    let (c, pos) = match g.world.peek(r) {
        Some(s) => (s.cell, s.pos),
        None => (p.cell, p.pos),
    };
    let across = (placed[0] - line[0]).hypot(placed[1] - line[1]);
    let floor = (-BELOW..ABOVE).contains(&(pos[2] - line[2]));
    (c == Some(cell) && floor && across < reach && fits(g, r)).then_some(across)
}

fn fits(g: &mut Game, r: FormId) -> bool {
    let Some(base) = g.world.placed.get(r).map(|p| p.base).or_else(|| g.world.peek(r).map(|s| s.base)) else { return false };
    g.data.sig(base) == Some(*b"NPC_")
        && g.present(r)
        && !g.is_dead(r)
        && !g.is_coop_actor(r)
        && !fnv_game::functions::essential(g, r)
        && g.services(r) == 0
        && !g.actor_race(r).is_some_and(|race| g.data.child_races.contains(&race))
        && !g.awareness.get(r).is_some_and(|a| a.state.in_combat())
        && !g.would_attack(r, FormId::PLAYER_REF)
}

/// Whether a cell is in the city ([`CITY_WORDS`]).
pub fn in_city(edid: &str, name: &str) -> bool {
    let (e, n) = (edid.to_ascii_lowercase(), name.to_ascii_lowercase().replace(' ', ""));
    CITY_WORDS.iter().any(|w| e.contains(w) || n.contains(w))
}

/// The ones who play at `board` (its cell's regulars first), in a fixed order.
pub fn roster(g: &mut Game, board: FormId, frame: &BoardFrame) -> Vec<Pick> {
    let Some(cell) = g.world.peek(board).and_then(|s| s.cell).or_else(|| g.world.placed.get(board).and_then(|p| p.cell)) else { return Vec::new() };
    let line = frame.to_world(UP);
    let here: Vec<FormId> = g.world.placed.by_cell.get(&cell).cloned().unwrap_or_default();
    // Indoors the room's people, outdoors those near; the nearest few, in a fixed order.
    let reach = if g.world.worldspace_of(board).is_none() { f32::INFINITY } else { REACH };
    let mut near: Vec<(FormId, f32)> = here.into_iter().filter_map(|r| regular(g, r, cell, line, reach).map(|d| (r, d))).collect();
    near.sort_by(|a, b| a.1.total_cmp(&b.1).then(a.0.0.cmp(&b.0.0)));
    let mut regulars: Vec<FormId> = near.into_iter().take(MAX_REGULARS).map(|(r, _)| r).collect();
    regulars.sort_by_key(|r| mix(board.0, r.0));
    let mut out: Vec<Pick> = regulars.into_iter().map(Pick::Regular).collect();
    if out.len() < 2 {
        let edid = g.data.name(cell);
        let full = g.world.forms.get(cell).and_then(|f| f.name.clone()).unwrap_or_default();
        let set = if in_city(&edid, &full) { CITY } else { WASTELAND };
        let mut strangers: Vec<FormId> = set.iter().filter_map(|e| g.world.forms.by_edid(e)).filter(|b| g.data.sig(*b) == Some(*b"NPC_")).collect();
        strangers.sort_by_key(|b| mix(board.0, b.0));
        out.extend(strangers.into_iter().take(MIN_PLAYERS.saturating_sub(out.len())).map(Pick::Stranger));
    }
    out
}

/// An opponent's own skill, 0..1, from its Agility and Perception (1..10 each)
/// [design].
pub fn own_skill(agility: f64, perception: f64) -> f32 {
    (((agility + perception) as f32 - 6.0) / 12.0).clamp(0.0, 1.0)
}

/// The skill an opponent throws with: half its own, half the bet's [design].
pub fn match_skill(own: f32, bet_skill: f32) -> f32 {
    (0.5 * own + 0.5 * bet_skill).clamp(0.0, 1.0)
}

/// The pause before an opponent's next dart: a steady one for a skilled thrower, a
/// longer, uneven one for a poor one (`roll` 0..99) [design].
pub fn throw_gap(skill: f32, roll: u32) -> f32 {
    let unsure = 1.0 - skill.clamp(0.0, 1.0);
    fnv_game::darts::AI_GAP * (0.75 + 0.6 * unsure) + unsure * 1.2 * (roll % 100) as f32 / 100.0
}

/// Where the opponent is to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Spot {
    Line,
    Waiting,
    /// Its seat in the end view's row beside the board ([`Opponent::end_spot`]).
    EndView,
    Home,
}

/// The opponent of this game's match at a board, with a body in the world.
#[derive(Debug, Clone)]
pub struct Opponent {
    pub actor: FormId,
    /// Made for the match (a stranger): removed once home.
    pub spawned: bool,
    pub name: String,
    /// Its own skill ([`own_skill`]) and the one it throws with this match.
    pub own: f32,
    pub skill: f32,
    pub board: u32,
    pub frame: BoardFrame,
    /// Where it is drawn (game space), facing, walking.
    pub feet: [f32; 3],
    pub heading: f32,
    pub moving: bool,
    /// Where it came from and faced.
    pub home: [f32; 3],
    pub home_heading: f32,
    pub to: Spot,
    /// Its throws so far (the engine plays the body's throw for each).
    pub throws: u32,
    /// A throw under way: when the dart leaves the hand, and where it lands.
    pub release: Option<(f32, (f32, f32))>,
    /// Back home after the match: the engine lets it go, and a stranger is removed.
    pub gone: bool,
    /// On furniture (sitting, getting up): the engine says so, and keeps [`Self::feet`]
    /// and [`Self::home`] where its body is; the walk waits till it is up.
    pub seated: bool,
    /// Where the walk goes now (game space, on the floor), for the engine to plan
    /// [`Self::route`] to.
    pub target: [f32; 3],
    /// The corners of the navmesh's way to [`Self::route_for`] still ahead, the last at
    /// it (the engine plans it, `darts_world.rs`); empty with `route_for` set: straight.
    pub route: Vec<[f32; 3]>,
    pub route_for: Option<[f32; 3]>,
    /// Seconds waited for a way to the target.
    pub route_wait: f32,
    /// Its seat in the end view's row beside the board (game space x, y) and the heading
    /// the row faces, set by the engine's end view (`nvforever` `match_end.rs`) once the
    /// match has a winner; it walks there and reacts in the shot beside the player
    /// [owner 2026-11-14 via parity, ship18: Sunny Smiles stood off by the wall in the end
    /// view, "why is she over there"].
    pub end_spot: Option<([f32; 2], f32)>,
}

impl Opponent {
    /// Standing at the line, facing the board, ready to throw.
    pub fn at_line(&self) -> bool {
        self.to == Spot::Line && !self.moving
    }
}

/// The player's darts screen at the opponent's board, as the walk reads it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtBoard {
    /// The opponent's turn in a match (or its throw still leaving the hand).
    pub throws: bool,
    /// The match has a winner (the result and the menus after it).
    pub over: bool,
}

/// Where the opponent goes, its spot (game space; the line's spots at the bull's height,
/// for the floor to be found under them) and the heading it faces there: the line on its
/// turn; its seat in the end view's row once the match is over and the end view has given
/// it one ([`Opponent::end_spot`]); beside the line otherwise while the player is at the
/// board; home once they have left it. A match not over clears the last end view's seat
/// (a rematch from the menu).
pub fn where_to(o: &mut Opponent, at_board: Option<AtBoard>) -> (Spot, [f32; 3], f32) {
    if at_board.is_some_and(|b| !b.over) {
        o.end_spot = None;
    }
    let f = o.frame;
    match (at_board, o.end_spot) {
        (Some(b), _) if b.throws => (Spot::Line, f.to_world(UP), f.theta),
        (Some(b), Some((xy, heading))) if b.over => (Spot::EndView, [xy[0], xy[1], o.home[2]], heading),
        (Some(_), _) => (Spot::Waiting, f.to_world(OPP_WAIT), f.theta),
        (None, _) => (Spot::Home, o.home, o.home_heading),
    }
}

/// The darts opponent with a body, while one is out (it walks home after the screen
/// closes).
#[derive(Resource, Default)]
pub struct DartsOpponent(pub Option<Opponent>);

/// Bring the next of `board`'s players over for a match: the `nth` of its roster.
pub fn bring(g: &mut Game, board: FormId, frame: BoardFrame, nth: usize, bet_skill: f32, came_from: Option<[f32; 3]>) -> Option<Opponent> {
    let roster = roster(g, board, &frame);
    if roster.is_empty() {
        return None;
    }
    let pick = roster[nth % roster.len()];
    let (actor, spawned) = match pick {
        Pick::Regular(r) => (r, false),
        Pick::Stranger(base) => {
            let cell = g.world.peek(board).and_then(|s| s.cell).or_else(|| g.world.placed.get(board).and_then(|p| p.cell));
            // Where the player came to the board from: open floor [design].
            let at = came_from.unwrap_or_else(|| frame.to_world([-90.0, LINE[1] - 80.0, 0.0]));
            match g.create_at(base, cell, at, [0.0, 0.0, frame.theta]) {
                Ok(r) => (r, true),
                Err(e) => {
                    warn!("darts: a stranger of {} not made: {e}", g.data.name(base));
                    return None;
                }
            }
        }
    };
    let s = g.world.peek(actor)?;
    let (feet, heading) = (s.pos, s.rot[2]);
    let agility = fnv_game::functions::actor_value(g, actor, fnv_game::actor_values::AGILITY).unwrap_or(5.0);
    let perception = fnv_game::functions::actor_value(g, actor, fnv_game::actor_values::PERCEPTION).unwrap_or(5.0);
    let own = own_skill(agility, perception);
    Some(Opponent {
        actor,
        spawned,
        name: String::new(),
        own,
        skill: match_skill(own, bet_skill),
        board: board.0,
        frame,
        feet,
        heading,
        moving: false,
        home: feet,
        home_heading: heading,
        to: Spot::Line,
        throws: 0,
        release: None,
        gone: false,
        seated: false,
        target: feet,
        route: Vec::new(),
        route_for: None,
        route_wait: 0.0,
        end_spot: None,
    })
}

/// A stranger made for a match is removed.
pub fn send_off(g: &mut Game, o: &Opponent) {
    if !o.spawned {
        return;
    }
    let r = o.actor;
    if g.world.set_enabled(r, false).is_ok() {
        g.emit(fnv_game::trace::EventKind::RefDisabled { reference: r, fade: true });
    }
    g.extra.entry(r).or_default().marked_for_delete = true;
    let _ = g.world.set_deleted(r, true);
}

/// One step of the walk to `target` (game space): along [`Opponent::route`] once it is
/// planned for it, straight if none comes in [`ROUTE_WAIT`]; standing while seated.
pub fn follow(o: &mut Opponent, target: [f32; 3], face: f32, dt: f32) {
    o.target = target;
    if o.seated {
        o.moving = false;
        return;
    }
    if o.route_for != Some(target) {
        o.route_wait += dt.max(0.0);
        if o.route_wait < ROUTE_WAIT {
            o.moving = false;
            return;
        }
        // No way planned in time: straight there.
        o.route.clear();
        o.route_for = Some(target);
    }
    o.route_wait = 0.0;
    while o.route.first().is_some_and(|c| (c[0] - o.feet[0]).hypot(c[1] - o.feet[1]) < CORNER) {
        o.route.remove(0);
    }
    match o.route.first().copied().filter(|_| o.route.len() > 1) {
        // A corner on the way: on through it, not stopping.
        Some(corner) => {
            step_to(o, corner, o.heading, dt);
            o.moving = true;
        }
        None => step_to(o, target, face, dt),
    }
}

/// One step of the walk to `target` (game space) at [`WALK`] (at [`RUN`] while far);
/// facing `face` there.
pub fn step_to(o: &mut Opponent, target: [f32; 3], face: f32, dt: f32) {
    let (dx, dy) = (target[0] - o.feet[0], target[1] - o.feet[1]);
    let d = dx.hypot(dy);
    // Far off it runs (its gait follows its speed), near it walks.
    let step = (if d > RUN_BEYOND { RUN } else { WALK }) * dt.max(0.0);
    if d <= step.max(1.0) {
        o.feet = target;
        o.heading = face;
        o.moving = false;
        return;
    }
    let t = step / d;
    o.feet = [o.feet[0] + dx * t, o.feet[1] + dy * t, o.feet[2] + (target[2] - o.feet[2]) * t];
    // Game headings turn clockwise from +Y: (sin h, cos h) is the way it faces.
    o.heading = dx.atan2(dy);
    o.moving = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_comes_half_from_the_actor_and_half_from_the_bet() {
        assert_eq!(own_skill(10.0, 10.0), 1.0);
        assert_eq!(own_skill(1.0, 1.0), 0.0);
        assert!((own_skill(5.0, 5.0) - 4.0 / 12.0).abs() < 1e-6);
        assert!((match_skill(1.0, 0.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_poor_thrower_pauses_longer_and_less_evenly() {
        let steady = (throw_gap(1.0, 0), throw_gap(1.0, 99));
        let shaky = (throw_gap(0.0, 0), throw_gap(0.0, 99));
        assert!((steady.0 - steady.1).abs() < 1e-6);
        assert!(shaky.0 > steady.0 && shaky.1 - shaky.0 > 1.0);
    }

    #[test]
    fn the_walk_reaches_the_spot_and_faces_the_board() {
        let f = BoardFrame::of([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], None);
        let mut o = Opponent {
            actor: FormId(1),
            spawned: false,
            name: String::new(),
            own: 0.5,
            skill: 0.5,
            board: 2,
            frame: f,
            feet: [0.0, -300.0, 0.0],
            heading: 0.0,
            moving: false,
            home: [0.0, -300.0, 0.0],
            home_heading: 0.0,
            to: Spot::Line,
            throws: 0,
            release: None,
            gone: false,
            seated: false,
            target: [0.0; 3],
            route: Vec::new(),
            route_for: None,
            route_wait: 0.0,
            end_spot: None,
        };
        let target = f.to_world(UP);
        step_to(&mut o, target, 0.5, 0.1);
        assert!(o.moving);
        // Walking towards +y and a little to -x: a heading just left of north.
        assert!(o.heading < 0.0 && o.heading > -0.3);
        for _ in 0..100 {
            step_to(&mut o, target, 0.5, 0.1);
        }
        assert!(!o.moving && o.feet == target && o.heading == 0.5);
    }

    /// The opponent's spots through a match: the line on its turn, beside it on the
    /// player's, its end-view seat once the match is won, home once the player has left;
    /// a rematch takes the seat away [owner 2026-11-14 via parity, ship18: Sunny Smiles
    /// stayed by the wall, out of the end view].
    #[test]
    fn the_opponent_takes_its_end_view_seat_once_the_match_is_over() {
        let f = BoardFrame::of([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], None);
        let mut o = Opponent {
            actor: FormId(1),
            spawned: false,
            name: String::new(),
            own: 0.5,
            skill: 0.5,
            board: 2,
            frame: f,
            feet: [0.0, -300.0, 0.0],
            heading: 0.0,
            moving: false,
            home: [10.0, -300.0, 5.0],
            home_heading: 1.0,
            to: Spot::Line,
            throws: 0,
            release: None,
            gone: false,
            seated: false,
            target: [0.0; 3],
            route: Vec::new(),
            route_for: None,
            route_wait: 0.0,
            end_spot: None,
        };
        let playing = |throws| Some(AtBoard { throws, over: false });
        let over = Some(AtBoard { throws: false, over: true });
        assert_eq!(where_to(&mut o, playing(true)), (Spot::Line, f.to_world(UP), f.theta));
        assert_eq!(where_to(&mut o, playing(false)), (Spot::Waiting, f.to_world(OPP_WAIT), f.theta));
        // Over, before the end view has given it a seat: it waits beside the line.
        assert_eq!(where_to(&mut o, over).0, Spot::Waiting);
        // The end view's seat: there, facing the row's way, on its own floor.
        o.end_spot = Some(([140.0, -5.0], 2.5));
        assert_eq!(where_to(&mut o, over), (Spot::EndView, [140.0, -5.0, 5.0], 2.5));
        // The player leaves: home.
        assert_eq!(where_to(&mut o, None), (Spot::Home, [10.0, -300.0, 5.0], 1.0));
        // A rematch: the seat goes, and it plays from the line again.
        assert_eq!(where_to(&mut o, playing(true)).0, Spot::Line);
        assert_eq!(o.end_spot, None);
        assert_eq!(where_to(&mut o, over).0, Spot::Waiting);
    }

    #[test]
    fn the_walk_follows_its_way_and_waits_while_seated() {
        let f = BoardFrame::of([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], None);
        let mut o = Opponent {
            actor: FormId(1),
            spawned: false,
            name: String::new(),
            own: 0.5,
            skill: 0.5,
            board: 2,
            frame: f,
            feet: [0.0, -300.0, 0.0],
            heading: 0.0,
            moving: false,
            home: [0.0, -300.0, 0.0],
            home_heading: 0.0,
            to: Spot::Line,
            throws: 0,
            release: None,
            gone: false,
            seated: true,
            target: [0.0; 3],
            route: Vec::new(),
            route_for: None,
            route_wait: 0.0,
            end_spot: None,
        };
        let target = [0.0, -72.0, 0.0];
        // Seated: it stays put.
        follow(&mut o, target, 0.0, 0.1);
        assert!(!o.moving && o.feet == [0.0, -300.0, 0.0] && o.target == target);
        // Up, with a way round something: through the corner, then to the spot.
        o.seated = false;
        o.route = vec![[100.0, -200.0, 0.0], target];
        o.route_for = Some(target);
        let mut passed = false;
        for _ in 0..200 {
            follow(&mut o, target, 0.5, 0.05);
            passed |= o.feet[0] > 90.0;
        }
        assert!(passed, "it went by the corner");
        assert!(!o.moving && o.feet == target && o.heading == 0.5);
        // No way planned: it waits a little, then goes straight.
        let back = [0.0, -300.0, 0.0];
        follow(&mut o, back, 0.0, 0.1);
        assert!(!o.moving && o.feet == target);
        for _ in 0..100 {
            follow(&mut o, back, 0.0, 0.1);
        }
        assert!(o.feet == back);
    }

    #[test]
    fn the_strip_and_freeside_are_the_city() {
        assert!(in_city("FreesideAtomicWrangler", "Atomic Wrangler"));
        assert!(in_city("x", "The Tops"));
        assert!(!in_city("GSProspectorSaloonInterior", "Prospector Saloon"));
        assert!(!in_city("QJOffice", "Mining Office"));
    }

    #[test]
    fn the_shuffle_is_fixed_by_board_and_form() {
        assert_eq!(mix(5, 7), mix(5, 7));
        assert_ne!(mix(5, 7), mix(6, 7));
    }
}
