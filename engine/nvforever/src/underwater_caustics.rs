//! Sunlit caustics on submerged receivers [design, #2674].
//! One depth-based pass, only while submerged outdoors with Water effects enabled.
//! A 25-bit mask bounds the loaded cell-water planes; placed water is not included.
//! Receivers beyond this 5x5 ring are omitted. Opaque depth cannot provide shadowing or
//! material identity: this is a restrained scene-color gain. Camera fog attenuates
//! only that added gain; the scene is not fogged a second time.
//!
//! Where in the frame [design]: without a temporal upscaler, after tonemapping and
//! before the game's TAA, which smooths the pattern with the rest of the picture
//! ([`caustics_pass`]). While FSR or DLSS upscales, at its render size on the HDR picture
//! before the upscaler instead ([`caustics_before_upscaler`]), the way the ambient
//! occlusion's picture placement does it (ssao.rs `ao_before_fsr`): the upscaler then
//! accumulates the gain with the scene it lights, and the pass costs the render size's
//! pixels, not the display's. After the upscaler the gain would be drawn unsmoothed every
//! frame, from a depth that was drawn jittered at the render size and expanded by nearest
//! texels (upscaler/prep.rs). `NVF_CAUSTICS_BEFORE_UPSCALER=0` keeps the post placement
//! under an upscaler too, for comparisons.

use bevy::core_pipeline::prepass::ViewPrepassTextures;
use bevy::core_pipeline::tonemapping::tonemapping;
use bevy::core_pipeline::{Core3d, Core3dSystems, FullscreenShader};
use bevy::pbr::{DistanceFog, FogFalloff};
use bevy::platform::collections::HashMap;
use bevy::prelude::*;
use bevy::render::diagnostic::RecordDiagnostics;
use bevy::render::extract_component::{ComponentUniforms, DynamicUniformIndex, ExtractComponent, ExtractComponentPlugin, UniformComponentPlugin};
use bevy::render::render_resource::binding_types::{sampler, texture_2d, texture_depth_2d, texture_depth_2d_multisampled, uniform_buffer};
use bevy::render::render_resource::*;
use bevy::render::renderer::{RenderContext, RenderDevice, ViewQuery};
use bevy::render::view::{ExtractedView, ViewTarget};
use bevy::render::{Render, RenderApp, RenderStartup, RenderSystems};

#[derive(Component, ExtractComponent, Clone, Copy, ShaderType)]
#[extract_app(bevy::render::RenderApp)]
struct UnderwaterCaustics {
    world_from_view: Mat4,
    // Perspective x/y scales, near plane, time.
    projection: Vec4,
    viewport: Vec4,
    sun: Vec4,
    // Surface height, minimum cell x/y, cell size.
    water: Vec4,
    // Effective camera fog near, far and alpha; written by underwater_fog in Update.
    fog: Vec4,
    // Matching-height, loaded water planes, one bit per cell (row-major).
    wet: UVec4,
}

/// Match the near-cell renderer (`world::spawn_water`), including No LOD Water
/// cells: that flag affects distant water only. This reads index metadata, not LAND.
fn cell_water_height(cell: &fnv_world::Cell, default_height: Option<f32>, default_type: Option<u32>) -> Option<f32> {
    cell.water_type.or(default_type)?;
    cell.water_surface(default_height)
}

/// Row-major lookup shared with the shader: floor game coordinates / CELL_SIZE
/// before subtracting the minimum cell, so negative coordinates keep their cells.
fn wet_index(cell: (i32, i32), origin: (i32, i32)) -> Option<usize> {
    let x = cell.0 - origin.0;
    let y = cell.1 - origin.1;
    ((0..5).contains(&x) && (0..5).contains(&y)).then_some((y * 5 + x) as usize)
}

/// Whether the gain goes into the HDR picture before a temporal upscaler while one upscales
/// ([`caustics_before_upscaler`]); `NVF_CAUSTICS_BEFORE_UPSCALER=0` keeps it after
/// tonemapping at the display size, as builds before this change did, for comparisons. Read once.
static BEFORE_UPSCALER: std::sync::LazyLock<bool> =
    std::sync::LazyLock::new(|| !std::env::var("NVF_CAUSTICS_BEFORE_UPSCALER").is_ok_and(|v| v.trim() == "0"));

pub struct UnderwaterCausticsPlugin;
impl Plugin for UnderwaterCausticsPlugin {
    fn build(&self, app: &mut App) {
        bevy::asset::embedded_asset!(app, "underwater_caustics.wesl");
        app.add_plugins((ExtractComponentPlugin::<UnderwaterCaustics>::default(), UniformComponentPlugin::<UnderwaterCaustics>::default()))
            // WaterPlugin updates underwater_fog in Update. PostUpdate therefore
            // captures this frame's effective fog, including the depth-density scale.
            .add_systems(PostUpdate, follow_camera.after(bevy::camera::CameraUpdateSystems).after(bevy::transform::TransformSystems::Propagate));
        let Some(render_app) = app.get_sub_app_mut(RenderApp) else { return };
        // Ordered against every other pass that flips the view target after tonemapping
        // (`post_process_write`). Core3d runs on the multi-threaded executor and these
        // passes conflict in no declared access, so two unordered ones may record on two
        // threads at once: each flip is taken when its system runs, but the command
        // buffers are submitted in the schedule's topological order. When the two orders
        // differ, one pass reads the texture the other has not written yet and the frame
        // shows a stale picture: the occlusion's `ao_pass` (no upscaler) and `taa_pass`
        // were unordered with this pass. Before the temporal resolve, so it smooths the
        // pattern's jitter along with the rest of the picture.
        render_app
            .add_systems(RenderStartup, init)
            .add_systems(Render, prepare.in_set(RenderSystems::PrepareResources))
            .add_systems(
                Core3d,
                caustics_pass
                    .in_set(Core3dSystems::PostProcess)
                    .after(tonemapping)
                    .after(crate::ssao::ao_pass)
                    .before(crate::taa::taa_pass)
                    .before(bevy::anti_alias::fxaa::fxaa)
                    .before(bevy::anti_alias::smaa::smaa)
                    .before(crate::imagespace::blur_pass),
            )
            // After the occlusion's multiply into the same picture, for the same reason as
            // above: both load and store the view's main texture.
            .add_systems(
                Core3d,
                caustics_before_upscaler
                    .in_set(Core3dSystems::EarlyPostProcess)
                    .after(crate::ssao::ao_before_fsr)
                    .before(crate::fsr::fsr_pass)
                    .before(crate::dlss::dlss_pass),
            );
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn follow_camera(
    mut commands: Commands,
    graphics: Option<Res<crate::graphics::Graphics>>,
    world: Option<Res<crate::world::WorldData>>,
    streamer: Option<Res<crate::world::Streamer>>,
    sky: Option<Res<crate::sky::SkyState>>,
    time: Res<Time>,
    cameras: Query<(Entity, &GlobalTransform, &Projection, &Camera, Option<&DistanceFog>, Has<UnderwaterCaustics>), crate::fly::WorldCamera>,
) {
    for (entity, transform, projection, camera_view, camera_fog, active) in &cameras {
        // Err: why the pass is off, said when it goes off (a camera bobbing at the
        // surface, a cell streaming in or out shows as lines in a row).
        let data = (|| -> Result<UnderwaterCaustics, &'static str> {
            let (Some(graphics), Some(world), Some(streamer), Some(sky)) = (graphics.as_deref(), world.as_deref(), streamer.as_deref(), sky.as_deref()) else {
                return Err("no world");
            };
            if !graphics.water_effects {
                return Err("water effects off");
            }
            if streamer.interior.is_some() {
                return Err("interior");
            }
            let sun = crate::water::caustic_sun(sky);
            if sun.w <= 0.0 {
                return Err("no sun");
            }
            let camera = crate::world::bevy_to_game(transform.translation());
            let cell = fnv_world::cell_of(camera.x, camera.y);
            if !streamer.loaded.contains_key(&cell) {
                return Err("the camera's cell is not loaded");
            }
            let ws = world.index.worldspaces.get(&world.world).ok_or("no worldspace")?;
            let height = world.grid_cell(cell).and_then(|c| cell_water_height(c, ws.default_water_height, ws.water)).ok_or("no cell water")?;
            if camera.z >= height {
                return Err("above the cell's water");
            }
            let Projection::Perspective(p) = projection else {
                return Err("not a perspective view");
            };
            let viewport = camera_view.physical_viewport_rect().ok_or("no viewport")?;
            let fog = camera_fog.ok_or("no fog")?;
            // Underwater fog is linear. Decline other fog modes instead of treating
            // their coefficients as a linear range during a camera transition.
            let FogFalloff::Linear { start, end } = &fog.falloff else { return Err("the fog is not the under-water fog") };
            let fog = Vec4::new(*start, *end, fog.color.to_linear().alpha, 0.0);
            let y = 1.0 / (p.fov * 0.5).tan();
            let origin = (cell.0 - 2, cell.1 - 2);
            // Only index lookups: even on cell boundaries this performs no LAND decode.
            // Rebuild the tiny mask so cells loaded/unloaded during streaming apply now.
            let mut wet = 0u32;
            for dy in 0..5 {
                for dx in 0..5 {
                    let c = (origin.0 + dx, origin.1 + dy);
                    if !streamer.loaded.contains_key(&c) {
                        continue;
                    }
                    let Some(h) = world.grid_cell(c).and_then(|c| cell_water_height(c, ws.default_water_height, ws.water)) else { continue };
                    if (h - height).abs() > 0.5 {
                        continue;
                    }
                    let index = wet_index(c, origin).expect("cell belongs to the 5x5 ring");
                    wet |= 1u32 << index;
                }
            }
            Ok(UnderwaterCaustics {
                world_from_view: transform.to_matrix(),
                // Exactly the clock used by GlobalsUniform (`prepare_globals_buffer`),
                // including its wrap period, so surface and submerged patterns agree.
                projection: Vec4::new(y / p.aspect_ratio.max(1e-3), y, p.near, time.elapsed_secs_wrapped()),
                viewport: Vec4::new(viewport.min.x as f32, viewport.min.y as f32, viewport.width() as f32, viewport.height() as f32),
                sun,
                water: Vec4::new(height, origin.0 as f32, origin.1 as f32, fnv_world::CELL_SIZE),
                fog,
                wet: UVec4::new(wet, 0, 0, 0),
            })
        })();
        match data {
            Ok(data) => {
                if !active {
                    info!("underwater caustics: active outdoors below the water surface");
                }
                commands.entity(entity).insert(data);
            }
            Err(reason) if active => {
                info!("underwater caustics: off ({reason})");
                commands.entity(entity).remove::<UnderwaterCaustics>();
            }
            Err(_) => {}
        }
    }
}

#[derive(Resource)]
struct Pipelines {
    layouts: [BindGroupLayoutDescriptor; 2],
    /// Before the upscaler: the single-sampled depth and the uniform (no picture read).
    multiply_layout: BindGroupLayoutDescriptor,
    base: RenderPipelineDescriptor,
    sampler: Sampler,
    /// By the picture's format, multisampling, and whether it is the before-upscaler variant.
    variants: HashMap<(TextureFormat, bool, bool), CachedRenderPipelineId>,
}
#[derive(Component)]
struct CausticsView {
    pipeline: CachedRenderPipelineId,
    format: TextureFormat,
    multi: bool,
    /// The variant that multiplies the gain into the HDR picture before the upscaler, while
    /// one upscales this view ([`caustics_before_upscaler`]).
    before_upscaler: Option<CachedRenderPipelineId>,
}

fn init(mut commands: Commands, device: Res<RenderDevice>, assets: Res<AssetServer>, fullscreen: Res<FullscreenShader>) {
    let layout = |multi| {
        BindGroupLayoutDescriptor::new(
            "underwater_caustics_layout",
            &BindGroupLayoutEntries::sequential(
                ShaderStages::FRAGMENT,
                (
                    if multi { texture_depth_2d_multisampled() } else { texture_depth_2d() },
                    uniform_buffer::<UnderwaterCaustics>(true),
                    texture_2d(TextureSampleType::Float { filterable: true }),
                    sampler(SamplerBindingType::Filtering),
                ),
            ),
        )
    };
    let multiply_layout = BindGroupLayoutDescriptor::new(
        "underwater_caustics_multiply_layout",
        &BindGroupLayoutEntries::sequential(ShaderStages::FRAGMENT, (texture_depth_2d(), uniform_buffer::<UnderwaterCaustics>(true))),
    );
    commands.insert_resource(Pipelines {
        layouts: [layout(false), layout(true)],
        multiply_layout,
        base: RenderPipelineDescriptor {
            label: Some("underwater_caustics_pipeline".into()),
            vertex: fullscreen.to_vertex_state(),
            fragment: Some(FragmentState {
                shader: assets.load("embedded://nvforever/underwater_caustics.wesl"),
                entry_point: Some("fragment".into()),
                ..default()
            }),
            ..default()
        },
        sampler: device.create_sampler(&SamplerDescriptor { mag_filter: FilterMode::Linear, min_filter: FilterMode::Linear, ..default() }),
        variants: HashMap::default(),
    });
}
/// The pipeline of a variant, queued the first time it is asked for.
fn variant(pipelines: &mut Pipelines, cache: &PipelineCache, format: TextureFormat, multi: bool, before_upscaler: bool) -> CachedRenderPipelineId {
    let key = (format, multi, before_upscaler);
    if let Some(id) = pipelines.variants.get(&key) {
        return *id;
    }
    let mut descriptor = pipelines.base.clone();
    descriptor.layout = vec![if before_upscaler { pipelines.multiply_layout.clone() } else { pipelines.layouts[multi as usize].clone() }];
    if let Some(fragment) = descriptor.fragment.as_mut() {
        // Before the upscaler the shader returns the factor and the blend multiplies the
        // picture by it; the picture's alpha is kept.
        let blend = before_upscaler.then_some(BlendState {
            color: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::Src, operation: BlendOperation::Add },
            alpha: BlendComponent { src_factor: BlendFactor::Zero, dst_factor: BlendFactor::One, operation: BlendOperation::Add },
        });
        fragment.targets = vec![Some(ColorTargetState { format, blend, write_mask: ColorWrites::ALL })];
        if multi {
            fragment.shader_defs.push("MULTISAMPLED".into());
        }
        if before_upscaler {
            fragment.shader_defs.push("BEFORE_UPSCALER".into());
            fragment.entry_point = Some("multiply".into());
        }
    }
    let id = cache.queue_render_pipeline(descriptor);
    pipelines.variants.insert(key, id);
    id
}

#[allow(clippy::type_complexity)]
fn prepare(
    mut commands: Commands,
    cache: Res<PipelineCache>,
    mut pipelines: ResMut<Pipelines>,
    views: Query<(Entity, &ExtractedView, &Msaa, Has<crate::upscaler::prep::PreparedUpscale>), With<UnderwaterCaustics>>,
    mut placed_before: Local<HashMap<Entity, bool>>,
) {
    placed_before.retain(|entity, _| views.contains(*entity));
    for (entity, view, msaa, upscaled) in &views {
        let format = view.target_format;
        let multi = msaa.samples() > 1;
        let pipeline = variant(&mut pipelines, &cache, format, multi, false);
        // An upscaled view is single-sampled and HDR (upscaler/prep.rs `prepare`).
        let before = *BEFORE_UPSCALER && upscaled && !multi;
        let before_upscaler = before.then(|| variant(&mut pipelines, &cache, format, false, true));
        if placed_before.insert(entity, before) != Some(before) {
            if before {
                info!("underwater caustics: on the HDR picture at the render size, before the upscaler");
            } else if upscaled {
                info!("underwater caustics: after tonemapping at the display size, after the upscaler");
            }
        }
        commands.entity(entity).insert(CausticsView { pipeline, format, multi, before_upscaler });
    }
}

/// While a temporal upscaler upscales: the gain worked out at its render size from the depth
/// as drawn (before the upscaler's common passes expand it) and multiplied into the
/// render-size rectangle of the HDR picture the upscaler reads, and, under FSR, of FSR's
/// opaque copy, so its reactive masks compare the two as before (ssao.rs `ao_before_fsr`
/// does the same). The upscaler then accumulates the pattern with the scene: jittered with
/// it and smoothed by it. It brightens the HDR light before tonemapping.
#[allow(clippy::type_complexity)]
fn caustics_before_upscaler(
    view: ViewQuery<(
        &ViewTarget,
        &ViewPrepassTextures,
        &CausticsView,
        &DynamicUniformIndex<UnderwaterCaustics>,
        &crate::upscaler::prep::PreparedUpscale,
        Option<&crate::fsr::PreparedFsr>,
    )>,
    pipelines: Res<Pipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<UnderwaterCaustics>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepass, prepared, index, upscale, fsr) = view.into_inner();
    let Some(pipeline) = prepared.before_upscaler else { return };
    if !crate::ssao::format_matches(target, prepared.format) || crate::ssao::depth_multisampled(prepass) != Some(false) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let Some(pipeline) = cache.get_render_pipeline(pipeline) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let layout = cache.get_bind_group_layout(&pipelines.multiply_layout);
    let group = device.create_bind_group("underwater_caustics_multiply_group", &layout, &BindGroupEntries::sequential((depth, binding)));
    // FSR's opaque copy, only where FSR draws: DLSS has none.
    let opaque = fsr.map(|fsr| fsr.opaque().create_view(&default()));
    let picture: &wgpu::TextureView = target.main_texture_view();
    let size = upscale.render_size.as_vec2();
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_underwater_caustics");
    // Loaded, not cleared (wgpu's default operations clear): the blend multiplies what is there.
    let ops = Operations { load: LoadOp::Load, store: StoreOp::Store };
    let targets = std::iter::once(("underwater_caustics_picture", picture)).chain(opaque.as_ref().map(|opaque| ("underwater_caustics_fsr_opaque", opaque)));
    for (label, view) in targets {
        let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(RenderPassColorAttachment { view, depth_slice: None, resolve_target: None, ops })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_viewport(0.0, 0.0, size.x, size.y, 0.0, 1.0);
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &group, &[index.index()]);
        pass.draw(0..3, 0..1);
    }
    span.end(ctx.command_encoder());
}
#[allow(clippy::type_complexity)]
fn caustics_pass(
    view: ViewQuery<(&ViewTarget, &ViewPrepassTextures, &CausticsView, &DynamicUniformIndex<UnderwaterCaustics>, &UnderwaterCaustics)>,
    pipelines: Res<Pipelines>,
    cache: Res<PipelineCache>,
    uniforms: Res<ComponentUniforms<UnderwaterCaustics>>,
    device: Res<RenderDevice>,
    mut ctx: RenderContext,
) {
    let (target, prepass, prepared, index, _) = view.into_inner();
    // Applied before the upscaler already ([`caustics_before_upscaler`]).
    if prepared.before_upscaler.is_some() {
        return;
    }
    if !crate::ssao::format_matches(target, prepared.format) || crate::ssao::depth_multisampled(prepass) != Some(prepared.multi) {
        return;
    }
    let Some(depth) = prepass.depth_only_view() else { return };
    let Some(pipeline) = cache.get_render_pipeline(prepared.pipeline) else { return };
    let Some(binding) = uniforms.uniforms().binding() else { return };
    let post = target.post_process_write();
    let layout = cache.get_bind_group_layout(&pipelines.layouts[prepared.multi as usize]);
    let group =
        device.create_bind_group("underwater_caustics_group", &layout, &BindGroupEntries::sequential((depth, binding, post.source, &pipelines.sampler)));
    let diagnostics = ctx.diagnostic_recorder();
    let diagnostics = diagnostics.as_deref();
    let span = diagnostics.time_span(ctx.command_encoder(), "nvf_underwater_caustics");
    let mut pass = ctx.command_encoder().begin_render_pass(&RenderPassDescriptor {
        label: Some("underwater_caustics"),
        color_attachments: &[Some(RenderPassColorAttachment { view: post.destination, depth_slice: None, resolve_target: None, ops: Operations::default() })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_pipeline(pipeline);
    pass.set_bind_group(0, &group, &[index.index()]);
    pass.draw(0..3, 0..1);
    drop(pass);
    span.end(ctx.command_encoder());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receiver_cells_use_floor_including_negative_boundaries() {
        let origin = (-2, -3);
        for (position, expected) in [
            (Vec2::new(-0.01, -0.01), Some(11)),
            (Vec2::ZERO, Some(17)),
            (Vec2::new(-8192.0, -12288.0), Some(0)),
            (Vec2::new(-8192.01, -12288.0), None),
            (Vec2::new(12288.0, 0.0), None),
            (Vec2::new(0.0, 8192.0), None),
        ] {
            let cell = fnv_world::cell_of(position.x, position.y);
            assert_eq!(wet_index(cell, origin), expected, "{position:?}");
        }
    }

    #[test]
    fn near_water_includes_no_lod_cells_and_world_defaults() {
        let mut cell = fnv_world::Cell { data_flags: 2 | 8, water_height: Some(f32::MAX), ..default() };
        assert_eq!(cell_water_height(&cell, Some(50.0), Some(1)), Some(50.0));
        cell.water_height = Some(75.0);
        cell.water_type = Some(2);
        assert_eq!(cell_water_height(&cell, Some(50.0), None), Some(75.0));
        cell.data_flags = 8;
        assert_eq!(cell_water_height(&cell, Some(50.0), Some(1)), None);
        cell.data_flags = 2;
        cell.water_type = None;
        assert_eq!(cell_water_height(&cell, Some(50.0), None), None);
    }

    /// [design] The shader composes and validates (naga) in every variant `variant` builds,
    /// and each binds only what its layout has: the post variants the depth, the uniform,
    /// the picture and its sampler; the before-upscaler variant the single-sampled depth and
    /// the uniform alone, its entry point `multiply`.
    #[test]
    fn every_variant_composes_with_the_bindings_of_its_layout() {
        use crate::mesh_blend_shader_tests::{compose, texture_bindings, vendored_modules};
        use bevy::shader::ShaderDefVal;
        let mut modules = vendored_modules();
        for (name, source) in [
            ("nvforever/underwater_caustics", include_str!("underwater_caustics.wesl")),
            ("nvforever/caustics", include_str!("caustics.wesl")),
            ("nvf_assets/first_person_depth", include_str!("../../nvf-assets/src/first_person_depth.wesl")),
        ] {
            modules.push((format!("embedded://{name}.wesl"), source.into()));
        }
        for (multi, before) in [(false, false), (true, false), (false, true)] {
            let defs = vec![ShaderDefVal::Bool("MULTISAMPLED".into(), multi), ShaderDefVal::Bool("BEFORE_UPSCALER".into(), before)];
            let source = compose(&modules, "embedded://nvforever/underwater_caustics.wesl", &defs);
            let module = wgpu::naga::front::wgsl::parse_str(&source).unwrap();
            let entries: Vec<_> = module.entry_points.iter().map(|e| e.name.as_str()).collect();
            assert_eq!(entries, [if before { "multiply" } else { "fragment" }], "multi={multi} before={before}");
            let bound: std::collections::BTreeSet<_> = module.global_variables.iter().filter_map(|(_, v)| v.binding.as_ref().map(|b| (b.group, b.binding))).collect();
            let expected: std::collections::BTreeSet<_> = if before { [(0, 0), (0, 1)].into() } else { [(0, 0), (0, 1), (0, 2), (0, 3)].into() };
            assert_eq!(bound, expected, "multi={multi} before={before}");
            assert_eq!(texture_bindings(&source).len(), if before { 1 } else { 2 }, "multi={multi} before={before}");
        }
    }

    #[test]
    fn row_major_bits_cover_the_full_ring_without_aliasing() {
        let mut mask = 0u32;
        for y in -3..2 {
            for x in -2..3 {
                mask |= 1 << wet_index((x, y), (-2, -3)).unwrap();
            }
        }
        assert_eq!(mask, (1 << 25) - 1);
    }
}
