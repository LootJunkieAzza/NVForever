//! Reading and building a physics cell away from the world: the plugin records (cell,
//! placements, LAND), the models' collision (archive read, NIF parse, collision blocks
//! to `fnv_collide` objects) and the rapier shapes. All of it is pure, so it runs on the
//! loader thread ([`super::loader`]) as well as inline; only inserting the result
//! ([`super::PhysState::load`], `stream`) touches the world.
//!
//! Whether a reference is in the world is not decided here: the simulation can change
//! that while a cell is being prepared, so every reference is prepared and the check is
//! made when the cell is inserted.

use super::cells::PhysCell;
use super::LoadTimes;
use crate::PlayData;
use fnv_collide::anim::ModelAnim;
use fnv_collide::CollisionObject;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::{ref_collision_with, ModelCollision, PhysParams, PreparedRef, PreparedTerrain, RefCollisionOptions, RefPose};
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::{base_object, placement};
use fnv_world::Placement;
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub(super) fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

type Objects = Arc<Vec<CollisionObject>>;

/// What a model contributes: its solid collision objects, its water volumes (phantoms
/// on the Water layer, physics.md §6.6) and, for a door model with "Open" and "Close"
/// sequences, its animation (`crate::doors`); or why it could not be read.
type ModelResult = Result<(Objects, Objects, Option<Arc<ModelAnim>>), String>;

/// Collision per model path (lower case), shared by the loader thread and the main
/// thread. A model is read once: a second caller waits for the first.
#[derive(Default)]
pub struct ModelCache {
    map: Mutex<HashMap<String, Arc<OnceLock<ModelResult>>>>,
}

impl ModelCache {
    fn slot(&self, path: &str) -> Arc<OnceLock<ModelResult>> {
        self.map.lock().unwrap_or_else(|e| e.into_inner()).entry(path.to_ascii_lowercase()).or_default().clone()
    }

    fn read(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> ModelResult {
        self.slot(path)
            .get_or_init(|| {
                let r = read_model(data, path, times);
                if let Err(e) = &r {
                    bevy::log::debug!("collision of {path}: {e}");
                }
                r
            })
            .clone()
    }

    /// A model's solid collision objects (empty when it has none), read through
    /// `fnv_collide::nif` once.
    pub fn objects(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> Result<Objects, String> {
        self.read(data, path, times).map(|(solid, _, _)| solid)
    }

    /// A door model's animation (read through the cache), when it has "Open" and
    /// "Close" sequences that move its nodes.
    pub fn door_anim(&self, data: &GameData, path: &str, times: &mut LoadTimes) -> Option<Arc<ModelAnim>> {
        self.read(data, path, times).ok().and_then(|(_, _, anim)| anim)
    }
}

/// Collision objects read from the archives through the shared cache.
struct Models<'a> {
    data: &'a GameData,
    cache: &'a ModelCache,
    times: &'a mut LoadTimes,
}

impl ModelCollision for Models<'_> {
    fn objects(&mut self, path: &str) -> Result<Objects, String> {
        self.cache.objects(self.data, path, self.times)
    }

    fn water(&mut self, path: &str) -> Objects {
        match self.cache.slot(path).get() {
            Some(Ok((_, water, _))) => water.clone(),
            _ => Arc::default(),
        }
    }
}

fn read_model(data: &GameData, path: &str, times: &mut LoadTimes) -> ModelResult {
    let schema = &*data.schema;
    let t = Instant::now();
    let read = data.vfs.read(path);
    times.read += ms_since(t);
    let bytes = match read {
        None => return Err("not found".to_string()),
        Some(Err(e)) => return Err(e.to_string()),
        Some(Ok(bytes)) => bytes,
    };
    let t = Instant::now();
    let file = NifFile::parse(&bytes, Some(schema)).map_err(|e| e.to_string());
    let dec = file.as_ref().ok().map(|file| Decoded::new(file, &bytes, schema));
    times.parse += ms_since(t);
    let file = file?;
    let t = Instant::now();
    let (solid, phantoms, _) = fnv_collide::nif::collision_and_phantoms(&file, dec.as_ref().expect("decoded with the file"));
    let water: Vec<CollisionObject> = phantoms.into_iter().filter(|o| o.layer == fnv_phys::layers::WATER).collect();
    let dec = dec.expect("decoded with the file");
    let anim = solid.iter().any(|o| o.motion == fnv_collide::Motion::Keyframed).then(|| ModelAnim::new(&file, &dec));
    let anim = anim.filter(|a| a.door().is_some()).map(Arc::new);
    times.convert += ms_since(t);
    Ok((Arc::new(solid), Arc::new(water), anim))
}

/// What preparing needs: the plugins, the archives and the world's parameters. Cheap to
/// clone (references and `Arc`s).
#[derive(Clone)]
pub struct Source {
    pub d: PlayData,
    /// Without archives only terrain and collision primitives load.
    pub data: Option<GameData>,
    pub models: Arc<ModelCache>,
    pub params: PhysParams,
}

/// One cell to prepare.
#[derive(Debug, Clone)]
pub struct Job {
    pub cell: PhysCell,
    /// The cell's references and, outdoors, the worldspace's persistent references
    /// standing in the square: sorted by FormID, no duplicates.
    pub refs: Vec<u32>,
    /// The cell's LAND record (exterior squares with terrain).
    pub land: Option<u32>,
    /// References the simulation holds at another pose than their placement (position,
    /// Euler angles; `SimState::moved_pose`), by FormID: their collision is built there.
    pub poses: Vec<(u32, [f32; 3], [f32; 3])>,
}

impl Job {
    /// The simulation's pose of `r`, when it overrides the placement.
    pub(crate) fn pose_of(&self, r: u32) -> Option<([f32; 3], [f32; 3])> {
        self.poses.binary_search_by_key(&r, |p| p.0).ok().map(|i| (self.poses[i].1, self.poses[i].2))
    }
}

/// A placement at the simulation's pose `pose` (position, Euler angles), when given.
pub fn at_pose(mut pl: Placement, pose: Option<([f32; 3], [f32; 3])>) -> Placement {
    if let Some((position, rotation)) = pose {
        pl.position = position;
        pl.rotation = rotation;
    }
    pl
}

/// A reference's collision, prepared.
pub struct RefShapes {
    /// The `refs.counts` outcomes `ref_collision` reported, in order.
    pub tallies: Vec<&'static str>,
    /// Water volumes of the model in the reference's frame (`RefCollision::water`).
    pub water: Vec<CollisionObject>,
    /// The shapes, when the reference has collision objects.
    pub prepared: Option<PreparedRef>,
    /// The collision object's node and body transforms, when the model has exactly one
    /// object and it is dynamic: physics can move the reference, and the body's pose
    /// gives the reference's (`crate::dropped::ref_pose_from_body`).
    pub dynamic_local: Option<fnv_collide::Iso>,
    /// A door that opens in place: its keyframed bodies follow its animation.
    pub door: Option<DoorBodies>,
}

/// The keyframed bodies of a door that opens in place (`crate::doors`), posed from its
/// model's "Open" and "Close" sequences.
#[derive(Clone)]
pub struct DoorBodies {
    pub anim: Arc<ModelAnim>,
    /// (object index in the reference's collision objects, node block, body transform)
    /// of each keyframed object.
    pub objects: Vec<(u16, usize, fnv_collide::Iso)>,
}

/// The model of a DOOR reference without XTEL (a door that opens in place).
fn in_place_door_model(src: &Source, pl: &Placement) -> Option<String> {
    if pl.teleport {
        return None;
    }
    let base = base_object(src.d.lo, src.d.schema, pl.base?)?;
    (&base.sig == b"DOOR").then(|| base.model_path()).flatten()
}

/// A reference of a prepared cell.
pub struct Entry {
    pub form_id: u32,
    /// `None` when its record could not be read.
    pub placement: Option<Placement>,
    pub shapes: Option<RefShapes>,
}

/// A cell ready to insert.
pub struct Prepared {
    pub cell: PhysCell,
    pub heights: Option<Vec<f32>>,
    pub terrain: Option<PreparedTerrain>,
    /// The job's references, in its (FormID) order.
    pub refs: Vec<Entry>,
    /// Time spent preparing, per part (on whichever thread prepared it).
    pub times: LoadTimes,
}

/// Prepare a cell: terrain, then every reference's collision (in the world or not).
pub fn cell(src: &Source, job: &Job) -> Prepared {
    let mut times = LoadTimes::default();
    let d = &src.d;
    let mut out =
        Prepared { cell: job.cell, heights: None, terrain: None, refs: Vec::with_capacity(job.refs.len()), times: LoadTimes::default() };
    if let (PhysCell::Exterior(x, y), Some(land)) = (job.cell, job.land) {
        let t = Instant::now();
        out.heights = load_land(d.lo, d.schema, land, LandParts::HEIGHTS).and_then(|l| l.heights);
        times.records += ms_since(t);
        if let Some(h) = &out.heights {
            let t = Instant::now();
            out.terrain = Some(PreparedTerrain::new(&src.params, (x, y), h));
            times.shapes += ms_since(t);
        }
    }
    for &r in &job.refs {
        let t = Instant::now();
        let pl = placement(d.lo, d.schema, r).map(|pl| at_pose(pl, job.pose_of(r)));
        times.records += ms_since(t);
        let shapes = pl.as_ref().map(|pl| reference(src, pl, &mut times));
        out.refs.push(Entry { form_id: r, placement: pl, shapes });
    }
    out.times = times;
    out
}

/// A placed reference with more than one dynamic collision object (a hanging lamp and its
/// chain, a globe on its stand, a bucket and its handle: the parts the model's constraints
/// join) keeps them keyframed where it was placed (physics.md §4.3.1): the constraints are
/// not modelled, so the parts fell apart, and the reference is drawn where it was placed
/// whatever its bodies do. Tallied as `jointed_held`.
fn hold_jointed(objects: &mut [fnv_collide::CollisionObject], tallies: &mut Vec<&'static str>) {
    use fnv_collide::Motion;
    if objects.iter().filter(|o| o.motion == Motion::Dynamic).count() < 2 {
        return;
    }
    for o in objects.iter_mut().filter(|o| o.motion == Motion::Dynamic) {
        o.motion = Motion::Keyframed;
    }
    tallies.push("jointed_held");
}

/// A placed static (base STAT, or a static collection SCOL) never moves, whatever its
/// model's Havok data says: its dynamic bodies are made fixed (physics.md §4.3.2;
/// playtest list 5 item 59). Tallied as `static_base_fixed`.
///
/// [observed: the three motorcycles outside the Goodsprings saloon are STAT
/// `Motorcycle01Static` references whose model, `vehicles\motorcycle01.nif`, has one dynamic
/// body (Props layer, 600 kg), the same model as the MSTT `Motorcycle01`; the owner reports
/// they do not move in the game. 1,131 STAT and 19 SCOL references placed in FalloutNV.esm use a
/// model with dynamic bodies, most of them `1stPerson…` weapon statics; hypothesis: the
/// game simulates Havok only for the moveable record types]
pub(super) fn fix_static_base(base_sig: Option<[u8; 4]>, objects: &mut [fnv_collide::CollisionObject], tallies: &mut Vec<&'static str>) {
    use fnv_collide::Motion;
    if !matches!(base_sig.as_ref(), Some(b"STAT" | b"SCOL")) || !objects.iter().any(|o| o.motion == Motion::Dynamic) {
        return;
    }
    for o in objects.iter_mut().filter(|o| o.motion == Motion::Dynamic) {
        o.motion = Motion::Static;
    }
    tallies.push("static_base_fixed");
}

/// Prepare one placed reference's collision (`fnv_phys::ref_collision`, the rule
/// `fnvtool phys cell` uses), its dynamic bodies asleep, a static's fixed
/// ([`fix_static_base`]) and a jointed object's held ([`hold_jointed`]).
pub fn reference(src: &Source, pl: &Placement, times: &mut LoadTimes) -> RefShapes {
    let t0 = Instant::now();
    let before = *times;
    let mut tallies = Vec::new();
    // A door with an animation keeps its moving parts even when it starts open: they are
    // posed from the animation (`fnv_phys::RefCollisionOptions::posed_doors`).
    let anim = match (&src.data, in_place_door_model(src, pl)) {
        (Some(data), Some(path)) => src.models.door_anim(data, &path, times),
        _ => None,
    };
    let options = RefCollisionOptions { posed_doors: anim.is_some() };
    let mut models = src.data.as_ref().map(|data| Models { data, cache: &src.models, times: &mut *times });
    let rc = ref_collision_with(
        src.d.lo,
        src.d.schema,
        pl,
        models.as_mut().map(|m| m as &mut dyn ModelCollision),
        &mut |what| tallies.push(what),
        options,
    );
    let in_models = (times.read - before.read) + (times.parse - before.parse) + (times.convert - before.convert);
    times.records += ms_since(t0) - in_models;
    let mut rc = rc;
    fix_static_base(rc.base_sig, &mut rc.objects, &mut tallies);
    hold_jointed(&mut rc.objects, &mut tallies);
    // Clutter starts asleep (physics.md §4.3.1).
    let prepared = (!rc.objects.is_empty()).then(|| {
        let t = Instant::now();
        let p = PreparedRef::new(&src.params, &RefPose::from_placement(pl), &rc.objects).asleep();
        times.shapes += ms_since(t);
        p
    });
    let dynamic_local = match rc.objects.as_slice() {
        [o] if o.motion == fnv_collide::Motion::Dynamic => Some(o.node_transform.compose(&o.body_transform)),
        _ => None,
    };
    let door = anim.map(|anim| DoorBodies {
        anim,
        objects: rc
            .objects
            .iter()
            .enumerate()
            .filter(|(_, o)| o.motion == fnv_collide::Motion::Keyframed)
            .map(|(i, o)| (i as u16, o.node_block, o.body_transform))
            .collect(),
    });
    RefShapes { tallies, water: rc.water, prepared, dynamic_local, door }
}
