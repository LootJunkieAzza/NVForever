//! CPU-only control of the real mirror producer and PlayerBody::script_rig.
//! Skeletons, entity graphs and metadata below are authored; no installed assets.
//! Asset loading, GPU drawing and the ScriptState acquisition wrapper are not exercised.
use super::*;
use fnv_game::game::{Game, RunCtx};
use fnv_script::vm::XVal;
use std::sync::Arc;

#[derive(Resource)]
struct TestGame(Game<'static>);
#[derive(Resource, Default)]
struct TestMemory(Memory);

#[allow(clippy::too_many_arguments)]
fn produce(
    mut game: ResMut<TestGame>, rigs: Rigs, visuals: Visuals,
    parents: Query<&ChildOf>, nodes: NodeQuery, commands: Commands,
    mut memory: ResMut<TestMemory>, materials: Materials, shapes: Shapes,
    extra: Extra, dropped: Dropped,
) {
    mirror_game(&mut game.0, rigs, visuals, parents, nodes, commands,
        &mut memory.0, materials, shapes, extra, dropped);
}

fn rig(world: &mut World, root_name: &str, x: f32) -> (Entity, ActorRig) {
    let root = world.spawn((Transform::IDENTITY, Visibility::Hidden)).id();
    let bone = world.spawn((Transform::from_xyz(x, 0.0, 0.0), ChildOf(root))).id();
    let skel = nvf_assets::actor::PreparedSkeleton {
        names: vec![Some(root_name.into()), Some("SharedBone".into())],
        parents: vec![None, Some(0)],
        locals: vec![Transform::IDENTITY, Transform::from_xyz(x, 0.0, 0.0)],
        ..Default::default()
    };
    (root, ActorRig {
        reference: FormId::PLAYER_REF.0, base: FormId::PLAYER_BASE.0,
        body: FormId::PLAYER_BASE.0, skeleton: format!("authored-{root_name}.nif"),
        skel: Arc::new(skel), nodes: vec![root, bone], base_scale: 1.0,
    })
}

fn call(app: &mut App, name: &str, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(&mut app.world_mut().resource_mut::<TestGame>().0,
        &RunCtx::default(), name, Some(FormId::PLAYER_REF), args).unwrap().unwrap()
}

fn scale(app: &mut App, selector: Option<f64>) -> XVal {
    let mut args = vec![XVal::Str("SharedBone".into())];
    if let Some(selector) = selector { args.push(XVal::Num(selector)); }
    call(app, "getnifblockscale", &args)
}

fn node(app: &mut App, selector: f64) -> SceneNode {
    let world = app.world_mut();
    // Use the actual script dispatch for the selector, then read the selected root's
    // published tree through the Rust-side model-specific mirror inspection API.
    let game = &mut world.resource_mut::<TestGame>().0;
    let result = fnv_game::nvse::call(game, &RunCtx::default(), "getnifblockparentnodes",
        Some(FormId::PLAYER_REF), &[XVal::Str("SharedBone".into()), XVal::Num(selector)]).unwrap().unwrap();
    let XVal::Array(array) = result else { panic!("expected parent array") };
    let expected = if selector == 1.0 { "ThirdRoot" } else { "FirstRoot" };
    assert_eq!(game.ext.array(array).unwrap().entries[0].1, XVal::Str(expected.into()));
    game.scene.node_for_model(FormId::PLAYER_REF,
        if selector == 1.0 { ModelView::ThirdPerson } else { ModelView::FirstPerson }, "SharedBone").unwrap()
}

#[test]
fn real_player_body_mirror_dispatch_and_held_edits_keep_rigs_separate() {
    // This existing fixture supplies only authored NIF/KF bytes and CPU asset stores.
    // No render plugin or device is created.
    let (mut app, _authored_files) = crate::outfits::tests::asset_fixture();
    let forms = Box::leak(Box::<fnv_sim::FormStore>::default());
    let refs = Box::leak(Box::<fnv_sim::RefTable>::default());
    let mut game = crate::outfits::tests::game(fnv_sim::World::new(forms, refs, Default::default()));
    fnv_game::nvse::call(&mut game, &RunCtx::default(), "setmodelpath",
        Some(FormId::PLAYER_REF), &[XVal::Str("authored-third.nif".into())]).unwrap().unwrap();
    let third = rig(app.world_mut(), "ThirdRoot", 30.0);
    let first = rig(app.world_mut(), "FirstRoot", 10.0);
    let third_bone = third.1.nodes[1]; let first_bone = first.1.nodes[1];
    let saved_third = third.clone();
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third, Some(first)));
    let mut memory = Memory::default();
    // A populated third-person file cache must not become first-person metadata.
    memory.tables.insert("authored-third.nif".into(), [("SharedBone".into(), [0.25; 9])].into());
    memory.props.insert("authored-third.nif".into(), [("SharedBone".into(), (Some([4845.0, 128.0]), Some([19840.0, 7.0, 255.0])))].into());
    memory.materials.insert((FormId::PLAYER_REF.0, ModelView::FirstPerson, "SharedBone".into()), [9.0; 9]);
    app.insert_resource(TestMemory(memory)).insert_resource(TestGame(game))
        .insert_resource(nvf_play::control::PlayerControl::default())
        .add_systems(Update, produce);
    // The first call only registers the watch, just as an installed script would.
    assert_eq!(scale(&mut app, None), XVal::Num(0.0));
    app.update();
    assert_eq!(scale(&mut app, Some(1.0)), XVal::Num(1.0));
    assert_eq!(scale(&mut app, Some(2.0)), XVal::Num(1.0));
    assert_eq!(node(&mut app, 1.0).translation, [30.0, 0.0, 0.0]);
    let first_node = node(&mut app, 2.0);
    assert_eq!(first_node.translation, [10.0, 0.0, 0.0]);
    assert_eq!(node(&mut app, 1.0).material, Some([0.25; 9]));
    assert_eq!(first_node.material, None);
    let first_info = first_node.info.unwrap();
    assert_eq!((first_info.alpha, first_info.stencil, first_info.emitter), (None, None, None));
    assert_eq!(node(&mut app, 1.0).info.unwrap().alpha, Some([4845.0, 128.0]));
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(2.0), XVal::Num(2.0)]);
    app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().pov = nvf_play::control::Pov::Third;
    app.update();
    assert_eq!(app.world().get::<Transform>(first_bone).unwrap().scale, Vec3::splat(2.0));
    assert_eq!(app.world().get::<Transform>(third_bone).unwrap().scale, Vec3::ONE);
    assert_eq!(scale(&mut app, None), XVal::Num(1.0), "old omitted default remains third person");
    app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().pov = nvf_play::control::Pov::First;
    app.update();
    assert_eq!(scale(&mut app, None), XVal::Num(1.0));
    assert_eq!(scale(&mut app, Some(2.0)), XVal::Num(2.0));
    app.world_mut().resource_mut::<TestGame>().0.scene.push_edit(SceneEdit::ForModel {
        view: ModelView::FirstPerson, edit: Box::new(SceneEdit::Material {
            r: FormId::PLAYER_REF, node: "SharedBone".into(), id: 0, v: 13.0,
        }),
    });
    app.update();
    assert_eq!(app.world().resource::<TestMemory>().0.materials[&(FormId::PLAYER_REF.0, ModelView::FirstPerson, "SharedBone".into())], [9.0; 9], "unsupported first material edit is refused");
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(saved_third, None));
    app.update();
    assert_eq!(scale(&mut app, Some(2.0)), XVal::Num(0.0), "removed first rig never falls back");
    assert_eq!(scale(&mut app, Some(1.0)), XVal::Num(1.0));
}

type NamedRig = (Entity, ActorRig);

fn naming_fixture() -> (App, crate::outfits::tests::TestFiles, NamedRig, NamedRig) {
    let (mut app, files) = crate::outfits::tests::asset_fixture();
    let forms = Box::leak(Box::<fnv_sim::FormStore>::default());
    let refs = Box::leak(Box::<fnv_sim::RefTable>::default());
    let game = crate::outfits::tests::game(fnv_sim::World::new(forms, refs, Default::default()));
    let third = rig(app.world_mut(), "ThirdRoot", 30.0);
    let first = rig(app.world_mut(), "FirstRoot", 10.0);
    app.world_mut().entity_mut(third.0).insert(Name::new("Factory Third Label"));
    app.world_mut().entity_mut(first.0).insert(Name::new("Factory First Label"));
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third.clone(), Some(first.clone())))
        .insert_resource(TestGame(game)).init_resource::<TestMemory>()
        .add_systems(Update, produce);
    assert_eq!(scale(&mut app, None), XVal::Num(0.0));
    app.update();
    (app, files, third, first)
}

fn published(app: &mut App, r: FormId, view: ModelView, name: &str) -> Option<SceneNode> {
    app.world_mut().resource_mut::<TestGame>().0.scene.node_for_model(r, view, name)
}

#[test]
fn explicitly_renamed_player_roots_override_factory_labels_and_keep_per_rig_identity() {
    let (mut app, _files, third, first) = naming_fixture();
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::ThirdPerson, "ThirdRoot").is_some());
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "FirstRoot").is_some());
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "Factory First Label").is_none(), "factory labels are not NIF names");
    for (selector, view, root, old, new) in [
        (2, ModelView::FirstPerson, first.0, "FirstRoot", "OwnedFirstRoot"),
        (1, ModelView::ThirdPerson, third.0, "ThirdRoot", "OwnedThirdRoot"),
    ] {
        // Dynamic source argument: get the real published root name rather than
        // baking a game asset's root name into a script or a smoke scenario.
        let script = format!("array_var roots\nstring_var rootName\nfloat success\nBegin Function {{ }}\nlet roots := PlayerREF.GetChildBlocks \"\" {selector}\nlet rootName := roots[0]\nlet success := PlayerREF.RenameNifBlock rootName \"{new}\" {selector}\nSetFunctionValue success\nEnd\n");
        assert_eq!(app.world_mut().resource_mut::<TestGame>().0.run_query(&script, RunCtx::default()).unwrap(), XVal::Num(1.0));
        assert!(published(&mut app, FormId::PLAYER_REF, view, new).is_some(), "script mirror changes at once");
        app.update(); app.update(); // Deferred Name edit, then a fresh publication.
        assert_eq!(app.world().get::<Name>(root).unwrap().as_str(), new);
        assert!(published(&mut app, FormId::PLAYER_REF, view, new).is_some(), "explicit root rename survives the real producer");
        assert!(published(&mut app, FormId::PLAYER_REF, view, old).is_none());
    }
    call(&mut app, "setnifblockscale", &[XVal::Str("OwnedFirstRoot".into()), XVal::Num(1.25), XVal::Num(2.0)]);
    app.update();
    assert_eq!(app.world().get::<Transform>(first.0).unwrap().scale, Vec3::splat(1.25), "new root name remains addressable by later edits");
    assert_eq!(app.world().get::<Transform>(third.0).unwrap().scale, Vec3::ONE);
    // A fresh rig of the same reference keeps its own skeleton name; the explicit
    // marker belongs to the old entity, not to that reference or bone-name string.
    let replacement = rig(app.world_mut(), "FirstRoot", 12.0);
    app.world_mut().entity_mut(replacement.0).insert(Name::new("Replacement Factory Label"));
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third, Some(replacement)));
    app.world_mut().entity_mut(first.0).despawn();
    app.update();
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "FirstRoot").is_some());
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "OwnedFirstRoot").is_none());
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "Replacement Factory Label").is_none());
}

#[test]
fn ordinary_actor_root_and_nonroot_renames_keep_existing_name_rules() {
    let (mut app, _files, _third, _first) = naming_fixture();
    call(&mut app, "renamenifblock", &[XVal::Str("SharedBone".into()), XVal::Str("OwnedFirstBone".into()), XVal::Num(2.0)]);
    app.update(); app.update();
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "OwnedFirstBone").is_some());
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::ThirdPerson, "SharedBone").is_some());
    let receiver = FormId(0x505);
    let (root, mut actor) = rig(app.world_mut(), "ActorRoot", 50.0);
    actor.reference = receiver.0;
    app.world_mut().entity_mut(root).insert((actor, Name::new("Factory Actor Label")));
    let dispatch = |app: &mut App, name, args: &[XVal]| fnv_game::nvse::call(
        &mut app.world_mut().resource_mut::<TestGame>().0, &RunCtx::default(), name, Some(receiver), args).unwrap().unwrap();
    dispatch(&mut app, "getchildblocks", &[XVal::Str(String::new())]);
    app.update();
    assert!(published(&mut app, receiver, ModelView::Reference, "ActorRoot").is_some());
    assert!(published(&mut app, receiver, ModelView::Reference, "Factory Actor Label").is_none());
    assert_eq!(dispatch(&mut app, "renamenifblock", &[XVal::Str("ActorRoot".into()), XVal::Str("OwnedActorRoot".into())]), XVal::Num(1.0));
    app.update(); app.update();
    assert!(published(&mut app, receiver, ModelView::Reference, "OwnedActorRoot").is_some());
    assert!(published(&mut app, receiver, ModelView::Reference, "ActorRoot").is_none());
    assert_eq!(app.world().get::<Name>(root).unwrap().as_str(), "OwnedActorRoot");
}

#[test]
fn owned_smoke_query_strings_keep_both_roots_through_the_real_cpu_producer() {
    let job: serde_json::Value = serde_json::from_str(include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"), "/../../tools/smoke/fixtures/nif-root-renames.steps.json"
    ))).unwrap();
    let (set, steps) = fnv_game::ext_probes::query_steps(&job).unwrap();
    assert_eq!(set, "nif-script-root-renames", "the runtime consumer normalizes the raw job name before logging it");
    assert_eq!(steps.len(), 6);
    let (mut app, _files, _third, _first) = naming_fixture();
    let mut values = Vec::new();
    for step in steps {
        let value = {
            let mut state = app.world_mut().resource_mut::<TestGame>();
            let before = state.0.events.len();
            let observation = fnv_game::ext_probes::run_step(&mut state.0, &step);
            assert!(!observation.contains_key("error"), "{}: {observation:?}", step.label);
            assert!(!state.0.events[before..].iter().any(|event|
                matches!(&event.kind, fnv_game::trace::EventKind::ScriptError { .. })),
                "{} emitted a ScriptError", step.label);
            observation.get("value").and_then(serde_json::Value::as_f64)
                .unwrap_or_else(|| panic!("{}: numeric query result expected, got {observation:?}", step.label))
        };
        values.push(value);
        // One real CPU mirror publication between each actual query string. The
        // owned dummy steps provide the deferred-command flush before each oracle.
        app.update();
    }
    assert_eq!(values, [1.0, 0.0, 1.0, 1.0, 0.0, 1.0]);
    assert!(!app.world().resource::<TestGame>().0.events.iter().any(|event|
        matches!(&event.kind, fnv_game::trace::EventKind::ScriptError { .. })));
}
