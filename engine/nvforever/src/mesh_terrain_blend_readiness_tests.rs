use super::*;

#[test]
fn acknowledge_materials_skips_when_settled_and_rechecks_on_rev_or_hold_change() {
    use bevy::render::erased_render_asset::ErasedRenderAssets;
    use bevy::render::material_bind_groups::MaterialBindGroupAllocators;

    let mut app = App::new();
    let shared = BlendReadiness::default();
    app.insert_resource(shared.clone())
        .insert_resource(ReadinessInput::default())
        .init_resource::<ErasedRenderAssets<bevy::pbr::PreparedMaterial>>()
        .init_resource::<MaterialBindGroupAllocators>()
        .add_systems(Update, acknowledge_materials);

    // Empty set settles on the first frame (ready.len() == 0 == all_count).
    app.update();
    assert!(shared.ready.lock().unwrap().is_empty());

    // Inject a sentinel into `shared.ready` to verify settled_state skips rewriting on steady frames.
    let sentinel = AssetId::<NifBlendMaterial>::default();
    shared.ready.lock().unwrap().insert(sentinel);
    app.update();
    assert!(
        shared.ready.lock().unwrap().contains(&sentinel),
        "settled steady frame must skip rebuilding shared.ready",
    );

    // Changing `ids_rev` invalidates `settled_state` and re-runs acknowledgement.
    app.world_mut().resource_mut::<ReadinessInput>().ids_rev += 1;
    app.update();
    assert!(
        !shared.ready.lock().unwrap().contains(&sentinel),
        "ids_rev bump must re-run acknowledgement",
    );

    // Changing `hold` also invalidates `settled_state`.
    shared.ready.lock().unwrap().insert(sentinel);
    shared.hold(1);
    app.update();
    assert!(
        !shared.ready.lock().unwrap().contains(&sentinel),
        "hold step change must re-run acknowledgement",
    );
}
