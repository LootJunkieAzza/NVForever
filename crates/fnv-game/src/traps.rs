//! Activate parents: a reference activated by its default action sets off its activate
//! children (XAPR), each after its own delay — the way tripwires and pressure plates set
//! off rigged shotguns, grenade bouquets, swinging traps and pitching machines
//! (docs/spec-notes/traps.md §2).
//!
//! - **What sets them off**: the parent's *default* activation — a script's `Activate`
//!   without the OnActivate flag (the tripwire's `activate self`, the pressure plate's
//!   `activate`), or the player's hand on a reference without an OnActivate block. An
//!   activation that runs the parent's OnActivate block instead does not [hypothesis: the
//!   tripwire's OnActivate offers the disarm box to the player, which would otherwise fire
//!   the shotgun it guards; GECK wiki Activate, flag 1 runs the block "instead of the
//!   default activation"].
//! - **How a child is set off**: it is activated with the parent as its action reference,
//!   running its OnActivate block when it has one, else its default action [hypothesis:
//!   the rigged shotgun's OnActivate fires only when the action reference is not the
//!   player, so the parent, not the actor that tripped it, must be the action reference].
//! - **When**: `delay` seconds of game time after the parent's activation (XAPR's float;
//!   [documented: xEdit "Delay"]), on the frame that passes it; a delay of 0 goes off on
//!   the same frame's tick. A child set off while its parent's cell is not loaded still
//!   goes off [hypothesis]. Pending children are not saved [design: they are due within
//!   seconds].

use crate::game::{ActMode, Game};
use crate::trace::EventKind;
use fnv_sim::FormId;

/// A child waiting for its delay to pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DueChild {
    /// Game time ([`Game::time`]) when it goes off.
    pub due: f64,
    pub parent: FormId,
    pub child: FormId,
}

impl Game<'_> {
    /// The activate children of `parent` (XAPR read backwards), with their delays.
    pub fn activate_children(&self, parent: FormId) -> Vec<(FormId, f32)> {
        self.world.placed.activate_children.get(&parent).cloned().unwrap_or_default()
    }

    /// Whether only its activate parents can set `r` off (XAPD "Parent Activate Only"):
    /// the player's hand is refused [hypothesis: the flag's meaning from its name].
    pub fn parent_activate_only(&self, r: FormId) -> bool {
        self.world.placed.get(r).is_some_and(|p| p.parent_activate_only)
    }

    /// `parent` was activated by its default action: queue its activate children.
    pub fn set_off_children(&mut self, parent: FormId) {
        for (child, delay) in self.activate_children(parent) {
            self.emit(EventKind::ActivateChild { parent, child, delay });
            self.due_children.push(DueChild { due: self.time + delay.max(0.0) as f64, parent, child });
        }
    }

    /// Activate the children whose delay has passed (from [`Game::tick`]). A child
    /// disabled or deleted meanwhile is skipped.
    pub(crate) fn activate_due_children(&mut self) {
        if self.due_children.is_empty() {
            return;
        }
        let now = self.time;
        let (due, wait): (Vec<DueChild>, Vec<DueChild>) = self.due_children.drain(..).partition(|d| d.due <= now);
        self.due_children = wait;
        for d in due {
            if !self.present(d.child) {
                continue;
            }
            if let Err(e) = self.activate(d.parent, d.child, ActMode::Block) {
                self.emit(EventKind::ScriptError { unit: format!("activate child {}", d.child), error: e });
            }
        }
    }
}
