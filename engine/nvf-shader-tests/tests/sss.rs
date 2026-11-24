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
            // Curvature baked per vertex (nvf-assets skin_curvature.rs, UV_1).
            ("nvf_assets/nif_material.wesl", &["NVF_SKIN_SSS", "VERTEX_UVS_B"][..]),
            ("nvf_assets/nif_material.wesl", &["NVF_SKIN_SSS", "VERTEX_UVS_B", "FACEGEN_TINT"][..]),
            (
                "nvf_assets/nif_material.wesl",
                &["NVF_SKIN_SSS", "VERTEX_UVS_B", "FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..],
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

/// The screen-space skin scattering's skin (`NVF_SKIN_SPLIT`, physical PBR only; nvforever
/// sss/screen.rs): the diffuse light kept apart per light composes with the image-based
/// light on and off and the ambient occlusion on and off, baked curvature or not.
#[test]
fn split_skin_composes_with_its_diffuse_share() {
    let mut compiler = Compiler::with_sss(true);
    for base in [&["NVF_SKIN_SSS", "NVF_SKIN_SPLIT"][..], &["NVF_SKIN_SSS", "NVF_SKIN_SPLIT", "VERTEX_UVS_B", "FACEGEN_TINT"][..]] {
        for env_map in [false, true] {
            for ao in [false, true] {
                let mut defs = base.to_vec();
                defs.push("NVF_PHYSICAL_PBR");
                if env_map {
                    defs.push("ENVIRONMENT_MAP");
                }
                if ao {
                    defs.push("NVF_AMBIENT_OCCLUSION");
                }
                let compiled = compiler.compile("nvf_assets/nif_material.wesl", &defs);
                assert!(
                    compiled.module.global_variables.iter().any(|(_, g)| g.name.as_deref().is_some_and(|n| n.contains("nvf_skin_last_diffuse"))),
                    "{defs:?}: the diffuse share is kept"
                );
                assert!(has_fn(&compiled, "nvf_skin_transmittance_profile"), "{defs:?}: transmittance kept");
            }
        }
    }
}

/// The screen-space skin scattering's two passes (nvforever sss_screen.wesl).
#[test]
fn screen_space_skin_blur_compiles_both_directions() {
    let mut compiler = Compiler::with_sss(true);
    // bevy_core_pipeline is not vendored: its fullscreen vertex output, as Bevy declares it.
    compiler.register(
        "bevy_core_pipeline/fullscreen_vertex_shader.wesl",
        "struct FullscreenVertexOutput {\n    @builtin(position) position: vec4<f32>,\n    @location(0) uv: vec2<f32>,\n};\n".into(),
    );
    for (defs, entry) in [(&[][..], "blur_h"), (&["VERTICAL"][..], "blur_v")] {
        let compiled = compiler.compile("nvforever/sss_screen.wesl", defs);
        let entries: Vec<_> = compiled.module.entry_points.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(entries, [entry], "{defs:?}");
        assert!(binds(&compiled, 0) && binds(&compiled, 1) && binds(&compiled, 2), "{defs:?}: depth, uniform, source");
    }
}

/// Hair under SSS with physical PBR (`NVF_HAIR`): the strand tangent from the texture and
/// the strand BSDF for every light kind, blended or not, with the image-based light and the
/// ambient occlusion on and off.
#[test]
fn hair_strand_shading_composes() {
    let mut compiler = Compiler::with_sss(true);
    for blend in [false, true] {
        for env_map in [false, true] {
            for ao in [false, true] {
                let mut defs = vec!["NVF_HAIR", "NVF_PHYSICAL_PBR", "VERTEX_UVS", "VERTEX_UVS_A", "STANDARD_MATERIAL_BASE_COLOR_TEXTURE"];
                if blend {
                    defs.push("BLEND_ALPHA");
                }
                if env_map {
                    defs.push("ENVIRONMENT_MAP");
                }
                if ao {
                    defs.push("NVF_AMBIENT_OCCLUSION");
                }
                let compiled = compiler.compile("nvf_assets/nif_material.wesl", &defs);
                assert!(has_fn(&compiled, "hair_strand"), "{defs:?}: strand tangent present");
                for f in ["nvf_hair_eval", "nvf_sss_point_hair", "nvf_sss_spot_hair", "nvf_sss_directional_hair"] {
                    assert!(has_fn(&compiled, f), "{defs:?}: {f} present");
                }
                assert!(!has_fn(&compiled, "nvf_preintegrated_skin"), "{defs:?}: no skin SSS on hair");
            }
        }
    }
    let plain = compiler.compile("nvf_assets/nif_material.wesl", &["NVF_PHYSICAL_PBR"]);
    assert!(!has_fn(&plain, "hair_strand") && !has_fn(&plain, "nvf_hair_eval"), "no strand shading without NVF_HAIR");
}

/// An eye under SSS (`NVF_EYE`): the iris shift and limbal ring, with the clear coat
/// Bevy compiles for the cornea, tinted or not, in both lighting modes.
#[test]
fn eye_shader_compiles_with_iris_depth_and_clear_coat() {
    for physical_pbr in [false, true] {
        let mut compiler = Compiler::with_sss(physical_pbr);
        for tint in [false, true] {
            for env_map in [false, true] {
                let mut defs = vec!["NVF_EYE", "STANDARD_MATERIAL_CLEARCOAT"];
                if physical_pbr {
                    defs.push("NVF_PHYSICAL_PBR");
                }
                if tint {
                    defs.push("FACEGEN_TINT");
                }
                if env_map {
                    defs.push("ENVIRONMENT_MAP");
                }
                let compiled = compiler.compile("nvf_assets/nif_material.wesl", &defs);
                assert!(has_fn(&compiled, "eye_iris_shift"), "{defs:?}: iris shift present");
                assert!(has_fn(&compiled, "eye_limbus"), "{defs:?}: limbal ring present");
                assert!(has_fn(&compiled, "eye_sky_share"), "{defs:?}: socket sky occlusion present");
                assert!(has_fn(&compiled, "eye_socket_occlusion"), "{defs:?}: lid and socket shading present");
                assert!(!has_fn(&compiled, "nvf_preintegrated_skin"), "{defs:?}: no skin SSS on an eye");
            }
        }
        let plain = compiler.compile("nvf_assets/nif_material.wesl", &[]);
        assert!(!has_fn(&plain, "eye_iris_shift"), "no iris shift without NVF_EYE");
    }
}

#[test]
fn foliage_canopy_and_shadow_prepass_shaders_compile() {
    for physical_pbr in [false, true] {
        let mut compiler = Compiler::with_sss(physical_pbr);

        let mut grass_defs = vec!["NVF_CANOPY", "NVF_FOLIAGE_SSS", "NVF_AMBIENT_OCCLUSION"];
        if physical_pbr {
            grass_defs.push("NVF_PHYSICAL_PBR");
        }
        let grass_main = compiler.compile("nvforever/grass.wesl", &grass_defs);
        assert!(has_fn(&grass_main, "nvf_foliage_direct"));

        let grass_shadow = compiler.compile(
            "nvforever/grass.wesl",
            &["PREPASS_PIPELINE", "VIEW_PROJECTION_ORTHOGRAPHIC"],
        );
        assert!(grass_shadow
            .module
            .entry_points
            .iter()
            .any(|e| e.stage == naga::ShaderStage::Vertex));

        let mut tree_defs = vec!["TREE_LEAVES", "NVF_CANOPY", "NVF_FOLIAGE_SSS", "NVF_CROWN_SHADOW"];
        if physical_pbr {
            tree_defs.push("NVF_PHYSICAL_PBR");
        }
        let tree_main = compiler.compile("nvforever/tree.wesl", &tree_defs);
        assert!(has_fn(&tree_main, "nvf_foliage_direct"));

        for extra in [
            &["PREPASS_PIPELINE", "MAY_DISCARD", "GRASS_SHADOW_PREPASS"][..],
            &["PREPASS_PIPELINE", "MAY_DISCARD", "FOLIAGE_SHADOW_PREPASS"][..],
        ] {
            let compiled = compiler.compile("nvf_assets/fade_prepass.wesl", extra);
            assert!(compiled
                .module
                .entry_points
                .iter()
                .any(|e| e.stage == naga::ShaderStage::Fragment));
        }

        let nif_shadow = compiler.compile(
            "nvf_assets/nif_prepass.wesl",
            &["PREPASS_PIPELINE", "MAY_DISCARD", "FOLIAGE_SHADOW_PREPASS"],
        );
        assert!(nif_shadow
            .module
            .entry_points
            .iter()
            .any(|e| e.stage == naga::ShaderStage::Fragment));
    }
}
