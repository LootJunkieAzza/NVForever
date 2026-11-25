//! [design, #2652] Run the real shooter over authored references, without assets.
//! Hit delivery below enters Game::harmless_hit directly; physical collision is
//! covered by the mainframe smoke scenario, not this fixture.

use super::*;
use bevy::ecs::system::RunSystemOnce;
use fnv_combat::data::WeaponStats;
use fnv_game::combat::HitInfo;
use fnv_game::game::event;
use fnv_game::use_weapon::WeaponUse;
use fnv_script::vm::Value;
use std::sync::Arc;

// The test plugin builder (crates/fnv-game/tests/common), loaded once as crate::authored_test_records.
use crate::authored_test_records::*;

const SHOOTER: FormId = FormId(0x801);
const TARGET: FormId = FormId(0x802);
const GUN: FormId = FormId(0x803);
const BASE: u32 = 0x804;
const SCRIPT: u32 = 0x805;
const CELL: u32 = 0x806;

fn fixture(kind: &[u8; 4], bounds: Option<[i16; 6]>) -> bevy::prelude::World {
    fixture_in_worlds(kind, bounds, None)
}

fn fixture_in_worlds(kind: &[u8; 4], bounds: Option<[i16; 6]>, parents: Option<[Option<u32>; 2]>) -> bevy::prelude::World {
    let placed = |sig, id, base, pos: [f32; 3]| record(sig, 0, id, &[
        sub(b"NAME", u32s(&[base])),
        sub(b"DATA", [pos.into_iter().flat_map(f32::to_le_bytes).collect::<Vec<_>>(), vec![0; 12]].concat()),
    ]);
    let mut base = vec![sub(b"EDID", z("AuthoredTarget"))];
    if let Some(bounds) = bounds {
        base.push(sub(b"OBND", bounds.into_iter().flat_map(i16::to_le_bytes).collect()));
    }
    if kind == b"ACTI" {
        base.push(sub(b"SCRI", u32s(&[SCRIPT])));
    }
    // [design: fnv-game/tests/script_events.rs] One OnHit parameter selects the
    // shooter's SCRO entry; only that shooter increments the authored local.
    let parameter = [le16(1), vec![b'r'], le16(1)].concat();
    let bump = set(var(b's', 1), ex(&[var(b's', 1), a("1"), a("+")]));
    let code = unit(0, &script(&[(event::ON_HIT, Some(parameter), bump)]), &[(1, "hits", 1)], &[SHOOTER.0]);
    let shooter = placed(b"ACHR", SHOOTER.0, 0x807, [0.0; 3]);
    let target = placed(if kind == b"CREA" { b"ACRE" } else { b"REFR" }, TARGET.0, BASE, [0.0, 500.0, 0.0]);
    let cell = |id: u32, refs: &[u8]| [record(b"CELL", 0, id, &[]), group(id.to_le_bytes(), 6, &group(id.to_le_bytes(), 9, refs))].concat();
    let cells = if let Some(parents) = parents {
        let worlds: Vec<u8> = parents.into_iter().enumerate().flat_map(|(i, parent)| {
            let id = 0x810 + i as u32;
            let subs = parent.map(|p| sub(b"WNAM", u32s(&[p]))).into_iter().collect::<Vec<_>>();
            [record(b"WRLD", 0, id, &subs), group(id.to_le_bytes(), 1, &cell(CELL + i as u32 * 0x10, if i == 0 { &shooter } else { &target }))].concat()
        }).collect();
        top(b"WRLD", &[record(b"WRLD", 0, 0x820, &[]), record(b"WRLD", 0, 0x821, &[]), worlds].concat())
    } else {
        top(b"CELL", &group([0; 4], 2, &group([0; 4], 3, &cell(CELL, &[shooter, target].concat()))))
    };
    let weap = |id: u32, edid: &str| record(b"WEAP", 0, id, &[sub(b"EDID", z(edid))]);
    let raw = [tes4(), top(b"NPC_", &record(b"NPC_", 0, 0x807, &[sub(b"EDID", z("AuthoredShooter"))])),
        top(b"WEAP", &[weap(0x840, "TestFragGrenade"), weap(0x841, "TestMachete"), weap(0x842, "TestCramOpener"), weap(0x843, "TestRifle")].concat()),
        top(kind, &record(kind, 0, BASE, &base)), top(b"SCPT", &record(b"SCPT", 0, SCRIPT, &code)), cells].concat();
    let plugins = Box::leak(Box::new(vec![fnv_esm::Plugin::parse(&raw, "Authored.esm".into()).unwrap()]));
    let script = ScriptState::over_plugins(plugins);
    let g = &script.rt.as_ref().unwrap().game;
    let sim = SimState { forms: g.world.forms, refs: g.world.placed, world: fnv_sim::World::new(g.world.forms, g.world.placed, Default::default()) };
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(fnv_esm::Schema::load_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../../schemas/fnv")).unwrap()));
    let play = nvf_play::PlayData {
        lo, schema, index: Box::leak(Box::default()), world: 0, interior: Some(CELL), eye: [0.0; 3],
        heading_deg: 0.0, pitch_deg: 0.0, auto_walk: 0.0, start_at_cell_spot: false, packages: false,
    };
    let mut combat = Combat::over_plugins(play);
    combat.forms.weapons.insert(GUN, WeaponStats {
        projectile: Some(FormId(0x808)), automatic: true, fire_rate: 5.0, clip_size: 20,
        ammo_use: 1, attack_mult: 1.0, anim_mult: 1.0, ..Default::default()
    });
    let mut world = bevy::prelude::World::new();
    let mut time = Time::<()>::default();
    time.advance_by(std::time::Duration::from_secs_f32(0.1));
    world.insert_resource(time);
    world.insert_resource(script);
    world.insert_resource(sim);
    world.insert_resource(combat);
    world.insert_resource(Shooters::default());
    world.insert_resource(AttackTimes::default());
    world.init_resource::<Messages<ActorFire>>();
    world.init_resource::<Messages<ActorStrike>>();
    world.init_resource::<Messages<PlayAttack>>();
    world.spawn((ActorRig {
        reference: SHOOTER.0, base: 0x807, body: 0x807, skeleton: String::new(),
        skel: Arc::new(nvf_assets::actor::PreparedSkeleton::default()), nodes: vec![], base_scale: 1.0,
    }, GlobalTransform::from(crate::world::game_to_bevy()), InheritedVisibility::VISIBLE));
    world
}

fn start(world: &mut bevy::prelude::World, repeat: bool) {
    let mut use_ = WeaponUse::of_script(Some(GUN), None, TARGET, 3, false, false, true, true, None);
    use_.in_place = true;
    if repeat {
        use_.source = Source::Package(FormId(0x809));
        use_.volleys = None;
    }
    world.resource_mut::<ScriptState>().rt.as_mut().unwrap().game.weapon_uses.insert(SHOOTER, use_);
}

fn fire(world: &mut bevy::prelude::World) -> Vec<ActorFire> {
    for _ in 0..50 {
        world.run_system_once(shoot).unwrap();
    }
    world.resource_mut::<Messages<ActorFire>>().drain().collect()
}

#[test]
fn activator_gets_always_hit_harmless_shots_and_shooter_filtered_on_hit() {
    let mut world = fixture(b"ACTI", Some([-20, -20, 20, 20, 20, 100]));
    start(&mut world, false);
    let shots = fire(&mut world);
    assert_eq!(shots.len(), 3);
    let mut script = world.resource_mut::<ScriptState>();
    let g = &mut script.rt.as_mut().unwrap().game;
    let info = |attacker| HitInfo { attacker: Some(attacker), weapon: Some(GUN), location: -1, ..HitInfo::default() };
    assert!(!g.harmless_hit(TARGET, info(FormId::PLAYER_REF)), "the OnHit block filters the shooter");
    for shot in shots {
        assert!(shot.harmless);
        assert_eq!(shot.cone, 0.0);
        let hit = vec::add(shot.origin, vec::scale(shot.aim, (500.0 - shot.origin[1]) / shot.aim[1]));
        assert!((hit[2] - 60.0).abs() < 0.001, "aim at OBND centre: {hit:?}");
        assert!(g.harmless_hit(TARGET, info(shot.shooter)));
    }
    let (key, _) = g.instance_of(TARGET).unwrap();
    assert_eq!(g.var(key, 1), Value::Num(3.0));
    assert!(g.weapon_use(SHOOTER).is_none());
}

#[test]
fn marker_script_fires_its_shots_and_ends_without_a_hit() {
    let mut world = fixture(b"STAT", None);
    start(&mut world, false);
    let shots = fire(&mut world);
    assert_eq!(shots.len(), 3);
    for shot in shots {
        let hit = vec::add(shot.origin, vec::scale(shot.aim, (500.0 - shot.origin[1]) / shot.aim[1]));
        assert!(hit[2].abs() < 0.001, "a marker has no chest offset: {hit:?}");
    }
    assert!(world.resource::<ScriptState>().rt.as_ref().unwrap().game.weapon_use(SHOOTER).is_none());
    assert_eq!(world.resource::<Shooters>().stats.done, 1);
}

#[test]
fn static_target_gets_shots_and_repeat_fire_keeps_going() {
    let mut world = fixture(b"STAT", Some([-40, -10, -2, 40, 10, 202]));
    start(&mut world, true);
    let shots = fire(&mut world);
    assert!(shots.len() > 3);
    let use_ = world.resource::<ScriptState>().rt.as_ref().unwrap().game.weapon_use(SHOOTER).unwrap();
    assert_eq!(use_.fired as usize, shots.len());
    assert!(!use_.done);
}

/// [hypothesis, #2652] Sibling worldspaces share coordinates for non-actor targets;
/// [design] authored WNAM records exercise the parent index and real shooter together.
#[test]
fn sibling_worldspace_static_gets_shots_at_its_bounds_centre() {
    let mut world = fixture_in_worlds(b"STAT", Some([-40, -10, -2, 40, 10, 202]), Some([Some(0x820); 2]));
    start(&mut world, true);
    let shots = fire(&mut world);
    assert!(shots.len() > 3);
    for shot in shots {
        assert!(shot.harmless);
        assert_eq!(shot.cone, 0.0);
        let hit = vec::add(shot.origin, vec::scale(shot.aim, (500.0 - shot.origin[1]) / shot.aim[1]));
        assert!((hit[2] - 100.0).abs() < 0.001, "sibling's OBND centre: {hit:?}");
    }
}

/// [design, #2652] Neither a different parent nor two absent parents allow projection.
#[test]
fn unrelated_worldspace_static_does_not_get_shots() {
    for parents in [[Some(0x820), Some(0x821)], [None, None], [Some(0x820), None]] {
        let mut world = fixture_in_worlds(b"STAT", Some([0, 0, 0, 0, 0, 200]), Some(parents));
        start(&mut world, true);
        assert!(fire(&mut world).is_empty(), "parents: {parents:?}");
        assert_eq!(world.resource::<Shooters>().stats.from_packages, 0);
        assert_eq!(world.resource::<ScriptState>().rt.as_ref().unwrap().game.weapon_use(SHOOTER).unwrap().fired, 0);
    }
}

#[test]
fn drawn_actor_keeps_its_posed_chest_aim_instead_of_object_bounds() {
    let mut world = fixture(b"ACTI", Some([0, 0, 0, 0, 0, 200]));
    let bone = world.spawn(GlobalTransform::from_translation(Vec3::new(5.0, 80.0, -500.0))).id();
    let mut skel = nvf_assets::actor::PreparedSkeleton::default();
    skel.by_name.insert("Bip01 Spine1".into(), 0);
    world.spawn((ActorRig {
        reference: TARGET.0, base: BASE, body: BASE, skeleton: String::new(),
        skel: Arc::new(skel), nodes: vec![bone], base_scale: 1.0,
    }, GlobalTransform::from(Transform { translation: Vec3::new(0.0, 0.0, -500.0), ..crate::world::game_to_bevy() }), InheritedVisibility::VISIBLE));
    start(&mut world, false);
    let shots = fire(&mut world);
    assert_eq!(shots.len(), 3);
    for shot in shots {
        let at = vec::add(shot.origin, vec::scale(shot.aim, (500.0 - shot.origin[1]) / shot.aim[1]));
        assert!((at[0] - 5.0).abs() < 0.001 && (at[2] - 80.0).abs() < 0.001, "posed chest: {at:?}");
    }
}

#[test]
fn fallback_tracks_moved_references_and_rejects_dead_disabled_or_missing_targets() {
    let mut world = fixture(b"ACTI", Some([10, -20, 0, 30, 20, 100]));
    let mut script = world.resource_mut::<ScriptState>();
    let g = &mut script.rt.as_mut().unwrap().game;
    assert_eq!(target_place(g, SHOOTER, TARGET), Some(([0.0, 500.0, 0.0], [20.0, 500.0, 50.0])));
    g.world.get(TARGET).unwrap().pos = [100.0, 200.0, 300.0];
    assert_eq!(target_place(g, SHOOTER, TARGET), Some(([100.0, 200.0, 300.0], [120.0, 200.0, 350.0])));
    g.extra.entry(TARGET).or_default().dead = true;
    assert!(target_place(g, SHOOTER, TARGET).is_none());
    g.extra.entry(TARGET).or_default().dead = false;
    g.world.set_enabled(TARGET, false).unwrap();
    assert!(target_place(g, SHOOTER, TARGET).is_none());
    assert!(target_place(g, SHOOTER, FormId(0xFFFF)).is_none());
}

/// [design, #2782 review] An actor target that is not drawn has no aim point: the
/// shooter does not fire, nothing counts as fired, and `acts_on` stays false so the
/// ordinary attacks keep the actor (#2145 review: never held with neither acting).
#[test]
fn undrawn_actor_target_gets_no_shots_and_leaves_the_fight_to_the_ordinary_attacks() {
    let mut world = fixture(b"CREA", Some([-20, -20, 0, 20, 20, 100]));
    {
        let script = world.resource::<ScriptState>();
        let g = &script.rt.as_ref().unwrap().game;
        assert!(g.present(TARGET), "the actor is enabled and alive");
        assert!(target_place(g, SHOOTER, TARGET).is_none(), "no aim point for an actor not drawn");
    }
    start(&mut world, true);
    assert!(fire(&mut world).is_empty());
    let script = world.resource::<ScriptState>();
    let use_ = script.rt.as_ref().unwrap().game.weapon_use(SHOOTER).unwrap();
    assert_eq!(use_.fired, 0);
    assert!(!acts_on(use_, target_place(&script.rt.as_ref().unwrap().game, SHOOTER, TARGET).is_some()));
}

/// [design, #3668] Real PACK records with target 1 object types 21, 22, 23, 0 and 12
/// plus a WEAP ObjectId: `Game::package_weapon_use` maps 21 -> Unarmed, 22 -> Melee,
/// 23 -> Ranged, other object types -> None, and a WEAP form -> explicit weapon.
#[test]
fn package_weapon_use_reads_target1_weapon_type() {
    use fnv_game::use_weapon::WeaponType;
    let pack = |id: u32, t1_kind: i32, t1_val: u32| {
        let pkw3 = [&0x0000_0100u32.to_le_bytes()[..], &[1, 1], &1u16.to_le_bytes(), &1u16.to_le_bytes(), &3u16.to_le_bytes(), &0.5f32.to_le_bytes(), &0.5f32.to_le_bytes(), &[0; 4]].concat();
        record(b"PACK", 0, id, &[
            sub(b"EDID", z(&format!("UseWeap{id:X}"))),
            sub(b"PKDT", [&0u32.to_le_bytes()[..], &[16, 0, 0, 0], &[0; 4]].concat()),
            sub(b"PTDT", [t1_kind.to_le_bytes(), t1_val.to_le_bytes(), 0i32.to_le_bytes()].concat()),
            sub(b"PKW3", pkw3),
            sub(b"PTD2", [0i32.to_le_bytes(), TARGET.0.to_le_bytes(), 0i32.to_le_bytes()].concat()),
        ])
    };
    let placed = |sig, id, base| record(sig, 0, id, &[sub(b"NAME", u32s(&[base])), sub(b"DATA", vec![0; 24])]);
    let cell = [record(b"CELL", 0, CELL, &[]), group(CELL.to_le_bytes(), 6, &group(CELL.to_le_bytes(), 9, &[placed(b"ACHR", SHOOTER.0, 0x807), placed(b"ACHR", TARGET.0, 0x807)].concat()))].concat();
    let raw = [
        tes4(),
        top(b"NPC_", &record(b"NPC_", 0, 0x807, &[sub(b"EDID", z("Initiate"))])),
        top(b"WEAP", &record(b"WEAP", 0, GUN.0, &[sub(b"EDID", z("TestGun"))])),
        top(b"PACK", &[
            pack(0x830, 2, 21),
            pack(0x831, 2, 22),
            pack(0x832, 2, 23),
            pack(0x833, 2, 0),
            pack(0x834, 2, 12),
            pack(0x835, 1, GUN.0),
        ].concat()),
        top(b"CELL", &group([0; 4], 2, &group([0; 4], 3, &cell))),
    ].concat();
    let plugins = Box::leak(Box::new(vec![fnv_esm::Plugin::parse(&raw, "PackTypes.esm".into()).unwrap()]));
    let script = ScriptState::over_plugins(plugins);
    let g = &script.rt.as_ref().unwrap().game;
    let check = |pack_id| {
        let u = g.package_weapon_use(SHOOTER, FormId(pack_id)).expect("valid Use Weapon package");
        (u.weapon, u.weapon_type)
    };
    assert_eq!(check(0x830), (None, Some(WeaponType::Unarmed)));
    assert_eq!(check(0x831), (None, Some(WeaponType::Melee)));
    assert_eq!(check(0x832), (None, Some(WeaponType::Ranged)));
    assert_eq!(check(0x833), (None, None));
    assert_eq!(check(0x834), (None, None));
    assert_eq!(check(0x835), (Some(GUN), None));
}

/// [design, #3668] End-to-end `shoot` checks for Use Weapon package target 1 weapon types:
/// - Type 21 (Unarmed): an initiate holding a frag grenade swings its fists instead of
///   throwing grenades; carrying an unarmed weapon uses that unarmed weapon.
/// - Type 22 (Melee): picks a carried melee weapon over a held grenade/gun; with only a
///   gun/grenade carried, does nothing.
/// - Type 23 (Ranged): picks a carried gun over a held melee weapon/grenade; with only a
///   melee weapon or grenade carried, does nothing.
#[test]
fn package_weapon_types_select_fists_melee_and_ranged_weapons() {
    use fnv_game::use_weapon::WeaponType;
    const GRENADE: FormId = FormId(0x840);
    const MACHETE: FormId = FormId(0x841);
    const CRAM_OPENER: FormId = FormId(0x842);
    const RIFLE: FormId = FormId(0x843);

    let setup = |carried: &[FormId], drawn: Option<FormId>, weapon_type: WeaponType| {
        let mut world = fixture(b"ACTI", None);
        // Place TARGET close enough (40 units ahead along +Y) for both melee blows and shots.
        world.resource_mut::<SimState>().world.get(TARGET).unwrap().pos = [0.0, 40.0, 0.0];
        world.resource_mut::<ScriptState>().rt.as_mut().unwrap().game.world.get(TARGET).unwrap().pos = [0.0, 40.0, 0.0];
        {
            let mut c = world.resource_mut::<Combat>();
            c.forms.weapons.insert(GRENADE, WeaponStats {
                anim_type: Some(crate::npc_grenades::GRENADE_ANIM.into()),
                projectile: Some(FormId(0x850)),
                base_damage: 100,
                fire_rate: 1.0,
                clip_size: 1,
                ammo_use: 1,
                attack_mult: 1.0,
                anim_mult: 1.0,
                ..Default::default()
            });
            c.forms.weapons.insert(MACHETE, WeaponStats {
                anim_type: Some("Melee (1 Hand)".into()),
                base_damage: 25,
                reach: 1.0,
                attack_mult: 1.0,
                anim_mult: 1.0,
                ..Default::default()
            });
            c.forms.weapons.insert(CRAM_OPENER, WeaponStats {
                anim_type: Some("Hand to Hand".into()),
                base_damage: 30,
                reach: 0.8,
                attack_mult: 1.0,
                anim_mult: 1.0,
                ..Default::default()
            });
            c.forms.weapons.insert(RIFLE, WeaponStats {
                anim_type: Some("Rifle - Ballistic (2 Hand)".into()),
                projectile: Some(FormId(0x851)),
                base_damage: 40,
                automatic: true,
                fire_rate: 5.0,
                clip_size: 10,
                ammo_use: 1,
                attack_mult: 1.0,
                anim_mult: 1.0,
                ..Default::default()
            });
        }
        {
            let mut sim = world.resource_mut::<SimState>();
            for &item in carried {
                sim.world.add_item(SHOOTER, item, 1).unwrap();
            }
        }
        if let Some(weapon) = drawn {
            world.spawn(HeldWeapon { reference: SHOOTER.0, weapon });
        }
        let mut use_ = WeaponUse::of_script(None, None, TARGET, 2, false, false, true, true, None);
        use_.source = Source::Package(FormId(0x809));
        use_.weapon_type = Some(weapon_type);
        use_.in_place = true;
        use_.volleys = None;
        world.resource_mut::<ScriptState>().rt.as_mut().unwrap().game.give_weapon_use(SHOOTER, use_);
        for _ in 0..10 {
            world.run_system_once(shoot).unwrap();
        }
        let shots: Vec<ActorFire> = world.resource_mut::<Messages<ActorFire>>().drain().collect();
        let attacks: Vec<PlayAttack> = world.resource_mut::<Messages<PlayAttack>>().drain().collect();
        (shots, attacks)
    };

    // Type 21 (Unarmed) while holding a frag grenade (Citadel Initiate 05): swings fists, throws no grenade.
    let (shots, attacks) = setup(&[GRENADE], Some(GRENADE), WeaponType::Unarmed);
    assert!(shots.is_empty(), "type 21 must never throw a held grenade: {shots:?}");
    assert!(!attacks.is_empty(), "type 21 should swing fists");
    assert!(attacks.iter().all(|a| a.melee && a.weapon.is_none()), "fists only: {attacks:?}");

    // Type 21 (Unarmed) carrying an unarmed weapon (Little Buster control): swings the unarmed weapon.
    let (shots, attacks) = setup(&[GRENADE, CRAM_OPENER], Some(GRENADE), WeaponType::Unarmed);
    assert!(shots.is_empty());
    assert!(!attacks.is_empty());
    assert!(attacks.iter().all(|a| a.melee && a.weapon == Some(CRAM_OPENER)), "unarmed weapon: {attacks:?}");

    // Type 22 (Melee) carrying a melee weapon alongside a grenade and rifle: swings the melee weapon.
    let (shots, attacks) = setup(&[GRENADE, RIFLE, MACHETE], Some(RIFLE), WeaponType::Melee);
    assert!(shots.is_empty(), "type 22 must not shoot a held gun");
    assert!(!attacks.is_empty());
    assert!(attacks.iter().all(|a| a.melee && a.weapon == Some(MACHETE)), "melee weapon: {attacks:?}");

    // Type 22 (Melee) carrying only a rifle/grenade: does nothing (GECK wiki Use Weapon Package, Notes).
    let (shots, attacks) = setup(&[GRENADE, RIFLE], Some(RIFLE), WeaponType::Melee);
    assert!(shots.is_empty() && attacks.is_empty(), "no melee weapon carried -> does nothing");

    // Type 23 (Ranged) carrying a gun alongside a grenade and melee weapon: fires the gun.
    let (shots, attacks) = setup(&[GRENADE, MACHETE, RIFLE], Some(MACHETE), WeaponType::Ranged);
    assert!(!shots.is_empty(), "type 23 should fire the carried gun");
    assert!(shots.iter().all(|s| s.weapon == RIFLE), "gun only: {shots:?}");
    assert!(attacks.iter().all(|a| !a.melee && a.weapon == Some(RIFLE)), "gun attack anim: {attacks:?}");

    // Type 23 (Ranged) carrying only a grenade or melee weapon: does nothing.
    let (shots, attacks) = setup(&[GRENADE, MACHETE], Some(GRENADE), WeaponType::Ranged);
    assert!(shots.is_empty() && attacks.is_empty(), "no gun carried -> does nothing");
}
