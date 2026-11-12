//! Music and radio on one timeline in co-op (queue item 55, owner playtest 2026-11-06:
//! "the radio and the music are not in sync between host and guest").
//!
//! - **The host's clock.** The host sends its real-time clock and a seed for the
//!   session every [`CLOCK_EVERY`] seconds ([`game::AUDIO`], table 0, to everyone); a
//!   guest keeps the difference to its own ([`SharedAudio::now`]): the largest heard, as a
//!   message that came late (a long frame, a queue) can only make it smaller [design;
//!   observed 2026-11-07: while loading, the guest's offsets varied by 3 s].
//! - **Location music** (`crate::music`): in a session a marker's set is the one the
//!   seed and the marker pick, not a random one, so every player in the marker hears the
//!   same set, and a set or music type starts where the shared clock is in its files
//!   (the clock modulo the first file's length), so they hear it at the same point
//!   [design]. Battle music, a script's music, the death sting and incidental phrases
//!   stay each game's own (each player's own fight, script and death).
//! - **Stations** (`crate::radio`): the host's game says the programmes. Each time one of
//!   its stations starts a line or a part of one, and every [`LINE_EVERY`] seconds while
//!   it says it, it sends the station, topic, line, part and how far into the part it is;
//!   a guest puts that line on the air there, joining it at the point it has reached, and
//!   picks no lines of its own (its scripts' radio conversations are left to the host)
//!   [design]. A guest's copy of the line that drifts more than [`RESYNC`] from where
//!   the host's clock puts it (a loading screen or pause on either side) is moved back to
//!   it and its sound starts again there (`crate::radio`, owner 2026-11-11). The stations a guest hears go to the host every [`HEARD_EVERY`] seconds,
//!   and the host runs their programmes too, heard by it or not, so a station only a
//!   guest hears is still the host's.
//! - **Another player's Pip-Boy radio** is heard from that player's body, as a radio's
//!   mono song file panned where they stand ([`pipboy_gain`]): full within
//!   [`PIPBOY_NEAR`] units, falling to a faint -20 dB at [`PIPBOY_FAINT`] (about 15 m),
//!   silent past [`PIPBOY_SILENT`] [design: owner, item 55]. It is not heard while this
//!   player's own Pip-Boy plays the same station (one sound, not two). Its loudness
//!   follows the radio volume as every radio sound does (`SoundCategory::Radio`). Each
//!   player sends its Pip-Boy station when it changes and every [`HEARD_EVERY`] seconds.
//! - Log: `co-op audio: the host's clock is <s> s (<offset> s off)` when a guest first
//!   takes it or it moves more than [`CLOCK_SLACK`], `co-op radio: <station> from the
//!   host: <info> part <n> at <s> s`, `co-op radio: <name>'s Pip-Boy plays <station>`.

use crate::coop_tables::{game, Tables, R, W};
use crate::PlaySet;
use bevy::prelude::*;
use std::collections::{BTreeSet, HashMap};

/// Seconds between two of the host's clock messages.
pub const CLOCK_EVERY: f64 = 2.0;
/// Seconds between two sendings of the line a station is saying.
pub const LINE_EVERY: f64 = 5.0;
/// Seconds between two sendings of the stations heard and the Pip-Boy station.
pub const HEARD_EVERY: f64 = 3.0;
/// A guest moves its copy of the host's clock when a message puts it later by more than
/// this (s); a message that arrived late puts it earlier, and is not taken.
pub const CLOCK_SLACK: f64 = 0.05;
/// Three messages in a row this much earlier (s) are a new clock (a host that restarted).
const RESET: f64 = 5.0;
/// A move of the clock's copy by more than this (s) starts the music again at its point
/// [design: a quarter second out is heard as an echo between two players side by side].
pub const RESYNC: f64 = 0.25;
/// Another player's Pip-Boy: full level within this many units, -20 dB at
/// [`PIPBOY_FAINT`], fading to nothing at [`PIPBOY_SILENT`] [design].
pub const PIPBOY_NEAR: f32 = 150.0;
pub const PIPBOY_FAINT: f32 = 1000.0;
pub const PIPBOY_SILENT: f32 = 1100.0;
/// The height above the feet another player's Pip-Boy sounds from (the forearm).
pub const PIPBOY_HEIGHT: f32 = 90.0;

/// A line a station of the host is saying.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HostLine {
    pub topic: u32,
    pub info: u32,
    pub part: u8,
    /// The host's clock when the part started.
    pub started: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Message {
    Clock { seconds: f64, seed: u64 },
    Line { station: u32, line: HostLine },
    Heard(Vec<u32>),
    PipBoy(Option<u32>),
}

impl Message {
    fn encode(&self) -> Vec<u8> {
        let mut w = W(Vec::new());
        match self {
            Message::Clock { seconds, seed } => {
                w.u8(0).bytes(&seconds.to_le_bytes()).bytes(&seed.to_le_bytes());
            }
            Message::Line { station, line } => {
                w.u8(1).u32(*station).u32(line.topic).u32(line.info).u8(line.part).bytes(&line.started.to_le_bytes());
            }
            Message::Heard(list) => {
                w.u8(2).u16(list.len() as u16);
                for s in list {
                    w.u32(*s);
                }
            }
            Message::PipBoy(s) => {
                w.u8(3).u32(s.unwrap_or(0));
            }
        }
        w.0
    }

    fn decode(b: &[u8]) -> Option<Message> {
        fn f64_of(r: &mut R) -> Option<f64> {
            Some(f64::from_le_bytes(r.bytes()?.try_into().ok()?))
        }
        let mut r = R(b);
        Some(match r.u8()? {
            0 => {
                let seconds = f64_of(&mut r)?;
                let seed = u64::from_le_bytes(r.bytes()?.try_into().ok()?);
                Message::Clock { seconds, seed }
            }
            1 => {
                let station = r.u32()?;
                let (topic, info, part) = (r.u32()?, r.u32()?, r.u8()?);
                let started = f64_of(&mut r)?;
                Message::Line { station, line: HostLine { topic, info, part, started } }
            }
            2 => {
                let n = r.u16()?;
                Message::Heard((0..n).map(|_| r.u32()).collect::<Option<Vec<_>>>()?)
            }
            3 => Message::PipBoy(Some(r.u32()?).filter(|&s| s != 0)),
            _ => return None,
        })
    }
}

/// The shared timeline, read and written by `crate::music` and `crate::radio`.
#[derive(Resource, Debug, Default)]
pub struct SharedAudio {
    /// A co-op session is on.
    pub on: bool,
    /// This game hosts it: its programmes are the ones heard.
    pub host: bool,
    /// The host's clock less this game's real clock; 0 on the host; `None` until heard.
    offset: Option<f64>,
    /// The session's seed for the music's picks.
    pub seed: Option<u64>,
    /// Counts the times this game's copy of the clock moved by more than [`RESYNC`]: music
    /// started at an older count starts again at the clock's point (`crate::music`).
    pub epoch: u32,
    /// The lines the host's stations are saying, by station base (guests).
    pub lines: HashMap<u32, HostLine>,
    /// Stations the other games hear, by station base, and until when (host's clock).
    pub asked: HashMap<u32, f64>,
    /// The other players' Pip-Boy stations, by player.
    pub pipboys: HashMap<u8, u32>,
    /// Written by `crate::radio` on the host: lines to send.
    pub lines_out: Vec<(u32, HostLine)>,
    /// Written by `crate::radio`: the stations this game hears and its Pip-Boy station.
    pub heard: BTreeSet<u32>,
    pub pipboy: Option<u32>,
}

impl SharedAudio {
    /// The shared clock at this game's real time `local`, in a session once known.
    pub fn now(&self, local: f64) -> Option<f64> {
        self.offset.filter(|_| self.on).map(|o| local + o)
    }

    /// Whether this game takes its stations' lines from the host.
    pub fn follows_host(&self) -> bool {
        self.on && !self.host
    }

    /// The set a marker plays in a session: one of `count`, the same in every game.
    pub fn pick(&self, marker: u32, relation: Option<i64>, count: usize) -> Option<usize> {
        let seed = self.seed.filter(|_| self.on)?;
        (count > 0).then(|| {
            let mut h = seed ^ (u64::from(marker) << 8) ^ relation.unwrap_or(0) as u64;
            h = h.wrapping_mul(0x9E37_79B9_7F4A_7C15);
            h ^= h >> 31;
            (h % count as u64) as usize
        })
    }
}

/// Another player's Pip-Boy heard `d` units away: linear gain (see the module notes).
pub fn pipboy_gain(d: f32) -> f32 {
    if d <= PIPBOY_NEAR {
        1.0
    } else if d <= PIPBOY_FAINT {
        10f32.powf(-((d - PIPBOY_NEAR) / (PIPBOY_FAINT - PIPBOY_NEAR)))
    } else if d < PIPBOY_SILENT {
        0.1 * (PIPBOY_SILENT - d) / (PIPBOY_SILENT - PIPBOY_FAINT)
    } else {
        0.0
    }
}

pub struct CoopAudioPlugin;

impl Plugin for CoopAudioPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SharedAudio>().add_systems(Update, exchange.in_set(PlaySet::Ui));
    }
}

#[derive(Default)]
struct Sent {
    clock: f64,
    heard: f64,
    lines: HashMap<u32, (HostLine, f64)>,
    last_heard: Option<BTreeSet<u32>>,
    last_pipboy: Option<Option<u32>>,
    /// Clock messages in a row far below the offset kept.
    lower: u32,
}

/// Send this game's side and take the others'.
fn exchange(time: Res<Time<Real>>, mut tables: Option<ResMut<Tables>>, mut shared: ResMut<SharedAudio>, mut sent: Local<Sent>) {
    let local = time.elapsed_secs_f64();
    let Some(tables) = tables.as_deref_mut().filter(|t| t.on) else {
        if shared.on {
            *shared = SharedAudio::default();
            *sent = Sent::default();
        }
        return;
    };
    let shared = &mut *shared;
    if !shared.on {
        shared.on = true;
        info!("co-op audio: on, {}", if tables.host { "the host's timeline" } else { "following the host's timeline" });
    }
    shared.host = tables.host;
    if shared.host {
        shared.offset = Some(0.0);
        let seed = *shared.seed.get_or_insert_with(|| {
            let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(1, |d| d.as_nanos() as u64);
            t | 1
        });
        if local - sent.clock >= CLOCK_EVERY {
            sent.clock = local;
            tables.send(0, game::AUDIO, Message::Clock { seconds: local, seed }.encode());
        }
        for (station, line) in std::mem::take(&mut shared.lines_out) {
            let again = sent.lines.get(&station).is_none_or(|(l, at)| *l != line || local - at >= LINE_EVERY);
            if again {
                sent.lines.insert(station, (line, local));
                tables.send(0, game::AUDIO, Message::Line { station, line }.encode());
            }
        }
        shared.asked.retain(|_, until| *until > local);
    } else {
        shared.lines_out.clear();
    }
    let heard_changed = sent.last_heard.as_ref() != Some(&shared.heard);
    let pip_changed = sent.last_pipboy != Some(shared.pipboy);
    if heard_changed || pip_changed || local - sent.heard >= HEARD_EVERY {
        sent.heard = local;
        if !shared.host {
            tables.send(0, game::AUDIO, Message::Heard(shared.heard.iter().copied().collect()).encode());
        }
        tables.send(0, game::AUDIO, Message::PipBoy(shared.pipboy).encode());
        sent.last_heard = Some(shared.heard.clone());
        sent.last_pipboy = Some(shared.pipboy);
    }
    for (from, _, blob) in tables.take_actions(game::AUDIO) {
        let Some(m) = Message::decode(&blob) else {
            warn_once!("co-op audio: a message not understood");
            continue;
        };
        match m {
            Message::Clock { seconds, seed } if !shared.host => {
                // A message that came late makes the offset smaller: the largest one heard
                // is the truest. One far smaller several times over is a new host clock.
                // This game's real clock as the message is read, not at the frame's start:
                // a long frame (loading) reads messages that came during it, and the
                // frame's start would put the host's clock seconds ahead, kept for good as
                // the largest (queue item 69, guest 1-2 s ahead) [hypothesis, measured
                // with the `co-op radio sync` log].
                // Test aid `NVF_COOP_CLOCK_FRAME_START=1`: the old reading (to measure).
                let since_frame = time.last_update().map_or(0.0, |t| t.elapsed().as_secs_f64());
                let read_at = if std::env::var_os("NVF_COOP_CLOCK_FRAME_START").is_some() { local } else { local + since_frame };
                let offset = seconds - read_at;
                let take = match shared.offset {
                    None => true,
                    Some(o) if offset > o + CLOCK_SLACK => true,
                    Some(o) if offset < o - RESET => {
                        sent.lower += 1;
                        sent.lower >= 3
                    }
                    _ => {
                        sent.lower = 0;
                        false
                    }
                };
                if take {
                    sent.lower = 0;
                    if shared.offset.is_none_or(|o| (o - offset).abs() > RESYNC) {
                        shared.epoch += 1;
                    }
                    info!("co-op audio: the host's clock is {seconds:.2} s ({:+.2} s off)", shared.offset.map_or(0.0, |o| offset - o));
                    shared.offset = Some(offset);
                }
                shared.seed = Some(seed);
            }
            Message::Line { station, line } if !shared.host => {
                if shared.lines.get(&station) != Some(&line) {
                    debug!("co-op radio: {station:08X} from the host: {:08X} part {} from {:.1}", line.info, line.part, line.started);
                }
                shared.lines.insert(station, line);
            }
            Message::Heard(list) if shared.host => {
                for s in list {
                    shared.asked.insert(s, local + 2.0 * HEARD_EVERY);
                }
            }
            Message::PipBoy(s) => {
                let old = shared.pipboys.get(&from).copied();
                if old != s {
                    info!("co-op radio: {}'s Pip-Boy {}", tables.name(from), s.map_or("is off".to_string(), |s| format!("plays {s:08X}")));
                }
                match s {
                    Some(s) => shared.pipboys.insert(from, s),
                    None => shared.pipboys.remove(&from),
                };
            }
            _ => {}
        }
    }
    // A player who left plays nothing.
    shared.pipboys.retain(|id, _| tables.names.contains_key(id));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_go_and_come_back() {
        for m in [
            Message::Clock { seconds: 1234.5, seed: 77 },
            Message::Line { station: 0x0010_1234, line: HostLine { topic: 1, info: 2, part: 3, started: 99.25 } },
            Message::Heard(vec![5, 6, 7]),
            Message::PipBoy(Some(0x0012_3456)),
            Message::PipBoy(None),
        ] {
            assert_eq!(Message::decode(&m.encode()), Some(m));
        }
        assert_eq!(Message::decode(&[9]), None);
    }

    #[test]
    fn another_pipboy_is_faint_at_fifteen_metres() {
        assert_eq!(pipboy_gain(100.0), 1.0);
        assert!((pipboy_gain(PIPBOY_FAINT) - 0.1).abs() < 1e-4);
        assert!(pipboy_gain(1050.0) > 0.0 && pipboy_gain(1050.0) < 0.1);
        assert_eq!(pipboy_gain(1200.0), 0.0);
        assert!(pipboy_gain(500.0) < pipboy_gain(300.0));
    }

    #[test]
    fn every_game_picks_the_same_set() {
        let a = SharedAudio { on: true, seed: Some(42), ..default() };
        let b = SharedAudio { on: true, seed: Some(42), ..default() };
        assert_eq!(a.pick(0x0011_2233, None, 4), b.pick(0x0011_2233, None, 4));
        assert!(a.pick(1, None, 3).is_some_and(|i| i < 3));
        assert_eq!(SharedAudio::default().pick(1, None, 3), None);
    }
}
