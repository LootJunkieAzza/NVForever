//! Radios in the world play their station (docs/spec-notes/radio.md).
//!
//! - **Receivers** are placed activators whose base names a radio station
//!   (`fnv_world::radio`). One plays while it is switched on (`fnv_game::radio`: a default
//!   activation switches it, the "StartOn" scripts activate their radio once on load),
//!   is enabled, is not destroyed (SetDestroyed: Goodsprings' broken radio) and its
//!   station broadcasts (SetBroadcastState, else the station's Continuous Broadcast flag).
//! - **The station's programme** is dialogue: its placed reference says the lines of its
//!   radio quests one after another (`Game::radio_next`, begin and end result scripts
//!   run, so the quests' "last played" variables rotate the songs). One programme per
//!   station, shared by all its receivers: a receiver that starts hearing it mid-line
//!   joins at the point the line has reached [design]. A station's programme starts when
//!   one of its receivers is first heard and runs on from then, heard or not [design: a
//!   station nobody has heard yet runs no scripts].
//! - **What a line plays**: a response with a sound form plays that sound's file — for a
//!   song its `_mono.ogg` beside the `.mp3` the record names [documented: GECK wiki
//!   "Creating Radio Stations": in-world radios play the mono file; observed: all 48 songs
//!   have one in `Fallout - Sound.bsa`], and the sound wins over a voice file [documented:
//!   GECK wiki Radio Stations: a song "will play instead of the info's voice file"];
//!   otherwise the response's voice file in the station's voice type; with neither, the
//!   line lasts its fitted length in silence.
//! - **Loudness** [design]: the sound's static attenuation, and by distance from the
//!   receiver the radio template's (the station's `INAM`, else the receiver's) minimum and
//!   maximum distance as `crate::sfx::distance_gain` uses them; without either template
//!   150 and 1800 units (the AMLRadio template's); panned as the sound effects are
//!   (`crate::sfx::Panned`). A receiver is heard only
//!   within that maximum distance.
//! - The Esc pause menu and a loading screen hold the receivers and the programmes, as
//!   for the placed loops (`crate::ambient`).
//! - Log: `radio: <receiver> tuned to <station> playing <file>` when a receiver starts a
//!   response; `radio summary: …` at the end.
//! - Not built: the Pip-Boy radio, a station's broadcast range (it decides what a Pip-Boy
//!   receives, radio.md §2), static, StartRadioConversation, SetNPCRadio.

use crate::music::Space;
use crate::pause::Paused;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::{distance_gain, static_gain, Panned, Sfx, SfxSettings, SoundAt};
use crate::sim::SimState;
use crate::voice::VoiceState;
use crate::{Location, PlayState, PlayWorld};
use bevy::audio::{AddAudioSource, AudioSinkPlayback, ChannelCount, Decodable, Sample, SampleRate, Source};
use bevy::prelude::*;
use fnv_combat::fx::Sound;
use fnv_sim::FormId;
use fnv_world::radio::{self, RadioForms, Receiver};
use std::collections::{BTreeSet, HashMap, HashSet};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Minimum and maximum distance without a radio template [design: AMLRadio's values].
const DEFAULT_MIN: f32 = 150.0;
const DEFAULT_MAX: f32 = 1800.0;
/// A receiver stops once the player is this many times its maximum distance away.
const LEAVE: f32 = 1.05;
/// Seconds a station with nothing to say waits before it asks again.
const RETRY: f64 = 5.0;
/// Silence between two lines, seconds [design].
const GAP: f64 = 0.5;
/// A receiver joining a response later than this many seconds into it seeks to there.
const JOIN: f64 = 0.25;

// ---- playback -------------------------------------------------------------------------

type Stem = rodio::Decoder<Cursor<Arc<[u8]>>>;

fn open(bytes: &Arc<[u8]>) -> Option<Stem> {
    rodio::Decoder::builder().with_byte_len(bytes.len() as u64).with_seekable(true).with_data(Cursor::new(bytes.clone())).build().ok()
}

/// One file played from a point in it, at a level changed while it plays.
#[derive(Asset, TypePath, Clone)]
pub struct RadioSound {
    bytes: Arc<[u8]>,
    start: Duration,
    level: Arc<AtomicU32>,
    channels: ChannelCount,
    rate: SampleRate,
}

impl RadioSound {
    fn new(bytes: Arc<[u8]>, start: f64) -> Option<(RadioSound, Arc<AtomicU32>)> {
        let d = open(&bytes)?;
        let level = Arc::new(AtomicU32::new(0));
        Some((RadioSound { bytes, start: Duration::from_secs_f64(start.max(0.0)), level: level.clone(), channels: d.channels(), rate: d.sample_rate() }, level))
    }
}

pub struct RadioDecoder {
    stem: Option<Stem>,
    level: Arc<AtomicU32>,
    channels: ChannelCount,
    rate: SampleRate,
}

impl Iterator for RadioDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = self.stem.as_mut()?.next()?;
        Some(s * f32::from_bits(self.level.load(Ordering::Relaxed)))
    }
}

impl Source for RadioDecoder {
    fn current_span_len(&self) -> Option<usize> {
        None
    }

    fn channels(&self) -> ChannelCount {
        self.channels
    }

    fn sample_rate(&self) -> SampleRate {
        self.rate
    }

    fn total_duration(&self) -> Option<Duration> {
        None
    }
}

impl Decodable for RadioSound {
    type Decoder = RadioDecoder;

    fn decoder(&self) -> RadioDecoder {
        let mut stem = open(&self.bytes);
        if let Some(s) = stem.as_mut().filter(|_| !self.start.is_zero()) {
            if s.try_seek(self.start).is_err() {
                // Decode and drop up to the point instead.
                let n = (self.start.as_secs_f64() * self.rate.get() as f64) as usize * self.channels.get() as usize;
                for _ in 0..n {
                    if s.next().is_none() {
                        break;
                    }
                }
            }
        }
        RadioDecoder { stem, level: self.level.clone(), channels: self.channels, rate: self.rate }
    }
}

// ---- the data -----------------------------------------------------------------------------

/// The radio forms and the placed receivers by space; the placed reference of each station.
#[derive(Resource, Default)]
pub struct RadioPlaces {
    pub forms: RadioForms,
    pub receivers: HashMap<Space, Vec<Receiver>>,
    /// Station base → its placed reference (the lowest FormID when there are several).
    pub station_ref: HashMap<u32, u32>,
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let t = std::time::Instant::now();
    let forms = RadioForms::build(d.lo, d.schema);
    let (stations, receivers) = radio::placed(d.lo, d.schema, d.index, &forms, d.index.cells.keys().copied());
    let space = |cell: u32| d.index.cells.get(&cell).and_then(|c| c.world).map_or(Space::Interior(cell), Space::World);
    let mut places = RadioPlaces { forms, ..Default::default() };
    for s in &stations {
        places.station_ref.entry(s.base).or_insert(s.ref_id);
    }
    let n = receivers.len();
    for r in receivers {
        places.receivers.entry(space(r.cell)).or_default().push(r);
    }
    info!(
        "radio: {} stations ({} placed), {} receiver forms, {n} receivers read in {:.0} ms",
        places.forms.stations.values().filter(|s| s.is_radio()).count(),
        places.station_ref.len(),
        places.forms.receivers.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    commands.insert_resource(places);
}

// ---- the programmes -----------------------------------------------------------------------

/// One response of a line on the air.
#[derive(Debug, Clone)]
struct Part {
    response: usize,
    path: Option<String>,
    seconds: f64,
    song: bool,
    gain: f32,
}

#[derive(Debug, Clone)]
struct OnAir {
    info: FormId,
    parts: Vec<Part>,
    part: usize,
    /// Radio clock at which the current part started.
    started: f64,
}

#[derive(Debug, Default)]
struct Programme {
    on_air: Option<OnAir>,
    last: Option<FormId>,
    /// Radio clock before which the station does not look for a line.
    wait_until: f64,
}

/// A receiver sounding.
struct Playing {
    entity: Entity,
    level: Arc<AtomicU32>,
    /// What it plays: the line and the part.
    info: FormId,
    part: usize,
}

#[derive(Component)]
struct RadioEntity;

#[derive(Resource, Default)]
pub struct Radio {
    /// Seconds the programmes have run (stands still while held).
    clock: f64,
    programmes: HashMap<u32, Programme>,
    playing: HashMap<u32, Playing>,
    files: HashMap<String, Option<Arc<[u8]>>>,
    lengths: HashMap<String, f64>,
    reported: HashSet<String>,
    space: Option<Space>,
    /// Times each receiver was switched, as last seen.
    switched: HashMap<u32, u32>,
    /// Destroyed receivers already reported.
    silent: HashSet<u32>,
    pub heard: BTreeSet<u32>,
    pub songs: u32,
    pub dj_lines: u32,
    pub silent_lines: u32,
    pub missing: u32,
}

impl Radio {
    fn bytes(&mut self, vfs: &fnv_vfs::Vfs, path: &str) -> Option<Arc<[u8]>> {
        self.files
            .entry(path.to_string())
            .or_insert_with(|| match vfs.read(path) {
                Some(Ok(b)) => Some(b.into()),
                Some(Err(e)) => {
                    warn!("radio: {path}: {e}");
                    None
                }
                None => None,
            })
            .clone()
    }

    /// Seconds a file lasts: from the Ogg or WAV headers, else by decoding it.
    fn length(&mut self, vfs: &fnv_vfs::Vfs, path: &str) -> Option<f64> {
        if let Some(&s) = self.lengths.get(path) {
            return Some(s);
        }
        let bytes = self.bytes(vfs, path)?;
        let secs = match fnv_voice::sound_info(&bytes) {
            Ok(i) => i.seconds,
            Err(_) => {
                let d = open(&bytes)?;
                let (ch, rate) = (d.channels().get() as f64, d.sample_rate().get() as f64);
                d.count() as f64 / (ch * rate)
            }
        };
        self.lengths.insert(path.to_string(), secs);
        Some(secs)
    }
}

/// The file a song plays from a receiver: the `_mono.ogg` beside the named file when the
/// archives have it, else the named file (see the module notes).
pub fn receiver_file(named: &str, exists: impl Fn(&str) -> bool) -> String {
    if let Some(stem) = named.strip_suffix(".mp3").or_else(|| named.strip_suffix(".wav")) {
        let mono = format!("{stem}_mono.ogg");
        if exists(&mono) {
            return mono;
        }
    }
    named.to_string()
}

/// The template a receiver is heard by: the station's, the receiver's, or the default
/// [documented: GECK wiki Activator: the template "must be applied to Talking Activator
/// to have proper effect"].
fn template(places: &RadioPlaces, sfx: &Sfx, r: &Receiver) -> Sound {
    let base = places.forms.receivers.get(&r.base);
    let id = places.forms.stations.get(&r.station).and_then(|s| s.template).or_else(|| base.and_then(|b| b.template));
    let found = id.and_then(|id| sfx.forms.sounds.get(&FormId(id))).filter(|s| s.max_distance > 0.0);
    found.cloned().unwrap_or(Sound { min_distance: DEFAULT_MIN, max_distance: DEFAULT_MAX, ..Sound::default() })
}

fn distance(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn name(places: &RadioPlaces, station: u32) -> String {
    places.forms.stations.get(&station).and_then(|s| s.edid.clone()).unwrap_or_else(|| format!("{station:08X}"))
}

/// The parts of a line: per response in the order said, its file and length.
#[allow(clippy::too_many_arguments)]
fn parts(radio: &mut Radio, g: &fnv_game::Game, sfx: &Sfx, bank: &fnv_voice::VoiceBank, vfs: &fnv_vfs::Vfs, base: u32, info: FormId) -> Vec<Part> {
    let Some(def) = g.data.infos.get(&info) else { return Vec::new() };
    let mut out = Vec::new();
    for r in def.response_order() {
        let resp = &def.responses[r];
        if let Some(s) = resp.sound.and_then(|id| sfx.forms.sounds.get(&id)) {
            let named = s.file.as_deref().map(|f| fnv_vfs::normalize(&format!("sound\\{}", f.trim_start_matches(['\\', '/']))));
            let path = named.map(|n| receiver_file(&n, |p| vfs.exists(p)));
            let seconds = path.as_deref().and_then(|p| radio.length(vfs, p));
            match (path, seconds) {
                (Some(p), Some(secs)) => {
                    out.push(Part { response: r, path: Some(p), seconds: secs, song: true, gain: static_gain(s) });
                    continue;
                }
                (p, _) => {
                    radio.missing += 1;
                    let what = p.unwrap_or_else(|| format!("{:?}", s.edid));
                    if radio.reported.insert(what.clone()) {
                        warn!("radio: {info}: the sound's file {what} is not in the archives");
                    }
                }
            }
        }
        let path = bank.file(info.0, r, base);
        let seconds = path.as_deref().and_then(|p| bank.seconds(p)).unwrap_or_else(|| g.response_seconds_as(Some(FormId(base)), info, r));
        out.push(Part { response: r, path, seconds, song: false, gain: 1.0 });
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn play(
    mut commands: Commands,
    time: Res<Time<Real>>,
    settings: Res<SfxSettings>,
    mut radio: ResMut<Radio>,
    places: Option<Res<RadioPlaces>>,
    sfx: Option<Res<Sfx>>,
    voice: Res<VoiceState>,
    mut script: ResMut<ScriptState>,
    mut sim: ResMut<SimState>,
    (loc, state, paused): (Res<Location>, Res<State<PlayState>>, Res<Paused>),
    player: Option<Res<Player>>,
    data: Option<Res<nvf_assets::GameData>>,
    mut assets: ResMut<Assets<Panned<RadioSound>>>,
    sinks: Query<&AudioSink, With<RadioEntity>>,
) {
    let (Some(places), Some(sfx), Some(player), Some(data)) = (places, sfx, player, data) else { return };
    let Some(bank) = voice.bank.clone() else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let radio = &mut *radio;
    let held = paused.0 || *state.get() == PlayState::Loading;
    if !held {
        radio.clock += time.delta_secs_f64();
    }
    let now = radio.clock;
    let eye = player.eye();
    let space = Space::of(&loc);
    if radio.space != Some(space) {
        for (_, p) in radio.playing.drain() {
            commands.entity(p.entity).try_despawn();
        }
        radio.space = Some(space);
    }
    let volume = settings.volume.max(0.0);

    // Receivers of this place switched on or off since the last frame.
    for r in places.receivers.get(&space).map_or(&[][..], Vec::as_slice) {
        let n = rt.game.radio_switches(FormId(r.ref_id));
        if radio.switched.get(&r.ref_id).copied().unwrap_or(0) != n {
            radio.switched.insert(r.ref_id, n);
            let on = rt.game.radio_on(FormId(r.ref_id));
            info!("radio: {:08X} ({}) switched {}", r.ref_id, name(&places, r.station), if on { "on" } else { "off" });
        }
    }

    // The receivers heard now, with their gain by distance.
    let mut heard: Vec<(&Receiver, f32)> = Vec::new();
    let receivers = if settings.play { places.receivers.get(&space).map_or(&[][..], Vec::as_slice) } else { &[] };
    for r in receivers {
        let t = template(&places, &sfx, r);
        let d = distance(r.position, eye);
        let playing = radio.playing.contains_key(&r.ref_id);
        if d >= t.max_distance * if playing { LEAVE } else { 1.0 } {
            continue;
        }
        let id = FormId(r.ref_id);
        let g = &rt.game;
        if g.extra.get(&id).is_some_and(|e| e.destroyed) {
            if radio.silent.insert(r.ref_id) {
                info!("radio: {:08X} ({}) is destroyed: silent", r.ref_id, name(&places, r.station));
            }
            continue;
        }
        if !g.radio_on(id) || !sim.world.is_enabled(id).unwrap_or(!r.initially_disabled) {
            continue;
        }
        let Some(&station_ref) = places.station_ref.get(&r.station) else { continue };
        let continuous = places.forms.stations.get(&r.station).is_some_and(|s| s.continuous());
        if !g.broadcasting(FormId(station_ref), continuous) {
            continue;
        }
        heard.push((r, distance_gain(&t, d)));
    }

    // The programmes move on: of the stations heard, and of every station heard before.
    let mut stations: BTreeSet<u32> = heard.iter().map(|(r, _)| r.station).collect();
    stations.extend(radio.programmes.keys().copied());
    if !held {
        for &st in &stations {
            let station_ref = FormId(places.station_ref[&st]);
            let station_name = name(&places, st);
            for _ in 0..8 {
                let p = radio.programmes.entry(st).or_default();
                if let Some(on) = p.on_air.as_mut() {
                    let secs = on.parts[on.part].seconds;
                    if now < on.started + secs {
                        break;
                    }
                    if on.part + 1 < on.parts.len() {
                        on.started += secs;
                        on.part += 1;
                        continue;
                    }
                    let (info, ended) = (on.info, on.started + secs);
                    p.on_air = None;
                    p.wait_until = ended + GAP;
                    with_world(rt, &mut sim, |rt| rt.game.radio_line_ends(station_ref, info));
                    continue;
                }
                if now < p.wait_until {
                    break;
                }
                let last = p.last;
                let next = with_world(rt, &mut sim, |rt| {
                    let n = rt.game.radio_next(station_ref, last);
                    if let Some((_, info)) = n {
                        rt.game.radio_line_begins(station_ref, info);
                    }
                    n
                });
                let p = radio.programmes.entry(st).or_default();
                let Some((topic, info)) = next else {
                    p.wait_until = now + RETRY;
                    break;
                };
                let start = p.wait_until.max(now - 1.0).min(now);
                let parts = parts(radio, &rt.game, &sfx, &bank, &data.vfs, st, info);
                let topic_name = rt.game.data.topic_defs.get(&topic).and_then(|t| t.edid.clone()).unwrap_or_default();
                for part in &parts {
                    match (&part.path, part.song) {
                        (Some(_), true) => radio.songs += 1,
                        (Some(_), false) => radio.dj_lines += 1,
                        (None, _) => radio.silent_lines += 1,
                    }
                }
                info!(
                    "radio: {station_name} says {topic_name} {info}: {}",
                    parts
                        .iter()
                        .map(|p| format!("{} {:.1} s", p.path.as_deref().unwrap_or(if p.song { "(no file)" } else { "(no voice file)" }), p.seconds))
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                let p = radio.programmes.entry(st).or_default();
                p.last = Some(info);
                p.on_air = if parts.is_empty() { None } else { Some(OnAir { info, parts, part: 0, started: start }) };
                if p.on_air.is_none() {
                    p.wait_until = now + GAP;
                }
            }
        }
    }

    // Each receiver heard plays its station's part, joining it where it has got to.
    let mut sounding: HashSet<u32> = HashSet::new();
    for (r, dgain) in &heard {
        let Some(on) = radio.programmes.get(&r.station).and_then(|p| p.on_air.clone()) else { continue };
        let part = &on.parts[on.part];
        let Some(path) = part.path.clone() else { continue };
        let gain = part.gain * dgain * volume;
        sounding.insert(r.ref_id);
        if let Some(p) = radio.playing.get(&r.ref_id) {
            if p.info == on.info && p.part == on.part {
                p.level.store(gain.to_bits(), Ordering::Relaxed);
                continue;
            }
            commands.entity(p.entity).try_despawn();
            radio.playing.remove(&r.ref_id);
        }
        if held {
            continue;
        }
        let into = now - on.started;
        let Some(bytes) = radio.bytes(&data.vfs, &path) else { continue };
        let Some((sound, level)) = RadioSound::new(bytes, if into > JOIN { into } else { 0.0 }) else {
            if radio.reported.insert(path.clone()) {
                warn!("radio: {path}: not decoded");
            }
            continue;
        };
        level.store(gain.to_bits(), Ordering::Relaxed);
        // Panned where the receiver is (`crate::sfx`).
        let at = SoundAt::new(r.position, Some(&player));
        let entity = commands.spawn((AudioPlayer::<Panned<RadioSound>>(assets.add(Panned { inner: sound, pan: at.pan.clone() })), PlaybackSettings::DESPAWN, at, RadioEntity)).id();
        info!(
            "radio: {:08X} tuned to {} playing {path}{} (response {}, gain {gain:.3})",
            r.ref_id,
            name(&places, r.station),
            if into > JOIN { format!(" from {into:.1} s") } else { String::new() },
            part.response
        );
        radio.heard.insert(r.ref_id);
        radio.playing.insert(r.ref_id, Playing { entity, level, info: on.info, part: on.part });
    }
    radio.playing.retain(|r, p| {
        let keep = sounding.contains(r);
        if !keep {
            commands.entity(p.entity).try_despawn();
        }
        keep
    });
    for sink in &sinks {
        match (held, sink.is_paused()) {
            (true, false) => sink.pause(),
            (false, true) => sink.play(),
            _ => {}
        }
    }
}

fn report(mut exit: MessageReader<AppExit>, radio: Res<Radio>, script: Res<ScriptState>, places: Option<Res<RadioPlaces>>) {
    if exit.read().last().is_some() {
        let receivers = places.iter().flat_map(|p| p.receivers.values().flatten().map(|r| FormId(r.ref_id)));
        let (mut switches, mut on) = (0, 0);
        if let Some(rt) = script.rt.as_ref() {
            for r in receivers {
                switches += rt.game.radio_switches(r);
                on += rt.game.radio_on(r) as u32;
            }
        }
        info!(
            "radio summary: {} receivers heard, {} songs started, {} DJ lines, {} lines without a file; receivers switched {switches} times, {on} on at exit; {} sound files missing",
            radio.heard.len(),
            radio.songs,
            radio.dj_lines,
            radio.silent_lines,
            radio.missing
        );
    }
}

pub struct RadioPlugin;

impl Plugin for RadioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Radio>()
            .add_audio_source::<Panned<RadioSound>>()
            .add_systems(Startup, init)
            .add_systems(Update, play.after(crate::PlaySet::Scripts))
            .add_systems(Last, report);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_song_plays_its_mono_file() {
        let have = |p: &str| p == "sound\\songs\\radionv\\mus_big_iron_mono.ogg";
        assert_eq!(receiver_file("sound\\songs\\radionv\\mus_big_iron.mp3", have), "sound\\songs\\radionv\\mus_big_iron_mono.ogg");
        // Without a mono file, the named file.
        assert_eq!(receiver_file("sound\\songs\\radio\\x.mp3", have), "sound\\songs\\radio\\x.mp3");
        assert_eq!(receiver_file("sound\\voice\\a.ogg", have), "sound\\voice\\a.ogg");
    }
}
