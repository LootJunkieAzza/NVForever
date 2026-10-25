//! What of an actor's package progress a save keeps (docs/design/save-load.md §3.3, §5
//! item 4). The package itself, and whether its goal is met, are the game's and saved with
//! it (`fnv_game`); the AI starts a package again from where the actor stands after a
//! load. Two things would show as a jump back:
//!
//! - **A patrol's point.** One that does not repeat starts at its first point, so a
//!   patroller saved half-way walked back to the start [documented: GECK wiki, Patrol
//!   Package, for the start]; a repeatable one starts at the nearest point. The save keeps
//!   the point being walked to.
//! - **A sandbox's day.** Its sleep and meal times are rolled from random numbers that
//!   started again at a load, so a sandboxer could find itself due for bed at another hour
//!   on the same day. The save keeps the day's times and where the numbers stand
//!   ([`crate::sandbox::Schedule`]).
//!
//! Each is put back the first time the actor takes up the same package after the load; an
//! actor whose package changed meanwhile starts it afresh [design]. Not kept: a wait at a
//! patrol point (it waits its full time again), walks under way, furniture in use, a
//! Dialogue package's phase, errands (save-load.md §3.3: rebuilt from the packages)
//! [design].

use crate::carry::Ai;
use crate::goal::Goal;
use crate::sandbox::Schedule;
use fnv_sim::FormId;
use serde::{Deserialize, Serialize};

/// One actor's progress with its package.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Progress {
    pub actor: FormId,
    pub package: FormId,
    /// The patrol point being walked to (an index into the package's route).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patrol_point: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sandbox: Option<Schedule>,
}

impl Ai {
    /// The progress of every actor that has some to keep, for a save.
    pub fn progress(&self) -> Vec<Progress> {
        self.agents
            .iter()
            .filter_map(|(&actor, a)| {
                let package = a.package?;
                let patrol_point = matches!(a.goal, Some(Goal::Route(_))).then_some(a.route_at);
                let sandbox = a.sandbox.as_ref().map(|s| s.schedule());
                (patrol_point.is_some() || sandbox.is_some()).then_some(Progress { actor, package, patrol_point, sandbox })
            })
            .collect()
    }

    /// A loaded save's progress: kept until each actor takes up that package again.
    pub fn restore(&mut self, saved: Vec<Progress>) {
        self.restored = saved.into_iter().map(|p| (p.actor, p)).collect();
    }

    /// The patrol point a save had `actor` walk to on `package`'s route of `points`
    /// points, taken once. An actor on another package forgets what was kept for it.
    pub(crate) fn restored_patrol(&mut self, actor: FormId, package: FormId, points: usize) -> Option<usize> {
        let p = self.restored.get_mut(&actor)?;
        if p.package != package {
            self.restored.remove(&actor);
            return None;
        }
        let at = p.patrol_point.take().filter(|&i| i < points);
        self.forget_if_done(actor);
        at
    }

    /// The sandbox day a save kept for `actor` on `package`, taken once.
    pub(crate) fn restored_schedule(&mut self, actor: FormId, package: FormId) -> Option<Schedule> {
        let p = self.restored.get_mut(&actor)?;
        if p.package != package {
            self.restored.remove(&actor);
            return None;
        }
        let s = p.sandbox.take();
        self.forget_if_done(actor);
        s
    }

    fn forget_if_done(&mut self, actor: FormId) {
        if self.restored.get(&actor).is_some_and(|p| p.patrol_point.is_none() && p.sandbox.is_none()) {
            self.restored.remove(&actor);
        }
    }
}
