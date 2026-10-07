//! `fnvtool phys ...`: the Layer 4 physics world built from the game data (fnv-phys).
//! JSON schemas: docs/spec-notes/physics.md §7.

use crate::esm_cmd::open_load_order;
use crate::{PhysCmd, WorldArgs};
use anyhow::{anyhow, bail, Context};
use fnv_esm::decode::formid_str;
use fnv_esm::{LoadOrder, Schema};
use fnv_phys::terrain::height_at;
use fnv_phys::{CharacterInput, CharacterParams, Filter, Owner, PhysParams, PhysWorld};
use fnv_world::land::{load_land, LandParts, SIDE, SPACING};
use fnv_world::placed::{base_object, placement};
use fnv_world::{cell_of, Cell, WorldIndex, CELL_SIZE};
use rayon::prelude::*;
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub fn run(cmd: PhysCmd) -> anyhow::Result<Value> {
    match cmd {
        PhysCmd::Cell { w, cell, world, x, y, radius, drops, seconds } => {
            let drops = drops.iter().map(|d| parse_point(d)).collect::<anyhow::Result<Vec<_>>>()?;
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
                cell_cmd(lo, schema, ix, centre, radius.max(0), &drops, seconds)
            })
        }
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
        Owner::Ref { form_id, object, layer } => json!({"kind": "ref", "formid": fid(form_id), "object": object, "layer": layer}),
        Owner::Character { id } => json!({"kind": "character", "id": id}),
    }
}

/// Build the static world of `centre` (and, for an exterior cell, the exterior cells
/// within `radius` grid squares), then drop a probe character at each drop point.
fn cell_cmd(
    lo: &LoadOrder,
    schema: &Schema,
    ix: &WorldIndex,
    centre: &Cell,
    radius: i32,
    drops: &[(f32, f32, Option<f32>)],
    seconds: f32,
) -> anyhow::Result<Value> {
    let params = PhysParams::default();
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
    for (&g, h) in &grids {
        match h {
            Some(h) => world.add_terrain(g, h),
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
        if pl.deleted() {
            *counts.entry("deleted").or_default() += 1;
            continue;
        }
        if pl.initially_disabled() {
            *counts.entry("initially_disabled").or_default() += 1;
            continue;
        }
        let Some(base) = pl.base.and_then(|b| base_object(lo, schema, b)) else {
            *counts.entry("no_base").or_default() += 1;
            continue;
        };
        *by_base.entry(String::from_utf8_lossy(&base.sig).into_owned()).or_default() += 1;
        if base.model_path().is_none() {
            *counts.entry("no_model").or_default() += 1;
            continue;
        }
        // Collision objects come from fnv-collide's conversion of the model's Havok
        // blocks (workstream 1), which is not delivered yet: such references are counted
        // here and added with `PhysWorld::add_ref` once it is (physics.md §4).
        *counts.entry("awaiting_collision").or_default() += 1;
    }

    let character = CharacterParams::default();
    let probes: Vec<Value> = if drops.is_empty() {
        let (cx, cy) = match centre.grid {
            Some((gx, gy)) => ((gx as f32 + 0.5) * CELL_SIZE, (gy as f32 + 0.5) * CELL_SIZE),
            None => (0.0, 0.0),
        };
        vec![probe(&mut world, &grids, character, (cx, cy, None), seconds)]
    } else {
        drops.iter().map(|&d| probe(&mut world, &grids, character, d, seconds)).collect()
    };

    Ok(json!({
        "schema": "fnv.phys.cell/1",
        "cell": fid(centre.form_id),
        "edid": centre.edid,
        "world": centre.world.map(fid),
        "grid": centre.grid.map(|(x, y)| json!([x, y])),
        "radius": radius,
        "params": serde_json::to_value(params)?,
        "character": serde_json::to_value(character)?,
        "cells": cells.iter().map(|c| json!({
            "cell": fid(c.form_id),
            "grid": c.grid.map(|(x, y)| json!([x, y])),
            "land": c.grid.and_then(|g| grids.get(&g)).is_some_and(|h| h.is_some()),
            "refs": c.refs.len(),
        })).collect::<Vec<_>>(),
        "terrain": {"cells": grids.len() - no_heights, "no_heights": no_heights},
        "refs": {
            "counts": counts,
            "by_base_type": by_base,
            "collision_source": Value::Null,
        },
        "world_stats": serde_json::to_value(world.stats())?,
        "probes": probes,
    }))
}

/// Drop a character at (x, y): from `z` if given, else 100 units above whatever a
/// downward ray finds (or z = 0 when it finds nothing); simulate `seconds` with no input.
fn probe(
    world: &mut PhysWorld,
    grids: &BTreeMap<(i32, i32), Option<Vec<f32>>>,
    params: CharacterParams,
    (x, y, z): (f32, f32, Option<f32>),
    seconds: f32,
) -> Value {
    let top = grids.values().flatten().flatten().copied().fold(0.0f32, f32::max) + 1000.0;
    let bottom = grids.values().flatten().flatten().copied().fold(0.0f32, f32::min) - 1000.0;
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
    fn samples_stay_inside_the_cell() {
        let s = samples(64);
        assert_eq!(s.len(), 64 + 2 * (SIDE - 2));
        assert!(s.iter().all(|&(x, y)| (0.0..=CELL_SIZE).contains(&x) && (0.0..=CELL_SIZE).contains(&y)));
    }
}
