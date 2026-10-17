//! Damage, critical hits, armour, knockdown and knockback (docs/design/combat.md §5.3).
//!
//! The base is the documented Weapon Damage Formula and Critical Hit Chance Formula
//! [documented: GECK wiki, owner exports 2026-10-09, summarised in combat.md §4.2]; the
//! steps no page documents are NVForever's own and marked [design].
//!
//! Every function is pure: the caller passes the random rolls, so a replay with the same
//! rolls gives the same numbers.

use serde::Serialize;

use crate::data::{AmmoEffect, WeaponStats};
use crate::tuning::{CombatTuning, Rules};

/// Perk entry points that change combat values (combat.md §4.2, "Entry Point"). Each
/// takes the value the formula has reached and returns the modified one; the default
/// leaves it unchanged. Evaluating real PERK records is a later step.
pub trait PerkHooks {
    /// "Calculate Weapon Damage" (attacker).
    fn weapon_damage(&self, v: f32) -> f32 {
        v
    }
    /// "Calculate My Critical Hit Chance" (attacker), percent.
    fn crit_chance(&self, v: f32) -> f32 {
        v
    }
    /// "Modify Enemy Critical Hit Chance" (target), percent.
    fn enemy_crit_chance(&self, v: f32) -> f32 {
        v
    }
    /// "Calculate My Critical Hit Damage" (attacker).
    fn crit_damage(&self, v: f32) -> f32 {
        v
    }
    /// "Modify Damage Threshold (attacker)".
    fn dt_attacker(&self, v: f32) -> f32 {
        v
    }
    /// "Modify Damage Threshold (defender)".
    fn dt_defender(&self, v: f32) -> f32 {
        v
    }
    /// "Calculate Gun Spread" (attacker): the actor spread, after the formula.
    fn gun_spread(&self, v: f32) -> f32 {
        v
    }
}

/// No perks.
pub struct NoPerks;
impl PerkHooks for NoPerks {}

/// How the attack is made, from the weapon's animation type [documented: the three forms
/// of the Weapon Damage Formula].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum AttackKind {
    Gun,
    Melee,
    Unarmed,
}

impl AttackKind {
    pub fn of(w: &WeaponStats) -> AttackKind {
        match w.anim_type.as_deref() {
            Some("Hand to Hand") => AttackKind::Unarmed,
            Some("Melee (1 Hand)") | Some("Melee (2 Hand)") => AttackKind::Melee,
            _ => AttackKind::Gun,
        }
    }
}

/// The attacker's values the formulas read.
#[derive(Debug, Clone, Serialize)]
pub struct Attacker {
    /// The weapon's skill (0–100).
    pub skill: f32,
    pub strength: f32,
    pub luck: f32,
    /// Unarmed skill, for the UnarmedDamage actor value.
    pub unarmed_skill: f32,
    /// Extra MeleeDamage / UnarmedDamage / CritChance from effects and abilities (the
    /// actor values' modifiers above their derived base).
    pub melee_damage_bonus: f32,
    pub unarmed_damage_bonus: f32,
    pub crit_chance_bonus: f32,
    /// Arm condition as the formula uses it: 1 healthy … 0 crippled (two-handed: 0.5 with
    /// one arm crippled).
    pub arm_condition: f32,
    /// Attacking unseen (sneak attack).
    pub sneaking: bool,
    pub in_vats: bool,
}

impl Default for Attacker {
    fn default() -> Self {
        Attacker {
            skill: 50.0,
            strength: 5.0,
            luck: 5.0,
            unarmed_skill: 50.0,
            melee_damage_bonus: 0.0,
            unarmed_damage_bonus: 0.0,
            crit_chance_bonus: 0.0,
            arm_condition: 1.0,
            sneaking: false,
            in_vats: false,
        }
    }
}

/// The target's values.
#[derive(Debug, Clone, Default, Serialize)]
pub struct Target {
    pub dt: f32,
    /// Damage resistance in percent (most New Vegas armour has 0).
    pub dr: f32,
    /// Its race has the Child flag: no damage [documented: GECK wiki Race].
    pub child: bool,
    /// Not aware of the attacker (for the headshot critical, [design]).
    pub unaware: bool,
}

/// Where the hit landed.
#[derive(Debug, Clone, Serialize)]
pub struct HitPart {
    /// The body part's damage multiplier (BPTD), 1 when unknown.
    pub damage_mult: f32,
    pub head: bool,
}

impl Default for HitPart {
    fn default() -> Self {
        HitPart { damage_mult: 1.0, head: false }
    }
}

/// One hit's inputs besides the actors.
#[derive(Debug, Clone)]
pub struct HitInput<'a> {
    pub weapon: &'a WeaponStats,
    /// Projectiles in the shot (≥ 1; melee 1).
    pub projectiles: u32,
    /// Weapon condition, 0..=1.
    pub condition: f32,
    pub ammo_effects: &'a [&'a AmmoEffect],
    pub part: HitPart,
    /// Extra multiplier on the base: power attacks ([`crate::melee::PowerDir::damage`]), 1
    /// otherwise [design].
    pub power: f32,
}

/// The breakdown of one hit.
#[derive(Debug, Clone, Serialize)]
pub struct Damage {
    pub kind: AttackKind,
    /// After skill, condition, the weapon multiplier and the per-projectile split.
    pub base: f32,
    /// After ammo damage effects and the "Calculate Weapon Damage" perks.
    pub modified: f32,
    pub sneak_mult: f32,
    pub part_mult: f32,
    pub crit_chance: f32,
    pub critical: bool,
    pub crit_bonus: f32,
    /// Before armour.
    pub raw: f32,
    pub dt: f32,
    pub dr: f32,
    /// Health damage dealt.
    pub dealt: f32,
    /// A hit worth nothing before perks counts as a miss: no critical, no hit effects
    /// [documented: Weapon Damage Formula notes].
    pub miss: bool,
}

/// Apply ammo effects of one kind to a value (Add, Multiply, Subtract), in list order.
pub fn apply_effects(v: f32, effects: &[&AmmoEffect], kind: &str) -> f32 {
    effects.iter().filter(|e| e.kind.as_deref() == Some(kind)).fold(v, |v, e| match e.operation.as_deref() {
        Some("Add") => v + e.value,
        Some("Multiply") => v * e.value,
        Some("Subtract") => v - e.value,
        _ => v,
    })
}

/// The documented skill bonus: `fDamageSkillBase + fDamageSkillMult × skill / 100`.
pub fn skill_bonus(r: &Rules, skill: f32) -> f32 {
    r.damage_skill_base + r.damage_skill_mult * skill.clamp(0.0, 100.0) / 100.0
}

/// Condition multiplier [design, CombatTuning::condition_min_mult].
pub fn condition_mult(t: &CombatTuning, condition: f32) -> f32 {
    t.condition_min_mult + (1.0 - t.condition_min_mult) * condition.clamp(0.0, 1.0)
}

/// The MeleeDamage actor value's base: Strength × mult + offset [documented].
pub fn melee_damage_av(r: &Rules, strength: f32) -> f32 {
    r.melee_damage_strength_mult * (r.melee_damage_strength_offset + strength)
}

/// The UnarmedDamage actor value's base [documented].
pub fn unarmed_damage_av(r: &Rules, unarmed_skill: f32) -> f32 {
    r.unarmed_damage_base + r.unarmed_damage_mult * unarmed_skill
}

/// Critical chance in percent [documented: Critical Hit Chance Formula]. The weapon
/// multiplier is divided by the fire rate for automatic weapons.
pub fn crit_chance(r: &Rules, w: &WeaponStats, a: &Attacker, attacker: &dyn PerkHooks, target: &dyn PerkHooks) -> f32 {
    let av = r.crit_luck_base + r.crit_luck_mult * a.luck + a.crit_chance_bonus;
    let mult = if w.automatic && w.fire_rate > 0.0 { w.crit_mult / w.fire_rate } else { w.crit_mult };
    let c = attacker.crit_chance(av * mult) + if a.in_vats { r.vats_crit_bonus } else { 0.0 };
    target.enemy_crit_chance(c)
}

/// One hit. `crit_roll` is uniform in [0, 1).
#[allow(clippy::too_many_arguments)]
pub fn hit(
    r: &Rules,
    t: &CombatTuning,
    h: &HitInput,
    a: &Attacker,
    target: &Target,
    attacker_perks: &dyn PerkHooks,
    target_perks: &dyn PerkHooks,
    crit_roll: f32,
) -> Damage {
    let w = h.weapon;
    let kind = AttackKind::of(w);
    let n = h.projectiles.max(1) as f32;
    let weapon_base = w.base_damage as f32 * r.damage_weapon_mult;
    let cond = condition_mult(t, h.condition);
    // Crippled-arm penalty for melee and unarmed [documented].
    let arm = weapon_base * (r.damage_arm_condition_base - 1.0 + r.damage_arm_condition_mult * a.arm_condition.clamp(0.0, 1.0)) / 2.0;
    let base = match kind {
        AttackKind::Gun => weapon_base * cond * skill_bonus(r, a.skill) / n,
        AttackKind::Melee => weapon_base * cond * skill_bonus(r, a.skill) + melee_damage_av(r, a.strength) + a.melee_damage_bonus + arm,
        AttackKind::Unarmed => weapon_base * cond + unarmed_damage_av(r, a.unarmed_skill) + a.unarmed_damage_bonus + arm,
    };
    let base = (base * h.power.max(0.0)).max(0.0);
    let miss = base <= 0.0;
    let modified = attacker_perks.weapon_damage(apply_effects(base, h.ammo_effects, "Damage Mod")).max(0.0);
    let sneak_mult = match (a.sneaking, kind) {
        (false, _) => 1.0,
        (true, AttackKind::Gun) => r.sneak_mult_gun,
        (true, _) => r.sneak_mult_melee,
    };
    // Body-part multiplier after the documented steps [design].
    let part_mult = h.part.damage_mult;
    let crit_chance = crit_chance(r, w, a, attacker_perks, target_perks);
    let critical = !miss
        && w.crit_mult > 0.0
        && ((t.headshot_unaware_crit && h.part.head && target.unaware) || crit_roll * 100.0 < crit_chance);
    // Critical damage is added before armour [documented]; shared among projectiles
    // like the base damage [design].
    let crit_bonus = if critical { attacker_perks.crit_damage(w.crit_damage as f32) / n } else { 0.0 };
    let raw = modified * sneak_mult * part_mult + crit_bonus;

    let ignores_armour = w.flags.iter().any(|f| f == "Ignores Normal Weapon Resistance");
    let dt = if ignores_armour {
        0.0
    } else {
        target_perks.dt_defender(attacker_perks.dt_attacker(apply_effects(target.dt, h.ammo_effects, "DT Mod"))).max(0.0)
    };
    let dr = if ignores_armour { 0.0 } else { apply_effects(target.dr, h.ammo_effects, "DR Mod").clamp(0.0, r.max_armor_rating) };
    // DT subtracts, but never more than (1 − floor) of the hit [design]; DR scales the rest.
    let after_dt = (raw - dt).max(raw * t.dt_floor);
    let dealt = if target.child || miss { 0.0 } else { after_dt * (1.0 - dr / 100.0) };
    Damage { kind, base, modified, sneak_mult, part_mult, crit_chance, critical, crit_bonus, raw, dt, dr, dealt, miss }
}

/// Melee reach in game units: `fCombatDistance` × the actor's scale × the weapon's Reach
/// [documented: GECK wiki Weapons]; a Reach of 0 (the Fists) counts as 1 [hypothesis].
pub fn melee_reach(r: &Rules, w: &WeaponStats, scale: f32) -> f32 {
    let reach = if w.reach > 0.0 { w.reach } else { 1.0 };
    r.combat_distance * scale * reach
}

/// Health damage left of a melee or unarmed hit taken while blocking [design: the
/// blocked share is CombatTuning::block; no page documents the original's rule].
pub fn blocked(t: &CombatTuning, dealt: f32) -> f32 {
    dealt * (1.0 - t.block.clamp(0.0, 1.0))
}

/// What a block makes of a melee or unarmed blow that meets it from the front: an
/// ordinary blow is [`blocked`] (the health damage left, `false`); a power attack breaks
/// through the block, deals all of it and staggers the blocker (`true`) [design: the
/// owner's request, 2026-10-16; no page documents the original's rule].
pub fn against_block(t: &CombatTuning, dealt: f32, power: bool) -> (f32, bool) {
    if power { (dealt, true) } else { (blocked(t, dealt), false) }
}

/// A blow is blocked only coming from within this of where the blocker faces, radians
/// [design: docs/design/combat-ai.md §14.3].
pub const BLOCK_FRONT: f32 = std::f32::consts::FRAC_PI_2;

/// Whether a blow going along `dir` (attacker to blocker) meets a blocker facing
/// `facing` (horizontal) from its front: within [`BLOCK_FRONT`] of straight ahead.
pub fn from_front(facing: [f32; 2], dir: [f32; 3]) -> bool {
    let came = [-dir[0], -dir[1]];
    let (a, b) = (facing[0].hypot(facing[1]), came[0].hypot(came[1]));
    if a < 1e-6 || b < 1e-6 {
        return true;
    }
    (facing[0] * came[0] + facing[1] * came[1]) / (a * b) >= BLOCK_FRONT.cos() - 1e-6
}

/// Whether a hit makes a living actor play its hit reaction [design: the owner's rule,
/// 2026-10-11]. The hit that cripples a body part always does. A later hit on a part
/// that is already crippled does when the actor's last reaction ended at least
/// [`CombatTuning::stagger_cooldown`] seconds ago (`since_last`: negative while it
/// plays; `None`: it has not reacted yet). Any other hit does not.
pub fn staggers(t: &CombatTuning, crippled_now: bool, part_crippled: bool, since_last: Option<f32>) -> bool {
    crippled_now || (part_crippled && since_last.is_none_or(|s| s >= t.stagger_cooldown))
}

/// Knockdown chance (0..=1) for explosion damage [documented: Explosion Knockdown
/// Formula]: never below the current-health threshold, certain above the base-health
/// threshold, otherwise the formula capped at `fKnockdownChance`.
pub fn knockdown_chance(r: &Rules, damage: f32, agility: f32, current_health: f32, max_health: f32) -> f32 {
    if max_health > 0.0 && damage > max_health * r.knockdown_base_health_threshold / 100.0 {
        return 1.0;
    }
    if damage < current_health * r.knockdown_current_health_threshold / 100.0 {
        return 0.0;
    }
    let denom = r.knockdown_agility_mult * agility * 10.0 + r.knockdown_agility_base;
    let c = if denom > 0.0 { (damage * r.knockdown_damage_mult + r.knockdown_damage_base) / denom } else { r.knockdown_chance_max };
    c.clamp(0.0, r.knockdown_chance_max)
}

/// Melee knockback force in game units, applied for `knockback_time` [documented: Melee
/// Knockback Formula].
pub fn knockback_force(r: &Rules, damage: f32, agility: f32) -> f32 {
    let f = (r.knockback_agility_mult * agility * 10.0 + r.knockback_agility_base) * (damage * r.knockback_damage_mult + r.knockback_damage_base);
    f.clamp(0.0, r.knockback_force_max)
}

/// Explosion damage at `distance` from the centre: full at the centre, falling linearly
/// to `explosion_edge` of it at the radius, nothing beyond [design].
pub fn explosion_damage(t: &CombatTuning, damage: f32, radius: f32, distance: f32) -> f32 {
    if radius <= 0.0 || distance > radius {
        return 0.0;
    }
    let k = (distance / radius).clamp(0.0, 1.0);
    damage * (1.0 - k * (1.0 - t.explosion_edge))
}

