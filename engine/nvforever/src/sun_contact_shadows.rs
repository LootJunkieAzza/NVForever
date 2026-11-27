//! Sun contact shadows of the screen (the settings row Sun contact shadows, Graphics ›
//! Shadows): short-range shadows of what the depth prepass holds, marched towards the sun
//! at half the render size and multiplied into the outdoor sun's direct light. They add
//! what the sun's shadow maps cannot resolve — a blade of grass on the ground, a kerb, a
//! crate's foot — and they are the only sun shadows where the cascades end or are coarse.
//!
//! **What it is not.** Not a shadow map: what the depth buffer shows is all it knows, so a
//! caster whose gap to the receiver lies off the screen, or behind another surface, casts
//! nothing; a surface seen at a grazing angle can shade itself where the depth is steep.
//! The march is short ([`LENGTH`], about a stride), which keeps both limits small: the
//! shadows are contact shadows, not the long shadows of the cascades. An enhancement of
//! our own, not the original game's.
//!
//! **Where it goes.** [`march`] writes half-resolution texels, [`denoise`] blurs them over
//! the 4 x 4 texels the jitter's pattern repeats over, and the view's mesh bind group
//! carries the result at the sun visibility's binding 45 (bevy_pbr
//! `NvfSunVisibilityBindings`, engine/vendor/README.md): the patched `apply_pbr_lighting`
//! multiplies the outdoor sun's shadow by it (sun_visibility.rs `HOOK_EDITS`,
//! sun_visibility.wesl `nvf_screen_visibility`). The pass runs between the depth prepass
//! and the main pass, where the ambient occlusion's does.
//!
//! **What it leaves out.** The first-person arms and weapon (their depth is squeezed into
//! the nearest 1/64 of the range, nif_material.rs "First-person depth", so neither they nor
//! the world behind them can be placed from the depth; the sun's shadows on them are the
//! shade light's), the sky, and every view drawn without a single-sampled depth prepass
//! (the multisampled views: the pass would need the depth resolved first). The shadow maps'
//! own shadows are untouched: this only darkens the sun where the maps leave it lit.
//!
//! **Tuning aids** (development only): `NVF_SUN_CONTACT=<0..1>` another strength (0 none),
//! `NVF_SUN_CONTACT_MARCH=<steps>,<thickness>,<length>` another march, and
//! `NVF_SUN_CONTACT_PRINT=1` one line a second with what the pass last saw.

use bevy::camera::MainPassResolutionOverride;
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::pbr::NvfSunVisibilityBindings;
use bevy::prelude::*;
use bevy::render::camera::TemporalJitter;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ExtractComponent, ExtractComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, Extent3d, FilterMode,
    FragmentState, LoadOp, Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler,
    SamplerBindingType, SamplerDescriptor, ShaderStages, ShaderType, StoreOp, TextureDescriptor, TextureDimension, TextureFormat, TextureSampleType,
    TextureUsages, UniformBuffer,
};
use bevy::render::renderer::{RenderContext, RenderDevice, RenderQueue, ViewQuery};
use bevy::render::texture::{CachedTexture, TextureCache};
use bevy::render::view::ExtractedView;
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

use crate::graphics::Graphics;
use crate::sky::Sun;
use crate::sun_visibility::SunVisibility;

/// How strongly a fully occluded texel darkens the sun's light on the surface behind it
/// (0..1) [engineering choice: by eye, against the sun's own shadows' edge].
pub const STRENGTH: f32 = 0.75;
/// The ray's length (game units, about 1.2 m): as far as a stride's shadows reach, past
/// which the sun's cascades are the shadows' source [engineering choice].
pub const LENGTH: f32 = 48.0;
/// How thick a surface the depth buffer shows is assumed to be (game units, about 20 cm):
/// a ray that passes within this of a surface behind it counts as blocked by it
/// [engineering choice: thicker than the grass blades it is for].
pub const THICKNESS: f32 = 8.0;
/// The ray's start's offset (game units): past the receiver's own texel's depth error and
/// the slope of its neighbours, so a surface does not shadow itself [engineering choice].
pub const BIAS: f32 = 1.5;
/// Steps of the march [engineering choice: with the crossing refined between steps, this is
/// what a blade's thickness is measured against; 20 over the stride is about 2.4 units a
/// step].
pub const STEPS: u32 = 20;
/// The sun's elevation below which nothing is drawn (the sine of about 1.1 degrees, the
/// horizon fade that stops the sun's own shadows, sky.rs `HORIZON_FADE`).
pub const MIN_LIGHT_SIN: f32 = 0.02;

/// The distances from the camera (game units) where the effect starts to fade and where it
/// is gone, as the F11 window's Distance slider sets the latter (the former is 0.85 of it).
pub const DISTANCE_DEFAULT: f32 = 3200.0;
pub const DISTANCE_RANGE: [f32; 2] = [800.0, 12000.0];
pub const DISTANT_DISTANCE_DEFAULT: f32 = 42_000.0;
pub const DISTANT_DISTANCE_MAX: f32 = 64_000.0;

/// The F11 window's slider ranges (the values a hand-edited `sun-shadows.ini` is held to).
/// The thickness has a ceiling: at 32 units (80 cm) the march found an occluder nearly
/// everywhere and the whole picture went into shadow (the owner's run of 2026-11-25, with
/// strength 1, thickness 32 and bias 0); the bias has a floor for the same reason.
pub const LENGTH_RANGE: [f32; 2] = [8.0, 160.0];
pub const THICKNESS_RANGE: [f32; 2] = [1.0, 16.0];
pub const BIAS_RANGE: [f32; 2] = [0.5, 8.0];

/// The default of the temporal history's weight ([`SunContactTuning::history`]).
pub const HISTORY: f32 = 0.6;

/// The pass's live tuning: the F11 window's Shadows tab, kept in `sun-shadows.ini` in the
/// engine home. The constants above are its defaults; the environment aids
/// (`NVF_SUN_CONTACT`, `NVF_SUN_CONTACT_MARCH`) override it for a run.
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct SunContactTuning {
    pub strength: f32,
    pub length: f32,
    pub thickness: f32,
    pub bias: f32,
    pub steps: u32,
    /// The camera distance (game units) where the shadows are gone ([`DISTANCE_DEFAULT`];
    /// they start to fade at 0.85 of it).
    pub distance: f32,
    /// Far screen-space coverage in game units, 0 disables the distant profile.
    pub distant_distance: f32,
    /// The march at the render size instead of half of it (sharper, four times the reads).
    pub full: bool,
    /// The denoise's softness, 0 (3 x 3) to 1 (7 x 7) ([`SunContactTuning::softness`]).
    pub softness: f32,
    /// The temporal history's weight ([`SunContactTuning::history`]).
    pub history: f32,
}

impl Default for SunContactTuning {
    fn default() -> Self {
        Self { strength: STRENGTH, length: LENGTH, thickness: THICKNESS, bias: BIAS, steps: STEPS, distance: DISTANCE_DEFAULT, distant_distance: DISTANT_DISTANCE_DEFAULT, full: false, softness: 0.5, history: HISTORY }
    }
}

/// Where [`SunContactTuning`] lives (the engine home, beside `nr-settings.ini`).
pub const TUNING_FILE: &str = "sun-shadows.ini";

impl SunContactTuning {
    /// The defaults, with the file's values where it has them.
    pub fn load() -> Self {
        let mut t = Self::default();
        let Some(home) = fnv_install::EngineHome::user() else { return t };
        let Ok(text) = std::fs::read_to_string(home.root.join(TUNING_FILE)) else { return t };
        for line in text.lines() {
            let Some((k, v)) = line.split_once('=') else { continue };
            let v = v.trim();
            match k.trim() {
                "strength" => t.strength = v.parse().unwrap_or(t.strength),
                "length" => t.length = v.parse().unwrap_or(t.length),
                "thickness" => t.thickness = v.parse().unwrap_or(t.thickness),
                "bias" => t.bias = v.parse().unwrap_or(t.bias),
                "steps" => t.steps = v.parse().unwrap_or(t.steps),
                "distance" => t.distance = v.parse().unwrap_or(t.distance),
                "distant-distance" => t.distant_distance = v.parse().unwrap_or(t.distant_distance),
                "full" => t.full = v != "0",
                "softness" => t.softness = v.parse().unwrap_or(t.softness),
                "history" => t.history = v.parse().unwrap_or(t.history),
                _ => {}
            }
        }
        t.held_to_range()
    }

    /// Every value inside the range its slider offers (a hand-edited file cannot make the
    /// march absurd: 160 units is a long stride, 32 steps a cost).
    pub fn held_to_range(mut self) -> Self {
        self.strength = self.strength.clamp(0.0, 1.0);
        self.length = self.length.clamp(LENGTH_RANGE[0], LENGTH_RANGE[1]);
        self.thickness = self.thickness.clamp(THICKNESS_RANGE[0], THICKNESS_RANGE[1]);
        self.bias = self.bias.clamp(BIAS_RANGE[0], BIAS_RANGE[1]);
        self.steps = self.steps.clamp(4, 32);
        self.distance = self.distance.clamp(DISTANCE_RANGE[0], DISTANCE_RANGE[1]);
        self.distant_distance = if self.distant_distance.is_finite() {
            self.distant_distance.clamp(0.0, DISTANT_DISTANCE_MAX)
        } else {
            DISTANT_DISTANCE_DEFAULT
        };
        self.softness = self.softness.clamp(0.0, 1.0);
        self.history = self.history.clamp(0.0, 0.95);
        self
    }

    pub fn save(&self) {
        let Some(home) = fnv_install::EngineHome::user() else { return };
        let text = format!(
            "strength={}\nlength={}\nthickness={}\nbias={}\nsteps={}\ndistance={}\ndistant-distance={}\nfull={}\nsoftness={}\nhistory={}\n",
            self.strength, self.length, self.thickness, self.bias, self.steps, self.distance, self.distant_distance, u8::from(self.full), self.softness, self.history
        );
        let _ = std::fs::write(home.root.join(TUNING_FILE), text);
    }
}

/// The sun contact shadows of a view, as the lighting multiplies the sun's light by them.
#[derive(Component, Clone, Copy, Debug, PartialEq, ExtractComponent)]
#[extract_app(bevy::render::RenderApp)]
pub struct SunContactShadows {
    /// The sun's direction in Bevy world space (unit, towards the light), as aimed this
    /// frame; the render side turns it into the view's space.
    pub toward_light: Vec3,
    /// 0..1, see [`STRENGTH`].
    pub strength: f32,
    /// See [`LENGTH`], [`THICKNESS`], [`BIAS`], [`STEPS`].
    pub length: f32,
    pub thickness: f32,
    pub bias: f32,
    pub steps: u32,
    /// The camera distance (game units) where the shadows are gone; they start to fade at
    /// 0.85 of it ([`DISTANCE_DEFAULT`]).
    pub distance: f32,
    pub distant_distance: f32,
    /// The march at the render size instead of half of it ([`SunContactTuning::full`]).
    pub full: bool,
    /// The denoise's softness, 0 (a 3 x 3 tent, crisp) to 1 (a 5 x 5) [engineering choice:
    /// Full resolution read as too crisp at 0, the owner's note of 2026-11-25].
    pub softness: f32,
    /// The temporal history's weight, 0 (current frame only) to 0.95 (mostly history): what
    /// the mod's `screenspace_docc.h` blends, reprojected by the camera's matrices.
    pub history: f32,
}

impl Default for SunContactShadows {
    fn default() -> Self {
        Self { toward_light: Vec3::Y, strength: STRENGTH, length: LENGTH, thickness: THICKNESS, bias: BIAS, steps: STEPS, distance: DISTANCE_DEFAULT, distant_distance: DISTANT_DISTANCE_DEFAULT, full: false, softness: 0.5, history: HISTORY }
    }
}

pub struct SunContactShadowsPlugin;

impl Plugin for SunContactShadowsPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "sun_contact_shadows.wesl");
        // The clear's own shader (a march shader that fails must not blacken the sun).
        bevy::asset::embedded_asset!(app, "sun_contact_clear.wesl");
        app.insert_resource(SunContactTuning::load())
            .add_plugins(ExtractComponentPlugin::<SunContactShadows>::default())
            // Before the sun visibility reads its switches (sun_visibility.rs `mark_targets`).
            .add_systems(PostUpdate, follow.before(crate::sun_visibility::mark_targets))
            .add_systems(Update, persist_tuning);
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .init_resource::<ContactGpu>()
            .add_systems(RenderStartup, init_pipelines)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            // As the ambient occlusion's lighting pass: after the depth prepass, before the
            // main pass reads the depth.
            .add_systems(Core3d, pass.after(Core3dSystems::Prepass).before(Core3dSystems::MainPass));
    }
}

/// `NVF_SUN_CONTACT`: another strength (0 none). `NVF_SUN_CONTACT_MARCH=<steps>,<thickness>,
/// <length>`: another march.
fn aids() -> (Option<f32>, Option<(u32, f32, f32)>) {
    static V: std::sync::OnceLock<(Option<f32>, Option<(u32, f32, f32)>)> = std::sync::OnceLock::new();
    *V.get_or_init(|| {
        let strength = std::env::var("NVF_SUN_CONTACT").ok().and_then(|v| v.trim().parse::<f32>().ok()).map(|v| v.clamp(0.0, 1.0));
        let march = std::env::var("NVF_SUN_CONTACT_MARCH").ok().and_then(|v| {
            let p: Vec<f32> = v.split(',').filter_map(|x| x.trim().parse().ok()).collect();
            (p.len() == 3).then(|| (p[0] as u32, p[1], p[2]))
        });
        (strength, march)
    })
}

/// Put [`SunContactShadows`] on the main camera while the row is on and the sun is up, take
/// it off otherwise, and give the sun visibility the switch and the strength it packs.
fn follow(
    graphics: Option<Res<Graphics>>,
    sky: Option<Res<crate::sky::SkyState>>,
    tuning: Res<SunContactTuning>,
    sun: Query<&GlobalTransform, With<Sun>>,
    cams: Query<Entity, With<crate::fly::FlyCam>>,
    marked: Query<Entity, With<SunContactShadows>>,
    mut vis: ResMut<SunVisibility>,
    mut commands: Commands,
) {
    let (aid_strength, aid_march) = aids();
    // Outdoors only: indoors no sun light is flagged (sun_visibility.rs `mark_targets`), and
    // the cell's own lamps are the light.
    let outdoors = sky.is_none_or(|s| s.interior.is_none());
    let on = outdoors && graphics.as_ref().is_some_and(|g| g.sun_contact_shadows);
    let toward = sun.iter().next().map(|t| t.back().as_vec3()).filter(|d| d.y > MIN_LIGHT_SIN);
    let candidate = aid_strength.unwrap_or(tuning.strength);
    // Enabled where the row is on (the views keep the plumbing, so switching it changes no
    // pipeline), but the strength is 0 with nothing to draw: a texture of the last frame
    // with a shadow in it is then multiplied by 0, and the pass stops writing it.
    let want = on && toward.is_some() && candidate > 0.0;
    let strength = if want { candidate } else { 0.0 };
    if vis.screen_enabled != on || vis.screen_strength != strength {
        vis.screen_enabled = on;
        vis.screen_strength = strength;
    }
    for e in &marked {
        if !want || !cams.contains(e) {
            commands.entity(e).remove::<SunContactShadows>();
        }
    }
    let Some(toward) = toward.filter(|_| want) else { return };
    let (steps, thickness, length) = aid_march.unwrap_or((tuning.steps, tuning.thickness, tuning.length));
    for e in &cams {
        commands.entity(e).insert(SunContactShadows { toward_light: toward, strength, length, thickness, bias: tuning.bias, steps, distance: tuning.distance, distant_distance: tuning.distant_distance, full: tuning.full, softness: tuning.softness, history: tuning.history });
    }
}

/// Keep [`SunContactTuning`] in its file, when it changed (the F11 window's Shadows tab).
fn persist_tuning(tuning: Res<SunContactTuning>, mut last: Local<Option<SunContactTuning>>) {
    if last.as_ref() == Some(&*tuning) {
        return;
    }
    tuning.save();
    *last = Some(tuning.clone());
}

/// The pass's textures (half the render size) and the pipelines chosen for the view.
#[derive(Component)]
struct ContactView {
    marched: CachedTexture,
    denoised: CachedTexture,
    /// The last two frames' results (red) and distances (green, in 1000-unit steps): the
    /// one at `history_index` is the last frame's.
    history: [CachedTexture; 2],
    history_index: usize,
}

/// `R8Unorm`: 1 where the sun reaches the fragment, below 1 in a contact shadow.
const FORMAT: TextureFormat = TextureFormat::R8Unorm;

/// The history: the factor and the surface's distance (x 0.001, half floats hold it).
const HISTORY_FORMAT: TextureFormat = TextureFormat::Rg16Float;

/// The texture's open value: 1, the sun unshadowed (only the red channel is read).
const OPEN: wgpu::Color = wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 };

#[derive(Resource)]
struct ContactPipelines {
    march_layout: BindGroupLayoutDescriptor,
    denoise_layout: BindGroupLayoutDescriptor,
    /// The descriptors, with their fragment targets already [`FORMAT`]; queued once by
    /// [`prepare`], as ssao.rs's are.
    march_base: RenderPipelineDescriptor,
    denoise_base: RenderPipelineDescriptor,
    clear_base: RenderPipelineDescriptor,
    march: Option<CachedRenderPipelineId>,
    denoise: Option<CachedRenderPipelineId>,
    clear: Option<CachedRenderPipelineId>,
    history_sampler: Sampler,
}

/// `ContactShadow` of sun_contact_shadows.wesl (field for field).
#[derive(Clone, Copy, Debug, PartialEq, ShaderType)]
struct ContactShadowUniform {
    clip_from_view: Mat4,
    view_from_clip: Mat4,
    world_from_view: Mat4,
    prev_clip_from_world: Mat4,
    light: Vec4,
    params: Vec4,
    size: Vec4,
    extra: Vec4,
    softness: Vec4,
    up: Vec4,
}

/// Render world: the one uniform buffer (the main camera is the only view with the pass) and
/// the previous frame's world-to-clip matrix for the history's reprojection.
#[derive(Resource, Default)]
struct ContactGpu {
    buffer: Option<UniformBuffer<ContactShadowUniform>>,
    prev_clip_from_world: Option<Mat4>,
}

fn init_pipelines(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let march_layout = BindGroupLayoutDescriptor::new(
        "sun_contact_march_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_depth_2d(), uniform_buffer::<ContactShadowUniform>(false))),
    );
    let denoise_layout = BindGroupLayoutDescriptor::new(
        "sun_contact_denoise_layout",
        &BindGroupLayoutEntries::with_indices(
            ShaderStages::FRAGMENT,
            (
                (0, texture_depth_2d()),
                (1, uniform_buffer::<ContactShadowUniform>(false)),
                (2, texture_2d(TextureSampleType::Float { filterable: false })),
                (3, texture_2d(TextureSampleType::Float { filterable: true })),
                (4, sampler(SamplerBindingType::Filtering)),
            ),
        ),
    );
    let shader = assets.load("embedded://nvforever/sun_contact_shadows.wesl");
    let clear_shader = assets.load("embedded://nvforever/sun_contact_clear.wesl");
    let base = |label: &'static str, entry: &'static str, shader: &Handle<Shader>| {
        let mut d = RenderPipelineDescriptor {
            label: Some(label.into()),
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState { shader: shader.clone(), entry_point: Some(entry.into()), ..default() }),
            ..default()
        };
        if let Some(f) = d.fragment.as_mut() {
            f.targets = vec![Some(ColorTargetState { format: FORMAT, blend: None, write_mask: ColorWrites::ALL })];
        }
        d
    };
    let mut march_base = base("sun_contact_march_pipeline", "march", &shader);
    march_base.layout = vec![march_layout.clone()];
    let mut denoise_base = base("sun_contact_denoise_pipeline", "denoise", &shader);
    denoise_base.layout = vec![denoise_layout.clone()];
    // The denoise writes the factor the lighting reads and this frame's history.
    if let Some(f) = denoise_base.fragment.as_mut() {
        f.targets = vec![
            Some(ColorTargetState { format: FORMAT, blend: None, write_mask: ColorWrites::ALL }),
            Some(ColorTargetState { format: HISTORY_FORMAT, blend: None, write_mask: ColorWrites::ALL }),
        ];
    }
    let history_sampler = device.create_sampler(&SamplerDescriptor {
        label: Some("sun_contact_history"),
        mag_filter: FilterMode::Linear,
        min_filter: FilterMode::Linear,
        ..default()
    });
    commands.insert_resource(ContactPipelines {
        march_layout,
        denoise_layout,
        march_base,
        denoise_base,
        clear_base: base("sun_contact_clear_pipeline", "clear", &clear_shader),
        march: None,
        denoise: None,
        clear: None,
        history_sampler,
    });
}

#[allow(clippy::too_many_arguments)]
fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipes: ResMut<ContactPipelines>,
    mut textures: ResMut<TextureCache>,
    (device, queue): (Res<RenderDevice>, Res<RenderQueue>),
    mut gpu: ResMut<ContactGpu>,
    mut views: Query<(
        Entity,
        &ExtractedView,
        &SunContactShadows,
        Option<&TemporalJitter>,
        Option<&MainPassResolutionOverride>,
        Option<&mut NvfSunVisibilityBindings>,
    )>,
    stale: Query<Entity, (With<ContactView>, Without<SunContactShadows>)>,
    // Which history texture holds the last frame's result, per view (the main camera's is
    // the only one).
    mut history_index: Local<Option<(Entity, usize)>>,
) {
    for entity in &stale {
        commands.entity(entity).remove::<ContactView>();
    }
    if views.is_empty() {
        return;
    }
    // The three pipelines draw into FORMAT; they are queued the first time a view needs
    // them (ssao.rs's rule).
    if pipes.march.is_none() {
        pipes.march = Some(cache.queue_render_pipeline(pipes.march_base.clone()));
    }
    if pipes.denoise.is_none() {
        pipes.denoise = Some(cache.queue_render_pipeline(pipes.denoise_base.clone()));
    }
    if pipes.clear.is_none() {
        pipes.clear = Some(cache.queue_render_pipeline(pipes.clear_base.clone()));
    }
    for (entity, view, settings, jitter, override_size, bindings) in &mut views {
        // The picture's size (the target's) and the size the main pass is drawn at (below
        // the target under FSR or DLSS, `MainPassResolutionOverride`).
        let target = Vec2::new(view.viewport.z as f32, view.viewport.w as f32).max(Vec2::ONE);
        let render = override_size.map(|o| o.0.as_vec2()).unwrap_or(target).max(Vec2::ONE);
        let size = if settings.full {
            render.as_uvec2().max(UVec2::ONE)
        } else {
            ((render + Vec2::ONE) / 2.0).max(Vec2::ONE).as_uvec2()
        };
        let half = |label: &'static str| TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let marched = textures.get(&device, half("sun_contact_shadows_marched"));
        let denoised = textures.get(&device, half("sun_contact_shadows"));
        // The history's two textures, the last frame's result in one and this frame's in the
        // other; the index is flipped here so the pass reads what the frame before wrote.
        let history_of = |label: &'static str| TextureDescriptor {
            label: Some(label),
            size: Extent3d { width: size.x, height: size.y, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: HISTORY_FORMAT,
            usage: TextureUsages::RENDER_ATTACHMENT | TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        let history = [textures.get(&device, history_of("sun_contact_history_a")), textures.get(&device, history_of("sun_contact_history_b"))];
        // Which of the two holds the last frame's result; the other is written this frame.
        let first = !matches!(*history_index, Some((e, _)) if e == entity);
        let read = match *history_index {
            Some((e, i)) if e == entity => i,
            _ => 0,
        };
        *history_index = Some((entity, 1 - read));
        if let Some(mut b) = bindings {
            if b.screen_texture.id() != denoised.default_view.id() {
                b.screen_texture = denoised.default_view.clone();
            }
        }
        // The projection as drawn (the view's, jittered as a temporal method jitters it) and its
        // inverse: the depth prepass and the main pass are drawn with the jittered one.
        let mut clip_from_view = view.clip_from_view;
        if let Some(jitter) = jitter {
            jitter.jitter_projection(&mut clip_from_view, render);
        }
        let view_from_world = view.world_from_view.affine().inverse();
        let light = view_from_world.transform_vector3(settings.toward_light).normalize_or_zero();
        let clip_from_world = clip_from_view * view_from_world;
        let gpu = &mut *gpu;
        let prev = gpu.prev_clip_from_world.unwrap_or(Mat4::IDENTITY);
        let reset = first || gpu.prev_clip_from_world.is_none();
        gpu.prev_clip_from_world = Some(clip_from_world);
        let up = view_from_world.transform_vector3(Vec3::Y).normalize_or_zero();
        let uniform = ContactShadowUniform {
            clip_from_view,
            view_from_clip: clip_from_view.inverse(),
            world_from_view: view.world_from_view.affine().into(),
            prev_clip_from_world: prev,
            light: Vec4::new(light.x, light.y, light.z, settings.strength),
            params: Vec4::new(settings.length, settings.thickness, settings.bias, settings.steps as f32),
            size: Vec4::new(render.x, render.y, size.x as f32, size.y as f32),
            extra: Vec4::new(target.x, target.y, settings.distance * 0.85, settings.distance),
            softness: Vec4::new(settings.softness.clamp(0.0, 1.0), settings.history.clamp(0.0, 1.0), if reset { 1.0 } else { 0.0 }, 0.0),
            up: Vec4::new(up.x, up.y, up.z, settings.distant_distance),
        };
        let buffer = gpu.buffer.get_or_insert_with(|| {
            let mut b = UniformBuffer::from(uniform);
            b.set_label(Some("nvf_sun_contact_shadows"));
            b
        });
        buffer.set(uniform);
        buffer.write_buffer(&device, &queue);
        commands.entity(entity).insert(ContactView { marched, denoised, history, history_index: read });
    }
}

#[allow(clippy::type_complexity)]
fn pass(
    view: ViewQuery<(&ContactView, Option<&ViewPrepassTextures>)>,
    pipes: Res<ContactPipelines>,
    cache: Res<PipelineCache>,
    device: Res<RenderDevice>,
    gpu: Res<ContactGpu>,
    mut ctx: RenderContext,
) {
    let (contact, prepass) = view.into_inner();
    // The prepass's depth as a single-sampled texture: a multisampled one cannot be bound
    // to this pass's `texture_depth_2d` (it would need resolving first).
    let single = prepass.and_then(|p| p.depth.as_ref()).is_some_and(|d| d.texture.texture.sample_count() == 1);
    let depth = prepass.and_then(|p| p.depth_only_view()).filter(|_| single);
    let ready = depth.and_then(|d| {
        Some((d, cache.get_render_pipeline(pipes.march?)?, cache.get_render_pipeline(pipes.denoise?)?, gpu.buffer.as_ref()?.binding()?))
    });
    let Some((depth, march, denoise, binding)) = ready else {
        clear(&mut ctx, &cache, &pipes, contact);
        return;
    };
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_sun_contact_shadows");
    let march_span = diagnostics.time_span(ctx.command_encoder(), "march");
    let layout = cache.get_bind_group_layout(&pipes.march_layout);
    let bind_group = device.create_bind_group("sun_contact_march_bind_group", &layout, &BindGroupEntries::sequential((depth, binding.clone())));
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("sun_contact_march_pass"),
        color_attachments: &[Some(RenderPassColorAttachment {
            view: &contact.marched.default_view,
            depth_slice: None,
            resolve_target: None,
            // A full-screen triangle writes every texel; loaded anyway, as nothing reads a
            // stale one (a texel the pipeline skips would read back the last frame's).
            ops: Operations { load: LoadOp::Clear(OPEN), store: StoreOp::Store },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(march);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);
    march_span.end(ctx.command_encoder());
    let denoise_span = diagnostics.time_span(ctx.command_encoder(), "denoise");
    let read = contact.history_index;
    let write = 1 - read;
    let layout = cache.get_bind_group_layout(&pipes.denoise_layout);
    let bind_group = device.create_bind_group(
        "sun_contact_denoise_bind_group",
        &layout,
        &BindGroupEntries::with_indices((
            (0, depth),
            (1, binding),
            (2, &contact.marched.default_view),
            (3, &contact.history[read].default_view),
            (4, &pipes.history_sampler),
        )),
    );
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("sun_contact_denoise_pass"),
        color_attachments: &[
            Some(RenderPassColorAttachment { view: &contact.denoised.default_view, depth_slice: None, resolve_target: None, ops: Operations::default() }),
            // The history is written whole, so nothing stale survives in it.
            Some(RenderPassColorAttachment { view: &contact.history[write].default_view, depth_slice: None, resolve_target: None, ops: Operations::default() }),
        ],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(denoise);
    pass.set_bind_group(0, &bind_group, &[]);
    pass.draw(0..3, 0..1);
    drop(pass);
    denoise_span.end(ctx.command_encoder());
    span.end(ctx.command_encoder());
}

/// The view's texture cleared open, so the sun is not darkened by a texture nothing wrote.
fn clear(ctx: &mut RenderContext, cache: &PipelineCache, pipes: &ContactPipelines, contact: &ContactView) {
    let Some(pipeline) = pipes.clear.and_then(|id| cache.get_render_pipeline(id)) else { return };
    let ops = Operations { load: LoadOp::Clear(OPEN), store: StoreOp::Store };
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("sun_contact_clear_pass"),
        color_attachments: &[Some(RenderPassColorAttachment { view: &contact.denoised.default_view, depth_slice: None, resolve_target: None, ops })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.draw(0..3, 0..1);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The shader is WGSL that naga accepts. The real pipeline's one import (the fullscreen
    /// vertex output, resolved by WESL at build time) is cut out here and given as the
    /// struct Bevy's shader has.
    #[test]
    fn shader_is_valid_wgsl() {
        use wgpu::naga;
        let source = include_str!("sun_contact_shadows.wesl").replace("\r\n", "\n");
        let mut out = String::new();
        for line in source.lines() {
            if line.starts_with("import bevy_core_pipeline::fullscreen_vertex_shader") {
                out.push_str("struct FullscreenVertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n}\n");
                continue;
            }
            out.push_str(line);
            out.push('\n');
        }
        let module = naga::front::wgsl::parse_str(&out).unwrap_or_else(|e| panic!("{}", e.emit_to_string(&out)));
        naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
            .validate(&module)
            .unwrap_or_else(|e| panic!("{e:?}"));
        for f in ["fn march(", "fn denoise("] {
            assert!(out.contains(f), "the shader has {f}");
        }
        // The clear is its own shader, so a broken march cannot blacken the sun.
        let clear = include_str!("sun_contact_clear.wesl").replace("\r\n", "\n");
        assert!(clear.contains("fn clear("));
        assert!(!out.contains("fn clear("), "the march shader has no clear of its own");
        // WGSL (the engine's parser) refuses `*` and `^` in one expression without
        // parentheses — the build of 2026-11-25 failed to compile the shader over exactly
        // that and left the sun black. A line here must not mix them.
        for line in out.lines().filter(|l| !l.trim_start().starts_with("//")) {
            let code = line.split("//").next().unwrap_or("");
            assert!(!(code.contains('^') && code.contains('*')), "`^` with `*` needs parentheses: {line}");
            assert!(!(code.contains('&') && code.contains('*')), "`&` with `*` needs parentheses: {line}");
        }
        // The first-person share is first_person_depth.wesl's, word for word.
        assert!(include_str!("../../nvf-assets/src/first_person_depth.wesl").contains("const FIRST_PERSON_DEPTH_SHARE: f32 = 0.015625;"));
        assert!(out.contains("const FIRST_PERSON_SHARE: f32 = 0.015625;"));
        // The fade's distances are the uniform's (the F11 window's Distance slider);
        // the distance range is the Rust side's.
        assert_eq!(DISTANCE_RANGE, [800.0, 12000.0]);
        assert!(out.contains("smoothstep(contact.extra.z, contact.extra.w, dist)"), "the fade reads extra.z/w");
        // The uniform's layout: two mat4 and four vec4, in the shader's order.
        assert_eq!(ContactShadowUniform::min_size().get(), 352, "four mat4 and six vec4");
        let mut last = 0;
        for f in ["clip_from_view: mat4x4<f32>", "view_from_clip: mat4x4<f32>", "world_from_view: mat4x4<f32>", "prev_clip_from_world: mat4x4<f32>", "light: vec4<f32>", "params: vec4<f32>", "size: vec4<f32>", "extra: vec4<f32>", "softness: vec4<f32>", "up: vec4<f32>"] {
            let at = out.find(f).unwrap_or_else(|| panic!("the uniform has {f}"));
            assert!(at > last, "{f} after the field before it");
            last = at;
        }
    }

    #[test]
    fn the_tuning_is_held_to_its_sliders_range() {
        let wild = SunContactTuning { strength: 9.0, length: 5_000.0, thickness: 0.0, bias: -2.0, steps: 900, distance: 1.0, distant_distance: 100_000.0, full: false, softness: 9.0, history: 9.0 }.held_to_range();
        assert_eq!(wild, SunContactTuning { strength: 1.0, length: 160.0, thickness: 1.0, bias: 0.5, steps: 32, distance: 800.0, distant_distance: DISTANT_DISTANCE_MAX, full: false, softness: 1.0, history: 0.95 });
        assert_eq!(SunContactTuning::default(), SunContactTuning { strength: STRENGTH, length: LENGTH, thickness: THICKNESS, bias: BIAS, steps: STEPS, distance: DISTANCE_DEFAULT, distant_distance: DISTANT_DISTANCE_DEFAULT, full: false, softness: 0.5, history: HISTORY });
    }
}
