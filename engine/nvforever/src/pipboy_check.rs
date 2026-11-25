//! The Pip-Boy glance checked on a third-person body (NVForever's own [design]): the
//! player's own body raises the left arm and looks at it while the glance is up, so the
//! third-person view shows the player checking it.
//!
//! - **The clip** is the game's own third-person idle [observed: FalloutNV.esm IDLE
//!   `PipboyExamine` 0x022C11; `meshes\characters\_male\idleanims\pipboyexamine.kf`
//!   (7.37 s: "AccessUp" at 0.03 s, the first "PipBoyStatic" at 0.60 s, "AccessDown" at
//!   6.73 s)]. Only the glance (`nvf_play::pipboy::glance`) plays it [design: the glance
//!   only looks]; the full Pip-Boy (Tab) has no check on the body, as on main.
//! - **Held while open** [design]: the clip plays from its start, then goes round its
//!   raised part (from its first "PipBoyUi" or "PipBoyStatic" key to its "AccessDown"
//!   key, [`CheckKeys`]) while the Pip-Boy stays up; put away, it goes on from
//!   "AccessDown" to its end, the arm lowered, and fades out. Put away before the arm was
//!   up, it fades out where it is.
//! - **Upper body only**: the clips drive the whole body (priority 15, the head 95)
//!   [observed]; the legs, the pelvis and the root are left to the walk, so a player who
//!   glances while moving keeps walking (as the stimpak idle is, `npc_stimpaks.rs`).
//! - Which check plays: `nvf_play::pipboy::glance::Check::of` (`player_body.rs`).
//! - Logged (`player_body.rs`): `player Third rig: Pip-Boy check <kind> put away <t> s
//!   into its clip ...`, `... Pip-Boy check <kind> gone <t> s after it was put away`.

use bevy::prelude::*;
use nvf_assets::anim_play::Clip;
use nvf_play::pipboy::glance::Check;

/// The glance's clip, in the skeleton's folder.
pub const GLANCE_CLIP: &str = "idleanims\\pipboyexamine.kf";

/// How long the check fades in and out, seconds [observed: the clips' "Blend: 15" key;
/// hypothesis: frames at 30 a second, as for the first-person clip].
pub const BLEND: f32 = 0.5;

/// `clip` with its lower body left out, its other tracks above the walk and the weapon's
/// stance (`npc_stimpaks::UPPER_PRIORITY`), and no root of its own.
pub fn upper_clip(clip: &Clip) -> Clip {
    let mut c = clip.clone();
    let keep: Vec<bool> = c.tracks.iter().map(|(n, _)| !crate::npc_stimpaks::lower_body(n)).collect();
    let mut k = keep.iter();
    c.tracks.retain(|_| *k.next().unwrap_or(&true));
    let mut k = keep.iter();
    c.priorities.retain(|_| *k.next().unwrap_or(&true));
    for p in c.priorities.iter_mut() {
        *p = (*p).max(crate::npc_stimpaks::UPPER_PRIORITY);
    }
    c.accum_root = None;
    c
}

/// Where a check clip's arm is up and where it goes down ([`CheckPlay`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CheckKeys {
    pub len: f32,
    /// The raised part's start: the first "PipBoyUi" or "PipBoyStatic" sound key.
    pub up: f32,
    /// The "AccessDown" sound key: the arm goes down from here.
    pub down: f32,
}

impl CheckKeys {
    /// From the clip's text keys; without them a quarter and nine tenths of its length
    /// [design].
    pub fn read(len: f32, keys: &[(f32, String)]) -> CheckKeys {
        let has = |t: &str, k: &str| t.to_ascii_lowercase().contains(k);
        let up = keys.iter().find(|(_, t)| has(t, "pipboyui") || has(t, "pipboystatic")).map_or(len * 0.25, |k| k.0);
        let down = keys.iter().find(|(_, t)| has(t, "accessdown")).map_or(len * 0.9, |k| k.0);
        if up < down && down <= len && up > 0.0 {
            CheckKeys { len, up, down }
        } else {
            CheckKeys { len, up: len * 0.25, down: len * 0.9 }
        }
    }
}

/// A check playing on a body.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CheckPlay {
    /// The check whose clip plays (`None`: nothing).
    pub kind: Check,
    /// Seconds into its clip.
    pub t: f32,
    /// How far it shows, 0 to 1 (before easing).
    pub weight: f32,
    /// Put away: going down.
    pub closing: bool,
    /// Seconds since it was put away (for the log).
    pub out: f32,
}

impl CheckPlay {
    /// Whether the weapon in the hand is put out of sight: while the glance is up, from
    /// the check's start until it is put away [design: the weapon is hidden while the
    /// Pip-Boy is up and comes back when it goes down].
    pub fn hides_weapon(&self) -> bool {
        self.kind != Check::None && !self.closing
    }

    /// The check whose clip is wanted now: the one going down until it is gone, else what
    /// the player does.
    pub fn clip_for(&self, want: Check) -> Check {
        if self.kind != Check::None && (self.closing || want == Check::None) {
            self.kind
        } else {
            want
        }
    }

    /// Move on by `dt` with the player doing `want`; `keys` are those of
    /// [`Self::clip_for`]'s clip. Returns the clip's time and its eased weight while it
    /// shows.
    pub fn step(&mut self, want: Check, dt: f32, keys: CheckKeys) -> Option<(f32, f32)> {
        let kind = self.clip_for(want);
        if kind == Check::None {
            *self = CheckPlay::default();
            return None;
        }
        if kind != self.kind {
            // The other check while the arm is up: the new clip from where its arm is up.
            let up_already = self.kind != Check::None && !self.closing && self.weight >= 0.5;
            *self = CheckPlay { kind, t: if up_already { keys.up } else { 0.0 }, weight: if up_already { self.weight } else { 0.0 }, closing: false, out: 0.0 };
        }
        if want == kind {
            if self.closing {
                (self.closing, self.out) = (false, 0.0);
                if self.t >= keys.down {
                    self.t = keys.up;
                }
            }
            self.t += dt;
            if self.t >= keys.down {
                let span = (keys.down - keys.up).max(0.01);
                self.t = keys.up + (self.t - keys.down) % span;
            }
            self.weight = (self.weight + dt / BLEND).min(1.0);
        } else {
            if !self.closing {
                self.closing = true;
                if self.t >= keys.up {
                    self.t = self.t.max(keys.down);
                }
            }
            self.out += dt;
            if self.t < keys.up {
                // Put away before the arm was up: it fades out where it is. Its time
                // stays put, so it never runs on into the raised part with the weight
                // left where it was (closed 1.8 s into a 1.9 s-to-arm-up check, 0.1 s before its
                // arm is up, the arm stayed raised the clip's next 10.8 s [observed:
                // buckallobill, in game]).
                self.weight -= dt / BLEND;
            } else {
                self.t += dt;
                self.weight = self.weight.min((keys.len - self.t) / BLEND);
            }
            if self.weight <= 0.0 || self.t >= keys.len {
                *self = CheckPlay::default();
                return None;
            }
        }
        let w = self.weight.clamp(0.0, 1.0);
        Some((self.t.min(keys.len), w * w * (3.0 - 2.0 * w)))
    }
}

/// This game's player's check on the third-person body (`player_body.rs`).
#[derive(Resource, Default)]
pub struct PlayerCheck(pub CheckPlay);

pub struct PipBoyCheckPlugin;

impl Plugin for PipBoyCheckPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<PlayerCheck>();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn examine() -> CheckKeys {
        let keys = vec![
            (0.0, "start".to_string()),
            (0.0333, "Blend: 15".to_string()),
            (0.0333, "Sound: NPCIdlePipBoyAccessUp".to_string()),
            (0.6, "Sound: NPCIdlePipBoyStatic".to_string()),
            (2.67, "Sound: NPCIdlePipBoyStatic".to_string()),
            (6.73, "Sound: NPCIdlePipBoyAccessDown".to_string()),
            (7.37, "end".to_string()),
        ];
        CheckKeys::read(7.37, &keys)
    }

    #[test]
    fn the_keys_are_read_from_the_clips_sounds() {
        assert_eq!(examine(), CheckKeys { len: 7.37, up: 0.6, down: 6.73 });
        let ui = vec![(0.37, "Sound: NPCIdlePipBoyAccessUp".to_string()), (1.9, "Sound: NPCIdlePipBoyUi".to_string()), (11.57, "Sound: NPCIdlePipBoyAccessDown".to_string())];
        assert_eq!(CheckKeys::read(12.67, &ui), CheckKeys { len: 12.67, up: 1.9, down: 11.57 });
        // Nothing to go by: a quarter and nine tenths.
        assert_eq!(CheckKeys::read(4.0, &[]), CheckKeys { len: 4.0, up: 1.0, down: 3.6 });
    }

    #[test]
    fn a_check_comes_up_goes_round_its_raised_part_and_goes_down() {
        let k = examine();
        let mut p = CheckPlay::default();
        assert_eq!(p.step(Check::None, 0.1, k), None);
        let (t, w) = p.step(Check::Glance, 0.1, k).unwrap();
        assert!((t - 0.1).abs() < 1e-5 && w > 0.0 && w < 1.0);
        // Held: round the raised part, at full weight.
        for _ in 0..200 {
            let (t, w) = p.step(Check::Glance, 0.1, k).unwrap();
            assert!(t < k.down, "{t}");
            if p.t > 1.0 {
                assert!((w - 1.0).abs() < 1e-5);
            }
        }
        assert!(p.t >= k.up);
        // Put away: from AccessDown to the end, fading.
        let (t, _) = p.step(Check::None, 0.1, k).unwrap();
        assert!(t >= k.down);
        let mut n = 0;
        while p.step(Check::None, 0.1, k).is_some() {
            n += 1;
            assert!(n < 100);
        }
        assert_eq!(p, CheckPlay::default());
    }

    #[test]
    fn the_weapon_is_hidden_while_the_pipboy_is_up() {
        let k = examine();
        let mut p = CheckPlay::default();
        assert!(!p.hides_weapon());
        p.step(Check::Glance, 0.1, k);
        assert!(p.hides_weapon(), "from the start");
        for _ in 0..50 {
            p.step(Check::Glance, 0.1, k);
        }
        assert!(p.hides_weapon(), "while up");
        p.step(Check::None, 0.1, k);
        assert!(!p.hides_weapon(), "back as soon as it is put away");
    }

    #[test]
    fn put_away_before_the_arm_is_up_it_fades_where_it_is() {
        let k = examine();
        let mut p = CheckPlay::default();
        p.step(Check::Glance, 0.2, k);
        let (t, _) = p.step(Check::None, 0.05, k).unwrap();
        assert!(t < k.up, "{t}");
        assert_eq!(p.clip_for(Check::None), Check::Glance);
        for _ in 0..20 {
            p.step(Check::None, 0.05, k);
        }
        assert_eq!(p.kind, Check::None);
    }

    /// Closed at any moment, the check is gone within its out-blend: the fade where it
    /// is ([`BLEND`]) before the arm is up, else the clip from "AccessDown" to its end.
    /// It never plays on into the raised part with the Pip-Boy down ([observed:
    /// buckallobill, in game]: a check closed 1.8 s in, 0.1 s before its arm was
    /// up, held the arm raised 10.8 s more).
    #[test]
    fn closed_at_any_moment_the_check_is_gone_within_its_out_blend() {
        let dt = 1.0 / 60.0;
        for (want, k) in [(Check::Glance, examine()), (Check::Glance, CheckKeys { len: 12.67, up: 1.9, down: 11.57 })] {
            let out = BLEND.max(k.len - k.down) + 2.0 * dt;
            let mut open = 0.0;
            while open < k.len + 3.0 {
                let mut p = CheckPlay::default();
                let mut t = 0.0;
                while t < open {
                    p.step(want, dt, k);
                    t += dt;
                }
                let (mut gone, mut last_w) = (0.0, f32::MAX);
                while let Some((at, w)) = p.step(Check::None, dt, k) {
                    gone += dt;
                    assert!(gone <= out, "{want:?} closed at {open:.2} s: still shown {gone:.2} s later (clip at {at:.2} s, weight {w:.2})");
                    assert!(w <= last_w + 1e-5, "{want:?} closed at {open:.2} s: the weight rose to {w:.2}");
                    assert!(at < k.up || at >= k.down, "{want:?} closed at {open:.2} s: the clip plays its raised part at {at:.2} s");
                    last_w = w;
                }
                assert_eq!(p, CheckPlay::default());
                open += 0.05;
            }
        }
    }
}
