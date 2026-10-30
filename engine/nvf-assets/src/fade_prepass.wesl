// Depth-only prepass fragment for meshes in a crossfade (nif_material.rs `fade_prepass`):
// the dither of the main pass, then the alpha-mask discard Bevy's depth-only prepass
// fragment does.

import bevy_pbr::{
    prepass::io::VertexOutput,
    render::pbr_prepass_functions::prepass_sample_color_and_alpha_discard,
};
@if(VISIBILITY_RANGE_DITHER) import bevy_pbr::render::pbr_functions::visibility_range_dither;

@fragment
fn fragment(in: VertexOutput) {
    @if(VISIBILITY_RANGE_DITHER) visibility_range_dither(in.position, in.visibility_range_dither);
    prepass_sample_color_and_alpha_discard(in);
}
