//! [design, #2652] Aim points for references absent from the shooter's drawn actors.

use fnv_combat::vec::V3;
use fnv_game::Game;
use fnv_sim::FormId;

/// [design, #2652] Use the current placed position, as the AI's facing does
/// (`fnv_ai::goal::place_of_ref`). Add the base's OBND centre when available; without
/// bounds (a marker), aim at the position itself [hypothesis: observer check 207].
/// [hypothesis, #2652] Non-actors in sibling worldspaces share coordinates, as in
/// `fnv_ai::goal::weapon_target_place`; unrelated-world objects have no aim point.
/// [design, #2782 review] An actor target has no aim point here: a drawn one comes from
/// the shooter's `places` (its posed chest), and one not drawn (unloaded cell, another
/// interior or worldspace) is left to the ordinary attacks, as before #2652, so
/// `acts_on` never hands the fight to a shooter whose target it can neither face nor see.
pub(crate) fn target_place(game: &Game, shooter: FormId, target: FormId) -> Option<(V3, V3)> {
    if !game.present(target) || game.extra.get(&target).is_some_and(|e| e.dead) {
        return None;
    }
    let base = fnv_game::functions::base_of(game, target);
    if base.is_some_and(|b| fnv_game::ai::is_actor_base(game.world.forms, b)) {
        return None;
    }
    let space = fnv_ai::goal::place_of_ref(game, shooter)?.space;
    let at = fnv_ai::goal::weapon_target_place(game, target, space)?;
    let feet = at.pos;
    let mut aim = feet;
    if let Some(bounds) = base.and_then(|b| game.data.object_bounds.get(&b)) {
        for i in 0..3 {
            aim[i] += (f32::from(bounds[i]) + f32::from(bounds[i + 3])) * 0.5;
        }
    }
    Some((feet, aim))
}
