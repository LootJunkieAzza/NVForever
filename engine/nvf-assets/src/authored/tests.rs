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
            // Height only: no authored set.
            ("textures/walls/stone.dds", dxt1.clone()),
            ("textures/pbr/walls/stone_p.dds", legacy(b"BC4U", 4, 4, &[0; 8])),
            // An authored set with `_p` and a depth; a height-only set of depth 0; an
            // authored set whose `_p` is no DDS file.
            ("textures/walls/relief.dds", dxt1.clone()),
            ("textures/pbr/walls/relief_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/relief_p.dds", dx10(80, 4, 4, &[255, 0, 0, 0, 0, 0, 0, 0])),
            ("textures/pbr/walls/relief.toml", b"parallax_depth = 0.05\n".to_vec()),
            ("textures/walls/flat.dds", dxt1.clone()),
            ("textures/pbr/walls/flat_p.dds", legacy(b"BC4U", 4, 4, &[0; 8])),
            ("textures/pbr/walls/flat.toml", b"parallax_depth = 0\n".to_vec()),
            ("textures/walls/brokenheight.dds", dxt1.clone()),
            ("textures/pbr/walls/brokenheight_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/brokenheight_p.dds", b"not a dds".to_vec()),
            // Signed BC4 `_p`: refused, legacy FourCC and DX10 (BC4_SNORM, 81) alike.
            ("textures/walls/signed.dds", dxt1.clone()),
            ("textures/pbr/walls/signed_p.dds", legacy(b"BC4S", 4, 4, &[0; 8])),
            ("textures/walls/signedrelief.dds", dxt1.clone()),
            ("textures/pbr/walls/signedrelief_orm.dds", dx10(98, 4, 4, &[0; 16])),
            ("textures/pbr/walls/signedrelief_p.dds", dx10(81, 4, 4, &[0; 8])),
            // A `_p` in a format no height map takes (unsigned BC5): refused too.
            ("textures/walls/twochannel.dds", dxt1.clone()),
            ("textures/pbr/walls/twochannel_p.dds", legacy(b"ATI2", 4, 4, &[0; 16])),
            // DXT3 `_p`, on the 4-texel grid (BC2 blocks) and off it (decoded to RGBA8 on the
            // CPU): refused alike, as `fnvtool pbr check` refuses DXT3; a DXT1 off the grid
            // (decoded the same way) is an allowed format but refused by its size, as the
            // validator refuses it, and so is an unsigned BC4 off the grid (6x6, decoded to R8
            // on the CPU); signed BC5: refused.
            ("textures/walls/dxt3.dds", dxt1.clone()),
            ("textures/pbr/walls/dxt3_p.dds", legacy(b"DXT3", 4, 4, &[0; 16])),
            ("textures/walls/odddxt3.dds", dxt1.clone()),
            ("textures/pbr/walls/odddxt3_p.dds", legacy(b"DXT3", 3, 3, &[0; 16])),
            ("textures/walls/odddxt1.dds", dxt1.clone()),
            ("textures/pbr/walls/odddxt1_p.dds", legacy(b"DXT1", 3, 3, &[0; 8])),
            ("textures/walls/oddbc4.dds", dxt1.clone()),
            ("textures/pbr/walls/oddbc4_p.dds", legacy(b"BC4U", 6, 6, &[0; 32])),
            ("textures/walls/signedtwo.dds", dxt1.clone()),
            ("textures/pbr/walls/signedtwo_p.dds", legacy(b"BC5S", 4, 4, &[0; 16])),
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
    let authored = |s: Option<PbrSet>| match s {
        Some(PbrSet::Authored(a)) => Some(a),
        _ => None,
    };
    let set = authored(source.pbr_set(&key)).expect("authored set");
    assert_eq!(set.stem, r"textures\pbr\walls\brick");
    assert_eq!(set.scalars, Scalars { parallax_depth: None, roughness_scale: Some(0.5), reflectance: Some(0.25) });
    assert!(set.height.is_none(), "brick has no _p");
    let normal = set.normal.clone().expect("_n");
    let none = authored(source.pbr_set(&source.resolve("walls/nonormal.dds"))).expect("set without _n");
    assert!(none.normal.is_none());
    assert_eq!(none.scalars, Scalars::default());
    // Height only: the `_p` alone, at the default depth.
    let stone = source.pbr_set(&source.resolve("walls/stone.dds")).expect("height-only set");
    assert!(stone.authored().is_none(), "height only is no authored set");
    let stone_height = stone.height().cloned().expect("its height map");
    assert_eq!(
        (stone.stem(), stone_height.depth_scale, stone_height.method, stone_height.max_layers),
        (r"textures\pbr\walls\stone", fnv_pbr::DEFAULT_PARALLAX_DEPTH, parallax_method(), parallax_layers())
    );
    // An authored set with `_p` and a depth in its scalar file.
    let relief = authored(source.pbr_set(&source.resolve("walls/relief.dds"))).expect("authored set with _p");
    let relief_height = relief.height.clone().expect("_p");
    assert_eq!(relief_height.depth_scale, 0.05);
    // A depth of 0: no height map, so a height-only set is no set at all and its `_p` is not read.
    assert!(source.pbr_set(&source.resolve("walls/flat.dds")).is_none(), "depth 0: nothing to draw");
    // A `_p` that does not load: an authored set keeps the rest.
    let broken = authored(source.pbr_set(&source.resolve("walls/brokenheight.dds"))).expect("the _orm still applies");
    assert!(broken.height.is_none());
    // Signed BC4: a height-only set gives nothing; an authored one keeps the rest.
    assert!(source.pbr_set(&source.resolve("walls/signed.dds")).is_none(), "a signed _p is refused");
    let signed = authored(source.pbr_set(&source.resolve("walls/signedrelief.dds"))).expect("the _orm still applies");
    assert!(signed.height.is_none(), "a signed _p is refused");
    assert!(source.pbr_set(&source.resolve("walls/twochannel.dds")).is_none(), "a _p in no unsigned height format is refused");
    // The file's own format decides, as the validator's does: DXT3 on or off the block grid.
    assert!(source.pbr_set(&source.resolve("walls/dxt3.dds")).is_none(), "an aligned DXT3 _p (BC2) is refused");
    assert!(source.pbr_set(&source.resolve("walls/odddxt3.dds")).is_none(), "an odd-sized DXT3 _p is refused though it went up as Rgba8Unorm");
    // Off the 4-texel grid: refused by size, though the format is allowed and decodes on the
    // CPU (the validator's rule, `not-block-aligned`).
    assert!(source.pbr_set(&source.resolve("walls/odddxt1.dds")).is_none(), "an odd-sized DXT1 _p is refused by its size");
    assert!(source.pbr_set(&source.resolve("walls/oddbc4.dds")).is_none(), "a 6x6 BC4 _p is refused by its size");
    assert!(source.pbr_set(&source.resolve("walls/signedtwo.dds")).is_none(), "a signed BC5 _p is refused");
    assert!(source.pbr_set(&source.resolve("walls/plain.dds")).is_none());
    assert!(source.pbr_set(&source.resolve("walls/bad.dds")).is_none(), "an _orm that does not load leaves the shape legacy");
    let twin = source.pbr_set(&source.resolve("weapons/1stpersongun.dds")).expect("the twin's own height-only set");
    assert_eq!((twin.stem(), twin.authored().is_none()), (r"textures\pbr\weapons\1stpersongun", true), "a height-only twin wins the lookup: no authored set, not the plain one's");
    assert_eq!(source.pbr_set(&source.resolve("weapons/gun.dds")).map(|s| s.stem().to_string()).as_deref(), Some(r"textures\pbr\weapons\gun"));
    // The console folder: the member the index names, not the PC file `get` would choose.
    let xbox_key = source.resolve(r"walls\xbox\tile.dds");
    assert_eq!(xbox_key, r"textures\walls\xbox\tile.dds", "no PC diffuse: the key stays");
    let xbox = authored(source.pbr_set(&xbox_key)).expect("xbox set");
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
    assert!(warned.contains(r"textures\pbr\walls\brokenheight_p.dds"), "{warned:?}");
    assert!(warned.contains(r"textures\pbr\walls\signed_p.dds") && warned.contains(r"textures\pbr\walls\signedrelief_p.dds"), "{warned:?}");
    assert!(signed_format(TextureFormat::Bc4RSnorm) && signed_format(TextureFormat::R8Snorm) && !signed_format(TextureFormat::Bc4RUnorm) && !signed_format(TextureFormat::R8Unorm));
    assert!(warned.contains(r"textures\pbr\walls\twochannel_p.dds"), "{warned:?}");
    for refused in ["dxt3", "odddxt3", "signedtwo"] {
        assert!(warned.contains(&format!(r"textures\pbr\walls\{refused}_p.dds")), "{refused}: {warned:?}");
    }
    for odd in ["odddxt1", "oddbc4"] {
        assert!(warned.contains(&format!(r"textures\pbr\walls\{odd}_p.dds")), "{odd}: one warning naming the file: {warned:?}");
    }
    // The size rule: each side a multiple of 4, as `fnvtool pbr check` wants.
    assert_eq!(height_size_refusal(4, 4), None);
    assert_eq!(height_size_refusal(512, 8), None);
    assert_eq!(height_size_refusal(3, 3).as_deref(), Some("is 3x3, a side not a multiple of 4"));
    assert_eq!(height_size_refusal(8, 6).as_deref(), Some("is 8x6, a side not a multiple of 4"));
    assert_eq!(height_size_refusal(6, 8).as_deref(), Some("is 6x8, a side not a multiple of 4"));
    // The refusal names the file's own format, signed BC5 as BC5, and judges by it.
    use fnv_dds::Format;
    assert_eq!(height_refusal(Some(Format::Bc5S), Some(TextureFormat::Bc5RgSnorm)), "is signed (BC5_SNORM); a height map is unsigned (0 low, 1 high)");
    assert_eq!(height_refusal(Some(Format::Bc4S), Some(TextureFormat::R8Snorm)), "is signed (BC4_SNORM); a height map is unsigned (0 low, 1 high)");
    assert_eq!(height_refusal(Some(Format::Dxt3), Some(TextureFormat::Rgba8Unorm)), "is in DXT3, not in a height map's format (BC4_UNORM, DXT1 or DXT5)");
    assert_eq!(height_refusal(None, Some(TextureFormat::Bc2RgbaUnorm)), "is in Bc2RgbaUnorm, not in a height map's format (BC4_UNORM, DXT1 or DXT5)");
    assert!(!height_source_ok(Some(Format::Dxt3), Some(TextureFormat::Rgba8Unorm)) && height_source_ok(Some(Format::Dxt1), Some(TextureFormat::Rgba8Unorm)));
    assert!(!height_source_ok(None, Some(TextureFormat::Rgba8Unorm)) && height_source_ok(None, Some(TextureFormat::Bc4RUnorm)), "an unknown source: only formats that name it");
    // Unknown format metadata is refused, not accepted by default.
    assert!(!height_format_ok(None) && !height_format_ok(Some(TextureFormat::Bc5RgUnorm)) && !height_format_ok(Some(TextureFormat::Bc4RSnorm)));
    assert!(height_format_ok(Some(TextureFormat::Bc4RUnorm)) && height_format_ok(Some(TextureFormat::R8Unorm)) && height_format_ok(Some(TextureFormat::Bc1RgbaUnorm)));
    assert!(!cache.has_or_ahead(r"textures\pbr\walls\flat_p.dds"), "a depth of 0 loads no _p");
    // `_p` in the linear role, as unsigned BC4.
    for h in [stone_height, relief_height] {
        assert_eq!(images.get(h.depth_map.as_ref().unwrap()).unwrap().texture_descriptor.format, TextureFormat::Bc4RUnorm);
    }
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
    for (c, n) in [
        (&s.built, 3),
        (&s.not_eligible, 5),
        (&s.excluded_texture, 1),
        (&s.orm_failed, 2),
        (&s.normal_without_tangents, 4),
        (&s.made_legacy, 6),
        (&s.swaps_authored, 7),
        (&s.swaps_legacy, 8),
        (&s.swaps_height, 9),
        (&s.swaps_height_applied, 20),
        (&s.height_built, 10),
        (&s.height_only_built, 11),
        (&s.height_alpha_tested, 12),
        (&s.height_blended, 21),
        (&s.height_double_sided, 13),
        (&s.height_decal, 14),
        (&s.height_no_tangents, 15),
        (&s.height_other, 16),
        (&s.height_failed, 17),
        (&s.height_refused, 18),
        (&s.height_dropped, 19),
    ] {
        for _ in 0..n {
            Stats::add(c);
        }
    }
    s.stems.lock().unwrap().extend(["a".to_string(), "b".to_string()]);
    assert_eq!(
        s.line(),
        "3 material builds applied an ORM set (2 distinct ORM set stems at build), 11 a height-only set's height map; builds without one: 5 not eligible, 1 on excluded textures; 2 ORM load failures at build or swap; 4 applied without their _n (no tangents); 6 role copies made legacy; 24 retexture calls (7 to an ORM set, 9 to a loaded height-only set (20 with its height map applied), 8 to the legacy look); height maps: 10 material builds applied one; not put on at build or swap: 12 alpha-tested, 21 blended, 13 double-sided, 14 decals, 15 without tangents, 16 unlit, refracting or holed; 17 _p not loaded, 18 _p refused (signed, in no height map's format or a side not a multiple of 4); 19 later copies dropped one"
    );
}

/// The role pass's gate: off, or a mount without active sets, means no shape can take one; a
/// mount with only height-only sets has them (its shapes' roles must still be excluded).
#[test]
fn a_mount_has_sets_only_with_the_setting_on_and_an_active_set() {
    let pack = Pack::new();
    assert!(!mount_has_sets_when(&pack.data, false));
    assert!(!pack.data.pbr_index_built(), "off asks nothing of the index");
    assert!(mount_has_sets_when(&pack.data, true));
    let empty = std::env::temp_dir().join(format!("nvf-authored-empty-{}", std::process::id()));
    std::fs::create_dir_all(empty.join("textures")).unwrap();
    let data = GameData::new(Arc::new(fnv_vfs::Vfs::open(&empty, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
    assert!(!mount_has_sets_when(&data, true), "no set in the mount");
    let _ = std::fs::remove_dir_all(&empty);
    let heights = std::env::temp_dir().join(format!("nvf-authored-heights-{}", std::process::id()));
    std::fs::create_dir_all(heights.join("textures/pbr/walls")).unwrap();
    std::fs::write(heights.join("textures/pbr/walls/stone_p.dds"), legacy(b"BC4U", 4, 4, &[0; 8])).unwrap();
    let data = GameData::new(Arc::new(fnv_vfs::Vfs::open(&heights, &[]).unwrap()), Arc::new(fnv_nif::Schema::default()));
    assert!(mount_has_sets_when(&data, true), "height-only sets count");
    let _ = std::fs::remove_dir_all(&heights);
}

/// The swap rule with height maps (#2981 stage 4): authored with `_p` → height only → none
/// → authored without `_p`, each from the legacy look, the old overlay taken off first; on a
/// drawn mesh without tangents no height map goes on; a blended copy takes none.
#[test]
fn retexturing_moves_height_maps_with_the_sets() {
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
    // relief: `_orm` and `_p` (tangents made for the `_p`); plain: no set, no tangents.
    let (relief, _) = shape_material(&props(r"walls\relief.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    let (bare, _) = shape_material(&props(r"walls\plain.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    assert!(is_authored(&relief) && relief.base.depth_map.is_some() && relief.base.parallax_depth_scale == 0.05);
    assert!(relief.extension.legacy.as_ref().unwrap().tangents && !bare.extension.legacy.as_ref().unwrap().tangents);
    let legacy = relief.extension.legacy.clone().unwrap();
    let mut m = relief.clone();
    // Authored with `_p` → height only: the ORM off, stone's height map on, the mark clear.
    assert_eq!(retexture(&mut m, r"walls\stone.dds", &mut cache, &pack.data, &mut images, None), Retextured::HeightOnly);
    assert!(!is_authored(&m) && m.base.metallic_roughness_texture.is_none());
    assert!(m.base.depth_map.is_some() && m.base.depth_map != relief.base.depth_map);
    assert_eq!(m.base.parallax_depth_scale, fnv_pbr::DEFAULT_PARALLAX_DEPTH);
    assert_eq!((m.base.reflectance, m.base.perceptual_roughness), (legacy.reflectance, legacy.roughness), "the game's values");
    // Height only → none: the baseline back.
    assert_eq!(retexture(&mut m, r"walls\plain.dds", &mut cache, &pack.data, &mut images, None), Retextured::Legacy);
    assert_eq!(Parallax::of(&m.base), legacy.parallax);
    // None → authored without `_p` → back to relief: no stale height map, then relief's.
    assert_eq!(retexture(&mut m, r"walls\nonormal.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert!(m.base.depth_map.is_none());
    assert_eq!(retexture(&mut m, r"walls\relief.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert_eq!(m.base.depth_map, relief.base.depth_map);
    // Height only → authored without `_p`: the height map off.
    assert_eq!(retexture(&mut m, r"walls\stone.dds", &mut cache, &pack.data, &mut images, None), Retextured::HeightOnly);
    assert_eq!(retexture(&mut m, r"walls\nonormal.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert!(m.base.depth_map.is_none() && is_authored(&m));
    assert_eq!(m.extension.legacy.as_ref(), Some(&legacy), "the legacy look itself never changes");
    // A drawn mesh without tangents: no height map from a swap (the recorded tangents).
    // `HeightOnly` says the new diffuse's height-only set loaded; the gate refused its map,
    // which the summary counts apart ("to a loaded height-only set (N with its height map
    // applied)": this call adds to the first count only).
    let mut b = bare.clone();
    assert_eq!(retexture(&mut b, r"walls\stone.dds", &mut cache, &pack.data, &mut images, None), Retextured::HeightOnly);
    assert!(b.base.depth_map.is_none() && !height_on(&b), "matched, not applied");
    assert_eq!(retexture(&mut b, r"walls\relief.dds", &mut cache, &pack.data, &mut images, None), Retextured::Authored);
    assert!(b.base.depth_map.is_none() && is_authored(&b));
    // A blended copy (a fade's) takes none either.
    let mut faded = relief.clone();
    faded.base.alpha_mode = AlphaMode::Blend;
    assert_eq!(retexture(&mut faded, r"walls\stone.dds", &mut cache, &pack.data, &mut images, None), Retextured::HeightOnly);
    assert!(faded.base.depth_map.is_none());
    // A height-only material made legacy for good (a role found later): the height map off.
    let (mut skin, _) = shape_material(&props(r"walls\stone.dds"), &mut triangle(), &mut cache, &pack.data, &mut images, None);
    assert!(skin.base.depth_map.is_some() && !is_authored(&skin));
    assert!(make_legacy(&mut skin) && skin.base.depth_map.is_none());
}

/// The layer switch (`NVF_PBR_PARALLAX_LAYERS`): whole numbers 2 to 64; anything else is refused.
#[test]
fn the_layer_switch_takes_whole_numbers_from_2_to_64() {
    assert_eq!(parse_parallax_layers("2"), Ok(2.0));
    assert_eq!(parse_parallax_layers(" 16 "), Ok(16.0));
    assert_eq!(parse_parallax_layers("64"), Ok(64.0));
    for bad in ["1", "0", "65", "-4", "8.5", "eight", ""] {
        assert!(parse_parallax_layers(bad).is_err(), "{bad:?}");
    }
    assert!(parse_parallax_layers("65").unwrap_err().contains("outside 2 to 64"));
}

/// The method switch (`NVF_PBR_PARALLAX_METHOD`): `occlusion` or `relief:<1..8>`, any case.
#[test]
fn the_method_switch_takes_occlusion_or_relief_with_its_steps() {
    assert_eq!(parse_parallax_method("occlusion"), Ok(ParallaxMappingMethod::Occlusion));
    assert_eq!(parse_parallax_method(" Occlusion "), Ok(ParallaxMappingMethod::Occlusion));
    assert_eq!(parse_parallax_method("relief:1"), Ok(ParallaxMappingMethod::Relief { max_steps: 1 }));
    assert_eq!(parse_parallax_method("RELIEF:8"), Ok(ParallaxMappingMethod::Relief { max_steps: 8 }));
    for bad in ["relief", "relief:0", "relief:9", "relief:x", "relief5", "steep", ""] {
        assert!(parse_parallax_method(bad).is_err(), "{bad:?}");
    }
    assert_eq!(parallax_method_name(ParallaxMappingMethod::Relief { max_steps: 5 }), "relief:5");
    // The default: relief with 4 halvings after the march (the occlusion method stays the
    // switch's comparison).
    assert_eq!(PARALLAX_METHOD, ParallaxMappingMethod::Relief { max_steps: 4 });
    assert_eq!(parallax_method_name(PARALLAX_METHOD), "relief:4");
    assert_eq!(parse_parallax_method(&parallax_method_name(PARALLAX_METHOD)), Ok(PARALLAX_METHOD), "the default is a value the switch takes");
}

/// The alpha-tested switch (`NVF_PBR_PARALLAX_MASK`): `1` on, `0` off, nothing else.
#[test]
fn the_mask_switch_takes_1_or_0() {
    assert_eq!(parse_parallax_mask("1"), Ok(true));
    assert_eq!(parse_parallax_mask(" 0 "), Ok(false));
    for bad in ["yes", "true", "2", ""] {
        assert!(parse_parallax_mask(bad).is_err(), "{bad:?}");
    }
}

/// A switch's text: unset or empty keeps the default quietly; a taken value is logged as
/// information (but for a value marked quiet); garbage and out-of-range text keep the
/// default with a warning naming the variable and the text. Without the variables, the
/// process-wide switches must use the defaults.
#[test]
fn a_switch_logs_what_it_took_or_refused_and_keeps_the_default_otherwise() {
    let layers = |text: Option<&str>| switch_value(PARALLAX_LAYERS_ENV, text, MAX_PARALLAX_LAYERS, parse_parallax_layers, |n| format!("{n} layers"), |_| false);
    assert_eq!(layers(None), (MAX_PARALLAX_LAYERS, SwitchLog::Quiet));
    assert_eq!(layers(Some("  ")), (MAX_PARALLAX_LAYERS, SwitchLog::Quiet));
    assert_eq!(layers(Some("16")), (16.0, SwitchLog::Taken("authored materials: NVF_PBR_PARALLAX_LAYERS=16: 16 layers".into())));
    for bad in ["99", "lots"] {
        let (v, log) = layers(Some(bad));
        assert_eq!(v, MAX_PARALLAX_LAYERS, "{bad}");
        assert!(matches!(&log, SwitchLog::Refused(l) if l.starts_with(&format!("authored materials: NVF_PBR_PARALLAX_LAYERS={bad} ")) && l.ends_with("; the default is kept")), "{bad}: {log:?}");
    }
    let method = |text: Option<&str>| switch_value(PARALLAX_METHOD_ENV, text, PARALLAX_METHOD, parse_parallax_method, parallax_method_name, |_| false);
    assert_eq!(method(Some("relief:3")).0, ParallaxMappingMethod::Relief { max_steps: 3 });
    assert!(matches!(method(Some("relief:12")), (m, SwitchLog::Refused(_)) if m == PARALLAX_METHOD), "out of range keeps the default");
    assert!(matches!(method(Some("occlusion")), (ParallaxMappingMethod::Occlusion, SwitchLog::Taken(_))), "the comparison method is taken");
    let mask = |text: Option<&str>| switch_value(PARALLAX_MASK_ENV, text, false, parse_parallax_mask, |_| "on".to_string(), |on| !on);
    assert_eq!(mask(Some("0")), (false, SwitchLog::Quiet));
    assert!(matches!(mask(Some("1")), (true, SwitchLog::Taken(_))));
    assert!(matches!(mask(Some("on")), (false, SwitchLog::Refused(_))));
    if [PARALLAX_LAYERS_ENV, PARALLAX_METHOD_ENV, PARALLAX_MASK_ENV].iter().all(|v| std::env::var_os(v).is_none()) {
        assert_eq!((parallax_layers(), parallax_method(), parallax_mask()), (MAX_PARALLAX_LAYERS, PARALLAX_METHOD, false));
    }
}

/// Which alpha modes may take a height map: opaque always; alpha-tested only under the
/// switch; blends never. A refusal says which kind it was.
#[test]
fn alpha_tested_and_blended_shapes_are_refused_apart() {
    for mask in [false, true] {
        assert_eq!(height_alpha(AlphaMode::Opaque, mask), Ok(()));
        for blended in [AlphaMode::Blend, AlphaMode::Premultiplied, AlphaMode::AlphaToCoverage, AlphaMode::Add, AlphaMode::Multiply] {
            assert_eq!(height_alpha(blended, mask), Err(HeightSkip::Blended), "{blended:?} mask={mask}");
        }
    }
    assert_eq!(height_alpha(AlphaMode::Mask(0.5), false), Err(HeightSkip::AlphaTested));
    assert_eq!(height_alpha(AlphaMode::Mask(0.5), true), Ok(()));
}
