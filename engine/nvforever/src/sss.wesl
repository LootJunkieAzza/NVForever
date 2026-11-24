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
import bevy_render::maths::PI;

// NVF_SKIN_SPLIT: the diffuse light of the last nvf_skin_eval, the share the screen-space
// scattering blurs (sss/screen.rs). The lighting hook (sss/hook.rs) adds it up per light.
var<private> nvf_skin_last_diffuse: vec3<f32>;

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

fn nvf_preintegrated_skin(ndotl_rgb: vec3<f32>, curvature_mm: f32) -> vec3<f32> {
    let k = clamp(curvature_mm, 0.0, 0.45);
    let w1 = vec3(0.451, 0.791, 0.649);
    let w2 = vec3(0.549, 0.209, 0.351);
    let s1 = vec3(0.20, 0.14, 0.08) * k;
    let s2 = vec3(1.55, 0.50, 0.24) * k;
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
const NVF_SKIN_PENUMBRA_SIGMA: vec3<f32> = vec3(0.20, 0.10, 0.06);

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
// is then blurred by its scattering width. Held so that 0 stays 0 and 1 stays 1; the blur
// is symmetric, so what the dark side gains the lit side's edge loses.
@if(!SHADE_LIGHT) fn nvf_skin_shadow(shadow: f32, curvature_mm: f32) -> vec3<f32> {
    let x = (saturate(shadow) - 0.5) * 2.0 + 0.5;
    let g = nvf_skin_soft_ramp(vec3(x), NVF_SKIN_PENUMBRA_SIGMA);
    let g0 = nvf_skin_soft_ramp(vec3(-0.5), NVF_SKIN_PENUMBRA_SIGMA);
    let g1 = nvf_skin_soft_ramp(vec3(1.5), NVF_SKIN_PENUMBRA_SIGMA);
    return saturate((g - g0) / max(g1 - g0, vec3(1e-4)));
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

// Dual-lobe skin specular: tight sebum/oil lobe (IOR 1.4, F0 = 0.028, weight 0.24)
// layered over the base epidermal GGX lobe. Returns vec4(spec_rgb, F_oil_r).
fn nvf_skin_sebum_specular(N: vec3<f32>, V: vec3<f32>, L: vec3<f32>, perceptual_roughness: f32) -> vec4<f32> {
    let NdotL = dot(N, L);
    let NdotV = max(dot(N, V), 0.0001);
    if NdotL <= 0.0 {
        return vec4(0.0);
    }
    let H = nvf_sss_safe_normalize(L + V, N);
    let NdotH = saturate(dot(N, H));
    let LdotH = saturate(dot(L, H));
    let r_oil = clamp(perceptual_roughness * 0.68, 0.38, 0.52);
    let a_oil = r_oil * r_oil;
    let F_oil = fresnel(vec3(0.028), LdotH);
    let spec = D_GGX(a_oil, NdotH) * V_SmithGGXCorrelated(a_oil, NdotV, NdotL) * F_oil * NdotL * 0.06;
    return vec4(spec, F_oil.r);
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
) -> vec3<f32> {
    let ng = nvf_sss_safe_normalize(world_normal, N);
    let n_r = nvf_sss_safe_normalize(mix(N, ng, 0.08), N);
    let n_g = nvf_sss_safe_normalize(mix(N, ng, 0.04), N);
    let n_b = N;
    let ndotl_rgb = vec3(dot(n_r, L), dot(n_g, L), dot(n_b, L));
    let s = saturate(shadow);
    // NVF_SKIN_SPLIT: the screen-space pass blurs this light across the skin by the
    // diffusion profile (sss/screen.rs), which does what the pre-integration, the blurred
    // normals and the warm penumbra stand in for; done twice the skin would blur twice.
    @if(!NVF_SKIN_SPLIT) let preint = nvf_preintegrated_skin(ndotl_rgb, curvature_mm);
    @if(!NVF_SKIN_SPLIT) let shadow_rgb = nvf_skin_shadow(s, curvature_mm);
    @if(NVF_SKIN_SPLIT) let preint = vec3(saturate(dot(N, L)));
    @if(NVF_SKIN_SPLIT) let shadow_rgb = vec3(s);

    let phase_fwd = 0.35 + 0.65 * pow(saturate(dot(V, -L)), 3.0);
    let d_est = clamp(0.65 / max(curvature_mm, 0.04), 3.5, 18.0);
    let eff_mm = select(d_est, max(thickness_mm, d_est), thickness_mm >= 0.0);
    let rim_grazing = pow(1.0 - saturate(dot(N, V)), 2.0);
    let back_wrap = saturate(dot(-N, L) * 0.5 + 0.25) * (0.35 + 0.65 * rim_grazing);
    let back_scatter = nvf_skin_transmittance_profile(eff_mm) * (back_wrap * phase_fwd * s * saturate(macro_vis) * 0.45);

    let sebum = nvf_skin_sebum_specular(N, V, L, perceptual_roughness);
    let spec_oil = sebum.xyz * (s * PI);
    let skin_scatter = preint * shadow_rgb + back_scatter;

    if physical {
        let t_base = nvf_sss_dielectric_T(F0_dielectric, saturate(perceptual_roughness), max(dot(N, V), 0.0001), max(dot(N, L), 0.0001));
        let t_oil = 1.0 - 0.06 * sebum.w;
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
        curvature_mm, shadow, -1.0, shadow, light_rad, physical,
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
) -> vec3<f32> {
    return nvf_sss_clustered_skin(
        light_id, P, N, world_normal, V, diffuse_color, F0_dielectric, perceptual_roughness,
        curvature_mm, shadow, false, is_display, physical,
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
) -> vec3<f32> {
    return nvf_sss_clustered_skin(
        light_id, P, N, world_normal, V, diffuse_color, F0_dielectric, perceptual_roughness,
        curvature_mm, shadow, true, is_display, physical,
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
        curvature_mm, shadow, thickness_mm, macro_vis, light_rad, physical,
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
    // Diffuse: wrapped (energy-kept) on the surface normal, a third Kajiya-Kay.
    let wrapped = saturate((ndl + NVF_HAIR_WRAP) / ((1.0 + NVF_HAIR_WRAP) * (1.0 + NVF_HAIR_WRAP)));
    let luma = max(dot(albedo, vec3(0.2126, 0.7152, 0.0722)), 1e-4);
    let tint = pow(max(albedo / luma, vec3(1e-4)), vec3(1.0 - s));
    if t_len2 < 1e-8 {
        return albedo * (1.0 / PI) * wrapped * tint * s;
    }
    let T = t_raw * inverseSqrt(t_len2);
    let kajiya = 1.0 - abs(dot(T, L));
    let diffuse = albedo * (1.0 / PI) * mix(wrapped, kajiya * 0.5 * saturate(ndl + 1.0), 0.33) * tint * s;

    let r = clamp(perceptual_roughness, 0.18, 1.0);
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

    return diffuse + (spec_r * NVF_HAIR_R_SHARE + spec_trt) * s;
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
