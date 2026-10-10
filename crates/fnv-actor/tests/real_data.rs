//! Checks against a real install (read-only). They run only when `NVFOREVER_DATA` or
//! `FNVLAB_DATA_DIR` names the game's `Data` folder, and pass as skipped otherwise:
//!
//! ```text
//! NVFOREVER_DATA="<...>/Fallout New Vegas/Data" cargo test --release -p fnv-actor --test real_data -- --nocapture
//! ```
//!
//! The numbers are the ones docs/spec-notes/lipsync.md §4 quotes as observed on the
//! owner's install (FNV 1.4.0.525, Steam, English, all add-ons).

use fnv_actor::tri::{label_aliases, Tri};
use fnv_vfs::Vfs;
use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    let dir = ["NVFOREVER_DATA", "FNVLAB_DATA_DIR"].iter().find_map(std::env::var_os).map(PathBuf::from)?;
    dir.join("FalloutNV.esm").is_file().then_some(dir)
}

/// The sixteen mouth shapes, as the head files label them.
const PHONEMES: [&str; 16] = ["Aah", "BigAah", "BMP", "ChJSh", "DST", "Ee", "Eh", "FV", "I", "K", "N", "Oh", "OohQ", "R", "Th", "W"];

#[test]
fn every_tri_file_parses_and_the_heads_carry_the_mouth_shapes() {
    let Some(dir) = data_dir() else {
        eprintln!("every_tri_file_parses_and_the_heads_carry_the_mouth_shapes: skipped (no game data: set NVFOREVER_DATA to the game's Data folder)");
        return;
    };
    let vfs = Vfs::open_default(&dir).unwrap();
    let mut paths = vfs.paths(|p| p.ends_with(".tri"));
    paths.sort();
    assert_eq!(paths.len(), 49);
    let mut heads = 0;
    for p in &paths {
        let tri = Tri::parse(&vfs.read(p).unwrap().unwrap()).unwrap_or_else(|e| panic!("{p}: {e}"));
        assert_eq!(tri.residue, 0, "{p}");
        assert_eq!(tri.quads, 0, "{p}");
        let has = |name: &str| label_aliases(name).iter().any(|l| tri.morph(l).is_some() || tri.modifier(l).is_some());
        let file = p.rsplit('\\').next().unwrap();
        if file.starts_with("head") {
            heads += 1;
            assert_eq!(tri.vertices, 1211, "{p}");
            assert_eq!((tri.morphs.len(), tri.modifiers.len()), (38, 8), "{p}");
            for name in PHONEMES {
                assert!(tri.morph(name).is_some(), "{p}: no {name}");
            }
            for name in ["BlinkLeft", "BlinkRight", "SquintLeft", "SquintRight", "LookDown", "LookLeft", "LookRight", "LookUp"] {
                assert!(tri.modifier(name).is_some(), "{p}: no {name}");
            }
            for name in ["BrowDownLeft", "BrowDownRight", "BrowInLeft", "BrowInRight", "BrowUpLeft", "BrowUpRight"] {
                assert!(tri.morph(name).is_some(), "{p}: no {name}");
            }
            // A blink lists 20 vertices of each lid and moves them by under a unit and a
            // half (the ghoul heads move only some of them).
            assert_eq!(tri.modifier("BlinkLeft").unwrap().indices.len(), 20, "{p}");
            let blink = tri.displacements("BlinkLeft", None).unwrap();
            assert!(!blink.is_empty() && blink.len() <= 20, "{p}");
            assert!(blink.iter().all(|(_, d)| d.iter().all(|c| c.abs() < 1.5)), "{p}");
        }
        if file.starts_with("mouth") {
            assert_eq!(tri.labels(), vec!["Aah", "Anger", "BigAah", "Surprise", "W"], "{p}");
        }
        if file.starts_with("teethupper") {
            assert!(tri.labels().is_empty(), "{p}");
        }
        if file.starts_with("eyeleft") || file.starts_with("eyeright") {
            assert_eq!(tri.labels(), vec!["LookDown", "LookLeft", "LookRight", "LookUp"], "{p}");
            assert!(!has("BlinkLeft"));
        }
    }
    assert_eq!(heads, 8);
}
