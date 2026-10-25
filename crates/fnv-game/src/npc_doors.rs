//! NPCs at scripted doors (docs/spec-notes/doors.md §5): an actor the AI walks through
//! a door whose script has an OnActivate block runs that block first, with itself as the
//! action reference, and goes through only if the block activates the door for it.
//!
//! - **The block runs for NPCs.** The original fires the activation event for NPCs that
//!   open doors, in unloaded cells too [observed: xNVSE OnActivate events naming NPCs at
//!   HVLevel1To2Door and vCCHQDoor, script-function-coverage.md §3.3]; the shipped door
//!   scripts are written for it: of the 214 scripted doors in the ten masters, 111 have
//!   a block that activates the door for whoever is not the player and VHDExteriorLadder
//!   moves an NPC to the ladder's linked marker itself [observed: the door survey,
//!   doors.md §5.1]; the GECK
//!   wiki's own IsActionRef example refuses the player and calls `Activate` for anyone
//!   else [documented: GECK wiki IsActionRef].
//! - **The block decides.** An OnActivate block runs instead of the default activation;
//!   the door opens only when the block calls `Activate` [documented: GECK wiki Activate,
//!   "The OnActivate block of the object will be run instead of the default
//!   activation"]. That this holds for an NPC's walk through the door as for the player's
//!   hand is a [hypothesis]: no capture saw an NPC held at a door.
//! - **A refused door** is closed to that actor's paths for [`REFUSED_SECONDS`], as a
//!   locked door without its key is (`fnv_nav::PathParams::refused_doors`); its package
//!   is taken up afresh and finds another way or is set aside as out of reach [design].
//! - **A block for the player only** (`Begin OnActivate player`, 12 of the scripted
//!   doors, IndDoorSmAnim01 among them) does not run for an NPC, so it cannot take its
//!   activation over: the NPC goes through as through a door without a script
//!   [hypothesis].
//! - **When it is asked**: once as the actor comes within reach of the door, then not
//!   again for [`ASKED_SECONDS`] (the walk through takes a few frames) [design]. The
//!   block's `Activate` lets the actor through without the default action: the AI moves
//!   it and opens the door, as for a door without a script. The door's activate children
//!   go off as on a default activation (`crate::traps`).
//!
//! Not saved [design: the answers last seconds; after a load the doors are asked again].

use crate::game::{event, Game};
use crate::trace::EventKind;
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// How long a door's refusal keeps it out of the actor's paths: one game hour at the
/// default TimeScale of 30, game time seconds [design: the package retry's hour,
/// `fnv_ai::AiConfig::retry_hours`].
pub const REFUSED_SECONDS: f64 = 120.0;

/// How long a door that let an actor through is not asked again for it, game seconds
/// [design].
pub const ASKED_SECONDS: f64 = 10.0;

/// What a door said to an actor walking through it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoorAnswer {
    /// Its script has no OnActivate block (or it has no script): the actor goes through.
    NoBlock,
    /// The block activated the door for the actor.
    Let,
    /// The block ran and did not activate the door.
    Refused,
}

/// A door's last answer to one actor and until when it stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Gate {
    pub answer: DoorAnswer,
    pub until: f64,
}

/// The block being run for an NPC at a door: who, which door, and whether the block
/// activated it ([`Game::npc_door_activated`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DoorRun {
    pub actor: FormId,
    pub door: FormId,
    pub let_through: bool,
}

impl Game<'_> {
    /// Whether `door`'s script has an OnActivate block.
    pub fn door_has_activate_block(&self, door: FormId) -> bool {
        self.ref_script(door).and_then(|s| self.data.script(s)).is_some_and(|u| u.has_block(event::ON_ACTIVATE))
    }

    /// `actor` (an NPC the AI walks) is about to go through `door`: run the door's
    /// OnActivate block for it, unless it was asked lately, and say whether it may go
    /// through.
    pub fn npc_door(&mut self, actor: FormId, door: FormId) -> DoorAnswer {
        if let Some(g) = self.door_gates.get(&(actor, door)).filter(|g| self.time < g.until) {
            return g.answer;
        }
        if !self.door_has_activate_block(door) || self.activation_refused(door) {
            return DoorAnswer::NoBlock;
        }
        let outer = self.npc_door_run.replace(DoorRun { actor, door, let_through: false });
        let ran = self.ref_event(door, event::ON_ACTIVATE, Some(Value::Ref(actor.0)), Some(actor));
        let run = std::mem::replace(&mut self.npc_door_run, outer).expect("set above");
        let (answer, keep) = match (ran, run.let_through) {
            // `Begin OnActivate player`: the block is not for this actor.
            (false, _) => (DoorAnswer::NoBlock, ASKED_SECONDS),
            (true, true) => (DoorAnswer::Let, ASKED_SECONDS),
            (true, false) => (DoorAnswer::Refused, REFUSED_SECONDS),
        };
        let base = crate::functions::base_of(self, door).map(|b| self.data.name(b)).unwrap_or_default();
        tracing::info!("npc doors: {} at door {door} ({base}): {answer:?}", self.data.name(actor));
        self.door_gates.insert((actor, door), Gate { answer, until: self.time + keep });
        answer
    }

    /// Inside [`Game::npc_door`]'s block run: the block's `Activate` of the door for the
    /// actor walking through. It lets the actor through (the AI moves it and opens the
    /// door, so no default action here) and sets off the door's activate children.
    /// Whether this was that activation.
    pub(crate) fn npc_door_activated(&mut self, actor: FormId, target: FormId) -> bool {
        let Some(run) = self.npc_door_run.as_mut().filter(|r| r.actor == actor && r.door == target) else { return false };
        run.let_through = true;
        self.emit(EventKind::Activated { reference: target, by: actor, outcome: None });
        self.set_off_children(target);
        true
    }

    /// The doors that refused `actor` and still stand refused: kept out of its paths.
    pub fn refused_doors(&self, actor: FormId) -> Vec<u32> {
        self.door_gates
            .iter()
            .filter(|((a, _), g)| *a == actor && g.answer == DoorAnswer::Refused && self.time < g.until)
            .map(|((_, d), _)| d.0)
            .collect()
    }
}
