//! Loose clutter comes to rest where it lies and falls asleep, at every physics rate
//! (docs/spec-notes/physics.md §5.2). Hand-built worlds, no game data.

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
/// their shelves, `fnvtool phys cell`, physics.md §5.2].
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
/// paperweight sank 10 units into the coffee table, physics.md §5.2].
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
