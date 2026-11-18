//! Bodies that sink through the terrain (#2753). Players' logs showed dropped weapons and
//! corpses in exteriors going through the ground [observed: player reports, build 0.7.0,
//! issue #2753]. The smoke probes at the reported spots found the corpses' cause: a body whose
//! centre starts under the two-sided terrain mesh is pushed down through it, and placed-dead
//! corpses started so [observed: smoke sinking-corpse-bighorner-spot and
//! sinking-corpse-radscorpion-spot, 2026-11-18]. A ragdoll ([`PhysState::lift_from_under`],
//! nvforever `ragdoll.rs` `out_of_terrain`) and a dropped item
//! ([`PhysState::lift_out_of_terrain`]) that would start so are lifted over the surface
//! first. The dropped weapons' cause is not shown yet [hypothesis: an NPC's death drop placed
//! into a slope]. Diagnostic lines: `dropped 0x… woke after resting` (debug), `dropped 0x…
//! sank:` (`PhysState::settle_created`) and `ragdoll: actor … sank:` (nvforever
//! `ragdoll.rs`). docs/spec-notes/physics.md §4.3.

use super::{trace::owner_name, PhysState};
use fnv_phys::{Filter, Key};

/// A body's centre (a dropped item's centre of mass, a ragdoll bone's position) more than
/// this far under the terrain height at its own x, y has sunk into the ground, game units
/// [design, #2753: over five times the terrain's 3.5-unit contact shell, and more than the
/// half thickness of a weapon or a bone lying on the surface].
pub const SANK_NOTE: f32 = 20.0;

/// How far above the higher of the body and the terrain the ground ray starts, and how far
/// down it looks from there, game units [design, #2753].
pub const GROUND_RAY_UP: f32 = 200.0;
pub const GROUND_RAY_DOWN: f32 = 800.0;

/// The most contacts or ray hits a line lists [design, #2753].
const LISTED: usize = 6;

/// A body that starts with its centre under the terrain surface (the two-sided terrain
/// mesh then pushes it down, and it falls through: `ragdoll: actor … sank` 0.0 to 0.2 s after
/// a placed-dead corpse's ragdoll started, [observed: smoke sinking-corpse-bighorner-spot
/// and sinking-corpse-radscorpion-spot, 2026-11-18]) is lifted so that its centre is this
/// far over what stands there, game units [design, #2753: past the terrain's 3.5-unit
/// contact shell].
pub const START_CLEARANCE: f32 = 5.0;

/// How far under a body a ray looks for anything other than the terrain to stand on (a
/// floor built below the terrain's surface): with such a support it is not under the ground
/// and is left where it is, game units [design, #2753].
pub const SUPPORT_BELOW: f32 = 100.0;

/// How far over the terrain a lifted body looks for what stands on the terrain there (a
/// rock, a foundation) to be lifted onto instead, game units [design, #2753: a terrain-layer
/// rock stood 49 units over the heightfield at the radscorpion's spot, observed: smoke
/// sinking-corpse-radscorpion-spot; low enough that most roofs over a spot are not taken].
pub const LIFT_RAY_UP: f32 = 100.0;

/// Of `points` (an index and a game-space centre), those whose centre is more than
/// `min_depth` under the terrain height `terrain` gives at its own x, y: index, centre and
/// that terrain height. Points with no terrain under them are left out.
pub fn under_terrain(points: impl IntoIterator<Item = (usize, [f32; 3])>, terrain: impl Fn(f32, f32) -> Option<f32>, min_depth: f32) -> Vec<(usize, [f32; 3], f32)> {
    points.into_iter().filter_map(|(i, p)| terrain(p[0], p[1]).filter(|&t| t - p[2] > min_depth).map(|t| (i, p, t))).collect()
}

/// How far bodies must rise so that each centre height `z` given is [`START_CLEARANCE`]
/// over its `base` (the surface over it): the most any one needs, with its index; `None`
/// when none needs to.
pub fn clearing_lift(points: impl IntoIterator<Item = (usize, f32, f32)>) -> Option<(usize, f32)> {
    points.into_iter().map(|(i, z, base)| (i, base + START_CLEARANCE - z)).filter(|&(_, up)| up > 0.0).max_by(|a, b| a.1.total_cmp(&b.1))
}

/// Of `points` (an index and a game-space position), the one deepest under the terrain
/// height `terrain` gives at its own x, y, when that depth is more than [`SANK_NOTE`]: its
/// index, its depth and the terrain height there. Points with no terrain under them are
/// left out.
pub fn deepest_under(points: impl IntoIterator<Item = (usize, [f32; 3])>, terrain: impl Fn(f32, f32) -> Option<f32>) -> Option<(usize, f32, f32)> {
    points
        .into_iter()
        .filter_map(|(i, p)| terrain(p[0], p[1]).map(|t| (i, t - p[2], t)))
        .filter(|&(_, depth, _)| depth > SANK_NOTE)
        .max_by(|a, b| a.1.total_cmp(&b.1))
}

/// What a ragdoll about to start was lifted by ([`PhysState::lift_from_under`]).
#[derive(Debug, Clone, PartialEq)]
pub struct Lift {
    /// The body that needed most.
    pub index: usize,
    /// How far its centre was under the terrain.
    pub under: f32,
    /// How far every body rises.
    pub up: f32,
    /// What that body was lifted onto.
    pub onto: String,
}

impl PhysState {
    /// Every hit of a ray straight down from `at`, `down` long (nearest first, at most a
    /// few), as `<owner> at z <z>`; "nothing" without one. Actors (characters, hit volumes,
    /// ragdolls) and the layers the character passes through are left out, and `exclude`.
    pub fn ground_hits(&mut self, at: [f32; 3], down: f32, exclude: Option<Key>) -> String {
        let filter = Filter { exclude: exclude.into_iter().collect(), no_actors: true, exclude_layers: fnv_phys::layers::character_ignores(), ..Default::default() };
        let hits = self.ray_all(at, [0.0, 0.0, -1.0], down, filter);
        if hits.is_empty() {
            return "nothing".into();
        }
        hits.iter().take(LISTED).map(|h| format!("{} at z {:.1}", owner_name(Some(h.owner)), h.point[2])).collect::<Vec<_>>().join(", ")
    }

    /// Whether anything fixed other than the terrain lies within [`SUPPORT_BELOW`] under `at`
    /// (actors, loose dynamic bodies, the layers the character passes through and `exclude`
    /// left out): a floor a body under the terrain's surface stands on.
    pub fn supported_below(&mut self, at: [f32; 3], exclude: Option<Key>) -> bool {
        let filter = Filter { exclude: exclude.into_iter().collect(), no_actors: true, no_terrain: true, no_dynamic: true, exclude_layers: fnv_phys::layers::character_ignores() };
        self.ray(at, [0.0, 0.0, -1.0], SUPPORT_BELOW, filter).is_some()
    }

    /// What stands at (x, y) on the terrain height `t`: the first collision a ray finds
    /// coming down from [`LIFT_RAY_UP`] over it (the terrain itself, or a rock or foundation
    /// standing on it), as its surface height, its contact shell and its name; the terrain
    /// when nothing is found, or when the ray starts inside a solid.
    pub fn surface_over(&mut self, x: f32, y: f32, t: f32, exclude: Option<Key>) -> (f32, f32, String) {
        let filter = Filter { exclude: exclude.into_iter().collect(), no_actors: true, exclude_layers: fnv_phys::layers::character_ignores(), ..Default::default() };
        match self.ray([x, y, t + LIFT_RAY_UP], [0.0, 0.0, -1.0], LIFT_RAY_UP + 1.0, filter) {
            // A hit at the ray's start is a solid it starts inside (a trunk, a pole taller than
            // the ray's start): not a surface to stand on.
            Some(h) if h.distance >= 0.01 && h.point[2] >= t - 1.0 => (h.point[2], h.shell, owner_name(Some(h.owner))),
            _ => (t, fnv_phys::TERRAIN_SHELL, "the terrain".into()),
        }
    }

    /// How far a ragdoll about to start in the pose `points` (an index and a body's centre)
    /// must rise: when a centre is more than `min_depth` under the terrain at its own x, y,
    /// and nothing fixed other than the terrain lies within [`SUPPORT_BELOW`] under any such
    /// body or under the lowest body when it is under the terrain (a floor built below the
    /// surface, a tunnel), every body rises
    /// until each such centre is [`START_CLEARANCE`] over what stands there
    /// ([`PhysState::surface_over`]). `None`: it starts as it is.
    pub fn lift_from_under(&mut self, points: &[(usize, [f32; 3])], min_depth: f32) -> Option<Lift> {
        let under = under_terrain(points.iter().copied(), |x, y| self.terrain_z(x, y), min_depth);
        if under.is_empty() {
            return None;
        }
        // The lowest body too, when it is under the terrain at all (a saved pose's under
        // list counts only past its depth): the feet on a floor under a sloping terrain.
        let lowest = points.iter().map(|p| p.1).min_by(|a, b| a[2].total_cmp(&b[2])).filter(|p| self.terrain_z(p[0], p[1]).is_some_and(|t| p[2] < t));
        let probes: Vec<[f32; 3]> = under.iter().map(|u| u.1).chain(lowest).collect();
        if probes.into_iter().any(|p| self.supported_below(p, None)) {
            return None;
        }
        let mut tops = Vec::with_capacity(under.len());
        for &(i, p, t) in &under {
            let (top, _, onto) = self.surface_over(p[0], p[1], t, None);
            tops.push((i, p[2], top, t - p[2], onto));
        }
        let (index, up) = clearing_lift(tops.iter().map(|t| (t.0, t.1, t.2)))?;
        let (_, _, _, under, onto) = tops.into_iter().find(|t| t.0 == index)?;
        Some(Lift { index, under, up, onto })
    }

    /// A created dynamic reference whose centre of mass starts under the terrain at its own
    /// x, y (an NPC's weapon dropped `fnv_sim::world::DROP_AHEAD` in front of it and
    /// `DROP_ABOVE` over its feet, into a slope [hypothesis]), with nothing other than the
    /// terrain within [`SUPPORT_BELOW`] under it (a floor below the surface), is lifted onto
    /// the contact shell of what stands there ([`PhysState::surface_over`]). Outdoors only
    /// (no terrain indoors). An item resting on a slope has its centre over the terrain and
    /// is left alone, however low its downhill end reaches, so a cell reload does not move it
    /// [design, #2753: a body started under the two-sided terrain mesh is pushed down through
    /// it, as the placed-dead corpses were, observed: smoke sinking-corpse-*-spot,
    /// 2026-11-18; the players' dropped weapons sank slowly at first (8-24 units/s), then
    /// fell, observed: issue #2753]. Whether it lifted.
    pub(super) fn lift_out_of_terrain(&mut self, id: u32) -> bool {
        let Some(c) = self.world.ref_center(id) else { return false };
        let Some(t) = self.terrain_z(c[0], c[1]).filter(|&t| c[2] < t) else { return false };
        let Some(low) = self.world.ref_lowest_z(id) else { return false };
        if self.supported_below(c, Some(Key::Ref(id))) {
            return false;
        }
        let (top, shell, onto) = self.surface_over(c[0], c[1], t, Some(Key::Ref(id)));
        let up = top + shell - low;
        if up <= 0.0 {
            return false;
        }
        self.world.lift_ref(id, up);
        bevy::log::info!("collision of dropped 0x{id:08X} started with its centre {:.1} under the terrain (terrain z {t:.1}): lifted {up:.1} units onto {onto}", t - c[2]);
        true
    }

    /// What a reference's colliders touch, as of the last narrow-phase update: each other
    /// owner once, with its deepest contact's distance (negative: overlapping), deepest
    /// first; "nothing" without contacts.
    pub fn contacts_text(&self, form_id: u32) -> String {
        let mut contacts = self.world.ref_contacts(form_id);
        contacts.sort_by(|a, b| a.2.total_cmp(&b.2));
        let mut seen = Vec::new();
        let mut out = Vec::new();
        for (_, owner, dist) in contacts {
            let key = owner.map(|o| o.key());
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
            out.push(format!("{} {dist:.1}", owner_name(owner)));
            if out.len() == LISTED {
                break;
            }
        }
        if out.is_empty() {
            "nothing".into()
        } else {
            out.join(", ")
        }
    }

    /// The created dynamic references found moving this frame (`settle_created`): those
    /// that `woke` after resting (seconds at rest, where), and those `awake` not yet logged
    /// as sunk, checked against the terrain under their centre of mass.
    pub(super) fn note_created_sinking(&mut self, woke: Vec<(u32, f32, [f32; 3])>, awake: Vec<(u32, [f32; 3])>) {
        for (id, rested, pos) in woke {
            let touching = self.contacts_text(id);
            bevy::log::debug!("dropped 0x{id:08X} woke after resting {rested:.2} s at ({:.1}, {:.1}, {:.2}): touching {touching}", pos[0], pos[1], pos[2]);
        }
        let now = self.world.stepped_secs();
        for (id, linvel) in awake {
            let Some(c) = self.world.ref_center(id) else { continue };
            let Some((_, depth, t)) = deepest_under([(0, c)], |x, y| self.terrain_z(x, y)) else { continue };
            let Some(cb) = self.created.get_mut(&id) else { continue };
            cb.sank = true;
            let since = (now - cb.added_at) as f32;
            let rest = cb.rested_at.map_or_else(|| "never at rest".to_string(), |r| format!("at rest from {:.2} s", (r - cb.added_at) as f32));
            let low = self.world.ref_lowest_z(id).map_or_else(|| "-".to_string(), |z| format!("{z:.1}"));
            let ground = self.ground_hits([c[0], c[1], t.max(c[2]) + GROUND_RAY_UP], GROUND_RAY_DOWN, Some(Key::Ref(id)));
            let touching = self.contacts_text(id);
            let speed = linvel.iter().map(|v| v * v).sum::<f32>().sqrt();
            bevy::log::info!(
                "dropped 0x{id:08X} sank: centre at ({:.1}, {:.1}, {:.1}), {depth:.1} under the terrain there (z {t:.1}), lowest point {low}, speed {speed:.0} units/s (vertical {:.0}), {since:.2} s after its collision was added, {rest}; ground there: {ground}; touching {touching}",
                c[0],
                c[1],
                c[2],
                linvel[2]
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A plane rising 0.5 units a unit eastwards from z 100 at x 0.
    fn slope(x: f32, _y: f32) -> Option<f32> {
        Some(100.0 + 0.5 * x)
    }

    #[test]
    fn a_point_on_or_just_under_the_surface_has_not_sunk() {
        assert_eq!(deepest_under([(0, [0.0, 0.0, 100.0]), (1, [10.0, 0.0, 105.0 - SANK_NOTE + 0.5])], slope), None);
    }

    #[test]
    fn the_terrain_under_each_point_is_its_own() {
        // 200 units uphill the terrain is at 200: a point at 150 there is 50 under it,
        // though it is above the terrain under the first point (the corpse guard's reading).
        let found = deepest_under([(0, [0.0, 0.0, 101.0]), (1, [200.0, 0.0, 150.0]), (2, [100.0, 0.0, 120.0])], slope);
        let (i, depth, t) = found.expect("the uphill point has sunk");
        assert_eq!(i, 1);
        assert!((depth - 50.0).abs() < 1e-3 && (t - 200.0).abs() < 1e-3);
        // Downhill of the first point, lower than it but on the surface: not sunk.
        assert_eq!(deepest_under([(0, [400.0, 0.0, 301.0]), (1, [0.0, 0.0, 99.0])], slope), None);
    }

    #[test]
    fn points_with_no_terrain_under_them_are_left_out() {
        assert_eq!(deepest_under([(0, [0.0, 0.0, -1000.0])], |_, _| None), None);
        assert!(under_terrain([(0, [0.0, 0.0, -1000.0])], |_, _| None, 0.0).is_empty());
    }

    #[test]
    fn a_start_over_the_surface_is_not_under_it() {
        // On the surface or over it (even a hair over), at each point's own height.
        assert!(under_terrain([(0, [0.0, 0.0, 100.0]), (1, [200.0, 0.0, 200.5]), (2, [-100.0, 0.0, 60.0])], slope, 0.0).is_empty());
        // A saved pose counts only past its depth: 3 under is not past 20.
        assert!(under_terrain([(0, [0.0, 0.0, 97.0])], slope, SANK_NOTE).is_empty());
    }

    #[test]
    fn a_start_under_the_surface_rises_until_the_neediest_centre_is_clear() {
        // 3 under at x 0 (a placed corpse a little in the ground), 360 under uphill.
        let under = under_terrain([(0, [0.0, 0.0, 97.0]), (1, [100.0, 0.0, 150.0 - 360.0]), (2, [50.0, 0.0, 200.0])], slope, 0.0);
        assert_eq!(under.iter().map(|u| u.0).collect::<Vec<_>>(), vec![0, 1]);
        // Each lifted onto the terrain under it ...
        let (i, up) = clearing_lift(under.iter().map(|&(i, p, t)| (i, p[2], t))).expect("under the surface");
        assert_eq!(i, 1);
        assert!((up - (360.0 + START_CLEARANCE)).abs() < 1e-3, "{up}");
        // ... or onto a rock 49 over the terrain at the first point: that one needs most.
        let (i, up) = clearing_lift([(0, 97.0, 149.0), (1, -210.0, 150.0)]).expect("under");
        assert_eq!(i, 1);
        assert!((up - (360.0 + START_CLEARANCE)).abs() < 1e-3, "{up}");
        let (i, up) = clearing_lift([(0, 97.0, 600.0), (1, -210.0, 150.0)]).expect("under");
        assert_eq!(i, 0);
        assert!((up - (503.0 + START_CLEARANCE)).abs() < 1e-3, "{up}");
        // Already clear: no lift.
        assert_eq!(clearing_lift([(0, 120.0, 100.0)]), None);
    }
}
