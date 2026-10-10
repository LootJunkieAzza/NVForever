// NIF surface material (nif_material.rs): Bevy's standard PBR fragment with four changes,
// all [hypothesis, docs/spec-notes/nif.md §6 "Emissive light", "Specular", "View-angle
// falloff", "Multiply blend"]:
// - the alpha is scaled by the view-angle falloff (blended effect shapes only);
// - the emissive light is modulated by the surface colour (diffuse texture × vertex
//   colour), as NIF emissive is;
// - ambient light is diffuse only: the specular part Bevy adds for its ambient light is
//   taken out again, since the game's ambient term is a flat colour and highlights come
//   from lights (and environment maps, not drawn yet);
// - a multiply-blended shape's colour is a factor on what lies behind it: summed in
//   display values, held to 0..1, and neither fogged nor tonemapped.

// Meshes in a crossfade (VisibilityRange; streamed cells fading in and out) are dithered
// first, as Bevy's own PBR fragment does.
//
// SOFT_EDGES (alpha-tested shapes in the main pass of a multisampled view): instead of
// the alpha test, the fragment's alpha is the share of the pixel's samples it covers
// (nif_material.rs `soft_edges`).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::alpha_discard,
}
#ifdef VISIBILITY_RANGE_DITHER
#import bevy_pbr::pbr_functions::visibility_range_dither
#endif

#ifdef PREPASS_PIPELINE
#import bevy_pbr::{
    prepass_io::{VertexOutput, FragmentOutput},
    pbr_deferred_functions::deferred_output,
}
#else
#import bevy_pbr::{
    forward_io::{VertexOutput, FragmentOutput},
    pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing, calculate_F0},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    lighting::{EnvBRDFApprox, F_AB},
    mesh_view_bindings::{lights, view},
}
#endif

struct NifSurface {
    // x start angle (cosine), y stop angle (cosine), z start opacity, w stop opacity;
    // (1, 0, 1, 1) when the shape has no falloff.
    falloff: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> surface: NifSurface;

// View-angle falloff of BSShaderNoLightingProperty (docs/spec-notes/nif.md §6): linear in
// the cosine from the start opacity at the start angle to the stop opacity at the stop
// angle, held beyond either end [hypothesis: linear and clamped].
fn falloff_opacity(cos_angle: f32, p: vec4<f32>) -> f32 {
    let t = clamp((cos_angle - p.y) / (p.x - p.y), 0.0, 1.0);
    return mix(p.w, p.z, t);
}

// The factor a multiply-blended shape puts on what lies behind it (blend factors ZERO and
// SRC_COLOR: result = destination × source colour), for the lit colour and the emissive
// light of the shape, both linear (CPU mirror: nif_material.rs `multiply_factor`). The
// game's shader output is taken as texture × (light + emissive) in display values, held
// to 0..1 when it is written [hypothesis], so the two parts are summed as display values
// (gamma 2.2) and the sum is limited before it goes back to linear.
fn multiply_factor(lit: vec3<f32>, emissive: vec3<f32>) -> vec3<f32> {
    let display = pow(max(lit, vec3(0.0)), vec3(1.0 / 2.2)) + pow(max(emissive, vec3(0.0)), vec3(1.0 / 2.2));
    return pow(saturate(display), vec3(2.2));
}

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // |N·V|: the normal is already flipped towards the viewer on back faces of double-sided
    // shapes; abs() also covers single-sided ones seen edge-on.
    let cos_angle = abs(dot(normalize(pbr_input.world_normal), pbr_input.V));
    pbr_input.material.base_color.a *= falloff_opacity(cos_angle, surface.falloff);
#ifdef SOFT_EDGES
    // Alpha-tested shape in a multisampled view (nif_material.rs `soft_edges`): the share
    // of the pixel's samples the shape covers, 1 at the cutoff and above, falling to 0
    // within a pixel outside it.
    let edge_alpha = pbr_input.material.base_color.a;
    let edge = saturate((edge_alpha - pbr_input.material.alpha_cutoff) / max(fwidth(edge_alpha), 0.0001) + 1.0);
    if edge <= 0.0 {
        discard;
    }
    pbr_input.material.base_color.a = 1.0;
#else
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
#endif
    // base_color already holds texture × vertex colour × material colour.
    let e = pbr_input.material.emissive;
    pbr_input.material.emissive = vec4(e.rgb * pbr_input.material.base_color.rgb, e.a);
#ifdef BLEND_MULTIPLY
    // Kept out of the lighting sum: multiply_factor adds it in display values.
    let multiply_emissive = pbr_input.material.emissive.rgb;
    pbr_input.material.emissive = vec4(vec3(0.0), e.a);
#endif

#ifdef PREPASS_PIPELINE
    let out = deferred_output(in, pbr_input);
#else
    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
        // Bevy's ambient specular, with the same inputs as in apply_pbr_lighting and
        // bevy_pbr::ambient::ambient_light (no lightmaps are used here).
        let m = pbr_input.material;
        let F0 = calculate_F0(m.base_color.rgb, m.metallic, m.reflectance);
        let NdotV = max(dot(pbr_input.N, pbr_input.V), 0.0001);
        let spec = EnvBRDFApprox(F0, F_AB(m.perceptual_roughness, NdotV));
        let spec_occlusion = saturate(dot(F0, vec3(50.0 * 0.33)));
        let ambient_spec = spec * spec_occlusion * lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
        out.color = vec4(max(out.color.rgb - view.exposure * ambient_spec, vec3(0.0)), out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
    }
#ifdef BLEND_MULTIPLY
    // The colour is a factor, not light: no fog and no tonemapping. Bevy's multiply blend
    // is destination × colour + destination × (1 − alpha), so the colour is premultiplied
    // as bevy_pbr::pbr_functions::premultiply_alpha does.
    let factor = multiply_factor(out.color.rgb, multiply_emissive);
    out.color = vec4(factor * out.color.a, out.color.a);
#else
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#ifdef SOFT_EDGES
    out.color.a = edge;
#endif
#endif
#endif
    return out;
}
