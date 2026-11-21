//! Radios play their station (docs/spec-notes/radio.md): receivers in the world, actors
//! made radios by SetNPCRadio, and the Pip-Boy radio.
//!
//! - **Receivers** are placed activators whose base names a radio station
//!   (`fnv_world::radio`). One plays while it is switched on (`fnv_game::radio`: a default
//!   activation switches it, the "StartOn" scripts activate their radio once on load),
//!   is enabled and is not destroyed (SetDestroyed: Goodsprings' broken radio). A
//!   receiver whose base has neither a script nor a name (the Tops performers'
//!   `marker_audio.nif` speakers, one Gomorrah speaker) starts switched on [hypothesis:
//!   nothing could switch them on otherwise; the player cannot activate an unnamed
//!   activator].
//! - **Actors made radios** (SetNPCRadio: Lily's recording, EDE) play like a receiver
//!   at the actor's position, heard by the station's radio template. The filter the game
//!   puts on lines the actor is not the speaker of is not built.
//! - **The Pip-Boy radio** (`fnv_game::radio::PipBoyRadio`, scripts' PipBoyRadio and
//!   PipBoyRadioOff) plays its station at the listener, with no falloff and the song's
//!   own (stereo) file, not the `_mono` copy [documented: GECK wiki Creating Radio
//!   Stations: the mono file "will be what is played out of speakers and radios"]. It
//!   receives the stations whose base is a radio station without the Non-Pipboy flag,
//!   whose placed reference is enabled and in range ([`fnv_world::radio::reception`]);
//!   a script may tune it to any enabled station in range [documented: GECK wiki
//!   PipBoyRadio: a disabled or out-of-range station "cannot be selected"; observed:
//!   Repcon's launch script tunes the Pip-Boy to its Non-Pipboy station]. There is no
//!   Pip-Boy screen yet: `]` tunes to the next station received (off → first → … → last
//!   → off) and `[` to the previous, unless a script has turned the Pip-Boy off
//!   (DisablePlayerControls) [design]. Turned on without a station it takes the first
//!   one received [design]. Out of range it falls silent until the station is received
//!   again [design]. In a Radius station's static band the station fades into the
//!   `UIRadioStaticLP` loop [hypothesis: the static sound by its name; GECK wiki
//!   Reference: "plays static over this percentage of the radius"].
//! - **The station's programme** is dialogue: its placed reference says the lines of its
//!   radio quests one after another (`Game::radio_next`, begin and end result scripts
//!   run, so the quests' "last played" variables rotate the songs) while it broadcasts
//!   (SetBroadcastState, else the Continuous Broadcast flag). One programme per station,
//!   shared by everything playing it: what starts hearing it mid-line joins at the point
//!   the line has reached [design]. A station's programme starts when it is first heard
//!   and runs on from then, heard or not [design: a station nobody has heard yet runs no
//!   scripts]. A station that stops broadcasting finishes the line it is saying [design].
//! - **The poker table** ([`TableStation`]): its casino's station plays at the listener,
//!   flat, with the song's own file as the Pip-Boy plays it; while it does, nothing else
//!   plays a station [design].
//! - **StartRadioConversation** (`Game::take_radio_conversations`): the conversation
//!   starts at once, heard or not, interrupting the programme's line (whose end script
//!   then does not run) [documented: GECK wiki StartRadioConversation, "interrupt the
//!   broadcast with the new conversation"]; it follows the lines' links until one links
//!   nowhere, and the station then goes back to its programme or, not broadcasting, falls
//!   silent [documented]. A call while the station is still saying a conversation's
//!   line is not taken [design: the NCR Emergency Radio's quest asks every ten seconds].
//! - **Silence and static**: what plays a station that is saying nothing plays the
//!   station's own looping sound (TACT `SNAM`; Hanlon's recording has `OBJRadioStaticLP`)
//!   [hypothesis: GECK wiki SetBroadcastState, off, "the station will play static except
//!   when playing scripted conversations"]; the Pip-Boy plays `UIRadioStaticLP` then.
//! - **What a line plays**: a response with a sound form plays that sound's file — for a
//!   song from a receiver its `_mono.ogg` beside the `.mp3` the record names [documented:
//!   GECK wiki "Creating Radio Stations"; observed: all 48 songs have one in
//!   `Fallout - Sound.bsa`], and the sound wins over a voice file [documented: GECK wiki
//!   Radio Stations]; otherwise the response's voice file in the voice type of the
//!   line's fixed speaker when it names one, else the station's [observed: Black
//!   Mountain Radio's segment lines name Tabitha or Raul Tejada and their files are in
//!   those voice types' folders; the station has no voice type; documented: GECK wiki
//!   SetNPCRadio, a radio plays lines whose speaker is someone else]; with neither, the
//!   line lasts its fitted length in silence.
//! - **Loudness** [design]: the sound's static attenuation (songs, loops), DJ lines at
//!   full level; by distance from a receiver or actor the radio template's (the station's
//!   `INAM`, else the receiver's) minimum and maximum distance and attenuation curve as
//!   `crate::sfx::distance_gain` uses them, times the template's static attenuation
//!   ([`by_template`], queue item 48, 2026-11-07), the curve's points read as a share of
//!   power (queue item 69, 2026-11-08); without either template 150 and 1800
//!   units and 13 dB (the AMLRadio template's). Receivers and actors are panned as the sound effects are
//!   (`crate::sfx::Panned`, `SoundAt`, its gains eased over `crate::sfx::PAN_EASE`); the
//!   Pip-Boy plays flat, at the listener. A
//!   receiver is heard only within that maximum distance.
//! - **No subtitles**: a station's talk lines are heard, never subtitled (owner, list 5
//!   item 27).
//! - The Esc pause menu and a loading screen hold the radios and the programmes, as for
//!   the placed loops (`crate::ambient`).
//! - **Co-op** (`crate::coop_audio`): the host's game says the programmes, a guest's
//!   stations say the lines the host sends, joined where they have got to, and run no
//!   line scripts of their own; a guest's script conversations are left to the host's
//!   game, and its stations play no static loop between lines [design]. A guest whose
//!   copy of a line is more than a quarter second off the host's (its own loading screen
//!   or pause held it, or the host's moved the host's) joins it again at the host's point,
//!   Pip-Boy and receivers alike (owner 2026-11-11). The other
//!   players' Pip-Boys are heard from their bodies, unless this Pip-Boy plays the same
//!   station.
//! - Test aid: `NVF_PLAY_TEST_RADIO=<seconds>:<next|previous|on|off|tune=<station>>[,...]`
//!   presses `]` or `[`, turns the Pip-Boy radio on or off, or tunes it as a script's
//!   `PipBoyRadio 1 <station>` does (a placed station or its base, by Editor ID or hex
//!   FormID). Seconds of the radios' clock: from the first frame the scripts run, less
//!   pauses and loading screens.
//! - **A radio's sound stopped** (another place, a track replaced, a receiver no longer
//!   wanted) fades out over `audio_out::DECLICK_FADE` (#2217) [design], not cut
//!   mid-waveform (`audio_out::stop_sound`, counted as radio stops in `sfx declicks:
//!   …`), panned or not; the station's clock and what plays next are not changed by it.
//! - Log: `radio: <who> tuned to <station> playing <file>` when something starts a
//!   response or a loop, `radio: <station> says <topic> <info>: …`, `pipboy radio: …`,
//!   and at the end `radio summary: …` and `pipboy radio summary: …`.

use crate::control::ControlLocks;
use crate::music::Space;
use crate::pause::Paused;
use crate::player::Player;
use crate::script::{with_world, ScriptState};
use crate::sfx::{distance_gain, heard_distance, heard_gain, static_gain, Panned, SecondListener, Sfx, SfxSettings, SoundAt};
use crate::sim::SimState;
use crate::voice::VoiceState;
use crate::{Location, PlayState, PlayWorld};
use bevy::audio::{AudioSinkPlayback, ChannelCount, Decodable, Sample, SampleRate, Source};
use crate::audio_out::{stop_sound, AddAudioSource, Retiring, SoundCategory, StopReason};
use bevy::prelude::*;
use fnv_combat::fx::Sound;
use fnv_sim::FormId;
use fnv_world::radio::{self, Broadcast, Listener, RadioForms, Receiver};
use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::Cursor;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

/// Minimum and maximum distance and static attenuation without a radio template [design:
/// AMLRadio's values, the template of the receivers' commonest station].
const DEFAULT_MIN: f32 = 150.0;
const DEFAULT_MAX: f32 = 1800.0;
const DEFAULT_DB: f32 = 13.0;
/// A receiver stops once the player is this many times its maximum distance away.
const LEAVE: f32 = 1.05;
/// Seconds a station with nothing to say waits before it asks again.
const RETRY: f64 = 5.0;
/// Silence between two lines, seconds [design].
const GAP: f64 = 0.5;
/// Something joining a response later than this many seconds into it seeks to there.
const JOIN: f64 = 0.25;
/// The keys of the Pip-Boy and its static among the receivers.
const PIPBOY: u32 = u32::MAX;
const PIPBOY_STATIC: u32 = u32::MAX - 1;
const TABLE: u32 = u32::MAX - 2;
/// Another co-op player's Pip-Boy: this less the player's id (`crate::coop_audio`).
const MATE_PIPBOY: u32 = u32::MAX - 16;

/// A station heard at the listener, flat, wherever the player is: the casino's music at
/// NVForever's poker table (`crate::poker`, docs/design/poker.md §4). While it is set
/// the world's receivers, actors and the Pip-Boy are not heard [design: the player is
/// at the table, away from them].
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq)]
pub struct TableStation(pub Option<u32>);
/// The Radiation Causes Radio Static tweak: rads a second that bring the most static,
/// and that most, as a share (ours) [design: docs/design/tweaks.md].
const RADS_FOR_FULL_STATIC: f64 = 10.0;
const MAX_RAD_STATIC: f32 = 0.8;

/// The Pip-Boy's static (see the module notes).
const STATIC_SOUND: &str = "UIRadioStaticLP";

// ---- playback -------------------------------------------------------------------------

type Stem = rodio::Decoder<Cursor<Arc<[u8]>>>;

/// A panic opening a broken file (a WAV of rate 0) is that file's failure, not the game's
/// (nvf_assets::guard); one of a rate or channels the mixer cannot take is not opened
/// (crate::sfx::format_ok).
fn open(bytes: &Arc<[u8]>) -> Option<Stem> {
    nvf_assets::guard::guarded(|| Ok(rodio::Decoder::builder().with_byte_len(bytes.len() as u64).with_seekable(true).with_data(Cursor::new(bytes.clone())).build().ok()))
        .ok()
        .flatten()
        .filter(|d| crate::sfx::format_ok(d.channels().get(), d.sample_rate().get()).is_ok())
}

/// One file played from a point in it (or round and round), at a level changed while it
/// plays.
#[derive(Asset, TypePath, Clone)]
pub struct RadioSound {
    bytes: Arc<[u8]>,
    start: Duration,
    looping: bool,
    level: Arc<AtomicU32>,
    /// How loud the file is where it plays (the root mean square of its last samples,
    /// before the level), for the Pip-Boy's waveform panel.
    meter: Arc<AtomicU32>,
    channels: ChannelCount,
    rate: SampleRate,
}

/// Samples the meter of a playing file is measured over.
const METER_SAMPLES: u32 = 2048;

impl RadioSound {
    fn new(bytes: Arc<[u8]>, start: f64, looping: bool) -> Option<(RadioSound, Arc<AtomicU32>)> {
        let d = open(&bytes)?;
        let level = Arc::new(AtomicU32::new(0));
        Some((RadioSound { bytes, start: Duration::from_secs_f64(start.max(0.0)), looping, level: level.clone(), meter: Arc::default(), channels: d.channels(), rate: d.sample_rate() }, level))
    }
}

pub struct RadioDecoder {
    stem: Option<Stem>,
    bytes: Arc<[u8]>,
    looping: bool,
    level: Arc<AtomicU32>,
    meter: Arc<AtomicU32>,
    /// The squares summed and the samples counted towards the next meter reading.
    squares: f32,
    counted: u32,
    channels: ChannelCount,
    rate: SampleRate,
}

impl Iterator for RadioDecoder {
    type Item = Sample;

    fn next(&mut self) -> Option<Sample> {
        let s = match self.stem.as_mut()?.next() {
            Some(s) => s,
            None if self.looping => {
                self.stem = open(&self.bytes);
                self.stem.as_mut()?.next()?
            }
            None => return None,
        };
        self.squares += s * s;
        self.counted += 1;
        if self.counted == METER_SAMPLES {
            self.meter.store((self.squares / METER_SAMPLES as f32).sqrt().to_bits(), Ordering::Relaxed);
            (self.squares, self.counted) = (0.0, 0);
        }
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
        RadioDecoder { stem, bytes: self.bytes.clone(), looping: self.looping, level: self.level.clone(), meter: self.meter.clone(), squares: 0.0, counted: 0, channels: self.channels, rate: self.rate }
    }
}

// ---- the data -----------------------------------------------------------------------------

/// A placed station: its base, whether it starts disabled, and where it broadcasts from.
#[derive(Debug, Clone)]
pub struct Site {
    pub base: u32,
    pub initially_disabled: bool,
    pub broadcast: Option<Broadcast>,
}

/// The radio forms and the placed receivers by space; the placed stations.
#[derive(Resource, Default)]
pub struct RadioPlaces {
    pub forms: RadioForms,
    pub receivers: HashMap<Space, Vec<Receiver>>,
    /// Station base → its placed reference (the lowest FormID when there are several).
    pub station_ref: HashMap<u32, u32>,
    /// Every placed station by reference.
    pub sites: HashMap<u32, Site>,
}

impl RadioPlaces {
    /// The station base of a placed station.
    fn base_of(&self, station_ref: u32) -> Option<u32> {
        self.sites.get(&station_ref).map(|s| s.base)
    }

    /// Whether receivers of `base` start switched on (see the module notes).
    fn starts_on(&self, base: u32) -> bool {
        self.forms.receivers.get(&base).is_some_and(|b| b.script.is_none() && b.name.is_none())
    }
}

fn init(mut commands: Commands, d: Res<PlayWorld>) {
    let d = d.0;
    crate::start_read::start(&mut commands, "nvf-radio-read", move || read_places(d));
}

/// [`RadioPlaces`] from the plugins, on its own thread (`crate::start_read`).
fn read_places(d: crate::PlayData) -> RadioPlaces {
    let t = std::time::Instant::now();
    let forms = RadioForms::build(d.lo, d.schema);
    let (stations, receivers) = radio::placed(d.lo, d.schema, d.index, &forms, d.index.cells.keys().copied());
    let space = |cell: u32| d.index.cells.get(&cell).and_then(|c| c.world).map_or(Space::Interior(cell), Space::World);
    let mut places = RadioPlaces { forms, ..Default::default() };
    for s in &stations {
        let first = places.station_ref.entry(s.base).or_insert(s.ref_id);
        *first = (*first).min(s.ref_id);
        let broadcast = radio::broadcast(d.lo, d.schema, d.index, s);
        places.sites.insert(s.ref_id, Site { base: s.base, initially_disabled: s.initially_disabled, broadcast });
    }
    let n = receivers.len();
    for r in receivers {
        places.receivers.entry(space(r.cell)).or_default().push(r);
    }
    info!(
        "radio: {} stations ({} placed, {} on the Pip-Boy), {} receiver forms, {n} receivers read in {:.0} ms",
        places.forms.stations.values().filter(|s| s.is_radio()).count(),
        places.station_ref.len(),
        places.station_ref.keys().filter(|b| places.forms.stations.get(b).is_some_and(|s| s.on_pipboy())).count(),
        places.forms.receivers.len(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    places
}

// ---- the programmes -----------------------------------------------------------------------

/// One response of a line on the air.
#[derive(Debug, Clone)]
struct Part {
    response: usize,
    /// The file a receiver plays (a song's `_mono` copy when there is one).
    path: Option<String>,
    /// The file the Pip-Boy plays (the song's own file).
    stereo: Option<String>,
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
    /// Times a guest moved `started` to the host's point (co-op): the sound playing
    /// starts again there.
    rejoined: u32,
}

/// A line on the air put at its end at radio clock `now`: on its last part, that part's
/// time just run out.
fn skip_to_end(on: &mut OnAir, now: f64) {
    let Some(last) = on.parts.len().checked_sub(1) else { return };
    on.part = last;
    on.started = now - on.parts[last].seconds;
}

#[derive(Debug, Default)]
struct Programme {
    on_air: Option<OnAir>,
    last: Option<FormId>,
    /// Radio clock before which the station does not look for a line.
    wait_until: f64,
    /// Saying a conversation a script started.
    conversation: bool,
    /// A conversation asked for (the topic named), not yet started.
    pending: Option<Option<FormId>>,
    /// Saying nothing, in no conversation and not broadcasting: its loop (static) plays.
    quiet: bool,
}

/// What something plays.
#[derive(Debug, Clone, PartialEq)]
enum Track {
    Line { info: FormId, part: usize, rejoined: u32 },
    Loop(String),
}

/// Something sounding.
struct Playing {
    entity: Entity,
    level: Arc<AtomicU32>,
    meter: Arc<AtomicU32>,
    track: Track,
    /// A song (a response with a sound form), not talk or a loop.
    song: bool,
}

#[derive(Component)]
struct RadioEntity;

/// A Pip-Boy radio control: a key or the test aid.
#[derive(Debug, Clone, PartialEq)]
pub enum RadioAid {
    Next,
    Previous,
    On,
    Off,
    Tune(String),
}

/// `NVF_PLAY_TEST_RADIO`: `<seconds>:<next|previous|on|off|tune=<station>>` items
/// separated by commas, in time order.
pub fn parse_aids(v: &str) -> Result<Vec<(f32, RadioAid)>, String> {
    let mut out = v
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (t, what) = item.split_once(':').ok_or_else(|| format!("{item:?}: expected <seconds>:<what>"))?;
            let at: f32 = t.trim().parse().map_err(|_| format!("{item:?}: bad seconds {t:?}"))?;
            let aid = match what.trim() {
                "next" => RadioAid::Next,
                "previous" => RadioAid::Previous,
                "on" => RadioAid::On,
                "off" => RadioAid::Off,
                w => match w.strip_prefix("tune=") {
                    Some(s) if !s.trim().is_empty() => RadioAid::Tune(s.trim().to_string()),
                    _ => return Err(format!("{item:?}: expected next, previous, on, off or tune=<station>")),
                },
            };
            Ok((at, aid))
        })
        .collect::<Result<Vec<_>, String>>()?;
    out.sort_by(|a, b| a.0.total_cmp(&b.0));
    Ok(out)
}

#[derive(Resource, Default)]
pub struct Radio {
    /// Seconds the programmes have run (stands still while held).
    clock: f64,
    programmes: HashMap<u32, Programme>,
    /// The topic of the line each station says last put on the air (co-op).
    topics: HashMap<u32, u32>,
    /// The host's line each station last took (co-op guests).
    adopted: HashMap<u32, crate::coop_audio::HostLine>,
    /// Times a guest's station joined the host's line again (co-op).
    rejoins: u32,
    playing: HashMap<u32, Playing>,
    pub files: crate::file_bytes::FileBytes,
    lengths: HashMap<String, f64>,
    reported: HashSet<String>,
    space: Option<Space>,
    /// Times each receiver was switched, as last seen.
    switched: HashMap<u32, u32>,
    /// Destroyed receivers already reported.
    silent: HashSet<u32>,
    /// The listener's worldspace and position when last outdoors.
    outdoors: Option<(u32, [f32; 3])>,
    /// The Pip-Boy's stations received now (bases, in the list's order), and as logged.
    /// Under the player's Select Unavailable Radios tweak it also holds the stations it
    /// has received before ([`Radio::received_once`]) that are out of range now.
    pub pipboy_list: Vec<u32>,
    /// The Pip-Boy's stations (bases) it has ever received, kept with the save
    /// ([`RECEIVED_SAVE_HOOK`]) [design: docs/design/tweaks.md].
    pub received_once: BTreeSet<u32>,
    logged_list: Option<Vec<u32>>,
    /// The Pip-Boy radio as last logged: on, the station, received.
    logged_pipboy: Option<(bool, Option<FormId>, bool)>,
    aids: VecDeque<(f32, RadioAid)>,
    static_sound: Option<Option<Sound>>,
    /// The Pip-Boy's played loudness (RMS after its gain), summed per frame, and the
    /// frames counted: songs, then talk (#1077).
    loudness: [(f64, u64); 2],
    pub heard: BTreeSet<u32>,
    pub npc_heard: BTreeSet<u32>,
    pub songs: u32,
    pub dj_lines: u32,
    pub silent_lines: u32,
    pub missing: u32,
    pub loops: u32,
    pub pipboy_parts: u32,
    pub tunes: u32,
    pub script_tunes: u32,
    pub refused: u32,
    pub conversations: u32,
    pub conversations_ignored: u32,
    /// The Skip Radio Song key was pressed (`crate::tweaks`): the Pip-Boy's station ends
    /// the line it is saying now [design: docs/design/tweaks.md].
    pub skip: bool,
}

impl Radio {
    /// How loud the Pip-Boy radio's sound is now (its station, else its static; 0 while
    /// it plays nothing), for the waveform panel.
    pub fn pipboy_loudness(&self) -> f32 {
        self.playing.get(&PIPBOY).or_else(|| self.playing.get(&PIPBOY_STATIC)).map_or(0.0, |p| f32::from_bits(p.meter.load(Ordering::Relaxed)))
    }

    /// A file to play, read off the main thread (#1993): `Got::Pending` until it is in.
    fn fetch(&mut self, vfs: &Arc<fnv_vfs::Vfs>, path: &str) -> crate::file_bytes::Got {
        self.files.fetch(path, vfs, "radio")
    }

    /// A file read at once, for its length when a station's broadcast is laid out (that
    /// needs every part's length in the same frame; the song's playback then finds it
    /// kept).
    fn bytes(&mut self, vfs: &fnv_vfs::Vfs, path: &str) -> Option<Arc<[u8]>> {
        self.files.get_or_read(path, || match vfs.read(path) {
            Some(Ok(b)) => Some(b.into()),
            Some(Err(e)) => {
                warn!("radio: {path}: {e}");
                None
            }
            None => None,
        })
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

/// The file a song plays from the Pip-Boy: the file the sound record names (the stereo
/// copy), not the `_mono.ogg` a receiver plays [documented: GECK wiki "Creating Radio
/// Stations", the mono file "will be what is played out of speakers and radios in-game";
/// Nexus forums, radio station tutorial threads: the stations play the mono `.ogg` from
/// in-world radios and the stereo `.mp3` from the Pip-Boy]; the receiver's file when the
/// named one is not in the archives.
pub fn pipboy_file(named: Option<&str>, receiver: &str, exists: impl Fn(&str) -> bool) -> String {
    named.filter(|n| exists(n)).unwrap_or(receiver).to_string()
}

/// A sound form's file as an archive path.
fn sound_path(s: &Sound) -> Option<String> {
    s.file.as_deref().map(|f| fnv_vfs::normalize(&format!("sound\\{}", f.trim_start_matches(['\\', '/']))))
}

/// The template a station is heard by from a receiver of base `receiver_base` (or an
/// actor): the station's, the receiver's, or the default [documented: GECK wiki
/// Activator: the template "must be applied to Talking Activator to have proper effect"].
fn template(places: &RadioPlaces, sfx: &Sfx, station: u32, receiver_base: Option<u32>) -> Sound {
    let base = receiver_base.and_then(|b| places.forms.receivers.get(&b));
    let id = places.forms.stations.get(&station).and_then(|s| s.template).or_else(|| base.and_then(|b| b.template));
    let found = id.and_then(|id| sfx.forms.sounds.get(&FormId(id))).filter(|s| s.max_distance > 0.0);
    found.cloned().unwrap_or(Sound { min_distance: DEFAULT_MIN, max_distance: DEFAULT_MAX, static_attenuation_db: DEFAULT_DB, ..Sound::default() })
}

/// How loud a radio heard by `template` is `distance` units away: its curve between its
/// two distances times its static attenuation [hypothesis: the template's attenuation
/// applies; owner's playtest 2026-11-06 (queue item 48): music in many Vegas places
/// louder than the original's, where the casinos' and Freeside's music comes from
/// receivers of stations with AMLRadio (13 dB), AMLRadioLarge and AMLRadioTheTops
/// templates, all heard ~13 dB louder before this]. The Pip-Boy has no template and
/// plays as before.
///
/// The curve's percentages are taken as a share of the sound's power, so the amplitude
/// is their square root [hypothesis, queue item 69, owner 2026-11-07: "radio in world is
/// loud, but also needs a wider range, its range is so short it cuts out". Read as a
/// share of the amplitude (`crate::sfx::distance_gain`, every other placed sound),
/// AMLRadio's curve 100, 59, 27, 7, 0 over 150-1800 units put a receiver 11 dB down at
/// 975 units and 23 dB down at 1390, so it was gone well before its maximum distance;
/// as power it is 5.7 dB down at 975, 11.5 dB at 1390 and fades out at 1800. How the
/// original reads the curve is a question for the decompilation reader (results
/// 2026-11-08); only radios take this reading until then].
pub fn by_template(template: &Sound, distance: f32) -> f32 {
    distance_gain(template, distance).sqrt() * static_gain(template)
}

/// [`by_template`] for a radio at `at`, heard by the nearer listener: the player's
/// `eye` or split screen's player 2 (`crate::sfx::heard_gain`).
fn heard_by_template(template: &Sound, eye: [f32; 3], second: Option<&SecondListener>, at: [f32; 3]) -> f32 {
    heard_gain(template, eye, second, at).sqrt() * static_gain(template)
}

fn name(places: &RadioPlaces, station: u32) -> String {
    places.forms.stations.get(&station).and_then(|s| s.edid.clone()).unwrap_or_else(|| format!("{station:08X}"))
}

/// The level a song a line plays (a response's sound form) is played at: full, its sound
/// record's static attenuation left out (#1077). [hypothesis, from the data: the radio
/// songs' records take 12.33 dB off, but four (MUSNVStreetsOfNewReno, New Vegas Valley,
/// Home on the Wastes, Cobwebs and Rainbows) take none, while every song file is about as
/// loud as the others (-30 to -34 dB mean, ffmpeg volumedetect); applied, those four would
/// play 12 dB above the rest. Applied, the music also sat about 26 dB under the DJ lines
/// on the Pip-Boy (RMS 0.004 against 0.080), the owner's "too quiet"; without it, about 13
/// dB under, which is the files' own difference.]
pub const SONG_GAIN: f32 = 1.0;

/// The parts of a line: per response in the order said, its files and length.
#[allow(clippy::too_many_arguments)]
fn parts(radio: &mut Radio, g: &fnv_game::Game, sfx: &Sfx, bank: &fnv_voice::VoiceBank, vfs: &fnv_vfs::Vfs, base: u32, info: FormId) -> Vec<Part> {
    let Some(def) = g.data.infos.get(&info) else { return Vec::new() };
    let mut out = Vec::new();
    for r in def.response_order() {
        let resp = &def.responses[r];
        if let Some(s) = resp.sound.and_then(|id| sfx.forms.sounds.get(&id)) {
            let named = sound_path(s);
            let path = named.as_deref().map(|n| receiver_file(n, |p| vfs.exists(p)));
            let seconds = path.as_deref().and_then(|p| radio.length(vfs, p));
            match (path, seconds) {
                (Some(p), Some(secs)) => {
                    let stereo = Some(pipboy_file(named.as_deref(), &p, |f| vfs.exists(f)));
                    out.push(Part { response: r, path: Some(p), stereo, seconds: secs, song: true, gain: SONG_GAIN });
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
        let voice = def.speaker.map_or(base, |s| s.0);
        let path = bank.file(info.0, r, voice);
        let seconds = path.as_deref().and_then(|p| bank.seconds(p)).unwrap_or_else(|| g.response_seconds_as(Some(FormId(voice)), info, r));
        out.push(Part { response: r, stereo: path.clone(), path, seconds, song: false, gain: 1.0 });
    }
    out
}

/// A guest's copy of the host's line (co-op): where its part should have started on this
/// game's radio clock (`now`), from where the host's clock (`host_now`) says the host's
/// started (`host_started`); `None` while the copy (`started`) is within
/// [`crate::coop_audio::RESYNC`] of it.
fn rejoin_at(started: f64, now: f64, host_now: f64, host_started: f64) -> Option<f64> {
    let want = now - (host_now - host_started);
    ((started - want).abs() > crate::coop_audio::RESYNC).then_some(want)
}

/// Put `info` on the air on station `st` from radio clock `start`, with its log line.
#[allow(clippy::too_many_arguments)]
fn put_on_air(radio: &mut Radio, places: &RadioPlaces, g: &fnv_game::Game, sfx: &Sfx, bank: &fnv_voice::VoiceBank, vfs: &fnv_vfs::Vfs, st: u32, topic: FormId, info: FormId, start: f64) {
    let parts = parts(radio, g, sfx, bank, vfs, st, info);
    let topic_name = g.data.topic_defs.get(&topic).and_then(|t| t.edid.clone()).unwrap_or_default();
    for part in &parts {
        match (&part.path, part.song) {
            (Some(_), true) => radio.songs += 1,
            (Some(_), false) => radio.dj_lines += 1,
            (None, _) => radio.silent_lines += 1,
        }
    }
    info!(
        "radio: {} says {topic_name} {info}: {}",
        name(places, st),
        parts
            .iter()
            .map(|p| format!("{} {:.1} s", p.path.as_deref().unwrap_or(if p.song { "(no file)" } else { "(no voice file)" }), p.seconds))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let now = radio.clock;
    radio.topics.insert(st, topic.0);
    let p = radio.programmes.entry(st).or_default();
    p.last = Some(info);
    p.on_air = if parts.is_empty() { None } else { Some(OnAir { info, parts, part: 0, started: start, rejoined: 0 }) };
    if p.on_air.is_none() {
        p.wait_until = now + GAP;
    }
}

/// Whether the placed station `station_ref` is enabled now.
fn enabled(sim: &SimState, places: &RadioPlaces, station_ref: u32) -> bool {
    let initially = places.sites.get(&station_ref).is_some_and(|s| s.initially_disabled);
    sim.world.is_enabled(FormId(station_ref)).unwrap_or(!initially)
}

/// The static share at which `station_ref` is received by `l`, `None` when it is not
/// (disabled or out of range).
fn received(sim: &SimState, places: &RadioPlaces, station_ref: u32, l: &Listener) -> Option<f32> {
    if !enabled(sim, places, station_ref) {
        return None;
    }
    places.sites.get(&station_ref)?.broadcast.as_ref().and_then(|b| radio::reception(b, l))
}

/// The Pip-Boy's list: radio stations without the Non-Pipboy flag received at `l`, by
/// name; with `also` (the Select Unavailable Radios tweak) the stations it holds as well
/// while they are enabled, received here or not.
fn pipboy_list(sim: &SimState, places: &RadioPlaces, l: &Listener, also: Option<&BTreeSet<u32>>) -> Vec<u32> {
    let mut list: Vec<(String, u32)> = places
        .station_ref
        .iter()
        .filter(|(b, _)| places.forms.stations.get(b).is_some_and(|s| s.on_pipboy()))
        .filter(|(b, &r)| received(sim, places, r, l).is_some() || (also.is_some_and(|a| a.contains(b)) && enabled(sim, places, r)))
        .map(|(&b, _)| {
            let s = &places.forms.stations[&b];
            (s.name.clone().or_else(|| s.edid.clone()).unwrap_or_default().to_ascii_lowercase(), b)
        })
        .collect();
    list.sort();
    list.into_iter().map(|(_, b)| b).collect()
}

/// The Pip-Boy's controls: `]` and `[` while playing (unless scripts turned the Pip-Boy
/// off), and the test aid.
#[allow(clippy::too_many_arguments)]
fn pipboy_controls(
    keys: crate::pad::ActionButtons<KeyCode>,
    state: Res<State<PlayState>>,
    locks: Option<Res<ControlLocks>>,
    mut radio: ResMut<Radio>,
    places: Option<Res<RadioPlaces>>,
    mut script: ResMut<ScriptState>,
) {
    let keys = keys.snapshot();
    let (Some(places), Some(rt)) = (places, script.rt.as_mut()) else { return };
    // The placed stations, for scripts' GetAvailableRadios (`fnv_game::nvse_gaps`).
    if rt.game.gaps.radio_sites.is_none() {
        let mut sites: Vec<_> = places
            .sites
            .iter()
            .filter_map(|(&r, s)| {
                Some(fnv_game::nvse_gaps::RadioSite {
                    station: FormId(r),
                    initially_disabled: s.initially_disabled,
                    on_pipboy: places.forms.stations.get(&s.base).is_some_and(|f| f.on_pipboy()),
                    broadcast: s.broadcast.clone()?,
                })
            })
            .collect();
        sites.sort_by_key(|s| s.station);
        rt.game.gaps.radio_sites = Some(sites);
    }
    let radio = &mut *radio;
    let mut steps: Vec<(RadioAid, &str)> = Vec::new();
    if *state.get() == PlayState::Playing && !locks.is_some_and(|l| l.pipboy) {
        if keys.just_pressed(KeyCode::BracketRight) {
            steps.push((RadioAid::Next, "key"));
        }
        if keys.just_pressed(KeyCode::BracketLeft) {
            steps.push((RadioAid::Previous, "key"));
        }
    }
    while let Some((t, _)) = radio.aids.front() {
        if *t as f64 > radio.clock {
            break;
        }
        if let Some((_, aid)) = radio.aids.pop_front() {
            steps.push((aid, "test aid"));
        }
    }
    for (aid, by) in steps {
        info!("pipboy radio: {aid:?} ({by})");
        let p = &mut rt.game.radio.pipboy;
        let list = &radio.pipboy_list;
        let at = p.station.and_then(|s| places.base_of(s.0)).and_then(|b| list.iter().position(|&x| x == b)).filter(|_| p.on);
        let pick = |i: usize| Some(FormId(places.station_ref[&list[i]]));
        match aid {
            RadioAid::Next | RadioAid::Previous if list.is_empty() => info!("pipboy radio: no station is received here"),
            RadioAid::Next => {
                match at {
                    Some(i) if i + 1 >= list.len() => p.on = false,
                    Some(i) => p.station = pick(i + 1),
                    None => (p.on, p.station) = (true, pick(0)),
                }
                radio.tunes += 1;
            }
            RadioAid::Previous => {
                match at {
                    Some(0) => p.on = false,
                    Some(i) => p.station = pick(i - 1),
                    None => (p.on, p.station) = (true, pick(list.len() - 1)),
                }
                radio.tunes += 1;
            }
            RadioAid::On => p.on = true,
            RadioAid::Off => p.on = false,
            RadioAid::Tune(name) => {
                let id = match u32::from_str_radix(name.trim_start_matches("0x"), 16) {
                    Ok(id) => Some(id),
                    Err(_) => rt.game.data.by_edid(&name).map(|f| f.0),
                };
                // A base names its placed station.
                let station = id.map(|i| places.station_ref.get(&i).copied().unwrap_or(i));
                match station.filter(|s| places.sites.contains_key(s)) {
                    Some(s) => (p.on, p.tune) = (true, Some(FormId(s))),
                    None => warn!("NVF_PLAY_TEST_RADIO: {name:?} is not a placed station"),
                }
            }
        }
    }
}

/// What something plays this frame.
struct Want {
    key: u32,
    station: u32,
    track: Track,
    path: String,
    level: f32,
    into: f64,
    /// A line's response: the line, the response, a song.
    response: Option<(FormId, usize, bool)>,
    /// Where it is, for panning; `None`: flat (the Pip-Boy).
    at: Option<[f32; 3]>,
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
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
    (loc, state, paused, pw, table): (Res<Location>, Res<State<PlayState>>, Res<Paused>, Res<PlayWorld>, Res<TableStation>),
    (player, intro, mut cover): (Option<Res<Player>>, Option<Res<crate::intro::Intro>>, Option<ResMut<crate::sound_cover::SoundCover>>),
    data: Option<Res<nvf_assets::GameData>>,
    (mut assets, mut panned): (ResMut<Assets<RadioSound>>, ResMut<Assets<Panned<RadioSound>>>),
    // The sinks held with the radios: not those fading out after a stop (audio_out).
    (sinks, mut placed): (Query<&AudioSink, (With<RadioEntity>, Without<Retiring>)>, Query<&mut SoundAt, With<RadioEntity>>),
    (mut shared, mates): (Option<ResMut<crate::coop_audio::SharedAudio>>, Option<Res<crate::coop_social::SocialMates>>),
    (tweaks, second): (Option<Res<crate::tweaks::TweakSettings>>, Option<Res<SecondListener>>),
) {
    let second = second.as_deref();
    // The player's tweak turns the radio down in a conversation [design: docs/design/tweaks.md].
    let talk = if *state.get() == PlayState::Dialogue { tweaks.as_ref().map_or(1.0, |t| t.dialogue_radio_volume) } else { 1.0 };
    let (Some(places), Some(sfx), Some(player), Some(data)) = (places, sfx, player, data) else { return };
    let Some(bank) = voice.bank.clone() else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let radio = &mut *radio;
    // A film with `muteAudio 0` leaves the world's radios playing (`crate::intro`).
    let film_keeps = intro.as_deref().and_then(|i| i.film_keeps_world_sound());
    let held = crate::intro::world_sound_held(paused.0, film_keeps) || *state.get() == PlayState::Loading;
    if !held {
        radio.clock += time.delta_secs_f64();
    }
    let now = radio.clock;
    let eye = player.eye();
    let space = Space::of(&loc);
    if radio.space != Some(space) {
        for (_, p) in radio.playing.drain() {
            stop_sound(&mut commands, p.entity, StopReason::Radio);
        }
        radio.space = Some(space);
    }
    if loc.interior.is_none() {
        radio.outdoors = Some((loc.world, eye));
    }
    let listener = Listener {
        world: loc.world,
        interior: loc.interior,
        position: radio.outdoors.filter(|(w, _)| *w == loc.world).map_or(eye, |(_, p)| p),
        // Tale of Two Wastelands' radio fix: a child worldspace hears its parent's stations.
        parent: rt.game.small.ttw_radio_fix.then(|| pw.index.worldspaces.get(&loc.world).and_then(|w| w.parent)).flatten(),
    };
    let volume = settings.volume.max(0.0) * talk;
    let static_sound = radio.static_sound.get_or_insert_with(|| sfx.forms.sounds.values().find(|s| s.edid.as_deref() == Some(STATIC_SOUND)).cloned()).clone();

    // Receivers of this place switched on or off since the last frame.
    for r in places.receivers.get(&space).map_or(&[][..], Vec::as_slice) {
        let n = rt.game.radio_switches(FormId(r.ref_id));
        if radio.switched.get(&r.ref_id).copied().unwrap_or(0) != n {
            radio.switched.insert(r.ref_id, n);
            let on = rt.game.radio_on(FormId(r.ref_id)) != places.starts_on(r.base);
            info!("radio: {:08X} ({}) switched {}", r.ref_id, name(&places, r.station), if on { "on" } else { "off" });
        }
    }

    // Conversations scripts started.
    let follows = shared.as_deref().is_some_and(|s| s.follows_host());
    for (station_ref, topic) in rt.game.take_radio_conversations() {
        if follows {
            info!("radio: StartRadioConversation on {station_ref} left to the host's game (co-op)");
            continue;
        }
        let Some(base) = places.base_of(station_ref.0) else {
            warn!("radio: StartRadioConversation on {station_ref}, which is not a placed station");
            continue;
        };
        radio.programmes.entry(base).or_default().pending = Some(topic);
    }

    // The Pip-Boy: its list, a script's tuning, and its station.
    let received_now = pipboy_list(&sim, &places, &listener, None);
    radio.received_once.extend(received_now.iter().copied());
    // Turned on without a station: the first received here (not one only kept on the list).
    let first_received = received_now.first().copied();
    radio.pipboy_list = if rt.game.tweaks.select_unavailable_radios { pipboy_list(&sim, &places, &listener, Some(&radio.received_once)) } else { received_now };
    if radio.logged_list.as_ref() != Some(&radio.pipboy_list) {
        info!("pipboy radio: stations received: {}", radio.pipboy_list.iter().map(|&b| name(&places, b)).collect::<Vec<_>>().join(", "));
        radio.logged_list = Some(radio.pipboy_list.clone());
    }
    // The player's tweak: radiation taken now brings static into the Pip-Boy's radio,
    // whole at `RADS_FOR_FULL_STATIC` a second, never all of it [design: docs/design/tweaks.md].
    let rad_static = if rt.game.tweaks.radiation_radio_static { (rt.game.radiation().rate / RADS_FOR_FULL_STATIC).clamp(0.0, 1.0) as f32 * MAX_RAD_STATIC } else { 0.0 };
    let pip = &mut rt.game.radio.pipboy;
    if let Some(t) = pip.tune.take() {
        if received(&sim, &places, t.0, &listener).is_some() {
            pip.station = Some(t);
            radio.script_tunes += 1;
        } else {
            radio.refused += 1;
            info!("pipboy radio: {} cannot be tuned to here (disabled or out of range)", places.base_of(t.0).map_or(t.to_string(), |b| name(&places, b)));
        }
    }
    if pip.on && pip.station.is_none() {
        pip.station = first_received.map(|b| FormId(places.station_ref[&b]));
    }
    let pip_station = pip.station.filter(|_| pip.on).and_then(|s| Some((s, places.base_of(s.0)?)));
    let pip_share = pip_station.and_then(|(s, _)| received(&sim, &places, s.0, &listener)).map(|share| share.max(rad_static));
    let logged = (pip.on, pip.station, pip_share.is_some());
    if radio.logged_pipboy != Some(logged) {
        match pip_station {
            None => info!("pipboy radio: off"),
            Some((_, b)) if pip_share.is_some() => {
                info!("pipboy radio: on, tuned to {} (static {:.0} %)", name(&places, b), pip_share.unwrap_or(0.0) * 100.0)
            }
            Some((_, b)) => info!("pipboy radio: on, tuned to {}, which is not received here: silent", name(&places, b)),
        }
        radio.logged_pipboy = Some(logged);
    }

    // Everything that plays a station now: (key, station base, gain by distance, Pip-Boy,
    // where it is: `None` for the Pip-Boy, which is not panned).
    let mut heard: Vec<(u32, u32, f32, bool, Option<[f32; 3]>)> = Vec::new();
    // At the poker table only its station is heard.
    let table = table.0.filter(|_| settings.play);
    if let Some(base) = table {
        if places.station_ref.contains_key(&base) {
            heard.push((TABLE, base, 1.0, false, None));
        } else if radio.reported.insert(format!("table {base:08X}")) {
            warn!("radio: the poker table's station {base:08X} is not a placed station: silent");
        }
    }
    let receivers = if settings.play && table.is_none() { places.receivers.get(&space).map_or(&[][..], Vec::as_slice) } else { &[] };
    for r in receivers {
        let t = template(&places, &sfx, r.station, Some(r.base));
        let d = heard_distance(eye, second, r.position);
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
        let on = g.radio_on(id) != places.starts_on(r.base);
        if !on || !sim.world.is_enabled(id).unwrap_or(!r.initially_disabled) || !places.station_ref.contains_key(&r.station) {
            continue;
        }
        heard.push((r.ref_id, r.station, heard_by_template(&t, eye, second, r.position), false, Some(r.position)));
    }
    if settings.play && table.is_none() {
        for (&actor, &station_ref) in &rt.game.radio.npc_radios {
            let Some(base) = places.base_of(station_ref.0) else { continue };
            let Some(s) = sim.world.peek(actor) else { continue };
            let here = match (space, s.cell) {
                (Space::Interior(c), Some(cell)) => c == cell.0,
                (Space::World(w), Some(cell)) => pw.index.cells.get(&cell.0).and_then(|c| c.world) == Some(w),
                _ => false,
            };
            let t = template(&places, &sfx, base, None);
            let d = heard_distance(eye, second, s.pos);
            if !here || !s.enabled() || d >= t.max_distance {
                continue;
            }
            if radio.npc_heard.insert(actor.0) {
                info!("radio: {actor} plays {} (SetNPCRadio)", name(&places, base));
            }
            heard.push((actor.0, base, heard_by_template(&t, eye, second, s.pos), false, Some(s.pos)));
        }
        if let (Some((_, base)), Some(share)) = (pip_station, pip_share) {
            heard.push((PIPBOY, base, 1.0 - share, true, None));
        }
        // The other co-op players' Pip-Boys, from their bodies (`crate::coop_audio`); not
        // one tuned to the station this Pip-Boy plays.
        let own = pip_station.filter(|_| pip_share.is_some()).map(|(_, b)| b);
        if let (Some(sh), Some(m)) = (shared.as_deref(), mates.as_deref()) {
            for mate in m.mates.iter().filter(|m| m.here) {
                let Some(&base) = sh.pipboys.get(&mate.id) else { continue };
                if Some(base) == own || !places.station_ref.contains_key(&base) {
                    continue;
                }
                let at = [mate.feet[0], mate.feet[1], mate.feet[2] + crate::coop_audio::PIPBOY_HEIGHT];
                let gain = crate::coop_audio::pipboy_gain(heard_distance(eye, second, at));
                if gain > 0.0 {
                    heard.push((MATE_PIPBOY - u32::from(mate.id), base, gain, false, Some(at)));
                }
            }
        }
    }
    if let Some(sh) = shared.as_deref_mut() {
        sh.heard = heard.iter().map(|h| h.1).collect();
        sh.pipboy = pip_station.map(|(_, b)| b);
    }

    // The Skip Radio Song key: the Pip-Boy's station, received, ends its line now, so the
    // line's end script runs and the station's own order picks the next. Not a
    // conversation a script started (quests wait on those) [design: docs/design/tweaks.md].
    if std::mem::take(&mut radio.skip) {
        match pip_station.filter(|_| pip_share.is_some()) {
            Some((_, base)) => {
                let p = radio.programmes.entry(base).or_default();
                let conversation = p.conversation;
                match p.on_air.as_mut().filter(|_| !conversation) {
                    Some(on) => {
                        skip_to_end(on, now);
                        info!("pipboy radio: {} skips {} (Skip Radio Song)", name(&places, base), on.info);
                    }
                    None => info!("pipboy radio: nothing to skip on {}", name(&places, base)),
                }
            }
            None => info!("pipboy radio: nothing to skip (off or not received)"),
        }
    }

    // The programmes move on: of the stations heard, of every station heard before, and
    // of those a script started a conversation on.
    let mut stations: BTreeSet<u32> = heard.iter().map(|h| h.1).collect();
    stations.extend(radio.programmes.keys().copied());
    if let Some(sh) = shared.as_deref().filter(|s| s.on && s.host) {
        stations.extend(sh.asked.keys().copied().filter(|b| places.station_ref.contains_key(b)));
    }
    // A guest's stations say the host's lines (`crate::coop_audio`).
    let host_now = shared.as_deref().and_then(|s| s.now(time.elapsed_secs_f64()));
    if let (true, Some(sh), Some(host_now)) = (follows, shared.as_deref(), host_now) {
        if !held {
            for &st in &stations {
                let Some(line) = sh.lines.get(&st).copied() else { continue };
                // Each line the host sends is taken once; one already over is not.
                if radio.adopted.get(&st) == Some(&line) {
                    continue;
                }
                radio.adopted.insert(st, line);
                let into = host_now - line.started;
                let p = radio.programmes.entry(st).or_default();
                let same = p.on_air.as_ref().is_some_and(|on| on.info.0 == line.info && on.part == usize::from(line.part));
                if same {
                    continue;
                }
                put_on_air(radio, &places, &rt.game, &sfx, &bank, &data.vfs, st, FormId(line.topic), FormId(line.info), now - into);
                let p = radio.programmes.entry(st).or_default();
                if let Some(on) = p.on_air.as_mut() {
                    on.part = usize::from(line.part).min(on.parts.len() - 1);
                    if into >= on.parts[on.part].seconds {
                        // Over already here: the host's next line comes.
                        p.on_air = None;
                        continue;
                    }
                }
                info!("co-op radio: {} from the host: {} part {} at {into:.1} s", name(&places, st), FormId(line.info), line.part);
            }
            // The host's line where the host has got to: a guest whose copy fell behind or
            // ran ahead (its own loading screen or pause held it, or the host's moved the
            // host's) joins it again there (queue item, owner 2026-11-11: "co-op radio
            // sync") [design: a station is one broadcast for every player].
            // Test aid `NVF_COOP_RADIO_NO_REJOIN=1`: no rejoining (the old behaviour, to
            // measure).
            let rejoin = std::env::var_os("NVF_COOP_RADIO_NO_REJOIN").is_none_or(|v| v == "0");
            for &st in stations.iter().filter(|_| rejoin) {
                let Some(line) = sh.lines.get(&st).copied() else { continue };
                let Some(on) = radio.programmes.get_mut(&st).and_then(|p| p.on_air.as_mut()) else { continue };
                if on.info.0 != line.info || on.part != usize::from(line.part) {
                    continue;
                }
                if let Some(start) = rejoin_at(on.started, now, host_now, line.started) {
                    info!("co-op radio: {} joins the host's {} part {} again: {:+.2} s off", name(&places, st), on.info, on.part, on.started - start);
                    on.started = start;
                    on.rejoined += 1;
                    radio.rejoins += 1;
                }
            }
        }
    }
    if !held && follows {
        // The host's line plays out; the next comes from the host.
        for &st in &stations {
            let p = radio.programmes.entry(st).or_default();
            if let Some(on) = p.on_air.as_mut() {
                let secs = on.parts[on.part].seconds;
                if now >= on.started + secs {
                    if on.part + 1 < on.parts.len() {
                        on.started += secs;
                        on.part += 1;
                    } else {
                        p.on_air = None;
                    }
                }
            }
            p.quiet = false;
        }
    }
    if !held && !follows {
        for &st in &stations {
            let Some(&sref) = places.station_ref.get(&st) else { continue };
            let station_ref = FormId(sref);
            let broadcasting = with_world(rt, &mut sim, |rt| rt.game.broadcasting(station_ref));
            for _ in 0..8 {
                let p = radio.programmes.entry(st).or_default();
                if let Some(topic) = p.pending.take() {
                    if p.conversation && p.on_air.is_some() {
                        radio.conversations_ignored += 1;
                        info!("radio: {} is still saying a conversation: StartRadioConversation not taken", name(&places, st));
                    } else {
                        if let Some(on) = p.on_air.take() {
                            info!("radio: {} interrupted at {} for a conversation", name(&places, st), on.info);
                        }
                        let next = with_world(rt, &mut sim, |rt| {
                            let n = rt.game.radio_start(station_ref, topic);
                            if let Some((_, info)) = n {
                                rt.game.radio_line_begins(station_ref, info);
                            }
                            n
                        });
                        match next {
                            Some((topic, info)) => {
                                radio.conversations += 1;
                                info!("radio: {} starts a conversation (StartRadioConversation)", name(&places, st));
                                radio.programmes.entry(st).or_default().conversation = true;
                                put_on_air(radio, &places, &rt.game, &sfx, &bank, &data.vfs, st, topic, info, now);
                                continue;
                            }
                            None => info!("radio: {} has nothing to say for StartRadioConversation", name(&places, st)),
                        }
                    }
                }
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
                let (last, conversation) = (p.last, p.conversation);
                let (next, ended) = with_world(rt, &mut sim, |rt| {
                    let mut n = None;
                    if conversation {
                        n = last.and_then(|l| rt.game.radio_follow(station_ref, l));
                    }
                    let ended = conversation && n.is_none();
                    if n.is_none() && broadcasting {
                        n = rt.game.radio_next(station_ref, if conversation { None } else { last });
                    }
                    if let Some((_, info)) = n {
                        rt.game.radio_line_begins(station_ref, info);
                    }
                    (n, ended)
                });
                let p = radio.programmes.entry(st).or_default();
                if ended {
                    p.conversation = false;
                    info!("radio: {} ends its conversation{}", name(&places, st), if broadcasting { "" } else { ": silent" });
                }
                let Some((topic, info)) = next else {
                    p.wait_until = now + if broadcasting { RETRY } else { GAP };
                    break;
                };
                let start = p.wait_until.max(now - 1.0).min(now);
                put_on_air(radio, &places, &rt.game, &sfx, &bank, &data.vfs, st, topic, info, start);
            }
            let p = radio.programmes.entry(st).or_default();
            p.quiet = !broadcasting && !p.conversation && p.on_air.is_none();
        }
        // The host's lines, for the guests (`crate::coop_audio`).
        if let (Some(sh), Some(host_now)) = (shared.as_deref_mut().filter(|s| s.on && s.host), host_now) {
            for (&st, p) in &radio.programmes {
                if let Some(on) = p.on_air.as_ref().filter(|_| !p.quiet) {
                    let topic = radio.topics.get(&st).copied().unwrap_or(0);
                    let line = crate::coop_audio::HostLine { topic, info: on.info.0, part: on.part as u8, started: host_now - (now - on.started) };
                    sh.lines_out.push((st, line));
                }
            }
        }
    }

    // What each plays: the station's part, joining it where it has got to, else the
    // station's loop (the Pip-Boy: static).
    let mut wants: Vec<Want> = Vec::new();
    let loop_of = |st: u32| places.forms.stations.get(&st).and_then(|s| s.looping).and_then(|id| sfx.forms.sounds.get(&FormId(id))).cloned();
    for &(key, st, dgain, pipboy, at) in &heard {
        let p = radio.programmes.get(&st);
        match p.and_then(|p| p.on_air.as_ref()) {
            Some(on) => {
                let part = &on.parts[on.part];
                // The Pip-Boy and the table play the song's own (stereo) file.
                let Some(path) = (if pipboy || key == TABLE { part.stereo.clone() } else { part.path.clone() }) else { continue };
                wants.push(Want {
                    key,
                    station: st,
                    track: Track::Line { info: on.info, part: on.part, rejoined: on.rejoined },
                    path,
                    // The player's tweak sets songs and talk apart [design: docs/design/tweaks.md].
                    level: part.gain * dgain * tweaks.as_ref().map_or(1.0, |t| if part.song { t.radio_music_volume } else { t.radio_talk_volume }),
                    into: now - on.started,
                    response: Some((on.info, part.response, part.song)),
                    at,
                });
            }
            None if p.is_some_and(|p| p.quiet) => {
                let s = if pipboy { static_sound.clone() } else { loop_of(st) };
                let Some(s) = s else { continue };
                let Some(path) = sound_path(&s) else { continue };
                wants.push(Want { key, station: st, track: Track::Loop(path.clone()), path, level: static_gain(&s) * dgain, into: 0.0, response: None, at });
            }
            // Between two lines.
            None => {}
        }
    }
    if let (Some((_, base)), Some(share), Some(s)) = (pip_station, pip_share.filter(|&x| x > 0.0 && settings.play && table.is_none()), static_sound.as_ref()) {
        if let Some(path) = sound_path(s) {
            wants.push(Want { key: PIPBOY_STATIC, station: base, track: Track::Loop(path.clone()), path, level: static_gain(s) * share, into: 0.0, response: None, at: None });
        }
    }

    let mut sounding: HashSet<u32> = HashSet::new();
    let cover_now = std::time::Instant::now();
    for w in &wants {
        let gain = w.level * volume;
        sounding.insert(w.key);
        // A radio at a place, heard at the listener (`crate::sound_cover`).
        if let (Some(at), Some(cover), true) = (w.at, cover.as_deref_mut(), settings.play) {
            let eye = player.eye();
            let d = ((at[0] - eye[0]).powi(2) + (at[1] - eye[1]).powi(2) + (at[2] - eye[2]).powi(2)).sqrt();
            cover.note(Some(crate::sound_cover::SoundCover::key(crate::sound_cover::RADIO_KIND, u64::from(w.key))), w.level, d, crate::sound_cover::LOOP_HOLD, cover_now);
        }
        if let Some(p) = radio.playing.get(&w.key) {
            if p.track == w.track {
                p.level.store(gain.to_bits(), Ordering::Relaxed);
                // An actor playing a station moves.
                if let (Some(at), Ok(mut s)) = (w.at, placed.get_mut(p.entity)) {
                    if s.at != at {
                        s.at = at;
                    }
                }
                continue;
            }
            stop_sound(&mut commands, p.entity, StopReason::Radio);
            radio.playing.remove(&w.key);
        }
        if held {
            continue;
        }
        let looping = matches!(w.track, Track::Loop(_));
        // Tried again next frame while the file is being read.
        let crate::file_bytes::Got::Ready(Some(bytes)) = radio.fetch(&data.vfs, &w.path) else { continue };
        // A part is joined where it has got to as the sound starts, not where it was at the
        // frame's start: in a long frame (loading a place, a hitch) the two are apart by the
        // frame's length, and a guest started its line that much late (queue item 69,
        // observed 0.47 s) [observed 2026-11-08, `co-op radio sync` log].
        let into = if looping { w.into } else { w.into + time.last_update().map_or(0.0, |t| t.elapsed().as_secs_f64()) };
        let from = if into > JOIN { into } else { 0.0 };
        let Some((sound, level)) = RadioSound::new(bytes, from, looping) else {
            if radio.reported.insert(w.path.clone()) {
                warn!("radio: {}: not decoded", w.path);
            }
            continue;
        };
        level.store(gain.to_bits(), Ordering::Relaxed);
        let meter = sound.meter.clone();
        let entity = match w.at {
            // Panned where the receiver or actor is (`crate::sfx`).
            Some(at) => {
                let at = SoundAt::new(at, Some(&player), second);
                let handle = panned.add(Panned::new(sound, at.pan.clone()));
                commands.spawn((AudioPlayer::<Panned<RadioSound>>(handle), PlaybackSettings::DESPAWN, at, RadioEntity, SoundCategory::Radio)).id()
            }
            None => commands.spawn((AudioPlayer::<RadioSound>(assets.add(sound)), PlaybackSettings::DESPAWN, RadioEntity, SoundCategory::Radio)).id(),
        };
        let who = match w.key {
            PIPBOY => "the Pip-Boy".to_string(),
            PIPBOY_STATIC => "the Pip-Boy's static".to_string(),
            TABLE => "the poker table".to_string(),
            k if k <= MATE_PIPBOY && k > MATE_PIPBOY - 256 => format!("player {}'s Pip-Boy", MATE_PIPBOY - k),
            k => format!("{k:08X}"),
        };
        let what = match (&w.track, w.response) {
            (Track::Line { .. }, Some((_, r, _))) => format!("response {r}"),
            _ => "loop".to_string(),
        };
        info!(
            "radio: {who} tuned to {} playing {}{} ({what}, gain {gain:.3})",
            name(&places, w.station),
            w.path,
            if from > 0.0 { format!(" from {from:.1} s") } else { String::new() },
        );
        // In co-op, where this part's beginning falls on the computer's clock: two games on
        // one computer that play the same part in step log the same time (queue item 69).
        if !looping && shared.as_deref().is_some_and(|s| s.on) {
            let wall = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0.0, |d| d.as_secs_f64());
            info!("co-op radio sync: {who} {} begins at {:.3} on the computer's clock (from {from:.2} s, host clock {:.2})", w.path, wall - from, host_now.map_or(-1.0, |h| h - from));
        }
        match w.key {
            PIPBOY => radio.pipboy_parts += 1,
            PIPBOY_STATIC | TABLE => {}
            k if k <= MATE_PIPBOY && k > MATE_PIPBOY - 256 => {}
            k if rt.game.radio.npc_radios.contains_key(&FormId(k)) => {}
            k => {
                radio.heard.insert(k);
            }
        }
        if looping {
            radio.loops += 1;
        }
        radio.playing.insert(w.key, Playing { entity, level, meter, track: w.track.clone(), song: w.response.is_some_and(|(_, _, song)| song) });
    }
    // The Pip-Boy station's loudness as played, for the exit summary (#1077).
    if let Some((song, loud)) = radio.playing.get(&PIPBOY).map(|p| (p.song, f32::from_bits(p.meter.load(Ordering::Relaxed)) * f32::from_bits(p.level.load(Ordering::Relaxed)))).filter(|(_, l)| *l > 0.0) {
        let slot = &mut radio.loudness[usize::from(!song)];
        slot.0 += f64::from(loud);
        slot.1 += 1;
    }
    radio.playing.retain(|r, p| {
        let keep = sounding.contains(r);
        if !keep {
            stop_sound(&mut commands, p.entity, StopReason::Radio);
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
        let (mut switches, mut on, mut start_on) = (0, 0, 0);
        let mut pipboy = "off".to_string();
        let (mut script_calls, mut npc_radios, mut forced) = (0, 0, 0);
        if let (Some(rt), Some(places)) = (script.rt.as_ref(), places.as_ref()) {
            for r in places.receivers.values().flatten() {
                let n = rt.game.radio_switches(FormId(r.ref_id));
                switches += n;
                let is_on = rt.game.radio_on(FormId(r.ref_id)) != places.starts_on(r.base);
                if n > 0 {
                    on += is_on as u32;
                } else {
                    start_on += is_on as u32;
                }
            }
            let p = &rt.game.radio.pipboy;
            if let Some(b) = p.station.filter(|_| p.on).and_then(|s| places.base_of(s.0)) {
                pipboy = format!("on ({})", name(places, b));
            }
            (script_calls, npc_radios, forced) = (p.script_calls, rt.game.radio.npc_radios.len(), rt.game.radio.force_updates);
        }
        let mean = |(sum, n): (f64, u64)| if n > 0 { format!("{:.3}", sum / n as f64) } else { "-".into() };
        info!("pipboy radio loudness (RMS as played, before the Radio and master volumes): songs {}, talk {}", mean(radio.loudness[0]), mean(radio.loudness[1]));
        info!(
            "radio summary: {} receivers heard, {} songs started, {} DJ lines, {} lines without a file; receivers switched {switches} times, {on} on at exit ({start_on} more start on); {} sound files missing",
            radio.heard.len(),
            radio.songs,
            radio.dj_lines,
            radio.silent_lines,
            radio.missing
        );
        info!("co-op radio summary: stations joined the host's line again {} times", radio.rejoins);
        info!(
            "pipboy radio summary: {pipboy} at exit, {} stations received; tuned {} times by next/previous, {} by tune requests of scripts or the test aid ({} refused), {script_calls} script calls; {} responses played; {} conversations started by scripts ({} not taken); {} actors played as radios ({npc_radios} at exit); {} loops started; {forced} ForceRadioStationUpdate calls",
            radio.pipboy_list.len(),
            radio.tunes,
            radio.script_tunes,
            radio.refused,
            radio.pipboy_parts,
            radio.conversations,
            radio.conversations_ignored,
            radio.npc_heard.len(),
            radio.loops
        );
    }
}

fn load_aids(mut radio: ResMut<Radio>) {
    if let Ok(v) = std::env::var("NVF_PLAY_TEST_RADIO") {
        match parse_aids(&v) {
            Ok(a) => radio.aids = a.into(),
            Err(e) => warn!("NVF_PLAY_TEST_RADIO: {e}"),
        }
    }
}

pub struct RadioPlugin;

/// Let go of the radio's files nothing plays ([`crate::file_bytes`]).
fn release_files(real: Res<Time<Real>>, limits: Option<Res<nvf_assets::KeepLimits>>, x: Option<ResMut<Radio>>) {
    if let Some(mut x) = x {
        x.files.sweep(real.elapsed_secs_f64(), crate::file_bytes::limits(limits));
    }
}

impl Plugin for RadioPlugin {
    fn build(&self, app: &mut App) {
        crate::start_read::add_collect::<RadioPlaces>(app);
        app.init_resource::<Radio>()
            .init_resource::<TableStation>()
            .add_audio_source::<RadioSound>()
            .add_audio_source::<Panned<RadioSound>>()
            .add_systems(Startup, (init, load_aids))
            .add_systems(Update, (pipboy_controls, play).chain().after(crate::PlaySet::Scripts))
            .add_systems(Last, (release_files, report));
        crate::save::add_save_hook(app, RECEIVED_SAVE_HOOK);
    }
}

/// The Pip-Boy's stations received once ([`Radio::received_once`]) in the save, by base
/// FormID; remapped to the current load order, those of a plugin no longer loaded
/// dropped. A save without them leaves none.
pub const RECEIVED_SAVE_HOOK: crate::save::SaveHook = crate::save::SaveHook {
    key: "radio_received",
    save: |w| w.get_resource::<Radio>().filter(|r| !r.received_once.is_empty()).map(|r| serde_json::Value::from(r.received_once.iter().copied().collect::<Vec<u32>>())),
    load: |w, v| {
        if let Some(mut r) = w.get_resource_mut::<Radio>() {
            r.received_once = v.as_array().into_iter().flatten().filter_map(|n| n.as_u64().map(|n| n as u32)).collect();
        }
    },
    remap: Some(|v, r| {
        if let Some(list) = v.as_array_mut() {
            *list = list.iter().filter_map(|n| n.as_u64().and_then(|n| r.map(n as u32))).map(serde_json::Value::from).collect();
        }
    }),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_guest_rejoins_the_hosts_line_when_its_copy_is_off() {
        // In step: the host's part began 10 s ago on the host's clock, this copy 10 s ago here.
        assert_eq!(rejoin_at(90.0, 100.0, 500.0, 490.0), None);
        // Within the slack.
        assert_eq!(rejoin_at(90.2, 100.0, 500.0, 490.0), None);
        // A 5 s loading screen held this game's radio clock: its copy began 5 s later than
        // the host's on the shared timeline, so it is moved back 5 s.
        assert_eq!(rejoin_at(95.0, 100.0, 500.0, 490.0), Some(90.0));
        // The host's own loading screen moved the host's start later: the copy follows.
        assert_eq!(rejoin_at(90.0, 100.0, 500.0, 494.0), Some(94.0));
    }

    /// A receiver is heard at its template's static attenuation as well as its curve; no
    /// template: AMLRadio's 150 / 1800 units and 13 dB.
    #[test]
    fn receivers_take_their_templates_attenuation() {
        let large = Sound { min_distance: 20.0, max_distance: 2500.0, static_attenuation_db: 13.22, ..Default::default() };
        assert!((by_template(&large, 10.0) - 10f32.powf(-13.22 / 20.0)).abs() < 1e-4);
        assert_eq!(by_template(&large, 2600.0), 0.0);
        let none = Sound { min_distance: DEFAULT_MIN, max_distance: DEFAULT_MAX, static_attenuation_db: DEFAULT_DB, ..Sound::default() };
        assert!((by_template(&none, 100.0) - 0.2239).abs() < 1e-3);
    }

    /// AMLRadio's curve read as power: a gradual fade out to its maximum distance.
    #[test]
    fn a_receiver_fades_out_over_its_whole_range() {
        let aml = Sound { min_distance: 150.0, max_distance: 1800.0, static_attenuation_db: 13.0, attenuation_curve: Some([100.0, 59.0, 27.0, 7.0, 0.0]), ..Default::default() };
        let db = |d: f32| 20.0 * (by_template(&aml, d) / by_template(&aml, 100.0)).log10();
        assert!((db(975.0) - -5.69).abs() < 0.05, "{}", db(975.0));
        assert!((db(1387.5) - -11.55).abs() < 0.05, "{}", db(1387.5));
        assert!(db(1700.0) > -20.0 && by_template(&aml, 1700.0) > 0.0, "still heard near its edge: {}", db(1700.0));
        assert_eq!(by_template(&aml, 1800.0), 0.0);
        let mut last = 1.0;
        for d in (100..1800).step_by(50) {
            let g = by_template(&aml, d as f32);
            assert!(g <= last, "never louder farther out");
            last = g;
        }
    }

    /// A skipped line is put on its last part with that part's time run out at once.
    #[test]
    fn a_skipped_line_ends_now() {
        let part = |seconds| Part { response: 0, path: None, stereo: None, seconds, song: true, gain: 1.0 };
        let mut on = OnAir { info: FormId(7), parts: vec![part(3.0), part(180.0)], part: 0, started: 10.0, rejoined: 0 };
        skip_to_end(&mut on, 12.0);
        assert_eq!(on.part, 1);
        assert!(12.0 >= on.started + on.parts[1].seconds - 1e-9);
    }

    /// The stations received once go with the save and come back with it.
    #[test]
    fn received_stations_are_kept_with_the_save() {
        let mut w = World::new();
        w.init_resource::<Radio>();
        assert_eq!((RECEIVED_SAVE_HOOK.save)(&w), None, "none received, nothing saved");
        w.resource_mut::<Radio>().received_once.extend([0x0011_2233, 0x0001_0203]);
        let v = (RECEIVED_SAVE_HOOK.save)(&w).expect("saved");
        assert_eq!(v, serde_json::json!([0x0001_0203, 0x0011_2233]));
        let mut fresh = World::new();
        fresh.init_resource::<Radio>();
        (RECEIVED_SAVE_HOOK.load)(&mut fresh, &v);
        assert_eq!(fresh.resource::<Radio>().received_once, w.resource::<Radio>().received_once);
    }

    /// A radio song plays at full level whatever its record's static attenuation (#1077).
    #[test]
    fn songs_play_at_full_level() {
        assert_eq!(SONG_GAIN, 1.0);
        let song = Sound { static_attenuation_db: 12.33, ..Default::default() };
        assert!(static_gain(&song) < 0.25, "what the record alone would give");
    }

    #[test]
    fn a_song_plays_its_mono_file() {
        let have = |p: &str| p == "sound\\songs\\radionv\\mus_big_iron_mono.ogg";
        assert_eq!(receiver_file("sound\\songs\\radionv\\mus_big_iron.mp3", have), "sound\\songs\\radionv\\mus_big_iron_mono.ogg");
        // Without a mono file, the named file.
        assert_eq!(receiver_file("sound\\songs\\radio\\x.mp3", have), "sound\\songs\\radio\\x.mp3");
        assert_eq!(receiver_file("sound\\voice\\a.ogg", have), "sound\\voice\\a.ogg");
    }

    /// Imperium Radio (LegionRadio.esp): SOUN 00legionradiokingragnar names
    /// `songs\legionradio\king ragnar.mp3`, and the mod ships `King Ragnar_mono.ogg` beside
    /// it. A receiver plays the mono file, the Pip-Boy the named one (mods-1114 window I).
    #[test]
    fn the_pipboy_plays_the_named_song_file_and_a_receiver_its_mono_copy() {
        let named = "sound\\songs\\legionradio\\king ragnar.mp3";
        let mono = "sound\\songs\\legionradio\\king ragnar_mono.ogg";
        let both = |p: &str| p == named || p == mono;
        let receiver = receiver_file(named, both);
        assert_eq!(receiver, mono);
        assert_eq!(pipboy_file(Some(named), &receiver, both), named);
        // Only the mono file in the archives: the Pip-Boy plays it too.
        let only_mono = |p: &str| p == mono;
        assert_eq!(pipboy_file(Some(named), &receiver_file(named, only_mono), only_mono), mono);
    }

    #[test]
    fn radio_aids_parse() {
        let a = parse_aids("20:off, 3:next,10:tune=RNVTARef,12:previous,14:on").unwrap();
        assert_eq!(
            a,
            vec![
                (3.0, RadioAid::Next),
                (10.0, RadioAid::Tune("RNVTARef".into())),
                (12.0, RadioAid::Previous),
                (14.0, RadioAid::On),
                (20.0, RadioAid::Off)
            ]
        );
        assert!(parse_aids("3:louder").is_err());
        assert!(parse_aids("x:next").is_err());
        assert!(parse_aids("3:tune=").is_err());
    }
}
