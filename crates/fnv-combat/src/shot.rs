//! One trigger pull to projectiles: the chain's projectile count, each with its own
//! direction inside the spread cone (docs/design/combat.md §5.1).

use crate::data::{CombatForms, ShotChain};
use crate::projectile::{ActorId, CombatWorld, Projectile};
use crate::spread::spread_dir;
use crate::tuning::CombatTuning;
use crate::vec::{self, V3};

/// How far short of the aim point the muzzle's ray may stop and still count as reaching
/// it, units [design]: the two rays meet the same surface at different angles.
pub const COVER_SLACK: f32 = 4.0;
/// How far from the muzzle something may be and still count as cover in front of the
/// barrel, units [design]. Beyond it the view decides: aimed along the ground, the
/// lower ray from the muzzle would otherwise stop on every rise far short of the point
/// aimed at [observed: a burst along level ground at Goodsprings, the muzzle's ray
/// stopped on the ground short of an aim point 1,328 units away].
pub const COVER_REACH: f32 = 150.0;

/// Where a projectile is drawn: beside the path it flies, by the muzzle's offset from
/// the eye at first and on the path once it has gone as far as the aim point
/// (docs/design/combat.md §5.1). The hit is decided on the path; this is for the eye.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seen {
    /// The muzzle relative to where the projectile starts.
    pub offset: V3,
    /// The distance over which the offset closes.
    pub converge: f32,
}

impl Seen {
    /// Drawn where it flies.
    pub const ON_PATH: Seen = Seen { offset: [0.0; 3], converge: 1.0 };

    /// Where a projectile at `pos` that has flown `travelled` units is drawn.
    pub fn at(&self, pos: V3, travelled: f32) -> V3 {
        let left = 1.0 - (travelled / self.converge.max(1e-3)).clamp(0.0, 1.0);
        vec::add(pos, vec::scale(self.offset, left))
    }
}

/// How one trigger pull leaves the shooter ([`leaving`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Leaving {
    /// Where the projectiles start and the direction their cone is around.
    pub origin: V3,
    pub aim: V3,
    /// What the view ray from the eye meets (its end when it meets nothing), and how
    /// far from the eye that is.
    pub aim_point: V3,
    pub aim_distance: f32,
    /// How the projectiles are drawn.
    pub seen: Seen,
    /// Something stands between the muzzle and the aim point: the projectiles leave the
    /// muzzle towards the aim point instead of the eye along the view.
    pub covered: bool,
}

/// How a shot leaves (docs/design/combat.md §5.1) [design].
///
/// - The aim is the view ray from `eye` along `aim`, `reach` units long: what it meets
///   first is the aim point.
/// - With a `muzzle` the projectiles are drawn from it, closing on the view ray by the
///   aim point ([`Seen`]); the muzzle is not used when the aim point is not in front of
///   it (a barrel pushed through a wall).
/// - With `cover` (third person) a second ray goes from the muzzle towards the aim
///   point, [`COVER_REACH`] units at most. If it stops on something other than the
///   actor aimed at, more than [`COVER_SLACK`] short of the aim point, the projectiles
///   leave the muzzle towards the aim point, so they strike what is right in front of
///   the barrel instead of passing it.
pub fn leaving(world: &dyn CombatWorld, eye: V3, aim: V3, muzzle: Option<V3>, reach: f32, shooter: Option<ActorId>, cover: bool) -> Leaving {
    let aim = vec::normalize(aim);
    let ignore: Vec<ActorId> = shooter.into_iter().collect();
    let end = vec::add(eye, vec::scale(aim, reach));
    let hit = world.ray(eye, end, &ignore);
    let (aim_point, aim_distance) = hit.as_ref().map_or((end, reach), |h| (h.point, h.distance));
    let from_eye = Leaving { origin: eye, aim, aim_point, aim_distance, seen: Seen::ON_PATH, covered: false };
    let Some(muzzle) = muzzle else { return from_eye };
    let to = vec::sub(aim_point, muzzle);
    if vec::dot(to, aim) <= COVER_SLACK {
        return from_eye;
    }
    if cover {
        let len = vec::len(to);
        let near = vec::add(muzzle, vec::scale(to, (COVER_REACH / len).min(1.0)));
        let blocked = world.ray(muzzle, near, &ignore).is_some_and(|h| {
            let same_actor = h.actor.is_some() && h.actor == hit.as_ref().and_then(|a| a.actor);
            !same_actor && h.distance + COVER_SLACK < len
        });
        if blocked {
            return Leaving { origin: muzzle, aim: vec::normalize(to), covered: true, ..from_eye };
        }
    }
    Leaving { seen: Seen { offset: vec::sub(muzzle, eye), converge: aim_distance }, ..from_eye }
}

/// The projectiles of one shot. `cone_deg` comes from [`crate::spread::cone_deg`];
/// `rand` returns uniform numbers in [0, 1) (two per projectile). A chain without a
/// projectile (melee, binoculars) gives none.
#[allow(clippy::too_many_arguments)]
pub fn fire(
    forms: &CombatForms,
    chain: &ShotChain,
    origin: V3,
    aim: V3,
    cone_deg: f32,
    shooter: Option<ActorId>,
    world_gravity: f32,
    t: &CombatTuning,
    rand: &mut dyn FnMut() -> f32,
) -> Vec<Projectile> {
    let Some(p) = chain.projectile.and_then(|p| forms.projectiles.get(&p)) else { return Vec::new() };
    (0..chain.projectiles_per_shot.max(1))
        .map(|_| {
            let dir = spread_dir(aim, cone_deg, rand(), rand());
            Projectile::spawn(p, chain.explosion, origin, dir, shooter, world_gravity, t)
        })
        .collect()
}
