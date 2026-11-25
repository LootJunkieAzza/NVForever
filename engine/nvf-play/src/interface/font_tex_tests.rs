//! Synthetic `.fnt` + `.tex`/`.dds` font loading controls via real VFS + `GameData`.
use super::*;
use bevy::render::render_resource::TextureFormat;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub(crate) fn synthetic_fnt(texture_name: &str) -> Vec<u8> {
    synthetic_fnt_sized(texture_name, 31.0, 10.0, 18.0)
}

pub(crate) fn synthetic_fnt_sized(
    texture_name: &str,
    size: f32,
    glyph_width: f32,
    glyph_ascent: f32,
) -> Vec<u8> {
    let mut bytes = vec![0u8; fnv_font::FILE_SIZE];
    bytes[0..4].copy_from_slice(&size.to_le_bytes());
    bytes[4..8].copy_from_slice(&1u32.to_le_bytes());
    bytes[8..12].copy_from_slice(&1u32.to_le_bytes());
    let name = texture_name.as_bytes();
    assert!(name.len() < 284);
    bytes[12..12 + name.len()].copy_from_slice(name);
    // Give 'A' a valid drawn glyph box.
    let at = 12 + 284 + (b'A' as usize) * 56;
    for (k, v) in [
        (1usize, 0.0f32),
        (2, 0.0),
        (3, 0.5),
        (4, 0.0),
        (5, 0.0),
        (6, 0.5),
        (7, 0.5),
        (8, 0.5),
        (9, glyph_width),
        (10, glyph_ascent + 2.0),
        (12, 1.0),
        (13, glyph_ascent),
    ] {
        bytes[at + 4 * k..at + 4 * k + 4].copy_from_slice(&v.to_le_bytes());
    }
    bytes
}

pub(crate) fn synthetic_tex(width: u32, height: u32, pixel: [u8; 4]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(8 + width as usize * height as usize * 4);
    bytes.extend_from_slice(&width.to_le_bytes());
    bytes.extend_from_slice(&height.to_le_bytes());
    for _ in 0..(width as usize * height as usize) {
        bytes.extend_from_slice(&pixel);
    }
    bytes
}

fn synthetic_rgba_dds(width: u32, height: u32, rgba: [u8; 4]) -> Vec<u8> {
    let mut bytes = tests::authored_rgba_dds(width, height);
    // authored_rgba_dds uses R=0xff0000, G=0x00ff00, B=0x0000ff, A=0xff000000 -> BGRA in LE.
    let bgra = [rgba[2], rgba[1], rgba[0], rgba[3]];
    for px in bytes[128..].as_chunks_mut::<4>().0 {
        *px = bgra;
    }
    bytes
}

pub(crate) struct OwnedFontFiles {
    root: PathBuf,
}

impl OwnedFontFiles {
    pub(crate) fn new(entries: &[(&str, Vec<u8>)]) -> Self {
        Self::with_extra_files(entries, &[])
    }

    pub(crate) fn with_extra_files(
        entries: &[(&str, Vec<u8>)],
        extra_files: &[(&str, Vec<u8>)],
    ) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "nvf-owned-font-tex-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let fonts_dir = root.join("textures").join("fonts");
        std::fs::create_dir_all(&fonts_dir).unwrap();
        for (name, bytes) in entries {
            assert!(!name.contains(['/', '\\']) && !name.is_empty());
            let path = fonts_dir.join(name);
            std::fs::write(&path, bytes).unwrap();
        }
        for (rel, bytes) in extra_files {
            assert!(!rel.is_empty() && !rel.starts_with(['/', '\\']));
            let mut path = root.clone();
            for part in rel.split(['/', '\\']) {
                assert!(!part.is_empty() && part != "." && part != "..");
                path.push(part);
            }
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&path, bytes).unwrap();
        }
        Self { root }
    }

    pub(crate) fn data(&self) -> GameData {
        let vfs = fnv_vfs::Vfs::open(&self.root, &[]).unwrap();
        let schema_root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif");
        let schema = fnv_nif::Schema::load_dir(&schema_root).unwrap();
        GameData::new(Arc::new(vfs), Arc::new(schema))
    }
}

impl Drop for OwnedFontFiles {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn fnt_with_dds_only_loads_dds() {
    let fnt_bytes = synthetic_fnt("Dds_Only_Font");
    let dds_bytes = synthetic_rgba_dds(2, 2, [200, 100, 50, 255]);
    let files = OwnedFontFiles::new(&[
        ("dds_only.fnt", fnt_bytes.clone()),
        ("dds_only_font.dds", dds_bytes),
    ]);
    let data = files.data();
    let font = Font::parse(&fnt_bytes).unwrap();
    assert!(font_tex(&font, &data).is_none());

    let mut images = Assets::<Image>::default();
    let loaded = face(
        "textures\\fonts\\dds_only.fnt",
        plain_font(20.0, 10.0),
        Some(&data),
        &mut images,
        true,
    );
    let (handle, size) = loaded.image.expect("DDS-only font should load");
    assert_eq!(size, Vec2::new(2.0, 2.0));
    assert_eq!(loaded.font.texture, "Dds_Only_Font");
    let img = images.get(&handle).unwrap();
    assert_eq!(img.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
    assert_eq!(&img.data.as_deref().unwrap()[..4], &[200, 100, 50, 255]);
}

#[test]
fn fnt_with_tex_or_mixed_case_tex_only_loads_without_dds() {
    for (fnt_name, tex_stem, tex_file, pixel) in [
        ("lower_tex.fnt", "Lower_Tex_Font", "lower_tex_font.tex", [10, 20, 30, 220]),
        ("mixed_tex.fnt", "Mixed_Tex_Font", "Mixed_Tex_Font.Tex", [40, 80, 120, 240]),
    ] {
        let fnt_bytes = synthetic_fnt(tex_stem);
        // Non-square atlas dimensions mirror modded 512x1024 `.Tex` files.
        let tex_bytes = synthetic_tex(4, 2, pixel);
        let files = OwnedFontFiles::new(&[(fnt_name, fnt_bytes.clone()), (tex_file, tex_bytes)]);
        let data = files.data();
        let font = Font::parse(&fnt_bytes).unwrap();
        assert!(!data.vfs.exists(&font.texture_path()));

        let parsed = font_tex(&font, &data).expect(".tex should load without a sibling .dds");
        assert_eq!((parsed.width, parsed.height), (4, 2));
        assert_eq!(&parsed.rgba[..4], &pixel);

        let mut images = Assets::<Image>::default();
        let loaded = face(
            &format!("textures\\fonts\\{fnt_name}"),
            plain_font(20.0, 10.0),
            Some(&data),
            &mut images,
            true,
        );
        let (handle, size) = loaded.image.expect("tex-only font should produce a face image");
        assert_eq!(size, Vec2::new(4.0, 2.0));
        assert_eq!(loaded.font.texture, tex_stem);
        let img = images.get(&handle).unwrap();
        assert_eq!(img.texture_descriptor.format, TextureFormat::Rgba8UnormSrgb);
        assert_eq!(&img.data.as_deref().unwrap()[..4], &pixel);
    }
}

#[test]
fn fnt_with_both_tex_and_dds_prefers_tex_and_falls_back_on_broken_tex() {
    // Valid .tex + valid .dds -> preserves .tex-before-.dds precedence (fonts.md §2.2).
    let fnt_bytes = synthetic_fnt("Both_Font");
    let tex_bytes = synthetic_tex(4, 2, [11, 22, 33, 255]);
    let dds_bytes = synthetic_rgba_dds(2, 2, [200, 100, 50, 255]);
    let files = OwnedFontFiles::new(&[
        ("both.fnt", fnt_bytes.clone()),
        ("Both_Font.Tex", tex_bytes),
        ("both_font.dds", dds_bytes.clone()),
    ]);
    let data = files.data();
    let font = Font::parse(&fnt_bytes).unwrap();
    let parsed = font_tex(&font, &data).expect(".tex wins when both exist");
    assert_eq!((parsed.width, parsed.height), (4, 2));

    let mut images = Assets::<Image>::default();
    let loaded = face(
        "textures\\fonts\\both.fnt",
        plain_font(20.0, 10.0),
        Some(&data),
        &mut images,
        true,
    );
    let (handle, size) = loaded.image.expect("face should load .tex");
    assert_eq!(size, Vec2::new(4.0, 2.0));
    assert_eq!(
        &images.get(&handle).unwrap().data.as_deref().unwrap()[..4],
        &[11, 22, 33, 255]
    );

    // Corrupt .tex + valid .dds -> warns and falls back to .dds.
    let broken_files = OwnedFontFiles::new(&[
        ("both.fnt", fnt_bytes.clone()),
        ("both_font.tex", vec![0u8; 5]),
        ("both_font.dds", dds_bytes),
    ]);
    let broken_data = broken_files.data();
    assert!(font_tex(&font, &broken_data).is_none());
    let mut fallback_images = Assets::<Image>::default();
    let fallback = face(
        "textures\\fonts\\both.fnt",
        plain_font(20.0, 10.0),
        Some(&broken_data),
        &mut fallback_images,
        true,
    );
    let (fb_handle, fb_size) = fallback.image.expect("broken .tex should fall back to .dds");
    assert_eq!(fb_size, Vec2::new(2.0, 2.0));
    assert_eq!(
        &fallback_images
            .get(&fb_handle)
            .unwrap()
            .data
            .as_deref()
            .unwrap()[..4],
        &[200, 100, 50, 255]
    );
}

#[test]
fn fnt_without_any_texture_or_with_only_broken_tex_returns_stand_in() {
    let fnt_bytes = synthetic_fnt("Missing_Atlas");
    let stand_in = plain_font(20.0, 10.0);

    for entries in [
        vec![("missing.fnt", fnt_bytes.clone())],
        vec![("missing.fnt", fnt_bytes.clone()), ("missing_atlas.tex", vec![1u8; 4])],
    ] {
        let files = OwnedFontFiles::new(&entries);
        let data = files.data();
        let font = Font::parse(&fnt_bytes).unwrap();
        assert!(font_tex(&font, &data).is_none());

        let mut images = Assets::<Image>::default();
        let loaded = face(
            "textures\\fonts\\missing.fnt",
            stand_in.clone(),
            Some(&data),
            &mut images,
            true,
        );
        assert!(loaded.image.is_none());
        assert_eq!(loaded.font, stand_in);
        assert_eq!(images.len(), 0);
    }
}

#[test]
fn face_keeps_font_9_unloaded_even_with_tex_and_dds() {
    let fnt_bytes = synthetic_fnt("NVFont_Test_0_Lod_A");
    let tex_bytes = synthetic_tex(4, 4, [255, 255, 255, 255]);
    let dds_bytes = synthetic_rgba_dds(2, 2, [200, 100, 50, 255]);
    let files = OwnedFontFiles::new(&[
        ("nvfont_test.fnt", fnt_bytes),
        ("nvfont_test_0_lod_a.tex", tex_bytes),
        ("nvfont_test_0_lod_a.dds", dds_bytes),
    ]);
    let data = files.data();
    let stand_in = plain_font(31.0, 15.0);

    for path in [
        FONT_FILES[8],
        "textures/fonts/NVFont_Test.fnt",
        "TEXTURES\\FONTS\\NVFONT_TEST.FNT",
    ] {
        let mut images = Assets::<Image>::default();
        let loaded = face(path, stand_in.clone(), Some(&data), &mut images, true);
        assert!(loaded.image.is_none(), "font 9 must stay unloaded for {path}");
        assert_eq!(loaded.font, stand_in);
        assert_eq!(images.len(), 0);
    }
}

#[test]
fn face_loads_tex_only_fonts_for_slots_1_to_8_and_skips_slot_9() {
    let mut entries = Vec::new();
    for (idx, path) in FONT_FILES.iter().enumerate() {
        let fnt_name = path.rsplit('\\').next().unwrap();
        let stem = format!("Slot_{}_Atlas", idx + 1);
        entries.push((fnt_name, synthetic_fnt(&stem)));
    }
    let tex_names: Vec<String> = (1..=9).map(|i| format!("Slot_{i}_Atlas.Tex")).collect();
    let mut all_entries: Vec<(&str, Vec<u8>)> = entries;
    for (idx, tex_name) in tex_names.iter().enumerate() {
        all_entries.push((tex_name.as_str(), synthetic_tex(4, 2, [idx as u8 + 10, 40, 80, 255])));
    }
    let files = OwnedFontFiles::new(&all_entries);
    let data = files.data();
    let stand_in = plain_font(31.0, 15.0);
    let mut images = Assets::<Image>::default();

    for (idx, path) in FONT_FILES.iter().enumerate() {
        let loaded = face(path, stand_in.clone(), Some(&data), &mut images, true);
        if idx < 8 {
            let (_, size) = loaded
                .image
                .unwrap_or_else(|| panic!("slot {} ({path}) should load .Tex without .dds", idx + 1));
            assert_eq!(size, Vec2::new(4.0, 2.0));
        } else {
            assert!(loaded.image.is_none(), "slot 9 ({path}) must stay unloaded");
            assert_eq!(loaded.font, stand_in);
        }
    }
    assert_eq!(images.len(), 8);
}

#[test]
fn xfonts_vfs_loads_mapmo_slots_21_and_22_tex_only_while_keeping_vanilla_unchanged() {
    let vanilla = OwnedFontFiles::new(&[]);
    assert!(load_xfonts(&vanilla.data().vfs).is_empty(), "vanilla without xfonts yields empty map");

    let mapmo_txt = b"; MAPMO extended fonts\n\
        1=textures\\fonts\\ignored_vanilla.fnt\n\
        9=textures\\fonts\\ignored_nine.fnt\n\
        21=textures\\fonts\\mapmo_hud_caps_font.fnt\n\
        22=textures\\fonts\\mapmo_hud_caps_font2.fnt\n";
    let files = OwnedFontFiles::with_extra_files(
        &[
            ("glow_monofonto_large.fnt", synthetic_fnt("Glow_Monofonto_Large")),
            ("Glow_Monofonto_Large.Tex", synthetic_tex(4, 2, [255, 255, 255, 255])),
            ("nvfont_test.fnt", synthetic_fnt("NVFont_Test_0_Lod_A")),
            ("nvfont_test_0_lod_a.tex", synthetic_tex(4, 4, [255, 255, 255, 255])),
            ("mapmo_hud_caps_font.fnt", synthetic_fnt_sized("mapmo_hud_caps_font", 76.7, 24.0, 58.0)),
            ("mapmo_hud_caps_font.Tex", synthetic_tex(8, 4, [20, 120, 220, 255])),
            ("mapmo_hud_caps_font2.fnt", synthetic_fnt_sized("mapmo_hud_caps_font2", 76.7, 28.0, 60.0)),
            ("mapmo_hud_caps_font2.Tex", synthetic_tex(8, 4, [40, 140, 240, 255])),
        ],
        &[("NVSE/Plugins/xfonts/MAPMO.txt", mapmo_txt.to_vec())],
    );
    let data = files.data();
    let extra = load_xfonts(&data.vfs);
    assert_eq!(extra.len(), 2);
    assert_eq!(extra.get(&21).map(String::as_str), Some("textures\\fonts\\mapmo_hud_caps_font.fnt"));
    assert_eq!(extra.get(&22).map(String::as_str), Some("textures\\fonts\\mapmo_hud_caps_font2.fnt"));

    let mut images = Assets::<Image>::default();
    let stand_in = plain_font(31.0, 15.0);

    let f21_path = slot_font_path(21, &extra).expect("slot 21 path");
    let f21 = face(&f21_path, stand_in.clone(), Some(&data), &mut images, true);
    let (_, s21) = f21.image.expect("slot 21 .Tex should load without .dds");
    assert_eq!(s21, Vec2::new(8.0, 4.0));
    assert!((f21.font.size - 76.7).abs() < 1e-4);

    let f22_path = slot_font_path(22, &extra).expect("slot 22 path");
    let f22 = face(&f22_path, stand_in.clone(), Some(&data), &mut images, true);
    let (_, s22) = f22.image.expect("slot 22 .Tex should load without .dds");
    assert_eq!(s22, Vec2::new(8.0, 4.0));

    // Vanilla slot 1 keeps its vanilla path; slot 9 stays unloaded; unregistered 42 has no path.
    let f1_path = slot_font_path(1, &extra).expect("slot 1 path");
    assert_eq!(f1_path, FONT_FILES[0]);
    assert!(face(&f1_path, stand_in.clone(), Some(&data), &mut images, true).image.is_some());

    let f9_path = slot_font_path(9, &extra).expect("slot 9 path");
    assert_eq!(f9_path, FONT_FILES[8]);
    assert!(face(&f9_path, stand_in, Some(&data), &mut images, true).image.is_none());
    assert_eq!(slot_font_path(42, &extra), None);
}
