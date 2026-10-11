//! Music in the game (docs/spec-notes/audio.md): the music of the place the player is in.
//!
//! - **Where.** Audio markers (references with `MMRK` data) give places a radius and a
//!   media location controller (ALOC), which names media sets (MSET). The marker the
//!   player is deepest inside chooses (`fnv_world::audio::marker_at`); its controller's
//!   set is picked once on entering it, at random among the list heard
//!   (`MediaController::sets`).
//! - **Layers.** A location set has three tracks for the day and three for the night,
//!   files of one length under `music\`. All three play together from the start, mixed
//!   in one source so they stay in step ([`LayeredMusic`]); the outer one sounds
//!   anywhere inside the marker's radius and the middle and inner ones are faded in as
//!   the player comes nearer the marker (`fnv_world::audio::layers`) [hypothesis: the
//!   layers add up]. A layer fades over the set's layer crossfade time, and so does a
//!   change of set or between day and night [design].
//! - **Cells and worldspaces** name a music type (MUSC); it plays when no marker reaches
//!   the player and its file exists [design]. The base game's cells name types whose
//!   folders it does not ship, so nothing is heard from them [observed].
//! - **Files** are MP3, decoded by rodio (symphonia) as they play; a set loops by
//!   starting its files again.
//! - Music goes on in menus and behind loading screens [design].
//! - Not built: battle, dungeon and incidental sets, the enemy, friend and ally sets of a
//!   controller with a faction, the controllers' delays and a set's minimum time on,
//!   the music scripts ask for (`PlayMusic`), the main menu's and the death music.
//!
//! [`MusicSettings`] turns it off (`nvforever --no-music`) and sets the volume
//! (`--music-volume`). One log line per set started and per change of layers; a
//! `music summary` line at the end.

use crate::player::Player;
use crate::sim::SimState;
use crate::{Location, PlayWorld};
use bevy::audio::{AddAudioSource, ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;
use fnv_combat::rng::SplitMix;
use fnv_world::audio::{self, AudioForms, AudioMarker, SoundMarker, LAYERS};
use std::collections::{HashMap, HashSet};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// How music behaves; insert before the play plugins to change the defaults.
#[derive(Resource, Clone, Copy, Debug)]
pub struct MusicSettings {
    pub play: bool,
    /// Linear volume, 1 = as recorded less the records' decibels.
    pub volume: f32,
}

impl Default for MusicSettings {
    fn default() -> Self {
        MusicSettings { play: true, volume: 1.0 }
    }
}

/// A space sound does not cross: a worldspace or an interior cell (FormID).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Space {
    World(u32),
    Interior(u32),
}

impl Space {
    pub fn of(loc: &Location) -> Space {
        loc.interior.map_or(Space::World(loc.world), Space::Interior)
    }
}

/// The music and acoustic-space forms and every audio marker and sound marker of the
/// load order, by the space they are in. Read once at the start.
#[derive(Resource, Default)]
pub struct AudioPlaces {
    pub forms: AudioForms,
    pub audio: HashMap<Space, Vec<AudioMarker>>,
    pub sounds: HashMap<Space, Vec<SoundMarker>>,
}

/// The cell the player is in: the interior, or the exterior cell under `eye`.
pub fn cell_at(d: &PlayWorld, loc: &Location, eye: [f32; 3]) -> Option<u32> {
    loc.interior.or_else(|| d.index.worldspaces.get(&loc.world)?.grid.get(&fnv_world::cell_of(eye[0], eye[1])).copied())
}

/// The game's hour of the day.
pub fn game_hour(sim: &SimState) -> f32 {
    sim.world.calendar().map_or(12.0, |c| c.hour).rem_euclid(24.0)
}

type Stem = rodio::Decoder<Cursor<Arc<[u8]>>>;

fn open(bytes: &Arc<[u8]>) -> Option<Stem> {
    rodio::Decoder::builder().with_byte_len(bytes.len() as u64).with_data(Cursor::new(bytes.clone())).build().ok()
}

/// Up to three music files of one format played in step, each at its own level, which
/// the game changes while they play.
#[derive(Asset, TypePath, Clone)]
pub struct LayeredMusic {
    /// The files, each with the index of its level.
    stems: Vec<(usize, Arc<[u8]>)>,
    /// Linear gains as `f32` bits.
    levels: Arc<[AtomicU32; 3]>,
    channels: ChannelCount,
    rate: SampleRate,
    /// Start again when the first file ends.
    looping: bool,
}

impl LayeredMusic {
    /// One file played as it is decoded, with a level the caller changes while it plays
    /// (the first of the three); `None` when it is not decoded.
    pub(crate) fn single(bytes: Arc<[u8]>, looping: bool) -> Option<(LayeredMusic, Arc<[AtomicU32; 3]>)> {
        let d = open(&bytes)?;
        let levels: Arc<[AtomicU32; 3]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)]);
        Some((LayeredMusic { stems: vec![(0, bytes)], levels: levels.clone(), channels: d.channels(), rate: d.sample_rate(), looping }, levels))
    }
}

pub struct LayeredDecoder {
    music: LayeredMusic,
    stems: Vec<(usize, Option<Stem>)>,
}

impl LayeredDecoder {
    /// Open every file; one that no longer opens, or whose format differs from the
    /// first's, stays silent.
    fn open(music: &LayeredMusic) -> Vec<(usize, Option<Stem>)> {
        music.stems.iter().map(|(k, b)| (*k, open(b).filter(|d| d.channels() == music.channels && d.sample_rate() == music.rate))).collect()
    }
}

impl Iterator for LayeredDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let lead = |stems: &mut Vec<(usize, Option<Stem>)>| stems.first_mut().and_then(|(_, s)| s.as_mut()).and_then(Iterator::next);
        let first = match lead(&mut self.stems) {
            Some(s) => s,
            None if self.music.looping => {
                self.stems = LayeredDecoder::open(&self.music);
                lead(&mut self.stems)?
            }
            None => return None,
        };
        let level = |k: usize| f32::from_bits(self.music.levels[k.min(2)].load(Ordering::Relaxed));
        let mut out = first * level(self.stems[0].0);
        for (k, stem) in self.stems.iter_mut().skip(1) {
            if let Some(s) = stem.as_mut().and_then(Iterator::next) {
                out += s * f32::from_bits(self.music.levels[(*k).min(2)].load(Ordering::Relaxed));
            }
        }
        Some(out)
    }
}

impl Source for LayeredDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.music.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.music.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for LayeredMusic {
    type Decoder = LayeredDecoder;

    fn decoder(&self) -> LayeredDecoder {
        LayeredDecoder { music: self.clone(), stems: LayeredDecoder::open(self) }
    }
}

/// What is to be heard: a media set by day or by night, or a music type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Key {
    Set { set: u32, night: bool },
    Type(u32),
}

/// The music wanted at a place and hour.
#[derive(Clone, Debug, PartialEq)]
pub struct Wanted {
    pub key: Key,
    pub name: String,
    /// Files as the records store them, relative to `music\`.
    pub files: [Option<String>; 3],
    pub db: [f32; 3],
    /// Which of the three sound here.
    pub on: [bool; 3],
    /// Seconds a layer takes to fade in or out.
    pub fade: f32,
    pub looping: bool,
}

/// Seconds a music type fades in and out over, and a set without a crossfade time
/// [design].
const DEFAULT_FADE: f32 = 3.0;

/// The tracks of `set` for a listener `distance` units from `marker`.
pub fn wanted_set(set: &audio::MediaSet, marker: &AudioMarker, distance: f32, day: bool) -> Wanted {
    let l = audio::layers(set, marker, distance, day);
    let base = if day { 0 } else { 3 };
    Wanted {
        key: Key::Set { set: set.form_id, night: !day },
        name: format!("{} {}", set.edid.as_deref().unwrap_or("?"), if day { "day" } else { "night" }),
        files: [0, 1, 2].map(|k| set.track(base + k).map(str::to_string)),
        db: [0, 1, 2].map(|k| set.db[base + k]),
        on: l.on,
        fade: if set.times[2] > 0.0 { set.times[2] } else { DEFAULT_FADE },
        looping: true,
    }
}

/// A music type as one track [hypothesis: `ANAM` is a level in decibels whose sign says
/// whether the file loops; its size is taken off].
pub fn wanted_type(m: &audio::MusicType) -> Option<Wanted> {
    let file = m.file.clone()?;
    Some(Wanted {
        key: Key::Type(m.form_id),
        name: m.edid.clone().unwrap_or_else(|| format!("{:08X}", m.form_id)),
        files: [Some(file), None, None],
        db: [-m.db.abs(), 0.0, 0.0],
        on: [true, false, false],
        fade: DEFAULT_FADE,
        looping: m.db > 0.0,
    })
}

/// One step of a fade: `level` moved towards `target` by `dt` seconds of a fade that
/// takes `fade` seconds from silence to full.
pub fn fade_step(level: f32, target: f32, dt: f32, fade: f32) -> f32 {
    let step = if fade > 0.0 { dt / fade } else { 1.0 };
    level + (target - level).clamp(-step, step)
}

/// Music being played.
struct Track {
    key: Key,
    name: String,
    /// `None`: no file of it could be played.
    entity: Option<Entity>,
    levels: Arc<[AtomicU32; 3]>,
    /// How far each layer is faded in, 0–1.
    level: [f32; 3],
    target: [f32; 3],
    /// Linear gain of each layer when fully in.
    gain: [f32; 3],
    fade: f32,
}

#[derive(Component)]
struct MusicSound;

#[derive(Resource)]
pub struct Music {
    /// File bytes by archive path; `None`: not there.
    files: HashMap<String, Option<Arc<[u8]>>>,
    folders: HashMap<String, Vec<String>>,
    /// CELL `XCMO` and WRLD `ZNAM` by FormID.
    types: HashMap<u32, Option<u32>>,
    current: Option<Track>,
    leaving: Vec<Track>,
    /// The marker the player is in and the set picked for it.
    marker: Option<(u32, Option<u32>)>,
    reported: HashSet<String>,
    rng: SplitMix,
    pub started: u32,
    pub layer_changes: u32,
    pub failed: u32,
}

impl Default for Music {
    fn default() -> Self {
        Music {
            files: HashMap::new(),
            folders: HashMap::new(),
            types: HashMap::new(),
            current: None,
            leaving: Vec::new(),
            marker: None,
            reported: HashSet::new(),
            rng: SplitMix(0x6d75_7369),
            started: 0,
            layer_changes: 0,
            failed: 0,
        }
    }
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let t = std::time::Instant::now();
    let forms = AudioForms::build(d.lo, d.schema);
    let (am, sm) = audio::markers(d.lo, d.schema, d.index, d.index.cells.keys().copied());
    let space = |cell: u32| d.index.cells.get(&cell).and_then(|c| c.world).map_or(Space::Interior(cell), Space::World);
    let mut places = AudioPlaces { forms, ..Default::default() };
    let (n_audio, n_sounds) = (am.len(), sm.len());
    for m in am {
        places.audio.entry(space(m.cell)).or_default().push(m);
    }
    for m in sm {
        places.sounds.entry(space(m.cell)).or_default().push(m);
    }
    info!(
        "music: {} media sets, {} controllers, {} acoustic spaces, {n_audio} audio markers and {n_sounds} sound markers read in {:.0} ms",
        places.forms.sets.len(),
        places.forms.controllers.len(),
        places.forms.spaces.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    commands.insert_resource(places);
}

impl Music {
    /// The archive path of a music file as a record stores it; a folder stands for one
    /// of the `.mp3` files in it [hypothesis].
    fn path(&mut self, vfs: &fnv_vfs::Vfs, file: &str) -> Option<String> {
        let path = fnv_vfs::normalize(&format!("music\\{}", file.trim_start_matches(['\\', '/'])));
        if !path.ends_with('\\') {
            return Some(path);
        }
        let files = self.folders.entry(path.clone()).or_insert_with(|| vfs.paths(|p| p.starts_with(path.as_str()) && p.ends_with(".mp3") && !p[path.len()..].contains('\\')));
        if files.is_empty() {
            return None;
        }
        let k = (self.rng.next_f32() * files.len() as f32) as usize;
        Some(files[k.min(files.len() - 1)].clone())
    }

    fn bytes(&mut self, vfs: &fnv_vfs::Vfs, file: &str) -> Option<Arc<[u8]>> {
        let Some(path) = self.path(vfs, file) else {
            if self.reported.insert(file.to_string()) {
                warn!("music: no files in music\\{file}");
            }
            return None;
        };
        if !self.files.contains_key(&path) {
            let read = match vfs.read(&path) {
                Some(Ok(b)) => Some(Arc::<[u8]>::from(b)),
                Some(Err(e)) => {
                    warn!("music: {path}: {e}");
                    None
                }
                None => {
                    warn!("music: {path}: not in the game's files");
                    None
                }
            };
            self.files.insert(path.clone(), read);
        }
        self.files[&path].clone()
    }

    /// Start what is wanted, silent, with its layers fading in.
    fn start(&mut self, commands: &mut Commands, vfs: &fnv_vfs::Vfs, assets: &mut Assets<LayeredMusic>, w: &Wanted) -> Track {
        let levels: Arc<[AtomicU32; 3]> = Arc::new([AtomicU32::new(0), AtomicU32::new(0), AtomicU32::new(0)]);
        let mut stems: Vec<(usize, Arc<[u8]>)> = Vec::new();
        let mut format: Option<(ChannelCount, SampleRate)> = None;
        for (k, file) in w.files.iter().enumerate() {
            let Some(file) = file else { continue };
            let Some(bytes) = self.bytes(vfs, file) else {
                self.failed += 1;
                continue;
            };
            match (open(&bytes).map(|d| (d.channels(), d.sample_rate())), format) {
                (Some(f), None) => format = Some(f),
                (Some(f), Some(first)) if f == first => {}
                (f, _) => {
                    self.failed += 1;
                    if self.reported.insert(file.clone()) {
                        warn!("music: {file}: {}", if f.is_some() { "another format than the set's first file" } else { "not decoded" });
                    }
                    continue;
                }
            }
            stems.push((k, bytes));
        }
        let entity = format.map(|(channels, rate)| {
            let music = assets.add(LayeredMusic { stems, levels: levels.clone(), channels, rate, looping: w.looping });
            commands.spawn((AudioPlayer::<LayeredMusic>(music), PlaybackSettings::DESPAWN, MusicSound)).id()
        });
        self.started += 1;
        let layers: Vec<&str> = (0..3).filter(|&k| w.on[k] && w.files[k].is_some()).map(|k| LAYERS[k].trim_start_matches("day_")).collect();
        match entity {
            Some(_) => info!("music: {} started, layers {}", w.name, layers.join("+")),
            None => info!("music: {}: none of its files can be played", w.name),
        }
        Track { key: w.key, name: w.name.clone(), entity, levels, level: [0.0; 3], target: w.on.map(|on| on as u8 as f32), gain: w.db.map(|db| 10f32.powf(db / 20.0)), fade: w.fade }
    }
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<MusicSettings>,
    music: Option<ResMut<Music>>,
    places: Option<Res<AudioPlaces>>,
    d: Res<PlayWorld>,
    loc: Res<Location>,
    sim: Res<SimState>,
    player: Option<Res<Player>>,
    data: Option<Res<nvf_assets::GameData>>,
    mut assets: ResMut<Assets<LayeredMusic>>,
) {
    let (Some(mut music), Some(places), Some(player), Some(data)) = (music, places, player, data) else { return };
    let music = &mut *music;
    let eye = player.eye();
    let hour = game_hour(&sim);
    // The marker the player is in, and the set picked when it was entered.
    let markers = places.audio.get(&Space::of(&loc)).map_or(&[][..], Vec::as_slice);
    let at = audio::marker_at(markers, eye, |m| sim.world.is_enabled(fnv_sim::FormId(m.ref_id)).unwrap_or(!m.initially_disabled));
    let controller = at.and_then(|(m, _)| places.forms.controllers.get(&m.controller?));
    if music.marker.map(|m| m.0) != at.map(|(m, _)| m.ref_id) {
        music.marker = at.map(|(m, _)| {
            let sets = controller.map_or(&[][..], |c| c.sets());
            let pick = (!sets.is_empty()).then(|| sets[((music.rng.next_f32() * sets.len() as f32) as usize).min(sets.len() - 1)]);
            (m.ref_id, pick)
        });
        match (at, controller) {
            (Some((m, distance)), Some(c)) => info!(
                "music: in marker {:08X} of {}, {distance:.0} of {:.0} units from it, {} sets to choose from",
                m.ref_id,
                c.edid.as_deref().unwrap_or("?"),
                m.radius,
                c.sets().len()
            ),
            (Some((m, _)), None) => info!("music: in marker {:08X}, whose controller is not in the load order", m.ref_id),
            (None, _) => info!("music: outside every audio marker"),
        }
    }
    let from_marker = at.zip(controller).and_then(|((m, distance), c)| {
        let set = places.forms.sets.get(&music.marker?.1?)?;
        Some(wanted_set(set, m, distance, c.is_day(hour)))
    });
    let wanted = from_marker.or_else(|| {
        let cell = cell_at(&d, &loc, eye);
        let of_cell = cell.and_then(|c| *music.types.entry(c).or_insert_with(|| audio::cell_audio(d.lo, d.schema, c).music));
        let of_world = || loc.interior.is_none().then(|| *music.types.entry(loc.world).or_insert_with(|| audio::world_music(d.lo, d.schema, loc.world))).flatten();
        wanted_type(places.forms.music.get(&of_cell.or_else(of_world)?)?)
    });
    // A change of music: what plays fades out, what is wanted starts.
    if music.current.as_ref().map(|t| t.key) != wanted.as_ref().map(|w| w.key) {
        if let Some(mut old) = music.current.take() {
            old.target = [0.0; 3];
            info!("music: {} fades out", old.name);
            music.leaving.push(old);
        }
        if let Some(w) = &wanted {
            let t = music.start(&mut commands, &data.vfs, &mut assets, w);
            music.current = Some(t);
        }
    }
    if let (Some(t), Some(w)) = (music.current.as_mut(), &wanted) {
        let target = w.on.map(|on| on as u8 as f32);
        if t.target != target {
            t.target = target;
            music.layer_changes += 1;
            let layers: Vec<&str> = (0..3).filter(|&k| w.on[k] && w.files[k].is_some()).map(|k| LAYERS[k].trim_start_matches("day_")).collect();
            info!("music: {} layers {}", t.name, if layers.is_empty() { "none".to_string() } else { layers.join("+") });
        }
    }
    let dt = time.delta_secs();
    let volume = if settings.play { settings.volume.max(0.0) } else { 0.0 };
    for t in music.current.iter_mut().chain(music.leaving.iter_mut()) {
        for k in 0..3 {
            t.level[k] = fade_step(t.level[k], t.target[k], dt, t.fade);
            t.levels[k].store((t.level[k] * t.gain[k] * volume).to_bits(), Ordering::Relaxed);
        }
    }
    music.leaving.retain(|t| {
        let over = t.level.iter().all(|l| *l <= 0.0);
        if let (true, Some(e)) = (over, t.entity) {
            commands.entity(e).try_despawn();
        }
        !over
    });
}

fn report(mut exit: MessageReader<AppExit>, music: Option<Res<Music>>) {
    if let (Some(_), Some(m)) = (exit.read().last(), music) {
        info!("music summary: {} started, {} changes of layers, {} files not played{}", m.started, m.layer_changes, m.failed, m.current.as_ref().map_or(String::new(), |t| format!("; playing {}", t.name)));
    }
}

pub struct MusicPlugin;

impl Plugin for MusicPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MusicSettings>()
            .init_resource::<Music>()
            .add_audio_source::<LayeredMusic>()
            .add_systems(Startup, init)
            .add_systems(Update, play.after(crate::PlaySet::Activation))
            .add_systems(Last, report);
        crate::settings::add_setting(
            app,
            crate::settings::Setting {
                key: "music",
                group: crate::pause::GROUP_MENUS,
                label: "Music",
                note: "On: the music of the place you are in plays. Off: it is silent and goes on unheard. (--no-music)",
                get: |w| w.resource::<MusicSettings>().play,
                set: |w, on| w.resource_mut::<MusicSettings>().play = on,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::audio::{MediaSet, MusicType};

    fn marker(radius: f32) -> AudioMarker {
        AudioMarker { ref_id: 1, cell: 1, position: [0.0; 3], radius, name: None, controller: Some(2), use_controller_values: true, layer2_percent: 0.33, layer3_percent: 0.67, initially_disabled: false }
    }

    #[test]
    fn a_set_by_day_and_by_night() {
        let set = MediaSet {
            form_id: 7,
            edid: Some("musSet".into()),
            tracks: ["d1", "d2", "d3", "n1", "n2", "n3"].map(|s| Some(s.to_string())),
            db: [-6.0, -6.0, -6.0, -3.0, -3.0, -6.0],
            boundary_percent: [100.0, 60.0, 20.0, 100.0, 60.0, 20.0],
            // The night's inner track is not enabled.
            enabled: 0b011111,
            times: [6.0, 15.0, 9.0, 0.0],
            ..Default::default()
        };
        let w = wanted_set(&set, &marker(1000.0), 500.0, true);
        assert_eq!(w.key, Key::Set { set: 7, night: false });
        assert_eq!(w.files, ["d1", "d2", "d3"].map(|s| Some(s.to_string())));
        assert_eq!((w.on, w.fade, w.looping), ([true, true, false], 9.0, true));
        let w = wanted_set(&set, &marker(1000.0), 100.0, false);
        assert_eq!(w.key, Key::Set { set: 7, night: true });
        assert_eq!(w.files, [Some("n1".to_string()), Some("n2".to_string()), None]);
        assert_eq!((w.db, w.on), ([-3.0, -3.0, -6.0], [true, true, true]));
        assert_eq!(w.name, "musSet night");
    }

    #[test]
    fn a_music_type_is_one_track() {
        let w = wanted_type(&MusicType { form_id: 9, edid: Some("musDoc".into()), file: Some("loc\\a.mp3".into()), db: 6.0 }).unwrap();
        assert_eq!((w.key, w.db[0], w.looping, w.on), (Key::Type(9), -6.0, true, [true, false, false]));
        // A negative level: played once.
        assert!(!wanted_type(&MusicType { form_id: 9, edid: None, file: Some("scr\\b.mp3".into()), db: -7.74 }).unwrap().looping);
        // No file ("1NoMusic"): nothing to play.
        assert!(wanted_type(&MusicType::default()).is_none());
    }

    #[test]
    fn fades_take_their_time() {
        // A 9 second fade, a tenth of a second at a time.
        let mut l = 0.0;
        for _ in 0..45 {
            l = fade_step(l, 1.0, 0.1, 9.0);
        }
        assert!((l - 0.5).abs() < 1e-4);
        for _ in 0..60 {
            l = fade_step(l, 1.0, 0.1, 9.0);
        }
        assert_eq!(l, 1.0);
        assert!((fade_step(1.0, 0.0, 0.5, 2.0) - 0.75).abs() < 1e-6);
        // No fade time: at once.
        assert_eq!(fade_step(0.2, 1.0, 0.016, 0.0), 1.0);
    }

    /// A WAV file of 16-bit samples, for the decoder.
    fn wav(samples: &[i16]) -> Arc<[u8]> {
        let mut v = b"RIFF".to_vec();
        v.extend((36 + samples.len() as u32 * 2).to_le_bytes());
        v.extend(b"WAVEfmt ");
        v.extend(16u32.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(1u16.to_le_bytes());
        v.extend(8000u32.to_le_bytes());
        v.extend(16000u32.to_le_bytes());
        v.extend(2u16.to_le_bytes());
        v.extend(16u16.to_le_bytes());
        v.extend(b"data");
        v.extend((samples.len() as u32 * 2).to_le_bytes());
        for s in samples {
            v.extend(s.to_le_bytes());
        }
        v.into()
    }

    #[test]
    fn layers_are_mixed_in_step_and_loop() {
        let (a, b) = (wav(&[8192, 16384, -8192, 0]), wav(&[16384, 16384, 16384, 16384]));
        let first = open(&a).expect("the test's WAV decodes");
        let levels: Arc<[AtomicU32; 3]> = Arc::new([AtomicU32::new(1f32.to_bits()), AtomicU32::new(0), AtomicU32::new(0.5f32.to_bits())]);
        let music = LayeredMusic { stems: vec![(0, a), (2, b)], levels: levels.clone(), channels: first.channels(), rate: first.sample_rate(), looping: true };
        let mut d = music.decoder();
        let got: Vec<f32> = (0..6).map(|_| d.next().unwrap()).collect();
        // The first file at full level and the third at half; then both from the start.
        for (g, want) in got.iter().zip([0.5, 0.75, 0.0, 0.25, 0.5, 0.75]) {
            assert!((g - want).abs() < 1e-3, "{got:?}");
        }
        // A level changed while it plays.
        levels[0].store(0f32.to_bits(), Ordering::Relaxed);
        assert!((d.next().unwrap() - 0.25).abs() < 1e-3);
        // Not looping: over with its first file.
        let once = LayeredMusic { looping: false, ..music };
        assert_eq!(once.decoder().count(), 4);
    }
}
