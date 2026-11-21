//! Evidence for the LAND texture-layer layout (docs/spec-notes/world.md §3).
//!
//! Neighbouring quadrants share a row or column of the 17 × 17 alpha grid. Under the
//! right reading of VTXT positions, a texture's opacity on that shared edge should agree
//! between the two quadrants. This counts agreement for each of the eight ways to lay a
//! 17 × 17 list over the square, and reports how often a layer on one side of an edge is
//! missing on the other side, and how many quadrants lack a base texture (BTXT).
//!
//! cargo run --release -p fnv-world --example land_layers -- <Data dir> <esm schema dir> [world edid] [--cell x,y]

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::land::{load_land, Land, LandParts, QUAD_SIDE};
use fnv_world::WorldIndex;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
    "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm",
];
const N: usize = QUAD_SIDE;

/// Position → (row south→north, column west→east) under symmetry `s` of the square.
fn place(s: u8, p: usize) -> (usize, usize) {
    let (a, b) = (p / N, p % N);
    let (r, c) = if s & 4 != 0 { (b, a) } else { (a, b) };
    let r = if s & 1 != 0 { N - 1 - r } else { r };
    let c = if s & 2 != 0 { N - 1 - c } else { c };
    (r, c)
}
const NAMES: [&str; 8] = [
    "rows S->N, cols W->E (current)", "rows N->S", "cols E->W", "rows N->S, cols E->W",
    "transposed", "transposed, rows N->S", "transposed, cols E->W", "transposed, both flipped",
];

/// Per quadrant: texture → 17 × 17 opacity grid (layers only), and the base texture.
struct Quads {
    layers: [HashMap<Option<u32>, Vec<f32>>; 4],
    base: [Option<Option<u32>>; 4],
}

fn quads(land: &Land, s: u8) -> Quads {
    let mut q = Quads { layers: Default::default(), base: [None; 4] };
    for l in &land.layers {
        let qi = l.quadrant as usize & 3;
        if l.base {
            q.base[qi] = Some(l.texture);
            continue;
        }
        let g = q.layers[qi].entry(l.texture).or_insert_with(|| vec![0.0; N * N]);
        for &(p, o) in &l.alpha {
            if (p as usize) < N * N {
                let (r, c) = place(s, p as usize);
                g[r * N + c] = o;
            }
        }
    }
    q
}

#[derive(Default, Clone)]
struct Edge {
    /// Texture layered on both sides: edges compared, edges agreeing within 1/255.
    both: u64,
    agree: u64,
    /// Texture layered on one side with opacity > 0.5 somewhere on the edge, absent
    /// (neither layer nor base) on the other side.
    one_sided: u64,
    /// ... but the other side's base texture is that texture.
    other_base: u64,
}

fn edge(a: &[f32], b: &[f32], east: bool) -> (Vec<f32>, Vec<f32>) {
    // `a` is west (or south) of `b`.
    (0..N)
        .map(|i| if east { (a[i * N + N - 1], b[i * N]) } else { (a[(N - 1) * N + i], b[i]) })
        .unzip()
}

fn compare(qa: &Quads, ia: usize, qb: &Quads, ib: usize, east: bool, e: &mut Edge) {
    for (t, ga) in &qa.layers[ia] {
        if let Some(gb) = qb.layers[ib].get(t) {
            let (x, y) = edge(ga, gb, east);
            e.both += 1;
            if x.iter().zip(&y).all(|(x, y)| (x - y).abs() <= 1.0 / 255.0 + 1e-6) {
                e.agree += 1;
            }
        }
    }
    for (qx, ix, qy, iy, x_is_a) in [(qa, ia, qb, ib, true), (qb, ib, qa, ia, false)] {
        for (t, g) in &qx.layers[ix] {
            if qy.layers[iy].contains_key(t) {
                continue;
            }
            let (ea, eb) = edge(g, g, east);
            let side = if x_is_a { ea } else { eb };
            if side.iter().any(|&o| o > 0.5) {
                if qy.base[iy] == Some(*t) {
                    e.other_base += 1;
                } else {
                    e.one_sided += 1;
                }
            }
        }
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let wname = a.get(3).filter(|s| !s.starts_with("--")).cloned().unwrap_or("WastelandNV".into());
    let dump: Option<(i32, i32)> = a.iter().position(|x| x == "--cell").map(|i| {
        let v: Vec<i32> = a[i + 1].split(',').map(|s| s.parse().unwrap()).collect();
        (v[0], v[1])
    });
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let ix = WorldIndex::build(&lo, &schema);
    let w = ix.world_by_name(&wname).expect("world");
    let lands: BTreeMap<(i32, i32), Land> = w
        .grid
        .iter()
        .filter_map(|(&g, c)| Some((g, load_land(&lo, &schema, ix.cells[c].land?, LandParts { heights: false, normals: false, colors: false, layers: true })?)))
        .collect();
    println!("{wname}: {} LAND records", lands.len());

    let (mut no_base, mut quads_total) = (0, 0);
    let mut no_base_at = Vec::new();
    for (g, l) in &lands {
        for q in 0..4u8 {
            quads_total += 1;
            if !l.layers.iter().any(|x| x.base && x.quadrant == q) {
                no_base += 1;
                if no_base_at.len() < 12 {
                    no_base_at.push((*g, q));
                }
            }
        }
    }
    println!("quadrants without BTXT: {no_base} of {quads_total}; e.g. {no_base_at:?}");
    let null_base = lands.values().flat_map(|l| &l.layers).filter(|x| x.base && x.texture.is_none()).count();
    println!("BTXT with null texture: {null_base}");

    for s in 0..8u8 {
        let qs: BTreeMap<_, _> = lands.iter().map(|(g, l)| (*g, quads(l, s))).collect();
        let (mut inner, mut cross) = (Edge::default(), Edge::default());
        for (&(x, y), q) in &qs {
            // Inside the cell: 0|1, 2|3 east; 0/2, 1/3 north.
            compare(q, 0, q, 1, true, &mut inner);
            compare(q, 2, q, 3, true, &mut inner);
            compare(q, 0, q, 2, false, &mut inner);
            compare(q, 1, q, 3, false, &mut inner);
            if let Some(e) = qs.get(&(x + 1, y)) {
                compare(q, 1, e, 0, true, &mut cross);
                compare(q, 3, e, 2, true, &mut cross);
            }
            if let Some(n) = qs.get(&(x, y + 1)) {
                compare(q, 2, n, 0, false, &mut cross);
                compare(q, 3, n, 1, false, &mut cross);
            }
        }
        println!(
            "{:<34} inner: {}/{} agree, one-sided {} (other base {}) | cross-cell: {}/{} agree, one-sided {} (other base {})",
            NAMES[s as usize], inner.agree, inner.both, inner.one_sided, inner.other_base,
            cross.agree, cross.both, cross.one_sided, cross.other_base
        );
    }

    if let Some(g) = dump {
        let l = &lands[&g];
        for x in &l.layers {
            let max = x.alpha.iter().map(|a| a.1).fold(0.0, f32::max);
            println!(
                "q{} {} tex {:?} layer {} points {} max {:.2}",
                x.quadrant, if x.base { "BTXT" } else { "ATXT" }, x.texture.map(|t| format!("{t:08X}")), x.layer, x.alpha.len(), max
            );
        }
    }
}
