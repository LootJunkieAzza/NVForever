// NIF surface material (nif_material.rs): Bevy's standard PBR fragment with six changes,
// all [hypothesis, docs/spec-notes/nif.md §6 "Emissive light", "Specular", "View-angle
// falloff", "Multiply blend", "Additive blend", "Alpha"]:
// - the alpha is scaled by the view-angle falloff (blended effect shapes only);
// - a blended shape that is also alpha-tested drops the fragments below its cut;
// - the emissive light is modulated by the surface colour (diffuse texture × vertex
//   colour), as NIF emissive is;
// - ambient light is diffuse only: the specular part Bevy adds for its ambient light is
//   taken out again, since the game's ambient term is a flat colour and highlights come
//   from lights (and environment maps, not drawn yet);
// - a multiply-blended shape's colour is a factor on what lies behind it: summed in
//   display values, held to 0..1, and neither fogged nor tonemapped;
// - an additive shape adds about what it would add to display values: its alpha is
//   raised to a power (`add_alpha`).

// Meshes in a crossfade (VisibilityRange; streamed cells fading in and out) are dithered
// first, as Bevy's own PBR fragment does.
//
// SHADE_LIGHT (shapes with NifSurface::shade_light, the player's first-person arms): the
// sun's shadows on the shape are those in the shade light's shadow map
// (`with_shade_light`) [engineering choice].
//
// REFRACT (shapes with the Refraction shader flag, main pass): what lies behind the shape
// (Bevy's view transmission texture), shifted on the screen by the shape's normal (normal
// map applied) in view space times `surface.refraction` [hypothesis, nif_material.rs
// "Refraction"]; nothing of the shape's own colour or lighting.
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
    pbr_types::{STANDARD_MATERIAL_FLAGS_UNLIT_BIT, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD},
    lighting::{EnvBRDFApprox, F_AB},
    mesh_view_bindings::{lights, view, view_transmission_texture, view_transmission_sampler},
    view_transformations::frag_coord_to_uv,
}
#ifdef SHADE_LIGHT
#import bevy_pbr::{
    pbr_types::PbrInput,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    shadows::fetch_directional_shadow,
}
#endif
#endif

struct NifSurface {
    // x start angle (cosine), y stop angle (cosine), z start opacity, w stop opacity;
    // (1, 0, 1, 1) when the shape has no falloff.
    falloff: vec4<f32>,
    // Fragments with a lower alpha are not drawn (the alpha test of a shape that is
    // drawn blended); 0 on every other shape. nif_prepass.wgsl has the same struct.
    alpha_cut: f32,
    // 1: an additive shape with source factor ONE, which adds its colour whole (only the
    // view-angle falloff fades it); 0 otherwise.
    add_whole: f32,
    // REFRACT: the screen shift (UV) per unit of the view-space normal; 0 otherwise.
    refraction: f32,
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

// The alpha an additive shape (blend factors SRC_ALPHA and ONE) is drawn with, for its
// alpha `a` (CPU mirror and the reasons: nif_material.rs `add_alpha`, ADD_ALPHA_POWER).
// The game adds colour × alpha to display values [hypothesis]; a fixed blend cannot,
// and alpha^1.5 added as linear light comes near it in the Prospector Saloon.
fn add_alpha(a: f32) -> f32 {
    return pow(saturate(a), 1.5);
}

#ifndef PREPASS_PIPELINE
#ifdef SHADE_LIGHT
// The lit colour of a fragment whose sun shadows are those of the shade light
// (nif_material.rs, "Shade light"). Of the directional lights with shadows, the brightest
// is the sun and the next the shade light, which has the sun's colour at `share` of its
// strength. `shadowed` is Bevy's lighting of `in`, each light with its own shadows.
// Without two such lights it is the result.
//
// Bevy's lighting has no opening for another shadow value, so the result is put together
// from two of its results. With S the sun's unshadowed light on the fragment and R the
// rest of its light:
//   shadowed = R + S * (own + share * shade)    (the shadow values of the two maps)
//   lit      = R + S * (1 + share)              (the same without shadows)
// so S = (lit - shadowed) / ((1 - own) + share * (1 - shade)), and the colour wanted,
// R + S * shade, is lit - S * (1 + share - shade).
fn with_shade_light(in: PbrInput, shadowed: vec4<f32>) -> vec4<f32> {
    if (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) == 0u {
        return shadowed;
    }
    let none = lights.n_directional_lights;
    var sun = none;
    var sun_amount = 0.0;
    var shade_light = none;
    var shade_amount = 0.0;
    for (var i = 0u; i < none; i += 1u) {
        if (lights.directional_lights[i].flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) == 0u {
            continue;
        }
        let colour = lights.directional_lights[i].color.rgb;
        let amount = max(colour.r, max(colour.g, colour.b));
        if sun == none || amount > sun_amount {
            shade_light = sun;
            shade_amount = sun_amount;
            sun = i;
            sun_amount = amount;
        } else if shade_light == none || amount > shade_amount {
            shade_light = i;
            shade_amount = amount;
        }
    }
    if shade_light == none || shade_amount <= 0.0 {
        return shadowed;
    }
    let share = shade_amount / sun_amount;
    // As in apply_pbr_lighting.
    let view_z = dot(vec4(
        view.view_from_world[0].z,
        view.view_from_world[1].z,
        view.view_from_world[2].z,
        view.view_from_world[3].z
    ), in.world_position);
    let own = fetch_directional_shadow(sun, in.world_position, in.world_normal, view_z, in.frag_coord.xy);
    let shade = fetch_directional_shadow(shade_light, in.world_position, in.world_normal, view_z, in.frag_coord.xy);
    let missing = (1.0 - own) + share * (1.0 - shade);
    if missing < 0.000001 {
        // Neither map has a shadow here.
        return shadowed;
    }
    var unshadowed = in;
    unshadowed.flags &= ~MESH_FLAGS_SHADOW_RECEIVER_BIT;
    let lit = apply_pbr_lighting(unshadowed);
    return lit - (lit - shadowed) * ((1.0 + share - shade) / missing);
}
#endif
#endif

@fragment
fn fragment(
    in: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
#ifdef VISIBILITY_RANGE_DITHER
    visibility_range_dither(in.position, in.visibility_range_dither);
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
#ifdef REFRACT
    let tilt = (view.view_from_world * vec4(pbr_input.N, 0.0)).xy;
    let shifted = clamp(frag_coord_to_uv(in.position.xy) + vec2(tilt.x, -tilt.y) * surface.refraction, vec2(0.0), vec2(1.0));
    var refracted: FragmentOutput;
    refracted.color = vec4(textureSampleLevel(view_transmission_texture, view_transmission_sampler, shifted, 0.0).rgb, 1.0);
    return refracted;
#endif
    // |N·V|: the normal is already flipped towards the viewer on back faces of double-sided
    // shapes; abs() also covers single-sided ones seen edge-on.
    let cos_angle = abs(dot(normalize(pbr_input.world_normal), pbr_input.V));
    let falloff = falloff_opacity(cos_angle, surface.falloff);
    pbr_input.material.base_color.a *= falloff;
    if pbr_input.material.base_color.a < surface.alpha_cut {
        discard;
    }
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
#ifdef SHADE_LIGHT
        out.color = with_shade_light(pbr_input, out.color);
#endif
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
#ifdef BLEND_PREMULTIPLIED_ALPHA
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS) == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD {
        out.color.a = select(add_alpha(out.color.a), falloff, surface.add_whole > 0.5);
    }
#endif
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#ifdef SOFT_EDGES
    out.color.a = edge;
#endif
#endif
#endif
    return out;
}
