//! Subsurface scattering (`NVF_SSS=1`): pre-integrated skin shading with dual-lobe sebum
//! specular and shadow-map thickness transmittance (`NVF_SKIN_SSS`), and energy-conserving
//! two-sided foliage BSDF (`NVF_FOLIAGE_SSS`), validated by Naga across lighting modes, IBL,
//! ambient occlusion, and depth-array variants.

mod support;
use support::{Compiled, Compiler};

/// Whether the composed module declares group 0, binding `binding`.
/// Whether the occlusion goes into Physical lighting's light loop: its hook comes with
/// #4287 (part 2 of #3828's split), which this branch is not stacked on; until then the game
/// never sets `NVF_AMBIENT_OCCLUSION` under Physical lighting (nvforever lights.rs).
const PHYSICAL_OCCLUSION: bool = false;

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
fn skin_sss_compiles_across_both_lighting_modes() {
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
                        let ao = ao && (!physical_pbr || PHYSICAL_OCCLUSION);
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
    }
}

/// The screen-space skin scattering's skin (`NVF_SKIN_SPLIT`, physical PBR only; nvforever
/// sss/screen.rs): the diffuse light kept apart per light composes with the image-based
/// light on and off and the ambient occlusion on and off, baked curvature or not. The split
/// needs a depth prepass and the skin's colour buffer (nvf_assets nif_material.rs
/// `skin_split_applies`), which its fragments write at bindings 50-51.
#[test]
fn split_skin_composes_with_its_diffuse_share() {
    let mut compiler = Compiler::with_sss(true);
    let split = ["NVF_SKIN_SSS", "NVF_SKIN_SPLIT", "DEPTH_PREPASS", "NVF_SKIN_ALBEDO"];
    for base in [&split[..], &[&split[..], &["VERTEX_UVS_B", "FACEGEN_TINT"][..]].concat()[..]] {
        for env_map in [false, true] {
            for ao in [false, true] {
                let mut defs = base.to_vec();
                defs.push("NVF_PHYSICAL_PBR");
                if env_map {
                    defs.push("ENVIRONMENT_MAP");
                }
                if ao && PHYSICAL_OCCLUSION {
                    defs.push("NVF_AMBIENT_OCCLUSION");
                }
                let compiled = compiler.compile("nvf_assets/nif_material.wesl", &defs);
                assert!(
                    compiled.module.global_variables.iter().any(|(_, g)| g.name.as_deref().is_some_and(|n| n.contains("nvf_skin_last_diffuse"))),
                    "{defs:?}: the diffuse share is kept"
                );
                assert!(has_fn(&compiled, "nvf_skin_transmittance_profile"), "{defs:?}: transmittance kept");
                assert!(has_fn(&compiled, "nvf_skin_post_scatter"), "{defs:?}: the colour split");
                assert!(has_fn(&compiled, "skin_strand_noise"), "{defs:?}: the hairline's strand noise");
                assert!(binds(&compiled, 50), "{defs:?}: the skin's colour buffer");
                assert!(binds(&compiled, 51), "{defs:?}: the skin's specular buffer");
                assert_eq!(has_fn(&compiled, "nvf_skin_sky"), env_map, "{defs:?}: the sky's light on skin");
            }
        }
    }
    let unsplit = compiler.compile("nvf_assets/nif_material.wesl", &["NVF_SKIN_SSS", "NVF_PHYSICAL_PBR", "DEPTH_PREPASS"]);
    assert!(!binds(&unsplit, 50) && !binds(&unsplit, 51) && !has_fn(&unsplit, "nvf_skin_post_scatter"), "no colour buffer without the split");
}

/// Hair in a view with the skin's colour buffer marks its pixels for the hairline's shade
/// (nvforever sss.wesl `nvf_hair_mark`, binding 50); without the buffer it does not.
#[test]
fn hair_marks_the_hairline_with_the_colour_buffer() {
    let mut compiler = Compiler::with_sss(true);
    let hair = ["NVF_HAIR", "NVF_PHYSICAL_PBR", "VERTEX_UVS", "VERTEX_UVS_A", "STANDARD_MATERIAL_BASE_COLOR_TEXTURE", "DEPTH_PREPASS"];
    for blend in [false, true] {
        let mut defs = hair.to_vec();
        defs.push("NVF_SKIN_ALBEDO");
        if blend {
            defs.push("BLEND_ALPHA");
        }
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", &defs);
        assert!(has_fn(&compiled, "nvf_hair_mark") && binds(&compiled, 50), "{defs:?}: hair marks its pixels");
    }
    let unbound = compiler.compile("nvf_assets/nif_material.wesl", &hair);
    assert!(!has_fn(&unbound, "nvf_hair_mark") && !binds(&unbound, 50), "no mark without the buffer");
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
        assert_eq!(binds(&compiled, 3), entry == "blur_v", "{defs:?}: the skin's colour, read by blur_v");
        assert_eq!(binds(&compiled, 4), entry == "blur_v", "{defs:?}: the skin's specular, read by blur_v");
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
                if ao && PHYSICAL_OCCLUSION {
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

/// Outfit material shading (`NVF_OUTFIT` under `NVF_PHYSICAL_PBR`): Charlie cloth sheen and
/// per-texel material separation across IBL, ambient occlusion, and first-person arms.
#[test]
fn outfit_shading_composes() {
    let mut compiler = Compiler::with_sss(true);
    for env_map in [false, true] {
        for ao in [false, true] {
            for first_person in [false, true] {
                let mut defs = vec![
                    "NVF_OUTFIT",
                    "NVF_PHYSICAL_PBR",
                    "VERTEX_UVS",
                    "VERTEX_UVS_A",
                    "STANDARD_MATERIAL_BASE_COLOR_TEXTURE",
                ];
                if env_map {
                    defs.push("ENVIRONMENT_MAP");
                }
                if ao && PHYSICAL_OCCLUSION {
                    defs.push("NVF_AMBIENT_OCCLUSION");
                }
                if first_person {
                    defs.push("FIRST_PERSON_DEPTH");
                    defs.push("SHADE_LIGHT");
                }
                let compiled = compiler.compile("nvf_assets/nif_material.wesl", &defs);
                assert!(has_fn(&compiled, "nvf_outfit_sheen_eval"), "{defs:?}: Charlie sheen present");
                assert_eq!(has_fn(&compiled, "nvf_outfit_sheen_sky"), env_map, "{defs:?}: sky sheen iff ENVIRONMENT_MAP");
            }
        }
    }
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
                assert!(has_fn(&compiled, "eye_iris_normal"), "{defs:?}: concave iris normal present");
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

