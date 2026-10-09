//! WorldStreamingPlugin: load the cells around the camera and spawn their terrain and
//! references; unload cells that fall out of range.

use crate::fade::CellFade;
use crate::terrain::{spawn_terrain, TerrainCtx, TerrainMaterials};
use crate::water::{water_material, WaterCtx};
use bevy::math::Affine3A;
use bevy::prelude::*;
use fnv_esm::{LoadOrder, Plugin as EsmPlugin, Schema as EsmSchema};
use fnv_world::placed::{base_object, placement, BaseObject, Placement};
use fnv_world::transform::ref_rotation;
use fnv_world::{cell_of, WorldIndex, CELL_SIZE};
use nvf_assets::actor::{ActorCtx, ActorParts};
use nvf_assets::model::ModelCtx;
use nvf_assets::{nif_material, NifMaterial};
use nvf_play::{RefVisual, RenderCell};
use std::collections::{HashMap, HashSet};
use std::f32::consts::FRAC_PI_2;
use std::path::Path;

/// Base object types drawn as static models. NPCs and creatures are assembled from parts
/// (`actors.rs`); sounds and idle markers have no model.
const DRAWN: [&[u8; 4]; 20] = [
    b"STAT", b"SCOL", b"MSTT", b"TREE", b"FURN", b"DOOR", b"CONT", b"ACTI", b"TACT", b"TERM", b"LIGH", b"MISC",
    b"WEAP", b"AMMO", b"ARMO", b"ALCH", b"BOOK", b"NOTE", b"KEYM", b"PWAT",
];

#[derive(Resource)]
pub struct WorldData {
    /// Leaked (they live for the whole run) so `nvf-play` can share them.
    pub lo: &'static LoadOrder<'static>,
    pub schema: &'static EsmSchema,
    pub index: &'static WorldIndex,
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
        let lo: &'static LoadOrder<'static> = Box::leak(Box::new(LoadOrder::new(plugins).map_err(|e| e.to_string())?));
        let index: &'static WorldIndex = Box::leak(Box::new(WorldIndex::build(lo, schema)));
        let w = index.world_by_name(world).ok_or_else(|| format!("no worldspace {world}"))?;
        let mut data = WorldData { lo, schema, index, world: w.form_id, persistent_by_grid: HashMap::new() };
        data.set_world(w.form_id);
        Ok(data)
    }

    /// Draw worldspace `world` (FormID): index its persistent references by grid square.
    pub fn set_world(&mut self, world: u32) {
        self.world = world;
        self.persistent_by_grid.clear();
        let pc = self.index.worldspaces.get(&world).and_then(|w| w.persistent_cell).and_then(|c| self.index.cells.get(&c));
        for &r in pc.map_or(&[][..], |c| &c.refs) {
            if let Some(p) = placement(self.lo, self.schema, r) {
                self.persistent_by_grid.entry(cell_of(p.position[0], p.position[1])).or_default().push(r);
            }
        }
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
            Some((parent, opposite)) => match placement(self.lo, self.schema, parent) {
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

/// A SpeedTree reference drawn as a billboard; `face_billboards` keeps it facing the camera.
#[derive(Component)]
pub struct TreeBillboard;

#[derive(Resource)]
pub struct Streamer {
    /// Cells within this many grid steps of the camera's cell are loaded.
    pub radius: i32,
    /// Cells drawn, including exterior cells still fading out.
    pub loaded: HashMap<(i32, i32), Entity>,
    /// Exterior cells fading in or out (fade.rs); loaded cells not in here are fully drawn.
    pub fades: HashMap<(i32, i32), CellFade>,
    /// Seconds a cell takes to fade in or out; 0 switches cells at once.
    pub fade_secs: f32,
    /// The cells around the start (or a door's destination) are in: cells loaded from now
    /// on fade in (the first ones appear at once, as behind a loading screen).
    pub settled: bool,
    /// Cells loaded per frame (loading is synchronous for now).
    pub per_frame: usize,
    /// Model path fragments not to draw (diagnostics).
    pub hide: Vec<String>,
    /// Interior mode: this one cell instead of the exterior grid.
    pub interior: Option<u32>,
    /// Multiplier on every LIGH light's brightness (1 = as stored).
    pub light_scale: f32,
    pub stats: StreamStats,
}

impl Streamer {
    /// How much of an exterior cell is drawn: 0 not loaded … 1 fully drawn.
    pub fn cell_level(&self, c: (i32, i32)) -> f32 {
        match self.fades.get(&c) {
            Some(f) => f.level,
            None if self.loaded.contains_key(&c) => 1.0,
            None => 0.0,
        }
    }
}

/// Which cell to load.
#[derive(Debug, Clone, Copy)]
pub enum CellKey {
    Exterior((i32, i32)),
    Interior(u32),
}

#[derive(Debug, Default, Clone)]
pub struct StreamStats {
    pub cells_loaded: usize,
    pub refs_drawn: usize,
    pub refs_skipped_disabled: usize,
    pub refs_skipped_type: usize,
    pub refs_no_model: usize,
    /// TREE references (SpeedTree, generated or as billboards).
    pub refs_speedtree: usize,
    /// References drawn with their own node hierarchy, playing an Idle sequence.
    pub refs_animated: usize,
    /// Of those, references whose base has "Random Anim Start" (a spread start phase).
    pub refs_random_start: usize,
    pub water_planes: usize,
    pub lights: usize,
    /// Light sources of references not enabled at the start, spawned hidden.
    pub lights_hidden: usize,
    pub actors: crate::actors::ActorStats,
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
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut models: ModelCtx,
    mut terrain: TerrainMaterials,
    mut water: WaterCtx,
    mut trees: crate::tree::TreeCtx,
    mut emittance: ResMut<crate::emittance::EmittanceState>,
    time: Res<Time>,
    actor_world: Option<Res<crate::actors::ActorWorld>>,
    mut actor_parts: ResMut<ActorParts>,
) {
    let (Ok(root), Ok(cam)) = (root.single(), camera.single()) else { return };
    let mut actors = actor_world.as_deref().map(|w| (w, &mut *actor_parts));
    if let Some(id) = streamer.interior {
        if streamer.loaded.is_empty() {
            let t0 = std::time::Instant::now();
            let s = &mut *streamer;
            let e = load_cell(&mut commands, &world, CellKey::Interior(id), &mut models, &mut terrain, &mut water, &mut trees, &mut emittance, &mut actors, s);
            commands.entity(root).add_child(e);
            streamer.loaded.insert((i32::MIN, i32::MIN), e);
            streamer.stats.cells_loaded += 1;
            streamer.stats.load_ms += t0.elapsed().as_secs_f64() * 1000.0;
        }
        return;
    }
    let g = bevy_to_game(cam.translation());
    let centre = cell_of(g.x, g.y);
    let r = streamer.radius;
    let want: HashSet<(i32, i32)> =
        (-r..=r).flat_map(|dx| (-r..=r).map(move |dy| (centre.0 + dx, centre.1 + dy))).collect();

    // Unload with one cell of hysteresis, fading out first (fade.rs).
    let far: Vec<(i32, i32)> = streamer
        .loaded
        .keys()
        .filter(|c| (c.0 - centre.0).abs() > r + 1 || (c.1 - centre.1).abs() > r + 1)
        .copied()
        .collect();
    let fading = streamer.fade_secs > 0.0;
    for c in far {
        if fading {
            streamer.fades.entry(c).or_insert(CellFade::new(1.0, true)).leaving = true;
        } else if let Some(e) = streamer.loaded.remove(&c) {
            commands.entity(e).despawn();
        }
    }
    // Cells wanted again while fading out fade back in.
    for c in &want {
        if let Some(f) = streamer.fades.get_mut(c) {
            f.leaving = false;
        }
    }
    let rate = if fading { time.delta_secs() / streamer.fade_secs } else { 1.0 };
    let mut gone = Vec::new();
    for (c, f) in &mut streamer.fades {
        f.level = (f.level + if f.leaving { -rate } else { rate }).clamp(0.0, 1.0);
        if f.leaving && f.level <= 0.0 {
            gone.push(*c);
        }
    }
    for c in gone {
        streamer.fades.remove(&c);
        if let Some(e) = streamer.loaded.remove(&c) {
            commands.entity(e).despawn();
        }
    }

    let mut missing: Vec<(i32, i32)> = want.iter().filter(|c| !streamer.loaded.contains_key(c)).copied().collect();
    missing.sort_by_key(|c| (c.0 - centre.0).abs().max((c.1 - centre.1).abs()) * 1000 + (c.0 - centre.0).abs() + (c.1 - centre.1).abs());
    for c in missing.into_iter().take(streamer.per_frame) {
        let t0 = std::time::Instant::now();
        let s = &mut *streamer;
        let e = load_cell(&mut commands, &world, CellKey::Exterior(c), &mut models, &mut terrain, &mut water, &mut trees, &mut emittance, &mut actors, s);
        commands.entity(root).add_child(e);
        streamer.loaded.insert(c, e);
        if fading {
            // Before the start square is in, cells appear at once; their meshes still need
            // their (fully drawn) ranges (fade.rs).
            let level = if streamer.settled { 0.0 } else { 1.0 };
            streamer.fades.insert(c, CellFade::new(level, false));
        }
        streamer.stats.cells_loaded += 1;
        streamer.stats.load_ms += t0.elapsed().as_secs_f64() * 1000.0;
    }
    if !streamer.settled && want.iter().all(|c| streamer.loaded.contains_key(c)) {
        streamer.settled = true;
    }
}

/// Turn every tree billboard about its vertical (game Z) axis so that its face (local
/// +X) points at the camera. Billboards are children of cells, which only translate, and
/// the cells of the world root, so a yaw about game Z in the billboard's own transform
/// is a yaw about the world's up axis.
pub fn face_billboards(
    camera: Query<&GlobalTransform, (With<Camera3d>, Without<crate::reflection::ReflectionCamera>)>,
    mut trees: Query<(&mut Transform, &GlobalTransform), With<TreeBillboard>>,
) {
    let Ok(cam) = camera.single() else { return };
    let eye = bevy_to_game(cam.translation());
    for (mut t, g) in &mut trees {
        let d = eye - bevy_to_game(g.translation());
        if d.x * d.x + d.y * d.y > 1e-6 {
            t.rotation = Quat::from_rotation_z(d.y.atan2(d.x));
        }
    }
}

/// `nvf_play::Travel` (door teleports): drop the loaded cells, switch between the
/// worldspace and an interior; streaming reloads around the camera, which the player
/// systems have moved to the destination.
pub fn travel(
    mut commands: Commands,
    mut msgs: MessageReader<nvf_play::Travel>,
    world: Res<WorldData>,
    mut streamer: ResMut<Streamer>,
    mut sky: ResMut<crate::sky::SkyState>,
    mut lod: ResMut<crate::lod::LodState>,
    args: Res<crate::Args>,
) {
    let Some(t) = msgs.read().last().copied() else { return };
    for (_, e) in streamer.loaded.drain() {
        commands.entity(e).despawn();
    }
    // The destination appears at once, as after a loading screen.
    streamer.fades.clear();
    streamer.settled = false;
    streamer.interior = t.interior;
    sky.set_interior(t.interior.and_then(|id| fnv_world::lighting::cell_lighting(world.lo, world.schema, id)));
    sky.interior_grade = t.interior.and_then(|id| crate::sky::SkyState::cell_grade(world.lo, world.schema, id));
    lod.set_enabled(&mut commands, t.interior.is_none() && !args.no_lod);
}

fn ref_affine(p: &Placement) -> Affine3A {
    let m = Mat3::from_cols_array(&ref_rotation(p.rotation)).transpose(); // row-major
    Affine3A::from_mat3_translation(m * p.scale, Vec3::from_array(p.position))
}

#[allow(clippy::too_many_arguments)]
fn load_cell(
    commands: &mut Commands,
    world: &WorldData,
    key: CellKey,
    models: &mut ModelCtx,
    terrain: &mut TerrainMaterials,
    water: &mut WaterCtx,
    trees: &mut crate::tree::TreeCtx,
    emittance: &mut crate::emittance::EmittanceState,
    actors: &mut Option<(&crate::actors::ActorWorld, &mut ActorParts)>,
    streamer: &mut Streamer,
) -> Entity {
    let (hide, light_scale, stats) = (&streamer.hide, streamer.light_scale, &mut streamer.stats);
    let (origin, cell, name) = match key {
        CellKey::Exterior(g) => {
            (Vec3::new(g.0 as f32 * CELL_SIZE, g.1 as f32 * CELL_SIZE, 0.0), world.grid_cell(g), format!("Cell {},{}", g.0, g.1))
        }
        CellKey::Interior(id) => (Vec3::ZERO, world.index.cells.get(&id), format!("Interior {id:08X}")),
    };
    let cell_e = commands.spawn((Name::new(name), CellEntity, Transform::from_translation(origin), Visibility::default())).id();
    // Hook for nvf-play: it draws the references the simulation created (dropped items)
    // in this cell as children of this entity.
    commands.entity(cell_e).insert(match key {
        CellKey::Exterior((x, y)) => RenderCell::Exterior { world: world.world, x, y },
        CellKey::Interior(id) => RenderCell::Interior(id),
    });
    let mut refs: Vec<u32> = cell.map(|c| c.refs.clone()).unwrap_or_default();
    if let CellKey::Exterior(g) = key {
        if let Some(land) = cell.and_then(|c| c.land) {
            spawn_terrain(commands, cell_e, world, land, g, &mut TerrainCtx { tm: terrain, assets: models });
        }
        if let Some(p) = world.persistent_by_grid.get(&g) {
            refs.extend(p);
        }
    }
    if let Some(c) = cell {
        spawn_water(commands, cell_e, world, c, models, water, stats);
    }
    let to_cell = Affine3A::from_translation(-origin);
    for r in refs {
        let Some(p) = placement(world.lo, world.schema, r) else { continue };
        if !world.enabled(&p, 0) {
            // Actors, creatures and light sources are spawned hidden, so a script's
            // Enable can show them (actors.rs `SpawnedDisabled`); other models are
            // nvf-play's (`script_draw.rs`).
            if !p.deleted() {
                spawn_dormant(commands, cell_e, world, &p, r, to_cell, models, actors, light_scale, emittance, stats);
            }
            stats.refs_skipped_disabled += 1;
            continue;
        }
        let base: Option<BaseObject> = p.base.and_then(|b| base_object(world.lo, world.schema, b));
        if let (Some((aw, parts)), Some(b)) = (actors.as_mut(), base.as_ref().filter(|b| &b.sig == b"NPC_")) {
            let mut ctx = ActorCtx { cache: parts, models };
            if crate::actors::spawn(commands, cell_e, to_cell * ref_affine(&p), Vec3::from_array(p.position), r, b.form_id, aw, &mut ctx, &mut stats.actors, false) {
                continue;
            }
        }
        // Placed creatures (actors.rs `spawn_creature`, actors.md §9).
        if let (Some((aw, parts)), Some(b)) = (actors.as_mut(), base.as_ref().filter(|b| &b.sig == b"CREA")) {
            let mut ctx = ActorCtx { cache: parts, models };
            if crate::actors::spawn_creature(commands, cell_e, to_cell * ref_affine(&p), r, b.form_id, aw, &mut ctx, &mut stats.actors, false) {
                continue;
            }
        }
        let Some(base) = base.filter(|b| DRAWN.contains(&&b.sig)) else {
            stats.refs_skipped_type += 1;
            let t = base_type_name(p.base.and_then(|b| base_object(world.lo, world.schema, b)).as_ref());
            *stats.skipped_types.entry(t).or_default() += 1;
            continue;
        };
        if &base.sig == b"LIGH"
            && crate::lights::spawn_light(
                commands,
                cell_e,
                world,
                base.form_id,
                (to_cell * ref_affine(&p)).translation.into(),
                light_scale,
                p.emittance,
                emittance,
            )
        {
            stats.lights += 1;
        }
        if base.is_speedtree() {
            // Geometry generated from the .spt parameters (tree.rs); without it, the
            // tree's billboard as one upright quad turned towards the camera. The
            // billboard drops the reference's rotation: it has no facing of its own.
            if let Some(v) = trees.visual(world, base.form_id, models) {
                let rt = Mat4::from(to_cell * ref_affine(&p));
                stats.shapes += crate::tree::spawn(commands, cell_e, &v, rt, RefVisual(r));
                stats.refs_speedtree += 1;
                continue;
            }
            if let Some((mesh, mat)) = billboard(world, base.form_id, models) {
                let at = Vec3::from_array(p.position) - origin;
                let t = Transform::from_translation(at).with_scale(Vec3::splat(p.scale));
                commands.entity(cell_e).with_child((Mesh3d(mesh), MeshMaterial3d(mat), t, TreeBillboard, RefVisual(r)));
                stats.shapes += 1;
            }
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
        // The reference transform replaces the model root's own (world.md §4).
        let rt = to_cell * ref_affine(&p);
        if let Some(rig) = &model.rig {
            // Animated (Idle sequence): its own node hierarchy per reference.
            // "Random Anim Start" bases start their idle at a phase spread by reference
            // (fnv_actor::anims::random_anim_start; animation.md §8).
            let phase = if random_anim_start(world, base.form_id) { crate::actors::clip_phase(r) } else { 0.0 };
            stats.refs_random_start += usize::from(phase > 0.0);
            let (root, drawn) = rig.spawn_at_phase(commands, cell_e, Transform::from_matrix(Mat4::from(rt)), &path, phase);
            commands.entity(root).insert(RefVisual(r));
            for e in &drawn {
                commands.entity(*e).insert(RefVisual(r));
            }
            stats.shapes += drawn.len();
            stats.refs_drawn += 1;
            stats.refs_animated += 1;
            continue;
        }
        for s in &model.shapes {
            let m = Mat4::from(rt * s.transform);
            // External emittance: the reference's XEMI source drives the colour.
            let material = p
                .emittance
                .filter(|_| s.external_emittance)
                .and_then(|x| emittance.material(world, &mut models.materials, &s.material, x))
                .unwrap_or_else(|| s.material.clone());
            commands.entity(cell_e).with_child((Mesh3d(s.mesh.clone()), MeshMaterial3d(material), Transform::from_matrix(m), RefVisual(r)));
            stats.shapes += 1;
        }
        stats.refs_drawn += 1;
    }
    cell_e
}

/// Whether base form `fid` has the "Random Anim Start" record flag (fnv_actor::anims).
fn random_anim_start(world: &WorldData, fid: u32) -> bool {
    let Some(d) = world.lo.index.get(&fid).and_then(|d| d.last()) else { return false };
    let h = &world.lo.records[d.plugin as usize][d.ordinal as usize].header;
    fnv_actor::anims::random_anim_start(&h.sig, h.flags)
}

/// A reference not enabled at the start of a game: an NPC or creature is spawned
/// hidden, and a light's light source hidden under a `RefVisual` holder, so that
/// `RefVisual` sync shows them when a script enables the reference (actors.rs
/// `SpawnedDisabled`; docs/design/layer5-play.md §4). The light's model is drawn by
/// nvf-play's `script_draw.rs` when enabled.
#[allow(clippy::too_many_arguments)]
fn spawn_dormant(
    commands: &mut Commands,
    cell_e: Entity,
    world: &WorldData,
    p: &Placement,
    r: u32,
    to_cell: Affine3A,
    models: &mut ModelCtx,
    actors: &mut Option<(&crate::actors::ActorWorld, &mut ActorParts)>,
    light_scale: f32,
    emittance: &mut crate::emittance::EmittanceState,
    stats: &mut StreamStats,
) {
    let Some(base) = p.base.and_then(|b| base_object(world.lo, world.schema, b)) else { return };
    let rt = to_cell * ref_affine(p);
    match (&base.sig, actors.as_mut()) {
        (b"NPC_", Some((aw, parts))) => {
            let mut ctx = ActorCtx { cache: parts, models };
            crate::actors::spawn(commands, cell_e, rt, Vec3::from_array(p.position), r, base.form_id, aw, &mut ctx, &mut stats.actors, true);
        }
        (b"CREA", Some((aw, parts))) => {
            let mut ctx = ActorCtx { cache: parts, models };
            crate::actors::spawn_creature(commands, cell_e, rt, r, base.form_id, aw, &mut ctx, &mut stats.actors, true);
        }
        (b"LIGH", _) => {
            let holder = commands.spawn((Transform::IDENTITY, Visibility::Inherited, RefVisual(r), ChildOf(cell_e))).id();
            if crate::lights::spawn_light(commands, holder, world, base.form_id, rt.translation.into(), light_scale, p.emittance, emittance) {
                crate::actors::spawn_hidden(commands, holder, r);
                stats.lights_hidden += 1;
            } else {
                commands.entity(holder).despawn();
            }
        }
        _ => {}
    }
}

/// Water surface of a cell: cells flagged "has water" get a plane at XCLW, or at the
/// worldspace's default water height when XCLW is absent or the 3.4e38 sentinel
/// [hypothesis, world.md §1]; the water type is CELL XCWT, else the worldspace's NAM2.
fn spawn_water(
    commands: &mut Commands,
    cell_e: Entity,
    world: &WorldData,
    c: &fnv_world::Cell,
    models: &mut ModelCtx,
    water: &mut WaterCtx,
    stats: &mut StreamStats,
) {
    if !c.has_water() {
        return;
    }
    let ws = &world.index.worldspaces[&world.world];
    // The same surface the physics swims in (`Cell::water_surface`, physics.md §6.6).
    let Some(height) = c.water_surface(ws.default_water_height) else { return };
    let Some(wid) = c.water_type.or(ws.water) else { return };
    let Some(mat) = water_material(world, wid, models, water) else { return };
    let s = CELL_SIZE;
    let mesh = water.plane(&mut models.meshes);
    commands.entity(cell_e).with_child((
        Mesh3d(mesh),
        MeshMaterial3d(mat),
        Transform::from_xyz(s / 2.0, s / 2.0, height),
        bevy::camera::visibility::RenderLayers::layer(crate::reflection::WATER_LAYER),
    ));
    stats.water_planes += 1;
}

/// Billboard quad mesh and material for a TREE base (cached per base).
fn billboard(world: &WorldData, tree: u32, models: &mut ModelCtx) -> Option<(Handle<Mesh>, Handle<NifMaterial>)> {
    if let Some(b) = models.cache.billboard(tree) {
        return b;
    }
    let made = (|| {
        let ([w, h], tex) = fnv_world::placed::tree_billboard(world.lo, world.schema, tree)?;
        let a = &mut *models;
        let image = a.textures.get(&tex, true, &a.data, &mut a.images, a.formats.as_deref())?;
        let hw = w / 2.0;
        // One vertical quad in the local YZ plane facing +X, bottom at z = 0 (game space);
        // `face_billboards` turns +X towards the camera. Normals point up, so the
        // pre-shaded billboard picture is lit like the ground under it rather than
        // darkening when seen against the sun [hypothesis: chosen by eye].
        // Counter-clockwise seen from +X (local +Y is to the viewer's right), so the front
        // face is the one shown: double-sided materials flip the normal on back faces.
        let pos = vec![[0.0, -hw, 0.0], [0.0, hw, 0.0], [0.0, hw, h], [0.0, -hw, h]];
        let nor = vec![[0.0, 0.0, 1.0]; 4];
        let uv = vec![[0.0, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]];
        let mut mesh = Mesh::new(bevy::mesh::PrimitiveTopology::TriangleList, bevy::asset::RenderAssetUsages::RENDER_WORLD);
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
        mesh.insert_indices(bevy::mesh::Indices::U32(vec![0, 1, 2, 0, 2, 3]));
        let mat = a.materials.add(nif_material(StandardMaterial {
            base_color_texture: Some(image),
            alpha_mode: AlphaMode::Mask(0.5),
            double_sided: true,
            cull_mode: None,
            perceptual_roughness: 0.9,
            ..default()
        }));
        Some((a.meshes.add(mesh), mat))
    })();
    models.cache.set_billboard(tree, made.clone());
    made
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
