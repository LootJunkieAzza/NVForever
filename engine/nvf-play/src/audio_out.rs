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
//! - Spatial `PlaybackSettings` (Bevy's `SpatialAudioSink`) are not built: NVForever pans
//!   its own sounds (`sfx::Panned`); a spatial request plays plain, with a warning.
//!
//! Log lines: `audio output: opened <device> (<rate> Hz, <channels> ch)`, `audio output:
//! reopening (<why>)`, `audio output: no device (<why>); sound runs silent until one
//! appears`, and at the end `audio output summary: opened N times, M reopens, X s mixed,
//! Y s since the last open`.
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
    PlaybackSettings,
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

/// The run-long mixer every sound plays into, and the line to the watcher thread.
#[derive(Resource)]
pub struct AudioOut {
    mixer: Mixer,
    watcher: Sender<Command>,
    stats: Arc<Stats>,
}

impl AudioOut {
    /// The mixer sounds play into (it outlives any device).
    pub fn mixer(&self) -> &Mixer {
        &self.mixer
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
}

/// The output and the playing of every `AudioPlayer<T>` registered with [`AddAudioSource`].
pub struct AudioOutPlugin;

impl Plugin for AudioOutPlugin {
    fn build(&self, app: &mut App) {
        let (mixer, source) = rodio::mixer::mixer(NonZero::new(CHANNELS).unwrap(), NonZero::new(RATE).unwrap());
        let shared = Arc::new(Mutex::new(source));
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
        app.insert_resource(AudioOut { mixer, watcher: tx, stats })
            .init_resource::<GlobalVolume>()
            .add_systems(Update, test_reopen)
            .add_systems(Last, report)
            .add_audio_source::<AudioSource>()
            .init_asset_loader::<AudioLoader>();
    }
}

#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct AudioOutSystems;

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
    queued: Query<(Entity, &AudioPlayer<T>, &PlaybackSettings), Without<AudioSink>>,
    mut warned: Local<bool>,
    mut commands: Commands,
) {
    let Some(out) = out else { return };
    for (entity, player, settings) in &queued {
        let Some(source) = sources.get(&player.0) else { continue };
        if settings.spatial && !*warned {
            *warned = true;
            warn!("audio output: spatial sound is not built; {entity} plays without it");
        }
        let sink = Player::connect_new(&out.mixer);
        let decoder = source.decoder();
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
        sink.set_speed(settings.speed);
        sink.set_volume(settings.volume * global.volume);
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
    shared: Arc<Mutex<MixerSource>>,
    stats: Arc<Stats>,
    buf: Vec<f32>,
    pos: usize,
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

/// Takes `n` samples (even) from the mixer, silence where nothing plays.
fn pull(shared: &Mutex<MixerSource>, stats: &Stats, n: usize, mut each: impl FnMut(f32)) {
    let mut source = shared.lock().unwrap_or_else(|e| e.into_inner());
    for _ in 0..n {
        each(source.next().unwrap_or(0.0));
    }
    stats.frames.fetch_add((n / CHANNELS as usize) as u64, Ordering::Relaxed);
}

fn default_id() -> Option<String> {
    let device = rodio::cpal::default_host().default_output_device()?;
    device.id().ok().map(|id| id.to_string())
}

fn device_name(device: &rodio::cpal::Device) -> String {
    device.description().map(|d| d.name().to_string()).unwrap_or_else(|_| "an unnamed device".into())
}

/// Opens the default output device, else the first other one that opens, and taps the mixer.
fn open(shared: &Arc<Mutex<MixerSource>>, tx: &Sender<Command>, stats: &Arc<Stats>) -> Result<MixerDeviceSink, String> {
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
                sink.mixer().add(Tap { shared: shared.clone(), stats: stats.clone(), buf: Vec::with_capacity(BLOCK), pos: 0 });
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
fn watch(shared: Arc<Mutex<MixerSource>>, rx: std::sync::mpsc::Receiver<Command>, tx: Sender<Command>, stats: Arc<Stats>) {
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

fn report(mut exit: MessageReader<AppExit>, out: Res<AudioOut>) {
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
    }
}
