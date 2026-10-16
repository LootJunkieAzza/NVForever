//! Combat's consequences in the game world (docs/design/combat.md §5.3 step 5, step C5):
//! health, hits, death and kill XP, through the same actor values, flags and script
//! events the scripts see (`functions::actor`). The damage itself is `fnv-combat`'s;
//! this module applies it.
//!
//! - **Level** [documented: GECK wiki Stats Tab pages]: ACBS level, or with "PC Level
//!   Mult" the player's level × the multiplier, kept within the calculated minimum and
//!   maximum (a maximum of 0 is none).
//! - **Maximum health** (the base of Health, code 16; `crate::actor_values` has the
//!   other values), from permanent Endurance:
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
//! - **Limb damage** (step C8): a hit on a body part lowers that part's condition actor
//!   value (25–31, starting at 100) by the limb damage as a share of the part's pool, the
//!   part's health percentage of the actor's maximum health: `loss = limb damage ÷
//!   (max health × percent ÷ 100) × 100`. The limb is crippled when its condition
//!   reaches 0 [documented: GECK wiki Body Part Data, crippled after that percentage of
//!   total health; hypothesis: the count-down form]. Conditions stop at 0.
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

use crate::data::MagicEffectDef;
use crate::functions::{actor, base_of};
use crate::game::{event, Game};
use fnv_script::vm::Value;
use fnv_sim::FormId;
use serde::Serialize;

pub use crate::actor_values::{HEALTH, LIMB_CONDITIONS};

/// What scripts ask of the engine's combat.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CombatRequest {
    /// Set off the placed explosives of this actor that detonate
    /// (`DetonatePlacedExplosives`).
    Detonate(FormId),
    /// `from` fires one shot of `weapon` from its centre, straight ahead (`FireWeapon`).
    Fire { from: FormId, weapon: FormId },
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
    /// The hit limb's condition after the hit, with its actor value code (step C8).
    pub limb: Option<(u16, f64)>,
    /// That hit crippled the limb (its condition reached 0 now).
    pub crippled: bool,
}

/// A hit on a body part (step C8).
#[derive(Debug, Clone, Copy, Default)]
pub struct LimbHit {
    /// The part's condition actor value code (25–31).
    pub condition: u16,
    /// The part's share of the actor's maximum health, percent (BPTD health percent).
    pub health_percent: f64,
    /// Limb damage: the hit's damage times the weapon's Limb Dmg Mult.
    pub damage: f64,
}

/// A hit's circumstances.
#[derive(Debug, Clone, Copy, Default)]
pub struct HitInfo {
    pub attacker: Option<FormId>,
    pub weapon: Option<FormId>,
    /// GetHitLocation's index (−1 none, 0 torso, …).
    pub location: i32,
    pub critical: bool,
    /// The body part's limb damage, when the hit is on a part with a condition.
    pub limb: Option<LimbHit>,
}

/// The limb-condition actor value code of a BPTD part's actor value name.
pub fn limb_condition(name: &str) -> Option<u16> {
    actor::av_by_name(name).filter(|c| LIMB_CONDITIONS.contains(c))
}

/// Settings with their documented defaults (GECK wiki Experience Settings).
pub(crate) fn setting(g: &Game, name: &str, default: f64) -> f64 {
    g.world.setting_f32(name).map_or(default, |v| v as f64)
}

impl Game<'_> {
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
        let level = self.actor_level(r);
        let Some(d) = self.stats_def(r) else { return 100.0 };
        // The permanent value, as before the other actor values were read
        // [hypothesis: whether damage to Endurance lowers the maximum].
        let end = actor::actor_value_base(self, r, crate::actor_values::ENDURANCE);
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

    /// Whether the placed actor `r` starts the game as a corpse: the actor that supplies
    /// its stats (its base, or the template while the template flag "Stats" is set) has
    /// a Base Health of 0 (NPC_), or a Health of 0 (CREA) [documented for NPCs: GECK wiki
    /// Stats Tab - NPC and Category:NPC, "If Base Health is set to 0 the actor will spawn
    /// as a corpse"; hypothesis for creatures, whose DATA Health stands where an NPC's
    /// Base Health does; docs/spec-notes/dead-on-spawn.md].
    pub fn starts_dead(&self, r: FormId) -> bool {
        let placed = self.world.placed.get(r);
        placed.is_some_and(|p| matches!(&p.sig, b"ACHR" | b"ACRE")) && self.stats_def(r).is_some_and(|d| d.zero_health)
    }

    /// Whether `r`'s base has "No Low Level Processing": its AI is not updated outside
    /// the loaded area [documented: GECK wiki Category:NPC].
    pub fn no_low_level_processing(&self, r: FormId) -> bool {
        self.base_data_def(r).is_some_and(|d| d.no_low_level)
    }

    /// Lay out the corpses of a new game: every placed actor that starts dead
    /// ([`Game::starts_dead`]) is dead from the start, without a killer, an OnDeath or a
    /// count in GetDeadCount [hypothesis: nobody killed it]. Returns how many.
    pub fn lay_out_corpses(&mut self) -> usize {
        let placed = self.world.placed;
        let dead: Vec<FormId> = placed.refs.keys().copied().filter(|&r| self.starts_dead(r)).collect();
        for &r in &dead {
            self.extra.entry(r).or_default().dead = true;
        }
        dead.len()
    }

    fn is_essential(&self, r: FormId) -> bool {
        self.extra.get(&r).and_then(|e| e.essential).unwrap_or_else(|| self.base_data_def(r).is_some_and(|d| d.essential))
    }

    /// Whether the actor takes no damage: its race has the Child flag, the creature is
    /// Invulnerable, or a script made it a ghost (SetGhost: "cannot be harmed by a weapon
    /// or spell/magic effect in any way" [documented: GECK wiki SetGhost notes]).
    pub fn is_immune(&self, r: FormId) -> bool {
        let invulnerable = self.base_data_def(r).is_some_and(|d| d.invulnerable);
        let ghost = self.extra.get(&r).is_some_and(|e| e.ghost);
        invulnerable || ghost || self.traits_def(r).and_then(|d| d.race).is_some_and(|race| self.data.child_races.contains(&race))
    }

    /// A creature's own attack: its damage (CREA DATA Damage) and its reach (RNAM Attack
    /// Reach) [documented: xEdit field names], from the actor that supplies its stats
    /// (template flag Stats) [hypothesis for the reach: which template group RNAM belongs
    /// to is not checked]. `None` for an NPC.
    pub fn creature_attack(&self, r: FormId) -> Option<(f64, Option<f64>)> {
        let d = self.stats_def(r)?;
        (&d.sig == b"CREA").then(|| (d.attack_damage.unwrap_or(0) as f64, d.attack_reach.map(f64::from)))
    }

    /// The actor dies: dead flag, killer, GetDeadCount, OnDeath (KillActor's effect).
    /// False when it was already dead.
    pub fn kill(&mut self, r: FormId, killer: Option<FormId>) -> bool {
        if self.is_dead(r) {
            return false;
        }
        // Karma and murder are judged while the victim still lives (`crate::crime`).
        if killer == Some(FormId::PLAYER_REF) {
            self.player_kills(r);
        }
        let e = self.extra.entry(r).or_default();
        e.dead = true;
        e.killer = killer;
        if let Some(b) = base_of(self, r) {
            *self.player.dead_count.entry(b).or_default() += 1;
        }
        self.ref_event(r, event::ON_DEATH, killer.map(|k| Value::Ref(k.0)), killer);
        // "Teammate state is cleared on death of the actor" [documented: GECK wiki
        // SetPlayerTeammate]; after OnDeath, which may still ask [hypothesis: the order].
        if let Some(e) = self.extra.get_mut(&r) {
            e.teammate = false;
        }
        // Its effects end, but those whose base effect has "No Death Dispel" (a flamer's
        // burning) and abilities (docs/spec-notes/hit-effects.md §4).
        let data = self.data;
        self.dispel(r, None, |e| !e.permanent && !data.magic_effects.get(&e.effect).is_some_and(|m| m.has(MagicEffectDef::NO_DEATH_DISPEL)));
        self.death_line(r, killer);
        true
    }

    /// The share (0–1) of a harmful effect's magnitude the target resists: its value of
    /// the base effect's resistance, as a percentage [documented: GECK wiki Stats Tab -
    /// Creatures, Poison Resist; hypothesis: the same for every resistance, Fire
    /// Resistance among them] (docs/spec-notes/hit-effects.md §3).
    pub fn resisted(&mut self, target: FormId, m: Option<&MagicEffectDef>) -> f64 {
        let Some(m) = m.filter(|m| m.has(MagicEffectDef::HOSTILE) || m.has(MagicEffectDef::DETRIMENTAL)) else { return 0.0 };
        let Some(code) = m.resistance else { return 0.0 };
        (actor::actor_value(self, target, code).unwrap_or(0.0) / 100.0).clamp(0.0, 1.0)
    }

    /// The magic item a hit applies with its effects: a weapon's object effect, a
    /// blast's, an actor's unarmed attack effect (docs/spec-notes/hit-effects.md §1). A
    /// dead or unharmable target takes none. The target's resistances cut the harmful
    /// magnitudes, a fully resisted effect does not start, and a hit with the same item
    /// while its effect lasts starts that effect's time again (§2, §3).
    pub fn apply_hit_effect(&mut self, target: FormId, item: FormId, caster: Option<FormId>) -> Vec<crate::aid::Applied> {
        if self.is_dead(target) || self.is_immune(target) {
            return Vec::new();
        }
        let mods = crate::aid::Mods { hit: true, ..crate::aid::Mods::default() };
        self.apply_magic_item_with(target, item, caster, false, &mods)
    }

    /// An actor's unarmed attack effect and the attack animation it needs (`EITM`,
    /// `EAMT`), from the actor base that supplies its actor effects (template flag "Actor
    /// Effect List") [hypothesis: the group, data.rs `template_spells`].
    pub fn unarmed_effect(&self, r: FormId) -> Option<(FormId, Option<&str>)> {
        let base = base_of(self, r)?;
        let (_, d) = self.data.template_source(base, r, |d| d.template_spells)?;
        Some((d.unarmed_effect?, d.unarmed_anim.as_deref()))
    }

    /// Health damage an effect does (a burn, a poison): as a hit's, kept at 1 for an
    /// essential actor (who falls unconscious) and killing the others at 0, the effect's
    /// caster the killer (docs/spec-notes/hit-effects.md §4).
    pub(crate) fn effect_health_damage(&mut self, target: FormId, amount: f64, caster: Option<FormId>) {
        if self.is_dead(target) || self.is_immune(target) {
            return;
        }
        let before = actor::actor_value(self, target, HEALTH).unwrap_or(0.0);
        let essential = self.is_essential(target);
        let floor = if essential { 1.0 } else { 0.0 };
        let dealt = amount.max(0.0).min((before - floor).max(0.0));
        actor::add_damage(self, target, HEALTH, dealt);
        {
            let e = self.extra.entry(target).or_default();
            if caster == Some(FormId::PLAYER_REF) {
                e.player_damage += dealt;
            }
            e.total_damage += dealt;
        }
        if before - dealt <= floor {
            if essential {
                self.extra.entry(target).or_default().unconscious = true;
            } else if self.kill(target, caster) && caster == Some(FormId::PLAYER_REF) {
                let xp = self.kill_xp(target);
                self.player.xp += xp;
            }
        }
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
        if let Some(l) = info.limb {
            let (condition, crippled) = self.damage_limb(target, l, max_health);
            out.limb = Some((l.condition, condition));
            out.crippled = crippled;
        }
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
        // What the target says (`crate::combat_lines`): Death was said by `kill`.
        if out.knocked_out {
            self.combat_line(target, info.attacker, "KOed");
        } else if !out.died {
            self.hit_lines(target, info.attacker, before, health, max_health, out.crippled);
        }
        out
    }

    /// A hit on a reference that is not an actor (a bottle, a target, a switch): its
    /// OnHit blocks run with the attacker and its OnHitWith blocks with the weapon, as
    /// for an actor; nothing else changes (object damage is [`Game::damage_object`]).
    /// Returns whether a block ran.
    ///
    /// [documented: GECK wiki Event Handling, the target of OnHit and OnHitWith is "any
    /// reference"; observed: the tutorial's bottles (MISC `VCG02Bottle`, script
    /// `VCG02TargetSCRIPT`) count the player's shots in an OnHitWith block without a
    /// parameter. Hypothesis: OnHit's subject is the attacking actor here too — that page
    /// gives a projectile reference for a ranged hit on a non-actor, and we have no
    /// projectile references.]
    pub fn hit_object(&mut self, target: FormId, info: HitInfo) -> bool {
        let mut ran = self.ref_event(target, event::ON_HIT, info.attacker.map(|a| Value::Ref(a.0)), info.attacker);
        if let Some(w) = info.weapon {
            ran |= self.ref_event(target, event::ON_HIT_WITH, Some(Value::Ref(w.0)), info.attacker);
        }
        ran
    }

    /// Tell the rules which weapon the animating side draws for `actor`
    /// (`RefExtra::held_weapon`).
    pub fn set_held_weapon(&mut self, actor: FormId, weapon: Option<FormId>) {
        if self.extra.get(&actor).and_then(|e| e.held_weapon) != weapon {
            self.extra.entry(actor).or_default().held_weapon = weapon;
        }
    }

    /// Tell the scripts whether an actor has its weapon out (IsWeaponOut).
    pub fn set_weapon_out(&mut self, actor: FormId, out: bool) {
        if self.extra.get(&actor).is_some_and(|e| e.weapon_out) != out {
            self.extra.entry(actor).or_default().weapon_out = out;
        }
    }

    /// Lower a limb's condition by a hit (see the module notes): its condition after, and
    /// whether this hit crippled it.
    pub fn damage_limb(&mut self, target: FormId, l: LimbHit, max_health: f64) -> (f64, bool) {
        let pool = max_health * l.health_percent / 100.0;
        let before = actor::actor_value(self, target, l.condition).unwrap_or(100.0);
        if pool <= 0.0 || l.damage <= 0.0 || before <= 0.0 {
            return (before, false);
        }
        let loss = (l.damage / pool * 100.0).min(before);
        actor::add_damage(self, target, l.condition, loss);
        let after = before - loss;
        (after, after <= 0.0)
    }

    /// An actor's limb conditions, codes 25–31 in order.
    pub fn limb_conditions(&mut self, r: FormId) -> [f64; 7] {
        let mut out = [100.0; 7];
        for (i, code) in LIMB_CONDITIONS.enumerate() {
            out[i] = actor::actor_value(self, r, code).unwrap_or(100.0);
        }
        out
    }

    /// Whether the actor's limb (condition actor value code) is crippled.
    pub fn is_crippled(&mut self, r: FormId, condition: u16) -> bool {
        actor::actor_value(self, r, condition).is_ok_and(|v| v <= 0.0)
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
