//! Prints a worldspace climate's weathers: cloud textures, cloud colours per time of day,
//! cloud speeds and the NAM0 sky colours, for the sky renderer (docs/spec-notes/world.md §6).
//!
//! With `--survey` instead of a climate: for every weather with six times of day, how
//! often the high-noon and midnight colours equal the day and night ones, or are black.
//!
//! cargo run --release -p fnv-world --example weather_dump -- <Data dir> <esm schema dir> [climate EDID | --survey]

use fnv_esm::{LoadOrder, Plugin, Schema};
use fnv_world::fields::Fields;
use fnv_world::sky::{climate, weather, COLOR_TYPES};
use std::path::Path;

const MASTERS: [&str; 10] = [
    "FalloutNV.esm", "DeadMoney.esm", "HonestHearts.esm", "OldWorldBlues.esm", "LonesomeRoad.esm",
    "GunRunnersArsenal.esm", "ClassicPack.esm", "MercenaryPack.esm", "TribalPack.esm", "CaravanPack.esm",
];

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let data = Path::new(&a[1]);
    let schema = Schema::load_dir(Path::new(&a[2])).unwrap();
    let want = a.get(3).map_or("NVDefaultClimate", |s| s.as_str());
    let plugins: Vec<Plugin> = MASTERS.iter().map(|m| Plugin::open(data.join(m)).unwrap()).collect();
    let lo = LoadOrder::new(&plugins).unwrap();
    if want == "--survey" {
        survey(&lo, &schema);
        return;
    }
    let mut clim_id = None;
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"CLMT") {
            let f = Fields::new(r, pi, &schema).unwrap();
            if f.string(b"EDID").is_some_and(|e| e.eq_ignore_ascii_case(want)) {
                clim_id = lo.to_global(pi, r.header.form_id);
            }
        }
    }
    let c = climate(&lo, &schema, clim_id.expect("climate not found")).unwrap();
    println!("climate {:?} sun {:?} glare {:?} sunrise {:?} sunset {:?}", c.edid, c.sun_texture, c.sun_glare_texture, c.sunrise, c.sunset);
    let names = ["sky-upper", "fog", "unused", "ambient", "sunlight", "sun", "stars", "sky-lower", "horizon", "effect"];
    for wc in &c.weathers {
        let Some(w) = weather(&lo, &schema, wc.weather) else { continue };
        println!("\nweather {:?} chance {} global {:?}", w.edid, wc.chance, wc.global.map(|g| format!("{g:#010x}")));
        println!("  max cloud layers {:?}, speeds {:?}", w.max_cloud_layers, w.cloud_speeds);
        for l in 0..4 {
            println!("  cloud layer {l}: {:?}  colours {:?}", w.cloud_textures[l], w.cloud_colors.get(l));
        }
        for (t, set) in w.colors.iter().enumerate().take(COLOR_TYPES) {
            println!("  {:10} {:?}", names[t], set);
        }
        println!("  fog {:?}", w.fog);
    }
}

fn survey(lo: &LoadOrder, schema: &Schema) {
    let names = ["sky-upper", "fog", "unused", "ambient", "sunlight", "sun", "stars", "sky-lower", "horizon", "effect"];
    let (mut six, mut four) = (0, 0);
    // Per colour type: [noon == day, noon black, midnight == night, midnight black].
    let mut counts = [[0usize; 4]; COLOR_TYPES];
    let mut black_mid_ambient = Vec::new();
    for (pi, recs) in lo.records.iter().enumerate() {
        for r in recs.iter().filter(|r| &r.header.sig == b"WTHR") {
            let Some(id) = lo.to_global(pi, r.header.form_id) else { continue };
            let Some(w) = weather(lo, schema, id) else { continue };
            if w.colors.first().is_none_or(|c| c.len() < 6) {
                four += 1;
                continue;
            }
            six += 1;
            for (t, set) in w.colors.iter().enumerate().take(COLOR_TYPES) {
                let c = &mut counts[t];
                c[0] += (set[4] == set[1]) as usize;
                c[1] += (set[4] == [0, 0, 0]) as usize;
                c[2] += (set[5] == set[3]) as usize;
                c[3] += (set[5] == [0, 0, 0]) as usize;
            }
            if w.colors[3][5] == [0, 0, 0] && w.colors[3][3] != [0, 0, 0] {
                black_mid_ambient.push(w.edid.clone().unwrap_or_default());
            }
        }
    }
    println!("weathers: {six} with six times, {four} with four");
    println!("{:10} {:>9} {:>10} {:>14} {:>14}", "type", "noon=day", "noon black", "midnight=night", "midnight black");
    for (t, c) in counts.iter().enumerate() {
        println!("{:10} {:9} {:10} {:14} {:14}", names[t], c[0], c[1], c[2], c[3]);
    }
    println!("black midnight ambient but non-black night ambient: {} {:?}", black_mid_ambient.len(), &black_mid_ambient[..black_mid_ambient.len().min(12)]);
}
