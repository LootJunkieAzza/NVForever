//! First-person shade-light single-pass shadow override (`SHADE_LIGHT`,
//! `nvf_assets/nif_material.wesl` + vendored `bevy_pbr` `pbr_types.wesl` / `pbr_functions.wesl`):
//! composed through Bevy's shader cache with the display-value patch, ambient occlusion hook,
//! and reflection gate, then validated by Naga. No render device is created.
//!
//! Verifies that:
//! - First-person `SHADE_LIGHT` NIF variants compile cleanly across lighting/material combos
//!   and invoke `apply_pbr_lighting` only once per fragment (no two-pass extrapolation across
//!   `GAME_CLIP`).
//! - `PbrInput` carries the `nvf_shade_shadow` / `nvf_shade_sun` / `nvf_shade_aux` fields
//!   only when `SHADE_LIGHT` is defined, leaving world/third-person shaders completely
//!   unchanged.

mod support;
use support::{Compiled, Compiler};

fn count_calls_to(compiled: &Compiled, caller_part: &str, callee_part: &str) -> usize {
    let mut callee_handles = Vec::new();
    for (handle, func) in compiled.module.functions.iter() {
        if func.name.as_deref().is_some_and(|n| n.contains(callee_part)) {
            callee_handles.push(handle);
        }
    }
    fn walk_block(block: &naga::Block, callees: &[naga::Handle<naga::Function>]) -> usize {
        let mut count = 0;
        for stmt in block.iter() {
            match stmt {
                naga::Statement::Call { function, .. } if callees.contains(function) => count += 1,
                naga::Statement::Block(b) => count += walk_block(b, callees),
                naga::Statement::If { accept, reject, .. } => {
                    count += walk_block(accept, callees) + walk_block(reject, callees);
                }
                naga::Statement::Switch { cases, .. } => {
                    for case in cases {
                        count += walk_block(&case.body, callees);
                    }
                }
                naga::Statement::Loop { body, continuing, .. } => {
                    count += walk_block(body, callees) + walk_block(continuing, callees);
                }
                _ => {}
            }
        }
        count
    }
    let mut total = 0;
    for (_, func) in compiled.module.functions.iter() {
        if func.name.as_deref().is_some_and(|n| n.contains(caller_part)) {
            total += walk_block(&func.body, &callee_handles);
        }
    }
    total
}

fn pbr_input_has_shade_fields(compiled: &Compiled) -> bool {
    compiled.module.types.iter().any(|(_, ty)| {
        if !ty.name.as_deref().is_some_and(|n| n.contains("PbrInput")) {
            return false;
        }
        matches!(
            &ty.inner,
            naga::TypeInner::Struct { members, .. }
                if members.iter().any(|m| m.name.as_deref() == Some("nvf_shade_shadow"))
        )
    })
}

#[test]
fn first_person_shade_light_compiles_single_pass_and_leaves_world_shaders_unchanged() {
    let mut compiler = Compiler::with_reflection_gate();

    for extra in [
        &["SHADE_LIGHT"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT", "MAY_DISCARD"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT", "NVF_AMBIENT_OCCLUSION"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT", "ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..],
        &["FIRST_PERSON_DEPTH", "SHADE_LIGHT", "NVF_AMBIENT_OCCLUSION", "ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..],
    ] {
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", extra);
        assert!(
            compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment),
            "{extra:?}: fragment entry point",
        );
        assert!(
            pbr_input_has_shade_fields(&compiled),
            "{extra:?}: SHADE_LIGHT PbrInput must include nvf_shade_shadow",
        );
        assert_eq!(
            count_calls_to(&compiled, "nif_fragment", "apply_pbr_lighting"),
            1,
            "{extra:?}: nif_fragment must light first-person shapes in one apply_pbr_lighting call",
        );
        assert!(
            compiled.module.functions.iter().all(|(_, f)| !f.name.as_deref().is_some_and(|n| n.contains("with_shade_light"))),
            "{extra:?}: legacy two-pass with_shade_light must be gone",
        );
    }

    for (shader, extra) in [
        ("nvf_assets/nif_material.wesl", &[][..]),
        ("nvf_assets/nif_material.wesl", &["FIRST_PERSON_DEPTH"][..]),
        ("bevy_pbr/render/pbr.wesl", &[][..]),
        ("nvforever/terrain.wesl", &[][..]),
        ("nvforever/grass.wesl", &[][..]),
        ("nvforever/tree.wesl", &["TREE_LEAVES"][..]),
    ] {
        let compiled = compiler.compile(shader, extra);
        assert!(
            !pbr_input_has_shade_fields(&compiled),
            "{shader} {extra:?}: non-SHADE_LIGHT shaders must not carry nvf_shade_shadow fields",
        );
    }
}
