use crate::*;
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use rapier3d::prelude::*;

fn shape(motion:Motion) -> CollisionObject {
    CollisionObject {node:None,node_block:0,node_transform:Iso::IDENTITY,body_transform:Iso::IDENTITY,
        motion,layer:1,mass:10.,friction:0.5,restitution:0.,shape:Shape::Box {half_extents:[10.;3]}.into(),shell:0.,material:None,wind:false}
}
fn pose(id:u32, position:[f32;3])->RefPose {RefPose {form_id:id,position,rotation:[1.,0.,0.,0.,1.,0.,0.,0.,1.],scale:1.}}
fn world()->(PhysWorld,RigidBodyHandle) {
    let mut w=PhysWorld::new(PhysParams {gravity:0.,..Default::default()});
    w.add_ref(&pose(99,[1000.,0.,500.]),&[shape(Motion::Dynamic)]);
    w.add_ref(&pose(98,[0.;3]),&[CollisionObject {shape:Shape::Box {half_extents:[70.;3]}.into(),..shape(Motion::Static)}]);
    w.step();let handle=w.refs[&99][0];assert!(!w.w.bodies[handle].is_sleeping());assert_eq!(w.stats().bodies_awake,1);(w,handle)
}

// Query refresh may find contacts, but must preserve motion queued for the next step.
// Compare with a direct normal step from the same complete world snapshot.
fn observed_new_kinematic_query_behavior(position_based: bool) {
    let (mut queried, neighbor) = world();
    let builder = if position_based { RigidBodyBuilder::kinematic_position_based() }
        else { RigidBodyBuilder::kinematic_velocity_based() };
    let h = queried.insert_body(builder.translation(rapier3d::math::Vector::new(0., 0., 1.15)));
    let collider = queried.w.insert_collider(ColliderBuilder::cuboid(0.25, 0.25, 0.25)
        .active_collision_types(ActiveCollisionTypes::all()), Some(h));
    if position_based { queried.w.bodies[h].set_next_kinematic_translation(rapier3d::math::Vector::new(1., 0., 1.15)); }
    else { queried.w.bodies[h].set_linvel(rapier3d::math::Vector::new(0.5, 0., 0.), false); }
    queried.w.bodies[neighbor].add_force(rapier3d::math::Vector::new(20., 0., 0.), true);
    queried.w.bodies[neighbor].add_torque(rapier3d::math::Vector::new(1., 2., 3.), true);
    queried.mark_dirty_bodies();
    let mut direct = queried.snapshot();
    let current = *queried.w.bodies[h].position();
    let target = *queried.w.bodies[h].next_position();
    let velocity = queried.w.bodies[h].linvel();
    let force = queried.w.bodies[neighbor].user_force();
    let torque = queried.w.bodies[neighbor].user_torque();
    let steps = queried.steps();
    assert!(force.length_squared() > 0. && torque.length_squared() > 0.);
    queried.update_queries();
    let target_after_query = *queried.w.bodies[h].next_position();
    assert_eq!(queried.steps(), steps);
    assert_eq!(*queried.w.bodies[h].position(), current);
    assert_eq!(queried.w.bodies[h].linvel(), velocity);
    assert_eq!(queried.w.bodies[neighbor].user_force(), force);
    assert_eq!(queried.w.bodies[neighbor].user_torque(), torque);
    assert!(queried.w.contact_pairs_with(collider).next().is_some(), "new collider has its actual fixed-support contact");
    let mut first = None;
    for frame in 1..=15 {
        queried.step(); direct.step();
        let q = *queried.w.bodies[h].position(); let d = *direct.w.bodies[h].position();
        if frame == 1 { first = Some((q, d)); }
        assert!(queried.island_split().is_none() && direct.island_split().is_none(), "frame {frame}");
        assert_eq!(queried.ref_bodies(99), direct.ref_bodies(99), "unrelated force recipient at frame {frame}");
        assert_eq!(queried.w.bodies[neighbor].angvel(), direct.w.bodies[neighbor].angvel(), "queued torque at frame {frame}");
    }
    let (first_q, first_d) = first.unwrap();
    eprintln!("position_based={position_based}; query target {target:?} -> {target_after_query:?}; first query/direct={first_q:?}/{first_d:?}; final query/direct={:?}/{:?}", queried.w.bodies[h].position(), direct.w.bodies[h].position());
    // Run both first/later simulation paths before reporting any target mismatch.
    assert_eq!(target_after_query, target, "query must retain the authored next kinematic pose");
    assert!(first_d.translation.x > current.translation.x, "direct normal step is the positive motion control");
    assert_eq!(first_q, first_d, "first-step pose must agree with direct integration");
    assert_eq!(queried.w.bodies[h].position(), direct.w.bodies[h].position(), "later-step pose");
    assert_eq!(queried.w.bodies[h].linvel(), direct.w.bodies[h].linvel(), "later-step velocity");
    assert!(queried.w.bodies[neighbor].linvel().x > 0. && queried.w.bodies[neighbor].angvel().length_squared() > 0., "queued force and torque acted");
}

#[test]
fn observable_query_preserves_new_position_kinematic_target_and_later_motion() {
    observed_new_kinematic_query_behavior(true);
}

#[test]
fn observable_query_preserves_new_velocity_kinematic_motion_and_neighbor_force() {
    observed_new_kinematic_query_behavior(false);
}

#[test]
fn observable_first_character_move_survives_unrelated_insert_then_query() {
    let (mut queried, _) = world();
    let mut character = queried.add_character(7, CharacterParams::default(), [3000., 0., 500.]);
    let h = queried.w.bodies.iter().find_map(|(h,b)| (Owner::decode(b.user_data) == Some(Owner::Character { id: 7 })).then_some(h)).unwrap();
    let current = *queried.w.bodies[h].position();
    let planned = queried.move_character(&mut character, CharacterInput { velocity: [120., 0.], ..Default::default() });
    let target = *queried.w.bodies[h].next_position();
    assert!(planned.feet[0] > 3000. && target.translation.x > current.translation.x, "actual controller produced a positive queued move: {planned:?}");
    // A normal public insertion dirties queries after apply_move but before step.
    assert_eq!(queried.add_ref(&pose(200, [5000., 0., 500.]), &[shape(Motion::Static)]).bodies, 1);
    let mut direct = queried.snapshot(); let steps = queried.steps();
    queried.update_queries();
    let target_after_query = *queried.w.bodies[h].next_position();
    assert_eq!(queried.steps(), steps); assert_eq!(*queried.w.bodies[h].position(), current);
    queried.step(); direct.step();
    let q = *queried.w.bodies[h].position(); let d = *direct.w.bodies[h].position();
    eprintln!("actual character move target={target:?}; after query={target_after_query:?}; first-step query/direct={q:?}/{d:?}");
    assert_eq!(d, target, "direct step consumes the actual controller target");
    assert_eq!(target_after_query, target, "unrelated query cannot erase the public controller's queued movement");
    assert_eq!(q, d, "physical character must reach the same first-step pose");
    assert_eq!(queried.w.bodies[h].linvel(), direct.w.bodies[h].linvel());
}
