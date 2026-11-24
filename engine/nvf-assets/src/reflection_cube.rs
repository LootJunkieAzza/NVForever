//! The reflection cube of authored materials (#2981 reflections;
//! docs/spec-notes/pbr-materials.md "Reflections on authored materials") [design].
//! NVForever's own rendering: nothing here describes the original, whose look has no ambient
//! specular (docs/design/layer3.md); only authored materials reflect.
//!
//! - The run's state ([`enabled`], fixed at the start by [`set_enabled`] from the settings'
//!   Reflections on authored materials and Authored materials rows, nvforever main.rs). On,
//!   every NIF main-pass pipeline gets the opt-in def [`REFLECT_DEF`] (nif_material.rs) and
//!   authored materials are built with metallic 1 (authored.rs `authored_metallic`);
//!   nif_material.wesl holds metallic at 0 in a view without a cube, so such views shade as
//!   with the row off. The world camera's cube and its content are nvforever
//!   reflection_sky.rs; the PBR reflection field (pbr_field/reflect.rs) fills its own camera's.
//! - The gate's state ([`gate_state`]): nvforever lights.rs patches bevy_pbr's
//!   `pbr_functions` so the view's environment map light reaches opted-in fragments only
//!   (nvforever `lights/reflection_gate.rs`). Until it is installed no cube is attached; if it
//!   fails, none ever is, since an ungated `pbr_functions` would give the cube to every lit
//!   shader.
//! - [`ReflectionCube`]: a component holding the cube pair (a specular cube, RGBA16F 32x32
//!   with six levels, and a black 1x1 diffuse cube whose sampler, linear between levels, Bevy
//!   uses for both) and the specular cube's content, written by [`ReflectionCube::fill`] from a
//!   closure `(world direction, level) -> linear RGB`. The render world writes every face of
//!   every level straight into the texture once the image exists, after each new fill
//!   ([`ReflectionCubePlugin`]; the pattern of nvforever ripples.rs: no material or image is
//!   prepared again). [`attach_lights`] puts Bevy's `EnvironmentMapLight` on the entity once
//!   the first fill is on the GPU and the gate is installed, so a view never draws with an
//!   empty cube.
//!
//! The producer supplies the blur: Bevy reads level `perceptual roughness x (levels - 1)`
//! (bevy_pbr `environment_map.wesl`, `radiance_level`), and the closure is asked for each level
//! separately, so a smooth source can evaluate itself with a wider blur per level instead of
//! filtering the level above.

use bevy::asset::RenderAssetUsages;
use bevy::image::{ImageFilterMode, ImageSampler, ImageSamplerDescriptor};
use bevy::light::EnvironmentMapLight;
use bevy::prelude::*;
use bevy::render::render_asset::RenderAssets;
use bevy::render::render_resource::{Extent3d, TexelCopyBufferLayout, TextureDimension, TextureFormat, TextureId, TextureUsages, TextureViewDescriptor, TextureViewDimension};
use bevy::render::renderer::RenderQueue;
use bevy::render::texture::GpuImage;
use bevy::render::{Extract, ExtractSchedule, Render, RenderApp, RenderSystems};
use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, AtomicU8, Ordering};
use std::sync::{Arc, OnceLock};

/// The shader def by which a fragment takes the view's environment map light (nvforever
/// `lights/reflection_gate.rs` writes it into `pbr_functions`; a test there checks the two
/// agree).
pub const REFLECT_DEF: &str = "NVF_PBR_REFLECT";

/// The run's state of reflections on authored materials ([`set_enabled`]).
static RUN: OnceLock<bool> = OnceLock::new();

/// Fix the run's state, before the first material is made and the first NIF pipeline is
/// specialized: the settings' Reflections on authored materials row and Authored materials
/// row both on (nvforever main.rs). Called once at the start; a later call that disagrees is
/// refused with a warning.
pub fn set_enabled(on: bool) {
    if RUN.set(on).is_err() && enabled() != on {
        warn!("authored reflections: already {} for this run; the setting takes effect at the next start", if enabled() { "on" } else { "off" });
    }
}

/// Whether authored materials may reflect a view's [`ReflectionCube`] in this run: as
/// [`set_enabled`] fixed it, else off (the viewer never sets it). On, every NIF main-pass
/// pipeline gets [`REFLECT_DEF`] and authored materials are built with metallic 1, held at 0
/// in any view without a cube; the world camera's cube itself is made only where the mount
/// has authored sets (nvforever reflection_sky.rs).
pub fn enabled() -> bool {
    *RUN.get_or_init(|| false)
}

/// Whether the reflection gate is in `pbr_functions` (nvforever lights.rs).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GateState {
    /// `pbr_functions` not patched yet.
    Pending,
    Installed,
    /// The anchors were not found: no cube is ever attached in this run.
    Failed,
}

static GATE: AtomicU8 = AtomicU8::new(0);

/// Set by nvforever lights.rs once it has patched `pbr_functions` (or failed to).
pub fn set_gate_state(state: GateState) {
    GATE.store(
        match state {
            GateState::Pending => 0,
            GateState::Installed => 1,
            GateState::Failed => 2,
        },
        Ordering::Release,
    );
}

pub fn gate_state() -> GateState {
    match GATE.load(Ordering::Acquire) {
        0 => GateState::Pending,
        1 => GateState::Installed,
        _ => GateState::Failed,
    }
}

/// The specular cube's side at level 0.
pub const SPECULAR_SIDE: u32 = 32;
/// The specular cube's levels: 32, 16, 8, 4, 2, 1.
pub const SPECULAR_LEVELS: u32 = 6;
/// Bytes per RGBA16F texel.
const TEXEL_BYTES: u32 = 8;
const FORMAT: TextureFormat = TextureFormat::Rgba16Float;

/// The side of the specular cube's `level`.
pub fn level_side(level: u32) -> u32 {
    (SPECULAR_SIDE >> level).max(1)
}

/// The bytes of one level: six faces of `side` x `side` texels.
pub fn level_bytes(level: u32) -> usize {
    let side = level_side(level) as usize;
    6 * side * side * TEXEL_BYTES as usize
}

/// The cube direction (not normalised) through the point `(s, t)` (each -1..1, `t` down the
/// rows) of the cube texture's layer `face`, in the layer order and face orientation of
/// cube textures (wgpu, Vulkan and Direct3D: +X, -X, +Y, -Y, +Z, -Z) [documented: the
/// Vulkan specification's cube map face selection table, which the other two share].
pub fn cube_direction(face: usize, s: f32, t: f32) -> Vec3 {
    match face {
        0 => Vec3::new(1.0, -t, -s),
        1 => Vec3::new(-1.0, -t, s),
        2 => Vec3::new(s, 1.0, t),
        3 => Vec3::new(s, -1.0, -t),
        4 => Vec3::new(s, -t, 1.0),
        _ => Vec3::new(-s, -t, -1.0),
    }
}

/// The Bevy world direction (normalised) a texel of the cube stands for: bevy_pbr samples
/// the cube at the world direction with its z negated (`environment_map.wesl`: "Cube maps are
/// left-handed so we negate the z coordinate"; the component's rotation is identity), so the
/// world direction is the cube direction with z negated. The field test's readbacks
/// (pbr_field/reflect.rs) pin this: a panel reflecting world +Z shows the +Z colour.
pub fn world_direction(face: usize, s: f32, t: f32) -> Vec3 {
    let d = cube_direction(face, s, t);
    Vec3::new(d.x, d.y, -d.z).normalize()
}

/// Every level of the specular cube as RGBA16F bytes, faces in layer order, rows top down:
/// the texel at the centre of each cell takes `colour(world direction, level)`, linear RGB,
/// held at 0 and above (a value that is not finite is written as 0); alpha 1.
pub fn specular_levels(colour: impl Fn(Vec3, u32) -> Vec3) -> Vec<Vec<u8>> {
    (0..SPECULAR_LEVELS)
        .map(|level| {
            let side = level_side(level);
            let mut bytes = Vec::with_capacity(level_bytes(level));
            for face in 0..6 {
                for row in 0..side {
                    for col in 0..side {
                        let s = 2.0 * (col as f32 + 0.5) / side as f32 - 1.0;
                        let t = 2.0 * (row as f32 + 0.5) / side as f32 - 1.0;
                        let c = colour(world_direction(face, s, t), level);
                        for v in [c.x, c.y, c.z, 1.0] {
                            let v = if v.is_finite() { v.max(0.0) } else { 0.0 };
                            bytes.extend_from_slice(&half::f16::from_f32(v).to_le_bytes());
                        }
                    }
                }
            }
            bytes
        })
        .collect()
}

/// Bevy's default exposure's inverse, nvforever sky.rs `light_unit`: the light level that
/// makes a colour of 1.0 light a white surface to 1.0 on screen.
pub fn light_unit() -> f32 {
    1.0 / bevy::camera::Exposure::default().exposure()
}

/// The sampler both cubes are read with (Bevy takes the diffuse image's for the pair):
/// linear within and between levels.
fn sampler() -> ImageSampler {
    ImageSampler::Descriptor(ImageSamplerDescriptor {
        mag_filter: ImageFilterMode::Linear,
        min_filter: ImageFilterMode::Linear,
        mipmap_filter: ImageFilterMode::Linear,
        ..default()
    })
}

fn cube_view() -> Option<TextureViewDescriptor<'static>> {
    Some(TextureViewDescriptor { dimension: Some(TextureViewDimension::Cube), ..default() })
}

/// A view's reflection cube pair and the specular cube's content (module notes). Put it on a
/// 3D camera; [`attach_lights`] adds the `EnvironmentMapLight` when the content is on the
/// GPU. Remove both together.
///
/// Units [design, fork-notes design draft 2 item 4]: the texels are linear light and the
/// light's intensity is [`light_unit`], so at the default exposure E (= 1 / light_unit)
/// Bevy's reflected radiance texel x intensity x E is the texel itself. With A the view's
/// exposed ambient (`lights.ambient_color.rgb x E`), a texel meant to match the ambient light
/// satisfies `texel x intensity x E = A x max(A, 1e-6)^1.2`, the fork's world conversion of
/// the ambient highlight (nvforever lights/display_shader.rs `nvf_ambient_ratio`), so at the
/// default exposure `texel = A^2.2` (per channel, A > 1e-6). The calibration check tests this
/// identity later.
#[derive(Component, Clone)]
pub struct ReflectionCube {
    pub specular: Handle<Image>,
    pub diffuse: Handle<Image>,
    levels: Arc<Vec<Vec<u8>>>,
    generation: u64,
    /// The generation the render world has written into the texture.
    uploaded: Arc<AtomicU64>,
}

impl ReflectionCube {
    /// The pair's images, the specular cube not yet filled ([`fill`](Self::fill)).
    pub fn new(images: &mut Assets<Image>) -> Self {
        let mut specular = Image::new_uninit(
            Extent3d { width: SPECULAR_SIDE, height: SPECULAR_SIDE, depth_or_array_layers: 6 },
            TextureDimension::D2,
            FORMAT,
            RenderAssetUsages::default(),
        );
        specular.texture_descriptor.mip_level_count = SPECULAR_LEVELS;
        specular.texture_descriptor.usage = TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST;
        specular.texture_view_descriptor = cube_view();
        specular.sampler = sampler();
        // Black: the ambient light stays the game's flat colour (design draft 2 item 3).
        let mut diffuse = Image::new(
            Extent3d { width: 1, height: 1, depth_or_array_layers: 6 },
            TextureDimension::D2,
            vec![0; 6 * TEXEL_BYTES as usize],
            FORMAT,
            RenderAssetUsages::default(),
        );
        diffuse.texture_view_descriptor = cube_view();
        diffuse.sampler = sampler();
        ReflectionCube { specular: images.add(specular), diffuse: images.add(diffuse), levels: Arc::new(Vec::new()), generation: 0, uploaded: Arc::new(AtomicU64::new(0)) }
    }

    /// Write the specular cube from `colour(world direction, level)` (linear RGB; see
    /// [`specular_levels`]); the render world writes it into the texture at the next frame.
    pub fn fill(&mut self, colour: impl Fn(Vec3, u32) -> Vec3) {
        self.set_levels(specular_levels(colour));
    }

    /// Take levels made elsewhere (by [`specular_levels`], e.g. on a worker thread) as the
    /// specular cube's content: no texel is computed here, the bytes are moved in and the
    /// render world writes them into the texture at the next frame, as for [`fill`](Self::fill).
    pub fn set_levels(&mut self, levels: Vec<Vec<u8>>) {
        self.levels = Arc::new(levels);
        self.generation += 1;
    }

    /// The last fill's levels, as [`specular_levels`] lays them out.
    pub fn levels(&self) -> &[Vec<u8>] {
        &self.levels
    }

    /// How many fills there have been.
    pub fn generation(&self) -> u64 {
        self.generation
    }

    /// The last fill is in the texture.
    pub fn ready(&self) -> bool {
        self.generation > 0 && self.uploaded.load(Ordering::Acquire) >= self.generation
    }

    /// Bevy's component for the pair (module notes, "Units").
    pub fn light(&self) -> EnvironmentMapLight {
        EnvironmentMapLight { diffuse_map: self.diffuse.clone(), specular_map: self.specular.clone(), intensity: light_unit(), ..default() }
    }
}

/// The render world's uploads of the cubes' content, by specular image.
#[derive(Resource, Default)]
struct CubeUploads(HashMap<AssetId<Image>, Upload>);

/// One cube's upload. `K` identifies the GPU texture written ([`TextureId`]; tests use a
/// number).
struct Upload<K = TextureId> {
    /// [`ReflectionCube::generation`] of the levels held.
    seen: u64,
    levels: Arc<Vec<Vec<u8>>>,
    /// Not yet written into the texture.
    pending: bool,
    /// The texture the held levels were last written into: when the GPU image is made again
    /// under the same handle (a new texture, empty), the retained levels are written again,
    /// though no new fill came (a static room may never fill again).
    written_to: Option<K>,
    /// The last incompatible GPU texture reported while waiting for a writable target.
    last_rejected: Option<K>,
    uploaded: Arc<AtomicU64>,
}

// Not derived: a derive would ask `K: Default`, which `TextureId` is not.
impl<K> Default for Upload<K> {
    fn default() -> Self {
        Upload { seen: 0, levels: Arc::default(), pending: false, written_to: None, last_rejected: None, uploaded: Arc::default() }
    }
}

impl<K: Copy + PartialEq> Upload<K> {
    /// Take the cube's content when it changed since the last extraction.
    fn receive(&mut self, cube: &ReflectionCube) {
        self.uploaded = cube.uploaded.clone();
        if cube.generation != self.seen {
            self.seen = cube.generation;
            self.levels = cube.levels.clone();
            self.pending = cube.generation > 0;
        }
    }

    /// The levels to write now into `texture`, or none while it is absent. A missing or
    /// replaced texture invalidates its upload and queues the retained levels again.
    /// Levels of the wrong sizes are dropped; valid levels stay pending until written.
    fn levels_to_write(&mut self, texture: Option<K>) -> Option<Arc<Vec<Vec<u8>>>> {
        if self.written_to.is_some() && self.written_to != texture {
            self.written_to = None;
            self.uploaded.store(0, Ordering::Release);
            self.pending = true;
        }
        texture?;
        if !self.pending {
            return None;
        }
        let sizes_ok = self.levels.len() == SPECULAR_LEVELS as usize && self.levels.iter().enumerate().all(|(l, b)| b.len() == level_bytes(l as u32));
        if sizes_ok {
            Some(self.levels.clone())
        } else {
            self.pending = false;
            None
        }
    }

    /// Report an incompatible target once while waiting for a valid one.
    fn reject(&mut self, texture: K) -> bool {
        self.last_rejected.replace(texture) != Some(texture)
    }

    /// The held generation is in `texture`, for [`ReflectionCube::ready`].
    fn written(&mut self, texture: K) {
        self.pending = false;
        self.written_to = Some(texture);
        self.last_rejected = None;
        self.uploaded.store(self.seen, Ordering::Release);
    }
}

fn extract_cubes(cubes: Extract<Query<&ReflectionCube>>, mut uploads: ResMut<CubeUploads>) {
    let mut live = HashSet::new();
    for cube in &cubes {
        let id = cube.specular.id();
        uploads.0.entry(id).or_default().receive(cube);
        live.insert(id);
    }
    uploads.0.retain(|id, _| live.contains(id));
}

/// Write the extracted levels into each specular cube's texture, after the render assets are
/// prepared (`PrepareResources` follows `PrepareAssets`, which makes the texture) and before
/// the frame draws (the queue's writes go ahead of the frame's command buffers): one write
/// per level covering the six layers. The image asset is never changed after it is made, but
/// should its GPU texture be made again under the same handle, the retained levels are
/// written into the new one ([`Upload::levels_to_write`] compares the texture's id). A texture
/// of another shape than [`ReflectionCube::new`] makes is left alone, with an error.
fn write_cubes(mut uploads: ResMut<CubeUploads>, images: Res<RenderAssets<GpuImage>>, queue: Res<RenderQueue>) {
    for (id, upload) in uploads.0.iter_mut() {
        let gpu = images.get(*id);
        let Some(levels) = upload.levels_to_write(gpu.map(|g| g.texture.id())) else { continue };
        let Some(gpu) = gpu else { continue };
        let texture = gpu.texture.id();
        let d = &gpu.texture_descriptor;
        if d.mip_level_count != SPECULAR_LEVELS
            || d.size != (Extent3d { width: SPECULAR_SIDE, height: SPECULAR_SIDE, depth_or_array_layers: 6 })
            || d.format != FORMAT
            || !d.usage.contains(TextureUsages::COPY_DST)
        {
            if upload.reject(texture) {
                error!("reflection cube: incompatible specular texture ({:?}, {:?}, {} levels, {:?}); upload deferred", d.size, d.format, d.mip_level_count, d.usage);
            }
            continue;
        }
        for (level, bytes) in levels.iter().enumerate() {
            let side = level_side(level as u32);
            let mut target = gpu.texture.as_image_copy();
            target.mip_level = level as u32;
            queue.write_texture(
                target,
                bytes,
                TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(side * TEXEL_BYTES), rows_per_image: Some(side) },
                Extent3d { width: side, height: side, depth_or_array_layers: 6 },
            );
        }
        upload.written(texture);
    }
}

/// Put the `EnvironmentMapLight` of each [`ReflectionCube`] on its entity once its last fill
/// is in the texture and the reflection gate is installed (module notes). With the gate
/// failed, never (one log line).
pub fn attach_lights(mut commands: Commands, cubes: Query<(Entity, &ReflectionCube), Without<EnvironmentMapLight>>, mut told: Local<bool>) {
    for (entity, cube) in &cubes {
        match gate_state() {
            GateState::Pending => return,
            GateState::Failed => {
                if !*told {
                    *told = true;
                    error!("reflection cube: the reflection gate is not installed; no cube is attached in this run");
                }
                return;
            }
            GateState::Installed => {}
        }
        if cube.ready() {
            commands.entity(entity).insert(cube.light());
            info!("reflection cube: attached to {entity} (generation {}, intensity {:.4})", cube.generation(), light_unit());
        }
    }
}

/// The cubes' render-world upload and the main world's attachment (module notes).
pub struct ReflectionCubePlugin;

impl Plugin for ReflectionCubePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostUpdate, attach_lights);
        if let Some(render) = app.get_sub_app_mut(RenderApp) {
            render
                .init_resource::<CubeUploads>()
                .add_systems(ExtractSchedule, extract_cubes)
                .add_systems(Render, write_cubes.in_set(RenderSystems::PrepareResources));
        }
    }
}

#[cfg(test)]
mod tests;
