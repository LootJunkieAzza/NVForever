//! Drawing between steps (`fnv_phys::drawn`) and the quarantine of NaN state
//! (`fnv_phys::quarantine`) on hand-built worlds (no game data).

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::*;
use fnv_world::land::SIDE;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// Flat ground at z = 0 around the origin.
fn ground() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    w
}

/// A dynamic 10-unit cube.
fn cube() -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion: Motion::Dynamic,
        layer: 4,
        mass: 10.0,
        friction: 0.5,
        restitution: 0.0,
        shape: Shape::Box { half_extents: [5.0; 3] }.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

fn at(form_id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id, position, rotation: ID, scale: 1.0 }
}

/// The turn about z of a row-major rotation, radians.
fn yaw(r: &[f32; 9]) -> f32 {
    r[3].atan2(r[0])
}

/// A cube falling and spinning: `alpha` 0 draws it exactly where it was before the last
/// step, 1 exactly where it is, a half half way (its translation and its turn); stepping is
/// what moves it, never the drawing.
#[test]
fn a_moving_body_is_drawn_between_its_last_two_steps() {
    let mut w = ground();
    w.add_ref(&at(1, [0.0, 0.0, 300.0]), &[cube()]);
    w.set_script_velocity(1, 0, None, Some([0.0, 0.0, 2.0]));
    for _ in 0..5 {
        w.step();
    }
    let before = w.ref_bodies(1)[0];
    w.step();
    let after = w.ref_bodies(1)[0];
    assert!(after.translation[2] < before.translation[2] - 0.5, "falling: {before:?} {after:?}");
    let d0 = w.drawn_ref_body(1, 0, 0.0).unwrap();
    let d1 = w.drawn_ref_body(1, 0, 1.0).unwrap();
    assert_eq!((d0.translation, d0.rotation), (before.translation, before.rotation));
    assert_eq!((d1.translation, d1.rotation), (after.translation, after.rotation));
    let half = w.drawn_ref_bodies(1, 0.5)[0];
    let mid = (before.translation[2] + after.translation[2]) / 2.0;
    assert!((half.translation[2] - mid).abs() < 1e-3, "{half:?}");
    let turn = yaw(&after.rotation) - yaw(&before.rotation);
    assert!(turn.abs() > 0.02, "spinning: {turn}");
    assert!((yaw(&half.rotation) - (yaw(&before.rotation) + turn / 2.0)).abs() < 1e-3, "{half:?}");
    // Out of range or not a number: the nearest end, and the step's own pose.
    assert_eq!(w.drawn_ref_body(1, 0, -3.0).unwrap().translation, before.translation);
    assert_eq!(w.drawn_ref_body(1, 0, 7.0).unwrap().translation, after.translation);
    assert_eq!(w.drawn_ref_body(1, 0, f32::NAN).unwrap().translation, after.translation);
    // The velocity and sleep are the body's now.
    assert_eq!((half.linvel, half.sleeping), (after.linvel, after.sleeping));
    assert_eq!(w.drawn_refs(), vec![1]);
    assert_eq!(w.drawn_counts().0, 1);
    // Reading changes nothing: the body is where the step put it.
    assert_eq!(w.ref_bodies(1)[0], after);
}

/// Two worlds stepped alike stay bit-identical whether or not the drawing reads them.
#[test]
fn drawing_leaves_the_steps_bit_identical() {
    let run = |read: bool| {
        let mut w = ground();
        for i in 0..4 {
            w.add_ref(&at(10 + i, [i as f32 * 12.0, 0.0, 40.0 + i as f32 * 15.0]), &[cube()]);
        }
        let mut digests = Vec::new();
        for _ in 0..120 {
            w.step();
            if read {
                for i in 0..4 {
                    std::hint::black_box(w.drawn_ref_bodies(10 + i, 0.3));
                }
                std::hint::black_box(w.drawn_refs());
            }
            digests.push(w.state_digest());
        }
        digests
    };
    assert_eq!(run(true), run(false));
}

/// A body added since the last step is drawn where it is; so is one built again elsewhere
/// (a script's SetPos), however far it went.
#[test]
fn a_new_or_rebuilt_body_is_drawn_where_it_is() {
    let mut w = ground();
    w.add_ref(&at(2, [0.0, 0.0, 300.0]), &[cube()]);
    let now = w.ref_bodies(2)[0];
    assert_eq!(w.drawn_ref_body(2, 0, 0.0).unwrap().translation, now.translation);
    assert!(w.drawn_refs().is_empty());
    for _ in 0..3 {
        w.step();
    }
    assert_eq!(w.drawn_refs(), vec![2]);
    // Built again 500 units away, as the engine does for a script's move.
    w.add_ref(&at(2, [500.0, 0.0, 300.0]), &[cube()]);
    let moved = w.ref_bodies(2)[0];
    assert_eq!(w.drawn_ref_body(2, 0, 0.0).unwrap().translation, moved.translation);
    assert!(w.drawn_refs().is_empty());
    // From the next step on it is drawn between its steps again, from the new place.
    w.step();
    let d = w.drawn_ref_body(2, 0, 0.0).unwrap();
    assert_eq!(d.translation, moved.translation);
}

/// A body asleep is drawn where it is, and one that went further than `DRAWN_JUMP` in one
/// step (a teleport rather than a move) too.
#[test]
fn a_sleeping_body_or_a_jump_is_not_blended() {
    let mut w = ground();
    let r = w.insert_ref(PreparedRef::new(&w.params, &at(3, [0.0, 0.0, 5.0]), &[cube()]).asleep());
    assert_eq!(r.bodies, 1);
    for _ in 0..10 {
        w.step();
    }
    assert!(w.ref_bodies(3)[0].sleeping);
    assert!(w.drawn_refs().is_empty());
    assert_eq!(w.drawn_ref_body(3, 0, 0.0).unwrap().translation, w.ref_bodies(3)[0].translation);
    // Thrown at 120 units a step (7200 units a second): drawn where each step puts it.
    w.add_ref(&at(4, [0.0, 0.0, 500.0]), &[cube()]);
    w.step();
    w.set_ref_velocity(4, 0, 120.0 * 60.0);
    let from = w.ref_bodies(4)[0];
    w.step();
    let to = w.ref_bodies(4)[0];
    assert!(to.translation[0] - from.translation[0] > DRAWN_JUMP, "{from:?} {to:?}");
    assert_eq!(w.drawn_ref_body(4, 0, 0.25).unwrap().translation, to.translation);
}

/// The body the grab holds is drawn between steps; the grip point the game reads stays
/// the stepped one.
#[test]
fn a_held_body_is_drawn_between_steps() {
    let mut w = ground();
    w.add_ref(&at(5, [0.0, 0.0, 5.0]), &[cube()]);
    for _ in 0..60 {
        w.step();
    }
    let off = 6.0 - w.ref_bodies(5)[0].translation[2];
    w.grab(GripOn::Ref(5), [0.0, 0.0, 6.0], HoldParams::default()).expect("held");
    w.set_grip_target([0.0, 0.0, 80.0]);
    for _ in 0..3 {
        w.step();
    }
    let before = w.ref_bodies(5)[0];
    w.step();
    let after = w.ref_bodies(5)[0];
    assert!(after.translation[2] > before.translation[2] + 0.5, "lifted: {before:?} {after:?}");
    assert_eq!(w.drawn_ref_body(5, 0, 0.0).unwrap().translation, before.translation);
    let p = w.grip_point().unwrap();
    assert!((p[2] - (after.translation[2] + off)).abs() < 0.1, "{p:?} {after:?}");
}

/// A body whose velocity is set to NaN is quarantined by rapier: reported once with its
/// owner and counted, frozen where it was (a fixed body, still solid), and the world steps
/// on.
#[test]
fn a_body_gone_nan_is_reported_once_and_frozen_in_place() {
    let mut w = ground();
    w.add_ref(&at(6, [40.0, 0.0, 5.0]), &[cube()]);
    w.add_ref(&at(7, [-40.0, 0.0, 30.0]), &[cube()]);
    for _ in 0..10 {
        w.step();
    }
    let was = w.ref_bodies(6)[0];
    assert_eq!(w.set_ref_velocity(6, 2, f32::NAN), 1);
    for _ in 0..30 {
        w.step();
    }
    let q = w.quarantined();
    assert_eq!(q.len(), 1, "{q:?}");
    assert_eq!(q[0].owner, Some(Owner::Ref { form_id: 6, object: 0, layer: 4 }));
    assert!(!q[0].collider && q[0].remedy == Remedy::Frozen, "{q:?}");
    assert_eq!((w.stats().quarantined_bodies, w.stats().quarantined_colliders), (1, 0));
    let b = w.ref_bodies(6)[0];
    assert_eq!(b.translation, was.translation, "rolled back to where it was");
    assert_eq!(w.ref_mass(6), None, "no longer a moving body");
    assert!(!w.drawn_refs().contains(&6));
    // Still solid: a ray from above meets it, not the ground under it.
    let hit = w.queries().ray([40.0, 0.0, 100.0], [0.0, 0.0, -1.0], 200.0, &Filter::default()).unwrap();
    assert!(matches!(hit.owner, Owner::Ref { form_id: 6, .. }), "{hit:?}");
    // The other cube fell and rests as before.
    assert!(w.ref_bodies(7)[0].translation[2] < 12.0, "{:?}", w.ref_bodies(7));
}

/// Held by the grab when it goes NaN: the grab lets go.
#[test]
fn a_held_body_gone_nan_is_let_go() {
    let mut w = ground();
    w.add_ref(&at(8, [0.0, 0.0, 5.0]), &[cube()]);
    w.step();
    w.grab(GripOn::Ref(8), [0.0, 0.0, 6.0], HoldParams::default()).expect("held");
    w.set_script_velocity(8, 0, Some([f32::NAN; 3]), None);
    w.step();
    assert!(w.holding().is_none());
    assert_eq!(w.quarantined()[0].remedy, Remedy::Frozen);
}
