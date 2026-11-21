//! Default-off field for authored PBR material sets (#2981; docs/spec-notes/pbr-materials.md
//! "Where a set does not apply, and checking one"), behind `NVF_PLAY_TEST_PBR_FIELD=1`, modelled on
//! `bc5_field.rs`. Every file is generated here, into a fresh folder under
//! `NVFOREVER_HOME`, and read through a real VFS over it; the panels' materials come from the
//! real `shape_material` with the device's actual BC support; two GPU readbacks are judged.
//!
//! Five panels, side by side under one directional light (with components along X, Y and Z,
//! so a normal tilted along Y, or read with its Y reversed, changes N·L) and a fixed ambient
//! light, all with the same diffuse bytes under five names:
//! - `plain`: no set;
//! - `lit`: `_orm` with occlusion 1 (roughness 1, metallic 0; 16x16) and a two-channel `_n`
//!   (unsigned BC5, 32x32, X 0 and Y 0.6: not flat), decoded on a worker thread
//!   (`texture::worker_texture_exact`) and adopted by the main-thread build;
//! - `dark`: the same as `lit` but for the ORM's red (occlusion 40/255): the only difference;
//! - `reference`: `lit`'s `_orm`, and the same normal as a three-channel map in the game's
//!   normal slot;
//! - `flat`: `lit`'s `_orm`, no normal map.
//!
//! On: `dark` darker than `lit` (the occlusion, alone), `lit` equal to `reference` (the
//! two-channel rebuild gives the three-channel normal, its Y included), `reference` darker
//! than `flat` by the observed sign (the normal shows, tilted the way the game's maps tilt).
//! Off: `plain`, `lit`, `dark` and `flat` equal, and `reference` (its game normal map is drawn
//! without a set too) darker than them by the observed sign, so a frame of identical panels,
//! or one whose normals' Y is reversed for every map alike, fails. Each readback logs every
//! panel's mean and the signed differences. The thresholds are an engineering policy set
//! from real readbacks (below); nothing here is a fact about the game.
//!
//! Two more panels (stage 4, height maps), judged apart from the five: `height` and
//! `height-control`, the same quad tilted about Y by [`TILT_DEGREES`] (only these two; the
//! camera and the five panels are as before), the same diffuse (a narrow bright stripe at
//! U 0.5 on a dark ground) under two names; `height` has a height-only set whose `_p` is
//! white (high) in its upper half and black (low) in its lower half, at depth
//! [`HEIGHT_DEPTH`]; the control has none. The camera is orthographic, so the control is the
//! identical tilted frame without `_p`. Per band the stripe's centre is measured in both
//! panels, away from the band boundary: with the setting on, the low band's stripe must move
//! to within [`SHIFT_TOLERANCE`] pixels of what the tangent-space view predicts
//! ([`predicted_low_shift`]: towards decreasing mesh U here) and the high band's must stay
//! within [`STILL`]; with it off, neither moves more than [`STILL`]. A wrong sign or no
//! movement fails. The height panels report on their own lines (`pbr field height: ...`,
//! the shifts in pixels, the brightest value and the ground level on every readback, a PASS
//! or FAIL per readback and a final PASS), so a height problem never fails the five panels'
//! verdict or their scenarios. [observed, field only, on `b4660af00`, the main session's
//! first GPU run (setting on), at the depth of then (0.1) and the occlusion method of then:
//! high +0.00 px, low −9.76 px against the predicted −9.81;
//! stripe centres height 64.00/54.24, control 64.00/64.00 px (high/low band); the then sRGB
//! 230 stripe clipped at 255, so the stripe and its ground were darkened (sRGB 180 on 90).]
//! The thresholds are an engineering policy set from that readback with margins. The depth
//! is 0.08 since the scalar range was cut to it (prediction −7.84 px) and the method relief
//! with 4 steps (both 2026-11-20); the field has not been run at them yet.
//!
//! [observed, on `7e2de4be1`, the main session's runs: on, plain 103, lit 67, dark 38,
//! reference 67, flat 103; off, plain, lit, dark and flat 103, reference 60.] Two of these
//! values have explanations that are [inferred], not isolated by a control: `flat` draws as
//! `plain` although their roughness and reflectance differ, because in this view and light
//! only the faint direct highlight would show them; `reference` is brighter on than off
//! because on it takes the set's reflectance (0.5) and roughness where the game's shape,
//! without the Specular flag, has reflectance 0 (roughness also enters Bevy's diffuse term,
//! so the split between the two is not measured).

use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, atomic::{AtomicU64, Ordering}};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use bevy::asset::RenderAssetUsages;
use bevy::camera::{Exposure, RenderTarget, ScalingMode, visibility::RenderLayers};
use bevy::core_pipeline::tonemapping::{DebandDither, Tonemapping};
use bevy::image::{CompressedImageFormatSupport, CompressedImageFormats};
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
use crate::texture::{worker_texture_exact, TextureSnapshot, TextureUsage};

const LAYER: usize = 30;
const PANEL: usize = 128;
/// The panels judged by mean luminance (stage 3).
const PANELS: usize = 5;
/// Every panel: the five, then the height panel and its control (stage 4).
const TOTAL: usize = PANELS + 2;
const WIDTH: usize = PANEL * TOTAL;
const HEIGHT: usize = PANEL;
/// The judged region of each panel: its middle half, away from the edges.
const ROI: std::ops::Range<usize> = 32..96;
const NAMES: [&str; PANELS] = ["plain", "lit", "dark", "reference", "flat"];
const PLAIN: usize = 0;
const LIT_PANEL: usize = 1;
const DARK: usize = 2;
const REFERENCE: usize = 3;
const FLAT: usize = 4;
const HEIGHT_PANEL: usize = 5;
const CONTROL_PANEL: usize = 6;
/// Every panel's name (the files drop the hyphen: `heightcontrol.dds`).
const ALL_NAMES: [&str; TOTAL] = ["plain", "lit", "dark", "reference", "flat", "height", "height-control"];

/// The height panels' tilt about Y: their +U edge turns away from the camera, so the
/// tangent-space view has a negative X and the march's samples move to larger U with depth
/// (vendored `parallax_mapping.wesl`, `pbr_fragment.wesl`).
const TILT_DEGREES: f32 = 50.0;
/// The height-only set's depth (its scalar file's `parallax_depth`): the scalar range's
/// upper end (`fnv_pbr::PARALLAX_DEPTH_RANGE`, 0.08 since the pictures of 2026-11-20; the
/// field ran at 0.1 before), so the low band's stripe travels several pixels
/// ([`predicted_low_shift`], about 7.8).
const HEIGHT_DEPTH: f32 = 0.08;
/// The stripe diffuse (64 x 64): ground and stripe colours (sRGB) and the stripe's columns,
/// centred on U 0.5. Mid values: an sRGB 230 stripe read 255 on the tilted panel (module
/// notes, observed); the five panels' grey 160 reads 103 flat on, and the tilted panels take
/// more of the light, so 180 should read about 150-200 and the ground (90) about 35-45
/// [estimate from those two readings, to be confirmed by the next run's logged values].
const STRIPE_SIDE: u32 = 64;
const STRIPE_GROUND: [u8; 4] = [90, 90, 90, 255];
const STRIPE: [u8; 4] = [180, 180, 180, 255];
const STRIPE_TEXELS: std::ops::Range<u32> = 30..34;
/// The `_p` (BC4 unsigned, 16 x 16): 255 (high) in the upper half, 0 (low) in the lower.
const HEIGHT_SIDE: u32 = 16;
/// The judged rows of each band (screen top is texture V 0, the high band), away from the
/// boundary at 64 and from the wrap at the top and bottom edges; the columns searched for
/// the stripe, inside the tilted quad (about 82 pixels wide, centred).
const HIGH_ROWS: std::ops::Range<usize> = 16..48;
const LOW_ROWS: std::ops::Range<usize> = 80..112;
const STRIPE_COLUMNS: std::ops::Range<usize> = 28..100;

/// The height comparator [engineering policy, set from the readback of `b4660af00` (module
/// notes: low −9.76 against −9.81, high +0.00, control 64.00/64.00) with margins]: the low
/// band's shift (height panel minus control, pixels) within `SHIFT_TOLERANCE` of the
/// prediction, and of its sign (more than `STILL` that way); the high band's within `STILL`
/// (off: both within `STILL`); each control stripe within `STILL` of the panel's middle and
/// of the other band's; a row's stripe at least `MIN_CONTRAST` above its ground; no channel
/// of the judged rows at `CLIP`.
const SHIFT_TOLERANCE: f64 = 2.5;
const STILL: f64 = 1.5;
const MIN_CONTRAST: f64 = 20.0;

/// The low band's predicted stripe shift in pixels (height panel minus control): the march
/// ends at depth 1 (height 0) where its UV offset is `depth × Vt.x / Vt.z` against the view,
/// `Vt` the view direction in tangent space (here `Vt.x = −sin tilt`, `Vt.z = cos tilt`), and
/// one U spans `PANEL × cos tilt` pixels: `−depth × sin tilt × PANEL` [derived from the
/// vendored shader; not measured]. The relief method's search (the default, `relief:4`)
/// stops within its last halving of depth 1, here about 1 % short of it (about 0.1 px),
/// well inside [`SHIFT_TOLERANCE`]; the occlusion method's interpolation lands on it [derived
/// from `parallaxed_uv` for the flat low band; not measured].
fn predicted_low_shift() -> f64 {
    -f64::from(HEIGHT_DEPTH) * f64::from(TILT_DEGREES.to_radians().sin()) * PANEL as f64
}

/// The diffuse (sRGB grey 160), the two ORMs (occlusion 1 and 40/255; roughness 1, metallic
/// 0) and the normal (X 0, Y 0.6, Z 0.8) as generated texels, RGBA byte order.
const DIFFUSE: [u8; 4] = [160, 160, 160, 255];
const ORM_OPEN: [u8; 4] = [255, 255, 0, 255];
const ORM_DARK: [u8; 4] = [40, 255, 0, 255];
const NORMAL_XY: [u8; 2] = [128, 204];
const NORMAL_RGB: [u8; 4] = [128, 204, 230, 255];
/// Sides of the generated maps: each its own (the format's rule, pbr-materials.md).
const DIFFUSE_SIDE: u32 = 8;
const ORM_SIDE: u32 = 16;
const NORMAL_SIDE: u32 = 32;
const N_PATHS: [&str; 2] = ["textures\\pbr\\field\\lit_n.dds", "textures\\pbr\\field\\dark_n.dds"];
const HEIGHT_PATH: &str = "textures\\pbr\\field\\height_p.dds";
const PBR_FILES: [&str; 7] = [
    "textures\\pbr\\field\\lit_orm.dds",
    "textures\\pbr\\field\\lit_n.dds",
    "textures\\pbr\\field\\dark_orm.dds",
    "textures\\pbr\\field\\dark_n.dds",
    "textures\\pbr\\field\\reference_orm.dds",
    "textures\\pbr\\field\\flat_orm.dds",
    HEIGHT_PATH,
];
/// The direction the light comes from (normalised in use).
const LIGHT_FROM: Vec3 = Vec3::new(0.5, 0.5, 0.7);

/// The comparator [engineering policy]. Mean luminance of a judged region, in bytes. On:
/// `lit - dark >= MIN_OCCLUSION`, `|lit - reference| <= SAME`, `|reference - flat| >=
/// MIN_NORMAL` in sign: `flat - reference >= MIN_NORMAL`. Off: `plain`, `lit`, `dark` and
/// `flat` within `SAME_OFF` of each other and `plain - reference >= MIN_NORMAL`. A judged region's mean must lie inside `LIT` and no
/// texel in it may be clipped at `CLIP`. Set from the main session's readbacks of
/// 2026-11-19 (two each, identical), with margins for other GPUs and drivers [observed: on,
/// plain 103, lit 67, dark 38, reference 67, flat 103; off, 103 everywhere but the reference,
/// 60]: occlusion 29 against 15, |lit - reference| 0 against 3, normal 36 (on) and 43 (off)
/// against 15, off differences 0 against 2. The readbacks' own log line is `pbr field:
/// readback N means`.
const MIN_OCCLUSION: f64 = 15.0;
const SAME: f64 = 3.0;
const MIN_NORMAL: f64 = 15.0;
const SAME_OFF: f64 = 2.0;
const LIT: std::ops::Range<f64> = 8.0..247.0;
const CLIP: u8 = 254;

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
    on: bool,
    pending: Option<Entity>,
    completed: u8,
    after_frame: u64,
    done: bool,
    /// Readbacks whose height panels passed their own comparator ([`judge_height`]).
    height_passes: u8,
}

struct Scene {
    entities: Vec<Entity>,
    target: Handle<Image>,
    /// The height panels' materials passed their check ([`check_height_panels`]); a failure
    /// is reported on a height line and leaves the five panels' verdict alone.
    height_ok: bool,
}

pub(crate) fn install(app: &mut App) {
    let Some(value) = std::env::var_os("NVF_PLAY_TEST_PBR_FIELD") else { return };
    if value != "1" {
        error!("pbr field: FAIL expected NVF_PLAY_TEST_PBR_FIELD=1");
        return;
    }
    let frames = RenderFrames::default();
    let Some(render) = app.get_sub_app_mut(RenderApp) else {
        error!("pbr field: FAIL missing normal RenderApp");
        return;
    };
    render.insert_resource(frames.clone()).add_systems(Render, count_rendered.in_set(RenderSystems::Cleanup));
    app.add_plugins(ExtractComponentPlugin::<FieldCamera>::default())
        .insert_resource(frames)
        .insert_resource(Field { started: None, output: None, scene: None, on: false, pending: None, completed: 0, after_frame: 0, done: false, height_passes: 0 })
        .add_systems(Update, tick);
}

fn count_rendered(views: Query<(&ExtractedCamera, &ViewTarget), With<FieldCamera>>, frames: Res<RenderFrames>) {
    if let Ok((camera, _)) = views.single() {
        if camera.physical_target_size == Some(UVec2::new(WIDTH as u32, HEIGHT as u32)) {
            frames.0.fetch_add(1, Ordering::AcqRel);
        }
    }
}

fn tick(world: &mut World) {
    let Some(mut field) = world.remove_resource::<Field>() else { return };
    let started = *field.started.get_or_insert_with(Instant::now);
    if !field.done {
        if let Err(reason) = advance(world, &mut field, started.elapsed()) {
            error!("pbr field: FAIL {reason}");
            field.done = true;
        }
    }
    if field.done {
        if let Some(entity) = field.pending.take() {
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

fn advance(world: &mut World, field: &mut Field, elapsed: Duration) -> Result<(), String> {
    if elapsed >= Duration::from_secs(30) {
        return Err("timeout before two completed rendered comparisons".into());
    }
    check_layer(world)?;
    if field.scene.is_none() {
        let Some(data) = world.get_resource::<GameData>().cloned() else { return Ok(()) };
        let home = std::env::var_os("NVFOREVER_HOME").ok_or("missing explicit owned NVFOREVER_HOME")?;
        let stamp = SystemTime::now().duration_since(UNIX_EPOCH).map_err(|e| format!("output clock: {e}"))?.as_nanos();
        let output = new_output(Path::new(&home), stamp, std::process::id()).map_err(|e| format!("fresh run output: {e}"))?;
        write_fixture(&output).map_err(|e| format!("generated fixture: {e}"))?;
        let local = owned_data(&output, data.schema.clone())?;
        field.on = crate::authored::enabled();
        info!("pbr field: evidence directory {}", output.display());
        field.scene = Some(make_scene(world, &local, field.on)?);
        field.output = Some(output);
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
    } else {
        Ok(())
    }
}

fn foreign_layer(layers: &RenderLayers, anchor: Option<&TransmissionAnchor>) -> bool {
    anchor.is_none() && layers.intersects(&RenderLayers::layer(LAYER))
}

/// A VFS over the generated files, after checking each reads back as written (the fixture's
/// own check reads every file, the set members included, straight from the VFS).
fn owned_data(out: &Path, schema: Arc<fnv_nif::Schema>) -> Result<GameData, String> {
    let vfs = fnv_vfs::Vfs::open(out, &[]).map_err(|e| format!("owned VFS: {e}"))?;
    for (rel, expected) in fixture() {
        let path = rel.replace('/', "\\");
        let actual = vfs.read(&path).ok_or_else(|| format!("missing owned VFS {path}"))?.map_err(|e| format!("owned VFS {path}: {e}"))?;
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

/// The NIF texture paths of a panel's shape (diffuse, then the game's normal map).
fn panel_textures(name: &str) -> Vec<String> {
    let mut t = vec![format!("textures\\field\\{}.dds", name.replace('-', ""))];
    if name == "reference" {
        t.push("textures\\field\\reference_n.dds".into());
    }
    t
}

fn make_scene(world: &mut World, data: &GameData, on: bool) -> Result<Scene, String> {
    let formats = world.get_resource::<CompressedImageFormatSupport>().map(|s| s.0).ok_or("missing the device's compressed format support")?;
    let support = CompressedImageFormatSupport(formats);
    let bc = formats.contains(CompressedImageFormats::BC);
    let mut cache = TextureCache::default();
    let prepared = world.resource_scope(|_, mut images: Mut<Assets<Image>>| -> Result<(Vec<(Mesh, NifMaterial)>, bool), String> {
        // The authored `_n` maps decoded on a worker thread under reserved handles, as a model
        // prepared there would have them, then adopted by the main thread's build (off: the
        // material path reads no file under textures\pbr\).
        let mut worker_n = Vec::new();
        if on {
            let (provider, workers, snapshot) = (images.get_handle_provider(), cache.workers.clone(), TextureSnapshot::new());
            for path in N_PATHS {
                let decoded = std::thread::scope(|s| {
                    let (snapshot, workers, provider) = (&snapshot, workers.clone(), provider.clone());
                    s.spawn(move || worker_texture_exact(path, TextureUsage::NormalTwoChannel, data, snapshot, &workers, &provider, bc)).join()
                })
                .map_err(|_| "worker decode panicked".to_string())?;
                worker_n.push(decoded.ok_or_else(|| format!("{path} not decoded on the worker"))?);
            }
            cache.take_workers(&mut images, 0.0);
        }
        let mut prepared = Vec::with_capacity(TOTAL);
        for name in ALL_NAMES {
            let mut mesh = mesh();
            let mut props = Props::default();
            props.textures = panel_textures(name);
            let (material, missing) = shape_material(&props, &mut mesh, &mut cache, data, &mut images, Some(&support));
            if missing.is_some() || material.base.base_color_texture.is_none() || material.base.unlit {
                return Err(format!("actual NIF material refused {name}"));
            }
            prepared.push((mesh, material));
        }
        let size = |h: &Option<Handle<Image>>| h.as_ref().and_then(|h| images.get(h)).map(|i| (i.size(), i.texture_descriptor.format));
        let m: Vec<&NifMaterial> = prepared.iter().map(|(_, m)| m).collect();
        if on {
            let want_format = if bc { TextureFormat::Bc5RgUnorm } else { TextureFormat::Rg8Unorm };
            for (panel, (worker_handle, worker_format, _)) in [LIT_PANEL, DARK].into_iter().zip(&worker_n) {
                let b = &m[panel].base;
                let orm = b.metallic_roughness_texture.clone();
                if orm.is_none() || b.occlusion_texture != orm || b.specular_texture.is_some() || !crate::authored::is_authored(m[panel]) {
                    return Err(format!("{} panel: the ORM is not in both slots", NAMES[panel]));
                }
                if b.normal_map_texture.as_ref() != Some(worker_handle) {
                    return Err(format!("{} panel: the normal map is not the worker's _n", NAMES[panel]));
                }
                let (diffuse, orm, normal) = (size(&b.base_color_texture), size(&orm), size(&b.normal_map_texture));
                let sides = [diffuse.map(|d| d.0), orm.map(|o| o.0), normal.map(|n| n.0)];
                if sides != [Some(UVec2::splat(DIFFUSE_SIDE)), Some(UVec2::splat(ORM_SIDE)), Some(UVec2::splat(NORMAL_SIDE))] {
                    return Err(format!("{} panel: image sizes {sides:?}", NAMES[panel]));
                }
                if normal.map(|n| n.1) != Some(want_format) || *worker_format != want_format {
                    return Err(format!("{} panel: normal format {normal:?}, worker {worker_format:?}, expected {want_format:?}", NAMES[panel]));
                }
            }
            // `lit` and `dark` differ only in their ORM (its red): every other drawn value is
            // equal (their `_n` files hold the same bytes, under each set's own name).
            let without_orm = |m: &NifMaterial| {
                let mut b = m.base.clone();
                (b.metallic_roughness_texture, b.occlusion_texture, b.base_color_texture, b.normal_map_texture) = (None, None, None, None);
                format!("{b:?}")
            };
            if without_orm(m[LIT_PANEL]) != without_orm(m[DARK]) || m[LIT_PANEL].base.metallic_roughness_texture == m[DARK].base.metallic_roughness_texture {
                return Err("lit and dark panels differ in more than their ORM".into());
            }
            let c = &m[REFERENCE].base;
            if c.metallic_roughness_texture.is_none() || c.specular_texture.is_some() || size(&c.normal_map_texture).map(|n| n.1) != Some(TextureFormat::Rgba8Unorm) {
                return Err("reference panel: expected the set's ORM and the three-channel game normal".into());
            }
            if m[FLAT].base.metallic_roughness_texture.is_none() || m[FLAT].base.normal_map_texture.is_some() || m[PLAIN].base.metallic_roughness_texture.is_some() {
                return Err("flat or plain panel: unexpected maps".into());
            }
            info!("pbr field: authored sets found (orm in both slots, normal {want_format:?} decoded on a worker); sizes diffuse {DIFFUSE_SIDE}x{DIFFUSE_SIDE}, orm {ORM_SIDE}x{ORM_SIDE}, normal {NORMAL_SIDE}x{NORMAL_SIDE}");
        } else {
            if m.iter().any(|m| m.base.metallic_roughness_texture.is_some() || m.base.occlusion_texture.is_some() || crate::authored::is_authored(m) || m.extension.legacy.is_some()) {
                return Err("setting off: a panel carries an authored map or a legacy look".into());
            }
            if data.pbr_index_built() || PBR_FILES.iter().any(|p| cache.has_or_ahead(p)) {
                return Err("setting off: the index was built or a textures\\pbr\\ file was loaded".into());
            }
            let plain_fields = |m: &NifMaterial| {
                let mut b = m.base.clone();
                b.base_color_texture = None;
                format!("{b:?}")
            };
            if [LIT_PANEL, DARK, FLAT].iter().any(|&i| plain_fields(m[PLAIN]) != plain_fields(m[i])) {
                return Err("setting off: the plain, lit, dark and flat materials differ".into());
            }
            info!("pbr field: setting off, no index built and no textures\\pbr\\ file loaded by the material path (the fixture's own check reads the generated files first)");
        }
        // The height panels report on their own lines, never failing the five's checks.
        let height_ok = match check_height_panels(m[HEIGHT_PANEL], m[CONTROL_PANEL], on, size) {
            Ok(note) => {
                info!("pbr field height: panels (height, height-control) tilted {TILT_DEGREES} degrees about Y: {note}");
                true
            }
            Err(why) => {
                error!("pbr field height: FAIL materials: {why}");
                false
            }
        };
        Ok((prepared, height_ok))
    })?;
    let (prepared, height_ok) = prepared;
    info!("pbr field: actual NIF material source, {PANELS} panels ({}), setting {}, BC {}", NAMES.join(", "), if on { "on" } else { "off" }, if bc { "yes" } else { "no" });
    let mut image = Image::new_target_texture(WIDTH as u32, HEIGHT as u32, TextureFormat::Rgba8Unorm, None);
    image.texture_descriptor.usage |= TextureUsages::COPY_SRC;
    let target = world.resource_mut::<Assets<Image>>().add(image);
    let mut entities = Vec::with_capacity(TOTAL + 2);
    // The camera is orthographic and looks straight down -Z: a panel's picture does not
    // depend on where along X it stands, so the five draw as before.
    let middle = (TOTAL as f32 - 1.0) / 2.0;
    for (i, (mesh, material)) in prepared.into_iter().enumerate() {
        let mesh = world.resource_mut::<Assets<Mesh>>().add(mesh);
        let material = world.resource_mut::<Assets<NifMaterial>>().add(material);
        let mut at = Transform::from_xyz((i as f32 - middle) * 2.0, 0.0, 0.0);
        if i >= PANELS {
            at.rotation = Quat::from_rotation_y(TILT_DEGREES.to_radians());
        }
        entities.push(world.spawn((FieldOwned, Name::new(format!("PBR field panel {}", ALL_NAMES[i])), Mesh3d(mesh), MeshMaterial3d(material),
            at, RenderLayers::layer(LAYER))).id());
    }
    // One directional light for every panel, with a Y component: the normal's Y tilt (and its
    // sign) changes N·L.
    entities.push(world.spawn((FieldOwned, Name::new("PBR field light"),
        DirectionalLight { illuminance: 4.0, shadow_maps_enabled: false, contact_shadows_enabled: false, ..default() },
        Transform::from_translation(LIGHT_FROM.normalize()).looking_at(Vec3::ZERO, Vec3::Z), RenderLayers::layer(LAYER))).id());
    entities.push(world.spawn((FieldOwned, FieldCamera, Name::new("PBR field camera"), Camera3d::default(),
        Camera { order: -101, clear_color: ClearColorConfig::Custom(Color::BLACK), ..default() },
        RenderTarget::Image(target.clone().into()),
        Projection::Orthographic(OrthographicProjection { scaling_mode: ScalingMode::Fixed { width: (2 * TOTAL) as f32, height: 2.0 }, near: 0.1, far: 10.0, ..OrthographicProjection::default_3d() }),
        Transform::from_xyz(0.0, 0.0, 2.0).looking_at(Vec3::ZERO, Vec3::Y),
        Exposure { ev100: 0.0 }, AmbientLight { color: Color::WHITE, brightness: 1.0, ..default() },
        Tonemapping::None, DebandDither::Disabled, Msaa::Off, RenderLayers::layer(LAYER))).id());
    Ok(Scene { entities, target, height_ok })
}

/// The height panel's and its control's materials: on, the height panel carries the
/// height-only set's `_p` as its depth map (BC4 unsigned, or R8 without BC support) at
/// [`HEIGHT_DEPTH`] with the run's method and layers (the authored defaults unless a
/// development switch is set), no authored mark, tangents on its
/// drawn mesh, and differs from the control in nothing else but its diffuse; off, neither has
/// a depth map and they differ only in their diffuse. The note for the log line.
fn check_height_panels(height: &NifMaterial, control: &NifMaterial, on: bool, size: impl Fn(&Option<Handle<Image>>) -> Option<(UVec2, TextureFormat)>) -> Result<String, String> {
    // The run's method and layers: the defaults unless a development switch is set
    // (crate::authored::parallax_method, parallax_layers).
    use crate::authored::{parallax_layers, parallax_method, parallax_method_name, Parallax};
    let without = |m: &NifMaterial| {
        let mut b = m.base.clone();
        b.base_color_texture = None;
        Parallax::default().put(&mut b);
        format!("{b:?}")
    };
    if without(height) != without(control) || control.base.depth_map.is_some() || crate::authored::is_authored(height) || crate::authored::is_authored(control) {
        return Err("height panels: the height panel and its control differ in more than the diffuse and the depth map".into());
    }
    if !on {
        if height.base.depth_map.is_some() {
            return Err("setting off: the height panel carries a depth map".into());
        }
        return Ok("setting off, neither has a depth map".into());
    }
    let format = size(&height.base.depth_map);
    let want = [TextureFormat::Bc4RUnorm, TextureFormat::R8Unorm];
    let b = &height.base;
    if !format.is_some_and(|(side, f)| side == UVec2::splat(HEIGHT_SIDE) && want.contains(&f))
        || b.parallax_depth_scale != HEIGHT_DEPTH
        || b.parallax_mapping_method != parallax_method()
        || b.max_parallax_layer_count != parallax_layers()
        || !height.extension.legacy.as_ref().is_some_and(|l| l.tangents)
    {
        return Err(format!("height panel: expected the _p ({HEIGHT_SIDE}x{HEIGHT_SIDE}, unsigned) at depth {HEIGHT_DEPTH} with tangents, got {format:?}, depth {}, {:?}, {} layers", b.parallax_depth_scale, b.parallax_mapping_method, b.max_parallax_layer_count));
    }
    Ok(format!(
        "the height-only set's _p ({:?}) at depth {HEIGHT_DEPTH}, {}, {} layers, on height; none on its control; predicted low-band shift {:+.2} px",
        format.map(|f| f.1),
        parallax_method_name(parallax_method()),
        parallax_layers(),
        predicted_low_shift()
    ))
}

fn received(
    event: On<ReadbackComplete>, mut field: ResMut<Field>, frames: Res<RenderFrames>,
    layers: Query<(&RenderLayers, Option<&TransmissionAnchor>), Without<FieldOwned>>, mut commands: Commands,
) {
    if field.done || field.pending != Some(event.entity) {
        return;
    }
    field.pending = None;
    commands.entity(event.entity).despawn();
    let ordinal = field.completed + 1;
    let frame = frames.0.load(Ordering::Acquire);
    let measured = measure(&event.data);
    // The height panels' verdict, on their own lines (module notes): it never decides the
    // five panels' verdict below.
    let mut height_verdict = "not measured".to_string();
    if let Ok(r) = &measured {
        info!("pbr field: readback {ordinal} means {}", r.describe());
        info!("pbr field height: readback {ordinal} {}", r.height.describe());
        match judge_height(&r.height, field.on) {
            Ok(()) => {
                info!("pbr field height: readback {ordinal} PASS");
                field.height_passes += 1;
                height_verdict = "PASS".into();
            }
            Err(why) => {
                error!("pbr field height: FAIL readback {ordinal}: {why}");
                height_verdict = format!("FAIL {why}");
            }
        }
    }
    let result = if layers.iter().any(|(layers, anchor)| foreign_layer(layers, anchor)) {
        Err("foreign recipient in isolated render layer at completion".into())
    } else {
        measured.and_then(|r| judge_panels(r, field.on))
    };
    let saved = field.output.as_ref().ok_or_else(|| "missing output directory".to_string())
        .and_then(|out| save_readback(out, ordinal, frame, &event.data, &result, &height_verdict).map_err(|e| format!("save readback: {e}")));
    if let Err(reason) = saved.and(result.map(|_| ())) {
        error!("pbr field: FAIL readback {ordinal}: {reason}");
        field.done = true;
        return;
    }
    field.completed = ordinal;
    field.after_frame = frame;
    if ordinal == 1 {
        info!("pbr field: first rendered readback PASS");
    } else {
        info!("pbr field: second rendered readback PASS");
        if field.on {
            info!("pbr field: occlusion alone darkens, the two-channel normal matches the three-channel reference, the normal shows PASS");
        } else {
            info!("pbr field: setting off, plain-material panels identical and the reference distinct PASS");
        }
        let height_ok = field.scene.as_ref().is_some_and(|s| s.height_ok) && field.height_passes == 2;
        match (height_ok, field.on) {
            (true, true) => info!("pbr field height: the height map moves the low band's stripe the predicted way and leaves the high band's in place PASS"),
            (true, false) => info!("pbr field height: setting off, no depth map on either panel, both stripes straight PASS"),
            (false, _) => error!("pbr field height: FAIL {} of 2 readbacks passed (materials {})", field.height_passes, if field.scene.as_ref().is_some_and(|s| s.height_ok) { "as expected" } else { "refused" }),
        }
        field.done = true;
    }
}

/// What a readback measured: per panel the mean luminance of its judged region and its
/// brightest channel value there.
#[derive(Debug, PartialEq)]
struct Report {
    mean: [f64; PANELS],
    brightest: [u8; PANELS],
    height: HeightReport,
}

/// What a readback measured of the height panels: the stripe's centre (pixels from the
/// panel's left edge, mean over the band's rows) in the high and the low band of the height
/// panel and of its control (`None`: some row showed no stripe), the brightest channel value
/// in the two panels' judged rows and their ground level (the mean of each judged row's
/// darkest luminance).
#[derive(Debug, PartialEq)]
struct HeightReport {
    height: [Option<f64>; 2],
    control: [Option<f64>; 2],
    brightest: u8,
    ground: f64,
}

impl HeightReport {
    /// The shift of each band (height panel minus control), `[high, low]`.
    fn shifts(&self) -> Option<[f64; 2]> {
        Some([self.height[0]? - self.control[0]?, self.height[1]? - self.control[1]?])
    }

    /// The shifts in pixels, the prediction and the centres, for the log.
    fn describe(&self) -> String {
        let c = |v: Option<f64>| v.map_or("none".to_string(), |v| format!("{v:.2}"));
        let shifts = self.shifts().map_or("no stripe found".to_string(), |[high, low]| format!("high={high:+.2} px low={low:+.2} px"));
        format!(
            "{shifts} (predicted low {:+.2} px, high 0); centres height {}/{} control {}/{} px (high/low band); brightest {}, ground {:.1}",
            predicted_low_shift(),
            c(self.height[0]),
            c(self.height[1]),
            c(self.control[0]),
            c(self.control[1]),
            self.brightest,
            self.ground
        )
    }
}

/// The stripe's centre in `panel` over `rows`: per row, the luminance-weighted centre of the
/// texels above the half-way value between the row's darkest and brightest in
/// [`STRIPE_COLUMNS`], averaged over the rows; `None` when a row's stripe stands less than
/// [`MIN_CONTRAST`] above its ground.
fn stripe_centre(bytes: &[u8], panel: usize, rows: std::ops::Range<usize>) -> Option<f64> {
    let mut sum = 0.0;
    let count = rows.len();
    for y in rows {
        let row: Vec<(usize, f64)> = STRIPE_COLUMNS.map(|x| (x, luminance(bytes, panel, x, y))).collect();
        let (lo, hi) = row.iter().fold((f64::MAX, f64::MIN), |(lo, hi), &(_, v)| (lo.min(v), hi.max(v)));
        if hi - lo < MIN_CONTRAST {
            return None;
        }
        let half = (lo + hi) / 2.0;
        let (weighted, weight) = row.iter().filter(|(_, v)| *v > half).fold((0.0, 0.0), |(s, w), &(x, v)| (s + (x as f64 + 0.5) * (v - half), w + (v - half)));
        sum += weighted / weight;
    }
    Some(sum / count as f64)
}

/// The luminance of the texel at (`x`, `y`) of `panel`.
fn luminance(bytes: &[u8], panel: usize, x: usize, y: usize) -> f64 {
    let i = (y * WIDTH + panel * PANEL + x) * 4;
    0.2126 * f64::from(bytes[i]) + 0.7152 * f64::from(bytes[i + 1]) + 0.0722 * f64::from(bytes[i + 2])
}

fn measure_height(bytes: &[u8]) -> HeightReport {
    let (mut brightest, mut ground, mut rows) = (0u8, 0.0, 0usize);
    for panel in [HEIGHT_PANEL, CONTROL_PANEL] {
        for y in HIGH_ROWS.chain(LOW_ROWS) {
            let mut darkest = f64::MAX;
            for x in STRIPE_COLUMNS {
                let i = (y * WIDTH + panel * PANEL + x) * 4;
                brightest = brightest.max(bytes[i]).max(bytes[i + 1]).max(bytes[i + 2]);
                darkest = darkest.min(luminance(bytes, panel, x, y));
            }
            ground += darkest;
            rows += 1;
        }
    }
    HeightReport {
        height: [stripe_centre(bytes, HEIGHT_PANEL, HIGH_ROWS), stripe_centre(bytes, HEIGHT_PANEL, LOW_ROWS)],
        control: [stripe_centre(bytes, CONTROL_PANEL, HIGH_ROWS), stripe_centre(bytes, CONTROL_PANEL, LOW_ROWS)],
        brightest,
        ground: ground / rows as f64,
    }
}

/// The height comparator (module notes) on a measured frame.
fn judge_height(h: &HeightReport, on: bool) -> Result<(), String> {
    let fail = |why: &str| Err(format!("height comparator ({}): {why}; {}", if on { "on" } else { "off" }, h.describe()));
    let Some([high, low]) = h.shifts() else { return fail("a band shows no stripe") };
    if h.brightest >= CLIP {
        return fail("clipped");
    }
    let middle = PANEL as f64 / 2.0;
    let (c_high, c_low) = (h.control[0].unwrap_or(0.0), h.control[1].unwrap_or(0.0));
    if (c_high - middle).abs() > STILL || (c_low - middle).abs() > STILL || (c_high - c_low).abs() > STILL {
        return fail("the control's stripe is not straight down the middle");
    }
    if high.abs() > STILL {
        return fail("the high band's stripe moved");
    }
    if on {
        let predicted = predicted_low_shift();
        let along = low * predicted.signum();
        if along < -STILL {
            return fail("the low band's stripe moved the wrong way");
        }
        if along <= STILL {
            return fail("the low band's stripe did not move");
        }
        if (low - predicted).abs() > SHIFT_TOLERANCE {
            return fail("the low band's stripe moved, but not by the predicted amount");
        }
    } else if low.abs() > STILL {
        return fail("setting off, the low band's stripe moved");
    }
    Ok(())
}

impl Report {
    /// Every panel's mean and the differences the comparator judges, for the log.
    fn describe(&self) -> String {
        let m = &self.mean;
        let panels: Vec<String> = NAMES.iter().zip(m).map(|(n, v)| format!("{n}={v:.2}")).collect();
        format!(
            "{}; lit-dark={:.2} |lit-reference|={:.2} flat-reference={:.2} plain-reference={:.2} |plain-lit|={:.2} |plain-dark|={:.2} |plain-flat|={:.2}",
            panels.join(" "),
            m[LIT_PANEL] - m[DARK],
            (m[LIT_PANEL] - m[REFERENCE]).abs(),
            m[FLAT] - m[REFERENCE],
            m[PLAIN] - m[REFERENCE],
            (m[PLAIN] - m[LIT_PANEL]).abs(),
            (m[PLAIN] - m[DARK]).abs(),
            (m[PLAIN] - m[FLAT]).abs(),
        )
    }
}

/// The panels' means, or why the frame cannot be judged (wrong size, not opaque, a blank or
/// clipped panel).
fn measure(bytes: &[u8]) -> Result<Report, String> {
    if bytes.len() != WIDTH * HEIGHT * 4 {
        return Err(format!("wrong byte count {}", bytes.len()));
    }
    if bytes.as_chunks::<4>().0.iter().any(|p| p[3] != 255) {
        return Err("nonopaque target alpha".into());
    }
    let mut report = Report { mean: [0.0; PANELS], brightest: [0; PANELS], height: measure_height(bytes) };
    for panel in 0..PANELS {
        let mut sum = 0.0;
        for y in ROI {
            for x in ROI {
                let i = (y * WIDTH + panel * PANEL + x) * 4;
                let p = &bytes[i..i + 3];
                sum += 0.2126 * f64::from(p[0]) + 0.7152 * f64::from(p[1]) + 0.0722 * f64::from(p[2]);
                report.brightest[panel] = report.brightest[panel].max(p[0]).max(p[1]).max(p[2]);
            }
        }
        report.mean[panel] = sum / (ROI.len() * ROI.len()) as f64;
    }
    if report.mean.iter().any(|m| !LIT.contains(m)) || report.brightest.iter().any(|&b| b >= CLIP) {
        return Err(format!("blank or clipped panel: {report:?}"));
    }
    Ok(report)
}

/// The five panels' comparator (module notes).
fn judge_panels(report: Report, on: bool) -> Result<Report, String> {
    let m = &report.mean;
    let pass = if on {
        // The signs as observed: the tilted normal draws darker than the flat one.
        m[LIT_PANEL] - m[DARK] >= MIN_OCCLUSION && (m[LIT_PANEL] - m[REFERENCE]).abs() <= SAME && m[FLAT] - m[REFERENCE] >= MIN_NORMAL
    } else {
        [LIT_PANEL, DARK, FLAT].iter().all(|&i| (m[PLAIN] - m[i]).abs() <= SAME_OFF) && m[PLAIN] - m[REFERENCE] >= MIN_NORMAL
    };
    if pass { Ok(report) } else { Err(format!("comparator ({}): {}", if on { "on" } else { "off" }, report.describe())) }
}

/// The five panels' comparator on a frame (tests; the height panels are judged by
/// [`judge_height`]).
#[cfg(test)]
fn compare(bytes: &[u8], on: bool) -> Result<Report, String> {
    measure(bytes).and_then(|r| judge_panels(r, on))
}

fn create_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    OpenOptions::new().write(true).create_new(true).open(path)?.write_all(bytes)
}

// A new child per run; earlier evidence is kept. create_dir is the reservation.
fn new_output(home: &Path, stamp: u128, process: u32) -> std::io::Result<PathBuf> {
    let root = home.join("pbr-field-output");
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

fn save_readback(out: &Path, ordinal: u8, frame: u64, bytes: &[u8], report: &Result<Report, String>, height_verdict: &str) -> std::io::Result<()> {
    create_file(&out.join(format!("readback-{ordinal}.rgba")), bytes)?;
    let described = match report {
        Ok(r) => format!("{}\nheight: {}\nheight verdict: {height_verdict}", r.describe(), r.height.describe()),
        Err(e) => e.clone(),
    };
    create_file(&out.join(format!("readback-{ordinal}.txt")), format!("submitted render frames at receipt: {frame}\npanels: {ALL_NAMES:?}\n{described}\n").as_bytes())?;
    if bytes.len() == WIDTH * HEIGHT * 4 {
        let mut ppm = format!("P6\n{WIDTH} {HEIGHT}\n255\n").into_bytes();
        for pixel in bytes.as_chunks::<4>().0 {
            ppm.extend_from_slice(&pixel[..3]);
        }
        create_file(&out.join(format!("readback-{ordinal}.ppm")), &ppm)?;
    }
    Ok(())
}

/// A legacy DDS header (docs/spec-notes/dds.md) for a `side` x `side` texture of one level:
/// 32-bit RGBA in byte order R, G, B, A (masks), or a FourCC block format.
fn header(side: u32, fourcc: Option<&[u8; 4]>) -> Vec<u8> {
    let mut h = vec![0; 128];
    h[..4].copy_from_slice(b"DDS ");
    let pitch = if fourcc.is_some() { 0 } else { side * 4 };
    for (at, n) in [(4, 124u32), (8, 0x1007 | if fourcc.is_some() { 0 } else { 8 }), (12, side), (16, side), (20, pitch), (28, 1), (76, 32), (80, if fourcc.is_some() { 4 } else { 0x41 }), (108, 0x1000)] {
        h[at..at + 4].copy_from_slice(&n.to_le_bytes());
    }
    match fourcc {
        Some(tag) => h[84..88].copy_from_slice(tag),
        None => {
            for (at, n) in [(88, 32u32), (92, 0xff), (96, 0xff00), (100, 0xff0000), (104, 0xff000000)] {
                h[at..at + 4].copy_from_slice(&n.to_le_bytes());
            }
        }
    }
    h
}

/// A uniform `side` x `side` RGBA texture.
fn rgba(side: u32, texel: [u8; 4]) -> Vec<u8> {
    let mut b = header(side, None);
    for _ in 0..side * side {
        b.extend_from_slice(&texel);
    }
    b
}

/// A uniform unsigned BC5 (`ATI2`) texture: each block's red and green endpoints are the
/// texel's, every index 0 [documented: BC5 is two BC4 blocks of 8 bytes, endpoint 0 first].
fn bc5(side: u32, xy: [u8; 2]) -> Vec<u8> {
    let mut b = header(side, Some(b"ATI2"));
    for _ in 0..(side / 4) * (side / 4) {
        b.extend_from_slice(&[xy[0], 0, 0, 0, 0, 0, 0, 0, xy[1], 0, 0, 0, 0, 0, 0, 0]);
    }
    b
}

/// The height panels' diffuse: [`STRIPE_GROUND`] with a [`STRIPE`] over the columns
/// [`STRIPE_TEXELS`], every row alike.
fn stripe() -> Vec<u8> {
    let mut b = header(STRIPE_SIDE, None);
    for _ in 0..STRIPE_SIDE {
        for x in 0..STRIPE_SIDE {
            b.extend_from_slice(if STRIPE_TEXELS.contains(&x) { &STRIPE } else { &STRIPE_GROUND });
        }
    }
    b
}

/// The height-only set's `_p`: unsigned BC4 (`BC4U`), 255 in the upper half's blocks and 0
/// in the lower half's, each block one value (endpoint 0, every index 0) [documented: a BC4
/// block is two endpoint bytes and 48 bits of 3-bit indices].
fn height_bc4() -> Vec<u8> {
    let mut b = header(HEIGHT_SIDE, Some(b"BC4U"));
    let blocks = HEIGHT_SIDE / 4;
    for row in 0..blocks {
        let v = if row < blocks / 2 { 255 } else { 0 };
        for _ in 0..blocks {
            b.extend_from_slice(&[v, 0, 0, 0, 0, 0, 0, 0]);
        }
    }
    b
}

/// Every generated file, by its path under the output folder.
fn fixture() -> Vec<(&'static str, Vec<u8>)> {
    let diffuse = rgba(DIFFUSE_SIDE, DIFFUSE);
    let (open, dark, normal) = (rgba(ORM_SIDE, ORM_OPEN), rgba(ORM_SIDE, ORM_DARK), bc5(NORMAL_SIDE, NORMAL_XY));
    let striped = stripe();
    vec![
        ("textures/field/height.dds", striped.clone()),
        ("textures/field/heightcontrol.dds", striped),
        ("textures/pbr/field/height_p.dds", height_bc4()),
        ("textures/pbr/field/height.toml", format!("parallax_depth = {HEIGHT_DEPTH}\n").into_bytes()),
        ("textures/field/plain.dds", diffuse.clone()),
        ("textures/field/lit.dds", diffuse.clone()),
        ("textures/field/dark.dds", diffuse.clone()),
        ("textures/field/reference.dds", diffuse.clone()),
        ("textures/field/flat.dds", diffuse),
        ("textures/field/reference_n.dds", rgba(4, NORMAL_RGB)),
        ("textures/pbr/field/lit_orm.dds", open.clone()),
        ("textures/pbr/field/lit_n.dds", normal.clone()),
        ("textures/pbr/field/dark_orm.dds", dark),
        ("textures/pbr/field/dark_n.dds", normal),
        ("textures/pbr/field/reference_orm.dds", open.clone()),
        ("textures/pbr/field/flat_orm.dds", open),
    ]
}

fn write_fixture(out: &Path) -> std::io::Result<()> {
    fs::create_dir_all(out.join("textures").join("field"))?;
    fs::create_dir_all(out.join("textures").join("pbr").join("field"))?;
    for (rel, bytes) in fixture() {
        create_file(&out.join(rel), &bytes)?;
    }
    create_file(&out.join("scene.txt"), format!("{PANELS} panels ({}), each a 2x2 quad with normal (0,0,1) and UVs; one directional light from {LIGHT_FROM:?} (normalised), illuminance 4; ambient white, brightness 1; exposure ev100 0; no tonemapping; then the height panels (height, height-control), the same quad tilted {TILT_DEGREES} degrees about Y, a stripe diffuse; height has a height-only set (_p high above, low below, depth {HEIGHT_DEPTH})\n", NAMES.join(", ")).as_bytes())
}

#[cfg(test)]
mod tests;
