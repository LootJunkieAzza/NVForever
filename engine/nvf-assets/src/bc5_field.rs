//! Default-off authored NIF material field; contract in docs/spec-notes/dds-bc5.md.
//! The reference bytes are literal authored inputs, never a production decode result.

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::asset::RenderAssetUsages;
use bevy::camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::mesh::{Indices, PrimitiveTopology};
use bevy::prelude::*;
use bevy::render::{Render, RenderApp, RenderSystems};
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::gpu_readback::{ReadbackComplete, ReadbackOnce};
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::view::ViewTarget;

use crate::{GameData, NifMaterial, TextureCache};
use crate::nif_material::TransmissionAnchor;
use crate::nif_spawn::{Props, shape_material};

const LAYER: usize = 31;
const WIDTH: usize = 384;
const HEIGHT: usize = 128;
const ROI: usize = 24;
const STARTS: [(usize, usize); 4] = [(16, 16), (72, 8), (8, 72), (88, 88)];
const ENDPOINTS: [(i8, i8); 4] = [(64, -64), (-64, 64), (96, 0), (0, -96)];
const REFERENCE: [[u8; 4]; 4] = [[192, 63, 217, 255], [63, 192, 217, 255], [224, 128, 211, 255], [128, 31, 211, 255]];

#[derive(Component)]
struct FieldOwned;

#[derive(Component, Clone, ExtractComponent)]
#[extract_app(RenderApp)]
struct FieldCamera;

#[derive(Resource, Clone, Default)]
struct RenderFrames(Arc<AtomicU64>);

#[derive(Resource)]
struct Field {
    started: Option<Instant>,
    output: Option<PathBuf>,
    scene: Option<Scene>,
    pending: Option<Entity>,
    completed: u8,
    after_frame: u64,
    done: bool,
}

struct Scene {
    entities: Vec<Entity>,
    target: Handle<Image>,
}

pub(crate) fn install(app: &mut App) {
    let Some(value) = std::env::var_os("NVF_PLAY_TEST_BC5_FIELD") else { return };
    if value != "1" {
        error!("bc5 field: FAIL expected NVF_PLAY_TEST_BC5_FIELD=1");
        return;
    }
    let frames = RenderFrames::default();
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        error!("bc5 field: FAIL missing normal RenderApp");
        return;
    };
    render.insert_resource(frames.clone()).add_systems(Render, count_rendered.in_set(RenderSystems::Cleanup));
    app.add_plugins(ExtractComponentPlugin::<FieldCamera>::default())
        .insert_resource(frames)
        .insert_resource(Field { started: None, output: None, scene: None, pending: None, completed: 0, after_frame: 0, done: false })
        .add_systems(Update, tick);
}

// Cleanup follows the render graph and command submission. An inactive camera or
// missing target has no extracted camera/ViewTarget and does not advance this clock.
fn count_rendered(views: Query<(&ExtractedCamera, &ViewTarget), With<FieldCamera>>, frames: Res<RenderFrames>) {
    if let Ok((camera, _)) = views.single() {
        if camera.physical_target_size == Some(UVec2::new(WIDTH as u32, HEIGHT as u32)) {
            let mut count = frames.0.load(Ordering::Relaxed);
            while let Some(next) = count.checked_add(1) {
                match frames.0.compare_exchange_weak(count, next, Ordering::Release, Ordering::Relaxed) {
                    Ok(_) => break,
                    Err(current) => count = current,
                }
            }
        }
    }
}

fn tick(world: &mut World) {
    let Some(mut field) = world.remove_resource::<Field>() else { return };
    let started = *field.started.get_or_insert_with(Instant::now);
    if !field.done {
        let result = advance(world, &mut field, started.elapsed());
        if let Err(reason) = result {
            error!("bc5 field: FAIL {reason}");
            field.done = true;
        }
    }
    if field.done {
        if let Some(entity) = field.pending.take() { world.despawn(entity); }
        if let Some(scene) = field.scene.take() {
            for entity in scene.entities { world.despawn(entity); }
        }
    }
    world.insert_resource(field);
}

fn advance(world: &mut World, field: &mut Field, elapsed: Duration) -> Result<(), String> {
    if elapsed >= Duration::from_secs(30) { return Err("timeout before two completed rendered comparisons".into()); }
    // A foreign layer user observed while a readback is pending is still a
    // failed field. Do not wait for the next request to notice it.
    check_layer(world)?;
    if field.scene.is_none() {
        let Some(data) = world.get_resource::<GameData>().cloned() else { return Ok(()) };
        let home = std::env::var_os("NVFOREVER_HOME").ok_or("missing explicit owned NVFOREVER_HOME")?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("output clock: {e}"))?.as_nanos();
        let output = new_output(Path::new(&home), stamp, std::process::id()).map_err(|e| format!("fresh run output: {e}"))?;
        write_fixture(&output).map_err(|e| format!("authored fixture: {e}"))?;
        let local = owned_data(&output, data.schema.clone())?;
        field.scene = Some(make_scene(world, &local)?);
        info!("bc5 field: evidence directory {}", output.display());
        field.output = Some(output);
        info!("bc5 field: actual NIF material source, 3 authored panels, 4 quadrants");
    }
    let frame = world.resource::<RenderFrames>().0.load(Ordering::Acquire);
    if field.pending.is_none() && elapsed >= Duration::from_secs(5) && frame >= field.after_frame + 10 {
        let target = field.scene.as_ref().ok_or("missing owned scene")?.target.clone();
        let entity = world.spawn((FieldOwned, ReadbackOnce::texture(target))).observe(received).id();
        field.pending = Some(entity);
    }
    Ok(())
}

fn check_layer(world: &mut World) -> Result<(), String> {
    let mut q = world.query_filtered::<(&RenderLayers, Option<&TransmissionAnchor>), Without<FieldOwned>>();
    if q.iter(world).any(|(layers, anchor)| foreign_layer(layers, anchor)) {
        Err("foreign recipient in isolated render layer".into())
    } else { Ok(()) }
}

fn foreign_layer(layers: &RenderLayers, anchor: Option<&TransmissionAnchor>) -> bool {
    anchor.is_none() && layers.intersects(&RenderLayers::layer(LAYER))
}

fn owned_data(out: &Path, schema: Arc<fnv_nif::Schema>) -> Result<GameData, String> {
    let vfs = fnv_vfs::Vfs::open(out, &[]).map_err(|e| format!("owned VFS: {e}"))?;
    for (name, expected) in fixture() {
        let path = format!("textures\\{name}");
        let actual = vfs.read(&path).ok_or_else(|| format!("missing owned VFS {path}"))?
            .map_err(|e| format!("owned VFS {path}: {e}"))?;
        if actual != expected || vfs.source_of(&path) != Some("loose") {
            return Err(format!("different owned VFS payload {path}"));
        }
    }
    Ok(GameData::new(Arc::new(vfs), schema))
}

fn mesh() -> Mesh {
    Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::default())
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, vec![[-1.0f32, -1.0, 0.0], [1.0, -1.0, 0.0], [1.0, 1.0, 0.0], [-1.0, 1.0, 0.0]])
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, vec![[0.0f32, 0.0, 1.0]; 4])
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, vec![[0.0f32, 1.0], [1.0, 1.0], [1.0, 0.0], [0.0, 0.0]])
        .with_inserted_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]))
}

fn make_scene(world: &mut World, data: &GameData) -> Result<Scene, String> {
    let mut cache = TextureCache::default();
    let prepared = world.resource_scope(|_, mut images: Mut<Assets<Image>>| {
        let mut prepared = Vec::with_capacity(3);
        for normal in ["signed.dds", "reference.dds", "flat.dds"] {
            let mut mesh = mesh();
            let mut props = Props::default();
            props.textures = vec!["white.dds".into(), normal.into()];
            let (material, _) = shape_material(&props, &mut mesh, &mut cache, data, &mut images, None);
            let normal_image = material.base.normal_map_texture.as_ref().and_then(|h| images.get(h));
            if material.base.base_color_texture.is_none() || normal_image.is_none()
                || material.base.unlit || !material.base.flip_normal_map_y || material.base.reflectance != 0.0
                || mesh.attribute(Mesh::ATTRIBUTE_TANGENT).is_none()
            { return Err(format!("actual NIF material refused {normal}")); }
            let image = normal_image.unwrap();
            if image.texture_descriptor.format != TextureFormat::Rgba8Unorm || image.size() != UVec2::splat(8) {
                return Err(format!("actual normal-role image shape/format differs for {normal}"));
            }
            prepared.push((mesh, material));
        }
        Ok(prepared)
    })?;
    let mut image = Image::new_target_texture(WIDTH as u32, HEIGHT as u32, TextureFormat::Rgba8Unorm, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let target = world.resource_mut::<Assets<Image>>().add(image);
    let mut entities = Vec::with_capacity(5);
    for (i, (mesh, material)) in prepared.into_iter().enumerate() {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = world.resource_mut::<Assets<NifMaterial>>().add(material);
        entities.push(world.spawn((FieldOwned, Name::new(format!("BC5 authored panel {i}")), Mesh3d(mesh), MeshMaterial3d(material),
            Transform::from_xyz((i as f32 - 1.0) * 2.0, 0.0, 0.0), RenderLayers::layer(LAYER))).id());
    }
    entities.push(world.spawn((FieldOwned, Name::new("BC5 authored light"),
        DirectionalLight { illuminance: 2.0, shadow_maps_enabled: false, contact_shadows_enabled: false, ..default() },
        Transform::from_translation(Vec3::new(0.25, 0.35, 1.0)).looking_at(Vec3::ZERO, Vec3::Y), RenderLayers::layer(LAYER))).id());
    entities.push(world.spawn((FieldOwned, FieldCamera, Name::new("BC5 authored camera"), Camera3d::default(),
        Camera { order: -100, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(target.clone().into()),
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: 6.0, height: 2.0 }, near: 0.1, far: 10.0, ..OrthographicProjection::default_3d() }),
        Transform::from_xyz(0.0, 0.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
        Exposure { ev100: 0.0 }, AmbientLight { brightness: 0.0, ..default() },
        Tonemapping::None, DebandDither::Disabled, Msaa::Off, RenderLayers::layer(LAYER))).id());
    Ok(Scene { entities, target })
}

fn received(
    event: On<ReadbackComplete>, mut field: ResMut<Field>, frames: Res<RenderFrames>,
    layers: Query<(&RenderLayers, Option<&TransmissionAnchor>), Without<FieldOwned>>, mut commands: Commands,
) {
    if field.done || field.pending != Some(event.entity) { return; }
    field.pending = None;
    commands.entity(event.entity).despawn();
    let ordinal = field.completed + 1;
    let frame = frames.0.load(Ordering::Acquire);
    // The last main-world update may precede a new layer user. Validate again
    // at the actual completion before either PASS marker, even for valid pixels.
    let result = if layers.iter().any(|(layers, anchor)| foreign_layer(layers, anchor)) {
        Err("foreign recipient in isolated render layer at completion".into())
    } else { compare(&event.data) };
    let saved = field.output.as_ref().ok_or_else(|| "missing output directory".to_string())
        .and_then(|out| save_readback(out, ordinal, frame, &event.data, &result).map_err(|e| format!("save readback: {e}")));
    if let Err(reason) = saved.and(result.map(|_| ())) {
        error!("bc5 field: FAIL readback {ordinal}: {reason}");
        field.done = true;
        return;
    }
    field.completed = ordinal;
    field.after_frame = frame;
    if ordinal == 1 {
        info!("bc5 field: first rendered readback PASS");
    } else {
        info!("bc5 field: second rendered readback PASS");
        info!("bc5 field: rendered comparison PASS");
        field.done = true;
    }
}

#[derive(Debug)]
struct Report {
    max_delta: [u8; 4],
    flat_mean_delta: [f64; 4],
    luminance: [[f64; 4]; 3],
}

fn compare(bytes: &[u8]) -> Result<Report, String> {
    if bytes.len() != WIDTH * HEIGHT * 4 { return Err(format!("wrong byte count {}", bytes.len())); }
    if bytes.as_chunks::<4>().0.iter().any(|p| p[3] != 255) { return Err("nonopaque target alpha".into()); }
    let mut report = Report { max_delta: [0; 4], flat_mean_delta: [0.0; 4], luminance: [[0.0; 4]; 3] };
    for (q, &(left, top)) in STARTS.iter().enumerate() {
        for y in top..top + ROI {
            for x in left..left + ROI {
                let offset = (y * WIDTH + x) * 4;
                for channel in 0..3 {
                    let a = bytes[offset + channel];
                    let b = bytes[offset + 128 * 4 + channel];
                    let flat = bytes[offset + 256 * 4 + channel];
                    report.max_delta[q] = report.max_delta[q].max(a.abs_diff(b));
                    report.flat_mean_delta[q] += f64::from(b.abs_diff(flat));
                }
                for (panel, row) in report.luminance.iter_mut().enumerate() {
                    let i = offset + panel * 128 * 4;
                    row[q] += 0.2126 * f64::from(bytes[i]) + 0.7152 * f64::from(bytes[i + 1]) + 0.0722 * f64::from(bytes[i + 2]);
                }
            }
        }
        report.flat_mean_delta[q] /= (ROI * ROI * 3) as f64;
        for row in &mut report.luminance { row[q] /= (ROI * ROI) as f64; }
    }
    if report.max_delta.iter().any(|&d| d > 1)
        || report.flat_mean_delta.iter().filter(|&&d| d >= 8.0).count() < 2
        || report.luminance.iter().flatten().any(|&l| l <= 8.0 || l >= 247.0)
    { return Err(format!("fixed comparator: {report:?}")); }
    Ok(report)
}

fn create_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)
}

// Retain previous captures, including the old single-run layout. create_dir is
// the reservation: even simultaneous runs cannot select the same evidence child.
fn new_output(home: &Path, stamp: u128, process: u32) -> std::io::Result<PathBuf> {
    let root = home.join("bc5-field-output");
    match fs::create_dir(&root) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let meta = fs::symlink_metadata(&root)?;
            if !meta.is_dir() || meta.file_type().is_symlink() {
                return Err(std::io::Error::other("output parent is not an ordinary directory"));
            }
        }
        Err(e) => return Err(e),
    }
    for attempt in 0..64 {
        let path = root.join(format!("run-{stamp}-{process}-{attempt}"));
        match fs::create_dir(&path) {
            Ok(()) => return Ok(path),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => return Err(e),
        }
    }
    Err(std::io::Error::other("output reservation collision limit"))
}

fn save_readback(out: &Path, ordinal: u8, frame: u64, bytes: &[u8], report: &Result<Report, String>) -> std::io::Result<()> {
    create_file(&out.join(format!("readback-{ordinal}.rgba")), bytes)?;
    create_file(&out.join(format!("readback-{ordinal}.txt")), format!("submitted render frames at receipt: {frame}\n{report:#?}\n").as_bytes())?;
    if bytes.len() == WIDTH * HEIGHT * 4 {
        let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for pixel in bytes.as_chunks::<4>().0 { ppm.extend_from_slice(&pixel[..3]); }
        create_file(&out.join(format!("readback-{ordinal}.ppm")), &ppm)?;
    }
    Ok(())
}

// Authoring a documented DDS header is separate from the production parser and
// decoder. Each reference pixel is one of the fixed independent tuples above.
fn header(signed: bool) -> Vec<u8> {
    let mut h = vec![0; 128];
    h[..4].copy_from_slice(b"DDS ");
    for (at, n) in [(4, 124u32), (8, 0x1007 | if signed { 0x80000 } else { 8 }), (12, 8), (16, 8), (20, if signed { 64 } else { 32 }), (28, 1), (76, 32), (80, if signed { 4 } else { 0x41 }), (108, 0x1000)] {
        h[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    if signed { h[84..88].copy_from_slice(b"BC5S"); }
    else {
        for (at, n) in [(88, 32u32), (92, 0xff), (96, 0xff00), (100, 0xff0000), (104, 0xff000000)] {
            h[at..at + 4].copy_from_slice(&n.to_le_bytes());
        }
    }
    h
}

fn fixture() -> [(&'static str, Vec<u8>); 4] {
    let mut signed = header(true);
    for (x, y) in ENDPOINTS {
        for v in [x, y] { signed.extend_from_slice(&[v as u8, 0, 0, 0, 0, 0, 0, 0]); }
    }
    let mut reference = header(false);
    let mut flat = header(false);
    let mut white = header(false);
    for y in 0..8 {
        for x in 0..8 {
            reference.extend_from_slice(&REFERENCE[(y / 4) * 2 + x / 4]);
            flat.extend_from_slice(&[128, 128, 255, 255]);
            white.extend_from_slice(&[255; 4]);
        }
    }
    [("signed.dds", signed), ("reference.dds", reference), ("flat.dds", flat), ("white.dds", white)]
}

fn write_fixture(out: &Path) -> std::io::Result<()> {
    fs::create_dir(out.join("textures"))?;
    for (name, bytes) in fixture() { create_file(&out.join("textures").join(name), &bytes)?; }
    create_file(&out.join("mesh.txt"), b"TriangleList; positions=(-1,-1,0),(1,-1,0),(1,1,0),(-1,1,0); normals=(0,0,1); UV=(0,1),(1,1),(1,0),(0,0); indices=0,1,2,0,2,3; tangents=actual shape_material\n")
}

#[cfg(test)]
mod tests;
