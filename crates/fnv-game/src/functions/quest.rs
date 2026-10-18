//! Quest functions. Behaviour: docs/spec-notes/scripts.md §5 (captures named there).

use super::{none, yes, Args, Ctx};
use crate::game::{Game, InstKey};
use fnv_script::vm::Value;
use fnv_sim::FormId;

fn quest(g: &Game, a: &Args, i: usize) -> Result<FormId, String> {
    let q = a.form(i)?;
    if g.data.is_quest(q) {
        Ok(q)
    } else {
        Err(format!("{} is not a quest", g.data.name(q)))
    }
}

/// The quest a function names, or the running quest when the argument is absent
/// (`GetQuestCompleted` documents an optional quest).
fn quest_or_own(g: &Game, ctx: &Ctx, a: &Args) -> Result<FormId, String> {
    match a.opt_form(0) {
        Some(_) => quest(g, a, 0),
        None => ctx.quest.ok_or_else(|| "no quest".to_string()),
    }
}

pub fn set_stage(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let stage = a.int(1)?;
    g.set_stage(q, stage);
    none()
}

pub fn get_stage(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    Ok(Value::Num(g.quests.get(&q).map(|s| s.stage()).unwrap_or(0) as f64))
}

pub fn get_stage_done(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let stage = a.int(1)?;
    yes(g.quests.get(&q).is_some_and(|s| s.stages_done.contains(&stage)))
}

pub fn start_quest(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    g.start_quest(q);
    none()
}

/// The quest stops; a running script finishes its current run [documented: GECK wiki
/// StopQuest].
pub fn stop_quest(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    g.stop_quest(q);
    none()
}

pub fn get_quest_running(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    yes(g.quests.get(&q).is_some_and(|s| s.running))
}

/// 1 for a completed or failed quest [documented: catalogue summary; observed: a Fail
/// Quest stage makes it 1, `L5.quest.stage_order`].
pub fn get_quest_completed(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest_or_own(g, ctx, a)?;
    yes(g.quests.get(&q).is_some_and(|s| s.completed || s.failed))
}

pub fn set_objective_displayed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let (i, on) = (a.int(1)?, a.num_or(2, 1.0) != 0.0);
    g.set_objective(q, i, Some(on), None);
    none()
}

pub fn set_objective_completed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let (i, on) = (a.int(1)?, a.num_or(2, 1.0) != 0.0);
    g.set_objective(q, i, None, Some(on));
    none()
}

pub fn get_objective_displayed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let i = a.int(1)?;
    yes(g.quests.get(&q).and_then(|s| s.objectives.get(&i)).is_some_and(|o| o.displayed))
}

pub fn get_objective_completed(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let i = a.int(1)?;
    yes(g.quests.get(&q).and_then(|s| s.objectives.get(&i)).is_some_and(|o| o.completed))
}

/// Conditions only: quest and variable index (xEdit `ptVariableName`).
pub fn get_quest_variable(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let i = a.int(1)? as u16;
    Ok(g.var(InstKey::Quest { quest: q }, i))
}

/// Conditions only: a reference's script variable by index.
pub fn get_script_variable(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = a.opt_form(0).map(Ok).unwrap_or_else(|| ctx.subject())?;
    let i = a.int(1)? as u16;
    let (key, _) = g.instance_of(r).ok_or_else(|| format!("{} runs no script", g.data.name(r)))?;
    Ok(g.var(key, i))
}

pub fn set_quest_delay(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let d = a.num(1)?;
    // A delay of 0 means the default [documented: GECK wiki SetQuestDelay].
    g.quest(q).delay = (d > 0.0).then_some(d);
    none()
}

/// Every objective the quest defines is marked completed (not displayed)
/// [documented: GECK wiki CompleteAllObjectives; the display rule of
/// SetObjectiveCompleted, scripts.md §5].
pub fn complete_all_objectives(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let ids: Vec<i32> = g.data.quests[&q].objectives.keys().copied().collect();
    for i in ids {
        g.set_objective(q, i, None, Some(true));
    }
    none()
}

/// The quest's stages, objectives, completed/failed state and script variables go back
/// to their start [documented: catalogue summary of ResetQuest]; whether it runs is
/// unchanged [hypothesis].
pub fn reset_quest(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let running = g.quests.get(&q).is_some_and(|s| s.running);
    g.quests.insert(q, crate::quest::QuestState { running, ..Default::default() });
    g.instances.remove(&InstKey::Quest { quest: q });
    none()
}

/// Marks the quest completed (the Pip-Boy part is presentation) [documented: catalogue
/// summary]. It does not stop the quest [hypothesis, as the Complete Quest stage flag,
/// layer5-captures.md §5.3].
pub fn complete_quest(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = quest(g, a, 0)?;
    let s = g.quest(q);
    if !s.completed {
        s.completed = true;
        g.quest_completed(q);
    }
    none()
}
