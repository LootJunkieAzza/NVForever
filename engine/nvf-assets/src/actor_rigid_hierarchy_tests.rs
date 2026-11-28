use super::*;

fn rigid_shape(
    name: &str,
    ancestors: Vec<(Option<String>, Affine3A)>,
    own: Affine3A,
) -> PartShape {
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default());
    mesh.insert_attribute(
        Mesh::ATTRIBUTE_POSITION,
        vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]],
    );
    let transform = ancestors
        .iter()
        .rev()
        .fold(Affine3A::IDENTITY, |acc, (_, t)| acc * *t)
        * own;
    PartShape {
        name: Some(name.into()),
        mesh: Handle::default(),
        material: Handle::default(),
        props: Props::default(),
        cpu: Arc::new(mesh),
        skin: None,
        transform,
        cap: false,
        partitions: Arc::default(),
        ancestors,
        own,
        geometry_index: 0,
    }
}

fn forearm_skeleton() -> PreparedSkeleton {
    let forearm = Transform::from_translation(Vec3::new(-29.0, -3.7, 94.4))
        .with_rotation(Quat::from_rotation_z(0.5));
    PreparedSkeleton {
        names: vec![Some("Scene Root".into()), Some("Bip01 L ForeTwist".into())],
        parents: vec![None, Some(0)],
        locals: vec![Transform::IDENTITY, forearm],
        bind_world: vec![Affine3A::IDENTITY, forearm.compute_affine()],
        by_name: HashMap::from([
            ("Scene Root".into(), 0),
            ("Bip01 L ForeTwist".into(), 1),
        ]),
        bbx_half_width: None,
    }
}

#[test]
fn pipboy_rigid_hierarchy_keeps_root_and_children_aligned_under_scripted_reset() {
    let skel = forearm_skeleton();
    let housing_t = Affine3A::from_rotation_translation(
        Quat::from_rotation_x(10.9_f32.to_radians()),
        Vec3::new(8.086, -0.003, -0.001),
    );
    let screen_node_t = Affine3A::from_translation(Vec3::new(11.287, 1.026, 3.935));
    let glow_node_t = Affine3A::from_translation(Vec3::new(6.1, -1.2, 2.5));
    let rad_node_t = Affine3A::from_translation(Vec3::new(5.4, 2.0, 1.1));
    let leaf_offset = Affine3A::from_translation(Vec3::new(0.2, 0.0, 0.0));

    let housing = rigid_shape("PipBoyArm:0", vec![], housing_t);
    let screen = rigid_shape(
        "pipboyscreen:0",
        vec![(Some("pipboyscreen".into()), screen_node_t)],
        Affine3A::IDENTITY,
    );
    let glow = rigid_shape(
        "StatsGlow:0",
        vec![(Some("StatsGlow".into()), glow_node_t)],
        Affine3A::IDENTITY,
    );
    let needle0 = rigid_shape(
        "RadNeedle:0",
        vec![(Some("RadNeedle".into()), rad_node_t)],
        leaf_offset,
    );
    let needle1 = rigid_shape(
        "RadNeedle:1",
        vec![(Some("RadNeedle".into()), rad_node_t)],
        Affine3A::IDENTITY,
    );

    let part = PartUse {
        part: Arc::new(PreparedPart {
            prn: Some("Bip01 L ForeTwist".into()),
            root_name: Some("PipBoyArm".into()),
            root: Affine3A::IDENTITY,
            shapes: vec![housing, screen, glow, needle0, needle1],
            ..Default::default()
        }),
        materials: Vec::new(),
        meshes: Vec::new(),
        layers: Vec::new(),
        hidden_shapes: Vec::new(),
        by_ancestor: false,
    };

    let mut app = App::new();
    app.add_plugins(bevy::transform::TransformPlugin);
    let parent = app.world_mut().spawn(Transform::IDENTITY).id();
    let spawned = spawn_actor(
        &mut app.world_mut().commands(),
        parent,
        Transform::IDENTITY,
        &skel,
        &[part],
    );
    app.world_mut().flush();
    app.update();

    let foretwist = spawned.nodes[1];
    let direct_children: Vec<Entity> = app
        .world()
        .get::<Children>(foretwist)
        .expect("foretwist children")
        .iter()
        .collect();
    assert_eq!(
        direct_children.len(),
        1,
        "Bip01 L ForeTwist must have PipBoyArm as its sole direct child, not flattened leaf meshes"
    );
    let pipboy_root = direct_children[0];
    assert_eq!(
        app.world().get::<Name>(pipboy_root).map(Name::as_str),
        Some("PipBoyArm")
    );
    assert_eq!(spawned.rigid_roots, vec![(0, pipboy_root)]);

    // PipBoyArm:0 sits under PipBoyArm; pipboyscreen:0 sits under PipBoyArm/pipboyscreen;
    // RadNeedle:0 and RadNeedle:1 share a single RadNeedle parent node.
    let housing_e = spawned.drawn[0];
    let screen_e = spawned.drawn[1];
    let glow_e = spawned.drawn[2];
    let needle0_e = spawned.drawn[3];
    let needle1_e = spawned.drawn[4];

    assert_eq!(app.world().get::<ChildOf>(housing_e).unwrap().parent(), pipboy_root);
    let screen_parent = app.world().get::<ChildOf>(screen_e).unwrap().parent();
    assert_eq!(app.world().get::<Name>(screen_parent).map(Name::as_str), Some("pipboyscreen"));
    assert_eq!(app.world().get::<ChildOf>(screen_parent).unwrap().parent(), pipboy_root);
    let glow_parent = app.world().get::<ChildOf>(glow_e).unwrap().parent();
    assert_eq!(app.world().get::<Name>(glow_parent).map(Name::as_str), Some("StatsGlow"));
    let rad_parent_0 = app.world().get::<ChildOf>(needle0_e).unwrap().parent();
    let rad_parent_1 = app.world().get::<ChildOf>(needle1_e).unwrap().parent();
    assert_eq!(rad_parent_0, rad_parent_1, "sibling shapes under RadNeedle share one node");
    assert_eq!(app.world().get::<Name>(rad_parent_0).map(Name::as_str), Some("RadNeedle"));

    // Verify bind-pose world transforms match bone * shape.transform exactly.
    let bone_world = app.world().get::<GlobalTransform>(foretwist).unwrap().affine();
    let housing_world_before = app.world().get::<GlobalTransform>(housing_e).unwrap().affine();
    let screen_world_before = app.world().get::<GlobalTransform>(screen_e).unwrap().affine();
    assert!(Mat4::from(housing_world_before).abs_diff_eq(Mat4::from(bone_world * housing_t), 1e-4));
    assert!(Mat4::from(screen_world_before).abs_diff_eq(Mat4::from(bone_world * screen_node_t), 1e-4));

    // Simulate what mods do via GetChildBlocks "Bip01 L ForeTwist" 2 -> aForms[0]:
    // resetting the first child's local TRS to identity (0,0,0 / 0,0,0 / 1) must leave
    // the housing and screen aligned at their bind-pose offset!
    *app.world_mut().get_mut::<Transform>(pipboy_root).unwrap() = Transform::IDENTITY;
    app.update();

    let housing_world_after = app.world().get::<GlobalTransform>(housing_e).unwrap().affine();
    let screen_world_after = app.world().get::<GlobalTransform>(screen_e).unwrap().affine();
    assert!(
        Mat4::from(housing_world_after).abs_diff_eq(Mat4::from(housing_world_before), 1e-4),
        "resetting PipBoyArm root to identity must not detach PipBoyArm:0"
    );
    assert!(
        Mat4::from(screen_world_after).abs_diff_eq(Mat4::from(screen_world_before), 1e-4),
        "screen must remain aligned with housing"
    );
}

#[test]
fn rigid_part_chain_folds_unnamed_intermediates_and_preserves_head_frame_product() {
    let head_frame = Affine3A::from_rotation_translation(
        Quat::from_rotation_y(-std::f32::consts::FRAC_PI_2),
        Vec3::new(0.0, 1.5, 2.0),
    );
    let u1 = Affine3A::from_translation(Vec3::new(1.0, 0.0, 0.0));
    let named_t = Affine3A::from_translation(Vec3::new(0.0, 2.0, 0.0));
    let u2 = Affine3A::from_translation(Vec3::new(0.0, 0.0, 3.0));
    let own = Affine3A::from_translation(Vec3::new(0.5, 0.25, 0.1));

    let s = rigid_shape(
        "HatMesh:0",
        vec![(None, u1), (Some("HatNode".into()), named_t), (None, u2)],
        own,
    );
    let part = PreparedPart {
        prn: Some(DEFAULT_RIGID_BONE.into()),
        root_name: Some("HatRoot".into()),
        root: Affine3A::IDENTITY,
        shapes: vec![s.clone()],
        ..Default::default()
    };

    let (chain, local) = rigid_part_chain(&part, head_frame, &s);
    assert_eq!(chain.len(), 2);
    assert_eq!(chain[0].0.as_deref(), Some("HatRoot"));
    assert_eq!(chain[1].0.as_deref(), Some("HatNode"));

    let combined = chain
        .iter()
        .fold(Affine3A::IDENTITY, |acc, (_, t)| acc * *t)
        * local;
    let expected = head_frame * s.transform;
    assert!(Mat4::from(combined).abs_diff_eq(Mat4::from(expected), 1e-4));

    // Unnamed part root with no named ancestors collapses to an empty chain.
    let anon_shape = rigid_shape("Anon:0", vec![(None, u1)], own);
    let anon_part = PreparedPart {
        root_name: None,
        shapes: vec![anon_shape.clone()],
        ..Default::default()
    };
    let (anon_chain, anon_local) = rigid_part_chain(&anon_part, head_frame, &anon_shape);
    assert!(anon_chain.is_empty());
    assert!(Mat4::from(anon_local).abs_diff_eq(Mat4::from(head_frame * anon_shape.transform), 1e-4));
}
