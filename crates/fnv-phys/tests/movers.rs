//! Keyframed bodies moved with a velocity (`fnv_phys::movers`, physics.md §5.5) on
//! hand-built worlds (no game data): what rests on one rides along, what it meets is pushed
//! out of its way smoothly, characters are not carried, and a jump is put, not swept.

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::*;
use fnv_world::land::SIDE;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn object(shape: Shape, motion: Motion) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion,
        layer: 1,
        mass: 10.0,
        friction: 0.5,
        restitution: 0.0,
        shape: shape.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

fn boxed(h: [f32; 3]) -> Shape {
    Shape::Box { half_extents: h }
}

fn at(form_id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id, position, rotation: ID, scale: 1.0 }
}

/// Flat ground at z = 0 around the origin, steps of `dt` seconds; movers swept or put.
fn ground(dt: f32, swept: bool) -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams { dt, ..PhysParams::default() });
    w.set_swept_movers(swept);
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    w
}

const PLATFORM: u32 = 1;
const LOOSE: u32 = 2;

/// A keyframed platform 200 × 200 with its top at z 55, and a loose box resting on it.
fn platform_with_a_box(dt: f32, swept: bool) -> PhysWorld {
    let mut w = ground(dt, swept);
    w.add_ref(&at(PLATFORM, [0.0, 0.0, 50.0]), &[object(boxed([100.0, 100.0, 5.0]), Motion::Keyframed)]);
    w.add_ref(&at(LOOSE, [0.0, 0.0, 61.0]), &[object(boxed([5.0, 5.0, 5.0]), Motion::Dynamic)]);
    for _ in 0..(1.0 / dt) as usize {
        w.step();
    }
    w
}

/// Pose `id`'s keyframed object 0 offset by `x` along x from where it was placed.
fn offset(w: &mut PhysWorld, id: u32, place: [f32; 3], x: f32, sweep: bool) {
    let node = Iso { translation: [x, 0.0, 0.0], rotation: ID };
    assert!(w.pose_ref_object(&at(id, place), 0, &node, &Iso::IDENTITY, sweep));
}

/// Move the platform `speed` units/s along x for `secs`, in frames of 1/60 s of
/// `per_frame` steps each; the loose box's x after.
fn ride(w: &mut PhysWorld, speed: f32, secs: f32, per_frame: u32) -> f32 {
    let frames = (secs * 60.0).round() as usize;
    for i in 1..=frames {
        offset(w, PLATFORM, [0.0, 0.0, 50.0], speed * i as f32 / 60.0, true);
        w.spread_moves(per_frame);
        for _ in 0..per_frame {
            w.step();
        }
    }
    w.ref_bodies(LOOSE)[0].translation[0]
}

#[test]
fn a_box_rides_a_swept_platform_and_stays_behind_a_put_one() {
    let mut swept = platform_with_a_box(1.0 / 60.0, true);
    let x = ride(&mut swept, 60.0, 2.0, 1);
    let platform = swept.ref_bodies(PLATFORM)[0].translation[0];
    assert!((platform - 120.0).abs() < 0.01, "the platform went its way: {platform}");
    assert!(x > 105.0, "the box rode along: {x:.1} of 120");
    let mut put = platform_with_a_box(1.0 / 60.0, false);
    let x = ride(&mut put, 60.0, 2.0, 1);
    assert!(x.abs() < 10.0, "put: the platform slid out under the box: {x:.1}");
}

#[test]
fn the_ride_is_the_same_at_any_physics_rate() {
    let mut a = platform_with_a_box(1.0 / 60.0, true);
    let mut b = platform_with_a_box(1.0 / 120.0, true);
    let mut c = platform_with_a_box(1.0 / 240.0, true);
    let (xa, xb, xc) = (ride(&mut a, 60.0, 2.0, 1), ride(&mut b, 60.0, 2.0, 2), ride(&mut c, 60.0, 2.0, 4));
    assert!((xa - xb).abs() < 5.0 && (xa - xc).abs() < 5.0, "60 Hz {xa:.1}, 120 Hz {xb:.1}, 240 Hz {xc:.1}");
}

#[test]
fn a_swept_wall_pushes_a_box_out_of_its_way_smoothly() {
    let dt = 1.0 / 60.0;
    let mut w = ground(dt, true);
    let place = [-60.0, 0.0, 50.0];
    w.add_ref(&at(PLATFORM, place), &[object(boxed([5.0, 100.0, 50.0]), Motion::Keyframed)]);
    w.add_ref(&at(LOOSE, [0.0, 0.0, 11.0]), &[object(boxed([10.0, 10.0, 10.0]), Motion::Dynamic)]);
    for _ in 0..60 {
        w.step();
    }
    let speed = 150.0;
    let mut fastest = 0.0f32;
    for i in 1..=60 {
        offset(&mut w, PLATFORM, place, speed * i as f32 / 60.0, true);
        w.spread_moves(1);
        w.step();
        let wall = w.ref_bodies(PLATFORM)[0].translation[0];
        let b = w.ref_bodies(LOOSE)[0];
        // Never through the wall: its front face is 5 ahead of its centre, the box's back
        // 10 behind its own; the step that first meets it may reach into the box by the
        // wall's move in a step (2.5 units) before the contact pushes it out.
        assert!(b.translation[0] - wall >= 15.0 - 4.0, "step {i}: the box at {:.1} is inside the wall at {wall:.1}", b.translation[0]);
        fastest = fastest.max(b.linvel[0]);
    }
    assert!(fastest <= speed * 1.5, "flung: {fastest:.0} units/s from a wall at {speed}");
    let b = w.ref_bodies(LOOSE)[0].translation[0];
    assert!(b > 90.0 + 15.0 - 1.5, "pushed ahead of the wall: {b:.1}");
}

#[test]
fn a_pose_is_reached_over_the_frames_steps_and_the_mover_rests_between_steps() {
    let mut w = ground(1.0 / 60.0, true);
    let place = [0.0, 0.0, 50.0];
    w.add_ref(&at(PLATFORM, place), &[object(boxed([100.0, 100.0, 5.0]), Motion::Keyframed)]);
    w.step();
    offset(&mut w, PLATFORM, place, 40.0, true);
    // Swept: queries still see it where it was until the steps move it.
    assert_eq!(w.ref_bodies(PLATFORM)[0].translation[0], 0.0);
    // A frame with no step keeps it for the next.
    w.spread_moves(0);
    w.spread_moves(4);
    for i in 1..=4 {
        w.step();
        let b = w.ref_bodies(PLATFORM)[0];
        assert!((b.translation[0] - 10.0 * i as f32).abs() < 0.01, "step {i}: {:?}", b.translation);
        assert_eq!(b.linvel, [0.0; 3], "at rest between steps (what the character controller reads)");
    }
    // Arrived: it stays.
    w.step();
    assert!((w.ref_bodies(PLATFORM)[0].translation[0] - 40.0).abs() < 0.01);
    let m = w.mover_stats();
    assert_eq!((m.swept, m.put, m.body_steps), (1, 0, 4));
    assert!((m.fastest - 600.0).abs() < 1.0, "10 units a step at 60 steps: {}", m.fastest);
}

#[test]
fn a_jump_is_put_at_once_not_swept() {
    let mut w = ground(1.0 / 60.0, true);
    let place = [0.0, 0.0, 50.0];
    w.add_ref(&at(PLATFORM, place), &[object(boxed([100.0, 100.0, 5.0]), Motion::Keyframed)]);
    // Not a continuation (the first pose, another sequence): put.
    offset(&mut w, PLATFORM, place, 30.0, false);
    assert_eq!(w.ref_bodies(PLATFORM)[0].translation[0], 30.0);
    // A continuation too far for one frame: put as well.
    offset(&mut w, PLATFORM, place, 30.0 + SWEEP_MOST + 1.0, true);
    assert!((w.ref_bodies(PLATFORM)[0].translation[0] - (31.0 + SWEEP_MOST)).abs() < 0.01);
    // A pending sweep overtaken by a put does not move it on afterwards.
    offset(&mut w, PLATFORM, place, 300.0, true);
    offset(&mut w, PLATFORM, place, 0.0, false);
    w.step();
    assert_eq!(w.ref_bodies(PLATFORM)[0].translation[0], 0.0);
    assert_eq!(w.mover_stats().put, 3);
}

#[test]
fn a_character_on_a_swept_platform_is_not_carried() {
    let mut w = ground(1.0 / 60.0, true);
    let place = [0.0, 0.0, 50.0];
    w.add_ref(&at(PLATFORM, place), &[object(boxed([100.0, 100.0, 5.0]), Motion::Keyframed)]);
    let mut c = w.add_character(9, CharacterParams::default(), [0.0, 0.0, 56.0]);
    for _ in 0..30 {
        w.move_character(&mut c, CharacterInput::default());
        w.step();
    }
    let start = c.feet;
    for i in 1..=30 {
        offset(&mut w, PLATFORM, place, 60.0 * i as f32 / 60.0, true);
        w.spread_moves(1);
        w.move_character(&mut c, CharacterInput::default());
        w.step();
    }
    assert!((c.feet[0] - start[0]).abs() < 2.0, "not carried, as with a put platform: {:?} from {start:?}", c.feet);
    assert!((c.feet[2] - start[2]).abs() < 2.0, "still standing on it: {:?}", c.feet);
}

#[test]
fn a_mover_tells_what_it_carried_or_pushed_once() {
    let mut w = platform_with_a_box(1.0 / 60.0, true);
    assert!(w.take_mover_contacts().is_empty(), "nothing while it stands");
    ride(&mut w, 60.0, 0.5, 1);
    let told = w.take_mover_contacts();
    assert_eq!(told, [MoverContact { mover: PLATFORM, other: LOOSE, actor: false }]);
    ride(&mut w, 60.0, 0.5, 1);
    assert!(w.take_mover_contacts().is_empty(), "told once");
    assert_eq!(w.mover_stats().touched, 1);
}

#[test]
fn movers_are_fixed_bodies_between_steps() {
    // Swept or put, a keyframed body is a fixed body whenever the characters and queries
    // look (`fnv_phys::movers`): kinematic only inside a step that moves it.
    for swept in [true, false] {
        let mut w = ground(1.0 / 60.0, swept);
        let place = [0.0, 0.0, 50.0];
        w.add_ref(&at(PLATFORM, place), &[object(boxed([100.0, 100.0, 5.0]), Motion::Keyframed)]);
        assert_eq!(w.stats().bodies_kinematic, 0);
        offset(&mut w, PLATFORM, place, 20.0, true);
        w.spread_moves(2);
        w.step();
        assert_eq!(w.stats().bodies_kinematic, 0, "swept {swept}: fixed again after the step");
        let (lo, hi) = w.fixed_bounds().expect("bounds");
        assert!(lo[2] <= 45.0 && hi[2] >= 55.0, "{lo:?} {hi:?}");
    }
}
