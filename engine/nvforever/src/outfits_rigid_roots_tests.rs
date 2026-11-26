use super::*;
use nvf_assets::actor::DEFAULT_RIGID_BONE;

fn head_and_foretwist_skeleton_nif() -> Vec<u8> {
    let mut root = av_object(0);
    words(&mut root, &[3, 1, 2, 3, 0]);
    let mut weapon = av_object(1);
    words(&mut weapon, &[0, 0]);
    let mut head = av_object(2);
    words(&mut head, &[0, 0]);
    let mut foretwist = av_object(3);
    words(&mut foretwist, &[0, 0]);
    nif(
        &[("NiNode", root), ("NiNode", weapon), ("NiNode", head), ("NiNode", foretwist)],
        &["Scene Root", "Weapon", DEFAULT_RIGID_BONE, "Bip01 L ForeTwist"],
    )
}

#[test]
fn rigid_part_roots_stay_out_of_dressed_parts_and_despawn_without_orphans_on_swap() {
    let (mut app, dir) = asset_fixture();
    // Replace the minimal fixture skeleton with one carrying Bip01 Head and Bip01 L ForeTwist
    // so rigid parts with named root NiNodes spawn real rigid_roots under the bone.
    std::fs::write(dir.path().join("meshes\\skeleton.nif"), head_and_foretwist_skeleton_nif()).unwrap();

    let forms: &'static fnv_sim::FormStore = Box::leak(Box::new(actor_forms(vec![])));
    let refs: &'static fnv_sim::RefTable = Box::leak(Box::default());
    let mut sim = fnv_sim::World::new(forms, refs, Default::default());
    let id = sim.place(OTHER, None, [0.0; 3], [0.0; 3], 1).unwrap();
    sim.get(id).unwrap().inventory = Some(equipped());
    app.insert_resource(SimState { forms, refs, world: sim });

    let parent = app.world_mut().spawn_empty().id();
    app.world_mut()
        .run_system_once(
            move |mut commands: Commands,
                  world: Res<ActorWorld>,
                  sim: Res<SimState>,
                  mut parts: ResMut<ActorParts>,
                  mut models: ModelCtx| {
                let mut ctx = ActorCtx { cache: &mut parts, models: &mut models };
                let mut stats = ActorStats::default();
                assert!(crate::actors::spawn(
                    &mut commands,
                    parent,
                    bevy::math::Affine3A::IDENTITY,
                    Vec3::ZERO,
                    id.0,
                    OTHER.0,
                    &world,
                    Some(&sim.world),
                    &mut ctx,
                    &mut stats,
                    false,
                ));
            },
        )
        .unwrap();

    let actor = {
        let w = app.world_mut();
        w.query_filtered::<Entity, With<Dressed>>().single(w).unwrap()
    };
    let head_bone = {
        let rig = app.world().get::<ActorRig>(actor).unwrap();
        let idx = *rig.skel.by_name.get(DEFAULT_RIGID_BONE).expect("Bip01 Head bone");
        rig.nodes[idx]
    };

    // Dressed::parts holds strictly mesh shapes with RefVisual + ActorBoundsCandidate,
    // while Dressed::rigid_roots holds the named NiNode root under Bip01 Head.
    let initial_shapes = dressed_shapes(&app, actor);
    assert!(!initial_shapes.is_empty());
    assert_eq!(new_shapes_refer_to(&app, actor, id.0, &[]), initial_shapes.len());
    for &e in &initial_shapes {
        assert!(app.world().get::<Mesh3d>(e).is_some(), "Dressed::parts must contain only meshes");
    }

    let initial_roots: Vec<Entity> = app
        .world()
        .get::<Dressed>(actor)
        .unwrap()
        .rigid_roots
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .collect();
    assert_eq!(initial_roots.len(), 1, "rigid armour part spawns one root NiNode under Bip01 Head");
    let armour_root = initial_roots[0];
    assert!(app.world().get::<Mesh3d>(armour_root).is_none(), "rigid root is a NiNode, not a mesh");
    assert_eq!(app.world().get::<Name>(armour_root).map(Name::as_str), Some("Root"));
    assert_eq!(app.world().get::<ChildOf>(armour_root).unwrap().parent(), head_bone);
    assert_eq!(app.world().get::<Children>(head_bone).unwrap().len(), 1);

    // Unequip armour: old leaf shape and old rigid root NiNode both despawn; bare body
    // takes their place with zero orphan NiNodes left under Bip01 Head.
    app.world_mut().resource_mut::<SimState>().world.get(id).unwrap().inventory = Some(Inventory::new());
    app.world_mut().run_system_once(follow_inventory).unwrap();

    for &e in &initial_shapes {
        assert!(app.world().get_entity(e).is_err(), "old armour mesh must be despawned");
    }
    assert!(app.world().get_entity(armour_root).is_err(), "old armour rigid root NiNode must be despawned");

    let bare_shapes = dressed_shapes(&app, actor);
    assert!(!bare_shapes.is_empty());
    assert_eq!(new_shapes_refer_to(&app, actor, id.0, &initial_shapes), bare_shapes.len());
    let bare_roots: Vec<Entity> = app
        .world()
        .get::<Dressed>(actor)
        .unwrap()
        .rigid_roots
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .collect();
    assert_eq!(bare_roots.len(), 1);
    let bare_root = bare_roots[0];
    assert_ne!(bare_root, armour_root);
    assert_eq!(
        app.world().get::<Children>(head_bone).unwrap().iter().collect::<Vec<_>>(),
        vec![bare_root],
        "no orphan NiNodes left under Bip01 Head after unequip"
    );

    // Re-equip armour: bare body leaf and rigid root both despawn cleanly.
    app.world_mut().resource_mut::<SimState>().world.get(id).unwrap().inventory = Some(equipped());
    app.world_mut().run_system_once(follow_inventory).unwrap();

    for &e in &bare_shapes {
        assert!(app.world().get_entity(e).is_err(), "bare mesh must be despawned on re-equip");
    }
    assert!(app.world().get_entity(bare_root).is_err(), "bare rigid root NiNode must be despawned on re-equip");
    let re_roots: Vec<Entity> = app
        .world()
        .get::<Dressed>(actor)
        .unwrap()
        .rigid_roots
        .iter()
        .flat_map(|(_, v)| v.iter().copied())
        .collect();
    assert_eq!(re_roots.len(), 1);
    assert_eq!(
        app.world().get::<Children>(head_bone).unwrap().iter().collect::<Vec<_>>(),
        re_roots,
        "no orphan NiNodes left under Bip01 Head after re-equip"
    );
    assert_eq!(
        new_shapes_refer_to(&app, actor, id.0, &bare_shapes),
        dressed_shapes(&app, actor).len()
    );
}
