//! Combat tactics on top of the combat AI (docs/design/combat-ai.md §12): cover, flanks,
//! a clear line of fire, and the fight summary. `npc_combat` calls in here for each
//! fighter; the posts it chooses go to `fnv_game::combat_group::Tactics::posts`, which
//! `fnv_ai::alert` walks the fighter to.
//!
//! - **Cover** [documented: GECK wiki Combat Style, Cover]: every 10 seconds a fighter
//!   with a gun decides by its style's Take Cover Chance whether it uses cover for the
//!   next 10. Using it, it looks for cover within the style's Combat Search Radius: a
//!   navmesh cover edge (`fnv_nav::CoverEdge`) its target stands beyond, from whose back
//!   the collision world hides the target from its eyes, with a peek point beside the
//!   edge's end from which it sees the target [design: the original's cover search is
//!   not described]. There it hides for the style's Wait Timer, steps out to the peek
//!   point and looks for its target for the Wait To Fire Timer, fires for the Fire Timer
//!   once it sees it, and goes back behind the cover. Cover another fighter has taken is
//!   left to it. Cover whose edge no longer faces the target, or too far from it, is
//!   given up.
//! - **Flanks** [design, the owner's list 2: "have them work together in order to win"]:
//!   fighters with guns of one combat group fighting one target, not in cover, spread
//!   round it: each gets a place at the distance it fights from, turned from the side its
//!   group comes from by 0, +35°, −35°, +70° … in FormID order, so they come at it from
//!   several sides and do not stand in each other's way.
//! - **Line of fire** [design]: a fighter does not fire while a comrade (or the player,
//!   when the player is not its target) stands within [`LINE_OF_FIRE_WIDTH`] of the line
//!   to its target; it steps aside, away from them. Bystanders do not hold it back (the
//!   Fiends' brahmin stand among them).
//! - **Fight summary** ([`Battle`]): the sides (comrades together, named by a faction),
//!   hits, kills, flights, cover, flanks, steps aside, alarms and searches, and how long
//!   the fight took: `battle over` when nobody has fought for [`RESOLVED_AFTER`] seconds,
//!   `battle summary` at exit.

use bevy::prelude::*;
use fnv_game::combat_group::GroupNote;
use fnv_game::game::Game;
use fnv_nav::{CoverEdge, NavGraph, Space};
use fnv_sim::FormId;
use nvf_play::combat::ActorHit;
use nvf_play::phys::PhysState;
use nvf_play::script::ScriptState;
use std::collections::BTreeSet;

/// Take Cover Chance is rolled this often, seconds [documented: "evaluated every 10
/// seconds to determine if cover will be used for the next 10 seconds"].
pub const COVER_EVERY: f32 = 10.0;
/// Cover is looked for within this radius when the style gives none, game units [design].
pub const COVER_SEARCH_DEFAULT: f32 = 1024.0;
/// Cover is never looked for farther than this from the fighter, whatever its style's
/// radius, game units [design: FalloutNV.esm's styles had troopers run 2,000 units to
/// cover in the middle of a fight].
pub const COVER_MAX_RUN: f32 = 1024.0;
/// A fighter without cover looks for it again after this long, seconds [design].
pub const COVER_RETRY: f32 = 2.0;
/// Behind a cover edge a fighter stands this far in from it, game units [design].
pub const COVER_STANDOFF: f32 = 40.0;
/// A peek point is this far past the end of the cover edge, game units [design].
pub const PEEK_STEP: f32 = 64.0;
/// Cover nearer its target than this is not taken, game units [design: not in the
/// target's arms' reach].
pub const COVER_MIN_TARGET: f32 = 400.0;
/// Two fighters' cover or posts at least this far apart [design].
pub const TAKEN_RADIUS: f32 = 96.0;
/// Cover edges looked at per search, nearest first [design: rays are not free].
pub const COVER_CANDIDATES: usize = 16;
/// A fighter counts as at its post within this distance, game units [design].
pub const AT_POST: f32 = 64.0;
/// Flank places turn this much more for each fighter of the group, degrees [design].
pub const FLANK_STEP_DEG: f32 = 35.0;
/// Flank places are looked at again this often, seconds [design].
pub const FLANK_EVERY: f32 = 2.0;
/// A flank place nearer than this to where the fighter stands is not walked to [design].
pub const FLANK_MIN_MOVE: f32 = 192.0;
/// Someone within this distance of the line from a shooter to its target is in its line
/// of fire, game units [design: a body's width and a little].
pub const LINE_OF_FIRE_WIDTH: f32 = 40.0;
/// A step aside out of a friend's way, game units [design].
pub const SIDESTEP: f32 = 128.0;
/// A gunman whose target is nearer than this steps back from it, walking backwards while
/// it fires, game units [design: owner 2026-10-16, "backtracking in order to go
/// backwards"]. Not the weapon's Min Range, which "tells combat AI they don't need to be
/// closer than this value" [documented: GECK wiki Weapons]: the service rifle's 768 had
/// the NCR troopers backing away most of their fight.
pub const BACK_OFF_WITHIN: f32 = 256.0;
/// Once stepping back, it goes on until the target is this much farther than
/// [`BACK_OFF_WITHIN`], so it does not stop and start at the edge [design].
pub const BACK_OFF_SLACK: f32 = 64.0;
/// One step back, game units [design].
pub const BACK_OFF_STEP: f32 = 192.0;
/// A fight is over when nobody has fought for this long, seconds [design].
pub const RESOLVED_AFTER: f32 = 5.0;
/// A kind of post the AI found no way to is not given again for this long, seconds
/// [design].
pub const NO_POST_SECONDS: f32 = 3.0;

/// Where a fighter is in its use of cover.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Phase {
    /// On its way behind the cover.
    Going,
    /// Behind it, out of sight, until `until` (seconds of [`Tactic`] time).
    Hiding { until: f32 },
    /// Out at the peek point looking for its target, until `until`.
    WaitToFire { until: f32 },
    /// Out at the peek point firing, until `until`.
    Firing { until: f32 },
}

/// Cover a fighter has chosen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cover {
    pub behind: [f32; 3],
    pub peek: [f32; 3],
    pub edge: CoverEdge,
    pub phase: Phase,
}

/// One fighter's tactics.
#[derive(Debug, Clone, Default)]
pub struct Tactic {
    /// Uses cover for the current 10 seconds; seconds to the next roll; to the next
    /// search for cover.
    pub use_cover: bool,
    pub cover_roll_in: f32,
    pub cover_search_in: f32,
    pub cover: Option<Cover>,
    /// Seconds to the next look at its flank place.
    pub flank_in: f32,
    /// Who stands in its line of fire now.
    pub blocked_by: Option<u32>,
    /// Its health when it reached its flee spot.
    pub fled_health: Option<f64>,
    /// The kind of post it was given last frame; a kind the AI found no way to, barred
    /// until then; behind-cover places it could not reach.
    pub last_post: Option<fnv_game::combat_group::PostKind>,
    pub no_post: Option<(fnv_game::combat_group::PostKind, f32)>,
    pub bad_cover: Vec<[f32; 3]>,
}

/// A name for logs: the reference's name (its Editor ID or FULL), else its base form's
/// Editor ID, with the FormID.
pub fn who(g: &Game, id: FormId) -> String {
    if id == FormId::PLAYER_REF {
        return "the player".into();
    }
    let name = g.data.name(id);
    if name.starts_with("0x") {
        let base = g.world.placed.get(id).and_then(|p| g.data.edid_of.get(&p.base).cloned()).unwrap_or_default();
        format!("{base} {id}").trim().to_string()
    } else {
        format!("{name} {id}")
    }
}

/// The flank turn of the `index`-th fighter of a group, degrees: 0, +35, −35, +70, −70 …
pub fn flank_turn(index: usize) -> f32 {
    if index == 0 {
        return 0.0;
    }
    let k = index.div_ceil(2) as f32;
    if index % 2 == 1 {
        k * FLANK_STEP_DEG
    } else {
        -k * FLANK_STEP_DEG
    }
}

/// The flank place `radius` from `target`, turned `turn_deg` from the side `from` is on.
pub fn flank_point(target: [f32; 3], from: [f32; 3], radius: f32, turn_deg: f32) -> [f32; 3] {
    let base = (from[1] - target[1]).atan2(from[0] - target[0]);
    let a = base + turn_deg.to_radians();
    [target[0] + radius * a.cos(), target[1] + radius * a.sin(), target[2]]
}

/// Whether `other` stands in the line of fire from `from` to `to` (within `width` of the
/// segment, between the two ends with a body's room at each, horizontally; and within
/// 96 units of the line's height there).
pub fn in_line_of_fire(from: [f32; 3], to: [f32; 3], other: [f32; 3], width: f32) -> bool {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = d[0].hypot(d[1]);
    if len < 1.0 {
        return false;
    }
    let u = [d[0] / len, d[1] / len];
    let rel = [other[0] - from[0], other[1] - from[1]];
    let along = rel[0] * u[0] + rel[1] * u[1];
    if along <= width || along >= len - width {
        return false;
    }
    let across = (rel[0] * u[1] - rel[1] * u[0]).abs();
    let z = from[2] + (to[2] - from[2]) * along / len;
    across < width && (other[2] - z).abs() < 96.0
}

/// A step `step` across the line of fire from `from` to `to`, away from `other`.
pub fn sidestep(from: [f32; 3], to: [f32; 3], other: [f32; 3], step: f32) -> [f32; 3] {
    let d = [to[0] - from[0], to[1] - from[1]];
    let len = d[0].hypot(d[1]).max(1e-3);
    let mut n = [-d[1] / len, d[0] / len];
    if (other[0] - from[0]) * n[0] + (other[1] - from[1]) * n[1] > 0.0 {
        n = [-n[0], -n[1]];
    }
    [from[0] + n[0] * step, from[1] + n[1] * step, from[2]]
}

/// A step `step` straight back from `target`, from `from` (on top of `target`: towards
/// -y).
pub fn back_off(from: [f32; 3], target: [f32; 3], step: f32) -> [f32; 3] {
    let d = [from[0] - target[0], from[1] - target[1]];
    let len = d[0].hypot(d[1]);
    let u = if len < 1e-3 { [0.0, -1.0] } else { [d[0] / len, d[1] / len] };
    [from[0] + u[0] * step, from[1] + u[1] * step, from[2]]
}

/// The two peek points of a cover edge: past each end of it, [`COVER_STANDOFF`] in from
/// its line.
pub fn peek_points(e: &CoverEdge) -> [[f32; 3]; 2] {
    let d = [e.b[0] - e.a[0], e.b[1] - e.a[1]];
    let len = d[0].hypot(d[1]).max(1e-3);
    let u = [d[0] / len, d[1] / len];
    let half = len / 2.0 + PEEK_STEP;
    let back = e.behind(COVER_STANDOFF);
    [[back[0] + u[0] * half, back[1] + u[1] * half, back[2]], [back[0] - u[0] * half, back[1] - u[1] * half, back[2]]]
}

fn flat(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

/// Look for cover for a fighter at `feet` against a target whose chest is at `chest`
/// (module notes): `eye` its eye height, `radius` how far to look, `max_range` how far
/// from the target it may fight, `taken` places others hold.
#[allow(clippy::too_many_arguments)]
pub fn find_cover(nav: &NavGraph, mut phys: Option<&mut PhysState>, space: Space, feet: [f32; 3], chest: [f32; 3], eye: f32, radius: f32, max_range: f32, taken: &[[f32; 3]]) -> Option<Cover> {
    let sees = |phys: &mut Option<&mut PhysState>, at: [f32; 3]| crate::detection::clear_sight(phys.as_deref_mut(), [at[0], at[1], at[2] + eye], chest);
    for e in nav.cover_edges_near(space, feet, radius).into_iter().filter(|e| e.faces(chest)).take(COVER_CANDIDATES) {
        let d = flat(e.mid, chest);
        if !(COVER_MIN_TARGET..=max_range).contains(&d) {
            continue;
        }
        let Some((_, behind)) = nav.nearest(space, e.behind(COVER_STANDOFF), 96.0) else { continue };
        if taken.iter().any(|t| flat(*t, behind) < TAKEN_RADIUS) {
            continue;
        }
        // Covered: the target cannot be seen from its eyes there.
        if sees(&mut phys, behind) {
            continue;
        }
        let peek = peek_points(&e).into_iter().filter_map(|p| nav.nearest(space, p, 96.0).map(|x| x.1)).find(|p| sees(&mut phys, *p));
        if let Some(peek) = peek {
            return Some(Cover { behind, peek, edge: e, phase: Phase::Going });
        }
    }
    None
}

/// Counts for one side of the fight.
#[derive(Debug, Clone, Default)]
pub struct Side {
    pub label: String,
    pub members: BTreeSet<u32>,
    pub dead: BTreeSet<u32>,
    pub hits: usize,
    pub kills: usize,
    pub flights: usize,
}

/// The fight summary (module notes).
#[derive(Resource, Debug, Default)]
pub struct Battle {
    pub sides: Vec<Side>,
    /// Elapsed seconds of the first fighter, of the last frame anyone fought, and when the
    /// fight was over (logged once).
    pub first: Option<f32>,
    pub last: f32,
    pub over: Option<f32>,
    pub flights: [usize; 3],
    pub cornered: usize,
    pub back_to_fight: usize,
    pub boosts: usize,
    pub cover_taken: usize,
    pub peeks: usize,
    pub flanks: usize,
    pub sidesteps: usize,
    pub back_offs: usize,
    pub held_fire: usize,
    pub alarms: usize,
    pub no_alarms: usize,
    pub searches: usize,
    pub npc_grudges: usize,
    grudges: BTreeSet<(u32, u32)>,
    /// Flights counted by how they went: (actor, when the flight began).
    pub counted_flights: BTreeSet<(u32, u64)>,
}

impl Battle {
    /// The side `actor` is on, putting it on one: with its comrades, else a new side
    /// named by its first faction (or its base).
    pub fn side_of(&mut self, g: &Game, actor: u32) -> usize {
        if let Some(i) = self.sides.iter().position(|s| s.members.contains(&actor)) {
            return i;
        }
        let id = FormId(actor);
        let i = match self.sides.iter().position(|s| s.members.iter().any(|m| g.comrades(FormId(*m), id))) {
            Some(i) => i,
            None => {
                self.sides.push(Side { label: g.side_name(id), ..Default::default() });
                self.sides.len() - 1
            }
        };
        self.sides[i].members.insert(actor);
        i
    }

    /// Someone fought this frame, at `now` (elapsed seconds).
    pub fn fought(&mut self, now: f32) {
        self.first.get_or_insert(now);
        self.last = now;
        self.over = None;
    }

    fn line(&self) -> String {
        self.sides
            .iter()
            .map(|s| {
                let standing = s.members.len() - s.dead.len();
                format!("{} ({}): {standing} standing, {} dead, {} hits, {} kills, {} flights", s.label, s.members.len(), s.dead.len(), s.hits, s.kills, s.flights)
            })
            .collect::<Vec<_>>()
            .join("; ")
    }
}

pub struct CombatTacticsPlugin;

impl Plugin for CombatTacticsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Battle>().add_systems(Update, (hits, notes, resolved).after(nvf_play::PlaySet::Scripts)).add_systems(Last, report);
    }
}

/// Hits and kills by side.
fn hits(mut read: MessageReader<ActorHit>, mut battle: ResMut<Battle>, script: Option<Res<ScriptState>>) {
    let Some(rt) = script.as_deref().and_then(|s| s.rt.as_ref()) else {
        read.clear();
        return;
    };
    for h in read.read() {
        let Some(by) = h.by else { continue };
        if battle.first.is_none() {
            continue;
        }
        let a = battle.side_of(&rt.game, by.0);
        battle.sides[a].hits += 1;
        if h.died {
            battle.sides[a].kills += 1;
            let v = battle.side_of(&rt.game, h.form_id);
            battle.sides[v].dead.insert(h.form_id);
            info!("battle: {} killed by {}", who(&rt.game, FormId(h.form_id)), who(&rt.game, by));
        }
    }
}

/// The combat groups' notes (alarms, searches) and NPC-against-NPC grudges, logged.
fn notes(mut script: Option<ResMut<ScriptState>>, mut battle: ResMut<Battle>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let notes = rt.game.awareness.tactics.take_notes();
    let g = &rt.game;
    let name = |id: FormId| who(g, id);
    // Fights between actors started by a hit (`Game::attacked_by_actor`).
    for ((a, b), gr) in &g.provocations.grudges {
        if *b != FormId::PLAYER_REF && gr.why == fnv_game::provocation::Why::Assault && battle.grudges.insert((a.0, b.0)) {
            battle.npc_grudges += 1;
        }
    }
    for n in notes {
        match n {
            GroupNote::Alarm { sender, receiver, about, combat } => {
                battle.alarms += 1;
                info!("combat group: {} alarms {} about {} ({})", name(sender), name(receiver), name(about), if combat { "started fighting" } else { "noticed it" });
            }
            GroupNote::NoAlarm { sender, about, ratio, alarm_confidence } => {
                battle.no_alarms += 1;
                info!("combat group: {} sends no alarm about {}: group threat ratio {ratio:.2} at or above its alarm confidence {alarm_confidence:.2}", name(sender), name(about));
            }
            GroupNote::Searched { actor, about, count, needed } => {
                battle.searches += 1;
                info!("combat group: {} has searched its share for {} ({count} of {needed} complete searches)", name(actor), name(about));
            }
        }
    }
}

/// `battle over` once nobody has fought for [`RESOLVED_AFTER`] seconds.
fn resolved(time: Res<Time>, mut battle: ResMut<Battle>) {
    let now = time.elapsed_secs();
    if let (Some(first), None) = (battle.first, battle.over) {
        if now - battle.last >= RESOLVED_AFTER {
            battle.over = Some(battle.last);
            info!("battle over after {:.1} s (first fighter at {first:.1} s, the last fought at {:.1} s): {}", battle.last - first, battle.last, battle.line());
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, battle: Res<Battle>) {
    if exit.read().next().is_none() {
        return;
    }
    let b = &*battle;
    let Some(first) = b.first else {
        info!("battle summary: no fight");
        return;
    };
    let actors: usize = b.sides.iter().map(|s| s.members.len()).sum();
    let resolved = match b.over {
        Some(last) => format!("resolved in {:.1} s", last - first),
        None => "still going at exit".to_string(),
    };
    info!(
        "battle summary: {} sides, {actors} actors; {}; {} flights ({} by a door, {} to cover, {} away; {} cornered), {} confidence boosts, {} back to the fight; {} cover taken, {} peeks out of it; {} flank posts, {} steps aside, {} steps back, {} shots held for someone in the line of fire; {} alarms, {} not sent; {} complete searches; {} fights started by a hit between actors; first fighter at {first:.1} s, {resolved}",
        b.sides.len(),
        b.line(),
        b.flights.iter().sum::<usize>(),
        b.flights[0],
        b.flights[1],
        b.flights[2],
        b.cornered,
        b.boosts,
        b.back_to_fight,
        b.cover_taken,
        b.peeks,
        b.flanks,
        b.sidesteps,
        b.back_offs,
        b.held_fire,
        b.alarms,
        b.no_alarms,
        b.searches,
        b.npc_grudges,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn flank_places_spread_round_the_target() {
        assert_eq!([0, 1, 2, 3, 4].map(flank_turn), [0.0, 35.0, -35.0, 70.0, -70.0]);
        // The group comes from the east (+x): the first stays east, the second turns 35°.
        let t = [0.0, 0.0, 0.0];
        let p = flank_point(t, [1000.0, 0.0, 0.0], 500.0, 0.0);
        assert!((p[0] - 500.0).abs() < 1e-3 && p[1].abs() < 1e-3);
        let q = flank_point(t, [1000.0, 0.0, 0.0], 500.0, flank_turn(1));
        assert!((q[1] - 500.0 * 35f32.to_radians().sin()).abs() < 1e-2);
    }

    #[test]
    fn a_friend_between_the_shooter_and_its_target_blocks_the_shot() {
        let (from, to) = ([0.0, 0.0, 100.0], [1000.0, 0.0, 100.0]);
        assert!(in_line_of_fire(from, to, [500.0, 20.0, 90.0], LINE_OF_FIRE_WIDTH));
        assert!(!in_line_of_fire(from, to, [500.0, 60.0, 90.0], LINE_OF_FIRE_WIDTH), "beside the line");
        assert!(!in_line_of_fire(from, to, [1100.0, 0.0, 90.0], LINE_OF_FIRE_WIDTH), "beyond the target");
        assert!(!in_line_of_fire(from, to, [-50.0, 0.0, 90.0], LINE_OF_FIRE_WIDTH), "behind the shooter");
        assert!(!in_line_of_fire(from, to, [500.0, 0.0, 400.0], LINE_OF_FIRE_WIDTH), "on a roof above");
        // It steps away from the friend: the friend is north (+y), the step south.
        let s = sidestep(from, to, [500.0, 20.0, 90.0], SIDESTEP);
        assert!((s[1] + SIDESTEP).abs() < 1e-3 && s[0].abs() < 1e-3);
    }

    #[test]
    fn peek_points_lie_past_the_ends_of_the_edge() {
        let e = CoverEdge { tri: 0, a: [0.0, 0.0, 0.0], b: [100.0, 0.0, 0.0], mid: [50.0, 0.0, 0.0], out: [0.0, 1.0] };
        let [p, q] = peek_points(&e);
        assert!((p[0] - (50.0 + 50.0 + PEEK_STEP)).abs() < 1e-3 && (p[1] + COVER_STANDOFF).abs() < 1e-3);
        assert!((q[0] - (50.0 - 50.0 - PEEK_STEP)).abs() < 1e-3);
    }
}
