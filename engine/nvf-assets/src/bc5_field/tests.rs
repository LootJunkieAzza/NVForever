use super::*;

struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!("nvf-bc5-field-{}-{}", std::process::id(), NEXT.fetch_add(1, Ordering::Relaxed)));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) { let _ = fs::remove_dir_all(&self.0); }
}

fn schema() -> Arc<fnv_nif::Schema> {
    Arc::new(fnv_nif::Schema::load_dir(&Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas/nif")).unwrap())
}

#[test]
fn authored_files_are_mounted_and_actual_nif_material_uses_the_normal_role() {
    let dir = Scratch::new();
    write_fixture(&dir.0).unwrap();
    let data = owned_data(&dir.0, schema()).unwrap();
    assert_eq!(data.vfs.archives().count(), 0);
    assert_eq!(data.vfs.paths(|_| true).len(), 4);
    assert!(data.vfs.read("textures\\not-authored.dds").is_none());
    let mut world = World::new();
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<NifMaterial>>();
    let scene = make_scene(&mut world, &data).unwrap();
    assert_eq!(scene.entities.len(), 5);
    let mut material_ids = Vec::new();
    for (i, &entity) in scene.entities[..3].iter().enumerate() {
        let mesh = &world.get::<Mesh3d>(entity).unwrap().0;
        assert!(world.resource::<Assets<Mesh>>().get(mesh).unwrap().attribute(Mesh::ATTRIBUTE_TANGENT).is_some());
        let material = &world.get::<MeshMaterial3d<NifMaterial>>(entity).unwrap().0;
        let material = world.resource::<Assets<NifMaterial>>().get(material).unwrap();
        assert!(!material.base.unlit);
        assert!(material.base.flip_normal_map_y);
        let handle = material.base.normal_map_texture.as_ref().unwrap();
        material_ids.push(handle.id());
        let actual = world.resource::<Assets<Image>>().get(handle).unwrap();
        assert_eq!(actual.texture_descriptor.format, TextureFormat::Rgba8Unorm);
        let expected = if i < 2 { fixture()[1].1[128..].to_vec() } else { [128, 128, 255, 255].repeat(64) };
        assert_eq!(actual.data.as_ref().unwrap(), &expected);
    }
    assert_ne!(material_ids[0], material_ids[1]);
    assert_ne!(material_ids[1], material_ids[2]);
    let camera = scene.entities[4];
    assert_eq!(world.get::<Exposure>(camera).unwrap().ev100, 0.0);
    assert_eq!(world.get::<AmbientLight>(camera).unwrap().brightness, 0.0);
    assert_eq!(world.get::<RenderLayers>(camera), Some(&RenderLayers::layer(LAYER)));
    assert!(check_layer(&mut world).is_ok());
    let foreign = world.spawn(RenderLayers::layer(LAYER)).id();
    assert!(check_layer(&mut world).is_err());
    world.despawn(foreign);
    for entity in scene.entities { assert!(world.despawn(entity)); }
}

#[test]
fn missing_or_different_authored_files_are_refused_and_outputs_never_overwritten() {
    let dir = Scratch::new();
    write_fixture(&dir.0).unwrap();
    assert!(write_fixture(&dir.0).is_err());
    let path = dir.0.join("textures/signed.dds");
    let original = fs::read(&path).unwrap();
    assert!(create_file(&path, b"replacement").is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    let mut wrong = original;
    wrong[128] = 1;
    fs::write(&path, wrong).unwrap();
    assert!(owned_data(&dir.0, schema()).err().unwrap().contains("different owned VFS"));
    fs::remove_file(&path).unwrap();
    assert!(owned_data(&dir.0, schema()).err().unwrap().contains("missing owned VFS"));
}

fn pixels() -> Vec<u8> {
    let mut pixels = [128u8, 128, 128, 255].repeat(WIDTH * HEIGHT);
    for (q, &(x, y)) in STARTS.iter().enumerate() {
        for y in y..y + ROI { for x in x..x + ROI {
            for panel in 0..2 {
                let offset = (y * WIDTH + x + panel * 128) * 4;
                pixels[offset..offset + 3].fill(70 + q as u8 * 10);
            }
        } }
    }
    pixels
}

#[test]
fn fixed_comparator_rejects_absence_flatness_mismatch_and_bad_alpha() {
    let good = pixels();
    assert!(compare(&good).is_ok());
    assert!(compare(&good[..good.len() - 1]).is_err());
    assert!(compare(&[0u8, 0, 0, 255].repeat(WIDTH * HEIGHT)).is_err());
    assert!(compare(&[255u8; 4].repeat(WIDTH * HEIGHT)).is_err());
    assert!(compare(&[128u8, 128, 128, 255].repeat(WIDTH * HEIGHT)).is_err());
    let mut bad = good.clone();
    bad[3] = 254;
    assert!(compare(&bad).is_err());
    let mut bad = good.clone();
    bad[(16 * WIDTH + 16) * 4] += 2;
    assert!(compare(&bad).is_err());
    let mut allowed = good;
    allowed[(16 * WIDTH + 16) * 4] += 1;
    assert!(compare(&allowed).is_ok());
}

#[test]
fn regions_avoid_boundaries_and_one_discriminating_quadrant_is_insufficient() {
    for &(left, top) in &STARTS {
        assert_eq!(left / 64, (left + ROI - 1) / 64);
        assert_eq!(top / 64, (top + ROI - 1) / 64);
        assert!(left % 64 >= 8 && (left + ROI) % 64 <= 56);
        assert!(top % 64 >= 8 && (top + ROI) % 64 <= 56);
        let sums = (left + top + 1, left + top + 2 * ROI - 1);
        assert!(sums.1 < 128 || sums.0 > 128);
    }
    let mut bytes = [128u8, 128, 128, 255].repeat(WIDTH * HEIGHT);
    for (index, &(left, top)) in STARTS.iter().take(2).enumerate() {
        for y in top..top + ROI { for x in left..left + ROI { for panel in 0..2 {
            let i = (y * WIDTH + x + panel * 128) * 4;
            bytes[i..i + 3].fill(120);
        } } }
        assert_eq!(compare(&bytes).is_ok(), index == 1);
    }
}

#[test]
fn failed_readbacks_are_retained_without_replacing_prior_evidence() {
    let dir = Scratch::new();
    let bad = [0u8, 0, 0, 255].repeat(WIDTH * HEIGHT);
    let result = compare(&bad);
    assert!(result.is_err());
    save_readback(&dir.0, 1, 12, &bad, &result).unwrap();
    assert_eq!(fs::read(dir.0.join("readback-1.rgba")).unwrap(), bad);
    assert!(fs::read_to_string(dir.0.join("readback-1.txt")).unwrap().contains("fixed comparator"));
    assert!(save_readback(&dir.0, 1, 13, &pixels(), &compare(&pixels())).is_err());
    assert_eq!(fs::read(dir.0.join("readback-1.rgba")).unwrap(), bad);
}

#[test]
fn observer_accepts_only_its_pending_event_and_cleanup_retires_only_its_scene() {
    // This exercises event ownership/state/cleanup, not a GPU or rendered result.
    let dir = Scratch::new();
    let mut world = World::new();
    let frames = RenderFrames::default();
    frames.0.store(10, Ordering::Release);
    world.insert_resource(frames);
    let owned = world.spawn(FieldOwned).id();
    let neighbor = world.spawn_empty().id();
    let first = world.spawn(FieldOwned).observe(received).id();
    let foreign = world.spawn_empty().observe(received).id();
    world.insert_resource(Field {
        started: Some(Instant::now()), output: Some(dir.0.clone()),
        scene: Some(Scene { entities: vec![owned], target: Handle::default() }),
        pending: Some(first), completed: 0, after_frame: 0, done: false,
    });
    world.trigger(ReadbackComplete { entity: foreign, data: pixels() });
    assert_eq!(world.resource::<Field>().completed, 0);
    assert_eq!(world.resource::<Field>().pending, Some(first));
    world.trigger(ReadbackComplete { entity: first, data: pixels() });
    world.flush();
    assert_eq!(world.resource::<Field>().completed, 1);
    assert_eq!(world.resource::<Field>().after_frame, 10);
    assert!(!world.resource::<Field>().done);
    let second = world.spawn(FieldOwned).observe(received).id();
    world.resource_mut::<Field>().pending = Some(second);
    world.resource::<RenderFrames>().0.store(20, Ordering::Release);
    world.trigger(ReadbackComplete { entity: second, data: pixels() });
    world.flush();
    assert_eq!(world.resource::<Field>().completed, 2);
    assert!(world.resource::<Field>().done);
    tick(&mut world);
    assert!(world.get_entity(owned).is_err());
    assert!(world.get_entity(first).is_err());
    assert!(world.get_entity(second).is_err());
    assert!(world.get_entity(neighbor).is_ok());
    assert!(world.get_entity(foreign).is_ok());
    assert!(dir.0.join("readback-1.rgba").is_file());
    assert!(dir.0.join("readback-2.rgba").is_file());
}

#[test]
fn foreign_layer_entering_after_second_request_prevents_success_and_survives_cleanup() {
    // Valid comparator bytes cannot certify a field with a new foreign owner.
    let dir = Scratch::new();
    let mut world = World::new();
    world.insert_resource(RenderFrames::default());
    let owned = world.spawn(FieldOwned).id();
    let second = world.spawn(FieldOwned).observe(received).id();
    world.insert_resource(Field {
        started: Some(Instant::now()), output: Some(dir.0.clone()),
        scene: Some(Scene { entities: vec![owned], target: Handle::default() }),
        pending: Some(second), completed: 1, after_frame: 10, done: false,
    });
    // No advance/update between the foreign insertion and the completion.
    let foreign = world.spawn(RenderLayers::layer(LAYER)).id();
    let bytes = pixels();
    assert!(compare(&bytes).is_ok());
    world.trigger(ReadbackComplete { entity: second, data: bytes.clone() });
    world.flush();
    assert!(world.resource::<Field>().done);
    assert_eq!(world.resource::<Field>().completed, 1, "a second PASS must not be credited");
    assert!(world.resource::<Field>().pending.is_none());
    assert!(fs::read_to_string(dir.0.join("readback-2.txt")).unwrap().contains("foreign recipient"));
    assert_eq!(fs::read(dir.0.join("readback-2.rgba")).unwrap(), bytes);
    tick(&mut world);
    assert!(world.get_entity(owned).is_err());
    assert!(world.get_entity(second).is_err());
    assert!(world.get_entity(foreign).is_ok());
}


#[test]
fn repeated_registered_launches_initialize_without_replacing_evidence() {
    // The actual smoke loader/plan/run_launch starts a fresh Rust process twice.
    // That child calls advance -> owned_data -> make_scene, with no render plugin.
    let dir = Scratch::new();
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tools/smoke/tests/test_bc5_field.py");
    let output = fs::File::create(dir.0.join("launcher.log")).unwrap();
    let python = std::env::var_os("PYTHON").unwrap_or_else(|| if cfg!(windows) { "python".into() } else { "python3".into() });
    let mut child = std::process::Command::new(python)
        .arg(script).arg("--rust-child").arg(std::env::current_exe().unwrap()).arg(&dir.0)
        .stdout(output.try_clone().unwrap()).stderr(output).spawn().unwrap();
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() { break status; }
        if start.elapsed() >= Duration::from_secs(50) {
            child.kill().unwrap();
            child.wait().unwrap();
            panic!("registered launcher exceeded its bounded initialization deadline");
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    let log = fs::read_to_string(dir.0.join("launcher.log")).unwrap();
    println!("{log}");
    assert!(status.success(), "real registered repeated-launch control failed: {log}");
    assert!(log.contains("two actual registered launches initialized distinct field recipients"), "{log}");
}

#[test]
#[ignore = "fresh process entered only by repeated_registered_launches_initialize_without_replacing_evidence"]
fn registered_launcher_child_initializes_actual_field() {
    assert_eq!(std::env::var("NVF_PLAY_TEST_BC5_FIELD").as_deref(), Ok("1"));
    let home = PathBuf::from(std::env::var_os("NVFOREVER_HOME").unwrap());
    assert_eq!(home, PathBuf::from("bc5-field-home"));
    fs::create_dir_all(&home).unwrap();
    let mut world = World::new();
    world.insert_resource(GameData { schema: schema(), vfs: Arc::new(fnv_vfs::Vfs::open(&home, &[]).unwrap()) });
    world.init_resource::<Assets<Image>>();
    world.init_resource::<Assets<Mesh>>();
    world.init_resource::<Assets<NifMaterial>>();
    world.insert_resource(RenderFrames::default());
    let mut field = Field { started: None, output: None, scene: None, pending: None, completed: 0, after_frame: 0, done: false };
    let result = advance(&mut world, &mut field, Duration::ZERO);
    assert!(result.is_ok(), "actual advance initialization failed: {result:?}");
    assert_eq!(field.scene.as_ref().unwrap().entities.len(), 5);
    assert!(field.pending.is_none());
    assert_eq!(field.completed, 0);
    let out = field.output.as_ref().unwrap();
    create_file(&out.join("cpu-initialization.txt"), b"actual field initialization only; no GPU or readback\n").unwrap();
    println!("\nBC5_FIELD_CHILD_OUTPUT={}", std::env::current_dir().unwrap().join(out).display());
}

#[test]
fn output_reservation_preserves_legacy_files_and_bounds_collisions() {
    let dir = Scratch::new();
    let root = dir.0.join("bc5-field-output");
    fs::create_dir(&root).unwrap();
    create_file(&root.join("readback-1.rgba"), b"legacy retained evidence").unwrap();
    for ordinal in 0..64 {
        let output = new_output(&dir.0, 1, 2).unwrap();
        assert_eq!(output, root.join(format!("run-1-2-{ordinal}")));
        create_file(&output.join("sentinel"), b"retained").unwrap();
    }
    assert!(new_output(&dir.0, 1, 2).unwrap_err().to_string().contains("collision limit"));
    assert_eq!(fs::read(root.join("readback-1.rgba")).unwrap(), b"legacy retained evidence");
    for ordinal in 0..64 {
        assert_eq!(fs::read(root.join(format!("run-1-2-{ordinal}/sentinel"))).unwrap(), b"retained");
    }
    let other = Scratch::new();
    create_file(&other.0.join("bc5-field-output"), b"foreign file").unwrap();
    assert!(new_output(&other.0, 1, 2).is_err());
    assert_eq!(fs::read(other.0.join("bc5-field-output")).unwrap(), b"foreign file");
}
