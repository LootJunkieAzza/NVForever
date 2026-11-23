//! Subsurface scattering text hook in bevy_pbr's `pbr_functions` (`sss.rs`; `NVF_SSS=1`).
//! Plain text and the standard library only, so `nvf-shader-tests` composes the very
//! same edits before validating skin and foliage shaders.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
    Replace,
}

const IMPORT_ANCHOR: &str = "import bevy_render::maths::{E, powsafe};\n";
const IMPORT_TEXT: &str = "// NVForever subsurface scattering (nvforever sss.rs).
@if(NVF_SKIN_SSS) import nvforever::sss::{nvf_sss_point_skin, nvf_sss_spot_skin, nvf_sss_directional_skin};
@if(NVF_FOLIAGE_SSS) import nvforever::sss::{nvf_sss_point_foliage, nvf_sss_spot_foliage, nvf_sss_directional_foliage};
";

const PHYSICAL_DIFFUSE_ANCHOR: &str = "    lighting_input.diffuse_color = diffuse_color;\n";
const PHYSICAL_DIFFUSE_SSS_TEXT: &str = "    @if(!NVF_SKIN_SSS && !NVF_FOLIAGE_SSS)
    lighting_input.diffuse_color = diffuse_color;
    @if(NVF_SKIN_SSS)
    lighting_input.diffuse_color = vec3(0.0);
    @if(NVF_FOLIAGE_SSS)
    lighting_input.diffuse_color = diffuse_color * 0.58;
";

const INDIRECT_RESTORE_ANCHOR: &str = "    var indirect_light = vec3(0.0f);\n";
const INDIRECT_RESTORE_TEXT: &str = "    // Restore unattenuated diffuse albedo for indirect IBL before environment_map_light.
    @if(NVF_SKIN_SSS)
    lighting_input.diffuse_color = diffuse_color;
    @if(NVF_FOLIAGE_SSS)
    lighting_input.diffuse_color = diffuse_color;
";

const POINT_LOOP_ANCHOR: &str = "    // Point lights (direct)\n";
const FOLIAGE_WEIGHT_DISPLAY: &str = "    // NVForever subsurface scattering (nvforever sss.rs): match grass direct-light weight.
    @if(NVF_FOLIAGE_SSS)
    let nvf_sss_weight = select(1.0, pow(max(nvf_weights.y, 0.0), 2.2), nvf_display);
";
const FOLIAGE_WEIGHT_PHYSICAL: &str = "    // NVForever subsurface scattering (nvforever sss.rs): direct_light *= nvf_w.y follows below.
    @if(NVF_FOLIAGE_SSS)
    let nvf_sss_weight = 1.0;
";

const POINT_ANCHOR: &str = "        let light_contrib = lighting::point_light(light_id, &lighting_input, enable_diffuse, true);\n";
const SPOT_ANCHOR: &str = "        let light_contrib = lighting::spot_light(light_id, &lighting_input, enable_diffuse);\n";
const DIR_ANCHOR: &str = "        var light_contrib = lighting::directional_light(i, &lighting_input, enable_diffuse);\n";

fn clustered_sss_text(kind: &str, ao_var: &str, physical: bool) -> String {
    let p = if physical { "true" } else { "false" };
    format!(
        "        @if(NVF_SKIN_SSS && NVF_AMBIENT_OCCLUSION)
        let nvf_skin_{ao_var}_ao = select(nvf_ao_lamps, vec3(1.0), !nvf_display || ((in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::clustered_lights.data[light_id].flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u));
        @if(NVF_SKIN_SSS && !NVF_AMBIENT_OCCLUSION)
        let nvf_skin_{ao_var}_ao = vec3(1.0);
        @if(NVF_SKIN_SSS)
        if enable_diffuse {{
            direct_light += nvf_sss_{kind}_skin(light_id, in.world_position.xyz, in.N, in.world_normal, in.V, diffuse_color, lighting_input.F0_dielectric, perceptual_roughness, thickness, shadow, nvf_display, {p}) * nvf_skin_{ao_var}_ao;
        }}
        @if(NVF_FOLIAGE_SSS && NVF_AMBIENT_OCCLUSION)
        let nvf_foliage_{ao_var}_ao = select(nvf_ao_lamps, vec3(1.0), !nvf_display || ((in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::clustered_lights.data[light_id].flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u));
        @if(NVF_FOLIAGE_SSS && !NVF_AMBIENT_OCCLUSION)
        let nvf_foliage_{ao_var}_ao = vec3(1.0);
        @if(NVF_FOLIAGE_SSS)
        if enable_diffuse {{
            direct_light += nvf_sss_{kind}_foliage(light_id, in.world_position.xyz, in.N, in.V, diffuse_color, shadow, nvf_display, {p}) * nvf_sss_weight * nvf_foliage_{ao_var}_ao;
        }}
"
    )
}

fn directional_sss_text(physical: bool) -> String {
    let p = if physical { "true" } else { "false" };
    format!(
        "        @if(NVF_SKIN_SSS)
        let nvf_skin_has_shadow = (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::lights.directional_lights[i].flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u;
        @if(NVF_SKIN_SSS && !NVF_SUN_VISIBILITY)
        let nvf_skin_macro_vis = 1.0;
        @if(NVF_SKIN_SSS && NVF_SUN_VISIBILITY)
        let nvf_skin_macro_vis = select(1.0, nvf_sun_visibility(in.world_position.xyz, (*light).direction_to_light, 1.0, view_z), ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT) != 0u);
        @if(NVF_SKIN_SSS)
        if enable_diffuse {{
            direct_light += nvf_sss_directional_skin(i, in.world_position.xyz, in.N, in.world_normal, in.V, diffuse_color, lighting_input.F0_dielectric, perceptual_roughness, thickness, shadow, view_z, nvf_skin_has_shadow, nvf_skin_macro_vis, nvf_display, {p});
        }}
        @if(NVF_FOLIAGE_SSS)
        let nvf_fol_has_shadow = (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::lights.directional_lights[i].flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u && (in.flags & NVF_MESH_FLAGS_NO_DIRECTIONAL_SHADOWS_BIT) == 0u;
        @if(NVF_FOLIAGE_SSS)
        var nvf_fol_shadow = shadow;
        @if(NVF_FOLIAGE_SSS)
        if nvf_fol_has_shadow && dot(in.N, (*light).direction_to_light.xyz) < 0.0 {{
            var nvf_back_sh = shadows::fetch_directional_shadow(i, in.world_position, -in.world_normal, view_z, in.frag_coord.xy) * in.directional_shadow_factor;
            @if(NVF_SUN_VISIBILITY)
            if ((*light).flags & mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT) != 0u {{
                nvf_back_sh = nvf_sun_visibility(in.world_position.xyz, (*light).direction_to_light, nvf_back_sh, view_z);
            }}
            nvf_fol_shadow = max(nvf_fol_shadow, nvf_back_sh);
        }}
        @if(NVF_FOLIAGE_SSS)
        if enable_diffuse {{
            direct_light += nvf_sss_directional_foliage(i, in.N, in.V, diffuse_color, nvf_fol_shadow, nvf_display, {p}) * nvf_sss_weight;
        }}
"
    )
}

/// Applies the `NVF_SSS=1` edits to `source` (already patched by `display_patched` or
/// `physical_patched`, plus `sun_visibility` and `ssao_lighting` hooks). Returns `None` if
/// any expected anchor is missing.
pub fn hook_patched(source: &str, physical: bool) -> Option<String> {
    let point_text = clustered_sss_text("point", "pt", physical);
    let spot_text = clustered_sss_text("spot", "sp", physical);
    let dir_text = directional_sss_text(physical);

    let mut edits: Vec<(&str, &str, Place)> = Vec::with_capacity(7);
    edits.push((IMPORT_ANCHOR, IMPORT_TEXT, Place::After));
    if physical {
        edits.push((PHYSICAL_DIFFUSE_ANCHOR, PHYSICAL_DIFFUSE_SSS_TEXT, Place::Replace));
    }
    edits.push((
        POINT_LOOP_ANCHOR,
        if physical { FOLIAGE_WEIGHT_PHYSICAL } else { FOLIAGE_WEIGHT_DISPLAY },
        Place::Before,
    ));
    edits.push((POINT_ANCHOR, &point_text, Place::After));
    edits.push((SPOT_ANCHOR, &spot_text, Place::After));
    edits.push((DIR_ANCHOR, &dir_text, Place::After));
    if physical {
        edits.push((INDIRECT_RESTORE_ANCHOR, INDIRECT_RESTORE_TEXT, Place::Before));
    }

    let mut out = String::with_capacity(source.len() + 3072);
    let mut rest = source;
    for (anchor, text, place) in edits {
        let at = rest.find(anchor)?;
        out.push_str(&rest[..at]);
        match place {
            Place::Before => {
                out.push_str(text);
                out.push_str(anchor);
            }
            Place::After => {
                out.push_str(anchor);
                out.push_str(text);
            }
            Place::Replace => out.push_str(text),
        }
        rest = &rest[at + anchor.len()..];
    }
    out.push_str(rest);
    Some(out)
}
