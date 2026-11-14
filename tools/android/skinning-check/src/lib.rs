//! CPU-only WESL/Naga and original math controls; no Bevy runtime, wgpu device or game.
#[cfg(test)]
#[path = "../fixtures/probe_data.rs"]
mod data;

#[cfg(test)]
mod tests {
    use super::data;
    use naga::{AddressSpace, ArraySize, ShaderStage, StorageAccess, TypeInner};
    const BASELINE: &str =
        include_str!("../fixtures/skinning_before.wesl");
    const PROPOSAL: &str =
        include_str!("../../../../engine/vendor/bevy_pbr/src/render/skinning.wesl");
    const EXPECTED: [[[f32; 3]; 5]; 2] = [
        [
            [-1., 2., 3.],
            [3., 2., 3.],
            [-7., 5., 5.],
            [-7., 3., 5.],
            [-3.5, 4.25, 4.5],
        ],
        [
            [2., 1., 4.5],
            [2., -3., 4.5],
            [3., 7., -3.5],
            [5., 7., -3.5],
            [4.25, 3.5, -1.5],
        ],
    ];
    fn compile(
        source: &str,
        uniform: bool,
    ) -> Result<(naga::Module, naga::valid::ModuleInfo), String> {
        let mut resolver = wesl::resolver::VirtualResolver::new();
        for (path, text) in [
            (
                "bevy_pbr::render::mesh_types",
                "struct SkinnedMesh { data: array<mat4x4<f32>,256>, }",
            ),
            (
                "bevy_pbr::render::mesh_bindings",
                "struct ProbeMesh { current_skin_index:u32, }\n@group(2) @binding(0) var<storage> mesh:array<ProbeMesh>;",
            ),
            ("bevy_pbr::render::skinning", source),
            (
                "bevy_pbr::probe",
                r#"
import bevy_pbr::render::skinning::{skin_model,skin_prev_model};
@vertex fn vertex(@builtin(vertex_index) index:u32)->@builtin(position) vec4<f32> {
    let p=vec4(f32(index),1.0,2.0,1.0);
    let joints=vec4<u32>(0u,1u,0u,0u);
    let weights=vec4(0.25,0.75,0.0,0.0);
    return 0.5*(skin_model(joints,weights,0u)*p+skin_prev_model(joints,weights,0u)*p);
}"#,
            ),
        ] {
            resolver.add_module(
                path.parse().map_err(|e| format!("{e}"))?,
                text.to_owned().into(),
            );
        }
        let mut options = wesl::CompileOptions {
            imports: true,
            condcomp: true,
            visibility: false,
            ..Default::default()
        };
        options.features.flags.insert("SKINNED".into(), true.into());
        options
            .features
            .flags
            .insert("SKINS_USE_UNIFORM_BUFFERS".into(), uniform.into());
        let root = "bevy_pbr::probe".parse().map_err(|e| format!("{e}"))?;
        let wgsl = wesl::compile(&root, &options, &resolver)
            .map_err(|e| format!("{e}"))?
            .to_string();
        let module = naga::front::wgsl::parse_str(&wgsl).map_err(|e| e.emit_to_string(&wgsl))?;
        let info = naga::valid::Validator::new(
            naga::valid::ValidationFlags::all(),
            naga::valid::Capabilities::all(),
        )
        .validate(&module)
        .map_err(|e| format!("{e}"))?;
        Ok((module, info))
    }
    #[derive(Debug, PartialEq, Eq)]
    struct Abi {
        offset: u32,
        stride: u32,
        matrix_size: u32,
        matrix_alignment: u32,
        minimum_binding: u32,
        space: AddressSpace,
        runtime_sized: bool,
    }
    fn abi(module: &naga::Module, binding: u32) -> Abi {
        let (_, global) = module
            .global_variables
            .iter()
            .find(|(_, variable)| {
                variable
                    .binding
                    .as_ref()
                    .is_some_and(|b| b.group == 2 && b.binding == binding)
            })
            .expect("exact current/previous bindings must remain live");
        let mut layout = naga::proc::Layouter::default();
        layout.update(module.to_ctx()).unwrap();
        let mut array = global.ty;
        let mut offset = 0;
        if let TypeInner::Struct { members, .. } = &module.types[array].inner {
            let last = members.last().expect("array wrapper member");
            offset = last.offset;
            array = last.ty;
        }
        let TypeInner::Array { base, stride, size } = &module.types[array].inner else {
            panic!("matrix array expected")
        };
        Abi {
            offset,
            stride: *stride,
            matrix_size: layout[*base].size,
            matrix_alignment: layout[*base].alignment * 1,
            minimum_binding: layout[global.ty].size,
            space: global.space,
            runtime_sized: *size == ArraySize::Dynamic,
        }
    }
    #[test]
    fn exact_wesl_baseline_and_proposal_validate_and_emit_spirv_for_both_buffer_modes() {
        let mut artifacts = std::collections::HashMap::new();
        for (name, source) in [("baseline", BASELINE), ("proposal", PROPOSAL)] {
            for uniform in [false, true] {
                let (module, info) = compile(source, uniform).unwrap();
                assert_eq!(module.entry_points.len(), 1);
                assert_eq!(module.entry_points[0].stage, ShaderStage::Vertex);
                let words = naga::back::spv::write_vec(
                    &module,
                    &info,
                    &Default::default(),
                    Some(&naga::back::spv::PipelineOptions {
                        shader_stage: ShaderStage::Vertex,
                        entry_point: module.entry_points[0].name.clone(),
                    }),
                )
                .expect("CPU SPIR-V generation must succeed, not driver qualification");
                assert_eq!(words[0], 0x0723_0203);
                assert!(words.len() > 5);
                artifacts.insert((name, uniform), words.clone());
                if let Some(folder) = std::env::var_os("NVF_SKIN_PROBE_ARTIFACT_DIR") {
                    let folder = std::path::PathBuf::from(folder);
                    std::fs::create_dir_all(&folder).unwrap_or_else(|error| {
                        panic!("cannot create owned artifact folder {folder:?}: {error}")
                    });
                    let mode = if uniform { "uniform" } else { "storage" };
                    let bytes: Vec<u8> = words.into_iter().flat_map(u32::to_le_bytes).collect();
                    std::fs::write(folder.join(format!("{name}-{mode}.spv")), bytes).unwrap();
                }
            }
        }
        assert_eq!(artifacts[&("baseline", true)], artifacts[&("proposal", true)]);
        assert_ne!(artifacts[&("baseline", false)], artifacts[&("proposal", false)]);
    }
    #[test]
    fn exact_current_previous_binding_layouts_match_at_offset_zero_and_matrix_stride_64() {
        for uniform in [false, true] {
            let (baseline, _) = compile(BASELINE, uniform).unwrap();
            let (proposal, _) = compile(PROPOSAL, uniform).unwrap();
            for binding in [1, 6] {
                let a = abi(&baseline, binding);
                let b = abi(&proposal, binding);
                assert_eq!(a, b);
                assert_eq!(
                    (b.offset, b.stride, b.matrix_size, b.matrix_alignment),
                    (0, 64, 64, 16)
                );
                assert_eq!(b.runtime_sized, !uniform);
                assert_eq!(b.minimum_binding, if uniform { 256 * 64 } else { 64 });
                assert_eq!(
                    b.space,
                    if uniform {
                        AddressSpace::Uniform
                    } else {
                        AddressSpace::Storage {
                            access: StorageAccess::LOAD,
                        }
                    }
                );
            }
        }
    }
    #[test]
    fn abi_control_detects_a_real_struct_offset_change_and_validator_rejects_bad_member_access() {
        // Git checkouts can use CRLF. The mutation control must change syntax,
        // independently of the production source's line-ending convention.
        let proposal = PROPOSAL.replace("\r\n", "\n");
        let shifted = proposal.replacen(
            "struct StorageJointMatrices {\n    data:",
            "struct StorageJointMatrices {\n    padding: vec4<f32>,\n    data:",
            1,
        );
        assert_ne!(shifted, proposal);
        let (baseline, _) = compile(BASELINE, false).unwrap();
        let (shifted, _) = compile(&shifted, false).unwrap();
        for binding in [1, 6] {
            assert_ne!(abi(&baseline, binding), abi(&shifted, binding));
            assert_eq!(abi(&shifted, binding).offset, 16);
        }
        let invalid = PROPOSAL.replace("joint_matrices.data[", "joint_matrices.no_such_member[");
        assert_ne!(invalid, PROPOSAL);
        assert!(
            compile(&invalid, false).is_err(),
            "WESL/Naga validation must actually inspect the module"
        );
    }
    #[test]
    fn two_joint_two_pose_morph_geometry_matches_handwritten_positions_for_five_instances() {
        assert!(data::INDICES.iter().all(|&i| i < data::POINTS.len() as u32));
        assert_eq!(data::INDICES.len(), 12);
        for weight in data::WEIGHTS {
            assert!(weight.iter().all(|&w| w >= 0.0 && w <= 1.0));
            assert_eq!(weight.into_iter().sum::<f32>(), 1.0);
        }
        for (frame, expected) in EXPECTED.iter().enumerate() {
            let actual = data::cpu_positions(frame);
            for instance in 0..5 {
                for (actual, expected) in actual.iter().zip(expected) {
                    for axis in 0..3 {
                        let offset = if axis == 0 {
                            instance as f32 * 24.0
                        } else {
                            0.0
                        };
                        assert!((actual[axis] + offset - (expected[axis] + offset)).abs() < 0.0001);
                        assert!(actual[axis].is_finite());
                    }
                }
            }
        }
        assert_eq!(data::morph_weight(0), 0.0);
        assert_eq!(data::morph_weight(1), 0.5);
        assert_ne!(data::cpu_positions(0), data::cpu_positions(1));
    }
    #[test]
    fn current_previous_matrix_bytes_roundtrip_all_columns_at_noncontiguous_skin_offsets() {
        let offsets = [0usize, 4, 12, 20, 28];
        let pack = |frame| {
            let mut columns = vec![[0.0f32; 16]; 32];
            for (instance, offset) in offsets.iter().enumerate() {
                for (joint, mut matrix) in data::joint_columns(frame).into_iter().enumerate() {
                    matrix[12] += instance as f32 * 24.0;
                    columns[offset + joint] = matrix;
                }
            }
            columns
                .into_iter()
                .flatten()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        };
        let previous = pack(0);
        let current = pack(1);
        assert_ne!(previous, current);
        assert_eq!((previous.len(), current.len()), (32 * 64, 32 * 64));
        for (frame, bytes) in [(0, &previous), (1, &current)] {
            for (instance, offset) in offsets.iter().enumerate() {
                for (joint, matrix) in data::joint_columns(frame).iter().enumerate() {
                    for col in 0..4 {
                        for row in 0..4 {
                            let at = (offset + joint) * 64 + col * 16 + row * 4;
                            let actual = f32::from_le_bytes(bytes[at..at + 4].try_into().unwrap());
                            let expected = matrix[col * 4 + row]
                                + if col == 3 && row == 0 {
                                    instance as f32 * 24.0
                                } else {
                                    0.0
                                };
                            assert_eq!(actual, expected);
                        }
                    }
                }
            }
        }
    }
}
