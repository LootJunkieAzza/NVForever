//! Subsurface scattering (`NVF_SSS=1`): pre-integrated skin shading with dual-lobe sebum
//! specular and shadow-map thickness transmittance (`NVF_SKIN_SSS`), and energy-conserving
//! two-sided foliage BSDF (`NVF_FOLIAGE_SSS`), validated by Naga across lighting modes, IBL,
//! ambient occlusion, and depth-array variants.

mod support;
use support::{Compiled, Compiler};

/// Whether the composed module declares group 0, binding `binding`.
fn binds(compiled: &Compiled, binding: u32) -> bool {
    compiled
        .module
        .global_variables
        .iter()
        .any(|(_, g)| g.binding.as_ref().is_some_and(|b| b.group == 0 && b.binding == binding))
}

fn has_fn(compiled: &Compiled, needle: &str) -> bool {
    compiled
        .module
        .functions
        .iter()
        .any(|(_, f)| f.name.as_deref().is_some_and(|n| n.contains(needle)))
}

#[test]
fn skin_and_foliage_sss_compile_across_both_lighting_modes() {
    for physical_pbr in [false, true] {
        let mut compiler = Compiler::with_sss(physical_pbr);

        // Skin variants (`NVF_SKIN_SSS`)
        for (shader, base_defs) in [
            ("nvf_assets/nif_material.wesl", &["NVF_SKIN_SSS"][..]),
            ("nvf_assets/nif_material.wesl", &["NVF_SKIN_SSS", "FACEGEN_TINT"][..]),
            (
                "nvf_assets/nif_material.wesl",
                &["NVF_SKIN_SSS", "FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..],
            ),
        ] {
            for env_map in [false, true] {
                for ao in [false, true] {
                    for no_array in [false, true] {
                        let mut defs = base_defs.to_vec();
                        if physical_pbr {
                            defs.push("NVF_PHYSICAL_PBR");
                        }
                        if env_map {
                            defs.push("ENVIRONMENT_MAP");
                        }
                        if ao {
                            defs.push("NVF_AMBIENT_OCCLUSION");
                        }
                        if no_array {
                            defs.push("NO_ARRAY_TEXTURES_SUPPORT");
                        }
                        let compiled = compiler.compile(shader, &defs);
                        assert_eq!(binds(&compiled, 43), ao, "{shader} {defs:?}: binding 43");
                        let first_person = defs.contains(&"FIRST_PERSON_DEPTH");
                        assert_eq!(
                            binds(&compiled, 44),
                            ao && !first_person,
                            "{shader} {defs:?}: binding 44"
                        );
                        assert!(has_fn(&compiled, "nvf_preintegrated_skin"), "{shader} {defs:?}: skin SSS present");
                        assert!(has_fn(&compiled, "nvf_skin_sebum_specular"), "{shader} {defs:?}: dual-lobe sebum specular present");
                        assert!(has_fn(&compiled, "nvf_dir_skin_thickness_mm"), "{shader} {defs:?}: shadow-map thickness present");
                        assert!(has_fn(&compiled, "nvf_skin_transmittance_profile"), "{shader} {defs:?}: transmittance profile present");
                        assert!(!has_fn(&compiled, "nvf_foliage_direct"), "{shader} {defs:?}: foliage SSS absent");
                        assert!(
                            compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment),
                            "{shader} {defs:?}: fragment entry point"
                        );
                    }
                }
            }
        }

        // Foliage variants (`NVF_FOLIAGE_SSS`)
        for (shader, base_defs) in [
            ("nvf_assets/nif_material.wesl", &["NVF_FOLIAGE_SSS", "MAY_DISCARD"][..]),
            ("nvforever/grass.wesl", &["NVF_FOLIAGE_SSS"][..]),
            ("nvforever/tree.wesl", &["TREE_LEAVES", "NVF_FOLIAGE_SSS"][..]),
            ("nvforever/tree.wesl", &["TREE_BILLBOARD", "NVF_FOLIAGE_SSS"][..]),
        ] {
            for env_map in [false, true] {
                for ao in [false, true] {
                    let mut defs = base_defs.to_vec();
                    if physical_pbr && shader == "nvf_assets/nif_material.wesl" {
                        defs.push("NVF_PHYSICAL_PBR");
                    }
                    if env_map {
                        defs.push("ENVIRONMENT_MAP");
                    }
                    if ao {
                        defs.push("NVF_AMBIENT_OCCLUSION");
                    }
                    let compiled = compiler.compile(shader, &defs);
                    assert_eq!(binds(&compiled, 43), ao, "{shader} {defs:?}: binding 43");
                    assert_eq!(binds(&compiled, 44), ao, "{shader} {defs:?}: binding 44");
                    assert!(has_fn(&compiled, "nvf_foliage_direct"), "{shader} {defs:?}: foliage SSS present");
                    assert!(!has_fn(&compiled, "nvf_preintegrated_skin"), "{shader} {defs:?}: skin SSS absent");
                    assert!(
                        compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment),
                        "{shader} {defs:?}: fragment entry point"
                    );
                }
            }
        }
    }
}
