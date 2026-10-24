//! Typed views over decoded blocks: the scene graph (nodes and shapes with their local
//! transforms) and triangle geometry. Field names are those of `schemas/nif`; the
//! object model is described in docs/spec-notes/nif.md §3.

use crate::file::{BlockStatus, NifFile};
use crate::schema::Schema;
use crate::value::Struct;

/// Every block decoded; `Err` keeps the reason a block could not be decoded.
pub struct Decoded {
    pub blocks: Vec<Result<Struct, BlockStatus>>,
    /// Lineage (type, then ancestors) per block; empty for unknown types.
    pub lineage: Vec<Vec<String>>,
}

impl Decoded {
    pub fn new(file: &NifFile, data: &[u8], schema: &Schema) -> Decoded {
        let blocks = (0..file.blocks.len()).map(|i| file.decode_block(schema, data, i)).collect();
        let lineage = file
            .blocks
            .iter()
            .map(|b| schema.block(&b.type_name).map(|d| d.lineage.clone()).unwrap_or_default())
            .collect();
        Decoded { blocks, lineage }
    }

    pub fn is_a(&self, i: usize, ty: &str) -> bool {
        self.lineage.get(i).is_some_and(|l| l.iter().any(|t| t == ty))
    }

    pub fn get(&self, i: i32) -> Option<&Struct> {
        usize::try_from(i).ok().and_then(|i| self.blocks.get(i)).and_then(|b| b.as_ref().ok())
    }
}

/// Local transform of a scene object.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Transform {
    pub translation: [f32; 3],
    /// Row-major 3×3.
    pub rotation: [f32; 9],
    pub scale: f32,
}

impl Transform {
    pub const IDENTITY: Transform =
        Transform { translation: [0.0; 3], rotation: [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0], scale: 1.0 };

    pub fn from_struct(s: &Struct) -> Option<Transform> {
        let t = s.floats("translation")?;
        let r = s.floats("rotation")?;
        Some(Transform {
            translation: [t[0], t[1], t[2]],
            rotation: r.try_into().ok()?,
            scale: s.float("scale")?,
        })
    }
}

/// A node or a triangle shape in the scene graph.
#[derive(Debug, Clone)]
pub struct SceneObject {
    pub block: usize,
    pub type_name: String,
    pub name: Option<String>,
    pub parent: Option<usize>,
    pub transform: Transform,
    pub is_node: bool,
    pub children: Vec<usize>,
}

pub const SHAPE_TYPES: [&str; 3] = ["NiTriShape", "NiTriStrips", "BSSegmentedTriShape"];
pub const SHAPE_DATA_TYPES: [&str; 2] = ["NiTriShapeData", "NiTriStripsData"];

/// The geometry blocks of a model ([`scene_objects`]) in depth-first order from its
/// root, children in stored order: the order the 3D Index of a record's alternate
/// textures counts in [observed: docs/spec-notes/texture-swaps.md §2]. Returns, per
/// geometry block, its position.
pub fn geometry_order(objects: &[SceneObject]) -> std::collections::HashMap<usize, usize> {
    let by_block: std::collections::HashMap<usize, &SceneObject> = objects.iter().map(|o| (o.block, o)).collect();
    let mut out = std::collections::HashMap::new();
    let mut stack: Vec<usize> = objects.iter().filter(|o| o.parent.is_none()).map(|o| o.block).rev().collect();
    let mut seen = std::collections::HashSet::new();
    while let Some(b) = stack.pop() {
        if !seen.insert(b) {
            continue;
        }
        let Some(o) = by_block.get(&b) else { continue };
        if !o.is_node {
            let n = out.len();
            out.insert(b, n);
        }
        stack.extend(o.children.iter().rev().copied());
    }
    out
}

/// Nodes (NiNode and derived types) and triangle shapes, in block order, with each
/// object's parent = the lowest-index node listing it as a child. A link that would
/// close a loop (a broken file's node listing itself or one of its ancestors) is left
/// out, so the parents always form a tree.
pub fn scene_objects(file: &NifFile, dec: &Decoded) -> Vec<SceneObject> {
    let n = file.blocks.len();
    let mut parent: Vec<Option<usize>> = vec![None; n];
    for i in 0..n {
        if !dec.is_a(i, "NiNode") {
            continue;
        }
        if let Some(children) = dec.blocks[i].as_ref().ok().and_then(|s| s.refs("children")) {
            for &c in children {
                if let Ok(c) = usize::try_from(c) {
                    if c < n && parent[c].is_none() && !is_ancestor(&parent, c, i) {
                        parent[c] = Some(i);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    for (i, b) in file.blocks.iter().enumerate() {
        let is_node = dec.is_a(i, "NiNode");
        if !is_node && !SHAPE_TYPES.contains(&b.type_name.as_str()) {
            continue;
        }
        let Ok(s) = &dec.blocks[i] else { continue };
        let children = if is_node {
            s.refs("children").unwrap_or_default().iter().filter_map(|&c| usize::try_from(c).ok()).filter(|&c| c < n).collect()
        } else {
            Vec::new()
        };
        out.push(SceneObject {
            block: i,
            type_name: b.type_name.clone(),
            name: s.text("name").map(str::to_string),
            parent: parent[i],
            transform: Transform::from_struct(s).unwrap_or(Transform::IDENTITY),
            is_node,
            children,
        });
    }
    out
}

/// Whether `a` is `b` or one of its ancestors by the links in `parent` (a tree so far).
fn is_ancestor(parent: &[Option<usize>], a: usize, b: usize) -> bool {
    let mut cur = Some(b);
    // The links form a tree, so the walk ends within `parent.len()` steps.
    for _ in 0..=parent.len() {
        match cur {
            Some(x) if x == a => return true,
            Some(x) => cur = parent[x],
            None => return false,
        }
    }
    true
}

/// Summary of one geometry-data block (NiTriShapeData / NiTriStripsData).
#[derive(Debug, Clone, PartialEq)]
pub struct ShapeSummary {
    pub block: usize,
    pub type_name: String,
    pub vertices: usize,
    pub triangles: usize,
    pub has_normals: bool,
    pub has_tangents: bool,
    pub uv_sets: usize,
    pub has_vertex_colors: bool,
    pub bound_center: [f32; 3],
    pub bound_radius: f32,
    pub aabb: Option<([f32; 3], [f32; 3])>,
}

fn vec3s(s: &Struct, name: &str) -> Vec<[f32; 3]> {
    s.floats(name).map(|f| f.as_chunks::<3>().0.to_vec()).unwrap_or_default()
}

pub fn shape_summary(block: usize, type_name: &str, s: &Struct) -> ShapeSummary {
    let verts = vec3s(s, "vertices");
    let aabb = if verts.is_empty() {
        None
    } else {
        let mut lo = verts[0];
        let mut hi = verts[0];
        for v in &verts {
            for k in 0..3 {
                lo[k] = lo[k].min(v[k]);
                hi[k] = hi[k].max(v[k]);
            }
        }
        Some((lo, hi))
    };
    let sphere = s.strukt("bounding_sphere");
    let c = sphere.and_then(|b| b.floats("center")).unwrap_or(&[0.0, 0.0, 0.0]);
    ShapeSummary {
        block,
        type_name: type_name.to_string(),
        vertices: s.int("num_vertices").map_or(verts.len(), |n| n as usize),
        triangles: triangle_count(s),
        has_normals: s.get("normals").is_some(),
        has_tangents: s.get("tangents").is_some(),
        uv_sets: s.list("uv_sets").map_or(0, |l| l.len()),
        has_vertex_colors: s.get("vertex_colors").is_some(),
        bound_center: [c[0], c[1], c[2]],
        bound_radius: sphere.and_then(|b| b.float("radius")).unwrap_or(0.0),
        aabb,
    }
}

/// Triangles described by the index data (strips count n − 2 per strip, including
/// degenerate joins).
pub fn triangle_count(s: &Struct) -> usize {
    if let Some(t) = s.ints("triangles") {
        return t.len() / 3;
    }
    if let Some(rows) = s.list("points") {
        return rows.iter().map(|r| r.len().unwrap_or(0).saturating_sub(2)).sum();
    }
    0
}

/// Triangle list (vertex indices) from a shape-data block. Strips are unrolled with every
/// second triangle's winding reversed; degenerate triangles are dropped.
pub fn triangle_indices(s: &Struct) -> Vec<[u32; 3]> {
    if let Some(t) = s.ints("triangles") {
        return t.as_chunks::<3>().0.iter().map(|c| c.map(|x| x as u32)).collect();
    }
    let mut out = Vec::new();
    if let Some(rows) = s.list("points") {
        for r in rows {
            let crate::Value::IntList(p) = r else { continue };
            for k in 2..p.len() {
                let (a, b, c) = (p[k - 2] as u32, p[k - 1] as u32, p[k] as u32);
                if a == b || b == c || a == c {
                    continue;
                }
                out.push(if k % 2 == 0 { [a, b, c] } else { [a, c, b] });
            }
        }
    }
    out
}

/// Vertex streams of a shape-data block, ready for upload.
#[derive(Debug, Clone, Default)]
pub struct MeshData {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub tangents: Vec<[f32; 3]>,
    pub bitangents: Vec<[f32; 3]>,
    pub colors: Vec<[f32; 4]>,
    /// One vec per UV set.
    pub uv_sets: Vec<Vec<[f32; 2]>>,
    pub triangles: Vec<[u32; 3]>,
}

pub fn mesh_data(s: &Struct) -> MeshData {
    let uv_sets = s
        .list("uv_sets")
        .map(|sets| {
            sets.iter()
                .map(|set| match set {
                    crate::Value::FloatList { data, .. } => data.as_chunks::<2>().0.to_vec(),
                    _ => Vec::new(),
                })
                .collect()
        })
        .unwrap_or_default();
    MeshData {
        positions: vec3s(s, "vertices"),
        normals: vec3s(s, "normals"),
        tangents: vec3s(s, "tangents"),
        bitangents: vec3s(s, "bitangents"),
        colors: s
            .floats("vertex_colors")
            .map(|f| f.as_chunks::<4>().0.to_vec())
            .unwrap_or_default(),
        uv_sets,
        triangles: triangle_indices(s),
    }
}

/// The triangles of each segment of a BSSegmentedTriShape, as ranges into its triangle
/// list of `triangles` triangles: `start_index` counts indices, three per triangle, and
/// `num_primitives` triangles [documented: nif.xml BSGeometrySegmentData]. `None` when
/// the shape has no segments or they do not tile the triangle list in order (each
/// non-empty segment starting where the one before ends, all of them together covering
/// it), as they do in all 355 object LOD block shapes of `Fallout - Meshes.bsa`
/// [observed: `examples/lod_segment_survey.rs`].
pub fn segment_triangles(shape: &Struct, triangles: usize) -> Option<Vec<std::ops::Range<usize>>> {
    let mut out = Vec::new();
    let mut next = 0;
    for seg in shape.list("segment")? {
        let crate::Value::Struct(seg) = seg else { return None };
        let (start, n) = (usize::try_from(seg.int("start_index")?).ok()?, usize::try_from(seg.int("num_primitives")?).ok()?);
        if n > 0 && start != 3 * next {
            return None;
        }
        out.push(next..next + n);
        next += n;
    }
    (!out.is_empty() && next == triangles).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_is_counted_depth_first_for_alternate_textures() {
        // terminal01.nif: root 0 > [79 > 81 > [86 > 88, 105 > [107 > [109 > [111, 117], 123], 127], 136]]
        let o = |block, parent, is_node, children: Vec<usize>| SceneObject {
            block,
            type_name: String::new(),
            name: None,
            parent,
            transform: Transform::IDENTITY,
            is_node,
            children,
        };
        let objects = vec![
            (0, None, true, vec![79]),
            (79, Some(0), true, vec![81]),
            (81, Some(79), true, vec![86, 105, 136]),
            (86, Some(81), true, vec![88]),
            (88, Some(86), false, vec![]),
            (105, Some(81), true, vec![107, 127]),
            (107, Some(105), true, vec![109, 123]),
            (109, Some(107), true, vec![111, 117]),
            (111, Some(109), false, vec![]),
            (117, Some(109), false, vec![]),
            (123, Some(107), false, vec![]),
            (127, Some(105), false, vec![]),
            (136, Some(81), false, vec![]),
        ];
        let objects: Vec<SceneObject> = objects.into_iter().map(|(b, p, n, c)| o(b, p, n, c)).collect();
        let order = geometry_order(&objects);
        let got: Vec<usize> = [88, 111, 117, 123, 127, 136].iter().map(|b| order[b]).collect();
        assert_eq!(got, vec![0, 1, 2, 3, 4, 5]);
    }
    use crate::Value;

    fn shape(segments: &[(i64, i64)]) -> Struct {
        let seg = |&(start, n): &(i64, i64)| {
            Value::Struct(Struct {
                fields: vec![("flags".into(), Value::Int(0)), ("start_index".into(), Value::Int(start)), ("num_primitives".into(), Value::Int(n))],
            })
        };
        Struct { fields: vec![("segment".into(), Value::List(segments.iter().map(seg).collect()))] }
    }

    #[test]
    fn segments_tile_the_triangle_list() {
        // Start indices count indices; an empty segment's start is not looked at.
        assert_eq!(segment_triangles(&shape(&[(0, 2), (0, 0), (6, 3)]), 5), Some(vec![0..2, 2..2, 2..5]));
        // Not a segmented shape, a gap, an overlap, triangles left over.
        assert_eq!(segment_triangles(&Struct::default(), 5), None);
        assert_eq!(segment_triangles(&shape(&[(0, 2), (9, 2)]), 5), None);
        assert_eq!(segment_triangles(&shape(&[(0, 2), (3, 3)]), 5), None);
        assert_eq!(segment_triangles(&shape(&[(0, 2), (6, 2)]), 5), None);
        assert_eq!(segment_triangles(&shape(&[]), 0), None);
    }
}
