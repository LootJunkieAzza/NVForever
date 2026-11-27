//! Regression tests for corpse limb hits wearing limb condition and requiring
//! limb depletion before rolling corpse limb dismemberment.

use super::critical_damage_hit_tests::app;
use super::*;
use fnv_game::combat::{HitInfo, LimbHit};

const TARGET: FormId = FormId(0x0000_0A46);
const RIFLE: FormId = FormId(0x0000_0A41);

#[test]
fn corpse_limb_hits_require_limb_depletion_before_dismemberment() {
    let mut a = app();
    let w = WeaponStats::default();
    let head_cond = fnv_game::combat::limb_condition("Perception Condition").unwrap();

    // Kill TARGET (1000 HP) with a torso hit; head condition stays intact at 100.
    let kill = a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game.hit(TARGET, 1000.0, HitInfo { attacker: Some(FormId::PLAYER_REF), weapon: Some(RIFLE), ..HitInfo::default() })
        })
    });
    assert!(kill.died);
    // Live killing blow on an intact limb still checks dismemberment (preserving observed dismember_odds).
    let live_with_intact_limb = HitResult { died: true, limb: Some((head_cond, 80.0)), max_health: 1000.0, ..HitResult::default() };
    assert!(dismember_check(TARGET, Some(1), &w, &live_with_intact_limb, Some(FormId::PLAYER_REF), [0.0; 3], [0.0, 1.0, 0.0]).is_some());

    // A stray 25-damage hit on the corpse's intact head (25% of 1000 = 250 HP limb pool)
    // lowers head condition from 100 -> 90 and must NOT roll dismemberment yet.
    let head_hit = |damage: f64| HitInfo {
        attacker: Some(FormId::PLAYER_REF),
        weapon: Some(RIFLE),
        limb: Some(LimbHit { condition: head_cond, health_percent: 25.0, damage, points: false }),
        ..HitInfo::default()
    };
    let stray = a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| rt.game.hit(TARGET, 25.0, head_hit(25.0)))
    });
    assert!(stray.corpse && stray.immune && stray.dealt == 0.0);
    let (code, cond) = stray.limb.expect("corpse limb hit reports remaining limb condition");
    assert_eq!(code, head_cond);
    assert!((cond - 90.0).abs() < 1e-3, "expected head condition 90.0 after 25/250 damage, got {cond}");
    assert!(!stray.crippled);
    assert!(
        dismember_check(TARGET, Some(1), &w, &stray, Some(FormId::PLAYER_REF), [0.0; 3], [0.0, 1.0, 0.0]).is_none(),
        "stray bullet on intact corpse limb must not trigger dismemberment"
    );

    // Follow-up damage depleting the remaining 225 HP of the head pool brings condition to 0
    // and enables the corpse dismemberment check.
    let breaking = a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| rt.game.hit(TARGET, 250.0, head_hit(250.0)))
    });
    assert!(breaking.corpse && breaking.crippled);
    assert_eq!(breaking.limb, Some((head_cond, 0.0)));
    let check = dismember_check(TARGET, Some(1), &w, &breaking, Some(FormId::PLAYER_REF), [0.0; 3], [0.0, 1.0, 0.0])
        .expect("corpse hit that depletes the limb must allow dismemberment");
    assert!(check.corpse && check.crippled);

    // A corpse hit without a specific limb (e.g. explosion blast) continues to allow dismemberment.
    let blast_corpse = HitResult { corpse: true, immune: true, overkill: 60.0, max_health: 1000.0, limb: None, ..HitResult::default() };
    assert!(dismember_check(TARGET, Some(1), &w, &blast_corpse, Some(FormId::PLAYER_REF), [0.0; 3], [0.0, 1.0, 0.0]).is_some());
}
