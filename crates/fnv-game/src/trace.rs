//! Script trace events (`fnv.script.trace/1` `events`, docs/design/layer5.md §3.8 and
//! docs/spec-notes/layer5-captures.md §4.2). Each event carries the frame it happened
//! in; the list is in the order things happened.

use fnv_script::vm::Value;
use fnv_sim::FormId;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum EventKind {
    QuestStarted { quest: FormId },
    QuestStopped { quest: FormId },
    QuestCompleted { quest: FormId },
    QuestFailed { quest: FormId },
    /// `SetStage` took effect; `entries` lists the stage entries whose conditions passed
    /// (their result scripts ran, in this order).
    StageSet { quest: FormId, stage: i32, entries: Vec<usize> },
    /// `SetStage` had no effect, and why.
    StageIgnored { quest: FormId, stage: i32, reason: &'static str },
    ObjectiveDisplayed { quest: FormId, index: i32, on: bool },
    ObjectiveCompleted { quest: FormId, index: i32, on: bool },
    /// A script variable was written: `owner` names the instance (`quest:…`,
    /// `ref:…`, `item:…@…`), `index` and `name` the variable.
    VarSet { owner: String, index: u16, #[serde(skip_serializing_if = "Option::is_none")] name: Option<String>, #[serde(serialize_with = "value_json")] value: Value },
    GlobalSet { global: FormId, value: f32 },
    RefEnabled { reference: FormId },
    RefDisabled { reference: FormId },
    ItemAdded { to: FormId, item: FormId, count: i32 },
    ItemRemoved { from: FormId, item: FormId, count: i32 },
    MessageShown { message: FormId, #[serde(skip_serializing_if = "Option::is_none")] text: Option<String>, #[serde(skip_serializing_if = "Vec::is_empty")] buttons: Vec<String> },
    Moved { reference: FormId, #[serde(skip_serializing_if = "Option::is_none")] cell: Option<FormId>, pos: [f32; 3] },
    Activated { reference: FormId, by: FormId, #[serde(skip_serializing_if = "Option::is_none")] outcome: Option<serde_json::Value> },
    /// A Begin block ran because of an event other than the per-frame ones.
    BlockRun { unit: String, block: String, #[serde(skip_serializing_if = "Option::is_none")] on: Option<FormId> },
    /// A dialogue or package result script ran (runner inputs standing in for Layer 6).
    ResultScript { unit: String, #[serde(skip_serializing_if = "Option::is_none")] on: Option<FormId> },
    /// A function whose system is Layer 6 (dialogue, AI packages, combat, sound, image
    /// space, menus): recorded, no effect. A call identical to one already traced from
    /// the same unit (a per-frame block repeating it) is counted in `final.stubbed`, not
    /// traced again.
    Stub { opcode: String, name: String, unit: String, #[serde(skip_serializing_if = "Option::is_none")] on: Option<FormId>, args: Vec<serde_json::Value> },
    /// A function with no implementation yet (first call per opcode and unit; all calls
    /// are counted in `final.unhandled`).
    Unhandled { opcode: String, name: String, unit: String },
    /// A function failed (bad argument, missing reference); the script went on with 0.
    CallFailed { opcode: String, name: String, unit: String, error: String },
    /// A script stopped early (budget, malformed expression, nesting depth).
    ScriptError { unit: String, error: String },
    /// Something the VM skipped (an unknown opcode in a lenient run).
    Skipped { unit: String, what: String },
    /// A variable index the target script does not declare (docs/spec-notes/scripts.md
    /// §2.5): read as 0, written as given.
    UndeclaredVariable { owner: String, index: u16, unit: String },
    /// An actor's AI packages were evaluated and it chose another package (none: no
    /// package's conditions pass).
    PackageChosen { actor: FormId, #[serde(skip_serializing_if = "Option::is_none")] package: Option<FormId> },
}

/// A value in the trace: a number, or a reference as its FormID.
fn value_json<S: serde::Serializer>(v: &Value, s: S) -> Result<S::Ok, S::Error> {
    match v {
        Value::Num(x) => s.serialize_f64(*x),
        Value::Ref(f) => s.serialize_str(&FormId(*f).to_string()),
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Event {
    pub frame: u64,
    #[serde(flatten)]
    pub kind: EventKind,
}
