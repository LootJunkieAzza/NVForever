//! An NPC's or creature's body in the air (docs/spec-notes/actor-jumps.md): when its
//! jump loop plays, when its landing plays and for how long, and which of the
//! skeleton's jump clips fits the way it moves.
//!
//! People have `<stance>jumploop<dir>.kf` (looped while in the air) and
//! `<stance>jumpland<dir>.kf` (once, on landing) beside their skeleton, with no clip for
//! the take-off [observed: the base meshes archive lists `mtjumploop.kf`,
//! `mtjumpland.kf` and their `forward`/`backward`/`left`/`right` forms in
//! `characters\_male\`, and no `jumpstart`]. No creature folder has any [observed]:
//! a creature keeps its movement clip in the air.

use crate::player_clips::Dir;

/// Time in the air before the jump loop plays, seconds: steps and slopes leave the
/// ground briefly [engineering choice: the player's body's, `player_body.rs`].
pub const AIR_LOOP: f32 = 0.15;
/// The shortest time in the air that plays a landing, seconds [engineering choice: as
/// the player's body].
pub const AIR_LAND: f32 = 0.35;
/// Cross-fade into the loop and out of the landing, seconds [engineering choice].
pub const FADE: f32 = 0.1;
/// Below this speed (units per second, the body's axes) the clip without a direction
/// plays [engineering choice].
pub const STILL_SPEED: f32 = 10.0;

/// Where a body is in a jump.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub enum JumpPhase {
    #[default]
    Ground,
    /// In the air for this long, seconds.
    Air(f32),
    /// Landing for this long, seconds (its clip playing once).
    Land(f32),
}

/// What a step of [`JumpState`] began.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum JumpEvent {
    /// In the air long enough for the loop.
    Fall,
    Land,
}

/// A body's jump: its phase, and counts for the summary.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct JumpState {
    pub phase: JumpPhase,
}

impl JumpState {
    /// Advance by `dt` with the body in the air or not; `land_len` is the length of
    /// the landing clip that would play (`None`: no clip, the landing is skipped).
    pub fn step(&mut self, in_air: bool, dt: f32, land_len: Option<f32>) -> Option<JumpEvent> {
        let dt = dt.max(0.0);
        let (next, event) = match (self.phase, in_air) {
            (JumpPhase::Air(t), true) => (JumpPhase::Air(t + dt), (t < AIR_LOOP && t + dt >= AIR_LOOP).then_some(JumpEvent::Fall)),
            (_, true) => (JumpPhase::Air(dt), (dt >= AIR_LOOP).then_some(JumpEvent::Fall)),
            (JumpPhase::Air(t), false) => match land_len {
                Some(_) if t >= AIR_LAND => (JumpPhase::Land(0.0), Some(JumpEvent::Land)),
                _ => (JumpPhase::Ground, None),
            },
            (JumpPhase::Land(t), false) => match land_len {
                Some(len) if t + dt < len => (JumpPhase::Land(t + dt), None),
                _ => (JumpPhase::Ground, None),
            },
            (JumpPhase::Ground, false) => (JumpPhase::Ground, None),
        };
        self.phase = next;
        event
    }

    /// The jump clip over the body's other clips: `(landing, phase, weight)`, `phase`
    /// the share of the clip played (looped in the air), `weight` its share of the
    /// pose; `None` on the ground (or in the air too briefly for the loop). `len` is
    /// the length of the clip that plays (the loop's in the air, the landing's after).
    pub fn clip(&self, len: f32) -> Option<(bool, f32, f32)> {
        let len = len.max(1e-3);
        match self.phase {
            JumpPhase::Ground => None,
            JumpPhase::Air(t) if t < AIR_LOOP => None,
            JumpPhase::Air(t) => {
                let s = t - AIR_LOOP;
                Some((false, (s / len).fract(), (s / FADE).min(1.0)))
            }
            JumpPhase::Land(t) => Some((true, (t / len).min(0.999), ((len - t) / FADE).clamp(0.0, 1.0))),
        }
    }

    /// Landing (GetAnimAction 12 for the player's landing [observed]; an NPC's taken to
    /// read the same [hypothesis], anim-state.md §1).
    pub fn landing(&self) -> bool {
        matches!(self.phase, JumpPhase::Land(_))
    }

    /// The jump loop playing.
    pub fn falling(&self) -> bool {
        matches!(self.phase, JumpPhase::Air(t) if t >= AIR_LOOP)
    }
}

/// The direction of a jump clip for a velocity in the body's axes (x right, y forward):
/// `None` (the plain clip) when nearly still, else the nearest of the four.
pub fn jump_dir(v: [f32; 2]) -> Option<Dir> {
    if v[0].hypot(v[1]) < STILL_SPEED {
        return None;
    }
    Some(match crate::locomotion::direction(v) {
        crate::locomotion::Dir::Forward => Dir::Forward,
        crate::locomotion::Dir::Backward => Dir::Backward,
        crate::locomotion::Dir::Left => Dir::Left,
        crate::locomotion::Dir::Right => Dir::Right,
    })
}

/// Index of a direction in a table of jump clips: 0 the plain clip, then forward,
/// backward, left, right.
pub fn dir_index(d: Option<Dir>) -> usize {
    match d {
        None => 0,
        Some(Dir::Forward) => 1,
        Some(Dir::Backward) => 2,
        Some(Dir::Left) => 3,
        Some(Dir::Right) => 4,
    }
}

/// The file names of a stance's jump clips (`mt`, `sneakmt`, a weapon's `1hp` …),
/// looped (`land` false) or landing, in [`dir_index`] order: `mtjumploop.kf`,
/// `mtjumploopforward.kf` …
pub fn clip_names(stance: &str, land: bool) -> [String; 5] {
    let j = if land { "jumpland" } else { "jumploop" };
    [None, Some(Dir::Forward), Some(Dir::Backward), Some(Dir::Left), Some(Dir::Right)].map(|d| format!("{stance}{j}{}.kf", d.map_or("", Dir::name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fall_loops_then_lands_once() {
        let mut j = JumpState::default();
        let dt = 1.0 / 60.0;
        let mut events = Vec::new();
        // Half a second in the air.
        for _ in 0..30 {
            events.extend(j.step(true, dt, Some(0.77)));
        }
        assert!(j.falling());
        assert_eq!(events, vec![JumpEvent::Fall]);
        let (land, phase, w) = j.clip(1.33).unwrap();
        assert!(!land && phase > 0.0 && (w - 1.0).abs() < 1e-6);
        // Down: the landing plays for its length, then the ground.
        events.extend(j.step(false, dt, Some(0.77)));
        assert_eq!(events.last(), Some(&JumpEvent::Land));
        assert!(j.landing());
        let mut frames = 1;
        while j.landing() {
            j.step(false, dt, Some(0.77));
            frames += 1;
            assert!(frames < 100);
        }
        assert!((45..=48).contains(&frames), "{frames}");
        assert_eq!(j.phase, JumpPhase::Ground);
    }

    #[test]
    fn brief_hops_play_nothing() {
        let mut j = JumpState::default();
        // A step down: a tenth of a second off the ground.
        for _ in 0..6 {
            assert_eq!(j.step(true, 1.0 / 60.0, Some(0.77)), None);
            assert_eq!(j.clip(1.0), None);
        }
        assert_eq!(j.step(false, 1.0 / 60.0, Some(0.77)), None);
        assert_eq!(j.phase, JumpPhase::Ground);
        // In the air long enough for the loop but not for a landing.
        let mut j = JumpState::default();
        for _ in 0..15 {
            j.step(true, 1.0 / 60.0, Some(0.77));
        }
        assert!(j.falling());
        assert_eq!(j.step(false, 1.0 / 60.0, Some(0.77)), None);
        assert_eq!(j.phase, JumpPhase::Ground);
        // Without a landing clip (a creature): straight back to the ground.
        let mut j = JumpState { phase: JumpPhase::Air(1.0) };
        assert_eq!(j.step(false, 0.01, None), None);
        assert_eq!(j.phase, JumpPhase::Ground);
    }

    #[test]
    fn the_landing_fades_out_at_its_end() {
        let j = JumpState { phase: JumpPhase::Land(0.72) };
        let (land, phase, w) = j.clip(0.77).unwrap();
        assert!(land && (phase - 0.72 / 0.77).abs() < 1e-5 && (w - 0.5).abs() < 1e-4, "{phase} {w}");
        // The loop fades in.
        let j = JumpState { phase: JumpPhase::Air(AIR_LOOP + 0.05) };
        assert!((j.clip(1.0).unwrap().2 - 0.5).abs() < 1e-4);
    }

    #[test]
    fn clips_by_direction() {
        assert_eq!(jump_dir([0.0, 2.0]), None);
        assert_eq!(jump_dir([0.0, 200.0]), Some(Dir::Forward));
        assert_eq!(jump_dir([-150.0, 20.0]), Some(Dir::Left));
        assert_eq!(dir_index(jump_dir([0.0, -90.0])), 2);
        assert_eq!(clip_names("mt", false)[0], "mtjumploop.kf");
        assert_eq!(clip_names("1hp", true)[4], "1hpjumplandright.kf");
        assert_eq!(clip_names("sneakmt", true)[1], "sneakmtjumplandforward.kf");
    }
}
