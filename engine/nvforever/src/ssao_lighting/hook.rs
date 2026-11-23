//! The ambient occlusion's text hook in bevy_pbr's `pbr_functions` (ssao_lighting.rs module
//! notes). Plain text and the standard library only, so the shader compile checks
//! (engine/nvf-shader-tests) apply the very same edits before composing the shaders that
//! read the occlusion.

/// Where an edit's text goes against its anchor.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Place {
    Before,
    After,
    Replace,
}

/// The display-value ambient line of lights/display_shader.rs `DISPLAY_EDITS`, as it
/// stands in the patched `apply_pbr_lighting`.
pub const AMBIENT_LINE: &str = "        let nvf_ambient = view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure * diffuse_occlusion;\n";

/// The same line kept without the def, and its twin with the occlusion under it, per colour
/// channel by the multi-bounce fit with the surface's diffuse colour
/// (ssao_lighting.wesl `nvf_ao_multibounce`; `diffuse_color` is `apply_pbr_lighting`'s own,
/// declared before the lights). `-view_z`
/// is the fragment's view distance from its own world position (so the first-person arms,
/// drawn at a squeezed depth, still compare their real distance with the texels', which the
/// occlusion pass unsqueezed).
pub const AMBIENT_LINE_HOOKED: &str = "        // NVForever ambient occlusion (nvforever ssao_lighting.rs): on the display-value
        // ambient alone, never on the direct lights nor on Bevy's ambient highlight above.
        @if(!NVF_AMBIENT_OCCLUSION)
        let nvf_ambient = view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure * diffuse_occlusion;
        @if(NVF_AMBIENT_OCCLUSION)
        let nvf_ambient = view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure * diffuse_occlusion * nvf_ao_multibounce(nvf_ao_ambient, diffuse_color);
";

/// The start of `apply_pbr_lighting`'s point light loop, after `view_z` is declared.
pub const LAMP_ANCHOR: &str = "    // Point lights (direct)\n";

/// Once per fragment, before the light loops: the ambient's factor (the one read of the
/// occlusion texture; the ambient line below uses it) and from it the factor on the light of
/// lamps that cast no shadow here (`NVF_PLAY_DEBUG_AO_LAMPS`), `mix(1, g, w)`.
pub const LAMP_TEXT: &str = "    // NVForever ambient occlusion (nvforever ssao_lighting.rs): the ambient's factor, read
    // once, and from it the factor on the light of lamps that cast no shadow here (1 while
    // NVF_PLAY_DEBUG_AO_LAMPS is 0). First-person shapes (the arms, the weapon, the Pip-Boy;
    // nvf_assets nif_material.rs FIRST_PERSON_DEPTH) take none: only the world is occluded.
    @if(NVF_AMBIENT_OCCLUSION && !FIRST_PERSON_DEPTH)
    let nvf_ao_ambient = nvf_ambient_visibility(in.frag_coord.xy, -view_z);
    @if(NVF_AMBIENT_OCCLUSION && FIRST_PERSON_DEPTH)
    let nvf_ao_ambient = 1.0;
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_ao_lamps = nvf_ao_multibounce(nvf_lamp_visibility(nvf_ao_ambient), diffuse_color);

";

/// A point light's part of the display-value sum (lights/display_shader.rs `DISPLAY_EDITS`).
pub const POINT_LINE: &str = "        if nvf_display { nvf_direct += nvf_point_display(light_id, in.world_position.xyz, in.N) * shadow; }\n";

/// The same kept without the def, and its twin taking the lamps' factor where the light's
/// shadow is not looked up for this fragment (the light casts none, or the surface receives
/// none: exactly the condition under which bevy_pbr leaves `shadow` at 1 above). Lights with
/// a shadow (the Pip-Boy light, flashlight.rs) are left as they are: their shadow already
/// keeps their light out of creases.
pub const POINT_LINE_HOOKED: &str = "        @if(!NVF_AMBIENT_OCCLUSION)
        if nvf_display { nvf_direct += nvf_point_display(light_id, in.world_position.xyz, in.N) * shadow; }
        // NVForever ambient occlusion (nvforever ssao_lighting.rs): lamps without a shadow here.
        @if(NVF_AMBIENT_OCCLUSION)
        if nvf_display { nvf_direct += nvf_point_display(light_id, in.world_position.xyz, in.N) * shadow * select(nvf_ao_lamps, vec3(1.0), (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::clustered_lights.data[light_id].flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u); }
";

/// A spot light's part of the display-value sum.
pub const SPOT_LINE: &str = "        if nvf_display { nvf_direct += nvf_spot_display(light_id, in.world_position.xyz, in.N) * shadow; }\n";

/// As [`POINT_LINE_HOOKED`], for spot lights.
pub const SPOT_LINE_HOOKED: &str = "        @if(!NVF_AMBIENT_OCCLUSION)
        if nvf_display { nvf_direct += nvf_spot_display(light_id, in.world_position.xyz, in.N) * shadow; }
        // NVForever ambient occlusion (nvforever ssao_lighting.rs): lamps without a shadow here.
        @if(NVF_AMBIENT_OCCLUSION)
        if nvf_display { nvf_direct += nvf_spot_display(light_id, in.world_position.xyz, in.N) * shadow * select(nvf_ao_lamps, vec3(1.0), (in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::clustered_lights.data[light_id].flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u); }
";

/// The end of `main_pass_post_lighting_processing`: after the fog, the tonemapping in the
/// shader and the dither, before the alpha is premultiplied.
pub const DEBUG_ANCHOR: &str = "    @if(PREMULTIPLY_ALPHA)\n    output_color = premultiply_alpha(pbr_input.material.flags, output_color);\n";

/// The function [`DEBUG_ANCHOR`] must lie in.
pub const DEBUG_FUNCTION: &str = "\nfn main_pass_post_lighting_processing(";

/// The debug view (`NVF_PLAY_DEBUG_AO=view-lighting`): the factor this fragment's ambient
/// took, drawn in place of the lit colour at the very end of the material's colour (after
/// whatever the material did to the lit colour, the fog and the dither), so what is seen is
/// exactly what `nvf_ambient_visibility` gave the fragment: the same position, the same
/// distance (`view_z` as `apply_pbr_lighting` forms it).
pub const DEBUG_TEXT: &str = "    // NVForever ambient occlusion (nvforever ssao_lighting.rs): the debug view draws the
    // factor this fragment's ambient light took, after everything else (first-person shapes
    // take none and keep their colour).
    @if(NVF_AMBIENT_OCCLUSION && !FIRST_PERSON_DEPTH)
    if nvf_ao_debug() {
        let nvf_view_z = dot(vec4<f32>(
            view_bindings::view.view_from_world[0].z,
            view_bindings::view.view_from_world[1].z,
            view_bindings::view.view_from_world[2].z,
            view_bindings::view.view_from_world[3].z
        ), pbr_input.world_position);
        output_color = vec4<f32>(vec3<f32>(nvf_ambient_visibility(pbr_input.frag_coord.xy, -nvf_view_z)), output_color.a);
    }
";

/// The hook in bevy_pbr's `pbr_functions` [documented:
/// engine/vendor/bevy_pbr/src/render/pbr_functions.wesl (bevy_pbr 0.20.1) with lights.rs
/// `DISPLAY_EDITS` (and, in the game, sun_visibility.rs `HOOK_EDITS`) made; the tests apply
/// them to that file], in order, each at the first occurrence of its anchor after the last
/// edit: (anchor, text, where the text goes). Deliberately left alone: Bevy's
/// `ambient::ambient_light` call and `let diffuse_occlusion = in.diffuse_occlusion;`, since
/// the NIF and terrain shaders subtract exactly that ambient highlight using the
/// un-occluded `diffuse_occlusion` (nvf_assets nif_material.wesl, terrain.wesl), and an
/// occluded highlight would be over-subtracted there; the directional lights (the sun, the
/// moon, an interior's directional light) and every light's specular part (Bevy's
/// `direct_light`).
pub const HOOK_EDITS: [(&str, &str, Place); 6] = [
    (
        "import bevy_render::maths::{E, powsafe};\n",
        "// NVForever ambient occlusion (nvforever ssao_lighting.rs).
@if(NVF_AMBIENT_OCCLUSION) import nvforever::ssao_lighting::{nvf_ambient_visibility, nvf_ao_debug, nvf_ao_multibounce, nvf_lamp_visibility};
",
        Place::After,
    ),
    (LAMP_ANCHOR, LAMP_TEXT, Place::Before),
    (POINT_LINE, POINT_LINE_HOOKED, Place::Replace),
    (SPOT_LINE, SPOT_LINE_HOOKED, Place::Replace),
    (AMBIENT_LINE, AMBIENT_LINE_HOOKED, Place::Replace),
    (DEBUG_ANCHOR, DEBUG_TEXT, Place::Before),
];

/// The point light accumulation line of `lights/display_shader.rs` `PHYSICAL_EDITS`.
pub const PHYSICAL_POINT_LINE: &str = "        direct_light += light_contrib * shadow; // nvf_physical_point\n";

/// Point lights without a shadow here take `nvf_ao_lamps` in world views under `NVF_PHYSICAL_PBR=1`.
pub const PHYSICAL_POINT_LINE_HOOKED: &str = "        @if(!NVF_AMBIENT_OCCLUSION)
        direct_light += light_contrib * shadow; // nvf_physical_point
        // NVForever ambient occlusion (nvforever ssao_lighting.rs): lamps without a shadow here.
        @if(NVF_AMBIENT_OCCLUSION)
        direct_light += light_contrib * shadow * select(nvf_ao_lamps, vec3(1.0), !nvf_display || ((in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::clustered_lights.data[light_id].flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u));
";

/// The spot light accumulation line of `lights/display_shader.rs` `PHYSICAL_EDITS`.
pub const PHYSICAL_SPOT_LINE: &str = "        direct_light += light_contrib * shadow; // nvf_physical_spot\n";

/// As [`PHYSICAL_POINT_LINE_HOOKED`], for spot lights.
pub const PHYSICAL_SPOT_LINE_HOOKED: &str = "        @if(!NVF_AMBIENT_OCCLUSION)
        direct_light += light_contrib * shadow; // nvf_physical_spot
        // NVForever ambient occlusion (nvforever ssao_lighting.rs): lamps without a shadow here.
        @if(NVF_AMBIENT_OCCLUSION)
        direct_light += light_contrib * shadow * select(nvf_ao_lamps, vec3(1.0), !nvf_display || ((in.flags & MESH_FLAGS_SHADOW_RECEIVER_BIT) != 0u && (view_bindings::clustered_lights.data[light_id].flags & mesh_view_types::POINT_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) != 0u));
";

/// The IBL occlusion lines of `lights/display_shader.rs` `PHYSICAL_EDITS`.
pub const PHYSICAL_IBL_AO_ANCHOR: &str = "    let nvf_ibl_diff_ao = diffuse_occlusion; // nvf_physical_ibl_diff_ao
    let nvf_ibl_spec_ao = specular_occlusion; // nvf_physical_ibl_spec_ao
";

/// Once per fragment, before the light loops under `NVF_PHYSICAL_PBR=1`: local AO,
/// architectural sky-visibility and sun contact shadow from one read of `nvf_ao_texture`.
pub const PHYSICAL_LAMP_TEXT: &str = "    // NVForever ambient occlusion (nvforever ssao_lighting.rs): local AO, architectural
    // sky-visibility and sun contact shadow, read once per fragment (first-person shapes take 1).
    @if(NVF_AMBIENT_OCCLUSION && !FIRST_PERSON_DEPTH)
    let nvf_ao_phys = nvf_physical_occlusion(in.frag_coord.xy, -view_z);
    @if(NVF_AMBIENT_OCCLUSION && FIRST_PERSON_DEPTH)
    let nvf_ao_phys = vec3<f32>(1.0);
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_ao_ambient = nvf_ao_phys.x;
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_sky_vis = nvf_ao_phys.y;
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_sun_contact = nvf_ao_phys.z;
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_ao_lamps = nvf_ao_multibounce(nvf_lamp_visibility(nvf_ao_ambient), diffuse_color);

";

/// Screen-space sun contact shadow applied to the primary outdoor/key directional light.
pub const PHYSICAL_CONTACT_SHADOW_TEXT: &str = "            // NVForever sun contact shadow (nvforever ssao_lighting.rs, NVF_PHYSICAL_PBR).
            @if(NVF_AMBIENT_OCCLUSION)
            if (i == 0u && nvf_display) { shadow *= nvf_sun_contact; }
";

/// Multi-bounce diffuse AO, architectural sky-visibility, and Frostbite specular AO on `ENVIRONMENT_MAP` IBL under `NVF_PHYSICAL_PBR=1`.
pub const PHYSICAL_IBL_AO_HOOKED: &str = "    @if(!NVF_AMBIENT_OCCLUSION)
    let nvf_ibl_diff_ao = diffuse_occlusion; // nvf_physical_ibl_diff_ao
    @if(!NVF_AMBIENT_OCCLUSION)
    let nvf_ibl_spec_ao = specular_occlusion; // nvf_physical_ibl_spec_ao
    // NVForever ambient occlusion (nvforever ssao_lighting.rs): local multi-bounce AO + architectural sky-visibility + Frostbite specular AO on IBL.
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_ibl_sky_factor = mix(0.08, 1.0, pow(max(nvf_sky_vis, 0.0001), 1.35));
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_ibl_diff_ao = diffuse_occlusion * nvf_ao_multibounce(nvf_ao_ambient, diffuse_color) * nvf_ibl_sky_factor;
    @if(NVF_AMBIENT_OCCLUSION)
    let nvf_ibl_spec_ao = specular_occlusion * nvf_specular_ao(NdotV, nvf_ao_ambient * nvf_ibl_sky_factor, roughness);
";

/// The fallback linear ambient line of `lights/display_shader.rs` `PHYSICAL_EDITS`.
pub const PHYSICAL_AMBIENT_LINE: &str = "            let nvf_ambient_lin = nvf_srgb_to_linear_hdr(view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure) * diffuse_occlusion; // nvf_physical_fallback_ambient\n";

/// Linear ambient kept without the def, and scaled by the multi-bounce occlusion fit and architectural sky-visibility with it.
pub const PHYSICAL_AMBIENT_LINE_HOOKED: &str = "            // NVForever ambient occlusion (nvforever ssao_lighting.rs): on the fallback linear
            // ambient alone, never on direct directional lights nor on Bevy's ambient highlight.
            @if(!NVF_AMBIENT_OCCLUSION)
            let nvf_ambient_lin = nvf_srgb_to_linear_hdr(view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure) * diffuse_occlusion; // nvf_physical_fallback_ambient
            @if(NVF_AMBIENT_OCCLUSION)
            let nvf_ambient_lin = nvf_srgb_to_linear_hdr(view_bindings::lights.ambient_color.rgb * view_bindings::view.exposure) * diffuse_occlusion * nvf_ao_multibounce(nvf_ao_ambient, diffuse_color) * mix(0.08, 1.0, pow(max(nvf_sky_vis, 0.0001), 1.35));
";

/// The ambient occlusion hook in `apply_pbr_lighting` for the physical PBR patch
/// (`lights/display_shader.rs` `PHYSICAL_EDITS`).
pub const PHYSICAL_HOOK_EDITS: [(&str, &str, Place); 8] = [
    (
        "import bevy_render::maths::{E, powsafe};\n",
        "// NVForever ambient occlusion (nvforever ssao_lighting.rs).
@if(NVF_AMBIENT_OCCLUSION) import nvforever::ssao_lighting::{nvf_ambient_visibility, nvf_ao_debug, nvf_ao_multibounce, nvf_lamp_visibility, nvf_physical_occlusion};
",
        Place::After,
    ),
    (LAMP_ANCHOR, PHYSICAL_LAMP_TEXT, Place::Before),
    (PHYSICAL_POINT_LINE, PHYSICAL_POINT_LINE_HOOKED, Place::Replace),
    (PHYSICAL_SPOT_LINE, PHYSICAL_SPOT_LINE_HOOKED, Place::Replace),
    ("            shadow *= in.directional_shadow_factor;\n", PHYSICAL_CONTACT_SHADOW_TEXT, Place::After),
    (PHYSICAL_IBL_AO_ANCHOR, PHYSICAL_IBL_AO_HOOKED, Place::Replace),
    (PHYSICAL_AMBIENT_LINE, PHYSICAL_AMBIENT_LINE_HOOKED, Place::Replace),
    (DEBUG_ANCHOR, DEBUG_TEXT, Place::Before),
];

/// The index in [`HOOK_EDITS`] of the debug view's edit.
pub const DEBUG_EDIT: usize = 5;

/// The index in [`PHYSICAL_HOOK_EDITS`] of the debug view's edit.
pub const PHYSICAL_DEBUG_EDIT: usize = 7;

fn apply_edits(source: &str, edits: &[(&str, &str, Place)], debug_edit: usize) -> Option<String> {
    let mut out = String::with_capacity(source.len() + 2048);
    let mut rest = source;
    for (i, (anchor, text, place)) in edits.iter().copied().enumerate() {
        let at = rest.find(anchor)?;
        if i == debug_edit {
            let head = &rest[..at];
            if !head.rfind("\nfn ").is_some_and(|f| head[f..].starts_with(DEBUG_FUNCTION)) {
                return None;
            }
        }
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

/// `source` (lights.rs's display-value patch, and in the game the sun visibility's hook,
/// made) with [`HOOK_EDITS`]; `None` when an anchor is missing or the debug view's anchor
/// is not in `main_pass_post_lighting_processing` (Bevy or the display patch changed), and
/// the caller keeps `source`.
pub fn hook_patched(source: &str) -> Option<String> {
    apply_edits(source, &HOOK_EDITS, DEBUG_EDIT)
}

/// `source` (`lights/display_shader.rs` `physical_patched`, and in the game
/// `sun_visibility::hook_patched`) with [`PHYSICAL_HOOK_EDITS`]; `None` when an anchor is
/// missing or the debug view's anchor is not in `main_pass_post_lighting_processing`.
pub fn physical_hook_patched(source: &str) -> Option<String> {
    apply_edits(source, &PHYSICAL_HOOK_EDITS, PHYSICAL_DEBUG_EDIT)
}
