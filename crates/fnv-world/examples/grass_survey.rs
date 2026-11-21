//! Evidence for the grass notes (docs/spec-notes/grass.md): every grass type (GRAS) with
//! its fields, which landscape textures (LTEX) name which grass types, and, for one cell,
//! how many plants the generator places per type.
//!
//! Reads the plugins named on the command line only (pass the base game's master, not a
//! whole install).
//!
//! cargo run --release -p fnv-world --example grass_survey -- <esm schema dir> <plugin.esm>... [--world edid] [--cell x,y]

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::grass::{load_grass, place, texture_grasses, Coverage, Grass, GrassSettings};
use fnv_world::land::{land_texture, load_land, LandParts};
use fnv_world::WorldIndex;
use std::collections::{BTreeMap, HashMap};
use std::path::Path;

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    let mut plugins = Vec::new();
    let (mut world, mut cell) = ("WastelandNV".to_string(), None);
    let mut i = 1;
    while i < a.len() {
        match a[i].as_str() {
            "--world" => {
                world = a[i + 1].clone();
                i += 1;
            }
            "--cell" => {
                let (x, y) = a[i + 1].split_once(',').expect("--cell x,y");
                cell = Some((x.parse::<i32>().unwrap(), y.parse::<i32>().unwrap()));
                i += 1;
            }
            p => plugins.push(Plugin::open(Path::new(p)).unwrap()),
        }
        i += 1;
    }
    let schema = Schema::load_dir(Path::new(&a[0])).unwrap();
    let lo = LoadOrder::new(&plugins).unwrap();

    // Every record of a type, by winning definition.
    let of_type = |sig: &[u8; 4]| -> Vec<u32> {
        let mut v: Vec<u32> = lo
            .index
            .iter()
            .filter(|(_, d)| d.last().is_some_and(|d| &lo.records[d.plugin as usize][d.ordinal as usize].header.sig == sig))
            .map(|(&id, _)| id)
            .collect();
        v.sort_unstable();
        v
    };

    let grasses: BTreeMap<u32, Grass> = of_type(b"GRAS").into_iter().filter_map(|id| load_grass(&lo, &schema, id).map(|g| (id, g))).collect();
    println!("{} grass types", grasses.len());
    println!("  formid   density slope    water              pos-range height-range colour-range wave  flags  edid / model");
    for g in grasses.values() {
        let flags: String = [(g.vertex_lighting, 'V'), (g.uniform_scaling, 'U'), (g.fit_to_slope, 'F')].iter().map(|&(on, c)| if on { c } else { '-' }).collect();
        println!(
            "  {:08X} {:>7} {:>3}-{:<3} {:<18} {:>9} {:>12} {:>12} {:>5}  {}  {} / {}",
            g.form_id,
            g.density,
            g.min_slope,
            g.max_slope,
            format!("{:?} {}", g.water_rule, g.water_distance),
            g.position_range,
            g.height_range,
            g.color_range,
            g.wave_period,
            flags,
            g.edid.as_deref().unwrap_or("-"),
            g.model.as_deref().unwrap_or("-"),
        );
    }

    let textures = of_type(b"LTEX");
    let mut with = 0;
    let mut most = 0;
    println!("\nlandscape textures that name grass:");
    for &t in &textures {
        let gs = texture_grasses(&lo, &schema, t);
        if gs.is_empty() {
            continue;
        }
        with += 1;
        most = most.max(gs.len());
        let lt = land_texture(&lo, &schema, t).unwrap_or_default();
        let names: Vec<String> = gs.iter().map(|g| grasses.get(g).and_then(|g| g.edid.clone()).unwrap_or_else(|| format!("{g:08X}?"))).collect();
        println!("  {:08X} {:<32} {:<44} {}", t, lt.edid.as_deref().unwrap_or("-"), lt.diffuse.as_deref().unwrap_or("-"), names.join(", "));
    }
    println!("{with} of {} landscape textures name grass; at most {most} types on one texture", textures.len());

    let Some(grid) = cell else { return };
    let ix = WorldIndex::build(&lo, &schema);
    let w = ix.world_by_name(&world).expect("worldspace");
    let c = &ix.cells[&w.grid[&grid]];
    let land = load_land(&lo, &schema, c.land.expect("cell has no LAND"), LandParts::ALL).expect("LAND");
    let settings = GrassSettings::default();
    let types: Vec<Grass> = grasses.values().cloned().collect();
    let index: HashMap<u32, usize> = types.iter().enumerate().map(|(i, g)| (g.form_id, i)).collect();
    let cover = Coverage::new(&land);
    let mut texture_types = HashMap::new();
    println!("\ncell {},{} of {world}: textures", grid.0, grid.1);
    for t in cover.textures() {
        let gs: Vec<usize> = texture_grasses(&lo, &schema, t).iter().filter_map(|g| index.get(g).copied()).take(settings.max_types_per_texture).collect();
        let lt = land_texture(&lo, &schema, t).unwrap_or_default();
        // Share of the cell the texture shows on, sampled on a 64 × 64 grid.
        let share: f32 = (0..64 * 64).map(|i| cover.at(t, (i % 64) as f32 * 64.0 + 32.0, (i / 64) as f32 * 64.0 + 32.0)).sum::<f32>() / 4096.0;
        println!("  {:08X} {:<32} {:>5.1}% of the cell, {} grass types", t, lt.edid.as_deref().unwrap_or("-"), share * 100.0, gs.len());
        texture_types.insert(t, gs);
    }
    let water = if c.has_water() { c.water_surface(w.default_water_height) } else { None };
    let plants = place(&land, grid, water, &types, &texture_types, &settings);
    let mut per: BTreeMap<usize, usize> = BTreeMap::new();
    for p in &plants {
        *per.entry(p.kind).or_default() += 1;
    }
    println!("{} plants (water {water:?}):", plants.len());
    for (k, n) in per {
        println!("  {:>6}  {}", n, types[k].edid.as_deref().unwrap_or("-"));
    }
}
