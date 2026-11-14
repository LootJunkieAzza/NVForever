//! Talking activators: radios with lines, intercoms, loudspeakers (TACT records,
//! docs/spec-notes/talking-activators.md).
//!
//! - **Talking to one.** The default action of a talking activator is a conversation
//!   with it, as an actor's is [hypothesis: the masters' intercom scripts end their
//!   OnActivate block with `Activate` after SetTalkingActivatorActor, so the default
//!   action is what starts the talk, observed: e.g. the Tops' presidential intercom,
//!   VMS18's waiter intercom, Repcon's]. The simulation answers it with
//!   `Outcome::Talk` (`fnv_sim::World::activate`).
//! - **Who speaks.** The actor SetTalkingActivatorActor gave the activator (kept on the
//!   activator's reference, `RefExtra::talking_actor`) speaks through it: its lines,
//!   chosen with it as the speaker, in its voice; without one, or when that actor is dead,
//!   the activator speaks for itself, in its own voice type (TACT VNAM) [documented:
//!   catalogue SetTalkingActivatorActor, "makes a talking activator use the dialogue of
//!   a given actor; with no actor it reverts to normal behaviour"; hypothesis: the dead
//!   actor's case, which the scripts avoid by testing GetDead first, observed]. The
//!   activator must be there; the actor need not be (Benny talks from his suite).
//! - **No talking actor set yet.** A talking activator that no SetTalkingActivatorActor
//!   has reached speaks for the one actor the load order's scripts give it, when they
//!   name exactly one: its base's own script on itself, or any script on it by name
//!   [hypothesis, issue #2454: the Hidden Valley intercom's lines are all Ramos's, and
//!   after McNamara sends the player out the inner door's script (SCPT 000E3665) has it
//!   start a conversation; on the routes into the bunker that never touch the intercom
//!   (sneaking in) neither its OnActivate (SCPT 000A418A) nor Veronica's trigger (SCPT
//!   0015B6D5) has set Ramos, and without a line the controls that line 0016329B took
//!   never come back; observer check 130].
//! - **IsActorTalkingThroughActivator** is 1 on that actor while its lines are chosen
//!   and while the conversation through the activator runs [documented: catalogue,
//!   "returns true if the actor is speaking via an activator; the actor must first be
//!   registered through the talking-activator setter"; the 92 conditions of the masters'
//!   lines use it so, observed; hypothesis: that it holds during the conversation].
//!   Outside one it is 0 with the actor set [observed: capture ac6]. The catalogue's
//!   note that IsTalkingActivatorActor is false "if the talking actor was assigned by
//!   script" does not hold: it read 1 after SetTalkingActivatorActor [observed: ac6].
//! - The engine looks at the activator, not at the far-away actor, while the
//!   conversation runs (`Conversation::through`).
//! - **A Goodbye greeting** through one opens a conversation, as a force greet's does
//!   (docs/spec-notes/dialogue.md §5), and the line ends it [owner statement 2026-10-22,
//!   of Chris Haversam's "I'm letting you in" from the REPCONN intercom (INFO 000CD87E):
//!   "even if the line has a goodbye it still draws into dialogue"]. Said at the player
//!   without one, it was said where Chris is, upstairs in another cell, out of hearing;
//!   the player tried again, and the new conversation cut the silent line before its end
//!   script unlocked the door beside the intercom.

use crate::functions::{base_of, yes, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;

impl Game<'_> {
    /// Whether `r` is a placed talking activator (base TACT).
    pub fn is_talking_activator(&self, r: FormId) -> bool {
        base_of(self, r).and_then(|b| self.data.sig(b)).is_some_and(|s| &s == b"TACT")
    }

    /// Who speaks when the player talks to `r`, and through which activator (module
    /// notes): an actor is itself; a talking activator its talking actor while that one
    /// lives, else the one actor its scripts name ([`Game::scripted_talking_actor`])
    /// while that one lives, else itself.
    pub fn talker(&self, r: FormId) -> (FormId, Option<FormId>) {
        if !self.is_talking_activator(r) {
            return (r, None);
        }
        let set = self.extra.get(&r).and_then(|e| e.talking_actor);
        let actor = set.or_else(|| self.scripted_talking_actor(r)).filter(|&a| !self.is_dead(a));
        (actor.unwrap_or(r), Some(r))
    }

    /// The talking actor the load order's scripts give the talking activator `r`, when
    /// they name exactly one: the `SetTalkingActivatorActor` calls of its base's own
    /// script (on itself) and the calls of any script on `r` by name (module notes,
    /// "No talking actor set yet"). `None` when no script names one, or several do.
    pub fn scripted_talking_actor(&self, r: FormId) -> Option<FormId> {
        const SET_TALKING_ACTIVATOR_ACTOR: u16 = 0x1171;
        let own = base_of(self, r).and_then(|b| self.data.script_of_base(b));
        let mut named: Option<FormId> = None;
        for c in &self.data.units {
            let is_own = own.is_some_and(|s| self.data.script(s).is_some_and(|o| std::ptr::eq(o, c)));
            let form = |i: fnv_script::ir::RefIndex| match c.unit.slot(i) {
                Some(fnv_script::unit::RefSlot::Form(f)) => Some(FormId(f)),
                _ => None,
            };
            let mut several = false;
            c.program.walk(&mut |st| {
                let fnv_script::ir::Stmt::Call { call } = st else { return };
                if call.opcode != SET_TALKING_ACTIVATOR_ACTOR {
                    return;
                }
                let on_r = match call.on {
                    None => is_own,
                    Some(i) => form(i) == Some(r),
                };
                if !on_r {
                    return;
                }
                let actor = match call.args.first() {
                    Some(fnv_script::ir::Arg::Ref(i)) => form(*i),
                    _ => None,
                };
                // A call that clears the actor or names it by a variable names none.
                let Some(actor) = actor else { return };
                match named {
                    None => named = Some(actor),
                    Some(a) if a != actor => several = true,
                    _ => {}
                }
            });
            if several {
                return None;
            }
        }
        named
    }

    /// The activator `actor` talks through now, if any (module notes).
    pub fn talking_through(&self, actor: FormId) -> Option<FormId> {
        if let Some((a, t)) = self.talking_through {
            if a == actor {
                return Some(t);
            }
        }
        self.dialogue.as_ref().filter(|c| c.actor == actor).and_then(|c| c.through)
    }

    /// Where the conversation with the player is heard and looked at: the activator it
    /// goes through, else its actor.
    pub fn dialogue_place(&self) -> Option<FormId> {
        self.dialogue.as_ref().map(|c| c.through.unwrap_or(c.actor))
    }
}

/// `IsActorTalkingThroughActivator` on the calling actor (module notes).
pub fn is_actor_talking_through_activator(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    yes(g.talking_through(r).is_some())
}

/// `activator.IsTalkingActivatorActor actor`: 1 when `actor` is the activator's talking
/// actor [documented: catalogue summary], set by script too [observed: capture ac6, 0
/// before, 1 after SetTalkingActivatorActor, 0 once cleared].
pub fn is_talking_activator_actor(g: &mut Game, ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let actor = a.form(0)?;
    yes(g.extra.get(&r).and_then(|e| e.talking_actor) == Some(actor))
}
