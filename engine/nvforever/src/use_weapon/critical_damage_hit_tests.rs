//! Real UseWeapon swing production into the public CombatPlugin and Game health.
//! Animation assets, renderer, native encoding and installed mods remain outside this test.
use super::*;
use fnv_game::game::RunCtx;
use fnv_script::vm::XVal;
use fnv_sim::{SimConfig, World};
use nvf_play::combat::{ActorHit, CombatPlugin};
use nvf_play::control::PlayerControl;
use nvf_play::hitvol::ActorParts;
use nvf_play::player::MoveIntent;
use nvf_play::{PlayData, PlayState, PlayWorld};
use std::sync::Arc;

#[path = "../../../test-fixtures/critical_hit_records.rs"]
mod records;
use records::*;

fn query(a: &mut App, body: &str) -> XVal {
    a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game
                .run_query(&format!("Begin Function {{}}\n{body}\nEnd"), RunCtx::default())
                .unwrap_or_else(|e| panic!("source {body}: {e}"))
        })
    })
}
// Public compiled getters read the actual JipHit recorded by Game::hit; a positive
// health-damage assertion also prevents the getters' absent-record defaults passing.
fn assert_jip_hit(a: &mut App, attacker: FormId, weapon: FormId, damage: f64, critical: bool) {
    let mut recorded = |getter: &str| {
        query(
            a,
            &format!(
                "ref struck\nlet struck := StringToRef \"{:08X}\"\nSetFunctionValue (struck.{getter})",
                TARGET.0
            ),
        )
    };
    let XVal::Num(health_damage) = recorded("GetHitHealthDamage") else {
        panic!("JIP hit health damage must be numeric")
    };
    assert!(health_damage > 0.0);
    assert!(
        (health_damage - damage).abs() < 0.001,
        "actual JIP recorder damage equals the health loss"
    );
    assert_eq!(recorded("GetHitExtendedFlag 1"), XVal::Num(f64::from(u8::from(critical))));
    assert_eq!(recorded("GetHitAttacker"), XVal::Ref(attacker.0));
    assert_eq!(recorded("GetHitWeapon"), XVal::Ref(weapon.0));
}
fn health(a: &mut App) -> f64 {
    let XVal::Num(h) = query(
        a,
        "ref target\nlet target := StringToRef \"00000A46\"\nSetFunctionValue (target.GetAV Health)",
    ) else {
        panic!("health query")
    };
    h
}
fn production_runtime(play: PlayData, forms: &'static fnv_sim::FormStore, refs: &'static fnv_sim::RefTable) -> (ScriptState, SimState) {
    let mut boot = App::new();
    boot.add_plugins(MinimalPlugins)
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .insert_resource(PlayWorld(play))
        .insert_resource(SimState {
            forms,
            refs,
            world: World::new(forms, refs, SimConfig::default()),
        })
        .insert_resource(nvf_play::Location {
            world: 0,
            interior: Some(CELL.0),
        })
        .insert_resource(State::new(PlayState::Playing))
        .init_resource::<NextState<PlayState>>()
        .insert_resource(Player {
            heading: 0.0,
            pitch: 0.0,
            eye_height: 64.0,
            feet: [500.0, 0.0, 0.0],
            body_eye: None,
            body_view: None,
            placed: None,
        })
        .init_resource::<PhysState>()
        .init_resource::<ButtonInput<KeyCode>>()
        .init_resource::<ButtonInput<MouseButton>>()
        .init_resource::<bevy::input::mouse::AccumulatedMouseScroll>()
        .init_resource::<nvf_play::bindings::Bindings>()
        .init_resource::<nvf_play::pause::PlayClock>()
        .init_resource::<nvf_play::pause::Paused>()
        .init_resource::<nvf_play::pause::Page>()
        .init_resource::<nvf_play::loading::Loading>()
        .init_resource::<nvf_play::tiles::mods::ModsPage>()
        .init_resource::<nvf_play::tutorial_menu::TutorialView>()
        .init_resource::<nvf_play::control::ControlLocks>()
        .init_resource::<nvf_play::control::LockStats>()
        .init_resource::<nvf_play::menu::ActiveMenu>()
        .init_resource::<nvf_play::wait::WaitMenu>()
        .init_resource::<nvf_play::terminal::TerminalScreen>()
        .init_resource::<nvf_play::lockpick::LockpickScreen>()
        .init_resource::<nvf_play::console::Console>()
        .init_resource::<nvf_play::poker::PokerScreen>()
        .init_resource::<nvf_play::credits::Credits>()
        .init_resource::<Assets<Image>>()
        .add_message::<bevy::input::mouse::MouseWheel>()
        .add_message::<nvf_play::tutorial_menu::TutorialInput>()
        .add_message::<nvf_play::RefChanged>()
        .add_message::<nvf_play::fade::RefFade>()
        .add_message::<nvf_play::RefRepose>()
        .add_message::<nvf_play::loading::TravelRequest>()
        .add_message::<nvf_play::OpenContainer>()
        .add_message::<nvf_play::ItemUsed>()
        .add_message::<nvf_play::furniture::UseFurniture>()
        .add_message::<nvf_play::terminal::OpenTerminal>()
        .add_message::<nvf_play::save::LoadRefused>()
        .add_message::<nvf_play::pause::MainMenuRequest>()
        // Keep the real public script loader and runtime-start systems. This bootstrap
        // has no rendered world; its UI drawing and model synchronization sets stay idle.
        .configure_sets(Update, nvf_play::PlaySet::Ui.run_if(|| false))
        .configure_sets(Update, nvf_play::PlaySet::Sync.run_if(|| false))
        .add_plugins(nvf_play::script::ScriptPlugin);
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(30);
    loop {
        boot.update();
        let state = boot.world().resource::<ScriptState>();
        assert!(state.off.is_none(), "production script loader failed: {:?}", state.off);
        if state.rt.is_some() {
            break;
        }
        assert!(std::time::Instant::now() < deadline, "production script runtime did not start");
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let script = boot.world_mut().remove_resource::<ScriptState>().unwrap();
    let sim = boot.world_mut().remove_resource::<SimState>().unwrap();
    (script, sim)
}
fn app() -> App {
    let plugins = Box::leak(vec![plugin()].into_boxed_slice());
    let lo = Box::leak(Box::new(fnv_esm::LoadOrder::new(plugins).unwrap()));
    let schema = Box::leak(Box::new(
        fnv_esm::Schema::load_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/fnv")).unwrap(),
    ));
    let forms = Box::leak(Box::new(fnv_sim::FormStore::build(lo, schema)));
    let refs = Box::leak(Box::new(fnv_sim::RefTable::build(lo, schema)));
    let play = PlayData {
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
    };
    let (script, sim) = production_runtime(play, forms, refs);
    let mut a = App::new();
    // GizmoPlugin supplies CombatPlugin's existing CPU drawing parameter. No RenderPlugin,
    // window, adapter or game installation is requested. Empty real mesh/bind-pose
    // stores supply the enabled gizmo plugin's normal system requirements.
    a.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default(), bevy::gizmos::GizmoPlugin))
        .init_resource::<Assets<Mesh>>()
        .init_resource::<Assets<bevy::mesh::skinning::SkinnedMeshInverseBindposes>>()
        .insert_resource(bevy::time::TimeUpdateStrategy::ManualDuration(std::time::Duration::from_secs_f64(
            1.0 / 60.0,
        )))
        .insert_resource(State::new(PlayState::Playing))
        .insert_resource(script)
        .insert_resource(sim)
        .insert_resource(PlayWorld(play))
        .insert_resource(Player {
            heading: 0.0,
            pitch: 0.0,
            eye_height: 64.0,
            feet: [500.0, 0.0, 0.0],
            body_eye: None,
            body_view: None,
            placed: None,
        })
        .init_resource::<PlayerControl>()
        .init_resource::<MoveIntent>()
        .init_resource::<PhysState>()
        .init_resource::<ActorParts>()
        .init_resource::<nvf_play::mines::Mines>()
        .init_resource::<nvf_play::grass_fire::GrassFires>()
        .init_resource::<nvf_play::thrown::ThrownStats>()
        .init_resource::<Shooters>()
        .init_resource::<AttackTimes>()
        .add_message::<nvf_play::sfx::PlaySound>()
        .add_message::<nvf_play::sfx::ScaledSound>()
        .add_message::<nvf_play::sfx::LoopSound>()
        .add_message::<nvf_play::RefChanged>()
        .add_message::<nvf_play::Travel>()
        .add_message::<nvf_play::world_sounds::ImpactPicked>()
        .add_message::<PlayAttack>()
        .add_plugins(CombatPlugin)
        .add_systems(Update, shoot.before(nvf_play::PlaySet::Activation));
    for (id, y) in [(SHOOTER, 0.0), (TARGET, 80.0)] {
        // A portable drawn actor with no bound clip uses the production fallback Hit
        // timing. Its UseWeapon request, range/facing checks and swing queue are real.
        a.world_mut().spawn((
            ActorRig {
                reference: id.0,
                base: ACTOR.0,
                body: ACTOR.0,
                skeleton: String::new(),
                skel: Arc::new(nvf_assets::actor::PreparedSkeleton::default()),
                nodes: Vec::new(),
                base_scale: 1.0,
            },
            GlobalTransform::from(crate::world::game_to_bevy().mul_transform(Transform::from_xyz(0.0, y, 0.0))),
            InheritedVisibility::VISIBLE,
        ));
    }
    a.update();
    a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            rt.game.world.get(FormId::PLAYER_REF).unwrap().cell = Some(CELL);
            rt.game.player_entered(Some(CELL));
        });
    });
    query(
        &mut a,
        "ref source\nlet source := StringToRef \"00000A45\"\nsource.SetPos Y 0\nsource.AddItem CriticalHitClub 1 1\nref target\nlet target := StringToRef \"00000A46\"\ntarget.SetAV Health 1000",
    );
    a
}
fn hit(value: Option<u16>, critical: bool) -> f64 {
    let mut a = app();
    if let Some(value) = value {
        query(&mut a, &format!("SetWeaponCritDamage {value} CriticalHitClub"));
    }
    query(
        &mut a,
        &format!(
            "ref source\nref target\nref empty\nlet source := StringToRef \"00000A45\"\nlet target := StringToRef \"00000A46\"\nsource.SetAV CriticalChance {}\nsource.UseWeapon CriticalHitClub empty target 1 0 0 0 0",
            if critical { 100 } else { 0 }
        ),
    );
    a.world_mut().resource_scope(|world, mut script: Mut<ScriptState>| {
        let mut sim = world.resource_mut::<SimState>();
        with_world(script.rt.as_mut().unwrap(), &mut sim, |rt| {
            // This is the public AI arrival report, after the source-created use has
            // reached its authored standing place. No swing, strike or hit is injected.
            assert!(rt.game.weapon_use(SHOOTER).is_some());
            rt.game.weapon_use_in_place(SHOOTER);
        });
    });
    let before = health(&mut a);
    assert!((before - 1000.0).abs() < 0.001);
    let mut events = Vec::new();
    a.update();
    events.extend(a.world_mut().resource_mut::<Messages<ActorHit>>().drain());
    {
        let shooters = a.world().resource::<Shooters>();
        assert_eq!(shooters.stats.blows, 1, "real shoot system began the UseWeapon swing");
        assert_eq!(shooters.stats.landed, 0, "the producer must wait for its Hit timing");
        assert_eq!(shooters.swingers[&SHOOTER.0].blows.len(), 1);
    }
    assert!(events.is_empty());
    assert_eq!(health(&mut a), before, "no premature damage before the production swing lands");
    for _ in 0..120 {
        a.update();
        events.extend(a.world_mut().resource_mut::<Messages<ActorHit>>().drain());
        if !events.is_empty() {
            break;
        }
    }
    let after = health(&mut a);
    assert!(after < before, "real producer must reach health damage");
    assert_eq!(events.len(), 1);
    let event = events[0];
    assert_eq!((event.form_id, event.by, event.weapon), (TARGET.0, Some(SHOOTER), Some(CLUB)));
    assert!(!event.died && !event.knocked_out);
    {
        let g = &a.world().resource::<ScriptState>().rt.as_ref().unwrap().game;
        let last = g.extra[&TARGET].last_hit.as_ref().unwrap();
        assert_eq!((last.attacker, last.weapon, last.critical), (Some(SHOOTER), Some(CLUB), critical));
    }
    assert_jip_hit(&mut a, SHOOTER, CLUB, before - after, critical);
    assert_eq!(a.world().resource::<Combat>().forms.weapons[&CLUB].crit_damage, 11);
    assert_eq!(a.world().resource::<Shooters>().stats.landed, 1);
    assert_eq!(a.world().resource::<CombatStats>().actor_strikes, 1);
    before - after
}
#[test]
fn compiled_critical_setter_reaches_real_npc_useweapon_strike_health_and_actor_hit() {
    let baseline = hit(None, true);
    let eleven = hit(Some(11), true);
    let thirty_seven = hit(Some(37), true);
    assert!((baseline - eleven).abs() < 0.001);
    assert!(
        (thirty_seven - eleven - 26.0).abs() < 0.001,
        "actual NPC strike critical damage delta"
    );
    let ordinary_eleven = hit(Some(11), false);
    let ordinary_thirty_seven = hit(Some(37), false);
    assert!((ordinary_eleven - ordinary_thirty_seven).abs() < 0.001);
    assert!((eleven - ordinary_eleven - 11.0).abs() < 0.001);
}
