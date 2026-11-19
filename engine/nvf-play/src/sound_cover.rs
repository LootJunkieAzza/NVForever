//! Sound cover while sneaking (#2688, owner request 2026-11-15) [design: not the
//! original's]: the sounds of the world near the player hide the noise the player makes.
//! When the player's noise (`fnv_game::detection::Settings::noise`) is under the loudest
//! sound of the world heard at the player, no actor's detection of the player counts its
//! sound term (`fnv_game::detection::masked`, `Situation::sound_cover`).
//!
//! - **What is heard.** Every sound played at a place records here how loud it is where
//!   the listener (the player's eye) is: its gain as [`crate::sfx`] works it out, the
//!   record's static attenuation times its attenuation curve at that distance, without the
//!   effects volume (a quieter mix must not make the player louder). The modules that note
//!   them: [`crate::sfx`]'s one-shots (other actors' footsteps, shots, impacts,
//!   creatures' cries, doors, region sounds about the listener), for the seconds they
//!   last; [`crate::ambient`]'s placed loops (sound markers, objects' and creatures' loops,
//!   [`crate::sfx::SoundSource`]), every frame they sound; [`crate::voice`]'s lines said
//!   at a place, by the voice's own fall-off, for the line's length; [`crate::radio`]'s
//!   placed radios, every frame they play.
//! - **Not the player's own.** Sounds at the listener (the player's footsteps, shots,
//!   interface, the background loops, 2D region sounds) are not placed and never count; a
//!   placed sound within [`OWN_REACH`] of the listener is taken as the player's own (an
//!   effect's loop on the player, the player's weapon in third person) and does not count
//!   either.
//! - **The scale.** The loudest gain heard, 0–1, times [`COVER_FULL`] is the cover in the
//!   noise's units: a sound heard whole covers a run in heavy armour; one at a third of
//!   its full loudness, a walk in light armour (about 31) [design: the noise gauge's own
//!   scale, `crate::sneak_gauges::NOISE_FULL`, so the gauge's mark reads against it].
//! - The Gameplay switch `sound-cover` (on by default, kept in the INI) turns it off: the
//!   cover is then 0 and detection is the original's.
//!
//! Sounds switched off (`--no-sound`) play nothing and so cover nothing. A `sound cover
//! summary` line at the end.

use crate::settings::{add_setting, Setting};
use bevy::prelude::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// The cover, in the noise's units, of a sound heard at its full loudness [design].
pub const COVER_FULL: f32 = 100.0;
/// A placed sound this near the listener (game units) is taken as the player's own [design:
/// about the reach of an arm; an actor stands farther off].
pub const OWN_REACH: f32 = 64.0;
/// How long a loop's note holds without a new one: loops are noted every frame they sound.
pub const LOOP_HOLD: Duration = Duration::from_millis(500);
/// One-shots kept at most; the oldest go first [engineering choice: a battle's shots].
const MAX_ONE_SHOTS: usize = 256;

/// The sounds heard at the listener (see the module notes).
#[derive(Resource, Debug)]
pub struct SoundCover {
    /// The setting.
    pub on: bool,
    /// One-shots: gain at the listener, heard until.
    one_shots: Vec<(f32, Instant)>,
    /// Loops and lines, by key ([`SoundCover::key`]): gain at the listener, heard until.
    loops: HashMap<u64, (f32, Instant)>,
    /// Notes taken, those left out as the player's own, the most cover measured.
    notes: u64,
    own: u64,
    most: f32,
    measured: u64,
    covered: u64,
}

impl Default for SoundCover {
    fn default() -> SoundCover {
        SoundCover { on: true, one_shots: Vec::new(), loops: HashMap::new(), notes: 0, own: 0, most: 0.0, measured: 0, covered: 0 }
    }
}

/// The kinds of keyed notes, so the modules' own keys do not meet.
pub const AMBIENT_KIND: u8 = 1;
pub const VOICE_KIND: u8 = 2;
pub const RADIO_KIND: u8 = 3;

impl SoundCover {
    /// A key from a kind and the module's own key.
    pub fn key(kind: u8, key: u64) -> u64 {
        (u64::from(kind) << 56) ^ key
    }

    /// A sound heard at the listener at `gain` (no effects volume in it), `distance` units
    /// away, for `hold` from `now`: keyed (a loop, a line: the last note of a key stands)
    /// or not (a one-shot). Too near is the player's own and not kept.
    pub fn note(&mut self, key: Option<u64>, gain: f32, distance: f32, hold: Duration, now: Instant) {
        if !gain.is_finite() || gain <= 0.0 || !distance.is_finite() {
            return;
        }
        self.notes += 1;
        if distance < OWN_REACH {
            self.own += 1;
            return;
        }
        let until = now + hold;
        match key {
            Some(k) => {
                self.loops.insert(k, (gain, until));
            }
            None => {
                if self.one_shots.len() >= MAX_ONE_SHOTS {
                    self.one_shots.retain(|&(_, t)| t > now);
                    if self.one_shots.len() >= MAX_ONE_SHOTS {
                        self.one_shots.remove(0);
                    }
                }
                self.one_shots.push((gain, until));
            }
        }
    }

    /// The loudest gain still heard at `now`; the notes no longer heard are let go.
    pub fn loudest(&mut self, now: Instant) -> f32 {
        self.one_shots.retain(|&(_, t)| t > now);
        self.loops.retain(|_, &mut (_, t)| t > now);
        self.one_shots.iter().chain(self.loops.values()).map(|&(g, _)| g).fold(0.0, f32::max)
    }

    /// The cover at `now`, in the noise's units (0 with the switch off), and the
    /// player's `noise` measured against it for the summary.
    pub fn cover(&mut self, now: Instant, noise: f64) -> f32 {
        let c = if self.on { self.loudest(now).min(1.0) * COVER_FULL } else { 0.0 };
        self.measured += 1;
        self.covered += u64::from(fnv_game::detection::masked(noise, f64::from(c)));
        self.most = self.most.max(c);
        c
    }
}

pub struct SoundCoverPlugin;

impl Plugin for SoundCoverPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SoundCover>().add_systems(Last, (take_one_shots, report));
        add_setting(
            app,
            Setting {
                key: "sound-cover",
                group: crate::pause::GROUP_MENUS,
                label: "Sounds of the world cover your noise",
                note: "On: while a louder sound plays near you (machines, voices, shots, creatures), the noise you make is not heard. Not in the original.",
                switches: "",
                get: |w| w.resource::<SoundCover>().on,
                set: |w, on| w.resource_mut::<SoundCover>().on = on,
                slider: None,
            },
        );
    }
}

/// The one-shots [`crate::sfx`] played at a place since the last frame.
fn take_one_shots(sfx: Option<ResMut<crate::sfx::Sfx>>, mut cover: ResMut<SoundCover>) {
    let Some(mut sfx) = sfx else { return };
    if sfx.cover_notes.is_empty() {
        return;
    }
    let now = Instant::now();
    for (gain, distance, seconds) in std::mem::take(&mut sfx.cover_notes) {
        cover.note(None, gain, distance, Duration::from_secs_f32(seconds.clamp(0.05, 30.0)), now);
    }
}

fn report(mut exit: MessageReader<AppExit>, cover: Res<SoundCover>) {
    if exit.read().next().is_none() {
        return;
    }
    info!(
        "sound cover summary: on {}, {} sounds noted, {} as the player's own, {} measurements, {} with the player's noise covered, most cover {:.0}",
        cover.on, cover.notes, cover.own, cover.measured, cover.covered, cover.most
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_loudest_sound_still_heard_covers_and_the_players_own_does_not() {
        let mut c = SoundCover::default();
        let t = Instant::now();
        c.note(None, 0.3, 500.0, Duration::from_secs(1), t);
        c.note(Some(SoundCover::key(AMBIENT_KIND, 7)), 0.2, 300.0, LOOP_HOLD, t);
        c.note(None, 0.9, 20.0, Duration::from_secs(1), t);
        assert!((c.cover(t, 25.0) - 30.0).abs() < 1e-4, "the one-shot, not the player's own");
        assert_eq!(c.covered, 1);
        // The one-shot ends; the loop holds while it is noted again.
        let later = t + Duration::from_millis(1200);
        c.note(Some(SoundCover::key(AMBIENT_KIND, 7)), 0.25, 280.0, LOOP_HOLD, later);
        assert!((c.cover(later, 0.0) - 25.0).abs() < 1e-4);
        assert_eq!(c.covered, 1, "silence is not covered");
        // Not noted again: gone.
        assert_eq!(c.cover(later + Duration::from_secs(1), 0.0), 0.0);
        c.on = false;
        c.note(None, 1.0, 500.0, Duration::from_secs(1), later);
        assert_eq!(c.cover(later, 10.0), 0.0, "switched off");
        assert!((c.most - 30.0).abs() < 1e-4);
    }

    #[test]
    fn one_shots_are_bounded() {
        let mut c = SoundCover::default();
        let t = Instant::now();
        for i in 0..(MAX_ONE_SHOTS + 10) {
            c.note(None, 0.001 * i as f32 + 0.001, 500.0, Duration::from_secs(5), t);
        }
        assert_eq!(c.one_shots.len(), MAX_ONE_SHOTS);
        assert!(c.loudest(t) > 0.26);
    }
}
