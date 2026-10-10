//! Diagnostic survey of the segments of object LOD blocks (BSSegmentedTriShape), for
//! deciding how the renderers hide a block's part under a loaded cell
//! (docs/spec-notes/world.md §7 "Objects"). Per block shape: whether the segments tile the
//! triangle list in order, which cell of the 4 × 4 square holds most of each segment's
//! triangle centres, how many triangles lie in another cell than their segment's, whether
//! any vertex is used by two segments, and how many connected pieces of geometry (triangles
//! sharing a vertex index) reach over more than one cell or more than one segment.
//!
//! cargo run --release -p fnv-nif --example lod_segment_survey -- <schema dir> <archive.bsa>... [--block <text>]
//!
//! With `--block <text>`, blocks whose path contains the text are also listed segment by
//! segment.

use fnv_nif::scene::{mesh_data, scene_objects, Decoded};
use fnv_nif::{NifFile, Schema, Value};
use std::collections::{BTreeMap, BTreeSet, HashMap};

const CELL: f32 = 4096.0;

/// A connected piece of geometry: the cells its triangle centres lie in, its segments.
type Piece = (BTreeSet<(i32, i32)>, BTreeSet<usize>);

fn find(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

#[derive(Default)]
struct Totals {
    shapes: usize,
    segmented: usize,
    in_order: usize,
    segments: usize,
    empty: usize,
    majority_own: usize,
    triangles: usize,
    outside: usize,
    shared_vertices: usize,
    pieces: usize,
    pieces_over_cells: usize,
    pieces_over_segments: usize,
    counts: BTreeMap<usize, usize>,
    flags: BTreeMap<i64, usize>,
}

fn main() {
    let mut a: Vec<String> = std::env::args().collect();
    let detail = a.iter().position(|x| x == "--block").map(|i| {
        let text = a.remove(i + 1).to_ascii_lowercase();
        a.remove(i);
        text
    });
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut t = Totals::default();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> = bsa
            .entries()
            .map(|e| (e.path(), e.record.clone()))
            .filter(|(p, _)| p.ends_with(".nif") && p.contains("\\lod\\") && p.contains("\\blocks\\"))
            .collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            let listed = detail.as_deref().is_some_and(|d| path.contains(d));
            for o in scene_objects(&file, &dec).iter().filter(|o| !o.is_node) {
                t.shapes += 1;
                let Ok(shape) = &dec.blocks[o.block] else { continue };
                let Some(segments) = shape.list("segment") else { continue };
                let Some(md) = shape.block_ref("data").and_then(|r| dec.get(r)).map(mesh_data) else { continue };
                t.segmented += 1;
                *t.counts.entry(segments.len()).or_default() += 1;
                let segs: Vec<(i64, usize, usize)> = segments
                    .iter()
                    .filter_map(|v| match v {
                        Value::Struct(s) => Some((s.int("flags")?, s.int("start_index")? as usize, s.int("num_primitives")? as usize)),
                        _ => None,
                    })
                    .collect();
                // In order: each segment starts where the one before ends (three indices
                // per triangle) and together they cover the triangle list.
                let mut next = 0;
                let mut ordered = true;
                for &(_, start, n) in &segs {
                    ordered &= n == 0 || start == next;
                    if n > 0 {
                        next = start + 3 * n;
                    }
                }
                ordered &= next == 3 * md.triangles.len();
                t.in_order += usize::from(ordered);
                // The shape's own positions are relative to the square's south-west corner.
                let cell = |tri: &[u32; 3]| {
                    let c = tri.iter().fold([0.0f32; 2], |c, &v| {
                        let p = md.positions[v as usize];
                        [c[0] + p[0] / 3.0, c[1] + p[1] / 3.0]
                    });
                    (((c[0] / CELL).floor() as i32).clamp(0, 3), ((c[1] / CELL).floor() as i32).clamp(0, 3))
                };
                let mut seg_of = vec![usize::MAX; md.triangles.len()];
                let mut vertex_segs: HashMap<u32, BTreeSet<usize>> = HashMap::new();
                for (i, &(flags, start, n)) in segs.iter().enumerate() {
                    t.segments += 1;
                    *t.flags.entry(flags).or_default() += 1;
                    if n == 0 {
                        t.empty += 1;
                        continue;
                    }
                    let own = ((i / 4) as i32, (i % 4) as i32);
                    let mut by_cell: BTreeMap<(i32, i32), usize> = BTreeMap::new();
                    for (tri, seg) in md.triangles.iter().zip(&mut seg_of).skip(start / 3).take(n) {
                        *seg = i;
                        *by_cell.entry(cell(tri)).or_default() += 1;
                        for &v in tri {
                            vertex_segs.entry(v).or_default().insert(i);
                        }
                    }
                    let inside = by_cell.get(&own).copied().unwrap_or(0);
                    let most = by_cell.iter().max_by_key(|(_, &c)| c).map(|(c, _)| *c);
                    t.majority_own += usize::from(most == Some(own));
                    t.triangles += n;
                    t.outside += n - inside;
                    if listed {
                        println!("{path} segment {i:2} cell {own:?} flags {flags} triangles {n:6} in its cell {inside:6} by cell {by_cell:?}");
                    }
                }
                t.shared_vertices += vertex_segs.values().filter(|s| s.len() > 1).count();
                // Connected pieces by shared vertex index.
                let mut parent: Vec<usize> = (0..md.positions.len()).collect();
                for tri in &md.triangles {
                    let r = find(&mut parent, tri[0] as usize);
                    for &v in &tri[1..] {
                        let s = find(&mut parent, v as usize);
                        parent[s] = r;
                    }
                }
                let mut pieces: HashMap<usize, Piece> = HashMap::new();
                for (k, tri) in md.triangles.iter().enumerate() {
                    let p = pieces.entry(find(&mut parent, tri[0] as usize)).or_default();
                    p.0.insert(cell(tri));
                    p.1.insert(seg_of[k]);
                }
                t.pieces += pieces.len();
                t.pieces_over_cells += pieces.values().filter(|p| p.0.len() > 1).count();
                t.pieces_over_segments += pieces.values().filter(|p| p.1.len() > 1).count();
            }
        }
    }
    println!("block shapes {} (with segments {}), segments tile the triangles in order in {}", t.shapes, t.segmented, t.in_order);
    println!("segments per shape: {:?}", t.counts);
    println!("segment flags: {:?}", t.flags);
    println!("segments {} (empty {}), most triangle centres in cell (i / 4, i mod 4): {}", t.segments, t.empty, t.majority_own);
    println!("triangles {} with their centre in another cell than their segment's: {}", t.triangles, t.outside);
    println!("vertices used by two segments: {}", t.shared_vertices);
    println!("connected pieces {}: over more than one cell {}, over more than one segment {}", t.pieces, t.pieces_over_cells, t.pieces_over_segments);
}
