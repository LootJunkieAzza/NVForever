//! Blood on the world behind a shot actor (#3318) through the real producer: an NPC's
//! FireWeapon shot flies (`fly`), strikes the target's hit volume, and the splatter is
//! sent as an `ImpactFx` onto the floor below it. Portable records, no game data.
use super::critical_damage_hit_tests::{app, query, tick};
use super::*;
use fnv_combat::data::BodyPartData;
use fnv_combat::fx::{FxForms, Impact, WeaponFx, IMPACT_MATERIALS};

// The critical-hit tests load the same fixture file as their own module.
#[allow(dead_code, clippy::duplicate_mod)]
#[path = "../../../test-fixtures/critical_hit_records.rs"]
mod records;
use records::*;

const WEAPON_SET: FormId = FormId(0xb10);
const BLOOD_SET: FormId = FormId(0xb11);
const SCORCH: FormId = FormId(0xb20);
const BLOOD: FormId = FormId(0xb21);
const BODY: FormId = FormId(0xb30);

/// One NPC shot on the target over flat ground, the target's body part data giving
/// `blood_set` (or none), the weapon with its impact data set or without one; every
/// impact sent.
fn shoot(blood_set: Option<FormId>, weapon_set: bool) -> (Vec<ImpactFx>, u32) {
    let mut a = app();
    let organic = IMPACT_MATERIALS.iter().position(|m| *m == "organic").unwrap();
    let mut forms = FxForms::default();
    forms.weapons.insert(GUN, WeaponFx { impact_dataset: weapon_set.then_some(WEAPON_SET), ..Default::default() });
    // The weapon's organic slot holds a scorch (as a flamer's does), the body part's set
    // the blood.
    for (set, impact) in [(WEAPON_SET, SCORCH), (BLOOD_SET, BLOOD)] {
        let mut slots = [None; 12];
        slots[organic] = Some(impact);
        forms.impact_sets.insert(set, slots);
        forms.impacts.insert(impact, Impact::default());
    }
    a.insert_resource(crate::sfx::Sfx::new(forms));
    {
        let mut c = a.world_mut().resource_mut::<Combat>();
        let part = fnv_combat::data::BodyPart { severable_impact_dataset: blood_set, ..Default::default() };
        c.forms.body_parts.insert(BODY, BodyPartData { edid: None, model: None, parts: vec![part], ragdoll: None });
        // A roll that wins at any chance above 0.005 (the first of this seed is 0.004).
        c.blood_rng = SplitMix(196);
    }
    a.world_mut().resource_mut::<ActorParts>().by_ref.insert(TARGET.0, crate::hitvol::PosedParts { data: BODY, segments: Vec::new(), weapon: None });
    {
        let mut phys = a.world_mut().resource_mut::<PhysState>();
        for grid in [(-1, -1), (0, -1), (-1, 0), (0, 0)] {
            phys.world.add_terrain(grid, &vec![0.0; fnv_world::land::SIDE * fnv_world::land::SIDE]);
        }
        phys.world.update_queries();
    }
    query(&mut a, &format!("ref source\nlet source := StringToRef \"{:08X}\"\nsource.AddItem CriticalHitGun 10 1", SHOOTER.0));
    query(&mut a, &format!("ref source\nlet source := StringToRef \"{:08X}\"\nsource.FireWeapon CriticalHitGun", SHOOTER.0));
    let mut impacts = Vec::new();
    let mut hits = 0;
    for _ in 0..180 {
        hits += tick(&mut a).len();
        impacts.extend(a.world_mut().resource_mut::<Messages<ImpactFx>>().drain());
        if hits > 0 {
            break;
        }
    }
    assert_eq!(hits, 1, "one real hit on the target");
    (impacts, a.world().resource::<CombatStats>().blood_sent)
}

#[test]
fn a_shot_through_fly_splatters_the_parts_blood_on_the_floor() {
    let (impacts, sent) = shoot(Some(BLOOD_SET), true);
    // The actor's own impact: the weapon's organic slot.
    let on_actor: Vec<&ImpactFx> = impacts.iter().filter(|i| i.actor == Some(TARGET.0)).collect();
    assert_eq!(on_actor.len(), 1);
    assert!(!on_actor[0].splatter);
    assert_eq!(on_actor[0].impact, SCORCH);
    // One splatter: the body part's blood, decal only, on the flat ground at z 0.
    let splatters: Vec<&ImpactFx> = impacts.iter().filter(|i| i.splatter).collect();
    assert_eq!(splatters.len(), 1, "{impacts:?}");
    let s = splatters[0];
    assert_eq!((s.impact, s.actor, s.shooter), (BLOOD, None, FormId(0)));
    assert!(s.no_spray && !s.no_decal);
    assert!(s.point[2].abs() < 1.0, "on the ground: {:?}", s.point);
    assert!(s.normal[2] > 0.9, "facing up: {:?}", s.normal);
    assert_eq!(sent, 1);
}

#[test]
fn no_blood_impact_on_the_part_leaves_no_splatter_not_the_weapons_scorch() {
    let (impacts, sent) = shoot(None, true);
    assert!(impacts.iter().any(|i| i.actor == Some(TARGET.0)), "the hit's own impact is sent");
    assert!(!impacts.iter().any(|i| i.splatter), "{impacts:?}");
    assert_eq!(sent, 0);
}

#[test]
fn a_weapon_without_an_impact_data_set_still_splatters_the_parts_blood() {
    let (impacts, sent) = shoot(Some(BLOOD_SET), false);
    assert!(!impacts.iter().any(|i| i.actor == Some(TARGET.0)), "no impact of the weapon's own");
    let splatters: Vec<&ImpactFx> = impacts.iter().filter(|i| i.splatter).collect();
    assert_eq!(splatters.len(), 1, "{impacts:?}");
    assert_eq!(splatters[0].impact, BLOOD);
    assert_eq!(sent, 1);
}
