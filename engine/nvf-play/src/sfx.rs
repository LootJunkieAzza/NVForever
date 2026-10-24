//! Sound effects in the game (docs/spec-notes/combat-rules.md §16): SOUN records played
//! on request ([`PlaySound`]), today the sounds of the player's shots: the weapon firing,
//! clicking empty and being drawn or put away, impacts by material, explosions.
//!
//! - **The record** [documented: xEdit SOUN]: a file or a folder under `sound\`, flags,
//!   a minimum and a maximum attenuation distance, a static attenuation in hundredths of
//!   a decibel and a frequency adjustment in percent.
//! - **The file.** A folder stands for the `.wav` and `.ogg` files directly in it; one is
//!   picked at random each time [hypothesis: from the folders holding a few takes of one
//!   sound]. A record names a `.wav`; where the archives hold an `.ogg` of that name
//!   instead, that is the file [observed: 2,184 of the base sound archive's effects are
//!   Ogg Vorbis, the ambient ones above all, docs/spec-notes/audio.md §5]. WAV files of
//!   PCM (8, 16 or 24 bits) and 32-bit float are decoded here, Ogg Vorbis by rodio, both
//!   to samples played through Bevy's audio as a source of our own ([`PcmSound`]).
//!   Other encodings are reported once and not played.
//! - **Loudness**: the static attenuation is taken off (gain 10^(−dB ÷ 20)). A sound with
//!   a place follows its record's attenuation curve [documented: GECK wiki Sound,
//!   Attenuation tab: five sliders, "the sound's volume percentage at its inner radius"
//!   down to "at its outer radius", the curve between, "logarithmic" by default]: the
//!   first point within its minimum distance of the listener (the player's eye), the last
//!   at its maximum distance, silent beyond ([`distance_gain`]). The three middle points
//!   stand evenly between the two distances and the volume goes straight from point to
//!   point ([`curve_gain`]) [hypothesis]; a percentage is a share of the amplitude
//!   [hypothesis]. A record without a curve plays the default 100, 50, 20, 5, 0.
//! - **Panning** [design; how the game places sounds is not known]: a sound with a place
//!   is heard more on the side of the listener it is on ([`pan_gains`]): the ear facing
//!   it hears it whole, the other ear loses up to [`PAN_DEPTH`] of it, by how far to the
//!   side it is (straight ahead, behind, above or below: both ears alike). A panned sound
//!   is played in stereo, its channels mixed to one first. The listener is the player's
//!   eye, facing the player's heading; [`pan_sounds`] follows the listener while a sound
//!   plays ([`SoundAt`]). Sounds at the listener (the player's own, interface sounds)
//!   and the background loops are not panned. Other modules' positioned streams use
//!   [`Panned`] (placed loops in [`crate::ambient`], radios in [`crate::radio`]).
//! - **Pitch**: the frequency adjustment, and ±5% at random with the Random Frequency
//!   Shift flag [design: the size of the shift].
//! - **Loops.** A [`LoopSound`] starts a SOUN going round and stops it again (a
//!   weapon's Shoot 3D Looping sound while it fires).
//! - **Things that sound while they are there** ([`SoundSource`]): a creature's conscious
//!   loop, an activator's or a container's looping sound; sent every frame they should
//!   sound and played by [`crate::ambient`] as it plays sound markers.
//! - **Reverb** ([`crate::reverb`]): a world sound plays through the reverb environment
//!   of the listener's acoustic space as it starts, in stereo, with a tail after its file.
//! - The sounds animations and doors name in their text keys
//!   (reloads, footsteps, armour rustle, doors) are sent here by nvforever's
//!   `anim_sounds.rs` (docs/spec-notes/audio.md §10). Placed sounds and the cells'
//!   background loops are [`crate::ambient`]; music is [`crate::music`].
//!
//! [`SfxSettings`] turns them off (`nvforever --no-sound`) and sets the volume.

use crate::player::Player;
use crate::reverb::{Preset, Reverb, RoomSend};
use crate::voice_filter::VoiceFilter;
use crate::PlayWorld;
use bevy::audio::{ChannelCount, Decodable, Sample, SampleRate, Source, Volume};
use crate::audio_out::{AddAudioSource, KeepsSpeed, SoundCategory};
use bevy::prelude::*;
use fnv_combat::fx::{FxForms, Sound, DEFAULT_ATTENUATION_CURVE};
use fnv_combat::rng::SplitMix;
use fnv_sim::FormId;
use std::collections::{HashMap, HashSet};
use std::num::NonZero;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How sound effects behave; insert before the play plugins to change the defaults.
#[derive(Resource, Clone, Copy, Debug)]
pub struct SfxSettings {
    pub play: bool,
    /// Linear volume, 1 = as recorded less the record's attenuation.
    pub volume: f32,
    /// World sounds through the acoustic space's reverb ([`crate::reverb`]).
    pub reverb: bool,
}

impl Default for SfxSettings {
    fn default() -> Self {
        SfxSettings { play: true, volume: 1.0, reverb: true }
    }
}

/// Play a SOUN: at a place (game space), or with `None` at the listener.
#[derive(Message, Clone, Copy, Debug)]
pub struct PlaySound {
    pub sound: FormId,
    pub at: Option<[f32; 3]>,
}

/// Start (`on`) or stop a SOUN that goes round until it is stopped, at the listener.
#[derive(Message, Clone, Copy, Debug)]
pub struct LoopSound {
    pub sound: FormId,
    pub on: bool,
}

/// A looping sound being played.
#[derive(Component)]
struct Looping(FormId);

/// Something that sounds as long as it is there, at a place (game space): sent every
/// frame it should sound; [`crate::ambient`] plays its SOUN as it plays a sound marker
/// (looping, or now and then for Play At Random) while the listener is within reach.
/// `key` tells one source from another ([`SoundSource::key`]).
#[derive(Message, Clone, Copy, Debug)]
pub struct SoundSource {
    pub key: u64,
    pub sound: FormId,
    pub at: [f32; 3],
    /// Go round whatever the sound's flags say (a weapon's idle sound, audio.md §15.2).
    pub looping: bool,
}

impl SoundSource {
    /// A key from a kind of source (one per sender and use) and a reference.
    pub fn key(kind: u8, reference: u32) -> u64 {
        (u64::from(kind) << 32) | u64::from(reference)
    }
}

/// How much of a sound straight to one side the far ear loses [design].
pub const PAN_DEPTH: f32 = 0.7;

/// The left and right gains of a sound at `at` heard from `eye` facing the compass
/// `heading` (radians, 0 north, π/2 east; game space, z up): see the module notes.
pub fn pan_gains(eye: [f32; 3], heading: f32, at: [f32; 3]) -> [f32; 2] {
    let d = [at[0] - eye[0], at[1] - eye[1], at[2] - eye[2]];
    let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    if len < 1.0 {
        return [1.0, 1.0];
    }
    // The listener's right: the heading turned a quarter clockwise.
    let side = ((d[0] * heading.cos() - d[1] * heading.sin()) / len).clamp(-1.0, 1.0);
    if side >= 0.0 {
        [1.0 - PAN_DEPTH * side, 1.0]
    } else {
        [1.0, 1.0 + PAN_DEPTH * side]
    }
}

/// Left and right gains shared with the audio thread while a sound plays.
#[derive(Clone, Debug)]
pub struct Pan(Arc<[AtomicU32; 2]>);

impl Pan {
    pub fn new(gains: [f32; 2]) -> Pan {
        Pan(Arc::new([AtomicU32::new(gains[0].to_bits()), AtomicU32::new(gains[1].to_bits())]))
    }

    pub fn set(&self, gains: [f32; 2]) {
        self.0[0].store(gains[0].to_bits(), Ordering::Relaxed);
        self.0[1].store(gains[1].to_bits(), Ordering::Relaxed);
    }

    pub fn get(&self) -> [f32; 2] {
        [f32::from_bits(self.0[0].load(Ordering::Relaxed)), f32::from_bits(self.0[1].load(Ordering::Relaxed))]
    }
}

/// A sound playing at a place: [`pan_sounds`] turns its [`Pan`] with the listener.
#[derive(Component, Clone, Debug)]
pub struct SoundAt {
    pub at: [f32; 3],
    pub pan: Pan,
}

impl SoundAt {
    /// A sound at `at`, panned for the listener as it is now (centred without one).
    pub fn new(at: [f32; 3], player: Option<&Player>) -> SoundAt {
        let gains = player.map_or([1.0, 1.0], |p| pan_gains(p.eye(), p.heading, at));
        SoundAt { at, pan: Pan::new(gains) }
    }
}

/// One frame of `channels` samples mixed to one and placed by `pan`: the left sample
/// and the right one; `None` when the input has ended.
fn pan_frame(frame: impl Iterator<Item = Sample>, channels: usize, pan: &Pan, filter: Option<&mut VoiceFilter>) -> Option<(Sample, Sample)> {
    let mut sum = 0.0;
    let mut n = 0;
    for s in frame.take(channels.max(1)) {
        sum += s;
        n += 1;
    }
    if n == 0 {
        return None;
    }
    let mut m = sum / channels.max(1) as f32;
    if let Some(f) = filter {
        m = f.run(m);
    }
    let [l, r] = pan.get();
    Some((m * l, m * r))
}

/// Another module's sound played at a place: its source, mixed to one channel and
/// panned, in stereo.
#[derive(Asset, TypePath, Clone)]
pub struct Panned<T: Decodable + TypePath + Clone> {
    pub inner: T,
    pub pan: Pan,
    /// Played through this environment's reverb ([`crate::reverb`]).
    pub reverb: Option<RoomSend>,
    /// Played through the voice filter ([`crate::voice_filter`]).
    pub voice_filter: bool,
}

impl<T: Decodable + TypePath + Clone> Panned<T> {
    /// Panned, without reverb.
    pub fn new(inner: T, pan: Pan) -> Panned<T> {
        Panned { inner, pan, reverb: None, voice_filter: false }
    }
}

pub struct PanDecoder<S> {
    input: S,
    pan: Pan,
    right: Option<Sample>,
    reverb: Option<Reverb>,
    filter: Option<VoiceFilter>,
}

impl<S: Source> Iterator for PanDecoder<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let c = self.input.channels().get() as usize;
        let (l, r) = with_reverb(pan_frame(&mut self.input, c, &self.pan, self.filter.as_mut()), &mut self.reverb)?;
        self.right = Some(r);
        Some(l)
    }
}

/// A frame through a sound's reverb, if it has one; once the input has ended
/// (`None`), the reverb's tail.
fn with_reverb(frame: Option<(Sample, Sample)>, reverb: &mut Option<Reverb>) -> Option<(Sample, Sample)> {
    match (frame, reverb) {
        (Some((l, r)), Some(rv)) => Some(rv.process(l, r)),
        (None, Some(rv)) => rv.tail(),
        (frame, None) => frame,
    }
}

impl<S: Source> Source for PanDecoder<S> {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        NonZero::new(2).unwrap_or(NonZero::<u16>::MIN)
    }

    fn sample_rate(&self) -> SampleRate {
        self.input.sample_rate()
    }

    fn total_duration(&self) -> Option<Duration> {
        let tail = self.reverb.as_ref().map_or(0.0, Reverb::seconds_left);
        self.input.total_duration().map(|d| d + Duration::from_secs_f32(tail))
    }
}

impl<T: Decodable + TypePath + Clone> Decodable for Panned<T> {
    type Decoder = PanDecoder<T::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        let input = self.inner.decoder();
        let reverb = self.reverb.map(|p| p.reverb(input.sample_rate().get()));
        let filter = self.voice_filter.then(|| VoiceFilter::new(input.sample_rate().get()));
        PanDecoder { input, pan: self.pan.clone(), right: None, reverb, filter }
    }
}

/// Decoded samples of a sound file.
#[derive(Asset, TypePath, Clone, Debug)]
pub struct PcmSound {
    samples: Arc<[f32]>,
    channels: u16,
    rate: u32,
    /// Played at a place: mixed to one channel and panned, in stereo.
    pan: Option<Pan>,
    /// Played through this environment's reverb, in stereo ([`crate::reverb`]).
    reverb: Option<RoomSend>,
}

pub struct PcmDecoder {
    sound: PcmSound,
    at: usize,
    right: Option<Sample>,
    reverb: Option<Reverb>,
}

impl PcmDecoder {
    /// The next frame as a stereo pair (panned, or a mono sound on both ears, or the
    /// first two channels); `None` at the end.
    fn stereo_frame(&mut self) -> Option<(Sample, Sample)> {
        let c = self.sound.channels.max(1) as usize;
        let frame = self.sound.samples.get(self.at..self.at + c)?;
        self.at += c;
        match &self.sound.pan {
            Some(pan) => pan_frame(frame.iter().copied(), c, pan, None),
            None if c == 1 => Some((frame[0], frame[0])),
            None => Some((frame[0], frame[1])),
        }
    }
}

impl Iterator for PcmDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if self.sound.pan.is_none() && self.reverb.is_none() {
            let s = self.sound.samples.get(self.at).copied();
            self.at += 1;
            return s;
        }
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let frame = self.stereo_frame();
        let (l, r) = with_reverb(frame, &mut self.reverb)?;
        self.right = Some(r);
        Some(l)
    }
}

impl Source for PcmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        let c = if self.sound.pan.is_some() || self.reverb.is_some() { 2 } else { self.sound.channels };
        NonZero::new(c).unwrap_or(NonZero::<u16>::MIN)
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.sound.rate).unwrap_or(NonZero::<u32>::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        let tail = self.reverb.as_ref().map_or(0.0, Reverb::seconds_left);
        Some(Duration::from_secs_f64(self.sound.seconds() + tail as f64))
    }
}

impl Decodable for PcmSound {
    type Decoder = PcmDecoder;

    fn decoder(&self) -> PcmDecoder {
        let reverb = self.reverb.map(|p| p.reverb(self.rate));
        PcmDecoder { sound: self.clone(), at: 0, right: None, reverb }
    }
}

impl PcmSound {
    pub fn seconds(&self) -> f64 {
        self.samples.len() as f64 / (self.channels.max(1) as f64 * self.rate.max(1) as f64)
    }

    /// Decode a whole file of another encoding (Ogg Vorbis) with rodio.
    pub fn from_encoded(bytes: Vec<u8>) -> Result<PcmSound, String> {
        let d = rodio::Decoder::new(std::io::Cursor::new(bytes)).map_err(|e| e.to_string())?;
        let (channels, rate) = (d.channels().get(), d.sample_rate().get());
        Ok(PcmSound { samples: d.collect::<Vec<f32>>().into(), channels, rate, pan: None, reverb: None })
    }

    /// Decode a WAV file: the `fmt ` and `data` chunks of a RIFF/WAVE file [documented:
    /// the WAVE format]. `Err` names what is not supported.
    pub fn from_wav(bytes: &[u8]) -> Result<PcmSound, String> {
        if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
            return Err("not a RIFF/WAVE file".into());
        }
        let u16_at = |b: &[u8], i: usize| u16::from_le_bytes([b[i], b[i + 1]]);
        let u32_at = |b: &[u8], i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]);
        let mut fmt: Option<(u16, u16, u32, u16)> = None;
        let mut at = 12;
        while at + 8 <= bytes.len() {
            let id = &bytes[at..at + 4];
            let len = u32_at(bytes, at + 4) as usize;
            let body = &bytes[at + 8..(at + 8 + len).min(bytes.len())];
            match id {
                b"fmt " if body.len() >= 16 => {
                    let mut tag = u16_at(body, 0);
                    // WAVE_FORMAT_EXTENSIBLE: the real format is the first two bytes of
                    // the sub-format.
                    if tag == 0xFFFE && body.len() >= 26 {
                        tag = u16_at(body, 24);
                    }
                    fmt = Some((tag, u16_at(body, 2), u32_at(body, 4), u16_at(body, 14)));
                }
                b"data" => {
                    let (tag, channels, rate, bits) = fmt.ok_or("data before fmt")?;
                    let samples: Vec<f32> = match (tag, bits) {
                        (1, 8) => body.iter().map(|&b| (b as f32 - 128.0) / 128.0).collect(),
                        (1, 16) => body.as_chunks::<2>().0.iter().map(|c| i16::from_le_bytes(*c) as f32 / 32768.0).collect(),
                        (1, 24) => body.as_chunks::<3>().0.iter().map(|c| (i32::from_le_bytes([0, c[0], c[1], c[2]]) >> 8) as f32 / 8_388_608.0).collect(),
                        (3, 32) => body.as_chunks::<4>().0.iter().map(|c| f32::from_le_bytes(*c)).collect(),
                        _ => return Err(format!("format tag {tag} with {bits} bits is not decoded")),
                    };
                    if channels == 0 || rate == 0 {
                        return Err("no channels or sample rate".into());
                    }
                    return Ok(PcmSound { samples: samples.into(), channels, rate, pan: None, reverb: None });
                }
                _ => {}
            }
            at += 8 + len + (len & 1);
        }
        Err("no data chunk".into())
    }
}

/// The gain of a sound `distance` units from the listener: its attenuation curve between
/// the minimum and the maximum distance (see the module notes).
pub fn distance_gain(s: &Sound, distance: f32) -> f32 {
    let curve = s.attenuation_curve.unwrap_or(DEFAULT_ATTENUATION_CURVE).map(|p| (p / 100.0).clamp(0.0, 1.0));
    let (min, max) = (s.min_distance.max(1.0), s.max_distance);
    if distance <= min {
        return curve[0];
    }
    if max <= min {
        // No span to lay the curve on: the inner level, falling as min ÷ distance [design].
        return curve[0] * min / distance;
    }
    if distance >= max {
        return 0.0;
    }
    curve_gain(&curve, (distance - min) / (max - min))
}

/// A five-point curve at `t` (0 at the inner radius, 1 at the outer): the points stand
/// evenly over the span, straight lines between them [hypothesis: the GECK's five
/// sliders say nothing of where the middle three stand].
pub fn curve_gain(curve: &[f32; 5], t: f32) -> f32 {
    let x = t.clamp(0.0, 1.0) * 4.0;
    let i = (x.floor() as usize).min(3);
    let f = x - i as f32;
    curve[i] + (curve[i + 1] - curve[i]) * f
}

/// The gain the record's static attenuation leaves.
pub fn static_gain(s: &Sound) -> f32 {
    10f32.powf(-s.static_attenuation_db / 20.0)
}

#[derive(Resource)]
pub struct Sfx {
    pub forms: FxForms,
    /// Decoded files by archive path; `None`: not there or not decoded.
    files: HashMap<String, Option<Handle<PcmSound>>>,
    /// The `.wav` files of a folder.
    folders: HashMap<String, Vec<String>>,
    reported: HashSet<String>,
    pub(crate) rng: SplitMix,
    pub played: u32,
    pub silent: u32,
    pub failed: u32,
    /// The listener's reverb environment now ([`crate::reverb::RoomReverb`] keeps it),
    /// and how many sounds were played through one.
    pub room_reverb: Option<&'static Preset>,
    pub reverbed: u32,
    /// Sounds kept out of the room's reverb by their Reverb Attenuation Control (100),
    /// and the sum of the sends of those played through one (`crate::reverb::send_of`).
    pub reverb_kept_out: u32,
    pub reverb_send_sum: f32,
    /// Placed sounds played by a curve of their own (not the default 100, 50, 20, 5, 0).
    pub own_curve: u32,
    /// The listener is under water ([`listener_submerged`]), how many sounds that
    /// silenced (Mute When Submerged, audio.md §15.2) and the deepest the listener went
    /// under a surface, units.
    pub submerged: bool,
    pub muted_submerged: u32,
    pub deepest: f32,
    /// Sounds with the 2D flag played at a place, not panned.
    pub flat_2d: u32,
    /// One-shot sounds playing now (counted each frame by [`play`], and as they start),
    /// those of each SOUN, the most at once, and how many were not started because
    /// [`MAX_VOICES`] or [`MAX_SAME`] were playing.
    voices: usize,
    same: HashMap<FormId, usize>,
    pub most_voices: usize,
    pub voice_limited: u32,
}

#[derive(Component)]
struct SfxSound;

/// A one-shot sound effect playing, of this SOUN ([`MAX_VOICES`]).
#[derive(Component)]
struct OneShot(FormId);

/// One-shot sound effects playing at once, at most; one asked for beyond it is not played
/// [engineering choice: the game's own limit is not known. Two dozen mantis nymphs walking
/// started about 130 footsteps a second; each mixed through its own reverb, the mixer fell
/// behind real time, sounds stopped ending and 1,800 piled up (docs/design/
/// frame-rate-audit.md §2)]. Interface sounds are not limited.
pub const MAX_VOICES: usize = 64;
/// The same SOUN playing at once, at most (a crowd's footsteps) [engineering choice].
pub const MAX_SAME: usize = 8;

/// Whether one more one-shot of `sound` may start with `voices` playing, `same` of them
/// of that sound ([`MAX_VOICES`], [`MAX_SAME`]).
pub fn voice_free(voices: usize, same: usize) -> bool {
    voices < MAX_VOICES && same < MAX_SAME
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let t = std::time::Instant::now();
    let forms = FxForms::build(d.lo, d.schema);
    info!(
        "sfx: {} sounds, {} impacts in {} sets, {} add-on nodes read in {:.0} ms",
        forms.sounds.len(),
        forms.impacts.len(),
        forms.impact_sets.len(),
        forms.addons.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    commands.insert_resource(Sfx { forms, files: HashMap::new(), folders: HashMap::new(), reported: HashSet::new(), rng: SplitMix(0x5f37_59df), played: 0, silent: 0, failed: 0, room_reverb: None, reverbed: 0, reverb_kept_out: 0, reverb_send_sum: 0.0, own_curve: 0, submerged: false, muted_submerged: 0, deepest: 0.0, flat_2d: 0, voices: 0, same: HashMap::new(), most_voices: 0, voice_limited: 0 });
}

/// The archive path of the file a SOUN plays this time: the file it names, or the Ogg
/// Vorbis file of the same name when the archives hold that instead; of a folder, one
/// of its `.wav` and `.ogg` files.
fn pick_file(sfx: &mut Sfx, vfs: &fnv_vfs::Vfs, file: &str) -> Option<String> {
    let path = fnv_vfs::normalize(&format!("sound\\{}", file.trim_start_matches(['\\', '/'])));
    if let Some(stem) = path.strip_suffix(".wav") {
        let ogg = format!("{stem}.ogg");
        return Some(if !vfs.exists(&path) && vfs.exists(&ogg) { ogg } else { path });
    }
    if path.ends_with(".ogg") {
        return Some(path);
    }
    let folder = if path.ends_with('\\') { path } else { format!("{path}\\") };
    let files = sfx.folders.entry(folder.clone()).or_insert_with(|| {
        let mut v = vfs.paths(|p| p.starts_with(folder.as_str()) && (p.ends_with(".wav") || p.ends_with(".ogg")) && !p[folder.len()..].contains('\\'));
        v.sort();
        v
    });
    if files.is_empty() {
        return None;
    }
    let k = (sfx.rng.next_f32() * files.len() as f32) as usize;
    Some(files[k.min(files.len() - 1)].clone())
}

/// The record, name and file of a SOUN for this playing of it; a sound without a file
/// is counted and reported once.
pub(crate) fn resolve(sfx: &mut Sfx, vfs: &fnv_vfs::Vfs, sound: FormId) -> Option<(Sound, String, String)> {
    let s = sfx.forms.sounds.get(&sound).cloned()?;
    let name = s.edid.clone().unwrap_or_else(|| sound.to_string());
    let Some(path) = s.file.as_deref().and_then(|f| pick_file(sfx, vfs, f)) else {
        sfx.failed += 1;
        if sfx.reported.insert(name.clone()) {
            warn!("sfx: {name}: no file for {:?}", s.file);
        }
        return None;
    };
    Some((s, name, path))
}

/// The record, name, file and decoded samples of a SOUN for this playing of it; a
/// sound without a file we can play is counted and reported once.
pub(crate) fn source(sfx: &mut Sfx, vfs: &fnv_vfs::Vfs, assets: &mut Assets<PcmSound>, sound: FormId) -> Option<(Sound, String, String, Handle<PcmSound>)> {
    let (s, name, path) = resolve(sfx, vfs, sound)?;
    if !sfx.files.contains_key(&path) {
        let decoded = match vfs.read(&path) {
            Some(Ok(bytes)) if path.ends_with(".wav") => PcmSound::from_wav(&bytes).map_err(|e| format!("{path}: {e}")),
            Some(Ok(bytes)) => PcmSound::from_encoded(bytes).map_err(|e| format!("{path}: {e}")),
            Some(Err(e)) => Err(format!("{path}: {e}")),
            None => Err(format!("{path}: not in the archives")),
        };
        let handle = match decoded {
            Ok(p) => Some(assets.add(p)),
            Err(e) => {
                warn!("sfx: {name}: {e}");
                None
            }
        };
        sfx.files.insert(path.clone(), handle);
    }
    let Some(handle) = sfx.files[&path].clone() else {
        sfx.failed += 1;
        return None;
    };
    Some((s, name, path, handle))
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    mut requests: MessageReader<PlaySound>,
    mut loops: MessageReader<LoopSound>,
    looping: Query<(Entity, &Looping)>,
    playing: Query<&OneShot>,
    sfx: Option<ResMut<Sfx>>,
    settings: Res<SfxSettings>,
    player: Option<Res<Player>>,
    data: Option<Res<nvf_assets::GameData>>,
    mut assets: ResMut<Assets<PcmSound>>,
) {
    let (Some(mut sfx), Some(data)) = (sfx, data) else {
        requests.clear();
        loops.clear();
        return;
    };
    let sfx = &mut *sfx;
    sfx.same.clear();
    for o in &playing {
        *sfx.same.entry(o.0).or_default() += 1;
    }
    sfx.voices = playing.iter().count();
    for l in loops.read() {
        for (e, playing) in &looping {
            if playing.0 == l.sound {
                commands.entity(e).try_despawn();
            }
        }
        if !l.on {
            continue;
        }
        let Some((s, name, path, handle)) = source(sfx, &data.vfs, &mut assets, l.sound) else { continue };
        let gain = static_gain(&s) * settings.volume.max(0.0);
        debug!("sfx: {name} {path} looping, gain {gain:.3}");
        if settings.play {
            sfx.played += 1;
            let mut e = commands.spawn((AudioPlayer::<PcmSound>(handle), PlaybackSettings::LOOP.with_volume(Volume::Linear(gain)), Looping(l.sound), SfxSound, SoundCategory::of_effect(s.edid.as_deref())));
            if is_interface(s.edid.as_deref()) {
                e.insert(KeepsSpeed);
            }
        } else {
            sfx.silent += 1;
        }
    }
    for r in requests.read() {
        one_shot_heard(&mut commands, sfx, &data.vfs, &mut assets, &settings, r.sound, r.at, player.as_deref());
    }
}

/// Whether the listener (the player's eye) is under water: the surface of the water it
/// is in or over (physics' water: cells' and placed volumes', none of an interior's
/// own, physics.md §6.6) lies above it [hypothesis: the game's listener is the camera].
/// A swimmer floating at the surface has its eye above it (feet `float_depth` 102.5
/// under); one below the floating height stays there, under water (physics.md §6.6).
pub fn listener_submerged(phys: Option<Res<crate::phys::PhysState>>, player: Option<Res<Player>>, sfx: Option<ResMut<Sfx>>) {
    let (Some(phys), Some(player), Some(mut sfx)) = (phys, player, sfx) else { return };
    let eye = player.eye();
    let depth = phys.world.water_surface_for(eye).map_or(0.0, |s| s - eye[2]);
    sfx.deepest = sfx.deepest.max(depth);
    let under = depth > 0.0;
    if sfx.submerged != under {
        sfx.submerged = under;
        debug!("sfx: the listener is {} water ({depth:.1} under the surface)", if under { "under" } else { "out of the" });
    }
}

/// Turn the sounds playing at a place with the listener.
pub fn pan_sounds(player: Option<Res<Player>>, sounds: Query<&SoundAt>) {
    let Some(p) = player else { return };
    let (eye, heading) = (p.eye(), p.heading);
    for s in &sounds {
        s.pan.set(pan_gains(eye, heading, s.at));
    }
}

/// Play a SOUN once, at a place (heard from `eye`) or at the listener. The seconds it
/// lasts as played, also when it is too quiet to be heard or sounds are switched off;
/// `None` without a file we can play. Not panned: [`one_shot_heard`] pans.
#[allow(clippy::too_many_arguments, dead_code)]
pub(crate) fn one_shot(
    commands: &mut Commands,
    sfx: &mut Sfx,
    vfs: &fnv_vfs::Vfs,
    assets: &mut Assets<PcmSound>,
    settings: &SfxSettings,
    sound: FormId,
    at: Option<[f32; 3]>,
    eye: Option<[f32; 3]>,
) -> Option<f32> {
    one_shot_at(commands, sfx, vfs, assets, settings, sound, at, eye, None)
}

/// [`one_shot`] heard by the player: a sound with a place is panned for them.
#[allow(clippy::too_many_arguments)]
pub(crate) fn one_shot_heard(
    commands: &mut Commands,
    sfx: &mut Sfx,
    vfs: &fnv_vfs::Vfs,
    assets: &mut Assets<PcmSound>,
    settings: &SfxSettings,
    sound: FormId,
    at: Option<[f32; 3]>,
    player: Option<&Player>,
) -> Option<f32> {
    let placed = at.zip(player).map(|(at, p)| SoundAt::new(at, Some(p)));
    one_shot_at(commands, sfx, vfs, assets, settings, sound, at, player.map(Player::eye), placed)
}

#[allow(clippy::too_many_arguments)]
fn one_shot_at(
    commands: &mut Commands,
    sfx: &mut Sfx,
    vfs: &fnv_vfs::Vfs,
    assets: &mut Assets<PcmSound>,
    settings: &SfxSettings,
    sound: FormId,
    at: Option<[f32; 3]>,
    eye: Option<[f32; 3]>,
    placed: Option<SoundAt>,
) -> Option<f32> {
    let (s, name, path, handle) = source(sfx, vfs, assets, sound)?;
    let distance = at.zip(eye).map(|(at, e)| ((at[0] - e[0]).powi(2) + (at[1] - e[1]).powi(2) + (at[2] - e[2]).powi(2)).sqrt());
    let gain = static_gain(&s) * distance.map_or(1.0, |d| distance_gain(&s, d)) * settings.volume.max(0.0);
    let mut speed = 1.0 + s.frequency_adjust_percent / 100.0;
    if s.has("Random Frequency Shift") {
        speed *= 0.95 + 0.1 * sfx.rng.next_f32();
    }
    let speed = speed.clamp(0.25, 4.0);
    let seconds = assets.get(&handle).map_or(0.0, |p| p.seconds() as f32) / speed;
    debug!("sfx: {name} {path} gain {gain:.3}{} speed {speed:.2}", distance.map_or(String::new(), |d| format!(" at {d:.0} units")));
    // Under water a sound so flagged is not heard; a 2D one is not panned (audio.md §15.2).
    if sfx.submerged && s.has("Mute When Submerged") {
        sfx.muted_submerged += 1;
        sfx.silent += 1;
        return Some(seconds);
    }
    let placed = match placed {
        Some(_) if s.has("2D") => {
            sfx.flat_2d += 1;
            None
        }
        p => p,
    };
    if !settings.play || gain < 0.001 {
        sfx.silent += 1;
        return Some(seconds);
    }
    let interface = is_interface(s.edid.as_deref());
    if !interface {
        let same = sfx.same.get(&sound).copied().unwrap_or(0);
        if !voice_free(sfx.voices, same) {
            sfx.voice_limited += 1;
            return Some(seconds);
        }
        sfx.voices += 1;
        sfx.same.insert(sound, same + 1);
        sfx.most_voices = sfx.most_voices.max(sfx.voices);
    }
    sfx.played += 1;
    let playback = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)).with_speed(speed);
    let category = SoundCategory::of_effect(s.edid.as_deref());
    let reverb = crate::reverb::for_sound(sfx.room_reverb, settings, &s);
    if reverb.is_none() && sfx.room_reverb.is_some() && settings.reverb && crate::reverb::send_of(s.reverb_attenuation_percent) <= 0.0 {
        sfx.reverb_kept_out += 1;
    }
    if distance.is_some() && s.attenuation_curve.is_some_and(|c| c != DEFAULT_ATTENUATION_CURVE) {
        sfx.own_curve += 1;
    }
    if let Some(p) = reverb {
        sfx.reverbed += 1;
        sfx.reverb_send_sum += p.send;
        debug!("sfx: {name} through reverb {} (send {:.2})", p.preset.name, p.send);
    }
    let own = |pcm: PcmSound, pan: Option<Pan>| PcmSound { pan, reverb, ..pcm };
    let mut e = match (placed, assets.get(&handle).cloned()) {
        // This playing's own asset: the samples are shared, the pan and reverb are its own.
        (Some(placed), Some(pcm)) => commands.spawn((AudioPlayer::<PcmSound>(assets.add(own(pcm, Some(placed.pan.clone())))), playback, placed, SfxSound, category)),
        (None, Some(pcm)) if reverb.is_some() => commands.spawn((AudioPlayer::<PcmSound>(assets.add(own(pcm, None))), playback, SfxSound, category)),
        _ => commands.spawn((AudioPlayer::<PcmSound>(handle), playback, SfxSound, category)),
    };
    if interface {
        e.insert(KeepsSpeed);
    } else {
        e.insert(OneShot(sound));
    }
    Some(seconds)
}

/// An interface sound by its SOUN editor ID (`UI…`: menus, VATS, the Pip-Boy), which keeps
/// its speed in slow motion (`audio_out::SoundSpeed`) [hypothesis: the base game's
/// interface records are named so, docs/spec-notes/audio.md].
pub(crate) fn is_interface(edid: Option<&str>) -> bool {
    edid.and_then(|e| e.get(..2)).is_some_and(|p| p.eq_ignore_ascii_case("UI"))
}

fn report(mut exit: MessageReader<AppExit>, sfx: Option<Res<Sfx>>) {
    if let (Some(_), Some(s)) = (exit.read().last(), sfx) {
        if s.played + s.silent + s.failed > 0 {
            info!(
                "sfx: {} sounds played, {} too quiet or switched off, {} without a file we can play; {} silenced under water (at most {:.0} under a surface), {} 2D sounds not panned; {} not started at the voice limit, at most {} one-shots at once; {} placed by a curve of their own",
                s.played, s.silent, s.failed, s.muted_submerged, s.deepest, s.flat_2d, s.voice_limited, s.most_voices, s.own_curve
            );
        }
    }
}

pub struct SfxPlugin;

impl Plugin for SfxPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SfxSettings>()
            .add_audio_source::<PcmSound>()
            .add_message::<PlaySound>()
            .add_message::<LoopSound>()
            .add_message::<SoundSource>()
            .add_systems(Startup, init)
            .add_systems(Update, (listener_submerged, play.after(crate::PlaySet::Activation), pan_sounds).chain())
            .add_systems(Last, report);
        crate::reverb::build(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_voice_limit_holds_back_a_crowd() {
        assert!(voice_free(0, 0));
        assert!(!voice_free(MAX_VOICES, 0), "all voices playing");
        assert!(!voice_free(1, MAX_SAME), "that sound playing its most");
        assert!(voice_free(MAX_VOICES - 1, MAX_SAME - 1));
    }

    fn wav(tag: u16, channels: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut v = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        v.extend(16u32.to_le_bytes());
        v.extend(tag.to_le_bytes());
        v.extend(channels.to_le_bytes());
        v.extend(rate.to_le_bytes());
        v.extend((rate * channels as u32 * bits as u32 / 8).to_le_bytes());
        v.extend((channels * bits / 8).to_le_bytes());
        v.extend(bits.to_le_bytes());
        // A chunk of odd length before the data, padded to an even one.
        v.extend(b"LIST");
        v.extend(3u32.to_le_bytes());
        v.extend([1, 2, 3, 0]);
        v.extend(b"data");
        v.extend((data.len() as u32).to_le_bytes());
        v.extend(data);
        v
    }

    #[test]
    fn wav_pcm_is_decoded() {
        let s = PcmSound::from_wav(&wav(1, 2, 44100, 16, &[0, 0x40, 0, 0xC0, 0xFF, 0x7F, 0, 0x80])).unwrap();
        assert_eq!((s.channels, s.rate), (2, 44100));
        assert_eq!(&s.samples[..], &[0.5, -0.5, 32767.0 / 32768.0, -1.0]);
        assert!((s.seconds() - 2.0 / 44100.0).abs() < 1e-9);
        let s = PcmSound::from_wav(&wav(1, 1, 22050, 8, &[128, 255, 0])).unwrap();
        assert_eq!(&s.samples[..], &[0.0, 127.0 / 128.0, -1.0]);
        // ADPCM (tag 2) is named, not played as noise.
        assert!(PcmSound::from_wav(&wav(2, 1, 22050, 4, &[1, 2])).unwrap_err().contains("format tag 2"));
        assert!(PcmSound::from_wav(b"OggS").is_err());
    }

    #[test]
    fn interface_sounds_are_the_ui_records() {
        assert!(is_interface(Some("UIVATSEnter")) && is_interface(Some("uiMenuOK")));
        assert!(!is_interface(Some("WPNPistol10mmFire3D")) && !is_interface(Some("U")) && !is_interface(None));
    }

    #[test]
    fn panned_by_side() {
        // Facing north (heading 0): east is to the right, west to the left.
        let eye = [0.0, 0.0, 0.0];
        assert_eq!(pan_gains(eye, 0.0, [0.0, 500.0, 0.0]), [1.0, 1.0]);
        assert_eq!(pan_gains(eye, 0.0, [0.0, -500.0, 0.0]), [1.0, 1.0]);
        assert_eq!(pan_gains(eye, 0.0, [0.0, 0.0, 500.0]), [1.0, 1.0]);
        let right = pan_gains(eye, 0.0, [500.0, 0.0, 0.0]);
        assert!((right[0] - (1.0 - PAN_DEPTH)).abs() < 1e-5 && right[1] == 1.0);
        let left = pan_gains(eye, 0.0, [-500.0, 0.0, 0.0]);
        assert!(left[0] == 1.0 && (left[1] - (1.0 - PAN_DEPTH)).abs() < 1e-5);
        // Facing east (π/2): a sound to the north is on the left.
        let g = pan_gains(eye, std::f32::consts::FRAC_PI_2, [0.0, 500.0, 0.0]);
        assert!(g[0] == 1.0 && g[1] < 0.5);
        // At the listener: centred.
        assert_eq!(pan_gains(eye, 1.0, [0.2, 0.0, 0.0]), [1.0, 1.0]);
    }

    #[test]
    fn a_panned_sound_is_stereo() {
        let pan = Pan::new([1.0, 0.5]);
        let mono = PcmSound { samples: vec![0.8, -0.4].into(), channels: 1, rate: 22050, pan: Some(pan.clone()), reverb: None };
        let d = mono.decoder();
        assert_eq!(d.channels().get(), 2);
        assert_eq!(d.collect::<Vec<_>>(), vec![0.8, 0.4, -0.4, -0.2]);
        // Stereo in: the two channels are mixed to one, then placed; a frame cut short
        // at the end is dropped.
        let stereo = PcmSound { samples: vec![0.2, 0.6, 1.0, 1.0, 0.5].into(), channels: 2, rate: 22050, pan: Some(pan.clone()), reverb: None };
        assert_eq!(stereo.decoder().collect::<Vec<_>>(), vec![0.4, 0.2, 1.0, 0.5]);
        // Not placed: as it is.
        let plain = PcmSound { pan: None, ..mono.clone() };
        assert_eq!(plain.decoder().channels().get(), 1);
        assert_eq!(plain.decoder().collect::<Vec<_>>(), vec![0.8, -0.4]);
        // Another module's source, panned.
        let other = Panned::new(plain, pan);
        assert_eq!(other.decoder().channels().get(), 2);
        assert_eq!(other.decoder().collect::<Vec<_>>(), vec![0.8, 0.4, -0.4, -0.2]);
    }

    #[test]
    fn a_sound_through_reverb_is_stereo_with_a_tail() {
        let room = crate::reverb::preset(Some("Small Room"));
        let mono = PcmSound { samples: vec![0.5; 100].into(), channels: 1, rate: 22050, pan: None, reverb: room.map(RoomSend::full) };
        let d = mono.decoder();
        assert_eq!(d.channels().get(), 2);
        let tail = crate::reverb::Reverb::tail_seconds(room.unwrap());
        assert!((d.total_duration().unwrap().as_secs_f64() - (100.0 / 22050.0 + tail as f64)).abs() < 1e-3);
        let out: Vec<f32> = d.collect();
        assert_eq!(out.len(), 2 * (100 + (tail * 22050.0) as usize));
        // The dry sound first, on both ears, before any reflection arrives.
        assert_eq!(&out[..4], &[0.5, 0.5, 0.5, 0.5]);
        // Something rings on after the file's end.
        assert!(out[200..].iter().any(|s| s.abs() > 1e-4));
        // A panned sound keeps its pan in the dry part.
        let panned = PcmSound { pan: Some(Pan::new([1.0, 0.5])), ..mono };
        assert_eq!(&panned.decoder().collect::<Vec<_>>()[..2], &[0.5, 0.25]);
    }

    #[test]
    fn loudness_by_distance() {
        let s = Sound { min_distance: 200.0, max_distance: 1000.0, static_attenuation_db: 20.0, ..Sound::default() };
        assert!((static_gain(&s) - 0.1).abs() < 1e-6);
        assert_eq!(distance_gain(&s, 0.0), 1.0);
        assert_eq!(distance_gain(&s, 200.0), 1.0);
        // The default curve (100, 50, 20, 5, 0) over the 800 units from 200 to 1000.
        assert!((distance_gain(&s, 400.0) - 0.5).abs() < 1e-6);
        assert!((distance_gain(&s, 600.0) - 0.2).abs() < 1e-6);
        assert!((distance_gain(&s, 700.0) - 0.125).abs() < 1e-6);
        assert!((distance_gain(&s, 800.0) - 0.05).abs() < 1e-6);
        assert_eq!(distance_gain(&s, 1000.0), 0.0);
        assert_eq!(distance_gain(&s, 5000.0), 0.0);
        // The record's own curve: FXBulletWhiz's (100, 30, 20, 5, 0) falls faster at first.
        let whiz = Sound { attenuation_curve: Some([100.0, 30.0, 20.0, 5.0, 0.0]), ..s.clone() };
        assert!((distance_gain(&whiz, 400.0) - 0.3).abs() < 1e-6);
        // A flat one (100 at every point) is whole up to the outer radius.
        let flat = Sound { attenuation_curve: Some([100.0; 5]), ..s.clone() };
        assert_eq!(distance_gain(&flat, 999.0), 1.0);
        // No span: the inner level, then min ÷ distance.
        let point = Sound { min_distance: 200.0, max_distance: 0.0, ..Sound::default() };
        assert!((distance_gain(&point, 400.0) - 0.5).abs() < 1e-6);
    }
}
