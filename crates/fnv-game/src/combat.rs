//! Combat's consequences in the game world (docs/design/combat.md §5.3 step 5, step C5):
//! health, hits, death and kill XP, through the same actor values, flags and script
//! events the scripts see (`functions::actor`). The damage itself is `fnv-combat`'s;
//! this module applies it.
//!
//! - **Level** [documented: GECK wiki Stats Tab pages]: ACBS level, or with "PC Level
//!   Mult" the player's level × the multiplier, kept within the calculated minimum and
//!   maximum (a maximum of 0 is none).
//! - **Maximum health** (the permanent part of Health, code 16, when no script set it):
//!   - player: `fAVDHealthEnduranceOffset` + Endurance × `fAVDHealthEnduranceMult` +
//!     (level − 1) × `fAVDHealthLevelMult` + its NPC_ Base Health;
//!   - NPC: Base Health + Endurance × `fAVDNPCHealthEnduranceMult` + level ×
//!     `fAVDNPCHealthLevelMult`;
//!   - creature: its DATA Health.
//!
//!   [documented: the settings' names and values are in the masters, combat-data.md §3,
//!   and the Stats Tab pages say NPC health derives from Endurance and level plus Base
//!   Health; hypothesis: the exact forms above, to be settled by capture.] A template
//!   with the "Stats" flag supplies the stats when it is an NPC_ or CREA.
//! - **A hit** lowers Health's damage part (as `DamageActorValue`), remembers the hit
//!   (location, critical, attacker, weapon) and runs the target's OnHit (the attacker as
//!   subject) and OnHitWith (the weapon) blocks [documented: block names, catalogue;
//!   hypothesis: their order].
//! - **No damage** for actors of a race with the Child flag [documented: GECK wiki Race]
//!   or creatures flagged Invulnerable [documented: xEdit flag name].
//! - **Essential** actors (base flag or `SetActorRefEssential`) are not killed: at zero
//!   health they fall unconscious and keep 1 health [documented: the flag; design: what
//!   happens instead].
//! - **Death** at zero health: as `KillActor` (dead flag, killer, GetDeadCount, OnDeath).
//! - **Kill XP** for the player [documented: GECK wiki Experience Settings]: the target's
//!   level picks its difficulty (the first `iXPLevelKill<NPC|Creature><Class>` at or above
//!   it, very hard beyond); the reward is that class's `iXPRewardKill<NPC|Opponent>
//!   <Class>`; none when others did more than `100 − iXPDeathRewardHealthThreshold` % of
//!   the damage.

use crate::functions::{actor, base_of};
use crate::game::{event, Game};
use fnv_script::vm::Value;
use fnv_sim::FormId;
use serde::Serialize;

/// Health's actor value code.
pub const HEALTH: u16 = 16;

/// What scripts ask of the engine's combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CombatRequest {
    /// Set off the placed explosives of this actor that detonate
    /// (`DetonatePlacedExplosives`).
    Detonate(FormId),
}

/// What damage did to a destructible reference (step C7).
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct DestructionChange {
    pub before: i32,
    pub after: i32,
    /// Explosions of the stages reached, in order.
    pub explosions: Vec<FormId>,
    pub destroyed: bool,
    pub disabled: bool,
    /// The last stage's replacement model and model damage stage, when it has them.
    pub model: Option<String>,
    pub damage_stage: Option<u8>,
}

/// What a hit did.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct HitResult {
    /// Health damage actually taken.
    pub dealt: f64,
    pub health: f64,
    pub max_health: f64,
    pub died: bool,
    /// An essential actor fell unconscious instead of dying.
    pub knocked_out: bool,
    /// The target takes no damage (child race, invulnerable, already dead).
    pub immune: bool,
    /// XP the player gained for the kill.
    pub xp: f64,
}

/// A hit's circumstances.
#[derive(Debug, Clone, Copy, Default)]
pub struct HitInfo {
    pub attacker: Option<FormId>,
    pub weapon: Option<FormId>,
    /// GetHitLocation's index (−1 none, 0 torso, …).
    pub location: i32,
    pub critical: bool,
}

/// Settings with their documented defaults (GECK wiki Experience Settings).
fn setting(g: &Game, name: &str, default: f64) -> f64 {
    g.world.forms.setting_f32(name).map_or(default, |v| v as f64)
}

impl Game<'_> {
    /// The actor definition supplying `r`'s stats: its base, or the base's template
    /// when it has the "Stats" template flag (followed a few levels).
    fn stats_def(&self, r: FormId) -> Option<&crate::data::ActorDef> {
        let mut def = self.data.actors.get(&base_of(self, r)?)?;
        for _ in 0..8 {
            match def.template.filter(|_| def.template_stats).and_then(|t| self.data.actors.get(&t)) {
                Some(d) => def = d,
                None => break,
            }
        }
        Some(def)
    }

    /// The player's level.
    pub fn player_level(&self) -> f64 {
        self.player.level.unwrap_or(self.world.config().player_level as i32) as f64
    }

    /// An actor's level (see the module notes); SetLevel's value wins.
    pub fn actor_level(&self, r: FormId) -> f64 {
        if r == FormId::PLAYER_REF {
            return self.player_level();
        }
        if let Some(l) = self.extra.get(&r).and_then(|e| e.level) {
            return l as f64;
        }
        let Some(d) = self.stats_def(r) else { return 1.0 };
        if d.pc_level_mult {
            let l = (self.player_level() * d.level_raw as f64 / 1000.0).round().max(d.calc_min as f64);
            if d.calc_max > 0 {
                l.min(d.calc_max as f64)
            } else {
                l
            }
            .max(1.0)
        } else {
            (d.level_raw as f64).max(1.0)
        }
    }

    /// Maximum health before any script changed it (see the module notes).
    pub fn max_health(&mut self, r: FormId) -> f64 {
        let end = self.world.actor_value(r, "Endurance").map(|v| v as f64).ok();
        let level = self.actor_level(r);
        let Some(d) = self.stats_def(r).cloned() else { return 100.0 };
        let end = end.unwrap_or(d.endurance as f64);
        if r == FormId::PLAYER_REF {
            return setting(self, "fAVDHealthEnduranceOffset", 0.0)
                + end * setting(self, "fAVDHealthEnduranceMult", 20.0)
                + (level - 1.0).max(0.0) * setting(self, "fAVDHealthLevelMult", 5.0)
                + d.base_health as f64;
        }
        if &d.sig == b"CREA" {
            return (d.base_health as f64).max(1.0);
        }
        (d.base_health as f64 + end * setting(self, "fAVDNPCHealthEnduranceMult", 5.0) + level * setting(self, "fAVDNPCHealthLevelMult", 5.0)).max(1.0)
    }

    pub fn is_dead(&self, r: FormId) -> bool {
        self.extra.get(&r).is_some_and(|e| e.dead)
    }

    fn is_essential(&self, r: FormId) -> bool {
        self.extra.get(&r).and_then(|e| e.essential).unwrap_or_else(|| base_of(self, r).and_then(|b| self.data.actors.get(&b)).is_some_and(|d| d.essential))
    }

    /// Whether the actor takes no damage: its race has the Child flag, or the creature is
    /// Invulnerable.
    pub fn is_immune(&self, r: FormId) -> bool {
        let Some(d) = base_of(self, r).and_then(|b| self.data.actors.get(&b)) else { return false };
        d.invulnerable || d.race.is_some_and(|race| self.data.child_races.contains(&race))
    }

    /// The actor dies: dead flag, killer, GetDeadCount, OnDeath (KillActor's effect).
    /// False when it was already dead.
    pub fn kill(&mut self, r: FormId, killer: Option<FormId>) -> bool {
        if self.is_dead(r) {
            return false;
        }
        let e = self.extra.entry(r).or_default();
        e.dead = true;
        e.killer = killer;
        if let Some(b) = base_of(self, r) {
            *self.player.dead_count.entry(b).or_default() += 1;
        }
        self.ref_event(r, event::ON_DEATH, killer.map(|k| Value::Ref(k.0)), killer);
        true
    }

    /// Apply `damage` health damage to `target` (see the module notes).
    pub fn hit(&mut self, target: FormId, damage: f64, info: HitInfo) -> HitResult {
        let max_health = actor::actor_value_base(self, target, HEALTH);
        if self.is_dead(target) || self.is_immune(target) {
            let health = actor::actor_value(self, target, HEALTH).unwrap_or(0.0);
            return HitResult { health, max_health, immune: true, ..HitResult::default() };
        }
        let before = actor::actor_value(self, target, HEALTH).unwrap_or(max_health);
        let essential = self.is_essential(target);
        let floor = if essential { 1.0 } else { 0.0 };
        let dealt = damage.max(0.0).min((before - floor).max(0.0));
        actor::add_damage(self, target, HEALTH, dealt);
        {
            let e = self.extra.entry(target).or_default();
            e.last_hit = Some(crate::game::LastHit { attacker: info.attacker, weapon: info.weapon, location: info.location, critical: info.critical });
            if Some(FormId::PLAYER_REF) == info.attacker {
                e.player_damage += dealt;
            }
            e.total_damage += dealt;
        }
        self.ref_event(target, event::ON_HIT, info.attacker.map(|a| Value::Ref(a.0)), info.attacker);
        if let Some(w) = info.weapon {
            self.ref_event(target, event::ON_HIT_WITH, Some(Value::Ref(w.0)), info.attacker);
        }
        let health = actor::actor_value(self, target, HEALTH).unwrap_or(before - dealt);
        let mut out = HitResult { dealt, health, max_health, ..HitResult::default() };
        if health <= floor {
            if essential {
                self.extra.entry(target).or_default().unconscious = true;
                out.knocked_out = true;
            } else {
                out.died = self.kill(target, info.attacker);
                if out.died && info.attacker == Some(FormId::PLAYER_REF) {
                    out.xp = self.kill_xp(target);
                    self.player.xp += out.xp;
                }
            }
        }
        out
    }

    /// Whether the reference's base has destruction data.
    pub fn is_destructible(&self, r: FormId) -> bool {
        base_of(self, r).is_some_and(|b| self.data.destructibles.contains_key(&b))
    }

    /// Object damage on a destructible reference (as `DamageObject`, step C7)
    /// [documented: GECK wiki Destructible Object Stage]:
    /// - damage adds up to the object's health; each stage applies once its health
    ///   percentage is reached;
    /// - a stage with Cap Damage stops one hit from going past it;
    /// - Destroy marks the object destroyed (it takes no more damage but its own
    ///   self-damage), Disable disables it;
    /// - the stage's self-damage per second keeps damaging it ([`Game::destruction_tick`]);
    /// - each stage reached runs OnDestructionStageChange [hypothesis: once per change]
    ///   and its explosion is returned for the engine to set off.
    ///
    /// `None` when the reference has no destruction data or nothing changed hands.
    pub fn damage_object(&mut self, r: FormId, amount: f64) -> Option<DestructionChange> {
        let def = base_of(self, r).and_then(|b| self.data.destructibles.get(&b)).cloned()?;
        let e = self.extra.get(&r);
        if e.is_some_and(|e| e.destroyed) && e.and_then(|e| e.burning).is_none() {
            return None;
        }
        let before = crate::functions::objects_destruction_stage(self, r);
        let health = def.health.max(0) as f64;
        let had = e.and_then(|e| e.damage).unwrap_or(0.0);
        let mut total = had + amount.max(0.0);
        if health > 0.0 {
            total = total.min(health);
            // Cap Damage: a stage newly reached stops the hit at its threshold.
            let mut by_pct: Vec<&crate::data::DestructionStage> = def.stages.iter().collect();
            by_pct.sort_by_key(|s| std::cmp::Reverse(s.health_percent));
            let pct_after = 100.0 * (health - total) / health;
            let pct_before = 100.0 * (health - had) / health;
            for s in by_pct {
                let p = s.health_percent as f64;
                if s.flags & crate::data::DestructionStage::CAP_DAMAGE != 0 && pct_after <= p && pct_before > p {
                    total = total.min(health * (1.0 - p / 100.0));
                    break;
                }
            }
        }
        self.extra.entry(r).or_default().damage = Some(total);
        let after = crate::functions::objects_destruction_stage(self, r);
        let mut out = DestructionChange { before, after, ..DestructionChange::default() };
        if after == before {
            return Some(out);
        }
        for s in def.stages.iter().filter(|s| s.index as i32 > before && s.index as i32 <= after) {
            if s.flags & crate::data::DestructionStage::DESTROY != 0 {
                self.extra.entry(r).or_default().destroyed = true;
                out.destroyed = true;
            }
            if s.flags & crate::data::DestructionStage::DISABLE != 0 {
                crate::functions::objects_disable(self, r);
                out.disabled = true;
            }
            out.explosions.extend(s.explosion);
            if s.model.is_some() {
                out.model = s.model.clone();
            }
            if s.damage_stage > 0 {
                out.damage_stage = Some(s.damage_stage);
            }
            let dps = s.self_damage_per_second;
            self.extra.entry(r).or_default().burning = (dps > 0).then_some(dps as f64);
        }
        if health > 0.0 && total >= health {
            self.extra.entry(r).or_default().burning = None;
        }
        self.ref_event(r, event::ON_DESTRUCTION_STAGE_CHANGE, None, None);
        Some(out)
    }

    /// Advance burning objects (a stage's self-damage per second) by `dt` seconds; the
    /// changes that reached new stages, with their references.
    pub fn destruction_tick(&mut self, dt: f64) -> Vec<(FormId, DestructionChange)> {
        let burning: Vec<(FormId, f64)> = self.extra.iter().filter_map(|(r, e)| e.burning.map(|d| (*r, d))).collect();
        let mut out = Vec::new();
        for (r, dps) in burning {
            if let Some(c) = self.damage_object(r, dps * dt) {
                if c.after != c.before {
                    out.push((r, c));
                }
            }
        }
        out
    }

    /// Run the OnFire blocks of the weapon `actor` fired (its object script)
    /// [documented: block name; observed: the detonator's script, `DetonatorOnFire`, is
    /// an OnFire block].
    pub fn weapon_fired(&mut self, actor: FormId, weapon: FormId) -> bool {
        self.item_event(actor, weapon, event::ON_FIRE, None)
    }

    /// The XP the player earns for killing `target` (see the module notes).
    pub fn kill_xp(&self, target: FormId) -> f64 {
        let e = self.extra.get(&target);
        let (by_player, total) = e.map_or((0.0, 0.0), |e| (e.player_damage, e.total_damage));
        if total > 0.0 && by_player / total * 100.0 < setting(self, "iXPDeathRewardHealthThreshold", 40.0) {
            return 0.0;
        }
        let creature = base_of(self, target).and_then(|b| self.data.actors.get(&b)).is_some_and(|d| &d.sig == b"CREA");
        let level = self.actor_level(target);
        let (who, reward) = if creature { ("Creature", "Opponent") } else { ("NPC", "NPC") };
        let classes: [(&str, f64, f64); 5] = if creature {
            [("VeryEasy", 1.0, 1.0), ("Easy", 3.0, 5.0), ("Average", 6.0, 10.0), ("Hard", 9.0, 25.0), ("VeryHard", 12.0, 50.0)]
        } else {
            [("VeryEasy", 0.0, 0.0), ("Easy", 1.0, 10.0), ("Average", 7.0, 20.0), ("Hard", 10.0, 30.0), ("VeryHard", 13.0, 50.0)]
        };
        let class = classes.iter().find(|(c, lvl, _)| level <= setting(self, &format!("iXPLevelKill{who}{c}"), *lvl)).unwrap_or(&classes[4]);
        setting(self, &format!("iXPRewardKill{reward}{}", class.0), class.2)
    }
}
