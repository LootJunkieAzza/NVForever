//! First-person flashlight attenuation (nvforever lights/display_shader.rs, #3121):
//! `pbr_functions` with the display-value patch, the ambient occlusion hook and the
//! reflection gate, composed through Bevy's shader cache and validated by Naga. No render
//! device is created.
//!
//! First-person shapes (`FIRST_PERSON_DEPTH`: arms, held weapon, Pip-Boy) compile with the
//! diffuse and specular attenuation scales for shadow-casting spot lights (the player's
//! flashlight), while every non-first-person lit shader (world NIFs, third-person body and
//! weapon on `BODY_LAYER`, Bevy's PBR fragment, terrain, grass, trees) compiles without
//! those scales so world and third-person flashlight illumination stay at full strength.

mod support;
use support::{Compiled, Compiler};

/// Whether a named function in the validated Naga module contains an `f32` literal equal
/// to `target` (within `1e-6`).
fn function_has_f32_literal(compiled: &Compiled, name_part: &str, target: f32) -> bool {
    let matches_literal = |e: &naga::Expression| {
        matches!(e, naga::Expression::Literal(naga::Literal::F32(v)) if (*v - target).abs() < 1e-6)
    };
    let mut found_fn = false;
    for (_, func) in compiled.module.functions.iter() {
        if func.name.as_deref().is_some_and(|n| n.contains(name_part)) {
            found_fn = true;
            if func.expressions.iter().any(|(_, expr)| matches_literal(expr)) {
                return true;
            }
        }
    }
    assert!(found_fn, "expected function containing `{name_part}` in composed shader");
    false
}

#[test]
fn first_person_shaders_scale_shadow_spot_lights_and_world_shaders_do_not() {
    let mut compiler = Compiler::with_reflection_gate();
    let diffuse = support::production_display_shader::FIRST_PERSON_FLASHLIGHT_DIFFUSE;
    let specular = support::production_display_shader::FIRST_PERSON_FLASHLIGHT_SPECULAR;

    // First-person NIF variants (arms, held weapon, Pip-Boy) across lighting/material combos.
    for extra in [
        &["FIRST_PERSON_DEPTH"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..],
        &["FIRST_PERSON_DEPTH", "MAY_DISCARD"][..],
        &["FIRST_PERSON_DEPTH", "NVF_AMBIENT_OCCLUSION"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT", "NVF_AMBIENT_OCCLUSION"][..],
        &["FIRST_PERSON_DEPTH", "ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT", "NVF_AMBIENT_OCCLUSION", "ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..],
    ] {
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", extra);
        assert!(
            compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment),
            "{extra:?}: fragment entry point",
        );
        assert!(
            function_has_f32_literal(&compiled, "nvf_spot_display", diffuse),
            "{extra:?}: nvf_spot_display must carry first-person flashlight diffuse scale {diffuse}",
        );
        assert!(
            function_has_f32_literal(&compiled, "nvf_apply_pbr_lighting", specular),
            "{extra:?}: nvf_apply_pbr_lighting must carry first-person flashlight specular scale {specular}",
        );
    }

    // World and third-person (BODY_LAYER) variants: no FIRST_PERSON_DEPTH, full flashlight strength.
    for (shader, extra) in [
        ("nvf_assets/nif_material.wesl", &[][..]),
        ("nvf_assets/nif_material.wesl", &["MAY_DISCARD"][..]),
        ("nvf_assets/nif_material.wesl", &["NVF_AMBIENT_OCCLUSION"][..]),
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..]),
        ("bevy_pbr/render/pbr.wesl", &[][..]),
        ("nvforever/terrain.wesl", &[][..]),
        ("nvforever/grass.wesl", &[][..]),
        ("nvforever/tree.wesl", &["TREE_LEAVES"][..]),
    ] {
        let compiled = compiler.compile(shader, extra);
        assert!(
            compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment),
            "{shader} {extra:?}: fragment entry point",
        );
        assert!(
            !function_has_f32_literal(&compiled, "nvf_spot_display", diffuse),
            "{shader} {extra:?}: world/third-person nvf_spot_display must stay unscaled",
        );
        assert!(
            !function_has_f32_literal(&compiled, "nvf_apply_pbr_lighting", specular),
            "{shader} {extra:?}: world/third-person nvf_apply_pbr_lighting must stay unscaled",
        );
    }
}
