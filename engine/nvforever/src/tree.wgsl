// SpeedTree trees (tree.rs): vertex shader for the bark (TREE_BARK) and the leaf cards
// (TREE_LEAVES), used for the main pass and the prepass (depth, shadows) so both see the
// same motion.
//
// Leaf cards: each card's four vertices sit at the leaf's position; UV_1 holds the
// corner's offset in the card's plane (local units, x right, y up). The card is spread
// in the plane of the view, so it always faces the camera (and, in the shadow pass, the
// light).
//
// Distant tree billboards (TREE_BILLBOARD, treelod.rs): every vertex of a card sits at
// the tree's foot; UV_1 holds the corner's offset (x right, y up, game units). The card
// stands upright and turns about the vertical towards the camera (the view position, so
// each tree faces it on its own). The wind uniform is zero for them.
//
// Wind: the whole tree rocks along the wind, more the higher a point is; leaf cards also
// rustle (flutter and tilt), each with its own phase. Speeds come from the TREE record's
// rock and rustle speeds and strength from the weather's wind speed; amplitudes and the
// mapping to frequencies are chosen by eye (speedtree.md §6).

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
#import bevy_pbr::forward_io::{Vertex, VertexOutput}
#import bevy_pbr::mesh_view_bindings::globals
#endif

struct TreeWind {
    // xyz: wind direction (world, horizontal, unit); w: strength 0-1.
    wind: vec4<f32>,
    // x: unused; y: rock angular speed; z: rustle angular speed (rad/s); w: tree
    // height in local units.
    motion: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> tree_wind: TreeWind;

// How far the top of the tree moves along the wind, as a fraction of its height, at
// full strength [tuned by eye].
const ROCK: f32 = 0.25;
// Leaf flutter along the wind (fraction of the card size) and tilt (radians) at full
// strength [tuned by eye].
const FLUTTER: f32 = 0.08;
const TILT: f32 = 0.35;

// World-space offset of a point `local` (game space, z up) of the tree placed by
// `world_from_local`.
fn rock(world_from_local: mat4x4<f32>, local: vec3<f32>) -> vec3<f32> {
    let origin = world_from_local[3].xyz;
    let scale = length(world_from_local[0].xyz);
    let height = max(tree_wind.motion.w, 1e-4);
    let h = clamp(local.z / height, 0.0, 1.5);
    // Each tree has its own phase, from where it stands.
    let phase = dot(origin.xz, vec2<f32>(0.0131, 0.0173));
    // Seconds since start-up (wraps after an hour: one jump in the motion per hour).
    let t = globals.time * tree_wind.motion.y;
    // Leaning with the wind, plus two unrelated swings.
    let swing = 0.5 + 0.35 * sin(t + phase) + 0.15 * sin(2.31 * t + 1.7 * phase);
    return tree_wind.wind.xyz * (tree_wind.wind.w * ROCK * h * h * height * scale * swing);
}

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let base = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;
    var world = base + rock(world_from_local, vertex.position);

#ifdef TREE_BILLBOARD
#ifdef VERTEX_UVS_B
    // Bevy space: y is up. Right = up x (towards the camera, flattened).
    let to_cam = view.world_position.xyz - base;
    var flat = vec2<f32>(to_cam.x, to_cam.z);
    var right = vec3<f32>(1.0, 0.0, 0.0);
    if (dot(flat, flat) > 1e-6) {
        flat = normalize(flat);
        right = vec3<f32>(flat.y, 0.0, -flat.x);
    }
    world += right * vertex.uv_b.x + vec3<f32>(0.0, vertex.uv_b.y, 0.0);
    out.uv_b = vertex.uv_b;
#endif
#endif

#ifdef TREE_LEAVES
#ifdef VERTEX_UVS_B
    // Rustle: each card (all four corners share its position) gets its own phase.
    let s = length(world_from_local[0].xyz);
    let phase = dot(vertex.position, vec3<f32>(311.7, 173.3, 237.1));
    let r = sin(globals.time * tree_wind.motion.z + phase);
    let a = tree_wind.wind.w * TILT * r;
    let c = cos(a);
    let sn = sin(a);
    let corner = vec2<f32>(c * vertex.uv_b.x - sn * vertex.uv_b.y, sn * vertex.uv_b.x + c * vertex.uv_b.y);
    let size = length(vertex.uv_b);
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    world += (right * corner.x + up * corner.y) * s;
    world += tree_wind.wind.xyz * (tree_wind.wind.w * FLUTTER * size * s * r);
    out.uv_b = vertex.uv_b;
#endif
#endif

    out.world_position = vec4<f32>(world, 1.0);
    out.position = position_world_to_clip(world);
#ifdef VERTEX_UVS_A
    out.uv = vertex.uv;
#endif
#ifdef TREE_BARK
#ifdef VERTEX_UVS_B
    out.uv_b = vertex.uv_b;
#endif
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
