//! Production PBR source preparation shared by LightsPlugin and CPU shader checks.

/// The display-value lighting sum (docs/spec-notes/world.md §8 "Light on surfaces in
/// display values") [hypothesis, fitted to the original's frames]: the game adds its
/// lights as display values (colour bytes / 255) — ambient + Σ sun and lights × fade ×
/// attenuation × N·L — and multiplies the texture by the sum, where Bevy adds linear
/// light. In a world view (marked by a negative ambient alpha, `lights::display_ambient`) the
/// patched `apply_pbr_lighting` leaves Bevy's diffuse terms out (diffuse colour 0 for the
/// lights and the ambient light, so only their specular parts stay), forms the sum from
/// the same lights' colours, which hold display values there (`lights::display_color`), and
/// adds diffuse colour × sum^2.2 (the display gamma) as light. Views without the mark
/// (the minigame views) keep Bevy's sum. `nvf_apply_pbr_lighting` takes weights for the
/// ambient and the direct part of the sum (grass.wesl); `apply_pbr_lighting` is it with
/// weights 1. Each helper carries the condition Bevy puts on `apply_pbr_lighting`
/// (`@if(!PREPASS_FRAGMENT)`), whose line the first edit takes in.
const DISPLAY_HELPERS: &str = "// NVForever (nvforever lights.rs DISPLAY_HELPERS): the game's lighting sum in display values.
@if(!PREPASS_FRAGMENT)
fn nvf_display_lighting() -> bool {
    return view_bindings::lights.ambient_color.a < 0.0;
}

// A clustered point or spot light's diffuse light on a surface at P facing N, as display
// values: colour × fade × attenuation × N.L (the uniform holds colour × fade × pi × light
// unit, nvforever lights.rs `lumens`).
@if(!PREPASS_FRAGMENT)
fn nvf_point_display(light_id: u32, P: vec3<f32>, N: vec3<f32>) -> vec3<f32> {
    let light = &view_bindings::clustered_lights.data[light_id];
    let to_light = (*light).position_radius.xyz - P;
    let d2 = dot(to_light, to_light);
    let attenuation = lighting::getDistanceAttenuation(d2, (*light).color_inverse_square_range.w);
    let n_dot_l = saturate(dot(N, to_light * inverseSqrt(max(d2, 0.0001))));
    return (*light).color_inverse_square_range.rgb * (attenuation * n_dot_l * view_bindings::view.exposure / 3.14159265);
}

// The same for a spot light, with Bevy's cone falloff (bevy_pbr::lighting::spot_light).
@if(!PREPASS_FRAGMENT)
fn nvf_spot_display(light_id: u32, P: vec3<f32>, N: vec3<f32>) -> vec3<f32> {
    let light = &view_bindings::clustered_lights.data[light_id];
    var spot_dir = vec3<f32>((*light).light_custom_data.x, 0.0, (*light).light_custom_data.y);
    spot_dir.y = sqrt(max(0.0, 1.0 - spot_dir.x * spot_dir.x - spot_dir.z * spot_dir.z));
    if ((*light).flags & mesh_view_types::POINT_LIGHT_FLAGS_SPOT_LIGHT_Y_NEGATIVE) != 0u {
        spot_dir.y = -spot_dir.y;
    }
    let cd = dot(-spot_dir, normalize((*light).position_radius.xyz - P));
    let cone = saturate(cd * (*light).light_custom_data.z + (*light).light_custom_data.w);
    return nvf_point_display(light_id, P, N) * (cone * cone);
}

// What is left of Bevy's ambient light in a world view, its highlight, takes the ambient
// colour as linear light: the factor from the display value to it (a^2.2 / a).
@if(!PREPASS_FRAGMENT)
fn nvf_ambient_ratio() -> vec3<f32> {
    return pow(max(view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure, vec3(1e-6)), vec3(1.2));
}

// A directional light's diffuse light on a surface facing N, as display values.
@if(!PREPASS_FRAGMENT)
fn nvf_directional_display(i: u32, N: vec3<f32>) -> vec3<f32> {
    let light = &view_bindings::lights.directional_lights[i];
    return (*light).color.rgb * (saturate(dot(N, (*light).direction_to_light.xyz)) * view_bindings::view.exposure / 3.14159265);
}

@if(!PREPASS_FRAGMENT)
fn apply_pbr_lighting(
    in: pbr_types::PbrInput,
) -> vec4<f32> {
    return nvf_apply_pbr_lighting(in, vec2(1.0));
}

@if(!PREPASS_FRAGMENT)
fn nvf_apply_pbr_lighting(
    in: pbr_types::PbrInput,
    nvf_weights: vec2<f32>,
) -> vec4<f32> {";

/// Replacements in `apply_pbr_lighting` [observed: bevy_pbr 0.20.1 `pbr_functions.wesl`],
/// in order; the light loops' line occurs three times (point, spot, directional).
const DISPLAY_EDITS: [(&str, &str); 7] = [
    ("@if(!PREPASS_FRAGMENT)\nfn apply_pbr_lighting(\n    in: pbr_types::PbrInput,\n) -> vec4<f32> {", DISPLAY_HELPERS),
    (
        "    lighting_input.diffuse_color = diffuse_color;",
        "    let nvf_display = nvf_display_lighting();
    var nvf_direct = vec3(0.0);
    lighting_input.diffuse_color = select(diffuse_color, vec3(0.0), nvf_display);",
    ),
    (
        "        direct_light += light_contrib * shadow;",
        "        direct_light += light_contrib * shadow;
        if nvf_display { nvf_direct += nvf_point_display(light_id, in.world_position.xyz, in.N) * shadow; }",
    ),
    (
        "        direct_light += light_contrib * shadow;",
        "        direct_light += light_contrib * shadow;
        if nvf_display { nvf_direct += nvf_spot_display(light_id, in.world_position.xyz, in.N) * shadow; }",
    ),
    (
        "        direct_light += light_contrib * shadow;",
        "        direct_light += light_contrib * shadow;
        if nvf_display { nvf_direct += nvf_directional_display(i, in.N) * shadow; }",
    ),
    (
        "        indirect_light += ambient::ambient_light(in.world_position, in.N, in.V, NdotV, diffuse_color, F0, perceptual_roughness, diffuse_occlusion);",
        "        indirect_light += ambient::ambient_light(in.world_position, in.N, in.V, NdotV, select(diffuse_color, vec3(0.0), nvf_display), F0, perceptual_roughness, diffuse_occlusion) * select(vec3(1.0), nvf_ambient_ratio(), nvf_display);",
    ),
    (
        "    // Total light\n",
        "    if nvf_display {
        let nvf_ambient = view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure * diffuse_occlusion;
        let nvf_sum = max(nvf_weights.x * nvf_ambient + nvf_weights.y * nvf_direct, vec3(0.0));
        // Held to 1 per channel, as the original's lit colour is (lights.rs GAME_CLIP).
        direct_light += min(diffuse_color * pow(nvf_sum, vec3(2.2)), vec3(1.0)) / view_bindings::view.exposure;
    }

    // Total light\n",
    ),
];

/// `source` with [`DISPLAY_EDITS`] made, each to the first occurrence after the last
/// edit; `None` when one is missing (Bevy changed).
pub(super) fn display_patched(source: &str) -> Option<String> {
    let mut out = String::with_capacity(source.len() + 4096);
    let mut rest = source;
    for (from, to) in DISPLAY_EDITS {
        let at = rest.find(from)?;
        out.push_str(&rest[..at]);
        out.push_str(to);
        rest = &rest[at + from.len()..];
    }
    out.push_str(rest);
    Some(out)
}

/// Bevy's shader text with Windows line ends made `\n`: the patches' anchors are written
/// with `\n`. A checkout with git's `core.autocrlf` gave the bevy_pbr vendored until #939
/// `\r\n` (issue #946: no patch matched, so grass lost `nvf_apply_pbr_lighting`); Bevy
/// now comes from crates.io as published, and this stays as a guard for any patched copy.
pub fn unix_lines(source: &str) -> String {
    source.replace("\r\n", "\n")
}
