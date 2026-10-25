//! Swinging traps: `trapUpdate` (docs/spec-notes/traps.md §4). TrapMaceSCRIPT (the
//! swinging log, I-beams, engine block and brahmin corpses) sets the trap's variables in
//! its OnActivate block and calls `trapUpdate` every frame from then on; the engine swings
//! the model's jointed bodies and tells which actors a moving body struck.
//!
//! - **Live**: a trap is live from its first `trapUpdate` while the calls go on
//!   ([`LIVE_SECONDS`] after the last): the engine lets its bodies go then (`nvforever`
//!   swing_traps.rs) [hypothesis: the script calls it only once activated, so the first
//!   call is the release].
//! - **A hit** ([`Game::trap_contact`]): a body moving at least `fTrapMinVelocity` (game
//!   units a second) that meets an actor hits it for `fTrapDamage` health and pushes it at
//!   `fTrapPushBack` units a second, and sets the script's `bTrapHasHit` to 1 (its GameMode
//!   plays the hit sound and sets it back) [hypothesis: the variables' meaning from their
//!   names; the catalogue has no page for trapUpdate]. An actor is hit once per trap
//!   unless `bTrapContinuous` is set [hypothesis]. The script lowers damage and push
//!   (40 → 5, 800 → 100) 6 s after the release, so a log still swinging does little.
//! - Not part of the saved state [design: a trap swings for seconds].

use crate::functions::{none, Args, Ctx};
use crate::game::Game;
use fnv_script::vm::Value;
use fnv_sim::FormId;
use std::collections::{BTreeMap, BTreeSet};

/// A trap stays live this long after its last `trapUpdate`, game seconds [design: a few
/// frames, so a menu's pause does not drop it].
pub const LIVE_SECONDS: f64 = 1.0;

/// The swinging traps' state.
#[derive(Debug, Clone, Default)]
pub struct SwingTraps {
    /// Each live trap and when it last called `trapUpdate`.
    pub live: BTreeMap<FormId, f64>,
    /// The actors each trap has hit.
    pub hit: BTreeMap<FormId, BTreeSet<FormId>>,
}

/// What a trap's hit does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrapStrike {
    /// Health taken.
    pub damage: f64,
    /// The struck actor's speed away from the trap, game units a second.
    pub push: f64,
}

/// `trap.trapUpdate`: the trap is live this frame (module notes).
pub fn trap_update(g: &mut Game, ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    let r = ctx.subject()?;
    let now = g.time;
    if g.swing_traps.live.insert(r, now).is_none() {
        tracing::info!("swing traps: {:08X} is let go", r.0);
    }
    none()
}

impl Game<'_> {
    /// The traps live now (module notes), for the engine.
    pub fn live_traps(&self) -> Vec<FormId> {
        let now = self.time;
        self.swing_traps.live.iter().filter(|(_, &t)| now - t <= LIVE_SECONDS).map(|(r, _)| *r).collect()
    }

    /// A body of `trap` moving at `speed` met `actor`: the hit it deals, if any (module
    /// notes). Sets the trap script's `bTrapHasHit`.
    pub fn trap_contact(&mut self, trap: FormId, actor: FormId, speed: f64) -> Option<TrapStrike> {
        let var = |g: &Game, name: &str| g.ref_var_named(trap, name).unwrap_or(0.0);
        let (damage, push, min, continuous) = (var(self, "fTrapDamage"), var(self, "fTrapPushBack"), var(self, "fTrapMinVelocity"), var(self, "bTrapContinuous") != 0.0);
        if speed < min || self.is_dead(actor) {
            return None;
        }
        let hit = self.swing_traps.hit.entry(trap).or_default();
        if !continuous && !hit.insert(actor) {
            return None;
        }
        if let Some((key, script)) = self.instance_of(trap) {
            if let Some(index) = self.data.script(script).and_then(|s| s.var_index("bTrapHasHit")) {
                self.store(key, Some(script), index, Value::Num(1.0), "trapUpdate", false);
            }
        }
        tracing::info!("swing traps: {:08X} hits {:08X} at {speed:.0} units/s: {damage:.0} damage, pushed at {push:.0}", trap.0, actor.0);
        Some(TrapStrike { damage, push })
    }
}
