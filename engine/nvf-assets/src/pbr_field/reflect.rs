//! The reflection field (#2981 reflections; docs/spec-notes/pbr-materials.md
//! "Reflections on authored materials"), default off, behind
//! `NVF_PLAY_TEST_PBR_REFLECT_FIELD=1`, on the PBR field's pattern (pbr_field.rs): every file
//! generated into a fresh folder under `NVFOREVER_HOME`, read through a real VFS, the panels'
//! materials from the real `shape_material`, GPU readbacks judged.
//!
//! Thirteen panels (2x2 quads) in three rows, drawn by two cameras of the same pose,
//! exposure (Bevy's default, the production one), MSAA (4x) and world-view ambient light (a
//! display value of 0.6 with the negative alpha that marks a world view,
//! nvforever `lights::display_ambient`; brightness `light_unit()`), no directional light:
//! - camera A carries a [`ReflectionCube`] filled with an asymmetric fixed-colour cube, a
//!   distinct linear colour per world face ([`FACES`], +Z and -Z included), each level mixed
//!   towards the faces' mean by level / 5 (the producer's blur, [`cube_colour`]);
//! - camera B has none.
//!
//! Row one: an authored metal (ORM blue 255, roughness 51/255 = 0.2) tilted so that its
//! reflection vector (orthographic view, V = +Z) points to +X, -X, +Y, -Y and +Z. Row two,
//! all facing the camera (reflection +Z, the face's centre at every level): the metal at
//! roughness 128/255 and 230/255, an authored dielectric (roughness 0.5, reflectance 0.5), a
//! legacy control (no set) and a zero-reflectance control (an authored set whose scalar file
//! sets reflectance 0). Rows one and two have a white diffuse, so the metal gain changes
//! nothing on them (the brightest channel is 1 already). Row three, the metal base colour
//! (docs/spec-notes/pbr-materials.md "Metal base colour"), head-on metals at roughness 0.5:
//! `lift`, a dark bronze diffuse ([`BRONZE`]) with the default metal gain (12, limited to
//! 1 / 0.40 by the bronze's red, so lifted to (1, 0.75, 0.5)); `lift1`, the same with
//! `metal_gain = 1`; `albedo`, a white diffuse whose set's `_albedo` is the bronze (gain
//! default 1 with an `_albedo`).
//!
//! "The switch" below is the run's reflections state (`reflection_cube::enabled`): the
//! settings' Reflections on authored materials row with Authored materials, which the
//! scenarios pin with `--graphics-only` (on: `authored-materials`, where the row stays on; off:
//! `no-authored-reflections`). The field forces nothing else: its cameras are its own, never
//! the world camera, so the world camera's cube (nvforever reflection_sky.rs, made only where
//! the game's mount has authored sets) cannot overwrite camera A's fixed cube.
//!
//! With the switch on, camera A: each tilted metal's colour is
//! nearest the face its reflection points at (pins the faces' orientation and Bevy's z flip);
//! the head-on metals' chromaticity lies nearer the cube's (neutral) mean as roughness rises
//! (pins the level rule); the smooth head-on metal is clearly tinted; the legacy and
//! zero-reflectance controls equal camera B's; the dielectric does not darken; the lifted
//! bronze is clearly brighter than `lift1` with the same hue, and `albedo` equals `lift1`.
//! Camera B (no cube): every white panel draws the same white under the ambient light, the
//! metals included (metallic held at 0 where the view has no cube), and the three bronze
//! panels the same bronze (no lift without a cube; the `_albedo` drawn in every view). With the switch off, the cube stays on
//! camera A, so the run also shows that the reflection gate keeps it from every shader that
//! did not opt in: camera A equals camera B panel for panel. The off run's camera B is
//! today's picture; that the on run's camera B equals it is compared across the two runs'
//! logs (the readback lines), not inside one run.
//!
//! Thresholds [engineering policy]: set from reasoning about the expected values (module
//! constants), with wide margins, not from readbacks; the main session confirms them against
//! the first real ones. Nothing here is a fact about the original game.

use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::CompressedImageFormatSupport;
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::camera::ExtractedCamera;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::gpu_readback::{ReadbackComplete, ReadbackOnce};
use bevy::render::render_resource::{TextureFormat, TextureUsages};
use bevy::render::{Render, RenderApp, RenderSystems};

use super::{create_file, mesh, new_output_in, owned_data_of, rgba};
use crate::nif_material::TransmissionAnchor;
use crate::nif_spawn::{Props, shape_material};
use crate::reflection_cube::{light_unit, ReflectionCube, SPECULAR_LEVELS};
use crate::{GameData, NifMaterial, TextureCache};

const LAYER: usize = 28;
const CELL: usize = 128;
const COLS: usize = 5;
const ROWS: usize = 3;
const WIDTH: usize = CELL * COLS;
const HEIGHT: usize = CELL * ROWS;
/// The judged region of each cell: its middle half (a tilted panel spans about 90 of the
/// cell's 128 pixels across its tilt).
const ROI: std::ops::Range<usize> = 32..96;

/// The world faces of the fixed cube and their linear colours: each a different pattern of
/// high and low channels, complementary across each axis.
pub(super) const FACES: [(&str, Vec3, Vec3); 6] = [
    ("+X", Vec3::X, Vec3::new(0.6, 0.1, 0.1)),
    ("-X", Vec3::NEG_X, Vec3::new(0.1, 0.6, 0.6)),
    ("+Y", Vec3::Y, Vec3::new(0.1, 0.6, 0.1)),
    ("-Y", Vec3::NEG_Y, Vec3::new(0.6, 0.1, 0.6)),
    ("+Z", Vec3::Z, Vec3::new(0.1, 0.1, 0.6)),
    ("-Z", Vec3::NEG_Z, Vec3::new(0.6, 0.6, 0.1)),
];

/// The faces' mean (neutral).
pub(super) const MEAN: Vec3 = Vec3::splat(0.35);

/// The ambient light's display value (camera brightness `light_unit()`, alpha -1).
const AMBIENT: f32 = 0.6;

/// A panel: its name, the diffuse texture's stem, the world direction its reflection
/// vector points to (V = +Z), and its cell (column, row).
pub(super) struct Panel {
    pub name: &'static str,
    pub stem: &'static str,
    pub reflect: Vec3,
    pub cell: (usize, usize),
}

pub(super) const PANELS: [Panel; 13] = [
    Panel { name: "metal20+X", stem: "metal20", reflect: Vec3::X, cell: (0, 0) },
    Panel { name: "metal20-X", stem: "metal20", reflect: Vec3::NEG_X, cell: (1, 0) },
    Panel { name: "metal20+Y", stem: "metal20", reflect: Vec3::Y, cell: (2, 0) },
    Panel { name: "metal20-Y", stem: "metal20", reflect: Vec3::NEG_Y, cell: (3, 0) },
    Panel { name: "metal20+Z", stem: "metal20", reflect: Vec3::Z, cell: (4, 0) },
    Panel { name: "metal50", stem: "metal50", reflect: Vec3::Z, cell: (0, 1) },
    Panel { name: "metal90", stem: "metal90", reflect: Vec3::Z, cell: (1, 1) },
    Panel { name: "dielectric", stem: "dielectric", reflect: Vec3::Z, cell: (2, 1) },
    Panel { name: "legacy", stem: "legacy", reflect: Vec3::Z, cell: (3, 1) },
    Panel { name: "zero", stem: "zero", reflect: Vec3::Z, cell: (4, 1) },
    Panel { name: "lift", stem: "lift", reflect: Vec3::Z, cell: (0, 2) },
    Panel { name: "lift1", stem: "lift1", reflect: Vec3::Z, cell: (1, 2) },
    Panel { name: "albedo", stem: "painted", reflect: Vec3::Z, cell: (2, 2) },
];
/// The tilted metals (orientation), by index.
const ORIENTED: std::ops::Range<usize> = 0..5;
/// The head-on metals at roughness 0.2, 0.5 and 0.9.
const ROUGHNESS: [usize; 3] = [4, 5, 6];
const DIELECTRIC: usize = 7;
const LEGACY: usize = 8;
const ZERO: usize = 9;
/// The bronze metal at its default gain, at gain 1, and the white diffuse with a bronze
/// `_albedo`.
const LIFT: usize = 10;
const LIFT1: usize = 11;
const PAINTED: usize = 12;
/// The panels with a white diffuse (rows one and two).
const WHITE_PANELS: std::ops::Range<usize> = 0..10;
const BRONZE_PANELS: [usize; 3] = [LIFT, LIFT1, PAINTED];

/// The ORMs (RGBA bytes: occlusion, roughness, metallic) of the authored sets.
const ORMS: [(&str, [u8; 4]); 8] = [
    ("metal20", [255, 51, 255, 255]),
    ("metal50", [255, 128, 255, 255]),
    ("metal90", [255, 230, 255, 255]),
    ("dielectric", [255, 128, 0, 255]),
    ("zero", [255, 128, 0, 255]),
    ("lift", [255, 128, 255, 255]),
    ("lift1", [255, 128, 255, 255]),
    ("painted", [255, 128, 255, 255]),
];
const WHITE: [u8; 4] = [255, 255, 255, 255];
/// A dark bronze as sRGB bytes, about linear (0.40, 0.30, 0.20): the `lift` panels' diffuse
/// and the `albedo` panel's `_albedo`.
const BRONZE: [u8; 4] = [170, 149, 124, 255];
const DIFFUSE_SIDE: u32 = 8;
const ORM_SIDE: u32 = 16;

/// The comparator [engineering policy, from reasoning; main session to confirm against the
/// first real readbacks]. Expected, by Bevy's environment term (F_ab polynomial, F0 = 1 for
/// the white metal, intensity x exposure = 1) and the cube above: the head-on metals'
/// chromaticity distances from the mean about 0.39, 0.22 and 0.04 (steps about 0.17 and
/// 0.18, against [`MIP_STEP`]); each tilted metal's colour has cosine about 0.99 with its
/// face and at most about 0.84 with any other (margin about 0.15, against [`FACE_MARGIN`]);
/// the smooth head-on metal about (34, 34, 125) in bytes (blue less red about 90, against
/// [`MIN_TINT`]); the dielectric about 2 bytes brighter on camera A (against
/// [`DIELECTRIC_GAIN`]); every camera-B panel and every control the same to rounding (against
/// [`SAME`] and [`SAME_B`]). Row three, with F0 the bronze (0.40, 0.30, 0.20) at roughness 0.5
/// (F_ab about 0.72, the +Z face at level 2.5, (0.225, 0.225, 0.475)): `lift1` about (17, 13,
/// 18) in bytes (luminance about 14, a little more with Bevy's multiple-scattering term),
/// `lift` 2.49 times it, about (41, 31, 44), the same chromaticity (distance under 0.01;
/// against [`LIFT_RATIO`] and [`HUE_SAME`]); `albedo` equal to `lift1` (against [`SAME`]);
/// on camera B the three bronze panels the same (against [`SAME_B`]).
const FACE_MARGIN: f64 = 0.05;
const MIP_STEP: f64 = 0.05;
const MIN_TINT: f64 = 30.0;
const SAME: f64 = 1.5;
const SAME_B: f64 = 2.0;
const DIELECTRIC_GAIN: std::ops::RangeInclusive<f64> = -1.5..=15.0;
const LIFT_RATIO: f64 = 1.6;
const HUE_SAME: f64 = 0.04;
const LIT: std::ops::Range<f64> = 8.0..247.0;
const CLIP: u8 = 254;

/// The fixed cube's colour toward world direction `d` at `level`: the face `d` points at
/// most, mixed towards [`MEAN`] by level / (levels - 1), so level 0 holds the faces as they
/// are and the 1x1 level their mean.
pub(super) fn cube_colour(d: Vec3, level: u32) -> Vec3 {
    let face = FACES.iter().max_by(|a, b| d.dot(a.1).total_cmp(&d.dot(b.1))).map(|f| f.2).unwrap_or(MEAN);
    face.lerp(MEAN, level as f32 / (SPECULAR_LEVELS - 1) as f32)
}

#[derive(Component)]
struct ReflectOwned;

#[derive(Component, Clone, ExtractComponent)]
#[extract_app(RenderApp)]
struct ReflectCameraA;

#[derive(Resource, Clone, Default)]
struct ReflectFrames(Arc<AtomicU64>);

#[derive(Resource)]
struct ReflectField {
    started: Option<Instant>,
    output: Option<PathBuf>,
    scene: Option<Scene>,
    on: bool,
    attached_at: Option<u64>,
    pending: [Option<Entity>; 2],
    received: [Option<Vec<u8>>; 2],
    completed: u8,
    after_frame: u64,
    done: bool,
}

pub(super) struct Scene {
    pub entities: Vec<Entity>,
    pub camera_a: Entity,
    /// Camera A's target, then camera B's.
    pub targets: [Handle<Image>; 2],
}

pub(crate) fn install(app: &mut App) {
    let Some(value) = std::env::var_os("NVF_PLAY_TEST_PBR_REFLECT_FIELD") else { return };
    if value != "1" {
        error!("pbr reflect field: FAIL expected NVF_PLAY_TEST_PBR_REFLECT_FIELD=1");
        return;
    }
    let frames = ReflectFrames::default();
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        error!("pbr reflect field: FAIL missing normal RenderApp");
        return;
    };
    render.insert_resource(frames.clone()).add_systems(Render, count_rendered.in_set(RenderSystems::Cleanup));
    app.add_plugins(ExtractComponentPlugin::<ReflectCameraA>::default())
        .insert_resource(frames)
        .insert_resource(ReflectField { started: None, output: None, scene: None, on: false, attached_at: None, pending: [None, None], received: [None, None], completed: 0, after_frame: 0, done: false })
        .add_systems(Update, tick);
}

fn count_rendered(views: Query<&ExtractedCamera, With<ReflectCameraA>>, frames: Res<ReflectFrames>) {
    if let Ok(camera) = views.single() {
        if camera.physical_target_size == Some(UVec2::new(WIDTH as u32, HEIGHT as u32)) {
            frames.0.fetch_add(1, Ordering::AcqRel);
        }
    }
}

fn tick(world: &mut World) {
    let Some(mut field) = world.remove_resource::<ReflectField>() else { return };
    let started = *field.started.get_or_insert_with(Instant::now);
    if !field.done {
        if let Err(reason) = advance(world, &mut field, started.elapsed()) {
            error!("pbr reflect field: FAIL {reason}");
            field.done = true;
        }
    }
    if field.done {
        for entity in field.pending.iter_mut().filter_map(Option::take) {
            world.despawn(entity);
        }
        if let Some(scene) = field.scene.take() {
            for entity in scene.entities {
                world.despawn(entity);
            }
        }
    }
    world.insert_resource(field);
}

fn advance(world: &mut World, field: &mut ReflectField, elapsed: Duration) -> Result<(), String> {
    if elapsed >= Duration::from_secs(45) {
        return Err(if field.attached_at.is_none() { "timeout before camera A's cube was attached".into() } else { "timeout before two completed rendered comparisons".into() });
    }
    check_layer(world)?;
    if field.scene.is_none() {
        // The gate decides whether a cube can be attached at all; wait for lights.rs.
        if crate::reflection_cube::gate_state() == crate::reflection_cube::GateState::Pending {
            return Ok(());
        }
        let Some(data) = world.get_resource::<GameData>().cloned() else { return Ok(()) };
        let home = std::env::var_os("NVFOREVER_HOME").ok_or("missing explicit owned NVFOREVER_HOME")?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("output clock: {e}"))?.as_nanos();
        let output = new_output_in(Path::new(&home), "pbr-reflect-field-output", stamp, std::process::id()).map_err(|e| format!("fresh run output: {e}"))?;
        write_fixture(&output).map_err(|e| format!("generated fixture: {e}"))?;
        let local = owned_data_of(&output, data.schema.clone(), &fixture())?;
        field.on = crate::reflection_cube::enabled();
        info!("pbr reflect field: evidence directory {}", output.display());
        field.scene = Some(make_scene(world, &local, field.on)?);
        field.output = Some(output);
    }
    let frame = world.resource::<ReflectFrames>().0.load(Ordering::Acquire);
    let camera_a = field.scene.as_ref().ok_or("missing owned scene")?.camera_a;
    if field.attached_at.is_none() {
        if !world.get_entity(camera_a).is_ok_and(|e| e.contains::<EnvironmentMapLight>()) {
            return Ok(());
        }
        field.attached_at = Some(frame);
        info!("pbr reflect field: camera A's cube attached (switch {})", if field.on { "on" } else { "off" });
    }
    let attached = field.attached_at.unwrap_or(frame);
    let idle = field.pending.iter().all(Option::is_none) && field.received.iter().all(Option::is_none);
    if idle && elapsed >= Duration::from_secs(8) && frame >= attached + 20 && frame >= field.after_frame + 10 {
        let targets = field.scene.as_ref().ok_or("missing owned scene")?.targets.clone();
        for (slot, target) in targets.into_iter().enumerate() {
            let entity = world.spawn((ReflectOwned, ReadbackOnce::texture(target))).observe(received).id();
            field.pending[slot] = Some(entity);
        }
    }
    Ok(())
}

fn check_layer(world: &mut World) -> Result<(), String> {
    let mut q = world.query_filtered::<(&RenderLayers, Option<&TransmissionAnchor>), Without<ReflectOwned>>();
    if q.iter(world).any(|(layers, anchor)| foreign_layer(layers, anchor)) {
        Err("foreign recipient in isolated render layer".into())
    } else {
        Ok(())
    }
}

fn foreign_layer(layers: &RenderLayers, anchor: Option<&TransmissionAnchor>) -> bool {
    anchor.is_none() && layers.intersects(&RenderLayers::layer(LAYER))
}

/// The panel's normal: halfway between the view direction (+Z, orthographic) and its
/// reflection direction.
pub(super) fn panel_normal(reflect: Vec3) -> Vec3 {
    (Vec3::Z + reflect).normalize()
}

/// The panel's place in the world: its cell's centre, the camera looking down -Z.
fn panel_position(cell: (usize, usize)) -> Vec3 {
    let x = (cell.0 as f32 - (COLS as f32 - 1.0) / 2.0) * 2.0;
    let y = ((ROWS as f32 - 1.0) / 2.0 - cell.1 as f32) * 2.0;
    Vec3::new(x, y, 0.0)
}

pub(super) fn make_scene(world: &mut World, data: &GameData, on: bool) -> Result<Scene, String> {
    let formats = world.get_resource::<CompressedImageFormatSupport>().map(|s| s.0).ok_or("missing the device's compressed format support")?;
    let support = CompressedImageFormatSupport(formats);
    let mut cache = TextureCache::default();
    let prepared = world.resource_scope(|_, mut images: Mut<Assets<Image>>| -> Result<Vec<(Mesh, NifMaterial)>, String> {
        let mut prepared = Vec::with_capacity(PANELS.len());
        for panel in &PANELS {
            let mut mesh = mesh();
            let mut props = Props::default();
            props.textures = vec![format!("textures\\reflect\\{}.dds", panel.stem)];
            let (material, missing) = shape_material(&props, &mut mesh, &mut cache, data, &mut images, Some(&support));
            if missing.is_some() || material.base.base_color_texture.is_none() || material.base.unlit {
                return Err(format!("actual NIF material refused {}", panel.name));
            }
            let authored = crate::authored::is_authored(&material);
            if authored == (panel.stem == "legacy") {
                return Err(format!("{} panel: authored {authored}, expected {}", panel.name, panel.stem != "legacy"));
            }
            if authored && material.base.metallic != crate::authored::authored_metallic() {
                return Err(format!("{} panel: metallic factor {}, expected {}", panel.name, material.base.metallic, crate::authored::authored_metallic()));
            }
            if panel.stem == "zero" && material.base.reflectance != 0.0 {
                return Err(format!("zero panel: reflectance {}, expected 0 from its scalar file", material.base.reflectance));
            }
            // Row three: each set's metal gain (the run's override replaces them all), and the
            // `_albedo` drawn in place of the white diffuse.
            let gain = match panel.stem {
                "lift" => Some(fnv_pbr::DEFAULT_METAL_GAIN),
                "lift1" | "painted" => Some(1.0),
                _ => None,
            };
            if let Some(want) = gain.map(|g| crate::authored::metal_gain_override().unwrap_or(g)) {
                if material.extension.metal_gain != want {
                    return Err(format!("{} panel: metal gain {}, expected {want}", panel.name, material.extension.metal_gain));
                }
            }
            if panel.stem == "painted" && (material.extension.authored_albedo.is_none() || material.base.base_color_texture != material.extension.authored_albedo) {
                return Err("albedo panel: its set's _albedo is not the base colour texture".into());
            }
            prepared.push((mesh, material));
        }
        Ok(prepared)
    })?;
    let names: Vec<&str> = PANELS.iter().map(|p| p.name).collect();
    info!("pbr reflect field: actual NIF material source, {} panels ({}), switch {}, gate {:?}, cube on camera A", PANELS.len(), names.join(", "), if on { "on" } else { "off" }, crate::reflection_cube::gate_state());
    let target = |world: &mut World| {
        let mut image = Image::new_target_texture(WIDTH as u32, HEIGHT as u32, TextureFormat::Rgba8Unorm, None);
        image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
        world.resource_mut::<Assets<Image>>().add(image)
    };
    let targets = [target(world), target(world)];
    let mut entities = Vec::with_capacity(PANELS.len() + 2);
    for (panel, (mesh, material)) in PANELS.iter().zip(prepared) {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = world.resource_mut::<Assets<NifMaterial>>().add(material);
        let transform = Transform::from_translation(panel_position(panel.cell)).with_rotation(Quat::from_rotation_arc(Vec3::Z, panel_normal(panel.reflect)));
        entities.push(world.spawn((ReflectOwned, Name::new(format!("PBR reflect field panel {}", panel.name)), Mesh3d(mesh), MeshMaterial3d(material), transform, RenderLayers::layer(LAYER))).id());
    }
    let mut cube = ReflectionCube::new(&mut world.resource_mut::<Assets<Image>>());
    cube.fill(cube_colour);
    let camera = |order: isize, target: Handle<Image>, name: &'static str| {
        (
            ReflectOwned,
            Name::new(name),
            Camera3d::default(),
            Camera { order, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
            RenderTarget::Image(target.into()),
            Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: (2 * COLS) as f32, height: (2 * ROWS) as f32 }, near: 0.1, far: 10.0, ..OrthographicProjection::default_3d() }),
            Transform::from_xyz(0.0, 0.0, 4.0).looking_at(Vec3::ZERO, Vec3::Y),
            (Exposure::default(), AmbientLight { color: Color::LinearRgba(LinearRgba::new(AMBIENT, AMBIENT, AMBIENT, -1.0)), brightness: light_unit(), ..default() }),
            (Tonemapping::None, DebandDither::Disabled, Msaa::Sample4, RenderLayers::layer(LAYER)),
        )
    };
    let camera_a = world.spawn((camera(-103, targets[0].clone(), "PBR reflect field camera A (cube)"), ReflectCameraA, cube)).id();
    entities.push(camera_a);
    entities.push(world.spawn(camera(-102, targets[1].clone(), "PBR reflect field camera B (no cube)")).id());
    Ok(Scene { entities, camera_a, targets })
}

fn received(event: On<ReadbackComplete>, mut field: ResMut<ReflectField>, frames: Res<ReflectFrames>, layers: Query<(&RenderLayers, Option<&TransmissionAnchor>), Without<ReflectOwned>>, mut commands: Commands) {
    if field.done {
        return;
    }
    let Some(slot) = field.pending.iter().position(|p| *p == Some(event.entity)) else { return };
    field.pending[slot] = None;
    field.received[slot] = Some(event.data.clone());
    commands.entity(event.entity).despawn();
    if field.received.iter().any(Option::is_none) {
        return;
    }
    let [Some(a), Some(b)] = std::mem::take(&mut field.received) else { return };
    let ordinal = field.completed + 1;
    let frame = frames.0.load(Ordering::Acquire);
    let measured = measure(&a).and_then(|ra| measure(&b).map(|rb| (ra, rb)));
    if let Ok((ra, rb)) = &measured {
        info!("pbr reflect field: readback {ordinal} {}", describe(ra, rb));
    }
    let result = if layers.iter().any(|(layers, anchor)| foreign_layer(layers, anchor)) {
        Err("foreign recipient in isolated render layer at completion".into())
    } else {
        measured.and_then(|(ra, rb)| judge(&ra, &rb, field.on).map(|_| (ra, rb)))
    };
    let saved = field.output.as_ref().ok_or_else(|| "missing output directory".to_string()).and_then(|out| save_readback(out, ordinal, frame, &a, &b, &result).map_err(|e| format!("save readback: {e}")));
    if let Err(reason) = saved.and(result.map(|_| ())) {
        error!("pbr reflect field: FAIL readback {ordinal}: {reason}");
        field.done = true;
        return;
    }
    field.completed = ordinal;
    field.after_frame = frame;
    if ordinal == 1 {
        info!("pbr reflect field: first rendered readback PASS");
    } else {
        info!("pbr reflect field: second rendered readback PASS");
        if field.on {
            info!("pbr reflect field: metals show the faces their reflections point at, rougher nearer the cube's mean, controls and the cube-less camera unchanged PASS");
        } else {
            info!("pbr reflect field: switch off, the cube on camera A changes no panel PASS");
        }
        field.done = true;
    }
}

/// One camera's readback: each panel's mean RGB over its judged region, and its brightest
/// channel value there.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Report {
    pub mean: [[f64; 3]; PANELS.len()],
    pub brightest: [u8; PANELS.len()],
}

fn luminance(c: [f64; 3]) -> f64 {
    0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]
}

/// The distance of `c`'s chromaticity (each channel over the sum) from the neutral one.
pub(super) fn chroma_distance(c: [f64; 3]) -> f64 {
    let sum = (c[0] + c[1] + c[2]).max(1e-9);
    c.iter().map(|v| (v / sum - 1.0 / 3.0).powi(2)).sum::<f64>().sqrt()
}

fn cosine(c: [f64; 3], f: Vec3) -> f64 {
    let f = [f64::from(f.x), f64::from(f.y), f64::from(f.z)];
    let dot: f64 = c.iter().zip(f).map(|(a, b)| a * b).sum();
    let len = |v: [f64; 3]| v.iter().map(|x| x * x).sum::<f64>().sqrt();
    dot / (len(c) * len(f)).max(1e-9)
}

/// The face whose colour `c` is nearest in direction (cosine), and the margin over the next.
pub(super) fn nearest_face(c: [f64; 3]) -> (&'static str, f64) {
    let mut scored: Vec<(&str, f64)> = FACES.iter().map(|(n, _, f)| (*n, cosine(c, *f))).collect();
    scored.sort_by(|a, b| b.1.total_cmp(&a.1));
    (scored[0].0, scored[0].1 - scored[1].1)
}

/// The expected face name of a reflection direction.
fn face_name(d: Vec3) -> &'static str {
    FACES.iter().find(|f| f.1 == d).map(|f| f.0).unwrap_or("?")
}

fn max_channel_diff(a: [f64; 3], b: [f64; 3]) -> f64 {
    (0..3).map(|i| (a[i] - b[i]).abs()).fold(0.0, f64::max)
}

pub(super) fn describe(a: &Report, b: &Report) -> String {
    let means = |r: &Report| PANELS.iter().zip(&r.mean).map(|(p, m)| format!("{}=({:.1},{:.1},{:.1})", p.name, m[0], m[1], m[2])).collect::<Vec<_>>().join(" ");
    let faces = ORIENTED.map(|i| {
        let (face, margin) = nearest_face(a.mean[i]);
        format!("{}->{face}({margin:.3})", PANELS[i].name)
    });
    let chroma = ROUGHNESS.map(|i| format!("{}={:.3}", PANELS[i].name, chroma_distance(a.mean[i])));
    format!(
        "A [{}] B [{}] faces A [{}] chroma A [{}] |A-B| legacy={:.2} zero={:.2} dielectric gain={:.2} B spread={:.2} lift A ratio={:.2} hue={:.3} |albedo-lift1| A={:.2} B bronze spread={:.2}",
        means(a),
        means(b),
        faces.collect::<Vec<_>>().join(" "),
        chroma.join(" "),
        max_channel_diff(a.mean[LEGACY], b.mean[LEGACY]),
        max_channel_diff(a.mean[ZERO], b.mean[ZERO]),
        luminance(a.mean[DIELECTRIC]) - luminance(b.mean[DIELECTRIC]),
        b_spread(b),
        lift_ratio(a),
        chroma_between(a.mean[LIFT], a.mean[LIFT1]),
        max_channel_diff(a.mean[PAINTED], a.mean[LIFT1]),
        bronze_spread(b),
    )
}

/// The largest channel difference of any white camera-B panel from B's dielectric.
fn b_spread(b: &Report) -> f64 {
    b.mean[WHITE_PANELS].iter().map(|m| max_channel_diff(*m, b.mean[DIELECTRIC])).fold(0.0, f64::max)
}

/// The largest channel difference of a bronze panel from `lift1` on one camera.
fn bronze_spread(r: &Report) -> f64 {
    BRONZE_PANELS.iter().map(|&i| max_channel_diff(r.mean[i], r.mean[LIFT1])).fold(0.0, f64::max)
}

/// The lifted bronze's luminance over the unlifted one's.
fn lift_ratio(a: &Report) -> f64 {
    luminance(a.mean[LIFT]) / luminance(a.mean[LIFT1]).max(1e-9)
}

/// The distance between two colours' chromaticities (each channel over the sum): 0 for the
/// same hue and saturation at any brightness.
pub(super) fn chroma_between(c: [f64; 3], d: [f64; 3]) -> f64 {
    let norm = |c: [f64; 3]| {
        let sum = (c[0] + c[1] + c[2]).max(1e-9);
        c.map(|v| v / sum)
    };
    let (c, d) = (norm(c), norm(d));
    (0..3).map(|i| (c[i] - d[i]).powi(2)).sum::<f64>().sqrt()
}

/// The panels' means, or why the frame cannot be judged (wrong size, not opaque, a blank or
/// clipped panel).
pub(super) fn measure(bytes: &[u8]) -> Result<Report, String> {
    if bytes.len() != WIDTH * HEIGHT * 4 {
        return Err(format!("wrong byte count {}", bytes.len()));
    }
    if bytes.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err("nonopaque target alpha".into());
    }
    let mut report = Report { mean: [[0.0; 3]; PANELS.len()], brightest: [0; PANELS.len()] };
    for (i, panel) in PANELS.iter().enumerate() {
        let (col, row) = panel.cell;
        let mut sum = [0.0; 3];
        for y in ROI {
            for x in ROI {
                let at = ((row * CELL + y) * WIDTH + col * CELL + x) * 4;
                let pixel = &bytes[at..at + 3];
                for (s, &v) in sum.iter_mut().zip(pixel) {
                    *s += f64::from(v);
                }
                report.brightest[i] = report.brightest[i].max(pixel[0]).max(pixel[1]).max(pixel[2]);
            }
        }
        report.mean[i] = sum.map(|s| s / (ROI.len() * ROI.len()) as f64);
    }
    if report.mean.iter().any(|m| !LIT.contains(&luminance(*m))) || report.brightest.iter().any(|&b| b >= CLIP) {
        return Err(format!("blank or clipped panel: {report:?}"));
    }
    Ok(report)
}

/// The comparator (module notes) on camera A's and camera B's frames.
pub(super) fn judge(a: &Report, b: &Report, on: bool) -> Result<(), String> {
    let mut failed = Vec::new();
    // Camera B, both runs: every panel the same white under the ambient light.
    if b_spread(b) > SAME_B {
        failed.push(format!("camera B's panels differ by {:.2}", b_spread(b)));
    }
    // Camera B, both runs: the bronze panels the same (no lift without a cube, the `_albedo`
    // drawn in every view).
    if bronze_spread(b) > SAME_B {
        failed.push(format!("camera B's bronze panels differ by {:.2}", bronze_spread(b)));
    }
    if on {
        for i in ORIENTED {
            let (face, margin) = nearest_face(a.mean[i]);
            let want = face_name(PANELS[i].reflect);
            if face != want || margin < FACE_MARGIN {
                failed.push(format!("{} shows {face} (margin {margin:.3}), expected {want}", PANELS[i].name));
            }
        }
        let d = ROUGHNESS.map(|i| chroma_distance(a.mean[i]));
        if d[0] - d[1] < MIP_STEP || d[1] - d[2] < MIP_STEP {
            failed.push(format!("chromaticity distances {:.3} {:.3} {:.3} do not fall by {MIP_STEP} per roughness step", d[0], d[1], d[2]));
        }
        let smooth = a.mean[ROUGHNESS[0]];
        if smooth[2] - smooth[0] < MIN_TINT {
            failed.push(format!("the smooth metal's blue less red {:.1} under {MIN_TINT}", smooth[2] - smooth[0]));
        }
        for i in [LEGACY, ZERO] {
            if max_channel_diff(a.mean[i], b.mean[i]) > SAME {
                failed.push(format!("{} differs between the cameras by {:.2}", PANELS[i].name, max_channel_diff(a.mean[i], b.mean[i])));
            }
        }
        let gain = luminance(a.mean[DIELECTRIC]) - luminance(b.mean[DIELECTRIC]);
        if !DIELECTRIC_GAIN.contains(&gain) {
            failed.push(format!("the dielectric's gain {gain:.2} outside {DIELECTRIC_GAIN:?}"));
        }
        // Metal base colour: the lift brightens the bronze and keeps its hue; the `_albedo`
        // draws as the same bronze at gain 1.
        if lift_ratio(a) < LIFT_RATIO {
            failed.push(format!("the lifted metal is {:.2} times as bright as at gain 1, under {LIFT_RATIO}", lift_ratio(a)));
        }
        let hue = chroma_between(a.mean[LIFT], a.mean[LIFT1]);
        if hue > HUE_SAME {
            failed.push(format!("the lift moved the hue by {hue:.3}, over {HUE_SAME}"));
        }
        if max_channel_diff(a.mean[PAINTED], a.mean[LIFT1]) > SAME {
            failed.push(format!("albedo differs from lift1 by {:.2}", max_channel_diff(a.mean[PAINTED], a.mean[LIFT1])));
        }
    } else {
        for (i, panel) in PANELS.iter().enumerate() {
            if max_channel_diff(a.mean[i], b.mean[i]) > SAME {
                failed.push(format!("{} differs between the cameras by {:.2}", panel.name, max_channel_diff(a.mean[i], b.mean[i])));
            }
        }
    }
    if failed.is_empty() { Ok(()) } else { Err(format!("comparator ({}): {}", if on { "on" } else { "off" }, failed.join("; "))) }
}

fn save_readback(out: &Path, ordinal: u8, frame: u64, a: &[u8], b: &[u8], result: &Result<(Report, Report), String>) -> std::io::Result<()> {
    for (camera, bytes) in [("a", a), ("b", b)] {
        create_file(&out.join(format!("readback-{ordinal}-{camera}.rgba")), bytes)?;
        if bytes.len() == WIDTH * HEIGHT * 4 {
            let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
            for pixel in bytes.as_chunks::<4>().0 {
                ppm.extend_from_slice(&pixel[..3]);
            }
            create_file(&out.join(format!("readback-{ordinal}-{camera}.ppm")), &ppm)?;
        }
    }
    let described = match result {
        Ok((ra, rb)) => describe(ra, rb),
        Err(e) => e.clone(),
    };
    let names: Vec<&str> = PANELS.iter().map(|p| p.name).collect();
    create_file(&out.join(format!("readback-{ordinal}.txt")), format!("submitted render frames at receipt: {frame}\npanels: {names:?}\n{described}\n").as_bytes())
}

/// Every generated file, by its path under the output folder.
pub(super) fn fixture() -> Vec<(&'static str, Vec<u8>)> {
    let white = rgba(DIFFUSE_SIDE, WHITE);
    let bronze = rgba(DIFFUSE_SIDE, BRONZE);
    let orm = |stem: &str| ORMS.iter().find(|(s, _)| *s == stem).map(|(_, t)| rgba(ORM_SIDE, *t)).expect("an ORM per authored stem");
    vec![
        ("textures/reflect/metal20.dds", white.clone()),
        ("textures/reflect/metal50.dds", white.clone()),
        ("textures/reflect/metal90.dds", white.clone()),
        ("textures/reflect/dielectric.dds", white.clone()),
        ("textures/reflect/legacy.dds", white.clone()),
        ("textures/reflect/zero.dds", white.clone()),
        ("textures/reflect/lift.dds", bronze.clone()),
        ("textures/reflect/lift1.dds", bronze.clone()),
        ("textures/reflect/painted.dds", white),
        ("textures/pbr/reflect/metal20_orm.dds", orm("metal20")),
        ("textures/pbr/reflect/metal50_orm.dds", orm("metal50")),
        ("textures/pbr/reflect/metal90_orm.dds", orm("metal90")),
        ("textures/pbr/reflect/dielectric_orm.dds", orm("dielectric")),
        ("textures/pbr/reflect/zero_orm.dds", orm("zero")),
        ("textures/pbr/reflect/zero.toml", b"# the zero-reflectance control\nreflectance = 0.0\n".to_vec()),
        // Row three: `lift` has no scalar file (the default gain), `lift1` sets gain 1,
        // `painted` draws its `_albedo` (the bronze) over a white diffuse.
        ("textures/pbr/reflect/lift_orm.dds", orm("lift")),
        ("textures/pbr/reflect/lift1_orm.dds", orm("lift1")),
        ("textures/pbr/reflect/lift1.toml", b"# the unlifted bronze\nmetal_gain = 1\n".to_vec()),
        ("textures/pbr/reflect/painted_orm.dds", orm("painted")),
        ("textures/pbr/reflect/painted_albedo.dds", bronze),
    ]
}

/// The test aid that adds the field's generated files ([`fixture`]) to the game's own mount,
/// so the mount has authored sets and the world camera gets its reflection cube (nvforever
/// reflection_sky.rs; the scenarios `authored-reflections-sky` and `-off`). Nothing the game
/// draws uses the files' textures: they only make the mount's index count authored sets
/// [engineering policy: no game-derived file, no game folder written].
pub const MOUNT_PACK_ENV: &str = "NVF_PLAY_TEST_PBR_MOUNT_PACK";

/// Whether [`MOUNT_PACK_ENV`] asks for the mount pack (`=1`).
pub fn mount_pack_requested() -> bool {
    std::env::var_os(MOUNT_PACK_ENV).is_some_and(|v| v == "1")
}

/// The home the mount pack may write under: `NVFOREVER_HOME` when it is set and not empty
/// (`fnv_install` takes an empty value for unset, the player's default home), else a refusal.
pub fn mount_pack_home(value: Option<std::ffi::OsString>) -> Result<PathBuf, String> {
    match value.filter(|v| !v.is_empty()) {
        Some(home) => Ok(PathBuf::from(home)),
        None => Err(format!("{MOUNT_PACK_ENV}=1 needs an explicit, non-empty NVFOREVER_HOME")),
    }
}

/// Write [`fixture`]'s files into a fresh folder of this run under the explicit home
/// ([`mount_pack_home`]): `<home>\pbr-mount-pack\run-<stamp>-<process>-<n>`, reserved by
/// creating it (never an existing folder), every file created new (`create_new`), so no file
/// that existed before is opened for writing. On any failure the run's own folder is removed
/// again and nothing is mounted. The caller mounts the folder over every other loose layer
/// for this run only (`GameData::open_for_plugins_in_over`); no mods folder is written. The
/// folder and the number of files written.
pub fn seed_mount_pack() -> Result<(PathBuf, usize), String> {
    let home = mount_pack_home(std::env::var_os("NVFOREVER_HOME"))?;
    let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("clock: {e}"))?.as_nanos();
    seed_mount_pack_in(&home, stamp, std::process::id())
}

pub(super) fn seed_mount_pack_in(home: &Path, stamp: u128, process: u32) -> Result<(PathBuf, usize), String> {
    let dir = new_output_in(home, "pbr-mount-pack", stamp, process).map_err(|e| format!("fresh fixture folder under {}: {e}", home.display()))?;
    let written = write_pack(&dir);
    match written {
        Ok(n) => Ok((dir, n)),
        Err(e) => {
            // The folder is this run's own (just reserved): nothing else is in it.
            let _ = std::fs::remove_dir_all(&dir);
            Err(format!("{}: {e}; nothing mounted", dir.display()))
        }
    }
}

/// [`fixture`]'s files under `out`, a folder of the caller's own: every folder made inside it,
/// every file created new.
fn write_pack(out: &Path) -> std::io::Result<usize> {
    let files = fixture();
    for (rel, bytes) in &files {
        let path = out.join(rel);
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        create_file(&path, bytes)?;
    }
    Ok(files.len())
}

fn write_fixture(out: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(out.join("textures").join("reflect"))?;
    std::fs::create_dir_all(out.join("textures").join("pbr").join("reflect"))?;
    for (rel, bytes) in fixture() {
        create_file(&out.join(rel), &bytes)?;
    }
    let faces: Vec<String> = FACES.iter().map(|(n, _, c)| format!("{n} ({:.2}, {:.2}, {:.2})", c.x, c.y, c.z)).collect();
    create_file(
        &out.join("scene.txt"),
        format!(
            "{} panels in {ROWS} rows of {COLS} cells of {CELL} px, 2x2 quads (white diffuse; row three a bronze diffuse at the default metal gain and at gain 1, and a white diffuse with a bronze _albedo), each tilted so its reflection (orthographic view, V = +Z) points to: {}; two cameras of one pose, exposure Bevy's default, MSAA 4x, ambient display value {AMBIENT} (world view, brightness light_unit), no directional light, no tonemapping; camera A has a reflection cube (linear faces {}; level L mixed towards ({:.2}) by L / 5), camera B none\n",
            PANELS.len(),
            PANELS.iter().map(|p| format!("{} {}", p.name, face_name(p.reflect))).collect::<Vec<_>>().join(", "),
            faces.join(", "),
            MEAN.x,
        )
        .as_bytes(),
    )
}

#[cfg(test)]
mod tests;
