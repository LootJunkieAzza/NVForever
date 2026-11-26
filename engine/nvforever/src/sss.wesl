// Subsurface scattering helpers (nvforever sss.rs; NVF_SSS=1):
// - NVF_SKIN_SSS: analytic 2-lobe sphere-damped Gaussian-ReLU fit to Penner & Borshukov
//   (2011) / d'Eon & Irving (2007) 6-Gaussian human skin diffusion integral, RGB normal
//   blurring, warm shadow-penumbra bleed (not on SHADE_LIGHT shapes), dual-lobe sebum GGX
//   specular, reciprocal dielectric coupling, and shadow-map depth-thickness Beer-Lambert
//   transmittance. Under NVF_SKIN_SPLIT (the screen-space scattering, sss/screen.rs) the
//   light is left unblurred here and the diffuse share of each light is kept apart
//   (`nvf_skin_last_diffuse`) for the pass after the opaque draws, which blurs it.
// - NVF_FOLIAGE_SSS: energy-conserving two-sided leaf BSDF with cuticle Fresnel entry/exit,
//   Beer-Lambert chlorophyll spectral absorption, and normalized Henyey-Greenstein phase.

import bevy_pbr::render::{
    mesh_view_bindings as view_bindings,
    mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE,
    pbr_lighting::{D_GGX, F_AB, V_SmithGGXCorrelated, fresnel, getDistanceAttenuation},
    shadows::{get_cascade_index, world_to_directional_light_local},
};
@if(NVF_PHYSICAL_PBR) import bevy_pbr::render::pbr_lighting::nvf_diffuse_brdf;
@if(ENVIRONMENT_MAP) import bevy_pbr::light_probe::environment_map::compute_radiances;
@if(ENVIRONMENT_MAP) import bevy_pbr::render::pbr_lighting::{LightingInput, LAYER_BASE};
@if(ENVIRONMENT_MAP) import bevy_pbr::render::clustered_forward::ClusterableObjectIndexRanges;
@if(NVF_SKIN_ALBEDO && DEPTH_PREPASS) import bevy_pbr::prepass::utils::prepass_depth;
import bevy_render::maths::PI;

// NVF_SKIN_SPLIT: the diffuse light of the last nvf_skin_eval, the share the screen-space
// scattering blurs (sss/screen.rs). The lighting hook (sss/hook.rs) adds it up per light.
var<private> nvf_skin_last_diffuse: vec3<f32>;

// NVF_SKIN_SPLIT: pre- and post-scatter texturing (Jimenez et al. 2015, "Separable
// Subsurface Scattering": L = A^(1-p) x blur(A^p x E), A the skin's colour, E its light).
// The face's pigment, the lips' red, the lash line and moles lie in a thin layer over the
// scattering dermis [documented: Donner et al. 2008, "A Layered, Heterogeneous Reflectance
// Model for Acquiring and Rendering Human Skin"], so most of the colour is applied after the
// blur and stays sharp; blurring A x E whole smeared the lips onto the chin. The skin's
// fragments draw their diffuse light divided by A^(1-p) and write A^(1-p) (its square root,
// for the dark end's precision in 8 bits) to binding 50 (bevy_pbr NvfSkinAlbedoBindings,
// sss/screen.rs), which the pass multiplies back in after the blur. Only a fragment that
// will be seen writes: the storage write runs the shader for hidden fragments too, so its
// depth is checked against the prepass's, as the main pass's equal test does
// (nif_material.rs `depth_equal`). p 0.25 [hypothesis: skin research notes 2026-11-26
// §2.2]; the darkest colour taken is NVF_SKIN_ALBEDO_MIN.
//
// The texture's alpha tells the pass what drew there (sss/screen.rs clears it to
// NVF_MARK_OTHER each frame): skin, NVF_MARK_SKIN plus its strand noise (nif-assets
// nif_material.wesl `skin_strand_noise`, carried in `material.ior`) times NVF_MARK_STRAND;
// hair (NVF_HAIR, `nvf_hair_mark`), 0. The pass shades the skin beside the hair with them
// (sss_screen.wesl `hairline`).
@if(NVF_SKIN_ALBEDO) @group(0) @binding(50) var nvf_skin_albedo: texture_storage_2d<rgba8unorm, write>;
@if(NVF_SKIN_ALBEDO && NVF_SKIN_SPLIT) @group(0) @binding(51) var nvf_skin_spec_buf: texture_storage_2d<rgba16float, write>;
const NVF_SKIN_PRE_SCATTER: f32 = 0.12;
const NVF_SKIN_ALBEDO_MIN: f32 = 0.01;
const NVF_MARK_SKIN: f32 = 0.6;
const NVF_MARK_STRAND: f32 = 0.4;

@if(NVF_SKIN_SPLIT) fn nvf_skin_post_scatter(frag_coord: vec4<f32>, diffuse_color: vec3<f32>, strand: f32, spec_hdr: vec3<f32>) -> vec3<f32> {
    let post = pow(clamp(diffuse_color, vec3(NVF_SKIN_ALBEDO_MIN), vec3(1.0)), vec3(1.0 - NVF_SKIN_PRE_SCATTER));
    if prepass_depth(frag_coord, 0u) == frag_coord.z {
        let px = vec2<i32>(floor(frag_coord.xy));
        textureStore(nvf_skin_albedo, px, vec4(sqrt(post), NVF_MARK_SKIN + NVF_MARK_STRAND * saturate(strand)));
        textureStore(nvf_skin_spec_buf, px, vec4(max(spec_hdr, vec3(0.0)), 1.0));
    }
    return post;
}

// NVF_HAIR: mark a hair fragment that will be seen (the depth check of
// nvf_skin_post_scatter) for the hairline's shade.
@if(NVF_SKIN_ALBEDO && DEPTH_PREPASS) fn nvf_hair_mark(frag_coord: vec4<f32>) {
    if prepass_depth(frag_coord, 0u) == frag_coord.z {
        textureStore(nvf_skin_albedo, vec2<i32>(floor(frag_coord.xy)), vec4(0.0));
    }
}

fn nvf_sss_srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let x = max(c, vec3(0.0));
    let m = max(max(x.r, max(x.g, x.b)), 1.0);
    let n = x / m;
    let lin = select(pow((n + vec3(0.055)) / 1.055, vec3(2.4)), n / 12.92, n <= vec3(0.04045));
    return lin * m;
}

fn nvf_sss_light_radiance(c: vec3<f32>, is_display: bool, physical: bool) -> vec3<f32> {
    let inv_pi = 1.0 / PI;
    if !is_display {
        return c * inv_pi;
    }
    let k = view_bindings::view.exposure * inv_pi;
    if physical {
        return nvf_sss_srgb_to_linear(c * k) / max(view_bindings::view.exposure, 1e-6);
    }
    return pow(max(c * k, vec3(0.0)), vec3(2.2)) / max(view_bindings::view.exposure, 1e-6);
}

fn nvf_sss_spot_attenuation(light_id: u32, L: vec3<f32>) -> f32 {
    let light = &view_bindings::clustered_lights.data[light_id];
    var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
    if ((*light).flags & POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
        spot_dir.y = -spot_dir.y;
    }
    let cd = dot(-spot_dir, L);
    let cone = saturate(cd * (*light).light_custom_data.z + (*light).light_custom_data.w);
    return cone * cone;
}

fn nvf_sss_safe_normalize(v: vec3<f32>, fallback: vec3<f32>) -> vec3<f32> {
    let l2 = dot(v, v);
    return select(fallback, v * inverseSqrt(max(l2, 1e-12)), l2 > 1e-12);
}

fn nvf_skin_lobe(u: vec3<f32>, sigma: vec3<f32>) -> vec3<f32> {
    let s = max(sigma, vec3(1e-4));
    let z = clamp(u / s, vec3(-20.0), vec3(20.0));
    let phi_cdf = vec3(1.0) / (vec3(1.0) + exp(-1.702 * z));
    let phi_pdf = 0.39894228 * exp(-0.5 * z * z);
    return max(vec3(0.0), (u * phi_cdf + s * phi_pdf) * exp(-0.5 * s * s));
}

// The red tail is the face's, not the forearm's the d'Eon & Irving profile was measured on:
// facial skin holds more blood and red light travels farther in it [hypothesis: skin
// research notes 2026-11-26 §1.4, citing Donner & Jensen 2006 and Chiang et al. 2016; the
// numbers are a starting point to tune by eye, the forearm fit was w2.r 0.549, s2.r 1.55].
fn nvf_preintegrated_skin(ndotl_rgb: vec3<f32>, curvature_mm: f32) -> vec3<f32> {
    let k = clamp(curvature_mm, 0.0, 0.45);
    let w1 = vec3(0.36, 0.791, 0.649);
    let w2 = vec3(0.64, 0.209, 0.351);
    let s1 = vec3(0.20, 0.14, 0.08) * k;
    let s2 = vec3(1.95, 0.50, 0.24) * k;
    return clamp(w1 * nvf_skin_lobe(ndotl_rgb, s1) + w2 * nvf_skin_lobe(ndotl_rgb, s2), vec3(0.0), vec3(1.0));
}

// E[max(u + n, 0)] for n ~ N(0, sigma²): a ramp's start blurred by a Gaussian.
fn nvf_skin_soft_start(u: vec3<f32>, sigma: vec3<f32>) -> vec3<f32> {
    let s = max(sigma, vec3(1e-4));
    let z = clamp(u / s, vec3(-20.0), vec3(20.0));
    let phi_cdf = vec3(1.0) / (vec3(1.0) + exp(-1.702 * z));
    let phi_pdf = 0.39894228 * exp(-0.5 * z * z);
    return max(vec3(0.0), u * phi_cdf + s * phi_pdf);
}

// The ramp clamp(x, 0, 1) blurred by a Gaussian of width sigma, per channel.
fn nvf_skin_soft_ramp(x: vec3<f32>, sigma: vec3<f32>) -> vec3<f32> {
    return nvf_skin_soft_start(x, sigma) - nvf_skin_soft_start(x - vec3(1.0), sigma);
}

// Width of each channel's scattering across a shadow's penumbra, in penumbra widths: red
// light travels farthest in skin, blue least (d'Eon & Irving 2007 profile, sss.rs tests).
const NVF_SKIN_PENUMBRA_SIGMA: vec3<f32> = vec3(0.34, 0.11, 0.05);

// How far each channel's diffuse light takes the geometric normal over the normal map's
// (nvf_skin_eval; Penner & Borshukov 2011 §2.4) [hypothesis: skin research notes
// 2026-11-26 §1.3; the earlier 0.08, 0.04, 0 left the pores shading as dark grit].
const NVF_SKIN_NORMAL_SMOOTH: vec3<f32> = vec3(0.80, 0.50, 0.20);
const NVF_SKIN_BODY_NORMAL_SMOOTH: vec3<f32> = vec3(0.32, 0.18, 0.06);

// NVF_SKIN_SPLIT: the screen-space pass (sss/screen.rs) blurs the skin's light afterwards,
// so the pre-integration's wrap and the penumbra's bleed here are kept at this share of
// their width; without it (multisampling, sss/screen.rs) they are whole. The screen blur
// cannot gather light round a silhouette seen edge-on (the nose, the jaw), where the
// pre-integration still can [hypothesis: skin research notes 2026-11-26 §1.4].
@if(NVF_SKIN_SPLIT) const NVF_SKIN_SPLIT_SHARE: f32 = 0.40;
@if(!NVF_SKIN_SPLIT) const NVF_SKIN_SPLIT_SHARE: f32 = 1.0;

// SHADE_LIGHT (the player's first-person arms): the shadow as it is. with_shade_light
// (nif_material.wesl) takes the shadowed light apart and puts it back together, which
// needs the light to be linear in the shadow.
@if(SHADE_LIGHT) fn nvf_skin_shadow(shadow: f32, curvature_mm: f32) -> vec3<f32> {
    return vec3(saturate(shadow));
}

// Penumbra scattering (Penner & Borshukov 2011, "Pre-Integrated Skin Shading", shadow
// penumbra): light scatters under the skin from the lit side of a shadow's edge into the
// dark side, red farthest, so a shadow's edge on skin runs warm. The filtered shadow value
// is read as the position across the penumbra, sharpened by 2 (the step takes the middle
// half of the filter's ramp, as the paper sharpens it before the blur), and each channel
// is then blurred by its scattering width, held so that 0 stays 0 and 1 stays 1. Warm
// only: on the dark side red runs past green and green past blue as blurred; on the lit
// side, where the symmetric blur took red and green below blue (an olive band before
// every shadow), each keeps blue's light (red never below blue, green between them)
// [hypothesis: skin research notes 2026-11-26 §1.4; the notes' own formula, a stretch
// about 0.35, put the bleed below the shadow on the dark side and so added none there].
@if(!SHADE_LIGHT) fn nvf_skin_shadow(shadow: f32, curvature_mm: f32) -> vec3<f32> {
    let sigma = NVF_SKIN_PENUMBRA_SIGMA * NVF_SKIN_SPLIT_SHARE;
    let x = (saturate(shadow) - 0.5) * 2.0 + 0.5;
    let g = nvf_skin_soft_ramp(vec3(x), sigma);
    let g0 = nvf_skin_soft_ramp(vec3(-0.5), sigma);
    let g1 = nvf_skin_soft_ramp(vec3(1.5), sigma);
    let blurred = saturate((g - g0) / max(g1 - g0, vec3(1e-4)));
    let r = max(blurred.r, blurred.b);
    let gr = min(max(blurred.g, blurred.b), r);
    return vec3(r, gr, blurred.b);
}

fn nvf_sss_dielectric_E(F0: vec3<f32>, perceptual_roughness: f32, mu: f32) -> vec3<f32> {
    let fab = F_AB(perceptual_roughness, max(mu, 0.0001));
    let Fss = F0 * fab.x + vec3(fab.y);
    let Ems = max(1.0 - (fab.x + fab.y), 0.0);
    let Favg = F0 + (vec3(1.0) - F0) * (1.0 / 21.0);
    let Fms = Fss * Favg / max(vec3(1.0) - Ems * Favg, vec3(1e-4)) * Ems;
    return min(Fss + Fms, vec3(0.999));
}

fn nvf_sss_dielectric_T(F0_d: vec3<f32>, r: f32, NdotV: f32, NdotL: f32) -> vec3<f32> {
    let E_o = nvf_sss_dielectric_E(F0_d, r, NdotV);
    let E_i = nvf_sss_dielectric_E(F0_d, r, NdotL);
    let E_avg = (5.0 / 18.0) * (2.0 * 0.11270167) * nvf_sss_dielectric_E(F0_d, r, 0.11270167)
        + (8.0 / 18.0) * (2.0 * 0.5) * nvf_sss_dielectric_E(F0_d, r, 0.5)
        + (5.0 / 18.0) * (2.0 * 0.88729833) * nvf_sss_dielectric_E(F0_d, r, 0.88729833);
    return ((vec3(1.0) - E_i) * (vec3(1.0) - E_o)) / max(vec3(1.0) - E_avg, vec3(1e-4));
}

// Skin's specular (Jimenez et al. 2013, "Next-Generation Character Rendering": two GGX
// lobes over a dielectric of F0 0.028, IOR about 1.4 [documented: Donner & Jensen 2006]).
// The base lobe is Bevy's own: the material (nvf-assets nif_material.wesl, NVF_SKIN_SSS)
// sets F0 0.028 and a roughness from 0.64 (dry) to 0.42 (oily) by the normal map's
// specular mask, the oiliness. The oil is read back from that roughness here.
const NVF_SKIN_ROUGH_DRY: f32 = 0.66;
const NVF_SKIN_ROUGH_OILY: f32 = 0.52;

fn nvf_skin_oiliness(perceptual_roughness: f32) -> f32 {
    return saturate((NVF_SKIN_ROUGH_DRY - perceptual_roughness) / (NVF_SKIN_ROUGH_DRY - NVF_SKIN_ROUGH_OILY));
}

// The sebum lobe's roughness and weight at oiliness `oil`: a dewy T-zone and lips, a drier
// jaw [hypothesis: skin research notes 2026-11-26 §1.2; starting values to tune by eye].
fn nvf_skin_sebum_roughness(oil: f32) -> f32 {
    return mix(0.36, 0.24, oil);
}

fn nvf_skin_sebum_weight(oil: f32) -> f32 {
    return mix(0.10, 0.22, oil);
}

// The sebum lobe for one light. Returns vec4(spec_rgb, the share of the light it reflects,
// which the diffuse light under it loses).
fn nvf_skin_sebum_specular(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, perceptual_roughness: f32) -> vec4<f32> {
    let NdotL = dot(N, L);
    let NdotV = max(dot(N, V), 0.0001);
    if NdotL <= 0.0 {
        return vec4(0.0);
    }
    let H = nvf_sss_safe_normalize(L + V, N);
    let NdotH = saturate(dot(N, H));
    let LdotH = saturate(dot(L, H));
    let oil = nvf_skin_oiliness(perceptual_roughness);
    let r_oil = nvf_skin_sebum_roughness(oil);
    let w_oil = nvf_skin_sebum_weight(oil);
    let a_oil = r_oil * r_oil;
    let F_oil = fresnel(vec3(0.028), LdotH);
    let spec = D_GGX(a_oil, NdotH) * V_SmithGGXCorrelated(a_oil, NdotV, NdotL) * F_oil * NdotL * w_oil;
    return vec4(spec, w_oil * F_oil.r);
}

// The sky's light on skin (sss/hook.rs, physical PBR, after Bevy's environment light):
// `diffuse_scale`, per channel, takes the sky's diffuse light from the normal map's N
// toward the geometric normal by NVF_SKIN_NORMAL_SMOOTH, as nvf_skin_eval does each lamp's
// (so the pores do not shade as grit in the shade either); `sebum`, the oil lobe's
// reflection of the sky (nvf_skin_sebum_specular is for lamps only), split-sum at the oil's
// roughness with F0 0.028 and the oil's weight.
struct NvfSkinSky {
    diffuse_scale: vec3<f32>,
    sebum: vec3<f32>,
}

@if(ENVIRONMENT_MAP) fn nvf_skin_sky(
    input: ptr<function, LightingInput>,
    ranges: ptr<function, ClusterableObjectIndexRanges>,
    found_diffuse_indirect: bool,
    world_normal: vec3<f32>,
    skin_body: f32,
) -> NvfSkinSky {
    var out: NvfSkinSky;
    let base = (*input).layers[LAYER_BASE];
    let oil = nvf_skin_oiliness(base.perceptual_roughness);
    let r_oil = nvf_skin_sebum_roughness(oil);
    var oil_layer = base;
    oil_layer.perceptual_roughness = r_oil;
    oil_layer.roughness = r_oil * r_oil;
    let at_n = compute_radiances(oil_layer, ranges, (*input).P, found_diffuse_indirect);
    var smooth_layer = base;
    smooth_layer.N = nvf_sss_safe_normalize(world_normal, base.N);
    let at_ng = compute_radiances(smooth_layer, ranges, (*input).P, found_diffuse_indirect);
    let smooth_w = mix(NVF_SKIN_NORMAL_SMOOTH, NVF_SKIN_BODY_NORMAL_SMOOTH, saturate(skin_body));
    let irr = mix(at_n.irradiance, at_ng.irradiance, smooth_w);
    let ok = at_n.irradiance > vec3(1e-6);
    out.diffuse_scale = select(vec3(1.0), clamp(irr / max(at_n.irradiance, vec3(1e-6)), vec3(0.0), vec3(4.0)), ok);
    let fab = F_AB(r_oil, max(base.NdotV, 0.0001));
    out.sebum = (0.028 * fab.x + fab.y) * nvf_skin_sebum_weight(oil) * at_n.radiance;
    return out;
}

fn nvf_dir_shadow_texel(coord: vec2<i32>, array_index: i32) -> f32 {
    @if(NO_ARRAY_TEXTURES_SUPPORT)
    return textureLoad(view_bindings::directional_shadow_textures, coord, 0);
    @else
    return textureLoad(view_bindings::directional_shadow_textures, coord, array_index, 0);
}

fn nvf_dir_shadow_depth(uv: vec2<f32>, array_index: i32) -> f32 {
    let dims = vec2<f32>(textureDimensions(view_bindings::directional_shadow_textures));
    let max_c = vec2<i32>(textureDimensions(view_bindings::directional_shadow_textures)) - vec2(1);
    let tc = clamp(uv, vec2(0.0), vec2(1.0)) * dims - vec2(0.5);
    let p0 = clamp(vec2<i32>(floor(tc)), vec2(0), max_c);
    let p1 = min(p0 + vec2(1), max_c);
    let f = saturate(tc - floor(tc));
    let d00 = nvf_dir_shadow_texel(p0, array_index);
    let d10 = nvf_dir_shadow_texel(vec2(p1.x, p0.y), array_index);
    let d01 = nvf_dir_shadow_texel(vec2(p0.x, p1.y), array_index);
    let d11 = nvf_dir_shadow_texel(p1, array_index);
    return mix(mix(d00, d10, f.x), mix(d01, d11, f.x), f.y);
}

// Reversed-Z shadow-map thickness (mm) from front lit surface to back fragment.
// Returns < 0 when outside cascades or unoccluded.
fn nvf_dir_skin_thickness_mm(light_id: u32, world_pos: vec3<f32>, ng: vec3<f32>, view_z: f32) -> f32 {
    let light = &view_bindings::lights.directional_lights[light_id];
    let cascade_index = get_cascade_index(light_id, view_z);
    if cascade_index >= (*light).num_cascades {
        return -1.0;
    }
    let inset_pos = vec4<f32>(world_pos - ng * 0.15, 1.0);
    let light_local = world_to_directional_light_local(light_id, cascade_index, inset_pos);
    if light_local.w == 0.0 {
        return -1.0;
    }
    let array_index = i32((*light).depth_texture_base_index + cascade_index);
    let blocker_z = nvf_dir_shadow_depth(light_local.xy, array_index);
    if blocker_z <= 1e-5 {
        return -1.0;
    }
    let M = (*light).cascades[cascade_index].clip_from_world;
    let ndc_z_per_world = max(length(vec3(M[0].z, M[1].z, M[2].z)), 1e-6);
    let delta_z = blocker_z - light_local.z;
    if delta_z <= 1e-5 {
        return -1.0;
    }
    let d_world = delta_z / ndc_z_per_world;
    if d_world < 0.20 {
        return -1.0;
    }
    return max(d_world * 14.3, 3.5);
}

// d'Eon & Irving (2007) 6-Gaussian Beer-Lambert slab transmittance T(d_mm).
fn nvf_skin_transmittance_profile(d_mm: f32) -> vec3<f32> {
    let s2 = d_mm * d_mm;
    return vec3(0.233, 0.455, 0.649) * exp(-s2 / (2.0 * 0.0064))
         + vec3(0.100, 0.336, 0.344) * exp(-s2 / (2.0 * 0.0484))
         + vec3(0.118, 0.198, 0.000) * exp(-s2 / (2.0 * 0.1870))
         + vec3(0.113, 0.007, 0.007) * exp(-s2 / (2.0 * 0.5670))
         + vec3(0.358, 0.004, 0.000) * exp(-s2 / (2.0 * 1.9900))
         + vec3(0.078, 0.000, 0.000) * exp(-s2 / (2.0 * 7.4100));
}

fn nvf_skin_eval(
    N: vec3<f32>,
    world_normal: vec3<f32>,
    V: vec3<f32>,
    L: vec3<f32>,
    diffuse_color: vec3<f32>,
    F0_dielectric: vec3<f32>,
    perceptual_roughness: f32,
    curvature_mm: f32,
    shadow: f32,
    thickness_mm: f32,
    macro_vis: f32,
    light_rad: vec3<f32>,
    physical: bool,
    skin_body: f32,
) -> vec3<f32> {
    let ng = nvf_sss_safe_normalize(world_normal, N);
    // Scattering under the skin smooths the light over the normal map's pores and bumps,
    // red most (Penner & Borshukov 2011 §2.4, "scattering applied to normal maps"); the
    // specular lobes keep the normal map's N whole. Head skin smooths facial pores strongly;
    // body skin keeps lower smoothing so neck tendons and muscle relief remain crisp.
    let smooth_w = mix(NVF_SKIN_NORMAL_SMOOTH, NVF_SKIN_BODY_NORMAL_SMOOTH, saturate(skin_body));
    let n_r = nvf_sss_safe_normalize(mix(N, ng, smooth_w.r), ng);
    let n_g = nvf_sss_safe_normalize(mix(N, ng, smooth_w.g), ng);
    let n_b = nvf_sss_safe_normalize(mix(N, ng, smooth_w.b), N);
    let ndotl_rgb = vec3(dot(n_r, L), dot(n_g, L), dot(n_b, L));
    let s = saturate(shadow);
    // NVF_SKIN_SPLIT: the screen-space pass blurs this light across the skin as well, so
    // the wrap and the penumbra's bleed are kept narrower (NVF_SKIN_SPLIT_SHARE).
    let preint = nvf_preintegrated_skin(ndotl_rgb, curvature_mm * NVF_SKIN_SPLIT_SHARE);
    let shadow_rgb = nvf_skin_shadow(s, curvature_mm);

    let phase_fwd = 0.35 + 0.65 * pow(saturate(dot(V, -L)), 3.0);
    let d_est = clamp(0.65 / max(curvature_mm, 0.04), 3.5, 18.0);
    let eff_mm = select(d_est, max(thickness_mm, d_est), thickness_mm >= 0.0);
    let rim_grazing = pow(1.0 - saturate(dot(N, V)), 2.0);
    let back_wrap = saturate(dot(-N, L) * 0.5 + 0.25) * (0.35 + 0.65 * rim_grazing);
    // The shadow map's own thickness (a directional light's, nvf_dir_skin_thickness_mm)
    // already measures what the light passes through to reach this side: the front of an
    // ear blocks the sun from its back by design, and a wall or a roof between them is far
    // thicker than the profile reaches (no light left past about 15 mm). The filtered
    // shadow would put the transmitted light out exactly where it shows; it applies only
    // when the thickness is guessed from the curvature. SHADE_LIGHT shapes (the
    // first-person arms) keep the light linear in the shadow (nvf_skin_shadow there).
    @if(!SHADE_LIGHT) let back_shadow = select(s, 1.0, thickness_mm >= 0.0);
    @if(SHADE_LIGHT) let back_shadow = s;
    let back_scatter = nvf_skin_transmittance_profile(eff_mm) * (back_wrap * phase_fwd * back_shadow * saturate(macro_vis) * 0.45);

    let sebum = nvf_skin_sebum_specular(N, V, L, perceptual_roughness);
    let spec_oil = sebum.xyz * (s * PI);
    let skin_scatter = preint * shadow_rgb + back_scatter;

    @if(NVF_PHYSICAL_PBR)
    if physical {
        let r_skin = clamp(perceptual_roughness, 0.045, 1.0);
        let h_skin = nvf_sss_safe_normalize(L + V, N);
        let skin_brdf_pi = nvf_diffuse_brdf(
            diffuse_color,
            r_skin,
            r_skin * r_skin,
            F0_dielectric,
            max(dot(N, V), 0.0001),
            max(dot(N, L), 0.0001),
            saturate(dot(N, h_skin)),
            saturate(dot(L, h_skin)),
        ) * PI;
        let t_oil = 1.0 - sebum.w;
        let diffuse = (skin_brdf_pi * (preint * shadow_rgb) + diffuse_color * back_scatter) * t_oil * light_rad;
        nvf_skin_last_diffuse = diffuse;
        return diffuse + spec_oil * light_rad;
    }
    @if(!NVF_PHYSICAL_PBR)
    if physical {
        let t_base = nvf_sss_dielectric_T(F0_dielectric, saturate(perceptual_roughness), max(dot(N, V), 0.0001), max(dot(N, L), 0.0001));
        let t_oil = 1.0 - sebum.w;
        let diffuse = diffuse_color * t_base * t_oil * skin_scatter * light_rad;
        nvf_skin_last_diffuse = diffuse;
        return diffuse + spec_oil * light_rad;
    }
    let base = vec3(saturate(dot(N, L)) * saturate(shadow));
    nvf_skin_last_diffuse = vec3(0.0);
    return (diffuse_color * max(skin_scatter - base, vec3(0.0)) + spec_oil * 0.5) * light_rad;
}

fn nvf_sss_clustered_skin(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    world_normal: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    F0_dielectric: vec3<f32>,
    perceptual_roughness: f32,
    curvature_mm: f32,
    shadow: f32,
    is_spot: bool,
    is_display: bool,
    physical: bool,
    skin_body: f32,
) -> vec3<f32> {
    let light = &view_bindings::clustered_lights.data[light_id];
    let to_light = (*light).position_radius.xyz - P;
    let d2 = dot(to_light, to_light);
    let L = to_light * inverseSqrt(max(d2, 0.0001));
    var attenuation = getDistanceAttenuation(d2, (*light).color_inverse_square_range.w);
    if is_spot {
        attenuation *= nvf_sss_spot_attenuation(light_id, L);
    }
    let light_rad = nvf_sss_light_radiance((*light).color_inverse_square_range.rgb, is_display, physical) * attenuation * select(1.0, 1.10, physical);
    return nvf_skin_eval(
        N, world_normal, V, L, diffuse_color, F0_dielectric, perceptual_roughness,
        curvature_mm, shadow, -1.0, shadow, light_rad, physical, skin_body,
    );
}

fn nvf_sss_point_skin(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    world_normal: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    F0_dielectric: vec3<f32>,
    perceptual_roughness: f32,
    curvature_mm: f32,
    shadow: f32,
    is_display: bool,
    physical: bool,
    skin_body: f32,
) -> vec3<f32> {
    return nvf_sss_clustered_skin(
        light_id, P, N, world_normal, V, diffuse_color, F0_dielectric, perceptual_roughness,
        curvature_mm, shadow, false, is_display, physical, skin_body,
    );
}

fn nvf_sss_spot_skin(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    world_normal: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    F0_dielectric: vec3<f32>,
    perceptual_roughness: f32,
    curvature_mm: f32,
    shadow: f32,
    is_display: bool,
    physical: bool,
    skin_body: f32,
) -> vec3<f32> {
    return nvf_sss_clustered_skin(
        light_id, P, N, world_normal, V, diffuse_color, F0_dielectric, perceptual_roughness,
        curvature_mm, shadow, true, is_display, physical, skin_body,
    );
}

fn nvf_sss_directional_skin(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    world_normal: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    F0_dielectric: vec3<f32>,
    perceptual_roughness: f32,
    curvature_mm: f32,
    shadow: f32,
    view_z: f32,
    has_dir_shadow: bool,
    macro_vis: f32,
    is_display: bool,
    physical: bool,
    skin_body: f32,
) -> vec3<f32> {
    let light = &view_bindings::lights.directional_lights[light_id];
    let L = (*light).direction_to_light.xyz;
    let light_rad = select(
        nvf_sss_light_radiance((*light).color.rgb, is_display, false),
        (*light).color.rgb * (1.0 / PI),
        physical,
    );
    let ng = nvf_sss_safe_normalize(world_normal, N);
    var thickness_mm = -1.0;
    if has_dir_shadow && dot(-ng, L) > 0.0 {
        thickness_mm = nvf_dir_skin_thickness_mm(light_id, P, ng, view_z);
    }
    return nvf_skin_eval(
        N, world_normal, V, L, diffuse_color, F0_dielectric, perceptual_roughness,
        curvature_mm, shadow, thickness_mm, macro_vis, light_rad, physical, skin_body,
    );
}

// Energy-conserving two-sided foliage BSDF (wrapped cuticle Fresnel entry/exit,
// desert-warm Beer-Lambert absorption, normalized Henyey-Greenstein phase, and grazing rim sheen).
fn nvf_foliage_direct(
    N: vec3<f32>,
    V: vec3<f32>,
    L: vec3<f32>,
    diffuse_color: vec3<f32>,
    shadow: f32,
    physical: bool,
) -> vec3<f32> {
    let s = saturate(shadow);
    let back_wrap = saturate((-dot(N, L) + 0.42) / 1.42);
    let ndotv = max(abs(dot(N, V)), 0.0001);
    let t_in = vec3(1.0) - fresnel(vec3(0.04), max(back_wrap, 0.15));
    let t_out = vec3(1.0) - fresnel(vec3(0.04), ndotv);
    let path_len = 0.42 / max(back_wrap, 0.30);
    let beer = exp(-vec3(0.18, 0.16, 1.15) * path_len);

    let cos_a = dot(V, -L);
    let g = 0.58;
    let denom = max(1.0 + g * g - 2.0 * g * cos_a, 0.05);
    let hg = (1.0 - g * g) / (2.0 * denom * sqrt(denom));
    let phase = mix(0.65, min(hg, 3.2), 0.58);

    let tau = select(0.26, 0.38, physical);
    let transmission = diffuse_color * (tau * back_wrap * phase * s) * (t_in * t_out * beer);

    let rim_grazing = pow(1.0 - saturate(ndotv), 2.5);
    let forward_rim = pow(saturate(cos_a * 0.5 + 0.5), 3.0);
    let sheen = diffuse_color * (rim_grazing * (0.14 + 0.36 * forward_rim) * s) * vec3(1.15, 1.02, 0.78);
    return transmission + sheen;
}

fn nvf_sss_clustered_foliage(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    shadow: f32,
    is_spot: bool,
    is_display: bool,
    physical: bool,
) -> vec3<f32> {
    let light = &view_bindings::clustered_lights.data[light_id];
    let to_light = (*light).position_radius.xyz - P;
    let d2 = dot(to_light, to_light);
    let L = to_light * inverseSqrt(max(d2, 0.0001));
    var attenuation = getDistanceAttenuation(d2, (*light).color_inverse_square_range.w);
    if is_spot {
        attenuation *= nvf_sss_spot_attenuation(light_id, L);
    }
    let light_rad = nvf_sss_light_radiance((*light).color_inverse_square_range.rgb, is_display, physical) * attenuation * select(1.0, 1.10, physical);
    return nvf_foliage_direct(N, V, L, diffuse_color, shadow, physical) * light_rad;
}

fn nvf_sss_point_foliage(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    shadow: f32,
    is_display: bool,
    physical: bool,
) -> vec3<f32> {
    return nvf_sss_clustered_foliage(light_id, P, N, V, diffuse_color, shadow, false, is_display, physical);
}

fn nvf_sss_spot_foliage(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    shadow: f32,
    is_display: bool,
    physical: bool,
) -> vec3<f32> {
    return nvf_sss_clustered_foliage(light_id, P, N, V, diffuse_color, shadow, true, is_display, physical);
}

fn nvf_sss_directional_foliage(
    light_id: u32,
    N: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    shadow: f32,
    is_display: bool,
    physical: bool,
) -> vec3<f32> {
    let light = &view_bindings::lights.directional_lights[light_id];
    let L = (*light).direction_to_light.xyz;
    let light_rad = select(
        nvf_sss_light_radiance((*light).color.rgb, is_display, false),
        (*light).color.rgb * (1.0 / PI),
        physical,
    );
    return nvf_foliage_direct(N, V, L, diffuse_color, shadow, physical) * light_rad;
}

// ---------------------------------------------------------------------------------------
// NVF_HAIR (hair-shader shapes: hair, beards, brows; NVF_SSS=1 with NVF_PHYSICAL_PBR=1):
// a strand BSDF in place of Bevy's isotropic one, along the strand tangent the fragment
// reads from the hair texture's own strokes (nif_material.wesl `hair_strand`, passed in
// PbrInput::anisotropy_T). Marschner et al. (2003) "Light Scattering from Human Hair
// Fibers": three lobes along the fibre, R (the white surface glint, shifted towards the
// root) and TRT (the coloured second highlight, shifted towards the tip), in the closed-form approximations of Karis (2016)
// "Physically Based Hair Shading in Unreal" (SIGGRAPH course notes). The diffuse is the
// light scattered between fibres: a wrapped term on the surface normal mixed with
// Kajiya & Kay (1989), tinted by the hair's own colour where it falls into shadow, the
// hair's counterpart of the skin's warm penumbra [engineering choice].

// Hair keratin's index of refraction, 1.55 [documented: Marschner et al. 2003, §4], and
// its normal-incidence reflectance ((n - 1) / (n + 1))^2.
const NVF_HAIR_F0: f32 = 0.0465;
// The cuticle scales' tilt, which shifts the highlights along the fibre (radians):
// R by -2 x, TRT by 4 x [documented: Karis 2016, after Marschner's 3 degrees].
const NVF_HAIR_SHIFT: f32 = 0.035;
// The wrap of the scattered diffuse: 0 is Lambert; at 0.5 light reaches 60 degrees past
// the terminator, softening it as the skin's scattering does [engineering choice].
const NVF_HAIR_WRAP: f32 = 0.5;
// The share of a single fibre's surface glint (R) the hair shell keeps. Marschner's lobes
// are one fibre's; the game's hair is a shell standing for many fibres that shade one
// another, and its strand direction (nif_material.wesl `hair_strand`) turns from stroke to
// stroke, which spreads the band into flecks. At 1 the glint of dark brown hair matched
// its whole diffuse on average and outshone Lambert in a quarter of the light and view
// directions; the owner's frames of 2026-11-24 read wet and grey. At 0.3 its mean is a
// quarter of the diffuse and the highlights outshine Lambert in 1 direction in 10
// [engineering choice, from a CPU copy of this function].
const NVF_HAIR_R_SHARE: f32 = 0.3;

fn nvf_hair_g(b: f32, t: f32) -> f32 {
    return exp(-0.5 * t * t / (b * b)) / (2.5066283 * b);
}

fn nvf_hair_fresnel(cos_t: f32) -> f32 {
    let m = 1.0 - saturate(cos_t);
    let m2 = m * m;
    return NVF_HAIR_F0 + (1.0 - NVF_HAIR_F0) * m2 * m2 * m;
}

// The hair's BSDF times the cosine for one light, per unit of light colour (the diffuse
// part carries its 1 / pi). T: the strand tangent, root to tip; N: the shading normal.
fn nvf_hair_eval(T_in: vec3<f32>, N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, albedo: vec3<f32>, perceptual_roughness: f32, shadow: f32) -> vec3<f32> {
    let s = saturate(shadow);
    let t_raw = T_in - N * dot(T_in, N);
    let t_len2 = dot(t_raw, t_raw);
    let ndl = dot(N, L);
    // Diffuse: wrapped (energy-kept) on the surface normal, a third Kajiya-Kay, modulated
    // by the active NVF_DIFFUSE BRDF under Physical PBR.
    let wrapped = saturate((ndl + NVF_HAIR_WRAP) / ((1.0 + NVF_HAIR_WRAP) * (1.0 + NVF_HAIR_WRAP)));
    let luma = max(dot(albedo, vec3(0.2126, 0.7152, 0.0722)), 1e-4);
    let tint = pow(max(albedo / luma, vec3(1e-4)), vec3(1.0 - s));
    let r = clamp(perceptual_roughness, 0.18, 1.0);
    let h_hair = nvf_sss_safe_normalize(L + V, N);
    @if(NVF_PHYSICAL_PBR)
    let hair_diff_brdf = nvf_diffuse_brdf(
        albedo,
        r,
        r * r,
        vec3(NVF_HAIR_F0),
        max(dot(N, V), 0.0001),
        max(ndl, 0.0001),
        saturate(dot(N, h_hair)),
        saturate(dot(L, h_hair)),
    );
    @if(!NVF_PHYSICAL_PBR)
    let hair_diff_brdf = albedo * (1.0 / PI);
    if t_len2 < 1e-8 {
        return hair_diff_brdf * wrapped * tint * s;
    }
    let T = t_raw * inverseSqrt(t_len2);
    let kajiya = 1.0 - abs(dot(T, L));
    // Kajiya-Kay under a half-Lambert on the shell (1 facing the light, 0 facing away).
    let diffuse = hair_diff_brdf * mix(wrapped, kajiya * 0.5 * (ndl + 1.0), 0.33) * tint * s;

    let B = vec2(r * r, 2.0 * r * r);
    let sin_l = clamp(dot(T, L), -1.0, 1.0);
    let sin_v = clamp(dot(T, V), -1.0, 1.0);
    let cos_d = max(cos(0.5 * abs(asin(sin_v) - asin(sin_l))), 0.05);
    let Lp = L - sin_l * T;
    let Vp = V - sin_v * T;
    let cos_phi = dot(Lp, Vp) * inverseSqrt(dot(Lp, Lp) * dot(Vp, Vp) + 1e-4);
    let cos_half_phi = sqrt(saturate(0.5 + 0.5 * cos_phi));

    // R
    let sa = sin(-2.0 * NVF_HAIR_SHIFT);
    let ca = cos(-2.0 * NVF_HAIR_SHIFT);
    let shift_r = 2.0 * sa * (ca * cos_half_phi * sqrt(max(1.0 - sin_v * sin_v, 0.0)) + sa * sin_v);
    let m_r = nvf_hair_g(B.x * 1.4142135 * cos_half_phi + 1e-3, sin_l + sin_v - shift_r);
    let spec_r = vec3(m_r * 0.25 * cos_half_phi * nvf_hair_fresnel(sqrt(saturate(0.5 + 0.5 * dot(L, V)))));
    // No TT (light through the fibre to the far side): the hair is a shell on the scalp, so
    // the light it would carry ends in the head. Kept, it peaked at 100 times Lambert
    // wherever a light stood behind the strand [engineering choice].
    // TRT
    let m_trt = nvf_hair_g(B.y, sin_l + sin_v - 4.0 * NVF_HAIR_SHIFT);
    let f_trt = nvf_hair_fresnel(cos_d * 0.5);
    let t_trt = pow(max(albedo, vec3(1e-4)), vec3(0.8 / cos_d));
    let spec_trt = m_trt * exp(17.0 * cos_phi - 16.78) * (1.0 - f_trt) * (1.0 - f_trt) * f_trt * t_trt;

    // The lobes are one fibre's, which has no surface normal; the hair here is a shell on
    // the head, and the light reaching it falls with the shell's cosine, as Bevy's own
    // specular does. Without it, a light behind the camera (L = V: cos phi 1, sin_l + sin_v
    // near 0 on every strand across the view) put both lobes at their peak over the whole
    // shell, silhouette included, 6.8 times Lambert at N.V 0.15 for dark brown hair: a grey
    // sheen over hair that read as see-through [engineering choice, from a CPU copy].
    let shell = saturate(ndl);
    // Soften TRT wrap and add a grazing backlit rim only away from front-lit L = V, where
    // `dot(V, -L)` is negative so front lighting keeps zero grey sheen.
    let side_back = saturate(0.5 - 0.5 * dot(V, L));
    let shell_trt = mix(shell, saturate((ndl + 0.25) / 1.25), side_back);
    let rim_nv = pow(1.0 - saturate(dot(N, V)), 2.5);
    let fwd_vl = pow(saturate(dot(V, -L) * 0.5 + 0.5), 3.0);
    let side_lit = saturate((ndl + 0.55) / 1.55);
    let hair_rim = pow(max(albedo, vec3(1e-4)), vec3(0.75)) * (rim_nv * fwd_vl * side_lit * s * 0.35);
    return diffuse + (spec_r * NVF_HAIR_R_SHARE * shell + spec_trt * shell_trt) * s + hair_rim;
}

fn nvf_sss_clustered_hair(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    T: vec3<f32>,
    V: vec3<f32>,
    albedo: vec3<f32>,
    perceptual_roughness: f32,
    shadow: f32,
    is_spot: bool,
    is_display: bool,
) -> vec3<f32> {
    let light = &view_bindings::clustered_lights.data[light_id];
    let to_light = (*light).position_radius.xyz - P;
    let d2 = dot(to_light, to_light);
    let L = to_light * inverseSqrt(max(d2, 0.0001));
    var attenuation = getDistanceAttenuation(d2, (*light).color_inverse_square_range.w);
    if is_spot {
        attenuation *= nvf_sss_spot_attenuation(light_id, L);
    }
    // nvf_sss_light_radiance carries 1 / pi; the hair's BSDF carries its own.
    let light_col = nvf_sss_light_radiance((*light).color_inverse_square_range.rgb, is_display, true) * (attenuation * PI);
    return nvf_hair_eval(T, N, V, L, albedo, perceptual_roughness, shadow) * light_col;
}

fn nvf_sss_point_hair(light_id: u32, P: vec3<f32>, N: vec3<f32>, T: vec3<f32>, V: vec3<f32>, albedo: vec3<f32>, perceptual_roughness: f32, shadow: f32, is_display: bool) -> vec3<f32> {
    return nvf_sss_clustered_hair(light_id, P, N, T, V, albedo, perceptual_roughness, shadow, false, is_display);
}

fn nvf_sss_spot_hair(light_id: u32, P: vec3<f32>, N: vec3<f32>, T: vec3<f32>, V: vec3<f32>, albedo: vec3<f32>, perceptual_roughness: f32, shadow: f32, is_display: bool) -> vec3<f32> {
    return nvf_sss_clustered_hair(light_id, P, N, T, V, albedo, perceptual_roughness, shadow, true, is_display);
}

fn nvf_sss_directional_hair(light_id: u32, N: vec3<f32>, T: vec3<f32>, V: vec3<f32>, albedo: vec3<f32>, perceptual_roughness: f32, shadow: f32) -> vec3<f32> {
    let light = &view_bindings::lights.directional_lights[light_id];
    return nvf_hair_eval(T, N, V, (*light).direction_to_light.xyz, albedo, perceptual_roughness, shadow) * (*light).color.rgb;
}

// ---------------------------------------------------------------------------------------
// NVF_OUTFIT (un-authored vanilla outfits & armor under NVF_PHYSICAL_PBR): Charlie
// micro-fiber cloth sheen BRDF (Estevez & Kulla 2017, "Production Friendly Microfacet
// Sheen BRDF"). Sheen strength rides in `material.thickness` (set by `nif_material.wesl`).
// Strictly linear in `shadow` so first-person `with_shade_light` subtraction stays exact.

const NVF_CLOTH_SHEEN_ROUGH: f32 = 0.45;

fn nvf_charlie_d(roughness: f32, ndoth: f32) -> f32 {
    let r = max(roughness, 0.05);
    let inv_r = 1.0 / r;
    let cos2h = ndoth * ndoth;
    let sin2h = max(1.0 - cos2h, 0.0078125);
    return (2.0 + inv_r) * pow(sin2h, 0.5 * inv_r) / (2.0 * PI);
}

fn nvf_charlie_l(x: f32, r: f32) -> f32 {
    let t = (1.0 - r) * (1.0 - r);
    let a = mix(21.5473, 25.3245, t);
    let b = mix(3.82987, 3.32435, t);
    let c = mix(0.19823, 0.16801, t);
    let d = mix(-1.97760, -1.27393, t);
    let e = mix(-4.32054, -4.85967, t);
    return a / (1.0 + b * pow(x, c)) + d * x + e;
}

fn nvf_charlie_v(ndotv: f32, ndotl: f32) -> f32 {
    let v = clamp(ndotv, 0.001, 1.0);
    let l = clamp(ndotl, 0.001, 1.0);
    let r = NVF_CLOTH_SHEEN_ROUGH;
    let lv = select(exp(2.0 * nvf_charlie_l(0.5, r) - nvf_charlie_l(1.0 - v, r)), exp(nvf_charlie_l(v, r)), v < 0.5);
    let ll = select(exp(2.0 * nvf_charlie_l(0.5, r) - nvf_charlie_l(1.0 - l, r)), exp(nvf_charlie_l(l, r)), l < 0.5);
    return 1.0 / max((1.0 + lv + ll) * 4.0 * v * l, 1e-4);
}

fn nvf_cloth_sheen_color(diffuse_color: vec3<f32>) -> vec3<f32> {
    let luma = dot(diffuse_color, vec3(0.2126, 0.7152, 0.0722));
    let lifted = diffuse_color * 0.75 + vec3(sqrt(max(luma, 0.0)) * 0.25);
    return clamp(lifted, vec3(0.02), vec3(0.65));
}

fn nvf_outfit_sheen_eval(
    N: vec3<f32>,
    V: vec3<f32>,
    L: vec3<f32>,
    diffuse_color: vec3<f32>,
    sheen_strength: f32,
    shadow: f32,
    light_rad: vec3<f32>,
) -> vec3<f32> {
    if sheen_strength <= 0.001 {
        return vec3(0.0);
    }
    let ndotl = dot(N, L);
    let ndotv = max(dot(N, V), 0.0001);
    let s = saturate(shadow);
    // Soft fabric terminator wrap past 90° while keeping strict linearity in `s`.
    let wrap_ndl = saturate((ndotl + 0.15) / 1.15);
    let H = nvf_sss_safe_normalize(L + V, N);
    let ndoth = saturate(dot(N, H));
    let d = nvf_charlie_d(NVF_CLOTH_SHEEN_ROUGH, ndoth);
    let v = nvf_charlie_v(ndotv, max(ndotl, 0.05));
    let sheen_col = nvf_cloth_sheen_color(diffuse_color);
    return sheen_col * (sheen_strength * d * v * wrap_ndl * s * 0.24) * light_rad;
}

fn nvf_sss_clustered_outfit(
    light_id: u32,
    P: vec3<f32>,
    N: vec3<f32>,
    V: vec3<f32>,
    diffuse_color: vec3<f32>,
    sheen_strength: f32,
    shadow: f32,
    is_spot: bool,
    is_display: bool,
) -> vec3<f32> {
    if sheen_strength <= 0.001 {
        return vec3(0.0);
    }
    let light = &view_bindings::clustered_lights.data[light_id];
    let to_light = (*light).position_radius.xyz - P;
    let d2 = dot(to_light, to_light);
    let L = to_light * inverseSqrt(max(d2, 0.0001));
    var attenuation = getDistanceAttenuation(d2, (*light).color_inverse_square_range.w);
    if is_spot {
        attenuation *= nvf_sss_spot_attenuation(light_id, L);
    }
    let light_rad = nvf_sss_light_radiance((*light).color_inverse_square_range.rgb, is_display, true) * attenuation;
    return nvf_outfit_sheen_eval(N, V, L, diffuse_color, sheen_strength, shadow, light_rad);
}

fn nvf_sss_point_outfit(light_id: u32, P: vec3<f32>, N: vec3<f32>, V: vec3<f32>, diffuse_color: vec3<f32>, sheen_strength: f32, shadow: f32, is_display: bool) -> vec3<f32> {
    return nvf_sss_clustered_outfit(light_id, P, N, V, diffuse_color, sheen_strength, shadow, false, is_display);
}

fn nvf_sss_spot_outfit(light_id: u32, P: vec3<f32>, N: vec3<f32>, V: vec3<f32>, diffuse_color: vec3<f32>, sheen_strength: f32, shadow: f32, is_display: bool) -> vec3<f32> {
    return nvf_sss_clustered_outfit(light_id, P, N, V, diffuse_color, sheen_strength, shadow, true, is_display);
}

fn nvf_sss_directional_outfit(light_id: u32, N: vec3<f32>, V: vec3<f32>, diffuse_color: vec3<f32>, sheen_strength: f32, shadow: f32) -> vec3<f32> {
    let light = &view_bindings::lights.directional_lights[light_id];
    return nvf_outfit_sheen_eval(N, V, (*light).direction_to_light.xyz, diffuse_color, sheen_strength, shadow, (*light).color.rgb * (1.0 / PI));
}

fn nvf_outfit_sheen_sky(NdotV: f32, diffuse_color: vec3<f32>, sheen_strength: f32, env_diffuse: vec3<f32>) -> vec3<f32> {
    if sheen_strength <= 0.001 {
        return vec3(0.0);
    }
    let grazing = pow(1.0 - saturate(NdotV), 4.0);
    let sheen_col = nvf_cloth_sheen_color(diffuse_color);
    let sky_irrad = min(env_diffuse / max(diffuse_color, vec3(0.06)), vec3(3.0));
    return sheen_col * sky_irrad * (sheen_strength * grazing * 0.35);
}
