//! Unit tests for [`VisualRootCache`] and [`MirrorIndex::build_from_cache`]:
//! incremental spawn, despawn, respawn, reparenting, `RefVisual` mutation,
//! `DrawOnly`/`PointLight` transitions, and zero-stale-entity parity with [`root_of`].

use super::*;

#[derive(Resource, Default)]
struct TestCache {
    cache: VisualRootCache,
    wanted: HashSet<u32>,
    results: HashMap<u32, Option<Entity>>,
    oracle: HashMap<u32, Option<Entity>>,
}

fn sync_and_query(
    mut state: ResMut<TestCache>,
    rigs: Rigs,
    visuals: Visuals,
    mut changes: VisualChanges,
    nodes: NodeQuery,
    dropped: Dropped,
) {
    state.cache.sync(&visuals, &mut changes, &nodes);
    let wanted = state.wanted.clone();
    let index = MirrorIndex::build_from_cache(&wanted, &rigs, &visuals, &dropped, &mut state.cache);
    state.results.clear();
    state.oracle.clear();
    for r in wanted {
        state.results.insert(r, index.root_of(r, ModelView::Reference));
        state.oracle.insert(r, root_of(r, ModelView::Reference, &rigs, &visuals));
    }
}

fn run_step(app: &mut App, refs: &[u32]) {
    app.world_mut().resource_mut::<TestCache>().wanted = refs.iter().copied().collect();
    app.update();
    let state = app.world().resource::<TestCache>();
    for &r in refs {
        assert_eq!(
            state.results.get(&r).copied().flatten(),
            state.oracle.get(&r).copied().flatten(),
            "cached root mismatch for 0x{r:08X}",
        );
    }
}

#[test]
fn visual_root_cache_tracks_add_remove_respawn_reparent_and_filter_changes() {
    const R_A: u32 = 0x5001;
    const R_B: u32 = 0x5002;
    const R_C: u32 = 0x5003;
    const R_D: u32 = 0x5004;
    const R_MISSING: u32 = 0x5099;
    let tracked = [R_A, R_B, R_C, R_D, R_MISSING];

    let mut app = App::new();
    app.init_resource::<TestCache>();
    app.add_systems(Update, sync_and_query);

    // Frame 1 (cold init): spawn hierarchy, light-only, DrawOnly.
    let a_root = app.world_mut().spawn((Transform::IDENTITY, RefVisual(R_A))).id();
    let a_child = app.world_mut().spawn((Transform::IDENTITY, RefVisual(R_A), ChildOf(a_root))).id();
    let b_draw_only = app.world_mut().spawn((Transform::IDENTITY, RefVisual(R_B), nvf_play::DrawOnly)).id();
    app.world_mut().spawn((PointLight::default(), RefVisual(R_C)));

    run_step(&mut app, &tracked);
    assert_eq!(app.world().resource::<TestCache>().results[&R_A], Some(a_root));
    assert_eq!(app.world().resource::<TestCache>().results[&R_B], None);
    assert_eq!(app.world().resource::<TestCache>().results[&R_C], None);
    assert_eq!(app.world().resource::<TestCache>().results[&R_MISSING], None);

    // Frame 2 (incremental): spawn R_C real visual, remove DrawOnly on R_B, remove RefVisual on a_root so a_child becomes root.
    let c_real = app.world_mut().spawn((Transform::IDENTITY, RefVisual(R_C))).id();
    app.world_mut().entity_mut(b_draw_only).remove::<nvf_play::DrawOnly>();
    app.world_mut().entity_mut(a_root).remove::<RefVisual>();

    run_step(&mut app, &tracked);
    assert_eq!(app.world().resource::<TestCache>().results[&R_A], Some(a_child));
    assert_eq!(app.world().resource::<TestCache>().results[&R_B], Some(b_draw_only));
    assert_eq!(app.world().resource::<TestCache>().results[&R_C], Some(c_real));

    // Frame 3 (despawn + respawn): despawn c_real -> None; then respawn new entity -> new entity (zero stale entity).
    app.world_mut().despawn(c_real);
    run_step(&mut app, &tracked);
    assert_eq!(app.world().resource::<TestCache>().results[&R_C], None);

    let c_respawn = app.world_mut().spawn((Transform::IDENTITY, RefVisual(R_C))).id();
    run_step(&mut app, &tracked);
    assert_eq!(app.world().resource::<TestCache>().results[&R_C], Some(c_respawn));
    assert_ne!(c_respawn, c_real);

    // Frame 4 (reparent + RefVisual mutation): spawn D root, reparent c_respawn under a new C parent, mutate R_B -> R_D.
    let c_new_parent = app.world_mut().spawn((Transform::IDENTITY, RefVisual(R_C))).id();
    app.world_mut().entity_mut(c_respawn).insert(ChildOf(c_new_parent));
    app.world_mut().entity_mut(b_draw_only).insert(RefVisual(R_D));

    run_step(&mut app, &tracked);
    assert_eq!(app.world().resource::<TestCache>().results[&R_C], Some(c_new_parent));
    assert_eq!(app.world().resource::<TestCache>().results[&R_B], None);
    assert_eq!(app.world().resource::<TestCache>().results[&R_D], Some(b_draw_only));

    // Frame 5 (filter addition): adding DrawOnly or PointLight invalidates cached entry lazily with zero stale return.
    app.world_mut().entity_mut(b_draw_only).insert(nvf_play::DrawOnly);
    app.world_mut().entity_mut(a_child).insert(PointLight::default());

    run_step(&mut app, &tracked);
    assert_eq!(app.world().resource::<TestCache>().results[&R_D], None);
    assert_eq!(app.world().resource::<TestCache>().results[&R_A], None);
}

#[test]
fn visual_root_cache_handles_thousands_of_undrawn_watched_refs_incrementally() {
    let mut app = App::new();
    app.init_resource::<TestCache>();
    app.add_systems(Update, sync_and_query);

    let mut target = None;
    for i in 0..2_000u32 {
        let r = 0x0020_0000 + i;
        let root = app.world_mut().spawn((Transform::IDENTITY, RefVisual(r))).id();
        app.world_mut().spawn((Transform::IDENTITY, RefVisual(r), ChildOf(root)));
        if i == 1_234 {
            target = Some((r, root));
        }
    }
    let (drawn_ref, drawn_root) = target.unwrap();
    let mut watched: Vec<u32> = (0..2_500u32).map(|i| 0x4200_0000 + i).collect();
    watched.push(drawn_ref);

    // Frame 1: cold init.
    run_step(&mut app, &watched);
    assert_eq!(app.world().resource::<TestCache>().results[&drawn_ref], Some(drawn_root));

    // Frame 2: steady-state (zero entity changes) still resolves drawn_ref and None for all 2,500 undrawn refs.
    run_step(&mut app, &watched);
    assert_eq!(app.world().resource::<TestCache>().results[&drawn_ref], Some(drawn_root));
    for i in 0..2_500u32 {
        assert_eq!(app.world().resource::<TestCache>().results[&(0x4200_0000 + i)], None);
    }

    // Frame 3: despawn drawn_root and spawn a fresh one for a previously undrawn ref.
    let late_ref = 0x4200_0777u32;
    app.world_mut().despawn(drawn_root);
    let late_entity = app.world_mut().spawn((Transform::IDENTITY, RefVisual(late_ref))).id();
    run_step(&mut app, &watched);
    assert_eq!(app.world().resource::<TestCache>().results[&drawn_ref], None);
    assert_eq!(app.world().resource::<TestCache>().results[&late_ref], Some(late_entity));
}
