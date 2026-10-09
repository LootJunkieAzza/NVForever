//! The Layer 5 game state over the Layer 4 world: script instances and their variables,
//! quests, event dispatch, conditions, and the [`fnv_script::vm::Host`] that connects the
//! interpreter to all of it. Behaviour notes with evidence: docs/spec-notes/scripts.md
//! §4–7.

use crate::cond::{self, Condition, Operand, RunOn};
use crate::data::{Compiled, GameData, UnitId};
use crate::functions::{self, Args, Ctx, Kind};
use crate::quest::{QuestState, STAGE_COMPLETE, STAGE_FAIL};
use crate::trace::{Event, EventKind};
use fnv_script::ir::{Arg, Block, RefIndex, Stmt, VarRef, VarSlot};
use fnv_script::unit::{RefSlot, VarType};
use fnv_script::vm::{to_int_storage, ArgValue, Host, RunOpts, Value, Vm, VmError};
use fnv_sim::world::{Outcome, Snapshot};
use fnv_sim::{FormId, World};
use rand_chacha::ChaCha8Rng;
use rand_core::SeedableRng;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Begin-block event codes (docs/spec-notes/script-functions.md §3.3 [observed]).
pub mod event {
    pub const GAME_MODE: u16 = 0x00;
    pub const MENU_MODE: u16 = 0x01;
    pub const ON_ACTIVATE: u16 = 0x02;
    pub const ON_ADD: u16 = 0x03;
    pub const ON_EQUIP: u16 = 0x04;
    pub const ON_UNEQUIP: u16 = 0x05;
    pub const ON_DROP: u16 = 0x06;
    pub const SAY_TO_DONE: u16 = 0x07;
    pub const ON_DEATH: u16 = 0x0A;
    pub const ON_PACKAGE_START: u16 = 0x0F;
    pub const ON_PACKAGE_DONE: u16 = 0x10;
    pub const ON_PACKAGE_CHANGE: u16 = 0x14;
    pub const ON_LOAD: u16 = 0x15;
    pub const ON_TRIGGER_ENTER: u16 = 0x1A;
    pub const ON_TRIGGER_LEAVE: u16 = 0x1B;
}

/// Nested script runs allowed (a stage result script setting another stage, an
/// `Activate` running an OnActivate block) [documented: GECK wiki SetStage, at least 250
/// recursive SetStage calls work].
pub const MAX_DEPTH: u32 = 256;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct GameConfig {
    /// Fixed frame step in seconds (deterministic traces).
    pub frame_seconds: f64,
    /// Unknown opcodes and undecodable pieces stop a script (tests) instead of being
    /// logged and skipped.
    pub strict: bool,
    /// Trace writes to an instance's own locals (off by default: timers write every
    /// frame). Writes to another script's variables and to globals are always traced.
    pub trace_locals: bool,
    pub seed: u64,
    pub max_steps: u64,
}

impl Default for GameConfig {
    fn default() -> Self {
        GameConfig { frame_seconds: 1.0 / 60.0, strict: false, trace_locals: false, seed: 0, max_steps: 100_000 }
    }
}

/// Whose variables a script instance holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum InstKey {
    /// A placed or created reference whose base form has a script.
    Ref { reference: FormId },
    /// A quest's script.
    Quest { quest: FormId },
    /// A scripted item in an inventory (one instance per holder and item).
    Item { holder: FormId, item: FormId },
}

impl InstKey {
    pub fn label(&self) -> String {
        match self {
            InstKey::Ref { reference } => format!("ref:{reference}"),
            InstKey::Quest { quest } => format!("quest:{quest}"),
            InstKey::Item { holder, item } => format!("item:{item}@{holder}"),
        }
    }
}

/// A script instance: the variables of one running copy of a script.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Instance {
    pub script: Option<FormId>,
    pub vars: BTreeMap<u16, Value>,
    /// Game time of the last `GetSecondsPassed` call, and the frame and value of it
    /// (repeated calls in one frame give the same value [documented: GECK wiki
    /// GetSecondsPassed]).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gsp_last: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub gsp_frame: Option<(u64, f64)>,
}

/// Per-reference state the Layer 4 world does not keep.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct RefExtra {
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub dead: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub destroyed: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub ghost: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub restrained: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub unconscious: bool,
    #[serde(skip_serializing_if = "std::ops::Not::not", default)]
    pub marked_for_delete: bool,
    /// Faction rank changes: a rank, or −1 for "removed".
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub factions: BTreeMap<FormId, i8>,
    /// Actor values other than S.P.E.C.I.A.L. (those live in the world), by code.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub actor_values: BTreeMap<u16, f64>,
    /// Packages added with AddScriptPackage (the last one is current).
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub packages: Vec<FormId>,
}

/// The player's state outside the world's reference table.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlayerState {
    pub xp: f64,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub perks: BTreeMap<FormId, u8>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub notes: BTreeSet<FormId>,
    /// Fame and infamy per reputation.
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub reputation: BTreeMap<FormId, [f64; 2]>,
    /// `SexChange`: true = female.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub female: Option<bool>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub tag_skills: BTreeSet<u16>,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub achievements: BTreeSet<i32>,
    /// Actors killed, by base form (GetDeadCount).
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub dead_count: BTreeMap<FormId, u32>,
    /// Disabled control groups: movement, Pip-Boy, fighting, POV, looking, rollover
    /// text, sneaking.
    #[serde(default)]
    pub controls_disabled: [bool; 7],
}

/// A message box waiting for a button (`ShowMessage` of a message with buttons).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MessageBox {
    pub message: FormId,
    pub buttons: usize,
    /// The button chosen (runner input), returned once by `GetButtonPressed`.
    pub pressed: Option<i32>,
}

pub struct Game<'a> {
    pub data: &'a GameData,
    pub world: World<'a>,
    pub cfg: GameConfig,
    pub quests: BTreeMap<FormId, QuestState>,
    pub instances: BTreeMap<InstKey, Instance>,
    pub extra: BTreeMap<FormId, RefExtra>,
    pub player: PlayerState,
    /// Forms added to form lists at run time.
    pub form_lists: BTreeMap<FormId, Vec<FormId>>,
    pub loaded_cells: BTreeSet<FormId>,
    /// Scripted references a script moved (they may have left their authored cell).
    pub moved: BTreeSet<FormId>,
    /// The open menu (MenuMode), by menu code.
    pub menu: Option<u16>,
    pub message_box: Option<MessageBox>,
    pub frame: u64,
    pub time: f64,
    pub events: Vec<Event>,
    /// Calls per opcode that have no implementation.
    pub unhandled: BTreeMap<u16, u64>,
    /// Calls per opcode that reached a Layer 6 stub.
    pub stubbed: BTreeMap<u16, u64>,
    unhandled_seen: BTreeSet<(u16, String)>,
    stubs_seen: BTreeSet<(u16, String, String)>,
    pub rng: ChaCha8Rng,
    depth: u32,
}

/// Snapshot of the Layer 5 state plus the world's (`fnv.game.state/1`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub format: String,
    pub sim: Snapshot,
    pub quests: BTreeMap<FormId, QuestState>,
    pub instances: Vec<(InstKey, Instance)>,
    pub extra: BTreeMap<FormId, RefExtra>,
    pub player: PlayerState,
    pub form_lists: BTreeMap<FormId, Vec<FormId>>,
    pub loaded_cells: BTreeSet<FormId>,
    pub moved: BTreeSet<FormId>,
    pub menu: Option<u16>,
    pub message_box: Option<MessageBox>,
    pub frame: u64,
    pub time: f64,
    pub rng_word_pos: String,
}

pub const SNAPSHOT_FORMAT: &str = "fnv.game.state/1";

/// How an activation runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActMode {
    /// The player (or an actor) activates by hand: a script's OnActivate block runs
    /// instead of the default action; the default action runs only when the block calls
    /// `Activate` [documented: GECK wiki Activate; layer5.md §3.4].
    Hand,
    /// `Activate` with flag 0 (or none): the default action, bypassing any OnActivate
    /// block [documented: GECK wiki Activate].
    Default,
    /// `Activate actor 1`: the OnActivate block instead of the default action; nothing
    /// when there is no block [documented: GECK wiki Activate].
    Block,
}

/// Where a running script keeps its locals.
enum Locals {
    Instance(InstKey),
    /// A result script (stage, dialogue, package): locals live for one run.
    Temp(BTreeMap<u16, Value>),
}

/// What a script run is attached to.
#[derive(Debug, Clone, Copy, Default)]
pub struct RunCtx {
    pub self_ref: Option<FormId>,
    pub action_ref: Option<FormId>,
    pub quest: Option<FormId>,
}

impl<'a> Game<'a> {
    pub fn new(data: &'a GameData, world: World<'a>, cfg: GameConfig) -> Game<'a> {
        Game {
            data,
            world,
            cfg,
            quests: BTreeMap::new(),
            instances: BTreeMap::new(),
            extra: BTreeMap::new(),
            player: PlayerState::default(),
            form_lists: BTreeMap::new(),
            loaded_cells: BTreeSet::new(),
            moved: BTreeSet::new(),
            menu: None,
            message_box: None,
            frame: 0,
            time: 0.0,
            events: Vec::new(),
            unhandled: BTreeMap::new(),
            stubbed: BTreeMap::new(),
            unhandled_seen: BTreeSet::new(),
            stubs_seen: BTreeSet::new(),
            rng: ChaCha8Rng::seed_from_u64(cfg.seed ^ 0x5C21_7000),
            depth: 0,
        }
    }

    pub fn emit(&mut self, kind: EventKind) {
        self.events.push(Event { frame: self.frame, kind });
    }

    /// The new-game state: every quest flagged "Start game enabled" is running, with no
    /// stage set [hypothesis: the flag's documented meaning (xEdit); which quest the
    /// "New Game" menu starts is a separate input, scripts.md §8].
    pub fn new_game(&mut self) {
        let start: Vec<FormId> = self.data.quests.values().filter(|q| q.start_enabled()).map(|q| q.form).collect();
        for q in start {
            self.start_quest(q);
        }
    }

    // ---- quests ------------------------------------------------------------------

    pub fn quest(&mut self, q: FormId) -> &mut QuestState {
        self.quests.entry(q).or_default()
    }

    pub fn quest_state(&self, q: FormId) -> QuestState {
        self.quests.get(&q).cloned().unwrap_or_default()
    }

    pub fn start_quest(&mut self, q: FormId) -> bool {
        if !self.data.is_quest(q) {
            return false;
        }
        let s = self.quest(q);
        if !s.running {
            s.running = true;
            s.since_run = 0.0;
            self.emit(EventKind::QuestStarted { quest: q });
        }
        true
    }

    pub fn stop_quest(&mut self, q: FormId) -> bool {
        if !self.data.is_quest(q) {
            return false;
        }
        let s = self.quest(q);
        if s.running {
            s.running = false;
            self.emit(EventKind::QuestStopped { quest: q });
        }
        true
    }

    /// `SetStage quest stage` [documented: GECK wiki SetStage; observed: the 5C captures
    /// named per rule]:
    /// - a stage the quest does not define does nothing [observed: VMS16 stage 15];
    /// - a stage already done does nothing unless the quest allows repeated stages
    ///   [observed: VMS16 stage 10 and VCG03 stage 10 set again did not rerun their
    ///   scripts; Generic, with the flag, did];
    /// - the quest starts if it is not running [observed: VMS16];
    /// - the stage is marked done, then every entry whose conditions pass runs its result
    ///   script immediately, nested in the caller [documented: wiki, "resolves any of the
    ///   attached stage items that pass their conditions"], then applies its Complete /
    ///   Fail flag [hypothesis on the order]. Completing does not stop the quest
    ///   [observed: VMS22 ran on after its complete stage].
    pub fn set_stage(&mut self, q: FormId, stage: i32) -> bool {
        let data = self.data;
        let Some(def) = data.quests.get(&q) else { return false };
        let Some(entries) = def.stages.get(&stage) else {
            self.emit(EventKind::StageIgnored { quest: q, stage, reason: "undefined_stage" });
            return false;
        };
        if self.quest(q).stages_done.contains(&stage) && !def.repeated_stages() {
            self.emit(EventKind::StageIgnored { quest: q, stage, reason: "already_done" });
            return false;
        }
        self.start_quest(q);
        self.quest(q).stages_done.insert(stage);
        let player = Some(FormId::PLAYER_REF);
        let label = format!("QUST {q} stage {stage}");
        let mut ran = Vec::new();
        for (i, e) in entries.iter().enumerate() {
            // Stage conditions run on the player [hypothesis].
            if self.conditions_pass(&e.conditions, player, player, &label) {
                ran.push(i);
            }
        }
        self.emit(EventKind::StageSet { quest: q, stage, entries: ran.clone() });
        for i in ran {
            let e = &entries[i];
            if let Some(u) = e.unit {
                self.run_result(u, RunCtx { quest: Some(q), ..Default::default() });
            }
            if e.flags & STAGE_COMPLETE != 0 {
                let s = self.quest(q);
                if !s.completed {
                    s.completed = true;
                    self.emit(EventKind::QuestCompleted { quest: q });
                }
            }
            if e.flags & STAGE_FAIL != 0 {
                // A Fail Quest stage also reads as completed (GetQuestCompleted 1)
                // [observed: VMS16's fail stage 110, `L5.quest.stage_order`].
                let s = self.quest(q);
                let first = !s.failed;
                s.failed = true;
                s.completed = true;
                if first {
                    self.emit(EventKind::QuestFailed { quest: q });
                }
            }
        }
        true
    }

    /// `SetObjectiveDisplayed` / `SetObjectiveCompleted`. Only objectives the quest
    /// defines change; displaying one starts a stopped quest [documented: GECK wiki
    /// SetObjectiveDisplayed]. Completing does not display; displaying clears the
    /// completed flag [observed: VMS16 objectives 10 and 5, layer5-captures.md §5.2].
    pub fn set_objective(&mut self, q: FormId, index: i32, displayed: Option<bool>, completed: Option<bool>) -> bool {
        let data = self.data;
        let Some(def) = data.quests.get(&q) else { return false };
        if !def.objectives.contains_key(&index) {
            return false;
        }
        if displayed == Some(true) {
            self.start_quest(q);
        }
        let o = self.quest(q).objectives.entry(index).or_default();
        let (d0, c0) = (o.displayed, o.completed);
        if let Some(d) = displayed {
            o.displayed = d;
            if d {
                o.completed = false;
            }
        }
        if let Some(c) = completed {
            o.completed = c;
        }
        let (d1, c1) = (o.displayed, o.completed);
        if displayed.is_some() && d0 != d1 {
            self.emit(EventKind::ObjectiveDisplayed { quest: q, index, on: d1 });
        }
        if c0 != c1 {
            self.emit(EventKind::ObjectiveCompleted { quest: q, index, on: c1 });
        }
        true
    }

    // ---- instances and variables -------------------------------------------------

    /// The script a reference runs (its base form's), if any.
    pub fn ref_script(&self, r: FormId) -> Option<FormId> {
        let base = match self.world.peek(r) {
            Some(s) => s.base,
            None => self.world.placed.get(r).map(|p| p.base).or((r == FormId::PLAYER_REF).then_some(FormId::PLAYER_BASE))?,
        };
        self.data.script_of_base(base)
    }

    /// The instance that `form.var` addresses: a quest's script or a reference's.
    pub fn instance_of(&self, form: FormId) -> Option<(InstKey, FormId)> {
        if let Some(q) = self.data.quests.get(&form) {
            let s = q.script.filter(|s| self.data.scripts.contains_key(s))?;
            return Some((InstKey::Quest { quest: form }, s));
        }
        Some((InstKey::Ref { reference: form }, self.ref_script(form)?))
    }

    fn instance(&mut self, key: InstKey, script: Option<FormId>) -> &mut Instance {
        self.instances.entry(key).or_insert_with(|| Instance { script, ..Default::default() })
    }

    /// Read a variable of an instance (0 when never written).
    pub fn var(&self, key: InstKey, index: u16) -> Value {
        self.instances.get(&key).and_then(|i| i.vars.get(&index)).copied().unwrap_or_default()
    }

    /// Store into a variable by its declared type: integer variables truncate (see
    /// [`to_int_storage`]), float/ref variables keep the value.
    pub fn store(&mut self, key: InstKey, script: Option<FormId>, index: u16, v: Value, unit: &str, letter_int: bool) {
        let data = self.data;
        let decl = script.and_then(|s| data.script(s)).and_then(|c| c.var_type(index).cloned());
        if script.is_some() && decl.is_none() {
            self.emit(EventKind::UndeclaredVariable { owner: key.label(), index, unit: unit.to_string() });
        }
        let int = match decl {
            Some(VarType::Int) => true,
            Some(_) => false,
            None => letter_int,
        };
        let v = if int { Value::Num(to_int_storage(v.num()) as f64) } else { v };
        self.instance(key, script).vars.insert(index, v);
    }

    pub fn var_name(&self, script: Option<FormId>, index: u16) -> Option<String> {
        script.and_then(|s| self.data.script(s)).and_then(|c| c.var_name(index)).map(str::to_string)
    }

    // ---- running scripts ---------------------------------------------------------

    fn opts(&self) -> RunOpts {
        RunOpts { strict: self.cfg.strict, max_steps: self.cfg.max_steps }
    }

    fn exec(&mut self, unit: &'a Compiled, stmts: &'a [Stmt], locals: Locals, ctx: RunCtx) {
        if self.depth >= MAX_DEPTH {
            self.emit(EventKind::ScriptError { unit: unit.key(), error: format!("nesting deeper than {MAX_DEPTH}") });
            return;
        }
        self.depth += 1;
        let opts = self.opts();
        let data = self.data;
        let table = &data.table;
        let mut ex = Exec { game: self, unit, locals, ctx };
        let r = Vm::new(table, &mut ex, opts).run(stmts);
        self.depth -= 1;
        if let Err(e) = r {
            self.emit(EventKind::ScriptError { unit: unit.key(), error: e.to_string() });
        }
    }

    /// Run a result script (quest stage, dialogue, package, terminal): the top-level
    /// statements, with temporary locals.
    pub fn run_result(&mut self, unit: UnitId, ctx: RunCtx) {
        let c = self.data.unit(unit);
        self.exec(c, &c.program.top, Locals::Temp(BTreeMap::new()), ctx);
    }

    /// Whether a block's parameter accepts `subject` (a block without parameters accepts
    /// everything; `Begin OnActivate player` only the player; `Begin MenuMode 1036` only
    /// that menu).
    fn block_accepts(&mut self, unit: &'a Compiled, key: InstKey, block: &Block, subject: Option<Value>) -> bool {
        let Some(params) = block.params.as_ref().filter(|p| !p.is_empty()) else { return true };
        let Some(subject) = subject else { return false };
        let want = match &params[0] {
            Arg::Int(i) => Value::Num(*i as f64),
            Arg::Float(f) => Value::Num(*f),
            Arg::Enum(c) => Value::Num(*c as f64),
            Arg::Ref(r) => {
                let mut ex = Exec { game: self, unit, locals: Locals::Instance(key), ctx: RunCtx::default() };
                match ex.ref_value(*r) {
                    Ok(v) => v,
                    Err(_) => return false,
                }
            }
            _ => return true,
        };
        match (want.as_form(), subject.as_form()) {
            (Some(a), Some(b)) => a == b,
            _ => want.num() == subject.num(),
        }
    }

    /// Run every block of `event` in the instance's script that accepts `subject`.
    /// Returns whether a block ran.
    pub fn run_event(&mut self, key: InstKey, script: FormId, event: u16, subject: Option<Value>, ctx: RunCtx) -> bool {
        let data = self.data;
        let Some(unit) = data.script(script) else { return false };
        let mut ran = false;
        for b in unit.program.blocks.iter().filter(|b| b.event == event) {
            if !self.block_accepts(unit, key, b, subject) {
                continue;
            }
            self.instance(key, Some(script));
            ran = true;
            self.exec(unit, &b.body, Locals::Instance(key), ctx);
        }
        ran
    }

    /// Run a non-frame event on a reference's script, with a trace line.
    pub fn ref_event(&mut self, r: FormId, event: u16, subject: Option<Value>, action_ref: Option<FormId>) -> bool {
        let Some(script) = self.ref_script(r) else { return false };
        let has = self.data.script(script).is_some_and(|u| u.has_block(event));
        if !has {
            return false;
        }
        let ran = self.run_event(
            InstKey::Ref { reference: r },
            script,
            event,
            subject,
            RunCtx { self_ref: Some(r), action_ref, quest: None },
        );
        if ran {
            let block = self.data.table.block_name(event).unwrap_or("?").to_string();
            let unit = self.data.script(script).map(|u| u.key()).unwrap_or_default();
            self.emit(EventKind::BlockRun { unit, block, on: Some(r) });
        }
        ran
    }

    // ---- frames ------------------------------------------------------------------

    /// One frame: quest scripts whose delay has passed, then the object scripts of
    /// enabled references in loaded cells (GameMode, or MenuMode while a menu is open)
    /// [hypothesis: quests before references, each in FormID order; observed: quest
    /// delays, see `QuestDef::script_delay`].
    pub fn tick(&mut self) {
        let dt = self.cfg.frame_seconds;
        self.frame += 1;
        self.time += dt;
        let ev = if self.menu.is_some() { event::MENU_MODE } else { event::GAME_MODE };
        let subject = self.menu.map(|m| Value::Num(m as f64));
        let data = self.data;
        let due: Vec<(FormId, FormId)> = self
            .quests
            .iter_mut()
            .filter(|(_, s)| s.running)
            .filter_map(|(q, s)| {
                let def = data.quests.get(q)?;
                let script = def.script.filter(|s| data.scripts.contains_key(s))?;
                s.since_run += dt;
                let delay = s.delay.unwrap_or_else(|| def.script_delay());
                if s.since_run + 1e-9 >= delay {
                    s.since_run = 0.0;
                    Some((*q, script))
                } else {
                    None
                }
            })
            .collect();
        for (q, script) in due {
            // A quest stopped by an earlier script this frame does not run.
            if self.quests.get(&q).is_some_and(|s| s.running) {
                self.run_event(InstKey::Quest { quest: q }, script, ev, subject, RunCtx { quest: Some(q), ..Default::default() });
            }
        }
        for r in self.active_refs() {
            let Some(script) = self.ref_script(r) else { continue };
            if self.data.script(script).is_some_and(|u| u.has_block(ev)) {
                self.run_event(InstKey::Ref { reference: r }, script, ev, subject, RunCtx { self_ref: Some(r), ..Default::default() });
            }
        }
    }

    pub fn frames(&mut self, n: u64) {
        for _ in 0..n {
            self.tick();
        }
    }

    /// The current cell of a reference.
    pub fn cell_of(&self, r: FormId) -> Option<FormId> {
        match self.world.peek(r) {
            Some(s) => s.cell,
            None => self.world.placed.get(r).and_then(|p| p.cell),
        }
    }

    /// Whether a reference is in the world and enabled (enable parents followed).
    pub fn present(&self, r: FormId) -> bool {
        self.world.peek(r).is_none_or(|s| !s.deleted) && self.world.is_enabled(r).unwrap_or(false)
    }

    /// Scripted references that run per-frame blocks now: enabled, in a loaded cell
    /// [hypothesis: disabled references do not run their scripts].
    fn active_refs(&self) -> Vec<FormId> {
        let mut out: BTreeSet<FormId> = BTreeSet::new();
        for c in &self.loaded_cells {
            if let Some(v) = self.data.scripted_by_cell.get(c) {
                out.extend(v.iter().copied());
            }
        }
        out.extend(self.moved.iter().copied());
        out.into_iter()
            .filter(|&r| self.cell_of(r).is_some_and(|c| self.loaded_cells.contains(&c)) && self.present(r))
            .collect()
    }

    /// The cells loaded around `cell`: the cell itself for an interior; for an exterior,
    /// the 5×5 cells around it [hypothesis: the uGridsToLoad default of 5].
    pub fn cells_around(&self, cell: FormId) -> BTreeSet<FormId> {
        let mut out = BTreeSet::from([cell]);
        let placed = self.world.placed;
        let Some(world) = placed.world_of(cell) else { return out };
        let Some(grid) = placed.world_grid.get(&world) else { return out };
        let Some((&(x, y), _)) = grid.iter().find(|(_, &c)| c == cell) else { return out };
        for dx in -2..=2 {
            for dy in -2..=2 {
                if let Some(&c) = grid.get(&(x + dx, y + dy)) {
                    out.insert(c);
                }
            }
        }
        out
    }

    /// Make `cells` the loaded set; scripted references in newly loaded cells get
    /// OnLoad [hypothesis: OnLoad runs when the reference's cell is loaded].
    pub fn set_loaded(&mut self, cells: BTreeSet<FormId>) {
        let new: Vec<FormId> = cells.difference(&self.loaded_cells).copied().collect();
        self.loaded_cells = cells;
        for c in new {
            let refs = self.data.scripted_by_cell.get(&c).cloned().unwrap_or_default();
            for r in refs {
                if self.present(r) {
                    self.ref_event(r, event::ON_LOAD, None, None);
                }
            }
        }
    }

    /// The player is now in `cell`: load it (and its neighbours outdoors).
    pub fn player_entered(&mut self, cell: Option<FormId>) {
        if let Some(c) = cell {
            let cells = self.cells_around(c);
            self.set_loaded(cells);
        }
    }

    // ---- activation and other events -------------------------------------------

    /// `actor` activates `target`.
    pub fn activate(&mut self, actor: FormId, target: FormId, mode: ActMode) -> Result<serde_json::Value, String> {
        let has_block = self.ref_script(target).and_then(|s| self.data.script(s)).is_some_and(|u| u.has_block(event::ON_ACTIVATE));
        match mode {
            ActMode::Hand if has_block => {
                let ran = self.ref_event(target, event::ON_ACTIVATE, Some(Value::Ref(actor.0)), Some(actor));
                Ok(serde_json::json!({"result": "script", "ran": ran}))
            }
            ActMode::Block => {
                let ran = has_block && self.ref_event(target, event::ON_ACTIVATE, Some(Value::Ref(actor.0)), Some(actor));
                Ok(serde_json::json!({"result": "script", "ran": ran}))
            }
            ActMode::Hand | ActMode::Default => self.default_activation(actor, target, mode == ActMode::Hand),
        }
    }

    /// The default action of an activation (Layer 4), with its Layer 5 consequences:
    /// a picked-up item reaches the actor's inventory (OnAdd), a door moves the player.
    pub fn default_activation(&mut self, actor: FormId, target: FormId, by_hand: bool) -> Result<serde_json::Value, String> {
        let act = if by_hand { self.world.activate(actor, target) } else { self.world.activate_scripted(actor, target) }
            .map_err(|e| e.to_string())?;
        let v = serde_json::to_value(&act).map_err(|e| e.to_string())?;
        self.emit(EventKind::Activated { reference: target, by: actor, outcome: Some(v.clone()) });
        match act.outcome {
            Outcome::PickedUp { item, count, .. } => {
                self.emit(EventKind::ItemAdded { to: actor, item, count });
                self.item_event(actor, item, event::ON_ADD, Some(Value::Ref(actor.0)));
            }
            Outcome::Teleported { cell, .. } if actor == FormId::PLAYER_REF => self.player_entered(cell),
            _ => {}
        }
        Ok(v)
    }

    /// An inventory event on a scripted item (OnAdd, OnDrop, OnEquip, OnUnequip): the
    /// item's script runs in an instance per holder and item [hypothesis].
    pub fn item_event(&mut self, holder: FormId, item: FormId, ev: u16, subject: Option<Value>) -> bool {
        let Some(script) = self.data.script_of_base(item) else { return false };
        if !self.data.script(script).is_some_and(|u| u.has_block(ev)) {
            return false;
        }
        let key = InstKey::Item { holder, item };
        let ran = self.run_event(key, script, ev, subject, RunCtx { self_ref: None, action_ref: Some(holder), quest: None });
        if ran {
            let block = self.data.table.block_name(ev).unwrap_or("?").to_string();
            let unit = self.data.script(script).map(|u| u.key()).unwrap_or_default();
            self.emit(EventKind::BlockRun { unit, block, on: Some(holder) });
        }
        ran
    }

    /// A dialogue line was said (runner input standing in for Layer 6 dialogue): its
    /// begin and end result scripts run on the speaker.
    ///
    /// The line's conditions are evaluated first (subject the speaker, target the player)
    /// and a line whose conditions fail is refused unless `force` is set: in the game such
    /// a line would not be said. Returns whether the conditions passed.
    pub fn run_info(&mut self, info: FormId, speaker: Option<FormId>, force: bool) -> Result<bool, String> {
        let data = self.data;
        let def = data.infos.get(&info).ok_or_else(|| format!("{info} is not a dialogue line (INFO)"))?;
        let speaker = speaker.or(def.speaker);
        let pass = self.conditions_pass(&def.conditions, speaker, Some(FormId::PLAYER_REF), &format!("INFO {info}"));
        if !pass && !force {
            return Err(format!("the conditions of {info} do not pass (say_info with force to run it anyway)"));
        }
        for u in [def.begin, def.end].into_iter().flatten() {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: speaker });
            self.run_result(u, RunCtx { self_ref: speaker, action_ref: None, quest: def.quest });
        }
        Ok(pass)
    }

    /// A package event (runner input standing in for Layer 6 AI): the package's result
    /// script (0 begin, 1 end, 2 change) and the actor's OnPackageStart / OnPackageDone /
    /// OnPackageChange blocks.
    pub fn package_event(&mut self, actor: FormId, package: FormId, which: usize) {
        let data = self.data;
        if let Some(Some(u)) = data.packages.get(&package).map(|p| p[which]) {
            self.emit(EventKind::ResultScript { unit: data.unit(u).key(), on: Some(actor) });
            self.run_result(u, RunCtx { self_ref: Some(actor), ..Default::default() });
        }
        let ev = [event::ON_PACKAGE_START, event::ON_PACKAGE_DONE, event::ON_PACKAGE_CHANGE][which];
        self.ref_event(actor, ev, Some(Value::Ref(package.0)), None);
    }

    // ---- conditions --------------------------------------------------------------

    /// Evaluate a condition list (`subject` and `target` for the run-on choices).
    pub fn conditions_pass(&mut self, conds: &[Condition], subject: Option<FormId>, target: Option<FormId>, label: &str) -> bool {
        if conds.is_empty() {
            return true;
        }
        let mut results = Vec::with_capacity(conds.len());
        for c in conds {
            let on = match c.run_on {
                RunOn::Subject => subject,
                RunOn::Target => target,
                RunOn::Reference(r) => r.map(FormId),
                RunOn::LinkedReference => subject.and_then(|s| self.world.placed.get(s)).and_then(|p| p.linked),
                RunOn::CombatTarget | RunOn::Other(_) => None,
            };
            let v = self.condition_value(c, on, label);
            let rhs = match c.rhs {
                Operand::Value(x) => x,
                Operand::Global(Some(g)) => self.world.global(FormId(g)).map(|v| v as f64).unwrap_or(0.0),
                Operand::Global(None) => 0.0,
            };
            results.push((c.cmp.test(v, rhs), c.or));
        }
        cond::combine(&results)
    }

    fn condition_value(&mut self, c: &Condition, on: Option<FormId>, label: &str) -> f64 {
        let ctx = Ctx {
            unit: label.to_string(),
            opcode: c.opcode(),
            on,
            self_ref: on,
            action_ref: None,
            quest: None,
            inst: None,
            in_condition: true,
        };
        self.call_function(&ctx, &c.params).num()
    }

    /// Dispatch a function by opcode (scripts and conditions share it).
    pub fn call_function(&mut self, ctx: &Ctx, args: &[ArgValue]) -> Value {
        let data = self.data;
        let name = || data.table.get(ctx.opcode).map(|f| f.name.clone()).unwrap_or_else(|| format!("0x{:04X}", ctx.opcode));
        match functions::lookup(ctx.opcode) {
            Some(Kind::Impl(f)) => match f(self, ctx, &Args(args)) {
                Ok(v) => v,
                Err(e) => {
                    let n = name();
                    self.emit(EventKind::CallFailed { opcode: format!("0x{:04X}", ctx.opcode), name: n, unit: ctx.unit.clone(), error: e });
                    Value::ZERO
                }
            },
            Some(Kind::Stub(_)) => {
                *self.stubbed.entry(ctx.opcode).or_default() += 1;
                self.stub_note(ctx, args);
                Value::ZERO
            }
            None => {
                *self.unhandled.entry(ctx.opcode).or_default() += 1;
                if self.unhandled_seen.insert((ctx.opcode, ctx.unit.clone())) {
                    let n = name();
                    tracing::debug!(opcode = ctx.opcode, name = %n, unit = %ctx.unit, "unhandled script function");
                    self.emit(EventKind::Unhandled { opcode: format!("0x{:04X}", ctx.opcode), name: n, unit: ctx.unit.clone() });
                }
                Value::ZERO
            }
        }
    }

    /// Trace a call to a function that is implemented for its Layer 5 part but whose
    /// effect belongs to Layer 6 (e.g. AddScriptPackage).
    pub fn stub_note(&mut self, ctx: &Ctx, args: &[ArgValue]) {
        let data = self.data;
        let name = data.table.get(ctx.opcode).map(|f| f.name.clone()).unwrap_or_default();
        let args: Vec<serde_json::Value> = args.iter().map(|a| arg_json(data, a)).collect();
        let on = ctx.on.or(ctx.self_ref);
        let sig = format!("{on:?}{args:?}");
        if !self.stubs_seen.insert((ctx.opcode, ctx.unit.clone(), sig)) {
            return;
        }
        self.emit(EventKind::Stub { opcode: format!("0x{:04X}", ctx.opcode), name, unit: ctx.unit.clone(), on, args });
    }

    // ---- snapshots -----------------------------------------------------------------

    pub fn snapshot(&self) -> GameSnapshot {
        GameSnapshot {
            format: SNAPSHOT_FORMAT.into(),
            sim: self.world.snapshot(),
            quests: self.quests.clone(),
            instances: self.instances.iter().map(|(k, v)| (*k, v.clone())).collect(),
            extra: self.extra.clone(),
            player: self.player.clone(),
            form_lists: self.form_lists.clone(),
            loaded_cells: self.loaded_cells.clone(),
            moved: self.moved.clone(),
            menu: self.menu,
            message_box: self.message_box.clone(),
            frame: self.frame,
            time: self.time,
            rng_word_pos: self.rng.get_word_pos().to_string(),
        }
    }

    pub fn restore(data: &'a GameData, world: World<'a>, cfg: GameConfig, s: &GameSnapshot) -> Result<Game<'a>, String> {
        if s.format != SNAPSHOT_FORMAT {
            return Err(format!("unsupported snapshot format {:?}", s.format));
        }
        let mut g = Game::new(data, world, cfg);
        g.quests = s.quests.clone();
        g.instances = s.instances.iter().cloned().collect();
        g.extra = s.extra.clone();
        g.player = s.player.clone();
        g.form_lists = s.form_lists.clone();
        g.loaded_cells = s.loaded_cells.clone();
        g.moved = s.moved.clone();
        g.menu = s.menu;
        g.message_box = s.message_box.clone();
        g.frame = s.frame;
        g.time = s.time;
        let pos: u128 = s.rng_word_pos.parse().map_err(|_| format!("bad rng_word_pos {:?}", s.rng_word_pos))?;
        g.rng.set_word_pos(pos);
        Ok(g)
    }
}

/// An argument as trace JSON (forms by FormID, with the Editor ID when known).
pub fn arg_json(data: &GameData, a: &ArgValue) -> serde_json::Value {
    use serde_json::json;
    match a {
        ArgValue::Value(Value::Ref(f)) => match data.edid_of.get(&FormId(*f)) {
            Some(e) => json!(format!("{} ({e})", FormId(*f))),
            None => json!(FormId(*f).to_string()),
        },
        ArgValue::Value(Value::Num(x)) => json!(x),
        ArgValue::Enum(c) => json!({"enum": c}),
        ArgValue::Axis(c) => json!(c.to_string()),
        ArgValue::Str(s) => json!(s),
        ArgValue::Vars(v) => json!(v.iter().map(|x| x.num()).collect::<Vec<_>>()),
        ArgValue::Int32(i) => json!(i),
    }
}

/// The host the VM runs against for one script run.
struct Exec<'g, 'a> {
    game: &'g mut Game<'a>,
    unit: &'a Compiled,
    locals: Locals,
    ctx: RunCtx,
}

impl Exec<'_, '_> {
    fn inst_key(&self) -> Option<InstKey> {
        match self.locals {
            Locals::Instance(k) => Some(k),
            Locals::Temp(_) => None,
        }
    }

    fn global_form(&mut self, g: RefIndex) -> Result<FormId, String> {
        match self.unit.unit.slot(g) {
            Some(RefSlot::Form(f)) => Ok(FormId(f)),
            other => Err(format!("global slot {} is {other:?}", g.0)),
        }
    }

    fn remote(&mut self, r: RefIndex) -> Result<(InstKey, FormId), String> {
        let f = self.ref_value(r)?.as_form().ok_or_else(|| format!("reference slot {} is empty", r.0))?;
        self.game.instance_of(FormId(f)).ok_or_else(|| format!("{} runs no script", self.game.data.name(FormId(f))))
    }
}

impl Host for Exec<'_, '_> {
    fn get_var(&mut self, var: &VarRef) -> Result<Value, String> {
        match (var.on, var.var) {
            (_, VarSlot::Global { global }) => {
                let g = self.global_form(global)?;
                Ok(Value::Num(self.game.world.global(g).map_err(|e| e.to_string())? as f64))
            }
            (None, VarSlot::Local { index, .. }) => Ok(match &self.locals {
                Locals::Temp(m) => m.get(&index).copied().unwrap_or_default(),
                Locals::Instance(k) => self.game.var(*k, index),
            }),
            (Some(r), VarSlot::Local { index, .. }) => {
                let (key, script) = self.remote(r)?;
                if self.game.data.script(script).is_some_and(|c| c.var_type(index).is_none()) {
                    let unit = self.unit.key();
                    self.game.emit(EventKind::UndeclaredVariable { owner: key.label(), index, unit });
                }
                Ok(self.game.var(key, index))
            }
        }
    }

    fn set_var(&mut self, var: &VarRef, value: Value) -> Result<(), String> {
        let short = |l| matches!(l, fnv_script::ir::VarLetter::Short);
        match (var.on, var.var) {
            (_, VarSlot::Global { global }) => {
                let g = self.global_form(global)?;
                let v = value.num() as f32;
                self.game.world.set_global(g, v).map_err(|e| e.to_string())?;
                self.game.emit(EventKind::GlobalSet { global: g, value: v });
            }
            (None, VarSlot::Local { index, letter }) => match &mut self.locals {
                Locals::Temp(m) => {
                    let int = matches!(self.unit.var_type(index), Some(VarType::Int)) || (self.unit.var_type(index).is_none() && short(letter));
                    m.insert(index, if int { Value::Num(to_int_storage(value.num()) as f64) } else { value });
                }
                Locals::Instance(k) => {
                    let k = *k;
                    let script = self.game.instances.get(&k).and_then(|i| i.script);
                    let unit = self.unit.key();
                    self.game.store(k, script, index, value, &unit, short(letter));
                    if self.game.cfg.trace_locals {
                        let value = self.game.var(k, index);
                        let name = self.game.var_name(script, index);
                        self.game.emit(EventKind::VarSet { owner: k.label(), index, name, value });
                    }
                }
            },
            (Some(r), VarSlot::Local { index, letter }) => {
                let (key, script) = self.remote(r)?;
                let unit = self.unit.key();
                self.game.store(key, Some(script), index, value, &unit, short(letter));
                let value = self.game.var(key, index);
                let name = self.game.var_name(Some(script), index);
                self.game.emit(EventKind::VarSet { owner: key.label(), index, name, value });
            }
        }
        Ok(())
    }

    fn ref_value(&mut self, r: RefIndex) -> Result<Value, String> {
        match self.unit.unit.slot(r) {
            Some(RefSlot::Form(f)) => Ok(Value::Ref(f)),
            Some(RefSlot::Local(i)) => {
                let v = VarRef { on: None, var: VarSlot::Local { index: i as u16, letter: fnv_script::ir::VarLetter::Float } };
                self.get_var(&v)
            }
            Some(RefSlot::Unmapped(l)) => Err(format!("reference slot {} holds unmapped FormID 0x{l:08X}", r.0)),
            None => Err(format!("reference slot {} is not in the list", r.0)),
        }
    }

    fn call(&mut self, opcode: u16, on: Option<Value>, args: &[ArgValue]) -> Result<Value, String> {
        let on = match on {
            Some(v) => match v.as_form() {
                Some(f) => Some(FormId(f)),
                None => {
                    // A call on an empty reference variable: the engine cannot run it
                    // [hypothesis: skipped with an error report].
                    let name = self.game.data.table.get(opcode).map(|f| f.name.clone()).unwrap_or_default();
                    let unit = self.unit.key();
                    self.game.emit(EventKind::CallFailed {
                        opcode: format!("0x{opcode:04X}"),
                        name,
                        unit,
                        error: "called on an empty reference".into(),
                    });
                    return Ok(Value::ZERO);
                }
            },
            None => None,
        };
        let ctx = Ctx {
            unit: self.unit.key(),
            opcode,
            on,
            self_ref: self.ctx.self_ref,
            action_ref: self.ctx.action_ref,
            quest: self.ctx.quest,
            inst: self.inst_key(),
            in_condition: false,
        };
        Ok(self.game.call_function(&ctx, args))
    }

    fn skipped(&mut self, what: &VmError) {
        let unit = self.unit.key();
        self.game.emit(EventKind::Skipped { unit, what: what.to_string() });
    }
}
