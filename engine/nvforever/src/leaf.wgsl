// SpeedTree leaf cards (tree.rs): each card's four vertices sit at the leaf's position;
// UV_1 holds the corner's offset in the card's plane (local units, x right, y up). The
// card is spread in the plane of the view, so it always faces the camera (and, in the
// shadow pass, the light). Used for the main pass and the prepass (depth, shadows).

#import bevy_pbr::{
    mesh_functions,
    mesh_view_bindings::view,
    view_transformations::position_world_to_clip,
}
#ifdef PREPASS_PIPELINE
#import bevy_pbr::prepass_io::{Vertex, VertexOutput}
#else
#import bevy_pbr::forward_io::{Vertex, VertexOutput}
#endif

@vertex
fn vertex(vertex: Vertex) -> VertexOutput {
    var out: VertexOutput;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let centre = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;
    // Uniform scale of the reference (and of the tree's size).
    let s = length(world_from_local[0].xyz);
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
#ifdef VERTEX_UVS_B
    let world = centre + (right * vertex.uv_b.x + up * vertex.uv_b.y) * s;
    out.uv_b = vertex.uv_b;
#else
    let world = centre;
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
