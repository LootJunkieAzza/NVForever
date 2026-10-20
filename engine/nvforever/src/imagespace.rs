//! The image-space grade as a full-screen pass after tonemapping: the worldspace's base
//! image space with the weather's adapters for the time of day (fnv_world::imagespace,
//! docs/spec-notes/world.md §6), and on top of it the image space modifiers scripts play
//! on the screen (fnv_world::image_modifier, docs/spec-notes/image-modifiers.md): a fade
//! to a colour, blur, double vision, radial blur, and their changes to the grade.
//!
//! Formula [hypothesis: parameter names from the records; order and the use of display
//! (sRGB) values follow the classic image-space pass, to compare with screenshots]:
//! saturation around the luminance, then a tint towards luminance × tint colour by the
//! tint amount, then brightness as a factor, then contrast around the average luminance.
//!
//! Modifiers (image-modifiers.md §5): the picture is blurred first (two passes of
//! [`blur_pass`], only while a modifier blurs), read doubled and radially blurred by the
//! grade pass, graded with the modifiers' multiply/add on the grade, and last covered by
//! the fade colour. The modifiers on the screen are the script runtime's
//! (`fnv_game::Game::image_modifiers`) and those of `--imod`.
//!
//! Depth of field (image-modifiers.md §8): the modifiers' and the engine's own (VATS
//! focused on the mark, aiming down the sights by the game settings), the strongest, mixed
//! into the blur passes by each point's depth; logged as "depth of field: …" when it
//! changes.
//!
//! VATS mode's look (docs/design/vats-mode.md §5, `nvf_play::vats_mode::VatsMode::look`)
//! comes after the grade and before the fade colour: the amber grade left of the swipe's
//! edge, the creamy band on the edge and the wash over the whole screen [design].

use crate::sky::SkyState;
use crate::world::WorldData;
use bevy::core_pipeline::fullscreen_material::{fullscreen_material_system, FullscreenMaterial, FullscreenMaterialPlugin};
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::ecs::schedule::ScheduleConfigs;
use bevy::ecs::system::BoxedSystem;
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent};
use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::{
    BindGroupEntries, BindGroupLayoutDescriptor, BindGroupLayoutEntries, CachedRenderPipelineId, ColorTargetState, ColorWrites, FilterMode, FragmentState,
    Operations, PipelineCache, RenderPassColorAttachment, RenderPassDescriptor, RenderPipelineDescriptor, Sampler, SamplerBindingType, SamplerDescriptor,
    ShaderStages, ShaderType, TextureFormat, TextureSampleType,
};
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};
use bevy::shader::ShaderRef;
use fnv_world::image_modifier::{combine, graded, image_modifier, Dof, ImageModifier, ScreenEffect, DOF_BACK, DOF_FRONT, DOF_NO_SKY};

/// Grade parameters on the main camera.
#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
pub struct ImageSpaceGrade {
    /// Tint colour (rgb) and amount (a).
    pub tint: Vec4,
    /// Fade colour (rgb) and amount (a): the picture goes towards the colour.
    pub fade: Vec4,
    /// Radial blur strength, ramp up, start, ramp down.
    pub radial: Vec4,
    /// Radial blur down start, its centre (x, y in screen parts); double vision strength.
    pub radial2: Vec4,
    pub saturation: f32,
    pub contrast: f32,
    pub contrast_avg_lum: f32,
    pub brightness: f32,
    /// Blur radius in the records' units (0 none, 7 the most).
    pub blur: f32,
    /// Seconds, for the swing of double vision.
    pub time: f32,
    /// The blur leaves alone what is nearer than this depth (the depth prepass's value,
    /// larger nearer) and does not take it into the blur of what is behind: 0 for none.
    /// The Pip-Boy keeps itself sharp over the blurred world so.
    pub sharp: f32,
    /// VATS mode: the swipe's edge (a share of the screen width, graded to its left), the
    /// band on the edge, the wash over the screen, the grade's strength (all 0: none).
    pub vats: Vec4,
    /// Depth of field: strength, distance, range (game units), the target's distance
    /// along the view (below 0: the point under the screen's centre).
    pub dof: Vec4,
    /// Depth of field: use target (0/1), the flags, the camera's near plane.
    pub dof_mode: Vec4,
}

impl Default for ImageSpaceGrade {
    fn default() -> Self {
        ImageSpaceGrade {
            tint: Vec4::new(1.0, 1.0, 1.0, 0.0),
            fade: Vec4::ZERO,
            radial: Vec4::ZERO,
            radial2: Vec4::new(0.0, 0.5, 0.5, 0.0),
            saturation: 1.0,
            contrast: 1.0,
            contrast_avg_lum: 0.5,
            brightness: 1.0,
            blur: 0.0,
            time: 0.0,
            sharp: 0.0,
            vats: Vec4::ZERO,
            dof: Vec4::new(0.0, 0.0, 0.0, -1.0),
            dof_mode: Vec4::ZERO,
        }
    }
}

impl FullscreenMaterial for ImageSpaceGrade {
    fn fragment_shader() -> ShaderRef {
        "embedded://nvforever/imagespace.wgsl".into()
    }

    /// After tonemapping: the game grades the final display colours [hypothesis].
    fn schedule_configs(system: ScheduleConfigs<BoxedSystem>) -> ScheduleConfigs<BoxedSystem> {
        system.in_set(Core3dSystems::PostProcess).after(tonemapping)
    }
}

/// A modifier put on the screen from the command line (`--imod`).
#[derive(Debug, Clone, PartialEq)]
pub struct Forced {
    pub form: u32,
    /// Hold the modifier at this many seconds after its start; without it an animated
    /// one plays again and again, a second apart.
    pub at: Option<f32>,
}

/// The image space modifiers on the screen.
#[derive(Resource, Default)]
pub struct ScreenModifiers {
    records: HashMap<u32, Option<ImageModifier>>,
    pub forced: Vec<Forced>,
    /// Modifiers the engine plays once (an explosion's, `shot_fx`): the record and when it
    /// started, in `Time::elapsed_secs`.
    played: Vec<(u32, f32)>,
    /// Records played once that are not animatable and were left off the screen.
    refused: Vec<u32>,
    /// The modifiers of the last frame, to log what comes and goes.
    shown: Vec<u32>,
    /// The game settings of the engine's own depth of field, read once.
    dof_settings: Option<DofSettings>,
    /// The depth of field logged last.
    dof_shown: String,
}

impl ScreenModifiers {
    /// Play modifier `form` once from `now`; one already playing starts again, as a
    /// script's does (image-modifiers.md §3). Only an animatable record is shown: one that
    /// is not would stay until something removes it [design].
    pub fn play(&mut self, form: u32, now: f32) {
        self.played.retain(|(f, _)| *f != form);
        self.played.push((form, now));
    }

    fn record(&mut self, world: &WorldData, form: u32) -> Option<&ImageModifier> {
        self.records.entry(form).or_insert_with(|| image_modifier(world.lo, world.schema, form)).as_ref()
    }
}

/// `--imod <Editor ID or 0xFormID>[@seconds]`.
pub fn parse_forced(world: &WorldData, arg: &str) -> Result<Forced, String> {
    let (name, at) = match arg.split_once('@') {
        Some((n, t)) => (n, Some(t.parse::<f32>().map_err(|_| format!("--imod {arg}: {t} is not a number of seconds"))?)),
        None => (arg, None),
    };
    let form = fnv_world::image_modifier::find_image_modifier(world.lo, world.schema, name).ok_or_else(|| format!("--imod {arg}: no image space modifier {name}"))?;
    Ok(Forced { form, at })
}

pub struct ImageSpacePlugin;

impl Plugin for ImageSpacePlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "imagespace.wgsl");
        bevy::asset::embedded_asset!(app, "imagespace_blur.wgsl");
        app.init_resource::<ScreenModifiers>()
            .add_plugins(FullscreenMaterialPlugin::<ImageSpaceGrade>::default())
            .add_systems(Update, update_grade.after(crate::sky::update_sky));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        render_app
            .add_systems(RenderStartup, init_blur)
            .add_systems(Render, prepare_blur.in_set(RenderSystems::Prepare))
            .add_systems(Core3d, blur_pass.in_set(Core3dSystems::PostProcess).after(tonemapping).after(bevy::anti_alias::fxaa::fxaa).after(bevy::anti_alias::smaa::smaa).before(fullscreen_material_system::<ImageSpaceGrade>));
    }
}

/// The modifiers on the screen now as one effect: the script runtime's, each at its own
/// age, and the command line's; with the forms on the screen and the Editor ID of the
/// one whose depth of field is shown.
fn screen_effect(mods: &mut ScreenModifiers, world: &WorldData, script: Option<&nvf_play::script::ScriptState>, now: f32) -> (ScreenEffect, Vec<u32>, String) {
    let mut playing: Vec<(u32, f32)> = Vec::new();
    if let Some(rt) = script.and_then(|s| s.rt.as_ref()) {
        playing.extend(rt.game.image_modifiers.iter().map(|m| (m.modifier.0, m.elapsed as f32)));
    }
    for f in mods.forced.clone() {
        let seconds = f.at.unwrap_or_else(|| {
            // Again and again, a second apart.
            let d = mods.record(world, f.form).filter(|m| m.animatable).map_or(0.0, |m| m.duration);
            if d > 0.0 {
                now % (d + 1.0)
            } else {
                now
            }
        });
        playing.push((f.form, seconds));
    }
    // Those played once: gone when they have run.
    let mut played = std::mem::take(&mut mods.played);
    played.retain(|&(form, start)| match mods.record(world, form) {
        Some(m) if m.animatable => !m.finished(now - start),
        _ => {
            if !mods.refused.contains(&form) {
                mods.refused.push(form);
                warn!("image space modifier 0x{form:08X} played once is not animatable (or not found): left off the screen");
            }
            false
        }
    });
    playing.extend(played.iter().map(|&(form, start)| (form, now - start)));
    mods.played = played;
    let mut effects = Vec::new();
    let mut forms = Vec::new();
    for (form, seconds) in playing {
        let Some(m) = mods.record(world, form) else { continue };
        // A looping `--imod` is off the screen in its second apart.
        if m.finished(seconds) {
            continue;
        }
        effects.push(m.at(seconds));
        forms.push(form);
    }
    // The strongest depth of field is the one shown (`combine`).
    let mut dof_source = String::new();
    let mut top = 0.0;
    for (e, &form) in effects.iter().zip(&forms) {
        if e.dof.strength > top {
            top = e.dof.strength;
            dof_source = mods.record(world, form).map_or_else(|| format!("{form:#010x}"), |m| m.edid.clone());
        }
    }
    (combine(&effects), forms, dof_source)
}

/// The game settings of the engine's own depth of field (image-modifiers.md §8.2, §8.4)
/// [observed values: FalloutNV.esm GMST; the fallbacks are those values].
#[derive(Debug, Clone, Copy)]
struct DofSettings {
    vats_range: f32,
    vats_strength: f32,
    sights_distance: f32,
    sights_range: f32,
    sights_strength: f32,
}

impl DofSettings {
    fn read(sim: Option<&nvf_play::sim::SimState>) -> DofSettings {
        let get = |name: &str, or: f32| sim.and_then(|s| s.forms.setting_f32(name)).unwrap_or(or);
        DofSettings {
            vats_range: get("fVATSDOFRange", 500.0),
            vats_strength: get("fVATSDOFStrengthCap", 0.6),
            sights_distance: get("fIronSightsDOFDistance", 275.0),
            sights_range: get("fIronSightsDOFRange", 650.0),
            sights_strength: get("fIronSightsDOFStrengthCap", 0.0),
        }
    }
}

/// The engine's own depth of field (image-modifiers.md §8.4) [hypothesis]: VATS focused
/// on the mark (`vats_focus`, its distance along the view), beyond it only, faded with
/// the mode's look (`vats_grade`, 0 … 1); aiming down the sights by the zoom's
/// `aim_blend`. The stronger, with the target's distance (below 0: none) and its name.
fn engine_dof(s: &DofSettings, vats_grade: f32, vats_focus: Option<f32>, aim_blend: f32) -> Option<(Dof, f32, &'static str)> {
    let vats = vats_focus
        .filter(|_| vats_grade > 0.0 && s.vats_strength > 0.0)
        .map(|f| (Dof { strength: s.vats_strength * vats_grade, distance: 0.0, range: s.vats_range, use_target: true, flags: DOF_BACK }, f.max(0.0), "VATS"));
    let sights = (aim_blend > 0.0 && s.sights_strength > 0.0).then(|| {
        (Dof { strength: s.sights_strength * aim_blend, distance: s.sights_distance, range: s.sights_range, use_target: false, flags: 0 }, -1.0, "iron sights")
    });
    match (vats, sights) {
        (Some(a), Some(b)) => Some(if b.0.strength > a.0.strength { b } else { a }),
        (a, b) => a.or(b),
    }
}

/// The log line's text for a depth of field (rounded, so a fade logs a few lines).
fn dof_text(d: &Dof, focus: f32, source: &str) -> String {
    if d.strength == 0.0 {
        return "off".to_string();
    }
    let mut t = format!("strength {:.1}, distance {:.0}, range {:.0}", d.strength, d.distance, d.range);
    if d.use_target {
        if focus >= 0.0 {
            t += &format!(", focus {:.0}", (focus / 100.0).round() * 100.0);
        } else {
            t += ", focus on the screen's centre";
        }
        if d.flags & DOF_FRONT != 0 {
            t += ", front";
        }
        if d.flags & DOF_BACK != 0 {
            t += ", back";
        }
    }
    if d.flags & DOF_NO_SKY != 0 {
        t += ", no sky";
    }
    t + &format!(" ({source})")
}

/// The blur behind the open wait menu, in the modifiers' blur radius units [engineering
/// choice: by eye against the owner's screenshot of the original menu].
const WAIT_MENU_BLUR: f32 = 3.0;

/// The blur of the world behind the Pip-Boy, in the same units [documented:
/// `Fallout_default.ini` `[Pipboy] fBlurRadiusPipboy=3.5`; hypothesis: the same units as
/// a modifier's blur radius; its `fBlurIntensityPipboy=0.25` is not used, what it scales
/// is not known]. Eased in and out with the Pip-Boy's clip.
const PIPBOY_BLUR: f32 = 3.5;

/// While the Pip-Boy shows, what is nearer to the eye than this (game units) is the
/// player's own arms and is not blurred [engineering choice: the Pip-Boy is about 11 in
/// front of the eye; the nearest world the player can stand against is further].
const PIPBOY_SHARP: f32 = 40.0;

/// A fade colour and amount (`ScreenEffect::fade`) with black laid over it to `f` (0 …
/// 1): the share of the screen covered grows to `f` of what was left, and the colour
/// darkens with it.
fn black_over(fade: [f32; 4], f: f32) -> [f32; 4] {
    let f = f.clamp(0.0, 1.0);
    let a = fade[3].clamp(0.0, 1.0);
    let covered = a + (1.0 - a) * f;
    let keep = if covered > 0.0 { a * (1.0 - f) / covered } else { 0.0 };
    [fade[0] * keep, fade[1] * keep, fade[2] * keep, covered]
}

/// Copy the sky's grade, with the modifiers on the screen, onto every graded camera.
#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn update_grade(
    time: Res<Time>,
    sky: Res<SkyState>,
    world: Option<Res<WorldData>>,
    script: Option<Res<nvf_play::script::ScriptState>>,
    new_game: Option<Res<nvf_play::script::NewGame>>,
    wait: Option<Res<nvf_play::wait::WaitMenu>>,
    sleep: Option<Res<nvf_play::wait::SleepFade>>,
    (pip, projections, death, vats): (
        Option<Res<nvf_play::pipboy::PipBoy>>,
        Query<(&Projection, &GlobalTransform), With<nvf_play::PlayerCamera>>,
        Option<Res<crate::death_cam::DeathCam>>,
        Option<Res<nvf_play::vats_mode::VatsMode>>,
    ),
    (sim, aim): (Option<Res<nvf_play::sim::SimState>>, Option<Res<nvf_play::aim_zoom::AimZoom>>),
    mut mods: ResMut<ScreenModifiers>,
    mut grades: Query<&mut ImageSpaceGrade>,
) {
    let now = time.elapsed_secs();
    let (mut effect, forms, dof_source) = match &world {
        Some(w) => screen_effect(&mut mods, w, script.as_deref(), now),
        None => (ScreenEffect::default(), Vec::new(), String::new()),
    };
    // A new game shows nothing of the world before its opening runs: until the scripts
    // have started the screen is white, as the opening's first modifier then holds it
    // [engineering choice: the script data loads while the room is already drawn].
    if new_game.is_some() && script.as_deref().is_some_and(|s| s.rt.is_none() && s.off.is_none()) {
        effect.fade = [1.0; 4];
    }
    // Sleeping fades the world to black under the menu (`nvf_play::wait`), and the dead
    // player's screen goes black (`death_cam.rs`): black over whatever fade is on the
    // screen already.
    if let Some(f) = sleep.map(|s| s.0).filter(|f| *f > 0.0) {
        effect.fade = black_over(effect.fade, f);
    }
    if let Some(f) = death.as_deref().map(|d| d.black()).filter(|f| *f > 0.0) {
        effect.fade = black_over(effect.fade, f);
    }
    if forms != mods.shown {
        if let Some(w) = &world {
            let names: Vec<String> = forms.iter().map(|&f| mods.record(w, f).map_or_else(|| format!("{f:#010x}"), |m| m.edid.clone())).collect();
            info!("image space modifiers on the screen: {}", if names.is_empty() { "none".to_string() } else { names.join(", ") });
        }
        mods.shown = forms;
    }
    // The world behind the Pip-Boy is blurred, the Pip-Boy is not.
    let camera = projections.single().ok();
    let near = camera.and_then(|(p, _)| match p {
        Projection::Perspective(p) => Some(p.near),
        _ => None,
    });
    // Depth of field: the modifiers' or the engine's own, the stronger.
    if mods.dof_settings.is_none() && sim.is_some() {
        mods.dof_settings = Some(DofSettings::read(sim.as_deref()));
    }
    let vats_look = vats.as_deref().map_or([0.0; 4], |v| v.look());
    let vats_focus = vats.as_deref().and_then(|v| v.focus()).zip(camera).map(|(p, (_, t))| (nvf_play::space::to_bevy(p) - t.translation()).dot(*t.forward()));
    let aim_blend = aim.as_deref().map_or(0.0, |a| if a.sight.is_some() { a.blend } else { 0.0 });
    let (mut dof, mut focus, mut source) = (effect.dof, -1.0f32, dof_source);
    if let Some((d, f, name)) = mods.dof_settings.as_ref().and_then(|s| engine_dof(s, vats_look[3], vats_focus, aim_blend)) {
        if d.strength > dof.strength {
            (dof, focus, source) = (d, f, name.to_string());
        }
    }
    let text = dof_text(&dof, focus, &source);
    if text != mods.dof_shown {
        info!("depth of field: {text}");
        mods.dof_shown = text;
    }
    let pipboy = pip.filter(|p| p.shown()).map_or(0.0, |p| crate::player_body::pipboy_ease(p.clip_time));
    let (pip_blur, sharp) = match near {
        Some(n) if pipboy > 0.0 => (PIPBOY_BLUR * pipboy, n / PIPBOY_SHARP),
        _ => (0.0, 0.0),
    };
    let c = graded(&sky.cinematic(), &effect);
    let r = &effect.radial;
    let g = ImageSpaceGrade {
        tint: Vec4::new(c.tint[0], c.tint[1], c.tint[2], c.tint_amount),
        fade: Vec4::from_array(effect.fade),
        radial: Vec4::new(r.strength, r.ramp_up, r.start, r.ramp_down),
        radial2: Vec4::new(r.down_start, r.center[0], r.center[1], effect.double_vision),
        saturation: c.saturation,
        contrast: c.contrast,
        contrast_avg_lum: c.contrast_avg_lum,
        brightness: c.brightness,
        // The world behind the wait menu is blurred (docs/spec-notes/wait-menu.md §2).
        // The dying player's view blurs (`death_cam.rs`).
        blur: effect.blur_radius.max(if wait.is_some_and(|w| w.is_open()) { WAIT_MENU_BLUR } else { 0.0 }).max(pip_blur).max(death.map_or(0.0, |d| d.blur())),
        time: now,
        sharp,
        vats: Vec4::from_array(vats_look),
        dof: Vec4::new(dof.strength, dof.distance, dof.range, focus),
        dof_mode: Vec4::new(if dof.use_target { 1.0 } else { 0.0 }, f32::from(dof.flags), near.unwrap_or(1.0), 0.0),
    };
    for mut grade in &mut grades {
        *grade = g;
    }
}

/// The blur passes' pipelines: across and down, per target format and whether the
/// view's depth is multisampled (the depth is read to keep what is near sharp).
#[derive(Resource)]
pub(crate) struct BlurPipelines {
    /// The layouts with a single-sampled and a multisampled depth.
    layouts: [BindGroupLayoutDescriptor; 2],
    sampler: Sampler,
    base: RenderPipelineDescriptor,
    by_format: HashMap<(TextureFormat, bool), [CachedRenderPipelineId; 2]>,
}

/// The blur pipelines of a view, and whether its depth is multisampled.
#[derive(Component)]
pub(crate) struct BlurPipelineIds([CachedRenderPipelineId; 2], bool);

fn init_blur(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let screen = || (texture_2d(TextureSampleType::Float { filterable: true }), sampler(SamplerBindingType::Filtering), uniform_buffer::<ImageSpaceGrade>(true));
    let (a, b, c) = screen();
    let single = BindGroupLayoutDescriptor::new("imagespace_blur_layout", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (a, b, c, texture_depth_2d())));
    let (a, b, c) = screen();
    let multi = BindGroupLayoutDescriptor::new("imagespace_blur_layout_ms", &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (a, b, c, texture_depth_2d_multisampled())));
    // Filtered: each sample of the blur reads two pixels at once.
    let sampler = device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() });
    let base = RenderPipelineDescriptor {
        label: Some("imagespace_blur_pipeline".into()),
        layout: vec![single.clone()],
        vertex: fullscreen.to_vertex_state(),
        fragment: Some(FragmentState { shader: assets.load("embedded://nvforever/imagespace_blur.wgsl"), ..default() }),
        ..default()
    };
    commands.insert_resource(BlurPipelines { layouts: [single, multi], sampler, base, by_format: HashMap::default() });
}

fn prepare_blur(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<BlurPipelines>,
    views: Query<(Entity, &ExtractedView, &Msaa), With<ImageSpaceGrade>>,
) {
    for (entity, view, msaa) in &views {
        let format = view.target_format;
        let multi = msaa.samples() > 1;
        let ids = match pipelines.by_format.get(&(format, multi)) {
            Some(ids) => *ids,
            None => {
                let ids = ["blur_across", "blur_down"].map(|entry| {
                    let mut d = pipelines.base.clone();
                    d.layout = vec![pipelines.layouts[multi as usize].clone()];
                    if let Some(f) = d.fragment.as_mut() {
                        f.entry_point = Some(entry.into());
                        f.targets = vec![Some(ColorTargetState { format, blend: None, write_mask: ColorWrites::ALL })];
                        if multi {
                            f.shader_defs.push("MULTISAMPLED".into());
                        }
                    }
                    cache.queue_render_pipeline(d)
                });
                pipelines.by_format.insert((format, multi), ids);
                ids
            }
        };
        commands.entity(entity).insert(BlurPipelineIds(ids, multi));
    }
}

/// Blur the picture across and then down, while a modifier blurs or a depth of field is
/// on (a view without a depth prepass is not blurred).
pub(crate) fn blur_pass(
    view: ViewQuery<(&ViewTarget, &DynamicUniformIndex<ImageSpaceGrade>, &ImageSpaceGrade, &BlurPipelineIds, &ViewPrepassTextures)>,
    pipelines: Res<BlurPipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<ImageSpaceGrade>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, index, grade, ids, prepass) = view.into_inner();
    if (grade.blur <= 0.0 && grade.dof.x == 0.0) || crate::ssao::depth_multisampled(prepass) != Some(ids.1) {
        return;
    }
    let Some(depth) = prepass.depth_view() else { return };
    let (Some(across), Some(down)) = (cache.get_render_pipeline(ids.0[0]), cache.get_render_pipeline(ids.0[1])) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let layout = cache.get_bind_group_layout(&pipelines.layouts[ids.1 as usize]);
    for pipeline in [across, down] {
        let post = target.post_process_write();
        let bind_group = device.create_bind_group("imagespace_blur_bind_group", &layout, &BindGroupEntries::sequential((post.source, &pipelines.sampler, binding.clone(), depth)));
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some("imagespace_blur_pass"),
            color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind_group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::{black_over, dof_text, engine_dof, DofSettings};

    #[test]
    fn the_engine_focuses_vats_on_the_mark() {
        let s = DofSettings { vats_range: 500.0, vats_strength: 0.6, sights_distance: 275.0, sights_range: 650.0, sights_strength: 0.0 };
        // No mark, or the look gone: none.
        assert!(engine_dof(&s, 1.0, None, 0.0).is_none());
        assert!(engine_dof(&s, 0.0, Some(800.0), 0.0).is_none());
        let (d, f, name) = engine_dof(&s, 1.0, Some(812.0), 0.0).unwrap();
        assert_eq!((d.strength, d.range, f, name), (0.6, 500.0, 812.0, "VATS"));
        assert_eq!(dof_text(&d, f, name), "strength 0.6, distance 0, range 500, focus 800, back (VATS)");
        // The shipped iron sights cap is 0: no depth of field down the sights.
        assert!(engine_dof(&s, 0.0, None, 1.0).is_none());
        let raised = DofSettings { sights_strength: 0.5, ..s };
        let (d, _, name) = engine_dof(&raised, 0.0, None, 1.0).unwrap();
        assert_eq!((d.strength, d.distance, d.range, name), (0.5, 275.0, 650.0, "iron sights"));
        assert_eq!(dof_text(&fnv_world::image_modifier::Dof::default(), -1.0, ""), "off");
    }

    #[test]
    fn black_lays_over_a_fade() {
        // Nothing on the screen: black to the amount asked.
        assert_eq!(black_over([0.0; 4], 0.0), [0.0; 4]);
        assert_eq!(black_over([0.0; 4], 0.25), [0.0, 0.0, 0.0, 0.25]);
        // Fully black whatever was there.
        assert_eq!(black_over([1.0, 1.0, 1.0, 0.5], 1.0), [0.0, 0.0, 0.0, 1.0]);
        // Half black over a half white fade: three quarters covered, a third of it white.
        let [r, _, _, a] = black_over([1.0, 1.0, 1.0, 0.5], 0.5);
        assert!((a - 0.75).abs() < 1e-6 && (r - 1.0 / 3.0).abs() < 1e-6, "{r} {a}");
    }
}
