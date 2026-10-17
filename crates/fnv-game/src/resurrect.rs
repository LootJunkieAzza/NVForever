//! ResurrectActor: a dead actor alive again (docs/spec-notes/resurrect.md).
//!
//! - **The rules** [observed: function probes fp5, 2026-10-17: after `KillActor player 0
//!   1` then `ResurrectActor 0` on Ringo, GetDead 0, GetCauseofDeath −1,
//!   GetHealthPercentage 1.0]: not dead or unconscious, its killer and cause of death
//!   forgotten, unharmed (every actor value's damage gone, the limbs' too [hypothesis:
//!   the owner's "generated again from its record", below]).
//! - **Generated again** [observed: owner, 2026-10-17: "resurrect rerolls any level list
//!   or random gen on the actor record so if it comes from a template with an item list
//!   or its a generic that can be male female all this will be re rolled on the
//!   resurrected actor"]: a dead actor's random choices are rolled again
//!   ([`RefExtra::rolls`](crate::game::RefExtra::rolls), the seed
//!   `fnv_sim::leveled::roll_seed`): the entry a leveled actor list stands for (which can
//!   be a man or a woman), the template its carried items come from, and the carried
//!   items themselves with their leveled lists (`fnv_sim::World::reset_inventory`), worn
//!   and drawn again. So what was looted from the corpse, or given to it, is not what it
//!   carries alive: it carries a fresh roll. What scripts gave it otherwise stays
//!   (factions, packages, actor values set, essential, AI off, dispositions) [hypothesis:
//!   the quests that resurrect companions and Yes Man rely on it].
//! - **Where**: where its reference is, which is where it died or where a script moved
//!   the corpse to [observed: the shipped scripts move the corpse first, e.g. Yes Man's
//!   `MoveTo` to a marker and `ResurrectActor` after it, VMQYesManSupport, and the robot
//!   squads' `bot01.MoveTo` then `bot01.ResurrectActor` a frame later, decompiled
//!   2026-10-17]. It stands there in its default standing pose [observed: owner,
//!   2026-10-17: "they respawn in their default standing position"]; the get-up clip of
//!   the animate flag is not played [design: the owner, 2026-10-17, "we shouldnt need a
//!   getup animation for resurrect"].
//! - **The animate flag** (0 just standing, 1 standing up [documented: GECK wiki
//!   ResurrectActor]) is kept with the resurrection for the engine; both stand at once.
//! - **A living actor** (scripts call it from OnHit when Health is below 0, decompiled)
//!   is only healed: nothing is rolled again [hypothesis: generating a living actor again
//!   would strip a companion of its gear].
//! - **Not loaded**: the rules are the same; the engine draws it alive when its cell is
//!   (it is no corpse any more), with its new roll.
//! - It leaves combat (`Game::stop_combat`) [hypothesis: a new actor starts calm].

use crate::functions::base_of;
use crate::game::Game;
use fnv_sim::leveled::roll_seed;
use fnv_sim::FormId;
use serde::Serialize;

/// One ResurrectActor for the engine (`Game::take_resurrections`).
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Resurrection {
    pub actor: FormId,
    /// The animate flag (1: stand up; module notes).
    pub animate: bool,
    /// It was dead and has been generated again (its look may have changed): the engine
    /// draws it anew.
    pub regenerated: bool,
    /// Its rolls after this one (`RefExtra::rolls`).
    pub rolls: u32,
}

impl Game<'_> {
    /// `actor.ResurrectActor animate` (module notes).
    pub fn resurrect(&mut self, r: FormId, animate: bool) {
        let regenerated = self.is_dead(r);
        let e = self.extra.entry(r).or_default();
        e.dead = false;
        e.unconscious = false;
        e.killer = None;
        e.death_cause = None;
        e.av_damage.clear();
        e.last_hit = None;
        e.player_damage = 0.0;
        e.total_damage = 0.0;
        e.critical_stage = 0;
        e.furniture = None;
        e.weapon_out = false;
        if regenerated {
            e.rolls += 1;
        }
        let rolls = e.rolls;
        if regenerated {
            self.roll_again(r, rolls);
            self.stop_combat(r);
        }
        self.resurrections.push(Resurrection { actor: r, animate, regenerated, rolls });
    }

    /// Roll the carried items of `r` again from the template its new seed leads to.
    fn roll_again(&mut self, r: FormId, rolls: u32) {
        if let Some(base) = base_of(self, r) {
            let source = self.data.template_source(base, roll_seed(r, rolls), |d| d.template_inventory).map(|(s, _)| s).filter(|&s| s != base);
            self.world.set_inventory_source(r, source);
        }
        // A base with no carried items: it carries nothing (what the corpse was given goes).
        if !self.world.reset_inventory(r).unwrap_or(true) {
            if let Ok(s) = self.world.get(r) {
                s.inventory = None;
            }
        }
    }

    /// How many times ResurrectActor rolled the actor's random choices again.
    pub fn rolls(&self, r: FormId) -> u32 {
        self.extra.get(&r).map_or(0, |e| e.rolls)
    }

    /// The resurrections since the last call (the engine stands the actors up).
    pub fn take_resurrections(&mut self) -> Vec<Resurrection> {
        std::mem::take(&mut self.resurrections)
    }
}
