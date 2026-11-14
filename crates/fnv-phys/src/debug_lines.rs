//! A wireframe of the world's collision shapes, for the engine's collision view
//! (`tcg`, docs/spec-notes/physics.md §9): line segments in game units, each with the
//! [`Category`] of what it belongs to, limited to a sphere around a point.
//!
//! The outlines come from rapier's debug-render pipeline (`DebugRenderPipeline`, its
//! `render_colliders` only: no joints, contacts or body axes), which outlines every shape
//! rapier has: heightfields and triangle meshes triangle by triangle, convex hulls by
//! their edges, boxes, balls and capsules from its own outlines. A collider whose bounding
//! box lies wholly outside the sphere is skipped before it is outlined; of the rest, the
//! segments that pass within the sphere are kept, up to a maximum.
//!
//! It only reads the world: stepping, queries, traces and `PhysWorld::state_digest` are the
//! same whether it is used or not. The poses are those of the last step (or of the last
//! `pose_ref_object` for keyframed bodies), not the interpolated ones the engine draws
//! characters and ragdolls at, so a moving body's outline can trail its model by up to a
//! step.

use crate::convert::arr3;
use crate::gore::GORE_PART;
use crate::layers::{self, LayerSet};
use crate::world::{Owner, PhysWorld};
use rapier3d::pipeline::{DebugColor, DebugRenderBackend, DebugRenderMode, DebugRenderObject, DebugRenderPipeline, DebugRenderStyle};
use rapier3d::prelude::{Collider, ColliderHandle};

/// What a segment's collider belongs to. The order is the order of
/// [`CollisionLines::counts`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// A cell's heightfield.
    Terrain,
    /// A placed reference's collision that never moves (fixed, not keyframed).
    Static,
    /// A reference's keyframed parts (doors, animated collision), posed from their
    /// animation.
    Keyframed,
    /// A dynamic body (clutter, dropped items, tumbleweeds) that is awake.
    DynamicAwake,
    /// A dynamic body asleep.
    DynamicAsleep,
    /// A character controller's capsule (the player, walking NPCs).
    Character,
    /// An actor's hit volume (the whole capsule or its part capsules).
    HitVolume,
    /// A ragdoll bone.
    Ragdoll,
    /// A gore piece.
    Gore,
    /// A volume the character passes through (triggers, zones, picks; see
    /// [`volume_layers`]) or a rapier sensor.
    Trigger,
}

impl Category {
    pub const COUNT: usize = 10;
    pub const ALL: [Category; Category::COUNT] = [
        Category::Terrain,
        Category::Static,
        Category::Keyframed,
        Category::DynamicAwake,
        Category::DynamicAsleep,
        Category::Character,
        Category::HitVolume,
        Category::Ragdoll,
        Category::Gore,
        Category::Trigger,
    ];

    /// The category's place in [`Category::ALL`].
    pub fn index(self) -> usize {
        self as usize
    }

    /// Terrain and static collision: outlines that move only when their cell or reference
    /// is loaded again.
    pub fn is_still(self) -> bool {
        matches!(self, Category::Terrain | Category::Static)
    }

    /// A short name for logs.
    pub fn name(self) -> &'static str {
        match self {
            Category::Terrain => "terrain",
            Category::Static => "static",
            Category::Keyframed => "keyframed",
            Category::DynamicAwake => "awake",
            Category::DynamicAsleep => "asleep",
            Category::Character => "characters",
            Category::HitVolume => "hit volumes",
            Category::Ragdoll => "ragdolls",
            Category::Gore => "gore",
            Category::Trigger => "triggers",
        }
    }
}

/// One line of the wireframe, game units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub category: Category,
}

/// The layers whose reference colliders count as [`Category::Trigger`]: the volumes the
/// character passes through ([`layers::character_ignores`]) less the small debris and shell
/// casings, which are objects [design].
pub fn volume_layers() -> LayerSet {
    let mut s = LayerSet::default();
    for l in layers::character_ignores().layers() {
        if l != layers::DEBRIS_SMALL && l != layers::SHELL_CASING {
            s.insert(l);
        }
    }
    s
}

/// The category of a collider of `world`.
pub(crate) fn category(world: &PhysWorld, co: &Collider, volumes: &LayerSet) -> Category {
    if co.is_sensor() {
        return Category::Trigger;
    }
    let body = co.parent().and_then(|h| world.w.bodies.get(h).map(|b| (h, b)));
    if body.is_some_and(|(h, _)| world.ragdoll_of.contains_key(&h)) {
        return Category::Ragdoll;
    }
    match Owner::decode(co.user_data) {
        Some(Owner::Terrain { .. }) => Category::Terrain,
        Some(Owner::Character { .. }) => Category::Character,
        Some(Owner::Actor { part: GORE_PART, .. }) => Category::Gore,
        Some(Owner::Actor { .. }) => Category::HitVolume,
        Some(Owner::Ref { form_id, layer, .. }) => {
            if volumes.contains(layer) {
                return Category::Trigger;
            }
            match body {
                Some((_, b)) if b.is_dynamic() => {
                    if b.is_sleeping() {
                        Category::DynamicAsleep
                    } else {
                        Category::DynamicAwake
                    }
                }
                Some((h, _)) if world.keyframed.get(&form_id).is_some_and(|v| v.contains(&h)) => Category::Keyframed,
                _ => Category::Static,
            }
        }
        None => Category::Static,
    }
}

/// The wireframe of the last [`CollisionLines::gather`], and rapier's pipeline kept for
/// the next one (it holds the outlines of the unit ball and box).
pub struct CollisionLines {
    pipeline: DebugRenderPipeline,
    volumes: LayerSet,
    pub segments: Vec<Segment>,
    /// Segments within the sphere left out because the maximum was reached.
    pub dropped: usize,
    /// Colliders outlined (their bounding box reached the sphere).
    pub colliders: usize,
    /// Leave out a character whose bounding box holds the centre: with the centre at the
    /// camera, the player's own capsule in first person, which would fence the view in.
    pub hide_character_at_center: bool,
}

impl Default for CollisionLines {
    fn default() -> Self {
        CollisionLines {
            pipeline: DebugRenderPipeline::new(DebugRenderStyle::default(), DebugRenderMode::COLLIDER_SHAPES),
            volumes: volume_layers(),
            segments: Vec::new(),
            dropped: 0,
            colliders: 0,
            hide_character_at_center: false,
        }
    }
}

impl CollisionLines {
    /// Replace the segments with those of `world`'s colliders that pass within `radius`
    /// game units of `center`, at most `max` of them.
    pub fn gather(&mut self, world: &PhysWorld, center: [f32; 3], radius: f32, max: usize) {
        self.gather_only(world, center, radius, max, &Category::ALL);
    }

    /// [`CollisionLines::gather`] for the colliders of the categories in `kinds` only (a
    /// caller can keep the outlines that never move, [`Category::is_still`], and gather the
    /// rest each step).
    pub fn gather_only(&mut self, world: &PhysWorld, center: [f32; 3], radius: f32, max: usize, kinds: &[Category]) {
        self.segments.clear();
        let mut keep = [false; Category::COUNT];
        for c in kinds {
            keep[c.index()] = true;
        }
        let k = world.k;
        let mut g = Gather {
            world,
            volumes: &self.volumes,
            center: center.map(|v| v * k),
            radius: radius.max(0.0) * k,
            to_game: 1.0 / k,
            out: &mut self.segments,
            max,
            dropped: 0,
            colliders: 0,
            last: None,
            keep,
            hide_character_at_center: self.hide_character_at_center,
        };
        self.pipeline.render_colliders(&mut g, &world.w.bodies, &world.w.colliders);
        // rapier draws no shape of its users' own: the hulls with large faces
        // (`crate::hull`) are drawn here, edge by edge as rapier draws a hull.
        for (h, co) in world.w.colliders.iter() {
            let Some(hull) = co.shape().as_shape::<crate::hull::Hull>() else { continue };
            let object = DebugRenderObject::Collider(h, co);
            if g.filter_object(object) {
                let poly = hull.polyhedron();
                let edges: Vec<[u32; 2]> = poly.edges().iter().map(|e| e.vertices).collect();
                g.draw_polyline(object, poly.points(), &edges, co.position(), rapier3d::math::Vector::splat(1.0), [0.0; 4]);
            }
        }
        self.dropped = g.dropped;
        self.colliders = g.colliders;
    }

    /// How many segments each category has, in [`Category::ALL`]'s order.
    pub fn counts(&self) -> [usize; Category::COUNT] {
        let mut n = [0; Category::COUNT];
        for s in &self.segments {
            n[s.category.index()] += 1;
        }
        n
    }
}

impl PhysWorld {
    /// The collision wireframe within `radius` game units of `center`, at most `max`
    /// segments ([`CollisionLines::gather`]; keep a [`CollisionLines`] to gather again
    /// without allocating).
    pub fn collision_lines(&self, center: [f32; 3], radius: f32, max: usize) -> Vec<Segment> {
        let mut l = CollisionLines::default();
        l.gather(self, center, radius, max);
        l.segments
    }

    /// How many colliders the world has (a caller keeping still outlines gathers them again
    /// when this changes: a cell or reference came or went).
    pub fn collider_count(&self) -> usize {
        self.w.colliders.len()
    }
}

/// rapier's backend: classifies, filters and converts the lines it is given.
struct Gather<'a> {
    world: &'a PhysWorld,
    volumes: &'a LayerSet,
    /// rapier units (metres).
    center: [f32; 3],
    radius: f32,
    to_game: f32,
    out: &'a mut Vec<Segment>,
    max: usize,
    dropped: usize,
    colliders: usize,
    /// The collider of the last line and its category: rapier draws a collider's lines
    /// one after another.
    last: Option<(ColliderHandle, Category)>,
    /// The categories gathered.
    keep: [bool; Category::COUNT],
    hide_character_at_center: bool,
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// Squared distance from `p` to the segment `a`–`b`.
fn segment_distance2(p: [f32; 3], a: [f32; 3], b: [f32; 3]) -> f32 {
    let ab = sub(b, a);
    let len2 = dot(ab, ab);
    let t = if len2 > 0.0 { (dot(sub(p, a), ab) / len2).clamp(0.0, 1.0) } else { 0.0 };
    let q = [a[0] + ab[0] * t, a[1] + ab[1] * t, a[2] + ab[2] * t];
    let d = sub(p, q);
    dot(d, d)
}

impl DebugRenderBackend for Gather<'_> {
    fn filter_object(&self, object: DebugRenderObject) -> bool {
        let DebugRenderObject::Collider(_, co) = object else { return false };
        let aabb = co.compute_aabb();
        let (lo, hi) = (arr3(aabb.mins), arr3(aabb.maxs));
        // Distance from the centre to the box (zero inside it).
        let d2: f32 = (0..3).map(|i| (lo[i] - self.center[i]).max(0.0).max(self.center[i] - hi[i])).map(|v| v * v).sum();
        if d2 > self.radius * self.radius {
            return false;
        }
        let c = category(self.world, co, self.volumes);
        self.keep[c.index()] && !(self.hide_character_at_center && c == Category::Character && d2 == 0.0)
    }

    fn draw_line(&mut self, object: DebugRenderObject, a: rapier3d::math::Vector, b: rapier3d::math::Vector, _color: DebugColor) {
        let DebugRenderObject::Collider(h, co) = object else { return };
        let category = match self.last {
            Some((last, c)) if last == h => c,
            _ => {
                let c = category(self.world, co, self.volumes);
                self.last = Some((h, c));
                self.colliders += 1;
                c
            }
        };
        let (a, b) = (arr3(a), arr3(b));
        if !(a.iter().chain(&b).all(|v| v.is_finite())) || segment_distance2(self.center, a, b) > self.radius * self.radius {
            return;
        }
        if self.out.len() >= self.max {
            self.dropped += 1;
            return;
        }
        self.out.push(Segment { a: a.map(|v| v * self.to_game), b: b.map(|v| v * self.to_game), category });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CharacterParams, HitVolume, PhysParams, PreparedRef, RefPose};
    use fnv_collide::{CollisionObject, Iso, Motion, Shape};
    use rapier3d::prelude::RigidBodyBuilder;

    const ID: [f32; 9] = [1.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0];

    fn object(shape: Shape, motion: Motion, layer: u8) -> CollisionObject {
        CollisionObject {
            node: None,
            node_block: 0,
            node_transform: Iso::IDENTITY,
            body_transform: Iso::IDENTITY,
            motion,
            layer,
            mass: 10.0,
            friction: 0.5,
            restitution: 0.0,
            shape: shape.into(),
            shell: 0.0,
            material: None,
            wind: false,
        }
    }

    fn at(form_id: u32, position: [f32; 3]) -> RefPose {
        RefPose { form_id, position, rotation: ID, scale: 1.0 }
    }

    fn cube(form_id: u32, position: [f32; 3], motion: Motion, layer: u8) -> (RefPose, Vec<CollisionObject>) {
        (at(form_id, position), vec![object(Shape::Box { half_extents: [10.0; 3] }, motion, layer)])
    }

    fn flat_cell() -> Vec<f32> {
        vec![0.0; fnv_world::land::SIDE * fnv_world::land::SIDE]
    }

    /// One of everything, spread along x 300 units apart, on flat terrain.
    fn everything() -> PhysWorld {
        let mut w = PhysWorld::new(PhysParams::default());
        w.add_terrain((0, 0), &flat_cell());
        for (id, x, motion, layer) in [(1, 300.0, Motion::Static, layers::STATIC), (2, 600.0, Motion::Keyframed, layers::ANIM_STATIC), (3, 900.0, Motion::Dynamic, layers::CLUTTER), (5, 1500.0, Motion::Static, layers::TRIGGER)] {
            let (p, o) = cube(id, [x, 300.0, 100.0], motion, layer);
            w.add_ref(&p, &o);
        }
        let (p, o) = cube(4, [1200.0, 300.0, 100.0], Motion::Dynamic, layers::CLUTTER);
        w.insert_ref(PreparedRef::new(&w.params, &p, &o).asleep());
        w.add_character(7, CharacterParams::default(), [1800.0, 300.0, 0.0]);
        w.set_actor_volume(8, HitVolume { feet: [2100.0, 300.0, 0.0], radius: 20.0, height: 120.0, dead: false });
        w.add_gore(&crate::gore::GoreStart {
            id: 1,
            actor: 9,
            translation: [2400.0, 300.0, 50.0],
            rotation: ID,
            half_extents: [5.0; 3],
            center: [0.0; 3],
            linvel: [0.0; 3],
            angvel: [0.0; 3],
            mass: 1.0,
        });
        // A ragdoll bone: a dynamic body the world knows as one.
        let k = w.k;
        let body = w.insert_body(RigidBodyBuilder::dynamic().translation(rapier3d::math::Vector::new(2700.0 * k, 300.0 * k, 100.0 * k)));
        let owner = Owner::Actor { form_id: 10, layer: layers::DEAD_BIP, part: 0 }.encode();
        w.w.insert_collider(rapier3d::prelude::ColliderBuilder::ball(10.0 * k).user_data(owner), Some(body));
        w.ragdoll_of.insert(body, (10, 0));
        // Places the standalone hit volumes.
        w.update_queries();
        w
    }

    #[test]
    fn every_category_gets_segments() {
        let w = everything();
        let mut l = CollisionLines::default();
        l.gather(&w, [1500.0, 300.0, 100.0], 10_000.0, usize::MAX);
        let n = l.counts();
        for c in Category::ALL {
            assert!(n[c.index()] > 0, "{} has no segments: {n:?}", c.name());
        }
        assert_eq!(l.dropped, 0);
        assert!(l.segments.iter().all(|s| s.a.iter().chain(&s.b).all(|v| v.is_finite())));
    }

    #[test]
    fn segments_are_in_game_units() {
        let w = everything();
        // The static cube: 20 units across, centred at (300, 300, 100).
        let lines = w.collision_lines([300.0, 300.0, 100.0], 10_000.0, usize::MAX);
        let cube: Vec<&Segment> = lines.iter().filter(|s| s.category == Category::Static).collect();
        assert_eq!(cube.len(), 12, "a box outlines its 12 edges");
        for s in cube {
            for p in [s.a, s.b] {
                for (v, c) in p.iter().zip([300.0, 300.0, 100.0]) {
                    assert!(((v - c).abs() - 10.0).abs() < 1e-3, "{p:?} is not a corner");
                }
            }
        }
    }

    #[test]
    fn the_radius_keeps_only_nearby_segments() {
        let w = everything();
        let (center, radius) = ([900.0, 300.0, 100.0], 150.0);
        let lines = w.collision_lines(center, radius, usize::MAX);
        assert!(!lines.is_empty());
        for s in &lines {
            assert!(segment_distance2(center, s.a, s.b) <= radius * radius + 1e-2, "{s:?}");
        }
        // The awake cube and the terrain under it; nothing 300 units off.
        let cats: std::collections::BTreeSet<Category> = lines.iter().map(|s| s.category).collect();
        assert_eq!(cats, [Category::Terrain, Category::DynamicAwake].into_iter().collect());
        // Far above everything: nothing.
        assert!(w.collision_lines([0.0, 0.0, 50_000.0], 1_000.0, usize::MAX).is_empty());
    }

    #[test]
    fn the_maximum_drops_the_rest() {
        let w = everything();
        let mut l = CollisionLines::default();
        l.gather(&w, [1500.0, 300.0, 100.0], 10_000.0, 50);
        assert_eq!(l.segments.len(), 50);
        assert!(l.dropped > 0);
    }

    #[test]
    fn gathering_changes_nothing() {
        let mut w = everything();
        let before = w.state_digest();
        let _ = w.collision_lines([0.0; 3], 1e6, usize::MAX);
        assert_eq!(w.state_digest(), before);
        w.step();
        let mut twin = everything();
        twin.step();
        let _ = twin.collision_lines([0.0; 3], 1e6, usize::MAX);
        assert_eq!(w.state_digest(), twin.state_digest());
    }

    #[test]
    fn gathering_by_kind_splits_the_lines() {
        let w = everything();
        let (center, radius) = ([1500.0, 300.0, 100.0], 10_000.0);
        let all = w.collision_lines(center, radius, usize::MAX);
        let still: Vec<Category> = Category::ALL.into_iter().filter(|c| c.is_still()).collect();
        let moving: Vec<Category> = Category::ALL.into_iter().filter(|c| !c.is_still()).collect();
        let mut l = CollisionLines::default();
        l.gather_only(&w, center, radius, usize::MAX, &still);
        let a = l.segments.clone();
        assert!(a.iter().all(|s| s.category.is_still()) && !a.is_empty());
        l.gather_only(&w, center, radius, usize::MAX, &moving);
        assert!(l.segments.iter().all(|s| !s.category.is_still()) && !l.segments.is_empty());
        assert_eq!(a.len() + l.segments.len(), all.len());
        assert!(w.collider_count() >= l.colliders);
    }

    #[test]
    fn the_character_around_the_centre_can_be_hidden() {
        let w = everything();
        // Inside the character standing at (1800, 300, 0), at about eye height.
        let eye = [1800.0, 300.0, 60.0];
        let mut l = CollisionLines::default();
        l.gather(&w, eye, 400.0, usize::MAX);
        assert!(l.counts()[Category::Character.index()] > 0);
        l.hide_character_at_center = true;
        l.gather(&w, eye, 400.0, usize::MAX);
        assert_eq!(l.counts()[Category::Character.index()], 0);
        assert!(l.counts()[Category::Terrain.index()] > 0);
        // From outside it, it is drawn.
        l.gather(&w, [1800.0, 600.0, 60.0], 400.0, usize::MAX);
        assert!(l.counts()[Category::Character.index()] > 0);
    }

    #[test]
    fn volumes_leave_out_debris() {
        let v = volume_layers();
        assert!(v.contains(layers::TRIGGER) && v.contains(layers::ACTOR_ZONE));
        assert!(!v.contains(layers::DEBRIS_SMALL) && !v.contains(layers::SHELL_CASING) && !v.contains(layers::STATIC));
    }
}
