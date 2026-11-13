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
