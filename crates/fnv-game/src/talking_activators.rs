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
    /// lives, else itself.
    pub fn talker(&self, r: FormId) -> (FormId, Option<FormId>) {
        if !self.is_talking_activator(r) {
            return (r, None);
        }
        let actor = self.extra.get(&r).and_then(|e| e.talking_actor).filter(|&a| !self.is_dead(a));
        (actor.unwrap_or(r), Some(r))
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
