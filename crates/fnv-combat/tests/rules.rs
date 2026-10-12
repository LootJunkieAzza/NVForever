//! Step C2 rules (docs/spec-notes/combat-rules.md): damage, spread, the attack cycle and
//! projectiles, on hand-made stats and a toy world. The expected numbers follow from the
//! documented formulas and our CombatTuning defaults.

use fnv_combat::attack::{ClipTiming, Cycle, Event, Input, Timing};
use fnv_combat::damage::{self, Attacker, HitInput, HitPart, NoPerks, Target};
use fnv_combat::data::{AmmoEffect, ExplosionStats, ProjectileStats, WeaponStats};
use fnv_combat::projectile::{blast, swing, swing_reaching, ActorId, CombatWorld, ProjEvent, Projectile, WorldHit};
use fnv_combat::shot::{self, Seen};
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
    let h = HitInput { weapon: w, projectiles, condition: 1.0, ammo_effects: effects, part, power: 1.0 };
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
    let h = HitInput { weapon: &w, projectiles: 1, condition: 0.0, ammo_effects: &[], part: HitPart::default(), power: 1.0 };
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
fn hit_reactions_on_crippling_and_crippled_parts() {
    let t = CombatTuning { stagger_cooldown: 3.0, ..CombatTuning::default() };
    // A hit on a healthy part does nothing, however long ago the last reaction was.
    assert!(!damage::staggers(&t, false, false, None));
    assert!(!damage::staggers(&t, false, false, Some(100.0)));
    // The crippling hit always reacts, also right after another reaction.
    assert!(damage::staggers(&t, true, true, None));
    assert!(damage::staggers(&t, true, true, Some(-0.5)));
    // Later hits on the crippled part: not while the last reaction plays, and only
    // once the cooldown after it has run out.
    assert!(damage::staggers(&t, false, true, None));
    assert!(!damage::staggers(&t, false, true, Some(-0.5)));
    assert!(!damage::staggers(&t, false, true, Some(0.1)));
    assert!(!damage::staggers(&t, false, true, Some(2.9)));
    assert!(damage::staggers(&t, false, true, Some(3.0)));
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

/// A minigun: its attack clip puts the first shot 0.967 s in (`2hhattackspin.kf`).
#[test]
fn a_spinning_weapon_fires_after_its_spin_up() {
    let t = CombatTuning::default();
    let w = WeaponStats { automatic: true, fire_rate: 20.0, clip_size: 240, ..gun(10) };
    let clips = ClipTiming { spin_up: Some(0.967), ..Default::default() };
    let timing = Timing::of(&w, &clips, &t);
    assert!(close(timing.spin_up, 0.967));
    let dt = 1.0 / 60.0;
    // Held for 2 s: the spin-up starts at once, nothing leaves for 0.967 s, then 20 a
    // second for the rest.
    let mut c = Cycle::new(&w, timing, false, 240);
    let ev = run(&mut c, &t, 120, dt, |_| true, 0);
    assert_eq!(ev[0], Event::SpinUp);
    assert_eq!(ev.iter().filter(|e| **e == Event::SpinUp).count(), 1);
    let mut c = Cycle::new(&w, timing, false, 240);
    assert_eq!(shots(&run(&mut c, &t, 57, dt, |_| true, 0)), 0, "0.95 s in");
    assert_eq!(shots(&run(&mut c, &t, 3, dt, |_| true, 0)), 1, "1 s in");
    let n = shots(&run(&mut c, &t, 60, dt, |_| true, 0));
    assert!((19..=21).contains(&n), "{n}");
    // Let go half way through the spin-up: nothing leaves, and the next pull spins up
    // again from the start.
    let mut c = Cycle::new(&w, timing, false, 240);
    let ev = run(&mut c, &t, 60, dt, |k| k < 30, 0);
    assert_eq!(ev, vec![Event::SpinUp]);
    let ev = run(&mut c, &t, 57, dt, |_| true, 0);
    assert_eq!(ev, vec![Event::SpinUp]);
    // A pause in a burst spins up again too.
    let mut c = Cycle::new(&w, timing, false, 240);
    let ev = run(&mut c, &t, 240, dt, |k| !(90..100).contains(&k), 0);
    assert_eq!(ev.iter().filter(|e| **e == Event::SpinUp).count(), 2);
    // A semi-automatic weapon does not spin up, whatever its clip; neither does an
    // automatic one whose clip has no loop.
    let semi = gun(20);
    assert_eq!(Timing::of(&semi, &clips, &t).spin_up, 0.0);
    assert_eq!(Timing::of(&w, &ClipTiming::default(), &t).spin_up, 0.0);
    let mut c = Cycle::new(&w, Timing::of(&w, &ClipTiming::default(), &t), false, 240);
    assert_eq!(shots(&run(&mut c, &t, 2, dt, |_| true, 0)), 1, "at once");
}

/// The .357 revolver's clips (`1hpreloadxstart.kf` 1.067 s; `1hpreloadx.kf` 1.767 s,
/// `Hit` at 0.633 s): it opens, loads a round per pass up to `Hit`, and closes.
#[test]
fn a_revolver_loads_round_by_round() {
    let t = CombatTuning::default();
    let w = WeaponStats { clip_size: 6, ..gun(26) };
    let clips = ClipTiming { reload_len: Some(1.767), reload_hit: Some(0.633), reload_open_len: Some(1.067), ..Default::default() };
    let timing = Timing::of(&w, &clips, &t);
    let r = timing.rounds.expect("round by round");
    assert!(close(r.open, 1.067) && close(r.round, 0.633) && close(r.close, 1.134), "{r:?}");
    // Without the opening clip, or without `Hit`: the whole magazine at once.
    assert_eq!(Timing::of(&w, &ClipTiming { reload_open_len: None, ..clips }, &t).rounds, None);
    assert_eq!(Timing::of(&w, &ClipTiming { reload_hit: None, ..clips }, &t).rounds, None);
    let dt = 0.01;
    let steps = |c: &mut Cycle, from: usize, ticks: usize, pull: &dyn Fn(usize) -> bool, carried: u32| -> Vec<Event> {
        (from..from + ticks).flat_map(|k| c.step(&t, Input { trigger: pull(k), reload: k == 0, dt, carried })).collect()
    };
    let rounds = |ev: &[Event]| ev.iter().filter(|e| **e == Event::RoundLoaded).count();

    // Four missing: open, four rounds, close: 1.067 + 4 × 0.633 + 1.134 = 4.73 s.
    let mut c = Cycle::new(&w, timing, false, 2);
    let ev = steps(&mut c, 0, 460, &|_| false, 20);
    assert_eq!(c.magazine, 6);
    assert!(!ev.iter().any(|e| matches!(e, Event::Reloaded { .. })), "closed too soon: {ev:?}");
    let ev2 = steps(&mut c, 460, 20, &|_| false, 20);
    assert_eq!(ev2, vec![Event::Reloaded { taken: 4 }]);
    assert_eq!(&ev[..3], &[Event::ReloadStarted, Event::ReloadRound, Event::RoundLoaded]);
    assert_eq!(rounds(&ev), 4);
    assert_eq!(ev.last(), Some(&Event::ReloadClosing { stopped: false }));

    // Empty, the trigger pulled 2 s in (the second round is in hand): it goes in and the
    // revolver closes (to 3.47 s); that pull fires nothing (owner, 2026-10-11).
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev = steps(&mut c, 0, 340, &|k| k == 200, 20);
    assert_eq!(rounds(&ev), 2);
    assert!(ev.contains(&Event::ReloadClosing { stopped: true }));
    assert_eq!(shots(&ev), 0, "not before it is closed");
    let ev = steps(&mut c, 340, 20, &|_| false, 20);
    assert_eq!(ev, vec![Event::Reloaded { taken: 2 }]);
    assert_eq!(c.magazine, 2);
    // Pulled again once it is closed: it fires.
    let ev = steps(&mut c, 360, 20, &|k| k == 365, 20);
    assert_eq!(shots(&ev), 1);

    // A second pull while it closes, within the grace of its end (3.47 s): the shot
    // follows the close. Earlier than the grace: dropped.
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev = steps(&mut c, 0, 360, &|k| k == 200 || k == 330, 20);
    assert_eq!(ev[ev.len() - 2..], [Event::Reloaded { taken: 2 }, Event::Fired { rounds: 1 }]);
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev = steps(&mut c, 0, 360, &|k| k == 200 || k == 300, 20);
    assert_eq!(shots(&ev), 0, "the pull 0.47 s before the close ended is dropped");

    // Pulled while it opens: the first round still goes in, then it closes, no shot.
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev = steps(&mut c, 0, 300, &|k| k == 50, 20);
    assert_eq!(rounds(&ev), 1);
    assert_eq!(shots(&ev), 0);

    // One round carried: it goes in and the revolver closes.
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev = steps(&mut c, 0, 300, &|_| false, 1);
    assert_eq!((rounds(&ev), c.magazine), (1, 1));
    assert!(ev.contains(&Event::Reloaded { taken: 1 }));

    // An empty pull starts the reload; that pull does not stop it.
    let mut c = Cycle::new(&w, timing, false, 0);
    let ev: Vec<Event> = (0..800).flat_map(|k| c.step(&t, Input { trigger: k < 3, reload: false, dt, carried: 20 })).collect();
    assert_eq!((rounds(&ev), shots(&ev)), (6, 0));
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
            take(WorldHit { point: vec::add(from, vec::scale(dir, s)), normal: [0.0, 0.0, 1.0], distance: s, actor: None, small_transparent: false, material: None, object: None, part: None });
        }
        for (x, small) in [(500.0, true)].into_iter().filter(|_| self.fence).chain(self.wall.map(|w| (w, false))) {
            if dir[0] > 0.0 && from[0] < x {
                let s = (x - from[0]) / dir[0];
                take(WorldHit { point: vec::add(from, vec::scale(dir, s)), normal: [-1.0, 0.0, 0.0], distance: s, actor: None, small_transparent: small, material: None, object: None, part: None });
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
                take(WorldHit { point: p, normal: vec::normalize(vec::sub(p, c)), distance: s, actor: Some(id), small_transparent: false, material: None, object: None, part: None });
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
    // C-4 (Explosion and Detonates, no Alt. Trigger, as WeapNVC4PlasticExplosive's
    // projectile): thrown at the floor it lands and waits for detonation.
    let c4 = proj("Lobber", &["Explosion", "Detonates"], 400.0, 1.0);
    let mut c = Projectile::spawn(&c4, Some(BOOM), [0.0, 0.0, 60.0], [1.0, 0.0, -0.5], Some(1), G, &t);
    assert!(fly(&mut c, &empty, 100.0).is_empty() && c.alive);
    assert!(matches!(c.detonate(), Some(ProjEvent::Explode { .. })));
}

#[test]
fn melee_swing_and_blast() {
    let t = CombatTuning::default();
    let world = Toy { actors: vec![(7, [80.0, 20.0, 100.0], 25.0), (8, [-80.0, 0.0, 100.0], 25.0), (9, [300.0, 0.0, 100.0], 25.0)], fence: false, wall: None };
    let hits = swing(&world, &t, [0.0, 0.0, 100.0], [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 120.0, 1);
    assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), vec![7]);
    // A short creature at the attacker's feet (a gecko): under the fan of rays at eye
    // height, and the drop from the eye would use up the reach; reached across the
    // ground it is struck. One behind is not, nor one beyond the reach.
    let low = Toy { actors: vec![(7, [70.0, 10.0, 15.0], 15.0), (8, [-70.0, 0.0, 15.0], 15.0), (9, [200.0, 0.0, 15.0], 15.0)], fence: false, wall: None };
    let eye = [0.0, 0.0, 112.0];
    assert!(swing(&low, &t, eye, [1.0, 0.0, 0.0], [0.0, 0.0, 1.0], 120.0, 1).is_empty());
    let hits = swing_reaching(&low, &t, eye, 0.0, [1.0, 0.0, 0.0], 120.0, 1);
    assert_eq!(hits.iter().map(|h| h.0).collect::<Vec<_>>(), vec![7]);
    // A tall one in front is still found by the fan, once.
    let both = Toy { actors: vec![(7, [70.0, 0.0, 15.0], 15.0), (8, [90.0, 30.0, 100.0], 25.0)], fence: false, wall: None };
    let mut ids: Vec<_> = swing_reaching(&both, &t, eye, 0.0, [1.0, 0.0, 0.0], 120.0, 1).iter().map(|h| h.0).collect();
    ids.sort();
    assert_eq!(ids, vec![7, 8]);
    let e =ExplosionStats { damage: 100.0, radius: 400.0, force: 300.0, ..ExplosionStats::default() };
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

#[test]
fn melee_reach_power_and_block() {
    let r = Rules::defaults();
    let t = CombatTuning::default();
    let knife = WeaponStats { anim_type: Some("Melee (1 Hand)".into()), reach: 0.5, ..gun(10) };
    let fists = WeaponStats { anim_type: Some("Hand to Hand".into()), reach: 0.0, ..gun(0) };
    assert!(close(damage::melee_reach(&r, &knife, 1.0), 0.5 * r.combat_distance));
    // A reach of 0 (the Fists) counts as 1.
    assert!(close(damage::melee_reach(&r, &fists, 1.0), r.combat_distance));
    // A power attack multiplies the base before armour, by its direction's setting.
    let a = Attacker { strength: 10.0, ..skilled() };
    let power = fnv_combat::melee::PowerDir::Stand.damage(&r);
    assert!(close(power, 3.0) && close(fnv_combat::melee::PowerDir::Left.damage(&r), 2.0));
    let h = HitInput { weapon: &knife, projectiles: 1, condition: 1.0, ammo_effects: &[], part: HitPart::default(), power };
    let d = damage::hit(&r, &t, &h, &a, &Target::default(), &NoPerks, &NoPerks, 0.99);
    assert!(close(d.dealt, (10.0 + 5.0) * power), "{d:?}");
    // Fists do the UnarmedDamage actor value; it is no miss.
    let h = HitInput { weapon: &fists, projectiles: 1, condition: 1.0, ammo_effects: &[], part: HitPart::default(), power: 1.0 };
    let d = damage::hit(&r, &t, &h, &Attacker { unarmed_skill: 40.0, ..Attacker::default() }, &Target::default(), &NoPerks, &NoPerks, 0.99);
    assert!(!d.miss && close(d.dealt, 0.5 + 0.05 * 40.0));
    assert!(close(damage::blocked(&t, 10.0), 10.0 * (1.0 - t.block)));
}

/// Where a shot leaves and where it is seen to leave (docs/design/combat.md §5.1).
#[test]
fn shots_are_aimed_from_the_eye_and_seen_from_the_muzzle() {
    let t = CombatTuning::default();
    let eye = [0.0, 0.0, 100.0];
    let aim = [1.0, 0.0, 0.0];
    let muzzle = [20.0, 10.0, 60.0];
    let wall = Toy { actors: vec![], fence: false, wall: Some(800.0) };
    // Without a muzzle the shot leaves the eye and is drawn where it flies.
    let l = shot::leaving(&wall, eye, aim, None, 5000.0, Some(1), true);
    assert_eq!((l.origin, l.aim, l.seen, l.covered), (eye, aim, Seen::ON_PATH, false));
    assert!(close(l.aim_distance, 800.0) && l.aim_point == [800.0, 0.0, 100.0]);
    // With one it still leaves the eye along the view; it is drawn from the muzzle and
    // is on its path by the aim point.
    let l = shot::leaving(&wall, eye, aim, Some(muzzle), 5000.0, Some(1), true);
    assert_eq!((l.origin, l.aim, l.covered), (eye, aim, false));
    assert_eq!(l.seen.at(eye, 0.0), muzzle);
    assert_eq!(l.seen.at([400.0, 0.0, 100.0], 400.0), [410.0, 5.0, 80.0]);
    assert_eq!(l.seen.at(l.aim_point, 800.0), l.aim_point);
    assert_eq!(l.seen.at([900.0, 0.0, 100.0], 900.0), [900.0, 0.0, 100.0]);
    // Nothing in the way of the view: the offset closes over the reach.
    let open = Toy { actors: vec![], fence: false, wall: None };
    let l = shot::leaving(&open, eye, aim, Some(muzzle), 5000.0, Some(1), true);
    assert!(close(l.aim_distance, 5000.0) && close(l.seen.converge, 5000.0) && !l.covered);
    // Something the muzzle's ray meets and the view passes over, but far from the
    // barrel (the ray goes through z = 80 at x = 400): no cover.
    let far = Toy { actors: vec![(8, [400.0, 5.0, 70.0], 15.0)], fence: false, wall: Some(800.0) };
    let l = shot::leaving(&far, eye, aim, Some(muzzle), 5000.0, Some(1), true);
    assert!(!l.covered && l.origin == eye && l.seen.offset == vec::sub(muzzle, eye));
    // The same right in front of the barrel (the ray goes through (100, 9, 64)): in
    // third person the projectiles leave the muzzle for the aim point and strike it;
    // in first person they do not.
    let cover = Toy { actors: vec![(8, [100.0, 9.0, 60.0], 10.0)], fence: false, wall: Some(800.0) };
    let l = shot::leaving(&cover, eye, aim, Some(muzzle), 5000.0, Some(1), false);
    assert!(!l.covered && l.origin == eye);
    let l = shot::leaving(&cover, eye, aim, Some(muzzle), 5000.0, Some(1), true);
    assert!(l.covered && l.origin == muzzle && l.seen == Seen::ON_PATH);
    assert!(close(vec::len(vec::sub(vec::add(muzzle, vec::scale(l.aim, vec::len(vec::sub(l.aim_point, muzzle)))), l.aim_point)), 0.0));
    let mut p = Projectile::spawn(&proj("Missile", &["Hitscan"], 20000.0, 0.0), None, l.origin, l.aim, Some(1), G, &t);
    assert!(matches!(&p.step(&cover, 1.0 / 60.0)[..], [ProjEvent::Hit { actor: Some(8), .. }]));
    // The actor aimed at is no cover, though the muzzle's ray meets it sooner.
    let target = Toy { actors: vec![(7, [400.0, 0.0, 100.0], 40.0)], fence: false, wall: None };
    let l = shot::leaving(&target, eye, aim, Some([20.0, 10.0, 60.0]), 5000.0, Some(1), true);
    assert!(!l.covered && l.origin == eye && close(l.aim_distance, 360.0));
    // A barrel pushed past the aim point: drawn from the eye.
    let l = shot::leaving(&wall, eye, aim, Some([820.0, 10.0, 60.0]), 5000.0, Some(1), true);
    assert_eq!((l.origin, l.seen, l.covered), (eye, Seen::ON_PATH, false));
}
