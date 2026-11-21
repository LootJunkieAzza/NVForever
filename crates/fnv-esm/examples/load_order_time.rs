//! Times `LoadOrder::new` over the plugins named on the command line (#1994): the
//! plugins are read first, then the load order is built `RUNS` times; prints each run's
//! milliseconds, the fastest and the median, and the index's size.
//!
//!     cargo run --release -p fnv-esm --example load_order_time -- <Data>/FalloutNV.esm ...

use fnv_esm::{LoadOrder, Plugin};
use std::time::Instant;

const RUNS: usize = 7;

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    let plugins: Vec<Plugin> = paths.iter().map(|p| Plugin::open(p).unwrap_or_else(|e| panic!("{p}: {e}"))).collect();
    let mut ms = Vec::new();
    let mut size = (0, 0);
    for _ in 0..RUNS {
        let t = Instant::now();
        let lo = LoadOrder::new(&plugins).expect("load order");
        ms.push(t.elapsed().as_secs_f64() * 1000.0);
        size = (lo.index.len(), lo.index.values().filter(|d| d.len() > 1).count());
        drop(lo);
    }
    let runs: Vec<String> = ms.iter().map(|m| format!("{m:.0}")).collect();
    let mut sorted = ms.clone();
    sorted.sort_by(f64::total_cmp);
    println!("LoadOrder::new over {} plugins: {} ms (fastest {:.0}, median {:.0}); {} FormIDs, {} with more than one definition", plugins.len(), runs.join(" "), sorted[0], sorted[RUNS / 2], size.0, size.1);
}
