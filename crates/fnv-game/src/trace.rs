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
    /// A reference was created at run time (PlaceAtMe, DropMe, Drop).
    Placed { reference: FormId, base: FormId, #[serde(skip_serializing_if = "Option::is_none")] cell: Option<FormId>, pos: [f32; 3] },
    /// A magic effect started on an actor (an ingestible consumed, a spell cast or
    /// added); `id` names its script instance (`effect:<id>`).
    EffectStarted { id: u32, target: FormId, source: FormId, effect: FormId },
    /// A magic effect ended (its duration passed, or it was dispelled).
    EffectFinished { id: u32, target: FormId, source: FormId },
    /// The same magic item was applied again while its effect was active: the active
    /// effect's duration grew to `duration` seconds (no new effect, no second start).
    EffectExtended { id: u32, target: FormId, source: FormId, duration: f64 },
    /// A menu opened (`menu` code; a message box is 1001) or was closed.
    MenuOpened { menu: u16 },
    MenuClosed { menu: u16 },
    /// A dialogue line started (SayTo, Say, a line of a conversation): who says it to
    /// whom, the topic and the line (INFO), and how long it lasts in seconds of game time.
    LineStarted { speaker: FormId, #[serde(skip_serializing_if = "Option::is_none")] target: Option<FormId>, topic: FormId, info: FormId, seconds: f64 },
    /// A dialogue line ended (its end result script runs next) or was cut off (`cut`:
    /// the speaker started another line, or was disabled or died).
    LineEnded { speaker: FormId, info: FormId, #[serde(skip_serializing_if = "std::ops::Not::not")] cut: bool },
    /// SayTo, Say or the start of a conversation found no line of the topic the speaker
    /// could say.
    NoLine { speaker: FormId, topic: FormId },
    /// A conversation between the player and `actor` started with a line of `topic`
    /// (`line_started` follows).
    DialogueStarted { actor: FormId, topic: FormId },
    /// One response of a conversation line started: its response number, the length of
    /// its text in characters, its emotion type code and how long it lasts.
    ResponseSaid { speaker: FormId, info: FormId, number: u8, chars: usize, emotion: u8, seconds: f64 },
    /// The list the player chooses from, in the order offered.
    ChoicesOffered { actor: FormId, choices: Vec<crate::dialogue::Choice> },
    /// The player chose entry `index` of the list.
    ChoiceMade { actor: FormId, index: usize, topic: FormId, info: FormId },
    /// A topic joined the topics the player knows: by a line's Add Topics list (`by`) or
    /// by AddTopic.
    TopicAdded { topic: FormId, #[serde(skip_serializing_if = "Option::is_none")] by: Option<FormId> },
    /// The conversation ended: `goodbye`, `no_choices`, `left` or `actor_gone`.
    DialogueEnded { actor: FormId, reason: &'static str, lines: u32 },
    /// A conversation between two actors started (StartConversation with an actor as the
    /// target): `speaker` says a line of `topic` to `target` (`line_started` follows).
    ActorConversationStarted { speaker: FormId, target: FormId, topic: FormId },
    /// A conversation between two actors ended after `lines` lines: `goodbye`, `no_links`,
    /// `cut` or `actor_gone`; `speaker` and `target` are those of its last line.
    ActorConversationEnded { speaker: FormId, target: FormId, reason: &'static str, lines: u32 },
    /// An actor is to play an idle animation, and what asked for it (`crate::idles`).
    IdleAsked { actor: FormId, idle: FormId, source: crate::idles::IdleSource },
    /// A script asked for a sound (`PlaySound`, `PlaySound3D`; `crate::sound`).
    SoundAsked { sound: FormId, #[serde(skip_serializing_if = "Option::is_none")] at: Option<FormId>, system: bool },
    /// A script asked for a music type (`PlayMusic`; `crate::sound`).
    MusicAsked { music: FormId },
    /// An idle was asked for and cannot be played, and why.
    IdleNotPlayed { actor: FormId, idle: FormId, source: crate::idles::IdleSource, reason: &'static str },
    /// An image space modifier started playing on the screen (ApplyImageSpaceModifier).
    ImageModifierApplied { modifier: FormId },
    /// An image space modifier left the screen: removed by a script, or (`ended`) an
    /// animated one reached the end of its duration.
    ImageModifierRemoved { modifier: FormId, #[serde(skip_serializing_if = "std::ops::Not::not")] ended: bool },
    /// A reputation changed: infamy (axis 0) or fame (axis 1), the new value.
    ReputationChanged { reputation: FormId, axis: usize, value: f64 },
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
