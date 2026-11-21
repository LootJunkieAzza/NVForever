//! The heartbeat at low health: while the player's health is low, a heartbeat goes round
//! at the listener; and Sprint's heartbeat (`crate::sprint`) while sprinting and while the AP
//! comes back after it.
//!
//! - **The sound** [observed: FalloutNV.esm SOUN `UIHealthHeartbeatALP` and
//!   `UIHealthHeartbeatBLP`, two looping heartbeats among the interface's health sounds;
//!   hypothesis: what they are for, from their names]: [`SOUND`] (A) while the health is
//!   under [`LOW`]. Their files, 11025 Hz loops: A has four beats in 2.86 s (about 84 a
//!   minute), B four in 1.93 s (about 124) [observed: the beats' peaks in the extracted
//!   `ui_health_heartbeat_a_lp.wav` and `_b_lp.wav`, 2026-11-18].
//! - **When** [hypothesis: no setting in the masters names a threshold; observer check
//!   110]: the player alive, the game played (no menu), health under [`LOW`] of the
//!   maximum. It stops at once otherwise.
//! - **The player's Heartbeat Sounds Fade tweak** [design: docs/design/tweaks.md]: the
//!   heartbeat's volume goes down to nothing over the tweak's seconds and is whole again
//!   at each loss of health.
//! - **Sprint's heartbeat** [design, #3000; the request: "slower heartbeat from 40-20%, the
//!   faster one from the last 20%"; `crate::sprint::Sprint::heart`]: A (the slower) or
//!   [`FAST_SOUND`] (B, the faster), at [`SPRINT_VOLUME`] of their volume, quiet under the
//!   game. Each comes and goes on a critically damped spring settling in
//!   [`SPRINT_FADE_SECONDS`], so a change of pace cross-fades and none starts or stops with a
//!   click ([`Hearts`]). The low-health heartbeat wins: while it beats, B stops and A is the
//!   health's (taken over from a sprint's A on the same spring); never two heartbeats at once.
//!
//! Log: `heartbeat: on at N% health`, `heartbeat: off`, `sprint: heartbeat slow|fast|off`.

use crate::ease::{omega_for, Spring};
use crate::hud::HudView;
use crate::sfx::{LoopScale, LoopSound, Sfx};
use crate::sprint::HeartPace;
use crate::{PlaySet, PlayState};
use bevy::prelude::*;
use fnv_sim::FormId;

/// The heartbeat [observed: the SOUN's Editor ID]; Sprint's slower one too.
pub const SOUND: &str = "UIHealthHeartbeatALP";
/// The faster heartbeat, Sprint's under 20 % AP [observed: the SOUN's Editor ID].
pub const FAST_SOUND: &str = "UIHealthHeartbeatBLP";
/// The share of the maximum health under which it beats [hypothesis: observer check 192].
pub const LOW: f32 = 0.25;
/// Sprint's heartbeat at this share of the loop's own volume: under the game's sounds
/// [design, #3000; the request: "quiet"].
pub const SPRINT_VOLUME: f32 = 0.45;
/// Sprint's heartbeat comes, goes and cross-fades through a spring settling (95%) in this
/// many seconds [design].
pub const SPRINT_FADE_SECONDS: f32 = 1.5;
/// Under this share of its volume a fading loop is stopped.
const SILENT: f32 = 1e-3;

pub struct HeartbeatPlugin;

impl Plugin for HeartbeatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, beat.in_set(PlaySet::Ui).after(crate::hud::HudSet));
    }
}

/// Which loop should sound at what share of its volume: (A, B). The low-health heartbeat
/// (`health`, at `health_share` of its volume) wins over Sprint's `pace`.
pub fn targets(health: bool, health_share: f32, pace: HeartPace) -> (f32, f32) {
    match (health, pace) {
        (true, _) => (health_share, 0.0),
        (false, HeartPace::Slow) => (SPRINT_VOLUME, 0.0),
        (false, HeartPace::Fast) => (0.0, SPRINT_VOLUME),
        (false, HeartPace::Off) => (0.0, 0.0),
    }
}

/// What [`Hearts::step`] asks of the loops this frame.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct HeartOut {
    /// Start (`Some(true)`) or stop (`Some(false)`) loop A or B.
    pub a: Option<bool>,
    pub b: Option<bool>,
    /// Their shares of their volume now.
    pub a_share: f32,
    pub b_share: f32,
}

/// The two loops' volumes across frames: the low-health heartbeat as before (on and off at
/// once), Sprint's on springs.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Hearts {
    a: Spring,
    b: Spring,
    a_on: bool,
    b_on: bool,
    /// A beats for the health; and whether it is still on its way from a sprint's volume.
    health: bool,
    blending: bool,
}

impl Hearts {
    /// One frame: `playing` (the game played, the player alive), the low-health heartbeat
    /// wanted (`health`, its share), Sprint's `pace`.
    pub fn step(&mut self, playing: bool, health: bool, health_share: f32, pace: HeartPace, dt: f32) -> HeartOut {
        let mut out = HeartOut::default();
        let (ta, tb) = targets(health, health_share, pace);
        let w = omega_for(SPRINT_FADE_SECONDS);
        if !playing {
            // A menu or death: both stop at once, as the health's always did.
            out.a = self.a_on.then_some(false);
            out.b = self.b_on.then_some(false);
            *self = Hearts::default();
            return out;
        }
        if health {
            // Never two at once: B goes (the mixer's own short stop), A is the health's.
            if self.b_on {
                out.b = Some(false);
                (self.b_on, self.b) = (false, Spring::REST);
            }
            if !self.health {
                // From a sprint's A: up on its spring; from silence: at once, as before.
                self.blending = self.a_on;
                self.health = true;
            }
            if !self.a_on {
                out.a = Some(true);
                self.a_on = true;
            }
            if self.blending {
                self.a.step(ta, w, dt);
                self.blending = (self.a.value - ta).abs() > 0.01;
            } else {
                self.a = Spring { value: ta, rate: 0.0 };
            }
        } else {
            if self.health {
                self.health = false;
                self.blending = false;
                if ta <= 0.0 {
                    // The health's heartbeat over and no sprint's: it stops at once, as before.
                    out.a = Some(false);
                    (self.a_on, self.a) = (false, Spring::REST);
                }
            }
            for (on, s, t, o) in [(&mut self.a_on, &mut self.a, ta, &mut out.a), (&mut self.b_on, &mut self.b, tb, &mut out.b)] {
                if t > 0.0 && !*on {
                    *on = true;
                    *o = Some(true);
                    *s = Spring::REST;
                }
                if *on {
                    s.step(t, w, dt);
                    if t <= 0.0 && s.value < SILENT && s.rate.abs() < 0.01 {
                        *on = false;
                        *o = Some(false);
                        *s = Spring::REST;
                    }
                }
            }
        }
        out.a_share = self.a.value.clamp(0.0, 1.0);
        out.b_share = self.b.value.clamp(0.0, 1.0);
        out
    }

    /// Whether the loops play now (A, B).
    pub fn playing(&self) -> (bool, bool) {
        (self.a_on, self.b_on)
    }
}

/// The heartbeat's state.
#[derive(Default)]
struct Beat {
    sounds: Option<(Option<FormId>, Option<FormId>)>,
    on: bool,
    /// Seconds since the last loss of health (the fade tweak), and the health then.
    since_hurt: f32,
    last_health: f32,
    hearts: Hearts,
    /// Sprint's heartbeat as last logged.
    told: HeartPace,
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
    (tweaks, sprint): (Option<Res<crate::tweaks::TweakSettings>>, Option<Res<crate::sprint::Sprint>>),
    mut loops: MessageWriter<LoopSound>,
    mut scale: ResMut<LoopScale>,
    mut b: Local<Beat>,
) {
    let Some(sfx) = sfx else { return };
    let find = |edid: &str| sfx.forms.sounds.iter().find(|(_, s)| s.edid.as_deref().is_some_and(|e| e.eq_ignore_ascii_case(edid))).map(|(f, _)| *f);
    let (Some(sound), fast) = *b.sounds.get_or_insert_with(|| (find(SOUND), find(FAST_SOUND))) else { return };
    let dead = crate::combat::is_dead(script.as_deref(), FormId::PLAYER_REF.0);
    let playing = *state.get() == PlayState::Playing && !dead;
    let want = playing && view.health > 0.0 && view.health < LOW;
    if want != b.on {
        b.on = want;
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
    // Sprint's, when the health's does not beat (and B is in the load order).
    let pace = sprint.as_deref().map_or(HeartPace::Off, crate::sprint::Sprint::heart);
    let pace = if !playing || want || (pace == HeartPace::Fast && fast.is_none()) { HeartPace::Off } else { pace };
    if pace != b.told {
        b.told = pace;
        info!("sprint: heartbeat {}", pace.text());
    }
    let out = b.hearts.step(playing, want, share, pace, time.delta_secs());
    for (s, start, now) in [(Some(sound), out.a, out.a_share), (fast, out.b, out.b_share)] {
        let Some(s) = s else { continue };
        if let Some(on) = start {
            loops.write(LoopSound { sound: s, on });
        }
        let was = scale.0.get(&s).copied().unwrap_or(1.0);
        if (was - now).abs() > 1e-3 {
            scale.0.insert(s, now);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    #[test]
    fn the_fade_runs_down_over_its_seconds() {
        assert_eq!(faded(0.0, 4.0), 1.0);
        assert!((faded(2.0, 4.0) - 0.5).abs() < 1e-6);
        assert_eq!(faded(9.0, 4.0), 0.0);
        assert_eq!(faded(9.0, 0.0), 1.0);
    }

    #[test]
    fn the_choice_slow_fast_and_the_health_heartbeat_winning() {
        assert_eq!(targets(false, 1.0, HeartPace::Off), (0.0, 0.0));
        assert_eq!(targets(false, 1.0, HeartPace::Slow), (SPRINT_VOLUME, 0.0));
        assert_eq!(targets(false, 1.0, HeartPace::Fast), (0.0, SPRINT_VOLUME));
        for pace in [HeartPace::Off, HeartPace::Slow, HeartPace::Fast] {
            assert_eq!(targets(true, 0.8, pace), (0.8, 0.0), "the health's wins over {pace:?}");
        }
        const { assert!(SPRINT_VOLUME < 0.5, "quiet under the game") };
    }

    /// `frames` of the same input; each frame's (A, B) shares and whether they play.
    fn run(h: &mut Hearts, health: bool, pace: HeartPace, frames: usize) -> Vec<(f32, f32, bool, bool)> {
        (0..frames)
            .map(|_| {
                let o = h.step(true, health, 1.0, pace, DT);
                let (a, b) = h.playing();
                (o.a_share, o.b_share, a, b)
            })
            .collect()
    }

    #[test]
    fn sprints_heartbeat_fades_in_cross_fades_and_out_without_steps() {
        let mut h = Hearts::default();
        let mut all = run(&mut h, false, HeartPace::Slow, 180);
        assert!(all[0].0 < 0.02 && all[0].2, "A starts silent and comes in");
        assert!((all[179].0 - SPRINT_VOLUME).abs() < 0.01);
        all.extend(run(&mut h, false, HeartPace::Fast, 240));
        assert!(all[419].1 > SPRINT_VOLUME - 0.01 && !all[419].2, "B in, A faded out and stopped");
        all.extend(run(&mut h, false, HeartPace::Off, 240));
        assert_eq!(h.playing(), (false, false), "both stopped once silent");
        // No step: at most the spring's top speed (ω / e of the volume a second) a frame.
        let most = omega_for(SPRINT_FADE_SECONDS) / std::f32::consts::E * SPRINT_VOLUME * DT * 1.05;
        assert!(all.windows(2).all(|w| (w[1].0 - w[0].0).abs() <= most && (w[1].1 - w[0].1).abs() <= most));
        // A loop stops only once it is silent.
        for w in all.windows(2) {
            if w[0].2 && !w[1].2 {
                assert!(w[0].0 < 0.01);
            }
            if w[0].3 && !w[1].3 {
                assert!(w[0].1 < 0.01);
            }
        }
    }

    #[test]
    fn the_health_heartbeat_wins_and_two_never_play_at_once() {
        // The fast sprint beat, then the health falls low: B stops, A is the health's at once.
        let mut h = Hearts::default();
        run(&mut h, false, HeartPace::Fast, 180);
        let o = h.step(true, true, 1.0, HeartPace::Fast, DT);
        assert_eq!((o.a, o.b), (Some(true), Some(false)));
        assert_eq!(o.a_share, 1.0, "from silence: at once, as before");
        for f in run(&mut h, true, HeartPace::Fast, 120) {
            assert!(f.2 && !f.3, "only A while the health's beats");
        }
        // The slow sprint beat, then the health's: A goes on and rises on its spring.
        let mut h = Hearts::default();
        run(&mut h, false, HeartPace::Slow, 180);
        let o = h.step(true, true, 1.0, HeartPace::Slow, DT);
        assert_eq!(o.a, None, "the same loop goes on");
        assert!(o.a_share < SPRINT_VOLUME + 0.05);
        let up = run(&mut h, true, HeartPace::Slow, 240);
        assert!(up.windows(2).all(|w| w[1].0 >= w[0].0 - 1e-6) && (up[239].0 - 1.0).abs() < 1e-3);
        // The health's over with the sprint's slow beat still wanted: back down, no stop.
        let o = h.step(true, false, 1.0, HeartPace::Slow, DT);
        assert_eq!(o.a, None);
        // Over with none wanted: it stops at once, as before.
        let mut h = Hearts::default();
        run(&mut h, true, HeartPace::Off, 30);
        let o = h.step(true, false, 1.0, HeartPace::Off, DT);
        assert_eq!(o.a, Some(false));
        // A menu stops both at once.
        let mut h = Hearts::default();
        run(&mut h, false, HeartPace::Fast, 60);
        let o = h.step(false, false, 1.0, HeartPace::Fast, DT);
        assert_eq!(o.b, Some(false));
    }
}
