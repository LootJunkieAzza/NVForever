// Depth-only prepass fragment of NIF shapes (nif_material.rs `nif_prepass`; the shadow
// passes use it too): the dither of a crossfade, then the alpha test on the alpha the
// main pass tests. Bevy's own prepass fragment tests material alpha × texture alpha and
// leaves the vertex alpha out, which the main pass multiplies in: a shape whose vertex
// alpha falls below its cutoff then held depth where the main pass drew nothing, and
// the clear colour showed through whatever lay behind it (docs/spec-notes/nif.md §6
// "Alpha", "Vertex alpha and the test").

#import bevy_pbr::{
    prepass_io::VertexOutput,
    pbr_prepass_functions::prepass_alpha_discard,
}
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif
#ifdef MAY_DISCARD
#import bevy_pbr::{
    pbr_bindings,
    pbr_types,
    mesh_view_bindings::view,
}

// As in nif_material.wgsl.
struct NifSurface {
    falloff: vec4<f32>,
    alpha_cut: f32,
    add_whole: f32,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> surface: NifSurface;

// Bevy's cutoff for blended shapes in a depth-only pass (its PREMULTIPLIED_ALPHA_CUTOFF).
const BLENDED_CUTOFF: f32 = 0.05;
#endif

@fragment
fn fragment(in: VertexOutput) {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
#ifdef MAY_DISCARD
    let flags = pbr_bindings::material.flags;
    var alpha = pbr_bindings::material.base_color.a;
#ifdef VERTEX_COLORS
    alpha *= in.color.a;
#endif
#ifdef VERTEX_UVS_A
    if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u {
        let uv = (pbr_bindings::material.uv_transform * vec3(in.uv, 1.0)).xy;
        alpha *= textureSampleBias(pbr_bindings::base_color_texture, pbr_bindings::base_color_sampler, uv, view.mip_bias).a;
    }
#endif
    // The cut of a blended shape that is also alpha-tested (0 on every other shape); the
    // view-angle falloff of the main pass is left out, since a shadow has no view angle.
    if alpha < surface.alpha_cut {
        discard;
    }
    let alpha_mode = flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK {
        if alpha < pbr_bindings::material.alpha_cutoff {
            discard;
        }
    } else if alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND
        || alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD
        || alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ALPHA_TO_COVERAGE {
        if alpha < BLENDED_CUTOFF {
            discard;
        }
    } else {
        // Premultiplied and multiply shapes: as Bevy tests them.
        prepass_alpha_discard(in);
    }
#endif
}
