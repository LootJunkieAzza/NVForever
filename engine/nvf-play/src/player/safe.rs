//! Safe standing spots (docs/design/layer4-play.md §6, physics.md §6.7): where the player
//! goes when an interior start or a door's destination has nothing under it, or when the
//! player has fallen out of an interior's geometry; and where a test aid stands the player
//! next to a reference.
//!
//! - **Candidates in an interior**: the arrival points of the doors that lead into it —
//!   the XTEL position and z rotation of every door reference whose destination door
//!   stands in the cell [documented: XTEL door, position, rotation, xEdit; that a door
//!   teleports its activator there, into the destination door's cell, is the default
//!   activation of simulation.md §5, observed for Doc Mitchell's front door]. That these
//!   are places to stand is [hypothesis: the game puts the player there on entering by that
//!   door]. What the game does with a start over nothing, or where `coc` puts the player in
//!   an interior, is not documented here (capture proposed in physics.md §6.7).
//! - **Standing test**: the arrival rule's vertical ray (physics.md §6.2) finds a support
//!   that is walkable (its normal within the controller's `max_slope_deg`) and is not the
//!   reference the spot is for, and the capsule fits on it [engineering choice].
//! - **Order**: nearest to the requested position first [engineering choice].
//! - **Interior fall rule**: feet more than [`INTERIOR_FELL_THROUGH`] below the lowest
//!   bound of the cell's static collision [engineering choice, like the exterior
//!   `FELL_THROUGH`].

use crate::phys::PhysState;
use fnv_collide::{Iso, Shape};
use fnv_phys::{layers, CharacterParams, Filter, Owner};
use fnv_sim::{FormId, RefTable};

/// Below the lowest bound of an interior's static collision by this much the player has
/// fallen out of the cell [engineering choice: a little under the 1000 units of the
/// exterior rule, as interiors have no terrain to fall back onto; a fall of 800 units
/// takes about 1.5 s at the game's gravity].
pub const INTERIOR_FELL_THROUGH: f32 = 800.0;

/// Where a spot comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    /// The XTEL destination of door `from`, whose destination door `door` is in the cell.
    DoorArrival { from: u32, door: u32 },
}

/// A candidate standing spot.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spot {
    /// Feet, game space (before the standing test settles them on the support).
    pub feet: [f32; 3],
    /// Compass heading, radians (an XTEL z rotation, simulation.md §5).
    pub heading: f32,
    pub source: Source,
}

/// A door's teleport: the door, its destination door and the XTEL position and z rotation.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorLink {
    pub from: u32,
    pub dest_door: u32,
    pub pos: [f32; 3],
    pub rot_z: f32,
}

/// The arrival points into `cell` among `links`: those whose destination door is in it
/// (`cell_of` gives a reference's cell), in FormID order of the door they come from.
pub fn arrivals_into(links: impl IntoIterator<Item = DoorLink>, cell: u32, cell_of: impl Fn(u32) -> Option<u32>) -> Vec<Spot> {
    let mut v: Vec<Spot> = links
        .into_iter()
        .filter(|l| cell_of(l.dest_door) == Some(cell))
        .map(|l| Spot { feet: l.pos, heading: l.rot_z, source: Source::DoorArrival { from: l.from, door: l.dest_door } })
        .collect();
    v.sort_by_key(|s| match s.source {
        Source::DoorArrival { from, .. } => from,
    });
    v
}

/// The arrival points of every door leading into interior `cell` (from the plugins).
pub fn door_arrivals(refs: &RefTable, cell: u32) -> Vec<Spot> {
    let links = refs.refs.values().filter_map(|r| {
        let t = r.teleport?;
        Some(DoorLink { from: r.id.0, dest_door: t.door.0, pos: t.pos, rot_z: t.rot[2] })
    });
    arrivals_into(links, cell, |d| refs.get(FormId(d)).and_then(|r| r.cell).map(|c| c.0))
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum()
}

/// The first spot, nearest to `near` first, where `stand` finds footing; with the feet
/// `stand` settled it at.
pub fn choose(spots: &[Spot], near: [f32; 3], mut stand: impl FnMut(&Spot) -> Option<[f32; 3]>) -> Option<(Spot, [f32; 3])> {
    let mut order: Vec<&Spot> = spots.iter().collect();
    order.sort_by(|a, b| dist2(a.feet, near).total_cmp(&dist2(b.feet, near)));
    order.into_iter().find_map(|s| stand(s).map(|f| (*s, f)))
}

/// Whether feet at `feet_z` have fallen out of an interior whose static collision reaches
/// down to `lowest_z`.
pub fn fell_out(feet_z: f32, lowest_z: f32) -> bool {
    feet_z < lowest_z - INTERIOR_FELL_THROUGH
}

/// Feet positions around `target` (on its z) at the given horizontal distances, `n` per
/// ring starting north and turning clockwise, nearest ring first.
pub fn ring_spots(target: [f32; 3], radii: &[f32], n: usize) -> Vec<[f32; 3]> {
    radii
        .iter()
        .flat_map(|&r| {
            (0..n).map(move |i| {
                let a = i as f32 / n as f32 * std::f32::consts::TAU;
                [target[0] + r * a.sin(), target[1] + r * a.cos(), target[2]]
            })
        })
        .collect()
}

/// Compass heading and pitch (radians) to look from `eye` at `at`.
pub fn aim(eye: [f32; 3], at: [f32; 3]) -> (f32, f32) {
    let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let heading = d[0].atan2(d[1]).rem_euclid(std::f32::consts::TAU);
    (heading, d[2].atan2(d[0].hypot(d[1])))
}

/// What may support or block the player: everything but the layers the character passes
/// through and the player's own capsule.
fn filter(phys: &PhysState) -> Filter {
    let mut f = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
    if let Some(c) = &phys.player {
        f.exclude.push(c.key());
    }
    f
}

/// The player's controller values (the defaults before the character exists).
pub fn params(phys: &PhysState) -> CharacterParams {
    phys.player.as_ref().map_or_else(CharacterParams::default, |c| c.params)
}

/// Feet where the player could stand at `at`: a walkable support under it that is not
/// `not_on` (the arrival rule's ray: half a capsule above to four capsules below), and
/// room for the capsule there.
pub fn stand(phys: &mut PhysState, at: [f32; 3], not_on: Option<u32>) -> Option<[f32; 3]> {
    let p = params(phys);
    let f = filter(phys);
    let up = p.height / 2.0;
    let hit = {
        phys.world.update_queries();
        phys.world.queries().ray([at[0], at[1], at[2] + up], [0.0, 0.0, -1.0], up + 4.0 * p.height, &f)?
    };
    if matches!(hit.owner, Owner::Ref { form_id, .. } if Some(form_id) == not_on) {
        return None;
    }
    if hit.normal[2] < p.max_slope_deg.to_radians().cos() {
        return None;
    }
    let feet = [at[0], at[1], hit.point[2] + p.rest_gap(hit.shell)];
    // The capsule, lifted by a skin so the support itself does not count.
    let lift = 2.0 * p.skin;
    let capsule = Shape::Capsule { a: [0.0, 0.0, p.radius + lift], b: [0.0, 0.0, p.height - p.radius], radius: p.radius };
    let at = Iso { translation: feet, ..Iso::IDENTITY };
    let blocked = phys.world.queries().overlap(&capsule, &at, &f).map_or(true, |o| !o.is_empty());
    (!blocked).then_some(feet)
}

/// Heights above a reference's position the test aid aims at, in order of preference
/// [engineering choice: a reference's position is often at its foot, where the view ray
/// grazes the floor].
pub const AIM_HEIGHTS: [f32; 4] = [60.0, 30.0, 90.0, 0.0];

/// A standing spot from which `target` (its position `target_pos`) can be activated: on
/// rings around it, grounded and with room ([`stand`], not on the target itself), with a
/// point of the target within `reach` of the eye whose view ray hits the target's
/// collision first ([`AIM_HEIGHTS`] above its position); else, for references without
/// collision, the first spot with nothing between the eye and the position
/// [engineering choice]. Returns the feet and the heading and pitch that look at it.
pub fn reach_spot(phys: &mut PhysState, eye_height: f32, target: u32, target_pos: [f32; 3], reach: f32) -> Option<([f32; 3], f32, f32)> {
    let f = filter(phys);
    let mut unblocked = None;
    for at in ring_spots(target_pos, &[60.0, 80.0, 100.0], 16) {
        let Some(feet) = stand(phys, at, Some(target)) else { continue };
        let eye = [feet[0], feet[1], feet[2] + eye_height];
        for h in AIM_HEIGHTS {
            let p = [target_pos[0], target_pos[1], target_pos[2] + h];
            let d = dist2(eye, p).sqrt();
            if d > reach || d < 1.0 {
                continue;
            }
            let dir = [(p[0] - eye[0]) / d, (p[1] - eye[1]) / d, (p[2] - eye[2]) / d];
            match phys.world.queries().ray(eye, dir, reach, &f) {
                Some(hit) if matches!(hit.owner, Owner::Ref { form_id, .. } if form_id == target) => {
                    let (heading, pitch) = aim(eye, hit.point);
                    return Some((feet, heading, pitch));
                }
                Some(hit) if hit.distance < d => {}
                _ if h == 0.0 && unblocked.is_none() => {
                    let (heading, pitch) = aim(eye, p);
                    unblocked = Some((feet, heading, pitch));
                }
                _ => {}
            }
        }
    }
    unblocked
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::{FRAC_PI_2, PI};

    fn spot(from: u32, feet: [f32; 3]) -> Spot {
        Spot { feet, heading: 0.0, source: Source::DoorArrival { from, door: 0 } }
    }

    #[test]
    fn arrivals_are_the_links_whose_destination_door_is_in_the_cell() {
        let link = |from, dest_door, x| DoorLink { from, dest_door, pos: [x, 0.0, 0.0], rot_z: 1.5 };
        // Door 10 (outside) leads to door 20 (inside cell 7); door 20 leads back out; door
        // 30 (elsewhere) leads to door 40 (another cell); door 50 is one-way into cell 7.
        let links = [link(20, 10, 1.0), link(30, 40, 2.0), link(50, 21, 3.0), link(10, 20, 4.0)];
        let cell_of = |d| match d {
            20 | 21 => Some(7),
            10 => Some(1),
            40 => Some(2),
            _ => None,
        };
        let v = arrivals_into(links, 7, cell_of);
        assert_eq!(v.iter().map(|s| s.source).collect::<Vec<_>>(), vec![
            Source::DoorArrival { from: 10, door: 20 },
            Source::DoorArrival { from: 50, door: 21 }
        ]);
        assert_eq!((v[0].feet, v[0].heading), ([4.0, 0.0, 0.0], 1.5));
        assert!(arrivals_into([link(1, 2, 0.0)], 7, |_| None).is_empty());
    }

    #[test]
    fn choose_takes_the_nearest_spot_with_footing() {
        let spots = [spot(1, [1000.0, 0.0, 0.0]), spot(2, [100.0, 0.0, 0.0]), spot(3, [300.0, 0.0, 0.0])];
        // Every spot stands: the nearest wins, feet as the standing test settled them.
        let (s, f) = choose(&spots, [0.0; 3], |s| Some([s.feet[0], 0.0, 5.0])).unwrap();
        assert_eq!((s.source, f), (Source::DoorArrival { from: 2, door: 0 }, [100.0, 0.0, 5.0]));
        // The nearest has no footing: the next nearest.
        let (s, _) = choose(&spots, [0.0; 3], |s| (s.feet[0] != 100.0).then_some(s.feet)).unwrap();
        assert_eq!(s.feet[0], 300.0);
        // Nearest to where it was asked, not to the origin.
        let (s, _) = choose(&spots, [900.0, 0.0, 0.0], |s| Some(s.feet)).unwrap();
        assert_eq!(s.feet[0], 1000.0);
        assert!(choose(&spots, [0.0; 3], |_| None).is_none());
        assert!(choose(&[], [0.0; 3], |s| Some(s.feet)).is_none());
    }

    #[test]
    fn fell_out_below_the_lowest_collision_by_the_margin() {
        assert!(!fell_out(7000.0, 7500.0));
        assert!(!fell_out(7500.0 - INTERIOR_FELL_THROUGH, 7500.0));
        assert!(fell_out(7500.0 - INTERIOR_FELL_THROUGH - 1.0, 7500.0));
    }

    #[test]
    fn rings_start_north_turn_clockwise_nearest_first() {
        let v = ring_spots([10.0, 20.0, 5.0], &[50.0, 100.0], 4);
        assert_eq!(v.len(), 8);
        let close = |a: [f32; 3], b: [f32; 3]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-3);
        assert!(close(v[0], [10.0, 70.0, 5.0]), "north first");
        assert!(close(v[1], [60.0, 20.0, 5.0]), "then east");
        assert!(close(v[4], [10.0, 120.0, 5.0]), "the outer ring after the inner");
    }

    #[test]
    fn aim_looks_at_the_target() {
        let (h, p) = aim([0.0; 3], [10.0, 0.0, 0.0]);
        assert!((h - FRAC_PI_2).abs() < 1e-5 && p.abs() < 1e-5, "east, level");
        let (h, p) = aim([0.0, 0.0, 10.0], [0.0, -10.0, 0.0]);
        assert!((h - PI).abs() < 1e-5 && (p + PI / 4.0).abs() < 1e-5, "south, 45° down");
    }
}
