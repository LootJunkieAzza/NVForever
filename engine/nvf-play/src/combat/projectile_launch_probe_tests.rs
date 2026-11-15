//! Authored portable records and real Game queries through actor_attacks -> fly.
//! CPU source/consumer controls; no installed script, native code or graphics assets.
use super::*;
use fnv_esm::raw::{Subrecord, write_subrecords};
use fnv_game::game::RunCtx;
use fnv_script::vm::XVal;
use fnv_sim::{World, SimConfig};

const GUN: FormId = FormId(0x900);
const NEIGHBOR_GUN: FormId = FormId(0x901);
const AMMO: FormId = FormId(0x902);
const PROJ: FormId = FormId(0x904);
const NEIGHBOR: FormId = FormId(0x905);
const CELL: FormId = FormId(0x906);

fn sub(sig: &[u8; 4], data: Vec<u8>) -> Subrecord { Subrecord { sig: *sig, data, extended: false } }
fn z(text: &str) -> Vec<u8> { [text.as_bytes(), &[0]].concat() }
fn record(sig: &[u8; 4], flags: u32, id: u32, subs: &[Subrecord]) -> Vec<u8> {
    let body = write_subrecords(subs);
    [&sig[..], &(body.len() as u32).to_le_bytes(), &flags.to_le_bytes(), &id.to_le_bytes(), &[0; 4], &15u16.to_le_bytes(), &[0; 2], &body].concat()
}
fn group(sig: &[u8; 4], body: &[u8]) -> Vec<u8> {
    [&b"GRUP"[..], &((24 + body.len()) as u32).to_le_bytes(), &sig[..], &0i32.to_le_bytes(), &[0; 8], body].concat()
}
fn weapon(id: FormId, name: &str, ammo: Option<FormId>, projectile: FormId) -> Vec<u8> {
    // Schema WeaponDNAM: animation type at0, projectile at36, count at42.
    let mut dnam = vec![0; 204];
    dnam[..4].copy_from_slice(&10u32.to_le_bytes());
    dnam[36..40].copy_from_slice(&projectile.0.to_le_bytes());
    dnam[42] = 1;
    let mut subs = vec![sub(b"EDID", z(name)), sub(b"DNAM", dnam), sub(b"DATA", [0i32.to_le_bytes().to_vec(), 100i32.to_le_bytes().to_vec(), 1f32.to_le_bytes().to_vec(), 10i16.to_le_bytes().to_vec(), vec![1]].concat())];
    if let Some(a) = ammo { subs.push(sub(b"NAM0", a.0.to_le_bytes().to_vec())); }
    record(b"WEAP", 0, id.0, &subs)
}
fn projectile(id: FormId, name: &str, speed: f32, force: f32) -> Vec<u8> {
    // Schema ProjectileData: flags/type at0/2, speed at8, range at12, force at52.
    let mut data = vec![0; 84];
    data[2..4].copy_from_slice(&1u16.to_le_bytes());
    data[8..12].copy_from_slice(&speed.to_le_bytes());
    data[12..16].copy_from_slice(&5000f32.to_le_bytes());
    data[52..56].copy_from_slice(&force.to_le_bytes());
    record(b"PROJ", 0, id.0, &[sub(b"EDID", z(name)), sub(b"DATA", data)])
}
fn plugin() -> fnv_esm::Plugin {
    let header = record(b"TES4", 1, 0, &[sub(b"HEDR", [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x1000u32.to_le_bytes()].concat())]);
    let player = record(b"NPC_", 0, FormId::PLAYER_BASE.0, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let weapons = [weapon(GUN, "OwnedGun", Some(AMMO), NEIGHBOR), weapon(NEIGHBOR_GUN, "NeighborGun", None, NEIGHBOR)].concat();
    let ammo = record(b"AMMO", 0, AMMO.0, &[sub(b"EDID", z("OwnedAmmo")), sub(b"DAT2", [1u32.to_le_bytes().to_vec(), PROJ.0.to_le_bytes().to_vec(), 0f32.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(), 1f32.to_le_bytes().to_vec()].concat())]);
    let cell = record(b"CELL", 0, CELL.0, &[sub(b"EDID", z("OwnedCell")), sub(b"DATA", vec![1])]);
    let projectiles = [projectile(PROJ, "OwnedProjectile", 1000.0, 10.0), projectile(NEIGHBOR, "NeighborProjectile", 1500.0, 20.0)].concat();
    fnv_esm::Plugin::parse(&[header, group(b"NPC_", &player), group(b"WEAP", &weapons), group(b"AMMO", &ammo), group(b"PROJ", &projectiles), group(b"CELL", &cell)].concat(), "OwnedLaunch.esm".into()).unwrap()
}
fn app(scratch: &Scratch) -> App {
    let plugins = Box::leak(vec![plugin()].into_boxed_slice());
    let state = ScriptState::over_plugins(plugins);
    let game = &state.rt.as_ref().unwrap().game;
    let (forms, refs) = (game.world.forms, game.world.placed);
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(fnv_esm::Schema::load_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")).unwrap()));
    let mut app = App::new();
    app.insert_resource(state)
        .insert_resource(SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) })
        .insert_resource(PlayWorld(crate::PlayData { lo, schema, index: Box::leak(Box::default()), world: 0, interior: None, eye: [0.0; 3], heading_deg: 0.0, pitch_deg: 0.0, auto_walk: 0.0, start_at_cell_spot: false, packages: false }))
        .init_resource::<Time>().init_resource::<PhysState>().init_resource::<CombatStats>()
        .init_resource::<HitFeedback>().init_resource::<ActorParts>().init_resource::<WeaponAnim>()
        .init_resource::<ShotVisuals>().init_resource::<crate::dismember::Dismemberment>()
        .init_resource::<held_grenade_probe::Probe>()
        .init_resource::<Time<Real>>()
        .insert_resource(SaveConfig::with_dir(Some(scratch.home.join("saves"))))
        .insert_resource(Probe::configured("OwnedGun".into(), scratch.home.clone(), &scratch.root).unwrap())
        .add_message::<AppExit>()
        .add_message::<ActorFire>().add_message::<ActorStrike>().add_message::<ShotFired>()
        .add_message::<PlaySound>().add_message::<crate::sfx::ScaledSound>().add_message::<ActorHit>().add_message::<ActorPush>()
        .add_message::<ImpactFx>().add_message::<WeaponBroken>().add_message::<Knockback>()
        .add_message::<ExplosionFx>().add_message::<crate::world_sounds::ImpactPicked>()
        .add_message::<crate::RefChanged>()
        .add_systems(Startup, init_combat).add_systems(Update, (drive, actor_attacks, fly).chain());
    app.add_systems(Last, finish);
    app.update();
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game.world.get(FormId::PLAYER_REF).unwrap().cell = Some(CELL);
            rt.game.player_entered(Some(CELL));
        });
    });
    query(&mut app, "Player.SetPos X 10\nPlayer.SetPos Y 20\nPlayer.SetPos Z 30\nPlayer.AddItem OwnedGun 20 1");
    app
}
fn query(app: &mut App, body: &str) -> XVal {
    app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| rt.game.run_query(&format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default()).unwrap_or_else(|e| panic!("authored source query failed: {body}: {e}")))
    })
}
fn tick(app: &mut App) {
    app.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f64(1.0 / 60.0));
    app.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs_f64(1.0));
    app.update();
}
fn typed_unchanged(app: &App) {
    let c = app.world().resource::<Combat>();
    assert_eq!((c.forms.projectiles[&PROJ].speed, c.forms.projectiles[&PROJ].impact_force), (1000.0, 10.0));
    assert_eq!((c.forms.projectiles[&NEIGHBOR].speed, c.forms.projectiles[&NEIGHBOR].impact_force), (1500.0, 20.0));
}


struct Scratch { root: std::path::PathBuf, home: std::path::PathBuf }
impl Scratch {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!("nvf-launch-smoke-{}-{n}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        let home = root.join(HOME); std::fs::create_dir(&home).unwrap();
        Self { root, home }
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = std::fs::remove_dir(&self.home); let _ = std::fs::remove_dir(&self.root); }
}
fn probe_state(app: &App) -> String {
    let p = app.world().resource::<Probe>();
    let c = app.world().resource::<Combat>();
    let flights: Vec<_> = c.flying.iter().take(4)
        .map(|f| (f.id, f.proj, f.p.speed, f.p.vel, f.force, f.by.weapon, f.by.ammo)).collect();
    let queued = app.world().resource::<ScriptState>().rt.as_ref().map(|rt| rt.game.combat_requests.len());
    format!("probe={p:?}; game_requests={queued:?}; script_shots={}; flights_total={}; first_four_flights={flights:?}", c.script_shots.len(), c.flying.len())
}
fn completed(a: &mut App) {
    for _ in 0..30 { tick(a); if a.world().resource::<Probe>().stage == Stage::Done { break; } }
}
#[test]
fn actual_script_requests_reach_two_physical_insertions_before_cleanup_pass() {
    let scratch = Scratch::new(); let mut a = app(&scratch); completed(&mut a);
    let p = a.world().resource::<Probe>();
    assert!(p.passed && p.cleaned && p.failed.is_none(), "{}", probe_state(&a));
    assert_eq!((p.requests, p.accepted), (2, 2)); assert!(p.backup.is_none());
    let c = a.world().resource::<Combat>();
    assert!(c.flying.iter().any(|f| f.p.speed == 500.0 && f.force == 7.0));
    assert!(c.flying.iter().any(|f| f.p.speed == 1000.0 && f.force == 10.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
    typed_unchanged(&a);
}
#[test]
fn script_failure_after_real_inventory_edits_cleans_counts_and_effective_fields() {
    let scratch = Scratch::new(); let mut a = app(&scratch);
    for _ in 0..21 { tick(&mut a); }
    a.world_mut().resource_mut::<Probe>().fail("authored injected script setup error after staging"); tick(&mut a);
    let p = a.world().resource::<Probe>(); assert!(p.cleaned && !p.passed && p.failed.is_some(), "{}", probe_state(&a));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetProjectileTraitNumeric OwnedProjectile 2)"), XVal::Num(1000.0));
}
#[test]
fn physical_observer_failure_cannot_become_pass_and_still_cleans() {
    let scratch = Scratch::new(); let mut a = app(&scratch);
    for _ in 0..20 { tick(&mut a); }
    assert_eq!(a.world().resource::<Probe>().stage, Stage::FirstPending);
    // Deliberately corrupt only the independent expectation, not a real projectile.
    a.world_mut().resource_mut::<Probe>().backup.as_mut().unwrap().sentinel[0] = 333.0;
    tick(&mut a);
    let p = a.world().resource::<Probe>(); assert!(p.failed.is_some() && p.cleaned && !p.passed, "{}", probe_state(&a));
    assert!(a.world().resource::<Combat>().flying.iter().any(|f| f.p.speed == 500.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
}
#[test]
fn active_timeout_cleans_without_dispatching_a_second_shot() {
    let scratch = Scratch::new(); let mut a = app(&scratch);
    for _ in 0..20 { tick(&mut a); }
    a.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs(20));
    a.update();
    let p = a.world().resource::<Probe>(); assert!(!p.passed && p.cleaned && p.failed.is_some(), "{}", probe_state(&a)); assert!(p.requests <= 1);
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
}
#[test]
fn exit_cleanup_runs_before_a_pending_probe_can_pass() {
    let scratch = Scratch::new(); let mut a = app(&scratch);
    for _ in 0..20 { tick(&mut a); }
    a.world_mut().write_message(AppExit::Success); tick(&mut a);
    let p = a.world().resource::<Probe>(); assert!(!p.passed && p.cleaned && p.failed.is_some(), "{}", probe_state(&a));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
}
#[test]
fn actual_configuration_refuses_foreign_home_existing_saves_and_source_injection() {
    let scratch = Scratch::new();
    assert!(Probe::configured("OwnedGun".into(), scratch.root.clone(), &scratch.root).is_err());
    assert!(Probe::configured("OwnedGun\nPlayer.KillActor".into(), scratch.home.clone(), &scratch.root).is_err());
    std::fs::create_dir(scratch.home.join("saves")).unwrap();
    std::fs::write(scratch.home.join("saves/owned.nvfsave"), b"authored marker only").unwrap();
    assert!(Probe::configured("OwnedGun".into(), scratch.home.clone(), &scratch.root).is_err());
    std::fs::remove_file(scratch.home.join("saves/owned.nvfsave")).unwrap(); std::fs::remove_dir(scratch.home.join("saves")).unwrap();
}


#[test]
fn generated_form_expressions_preserve_getter_and_vanilla_statement_contexts() {
    fn result(app: &mut App, body: &str) -> Result<XVal, String> {
        app.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
            let mut sim = world.resource_mut::<SimState>();
            with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| rt.game.run_query(
                &format!("Begin Function {{}}\n{body}\nEnd\n"), RunCtx::default()))
        })
    }
    let scratch = Scratch::new(); let mut a = app(&scratch);
    for id in [GUN, NEIGHBOR_GUN, AMMO, PROJ, NEIGHBOR, CELL] {
        assert_eq!(query(&mut a, &format!("SetFunctionValue {}", source_form(id))), XVal::Ref(id.0), "exact loaded form {id}");
    }
    assert_eq!(query(&mut a, "SetFunctionValue (GetAmmoProjectile 0x00000902)"), XVal::Ref(PROJ.0), "actual R2 counterevidence: extended hexadecimal getter arguments work");
    assert_eq!(query(&mut a, &format!("SetFunctionValue (GetAmmoProjectile {})", source_form(AMMO))), XVal::Ref(PROJ.0));
    assert_eq!(query(&mut a, &format!("SetFunctionValue (GetWeaponProjectile {})", source_form(GUN))), XVal::Ref(NEIGHBOR.0));
    assert_eq!(query(&mut a, &format!("SetFunctionValue {}", source_form(FormId(0x00AB_CDEF)))), XVal::Ref(0), "missing forms remain absent");
    let original_staging = "Player.AddItem 0x00000900 2 1\nPlayer.AddItem 0x00000902 2 1\nSetProjectileTraitNumeric 0x00000904 2 500\nSetProjectileTraitNumeric 0x00000904 9 7\nPlayer.FireWeapon 0x00000900\nPlayer.RemoveItem 0x00000900 1 1";
    for body in ["Player.AddItem 0x00000900 2 1", "Player.RemoveItem 0x00000900 1 1", "Player.FireWeapon 0x00000900", original_staging] {
        let rejected = result(&mut a, body);
        assert!(rejected.is_err(), "bare vanilla statement must be distinguished from an extended getter: {body:?}: {rejected:?}");
        assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
        assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
        assert_eq!(query(&mut a, "SetFunctionValue (GetProjectileTraitNumeric OwnedProjectile 2)"), XVal::Num(1000.0));
        assert_eq!(query(&mut a, "SetFunctionValue (GetProjectileTraitNumeric OwnedProjectile 9)"), XVal::Num(10.0));
        assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.combat_requests.is_empty());
        let c = a.world().resource::<Combat>();
        assert!(c.script_shots.is_empty() && c.flying.is_empty(), "rejected source must create no physical work");
    }
    query(&mut a, &format!("Player.AddItem {} 2 1", source_form(GUN)));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(22.0));
    query(&mut a, &format!("Player.RemoveItem {} 2 1", source_form(GUN)));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
    query(&mut a, &format!("Player.FireWeapon {}", source_form(GUN)));
    assert_eq!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.combat_requests.len(), 1);
    tick(&mut a);
    assert_eq!(a.world().resource::<Combat>().script_shots.len(), 1);
    assert!(a.world().resource::<Combat>().flying.is_empty(), "retain next-frame scheduling");
    tick(&mut a);
    let c = a.world().resource::<Combat>();
    assert!(c.script_shots.is_empty());
    assert_eq!(c.flying.len(), 1);
    let f = &c.flying[0];
    assert_eq!((f.proj, f.p.speed, f.force, f.by.weapon, f.by.ammo), (PROJ, 1000.0, 10.0, GUN, Some(AMMO)), "real corrected FireWeapon consumer");
    let p = a.world().resource::<Probe>();
    assert_eq!((p.stage, p.requests, p.accepted), (Stage::Waiting, 0, 0), "this control does not fabricate probe acceptance");
    typed_unchanged(&a);
}

#[test]
fn waiting_frame_cap_accepts_7200_then_aborts_7201_without_staging() {
    assert_eq!(MAX_WAIT_FRAMES, 7200);
    let scratch = Scratch::new();
    let mut a = app(&scratch);
    {
        let now = a.world().resource::<Time<Real>>().elapsed_secs_f64();
        let p = a.world().resource::<Probe>();
        assert_eq!(p.stage, Stage::Waiting);
        assert!(now - p.started.unwrap() < 20.0 && now - p.started.unwrap() < WAIT_SECONDS);
        assert!(p.backup.is_none());
    }
    a.world_mut().resource_mut::<Probe>().frames = MAX_WAIT_FRAMES - 1;
    a.update();
    {
        let p = a.world().resource::<Probe>();
        assert_eq!((p.frames, p.stage, p.requests, p.accepted), (7200, Stage::Waiting, 0, 0));
        assert!(p.failed.is_none() && p.backup.is_none() && !p.cleaned && !p.passed, "{}", probe_state(&a));
        let c = a.world().resource::<Combat>();
        assert!(c.script_shots.is_empty() && c.flying.is_empty());
        let game = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
        assert!(game.combat_requests.is_empty());
        let fields = game.projectile_launch_traits(PROJ).unwrap();
        assert_eq!((fields.speed, fields.impact_force), (None, None));
    }
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
    // At the over-limit update preparation would otherwise become eligible.
    // It still precedes the independent waiting timeout; no active capsule exists.
    a.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs(20));
    {
        let now = a.world().resource::<Time<Real>>().elapsed_secs_f64();
        let p = a.world().resource::<Probe>();
        assert!(now - p.started.unwrap() >= 20.0 && now - p.started.unwrap() < WAIT_SECONDS);
        assert!(p.backup.is_none());
    }
    a.update();
    {
        let p = a.world().resource::<Probe>();
        assert_eq!((p.frames, p.stage, p.requests, p.accepted), (7201, Stage::Done, 0, 0));
        assert_eq!(p.failed.as_deref(), Some("bounded wait/stage deadline"));
        assert!(p.cleaned && !p.passed && p.backup.is_none(), "{}", probe_state(&a));
        assert_eq!(p.cleanup_attempts, 0, "waiting acquired no capsule to restore");
    }
    a.update();
    {
        let p = a.world().resource::<Probe>();
        assert_eq!((p.frames, p.stage, p.requests, p.accepted), (7201, Stage::Done, 0, 0));
        let c = a.world().resource::<Combat>();
        assert!(c.script_shots.is_empty() && c.flying.is_empty());
        assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.combat_requests.is_empty());
        let stats = a.world().resource::<CombatStats>();
        assert_eq!((stats.shots, stats.projectiles, stats.actor_shots), (0, 0, 0));
    }
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
    typed_unchanged(&a);
}

#[test]
fn active_frame_cap_accepts_300_then_aborts_301_cleans_without_second_fire() {
    assert_eq!(MAX_ACTIVE_FRAMES, 300);
    let scratch = Scratch::new();
    let mut a = app(&scratch);
    // One real staging update, not hundreds of simulated active frames.
    a.world_mut().resource_mut::<Time<Real>>().advance_by(std::time::Duration::from_secs(20));
    a.update();
    {
        let p = a.world().resource::<Probe>();
        assert_eq!((p.stage, p.requests, p.accepted), (Stage::FirstPending, 0, 0));
        assert!(p.failed.is_none() && p.backup.is_some(), "{}", probe_state(&a));
        assert_eq!(a.world().resource::<Combat>().script_shots.len(), 1);
        assert!(a.world().resource::<Combat>().flying.is_empty());
    }
    {
        let mut p = a.world_mut().resource_mut::<Probe>();
        p.frames = p.armed_frame + MAX_ACTIVE_FRAMES - 1;
    }
    tick(&mut a);
    let (first_visual, counters, armed_frame) = {
        let now = a.world().resource::<Time<Real>>().elapsed_secs_f64();
        let p = a.world().resource::<Probe>();
        assert!(now - p.armed_at < ACTIVE_SECONDS && now - p.started.unwrap() < WAIT_SECONDS);
        assert_eq!(p.frames - p.armed_frame, 300);
        assert!(p.frames < MAX_WAIT_FRAMES);
        assert_eq!((p.stage, p.requests, p.accepted), (Stage::Restore, 1, 1));
        assert!(p.failed.is_none() && p.backup.is_some() && !p.cleaned && !p.passed, "{}", probe_state(&a));
        let c = a.world().resource::<Combat>();
        assert!(c.script_shots.is_empty());
        assert_eq!(c.flying.len(), 1);
        let f = &c.flying[0];
        assert_eq!((f.proj, f.p.speed, f.force), (PROJ, 500.0, 7.0));
        assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.combat_requests.is_empty());
        let stats = a.world().resource::<CombatStats>();
        (f.id, (stats.shots, stats.projectiles, stats.actor_shots), p.armed_frame)
    };
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(21.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(2.0));
    tick(&mut a);
    {
        let now = a.world().resource::<Time<Real>>().elapsed_secs_f64();
        let p = a.world().resource::<Probe>();
        assert!(now - p.armed_at < ACTIVE_SECONDS && now - p.started.unwrap() < WAIT_SECONDS);
        assert_eq!(p.frames - armed_frame, 301);
        assert_eq!((p.stage, p.requests, p.accepted), (Stage::Done, 1, 1));
        assert_eq!(p.failed.as_deref(), Some("bounded wait/stage deadline"));
        assert!(p.cleaned && !p.passed && p.backup.is_none(), "{}", probe_state(&a));
        assert_eq!(p.cleanup_attempts, 1);
        let game = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
        assert!(game.combat_requests.is_empty());
        let fields = game.projectile_launch_traits(PROJ).unwrap();
        assert_eq!((fields.speed, fields.impact_force), (None, None), "restore absence, not equal-valued replacement overrides");
    }
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedGun)"), XVal::Num(20.0));
    assert_eq!(query(&mut a, "SetFunctionValue (Player.GetItemCount OwnedAmmo)"), XVal::Num(0.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetProjectileTraitNumeric OwnedProjectile 2)"), XVal::Num(1000.0));
    assert_eq!(query(&mut a, "SetFunctionValue (GetProjectileTraitNumeric OwnedProjectile 9)"), XVal::Num(10.0));
    tick(&mut a);
    {
        let p = a.world().resource::<Probe>();
        assert_eq!((p.frames - armed_frame, p.requests, p.accepted, p.cleanup_attempts), (301, 1, 1, 1));
        let c = a.world().resource::<Combat>();
        assert!(c.script_shots.is_empty());
        assert_eq!(c.flying.len(), 1);
        assert_eq!((c.flying[0].id, c.flying[0].p.speed, c.flying[0].force), (first_visual, 500.0, 7.0));
        assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.combat_requests.is_empty());
        let stats = a.world().resource::<CombatStats>();
        assert_eq!((stats.shots, stats.projectiles, stats.actor_shots), counters);
    }
    typed_unchanged(&a);
}
