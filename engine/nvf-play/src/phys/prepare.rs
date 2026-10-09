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
use fnv_collide::CollisionObject;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::{ref_collision, ModelCollision, PhysParams, PreparedRef, PreparedTerrain, RefPose};
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::placement;
use fnv_world::Placement;
use nvf_assets::GameData;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

pub(super) fn ms_since(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

type Objects = Arc<Vec<CollisionObject>>;

/// What a model contributes: its solid collision objects and its water volumes (phantoms
/// on the Water layer, physics.md §6.6), or why it could not be read.
type ModelResult = Result<(Objects, Objects), String>;

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
        self.read(data, path, times).map(|(solid, _)| solid)
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
            Some(Ok((_, water))) => water.clone(),
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
    times.convert += ms_since(t);
    Ok((Arc::new(solid), Arc::new(water)))
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
}

/// A reference's collision, prepared.
pub struct RefShapes {
    /// The `refs.counts` outcomes `ref_collision` reported, in order.
    pub tallies: Vec<&'static str>,
    /// Water volumes of the model in the reference's frame (`RefCollision::water`).
    pub water: Vec<CollisionObject>,
    /// The shapes, when the reference has collision objects.
    pub prepared: Option<PreparedRef>,
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
        let pl = placement(d.lo, d.schema, r);
        times.records += ms_since(t);
        let shapes = pl.as_ref().map(|pl| reference(src, pl, &mut times));
        out.refs.push(Entry { form_id: r, placement: pl, shapes });
    }
    out.times = times;
    out
}

/// Prepare one placed reference's collision (`fnv_phys::ref_collision`, the rule
/// `fnvtool phys cell` uses).
pub fn reference(src: &Source, pl: &Placement, times: &mut LoadTimes) -> RefShapes {
    let t0 = Instant::now();
    let before = *times;
    let mut tallies = Vec::new();
    let mut models = src.data.as_ref().map(|data| Models { data, cache: &src.models, times: &mut *times });
    let rc = ref_collision(
        src.d.lo,
        src.d.schema,
        pl,
        models.as_mut().map(|m| m as &mut dyn ModelCollision),
        &mut |what| tallies.push(what),
    );
    let in_models = (times.read - before.read) + (times.parse - before.parse) + (times.convert - before.convert);
    times.records += ms_since(t0) - in_models;
    let prepared = (!rc.objects.is_empty()).then(|| {
        let t = Instant::now();
        let p = PreparedRef::new(&src.params, &RefPose::from_placement(pl), &rc.objects);
        times.shapes += ms_since(t);
        p
    });
    RefShapes { tallies, water: rc.water, prepared }
}
