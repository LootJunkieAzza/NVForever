//! rapier's island bookkeeping under everything the game does to bodies (no game data):
//! corpses falling, cut apart, settled, held at rest and roused, struck, gore pieces,
//! clutter added awake and asleep, struck and blasted, animated collision put, queries
//! between steps and rate changes, in random order. The owner's crash of 2026-11-11 (a VATS
//! kill with dismemberment): rapier's `extract_sleeping_sub_island` found a body of another
//! set in the island it put to sleep (`fnv_phys::islands`).

use fnv_collide::ragdoll::{JointKind, Ragdoll, RagdollBody, RagdollJoint};
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::gore::GoreStart;
use fnv_phys::*;
use fnv_world::land::SIDE;
use std::sync::Arc;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];
const X: [f32; 3] = [1.0, 0.0, 0.0];
const Y: [f32; 3] = [0.0, 1.0, 0.0];
const Z: [f32; 3] = [0.0, 0.0, 1.0];

fn ground() -> PhysWorld {
    let mut w = PhysWorld::new(PhysParams::default());
    for x in -1..=0 {
        for y in -1..=0 {
            w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
        }
    }
    w
}

fn iso(p: [f32; 3]) -> Iso {
    Iso { translation: p, rotation: ID }
}

fn object(shape: Shape, motion: Motion, layer: u8, mass: f32) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion,
        layer,
        mass,
        friction: 0.5,
        restitution: 0.0,
        shape: shape.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

fn body(name: &str, shape: Shape, mass: f32, rest: [f32; 3]) -> RagdollBody {
    let mut object = object(shape, Motion::Keyframed, layers::BIPED, mass);
    object.node = Some(name.into());
    object.node_transform = iso(rest);
    RagdollBody { object, linear_damping: 0.1, angular_damping: 0.05, stored: iso(rest), unit: 7.0, inertia: None }
}

fn joint(a: usize, b: usize, pivot_a: [f32; 3], pivot_b: [f32; 3], kind: JointKind) -> RagdollJoint {
    RagdollJoint { block: 0, a, b, pivot_a, pivot_b, kind, max_friction: 100.0, strength: None }
}

/// A seven-body figure: pelvis, chest, head, two legs of two bodies.
fn figure() -> Arc<Ragdoll> {
    let cap = |a: [f32; 3], b: [f32; 3], radius: f32| Shape::Capsule { a, b, radius };
    let ball = |cone: f32, twist: f32| JointKind::Ragdoll { twist_a: Z, plane_a: X, twist_b: Z, plane_b: X, cone_max: cone, plane_min: -cone, plane_max: cone, twist_min: -twist, twist_max: twist };
    let knee = JointKind::Hinge { axis_a: X, perp_a: Y, axis_b: X, perp_b: Y, limits: Some([-0.1, 2.2]) };
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

fn crate_box() -> [CollisionObject; 1] {
    [object(Shape::Box { half_extents: [5.0, 5.0, 5.0] }, Motion::Dynamic, layers::CLUTTER, 2.0)]
}

fn platform() -> [CollisionObject; 1] {
    [object(Shape::Box { half_extents: [100.0, 100.0, 5.0] }, Motion::Keyframed, layers::ANIM_STATIC, 0.0)]
}

fn gore(id: u32, actor: u32, at: [f32; 3], linvel: [f32; 3]) -> GoreStart {
    GoreStart { id, actor, translation: at, rotation: ID, half_extents: [2.0, 1.5, 1.0], center: [0.0; 3], linvel, angvel: [0.0; 3], mass: 0.3 }
}

/// A small deterministic random sequence.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (self.0 >> 33) as u32
    }
    fn below(&mut self, n: u32) -> u32 {
        self.next() % n.max(1)
    }
    fn f(&mut self, lo: f32, hi: f32) -> f32 {
        lo + (hi - lo) * (self.below(10_000) as f32 / 10_000.0)
    }
}

/// One random run of `steps` steps (see the module notes). Returns the first split island
/// found after a step, with the seed and step.
fn random_play(seed: u64, steps: usize) -> Option<String> {
    let mut r = Lcg(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ 0x5DEE_CE66);
    let mut w = ground();
    let rag = figure();
    let movers: Vec<[f32; 3]> = (0..3).map(|i| [i as f32 * 300.0 - 300.0, 300.0, 40.0]).collect();
    for (i, p) in movers.iter().enumerate() {
        w.add_ref(&RefPose { form_id: 10 + i as u32, position: *p, rotation: ID, scale: 1.0 }, &platform());
    }
    let mut offsets = [0.0f32; 3];
    let (mut next_corpse, mut next_loose, mut next_gore) = (0x1000u32, 0x100u32, 0u32);
    // The player walks about between steps, as in play: its moves query the world, which
    // brings the broad phase up to date first when anything changed.
    let mut player = w.add_character(1, CharacterParams::default(), [0.0, -300.0, 4.0]);
    let mut heading = 0.0f32;
    for step in 0..steps {
        // One to three things happen between two steps.
        for _ in 0..1 + r.below(3) {
            let pick = |r: &mut Lcg, base: u32, next: u32| base + r.below(next - base);
            match r.below(40) {
                0 | 1 => {
                    // A corpse falls where things lie, sometimes put back at rest.
                    let origin = [r.f(-400.0, 400.0), r.f(-100.0, 400.0), r.f(0.0, 80.0)];
                    let poses: Vec<Iso> = rag.bodies.iter().map(|b| iso(origin).compose(&b.rest())).collect();
                    let velocity = [r.f(-300.0, 300.0), r.f(-300.0, 300.0), r.f(0.0, 200.0)];
                    let asleep = r.below(6) == 0;
                    w.add_ragdoll(&RagdollStart { form_id: next_corpse, ragdoll: &rag, scale: 1.0, nodes: &poses, velocity, parts: &[], asleep });
                    next_corpse += 1;
                }
                2 if next_corpse > 0x1000 => {
                    // Dismembered: a limb cut off, gore pieces flung from it.
                    let id = pick(&mut r, 0x1000, next_corpse);
                    let limb = [vec![2], vec![3, 4], vec![5, 6], vec![4], vec![1, 2]][r.below(5) as usize].clone();
                    w.cut_ragdoll(id, &limb);
                    if let Some(c) = w.ragdoll_center(id) {
                        for _ in 0..r.below(9) {
                            let at = [c[0] + r.f(-10.0, 10.0), c[1] + r.f(-10.0, 10.0), c[2] + r.f(0.0, 20.0)];
                            let v = [r.f(-300.0, 300.0), r.f(-300.0, 300.0), r.f(0.0, 300.0)];
                            w.add_gore(&gore(next_gore, id, at, v));
                            next_gore += 1;
                        }
                    }
                }
                3 | 4 if next_corpse > 0x1000 => {
                    let id = pick(&mut r, 0x1000, next_corpse);
                    w.settle_ragdoll(id, r.f(0.0, 1.0));
                }
                5 | 6 if next_corpse > 0x1000 => {
                    let id = pick(&mut r, 0x1000, next_corpse);
                    w.set_ragdoll_at_rest(id);
                }
                7 | 8 if next_corpse > 0x1000 => {
                    // A shot or a VATS hit on a corpse.
                    let id = pick(&mut r, 0x1000, next_corpse);
                    if let Some(c) = w.ragdoll_center(id) {
                        w.push_ragdoll(id, c, [r.f(-200.0, 200.0), r.f(-200.0, 200.0), r.f(0.0, 200.0)]);
                    }
                }
                9 if next_corpse > 0x1000 => {
                    let id = pick(&mut r, 0x1000, next_corpse);
                    w.unsettle_ragdoll(id);
                }
                10 if next_corpse > 0x1000 && r.below(4) == 0 => {
                    let id = pick(&mut r, 0x1000, next_corpse);
                    w.remove_ragdoll(id);
                }
                11 if next_gore > 0 => {
                    let id = r.below(next_gore);
                    if r.below(3) == 0 {
                        w.remove_gore(id);
                    } else {
                        w.push_gore(id, [r.f(-100.0, 100.0), 0.0, r.f(0.0, 100.0)]);
                    }
                }
                12..=14 => {
                    // Clutter, awake or placed asleep (a cell loading).
                    let p = [r.f(-400.0, 400.0), r.f(-100.0, 400.0), r.f(5.0, 120.0)];
                    let prepared = PreparedRef::new(&w.params, &RefPose { form_id: next_loose, position: p, rotation: ID, scale: 1.0 }, &crate_box());
                    w.insert_ref(if r.below(2) == 0 { prepared.asleep() } else { prepared });
                    next_loose += 1;
                }
                15 if next_loose > 0x100 => {
                    let id = pick(&mut r, 0x100, next_loose);
                    let _ = w.push_ref(id, [0.0; 3], [r.f(-30.0, 30.0), 0.0, r.f(0.0, 30.0)], 300.0);
                }
                16 if next_loose > 0x100 && r.below(3) == 0 => {
                    let id = pick(&mut r, 0x100, next_loose);
                    w.remove_ref(id);
                }
                17 => {
                    // A blast: wakes and rouses what is near.
                    w.push_dynamic([r.f(-400.0, 400.0), r.f(-100.0, 400.0), 20.0], r.f(50.0, 300.0), r.f(50.0, 400.0));
                }
                18..=21 => {
                    // Animated collision moves on; now and then built again.
                    let i = r.below(3) as usize;
                    let id = 10 + i as u32;
                    let pose = RefPose { form_id: id, position: movers[i], rotation: ID, scale: 1.0 };
                    if r.below(15) == 0 {
                        w.add_ref(&pose, &platform());
                        offsets[i] = 0.0;
                    } else {
                        offsets[i] += r.f(-8.0, 8.0);
                        w.pose_ref_object(&pose, 0, &iso([offsets[i], 0.0, r.f(-1.0, 1.0)]), &Iso::IDENTITY);
                    }
                }
                22..=24 => {
                    // Queries between steps (the refresh and its repairs).
                    let _ = w.queries().ray([r.f(-400.0, 400.0), r.f(-100.0, 400.0), 300.0], [0.0, 0.0, -1.0], 400.0, &Filter::default());
                }
                25 => {
                    // A rate change, or slow motion's shortest step.
                    let dt = [1.0 / 60.0, 1.0 / 120.0, 1.0 / 144.0, 1.0 / 152.0, 1.0 / 160.0, 1.0 / 240.0][r.below(6) as usize];
                    w.set_step(dt);
                }
                _ => {}
            }
        }
        heading += r.f(-0.3, 0.3);
        w.move_character(&mut player, CharacterInput { velocity: [heading.sin() * 150.0, heading.cos() * 150.0], ..Default::default() });
        if player.feet[0].abs() > 450.0 || player.feet[1].abs() > 450.0 {
            heading += std::f32::consts::PI;
        }
        w.step();
        if let Some(split) = w.island_split() {
            return Some(format!("seed {seed}, step {step}: {split}"));
        }
    }
    None
}

#[test]
fn rapiers_islands_hold_under_random_play() {
    // `ISLAND_FUZZ=<first>..<end>` runs other seeds (a debug build also turns rapier's own
    // island checks on, `PersistentIslands::assert_consistent`).
    let seeds = std::env::var("ISLAND_FUZZ").ok().and_then(|v| v.split_once("..").and_then(|(a, b)| Some(a.parse::<u64>().ok()?..b.parse::<u64>().ok()?))).unwrap_or(0..8);
    let steps = std::env::var("ISLAND_FUZZ_STEPS").ok().and_then(|v| v.parse().ok()).unwrap_or(1200);
    for seed in seeds {
        if let Some(split) = random_play(seed, steps) {
            panic!("{split}");
        }
    }
}

/// The owner's crash, step by step: a corpse asleep is struck (`push_ragdoll`, a VATS shot)
/// and a query runs before the next step. rapier's collision-only update took the struck
/// bone's wake-up without telling its islands: the bone was awake but left in its sleeping
/// set. Pieces that land on it join it to the awake set's island, and when that island fell
/// asleep rapier found the bone in another set and panicked.
#[test]
fn a_sleeping_corpse_struck_before_a_query_wakes_whole() {
    for dt in [1.0 / 60.0, 1.0 / 152.0] {
        let mut w = ground();
        w.set_step(dt);
        let rag = figure();
        let poses: Vec<Iso> = rag.bodies.iter().map(|b| iso([0.0, 0.0, 2.0]).compose(&b.rest())).collect();
        w.add_ragdoll(&RagdollStart { form_id: 0x1000, ragdoll: &rag, scale: 1.0, nodes: &poses, velocity: [0.0; 3], parts: &[], asleep: false });
        for _ in 0..3000 {
            w.step();
            if w.ragdoll_asleep(0x1000) {
                break;
            }
        }
        assert!(w.ragdoll_asleep(0x1000), "the corpse fell asleep");
        let c = w.ragdoll_center(0x1000).unwrap();
        w.push_ragdoll(0x1000, c, [0.0, 0.0, 1.0]);
        // Something else changed (a body added elsewhere), so the query refreshes first.
        w.insert_ref(PreparedRef::new(&w.params, &RefPose { form_id: 0x100, position: [300.0, 300.0, 6.0], rotation: ID, scale: 1.0 }, &crate_box()));
        let _ = w.queries().ray([c[0], c[1], 200.0], [0.0, 0.0, -1.0], 400.0, &Filter::default());
        assert_eq!(w.island_split(), None, "after the query");
        let before = w.ragdoll_center(0x1000).unwrap();
        w.step();
        w.step();
        let after = w.ragdoll_center(0x1000).unwrap();
        assert!(after != before, "the struck corpse is simulated again: {before:?} -> {after:?}");
        // A crate and gore pieces land on it and come to rest.
        let lid = [object(Shape::Box { half_extents: [40.0, 40.0, 3.0] }, Motion::Dynamic, layers::CLUTTER, 5.0)];
        w.insert_ref(PreparedRef::new(&w.params, &RefPose { form_id: 0x101, position: [c[0], c[1], c[2] + 60.0], rotation: ID, scale: 1.0 }, &lid));
        for i in 0..8 {
            w.add_gore(&gore(i, 0x1000, [c[0] + i as f32 * 2.0 - 8.0, c[1], c[2] + 30.0], [0.0, 0.0, -50.0]));
        }
        for i in 0..(20.0 / dt) as usize {
            w.step();
            if let Some(split) = w.island_split() {
                panic!("dt {dt}, step {i}: {split}");
            }
        }
    }
}
