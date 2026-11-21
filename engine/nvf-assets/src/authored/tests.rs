//! Authored set lookup (#2981). Every file is generated here, in a folder of the test's own.

use super::*;
use crate::nif_spawn::{StoreTextures, TextureSource};
use crate::texture::TextureCache;
use bevy::render::render_resource::TextureFormat;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

/// A legacy DDS file (FourCC `tag`) of one level.
pub(crate) fn legacy(tag: &[u8; 4], w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = vec![0u8; 128];
    b[..4].copy_from_slice(b"DDS ");
    for (at, v) in [(4, 124u32), (8, 0x1007), (12, h), (16, w), (28, 1), (76, 32), (80, 4), (108, 0x1000)] {
        b[at..at + 4].copy_from_slice(&v.to_le_bytes());
    }
    b[84..88].copy_from_slice(tag);
    b.extend_from_slice(payload);
    b
}

/// A DX10 DDS file (DXGI code `dxgi`, 2D, one array element) of one level.
pub(crate) fn dx10(dxgi: u32, w: u32, h: u32, payload: &[u8]) -> Vec<u8> {
    let mut b = legacy(b"DX10", w, h, &[]);
    for v in [dxgi, 3, 0, 1, 0] {
        b.extend_from_slice(&v.to_le_bytes());
    }
    b.extend_from_slice(payload);
    b
}

/// A load order of generated files with authored sets (module notes of the cases below).
pub(crate) struct Pack {
    root: PathBuf,
    pub(crate) data: GameData,
}

impl Pack {
    pub(crate) fn new() -> Pack {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("nvf-authored-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        let dxt1 = legacy(b"DXT1", 4, 4, &[0xFF, 0xFF, 0, 0, 0, 0, 0, 0]);
        let files: Vec<(&str, Vec<u8>)> = vec![
            // A full set: an 8x8 BC7 `_orm` whose header says sRGB, an 8x4 BC5 `_n`, scalars.
            ("textures/walls/brick.dds", dxt1.clone()),
            ("textures/pbr/walls/brick_orm.dds", dx10(99, 8, 8, &[0; 64])),
            ("textures/pbr/walls/brick_n.dds", legacy(b"ATI2", 8, 4, &[0; 32])),
            ("textures/pbr/walls/brick.toml", b"roughness_scale = 0.5\nreflectance = 0.25\n".to_vec()),
            // `_orm` alone.
            ("textures/walls/nonormal.dds", dxt1.clone()),
            ("textures/pbr/walls/nonormal_orm.dds", dx10(98, 4, 4, &[0; 16])),
            // A set with an `_albedo` (BC7, read as sRGB colour) and no scalar file.
            ("textures/walls/painted.dds", dxt1.clone()),
            ("textures/pbr/walls/painted_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/painted_albedo.dds", dx10(98, 4, 4, &[0; 16])),
            // An `_albedo` that is no DDS file: the game's diffuse stays.
            ("textures/walls/smudged.dds", dxt1.clone()),
            ("textures/pbr/walls/smudged_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/smudged_albedo.dds", b"not a dds".to_vec()),
            // An `_albedo` without `_orm`: no set.
            ("textures/walls/lonely.dds", dxt1.clone()),
            ("textures/pbr/walls/lonely_albedo.dds", dx10(98, 4, 4, &[0; 16])),
            // A set whose scalar file sets its metal gain.
            ("textures/walls/steel.dds", dxt1.clone()),
            ("textures/pbr/walls/steel_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/steel.toml", b"metal_gain = 3\n".to_vec()),
            // Height only: no authored set.
            ("textures/walls/stone.dds", dxt1.clone()),
            ("textures/pbr/walls/stone_p.dds", legacy(b"BC4U", 4, 4, &[0; 8])),
            // A first-person twin whose own set is height only, and the plain texture's set.
            ("textures/weapons/gun.dds", dxt1.clone()),
            ("textures/weapons/1stpersongun.dds", dxt1.clone()),
            ("textures/pbr/weapons/gun_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/weapons/1stpersongun_p.dds", legacy(b"BC4U", 4, 4, &[0; 8])),
            // An `_orm` that is no DDS file.
            ("textures/walls/bad.dds", dxt1.clone()),
            ("textures/pbr/walls/bad_orm.dds", b"not a dds".to_vec()),
            // A console folder: the member under `\xbox\` and another set's under `\pc\`.
            ("textures/walls/xbox/tile.dds", dxt1.clone()),
            ("textures/pbr/walls/xbox/tile_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/pc/tile_orm.dds", dxt1.clone()),
            // No set at all.
            ("textures/walls/plain.dds", dxt1),
        ];
        for (rel, bytes) in files {
            let p = root.join(rel);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, bytes).unwrap();
        }
        let data = GameData::new(Arc::new(fnv_vfs::Vfs::open(&root, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
        Pack { root, data }
    }
}

impl Drop for Pack {
    fn drop(&mut self) {
        // The folder is this test's own (a fresh name under the temp folder).
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn the_synchronous_source_loads_a_set_by_its_exact_member_paths() {
    let pack = Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let mut source = StoreTextures { textures: &mut cache, data: &pack.data, images: &mut images, formats: None };
    let key = source.resolve("Walls/Brick.dds");
    assert_eq!(key, r"textures\walls\brick.dds");
    let set = source.authored(&key).expect("authored set");
    assert_eq!(set.stem, r"textures\pbr\walls\brick");
    assert_eq!(set.scalars, Scalars { parallax_depth: None, roughness_scale: Some(0.5), reflectance: Some(0.25), metal_gain: None });
    if std::env::var_os(METAL_GAIN_ENV).is_none() {
        assert_eq!(set.metal_gain, fnv_pbr::DEFAULT_METAL_GAIN, "no _albedo, no metal_gain: the default for the game's diffuse");
    }
    let normal = set.normal.clone().expect("_n");
    let none = source.authored(&source.resolve("walls/nonormal.dds")).expect("set without _n");
    assert!(none.normal.is_none());
    assert_eq!(none.scalars, Scalars::default());
    assert!(source.authored(&source.resolve("walls/stone.dds")).is_none(), "height only is no authored set");
    assert!(source.authored(&source.resolve("walls/plain.dds")).is_none());
    assert!(source.authored(&source.resolve("walls/bad.dds")).is_none(), "an _orm that does not load leaves the shape legacy");
    assert!(source.authored(&source.resolve("weapons/1stpersongun.dds")).is_none(), "a height-only twin wins the lookup: no authored set, not the plain one's");
    assert_eq!(source.authored(&source.resolve("weapons/gun.dds")).map(|s| s.stem).as_deref(), Some(r"textures\pbr\weapons\gun"));
    // The console folder: the member the index names, not the PC file `get` would choose.
    let xbox_key = source.resolve(r"walls\xbox\tile.dds");
    assert_eq!(xbox_key, r"textures\walls\xbox\tile.dds", "no PC diffuse: the key stays");
    let xbox = source.authored(&xbox_key).expect("xbox set");
    let resolved = source.texture(r"textures\pbr\walls\xbox\tile_orm.dds", false).unwrap();
    // Every member is in place and in its role.
    let (orm, xbox_orm) = (set.orm.clone(), xbox.orm.clone());
    let warned = cache.workers.authored.warned.lock().unwrap().clone();
    assert_eq!(images.get(&orm).unwrap().texture_descriptor.format, TextureFormat::Bc7RgbaUnorm, "linear, whatever the header says");
    assert_eq!(images.get(&orm).unwrap().size(), UVec2::new(8, 8));
    assert_eq!(images.get(&normal).unwrap().texture_descriptor.format, TextureFormat::Bc5RgUnorm);
    assert_eq!(images.get(&normal).unwrap().size(), UVec2::new(8, 4));
    assert_eq!(images.get(&xbox_orm).unwrap().texture_descriptor.format, TextureFormat::Bc7RgbaUnorm, "the \\xbox\\ member, exactly");
    assert_eq!(images.get(&resolved).unwrap().texture_descriptor.format, TextureFormat::Bc1RgbaUnorm, "the resolving load picks the PC file");
    assert!(warned.contains(r"textures\pbr\walls\brick_orm.dds"), "the _SRGB code is warned about: {warned:?}");
    assert!(warned.contains(r"textures\pbr\walls\bad_orm.dds"), "{warned:?}");
    assert!(!warned.contains(r"textures\pbr\walls\nonormal_orm.dds"));
}

#[test]
fn off_asks_nothing_of_the_index() {
    let pack = Pack::new();
    let memo = Memo::default();
    assert!(find(&pack.data, false, r"textures\walls\brick.dds", &memo, |_, _| panic!("no file is loaded when off")).is_none());
    assert!(!pack.data.pbr_index_built(), "the index is never built while the setting is off");
    assert!(find(&pack.data, true, r"textures\walls\plain.dds", &memo, |_, _| panic!("no set")).is_none());
    assert!(pack.data.pbr_index_built());
}

#[test]
fn a_scalar_file_is_parsed_once_per_file() {
    let cache = ScalarCache::default();
    let reads = AtomicU64::new(0);
    let read = || {
        reads.fetch_add(1, Ordering::Relaxed);
        Some(b"reflectance = 0.1\nbogus = 1\n".to_vec())
    };
    let a = scalars_cached(&cache, "textures/pbr/a.toml", read);
    let b = scalars_cached(&cache, "textures/pbr/a.toml", || panic!("read again"));
    assert_eq!((a, a.reflectance), (b, Some(0.1)));
    assert_eq!(reads.load(Ordering::Relaxed), 1);
    assert_eq!(scalars_cached(&cache, "textures/pbr/gone.toml", || None), Scalars::default());
}

/// Workers asking for the same file at once: one reads (and warns), the rest wait for it.
#[test]
fn simultaneous_first_reads_of_a_scalar_file_read_it_once() {
    let cache = ScalarCache::default();
    let reads = AtomicU64::new(0);
    let start = std::sync::Barrier::new(8);
    let got: Vec<Scalars> = std::thread::scope(|scope| {
        let workers: Vec<_> = (0..8)
            .map(|_| {
                scope.spawn(|| {
                    start.wait();
                    scalars_cached(&cache, "textures/pbr/walls/brick.toml", || {
                        reads.fetch_add(1, Ordering::Relaxed);
                        std::thread::sleep(std::time::Duration::from_millis(50));
                        Some(b"roughness_scale = 0.5\nunknown = 2\n".to_vec())
                    })
                })
            })
            .collect();
        workers.into_iter().map(|w| w.join().unwrap()).collect()
    });
    assert_eq!(reads.load(Ordering::Relaxed), 1, "one read and one set of warnings");
    assert!(got.iter().all(|s| s.roughness_scale == Some(0.5)));
}

/// The scalar files belong to the mount: another VFS mounted in place reads them afresh.
#[test]
fn scalars_are_read_per_mount() {
    let pack = Pack::new();
    let path = r"textures\pbr\walls\brick.toml";
    assert_eq!(pack.data.pbr_scalars(path).roughness_scale, Some(0.5));
    let other = Pack::new();
    let mut moved = pack.data.clone();
    moved.set_vfs(other.data.vfs.clone());
    std::fs::write(other.root.join("textures/pbr/walls/brick.toml"), b"roughness_scale = 0.25\n").unwrap();
    assert_eq!(moved.pbr_scalars(path).roughness_scale, Some(0.25), "not the old mount's value");
    assert_eq!(pack.data.pbr_scalars(path).roughness_scale, Some(0.5), "the old mount keeps its own");
}

#[test]
fn a_height_only_set_is_no_authored_set_even_as_a_first_person_twin() {
    let index = PbrIndex::build([r"textures\pbr\a\stone_p.dds", r"textures\pbr\a\1stpersongun_p.dds", r"textures\pbr\a\gun_orm.dds", r"textures\pbr\a\knife_orm.dds", r"textures\pbr\a\1stpersonknife_n.dds"]);
    assert!(authored_found(&index, r"textures\a\stone.dds").is_none());
    assert!(authored_found(&index, r"textures\a\1stpersongun.dds").is_none(), "the height-only twin wins; the plain set is not used behind it");
    assert!(authored_found(&index, r"textures\a\gun.dds").is_some());
    // An incomplete twin is no active set, so the plain set serves first person.
    let knife = authored_found(&index, r"textures\a\1stpersonknife.dds").unwrap();
    assert_eq!((knife.stem.as_str(), knife.first_person_fallback), (r"textures\pbr\a\knife", true));
}

/// The swap rule (`retexture`): authored → another set → no set, always from the legacy look,
/// for a shape whose drawn mesh has tangents and for one without (its later `_n` is not
/// applied); a material that takes no set is left alone.
#[test]
fn retexturing_goes_from_set_to_set_to_legacy_from_the_legacy_look() {
    use crate::nif_spawn::{shape_material, Props};
    use crate::texture::bc5_tests::triangle;
    let pack = Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let props = |t: &str| {
        let mut p = Props::default();
        p.textures = vec![t.into()];
        p
    };
    // Drawn with brick (its `_n` gives the mesh tangents), then with plain (no set, no tangents).
    let (with_n, _) = shape_material(&props(r"walls\brick.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    let (bare, _) = shape_material(&props(r"walls\plain.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    assert!(is_authored(&with_n) && with_n.base.normal_map_texture.is_some());
    assert!(!is_authored(&bare) && !bare.extension.legacy.as_ref().unwrap().tangents);
    let brick_n = with_n.base.normal_map_texture.clone();
    for (start, tangents) in [(with_n, true), (bare, false)] {
        let legacy = start.extension.legacy.clone().unwrap();
        let mut m = start.clone();
        // Card A: brick's set.
        assert_eq!(retexture(&mut m, r"walls\brick.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
        assert_eq!(m.base.normal_map_texture.is_some(), tangents, "brick's _n only where the drawn mesh has tangents");
        assert_eq!((m.base.reflectance, m.base.perceptual_roughness), (0.25, 0.5));
        // Card B, from the copy card A wore: its own set, nothing of brick's.
        let mut b = m.clone();
        assert_eq!(retexture(&mut b, r"walls\nonormal.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
        assert_eq!(b.base.normal_map_texture, legacy.normal, "no _n: the legacy normal state, never brick's");
        assert_ne!(b.base.metallic_roughness_texture, m.base.metallic_roughness_texture);
        assert_eq!((b.base.reflectance, b.base.perceptual_roughness), (0.5, 1.0), "the scalars' defaults");
        // No set: the legacy look back, mark and maps gone.
        assert_eq!(retexture(&mut b, r"walls\plain.dds", &mut cache, &pack.data, &mut images, None), Retextured::Legacy);
        assert!(!is_authored(&b) && b.base.metallic_roughness_texture.is_none() && b.base.occlusion_texture.is_none());
        assert_eq!((b.base.normal_map_texture.clone(), b.base.specular_texture.clone(), b.base.reflectance, b.base.perceptual_roughness, b.base.metallic), (legacy.normal.clone(), legacy.specular.clone(), legacy.reflectance, legacy.roughness, legacy.metallic));
        assert_eq!(b.extension.legacy, Some(legacy), "the legacy look itself never changes");
    }
    assert!(brick_n.is_some());
    // A material that takes no set (no legacy look): untouched.
    let mut fx = crate::nif_material::nif_material(StandardMaterial { unlit: true, ..default() });
    assert_eq!(retexture(&mut fx, r"walls\brick.dds", &mut cache, &pack.data, &mut images, None), Retextured::Untouched);
    assert!(fx.base.metallic_roughness_texture.is_none());
    // Made legacy for good (a FaceGen target, the hair model): no later set either.
    let (mut skin, _) = shape_material(&props(r"walls\brick.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    assert!(make_legacy(&mut skin) && !is_authored(&skin));
    assert_eq!(retexture(&mut skin, r"walls\brick.dds", &mut cache, &pack.data, &mut images, None), Retextured::Untouched);
}

/// Review of stage 3 (#2981): a specular colour a script set itself stays drawn when a swap
/// puts another set on the copy, and when it puts the legacy look back; the stored colour the
/// mirror writes back with another value's edit does not, and the next set draws white.
#[test]
fn a_scripted_specular_colour_survives_a_swap_to_another_set_and_back_to_legacy() {
    use crate::nif_spawn::{shape_material, Props};
    use crate::texture::bc5_tests::triangle;
    let pack = Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let mut props = Props::default();
    props.textures = vec![r"walls\brick.dds".into()];
    let (built, _) = shape_material(&props, &mut triangle(), &mut cache, &pack.data, &mut images, None);
    assert!(is_authored(&built));
    let red = Color::linear_rgb(1.0, 0.25, 0.25);
    let mut scripted = built.clone();
    set_legacy_specular_tint(&mut scripted, red, true);
    assert_eq!(scripted.base.specular_tint, red, "drawn at once");
    // Another value's edit writes the same colour back, not as the change.
    set_legacy_specular_tint(&mut scripted, red, false);
    assert_eq!(scripted.extension.legacy.as_ref().unwrap().scripted_tint, Some(red));
    let mut swapped = scripted.clone();
    assert_eq!(retexture(&mut swapped, r"walls\nonormal.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert_eq!(swapped.base.specular_tint, red, "another set keeps the scripted colour");
    assert_eq!(retexture(&mut swapped, r"walls\plain.dds", &mut cache, &pack.data, &mut images, None), Retextured::Legacy);
    assert_eq!(swapped.base.specular_tint, red, "the legacy look keeps it too");
    // Only written back (a stored black with an alpha edit): the next set draws white.
    let mut stored = built.clone();
    set_legacy_specular_tint(&mut stored, Color::linear_rgb(0.0, 0.0, 0.0), false);
    assert_eq!(stored.base.specular_tint, Color::WHITE);
    assert_eq!(retexture(&mut stored, r"walls\nonormal.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert_eq!(stored.base.specular_tint, Color::WHITE, "a stored colour does not scale a set's highlight");
}

#[test]
fn channel_views_parse_and_name_their_defs() {
    assert_eq!(PbrView::parse(""), Some(PbrView::Off));
    assert_eq!(PbrView::parse(" Roughness "), Some(PbrView::Roughness));
    assert_eq!(PbrView::parse("bogus"), None);
    for v in PbrView::ALL {
        assert_eq!(PbrView::parse(v.name()), Some(v));
        let defs = v.shader_defs();
        assert_eq!(defs[0], "NVF_PBR_VIEW");
        assert!(defs[1].ends_with(&v.name().to_ascii_uppercase()));
    }
    assert!(PbrView::Off.shader_defs().is_empty());
    // The tests run without the variables: no view, no metallic switch, metallic held at 0.
    if std::env::var_os(VIEW_ENV).is_none() && std::env::var_os(METALLIC_ENV).is_none() {
        assert!(!metallic_debug());
        assert_eq!((view(), authored_metallic()), (PbrView::Off, 0.0));
    }
}

/// Landscape LOD models take no set, through the model loader and the direct spawn alike.
#[test]
fn landscape_lod_models_are_recognised_whatever_the_spelling() {
    assert!(crate::model::is_lod_model("Meshes/Landscape/LOD/WastelandNV/WastelandNV.level4.x0.y0.nif"));
    assert!(crate::model::is_lod_model("meshes/landscape/lod/wastelandnv/blocks/wastelandnv.level4.x0.y0.nif"));
    assert!(!crate::model::is_lod_model("meshes/architecture/goodsprings/house01.nif"));
}

/// The exit summary's counts read back in its line (a run's own counters are global; this
/// one is fresh).
#[test]
fn the_summary_line_names_every_count() {
    let s = Stats::default();
    for (c, n) in [(&s.built, 3), (&s.not_eligible, 5), (&s.excluded_texture, 1), (&s.orm_failed, 2), (&s.normal_without_tangents, 4), (&s.made_legacy, 6), (&s.swaps_authored, 7), (&s.swaps_legacy, 8)] {
        for _ in 0..n {
            Stats::add(c);
        }
    }
    s.stems.lock().unwrap().extend(["a".to_string(), "b".to_string()]);
    assert_eq!(
        s.line(),
        "3 material builds applied a set (2 distinct set stems at build); builds without one: 5 not eligible, 1 on excluded textures; 2 ORM load failures at build or swap; 4 applied without their _n (no tangents); 6 role copies made legacy; 15 retexture calls (7 to a set, 8 to the legacy look)"
    );
}

/// The role pass's gate: off, or a mount without authored sets, means no shape can take one.
#[test]
fn a_mount_has_sets_only_with_the_setting_on_and_an_authored_set() {
    let pack = Pack::new();
    assert!(!mount_has_sets_when(&pack.data, false));
    assert!(!pack.data.pbr_index_built(), "off asks nothing of the index");
    assert!(mount_has_sets_when(&pack.data, true));
    let empty = std::env::temp_dir().join(format!("nvf-authored-empty-{}", std::process::id()));
    std::fs::create_dir_all(empty.join("textures")).unwrap();
    let data = GameData::new(Arc::new(fnv_vfs::Vfs::open(&empty, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
    assert!(!mount_has_sets_when(&data, true), "no authored set in the mount");
    let _ = std::fs::remove_dir_all(&empty);
}

/// The metal gain override's value: a gain from 1 to 40 (the scalar file's range), nothing for
/// no value, a reason for anything else (the caller warns once and the sets keep their own).
#[test]
fn the_metal_gain_override_takes_a_gain_from_one_to_forty() {
    assert_eq!(parse_metal_gain(None), Ok(None));
    assert_eq!(parse_metal_gain(Some("  ")), Ok(None));
    assert_eq!(parse_metal_gain(Some("1")), Ok(Some(1.0)));
    assert_eq!(parse_metal_gain(Some(" 2.5 ")), Ok(Some(2.5)));
    assert_eq!(parse_metal_gain(Some("40")), Ok(Some(40.0)));
    assert!(parse_metal_gain(Some("0.5")).is_err_and(|e| e.contains("outside 1 to 40")));
    assert!(parse_metal_gain(Some("41")).is_err());
    assert!(parse_metal_gain(Some("NaN")).is_err(), "not in the range");
    assert!(parse_metal_gain(Some("bright")).is_err_and(|e| e.contains("not a number")));
    // The tests run without the variable: no override.
    if std::env::var_os(METAL_GAIN_ENV).is_none() {
        assert_eq!(metal_gain_override(), None);
    }
}

/// A set's gain: the override over the file over the default by `_albedo`.
#[test]
fn a_sets_metal_gain_is_the_override_the_file_or_the_default_by_albedo() {
    let none = Scalars::default();
    let six = Scalars { metal_gain: Some(6.0), ..Scalars::default() };
    assert_eq!(set_metal_gain(&none, false, None), 12.0, "the game's diffuse");
    assert_eq!(set_metal_gain(&none, true, None), 1.0, "an _albedo holds real reflectances");
    assert_eq!(set_metal_gain(&six, false, None), 6.0);
    assert_eq!(set_metal_gain(&six, true, None), 6.0, "the file wins over the _albedo default");
    assert_eq!(set_metal_gain(&six, true, Some(20.0)), 20.0, "the run's override wins over everything");
    assert_eq!(set_metal_gain(&none, false, Some(1.0)), 1.0);
}

/// The CPU mirror of the shader's lift (nif_material.wesl `nvf_metal_base`): one gain on all
/// channels, limited so the brightest reaches 1 (hue kept), never below the base, by the
/// metallic; gain 1 and metallic 0 leave the base as it is.
#[test]
fn the_metal_lift_keeps_the_hue_and_stops_at_one() {
    let near = |a: Vec3, b: Vec3| a.distance(b) < 1e-5;
    let minigun = Vec3::new(0.03, 0.02, 0.015);
    // Under the limit: the plain gain.
    assert!(near(lifted_base(minigun, 12.0, 1.0), minigun * 12.0));
    // Over it: the brightest channel reaches exactly 1 and the ratios stay.
    let bronze = Vec3::new(0.4, 0.3, 0.2);
    let l = lifted_base(bronze, 12.0, 1.0);
    assert!((l.max_element() - 1.0).abs() < 1e-6, "{l}");
    assert!(near(l / l.x, bronze / bronze.x), "hue kept: {l}");
    assert!(near(l, Vec3::new(1.0, 0.75, 0.5)));
    // The experiment's per-channel clip would have shifted it: min(base x 12, 1) is white.
    assert!(near((bronze * 12.0).min(Vec3::ONE), Vec3::ONE));
    // No lift: gain 1, metallic 0, or a base already at 1.
    assert!(near(lifted_base(bronze, 1.0, 1.0), bronze));
    assert!(near(lifted_base(bronze, 40.0, 0.0), bronze));
    assert!(near(lifted_base(Vec3::new(1.0, 0.5, 0.1), 12.0, 1.0), Vec3::new(1.0, 0.5, 0.1)));
    // Half metallic: half way.
    assert!(near(lifted_base(bronze, 12.0, 0.5), (bronze + Vec3::new(1.0, 0.75, 0.5)) * 0.5));
    // Never below the base (a base colour above 1, a gain lane of 0 on a material without a set).
    assert!(near(lifted_base(Vec3::new(2.0, 1.0, 0.5), 12.0, 1.0), Vec3::new(2.0, 1.0, 0.5)));
    assert!(near(lifted_base(bronze, 0.0, 1.0), bronze));
    // Black stays black (the 1e-4 floor keeps the division finite).
    assert!(lifted_base(Vec3::ZERO, 40.0, 1.0).is_finite() && near(lifted_base(Vec3::ZERO, 40.0, 1.0), Vec3::ZERO));
}

/// The metal gain lane (`NifSurface::metal_gain`, the uniform's `terrain.y`): a build with a
/// set carries its gain, a build without one 0; a swap carries the new set's gain and the
/// legacy look's 0; restore, make_legacy and an unlit copy clear it.
#[test]
fn the_metal_gain_lane_follows_the_set_on_the_material() {
    use crate::nif_material::NifSurfaceUniform;
    use crate::nif_spawn::{shape_material, Props};
    use crate::texture::bc5_tests::triangle;
    if std::env::var_os(METAL_GAIN_ENV).is_some() {
        return; // an override replaces every set's gain
    }
    let pack = Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let props = |t: &str| {
        let mut p = Props::default();
        p.textures = vec![t.into()];
        p
    };
    let lane = |m: &crate::NifMaterial| (m.extension.metal_gain, NifSurfaceUniform::from(&m.extension).terrain.y);
    let (brick, _) = shape_material(&props(r"walls\brick.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    let (steel, _) = shape_material(&props(r"walls\steel.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    let (plain, _) = shape_material(&props(r"walls\plain.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    assert_eq!(lane(&brick), (12.0, 12.0), "no metal_gain in brick's file: the default");
    assert_eq!(lane(&steel), (3.0, 3.0), "the scalar file's");
    assert_eq!(lane(&plain), (0.0, 0.0), "no set");
    // Swaps: the new set's gain, the legacy look's 0, back again.
    let mut m = plain.clone();
    assert_eq!(retexture(&mut m, r"walls\steel.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert_eq!(lane(&m), (3.0, 3.0));
    assert_eq!(retexture(&mut m, r"walls\brick.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert_eq!(lane(&m), (12.0, 12.0), "never the old set's");
    assert_eq!(retexture(&mut m, r"walls\plain.dds", &mut cache, &pack.data, &mut images, None), Retextured::Legacy);
    assert_eq!(lane(&m), (0.0, 0.0));
    // A copy carries the lane (it is a field of the material).
    let copy = steel.clone();
    assert_eq!(lane(&copy), (3.0, 3.0));
    // Restore, make_legacy, an unlit copy: cleared.
    let mut restored = steel.clone();
    restore(&mut restored, steel.extension.legacy.as_ref().unwrap());
    assert_eq!(lane(&restored), (0.0, 0.0));
    let mut legacy_for_good = steel.clone();
    assert!(make_legacy(&mut legacy_for_good));
    assert_eq!(lane(&legacy_for_good), (0.0, 0.0));
    let mut unlit = steel.clone();
    clear_for_unlit_copy(&mut unlit);
    assert_eq!(lane(&unlit), (0.0, 0.0));
}

/// `_albedo`: loaded by its exact path in the colour role (sRGB), the gain's default 1 with it
/// and 12 when it does not load (warned once, the game's diffuse stays); alone it is no set.
#[test]
fn an_albedo_loads_as_colour_and_sets_the_gain_default() {
    let pack = Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let mut source = StoreTextures { textures: &mut cache, data: &pack.data, images: &mut images, formats: None };
    let painted = source.authored(&source.resolve("walls/painted.dds")).expect("painted set");
    let smudged = source.authored(&source.resolve("walls/smudged.dds")).expect("smudged set: its _orm loads");
    assert!(source.authored(&source.resolve("walls/lonely.dds")).is_none(), "an _albedo without _orm is no set");
    let brick = source.authored(&source.resolve("walls/brick.dds")).unwrap();
    assert!(brick.albedo.is_none());
    let albedo = painted.albedo.clone().expect("_albedo loaded");
    assert!(smudged.albedo.is_none(), "an _albedo that does not load leaves the diffuse");
    if std::env::var_os(METAL_GAIN_ENV).is_none() {
        assert_eq!((painted.metal_gain, smudged.metal_gain, brick.metal_gain), (1.0, 12.0, 12.0));
    }
    let warned = cache.workers.authored.warned.lock().unwrap().clone();
    assert_eq!(images.get(&albedo).unwrap().texture_descriptor.format, TextureFormat::Bc7RgbaUnormSrgb, "colour, as the game's diffuse");
    assert!(warned.contains(r"textures\pbr\walls\smudged_albedo.dds"), "{warned:?}");
    assert!(!warned.contains(r"textures\pbr\walls\painted_albedo.dds"), "no sRGB warning for a colour member");
}

/// Swaps with `_albedo` (`retexture`, the callers putting the new game diffuse on first): to a
/// set with one, its `_albedo`; to a set without, the new diffuse; to no set, the new diffuse
/// and the legacy look; restore gives back the diffuse of the texture shown last.
#[test]
fn a_swap_draws_the_new_sets_albedo_or_the_new_diffuse() {
    use crate::nif_spawn::{shape_material, Props};
    use crate::texture::bc5_tests::triangle;
    let pack = Pack::new();
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let props = |t: &str| {
        let mut p = Props::default();
        p.textures = vec![t.into()];
        p
    };
    let (plain, _) = shape_material(&props(r"walls\plain.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    let (painted, _) = shape_material(&props(r"walls\painted.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    let albedo = painted.extension.authored_albedo.clone().expect("built with the _albedo");
    assert_eq!(painted.base.base_color_texture.as_ref(), Some(&albedo));
    // What a caller does: the new game diffuse on the copy, then the rule.
    let mut swap = |m: &mut crate::NifMaterial, t: &str| {
        m.base.base_color_texture = cache.get(t, true, &pack.data, &mut images, None);
        let shown = m.base.base_color_texture.clone();
        (retexture(m, t, &mut cache, &pack.data, &mut images, None), shown)
    };
    let mut m = plain.clone();
    let (r, _) = swap(&mut m, r"walls\painted.dds");
    assert_eq!((r, m.base.base_color_texture.as_ref()), (Retextured::Authored, Some(&albedo)), "the new set's _albedo");
    let (r, brick_diffuse) = swap(&mut m, r"walls\brick.dds");
    assert_eq!((r, m.base.base_color_texture.clone()), (Retextured::Authored, brick_diffuse.clone()), "a set without one: the new diffuse");
    assert!(m.extension.authored_albedo.is_none());
    swap(&mut m, r"walls\painted.dds");
    let (r, plain_diffuse) = swap(&mut m, r"walls\plain.dds");
    assert_eq!((r, m.base.base_color_texture.clone()), (Retextured::Legacy, plain_diffuse.clone()), "no set: the new diffuse");
    // The legacy look follows the diffuse shown last; restore gives that one back.
    let (_, painted_diffuse) = swap(&mut m, r"walls\painted.dds");
    assert_eq!(m.extension.legacy.as_ref().unwrap().diffuse, painted_diffuse);
    let legacy = m.extension.legacy.clone().unwrap();
    restore(&mut m, &legacy);
    assert_eq!(m.base.base_color_texture, painted_diffuse);
    // SwapTexture can explicitly name the albedo file already on the material. The cache
    // gives the same handle, now as the replacement diffuse.
    let mut direct = painted.clone();
    let (r, diffuse) = swap(&mut direct, r"textures\pbr\walls\painted_albedo.dds");
    assert_eq!(r, Retextured::Legacy);
    assert_eq!(diffuse.as_ref(), Some(&albedo));
    assert_eq!(direct.base.base_color_texture, diffuse);
    assert_eq!(direct.extension.legacy.as_ref().unwrap().diffuse, diffuse);
    assert!(direct.extension.authored_albedo.is_none());
}
