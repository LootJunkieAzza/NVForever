//! Structured events (docs/design/player-reports.md §4.1): beside the run log,
//! `<home>\logs\events-<UTC time>-<pid>.jsonl` gets every log event at `info` and above as
//! one JSON line, so a tool (or the reports' server) can group and filter what the run log
//! says in prose:
//!
//! ```json
//! {"t":81234,"frame":4711,"level":"WARN","target":"nvf_play::doors","msg":"…","fields":{"form":"0001A2B3"},
//!  "where":"WastelandNV cell -3,-1 Goodsprings, position …","hour":13.2,"state":"playing"}
//! ```
//!
//! - `t`: milliseconds since the start; `frame`: frames since the start ([`note_context`]).
//! - `fields`: the event's own fields besides `message` (engine code that logs with fields,
//!   `warn!(form = %id, "…")`, gets them here as they are); absent when there are none.
//! - `where`, `hour`, `state`: where the camera is (crash.rs `place_text`), the game hour
//!   and the game state (menu, loading, playing, dialogue, vats, pipboy, paused, viewer);
//!   absent until known.
//! - A line that comes again at once (same level, target, message and fields) is counted,
//!   not written again: the next different line is preceded by `{"repeated":N}`.
//!
//! Local, opted in or not, as the run log (§4: useful in `<home>\logs` to anyone); not sent.
//! Text is redacted as the run log is (crash.rs `redact`). Bounded: [`MAX_BYTES`] a run, then
//! one line saying the rest was left out. The last [`crate::crash::KEEP_LOGS`] are kept.

use bevy::log::tracing_subscriber::{layer::Context, Layer};
use bevy::log::{tracing, BoxedLayer};
use bevy::prelude::*;
use serde_json::{Map, Value};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

/// The most one run's events file grows to [our choice: a run log rarely passes a few MiB].
pub const MAX_BYTES: u64 = 32 * 1024 * 1024;

struct EventsFile {
    out: BufWriter<std::fs::File>,
    bytes: u64,
    /// The last line written (without `t`, `frame` and context) and how often it came again.
    last: String,
    repeats: u64,
    /// Events written, a repeat counting once.
    events: u64,
    full: bool,
}

static FILE: Mutex<Option<EventsFile>> = Mutex::new(None);
static PATH: OnceLock<PathBuf> = OnceLock::new();
static FRAME: AtomicU64 = AtomicU64::new(0);
/// The game hour (×100, `u64::MAX` unknown) and state of the last frame.
static HOUR: AtomicU64 = AtomicU64::new(u64::MAX);
static STATE: Mutex<&'static str> = Mutex::new("");

fn start() -> Instant {
    static START: OnceLock<Instant> = OnceLock::new();
    *START.get_or_init(Instant::now)
}

/// Opens this run's events file in the logs folder `dir` (crash.rs `install`).
pub fn install(dir: &Path, stamp: &str) {
    start();
    let path = dir.join(format!("events-{stamp}-{}.jsonl", std::process::id()));
    if let Err(e) = fnv_install::guard_write(&path, None) {
        eprintln!("events log: not written ({e})");
        return;
    }
    match std::fs::File::create(&path) {
        Ok(f) => {
            *FILE.lock().unwrap_or_else(|e| e.into_inner()) = Some(EventsFile { out: BufWriter::new(f), bytes: 0, last: String::new(), repeats: 0, events: 0, full: false });
            let _ = PATH.set(path);
        }
        Err(e) => eprintln!("events log: {}: {e}", crate::crash::redact(&path.display().to_string())),
    }
}

/// This run's events file, if one is written.
pub fn path() -> Option<&'static Path> {
    PATH.get().map(PathBuf::as_path)
}

/// The game state and hour of the last frame, as events say them (reports' anomaly events
/// use the same).
pub fn context() -> (Option<f32>, &'static str) {
    let h = HOUR.load(Ordering::Relaxed);
    let hour = (h != u64::MAX).then(|| h as f32 / 100.0);
    (hour, STATE.try_lock().map(|s| *s).unwrap_or(""))
}

/// The Pip-Boy, VATS and the pause menu, for the game state.
type Menus<'w> = (Option<Res<'w, nvf_play::pipboy::PipBoy>>, Option<Res<'w, nvf_play::vats_mode::VatsMode>>, Option<Res<'w, nvf_play::pause::Paused>>);

/// Each frame: the frame count, the game hour and state for the events of the next frame;
/// the file is flushed so a crash loses at most one frame's lines.
pub fn note_context(loading: Option<Res<nvf_play::loading::Loading>>, state: Option<Res<State<nvf_play::PlayState>>>, menus: Menus, sim: Option<Res<nvf_play::sim::SimState>>) {
    FRAME.fetch_add(1, Ordering::Relaxed);
    let (pip, vats, paused) = menus;
    let hour = sim.as_ref().map(|s| nvf_play::music::game_hour(s));
    HOUR.store(hour.map_or(u64::MAX, |h| (h * 100.0).round() as u64), Ordering::Relaxed);
    let now = if loading.as_ref().is_some_and(|l| l.active()) {
        "loading"
    } else if vats.as_ref().is_some_and(|v| v.active()) {
        "vats"
    } else if pip.as_ref().is_some_and(|p| p.open()) {
        "pipboy"
    } else if paused.as_ref().is_some_and(|p| p.0) {
        "paused"
    } else {
        match state.as_ref().map(|s| *s.get()) {
            Some(nvf_play::PlayState::Menu) => "menu",
            Some(nvf_play::PlayState::Dialogue) => "dialogue",
            Some(nvf_play::PlayState::Loading) => "loading",
            Some(nvf_play::PlayState::Playing) => "playing",
            None => "viewer",
        }
    };
    if let Ok(mut s) = STATE.try_lock() {
        *s = now;
    }
    if let Ok(mut f) = FILE.try_lock() {
        if let Some(f) = f.as_mut() {
            let _ = f.out.flush();
        }
    }
}

/// The message and the other fields of an event.
#[derive(Default)]
struct Fields {
    msg: String,
    rest: Map<String, Value>,
}

impl tracing::field::Visit for Fields {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let text = format!("{value:?}");
        if field.name() == "message" {
            self.msg = text;
        } else {
            self.rest.insert(field.name().to_string(), Value::from(text));
        }
    }
    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.msg = value.to_string();
        } else {
            self.rest.insert(field.name().to_string(), Value::from(value));
        }
    }
    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.rest.insert(field.name().to_string(), Value::from(value));
    }
    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.rest.insert(field.name().to_string(), Value::from(value));
    }
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        self.rest.insert(field.name().to_string(), Value::from(value));
    }
    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.rest.insert(field.name().to_string(), Value::from(value));
    }
}

/// The part of a line that says what happened (compared to fold repeats).
fn body(level: &str, target: &str, f: &Fields) -> Map<String, Value> {
    let mut m = Map::new();
    m.insert("level".into(), Value::from(level));
    m.insert("target".into(), Value::from(target));
    m.insert("msg".into(), Value::from(crate::crash::redact(&f.msg).into_owned()));
    if !f.rest.is_empty() {
        let rest = f.rest.iter().map(|(k, v)| (k.clone(), v.as_str().map_or_else(|| v.clone(), |s| Value::from(crate::crash::redact(s).into_owned())))).collect();
        m.insert("fields".into(), Value::Object(rest));
    }
    m
}

/// The line to write for an event: `t`, `frame`, then `body`, then the context.
fn line(t_ms: u64, frame: u64, body: Map<String, Value>, place: &str, hour: Option<f32>, state: &str) -> String {
    let mut m = Map::new();
    m.insert("t".into(), Value::from(t_ms));
    m.insert("frame".into(), Value::from(frame));
    m.extend(body);
    if !place.is_empty() {
        m.insert("where".into(), Value::from(place));
    }
    if let Some(h) = hour {
        m.insert("hour".into(), Value::from((f64::from(h) * 10.0).round() / 10.0));
    }
    if !state.is_empty() {
        m.insert("state".into(), Value::from(state));
    }
    Value::Object(m).to_string()
}

/// What to write for an event whose `body` text is `key`: nothing (a repeat, counted), or the
/// line after a `{"repeated":N}` line for the one before it.
fn fold(f: &mut EventsFile, key: String, line: impl FnOnce() -> String) -> Option<String> {
    if key == f.last {
        f.repeats += 1;
        return None;
    }
    let note = (f.repeats > 0).then(|| format!("{{\"repeated\":{}}}\n", f.repeats));
    f.last = key;
    f.repeats = 0;
    Some(note.unwrap_or_default() + &line() + "\n")
}

struct EventsLayer;

impl<S: tracing::Subscriber> Layer<S> for EventsLayer {
    fn on_event(&self, event: &tracing::Event<'_>, _: Context<'_, S>) {
        let meta = event.metadata();
        // info and up (§4.1); `[Log]`'s debug and trace stay in the run log.
        if *meta.level() > tracing::Level::INFO {
            return;
        }
        let mut fields = Fields::default();
        event.record(&mut fields);
        let body = body(meta.level().as_str(), meta.target(), &fields);
        let key = Value::Object(body.clone()).to_string();
        let t_ms = start().elapsed().as_millis() as u64;
        let frame = FRAME.load(Ordering::Relaxed);
        let (hour, state) = context();
        let place = crate::crash::place_text();
        // Never waits: an event logged while the file is being written (a panic in a write)
        // is left out rather than hang.
        let Ok(mut guard) = FILE.try_lock() else { return };
        let Some(f) = guard.as_mut() else { return };
        if f.full {
            return;
        }
        let Some(text) = fold(f, key, || line(t_ms, frame, body, &place, hour, state)) else { return };
        if f.bytes + text.len() as u64 > MAX_BYTES {
            f.full = true;
            let _ = writeln!(f.out, "{{\"t\":{t_ms},\"frame\":{frame},\"level\":\"WARN\",\"target\":\"nvforever::events_log\",\"msg\":\"events file full ({} MiB): the rest of this run is in the run log only\"}}", MAX_BYTES >> 20);
            let _ = f.out.flush();
            return;
        }
        f.bytes += text.len() as u64;
        f.events += 1;
        let _ = f.out.write_all(text.as_bytes());
        if *meta.level() <= tracing::Level::WARN {
            let _ = f.out.flush();
        }
    }
}

/// The layer, for the log plugin (crash.rs `file_layer`), when the file is open.
pub fn layer() -> Option<BoxedLayer> {
    PATH.get().is_some().then(|| Box::new(EventsLayer) as BoxedLayer)
}

/// Flushed at a panic and at exit (crash.rs). Returns the events written so far, when the
/// file could be reached.
pub fn flush() -> Option<u64> {
    let mut f = FILE.try_lock().ok()?;
    let f = f.as_mut()?;
    if f.repeats > 0 {
        let _ = writeln!(f.out, "{{\"repeated\":{}}}", f.repeats);
        f.repeats = 0;
        f.last.clear();
    }
    let _ = f.out.flush();
    Some(f.events)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file() -> EventsFile {
        let f = tempfile_like();
        EventsFile { out: BufWriter::new(f), bytes: 0, last: String::new(), repeats: 0, events: 0, full: false }
    }

    fn tempfile_like() -> std::fs::File {
        let p = std::env::temp_dir().join(format!("nvf-events-test-{}-{:?}", std::process::id(), std::thread::current().id()));
        std::fs::File::create(p).unwrap()
    }

    #[test]
    fn a_line_has_its_fields_and_context() {
        let mut f = Fields { msg: "door locked".into(), ..Default::default() };
        f.rest.insert("form".into(), Value::from("0001A2B3"));
        let l = line(1500, 90, body("WARN", "nvf_play::doors", &f), "WastelandNV cell 0,0", Some(13.25), "playing");
        let v: Value = serde_json::from_str(&l).unwrap();
        assert_eq!(v["t"], 1500);
        assert_eq!(v["frame"], 90);
        assert_eq!(v["level"], "WARN");
        assert_eq!(v["target"], "nvf_play::doors");
        assert_eq!(v["msg"], "door locked");
        assert_eq!(v["fields"]["form"], "0001A2B3");
        assert_eq!(v["where"], "WastelandNV cell 0,0");
        assert_eq!(v["hour"], 13.3);
        assert_eq!(v["state"], "playing");
        // Before the camera has a place: no context keys, no empty fields.
        let v: Value = serde_json::from_str(&line(0, 0, body("INFO", "x", &Fields { msg: "m".into(), ..Default::default() }), "", None, "")).unwrap();
        assert!(v.get("where").is_none() && v.get("hour").is_none() && v.get("state").is_none() && v.get("fields").is_none());
    }

    #[test]
    fn repeats_are_folded() {
        let mut f = file();
        assert_eq!(fold(&mut f, "a".into(), || "A".into()).as_deref(), Some("A\n"));
        assert_eq!(fold(&mut f, "a".into(), || "A".into()), None);
        assert_eq!(fold(&mut f, "a".into(), || "A".into()), None);
        assert_eq!(fold(&mut f, "b".into(), || "B".into()).as_deref(), Some("{\"repeated\":2}\nB\n"));
        assert_eq!(fold(&mut f, "a".into(), || "A".into()).as_deref(), Some("A\n"));
    }
}
