//! `fnvtool nav ...`: navmeshes (fnv-nav). JSON schemas: docs/spec-notes/navmesh.md §6.

use crate::esm_cmd::open_load_order;
use crate::WorldArgs;
use anyhow::Context;
use clap::Subcommand;
use fnv_esm::{LoadOrder, Schema};
use fnv_nav::record::{link_kind, tri_flags};
use fnv_nav::NavRecords;
use fnv_world::WorldIndex;
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};

#[derive(Subcommand)]
pub enum NavCmd {
    /// What the load order's navmesh records hold, with consistency checks on their
    /// indices, links and doors (fnv.nav.survey/1).
    Survey {
        #[command(flatten)]
        w: WorldArgs,
    },
    /// Build the nav graph of the load order: what resolved, how long it took, and the
    /// connected areas of each space (fnv.nav.graph/1).
    Graph {
        #[command(flatten)]
        w: WorldArgs,
        /// Only report this space (worldspace or interior cell, Editor ID or FormID).
        #[arg(long)]
        space: Option<String>,
    },
    /// The navmesh triangles near a point: corners, flags, what each edge leads to, and
    /// the doors of each (fnv.nav.near/1).
    Near {
        #[command(flatten)]
        w: WorldArgs,
        /// A placed reference (Editor ID or FormID) or "x,y,z" in --space.
        #[arg(long, allow_hyphen_values = true)]
        at: String,
        #[arg(long, default_value = "WastelandNV")]
        space: String,
        /// Horizontal distance, game units.
        #[arg(long, default_value_t = 128.0)]
        radius: f32,
    },
    /// Compare NPC movement recorded in the original game (AI observer captures,
    /// docs/spec-notes/ai-observer.md) with the navmesh: how far recorded positions lie
    /// from it, and how far each recorded walk strays from the path planned between its
    /// ends (fnv.nav.observed/1). Captures stay private; only the summary is reported.
    Observed {
        #[command(flatten)]
        w: WorldArgs,
        /// Observer output files (`.jsonl`).
        #[arg(long, required = true, num_args = 1..)]
        samples: Vec<std::path::PathBuf>,
        /// A walk is a run of moving samples whose ends are at least this far apart.
        #[arg(long, default_value_t = 512.0)]
        min_walk: f32,
    },
    /// A path between two places (fnv.nav.path/1).
    Path {
        #[command(flatten)]
        w: WorldArgs,
        #[command(flatten)]
        ends: crate::nav_path::Ends,
        #[command(flatten)]
        opts: crate::nav_path::PathOpts,
    },
    /// An NPC-sized character walks a path in the physics world (terrain and collision
    /// of the cells along it), steered by the path follower, replanning when it is stuck
    /// (fnv.nav.walk/1). The headless test that navmesh paths are walkable.
    Walk {
        #[command(flatten)]
        w: WorldArgs,
        #[command(flatten)]
        ends: crate::nav_path::Ends,
        #[command(flatten)]
        opts: crate::nav_path::PathOpts,
        #[command(flatten)]
        walk: crate::nav_path::WalkOpts,
    },
}

pub fn run(cmd: NavCmd) -> anyhow::Result<Value> {
    match cmd {
        NavCmd::Survey { w } => with_lo(&w, |lo, schema| Ok(survey(lo, &WorldIndex::build(lo, schema), &NavRecords::read(lo)))),
        NavCmd::Graph { w, space } => with_lo(&w, |lo, schema| crate::nav_path::graph_cmd(lo, schema, space.as_deref())),
        NavCmd::Near { w, at, space, radius } => with_lo(&w, |lo, schema| crate::nav_path::near_cmd(lo, schema, &at, &space, radius)),
        NavCmd::Observed { w, samples, min_walk } => with_lo(&w, |lo, schema| crate::nav_path::observed_cmd(lo, schema, &samples, min_walk)),
        NavCmd::Path { w, ends, opts } => with_lo(&w, |lo, schema| crate::nav_path::path_cmd(lo, schema, &ends, &opts)),
        NavCmd::Walk { w, ends, opts, walk } => {
            let data_dir = w.lo.data_dir.clone();
            let esm_schema = w.schema.clone();
            with_lo(&w, |lo, schema| crate::nav_path::walk_cmd(lo, schema, &ends, &opts, &walk, (&data_dir, &esm_schema)))
        }
    }
}

pub(crate) fn with_lo(w: &WorldArgs, f: impl FnOnce(&LoadOrder, &Schema) -> anyhow::Result<Value>) -> anyhow::Result<Value> {
    let schema = Schema::load_dir(&w.schema).with_context(|| w.schema.display().to_string())?;
    let plugins = open_load_order(&w.lo)?;
    let lo = LoadOrder::new(&plugins)?;
    let mut v = f(&lo, &schema)?;
    v["load_order"] = json!(plugins.iter().map(|p| p.name.clone()).collect::<Vec<_>>());
    Ok(v)
}

pub(crate) fn fid(v: u32) -> String {
    format!("0x{v:08X}")
}

/// Counter of named observations with up to three examples each.
#[derive(Default)]
struct Tally(BTreeMap<String, (u64, Vec<String>)>);

impl Tally {
    fn add(&mut self, what: &str, example: impl FnOnce() -> String) {
        let e = self.0.entry(what.to_string()).or_default();
        e.0 += 1;
        if e.1.len() < 3 {
            e.1.push(example());
        }
    }
    fn json(&self) -> Value {
        json!(self.0.iter().map(|(k, (n, ex))| (k.clone(), json!({"count": n, "examples": ex}))).collect::<serde_json::Map<_, _>>())
    }
}

fn survey(lo: &LoadOrder, ix: &WorldIndex, nav: &NavRecords) -> Value {
    let ms = &nav.navmeshes;
    let mut t = Tally::default();
    let mut versions: BTreeMap<String, u64> = BTreeMap::new();
    let mut flag_bits = [0u64; 16];
    let mut kinds: BTreeMap<&str, u64> = BTreeMap::new();
    let mut divisors: BTreeMap<u32, u64> = BTreeMap::new();
    let mut per_space: BTreeMap<String, (u64, u64)> = BTreeMap::new();
    let mut per_cell: HashMap<u32, u32> = HashMap::new();
    let mut winding = [0u64; 3]; // ccw, cw, degenerate (2D, seen from above)
    let mut slopes: BTreeMap<u32, u64> = BTreeMap::new(); // 5-degree bins
    let (mut verts, mut tris, mut links, mut doors, mut cover) = (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut header_flags: BTreeMap<String, u64> = BTreeMap::new();
    // Per link kind: (vertical drop from this edge's middle to the target triangle's
    // centre, horizontal distance between them), for percentiles.
    let mut link_geom: BTreeMap<&str, Vec<(f32, f32)>> = BTreeMap::new();
    for (&id, m) in ms {
        *versions.entry(format!("{:?}", m.version)).or_default() += 1;
        verts += m.vertices.len() as u64;
        tris += m.triangles.len() as u64;
        links += m.edge_links.len() as u64;
        doors += m.door_links.len() as u64;
        cover += m.cover_triangles.len() as u64;
        for b in 0..32 {
            if m.flags & (1 << b) != 0 {
                *header_flags.entry(format!("bit{b}")).or_default() += 1;
            }
        }
        for i in &m.issues {
            t.add(&format!("record issue: {}", i.split(':').next().unwrap_or(i)), || format!("{} {i}", fid(id)));
        }
        let actual = [m.vertices.len(), m.triangles.len(), m.edge_links.len(), m.cover_triangles.len(), m.door_links.len()];
        for (k, name) in ["vertices", "triangles", "edge_links", "cover_triangles", "door_links"].iter().enumerate() {
            if m.counts[k] as usize != actual[k] {
                t.add(&format!("DATA count differs: {name}"), || format!("{} DATA {} actual {}", fid(id), m.counts[k], actual[k]));
            }
        }
        let stored = nav.stored_in.get(&id).copied();
        if stored != m.cell {
            t.add("DATA cell differs from the group's cell", || format!("{} DATA {:?} group {:?}", fid(id), m.cell.map(fid), stored.map(fid)));
        }
        let cell = m.cell.or(stored);
        if let Some(c) = cell {
            *per_cell.entry(c).or_default() += 1;
        }
        let space = match cell.and_then(|c| ix.cells.get(&c)) {
            Some(c) => match c.world.and_then(|w| ix.worldspaces.get(&w)) {
                Some(w) => w.edid.clone().unwrap_or_else(|| fid(w.form_id)),
                None => "(interiors)".to_string(),
            },
            None => "(cell not indexed)".to_string(),
        };
        let e = per_space.entry(space).or_default();
        e.0 += 1;
        e.1 += m.triangles.len() as u64;
        let nv = m.vertices.len();
        let nt = m.triangles.len();
        for (ti, tr) in m.triangles.iter().enumerate() {
            for (b, n) in flag_bits.iter_mut().enumerate() {
                if tr.flags & (1 << b) != 0 {
                    *n += 1;
                }
            }
            if tr.v.iter().any(|&v| v as usize >= nv) {
                t.add("triangle vertex out of range", || format!("{} tri {ti}", fid(id)));
                continue;
            }
            let p = tr.v.map(|v| m.vertices[v as usize]);
            let area = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
            winding[if area > 1e-3 { 0 } else if area < -1e-3 { 1 } else { 2 }] += 1;
            if area.abs() > 1e-3 {
                let deg = fnv_nav::graph::slope_cos(p).clamp(0.0, 1.0).acos().to_degrees();
                *slopes.entry((deg / 5.0) as u32 * 5).or_default() += 1;
            }
            for e in 0..3 {
                let v = tr.edges[e];
                if tr.edge_is_link(e) {
                    match m.edge_links.get(v as usize).filter(|_| v >= 0) {
                        None => t.add("linked edge: NVEX index out of range", || format!("{} tri {ti} edge {e} = {v}", fid(id))),
                        Some(l) => match l.navmesh.and_then(|n| ms.get(&n)) {
                            None => t.add("linked edge: target navmesh missing", || format!("{} tri {ti} -> {:?}", fid(id), l.navmesh.map(fid))),
                            Some(o) => match o.triangles.get(l.triangle as usize) {
                                None => t.add("linked edge: target triangle out of range", || format!("{} tri {ti} -> {} tri {}", fid(id), fid(o.form_id), l.triangle)),
                                Some(ot) => {
                                    let back = (0..3).any(|oe| {
                                        ot.edge_is_link(oe)
                                            && ot.edges[oe] >= 0
                                            && o.edge_links.get(ot.edges[oe] as usize).is_some_and(|bl| bl.navmesh == Some(id) && bl.triangle as usize == ti)
                                    });
                                    t.add(if back { "linked edge: reciprocal" } else { "linked edge: not reciprocal" }, || {
                                        format!("{} tri {ti} -> {} tri {}", fid(id), fid(o.form_id), l.triangle)
                                    });
                                    if l.navmesh == Some(id) {
                                        t.add(&format!("linked edge: same navmesh ({})", link_kind::name(l.kind)), || format!("{} tri {ti}", fid(id)));
                                    }
                                    if ot.v.iter().all(|&v| (v as usize) < o.vertices.len()) {
                                        let c = ot.v.map(|v| o.vertices[v as usize]);
                                        let c = [0, 1, 2].map(|k| (c[0][k] + c[1][k] + c[2][k]) / 3.0);
                                        let mid = [0, 1, 2].map(|k| (p[e][k] + p[(e + 1) % 3][k]) / 2.0);
                                        link_geom.entry(link_kind::name(l.kind)).or_default().push((mid[2] - c[2], (mid[0] - c[0]).hypot(mid[1] - c[1])));
                                    }
                                    // Same edge geometry? The two shared vertices of the
                                    // other triangle closest to this edge's ends.
                                    let a = p[e];
                                    let b = p[(e + 1) % 3];
                                    let near = |q: [f32; 3]| {
                                        ot.v.iter()
                                            .filter_map(|&ov| o.vertices.get(ov as usize))
                                            .map(|w| ((w[0] - q[0]).powi(2) + (w[1] - q[1]).powi(2) + (w[2] - q[2]).powi(2)).sqrt())
                                            .fold(f32::INFINITY, f32::min)
                                    };
                                    let gap = near(a).max(near(b));
                                    let bucket = if gap < 0.01 {
                                        "<0.01"
                                    } else if gap < 1.0 {
                                        "<1"
                                    } else if gap < 16.0 {
                                        "<16"
                                    } else {
                                        ">=16"
                                    };
                                    t.add(&format!("linked edge: end gap {bucket}"), || format!("{} tri {ti} gap {gap:.3}", fid(id)));
                                }
                            },
                        },
                    }
                } else if v == -1 {
                    t.add("edge: none (-1)", String::new);
                } else if v < 0 || v as usize >= nt {
                    t.add("edge: neighbour out of range", || format!("{} tri {ti} edge {e} = {v}", fid(id)));
                } else {
                    let o = &m.triangles[v as usize];
                    let back = (0..3).any(|oe| !o.edge_is_link(oe) && o.edges[oe] as usize == ti);
                    if !back {
                        t.add("edge: neighbour not reciprocal", || format!("{} tri {ti} edge {e} -> {v}", fid(id)));
                    }
                    // The neighbour shares this edge's two vertices.
                    let (a, b) = (tr.v[e], tr.v[(e + 1) % 3]);
                    if !(o.v.contains(&a) && o.v.contains(&b)) {
                        t.add("edge: neighbour does not share the edge's vertices", || format!("{} tri {ti} edge {e} -> {v}", fid(id)));
                    }
                }
            }
        }
        for l in &m.edge_links {
            *kinds.entry(link_kind::name(l.kind)).or_default() += 1;
        }
        for d in &m.door_links {
            match m.triangles.get(d.triangle as usize) {
                None => t.add("door link: triangle out of range", || format!("{} -> door {}", fid(id), fid(d.door))),
                Some(tr) if !tr.has(tri_flags::DOOR) => t.add("door link: triangle lacks the Door flag", || format!("{} tri {}", fid(id), d.triangle)),
                Some(_) => t.add("door link: triangle has the Door flag", String::new),
            }
            match ix.refs.get(&d.door) {
                None => t.add("door link: door reference not placed", || format!("{} -> {}", fid(id), fid(d.door))),
                Some(r) if Some(r.cell) == cell => t.add("door link: door in the navmesh's cell", String::new),
                Some(_) => t.add("door link: door in another cell", || format!("{} -> {}", fid(id), fid(d.door))),
            }
        }
        let door_tris: Vec<usize> = m.triangles.iter().enumerate().filter(|(_, tr)| tr.has(tri_flags::DOOR)).map(|(i, _)| i).collect();
        for ti in door_tris {
            if !m.door_links.iter().any(|d| d.triangle as usize == ti) {
                t.add("Door-flag triangle without a door link", || format!("{} tri {ti}", fid(id)));
            }
        }
        match &m.grid {
            None => t.add("no NVGD", || fid(id)),
            Some(g) => {
                *divisors.entry(g.divisor).or_default() += 1;
                if g.cells.len() as u64 != (g.divisor as u64).pow(2) {
                    t.add("NVGD squares != divisor^2", || format!("{} divisor {} squares {}", fid(id), g.divisor, g.cells.len()));
                }
                if g.cells.iter().flatten().any(|&ti| ti as usize >= nt) {
                    t.add("NVGD triangle out of range", || fid(id));
                }
                let listed: std::collections::HashSet<u16> = g.cells.iter().flatten().copied().collect();
                if listed.len() < nt {
                    t.add("NVGD leaves triangles out", || format!("{} {} of {}", fid(id), listed.len(), nt));
                }
            }
        }
    }
    let mut cells_hist: BTreeMap<u32, u64> = BTreeMap::new();
    for n in per_cell.values() {
        *cells_hist.entry(*n).or_default() += 1;
    }
    // NAVI
    let mut info_t = Tally::default();
    let mut infos = 0u64;
    let mut islands = 0u64;
    let mut covered = std::collections::HashSet::new();
    for map in &nav.info_maps {
        for i in &map.issues {
            info_t.add(&format!("record issue: {}", i.split(':').next().unwrap_or(i)), || format!("{} {i}", fid(map.form_id)));
        }
        for i in &map.infos {
            infos += 1;
            islands += i.island.is_some() as u64;
            match i.navmesh {
                Some(n) if ms.contains_key(&n) => {
                    covered.insert(n);
                    let m = &ms[&n];
                    let loc_ok = i.location == m.cell || i.location.and_then(|l| ix.worldspaces.get(&l)).is_some();
                    info_t.add(if loc_ok { "NVMI location is the cell or a worldspace" } else { "NVMI location differs" }, || {
                        format!("{} loc {:?} cell {:?}", fid(n), i.location.map(fid), m.cell.map(fid))
                    });
                }
                other => info_t.add("NVMI navmesh missing", || format!("{:?}", other.map(fid))),
            }
        }
    }
    json!({
        "schema": "fnv.nav.survey/1",
        "definitions_per_plugin": lo.plugins.iter().zip(&nav.definitions).map(|(p, n)| json!({"plugin": p.name, "navm": n})).collect::<Vec<_>>(),
        "navmeshes": ms.len(),
        "deleted_winners": nav.deleted,
        "errors": nav.errors.iter().take(20).map(|(f, e)| json!({"form_id": fid(*f), "error": e})).collect::<Vec<_>>(),
        "versions": versions,
        "header_flags": header_flags,
        "initially_disabled": ms.values().filter(|m| m.initially_disabled()).count(),
        "totals": {"vertices": verts, "triangles": tris, "edge_links": links, "door_links": doors, "cover_triangles": cover},
        "by_space": per_space.iter().map(|(k, (n, t))| (k.clone(), json!({"navmeshes": n, "triangles": t}))).collect::<serde_json::Map<_, _>>(),
        "navmeshes_per_cell": cells_hist,
        "triangle_flag_bits": (0..16).filter(|&b| flag_bits[b] > 0).map(|b| (format!("bit{b}"), json!(flag_bits[b]))).collect::<serde_json::Map<_, _>>(),
        "slope_degrees_by_5": slopes,
        "winding_from_above": {"counter_clockwise": winding[0], "clockwise": winding[1], "degenerate": winding[2]},
        "edge_link_kinds": kinds,
        "edge_link_geometry": link_geom.iter_mut().map(|(k, v)| {
            let pct = |v: &mut Vec<f32>| {
                v.sort_by(f32::total_cmp);
                json!([0.0, 0.05, 0.5, 0.95, 1.0].map(|q| v[((v.len() - 1) as f32 * q) as usize]))
            };
            let mut dz: Vec<f32> = v.iter().map(|g| g.0).collect();
            let mut dxy: Vec<f32> = v.iter().map(|g| g.1).collect();
            (k.to_string(), json!({"percentiles": [0, 5, 50, 95, 100], "drop_to_target_centre": pct(&mut dz), "horizontal_to_target_centre": pct(&mut dxy)}))
        }).collect::<serde_json::Map<_, _>>(),
        "grid_divisors": divisors,
        "checks": t.json(),
        "navi": {
            "records": nav.info_maps.len(),
            "infos": infos,
            "islands": islands,
            "connections": nav.info_maps.iter().map(|m| m.connections.len()).sum::<usize>(),
            "navmeshes_with_info": covered.len(),
            "checks": info_t.json(),
        },
    })
}
