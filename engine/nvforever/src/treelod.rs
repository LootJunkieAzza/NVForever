//! Distant trees: beyond the loaded cells, TREE references are drawn as their billboards
//! out to the Tree distance reach, at the start [`LodSettings::trees`](crate::lod::LodSettings)
//! (fTreeLoadDistance; below).
//!
//! The game ships no pre-built tree LOD [observed: no tree LOD meshes or lists in the
//! archives; every `.spt` has a billboard texture, `textures\trees\billboards\<stem>.dds`,
//! and TREE `BNAM` gives its size, world.md §2]. That distant trees are these billboards,
//! upright and turned towards the camera, out to fTreeLoadDistance, is [hypothesis: the
//! Launcher's presets set fTreeLoadDistance 0 / 25,000 / 26,000 / 40,000 from Low to Very
//! High, observed in the game folder's `low.ini` … `VeryHigh.ini`].
//!
//! Each cell whose closest point lies within the distance gets one mesh per TREE base:
//! a card per reference at its foot, BNAM width × height × the reference's scale, lit
//! with an up normal like the near billboards (world.rs). The cards turn in the vertex
//! shader (tree.wesl `TREE_BILLBOARD`), so the meshes never change. A loaded cell draws
//! its own trees; its billboards crossfade with it (fade.rs, the complementary pattern,
//! culled once the cell is fully drawn). Billboards cast and receive no shadows.
//!
//! The reach is the settings page's Tree distance part
//! ([`DrawDistances::trees`](crate::draw_distance::DrawDistances), #3016) of the distant
//! LOD's far limit ([`LodSettings::far`](crate::lod::LodSettings), `--lod-distance`), read
//! each frame ([`reach`]); the part starts at `--tree-distance`'s share of that limit
//! ([`start_part`]), so an untouched game reaches as the flag says, and `--tree-distance 0`
//! draws none whatever the part. When the reach changes the wanted cells are chosen again at
//! once, so billboards beyond a shortened reach go and those within a lengthened one are
//! built over the next frames, not at the next cell crossing.

use crate::draw_distance::DrawDistances;
use crate::fade;
use crate::lod::{square_distance, LodState};
use crate::tree::{BillboardMaterial, TreeBillboards};
use crate::world::{bevy_to_game, Streamer, WorldData, WorldRoot};
use bevy::asset::RenderAssetUsages;
use bevy::camera::primitives::Aabb;
use bevy::camera::visibility::NoAutoAabb;
use bevy::light::{NotShadowCaster, NotShadowReceiver};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, Task};
use fnv_world::placed::{base_object, placement, tree_billboard};
use fnv_world::{cell_of, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

type Grid = (i32, i32);

/// Cells whose billboards are built per frame at most (nearest first).
const CELLS_PER_FRAME: usize = 24;

/// One TREE reference for its billboard: base, foot position (game space), scale.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Placed {
    pub base: u32,
    pub pos: [f32; 3],
    pub scale: f32,
}

/// A TREE base's billboard: material and size (width, height).
type Board = Option<(Handle<BillboardMaterial>, [f32; 2])>;

/// The distant trees drawn, per cell.
#[derive(Default)]
pub struct DistantTrees {
    /// The camera's cell the cells were chosen for.
    centre: Option<Grid>,
    /// The reach the cells were chosen for.
    dist: f32,
    /// Cells wanted, nearest first.
    want: Vec<Grid>,
    /// TREE references per cell (enabled at the start), read once.
    cells: HashMap<Grid, Arc<Vec<Placed>>>,
    /// Cells whose references are being read on a read thread ([`READS_IN_FLIGHT`]).
    reading: HashMap<Grid, Task<Vec<Placed>>>,
    bases: HashMap<u32, Board>,
    drawn: HashMap<Grid, DrawnCell>,
    /// Trees drawn as distant billboards.
    pub count: usize,
}

struct DrawnCell {
    entities: Vec<Entity>,
    trees: usize,
    /// The fade step given (complement of the cell's).
    step: u8,
}

impl DistantTrees {
    /// Remove every billboard (LOD off, interiors, another worldspace).
    pub fn clear(&mut self, commands: &mut Commands) {
        for (_, d) in self.drawn.drain() {
            for e in d.entities {
                commands.entity(e).despawn();
            }
        }
        self.centre = None;
        self.want.clear();
        self.reading.clear();
        self.count = 0;
    }

    /// Take the reach now wanted: when it differs from the one the cells were chosen for,
    /// the choice is made again at the next update (even in the same camera cell). Returns
    /// the earlier reach when it changed.
    fn follow_reach(&mut self, dist: f32) -> Option<f32> {
        if self.dist == dist {
            return None;
        }
        let was = std::mem::replace(&mut self.dist, dist);
        self.centre = None;
        Some(was)
    }
}

/// The Tree distance slider's part of the distant LOD's far limit `far` in whole game
/// units. The slider ends at `far`: billboards beyond it would stand where no distant
/// terrain is drawn (lod.rs draws no chunk beyond `far`) [design, #3016].
pub fn part_reach(far: f32, part: f32) -> f32 {
    (far.max(0.0) * part.clamp(0.0, 1.0)).round()
}

/// The billboards' reach in game units: `part` of `far` ([`part_reach`]), none while
/// `--tree-distance` (`flag`) is 0 whatever the part.
pub fn reach(flag: f32, far: f32, part: f32) -> f32 {
    if flag <= 0.0 {
        0.0
    } else {
        part_reach(far, part)
    }
}

/// The slider's part at the start: `--tree-distance`'s share of `far`, a flag beyond `far`
/// taken as `far` (the part 1), so an untouched slider reaches as the flag says. 0 when
/// either is 0 (no billboards, or no distant LOD to stand on).
pub fn start_part(flag: f32, far: f32) -> f32 {
    if far <= 0.0 {
        0.0
    } else {
        flag.clamp(0.0, far) / far
    }
}

/// The cells within `dist` of `p` (closest point), nearest first.
pub fn cells_within(p: Vec2, dist: f32) -> Vec<Grid> {
    if dist <= 0.0 {
        return Vec::new();
    }
    let r = (dist / CELL_SIZE).ceil() as i32 + 1;
    let c = cell_of(p.x, p.y);
    let mut out: Vec<(f32, Grid)> = (-r..=r)
        .flat_map(|dx| (-r..=r).map(move |dy| (c.0 + dx, c.1 + dy)))
        .map(|g| (square_distance(p, g, 1), g))
        .filter(|(d, _)| *d < dist)
        .collect();
    out.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    out.into_iter().map(|(_, g)| g).collect()
}

/// [design] Cells whose TREE references are read at once on the read pool
/// (docs/design/streaming-hitches.md §3.5): two frames' worth of [`CELLS_PER_FRAME`], so
/// the cards always have cells to be built from.
const READS_IN_FLIGHT: usize = 48;

/// The references of an exterior cell: its own and the persistent ones standing in it.
fn cell_refs(world: &WorldData, g: Grid) -> Vec<u32> {
    let mut refs: Vec<u32> = world.grid_cell(g).map(|c| c.refs.clone()).unwrap_or_default();
    if let Some(p) = world.persistent_by_grid.get(&g) {
        refs.extend(p);
    }
    refs
}

/// The TREE references among `refs` that are enabled at the start. Reads the load order's
/// records only, so it runs on a read thread ([`update_distant_trees`]): it took up to
/// 5 ms of a frame for a step's cells on the main thread [observed 2026-11-20,
/// `profile hitch`: `treelod::update_distant_trees`].
fn trees_of(lo: &fnv_esm::LoadOrder, schema: &fnv_esm::Schema, refs: &[u32]) -> Vec<Placed> {
    let mut out = Vec::new();
    let mut is_tree: HashMap<u32, bool> = HashMap::new();
    for &r in refs {
        let Some(p) = placement(lo, schema, r) else { continue };
        let Some(base) = p.base else { continue };
        let tree = *is_tree.entry(base).or_insert_with(|| base_object(lo, schema, base).is_some_and(|b| &b.sig == b"TREE" && b.is_speedtree()));
        if tree && crate::world::enabled(lo, schema, &p, 0) {
            out.push(Placed { base, pos: p.position, scale: p.scale });
        }
    }
    out
}

/// One mesh of upright cards (tree.wesl `TREE_BILLBOARD`) for `trees`, positions relative
/// to `origin`: per card four vertices at the foot, UV_1 = corner offset (x right, y
/// up), UV_0 = the billboard picture (v = 1 at the bottom). Returns the mesh and its
/// bounds in the same space.
pub fn cards(trees: &[Placed], origin: Vec3, [w, h]: [f32; 2]) -> (Mesh, Aabb) {
    let n = trees.len();
    let (mut pos, mut uv, mut corner, mut idx) = (Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(4 * n), Vec::with_capacity(6 * n));
    let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
    for t in trees {
        let foot = Vec3::from_array(t.pos) - origin;
        let (hw, ht) = (w / 2.0 * t.scale, h * t.scale);
        lo = lo.min(foot - Vec3::new(hw, hw, 0.0));
        hi = hi.max(foot + Vec3::new(hw, hw, ht));
        let first = pos.len() as u32;
        // Bottom left, bottom right, top right, top left: counter-clockwise on screen.
        for (cx, cy, u, v) in [(-hw, 0.0, 0.0, 1.0), (hw, 0.0, 1.0, 1.0), (hw, ht, 1.0, 0.0), (-hw, ht, 0.0, 0.0)] {
            pos.push(foot.to_array());
            corner.push([cx, cy]);
            uv.push([u, v]);
        }
        idx.extend([first, first + 1, first + 2, first, first + 2, first + 3]);
    }
    if lo.x > hi.x {
        (lo, hi) = (Vec3::ZERO, Vec3::ZERO);
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    // Lit like the ground under it, as the near billboards (world.rs).
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0, 0.0, 1.0]; 4 * n]);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, corner);
    mesh.insert_indices(Indices::U32(idx));
    (mesh, Aabb::from_min_max(lo, hi))
}

fn board(world: &WorldData, base: u32, models: &mut ModelCtx, materials: &mut Assets<BillboardMaterial>) -> Board {
    let (size, tex) = tree_billboard(world.lo, world.schema, base)?;
    let a = &mut *models;
    let image = a.textures.get(&tex, true, &a.data, &mut a.images, a.formats.as_deref())?;
    let mat = materials.add(BillboardMaterial {
        base: StandardMaterial {
            base_color_texture: Some(image),
            alpha_mode: AlphaMode::Mask(0.5),
            cull_mode: None,
            perceptual_roughness: 0.9,
            reflectance: 0.1,
            ..default()
        },
        extension: TreeBillboards::default(),
    });
    Some((mat, size))
}

#[allow(clippy::too_many_arguments)]
pub fn update_distant_trees(
    mut commands: Commands,
    streamer: Res<Streamer>,
    world: Res<WorldData>,
    mut lod: ResMut<LodState>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, crate::fly::WorldCamera>,
    mut models: ModelCtx,
    mut materials: ResMut<Assets<BillboardMaterial>>,
    pace: Res<crate::stream_pace::StreamPace>,
    distances: Option<Res<DrawDistances>>,
) {
    let (flag, far) = (lod.settings.trees, lod.settings.far);
    let dist = reach(flag, far, distances.map_or_else(|| start_part(flag, far), |d| d.trees));
    let lod = &mut *lod;
    let trees = &mut lod.trees;
    if !lod.enabled || dist <= 0.0 || streamer.interior.is_some() {
        if !trees.drawn.is_empty() {
            trees.clear(&mut commands);
        }
        return;
    }
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let c = cell_of(g.x, g.y);
    if let Some(was) = trees.follow_reach(dist).filter(|was| *was > 0.0) {
        info!("distant trees: reach {was:.0} -> {dist:.0} units");
    }
    if trees.centre != Some(c) {
        trees.centre = Some(c);
        trees.want = cells_within(g.truncate(), dist);
        let keep: HashSet<Grid> = trees.want.iter().copied().collect();
        let gone: Vec<Grid> = trees.drawn.keys().filter(|g| !keep.contains(g)).copied().collect();
        for g in gone {
            if let Some(d) = trees.drawn.remove(&g) {
                trees.count -= d.trees;
                for e in d.entities {
                    commands.entity(e).despawn();
                }
            }
        }
        // Keep the reference lists of cells a little beyond the range.
        let r = (dist / CELL_SIZE).ceil() as i32 + 3;
        trees.cells.retain(|g, _| (g.0 - c.0).abs() <= r && (g.1 - c.1).abs() <= r);
        trees.reading.retain(|g, _| (g.0 - c.0).abs() <= r && (g.1 - c.1).abs() <= r);
    }
    // Fade steps of the drawn cells follow their cells (culled when fully drawn).
    for (g, d) in trees.drawn.iter_mut() {
        let k = fade::step(streamer.cell_level(*g));
        if k != d.step {
            d.step = k;
            for &e in &d.entities {
                commands.entity(e).insert(fade::complement(k));
            }
        }
    }
    // The cells' TREE references are read on the read pool, nearest first; a cell's cards
    // are built once its list is in.
    let mut read = Vec::new();
    trees.reading.retain(|g, task| match check_ready(task) {
        Some(list) => {
            read.push((*g, list));
            false
        }
        None => true,
    });
    for (g, list) in read {
        trees.cells.insert(g, Arc::new(list));
    }
    // Above 60 fps a frame gets the same share of itself (stream_pace.rs).
    let per_frame = pace.count(CELLS_PER_FRAME);
    let mut missing: Vec<Grid> = Vec::new();
    let on_read_threads = crate::stream_pace::on_read_threads();
    for &g in trees.want.iter().filter(|g| !trees.drawn.contains_key(g)) {
        if !on_read_threads && missing.len() < per_frame {
            // As before: read here, within the frame's count.
            trees.cells.entry(g).or_insert_with(|| Arc::new(trees_of(world.lo, world.schema, &cell_refs(&world, g))));
        }
        if trees.cells.contains_key(&g) {
            if missing.len() < per_frame {
                missing.push(g);
            }
        } else if on_read_threads {
            let in_flight = trees.reading.len();
            if let std::collections::hash_map::Entry::Vacant(reading) = trees.reading.entry(g) {
                if in_flight < READS_IN_FLIGHT {
                    let (lo, schema, refs) = (world.lo, world.schema, cell_refs(&world, g));
                    reading.insert(nvf_assets::read_pool::read_pool().spawn(async move { trees_of(lo, schema, &refs) }));
                }
            }
        }
    }
    let _spending = (!missing.is_empty()).then(|| pace.spending());
    for g in missing {
        let Some(placed) = trees.cells.get(&g).cloned() else { continue };
        let mut by_base: HashMap<u32, Vec<Placed>> = HashMap::new();
        for t in placed.iter() {
            by_base.entry(t.base).or_default().push(*t);
        }
        let mut bases: Vec<u32> = by_base.keys().copied().collect();
        bases.sort_unstable();
        let origin = Vec3::new(g.0 as f32 * CELL_SIZE, g.1 as f32 * CELL_SIZE, 0.0);
        let step = fade::step(streamer.cell_level(g));
        let mut d = DrawnCell { entities: Vec::new(), trees: 0, step };
        for b in bases {
            let board = trees.bases.entry(b).or_insert_with(|| board(&world, b, &mut models, &mut materials)).clone();
            let Some((mat, size)) = board else { continue };
            let list = &by_base[&b];
            let (mesh, aabb) = cards(list, origin, size);
            let e = commands
                .spawn((
                    Name::new(format!("Distant trees {},{}", g.0, g.1)),
                    Mesh3d(models.meshes.add(mesh)),
                    MeshMaterial3d(mat),
                    Transform::from_translation(origin),
                    aabb,
                    NoAutoAabb,
                    NotShadowCaster,
                    NotShadowReceiver,
                    fade::complement(step),
                ))
                .id();
            commands.entity(root).add_child(e);
            d.entities.push(e);
            d.trees += list.len();
        }
        trees.count += d.trees;
        trees.drawn.insert(g, d);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    #[test]
    fn cells_in_range_nearest_first() {
        let p = Vec2::new(100.0, 100.0);
        let cells = cells_within(p, 10_000.0);
        assert_eq!(cells[0], (0, 0));
        assert!(cells.iter().all(|&g| square_distance(p, g, 1) < 10_000.0));
        // A disc around the camera's cell: (2, 2) is 11,444 units away, (2, 1) 9,025.
        assert!(cells.contains(&(2, 1)) && !cells.contains(&(2, 2)) && cells.contains(&(-3, 0)));
        let d: Vec<f32> = cells.iter().map(|&g| square_distance(p, g, 1)).collect();
        assert!(d.windows(2).all(|w| w[0] <= w[1]));
        assert!(cells_within(p, 0.0).is_empty());
    }

    #[test]
    fn the_reach_is_the_part_of_the_far_limit_and_starts_at_the_flag() {
        let (flag, far) = (40_000.0, 125_000.0);
        assert_eq!(start_part(flag, far), 0.32, "the defaults: 40,000 of 125,000");
        assert_eq!(reach(flag, far, start_part(flag, far)), 40_000.0, "untouched: exactly the flag's value");
        assert_eq!(reach(flag, far, 1.0), 125_000.0, "the full part: the distant LOD's far limit");
        assert_eq!(reach(flag, far, 0.08), 10_000.0);
        assert_eq!(reach(flag, far, 0.34) - reach(flag, far, 0.32), 2_500.0, "a step of 0.02");
        assert_eq!(reach(flag, far, 1.5), 125_000.0, "never beyond the far limit");
        assert_eq!(reach(flag, far, 0.0), 0.0);
        assert_eq!(reach(0.0, far, 0.5), 0.0, "--tree-distance 0 stays off whatever the part");
        assert_eq!(reach(0.0, far, 1.0), 0.0);
        assert_eq!(start_part(200_000.0, far), 1.0, "a flag beyond the far limit is taken as it");
        assert_eq!(reach(200_000.0, far, start_part(200_000.0, far)), 125_000.0);
        assert_eq!(start_part(0.0, far), 0.0);
        assert_eq!(start_part(flag, 0.0), 0.0, "no distant LOD: no part");
        assert_eq!(reach(flag, 0.0, 0.5), 0.0);
    }

    #[test]
    fn a_changed_reach_chooses_the_cells_again_in_the_same_cell() {
        let mut t = DistantTrees::default();
        // The first update's choice: the reach before it was none.
        assert_eq!(t.follow_reach(40_000.0), Some(0.0));
        t.centre = Some((3, 4));
        assert_eq!(t.follow_reach(40_000.0), None, "unchanged: the choice stands");
        assert_eq!(t.centre, Some((3, 4)));
        // Shortened: chosen again at once, and the cells beyond are no longer wanted.
        assert_eq!(t.follow_reach(10_000.0), Some(40_000.0));
        assert_eq!(t.centre, None);
        let p = Vec2::new(100.0, 100.0);
        let (long, short) = (cells_within(p, 40_000.0), cells_within(p, 10_000.0));
        assert!(short.len() < long.len() && short.iter().all(|g| long.contains(g)));
        assert!(long.iter().any(|g| !short.contains(g)), "billboards of those cells go");
        // Lengthened again: the nearer cells stay, the farther come back.
        t.centre = Some((3, 4));
        assert_eq!(t.follow_reach(40_000.0), Some(10_000.0));
        assert_eq!(t.centre, None);
    }

    #[test]
    fn card_layout() {
        let trees = [Placed { base: 1, pos: [4196.0, 50.0, 300.0], scale: 2.0 }, Placed { base: 1, pos: [5000.0, 900.0, 310.0], scale: 1.0 }];
        let (m, aabb) = cards(&trees, Vec3::new(4096.0, 0.0, 0.0), [100.0, 175.0]);
        assert_eq!(m.count_vertices(), 8);
        let Some(VertexAttributeValues::Float32x3(p)) = m.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        assert!(p[..4].iter().all(|v| *v == [100.0, 50.0, 300.0]));
        let Some(VertexAttributeValues::Float32x2(c)) = m.attribute(Mesh::ATTRIBUTE_UV_1) else { panic!() };
        // Scaled: half width 100, height 350.
        assert_eq!(&c[..4], &[[-100.0, 0.0], [100.0, 0.0], [100.0, 350.0], [-100.0, 350.0]]);
        let Some(VertexAttributeValues::Float32x2(uv)) = m.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        assert_eq!(&uv[..4], &[[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]]);
        assert_eq!(m.indices().unwrap().len(), 12);
        // Bounds hold both trees at full size.
        assert_eq!(Vec3::from(aabb.min()), Vec3::new(0.0, -50.0, 300.0));
        assert_eq!(Vec3::from(aabb.max()), Vec3::new(954.0, 950.0, 650.0));
    }
}
