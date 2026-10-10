// Grass (grass.rs): vertex shader for the main pass and the prepass (depth), so both see
// the same motion, and the main pass's fragment shader.
//
// A grass mesh holds every plant of one type in a quarter of a cell. UV_1 holds, per
// vertex, the sway weight (the model's vertex alpha: 0 at the foot, more towards the
// tips) and the height above the plant's foot (game units).
//
// Wind: a vertex moves along the wind by its sway weight × the wind's magnitude, in waves
// that travel across the ground, so neighbouring plants move nearly together and distant
// ones do not. The wave speed comes from the grass type's wave period and the magnitude
// from the weather's wind speed (grass.md §5).
//
// Distance: beyond the grass distance the plants sink into the ground, each vertex by its
// own height (so the plant flattens) and a little more, over the fade range. The game
// fades its grass out over that range; how it does so is not known (grass.md §5).
//
// Edges (fragment, GRASS_COVERAGE: the view is multisampled): the texture's alpha is cut
// at the model's alpha-test threshold, as in the depth prepass, but texels below the
// threshold are not dropped: they cover a share of the pixel's samples, alpha / threshold
// (alpha to coverage). Strands end softly instead of in hard steps, and thin strands
// whose alpha the smaller mip levels have averaged down stay visible at a distance. The
// share is 1 wherever the prepass wrote depth, so nothing behind is wrongly hidden.
// Without multisampling the alpha is tested as before.
//
// Light (fragment): lit with the ground's normal (grass.rs), but with the ambient light
// and the sun weighted by `light` (grass.md §5).

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#import bevy_render::globals::Globals
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
// The prepass view bind group holds the globals at binding 1 (the main pass at 11).
@group(0) @binding(1) var<uniform> globals: Globals;
#else
#import bevy_pbr::forward_io::{Vertex, VertexOutput, FragmentOutput}
#import bevy_pbr::mesh_view_bindings::globals
#import bevy_pbr::pbr_fragment::pbr_input_from_standard_material
#import bevy_pbr::pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing}
#import bevy_pbr::lighting::{EnvBRDFApprox, F_AB}
#import bevy_pbr::mesh_view_bindings::lights
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif
#endif

struct GrassParams {
    // xyz: wind direction (world, horizontal, unit); w: how far a vertex with sway
    // weight 1 moves (game units).
    wind: vec4<f32>,
    // x: wave angular speed (rad/s); y: distance at which the grass starts to go; z:
    // distance over which it goes; w: unused.
    motion: vec4<f32>,
    // x: factor on the ambient light; y: factor on every other light (the sun).
    light: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> grass: GrassParams;

// How far below the ground a plant's foot ends up once it is gone (game units).
const SINK: f32 = 24.0;
// Wave length of the wind's waves along and across the wind (game units) [chosen].
const WAVE_ALONG: f32 = 900.0;
const WAVE_ACROSS: f32 = 2300.0;

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    var world = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;

#ifdef VERTEX_UVS_B
    // Bevy space: y is up, the ground is the xz plane.
    let along = grass.wind.xyz;
    let across = vec3<f32>(-along.z, 0.0, along.x);
    let phase = 6.2831853 * (dot(world, along) / WAVE_ALONG + dot(world, across) / WAVE_ACROSS);
    // Seconds since start-up (wraps after an hour: one jump in the motion per hour).
    let t = globals.time * grass.motion.x;
    // Leaning with the wind, a main wave and a smaller, unrelated one.
    let swing = 0.5 + 0.35 * sin(t - phase) + 0.15 * sin(2.31 * t - 1.7 * phase);
    world += along * (grass.wind.w * vertex.uv_b.x * swing);

    let gone = clamp((distance(view.world_position.xyz, world) - grass.motion.y) / grass.motion.z, 0.0, 1.0);
    world.y -= gone * (vertex.uv_b.y + SINK);
    out.uv_b = vertex.uv_b;
#endif

    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif

#ifdef PREPASS_PIPELINE
#ifdef UNCLIPPED_DEPTH_ORTHO_EMULATION
    out.unclipped_depth = out.position.z;
    out.position.z = min(out.position.z, 1.0);
#endif
#ifdef NORMAL_PREPASS_OR_DEFERRED_PREPASS
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#else
    out.world_normal = view.world_from_view[2].xyz;
#endif
#endif
#ifdef MOTION_VECTOR_PREPASS
    out.previous_world_position = out.world_position;
#endif
#else
#ifdef VERTEX_NORMALS
    out.world_normal = mesh_functions::mesh_normal_local_to_world(vertex.normal, vertex.instance_index);
#endif
#endif

#ifdef VERTEX_COLORS
    out.color = vertex.color;
#endif
#ifdef VERTEX_OUTPUT_INSTANCE_INDEX
    out.instance_index = vertex.instance_index;
#endif
#ifdef VISIBILITY_RANGE_DITHER
    out.visibility_range_dither = mesh_functions::get_visibility_range_dither_level(
        vertex.instance_index, world_from_local[3]);
#endif
    return out;
}

#ifndef PREPASS_PIPELINE
@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let cutoff = max(pbr_input.material.alpha_cutoff, 0.01);
    let alpha = pbr_input.material.base_color.a;
#ifdef GRASS_COVERAGE
    let coverage = saturate(alpha / cutoff);
    if coverage < 0.03 {
        discard;
    }
#else
    let coverage = 1.0;
    if alpha < cutoff {
        discard;
    }
#endif
    pbr_input.material.base_color.a = 1.0;
    var out: FragmentOutput;
    // Grass weighs the ambient light and the sun differently from other surfaces
    // (grass.md §5): the ambient part of Bevy's result, as bevy_pbr::ambient adds it for a
    // surface without specular reflection, is taken out and both parts are scaled.
    let lit = apply_pbr_lighting(pbr_input).rgb;
    let n_dot_v = max(dot(pbr_input.N, pbr_input.V), 0.0001);
    let ambient = EnvBRDFApprox(pbr_input.material.base_color.rgb, F_AB(1.0, n_dot_v)) * lights.ambient_color.rgb * view.exposure;
    let direct = max(lit - ambient, vec3(0.0));
    out.color = vec4(grass.light.x * ambient + grass.light.y * direct, 1.0);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    out.color.a = coverage;
    return out;
}
#endif
