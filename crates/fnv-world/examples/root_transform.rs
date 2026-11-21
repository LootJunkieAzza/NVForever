//! Evidence for how a placed model's root node transform combines with the reference
//! transform (docs/spec-notes/world.md §4).
//!
//! 1. Census: how many placed references use a model whose root node has a non-identity
//!    transform, split into interior and exterior cells, with the most used examples.
//! 2. Static collections: for parts whose model has a non-identity root, does the combined
//!    mesh's part node carry the placement alone (root replaced) or placement ∘ root
//!    (root composed)?
//!
//! cargo run --release -p fnv-world --example root_transform -- <Data dir> <esm schema dir> <nif schema dir>

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_nif::scene::{scene_objects, Decoded, Transform};
use fnv_nif::NifFile;
use fnv_world::fields::{formid_of, global, num, vec3, Fields};
use fnv_world::placed::{base_object, placement};
use fnv_world::transform::{max_diff, mul, ref_rotation, IDENTITY};
use fnv_world::WorldIndex;
use std::collections::HashMap;
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
    "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm",
];

fn is_identity(t: &Transform) -> bool {
    max_diff(&t.rotation, &IDENTITY) < 1e-4 && t.translation.iter().all(|v| v.abs() < 1e-3) && (t.scale - 1.0).abs() < 1e-4
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let nif_schema = fnv_nif::Schema::load_dir(Path::new(&a[3])).unwrap();
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let vfs = fnv_vfs::Vfs::open_default(data).unwrap();
    let mut roots: HashMap<String, Option<Transform>> = HashMap::new();
    let mut root_of = |path: &str| -> Option<Transform> {
        *roots
            .entry(path.to_string())
            .or_insert_with(|| {
                let bytes = vfs.read(path)?.ok()?;
                let nf = NifFile::parse(&bytes, Some(&nif_schema)).ok()?;
                let dec = Decoded::new(&nf, &bytes, &nif_schema);
                scene_objects(&nf, &dec).into_iter().find(|o| o.parent.is_none()).map(|o| o.transform)
            })
    };

    // 1. Census over every placed reference.
    let index = WorldIndex::build(&lo, &schema);
    let (mut total, mut odd) = ([0usize; 2], [0usize; 2]);
    let mut examples: HashMap<String, (usize, usize, Transform)> = HashMap::new();
    for (&fid, e) in &index.refs {
        let interior = index.cells.get(&e.cell).is_some_and(|c| c.is_interior()) as usize;
        let Some(p) = placement(&lo, &schema, fid) else { continue };
        let Some(path) = p.base.and_then(|b| base_object(&lo, &schema, b)).and_then(|b| b.model_path()) else { continue };
        let Some(t) = root_of(&path) else { continue };
        total[interior] += 1;
        if !is_identity(&t) {
            odd[interior] += 1;
            let x = examples.entry(path).or_insert((0, 0, t));
            x.0 += 1 - interior;
            x.1 += interior;
        }
    }
    println!("references with a non-identity model root: exterior {} of {}, interior {} of {}", odd[0], total[0], odd[1], total[1]);
    let mut ex: Vec<_> = examples.into_iter().collect();
    ex.sort_by_key(|(_, (e, i, _))| std::cmp::Reverse(e + i));
    println!("models with a non-identity root: {}; most placed:", ex.len());
    for (path, (e, i, t)) in ex.iter().take(15) {
        println!("  ext {e:5} int {i:5}  {path}  t {:?} r {:?} s {}", t.translation, t.rotation, t.scale);
    }

    // 2. Static collection parts whose model root is not identity.
    let (mut replaced, mut composed, mut neither, mut both) = (0, 0, 0, 0);
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"SCOL") {
            let f = Fields::new(r, pi, &schema).unwrap();
            let model = f.string(b"MODL").unwrap_or_default();
            let mut parts = Vec::new();
            let mut cur: Option<u32> = None;
            for s in &f.subs {
                match &s.sig {
                    b"ONAM" => cur = f.decode(s).and_then(|v| formid_of(&v)).and_then(|l| global(&lo, pi, l)),
                    b"DATA" => {
                        for p in f.decode(s).and_then(|v| v.as_array().cloned()).unwrap_or_default() {
                            parts.push((cur, vec3(&p["position"]).unwrap(), vec3(&p["rotation_radians"]).unwrap(), num(&p, "scale").unwrap() as f32));
                        }
                    }
                    _ => {}
                }
            }
            let path = format!("meshes\\{}", model.to_ascii_lowercase());
            let Some(Ok(bytes)) = vfs.read(&path) else { continue };
            let Ok(nf) = NifFile::parse(&bytes, Some(&nif_schema)) else { continue };
            let dec = Decoded::new(&nf, &bytes, &nif_schema);
            let nodes: Vec<_> = scene_objects(&nf, &dec).into_iter().filter(|o| o.is_node && o.parent.is_some()).collect();
            for (b, pos, rot, sc) in &parts {
                let Some(part_path) = b.and_then(|b| base_object(&lo, &schema, b)).and_then(|b| b.model_path()) else { continue };
                let Some(root) = root_of(&part_path) else { continue };
                if max_diff(&root.rotation, &IDENTITY) < 1e-4 {
                    continue;
                }
                let cands: Vec<_> =
                    nodes.iter().filter(|n| n.transform.translation.iter().zip(pos).all(|(a, b)| (a - b).abs() < 0.01)).collect();
                if cands.is_empty() {
                    continue;
                }
                let place = ref_rotation(*rot);
                let comp = mul(&place, &root.rotation);
                let fit = |m: &[f32; 9]| cands.iter().any(|n| max_diff(m, &n.transform.rotation) < 1e-3);
                match (fit(&place), fit(&comp)) {
                    (true, false) => replaced += 1,
                    (false, true) => composed += 1,
                    (true, true) => both += 1,
                    (false, false) => {
                        neither += 1;
                        if neither <= 5 {
                            println!("  neither: part {part_path} scale {sc} root r {:?}", root.rotation);
                        }
                    }
                }
            }
        }
    }
    println!(
        "SCOL parts with a rotated model root: node = placement only {replaced}, = placement ∘ root {composed}, both {both}, neither {neither}"
    );

    // 3. Seams: modular pieces are built to meet their neighbours edge to edge. Place
    //    the geometry of every reference in a cell under each reading and count open
    //    (boundary) edges shared by two references, one of them with a rotated-root
    //    model (the others are placed the same way under both readings). Every cell, or
    //    only `--cell <Editor ID or FormID>`.
    let only = a.iter().position(|x| x == "--cell").map(|i| index.cell_by_name(&a[i + 1]).expect("no such cell").form_id);
    let mut meshes: HashMap<String, Option<Model>> = HashMap::new();
    // [interior][reading]: shared edges; reading 0 = root composed, 1 = root replaced.
    let mut shared = [[0usize; 2]; 2];
    let mut cells_used = [0usize; 2];
    // [interior]: cells where root composed / root replaced shares more edges.
    let mut favours = [[0usize; 2]; 2];
    for c in index.cells.values().filter(|c| only.is_none_or(|f| f == c.form_id)) {
        let mut placed = Vec::new();
        let mut any_rotated = false;
        for &r in &c.refs {
            let Some(p) = placement(&lo, &schema, r) else { continue };
            if p.deleted() {
                continue;
            }
            let Some(path) = p.base.and_then(|b| base_object(&lo, &schema, b)).and_then(|b| b.model_path()) else { continue };
            let m = meshes.entry(path.clone()).or_insert_with(|| Model::load(&vfs, &nif_schema, &path));
            if let Some(m) = m {
                any_rotated |= m.rotated_root;
                placed.push((r, p, path));
            }
        }
        if !any_rotated {
            continue;
        }
        let interior = c.is_interior() as usize;
        cells_used[interior] += 1;
        let mut here = [0usize; 2];
        for (reading, keep_root) in [true, false].into_iter().enumerate() {
            let mut edges: HashMap<EdgeKey, Vec<(u32, bool)>> = HashMap::new();
            for (r, p, path) in &placed {
                let m = meshes[path].as_ref().unwrap();
                let place = Affine::new(ref_rotation(p.rotation), p.position, p.scale);
                let t = if keep_root { place.then_inner(&Affine::of(&m.root)) } else { place };
                for e in &m.open_edges {
                    let q = |v: [f32; 3]| t.apply(v).map(|x| x.round() as i32);
                    let (u, v) = (q(e.0), q(e.1));
                    edges.entry(if u < v { (u, v) } else { (v, u) }).or_default().push((*r, m.rotated_root));
                }
            }
            here[reading] = edges.values().filter(|v| v.iter().any(|x| x.1) && v.iter().any(|x| x.0 != v[0].0)).count();
            shared[interior][reading] += here[reading];
        }
        if here[0] != here[1] {
            favours[interior][(here[1] > here[0]) as usize] += 1;
        }
    }
    for (i, name) in ["exterior", "interior"].iter().enumerate() {
        println!(
            "{name}: {} cells with a rotated-root model; open edges shared between references: root composed {}, root replaced {}; cells favouring composed {}, replaced {}",
            cells_used[i], shared[i][0], shared[i][1], favours[i][0], favours[i][1]
        );
    }
}

/// A model's root transform and its open (boundary) edges in root space, i.e. below
/// the root's own transform.
struct Model {
    root: Transform,
    rotated_root: bool,
    open_edges: Vec<([f32; 3], [f32; 3])>,
}

impl Model {
    fn load(vfs: &fnv_vfs::Vfs, nif_schema: &fnv_nif::Schema, path: &str) -> Option<Model> {
        let bytes = vfs.read(path)?.ok()?;
        let nf = NifFile::parse(&bytes, Some(nif_schema)).ok()?;
        let dec = Decoded::new(&nf, &bytes, nif_schema);
        let objs = scene_objects(&nf, &dec);
        let root = objs.iter().find(|o| o.parent.is_none())?.transform;
        let by_block: HashMap<usize, &fnv_nif::scene::SceneObject> = objs.iter().map(|o| (o.block, o)).collect();
        let mut count: HashMap<EdgeKey, (u32, [f32; 3], [f32; 3])> = HashMap::new();
        for o in objs.iter().filter(|o| !o.is_node) {
            let mut t = Affine::of(&o.transform);
            let mut cur = o.parent;
            while let Some(po) = cur.and_then(|b| by_block.get(&b)) {
                if po.parent.is_some() {
                    t = Affine::of(&po.transform).then_inner(&t);
                }
                cur = po.parent;
            }
            let Some(Ok(shape)) = dec.blocks.get(o.block) else { continue };
            if shape.int("flags").unwrap_or(0) & 1 != 0 {
                continue;
            }
            let Some(data) = shape.block_ref("data").and_then(|r| dec.get(r)) else { continue };
            let md = fnv_nif::scene::mesh_data(data);
            let pos: Vec<[f32; 3]> = md.positions.iter().map(|v| t.apply(*v)).collect();
            let key = |v: [f32; 3]| v.map(|x| (x * 8.0).round() as i32);
            for tri in &md.triangles {
                for k in 0..3 {
                    let (Some(&u), Some(&v)) = (pos.get(tri[k] as usize), pos.get(tri[(k + 1) % 3] as usize)) else { continue };
                    let (ku, kv) = (key(u), key(v));
                    if ku != kv {
                        count.entry(if ku < kv { (ku, kv) } else { (kv, ku) }).or_insert((0, u, v)).0 += 1;
                    }
                }
            }
        }
        let open_edges = count.into_values().filter(|e| e.0 == 1).map(|e| (e.1, e.2)).collect();
        Some(Model { rotated_root: max_diff(&root.rotation, &IDENTITY) >= 1e-4, root, open_edges })
    }
}

/// An edge between two quantised positions, smaller endpoint first.
type EdgeKey = ([i32; 3], [i32; 3]);

/// Row-major rotation·scale plus translation, rotating column vectors.
struct Affine {
    m: [f32; 9],
    t: [f32; 3],
}

impl Affine {
    fn new(r: [f32; 9], t: [f32; 3], s: f32) -> Affine {
        Affine { m: r.map(|v| v * s), t }
    }
    fn of(t: &Transform) -> Affine {
        Affine::new(t.rotation, t.translation, t.scale)
    }
    fn apply(&self, v: [f32; 3]) -> [f32; 3] {
        let m = &self.m;
        [0, 1, 2].map(|r| m[r * 3] * v[0] + m[r * 3 + 1] * v[1] + m[r * 3 + 2] * v[2] + self.t[r])
    }
    /// `self ∘ inner`: apply `inner` first.
    fn then_inner(&self, inner: &Affine) -> Affine {
        Affine { m: mul(&self.m, &inner.m), t: self.apply(inner.t) }
    }
}
