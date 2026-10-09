//! One trigger pull to projectiles: the chain's projectile count, each with its own
//! direction inside the spread cone (docs/design/combat.md §5.1).

use crate::data::{CombatForms, ShotChain};
use crate::projectile::{ActorId, Projectile};
use crate::spread::spread_dir;
use crate::tuning::CombatTuning;
use crate::vec::V3;

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
