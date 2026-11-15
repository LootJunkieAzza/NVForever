//! Terrain: one mesh per LAND quadrant for the base texture, plus one alpha-blended copy
//! per additional texture layer whose vertex alpha is the layer's opacity.
//!
//! This multi-pass layering is an interim technique with Bevy's standard material; a
//! single-pass splatting shader replaces it once the layer rules are confirmed against
//! in-game screenshots (docs/design/layer3.md). Facts used: docs/spec-notes/world.md §3.
//! Hypotheses here, each to be checked against screenshots:
//! - VTXT positions index the 17 × 17 quadrant grid row by row, rows south → north;
//! - layers draw in ascending `layer` order over the quadrant's base texture;
//! - the normal map (TXST TX01) is a tangent-space map in the frame of the texture
//!   coordinates: red towards +u (east), green towards +v (north), [`TANGENT_SIGN`];
//!   its alpha (a specular mask on NIF shapes) and the LTEX specular exponent are unused;
//! - each grid square is split along its south-west → north-east diagonal;
//! - a quadrant without BTXT, or a layer with a null texture, uses the INI default
//!   ([`DEFAULT_DIFFUSE`]); drawing such quadrants untextured left hard-edged patches.
//!
//! Measured [observed, world.md §3 "Texture repeat"]: textures repeat every 256 world
//! units ([`TerrainSettings::tile`]) with u = x / 256 and v = y / 256, v growing northwards.
//!
//! # One draw for the base and its first layers
//!
//! Each blended layer is a second lighting of the ground under it: at Goodsprings a
//! ground triangle carries 2.2 painted layers on average, so the ground was lit 3.2 times
//! [observed 2026-11-10: 50 cells, 102,400 base triangles, 224,778 painted layer
//! triangles; docs/design/frame-rate-audit.md §9]. Where the GPU has bindless textures
//! ([`TerrainMaterials::folds`]), a quadrant's first [`folded_layers`] layers are folded
//! into its base instead: one opaque mesh ([`folded_mesh`]) that carries the layers'
//! opacities at each vertex, with a [`TerrainMaterial`] holding their textures, whose
//! shader (terrain.wesl) blends the surface colour and the normal in the layers' order
//! and lights the result once. That is [`FOLDED_LAYERS`] layers, every layer of every
//! quadrant seen (up to 6), when the texture repeat divides the cell: the texture
//! coordinate is then the vertex's place in the cell over the repeat, which the shader
//! works out itself, and its slot carries two more opacities. Otherwise it is
//! [`FOLDED_LAYERS_WITH_UV`], and a quadrant's later layers keep their blended draws over
//! it, in the same order as before (at Goodsprings the fifth and sixth layers are 15 % of
//! the painted triangles and cost 0.18 ms a frame at 2560 x 1440 [observed 2026-11-10]).
//!
//! What differs in the picture: the layers' *surfaces* are blended and then lit, where
//! their *lit colours* were blended. The two agree where a layer is fully opaque or
//! absent; in between, the bumps of two normal maps are mixed before the light falls on
//! them [engineering choice: the interim technique above was to be replaced by this].
//! Without bindless textures (a material would need eight more textures than such a GPU
//! can bind beside the standard ones) every layer keeps its blended draw.
//!
//! # Height blending
//!
//! [design, #2550; docs/design/graphics-enhanced.md §3.12] The settings switch Terrain
//! height blending (`Graphics::terrain_height_blend`, off by default and in both presets)
//! gives the folded material's pipeline `TERRAIN_HEIGHT_BLEND` ([`TerrainLayersKey`]):
//! terrain.wesl then turns the ordered blend into a weight for the base and each folded
//! layer and competes within a depth of the highest share-plus-height score. Height is
//! raw decoded texel brightness less a global or local reference, so a layer comes
//! in through its bright stones and clumps first (terrain_weights.wesl, mirrored on
//! the CPU in the tests). A share of 0 stays 0 and a sole contributor keeps 1: fully painted
//! and unpainted ground looks as without it. The switch flips the materials in place
//! ([`sync_height_blend`]); new materials take the current setting. Start-up test aid
//! `NVF_PLAY_TEST_HEIGHT_BLEND=depth:gain[:source]` configures it without enabling it:
//! defaults `0.10:8:global` (the user's choice from in-game comparisons, 2026-11-15); `local` subtracts a coarser mip at the same UV. Equal heights
//! sharpen unequal shares too; colour and normals use the same resulting weights.
//!
//! Only the folded draw is weighted: a quadrant's later layers that keep their blended
//! draws (past [`folded_layers`]), and every layer where the GPU has no bindless textures,
//! blend as before. Their coverage over the folded result is unchanged, but height does not
//! compete across that draw boundary.
//!
//! # Anti-tiling
//!
//! [design, #2551; docs/design/graphics-enhanced.md §3.13] The settings switch Terrain
//! anti-tiling (`Graphics::terrain_anti_tiling`, off by default and in both presets, not an
//! effect of the Enhanced effects row) gives the folded material's pipeline
//! `TERRAIN_ANTI_TILING` ([`TerrainLayersKey`], nvf_assets `terrain_sampling::anti_tiling_defs`):
//! every texture of the quadrant (the base's and the folded layers', diffuse and normal map)
//! is read at up to three offset and rotated places (Hex3) by a hash of a lattice
//! four repeats on a side laid over the world, and the reads are blended, so the 256-unit
//! repeat no longer shows as a grid (nvf_assets terrain_sampling.wesl and terrain_pbr.wesl).
//! The lattice hangs on world coordinates, so neighbouring quadrants and cells read the
//! same place the same way. Reads take explicit gradients from the plain coordinate
//! (scaled by the view's mip bias), so the hash borders cause no mip jumps. The layers are
//! walked last to first and a texture is read only where its share can be above 0; with
//! height blending on too, the heights come from the same reads. The switch flips the
//! materials in place ([`sync_anti_tiling`]); new materials take the current setting. The
//! texture repeat travels in the material ([`TerrainLayers::texture_repeat`]), also where
//! the mesh carries the coordinate (a repeat that does not divide the cell).
//!
//! The landscape's own NIF materials get the same reads ([`TerrainCtx::base_material`],
//! [`TerrainCtx::layer_material`]: their texture repeat and the switch in
//! `NifSurface::terrain_repeat` and `anti_tiling`; nvf_assets nif_material.rs "Landscape
//! anti-tiling"): a quadrant's base without folded layers, the layers past
//! [`folded_layers`] drawn blended over the folded result, and every layer where the GPU has
//! no bindless textures. Their order, blending and depth bias are as before, and the same
//! world lattice lines them up with the folded draw under them.
//!
//! Not anti-tiled: the depth prepass and the shadows (same depth either way), the normal
//! prepass's normals (screen-space passes that read them keep the plain reads), and the
//! distant LOD terrain's baked textures. Height blending stays within the folded draw.

use crate::world::WorldData;
use fnv_esm::{LoadOrder, Schema as EsmSchema};
use bevy::asset::RenderAssetUsages;
use bevy::ecs::system::SystemParam;
use bevy::mesh::{Indices, MeshVertexBufferLayoutRef, PrimitiveTopology};
use bevy::pbr::{ExtendedMaterial, MaterialExtension, MaterialExtensionKey, MaterialExtensionPipeline};
use bevy::prelude::*;
use bevy::render::render_resource::{AsBindGroup, RenderPipelineDescriptor, ShaderType, SpecializedMeshPipelineError};
use bevy::shader::ShaderRef;
use fnv_world::land::{land_texture, load_land, Land, LandParts, Layer, DEFAULT_DIFFUSE, DEFAULT_NORMAL, QUAD_SIDE, SIDE, SPACING};
use nvf_assets::model::ModelCtx;
use nvf_assets::nif_material::sort_first;
use nvf_assets::{nif_material, NifMaterial};
use std::collections::HashMap;
use std::sync::OnceLock;

#[derive(Resource)]
pub struct TerrainSettings {
    /// World units per texture repeat (256 in the game [observed, see module docs]).
    pub tile: f32,
}

#[derive(Resource, Default)]
pub struct TerrainCache {
    /// LTEX FormID → diffuse texture path.
    textures: HashMap<u32, Option<String>>,
    /// Diffuse texture path → normal map path (the LTEX's TXST TX01).
    normals: HashMap<String, String>,
    base: HashMap<Option<String>, Handle<NifMaterial>>,
    layer: HashMap<(String, usize), Handle<NifMaterial>>,
    /// Base texture and folded layer textures → the material of a folded quadrant.
    folded: HashMap<(Option<String>, Vec<String>, bool), Handle<TerrainMaterial>>,
    /// Quadrants drawn with their first layers folded in, and the layers folded.
    pub folded_quadrants: usize,
    pub folded_layers: usize,
    /// Layers drawn blended over a folded quadrant (past [`folded_layers`]), so far.
    pub overflow_layers: usize,
    pub quadrants_without_base: usize,
    pub layers: usize,
}

impl TerrainCache {
    /// The landscape materials held, for the stress census.
    pub fn share(&self, materials: &Assets<NifMaterial>) -> nvf_assets::release::CacheShare {
        let mut s = nvf_assets::release::CacheShare::default();
        for h in self.base.values().chain(self.layer.values()) {
            s.material(materials, h, 1);
        }
        s
    }
}

/// The layers of a quadrant folded into its base's draw (module notes, "One draw for the
/// base and its first layers") when the shader works the texture coordinate out, and when
/// the mesh has to carry it.
pub const FOLDED_LAYERS: usize = 6;
pub const FOLDED_LAYERS_WITH_UV: usize = 4;

/// How many layers a quadrant folds with textures repeating every `tile` units: all
/// [`FOLDED_LAYERS`] when a cell holds a whole number of repeats (16 at the game's 256),
/// since the texture coordinate of a vertex is then its place in the cell over `tile`,
/// but for whole repeats.
pub fn folded_layers(tile: f32) -> usize {
    if tile > 0.0 && (fnv_world::CELL_SIZE / tile).fract() == 0.0 {
        FOLDED_LAYERS
    } else {
        FOLDED_LAYERS_WITH_UV
    }
}

/// The landscape material of a quadrant whose first layers are folded into the base:
/// the base's StandardMaterial with the layers' textures (terrain.wesl).
pub type TerrainMaterial = ExtendedMaterial<StandardMaterial, TerrainLayers>;

const SHADER: &str = "embedded://nvforever/terrain.wesl";

/// The folded layers' textures, in the layers' order, and how many there are. The values
/// and texture indices sit in bindless tables beside StandardMaterial's, as NifSurface's
/// do (nvf_assets nif_material.rs "Bindless"), so quadrants still draw in one batch.
#[derive(Asset, AsBindGroup, Reflect, Debug, Clone, Default)]
#[data(50, TerrainLayersUniform, binding_array(101))]
#[bindless(limit(512), index_table(range(50..64), binding(100)))]
#[bind_group_data(TerrainLayersKey)]
pub struct TerrainLayers {
    /// Layers held (at most [`FOLDED_LAYERS`]).
    pub count: u32,
    /// Height blending (module notes, "Height blending"): selects the pipeline
    /// ([`TerrainLayersKey`]).
    pub height_blend: bool,
    /// Anti-tiling (module notes, "Anti-tiling"): selects the pipeline
    /// ([`TerrainLayersKey`]) when [`texture_repeat`](Self::texture_repeat) can place it.
    pub anti_tiling: bool,
    /// World units per texture repeat, always (anti-tiling's coordinate), also where the
    /// mesh carries the texture coordinate and [`tile`](Self::tile) is 0.
    pub texture_repeat: f32,
    /// World units per texture repeat when the shader works the texture coordinate out
    /// (the mesh's first texture coordinate then holds the fifth and sixth opacities);
    /// 0 when the mesh carries it.
    pub tile: f32,
    #[texture(51)]
    pub diffuse0: Option<Handle<Image>>,
    #[texture(52)]
    pub normal0: Option<Handle<Image>>,
    #[texture(53)]
    pub diffuse1: Option<Handle<Image>>,
    #[texture(54)]
    pub normal1: Option<Handle<Image>>,
    #[texture(55)]
    pub diffuse2: Option<Handle<Image>>,
    #[texture(56)]
    pub normal2: Option<Handle<Image>>,
    #[texture(57)]
    pub diffuse3: Option<Handle<Image>>,
    #[texture(58)]
    pub normal3: Option<Handle<Image>>,
    #[texture(59)]
    pub diffuse4: Option<Handle<Image>>,
    #[texture(60)]
    pub normal4: Option<Handle<Image>>,
    #[texture(61)]
    pub diffuse5: Option<Handle<Image>>,
    #[texture(62)]
    pub normal5: Option<Handle<Image>>,
    /// The grass fires' burn map (`crate::grass::GrassBurn`): burnt ground is drawn
    /// scorched (terrain.wesl "Scorch"). `None`: no fires, nothing scorched.
    #[texture(63)]
    pub burn: Option<Handle<Image>>,
}

/// What terrain.wesl reads of a [`TerrainLayers`]: `counts` x the layer count, y a bit for
/// each layer that has a normal map, z 1 when it has the burn map, w the feature bits
/// ([`FEATURE_HEIGHT_BLEND`], [`FEATURE_ANTI_TILING`]); `tile` x [`TerrainLayers::tile`],
/// y [`TerrainLayers::texture_repeat`]; `height` the start-up
/// (depth, gain, source ID: global 0 / local 1, reserved) [design, #2550].
#[derive(Clone, Default, ShaderType)]
pub struct TerrainLayersUniform {
    pub counts: UVec4,
    pub tile: Vec4,
    pub height: Vec4,
}

/// Start-up comparison parameters [design, #2550], independent of the on/off key.
#[derive(Clone, Copy, Debug, PartialEq)]
struct HeightBlendParams {
    depth: f32,
    gain: f32,
    local: bool,
}

impl Default for HeightBlendParams {
    fn default() -> Self {
        Self { depth: 0.10, gain: 8.0, local: false }
    }
}

impl HeightBlendParams {
    fn uniform(self) -> Vec4 {
        Vec4::new(self.depth, self.gain, if self.local { 1.0 } else { 0.0 }, 0.0)
    }
}

/// The smallest transition depth accepted (and the shader's floor) [design, #2550].
const MIN_DEPTH: f32 = 0.001;

/// Pure parser; invalid input falls back as a whole at the environment boundary.
fn parse_height_blend(value: &str) -> Result<HeightBlendParams, &'static str> {
    let fields: Vec<_> = value.split(':').collect();
    if !(2..=3).contains(&fields.len()) {
        return Err("expected depth:gain[:global|local]");
    }
    let depth: f32 = fields[0].parse().map_err(|_| "invalid depth")?;
    let gain: f32 = fields[1].parse().map_err(|_| "invalid gain")?;
    if !depth.is_finite() || depth < MIN_DEPTH {
        return Err("depth must be finite and at least 0.001");
    }
    if !gain.is_finite() || gain < 0.0 {
        return Err("gain must be finite and non-negative");
    }
    let local = match fields.get(2).copied().unwrap_or("global") {
        "global" => false,
        "local" => true,
        _ => return Err("source must be global or local"),
    };
    Ok(HeightBlendParams { depth, gain, local })
}

fn height_blend_params() -> &'static HeightBlendParams {
    static PARAMS: OnceLock<HeightBlendParams> = OnceLock::new();
    PARAMS.get_or_init(|| match std::env::var("NVF_PLAY_TEST_HEIGHT_BLEND") {
        Ok(value) => parse_height_blend(&value).unwrap_or_else(|why| {
            warn!("terrain: invalid NVF_PLAY_TEST_HEIGHT_BLEND ({why}); using 0.10:8:global");
            HeightBlendParams::default()
        }),
        Err(std::env::VarError::NotPresent) => HeightBlendParams::default(),
        Err(std::env::VarError::NotUnicode(_)) => {
            warn!("terrain: non-Unicode NVF_PLAY_TEST_HEIGHT_BLEND; using 0.10:8:global");
            HeightBlendParams::default()
        }
    })
}

fn log_height_blend_params() {
    let p = height_blend_params();
    info!("terrain: height blend parameters algorithm=depth depth={} gain={} source={} height_limit=0.25 (start-up configuration; graphics switch enables)", p.depth, p.gain, if p.local { "local" } else { "global" });
}

/// Bit of `counts.w` set while the material blends by height (the pipeline's
/// `TERRAIN_HEIGHT_BLEND` decides what is drawn; the bit records the material's state).
pub const FEATURE_HEIGHT_BLEND: u32 = 1;

/// Bit of `counts.w` set while the material is anti-tiled (as [`FEATURE_HEIGHT_BLEND`]).
pub const FEATURE_ANTI_TILING: u32 = 2;

impl From<&TerrainLayers> for TerrainLayersUniform {
    fn from(l: &TerrainLayers) -> Self {
        let normals = [&l.normal0, &l.normal1, &l.normal2, &l.normal3, &l.normal4, &l.normal5];
        let bits = normals.iter().enumerate().filter(|(_, n)| n.is_some()).fold(0u32, |b, (i, _)| b | 1 << i);
        let most = if l.tile > 0.0 { FOLDED_LAYERS } else { FOLDED_LAYERS_WITH_UV };
        let features = if l.height_blend { FEATURE_HEIGHT_BLEND } else { 0 } | if TerrainLayersKey::from(l).anti_tiling { FEATURE_ANTI_TILING } else { 0 };
        let repeat = if nvf_assets::terrain_sampling::repeat_valid(l.texture_repeat) { l.texture_repeat } else { 0.0 };
        TerrainLayersUniform { counts: UVec4::new(l.count.min(most as u32), bits, u32::from(l.burn.is_some()), features), tile: Vec4::new(l.tile.max(0.0), repeat, 0.0, 0.0), height: height_blend_params().uniform() }
    }
}

/// The part of [`TerrainLayers`] that selects the pipeline.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TerrainLayersKey {
    pub height_blend: bool,
    /// Anti-tiling asked for and a texture repeat to place it with.
    pub anti_tiling: bool,
}

impl From<&TerrainLayers> for TerrainLayersKey {
    fn from(l: &TerrainLayers) -> Self {
        TerrainLayersKey { height_blend: l.height_blend, anti_tiling: l.anti_tiling && nvf_assets::terrain_sampling::repeat_valid(l.texture_repeat) }
    }
}

/// terrain.wesl's switch for height blending, in the main pass only (the prepass and the
/// shadows draw the same depth either way).
fn height_blend_def(descriptor: &mut RenderPipelineDescriptor, key: TerrainLayersKey) {
    use bevy::shader::ShaderDefVal;
    if !key.height_blend || descriptor.vertex.shader_defs.contains(&ShaderDefVal::from("PREPASS_PIPELINE")) {
        return;
    }
    if let Some(fragment) = descriptor.fragment.as_mut() {
        fragment.shader_defs.push("TERRAIN_HEIGHT_BLEND".into());
    }
}

impl MaterialExtension for TerrainLayers {
    fn fragment_shader() -> ShaderRef {
        SHADER.into()
    }
    fn specialize(
        _: &MaterialExtensionPipeline,
        descriptor: &mut RenderPipelineDescriptor,
        _: &MeshVertexBufferLayoutRef,
        key: MaterialExtensionKey<Self>,
    ) -> Result<(), SpecializedMeshPipelineError> {
        // Land fades in and out with its cell (fade.rs), and is opaque.
        nvf_assets::fade_prepass(descriptor, key.mesh_key);
        nvf_assets::depth_equal(descriptor, key.mesh_key);
        height_blend_def(descriptor, key.bind_group_data);
        if key.bind_group_data.anti_tiling {
            nvf_assets::terrain_sampling::anti_tiling_defs(descriptor, false);
        }
        Ok(())
    }
}

/// terrain_weights.wesl, imported by terrain.wesl as `nvforever::terrain_weights`.
const WEIGHTS_SHADER: Handle<bevy::shader::Shader> = bevy::asset::uuid_handle!("8ec9dd26-246b-49a1-91f0-1368b1a56604");

pub struct TerrainPlugin;

impl Plugin for TerrainPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "terrain.wesl");
        bevy::asset::load_internal_asset!(app, WEIGHTS_SHADER, "terrain_weights.wesl", |s, _| bevy::shader::Shader::from_wesl(s, "embedded://nvforever/terrain_weights.wesl"));
        app.add_plugins(MaterialPlugin::<TerrainMaterial>::default()).add_systems(Startup, log_height_blend_params).add_systems(PostUpdate, (sync_height_blend, sync_anti_tiling));
    }
}

/// Whether the settings ask for height blending (module notes); false without the
/// `Graphics` resource.
fn height_blend_wanted(graphics: Option<&crate::graphics::Graphics>) -> bool {
    graphics.is_some_and(|g| g.terrain_height_blend)
}

/// Height blending switched on or off: every folded material flipped in place (the
/// pipeline follows its key), logged with how many there are. New materials take the
/// setting when they are made ([`TerrainCtx::folded_material`]).
pub fn sync_height_blend(graphics: Option<Res<crate::graphics::Graphics>>, mut materials: ResMut<Assets<TerrainMaterial>>, mut last: Local<Option<bool>>) {
    let want = height_blend_wanted(graphics.as_deref());
    if *last == Some(want) {
        return;
    }
    let stale: Vec<AssetId<TerrainMaterial>> = materials.iter().filter(|(_, m)| m.extension.height_blend != want).map(|(id, _)| id).collect();
    for id in &stale {
        if let Some(mut m) = materials.get_mut(*id) {
            m.extension.height_blend = want;
        }
    }
    if last.is_some() || want {
        info!(
            "terrain: height blending {} ({} folded quadrant materials, {} changed; layers drawn separately keep the ordered blend)",
            if want { "on" } else { "off" },
            materials.len(),
            stale.len()
        );
    }
    *last = Some(want);
}

/// Whether the settings ask for anti-tiling (module notes); false without the `Graphics`
/// resource.
fn anti_tiling_wanted(graphics: Option<&crate::graphics::Graphics>) -> bool {
    graphics.is_some_and(|g| g.terrain_anti_tiling)
}

/// Anti-tiling switched on or off: every folded material and every landscape NIF material
/// (a texture repeat set, [`terrain_nif_material`]) flipped in place (the pipelines follow
/// their keys), logged with how many of each there are and how many changed. New materials
/// take the setting when they are made ([`TerrainCtx::folded_material`],
/// [`TerrainCtx::base_material`], [`TerrainCtx::layer_material`]). A texture repeat that
/// cannot place the reads (not above 0) keeps the ordinary reads, said when it is switched
/// on.
pub fn sync_anti_tiling(
    graphics: Option<Res<crate::graphics::Graphics>>,
    settings: Option<Res<TerrainSettings>>,
    mut folded: ResMut<Assets<TerrainMaterial>>,
    mut separate: Option<ResMut<Assets<NifMaterial>>>,
    mut last: Local<Option<bool>>,
) {
    let want = anti_tiling_wanted(graphics.as_deref());
    if *last == Some(want) {
        return;
    }
    let stale: Vec<AssetId<TerrainMaterial>> = folded.iter().filter(|(_, m)| m.extension.anti_tiling != want).map(|(id, _)| id).collect();
    for id in &stale {
        if let Some(mut m) = folded.get_mut(*id) {
            m.extension.anti_tiling = want;
        }
    }
    let (mut landscape, mut stale_separate) = (0usize, 0usize);
    if let Some(separate) = separate.as_deref_mut() {
        let ids: Vec<(AssetId<NifMaterial>, bool)> = separate.iter().filter(|(_, m)| m.extension.terrain_repeat != 0.0).map(|(id, m)| (id, m.extension.anti_tiling != want)).collect();
        landscape = ids.len();
        for (id, _) in ids.iter().filter(|(_, stale)| *stale) {
            if let Some(mut m) = separate.get_mut(*id) {
                m.extension.anti_tiling = want;
                stale_separate += 1;
            }
        }
    }
    if want {
        let repeat = settings.as_deref().map_or(0.0, |s| s.tile);
        if !nvf_assets::terrain_sampling::repeat_valid(repeat) {
            warn!("terrain: anti-tiling needs a texture repeat above 0 (it is {repeat}); the textures keep their ordinary reads");
        }
    }
    if last.is_some() || want {
        info!("terrain: anti-tiling {} ({} folded quadrant materials, {} changed) and {landscape} separate landscape materials ({stale_separate} changed)", if want { "on" } else { "off" }, folded.len(), stale.len());
    }
    *last = Some(want);
}

/// Terrain settings and caches; GPU assets come from the shared [`ModelCtx`].
#[derive(SystemParam)]
pub struct TerrainMaterials<'w> {
    pub settings: Res<'w, TerrainSettings>,
    pub cache: ResMut<'w, TerrainCache>,
    pub folded: ResMut<'w, Assets<TerrainMaterial>>,
    device: Option<Res<'w, bevy::render::renderer::RenderDevice>>,
    burn: Option<Res<'w, crate::grass::GrassBurn>>,
    graphics: Option<Res<'w, crate::graphics::Graphics>>,
}

impl TerrainMaterials<'_> {
    /// Whether quadrants fold their first layers into the base (module notes): the GPU
    /// keeps materials' textures in bindless tables. Test aid `NVF_PLAY_TEST_TERRAIN_LAYERS=blended`:
    /// every layer keeps its blended draw, for comparing the two.
    pub fn folds(&self) -> bool {
        static BLENDED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        if *BLENDED.get_or_init(|| std::env::var("NVF_PLAY_TEST_TERRAIN_LAYERS").is_ok_and(|v| v == "blended")) {
            return false;
        }
        static ONCE: std::sync::Once = std::sync::Once::new();
        let folds = self.device.as_ref().is_some_and(|d| bevy::render::material_bind_groups::material_uses_bindless_resources::<TerrainMaterial>(d));
        ONCE.call_once(|| info!("terrain: the first {} texture layers of a quadrant are {} (render device {})", folded_layers(self.settings.tile), if folds { "folded into its base's draw" } else { "blended over it, as every layer" }, if self.device.is_some() { "known" } else { "not known" }));
        folds
    }
}

/// Terrain state plus the shared asset context, for one spawn.
pub struct TerrainCtx<'a, 'w1, 'w2> {
    pub tm: &'a mut TerrainMaterials<'w1>,
    pub assets: &'a mut ModelCtx<'w2>,
}

impl TerrainCtx<'_, '_, '_> {
    /// Diffuse path of an LTEX; `None` (no BTXT, or a null layer texture) is the default.
    fn texture_of(&mut self, world: &WorldData, ltex: Option<u32>) -> Option<String> {
        let cache = &mut *self.tm.cache;
        let Some(id) = ltex else {
            cache.normals.entry(DEFAULT_DIFFUSE.to_string()).or_insert_with(|| DEFAULT_NORMAL.to_string());
            return Some(DEFAULT_DIFFUSE.to_string());
        };
        if let Some(diffuse) = cache.textures.get(&id) {
            return diffuse.clone();
        }
        let tex = land_texture(world.lo, world.schema, id);
        let diffuse = tex.as_ref().and_then(|t| t.diffuse.clone());
        if let (Some(d), Some(n)) = (&diffuse, tex.and_then(|t| t.normal)) {
            cache.normals.entry(d.clone()).or_insert(n);
        }
        cache.textures.insert(id, diffuse.clone());
        diffuse
    }

    fn image(&mut self, path: &str, srgb: bool) -> Option<Handle<Image>> {
        let a = &mut *self.assets;
        a.textures.get(path, srgb, &a.data, &mut a.images, a.formats.as_deref())
    }

    fn normal(&mut self, path: &str) -> Option<Handle<Image>> {
        let a = &mut *self.assets;
        a.textures.normal(path, &a.data, &mut a.images, a.formats.as_deref()).map(|(image, _)| image)
    }

    /// The lit surface of a landscape texture: diffuse and normal map. `None` for the
    /// diffuse path (an LTEX without a texture set) gives a plain grey.
    fn surface(&mut self, tex: Option<&str>) -> StandardMaterial {
        let image = tex.and_then(|t| self.image(t, true));
        let normal = tex.and_then(|t| self.tm.cache.normals.get(t).cloned()).and_then(|n| self.normal(&n));
        surface_material(image, normal)
    }

    /// A landscape NIF material: `base` with the texture repeat and the anti-tiling switch
    /// (module notes, "Anti-tiling").
    fn terrain_nif(&self, base: StandardMaterial) -> NifMaterial {
        terrain_nif_material(base, self.tm.settings.tile, anti_tiling_wanted(self.tm.graphics.as_deref()))
    }

    fn base_material(&mut self, tex: Option<String>) -> Handle<NifMaterial> {
        if let Some(h) = self.tm.cache.base.get(&tex) {
            return h.clone();
        }
        let m = self.surface(tex.as_deref());
        let m = self.terrain_nif(m);
        let h = self.assets.materials.add(m);
        self.tm.cache.base.insert(tex, h.clone());
        h
    }

    /// The material of a quadrant with `layers` (diffuse paths, in order) folded into the
    /// base `tex`. `None` when a layer's diffuse texture is not found: the quadrant then
    /// keeps its blended layers, which leave such a layer out.
    fn folded_material(&mut self, tex: Option<String>, layers: &[String], tile: f32) -> Option<Handle<TerrainMaterial>> {
        let key = (tex.clone(), layers.to_vec(), tile > 0.0);
        if let Some(h) = self.tm.cache.folded.get(&key) {
            return Some(h.clone());
        }
        let mut textures = Vec::with_capacity(layers.len());
        for l in layers {
            let diffuse = self.image(l, true)?;
            let normal = self.tm.cache.normals.get(l).cloned().and_then(|n| self.normal(&n));
            textures.push((diffuse, normal));
        }
        let mut textures = textures.into_iter();
        let mut next = || textures.next().map_or((None, None), |(d, n)| (Some(d), n));
        let ((diffuse0, normal0), (diffuse1, normal1), (diffuse2, normal2), (diffuse3, normal3), (diffuse4, normal4), (diffuse5, normal5)) = (next(), next(), next(), next(), next(), next());
        let burn = self.tm.burn.as_deref().map(|b| b.0.clone());
        let height_blend = height_blend_wanted(self.tm.graphics.as_deref());
        let anti_tiling = anti_tiling_wanted(self.tm.graphics.as_deref());
        let texture_repeat = self.tm.settings.tile;
        let extension = TerrainLayers { count: layers.len() as u32, tile, height_blend, anti_tiling, texture_repeat, diffuse0, normal0, diffuse1, normal1, diffuse2, normal2, diffuse3, normal3, diffuse4, normal4, diffuse5, normal5, burn };
        let base = self.surface(tex.as_deref());
        let h = self.tm.folded.add(TerrainMaterial { base, extension });
        self.tm.cache.folded.insert(key, h.clone());
        Some(h)
    }

    fn layer_material(&mut self, tex: String, rank: usize) -> Option<Handle<NifMaterial>> {
        if let Some(h) = self.tm.cache.layer.get(&(tex.clone(), rank)) {
            return Some(h.clone());
        }
        let surface = self.surface(Some(&tex));
        surface.base_color_texture.as_ref()?;
        let m = StandardMaterial {
            alpha_mode: AlphaMode::Blend,
            // Draw over the base and earlier layers (also orders the blending), and before
            // every other blended mesh: one without depth (the corn rows of
            // `scol\scolmaizerow*.nif`) was painted over by the layers of the ground
            // behind it and looked see-through [observed].
            depth_bias: sort_first(2.0 * (rank as f32 + 1.0)),
            ..surface
        };
        let m = self.terrain_nif(m);
        let h = self.assets.materials.add(m);
        self.tm.cache.layer.insert((tex, rank), h.clone());
        Some(h)
    }
}

/// The landscape's StandardMaterial for a diffuse texture and normal map ([`TerrainCtx::surface`]).
/// Anti-tiling's surface set-up (nvf_assets terrain_pbr.wesl) reads exactly what this holds:
/// the colour, the diffuse texture, the normal map and scalar values (the contract test).
fn surface_material(image: Option<Handle<Image>>, normal: Option<Handle<Image>>) -> StandardMaterial {
    StandardMaterial {
        base_color: if image.is_some() { Color::WHITE } else { Color::srgb(0.45, 0.42, 0.38) },
        base_color_texture: image,
        normal_map_texture: normal,
        // Green points down the image, as on NIF shapes (docs/spec-notes/nif.md §6
        // "Normal maps").
        flip_normal_map_y: true,
        perceptual_roughness: 0.95,
        reflectance: 0.2,
        ..default()
    }
}

/// A landscape NIF material (a quadrant's base or a layer drawn on its own): `base` with
/// the texture repeat `repeat` (which marks it as the landscape's for [`sync_anti_tiling`])
/// and the anti-tiling switch.
fn terrain_nif_material(base: StandardMaterial, repeat: f32, anti_tiling: bool) -> NifMaterial {
    let mut m = nif_material(base);
    m.extension.terrain_repeat = repeat;
    m.extension.anti_tiling = anti_tiling;
    m
}

fn srgb_to_linear(c: u8) -> f32 {
    let c = c as f32 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Sign (w) of the terrain's vertex tangents: the one Bevy's tangent generator gives a mesh
/// whose texture coordinates grow with x and y (checked in the tests), so the normal maps
/// are read in the same frame as on NIF shapes. With `flip_normal_map_y`, red tilts the
/// normal towards +u (east) and green towards +v (north).
const TANGENT_SIGN: f32 = -1.0;

/// Mesh of one quadrant (17 × 17 vertices) in cell-local game space.
fn quadrant_mesh(land: &Land, q: u8, alpha: Option<&[f32]>, cell_origin: Vec2, tile: f32) -> Option<Mesh> {
    let h = land.heights.as_ref()?;
    let (qx, qy) = ((q % 2) as usize * (QUAD_SIDE - 1), (q / 2) as usize * (QUAD_SIDE - 1));
    let n = QUAD_SIDE * QUAD_SIDE;
    let (mut pos, mut nor, mut uv, mut col) = (Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n), Vec::with_capacity(n));
    let mut tan = Vec::with_capacity(n);
    for r in 0..QUAD_SIDE {
        for c in 0..QUAD_SIDE {
            let (gr, gc) = (qy + r, qx + c);
            let i = gr * SIDE + gc;
            let (x, y) = (gc as f32 * SPACING, gr as f32 * SPACING);
            pos.push([x, y, h[i]]);
            let normal = land
                .normals
                .as_ref()
                .map(|v| Vec3::new(v[i][0] as f32, v[i][1] as f32, v[i][2] as f32).normalize_or(Vec3::Z))
                .unwrap_or(Vec3::Z);
            nor.push(normal.to_array());
            // East (the direction of +u) laid into the surface.
            let east = (Vec3::X - normal * normal.x).normalize_or(Vec3::X);
            tan.push([east.x, east.y, east.z, TANGENT_SIGN]);
            let (wx, wy) = (cell_origin.x + x, cell_origin.y + y);
            uv.push([wx / tile, wy / tile]);
            let rgb = land.colors.as_ref().map_or([1.0; 3], |v| v[i].map(srgb_to_linear));
            let a = alpha.map_or(1.0, |a| a[r * QUAD_SIDE + c]);
            col.push([rgb[0], rgb[1], rgb[2], a]);
        }
    }
    let mut idx = Vec::with_capacity((QUAD_SIDE - 1) * (QUAD_SIDE - 1) * 6);
    // A layer keeps only the triangles it paints: one whose three corners have opacity 0
    // blends to exactly what lies under it (its alpha is 0 across the triangle), so it
    // is left out [engineering choice: the same picture; most layers paint patches, and
    // drawing them over whole quadrants cost about a millisecond of blended terrain a
    // frame outdoors at 1440p, docs/design/frame-rate-audit.md, terrain texture layers,
    // #1886].
    let paints = |t: [u32; 3]| alpha.is_none_or(|a| t.iter().any(|&v| a[v as usize] > 0.0));
    for r in 0..QUAD_SIDE - 1 {
        for c in 0..QUAD_SIDE - 1 {
            let i = (r * QUAD_SIDE + c) as u32;
            let (e, nn, ne) = (i + 1, i + QUAD_SIDE as u32, i + QUAD_SIDE as u32 + 1);
            // Counter-clockwise seen from above (+z).
            for t in [[i, e, ne], [i, ne, nn]] {
                if paints(t) {
                    idx.extend_from_slice(&t);
                }
            }
        }
    }
    if idx.is_empty() {
        return None;
    }
    let mut mesh = Mesh::new(PrimitiveTopology::TriangleList, RenderAssetUsages::RENDER_WORLD);
    mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, pos);
    mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, nor);
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uv);
    mesh.insert_attribute(Mesh::ATTRIBUTE_TANGENT, tan);
    mesh.insert_attribute(Mesh::ATTRIBUTE_COLOR, col);
    mesh.insert_indices(Indices::U32(idx));
    Some(mesh)
}

/// The neighbouring cells' grid squares, in the order [`stitch_layers`] takes their
/// layers: north, east, south, west.
pub const NEIGHBOURS: [(i32, i32); 4] = [(0, 1), (1, 0), (0, -1), (-1, 0)];
const NORTH: usize = 0;
const EAST: usize = 1;
const SOUTH: usize = 2;
const WEST: usize = 3;

/// The positions along one side of a quadrant's 17 × 17 grid, west → east or south →
/// north (rows run south → north, world.md §3).
fn edge_position(side: usize, k: usize) -> usize {
    let last = QUAD_SIDE - 1;
    match side {
        NORTH => last * QUAD_SIDE + k,
        SOUTH => k,
        EAST => k * QUAD_SIDE + last,
        _ => k * QUAD_SIDE,
    }
}

/// The quadrant across `side` of quadrant `q` (0 south-west, 1 south-east, 2 north-west,
/// 3 north-east): in the same cell (`None`) or in the neighbouring cell on that side.
fn across(q: u8, side: usize) -> (Option<usize>, u8) {
    match side {
        EAST if q.is_multiple_of(2) => (None, q + 1),
        EAST => (Some(EAST), q - 1),
        WEST if q % 2 == 1 => (None, q - 1),
        WEST => (Some(WEST), q + 1),
        NORTH if q < 2 => (None, q + 2),
        NORTH => (Some(NORTH), q - 2),
        SOUTH if q >= 2 => (None, q - 2),
        _ => (Some(SOUTH), q + 2),
    }
}

/// The base mesh of quadrant `q` with the opacities of its first layers (`alphas`, at most
/// [`folded_layers`], in the layers' order) at each vertex, where terrain.wesl reads them:
/// the first in the vertex colour's alpha, the second and third in the second texture
/// coordinate, the fourth in the size of the tangent's last number past 1 (its sign stays
/// [`TANGENT_SIGN`]), and, where [`folded_layers`] is [`FOLDED_LAYERS`], the fifth and
/// sixth in the first texture coordinate, which the shader then works out.
fn folded_mesh(land: &Land, q: u8, alphas: &[&[f32]], cell_origin: Vec2, tile: f32) -> Option<Mesh> {
    use bevy::mesh::VertexAttributeValues;
    let mut mesh = quadrant_mesh(land, q, None, cell_origin, tile)?;
    let at = |k: usize, v: usize| alphas.get(k).map_or(0.0, |a| a[v]);
    let n = QUAD_SIDE * QUAD_SIDE;
    if let Some(VertexAttributeValues::Float32x4(col)) = mesh.attribute_mut(Mesh::ATTRIBUTE_COLOR) {
        for (v, c) in col.iter_mut().enumerate() {
            c[3] = at(0, v);
        }
    }
    mesh.insert_attribute(Mesh::ATTRIBUTE_UV_1, (0..n).map(|v| [at(1, v), at(2, v)]).collect::<Vec<_>>());
    if let Some(VertexAttributeValues::Float32x4(tan)) = mesh.attribute_mut(Mesh::ATTRIBUTE_TANGENT) {
        for (v, t) in tan.iter_mut().enumerate() {
            t[3] = TANGENT_SIGN * (1.0 + at(3, v));
        }
    }
    if folded_layers(tile) == FOLDED_LAYERS {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, (0..n).map(|v| [at(4, v), at(5, v)]).collect::<Vec<_>>());
    }
    Some(mesh)
}

/// A layer's opacity over the 17 × 17 grid.
fn alpha_grid(l: &Layer) -> Vec<f32> {
    let mut alpha = vec![0.0f32; QUAD_SIDE * QUAD_SIDE];
    for &(p, o) in &l.alpha {
        if let Some(a) = alpha.get_mut(p as usize) {
            *a = o.clamp(0.0, 1.0);
        }
    }
    alpha
}

/// The opacity a cell's (or a neighbour's) layers give texture `tex` in quadrant `q`
/// (several layers of one texture: the most), `None` when no layer paints it there.
fn texture_grid(layers: &[Layer], q: u8, tex: Option<u32>) -> Option<Vec<f32>> {
    layers.iter().filter(|l| !l.base && l.quadrant == q && l.texture == tex).map(alpha_grid).reduce(|a, b| a.iter().zip(&b).map(|(x, y)| x.max(*y)).collect())
}

/// The texture layers a cell is drawn with: its own, stitched to the quadrants across
/// each quadrant edge, inside the cell and in the neighbouring cells (`neighbours`, by
/// side: [`NEIGHBOURS`]; `None` where there is no landscape or it is not read).
///
/// Each quadrant's layers come from its own record, so along a shared edge the two sides
/// can disagree and the ground changes texture along a straight line: at save 36 (cell
/// (−14, −17), issue #2089) the south cell paints LTEX 000357BA at up to 1.00 along its
/// north edge and the north cell has no layer of it [observed: `fnvtool world land
/// --full`; world.md §3 counts the disagreeing edges]. The rule [design; what the original
/// does at such edges is not known]: at every vertex of a shared edge a texture layer's
/// opacity is the larger of the two sides'; a quadrant with no layer of a texture its
/// neighbour paints on the edge gets one, drawn over its own layers in the neighbour's
/// order, painted on the edge only (it fades out across the next grid square). A texture
/// that is the quadrant's own base is not added (the base shows it), and bases are left
/// as they are. Both cells take the larger value from the same records, so they agree.
/// Corners shared only diagonally are not looked at.
pub fn stitch_layers(own: &[Layer], neighbours: &[Option<Vec<Layer>>; 4]) -> Vec<Layer> {
    let mut out: Vec<Layer> = own.to_vec();
    // Stitched grids of `out`'s non-base layers, by index; added layers after them.
    let mut grids: HashMap<usize, Vec<f32>> = HashMap::new();
    for q in 0..4u8 {
        let base_tex = own.iter().rev().find(|l| l.base && l.quadrant == q).and_then(|b| b.texture);
        let top = own.iter().filter(|l| l.quadrant == q).map(|l| l.layer).max().unwrap_or(-1);
        let mut added = 0i16;
        for side in [NORTH, EAST, SOUTH, WEST] {
            let (cell, q2) = across(q, side);
            let Some(theirs) = (match cell {
                None => Some(own),
                Some(c) => neighbours[c].as_deref(),
            }) else {
                continue;
            };
            let opposite = (side + 2) % 4;
            let mut their_layers: Vec<&Layer> = theirs.iter().filter(|l| !l.base && l.quadrant == q2).collect();
            their_layers.sort_by_key(|l| l.layer);
            for l in their_layers {
                let tex = l.texture;
                if tex == base_tex {
                    continue;
                }
                let their_grid = alpha_grid(l);
                let edge: Vec<(usize, f32)> = (0..QUAD_SIDE).map(|k| (edge_position(side, k), their_grid[edge_position(opposite, k)])).filter(|e| e.1 > 0.0).collect();
                if edge.is_empty() {
                    continue;
                }
                let i = match out.iter().position(|o| !o.base && o.quadrant == q && o.texture == tex) {
                    Some(i) => i,
                    None => {
                        added += 1;
                        out.push(Layer { quadrant: q, texture: tex, layer: top + added, base: false, alpha: Vec::new() });
                        out.len() - 1
                    }
                };
                let g = grids.entry(i).or_insert_with(|| texture_grid(own, q, tex).unwrap_or_else(|| vec![0.0; QUAD_SIDE * QUAD_SIDE]));
                for (p, a) in edge {
                    g[p] = g[p].max(a);
                }
            }
        }
    }
    for (i, g) in grids {
        out[i].alpha = g.iter().enumerate().filter(|(_, a)| **a > 0.0).map(|(p, a)| (p as u16, *a)).collect();
    }
    out
}

/// A quadrant's layers over its base, in drawing order, with their index in `layers`.
fn quadrant_layers(layers: &[Layer], q: u8) -> Vec<(usize, &Layer)> {
    let mut layers: Vec<_> = layers.iter().enumerate().filter(|(_, l)| !l.base && l.quadrant == q).collect();
    layers.sort_by_key(|(_, l)| l.layer);
    layers
}

/// A cell's landscape, read and its quadrant meshes built (on a worker thread when it is
/// read ahead, world.rs): the base mesh of each quadrant, `(quadrant, None)`, and the
/// blended mesh of each drawn layer, `(quadrant, Some(index in layers))`.
pub struct TerrainPrep {
    /// The cell's landscape as the data has it (grass and footsteps read its layers).
    pub land: Land,
    /// The layers drawn: the land's, stitched to the neighbours' ([`stitch_layers`]).
    layers: Vec<Layer>,
    meshes: HashMap<(u8, Option<usize>), Mesh>,
    /// Each quadrant's base with its first [`folded_layers`] layers folded in
    /// ([`folded_mesh`]); none for a quadrant without layers.
    folded: HashMap<u8, Mesh>,
    /// The texture repeat the meshes were built for.
    tile: f32,
}

/// Read LAND `land_id` of grid square `grid` and build its meshes (`tile`: world units per
/// texture repeat, [`TerrainSettings::tile`]); `neighbours`: the LAND of the grid squares
/// around it ([`NEIGHBOURS`]), whose texture layers its own are stitched to. `None`: no
/// landscape or no heights.
pub fn prepare_terrain(lo: &LoadOrder, schema: &EsmSchema, land_id: u32, grid: (i32, i32), tile: f32, neighbours: [Option<u32>; 4]) -> Option<TerrainPrep> {
    let land = load_land(lo, schema, land_id, LandParts::ALL)?;
    land.heights.as_ref()?;
    let parts = LandParts { heights: false, normals: false, colors: false, layers: true };
    let around = neighbours.map(|n| n.and_then(|id| load_land(lo, schema, id, parts)).map(|l| l.layers));
    let layers = stitch_layers(&land.layers, &around);
    let origin = Vec2::new(grid.0 as f32 * fnv_world::CELL_SIZE, grid.1 as f32 * fnv_world::CELL_SIZE);
    let mut meshes = HashMap::new();
    let mut folded = HashMap::new();
    for q in 0..4u8 {
        if let Some(mesh) = quadrant_mesh(&land, q, None, origin, tile) {
            meshes.insert((q, None), mesh);
        }
        let quad = quadrant_layers(&layers, q);
        let alphas: Vec<Vec<f32>> = quad.iter().map(|(_, l)| alpha_grid(l)).collect();
        for ((i, _), alpha) in quad.iter().zip(&alphas) {
            if let Some(mesh) = quadrant_mesh(&land, q, Some(alpha), origin, tile) {
                meshes.insert((q, Some(*i)), mesh);
            }
        }
        if !alphas.is_empty() {
            let first: Vec<&[f32]> = alphas.iter().take(folded_layers(tile)).map(Vec::as_slice).collect();
            if let Some(mesh) = folded_mesh(&land, q, &first, origin, tile) {
                folded.insert(q, mesh);
            }
        }
    }
    Some(TerrainPrep { land, layers, meshes, folded, tile })
}

/// A mesh of a cell's land (its base or a texture layer over it).
#[derive(Component)]
pub struct LandMesh;

/// Spawn a cell's terrain from its prepared landscape ([`prepare_terrain`]); returns the
/// landscape (for the grass).
pub fn spawn_terrain(commands: &mut Commands, cell: Entity, world: &WorldData, prep: TerrainPrep, tm: &mut TerrainCtx) -> Land {
    let TerrainPrep { land, layers: drawn, mut meshes, mut folded, tile } = prep;
    let folds = tm.tm.folds();
    let most = folded_layers(tile);
    // The repeat for the shader to work the texture coordinate out with, 0 when the mesh
    // carries the coordinate.
    let shader_tile = if most == FOLDED_LAYERS { tile } else { 0.0 };
    let mut overflow = 0usize;
    for q in 0..4u8 {
        let base = drawn.iter().rev().find(|l| l.base && l.quadrant == q);
        if base.is_none() {
            tm.tm.cache.quadrants_without_base += 1;
        }
        let tex = tm.texture_of(world, base.and_then(|b| b.texture));
        let layers = quadrant_layers(&drawn, q);
        // The first layers folded into the base's draw (module notes), when each of them
        // has a texture; otherwise the base alone and every layer blended over it.
        let mut blended_from = 0;
        let folded_mat = match folded.remove(&q).filter(|_| folds) {
            Some(mesh) => {
                let first: Option<Vec<String>> = layers.iter().take(most).map(|(_, l)| tm.texture_of(world, l.texture)).collect();
                first.and_then(|first: Vec<String>| Some((mesh, tm.folded_material(tex.clone(), &first, shader_tile)?, first.len())))
            }
            None => None,
        };
        if let Some((mesh, mat, n)) = folded_mat {
            let mesh = tm.assets.meshes.add(mesh);
            commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY, LandMesh));
            tm.tm.cache.folded_quadrants += 1;
            tm.tm.cache.folded_layers += n;
            blended_from = n;
        } else {
            let mat = tm.base_material(tex);
            if let Some(mesh) = meshes.remove(&(q, None)) {
                let mesh = tm.assets.meshes.add(mesh);
                commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY, LandMesh));
            }
        }
        for (rank, (i, l)) in layers.into_iter().enumerate().skip(blended_from) {
            let Some(tex) = tm.texture_of(world, l.texture) else { continue };
            let Some(mat) = tm.layer_material(tex, rank) else { continue };
            if let Some(mesh) = meshes.remove(&(q, Some(i))) {
                let mesh = tm.assets.meshes.add(mesh);
                commands.entity(cell).with_child((Mesh3d(mesh), MeshMaterial3d(mat), Transform::IDENTITY, LandMesh));
                tm.tm.cache.layers += 1;
                if blended_from > 0 {
                    overflow += 1;
                }
            }
        }
    }
    // The first cell with layers past a folded group says how many it drew blended over
    // them (once a run; the smoke check that such layer draws exist at all).
    if overflow > 0 {
        let first = tm.tm.cache.overflow_layers == 0;
        tm.tm.cache.overflow_layers += overflow;
        if first {
            info!("terrain: {overflow} layers drawn blended over folded quadrants (past the first {most} of a quadrant), in the first cell that has them");
        }
    }
    land
}

#[cfg(test)]
use crate::decals::bc5_fixture;

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::mesh::VertexAttributeValues;

    fn sloped_land() -> Land {
        // A slope rising to the east and, less, to the north.
        let heights = (0..SIDE * SIDE).map(|i| (i % SIDE) as f32 * 40.0 + (i / SIDE) as f32 * 15.0).collect();
        let n = Vec3::new(-40.0 / SPACING, -15.0 / SPACING, 1.0).normalize() * 127.0;
        Land { heights: Some(heights), normals: Some(vec![[n.x as i8, n.y as i8, n.z as i8]; SIDE * SIDE]), ..Default::default() }
    }

    #[test]
    fn texture_repeat_and_direction() {
        // Quadrant 3 (north-east) of cell (-17, 0): u and v grow with world x and y.
        let origin = Vec2::new(-17.0 * 4096.0, 0.0);
        let mesh = quadrant_mesh(&sloped_land(), 3, None, origin, 256.0).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = mesh.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        assert_eq!(uv[0], [(origin.x + 2048.0) / 256.0, 2048.0 / 256.0]);
        // One repeat every two grid squares, eastwards and northwards.
        assert_eq!(uv[2][0] - uv[0][0], 1.0);
        assert_eq!(uv[2 * QUAD_SIDE][1] - uv[0][1], 1.0);
    }

    #[test]
    fn tangents_match_the_generator() {
        let mesh = quadrant_mesh(&sloped_land(), 0, None, Vec2::ZERO, 256.0).unwrap();
        let Some(VertexAttributeValues::Float32x4(ours)) = mesh.attribute(Mesh::ATTRIBUTE_TANGENT).cloned() else { panic!() };
        let mut generated = mesh.clone();
        generated.generate_tangents().unwrap();
        let Some(VertexAttributeValues::Float32x4(theirs)) = generated.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        let Some(VertexAttributeValues::Float32x3(normals)) = mesh.attribute(Mesh::ATTRIBUTE_NORMAL) else { panic!() };
        for ((a, b), n) in ours.iter().zip(theirs).zip(normals) {
            let (ta, tb) = (Vec3::new(a[0], a[1], a[2]), Vec3::new(b[0], b[1], b[2]));
            assert!(ta.dot(tb) > 0.99, "{a:?} {b:?}");
            assert_eq!(a[3], b[3]);
            assert!(ta.dot(Vec3::from_array(*n)).abs() < 1e-3 && (ta.length() - 1.0).abs() < 1e-4);
        }
    }

    #[test]
    fn a_folded_mesh_carries_its_layers_opacities_and_keeps_the_base() {
        let land = sloped_land();
        let n = QUAD_SIDE * QUAD_SIDE;
        let v = 5 * QUAD_SIDE + 7;
        let layer = |o: f32| {
            let mut a = vec![0.0; n];
            a[v] = o;
            a
        };
        let alphas = [layer(0.2), layer(0.4), layer(0.6), layer(0.8)];
        let refs: Vec<&[f32]> = alphas.iter().map(Vec::as_slice).collect();
        // A repeat that does not divide the cell: four layers, the mesh keeps its texture
        // coordinate.
        assert_eq!((folded_layers(256.0), folded_layers(300.0), folded_layers(0.0)), (FOLDED_LAYERS, FOLDED_LAYERS_WITH_UV, FOLDED_LAYERS_WITH_UV));
        let base = quadrant_mesh(&land, 0, None, Vec2::ZERO, 300.0).unwrap();
        let folded = folded_mesh(&land, 0, &refs, Vec2::ZERO, 300.0).unwrap();
        // The base's own geometry: every triangle, the same positions, normals and texture
        // coordinates, so the depth prepass and the shadows see the same ground.
        assert_eq!(triangles(&folded), triangles(&base));
        for a in [Mesh::ATTRIBUTE_POSITION, Mesh::ATTRIBUTE_NORMAL, Mesh::ATTRIBUTE_UV_0] {
            assert_eq!(folded.attribute(a).unwrap().get_bytes(), base.attribute(a).unwrap().get_bytes());
        }
        let Some(VertexAttributeValues::Float32x4(col)) = folded.attribute(Mesh::ATTRIBUTE_COLOR) else { panic!() };
        let Some(VertexAttributeValues::Float32x2(uv_b)) = folded.attribute(Mesh::ATTRIBUTE_UV_1) else { panic!() };
        let Some(VertexAttributeValues::Float32x4(tan)) = folded.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        let Some(VertexAttributeValues::Float32x4(base_tan)) = base.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        // As terrain.wesl reads them back.
        let read = |i: usize| [col[i][3], uv_b[i][0], uv_b[i][1], tan[i][3].abs() - 1.0];
        let close = |a: [f32; 4], b: [f32; 4]| a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-6);
        assert!(close(read(v), [0.2, 0.4, 0.6, 0.8]), "{:?}", read(v));
        assert!(close(read(0), [0.0; 4]), "{:?}", read(0));
        // The tangent's direction and handedness are the base's.
        for (t, b) in tan.iter().zip(base_tan) {
            assert_eq!(t[..3], b[..3]);
            assert_eq!(t[3].signum(), b[3].signum());
        }
        // The game's repeat: six layers, the fifth and sixth in the first texture coordinate,
        // which is the vertex's place in the cell over the repeat but for whole repeats (the
        // cell's origin is a whole number of them away), so the shader can work it out.
        let six = [layer(0.1), layer(0.2), layer(0.3), layer(0.4), layer(0.5), layer(0.6)];
        let six_refs: Vec<&[f32]> = six.iter().map(Vec::as_slice).collect();
        let origin = Vec2::new(-17.0 * 4096.0, 3.0 * 4096.0);
        let all = folded_mesh(&land, 0, &six_refs, origin, 256.0).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv)) = all.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        assert!(close([uv[v][0], uv[v][1], 0.0, 0.0], [0.5, 0.6, 0.0, 0.0]) && uv[0] == [0.0, 0.0]);
        let with_uv = quadrant_mesh(&land, 0, None, origin, 256.0).unwrap();
        let Some(VertexAttributeValues::Float32x2(real)) = with_uv.attribute(Mesh::ATTRIBUTE_UV_0) else { panic!() };
        let Some(VertexAttributeValues::Float32x3(pos)) = with_uv.attribute(Mesh::ATTRIBUTE_POSITION) else { panic!() };
        for (uv, p) in real.iter().zip(pos) {
            let worked_out = [p[0] / 256.0, p[1] / 256.0];
            assert_eq!(((uv[0] - worked_out[0]).fract(), (uv[1] - worked_out[1]).fract()), (0.0, 0.0), "{uv:?} {p:?}");
        }
        // Fewer layers: the others' opacities are 0.
        let two = folded_mesh(&land, 0, &refs[..2], Vec2::ZERO, 300.0).unwrap();
        let Some(VertexAttributeValues::Float32x2(uv_b)) = two.attribute(Mesh::ATTRIBUTE_UV_1) else { panic!() };
        let Some(VertexAttributeValues::Float32x4(tan)) = two.attribute(Mesh::ATTRIBUTE_TANGENT) else { panic!() };
        assert_eq!((uv_b[v], tan[v][3]), ([0.4, 0.0], TANGENT_SIGN));
    }

    #[test]
    fn the_shader_is_told_how_many_layers_and_which_have_normal_maps() {
        let image = Handle::<Image>::default();
        let layers = TerrainLayers { count: 3, diffuse0: Some(image.clone()), normal0: Some(image.clone()), diffuse1: Some(image.clone()), diffuse2: Some(image.clone()), normal2: Some(image.clone()), ..default() };
        let uniform = TerrainLayersUniform::from(&layers);
        assert_eq!((uniform.counts, uniform.tile.x), (UVec4::new(3, 0b101, 0, 0), 0.0));
        // No more layers than the mesh can carry: four with its own texture coordinate, six
        // when the shader works it out.
        assert_eq!(TerrainLayersUniform::from(&TerrainLayers { count: 9, ..default() }).counts.x, FOLDED_LAYERS_WITH_UV as u32);
        let six = TerrainLayersUniform::from(&TerrainLayers { count: 9, tile: 256.0, normal5: Some(image), ..default() });
        assert_eq!((six.counts, six.tile.x), (UVec4::new(FOLDED_LAYERS as u32, 1 << 5, 0, 0), 256.0));
    }

    // CPU mirror of terrain_weights.wesl, line for line (its constants are checked against
    // the file below).
    const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
    // A fixture gain for the height function's own tests (the default is 8, tested above).
    const HEIGHT_GAIN: f32 = 4.0;
    const HEIGHT_LIMIT: f32 = 0.25;
    const DEPTH: f32 = 0.10;

    fn luma(c: [f32; 3]) -> f32 {
        c[0] * LUMA[0] + c[1] * LUMA[1] + c[2] * LUMA[2]
    }

    fn terrain_height(texel: [f32; 3], reference: [f32; 3], gain: f32) -> f32 {
        (gain * (luma(texel) - luma(reference))).clamp(-HEIGHT_LIMIT, HEIGHT_LIMIT)
    }

    fn ordered_shares(a: [f32; 6], n: usize) -> [f32; 7] {
        let mut w = [0.0; 7];
        let mut remaining = 1.0;
        for i in (0..n.min(6)).rev() {
            let s = a[i].clamp(0.0, 1.0);
            w[i + 1] = s * remaining;
            remaining *= 1.0 - s;
        }
        w[0] = remaining;
        w
    }

    fn height_weights(w: [f32; 7], h: [f32; 7], depth: f32) -> [f32; 7] {
        let mut p = w.map(|x| x.max(0.0));
        let total: f32 = p.iter().sum();
        if total <= 0.0 {
            return [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0];
        }
        let mut score = [0.0f32; 7];
        let mut highest = 0.0f32;
        for i in 0..7 {
            p[i] /= total;
            score[i] = p[i] + h[i].clamp(-p[i], p[i]);
            highest = highest.max(score[i]);
        }
        let d = depth.max(MIN_DEPTH);
        let q: [f32; 7] = std::array::from_fn(|i| p[i] * (d - (highest - score[i])).max(0.0));
        let sum: f32 = q.iter().sum();
        q.map(|x| x / sum)
    }

    fn close7(a: [f32; 7], b: [f32; 7], tol: f32) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tol)
    }

    #[test]
    fn the_weights_shader_is_valid_wgsl_and_matches_the_mirror() {
        use wgpu::naga;
        let source = include_str!("terrain_weights.wesl");
        let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
        for line in [
            "const TERRAIN_LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);",
            "const TERRAIN_HEIGHT_LIMIT: f32 = 0.25;",
        ] {
            assert!(source.contains(line), "terrain_weights.wesl must declare `{line}` (the tests' mirror)");
        }
        // terrain.wesl takes those functions by name, only with the switch on.
        let terrain = include_str!("terrain.wesl");
        assert!(terrain.contains("@if(TERRAIN_HEIGHT_BLEND) import nvforever::terrain_weights::{terrain_height, terrain_ordered_shares, terrain_height_weights};"));
        for f in ["fn terrain_height(", "fn terrain_ordered_shares(", "fn terrain_height_weights("] {
            assert!(source.contains(f), "{f}");
        }
    }

    #[test]
    fn height_reference_sampling_is_valid_wgsl() {
        use wgpu::naga;
        // Validate the actual shared reference sampler, including its texture/sampler
        // arguments and explicit LOD. Runtime smoke covers the active binding form.
        // Line endings as checked out (CRLF on Windows) normalised before cutting the function out.
        let terrain = include_str!("terrain.wesl").replace("\r\n", "\n");
        // From the plain-value helper through the non-bindless reader that calls it; the WESL
        // conditions in front of each `fn` are left out of the cut.
        let start = terrain.find("fn height_reference_at(").unwrap();
        let reader = terrain.find("fn height_reference(").unwrap();
        let end = reader + terrain[reader..].find("\n}\n").unwrap() + 2;
        let source = format!("struct View {{ mip_bias: f32 }} @group(0) @binding(0) var<uniform> view: View;\n{}", terrain[start..end].replace("@if(TERRAIN_HEIGHT_BLEND) ", ""));
        let module = naga::front::wgsl::parse_str(&source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&source)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
    }

    #[test]
    fn ordered_shares_give_what_the_mix_chain_gives() {
        // Synthetic colours and opacities (no texture's values), the chain of terrain.wesl's
        // ordinary path against the weighted sum, within rounding.
        let mut seed = 0x2550u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        for case in 0..400 {
            let n = case % 7;
            let mut a = [0.0f32; 6];
            for x in a.iter_mut().take(n) {
                // Zeros, ones, tiny shares and the rest.
                *x = match (next() * 5.0) as u32 {
                    0 => 0.0,
                    1 => 1.0,
                    2 => 1e-4,
                    _ => next(),
                };
            }
            let colours: Vec<[f32; 3]> = (0..7).map(|_| [next(), next(), next()]).collect();
            let mut chain = colours[0];
            for (k, layer) in colours[1..=n].iter().enumerate() {
                for (x, l) in chain.iter_mut().zip(layer) {
                    *x = *x * (1.0 - a[k]) + l * a[k];
                }
            }
            let w = ordered_shares(a, n);
            assert!((w.iter().sum::<f32>() - 1.0).abs() < 1e-5, "{w:?}");
            assert!(w.iter().all(|x| *x >= 0.0) && w[n + 1..].iter().all(|x| *x == 0.0), "{w:?}");
            for (c, want) in chain.iter().enumerate() {
                let sum: f32 = w.iter().zip(&colours).map(|(w, colour)| w * colour[c]).sum();
                assert!((sum - want).abs() < 1e-5, "case {case}: {sum} vs {want}");
            }
            // A sufficiently large depth converges to the ordered shares, even though
            // equal heights at the starting depth now sharpen unequal shares.
            assert!(close7(height_weights(w, [0.1; 7], 1e6), w, 2e-6));
        }
    }

    #[test]
    fn height_weights_keep_what_the_data_paints() {
        let heights = [0.2, -0.25, 0.25, 0.0, -0.1, 0.15, 0.05];
        for (paint, expected) in [
            ([0.0; 6], [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]),
            ([1.0; 6], [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0]),
            ([0.5, 0.7, 1.0, 0.0, 0.0, 0.0], [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]),
        ] {
            assert_eq!(height_weights(ordered_shares(paint, 6), heights, DEPTH), expected);
        }
        let w = ordered_shares([0.4, 0.0, 1e-4, 0.6, 0.0, 0.3], 6);
        let f = height_weights(w, heights, DEPTH);
        for (fi, wi) in f.iter().zip(w) {
            assert!(*fi >= 0.0 && fi.is_finite(), "{f:?}");
            if wi == 0.0 {
                assert_eq!(*fi, 0.0);
            }
        }
        assert!((f.iter().sum::<f32>() - 1.0).abs() < 1e-5);
        assert_eq!(height_weights([0.0; 7], heights, DEPTH), [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        let half = [0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0];
        let f = height_weights(half, [-0.05, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0], DEPTH);
        assert!(close7(f, [0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0], 1e-6), "{f:?}");
        assert!(close7(height_weights(half, [0.0; 7], DEPTH), half, 1e-6));
        let unequal = [0.6, 0.4, 0.0, 0.0, 0.0, 0.0, 0.0];
        assert_eq!(height_weights(unequal, [0.0; 7], DEPTH), [1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]);
        // Inactive heights have no vote, even at their extremes.
        assert_eq!(height_weights(half, [0.0, 0.0, 0.25, -0.25, 0.25, -0.25, 0.25], DEPTH), height_weights(half, [0.0; 7], DEPTH));
    }

    #[test]
    fn depth_weights_are_continuous_at_share_and_height_boundaries() {
        let epsilon = 1e-6;
        // Appearance/disappearance, equal shares, a score crossing the depth cutoff,
        // and a score crossing the share cap. No equal-height gate or special cases.
        for h in [[0.0; 7], [-0.05, 0.05, 0.0, 0.0, 0.0, 0.0, 0.0], [0.25, -0.25, 0.0, 0.0, 0.0, 0.0, 0.0]] {
            for a in [0.0f32, 1e-4, 0.25, 0.45, 0.5, 0.55, 0.75, 1.0] {
                let weights = |a: f32| height_weights(ordered_shares([a, 0.0, 0.0, 0.0, 0.0, 0.0], 1), h, DEPTH);
                assert!(close7(weights((a - epsilon).max(0.0)), weights((a + epsilon).min(1.0)), 1e-4), "a={a} h={h:?}");
            }
        }
        let half = [0.5, 0.5, 0.0, 0.0, 0.0, 0.0, 0.0];
        for h in [-0.1, 0.0, 0.1] {
            let weights = |h| height_weights(half, [0.0, h, 0.0, 0.0, 0.0, 0.0, 0.0], DEPTH);
            assert!(close7(weights(h - epsilon), weights(h + epsilon), 1e-4));
        }
    }

    #[test]
    fn large_depth_converges_to_ordered_shares() {
        let w = ordered_shares([0.4, 0.0, 1e-4, 0.6, 0.0, 0.3], 6);
        let h = [0.2, -0.25, 0.25, 0.0, -0.1, 0.15, 0.05];
        let error = |d| height_weights(w, h, d).iter().zip(w).map(|(a, b)| (a - b).abs()).sum::<f32>();
        assert!(error(100.0) < error(1.0));
        assert!(error(1e6) < 2e-6);
    }

    #[test]
    fn height_blend_parameters_parse_without_changing_the_environment() {
        assert_eq!(HeightBlendParams::default().gain, 8.0);
        assert_eq!(HeightBlendParams::default().depth, DEPTH);
        assert_eq!(parse_height_blend("0.10:8"), Ok(HeightBlendParams::default()));
        assert_eq!(parse_height_blend("0.10:8:global"), Ok(HeightBlendParams::default()));
        assert_ne!(parse_height_blend("0.10:4"), Ok(HeightBlendParams::default()));
        let local = parse_height_blend("0.05:8:local").unwrap();
        assert_eq!(local.uniform(), Vec4::new(0.05, 8.0, 1.0, 0.0));
        assert_eq!(parse_height_blend("1:0").unwrap().uniform(), Vec4::new(1.0, 0.0, 0.0, 0.0));
        assert_eq!(parse_height_blend("0.001:4").unwrap().depth, MIN_DEPTH);
        for invalid in ["", "0.1", "0.1:4:", "0.1:4:GLOBAL", "0.1:4:local:extra", "0:4", "0.0009:4", "0.00000001:8:local", "-1:4", "NaN:4", "inf:4", "0.1:NaN", "0.1:inf", "0.1:-1", ":4", "0.1:", "abc:4"] {
            assert!(parse_height_blend(invalid).is_err(), "{invalid}");
        }
        // The additional uniform does not enter TerrainLayersKey or enable the effect.
        assert_eq!(TerrainLayersUniform::min_size().get(), 48);
        let off = TerrainLayers::default();
        assert!(!TerrainLayersKey::from(&off).height_blend);
        assert_eq!(TerrainLayersUniform::from(&off).height, height_blend_params().uniform());
    }

    #[test]
    fn a_texel_height_is_its_brightness_over_its_textures_mean() {
        assert_eq!(terrain_height([0.5; 3], [0.5; 3], HEIGHT_GAIN), 0.0);
        assert!((terrain_height([0.55; 3], [0.5; 3], HEIGHT_GAIN) - 0.2).abs() < 1e-6);
        assert_eq!(terrain_height([0.6; 3], [0.5; 3], 0.0), 0.0);
        assert!((terrain_height([0.55; 3], [0.5; 3], 1.0) - 0.05).abs() < 1e-6);
        assert_eq!(terrain_height([0.55; 3], [0.5; 3], 8.0), HEIGHT_LIMIT);
        // A bright texture does not stand higher for being bright: only the texel against
        // its own texture's mean counts.
        assert_eq!(terrain_height([0.9; 3], [0.9; 3], HEIGHT_GAIN), terrain_height([0.1; 3], [0.1; 3], HEIGHT_GAIN));
        assert_eq!(terrain_height([1.0; 3], [0.0; 3], HEIGHT_GAIN), HEIGHT_LIMIT);
        assert_eq!(terrain_height([0.0; 3], [1.0; 3], HEIGHT_GAIN), -HEIGHT_LIMIT);
        // Green counts most, blue least.
        assert!(terrain_height([0.0, 0.1, 0.0], [0.0; 3], HEIGHT_GAIN) > terrain_height([0.1, 0.0, 0.0], [0.0; 3], HEIGHT_GAIN));
    }

    #[test]
    fn height_blending_selects_the_pipeline_and_the_feature_bit() {
        let off = TerrainLayers { count: 2, ..default() };
        let on = TerrainLayers { count: 2, height_blend: true, ..default() };
        assert_eq!((TerrainLayersKey::from(&off), TerrainLayersKey::from(&on)), (TerrainLayersKey { height_blend: false, anti_tiling: false }, TerrainLayersKey { height_blend: true, anti_tiling: false }));
        assert_eq!((TerrainLayersUniform::from(&off).counts.w, TerrainLayersUniform::from(&on).counts.w), (0, FEATURE_HEIGHT_BLEND));
        assert_eq!(TerrainLayersUniform::from(&on).counts.truncate(), TerrainLayersUniform::from(&off).counts.truncate());
        let main = || RenderPipelineDescriptor { fragment: Some(bevy::render::render_resource::FragmentState { shader_defs: vec!["VERTEX_UVS_A".into()], ..default() }), ..default() };
        let has = |d: &RenderPipelineDescriptor| d.fragment.as_ref().unwrap().shader_defs.contains(&"TERRAIN_HEIGHT_BLEND".into());
        let mut d = main();
        height_blend_def(&mut d, TerrainLayersKey { height_blend: true, anti_tiling: false });
        assert!(has(&d));
        let mut d = main();
        height_blend_def(&mut d, TerrainLayersKey { height_blend: false, anti_tiling: false });
        assert!(!has(&d), "off: the pipeline there always was");
        let mut prepass = main();
        prepass.vertex.shader_defs.push("PREPASS_PIPELINE".into());
        height_blend_def(&mut prepass, TerrainLayersKey { height_blend: true, anti_tiling: false });
        assert!(!has(&prepass), "the prepass draws the same depth either way");
    }

    #[test]
    fn the_switch_flips_every_folded_material_in_place() {
        use crate::graphics::{Graphics, Preset};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(Preset::Vanilla));
        app.insert_resource(Assets::<TerrainMaterial>::default());
        app.add_systems(Update, sync_height_blend);
        let material = || TerrainMaterial { base: StandardMaterial::default(), extension: TerrainLayers { count: 1, ..default() } };
        let a = app.world_mut().resource_mut::<Assets<TerrainMaterial>>().add(material());
        app.update();
        let blends = |app: &App, h: &Handle<TerrainMaterial>| app.world().resource::<Assets<TerrainMaterial>>().get(h).unwrap().extension.height_blend;
        assert!(!blends(&app, &a), "off by default");
        assert!(!Graphics::preset(Preset::Enhanced).terrain_height_blend, "and in the enhanced preset");
        app.world_mut().resource_mut::<Graphics>().terrain_height_blend = true;
        let b = app.world_mut().resource_mut::<Assets<TerrainMaterial>>().add(material());
        app.update();
        assert!(blends(&app, &a) && blends(&app, &b));
        app.world_mut().resource_mut::<Graphics>().terrain_height_blend = false;
        app.update();
        assert!(!blends(&app, &a) && !blends(&app, &b));
        // Without the settings resource (a tool's world): off.
        assert!(!height_blend_wanted(None));
    }

    #[test]
    fn the_sampling_shader_is_valid_wgsl_and_reads_inside_branches() {
        use wgpu::naga;
        let validate = |source: &str| {
            let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
            naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty()).validate(&module).unwrap_or_else(|e| panic!("{e:?}"));
        };
        let helper = nvf_assets::terrain_sampling::SAMPLING_SOURCE;
        validate(helper);
        // A fragment that reads a texture through the helpers inside a branch that differs
        // between the fragments of a quad (explicit gradients allow it), with derivatives
        // taken before it, as terrain.wesl and terrain_pbr.wesl do.
        let harness = format!(
            "{helper}\n\
             @group(0) @binding(0) var tex: texture_2d<f32>;\n\
             @group(0) @binding(1) var samp: sampler;\n\
             @fragment fn harness(@location(0) world: vec3<f32>, @location(1) o: f32) -> @location(0) vec4<f32> {{\n\
                 let uv = terrain_world_uv(world, 256.0);\n\
                 let fp = TerrainFootprint(uv, dpdx(uv) * 2.0, dpdy(uv) * 2.0);\n\
                 var c = vec4<f32>(0.0);\n\
                 if o > 0.5 {{\n\
                     c = terrain_sample_diffuse(tex, samp, terrain_hex3(fp.uv), fp);\n\
                     let n = terrain_sample_normal(tex, samp, terrain_hex3(fp.uv), fp, o > 0.75, true);\n\
                     let tbn = mat3x3<f32>(vec3<f32>(1.0, 0.0, 0.0), vec3<f32>(0.0, 1.0, 0.0), vec3<f32>(0.0, 0.0, 1.0));\n\
                     c += vec4<f32>(terrain_world_normal(tbn, n, o > 0.9), 0.0);\n\
                 }}\n\
                 return c;\n\
             }}\n"
        );
        validate(&harness);
        // terrain.wesl takes the helpers and the surface set-up by name, only with the switch.
        let terrain = include_str!("terrain.wesl");
        assert!(terrain.contains("@if(TERRAIN_ANTI_TILING) import nvf_assets::terrain_sampling::{"));
        assert!(terrain.contains("@if(TERRAIN_ANTI_TILING) import nvf_assets::terrain_pbr::{"));
    }

    #[test]
    fn with_anti_tiling_off_both_old_paths_stay_as_they_were() {
        // The two #2550 paths are fenced off from the anti-tiled one, not edited: their
        // conditions gain `!TERRAIN_ANTI_TILING`, and Bevy's set-up is kept for them.
        let terrain = include_str!("terrain.wesl");
        assert!(terrain.contains("        @if(!TERRAIN_HEIGHT_BLEND && !TERRAIN_ANTI_TILING) {"));
        assert!(terrain.contains("        @if(TERRAIN_HEIGHT_BLEND && !TERRAIN_ANTI_TILING) {"));
        assert!(terrain.contains("    @if(!TERRAIN_ANTI_TILING) var pbr_input = pbr_input_from_standard_material(in, is_front);"));
        assert!(terrain.contains("    @if(TERRAIN_ANTI_TILING) var pbr_input = terrain_pbr_begin(in, is_front, standard_material(in.instance_index));"));
        // The anti-tiled walk is the ordered blend's (mirrored in `antitiled_walk` below)
        // and the heights come from its reads.
        for line in [
            "                if remaining == 0.0 {",
            "                if o == 0.0 {",
            "                let a = saturate(o * raw[k + 1u].a);",
            "                w[k + 1u] = a * remaining;",
            "                remaining *= 1.0 - a;",
            "            if w[0] != 0.0 {",
            "                    height[0] = terrain_height(raw[0].rgb, base_reference(in.instance_index, vec2(0.5), vec2(0.0), vec2(0.0), 0.0), anti_params.y);",
            "                        height[k + 1u] = terrain_height(raw[k + 1u].rgb, layer_reference(k, in.instance_index, vec2(0.5), vec2(0.0), vec2(0.0), 0.0), anti_params.y);",
        ] {
            assert!(terrain.contains(line), "terrain.wesl: `{line}`");
        }
    }

    /// CPU mirror of terrain.wesl's anti-tiled walk over the layers: the ordered shares
    /// (base at 0, layer k at k + 1) and which diffuse textures were read. `alpha` is the
    /// texture alpha a read would give.
    fn antitiled_walk(opacity: [f32; 6], alpha: [f32; 7], n: usize) -> ([f32; 7], [bool; 7]) {
        let mut w = [0.0f32; 7];
        let mut read = [false; 7];
        let mut remaining = 1.0f32;
        for k in (0..n.min(6)).rev() {
            if remaining == 0.0 {
                break;
            }
            let o = opacity[k].clamp(0.0, 1.0);
            if o == 0.0 {
                continue;
            }
            read[k + 1] = true;
            let a = (o * alpha[k + 1]).clamp(0.0, 1.0);
            w[k + 1] = a * remaining;
            remaining *= 1.0 - a;
        }
        w[0] = remaining;
        read[0] = w[0] != 0.0;
        (w, read)
    }

    #[test]
    fn the_antitiled_walk_gives_the_ordered_shares_and_reads_only_what_can_show() {
        // Synthetic opacities and alphas (no texture's values): the same shares as the
        // ordered blend, exactly.
        let mut seed = 0x2551u32;
        let mut next = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1u32 << 24) as f32
        };
        for case in 0..600 {
            let n = case % 7;
            let mut opacity = [0.0f32; 6];
            let mut alpha = [1.0f32; 7];
            for k in 0..6 {
                opacity[k] = match (next() * 5.0) as u32 {
                    0 => 0.0,
                    1 => 1.0,
                    2 => 1e-4,
                    _ => next(),
                };
                alpha[k + 1] = match (next() * 4.0) as u32 {
                    0 => 0.0,
                    1 => 1.0,
                    _ => next(),
                };
            }
            let paint: [f32; 6] = std::array::from_fn(|k| opacity[k].clamp(0.0, 1.0) * alpha[k + 1]);
            let (w, read) = antitiled_walk(opacity, alpha, n);
            assert_eq!(w, ordered_shares(paint, n), "case {case}");
            for k in 0..n {
                // Opacity 0: never read. A share above 0: always read.
                if opacity[k] == 0.0 {
                    assert!(!read[k + 1], "case {case}: layer {k}");
                }
                if w[k + 1] != 0.0 {
                    assert!(read[k + 1], "case {case}: layer {k}");
                }
            }
            assert_eq!(read[0], w[0] != 0.0);
            // With height blending, a texture not read has a share of 0 and keeps it.
            for depth in [MIN_DEPTH, DEPTH, 0.5, 1e6] {
                let f = height_weights(w, [0.2, -0.25, 0.25, 0.0, -0.1, 0.15, 0.05], depth);
                assert!(f.iter().all(|x| x.is_finite() && *x >= 0.0));
                assert!((f.iter().sum::<f32>() - 1.0).abs() < 1e-5);
                for i in 0..7 {
                    assert!(read[i] || f[i] == 0.0, "case {case}, depth {depth}: {i} unread but weighted");
                }
            }
        }
        // A fully covering later layer: everything under it (the base too) is not read.
        let (w, read) = antitiled_walk([0.6, 0.3, 1.0, 0.0, 0.0, 0.0], [1.0; 7], 3);
        assert_eq!(w, [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        assert_eq!(read, [false, false, false, true, false, false, false]);
        // An alpha of 0 is only found by reading: the layer is read and shares nothing.
        let mut alpha = [1.0; 7];
        alpha[2] = 0.0;
        let (w, read) = antitiled_walk([0.0, 1.0, 0.0, 0.0, 0.0, 0.0], alpha, 2);
        assert!(read[2] && w[2] == 0.0 && read[0] && w[0] == 1.0);
        // A tiny painted share survives (exact zeros skip, no epsilon).
        let (w, read) = antitiled_walk([1e-4, 0.0, 0.0, 0.0, 0.0, 0.0], [1.0; 7], 1);
        assert!(read[1] && w[1] > 0.0 && w[1] < 1e-3);
        // Unpainted ground reads the base alone.
        assert_eq!(antitiled_walk([0.0; 6], [1.0; 7], 6).1, [true, false, false, false, false, false, false]);
    }

    #[test]
    fn anti_tiling_selects_the_pipeline_the_feature_bit_and_the_repeat() {
        let off = TerrainLayers { count: 2, texture_repeat: 256.0, ..default() };
        let on = TerrainLayers { count: 2, texture_repeat: 256.0, anti_tiling: true, ..default() };
        assert_eq!(TerrainLayersKey::from(&off), TerrainLayersKey { height_blend: false, anti_tiling: false });
        assert_eq!(TerrainLayersKey::from(&on), TerrainLayersKey { height_blend: false, anti_tiling: true });
        assert_eq!((TerrainLayersUniform::from(&off).counts.w, TerrainLayersUniform::from(&on).counts.w), (0, FEATURE_ANTI_TILING));
        let both = TerrainLayers { height_blend: true, ..on.clone() };
        assert_eq!(TerrainLayersUniform::from(&both).counts.w, FEATURE_HEIGHT_BLEND | FEATURE_ANTI_TILING);
        assert_eq!(TerrainLayersKey::from(&both), TerrainLayersKey { height_blend: true, anti_tiling: true });
        // The repeat travels in tile.y, also where the mesh carries the coordinate (tile 0),
        // and the folding (tile.x, the layer count) is as before.
        let carried = TerrainLayersUniform::from(&TerrainLayers { count: 9, texture_repeat: 300.0, anti_tiling: true, ..default() });
        assert_eq!((carried.tile.x, carried.tile.y, carried.counts.x), (0.0, 300.0, FOLDED_LAYERS_WITH_UV as u32));
        let worked_out = TerrainLayersUniform::from(&TerrainLayers { count: 9, tile: 256.0, texture_repeat: 256.0, ..default() });
        assert_eq!((worked_out.tile.x, worked_out.tile.y, worked_out.counts.x), (256.0, 256.0, FOLDED_LAYERS as u32));
        // A repeat that cannot place the reads keeps the ordinary pipeline.
        for bad in [0.0, -256.0, f32::NAN] {
            let l = TerrainLayers { count: 2, texture_repeat: bad, anti_tiling: true, ..default() };
            assert!(!TerrainLayersKey::from(&l).anti_tiling, "{bad}");
            assert_eq!((TerrainLayersUniform::from(&l).counts.w, TerrainLayersUniform::from(&l).tile.y), (0, 0.0), "{bad}");
        }
        // The definitions go to the main pass only (nvf_assets terrain_sampling's test has
        // the rest); height blending's are untouched.
        let main = || RenderPipelineDescriptor { fragment: Some(bevy::render::render_resource::FragmentState { shader_defs: vec!["VERTEX_UVS_A".into()], ..default() }), ..default() };
        let mut d = main();
        height_blend_def(&mut d, TerrainLayersKey { height_blend: false, anti_tiling: true });
        assert_eq!(d.fragment.as_ref().unwrap().shader_defs.len(), 1, "height blending's switch alone decides TERRAIN_HEIGHT_BLEND");
    }

    #[test]
    fn the_anti_tiling_switch_flips_every_folded_material_in_place() {
        use crate::graphics::{Graphics, Preset};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(Preset::Vanilla));
        app.insert_resource(TerrainSettings { tile: 256.0 });
        app.insert_resource(Assets::<TerrainMaterial>::default());
        app.add_systems(Update, (sync_height_blend, sync_anti_tiling));
        let material = || TerrainMaterial { base: StandardMaterial::default(), extension: TerrainLayers { count: 1, texture_repeat: 256.0, ..default() } };
        let a = app.world_mut().resource_mut::<Assets<TerrainMaterial>>().add(material());
        app.update();
        let get = |app: &App, h: &Handle<TerrainMaterial>| {
            let e = &app.world().resource::<Assets<TerrainMaterial>>().get(h).unwrap().extension;
            (e.anti_tiling, e.height_blend)
        };
        assert_eq!(get(&app, &a), (false, false), "off by default");
        assert!(!Graphics::preset(Preset::Enhanced).terrain_anti_tiling, "and in the enhanced preset");
        app.world_mut().resource_mut::<Graphics>().terrain_anti_tiling = true;
        let b = app.world_mut().resource_mut::<Assets<TerrainMaterial>>().add(material());
        app.update();
        assert_eq!((get(&app, &a), get(&app, &b)), ((true, false), (true, false)), "height blending left alone");
        app.world_mut().resource_mut::<Graphics>().terrain_height_blend = true;
        app.update();
        assert_eq!(get(&app, &a), (true, true), "the two switches together");
        app.world_mut().resource_mut::<Graphics>().terrain_anti_tiling = false;
        app.update();
        assert_eq!((get(&app, &a), get(&app, &b)), ((false, true), (false, true)));
        assert!(!anti_tiling_wanted(None));
    }

    #[test]
    fn the_switch_reaches_the_landscapes_own_nif_materials_and_no_other() {
        use crate::graphics::{Graphics, Preset};
        let mut app = App::new();
        app.insert_resource(Graphics::preset(Preset::Vanilla));
        app.insert_resource(TerrainSettings { tile: 300.0 });
        app.insert_resource(Assets::<TerrainMaterial>::default());
        app.insert_resource(Assets::<NifMaterial>::default());
        app.add_systems(Update, sync_anti_tiling);
        let surface = || surface_material(None, None);
        let (base, layer, other) = {
            let mut nif = app.world_mut().resource_mut::<Assets<NifMaterial>>();
            let base = nif.add(terrain_nif_material(surface(), 300.0, false));
            let layer = nif.add(terrain_nif_material(StandardMaterial { alpha_mode: AlphaMode::Blend, depth_bias: sort_first(4.0), ..surface() }, 300.0, false));
            (base, layer, nif.add(nif_material(StandardMaterial::default())))
        };
        let on = |app: &App, h: &Handle<NifMaterial>| app.world().resource::<Assets<NifMaterial>>().get(h).unwrap().extension.anti_tiling;
        app.update();
        assert!(!on(&app, &base) && !on(&app, &layer) && !on(&app, &other));
        app.world_mut().resource_mut::<Graphics>().terrain_anti_tiling = true;
        app.update();
        assert!(on(&app, &base) && on(&app, &layer), "the landscape's materials flipped in place");
        assert!(!on(&app, &other), "a shape's material is left alone (no texture repeat)");
        // The layer keeps its blending and draw order; the key selects the separate path.
        let nif = app.world().resource::<Assets<NifMaterial>>();
        let l = nif.get(&layer).unwrap();
        assert!(matches!(l.base.alpha_mode, AlphaMode::Blend) && l.base.depth_bias == sort_first(4.0));
        assert!(nvf_assets::nif_material::NifSurfaceKey::from(&l.extension).anti_tiling);
        assert!(!nvf_assets::nif_material::NifSurfaceKey::from(&nif.get(&other).unwrap().extension).anti_tiling);
        assert_eq!(terrain_nif_material(surface(), 256.0, true).extension.terrain_repeat, 256.0);
        app.world_mut().resource_mut::<Graphics>().terrain_anti_tiling = false;
        app.update();
        assert!(!on(&app, &base) && !on(&app, &layer));
    }

    #[test]
    fn the_landscape_materials_hold_only_what_the_anti_tiled_set_up_reads() {
        // nvf_assets terrain_pbr.wesl's contract: colour, diffuse texture, normal map and
        // scalar values; nothing it would leave out.
        let image = Handle::<Image>::default();
        for m in [surface_material(Some(image.clone()), Some(image.clone())), surface_material(None, None)] {
            assert!(m.emissive_texture.is_none() && m.metallic_roughness_texture.is_none() && m.occlusion_texture.is_none() && m.depth_map.is_none());
            // (The transmission, clearcoat and anisotropy textures are not built into this
            // Bevy; their strengths are checked below.)
            assert!(m.specular_texture.is_none() && m.specular_tint_texture.is_none());
            assert_eq!((m.specular_transmission, m.diffuse_transmission, m.thickness, m.clearcoat, m.anisotropy_strength), (0.0, 0.0, 0.0, 0.0, 0.0));
            assert!(!m.unlit && !m.double_sided && m.flip_normal_map_y && matches!(m.alpha_mode, AlphaMode::Opaque));
            assert_eq!(m.uv_transform, bevy::math::Affine2::IDENTITY, "a transform would need the normal frame turned too");
            assert_eq!(m.emissive, LinearRgba::BLACK);
        }
    }

    fn triangles(mesh: &Mesh) -> Vec<[u32; 3]> {
        let Some(Indices::U32(idx)) = mesh.indices() else { panic!() };
        idx.chunks(3).map(|t| [t[0], t[1], t[2]]).collect()
    }

    #[test]
    fn a_layer_keeps_only_the_triangles_it_paints() {
        let land = sloped_land();
        // The base keeps every triangle.
        let base = quadrant_mesh(&land, 0, None, Vec2::ZERO, 256.0).unwrap();
        assert_eq!(triangles(&base).len(), (QUAD_SIDE - 1) * (QUAD_SIDE - 1) * 2);
        // Opacity at one inner vertex: the six triangles around it.
        let mut alpha = vec![0.0; QUAD_SIDE * QUAD_SIDE];
        let v = 5 * QUAD_SIDE + 7;
        alpha[v] = 0.4;
        let layer = quadrant_mesh(&land, 0, Some(&alpha), Vec2::ZERO, 256.0).unwrap();
        let tris = triangles(&layer);
        assert_eq!(tris.len(), 6);
        assert!(tris.iter().all(|t| t.contains(&(v as u32))));
        // No opacity anywhere: no mesh at all.
        assert!(quadrant_mesh(&land, 0, Some(&vec![0.0; QUAD_SIDE * QUAD_SIDE]), Vec2::ZERO, 256.0).is_none());
    }

    fn base(q: u8, tex: u32) -> Layer {
        Layer { quadrant: q, texture: Some(tex), layer: -1, base: true, alpha: Vec::new() }
    }

    fn layer(q: u8, tex: u32, n: i16, alpha: &[(usize, f32)]) -> Layer {
        Layer { quadrant: q, texture: Some(tex), layer: n, base: false, alpha: alpha.iter().map(|&(p, a)| (p as u16, a)).collect() }
    }

    fn opacity(layers: &[Layer], q: u8, tex: u32, p: usize) -> Option<f32> {
        layers.iter().find(|l| !l.base && l.quadrant == q && l.texture == Some(tex)).map(|l| alpha_grid(l)[p])
    }

    const L: usize = QUAD_SIDE - 1;

    #[test]
    fn a_texture_painted_up_to_a_cell_edge_carries_over_it() {
        // Save 36 (issue #2089): the south cell's quadrant 2 paints texture 0xA along its
        // north edge; the north cell's quadrant 0 has no layer of it.
        let south = vec![base(2, 0xB), layer(2, 0xA, 0, &[(L * QUAD_SIDE + 12, 1.0), (L * QUAD_SIDE + 13, 0.5), ((L - 1) * QUAD_SIDE + 12, 0.8)])];
        let north = vec![base(0, 0xB), layer(0, 0xC, 0, &[(5 * QUAD_SIDE + 3, 0.4)])];
        let stitched = stitch_layers(&north, &[None, None, Some(south.clone()), None]);
        // A layer of 0xA over the north cell's own, painted on its south edge only.
        let added = stitched.iter().find(|l| l.texture == Some(0xA)).unwrap();
        assert_eq!((added.quadrant, added.layer, added.base), (0, 1, false));
        assert_eq!(opacity(&stitched, 0, 0xA, 12), Some(1.0));
        assert_eq!(opacity(&stitched, 0, 0xA, 13), Some(0.5));
        assert_eq!(added.alpha.len(), 2);
        // Its own layer and base are as they were.
        assert_eq!(&stitched[..2], &north[..]);
        // The south cell keeps what it paints: nothing of 0xC reaches its edge.
        assert_eq!(stitch_layers(&south, &[Some(north), None, None, None]), south);
    }

    #[test]
    fn edge_opacities_take_the_larger_side_inside_a_cell_too() {
        // Quadrant 0's east edge meets quadrant 1's west edge: 0xA disagrees on both
        // sides' rows 3 and 5; a texture that is quadrant 1's base is not added there.
        let own = vec![
            base(0, 0xB),
            base(1, 0xD),
            layer(0, 0xA, 0, &[(3 * QUAD_SIDE + L, 0.2), (5 * QUAD_SIDE + L, 0.9)]),
            layer(1, 0xA, 0, &[(3 * QUAD_SIDE, 0.6), (5 * QUAD_SIDE, 0.1)]),
            layer(0, 0xD, 1, &[(7 * QUAD_SIDE + L, 1.0)]),
        ];
        let stitched = stitch_layers(&own, &[None, None, None, None]);
        assert_eq!(opacity(&stitched, 0, 0xA, 3 * QUAD_SIDE + L), Some(0.6));
        assert_eq!(opacity(&stitched, 0, 0xA, 5 * QUAD_SIDE + L), Some(0.9));
        assert_eq!(opacity(&stitched, 1, 0xA, 3 * QUAD_SIDE), Some(0.6));
        assert_eq!(opacity(&stitched, 1, 0xA, 5 * QUAD_SIDE), Some(0.9));
        assert_eq!(opacity(&stitched, 1, 0xD, 7 * QUAD_SIDE), None);
        assert_eq!(stitched.len(), own.len());
        // Edges that agree are left alone.
        let agreed = stitch_layers(&stitched, &[None, None, None, None]);
        assert_eq!(agreed, stitched);
    }

    #[test]
    fn quadrants_across_each_side() {
        assert_eq!(across(0, EAST), (None, 1));
        assert_eq!(across(1, EAST), (Some(EAST), 0));
        assert_eq!(across(0, WEST), (Some(WEST), 1));
        assert_eq!(across(2, SOUTH), (None, 0));
        assert_eq!(across(0, SOUTH), (Some(SOUTH), 2));
        assert_eq!(across(3, NORTH), (Some(NORTH), 1));
        assert_eq!(across(1, NORTH), (None, 3));
        // A side's positions meet the opposite side's of the quadrant across.
        assert_eq!(edge_position(NORTH, 4), L * QUAD_SIDE + 4);
        assert_eq!(edge_position(SOUTH, 4), 4);
        assert_eq!(edge_position(EAST, 4), 4 * QUAD_SIDE + L);
        assert_eq!(edge_position(WEST, 4), 4 * QUAD_SIDE);
    }
    #[test]
    fn bc5_terrain_base_blended_and_all_folded_slots_use_normal_role() {
        use bevy::ecs::system::SystemState;
        let (_inputs, mut world) = bc5_fixture::inputs();
        world.insert_resource(TerrainSettings { tile: 256.0 });
        world.init_resource::<TerrainCache>();
        world.init_resource::<Assets<TerrainMaterial>>();
        let mut state = SystemState::<(ModelCtx, TerrainMaterials)>::new(&mut world);
        let (mut assets, mut tm) = state.get_mut(&mut world).unwrap();
        for (normal, size) in [("signed.dds", UVec2::splat(4)), ("partial.dds", UVec2::new(3, 5)), ("unsigned.dds", UVec2::splat(4))] {
            // Independent fresh material ownership for each normal; one texture cache.
            *tm.cache = TerrainCache::default();
            tm.cache.normals.insert("old.dds".into(), normal.into());
            let linear = assets.textures.get(normal, false, &assets.data, &mut assets.images, None).unwrap();
            let mut ctx = TerrainCtx { tm: &mut tm, assets: &mut assets };
            let base = ctx.base_material(Some("old.dds".into()));
            let blended = ctx.layer_material("old.dds".into(), 1).unwrap();
            let layers = vec!["old.dds".to_string(); FOLDED_LAYERS];
            let folded = ctx.folded_material(Some("old.dds".into()), &layers, 256.0).unwrap();
            let material = ctx.assets.materials.get(&base).unwrap();
            let normal_h = material.base.normal_map_texture.as_ref().unwrap().clone();
            bc5_fixture::normal(&ctx.assets.images, &normal_h, size);
            assert_ne!(normal_h, linear);
            assert!(material.base.flip_normal_map_y);
            assert_eq!(material.base.reflectance, 0.2);
            let material = ctx.assets.materials.get(&blended).unwrap();
            assert_eq!(material.base.normal_map_texture, Some(normal_h.clone()));
            assert_eq!(material.base.alpha_mode, AlphaMode::Blend);
            let material = ctx.tm.folded.get(&folded).unwrap();
            assert_eq!(material.base.normal_map_texture, Some(normal_h.clone()));
            let ext = &material.extension;
            assert_eq!(ext.count, FOLDED_LAYERS as u32);
            for slot in [&ext.normal0, &ext.normal1, &ext.normal2, &ext.normal3, &ext.normal4, &ext.normal5] {
                assert_eq!(slot.as_ref(), Some(&normal_h));
            }
            assert_eq!(ctx.base_material(Some("old.dds".into())), base);
            assert_eq!(ctx.layer_material("old.dds".into(), 1), Some(blended));
            assert_eq!(ctx.folded_material(Some("old.dds".into()), &layers, 256.0), Some(folded));
        }
        let mut ctx = TerrainCtx { tm: &mut tm, assets: &mut assets };
        ctx.tm.cache.normals.insert("old.dds".into(), "old.dds".into());
        let old = ctx.surface(Some("old.dds"));
        assert!(old.normal_map_texture.is_some());
        let image = ctx.assets.images.get(old.normal_map_texture.as_ref().unwrap()).unwrap();
        assert_eq!(image.texture_descriptor.format, bevy::render::render_resource::TextureFormat::Bc3RgbaUnorm);
        assert_eq!(image.data.as_deref(), Some([255u8; 16].as_slice()));
        assert_eq!(old.perceptual_roughness, 0.95);
        ctx.tm.cache.normals.insert("old.dds".into(), "missing.dds".into());
        let missing = ctx.surface(Some("old.dds"));
        assert!(missing.normal_map_texture.is_none());
        assert!(missing.base_color_texture.is_some());
        assert!(ctx.surface(None).normal_map_texture.is_none());
    }

    #[test]
    fn bc5_folded_normals_are_admitted_independently_of_the_base_normal() {
        use bevy::ecs::system::SystemState;
        let (_inputs, mut world) = bc5_fixture::inputs();
        world.insert_resource(TerrainSettings { tile: 256.0 });
        world.init_resource::<TerrainCache>();
        world.init_resource::<Assets<TerrainMaterial>>();
        let mut state = SystemState::<(ModelCtx, TerrainMaterials)>::new(&mut world);
        let (mut assets, mut tm) = state.get_mut(&mut world).unwrap();
        let layers: Vec<String> = ["old.dds", "OLD.dds", "Old.dds", "oLd.dds", "olD.dds", "OLD.DDS"].into_iter().map(str::to_string).collect();
        let normals = ["signed.dds", "partial.dds", "unsigned.dds", "signed.dds", "partial.dds", "unsigned.dds"];
        for (layer, normal) in layers.iter().zip(normals) { tm.cache.normals.insert(layer.clone(), normal.into()); }
        let mut ctx = TerrainCtx { tm: &mut tm, assets: &mut assets };
        for count in [1, FOLDED_LAYERS] {
            let h = ctx.folded_material(None, &layers[..count], 256.0).unwrap();
            let material = ctx.tm.folded.get(&h).unwrap();
            assert!(material.base.normal_map_texture.is_none(), "base normal cannot stand in for a folded slot");
            let ext = &material.extension;
            assert_eq!(ext.count, count as u32);
            let slots = [&ext.normal0, &ext.normal1, &ext.normal2, &ext.normal3, &ext.normal4, &ext.normal5];
            for (i, slot) in slots.into_iter().enumerate() {
                if i < count {
                    bc5_fixture::normal(&ctx.assets.images, slot.as_ref().unwrap(), if i % 3 == 1 { UVec2::new(3, 5) } else { UVec2::splat(4) });
                    if i >= 3 { assert_eq!(slot, slots[i - 3]); }
                } else { assert!(slot.is_none()); }
            }
            assert_eq!(ctx.folded_material(None, &layers[..count], 256.0), Some(h));
        }
    }

}
