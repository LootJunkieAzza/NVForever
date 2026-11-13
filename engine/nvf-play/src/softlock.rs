//! The soft-lock watchdog in the game (issue #2365): `fnv_game::softlock` run each frame
//! the game runs, its findings in the log.
//!
//! - The clock is real seconds in play or in a conversation (a conversation's lines go
//!   on in real time), not the pause menu, another menu or a loading screen; one frame
//!   counts [`FRAME_CAP`] at most, so a hitch is not a hold.
//! - A menu the engine draws itself (the repair menu, barter, a container) has no screen
//!   while a conversation is open (`dialogue::sync_state` keeps the state `Dialogue`), so
//!   one set under a conversation is `menu_never_opens` (#2308).
//! - Log, a warning, once per hold: `softlock: <kind> for <s> s: <what holds the player>`.
//!   The player reports read it as a `softlock` anomaly (nvforever `reports/anomaly.rs`),
//!   and it is a `softlock` line of the script trace.
//! - Test aid `NVF_PLAY_TEST_SOFTLOCK_SECS=<seconds>`: the `controls_held` limit (30 s
//!   otherwise), for a check that cannot wait that long.

use crate::pause::Paused;
use crate::script::{is_script_menu, ScriptState};
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_game::softlock::{Limits, Outside, Watchdog};

/// Seconds one frame may add to a hold.
const FRAME_CAP: f64 = 0.25;

pub struct SoftlockPlugin;

impl Plugin for SoftlockPlugin {
    fn build(&self, app: &mut App) {
        let mut limits = Limits::default();
        if let Some(v) = std::env::var("NVF_PLAY_TEST_SOFTLOCK_SECS").ok().filter(|v| !v.trim().is_empty()) {
            match v.trim().parse::<f64>() {
                Ok(s) if s.is_finite() && s > 0.0 => {
                    limits.controls = s;
                    info!("test aid NVF_PLAY_TEST_SOFTLOCK_SECS: controls held {s} s with no story event is a soft lock");
                }
                _ => warn!("test aid NVF_PLAY_TEST_SOFTLOCK_SECS: expected seconds above 0, got {v:?}"),
            }
        }
        app.insert_resource(SoftlockWatch { dog: Watchdog::new(limits), found: 0 }).add_systems(Update, watch.in_set(PlaySet::Sync));
    }
}

#[derive(Resource)]
pub struct SoftlockWatch {
    dog: Watchdog,
    /// Findings this session.
    pub found: u32,
}

fn watch(real: Res<Time<Real>>, state: Res<State<PlayState>>, paused: Res<Paused>, mut script: ResMut<ScriptState>, mut watch: ResMut<SoftlockWatch>) {
    let state = *state.get();
    if paused.0 || !matches!(state, PlayState::Playing | PlayState::Dialogue) {
        return;
    }
    let Some(rt) = script.rt.as_ref() else { return };
    let g = &rt.game;
    let outside = Outside { menu_shown: !(state == PlayState::Dialogue && g.menu.is_some_and(|m| !is_script_menu(m))) };
    let Some(f) = watch.dog.step(g, real.delta_secs_f64().min(FRAME_CAP), outside) else { return };
    watch.found += 1;
    warn!("softlock: {} for {:.0} s: {}", f.kind.name(), f.seconds, f.text);
    script.trace_note(serde_json::json!({"kind": "softlock", "what": f.kind.name(), "seconds": f.seconds.round(), "text": f.text}));
}
