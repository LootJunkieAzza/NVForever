//! `fnvtool combat damage` and `fnvtool combat sim` (step C2): the rules of `fnv-combat`
//! on real weapons. Contract: docs/spec-notes/combat-rules.md §6.

use anyhow::{bail, Result};
use clap::Args;
use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::damage::{self, Attacker, HitInput, HitPart, NoPerks, Target};
use fnv_combat::data::AmmoEffect;
use fnv_combat::projectile::{blast, ActorId, CombatWorld, ProjEvent, WorldHit};
use fnv_combat::spread::{self, Kick, Stance};
use fnv_combat::vec::{self, V3};
use fnv_combat::{shot, CombatForms, CombatTuning, Rules};
use fnv_sim::FormId;
use serde_json::{json, Value};

/// The shooter and target, shared by `damage` and `sim`.
#[derive(Args, Clone)]
pub struct Fight {
    /// Ammo (Editor ID or FormID); default: the weapon's first.
    #[arg(long)]
    pub ammo: Option<String>,
    /// The weapon's skill (0–100).
    #[arg(long, default_value_t = 50.0)]
    pub skill: f32,
    #[arg(long, default_value_t = 5.0)]
    pub strength: f32,
    #[arg(long, default_value_t = 5.0)]
    pub luck: f32,
    /// Unarmed skill, for unarmed weapons.
    #[arg(long, default_value_t = 50.0)]
    pub unarmed: f32,
    /// Weapon condition, 0–1.
    #[arg(long, default_value_t = 1.0)]
    pub condition: f32,
    #[arg(long)]
    pub sneaking: bool,
    /// Target damage threshold and resistance (percent).
    #[arg(long, default_value_t = 0.0)]
    pub dt: f32,
    #[arg(long, default_value_t = 0.0)]
    pub dr: f32,
    /// Body part damage multiplier and whether it is the head.
    #[arg(long, default_value_t = 1.0)]
    pub part_mult: f32,
    #[arg(long)]
    pub head: bool,
    #[arg(long)]
    pub unaware: bool,
}

fn ammo_effects<'a>(c: &'a CombatForms, ids: &[FormId]) -> Vec<&'a AmmoEffect> {
    ids.iter().filter_map(|e| c.ammo_effects.get(e)).collect()
}

fn attacker(f: &Fight) -> Attacker {
    Attacker { skill: f.skill, strength: f.strength, luck: f.luck, unarmed_skill: f.unarmed, sneaking: f.sneaking, ..Attacker::default() }
}

fn target(f: &Fight) -> Target {
    Target { dt: f.dt, dr: f.dr, unaware: f.unaware, ..Target::default() }
}

/// One hit's breakdown, with and without a critical (fnv.combat.damage/1).
pub fn damage_report(c: &CombatForms, weapon: FormId, ammo: Option<FormId>, f: &Fight) -> Result<Value> {
    let Some(w) = c.weapons.get(&weapon) else { bail!("not a weapon") };
    let ch = c.chain(weapon, ammo).expect("weapon present");
    let rules = Rules::from_forms(c);
    let t = CombatTuning::default();
    let eff = ammo_effects(c, &ch.ammo_effects);
    let h = HitInput { weapon: w, projectiles: ch.projectiles_per_shot, condition: f.condition, ammo_effects: &eff, part: HitPart { damage_mult: f.part_mult, head: f.head }, power: 1.0 };
    let normal = damage::hit(&rules, &t, &h, &attacker(f), &target(f), &NoPerks, &NoPerks, 0.999_999);
    let crit = damage::hit(&rules, &t, &h, &attacker(f), &target(f), &NoPerks, &NoPerks, 0.0);
    let per_shot = normal.dealt * ch.projectiles_per_shot.max(1) as f32;
    let explosion = ch.explosion.and_then(|e| c.explosions.get(&e)).map(|e| json!({"damage": e.damage, "radius": e.radius, "force": e.force}));
    Ok(json!({
        "schema": "fnv.combat.damage/1",
        "weapon": w.edid, "ammo": ch.ammo.map(|a| a.to_string()), "projectiles_per_shot": ch.projectiles_per_shot,
        "hit": normal, "critical_hit": crit, "per_shot": per_shot, "explosion": explosion,
        "settings_defaulted": rules.defaulted,
    }))
}

/// Floor at z = 0 and one spherical target, for `sim`.
struct Range {
    target: V3,
    radius: f32,
}

const TARGET: ActorId = 2;
const SHOOTER: ActorId = 1;

impl CombatWorld for Range {
    fn ray(&self, from: V3, to: V3, ignore: &[ActorId]) -> Option<WorldHit> {
        let d = vec::sub(to, from);
        let len = vec::len(d);
        let dir = vec::normalize(d);
        let mut best: Option<WorldHit> = None;
        if !ignore.contains(&TARGET) {
            let oc = vec::sub(from, self.target);
            let b = vec::dot(oc, dir);
            let disc = b * b - (vec::dot(oc, oc) - self.radius * self.radius);
            if disc >= 0.0 {
                let s = -b - disc.sqrt();
                if (0.0..=len).contains(&s) {
                    let p = vec::add(from, vec::scale(dir, s));
                    best = Some(WorldHit { point: p, normal: vec::normalize(vec::sub(p, self.target)), distance: s, actor: Some(TARGET), small_transparent: false, material: None, object: None, part: None });
                }
            }
        }
        if dir[2] < 0.0 && from[2] >= 0.0 {
            let s = -from[2] / dir[2];
            if s <= len && best.as_ref().is_none_or(|b| s < b.distance) {
                best = Some(WorldHit { point: vec::add(from, vec::scale(dir, s)), normal: [0.0, 0.0, 1.0], distance: s, actor: None, small_transparent: false, material: None, object: None, part: None });
            }
        }
        best
    }

    fn actors_near(&self, point: V3, radius: f32) -> Vec<(ActorId, V3)> {
        if vec::len(vec::sub(point, self.target)) <= radius + self.radius {
            vec![(TARGET, self.target)]
        } else {
            Vec::new()
        }
    }
}


#[derive(Args, Clone)]
pub struct Shooting {
    /// Distance to the target, units.
    #[arg(long, default_value_t = 1000.0)]
    pub distance: f32,
    /// Target radius, units (a person is about 30).
    #[arg(long, default_value_t = 30.0)]
    pub radius: f32,
    /// Target health.
    #[arg(long, default_value_t = 100.0)]
    pub health: f32,
    /// Seconds to simulate.
    #[arg(long, default_value_t = 10.0)]
    pub seconds: f32,
    /// Aim down the sights.
    #[arg(long)]
    pub aim: bool,
    /// Crouched.
    #[arg(long)]
    pub crouch: bool,
    /// Rounds carried besides a full magazine.
    #[arg(long, default_value_t = 1000)]
    pub carried: u32,
    #[arg(long, default_value_t = 1)]
    pub seed: u64,
}

/// Fire at a target until it dies or time runs out (fnv.combat.sim/1). The shooter stands
/// at the origin with the eye at 100 units, aiming at the target's centre at the same
/// height; recoil moves the aim, bloom and the documented spread size the cone.
pub fn sim(c: &CombatForms, weapon: FormId, ammo: Option<FormId>, f: &Fight, r: &Shooting) -> Result<Value> {
    let Some(w) = c.weapons.get(&weapon) else { bail!("not a weapon") };
    let ch = c.chain(weapon, ammo).expect("weapon present");
    if ch.projectile.is_none() {
        bail!("{} fires no projectile (melee or tool)", w.edid.as_deref().unwrap_or("weapon"));
    }
    let rules = Rules::from_forms(c);
    let t = CombatTuning::default();
    let eff = ammo_effects(c, &ch.ammo_effects);
    let eye = [0.0, 0.0, 100.0];
    let world = Range { target: [r.distance, 0.0, 100.0], radius: r.radius };
    let g = 9.81 * 70.0;
    let mut rng = fnv_combat::rng::SplitMix(r.seed);
    let timing = Timing::of(w, &ClipTiming::default(), &t);
    let mut cycle = Cycle::new(w, timing, false, w.clip_size);
    let mut kick = Kick::default();
    let stance = Stance { aiming: r.aim, sneaking: r.crouch, skill: f.skill, strength: f.strength, aiming_move_divisor: 1.0, ..Stance::default() };
    let actor_spread = spread::actor_spread(&rules, &t, w, &stance);
    let mut carried = r.carried;
    let mut health = r.health;
    let (mut shots, mut hits, mut crits, mut reloads) = (0u32, 0u32, 0u32, 0u32);
    let mut dealt = 0.0f32;
    let mut killed_at: Option<f32> = None;
    let mut flying = Vec::new();
    let mut pending: Vec<(f32, Option<ActorId>, V3)> = Vec::new();
    let dt = 1.0 / 60.0;
    let mut time = 0.0f32;
    let mut tick = 0u64;
    while time < r.seconds && killed_at.is_none() {
        let trigger = w.automatic || tick.is_multiple_of(2);
        for e in cycle.step(&t, Input { trigger, reload: false, dt, carried }) {
            match e {
                Event::Fired { .. } => {
                    shots += 1;
                    let cone = spread::cone_deg(actor_spread, kick.bloom, w, &eff);
                    let pitch = kick.recoil_deg.to_radians();
                    let aim = [pitch.cos(), 0.0, pitch.sin()];
                    let mut rnd = || rng.next_f32();
                    flying.extend(shot::fire(c, &ch, eye, aim, cone, Some(SHOOTER), g, &t, &mut rnd));
                    kick.shot(&t, w.min_spread, w.base_damage as f32);
                }
                Event::Reloaded { taken } => {
                    carried -= taken;
                    reloads += 1;
                }
                _ => {}
            }
        }
        kick.recover(&t, dt);
        for p in &mut flying {
            for e in p.step(&world, dt) {
                match e {
                    ProjEvent::Hit { actor, point, delay, .. } => pending.push((time + delay, actor, point)),
                    ProjEvent::Explode { explosion, point } => {
                        if let Some(x) = c.explosions.get(&explosion) {
                            for b in blast(&world, &t, x, point) {
                                let d = (b.damage - f.dt).max(b.damage * t.dt_floor);
                                health -= d;
                                dealt += d;
                            }
                        }
                    }
                    ProjEvent::Expired { .. } => {}
                }
            }
        }
        flying.retain(|p| p.alive);
        let due: Vec<_> = pending.iter().filter(|p| p.0 <= time).cloned().collect();
        pending.retain(|p| p.0 > time);
        for (_, actor, _) in due {
            if actor != Some(TARGET) {
                continue;
            }
            hits += 1;
            let h = HitInput { weapon: w, projectiles: ch.projectiles_per_shot, condition: f.condition, ammo_effects: &eff, part: HitPart { damage_mult: f.part_mult, head: f.head }, power: 1.0 };
            let d = damage::hit(&rules, &t, &h, &attacker(f), &target(f), &NoPerks, &NoPerks, rng.next_f32());
            if d.critical {
                crits += 1;
            }
            health -= d.dealt;
            dealt += d.dealt;
        }
        if health <= 0.0 && killed_at.is_none() {
            killed_at = Some(time);
        }
        time += dt;
        tick += 1;
    }
    let projectiles = shots * ch.projectiles_per_shot.max(1);
    Ok(json!({
        "schema": "fnv.combat.sim/1",
        "weapon": w.edid, "ammo": ch.ammo.map(|a| a.to_string()),
        "interval": timing.interval, "actor_spread": actor_spread,
        "cone_deg_first_shot": spread::cone_deg(actor_spread, 0.0, w, &eff),
        "shots": shots, "projectiles": projectiles, "hits": hits, "hit_rate": if projectiles > 0 { hits as f32 / projectiles as f32 } else { 0.0 },
        "criticals": crits, "reloads": reloads, "damage": dealt, "time_to_kill": killed_at,
    }))
}
