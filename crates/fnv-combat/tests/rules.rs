//! Step C2 rules (docs/spec-notes/combat-rules.md): damage, spread, the attack cycle and
//! projectiles, on hand-made stats and a toy world. The expected numbers follow from the
//! documented formulas and our CombatTuning defaults.

use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::damage::{self, Attacker, HitInput, HitPart, NoPerks, Target};
use fnv_combat::data::{AmmoEffect, ExplosionStats, ProjectileStats, WeaponStats};
use fnv_combat::projectile::{blast, swing, ActorId, CombatWorld, ProjEvent, Projectile, WorldHit};
use fnv_combat::spread::{self, Kick, Stance};
use fnv_combat::vec::{self, V3};
use fnv_combat::{CombatTuning, Rules};
use fnv_sim::FormId;

fn gun(base: i32) -> WeaponStats {
    WeaponStats {
        base_damage: base,
        anim_type: Some("Pistol - Ballistic (1 Hand)".into()),
        crit_damage: 10,
        crit_mult: 1.0,
        clip_size: 2,
        ammo_use: 1,
        attack_mult: 1.0,
        anim_mult: 1.0,
        attack_shots_per_sec: 2.0,
        min_spread: 1.0,
        ..WeaponStats::default()
    }
}

fn effect(kind: &str, op: &str, value: f32) -> AmmoEffect {
    AmmoEffect { kind: Some(kind.into()), operation: Some(op.into()), value, ..AmmoEffect::default() }
}

fn skilled() -> Attacker {
    Attacker { skill: 100.0, ..Attacker::default() }
}

/// One hit with no perks and a crit roll that never crits unless forced.
fn hit(w: &WeaponStats, projectiles: u32, a: &Attacker, t: &Target, effects: &[&AmmoEffect], part: HitPart, roll: f32) -> damage::Damage {
    let h = HitInput { weapon: w, projectiles, condition: 1.0, ammo_effects: effects, part };
    damage::hit(&Rules::defaults(), &CombatTuning::default(), &h, a, t, &NoPerks, &NoPerks, roll)
}

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn gun_damage_skill_and_dt() {
    let w = gun(20);
    let d = hit(&w, 1, &skilled(), &Target { dt: 5.0, ..Target::default() }, &[], HitPart::default(), 0.99);
    assert!(close(d.base, 20.0) && close(d.dealt, 15.0) && !d.critical, "{d:?}");
    // Skill 0: half damage [documented skill bonus 0.5 + 0.5 × skill / 100].
    let d = hit(&w, 1, &Attacker { skill: 0.0, ..Attacker::default() }, &Target::default(), &[], HitPart::default(), 0.99);
    assert!(close(d.dealt, 10.0));
    // DT never takes more than 80 % (dt_floor 0.2).
    let d = hit(&w, 1, &skilled(), &Target { dt: 30.0, ..Target::default() }, &[], HitPart::default(), 0.99);
    assert!(close(d.dealt, 4.0));
    // DR scales what DT leaves.
    let d = hit(&w, 1, &skilled(), &Target { dt: 5.0, dr: 50.0, ..Target::default() }, &[], HitPart::default(), 0.99);
    assert!(close(d.dealt, 7.5));
}

#[test]
fn shotgun_damage_is_shared() {
    let d = hit(&gun(100), 14, &skilled(), &Target::default(), &[], HitPart::default(), 0.99);
    assert!(close(d.dealt, 100.0 / 14.0));
}

#[test]
fn criticals() {
    let w = gun(20);
    // Roll under Luck 5 % → critical, its damage added before DT.
    let d = hit(&w, 1, &skilled(), &Target { dt: 5.0, ..Target::default() }, &[], HitPart::default(), 0.0);
    assert!(d.critical && close(d.dealt, 25.0), "{d:?}");
    // Automatic: the weapon multiplier is divided by the fire rate.
    let auto = WeaponStats { automatic: true, fire_rate: 8.0, ..gun(20) };
    let c = damage::crit_chance(&Rules::defaults(), &auto, &Attacker { luck: 10.0, ..Attacker::default() }, &NoPerks, &NoPerks);
    assert!(close(c, 1.25));
    // A headshot on an unaware target always crits [design].
    let d = hit(&w, 1, &skilled(), &Target { unaware: true, ..Target::default() }, &[], HitPart { damage_mult: 2.0, head: true }, 0.99);
    assert!(d.critical && close(d.dealt, 50.0));
    // No critical without a crit multiplier.
    let none = WeaponStats { crit_mult: 0.0, ..gun(20) };
    assert!(!hit(&none, 1, &skilled(), &Target { unaware: true, ..Target::default() }, &[], HitPart { damage_mult: 1.0, head: true }, 0.0).critical);
}

#[test]
fn sneak_melee_unarmed() {
    let sneak = Attacker { sneaking: true, ..skilled() };
    assert!(close(hit(&gun(20), 1, &sneak, &Target::default(), &[], HitPart::default(), 0.99).dealt, 40.0));
    let blade = WeaponStats { anim_type: Some("Melee (1 Hand)".into()), ..gun(10) };
    // 10 × skill 1.0 + MeleeDamage 0.5 × Strength 10.
    let strong = Attacker { strength: 10.0, ..skilled() };
    assert!(close(hit(&blade, 1, &strong, &Target::default(), &[], HitPart::default(), 0.99).dealt, 15.0));
    // Crippled arm: 10 × (0.2 − 1 + 0) / 2 = −4.
    let crippled = Attacker { arm_condition: 0.0, ..strong.clone() };
    assert!(close(hit(&blade, 1, &crippled, &Target::default(), &[], HitPart::default(), 0.99).dealt, 11.0));
    // Melee sneak ×5.
    let sneaky = Attacker { sneaking: true, ..strong };
    assert!(close(hit(&blade, 1, &sneaky, &Target::default(), &[], HitPart::default(), 0.99).dealt, 75.0));
    // Unarmed: 2 + UnarmedDamage (0.5 + 0.05 × 100), no skill bonus.
    let fists = WeaponStats { anim_type: Some("Hand to Hand".into()), ..gun(2) };
    let boxer = Attacker { unarmed_skill: 100.0, ..Attacker::default() };
    assert!(close(hit(&fists, 1, &boxer, &Target::default(), &[], HitPart::default(), 0.99).dealt, 7.5));
}

#[test]
fn ammo_effects_child_miss_armour_flag() {
    let w = gun(20);
    let hp = effect("Damage Mod", "Multiply", 1.2);
    let ap = effect("DT Mod", "Subtract", 2.0);
    let d = hit(&w, 1, &skilled(), &Target { dt: 5.0, ..Target::default() }, &[&hp, &ap], HitPart::default(), 0.99);
    assert!(close(d.dt, 3.0) && close(d.dealt, 21.0), "{d:?}");
    assert_eq!(hit(&w, 1, &skilled(), &Target { child: true, ..Target::default() }, &[], HitPart::default(), 0.0).dealt, 0.0);
    let zero = hit(&gun(0), 1, &skilled(), &Target::default(), &[], HitPart::default(), 0.0);
    assert!(zero.miss && !zero.critical && zero.dealt == 0.0);
    let piercing = WeaponStats { flags: vec!["Ignores Normal Weapon Resistance".into()], ..gun(20) };
    assert!(close(hit(&piercing, 1, &skilled(), &Target { dt: 15.0, ..Target::default() }, &[], HitPart::default(), 0.99).dealt, 20.0));
    // Condition 0 halves the damage [design].
    let h = HitInput { weapon: &w, projectiles: 1, condition: 0.0, ammo_effects: &[], part: HitPart::default() };
    let d = damage::hit(&Rules::defaults(), &CombatTuning::default(), &h, &skilled(), &Target::default(), &NoPerks, &NoPerks, 0.99);
    assert!(close(d.dealt, 10.0));
}

#[test]
fn knockdown_knockback_explosion_falloff() {
    let r = Rules::defaults();
    assert_eq!(damage::knockdown_chance(&r, 20.0, 5.0, 60.0, 100.0), 0.0);
    assert_eq!(damage::knockdown_chance(&r, 80.0, 5.0, 100.0, 100.0), 1.0);
    assert!(close(damage::knockdown_chance(&r, 40.0, 5.0, 60.0, 100.0), 0.24));
    assert!(close(damage::knockback_force(&r, 10.0, 5.0), 90.0));
    let t = CombatTuning::default();
    assert!(close(damage::explosion_damage(&t, 100.0, 400.0, 0.0), 100.0));
    assert!(close(damage::explosion_damage(&t, 100.0, 400.0, 200.0), 62.5));
    assert!(close(damage::explosion_damage(&t, 100.0, 400.0, 400.0), 25.0));
    assert_eq!(damage::explosion_damage(&t, 100.0, 400.0, 401.0), 0.0);
}

#[test]
fn spread_formula_cone_bloom_recoil() {
    let r = Rules::defaults();
    let t = CombatTuning::default();
    let w = gun(20);
    let best = Stance { aiming: true, sneaking: true, skill: 100.0, strength: 10.0, aiming_move_divisor: 1.0, ..Stance::default() };
    assert!(close(spread::actor_spread(&r, &t, &w, &best), r.min_gun_spread));
    let hip = Stance { skill: 0.0, strength: 10.0, aiming_move_divisor: 1.0, ..Stance::default() };
    assert!(close(spread::actor_spread(&r, &t, &w, &hip), 0.3));
    let running = Stance { moving: true, running: true, ..hip.clone() };
    assert!(close(spread::actor_spread(&r, &t, &w, &running), 0.5));
    assert!(close(spread::deviation_deg(1.0, 0.0), 5.955));
    assert_eq!(spread::spread_dir([1.0, 0.0, 0.0], 0.0, 0.5, 0.5), [1.0, 0.0, 0.0]);
    for k in 0..20 {
        let d = spread::spread_dir([1.0, 0.0, 0.0], 10.0, k as f32 / 20.0, 0.37);
        assert!(d[0].acos().to_degrees() <= 10.0 + 1e-3);
    }
    let mut kick = Kick::default();
    let up = kick.shot(&t, 1.0, 20.0);
    assert!(up > 0.0 && kick.bloom > 0.0);
    kick.recover(&t, 5.0);
    assert_eq!((kick.bloom, kick.recoil_deg), (0.0, 0.0));
}

fn run(c: &mut Cycle, t: &CombatTuning, ticks: usize, dt: f32, trigger: impl Fn(usize) -> bool, carried: u32) -> Vec<Event> {
    (0..ticks).flat_map(|k| c.step(t, Input { trigger: trigger(k), reload: false, dt, carried })).collect()
}

fn shots(ev: &[Event]) -> usize {
    ev.iter().filter(|e| matches!(e, Event::Fired { .. })).count()
}

#[test]
fn semi_automatic_cycle_and_reload() {
    let t = CombatTuning::default();
    let w = gun(20);
    let timing = Timing::of(&w, &ClipTiming::default(), &t);
    assert!(close(timing.interval, 0.5));
    // Held: one shot only.
    let mut c = Cycle::new(&w, timing, false, 2);
    assert_eq!(shots(&run(&mut c, &t, 40, 0.05, |_| true, 10)), 1);
    // Pulled every 0.6 s for 6 s: two shots, the empty pull starts a 2 s reload (pulls
    // during it are ignored), two more shots, another reload.
    let mut c = Cycle::new(&w, timing, false, 2);
    let ev = run(&mut c, &t, 120, 0.05, |k| k % 12 == 0, 10);
    let fired = Event::Fired { rounds: 1 };
    assert_eq!(&ev[..5], &[fired.clone(), fired.clone(), Event::ReloadStarted, Event::Reloaded { taken: 2 }, fired], "{ev:?}");
    assert_eq!(shots(&ev), 4);
    // Nothing carried: one dry click per pull.
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev = run(&mut c, &t, 20, 0.05, |_| true, 0);
    assert_eq!(ev, vec![Event::DryFire]);
}

#[test]
fn automatic_fire_rate_and_regen() {
    let t = CombatTuning::default();
    let w = WeaponStats { automatic: true, fire_rate: 10.0, clip_size: 30, ..gun(10) };
    let mut c = Cycle::new(&w, Timing::of(&w, &ClipTiming::default(), &t), false, 30);
    let n = shots(&run(&mut c, &t, 60, 1.0 / 60.0, |_| true, 0));
    assert!((10..=11).contains(&n), "{n}");
    // Regenerating weapon: +1 per regen_rate seconds, never reloads by hand.
    let r = WeaponStats { regen_rate: 1.0, clip_size: 5, ..gun(10) };
    let mut c = Cycle::new(&r, Timing::of(&r, &ClipTiming::default(), &t), false, 3);
    let ev: Vec<Event> = (0..150).flat_map(|_| c.step(&t, Input { trigger: false, reload: true, dt: 0.02, carried: 99 })).collect();
    assert_eq!(c.magazine, 5);
    assert!(!ev.contains(&Event::ReloadStarted));
    // A melee weapon swings without ammo.
    let blade = WeaponStats { anim_type: Some("Melee (1 Hand)".into()), clip_size: 0, ..gun(10) };
    let mut c = Cycle::new(&blade, Timing::of(&blade, &ClipTiming::default(), &t), true, 0);
    assert_eq!(shots(&run(&mut c, &t, 10, 0.05, |k| k == 0, 0)), 1);
}

/// Floor at z = 0, spherical actors, and an optional small-transparent fence at x = 500.
struct Toy {
    actors: Vec<(ActorId, V3, f32)>,
    fence: bool,
    wall: Option<f32>,
}

impl CombatWorld for Toy {
    fn ray(&self, from: V3, to: V3, ignore: &[ActorId]) -> Option<WorldHit> {
        let d = vec::sub(to, from);
        let len = vec::len(d);
        let dir = vec::normalize(d);
        let mut best: Option<WorldHit> = None;
        let mut take = |h: WorldHit| {
            if h.distance >= 0.0 && h.distance <= len && best.as_ref().is_none_or(|b| h.distance < b.distance) {
                best = Some(h);
            }
        };
        if dir[2] < 0.0 && from[2] >= 0.0 {
            let s = -from[2] / dir[2];
            take(WorldHit { point: vec::add(from, vec::scale(dir, s)), normal: [0.0, 0.0, 1.0], distance: s, actor: None, small_transparent: false, material: None });
        }
        for (x, small) in [(500.0, true)].into_iter().filter(|_| self.fence).chain(self.wall.map(|w| (w, false))) {
            if dir[0] > 0.0 && from[0] < x {
                let s = (x - from[0]) / dir[0];
                take(WorldHit { point: vec::add(from, vec::scale(dir, s)), normal: [-1.0, 0.0, 0.0], distance: s, actor: None, small_transparent: small, material: None });
            }
        }
        for &(id, c, r) in &self.actors {
            if ignore.contains(&id) {
                continue;
            }
            let oc = vec::sub(from, c);
            let b = vec::dot(oc, dir);
            let q = vec::dot(oc, oc) - r * r;
            let disc = b * b - q;
            if disc >= 0.0 {
                let s = -b - disc.sqrt();
                let p = vec::add(from, vec::scale(dir, s));
                take(WorldHit { point: p, normal: vec::normalize(vec::sub(p, c)), distance: s, actor: Some(id), small_transparent: false, material: None });
            }
        }
        best
    }

    fn actors_near(&self, point: V3, radius: f32) -> Vec<(ActorId, V3)> {
        self.actors.iter().filter(|(_, c, r)| vec::len(vec::sub(*c, point)) <= radius + r).map(|&(id, c, _)| (id, c)).collect()
    }
}

fn proj(kind: &str, flags: &[&str], speed: f32, gravity: f32) -> ProjectileStats {
    ProjectileStats { kind: Some(kind.into()), flags: flags.iter().map(|s| s.to_string()).collect(), speed, gravity, range: 10_000.0, ..ProjectileStats::default() }
}

fn fly(p: &mut Projectile, w: &Toy, seconds: f32) -> Vec<ProjEvent> {
    let dt = 1.0 / 60.0;
    let mut out = Vec::new();
    for _ in 0..(seconds / dt) as usize {
        out.extend(p.step(w, dt));
        if !p.alive {
            break;
        }
    }
    out
}

const G: f32 = 9.81 * 70.0;
const BOOM: FormId = FormId(0x99);

#[test]
fn hitscan_beam_and_missile_drop() {
    let t = CombatTuning::default();
    let world = Toy { actors: vec![(7, [1000.0, 0.0, 100.0], 30.0)], fence: false, wall: None };
    let mut p = Projectile::spawn(&proj("Missile", &["Hitscan"], 20000.0, 1.0), None, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    match &p.step(&world, 1.0 / 60.0)[..] {
        [ProjEvent::Hit { actor: Some(7), delay, .. }] => assert!(close(*delay, 970.0 / 20000.0)),
        e => panic!("{e:?}"),
    }
    let mut b = Projectile::spawn(&proj("Beam", &[], 100.0, 0.0), None, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    assert!(matches!(&b.step(&world, 1.0 / 60.0)[..], [ProjEvent::Hit { actor: Some(7), delay, .. }] if *delay == 0.0));
    // A slow missile aimed level under gravity falls short of the actor: it hits the floor
    // after about sqrt(2 × 100 / G) = 0.54 s, near x = 540.
    let empty = Toy { actors: vec![], fence: false, wall: None };
    let mut m = Projectile::spawn(&proj("Missile", &[], 1000.0, 1.0), Some(BOOM), [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    let ev = fly(&mut m, &empty, 2.0);
    match &ev[..] {
        [ProjEvent::Hit { actor: None, point, .. }, ProjEvent::Explode { explosion, .. }] => {
            assert!((point[0] - 540.0).abs() < 20.0, "{point:?}");
            assert_eq!(*explosion, BOOM);
        }
        e => panic!("{e:?}"),
    }
}

#[test]
fn flame_fence_range() {
    let t = CombatTuning::default();
    let two = Toy { actors: vec![(7, [200.0, 0.0, 100.0], 30.0), (8, [400.0, 0.0, 100.0], 30.0)], fence: false, wall: Some(800.0) };
    let mut f = Projectile::spawn(&proj("Flame", &[], 1500.0, 0.0), None, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    let hits: Vec<Option<ActorId>> = fly(&mut f, &two, 2.0).iter().filter_map(|e| if let ProjEvent::Hit { actor, .. } = e { Some(*actor) } else { None }).collect();
    assert_eq!(hits, vec![Some(7), Some(8), None]);
    // The fence stops a bullet unless it passes through small transparent objects.
    let fence = Toy { actors: vec![(7, [1000.0, 0.0, 100.0], 30.0)], fence: true, wall: None };
    let mut stopped = Projectile::spawn(&proj("Missile", &["Hitscan"], 20000.0, 0.0), None, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    assert!(matches!(&stopped.step(&fence, 0.016)[..], [ProjEvent::Hit { actor: None, .. }]));
    let mut through = Projectile::spawn(&proj("Missile", &["Hitscan", "Pass Through Small Transparent"], 20000.0, 0.0), None, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    assert!(matches!(&through.step(&fence, 0.016)[..], [ProjEvent::Hit { actor: Some(7), .. }]));
    // Range ends a missile with nothing to hit.
    let empty = Toy { actors: vec![], fence: false, wall: None };
    let mut short = Projectile::spawn(&ProjectileStats { range: 300.0, ..proj("Missile", &[], 1000.0, 0.0) }, None, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    match &fly(&mut short, &empty, 2.0)[..] {
        [ProjEvent::Expired { point }] => assert!(close(point[0], 300.0)),
        e => panic!("{e:?}"),
    }
}

#[test]
fn lobbers_timer_proximity_detonation() {
    let t = CombatTuning::default();
    let empty = Toy { actors: vec![], fence: false, wall: None };
    // Impact trigger: explodes where it lands.
    let mut g = Projectile::spawn(&proj("Lobber", &["Explosion"], 600.0, 1.0), Some(BOOM), [0.0, 0.0, 100.0], [1.0, 0.0, 1.0], Some(1), G, &t);
    assert!(fly(&mut g, &empty, 5.0).iter().any(|e| matches!(e, ProjEvent::Explode { .. })));
    // Timer: bounces, comes to rest, explodes at 3 s.
    let timed = ProjectileStats { alt_trigger_timer: 3.0, ..proj("Lobber", &["Explosion", "Alt. Trigger"], 600.0, 1.0) };
    let mut g = Projectile::spawn(&timed, Some(BOOM), [0.0, 0.0, 100.0], [1.0, 0.0, 1.0], Some(1), G, &t);
    let ev = fly(&mut g, &empty, 2.9);
    assert!(ev.is_empty() && g.alive);
    assert!(matches!(&fly(&mut g, &empty, 0.2)[..], [ProjEvent::Explode { .. }]));
    // Proximity mine: armed after its timer, goes off when an actor comes near.
    let mine = ProjectileStats { alt_trigger_timer: 1.0, alt_trigger_proximity: 100.0, ..proj("Lobber", &["Explosion", "Alt. Trigger"], 0.0, 1.0) };
    let mut m = Projectile::spawn(&mine, Some(BOOM), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    assert!(fly(&mut m, &empty, 3.0).is_empty() && m.alive);
    let walker = Toy { actors: vec![(7, [50.0, 0.0, 30.0], 30.0)], fence: false, wall: None };
    assert!(matches!(&fly(&mut m, &walker, 0.1)[..], [ProjEvent::Explode { .. }]));
    // C-4: waits for detonation.
    let c4 = ProjectileStats { ..proj("Lobber", &["Explosion", "Alt. Trigger", "Detonates"], 0.0, 1.0) };
    let mut c = Projectile::spawn(&c4, Some(BOOM), [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], Some(1), G, &t);
    assert!(fly(&mut c, &empty, 100.0).is_empty() && c.alive);
    assert!(matches!(c.detonate(), Some(ProjEvent::Explode { .. })));
}

#[test]
fn melee_swing_and_blast() {
    let t = CombatTuning::default();
    let world = Toy { actors: vec![(7, [80.0, 20.0, 100.0], 25.0), (8, [-80.0, 0.0, 100.0], 25.0), (9, [300.0, 0.0, 100.0], 25.0)], fence: false, wall: None };
    let hits = swing(&world, &t, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 120.0, 1);
    assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), vec![7]);
    let e = ExplosionStats { damage: 100.0, radius: 400.0, force: 300.0, ..ExplosionStats::default() };
    let open = Toy { actors: vec![(7, [200.0, 0.0, 30.0], 30.0)], fence: false, wall: None };
    let b = blast(&open, &t, &e, [0.0, 0.0, 30.0]);
    assert_eq!(b.len(), 1);
    assert!(close(b[0].damage, 62.5) && close(b[0].push[0], 1.0));
    // A wall between the blast and the actor shields it, unless the EXPL ignores LOS.
    let walled = Toy { actors: vec![(7, [200.0, 0.0, 30.0], 30.0)], fence: false, wall: Some(100.0) };
    assert!(blast(&walled, &t, &e, [0.0, 0.0, 30.0]).is_empty());
    let ignore = ExplosionStats { flags: vec!["Ignore LOS Check".into()], ..e };
    assert_eq!(blast(&walled, &t, &ignore, [0.0, 0.0, 30.0]).len(), 1);
}
