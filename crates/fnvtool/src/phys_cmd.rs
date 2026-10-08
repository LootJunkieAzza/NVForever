//! `fnvtool phys ...`: the Layer 4 physics world built from the game data (fnv-phys).
//! JSON schemas: docs/spec-notes/physics.md §7.

use crate::esm_cmd::open_load_order;
use crate::{PhysCmd, WorldArgs};
use anyhow::{anyhow, bail, Context};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_collide::CollisionObject;
use fnv_nif::file::NifFile;
use fnv_nif::scene::Decoded;
use fnv_phys::terrain::height_at;
use fnv_phys::{layers, ref_collision, CharacterInput, CharacterParams, Filter, ModelCollision, Owner, PhysParams, PhysWorld, RefPose};
use fnv_world::land::{load_land, LandParts, SIDE, SPACING};
use fnv_world::placed::placement;
use fnv_world::{cell_of, Cell, WorldIndex, CELL_SIZE};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub fn run(cmd: PhysCmd) -> anyhow::Result<Value> {
    match cmd {
        PhysCmd::Cell { w, cell, world, x, y, radius, drops, seconds, nif_schema, no_meshes } => {
            let drops = drops.iter().map(|d| parse_point(d)).collect::<anyhow::Result<Vec<_>>>()?;
            let mut meshes = if no_meshes { None } else { Meshes::open(&w.lo.data_dir, &w.schema, nif_schema.as_deref())? };
            with_world(&w, |lo, schema, ix| {
                let centre = match (cell, world, x, y) {
                    (Some(c), None, None, None) => ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?,
                    (None, Some(wn), Some(x), Some(y)) => {
                        let ws = ix.world_by_name(&wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
                        let id = ws.grid.get(&(x, y)).ok_or_else(|| anyhow!("{wn} has no cell at {x},{y}"))?;
                        &ix.cells[id]
                    }
                    _ => bail!("give --cell, or --world with --x and --y"),
                };
                cell_cmd(lo, schema, ix, centre, radius.max(0), (&drops, seconds), meshes.as_mut())
            })
        }
        PhysCmd::Walk { w, script, cell, world, radius, every, nif_schema, no_meshes } => {
            let text = std::fs::read_to_string(&script).with_context(|| script.display().to_string())?;
            let script = Script::parse(&serde_json::from_str(&text).with_context(|| script.display().to_string())?)?;
            let mut meshes = if no_meshes { None } else { Meshes::open(&w.lo.data_dir, &w.schema, nif_schema.as_deref())? };
            with_world(&w, |lo, schema, ix| {
                let centre = match (cell, world) {
                    (Some(c), None) => ix.cell_by_name(&c).ok_or_else(|| anyhow!("no cell {c}"))?,
                    (None, Some(wn)) => {
                        let ws = ix.world_by_name(&wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
                        let g = cell_of(script.start[0], script.start[1]);
                        let id = ws.grid.get(&g).ok_or_else(|| anyhow!("{wn} has no cell at {},{}", g.0, g.1))?;
                        &ix.cells[id]
                    }
                    _ => bail!("give --cell or --world"),
                };
                walk_cmd(lo, schema, ix, centre, radius.max(0), &script, every.max(1), meshes.as_mut())
            })
        }
        PhysCmd::Heights { w, world } => with_world(&w, |lo, schema, ix| heights_cmd(lo, schema, ix, &world)),
        PhysCmd::Terrain { w, world, samples } => with_world(&w, |lo, schema, ix| terrain_cmd(lo, schema, ix, &world, samples)),
    }
}

fn with_world(
    w: &WorldArgs,
    f: impl FnOnce(&LoadOrder, &Schema, &WorldIndex) -> anyhow::Result<Value>,
) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let ix = WorldIndex::build(&lo, &schema);
    let mut v = f(&lo, &schema, &ix)?;
    v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
    Ok(v)
}

/// A probe drop point: x, y and optionally z, world units.
type DropPoint = (f32, f32, Option<f32>);

/// "x,y" or "x,y,z" in world units.
fn parse_point(s: &str) -> anyhow::Result<(f32, f32, Option<f32>)> {
    let v: Vec<f32> =
        s.split(',').map(|t| t.trim().parse::<f32>()).collect::<Result<_, _>>().with_context(|| format!("bad point {s}"))?;
    match v[..] {
        [x, y] => Ok((x, y, None)),
        [x, y, z] => Ok((x, y, Some(z))),
        _ => bail!("a point is x,y or x,y,z: {s}"),
    }
}

fn fid(v: u32) -> Value {
    json!(formid_str(v))
}

fn owner_json(o: &Owner) -> Value {
    match *o {
        Owner::Terrain { x, y } => json!({"kind": "terrain", "grid": [x, y]}),
        Owner::Ref { form_id, object, layer } => json!({
            "kind": "ref",
            "formid": fid(form_id),
            "object": object,
            "layer": layer,
            "layer_name": layers::name(layer as u32),
        }),
        Owner::Character { id } => json!({"kind": "character", "id": id}),
    }
}

/// Collision objects of models, read through workstream 1's converter
/// (`fnv_collide::nif::collision_objects`) from the game's archives and loose files.
/// fnv-phys itself never reads NIF blocks (docs/design/layer4.md §7).
pub(crate) struct Meshes {
    vfs: fnv_vfs::Vfs,
    schema: fnv_nif::Schema,
    cache: BTreeMap<String, Result<Arc<Vec<CollisionObject>>, String>>,
}

impl Meshes {
    /// `None` when no NIF schema is given and none sits next to the ESM schema.
    pub(crate) fn open(data_dir: &Path, esm_schema: &Path, nif_schema: Option<&Path>) -> anyhow::Result<Option<Meshes>> {
        let dir: PathBuf = match nif_schema {
            Some(d) => d.to_path_buf(),
            None => match esm_schema.parent().map(|p| p.join("nif")).filter(|d| d.is_dir()) {
                Some(d) => d,
                None => return Ok(None),
            },
        };
        let schema = fnv_nif::Schema::load_dir(&dir).with_context(|| dir.display().to_string())?;
        let vfs = fnv_vfs::Vfs::open_default(data_dir).with_context(|| data_dir.display().to_string())?;
        Ok(Some(Meshes { vfs, schema, cache: BTreeMap::new() }))
    }
}

impl ModelCollision for Meshes {
    /// The solid collision objects of a model (empty when it has none).
    fn objects(&mut self, path: &str) -> Result<Arc<Vec<CollisionObject>>, String> {
        if let Some(r) = self.cache.get(path) {
            return r.clone();
        }
        let r = match self.vfs.read(path) {
            None => Err("not found".to_string()),
            Some(Err(e)) => Err(e.to_string()),
            Some(Ok(data)) => NifFile::parse(&data, Some(&self.schema)).map_err(|e| e.to_string()).map(|file| {
                let dec = Decoded::new(&file, &data, &self.schema);
                Arc::new(fnv_collide::nif::collision_objects(&file, &dec).0)
            }),
        };
        self.cache.insert(path.to_string(), r.clone());
        r
    }
}

/// The physics world of a cell and its surroundings, with what went into it.
pub(crate) struct Built<'a> {
    pub(crate) world: PhysWorld,
    cells: Vec<&'a Cell>,
    grids: BTreeMap<(i32, i32), Option<Vec<f32>>>,
    no_heights: usize,
    counts: BTreeMap<&'static str, usize>,
    by_base: BTreeMap<String, usize>,
    collision_source: Option<Value>,
    /// Lowest and highest z of the loaded terrain and collision references' placements.
    z_range: (f32, f32),
    /// Bounds (min x, min y, max x, max y) of the collision references' placements.
    xy_bounds: [f32; 4],
}

impl Built<'_> {
    /// Where to drop a probe by default: an exterior cell's centre, else the middle of
    /// the placements that have collision.
    fn default_drop(&self, centre: &Cell) -> (f32, f32) {
        match centre.grid {
            Some((gx, gy)) => ((gx as f32 + 0.5) * CELL_SIZE, (gy as f32 + 0.5) * CELL_SIZE),
            None if self.xy_bounds[0] <= self.xy_bounds[2] => {
                ((self.xy_bounds[0] + self.xy_bounds[2]) / 2.0, (self.xy_bounds[1] + self.xy_bounds[3]) / 2.0)
            }
            None => (0.0, 0.0),
        }
    }

    /// Range for vertical probe rays: the loaded content with 1,000 units to spare.
    fn ray_span(&self) -> (f32, f32) {
        (self.z_range.1 + 1000.0, self.z_range.0 - 1000.0)
    }

    fn json(&self, schema: &str, centre: &Cell, radius: i32, character: &CharacterParams) -> anyhow::Result<Value> {
        Ok(json!({
            "schema": schema,
            "cell": fid(centre.form_id),
            "edid": centre.edid,
            "world": centre.world.map(fid),
            "grid": centre.grid.map(|(x, y)| json!([x, y])),
            "radius": radius,
            "params": serde_json::to_value(self.world.params)?,
            "character": serde_json::to_value(character)?,
            "cells": self.cells.iter().map(|c| json!({
                "cell": fid(c.form_id),
                "grid": c.grid.map(|(x, y)| json!([x, y])),
                "land": c.grid.and_then(|g| self.grids.get(&g)).is_some_and(|h| h.is_some()),
                "refs": c.refs.len(),
            })).collect::<Vec<_>>(),
            "terrain": {"cells": self.grids.len() - self.no_heights, "no_heights": self.no_heights},
            "refs": {
                "counts": self.counts,
                "by_base_type": self.by_base,
                "collision_source": self.collision_source,
            },
        }))
    }
}

/// Whether a placed reference is in the world: `Ok`, or the `refs.counts` name of why
/// not (e.g. `"initially_disabled"`).
pub(crate) type InWorld<'f> = &'f dyn Fn(&fnv_world::placed::Placement) -> Result<(), &'static str>;

/// The plugins' own rule (a game just started): not deleted, not initially disabled, and
/// enabled through its enable parent.
pub(crate) fn plugin_in_world<'a>(
    lo: &'a LoadOrder,
    schema: &'a Schema,
) -> impl Fn(&fnv_world::placed::Placement) -> Result<(), &'static str> + 'a {
    move |pl| {
        if pl.deleted() {
            Err("deleted")
        } else if pl.initially_disabled() {
            Err("initially_disabled")
        } else if !parent_allows(lo, schema, pl, 0) {
            Err("disabled_by_parent")
        } else {
            Ok(())
        }
    }
}

/// Build the static world of `centre` (and, for an exterior cell, the exterior cells
/// within `radius` grid squares), with the references `in_world` admits.
#[allow(clippy::too_many_arguments)]
pub(crate) fn build<'a>(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &'a WorldIndex,
    centre: &'a Cell,
    radius: i32,
    params: PhysParams,
    mut meshes: Option<&mut Meshes>,
    in_world: InWorld,
) -> Built<'a> {
    let mut world = PhysWorld::new(params);
    let mut cells: Vec<&Cell> = vec![centre];
    let mut grids: BTreeMap<(i32, i32), Option<Vec<f32>>> = BTreeMap::new();
    let ws = centre.world.and_then(|w| ix.worldspaces.get(&w));
    if let (Some(ws), Some((gx, gy))) = (ws, centre.grid) {
        cells.clear();
        for y in gy - radius..=gy + radius {
            for x in gx - radius..=gx + radius {
                let Some(id) = ws.grid.get(&(x, y)) else { continue };
                let c = &ix.cells[id];
                cells.push(c);
                let h = c.land.and_then(|l| load_land(lo, schema, l, LandParts::HEIGHTS)).and_then(|l| l.heights);
                grids.insert((x, y), h);
            }
        }
    }
    let mut no_heights = 0;
    let mut z_range = (f32::INFINITY, f32::NEG_INFINITY);
    let mut xy_bounds = [f32::INFINITY, f32::INFINITY, f32::NEG_INFINITY, f32::NEG_INFINITY];
    for (&g, h) in &grids {
        match h {
            Some(h) => {
                world.add_terrain(g, h);
                for &z in h {
                    z_range = (z_range.0.min(z), z_range.1.max(z));
                }
            }
            None => no_heights += 1,
        }
    }

    // References: those of the loaded cells, plus the worldspace's persistent references
    // standing in a loaded grid square.
    let mut ref_ids: Vec<u32> = cells.iter().flat_map(|c| c.refs.iter().copied()).collect();
    let persistent = ws.and_then(|w| w.persistent_cell).and_then(|p| ix.cells.get(&p));
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    let mut by_base: BTreeMap<String, usize> = BTreeMap::new();
    if let Some(p) = persistent.filter(|_| !grids.is_empty()) {
        for &r in &p.refs {
            if let Some(pl) = placement(lo, schema, r) {
                if grids.contains_key(&cell_of(pl.position[0], pl.position[1])) {
                    ref_ids.push(r);
                }
            }
        }
    }
    ref_ids.sort_unstable();
    ref_ids.dedup();
    for &r in &ref_ids {
        *counts.entry("placed").or_default() += 1;
        let Some(pl) = placement(lo, schema, r) else {
            *counts.entry("unreadable").or_default() += 1;
            continue;
        };
        if let Err(why) = in_world(&pl) {
            *counts.entry(why).or_default() += 1;
            continue;
        }
        let models = meshes.as_deref_mut().map(|m| m as &mut dyn ModelCollision);
        let rc = ref_collision(lo, schema, &pl, models, &mut |what| *counts.entry(what).or_default() += 1);
        if let Some(sig) = rc.base_sig {
            *by_base.entry(String::from_utf8_lossy(&sig).into_owned()).or_default() += 1;
        }
        let objects = rc.objects;
        if objects.is_empty() {
            continue;
        }
        let rep = world.add_ref(&RefPose::from_placement(&pl), &objects);
        z_range = (z_range.0.min(pl.position[2]), z_range.1.max(pl.position[2]));
        let [x, y, _] = pl.position;
        xy_bounds = [xy_bounds[0].min(x), xy_bounds[1].min(y), xy_bounds[2].max(x), xy_bounds[3].max(y)];
        *counts.entry("with_collision").or_default() += 1;
        *counts.entry("colliders").or_default() += rep.colliders;
        if !rep.errors.is_empty() {
            *counts.entry("object_errors").or_default() += rep.errors.len();
        }
    }
    let collision_source = meshes.as_deref().map(|ms| {
        let unreadable: Vec<Value> =
            ms.cache.iter().filter_map(|(p, r)| r.as_ref().err().map(|e| json!({"model": p, "error": e}))).take(20).collect();
        json!({
            "kind": "fnv_collide::nif",
            "models": ms.cache.len(),
            "models_with_collision": ms.cache.values().filter(|r| r.as_ref().is_ok_and(|o| !o.is_empty())).count(),
            "models_unreadable": ms.cache.values().filter(|r| r.is_err()).count(),
            "unreadable_examples": unreadable,
        })
    });

    if !z_range.0.is_finite() {
        z_range = (0.0, 0.0);
    }
    Built { world, cells, grids, no_heights, counts, by_base, collision_source, z_range, xy_bounds }
}

/// Build the world around `centre`, then drop a probe character at each drop point.
/// Whether a reference's enable parent leaves it enabled at the start of a game, by the
/// renderer's rule (`engine/nvforever/src/world.rs`): the parent must itself be enabled
/// (not deleted or initially disabled, its own parent followed), inverted for "opposite";
/// a missing parent counts as enabled unless "opposite", a chain deeper than 16 as
/// disabled. Disabled references have no collision [observed by the owner in game,
/// physics.md §4].
fn parent_allows(lo: &LoadOrder, schema: &Schema, p: &fnv_world::placed::Placement, depth: usize) -> bool {
    match p.enable_parent {
        None => true,
        Some(_) if depth > 16 => false,
        Some((parent, opposite)) => match placement(lo, schema, parent) {
            Some(pp) => (!pp.deleted() && !pp.initially_disabled() && parent_allows(lo, schema, &pp, depth + 1)) != opposite,
            None => !opposite,
        },
    }
}

fn cell_cmd(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &WorldIndex,
    centre: &Cell,
    radius: i32,
    (drops, seconds): (&[DropPoint], f32),
    meshes: Option<&mut Meshes>,
) -> anyhow::Result<Value> {
    let mut b = build(lo, schema, ix, centre, radius, PhysParams::default(), meshes, &plugin_in_world(lo, schema));
    let character = CharacterParams::default();
    let probes: Vec<Value> = if drops.is_empty() {
        let (cx, cy) = b.default_drop(centre);
        vec![probe(&mut b, character, (cx, cy, None), seconds)]
    } else {
        drops.iter().map(|&d| probe(&mut b, character, d, seconds)).collect()
    };
    let mut v = b.json("fnv.phys.cell/1", centre, radius, &character)?;
    v["world_stats"] = serde_json::to_value(b.world.stats())?;
    v["probes"] = json!(probes);
    Ok(v)
}

/// Drop a character at (x, y): from `z` if given, else 100 units above whatever a
/// downward ray finds (or z = 0 when it finds nothing); simulate `seconds` with no input.
fn probe(b: &mut Built, params: CharacterParams, (x, y, z): (f32, f32, Option<f32>), seconds: f32) -> Value {
    let (top, bottom) = b.ray_span();
    let (world, grids) = (&mut b.world, &b.grids);
    let from = z.map_or(top, |z| z.max(top));
    let ground = world.queries().ray([x, y, from], [0.0, 0.0, -1.0], from - bottom, &Filter::default());
    let start = [x, y, z.unwrap_or_else(|| ground.map_or(0.0, |g| g.point[2]) + 100.0)];
    let terrain_z = grids
        .get(&cell_of(x, y))
        .and_then(|h| h.as_deref())
        .and_then(|h| {
            let (gx, gy) = cell_of(x, y);
            height_at(h, x - gx as f32 * CELL_SIZE, y - gy as f32 * CELL_SIZE)
        });
    let mut c = world.add_character(0, params, start);
    let steps = (seconds / world.params.dt).round().max(1.0) as usize;
    let mut landed: Option<(f32, f32)> = None;
    let mut last = None;
    for i in 0..steps {
        let s = world.move_character(&mut c, CharacterInput::default());
        world.step();
        if landed.is_none() {
            if let Some(v) = s.landed_speed {
                landed = Some(((i + 1) as f32 * world.params.dt, v));
            }
        }
        last = Some(s);
    }
    world.remove_character(c);
    let last = last.expect("at least one step");
    json!({
        "start": start,
        "ground_ray": ground.map(|g| json!({"owner": owner_json(&g.owner), "z": g.point[2], "normal": g.normal})),
        "terrain_z": terrain_z,
        "landed_after_s": landed.map(|l| l.0),
        "landing_speed": landed.map(|l| l.1),
        "final_feet": last.feet,
        "grounded": last.grounded,
        "sliding": last.sliding,
        "rest_above_terrain": terrain_z.map(|t| last.feet[2] - t),
        "touched": last.touched.iter().map(owner_json).collect::<Vec<_>>(),
    })
}

/// A walk script (`fnv.phys.walk-script/1`): where to start, the step rate and input
/// segments, and optional parameter overrides.
struct Script {
    /// x, y and optionally z (feet); without z the walk starts on what a downward ray
    /// finds.
    start: [f32; 2],
    start_z: Option<f32>,
    hz: u32,
    segments: Vec<Segment>,
    params: BTreeMap<String, f64>,
}

struct Segment {
    steps: usize,
    velocity: [f32; 2],
    jump: bool,
}

impl Script {
    fn parse(v: &Value) -> anyhow::Result<Script> {
        if v["schema"] != json!("fnv.phys.walk-script/1") {
            bail!("a walk script needs \"schema\": \"fnv.phys.walk-script/1\"");
        }
        let num = |x: &Value, what: &str| -> anyhow::Result<f32> {
            x.as_f64().map(|v| v as f32).filter(|v| v.is_finite()).ok_or_else(|| anyhow!("{what} must be a finite number"))
        };
        let start = v["start"].as_array().filter(|a| a.len() == 2 || a.len() == 3).ok_or_else(|| anyhow!("\"start\" must be [x, y] or [x, y, z]"))?;
        let hz = v.get("hz").map_or(Ok(60), |h| h.as_u64().filter(|h| (1..=1000).contains(h)).ok_or_else(|| anyhow!("\"hz\" must be 1..=1000")))? as u32;
        let mut segments = Vec::new();
        for s in v["segments"].as_array().ok_or_else(|| anyhow!("\"segments\" must be a list"))? {
            let steps = s["steps"].as_u64().ok_or_else(|| anyhow!("a segment needs \"steps\""))? as usize;
            let velocity = match s.get("velocity") {
                None => [0.0, 0.0],
                Some(w) => [num(&w[0], "velocity[0]")?, num(&w[1], "velocity[1]")?],
            };
            segments.push(Segment { steps, velocity, jump: s.get("jump").and_then(Value::as_bool).unwrap_or(false) });
        }
        let total: usize = segments.iter().map(|s| s.steps).sum();
        if total > 1_000_000 {
            bail!("script too long: {total} steps");
        }
        let mut params = BTreeMap::new();
        if let Some(p) = v.get("params").and_then(Value::as_object) {
            for (k, x) in p {
                params.insert(k.clone(), x.as_f64().filter(|v| v.is_finite()).ok_or_else(|| anyhow!("param {k} must be a number"))?);
            }
        }
        Ok(Script {
            start: [num(&start[0], "start[0]")?, num(&start[1], "start[1]")?],
            start_z: start.get(2).map(|z| num(z, "start[2]")).transpose()?,
            hz,
            segments,
            params,
        })
    }

    /// World and character parameters: defaults with the script's overrides.
    fn parameters(&self) -> anyhow::Result<(PhysParams, CharacterParams)> {
        let mut w = PhysParams { dt: 1.0 / self.hz as f32, ..Default::default() };
        let mut c = CharacterParams::default();
        for (k, &v) in &self.params {
            let f = v as f32;
            match k.as_str() {
                "gravity" => w.gravity = f,
                "radius" => c.radius = f,
                "height" => c.height = f,
                "step_height" => c.step_height = f,
                "max_slope_deg" => c.max_slope_deg = f,
                "jump_height" => c.jump_height = f,
                "skin" => c.skin = f,
                "max_fall_speed" => c.max_fall_speed = Some(f),
                "push_mass" => c.push_mass = f,
                _ => bail!("unknown parameter {k}"),
            }
        }
        Ok((w, c))
    }
}

/// Run a walk script in the world built around its start; every `every`th step is traced.
#[allow(clippy::too_many_arguments)]
fn walk_cmd(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &WorldIndex,
    centre: &Cell,
    radius: i32,
    script: &Script,
    every: usize,
    meshes: Option<&mut Meshes>,
) -> anyhow::Result<Value> {
    let (params, character) = script.parameters()?;
    let mut b = build(lo, schema, ix, centre, radius, params, meshes, &plugin_in_world(lo, schema));
    let [x, y] = script.start;
    let start_z = match script.start_z {
        Some(z) => z,
        None => {
            let (top, bottom) = b.ray_span();
            let hit = b.world.queries().ray([x, y, top], [0.0, 0.0, -1.0], top - bottom, &Filter::default());
            hit.ok_or_else(|| anyhow!("nothing below the start {x},{y}"))?.point[2] + 1.0
        }
    };
    let world = &mut b.world;
    let mut c = world.add_character(0, character, [x, y, start_z]);
    let total: usize = script.segments.iter().map(|s| s.steps).sum();
    let (mut step, mut path, mut prev) = (0usize, 0.0f64, c.feet);
    let (mut min_z, mut max_z) = (c.feet[2], c.feet[2]);
    let (mut jumps, mut landings, mut airborne) = (0usize, 0usize, 0usize);
    let mut touched: BTreeMap<Owner, usize> = BTreeMap::new();
    let mut trace = Vec::new();
    for seg in &script.segments {
        for i in 0..seg.steps {
            let jump = seg.jump && i == 0;
            let was_grounded = c.grounded;
            let s = world.move_character(&mut c, CharacterInput { velocity: seg.velocity, jump });
            world.step();
            step += 1;
            jumps += (jump && was_grounded) as usize;
            landings += s.landed_speed.is_some() as usize;
            airborne += !s.grounded as usize;
            path += ((s.feet[0] - prev[0]) as f64).hypot((s.feet[1] - prev[1]) as f64);
            prev = s.feet;
            min_z = min_z.min(s.feet[2]);
            max_z = max_z.max(s.feet[2]);
            for o in &s.touched {
                *touched.entry(*o).or_default() += 1;
            }
            if step.is_multiple_of(every) || step == total {
                trace.push(json!({
                    "step": step,
                    "feet": s.feet,
                    "vertical_speed": c.vertical_speed,
                    "grounded": s.grounded,
                    "sliding": s.sliding,
                    "landed_speed": s.landed_speed,
                    "touched": s.touched.iter().map(owner_json).collect::<Vec<_>>(),
                }));
            }
        }
    }
    let mut v = b.json("fnv.phys.walk/1", centre, radius, &character)?;
    v["start"] = json!([x, y, start_z]);
    v["summary"] = json!({
        "steps": step,
        "seconds": step as f64 / script.hz as f64,
        "end": c.feet,
        "horizontal_path": path,
        "min_z": min_z,
        "max_z": max_z,
        "jumps": jumps,
        "landings": landings,
        "airborne_steps": airborne,
        "touched": touched.iter().map(|(o, n)| json!({"owner": owner_json(o), "steps": n})).collect::<Vec<_>>(),
    });
    v["trace"] = json!(trace);
    Ok(v)
}

/// Sample points of `phys heights`, in grid steps (128 units) east and north of a cell's
/// south-west corner: four interior vertices, then four points inside grid squares away
/// from both diagonals (physics.md §7).
pub const HEIGHT_POINTS: [[f32; 2]; 8] =
    [[5.0, 9.0], [27.0, 3.0], [13.0, 30.0], [30.0, 22.0], [8.2, 8.6], [24.6, 7.3], [7.7, 23.2], [23.9, 24.4]];

/// Every landscape cell of a worldspace: the physics terrain's height under
/// [`HEIGHT_POINTS`] (vertical rays, each cell in a world of its own).
fn heights_cmd(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, wn: &str) -> anyhow::Result<Value> {
    let ws = ix.world_by_name(wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
    let cells: Vec<((i32, i32), u32)> = ws.grid.iter().filter_map(|(&g, c)| Some((g, ix.cells[c].land?))).collect();
    let rows: Vec<Value> = cells
        .par_iter()
        .map(|&(grid, lf)| {
            let Some(h) = load_land(lo, schema, lf, LandParts::HEIGHTS).and_then(|l| l.heights) else {
                return json!({"grid": [grid.0, grid.1], "heights": Value::Null});
            };
            let mut world = PhysWorld::new(PhysParams::default());
            world.add_terrain(grid, &h);
            let (lo_h, hi_h) = h.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
            let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
            let q = world.queries();
            let heights: Vec<Value> = HEIGHT_POINTS
                .iter()
                .map(|&[u, v]| {
                    let from = hi_h + 100.0;
                    let at = [x0 + u * SPACING, y0 + v * SPACING, from];
                    q.ray(at, [0.0, 0.0, -1.0], from - lo_h + 200.0, &Filter::default()).map_or(Value::Null, |hit| json!(hit.point[2]))
                })
                .collect();
            json!({"grid": [grid.0, grid.1], "heights": heights})
        })
        .collect();
    Ok(json!({
        "schema": "fnv.phys.heights/1",
        "world": fid(ws.form_id),
        "edid": ws.edid,
        "points": HEIGHT_POINTS,
        "cells": rows,
    }))
}

/// Deterministic sample points in a cell: `n` scattered points (a fixed low-discrepancy
/// sequence), plus vertices and grid-line midpoints along the diagonal.
fn samples(n: usize) -> Vec<(f32, f32)> {
    let mut out: Vec<(f32, f32)> = (0..n)
        .map(|i| {
            // Additive recurrence with the plastic number (R2 sequence).
            let (a1, a2) = (0.754_877_7_f64, 0.569_840_3_f64);
            let u = (0.5 + a1 * (i as f64 + 1.0)).fract() as f32;
            let v = (0.5 + a2 * (i as f64 + 1.0)).fract() as f32;
            (u * CELL_SIZE, v * CELL_SIZE)
        })
        .collect();
    for k in 1..SIDE - 1 {
        let p = k as f32 * SPACING;
        out.push((p, p)); // vertex
        out.push((p + SPACING / 2.0, p)); // on a grid line
    }
    out
}

/// Every landscape cell of a worldspace: vertical rays against the terrain collider
/// compared with the analytic surface of the same LAND heights.
fn terrain_cmd(lo: &LoadOrder, schema: &Schema, ix: &WorldIndex, wn: &str, n: usize) -> anyhow::Result<Value> {
    let ws = ix.world_by_name(wn).ok_or_else(|| anyhow!("no worldspace {wn}"))?;
    let cells: Vec<((i32, i32), u32)> = ws.grid.iter().filter_map(|(&g, c)| Some((g, ix.cells[c].land?))).collect();
    let pts = samples(n);
    struct CellResult {
        grid: (i32, i32),
        no_heights: bool,
        rays: usize,
        misses: usize,
        wrong_owner: usize,
        sum_err: f64,
        worst: (f32, f32, f32, f32, f32),
    }
    let results: Vec<CellResult> = cells
        .par_iter()
        .map(|&(grid, lf)| {
            let mut r = CellResult { grid, no_heights: false, rays: 0, misses: 0, wrong_owner: 0, sum_err: 0.0, worst: (0.0, 0.0, 0.0, 0.0, -1.0) };
            let Some(h) = load_land(lo, schema, lf, LandParts::HEIGHTS).and_then(|l| l.heights) else {
                r.no_heights = true;
                return r;
            };
            let mut world = PhysWorld::new(PhysParams::default());
            world.add_terrain(grid, &h);
            let (lo_h, hi_h) = h.iter().fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), &v| (a.min(v), b.max(v)));
            let (x0, y0) = (grid.0 as f32 * CELL_SIZE, grid.1 as f32 * CELL_SIZE);
            let q = world.queries();
            for &(x, y) in &pts {
                let want = height_at(&h, x, y).expect("sample inside the cell");
                r.rays += 1;
                let from = hi_h + 100.0;
                match q.ray([x0 + x, y0 + y, from], [0.0, 0.0, -1.0], from - lo_h + 200.0, &Filter::default()) {
                    None => r.misses += 1,
                    Some(hit) => {
                        if hit.owner != (Owner::Terrain { x: grid.0, y: grid.1 }) {
                            r.wrong_owner += 1;
                        }
                        let e = (hit.point[2] - want).abs();
                        r.sum_err += e as f64;
                        if e > r.worst.4 {
                            r.worst = (x0 + x, y0 + y, hit.point[2], want, e);
                        }
                    }
                }
            }
            r
        })
        .collect();
    let rays: usize = results.iter().map(|r| r.rays).sum();
    let misses: usize = results.iter().map(|r| r.misses).sum();
    let hits = rays - misses;
    let worst = results.iter().filter(|r| r.worst.4 >= 0.0).max_by(|a, b| a.worst.4.total_cmp(&b.worst.4));
    Ok(json!({
        "schema": "fnv.phys.terrain/1",
        "world": fid(ws.form_id),
        "edid": ws.edid,
        "cells": results.iter().filter(|r| !r.no_heights).count(),
        "land_without_heights": results.iter().filter(|r| r.no_heights).map(|r| json!([r.grid.0, r.grid.1])).collect::<Vec<_>>(),
        "samples_per_cell": pts.len(),
        "rays": rays,
        "misses": misses,
        "wrong_owner": results.iter().map(|r| r.wrong_owner).sum::<usize>(),
        "mean_abs_error": if hits > 0 { results.iter().map(|r| r.sum_err).sum::<f64>() / hits as f64 } else { 0.0 },
        "max_abs_error": worst.map(|w| w.worst.4),
        "worst": worst.map(|w| json!({
            "grid": [w.grid.0, w.grid.1],
            "point": [w.worst.0, w.worst.1],
            "physics_z": w.worst.2,
            "analytic_z": w.worst.3,
        })),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_parse() {
        assert_eq!(parse_point("1,-2").unwrap(), (1.0, -2.0, None));
        assert_eq!(parse_point(" 1.5, 2 ,3").unwrap(), (1.5, 2.0, Some(3.0)));
        assert!(parse_point("1").is_err() && parse_point("a,b").is_err() && parse_point("1,2,3,4").is_err());
    }

    #[test]
    fn walk_scripts_parse() {
        let v = json!({
            "schema": "fnv.phys.walk-script/1",
            "start": [10.0, -20.0],
            "segments": [{"steps": 5, "velocity": [100.0, 0.0]}, {"steps": 1, "jump": true}],
            "params": {"step_height": 30.0, "gravity": 700.0},
        });
        let s = Script::parse(&v).unwrap();
        assert_eq!((s.start, s.start_z, s.hz), ([10.0, -20.0], None, 60));
        assert_eq!(s.segments.len(), 2);
        assert!(s.segments[1].jump && s.segments[1].velocity == [0.0, 0.0]);
        let (w, c) = s.parameters().unwrap();
        assert_eq!((w.gravity, c.step_height), (700.0, 30.0));
        assert!(Script::parse(&json!({"start": [0, 0], "segments": []})).is_err());
        let bad = json!({"schema": "fnv.phys.walk-script/1", "start": [0, 0], "segments": [], "params": {"nope": 1}});
        assert!(Script::parse(&bad).unwrap().parameters().is_err());
    }

    #[test]
    fn height_points_avoid_triangle_edges() {
        for (i, [u, v]) in HEIGHT_POINTS.iter().enumerate() {
            assert!(*u > 0.0 && *v > 0.0 && *u < 32.0 && *v < 32.0);
            let (fu, fv) = (u.fract(), v.fract());
            if i < 4 {
                assert_eq!((fu, fv), (0.0, 0.0), "vertex");
            } else {
                assert!((fu - fv).abs() > 0.05 && (fu + fv - 1.0).abs() > 0.05, "{u},{v} near a diagonal");
            }
        }
    }

    #[test]
    fn samples_stay_inside_the_cell() {
        let s = samples(64);
        assert_eq!(s.len(), 64 + 2 * (SIDE - 2));
        assert!(s.iter().all(|&(x, y)| (0.0..=CELL_SIZE).contains(&x) && (0.0..=CELL_SIZE).contains(&y)));
    }
}
