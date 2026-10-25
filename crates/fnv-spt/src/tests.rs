use super::*;

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
