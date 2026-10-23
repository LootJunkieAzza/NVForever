//! Fall damage (docs/spec-notes/physics.md §6.12; the owner's playtest list 7 item 21): a
//! character that lands from higher than `fJumpFallHeightMin` loses health.
//!
//! - **Height.** From the highest point of the fall (the feet's, since the character left
//!   the ground) to where it lands [observed: the capture `sw_fall_breath` measured the
//!   height from the start feet to the landing feet, physics.md §6.6]. A character put
//!   somewhere else while in the air (a teleport, a `SetPos`) falls from where it was put
//!   [design: the capture's falls started from `SetPos`].
//! - **Damage.** `fJumpFallHeightMult × (h − fJumpFallHeightMin)^fJumpFallHeightExponent`
//!   health for a fall of `h` units above the minimum, nothing below it [observed: the
//!   capture's three land falls (695.6, 1495.4, 2995.3 units: 46.7, 1858.6, 9419.6 health)
//!   fit k·(h − 600)^1.65 with k ≈ 0.0252 within 0.8 %; FalloutNV.esm sets
//!   `fJumpFallHeightMin` 600 and `fJumpFallHeightExponent` 1.65; hypothesis: that the
//!   formula is these settings', and that the factor is the exe's `fJumpFallHeightMult`,
//!   which no plugin sets, so [`DEFAULT_MULT`] is the fitted 0.0252].
//! - **Not in water**: a fall into water does no damage [observed: same capture, every
//!   height]. A character that enters the water while falling forgets the fall.
//! - **Who.** The player, and the NPCs whose characters the physics walks
//!   (`PhysState::npcs`) by the same rule [hypothesis: FalloutNV.esm holds no NPC-specific
//!   fall setting]. The damage is a hit with no attacker and no body part, through the
//!   combat rules' hit path (god mode spares the player); armour does not reduce it
//!   [hypothesis]. No limb takes damage [hypothesis: unmeasured].
//! - Not used: `fJumpFallVelocityMin` (2785 in FalloutNV.esm); what it limits is open.

use crate::combat::{apply_hit, ActorHit, CombatStats, HitFeedback};
use crate::activate::HudMessage;
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::PlaySet;
use bevy::prelude::*;
use fnv_phys::character::CharacterStep;
use fnv_sim::FormId;
use std::collections::HashMap;

/// `fJumpFallHeightMin` when the load order lacks it [observed: 600 in FalloutNV.esm].
pub const DEFAULT_MIN: f32 = 600.0;
/// `fJumpFallHeightExponent` when the load order lacks it [observed: 1.65 in FalloutNV.esm].
pub const DEFAULT_EXPONENT: f32 = 1.65;
/// `fJumpFallHeightMult` when the load order lacks it (no plugin sets it) [observed: the
/// fit to the capture's falls, module notes].
pub const DEFAULT_MULT: f32 = 0.0252;

/// A start of step this far from where the last step ended was a placement, not a
/// fall [design].
const PLACED_JUMP: f32 = 50.0;

/// The fall damage settings.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FallRules {
    pub min: f32,
    pub exponent: f32,
    pub mult: f32,
}

impl Default for FallRules {
    fn default() -> Self {
        FallRules { min: DEFAULT_MIN, exponent: DEFAULT_EXPONENT, mult: DEFAULT_MULT }
    }
}

impl FallRules {
    /// From the load order's settings, with the defaults above.
    pub fn from_world(w: &fnv_sim::World) -> FallRules {
        let d = FallRules::default();
        FallRules {
            min: w.setting_f32("fJumpFallHeightMin").unwrap_or(d.min),
            exponent: w.setting_f32("fJumpFallHeightExponent").unwrap_or(d.exponent),
            mult: w.setting_f32("fJumpFallHeightMult").unwrap_or(d.mult),
        }
    }

    /// Health lost by a fall of `height` units onto land.
    pub fn damage(&self, height: f32) -> f32 {
        let over = height - self.min;
        if over <= 0.0 {
            return 0.0;
        }
        (self.mult * over.powf(self.exponent)).max(0.0)
    }
}

/// The falls in progress, by character.
#[derive(Resource, Default)]
pub struct Falls {
    /// By character id: the highest feet height of the fall, and where the last step ended.
    open: HashMap<u32, (f32, f32)>,
    rules: Option<FallRules>,
}

/// A character landed after falling `height` units (feet to feet).
#[derive(Message, Clone, Copy, Debug)]
pub struct Fell {
    pub actor: FormId,
    pub height: f32,
}

impl Falls {
    /// One physics step of character `id` that started with the feet at `start_z`; the
    /// fall's height when the step landed it on land.
    pub fn step(&mut self, id: u32, start_z: f32, s: &CharacterStep) -> Option<f32> {
        if s.swimming {
            self.open.remove(&id);
            return None;
        }
        let end = s.feet[2];
        if s.grounded {
            let fall = self.open.remove(&id);
            return s.landed_speed.and(fall).map(|(apex, last)| {
                let apex = if (start_z - last).abs() > PLACED_JUMP { start_z } else { apex.max(start_z) };
                (apex - end).max(0.0)
            });
        }
        let e = self.open.entry(id).or_insert((start_z, start_z));
        if (start_z - e.1).abs() > PLACED_JUMP {
            e.0 = start_z;
        }
        e.0 = e.0.max(start_z).max(end);
        e.1 = end;
        None
    }

    /// Forget a character's fall (seated, flying, put down on something).
    pub fn forget(&mut self, id: u32) {
        self.open.remove(&id);
    }
}

pub struct FallsPlugin;

impl Plugin for FallsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Falls>().add_message::<Fell>().add_systems(Update, hurt.in_set(PlaySet::Activation).run_if(resource_exists::<SimState>).run_if(resource_exists::<CombatStats>).run_if(resource_exists::<HitFeedback>));
    }
}

/// Apply the falls' damage.
#[allow(clippy::too_many_arguments)]
fn hurt(
    mut falls: ResMut<Falls>,
    mut fell: MessageReader<Fell>,
    mut sim: ResMut<SimState>,
    mut script: Option<ResMut<ScriptState>>,
    (mut stats, mut feedback): (ResMut<CombatStats>, ResMut<HitFeedback>),
    mut hud: Option<ResMut<HudMessage>>,
    mut messages: MessageWriter<ActorHit>,
) {
    for f in fell.read() {
        let rules = *falls.rules.get_or_insert_with(|| FallRules::from_world(&sim.world));
        let dmg = rules.damage(f.height);
        if dmg <= 0.0 {
            debug!("falls: {} lands from {:.0} units, no damage (from {:.0})", f.actor, f.height, rules.min);
            continue;
        }
        let info = fnv_game::combat::HitInfo { attacker: None, weapon: None, location: -1, critical: false, limb: None };
        let r = apply_hit(script.as_deref_mut(), &mut sim, f.actor, dmg, info, &mut stats, &mut feedback, hud.as_deref_mut(), &mut messages, &[], false, None);
        match r {
            Some(r) => info!(
                "falls: {} lands from {:.0} units: {dmg:.1} fall damage, {:.1} dealt, health {:.0}/{:.0}{}",
                f.actor,
                f.height,
                r.dealt,
                r.health,
                r.max_health,
                if r.immune { " (immune: god mode)" } else if r.died { ", dead" } else { "" }
            ),
            None => info!("falls: {} lands from {:.0} units: {dmg:.1} fall damage (no script runtime)", f.actor, f.height),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(z: f32, grounded: bool, landed: Option<f32>, swimming: bool) -> CharacterStep {
        CharacterStep { feet: [0.0, 0.0, z], grounded, sliding: false, landed_speed: landed, gap: 0.0, swimming, touched: Vec::new() }
    }

    #[test]
    fn damage_fits_the_capture() {
        let r = FallRules::default();
        assert_eq!(r.damage(300.0), 0.0);
        assert_eq!(r.damage(600.0), 0.0);
        // sw_fall_breath: 695.6 -> 46.7, 1495.4 -> 1858.6, 2995.3 -> 9419.6 (within 1 %).
        for (h, want) in [(695.6, 46.7), (1495.4, 1858.6), (2995.3, 9419.6)] {
            let got = r.damage(h);
            assert!((got - want).abs() / want < 0.01, "{h}: {got} vs {want}");
        }
    }

    #[test]
    fn height_is_from_the_top_of_the_fall() {
        let mut f = Falls::default();
        // A jump: up 60 from 100, then down to 0 (a ledge 100 below).
        assert_eq!(f.step(1, 100.0, &step(130.0, false, None, false)), None);
        assert_eq!(f.step(1, 130.0, &step(160.0, false, None, false)), None);
        assert_eq!(f.step(1, 160.0, &step(80.0, false, None, false)), None);
        let h = f.step(1, 80.0, &step(0.0, true, Some(500.0), false)).expect("landed");
        assert!((h - 160.0).abs() < 1e-3);
        // Walking on: no fall.
        assert_eq!(f.step(1, 0.0, &step(0.0, true, None, false)), None);
    }

    #[test]
    fn water_and_placement() {
        let mut f = Falls::default();
        f.step(2, 2000.0, &step(1990.0, false, None, false));
        // Into the water: forgotten.
        assert_eq!(f.step(2, 1990.0, &step(0.0, false, None, true)), None);
        assert_eq!(f.step(2, 0.0, &step(0.0, true, Some(10.0), false)), None);
        // Falling from 3000, put at 700 mid-air: the fall counts from 700.
        f.step(3, 3000.0, &step(2990.0, false, None, false));
        f.step(3, 700.0, &step(690.0, false, None, false));
        let h = f.step(3, 690.0, &step(0.0, true, Some(900.0), false)).expect("landed");
        assert!((h - 700.0).abs() < 1e-3);
    }
}
