//! The heartbeat at low health: while the player's health is low, a heartbeat goes round
//! at the listener.
//!
//! - **The sound** [observed: FalloutNV.esm SOUN `UIHealthHeartbeatALP` and
//!   `UIHealthHeartbeatBLP`, two looping heartbeats among the interface's health sounds;
//!   hypothesis: what they are for, from their names]: [`SOUND`] (A) while the health is
//!   under [`LOW`]; B is not played.
//! - **When** [hypothesis: no setting in the masters names a threshold; observer check
//!   110]: the player alive, the game played (no menu), health under [`LOW`] of the
//!   maximum. It stops at once otherwise.
//! - **The player's Heartbeat Sounds Fade tweak** [design: docs/design/tweaks.md]: the
//!   heartbeat's volume goes down to nothing over the tweak's seconds and is whole again
//!   at each loss of health.
//!
//! Log: `heartbeat: on at N% health` and `heartbeat: off`.

use crate::hud::HudView;
use crate::sfx::{LoopScale, LoopSound, Sfx};
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_sim::FormId;

/// The heartbeat [observed: the SOUN's Editor ID].
pub const SOUND: &str = "UIHealthHeartbeatALP";
/// The share of the maximum health under which it beats [hypothesis: observer check 110].
pub const LOW: f32 = 0.25;

pub struct HeartbeatPlugin;

impl Plugin for HeartbeatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, beat.in_set(PlaySet::Ui).after(crate::hud::HudSet));
    }
}

/// The heartbeat's state.
#[derive(Default)]
struct Beat {
    sound: Option<Option<FormId>>,
    on: bool,
    /// Seconds since the last loss of health (the fade tweak), and the health then.
    since_hurt: f32,
    last_health: f32,
}

/// The share of its full volume the fade tweak leaves `since` seconds after a loss of
/// health over `seconds` (0: no fade).
pub fn faded(since: f32, seconds: f32) -> f32 {
    if seconds <= 0.0 {
        1.0
    } else {
        (1.0 - since / seconds).clamp(0.0, 1.0)
    }
}

#[allow(clippy::too_many_arguments)]
fn beat(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    view: Res<HudView>,
    sfx: Option<Res<Sfx>>,
    script: Option<Res<crate::script::ScriptState>>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    mut loops: MessageWriter<LoopSound>,
    mut scale: ResMut<LoopScale>,
    mut b: Local<Beat>,
) {
    let Some(sfx) = sfx else { return };
    let Some(sound) = *b.sound.get_or_insert_with(|| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(SOUND))).map(|(f, _)| *f)) else { return };
    let dead = crate::combat::is_dead(script.as_deref(), FormId::PLAYER_REF.0);
    let want = *state.get() == PlayState::Playing && !dead && view.health > 0.0 && view.health < LOW;
    if want != b.on {
        b.on = want;
        loops.write(LoopSound { sound, on: want });
        if want {
            info!("heartbeat: on at {:.0}% health", view.health * 100.0);
            b.since_hurt = 0.0;
        } else {
            info!("heartbeat: off");
        }
    }
    // The player's tweak: the beat fades, and is whole again at each hurt.
    let fade = tweaks.as_ref().filter(|t| t.heartbeat_fade).map_or(0.0, |t| t.heartbeat_fade_seconds);
    if view.health < b.last_health - 1e-4 {
        b.since_hurt = 0.0;
    } else {
        b.since_hurt += time.delta_secs();
    }
    b.last_health = view.health;
    let share = if b.on { faded(b.since_hurt, fade) } else { 1.0 };
    let now = scale.0.get(&sound).copied().unwrap_or(1.0);
    if (now - share).abs() > 1e-3 {
        scale.0.insert(sound, share);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fade_runs_down_over_its_seconds() {
        assert_eq!(faded(0.0, 4.0), 1.0);
        assert!((faded(2.0, 4.0) - 0.5).abs() < 1e-6);
        assert_eq!(faded(9.0, 4.0), 0.0);
        assert_eq!(faded(9.0, 0.0), 1.0);
    }
}
