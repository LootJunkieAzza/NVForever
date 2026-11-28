//! The lamp shadows' grid of shadow casters (light_shadows.rs; docs/design/graphics-enhanced.md
//! §3.17 "Performance pass"). Bevy finds a shadowed point or spot light's casters by testing
//! every shadow caster in the world against it, in parallel, for each light that draws
//! (bevy_light `check_point_light_mesh_visibility`: in Primm 0.4 ms a frame at the median
//! and 7.6 ms in the worst frames of the main session's profile, 2026-11-18). This keeps
//! every mesh in a grid by where its bounds are, and hands the patched bevy_light the
//! meshes of the cells a drawing lamp's light reaches (`NvfShadowCasterCandidates`,
//! engine/vendor/README.md "shadow caster candidates"); Bevy then tests only those.
//!
//! - A mesh is filed by its bounds' centre in a cell of [`CELL`] units; one whose bounding
//!   sphere is larger than [`MARGIN`] (a building, the landscape), or that has no bounds or
//!   [`NoFrustumCulling`], is listed for every light. A light's candidates are the cells its
//!   sphere grown by the margin overlaps, and those: every mesh whose bounds can reach it.
//! - Every mesh is filed, casters or not (a superset: Bevy skips what is not a caster), so
//!   one that becomes a caster later without moving is in the grid.
//! - Kept up to date from the meshes that moved, had their bounds or culling changed, or
//!   went this frame; built anew from every mesh when the lamp shadows are switched on.
//! - Only the game's lamps (a slot in `HoldLightShadowMaps`) get candidates; the
//!   flashlight and the menu scenes' lights keep Bevy's search.
//! - Test aid `NVF_LIGHT_SHADOW_CASTER_GRID=0`: no grid; Bevy searches every caster, as
//!   before 2026-11-18.

use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::{NoFrustumCulling, VisibilitySystems};
use bevy::ecs::entity::{EntityHashMap, EntityHashSet};
use bevy::light::{NvfShadowCasterCandidates, NvfSpatialMeshGrid, SimulationLightSystems};
use bevy::math::Vec3A;
use bevy::pbr::HoldLightShadowMaps;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use std::sync::atomic::{AtomicU64, Ordering::Relaxed};

use crate::light_shadows::LightShadows;

/// A grid cell's side, game units (about 15 m) [engineering choice: a lamp's light (300 to
/// 1,000 units) reaches a few cells each way; a cell of a town holds some hundreds of meshes].
pub const CELL: f32 = 1024.0;

/// A mesh whose bounding sphere is larger than this (game units) is listed for every light;
/// a smaller one only in the cell its centre is in, which lights reaching within this of the
/// cell look at [engineering choice: nearly every placed shape is smaller].
pub const MARGIN: f32 = 512.0;

/// Where a mesh is filed: a cell, or the list for every light.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Place {
    Cell(IVec3),
    Everywhere,
}

/// The meshes by cell ([`CELL`]) and those listed for every light.
#[allow(dead_code)]
#[derive(Debug, Default)]
pub struct CasterGrid {
    cells: HashMap<IVec3, Vec<Entity>>,
    everywhere: EntityHashSet,
    at: EntityHashMap<Place>,
    /// Bumped only when a mesh enters, leaves, or changes cell/Everywhere (moving within
    /// the same 1024-unit cell leaves `gen` unchanged) [design].
    gen: u64,
}

/// The cell a point lies in.
pub fn cell_of(p: Vec3) -> IVec3 {
    (p / CELL).floor().as_ivec3()
}

#[allow(dead_code)]
impl CasterGrid {
    /// Monotonic generation bumped whenever any bucket's membership changes [design].
    pub fn generation(&self) -> u64 {
        self.gen
    }

    /// File a mesh by its bounds' centre and bounding radius; `bounded` false (no bounds, or
    /// not culled) lists it for every light. Moving within its cell costs a lookup.
    pub fn place(&mut self, e: Entity, centre: Vec3, radius: f32, bounded: bool) {
        let place = if bounded && radius <= MARGIN { Place::Cell(cell_of(centre)) } else { Place::Everywhere };
        if self.at.get(&e) == Some(&place) {
            return;
        }
        self.remove(e);
        match place {
            Place::Cell(c) => self.cells.entry(c).or_default().push(e),
            Place::Everywhere => {
                self.everywhere.insert(e);
            }
        }
        self.at.insert(e, place);
        self.gen = self.gen.wrapping_add(1);
    }

    /// Take a mesh out.
    pub fn remove(&mut self, e: Entity) {
        match self.at.remove(&e) {
            Some(Place::Cell(c)) => {
                if let Some(v) = self.cells.get_mut(&c) {
                    if let Some(i) = v.iter().position(|x| *x == e) {
                        v.swap_remove(i);
                    }
                    if v.is_empty() {
                        self.cells.remove(&c);
                    }
                }
                self.gen = self.gen.wrapping_add(1);
            }
            Some(Place::Everywhere) => {
                self.everywhere.remove(&e);
                self.gen = self.gen.wrapping_add(1);
            }
            None => {}
        }
    }

    /// Every mesh whose bounds may reach a light's sphere at `centre` with `range`, into
    /// `out` (each once).
    pub fn candidates(&self, centre: Vec3, range: f32, out: &mut Vec<Entity>) {
        out.extend(self.everywhere.iter().copied());
        let reach = range + MARGIN;
        let (lo, hi) = (cell_of(centre - reach), cell_of(centre + reach));
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    if let Some(v) = self.cells.get(&IVec3::new(x, y, z)) {
                        out.extend_from_slice(v);
                    }
                }
            }
        }
    }

    /// Meshes filed, and those of them listed for every light.
    pub fn counts(&self) -> (usize, usize) {
        (self.at.len(), self.everywhere.len())
    }
}

/// `NVF_LIGHT_SHADOW_CASTER_GRID=0` (a measuring aid): no grid, Bevy searches every caster
/// for each drawing lamp.
pub fn grid_on() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| {
        let off = std::env::var("NVF_LIGHT_SHADOW_CASTER_GRID").is_ok_and(|v| v.trim() == "0");
        if off {
            info!("light shadows: NVF_LIGHT_SHADOW_CASTER_GRID=0, Bevy searches every shadow caster");
        }
        !off
    })
}

/// The grid's counts since the last summary: lamps served, their candidates, and the
/// meshes filed (and listed everywhere) when last seen.
static SERVED: AtomicU64 = AtomicU64::new(0);
static CANDIDATES: AtomicU64 = AtomicU64::new(0);
static FILED: AtomicU64 = AtomicU64::new(0);
static EVERYWHERE: AtomicU64 = AtomicU64::new(0);

/// The grid's counts for the summary: (lamps served, candidates over them, meshes filed,
/// of them listed for every light); the first two start anew.
pub fn take_grid_counts() -> (u64, u64, u64, u64) {
    (SERVED.swap(0, Relaxed), CANDIDATES.swap(0, Relaxed), FILED.load(Relaxed), EVERYWHERE.load(Relaxed))
}

/// O(1) incremental index backing [`NvfSpatialMeshGrid`] so both point/spot lamp
/// candidate queries and directional sun cascade culling share one zero-copy spatial grid
/// [design].
#[derive(Debug, Default)]
struct SpatialIndex {
    cell_slots: HashMap<IVec3, usize>,
    everywhere_slots: EntityHashMap<usize>,
    at: EntityHashMap<Place>,
    gen: u64,
}

impl SpatialIndex {
    fn generation(&self) -> u64 {
        self.gen
    }

    fn place(&mut self, grid: &mut NvfSpatialMeshGrid, e: Entity, centre: Vec3, radius: f32, bounded: bool) {
        let place = if bounded && radius <= MARGIN { Place::Cell(cell_of(centre)) } else { Place::Everywhere };
        if self.at.get(&e) == Some(&place) {
            return;
        }
        self.remove(grid, e);
        match place {
            Place::Cell(c) => {
                if let Some(&slot) = self.cell_slots.get(&c) {
                    grid.cells[slot].2.push(e);
                } else {
                    let slot = grid.cells.len();
                    let world_center = (c.as_vec3a() + Vec3A::splat(0.5)) * CELL;
                    grid.cells.push((c, world_center, vec![e]));
                    self.cell_slots.insert(c, slot);
                }
            }
            Place::Everywhere => {
                let slot = grid.everywhere.len();
                grid.everywhere.push(e);
                self.everywhere_slots.insert(e, slot);
            }
        }
        self.at.insert(e, place);
        self.gen = self.gen.wrapping_add(1);
    }

    fn remove(&mut self, grid: &mut NvfSpatialMeshGrid, e: Entity) {
        match self.at.remove(&e) {
            Some(Place::Cell(c)) => {
                if let Some(&slot) = self.cell_slots.get(&c) {
                    let v = &mut grid.cells[slot].2;
                    if let Some(i) = v.iter().position(|x| *x == e) {
                        v.swap_remove(i);
                    }
                    if v.is_empty() {
                        grid.cells.swap_remove(slot);
                        self.cell_slots.remove(&c);
                        if slot < grid.cells.len() {
                            let moved_coord = grid.cells[slot].0;
                            self.cell_slots.insert(moved_coord, slot);
                        }
                    }
                }
                self.gen = self.gen.wrapping_add(1);
            }
            Some(Place::Everywhere) => {
                if let Some(slot) = self.everywhere_slots.remove(&e) {
                    grid.everywhere.swap_remove(slot);
                    if slot < grid.everywhere.len() {
                        let moved = grid.everywhere[slot];
                        self.everywhere_slots.insert(moved, slot);
                    }
                }
                self.gen = self.gen.wrapping_add(1);
            }
            None => {}
        }
    }

    fn candidates(&self, grid: &NvfSpatialMeshGrid, centre: Vec3, range: f32, out: &mut Vec<Entity>) {
        out.extend_from_slice(&grid.everywhere);
        let reach = range + MARGIN;
        let (lo, hi) = (cell_of(centre - reach), cell_of(centre + reach));
        for x in lo.x..=hi.x {
            for y in lo.y..=hi.y {
                for z in lo.z..=hi.z {
                    if let Some(&slot) = self.cell_slots.get(&IVec3::new(x, y, z)) {
                        out.extend_from_slice(&grid.cells[slot].2);
                    }
                }
            }
        }
    }

    fn counts(&self) -> (usize, usize) {
        (self.at.len(), self.everywhere_slots.len())
    }
}

pub struct LightShadowCastersPlugin;

impl Plugin for LightShadowCastersPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NvfShadowCasterCandidates>()
            .init_resource::<NvfSpatialMeshGrid>()
            .add_systems(
                PostUpdate,
                caster_candidates
                    .after(crate::light_shadows::ChooseLamps)
                    .after(TransformSystems::Propagate)
                    .after(VisibilitySystems::CalculateBounds)
                    .before(VisibilitySystems::CheckVisibility)
                    .before(SimulationLightSystems::CheckLightVisibility),
            );
    }
}

type Meshes<'a> = (Entity, &'a GlobalTransform, Option<&'a Aabb>, Has<NoFrustumCulling>);

type ChangedMeshes = (With<Mesh3d>, Or<(Added<Mesh3d>, Changed<GlobalTransform>, Changed<Aabb>, Added<NoFrustumCulling>)>);

/// A mesh's bounds' centre and bounding radius in the world, and whether it is bounded
/// (culled by its bounds).
fn bounds(t: &GlobalTransform, aabb: Option<&Aabb>, no_culling: bool) -> (Vec3, f32, bool) {
    match aabb {
        Some(a) if !no_culling => {
            let m = t.affine().matrix3;
            let scale = m.x_axis.length().max(m.y_axis.length()).max(m.z_axis.length());
            (t.transform_point(Vec3::from(a.center)), Vec3::from(a.half_extents).length() * scale, true)
        }
        _ => (t.translation(), 0.0, false),
    }
}

/// Keep the shared spatial grid up to date for directional sun cascades and point/spot
/// lamps, and give each drawing lamp its cached candidates [design].
#[allow(clippy::too_many_arguments)]
fn caster_candidates(
    settings: Option<Res<LightShadows>>,
    hold: Option<Res<HoldLightShadowMaps>>,
    mut out: ResMut<NvfShadowCasterCandidates>,
    mut spatial: ResMut<NvfSpatialMeshGrid>,
    mut index: Local<Option<SpatialIndex>>,
    mut cache: Local<EntityHashMap<([u32; 3], u32, u64)>>,
    mut active: Local<EntityHashSet>,
    all: Query<Meshes, With<Mesh3d>>,
    changed: Query<Meshes, ChangedMeshes>,
    (mut gone, mut unbounded, mut culled): (RemovedComponents<Mesh3d>, RemovedComponents<Aabb>, RemovedComponents<NoFrustumCulling>),
    lights: Query<(&GlobalTransform, Option<&PointLight>, Option<&SpotLight>)>,
) {
    if !grid_on() {
        *index = None;
        if spatial.ready {
            spatial.cells.clear();
            spatial.everywhere.clear();
            spatial.ready = false;
        }
        cache.clear();
        if !out.lights.is_empty() {
            out.lights.clear();
        }
        gone.clear();
        unbounded.clear();
        culled.clear();
        return;
    }
    let spatial = &mut *spatial;
    if let Some(idx) = index.as_mut() {
        for e in gone.read() {
            idx.remove(spatial, e);
        }
        for (e, t, aabb, no_culling) in &changed {
            let (c, r, b) = bounds(t, aabb, no_culling);
            idx.place(spatial, e, c, r, b);
        }
        // Bounds or culling taken away: filed again by what is left.
        for e in unbounded.read().chain(culled.read()) {
            if let Ok((e, t, aabb, no_culling)) = all.get(e) {
                let (c, r, b) = bounds(t, aabb, no_culling);
                idx.place(spatial, e, c, r, b);
            }
        }
    } else {
        gone.clear();
        unbounded.clear();
        culled.clear();
        cache.clear();
        spatial.cells.clear();
        spatial.everywhere.clear();
        spatial.cell_half_extent = CELL * 0.5 + MARGIN;
        let mut idx = SpatialIndex::default();
        for (e, t, aabb, no_culling) in &all {
            let (c, r, b) = bounds(t, aabb, no_culling);
            idx.place(spatial, e, c, r, b);
        }
        spatial.ready = true;
        *index = Some(idx);
    }
    let Some(idx) = index.as_ref() else { return };
    let (filed, everywhere) = idx.counts();
    FILED.store(filed as u64, Relaxed);
    EVERYWHERE.store(everywhere as u64, Relaxed);

    let lamps_live = settings.as_deref().is_some_and(|s| s.choice != 0);
    let Some(hold) = hold.as_deref().filter(|_| lamps_live) else {
        if !out.lights.is_empty() {
            out.lights.clear();
        }
        cache.clear();
        return;
    };
    // The lamps drawing this frame (shadows switched on, a slot of the game's): looked up by
    // the slots (a few dozen), not by going over every entity with a place. Reuse cached
    // candidate `Vec`s whenever the grid's cell membership and the lamp's sphere are
    // unchanged [design].
    let out = &mut *out;
    let grid_gen = idx.generation();
    active.clear();
    // (The far texture's lamps too: left out until 2026-11-25, they fell back to the search
    // of every caster on each draw.)
    for &e in hold.slots.keys().chain(hold.far_slots.keys()).chain(hold.spot_slots.keys()) {
        let Ok((t, point, spot)) = lights.get(e) else { continue };
        let (on, range) = match (point, spot) {
            (Some(p), _) if hold.slots.contains_key(&e) || hold.far_slots.contains_key(&e) => (p.shadow_maps_enabled, p.range),
            (None, Some(s)) if hold.spot_slots.contains_key(&e) => (s.shadow_maps_enabled, s.range),
            _ => continue,
        };
        if !on {
            continue;
        }
        active.insert(e);
        let pos = t.translation();
        let key = ([pos.x.to_bits(), pos.y.to_bits(), pos.z.to_bits()], range.to_bits(), grid_gen);
        let len = if cache.get(&e) == Some(&key) && out.lights.contains_key(&e) {
            out.lights[&e].len()
        } else {
            let list = out.lights.entry(e).or_default();
            list.clear();
            idx.candidates(spatial, pos, range, list);
            cache.insert(e, key);
            list.len()
        };
        SERVED.fetch_add(1, Relaxed);
        CANDIDATES.fetch_add(len as u64, Relaxed);
    }
    out.lights.retain(|e, _| active.contains(e));
    cache.retain(|e, _| active.contains(e));
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::camera::primitives::Sphere;

    fn e(n: u32) -> Entity {
        Entity::from_raw_u32(n).unwrap()
    }

    #[test]
    fn a_lamp_gets_every_mesh_whose_bounds_can_reach_its_light() {
        let mut g = CasterGrid::default();
        // A lattice of small meshes, a building and an unbounded mesh.
        let mut meshes = Vec::new();
        let mut n = 1;
        for x in (-6000..=6000).step_by(300) {
            for z in (-3000..=3000).step_by(300) {
                let c = Vec3::new(x as f32, 50.0, z as f32);
                let r = if n % 7 == 0 { MARGIN } else { 20.0 };
                g.place(e(n), c, r, true);
                meshes.push((e(n), c, r));
                n += 1;
            }
        }
        g.place(e(90_000), Vec3::new(9000.0, 0.0, 0.0), 2000.0, true);
        g.place(e(90_001), Vec3::ZERO, 0.0, false);
        assert_eq!(g.counts(), (meshes.len() + 2, 2));
        for (light, range) in [(Vec3::new(100.0, 200.0, -50.0), 600.0), (Vec3::new(-5100.0, 0.0, 2900.0), 1000.0), (Vec3::new(3000.0, 900.0, 0.0), 300.0)] {
            let mut list = Vec::new();
            g.candidates(light, range, &mut list);
            let sphere = Sphere { center: light.into(), radius: range };
            for (m, c, r) in &meshes {
                if Vec3::from(sphere.center).distance(*c) <= range + r {
                    assert!(list.contains(m), "{m} at {c} missed by the light at {light}");
                }
            }
            assert!(list.contains(&e(90_000)) && list.contains(&e(90_001)), "the large and the unbounded for every light");
            let mut unique = list.clone();
            unique.sort_unstable();
            unique.dedup();
            assert_eq!(unique.len(), list.len(), "each once");
            assert!(list.len() < meshes.len() / 4, "only the cells around: {} of {}", list.len(), meshes.len());
        }
    }

    #[test]
    fn a_mesh_moved_removed_or_grown_is_filed_again() {
        let mut g = CasterGrid::default();
        g.place(e(1), Vec3::new(10.0, 0.0, 10.0), 5.0, true);
        g.place(e(1), Vec3::new(20.0, 0.0, 10.0), 5.0, true);
        assert_eq!(g.counts(), (1, 0), "moved within its cell: still one");
        g.place(e(1), Vec3::new(5000.0, 0.0, 10.0), 5.0, true);
        let mut near = Vec::new();
        g.candidates(Vec3::ZERO, 500.0, &mut near);
        assert!(near.is_empty(), "moved away");
        g.place(e(1), Vec3::new(5000.0, 0.0, 10.0), MARGIN + 1.0, true);
        g.candidates(Vec3::ZERO, 500.0, &mut near);
        assert_eq!(near, vec![e(1)], "grown past the margin: everywhere");
        g.remove(e(1));
        assert_eq!(g.counts(), (0, 0));
        g.remove(e(1));
        assert_eq!(g.counts(), (0, 0), "removing twice is harmless");
        assert_eq!(cell_of(Vec3::new(-1.0, 0.0, CELL)), IVec3::new(-1, 0, 1));
    }

    #[test]
    fn bounds_follow_the_mesh_and_unculled_meshes_are_unbounded() {
        let t = GlobalTransform::from(Transform::from_xyz(100.0, 0.0, 0.0).with_scale(Vec3::splat(2.0)));
        let aabb = Aabb::from_min_max(Vec3::new(-1.0, -1.0, -1.0), Vec3::new(3.0, 1.0, 1.0));
        let (c, r, b) = bounds(&t, Some(&aabb), false);
        assert!((c - Vec3::new(102.0, 0.0, 0.0)).length() < 1e-4 && (r - 2.0 * 6f32.sqrt()).abs() < 1e-4 && b);
        assert!(!bounds(&t, Some(&aabb), true).2, "NoFrustumCulling");
        assert!(!bounds(&t, None, false).2, "no bounds");
    }

    #[test]
    fn spatial_index_keeps_nvf_spatial_mesh_grid_consistent_across_moves_and_removals() {
        let mut grid = NvfSpatialMeshGrid { cell_half_extent: CELL * 0.5 + MARGIN, ready: true, ..default() };
        let mut idx = SpatialIndex::default();
        idx.place(&mut grid, e(1), Vec3::new(100.0, 0.0, 100.0), 50.0, true);
        idx.place(&mut grid, e(2), Vec3::new(2500.0, 0.0, 100.0), 50.0, true);
        idx.place(&mut grid, e(3), Vec3::new(5000.0, 0.0, 100.0), MARGIN + 10.0, true);
        assert_eq!(grid.cells.len(), 2);
        assert_eq!(grid.everywhere, vec![e(3)]);

        // Move e(1) into e(2)'s cell -> cell 0 swap-removed cleanly, slot map updated.
        idx.place(&mut grid, e(1), Vec3::new(2600.0, 0.0, 100.0), 50.0, true);
        assert_eq!(grid.cells.len(), 1);
        assert_eq!(grid.cells[0].2.len(), 2);

        // Candidate query near (2500, 0, 100) returns both cell entities + everywhere e(3).
        let mut out = Vec::new();
        idx.candidates(&grid, Vec3::new(2500.0, 0.0, 100.0), 300.0, &mut out);
        assert!(out.contains(&e(1)) && out.contains(&e(2)) && out.contains(&e(3)));

        // Remove all -> grid empties cleanly.
        idx.remove(&mut grid, e(1));
        idx.remove(&mut grid, e(2));
        idx.remove(&mut grid, e(3));
        assert!(grid.cells.is_empty() && grid.everywhere.is_empty());
        assert_eq!(idx.counts(), (0, 0));
    }
}
