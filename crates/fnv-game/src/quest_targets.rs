//! Quest objective targets: which targets of the objectives shown and not done pass their
//! conditions, and the door outdoors at which a target indoors is marked
//! (docs/spec-notes/quests-dialogue.md §2, "Target conditions"; ESM gap audit row 17).
//!
//! - A target's conditions run with the **target reference as subject** and no target
//!   [documented: GECK wiki Quest Objectives Tab, "the conditions are applied to the quest
//!   target", "Run on Target is invalid"; observed: the masters' subject-run `GetDead` /
//!   `GetDisabled` items on targets name no reference].
//! - The engine evaluates them for its markers as they change; the console's
//!   ShowQuestTargets and MoveToQuestTarget when called.

use crate::game::Game;
use fnv_sim::{FormId, World};

/// One target of an objective shown and not done, and whether its conditions pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetCheck {
    pub quest: FormId,
    pub objective: i32,
    /// The target's place in the objective's list (record order).
    pub index: usize,
    pub target: FormId,
    /// "Compass Marker Ignores Locks".
    pub ignores_locks: bool,
    /// The conditions pass: the target is marked.
    pub shown: bool,
}

impl Game<'_> {
    /// Every target of `quest`'s objectives shown and not done (objectives by index,
    /// targets in record order), each with whether its conditions pass now. A target
    /// with no reference (none set, or one the load order lacks) is left out.
    pub fn check_quest_targets(&mut self, quest: FormId) -> Vec<TargetCheck> {
        let data = self.data;
        let (Some(def), Some(state)) = (data.quests.get(&quest), self.quests.get(&quest)) else { return Vec::new() };
        let open: Vec<i32> = state.objectives.iter().filter(|(_, o)| o.displayed && !o.completed).map(|(i, _)| *i).collect();
        let mut out = Vec::new();
        for objective in open {
            for (index, t) in def.objectives.get(&objective).into_iter().flat_map(|d| d.targets.iter().enumerate()) {
                let Some(target) = t.target else { continue };
                let label = format!("QUST {quest} objective {objective} target {index}");
                let shown = self.conditions_pass(&t.conditions, Some(target), None, &label);
                out.push(TargetCheck { quest, objective, index, target, ignores_locks: t.ignores_locks, shown });
            }
        }
        out
    }

    /// The doors outdoors (in an exterior cell) that lead into the interior `cell`, in
    /// FormID order.
    pub fn doors_into(&self, cell: FormId) -> Vec<FormId> {
        let refs = self.world.placed;
        refs.refs
            .iter()
            .filter(|(_, d)| {
                d.teleport.is_some_and(|t| refs.get(t.door).is_some_and(|inside| inside.cell == Some(cell)))
                    && d.cell.is_some_and(|c| refs.cell_world.contains_key(&c))
            })
            .map(|(id, _)| *id)
            .collect()
    }
}

/// The door a marker for a target indoors stands at: the first of `doors` that is not
/// locked in `world`, else the first; with `ignores_locks` the first [hypothesis: the
/// Objectives Tab's path "will attempt to avoid locked doors if possible"; NVForever picks
/// one door, quests-dialogue.md §2].
pub fn marker_door(world: &World, doors: &[FormId], ignores_locks: bool) -> Option<FormId> {
    let first = doors.first().copied();
    if ignores_locks {
        return first;
    }
    doors.iter().copied().find(|d| !door_locked(world, *d)).or(first)
}

/// The door is locked now: its run-time state once the world holds one, else its placed
/// lock.
fn door_locked(world: &World, door: FormId) -> bool {
    match world.peek(door) {
        Some(s) => s.lock.is_some_and(|l| l.locked),
        None => world.placed.get(door).is_some_and(|p| p.lock.is_some()),
    }
}
