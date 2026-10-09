//! The headless script runner: `fnv.script.actions/1` in, `fnv.script.trace/1` out.
//!
//! The contract is docs/spec-notes/layer5-captures.md §4 (written by workstream 5C for
//! the `L5.quest.*` scenarios): the same operations, the same frames between them, and
//! observations keyed by `label`. Operations beyond that contract stand in for Layer 6
//! systems in headless walk-throughs (dialogue lines, package events, menus, message
//! box buttons) and are listed in docs/spec-notes/scripts.md §8.

use crate::functions::Ctx;
use crate::game::{event, ActMode, Game, GameConfig, InstKey};
use crate::GameData;
use fnv_script::vm::{literal, to_int_storage, ArgValue, Value};
use fnv_sim::world::SimConfig;
use fnv_sim::{FormId, FormStore, RefTable, World};
use serde::Deserialize;
use serde_json::{json, Map, Value as Json};
use std::collections::BTreeMap;

pub const ACTIONS_FORMAT: &str = "fnv.script.actions/1";
pub const TRACE_FORMAT: &str = "fnv.script.trace/1";

fn default_frame() -> f64 {
    1.0 / 60.0
}
fn one() -> u16 {
    1
}
fn yes() -> bool {
    true
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Input {
    pub format: String,
    #[serde(default)]
    pub seed: u64,
    #[serde(default = "default_frame")]
    pub frame_seconds: f64,
    #[serde(default)]
    pub title: Option<String>,
    /// Informational: the capture save a golden came from.
    #[serde(default)]
    pub save: Option<String>,
    #[serde(default = "one")]
    pub player_level: u16,
    #[serde(default)]
    pub hardcore: bool,
    /// Start from the new-game state (start-game-enabled quests running). Default true.
    #[serde(default = "yes")]
    pub new_game: bool,
    /// Trace writes to an instance's own locals too.
    #[serde(default)]
    pub trace_locals: bool,
    #[serde(default)]
    pub given: Vec<Given>,
    pub actions: Vec<Action>,
    /// References whose final state is reported.
    #[serde(default)]
    pub report: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Given {
    Quest {
        quest: String,
        #[serde(default)]
        running: Option<bool>,
        #[serde(default)]
        completed: Option<bool>,
        #[serde(default)]
        stage: Option<i32>,
        #[serde(default)]
        done: Vec<i32>,
        #[serde(default)]
        not_done: Vec<i32>,
        #[serde(default)]
        objectives: Vec<GivenObjective>,
        #[serde(default)]
        vars: BTreeMap<String, f64>,
    },
    Global { name: String, value: f32 },
    ItemCount { of: String, item: String, count: i32 },
    Disabled { of: String, disabled: bool },
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GivenObjective {
    pub index: i32,
    #[serde(default)]
    pub displayed: Option<bool>,
    #[serde(default)]
    pub completed: Option<bool>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Action {
    // ---- the 5C contract ----
    SetStage { quest: String, stage: i32 },
    SetStageRead { quest: String, stage: i32, field: String, label: String },
    StartQuest { quest: String },
    StopQuest { quest: String },
    SetQuestVar { quest: String, var: String, value: f64 },
    SetLocal { #[serde(rename = "type")] ty: String, value: f64, #[serde(default)] label: Option<String> },
    SetObjective { quest: String, index: i32, #[serde(default)] displayed: Option<bool>, #[serde(default)] completed: Option<bool> },
    GetQuest { quest: String, fields: Vec<String>, #[serde(default)] label: Option<String> },
    SampleVar { quest: String, var: String, frames: u64, #[serde(default)] runs: Option<usize>, #[serde(default)] skip: usize, #[serde(default)] label: Option<String> },
    WaitFrames { frames: u64 },
    AddItem { to: String, item: String, count: i32 },
    RemoveItem { from: String, item: String, count: i32 },
    RemoveAllItems { from: String, #[serde(default)] to: Option<String> },
    Enable { target: String },
    Disable { target: String },
    SetGlobal { name: String, value: f32 },
    Activate { #[serde(default)] actor: Option<String>, target: String, #[serde(default)] label: Option<String> },
    GetItemCount { of: String, item: String, #[serde(default)] label: Option<String> },
    GetDisabled { of: String, #[serde(default)] label: Option<String> },
    GetGlobal { name: String, #[serde(default)] label: Option<String> },
    // ---- headless stand-ins (scripts.md §8) ----
    /// Seconds of game loop at the fixed frame step.
    AdvanceSeconds { seconds: f64 },
    /// An actor (the player by default) activates by hand: OnActivate instead of the
    /// default action.
    PlayerActivate { target: String, #[serde(default)] actor: Option<String> },
    /// A dialogue line (INFO) is said: its result scripts run on the speaker.
    SayInfo { info: String, #[serde(default)] speaker: Option<String> },
    /// A package starts, finishes or changes on an actor.
    Package { actor: String, package: String, event: String },
    /// A `SayTo` line finished (SayToDone blocks of the speaker).
    SayToDone { actor: String, #[serde(default)] topic: Option<String> },
    /// An actor enters a trigger (OnTriggerEnter of the trigger reference).
    TriggerEnter { trigger: String, #[serde(default)] actor: Option<String> },
    /// The player picks a button of the open message box.
    PressButton { button: i32 },
    /// A menu opens (MenuMode with its code) or closes (`null`).
    Menu { menu: Option<u16> },
    /// The player moves to a reference (and its cells load).
    MovePlayer { to: String },
    LoadCell { cell: String },
    /// A script variable of a quest or reference, by name.
    GetVar { of: String, var: String, #[serde(default)] label: Option<String> },
    /// Enabled state, cell, position and Layer 5 flags of a reference.
    GetRef { of: String, #[serde(default)] label: Option<String> },
    Kill { actor: String, #[serde(default)] killer: Option<String> },
}

/// Frames that pass after an action (layer5-captures.md §4.1).
fn frames_after(a: &Action) -> u64 {
    match a {
        Action::SetStage { .. } | Action::SetStageRead { .. } => 31,
        Action::StartQuest { .. } | Action::StopQuest { .. } | Action::SetQuestVar { .. } | Action::SetObjective { .. } => 11,
        Action::SetLocal { .. } => 1,
        Action::Activate { .. } | Action::RemoveAllItems { .. } => 31,
        Action::AddItem { .. } | Action::RemoveItem { .. } | Action::Enable { .. } | Action::Disable { .. } | Action::SetGlobal { .. } => 11,
        Action::GetItemCount { .. } | Action::GetDisabled { .. } | Action::GetGlobal { .. } => 11,
        Action::GetQuest { .. } | Action::SampleVar { .. } | Action::WaitFrames { .. } | Action::AdvanceSeconds { .. } => 0,
        // Stand-ins: one frame.
        _ => 1,
    }
}

/// Name resolution: `player`, a hex FormID, or an Editor ID (any record).
pub fn resolve(g: &Game, s: &str) -> Result<FormId, String> {
    if s.eq_ignore_ascii_case("player") || s.eq_ignore_ascii_case("playerref") {
        return Ok(FormId::PLAYER_REF);
    }
    if s.starts_with("0x") || s.starts_with("0X") {
        return s.parse().map_err(|e: fnv_sim::form::BadFormId| e.to_string());
    }
    if let Some(r) = g.world.placed.by_edid(s) {
        return Ok(r);
    }
    g.data.by_edid(s).or_else(|| g.world.placed.cell_by_edid(s)).ok_or_else(|| format!("no form named {s:?}"))
}

fn quest(g: &Game, s: &str) -> Result<FormId, String> {
    let q = resolve(g, s)?;
    if g.data.is_quest(q) {
        Ok(q)
    } else {
        Err(format!("{s:?} is not a quest"))
    }
}

/// A quest variable by name: (instance, index).
fn quest_var(g: &Game, q: FormId, var: &str) -> Result<(InstKey, u16), String> {
    let def = &g.data.quests[&q];
    let script = def.script.and_then(|s| g.data.script(s)).ok_or_else(|| format!("{} has no script", g.data.name(q)))?;
    let i = script.var_index(var).ok_or_else(|| format!("{} declares no variable {var:?}", g.data.name(q)))?;
    Ok((InstKey::Quest { quest: q }, i))
}

fn var_of(g: &Game, of: FormId, var: &str) -> Result<(InstKey, FormId, u16), String> {
    let (key, script) = g.instance_of(of).ok_or_else(|| format!("{} runs no script", g.data.name(of)))?;
    let i = g.data.script(script).and_then(|s| s.var_index(var)).ok_or_else(|| format!("no variable {var:?}"))?;
    Ok((key, script, i))
}

/// Store a number in a quest variable as given (xNVSE SetVariable in the capture),
/// with the variable's declared storage.
fn set_quest_var(g: &mut Game, q: FormId, var: &str, value: f64) -> Result<(), String> {
    let (key, i) = quest_var(g, q, var)?;
    let script = g.data.quests[&q].script;
    g.store(key, script, i, Value::Num(value), "runner", false);
    Ok(())
}

fn field(g: &Game, q: FormId, f: &str) -> Result<Json, String> {
    let s = g.quest_state(q);
    Ok(match f {
        "running" => json!(s.running),
        "completed" => json!(s.completed || s.failed),
        "stage" => json!(s.stage()),
        _ => {
            if let Some(n) = f.strip_prefix("done.") {
                let n: i32 = n.parse().map_err(|_| format!("bad field {f:?}"))?;
                json!(s.stages_done.contains(&n))
            } else if let Some(rest) = f.strip_prefix("objective.") {
                let (n, which) = rest.split_once('.').ok_or_else(|| format!("bad field {f:?}"))?;
                let n: i32 = n.parse().map_err(|_| format!("bad field {f:?}"))?;
                let o = s.objectives.get(&n).copied().unwrap_or_default();
                match which {
                    "displayed" => json!(o.displayed),
                    "completed" => json!(o.completed),
                    _ => return Err(format!("bad field {f:?}")),
                }
            } else if let Some(v) = f.strip_prefix("var.") {
                let (key, i) = quest_var(g, q, v)?;
                json!(g.var(key, i).num())
            } else {
                return Err(format!("unknown quest field {f:?}"));
            }
        }
    })
}

/// The literal text the capture writes for a number (layer5-captures.md §4.1): a whole
/// number without a decimal point, anything else with one.
fn literal_text(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e18 {
        format!("{}", v.abs() as u64)
    } else {
        let s = format!("{}", v.abs());
        if s.contains('.') {
            s
        } else {
            format!("{s}.0")
        }
    }
}

/// `set x to <literal>` into a fresh local of the given type, read back (the compiler
/// writes a negative literal as the positive one and a unary minus, scripts.md §2.3).
pub fn set_local(ty: &str, value: f64) -> Result<f64, String> {
    let text = literal_text(value);
    let mut v = literal(&text, value.abs());
    if value < 0.0 {
        v = -v;
    }
    match ty {
        "short" | "long" | "int" => Ok(to_int_storage(v) as f64),
        "float" | "ref" => Ok(v),
        _ => Err(format!("unknown variable type {ty:?}")),
    }
}

fn label(l: &Option<String>, i: usize) -> String {
    l.clone().unwrap_or_else(|| format!("step{i}"))
}

/// Call a script function as a script would (`on.Func args`).
fn call(g: &mut Game, opcode: u16, on: Option<FormId>, args: Vec<ArgValue>) -> Value {
    let ctx = Ctx {
        unit: "runner".into(),
        opcode,
        on,
        self_ref: None,
        action_ref: None,
        quest: None,
        inst: None,
        in_condition: false,
    };
    g.call_function(&ctx, &args)
}

fn form_arg(f: FormId) -> ArgValue {
    ArgValue::Value(Value::Ref(f.0))
}
fn num_arg(x: f64) -> ArgValue {
    ArgValue::Value(Value::Num(x))
}

fn apply_given(g: &mut Game, gv: &Given) -> Result<(), String> {
    match gv {
        Given::Quest { quest: name, running, completed, stage, done, not_done, objectives, vars } => {
            let q = quest(g, name)?;
            for (k, v) in vars {
                set_quest_var(g, q, k, *v)?;
            }
            let s = g.quest(q);
            if let Some(r) = running {
                s.running = *r;
            }
            if let Some(c) = completed {
                s.completed = *c;
                if !c {
                    s.failed = false;
                }
            }
            for d in done {
                s.stages_done.insert(*d);
            }
            for d in not_done {
                s.stages_done.remove(d);
            }
            if let Some(st) = stage {
                if s.stage() != *st {
                    s.stage_override = Some(*st);
                }
            }
            for o in objectives {
                let e = s.objectives.entry(o.index).or_default();
                if let Some(d) = o.displayed {
                    e.displayed = d;
                }
                if let Some(c) = o.completed {
                    e.completed = c;
                }
            }
        }
        Given::Global { name, value } => {
            let id = resolve(g, name)?;
            g.world.set_global(id, *value).map_err(|e| e.to_string())?;
        }
        Given::ItemCount { of, item, count } => {
            let (of, item) = (resolve(g, of)?, resolve(g, item)?);
            let have = g.world.item_count(of, item).map_err(|e| e.to_string())?;
            if have < *count {
                g.world.add_item(of, item, count - have).map_err(|e| e.to_string())?;
            } else if have > *count {
                g.world.remove_item(of, item, have - count).map_err(|e| e.to_string())?;
            }
        }
        Given::Disabled { of, disabled } => {
            let r = resolve(g, of)?;
            g.world.set_enabled(r, !disabled).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Run one action; returns its observation fields (for labelled actions).
fn step(g: &mut Game, a: &Action) -> Result<Option<(String, Map<String, Json>)>, String> {
    let mut obs = Map::new();
    let mut lab: Option<String> = None;
    match a {
        Action::SetStage { quest: q, stage } => {
            let q = quest(g, q)?;
            g.set_stage(q, *stage);
        }
        Action::SetStageRead { quest: q, stage, field: f, label: l } => {
            let q = quest(g, q)?;
            g.set_stage(q, *stage);
            obs.insert(f.clone(), field(g, q, f)?);
            lab = Some(l.clone());
        }
        Action::StartQuest { quest: q } => {
            let q = quest(g, q)?;
            g.start_quest(q);
        }
        Action::StopQuest { quest: q } => {
            let q = quest(g, q)?;
            g.stop_quest(q);
        }
        Action::SetQuestVar { quest: q, var, value } => {
            let q = quest(g, q)?;
            set_quest_var(g, q, var, *value)?;
        }
        Action::SetLocal { ty, value, label: l } => {
            obs.insert("value".into(), json!(set_local(ty, *value)?));
            lab = l.clone();
        }
        Action::SetObjective { quest: q, index, displayed, completed } => {
            let q = quest(g, q)?;
            g.set_objective(q, *index, *displayed, *completed);
        }
        Action::GetQuest { .. } | Action::SampleVar { .. } => unreachable!("handled by the caller"),
        Action::WaitFrames { .. } | Action::AdvanceSeconds { .. } => {}
        Action::AddItem { to, item, count } => {
            let (to, item) = (resolve(g, to)?, resolve(g, item)?);
            call(g, 0x1002, Some(to), vec![form_arg(item), num_arg(*count as f64)]);
        }
        Action::RemoveItem { from, item, count } => {
            let (from, item) = (resolve(g, from)?, resolve(g, item)?);
            call(g, 0x1052, Some(from), vec![form_arg(item), num_arg(*count as f64)]);
        }
        Action::RemoveAllItems { from, to } => {
            let from = resolve(g, from)?;
            let args = match to {
                Some(t) => vec![form_arg(resolve(g, t)?)],
                None => vec![],
            };
            call(g, 0x10AD, Some(from), args);
        }
        Action::Enable { target } => {
            let t = resolve(g, target)?;
            call(g, 0x1021, Some(t), vec![]);
        }
        Action::Disable { target } => {
            let t = resolve(g, target)?;
            call(g, 0x1022, Some(t), vec![]);
        }
        Action::SetGlobal { name, value } => {
            let id = resolve(g, name)?;
            g.world.set_global(id, *value).map_err(|e| e.to_string())?;
            g.emit(crate::trace::EventKind::GlobalSet { global: id, value: *value });
        }
        Action::Activate { actor, target, label: l } => {
            // `target.Activate actor`: the default action, bypassing OnActivate
            // (how the capture issues it; GECK wiki Activate).
            let actor = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let t = resolve(g, target)?;
            let v = g.activate(actor, t, ActMode::Default)?;
            if l.is_some() {
                obs.insert("opened".into(), json!(v["result"] == "opened_container"));
                lab = l.clone();
            }
        }
        Action::GetItemCount { of, item, label: l } => {
            let (of, item) = (resolve(g, of)?, resolve(g, item)?);
            obs.insert("count".into(), json!(g.world.item_count(of, item).map_err(|e| e.to_string())?));
            lab = l.clone();
        }
        Action::GetDisabled { of, label: l } => {
            let r = resolve(g, of)?;
            obs.insert("disabled".into(), json!(!g.world.is_enabled(r).map_err(|e| e.to_string())?));
            lab = l.clone();
        }
        Action::GetGlobal { name, label: l } => {
            let id = resolve(g, name)?;
            obs.insert("value".into(), json!(g.world.global(id).map_err(|e| e.to_string())? as f64));
            lab = l.clone();
        }
        Action::PlayerActivate { target, actor } => {
            let actor = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            let t = resolve(g, target)?;
            g.activate(actor, t, ActMode::Hand)?;
        }
        Action::SayInfo { info, speaker } => {
            let i = resolve(g, info)?;
            let s = match speaker {
                Some(s) => Some(resolve(g, s)?),
                None => None,
            };
            g.run_info(i, s)?;
        }
        Action::Package { actor, package, event: ev } => {
            let (a, p) = (resolve(g, actor)?, resolve(g, package)?);
            let which = match ev.as_str() {
                "start" => 0,
                "done" | "end" => 1,
                "change" => 2,
                _ => return Err(format!("package event {ev:?} is not start, done or change")),
            };
            g.package_event(a, p, which);
        }
        Action::SayToDone { actor, topic } => {
            let a = resolve(g, actor)?;
            let t = match topic {
                Some(t) => Some(Value::Ref(resolve(g, t)?.0)),
                None => None,
            };
            g.ref_event(a, event::SAY_TO_DONE, t, None);
        }
        Action::TriggerEnter { trigger, actor } => {
            let t = resolve(g, trigger)?;
            let a = match actor {
                Some(a) => resolve(g, a)?,
                None => FormId::PLAYER_REF,
            };
            g.ref_event(t, event::ON_TRIGGER_ENTER, Some(Value::Ref(a.0)), Some(a));
        }
        Action::PressButton { button } => match &mut g.message_box {
            Some(m) => m.pressed = Some(*button),
            None => return Err("no message box is open".into()),
        },
        Action::Menu { menu } => g.menu = *menu,
        Action::MovePlayer { to } => {
            let t = resolve(g, to)?;
            call(g, 0x109E, Some(FormId::PLAYER_REF), vec![form_arg(t)]);
        }
        Action::LoadCell { cell } => {
            let c = resolve(g, cell)?;
            let mut cells = g.loaded_cells.clone();
            cells.extend(g.cells_around(c));
            g.set_loaded(cells);
        }
        Action::GetVar { of, var, label: l } => {
            let f = resolve(g, of)?;
            let (key, _, i) = var_of(g, f, var)?;
            obs.insert("value".into(), json!(g.var(key, i).num()));
            lab = l.clone();
        }
        Action::GetRef { of, label: l } => {
            let r = resolve(g, of)?;
            let enabled = g.world.is_enabled(r).map_err(|e| e.to_string())?;
            let s = g.world.get(r).map_err(|e| e.to_string())?.clone();
            obs.insert("disabled".into(), json!(!enabled));
            obs.insert("cell".into(), json!(s.cell));
            obs.insert("pos".into(), json!(s.pos));
            let x = g.extra.get(&r).cloned().unwrap_or_default();
            obs.insert("dead".into(), json!(x.dead));
            obs.insert("destroyed".into(), json!(x.destroyed));
            lab = l.clone();
        }
        Action::Kill { actor, killer } => {
            let a = resolve(g, actor)?;
            let args = match killer {
                Some(k) => vec![form_arg(resolve(g, k)?)],
                None => vec![],
            };
            call(g, 0x108B, Some(a), args);
        }
    }
    Ok(lab.map(|l| (l, obs)))
}

fn quests_json(g: &Game) -> Json {
    let mut out = Map::new();
    for (q, s) in &g.quests {
        let script = g.data.quests.get(q).and_then(|d| d.script);
        let vars: Map<String, Json> = g
            .instances
            .get(&InstKey::Quest { quest: *q })
            .map(|i| {
                i.vars
                    .iter()
                    .map(|(k, v)| (g.var_name(script, *k).unwrap_or_else(|| format!("#{k}")), json!(v.num())))
                    .collect()
            })
            .unwrap_or_default();
        let objectives: Map<String, Json> =
            s.objectives.iter().map(|(k, o)| (k.to_string(), json!({"displayed": o.displayed, "completed": o.completed}))).collect();
        out.insert(
            q.to_string(),
            json!({"edid": g.data.edid_of.get(q), "running": s.running, "completed": s.completed, "failed": s.failed,
                   "stage": s.stage(), "stages_done": s.stages_done, "objectives": objectives, "vars": vars}),
        );
    }
    Json::Object(out)
}

/// Run an input document against a fresh world and game state.
pub fn run(data: &GameData, forms: &FormStore, refs: &RefTable, input: &Input) -> Result<Json, String> {
    if input.format != ACTIONS_FORMAT {
        return Err(format!("unsupported actions format {:?} (expected {ACTIONS_FORMAT})", input.format));
    }
    if !(input.frame_seconds > 0.0) {
        return Err("frame_seconds must be positive".into());
    }
    let world = World::new(
        forms,
        refs,
        SimConfig { seed: input.seed, player_level: input.player_level, hardcore: input.hardcore, ..SimConfig::default() },
    );
    let cfg = GameConfig { frame_seconds: input.frame_seconds, trace_locals: input.trace_locals, seed: input.seed, ..GameConfig::default() };
    let mut g = Game::new(data, world, cfg);
    if input.new_game {
        g.new_game();
    }
    let mut given_errors = Vec::new();
    for (i, gv) in input.given.iter().enumerate() {
        if let Err(e) = apply_given(&mut g, gv) {
            given_errors.push(json!({"given": i, "error": e}));
        }
    }
    let mut observations = Vec::new();
    let mut steps = Vec::new();
    for (i, a) in input.actions.iter().enumerate() {
        let start_frame = g.frame;
        let res = match a {
            Action::GetQuest { quest: q, fields, label: l } => match quest(&g, q) {
                Ok(q) => {
                    let mut o = json!({"label": label(l, i), "step": i});
                    let mut errors = Map::new();
                    for f in fields {
                        match field(&g, q, f) {
                            Ok(v) => o[f] = v,
                            Err(e) => {
                                errors.insert(f.clone(), json!(e));
                            }
                        }
                        g.tick();
                    }
                    if !errors.is_empty() {
                        o["errors"] = Json::Object(errors);
                    }
                    observations.push(o);
                    Ok(())
                }
                Err(e) => Err(e),
            },
            Action::SampleVar { quest: q, var, frames, runs, skip, label: l } => match quest(&g, q).and_then(|q| quest_var(&g, q, var)) {
                Ok((key, idx)) => {
                    let mut prev = g.var(key, idx).num();
                    let (mut changes, mut drops) = (0u64, Vec::new());
                    for _ in 0..*frames {
                        g.tick();
                        let v = g.var(key, idx).num();
                        if v != prev {
                            changes += 1;
                            drops.push(prev - v);
                            prev = v;
                        }
                    }
                    let mut o = json!({"label": label(l, i), "step": i});
                    match runs {
                        Some(n) => o["drops"] = json!(drops.iter().skip(*skip).take(*n).collect::<Vec<_>>()),
                        None => o["changes"] = json!(changes),
                    }
                    observations.push(o);
                    Ok(())
                }
                Err(e) => Err(e),
            },
            _ => match step(&mut g, a) {
                Ok(Some((l, fields))) => {
                    let mut o = json!({"label": l, "step": i});
                    for (k, v) in fields {
                        o[k] = v;
                    }
                    observations.push(o);
                    Ok(())
                }
                Ok(None) => Ok(()),
                Err(e) => {
                    // A labelled action that fails still answers, with its error.
                    if let Some(l) = action_label(a) {
                        observations.push(json!({"label": l, "step": i, "errors": {"action": e.clone()}}));
                    }
                    Err(e)
                }
            },
        };
        let wait = match a {
            Action::WaitFrames { frames } => *frames,
            Action::AdvanceSeconds { seconds } => (seconds / input.frame_seconds).round().max(0.0) as u64,
            _ => frames_after(a),
        };
        g.frames(wait);
        let mut s = json!({"step": i, "frame": start_frame});
        if let Err(e) = res {
            s["error"] = json!(e);
        }
        steps.push(s);
    }
    let mut report = Map::new();
    for n in &input.report {
        let r = resolve(&g, n)?;
        let st = g.world.get(r).map_err(|e| e.to_string())?.clone();
        let mut v = serde_json::to_value(&st).map_err(|e| e.to_string())?;
        if let Some(x) = g.extra.get(&r) {
            v["layer5"] = serde_json::to_value(x).map_err(|e| e.to_string())?;
        }
        report.insert(r.to_string(), v);
    }
    let name = |op: &u16| g.data.table.get(*op).map(|f| f.name.clone()).unwrap_or_default();
    let unhandled: Map<String, Json> =
        g.unhandled.iter().map(|(op, n)| (format!("0x{op:04X}"), json!({"name": name(op), "calls": n}))).collect();
    let stubbed: Map<String, Json> =
        g.stubbed.iter().map(|(op, n)| (format!("0x{op:04X}"), json!({"name": name(op), "calls": n}))).collect();
    Ok(json!({
        "format": TRACE_FORMAT,
        "seed": input.seed,
        "frame_seconds": input.frame_seconds,
        "title": input.title,
        "observations": observations,
        "steps": steps,
        "given_errors": given_errors,
        "events": g.events,
        "final": {
            "frames": g.frame,
            "quests": quests_json(&g),
            "player": g.player,
            "references": report,
            "unhandled": unhandled,
            "stubbed": stubbed,
        },
    }))
}

fn action_label(a: &Action) -> Option<String> {
    match a {
        Action::SetStageRead { label, .. } => Some(label.clone()),
        Action::SetLocal { label, .. }
        | Action::Activate { label, .. }
        | Action::GetItemCount { label, .. }
        | Action::GetDisabled { label, .. }
        | Action::GetGlobal { label, .. }
        | Action::GetVar { label, .. }
        | Action::GetRef { label, .. } => label.clone(),
        _ => None,
    }
}
