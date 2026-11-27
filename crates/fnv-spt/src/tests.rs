use super::*;
use crate::generate::springs_from_stub;

/// Builds a file entry by entry (layouts as in speedtree.md §2).
#[derive(Default)]
struct File(Vec<u8>);

impl File {
    fn tag(&mut self, t: u32) -> &mut Self {
        self.0.extend(t.to_le_bytes());
        self
    }
    fn str(&mut self, t: u32, s: &str) -> &mut Self {
        self.tag(t);
        self.0.extend((s.len() as u32).to_le_bytes());
        self.0.extend(s.bytes());
        self
    }
    fn u32(&mut self, t: u32, v: u32) -> &mut Self {
        self.tag(t);
        self.0.extend(v.to_le_bytes());
        self
    }
    fn f32(&mut self, t: u32, v: f32) -> &mut Self {
        self.tag(t);
        self.0.extend(v.to_le_bytes());
        self
    }
    fn u8(&mut self, t: u32, v: u8) -> &mut Self {
        self.tag(t);
        self.0.push(v);
        self
    }
    fn floats(&mut self, t: u32, v: &[f32]) -> &mut Self {
        self.tag(t);
        for f in v {
            self.0.extend(f.to_le_bytes());
        }
        self
    }
    fn level(&mut self, length: [f32; 2], radius: f32, angle: [f32; 2], frequency: f32) -> &mut Self {
        self.tag(1016);
        for t in 6000..=6007 {
            let (lo, hi) = match t {
                6004 => (length[0], length[1]),
                6005 => (radius, radius),
                6007 => (angle[0], angle[1]),
                _ => (0.0, 1.0),
            };
            // A straight rising line for every curve except the taper (falling).
            let pts = if t == 6006 { "0 1 0.707 -0.707 0.1\n\t1 0 0.707 -0.707 0.1" } else { "0 0 0.707 0.707 0.1\n\t1 1 0.707 0.707 0.1" };
            self.str(t, &format!("BezierSpline {lo}\t{hi}\t0\n{{\n\n\t2\n\t{pts}\n\n}}\n"));
        }
        self.u32(6008, 3).u32(6009, 6).f32(6010, 0.2).f32(6011, 1.0).f32(6012, frequency);
        self.f32(6013, 2.0).f32(6014, 5.0).u8(6015, 1).u8(6016, 0);
        self.str(6017, "BezierSpline 0\t1\t0\n{\n\n\t2\n\t0 0 1 0 0.1\n\t1 1 1 0 0.1\n\n}\n");
        self.tag(1017)
    }
}

fn sample() -> Vec<u8> {
    let mut f = File::default();
    f.str(1000, "__IdvSpt_02_").tag(1002).str(2000, "C:\\x\\\\TestBark.tga").f32(2001, 1100.0).u8(2002, 0);
    f.u32(2005, 1234).u32(1014, 3).level([0.1, 0.1], 0.01, [-90.0, -90.0], 30.0);
    f.level([0.05, 0.1], 0.005, [30.0, 60.0], 40.0).level([0.0, 0.0], 0.001, [90.0, 90.0], 0.0);
    f.tag(1015).u32(1006, 1).tag(1007).u8(4000, 0).floats(4001, &[1.0; 3]).f32(4002, 0.1);
    f.str(4003, "C:\\x\\\\Leaf.dds").floats(4004, &[0.5, 0.25, 0.0]).floats(4005, &[0.1, 0.1, 0.0]);
    f.floats(4006, &[5.0, 5.0, 0.0]).f32(4007, 1.0).tag(1008);
    f.tag(10000).tag(10002);
    f.0.extend(1u32.to_le_bytes());
    for v in [1.0f32, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0] {
        f.0.extend(v.to_le_bytes());
    }
    f.tag(10001).u8(22000, 1);
    f.0
}

#[test]
fn parses_every_entry_to_the_end() {
    let data = sample();
    let e = parse_entries(&data).unwrap();
    assert_eq!(e[0].payload.str(), Some("__IdvSpt_02_"));
    assert_eq!(e.last().unwrap().tag, 22000);
    let uv = e.iter().find(|e| e.tag == 10002).unwrap();
    assert_eq!(uv.payload, Payload::UvList(vec![[1.0, 1.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.0]]));
}

#[test]
fn rejects_unknown_tags_and_truncation() {
    let mut data = sample();
    data.extend(99u32.to_le_bytes());
    assert!(matches!(parse_entries(&data), Err(SptError::UnknownTag { tag: 99, .. })));
    let data = sample();
    assert!(matches!(parse_entries(&data[..data.len() - 1]), Err(SptError::Truncated { tag: 22000, .. })));
}

#[test]
fn tree_fields() {
    let t = Tree::parse(&sample()).unwrap();
    assert_eq!(t.seed, 1234);
    assert_eq!(t.bark_texture, "C:\\x\\\\TestBark.tga");
    assert_eq!(t.levels.len(), 3);
    assert_eq!((t.levels[1].segments_along, t.levels[1].segments_around), (3, 6));
    assert_eq!((t.levels[1].frequency, t.levels[1].texture_repeat), (40.0, [2.0, 5.0]));
    assert_eq!(t.leaf_maps[0].pivot, [0.5, 0.25]);
    assert_eq!(t.leaf_maps[0].size, [0.1, 0.1]);
    assert_eq!(t.leaf_uvs.len(), 1);
}

#[test]
fn curves() {
    let c = Curve::parse("BezierSpline 10\t20\t2\n{\n\n\t2\n\t0 0 0.707 0.707 0.1\n\t1 1 0.707 0.707 0.1\n\n}\n").unwrap();
    assert!((c.y(0.0)).abs() < 1e-4 && (c.y(1.0) - 1.0).abs() < 1e-4);
    assert!((c.y(0.5) - 0.5).abs() < 1e-3, "{}", c.y(0.5));
    assert!((c.at(0.5) - 15.0).abs() < 0.02);
    assert_eq!((c.sample(0.0, 0.0), c.sample(1.0, 1.0)), (8.0, 22.0));
    assert!(Curve::parse("BezierSpline 0 1 0 { 2 0 0 }").is_err());
}

#[test]
fn generated_tree() {
    let t = Tree::parse(&sample()).unwrap();
    let g = generate(&t);
    assert_eq!(g, generate(&t), "deterministic");
    // Trunk 0.1 long at 30 per unit: 3 branches; each 0.05-0.1 long at 40 per unit:
    // 2-4 leaf cards.
    assert!((6..=12).contains(&g.leaves.len()), "{} leaves", g.leaves.len());
    assert_eq!(g.positions.len(), g.normals.len());
    assert_eq!(g.indices.len() % 3, 0);
    assert!(g.indices.iter().all(|&i| (i as usize) < g.positions.len()));
    // The trunk grows straight up from the origin to 0.1; branches reach higher.
    assert!(g.top() > 0.1 && g.top() < 0.4, "top {}", g.top());
    assert!(g.leaves.iter().all(|l| l.map == 0 && (l.normal[2] > 0.0)));
    // Dimming: the floor at and below the bottom of the crown, 1 at its top.
    let [lo, hi] = g.crown;
    assert!(lo < hi);
    assert_eq!((g.brightness(0.2, lo - 1.0), g.brightness(0.2, lo), g.brightness(0.2, hi)), (0.2, 0.2, 1.0));
    let mid = g.brightness(0.2, lo + 0.25 * (hi - lo));
    assert!((mid - (0.2 + 0.8 * 0.25f32.cbrt())).abs() < 1e-5, "{mid}");
}

/// A broken file's frequencies and level count cannot grow a tree without end.
#[test]
fn runaway_tree_stays_within_limits() {
    let mut t = Tree::parse(&sample()).unwrap();
    let wood = t.levels[1].clone();
    t.levels.splice(1..1, std::iter::repeat_n(wood, 10));
    for lv in &mut t.levels {
        lv.frequency = f32::INFINITY;
    }
    let g = generate(&t);
    assert!(g.positions.len() <= crate::generate::MAX_VERTICES + 33 * 33, "{} vertices", g.positions.len());
    assert!(g.leaves.len() <= crate::generate::MAX_LEAVES);
    assert!(Curve::parse(&format!("BezierSpline 0 1 0 {{ {} 0 }}", usize::MAX)).is_err());
}

/// A made-up bush for the shape probe (speedtree.md §7, #3126): a 0.3 trunk carrying
/// stems 0.3-0.4 long at 0-10° from the plane across it (near level), each with cards.
fn flat_bush() -> Tree {
    let mut f = File::default();
    f.str(1000, "__IdvSpt_02_").tag(1002).str(2000, "Bush.tga").f32(2001, 600.0).u8(2002, 0);
    f.u32(2005, 77).u32(1014, 3).level([0.3, 0.3], 0.02, [-90.0, -90.0], 30.0);
    f.level([0.3, 0.4], 0.01, [0.0, 10.0], 20.0).level([0.0, 0.0], 0.001, [90.0, 90.0], 0.0);
    f.tag(1015).u32(1006, 1).tag(1007).u8(4000, 0).floats(4001, &[1.0; 3]).f32(4002, 0.1);
    f.str(4003, "Leaf.dds").floats(4004, &[0.5, 0.3, 0.0]).floats(4005, &[0.2, 0.15, 0.0]);
    f.floats(4006, &[10.0, 7.5, 0.0]).f32(4007, 1.0).tag(1008);
    Tree::parse(&f.0).unwrap()
}

#[test]
fn probe_extents_follow_the_renderers_rule() {
    use crate::probe::{extents, Cards};
    let g = generate(&flat_bush());
    let full = extents(&g, Cards::Full);
    // Full cards measure exactly what the renderer scales by.
    assert_eq!((full.width(), full.top), (g.width(), g.top()));
    let pivots = extents(&g, Cards::Pivots);
    let wood = extents(&g, Cards::None);
    assert!(pivots.width() < full.width() && pivots.top < full.top, "{pivots:?} {full:?}");
    assert!(wood.top <= full.top && wood.width() > 0.0);
    // Level stems on a 0.3 trunk: wider than tall.
    assert!(full.aspect() > 1.5, "{full:?}");
}

#[test]
fn probe_readings() {
    use crate::generate::angle_from_axis;
    use crate::probe::{angle_readings, apply, build, readings, shapes, Reading};
    let t = flat_bush();
    assert_eq!(apply(&t, Reading::AsIs), t);
    // Three levels: one wood level grows from another (level 1).
    assert_eq!(
        readings(&t),
        [
            Reading::AsIs,
            Reading::AngleFromAxis(1),
            Reading::AngleFromAxisAllWood,
            Reading::AngleFromPlaneEverywhere,
            Reading::NoLengthSpread,
            Reading::NoBend,
            Reading::FrequencyOverSpan
        ]
    );
    // The angle readings change no parameter, only which levels read from the axis.
    assert_eq!(apply(&t, Reading::AngleFromAxis(1)), t);
    assert_eq!(angle_readings(&t, Reading::AsIs), [false, false, false]);
    assert_eq!(angle_readings(&t, Reading::AngleFromAxis(1)), [false, true, false]);
    assert_eq!(angle_readings(&t, Reading::AngleFromAxisAllWood), [false, true, false]);
    let span = apply(&t, Reading::FrequencyOverSpan);
    assert!((span.levels[1].frequency - 20.0 * 0.8).abs() < 1e-5);
    let s = shapes(&t);
    assert_eq!(s, shapes(&t), "deterministic");
    assert_eq!(s[0].full.width(), generate(&t).width());
    assert_eq!(build(&t, Reading::AsIs), generate(&t));
    // Stems 0-10° off the axis instead of off the plane: a column, not a plate.
    assert!(s[1].full.aspect() < 0.7 * s[0].full.aspect(), "{:?} vs {:?}", s[1].full, s[0].full);
    // Fewer children over the 0.2-1.0 span.
    assert!(s[6].leaves < s[0].leaves, "{} vs {}", s[6].leaves, s[0].leaves);
    // The 0.3 trunk is no shorter than its 0.3-0.4 stems: no stub, the plane reading.
    assert!(!springs_from_stub(&t, 1));
    assert_eq!(build(&t, Reading::AngleFromPlaneEverywhere), generate(&t));
    // On a stub the generator reads from the axis already.
    let stub = stub_bush();
    assert_eq!(build(&stub, Reading::AngleFromAxis(1)), generate(&stub));
    assert_ne!(build(&stub, Reading::AngleFromPlaneEverywhere), generate(&stub));

    // Stems at least 0.19 (0.25 ± 0.06) on a 0.1 trunk: under twice the trunk, no stub;
    // without the spread 2.5 times it. The probe's no-spread reading keeps the tree's
    // own angle choice, so it shows the spread's effect alone.
    let mut near = flat_bush();
    let c = &mut near.levels[0].curves[4];
    (c.min, c.max) = (0.1, 0.1);
    let c = &mut near.levels[1].curves[4];
    (c.min, c.max, c.variance) = (0.25, 0.3, 0.06);
    assert!(!springs_from_stub(&near, 1));
    assert!(springs_from_stub(&apply(&near, Reading::NoLengthSpread), 1));
    assert_eq!(angle_readings(&near, Reading::NoLengthSpread), angle_from_axis(&near));
    assert_eq!(angle_readings(&near, Reading::NoLengthSpread), [false, false, false]);
}

/// The made-up bush on a 0.01 stub instead of its 0.3 trunk (#3126: EuonymusBush01's
/// stems spring from a trunk a tenth of their length).
fn stub_bush() -> Tree {
    let mut t = flat_bush();
    let c = &mut t.levels[0].curves[4];
    (c.min, c.max) = (0.01, 0.01);
    t.levels[0].frequency = 800.0;
    t
}

/// Branches springing from a stub take their 6007 angle from the stub's axis, so stems
/// at 0-10° stand up instead of lying flat (speedtree.md §7, #3126).
#[test]
fn stems_on_a_stub_stand_up() {
    use crate::generate::{length_ratio, STUB_RATIO};
    use crate::probe::{build, extents, Cards, Reading};
    let stub = stub_bush();
    assert!(springs_from_stub(&stub, 1) && !springs_from_stub(&stub, 2) && !springs_from_stub(&stub, 0));
    assert!((length_ratio(&stub, 1).unwrap() - 30.0).abs() < 1e-3);
    let now = extents(&generate(&stub), Cards::Full);
    // The reading before #3126: the same stems 0-10° off the plane, a flat plate.
    let before = extents(&build(&stub, Reading::AngleFromPlaneEverywhere), Cards::Full);
    assert!(before.aspect() > 2.0, "{before:?}");
    assert!(now.aspect() < 1.0, "{now:?}");
    // Every branch springs straight up within 10° of the stub's axis.
    let g = generate(&stub);
    assert!(g.leaves.iter().all(|l| l.position[2] > 0.0));
    // A trunk barely shorter than its stems is no stub: the margin.
    let mut long = stub.clone();
    let c = &mut long.levels[0].curves[4];
    (c.min, c.max) = (0.29, 0.29);
    assert!(length_ratio(&long, 1).unwrap() < STUB_RATIO && !springs_from_stub(&long, 1));
}

/// On a stub level with a spread, the axis reading builds the same branches as the
/// curve mirrored about 90° under the plane reading (the spread taken as 1 − r), which is
/// what the measured rows of speedtree.md §7 were made with; equal up to rounding (the
/// angle is worked out in another order).
#[test]
fn stub_reading_with_a_spread_matches_the_mirrored_curve() {
    use crate::generate::generate_with;
    let mut stub = stub_bush();
    stub.levels[1].curves[7].variance = 12.0;
    let mut mirrored = stub.clone();
    let c = &mut mirrored.levels[1].curves[7];
    (c.min, c.max) = (90.0 - c.min, 90.0 - c.max);
    let g = generate(&stub);
    let same = |a: &Geometry, b: &Geometry| {
        a.leaves.len() == b.leaves.len()
            && a.positions.len() == b.positions.len()
            && a.leaves.iter().zip(&b.leaves).all(|(x, y)| (0..3).all(|k| (x.position[k] - y.position[k]).abs() < 1e-4))
            && a.positions.iter().zip(&b.positions).all(|(x, y)| (0..3).all(|k| (x[k] - y[k]).abs() < 1e-4))
    };
    assert!(same(&g, &generate_with(&mirrored, &[false, false, false])));
    // The spread is in use: without it the branches differ.
    let mut still = stub.clone();
    still.levels[1].curves[7].variance = 0.0;
    assert!(!same(&g, &generate(&still)));
}
