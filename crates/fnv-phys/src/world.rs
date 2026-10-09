//! The physics world: terrain heightfields, placed references' collision bodies,
//! characters, fixed-step stepping and scene queries over rapier3d.
//!
//! Determinism (docs/spec-notes/physics.md §5): rapier is built with
//! `enhanced-determinism`; the world advances only in fixed steps of `PhysParams::dt`;
//! bodies are created in the order the caller adds them, so callers add terrain by grid
//! and references by FormID (the `fnvtool phys` commands do). Query results that can
//! list several colliders are sorted. Shapes can be built away from the world
//! ([`PreparedRef`], [`PreparedTerrain`], e.g. on a loader thread) and inserted later;
//! the order of insertion is what counts.

use crate::convert::{self, arr3, vec3, ConvertError, Leaf};
use fnv_collide::{CollisionObject, Iso, Motion, Shape};
use rapier3d::math::{Pose, Vector};
use rapier3d::parry::query::ShapeCastOptions;
use rapier3d::prelude::*;
use std::collections::{BTreeMap, HashMap};

/// World parameters. The defaults are engineering choices or [hypothesis] values to be
/// replaced by captured ones (physics.md §6 lists the capture questions).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct PhysParams {
    /// Fixed step in seconds [hypothesis: the original's physics rate is unknown].
    pub dt: f32,
    /// Downward acceleration in game units/s²: 9.81 m/s² at 70 units per metre, which the
    /// game agrees with [observed: 683.8 from a 300-unit fall and 685.5 from a jump, fitted
    /// to positions against the game clock, capture mv_trial, physics.md §6.3].
    pub gravity: f32,
    /// Game units per metre. rapier runs in metres internally because several of its
    /// tolerances are absolute and tuned for metres (e.g. the character controller's
    /// ground-detection margin); lengths are divided by this on the way in and multiplied
    /// on the way out. A numerical choice, not a game fact [hypothesis: 70].
    pub units_per_metre: f32,
    /// Most fixed steps one [`Stepper::advance`] call runs; time beyond is dropped.
    pub max_substeps: u32,
}

impl Default for PhysParams {
    fn default() -> Self {
        PhysParams { dt: 1.0 / 60.0, gravity: 9.81 * 70.0, units_per_metre: 70.0, max_substeps: 8 }
    }
}

/// Contact shell of the terrain, game units: actors rest about this high above the LAND
/// surface, as they rest about 0.7 units (a stored shape radius of 0.1 times the Havok
/// scale) above static models (physics.md §6.2). Derived from in-game rest heights
/// [observed: after door teleports and drops the player rested 3.32–3.85 units above the
/// triangulated LAND surface and 0.54–1.05 above static floors, the same band shifted by
/// 2.8 units; 0.7 + 2.8 = 3.5]. That the excess is a shell (a Havok convex radius of the
/// landscape shape) rather than a raised surface is [hypothesis]; queries still report
/// the bare surface.
pub const TERRAIN_SHELL: f32 = 3.5;

/// What a collider belongs to. Stored in the collider's `user_data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Owner {
    /// The heightfield of the exterior cell at this grid square.
    Terrain { x: i32, y: i32 },
    /// Collision object `object` (index in the list given to [`PhysWorld::add_ref`]) of a
    /// placed reference, with its Havok collision layer.
    Ref { form_id: u32, object: u16, layer: u8 },
    /// A character added with [`PhysWorld::add_character`].
    Character { id: u32 },
}

impl Owner {
    pub fn encode(self) -> u128 {
        let (kind, a, b, c) = match self {
            Owner::Terrain { x, y } => (1u128, x as u32, y as u32, 0u32),
            Owner::Ref { form_id, object, layer } => (2, form_id, object as u32, layer as u32),
            Owner::Character { id } => (3, id, 0, 0),
        };
        kind | (a as u128) << 8 | (b as u128) << 40 | (c as u128) << 72
    }

    pub fn decode(v: u128) -> Option<Owner> {
        let a = (v >> 8) as u32;
        let b = (v >> 40) as u32;
        let c = (v >> 72) as u32;
        match v & 0xFF {
            1 => Some(Owner::Terrain { x: a as i32, y: b as i32 }),
            2 => Some(Owner::Ref { form_id: a, object: b as u16, layer: c as u8 }),
            3 => Some(Owner::Character { id: a }),
            _ => None,
        }
    }

    /// The entity this collider belongs to, ignoring which of its objects it is.
    pub fn key(self) -> Key {
        match self {
            Owner::Terrain { x, y } => Key::Terrain(x, y),
            Owner::Ref { form_id, .. } => Key::Ref(form_id),
            Owner::Character { id } => Key::Character(id),
        }
    }
}

/// An entity in the world, for filters and removal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Key {
    Terrain(i32, i32),
    Ref(u32),
    Character(u32),
}

/// Placement of a reference: `p' = position + rotation · (scale · p)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RefPose {
    pub form_id: u32,
    pub position: [f32; 3],
    /// Row-major 3×3.
    pub rotation: [f32; 9],
    pub scale: f32,
}

impl RefPose {
    /// From a Layer 3 placement, with the engine's rotation convention
    /// (docs/spec-notes/world.md §4).
    pub fn from_placement(p: &fnv_world::Placement) -> RefPose {
        RefPose {
            form_id: p.form_id,
            position: p.position,
            rotation: fnv_world::transform::ref_rotation(p.rotation),
            scale: p.scale,
        }
    }
}

/// What [`PhysWorld::add_ref`] built.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AddReport {
    pub bodies: usize,
    pub colliders: usize,
    /// Leaves (or whole objects, for a bad placement) that could not be converted.
    pub errors: Vec<ConvertError>,
}

/// Counts for reports.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct Stats {
    pub terrain_cells: usize,
    pub refs: usize,
    pub bodies_fixed: usize,
    pub bodies_kinematic: usize,
    pub bodies_dynamic: usize,
    /// Colliders by rapier shape type name (`"cuboid"`, `"heightfield"`, ...).
    pub colliders: BTreeMap<String, usize>,
    pub steps: u64,
}

/// State of one body, for readback (e.g. dynamic clutter positions).
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct BodyState {
    pub translation: [f32; 3],
    /// Row-major 3×3.
    pub rotation: [f32; 9],
    pub linvel: [f32; 3],
    pub sleeping: bool,
}

/// A reference's collision objects converted to rapier shapes and placed, but not in a
/// world yet: built by [`PreparedRef::new`] (pure, so it may run on another thread) and
/// added by [`PhysWorld::insert_ref`]. Building the shapes (triangle-mesh trees, convex
/// hulls) is most of the cost of adding a reference; inserting is cheap.
pub struct PreparedRef {
    pub form_id: u32,
    objects: Vec<PreparedObject>,
    /// Conversion errors, in the order [`PhysWorld::add_ref`] reports them.
    errors: Vec<ConvertError>,
}

struct PreparedObject {
    /// Index in the list given to [`PreparedRef::new`].
    index: u16,
    layer: u8,
    motion: Motion,
    material: Material,
    /// Body pose in the world, rapier units.
    pose: Pose,
    leaves: Vec<Leaf>,
    /// Contact shell times the reference's scale, game units.
    shell: f32,
}

/// The surface and mass properties of a collision object.
struct Material {
    friction: f32,
    restitution: f32,
    mass: f32,
}

impl PreparedRef {
    /// Convert `objects` placed by `pose` for a world with `params` (see
    /// [`PhysWorld::add_ref`]). Objects without convertible leaves are left out.
    pub fn new(params: &PhysParams, pose: &RefPose, objects: &[CollisionObject]) -> PreparedRef {
        PreparedRef::build(1.0 / params.units_per_metre, pose, objects)
    }

    /// With `k` metres per game unit.
    fn build(k: f32, pose: &RefPose, objects: &[CollisionObject]) -> PreparedRef {
        let mut out = PreparedRef { form_id: pose.form_id, objects: Vec::new(), errors: Vec::new() };
        let ref_rot = match convert::rotation(&pose.rotation) {
            Ok(r) => r,
            Err(e) => {
                out.errors.push(e);
                return out;
            }
        };
        let ref_pose = Pose::from_parts(vec3(pose.position) * k, ref_rot);
        for (i, o) in objects.iter().enumerate() {
            let local = o.node_transform.compose(&o.body_transform);
            let body_pose = match convert::pose(&local, pose.scale * k) {
                Ok(p) => ref_pose * p,
                Err(e) => {
                    out.errors.push(e);
                    continue;
                }
            };
            let mut leaves = Vec::new();
            convert::leaves(&o.shape, &Iso::IDENTITY, pose.scale * k, &mut leaves, &mut out.errors);
            if leaves.is_empty() {
                continue;
            }
            out.objects.push(PreparedObject {
                index: i as u16,
                layer: o.layer,
                motion: o.motion,
                material: Material { friction: o.friction, restitution: o.restitution, mass: o.mass },
                pose: body_pose,
                leaves,
                shell: o.shell * pose.scale,
            });
        }
        out
    }

    /// Colliders it will add.
    pub fn colliders(&self) -> usize {
        self.objects.iter().map(|o| o.leaves.len()).sum()
    }
}

/// The terrain collider of an exterior cell, built but not in a world yet (see
/// [`PreparedRef`]; added by [`PhysWorld::insert_terrain`]).
pub struct PreparedTerrain {
    pub grid: (i32, i32),
    collider: ColliderBuilder,
}

impl PreparedTerrain {
    /// The terrain of the cell at `grid` from its LAND heights, for a world with `params`.
    pub fn new(params: &PhysParams, grid: (i32, i32), heights: &[f32]) -> PreparedTerrain {
        PreparedTerrain::build(1.0 / params.units_per_metre, grid, heights)
    }

    fn build(k: f32, grid: (i32, i32), heights: &[f32]) -> PreparedTerrain {
        PreparedTerrain { grid, collider: crate::terrain::collider(grid, heights, k) }
    }
}

pub struct PhysWorld {
    pub params: PhysParams,
    pub(crate) w: PhysicsWorld,
    /// Metres per game unit (rapier's internal length unit).
    pub(crate) k: f32,
    terrain: BTreeMap<(i32, i32), ColliderHandle>,
    refs: BTreeMap<u32, Vec<RigidBodyHandle>>,
    /// Per-triangle material codes of triangle-mesh colliders.
    materials: HashMap<ColliderHandle, Vec<u32>>,
    /// Contact shells (game units) of reference colliders that have one
    /// (`CollisionObject::shell` times the reference's scale).
    shells: HashMap<ColliderHandle, f32>,
    /// Colliders changed since the broad phase was last updated.
    dirty: bool,
    /// Bodies inserted since the last step (see [`PhysWorld::update_queries`]).
    new_bodies: Vec<RigidBodyHandle>,
    steps: u64,
    /// Water: cell surfaces by area and placed volumes (`crate::water`).
    pub(crate) water: crate::water::Waters,
}

impl PhysWorld {
    pub fn new(params: PhysParams) -> PhysWorld {
        let k = 1.0 / params.units_per_metre;
        let mut w = PhysicsWorld::new();
        w.gravity = Vector::new(0.0, 0.0, -params.gravity * k);
        w.integration_parameters.dt = params.dt;
        PhysWorld {
            params,
            w,
            k,
            terrain: BTreeMap::new(),
            refs: BTreeMap::new(),
            materials: HashMap::new(),
            shells: HashMap::new(),
            dirty: false,
            new_bodies: Vec::new(),
            steps: 0,
            water: Default::default(),
        }
    }

    /// Add (or replace) the terrain of the exterior cell at `grid` from its LAND heights.
    pub fn add_terrain(&mut self, grid: (i32, i32), heights: &[f32]) {
        self.insert_terrain(PreparedTerrain::build(self.k, grid, heights));
    }

    /// Add (or replace) terrain built by [`PreparedTerrain::new`]: the same result as
    /// [`PhysWorld::add_terrain`] with the same heights.
    pub fn insert_terrain(&mut self, t: PreparedTerrain) {
        let grid = t.grid;
        self.remove_terrain(grid);
        // Dynamic bodies rest on the terrain's contact shell too (see `TERRAIN_SHELL`).
        let c = t.collider.user_data(Owner::Terrain { x: grid.0, y: grid.1 }.encode()).contact_skin(TERRAIN_SHELL * self.k);
        let h = self.w.insert_collider(c, None);
        self.terrain.insert(grid, h);
        self.dirty = true;
    }

    pub fn remove_terrain(&mut self, grid: (i32, i32)) -> bool {
        let Some(h) = self.terrain.remove(&grid) else { return false };
        self.w.remove_collider(h);
        self.dirty = true;
        true
    }

    /// Add the collision objects of a placed reference: one body per object, placed at
    /// `pose ∘ node_transform ∘ body_transform`, with the reference's uniform scale baked
    /// into the shapes. Static objects become fixed bodies, keyframed ones
    /// kinematic, dynamic ones dynamic with the object's mass, friction and restitution
    /// (physics.md §4). Replaces an earlier add of the same reference's bodies; its placed
    /// water ([`PhysWorld::add_placed_water`]) is kept, so the two can be added in either
    /// order.
    pub fn add_ref(&mut self, pose: &RefPose, objects: &[CollisionObject]) -> AddReport {
        self.insert_ref(PreparedRef::build(self.k, pose, objects))
    }

    /// Add a reference's collision converted by [`PreparedRef::new`]: the same bodies,
    /// colliders and report, in the same order, as [`PhysWorld::add_ref`] with the same
    /// pose and objects. Replaces an earlier add of the same reference's bodies; its placed
    /// water is kept (see [`PhysWorld::add_ref`]).
    pub fn insert_ref(&mut self, p: PreparedRef) -> AddReport {
        self.remove_bodies(p.form_id);
        let mut report = AddReport { errors: p.errors, ..Default::default() };
        let mut handles = Vec::new();
        for o in p.objects {
            let owner = Owner::Ref { form_id: p.form_id, object: o.index, layer: o.layer };
            let builder = match o.motion {
                Motion::Static => RigidBodyBuilder::fixed(),
                Motion::Keyframed => RigidBodyBuilder::kinematic_position_based(),
                Motion::Dynamic => RigidBodyBuilder::dynamic(),
            };
            let body = self.insert_body(builder.pose(o.pose).user_data(owner.encode()));
            report.colliders += self.attach(body, owner, &o.material, o.leaves, o.shell);
            if o.motion == Motion::Dynamic {
                self.set_mass(body, o.material.mass);
            }
            handles.push(body);
        }
        report.bodies = handles.len();
        if !handles.is_empty() {
            self.refs.insert(p.form_id, handles);
            self.dirty = true;
        }
        report
    }

    fn attach(&mut self, body: RigidBodyHandle, owner: Owner, o: &Material, leaves: Vec<Leaf>, shell: f32) -> usize {
        let n = leaves.len();
        for leaf in leaves {
            let mut c = ColliderBuilder::new(leaf.shape).position(leaf.pose).user_data(owner.encode());
            if o.friction.is_finite() && o.friction >= 0.0 {
                c = c.friction(o.friction);
            }
            if o.restitution.is_finite() && o.restitution >= 0.0 {
                c = c.restitution(o.restitution);
            }
            let h = self.w.insert_collider(c, Some(body));
            if !leaf.materials.is_empty() {
                self.materials.insert(h, leaf.materials);
            }
            if shell.is_finite() && shell > 0.0 {
                self.shells.insert(h, shell);
            }
        }
        n
    }

    /// Scale the colliders' densities so the body's mass is `mass`; leaves the
    /// volume-derived mass when `mass` is not positive or the shapes have no volume.
    fn set_mass(&mut self, body: RigidBodyHandle, mass: f32) {
        let b = &mut self.w.bodies[body];
        b.recompute_mass_properties_from_colliders(&self.w.colliders);
        let m0 = b.mass();
        if !(mass.is_finite() && mass > 0.0 && m0 > 0.0) {
            return;
        }
        let colliders: Vec<ColliderHandle> = b.colliders().to_vec();
        for c in colliders {
            let d = self.w.colliders[c].density();
            self.w.colliders[c].set_density(d * mass / m0);
        }
        self.w.bodies[body].recompute_mass_properties_from_colliders(&self.w.colliders);
    }

    /// Remove a reference from the world: its bodies and its placed water.
    pub fn remove_ref(&mut self, form_id: u32) -> bool {
        let water = self.water.remove_ref(form_id);
        self.remove_bodies(form_id) || water
    }

    /// Remove a reference's bodies only.
    fn remove_bodies(&mut self, form_id: u32) -> bool {
        let Some(bodies) = self.refs.remove(&form_id) else { return false };
        for b in bodies {
            self.remove_body(b);
        }
        self.dirty = true;
        true
    }

    pub(crate) fn remove_body(&mut self, b: RigidBodyHandle) {
        if let Some(body) = self.w.bodies.get(b) {
            for c in body.colliders() {
                self.materials.remove(c);
                self.shells.remove(c);
            }
        }
        self.w.remove_body(b);
        self.dirty = true;
    }

    /// Current state of each body of a reference, in object order.
    pub fn ref_bodies(&self, form_id: u32) -> Vec<BodyState> {
        self.refs.get(&form_id).into_iter().flatten().map(|&b| body_state(&self.w.bodies[b], self.k)).collect()
    }

    pub(crate) fn insert_body(&mut self, b: impl Into<RigidBody>) -> RigidBodyHandle {
        let h = self.w.insert_body(b);
        self.new_bodies.push(h);
        h
    }

    /// Advance one fixed step.
    pub fn step(&mut self) {
        self.w.step();
        self.dirty = false;
        self.new_bodies.clear();
        self.steps += 1;
    }

    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Bring the broad phase up to date after adding or removing bodies, so that queries
    /// see them, without advancing time.
    pub fn update_queries(&mut self) {
        if self.dirty {
            self.w.detect_collisions(&(), &());
            // rapier 0.36's collision-only pipeline consumes the change flags of bodies
            // inserted since the last step without registering them with the island
            // manager, so they would never move [observed: a dynamic box added, queried
            // and stepped stays put; test `bodies_added_then_queried_still_simulate`].
            // Flag them changed again so the next step registers them.
            for &h in &self.new_bodies {
                if let Some(b) = self.w.bodies.get_mut(h) {
                    let p = *b.position();
                    b.set_position(p, false);
                }
            }
            self.dirty = false;
        }
    }

    /// Scene queries on the current state (updates the broad phase first if needed).
    pub fn queries(&mut self) -> Queries<'_> {
        self.update_queries();
        Queries { world: self }
    }

    pub(crate) fn mark_dirty(&mut self) {
        self.dirty = true;
    }

    /// Bounding box (game units: min and max corners) of every collider on a fixed body —
    /// terrain and references' static collision, not characters or dynamic bodies; `None`
    /// when there are none.
    pub fn fixed_bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let mut out: Option<([f32; 3], [f32; 3])> = None;
        for (_, c) in self.w.colliders.iter() {
            if !c.parent().is_some_and(|b| self.w.bodies[b].is_fixed()) {
                continue;
            }
            let a = c.compute_aabb();
            let (lo, hi) = (arr3(a.mins / self.k), arr3(a.maxs / self.k));
            let b = out.get_or_insert((lo, hi));
            for i in 0..3 {
                b.0[i] = b.0[i].min(lo[i]);
                b.1[i] = b.1[i].max(hi[i]);
            }
        }
        out
    }

    pub fn stats(&self) -> Stats {
        let mut s = Stats { terrain_cells: self.terrain.len(), refs: self.refs.len(), steps: self.steps, ..Default::default() };
        for (_, b) in self.w.bodies.iter() {
            match b.body_type() {
                RigidBodyType::Fixed => s.bodies_fixed += 1,
                RigidBodyType::Dynamic => s.bodies_dynamic += 1,
                _ => s.bodies_kinematic += 1,
            }
        }
        for (_, c) in self.w.colliders.iter() {
            *s.colliders.entry(shape_name(c.shape().shape_type())).or_default() += 1;
        }
        s
    }

    /// A full copy of the world (bodies, colliders, contacts, islands, broad phase and the
    /// tables here). Stepping the copy gives bit-identical results to stepping the
    /// original [observed: test `snapshot_continues_identically`]; characters keep their
    /// handles, so a cloned [`crate::Character`] drives the copy.
    pub fn snapshot(&self) -> PhysWorld {
        let w = &self.w;
        PhysWorld {
            params: self.params,
            w: PhysicsWorld {
                gravity: w.gravity,
                integration_parameters: w.integration_parameters,
                physics_pipeline: PhysicsPipeline::new(),
                collision_pipeline: CollisionPipeline::new(),
                islands: w.islands.clone(),
                broad_phase: w.broad_phase.clone(),
                narrow_phase: w.narrow_phase.clone(),
                bodies: w.bodies.clone(),
                colliders: w.colliders.clone(),
                impulse_joints: w.impulse_joints.clone(),
                multibody_joints: w.multibody_joints.clone(),
                soft_bodies: w.soft_bodies.clone(),
                ccd_solver: CCDSolver::new(),
            },
            k: self.k,
            terrain: self.terrain.clone(),
            refs: self.refs.clone(),
            materials: self.materials.clone(),
            shells: self.shells.clone(),
            dirty: self.dirty,
            new_bodies: self.new_bodies.clone(),
            steps: self.steps,
            water: self.water.clone(),
        }
    }

    /// A digest of every body's position and velocity bits, in handle order: equal
    /// digests mean bit-identical states (determinism checks).
    pub fn state_digest(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: f32| {
            for byte in v.to_bits().to_le_bytes() {
                h ^= byte as u64;
                h = h.wrapping_mul(0x0100_0000_01b3);
            }
        };
        for (_, b) in self.w.bodies.iter() {
            let p = b.position();
            for v in [p.translation.x, p.translation.y, p.translation.z, p.rotation.x, p.rotation.y, p.rotation.z, p.rotation.w] {
                eat(v);
            }
            let l = b.linvel();
            for v in [l.x, l.y, l.z] {
                eat(v);
            }
        }
        h
    }

    /// Contact shell of a collider, game units: [`TERRAIN_SHELL`] for terrain, the
    /// collision object's shell for a reference's collider, 0 otherwise.
    pub(crate) fn shell_of(&self, c: ColliderHandle) -> f32 {
        match self.w.colliders.get(c).and_then(|co| Owner::decode(co.user_data)) {
            Some(Owner::Terrain { .. }) => TERRAIN_SHELL,
            _ => self.shells.get(&c).copied().unwrap_or(0.0),
        }
    }

    /// Material code of triangle `triangle` of a triangle-mesh collider.
    pub(crate) fn material_of(&self, c: ColliderHandle, triangle: u32) -> Option<u32> {
        self.materials.get(&c)?.get(triangle as usize).copied()
    }
}

fn shape_name(t: ShapeType) -> String {
    format!("{t:?}").to_ascii_lowercase()
}

pub(crate) fn body_state(b: &RigidBody, k: f32) -> BodyState {
    let p = b.position();
    let m = rapier3d::math::Matrix::from_quat(p.rotation).transpose().to_cols_array();
    BodyState { translation: arr3(p.translation / k), rotation: m, linvel: arr3(b.linvel() / k), sleeping: b.is_sleeping() }
}

/// Runs a variable frame time as whole fixed steps; the remainder carries over.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stepper {
    pub accumulated: f32,
}

impl Stepper {
    /// How many fixed steps `elapsed` seconds call for (at most `params.max_substeps`).
    pub fn advance(&mut self, params: &PhysParams, elapsed: f32) -> u32 {
        self.accumulated += elapsed.max(0.0);
        let mut n = 0;
        while self.accumulated >= params.dt && n < params.max_substeps {
            self.accumulated -= params.dt;
            n += 1;
        }
        if n == params.max_substeps {
            self.accumulated = self.accumulated.min(params.dt);
        }
        n
    }
}

/// Which colliders a query considers.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Filter {
    /// Entities to ignore (e.g. the character casting the ray).
    pub exclude: Vec<Key>,
    /// Ignore terrain.
    pub no_terrain: bool,
    /// Ignore dynamic bodies.
    pub no_dynamic: bool,
    /// Ignore reference colliders on these Havok layers (`crate::layers`).
    pub exclude_layers: crate::layers::LayerSet,
}

impl Filter {
    pub fn excluding(key: Key) -> Filter {
        Filter { exclude: vec![key], ..Default::default() }
    }

    fn accepts(&self, c: &Collider, bodies: &RigidBodySet) -> bool {
        let Some(owner) = Owner::decode(c.user_data) else { return false };
        if self.no_terrain && matches!(owner, Owner::Terrain { .. }) {
            return false;
        }
        if self.no_dynamic && c.parent().is_some_and(|b| bodies[b].is_dynamic()) {
            return false;
        }
        if let Owner::Ref { layer, .. } = owner {
            if self.exclude_layers.contains(layer) {
                return false;
            }
        }
        !self.exclude.contains(&owner.key())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct RayHit {
    pub owner: Owner,
    pub distance: f32,
    pub point: [f32; 3],
    pub normal: [f32; 3],
    /// Havok material code of the triangle hit, for triangle meshes.
    pub material: Option<u32>,
    /// Contact shell of the collider hit, game units ([`TERRAIN_SHELL`],
    /// `CollisionObject::shell`); the hit point itself is on the bare surface.
    pub shell: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize)]
pub struct SweepHit {
    pub owner: Owner,
    /// Fraction of the motion travelled before contact (0..=1).
    pub fraction: f32,
    /// Contact point on the hit collider, world space.
    pub point: [f32; 3],
    /// Contact normal pointing out of the hit collider.
    pub normal: [f32; 3],
}

/// Read-only scene queries on an up-to-date world (from [`PhysWorld::queries`]).
pub struct Queries<'a> {
    world: &'a PhysWorld,
}

impl Queries<'_> {
    fn with_filter<R>(&self, filter: &Filter, f: impl FnOnce(QueryFilter) -> R) -> R {
        let w = &self.world.w;
        let pred = |_: ColliderHandle, c: &Collider| filter.accepts(c, &w.bodies);
        f(QueryFilter::default().exclude_sensors().predicate(&pred))
    }

    /// First hit along the ray from `origin` in `direction` (normalised here) within
    /// `max_distance`. Activation targeting casts this from the eye along the view.
    pub fn ray(&self, origin: [f32; 3], direction: [f32; 3], max_distance: f32, filter: &Filter) -> Option<RayHit> {
        let dir = vec3(direction).try_normalize()?;
        let ray = Ray::new(vec3(origin) * self.world.k, dir);
        let w = &self.world.w;
        let (c, hit) = self.with_filter(filter, |f| w.cast_ray_and_get_normal(&ray, max_distance * self.world.k, true, f))?;
        Some(self.ray_hit(c, &ray, hit))
    }

    fn ray_hit(&self, c: ColliderHandle, ray: &Ray, hit: RayIntersection) -> RayHit {
        let owner = Owner::decode(self.world.w.colliders[c].user_data).expect("every collider has an owner");
        let k = self.world.k;
        RayHit {
            owner,
            distance: hit.time_of_impact / k,
            point: arr3(ray.point_at(hit.time_of_impact) / k),
            normal: arr3(hit.normal),
            material: self.world.material_of(c, hit.subshape),
            shell: self.world.shell_of(c),
        }
    }

    /// Every hit along the ray, nearest first (ties by owner).
    pub fn ray_all(&self, origin: [f32; 3], direction: [f32; 3], max_distance: f32, filter: &Filter) -> Vec<RayHit> {
        let Some(dir) = vec3(direction).try_normalize() else { return Vec::new() };
        let ray = Ray::new(vec3(origin) * self.world.k, dir);
        let w = &self.world.w;
        let mut hits: Vec<RayHit> = self.with_filter(filter, |f| {
            w.intersect_ray(ray, max_distance * self.world.k, true, f).map(|(c, _, hit)| self.ray_hit(c, &ray, hit)).collect()
        });
        hits.sort_by(|a, b| a.distance.total_cmp(&b.distance).then(a.owner.cmp(&b.owner)));
        hits
    }

    /// Sweep a convex `shape` placed at `from` along `motion`; the first contact.
    pub fn sweep(&self, shape: &Shape, from: &Iso, motion: [f32; 3], filter: &Filter) -> Result<Option<SweepHit>, ConvertError> {
        let k = self.world.k;
        let (pose, shape) = probe(shape, from, k)?;
        let w = &self.world.w;
        let opts = ShapeCastOptions { max_time_of_impact: 1.0, stop_at_penetration: true, ..Default::default() };
        let hit = self.with_filter(filter, |f| w.cast_shape(&pose, vec3(motion) * k, shape.as_ref(), opts, f));
        Ok(hit.map(|(c, hit)| {
            let co = &self.world.w.colliders[c];
            SweepHit {
                owner: Owner::decode(co.user_data).expect("every collider has an owner"),
                fraction: hit.time_of_impact,
                // The hit collider's side of the hit is `witness1`/`normal1`, in world space.
                point: arr3(hit.witness1 / k),
                normal: arr3(hit.normal1),
            }
        }))
    }

    /// Every entity whose collision overlaps a convex `shape` placed at `at`, sorted.
    pub fn overlap(&self, shape: &Shape, at: &Iso, filter: &Filter) -> Result<Vec<Owner>, ConvertError> {
        let (pose, shape) = probe(shape, at, self.world.k)?;
        let w = &self.world.w;
        let mut out: Vec<Owner> = self.with_filter(filter, |f| {
            w.intersect_shape(pose, shape.as_ref(), f).filter_map(|(_, c)| Owner::decode(c.user_data)).collect()
        });
        out.sort();
        out.dedup();
        Ok(out)
    }
}

/// A single query shape (compounds are not supported as probes), scaled by `k`.
fn probe(shape: &Shape, at: &Iso, k: f32) -> Result<(Pose, SharedShape), ConvertError> {
    let mut leaves = Vec::new();
    let mut errors = Vec::new();
    convert::leaves(shape, at, k, &mut leaves, &mut errors);
    if let Some(e) = errors.into_iter().next() {
        return Err(e);
    }
    match leaves.len() {
        1 => {
            let l = leaves.pop().expect("one leaf");
            Ok((l.pose, l.shape))
        }
        _ => Err(ConvertError::NotSingleShape),
    }
}
