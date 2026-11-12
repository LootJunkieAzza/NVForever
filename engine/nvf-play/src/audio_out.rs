//! The sound output: NVForever's stand-in for Bevy's `AudioPlugin` output, so sound
//! follows the audio device (engine behaviour, not a parity target: the original game's
//! behaviour on a device change is not documented here).
//!
//! **Why not Bevy's.** `bevy_audio` 0.19 opens the default device once at startup
//! (`AudioOutput`, `DeviceSinkBuilder::open_default_sink`) and keeps it for the whole run;
//! the resource is crate-private, so nothing outside can reopen it, and if no device
//! exists at startup the run stays silent [observed: bevy_audio-0.19.1
//! src/audio_output.rs]. cpal 0.17's WASAPI stream does not follow the default device
//! either: `default_output_device` resolves the endpoint once
//! (`GetDefaultAudioEndpoint`), the stream's client is activated on that endpoint, no
//! `IMMNotificationClient` is registered, and when the endpoint goes away the render
//! loop reports `DeviceNotAvailable` (`AUDCLNT_E_DEVICE_INVALIDATED`) to the error
//! callback and stops [observed: cpal-0.17.3 src/host/wasapi/device.rs `default_device`,
//! stream.rs `process_output`]. So after Windows switches the default output the sound
//! stays on the old device, and after an unplug it goes silent [hypothesis for the
//! unplug: read from the source, not watched on every driver].
//!
//! **How it works.**
//! - Every sound plays into one mixer that lives for the whole run ([`AudioOut::mixer`],
//!   48 kHz stereo; rodio converts each sound to it and the device's mixer converts it
//!   to the device). The device only taps that mixer, so dropping a device and opening
//!   another loses nothing: music, loops, radio, voice and one-shots carry on from where
//!   they were, the components (`AudioPlayer`, `AudioSink`, `PlaybackSettings`) are
//!   Bevy's own, and the call sites only swap `bevy::audio::AddAudioSource` for
//!   [`AddAudioSource`] here.
//! - A watcher thread owns the device. It reopens on: the stream's error callback
//!   (device gone or the stream invalidated; underruns are ignored), the default output
//!   device's id changing (polled every [`POLL`]: one `GetDefaultAudioEndpoint` call,
//!   cheaper and simpler than a COM `IMMNotificationClient`, works on any cpal host, and
//!   1 s is inside the ~2 s the owner allowed), and the test aid. Opening tries the
//!   default device, then any other output device.
//! - With no device (none at startup, or all unplugged) the watcher drains the mixer at
//!   the real-time rate into nothing, so sounds still end on time (one-shots despawn,
//!   voice lines finish, music moves on), and it tries to open a device every [`POLL`].
//! - **Volumes by category** (owner request 2026-10-17, the original's audio menu:
//!   Master, Music, Footstep, Voice, Effects, Radio [observed: the owner's photo of the
//!   game's settings]). Each [`SoundCategory`] has a mixer of its own that plays into the
//!   run-long one at its volume times the master volume ([`SoundVolumes`]), so a change
//!   reaches the sounds already playing, eased over [`VOLUME_EASE`] so a slider does not
//!   click. A sound's category is the [`SoundCategory`] component of its entity, Effects
//!   without one. Music (the music and the main menu's theme), voice (dialogue lines) and
//!   radio tag their sounds; effects whose SOUN editor ID starts with `FST` (footsteps,
//!   jumps, landings, armour steps) are Footstep [hypothesis: which sounds the original
//!   counts as footsteps; the names are docs/spec-notes/audio.md §10]; everything else
//!   (shots, impacts, interface, placed and background sounds, the intro) is Effects
//!   [hypothesis]. A volume scales the samples linearly [hypothesis: the original's curve
//!   is not known; at its default Music 60 its main menu theme plays at 0.61 of the file,
//!   which a linear 0.6 matches; docs/spec-notes/audio.md]. They start at the original's
//!   defaults ([`DEFAULT_VOLUMES`]; issue #1450: all at 1 made music, voices and radio
//!   louder than the original). The settings page's Sound tab sets them
//!   ([`register_settings`]), kept in the INI like every row: an INI written before keeps
//!   its volumes, and the tab's Defaults goes back to these.
//! - **Slowed sound** (owner 2026-10-18: in slow motion sounds should be "more drawn out
//!   and lower pitch like the base game" [observed: the owner's ear, the original's kill
//!   camera and VATS]). [`SoundSpeed`] is the speed the engine's own slow motion (VATS
//!   mode, the death camera) asks sounds to play at; every playing sound of a slowed
//!   category follows it at once, as tape played slower: longer and lower together.
//!   Effects, footsteps and voices slow; music and radio do not, nor interface sounds
//!   ([`KeepsSpeed`]) [hypothesis: what the original leaves at speed]. A script's
//!   SetGlobalTimeMultiplier changes the same sounds by its multiplier unless it asked
//!   for sound unchanged (`script::time_multiplier`).
//! - Spatial `PlaybackSettings` (Bevy's `SpatialAudioSink`) are not built: NVForever pans
//!   its own sounds (`sfx::Panned`); a spatial request plays plain, with a warning.
//! - **The mix bus** ([`MixBus`], #2217) [design]: the run-long mixer on its way to the
//!   device (or to the drain without one) goes through one bus, kept with its state for
//!   the whole run (a reopened device taps the same bus). It measures the summed mix: the
//!   loudest sample and the samples over full scale (`abs(sample) > 1`, each channel
//!   counted apart; exactly ±1 is not over), before and after the output's protection.
//!   The protection (a limiter) is not built yet: the bus passes the mix as it is, so the
//!   two measurements are the same and no frame is limited. Each pull measures its block
//!   locally and publishes the totals once; the audio thread takes no other lock and
//!   allocates nothing for it. Whether the original game limits or clips its mix is not
//!   known.
//! - **Stopping a playing sound** ([`stop_sound`], #2217): a sound stopped before its end
//!   (a voice taken over at the voice limit, a loop or a script's sound stopped, an
//!   ambient loop, a radio or a voice line no longer wanted) goes through one call, which
//!   counts why ([`SoundStops`]). Today it is removed at once, mid-waveform (a hard stop);
//!   one not attached to the output yet, or already over, is removed without a count.
//!
//! Log lines: `audio output: opened <device> (<rate> Hz, <channels> ch)`, `audio output:
//! reopening (<why>)`, `audio output: no device (<why>); sound runs silent until one
//! appears`, and at the end `audio output summary: opened N times, M reopens, X s mixed,
//! Y s since the last open`, then `audio categories: music X s, footstep X s, voice X s,
//! effects X s, radio X s heard; volumes master N%, music N%, …` (the seconds in which a
//! category's mixer had sound, before its volume), then `audio categories after volumes:
//! music X s, … audible` (the same after its volume and the master's: what a slider
//! turned down leaves to be heard; #1077), then `audio mix: pre peak P, N samples over
//! full scale; post peak Q, M samples over full scale; K frames limited` (the mix bus's
//! measurements, peaks to nine decimals). The stop counts are told by `crate::sfx`'s
//! report (`sfx hard stops: …`).
//!
//! Test aid (unattended runs only; no effect unless set):
//! `NVF_PLAY_TEST_AUDIO_REOPEN=<seconds>[,...]` drops the device and opens the default
//! one again at each time, as a switch of device would.

use std::num::NonZero;
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use bevy::asset::Asset;
use bevy::audio::{
    AudioLoader, AudioPlayer, AudioSink, AudioSinkPlayback, AudioSource, Decodable, GlobalVolume, PlaybackMode,
    PlaybackSettings, Volume,
};
use bevy::prelude::*;
use rodio::cpal::traits::{DeviceTrait, HostTrait};
use rodio::mixer::{Mixer, MixerSource};
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player, Source};

/// How often the watcher looks at the default device (and, with none open, tries to open one).
pub const POLL: Duration = Duration::from_secs(1);
/// The run-long mixer's format.
const RATE: u32 = 48_000;
const CHANNELS: u16 = 2;
/// Samples the device tap takes from the mixer per lock (an even number keeps the channels
/// in step).
const BLOCK: usize = 512;
/// How often the watcher drains the mixer while there is no device.
const DRAIN_STEP: Duration = Duration::from_millis(10);
/// How long a change of volume takes to reach its new level.
pub const VOLUME_EASE: Duration = Duration::from_millis(20);
/// A sample louder than this counts as sound for the categories' summary.
const HEARD: f32 = 1e-4;
/// The settings page's tab of volumes.
pub const GROUP_SOUND: &str = "Sound";

/// What kind of sound an entity's `AudioPlayer` is, for its volume (module notes);
/// Effects without the component.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SoundCategory {
    Music,
    Footstep,
    Voice,
    #[default]
    Effects,
    Radio,
}

impl SoundCategory {
    /// In the order of the original's audio menu.
    pub const ALL: [SoundCategory; 5] = [SoundCategory::Music, SoundCategory::Footstep, SoundCategory::Voice, SoundCategory::Effects, SoundCategory::Radio];

    fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            SoundCategory::Music => "music",
            SoundCategory::Footstep => "footstep",
            SoundCategory::Voice => "voice",
            SoundCategory::Effects => "effects",
            SoundCategory::Radio => "radio",
        }
    }

    /// Whether the category's sounds slow with [`SoundSpeed`] (module notes).
    pub fn slows(self) -> bool {
        !matches!(self, SoundCategory::Music | SoundCategory::Radio)
    }

    /// The category of a sound effect by its SOUN editor ID (module notes).
    pub fn of_effect(edid: Option<&str>) -> SoundCategory {
        match edid.and_then(|e| e.get(..3)) {
            Some(p) if p.eq_ignore_ascii_case("FST") => SoundCategory::Footstep,
            _ => SoundCategory::Effects,
        }
    }
}

/// The speed the engine's slow motion asks sounds to play at, 1 for none (module notes);
/// a sound plays at its own speed times this.
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct SoundSpeed(pub f32);

impl Default for SoundSpeed {
    fn default() -> Self {
        SoundSpeed(1.0)
    }
}

/// The slowest slowed sound [design: below about 0.4 the shots and voices turn to a
/// drone nobody can make out].
pub const SOUND_FLOOR: f32 = 0.4;

/// The sound speed for the engine's slow motion at clock speed `clock`: its square root,
/// no lower than [`SOUND_FLOOR`] [design: sound stays nearer its speed than the picture;
/// the death camera's 0.25 plays sound at 0.5, an octave down, VATS firing's 0.5 at 0.71
/// and VATS marking's 0.05 at the floor].
pub fn slowed_sound(clock: f32) -> f32 {
    clock.clamp(0.0, 1.0).sqrt().max(SOUND_FLOOR)
}

/// A sound that never slows with [`SoundSpeed`]: the interface's.
#[derive(Component, Clone, Copy, Debug, Default)]
pub struct KeepsSpeed;

/// The volumes, each a linear part from 0 to 1: the master's (all sound) and each
/// category's. Shared with the mixing, so a change is heard at once.
#[derive(Resource, Clone)]
pub struct SoundVolumes(Arc<Levels>);

struct Levels {
    master: AtomicU32,
    categories: [AtomicU32; 5],
    /// Stereo frames in which each category's mixer had sound.
    heard: [AtomicU64; 5],
    /// The same, after the category's volume and the master's.
    audible: [AtomicU64; 5],
}

/// The volumes a new player starts with, in [`SoundCategory::ALL`]'s order: the
/// original's (`fDefaultMusicVolume` 0.6, `fDefaultFootVolume` 0.5, `fDefaultVoiceVolume`
/// 0.6, `fDefaultEffectsVolume` 1, `fDefaultRadioVolume` 0.5) [observed: Fallout_default.ini,
/// and the original's sliders read with GetGameVolume on an install left at its defaults:
/// 60, 50, 60, 100, 50]; master 1 (`fDefaultMasterVolume`).
pub const DEFAULT_VOLUMES: [f32; 5] = [0.6, 0.5, 0.6, 1.0, 0.5];

impl Default for SoundVolumes {
    fn default() -> Self {
        let level = |v: f32| AtomicU32::new(v.to_bits());
        SoundVolumes(Arc::new(Levels {
            master: level(1.0),
            categories: DEFAULT_VOLUMES.map(level),
            heard: Default::default(),
            audible: Default::default(),
        }))
    }
}

impl SoundVolumes {
    pub fn master(&self) -> f32 {
        f32::from_bits(self.0.master.load(Ordering::Relaxed))
    }

    pub fn set_master(&self, v: f32) {
        self.0.master.store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self, c: SoundCategory) -> f32 {
        f32::from_bits(self.0.categories[c.index()].load(Ordering::Relaxed))
    }

    pub fn set(&self, c: SoundCategory, v: f32) {
        self.0.categories[c.index()].store(v.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    /// What a category's sound is played at: its volume times the master's.
    pub fn gain(&self, c: SoundCategory) -> f32 {
        self.master() * self.get(c)
    }
}

/// A category's mixer as the run-long mixer hears it: at its [`SoundVolumes::gain`],
/// eased to a new one, never ending (silence when nothing plays).
struct Gain {
    inner: MixerSource,
    volumes: SoundVolumes,
    category: SoundCategory,
    now: f32,
    /// Samples of the stereo frame being counted for the summary, and whether one had
    /// sound before and after the gain.
    counted: u16,
    heard: bool,
    audible: bool,
}

impl Gain {
    fn new(inner: MixerSource, volumes: SoundVolumes, category: SoundCategory) -> Gain {
        let now = volumes.gain(category);
        Gain { inner, volumes, category, now, counted: 0, heard: false, audible: false }
    }
}

/// The most a gain moves per sample ([`VOLUME_EASE`] for the whole way).
fn ease_step() -> f32 {
    1.0 / (VOLUME_EASE.as_secs_f32() * RATE as f32 * CHANNELS as f32)
}

impl Iterator for Gain {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        let target = self.volumes.gain(self.category);
        let step = ease_step();
        self.now += (target - self.now).clamp(-step, step);
        let s = self.inner.next().unwrap_or(0.0);
        self.heard |= s.abs() > HEARD;
        self.audible |= (s * self.now).abs() > HEARD;
        self.counted += 1;
        if self.counted == CHANNELS {
            if self.heard {
                self.volumes.0.heard[self.category.index()].fetch_add(1, Ordering::Relaxed);
            }
            if self.audible {
                self.volumes.0.audible[self.category.index()].fetch_add(1, Ordering::Relaxed);
            }
            self.counted = 0;
            self.heard = false;
            self.audible = false;
        }
        Some(s * self.now)
    }
}

impl Source for Gain {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(CHANNELS).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// A playing sound that ends, instead of taking the audio output thread down with it, when
/// its decoder panics on a broken part of its file (nvf_assets::guard); every sound
/// played goes through one. A sample that is not a number (a garbled float file, a gain
/// from a broken place) is silence, so it cannot spread through the mix and the reverb.
struct GuardedSource<S> {
    inner: S,
    dead: bool,
}

impl<S: Source> GuardedSource<S> {
    fn new(inner: S) -> Self {
        GuardedSource { inner, dead: false }
    }
}

impl<S: Source> Iterator for GuardedSource<S> {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        if self.dead {
            return None;
        }
        match nvf_assets::guard::guarded(|| Ok(self.inner.next())) {
            Ok(s) => s.map(|x| if x.is_finite() { x } else { 0.0 }),
            Err(e) => {
                warn!("audio output: a sound's file broke while playing ({e}); it stops");
                self.dead = true;
                None
            }
        }
    }
}

impl<S: Source> Source for GuardedSource<S> {
    fn current_span_len(&self) -> Option<usize> {
        if self.dead {
            Some(0)
        } else {
            self.inner.current_span_len()
        }
    }
    fn channels(&self) -> rodio::ChannelCount {
        self.inner.channels()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        self.inner.sample_rate()
    }
    fn total_duration(&self) -> Option<Duration> {
        self.inner.total_duration()
    }
    fn try_seek(&mut self, pos: Duration) -> Result<(), rodio::source::SeekError> {
        self.inner.try_seek(pos)
    }
}

/// The settings page's volumes: the master volume and each category's, as the original's
/// audio menu lists them, registered in the Sound group and shown on the Audio tab
/// (`crate::settings_layout`, #1953). Called by the binary after the picture's rows.
pub fn register_settings(app: &mut App) {
    use crate::settings::{add_setting, Setting, Slider};
    if !app.world().contains_resource::<SoundVolumes>() {
        return;
    }
    let slider = |get: fn(&World) -> f32, set: fn(&mut World, f32)| Slider { get, set, min: 0.0, step: 0.05, shown: None };
    add_setting(
        app,
        Setting::slider(
            "volume-master",
            GROUP_SOUND,
            "Master",
            "How loud all sound is: each volume below is a part of this one.",
            slider(|w| w.resource::<SoundVolumes>().master(), |w, v| w.resource::<SoundVolumes>().set_master(v)),
        ),
    );
    // Key, label, note, get, set.
    type Row = (&'static str, &'static str, &'static str, fn(&World) -> f32, fn(&mut World, f32));
    let rows: [Row; 5] = [
        ("volume-music", "Music", "The music of the place you are in and the main menu's theme.", |w| w.resource::<SoundVolumes>().get(SoundCategory::Music), |w, v| w.resource::<SoundVolumes>().set(SoundCategory::Music, v)),
        ("volume-footstep", "Footstep", "Footsteps, jumps and landings, yours and everyone's.", |w| w.resource::<SoundVolumes>().get(SoundCategory::Footstep), |w, v| w.resource::<SoundVolumes>().set(SoundCategory::Footstep, v)),
        ("volume-voice", "Voice", "Spoken lines: conversations and what people say around you.", |w| w.resource::<SoundVolumes>().get(SoundCategory::Voice), |w, v| w.resource::<SoundVolumes>().set(SoundCategory::Voice, v)),
        ("volume-effects", "Effects", "Every other sound: weapons, impacts, menus, doors and the sounds of places.", |w| w.resource::<SoundVolumes>().get(SoundCategory::Effects), |w, v| w.resource::<SoundVolumes>().set(SoundCategory::Effects, v)),
        ("volume-radio", "Radio", "Radio stations, from the Pip-Boy and from radios in the world.", |w| w.resource::<SoundVolumes>().get(SoundCategory::Radio), |w, v| w.resource::<SoundVolumes>().set(SoundCategory::Radio, v)),
    ];
    for (key, label, note, get, set) in rows {
        add_setting(app, Setting::slider(key, GROUP_SOUND, label, note, slider(get, set)));
    }
    if app.world().contains_resource::<crate::sfx::SfxSettings>() {
        add_setting(
            app,
            Setting {
                key: "reverb",
                group: GROUP_SOUND,
                label: "Reverb",
                note: "World sounds echo as the place you are in does (a cave, a hall, a small room). Off: --no-reverb.",
                get: |w| w.resource::<crate::sfx::SfxSettings>().reverb,
                set: |w, on| w.resource_mut::<crate::sfx::SfxSettings>().reverb = on,
                slider: None,
            },
        );
    }
    if app.world().contains_resource::<crate::voice::VoiceSettings>() {
        add_setting(
            app,
            Setting {
                key: "voice-reverb",
                group: GROUP_SOUND,
                label: "Voice reverb",
                note: "What people say around you echoes in the place you are in, up to the end of each line, as Reverb does for world sounds. Conversations stay dry.",
                get: |w| w.resource::<crate::voice::VoiceSettings>().reverb,
                set: |w, on| w.resource_mut::<crate::voice::VoiceSettings>().reverb = on,
                slider: None,
            },
        );
    }
}

/// The run-long mixer every sound plays into, and the line to the watcher thread.
#[derive(Resource)]
pub struct AudioOut {
    mixer: Mixer,
    /// Each category's mixer, in [`SoundCategory::ALL`]'s order, playing into `mixer`.
    categories: [Mixer; 5],
    watcher: Sender<Command>,
    stats: Arc<Stats>,
}

impl AudioOut {
    /// The mixer sounds play into (it outlives any device).
    pub fn mixer(&self) -> &Mixer {
        &self.mixer
    }

    /// The mixer a category's sounds play into.
    pub fn category_mixer(&self, c: SoundCategory) -> &Mixer {
        &self.categories[c.index()]
    }

    /// Drops the device and opens the default one again (`why` goes to the log).
    pub fn reopen(&self, why: &'static str) {
        let _ = self.watcher.send(Command::Reopen(why));
    }
}

enum Command {
    Reopen(&'static str),
    /// The open stream reported an error (sent from cpal's thread).
    StreamFailed(String),
}

#[derive(Default)]
struct Stats {
    /// Stereo frames taken from the mixer, by a device or the drain.
    frames: AtomicU64,
    opens: AtomicU32,
    reopens: AtomicU32,
    /// `frames` when the last device opened.
    frames_at_open: AtomicU64,
    /// The mix bus's measurements ([`Meter`]), summed over the run: the peaks as the bits
    /// of a non-negative `f32` (whose order is the numbers' order, so `fetch_max` keeps
    /// the loudest), the samples over full scale and the frames limited.
    pre_peak: AtomicU32,
    pre_over: AtomicU64,
    post_peak: AtomicU32,
    post_over: AtomicU64,
    limited: AtomicU64,
}

impl Stats {
    /// Adds a pull's measurements to the run's.
    fn publish(&self, m: &Meter) {
        self.pre_peak.fetch_max(m.pre_peak.to_bits(), Ordering::Relaxed);
        self.post_peak.fetch_max(m.post_peak.to_bits(), Ordering::Relaxed);
        self.pre_over.fetch_add(m.pre_over, Ordering::Relaxed);
        self.post_over.fetch_add(m.post_over, Ordering::Relaxed);
        self.limited.fetch_add(m.limited, Ordering::Relaxed);
    }

    /// The run's measurements so far.
    fn mix(&self) -> Meter {
        Meter {
            pre_peak: f32::from_bits(self.pre_peak.load(Ordering::Relaxed)),
            pre_over: self.pre_over.load(Ordering::Relaxed),
            post_peak: f32::from_bits(self.post_peak.load(Ordering::Relaxed)),
            post_over: self.post_over.load(Ordering::Relaxed),
            limited: self.limited.load(Ordering::Relaxed),
        }
    }
}

/// What the mix bus measured over some samples: the loudest sample and the samples over
/// full scale (`abs > 1`) before the output's protection (`pre`) and after it (`post`),
/// and the stereo frames the protection turned down.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
struct Meter {
    pre_peak: f32,
    pre_over: u64,
    post_peak: f32,
    post_over: u64,
    limited: u64,
}

/// Counts one sample into a peak and an over-full-scale count (a sample that is not a
/// number counts as neither).
fn measure(peak: &mut f32, over: &mut u64, s: f32) {
    let a = s.abs();
    if a > *peak {
        *peak = a;
    }
    if a > 1.0 {
        *over += 1;
    }
}

/// The run-long mix on its way to the output (module notes): the mixer every category
/// plays into, measured before and after the output's protection. The limiter (#2217
/// stage 4) will keep its state here, so a reopened device goes on with it; it is not
/// built yet and the bus passes the mix unchanged.
pub(crate) struct MixBus {
    source: MixerSource,
}

impl MixBus {
    fn new(source: MixerSource) -> MixBus {
        MixBus { source }
    }

    /// `n` samples (whole frames) of the mix as the output gets them, each to `each`,
    /// measured into `meter`; silence where nothing plays.
    fn fill(&mut self, n: usize, meter: &mut Meter, mut each: impl FnMut(f32)) {
        for _ in 0..n {
            let pre = self.source.next().unwrap_or(0.0);
            measure(&mut meter.pre_peak, &mut meter.pre_over, pre);
            // The output's protection (the limiter) goes here; until it is built the
            // output is the mix as it is.
            let post = pre;
            measure(&mut meter.post_peak, &mut meter.post_over, post);
            each(post);
        }
    }
}

/// Why a playing sound was stopped before its end ([`stop_sound`]), as the summary lines
/// name them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StopReason {
    /// A one-shot taken over at its SOUN's voice limit (`crate::sfx::MAX_SAME`).
    Steal,
    /// A LoopSound stopped or started again.
    Loop,
    /// A script's StopSound (without a fade).
    Script,
    /// An ambient loop no longer heard, or of a place left.
    Ambient,
    /// A radio's track replaced, a receiver no longer heard, or of a place left.
    Radio,
    /// A voice line cut or replaced.
    Voice,
}

impl StopReason {
    pub const ALL: [StopReason; 6] = [StopReason::Steal, StopReason::Loop, StopReason::Script, StopReason::Ambient, StopReason::Radio, StopReason::Voice];

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            StopReason::Steal => "steals",
            StopReason::Loop => "loop stops",
            StopReason::Script => "script stops",
            StopReason::Ambient => "ambient stops",
            StopReason::Radio => "radio stops",
            StopReason::Voice => "voice stops",
        }
    }
}

/// The playing sounds stopped before their end, by [`StopReason`]: removed at once,
/// mid-waveform (`hard`), or faded out (`declicked`, and of those the fades that ended,
/// `completed`). One not attached to the output yet, or already over, counts as neither.
#[derive(Resource, Clone, Debug, Default, PartialEq)]
pub struct SoundStops {
    pub hard: [u32; 6],
    pub declicked: [u32; 6],
    pub completed: u32,
}

impl SoundStops {
    /// `N steals, N loop stops, …` of `counts`.
    fn fields(counts: &[u32; 6]) -> String {
        StopReason::ALL.map(|r| format!("{} {}", counts[r.index()], r.name())).join(", ")
    }

    /// The `sfx hard stops: …` summary line.
    pub fn hard_line(&self) -> String {
        format!("sfx hard stops: {}", SoundStops::fields(&self.hard))
    }

    /// The `sfx declicks: …` summary line.
    pub fn declick_line(&self) -> String {
        let started: u32 = self.declicked.iter().sum();
        format!("sfx declicks: {}; {} completed, {} pending", SoundStops::fields(&self.declicked), self.completed, started.saturating_sub(self.completed))
    }
}

/// Stops the sound `entity` plays, before its end, for `why` (module notes): removed at
/// once, counted as a hard stop when it was attached to the output and still playing.
/// A sound asked for but not attached yet, or already over, is removed without a count.
pub fn stop_sound(commands: &mut Commands, entity: Entity, why: StopReason) {
    commands.entity(entity).queue_silenced(move |e: EntityWorldMut| stop_now(e, why));
}

fn stop_now(mut e: EntityWorldMut, why: StopReason) {
    let playing = e.get::<AudioSink>().is_some_and(|s| !s.empty());
    if playing {
        if let Some(mut stops) = e.get_resource_mut::<SoundStops>() {
            stops.hard[why.index()] += 1;
        }
    }
    e.despawn();
}

/// The output and the playing of every `AudioPlayer<T>` registered with [`AddAudioSource`].
pub struct AudioOutPlugin;

impl Plugin for AudioOutPlugin {
    fn build(&self, app: &mut App) {
        let new_mixer = || rodio::mixer::mixer(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap());
        let (mixer, source) = new_mixer();
        let volumes = SoundVolumes::default();
        let categories = SoundCategory::ALL.map(|c| {
            let (m, s) = new_mixer();
            mixer.add(Gain::new(s, volumes.clone(), c));
            m
        });
        let shared = Arc::new(Mutex::new(MixBus::new(source)));
        let stats = Arc::new(Stats::default());
        let (tx, rx) = channel();
        let thread_tx = tx.clone();
        let thread_stats = stats.clone();
        if let Err(e) = std::thread::Builder::new()
            .name("audio output".into())
            .spawn(move || watch(shared, rx, thread_tx, thread_stats))
        {
            warn!("audio output: no watcher thread ({e}); no sound");
        }
        app.insert_resource(AudioOut { mixer, categories, watcher: tx, stats })
            .insert_resource(volumes)
            .init_resource::<GlobalVolume>()
            .init_resource::<SoundSpeed>()
            .init_resource::<SoundStops>()
            .add_systems(Update, test_reopen)
            .add_systems(PostUpdate, follow_sound_speed.before(AudioOutSystems))
            .add_systems(Last, report)
            .add_audio_source::<AudioSource>()
            .init_asset_loader::<AudioLoader>();
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct AudioOutSystems;

/// A `PlaybackMode::Despawn` sound: despawned when it ends (Bevy's marker is crate-private).
#[derive(Component)]
struct DespawnWhenDone;

/// A `PlaybackMode::Remove` sound: its audio components removed when it ends.
#[derive(Component)]
struct RemoveWhenDone;

/// Registers a [`Decodable`] asset type so its `AudioPlayer<T>` entities play through
/// [`AudioOut`] (the replacement for `bevy::audio::AddAudioSource`).
pub trait AddAudioSource {
    fn add_audio_source<T: Decodable + Asset>(&mut self) -> &mut Self;
}

impl AddAudioSource for App {
    fn add_audio_source<T: Decodable + Asset>(&mut self) -> &mut Self {
        self.init_asset::<T>()
            .add_systems(PostUpdate, (play_queued::<T>, cleanup_finished::<T>).in_set(AudioOutSystems))
    }
}

/// Starts every `AudioPlayer<T>` without a sink whose asset has loaded (as Bevy's
/// `play_queued_audio_system`, plain sinks only).
#[allow(clippy::type_complexity)]
fn play_queued<T: Decodable + Asset>(
    out: Option<Res<AudioOut>>,
    sources: Res<Assets<T>>,
    global: Res<GlobalVolume>,
    slow: Res<SoundSpeed>,
    queued: Query<(Entity, &AudioPlayer<T>, &PlaybackSettings, Option<&SoundCategory>, Has<KeepsSpeed>), Without<AudioSink>>,
    mut warned: Local<bool>,
    mut commands: Commands,
) {
    let Some(out) = out else { return };
    for (entity, player, settings, category, keeps) in &queued {
        let Some(source) = sources.get(&player.0) else { continue };
        if settings.spatial && !*warned {
            *warned = true;
            warn!("audio output: spatial sound is not built; {entity} plays without it");
        }
        // A broken file's decoder that panics while opening is that sound's failure, not
        // the game's (nvf_assets::guard): the sound is dropped.
        let decoder = match nvf_assets::guard::guarded(|| Ok(source.decoder())) {
            Ok(d) => GuardedSource::new(d),
            Err(e) => {
                warn!("audio output: a sound could not be opened ({e}); it is skipped");
                commands.entity(entity).try_despawn();
                continue;
            }
        };
        let sink = Player::connect_new(out.category_mixer(category.copied().unwrap_or_default()));
        let looped = matches!(settings.mode, PlaybackMode::Loop);
        match (settings.start_position, settings.duration, looped) {
            (Some(s), Some(d), true) => sink.append(decoder.skip_duration(s).take_duration(d).repeat_infinite()),
            (Some(s), None, true) => sink.append(decoder.skip_duration(s).repeat_infinite()),
            (None, Some(d), true) => sink.append(decoder.take_duration(d).repeat_infinite()),
            (None, None, true) => sink.append(decoder.repeat_infinite()),
            (Some(s), Some(d), false) => sink.append(decoder.skip_duration(s).take_duration(d)),
            (Some(s), None, false) => sink.append(decoder.skip_duration(s)),
            (None, Some(d), false) => sink.append(decoder.take_duration(d)),
            (None, None, false) => sink.append(decoder),
        }
        let mut sink = AudioSink::new(sink);
        if settings.muted {
            sink.mute();
        }
        sink.set_speed(settings.speed * speed_factor(slow.0, category.copied().unwrap_or_default(), keeps));
        // A volume that is not a number would reach the mix after the guard: silence.
        sink.set_volume(Volume::Linear(Some(settings.volume.to_linear() * global.volume.to_linear()).filter(|v| v.is_finite()).unwrap_or(0.0)));
        if settings.paused {
            sink.pause();
        }
        let mut e = commands.entity(entity);
        match settings.mode {
            PlaybackMode::Loop | PlaybackMode::Once => e.insert(sink),
            PlaybackMode::Despawn => e.insert((sink, DespawnWhenDone)),
            PlaybackMode::Remove => e.insert((sink, RemoveWhenDone)),
        };
    }
}

/// What a sound's own speed is multiplied by at sound speed `slow`.
fn speed_factor(slow: f32, category: SoundCategory, keeps: bool) -> f32 {
    if keeps || !category.slows() {
        1.0
    } else {
        slow
    }
}

/// A change of [`SoundSpeed`] reaches every sound already playing (new ones start at it,
/// [`play_queued`]).
fn follow_sound_speed(
    slow: Res<SoundSpeed>,
    sinks: Query<(&AudioSink, &PlaybackSettings, Option<&SoundCategory>, Has<KeepsSpeed>)>,
    mut told: Local<Option<f32>>,
) {
    if !slow.is_changed() {
        return;
    }
    let before = told.replace(slow.0).unwrap_or(1.0);
    // Easing changes it every frame: the log tells only of slowing and of speed back
    // (below 0.995, so the speed told as slowed reads below 1.00).
    let slowed = |s: f32| s < 0.995;
    if slowed(before) != slowed(slow.0) {
        info!("sound speed: {before:.2} -> {:.2}", slow.0);
    }
    for (sink, settings, category, keeps) in &sinks {
        sink.set_speed(settings.speed * speed_factor(slow.0, category.copied().unwrap_or_default(), keeps));
    }
}

/// Despawns (or strips) finished `Despawn` (`Remove`) sounds, as Bevy's `cleanup_finished_audio`.
#[allow(clippy::type_complexity)]
fn cleanup_finished<T: Decodable + Asset>(
    mut commands: Commands,
    despawn: Query<(Entity, &AudioSink), (With<DespawnWhenDone>, With<AudioPlayer<T>>)>,
    remove: Query<(Entity, &AudioSink), (With<RemoveWhenDone>, With<AudioPlayer<T>>)>,
) {
    for (entity, sink) in &despawn {
        if sink.empty() {
            commands.entity(entity).try_despawn();
        }
    }
    for (entity, sink) in &remove {
        if sink.empty() {
            commands.entity(entity).remove::<(AudioPlayer<T>, AudioSink, PlaybackSettings, RemoveWhenDone)>();
        }
    }
}

/// The device's view of the run-long mixer: never ends (silence when nothing plays), so
/// the device mixer keeps it.
struct Tap {
    shared: Arc<Mutex<MixBus>>,
    stats: Arc<Stats>,
    buf: Vec<f32>,
    pos: usize,
}

impl Tap {
    fn new(shared: Arc<Mutex<MixBus>>, stats: Arc<Stats>) -> Tap {
        Tap { shared, stats, buf: Vec::with_capacity(BLOCK), pos: 0 }
    }
}

impl Iterator for Tap {
    type Item = rodio::Sample;

    fn next(&mut self) -> Option<rodio::Sample> {
        if self.pos == self.buf.len() {
            self.buf.clear();
            pull(&self.shared, &self.stats, BLOCK, |s| self.buf.push(s));
            self.pos = 0;
        }
        self.pos += 1;
        Some(self.buf[self.pos - 1])
    }
}

impl Source for Tap {
    fn current_span_len(&self) -> Option<usize> {
        None
    }
    fn channels(&self) -> rodio::ChannelCount {
        NonZero::new(CHANNELS).unwrap()
    }
    fn sample_rate(&self) -> rodio::SampleRate {
        NonZero::new(RATE).unwrap()
    }
    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

/// Takes `n` samples (even) from the mix bus, silence where nothing plays: the device's
/// tap and the drain without a device alike, so both are measured the same way. The
/// block's measurements are published once, after the lock is let go.
fn pull(shared: &Mutex<MixBus>, stats: &Stats, n: usize, each: impl FnMut(f32)) {
    let mut meter = Meter::default();
    shared.lock().unwrap_or_else(|e| e.into_inner()).fill(n, &mut meter, each);
    stats.frames.fetch_add((n / CHANNELS as usize) as u64, Ordering::Relaxed);
    stats.publish(&meter);
}

fn default_id() -> Option<String> {
    let device = rodio::cpal::default_host().default_output_device()?;
    device.id().ok().map(|id| id.to_string())
}

fn device_name(device: &rodio::cpal::Device) -> String {
    device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "an unnamed device".into())
}

/// Opens the default output device, else the first other one that opens, and taps the mixer.
fn open(shared: &Arc<Mutex<MixBus>>, tx: &Sender<Command>, stats: &Arc<Stats>) -> Result<MixerDeviceSink, String> {
    let host = rodio::cpal::default_host();
    let mut candidates: Vec<rodio::cpal::Device> = host.default_output_device().into_iter().collect();
    let default = candidates.first().and_then(|d| d.id().ok());
    if let Ok(all) = host.output_devices() {
        candidates.extend(all.filter(|d| d.id().ok() != default || default.is_none()));
    }
    let mut first_err = None;
    for device in candidates {
        let name = device_name(&device);
        let tx = tx.clone();
        let opened = DeviceSinkBuilder::from_device(device).and_then(|b| {
            b.with_error_callback(move |e: rodio::cpal::StreamError| {
                if !matches!(e, rodio::cpal::StreamError::BufferUnderrun) {
                    let _ = tx.send(Command::StreamFailed(e.to_string()));
                }
            })
            .open_stream()
        });
        match opened {
            Ok(mut sink) => {
                sink.log_on_drop(false);
                let config = sink.config();
                info!("audio output: opened {name} ({} Hz, {} ch)", config.sample_rate(), config.channel_count());
                sink.mixer().add(Tap::new(shared.clone(), stats.clone()));
                stats.opens.fetch_add(1, Ordering::Relaxed);
                stats.frames_at_open.store(stats.frames.load(Ordering::Relaxed), Ordering::Relaxed);
                return Ok(sink);
            }
            Err(e) => {
                first_err.get_or_insert(format!("{name}: {e}"));
            }
        }
    }
    Err(first_err.unwrap_or_else(|| "no output device".into()))
}

/// The watcher thread: owns the device, reopens it on a change, drains the mixer without one.
fn watch(shared: Arc<Mutex<MixBus>>, rx: std::sync::mpsc::Receiver<Command>, tx: Sender<Command>, stats: Arc<Stats>) {
    let mut device: Option<MixerDeviceSink> = None;
    let mut last_default = default_id();
    let mut next_poll = Instant::now();
    let mut last_drain = Instant::now();
    let mut reported_none = false;
    let mut reason: Option<String> = Some(String::new());
    loop {
        let wait = if device.is_some() { next_poll.saturating_duration_since(Instant::now()) } else { DRAIN_STEP };
        match rx.recv_timeout(wait) {
            Ok(Command::Reopen(why)) => reason = Some(why.to_string()),
            // A stale error from a device already dropped would be harmless: reopening is cheap.
            Ok(Command::StreamFailed(e)) if device.is_some() => reason = Some(format!("the stream stopped: {e}")),
            Ok(Command::StreamFailed(_)) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        if device.is_none() {
            // Sound goes on into nothing at the real-time rate (capped, should the thread stall).
            let frames = ((now - last_drain).as_secs_f64() * RATE as f64).min(RATE as f64) as usize;
            last_drain = now;
            pull(&shared, &stats, frames * CHANNELS as usize, |_| {});
        }
        if now >= next_poll {
            next_poll = now + POLL;
            let current = default_id();
            if current != last_default {
                last_default = current;
                if last_default.is_some() && reason.is_none() {
                    reason = Some("the default output device changed".into());
                }
            }
            if device.is_none() && reason.is_none() {
                reason = Some(String::new());
            }
        }
        let Some(why) = reason.take() else { continue };
        if !why.is_empty() {
            info!("audio output: reopening ({why})");
            stats.reopens.fetch_add(1, Ordering::Relaxed);
        }
        // The old stream goes first: one tap pulls the mixer at a time.
        device = None;
        last_drain = Instant::now();
        match open(&shared, &tx, &stats) {
            Ok(d) => {
                device = Some(d);
                reported_none = false;
            }
            Err(e) if !reported_none => {
                reported_none = true;
                warn!("audio output: no device ({e}); sound runs silent until one appears");
            }
            Err(_) => {}
        }
        next_poll = Instant::now() + POLL;
    }
}

/// `NVF_PLAY_TEST_AUDIO_REOPEN=<seconds>[,...]`: a reopen at each time.
fn test_reopen(time: Res<Time<Real>>, out: Res<AudioOut>, mut times: Local<Option<Vec<f32>>>) {
    let times = times.get_or_insert_with(|| {
        std::env::var("NVF_PLAY_TEST_AUDIO_REOPEN")
            .map(|v| v.split(',').filter_map(|t| t.trim().parse().ok()).collect())
            .unwrap_or_default()
    });
    let now = time.elapsed_secs();
    times.retain(|t| {
        let due = *t <= now;
        if due {
            out.reopen("test aid");
        }
        !due
    });
}

fn report(mut exit: MessageReader<AppExit>, out: Res<AudioOut>, volumes: Res<SoundVolumes>) {
    if exit.read().last().is_some() {
        let s = &out.stats;
        let frames = s.frames.load(Ordering::Relaxed);
        info!(
            "audio output summary: opened {} times, {} reopens, {:.1} s mixed, {:.1} s since the last open",
            s.opens.load(Ordering::Relaxed),
            s.reopens.load(Ordering::Relaxed),
            frames as f64 / RATE as f64,
            frames.saturating_sub(s.frames_at_open.load(Ordering::Relaxed)) as f64 / RATE as f64,
        );
        let heard = SoundCategory::ALL.map(|c| format!("{} {:.1} s", c.name(), volumes.0.heard[c.index()].load(Ordering::Relaxed) as f64 / RATE as f64));
        let levels = SoundCategory::ALL.map(|c| format!("{} {:.0}%", c.name(), volumes.get(c) * 100.0));
        info!("audio categories: {} heard; volumes master {:.0}%, {}", heard.join(", "), volumes.master() * 100.0, levels.join(", "));
        let audible = SoundCategory::ALL.map(|c| format!("{} {:.1} s", c.name(), volumes.0.audible[c.index()].load(Ordering::Relaxed) as f64 / RATE as f64));
        info!("audio categories after volumes: {} audible", audible.join(", "));
        let m = s.mix();
        info!(
            "audio mix: pre peak {:.9}, {} samples over full scale; post peak {:.9}, {} samples over full scale; {} frames limited",
            m.pre_peak, m.pre_over, m.post_peak, m.post_over, m.limited
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rodio::buffer::SamplesBuffer;

    fn ones(samples: usize) -> SamplesBuffer {
        SamplesBuffer::new(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap(), vec![1.0; samples])
    }

    #[test]
    fn footsteps_are_the_fst_records() {
        for (edid, c) in [
            (Some("FSTDirtWalkL"), SoundCategory::Footstep),
            (Some("fstWaterSwim"), SoundCategory::Footstep),
            (Some("WPNPistol10mmFire3D"), SoundCategory::Effects),
            (Some("FS"), SoundCategory::Effects),
            (None, SoundCategory::Effects),
        ] {
            assert_eq!(SoundCategory::of_effect(edid), c, "{edid:?}");
        }
    }

    #[test]
    fn the_volumes_start_at_the_originals_defaults() {
        let volumes = SoundVolumes::default();
        assert_eq!(volumes.master(), 1.0);
        assert_eq!(SoundCategory::ALL.map(|c| volumes.get(c)), [0.6, 0.5, 0.6, 1.0, 0.5], "music, footstep, voice, effects, radio");
        assert!((volumes.gain(SoundCategory::Music) - 0.6).abs() < 1e-6);
    }

    #[test]
    fn a_category_plays_at_its_volume_times_the_master_and_eases_to_a_change() {
        let volumes = SoundVolumes::default();
        volumes.set(SoundCategory::Music, 1.0);
        let (music, source) = rodio::mixer::mixer(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap());
        let mut gain = Gain::new(source, volumes.clone(), SoundCategory::Music);
        music.add(ones(RATE as usize));
        assert_eq!(gain.next(), Some(1.0), "full volume at the start");
        volumes.set_master(0.5);
        volumes.set(SoundCategory::Music, 0.5);
        volumes.set(SoundCategory::Effects, 0.0);
        let eased: Vec<f32> = gain.by_ref().take(10).collect();
        assert!(eased.windows(2).all(|w| w[1] < w[0]) && eased[9] > 0.9, "eased down, not jumped: {eased:?}");
        let ease = (VOLUME_EASE.as_secs_f32() * RATE as f32 * CHANNELS as f32) as usize;
        let settled = gain.by_ref().nth(ease).unwrap();
        assert!((settled - 0.25).abs() < 1e-4, "{settled}");
        // With nothing playing: silence, not the end.
        let rest = gain.by_ref().take(RATE as usize * 2).last();
        assert_eq!(rest, Some(0.0));
        assert!(volumes.0.heard[SoundCategory::Music.index()].load(Ordering::Relaxed) > 0);
        assert_eq!(volumes.0.heard[SoundCategory::Voice.index()].load(Ordering::Relaxed), 0);
    }

    #[test]
    fn a_category_turned_down_is_heard_but_not_audible() {
        let volumes = SoundVolumes::default();
        volumes.set(SoundCategory::Radio, 0.0);
        let (radio, source) = rodio::mixer::mixer(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap());
        let mut gain = Gain::new(source, volumes.clone(), SoundCategory::Radio);
        radio.add(ones(RATE as usize));
        assert!(gain.by_ref().take(RATE as usize).all(|s| s == 0.0));
        let count = |c: &[AtomicU64; 5]| c[SoundCategory::Radio.index()].load(Ordering::Relaxed);
        assert!(count(&volumes.0.heard) > 0);
        assert_eq!(count(&volumes.0.audible), 0);
    }

    #[test]
    fn slow_motion_slows_effects_and_voices_not_music_radio_or_the_interface() {
        assert_eq!(slowed_sound(1.0), 1.0);
        assert!((slowed_sound(0.25) - 0.5).abs() < 1e-6, "the death camera: an octave down");
        assert!((slowed_sound(0.5) - 0.5f32.sqrt()).abs() < 1e-6, "VATS firing");
        assert_eq!(slowed_sound(0.05), SOUND_FLOOR, "VATS marking");
        assert_eq!(slowed_sound(0.0), SOUND_FLOOR);
        assert_eq!(slowed_sound(2.0), 1.0, "never faster");
        for c in [SoundCategory::Effects, SoundCategory::Footstep, SoundCategory::Voice] {
            assert_eq!(speed_factor(0.5, c, false), 0.5, "{c:?}");
            assert_eq!(speed_factor(0.5, c, true), 1.0, "{c:?}, interface");
        }
        for c in [SoundCategory::Music, SoundCategory::Radio] {
            assert_eq!(speed_factor(0.5, c, false), 1.0, "{c:?}");
        }
    }

    /// A stereo 48 kHz mix of known samples: loud ones of either sign, ones exactly at
    /// full scale (not over), quiet ones and silence.
    fn known_mix() -> Vec<f32> {
        let pattern = [0.5, -0.5, 1.0, -1.0, 1.25, -0.25, -2.5, 0.0, 0.0, 1.000_001, 0.0, 0.0, -0.999, 0.75];
        pattern.iter().copied().cycle().take(4096).collect()
    }

    /// The peak and the count over full scale of `samples`, worked out plainly.
    fn expected(samples: &[f32]) -> (f32, u64) {
        let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        (peak, samples.iter().filter(|s| s.abs() > 1.0).count() as u64)
    }

    fn bus_of(samples: &[f32]) -> Mutex<MixBus> {
        let (mixer, source) = rodio::mixer::mixer(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap());
        mixer.add(SamplesBuffer::new(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap(), samples.to_vec()));
        Mutex::new(MixBus::new(source))
    }

    #[test]
    fn mix_measurements_count_before_and_after_limiting() {
        let samples = known_mix();
        let (peak, over) = expected(&samples);
        assert_eq!((peak, over), (2.5, 3 * (4096 / 14) as u64 + 2), "the pattern's own count (1.25, -2.5 and 1.000001 per round; the last part round holds two)");
        // Pulled in the device's blocks, in uneven blocks, and drained into nothing (the
        // watcher without a device), past the end of the sound into silence.
        let mut outputs = Vec::new();
        for blocks in [vec![BLOCK; 10], vec![2, 6, 98, 1000, 40, 3974], vec![5120]] {
            let (bus, stats) = (bus_of(&samples), Stats::default());
            let mut out = Vec::new();
            for (i, &n) in blocks.iter().enumerate() {
                if i == blocks.len() - 1 && blocks.len() == 1 {
                    pull(&bus, &stats, n, |_| {});
                } else {
                    pull(&bus, &stats, n, |s| out.push(s));
                }
            }
            let m = stats.mix();
            assert_eq!((m.pre_peak, m.pre_over), (peak, over), "{blocks:?}");
            // No limiter yet: the output is the mix as it is.
            assert_eq!((m.post_peak, m.post_over, m.limited), (m.pre_peak, m.pre_over, 0), "{blocks:?}");
            assert_eq!(stats.frames.load(Ordering::Relaxed), 5120 / CHANNELS as u64);
            outputs.push(out);
        }
        assert_eq!(&outputs[0][..samples.len()], &samples[..], "passed unchanged");
        assert!(outputs[0][samples.len()..].iter().all(|&s| s == 0.0), "silence after the sound");
        assert_eq!(outputs[0], outputs[1], "the same however it is split");
    }

    #[test]
    fn reopening_preserves_the_mix_bus() {
        let samples = known_mix();
        let shared = Arc::new(bus_of(&samples));
        let stats = Arc::new(Stats::default());
        // One device's tap takes a block, then it goes and another taps the same bus.
        let first: Vec<f32> = Tap::new(shared.clone(), stats.clone()).take(BLOCK).collect();
        let second: Vec<f32> = Tap::new(shared.clone(), stats.clone()).take(BLOCK).collect();
        assert_eq!(first, samples[..BLOCK], "the first tap");
        assert_eq!(second, samples[BLOCK..2 * BLOCK], "the second goes on where the first stopped");
        // The measurements go on across the taps.
        let (peak, over) = expected(&samples[..2 * BLOCK]);
        let m = stats.mix();
        assert_eq!((m.pre_peak, m.pre_over, m.post_peak, m.post_over, m.limited), (peak, over, peak, over, 0));
        assert_eq!(stats.frames.load(Ordering::Relaxed), (2 * BLOCK / CHANNELS as usize) as u64);
    }

    #[test]
    fn stop_lines_name_every_reason() {
        let mut stops = SoundStops::default();
        assert_eq!(stops.hard_line(), "sfx hard stops: 0 steals, 0 loop stops, 0 script stops, 0 ambient stops, 0 radio stops, 0 voice stops");
        stops.declicked = [1, 2, 3, 4, 5, 6];
        stops.completed = 20;
        assert_eq!(stops.declick_line(), "sfx declicks: 1 steals, 2 loop stops, 3 script stops, 4 ambient stops, 5 radio stops, 6 voice stops; 20 completed, 1 pending");
    }

    #[test]
    fn volumes_stay_within_a_full_amount() {
        let v = SoundVolumes::default();
        v.set(SoundCategory::Radio, 1.5);
        v.set_master(-1.0);
        assert_eq!((v.get(SoundCategory::Radio), v.master(), v.gain(SoundCategory::Radio)), (1.0, 0.0, 0.0));
    }
}
