// NIF surface material (nif_material.rs): Bevy's standard PBR fragment with six changes,
// all [hypothesis, docs/spec-notes/nif.md §6 "Emissive light", "Specular", "View-angle
// falloff", "Multiply blend", "Additive blend", "Alpha"]:
// - the alpha is scaled by the view-angle falloff (blended effect shapes only) and, in the
//   main pass, by the soft intersection with the surface behind (`soft_fade`);
// - a blended shape that is also alpha-tested drops the fragments below its cut;
// - the emissive light is modulated by the surface colour (diffuse texture × vertex
//   colour), as NIF emissive is;
// - ambient light is diffuse only: the specular part Bevy adds for its ambient light is
//   taken out again, since the game's ambient term is a flat colour and highlights come
//   from lights (and environment maps, not drawn yet);
// - a multiply-blended shape's colour is a factor on what lies behind it: summed in
//   display values, held to 0..1, and neither fogged nor tonemapped;
// - an additive shape adds what it would add to the display values of the opaque frame
//   behind it, read from Bevy's view transmission texture (`add_display`);
// - an unlit alpha-blended shape (smoke, mist, dust, particles) puts about what it would
//   put on display values: its colour term is colour × alpha^1.5 (`blend_colour_scale`).

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
// "Refraction"]; nothing of the shape's own colour or lighting. With a fire period the
// normal map's tilt, scrolled over time, is added (nif_material.rs "Fire refraction").
//
// Holes (nif_material.rs "Holes"): where `surface.holes.x` is 0 or more, the holes
// texture's alpha (0-255) at or above it leaves a hole and the band of `surface.holes.y`
// below it is drawn in `surface.holes_edge`; at 255 or more there are none.
//
// FACEGEN_TINT (an NPC's face and skin shapes, main pass; nif_material.rs "FaceGen tint"):
// the `.egt` colour offset is added to the diffuse texture's stored sRGB value (0-255)
// and held to 0-255, as fnv_actor::facegen::add_offset does to the texture's bytes.
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
    pbr_types::{STANDARD_MATERIAL_FLAGS_UNLIT_BIT, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND, STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT},
    lighting::{EnvBRDFApprox, F_AB},
    mesh_view_bindings::{lights, view, globals, view_transmission_texture, view_transmission_sampler, fog},
    mesh_view_types::FOG_MODE_ATMOSPHERIC,
    view_transformations::{frag_coord_to_uv, frag_coord_to_ndc, position_ndc_to_world, depth_ndc_to_view_z},
}
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils
#endif
#ifdef SHADE_LIGHT
#import bevy_pbr::{
    pbr_types::PbrInput,
    mesh_types::MESH_FLAGS_SHADOW_RECEIVER_BIT,
    mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT,
    shadows::fetch_directional_shadow,
}
#endif
#endif

#import nvf_assets::nif_surface_bindings::{NifSurface, nif_surface, standard_material, holes_alpha, facegen_size, facegen_texel, base_color_texel, normal_map_texel}

#ifdef FACEGEN_TINT
// The FaceGen offset at texture coordinate `uv` (CPU: fnv_actor::facegen::add_offset):
// the offset image stretched over the texture, flipped vertically (its first row is the
// texture's bottom row) and read bilinearly between texel centres, held at its edges. A
// coordinate outside 0..1 reads the offset where the repeated texture is read.
fn facegen_offset_at(ii: u32, uv: vec2<f32>) -> vec3<f32> {
    let size = vec2<f32>(facegen_size(ii));
    let t = fract(uv);
    let p = clamp(vec2(t.x, 1.0 - t.y) * size - 0.5, vec2(0.0), size - 1.0);
    let p0 = vec2<i32>(floor(p));
    let p1 = min(p0 + 1, vec2<i32>(size) - 1);
    let f = p - floor(p);
    let top = mix(facegen_texel(ii, p0), facegen_texel(ii, vec2(p1.x, p0.y)), f.x);
    let bottom = mix(facegen_texel(ii, vec2(p0.x, p1.y)), facegen_texel(ii, p1), f.x);
    return mix(top, bottom, f.y);
}

// sRGB transfer functions (IEC 61966-2-1), as the GPU applies them to an sRGB texture.
fn linear_to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3(0.0031308));
}

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

// A texel `texel` (linear, as sampled) with the offset `offset` (colour units) added to
// its stored value and held to 0..255 (CPU mirror: nvforever facegen.rs tests, `tint_texel`).
fn facegen_tint(texel: vec3<f32>, offset: vec3<f32>) -> vec3<f32> {
    let stored = linear_to_srgb(saturate(texel)) * 255.0;
    return srgb_to_linear(clamp(stored + offset, vec3(0.0), vec3(255.0)) / 255.0);
}
#endif

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

// The linear light an additive shape (blend factors SRC_ALPHA or ONE, and ONE) of linear
// colour `colour` and alpha `a` adds over the opaque frame `behind` (linear; CPU mirror
// and the reasons: nif_material.rs `add_display`). The game adds colour × alpha to the
// frame's display values [hypothesis]: the sum is formed in display values (gamma 2.2),
// held to ADD_DISPLAY_MAX (2.0) or the frame's own value if brighter, and the difference
// to the frame returned as linear light.
fn add_display(colour: vec3<f32>, a: f32, behind: vec3<f32>) -> vec3<f32> {
    let b = max(behind, vec3(0.0));
    let shown = pow(b, vec3(1.0 / 2.2));
    let added = pow(max(colour, vec3(0.0)), vec3(1.0 / 2.2)) * saturate(a);
    let sum = min(shown + added, max(shown, vec3(2.0)));
    return max(pow(sum, vec3(2.2)) - b, vec3(0.0));
}

// The factor on the colour of an unlit alpha-blended shape of alpha `a` (CPU mirror and
// the reasons: nif_material.rs `blend_colour_scale`). Bevy's blend is colour × alpha +
// behind × (1 − alpha) in linear light; the game's is the same sum in display values
// [hypothesis], where light smoke over a dark room adds far less. Scaled by √alpha, the
// colour term becomes colour × alpha^1.5, as for additive shapes; the share of what lies
// behind is kept (nif_material.rs BLEND_ALPHA_POWER).
fn blend_colour_scale(a: f32) -> f32 {
    return sqrt(saturate(a));
}



#ifndef PREPASS_PIPELINE
// The image space's Emissive Mult over the worldspace base image space's (nvforever
// fog.rs and sky.rs `emissive_scale`), which the view's game fog carries in `bi.y`; 1
// with any other fog.
fn view_emissive_scale() -> f32 {
    return select(1.0, fog.bi.y, fog.mode == FOG_MODE_ATMOSPHERIC && fog.bi.y > 0.0);
}

// Soft intersections (nif_material.rs module notes): the share of the shape kept at this
// fragment, falling from 1 at `distance` in front of the opaque surface behind it (the
// depth prepass) to 0 on it. Not faded where the shape lies along that surface (the two
// facings, from screen derivatives, within about 25 degrees), nor in front of the sky,
// nor without a prepass. Called in uniform control flow (screen derivatives).
fn soft_fade(frag_coord: vec4<f32>, world_pos: vec3<f32>, distance: f32) -> f32 {
#ifdef DEPTH_PREPASS
    let depth = prepass_utils::prepass_depth(frag_coord, 0u);
    let scene = position_ndc_to_world(frag_coord_to_ndc(vec4(frag_coord.xy, max(depth, 1e-7), 1.0)));
    let scene_n = cross(dpdx(scene), dpdy(scene));
    let own_n = cross(dpdx(world_pos), dpdy(world_pos));
    let lengths = length(scene_n) * length(own_n);
    let along = select(0.0, abs(dot(scene_n, own_n)) / lengths, lengths > 1e-12);
    let gap = depth_ndc_to_view_z(frag_coord.z) - depth_ndc_to_view_z(max(depth, 1e-7));
    let fade = mix(saturate(gap / max(distance, 1e-3)), 1.0, smoothstep(0.85, 0.95, along));
    return select(fade, 1.0, distance <= 0.0 || depth <= 0.0);
#else
    return 1.0;
#endif
}

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
    let k = (1.0 + share - shade) / missing;
    if lights.ambient_color.a < 0.0 {
        // The display-value sum (nvforever lights.rs DISPLAY_HELPERS): the diffuse light is
        // texture × sum^2.2, so its display value (^(1/2.2)) is linear in the shadow
        // values; the same algebra there (the specular part is taken along).
        let lit_d = pow(max(lit.rgb, vec3(0.0)), vec3(1.0 / 2.2));
        let shadowed_d = pow(max(shadowed.rgb, vec3(0.0)), vec3(1.0 / 2.2));
        return vec4(pow(max(lit_d - (lit_d - shadowed_d) * k, vec3(0.0)), vec3(2.2)), shadowed.a);
    }
    return lit - (lit - shadowed) * k;
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
    let surface = nif_surface(in.instance_index);
    // Holes (nif_material.rs "Holes"): 1 in the edge band, 0 elsewhere.
    var holes_band = 0.0;
#ifdef VERTEX_UVS_A
    if surface.holes.x >= 0.0 && surface.holes.x < 255.0 {
        let h = holes_alpha(in.instance_index, in.uv) * 255.0;
        if h >= surface.holes.x {
            discard;
        }
        holes_band = select(0.0, 1.0, h >= surface.holes.x - surface.holes.y);
    }
#endif
    var pbr_input = pbr_input_from_standard_material(in, is_front);
#ifdef FACEGEN_TINT
#ifdef VERTEX_UVS_A
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_BASE_COLOR_TEXTURE_BIT) != 0u {
        // base_color is material colour × vertex colour × texel: the texel is read again
        // as pbr_input_from_standard_material reads it, and its tinted value takes its
        // place in that product.
        let standard = standard_material(in.instance_index);
        let uv = (standard.uv_transform * vec3(in.uv, 1.0)).xy;
        let texel = base_color_texel(in.instance_index, uv, view.mip_bias).rgb;
        var factor = standard.base_color.rgb;
#ifdef VERTEX_COLORS
        factor *= in.color.rgb;
#endif
        let tinted = facegen_tint(texel, facegen_offset_at(in.instance_index, uv));
        pbr_input.material.base_color = vec4(pbr_input.material.base_color.rgb + factor * (tinted - texel), pbr_input.material.base_color.a);
    }
#endif
#endif
#ifdef REFRACT
    var tilt = (view.view_from_world * vec4(pbr_input.N, 0.0)).xy;
#ifdef STANDARD_MATERIAL_NORMAL_MAP
#ifdef VERTEX_UVS_A
    if surface.fire_period != 0.0 {
        // Fire refraction (nif_material.rs "Fire refraction"): the normal map read again,
        // scrolled up its texture once every `fire_period` seconds (down when negative:
        // fract keeps the offset in 0..1 either way); its tangent-space tilt
        // is added to the shift as it is.
        let scrolled = in.uv + vec2(0.0, fract(globals.time / surface.fire_period));
        tilt += normal_map_texel(in.instance_index, scrolled).rg * 2.0 - 1.0;
    }
#endif
#endif
    // A cut-out (hair, a refracting actor's alpha-tested parts) keeps its outline.
    if pbr_input.material.base_color.a < surface.alpha_cut {
        discard;
    }
    let shifted = clamp(frag_coord_to_uv(in.position.xy) + vec2(tilt.x, -tilt.y) * surface.refraction, vec2(0.0), vec2(1.0));
    var refracted: FragmentOutput;
    refracted.color = vec4(textureSampleLevel(view_transmission_texture, view_transmission_sampler, shifted, 0.0).rgb, 1.0);
    return refracted;
#endif
    // |N·V|: the normal is already flipped towards the viewer on back faces of double-sided
    // shapes; abs() also covers single-sided ones seen edge-on.
    let cos_angle = abs(dot(normalize(pbr_input.world_normal), pbr_input.V));
#ifdef PREPASS_PIPELINE
    let falloff = falloff_opacity(cos_angle, surface.falloff);
#else
    let falloff = falloff_opacity(cos_angle, surface.falloff) * soft_fade(in.position, pbr_input.world_position.xyz, surface.soft.x);
#endif
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
    // Bevy's pbr_input_from_standard_material fills the emissive light in only for lit
    // materials, so an unlit shape's stays black: it is read from the material here
    // (without the glow map, which unlit effect shapes do not use).
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 0u {
        pbr_input.material.emissive = standard_material(in.instance_index).emissive;
    }
    // The image space's Emissive Mult; an additive shape forms its light in display
    // values (`add_display`), so there it is raised to the display gamma.
    let emissive_scale = select(view_emissive_scale(), pow(view_emissive_scale(), 2.2), (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS) == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD);
    let e = vec4(pbr_input.material.emissive.rgb * emissive_scale, pbr_input.material.emissive.a);
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
        var ambient_spec = spec * spec_occlusion * lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
        if lights.ambient_color.a < 0.0 {
            // A world view (nvforever lights.rs DISPLAY_HELPERS): the highlight Bevy added
            // took the ambient colour as linear light (`nvf_ambient_ratio`).
            ambient_spec *= pow(max(lights.ambient_color.rgb * view.exposure, vec3(1e-6)), vec3(1.2));
        }
        out.color = vec4(max(out.color.rgb - view.exposure * ambient_spec, vec3(0.0)), out.color.a);
    } else {
        out.color = pbr_input.material.base_color;
#ifndef BLEND_MULTIPLY
        // An unlit shape with a non-black emissive colour is drawn at its emissive light:
        // surface colour × emissive colour (a display colour, nif_spawn.rs
        // `shape_emissive`) × emissive multiplier, so a warm colour tints a white glow
        // texture (Dead Money's lantern glow) and neon authored at multiplier 25
        // (Gomorrah's tubes) burns far brighter than white and glows in an HDR view, as in
        // the original's frames. A black emissive colour leaves the surface colour
        // [hypothesis from the original's frames, nif.md §6 "Emissive colour of unlit
        // shapes"]. Without the light glow (tonemapped in the shader) it is never
        // brighter than the surface colour.
        let unlit_e = pbr_input.material.emissive.rgb;
        if any(unlit_e > vec3(0.0)) {
#ifdef TONEMAP_IN_SHADER
            out.color = vec4(min(unlit_e, out.color.rgb), out.color.a);
#else
            out.color = vec4(unlit_e, out.color.a);
#endif
        }
#endif
    }
    out.color = vec4(mix(out.color.rgb, surface.holes_edge.rgb, holes_band), out.color.a);
#ifdef BLEND_MULTIPLY
    // The colour is a factor, not light: no fog and no tonemapping. Bevy's multiply blend
    // is destination × colour + destination × (1 − alpha), so the colour is premultiplied
    // as bevy_pbr::pbr_functions::premultiply_alpha does.
    let factor = multiply_factor(out.color.rgb, multiply_emissive);
    out.color = vec4(factor * out.color.a, out.color.a);
#else
#ifdef BLEND_PREMULTIPLIED_ALPHA
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS) == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD {
        // Additive: fog (and, in views without HDR, the tonemapping) on the colour at
        // alpha 1, which Bevy's premultiplying leaves as it is with alpha 0; then the
        // display-value sum over the opaque frame behind (the anchor in nif_material.rs
        // keeps Bevy's copy of it in every view). A ONE / ONE shape adds its colour whole,
        // faded only by the view-angle falloff.
        let add_a = select(out.color.a, falloff, surface.add_whole > 0.5);
        let fogged = main_pass_post_lighting_processing(pbr_input, vec4(out.color.rgb, 1.0));
        let behind = textureSampleLevel(view_transmission_texture, view_transmission_sampler, frag_coord_to_uv(in.position.xy), 0.0).rgb;
        out.color = vec4(add_display(fogged.rgb, add_a, behind), 0.0);
    } else {
        out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    }
#else
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
#endif
    let blend_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if blend_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_BLEND && (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) != 0u {
        out.color = vec4(out.color.rgb * blend_colour_scale(out.color.a), out.color.a);
    }
#ifdef SOFT_EDGES
    out.color.a = edge;
#endif
#endif
#endif
    return out;
}
