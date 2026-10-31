//! How long the player's last fight has been over: the reputation box (`crate::reputation`)
//! and the level-up menu (`crate::xp_meter`) wait until it has been over for
//! [`AFTER_COMBAT_DELAY`] seconds, so the player sees the kill land and the body fall
//! first [owner request 2026-10-30, issue #1050].
//!
//! - **In a fight**: a drawn, living actor in a combat state hunts the player (its target,
//!   or an alarm it raised), as IsInCombat reads it; an actor left in an unloaded cell
//!   keeps its state, so only drawn ones count (the wait menu's test, `crate::wait`).
//!   Without the drawn list the game's own count stands (`experience::hunters`).
//! - **The delay** counts seconds of play: in the world and out of every menu. A fight
//!   that starts again before it has run out starts it again.
//! - Before any fight it is already over: nothing waits.
//! - **Log**: `after combat:` when a fight starts, when it ends and when the delay has
//!   run out.

use crate::script::ScriptState;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_sim::FormId;

/// Seconds of play after a fight before the reputation box or the level-up menu comes
/// up [design: the owner asked for "a short while"; the original's timing is not
/// documented].
pub const AFTER_COMBAT_DELAY: f32 = 2.0;

/// The player's fights as the boxes see them.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct AfterCombat {
    /// Whether the player was in a fight the last frame it was looked at.
    pub fighting: bool,
    /// Seconds of play since the last fight ended (infinite before any).
    pub quiet: f32,
}

impl Default for AfterCombat {
    fn default() -> Self {
        AfterCombat { fighting: false, quiet: f32::INFINITY }
    }
}

/// What [`AfterCombat::step`] has to say.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Change {
    Started,
    Over,
    Settled,
}

impl AfterCombat {
    /// Whether the boxes may come up: no fight now, and the last one over for the delay.
    pub fn settled(&self) -> bool {
        !self.fighting && self.quiet >= AFTER_COMBAT_DELAY
    }

    /// One frame: `fighting` now, and `seconds` of play gone by (0 in a menu).
    fn step(&mut self, fighting: bool, seconds: f32) -> Option<Change> {
        if fighting {
            let started = !self.fighting;
            self.fighting = true;
            self.quiet = 0.0;
            return started.then_some(Change::Started);
        }
        if self.fighting {
            self.fighting = false;
            self.quiet = 0.0;
            return Some(Change::Over);
        }
        let before = self.quiet;
        self.quiet += seconds.max(0.0);
        (before < AFTER_COMBAT_DELAY && self.quiet >= AFTER_COMBAT_DELAY).then_some(Change::Settled)
    }
}

/// Whether a drawn, living actor in a combat state hunts the player.
pub fn player_in_combat(g: &fnv_game::Game, living: &[(u32, [f32; 3])]) -> bool {
    let player = FormId::PLAYER_REF;
    living.iter().any(|&(a, _)| g.awareness.get(FormId(a)).is_some_and(|w| w.state.in_combat() && (w.targets.contains_key(&player) || w.alarms.iter().any(|x| x.owner == player))))
}

fn watch(time: Res<Time>, state: Res<State<PlayState>>, script: Option<Res<ScriptState>>, actors: Option<Res<crate::hitvol::HitVolumes>>, mut after: ResMut<AfterCombat>) {
    let Some(g) = script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game) else { return };
    let fighting = match actors.as_deref() {
        Some(a) => player_in_combat(g, &a.living),
        None => fnv_game::experience::hunters(g) > 0,
    };
    let playing = *state.get() == PlayState::Playing && !g.in_menu();
    match after.step(fighting, if playing { time.delta_secs() } else { 0.0 }) {
        Some(Change::Started) => info!("after combat: in a fight, boxes wait"),
        Some(Change::Over) => info!("after combat: the fight is over"),
        Some(Change::Settled) => info!("after combat: settled, boxes may come up ({AFTER_COMBAT_DELAY:.1} s of play after the fight)"),
        None => {}
    }
}

pub struct AfterCombatPlugin;

impl Plugin for AfterCombatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AfterCombat>().add_systems(Update, watch.after(PlaySet::Scripts).before(PlaySet::Ui).before(crate::reputation::watch));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_boxes_wait_out_the_delay_after_a_fight_and_start_again_with_a_new_one() {
        let mut a = AfterCombat::default();
        assert!(a.settled(), "before any fight nothing waits");
        assert_eq!(a.step(true, 0.1), Some(Change::Started));
        assert!(!a.settled());
        assert_eq!(a.step(false, 0.1), Some(Change::Over));
        assert_eq!(a.step(false, 1.0), None);
        // A menu: no time of play.
        assert_eq!(a.step(false, 0.0), None);
        assert!(!a.settled());
        // The fight starts again before the delay runs out: it starts again after.
        assert_eq!(a.step(true, 0.1), Some(Change::Started));
        a.step(false, 0.1);
        a.step(false, 1.5);
        assert!(!a.settled());
        assert_eq!(a.step(false, 0.6), Some(Change::Settled));
        assert!(a.settled());
        assert_eq!(a.step(false, 5.0), None);
    }
}
