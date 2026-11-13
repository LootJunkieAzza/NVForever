//! Convex hulls whose faces have more than four corners keep their whole face in contact
//! (docs/spec-notes/physics.md §5.2).
//!
//! rapier finds the contact between two convex shapes by clipping the face of each that
//! looks at the other, and parry hands it at most four corners of a face: the first four
//! in the face's order, whatever its size [documented: parry3d 0.31.1,
//! `ConvexPolyhedron::local_support_feature`, "TODO: if there are more than 4 vertices,
//! we need to select four vertices that maximize the area"]. A hull merges every
//! coplanar triangle into one face, so a table top or the bottom of a bottle is one face
//! of 8 to 18 corners, and four neighbours among them are a sliver along one edge. A body
//! lying on the rest of the face clipped against that sliver kept one contact point and
//! sank into the table by its weight, rocking about that point, and never slept: a
//! whetstone, an ashtray, reading glasses and a cigarette sank 3 to 13 units into the two
//! Goodsprings coffee tables (`SubCoffeeTableDirty01`, a hull whose top face has 11
//! corners) [observed: `fnvtool phys cell` with every dynamic reference awake,
//! 2026-11-11; again 2026-11-13: Doc Mitchell's paperweight 001062BB sank 10 units into
//! one and was awake after 20 s at 60 Hz; with `Hull` it rested on the top, asleep,
//! 0.8 units from where it was placed].
//!
//! [`Hull`] is parry's hull with that choice made well: of a face with more than four
//! corners it gives the four that span the most area (a table top all but whole, a round
//! bottom its inscribed square), or, when the contact lies outside them (near a round
//! bottom's rim), the four that span the most with the corner nearest the contact. Both
//! are worked out once, when the shape is made. Everything else (rays, distances, mass,
//! bounding boxes) is the hull's own. `NVF_PHYS_HULL_FACES=0` (a test aid, read once)
//! builds parry's hull as before.

use rapier3d::math::{Pose, Real, Vector};
use rapier3d::parry::bounding_volume::{Aabb, BoundingSphere};
use rapier3d::parry::mass_properties::MassProperties;
use rapier3d::parry::query::{PointProjection, PointQuery, Ray, RayCast, RayIntersection};
use rapier3d::parry::shape::{
    ConvexPolyhedron, FeatureId, PackedFeatureId, PolygonalFeature, PolygonalFeatureMap, Shape, ShapeType, SharedShape, SubShapeId, SupportMap,
    TypedShape,
};
use std::sync::Arc;

/// Whether hulls are built as [`Hull`]: `NVF_PHYS_HULL_FACES=0` builds parry's plain hull
/// (a test aid for comparing, read once).
pub fn enabled() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var("NVF_PHYS_HULL_FACES").map_or(true, |v| v != "0"))
}

/// The convex hull of `points` as a shape: a [`Hull`] when one of its faces has more than
/// four corners (and [`enabled`]), parry's hull otherwise. `None` when the points are
/// degenerate.
pub fn convex_hull(points: &[Vector]) -> Option<SharedShape> {
    let poly = ConvexPolyhedron::from_convex_hull(points)?;
    Some(if enabled() && poly.faces().iter().any(|f| f.num_vertices_or_edges > 4) {
        SharedShape(Arc::new(Hull::new(poly)))
    } else {
        SharedShape(Arc::new(poly))
    })
}

/// Four corners of a face, as positions in its corner loop, in loop order.
type Quad = [u32; 4];

/// The corners a face with more than four of them gives for contacts.
#[derive(Debug, Clone)]
struct FaceQuads {
    /// The four spanning the most area.
    best: Quad,
    /// For each corner, the four spanning the most area with it.
    with: Vec<Quad>,
}

/// A convex hull that gives the four corners of a large face that span it best (see the
/// module notes).
#[derive(Debug, Clone)]
pub struct Hull {
    poly: ConvexPolyhedron,
    /// By face: `None` for a face of four corners or fewer.
    quads: Vec<Option<FaceQuads>>,
}

impl Hull {
    pub fn new(poly: ConvexPolyhedron) -> Hull {
        let quads = poly
            .faces()
            .iter()
            .map(|f| {
                let n = f.num_vertices_or_edges as usize;
                (n > 4).then(|| {
                    let first = f.first_vertex_or_edge as usize;
                    let corners: Vec<Vector> = poly.vertices_adj_to_face()[first..first + n].iter().map(|&v| poly.points()[v as usize]).collect();
                    face_quads(&corners)
                })
            })
            .collect();
        Hull { poly, quads }
    }

    /// The hull itself.
    pub fn polyhedron(&self) -> &ConvexPolyhedron {
        &self.poly
    }
}

/// Twice the area of triangle `a b c`.
fn area2(a: Vector, b: Vector, c: Vector) -> Real {
    (b - a).cross(c - a).length()
}

/// The best quads of a convex polygon (`corners` in loop order, more than four).
fn face_quads(corners: &[Vector]) -> FaceQuads {
    let n = corners.len();
    let at = |i: usize| corners[i % n];
    // With corner `a`: the opposite corner `c`, and on either side of the diagonal the
    // corner farthest from it (the quad's area is the two triangles').
    let with = |a: usize| -> (Quad, Real) {
        let mut best = ([0; 4], -1.0);
        for c in 2..=n - 2 {
            let side = |range: std::ops::Range<usize>| {
                range.map(|j| (j, area2(at(a), at(a + j), at(a + c)))).fold((0, -1.0), |m, x| if x.1 > m.1 { x } else { m })
            };
            let (b, ab) = side(1..c);
            let (d, ad) = side(c + 1..n);
            if ab + ad > best.1 {
                best = ([a, a + b, a + c, a + d].map(|i| (i % n) as u32), ab + ad);
            }
        }
        best
    };
    let all: Vec<(Quad, Real)> = (0..n).map(with).collect();
    let best = all.iter().fold(all[0], |m, x| if x.1 > m.1 { *x } else { m }).0;
    FaceQuads { best, with: all.into_iter().map(|x| x.0).collect() }
}

/// Whether `p`, projected on the plane of `quad` (normal `n`), lies within it.
fn within(quad: &[Vector; 4], n: Vector, p: Vector) -> bool {
    (0..4).all(|i| (quad[(i + 1) % 4] - quad[i]).cross(p - quad[i]).dot(n) >= 0.0)
}

impl Hull {
    /// The face of the hull that looks most along `dir`, with the corners chosen for a
    /// contact near local point `hint` (see the module notes).
    fn feature(&self, dir: Vector, hint: Option<Vector>, out: &mut PolygonalFeature) {
        let faces = self.poly.faces();
        let mut best = 0;
        let mut best_dot = faces[0].normal.dot(dir);
        for (i, f) in faces.iter().enumerate().skip(1) {
            let d = f.normal.dot(dir);
            if d > best_dot {
                best = i;
                best_dot = d;
            }
        }
        let face = &faces[best];
        let first = face.first_vertex_or_edge as usize;
        let n = face.num_vertices_or_edges as usize;
        let verts = &self.poly.vertices_adj_to_face()[first..first + n];
        let edges = &self.poly.edges_adj_to_face()[first..first + n];
        let pick: Quad = match &self.quads[best] {
            None => [0, 1, 2, 3],
            Some(q) => {
                let corner = |i: u32| self.poly.points()[verts[i as usize] as usize];
                match hint {
                    Some(h) if !within(&q.best.map(corner), face.normal, h) => {
                        let nearest = (0..n).min_by(|&a, &b| (corner(a as u32) - h).length_squared().total_cmp(&(corner(b as u32) - h).length_squared())).unwrap_or(0);
                        q.with[nearest]
                    }
                    _ => q.best,
                }
            }
        };
        let count = n.min(4);
        for (slot, &i) in pick[..count].iter().enumerate() {
            let v = verts[i as usize];
            out.vertices[slot] = self.poly.points()[v as usize];
            out.vids[slot] = PackedFeatureId::vertex(v);
            out.eids[slot] = PackedFeatureId::edge(edges[i as usize]);
        }
        out.fid = PackedFeatureId::face(best as u32);
        out.num_vertices = count;
    }
}

impl SupportMap for Hull {
    fn local_support_point(&self, dir: Vector) -> Vector {
        self.poly.local_support_point(dir)
    }
}

impl PolygonalFeatureMap for Hull {
    fn local_support_feature(&self, dir: Vector, out: &mut PolygonalFeature) {
        self.feature(dir, None, out);
    }

    fn local_support_feature_toward(&self, dir: Vector, hint: Vector, out: &mut PolygonalFeature) {
        self.feature(dir, Some(hint), out);
    }

    fn is_convex_polyhedron(&self) -> bool {
        true
    }
}

impl RayCast for Hull {
    fn cast_local_ray_and_get_normal(&self, ray: &Ray, max_time_of_impact: Real, solid: bool) -> Option<RayIntersection> {
        self.poly.cast_local_ray_and_get_normal(ray, max_time_of_impact, solid)
    }
}

impl PointQuery for Hull {
    fn project_local_point(&self, point: Vector, solid: bool) -> PointProjection {
        self.poly.project_local_point(point, solid)
    }

    fn project_local_point_and_get_feature(&self, point: Vector) -> (PointProjection, FeatureId) {
        self.poly.project_local_point_and_get_feature(point)
    }
}

impl Shape for Hull {
    fn compute_local_aabb(&self) -> Aabb {
        self.poly.compute_local_aabb()
    }

    fn compute_local_bounding_sphere(&self) -> BoundingSphere {
        self.poly.compute_local_bounding_sphere()
    }

    fn compute_aabb(&self, position: &Pose) -> Aabb {
        self.poly.compute_aabb(position)
    }

    fn clone_dyn(&self) -> Box<dyn Shape> {
        Box::new(self.clone())
    }

    fn scale_dyn(&self, scale: Vector, _num_subdivisions: u32) -> Option<Box<dyn Shape>> {
        Some(Box::new(Hull::new(self.poly.clone().scaled(scale)?)))
    }

    fn mass_properties(&self, density: Real) -> MassProperties {
        self.poly.mass_properties(density)
    }

    fn is_convex(&self) -> bool {
        true
    }

    fn shape_type(&self) -> ShapeType {
        ShapeType::Custom
    }

    fn as_typed_shape(&self) -> TypedShape<'_> {
        TypedShape::Custom(self)
    }

    fn ccd_thickness(&self) -> Real {
        self.poly.ccd_thickness()
    }

    fn ccd_angular_thickness(&self) -> Real {
        self.poly.ccd_angular_thickness()
    }

    fn as_support_map(&self) -> Option<&dyn SupportMap> {
        Some(self as &dyn SupportMap)
    }

    fn as_polygonal_feature_map(&self) -> Option<(&dyn PolygonalFeatureMap, Real)> {
        Some((self as &dyn PolygonalFeatureMap, 0.0))
    }

    fn feature_normal_at_point(&self, subshape: SubShapeId, feature: FeatureId, point: Vector) -> Option<Vector> {
        self.poly.feature_normal_at_point(subshape, feature, point)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A slab `w` by `d` and `h` thick whose top and bottom are `n`-gons (a round table top
    /// when `n` is large; corners on the ellipse through the slab's corners otherwise).
    fn slab(n: usize, w: f32, d: f32, h: f32) -> Vec<Vector> {
        let mut pts = Vec::new();
        for i in 0..n {
            let a = std::f32::consts::TAU * (i as f32 + 0.5) / n as f32;
            for z in [0.0, h] {
                pts.push(Vector::new(w * a.cos(), d * a.sin(), z));
            }
        }
        pts
    }

    fn top_feature(shape: &dyn PolygonalFeatureMap, hint: Option<Vector>) -> Vec<Vector> {
        let mut f = PolygonalFeature::default();
        match hint {
            Some(h) => shape.local_support_feature_toward(Vector::Z, h, &mut f),
            None => shape.local_support_feature(Vector::Z, &mut f),
        }
        f.vertices[..f.num_vertices].to_vec()
    }

    /// Area of a convex polygon in the xy plane.
    fn area(p: &[Vector]) -> f32 {
        (0..p.len()).map(|i| p[i].x * p[(i + 1) % p.len()].y - p[(i + 1) % p.len()].x * p[i].y).sum::<f32>().abs() / 2.0
    }

    #[test]
    fn a_large_face_gives_the_four_corners_spanning_most_of_it() {
        // A 12-gon top: parry's four are a third of the rim, under a quarter of the face;
        // the best four are its inscribed square (64 % of a circle; here of the 12-gon, 67 %).
        let poly = ConvexPolyhedron::from_convex_hull(&slab(12, 1.0, 1.0, 0.1)).unwrap();
        let top = poly.faces().iter().find(|f| f.normal.z > 0.99).unwrap();
        assert_eq!(top.num_vertices_or_edges, 12);
        let full = 3.0; // 12-gon of radius 1: 3 r²
        let plain = area(&top_feature(&poly, None));
        let hull = Hull::new(poly.clone());
        let best = area(&top_feature(&hull, None));
        assert!(plain < 0.25 * full, "parry's four: {plain}");
        assert!(best > 0.65 * full, "the best four: {best}");
        // A face of four corners or fewer (a side) is given as parry gives it.
        let side = |s: &dyn PolygonalFeatureMap| {
            let mut f = PolygonalFeature::default();
            s.local_support_feature(Vector::X, &mut f);
            (f.vertices[..f.num_vertices].to_vec(), f.fid)
        };
        assert_eq!(side(&hull), side(&poly));
        assert_eq!(side(&hull).0.len(), 4);
    }

    #[test]
    fn a_contact_outside_the_best_four_gets_four_around_it() {
        let hull = Hull::new(ConvexPolyhedron::from_convex_hull(&slab(12, 1.0, 1.0, 0.1)).unwrap());
        // On the rim between two corners of the inscribed square.
        let mut rim = None;
        for k in 0..12 {
            let a = std::f32::consts::TAU * k as f32 / 12.0;
            let p = Vector::new(0.97 * a.cos(), 0.97 * a.sin(), 0.1);
            let q = top_feature(&hull, None);
            let n = Vector::Z;
            if !within(&[q[0], q[1], q[2], q[3]], n, p) {
                rim = Some(p);
                break;
            }
        }
        let p = rim.expect("a rim point outside the best four");
        let q = top_feature(&hull, Some(p));
        assert!(within(&[q[0], q[1], q[2], q[3]], Vector::Z, p) || q.iter().any(|c| (*c - p).length() < 0.3), "{q:?} around {p:?}");
        assert!(area(&q) > 0.6 * 3.0, "{}", area(&q));
    }

    /// The table of [`a_table_top_of_eleven_corners_is_all_but_whole`]: 100 by 70 by 40 game
    /// units, top at z = 40.
    fn table_points() -> Vec<Vector> {
        let top = [(-50.0, -30.0), (-45.0, -35.0), (45.0, -35.0), (50.0, -30.0), (50.0, 30.0), (45.0, 35.0), (10.0, 35.0), (0.0, 35.0), (-10.0, 35.0), (-45.0, 35.0), (-50.0, 30.0)];
        top.iter().flat_map(|&(x, y)| [Vector::new(x, y, 40.0), Vector::new(x, y, 0.0)]).collect()
    }

    /// A whetstone (37 by 13 by 7 game units, 25 kg) laid on the table's top where parry's
    /// four corners of it are not; where it is after `secs` (centre z, game units) and
    /// whether it fell asleep, in a world of rapier units (70 game units a metre).
    fn stone_on_table(table: SharedShape, secs: f32) -> (f32, bool) {
        use rapier3d::prelude::*;
        let k = 1.0 / 70.0;
        let mut w = PhysicsWorld::new();
        w.gravity = Vector::new(0.0, 0.0, -9.81);
        let t = w.insert_body(RigidBodyBuilder::fixed());
        w.insert_collider(ColliderBuilder::new(table), Some(t));
        // Away from parry's four: opposite their middle across the top's middle.
        let poly = ConvexPolyhedron::from_convex_hull(&table_points()).unwrap();
        let four = top_feature(&poly, None);
        let mid = four.iter().fold(Vector::ZERO, |a, b| a + *b) / 4.0;
        let at = Vector::new(-mid.x.signum() * 25.0, -mid.y.signum() * 15.0, 40.0 + 3.5);
        let s = w.insert_body(RigidBodyBuilder::dynamic().translation(at * k));
        let c = ColliderBuilder::cuboid(18.5 * k, 6.5 * k, 3.5 * k).friction(0.5).restitution(0.4).mass(25.0);
        w.insert_collider(c, Some(s));
        let mut asleep = false;
        for _ in 0..(secs * 60.0) as usize {
            w.step();
            asleep = w.bodies[s].is_sleeping();
        }
        (w.bodies[s].translation().z / k, asleep)
    }

    #[test]
    fn a_stone_on_a_table_top_of_many_corners_rests_on_it() {
        let k = 1.0 / 70.0;
        let pts: Vec<Vector> = table_points().into_iter().map(|p| p * k).collect();
        let hull = SharedShape(Arc::new(Hull::new(ConvexPolyhedron::from_convex_hull(&pts).unwrap())));
        let (z, asleep) = stone_on_table(hull, 3.0);
        assert!((z - 43.5).abs() < 0.5 && asleep, "with its whole top: centre at z {z} (resting: 43.5), asleep {asleep}");
        // parry's own hull clips the stone against four corners along one edge: one point
        // is left and the stone sinks [observed 2026-11-11: 8 units in 3 s]. Should a parry
        // update choose the corners itself, this fails and `Hull` can go.
        let plain = SharedShape(Arc::new(ConvexPolyhedron::from_convex_hull(&pts).unwrap()));
        let (z, _) = stone_on_table(plain, 3.0);
        assert!(z < 42.5, "parry's hull: centre at z {z}");
    }

    #[test]
    fn a_table_top_of_eleven_corners_is_all_but_whole() {
        // A rectangle 100 by 70 with its corners cut (8 corners) plus three on one edge.
        let mut top = vec![(-50.0, -30.0), (-45.0, -35.0), (45.0, -35.0), (50.0, -30.0), (50.0, 30.0), (45.0, 35.0), (10.0, 35.0), (0.0, 35.0), (-10.0, 35.0), (-45.0, 35.0), (-50.0, 30.0)];
        top.reverse();
        let mut pts = Vec::new();
        for (x, y) in &top {
            pts.push(Vector::new(*x, *y, 40.0));
            pts.push(Vector::new(*x, *y, 0.0));
        }
        let poly = ConvexPolyhedron::from_convex_hull(&pts).unwrap();
        let full = 100.0 * 70.0 - 4.0 * 12.5;
        let plain = area(&top_feature(&poly, None));
        let a = area(&top_feature(&Hull::new(poly), None));
        assert!(plain < 0.5 * full, "parry's four: {plain}");
        assert!(a > 0.9 * full, "{a}");
    }
}
