//! Destruction-stage blast self-damage exclusion and burning countdown tests.
//!
//! Models vanilla `Car01` (`DEST` health 325, stages 93/89/83/75/65/0 %, stage 2
//! `MediumExplosion` with `self_dps = 10`, stage 4 `CAP_DAMAGE | DESTROY` with
//! `self_dps = 1` and `SmallNukeExplosion`) in `nvf-play`'s live `fly` system: a car's
//! own intermediate stage explosion must not self-damage it straight into its final
//! explosion, while still damaging nearby destructibles.

use super::*;
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::RefPose;
use fnv_sim::{SimConfig, World};

const CELL: FormId = FormId(0x0000_0E10);
const CAR_BASE: FormId = FormId(0x0000_0E20);
const CAR_A: FormId = FormId(0x0000_0E21);
const CAR_B: FormId = FormId(0x0000_0E22);
const EXPL_MED: FormId = FormId(0x0000_0E30);
const EXPL_NUKE: FormId = FormId(0x0000_0E31);

fn sub(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(6 + data.len());
    out.extend_from_slice(sig);
    out.extend_from_slice(&(data.len() as u16).to_le_bytes());
    out.extend_from_slice(data);
    out
}

fn rec(sig: &[u8; 4], id: u32, subs: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = subs.concat();
    let mut out = Vec::with_capacity(24 + body.len());
    out.extend_from_slice(sig);
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&id.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&body);
    out
}

fn group(label: [u8; 4], gtype: i32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(24 + body.len());
    out.extend_from_slice(b"GRUP");
    out.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
    out.extend_from_slice(&label);
    out.extend_from_slice(&gtype.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(body);
    out
}

fn top(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    group(*sig, 0, body)
}

fn z(s: &str) -> Vec<u8> {
    let mut out = s.as_bytes().to_vec();
    out.push(0);
    out
}

fn dstd(health_pct: u8, index: u8, stage: u8, flags: u8, self_dps: i32, explosion: u32) -> Vec<u8> {
    let mut out = Vec::with_capacity(20);
    out.extend_from_slice(&[health_pct, index, stage, flags]);
    out.extend_from_slice(&self_dps.to_le_bytes());
    out.extend_from_slice(&explosion.to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out
}

fn refr(id: FormId, base: FormId, pos: [f32; 3]) -> Vec<u8> {
    let mut data = Vec::with_capacity(24);
    for v in pos {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&[0u8; 12]);
    rec(b"REFR", id.0, &[sub(b"NAME", &base.0.to_le_bytes()), sub(b"DATA", &data)])
}

fn build_plugin() -> fnv_esm::Plugin {
    let tes4 = rec(b"TES4", 0, &[sub(b"HEDR", &[0, 0, 0x80, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0])]);
    let mut dest = Vec::with_capacity(8);
    dest.extend_from_slice(&325i32.to_le_bytes());
    dest.extend_from_slice(&[6, 0, 0, 0]);
    let acti = rec(
        b"ACTI",
        CAR_BASE.0,
        &[
            sub(b"EDID", &z("TestCar01")),
            sub(b"DEST", &dest),
            sub(b"DSTD", &dstd(93, 0, 1, 0, 0, 0)),
            sub(b"DSTF", &[]),
            sub(b"DSTD", &dstd(89, 1, 2, 0, 2, 0)),
            sub(b"DSTF", &[]),
            sub(b"DSTD", &dstd(83, 2, 3, 0, 10, EXPL_MED.0)),
            sub(b"DSTF", &[]),
            sub(b"DSTD", &dstd(75, 3, 4, 0, 10, 0)),
            sub(b"DSTF", &[]),
            sub(b"DSTD", &dstd(65, 4, 5, 5, 1, EXPL_NUKE.0)),
            sub(b"DSTF", &[]),
            sub(b"DSTD", &dstd(0, 5, 6, 0, 0, 0)),
            sub(b"DSTF", &[]),
        ],
    );
    let cell = rec(b"CELL", CELL.0, &[sub(b"EDID", &z("TestCarCell"))]);
    let cars = [refr(CAR_A, CAR_BASE, [0.0, 0.0, 0.0]), refr(CAR_B, CAR_BASE, [150.0, 0.0, 0.0])].concat();
    let children = group(CELL.0.to_le_bytes(), 6, &group(CELL.0.to_le_bytes(), 9, &cars));
    let bytes = [
        tes4,
        top(b"ACTI", &acti),
        top(b"CELL", &group(0i32.to_le_bytes(), 2, &group(0i32.to_le_bytes(), 3, &[cell, children].concat()))),
    ]
    .concat();
    fnv_esm::Plugin::parse(&bytes, "CarTest.esm".into()).unwrap()
}

fn car_box(form_id: u32, position: [f32; 3]) -> (RefPose, CollisionObject) {
    let pose = RefPose { form_id, position, rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 };
    let obj = CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion: Motion::Static,
        layer: fnv_phys::layers::STATIC,
        mass: 0.0,
        friction: 0.5,
        restitution: 0.0,
        shape: Shape::Box { half_extents: [60.0, 40.0, 40.0] }.into(),
        shell: 0.0,
        material: None,
        wind: false,
    };
    (pose, obj)
}

fn make_app() -> App {
    let plugins = Box::leak(vec![build_plugin()].into_boxed_slice());
    let script = ScriptState::over_plugins(plugins);
    let game = &script.rt.as_ref().unwrap().game;
    let (forms, refs) = (game.world.forms, game.world.placed);
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(
        fnv_esm::Schema::load_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")).unwrap(),
    ));
    let mut a = App::new();
    a.insert_resource(script)
        .insert_resource(SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) })
        .insert_resource(PlayWorld(crate::PlayData {
            lo,
            schema,
            index: Box::leak(Box::default()),
            world: 0,
            interior: None,
            eye: [0.0; 3],
            heading_deg: 0.0,
            pitch_deg: 0.0,
            auto_walk: 0.0,
            start_at_cell_spot: false,
            packages: false,
        }))
        .insert_resource(Player { heading: 0.0, pitch: 0.0, eye_height: 64.0, feet: [500.0, 500.0, 0.0], body_eye: None, body_view: None, placed: None })
        .init_resource::<PlayerControl>()
        .init_resource::<MoveIntent>()
        .init_resource::<PlayerLimbs>()
        .init_resource::<Time>()
        .init_resource::<PhysState>()
        .init_resource::<CombatStats>()
        .init_resource::<HitFeedback>()
        .init_resource::<ActorParts>()
        .init_resource::<WeaponAnim>()
        .init_resource::<ShotVisuals>()
        .init_resource::<crate::dismember::Dismemberment>()
        .init_resource::<held_grenade_probe::Probe>()
        .add_message::<ActorFire>()
        .add_message::<ActorStrike>()
        .add_message::<ShotFired>()
        .add_message::<PlaySound>()
        .add_message::<ScaledSound>()
        .add_message::<ActorHit>()
        .add_message::<ActorPush>()
        .add_message::<ImpactFx>()
        .add_message::<WeaponBroken>()
        .add_message::<Knockback>()
        .add_message::<ExplosionFx>()
        .add_message::<crate::world_sounds::ImpactPicked>()
        .add_message::<crate::RefChanged>()
        .add_message::<CasingFx>()
        .add_message::<LoopSound>()
        .add_message::<ForcedHitReaction>()
        .add_systems(Startup, init_combat)
        .add_systems(Update, fly);
    a.update();
    {
        let mut combat = a.world_mut().resource_mut::<Combat>();
        combat.forms.explosions.insert(
            EXPL_MED,
            fnv_combat::data::ExplosionStats { edid: Some("MediumExplosion".into()), damage: 75.0, radius: 220.0, ..Default::default() },
        );
        combat.forms.explosions.insert(
            EXPL_NUKE,
            fnv_combat::data::ExplosionStats { edid: Some("SmallNukeExplosion".into()), damage: 150.0, radius: 1200.0, ..Default::default() },
        );
    }
    a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game.world.get(FormId::PLAYER_REF).unwrap().cell = Some(CELL);
            rt.game.player_entered(Some(CELL));
        });
    });
    {
        let mut phys = a.world_mut().resource_mut::<PhysState>();
        let (pose_a, obj_a) = car_box(CAR_A.0, [0.0, 0.0, 0.0]);
        let (pose_b, obj_b) = car_box(CAR_B.0, [150.0, 0.0, 0.0]);
        phys.world.add_ref(&pose_a, &[obj_a]);
        phys.world.add_ref(&pose_b, &[obj_b]);
        phys.world.update_queries();
    }
    a
}

fn step(a: &mut App, dt_secs: f64) -> Vec<ExplosionFx> {
    a.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f64(dt_secs));
    a.update();
    a.world_mut().resource_mut::<Messages<ExplosionFx>>().drain().collect()
}

fn stage_of(a: &mut App, r: FormId) -> (u8, bool, Option<f64>) {
    a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            let stage = rt.game.model_damage_stage(r);
            let e = rt.game.extra.get(&r);
            (stage, e.is_some_and(|e| e.destroyed), e.and_then(|e| e.burning))
        })
    })
}

#[test]
fn stage_explosion_does_not_self_nuke_emitting_car_but_damages_neighbor_and_burns_down() {
    let mut a = make_app();
    // Single 60-damage shot hits stage 2 (83% of 325 = 55.25 damage, damage_stage = 3),
    // queuing MediumExplosion.
    a.world_mut().resource_scope(|world, mut combat: Mut<Combat>| {
        world.resource_scope(|world, mut sim: Mut<SimState>| {
            let mut script = world.resource_mut::<ScriptState>();
            damage_destructible(&mut combat, &mut sim, Some(&mut script), CAR_A, 60.0, None);
        });
    });
    assert_eq!(stage_of(&mut a, CAR_A), (3, false, Some(10.0)));

    // One frame detonates MediumExplosion: CAR_A must NOT self-damage into stage 4 (damage_stage = 5),
    // while adjacent CAR_B (150u away, inside 220u radius) takes attenuated blast damage (~22.3 -> damage_stage 1).
    let blasts = step(&mut a, 0.05);
    assert_eq!(blasts.iter().map(|b| b.explosion).collect::<Vec<_>>(), vec![EXPL_MED]);
    assert_eq!(stage_of(&mut a, CAR_A), (3, false, Some(10.0)), "CAR_A must not self-damage into stage 4 from its own stage-2 explosion");
    let (stage_b, destroyed_b, _) = stage_of(&mut a, CAR_B);
    assert_eq!((stage_b, destroyed_b), (1, false), "adjacent CAR_B must still take damage from CAR_A's stage-2 explosion");

    // Burning at 10 dps: after 3s (90 dmg total) CAR_A reaches stage 3 (75% = 81.25 dmg, damage_stage = 4), still not destroyed.
    let mid_blasts = step(&mut a, 3.0);
    assert!(mid_blasts.is_empty());
    assert_eq!(stage_of(&mut a, CAR_A), (4, false, Some(10.0)));

    // After 3s more (120 dmg, capped at stage 4 = 113.75, damage_stage = 5), CAR_A reaches stage 4 (destroyed) and emits SmallNukeExplosion.
    let nuke_blasts = step(&mut a, 3.0);
    assert!(nuke_blasts.iter().any(|b| b.explosion == EXPL_NUKE), "CAR_A must detonate SmallNukeExplosion after burning down: {nuke_blasts:?}");
    assert_eq!(stage_of(&mut a, CAR_A), (5, true, Some(1.0)), "CAR_A must remain at stage 4 (not self-pushed to stage 5)");
}
