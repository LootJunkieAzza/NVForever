//! Fog shader patch constants shared by `FogPlugin` (`fog.rs`) and `nvf-shader-tests`.

/// Bevy's atmospheric fog factors [observed: bevy_pbr 0.20.1 `fog.wesl`].
pub const BEVY_ATMOSPHERIC: &str = "    let extinction_factor = 1.0 - 1.0 / exp(distance * fog_params.be);
    let inscattering_factor = 1.0 - 1.0 / exp(distance * fog_params.bi);
    return vec4<f32>(
        input_color.rgb * (1.0 - extinction_factor * fog_color.a)
            + fog_color.rgb * inscattering_factor * fog_color.a,
        input_color.a
    );";

/// The game's fog: near, far and power in `be` (the atmospheric fog's extinction). With
/// `bi.x` 1 the colour is blended in display values (gamma 2.2), else in linear light.
pub const GAME_FOG: &str = "    let fog_t = clamp((distance - fog_params.be.x) / max(fog_params.be.y - fog_params.be.x, 1.0), 0.0, 1.0);
    let fog_share = select(0.0, pow(fog_t, fog_params.be.z), fog_t > 0.0) * fog_color.a;
    if fog_params.bi.x > 0.5 {
        if scattering.x > 0.0 && fog_params.directional_light_color.a <= 0.0 {
            // NVForever desert haze (haze.rs): total share 1 - (1 - fog)(1 - haze), one blend.
            let haze_share = clamp(scattering.x, 0.0, 1.0);
            let haze_colour = mix(fog_color.rgb, fog_params.directional_light_color.rgb, clamp(scattering.y, 0.0, 1.0));
            let veil_share = 1.0 - (1.0 - fog_share) * (1.0 - haze_share);
            let veil = (pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)) * fog_share + pow(max(haze_colour, vec3(0.0)), vec3(1.0 / 2.2)) * (haze_share * (1.0 - fog_share))) / max(veil_share, 1e-6);
            let hazy = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), veil, veil_share);
            return vec4<f32>(pow(hazy, vec3(2.2)), input_color.a);
        }
        let shown = mix(pow(max(input_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), pow(max(fog_color.rgb, vec3(0.0)), vec3(1.0 / 2.2)), fog_share);
        return vec4<f32>(pow(shown, vec3(2.2)), input_color.a);
    }
    return vec4<f32>(mix(input_color.rgb, fog_color.rgb, fog_share), input_color.a);";

/// Physical linear Beer-Lambert fog indoors and wavelength-dependent Rayleigh (`lambda^-4`) +
/// Mie (Henyey-Greenstein `g = 0.72`) desert atmospheric scattering outdoors (`NVF_PHYSICAL_PBR=1`).
/// Optical depth is linear in distance (no near-camera concave `fog_power < 1` wall) so clear
/// desert foregrounds stay crisp while distant ridges pick up sky-blue Rayleigh aerial
/// perspective and warm solar Mie forward scatter; thick/stormy weathers (small `far`) retain
/// full Beer-Lambert haze and optional `desert-haze` transmittance composition.
pub const PHYSICAL_FOG: &str = "    let d = max(distance - fog_params.be.x, 0.0);
    let range = max(fog_params.be.y - fog_params.be.x, 1.0);
    if fog_params.bi.x <= 0.5 {
        let tau = (d / range) * 1.6;
        let fog_share = (1.0 - exp(-tau)) * fog_color.a;
        return vec4<f32>(mix(input_color.rgb, fog_color.rgb, fog_share), input_color.a);
    }
    let has_haze = fog_params.bi.z > 0.0 && fog_params.directional_light_color.a <= 0.0;
    let sun_cos = select(clamp(scattering.x, -1.0, 1.0), pow(max(clamp(scattering.y, 0.0, 1.0), 1e-5), 0.125), has_haze);
    let sun_elev = select(scattering.y, 0.25, has_haze);
    let ray_elev = select(scattering.z, 0.0, has_haze);
    let h_scale = exp(-max(ray_elev, 0.0) * 1.8);
    let d_km = d / 70000.0;
    let tau_R = vec3<f32>(0.052, 0.122, 0.300) * (d_km * h_scale);
    let mie_weight = clamp(1.0 - smoothstep(25000.0, 120000.0, range), 0.18, 1.0);
    let tau_M = vec3<f32>(0.86, 0.93, 1.0) * ((d / max(range, 1000.0)) * (1.75 * mie_weight) * h_scale);
    let fog_lum = dot(fog_color.rgb, vec3<f32>(0.2126, 0.7152, 0.0722));
    let day_factor = smoothstep(-0.04, 0.18, sun_elev) * smoothstep(0.015, 0.12, fog_lum);
    let phase_R = 0.75 * (1.0 + sun_cos * sun_cos);
    let g = 0.72;
    let hg_denom = max(1.0 + g * g - 2.0 * g * sun_cos, 0.06);
    let phase_M = (1.0 - g * g) / (hg_denom * sqrt(hg_denom));
    let rayleigh_tint = mix(vec3<f32>(0.96, 0.80, 0.66), vec3<f32>(0.58, 0.80, 1.22), smoothstep(0.02, 0.32, sun_elev));
    let L_R = fog_color.rgb * rayleigh_tint * mix(0.85, phase_R, day_factor);
    let L_M = fog_color.rgb * (0.78 + 0.30 * day_factor * min(phase_M, 4.5));
    let tau_sum = tau_R + tau_M;
    let tau = tau_sum * fog_color.a;
    let T_atm = exp(-tau);
    let inscatter_col = (L_R * tau_R + L_M * tau_M) / max(tau_sum, vec3<f32>(1e-5));
    var result = input_color.rgb * T_atm + inscatter_col * (vec3<f32>(1.0) - T_atm);
    if has_haze && scattering.x > 0.0 {
        let haze_share = clamp(scattering.x, 0.0, 1.0);
        let haze_colour = mix(fog_color.rgb, fog_params.directional_light_color.rgb, clamp(scattering.y, 0.0, 1.0));
        result = result * (1.0 - haze_share) + haze_colour * haze_share;
    }
    return vec4<f32>(result, input_color.a);";

/// Patches `bevy_pbr/render/fog.wesl` for physical linear fog (`physical = true`) or the
/// game's curve (`physical = false`). Returns `None` if `BEVY_ATMOSPHERIC` is missing.
#[allow(dead_code)]
pub fn fog_patched(source: &str, physical: bool) -> Option<String> {
    let source = source.replace("\r\n", "\n");
    if !source.contains(BEVY_ATMOSPHERIC) {
        return None;
    }
    Some(source.replace(BEVY_ATMOSPHERIC, if physical { PHYSICAL_FOG } else { GAME_FOG }))
}
