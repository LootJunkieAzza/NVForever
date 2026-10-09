// Depth-only prepass fragment for meshes in a crossfade (nif_material.rs `fade_prepass`):
// the dither of the main pass, then the alpha-mask discard Bevy's depth-only prepass
// fragment does.

#import bevy_pbr::{
    prepass_io::VertexOutput,
    pbr_prepass_functions::prepass_alpha_discard,
}
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

@fragment
fn fragment(in: VertexOutput) {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    prepass_alpha_discard(in);
}
