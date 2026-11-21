//! Resolve only the supported session launch fields, without mutating CombatForms.
use fnv_combat::{CombatForms, ShotChain, data::ProjectileStats};

pub(super) struct Launch {
    pub chain: ShotChain,
    pub projectile: Option<ProjectileStats>,
}

pub(super) fn resolve(forms: &CombatForms, chain: &ShotChain, game: Option<&fnv_game::Game<'_>>) -> Launch {
    let mut chain = chain.clone();
    if let Some(game) = game {
        let weapon_default = forms.weapons.get(&chain.weapon).and_then(|w| w.projectile);
        let ammo_default = chain.ammo.and_then(|a| forms.ammo.get(&a)).and_then(|a| a.projectile);
        (chain.projectile, chain.projectile_from) = game.projectile_for_launch(chain.weapon, chain.ammo, weapon_default, ammo_default);
    }
    let projectile = chain.projectile.and_then(|requested| {
        let traits = game.and_then(|g| g.projectile_launch_traits(requested));
        let original = traits.map_or(requested, |t| t.original);
        let mut p = forms.projectiles.get(&original)?.clone();
        if let Some(t) = traits {
            if let Some(speed) = t.speed { p.speed = speed; }
            if let Some(force) = t.impact_force { p.impact_force = force; }
        }
        Some(p)
    });
    // A missing typed record used to yield no physical projectiles without skipping
    // the caller's normal effects/counters. Preserve that control-flow boundary.
    chain.explosion = projectile.as_ref().and_then(ProjectileStats::explodes);
    Launch { chain, projectile }
}
