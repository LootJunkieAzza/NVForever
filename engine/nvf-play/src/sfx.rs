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
//! - **Loudness** [design; the record's five-point attenuation curve is not used]: the
//!   static attenuation is taken off (gain 10^(−dB ÷ 20)). A sound with a place is at
//!   full gain within its minimum distance of the listener (the player's eye), falls as
//!   minimum ÷ distance beyond it and fades to nothing at its maximum distance.
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
//! - Not built: reverb. The sounds animations and doors name in their text keys
//!   (reloads, footsteps, armour rustle, doors) are sent here by nvforever's
//!   `anim_sounds.rs` (docs/spec-notes/audio.md §10). Placed sounds and the cells'
//!   background loops are [`crate::ambient`]; music is [`crate::music`].
//!
//! [`SfxSettings`] turns them off (`nvforever --no-sound`) and sets the volume.

use crate::player::Player;
use crate::PlayWorld;
use bevy::audio::{ChannelCount, Decodable, Sample, SampleRate, Source, Volume};
use crate::audio_out::{AddAudioSource, KeepsSpeed, SoundCategory};
use bevy::prelude::*;
use fnv_combat::fx::{FxForms, Sound};
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
}

impl Default for SfxSettings {
    fn default() -> Self {
        SfxSettings { play: true, volume: 1.0 }
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
fn pan_frame(frame: impl Iterator<Item = Sample>, channels: usize, pan: &Pan) -> Option<(Sample, Sample)> {
    let mut sum = 0.0;
    let mut n = 0;
    for s in frame.take(channels.max(1)) {
        sum += s;
        n += 1;
    }
    if n == 0 {
        return None;
    }
    let m = sum / channels.max(1) as f32;
    let [l, r] = pan.get();
    Some((m * l, m * r))
}

/// Another module's sound played at a place: its source, mixed to one channel and
/// panned, in stereo.
#[derive(Asset, TypePath, Clone)]
pub struct Panned<T: Decodable + TypePath + Clone> {
    pub inner: T,
    pub pan: Pan,
}

pub struct PanDecoder<S> {
    input: S,
    pan: Pan,
    right: Option<Sample>,
}

impl<S: Source> Iterator for PanDecoder<S> {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let c = self.input.channels().get() as usize;
        let (l, r) = pan_frame(&mut self.input, c, &self.pan)?;
        self.right = Some(r);
        Some(l)
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
        self.input.total_duration()
    }
}

impl<T: Decodable + TypePath + Clone> Decodable for Panned<T> {
    type Decoder = PanDecoder<T::Decoder>;

    fn decoder(&self) -> Self::Decoder {
        PanDecoder { input: self.inner.decoder(), pan: self.pan.clone(), right: None }
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
}

pub struct PcmDecoder {
    sound: PcmSound,
    at: usize,
    right: Option<Sample>,
}

impl Iterator for PcmDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let Some(pan) = &self.sound.pan else {
            let s = self.sound.samples.get(self.at).copied();
            self.at += 1;
            return s;
        };
        if let Some(r) = self.right.take() {
            return Some(r);
        }
        let c = self.sound.channels.max(1) as usize;
        let frame = self.sound.samples.get(self.at..self.at + c)?;
        self.at += c;
        let (l, r) = pan_frame(frame.iter().copied(), c, pan)?;
        self.right = Some(r);
        Some(l)
    }
}

impl Source for PcmDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        let c = if self.sound.pan.is_some() { 2 } else { self.sound.channels };
        NonZero::new(c).unwrap_or(NonZero::<u16>::MIN)
    }

    fn sample_rate(&self) -> SampleRate {
        NonZero::new(self.sound.rate).unwrap_or(NonZero::<u32>::MIN)
    }

    fn total_duration(&self) -> Option<Duration> {
        Some(Duration::from_secs_f64(self.sound.seconds()))
    }
}

impl Decodable for PcmSound {
    type Decoder = PcmDecoder;

    fn decoder(&self) -> PcmDecoder {
        PcmDecoder { sound: self.clone(), at: 0, right: None }
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
        Ok(PcmSound { samples: d.collect::<Vec<f32>>().into(), channels, rate, pan: None })
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
                    return Ok(PcmSound { samples: samples.into(), channels, rate, pan: None });
                }
                _ => {}
            }
            at += 8 + len + (len & 1);
        }
        Err("no data chunk".into())
    }
}

/// The gain of a sound `distance` units from the listener (see the module notes).
pub fn distance_gain(s: &Sound, distance: f32) -> f32 {
    let (min, max) = (s.min_distance.max(1.0), s.max_distance);
    if max > 0.0 && distance >= max {
        return 0.0;
    }
    let near = (min / distance.max(min)).min(1.0);
    let fade = if max > min && distance > min { 1.0 - (distance - min) / (max - min) } else { 1.0 };
    near * fade
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
}

#[derive(Component)]
struct SfxSound;

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
    commands.insert_resource(Sfx { forms, files: HashMap::new(), folders: HashMap::new(), reported: HashSet::new(), rng: SplitMix(0x5f37_59df), played: 0, silent: 0, failed: 0 });
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
    if !settings.play || gain < 0.001 {
        sfx.silent += 1;
        return Some(seconds);
    }
    sfx.played += 1;
    let playback = PlaybackSettings::DESPAWN.with_volume(Volume::Linear(gain)).with_speed(speed);
    let category = SoundCategory::of_effect(s.edid.as_deref());
    let mut e = match placed.and_then(|p| assets.get(&handle).map(|pcm| (PcmSound { pan: Some(p.pan.clone()), ..pcm.clone() }, p))) {
        // This playing's own asset: the samples are shared, the pan is its own.
        Some((pcm, placed)) => commands.spawn((AudioPlayer::<PcmSound>(assets.add(pcm)), playback, placed, SfxSound, category)),
        None => commands.spawn((AudioPlayer::<PcmSound>(handle), playback, SfxSound, category)),
    };
    if is_interface(s.edid.as_deref()) {
        e.insert(KeepsSpeed);
    }
    Some(seconds)
}

/// An interface sound by its SOUN editor ID (`UI…`: menus, VATS, the Pip-Boy), which keeps
/// its speed in slow motion (`audio_out::SoundSpeed`) [hypothesis: the base game's
/// interface records are named so, docs/spec-notes/audio.md].
fn is_interface(edid: Option<&str>) -> bool {
    edid.and_then(|e| e.get(..2)).is_some_and(|p| p.eq_ignore_ascii_case("UI"))
}

fn report(mut exit: MessageReader<AppExit>, sfx: Option<Res<Sfx>>) {
    if let (Some(_), Some(s)) = (exit.read().last(), sfx) {
        if s.played + s.silent + s.failed > 0 {
            info!("sfx: {} sounds played, {} too quiet or switched off, {} without a file we can play", s.played, s.silent, s.failed);
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
            .add_systems(Update, (play.after(crate::PlaySet::Activation), pan_sounds).chain())
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let mono = PcmSound { samples: vec![0.8, -0.4].into(), channels: 1, rate: 22050, pan: Some(pan.clone()) };
        let d = mono.decoder();
        assert_eq!(d.channels().get(), 2);
        assert_eq!(d.collect::<Vec<_>>(), vec![0.8, 0.4, -0.4, -0.2]);
        // Stereo in: the two channels are mixed to one, then placed; a frame cut short
        // at the end is dropped.
        let stereo = PcmSound { samples: vec![0.2, 0.6, 1.0, 1.0, 0.5].into(), channels: 2, rate: 22050, pan: Some(pan.clone()) };
        assert_eq!(stereo.decoder().collect::<Vec<_>>(), vec![0.4, 0.2, 1.0, 0.5]);
        // Not placed: as it is.
        let plain = PcmSound { pan: None, ..mono.clone() };
        assert_eq!(plain.decoder().channels().get(), 1);
        assert_eq!(plain.decoder().collect::<Vec<_>>(), vec![0.8, -0.4]);
        // Another module's source, panned.
        let other = Panned { inner: plain, pan };
        assert_eq!(other.decoder().channels().get(), 2);
        assert_eq!(other.decoder().collect::<Vec<_>>(), vec![0.8, 0.4, -0.4, -0.2]);
    }

    #[test]
    fn loudness_by_distance() {
        let s = Sound { min_distance: 200.0, max_distance: 1000.0, static_attenuation_db: 20.0, ..Sound::default() };
        assert!((static_gain(&s) - 0.1).abs() < 1e-6);
        assert_eq!(distance_gain(&s, 0.0), 1.0);
        assert_eq!(distance_gain(&s, 200.0), 1.0);
        // Half way out: 200 ÷ 600 of the level, half faded.
        assert!((distance_gain(&s, 600.0) - (200.0 / 600.0) * 0.5).abs() < 1e-6);
        assert_eq!(distance_gain(&s, 1000.0), 0.0);
        assert_eq!(distance_gain(&s, 5000.0), 0.0);
    }
}
