//! Regression tests for `SetWeaponOnHitSetting` (`DNAM/on_hit`) reaching combat
//! dismemberment checks, including `TempCloneForm` fallback and clone-first lookup.

use super::critical_damage_hit_tests::{app, query};
use super::*;
use fnv_combat::data::BodyPart;
use fnv_combat::dismember::{chances, Blow, OnHit, Settings};
use fnv_script::vm::XVal;

const TARGET: FormId = FormId(0x0000_0A46);
const RIFLE: FormId = FormId(0x0000_0A41);

fn run_on_hit(a: &mut App, weapon: FormId) -> OnHit {
    let mut w = a.world().resource::<Combat>().forms.weapons.get(&weapon).cloned().unwrap_or_default();
    let script = a.world().resource::<ScriptState>();
    script_weapon_on_hit(script.rt.as_ref().map(|rt| &rt.game), weapon, &mut w);
    let kill = HitResult { died: true, max_health: 100.0, overkill: 10.0, ..HitResult::default() };
    let check = dismember_check(TARGET, Some(1), &w, &kill, Some(FormId::PLAYER_REF), [0.0; 3], [0.0, 1.0, 0.0])
        .expect("killing blow on severable part creates dismember check");
    check.on_hit
}

#[test]
fn set_weapon_on_hit_setting_reaches_combat_dismember_check() {
    let mut a = app();
    let head = BodyPart {
        part_type: Some("Head 1".into()),
        flags: vec!["Severable".into(), "Explodable".into()],
        health_percent: 25,
        ..BodyPart::default()
    };
    let s = Settings::default();

    // Default authored weapon is Normal.
    assert_eq!(run_on_hit(&mut a, RIFLE), OnHit::Normal);

    // 3 = No Dismember/Explode (as used by realistic dismemberment scripts).
    query(&mut a, &format!("SetWeaponOnHitSetting (StringToRef \"{:08X}\") 3", RIFLE.0));
    let on_hit = run_on_hit(&mut a, RIFLE);
    assert_eq!(on_hit, OnHit::Neither);
    let blow = Blow { on_hit, max_health: 100.0, ..Blow::default() };
    assert_eq!(chances(&head, &blow, &s), (0.0, 0.0), "No Dismember/Explode must suppress dismemberment without Bloody Mess");

    // 1 = Dismember Only, 2 = Explode Only, 0 = Normal.
    query(&mut a, &format!("SetWeaponOnHitSetting (StringToRef \"{:08X}\") 1", RIFLE.0));
    assert_eq!(run_on_hit(&mut a, RIFLE), OnHit::DismemberOnly);

    query(&mut a, &format!("SetWeaponOnHitSetting (StringToRef \"{:08X}\") 2", RIFLE.0));
    assert_eq!(run_on_hit(&mut a, RIFLE), OnHit::ExplodeOnly);

    query(&mut a, &format!("SetWeaponOnHitSetting (StringToRef \"{:08X}\") 0", RIFLE.0));
    assert_eq!(run_on_hit(&mut a, RIFLE), OnHit::Normal);

    // TempCloneForm falls back to its source weapon's override.
    let XVal::Ref(clone_id) = query(&mut a, &format!("SetFunctionValue (TempCloneForm (StringToRef \"{:08X}\"))", RIFLE.0)) else {
        panic!("TempCloneForm must return a ref");
    };
    let clone = FormId(clone_id);
    query(&mut a, &format!("SetWeaponOnHitSetting (StringToRef \"{:08X}\") 3", RIFLE.0));
    assert_eq!(run_on_hit(&mut a, clone), OnHit::Neither, "clone falls back to source override");
    query(&mut a, &format!("SetWeaponOnHitSetting (StringToRef \"{:08X}\") 1", RIFLE.0));
    assert_eq!(run_on_hit(&mut a, clone), OnHit::DismemberOnly, "clone reflects updated source override");
}
