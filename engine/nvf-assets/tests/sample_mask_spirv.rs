//! The SPIR-V naga writes for `@builtin(sample_mask)` (#2413; engine/vendor/README.md, "naga").
//!
//! Vulkan requires the `SampleMask` built-in to be an array of 32-bit integers
//! (VUID-SampleMask-SampleMask-04359). naga 30.0.1 as published declares it as a scalar
//! `u32`, as WGSL writes it (gfx-rs/wgpu#7311), and Intel's shader compiler crashed building
//! the depth prepass that returns it (`nif_material.rs` "Prepass coverage"). The vendored
//! copy declares `array<u32, 1>` and reaches the mask through an access chain to element 0.
//! These tests fail when that is lost: a naga upgrade without the change, or the
//! `[patch.crates-io]` line gone from engine/Cargo.toml.
//!
//! The shaders here are written for the test. The SPIR-V is read word by word; the numbers
//! below are the SPIR-V specification's (the `spirv` crate naga uses has the same values).

const OP_TYPE_INT: u16 = 21;
const OP_TYPE_ARRAY: u16 = 28;
const OP_TYPE_POINTER: u16 = 32;
const OP_CONSTANT: u16 = 43;
const OP_VARIABLE: u16 = 59;
const OP_ACCESS_CHAIN: u16 = 65;
const OP_IN_BOUNDS_ACCESS_CHAIN: u16 = 66;
const OP_DECORATE: u16 = 71;
const DECORATION_BUILT_IN: u32 = 11;
const BUILT_IN_SAMPLE_MASK: u32 = 20;
const STORAGE_CLASS_INPUT: u32 = 1;
const STORAGE_CLASS_OUTPUT: u32 = 3;

/// A fragment shader that only returns a sample mask.
const MASK_OUT: &str = r#"
@fragment
fn mask_out() -> @builtin(sample_mask) u32 {
    return 5u;
}
"#;

/// A fragment shader that reads the mask in a struct and returns one beside a colour.
const MASK_IN_AND_OUT: &str = r#"
struct Inputs {
    @builtin(position) position: vec4<f32>,
    @builtin(sample_mask) mask: u32,
}

struct Outputs {
    @location(0) colour: vec4<f32>,
    @builtin(sample_mask) mask: u32,
}

@fragment
fn mask_in_and_out(inputs: Inputs) -> Outputs {
    var outputs: Outputs;
    outputs.colour = vec4<f32>(fract(inputs.position.xy), 0.0, 1.0);
    outputs.mask = inputs.mask & 3u;
    return outputs;
}
"#;

fn spirv_for(source: &str) -> Vec<u32> {
    let module = naga::front::wgsl::parse_str(source).expect("the WGSL parses");
    let info = naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::default(),
    )
    .validate(&module)
    .expect("the module validates");
    naga::back::spv::write_vec(&module, &info, &naga::back::spv::Options::default(), None)
        .expect("naga writes SPIR-V")
}

/// The instructions after the five-word header: (opcode, operands).
fn instructions(words: &[u32]) -> Vec<(u16, &[u32])> {
    assert_eq!(words.first(), Some(&0x0723_0203), "SPIR-V magic number");
    let mut found = Vec::new();
    let mut at = 5;
    while at < words.len() {
        let count = (words[at] >> 16) as usize;
        assert!(
            count > 0 && at + count <= words.len(),
            "malformed instruction at word {at}"
        );
        found.push(((words[at] & 0xffff) as u16, &words[at + 1..at + count]));
        at += count;
    }
    found
}

/// The operands of the `op` instruction whose operand `id_at` is `id`.
fn defining<'a>(insts: &[(u16, &'a [u32])], op: u16, id_at: usize, id: u32) -> Option<&'a [u32]> {
    insts
        .iter()
        .find(|(o, operands)| *o == op && operands.get(id_at) == Some(&id))
        .map(|(_, operands)| *operands)
}

/// Checks every variable decorated `BuiltIn SampleMask` and returns their storage classes.
fn checked_sample_mask_classes(words: &[u32]) -> Vec<u32> {
    let insts = instructions(words);
    let masks: Vec<u32> = insts
        .iter()
        .filter(|(op, operands)| {
            *op == OP_DECORATE
                && operands.get(1..3) == Some(&[DECORATION_BUILT_IN, BUILT_IN_SAMPLE_MASK][..])
        })
        .map(|(_, operands)| operands[0])
        .collect();
    let mut classes = Vec::new();
    for var in masks {
        let variable = defining(&insts, OP_VARIABLE, 1, var)
            .unwrap_or_else(|| panic!("SampleMask %{var} is an OpVariable"));
        let (pointer_type, class) = (variable[0], variable[2]);
        let pointer = defining(&insts, OP_TYPE_POINTER, 0, pointer_type)
            .unwrap_or_else(|| panic!("SampleMask %{var} has a pointer type"));
        assert_eq!(
            pointer[1], class,
            "SampleMask %{var}: pointer and variable storage classes"
        );
        let pointee = pointer[2];
        let array = defining(&insts, OP_TYPE_ARRAY, 0, pointee).unwrap_or_else(|| {
            panic!(
                "SampleMask %{var} points to %{pointee}, not an OpTypeArray \
                 (a scalar breaks VUID-SampleMask-SampleMask-04359, #2413)"
            )
        });
        let element = defining(&insts, OP_TYPE_INT, 0, array[1])
            .unwrap_or_else(|| panic!("SampleMask %{var}: the array's element is an integer"));
        assert_eq!(element[1], 32, "SampleMask %{var}: 32-bit elements");
        let length = defining(&insts, OP_CONSTANT, 1, array[2])
            .unwrap_or_else(|| panic!("SampleMask %{var}: the array's length is an OpConstant"));
        assert_eq!(length[2], 1, "SampleMask %{var}: an array of one");
        assert!(
            insts.iter().any(|(op, operands)| {
                (*op == OP_ACCESS_CHAIN || *op == OP_IN_BOUNDS_ACCESS_CHAIN)
                    && operands.get(2) == Some(&var)
            }),
            "SampleMask %{var} is reached through an access chain"
        );
        classes.push(class);
    }
    classes.sort_unstable();
    classes
}

#[test]
fn returned_sample_mask_is_an_array_of_one_u32() {
    let words = spirv_for(MASK_OUT);
    assert_eq!(checked_sample_mask_classes(&words), [STORAGE_CLASS_OUTPUT]);
}

#[test]
fn sample_mask_in_structs_is_an_array_both_ways() {
    let words = spirv_for(MASK_IN_AND_OUT);
    assert_eq!(
        checked_sample_mask_classes(&words),
        [STORAGE_CLASS_INPUT, STORAGE_CLASS_OUTPUT]
    );
}
