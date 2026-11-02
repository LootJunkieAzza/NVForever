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

/// Every query kind, from probes in -1..1 (four per probe), on two worlds: equal results,
/// bit for bit.
fn same_queries(a: &mut PhysWorld, b: &mut PhysWorld, probes: &[f32]) {
    let ball = Shape::Sphere { radius: 6.0 };
    let fs = filters();
    let (qa, qb) = (a.queries(), b.queries());
    for (n, p) in probes.chunks_exact(4).enumerate() {
        let (u, v, s, t) = (p[0], p[1], p[2], p[3]);
        // From above, and level from the west (through the actors).
        let rays = [([u * 800.0, v * 700.0, 320.0], [s * 0.6, t * 0.6, -1.0]), ([-900.0, v * 700.0, 20.0 + (u + 1.0) * 70.0], [1.0, s * 0.3, t * 0.1])];
        for f in &fs {
            for (o, d) in rays {
                assert_eq!(qa.ray(o, d, 2_000.0, f), qb.ray(o, d, 2_000.0, f), "ray {n} {o:?} {d:?} {f:?}");
                assert_eq!(qa.ray_all(o, d, 2_000.0, f), qb.ray_all(o, d, 2_000.0, f), "ray_all {n} {o:?} {d:?} {f:?}");
            }
            let from = Iso { translation: [u * 800.0, v * 700.0, 320.0], ..Iso::IDENTITY };
            let motion = [s * 150.0, t * 150.0, -400.0];
            assert_eq!(qa.sweep(&ball, &from, motion, f).unwrap(), qb.sweep(&ball, &from, motion, f).unwrap(), "sweep {n} {f:?}");
            let blob = Shape::Sphere { radius: 20.0 + (t + 1.0) * 30.0 };
            let centre = Iso { translation: [u * 800.0, v * 700.0, (s + 1.0) * 80.0], ..Iso::IDENTITY };
            assert_eq!(qa.overlap(&blob, &centre, f).unwrap(), qb.overlap(&blob, &centre, f).unwrap(), "overlap {n} {f:?}");
        }
    }
    for id in ACTORS.iter().copied().chain([VOLUME_ACTOR]) {
        assert_eq!(a.actor_part_centers(id), b.actor_part_centers(id), "actor {id:#x}");
        assert_eq!(a.actor_aim_point(id), b.actor_aim_point(id), "actor {id:#x}");
    }
}

/// Bodies, sleep and contacts equal, bit for bit.
fn same_state(a: &PhysWorld, b: &PhysWorld) {
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
    fn moved_actor_colliders_refreshed_alone_query_like_a_full_pass(
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
fn anything_besides_moved_actor_colliders_takes_the_full_pass() {
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
