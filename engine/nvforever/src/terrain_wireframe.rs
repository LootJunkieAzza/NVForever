//! The terrain wireframe (#2149): the triangle edges of the loaded landscape drawn as
//! thin lines over it, to see the shape smoothing gives the ground (and the stored
//! triangles with smoothing off). For developers and testers [design, #2149].
//!
//! - **Switch**: the settings page's Debug row Terrain wireframe ([`KEY`], live) and
//!   `--terrain-wireframe` at start. Off by default.
//! - **What**: each loaded, shown exterior cell's surface (`fnv_world::land_surface`, the
//!   run's own `nvf_play::smooth_terrain::spec`, so the same vertices and SW → NE triangles
//!   the drawn ground and the collision have), every undirected triangle edge once: the
//!   rows and the columns of the grid and its diagonals, as line strips. Not the texture
//!   layers' meshes (they share the base's vertices) and not the distant LOD terrain.
//! - **How**: on switching on, a job on the read pool for each such cell builds its surface
//!   from the LAND records again and its lines ([`outline`]); a cell that loads or is
//!   shown while on gets one too, and one that unloads or is hidden loses its lines. Each
//!   cell's lines are a retained gizmo (sent to the GPU once), in game space under the
//!   world's one game → Bevy turn ([`crate::world::game_to_bevy`]), depth-tested, nearer
//!   the camera by a hair ([`DEPTH_BIAS`], the collision view's) so they do not flicker in
//!   the ground they outline; nothing lifts the vertices.
//! - **When**: in `PostUpdate` after the visibility propagation, so a cell hidden this frame
//!   loses its lines this frame; a new outline is spawned with its global transform set
//!   (transforms were propagated before), so it is drawn in place from its first frame.
//! - **Cost**: off, one run condition a frame and nothing else: no surfaces, line buffers
//!   or jobs are kept, nothing is uploaded or drawn. Switching off drops the jobs not yet
//!   done (a dropped task is cancelled), despawns the lines and frees their assets.
//!
//! - **4 × 4 near** (#2676): the lines show each quadrant as it is drawn now (4 × 4 or 2 × 2,
//!   a 4 × 4 quadrant's coarse sides on their 2 × 2 neighbours' segments: `crate::terrain_lod`),
//!   one grid of lines per quadrant ([`outline_drawn`]; the quadrants' shared sides are drawn
//!   by both). A cell whose quadrants are redrawn at another level gets a new outline job
//!   and keeps its old lines until the job is done. The count line then ends `; <n>
//!   quadrants outlined 4 x 4`. Once the levels have settled (`TerrainLod::settled_revision`)
//!   and every outline matches the quadrants as drawn at that revision, `terrain wireframe:
//!   settled terrain revision <r> outlined: <n> quadrants 4 x 4, <c> cells, <e> edges`, once
//!   per revision (the smoke scenario `terrain-wireframe-near` reads it: it ties the lines
//!   to the drawing they show, whatever the order of the two systems' frames).
//!
//! [design, #2149] One gizmo per cell rather than per quadrant (the plan's sketch): the
//! cell is what loads and unloads, and its surface is one grid. Jobs are keyed by the
//! cell's entity; a cell that goes takes its pending job with it, so no stale result can
//! land on a later cell (the run's surface spec cannot change while it runs).
//!
//! Log: `terrain wireframe: on (…)` and `terrain wireframe: off; …` when it switches, and
//! `terrain wireframe: <n> cells outlined, <e> edges, <m> without landscape, <a> line
//! assets` each time the set settles with a different count (the smoke scenario
//! `terrain-wireframe-live` reads them).

use bevy::camera::visibility::VisibilitySystems;
use bevy::prelude::*;
use bevy::tasks::{futures::check_ready, Task};
use fnv_esm::{LoadOrder, Schema};
use fnv_world::land_surface::{QuadrantDraw, Surface, SurfaceSpec};
use fnv_world::WorldIndex;
use nvf_play::RenderCell;
use std::collections::HashMap;
use std::sync::Arc;

/// The settings row's key, INI key too.
pub const KEY: &str = "terrain-wireframe";
/// The settings row's note, NVForever's own words [design, #2149].
pub const NOTE: &str = "On: draw triangle edges over the loaded landscape to inspect its shape. Changes immediately. Distant terrain is not outlined.";
/// The row's command-line switch.
pub const SWITCHES: &str = "--terrain-wireframe";
/// Line colour [design, #2149: magenta reads against the sand, rock and scrub of the
/// Mojave and is no colour the collision view uses for terrain].
pub const COLOUR: [f32; 3] = [1.0, 0.2, 0.85];
/// How much nearer the camera the lines are drawn than the ground (Bevy's gizmo depth
/// bias, -1 to 1): the collision view's [design].
pub const DEPTH_BIAS: f32 = -0.0005;
/// Line width, pixels [design: thin, so the shading under it still shows].
pub const LINE_WIDTH: f32 = 1.0;

/// A cell's surface (worldspace, grid square, the run's surface spec) for the outline
/// jobs: the LAND records in the game, a made-up surface in tests.
type SurfaceOf = dyn Fn(u32, (i32, i32), SurfaceSpec) -> Option<Surface> + Send + Sync;

/// Where the outline jobs get a cell's surface.
#[derive(Resource, Clone)]
pub struct WireSource(Arc<SurfaceOf>);

impl WireSource {
    pub fn new(f: impl Fn(u32, (i32, i32), SurfaceSpec) -> Option<Surface> + Send + Sync + 'static) -> WireSource {
        WireSource(Arc::new(f))
    }

    /// From the load order's LAND records: the cell's and its eight neighbours' (a smoothed
    /// surface reads them along its edges), as the drawn terrain reads them.
    pub fn of_records(lo: &'static LoadOrder<'static>, schema: &'static Schema, index: &'static WorldIndex) -> WireSource {
        WireSource::new(move |world, g, spec| {
            let ws = index.worldspaces.get(&world)?;
            let ids = fnv_world::land_surface::land_ids(g, |n| ws.grid.get(&n).and_then(|c| index.cells.get(c)).and_then(|c| c.land));
            ids[1][1]?;
            fnv_world::land_surface::load_surface(lo, schema, &ids, spec)
        })
    }
}

/// A cell's outline: its job, its drawn lines, or nothing to draw; each with how its
/// quadrants were drawn when the job was made (`None`: in full, no 4 × 4 near).
/// A job outlining a cell again after a redraw keeps the old lines (`old`) until it is done.
enum WireCell {
    Pending { task: Task<Option<(GizmoAsset, usize)>>, draws: Option<[QuadrantDraw; 4]>, old: Option<(Entity, Handle<GizmoAsset>)> },
    Drawn { entity: Entity, asset: Handle<GizmoAsset>, edges: usize, draws: Option<[QuadrantDraw; 4]> },
    Empty(Option<[QuadrantDraw; 4]>),
}

impl WireCell {
    fn draws(&self) -> Option<[QuadrantDraw; 4]> {
        match self {
            WireCell::Pending { draws: d, .. } | WireCell::Drawn { draws: d, .. } | WireCell::Empty(d) => *d,
        }
    }
}

/// Whether the wireframe is on, and each outlined cell's lines.
#[derive(Resource, Default)]
pub struct TerrainWireframe {
    pub on: bool,
    /// By the cell's entity (`RenderCell`).
    cells: HashMap<Entity, WireCell>,
    /// Whether it was on at the last run of [`sync`] (to log the switch once).
    was_on: bool,
    /// The counts last logged.
    logged: Option<Counts>,
    /// The settled terrain revision last said to be outlined (4 x 4 near).
    outlined: Option<u64>,
}

/// What the wireframe holds now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Counts {
    pub drawn: usize,
    pub edges: usize,
    pub empty: usize,
    pub pending: usize,
    /// Quadrants outlined at 4 × 4 under 4 × 4 near (0 otherwise).
    pub fine: usize,
}

impl TerrainWireframe {
    pub fn new(on: bool) -> TerrainWireframe {
        TerrainWireframe { on, ..Default::default() }
    }

    /// The cells drawn, their edges, the cells without landscape and the jobs running.
    pub fn counts(&self) -> Counts {
        let mut c = Counts::default();
        for w in self.cells.values() {
            match w {
                WireCell::Pending { .. } => c.pending += 1,
                WireCell::Drawn { edges, draws, .. } => {
                    c.drawn += 1;
                    c.edges += edges;
                    c.fine += draws.map_or(0, |d| d.iter().filter(|d| d.step == 1).count());
                }
                WireCell::Empty(_) => c.empty += 1,
            }
        }
        c
    }
}

pub struct TerrainWireframePlugin {
    /// On at start (`--terrain-wireframe`).
    pub on: bool,
}

impl Plugin for TerrainWireframePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(TerrainWireframe::new(self.on)).add_systems(PostUpdate, sync.run_if(active).after(VisibilitySystems::VisibilityPropagate));
    }
}

/// On, or still holding something to let go of (or to log the switch off).
fn active(w: Res<TerrainWireframe>) -> bool {
    w.on || w.was_on || !w.cells.is_empty()
}

/// The vertex strips over a surface's grid that cover each undirected triangle edge once:
/// each row west → east, each column south → north, each south-west → north-east diagonal
/// (`fnv_world::land_surface::grid_triangles`). Indices into `Surface::heights`.
pub fn strips(side: usize) -> Vec<Vec<usize>> {
    let mut out = Vec::with_capacity(4 * side);
    for r in 0..side {
        out.push((0..side).map(|c| r * side + c).collect());
    }
    for c in 0..side {
        out.push((0..side).map(|r| r * side + c).collect());
    }
    // Diagonal d = column − row, from the south-east corner's to the north-west's; the two
    // single corners carry no edge.
    for d in -(side as isize - 2)..=(side as isize - 2) {
        let r0 = (-d).max(0) as usize;
        let c0 = d.max(0) as usize;
        let n = side - r0.max(c0);
        out.push((0..n).map(|k| (r0 + k) * side + c0 + k).collect());
    }
    out
}

/// The lines of a cell's surface in game space (`origin`: the cell's south-west corner),
/// and how many edges they are.
pub fn outline(s: &Surface, origin: Vec2) -> (GizmoAsset, usize) {
    let mut asset = GizmoAsset::default();
    let colour = Color::srgb(COLOUR[0], COLOUR[1], COLOUR[2]);
    let mut edges = 0;
    for strip in strips(s.side()) {
        edges += strip.len() - 1;
        asset.linestrip(
            strip.iter().map(|&i| {
                let [x, y, z] = s.vertex(i);
                Vec3::new(origin.x + x, origin.y + y, z)
            }),
            colour,
        );
    }
    (asset, edges)
}

/// The lines of a cell's quadrants as they are drawn (`draws`, by quadrant: 4 × 4 near,
/// `crate::terrain_lod`), each quadrant's grid of drawn vertices ([`strips`]) in game space,
/// and how many edges they are (the quadrants' shared sides counted by both).
pub fn outline_drawn(s: &Surface, origin: Vec2, draws: [QuadrantDraw; 4]) -> (GizmoAsset, usize) {
    let mut asset = GizmoAsset::default();
    let colour = Color::srgb(COLOUR[0], COLOUR[1], COLOUR[2]);
    let mut edges = 0;
    for (q, d) in draws.into_iter().enumerate() {
        let grid = s.drawn_quadrant(q as u8, d);
        for strip in strips(grid.side) {
            edges += strip.len() - 1;
            asset.linestrip(
                strip.iter().map(|&i| {
                    let [x, y, z] = grid.positions[i];
                    Vec3::new(origin.x + x, origin.y + y, z)
                }),
                colour,
            );
        }
    }
    (asset, edges)
}

fn drop_lines(entity: Entity, asset: &Handle<GizmoAsset>, commands: &mut Commands, assets: &mut Assets<GizmoAsset>) {
    commands.entity(entity).try_despawn();
    assets.remove(asset);
}

fn drop_cell(c: WireCell, commands: &mut Commands, assets: &mut Assets<GizmoAsset>) {
    // A pending job is cancelled when its task is dropped here.
    match c {
        WireCell::Drawn { entity, asset, .. } | WireCell::Pending { old: Some((entity, asset)), .. } => drop_lines(entity, &asset, commands, assets),
        _ => {}
    }
}

/// Keep the outlines to the loaded, shown exterior cells while on; let them all go when off.
fn sync(
    mut wire: ResMut<TerrainWireframe>,
    source: Option<Res<WireSource>>,
    cells: Query<(Entity, &RenderCell, &InheritedVisibility)>,
    terrain: Option<Res<crate::terrain::TerrainCache>>,
    mut commands: Commands,
    mut assets: ResMut<Assets<GizmoAsset>>,
) {
    let w = wire.bypass_change_detection();
    if !w.on {
        let removed = w.cells.len();
        for (_, c) in w.cells.drain() {
            drop_cell(c, &mut commands, &mut assets);
        }
        if w.was_on {
            info!("terrain wireframe: off; {removed} cell outlines let go, 0 line assets kept");
        }
        w.was_on = false;
        w.logged = None;
        w.outlined = None;
        return;
    }
    let spec = nvf_play::smooth_terrain::spec();
    if !w.was_on {
        info!("terrain wireframe: on ({})", spec.describe());
        w.was_on = true;
    }
    let shown: HashMap<Entity, (u32, i32, i32)> = cells
        .iter()
        .filter_map(|(e, cell, vis)| match *cell {
            RenderCell::Exterior { world, x, y } if vis.get() => Some((e, (world, x, y))),
            _ => None,
        })
        .collect();
    // How each cell's quadrants are drawn now (4 x 4 near), `None` in full.
    let lod = terrain.as_deref().and_then(|t| t.lod.as_ref());
    let draws_of = |e: Entity| lod.map(|l| l.drawn(e).unwrap_or([QuadrantDraw::FULL; 4]));
    // Cells that unloaded or were hidden lose their lines; cells whose quadrants are drawn
    // otherwise now are outlined again below, keeping their lines until that is done.
    let gone: Vec<Entity> = w.cells.keys().filter(|e| !shown.contains_key(e)).copied().collect();
    for e in gone {
        if let Some(c) = w.cells.remove(&e) {
            drop_cell(c, &mut commands, &mut assets);
        }
    }
    let mut kept: HashMap<Entity, (Entity, Handle<GizmoAsset>)> = HashMap::new();
    let redrawn: Vec<Entity> = w.cells.iter().filter(|(e, c)| c.draws() != draws_of(**e)).map(|(e, _)| *e).collect();
    for e in redrawn {
        match w.cells.remove(&e) {
            Some(WireCell::Drawn { entity, asset, .. }) | Some(WireCell::Pending { old: Some((entity, asset)), .. }) => {
                kept.insert(e, (entity, asset));
            }
            _ => {}
        }
    }
    // Cells loaded or shown since: a job each.
    if let Some(source) = source {
        for (&e, &(world, x, y)) in &shown {
            if w.cells.contains_key(&e) {
                continue;
            }
            let of = source.0.clone();
            let origin = Vec2::new(x as f32 * fnv_world::CELL_SIZE, y as f32 * fnv_world::CELL_SIZE);
            let draws = draws_of(e);
            let task = nvf_assets::read_pool::read_pool().spawn(async move {
                of(world, (x, y), spec).map(|s| match draws {
                    Some(d) => outline_drawn(&s, origin, d),
                    None => outline(&s, origin),
                })
            });
            w.cells.insert(e, WireCell::Pending { task, draws, old: kept.remove(&e) });
        }
    }
    // Lines kept for a cell no job could be made for (no source).
    for (_, (entity, asset)) in kept {
        drop_lines(entity, &asset, &mut commands, &mut assets);
    }
    // Jobs done: their lines drawn, in place of the old ones.
    for c in w.cells.values_mut() {
        let WireCell::Pending { task, draws, old } = c else { continue };
        let draws = *draws;
        let Some(done) = check_ready(task) else { continue };
        if let Some((entity, asset)) = old.take() {
            drop_lines(entity, &asset, &mut commands, &mut assets);
        }
        *c = match done {
            Some((asset, edges)) => {
                let asset = assets.add(asset);
                let line_config = GizmoLineConfig { width: LINE_WIDTH, ..default() };
                // The game → Bevy turn, and its global transform at once: this frame's
                // transforms were propagated before this system.
                let place = crate::world::game_to_bevy();
                let gizmo = Gizmo { handle: asset.clone(), line_config, depth_bias: DEPTH_BIAS };
                let entity = commands.spawn((Name::new("Terrain wireframe"), gizmo, place, GlobalTransform::from(place))).id();
                WireCell::Drawn { entity, asset, edges, draws }
            }
            None => WireCell::Empty(draws),
        };
    }
    let counts = w.counts();
    if counts.pending == 0 && w.logged != Some(counts) {
        // Under 4 x 4 near, how many quadrants the lines show at 4 x 4 (the smoke scenario
        // `terrain-wireframe-near` reads it).
        let fine = if lod.is_some() { format!("; {} quadrants outlined 4 x 4", counts.fine) } else { String::new() };
        info!("terrain wireframe: {} cells outlined, {} edges, {} without landscape, {} line assets{fine}", counts.drawn, counts.edges, counts.empty, counts.drawn);
        w.logged = Some(counts);
    }
    // Every outline is of the quadrants as drawn now (a redrawn cell is pending until its
    // new lines are in), and the levels are settled at this revision: said once.
    let settled = lod.and_then(|l| l.settled_revision());
    if let Some(r) = settled.filter(|r| counts.pending == 0 && w.outlined != Some(*r)) {
        info!("terrain wireframe: settled terrain revision {r} outlined: {} quadrants 4 x 4, {} cells, {} edges", counts.fine, counts.drawn, counts.edges);
        w.outlined = Some(r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_world::land::SIDE;
    use fnv_world::land_surface::{grid_triangles, SmoothMode, SmoothParams};
    use std::collections::HashSet;

    fn hills(g: (i32, i32)) -> Vec<f32> {
        (0..SIDE * SIDE).map(|i| (((i % SIDE) as f32 * 0.4 + g.0 as f32).sin() * 60.0 + ((i / SIDE) as f32 * 0.3 + g.1 as f32).cos() * 40.0 / 8.0).round() * 8.0).collect()
    }

    fn surface(g: (i32, i32), spec: SurfaceSpec) -> Surface {
        let h = hills(g);
        let mut block: fnv_world::land_surface::Block = [[None; 3]; 3];
        block[1][1] = Some(&h);
        Surface::new(&block, spec).unwrap()
    }

    fn edge(a: usize, b: usize) -> (usize, usize) {
        (a.min(b), a.max(b))
    }

    /// The strips cover exactly the triangles' edges, each once, at every subdivision.
    #[test]
    fn the_strips_are_the_triangle_edges_each_once() {
        for side in [SIDE, 65, 129, 3] {
            let mut want = HashSet::new();
            for t in grid_triangles(side) {
                let t = t.map(|v| v as usize);
                for (a, b) in [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])] {
                    want.insert(edge(a, b));
                }
            }
            let mut got = Vec::new();
            for s in strips(side) {
                assert!(s.len() >= 2, "side {side}: a strip without an edge");
                got.extend(s.windows(2).map(|w| edge(w[0], w[1])));
            }
            let set: HashSet<_> = got.iter().copied().collect();
            assert_eq!(set.len(), got.len(), "side {side}: an edge drawn twice");
            assert_eq!(set, want, "side {side}");
            assert_eq!(got.len(), 2 * side * (side - 1) + (side - 1) * (side - 1));
        }
    }

    /// The lines lie on the surface's own vertices, moved to the cell's place once.
    #[test]
    fn the_outline_follows_the_surface_in_game_space() {
        let fit = SurfaceSpec::Smoothed(SmoothParams { mode: SmoothMode::Fit, subdiv: 4, max_deviation: 12.0 });
        for spec in [SurfaceSpec::Stored, SurfaceSpec::DEFAULT, fit] {
            let s = surface((2, -1), spec);
            let origin = Vec2::new(2.0 * fnv_world::CELL_SIZE, -fnv_world::CELL_SIZE);
            let (asset, edges) = outline(&s, origin);
            let n = s.side();
            assert_eq!(edges, 2 * n * (n - 1) + (n - 1) * (n - 1), "{spec:?}");
            let points: Vec<Vec3> = asset.strip_positions.iter().copied().filter(|p| p.is_finite()).collect();
            let [x, y, z] = s.vertex(n + 1);
            assert!(points.contains(&Vec3::new(origin.x + x, origin.y + y, z)), "{spec:?}");
            assert!(points.iter().all(|p| p.x >= origin.x && p.x <= origin.x + fnv_world::CELL_SIZE && p.y >= origin.y), "{spec:?}");
        }
    }

    /// Per quadrant as drawn: in full it is the cell's grid by quadrants (four 65 x 65 grids
    /// at 4 x 4), 2 x 2 a quarter of the vertices, and a coarse side's middle vertices on the
    /// segment between their neighbours.
    #[test]
    fn the_drawn_outline_follows_each_quadrants_level() {
        let s = surface((2, -1), SurfaceSpec::DEFAULT);
        let per = |side: usize| 2 * side * (side - 1) + (side - 1) * (side - 1);
        let (_, full) = outline_drawn(&s, Vec2::ZERO, [QuadrantDraw::FULL; 4]);
        assert_eq!(full, 4 * per(65));
        let mut coarse = QuadrantDraw::FULL;
        coarse.coarse[fnv_world::land_surface::QUADRANT_EAST] = true;
        let (asset, mixed) = outline_drawn(&s, Vec2::ZERO, [coarse, QuadrantDraw::HALF, QuadrantDraw::HALF, QuadrantDraw::HALF]);
        assert_eq!(mixed, per(65) + 3 * per(33));
        // The coarse east side of quadrant 0 (x = 2048): its vertex at row 1 lies halfway
        // between rows 0 and 2.
        let p = |r: usize| s.vertex(r * s.side() + 64);
        let mid = Vec3::new(p(1)[0], p(1)[1], (p(0)[2] + p(2)[2]) * 0.5);
        assert!(asset.strip_positions.contains(&mid));
    }

    fn app(on: bool) -> App {
        let mut app = App::new();
        // Transforms and visibility as the game propagates them.
        app.add_plugins((MinimalPlugins, bevy::asset::AssetPlugin::default(), bevy::transform::TransformPlugin, bevy::camera::CameraPlugin))
            .init_asset::<Mesh>()
            // CameraPlugin's skinned-mesh bounds system reads this store (MeshPlugin adds it in the game).
            .init_asset::<bevy::mesh::skinning::SkinnedMeshInverseBindposes>()
            .init_asset::<GizmoAsset>()
            .add_plugins(TerrainWireframePlugin { on })
            .insert_resource(WireSource::new(|world, g, spec| (world == 7 && g.0 >= 0).then(|| surface(g, spec))));
        app
    }

    fn cell(app: &mut App, x: i32, y: i32) -> Entity {
        app.world_mut().spawn((RenderCell::Exterior { world: 7, x, y }, Visibility::Visible)).id()
    }

    /// Run until the jobs are done (they run on the read pool).
    fn settle(app: &mut App) -> Counts {
        for _ in 0..500 {
            app.update();
            let c = app.world().resource::<TerrainWireframe>().counts();
            if c.pending == 0 {
                return c;
            }
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        panic!("the outline jobs did not finish");
    }

    fn gizmos(app: &mut App) -> usize {
        app.world_mut().query::<&Gizmo>().iter(app.world()).count()
    }

    /// Every outline's global transform is the game → Bevy turn.
    fn placed(app: &mut App) -> bool {
        let want = GlobalTransform::from(crate::world::game_to_bevy());
        app.world_mut().query_filtered::<&GlobalTransform, With<Gizmo>>().iter(app.world()).all(|g| *g == want)
    }

    /// Off: nothing; switched on with cells already in: each gets its lines; a cell loaded
    /// while on gets them, one unloaded or hidden loses them; switched off: no lines, no
    /// assets, no jobs.
    #[test]
    fn it_follows_the_switch_and_the_cells() {
        let mut app = app(false);
        let a = cell(&mut app, 0, 0);
        let _b = cell(&mut app, 1, 0);
        let _none = cell(&mut app, -1, 0); // no landscape there
        app.world_mut().spawn(RenderCell::Interior(5));
        settle(&mut app);
        assert_eq!((app.world().resource::<TerrainWireframe>().counts(), gizmos(&mut app)), (Counts::default(), 0));
        assert_eq!(app.world().resource::<Assets<GizmoAsset>>().len(), 0);

        app.world_mut().resource_mut::<TerrainWireframe>().on = true;
        let c = settle(&mut app);
        assert_eq!((c.drawn, c.empty), (2, 1));
        // Both cells on the run's surface: the same number of edges each.
        assert!(c.edges > 0 && c.edges.is_multiple_of(2), "{c:?}");
        // Spawned by the update that just ran, after its transform propagation: in place.
        assert_eq!(gizmos(&mut app), 2);
        assert!(placed(&mut app), "the outlines' first-frame transform");
        app.update();
        assert_eq!(gizmos(&mut app), 2);
        assert!(placed(&mut app));

        // Streaming while on.
        let d = cell(&mut app, 0, 1);
        let c = settle(&mut app);
        assert_eq!(c.drawn, 3);
        // Unloaded, and hidden (its Visibility, propagated): both outlines go in that frame.
        app.world_mut().despawn(d);
        *app.world_mut().get_mut::<Visibility>(a).unwrap() = Visibility::Hidden;
        app.update();
        let c = app.world().resource::<TerrainWireframe>().counts();
        assert_eq!((c.drawn, c.pending, gizmos(&mut app)), (1, 0, 1));
        assert_eq!(app.world().resource::<Assets<GizmoAsset>>().len(), 1);

        app.world_mut().resource_mut::<TerrainWireframe>().on = false;
        app.update();
        app.update();
        assert_eq!((app.world().resource::<TerrainWireframe>().counts(), gizmos(&mut app)), (Counts::default(), 0));
        assert_eq!(app.world().resource::<Assets<GizmoAsset>>().len(), 0);
        assert!(!active_now(&mut app), "off and empty: the system no longer runs");
    }

    /// Switched off before its jobs finish: they are dropped, nothing lands afterwards.
    #[test]
    fn switching_off_drops_the_jobs_not_done() {
        let mut app = app(true);
        cell(&mut app, 0, 0);
        cell(&mut app, 1, 1);
        app.update();
        app.world_mut().resource_mut::<TerrainWireframe>().on = false;
        for _ in 0..20 {
            app.update();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert_eq!((app.world().resource::<TerrainWireframe>().counts(), gizmos(&mut app)), (Counts::default(), 0));
        assert_eq!(app.world().resource::<Assets<GizmoAsset>>().len(), 0);
    }

    fn active_now(app: &mut App) -> bool {
        let w = app.world().resource::<TerrainWireframe>();
        w.on || w.was_on || !w.cells.is_empty()
    }
}
