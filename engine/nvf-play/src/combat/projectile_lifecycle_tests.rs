//! Authored records and compiled public calls through the real flight consumers.
//! No installed script, injected Flying/Pending/Ghost, renderer or native execution.
use super::*;

const BLAST: FormId = FormId(0x908);

fn lifecycle_projectile(id: FormId, name: &str, flags: u16, kind: u16, range: f32) -> Vec<u8> {
    let mut data = vec![0; 84];
    data[0..2].copy_from_slice(&flags.to_le_bytes());
    data[2..4].copy_from_slice(&kind.to_le_bytes());
    data[8..12].copy_from_slice(&1000f32.to_le_bytes());
    data[12..16].copy_from_slice(&range.to_le_bytes());
    data[24..28].copy_from_slice(&1f32.to_le_bytes()); // actual tracer/visual admission
    data[32..36].copy_from_slice(&30f32.to_le_bytes());
    if flags & 2 != 0 {
        data[36..40].copy_from_slice(&BLAST.0.to_le_bytes());
    }
    data[48..52].copy_from_slice(&0.5f32.to_le_bytes());
    record(b"PROJ", 0, id.0, &[sub(b"EDID", z(name)), sub(b"DATA", data)])
}

fn lifecycle_plugin(flags: u16, kind: u16, range: f32) -> fnv_esm::Plugin {
    let header = record(b"TES4", 1, 0, &[sub(b"HEDR", [1.34f32.to_le_bytes(), 0u32.to_le_bytes(), 0x1000u32.to_le_bytes()].concat())]);
    let player = record(b"NPC_", 0, FormId::PLAYER_BASE.0, &[sub(b"EDID", z("Player")), sub(b"DATA", [100i32.to_le_bytes().to_vec(), vec![5; 7]].concat())]);
    let weapons = [weapon(GUN, "OwnedGun", Some(AMMO), PROJ), weapon(NEIGHBOR_GUN, "NeighborGun", None, NEIGHBOR)].concat();
    let ammo = record(b"AMMO", 0, AMMO.0, &[sub(b"EDID", z("OwnedAmmo")), sub(b"DAT2", [1u32.to_le_bytes().to_vec(), PROJ.0.to_le_bytes().to_vec(), 0f32.to_le_bytes().to_vec(), 0u32.to_le_bytes().to_vec(), 1f32.to_le_bytes().to_vec()].concat())]);
    let cell = record(b"CELL", 0, CELL.0, &[sub(b"EDID", z("OwnedCell")), sub(b"DATA", vec![1])]);
    let projectiles = [lifecycle_projectile(PROJ, "OwnedProjectile", flags, kind, range), lifecycle_projectile(NEIGHBOR, "NeighborProjectile", flags, kind, range)].concat();
    // ExplosionData is thirteen four-byte fields. A zero-radius/damage authored
    // explosion still reaches the real explosion counter without hitting an actor.
    let explosion = record(b"EXPL", 0, BLAST.0, &[sub(b"EDID", z("OwnedExplosion")), sub(b"DATA", vec![0; 52])]);
    fnv_esm::Plugin::parse(&[header, group(b"NPC_", &player), group(b"WEAP", &weapons), group(b"AMMO", &ammo), group(b"PROJ", &projectiles), group(b"EXPL", &explosion), group(b"CELL", &cell)].concat(), "OwnedLifecycle.esm".into()).unwrap()
}

fn lifecycle_app(flags: u16, kind: u16, range: f32) -> App {
    let plugins = Box::leak(vec![lifecycle_plugin(flags, kind, range)].into_boxed_slice());
    let state = ScriptState::over_plugins(plugins);
    let game = &state.rt.as_ref().unwrap().game;
    let (forms, refs) = (game.world.forms, game.world.placed);
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(fnv_esm::Schema::load_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")).unwrap()));
    let mut a = App::new();
    a.insert_resource(state)
        .insert_resource(SimState { forms, refs, world: World::new(forms, refs, SimConfig::default()) })
        .insert_resource(PlayWorld(crate::PlayData { lo, schema, index: Box::leak(Box::default()), world: 0, interior: None, eye: [0.0; 3], heading_deg: 0.0, pitch_deg: 0.0, auto_walk: 0.0, start_at_cell_spot: false, packages: false }))
        .insert_resource(Player { heading: 0.0, pitch: 0.0, eye_height: 64.0, feet: [10.0, 20.0, 30.0], body_eye: None, body_view: None, placed: None })
        .init_resource::<PlayerControl>().init_resource::<PlayerLimbs>()
        .init_resource::<Time>().init_resource::<PhysState>().init_resource::<CombatStats>()
        .init_resource::<HitFeedback>().init_resource::<ActorParts>().init_resource::<WeaponAnim>()
        .init_resource::<ShotVisuals>().init_resource::<crate::dismember::Dismemberment>()
        .init_resource::<held_grenade_probe::Probe>()
        .add_message::<ActorFire>().add_message::<ActorStrike>().add_message::<ShotFired>()
        .add_message::<PlaySound>().add_message::<crate::sfx::ScaledSound>().add_message::<ActorHit>().add_message::<ActorPush>()
        .add_message::<ImpactFx>().add_message::<WeaponBroken>().add_message::<Knockback>()
        .add_message::<ExplosionFx>().add_message::<crate::world_sounds::ImpactPicked>()
        .add_message::<crate::RefChanged>().add_message::<crate::Travel>()
        .add_message::<CasingFx>().add_message::<LoopSound>()
        .add_systems(Startup, init_combat)
        .add_systems(Update, (super::super::fire, actor_attacks, fly, leave_space_shots).chain());
    a.update();
    a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game.world.get(FormId::PLAYER_REF).unwrap().cell = Some(CELL);
            rt.game.player_entered(Some(CELL));
        });
    });
    query(&mut a, "Player.SetPos X 10\nPlayer.SetPos Y 20\nPlayer.SetPos Z 30\nPlayer.AddItem OwnedGun 20 1");
    let c = a.world().resource::<Combat>();
    assert_eq!(c.forms.projectiles[&PROJ].speed, 1000.0);
    assert_eq!(c.forms.projectiles[&PROJ].has("Hitscan"), flags & 1 != 0);
    assert_eq!(c.forms.projectiles[&PROJ].has("Detonates"), flags & 1024 != 0);
    assert!(c.forms.explosions.contains_key(&BLAST));
    a
}

fn step(a: &mut App, seconds: f64) {
    a.world_mut().resource_mut::<Time>().advance_by(std::time::Duration::from_secs_f64(seconds));
    a.update();
}

fn on(a: &mut App, r: FormId, body: &str) -> XVal {
    query(a, &format!("ref subject\nlet subject := StringToRef \"{:08X}\"\n{body}", r.0))
}

fn launch(a: &mut App, weapon: &str) -> (FormId, u64) {
    let before: std::collections::HashSet<_> = a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.gaps.projs.shots.keys().copied().collect();
    let previous = a.world().resource::<Combat>().next_visual;
    let count = a.world().resource::<CombatStats>().actor_shots;
    query(a, &format!("Player.FireWeapon {weapon}"));
    step(a, 1.0 / 60.0);
    assert_eq!(a.world().resource::<Combat>().script_shots.len(), 1, "actual current queued request");
    step(a, 1.0 / 60.0);
    let refs: Vec<_> = a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.gaps.projs.shots.keys().filter(|r| !before.contains(r)).copied().collect();
    assert_eq!(refs.len(), 1, "actual registered shot, not an injected flight");
    let c = a.world().resource::<Combat>();
    assert_eq!(c.next_visual, previous + 1);
    assert_eq!(a.world().resource::<CombatStats>().actor_shots, count + 1);
    assert!(c.flying.iter().any(|f| f.rref == Some(refs[0])) || c.pending.iter().any(|p| p.rref == Some(refs[0])));
    (refs[0], c.next_visual)
}

fn retired(a: &App, r: FormId, id: u64) {
    let c = a.world().resource::<Combat>();
    assert!(!c.flying.iter().any(|f| f.rref == Some(r)), "deleted reference still drives a physical flight");
    assert!(!c.pending.iter().any(|p| p.rref == Some(r)), "deleted reference still has a queued hit");
    assert!(!a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.id == id), "deleted projectile still published to renderer/sound");
    let game = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
    assert!(!game.gaps.projs.shots.contains_key(&r));
    assert!(game.gaps.projs.gone.contains(&r));
    assert!(!game.other.jip14.fuses.contains_key(&r));
    assert!(!game.other.jip14.fuse_sets.iter().any(|(x, _)| *x == r));
    assert!(a.world().resource::<SimState>().world.peek(r).unwrap().deleted());
}

#[test]
fn deleted_live_shot_retires_actual_flight_and_keeps_neighbor() {
    let mut a = lifecycle_app(2 | 4, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    let (other, other_id) = launch(&mut a, "NeighborGun");
    assert!(a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.id == id));
    on(&mut a, other, "subject.SetProjectileRefSpeedMult 0.5\nsubject.SetProjectileRefDamage 17\nsubject.SetProjectileRefSource Player");
    on(&mut a, r, "subject.SetDetonationTimer 7\nsubject.Disable\nsubject.MarkForDelete");
    assert!(a.world().resource::<SimState>().world.peek(r).unwrap().deleted());
    let prior = a.world().resource::<Combat>().flying.iter().find(|f| f.rref == Some(other)).unwrap().p.pos;
    step(&mut a, 1.0 / 60.0);
    retired(&a, r, id);
    let c = a.world().resource::<Combat>();
    let remaining = c.flying.iter().find(|f| f.rref == Some(other)).unwrap();
    assert_ne!(remaining.p.pos, prior);
    assert_eq!(remaining.p.speed, 500.0);
    assert!(a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.id == other_id));
    let game = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
    assert_eq!(game.gaps.projs.shots[&other].damage, 17.0);
    assert!(game.other.jip14.fuses.contains_key(&other));
    assert_eq!(a.world().resource::<CombatStats>().explosions, 0);
    step(&mut a, 1.0 / 60.0);
    retired(&a, r, id);
    assert_eq!(a.world().resource::<CombatStats>().actor_shots, 2);
    assert_eq!(a.world().resource::<CombatStats>().explosions, 0);
}

#[test]
fn queued_replacement_keeps_deleted_callers_captured_pose() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    let pose = a.world().resource::<SimState>().world.peek(r).unwrap().pos;
    on(&mut a, r, "subject.FireWeapon NeighborGun\nsubject.Disable\nsubject.MarkForDelete");
    step(&mut a, 1.0 / 60.0);
    retired(&a, r, id);
    assert_eq!(a.world().resource::<Combat>().script_shots.len(), 1);
    step(&mut a, 1.0 / 60.0);
    let c = a.world().resource::<Combat>();
    assert!(c.script_shots.is_empty());
    assert_eq!(c.flying.len(), 1);
    assert_eq!(c.flying[0].proj, NEIGHBOR);
    assert_eq!(c.flying[0].origin, pose);
    assert_ne!(c.flying[0].rref, Some(r));
    assert_eq!(a.world().resource::<CombatStats>().actor_shots, 2);
}

#[test]
fn disable_and_fade_remain_reversible_without_cancelling_physics() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, _) = launch(&mut a, "OwnedGun");
    let before = a.world().resource::<Combat>().flying[0].p.pos;
    on(&mut a, r, "subject.Disable");
    assert!(!a.world().resource::<SimState>().world.peek(r).unwrap().enabled());
    step(&mut a, 1.0 / 60.0);
    assert_eq!(a.world().resource::<Combat>().flying[0].rref, Some(r));
    assert_ne!(a.world().resource::<Combat>().flying[0].p.pos, before);
    on(&mut a, r, "subject.Enable\nsubject.Disable 1");
    assert!(a.world().resource::<SimState>().world.peek(r).unwrap().enabled());
    assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.script_looks.fading_out.contains_key(&r));
    on(&mut a, r, "subject.Enable");
    step(&mut a, 1.0 / 60.0);
    let game = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
    assert!(!game.script_looks.fading_out.contains_key(&r));
    assert!(game.gaps.projs.shots.contains_key(&r));
    assert!(a.world().resource::<SimState>().world.peek(r).unwrap().enabled());
    assert_eq!(a.world().resource::<Combat>().flying.len(), 1);
}

fn ground_and_down(a: &mut App) {
    a.world_mut().resource_mut::<PhysState>().world.add_terrain((0, 0), &vec![0.0; fnv_world::land::SIDE * fnv_world::land::SIDE]);
    query(a, "Player.SetAngle X 90");
}

#[test]
fn deleted_actual_hitscan_pending_and_ghost_cannot_deliver_later() {
    let mut a = lifecycle_app(1, 1, 5000.0);
    ground_and_down(&mut a);
    let (r, id) = launch(&mut a, "OwnedGun");
    let c = a.world().resource::<Combat>();
    assert!(c.flying.is_empty());
    assert_eq!(c.pending.len(), 1);
    assert!(c.pending[0].due > c.time);
    assert_eq!(c.ghosts.len(), 1);
    on(&mut a, r, "subject.MarkForDelete");
    step(&mut a, 0.2);
    assert_eq!(a.world().resource::<CombatStats>().world_hits, 0, "deleted pending shot must not deliver a physical hit");
    retired(&a, r, id);
    assert!(a.world().resource::<Combat>().ghosts.is_empty());
    assert_eq!(a.world().resource::<CombatStats>().actor_hits, 0);
}

#[test]
fn untouched_actual_hitscan_delivers_its_due_hit() {
    let mut a = lifecycle_app(1, 1, 5000.0);
    ground_and_down(&mut a);
    let (r, _) = launch(&mut a, "OwnedGun");
    assert_eq!(a.world().resource::<Combat>().pending.len(), 1);
    step(&mut a, 0.2);
    assert_eq!(a.world().resource::<CombatStats>().world_hits, 1);
    assert!(a.world().resource::<Combat>().pending.is_empty());
    let game = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
    assert!(game.gaps.projs.shots[&r].impact.is_some());
    assert!(!game.gaps.projs.gone.contains(&r));
}

#[test]
fn deleted_shot_cannot_fallback_to_weapon_damage_when_session_entry_is_missing() {
    let mut a = lifecycle_app(1, 1, 5000.0);
    ground_and_down(&mut a);
    let (r, id) = launch(&mut a, "OwnedGun");
    on(&mut a, r, "subject.SetDetonationTimer 0\nsubject.MarkForDelete");
    // Fault injection at the exact split-state boundary, after real flight/hit creation.
    // No fake Pending or hit is inserted; cleanup must tolerate the absent session row.
    a.world_mut().resource_mut::<ScriptState>().rt.as_mut().unwrap().game.gaps.projs.shots.remove(&r).unwrap();
    step(&mut a, 0.2);
    assert_eq!(a.world().resource::<CombatStats>().world_hits, 0, "missing session row must not revive a deleted pending hit");
    retired(&a, r, id);
}

#[test]
fn deletion_precedes_remote_detonation_without_suppressing_live_neighbor() {
    let mut a = lifecycle_app(2 | 1024, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    launch(&mut a, "NeighborGun");
    on(&mut a, r, "subject.MarkForDelete\nPlayer.DetonatePlacedExplosives");
    step(&mut a, 1.0 / 60.0);
    assert_eq!(a.world().resource::<CombatStats>().explosions, 1, "only the live neighbor detonates");
    retired(&a, r, id);
    assert!(a.world().resource::<Combat>().flying.is_empty());
}

#[test]
fn deleted_flight_does_not_erase_independent_queued_blast() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    on(&mut a, r, "subject.MarkForDelete\nPlayer.PlaceAtMe OwnedExplosion 1");
    assert!(a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.combat_requests.iter().any(|r| matches!(r, CombatRequest::Explode { explosion, .. } if *explosion == BLAST)));
    step(&mut a, 1.0 / 60.0);
    retired(&a, r, id);
    assert_eq!(a.world().resource::<CombatStats>().explosions, 1);
}

#[test]
fn natural_beam_completion_keeps_its_existing_fade() {
    let mut a = lifecycle_app(0, 4, 100.0);
    query(&mut a, "Player.FireWeapon OwnedGun");
    step(&mut a, 1.0 / 60.0);
    step(&mut a, 1.0 / 60.0);
    let c = a.world().resource::<Combat>();
    assert!(c.flying.is_empty() && c.pending.is_empty());
    assert_eq!(c.ghosts.len(), 1);
    let id = c.ghosts[0].id;
    assert!(!a.world().resource::<ScriptState>().rt.as_ref().unwrap().game.gaps.projs.gone.is_empty());
    step(&mut a, 0.1);
    assert!(a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.id == id));
    step(&mut a, 0.5);
    assert!(a.world().resource::<ShotVisuals>().flying.is_empty());
}

#[test]
fn natural_expiry_travel_and_no_runtime_keep_their_existing_boundaries() {
    let mut a = lifecycle_app(0, 1, 100.0);
    let (r, id) = launch(&mut a, "OwnedGun");
    step(&mut a, 0.2);
    retired(&a, r, id);
    let (other, other_id) = launch(&mut a, "NeighborGun");
    a.world_mut().resource_mut::<Messages<crate::Travel>>().write(crate::Travel { world: None, interior: Some(CELL.0), feet: [0.0; 3], heading_deg: 0.0 });
    step(&mut a, 1.0 / 60.0);
    retired(&a, other, other_id);
    let (last, _) = launch(&mut a, "OwnedGun");
    a.world_mut().remove_resource::<ScriptState>().unwrap();
    step(&mut a, 1.0 / 60.0);
    assert!(a.world().resource::<Combat>().flying.iter().any(|f| f.rref == Some(last)), "missing runtime is not a deletion request");
}

#[test]
fn actor_shot_without_script_runtime_still_creates_unregistered_flight() {
    let mut a = lifecycle_app(0, 1, 5000.0);
    a.world_mut().remove_resource::<ScriptState>().unwrap();
    a.world_mut().resource_mut::<Messages<ActorFire>>().write(ActorFire { shooter: FormId::PLAYER_REF, weapon: GUN, origin: [10.0, 20.0, 94.0], aim: [0.0, 1.0, 0.0], cone: 0.0, muzzle: None, harmless: false });
    step(&mut a, 1.0 / 60.0);
    let c = a.world().resource::<Combat>();
    assert_eq!(c.flying.len(), 1);
    assert!(!c.flying[0].registered && c.flying[0].rref.is_none());
    let (id, before) = (c.flying[0].id, c.flying[0].p.pos);
    step(&mut a, 1.0 / 60.0);
    let c = a.world().resource::<Combat>();
    assert_eq!(c.flying.len(), 1);
    assert_ne!(c.flying[0].p.pos, before);
    assert!(a.world().resource::<ShotVisuals>().flying.iter().any(|v| v.id == id));
}
