//! Opt-in subsurface scattering prototype (`NVF_SSS=1`):
//! - Pre-integrated skin shading (`NVF_SKIN_SSS`) for actor faces, bodies, hands, and
//!   first-person arms using an analytic 2-lobe sphere-damped Gaussian-ReLU fit to the
//!   Penner & Borshukov (2011) / d'Eon & Irving (2007) 6-Gaussian skin diffusion integral.
//! - Foliage backlit translucency (`NVF_FOLIAGE_SSS`) for grass, SpeedTree leaf cards and
//!   billboards, and placed NIF plants.

use bevy::asset::{load_internal_asset, uuid_handle};
use bevy::prelude::*;
use bevy::shader::Shader;

pub mod hook;
pub(crate) use hook::hook_patched;

/// `sss.wesl`, imported by `pbr_functions` as `nvforever::sss` when `NVF_SSS=1`.
const SHADER: Handle<Shader> = uuid_handle!("c4a7b29e-6d18-4f3a-9e85-1b37d6f2a904");

pub struct SssPlugin;

impl Plugin for SssPlugin {
    fn build(&self, app: &mut App) {
        load_internal_asset!(app, SHADER, "sss.wesl", |s, _| Shader::from_wesl(s, "embedded://nvforever/sss.wesl"));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lights::unix_lines;
    use crate::sun_visibility::tests::{balanced, block_after, code, code_lines, resolve_known};

    /// d'Eon & Irving (2007) / Penner (2011) 6-Gaussian human skin diffusion profile
    /// (`variance` in mm^2, RGB weights).
    const GAUSSIANS: [(f32, [f32; 3]); 6] = [
        (0.0064, [0.233, 0.455, 0.649]),
        (0.0484, [0.100, 0.336, 0.344]),
        (0.1870, [0.118, 0.198, 0.000]),
        (0.5670, [0.113, 0.007, 0.007]),
        (1.9900, [0.358, 0.004, 0.000]),
        (7.4100, [0.078, 0.000, 0.000]),
    ];

    fn exact_penner_ring(ndotl: f32, inv_r_mm: f32) -> [f32; 3] {
        let theta = ndotl.clamp(-1.0, 1.0).acos();
        let r = 1.0 / inv_r_mm.max(1e-4);
        let n = 720;
        let dx = std::f32::consts::TAU / n as f32;
        let mut num = [0.0f32; 3];
        let mut den = [0.0f32; 3];
        for i in 0..n {
            let x = -std::f32::consts::PI + (i as f32 + 0.5) * dx;
            let d = 2.0 * r * (x.abs() * 0.5).sin();
            let d2 = d * d;
            let c = (theta + x).cos().max(0.0);
            for (v, w) in GAUSSIANS {
                let g1d = (-d2 / (2.0 * v)).exp() / (std::f32::consts::TAU * v).sqrt();
                for ch in 0..3 {
                    num[ch] += c * w[ch] * g1d;
                    den[ch] += w[ch] * g1d;
                }
            }
        }
        [0, 1, 2].map(|ch| num[ch] / den[ch].max(1e-12))
    }

    fn cpu_skin_lobe(u: f32, sigma: f32) -> f32 {
        let s = sigma.max(1e-4);
        let z = (u / s).clamp(-20.0, 20.0);
        let phi_cdf = 1.0 / (1.0 + (-1.702 * z).exp());
        let phi_pdf = 0.398_942_3 * (-0.5 * z * z).exp();
        ((u * phi_cdf + s * phi_pdf) * (-0.5 * s * s).exp()).max(0.0)
    }

    fn cpu_preintegrated_skin(ndotl: f32, curvature_mm: f32) -> [f32; 3] {
        let k = curvature_mm.clamp(0.0, 0.45);
        let w1 = [0.451f32, 0.791, 0.649];
        let w2 = [0.549f32, 0.209, 0.351];
        let s1 = [0.20 * k, 0.14 * k, 0.08 * k];
        let s2 = [1.55 * k, 0.50 * k, 0.24 * k];
        [0, 1, 2].map(|ch| (w1[ch] * cpu_skin_lobe(ndotl, s1[ch]) + w2[ch] * cpu_skin_lobe(ndotl, s2[ch])).clamp(0.0, 1.0))
    }

    #[test]
    fn analytic_preintegrated_skin_matches_penner_diffusion_integral() {
        let wesl = unix_lines(include_str!("sss.wesl"));
        for snippet in [
            "fn nvf_preintegrated_skin(",
            "let w1 = vec3(0.451, 0.791, 0.649);",
            "let w2 = vec3(0.549, 0.209, 0.351);",
            "let s1 = vec3(0.20, 0.14, 0.08) * k;",
            "let s2 = vec3(1.55, 0.50, 0.24) * k;",
        ] {
            assert!(wesl.contains(snippet), "sss.wesl contains {snippet}");
        }
        // Flat surface (curvature 0) matches Lambertian N.L.
        for ndotl in [-0.3f32, 0.0, 0.25, 0.7, 1.0] {
            let flat = cpu_preintegrated_skin(ndotl, 0.0);
            let want = ndotl.max(0.0);
            for ch in 0..3 {
                assert!((flat[ch] - want).abs() < 1e-3, "flat ndotl={ndotl}: {flat:?}");
            }
        }
        // Across curved surfaces (0.02..0.35 mm^-1), matches exact 6-Gaussian Penner integral
        // within 0.015 and produces warm R > G > B bleed at the terminator.
        for inv_r in [0.02f32, 0.08, 0.20, 0.35] {
            let term = cpu_preintegrated_skin(0.0, inv_r);
            assert!(term[0] > term[1] && term[1] > term[2], "R > G > B at terminator for k={inv_r}: {term:?}");
            for ndotl in [-0.4f32, -0.2, -0.1, 0.0, 0.15, 0.4, 0.8, 1.0] {
                let exact = exact_penner_ring(ndotl, inv_r);
                let approx = cpu_preintegrated_skin(ndotl, inv_r);
                let err = [0, 1, 2].map(|ch| (exact[ch] - approx[ch]).abs()).into_iter().fold(0.0f32, f32::max);
                assert!(err < 0.015, "k={inv_r} ndotl={ndotl}: err={err} exact={exact:?} approx={approx:?}");
            }
        }
    }

    fn cpu_skin_transmittance(d_mm: f32) -> [f32; 3] {
        let s2 = d_mm * d_mm;
        let mut out = [0.0f32; 3];
        for (v, w) in GAUSSIANS {
            let g = (-s2 / (2.0 * v)).exp();
            for ch in 0..3 {
                out[ch] += w[ch] * g;
            }
        }
        out
    }

    #[test]
    fn skin_transmittance_profile_is_monotone_spectrally_ordered_and_rejects_thick_occluders() {
        let wesl = unix_lines(include_str!("sss.wesl"));
        for sym in [
            "fn nvf_skin_sebum_specular(",
            "fn nvf_dir_skin_thickness_mm(",
            "fn nvf_skin_transmittance_profile(",
            "fn nvf_foliage_direct(",
        ] {
            assert!(wesl.contains(sym), "sss.wesl defines {sym}");
        }
        let t1 = cpu_skin_transmittance(1.5);
        let t3 = cpu_skin_transmittance(3.0);
        let t15 = cpu_skin_transmittance(15.0);
        assert!(t1[0] > t1[1] && t1[1] > t1[2], "red > green > blue at 1.5 mm: {t1:?}");
        for ch in 0..3 {
            assert!(t1[ch] >= t3[ch] && t3[ch] >= t15[ch], "monotone falloff ch={ch}");
        }
        assert!(t1[0] > 0.20, "thin ear rim transmits warm red: {t1:?}");
        assert!(t15[0] < 1e-4, "thick torso/wall shadow rejects glow: {t15:?}");
    }

    #[test]
    fn sss_hook_resolves_for_both_lighting_modes_and_is_identity_without_defs() {
        let pbr_functions = unix_lines(
            &std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../vendor/bevy_pbr/src/render/pbr_functions.wesl"))
                .expect("vendored pbr_functions.wesl"),
        );
        for physical in [false, true] {
            let base = if physical {
                crate::lights::physical_patched(&pbr_functions).expect("physical patch applies")
            } else {
                crate::lights::display_patched(&pbr_functions).expect("display patch applies")
            };
            let sun = crate::sun_visibility::hook_patched(&base).expect("sun hook applies");
            let ao = if physical {
                crate::ssao_lighting::physical_hook_patched(&sun).expect("physical AO hook applies")
            } else {
                crate::ssao_lighting::hook_patched(&sun).expect("AO hook applies")
            };
            let hooked = hook_patched(&ao, physical).expect("SSS hook applies");
            assert!(balanced(&hooked), "physical={physical}: balanced brackets");

            for env_map in [false, true] {
                for ao_on in [false, true] {
                    for sun_on in [false, true] {
                        let resolve = |src: &str, defs: &[(&str, bool)]| {
                            let inner = resolve_known(src, defs);
                            resolve_known(&inner, &[("ENVIRONMENT_MAP", env_map)])
                        };
                        // Identity when neither SSS def is enabled.
                        let off = resolve(
                            &hooked,
                            &[
                                ("NVF_SKIN_SSS", false),
                                ("NVF_FOLIAGE_SSS", false),
                                ("NVF_AMBIENT_OCCLUSION", ao_on),
                                ("NVF_SUN_VISIBILITY", sun_on),
                            ],
                        );
                        let want = resolve(&ao, &[("NVF_AMBIENT_OCCLUSION", ao_on), ("NVF_SUN_VISIBILITY", sun_on)]);
                        assert_eq!(code_lines(&off), code_lines(&want), "physical={physical} env={env_map} ao={ao_on} sun={sun_on}: identity when SSS defs are off");
                        assert!(!code(&off).contains("nvf_sss_"), "no SSS calls when off");

                        // Skin enabled alone.
                        let skin = resolve(
                            &hooked,
                            &[
                                ("NVF_SKIN_SSS", true),
                                ("NVF_FOLIAGE_SSS", false),
                                ("NVF_AMBIENT_OCCLUSION", ao_on),
                                ("NVF_SUN_VISIBILITY", sun_on),
                            ],
                        );
                        assert!(balanced(&skin), "skin physical={physical} env={env_map} ao={ao_on} sun={sun_on}");
                        let lighting = block_after(&skin, "fn nvf_apply_pbr_lighting(");
                        assert!(lighting.contains("nvf_sss_point_skin("));
                        assert!(lighting.contains("nvf_sss_spot_skin("));
                        assert!(lighting.contains("nvf_sss_directional_skin("));
                        let has_shadow_line = lighting
                            .lines()
                            .find(|l| l.contains("let nvf_skin_has_shadow ="))
                            .expect("nvf_skin_has_shadow binding");
                        assert!(
                            !has_shadow_line.contains("NVF_MESH_FLAGS_NO_DIRECTIONAL_SHADOWS_BIT"),
                            "nvf_skin_has_shadow must not branch differently between lit and shadowed passes: {has_shadow_line}"
                        );
                        assert!(!code(&skin).contains("nvf_sss_point_foliage("));
                        if physical {
                            let zero_at = lighting.find("lighting_input.diffuse_color = vec3(0.0);").expect("physical skin zeros direct diffuse");
                            let restore_at = lighting.find("lighting_input.diffuse_color = diffuse_color;").expect("physical skin restores IBL diffuse");
                            let indirect_at = lighting.find("var indirect_light = vec3(0.0f);").expect("indirect_light decl");
                            assert!(zero_at < restore_at && restore_at < indirect_at);
                        }

                        // Foliage enabled alone.
                        let foliage = resolve(
                            &hooked,
                            &[
                                ("NVF_SKIN_SSS", false),
                                ("NVF_FOLIAGE_SSS", true),
                                ("NVF_AMBIENT_OCCLUSION", ao_on),
                                ("NVF_SUN_VISIBILITY", sun_on),
                            ],
                        );
                        assert!(balanced(&foliage), "foliage physical={physical} env={env_map} ao={ao_on} sun={sun_on}");
                        let lighting = block_after(&foliage, "fn nvf_apply_pbr_lighting(");
                        assert!(lighting.contains("nvf_sss_point_foliage("));
                        assert!(lighting.contains("nvf_sss_spot_foliage("));
                        assert!(lighting.contains("nvf_sss_directional_foliage("));
                        assert!(!code(&foliage).contains("nvf_sss_point_skin("));
                        if physical {
                            let front_at = lighting.find("lighting_input.diffuse_color = diffuse_color * 0.78;").expect("physical foliage scales direct diffuse");
                            let restore_at = lighting.find("lighting_input.diffuse_color = diffuse_color;").expect("physical foliage restores IBL diffuse");
                            let indirect_at = lighting.find("var indirect_light = vec3(0.0f);").expect("indirect_light decl");
                            assert!(front_at < restore_at && restore_at < indirect_at);
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn skin_eval_is_linear_in_shadow_and_stable_under_shade_light_amplification() {
        let wesl = unix_lines(include_str!("sss.wesl"));
        for snippet in [
            "return vec3(saturate(shadow));",
            "if delta_z <= 1e-5 {",
            "if d_world < 0.20 {",
            "let eff_mm = select(d_est, max(thickness_mm, d_est), thickness_mm >= 0.0);",
            "let rim_grazing = pow(1.0 - saturate(dot(N, V)), 2.0);",
            "let back_scatter = nvf_skin_transmittance_profile(eff_mm) * (back_wrap * phase_fwd * s * saturate(macro_vis) * 0.45);",
        ] {
            assert!(wesl.contains(snippet), "sss.wesl contains {snippet}");
        }

        #[allow(clippy::too_many_arguments)]
        fn cpu_skin_eval(
            dot_nl: f32,
            dot_nv: f32,
            dot_v_neg_l: f32,
            curvature_mm: f32,
            shadow: f32,
            thickness_mm: f32,
            physical: bool,
        ) -> [f32; 3] {
            let s = shadow.clamp(0.0, 1.0);
            let preint = cpu_preintegrated_skin(dot_nl, curvature_mm);
            let phase_fwd = 0.35 + 0.65 * dot_v_neg_l.clamp(0.0, 1.0).powi(3);
            let d_est = (0.65 / curvature_mm.max(0.04)).clamp(3.5, 18.0);
            let eff_mm = if thickness_mm >= 0.0 { thickness_mm.max(d_est) } else { d_est };
            let rim_grazing = (1.0 - dot_nv.clamp(0.0, 1.0)).powi(2);
            let back_wrap = (-dot_nl * 0.5 + 0.25).clamp(0.0, 1.0) * (0.35 + 0.65 * rim_grazing);
            let trans = cpu_skin_transmittance(eff_mm);
            let diffuse_color = [0.55f32, 0.36, 0.28];
            let spec_oil = 0.08 * dot_nl.max(0.0) * (s * std::f32::consts::PI);
            let base = dot_nl.clamp(0.0, 1.0) * s;
            [0, 1, 2].map(|ch| {
                let back_scatter = trans[ch] * (back_wrap * phase_fwd * s * 0.45);
                let skin_scatter = preint[ch] * s + back_scatter;
                if physical {
                    diffuse_color[ch] * 0.94 * skin_scatter + spec_oil
                } else {
                    diffuse_color[ch] * (skin_scatter - base).max(0.0) + spec_oil * 0.5
                }
            })
        }

        // 1. Strict homogeneity f(shadow) == shadow * f(1.0).
        // 2. First-person `with_shade_light` subtraction+amplification at share = 1 / 1024.0
        //    reconstructs `wanted * f(1.0)` cleanly with zero negative or > 1 blowups.
        let share = 1.0f32 / 1024.0;
        for physical in [false, true] {
            for curvature_mm in [0.15f32, 0.18] {
                for thickness_mm in [-1.0f32, 4.0, 12.0] {
                    for dot_nl in [-0.4f32, -0.1, 0.0, 0.35, 0.85] {
                        for dot_nv in [0.1f32, 0.5, 0.9] {
                            let f1 = cpu_skin_eval(dot_nl, dot_nv, 0.75, curvature_mm, 1.0, thickness_mm, physical);
                            for s in [0.0f32, 0.05, 0.25, 0.5, 0.75, 0.999, 1.0] {
                                let fs = cpu_skin_eval(dot_nl, dot_nv, 0.75, curvature_mm, s, thickness_mm, physical);
                                for ch in 0..3 {
                                    assert!(
                                        (fs[ch] - s * f1[ch]).abs() < 1e-6,
                                        "non-linear skin eval physical={physical} s={s} ch={ch}: fs={} want={}",
                                        fs[ch],
                                        s * f1[ch]
                                    );
                                }
                            }
                            for (v_own, shade, wanted) in [
                                (1.0f32, 0.0, 0.0),
                                (1.0, 0.0, 0.35),
                                (1.0, 0.25, 0.15),
                                (0.6, 0.0, 0.0),
                                (0.4, 0.8, 0.4),
                            ] {
                                let missing = (1.0 - v_own) + share * (1.0 - shade);
                                let k = (1.0 + share - wanted) / missing;
                                let lit = [0, 1, 2].map(|ch| f1[ch] + share * f1[ch]);
                                let f_own = cpu_skin_eval(dot_nl, dot_nv, 0.75, curvature_mm, v_own, thickness_mm, physical);
                                let f_shade = cpu_skin_eval(dot_nl, dot_nv, 0.75, curvature_mm, shade, thickness_mm, physical);
                                for ch in 0..3 {
                                    let shadowed = f_own[ch] + share * f_shade[ch];
                                    let out = lit[ch] - (lit[ch] - shadowed) * k;
                                    let want = wanted * f1[ch];
                                    assert!(
                                        (out - want).abs() < 1e-4,
                                        "with_shade_light amplification error physical={physical} v_own={v_own} shade={shade} wanted={wanted} ch={ch}: out={out} want={want}"
                                    );
                                    let clamped = out.max(0.0);
                                    assert!((0.0..=1.0).contains(&clamped), "out-of-range HDR blowup clamped={clamped}");
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
