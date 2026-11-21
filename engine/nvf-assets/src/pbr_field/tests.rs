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
    // The height panel's set is height only; its control has none.
    assert_eq!(index.lookup("textures\\field\\height.dds").unwrap().state(), fnv_pbr::SetState::HeightOnly);
    assert!(index.lookup("textures\\field\\heightcontrol.dds").is_none());
    assert_eq!(data.pbr_scalars("textures\\pbr\\field\\height.toml").parallax_depth, Some(HEIGHT_DEPTH));
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
    // The `_p`: unsigned BC4, high (255) blocks above, low (0) below; the stripe diffuse.
    let height = load(HEIGHT_PATH, TextureUsage::Linear);
    assert_eq!((height.0, height.1), (UVec2::splat(HEIGHT_SIDE), TextureFormat::Bc4RUnorm));
    let last = height.2.len() - 8;
    assert_eq!((height.2[0], height.2[last]), (255, 0));
    let striped = load("textures\\field\\height.dds", TextureUsage::Color);
    assert_eq!(striped.0, UVec2::splat(STRIPE_SIDE));
    let texel = |x: usize| &striped.2[x * 4..x * 4 + 4];
    assert_eq!((texel(31), texel(10)), (&STRIPE[..], &STRIPE_GROUND[..]));
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
        assert_eq!(scene.entities.len(), TOTAL + 2);
        let materials = world.resource::<Assets<NifMaterial>>();
        assert_eq!(materials.iter().filter(|(_, m)| crate::authored::is_authored(m)).count(), 4, "lit, dark, reference and flat (the height-only set leaves its panel unmarked)");
        assert_eq!(materials.iter().filter(|(_, m)| m.base.depth_map.is_some()).count(), 1, "the height panel alone has a depth map");
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
        let v = means.get((i % WIDTH) / PANEL).copied().unwrap_or(100);
        *p = [v, v, v, 255];
    }
    b
}

/// A frame whose height panels show a 5-pixel stripe of `value` on a ground of 50: the
/// control's down the middle, the height panel's moved by `shift` pixels in its high and low
/// band.
fn stripes(shift: [i64; 2], value: u8) -> Vec<u8> {
    let mut b = frame([150, 151, 149, 130, 150]);
    for (panel, offsets) in [(HEIGHT_PANEL, shift), (CONTROL_PANEL, [0, 0])] {
        for y in 0..HEIGHT {
            let offset = if y < HEIGHT / 2 { offsets[0] } else { offsets[1] };
            let centre = PANEL as i64 / 2 + offset;
            for x in 0..PANEL {
                let v = if ((centre - 2)..=(centre + 2)).contains(&(x as i64)) { value } else { 50 };
                let i = (y * WIDTH + panel * PANEL + x) * 4;
                b[i..i + 4].copy_from_slice(&[v, v, v, 255]);
            }
        }
    }
    b
}

/// The height comparator: on, the low band's stripe must move by the predicted amount (within
/// the tolerance) and the high band's stay; off, neither moves; a wrong sign, no movement, a
/// wrong amount, a moved high band, a clipped frame, a missing stripe or a crooked control
/// fail. The prediction is the one at the field's depth (0.08, the scalar range's end).
#[test]
fn the_height_comparator_wants_the_predicted_shift_in_the_low_band_only() {
    let predicted = predicted_low_shift();
    assert!((predicted + 7.84).abs() < 0.01, "the prediction at depth 0.08: {predicted}");
    assert_eq!(HEIGHT_DEPTH, *fnv_pbr::PARALLAX_DEPTH_RANGE.end(), "the field's depth is the range's end, so its scalar file is not clamped");
    let way = predicted.signum() as i64;
    let judged = |shift: [i64; 2], on: bool| judge_height(&measure(&stripes(shift, 180)).unwrap().height, on);
    let r = measure(&stripes([0, 8 * way], 180)).unwrap().height;
    let [high, low] = r.shifts().unwrap();
    assert!(high.abs() < 0.01 && (low - 8.0 * way as f64).abs() < 0.01, "{}", r.describe());
    assert!((r.control[0].unwrap() - 64.5).abs() < 0.01, "pixel centres: {}", r.describe());
    assert!(r.brightest == 180 && (r.ground - 50.0).abs() < 1e-9, "{}", r.describe());
    assert!(r.describe().contains("high=+0.00 px") && r.describe().contains("predicted low -7.84 px") && r.describe().contains("brightest 180, ground 50.0"), "{}", r.describe());
    assert!(judged([0, 8 * way], true).is_ok());
    assert!(judged([1, 7 * way], true).is_ok());
    assert!(judged([0, -8 * way], true).err().is_some_and(|e| e.contains("wrong way")), "a wrong sign fails");
    assert!(judged([0, 0], true).err().is_some_and(|e| e.contains("did not move")), "on: no movement fails");
    assert!(judged([0, way], true).err().is_some_and(|e| e.contains("did not move")));
    assert!(judged([0, 5 * way], true).err().is_some_and(|e| e.contains("predicted amount")), "too little");
    assert!(judged([0, 11 * way], true).err().is_some_and(|e| e.contains("predicted amount")), "too far");
    assert!(judged([2, 8 * way], true).err().is_some_and(|e| e.contains("high band")));
    assert!(judged([0, 0], false).is_ok(), "off: both stripes straight");
    assert!(judged([0, 8 * way], false).err().is_some_and(|e| e.contains("setting off")), "off: the low band must not move");
    // A clipped frame fails, whatever the shifts.
    assert!(judge_height(&measure(&stripes([0, 8 * way], 255)).unwrap().height, true).err().is_some_and(|e| e.contains("clipped")));
    // No stripe at all (uniform height panels).
    assert!(judge_height(&measure(&frame([150, 151, 149, 130, 150])).unwrap().height, false).err().is_some_and(|e| e.contains("no stripe")));
    // The observed readback (b4660af00, setting on, at the depth of then, 0.1: low -9.76
    // against -9.81): judged off it fails, and as observed (clipped) it fails on the clip.
    // It is not judged on against today's prediction, which is for another depth.
    let observed = HeightReport { height: [Some(64.0), Some(54.24)], control: [Some(64.0), Some(64.0)], brightest: 190, ground: 40.0 };
    assert!(judge_height(&observed, false).is_err());
    assert!(judge_height(&HeightReport { brightest: 255, ..observed }, true).err().is_some_and(|e| e.contains("clipped")), "as it was observed: clipped");
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
