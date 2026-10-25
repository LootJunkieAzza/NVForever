//! Pickpocketing (docs/spec-notes/pickpocket.md): who may be pickpocketed, the chance
//! of each take or placement, being caught, and live explosives planted in an actor's
//! inventory ("exploding pants").
//!
//! - **Who** ([`Game::pickpocket_refusal`]): a living, present actor other than the
//!   player that is not hostile to the player and not a teammate; every NPC ("can all be
//!   pickpocketed" [documented: GECK wiki Category:NPC]) and the creatures flagged Allow
//!   Pickpocket (CREA ACBS bit 28 [documented: GECK wiki ActorHasBaseFlag]). Not a ghost:
//!   "Cannot be pickpocketed … Instead, regular activation (dialog) occurs" [documented:
//!   GECK wiki SetGhost notes]. Not one that has caught the player before: the game's
//!   `sNoPickPocketAgain` "has already caught you." [observed: FalloutNV.esm].
//! - **Chance** ([`Rules::chance`]): `(thief Sneak × fPickPocketActorSkillMult +
//!   fPickPocketActorSkillBase) + (target Sneak × fPickPocketTargetSkillMult +
//!   fPickPocketTargetSkillBase) + (item value × fPickPocketAmountMult +
//!   fPickPocketAmountBase)`, rounded down, kept within fPickPocketMinChance and
//!   fPickPocketMaxChance [documented: GECK wiki Pickpocket].
//! - **Caught** ([`Game::pickpocket_caught`]): the victim says Pickpocket ("Said by an
//!   actor if they notice you picking their pocket" [documented: GECK wiki Combat Tab])
//!   and turns on the player as for a seen theft; it will not be pickpocketed again.
//! - **Planted explosives** ([`Game::plant_explosive`]): a grenade or mine placed in an
//!   actor's inventory goes live; GetPlantedExplosive is 1 after
//!   `fProjectileInventoryGrenadeFreakoutTime`, and after
//!   `fProjectileInventoryGrenadeTimer` it explodes, "always killing it regardless of its
//!   HP" [documented: GECK wiki Projectile notes].

use crate::crime::{Kind, Note, Ownership};
use crate::functions::{actor, base_of};
use crate::game::Game;
use fnv_sim::FormId;

/// The pickpocket settings, with the wiki's defaults (FalloutNV.esm sets all but the two
/// bases and the bounds to these values [observed]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rules {
    pub actor_skill_mult: f64,
    pub actor_skill_base: f64,
    pub target_skill_mult: f64,
    pub target_skill_base: f64,
    pub amount_mult: f64,
    pub amount_base: f64,
    pub min_chance: f64,
    pub max_chance: f64,
    /// `fProjectileInventoryGrenadeTimer`, `fProjectileInventoryGrenadeFreakoutTime`
    /// (seconds) [observed: FalloutNV.esm 2.0 and 0.1].
    pub grenade_timer: f64,
    pub freakout_time: f64,
}

impl Default for Rules {
    fn default() -> Rules {
        Rules {
            actor_skill_mult: 0.6,
            actor_skill_base: 40.0,
            target_skill_mult: -0.6,
            target_skill_base: 0.0,
            amount_mult: -0.5,
            amount_base: 0.0,
            min_chance: 5.0,
            max_chance: 85.0,
            grenade_timer: 2.0,
            freakout_time: 0.1,
        }
    }
}

impl Rules {
    pub fn read(g: &Game) -> Rules {
        let d = Rules::default();
        let s = |name: &str, v: f64| crate::combat::setting(g, name, v);
        Rules {
            actor_skill_mult: s("fPickPocketActorSkillMult", d.actor_skill_mult),
            actor_skill_base: s("fPickPocketActorSkillBase", d.actor_skill_base),
            target_skill_mult: s("fPickPocketTargetSkillMult", d.target_skill_mult),
            target_skill_base: s("fPickPocketTargetSkillBase", d.target_skill_base),
            amount_mult: s("fPickPocketAmountMult", d.amount_mult),
            amount_base: s("fPickPocketAmountBase", d.amount_base),
            min_chance: s("fPickPocketMinChance", d.min_chance),
            max_chance: s("fPickPocketMaxChance", d.max_chance),
            grenade_timer: s("fProjectileInventoryGrenadeTimer", d.grenade_timer),
            freakout_time: s("fProjectileInventoryGrenadeFreakoutTime", d.freakout_time),
        }
    }

    /// The chance in percent of taking or placing items worth `value` caps (module notes).
    pub fn chance(&self, thief_sneak: f64, target_sneak: f64, value: f64) -> i32 {
        let c = (thief_sneak * self.actor_skill_mult + self.actor_skill_base)
            + (target_sneak * self.target_skill_mult + self.target_skill_base)
            + (value * self.amount_mult + self.amount_base);
        let c = c.floor();
        let (lo, hi) = (self.min_chance.min(self.max_chance), self.max_chance.max(self.min_chance));
        c.clamp(lo, hi) as i32
    }
}

/// Why the player may not pickpocket an actor now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    /// Dead, absent, the player, hostile, a teammate, a ghost or a creature without
    /// Allow Pickpocket: the actor is talked to (or searched) as usual.
    NotAllowed,
    /// It caught the player before: `sNoPickPocketAgain`.
    CaughtBefore,
}

/// A live explosive placed in an actor's inventory.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Planted {
    pub victim: FormId,
    pub item: FormId,
    /// Game time (seconds) it was placed.
    pub at: f64,
}

/// What [`Game::tick_planted`] found due: the victim and the explosive item, for the
/// engine to set off its explosion where the victim stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Blast {
    pub victim: FormId,
    pub item: FormId,
}

/// The pickpocket state the game keeps for the session (who caught the player is the
/// saved `RefExtra::caught_pickpocket`).
#[derive(Debug, Clone, Default)]
pub struct State {
    pub rules: Option<Rules>,
    pub planted: Vec<Planted>,
}

impl Game<'_> {
    fn pickpocket_rules(&mut self) -> Rules {
        if let Some(r) = self.pickpocket.rules {
            return r;
        }
        let r = Rules::read(self);
        self.pickpocket.rules = Some(r);
        r
    }

    /// Whether the player may pickpocket `r` now (module notes); `None`: it may.
    pub fn pickpocket_refusal(&mut self, r: FormId) -> Option<Refusal> {
        if r == FormId::PLAYER_REF || self.is_dead(r) || !self.present(r) || self.is_ghost(r) {
            return Some(Refusal::NotAllowed);
        }
        if self.extra.get(&r).is_some_and(|e| e.teammate || e.out_cold()) {
            return Some(Refusal::NotAllowed);
        }
        let creature = base_of(self, r).and_then(|b| self.data.actors.get(&b)).is_some_and(|d| &d.sig == b"CREA");
        if creature && !self.base_data_def(r).is_some_and(|d| d.allow_pickpocket) {
            return Some(Refusal::NotAllowed);
        }
        let fighting_player = self.awareness.get(r).is_some_and(|a| a.state.in_combat() && a.targets.contains_key(&FormId::PLAYER_REF));
        if fighting_player || self.would_attack(r, FormId::PLAYER_REF) {
            return Some(Refusal::NotAllowed);
        }
        if self.extra.get(&r).is_some_and(|e| e.caught_pickpocket) {
            return Some(Refusal::CaughtBefore);
        }
        None
    }

    /// The player's and the victim's Sneak, for [`Rules::chance`]. A creature's Sneak is
    /// its Stealth skill (`actor_values`).
    pub fn pickpocket_skills(&mut self, victim: FormId) -> (f64, f64) {
        let thief = actor::actor_value_capped(self, FormId::PLAYER_REF, crate::actor_values::SNEAK);
        let target = actor::actor_value_capped(self, victim, crate::actor_values::SNEAK);
        (thief, target)
    }

    /// The chance of taking or placing items worth `value` from or on `victim`.
    pub fn pickpocket_chance(&mut self, victim: FormId, value: f64) -> i32 {
        let rules = self.pickpocket_rules();
        let (thief, target) = self.pickpocket_skills(victim);
        rules.chance(thief, target, value)
    }

    /// The rules as read from the load order.
    pub fn pickpocket_settings(&mut self) -> Rules {
        self.pickpocket_rules()
    }

    /// The player took `count` of `item` from `victim` unseen: theft from its owner (the
    /// victim's base), so karma as for any theft (`crime.md` §3.1: an unseen theft costs
    /// karma too), and one more Pockets Picked [hypothesis: one per successful take].
    pub fn pickpocket_took(&mut self, victim: FormId, item: FormId, count: i32) {
        if let Some(owner) = base_of(self, victim) {
            let o = Ownership { owner, rank: None, from_cell: false };
            let s = self.crime_settings();
            if self.karma_for_theft_from(&o) {
                let name = self.data.name(item);
                self.change_karma(s.stealing, format!("pickpocketing {name} x{count}"));
            }
        }
        self.count_stat(crate::misc_stats::POCKETS_PICKED, 1.0);
    }

    /// The victim noticed the player's hand in its pocket: it says Pickpocket, turns on
    /// the player as a seen thief does (unless it ignores crime), and will not be
    /// pickpocketed again. Returns whether it turned.
    pub fn pickpocket_caught(&mut self, victim: FormId) -> bool {
        self.extra.entry(victim).or_default().caught_pickpocket = true;
        let ignores = self.extra.get(&victim).is_some_and(|e| e.ignore_crime);
        if ignores {
            // "The same as Pickpocket, but the actor doesn't regard it as a crime"
            // [documented: GECK wiki Combat Tab, PickpocketNoCrime].
            self.detection_bark(victim, Some(FormId::PLAYER_REF), "PickpocketNoCrime");
            return false;
        }
        let turned = self.crime_turn(victim, Kind::Pickpocket);
        let owner = base_of(self, victim);
        self.crimes.notes.push(Note::Crime { kind: Kind::Pickpocket, victim: owner, seen_by: vec![victim], hostile: if turned { vec![victim] } else { Vec::new() } });
        turned
    }

    /// The player placed `item` in `victim`'s pocket: a live explosive when it is one
    /// (`live`: the engine knows the weapons' projectiles), else nothing happens.
    pub fn plant_explosive(&mut self, victim: FormId, item: FormId) {
        let at = self.time;
        self.pickpocket.planted.push(Planted { victim, item, at });
    }

    /// GetPlantedExplosive: a live explosive was planted on `r` at least
    /// `fProjectileInventoryGrenadeFreakoutTime` ago and has not gone off.
    pub fn has_planted_explosive(&mut self, r: FormId) -> bool {
        let rules = self.pickpocket_rules();
        let now = self.time;
        self.pickpocket.planted.iter().any(|p| p.victim == r && now - p.at >= rules.freakout_time)
    }

    /// The planted explosives whose time is up: each victim dies (killed by the player)
    /// and loses the explosive, and one more Pants Exploded; the engine sets off the
    /// explosions. A victim that died or left before is let off [hypothesis].
    pub fn tick_planted(&mut self) -> Vec<Blast> {
        if self.pickpocket.planted.is_empty() {
            return Vec::new();
        }
        let rules = self.pickpocket_rules();
        let now = self.time;
        let (due, keep): (Vec<Planted>, Vec<Planted>) = std::mem::take(&mut self.pickpocket.planted).into_iter().partition(|p| now - p.at >= rules.grenade_timer);
        self.pickpocket.planted = keep;
        let mut out = Vec::new();
        for p in due {
            if self.is_dead(p.victim) {
                continue;
            }
            let _ = self.world.remove_item(p.victim, p.item, 1);
            self.kill(p.victim, Some(FormId::PLAYER_REF));
            self.count_stat(crate::misc_stats::PANTS_EXPLODED, 1.0);
            out.push(Blast { victim: p.victim, item: p.item });
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chance_follows_the_wiki() {
        let r = Rules::default();
        // Sneak 50 against Sneak 20, an item worth 10: 30 + 40 - 12 - 5 = 53.
        assert_eq!(r.chance(50.0, 20.0, 10.0), 53);
        // Rounded down: 0.6 × 33 = 19.8.
        assert_eq!(r.chance(33.0, 0.0, 0.0), 59);
        // Within the bounds.
        assert_eq!(r.chance(100.0, 0.0, 0.0), 85);
        assert_eq!(r.chance(0.0, 100.0, 1000.0), 5);
    }
}
