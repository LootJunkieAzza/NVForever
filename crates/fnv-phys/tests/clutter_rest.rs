//! Loose clutter comes to rest where it lies and falls asleep, at every physics rate
//! (docs/spec-notes/physics.md §5.5). Hand-built worlds, no game data.

use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use fnv_phys::*;

const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

/// The physics rates the game offers (60, 120, 144, 160 Hz; Match FPS lies between).
const RATES: [f32; 2] = [60.0, 160.0];

fn object(shape: Shape, motion: Motion, mass: f32) -> CollisionObject {
    CollisionObject {
        node: None,
        node_block: 0,
        node_transform: Iso::IDENTITY,
        body_transform: Iso::IDENTITY,
        motion,
        layer: if motion == Motion::Static { layers::STATIC } else { layers::CLUTTER },
        mass,
        friction: 0.5,
        restitution: 0.4,
        shape: shape.into(),
        shell: 0.0,
        material: None,
        wind: false,
    }
}

fn at(form_id: u32, position: [f32; 3]) -> RefPose {
    RefPose { form_id, position, rotation: ID, scale: 1.0 }
}

fn dist(a: &[f32; 3], b: &[f32; 3]) -> f32 {
    (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>().sqrt()
}

/// Steps `w` for `secs` at `hz`; returns, for `form_id`'s body, the most it moved between
/// two steps over the last second and whether it sleeps at the end.
fn settle(w: &mut PhysWorld, hz: f32, secs: f32, form_id: u32) -> (f32, bool) {
    w.set_step(1.0 / hz);
    let n = (secs * hz).round() as usize;
    let mut last: Vec<[f32; 3]> = Vec::new();
    for _ in 0..n {
        w.step();
        last.push(w.ref_bodies(form_id)[0].translation);
        if last.len() > hz as usize + 1 {
            last.remove(0);
        }
    }
    let step_max = last.windows(2).map(|p| dist(&p[0], &p[1])).fold(0.0, f32::max);
    (step_max, w.ref_bodies(form_id)[0].sleeping)
}

/// A shelf board as a triangle mesh, 60 units wide: its top at z = 0 for y below 1, and
/// past a riser a lip 1.7 units lower (a moulding along the board's front).
fn shelf(w: &mut PhysWorld) {
    let mut v = Vec::new();
    let mut t = Vec::new();
    let mut quad = |q: [[f32; 3]; 4]| {
        let i = v.len() as u32;
        v.extend(q);
        t.extend([[i, i + 1, i + 2], [i, i + 2, i + 3]]);
    };
    let (lo, x) = (-1.7, 30.0);
    quad([[-x, -20.0, 0.0], [x, -20.0, 0.0], [x, 1.0, 0.0], [-x, 1.0, 0.0]]);
    quad([[-x, 1.0, 0.0], [x, 1.0, 0.0], [x, 1.0, lo], [-x, 1.0, lo]]);
    quad([[-x, 1.0, lo], [x, 1.0, lo], [x, 20.0, lo], [-x, 20.0, lo]]);
    let mesh = Shape::TriMesh { vertices: v, triangles: t, materials: vec![] };
    let r = w.add_ref(&at(0x100, [0.0; 3]), &[object(mesh, Motion::Static, 0.0)]);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

/// Indoors (no terrain) the contact prediction distance is rapier's own (1.4 units): the
/// lip, 1.7 units below a book's overhang, is out of it, so the book lies on the points
/// it lies on and falls asleep. With the distance raised by the terrain's 3.5-unit shell,
/// as indoors before 2026-11-13, rapier's four contact points took in the lip's and the
/// book trembled awake [observed: `NVF_PHYS_INDOOR_PREDICTION=0`, awake after 3 s, moving
/// up to 0.16 units a step at 60 Hz and 0.09 at 160; Doc Mitchell's books did the same on
/// their shelves, `fnvtool phys cell`, physics.md §5.5].
#[test]
fn a_book_over_a_lower_lip_falls_asleep_indoors() {
    for hz in RATES {
        let mut w = PhysWorld::new(PhysParams::default());
        shelf(&mut w);
        // 14 by 20 by 3 units, 1 kg, 5 units of it over the lip.
        let book = object(Shape::Box { half_extents: [7.0, 10.0, 1.5] }, Motion::Dynamic, 1.0);
        w.add_ref(&at(0x200, [0.0, -4.0, 1.5]), &[book]);
        let (step_max, asleep) = settle(&mut w, hz, 3.0, 0x200);
        assert!(asleep, "{hz} Hz: awake after 3 s, moving up to {step_max} a step");
        let (step_max, asleep) = settle(&mut w, hz, 5.0, 0x200);
        assert!(asleep && step_max == 0.0, "{hz} Hz, 5 s on: asleep {asleep}, up to {step_max} a step");
    }
}

/// A table 100 by 70 by 40 units as a convex hull, its top (z = 40) a face of 11 corners
/// (a rectangle with its corners cut, plus three along one edge), like the Goodsprings
/// coffee table `SubCoffeeTableDirty01`.
fn table(w: &mut PhysWorld) {
    let top = [(-50.0, -30.0), (-45.0, -35.0), (45.0, -35.0), (50.0, -30.0), (50.0, 30.0), (45.0, 35.0), (10.0, 35.0), (0.0, 35.0), (-10.0, 35.0), (-45.0, 35.0), (-50.0, 30.0)];
    let points = top.iter().flat_map(|&(x, y)| [[x, y, 40.0], [x, y, 0.0]]).collect();
    let r = w.add_ref(&at(0x100, [0.0; 3]), &[object(Shape::ConvexHull { points }, Motion::Static, 0.0)]);
    assert!(r.errors.is_empty(), "{:?}", r.errors);
}

/// A stone (37 by 13 by 7 units, 25 kg) laid on the table's top in each quarter of it
/// rests on the top and falls asleep. parry gives rapier only the first four corners of a
/// hull's face, a sliver along one edge of this top; a stone lying away from them kept one
/// contact point, sank into the table and rocked there awake [observed:
/// `NVF_PHYS_HULL_FACES=0`, 8 units in 3 s at 60 Hz and 5 at 160; Doc Mitchell's
/// paperweight sank 10 units into the coffee table, physics.md §5.5].
#[test]
fn a_stone_on_a_table_top_of_many_corners_rests_on_it() {
    for hz in RATES {
        for (i, (x, y)) in [(25.0, 15.0), (-25.0, 15.0), (25.0, -15.0), (-25.0, -15.0)].into_iter().enumerate() {
            let mut w = PhysWorld::new(PhysParams::default());
            table(&mut w);
            let stone = object(Shape::Box { half_extents: [18.5, 6.5, 3.5] }, Motion::Dynamic, 25.0);
            w.add_ref(&at(0x200, [x, y, 43.5]), &[stone]);
            let (step_max, asleep) = settle(&mut w, hz, 3.0, 0x200);
            let z = w.ref_bodies(0x200)[0].translation[2];
            assert!((z - 43.5).abs() < 0.5, "{hz} Hz, quarter {i}: centre at z {z} (resting: 43.5)");
            assert!(asleep, "{hz} Hz, quarter {i}: awake after 3 s, moving up to {step_max} a step");
        }
    }
}

/// Turned `a` about z after a tilt of `tilt` about x (row-major).
fn turned(a: f32, tilt: f32) -> [f32; 9] {
    let ((s, c), (st, ct)) = (a.sin_cos(), tilt.sin_cos());
    [c, -s * ct, s * st, s, c * ct, -c * st, 0.0, st, ct]
}

/// A hill over one cell, heights at the LAND vertices (row-major, rows south → north),
/// slopes up to about 25°. `stepped` rounds them to the 8-unit steps LAND stores
/// (world.md §3), so neighbouring triangles often meet at one step over 128 units, 3.6°.
fn hill(stepped: bool) -> Vec<f32> {
    use fnv_world::land::{SIDE, SPACING};
    (0..SIDE * SIDE)
        .map(|i| {
            let (x, y) = ((i % SIDE) as f32 * SPACING, (i / SIDE) as f32 * SPACING);
            let z = 200.0 * (x / 600.0 + 0.3).sin() * (y / 780.0).cos() + 0.05 * x + 0.03 * y;
            if stepped { (z / 8.0).round() * 8.0 } else { z }
        })
        .collect()
}

/// A garment laid out flat: a body and two sleeves, thin boxes in one compound, 1 kg.
fn garment() -> CollisionObject {
    let plate = |x: f32, y: f32, hx: f32, hy: f32| (Iso { translation: [x, y, 0.0], rotation: ID }, Shape::Box { half_extents: [hx, hy, 0.6] });
    let children = vec![plate(0.0, 0.0, 11.0, 15.0), plate(-17.0, 9.0, 6.0, 4.0), plate(17.0, 9.0, 6.0, 4.0)];
    object(Shape::Compound { children }, Motion::Dynamic, 1.0)
}

/// A cloth folded flat: one thin box, 1 kg, rough and dead.
fn folded_cloth() -> CollisionObject {
    CollisionObject { friction: 0.8, restitution: 0.0, ..object(Shape::Box { half_extents: [12.0, 16.0, 0.5] }, Motion::Dynamic, 1.0) }
}

/// A flat stone as a hull of ten points, 3 kg.
fn flat_stone() -> CollisionObject {
    let points = vec![
        [-9.0, -5.0, -2.0], [8.0, -6.0, -2.0], [10.0, 4.0, -1.5], [-7.0, 6.0, -2.0], [0.0, 0.0, -2.5],
        [-8.0, -4.0, 2.0], [7.0, -5.0, 1.5], [9.0, 3.0, 2.0], [-6.0, 5.0, 1.5], [1.0, 1.0, 3.0],
    ];
    object(Shape::ConvexHull { points }, Motion::Dynamic, 3.0)
}

/// Placed clutter dropped on creased ground falls asleep resting on it. rapier merges the
/// contacts of one pair whose normals lie within about 5° into one and solves them with
/// one normal [documented: rapier3d 0.36.1 `contact_clustering.rs`, `COS_MERGE_ANGLE`]:
/// a flat body across two terrain triangles that meet at a small angle was tipped one way,
/// then the other, its farthest point above rapier's 7 units/s, and never slept
/// [observed: issue #2663]. Placed dynamic bodies sleep under `PLACED_SLEEP_SPEED`
/// (physics.md §5.5). 64 drops each of a garment, a folded cloth, a box and a stone on a
/// hill as stored (8-unit steps) and smoothed, at 60 and 160 Hz. With rapier's own
/// threshold 3-8 of the 256 drops stayed awake in each of the four set-ups, most of them
/// garments and cloths, their centres creeping at 0.2-4 units/s [observed: this test's
/// set-up with rapier's threshold, 2026-11-17].
#[test]
fn placed_clutter_on_creased_ground_falls_asleep() {
    use fnv_world::land_surface::{Block, SmoothParams, Surface};
    let shell = TERRAIN_SHELL;
    for stepped in [true, false] {
        let heights = hill(stepped);
        let surface = if stepped {
            Surface::stored(&heights)
        } else {
            let mut block: Block = [[None; 3]; 3];
            block[1][1] = Some(&heights);
            Surface::smoothed(&block, SmoothParams::DEFAULT).expect("a smoothed surface")
        };
        for hz in RATES {
            for (kind, make) in [("garment", garment as fn() -> CollisionObject), ("cloth", folded_cloth), ("box", || object(Shape::Box { half_extents: [8.0, 5.0, 2.5] }, Motion::Dynamic, 4.0)), ("stone", flat_stone)] {
                let mut w = PhysWorld::new(PhysParams::default());
                w.insert_terrain(PreparedTerrain::from_surface(&w.params, (0, 0), &surface));
                let mut spots = Vec::new();
                for i in 0..64u32 {
                    let (x, y) = (400.0 + (i % 8) as f32 * 430.0 + (i / 8) as f32 * 17.0, 400.0 + (i / 8) as f32 * 440.0 + (i % 8) as f32 * 23.0);
                    let ground = surface.height_at(x, y).expect("inside the cell");
                    let pose = RefPose { rotation: turned(i as f32 * 0.7, 0.05 * (i % 5) as f32), ..at(0x300 + i, [x, y, ground + shell + 10.0]) };
                    let r = w.add_ref(&pose, &[make()]);
                    assert!(r.errors.is_empty(), "{:?}", r.errors);
                    spots.push((0x300 + i, x, y));
                }
                w.set_step(1.0 / hz);
                for _ in 0..(12.0 * hz) as usize {
                    w.step();
                }
                let mut awake = Vec::new();
                for &(id, x, y) in &spots {
                    let b = w.ref_bodies(id)[0];
                    let ground = surface.height_at(b.translation[0], b.translation[1]).expect("still in the cell");
                    let above = b.translation[2] - ground;
                    let near = dist(&b.translation, &[x, y, b.translation[2]]);
                    // On the ground: the centre within the body's half-height (at most 3)
                    // plus the shell and a tilt's worth of the slope, near where it fell.
                    assert!(above > 0.0 && above < 12.0 && near < 100.0, "{kind}, stepped {stepped}, {hz} Hz, ref {id:#x}: {above} above the ground, {near} from the drop, {b:?}");
                    if !b.sleeping {
                        awake.push(id);
                    }
                }
                assert!(awake.is_empty(), "{kind}, stepped {stepped}, {hz} Hz: {} of 64 awake after 12 s: {awake:x?}", awake.len());
            }
        }
    }
}

/// The higher sleep speed stops only what is nearly still: a ball rolling at 40 units/s
/// on flat ground is still awake and rolling 3 s later, and so is one started at 21
/// whose centre rolls on slower than `PLACED_SLEEP_SPEED` (once rolling, 5/7 of its
/// start): rapier measures the farthest point, and a rolling ball's top moves at twice
/// its centre's speed [documented: rapier3d 0.36.1 `relative_pose_drift`, the turn
/// times the body's extent added to the move].
#[test]
fn a_ball_rolling_slowly_stays_awake() {
    use fnv_world::land::SIDE;
    // Start speed, and the least and most the centre may move in the last second.
    for (start, least, most) in [(40.0, 20.0, f32::INFINITY), (21.0, 10.0, PLACED_SLEEP_SPEED)] {
        for hz in RATES {
            let mut w = PhysWorld::new(PhysParams::default());
            for x in -1..=0 {
                for y in -1..=0 {
                    w.add_terrain((x, y), &vec![0.0; SIDE * SIDE]);
                }
            }
            let ball = object(Shape::Sphere { radius: 6.0 }, Motion::Dynamic, 2.0);
            w.add_ref(&at(0x400, [-300.0, 0.0, 6.0 + TERRAIN_SHELL]), &[ball]);
            w.set_step(1.0 / hz);
            assert_eq!(w.set_ref_velocity(0x400, 0, start), 1);
            let mut last = [0.0; 3];
            for i in 0..(3.0 * hz) as usize {
                if i == (2.0 * hz) as usize {
                    last = w.ref_bodies(0x400)[0].translation;
                }
                w.step();
            }
            let b = w.ref_bodies(0x400)[0];
            let moved = dist(&b.translation, &last);
            assert!(!b.sleeping && moved > least && moved < most, "{start} units/s, {hz} Hz: asleep {}, {moved} units in the last second, {b:?}", b.sleeping);
        }
    }
}
