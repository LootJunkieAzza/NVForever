//! Diagnostic: the text keys of every animation sequence in the archives named, by kind,
//! with the values of the sound-like ones (`Sound:`, `Enum:`, `m:`) and the files they
//! sit in. Used for the sounds animations name (docs/spec-notes/audio.md §10).
//!
//! cargo run --release -p fnv-nif --example kf_sound_keys -- <nif schema dir> <archive.bsa>...
//!
//! Name the archives explicitly (the base game's `Fallout - Meshes.bsa`); `KEYS_FILTER=x`
//! prints every key whose text contains `x` (case-insensitive) with its file and time.

use fnv_nif::anim::sequences;
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema};
use std::collections::{BTreeMap, BTreeSet};

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let filter = std::env::var("KEYS_FILTER").ok().map(|f| f.to_ascii_lowercase());
    let mut kinds: BTreeMap<String, u64> = BTreeMap::new();
    // kind → value → (count, a few files)
    let mut values: BTreeMap<String, BTreeMap<String, (u64, BTreeSet<String>)>> = BTreeMap::new();
    let (mut files, mut seqs) = (0u64, 0u64);
    for bsa in &a[2..] {
        let mut ar = fnv_bsa::Archive::open(bsa).unwrap();
        let list: Vec<(String, fnv_bsa::FileRecord)> = ar
            .entries()
            .map(|e| (e.path().to_ascii_lowercase(), e.record.clone()))
            .filter(|(p, _)| p.ends_with(".kf") || p.ends_with(".nif"))
            .collect();
        for (p, r) in list {
            let Ok(data) = ar.extract(&r) else { continue };
            let Ok(f) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&f, &data, &schema);
            let ss = sequences(&f, &dec);
            if ss.is_empty() {
                continue;
            }
            files += 1;
            for s in ss {
                seqs += 1;
                for (t, k) in &s.text_keys {
                    let k = k.replace(['\r', '\n'], " ");
                    let word = k.split(|c: char| c == ':' || c.is_whitespace()).next().unwrap_or_default().to_ascii_lowercase();
                    *kinds.entry(word.clone()).or_default() += 1;
                    if let Some(f) = &filter {
                        if k.to_ascii_lowercase().contains(f.as_str()) {
                            println!("{p} {:?} {t:.3}: {k}", s.name.as_deref().unwrap_or(""));
                        }
                    }
                    if matches!(word.as_str(), "sound" | "enum" | "m") {
                        let v = k.split_once(':').map_or("", |(_, v)| v).trim().to_string();
                        let e = values.entry(word).or_default().entry(v).or_default();
                        e.0 += 1;
                        if e.1.len() < 3 {
                            e.1.insert(p.clone());
                        }
                    }
                }
            }
        }
    }
    println!("{files} files with sequences, {seqs} sequences");
    for (k, n) in &kinds {
        println!("kind {k:?}: {n}");
    }
    for (kind, vs) in &values {
        println!("== {kind}: {} distinct values", vs.len());
        for (v, (n, fs)) in vs {
            println!("  {n:6} {v:?}  e.g. {}", fs.iter().cloned().collect::<Vec<_>>().join(", "));
        }
    }
}
