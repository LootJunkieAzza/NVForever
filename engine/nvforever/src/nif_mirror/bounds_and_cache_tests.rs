use super::*;
use bevy::ecs::system::RunSystemOnce;
use fnv_game::game::Game;
use nvf_assets::particles::{Emitter, ParticleSystemDef};

#[derive(Resource)]
struct CacheTestGame(Game<'static>);

#[derive(Resource, Default)]
struct CacheTestMemory(Memory);

#[allow(clippy::too_many_arguments)]
fn produce_with_cache(
    mut game: ResMut<CacheTestGame>,
    rigs: Rigs,
    visuals: Visuals,
    parents: Query<&ChildOf>,
    nodes: NodeQuery,
    changed_nodes: ChangedNodes,
    mut removed_nodes: RemovedNodes,
    commands: Commands,
    mut memory: ResMut<CacheTestMemory>,
    materials: Materials,
    shapes: Shapes,
    extra: Extra,
    dropped: Dropped,
) {
    mirror_game_impl(
        &mut game.0,
        rigs,
        visuals,
        parents,
        nodes,
        Some(&changed_nodes),
        Some(&mut removed_nodes),
        commands,
        &mut memory.0,
        materials,
        shapes,
        extra,
        dropped,
        None,
    );
}

#[test]
fn compute_node_bounds_accumulates_descendants_and_reuses_scratch() {
    let mut world = World::new();
    let root = world.spawn(GlobalTransform::from(Transform::from_xyz(0.0, 0.0, 0.0))).id();
    let branch = world.spawn(GlobalTransform::from(Transform::from_xyz(10.0, 0.0, 0.0))).id();
    let leaf_a = world
        .spawn((
            GlobalTransform::from(Transform::from_xyz(10.0, 2.0, 0.0).with_scale(Vec3::new(2.0, 1.0, 1.0))),
            Aabb::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)),
        ))
        .id();
    let leaf_b = world
        .spawn((
            GlobalTransform::from(Transform::from_xyz(10.0, -2.0, 0.0)),
            Aabb::from_min_max(Vec3::new(-0.5, -0.5, -0.5), Vec3::new(0.5, 0.5, 0.5)),
        ))
        .id();
    let empty_sibling = world.spawn(GlobalTransform::from(Transform::from_xyz(-5.0, 0.0, 0.0))).id();

    let tree = vec![
        (root, None),
        (branch, Some(0)),
        (leaf_a, Some(1)),
        (leaf_b, Some(1)),
        (empty_sibling, Some(0)),
    ];
    let smaller_tree = vec![(empty_sibling, None)];

    world
        .run_system_once(move |globals: Query<&GlobalTransform>, boxes: Query<&Aabb>| {
            let mut scratch = Vec::new();
            let bounds = compute_node_bounds(&tree, &globals, &boxes, &mut scratch);
            assert_eq!(bounds.len(), 5);
            // Empty sibling has radius 0 at its own position.
            assert_eq!(bounds[4].unwrap()[3], 0.0);
            // Leaf A has half_extents = (1,1,1) => length sqrt(3), max axis scale = 2.0.
            let expected_a_r = Vec3::ONE.length() * 2.0;
            assert!((bounds[2].unwrap()[3] - expected_a_r).abs() < 1e-4);
            // Branch and root both enclose leaf_a and leaf_b, so they share identical bounds.
            assert_eq!(bounds[0], bounds[1]);
            assert!(bounds[0].unwrap()[3] > expected_a_r);

            // Reusing `scratch` on a shorter tree must clear buckets so stale spheres do not leak.
            let second = compute_node_bounds(&smaller_tree, &globals, &boxes, &mut scratch);
            assert_eq!(second.len(), 1);
            assert_eq!(second[0].unwrap()[3], 0.0);
            assert!(scratch.len() >= 5, "scratch capacity should be reused");
        })
        .unwrap();
}

#[test]
fn cistr_deduplicates_ascii_case_insensitively() {
    let mut seen: HashMap<CiStr<'_>, u32> = HashMap::new();
    for name in ["Object", "object", "OBJECT", "Other"] {
        *seen.entry(CiStr(name)).or_insert(0) += 1;
    }
    assert_eq!(seen.len(), 2);
    assert_eq!(seen.get(&CiStr("oBjEcT")), Some(&3));
    assert_eq!(seen.get(&CiStr("other")), Some(&1));
}

#[test]
fn placed_ref_set_tracks_only_placed_particle_owners() {
    let mut particles = crate::particles::Particles::default();
    assert!(crate::particles::placed_ref_set(&particles).is_empty());

    let mut world = World::new();
    let placed_owner = world.spawn_empty().id();
    let attached_owner = world.spawn_empty().id();
    let e1 = world.spawn_empty().id();
    let e2 = world.spawn_empty().id();
    let def = ParticleSystemDef {
        name: "Smoke".into(),
        max: 16,
        material: Handle::default(),
        look: nvf_assets::material_anim::Look::stored([0.0; 3], 1.0, 1.0),
        emitter: Emitter {
            frame: bevy::math::Affine3A::IDENTITY,
            size: Vec3::ONE,
            round: false,
            speed: 1.0,
            speed_variation: 0.0,
            declination: 0.0,
            declination_variation: 0.0,
            planar_angle: 0.0,
            planar_angle_variation: 0.0,
            color: [1.0; 4],
            radius: 1.0,
            radius_variation: 0.0,
            life: 1.0,
            life_variation: 0.0,
            mesh: None,
            keyed: Vec::new(),
            object: None,
        },
        rate: 1.0,
        rate_keys: None,
        active: vec![(0.0, true)],
        period: 1.0,
        loops: true,
        sequences: Vec::new(),
        max_emitters: 0,
        grow_time: 0.0,
        fade_time: 0.0,
        base_scale: 1.0,
        system_scale: 1.0,
        color: None,
        color_keys: None,
        rotation: None,
        gravity: Vec::new(),
        drag: Vec::new(),
        bombs: Vec::new(),
        colliders: Vec::new(),
        strip: None,
        subtextures: Vec::new(),
        mover: None,
        alpha_keys: None,
        soft: 0.0,
    };
    crate::particles::seed_test_system(&mut particles, placed_owner, 0x0010_0001, false, "a.nif", def.clone(), Handle::default(), e1);
    crate::particles::seed_test_system(&mut particles, attached_owner, 0x0010_0002, true, "b.nif", def, Handle::default(), e2);

    let refs = crate::particles::placed_ref_set(&particles);
    assert!(refs.contains(&0x0010_0001));
    assert!(!refs.contains(&0x0010_0002));
}

#[test]
fn mirror_published_cache_skips_unchanged_and_invalidates_on_transform_or_hidden_removal() {
    const R: u32 = 0x0012_3456;
    let mut app = App::new();
    app.init_resource::<Assets<NifMaterial>>();
    app.init_resource::<Assets<Mesh>>();

    let forms = Box::leak(Box::<fnv_sim::FormStore>::default());
    let refs = Box::leak(Box::<fnv_sim::RefTable>::default());
    let mut game = crate::outfits::tests::game(fnv_sim::World::new(forms, refs, Default::default()));
    game.scene.watch(FormId(R));

    let root = app
        .world_mut()
        .spawn((RefVisual(R), Name::new("Root"), Transform::IDENTITY, GlobalTransform::IDENTITY))
        .id();
    let child = app
        .world_mut()
        .spawn((Name::new("NodeA"), Transform::from_xyz(1.0, 2.0, 3.0), GlobalTransform::IDENTITY, NifHidden, ChildOf(root)))
        .id();

    app.insert_resource(CacheTestGame(game))
        .insert_resource(CacheTestMemory::default())
        .add_systems(Update, produce_with_cache);

    // Frame 1: initial publish records the hidden flag and child position.
    app.update();
    assert!(app.world().resource::<CacheTestMemory>().0.published_models.contains_key(&(R, ModelView::Reference)));
    let node_a = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").expect("NodeA published");
    assert_eq!(node_a.flags & 1, 1);
    assert_eq!(node_a.translation, [1.0, 2.0, 3.0]);

    // Frame 2 (no changes): steady-state fast path runs cleanly without ECS access conflicts.
    app.update();
    let node_a2 = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").unwrap();
    assert_eq!(node_a2.translation, [1.0, 2.0, 3.0]);
    assert_eq!(node_a2.flags & 1, 1);

    // Frame 3: mutate Transform -> must invalidate fast path and publish new translation.
    app.world_mut().get_mut::<Transform>(child).unwrap().translation = Vec3::new(4.0, 5.0, 6.0);
    app.update();
    let node_a3 = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").unwrap();
    assert_eq!(node_a3.translation, [4.0, 5.0, 6.0]);

    // Frame 4: remove NifHidden -> RemovedNodes must invalidate fast path and clear hidden bit.
    app.world_mut().entity_mut(child).remove::<NifHidden>();
    app.update();
    let node_a4 = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").unwrap();
    assert_eq!(node_a4.flags & 1, 0);

    // Frame 5: add Aabb, publish, then remove Aabb -> RemovedNodes must invalidate fast path and clear radius.
    app.world_mut().entity_mut(child).insert(Aabb::from_min_max(Vec3::splat(-2.0), Vec3::splat(2.0)));
    app.update();
    let node_a5 = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").unwrap();
    assert!(node_a5.info.as_ref().and_then(|i| i.bound).is_some_and(|b| b[3] > 0.0));
    app.world_mut().entity_mut(child).remove::<Aabb>();
    app.update();
    let node_a6 = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").unwrap();
    assert_eq!(node_a6.info.as_ref().and_then(|i| i.bound).map(|b| b[3]), Some(0.0));

    // Frame 6: remove Name -> RemovedNodes must invalidate fast path and clear the node's name.
    app.world_mut().entity_mut(child).remove::<Name>();
    app.update();
    assert!(app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").is_none());
}

#[test]
fn mirror_published_cache_clears_stale_emitter_when_placed_particles_disappear() {
    const R: u32 = 0x0012_3457;
    let mut app = App::new();
    app.init_resource::<Assets<NifMaterial>>();
    app.init_resource::<Assets<Mesh>>();

    let forms = Box::leak(Box::<fnv_sim::FormStore>::default());
    let refs = Box::leak(Box::<fnv_sim::RefTable>::default());
    let mut game = crate::outfits::tests::game(fnv_sim::World::new(forms, refs, Default::default()));
    game.scene.watch(FormId(R));

    let root = app
        .world_mut()
        .spawn((RefVisual(R), Name::new("Root"), Transform::IDENTITY, GlobalTransform::IDENTITY))
        .id();
    let _child = app
        .world_mut()
        .spawn((Name::new("NodeA"), Transform::IDENTITY, GlobalTransform::IDENTITY, ChildOf(root)))
        .id();
    let placed_owner = app.world_mut().spawn_empty().id();
    let particle_entity = app.world_mut().spawn_empty().id();
    let mut particles = crate::particles::Particles::default();
    let def = ParticleSystemDef {
        name: "NodeA".into(),
        max: 16,
        material: Handle::default(),
        look: nvf_assets::material_anim::Look::stored([0.0; 3], 1.0, 1.0),
        emitter: Emitter {
            frame: bevy::math::Affine3A::IDENTITY,
            size: Vec3::ONE,
            round: false,
            speed: 1.0,
            speed_variation: 0.0,
            declination: 0.0,
            declination_variation: 0.0,
            planar_angle: 0.0,
            planar_angle_variation: 0.0,
            color: [1.0; 4],
            radius: 1.0,
            radius_variation: 0.0,
            life: 1.0,
            life_variation: 0.0,
            mesh: None,
            keyed: Vec::new(),
            object: None,
        },
        rate: 2.0,
        rate_keys: None,
        active: vec![(0.0, true)],
        period: 1.0,
        loops: true,
        sequences: Vec::new(),
        max_emitters: 0,
        grow_time: 0.0,
        fade_time: 0.0,
        base_scale: 1.0,
        system_scale: 1.0,
        color: None,
        color_keys: None,
        rotation: None,
        gravity: Vec::new(),
        drag: Vec::new(),
        bombs: Vec::new(),
        colliders: Vec::new(),
        strip: None,
        subtextures: Vec::new(),
        mover: None,
        alpha_keys: None,
        soft: 0.0,
    };
    crate::particles::seed_test_system(&mut particles, placed_owner, R, false, "a.nif", def, Handle::default(), particle_entity);

    app.insert_resource(CacheTestGame(game))
        .insert_resource(CacheTestMemory::default())
        .insert_resource(particles)
        .add_systems(Update, produce_with_cache);

    // Frame 1: placed particle system is active -> emitter data is published and cache is not locked.
    app.update();
    let node_with = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").expect("NodeA published");
    assert!(node_with.info.as_ref().and_then(|i| i.emitter).is_some());
    assert!(!app.world().resource::<CacheTestMemory>().0.published_models.contains_key(&(R, ModelView::Reference)));

    // Frame 2: placed particle disappears without any ECS node change -> must republish with emitter == None.
    *app.world_mut().resource_mut::<crate::particles::Particles>() = crate::particles::Particles::default();
    app.update();
    let node_without = app.world_mut().resource_mut::<CacheTestGame>().0.scene.node_for_model(FormId(R), ModelView::Reference, "NodeA").expect("NodeA republished");
    assert!(node_without.info.as_ref().and_then(|i| i.emitter).is_none());
    assert!(app.world().resource::<CacheTestMemory>().0.published_models.contains_key(&(R, ModelView::Reference)));
}
