//! Diagnostic survey of the specular masks in normal maps' alpha (docs/spec-notes/nif.md
//! §6 "Specular"): for every `*_n.dds` in the archives given, its format and, for the
//! formats with an alpha channel (DXT3, DXT5, A8R8G8B8), the mean of the top mip's alpha.
//! Prints the counts by format and the spread of the per-file means (quartiles), for
//! choosing the mask of normal maps without an alpha channel (DXT1).
//!
//! cargo run --release -p fnv-nif --example normal_mask_survey -- <Textures.bsa>...

use fnv_dds::{Dds, Format};
use std::collections::BTreeMap;

/// Mean alpha (0–255) of a DXT5 top mip.
fn dxt5_mean(data: &[u8]) -> f64 {
    let (mut sum, mut n) = (0u64, 0u64);
    for blk in data.as_chunks::<16>().0 {
        let (a0, a1) = (blk[0] as u32, blk[1] as u32);
        let mut bits = 0u64;
        for (k, b) in blk[2..8].iter().enumerate() {
            bits |= (*b as u64) << (8 * k);
        }
        let pal: Vec<u32> = if a0 > a1 {
            let mut p = vec![a0, a1];
            p.extend((1..7).map(|k| ((7 - k) * a0 + k * a1) / 7));
            p
        } else {
            let mut p = vec![a0, a1];
            p.extend((1..5).map(|k| ((5 - k) * a0 + k * a1) / 5));
            p.extend([0, 255]);
            p
        };
        for i in 0..16 {
            sum += pal[((bits >> (3 * i)) & 7) as usize] as u64;
            n += 1;
        }
    }
    sum as f64 / n.max(1) as f64
}

/// Mean alpha of a DXT3 top mip.
fn dxt3_mean(data: &[u8]) -> f64 {
    let (mut sum, mut n) = (0u64, 0u64);
    for blk in data.as_chunks::<16>().0 {
        for b in &blk[..8] {
            sum += ((b & 15) * 17) as u64 + ((b >> 4) * 17) as u64;
            n += 2;
        }
    }
    sum as f64 / n.max(1) as f64
}

fn main() {
    let mut formats: BTreeMap<&'static str, usize> = BTreeMap::new();
    let mut means: Vec<f64> = Vec::new();
    for arc in std::env::args().skip(1) {
        let mut bsa = fnv_bsa::Archive::open(&arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> =
            bsa.entries().map(|e| (e.path(), e.record.clone())).filter(|(p, _)| p.to_ascii_lowercase().ends_with("_n.dds")).collect();
        for (_path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(dds) = Dds::parse(&data) else {
                *formats.entry("unreadable").or_default() += 1;
                continue;
            };
            let format = dds.format;
            *formats.entry(format.name()).or_default() += 1;
            let top = dds.levels[0][0];
            match format {
                Format::Dxt5 => means.push(dxt5_mean(top)),
                Format::Dxt3 => means.push(dxt3_mean(top)),
                Format::A8R8G8B8 => means.push(top.as_chunks::<4>().0.iter().map(|p| p[3] as f64).sum::<f64>() / (top.len() / 4).max(1) as f64),
                _ => {}
            }
        }
    }
    means.sort_by(|a, b| a.total_cmp(b));
    let q = |f: f64| means.get(((means.len() as f64 - 1.0) * f).round() as usize).copied().unwrap_or(0.0);
    println!("normal maps by format: {formats:?}");
    println!(
        "mean alpha of the {} with an alpha channel: quartiles {:.0} / {:.0} / {:.0}, 10th and 90th percentiles {:.0} / {:.0} (of 255)",
        means.len(),
        q(0.25),
        q(0.5),
        q(0.75),
        q(0.1),
        q(0.9)
    );
}
