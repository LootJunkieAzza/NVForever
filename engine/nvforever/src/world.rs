//! WorldStreamingPlugin: load the cells around the camera and spawn their terrain and
//! references; unload cells that fall out of range.

use crate::terrain::{spawn_terrain, TerrainCtx, TerrainMaterials};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_esm::{LoadOrder, Plugin as EsmPlugin, Schema as EsmSchema};
use fnv_world::placed::{base_object, placement, BaseObject, Placement};
use fnv_world::transform::ref_rotation;
use fnv_world::{cell_of, WorldIndex, CELL_SIZE};
use nvf_assets::model::ModelCtx;
use std::collections::{HashMap, HashSet};
use std::f32::consts::FRAC_PI_2;
use std::path::Path;

/// Base object types drawn as static models. Actors (NPC_, CREA) need Layer 4 records;
/// sounds and idle markers have no model.
const DRAWN: [&[u8; 4]; 20] = [
    b"STAT", b"SCOL", b"MSTT", b"TREE", b"FURN", b"DOOR", b"CONT", b"ACTI", b"TACT", b"TERM", b"LIGH", b"MISC",
    b"WEAP", b"AMMO", b"ARMO", b"ALCH", b"BOOK", b"NOTE", b"KEYM", b"PWAT",
];

#[derive(Resource)]
pub struct WorldData {
    pub lo: LoadOrder<'static>,
    pub schema: &'static EsmSchema,
    pub index: WorldIndex,
    pub world: u32,
    /// The worldspace's persistent references by the grid square they stand in.
    pub persistent_by_grid: HashMap<(i32, i32), Vec<u32>>,
}

impl WorldData {
    pub fn open(data_dir: &Path, schema_dir: &Path, plugins: &[String], world: &str) -> Result<WorldData, String> {
        let schema: &'static EsmSchema =
            Box::leak(Box::new(EsmSchema::load_dir(schema_dir).map_err(|e| format!("{}: {e}", schema_dir.display()))?));
        let plugins: Vec<EsmPlugin> = plugins
            .iter()
            .map(|p| EsmPlugin::open(data_dir.join(p)).map_err(|e| format!("{p}: {e}")))
            .collect::<Result<_, _>>()?;
        let plugins: &'static [EsmPlugin] = Box::leak(plugins.into_boxed_slice());
        let lo = LoadOrder::new(plugins).map_err(|e| e.to_string())?;
        let index = WorldIndex::build(&lo, schema);
        let w = index.world_by_name(world).ok_or_else(|| format!("no worldspace {world}"))?;
        let world_id = w.form_id;
        let mut persistent_by_grid: HashMap<(i32, i32), Vec<u32>> = HashMap::new();
        if let Some(pc) = w.persistent_cell.and_then(|c| index.cells.get(&c)) {
            for &r in &pc.refs {
                if let Some(p) = placement(&lo, schema, r) {
                    persistent_by_grid.entry(cell_of(p.position[0], p.position[1])).or_default().push(r);
                }
            }
        }
        Ok(WorldData { lo, schema, index, world: world_id, persistent_by_grid })
    }

    pub fn grid_cell(&self, g: (i32, i32)) -> Option<&fnv_world::Cell> {
        let w = self.index.worldspaces.get(&self.world)?;
        self.index.cells.get(w.grid.get(&g)?)
    }

    /// Whether a reference is enabled at the start of a game: not initially disabled,
    /// and its enable parent (if any) in the required state [documented: XESP; the
    /// parent chain is followed, cycles count as disabled].
    fn enabled(&self, p: &Placement, depth: usize) -> bool {
        if p.deleted() || p.initially_disabled() {
            return false;
        }
        match p.enable_parent {
            None => true,
            Some(_) if depth > 16 => false,
            Some((parent, opposite)) => match placement(&self.lo, self.schema, parent) {
                Some(pp) => self.enabled(&pp, depth + 1) != opposite,
                None => !opposite,
            },
        }
    }
}

/// Marks the root that maps game space (Z up) to Bevy space (Y up).
#[derive(Component)]
pub struct WorldRoot;

#[derive(Component)]
pub struct CellEntity;

#[derive(Resource)]
pub struct Streamer {
    /// Cells within this many grid steps of the camera's cell are loaded.
    pub radius: i32,
    pub loaded: HashMap<(i32, i32), Entity>,
    /// Cells loaded per frame (loading is synchronous for now).
    pub per_frame: usize,
    /// Model path fragments not to draw (diagnostics).
    pub hide: Vec<String>,
    pub stats: StreamStats,
}

#[derive(Debug, Default, Clone)]
pub struct StreamStats {
    pub cells_loaded: usize,
    pub refs_drawn: usize,
    pub refs_skipped_disabled: usize,
    pub refs_skipped_type: usize,
    pub refs_no_model: usize,
    /// TREE references: SpeedTree models are not drawn yet (layer3.md work item).
    pub refs_speedtree: usize,
    pub shapes: usize,
    pub load_ms: f64,
    pub skipped_types: HashMap<String, usize>,
}

/// Game space → Bevy space.
pub fn game_to_bevy() -> Transform {
    Transform::from_rotation(Quat::from_rotation_x(-FRAC_PI_2))
}

/// Bevy-space position → game-space position.
pub fn bevy_to_game(p: Vec3) -> Vec3 {
    Vec3::new(p.x, -p.z, p.y)
}

pub fn setup_root(mut commands: Commands) {
    commands.spawn((Name::new("World"), WorldRoot, game_to_bevy(), Visibility::default()));
}

#[allow(clippy::too_many_arguments)]
pub fn stream(
    mut commands: Commands,
    world: Res<WorldData>,
    mut streamer: ResMut<Streamer>,
    root: Query<Entity, With<WorldRoot>>,
    camera: Query<&GlobalTransform, With<Camera3d>>,
    mut models: ModelCtx,
    mut terrain: TerrainMaterials,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let g = bevy_to_game(cam.translation());
    let centre = cell_of(g.x, g.y);
    let r = streamer.radius;
    let want: HashSet<(i32, i32)> =
        (-r..=r).flat_map(|dx| (-r..=r).map(move |dy| (centre.0 + dx, centre.1 + dy))).collect();

    // Unload with one cell of hysteresis.
    let far: Vec<(i32, i32)> = streamer
        .loaded
        .keys()
        .filter(|c| (c.0 - centre.0).abs() > r + 1 || (c.1 - centre.1).abs() > r + 1)
        .copied()
        .collect();
    for c in far {
        if let Some(e) = streamer.loaded.remove(&c) {
            commands.entity(e).despawn();
        }
    }

    let mut missing: Vec<(i32, i32)> = want.into_iter().filter(|c| !streamer.loaded.contains_key(c)).collect();
    missing.sort_by_key(|c| (c.0 - centre.0).abs().max((c.1 - centre.1).abs()) * 1000 + (c.0 - centre.0).abs() + (c.1 - centre.1).abs());
    for c in missing.into_iter().take(streamer.per_frame) {
        let t0 = std::time::Instant::now();
        let s = &mut *streamer;
        let e = load_cell(&mut commands, &world, c, &mut models, &mut terrain, &s.hide, &mut s.stats);
        commands.entity(root).add_child(e);
        streamer.loaded.insert(c, e);
        streamer.stats.cells_loaded += 1;
        streamer.stats.load_ms += t0.elapsed().as_secs_f64() * 1000.0;
    }
}

fn ref_affine(p: &Placement) -> Affine3A {
    let m = Mat3::from_cols_array(&ref_rotation(p.rotation)).transpose(); // row-major
    Affine3A::from_mat3_translation(m * p.scale, Vec3::from_array(p.position))
}

fn load_cell(
    commands: &mut Commands,
    world: &WorldData,
    g: (i32, i32),
    models: &mut ModelCtx,
    terrain: &mut TerrainMaterials,
    hide: &[String],
    stats: &mut StreamStats,
) -> Entity {
    let origin = Vec3::new(g.0 as f32 * CELL_SIZE, g.1 as f32 * CELL_SIZE, 0.0);
    let cell_e = commands
        .spawn((Name::new(format!("Cell {},{}", g.0, g.1)), CellEntity, Transform::from_translation(origin), Visibility::default()))
        .id();
    let cell = world.grid_cell(g);
    if let Some(land) = cell.and_then(|c| c.land) {
        spawn_terrain(commands, cell_e, world, land, g, &mut TerrainCtx { tm: terrain, assets: models });
    }
    let mut refs: Vec<u32> = cell.map(|c| c.refs.clone()).unwrap_or_default();
    if let Some(p) = world.persistent_by_grid.get(&g) {
        refs.extend(p);
    }
    let to_cell = Affine3A::from_translation(-origin);
    for r in refs {
        let Some(p) = placement(&world.lo, world.schema, r) else { continue };
        if !world.enabled(&p, 0) {
            stats.refs_skipped_disabled += 1;
            continue;
        }
        let base: Option<BaseObject> = p.base.and_then(|b| base_object(&world.lo, world.schema, b));
        let Some(base) = base.filter(|b| DRAWN.contains(&&b.sig)) else {
            stats.refs_skipped_type += 1;
            let t = base_type_name(p.base.and_then(|b| base_object(&world.lo, world.schema, b)).as_ref());
            *stats.skipped_types.entry(t).or_default() += 1;
            continue;
        };
        if base.is_speedtree() {
            stats.refs_speedtree += 1;
            continue;
        }
        let Some(path) = base.model_path() else {
            stats.refs_no_model += 1;
            continue;
        };
        if hide.iter().any(|h| path.contains(h.as_str())) {
            continue;
        }
        if is_marker(&path) {
            stats.refs_skipped_type += 1;
            *stats.skipped_types.entry("marker".into()).or_default() += 1;
            continue;
        }
        let Some(model) = models.model(&path) else {
            stats.refs_no_model += 1;
            continue;
        };
        let rt = to_cell * ref_affine(&p);
        for s in &model.shapes {
            let m = Mat4::from(rt * s.transform);
            commands.entity(cell_e).with_child((
                Mesh3d(s.mesh.clone()),
                MeshMaterial3d(s.material.clone()),
                Transform::from_matrix(m),
            ));
            stats.shapes += 1;
        }
        stats.refs_drawn += 1;
    }
    cell_e
}

fn base_type_name(b: Option<&BaseObject>) -> String {
    b.map_or("none".into(), |b| String::from_utf8_lossy(&b.sig).into_owned())
}

/// Editor-only marker meshes [hypothesis: the game hides them; models named marker*].
fn is_marker(path: &str) -> bool {
    let file = path.rsplit('\\').next().unwrap_or(path);
    file.starts_with("marker") || path.contains("\\markers\\")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces() {
        let t = game_to_bevy();
        let g = Vec3::new(1.0, 2.0, 3.0);
        let b = t.transform_point(g);
        assert!((bevy_to_game(b) - g).length() < 1e-5);
        // Game up (z) is Bevy up (y).
        assert!((t.transform_point(Vec3::Z) - Vec3::Y).length() < 1e-5);
    }

    #[test]
    fn markers() {
        assert!(is_marker("meshes\\marker_x.nif"));
        assert!(!is_marker("meshes\\clutter\\junk\\can.nif"));
    }
}
