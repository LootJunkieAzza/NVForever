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

#[test]
fn follow_coverage_in_place_edit_and_asset_modified_invalidate_settled_readiness() {
    use bevy::render::erased_render_asset::ErasedRenderAssets;
    use bevy::render::material_bind_groups::MaterialBindGroupAllocators;

    let mut blends = Assets::<NifBlendMaterial>::default();
    let cache = SurfaceCache::default();
    let surface = crate::terrain_surface::PublishedSurface::default();
    let mut mat = enabled_material(&NifMaterial::default(), parse("24:status").unwrap(), &surface);
    // Make the clone's current coverage differ from `cache.coverage_for(&surface)` (0)
    // so `restrict_clone` edits it in place through `blends.get_mut`.
    mat.extension.mesh_terrain_origin.z = 1.0;
    let handle = blends.add(mat);
    let id = handle.id();

    let mut variants = Variants::default();
    variants.publications.insert(id, surface.clone());
    variants.surfaces.insert(publication_key(&surface), (surface, None));
    let rev0 = variants.id_rev;

    let changed = follow_coverage(&cache, &mut variants, &mut blends, 8);
    assert_eq!(changed, 1, "restrict_clone should mutate the clone in place");
    assert_eq!(variants.id_rev, rev0.wrapping_add(1), "in-place restrict_clone edit must bump id_rev");

    // A steady follow_coverage pass with no further clone changes must not bump id_rev.
    let changed_again = follow_coverage(&cache, &mut variants, &mut blends, 8);
    assert_eq!(changed_again, 0);
    assert_eq!(variants.id_rev, rev0.wrapping_add(1));

    // Verify both `variants.id_rev` bump and `AssetEvent::Modified` bump `ReadinessInput::ids_rev`
    // and invalidate `acknowledge_materials`'s `settled_state`.
    let mut input = ReadinessInput::default();
    let mut last_rev = None;
    sync_readiness_input(&mut input, &variants, false, &mut last_rev);
    let ids_rev_after_initial = input.ids_rev;
    assert!(input.ids.contains(&id));

    // Steady frame (same id_rev, no AssetEvent::Modified): no ids_rev bump.
    sync_readiness_input(&mut input, &variants, false, &mut last_rev);
    assert_eq!(input.ids_rev, ids_rev_after_initial);

    // An external AssetEvent::Modified (e.g., HDR respecialize) bumps ids_rev even when id_rev is unchanged.
    sync_readiness_input(&mut input, &variants, true, &mut last_rev);
    assert_eq!(input.ids_rev, ids_rev_after_initial.wrapping_add(1));

    // Settle acknowledge_materials, inject a sentinel, then confirm a subsequent in-place
    // edit via `follow_coverage` -> `sync_readiness_input` invalidates `settled_state`.
    let mut app = App::new();
    let shared = BlendReadiness::default();
    app.insert_resource(shared.clone())
        .insert_resource(ReadinessInput::default())
        .init_resource::<ErasedRenderAssets<bevy::pbr::PreparedMaterial>>()
        .init_resource::<MaterialBindGroupAllocators>()
        .add_systems(Update, acknowledge_materials);
    app.update();

    let sentinel = AssetId::<NifBlendMaterial>::default();
    shared.ready.lock().unwrap().insert(sentinel);
    blends.get_mut(&handle).unwrap().extension.mesh_terrain_origin.z = 2.0;
    for (_, applied) in variants.surfaces.values_mut() {
        *applied = None;
    }
    assert_eq!(follow_coverage(&cache, &mut variants, &mut blends, 8), 1);
    {
        let mut r_input = app.world_mut().resource_mut::<ReadinessInput>();
        sync_readiness_input(&mut r_input, &variants, false, &mut last_rev);
        r_input.ids.clear(); // keep empty so acknowledge_materials re-runs and clears sentinel
    }
    app.update();
    assert!(
        !shared.ready.lock().unwrap().contains(&sentinel),
        "in-place follow_coverage edit must invalidate settled_state and re-run acknowledgement",
    );
}
