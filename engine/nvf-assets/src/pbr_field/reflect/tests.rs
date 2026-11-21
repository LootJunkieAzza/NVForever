//! The reflection field's fixture, scene and comparator, without a GPU.

use super::*;
use std::fs;

/// A fresh folder under the temp folder with the field's generated files.
struct Owned(PathBuf);

impl Owned {
    fn new() -> Owned {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let root = std::env::temp_dir().join(format!("nvf-pbr-reflect-field-{}-{stamp}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
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
fn the_cube_holds_the_faces_at_level_0_and_their_mean_at_the_last() {
    for (_, axis, colour) in FACES {
        assert!(cube_colour(axis, 0).distance(colour) < 1e-6);
        assert!(cube_colour(axis, SPECULAR_LEVELS - 1).distance(MEAN) < 1e-6);
        // Off-centre directions keep their face.
        let tilted = (axis + 0.3 * axis.any_orthonormal_vector()).normalize();
        assert!(cube_colour(tilted, 0).distance(colour) < 1e-6);
    }
    // The faces' mean is the neutral MEAN.
    let mean = FACES.iter().map(|f| f.2).sum::<Vec3>() / 6.0;
    assert!(mean.distance(MEAN) < 1e-6);
}

#[test]
fn each_panel_reflects_the_view_into_its_direction() {
    // Orthographic view along -Z: V = +Z; R = reflect(-V, N).
    let v = Vec3::Z;
    for panel in &PANELS {
        let n = panel_normal(panel.reflect);
        let r = -v - 2.0 * (-v).dot(n) * n;
        assert!(r.distance(panel.reflect) < 1e-5, "{}: {r}", panel.name);
        assert!(n.dot(v) > 0.5, "{}: faces the camera", panel.name);
        // The rotation the scene gives it carries the quad's +Z normal there.
        let q = Quat::from_rotation_arc(Vec3::Z, n);
        assert!((q * Vec3::Z).distance(n) < 1e-5);
    }
    // Every cell is used once and the cube's faces but -Z are reflected (-Z cannot be seen
    // from a camera on +Z: its own colour pins the z flip through the +Z panel).
    let mut cells: Vec<_> = PANELS.iter().map(|p| p.cell).collect();
    cells.sort();
    cells.dedup();
    assert_eq!(cells.len(), PANELS.len());
    for name in ["+X", "-X", "+Y", "-Y", "+Z"] {
        assert!(PANELS[ORIENTED].iter().any(|p| face_name(p.reflect) == name), "{name}");
    }
}

/// The scene builds from the real material source (the run's Authored materials setting is
/// on in tests, the reflections switch off: metallic factor 0).
#[test]
fn the_scene_builds_and_passes_its_material_checks() {
    let owned = Owned::new();
    let data = owned_data_of(&owned.0, Arc::new(fnv_nif::Schema::default()), &fixture()).unwrap();
    let mut world = World::new();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<NifMaterial>>();
    world.insert_resource(CompressedImageFormatSupport(bevy::image::CompressedImageFormats::BC));
    let scene = make_scene(&mut world, &data, false).unwrap_or_else(|e| panic!("{e}"));
    assert_eq!(scene.entities.len(), PANELS.len() + 2);
    assert!(world.get::<ReflectionCube>(scene.camera_a).is_some_and(|c| c.generation() == 1), "camera A carries the filled cube");
    let materials = world.resource::<Assets<NifMaterial>>();
    assert_eq!(materials.iter().filter(|(_, m)| crate::authored::is_authored(m)).count(), PANELS.len() - 1, "all but the legacy control");
    assert_eq!(materials.iter().filter(|(_, m)| m.base.reflectance == 0.0).count(), 1, "the zero-reflectance control");
    assert!(materials.iter().filter(|(_, m)| crate::authored::is_authored(m)).all(|(_, m)| m.base.metallic == crate::authored::authored_metallic()));
}

fn grey(v: f64) -> [f64; 3] {
    [v; 3]
}

/// The bytes of a frame whose panels' judged regions hold these colours, alpha 255.
fn frame(colours: &[[f64; 3]; PANELS.len()]) -> Vec<u8> {
    let mut b = vec![0u8; WIDTH * HEIGHT * 4];
    for (i, panel) in PANELS.iter().enumerate() {
        let (col, row) = panel.cell;
        for y in 0..CELL {
            for x in 0..CELL {
                let at = ((row * CELL + y) * WIDTH + col * CELL + x) * 4;
                for (dst, v) in b[at..at + 3].iter_mut().zip(colours[i]) {
                    *dst = v.round() as u8;
                }
            }
        }
    }
    for p in b.as_chunks_mut::<4>().0 {
        p[3] = 255;
    }
    b
}

/// Camera B's expected shape: the white panels one grey, the bronze ones that grey times the
/// bronze.
fn expected_b() -> [[f64; 3]; PANELS.len()] {
    let mut b = [grey(83.0); PANELS.len()];
    for i in BRONZE_PANELS {
        b[i] = [33.0, 25.0, 17.0];
    }
    b
}

/// The expected shape of camera A's readback with the switch on (module constants' notes):
/// metals at F_ab sums of about 0.89, 0.73 and 0.51 times the cube's level colours; the bronze
/// at gain 1 (and its `_albedo` twin) and lifted 2.49 times.
fn expected_a() -> [[f64; 3]; PANELS.len()] {
    let metal = |d: Vec3, level: f32, fab: f32| {
        let s = level / (SPECULAR_LEVELS - 1) as f32;
        let face = FACES.iter().find(|f| f.1 == d).unwrap().2;
        let c = face.lerp(MEAN, s) * fab * 255.0;
        [f64::from(c.x), f64::from(c.y), f64::from(c.z)]
    };
    let mut a = [grey(83.0); PANELS.len()];
    for i in ORIENTED {
        a[i] = metal(PANELS[i].reflect, 1.0, 0.89);
    }
    a[ROUGHNESS[1]] = metal(Vec3::Z, 2.5, 0.73);
    a[ROUGHNESS[2]] = metal(Vec3::Z, 4.5, 0.51);
    a[DIELECTRIC] = [84.0, 84.0, 87.0];
    a[LIFT1] = [16.7, 12.5, 17.7];
    a[PAINTED] = a[LIFT1];
    a[LIFT] = a[LIFT1].map(|v| v * 2.49);
    a
}

#[test]
fn the_comparator_passes_the_expected_shape_and_refuses_each_fault() {
    let b = measure(&frame(&expected_b())).unwrap();
    let a = measure(&frame(&expected_a())).unwrap();
    judge(&a, &b, true).unwrap_or_else(|e| panic!("{e}: {}", describe(&a, &b)));
    assert!(describe(&a, &b).contains("metal20+X->+X"), "{}", describe(&a, &b));
    // The switch-off rule refuses it (camera A differs).
    assert!(judge(&a, &b, false).is_err());
    // The z flip wrong: the head-on metals show -Z's colour.
    let mut flipped = expected_a();
    flipped[ROUGHNESS[0]] = [113.0, 113.0, 34.0];
    assert!(judge(&measure(&frame(&flipped)).unwrap(), &b, true).err().is_some_and(|e| e.contains("metal20+Z shows -Z")));
    // X faces swapped.
    let mut swapped = expected_a();
    swapped.swap(0, 1);
    assert!(judge(&measure(&frame(&swapped)).unwrap(), &b, true).is_err());
    // No level rule (every roughness the smooth level's colour).
    let mut flat = expected_a();
    flat[ROUGHNESS[1]] = flat[ROUGHNESS[0]];
    flat[ROUGHNESS[2]] = flat[ROUGHNESS[0]];
    assert!(judge(&measure(&frame(&flat)).unwrap(), &b, true).err().is_some_and(|e| e.contains("chromaticity")));
    // The legacy control reflects.
    let mut leak = expected_a();
    leak[LEGACY] = [86.0, 86.0, 92.0];
    assert!(judge(&measure(&frame(&leak)).unwrap(), &b, true).err().is_some_and(|e| e.contains("legacy differs")));
    // The zero-reflectance control reflects.
    let mut zero = expected_a();
    zero[ZERO] = [85.0, 85.0, 90.0];
    assert!(judge(&measure(&frame(&zero)).unwrap(), &b, true).err().is_some_and(|e| e.contains("zero differs")));
    // Camera B's metals black (metallic not held at 0 without a cube).
    let mut b_black = expected_b();
    for i in ORIENTED.chain(ROUGHNESS[1..].iter().copied()) {
        b_black[i] = grey(10.0);
    }
    assert!(judge(&a, &measure(&frame(&b_black)).unwrap(), true).err().is_some_and(|e| e.contains("camera B's panels differ")));
    // Nothing reflected at all (the cube never reached the metals: they draw as camera B's).
    let none = measure(&frame(&expected_b())).unwrap();
    assert!(judge(&none, &b, true).is_err());
    // Off: camera A equal to camera B passes; a difference anywhere fails.
    assert!(judge(&none, &b, false).is_ok());
    let mut one = expected_b();
    one[DIELECTRIC] = [83.0, 83.0, 86.0];
    assert!(judge(&measure(&frame(&one)).unwrap(), &b, false).err().is_some_and(|e| e.contains("dielectric differs")));
}

#[test]
fn blank_clipped_translucent_and_short_frames_are_refused() {
    assert!(measure(&frame(&[grey(0.0); PANELS.len()])).err().is_some_and(|e| e.contains("blank")));
    let mut clipped = frame(&[grey(83.0); PANELS.len()]);
    let at = (64 * WIDTH + 64) * 4;
    clipped[at] = 255;
    assert!(measure(&clipped).err().is_some_and(|e| e.contains("clipped")));
    let mut clear = frame(&[grey(83.0); PANELS.len()]);
    clear[3] = 0;
    assert!(measure(&clear).is_err());
    assert!(measure(&clear[..16]).is_err());
}

#[test]
fn the_aid_owns_its_layer() {
    assert!(foreign_layer(&RenderLayers::layer(LAYER), None));
    assert!(!foreign_layer(&RenderLayers::layer(0), None));
    assert!(![29, 30, 31].contains(&LAYER), "apart from the culling, PBR and BC5 field layers");
}

/// Row three (metal base colour): the lift must brighten the bronze, keep its hue, and the
/// `_albedo` must draw as the bronze, on camera A; on camera B the bronze panels agree.
#[test]
fn the_comparator_refuses_a_missing_lift_a_hue_shift_and_an_ignored_albedo() {
    let b = measure(&frame(&expected_b())).unwrap();
    let a = measure(&frame(&expected_a())).unwrap();
    judge(&a, &b, true).unwrap_or_else(|e| panic!("{e}: {}", describe(&a, &b)));
    assert!(describe(&a, &b).contains("lift A ratio=2.4"), "{}", describe(&a, &b));
    // No lift (the gain not reaching the shader).
    let mut flat = expected_a();
    flat[LIFT] = flat[LIFT1];
    assert!(judge(&measure(&frame(&flat)).unwrap(), &b, true).err().is_some_and(|e| e.contains("lifted metal")));
    // The experiment's per-channel clip: min(bronze x 12, 1) is a white metal.
    let mut clipped = expected_a();
    clipped[LIFT] = [41.9, 41.9, 88.4];
    assert!(judge(&measure(&frame(&clipped)).unwrap(), &b, true).err().is_some_and(|e| e.contains("moved the hue")));
    // The `_albedo` ignored: the white diffuse draws as the white metal at roughness 0.5.
    let mut ignored = expected_a();
    ignored[PAINTED] = ignored[ROUGHNESS[1]];
    assert!(judge(&measure(&frame(&ignored)).unwrap(), &b, true).err().is_some_and(|e| e.contains("albedo differs")));
    // Camera B: the `_albedo` not drawn without a cube either (the albedo panel white).
    let mut b_white = expected_b();
    b_white[PAINTED] = grey(83.0);
    let b_white = measure(&frame(&b_white)).unwrap();
    assert!(judge(&a, &b_white, true).err().is_some_and(|e| e.contains("bronze panels differ")));
    assert!(judge(&measure(&frame(&expected_b())).unwrap(), &b_white, false).is_err(), "the switch-off run judges camera B's bronze too");
    // The chromaticity distance ignores brightness.
    assert!(chroma_between([10.0, 20.0, 30.0], [20.0, 40.0, 60.0]) < 1e-12);
    assert!(chroma_between([10.0, 20.0, 30.0], [30.0, 20.0, 10.0]) > 0.2);
}

/// The mount pack's home: an explicit, non-empty `NVFOREVER_HOME` only (empty means the
/// player's default home to fnv-install).
#[test]
fn the_mount_pack_needs_an_explicit_home() {
    assert!(mount_pack_home(None).is_err());
    assert!(mount_pack_home(Some("".into())).is_err());
    assert_eq!(mount_pack_home(Some("pbr-reflect-home".into())), Ok(PathBuf::from("pbr-reflect-home")));
}

/// The mount pack writes only into a fresh folder of its own: a second run gets another one,
/// and files already in the home (its mods folder included) are never touched.
#[test]
fn the_mount_pack_writes_only_a_fresh_folder_of_its_own() {
    let home = std::env::temp_dir().join(format!("nvf-mount-pack-{}-{}", std::process::id(), SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos()));
    let mine = home.join("mods").join("textures").join("pbr").join("reflect");
    fs::create_dir_all(&mine).unwrap();
    fs::write(mine.join("metal20_orm.dds"), b"the player's own").unwrap();
    let (first, n) = seed_mount_pack_in(&home, 7, 1).unwrap();
    let (second, _) = seed_mount_pack_in(&home, 7, 1).unwrap();
    assert_ne!(first, second, "a new folder per run");
    assert!(first.starts_with(home.join("pbr-mount-pack")));
    assert_eq!(n, fixture().len());
    for (rel, bytes) in fixture() {
        assert_eq!(fs::read(first.join(rel)).unwrap(), bytes, "{rel}");
    }
    assert_eq!(fs::read(mine.join("metal20_orm.dds")).unwrap(), b"the player's own", "the home's files untouched");
    // A file in the way inside a fresh folder cannot happen, but write_pack refuses one.
    let taken = home.join("taken");
    fs::create_dir_all(taken.join("textures").join("reflect")).unwrap();
    fs::write(taken.join(fixture()[0].0), b"there before").unwrap();
    assert!(write_pack(&taken).is_err());
    assert_eq!(fs::read(taken.join(fixture()[0].0)).unwrap(), b"there before", "never truncated");
    let _ = fs::remove_dir_all(&home);
}
