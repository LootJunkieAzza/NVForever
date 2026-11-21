//! The PBR field's fixture, material checks and comparator, without a GPU.

use super::*;
use crate::texture::TextureUsage;

/// A fresh folder under the temp folder with the field's generated files.
struct Owned(PathBuf);

impl Owned {
    fn new() -> Owned {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("nvf-pbr-field-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&root).unwrap();
        write_fixture(&root).unwrap();
        Owned(root)
    }
}

impl Drop for Owned {
    fn drop(&mut self) {
        // This test's own fresh folder.
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_fixture_reads_back_through_the_vfs_in_its_roles_and_sizes() {
    let owned = Owned::new();
    let data = owned_data(&owned.0, Arc::new(fnv_nif::Schema::default())).unwrap();
    let index = data.pbr_index();
    for name in ["lit", "dark"] {
        let f = index.lookup(&format!("textures\\field\\{name}.dds")).unwrap();
        assert!(f.files.has(fnv_pbr::Member::Orm) && f.files.has(fnv_pbr::Member::Normal), "{name}");
    }
    for name in ["reference", "flat"] {
        let f = index.lookup(&format!("textures\\field\\{name}.dds")).unwrap();
        assert!(f.files.has(fnv_pbr::Member::Orm) && !f.files.has(fnv_pbr::Member::Normal), "{name}");
    }
    assert!(index.lookup("textures\\field\\plain.dds").is_none());
    let mut cache = TextureCache::default();
    let mut images = Assets::<Image>::default();
    let mut load = |path: &str, usage| {
        let (h, _) = cache.exact(path, usage, &data, &mut images, None).unwrap();
        let i = images.get(&h).unwrap();
        (i.size(), i.texture_descriptor.format, i.data.clone().unwrap())
    };
    assert_eq!(load("textures\\field\\plain.dds", TextureUsage::Color).0, UVec2::splat(DIFFUSE_SIDE));
    let (open, dark) = (load(PBR_FILES[0], TextureUsage::Linear), load(PBR_FILES[2], TextureUsage::Linear));
    assert_eq!((open.0, open.1), (UVec2::splat(ORM_SIDE), TextureFormat::Rgba8Unorm));
    // The two ORMs differ in red only.
    assert_eq!((&open.2[..4], &dark.2[..4]), (&ORM_OPEN[..], &ORM_DARK[..]));
    assert_eq!(ORM_OPEN[1..], ORM_DARK[1..]);
    assert_ne!(ORM_OPEN[0], ORM_DARK[0]);
    for path in N_PATHS {
        let n = load(path, TextureUsage::NormalTwoChannel);
        assert_eq!((n.0, n.1), (UVec2::splat(NORMAL_SIDE), TextureFormat::Bc5RgUnorm));
        assert_eq!(&n.2[..16], &[NORMAL_XY[0], 0, 0, 0, 0, 0, 0, 0, NORMAL_XY[1], 0, 0, 0, 0, 0, 0, 0]);
    }
    let reference = load("textures\\field\\reference_n.dds", TextureUsage::Normal);
    assert_eq!((reference.1, &reference.2[..4]), (TextureFormat::Rgba8Unorm, &NORMAL_RGB[..]));
    // The two normals are the same vector: X 0, Y 0.6, Z 0.8 (to a byte).
    let unbias = |b: u8| f32::from(b) / 255.0 * 2.0 - 1.0;
    let (x, y) = (unbias(NORMAL_XY[0]), unbias(NORMAL_XY[1]));
    let z = (1.0 - x * x - y * y).max(0.0).sqrt();
    assert!((z - unbias(NORMAL_RGB[2])).abs() < 0.01 && y > 0.5, "non-flat, with a Y component");
    // The light has a Y component, so the normal's Y (and its sign) changes N·L.
    let l = LIGHT_FROM.normalize();
    let (up, down) = (Vec3::new(x, y, z).dot(l), Vec3::new(x, -y, z).dot(l));
    assert!((up - down).abs() > 0.3 && (up - l.z).abs() > 0.05 && (down - l.z).abs() > 0.05, "{up} {down} {}", l.z);
    // Without BC support the two channels are stored as they are.
    let mut plain = TextureCache::default();
    let unsupported = CompressedImageFormatSupport(CompressedImageFormats::empty());
    let (h, _) = plain.exact(N_PATHS[0], TextureUsage::NormalTwoChannel, &data, &mut images, Some(&unsupported)).unwrap();
    let i = images.get(&h).unwrap();
    assert_eq!((i.texture_descriptor.format, &i.data.as_ref().unwrap()[..2]), (TextureFormat::Rg8Unorm, &NORMAL_XY[..]));
}

/// The scene is built from the real material source and passes its own checks (the run's
/// setting is on in tests), the `_n` handles being the worker's.
#[test]
fn the_scene_builds_and_passes_its_material_checks_with_and_without_bc() {
    for formats in [CompressedImageFormats::BC, CompressedImageFormats::empty()] {
        let owned = Owned::new();
        let data = owned_data(&owned.0, Arc::new(fnv_nif::Schema::default())).unwrap();
        let mut world = World::new();
        world.init_resource::<Assets<Image>>();
        world.init_resource::<Assets<Mesh>>();
        world.init_resource::<Assets<NifMaterial>>();
        world.insert_resource(CompressedImageFormatSupport(formats));
        let scene = make_scene(&mut world, &data, true).unwrap_or_else(|e| panic!("{formats:?}: {e}"));
        assert_eq!(scene.entities.len(), PANELS + 2);
        let materials = world.resource::<Assets<NifMaterial>>();
        assert_eq!(materials.iter().filter(|(_, m)| crate::authored::is_authored(m)).count(), 4, "lit, dark, reference and flat");
    }
    let mut world = World::new();
    world.init_resource::<Assets<Image>>();
    let owned = Owned::new();
    let data = owned_data(&owned.0, Arc::new(fnv_nif::Schema::default())).unwrap();
    assert!(make_scene(&mut world, &data, true).err().is_some_and(|e| e.contains("compressed format support")), "the actual support is required");
}

/// A readback whose judged regions have these mean luminances (grey texels), alpha 255.
fn frame(means: [u8; PANELS]) -> Vec<u8> {
    let mut b = vec![0u8; WIDTH * HEIGHT * 4];
    for (i, p) in b.as_chunks_mut::<4>().0.iter_mut().enumerate() {
        let v = means[(i % WIDTH) / PANEL];
        *p = [v, v, v, 255];
    }
    b
}

#[test]
fn the_comparator_needs_occlusion_alone_the_reference_and_the_normals_sign() {
    // plain, lit, dark, reference, flat; the shape of the observed readbacks.
    let on = frame([150, 120, 70, 121, 150]);
    let r = compare(&on, true).unwrap();
    assert_eq!(r.brightest, [150, 120, 70, 121, 150]);
    assert!(r.describe().contains("lit-dark=50.00") && r.describe().contains("flat-reference=29.00"), "{}", r.describe());
    assert!(compare(&on, false).is_err(), "off wants the plain-material panels equal");
    assert!(compare(&frame([150, 120, 115, 121, 150]), true).is_err(), "the occlusion does not show");
    assert!(compare(&frame([150, 120, 70, 140, 150]), true).is_err(), "the two-channel normal differs from the reference");
    assert!(compare(&frame([150, 120, 70, 121, 125]), true).is_err(), "the normal does not show");
    assert!(compare(&frame([150, 160, 110, 161, 130]), true).is_err(), "the normal's sign reversed for every map alike");
    // No set applied at all (every authored panel as plain, the reference only its normal):
    // refused when the setting is on.
    assert!(compare(&frame([150, 150, 150, 130, 150]), true).is_err(), "no occlusion: refused");
    assert!(compare(&frame([150, 151, 149, 130, 150]), false).is_ok());
    assert!(compare(&frame([150, 140, 150, 130, 150]), false).is_err(), "off: a plain-material panel differs");
    assert!(compare(&frame([150; PANELS]), false).is_err(), "off: identical panels mean the reference normal vanished");
    assert!(compare(&frame([150, 151, 149, 170, 150]), false).is_err(), "off: the reference brighter, its normal's sign reversed");
    // Blank, clipped, translucent or short frames are refused whatever the setting.
    assert!(compare(&frame([0; PANELS]), false).err().is_some_and(|e| e.contains("blank")));
    let mut clipped = frame([150, 151, 149, 130, 150]);
    let i = (64 * WIDTH + 64) * 4;
    clipped[i] = 255;
    assert!(compare(&clipped, false).err().is_some_and(|e| e.contains("clipped")));
    let mut clear = frame([150, 151, 149, 130, 150]);
    clear[3] = 0;
    assert!(compare(&clear, false).is_err());
    assert!(compare(&clear[..16], false).is_err());
}

#[test]
fn the_aid_owns_its_layer() {
    assert!(foreign_layer(&RenderLayers::layer(LAYER), None));
    assert!(!foreign_layer(&RenderLayers::layer(0), None));
    assert_ne!(LAYER, 31, "apart from the BC5 field's layer");
}
