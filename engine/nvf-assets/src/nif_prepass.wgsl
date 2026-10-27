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
#ifdef FIRST_PERSON_DEPTH
#import nvf_assets::first_person_depth::squeeze
#endif
#ifdef MAY_DISCARD
#import bevy_pbr::{
    pbr_types,
    mesh_view_bindings::view,
}
#import nvf_assets::nif_surface_bindings::{nif_surface, standard_material, holes_alpha, base_color_texel}

// Bevy's cutoff for blended shapes in a depth-only pass (its PREMULTIPLIED_ALPHA_CUTOFF).
const BLENDED_CUTOFF: f32 = 0.05;
#endif

// A first-person shape (nif_material.rs "First-person depth") writes its depth squeezed
// in front of the world, as the main pass does.
@fragment
#ifdef FIRST_PERSON_DEPTH
fn fragment(in: VertexOutput) -> @builtin(frag_depth) f32 {
#else
fn fragment(in: VertexOutput) {
#endif
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
#ifdef MAY_DISCARD
    let standard = standard_material(in.instance_index);
    let surface = nif_surface(in.instance_index);
    let flags = standard.flags;
    var alpha = standard.base_color.a;
#ifdef VERTEX_COLORS
    alpha *= in.color.a;
#endif
#ifdef VERTEX_UVS_A
    if (flags & pbr_types::STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u {
        let uv = (standard.uv_transform * vec3(in.uv, 1.0)).xy;
        alpha *= base_color_texel(in.instance_index, uv, view.mip_bias).a;
    }
#endif
    // The cut of a blended shape that is also alpha-tested (0 on every other shape); the
    // view-angle falloff of the main pass is left out, since a shadow has no view angle.
    if alpha < surface.alpha_cut {
        discard;
    }
#ifdef VERTEX_UVS_A
    // The holes of the main pass (nif_material.rs "Holes").
    if surface.holes.x >= 0.0 && surface.holes.x < 255.0 {
        if holes_alpha(in.instance_index, in.uv) * 255.0 >= surface.holes.x {
            discard;
        }
    }
#endif
    let alpha_mode = flags & pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode == pbr_types::STANDARD_MATERIAL_FLAGS_ALPHA_MODE_MASK {
        if alpha < standard.alpha_cutoff {
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
#ifdef FIRST_PERSON_DEPTH
    return squeeze(in.position.z);
#endif
}
