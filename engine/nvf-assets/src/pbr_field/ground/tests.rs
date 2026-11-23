//! The ground pack test aid (#3322). Every file is generated here, in a folder of the test's own.

use super::*;
use crate::{GameData, TextureCache};
use bevy::prelude::*;
use bevy::render::render_resource::TextureFormat;
use std::fs;
use std::sync::Arc;

fn temp_home(tag: &str) -> PathBuf {
    std::env::temp_dir().join(format!("nvf-ground-pack-{tag}-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()))
}

#[test]
fn the_generated_height_map_is_a_valid_bc4_chain_with_the_full_range() {
    let bytes = ground_height_dds();
    assert_eq!(bytes, ground_height_dds(), "deterministic");
    let dds = fnv_dds::Dds::parse(&bytes).expect("a DDS file the engine reads");
    assert_eq!(dds.format, fnv_dds::Format::Bc4U, "unsigned BC4, as the pack rules ask of a _p");
    assert_eq!((dds.header.width, dds.header.height), (GROUND_SIDE, GROUND_SIDE));
    assert_eq!(GROUND_SIDE % 4, 0, "sides on the 4-texel grid");
    assert_eq!(dds.header.levels(), GROUND_SIDE.ilog2() + 1, "a full mip chain down to 1x1");
    assert_eq!(dds.levels[0].len(), dds.header.levels() as usize);
    let top = fnv_dds::decode_bc4(dds.format, GROUND_SIDE, GROUND_SIDE, dds.levels[0][0]).expect("level 0 decodes");
    let (min, max) = top.iter().fold((f32::MAX, f32::MIN), |(a, b), &v| (a.min(v), b.max(v)));
    assert_eq!((min, max), (0.0, 1.0), "the full 0..1 range");
    let low = top.iter().filter(|&&v| v == 0.0).count() as f32 / top.len() as f32;
    // An eighth of each direction raised: (7/8)^2 of the map low.
    assert!((low - 49.0 / 64.0).abs() < 1e-6, "mostly low, so a layer sinks as one: {low}");
    // The pattern by texel: the ridge rows and columns, every 64 texels, 8 wide.
    let at = |x: usize, y: usize| top[y * GROUND_SIDE as usize + x];
    assert_eq!((at(0, 30), at(7, 30), at(8, 30), at(30, 64), at(30, 72)), (1.0, 1.0, 0.0, 1.0, 0.0));
    // The last level is one value, the map's mean (15/64 raised), rounded to a byte.
    let last = dds.levels[0].last().unwrap();
    assert_eq!(last[0], (255.0f32 * 15.0 / 64.0).round() as u8);
}

#[test]
fn every_landscape_texture_gets_a_height_only_set_where_the_index_looks() {
    let diffuses = ["Landscape\\Dirt01.dds", "landscape/dirt01.dds", "data\\textures\\landscape\\Rocks02.dds", "landscape\\xbox\\Sand.dds", ""];
    let files = ground_fixture(&diffuses);
    assert_eq!(files, ground_fixture(&diffuses), "deterministic");
    let names: Vec<&str> = files.iter().map(|(p, _)| p.as_str()).collect();
    assert_eq!(
        names,
        [
            "textures/pbr/landscape/dirt01_p.dds",
            "textures/pbr/landscape/dirt01.toml",
            "textures/pbr/landscape/pc/sand_p.dds",
            "textures/pbr/landscape/pc/sand.toml",
            "textures/pbr/landscape/rocks02_p.dds",
            "textures/pbr/landscape/rocks02.toml",
            "textures/pbr/landscape/xbox/sand_p.dds",
            "textures/pbr/landscape/xbox/sand.toml",
        ],
        "one set per texture (two spellings share one), the \\pc\\ twin of a console folder, nothing for an empty path"
    );
    // The index finds each as a height-only set by the diffuse's path.
    let index = fnv_pbr::PbrIndex::build(names.iter().copied());
    for d in ["textures\\landscape\\dirt01.dds", "textures\\landscape\\rocks02.dds", "textures\\landscape\\pc\\sand.dds"] {
        let found = index.lookup(d).unwrap_or_else(|| panic!("{d}"));
        assert_eq!(found.state(), fnv_pbr::SetState::HeightOnly, "{d}");
    }
    assert_eq!(index.counts().unmatched, 0, "every file is a set member");
    for (path, bytes) in &files {
        if path.ends_with(".toml") {
            let parsed = fnv_pbr::parse_scalars(bytes);
            assert!(!parsed.unreadable && parsed.warnings.is_empty(), "{path}: {:?}", parsed.warnings);
            assert_eq!(parsed.scalars.parallax_depth, Some(GROUND_DEPTH), "{path}");
        } else {
            assert_eq!(bytes, &ground_height_dds(), "{path}");
        }
    }
}

#[test]
fn the_terrain_lookup_takes_the_generated_height_map_at_the_set_depth() {
    // A load order of one diffuse, with the pack mounted over it as the game mounts it.
    let home = temp_home("lookup");
    let data_dir = home.join("Data");
    fs::create_dir_all(data_dir.join("textures").join("landscape")).unwrap();
    fs::write(data_dir.join("textures").join("landscape").join("dirt01.dds"), crate::authored::tests::legacy(b"DXT1", 4, 4, &[0xFF, 0xFF, 0, 0, 0, 0, 0, 0])).unwrap();
    let (pack, sets, written) = seed_ground_pack_in(&home, 3, 1, &["Landscape\\Dirt01.dds"]).unwrap();
    assert_eq!((sets, written), (1, 2));
    let mut vfs = fnv_vfs::Vfs::open(&data_dir, &[]).unwrap();
    vfs.add_loose_root(&pack);
    let data = GameData::new(Arc::new(vfs), Arc::new(fnv_nif::Schema::default()));
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let (height, depth) = cache.authored_height_with(true, "Landscape\\Dirt01.dds", &data, &mut images, None).expect("the generated set's _p");
    assert_eq!(depth, GROUND_DEPTH);
    let image = images.get(&height).unwrap();
    assert_eq!(image.texture_descriptor.format, TextureFormat::Bc4RUnorm);
    assert!(image.texture_descriptor.mip_level_count > 1, "the mip chain is uploaded");
    // Another texture has no set; with the setting off nothing is asked.
    assert!(cache.authored_height_with(true, "landscape\\other.dds", &data, &mut images, None).is_none());
    assert!(cache.authored_height_with(false, "Landscape\\Dirt01.dds", &data, &mut images, None).is_none());
    let _ = fs::remove_dir_all(&home);
}

/// The pack writes only into a fresh folder of its own: a second run gets another one, files
/// already in the home are never touched, and nothing is written without a texture or a home.
#[test]
fn the_ground_pack_writes_only_a_fresh_folder_of_its_own() {
    let home = temp_home("fresh");
    let mine = home.join("mods").join("textures").join("pbr").join("landscape");
    fs::create_dir_all(&mine).unwrap();
    fs::write(mine.join("dirt01_p.dds"), b"the player's own").unwrap();
    let (first, ..) = seed_ground_pack_in(&home, 7, 1, &["landscape\\dirt01.dds"]).unwrap();
    let (second, ..) = seed_ground_pack_in(&home, 7, 1, &["landscape\\dirt01.dds"]).unwrap();
    assert_ne!(first, second, "a new folder per run");
    assert!(first.starts_with(home.join("pbr-ground-pack")));
    for (rel, bytes) in ground_fixture(&["landscape\\dirt01.dds"]) {
        assert_eq!(fs::read(first.join(&rel)).unwrap(), bytes, "{rel}");
    }
    assert_eq!(fs::read(mine.join("dirt01_p.dds")).unwrap(), b"the player's own", "the home's files untouched");
    assert!(seed_ground_pack_in::<&str>(&home, 8, 1, &[]).is_err(), "no texture: nothing written");
    assert!(!home.join("pbr-ground-pack").join("run-8-1-0").exists());
    assert!(ground_pack_home(None).is_err() && ground_pack_home(Some("".into())).is_err());
    assert_eq!(ground_pack_home(Some("h".into())).unwrap(), PathBuf::from("h"));
    // A file in the way inside a fresh folder cannot happen, but write_files refuses one.
    let taken = home.join("taken");
    let files = ground_fixture(&["landscape\\dirt01.dds"]);
    fs::create_dir_all(taken.join("textures").join("pbr").join("landscape")).unwrap();
    fs::write(taken.join(&files[0].0), b"there before").unwrap();
    assert!(write_files(&taken, &files).is_err());
    assert_eq!(fs::read(taken.join(&files[0].0)).unwrap(), b"there before", "never truncated");
    let _ = fs::remove_dir_all(&home);
}

/// A plugin's texture path is data: one with parent steps, a dot component or a colon gets no
/// set, so nothing is written outside the run's own folder, and the writer refuses any such
/// path before its first write.
#[test]
fn no_texture_path_can_write_outside_the_pack_folder() {
    let hostile = [r"..\..\..\escape.dds", r"landscape\..\..\..\..\escape.dds", r"landscape\.\dot.dds", r"landscape\c:\stream.dds", r"landscape\xbox\..\..\..\escape.dds"];
    for d in hostile {
        assert!(ground_fixture(&[d]).is_empty(), "{d}: {:?}", ground_fixture(&[d]).iter().map(|f| &f.0).collect::<Vec<_>>());
    }
    // Mixed with a plain texture, only the plain one is written.
    let mixed = ground_fixture(&[r"..\..\escape.dds", r"landscape\dirt01.dds"]);
    assert_eq!(mixed.iter().map(|f| f.0.as_str()).collect::<Vec<_>>(), ["textures/pbr/landscape/dirt01_p.dds", "textures/pbr/landscape/dirt01.toml"]);
    let home = temp_home("contain");
    // The home exists, as the game's NVFOREVER_HOME does (the pack makes only its own folder).
    fs::create_dir_all(&home).unwrap();
    let (dir, sets, _) = seed_ground_pack_in(&home, 9, 1, &[r"..\..\..\..\..\escape.dds", r"landscape\dirt01.dds"]).unwrap();
    assert_eq!(sets, 1);
    let written: Vec<PathBuf> = walk(&home);
    assert!(written.iter().all(|p| p.starts_with(&dir)), "every file inside the run's folder: {written:?}");
    assert!(seed_ground_pack_in(&home, 10, 1, &[r"..\..\escape.dds"]).is_err(), "nothing safe to write: refused");
    // The writer's own check, before any write.
    let out = home.join("guarded");
    fs::create_dir_all(&out).unwrap();
    for bad in ["../escape_p.dds", "textures/../../escape_p.dds", "/abs_p.dds", "c:/drive_p.dds", "textures//empty_p.dds", "./dot_p.dds"] {
        let files = vec![("textures/pbr/ok_p.dds".to_string(), b"x".to_vec()), (bad.to_string(), b"x".to_vec())];
        assert!(write_files(&out, &files).is_err(), "{bad}");
        assert!(!out.join("textures").exists(), "{bad}: nothing written before the refusal");
    }
    assert!(!home.parent().unwrap().join("escape_p.dds").exists());
    let _ = fs::remove_dir_all(&home);
}

/// Every file under `dir`.
fn walk(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    for e in fs::read_dir(dir).unwrap().flatten() {
        let p = e.path();
        if p.is_dir() {
            out.extend(walk(&p));
        } else {
            out.push(p);
        }
    }
    out
}
