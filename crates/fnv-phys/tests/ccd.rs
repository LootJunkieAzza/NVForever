//! Continuous collision of ragdoll bones and gore pieces (#1798, `fnv_phys::ccd`): at every
//! CCD choice and at 60 and 152 steps a second, fast bones, severed heads and gore pieces
//! stop at thin floors, thin walls, triangle-mesh floors (no thickness) and terrain, and
//! corpses come to rest on stairs and slopes at the same height whatever the choice.
//! Hand-built worlds, no game data.

use fnv_collide::ragdoll::{JointKind, Ragdoll, RagdollBody, RagdollJoint};
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::gore::GoreStart;
use fnv_phys::*;
use fnv_world::land::SIDE;
use std::sync::Arc;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
const X: [f32; 3] = [1.0, 0.0, 0.0];
const Z: [f32; 3] = [0.0, 0.0, 1.0];

/// Every choice, the same for bones and gore.
const CHOICES: [BodyCcd; 3] = [BodyCcd::Plain, BodyCcd::Soft(fnv_phys::ccd::SOFT_UNITS), BodyCcd::Bullet];
/// The rates: the default and the owner's display rate.
const RATES: [f32; 2] = [60.0, 152.0];

fn world(ccd: BodyCcd, hz: f32) -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    w.set_step(1.0 / hz);
    w.set_ccd(CcdChoice { bones: ccd, gore: ccd });
    w
}

fn steps(w: &mut PhysWorld, hz: f32, seconds: f32) {
    for _ in 0..(seconds * hz).round() as usize {
        w.step();
    }
}

fn object(shape: Shape) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion: Motion::Static,
        layer: layers::STATIC,
        mass: 0.0,
        friction: 0.5,
        restitution: 0.0,
        shape: shape.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

fn place(w: &mut PhysWorld, form_id: u32, position: [f32; 3], rotation: [f32; 9], shape: Shape) {
    let r = w.add_ref(&RefPose { form_id, position, rotation, scale: 1.0 }, &[object(shape)]);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

/// A static box: `half` its half extents, centred at `at`.
fn slab(w: &mut PhysWorld, form_id: u32, at: [f32; 3], half: [f32; 3]) {
    place(w, form_id, at, ID, Shape::Box { half_extents: half });
}

/// A square of two triangles (no thickness) at height `z`, `half` units each way from the
/// origin, facing up.
fn sheet(w: &mut PhysWorld, form_id: u32, z: f32, half: f32) {
    let mesh = Shape::TriMesh {
        vertices: vec![[-half, -half, z], [half, -half, z], [half, half, z], [-half, half, z]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        materials: vec![],
    };
    place(w, form_id, [0.0; 3], ID, mesh);
}

fn body(name: &str, shape: Shape, mass: f32, rest: [f32; 3]) -> RagdollBody {
    let at = Iso { translation: rest, rotation: ID };
    RagdollBody {
        object: CollisionObject { node: Some(name.into()), node_transform: at, motion: Motion::Keyframed, layer: layers::BIPED, mass, friction: 0.3, ..object(shape) },
        linear_damping: 0.1,
        angular_damping: 0.05,
        stored: at,
        unit: 7.0,
    }
}

/// A small standing figure (as `tests/ragdoll.rs`'s): pelvis, chest, head, thighs, calves.
fn figure() -> Arc<Ragdoll> {
    let cap = |a: [f32; 3], b: [f32; 3], radius: f32| Shape::Capsule { a, b, radius };
    let ball = |cone: f32, twist: f32| JointKind::Ragdoll { twist_a: Z, plane_a: X, twist_b: Z, plane_b: X, cone_max: cone, plane_min: -cone, plane_max: cone, twist_min: -twist, twist_max: twist };
    let knee = JointKind::Hinge { axis_a: X, perp_a: [0.0, 1.0, 0.0], axis_b: X, perp_b: [0.0, 1.0, 0.0], limits: Some([-0.1, 2.2]) };
    let joint = |a: usize, b: usize, pivot_a: [f32; 3], pivot_b: [f32; 3], kind: JointKind| RagdollJoint { block: 0, a, b, pivot_a, pivot_b, kind, max_friction: 100.0, strength: None };
    Arc::new(Ragdoll {
        bodies: vec![
            body("pelvis", cap([-6.0, 0.0, 0.0], [6.0, 0.0, 0.0], 7.0), 12.0, [0.0, 0.0, 66.0]),
            body("chest", cap([0.0, 0.0, 6.0], [0.0, 0.0, 26.0], 9.0), 20.0, [0.0, 0.0, 74.0]),
            body("head", Shape::Sphere { radius: 8.0 }, 4.0, [0.0, 0.0, 120.0]),
            body("l thigh", cap([0.0, 0.0, -4.0], [0.0, 0.0, -28.0], 5.0), 7.0, [-8.0, 0.0, 62.0]),
            body("l calf", cap([0.0, 0.0, -3.0], [0.0, 0.0, -24.0], 4.0), 4.0, [-8.0, 0.0, 32.0]),
            body("r thigh", cap([0.0, 0.0, -4.0], [0.0, 0.0, -28.0], 5.0), 7.0, [8.0, 0.0, 62.0]),
            body("r calf", cap([0.0, 0.0, -3.0], [0.0, 0.0, -24.0], 4.0), 4.0, [8.0, 0.0, 32.0]),
        ],
        joints: vec![
            joint(1, 0, [0.0; 3], [0.0, 0.0, 8.0], ball(0.4, 0.2)),
            joint(2, 1, [0.0, 0.0, -9.0], [0.0, 0.0, 37.0], ball(0.5, 0.6)),
            joint(3, 0, [0.0; 3], [-8.0, 0.0, -4.0], ball(0.9, 0.2)),
            joint(4, 3, [0.0; 3], [0.0, 0.0, -30.0], knee),
            joint(5, 0, [0.0; 3], [8.0, 0.0, -4.0], ball(0.9, 0.2)),
            joint(6, 5, [0.0; 3], [0.0, 0.0, -30.0], knee),
        ],
        problems: vec![],
    })
}

/// A severed head: one sphere of radius 8, alone.
fn head() -> Arc<Ragdoll> {
    Arc::new(Ragdoll { bodies: vec![body("head", Shape::Sphere { radius: 8.0 }, 4.0, [0.0; 3])], joints: vec![], problems: vec![] })
}

/// Add ragdoll `rag` as actor `id`, its bind pose moved to `origin` (tipped over onto its
/// back when `lying`), every body at `velocity`.
fn throw(w: &mut PhysWorld, id: u32, rag: &Arc<Ragdoll>, origin: [f32; 3], lying: bool, velocity: [f32; 3]) {
    let turn = if lying { [1.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 1.0, 0.0] } else { ID };
    let at = Iso { translation: origin, rotation: turn };
    let nodes: Vec<Iso> = rag.bodies.iter().map(|b| at.compose(&b.rest())).collect();
    let r = w.add_ragdoll(&RagdollStart { form_id: id, ragdoll: rag, scale: 1.0, nodes: &nodes, velocity, parts: &[], asleep: false });
    assert_eq!(r.bodies, rag.bodies.len(), "{:?}", r.errors);
}

/// A gore piece (2 × 1.5 × 1 half extents, the smallest the game makes are about this).
fn gore(w: &mut PhysWorld, id: u32, at: [f32; 3], linvel: [f32; 3]) {
    w.add_gore(&GoreStart { id, actor: 0x99, translation: at, rotation: ID, half_extents: [2.0, 1.5, 1.0], center: [0.0; 3], linvel, angvel: [9.0, 4.0, 2.0], mass: 0.5 });
}

fn bone_points(w: &PhysWorld, id: u32) -> Vec<[f32; 3]> {
    w.ragdoll_nodes(id).unwrap().into_iter().flatten().map(|b| b.translation).collect()
}

/// Ragdolls, severed heads and gore pieces dropped at `speed` onto a floor whose top is at
/// z = 0, from 400 units up: the lowest point any of them reaches after `seconds`, their
/// bone or piece centres (a bone centre sits at least its radius above the floor).
fn lowest_after_drop(w: &mut PhysWorld, hz: f32, speed: f32) -> f32 {
    let down = [0.0, 0.0, -speed];
    throw(w, 1, &figure(), [0.0, 0.0, 400.0], true, down);
    throw(w, 2, &head(), [120.0, 0.0, 400.0], false, down);
    for i in 0..6u32 {
        gore(w, 10 + i, [-120.0 + i as f32 * 8.0, 60.0, 400.0], [i as f32 * 20.0, 0.0, -speed]);
    }
    let mut lowest = f32::INFINITY;
    for _ in 0..(1.5 * hz) as usize {
        w.step();
        for id in [1, 2] {
            lowest = bone_points(w, id).into_iter().map(|p| p[2]).fold(lowest, f32::min);
        }
        for i in 0..6u32 {
            lowest = lowest.min(w.gore_state(10 + i).unwrap().translation[2]);
        }
    }
    lowest
}

/// A 2-unit-thick floor over nothing.
#[test]
fn fast_bones_heads_and_gore_stop_on_a_thin_floor() {
    for ccd in CHOICES {
        for hz in RATES {
            for speed in [300.0, 1200.0, 3000.0] {
                let mut w = world(ccd, hz);
                slab(&mut w, 0x500, [0.0, 0.0, -1.0], [400.0, 400.0, 1.0]);
                let low = lowest_after_drop(&mut w, hz, speed);
                assert!(low > -0.5, "{ccd} at {hz} Hz, {speed} units/s: a body reached z {low} under a floor at 0");
            }
        }
    }
}

/// Triangles with no thickness: an interior floor.
#[test]
fn fast_bones_heads_and_gore_stop_on_a_triangle_floor() {
    for ccd in CHOICES {
        for hz in RATES {
            for speed in [300.0, 1200.0, 3000.0] {
                let mut w = world(ccd, hz);
                sheet(&mut w, 0x500, 0.0, 400.0);
                let low = lowest_after_drop(&mut w, hz, speed);
                assert!(low > -0.5, "{ccd} at {hz} Hz, {speed} units/s: a body reached z {low} under a mesh floor at 0");
            }
        }
    }
}

#[test]
fn fast_bones_heads_and_gore_stop_on_terrain() {
    for ccd in CHOICES {
        for hz in RATES {
            let mut w = world(ccd, hz);
            for x in -1..=0 {
                for y in -1..=0 {
                    w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
                }
            }
            let low = lowest_after_drop(&mut w, hz, 3000.0);
            assert!(low > -0.5, "{ccd} at {hz} Hz: a body reached z {low} under the terrain at 0");
        }
    }
}

/// A severed head, a whole corpse and gore pieces thrown level at a 2-unit-thick wall
/// (x 299 to 301) stay on the near side; the floor under them is thick.
#[test]
fn a_head_corpse_and_gore_thrown_at_a_thin_wall_stay_in() {
    for ccd in CHOICES {
        for hz in RATES {
            for speed in [600.0, 1500.0, 3000.0] {
                let mut w = world(ccd, hz);
                slab(&mut w, 0x500, [0.0, 0.0, -50.0], [800.0, 800.0, 50.0]);
                slab(&mut w, 0x501, [300.0, 0.0, 200.0], [1.0, 400.0, 200.0]);
                let at = [speed, 0.0, 0.0];
                throw(&mut w, 1, &head(), [0.0, 0.0, 60.0], false, at);
                throw(&mut w, 2, &figure(), [0.0, 100.0, 30.0], true, at);
                throw(&mut w, 3, &head(), [200.0, -100.0, 40.0], false, [speed, 0.0, speed * 0.3]);
                for i in 0..6u32 {
                    gore(&mut w, 10 + i, [0.0, -60.0 + i as f32 * 8.0, 30.0 + i as f32 * 5.0], [speed, i as f32 * 15.0, 100.0]);
                }
                let mut farthest = f32::NEG_INFINITY;
                for _ in 0..(1.5 * hz) as usize {
                    w.step();
                    for id in [1, 2, 3] {
                        farthest = bone_points(&w, id).into_iter().map(|p| p[0]).fold(farthest, f32::max);
                    }
                    for i in 0..6u32 {
                        farthest = farthest.max(w.gore_state(10 + i).unwrap().translation[0]);
                    }
                }
                assert!(farthest < 299.5, "{ccd} at {hz} Hz, {speed} units/s: a body reached x {farthest} past a wall at 299");
            }
        }
    }
}

/// The slope of the ramp in [`stairs_or_ramp`]: steep enough to slide on with less friction,
/// not with the bones' 0.3 and the ramp's 0.5.
const RAMP_DEGREES: f32 = 15.0;

/// Stairs of 16-unit steps going up along +x (static boxes, x -162 to 126, with flat ground
/// beyond the top step at its height and below the bottom one at 0), or a ramp of
/// [`RAMP_DEGREES`] going down along +x. Returns the height of the surface under a point.
fn stairs_or_ramp(w: &mut PhysWorld, ramp: bool) -> impl Fn([f32; 3]) -> f32 {
    let (s, c) = RAMP_DEGREES.to_radians().sin_cos();
    if ramp {
        place(w, 0x600, [0.0; 3], [c, 0.0, s, 0.0, 1.0, 0.0, -s, 0.0, c], Shape::Box { half_extents: [400.0, 200.0, 10.0] });
    } else {
        for i in 0..12 {
            let top = 16.0 * (i + 1) as f32;
            slab(w, 0x600 + i, [i as f32 * 24.0 - 150.0, 0.0, top / 2.0], [12.0, 200.0, top / 2.0]);
        }
        slab(w, 0x700, [326.0, 0.0, 96.0], [200.0, 200.0, 96.0]);
        slab(w, 0x701, [-362.0, 0.0, -10.0], [200.0, 200.0, 10.0]);
    }
    move |p: [f32; 3]| {
        if ramp {
            // The top face: s·x + c·z = 10.
            (10.0 - s * p[0]) / c
        } else {
            16.0 * (((p[0] + 162.0) / 24.0).floor() + 1.0).clamp(0.0, 12.0)
        }
    }
}

/// A corpse laid on a step of the stairs, on the ramp and on flat ground: where it is after
/// `settle` seconds and `more` seconds later (bone centres), the surface under each bone.
fn lay(ccd: BodyCcd, hz: f32, ground: u8, settle: f32, more: f32) -> (Vec<[f32; 3]>, Vec<[f32; 3]>, Vec<f32>) {
    let mut w = world(ccd, hz);
    let surface: Box<dyn Fn([f32; 3]) -> f32> = match ground {
        0 => Box::new(stairs_or_ramp(&mut w, false)),
        1 => Box::new(stairs_or_ramp(&mut w, true)),
        _ => {
            slab(&mut w, 0x600, [0.0, 0.0, -10.0], [400.0, 400.0, 10.0]);
            Box::new(|_| 0.0)
        }
    };
    // Lying along -y (across the steps), a little above the surface under its middle.
    let at = [6.0, 0.0, surface([6.0, -60.0, 0.0]) + 12.0];
    throw(&mut w, 1, &figure(), at, true, [0.0; 3]);
    steps(&mut w, hz, settle);
    let first = bone_points(&w, 1);
    steps(&mut w, hz, more);
    let then = bone_points(&w, 1);
    let under = then.iter().map(|p| surface(*p)).collect();
    (first, then, under)
}

const GROUNDS: [&str; 3] = ["stairs", "ramp", "flat"];

/// A corpse laid on stairs, on a 15° ramp and on flat ground comes to rest on the surface
/// and stays put at every choice and rate: no bone sinks into a step or the ramp, and it
/// moves less than 2 units in the 4 seconds after it has had 6 to settle.
#[test]
fn corpses_stay_put_on_stairs_and_slopes() {
    for ccd in CHOICES {
        for hz in RATES {
            for (g, name) in GROUNDS.iter().enumerate() {
                let (first, then, under) = lay(ccd, hz, g as u8, 6.0, 4.0);
                for (p, s) in then.iter().zip(&under) {
                    // A bone centre sits at least the thinnest bone's radius (4) above what it
                    // rests on; one overhanging a step's edge can be lower than that step.
                    let floor = if g == 0 { s - 16.5 } else { s + 3.0 };
                    assert!(p[2] > floor, "{ccd} at {hz} Hz on the {name}: bone at {p:?}, surface {s}");
                }
                let moved = first.iter().zip(&then).map(|(a, b)| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()).fold(0.0, f32::max);
                assert!(moved < 2.0, "{ccd} at {hz} Hz on the {name}: a bone moved {moved} after settling");
            }
        }
    }
}

/// Soft CCD's early contacts do not hold a corpse off the ground: at rest on flat ground and
/// on the ramp its bones lie as high as with plain CCD, within half a unit.
#[test]
fn soft_ccd_does_not_hold_a_corpse_off_the_ground() {
    let mean_z = |pts: &[[f32; 3]]| pts.iter().map(|p| p[2]).sum::<f32>() / pts.len() as f32;
    for hz in RATES {
        for g in [1u8, 2] {
            let plain = mean_z(&lay(BodyCcd::Plain, hz, g, 6.0, 0.0).0);
            for ccd in [BodyCcd::Soft(fnv_phys::ccd::SOFT_UNITS), BodyCcd::Bullet] {
                let other = mean_z(&lay(ccd, hz, g, 6.0, 0.0).0);
                assert!((other - plain).abs() < 0.5, "{ccd} at {hz} Hz on the {}: mean bone z {other}, plain {plain}", GROUNDS[g as usize]);
            }
        }
    }
}

/// What [`battle_scene`] cost: rapier's stage times per step (milliseconds; the solve, the
/// collision detection, the CCD sweeps, the whole step) and the fast bodies CCD swept per
/// step. With the `solver-timers` feature only (0 otherwise).
#[derive(Debug, Clone, Copy, Default)]
struct Cost {
    solve: f64,
    detect: f64,
    ccd: f64,
    step: f64,
    swept: f64,
}

/// A battle's physics, headless: terrain, `statics` static boxes about (rocks, fences, walls),
/// 100 characters walking in circles, 30 crates of clutter, and 8 corpses, 4 severed heads
/// and 24 gore pieces thrown at 150 to 1,500 units a second and thrown again every half
/// second (the moving ragdolls of battle-50's peak). `seconds` of game time at `hz`.
fn battle_scene(ccd: CcdChoice, hz: f32, statics: u32, seconds: f32) -> Cost {
    let mut w = PhysWorld::new(PhysParams::default());
    w.set_step(1.0 / hz);
    w.set_ccd(ccd);
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    // A fixed pseudo-random sequence, the same for every choice.
    let mut seed = 0x2545_f491_u32;
    let mut rnd = move || {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        seed as f32 / u32::MAX as f32
    };
    for i in 0..statics {
        let at = [rnd() * 3600.0 - 1800.0, rnd() * 3600.0 - 1800.0, 0.0];
        let half = [4.0 + rnd() * 60.0, 1.0 + rnd() * 30.0, 10.0 + rnd() * 60.0];
        slab(&mut w, 0x1000 + i, [at[0], at[1], half[2]], half);
    }
    for i in 0..30u32 {
        let o = CollisionObject { motion: Motion::Dynamic, layer: layers::CLUTTER, mass: 10.0, ..object(Shape::Box { half_extents: [8.0, 8.0, 8.0] }) };
        w.add_ref(&RefPose { form_id: 0x3000 + i, position: [rnd() * 800.0 - 400.0, rnd() * 800.0 - 400.0, 9.0], rotation: ID, scale: 1.0 }, &[o]);
    }
    let mut people: Vec<Character> = (0..100u32).map(|i| w.add_character(0x4000 + i, CharacterParams::default(), [rnd() * 1600.0 - 800.0, rnd() * 1600.0 - 800.0, 4.0])).collect();
    let (fig, hd) = (figure(), head());
    let throw_speed = |r: f32| 150.0 + r * 1350.0;
    for i in 0..8u32 {
        let (a, v) = (rnd() * std::f32::consts::TAU, throw_speed(rnd()));
        throw(&mut w, 0x100 + i, &fig, [rnd() * 600.0 - 300.0, rnd() * 600.0 - 300.0, 40.0], true, [a.cos() * v, a.sin() * v, 200.0]);
    }
    for i in 0..4u32 {
        let (a, v) = (rnd() * std::f32::consts::TAU, throw_speed(rnd()));
        throw(&mut w, 0x200 + i, &hd, [rnd() * 600.0 - 300.0, rnd() * 600.0 - 300.0, 60.0], false, [a.cos() * v, a.sin() * v, 300.0]);
    }
    for i in 0..24u32 {
        let a = rnd() * std::f32::consts::TAU;
        let v = 150.0 + rnd() * 200.0;
        gore(&mut w, 0x300 + i, [rnd() * 600.0 - 300.0, rnd() * 600.0 - 300.0, 50.0], [a.cos() * v, a.sin() * v, 250.0]);
    }
    let before = w.stats();
    let n = (seconds * hz).round() as usize;
    let rethrow = (hz / 2.0).round() as usize;
    for s in 0..n {
        for (i, c) in people.iter_mut().enumerate() {
            let a = s as f32 / hz + i as f32;
            w.move_character(c, CharacterInput { velocity: [a.cos() * 150.0, a.sin() * 150.0], ..Default::default() });
        }
        if s % rethrow == rethrow - 1 {
            for i in 0..8u32 {
                let (a, v) = (rnd() * std::f32::consts::TAU, throw_speed(rnd()));
                let at = w.ragdoll_center(0x100 + i).unwrap();
                w.push_ragdoll(0x100 + i, at, [a.cos() * v, a.sin() * v, 250.0]);
            }
            for i in 0..4u32 {
                let (a, v) = (rnd() * std::f32::consts::TAU, throw_speed(rnd()));
                let at = w.ragdoll_center(0x200 + i).unwrap();
                w.push_ragdoll(0x200 + i, at, [a.cos() * v, a.sin() * v, 250.0]);
            }
            for i in 0..24u32 {
                let a = rnd() * std::f32::consts::TAU;
                w.push_gore(0x300 + i, [a.cos() * 300.0, a.sin() * 300.0, 200.0]);
            }
        }
        w.step();
    }
    let after = w.stats();
    let per = |i: usize| (after.solver_us[i] - before.solver_us[i]) as f64 / 1000.0 / n as f64;
    Cost { solve: per(1), detect: per(2), ccd: per(4), step: per(5), swept: (after.ccd_bodies - before.ccd_bodies) as f64 / n as f64 }
}

/// The cost of each choice in [`battle_scene`], at 60 and 152 Hz, with few static colliders
/// (rapier sweeps non-bullets against a list of them) and many (the whole broad phase, as in
/// a loaded exterior). A measurement, not a check (about 17 minutes): run with
/// `cargo test -p fnv-phys --release --features solver-timers --test ccd -- --ignored --nocapture`.
#[test]
#[ignore = "measurement; needs --features solver-timers"]
fn ccd_cost_in_a_battle() {
    let runs = 3;
    for statics in [200, 2000] {
        for hz in RATES {
            for ccd in CHOICES {
                let choice = CcdChoice { bones: ccd, gore: ccd };
                // The least of a few runs: the same work each time, less of the machine's noise.
                let costs: Vec<Cost> = (0..runs).map(|_| battle_scene(choice, hz, statics, 6.0)).collect();
                let least = |f: fn(&Cost) -> f64| costs.iter().map(f).fold(f64::INFINITY, f64::min);
                let (ccd_ms, step_ms) = (least(|c| c.ccd), least(|c| c.step));
                eprintln!(
                    "statics {statics:4}, {hz:3} Hz, {:<8}: per step ccd {ccd_ms:.3} ms, solve {:.3}, detect {:.3}, step {step_ms:.3} ms; bodies swept {:.1}; per real second ccd {:.1} ms, step {:.1} ms",
                    ccd.to_string(),
                    least(|c| c.solve),
                    least(|c| c.detect),
                    costs[0].swept,
                    ccd_ms * hz as f64,
                    step_ms * hz as f64,
                );
            }
        }
    }
}
