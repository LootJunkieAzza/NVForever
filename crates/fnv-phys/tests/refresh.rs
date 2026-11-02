//! Bringing queries up to date between steps (`PhysWorld::update_queries`): the reasons
//! each refresh is counted by (`fnv_phys::refresh`). Hand-built worlds, no game data.

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::*;
use fnv_phys::layers;
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
    }
}

fn at(form_id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id, position, rotation: ID, scale: 1.0 }
}

fn ground() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    w
}

fn person(feet: [f32; 3]) -> HitVolume {
    HitVolume { feet, radius: 20.0, height: 128.0, dead: false }
}

fn arm(feet: [f32; 3], lift: f32) -> Vec<PartCapsule> {
    vec![
        PartCapsule { part: 0, a: [feet[0], feet[1], feet[2] + 40.0], b: [feet[0], feet[1], feet[2] + 100.0], radius: 15.0 },
        PartCapsule { part: 1, a: [feet[0] + 20.0, feet[1], feet[2] + 90.0], b: [feet[0] + 50.0, feet[1], feet[2] + 90.0 + lift], radius: 5.0 },
    ]
}

#[test]
fn refreshes_are_counted_by_the_reasons_that_called_for_them() {
    let mut w = ground();
    assert_eq!(w.pending_refresh(), DirtyReasons::TERRAIN);
    w.update_queries();
    assert!(w.pending_refresh().is_empty());
    // Up to date: no second refresh.
    w.update_queries();
    assert_eq!(w.refresh_stats().full, 1);
    assert_eq!(w.refresh_stats().by_reasons[&DirtyReasons::TERRAIN], 1);

    w.set_actor_parts(0x10, person([200.0, 0.0, 0.0]), &arm([200.0, 0.0, 0.0], 0.0));
    assert!(w.pending_refresh().contains(DirtyReasons::ACTOR_PARTS));
    w.update_queries();
    w.set_actor_parts(0x10, person([200.0, 0.0, 0.0]), &arm([200.0, 0.0, 0.0], 10.0));
    w.add_ref(&at(7, [0.0, 0.0, 50.0]), &[object(Shape::Sphere { radius: 5.0 }, Motion::Static)]);
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES);
    w.update_queries();
    let s = w.refresh_stats();
    assert_eq!(s.by_reasons[&(DirtyReasons::ACTOR_PARTS | DirtyReasons::BODIES)], 1);
    assert_eq!(s.full, s.by_reasons.values().sum::<u64>());

    // A step brings everything up to date without counting a refresh.
    w.set_actor_volume(0x11, person([-200.0, 0.0, 0.0]));
    let before = w.refresh_stats().full;
    w.step();
    assert!(w.pending_refresh().is_empty());
    w.update_queries();
    assert_eq!(w.refresh_stats().full, before);
    // A snapshot keeps the counts.
    assert_eq!(w.snapshot().refresh_stats(), w.refresh_stats());
}

#[test]
fn setting_the_same_parts_again_needs_no_refresh() {
    let mut w = ground();
    let feet = [200.0, 0.0, 0.0];
    w.set_actor_parts(0x10, person(feet), &arm(feet, 0.0));
    w.step();
    assert!(w.pending_refresh().is_empty());
    let shot = |w: &mut PhysWorld| w.queries().ray([0.0, 0.0, 70.0], [1.0, 0.0, 0.0], 1000.0, &Filter::default());
    let before = shot(&mut w).expect("hits the body capsule");
    // The same pose: nothing to refresh, and the same hit.
    w.set_actor_parts(0x10, person(feet), &arm(feet, 0.0));
    assert!(w.pending_refresh().is_empty());
    assert_eq!(shot(&mut w), Some(before));
    // A pose that moves one capsule a little (less than the resize tolerance): a refresh.
    w.set_actor_parts(0x10, person(feet), &arm(feet, 0.01));
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS);
    w.update_queries();
    w.set_actor_parts(0x10, person(feet), &arm(feet, 0.01));
    assert!(w.pending_refresh().is_empty());
    // A radius changed by less than the tolerance keeps the shape but changes the stored
    // size later calls compare with: a refresh too.
    let mut thinner = arm(feet, 0.01);
    thinner[1].radius -= 0.1;
    w.set_actor_parts(0x10, person(feet), &thinner);
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS);
    w.update_queries();
    // Dead (another layer), moved feet, and a different part list all count as changes.
    w.set_actor_parts(0x10, HitVolume { dead: true, ..person(feet) }, &thinner);
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS);
    w.update_queries();
    w.set_actor_parts(0x10, HitVolume { dead: true, ..person([201.0, 0.0, 0.0]) }, &thinner);
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS);
    w.update_queries();
    w.set_actor_parts(0x10, HitVolume { dead: true, ..person([201.0, 0.0, 0.0]) }, &thinner[..1]);
    assert!(w.pending_refresh().contains(DirtyReasons::ACTOR_PARTS));
}

// ---------------------------------------------------------------------------------------
// Moved actor hit colliders refreshed without a detection pass give the queries exactly
// what a full pass gives them, and leave the next step exactly as it would have been
// without the query.

const ACTORS: [u32; 4] = [0x100, 0x101, 0x102, 0x103];
const VOLUME_ACTOR: u32 = 0x200;
const DOOR: u32 = 0x50;
const PARTS: usize = 4;
const JIT: usize = 7;
const CLUTTER: std::ops::Range<u32> = 0x60..0x66;

fn actor_feet(j: usize) -> [f32; 3] {
    [-450.0 + 300.0 * j as f32, 150.0 * (j % 2) as f32, 0.0]
}

/// Four capsules on a 2 x 2 grid 100 units apart, each moved by `jit` (7 values per part
/// in -1..1): they stay more than 50 units apart, so no ray meets two of them at the same
/// distance (an exact tie's winner depends on the broad-phase tree, in either path).
fn parts(feet: [f32; 3], jit: &[f32]) -> Vec<PartCapsule> {
    (0..PARTS)
        .map(|i| {
            let j = &jit[i * JIT..(i + 1) * JIT];
            let c = [feet[0] + (i % 2) as f32 * 100.0 - 50.0, feet[1] + (i / 2) as f32 * 100.0 - 50.0, feet[2] + 70.0];
            let a = [c[0] + j[0] * 8.0, c[1] + j[1] * 8.0, c[2] - 15.0 + j[2] * 5.0];
            let b = [c[0] + j[3] * 8.0, c[1] + j[4] * 8.0, c[2] + 15.0 + j[5] * 5.0];
            PartCapsule { part: i as u8, a, b, radius: 9.0 + j[6] * 3.0 }
        })
        .collect()
}

fn door_pose() -> RefPose {
    at(DOOR, [300.0, -500.0, 60.0])
}

/// Terrain, static boxes and a wall, a door, falling clutter (awake, touching the ground
/// by the end), two characters, four actors with part capsules and one with a single
/// capsule; stepped until everything has been through a step. The four terrain cells are
/// flat at different heights, so a sweep landing across a cell edge touches one cell first
/// (on one level, both at the same time: a tie, whose winner depends on the broad-phase
/// tree in either path).
fn base() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for (i, (x, y)) in [(-1, -1), (-1, 0), (0, -1), (0, 0)].into_iter().enumerate() {
        w.add_terrain((x, y), &vec![i as f32; SIDE * SIDE]);
    }
    let boxed = |h: [f32; 3]| Shape::Box { half_extents: h };
    w.add_ref(&at(0x40, [0.0, -300.0, 30.0]), &[object(boxed([40.0, 40.0, 30.0]), Motion::Static)]);
    w.add_ref(&at(0x41, [-300.0, -300.0, 20.0]), &[object(boxed([30.0, 50.0, 20.0]), Motion::Static)]);
    w.add_ref(&at(0x42, [750.0, 0.0, 60.0]), &[object(boxed([10.0, 200.0, 60.0]), Motion::Static)]);
    w.add_ref(&door_pose(), &[object(boxed([5.0, 40.0, 60.0]), Motion::Keyframed)]);
    for (i, id) in CLUTTER.enumerate() {
        let p = [-250.0 + 100.0 * i as f32, -550.0, 30.0 + 15.0 * i as f32];
        w.add_ref(&at(id, p), &[object(boxed([8.0, 8.0, 8.0]), Motion::Dynamic)]);
    }
    w.add_character(1, CharacterParams::default(), [-650.0, -400.0, 0.0]);
    w.add_character(2, CharacterParams::default(), [650.0, -400.0, 0.0]);
    let still = vec![0.0; PARTS * JIT];
    for (j, &id) in ACTORS.iter().enumerate() {
        let feet = actor_feet(j);
        w.set_actor_parts(id, person(feet), &parts(feet, &still));
    }
    w.set_actor_volume(VOLUME_ACTOR, person([500.0, -300.0, 0.0]));
    for _ in 0..24 {
        w.step();
    }
    assert!(w.pending_refresh().is_empty());
    w
}

/// Pose every actor from `jit` (`ACTORS.len() * PARTS * JIT` values), `moves` (10) and
/// `dead` (4): moved only, so the refresh can skip the detection pass.
fn pose(w: &mut PhysWorld, jit: &[f32], moves: &[f32], dead: &[bool]) {
    for (j, &id) in ACTORS.iter().enumerate() {
        let f = actor_feet(j);
        let feet = [f[0] + moves[2 * j] * 10.0, f[1] + moves[2 * j + 1] * 10.0, f[2]];
        let v = HitVolume { dead: dead[j], ..person(feet) };
        w.set_actor_parts(id, v, &parts(feet, &jit[j * PARTS * JIT..(j + 1) * PARTS * JIT]));
    }
    let v = HitVolume { feet: [500.0 + moves[8] * 20.0, -300.0, 0.0], radius: 20.0 + moves[9] * 4.0, height: 128.0, dead: false };
    w.set_actor_volume(VOLUME_ACTOR, v);
}

fn filters() -> Vec<Filter> {
    vec![
        Filter::default(),
        Filter { no_actors: true, ..Default::default() },
        Filter { exclude: vec![Key::Actor(ACTORS[1])], ..Default::default() },
        Filter { exclude_layers: LayerSet::from_layers(&[layers::DEAD_BIP]), ..Default::default() },
        Filter { no_terrain: true, no_dynamic: true, ..Default::default() },
    ]
}

fn rz(a: f32) -> [f32; 9] {
    let (s, c) = a.sin_cos();
    [c, -s, 0.0, s, c, 0.0, 0.0, 0.0, 1.0]
}

fn bits3(v: [f32; 3]) -> [u32; 3] {
    v.map(f32::to_bits)
}

/// A ray hit as bits: `PartialEq` on floats would take -0.0 for 0.0.
type RayBits = (Owner, u32, [u32; 3], [u32; 3], Option<u32>, u32);

fn ray_bits(h: &RayHit) -> RayBits {
    (h.owner, h.distance.to_bits(), bits3(h.point), bits3(h.normal), h.material, h.shell.to_bits())
}

fn sweep_bits(h: &SweepHit) -> (Owner, u32, [u32; 3], [u32; 3]) {
    (h.owner, h.fraction.to_bits(), bits3(h.point), bits3(h.normal))
}

/// Both query paths on one ray, bit for bit.
fn same_ray(qa: &Queries, qb: &Queries, o: [f32; 3], d: [f32; 3], max: f32, f: &Filter) {
    assert_eq!(qa.ray(o, d, max, f).as_ref().map(ray_bits), qb.ray(o, d, max, f).as_ref().map(ray_bits), "ray {o:?} {d:?} {max} {f:?}");
    let all = |q: &Queries| q.ray_all(o, d, max, f).iter().map(ray_bits).collect::<Vec<_>>();
    assert_eq!(all(qa), all(qb), "ray_all {o:?} {d:?} {max} {f:?}");
}

/// Both query paths on one sweep, bit for bit.
fn same_sweep(qa: &Queries, qb: &Queries, shape: &Shape, from: &Iso, motion: [f32; 3], f: &Filter) {
    let sweep = |q: &Queries| q.sweep(shape, from, motion, f).unwrap().as_ref().map(sweep_bits);
    assert_eq!(sweep(qa), sweep(qb), "sweep {shape:?} {from:?} {motion:?} {f:?}");
}

/// Every query kind, from probes in -1..1 (four per probe), on two worlds: equal results,
/// bit for bit. The probes start outside every collider and the scene keeps colliders
/// apart, so no probe meets an exact tie (whose winner the two paths may choose
/// differently: `exact_ties_follow_the_documented_order`).
fn same_queries(a: &mut PhysWorld, b: &mut PhysWorld, probes: &[f32]) {
    let ball = Shape::Sphere { radius: 6.0 };
    let brick = Shape::Box { half_extents: [7.0, 4.0, 3.0] };
    let pill = Shape::Capsule { a: [-5.0, 0.0, 0.0], b: [5.0, 0.0, 0.0], radius: 4.0 };
    let fs = filters();
    let (qa, qb) = (a.queries(), b.queries());
    for (n, p) in probes.as_chunks::<4>().0.iter().enumerate() {
        let (u, v, s, t) = (p[0], p[1], p[2], p[3]);
        // From above, and level from the west (through the actors).
        let rays = [([u * 800.0, v * 700.0, 320.0], [s * 0.6, t * 0.6, -1.0]), ([-900.0, v * 700.0, 20.0 + (u + 1.0) * 70.0], [1.0, s * 0.3, t * 0.1])];
        for f in &fs {
            for (o, d) in rays {
                same_ray(&qa, &qb, o, d, 2_000.0, f);
            }
            let motion = [s * 150.0, t * 150.0, -400.0];
            for (shape, turn) in [(&ball, 0.0), (&brick, u * 3.0), (&pill, v * 3.0)] {
                let from = Iso { translation: [u * 800.0, v * 700.0, 320.0], rotation: rz(turn) };
                same_sweep(&qa, &qb, shape, &from, motion, f);
            }
            // Level sweeps through the actors.
            let from = Iso { translation: [-900.0, v * 700.0, 20.0 + (u + 1.0) * 70.0], rotation: rz(t) };
            same_sweep(&qa, &qb, &brick, &from, [1_700.0, s * 300.0, 0.0], f);
            let blob = Shape::Sphere { radius: 20.0 + (t + 1.0) * 30.0 };
            let centre = Iso { translation: [u * 800.0, v * 700.0, (s + 1.0) * 80.0], ..Iso::IDENTITY };
            assert_eq!(qa.overlap(&blob, &centre, f).unwrap(), qb.overlap(&blob, &centre, f).unwrap(), "overlap {n} {f:?}");
            let slab = Iso { translation: centre.translation, rotation: rz(s) };
            assert_eq!(qa.overlap(&brick, &slab, f).unwrap(), qb.overlap(&brick, &slab, f).unwrap(), "overlap box {n} {f:?}");
        }
    }
    for id in ACTORS.iter().copied().chain([VOLUME_ACTOR]) {
        let centres = |w: &PhysWorld| w.actor_part_centers(id).into_iter().map(bits3).collect::<Vec<_>>();
        assert_eq!(centres(a), centres(b), "actor {id:#x}");
        assert_eq!(a.actor_aim_point(id).map(bits3), b.actor_aim_point(id).map(bits3), "actor {id:#x}");
    }
}

/// What the next step reads, equal bit for bit: every body's pose, velocities and sleep,
/// every contact pair with its manifolds, depths and impulses (`PhysWorld::sim_digest`),
/// and the readbacks.
fn same_state(a: &PhysWorld, b: &PhysWorld) {
    assert_eq!(a.sim_digest(), b.sim_digest());
    assert_eq!(a.state_digest(), b.state_digest());
    for id in CLUTTER.chain([DOOR]) {
        assert_eq!(a.ref_bodies(id), b.ref_bodies(id), "ref {id:#x}");
        assert_eq!(a.ref_contacts(id), b.ref_contacts(id), "ref {id:#x}");
    }
    for id in ACTORS.iter().copied().chain([VOLUME_ACTOR]) {
        assert_eq!(a.actor_volume_state(id), b.actor_volume_state(id), "actor {id:#x}");
    }
}

/// Deterministic values in -1..1.
fn noise(seed: u32, n: usize) -> Vec<f32> {
    let mut x = seed.wrapping_mul(2_654_435_761).wrapping_add(1);
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            (x as f32 / u32::MAX as f32) * 2.0 - 1.0
        })
        .collect()
}

fn posed(seed: u32) -> PhysWorld {
    let mut w = base();
    pose(&mut w, &noise(seed, ACTORS.len() * PARTS * JIT), &noise(seed + 1, 10), &[false, true, false, true]);
    assert!(w.pending_refresh().is_within(DirtyReasons::ACTOR_POSES), "{}", w.pending_refresh());
    w
}

proptest::proptest! {
    #![proptest_config(proptest::prelude::ProptestConfig::with_cases(48))]
    #[test]
    fn tie_free_queries_match_the_full_pass_and_steps_match_an_unqueried_copy(
        jit in proptest::collection::vec(-1.0f32..1.0, ACTORS.len() * PARTS * JIT),
        moves in proptest::collection::vec(-1.0f32..1.0, 10),
        dead in proptest::collection::vec(proptest::bool::ANY, 4),
        probes in proptest::collection::vec(-1.0f32..1.0, 24 * 4),
    ) {
        let mut w = base();
        pose(&mut w, &jit, &moves, &dead);
        proptest::prop_assert!(w.pending_refresh().is_within(DirtyReasons::ACTOR_POSES));
        let mut full = w.snapshot();
        let mut never = w.snapshot();
        let (fast0, full0) = (w.refresh_stats().fast, full.refresh_stats().full);
        w.update_queries();
        full.update_queries_full();
        proptest::prop_assert_eq!(w.refresh_stats().fast, fast0 + 1);
        proptest::prop_assert_eq!(full.refresh_stats().full, full0 + 1);
        same_queries(&mut w, &mut full, &probes);
        // The query changed nothing the step reads: the same as a world never queried.
        for _ in 0..3 {
            w.step();
            never.step();
            same_state(&w, &never);
        }
    }
}

#[test]
fn a_second_pose_before_the_step_replaces_the_colliders_kept_aside() {
    let mut w = posed(7);
    let mut full = w.snapshot();
    w.update_queries();
    full.update_queries_full();
    // Posed again (the same actors), refreshed again: no collider is tested twice.
    let (jit, moves) = (noise(9, ACTORS.len() * PARTS * JIT), noise(10, 10));
    pose(&mut w, &jit, &moves, &[true, false, false, false]);
    pose(&mut full, &jit, &moves, &[true, false, false, false]);
    w.update_queries();
    full.update_queries_full();
    assert_eq!(w.refresh_stats().fast, 2);
    same_queries(&mut w, &mut full, &noise(11, 32 * 4));
    w.step();
    full.step();
    same_queries(&mut w, &mut full, &noise(12, 16 * 4));
}

type Change = Box<dyn Fn(&mut PhysWorld)>;

#[test]
fn mixed_changes_take_the_full_pass_and_query_like_it() {
    let node = Iso { translation: [0.0, 0.0, 0.0], rotation: [0.0, -1.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0] };
    let changes: Vec<(&str, Change)> = vec![
        ("door", Box::new(move |w: &mut PhysWorld| assert!(w.pose_ref_object(&door_pose(), 0, &node, &Iso::IDENTITY)))),
        ("door solidity", Box::new(|w: &mut PhysWorld| assert_eq!(w.set_ref_keyframed_solid(DOOR, false), 1))),
        (
            "reference",
            Box::new(|w: &mut PhysWorld| {
                w.add_ref(&at(0x70, [100.0, 300.0, 40.0]), &[object(Shape::Sphere { radius: 30.0 }, Motion::Static)]);
            }),
        ),
        (
            "part list",
            Box::new(|w: &mut PhysWorld| {
                let feet = actor_feet(2);
                w.set_actor_parts(ACTORS[2], person(feet), &parts(feet, &noise(3, PARTS * JIT))[..2]);
            }),
        ),
        ("removed actor", Box::new(|w: &mut PhysWorld| assert!(w.remove_actor_volume(ACTORS[3])))),
        ("new actor", Box::new(|w: &mut PhysWorld| w.set_actor_volume(0x300, person([0.0, 400.0, 0.0])))),
    ];
    for (what, change) in &changes {
        // Changed before any refresh, and after a refresh that skipped the pass.
        for refreshed_first in [false, true] {
            let mut w = posed(21);
            let mut full = w.snapshot();
            if refreshed_first {
                w.update_queries();
                full.update_queries_full();
                assert_eq!(w.refresh_stats().fast, 1, "{what}");
            }
            change(&mut w);
            change(&mut full);
            let before = w.refresh_stats().full;
            w.update_queries();
            full.update_queries_full();
            assert_eq!(w.refresh_stats().full, before + 1, "{what}");
            assert_eq!(w.refresh_stats().fast, refreshed_first as u64, "{what}");
            same_queries(&mut w, &mut full, &noise(22, 24 * 4));
            w.step();
            full.step();
            same_queries(&mut w, &mut full, &noise(23, 8 * 4));
        }
    }
}

/// An actor's gore pieces (and ragdoll) report the actor's own FormID: refreshing its
/// hit capsules without a pass must not hide them from the queries.
#[test]
fn gore_of_an_actor_refreshed_without_a_pass_is_still_hit() {
    use fnv_phys::gore::{GoreStart, GORE_PART};
    let mut w = posed(31);
    let id = ACTORS[0];
    let piece = GoreStart {
        id: 1,
        actor: id,
        translation: [0.0, 400.0, 200.0],
        rotation: ID,
        half_extents: [10.0, 10.0, 10.0],
        center: [0.0; 3],
        linvel: [0.0; 3],
        angvel: [0.0; 3],
        mass: 5.0,
    };
    w.add_gore(&piece);
    w.step();
    pose(&mut w, &noise(32, ACTORS.len() * PARTS * JIT), &noise(33, 10), &[true; 4]);
    let mut full = w.snapshot();
    w.update_queries();
    full.update_queries_full();
    assert_eq!(w.refresh_stats().fast, 1);
    let down = |w: &mut PhysWorld| w.queries().ray([0.0, 400.0, 320.0], [0.0, 0.0, -1.0], 1_000.0, &Filter::default());
    let hit = down(&mut w).expect("hits the piece");
    assert_eq!(hit.owner, Owner::Actor { form_id: id, layer: layers::DEAD_BIP, part: GORE_PART });
    assert_eq!(Some(hit), down(&mut full));
    same_queries(&mut w, &mut full, &noise(34, 16 * 4));
}

/// The reference pass runs whatever changed: after an unchanged pose (which asks for no
/// refresh any more) and after a refresh that skipped the pass, the queries equal those
/// after a mandatory pass, as the pass ran on any change before.
#[test]
fn the_reference_pass_always_runs_and_queries_match_it() {
    let mut w = posed(41);
    w.step();
    // The same pose again: nothing pending, no refresh.
    pose(&mut w, &noise(41, ACTORS.len() * PARTS * JIT), &noise(42, 10), &[false, true, false, true]);
    assert!(w.pending_refresh().is_empty());
    let mut full = w.snapshot();
    let (fast0, full0) = (w.refresh_stats().fast, w.refresh_stats().full);
    w.update_queries();
    assert_eq!((w.refresh_stats().fast, w.refresh_stats().full), (fast0, full0));
    full.update_queries_full();
    assert_eq!(full.refresh_stats().full, full0 + 1);
    same_queries(&mut w, &mut full, &noise(43, 16 * 4));

    // After a refresh that skipped the pass, the reference still runs one.
    pose(&mut w, &noise(44, ACTORS.len() * PARTS * JIT), &noise(45, 10), &[false; 4]);
    w.update_queries();
    assert_eq!(w.refresh_stats().fast, fast0 + 1);
    let mut full = w.snapshot();
    full.update_queries_full();
    assert_eq!(full.refresh_stats().full, w.refresh_stats().full + 1);
    same_queries(&mut w, &mut full, &noise(46, 16 * 4));
}

/// The documented tie rule of the refresh without a pass (`fnv_phys::refresh`): at
/// exactly the same distance, a broad-phase hit beats a kept-aside collider, and among
/// kept-aside colliders the lowest FormID (then the earliest capsule) wins. A full pass
/// may pick either; this pins the fast path only. Two actors with the same capsules in the
/// same place tie for every ray and sweep, from outside and from inside (distance 0).
#[test]
fn exact_ties_follow_the_documented_order() {
    let mut w = ground();
    let one = |x: f32| vec![PartCapsule { part: 3, a: [x, 0.0, 40.0], b: [x, 0.0, 100.0], radius: 15.0 }];
    let at_x = |x: f32| HitVolume { feet: [x, 0.0, 0.0], radius: 20.0, height: 128.0, dead: false };
    let (low, high) = (0x10u32, 0x20u32);
    for id in [high, low] {
        w.set_actor_parts(id, at_x(200.0), &one(200.0));
    }
    w.step();
    let ball = Shape::Sphere { radius: 5.0 };
    // Rays from outside and from inside both capsules; a sweep from inside both.
    let probes = |w: &mut PhysWorld, x: f32| -> Vec<(Option<Owner>, u32)> {
        let q = w.queries();
        let f = Filter::default();
        let outside = q.ray([x - 100.0, 0.0, 70.0], [1.0, 0.0, 0.0], 1_000.0, &f).map(|h| (h.owner, h.distance.to_bits()));
        let inside = q.ray([x, 0.0, 70.0], [1.0, 0.0, 0.0], 1_000.0, &f).map(|h| (h.owner, h.distance.to_bits()));
        let from = Iso { translation: [x, 0.0, 70.0], ..Iso::IDENTITY };
        let sweep = q.sweep(&ball, &from, [50.0, 0.0, 0.0], &f).unwrap().map(|h| (h.owner, h.fraction.to_bits()));
        [outside, inside, sweep].into_iter().map(|p| (p.map(|p| p.0), p.map_or(u32::MAX, |p| p.1))).collect()
    };
    let owner = |id: u32| Some(Owner::Actor { form_id: id, layer: layers::BIPED, part: 3 });
    // The two really tie: each alone is met at the same distance, bit for bit.
    let alone = |w: &mut PhysWorld, x: f32, id: u32| {
        let other = if id == low { high } else { low };
        let f = Filter { exclude: vec![Key::Actor(other)], ..Default::default() };
        w.queries().ray([x - 100.0, 0.0, 70.0], [1.0, 0.0, 0.0], 1_000.0, &f).map(|h| h.distance.to_bits())
    };

    // Both kept aside: the lower FormID wins, whichever was posed first.
    for id in [high, low] {
        w.set_actor_parts(id, at_x(260.0), &one(260.0));
    }
    w.update_queries();
    assert_eq!(w.refresh_stats().fast, 1);
    assert_eq!(alone(&mut w, 260.0, low), alone(&mut w, 260.0, high));
    let got = probes(&mut w, 260.0);
    assert_eq!(got.iter().map(|p| p.0).collect::<Vec<_>>(), vec![owner(low); 3], "{got:?}");
    assert_eq!((got[1].1, got[2].1), (0.0f32.to_bits(), 0.0f32.to_bits()), "inside: distance 0");

    // One in the broad phase (placed by a full pass), one kept aside: the broad-phase hit
    // wins.
    for id in [high, low] {
        w.set_actor_parts(id, at_x(320.0), &one(320.0));
    }
    w.update_queries_full();
    w.set_actor_parts(low, at_x(330.0), &one(330.0));
    w.update_queries();
    w.set_actor_parts(low, at_x(320.0), &one(320.0));
    w.update_queries();
    assert_eq!(w.refresh_stats().fast, 3);
    assert_eq!(alone(&mut w, 320.0, low), alone(&mut w, 320.0, high));
    let got = probes(&mut w, 320.0);
    assert_eq!(got.iter().map(|p| p.0).collect::<Vec<_>>(), vec![owner(high); 3], "{got:?}");
}

/// Rays at the edges: ending exactly at a capsule (a hit at the maximum distance does not
/// count, in either path), grazing a capsule's side and cap, and an actor far from the
/// origin.
#[test]
fn edge_rays_match_the_full_pass() {
    let mut w = ground();
    let far = [30_000.0, -30_000.0, 500.0];
    let one = |p: [f32; 3]| vec![PartCapsule { part: 0, a: [p[0], p[1], p[2] + 40.0], b: [p[0], p[1], p[2] + 100.0], radius: 15.0 }];
    let vol = |p: [f32; 3]| HitVolume { feet: p, radius: 20.0, height: 128.0, dead: false };
    w.set_actor_parts(0x10, vol([200.0, 0.0, 0.0]), &one([200.0, 0.0, 0.0]));
    w.set_actor_parts(0x11, vol(far), &one(far));
    w.step();
    w.set_actor_parts(0x10, vol([210.0, 0.0, 0.0]), &one([210.0, 0.0, 0.0]));
    let far2 = [far[0] + 10.0, far[1], far[2]];
    w.set_actor_parts(0x11, vol(far2), &one(far2));
    let mut full = w.snapshot();
    w.update_queries();
    full.update_queries_full();
    assert_eq!(w.refresh_stats().fast, 1);
    let f = Filter::default();
    let (qa, qb) = (w.queries(), full.queries());
    for (x, base) in [(210.0f32, [0.0f32, 0.0, 0.0]), (far2[0], far2)] {
        let o = [x - 300.0, base[1], base[2] + 70.0];
        let d = [1.0, 0.0, 0.0];
        let hit = qa.ray(o, d, 1_000.0, &f).expect("hits the capsule");
        // Maximum distances a few ulps either side of the hit.
        for i in -4i32..=4 {
            let max = f32::from_bits((hit.distance.to_bits() as i32 + i) as u32);
            same_ray(&qa, &qb, o, d, max, &f);
        }
        // Grazing the side (radius 15 off the axis) and the top cap, and an ulp either side.
        for off in [15.0f32, 14.999999, 15.000001, -15.0] {
            same_ray(&qa, &qb, [x - 300.0, base[1] + off, base[2] + 70.0], d, 1_000.0, &f);
        }
        for top in [base[2] + 115.0, base[2] + 114.99999, base[2] + 115.00001] {
            same_ray(&qa, &qb, [x - 300.0, base[1], top], d, 1_000.0, &f);
        }
        same_ray(&qa, &qb, [x, base[1] + 300.0, base[2] + 70.0], [0.0, -1.0, 0.0], 1_000.0, &f);
    }
}

/// The character controller rejects actor hit volumes (`solid_for`), so a refresh without
/// a pass changes none of its results: the same step and the same character, bit for bit,
/// for several inputs.
#[test]
fn the_character_controller_moves_alike_after_either_refresh() {
    let mut w = posed(51);
    w.update_queries_full();
    let start = w.add_character(9, CharacterParams::default(), [-450.0, -150.0, 0.0]);
    w.step();
    pose(&mut w, &noise(52, ACTORS.len() * PARTS * JIT), &noise(53, 10), &[false; 4]);
    let mut full = w.snapshot();
    w.update_queries();
    full.update_queries_full();
    assert_eq!(w.refresh_stats().fast, 1);
    for (i, velocity) in [[0.0, 300.0], [250.0, 250.0], [-200.0, 100.0], [0.0, 0.0]].into_iter().enumerate() {
        let input = CharacterInput { velocity, jump: i == 3, ..Default::default() };
        let (mut a, mut b) = (w.snapshot(), full.snapshot());
        let (mut ca, mut cb) = (start.clone(), start.clone());
        let (sa, sb) = (a.move_character(&mut ca, input), b.move_character(&mut cb, input));
        // Debug prints every float so that it reads back to the same bits (-0.0 included).
        assert_eq!(format!("{sa:?}"), format!("{sb:?}"), "input {i}");
        assert_eq!(format!("{ca:?}"), format!("{cb:?}"), "input {i}");
    }
}

/// A copy made after a refresh that skipped the pass keeps what it kept aside: it queries
/// like the original and the full pass, and steps like the original.
#[test]
fn a_copy_after_a_refresh_without_a_pass_queries_and_steps_alike() {
    let mut w = posed(61);
    let mut full = w.snapshot();
    w.update_queries();
    full.update_queries_full();
    let mut copy = w.snapshot();
    assert_eq!(copy.refresh_stats().fast, 1);
    same_queries(&mut copy, &mut full, &noise(62, 16 * 4));
    same_queries(&mut copy, &mut w, &noise(63, 8 * 4));
    for _ in 0..3 {
        w.step();
        copy.step();
        same_state(&w, &copy);
    }
}

/// One actor reposed among several: the others keep their broad-phase leaves.
#[test]
fn one_actor_reposed_among_several_queries_like_the_full_pass() {
    let mut w = posed(71);
    w.step();
    let j = 2;
    let f = actor_feet(j);
    let feet = [f[0] + 4.0, f[1] - 3.0, f[2]];
    w.set_actor_parts(ACTORS[j], person(feet), &parts(feet, &noise(72, PARTS * JIT)));
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS);
    let mut full = w.snapshot();
    let mut never = w.snapshot();
    w.update_queries();
    full.update_queries_full();
    assert_eq!(w.refresh_stats().fast, 1);
    same_queries(&mut w, &mut full, &noise(73, 24 * 4));
    w.step();
    never.step();
    same_state(&w, &never);
}

/// A tiny ragdoll: one capsule body.
fn stick() -> std::sync::Arc<fnv_collide::ragdoll::Ragdoll> {
    use fnv_collide::ragdoll::{Ragdoll, RagdollBody};
    let mut object = object(Shape::Capsule { a: [0.0, 0.0, -10.0], b: [0.0, 0.0, 10.0], radius: 5.0 }, Motion::Keyframed);
    object.node = Some("stick".into());
    object.layer = layers::BIPED;
    let rest = Iso { translation: [0.0, 0.0, 0.0], rotation: ID };
    object.node_transform = rest;
    let body = RagdollBody { object, linear_damping: 0.1, angular_damping: 0.05, stored: rest, unit: 7.0 };
    std::sync::Arc::new(Ragdoll { bodies: vec![body], joints: vec![], problems: vec![] })
}

/// Every other reason, alongside a moved actor, takes the full pass, and that pass is
/// counted under a combination that names it.
#[test]
fn each_other_reason_forces_the_full_pass() {
    let rag = stick();
    let rag_nodes = [Iso { translation: [0.0, 500.0, 100.0], rotation: ID }];
    let cases: Vec<(DirtyReasons, Change)> = vec![
        (DirtyReasons::BODIES, Box::new(|w: &mut PhysWorld| assert!(w.remove_ref(0x41)))),
        (DirtyReasons::DOORS, Box::new(|w: &mut PhysWorld| assert_eq!(w.set_ref_keyframed_solid(DOOR, false), 1))),
        (DirtyReasons::CHARACTERS, Box::new(|w: &mut PhysWorld| {
            w.add_character(7, CharacterParams::default(), [0.0, 500.0, 0.0]);
        })),
        (DirtyReasons::RAGDOLLS, Box::new(move |w: &mut PhysWorld| {
            w.add_ragdoll(&RagdollStart { form_id: 0x400, ragdoll: &rag, scale: 1.0, nodes: &rag_nodes, velocity: [0.0; 3], parts: &[], asleep: false });
        })),
        (DirtyReasons::GORE, Box::new(|w: &mut PhysWorld| {
            w.add_gore(&fnv_phys::gore::GoreStart {
                id: 3,
                actor: ACTORS[0],
                translation: [0.0, 450.0, 150.0],
                rotation: ID,
                half_extents: [5.0; 3],
                center: [0.0; 3],
                linvel: [0.0; 3],
                angvel: [0.0; 3],
                mass: 2.0,
            })
        })),
        (DirtyReasons::TERRAIN, Box::new(|w: &mut PhysWorld| w.add_terrain((1, 0), &vec![5.0; SIDE * SIDE]))),
        (DirtyReasons::ACTOR_REBUILT, Box::new(|w: &mut PhysWorld| w.set_actor_volume(0x300, person([0.0, 400.0, 0.0])))),
    ];
    for (reason, change) in &cases {
        let mut w = posed(81);
        let mut full = w.snapshot();
        change(&mut w);
        change(&mut full);
        assert!(w.pending_refresh().contains(*reason), "{reason}: {}", w.pending_refresh());
        let (fast0, full0) = (w.refresh_stats().fast, w.refresh_stats().full);
        w.update_queries();
        full.update_queries_full();
        assert_eq!((w.refresh_stats().fast, w.refresh_stats().full), (fast0, full0 + 1), "{reason}");
        assert!(w.refresh_stats().by_reasons.keys().any(|k| k.contains(*reason | DirtyReasons::ACTOR_PARTS)), "{reason}");
        same_queries(&mut w, &mut full, &noise(82, 8 * 4));
    }

    // A teleport (PLACED) needs a character that exists already.
    let mut w = posed(83);
    w.update_queries_full();
    let mut c = w.add_character(8, CharacterParams::default(), [0.0, 500.0, 0.0]);
    w.step();
    pose(&mut w, &noise(84, ACTORS.len() * PARTS * JIT), &noise(85, 10), &[false; 4]);
    w.place_character(&mut c, [100.0, 500.0, 0.0]);
    assert_eq!(w.pending_refresh(), DirtyReasons::ACTOR_PARTS | DirtyReasons::ACTOR_VOLUME | DirtyReasons::PLACED);
    let fast0 = w.refresh_stats().fast;
    w.update_queries();
    assert_eq!(w.refresh_stats().fast, fast0);

    // Clutter added asleep (SLEEP) is found its contacts by a full pass before the step.
    let mut w = posed(86);
    let p = PreparedRef::new(&w.params, &at(0x71, [0.0, 450.0, 8.0]), &[object(Shape::Box { half_extents: [8.0; 3] }, Motion::Dynamic)]).asleep();
    w.insert_ref(p);
    w.step();
    assert!(w.refresh_stats().by_reasons.keys().any(|k| k.contains(DirtyReasons::SLEEP | DirtyReasons::ACTOR_PARTS)));
    assert_eq!(w.refresh_stats().fast, 0);
}
