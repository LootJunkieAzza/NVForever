//! Compose production WESL through Bevy's cache, then validate its position contract
//! with the same Naga version used by the renderer. No render device is created.

mod support;
use support::{Compiled, Compiler};

fn invariant_position(compiled: &Compiled) -> Result<(), &'static str> {
    let entry = compiled
        .module
        .entry_points
        .iter()
        .find(|entry| entry.stage == naga::ShaderStage::Vertex && entry.name == "vertex")
        .expect("production vertex entry");
    let output = entry.function.result.as_ref().expect("vertex output");
    let naga::TypeInner::Struct { members, .. } = &compiled.module.types[output.ty].inner else {
        panic!("production vertex returns its IO struct");
    };
    let position = members
        .iter()
        .find_map(|member| match member.binding {
            Some(naga::Binding::BuiltIn(naga::BuiltIn::Position { invariant })) => Some(invariant),
            _ => None,
        })
        .expect("vertex output includes builtin position");
    if !position {
        return Err("equal-depth-position-is-not-invariant");
    }
    Ok(())
}

fn assert_spirv_position_invariant(compiled: &Compiled) {
    let words = naga::back::spv::write_vec(
        &compiled.module,
        &compiled.info,
        &naga::back::spv::Options::default(),
        Some(&naga::back::spv::PipelineOptions {
            shader_stage: naga::ShaderStage::Vertex,
            entry_point: "vertex".into(),
        }),
    )
    .expect("production vertex compiles to SPIR-V");
    let mut decorations = Vec::new();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        assert!(
            count > 0 && at + count <= words.len(),
            "valid SPIR-V instruction length"
        );
        if words[at] & 0xffff == 71 {
            // OpDecorate, SPIR-V 1.6
            decorations.push(&words[at + 1..at + count]);
        }
        at += count;
    }
    // BuiltIn = 11, Position = 0, Invariant = 18 (SPIR-V 1.6, Decorations/BuiltIn).
    let positions: Vec<_> = decorations
        .iter()
        .filter(|operands| operands.get(1..3) == Some(&[11, 0][..]))
        .map(|operands| operands[0])
        .collect();
    assert_eq!(positions.len(), 1, "one SPIR-V position output");
    assert!(
        decorations
            .iter()
            .any(|operands| **operands == [positions[0], 18]),
        "SPIR-V position retains Invariant decoration"
    );
}

#[test]
fn production_mesh_and_prepass_positions_are_invariant() {
    let mut compiler = Compiler::new(&[]);
    for extra in [
        &[][..],
        &["SKINNED"][..],
        &["MORPH_TARGETS"][..],
        &["VERTEX_POSITIONS_COMPRESSED", "VERTEX_NORMALS_COMPRESSED"][..],
    ] {
        for shader in ["bevy_pbr/render/mesh.wesl", "bevy_pbr/prepass/prepass.wesl"] {
            let mut variants = vec![extra.to_vec()];
            if shader == "bevy_pbr/prepass/prepass.wesl" {
                let mut normal_prepass = extra.to_vec();
                normal_prepass.extend(["NORMAL_PREPASS", "NORMAL_PREPASS_OR_DEFERRED_PREPASS"]);
                variants.push(normal_prepass);
            }
            for variant in variants {
                let compiled = compiler.compile(shader, &variant);
                invariant_position(&compiled)
                    .expect("main and depth writer must both be invariant");
                assert_spirv_position_invariant(&compiled);
            }
        }
    }
}

#[test]
fn production_grass_and_tree_positions_are_invariant_in_both_passes() {
    let mut compiler = Compiler::new(&[]);
    for (shader, shape) in [
        ("nvforever/grass.wesl", None),
        ("nvforever/tree.wesl", None),
        ("nvforever/tree.wesl", Some("TREE_LEAVES")),
        ("nvforever/tree.wesl", Some("TREE_BILLBOARD")),
    ] {
        for extra in [
            &[][..],
            &["PREPASS_PIPELINE"][..],
            &["PREPASS_PIPELINE", "MOTION_VECTOR_PREPASS"][..],
            &[
                "PREPASS_PIPELINE",
                "NORMAL_PREPASS",
                "NORMAL_PREPASS_OR_DEFERRED_PREPASS",
            ][..],
            &[
                "PREPASS_PIPELINE",
                "NORMAL_PREPASS",
                "NORMAL_PREPASS_OR_DEFERRED_PREPASS",
                "MOTION_VECTOR_PREPASS",
            ][..],
        ] {
            let mut features = extra.to_vec();
            features.extend(shape);
            let compiled = compiler.compile(shader, &features);
            invariant_position(&compiled)
                .expect("moving geometry must be invariant in both passes");
            assert_spirv_position_invariant(&compiled);
        }
    }
}

#[test]
fn unannotated_baseline_is_rejected_after_actual_shader_compilation() {
    let mut compiler = Compiler::new(&[
        "bevy_pbr/render/forward_io.wesl",
        "bevy_pbr/prepass/io.wesl",
    ]);
    for shader in ["bevy_pbr/render/mesh.wesl", "bevy_pbr/prepass/prepass.wesl"] {
        assert_eq!(
            invariant_position(&compiler.compile(shader, &[])),
            Err("equal-depth-position-is-not-invariant"),
            "baseline {shader}"
        );
    }
}

#[test]
fn regression_detects_missing_forward_and_prepass_annotations_independently() {
    for (removed, rejected, retained) in [
        (
            "bevy_pbr/render/forward_io.wesl",
            "bevy_pbr/render/mesh.wesl",
            "bevy_pbr/prepass/prepass.wesl",
        ),
        (
            "bevy_pbr/prepass/io.wesl",
            "bevy_pbr/prepass/prepass.wesl",
            "bevy_pbr/render/mesh.wesl",
        ),
    ] {
        let mut compiler = Compiler::new(&[removed]);
        assert_eq!(
            invariant_position(&compiler.compile(rejected, &[])),
            Err("equal-depth-position-is-not-invariant"),
            "{removed}"
        );
        invariant_position(&compiler.compile(retained, &[]))
            .expect("the independently annotated other producer remains invariant");
    }
}

#[test]
fn shared_production_position_structs_validate_as_fragment_inputs() {
    let mut compiler = Compiler::new(&[]);
    for (name, module) in [
        ("forward", "render::forward_io"),
        ("prepass", "prepass::io"),
    ] {
        let name = format!("invariance_test/{name}.wesl");
        compiler.register(
            &name,
            format!(
                "import bevy_pbr::{module}::VertexOutput;\n\
             @fragment fn fragment(input: VertexOutput) -> @location(0) vec4<f32> {{\n\
             return vec4<f32>(input.position.xyz, 1.0);\n}}"
            ),
        );
        let compiled = compiler.compile(&name, &[]);
        assert!(
            compiled
                .module
                .entry_points
                .iter()
                .any(|entry| entry.stage == naga::ShaderStage::Fragment)
        );
        naga::back::spv::write_vec(
            &compiled.module,
            &compiled.info,
            &naga::back::spv::Options::default(),
            None,
        )
        .expect("shared position input compiles to SPIR-V");
    }
}
