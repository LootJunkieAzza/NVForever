//! Whose health the HUD's enemy health bar shows (docs/spec-notes/hud.md §8; the bar is
//! drawn by `crate::hud`).
//!
//! - **Under the crosshair**: a ray along the crosshair's line, through the collision
//!   world up to [`AIM_RANGE`], to the first thing it meets; an actor's hit volume or
//!   capsule (`Owner::Actor`) makes that actor the one aimed at.
//! - **Shown** for a living actor aimed at that is hostile to the player
//!   (`Game::would_attack`, the detection states' rule) or in combat with the player, or
//!   that the player hit within [`HIT_SECONDS`]; and for [`HIT_SECONDS`] after the player
//!   hits an actor, aimed at or not [hypothesis: the original's rule is not documented;
//!   the owner's screenshot shows it over a radscorpion in a fight].
//! - The health is the actor's Health over its full value; the name is the one the
//!   crosshair's prompt gives it (`SimState::ref_name`).

use crate::combat::ActorHit;
use crate::phys::PhysState;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_phys::{layers, Filter, Owner};
use fnv_sim::FormId;
use std::collections::HashMap;

/// How far the crosshair's ray looks for an actor [hypothesis: no source gives the
/// original's range; a long rifle shot].
pub const AIM_RANGE: f32 = 8192.0;
/// Seconds an actor the player hit counts as the target [hypothesis].
pub const HIT_SECONDS: f32 = 5.0;

/// The actor whose health the bar shows: reference, name, Health as 0 to 1 of its full
/// value, and whether it is hostile (drawn in the HUD's alternative colour).
#[derive(Resource, Debug, Clone, PartialEq, Default)]
pub struct HudTarget(pub Option<TargetHealth>);

/// The actor under the crosshair (the same ray, [`AIM_RANGE`]) is alive and hostile to
/// the player, by the same rule: the crosshair turns red (`crate::reticle`).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct AimHostile(pub bool);

#[derive(Debug, Clone, PartialEq)]
pub struct TargetHealth {
    pub form_id: u32,
    pub name: String,
    pub health: f32,
}

/// Whether an actor gets the bar: aimed at and hostile, aimed at and hit lately, or hit
/// just now (`since_hit`: seconds since the player last hit it).
pub fn shown(aimed: bool, hostile: bool, since_hit: Option<f32>) -> bool {
    let hit = since_hit.is_some_and(|s| s < HIT_SECONDS);
    (aimed && hostile) || hit
}

pub struct HudTargetPlugin;

impl Plugin for HudTargetPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<HudTarget>().init_resource::<AimHostile>().add_systems(Update, aim.in_set(PlaySet::Ui).before(crate::hud::HudSet));
    }
}

#[allow(clippy::too_many_arguments)]
fn aim(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    player: Option<Res<Player>>,
    control: Option<Res<crate::control::PlayerControl>>,
    phys: Option<ResMut<PhysState>>,
    mut script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    mut hits: MessageReader<ActorHit>,
    mut hit_ago: Local<HashMap<u32, f32>>,
    mut target: ResMut<HudTarget>,
    mut aim_hostile: ResMut<AimHostile>,
) {
    let dt = time.delta_secs();
    for t in hit_ago.values_mut() {
        *t += dt;
    }
    hit_ago.retain(|_, t| *t < HIT_SECONDS);
    let mut latest = None;
    for h in hits.read() {
        if h.by == Some(FormId::PLAYER_REF) {
            hit_ago.insert(h.form_id, 0.0);
            latest = Some(h.form_id);
        }
    }
    if *state.get() != PlayState::Playing {
        aim_hostile.set_if_neq(AimHostile(false));
        return;
    }
    let Some(player) = player else { return };
    let crosshair = control.as_deref().and_then(|c| c.crosshair);
    let (eye, dir) = crate::control::Crosshair::ray(crosshair.as_ref(), player.eye(), player.look_dir());
    let aimed = phys.and_then(|mut p| {
        let mut filter = p.player.as_ref().map_or_else(Filter::default, |c| Filter::excluding(c.key()));
        filter.exclude_layers.insert(layers::TRIGGER);
        match p.world.queries().ray(eye, dir, AIM_RANGE, &filter)?.owner {
            Owner::Actor { form_id, .. } => Some(form_id),
            _ => None,
        }
    });
    // The one aimed at first; else the one the player hit last, while that counts.
    let mut candidates: Vec<(u32, bool)> = aimed.map(|a| (a, true)).into_iter().collect();
    if let Some(h) = latest.or_else(|| hit_ago.iter().min_by(|a, b| a.1.total_cmp(b.1)).map(|(id, _)| *id)) {
        candidates.push((h, false));
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let (found, foe) = with_world(rt, &mut sim, |rt| {
        let foe = aimed.is_some_and(|a| {
            let (r, g) = (FormId(a), &mut rt.game);
            let fighting = g.awareness.get(r).and_then(|x| x.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
            !g.is_dead(r) && (fighting || g.would_attack(r, FormId::PLAYER_REF))
        });
        let found = candidates.into_iter().find_map(|(id, is_aimed)| {
            let r = FormId(id);
            let g = &mut rt.game;
            if g.is_dead(r) {
                return None;
            }
            let fighting = g.awareness.get(r).and_then(|a| a.combat_target()).is_some_and(|(t, _)| t == FormId::PLAYER_REF);
            let hostile = fighting || g.would_attack(r, FormId::PLAYER_REF);
            if !shown(is_aimed, hostile, hit_ago.get(&id).copied()) {
                return None;
            }
            let max = g.max_health(r);
            let health = fnv_game::functions::actor_health(g, r).unwrap_or(max);
            Some((id, if max > 0.0 { (health / max).clamp(0.0, 1.0) as f32 } else { 0.0 }))
        });
        (found, foe)
    });
    aim_hostile.set_if_neq(AimHostile(foe));
    let now = found.map(|(form_id, health)| {
        let name = sim.ref_name(script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game), form_id).unwrap_or_default().to_string();
        TargetHealth { form_id, name, health }
    });
    let key = |t: &Option<TargetHealth>| t.as_ref().map(|t| (t.form_id, (t.health * 100.0).round() as i32));
    if key(&target.0) != key(&now) {
        match &now {
            Some(t) => info!("hud: enemy health {} 0x{:08X} {:.0} %", t.name, t.form_id, t.health * 100.0),
            None => info!("hud: enemy health off"),
        }
    }
    target.set_if_neq(HudTarget(now));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_bar_is_for_foes_aimed_at_and_actors_just_hit() {
        assert!(shown(true, true, None));
        // A friend aimed at: no bar, until the player hits it.
        assert!(!shown(true, false, None));
        assert!(shown(true, false, Some(1.0)));
        // Hit lately: shown without aiming, until the time is up.
        assert!(shown(false, false, Some(0.0)));
        assert!(!shown(false, true, Some(HIT_SECONDS)));
        assert!(!shown(false, true, None));
    }
}
