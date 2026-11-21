//! Diagnostic survey for animated statics:
//! - which stored tangent of a quadratic key leaves the key and which arrives at it. For
//!   every quadratic key group (interpolation 2) with at least three keys, at each inner
//!   key whose neighbouring segments change by clearly different amounts, compares the
//!   two tangents with the change over the previous and the next segment. Reading
//!   "forward leaves" expects forward ≈ next change and backward ≈ previous change;
//!   reading "forward arrives" the other way round.
//! - the names of the sequences embedded in meshes (`.nif`), as sets per file, for
//!   deciding which sequence a placed object plays from load.
//!
//! cargo run --release -p fnv-nif --example quad_key_survey -- <schema dir> <archive.bsa>...

use fnv_nif::anim::sequences;
use fnv_nif::scene::Decoded;
use fnv_nif::{NifFile, Schema, Struct, Value};
use std::collections::BTreeMap;

#[derive(Default)]
struct Tally {
    groups: usize,
    keys: usize,
    forward_leaves: usize,
    forward_arrives: usize,
    unclear: usize,
    examples: Vec<String>,
}

fn floats(v: Option<&Value>) -> Option<Vec<f32>> {
    match v? {
        Value::Float(f) => Some(vec![*f]),
        Value::Floats(f) => Some(f.clone()),
        _ => None,
    }
}

fn dist(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).sum()
}

fn group(s: &Struct, path: &str, t: &mut Tally) {
    let Some(keys) = s.list("keys") else { return };
    let ks: Vec<(Vec<f32>, Vec<f32>, Vec<f32>)> = keys
        .iter()
        .filter_map(|k| match k {
            Value::Struct(k) => Some((floats(k.get("value"))?, floats(k.get("forward"))?, floats(k.get("backward"))?)),
            _ => None,
        })
        .collect();
    if ks.len() < 3 {
        return;
    }
    t.groups += 1;
    for i in 1..ks.len() - 1 {
        let d = |a: &[f32], b: &[f32]| -> Vec<f32> { a.iter().zip(b).map(|(x, y)| y - x).collect() };
        let prev = d(&ks[i - 1].0, &ks[i].0);
        let next = d(&ks[i].0, &ks[i + 1].0);
        let (fwd, bwd) = (&ks[i].1, &ks[i].2);
        // Only keys where the two readings predict clearly different tangents.
        let gap = dist(&prev, &next);
        if gap < 1e-3 {
            continue;
        }
        t.keys += 1;
        let leaves = dist(fwd, &next) + dist(bwd, &prev);
        let arrives = dist(fwd, &prev) + dist(bwd, &next);
        if (leaves - arrives).abs() < 0.1 * gap {
            t.unclear += 1;
        } else if leaves < arrives {
            t.forward_leaves += 1;
        } else {
            t.forward_arrives += 1;
            if t.examples.len() < 5 {
                t.examples.push(format!("{path}: prev {prev:.3?} next {next:.3?} forward {fwd:.3?} backward {bwd:.3?}"));
            }
        }
    }
}

fn walk(v: &Value, path: &str, t: &mut Tally) {
    match v {
        Value::Struct(s) => walk_struct(s, path, t),
        Value::List(l) => l.iter().for_each(|x| walk(x, path, t)),
        _ => {}
    }
}

fn walk_struct(s: &Struct, path: &str, t: &mut Tally) {
    if s.int("interpolation") == Some(2) {
        group(s, path, t);
    }
    for (_, v) in &s.fields {
        walk(v, path, t);
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    let schema = Schema::load_dir(std::path::Path::new(&a[1])).unwrap();
    let mut t = Tally::default();
    let mut sets: BTreeMap<String, usize> = BTreeMap::new();
    let mut set_examples: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut special: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for arc in &a[2..] {
        let mut bsa = fnv_bsa::Archive::open(arc).unwrap();
        let entries: Vec<(String, fnv_bsa::FileRecord)> = bsa
            .entries()
            .map(|e| (e.path(), e.record.clone()))
            .filter(|(p, _)| p.ends_with(".nif") || p.ends_with(".kf"))
            .collect();
        for (path, rec) in entries {
            let Ok(data) = bsa.extract(&rec) else { continue };
            let Ok(file) = NifFile::parse(&data, Some(&schema)) else { continue };
            let dec = Decoded::new(&file, &data, &schema);
            for s in dec.blocks.iter().flatten() {
                walk_struct(s, &path, &mut t);
            }
            if path.ends_with(".nif") {
                let seqs = sequences(&file, &dec);
                for s in seqs.iter().filter(|s| s.name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case("specialidle"))) {
                    let only = seqs.len() == 1;
                    let key = format!("cycle {} {}", s.cycle_type, if only { "only sequence" } else { "with others" });
                    special.entry(key).or_default().push(path.clone());
                }
                let mut names: Vec<String> = seqs.iter().map(|s| s.name.clone().unwrap_or_default()).collect();
                if names.is_empty() {
                    continue;
                }
                names.sort();
                names.dedup();
                let key = names.join(", ");
                let ex = set_examples.entry(key.clone()).or_default();
                if ex.len() < 3 {
                    ex.push(path.clone());
                }
                *sets.entry(key).or_default() += 1;
            }
        }
    }
    println!("quadratic key groups with 3+ keys: {}", t.groups);
    println!("inner keys whose neighbouring segments differ: {}", t.keys);
    println!("  fit 'forward leaves the key':  {}", t.forward_leaves);
    println!("  fit 'forward arrives at the key': {}", t.forward_arrives);
    println!("  unclear (within 10%):          {}", t.unclear);
    for e in &t.examples {
        println!("    e.g. {e}");
    }
    println!("\nSpecialIdle sequences in meshes by cycle type:");
    for (k, v) in &special {
        println!("  {:6}  {k}", v.len());
        let mut dirs: BTreeMap<&str, usize> = BTreeMap::new();
        for p in v {
            *dirs.entry(p.rsplit_once('\\').map_or(p.as_str(), |x| x.0)).or_default() += 1;
        }
        let mut dv: Vec<_> = dirs.into_iter().collect();
        dv.sort_by_key(|a| std::cmp::Reverse(a.1));
        for (d, c) in dv.iter().take(12) {
            println!("          {c:4}  {d}");
        }
    }
    println!("\nsequence name sets in meshes:");
    let mut v: Vec<_> = sets.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    for (k, c) in v.iter().take(40) {
        println!("  {c:6}  {k}   e.g. {}", set_examples[*k].join(" | "));
    }
}
