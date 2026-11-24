//! Aim-down-sights sway (a local build, not part of NVForever upstream).
//!
//! With true iron sights the gun is locked onto the eye (`iron_sights.rs`) and the normal
//! sway (`view_sway.rs`) turns it about that same point, so down the sights it hardly
//! moves. This adds springs, only while aimed:
//!
//! - **Mouse inertia**: turning or looking up and down knocks the gun the other way
//!   and a damped spring pulls it back to centre.
//! - **Movement**: walking or strafing while aimed moves the gun a little and lets it
//!   spring back when you stop.
//! - **Stop settle**: a short wobble when you stop moving (our own damped springs,
//!   `ads_sway_curves.rs`).
//!
//! Everything fades with the sights share, so the hip is not touched. The strengths
//! are read from `ads_sway.ini` beside `nvforever.exe` (written with the defaults the
//! first time it is missing), so they can be tuned without rebuilding.

use bevy::math::{Quat, Vec2, Vec3};
use std::sync::OnceLock;

include!("ads_sway_curves.rs");

/// The tunable numbers (`ads_sway.ini`).
#[derive(Debug, Clone)]
pub struct AdsSwaySettings {
    pub enabled: bool,
    // Mouse inertia. Positive strengths make the gun trail the view; negative reverse it.
    pub yaw_strength: f32,
    pub pitch_strength: f32,
    pub horizontal_stiffness: f32,
    pub horizontal_damping: f32,
    pub horizontal_max: f32,
    pub vertical_stiffness: f32,
    pub vertical_damping: f32,
    pub vertical_max: f32,
    // Movement.
    pub movement_enabled: bool,
    pub strafe_strength: f32,
    pub forward_strength: f32,
    pub backward_strength: f32,
    pub movement_stiffness: f32,
    pub movement_damping: f32,
    pub movement_max: f32,
    /// The ground speed (units/s) that counts as a full key press.
    pub full_speed: f32,
    // Stop settle.
    pub stop_enabled: bool,
    pub stop_translation: f32,
    pub stop_pitch: f32,
    pub stop_roll: f32,
    pub stop_blend_in: f32,
    pub stop_delay_frames: f32,
    pub stop_speed: f32,
}

impl Default for AdsSwaySettings {
    fn default() -> Self {
        AdsSwaySettings {
            enabled: true,
            yaw_strength: 0.3,
            pitch_strength: 0.5,
            horizontal_stiffness: 40.0,
            horizontal_damping: 12.0,
            horizontal_max: 1.25,
            vertical_stiffness: 40.0,
            vertical_damping: 12.0,
            vertical_max: 1.25,
            movement_enabled: true,
            strafe_strength: 0.6,
            forward_strength: 0.3,
            backward_strength: 0.25,
            movement_stiffness: 30.0,
            movement_damping: 12.0,
            movement_max: 1.0,
            full_speed: 150.0,
            stop_enabled: true,
            stop_translation: 0.05,
            stop_pitch: 0.12,
            stop_roll: 0.05,
            stop_blend_in: 0.15,
            stop_delay_frames: 2.0,
            stop_speed: 0.75,
        }
    }
}

const DEFAULT_INI: &str = "\
; Aim-down-sights sway for NVForever (local build).
; Only works while aiming down the sights; the hip is unchanged.
; Edit and restart the game to apply. Delete this file to get the defaults back.
; Offsets are in game units at the eye: about 1 is a clearly visible shift.

[General]
Enabled=1

[Inertia]
; Mouse movement knocks the gun; positive = the gun trails the view, negative = leads it.
YawStrength=0.3
PitchStrength=0.5
HorizontalStiffness=40
HorizontalDamping=12
HorizontalMaxOffset=1.25
VerticalStiffness=40
VerticalDamping=12
VerticalMaxOffset=1.25

[Movement]
Enabled=1
; Positive strafe = the gun moves the way you strafe.
StrafeStrength=0.6
; Walking forward pulls the gun back toward you, backward pushes it away.
ForwardStrength=0.3
BackwardStrength=0.25
Stiffness=30
Damping=12
MaxOffset=1.0
; Ground speed (units per second) treated as a full key press.
FullSpeed=150

[StopSettle]
Enabled=1
TranslationStrength=0.05
PitchStrength=0.12
RollStrength=0.05
; Seconds to fade the wobble in.
BlendInSeconds=0.15
; Frames (at 30 FPS) to wait after stopping before it plays.
DelayFrames=2
; Playback speed (1.0 = 0.87 s).
PlaybackSpeed=0.75
";

fn ini_path() -> Option<std::path::PathBuf> {
    Some(std::env::current_exe().ok()?.parent()?.join("ads_sway.ini"))
}

/// Read `key=value` lines under `[Section]` headers into `Section.Key` (lower case).
fn parse_ini(text: &str) -> std::collections::HashMap<String, String> {
    let mut out = std::collections::HashMap::new();
    let mut section = String::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with(';') || line.starts_with('#') {
            continue;
        }
        if let Some(s) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            section = s.trim().to_ascii_lowercase();
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.split(';').next().unwrap_or("").trim();
            out.insert(format!("{section}.{}", k.trim().to_ascii_lowercase()), v.to_string());
        }
    }
    out
}

impl AdsSwaySettings {
    pub fn from_ini_text(text: &str) -> Self {
        let map = parse_ini(text);
        let d = AdsSwaySettings::default();
        let f = |key: &str, def: f32, lo: f32, hi: f32| map.get(key).and_then(|v| v.parse::<f32>().ok()).filter(|v| v.is_finite()).map_or(def, |v| v.clamp(lo, hi));
        let b = |key: &str, def: bool| map.get(key).map_or(def, |v| !matches!(v.as_str(), "0" | "false" | "off" | "no"));
        AdsSwaySettings {
            enabled: b("general.enabled", d.enabled),
            yaw_strength: f("inertia.yawstrength", d.yaw_strength, -5.0, 5.0),
            pitch_strength: f("inertia.pitchstrength", d.pitch_strength, -5.0, 5.0),
            horizontal_stiffness: f("inertia.horizontalstiffness", d.horizontal_stiffness, 1.0, 400.0),
            horizontal_damping: f("inertia.horizontaldamping", d.horizontal_damping, 0.0, 100.0),
            horizontal_max: f("inertia.horizontalmaxoffset", d.horizontal_max, 0.0, 10.0),
            vertical_stiffness: f("inertia.verticalstiffness", d.vertical_stiffness, 1.0, 400.0),
            vertical_damping: f("inertia.verticaldamping", d.vertical_damping, 0.0, 100.0),
            vertical_max: f("inertia.verticalmaxoffset", d.vertical_max, 0.0, 10.0),
            movement_enabled: b("movement.enabled", d.movement_enabled),
            strafe_strength: f("movement.strafestrength", d.strafe_strength, -10.0, 10.0),
            forward_strength: f("movement.forwardstrength", d.forward_strength, -10.0, 10.0),
            backward_strength: f("movement.backwardstrength", d.backward_strength, -10.0, 10.0),
            movement_stiffness: f("movement.stiffness", d.movement_stiffness, 1.0, 400.0),
            movement_damping: f("movement.damping", d.movement_damping, 0.0, 100.0),
            movement_max: f("movement.maxoffset", d.movement_max, 0.0, 10.0),
            full_speed: f("movement.fullspeed", d.full_speed, 10.0, 2000.0),
            stop_enabled: b("stopsettle.enabled", d.stop_enabled),
            stop_translation: f("stopsettle.translationstrength", d.stop_translation, 0.0, 5.0),
            stop_pitch: f("stopsettle.pitchstrength", d.stop_pitch, 0.0, 5.0),
            stop_roll: f("stopsettle.rollstrength", d.stop_roll, 0.0, 5.0),
            stop_blend_in: f("stopsettle.blendinseconds", d.stop_blend_in, 0.0, 0.5),
            stop_delay_frames: f("stopsettle.delayframes", d.stop_delay_frames, 0.0, 30.0),
            stop_speed: f("stopsettle.playbackspeed", d.stop_speed, 0.25, 3.0),
        }
    }

    /// The settings from `ads_sway.ini` beside the program, read once; the file is
    /// written with the defaults when it is missing.
    pub fn get() -> &'static AdsSwaySettings {
        static SETTINGS: OnceLock<AdsSwaySettings> = OnceLock::new();
        SETTINGS.get_or_init(|| {
            let Some(path) = ini_path() else { return AdsSwaySettings::default() };
            match std::fs::read_to_string(&path) {
                Ok(text) => {
                    let s = AdsSwaySettings::from_ini_text(&text);
                    bevy::log::info!("ads sway: settings from {} ({s:?})", path.display());
                    s
                }
                Err(_) => {
                    let wrote = std::fs::write(&path, DEFAULT_INI).is_ok();
                    bevy::log::info!("ads sway: no {}; defaults used{}", path.display(), if wrote { " and written there" } else { "" });
                    AdsSwaySettings::default()
                }
            }
        })
    }
}

/// What the aim sway reads each frame.
#[derive(Debug, Clone, Copy, Default)]
pub struct AdsInput {
    pub dt: f32,
    /// The view's compass heading (clockwise) and pitch (up), radians.
    pub heading: f32,
    pub pitch: f32,
    /// Ground velocity in the view's frame: x to the right, y ahead (units/s).
    pub velocity: Vec2,
    pub airborne: bool,
    /// How far the gun is up on the sights (0–1, eased); 0 shows nothing.
    pub share: f32,
}

/// The aim sway's state across frames.
#[derive(Debug, Clone, Default)]
pub struct AdsSway {
    last: Option<(f32, f32)>,
    /// Mouse spring: x sideways, y up (view frame), and their velocities.
    look: Vec2,
    look_vel: Vec2,
    /// Movement spring: x sideways, y ahead.
    walk: Vec2,
    walk_vel: Vec2,
    // Stop settle.
    moving_for: f32,
    idle_for: f32,
    resume_for: f32,
    armed: bool,
    playing: bool,
    interrupted: bool,
    stop_time: f32,
    stop_weight: f32,
    cooldown: f32,
}

/// `a - b` as the shorter way round, radians.
fn angle_diff(a: f32, b: f32) -> f32 {
    use std::f32::consts::{PI, TAU};
    (a - b + PI).rem_euclid(TAU) - PI
}

/// A settle channel at `t` seconds (0 outside the settle).
fn sample(c: &Settle, t: f32) -> f32 {
    if !(0.0..=STOP_LENGTH).contains(&t) {
        return 0.0;
    }
    let taper = 1.0 - (t / STOP_LENGTH).powi(2);
    (std::f32::consts::TAU * c.freq * t).sin() * (-STOP_DECAY * t).exp() * taper * c.peak / c.raw_peak
}

/// The length of the stop settle, seconds.
fn stop_length() -> f32 {
    STOP_LENGTH
}

impl AdsSway {
    pub fn update(&mut self, i: AdsInput) -> crate::view_sway::SwayPose {
        self.update_with(i, AdsSwaySettings::get())
    }

    pub fn update_with(&mut self, i: AdsInput, s: &AdsSwaySettings) -> crate::view_sway::SwayPose {
        use crate::view_sway::SwayPose;
        let dt = i.dt.clamp(0.0, 0.05);
        // Mouse: the view's turn this frame, skipped on a jump of the view.
        let mut turn = Vec2::ZERO;
        if let Some((h, p)) = self.last {
            let d = Vec2::new(angle_diff(i.heading, h), i.pitch - p);
            if d.length() < 0.5 {
                turn = d;
            }
        }
        self.last = Some((i.heading, i.pitch));
        if !s.enabled || i.share <= 0.0 {
            // Rest while not aimed, so each aim starts centred.
            *self = AdsSway { last: self.last, ..AdsSway::default() };
            return SwayPose::NONE;
        }
        // The impulse: velocity kicked by the turn (×30), against the turn when the
        // strength is positive (turning right sends the gun left; looking up, down).
        self.look_vel.x = (self.look_vel.x - turn.x * s.yaw_strength * 30.0).clamp(-120.0, 120.0);
        self.look_vel.y = (self.look_vel.y - turn.y * s.pitch_strength * 30.0).clamp(-120.0, 120.0);
        // Movement targets.
        let (side, ahead) = if i.airborne || !s.movement_enabled {
            (0.0, 0.0)
        } else {
            ((i.velocity.x / s.full_speed).clamp(-1.0, 1.0), (i.velocity.y / s.full_speed).clamp(-1.0, 1.0))
        };
        let target = Vec2::new(
            (side * s.strafe_strength).clamp(-s.movement_max, s.movement_max),
            (-ahead * if ahead >= 0.0 { s.forward_strength } else { s.backward_strength }).clamp(-s.movement_max, s.movement_max),
        );
        // Springs, in short steps for stability.
        let steps = (dt / 0.008).ceil().max(1.0) as usize;
        let h = dt / steps as f32;
        for _ in 0..steps {
            self.look_vel.x += (-s.horizontal_stiffness * self.look.x - s.horizontal_damping * self.look_vel.x) * h;
            self.look_vel.y += (-s.vertical_stiffness * self.look.y - s.vertical_damping * self.look_vel.y) * h;
            self.look.x = (self.look.x + self.look_vel.x * h).clamp(-s.horizontal_max, s.horizontal_max);
            self.look.y = (self.look.y + self.look_vel.y * h).clamp(-s.vertical_max, s.vertical_max);
            self.walk_vel += (s.movement_stiffness * (target - self.walk) - s.movement_damping * self.walk_vel) * h;
            self.walk = (self.walk + self.walk_vel * h).clamp(Vec2::splat(-s.movement_max), Vec2::splat(s.movement_max));
        }
        // Stop settle: one per stop after moving a moment; moving again fades it out.
        let moving = !i.airborne && i.velocity.length() > 0.1 * s.full_speed;
        self.cooldown = (self.cooldown - dt).max(0.0);
        if moving {
            self.moving_for = (self.moving_for + dt).min(1.0);
            self.idle_for = 0.0;
            if !self.playing && self.moving_for >= 0.12 {
                self.armed = true;
            }
        } else {
            self.idle_for = (self.idle_for + dt).min(1.0);
            if self.idle_for >= 0.08 {
                self.moving_for = 0.0;
            }
            if s.stop_enabled && !self.playing && self.armed && self.idle_for >= 0.08 && self.cooldown <= 0.0 {
                self.playing = true;
                self.interrupted = false;
                self.armed = false;
                self.stop_time = 0.0;
                self.stop_weight = 1.0;
                self.resume_for = 0.0;
                self.cooldown = 1.0;
            }
        }
        let delay = s.stop_delay_frames / 30.0;
        if self.playing {
            self.resume_for = if moving { (self.resume_for + dt).min(1.0) } else { 0.0 };
            if self.resume_for >= 0.1 {
                self.interrupted = true;
            }
            self.stop_time += dt;
            if self.interrupted {
                self.stop_weight = (self.stop_weight - dt * 8.0).max(0.0);
            }
            if self.stop_time >= stop_length() / s.stop_speed + delay || self.stop_weight <= 0.0 {
                self.playing = false;
                self.stop_weight = 0.0;
                self.armed = false;
            }
        }
        let (mut stop_ahead, mut stop_up, mut stop_pitch, mut stop_roll) = (0.0, 0.0, 0.0, 0.0);
        if self.playing {
            let elapsed = (self.stop_time - delay).max(0.0);
            let t = elapsed * s.stop_speed;
            let blend = if s.stop_blend_in > 1e-4 {
                let u = (elapsed / s.stop_blend_in).clamp(0.0, 1.0);
                u * u * (3.0 - 2.0 * u)
            } else {
                1.0
            };
            let w = self.stop_weight * blend;
            stop_ahead = sample(&STOP_FORWARD, t) * s.stop_translation * w;
            stop_up = sample(&STOP_UP, t) * s.stop_translation * w;
            stop_pitch = sample(&STOP_PITCH_DEG, t).to_radians() * s.stop_pitch * w;
            stop_roll = sample(&STOP_ROLL_DEG, t).to_radians() * s.stop_roll * w;
        }
        let k = i.share.clamp(0.0, 1.0);
        let offset = Vec3::new(self.look.x + self.walk.x, self.walk.y + stop_ahead, self.look.y + stop_up) * k;
        let rotation = Quat::from_rotation_x(stop_pitch * k) * Quat::from_rotation_y(stop_roll * k);
        SwayPose { rotation, offset }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn aimed(heading: f32, pitch: f32, velocity: Vec2) -> AdsInput {
        AdsInput { dt: 1.0 / 60.0, heading, pitch, velocity, airborne: false, share: 1.0 }
    }

    #[test]
    fn standing_still_aimed_rests() {
        let s = AdsSwaySettings::default();
        let mut a = AdsSway::default();
        let mut p = crate::view_sway::SwayPose::NONE;
        for _ in 0..120 {
            p = a.update_with(aimed(1.0, 0.0, Vec2::ZERO), &s);
        }
        assert!(p.offset.length() < 1e-4, "{p:?}");
    }

    #[test]
    fn turning_right_sends_the_gun_left_and_it_comes_back() {
        let s = AdsSwaySettings::default();
        let mut a = AdsSway::default();
        a.update_with(aimed(0.0, 0.0, Vec2::ZERO), &s);
        let mut h = 0.0;
        let mut p = crate::view_sway::SwayPose::NONE;
        for _ in 0..20 {
            h += 2.0 / 60.0;
            p = a.update_with(aimed(h, 0.0, Vec2::ZERO), &s);
        }
        assert!(p.offset.x < -0.1, "{p:?}");
        assert!(p.offset.x >= -s.horizontal_max - 1e-4);
        for _ in 0..120 {
            p = a.update_with(aimed(h, 0.0, Vec2::ZERO), &s);
        }
        assert!(p.offset.x.abs() < 1e-3, "{p:?}");
    }

    #[test]
    fn looking_up_sends_the_gun_down() {
        let s = AdsSwaySettings::default();
        let mut a = AdsSway::default();
        a.update_with(aimed(0.0, 0.0, Vec2::ZERO), &s);
        let mut pitch = 0.0;
        let mut p = crate::view_sway::SwayPose::NONE;
        for _ in 0..15 {
            pitch += 1.5 / 60.0;
            p = a.update_with(aimed(0.0, pitch, Vec2::ZERO), &s);
        }
        assert!(p.offset.z < -0.1, "{p:?}");
    }

    #[test]
    fn not_aimed_shows_nothing() {
        let s = AdsSwaySettings::default();
        let mut a = AdsSway::default();
        let mut h = 0.0;
        for _ in 0..30 {
            h += 0.05;
            let p = a.update_with(AdsInput { share: 0.0, ..aimed(h, 0.0, Vec2::new(150.0, 0.0)) }, &s);
            assert_eq!(p, crate::view_sway::SwayPose::NONE);
        }
    }

    #[test]
    fn the_settle_reaches_its_peak_and_ends_still() {
        for c in [&STOP_FORWARD, &STOP_UP, &STOP_ROLL_DEG, &STOP_PITCH_DEG] {
            let most = (0..=1000).map(|i| sample(c, i as f32 / 1000.0 * STOP_LENGTH).abs()).fold(0.0, f32::max);
            assert!((most - c.peak.abs()).abs() < 0.01 * c.peak.abs(), "{most} vs {}", c.peak);
            assert!(sample(c, STOP_LENGTH).abs() < 1e-4 && sample(c, STOP_LENGTH + 0.1) == 0.0);
        }
    }

    #[test]
    fn stopping_plays_the_settle_once() {
        let s = AdsSwaySettings::default();
        let mut a = AdsSway::default();
        for _ in 0..60 {
            a.update_with(aimed(0.0, 0.0, Vec2::new(0.0, 100.0)), &s);
        }
        let mut peak: f32 = 0.0;
        for _ in 0..90 {
            let p = a.update_with(aimed(0.0, 0.0, Vec2::ZERO), &s);
            peak = peak.max(p.rotation.angle_between(Quat::IDENTITY));
        }
        assert!(peak > 0.005, "the settle turned the gun: {peak}");
        assert!(!a.playing);
    }

    #[test]
    fn the_ini_is_read_and_clamped() {
        let s = AdsSwaySettings::from_ini_text("[Inertia]\nYawStrength = 1.5 ; note\nHorizontalMaxOffset=99\n[General]\nEnabled=0\n");
        assert_eq!(s.yaw_strength, 1.5);
        assert_eq!(s.horizontal_max, 10.0);
        assert!(!s.enabled);
        let d = AdsSwaySettings::from_ini_text(DEFAULT_INI);
        let def = AdsSwaySettings::default();
        assert_eq!((d.yaw_strength, d.vertical_max, d.full_speed, d.stop_speed), (def.yaw_strength, def.vertical_max, def.full_speed, def.stop_speed));
    }
}
