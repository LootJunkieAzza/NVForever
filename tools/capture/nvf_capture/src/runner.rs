//! Capture jobs and the frame-driven step runner (no game dependency, unit-tested).
//!
//! Protocol and file formats: docs/spec-notes/xnvse-capture.md §4.

use std::collections::{BTreeMap, HashMap};
use std::time::Instant;

use serde::{Deserialize, Deserializer};
use serde_json::{json, Value as Json};

use crate::short::{self, Kind};

pub const JOB_FORMAT: &str = "fnv.capture.steps/1";
/// DirectX scan codes. [documented: GECK wiki `IsKeyPressed`, DirectX scan code table]
pub const KEY_END: u32 = 207;
pub const KEY_HOME: u32 = 199;

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub format: String,
    pub name: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub start: String,
    /// Forms the job's short scripts use as `@name`: `"<plugin file>:<FormID in hex>"`
    /// (plugin version 8; `short.rs`).
    #[serde(default)]
    pub forms: BTreeMap<String, String>,
    pub steps: Vec<Step>,
}

/// A script given as one string or as a list of lines (plugin version 8).
fn text_or_lines<'de, D: Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Text {
        One(String),
        Lines(Vec<String>),
    }
    Ok(match Text::deserialize(d)? {
        Text::One(s) => s,
        Text::Lines(l) => l.join("\n") + "\n",
    })
}

/// One step; the variant is chosen by its key (`query`, `script`, `console`, `manual`,
/// `frames`, `until`, `remember`, `observe`, `screenshot`).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(untagged)]
pub enum Step {
    Query(Query),
    Script(ScriptStep),
    Console(ConsoleStep),
    Manual(ManualStep),
    Frames(FramesStep),
    Until(UntilStep),
    Remember(RememberStep),
    Observe(ObserveStep),
    Screenshot(ScreenshotStep),
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Query {
    #[serde(deserialize_with = "text_or_lines")]
    pub query: String,
    #[serde(default)]
    pub label: String,
    /// Left out with a short query: the query's own text.
    #[serde(default)]
    pub field: String,
    /// A short query whose value is a string, not a number.
    #[serde(default)]
    pub text: bool,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScriptStep {
    #[serde(deserialize_with = "text_or_lines")]
    pub script: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConsoleStep {
    pub console: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualStep {
    pub manual: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FramesStep {
    pub frames: u32,
}

/// Run `until` (a function script) once per game loop until it returns a non-zero
/// number, at most `max_frames` times; the number of loops is recorded.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UntilStep {
    #[serde(deserialize_with = "text_or_lines")]
    pub until: String,
    pub max_frames: u32,
}

/// Run `remember` (a function script returning a number) and keep the number as `as`;
/// later step texts use it as `%{name}`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RememberStep {
    #[serde(deserialize_with = "text_or_lines")]
    pub remember: String,
    #[serde(rename = "as")]
    pub name: String,
}

/// Save a picture of the game window as `shots\<job>.<step>.<label>.<unix seconds>.png`
/// in the capture directory (plugin version 7). The label is `[A-Za-z0-9_.-]`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenshotStep {
    pub screenshot: String,
}

/// A saved screenshot, as the host reports it.
#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    /// Path relative to the capture directory, with `/` separators.
    pub file: String,
    pub width: u32,
    pub height: u32,
    /// Mean of all channels, 0 (black) to 1 (white).
    pub mean: f64,
    /// Whether the game window was in front (otherwise another window may be in the picture).
    pub foreground: bool,
}

pub fn valid_name(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '.')
}

/// Sample the game repeatedly (plugin version 4; docs/spec-notes/ai-observer.md §3).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObserveStep {
    pub observe: Observe,
}

/// Every `every` game loops: run the `header` parts once and, for each group, its
/// `count` script and then its `probe` parts once per subject index (`%{i}` in the
/// probe text). Ends when `until` returns non-zero (checked before each sample), after
/// `max_samples`, or when End is pressed. `drive` runs once per game loop meanwhile,
/// until it returns non-zero or `drive_frames` loops have passed (0: no limit); then
/// `drive_end` runs once (also when the observation ends first).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Observe {
    pub label: String,
    /// Field names, echoed into the output so that it describes itself.
    #[serde(default)]
    pub fields: Json,
    #[serde(default)]
    pub header: Vec<String>,
    #[serde(default)]
    pub groups: Vec<Group>,
    pub every: u32,
    #[serde(default)]
    pub until: Option<String>,
    pub max_samples: u32,
    #[serde(default)]
    pub drive: Option<String>,
    #[serde(default)]
    pub drive_frames: u32,
    #[serde(default)]
    pub drive_end: Option<String>,
    /// Checked once per sample (plugin version 6).
    #[serde(default)]
    pub hooks: Vec<Hook>,
    /// Game events heard while the step runs (plugin version 9).
    #[serde(default)]
    pub events: Vec<EventSpec>,
    /// Screenshots taken at samples where a condition holds (plugin version 9).
    #[serde(default)]
    pub shots: Option<ShotHook>,
}

/// A game event to hear (xNVSE's event names, e.g. `OnHit`, `OnDeath`, `OnStartCombat`):
/// for each one the game reports, `probe` (a full function script taking the event's two
/// arguments, `Begin Function {rEa, rEb}`) is run at once and its result written as
/// `{"kind": "event", step, n, name, value}` (docs/spec-notes/ai-observer.md §12).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventSpec {
    pub name: String,
    pub probe: String,
}

/// An event heard: its index in the `listen` list and its probe's result.
pub type Heard = (usize, Result<Value, String>);

/// Most events a step may hear (the plugin has this many handler slots).
pub const MAX_EVENTS: usize = 16;

/// At a sample where `when` returns non-zero, save a screenshot labelled `label` (at most
/// `max`, at least `gap` samples apart).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ShotHook {
    pub label: String,
    pub when: String,
    #[serde(default = "default_shot_gap")]
    pub gap: u32,
    #[serde(default = "default_shot_max")]
    pub max: u32,
}

fn default_shot_gap() -> u32 {
    1
}

fn default_shot_max() -> u32 {
    100
}

/// When `when` has returned non-zero at `after` samples in a row, `then` runs once (and
/// the count starts again). Used to close a dialogue an NPC opened with the player
/// (docs/spec-notes/ai-observer.md §3).
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Hook {
    pub name: String,
    pub when: String,
    pub after: u32,
    pub then: String,
    /// Fire once only (plugin version 9): a scene action at a set sample, e.g. a push
    /// while the observation samples every game loop.
    #[serde(default)]
    pub once: bool,
}

/// Subjects of one kind (e.g. actors near the player, or named actors wherever they
/// are). A subject whose first probe part returns 0, nothing or "" is left out.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Group {
    pub name: String,
    pub count: String,
    pub probe: Vec<String>,
    #[serde(default = "default_group_max")]
    pub max: u32,
}

fn default_group_max() -> u32 {
    256
}

/// `text` with every `%{name}` replaced by the remembered number.
pub fn expand(text: &str, vars: &HashMap<String, f64>) -> Result<String, String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("%{") {
        out.push_str(&rest[..i]);
        let after = &rest[i + 2..];
        let j = after.find('}').ok_or_else(|| "unclosed %{ in step text".to_string())?;
        let name = &after[..j];
        let v = vars.get(name).ok_or_else(|| format!("%{{{name}}} was not remembered"))?;
        out.push_str(&format!("{v}"));
        rest = &after[j + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

impl Job {
    pub fn parse(text: &str) -> Result<Job, String> {
        let mut job = Job::read(text)?;
        job.write_out_short_scripts()?;
        Ok(job)
    }

    /// The job as it is written in the file, short scripts not yet written out.
    fn read(text: &str) -> Result<Job, String> {
        let job: Job = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if job.format != JOB_FORMAT {
            return Err(format!("unsupported job format {:?} (expected {JOB_FORMAT})", job.format));
        }
        if !valid_name(&job.name) {
            return Err(format!("job name {:?} must be non-empty [A-Za-z0-9_.-]", job.name));
        }
        Ok(job)
    }

    /// Replaces every short script by its full function script (`short.rs`), so that the
    /// runner only ever sees full scripts.
    fn write_out_short_scripts(&mut self) -> Result<(), String> {
        let mut forms = BTreeMap::new();
        for (name, spec) in &self.forms {
            if name.is_empty() || !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                return Err(format!("form name {name:?} must be non-empty [A-Za-z0-9_]"));
            }
            forms.insert(name.clone(), short::parse_form(spec)?);
        }
        for (i, step) in self.steps.iter_mut().enumerate() {
            let full = |text: &mut String, kind: Kind| -> Result<(), String> {
                *text = short::wrap(text, kind, &forms).map_err(|e| format!("step {i}: {e}"))?;
                Ok(())
            };
            match step {
                Step::Query(q) => {
                    if q.field.is_empty() && !short::is_full_script(&q.query) {
                        q.field = q.query.trim().to_string();
                    }
                    full(&mut q.query, if q.text { Kind::Text } else { Kind::Number })?;
                }
                Step::Script(s) => full(&mut s.script, Kind::Action)?,
                Step::Until(u) => full(&mut u.until, Kind::Condition)?,
                Step::Remember(r) => full(&mut r.remember, Kind::Number)?,
                Step::Observe(o) => {
                    let o = &mut o.observe;
                    for h in &mut o.header {
                        full(h, Kind::Number)?;
                    }
                    for g in &mut o.groups {
                        full(&mut g.count, Kind::Number)?;
                        for p in &mut g.probe {
                            full(p, Kind::Number)?;
                        }
                    }
                    for (text, kind) in [(&mut o.until, Kind::Condition), (&mut o.drive, Kind::Number), (&mut o.drive_end, Kind::Action)] {
                        if let Some(t) = text {
                            full(t, kind)?;
                        }
                    }
                    for h in &mut o.hooks {
                        full(&mut h.when, Kind::Condition)?;
                        full(&mut h.then, Kind::Action)?;
                    }
                    if let Some(sh) = &mut o.shots {
                        if !valid_name(&sh.label) {
                            return Err(format!("step {i}: shots label {:?} must be non-empty [A-Za-z0-9_.-]", sh.label));
                        }
                        full(&mut sh.when, Kind::Condition)?;
                    }
                    if o.events.len() > MAX_EVENTS {
                        return Err(format!("step {i}: at most {MAX_EVENTS} events"));
                    }
                    for e in &o.events {
                        if e.name.is_empty() || !e.name.chars().all(|c| c.is_ascii_alphanumeric()) {
                            return Err(format!("step {i}: event name {:?} must be non-empty [A-Za-z0-9]", e.name));
                        }
                        if !short::is_full_script(&e.probe) {
                            return Err(format!("step {i}: event {}: the probe must be a full function script taking two refs", e.name));
                        }
                    }
                }
                Step::Console(_) | Step::Manual(_) | Step::Frames(_) | Step::Screenshot(_) => {}
            }
        }
        Ok(())
    }
}

/// A function script's result.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Number(f64),
    Text(String),
    /// A form: its load-order FormID as GetFormIDString writes it (plugin version 10).
    Form(String),
    /// An array's elements in key order (plugin version 10).
    List(Vec<Value>),
    /// A map's (or a list with gaps') keys and values, in key order (plugin version 10).
    Map(Vec<(String, Value)>),
    Nothing,
}

impl Value {
    fn to_json(&self) -> Json {
        match self {
            Value::Number(n) if n.is_finite() => json!(n),
            Value::Number(_) | Value::Nothing => Json::Null,
            Value::Text(s) => json!(s),
            Value::Form(id) => json!({ "form": id }),
            Value::List(v) => Json::Array(v.iter().map(Value::to_json).collect()),
            Value::Map(v) => Json::Object(v.iter().map(|(k, x)| (k.clone(), x.to_json())).collect()),
        }
    }
}

/// What the runner needs from the game (implemented over xNVSE in `lib.rs`, mocked in tests).
pub trait Host {
    /// Compile and run a "Begin Function {} … End" script; its SetFunctionValue result.
    fn script(&mut self, text: &str) -> Result<Value, String>;
    /// Run one console line.
    fn console(&mut self, line: &str) -> Result<(), String>;
    fn key_down(&mut self, scancode: u32) -> bool;
    /// Show a short message on screen.
    fn show(&mut self, text: &str);
    /// Save a picture of the game window; `stem` is the file name without extension.
    fn screenshot(&mut self, stem: &str) -> Result<Shot, String>;
    /// Start hearing these game events (replacing any heard before).
    fn listen(&mut self, events: &[EventSpec]) -> Result<(), String>;
    /// Events heard since the last call: (index in the `listen` list, the probe's result),
    /// and how many were lost (heard while the probe could not be run).
    fn heard(&mut self) -> (Vec<Heard>, u32);
    /// Stop hearing events.
    fn unlisten(&mut self);
}

/// Something that happened to a job during a tick; the caller handles files.
#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// Output lines for the job (JSON objects, one per line in the .jsonl file).
    Output { job: String, line: Json },
    Finished { job: String },
    Aborted { job: String },
    /// Automatic mode only: no job is left for this session (sent once per load).
    AllDone,
}

#[derive(Debug, Clone, PartialEq)]
enum State {
    Idle,
    /// Waiting for End (or, in automatic mode, `wait` more game loops).
    Announced { wait: u32 },
    /// `polls`: game loops spent on the current `until` step.
    Running { step: usize, wait: u32, manual: bool, polls: u32 },
    /// An `observe` step: samples taken, loops to the next sample, drive state.
    Observing { step: usize, sample: u32, wait: u32, drive_on: bool, drive_loops: u32 },
}

pub struct Runner {
    pending: Vec<Job>,
    state: State,
    /// Key state in the previous tick, for press edges: (End, Home).
    prev: (bool, bool),
    /// Automatic mode: start announced jobs after this many game loops instead of
    /// waiting for End (docs/spec-notes/xnvse-capture.md §4). Manual steps still wait.
    auto_start: Option<u32>,
    /// Numbers kept by `remember` steps, for the running job.
    vars: HashMap<String, f64>,
    /// Consecutive samples each hook of the running observe step has been true.
    hook_counts: Vec<u32>,
    /// Screenshots of the running observe step: taken, and the sample of the last one.
    shots: (u32, Option<u32>),
    pub plugin_version: u32,
    pub nvse_version: u32,
    pub runtime_version: u32,
}

impl Runner {
    pub fn new(plugin_version: u32, nvse_version: u32, runtime_version: u32) -> Runner {
        Runner { pending: Vec::new(), state: State::Idle, prev: (false, false), auto_start: None, vars: HashMap::new(), hook_counts: Vec::new(), shots: (0, None), plugin_version, nvse_version, runtime_version }
    }

    /// Switch to automatic mode (see `auto_start`).
    pub fn set_auto_start(&mut self, frames: Option<u32>) {
        self.auto_start = frames;
    }

    pub fn current(&self) -> Option<&Job> {
        match self.state {
            State::Idle => None,
            _ => self.pending.first(),
        }
    }

    pub fn is_running(&self) -> bool {
        matches!(self.state, State::Running { .. } | State::Observing { .. })
    }

    /// A save finished loading. Replaces the pending jobs (rescanned by the caller),
    /// aborting a running job, and announces the first one.
    pub fn game_loaded(&mut self, jobs: Vec<Job>, host: &mut dyn Host) -> Vec<Event> {
        let mut ev = Vec::new();
        if matches!(self.state, State::Observing { .. }) {
            host.unlisten();
        }
        if let (State::Running { .. } | State::Observing { .. }, Some(j)) = (&self.state, self.pending.first()) {
            ev.push(Event::Output { job: j.name.clone(), line: json!({"kind": "aborted", "reason": "a save was loaded"}) });
            ev.push(Event::Aborted { job: j.name.clone() });
        }
        self.pending = jobs;
        self.announce(host, &mut ev);
        ev
    }

    fn announce(&mut self, host: &mut dyn Host, ev: &mut Vec<Event>) {
        match self.pending.first() {
            Some(j) => {
                let title = if j.title.is_empty() { &j.name } else { &j.title };
                match self.auto_start {
                    Some(wait) => {
                        host.show(&format!("NVF capture: {title}. Starting automatically."));
                        self.state = State::Announced { wait };
                    }
                    None => {
                        host.show(&format!("NVF capture: {title}. {} Press End to start, Home to skip.", j.start));
                        self.state = State::Announced { wait: 0 };
                    }
                }
            }
            None => {
                if self.auto_start.is_some() {
                    ev.push(Event::AllDone);
                }
                self.state = State::Idle;
            }
        }
    }

    /// One game loop.
    pub fn tick(&mut self, host: &mut dyn Host) -> Vec<Event> {
        let mut ev = Vec::new();
        if self.state == State::Idle {
            return ev;
        }
        let (end, home) = (host.key_down(KEY_END), host.key_down(KEY_HOME));
        let (end_pressed, home_pressed) = (end && !self.prev.0, home && !self.prev.1);
        self.prev = (end, home);
        let job = self.pending.first().expect("not idle").clone();
        match self.state.clone() {
            State::Idle => {}
            State::Announced { wait } => {
                let auto_go = match self.auto_start {
                    Some(_) if wait > 0 => {
                        self.state = State::Announced { wait: wait - 1 };
                        return ev;
                    }
                    Some(_) => true,
                    None => false,
                };
                if end_pressed || auto_go {
                    ev.push(Event::Output {
                        job: job.name.clone(),
                        line: json!({"kind": "start", "job": job.name, "plugin_version": self.plugin_version,
                                     "nvse_version": format!("0x{:08X}", self.nvse_version),
                                     "runtime_version": format!("0x{:08X}", self.runtime_version)}),
                    });
                    host.show(&format!("NVF capture: running {} ({} steps).", job.name, job.steps.len()));
                    self.vars.clear();
                    self.state = State::Running { step: 0, wait: 0, manual: false, polls: 0 };
                } else if home_pressed {
                    host.show(&format!("NVF capture: skipped {}.", job.name));
                    self.pending.remove(0);
                    self.announce(host, &mut ev);
                }
            }
            State::Running { step, wait, manual, polls } => {
                if wait > 0 {
                    self.state = State::Running { step, wait: wait - 1, manual, polls };
                    return ev;
                }
                if manual {
                    if end_pressed {
                        self.state = State::Running { step: step + 1, wait: 0, manual: false, polls: 0 };
                    }
                    return ev;
                }
                if step >= job.steps.len() {
                    ev.push(Event::Output { job: job.name.clone(), line: json!({"kind": "end", "steps": job.steps.len()}) });
                    ev.push(Event::Finished { job: job.name.clone() });
                    host.show(&format!("NVF capture: {} finished.", job.name));
                    self.pending.remove(0);
                    self.announce(host, &mut ev);
                    return ev;
                }
                let mut next = State::Running { step: step + 1, wait: 0, manual: false, polls: 0 };
                let err = |e: String| Event::Output { job: job.name.clone(), line: json!({"kind": "error", "step": step, "error": e}) };
                let vars = &self.vars;
                match &job.steps[step] {
                    Step::Query(q) => {
                        let mut line = json!({"kind": "result", "step": step, "label": q.label, "field": q.field});
                        match expand(&q.query, vars).and_then(|t| host.script(&t)) {
                            Ok(v) => line["value"] = v.to_json(),
                            Err(e) => line["error"] = json!(e),
                        }
                        ev.push(Event::Output { job: job.name.clone(), line });
                    }
                    Step::Script(s) => {
                        if let Err(e) = expand(&s.script, vars).and_then(|t| host.script(&t)) {
                            ev.push(err(e));
                        }
                    }
                    Step::Console(c) => {
                        if let Err(e) = expand(&c.console, vars).and_then(|t| host.console(&t)) {
                            ev.push(err(e));
                        }
                    }
                    Step::Manual(m) => {
                        host.show(&format!("NVF capture: {} Then press End.", m.manual));
                        next = State::Running { step, wait: 0, manual: true, polls: 0 };
                    }
                    Step::Frames(f) => next = State::Running { step: step + 1, wait: f.frames, manual: false, polls: 0 },
                    Step::Until(u) => match expand(&u.until, vars).and_then(|t| host.script(&t)) {
                        Ok(Value::Number(n)) if n != 0.0 => {
                            ev.push(Event::Output { job: job.name.clone(), line: json!({"kind": "waited", "step": step, "frames": polls + 1}) });
                        }
                        Ok(_) if polls + 1 < u.max_frames => next = State::Running { step, wait: 0, manual: false, polls: polls + 1 },
                        Ok(_) => ev.push(err(format!("condition not met in {} game loops", u.max_frames))),
                        Err(e) => ev.push(err(e)),
                    },
                    Step::Observe(o) => {
                        let o = &o.observe;
                        let groups: Vec<&str> = o.groups.iter().map(|g| g.name.as_str()).collect();
                        ev.push(Event::Output {
                            job: job.name.clone(),
                            line: json!({"kind": "observing", "step": step, "label": o.label, "fields": o.fields,
                                         "every": o.every, "groups": groups}),
                        });
                        self.hook_counts = vec![0; o.hooks.len()];
                        self.shots = (0, None);
                        if !o.events.is_empty() {
                            if let Err(e) = host.listen(&o.events) {
                                ev.push(err(format!("events: {e}")));
                            }
                        }
                        next = State::Observing { step, sample: 0, wait: 0, drive_on: o.drive.is_some(), drive_loops: 0 };
                    }
                    Step::Screenshot(s) => {
                        if !valid_name(&s.screenshot) {
                            ev.push(err(format!("screenshot label {:?} must be non-empty [A-Za-z0-9_.-]", s.screenshot)));
                        } else {
                            match host.screenshot(&format!("{}.{step:03}.{}", job.name, s.screenshot)) {
                                Ok(shot) => ev.push(Event::Output {
                                    job: job.name.clone(),
                                    line: json!({"kind": "screenshot", "step": step, "label": s.screenshot, "file": shot.file,
                                                 "width": shot.width, "height": shot.height,
                                                 "mean": (shot.mean * 1e4).round() / 1e4, "foreground": shot.foreground}),
                                }),
                                Err(e) => ev.push(err(format!("screenshot {}: {e}", s.screenshot))),
                            }
                        }
                    }
                    Step::Remember(r) => match expand(&r.remember, vars).and_then(|t| host.script(&t)) {
                        Ok(Value::Number(n)) if n.is_finite() => {
                            self.vars.insert(r.name.clone(), n);
                            ev.push(Event::Output { job: job.name.clone(), line: json!({"kind": "remembered", "step": step, "name": r.name, "value": n}) });
                        }
                        Ok(v) => ev.push(err(format!("remember {}: not a number ({v:?})", r.name))),
                        Err(e) => ev.push(err(e)),
                    },
                }
                self.state = next;
            }
            State::Observing { step, sample, wait, drive_on, drive_loops } => {
                let Step::Observe(o) = &job.steps[step] else { unreachable!("observing a step that is not observe") };
                let mut counts = std::mem::take(&mut self.hook_counts);
                let mut shots = self.shots;
                self.state = self.observe_tick(&job.name, step, &o.observe, (sample, wait, drive_on, drive_loops), end_pressed, host, &mut ev, &mut counts, &mut shots);
                self.hook_counts = counts;
                self.shots = shots;
            }
        }
        ev
    }

    /// One game loop of an `observe` step; returns the next state.
    #[allow(clippy::too_many_arguments)]
    fn observe_tick(&self, job: &str, step: usize, o: &Observe, (sample, wait, mut drive_on, mut drive_loops): (u32, u32, bool, u32),
                    end_pressed: bool, host: &mut dyn Host, ev: &mut Vec<Event>, hook_counts: &mut [u32],
                    shots: &mut (u32, Option<u32>)) -> State {
        let out = |ev: &mut Vec<Event>, line: Json| ev.push(Event::Output { job: job.to_string(), line });
        let vars = &self.vars;
        // Events heard since the last game loop, written before anything else of this one.
        let drain = |ev: &mut Vec<Event>, host: &mut dyn Host| {
            if o.events.is_empty() {
                return;
            }
            let (heard, lost) = host.heard();
            for (k, v) in heard {
                let name = o.events.get(k).map(|e| e.name.as_str()).unwrap_or("?");
                let mut line = json!({"kind": "event", "step": step, "n": sample, "name": name});
                match v {
                    Ok(v) => line["value"] = v.to_json(),
                    Err(e) => line["error"] = json!(e),
                }
                out(ev, line);
            }
            if lost > 0 {
                out(ev, json!({"kind": "events_lost", "step": step, "n": sample, "count": lost}));
            }
        };
        drain(ev, host);
        let stop_drive = |ev: &mut Vec<Event>, host: &mut dyn Host, loops: u32, reason: &str, value: Option<f64>| {
            let mut line = json!({"kind": "drive", "step": step, "loops": loops, "reason": reason});
            if let Some(v) = value {
                line["value"] = json!(v);
            }
            out(ev, line);
            if let Some(d) = &o.drive_end {
                if let Err(e) = expand(d, vars).and_then(|t| host.script(&t)) {
                    out(ev, json!({"kind": "error", "step": step, "error": format!("drive_end: {e}")}));
                }
            }
        };
        if drive_on {
            if let Some(d) = &o.drive {
                drive_loops += 1;
                let reason = match expand(d, vars).and_then(|t| host.script(&t)) {
                    Ok(Value::Number(n)) if n != 0.0 => Some(("arrived".to_string(), Some(n))),
                    Ok(_) if o.drive_frames > 0 && drive_loops >= o.drive_frames => Some(("max_frames".to_string(), None)),
                    Ok(_) => None,
                    Err(e) => Some((format!("error: {e}"), None)),
                };
                if let Some((r, value)) = reason {
                    drive_on = false;
                    stop_drive(ev, host, drive_loops, &r, value);
                }
            }
        }
        if wait > 0 && !end_pressed {
            return State::Observing { step, sample, wait: wait - 1, drive_on, drive_loops };
        }
        let done = if end_pressed {
            Some("end_key".to_string())
        } else if sample >= o.max_samples {
            Some("max_samples".to_string())
        } else if let Some(u) = &o.until {
            match expand(u, vars).and_then(|t| host.script(&t)) {
                Ok(Value::Number(n)) if n != 0.0 => Some("until".to_string()),
                Ok(_) => None,
                Err(e) => Some(format!("until failed: {e}")),
            }
        } else {
            None
        };
        if let Some(reason) = done {
            if drive_on {
                stop_drive(ev, host, drive_loops, "observation ended", None);
            }
            if !o.events.is_empty() {
                host.unlisten();
                drain(ev, host);
            }
            out(ev, json!({"kind": "observed", "step": step, "label": o.label, "samples": sample, "reason": reason}));
            return State::Running { step: step + 1, wait: 0, manual: false, polls: 0 };
        }
        let started = Instant::now();
        let mut errors = 0u32;
        let mut first_error: Option<String> = None;
        let mut note = |e: String| {
            errors += 1;
            first_error.get_or_insert(e);
        };
        for (h, count) in o.hooks.iter().zip(hook_counts.iter_mut()) {
            if *count == u32::MAX {
                continue; // a `once` hook that has fired
            }
            match expand(&h.when, vars).and_then(|t| host.script(&t)) {
                Ok(Value::Number(n)) if n != 0.0 => *count += 1,
                Ok(_) => *count = 0,
                Err(e) => {
                    note(format!("hook {} when: {e}", h.name));
                    *count = 0;
                }
            }
            if *count >= h.after.max(1) {
                let mut line = json!({"kind": "hook", "step": step, "n": sample, "name": h.name, "samples": *count});
                if let Err(e) = expand(&h.then, vars).and_then(|t| host.script(&t)) {
                    line["error"] = json!(e);
                }
                out(ev, line);
                *count = if h.once { u32::MAX } else { 0 };
            }
        }
        let mut header = Vec::with_capacity(o.header.len());
        for h in &o.header {
            match expand(h, vars).and_then(|t| host.script(&t)) {
                Ok(v) => header.push(v.to_json()),
                Err(e) => {
                    note(format!("header: {e}"));
                    header.push(Json::Null);
                }
            }
        }
        let mut groups = serde_json::Map::new();
        for g in &o.groups {
            let n = match expand(&g.count, vars).and_then(|t| host.script(&t)) {
                Ok(Value::Number(n)) if n.is_finite() && n >= 0.0 => (n as u32).min(g.max),
                Ok(v) => {
                    note(format!("{} count: not a number ({v:?})", g.name));
                    groups.insert(g.name.clone(), Json::Null);
                    continue;
                }
                Err(e) => {
                    note(format!("{} count: {e}", g.name));
                    groups.insert(g.name.clone(), Json::Null);
                    continue;
                }
            };
            let mut subjects = Vec::new();
            let mut iv = vars.clone();
            for i in 0..n {
                iv.insert("i".to_string(), f64::from(i));
                let mut parts = Vec::with_capacity(g.probe.len());
                for (k, p) in g.probe.iter().enumerate() {
                    match expand(p, &iv).and_then(|t| host.script(&t)) {
                        Ok(Value::Nothing) if k == 0 => break,
                        Ok(Value::Number(n)) if k == 0 && n == 0.0 => break,
                        Ok(Value::Text(t)) if k == 0 && t.is_empty() => break,
                        Ok(v) => parts.push(v.to_json()),
                        Err(e) => {
                            note(format!("{} subject {i} part {k}: {e}", g.name));
                            parts.push(Json::Null);
                        }
                    }
                }
                if !parts.is_empty() {
                    subjects.push(Json::Array(parts));
                }
            }
            groups.insert(g.name.clone(), Json::Array(subjects));
        }
        let mut line = json!({"kind": "sample", "step": step, "n": sample, "header": header, "groups": groups,
                              "errors": errors, "us": started.elapsed().as_micros() as u64});
        if let Some(e) = first_error {
            line["first_error"] = json!(e);
        }
        out(ev, line);
        if let Some(sh) = &o.shots {
            let due = shots.0 < sh.max && shots.1.is_none_or(|last| sample >= last + sh.gap.max(1));
            if due {
                match expand(&sh.when, vars).and_then(|t| host.script(&t)) {
                    Ok(Value::Number(n)) if n != 0.0 => {
                        let mut line = json!({"kind": "screenshot", "step": step, "n": sample, "label": sh.label});
                        match host.screenshot(&format!("{job}.{step:03}.{}.{sample:06}", sh.label)) {
                            Ok(shot) => {
                                line["file"] = json!(shot.file);
                                line["mean"] = json!((shot.mean * 1e4).round() / 1e4);
                                line["foreground"] = json!(shot.foreground);
                            }
                            Err(e) => line["error"] = json!(e),
                        }
                        out(ev, line);
                        *shots = (shots.0 + 1, Some(sample));
                    }
                    Ok(_) => {}
                    Err(e) => out(ev, json!({"kind": "error", "step": step, "error": format!("shots when: {e}")})),
                }
            }
        }
        State::Observing { step, sample: sample + 1, wait: o.every.saturating_sub(1), drive_on, drive_loops }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn forms_and_arrays_as_json() {
        let v = Value::List(vec![Value::Form("00000014".into()), Value::Number(2.5), Value::Text("a".into()), Value::List(vec![]), Value::Nothing]);
        assert_eq!(v.to_json(), json!([{ "form": "00000014" }, 2.5, "a", [], null]));
        let m = Value::Map(vec![("a".into(), Value::Number(1.0)), ("b".into(), Value::Form("0000000F".into()))]);
        assert_eq!(m.to_json(), json!({ "a": 1.0, "b": { "form": "0000000F" } }));
    }

    #[derive(Default)]
    struct Mock {
        keys: VecDeque<(bool, bool)>,
        scripts: Vec<String>,
        console: Vec<String>,
        shown: Vec<String>,
        shots: Vec<String>,
        /// Event names listened to, events waiting to be heard, and unlisten calls.
        listening: Vec<String>,
        queue: Vec<Heard>,
        lost: u32,
        unlistened: u32,
    }

    impl Host for Mock {
        fn script(&mut self, text: &str) -> Result<Value, String> {
            self.scripts.push(text.into());
            match text {
                "count" => Ok(Value::Number(5.0)),
                "text" => Ok(Value::Text("a|b".into())),
                "none" => Ok(Value::Nothing),
                "clock" => Ok(Value::Number(6.5)),
                t if t.starts_with("ready ") => {
                    // Non-zero from the n-th call with this text on.
                    let n: usize = t[6..].parse().unwrap();
                    Ok(Value::Number(if self.scripts.iter().filter(|s| *s == t).count() >= n { 1.0 } else { 0.0 }))
                }
                "add 6.5" => Ok(Value::Number(7.5)),
                "subj 1" => Ok(Value::Number(0.0)),
                t if t.starts_with("subj ") => Ok(Value::Text(format!("s{}", &t[5..]))),
                "part 0" => Ok(Value::Number(7.0)),
                _ => Err("compile failed".into()),
            }
        }
        fn console(&mut self, line: &str) -> Result<(), String> {
            self.console.push(line.into());
            Ok(())
        }
        fn key_down(&mut self, scancode: u32) -> bool {
            // One (End, Home) pair per tick, consumed on the Home query.
            let (e, h) = self.keys.front().copied().unwrap_or((false, false));
            if scancode == KEY_HOME {
                self.keys.pop_front();
                h
            } else {
                e
            }
        }
        fn show(&mut self, text: &str) {
            self.shown.push(text.into());
        }
        fn screenshot(&mut self, stem: &str) -> Result<Shot, String> {
            self.shots.push(stem.into());
            Ok(Shot { file: format!("shots/{stem}.1.png"), width: 4, height: 2, mean: 0.123456, foreground: true })
        }
        fn listen(&mut self, events: &[EventSpec]) -> Result<(), String> {
            self.listening = events.iter().map(|e| e.name.clone()).collect();
            Ok(())
        }
        fn heard(&mut self) -> (Vec<Heard>, u32) {
            (std::mem::take(&mut self.queue), std::mem::take(&mut self.lost))
        }
        fn unlisten(&mut self) {
            self.unlistened += 1;
            self.listening.clear();
        }
    }

    // The mock host answers stand-in texts ("count", "ready 3"), not scripts, so the
    // runner's tests take jobs as written (`Job::read`); short scripts have their own
    // tests below and in short.rs.
    const JOB: &str = r#"{"format": "fnv.capture.steps/1", "name": "t1", "title": "Test", "start": "Load a save.",
      "steps": [{"query": "count", "label": "caps", "field": "count"},
                {"script": "bad"},
                {"console": "player.setlevel 5"},
                {"manual": "Wait 24 hours."},
                {"frames": 2},
                {"query": "text", "label": "mods", "field": "names"},
                {"query": "none", "label": "x", "field": "y"}]}"#;

    fn lines(ev: &[Event]) -> Vec<Json> {
        ev.iter().filter_map(|e| if let Event::Output { line, .. } = e { Some(line.clone()) } else { None }).collect()
    }

    #[test]
    fn parses_every_step_kind_and_rejects_bad_jobs() {
        let j = Job::read(JOB).unwrap();
        assert_eq!(j.steps.len(), 7);
        assert!(matches!(j.steps[0], Step::Query(_)));
        assert!(matches!(j.steps[1], Step::Script(_)));
        assert!(matches!(j.steps[2], Step::Console(_)));
        assert!(matches!(j.steps[3], Step::Manual(_)));
        assert_eq!(j.steps[4], Step::Frames(FramesStep { frames: 2 }));
        assert!(Job::read(&JOB.replace("fnv.capture.steps/1", "x")).is_err());
        assert!(Job::read(&JOB.replace("\"t1\"", "\"../evil\"")).is_err());
        assert!(Job::read(&JOB.replace("\"frames\": 2", "\"frames\": 2, \"extra\": 1")).is_err());
    }

    #[test]
    fn runs_a_job_step_by_step() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0x0604_0090, 0x0400_20D0);
        assert!(r.tick(&mut m).is_empty(), "idle before a load");
        r.game_loaded(vec![Job::read(JOB).unwrap()], &mut m);
        assert!(m.shown[0].contains("Press End to start"));
        let mut all = Vec::new();
        // tick 1: nothing pressed; tick 2: End pressed -> start.
        m.keys.extend([(false, false), (true, false)]);
        all.extend(r.tick(&mut m));
        all.extend(r.tick(&mut m));
        assert!(r.is_running());
        // Holding End must not skip the manual step later: release first.
        for _ in 0..4 {
            m.keys.push_back((false, false));
            all.extend(r.tick(&mut m)); // query, script, console, manual shown
        }
        assert!(m.shown.last().unwrap().contains("Wait 24 hours"));
        for _ in 0..3 {
            m.keys.push_back((false, false));
            all.extend(r.tick(&mut m)); // still waiting for End
        }
        m.keys.extend([(true, false), (false, false), (false, false), (false, false)]);
        for _ in 0..4 {
            all.extend(r.tick(&mut m)); // End -> frames step sets wait 2 -> two waits
        }
        for _ in 0..4 {
            m.keys.push_back((false, false));
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        assert_eq!(out[0]["kind"], "start");
        assert_eq!(out[0]["nvse_version"], "0x06040090");
        assert_eq!(out[1], json!({"kind": "result", "step": 0, "label": "caps", "field": "count", "value": 5.0}));
        assert_eq!(out[2]["kind"], "error");
        assert_eq!(out[2]["step"], 1);
        assert_eq!(out[3]["value"], "a|b");
        assert_eq!(out[4]["value"], Json::Null);
        assert_eq!(out[5]["kind"], "end");
        assert!(all.contains(&Event::Finished { job: "t1".into() }));
        assert_eq!(m.console, vec!["player.setlevel 5"]);
        assert!(r.current().is_none());
        assert!(!r.is_running());
    }

    #[test]
    fn home_skips_and_a_load_aborts() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0, 0);
        let a = Job::read(JOB).unwrap();
        let mut b = a.clone();
        b.name = "t2".into();
        r.game_loaded(vec![a.clone(), b.clone()], &mut m);
        m.keys.push_back((false, true));
        r.tick(&mut m);
        assert_eq!(r.current().unwrap().name, "t2");
        m.keys.push_back((true, false));
        r.tick(&mut m);
        assert!(r.is_running());
        let ev = r.game_loaded(vec![a, b], &mut m);
        assert!(ev.contains(&Event::Aborted { job: "t2".into() }));
        assert!(!r.is_running());
        assert_eq!(r.current().unwrap().name, "t1");
    }

    #[test]
    fn until_polls_and_remember_feeds_later_steps() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0, 0);
        r.set_auto_start(Some(0));
        let job = Job::read(r#"{"format": "fnv.capture.steps/1", "name": "u1", "steps": [
            {"until": "ready 3", "max_frames": 10},
            {"until": "ready 99", "max_frames": 2},
            {"remember": "clock", "as": "t0"},
            {"query": "add %{t0}", "label": "x", "field": "v"},
            {"console": "set TimeScale to %{t0}"},
            {"query": "add %{nope}", "label": "y", "field": "v"},
            {"remember": "text", "as": "bad"}]}"#).unwrap();
        r.game_loaded(vec![job], &mut m);
        let mut all = Vec::new();
        for _ in 0..20 {
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        assert_eq!(out[1], json!({"kind": "waited", "step": 0, "frames": 3}));
        assert_eq!(out[2]["kind"], "error");
        assert!(out[2]["error"].as_str().unwrap().contains("2 game loops"));
        assert_eq!(m.scripts.iter().filter(|s| *s == "ready 99").count(), 2);
        assert_eq!(out[3], json!({"kind": "remembered", "step": 2, "name": "t0", "value": 6.5}));
        assert_eq!(out[4]["value"], 7.5);
        assert_eq!(m.console, vec!["set TimeScale to 6.5"]);
        assert!(out[5]["error"].as_str().unwrap().contains("%{nope}"));
        assert_eq!(out[6]["kind"], "error");
        assert_eq!(out[7]["kind"], "end");
    }

    #[test]
    fn observe_samples_groups_drives_and_stops() {
        let mut m = Mock::default();
        let mut r = Runner::new(4, 0, 0);
        r.set_auto_start(Some(0));
        // Subjects: index 0 is kept (two parts), index 1 is left out (first part 0),
        // index 2's second part fails. `until` is met at its 3rd check; the drive at its
        // 2nd call. Samples every 2 game loops.
        let job = Job::read(r#"{"format": "fnv.capture.steps/1", "name": "o1", "steps": [
            {"observe": {"label": "near", "fields": {"header": ["h"]}, "header": ["clock", "bad"],
                         "groups": [{"name": "actors", "count": "count", "max": 3,
                                     "probe": ["subj %{i}", "part %{i}"]}],
                         "every": 2, "until": "ready 3", "max_samples": 10,
                         "drive": "ready 2", "drive_end": "count"}},
            {"query": "count", "label": "after", "field": "x"}]}"#).unwrap();
        r.game_loaded(vec![job], &mut m);
        let mut all = Vec::new();
        for _ in 0..12 {
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        let kinds: Vec<&str> = out.iter().map(|l| l["kind"].as_str().unwrap()).collect();
        assert_eq!(kinds, ["start", "observing", "sample", "drive", "sample", "observed", "result", "end"]);
        assert_eq!(out[1]["fields"], json!({"header": ["h"]}));
        assert_eq!(out[1]["groups"], json!(["actors"]));
        assert_eq!(out[3]["reason"], "arrived");
        assert_eq!(out[3]["value"], 1.0, "the drive's non-zero result is recorded");
        assert_eq!(out[3]["loops"], 2);
        let s = &out[2];
        assert_eq!(s["n"], 0);
        assert_eq!(s["header"], json!([6.5, null]));
        assert_eq!(s["groups"]["actors"], json!([["s0", 7.0], ["s2", null]]), "count 5 capped at max 3");
        assert_eq!(s["errors"], 2);
        assert!(s["first_error"].as_str().unwrap().starts_with("header"));
        assert_eq!(out[5]["samples"], 2);
        assert_eq!(out[5]["reason"], "until");
        assert_eq!(m.scripts.iter().filter(|t| *t == "ready 2").count(), 2, "the drive stops once it returns non-zero");
        assert_eq!(m.scripts.iter().filter(|t| *t == "subj 0").count(), 2);
    }

    #[test]
    fn observe_stops_at_max_samples_or_end_and_ends_the_drive() {
        let job = r#"{"format": "fnv.capture.steps/1", "name": "o2", "steps": [
            {"observe": {"label": "x", "every": 1, "max_samples": 2, "drive": "ready 99", "drive_end": "count"}}]}"#;
        let mut m = Mock::default();
        let mut r = Runner::new(4, 0, 0);
        r.set_auto_start(Some(0));
        r.game_loaded(vec![Job::read(job).unwrap()], &mut m);
        let mut all = Vec::new();
        for _ in 0..8 {
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        let ended: Vec<&Json> = out.iter().filter(|l| l["kind"] == "observed" || l["kind"] == "drive").collect();
        assert_eq!(ended[0]["reason"], "observation ended", "drive_end runs when the observation ends first");
        assert_eq!(ended[1]["reason"], "max_samples");
        assert_eq!(m.scripts.iter().filter(|t| *t == "count").count(), 1);
        // End stops an observation at once (attended runs).
        let mut m = Mock::default();
        let mut r = Runner::new(4, 0, 0);
        r.set_auto_start(Some(0));
        r.game_loaded(vec![Job::read(&job.replace("\"max_samples\": 2", "\"max_samples\": 1000")).unwrap()], &mut m);
        let mut all = Vec::new();
        for _ in 0..4 {
            all.extend(r.tick(&mut m));
        }
        m.keys.extend([(true, false)]);
        all.extend(r.tick(&mut m));
        assert!(lines(&all).iter().any(|l| l["kind"] == "observed" && l["reason"] == "end_key"));
    }

    #[test]
    fn hooks_fire_after_consecutive_samples() {
        let mut m = Mock::default();
        let mut r = Runner::new(6, 0, 0);
        r.set_auto_start(Some(0));
        // "ready 3" is non-zero from its 3rd call on: true at samples 2, 3, 4, ...; with
        // after 2 the hook fires at sample 3 and, counting again, at sample 5.
        let job = Job::read(r#"{"format": "fnv.capture.steps/1", "name": "h1", "steps": [
            {"observe": {"label": "x", "every": 1, "max_samples": 6,
                         "hooks": [{"name": "menu", "when": "ready 3", "after": 2, "then": "count"},
                                   {"name": "bad", "when": "nope", "after": 1, "then": "count"}]}}]}"#).unwrap();
        r.game_loaded(vec![job], &mut m);
        let mut all = Vec::new();
        for _ in 0..12 {
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        let fired: Vec<(u64, &str)> = out.iter().filter(|l| l["kind"] == "hook")
            .map(|l| (l["n"].as_u64().unwrap(), l["name"].as_str().unwrap())).collect();
        assert_eq!(fired, [(3, "menu"), (5, "menu")]);
        let once = Job::read(r#"{"format": "fnv.capture.steps/1", "name": "h2", "steps": [
            {"observe": {"label": "x", "every": 1, "max_samples": 6,
                         "hooks": [{"name": "push", "when": "ready 1", "after": 2, "then": "text", "once": true}]}}]}"#).unwrap();
        r.game_loaded(vec![once], &mut m);
        let mut all = Vec::new();
        for _ in 0..12 {
            all.extend(r.tick(&mut m));
        }
        let fired: Vec<u64> = lines(&all).iter().filter(|l| l["kind"] == "hook").map(|l| l["n"].as_u64().unwrap()).collect();
        assert_eq!(fired, [1], "a once hook fires at its sample and never again");
        assert!(out.iter().filter(|l| l["kind"] == "sample").all(|l| l["errors"] == 1), "a failing hook is an error, not a stop");
        assert_eq!(m.scripts.iter().filter(|t| *t == "count").count(), 2);
    }

    #[test]
    fn events_are_written_as_heard_and_shots_follow_their_condition() {
        let mut m = Mock::default();
        let mut r = Runner::new(9, 0, 0);
        r.set_auto_start(Some(0));
        // "ready 2" is true from sample 1 on; gap 2 and max 2 give shots at samples 1 and 3.
        let job = Job::read(r#"{"format": "fnv.capture.steps/1", "name": "e1", "steps": [
            {"observe": {"label": "x", "every": 1, "max_samples": 6,
                         "events": [{"name": "OnHit", "probe": "p"}, {"name": "OnDeath", "probe": "q"}],
                         "shots": {"label": "down", "when": "ready 2", "gap": 2, "max": 2}}}]}"#).unwrap();
        r.game_loaded(vec![job], &mut m);
        let mut all = Vec::new();
        all.extend(r.tick(&mut m)); // start
        all.extend(r.tick(&mut m)); // observing: listen
        assert_eq!(m.listening, ["OnHit", "OnDeath"]);
        for k in 0..8 {
            if k == 2 {
                m.queue.push((1, Ok(Value::Text("a|b".into()))));
                m.queue.push((0, Err("script call failed".into())));
                m.lost = 3;
            }
            all.extend(r.tick(&mut m));
        }
        let out = lines(&all);
        let evs: Vec<&Json> = out.iter().filter(|l| l["kind"] == "event").collect();
        assert_eq!(evs.len(), 2);
        assert_eq!((evs[0]["name"].as_str(), evs[0]["value"].as_str()), (Some("OnDeath"), Some("a|b")));
        assert_eq!((evs[1]["name"].as_str(), evs[1]["error"].as_str()), (Some("OnHit"), Some("script call failed")));
        assert!(out.iter().any(|l| l["kind"] == "events_lost" && l["count"] == 3));
        let shots: Vec<u64> = out.iter().filter(|l| l["kind"] == "screenshot").map(|l| l["n"].as_u64().unwrap()).collect();
        assert_eq!(shots, [1, 3]);
        assert_eq!(m.shots[0], "e1.000.down.000001");
        assert_eq!(m.unlistened, 1, "the observation's end stops the events");
        assert!(out.iter().any(|l| l["kind"] == "observed"));
    }

    #[test]
    fn a_load_during_an_observation_stops_the_events() {
        let mut m = Mock::default();
        let mut r = Runner::new(9, 0, 0);
        r.set_auto_start(Some(0));
        let job = r#"{"format": "fnv.capture.steps/1", "name": "e2", "steps": [
            {"observe": {"label": "x", "every": 1, "max_samples": 60, "events": [{"name": "OnHit", "probe": "p"}]}}]}"#;
        r.game_loaded(vec![Job::read(job).unwrap()], &mut m);
        r.tick(&mut m);
        r.tick(&mut m);
        r.tick(&mut m);
        r.game_loaded(vec![], &mut m);
        assert_eq!(m.unlistened, 1);
    }

    #[test]
    fn event_probes_must_be_full_scripts() {
        let bad = r#"{"format": "fnv.capture.steps/1", "name": "e3", "steps": [
            {"observe": {"label": "x", "every": 1, "max_samples": 1, "events": [{"name": "OnHit", "probe": "GetHealth"}]}}]}"#;
        assert!(Job::parse(bad).unwrap_err().contains("full function script"));
        let good = bad.replace("GetHealth", r"ref rEa\nref rEb\nBegin Function {rEa, rEb}\nSetFunctionValue 1\nEnd\n");
        assert!(Job::parse(&good).is_ok());
    }

    #[test]
    fn parses_the_committed_observer_diagnostic_job() {
        // Generated by tools/capture/observe.py (diag); keeps the two sides in step.
        let j = Job::parse(include_str!("../../jobs/aa_observe_diag.steps.json")).unwrap();
        assert!(j.steps.iter().any(|s| matches!(s, Step::Observe(o) if o.observe.groups.len() == 1)));
    }

    #[test]
    fn parses_the_examples_shipped_with_a_release() {
        // package/examples (tools/capture/package_plugin.py puts them into the archive).
        let j = Job::parse(include_str!("../package/examples/example_read_only.steps.json")).unwrap();
        assert!(j.steps.iter().all(|s| matches!(s, Step::Query(_))), "read-only: queries only");
        let j = Job::parse(include_str!("../package/examples/example_wait_and_screenshot.steps.json")).unwrap();
        assert!(matches!(j.steps[0], Step::Remember(_)) && matches!(j.steps[1], Step::Until(_)));
        assert!(j.steps.iter().any(|s| matches!(s, Step::Screenshot(_))));
        let j = Job::parse(include_str!("../package/examples/example_items.steps.json")).unwrap();
        assert!(matches!(&j.steps[1], Step::Script(s) if s.script.contains("let rF1 := BuildRef fM1 15\nplayer.AddItem rF1 3 1\n")));
        let j = Job::parse(include_str!("../package/examples/example_manual_step.steps.json")).unwrap();
        assert!(matches!(j.steps[2], Step::Manual(_)));
        let j = Job::parse(include_str!("../package/examples/example_observe.steps.json")).unwrap();
        assert!(matches!(&j.steps[0], Step::Observe(o) if o.observe.header.len() == 5 && o.observe.header.iter().all(|h| short::is_full_script(h))));
    }

    #[test]
    fn short_scripts_are_written_out_when_a_job_is_parsed() {
        let j = Job::parse(r#"{"format": "fnv.capture.steps/1", "name": "s1",
            "forms": {"caps": "FalloutNV.esm:0000000F"},
            "steps": [{"query": "player.GetItemCount @caps"},
                      {"query": "GetNthModName 0", "text": true, "label": "mods", "field": "first"},
                      {"script": ["player.AddItem @caps 3 1", "player.RemoveItem @caps 1"]},
                      {"until": "GameHour >= 12", "max_frames": 10},
                      {"remember": "GameDaysPassed", "as": "day0"},
                      {"query": ["float fV", "Begin Function { }", "let fV := 2", "SetFunctionValue fV", "End"], "label": "l", "field": "f"},
                      {"observe": {"label": "o", "every": 1, "max_samples": 1, "header": ["GameHour"], "until": "GameHour >= 13",
                                   "hooks": [{"name": "h", "when": "MenuMode 1009", "after": 2, "then": "CloseAllMenus"}]}}]}"#).unwrap();
        let Step::Query(q) = &j.steps[0] else { panic!() };
        assert_eq!(q.field, "player.GetItemCount @caps", "the field defaults to the query as written");
        assert_eq!(q.query, "float fM1\nref rF1\nfloat fV\nBegin Function { }\nlet fM1 := GetModIndex \"FalloutNV.esm\"\nlet rF1 := BuildRef fM1 15\nlet fV := player.GetItemCount rF1\nSetFunctionValue fV\nEnd\n");
        let Step::Query(q) = &j.steps[1] else { panic!() };
        assert_eq!((q.query.as_str(), q.field.as_str()), ("Begin Function { }\nSetFunctionValue (GetNthModName 0)\nEnd\n", "first"));
        let Step::Script(s) = &j.steps[2] else { panic!() };
        assert!(s.script.ends_with("player.AddItem rF1 3 1\nplayer.RemoveItem rF1 1\nSetFunctionValue 1\nEnd\n"), "{}", s.script);
        let Step::Until(u) = &j.steps[3] else { panic!() };
        assert!(u.until.contains("if GameHour >= 12\n  let fV := 1\nendif"));
        let Step::Remember(r) = &j.steps[4] else { panic!() };
        assert!(r.remember.contains("let fV := GameDaysPassed\n"));
        let Step::Query(q) = &j.steps[5] else { panic!() };
        assert_eq!(q.query, "float fV\nBegin Function { }\nlet fV := 2\nSetFunctionValue fV\nEnd\n", "a full script as lines is only joined");
        let Step::Observe(o) = &j.steps[6] else { panic!() };
        assert!(o.observe.header[0].contains("let fV := GameHour\n") && o.observe.until.as_ref().unwrap().contains("if GameHour >= 13\n"));
        assert!(o.observe.hooks[0].when.contains("if MenuMode 1009\n") && o.observe.hooks[0].then.contains("CloseAllMenus\nSetFunctionValue 1\n"));
        // Mistakes name the step; a job with one is skipped as a whole.
        let bad = |steps: &str, forms: &str| Job::parse(&format!(r#"{{"format": "fnv.capture.steps/1", "name": "b", "forms": {forms}, "steps": [{{"frames": 1}}, {steps}]}}"#)).unwrap_err();
        assert!(bad(r#"{"query": "player.GetItemCount @caps"}"#, "{}").starts_with("step 1: @caps"));
        assert!(bad(r#"{"frames": 1}"#, r#"{"caps": "Caps001"}"#).contains("FormID in hex"));
        assert!(bad(r#"{"frames": 1}"#, r#"{"my caps": "A.esm:F"}"#).contains("form name"));
    }

    #[test]
    fn expand_replaces_names() {
        let vars = HashMap::from([("a".to_string(), 1.25), ("b".to_string(), 100000.0)]);
        assert_eq!(expand("x %{a} y %{b}", &vars).unwrap(), "x 1.25 y 100000");
        assert_eq!(expand("no names", &vars).unwrap(), "no names");
        assert!(expand("%{a", &vars).is_err());
    }

    #[test]
    fn automatic_mode_starts_after_a_delay_and_reports_all_done() {
        let mut m = Mock::default();
        let mut r = Runner::new(1, 0, 0);
        r.set_auto_start(Some(2));
        let job = Job::read(r#"{"format": "fnv.capture.steps/1", "name": "a1",
            "steps": [{"query": "count", "label": "caps", "field": "count"}, {"manual": "Do it."}]}"#).unwrap();
        r.game_loaded(vec![job], &mut m);
        assert!(m.shown[0].contains("Starting automatically"));
        let mut all = Vec::new();
        for _ in 0..2 {
            all.extend(r.tick(&mut m)); // the start delay, no key pressed
        }
        assert!(!r.is_running());
        all.extend(r.tick(&mut m));
        assert!(r.is_running());
        all.extend(r.tick(&mut m)); // query
        for _ in 0..5 {
            all.extend(r.tick(&mut m)); // a manual step still waits for End
        }
        assert!(r.is_running());
        assert!(!all.contains(&Event::AllDone));
        m.keys.extend([(true, false), (false, false)]);
        all.extend(r.tick(&mut m));
        all.extend(r.tick(&mut m)); // end of job
        assert!(all.contains(&Event::Finished { job: "a1".into() }));
        assert_eq!(all.last(), Some(&Event::AllDone));
        assert_eq!(lines(&all)[1]["value"], 5.0);
        // No pending jobs at a load: done at once; never in manual mode.
        assert_eq!(r.game_loaded(Vec::new(), &mut m), vec![Event::AllDone]);
        let mut manual = Runner::new(1, 0, 0);
        assert!(manual.game_loaded(Vec::new(), &mut m).is_empty());
    }
    #[test]
    fn screenshot_steps_record_the_file_or_an_error() {
        let job = r#"{"format": "fnv.capture.steps/1", "name": "fr", "steps": [
            {"screenshot": "goodsprings_noon"}, {"screenshot": "../bad"}]}"#;
        let mut m = Mock::default();
        let mut r = Runner::new(7, 0, 0);
        r.set_auto_start(Some(0));
        r.game_loaded(vec![Job::read(job).unwrap()], &mut m);
        let mut all = Vec::new();
        for _ in 0..5 {
            all.extend(r.tick(&mut m));
        }
        assert_eq!(m.shots, ["fr.000.goodsprings_noon"]);
        let out = lines(&all);
        assert_eq!(out[1], json!({"kind": "screenshot", "step": 0, "label": "goodsprings_noon",
                                  "file": "shots/fr.000.goodsprings_noon.1.png", "width": 4, "height": 2,
                                  "mean": 0.1235, "foreground": true}));
        assert_eq!(out[2]["kind"], "error");
        assert_eq!(out[2]["step"], 1);
        assert_eq!(out[3]["kind"], "end");
    }
}
