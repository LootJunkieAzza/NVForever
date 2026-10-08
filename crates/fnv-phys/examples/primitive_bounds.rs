//! Evidence for what placed primitive bounds (REFR XPRM) mean: half or full extents
//! (docs/spec-notes/physics.md §2.3).
//!
//! For every exterior CollisionMarker box (XPRM type 1 or 3) without x/y rotation, compare
//! the box bottom under both readings with the terrain height under the reference:
//! `gap = bottom - ground`. A collision wall the player cannot pass under should not
//! float; the reading under which few boxes float well above the ground is the likelier
//! one. Also reports how far box tops reach above the ground.
//!
//! cargo run --release -p fnv-phys --example primitive_bounds -- <Data dir> <esm schema dir>

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_phys::primitive::{primitive, PrimitiveKind, COLLISION_MARKER};
use fnv_phys::terrain::height_at;
use fnv_world::land::{load_land, LandParts};
use fnv_world::placed::placement;
use fnv_world::{cell_of, WorldIndex, CELL_SIZE};
use std::collections::BTreeMap;
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm",
    "DeadMoney.esm",
    "HonestHearts.esm",
    "OldWorldBlues.esm",
    "LonesomeRoad.esm",
    "GunRunnersArsenal.esm",
    "ClassicPack.esm",
    "MercenaryPack.esm",
    "TribalPack.esm",
    "CaravanPack.esm",
];

fn quantiles(v: &mut [f32]) -> String {
    if v.is_empty() {
        return "-".into();
    }
    v.sort_by(f32::total_cmp);
    let q = |p: f32| v[((v.len() - 1) as f32 * p) as usize];
    format!("p10 {:.0}  p25 {:.0}  median {:.0}  p75 {:.0}  p90 {:.0}", q(0.1), q(0.25), q(0.5), q(0.75), q(0.9))
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    let ix = WorldIndex::build(&lo, &schema);

    let mut half_gap = Vec::new();
    let mut full_gap = Vec::new();
    let mut half_top = Vec::new();
    let mut full_top = Vec::new();
    let mut skipped_rotated = 0;
    let mut no_ground = 0;
    for ws in ix.worldspaces.values() {
        let mut land: BTreeMap<(i32, i32), Vec<f32>> = BTreeMap::new();
        for (&g, c) in &ws.grid {
            let h = ix.cells[c].land.and_then(|l| load_land(&lo, &schema, l, LandParts::HEIGHTS)).and_then(|l| l.heights);
            if let Some(h) = h {
                land.insert(g, h);
            }
        }
        let ground_at = |x: f32, y: f32| {
            let g = cell_of(x, y);
            height_at(land.get(&g)?, x - g.0 as f32 * CELL_SIZE, y - g.1 as f32 * CELL_SIZE)
        };
        let cells = ws.grid.values().chain(ws.persistent_cell.iter());
        for c in cells {
            for &r in &ix.cells[c].refs {
                let Some(p) = placement(&lo, &schema, r) else { continue };
                if p.base != Some(COLLISION_MARKER) || p.deleted() {
                    continue;
                }
                let Some(prim) = primitive(&lo, &schema, r) else { continue };
                if !matches!(prim.kind, PrimitiveKind::Box | PrimitiveKind::PortalBox) {
                    continue;
                }
                if p.rotation[0].abs() > 1e-3 || p.rotation[1].abs() > 1e-3 {
                    skipped_rotated += 1;
                    continue;
                }
                let Some(ground) = ground_at(p.position[0], p.position[1]) else {
                    no_ground += 1;
                    continue;
                };
                let bz = prim.bounds[2].abs() * p.scale;
                half_gap.push(p.position[2] - bz - ground);
                full_gap.push(p.position[2] - bz / 2.0 - ground);
                half_top.push(p.position[2] + bz - ground);
                full_top.push(p.position[2] + bz / 2.0 - ground);
            }
        }
    }
    let n = half_gap.len();
    let floating = |v: &[f32], t: f32| v.iter().filter(|&&g| g > t).count();
    println!("exterior CollisionMarker boxes without x/y rotation: {n} (rotated skipped {skipped_rotated}, no terrain {no_ground})");
    for t in [0.0, 16.0, 64.0] {
        println!(
            "bottom more than {t:>4} above ground: half-extent reading {} ({:.1}%), full-extent reading {} ({:.1}%)",
            floating(&half_gap, t),
            100.0 * floating(&half_gap, t) as f32 / n as f32,
            floating(&full_gap, t),
            100.0 * floating(&full_gap, t) as f32 / n as f32
        );
    }
    println!("bottom - ground, half: {}", quantiles(&mut half_gap));
    println!("bottom - ground, full: {}", quantiles(&mut full_gap));
    println!("top - ground, half:    {}", quantiles(&mut half_top));
    println!("top - ground, full:    {}", quantiles(&mut full_top));
}
