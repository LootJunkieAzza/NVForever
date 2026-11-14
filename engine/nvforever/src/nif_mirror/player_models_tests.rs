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
    let (mut app, files, third, first) = unpublished_fixture();
    assert_eq!(scale(&mut app, None), XVal::Num(0.0));
    app.update();
    (app, files, third, first)
}

fn unpublished_fixture() -> (App, crate::outfits::tests::TestFiles, NamedRig, NamedRig) {
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


fn call_ref(app: &mut App, reference: FormId, name: &str, args: &[XVal]) -> XVal {
    fnv_game::nvse::call(&mut app.world_mut().resource_mut::<TestGame>().0,
        &RunCtx::default(), name, Some(reference), args).unwrap().unwrap()
}

fn replace_first(app: &mut App, third: NamedRig, old_first: NamedRig, x: f32) -> NamedRig {
    app.world_mut().entity_mut(old_first.0).despawn();
    let first = rig(app.world_mut(), "FirstRoot", x);
    app.world_mut().entity_mut(first.0).insert(Name::new("Replacement Factory First"));
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third, Some(first.clone())));
    first
}

#[test]
fn held_and_queued_transforms_expire_on_direct_player_model_replacement() {
    let (mut app, _files, third, first) = naming_fixture();
    call(&mut app, "setnifblocktranslation", &[XVal::Str("SharedBone".into()), XVal::Num(7.0), XVal::Num(8.0), XVal::Num(9.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblockrotation", &[XVal::Str("SharedBone".into()), XVal::Num(10.0), XVal::Num(20.0), XVal::Num(30.0), XVal::Num(0.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(2.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(3.0), XVal::Num(1.0)]);
    app.update();
    assert_eq!(app.world().get::<Transform>(first.1.nodes[1]).unwrap().scale, Vec3::splat(2.0));
    // Queue against the old published model, then replace it before consumption.
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(11.0), XVal::Num(2.0)]);
    let new_first = replace_first(&mut app, third.clone(), first, 90.0);
    app.update();
    let transform = app.world().get::<Transform>(new_first.1.nodes[1]).unwrap();
    assert_eq!(transform.translation, Vec3::new(90.0, 0.0, 0.0));
    assert_eq!(transform.rotation, Quat::IDENTITY);
    assert_eq!(transform.scale, Vec3::ONE);
    assert_eq!(app.world().get::<Transform>(third.1.nodes[1]).unwrap().scale, Vec3::splat(3.0));
    assert_eq!(scale(&mut app, Some(2.0)), XVal::Num(1.0));
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "FirstRoot").is_some());
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "Replacement Factory First").is_none());
}

#[test]
fn disappearance_and_reappearance_never_transfer_first_rig_holds_or_fall_back() {
    let (mut app, _files, third, first) = naming_fixture();
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(2.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(3.0), XVal::Num(1.0)]);
    app.update();
    app.world_mut().entity_mut(first.0).despawn();
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third.clone(), None));
    app.update();
    assert_eq!(scale(&mut app, Some(2.0)), XVal::Num(0.0));
    // Even a setter while absent cannot bind the next matching first-person name.
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(19.0), XVal::Num(2.0)]);
    let first = rig(app.world_mut(), "FirstRoot", 80.0);
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third.clone(), Some(first.clone())));
    app.update();
    assert_eq!(app.world().get::<Transform>(first.1.nodes[1]).unwrap().scale, Vec3::ONE);
    assert_eq!(app.world().get::<Transform>(third.1.nodes[1]).unwrap().scale, Vec3::splat(3.0));
}

#[test]
fn child_replacement_rejects_stale_transform_rename_remove_and_child_flag_edits() {
    for operation in ["scale", "rename", "remove", "childflags"] {
        let (mut app, _files, third, mut first) = naming_fixture();
        call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(2.0), XVal::Num(2.0)]);
        app.update();
        match operation {
            "scale" => { call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(12.0), XVal::Num(2.0)]); }
            "rename" => { call(&mut app, "renamenifblock", &[XVal::Str("SharedBone".into()), XVal::Str("RenamedOldChild".into()), XVal::Num(2.0)]); }
            "remove" => { call(&mut app, "removenifblock", &[XVal::Str("SharedBone".into()), XVal::Num(2.0)]); }
            _ => app.world_mut().resource_mut::<TestGame>().0.scene.push_edit(SceneEdit::ForModel {
                view: ModelView::FirstPerson, edit: Box::new(SceneEdit::ChildFlags {
                    r: FormId::PLAYER_REF, node: "FirstRoot".into(), child: 0, flags: 1,
                }),
            }),
        }
        let old_child = first.1.nodes[1];
        let child = app.world_mut().despawn_no_free(old_child).unwrap();
        assert_eq!(old_child.index(), child.index(), "control must reuse the actual entity slot");
        assert_ne!(old_child.generation(), child.generation(), "control must advance its generation");
        app.world_mut().spawn_at(child, (Transform::from_xyz(77.0, 0.0, 0.0), ChildOf(first.0))).unwrap();
        first.1.nodes[1] = child;
        app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third, Some(first)));
        app.update();
        assert_eq!(app.world().get::<Transform>(child).unwrap().scale, Vec3::ONE, "{operation}: stale hold/edit reached new child");
        assert_eq!(app.world().get::<Transform>(child).unwrap().translation.x, 77.0);
        assert!(app.world().get::<NifHidden>(child).is_none(), "{operation}");
        assert!(app.world().get::<Name>(child).is_none(), "{operation}: old rename reached namesake");
        assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "SharedBone").is_some(), "{operation}");
    }
}

#[test]
fn ordinary_actor_cell_lifetime_and_unpublished_target_do_not_transfer_holds() {
    let (mut app, _files, _third, _first) = naming_fixture();
    let reference = FormId(0x911);
    // A one-shot cold write retains main's startup contract without a prior read.
    // After its first binding, a later same-name actor must not inherit the hold.
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(25.0)]);
    let (root, mut actor) = rig(app.world_mut(), "OwnedActorRoot", 50.0);
    actor.reference = reference.0;
    app.world_mut().entity_mut(root).insert((actor.clone(), Name::new("Factory Actor Label")));
    app.update();
    assert_eq!(app.world().get::<Transform>(actor.nodes[1]).unwrap().scale, Vec3::splat(25.0));
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(4.0)]);
    app.update();
    assert_eq!(app.world().get::<Transform>(actor.nodes[1]).unwrap().scale, Vec3::splat(4.0));
    app.world_mut().entity_mut(root).despawn();
    app.update();
    assert!(published(&mut app, reference, ModelView::Reference, "SharedBone").is_none());
    let (root, mut replacement) = rig(app.world_mut(), "OwnedActorRoot", 60.0);
    replacement.reference = reference.0;
    app.world_mut().entity_mut(root).insert((replacement.clone(), Name::new("Replacement Actor Label")));
    app.update();
    assert_eq!(app.world().get::<Transform>(replacement.nodes[1]).unwrap().scale, Vec3::ONE);
    assert!(published(&mut app, reference, ModelView::Reference, "OwnedActorRoot").is_some());
    assert!(published(&mut app, reference, ModelView::Reference, "Replacement Actor Label").is_none());
}

#[test]
fn cold_player_write_keeps_selected_view_without_a_preliminary_read() {
    let (mut app, _files, third, first) = unpublished_fixture();
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(6.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblocktranslation", &[XVal::Str("SharedBone".into()), XVal::Num(7.0), XVal::Num(8.0), XVal::Num(9.0)]);
    app.insert_resource(nvf_play::control::PlayerControl::default());
    app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().pov = nvf_play::control::Pov::Third;
    app.update();
    assert_eq!(app.world().get::<Transform>(first.1.nodes[1]).unwrap().scale, Vec3::splat(6.0));
    assert_eq!(app.world().get::<Transform>(third.1.nodes[1]).unwrap().scale, Vec3::ONE);
    assert_eq!(app.world().get::<Transform>(third.1.nodes[1]).unwrap().translation, Vec3::new(7.0, 8.0, 9.0));
    assert_eq!(app.world().get::<Transform>(first.1.nodes[1]).unwrap().translation.x, 10.0);
}

#[test]
fn published_missing_name_binds_late_node_once_in_the_same_model() {
    let (mut app, _files, _third, _first) = naming_fixture();
    let reference = FormId(0x933);
    let root = app.world_mut().spawn((Name::new("OwnedRoot"), Transform::IDENTITY, RefVisual(reference.0))).id();
    // An ordinary RefVisual root is a wrapper, not a script-visible NIF node:
    // this first publication has an actual model identity but an empty node tree.
    assert_eq!(call_ref(&mut app, reference, "getnifblockscale", &[XVal::Str("Late".into())]), XVal::Num(0.0));
    app.update();
    assert_eq!(call_ref(&mut app, reference, "getnifblockscale", &[XVal::Str("Late".into())]), XVal::Num(0.0));
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("Late".into()), XVal::Num(7.0)]);
    // The queued missing name acquires the real node created after publication.
    let child = app.world_mut().spawn((Name::new("Late"), Transform::IDENTITY, ChildOf(root))).id();
    app.update();
    assert_eq!(app.world().get::<Transform>(child).unwrap().scale, Vec3::splat(7.0));
    app.world_mut().entity_mut(child).despawn();
    let replacement = app.world_mut().spawn((Name::new("Late"), Transform::IDENTITY, ChildOf(root))).id();
    app.update();
    assert_eq!(app.world().get::<Transform>(replacement).unwrap().scale, Vec3::ONE);
}

#[test]
fn cold_pending_hold_pins_first_model_even_before_the_named_child_exists() {
    let (mut app, _files, _third, _first) = naming_fixture();
    let reference = FormId(0x934);
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("Late".into()), XVal::Num(8.0)]);
    app.update(); // No model yet: retain main's pre-draw hold.
    let root = app.world_mut().spawn((Name::new("OwnedRoot"), Transform::IDENTITY, RefVisual(reference.0))).id();
    app.update(); // Pin this model despite the missing child.
    app.world_mut().entity_mut(root).despawn();
    let root = app.world_mut().spawn((Name::new("OwnedRoot"), Transform::IDENTITY, RefVisual(reference.0))).id();
    let replacement = app.world_mut().spawn((Name::new("Late"), Transform::IDENTITY, ChildOf(root))).id();
    app.update();
    assert_eq!(app.world().get::<Transform>(replacement).unwrap().scale, Vec3::ONE, "pending hold must not transfer to a replacement model");
}

#[test]
fn cold_transform_writes_survive_frames_before_the_first_actor_is_drawn() {
    let (mut app, _files, _third, _first) = naming_fixture();
    let reference = FormId(0x937);
    call_ref(&mut app, reference, "setnifblocktranslation", &[XVal::Str("SharedBone".into()), XVal::Num(1.0), XVal::Num(2.0), XVal::Num(3.0)]);
    call_ref(&mut app, reference, "setnifblockrotation", &[XVal::Str("SharedBone".into()), XVal::Num(0.0), XVal::Num(0.0), XVal::Num(90.0)]);
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(5.0)]);
    app.update();
    assert_eq!(call_ref(&mut app, reference, "getnifblockscale", &[XVal::Str("SharedBone".into())]), XVal::Num(0.0), "pending writes do not invent a readable tree");
    let (root, mut actor) = rig(app.world_mut(), "OwnedActorRoot", 50.0);
    actor.reference = reference.0;
    app.world_mut().entity_mut(root).insert(actor.clone());
    // A later component write when the first model becomes available must merge,
    // not discard the pending cold scale and rotation.
    call_ref(&mut app, reference, "setnifblocktranslation", &[XVal::Str("SharedBone".into()), XVal::Num(4.0), XVal::Num(5.0), XVal::Num(6.0)]);
    app.update();
    let transform = app.world().get::<Transform>(actor.nodes[1]).unwrap();
    assert_eq!(transform.translation, Vec3::new(4.0, 5.0, 6.0));
    assert_eq!(transform.scale, Vec3::splat(5.0));
    assert!(transform.rotation.angle_between(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)).abs() < 1e-5);
}

#[test]
fn observed_missing_root_transform_cannot_grant_a_later_pending_write() {
    let (mut app, _files, _third, first) = naming_fixture();
    app.world_mut().entity_mut(first.0).remove::<Transform>();
    app.update(); // A publication actually observes the unavailable root query.
    assert_eq!(scale(&mut app, Some(2.0)), XVal::Num(0.0));
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(15.0), XVal::Num(2.0)]);
    app.world_mut().entity_mut(first.0).insert(Transform::IDENTITY);
    app.update(); // Transform returns before that queued request is consumed.
    assert_eq!(app.world().get::<Transform>(first.1.nodes[1]).unwrap().scale, Vec3::ONE,
        "an observed invalid root must not publish a Model token for future binding");
    // A new request after a valid republication still addresses the actual node.
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(4.0), XVal::Num(2.0)]);
    app.update();
    assert_eq!(app.world().get::<Transform>(first.1.nodes[1]).unwrap().scale, Vec3::splat(4.0));
}

#[test]
fn published_empty_model_request_cannot_bind_a_replacement_root() {
    let (mut app, _files, _third, _first) = naming_fixture();
    let reference = FormId(0x938);
    let root = app.world_mut().spawn((Transform::IDENTITY, RefVisual(reference.0))).id();
    assert_eq!(call_ref(&mut app, reference, "getnifblockscale", &[XVal::Str("Late".into())]), XVal::Num(0.0));
    app.update();
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("Late".into()), XVal::Num(8.0)]);
    // Replace between queue capture and consumption, without an absent frame.
    app.world_mut().entity_mut(root).despawn();
    let root = app.world_mut().spawn((Transform::IDENTITY, RefVisual(reference.0))).id();
    let child = app.world_mut().spawn((Name::new("Late"), Transform::IDENTITY, ChildOf(root))).id();
    app.update();
    assert_eq!(app.world().get::<Transform>(child).unwrap().scale, Vec3::ONE);
}

#[test]
fn same_frame_public_insert_then_transform_waits_for_actual_deferred_node_creation() {
    let (mut app, _files, _third, _first) = naming_fixture();
    let reference = FormId(0x935);
    let root = app.world_mut().spawn((Name::new("OwnedRoot"), Transform::IDENTITY, RefVisual(reference.0))).id();
    let anchor = app.world_mut().spawn((Name::new("Anchor"), Transform::IDENTITY, ChildOf(root))).id();
    assert_eq!(call_ref(&mut app, reference, "getnifblockscale", &[XVal::Str("Anchor".into())]), XVal::Num(0.0));
    app.update();
    assert_eq!(call_ref(&mut app, reference, "insertnode", &[XVal::Ref(reference.0), XVal::Num(1.0), XVal::Str("Anchor|Inserted".into())]), XVal::Num(1.0));
    // Public InsertNode has created only a core placeholder, with no entity identity.
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("Inserted".into()), XVal::Num(9.0)]);
    app.update(); // Commands create the child after this producer invocation.
    let child = app.world().get::<Children>(anchor).unwrap().iter().find(|child| app.world().get::<Name>(*child).is_some_and(|name| name.as_str() == "Inserted")).unwrap();
    app.update();
    assert_eq!(app.world().get::<Transform>(child).unwrap().scale, Vec3::splat(9.0));
}

#[test]
fn pending_aliases_merge_per_property_without_overwriting_newer_bound_edits() {
    for newer_bound_scale in [false, true] {
    let (mut app, _files, _third, first) = naming_fixture();
    call(&mut app, "setnifblockscale", &[XVal::Str("FutureAlias".into()), XVal::Num(7.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblockscale", &[XVal::Str("futurealias".into()), XVal::Num(9.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblocktranslation", &[XVal::Str("FutureAlias".into()), XVal::Num(1.0), XVal::Num(2.0), XVal::Num(3.0), XVal::Num(2.0)]);
    app.update(); // Both names wait in the same model, neither names a node yet.
    if newer_bound_scale {
        call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(11.0), XVal::Num(2.0)]);
    }
    call(&mut app, "renamenifblock", &[XVal::Str("SharedBone".into()), XVal::Str("FutureAlias".into()), XVal::Num(2.0)]);
    app.update(); // New bound scale applies; rename is a deferred command.
    app.update(); // Both older pending aliases now resolve to that same identity.
    let transform = app.world().get::<Transform>(first.1.nodes[1]).unwrap();
    assert_eq!(transform.scale, Vec3::splat(if newer_bound_scale { 11.0 } else { 9.0 }), "last writer must win independently of alias promotion order");
    assert_eq!(transform.translation, Vec3::new(1.0, 2.0, 3.0), "independent earlier properties must still merge");
    assert!(app.world().resource::<TestMemory>().0.pending_held.is_empty());
    }
}

#[test]
fn cold_structural_writes_are_consumed_once_on_already_drawn_nodes() {
    // Producer-side SceneMirror API contract. Public structural script dispatch
    // still requires a mirrored target, just as on main; no new admission here.
    for operation in ["flags", "rename", "remove", "insert"] {
        let (mut app, _files, _third, _first) = naming_fixture();
        let reference = FormId(0x936);
        let root = app.world_mut().spawn((Name::new("OwnedRoot"), Transform::IDENTITY, RefVisual(reference.0))).id();
        let child = app.world_mut().spawn((Name::new("OwnedChild"), Transform::IDENTITY, ChildOf(root))).id();
        let edit = match operation {
            "flags" => SceneEdit::Flags { r: reference, node: "OwnedChild".into(), flags: 1 },
            "rename" => SceneEdit::Rename { r: reference, node: "OwnedChild".into(), to: "Renamed".into() },
            "remove" => SceneEdit::Remove { r: reference, node: "OwnedChild".into() },
            _ => SceneEdit::Insert { r: reference, target: "OwnedChild".into(), name: "Inserted".into(), as_parent: false },
        };
        app.world_mut().resource_mut::<TestGame>().0.scene.push_edit(edit);
        app.update();
        match operation {
            "flags" => assert!(app.world().get::<NifHidden>(child).is_some()),
            "rename" => assert_eq!(app.world().get::<Name>(child).unwrap().as_str(), "Renamed"),
            "remove" => assert!(app.world().get_entity(child).is_err()),
            _ => assert!(app.world().get::<Children>(child).unwrap().iter().any(|entity| app.world().get::<Name>(entity).is_some_and(|name| name.as_str() == "Inserted"))),
        }
        let later = app.world_mut().spawn((Name::new("OwnedChild"), Transform::IDENTITY, ChildOf(root))).id();
        app.update();
        assert!(app.world().get::<NifHidden>(later).is_none());
        assert_eq!(app.world().get::<Name>(later).unwrap().as_str(), "OwnedChild");
        assert!(app.world().get::<Children>(later).is_none(), "{operation} must not replay on a later namesake");
    }
}

#[test]
fn live_node_holds_survive_pose_and_pov_changes_but_not_subtree_departure() {
    let (mut app, _files, third, mut first) = naming_fixture();
    app.insert_resource(nvf_play::control::PlayerControl::default());
    call(&mut app, "setnifblocktranslation", &[XVal::Str("SharedBone".into()), XVal::Num(7.0), XVal::Num(8.0), XVal::Num(9.0), XVal::Num(2.0)]);
    call(&mut app, "setnifblockscale", &[XVal::Str("SharedBone".into()), XVal::Num(2.0), XVal::Num(2.0)]);
    app.update();
    for pov in [nvf_play::control::Pov::Third, nvf_play::control::Pov::First] {
        app.world_mut().resource_mut::<nvf_play::control::PlayerControl>().pov = pov;
        *app.world_mut().get_mut::<Transform>(first.1.nodes[1]).unwrap() = Transform::from_xyz(88.0, 0.0, 0.0);
        app.update();
        let transform = app.world().get::<Transform>(first.1.nodes[1]).unwrap();
        assert_eq!(transform.translation, Vec3::new(7.0, 8.0, 9.0));
        assert_eq!(transform.scale, Vec3::splat(2.0));
        assert_eq!(app.world().get::<Transform>(third.1.nodes[1]).unwrap().scale, Vec3::ONE);
    }
    let old_child = first.1.nodes[1];
    let outside = app.world_mut().spawn(Transform::IDENTITY).id();
    app.world_mut().entity_mut(old_child).insert(ChildOf(outside));
    *app.world_mut().get_mut::<Transform>(old_child).unwrap() = Transform::from_xyz(44.0, 0.0, 0.0);
    let new_child = app.world_mut().spawn((Transform::from_xyz(66.0, 0.0, 0.0), ChildOf(first.0))).id();
    first.1.nodes[1] = new_child;
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third, Some(first)));
    app.update();
    assert_eq!(app.world().get::<Transform>(old_child).unwrap().translation.x, 44.0, "former child is outside current ancestry");
    assert_eq!(app.world().get::<Transform>(new_child).unwrap().scale, Vec3::ONE);
    assert_eq!(app.world().get::<Transform>(new_child).unwrap().translation.x, 66.0);
}


fn legacy_flat_fixture(visual: bool, virtual_name: &str) -> (App, crate::outfits::tests::TestFiles, FormId, Entity) {
    let (mut app, files, _third, _first) = naming_fixture();
    let reference = FormId(0x922);
    let root = app.world_mut().spawn((Transform::IDENTITY,
        nvf_play::dropped::DroppedRoot(reference.0),
        nvf_play::dropped::DroppedModel("authored-flat.nif".into()))).id();
    let shape = app.world_mut().spawn((Transform::IDENTITY, Name::new("ConcreteFlatShape"), ChildOf(root))).id();
    if visual {
        // Authored handler topology: root_of resolves the parent, and unchanged
        // non-rig walk visits its child shape. Normal dropped-flat spawning marks
        // mesh entities instead; this control does not qualify that spawn topology.
        app.world_mut().entity_mut(root).insert(RefVisual(reference.0));
        app.world_mut().entity_mut(shape).insert(RefVisual(reference.0));
    }
    {
        // Seed authored file metadata to exercise the real legacy producer branch;
        // this control does not qualify NIF parsing or asset/GPU material drawing.
        let mut memory = app.world_mut().resource_mut::<TestMemory>();
        memory.0.files.insert("authored-flat.nif".into(), vec![FileNode {
            name: virtual_name.into(), parent: None, translation: Vec3::ZERO,
            rotation: Quat::IDENTITY, scale: 1.0,
        }]);
        memory.0.tables.insert("authored-flat.nif".into(), [(virtual_name.into(), [0.25; 9])].into());
        // An unrecorded synthetic reference has no MODL; the material handler's
        // original fallback table is therefore keyed by the empty model path.
        memory.0.tables.insert(String::new(), [(virtual_name.into(), [0.25; 9])].into());
    }
    published(&mut app, reference, ModelView::Reference, virtual_name);
    app.update();
    (app, files, reference, shape)
}

#[test]
fn legacy_flat_transforms_and_nontransform_handlers_keep_their_original_routes() {
    let (mut app, _files, reference, _) = legacy_flat_fixture(false, "FileOnlyShape");
    call_ref(&mut app, reference, "setnifblockscale", &[XVal::Str("FileOnlyShape".into()), XVal::Num(2.0)]);
    app.update();
    assert_eq!(published(&mut app, reference, ModelView::Reference, "FileOnlyShape").unwrap().scale, 2.0,
        "flat transform remembering precedes root_of, even without any RefVisual root");

    for operation in ["flags", "rename", "remove", "insert"] {
        let (mut app, _files, reference, shape) = legacy_flat_fixture(true, "ConcreteFlatShape");
        match operation {
            "flags" => { call_ref(&mut app, reference, "setnifblockflag", &[XVal::Str("ConcreteFlatShape".into()), XVal::Num(0.0), XVal::Num(1.0)]); }
            "rename" => { call_ref(&mut app, reference, "renamenifblock", &[XVal::Str("ConcreteFlatShape".into()), XVal::Str("LegacyRenamed".into())]); }
            "remove" => { call_ref(&mut app, reference, "removenifblock", &[XVal::Str("ConcreteFlatShape".into())]); }
            _ => { call_ref(&mut app, reference, "insertnode", &[XVal::Ref(reference.0), XVal::Num(3.0), XVal::Str("ConcreteFlatShape|LegacyInserted".into())]); }
        }
        app.update();
        match operation {
            "flags" => assert!(app.world().get::<NifHidden>(shape).is_some()),
            "rename" => assert_eq!(app.world().get::<Name>(shape).unwrap().as_str(), "LegacyRenamed"),
            "remove" => assert!(app.world().get_entity(shape).is_err()),
            _ => assert!(app.world().get::<Children>(shape).unwrap().iter().any(|e|
                app.world().get::<Name>(e).is_some_and(|name| name.as_str() == "LegacyInserted"))),
        }
    }

    let (mut app, _files, reference, _) = legacy_flat_fixture(true, "FileOnlyShape");
    call_ref(&mut app, reference, "setmaterialpropertyvalue", &[XVal::Str("FileOnlyShape".into()), XVal::Num(6.0), XVal::Num(23.0)]);
    app.update();
    assert_eq!(published(&mut app, reference, ModelView::Reference, "FileOnlyShape").unwrap().material.unwrap()[6], 23.0,
        "legacy material metadata updates precede a failed concrete shape lookup");
}

#[test]
fn selected_root_and_nodes_require_live_transform_queries_before_holds_can_survive() {
    for target_root in [false, true] {
        let (mut app, _files, _third, first) = naming_fixture();
        let name = if target_root { "FirstRoot" } else { "SharedBone" };
        let entity = if target_root { first.0 } else { first.1.nodes[1] };
        call(&mut app, "setnifblockscale", &[XVal::Str(name.into()), XVal::Num(2.0), XVal::Num(2.0)]);
        app.update();
        call(&mut app, "setnifblockscale", &[XVal::Str(name.into()), XVal::Num(9.0), XVal::Num(2.0)]);
        app.world_mut().entity_mut(entity).remove::<Transform>();
        app.update();
        assert!(app.world().resource::<TestMemory>().0.bound_held.keys().all(|(_, view, _)| *view != ModelView::FirstPerson),
            "a node or selected root outside NodeQuery expires holds immediately");
        app.world_mut().entity_mut(entity).insert(Transform::IDENTITY);
        app.update();
        assert_eq!(app.world().get::<Transform>(entity).unwrap().scale, Vec3::ONE,
            "an observed missing Transform must not revive an old or queued hold");
    }
    let (mut app, _files, _third, first) = naming_fixture();
    call(&mut app, "setnifblockscale", &[XVal::Str("FirstRoot".into()), XVal::Num(2.0), XVal::Num(2.0)]);
    app.update();
    call(&mut app, "setnifblockscale", &[XVal::Str("FirstRoot".into()), XVal::Num(9.0), XVal::Num(2.0)]);
    app.world_mut().entity_mut(first.0).despawn();
    // Keep PlayerBody's cached selection deliberately stale for this producer frame.
    app.update();
    assert!(app.world().resource::<TestMemory>().0.bound_held.keys().all(|(_, view, _)| *view != ModelView::FirstPerson),
        "walk's cached root enumeration alone is not proof of a live selected root");
}

#[test]
fn indexed_child_queue_survives_sibling_reorder_without_redirecting_to_new_index() {
    let (mut app, _files, third, mut first) = naming_fixture();
    let original = first.1.nodes[1];
    let sibling = app.world_mut().spawn((Transform::from_xyz(55.0, 0.0, 0.0), ChildOf(first.0))).id();
    let mut skel = (*first.1.skel).clone();
    skel.names.push(Some("SiblingBone".into()));
    skel.parents.push(Some(0));
    skel.locals.push(Transform::from_xyz(55.0, 0.0, 0.0));
    first.1.skel = Arc::new(skel);
    first.1.nodes.push(sibling);
    app.insert_resource(crate::player_body::PlayerBody::synthetic_script_models(third, Some(first.clone())));
    app.update();
    app.world_mut().resource_mut::<TestGame>().0.scene.push_edit(SceneEdit::ForModel {
        view: ModelView::FirstPerson, edit: Box::new(SceneEdit::ChildFlags {
            r: FormId::PLAYER_REF, node: "FirstRoot".into(), child: 0, flags: 1,
        }),
    });
    app.world_mut().entity_mut(first.0).replace_children(&[sibling, original]);
    app.update();
    assert!(app.world().get::<NifHidden>(original).is_some(), "published child zero remains the addressed entity");
    assert!(app.world().get::<NifHidden>(sibling).is_none(), "new child zero is untouched");
    assert_eq!(app.world().get::<Transform>(sibling).unwrap().translation.x, 55.0);
}

#[test]
fn rename_then_transform_preserves_concrete_target_and_same_model_reparenting() {
    let (mut app, _files, _third, first) = naming_fixture();
    let child = first.1.nodes[1];
    assert_eq!(call(&mut app, "renamenifblock", &[XVal::Str("SharedBone".into()), XVal::Str("OwnedRenamedBone".into()), XVal::Num(2.0)]), XVal::Num(1.0));
    call(&mut app, "setnifblockscale", &[XVal::Str("OwnedRenamedBone".into()), XVal::Num(4.0), XVal::Num(2.0)]);
    app.update(); app.update();
    assert_eq!(app.world().get::<Name>(child).unwrap().as_str(), "OwnedRenamedBone");
    assert_eq!(app.world().get::<Transform>(child).unwrap().scale, Vec3::splat(4.0));
    let wrapper = app.world_mut().spawn((Transform::IDENTITY, Name::new("OwnedWrapper"), ChildOf(first.0))).id();
    app.world_mut().entity_mut(child).insert(ChildOf(wrapper));
    *app.world_mut().get_mut::<Transform>(child).unwrap() = Transform::IDENTITY;
    app.update();
    assert_eq!(app.world().get::<Transform>(child).unwrap().scale, Vec3::splat(4.0),
        "reparenting within the same live model subtree does not invalidate identity");
    assert!(published(&mut app, FormId::PLAYER_REF, ModelView::FirstPerson, "OwnedRenamedBone").is_some());
}
