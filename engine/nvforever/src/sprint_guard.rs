//! Guard for queue item 113: the first-person sprint pose that stays after the sprint.
//!
//! In the owner's MO2 load order, Just Assorted Mods' sprint plays `SpecialIdleSprint`,
//! a looping Movement-section idle with a `Camera1st` track, on the first-person rig
//! (`idles.rs`, `moves_with_player`), and its script stops it when the sprint ends. On a
//! guest in the replication build the arms and weapon were seen to stay in that pose
//! after the sprint [observed: owner's playtest 2026-11-08/09, item 113; not reproduced
//! without the JAM load order]. Why the script's stop does not come is not known, so
//! this is a guard rather than a fix [design: owner 2026-11-09, "build a guard for 113"].
//!
//! - **What it watches.** A first-person idle a script asked for (`PlayIdle`), of the
//!   Movement section, that loops and has not ended: the sprint pose.
//! - **Not sprinting.** No forward input (JAM holds Forward itself while it sprints, so
//!   a sprint has it), walking (the Run key), sneaking, the feet slower than a run, or
//!   the weapon raised (aiming), fired or reloaded [design].
//! - **Grace.** It must be so for [`GRACE`] seconds in a row: the script's own stop, and
//!   the idle's fade-out after it, come well inside that, and are not cut [design].
//! - **Ending it.** The idle is stopped as a script's `StopIdle` stops it: it fades out
//!   over its own blend-out to the first-person idle or aim under it, no snap. One log
//!   line each time: `guard 113: first-person sprint pose stuck for ... s (why), back to
//!   idle`.
//! - **A script that holds it on purpose is left alone.** The script asking for the same
//!   idle again while it plays resets the wait; asking for it again after the guard
//!   ended it (the same idle comes back) means the script wants it: the guard leaves
//!   that idle be until a new one starts, and says so once. So it never fights the
//!   mod's sprint script [design].

/// Seconds the player must be not sprinting, the sprint pose still playing, before the
/// guard ends it [design: the brief's 0.3-0.5 s; JAM's script stops sprinting the frame
/// after the player stops moving, and an idle's default fade is 0.2 s].
pub const GRACE: f32 = 0.4;

/// Feet slower than this (units per second, across the ground) are not a sprint:
/// one and a half times the measured walk, well under the measured run (344)
/// [engineering choice from `MoveSpeeds::OBSERVED`].
pub const SLOW: f32 = nvf_play::player::MoveSpeeds::OBSERVED.walk * 1.5;

/// What the player does this frame, as the guard reads it.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Sprinting {
    /// Forward input (+1), from the keys or a script's held Forward.
    pub forward: f32,
    pub walk: bool,
    pub sneak: bool,
    /// Speed of the feet across the ground, units per second.
    pub speed: f32,
    pub aiming: bool,
    pub firing: bool,
    pub reloading: bool,
}

/// Why the player is not sprinting, `None` when they may be.
pub fn not_sprinting(s: &Sprinting) -> Option<&'static str> {
    if s.aiming {
        Some("aiming")
    } else if s.firing {
        Some("firing")
    } else if s.reloading {
        Some("reloading")
    } else if s.forward <= 0.0 {
        Some("no forward input")
    } else if s.sneak {
        Some("sneaking")
    } else if s.walk {
        Some("walking")
    } else if s.speed < SLOW {
        Some("not moving at run speed")
    } else {
        None
    }
}

/// The sprint pose as the guard sees it this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Pose {
    /// The slot's serial of the idle playing (tells a new start from the same idle).
    pub serial: u32,
    /// Ended: fading out (the script's stop, or the guard's).
    pub ended: bool,
    /// The script asked for this idle again this frame while it showed.
    pub asked_again: bool,
}

/// What the guard decides this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    /// Nothing to do.
    Fine,
    /// End the pose: stuck this long, for this reason.
    Stuck { secs: f32, why: &'static str },
    /// The script brought the pose back after the guard ended it: left alone from now.
    LeftToScript,
}

/// The guard's memory between frames.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct SprintGuard {
    /// The idle watched, and seconds it has been playing while not sprinting.
    watching: Option<(u32, f32)>,
    /// The idle the guard ended last.
    ended: Option<u32>,
    /// The idle the script holds on purpose: not ended again.
    left: Option<u32>,
    /// Times it fired, for the run summary.
    pub fired: u32,
}

impl SprintGuard {
    /// One frame of `dt` seconds. `pose` is the sprint pose, `None` when none plays;
    /// `why` is [`not_sprinting`].
    pub fn step(&mut self, pose: Option<Pose>, why: Option<&'static str>, dt: f32) -> Verdict {
        let Some(p) = pose else {
            self.watching = None;
            return Verdict::Fine;
        };
        if self.left == Some(p.serial) {
            return Verdict::Fine;
        }
        if p.ended {
            // The normal exit (or the guard's own stop) fading out.
            self.watching = None;
            return Verdict::Fine;
        }
        if self.ended == Some(p.serial) {
            // The guard ended it and the same idle plays again: the script asked for it.
            self.left = Some(p.serial);
            self.watching = None;
            return Verdict::LeftToScript;
        }
        let Some(why) = why.filter(|_| !p.asked_again) else {
            self.watching = None;
            return Verdict::Fine;
        };
        let secs = match self.watching {
            Some((s, t)) if s == p.serial => t + dt,
            _ => dt,
        };
        if secs < GRACE {
            self.watching = Some((p.serial, secs));
            return Verdict::Fine;
        }
        self.watching = None;
        self.ended = Some(p.serial);
        self.fired += 1;
        Verdict::Stuck { secs, why }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DT: f32 = 1.0 / 60.0;

    fn sprint() -> Sprinting {
        Sprinting { forward: 1.0, speed: 500.0, ..Default::default() }
    }

    fn standing() -> Sprinting {
        Sprinting::default()
    }

    fn pose(serial: u32) -> Option<Pose> {
        Some(Pose { serial, ended: false, asked_again: false })
    }

    /// Run `frames` frames; the verdicts that were not `Fine`.
    fn run(g: &mut SprintGuard, frames: usize, mut f: impl FnMut(usize) -> (Option<Pose>, Sprinting)) -> Vec<(usize, Verdict)> {
        (0..frames)
            .filter_map(|k| {
                let (p, s) = f(k);
                let v = g.step(p, not_sprinting(&s), DT);
                (v != Verdict::Fine).then_some((k, v))
            })
            .collect()
    }

    #[test]
    fn what_is_not_a_sprint() {
        assert_eq!(not_sprinting(&sprint()), None);
        assert_eq!(not_sprinting(&standing()), Some("no forward input"));
        assert_eq!(not_sprinting(&Sprinting { aiming: true, ..sprint() }), Some("aiming"));
        assert_eq!(not_sprinting(&Sprinting { firing: true, ..sprint() }), Some("firing"));
        assert_eq!(not_sprinting(&Sprinting { reloading: true, ..sprint() }), Some("reloading"));
        assert_eq!(not_sprinting(&Sprinting { walk: true, ..sprint() }), Some("walking"));
        assert_eq!(not_sprinting(&Sprinting { sneak: true, ..sprint() }), Some("sneaking"));
        // Against a wall, forward held.
        assert_eq!(not_sprinting(&Sprinting { speed: 0.0, ..sprint() }), Some("not moving at run speed"));
    }

    #[test]
    fn a_held_sprint_is_left_alone() {
        let mut g = SprintGuard::default();
        assert!(run(&mut g, 600, |_| (pose(1), sprint())).is_empty());
        assert_eq!(g.fired, 0);
    }

    #[test]
    fn the_scripts_own_stop_inside_the_grace_is_not_cut() {
        // The player stops; the script stops the idle 0.1 s later, and it fades out.
        let mut g = SprintGuard::default();
        let out = run(&mut g, 120, |k| {
            let p = match k {
                0..60 => pose(1),
                60..66 => pose(1),
                66..80 => Some(Pose { serial: 1, ended: true, asked_again: false }),
                _ => None,
            };
            (p, if k < 60 { sprint() } else { standing() })
        });
        assert!(out.is_empty(), "{out:?}");
        assert_eq!(g.fired, 0);
    }

    /// A slot playing idle `serial` that the guard's `Stuck` ends (it fades out from the
    /// next frame), with the script asking for it again at frame `again` if given.
    fn live(g: &mut SprintGuard, frames: usize, serial: u32, again: Option<usize>, s: impl Fn(usize) -> Sprinting) -> Vec<(usize, Verdict)> {
        let mut ended = false;
        let mut out = Vec::new();
        for k in 0..frames {
            let asked_again = again == Some(k);
            if asked_again {
                // `IdlePlay::again` brings an ended idle back.
                ended = false;
            }
            let v = g.step(Some(Pose { serial, ended, asked_again }), not_sprinting(&s(k)), DT);
            if matches!(v, Verdict::Stuck { .. }) {
                ended = true;
            }
            if v != Verdict::Fine {
                out.push((k, v));
            }
        }
        out
    }

    #[test]
    fn a_stuck_pose_is_ended_after_the_grace_once() {
        let mut g = SprintGuard::default();
        let out = live(&mut g, 120, 1, None, |k| if k < 60 { sprint() } else { standing() });
        assert_eq!(out.len(), 1, "{out:?}");
        let (k, v) = out[0];
        // 0.4 s at 60 frames a second after frame 60.
        assert!((83..=85).contains(&k), "fired at frame {k}");
        match v {
            Verdict::Stuck { secs, why } => {
                assert!((GRACE..GRACE + 2.0 * DT).contains(&secs), "{secs}");
                assert_eq!(why, "no forward input");
            }
            v => panic!("{v:?}"),
        }
        assert_eq!(g.fired, 1);
    }

    #[test]
    fn raising_the_weapon_mid_sprint_ends_a_pose_the_script_keeps() {
        let mut g = SprintGuard::default();
        let out = run(&mut g, 90, |k| (pose(1), Sprinting { aiming: k >= 30, ..sprint() }));
        assert!(matches!(out.first(), Some((_, Verdict::Stuck { why: "aiming", .. }))), "{out:?}");
    }

    #[test]
    fn the_script_asking_again_resets_the_wait() {
        // A script that plays the idle again every 0.3 s while the player stands is holding it.
        let mut g = SprintGuard::default();
        let out = run(&mut g, 300, |k| (Some(Pose { serial: 1, ended: false, asked_again: k % 18 == 0 }), standing()));
        assert!(out.is_empty(), "{out:?}");
    }

    #[test]
    fn a_pose_the_script_brings_back_is_left_to_it_until_a_new_one() {
        let mut g = SprintGuard::default();
        // Stuck; the guard ends it; the script asks again while it fades and the same
        // idle plays on (its serial unchanged).
        let out = live(&mut g, 200, 7, Some(27), |_| standing());
        assert_eq!(out.len(), 2, "{out:?}");
        assert!(matches!(out[0].1, Verdict::Stuck { .. }));
        assert_eq!(out[1], (27, Verdict::LeftToScript));
        assert_eq!(g.fired, 1);
        // A new sprint (a new start of the idle) is watched again.
        let out = live(&mut g, 60, 8, None, |_| standing());
        assert!(matches!(out.as_slice(), [(_, Verdict::Stuck { .. })]), "{out:?}");
    }

    #[test]
    fn no_pose_resets_the_wait() {
        let mut g = SprintGuard::default();
        let out = run(&mut g, 120, |k| (if k % 20 == 19 { None } else { pose(1) }, standing()));
        assert!(out.is_empty(), "{out:?}");
    }
}
