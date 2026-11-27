//! End-to-end engine boundary tests verifying that compiled NVSE `SetWeaponSound`
//! scripts control the live sound FormIDs emitted by `nvf-play`'s combat playback
//! systems (`weapon_sounds`, `fire`, `actor_attacks`, `block_sound`) for both player
//! and NPC weapons, while preserving static authored `WEAP` fallbacks for unmodified
//! slots and weapons.

use super::*;
use crate::world_sounds::{script_weapon_sounds, WeaponSoundSync};
use fnv_combat::data::{ProjectileStats, WeaponMod};
use fnv_combat::fx::{FxForms, Sound, WeaponFx};
use fnv_game::game::RunCtx;
use fnv_script::vm::XVal;
use fnv_sim::{SimConfig, World};

const WEAP_PISTOL: FormId = FormId(0x0000_0B10);
const WEAP_RIFLE: FormId = FormId(0x0000_0B11);
const WEAP_CLUB: FormId = FormId(0x0000_0B12);
const PROJ_BULLET: FormId = FormId(0x0000_0B20);
const NPC_BASE: FormId = FormId(0x0000_0B30);
const NPC_REF: FormId = FormId(0x0000_0B31);
const CELL_ID: FormId = FormId(0x0000_0B40);

// Authored SOUN FormIDs (0..=11) + override SOUN FormIDs.
const SND_3D: FormId = FormId(0x0000_0C00);
const SND_DIST: FormId = FormId(0x0000_0C01);
const SND_2D: FormId = FormId(0x0000_0C02);
const SND_LOOP: FormId = FormId(0x0000_0C03);
const SND_DRY: FormId = FormId(0x0000_0C04);
const SND_BLOCK: FormId = FormId(0x0000_0C05);
const SND_IDLE: FormId = FormId(0x0000_0C06);
const SND_EQUIP: FormId = FormId(0x0000_0C07);
const SND_UNEQUIP: FormId = FormId(0x0000_0C08);
const SND_MOD_3D: FormId = FormId(0x0000_0C09);
const SND_MOD_DIST: FormId = FormId(0x0000_0C0A);
const SND_MOD_2D: FormId = FormId(0x0000_0C0B);

const OVR_3D: FormId = FormId(0x0000_0C20);
const OVR_DIST: FormId = FormId(0x0000_0C21);
const OVR_2D: FormId = FormId(0x0000_0C22);
const OVR_LOOP: FormId = FormId(0x0000_0C23);
const OVR_DRY: FormId = FormId(0x0000_0C24);
const OVR_BLOCK: FormId = FormId(0x0000_0C25);
const OVR_IDLE: FormId = FormId(0x0000_0C26);
const OVR_EQUIP: FormId = FormId(0x0000_0C27);
const OVR_UNEQUIP: FormId = FormId(0x0000_0C28);
const OVR_MOD_3D: FormId = FormId(0x0000_0C29);
const OVR_MOD_DIST: FormId = FormId(0x0000_0C2A);
const OVR_MOD_2D: FormId = FormId(0x0000_0C2B);

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

fn fid_bytes(id: FormId) -> Vec<u8> {
    id.0.to_le_bytes().to_vec()
}

fn build_plugin() -> fnv_esm::Plugin {
    let mut sounds = Vec::new();
    for (id, edid) in [
        (SND_3D, "AuthSnd3D"),
        (SND_DIST, "AuthSndDist"),
        (SND_2D, "AuthSnd2D"),
        (SND_LOOP, "AuthSndLoop"),
        (SND_DRY, "AuthSndDry"),
        (SND_BLOCK, "AuthSndBlock"),
        (SND_IDLE, "AuthSndIdle"),
        (SND_EQUIP, "AuthSndEquip"),
        (SND_UNEQUIP, "AuthSndUnequip"),
        (SND_MOD_3D, "AuthSndMod3D"),
        (SND_MOD_DIST, "AuthSndModDist"),
        (SND_MOD_2D, "AuthSndMod2D"),
        (OVR_3D, "OvrSnd3D"),
        (OVR_DIST, "OvrSndDist"),
        (OVR_2D, "OvrSnd2D"),
        (OVR_LOOP, "OvrSndLoop"),
        (OVR_DRY, "OvrSndDry"),
        (OVR_BLOCK, "OvrSndBlock"),
        (OVR_IDLE, "OvrSndIdle"),
        (OVR_EQUIP, "OvrSndEquip"),
        (OVR_UNEQUIP, "OvrSndUnequip"),
        (OVR_MOD_3D, "OvrSndMod3D"),
        (OVR_MOD_DIST, "OvrSndModDist"),
        (OVR_MOD_2D, "OvrSndMod2D"),
    ] {
        sounds.extend(rec(b"SOUN", id.0, &[sub(b"EDID", &z(edid)), sub(b"FNAM", &z("fx\\owned\\s.wav"))]));
    }

    let weap_subs = |edid: &str| -> Vec<Vec<u8>> {
        vec![
            sub(b"EDID", &z(edid)),
            sub(b"SNAM", &fid_bytes(SND_3D)),
            sub(b"SNAM", &fid_bytes(SND_DIST)),
            sub(b"XNAM", &fid_bytes(SND_2D)),
            sub(b"NAM7", &fid_bytes(SND_LOOP)),
            sub(b"TNAM", &fid_bytes(SND_DRY)),
            sub(b"NAM6", &fid_bytes(SND_BLOCK)),
            sub(b"UNAM", &fid_bytes(SND_IDLE)),
            sub(b"NAM9", &fid_bytes(SND_EQUIP)),
            sub(b"NAM8", &fid_bytes(SND_UNEQUIP)),
            sub(b"WMS1", &fid_bytes(SND_MOD_3D)),
            sub(b"WMS1", &fid_bytes(SND_MOD_DIST)),
            sub(b"WMS2", &fid_bytes(SND_MOD_2D)),
        ]
    };

    let weapons = [
        rec(b"WEAP", WEAP_PISTOL.0, &weap_subs("BridgePistol")),
        rec(b"WEAP", WEAP_RIFLE.0, &weap_subs("BridgeRifle")),
        rec(b"WEAP", WEAP_CLUB.0, &weap_subs("BridgeClub")),
    ]
    .concat();

    let npcs = [
        rec(b"NPC_", 7, &[sub(b"EDID", &z("Player"))]),
        rec(b"NPC_", NPC_BASE.0, &[sub(b"EDID", &z("BridgeNpcBase"))]),
    ]
    .concat();

    let cell = [
        rec(b"CELL", CELL_ID.0, &[sub(b"EDID", &z("BridgeCell"))]),
        group(
            CELL_ID.0.to_le_bytes(),
            6,
            &group(
                CELL_ID.0.to_le_bytes(),
                8,
                &rec(b"ACHR", NPC_REF.0, &[sub(b"EDID", &z("BridgeNpc")), sub(b"NAME", &fid_bytes(NPC_BASE))]),
            ),
        ),
    ]
    .concat();

    let bytes = [
        rec(b"TES4", 0, &[sub(b"HEDR", &[0, 0, 0x80, 0x3F, 0, 0, 0, 0, 0, 0, 0, 0])]),
        top(b"SOUN", &sounds),
        top(b"WEAP", &weapons),
        top(b"NPC_", &npcs),
        top(b"CELL", &cell),
    ]
    .concat();
    fnv_esm::Plugin::parse(&bytes, "OwnedWeaponSounds.esm".into()).unwrap()
}

fn authored_fx() -> WeaponFx {
    WeaponFx {
        shoot_3d: Some(SND_3D),
        shoot_dist: Some(SND_DIST),
        shoot_2d: Some(SND_2D),
        shoot_loop: Some(SND_LOOP),
        no_ammo: Some(SND_DRY),
        block: Some(SND_BLOCK),
        idle: Some(SND_IDLE),
        equip: Some(SND_EQUIP),
        unequip: Some(SND_UNEQUIP),
        mod_shoot_3d: Some(SND_MOD_3D),
        mod_shoot_dist: Some(SND_MOD_DIST),
        mod_shoot_2d: Some(SND_MOD_2D),
        ..WeaponFx::default()
    }
}

fn run_nvse(app: &mut App, body: &str) -> XVal {
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game
                .run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default())
                .unwrap_or_else(|e| panic!("NVSE `{body}` failed: {e}"))
        })
    })
}

fn drain_play_sounds(app: &mut App) -> Vec<PlaySound> {
    app.world_mut().resource_mut::<Messages<PlaySound>>().drain().collect()
}

fn drain_loop_sounds(app: &mut App) -> Vec<LoopSound> {
    app.world_mut().resource_mut::<Messages<LoopSound>>().drain().collect()
}

fn bridge_app() -> App {
    let plugins = Box::leak(vec![build_plugin()].into_boxed_slice());
    let script = ScriptState::over_plugins(plugins);
    let game = &script.rt.as_ref().unwrap().game;
    let (forms, refs) = (game.world.forms, game.world.placed);
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(
        fnv_esm::Schema::load_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")).unwrap(),
    ));

    let mut fx_forms = FxForms::default();
    for id in [WEAP_PISTOL, WEAP_RIFLE, WEAP_CLUB] {
        fx_forms.weapons.insert(id, authored_fx());
    }
    for (id, max_dist) in [
        (SND_3D, 1000.0),
        (SND_DIST, 4000.0),
        (SND_2D, 0.0),
        (SND_MOD_3D, 800.0),
        (SND_MOD_DIST, 3000.0),
        (SND_MOD_2D, 0.0),
        (OVR_3D, 1000.0),
        (OVR_DIST, 4000.0),
        (OVR_2D, 0.0),
        (OVR_MOD_3D, 800.0),
        (OVR_MOD_DIST, 3000.0),
        (OVR_MOD_2D, 0.0),
    ] {
        fx_forms.sounds.insert(id, Sound { max_distance: max_dist, ..Sound::default() });
    }

    let mut app = App::new();
    app.insert_resource(script)
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
        .insert_resource(Player {
            heading: 0.0,
            pitch: 0.0,
            eye_height: 64.0,
            feet: [0.0; 3],
            body_eye: None,
            body_view: None,
            placed: None,
        })
        .insert_resource(Sfx::new(fx_forms))
        .init_resource::<WeaponSoundSync>()
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
        .add_systems(Update, (script_weapon_sounds, weapon_sounds, fire, actor_attacks).chain());
    app.update();

    {
        let mut c = app.world_mut().resource_mut::<Combat>();
        let gun = |edid: &str| WeaponStats {
            edid: Some(edid.into()),
            projectile: Some(PROJ_BULLET),
            clip_size: 8,
            ammo_use: 1,
            fire_rate: 10.0,
            attack_shots_per_sec: 10.0,
            base_damage: 10,
            mods: vec![WeaponMod { slot: 1, item: Some(FormId(0x9001)), effect: Some("Silence".into()), value_a: 0.0, value_b: 0.0 }],
            ..WeaponStats::default()
        };
        c.forms.weapons.insert(WEAP_PISTOL, gun("BridgePistol"));
        c.forms.weapons.insert(WEAP_RIFLE, gun("BridgeRifle"));
        c.forms.weapons.insert(
            WEAP_CLUB,
            WeaponStats { edid: Some("BridgeClub".into()), base_damage: 10, ..WeaponStats::default() },
        );
        c.forms.projectiles.insert(PROJ_BULLET, ProjectileStats { speed: 2000.0, range: 5000.0, ..ProjectileStats::default() });
    }
    drain_play_sounds(&mut app);
    drain_loop_sounds(&mut app);
    app
}

#[test]
fn set_weapon_sound_drives_player_equip_unequip_idle_fire_dry_fire_and_block() {
    let mut app = bridge_app();

    // Override player pistol's equip (7), unequip (8), idle (6), shoot_2d (2), shoot_loop (3),
    // no_ammo (4), block (5), and mod_shoot_2d (11) through compiled NVSE SetWeaponSound.
    run_nvse(
        &mut app,
        "SetWeaponSound BridgePistol 7 OvrSndEquip\n\
         SetWeaponSound BridgePistol 8 OvrSndUnequip\n\
         SetWeaponSound BridgePistol 6 OvrSndIdle\n\
         SetWeaponSound BridgePistol 2 OvrSnd2D\n\
         SetWeaponSound BridgePistol 3 OvrSndLoop\n\
         SetWeaponSound BridgePistol 4 OvrSndDry\n\
         SetWeaponSound BridgePistol 5 OvrSndBlock\n\
         SetWeaponSound BridgePistol 11 OvrSndMod2D",
    );

    // Equip pistol and draw: equip + idle use overrides.
    {
        let mut c = app.world_mut().resource_mut::<Combat>();
        c.weapon = Some(WEAP_PISTOL);
        c.equipped = Some(Some(WEAP_PISTOL));
        c.mods = 0;
        let w = c.forms.weapons[&WEAP_PISTOL].clone();
        c.cycle = Some(Cycle::new(&w, Timing::of(&w, &ClipTiming::default(), &c.tuning), true, 8));
    }
    app.world_mut().resource_mut::<WeaponAnim>().ready = true;
    app.world_mut().resource_mut::<PlayerControl>().weapon_out = true;
    app.update();

    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    let loops = drain_loop_sounds(&mut app);
    assert_eq!(played, vec![OVR_EQUIP], "draw emits overridden equip sound");
    assert_eq!(loops.len(), 1);
    assert_eq!((loops[0].sound, loops[0].on), (OVR_IDLE, true), "draw starts overridden idle loop");

    // Player fires plain pistol: emits overridden shoot_2d and overridden shoot_loop.
    app.world_mut().resource_mut::<PlayerControl>().trigger = true;
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(0.05));
    app.update();

    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    let loops = drain_loop_sounds(&mut app);
    assert!(played.contains(&OVR_2D), "player shot emits overridden shoot_2d: {played:?}");
    assert!(loops.iter().any(|l| l.sound == OVR_LOOP && l.on), "player shot starts overridden shoot_loop");

    // Release trigger, enable Silence mod (slot 1 -> bit 0x1), fire again: emits overridden mod_shoot_2d.
    app.world_mut().resource_mut::<PlayerControl>().trigger = false;
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(0.25));
    app.update();
    drain_play_sounds(&mut app);
    drain_loop_sounds(&mut app);

    app.world_mut().resource_mut::<Combat>().mods = 1;
    app.world_mut().resource_mut::<PlayerControl>().trigger = true;
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(0.25));
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.contains(&OVR_MOD_2D), "silenced player shot emits overridden mod_shoot_2d: {played:?}");

    // Dry fire (empty magazine, ammo required): emits overridden no_ammo sound.
    app.world_mut().resource_mut::<PlayerControl>().trigger = false;
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(0.25));
    app.update();
    drain_play_sounds(&mut app);
    {
        let mut c = app.world_mut().resource_mut::<Combat>();
        let w = c.forms.weapons[&WEAP_PISTOL].clone();
        c.cycle = Some(Cycle::new(&w, Timing::of(&w, &ClipTiming::default(), &c.tuning), false, 0));
    }
    app.world_mut().resource_mut::<PlayerControl>().trigger = true;
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f32(0.05));
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.contains(&OVR_DRY), "dry fire emits overridden no_ammo sound: {played:?}");

    // Player block sound uses overridden slot 5; clearing slot 5 to 0 suppresses it.
    {
        let c = app.world().resource::<Combat>();
        let sim = app.world().resource::<SimState>();
        let sfx = app.world().resource::<Sfx>();
        assert_eq!(block_sound(c, sim, Some(sfx), FormId::PLAYER_REF).map(|p| p.sound), Some(OVR_BLOCK));
    }
    run_nvse(&mut app, "SetWeaponSound BridgePistol 5 0");
    app.world_mut().resource_mut::<PlayerControl>().trigger = false;
    app.update();
    {
        let c = app.world().resource::<Combat>();
        let sim = app.world().resource::<SimState>();
        let sfx = app.world().resource::<Sfx>();
        assert!(block_sound(c, sim, Some(sfx), FormId::PLAYER_REF).is_none(), "cleared block slot suppresses block sound");
    }

    // Clearing idle (slot 6) while weapon is drawn stops the active idle loop immediately.
    drain_loop_sounds(&mut app);
    run_nvse(&mut app, "SetWeaponSound BridgePistol 6 0");
    app.update();
    let loops = drain_loop_sounds(&mut app);
    assert!(loops.iter().any(|l| l.sound == OVR_IDLE && !l.on), "clearing idle stops active loop");

    // Holstering plays overridden unequip (slot 8).
    drain_play_sounds(&mut app);
    app.world_mut().resource_mut::<PlayerControl>().weapon_out = false;
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert_eq!(played, vec![OVR_UNEQUIP], "holster emits overridden unequip sound");

    // Unmodified sibling weapon (BridgeRifle) keeps its static authored WEAP sounds.
    {
        let mut c = app.world_mut().resource_mut::<Combat>();
        c.weapon = Some(WEAP_RIFLE);
        c.equipped = Some(Some(WEAP_RIFLE));
        c.mods = 0;
    }
    app.world_mut().resource_mut::<PlayerControl>().weapon_out = true;
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    let loops = drain_loop_sounds(&mut app);
    assert_eq!(played, vec![SND_EQUIP], "unmodified weapon preserves static authored equip sound");
    assert!(loops.iter().any(|l| l.sound == SND_IDLE && l.on), "unmodified weapon preserves static authored idle sound");
}

#[test]
fn set_weapon_sound_drives_npc_near_distant_silenced_melee_and_block_sounds() {
    let mut app = bridge_app();

    // Override only BridgePistol's 3D/dist/mod-3D/mod-dist/block slots (0, 1, 9, 10, 5)
    // and BridgeClub's melee shoot_3d (0). Leave BridgeRifle completely unmodified.
    run_nvse(
        &mut app,
        "SetWeaponSound BridgePistol 0 OvrSnd3D\n\
         SetWeaponSound BridgePistol 1 OvrSndDist\n\
         SetWeaponSound BridgePistol 9 OvrSndMod3D\n\
         SetWeaponSound BridgePistol 10 OvrSndModDist\n\
         SetWeaponSound BridgePistol 5 OvrSndBlock\n\
         SetWeaponSound BridgeClub 0 OvrSnd3D",
    );

    // 1. Near NPC shot with BridgePistol -> OVR_3D; near NPC shot with unmodified BridgeRifle -> static SND_3D.
    {
        let mut fire_msg = app.world_mut().resource_mut::<Messages<ActorFire>>();
        fire_msg.write(ActorFire {
            shooter: NPC_REF,
            weapon: WEAP_PISTOL,
            origin: [100.0, 0.0, 64.0],
            aim: [1.0, 0.0, 0.0],
            cone: 0.0,
            muzzle: None,
            harmless: true,
            sure: None,
        });
        fire_msg.write(ActorFire {
            shooter: NPC_REF,
            weapon: WEAP_RIFLE,
            origin: [100.0, 0.0, 64.0],
            aim: [1.0, 0.0, 0.0],
            cone: 0.0,
            muzzle: None,
            harmless: true,
            sure: None,
        });
    }
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.contains(&OVR_3D), "near NPC shot uses overridden shoot_3d: {played:?}");
    assert!(played.contains(&SND_3D), "unmodified NPC weapon keeps static shoot_3d: {played:?}");

    // 2. Distant NPC shot (> 1000.0 max_distance) with BridgePistol -> OVR_DIST.
    app.world_mut().resource_mut::<Messages<ActorFire>>().write(ActorFire {
        shooter: NPC_REF,
        weapon: WEAP_PISTOL,
        origin: [2500.0, 0.0, 64.0],
        aim: [1.0, 0.0, 0.0],
        cone: 0.0,
        muzzle: None,
        harmless: true,
        sure: None,
    });
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.contains(&OVR_DIST), "distant NPC shot uses overridden shoot_dist: {played:?}");

    // 3. Equip silenced BridgePistol (mod bit 0x1) on NPC: near -> OVR_MOD_3D, distant -> OVR_MOD_DIST,
    //    and NPC block_sound -> OVR_BLOCK.
    {
        let mut sim = app.world_mut().resource_mut::<SimState>();
        sim.world.add_item(NPC_REF, WEAP_PISTOL, 1).unwrap();
        sim.world.equip(NPC_REF, WEAP_PISTOL).unwrap();
        sim.world
            .get(NPC_REF)
            .unwrap()
            .inventory
            .as_mut()
            .unwrap()
            .map_equipped(WEAP_PISTOL, |mut e| {
                e.mods = 1;
                e
            });
    }
    {
        let mut fire_msg = app.world_mut().resource_mut::<Messages<ActorFire>>();
        fire_msg.write(ActorFire {
            shooter: NPC_REF,
            weapon: WEAP_PISTOL,
            origin: [100.0, 0.0, 64.0],
            aim: [1.0, 0.0, 0.0],
            cone: 0.0,
            muzzle: None,
            harmless: true,
            sure: None,
        });
        fire_msg.write(ActorFire {
            shooter: NPC_REF,
            weapon: WEAP_PISTOL,
            origin: [2500.0, 0.0, 64.0],
            aim: [1.0, 0.0, 0.0],
            cone: 0.0,
            muzzle: None,
            harmless: true,
            sure: None,
        });
    }
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.contains(&OVR_MOD_3D), "silenced near NPC shot uses overridden mod_shoot_3d: {played:?}");
    assert!(played.contains(&OVR_MOD_DIST), "silenced distant NPC shot uses overridden mod_shoot_dist: {played:?}");
    {
        let c = app.world().resource::<Combat>();
        let sim = app.world().resource::<SimState>();
        let sfx = app.world().resource::<Sfx>();
        assert_eq!(block_sound(c, sim, Some(sfx), NPC_REF).map(|p| p.sound), Some(OVR_BLOCK));
    }

    // 4. Clearing both shoot_3d (0) and shoot_2d (2) on BridgeClub silences NPC melee strike;
    //    before clearing, NPC melee strike with BridgeClub emits OVR_3D.
    app.world_mut().resource_mut::<Messages<ActorStrike>>().write(ActorStrike {
        attacker: NPC_REF,
        target: FormId::PLAYER_REF,
        weapon: Some(WEAP_CLUB),
        creature_damage: None,
        point: [32.0, 0.0, 64.0],
        dir: [-1.0, 0.0, 0.0],
        anim: None,
    });
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.contains(&OVR_3D), "NPC melee strike emits overridden shoot_3d: {played:?}");

    run_nvse(&mut app, "SetWeaponSound BridgeClub 0 0\nSetWeaponSound BridgeClub 2 0");
    app.world_mut().resource_mut::<Messages<ActorStrike>>().write(ActorStrike {
        attacker: NPC_REF,
        target: FormId::PLAYER_REF,
        weapon: Some(WEAP_CLUB),
        creature_damage: None,
        point: [32.0, 0.0, 64.0],
        dir: [-1.0, 0.0, 0.0],
        anim: None,
    });
    app.update();
    let played: Vec<FormId> = drain_play_sounds(&mut app).into_iter().map(|p| p.sound).collect();
    assert!(played.is_empty(), "clearing melee weapon shoot_3d and shoot_2d suppresses strike sound: {played:?}");
}
