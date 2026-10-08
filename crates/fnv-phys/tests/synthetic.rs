//! fnv-phys on hand-built worlds: terrain from synthetic LAND heights and references
//! with hand-built `fnv_collide` shapes (no game data).

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::terrain::height_at;
use fnv_phys::*;
use fnv_phys::layers;
use fnv_world::land::SIDE;
use fnv_world::CELL_SIZE;
use proptest::prelude::*;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

fn rz(a: f32) -> [f32; 9] {
    let (s, c) = a.sin_cos();
    [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
}

fn flat(z: f32) -> Vec<f32> {
    vec![z; SIDE * SIDE]
}

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
        shape,
        shell: 0.0,
    }
}

fn at(form_id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id, position, rotation: ID, scale: 1.0 }
}

fn boxed(h: [f32; 3]) -> Shape {
    Shape::Box { half_extents: h }
}

/// Flat ground at z = 0 over the four cells around the origin.
fn ground() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &flat(0.0));
        }
    }
    w
}

/// Rays start close to the geometry: f32 ray–sphere tests from thousands of units away
/// lose about a unit of precision (physics.md §5).
fn down(w: &mut PhysWorld, x: f32, y: f32) -> Option<RayHit> {
    w.queries().ray([x, y, 1_000.0], [0.0, 0.0, -1.0], 2_000.0, &Filter::default())
}

#[test]
fn terrain_seams_and_owners() {
    let mut w = PhysWorld::new(PhysParams::default());
    let slope: Vec<f32> = (0..SIDE * SIDE).map(|k| (k % SIDE) as f32 * 4.0).collect();
    w.add_terrain((0, 0), &slope);
    w.add_terrain((1, 0), &slope.iter().map(|h| h + 128.0).collect::<Vec<_>>());
    // Exactly on the shared edge, and just either side of it.
    for (x, want_cell) in [(CELL_SIZE - 0.5, 0), (CELL_SIZE + 0.5, 1)] {
        let hit = down(&mut w, x, 1000.0).expect("hit");
        assert_eq!(hit.owner, Owner::Terrain { x: want_cell, y: 0 });
    }
    let edge = down(&mut w, CELL_SIZE, 1000.0).expect("rays on a cell edge hit");
    assert!((edge.point[2] - 128.0).abs() < 0.01);
    let local = height_at(&slope, 333.0, 1000.0).unwrap();
    let hit = down(&mut w, 333.0, 1000.0).unwrap();
    assert!((hit.point[2] - local).abs() < 0.01);
    assert!(hit.normal[2] > 0.99);
    assert!(w.remove_terrain((0, 0)));
    assert!(down(&mut w, 333.0, 1000.0).is_none());
    assert_eq!(w.stats().terrain_cells, 1);
}

#[test]
fn reference_transform_rotation_and_scale() {
    let mut w = ground();
    // A 10 × 2 × 2 box offset 20 along the node's x, the reference turned 90° about z
    // and scaled ×2: the box ends up along world y, centred 40 north of the reference.
    let mut o = object(boxed([10.0, 2.0, 2.0]), Motion::Static);
    o.node_transform = Iso { translation: [20.0, 0.0, 0.0], rotation: ID };
    let pose = RefPose { form_id: 0x1234, position: [100.0, 100.0, 50.0], rotation: rz(std::f32::consts::FRAC_PI_2), scale: 2.0 };
    let r = w.add_ref(&pose, &[o]);
    assert_eq!((r.bodies, r.colliders, r.errors.len()), (1, 1, 0));
    let hit = down(&mut w, 100.0, 140.0 + 19.0).expect("hits the box");
    assert_eq!(hit.owner, Owner::Ref { form_id: 0x1234, object: 0, layer: 1 });
    assert!((hit.point[2] - 54.0).abs() < 1e-3, "{hit:?}");
    // Width along world x is 2 × 2 × 2 = 8: 3 units off-axis hits, 5 misses.
    assert!(matches!(down(&mut w, 103.0, 140.0).unwrap().owner, Owner::Ref { .. }));
    assert!(matches!(down(&mut w, 105.0, 140.0).unwrap().owner, Owner::Terrain { .. }));
    assert!(w.remove_ref(0x1234));
    assert!(matches!(down(&mut w, 100.0, 140.0).unwrap().owner, Owner::Terrain { .. }));
}

#[test]
fn body_transform_composes_after_node() {
    let mut w = ground();
    let mut o = object(Shape::Sphere { radius: 5.0 }, Motion::Static);
    o.node_transform = Iso { translation: [0.0, 0.0, 30.0], rotation: rz(std::f32::consts::FRAC_PI_2) };
    o.body_transform = Iso { translation: [10.0, 0.0, 0.0], rotation: ID };
    w.add_ref(&at(7, [0.0, 0.0, 0.0]), &[o]);
    // node ∘ body: the body offset is turned by the node rotation → (0, 10, 30).
    let hit = w.queries().ray([0.0, 10.0, 100.0], [0.0, 0.0, -1.0], 200.0, &Filter::default()).unwrap();
    assert!((hit.point[2] - 35.0).abs() < 1e-2, "{hit:?}");
}

#[test]
fn bad_inputs_are_reported_not_dropped() {
    let mut w = ground();
    let r = w.add_ref(&RefPose { rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, -1.0], ..at(1, [0.0; 3]) }, &[object(boxed([1.0; 3]), Motion::Static)]);
    assert_eq!((r.bodies, r.errors.len()), (0, 1));
    let shape = Shape::Compound {
        children: vec![(Iso::IDENTITY, boxed([1.0; 3])), (Iso::IDENTITY, Shape::Sphere { radius: -1.0 })],
    };
    let r = w.add_ref(&at(2, [0.0; 3]), &[object(shape, Motion::Static)]);
    assert_eq!((r.bodies, r.colliders, r.errors), (1, 1, vec![ConvertError::BadDimension]));
}

#[test]
fn trimesh_material_on_ray_hit() {
    let mut w = PhysWorld::new(PhysParams::default());
    let mesh = Shape::TriMesh {
        vertices: vec![[0.0, 0.0, 0.0], [100.0, 0.0, 0.0], [100.0, 100.0, 0.0], [0.0, 100.0, 0.0]],
        triangles: vec![[0, 1, 2], [0, 2, 3]],
        materials: vec![11, 22],
    };
    w.add_ref(&at(9, [0.0; 3]), &[object(mesh, Motion::Static)]);
    assert_eq!(down(&mut w, 80.0, 20.0).unwrap().material, Some(11));
    assert_eq!(down(&mut w, 20.0, 80.0).unwrap().material, Some(22));
    // From below (back face).
    let up = w.queries().ray([20.0, 80.0, -50.0], [0.0, 0.0, 1.0], 100.0, &Filter::default()).unwrap();
    assert_eq!(up.material, Some(22));
}

#[test]
fn ray_all_sweep_overlap_and_filters() {
    let mut w = ground();
    w.add_ref(&at(1, [0.0, 0.0, 10.0]), &[object(boxed([5.0; 3]), Motion::Static)]);
    w.add_ref(&at(2, [0.0, 0.0, 40.0]), &[object(boxed([5.0; 3]), Motion::Static)]);
    let q = w.queries();
    // Off the four-cell corner at the origin, so one terrain cell is below.
    let all = q.ray_all([1.0, 1.0, 100.0], [0.0, 0.0, -1.0], 200.0, &Filter::default());
    let keys: Vec<Key> = all.iter().map(|h| h.owner.key()).collect();
    assert_eq!(keys, vec![Key::Ref(2), Key::Ref(1), Key::Terrain(0, 0)]);
    let skip2 = q.ray([0.0, 0.0, 100.0], [0.0, 0.0, -1.0], 200.0, &Filter::excluding(Key::Ref(2))).unwrap();
    assert_eq!(skip2.owner.key(), Key::Ref(1));
    let no_terrain = Filter { no_terrain: true, ..Default::default() };
    assert!(q.ray([50.0, 0.0, 100.0], [0.0, 0.0, -1.0], 200.0, &no_terrain).is_none());
    // A sphere of radius 2 swept down from z = 100 touches the top box (z = 45) after 53.
    let ball = Shape::Sphere { radius: 2.0 };
    let from = Iso { translation: [0.0, 0.0, 100.0], rotation: ID };
    let hit = q.sweep(&ball, &from, [0.0, 0.0, -100.0], &Filter::default()).unwrap().unwrap();
    assert_eq!(hit.owner.key(), Key::Ref(2));
    assert!((hit.fraction - 0.53).abs() < 1e-3, "{hit:?}");
    assert!((hit.point[2] - 45.0).abs() < 1e-2 && hit.normal[2] > 0.99, "{hit:?}");
    let big = Shape::Sphere { radius: 22.0 };
    let mid = Iso { translation: [0.0, 0.0, 17.0], rotation: ID };
    let over: Vec<Key> = q.overlap(&big, &mid, &Filter::default()).unwrap().iter().map(|o| o.key()).collect();
    assert_eq!(over, vec![Key::Terrain(-1, -1), Key::Terrain(-1, 0), Key::Terrain(0, -1), Key::Terrain(0, 0), Key::Ref(1), Key::Ref(2)]);
}

#[test]
fn dynamic_clutter_settles_on_terrain() {
    let mut w = ground();
    let o = object(boxed([4.0, 4.0, 4.0]), Motion::Dynamic);
    w.add_ref(&RefPose { rotation: rz(0.3), ..at(5, [10.0, 10.0, 200.0]) }, &[o]);
    for _ in 0..600 {
        w.step();
    }
    let b = w.ref_bodies(5)[0];
    assert!((b.translation[2] - 4.0).abs() < 0.5, "{b:?}");
    assert!((b.translation[0] - 10.0).abs() < 2.0 && (b.translation[1] - 10.0).abs() < 2.0, "{b:?}");
    assert!(b.sleeping || b.linvel.iter().all(|v| v.abs() < 1.0), "{b:?}");
    let s = w.stats();
    assert_eq!((s.bodies_dynamic, s.steps), (1, 600));
}

#[test]
fn falling_body_follows_gravity() {
    // Free fall for 0.5 s: z = z0 - g t² / 2 (semi-implicit Euler is within g·dt·t).
    let params = PhysParams::default();
    let mut w = PhysWorld::new(params);
    w.add_ref(&at(1, [0.0, 0.0, 10_000.0]), &[object(Shape::Sphere { radius: 1.0 }, Motion::Dynamic)]);
    for _ in 0..30 {
        w.step();
    }
    let t = 30.0 * params.dt;
    let z = w.ref_bodies(1)[0].translation[2];
    let want = 10_000.0 - params.gravity * t * t / 2.0;
    assert!((z - want).abs() <= params.gravity * params.dt * t + 1e-2, "{z} vs {want}");
}

fn scene() -> PhysWorld {
    let mut w = ground();
    for i in 0..12u32 {
        let o = object(boxed([3.0, 3.0, 3.0]), Motion::Dynamic);
        w.add_ref(&RefPose { rotation: rz(i as f32), ..at(100 + i, [(i % 4) as f32 * 5.0, (i / 4) as f32 * 5.0, 20.0 + i as f32 * 7.0]) }, &[o]);
    }
    w
}

#[test]
fn stepping_is_deterministic() {
    let (mut a, mut b) = (scene(), scene());
    let mut digests = Vec::new();
    for _ in 0..240 {
        a.step();
        b.step();
        assert_eq!(a.state_digest(), b.state_digest());
        digests.push(a.state_digest());
    }
    // Something actually moved.
    assert!(digests.windows(2).any(|d| d[0] != d[1]));
}

#[test]
fn stepper_runs_whole_fixed_steps() {
    let p = PhysParams { dt: 0.01, max_substeps: 4, ..Default::default() };
    let mut s = Stepper::default();
    assert_eq!(s.advance(&p, 0.025), 2);
    assert_eq!(s.advance(&p, 0.006), 1);
    assert!((s.accumulated - 0.001).abs() < 1e-6);
    assert_eq!(s.advance(&p, 1.0), 4);
    assert!(s.accumulated <= p.dt);
    assert_eq!(s.advance(&p, -1.0), 1);
}

// --- character controller ---

fn settle(w: &mut PhysWorld, c: &mut Character, steps: usize) -> CharacterStep {
    let mut last = None;
    for _ in 0..steps {
        last = Some(w.move_character(c, CharacterInput::default()));
        w.step();
    }
    last.unwrap()
}

fn walk(w: &mut PhysWorld, c: &mut Character, velocity: [f32; 2], steps: usize) -> Vec<CharacterStep> {
    (0..steps)
        .map(|_| {
            let s = w.move_character(c, CharacterInput { velocity, jump: false, ..Default::default() });
            w.step();
            s
        })
        .collect()
}

#[test]
fn character_falls_lands_and_walks() {
    let mut w = ground();
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 300.0]);
    let mut landed = None;
    for _ in 0..120 {
        let s = w.move_character(&mut c, CharacterInput::default());
        w.step();
        if let Some(v) = s.landed_speed {
            landed = Some(v);
        }
    }
    assert!(c.grounded);
    // Fell ~300 units: touchdown speed ≈ sqrt(2 g h).
    let v = landed.expect("landed once");
    let want = (2.0 * w.params.gravity * 300.0f32).sqrt();
    assert!((v - want).abs() < 0.05 * want, "{v} vs {want}");
    // On the terrain's contact shell (physics.md §6.2).
    assert!((c.feet[2] - TERRAIN_SHELL).abs() < 0.02, "{:?}", c.feet);
    // fMoveBaseSpeed is 77 in FalloutNV.esm [observed]; whether it is a walk speed in
    // units/s is a hypothesis, used here only as a plausible speed.
    let steps = walk(&mut w, &mut c, [77.0, 0.0], 60);
    assert!(steps.iter().all(|s| s.grounded));
    assert!((c.feet[0] - 77.0).abs() < 1.0, "{:?}", c.feet);
    assert!(c.feet[1].abs() < 0.1, "{:?}", c.feet);
}

#[test]
fn character_climbs_steps_but_not_walls() {
    let p = CharacterParams::default();
    for (height, climbs) in [(p.step_height * 0.6, true), (p.step_height * 2.0, false)] {
        let mut w = ground();
        // A wide block whose west face is at x = 100.
        w.add_ref(&at(1, [300.0, 0.0, height / 2.0]), &[object(boxed([200.0, 200.0, height / 2.0]), Motion::Static)]);
        let mut c = w.add_character(1, p, [0.0, 0.0, 1.0]);
        settle(&mut w, &mut c, 30);
        walk(&mut w, &mut c, [150.0, 0.0], 90);
        if climbs {
            assert!(c.feet[0] > 150.0 && (c.feet[2] - height).abs() < 2.0, "height {height}: {:?}", c.feet);
        } else {
            assert!(c.feet[0] < 100.0 - p.radius + 1.0 && c.feet[2] < TERRAIN_SHELL + 1.0, "height {height}: {:?}", c.feet);
        }
    }
}

/// A ramp rising eastwards at `deg` degrees from x = 100, as a rotated box.
fn ramp(w: &mut PhysWorld, deg: f32) {
    let a = deg.to_radians();
    let (s, c) = a.sin_cos();
    // Rotation about y by -a tilts +x upwards.
    let rot = [c, 0.0, -s, 0.0, 1.0, 0.0, s, 0.0, c];
    let half = [400.0, 200.0, 10.0];
    // Place the top face's west edge at (100, 0, 0).
    let top_west = [-half[0], 0.0, half[2]];
    let rotated = [rot[0] * top_west[0] + rot[2] * top_west[2], 0.0, rot[6] * top_west[0] + rot[8] * top_west[2]];
    let pos = [100.0 - rotated[0], 0.0, -rotated[2]];
    w.add_ref(&RefPose { form_id: 50, position: pos, rotation: rot, scale: 1.0 }, &[object(boxed(half), Motion::Static)]);
}

#[test]
fn character_respects_slope_limit() {
    for (deg, climbs) in [(25.0f32, true), (60.0, false)] {
        let mut w = ground();
        ramp(&mut w, deg);
        let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
        settle(&mut w, &mut c, 30);
        walk(&mut w, &mut c, [150.0, 0.0], 120);
        let up = c.feet[2];
        if climbs {
            assert!(up > 50.0, "{deg}°: {:?}", c.feet);
        } else {
            assert!(up < c.params.step_height + 2.0, "{deg}°: {:?}", c.feet);
        }
    }
}

#[test]
fn character_jump_apex() {
    let mut w = ground();
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
    settle(&mut w, &mut c, 30);
    let z0 = c.feet[2];
    let mut apex = z0;
    let mut grounded_again = false;
    for i in 0..120 {
        let s = w.move_character(&mut c, CharacterInput { velocity: [0.0, 0.0], jump: i == 0, ..Default::default() });
        w.step();
        apex = apex.max(s.feet[2]);
        if i > 5 && s.grounded {
            grounded_again = true;
            break;
        }
    }
    assert!(grounded_again);
    // Discrete integration lands within one step's travel of the analytic apex.
    let g = w.params.gravity;
    let tol = (2.0 * g * c.params.jump_height).sqrt() * w.params.dt;
    assert!((apex - z0 - c.params.jump_height).abs() < tol, "apex {}", apex - z0);
}

#[test]
fn character_ceiling_stops_jump() {
    let mut w = ground();
    // A slab 20 units above the head.
    let top = CharacterParams::default().height + 20.0;
    w.add_ref(&at(1, [0.0, 0.0, top + 5.0]), &[object(boxed([200.0, 200.0, 5.0]), Motion::Static)]);
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
    settle(&mut w, &mut c, 10);
    let mut apex: f32 = 0.0;
    for i in 0..60 {
        let s = w.move_character(&mut c, CharacterInput { velocity: [0.0, 0.0], jump: i == 0, ..Default::default() });
        w.step();
        apex = apex.max(s.feet[2]);
    }
    assert!(apex < 21.0, "head went through the ceiling: {apex}");
    assert!(c.grounded);
}

#[test]
fn character_slides_along_walls() {
    let mut w = ground();
    // A wall along y = 50 (south face at 50), long in x.
    w.add_ref(&at(1, [0.0, 60.0, 100.0]), &[object(boxed([500.0, 10.0, 100.0]), Motion::Static)]);
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
    settle(&mut w, &mut c, 30);
    walk(&mut w, &mut c, [100.0, 100.0], 120);
    assert!(c.feet[1] <= 50.0 - c.params.radius + 0.01, "{:?}", c.feet);
    assert!(c.feet[0] > 100.0, "kept moving along the wall: {:?}", c.feet);
}

#[test]
fn character_pushes_dynamic_clutter() {
    let mut w = ground();
    w.add_ref(&at(1, [60.0, 0.0, 6.0]), &[object(boxed([6.0, 6.0, 6.0]), Motion::Dynamic)]);
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
    settle(&mut w, &mut c, 30);
    let steps = walk(&mut w, &mut c, [120.0, 0.0], 60);
    assert!(steps.iter().any(|s| s.touched.iter().any(|o| o.key() == Key::Ref(1))));
    assert!(w.ref_bodies(1)[0].translation[0] > 62.0, "{:?}", w.ref_bodies(1));
}

#[test]
fn activation_ray_skips_the_caster() {
    let mut w = ground();
    w.add_ref(&at(77, [100.0, 0.0, 100.0]), &[object(boxed([10.0; 3]), Motion::Static)]);
    let c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 0.0]);
    let eye = [0.0, 0.0, 100.0];
    let q = w.queries();
    let any = q.ray(eye, [1.0, 0.0, 0.0], 150.0, &Filter::default()).unwrap();
    assert_eq!(any.owner, Owner::Character { id: 1 }, "the eye is inside the caster's capsule");
    let hit = q.ray(eye, [1.0, 0.0, 0.0], 150.0, &Filter::excluding(c.key())).unwrap();
    assert_eq!(hit.owner.key(), Key::Ref(77));
    assert!((hit.distance - 90.0).abs() < 1e-3);
    assert!(q.ray(eye, [1.0, 0.0, 0.0], 80.0, &Filter::excluding(c.key())).is_none());
}

#[test]
fn character_runs_are_deterministic() {
    let run = || {
        let mut w = scene();
        let mut c = w.add_character(1, CharacterParams::default(), [-50.0, -50.0, 5.0]);
        let mut trace = Vec::new();
        for i in 0..300u32 {
            let a = i as f32 * 0.05;
            let s = w.move_character(&mut c, CharacterInput { velocity: [120.0 * a.cos(), 120.0 * a.sin()], jump: i.is_multiple_of(97), ..Default::default() });
            w.step();
            trace.push((s.feet.map(f32::to_bits), w.state_digest()));
        }
        trace
    };
    assert_eq!(run(), run());
}

proptest! {
    #[test]
    fn owner_roundtrip(x in any::<i32>(), y in any::<i32>(), f in any::<u32>(), o in any::<u16>(), l in any::<u8>()) {
        for owner in [Owner::Terrain { x, y }, Owner::Ref { form_id: f, object: o, layer: l }, Owner::Character { id: f }] {
            prop_assert_eq!(Owner::decode(owner.encode()), Some(owner));
        }
    }

    #[test]
    fn terrain_ray_matches_analytic(seed in proptest::collection::vec(-40i32..40, SIDE * SIDE), px in 0.0f32..4096.0, py in 0.0f32..4096.0) {
        let heights: Vec<f32> = seed.iter().map(|&v| v as f32 * 8.0).collect();
        let mut w = PhysWorld::new(PhysParams::default());
        w.add_terrain((2, -5), &heights);
        let (x0, y0) = (2.0 * CELL_SIZE, -5.0 * CELL_SIZE);
        let hit = down(&mut w, x0 + px, y0 + py).expect("hit");
        let want = height_at(&heights, px, py).unwrap();
        prop_assert!((hit.point[2] - want).abs() < 0.05, "{} vs {}", hit.point[2], want);
    }
}

#[test]
fn bodies_added_then_queried_still_simulate() {
    // Adding a dynamic body and querying before the first step syncs the broad phase
    // through rapier's collision-only pipeline; the body must still fall afterwards.
    let mut w = ground();
    w.add_ref(&at(0x99, [100.0, 100.0, 500.0]), &[object(boxed([10.0, 10.0, 10.0]), Motion::Dynamic)]);
    let _ = down(&mut w, 0.0, 0.0);
    for _ in 0..30 {
        w.step();
    }
    let z = w.ref_bodies(0x99)[0].translation[2];
    assert!(z < 490.0, "the box fell: z = {z}");
}

#[test]
fn snapshot_continues_identically() {
    let mut a = scene();
    let mut ca = a.add_character(1, CharacterParams::default(), [-50.0, -50.0, 5.0]);
    let input = |i: u32| CharacterInput { velocity: [100.0, 60.0 - i as f32 * 0.4], jump: i.is_multiple_of(53), ..Default::default() };
    for i in 0..90u32 {
        a.move_character(&mut ca, input(i));
        a.step();
    }
    // The clutter was simulated (the top box started at z = 97).
    assert!(a.ref_bodies(111)[0].translation[2] < 60.0, "{:?}", a.ref_bodies(111));
    let mut b = a.snapshot();
    let mut cb = ca.clone();
    for i in 90..300u32 {
        let sa = a.move_character(&mut ca, input(i));
        let sb = b.move_character(&mut cb, input(i));
        a.step();
        b.step();
        assert_eq!(sa, sb);
        assert_eq!(a.state_digest(), b.state_digest(), "step {i}");
    }
}

fn on_layer(shape: Shape, layer: u8) -> CollisionObject {
    CollisionObject { layer, ..object(shape, Motion::Static) }
}

#[test]
fn queries_skip_excluded_layers() {
    let mut w = ground();
    w.add_ref(&at(1, [0.0, 0.0, 50.0]), &[on_layer(boxed([20.0, 20.0, 20.0]), layers::TRIGGER)]);
    let hit = down(&mut w, 0.0, 0.0).unwrap();
    assert_eq!(hit.owner, Owner::Ref { form_id: 1, object: 0, layer: layers::TRIGGER });
    let f = Filter { exclude_layers: LayerSet::from_layers(&[layers::TRIGGER]), ..Default::default() };
    let hit = w.queries().ray([0.0, 0.0, 1_000.0], [0.0, 0.0, -1.0], 2_000.0, &f).unwrap();
    assert_eq!(hit.owner, Owner::Terrain { x: 0, y: 0 });
}

#[test]
fn character_passes_through_non_solid_layers() {
    for (layer, blocks) in [(layers::TRIGGER, false), (layers::STATIC, true), (layers::INVISIBLE_WALL, true)] {
        let mut w = ground();
        // A wall across the path, west face at x = 100.
        w.add_ref(&at(1, [110.0, 0.0, 100.0]), &[on_layer(boxed([10.0, 300.0, 100.0]), layer)]);
        let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 1.0]);
        settle(&mut w, &mut c, 30);
        walk(&mut w, &mut c, [150.0, 0.0], 90);
        let through = c.feet[0] > 200.0;
        assert_eq!(through, !blocks, "layer {layer} ({:?}): {:?}", layers::name(layer as u32), c.feet);
    }
}

#[test]
fn collision_marker_primitives_become_boxes() {
    use fnv_phys::primitive::{primitive_object, BoundsMeaning, Primitive, PrimitiveKind};
    let p = Primitive { bounds: [100.0, 10.0, 200.0], kind: PrimitiveKind::PortalBox, layer: None };
    let o = primitive_object(&p, BoundsMeaning::HalfExtents).unwrap();
    let mut w = ground();
    // A reference turned 90° about z: the long x bound lies along world y.
    let r = w.add_ref(&RefPose { rotation: rz(std::f32::consts::FRAC_PI_2), ..at(0x21_0001, [500.0, 500.0, 150.0]) }, &[o]);
    assert_eq!((r.bodies, r.colliders, r.errors.len()), (1, 1, 0));
    assert!(down(&mut w, 500.0, 595.0).is_some_and(|h| h.owner.key() == Key::Ref(0x21_0001)));
    assert!(down(&mut w, 515.0, 500.0).is_some_and(|h| h.owner.key() == Key::Terrain(0, 0)));
}

#[test]
fn arrival_rests_on_the_support_shell() {
    let mut w = ground();
    // A static slab (top at z = 40) carrying a 0.7-unit shell, at x = 500.
    let mut slab = object(boxed([50.0, 50.0, 20.0]), Motion::Static);
    slab.shell = 0.7;
    w.add_ref(&at(0x10, [500.0, 0.0, 20.0]), &[slab]);
    let p = CharacterParams::default();
    let f = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
    // Below the terrain, on it, or above it: the feet end on the terrain's shell.
    for z in [-8.0, 0.0, 3.0, 60.0] {
        let a = w.arrival([100.0, 100.0, z], &p, &f);
        assert!((a.feet[2] - TERRAIN_SHELL).abs() < 1e-3, "from {z}: {:?}", a.feet);
        let s = a.support.expect("terrain under the point");
        assert!(matches!(s.owner, Owner::Terrain { .. }) && s.point[2].abs() < 1e-3 && s.shell == TERRAIN_SHELL);
        assert_eq!((a.feet[0], a.feet[1]), (100.0, 100.0));
    }
    // On the slab: its shell, from just under its top as from above.
    for z in [38.0, 40.0, 90.0] {
        let a = w.arrival([500.0, 0.0, z], &p, &f);
        assert!((a.feet[2] - 40.7).abs() < 1e-3, "from {z}: {:?}", a.feet);
        assert_eq!(a.support.unwrap().owner, Owner::Ref { form_id: 0x10, object: 0, layer: 1 });
    }
    // The shell scales with the reference.
    let mut slab = object(boxed([50.0, 50.0, 20.0]), Motion::Static);
    slab.shell = 0.7;
    w.add_ref(&RefPose { scale: 2.0, ..at(0x11, [-500.0, 0.0, 40.0]) }, &[slab]);
    let a = w.arrival([-500.0, 0.0, 80.0], &p, &f);
    assert!((a.feet[2] - 81.4).abs() < 1e-3, "{:?}", a.feet);
    // Nothing in range: the actor stays where it was put.
    let a = w.arrival([100.0, 100.0, 5_000.0], &p, &f);
    assert_eq!((a.feet, a.support), ([100.0, 100.0, 5_000.0], None));
}

/// Two static walls forming a notch that opens to the west, apex at (200, 0), faces at
/// `half_angle` degrees either side of the x axis.
fn notch(w: &mut PhysWorld, half_angle: f32) {
    let a = half_angle.to_radians();
    let (s, c) = a.sin_cos();
    for (id, sign) in [(60u32, 1.0f32), (61, -1.0)] {
        // Wall along the line from the apex towards (-c, sign·s); the notch's inside is
        // on the side of normal (-s, -sign·c).
        let along = [-c, sign * s];
        let out = [s, sign * c];
        let centre = [200.0 + along[0] * 100.0 + out[0] * 5.0, along[1] * 100.0 + out[1] * 5.0, 100.0];
        w.add_ref(&RefPose { rotation: rz(-sign * a), ..at(id, centre) }, &[object(boxed([100.0, 5.0, 100.0]), Motion::Static)]);
    }
}

#[test]
fn character_wedged_in_a_corner_stays_put() {
    // Regression: rapier's controller slid along one face of a concave corner at a time
    // and went back and forth (about 2 units per step against rock 0x000EDE82,
    // physics.md §6.1).
    for half_angle in [20.0f32, 30.0, 45.0] {
        let mut w = ground();
        notch(&mut w, half_angle);
        let mut c = w.add_character(1, CharacterParams::default(), [0.0, 3.0, 1.0]);
        settle(&mut w, &mut c, 30);
        let steps = walk(&mut w, &mut c, [300.0, 20.0], 180);
        let tail = &steps[120..];
        let path: f32 = tail.windows(2).map(|p| (p[1].feet[0] - p[0].feet[0]).hypot(p[1].feet[1] - p[0].feet[1])).sum();
        let first = tail[0].feet;
        let last = tail[tail.len() - 1].feet;
        assert!(last[0] > 100.0, "{half_angle}°: reached the notch: {last:?}");
        let net = (last[0] - first[0]).hypot(last[1] - first[1]);
        // Pressed into the corner it may still settle deeper, but never back and forth.
        assert!(path < 1.0 && path - net < 0.1, "{half_angle}°: path {path}, net {net} over the last 60 steps");
        assert!((last[2] - first[2]).abs() < 0.01, "{half_angle}°: crept from {} to {}", first[2], last[2]);
    }
}

#[test]
fn character_rests_on_contact_shells() {
    // Terrain (shell 3.5) and a static slab with a 0.7 shell whose top is at z = 40.
    let mut w = ground();
    let mut slab = object(boxed([60.0, 60.0, 10.0]), Motion::Static);
    slab.shell = 0.7;
    w.add_ref(&at(0x10, [300.0, 0.0, 30.0]), &[slab]);
    let p = CharacterParams::default();
    let f = Filter { exclude_layers: layers::character_ignores(), ..Default::default() };
    // Dropped from above, the controller ends where the arrival rule puts the feet.
    let mut c = w.add_character(1, p, [0.0, 0.0, 60.0]);
    let s = settle(&mut w, &mut c, 90);
    let a = w.arrival([0.0, 0.0, 60.0], &p, &Filter { exclude: vec![c.key()], ..f.clone() });
    assert!(s.grounded && (s.feet[2] - TERRAIN_SHELL).abs() < 0.02, "{:?}", s.feet);
    assert!((s.feet[2] - a.feet[2]).abs() < 0.02 && s.gap == TERRAIN_SHELL);
    // Standing still, it stays there.
    let s2 = settle(&mut w, &mut c, 60);
    assert!((0..3).all(|i| (s2.feet[i] - s.feet[i]).abs() < 0.01), "{:?} -> {:?}", s.feet, s2.feet);
    // Walking onto the slab (the edge is 40 - 3.5 high from the terrain's shell: too high
    // to step, so start on it) rests 0.7 above it; walking off onto the terrain, 3.5.
    w.place_character(&mut c, [300.0, 0.0, 41.0]);
    let s = settle(&mut w, &mut c, 30);
    assert!((s.feet[2] - 40.7).abs() < 0.02 && (s.gap - 0.7).abs() < 1e-6, "{:?}", s.feet);
    let steps = walk(&mut w, &mut c, [-200.0, 0.0], 120);
    let s = steps.last().unwrap();
    assert!(s.feet[0] < 200.0 && s.grounded, "{:?}", s.feet);
    assert!((s.feet[2] - TERRAIN_SHELL).abs() < 0.02 && s.gap == TERRAIN_SHELL, "{:?}", s.feet);
    // A low static floor flush with the terrain: from terrain onto it the feet sink to its
    // shell, and back on the terrain they are lifted again.
    let mut mat = object(boxed([60.0, 60.0, 0.5]), Motion::Static);
    mat.shell = 0.7;
    w.add_ref(&at(0x11, [-300.0, 0.0, 0.0]), &[mat]);
    let steps = walk(&mut w, &mut c, [-200.0, 0.0], 60);
    let on_mat = steps.iter().rfind(|s| s.feet[0] < -260.0 && s.feet[0] > -340.0).expect("crossed the mat");
    assert!((on_mat.feet[2] - 1.2).abs() < 0.05, "{:?}", on_mat.feet);
    let steps = walk(&mut w, &mut c, [-200.0, 0.0], 60);
    let s = steps.last().unwrap();
    assert!((s.feet[2] - TERRAIN_SHELL).abs() < 0.05, "{:?}", s.feet);
}

#[test]
fn walking_on_level_ground_loses_no_step() {
    // Regression: rapier dropped a step's horizontal motion now and then on exactly level
    // surfaces (degenerate slope tangent), here once in 60 steps along a cell seam.
    let mut w = ground();
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 300.0]);
    settle(&mut w, &mut c, 120);
    let mut prev = c.feet;
    for (i, s) in walk(&mut w, &mut c, [77.0, 0.0], 240).into_iter().enumerate() {
        let dx = s.feet[0] - prev[0];
        assert!((dx - 77.0 * w.params.dt).abs() < 1e-2, "step {i}: moved {dx}");
        prev = s.feet;
    }
}

// --- swimming (physics.md §6.6) ---

/// The flat ground of [`ground`] under water whose surface is 500 units above it.
fn lake() -> PhysWorld {
    let mut w = ground();
    for x in -1..=0 {
        for y in -1..=0 {
            w.set_water(WaterArea::Exterior(x, y), Some(500.0));
        }
    }
    w
}

fn swim(w: &mut PhysWorld, c: &mut Character, velocity: [f32; 2], vertical: f32, jump: bool, steps: usize) -> CharacterStep {
    let mut last = None;
    for i in 0..steps {
        last = Some(w.move_character(c, CharacterInput { velocity, jump: jump && i == 0, vertical }));
        w.step();
    }
    last.unwrap()
}

#[test]
fn a_fall_into_water_stops_and_floats() {
    let mut w = lake();
    let p = CharacterParams::default();
    let mut c = w.add_character(1, p, [0.0, 0.0, 500.0]);
    let mut landed = false;
    let mut entered_at = None;
    for i in 0..240 {
        let s = w.move_character(&mut c, CharacterInput::default());
        w.step();
        landed |= s.landed_speed.is_some();
        if s.swimming && entered_at.is_none() {
            entered_at = Some((i, s.feet[2]));
        }
    }
    let (_, z) = entered_at.expect("swims");
    // The fall stops in the step that takes the feet past the swimming depth.
    assert!(z <= 500.0 - p.swim_depth && z > 500.0 - p.swim_depth - 7.0, "{z}");
    assert!(!landed);
    assert!(c.swimming && !c.grounded);
    // Above the floating height it sinks to it and stays.
    assert!((c.feet[2] - (500.0 - p.float_depth)).abs() < 0.01 || c.feet[2] < 500.0 - p.float_depth, "{:?}", c.feet);
}

#[test]
fn walking_on_the_bottom_does_not_swim() {
    let mut w = lake();
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, TERRAIN_SHELL]);
    let steps = walk(&mut w, &mut c, [86.8, 0.0], 120);
    assert!(steps.iter().all(|s| s.grounded && !s.swimming));
    assert!((c.feet[0] - 173.6).abs() < 1.0, "{:?}", c.feet);
}

#[test]
fn swimming_follows_the_view_and_has_no_buoyancy() {
    let mut w = lake();
    let p = CharacterParams::default();
    let mut c = w.add_character(1, p, [0.0, 0.0, 405.0]);
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 120);
    assert!(c.swimming);
    let float = 500.0 - p.float_depth;
    assert!((c.feet[2] - float).abs() < 0.01, "{:?}", c.feet);
    // Horizontal swimming keeps the floating height.
    swim(&mut w, &mut c, [348.0, 0.0], 0.0, false, 60);
    assert!((c.feet[0] - 348.0).abs() < 1.0 && (c.feet[2] - float).abs() < 0.01, "{:?}", c.feet);
    // Diving at 30 degrees down for half a second.
    let (s, k) = 30f32.to_radians().sin_cos();
    swim(&mut w, &mut c, [348.0 * k, 0.0], -348.0 * s, false, 30);
    assert!((c.feet[2] - (float - 87.0)).abs() < 1.0, "{:?}", c.feet);
    // Released, it stays at its depth.
    let z = c.feet[2];
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 120);
    assert!((c.feet[2] - z).abs() < 0.01 && c.swimming, "{:?}", c.feet);
    // Swimming up stops within one step above the floating height and sinks back.
    let mut top = f32::MIN;
    for _ in 0..90 {
        let st = swim(&mut w, &mut c, [0.0, 0.0], 246.0, false, 1);
        top = top.max(st.feet[2]);
    }
    assert!(top > float && top <= float + 246.0 / 60.0 + 0.01, "{top}");
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 60);
    assert!((c.feet[2] - float).abs() < 0.01, "{:?}", c.feet);
}

#[test]
fn jumping_works_only_above_the_floating_height() {
    let mut w = lake();
    let p = CharacterParams::default();
    let mut c = w.add_character(1, p, [0.0, 0.0, 405.0]);
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 120);
    let float = c.feet[2];
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, true, 30);
    assert!((c.feet[2] - float).abs() < 0.01 && c.swimming, "{:?}", c.feet);
    // Placed above the floating height (the swimming state is kept), it jumps out at twice
    // the land height and falls back into the water.
    w.place_character(&mut c, [0.0, 0.0, 440.0]);
    assert!(c.swimming);
    let mut top = f32::MIN;
    for i in 0..120 {
        let st = swim(&mut w, &mut c, [0.0, 0.0], 0.0, i == 0, 1);
        top = top.max(st.feet[2]);
    }
    let rise = top - 440.0;
    assert!((rise - 2.0 * p.jump_height).abs() < 6.0, "{rise}");
    assert!(c.swimming);
}

#[test]
fn swimming_down_to_the_bottom_ends_swimming() {
    let mut w = lake();
    let mut c = w.add_character(1, CharacterParams::default(), [0.0, 0.0, 405.0]);
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 60);
    assert!(c.swimming);
    swim(&mut w, &mut c, [0.0, 0.0], -348.0, false, 120);
    assert!(!c.swimming && c.grounded, "{:?}", c.feet);
    assert!((c.feet[2] - TERRAIN_SHELL).abs() < 0.05, "{:?}", c.feet);
}

#[test]
fn swim_vertical_rules() {
    let p = CharacterParams::default();
    let dt = 1.0 / 60.0;
    let float = 500.0 - p.float_depth;
    // Diving always applies; rising only from the floating height or below it.
    assert_eq!(fnv_phys::character::swim_vertical(&p, float + 30.0, 500.0, -100.0, dt), -100.0);
    assert_eq!(fnv_phys::character::swim_vertical(&p, float - 30.0, 500.0, 100.0, dt), 100.0);
    assert_eq!(fnv_phys::character::swim_vertical(&p, float, 500.0, 100.0, dt), 100.0);
    assert_eq!(fnv_phys::character::swim_vertical(&p, float + 30.0, 500.0, 100.0, dt), -p.swim_sink_speed);
    // Sinking stops at the floating height; below it there is no buoyancy.
    let v = fnv_phys::character::swim_vertical(&p, float + 0.1, 500.0, 0.0, dt);
    assert!((v * dt + 0.1).abs() < 1e-4, "{v}");
    assert_eq!(fnv_phys::character::swim_vertical(&p, float - 50.0, 500.0, 0.0, dt), 0.0);
}

#[test]
fn swimming_survives_above_the_surface() {
    // Placed 60 above the surface while swimming, the character keeps swimming and sinks
    // at the sinking speed (the game's player did 101 above placed water, physics.md §6.6).
    let mut w = lake();
    let p = CharacterParams::default();
    let mut c = w.add_character(1, p, [0.0, 0.0, 405.0]);
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 60);
    assert!(c.swimming);
    w.place_character(&mut c, [0.0, 0.0, 560.0]);
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 60);
    assert!(c.swimming && !c.grounded);
    assert!((c.feet[2] - (560.0 - p.swim_sink_speed)).abs() < 0.05, "{:?}", c.feet);
}

/// A placeable water model's volume: a box under the surface (half extents of the
/// 1×2 model, its top at the model's origin), as the Water-layer phantoms hold it.
fn water_box() -> CollisionObject {
    let mut o = on_layer(boxed([256.0, 512.0, 512.0]), layers::WATER);
    o.body_transform.translation = [0.0, 0.0, -512.0];
    o
}

#[test]
fn placed_water_is_the_box_of_its_reference() {
    let mut w = ground();
    // Scale 2, turned a quarter: 1024 wide along x, 2048 along y before the turn.
    let pose = RefPose { form_id: 7, position: [1000.0, 0.0, 300.0], rotation: rz(std::f32::consts::FRAC_PI_2), scale: 2.0 };
    assert_eq!(w.add_placed_water(&pose, &[water_box()]), 1);
    let v = *w.placed_water().next().unwrap();
    assert!((v.surface - 300.0).abs() < 1e-3, "{v:?}");
    // After the turn the long side runs along x.
    assert_eq!(w.water_surface_at(1000.0 + 1000.0, 0.0), Some(300.0));
    assert_eq!(w.water_surface_at(1000.0, 1000.0), None);
    assert_eq!(w.water_surface_at(1000.0, 500.0), Some(300.0));
    // Down to the bottom of the box only, for the character's lookup.
    assert_eq!(w.water_surface_for([1000.0, 0.0, 300.0 - 2047.0]), Some(300.0));
    assert_eq!(w.water_surface_for([1000.0, 0.0, 300.0 - 2049.0]), None);
    // A fall into it stops past the swimming depth; outside the box it lands.
    let p = CharacterParams::default();
    let mut c = w.add_character(1, p, [1000.0, 0.0, 305.0]);
    swim(&mut w, &mut c, [0.0, 0.0], 0.0, false, 240);
    assert!(c.swimming && (c.feet[2] - (300.0 - p.float_depth)).abs() < 8.0, "{:?}", c.feet);
    let mut d = w.add_character(2, p, [1000.0, 1000.0, 305.0]);
    swim(&mut w, &mut d, [0.0, 0.0], 0.0, false, 240);
    assert!(!d.swimming && d.grounded, "{:?}", d.feet);
    // Removed with its reference.
    assert!(w.remove_ref(7));
    assert_eq!(w.water_surface_at(1000.0, 0.0), None);
}
