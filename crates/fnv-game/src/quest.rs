//! Quests: the definition read from `QUST` and the run-time state. Behaviour notes with
//! their evidence: docs/spec-notes/scripts.md §5.

use crate::cond::Condition;
use crate::data::UnitId;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// `QUST DATA` flag bits [documented: xEdit; schema `gameplay.json`].
pub const START_GAME_ENABLED: u8 = 0x01;
pub const ALLOW_REPEATED_TOPICS: u8 = 0x04;
pub const ALLOW_REPEATED_STAGES: u8 = 0x08;
pub const DEFAULT_SCRIPT_DELAY: u8 = 0x10;

/// `QSDT` stage entry flags [documented: xEdit].
pub const STAGE_COMPLETE: u8 = 0x01;
pub const STAGE_FAIL: u8 = 0x02;

/// The script delay of a quest whose `DATA` delay is 0: 5 seconds [documented: GECK
/// wiki SetQuestDelay, "quest scripts process every 5 seconds" by default and a delay of
/// 0 means that default; observed: VMS40Timer (flag clear) with
/// a delay of 0 ran every 5.0 s, `L5.quest.delay_zero`; docs/spec-notes/layer5-captures.md §3].
pub const DEFAULT_QUEST_DELAY: f64 = 5.0;

#[derive(Debug, Clone, Default)]
pub struct StageEntry {
    pub flags: u8,
    pub conditions: Vec<Condition>,
    /// CNAM: the log entry text.
    pub log: Option<String>,
    pub unit: Option<UnitId>,
    pub next_quest: Option<FormId>,
}

#[derive(Debug, Clone, Default)]
pub struct Target {
    pub target: Option<FormId>,
    pub conditions: Vec<Condition>,
}

#[derive(Debug, Clone, Default)]
pub struct Objective {
    pub index: i32,
    pub text: Option<String>,
    pub targets: Vec<Target>,
}

#[derive(Debug, Clone)]
pub struct QuestDef {
    pub form: FormId,
    pub edid: Option<String>,
    pub name: Option<String>,
    pub flags: u8,
    pub priority: u8,
    /// `DATA` script delay in seconds (absent in short `DATA`).
    pub delay: Option<f32>,
    pub script: Option<FormId>,
    pub conditions: Vec<Condition>,
    pub stages: BTreeMap<i32, Vec<StageEntry>>,
    pub objectives: BTreeMap<i32, Objective>,
    pub next_quest: Option<FormId>,
}

impl Default for QuestDef {
    fn default() -> Self {
        QuestDef {
            form: FormId(0),
            edid: None,
            name: None,
            flags: 0,
            priority: 0,
            delay: None,
            script: None,
            conditions: Vec::new(),
            stages: BTreeMap::new(),
            objectives: BTreeMap::new(),
            next_quest: None,
        }
    }
}

impl QuestDef {
    pub fn start_enabled(&self) -> bool {
        self.flags & START_GAME_ENABLED != 0
    }

    pub fn repeated_stages(&self) -> bool {
        self.flags & ALLOW_REPEATED_STAGES != 0
    }

    /// How often the quest script runs: the `DATA` delay, or 5 s when it is 0 or absent.
    /// The "Default Script Processing Delay" flag does not change it [observed: the
    /// delay is the same with the flag set (CGTutorial 1.0 s, VCG01 0.01 s) and clear
    /// (VMQ03aTimer 0.5 s, VCG00 0.01 s); a delay shorter than a frame runs every frame;
    /// `L5.quest.delay_*`, docs/spec-notes/layer5-captures.md].
    pub fn script_delay(&self) -> f64 {
        match self.delay {
            Some(d) if d > 0.0 => d as f64,
            _ => DEFAULT_QUEST_DELAY,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectiveState {
    pub displayed: bool,
    pub completed: bool,
}

/// A quest at run time.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct QuestState {
    pub running: bool,
    pub completed: bool,
    pub failed: bool,
    pub stages_done: BTreeSet<i32>,
    /// `SetObjectiveDisplayed`/`Completed` flags by objective index.
    pub objectives: BTreeMap<i32, ObjectiveState>,
    /// Game-mode seconds since the quest script last ran (GameMode's delay counter;
    /// stopped in menus and loading screens).
    pub since_run: f64,
    /// Menu seconds since the quest script last ran in a menu (MenuMode's delay counter)
    /// [hypothesis: each mode keeps its own counter].
    #[serde(default)]
    pub since_menu_run: f64,
    /// `SetQuestDelay` override.
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub delay: Option<f64>,
    /// Set by the save-state `given` of a scenario: the current stage when no stage is
    /// done (a save can report a stage the stages-done list does not explain).
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub stage_override: Option<i32>,
}

impl QuestState {
    /// `GetStage`: the highest stage done, 0 when none [observed: a lower stage set after
    /// a higher one leaves GetStage at the higher (VMS16 50 then 10, Generic 70 then 60,
    /// `L5.quest.stage_order`, `repeated_stage`); documented: GECK wiki GetStage].
    pub fn stage(&self) -> i32 {
        let done = self.stages_done.iter().next_back().copied();
        match (done, self.stage_override) {
            (Some(d), Some(o)) => d.max(o),
            (Some(d), None) => d,
            (None, Some(o)) => o,
            (None, None) => 0,
        }
    }
}
