//! Compose production WESL through Bevy's cache, then validate its position contract
//! with the same Naga version used by the renderer. No render device is created.

use bevy_asset::{AssetId, Assets};
use bevy_shader::{Shader, ShaderCache, ShaderCacheSource, ShaderDefVal};
use std::{collections::HashMap, fs, path::Path, sync::Arc};

#[path = "../../nvforever/src/lights/display_shader.rs"]
mod production_display_shader;

struct Compiled {
    module: naga::Module,
    info: naga::valid::ModuleInfo,
}

struct Compiler {
    cache: ShaderCache<Compiled, ()>,
    shaders: HashMap<String, AssetId<Shader>>,
    assets: Assets<Shader>,
}

impl Compiler {
    /// `remove_invariance` restores the unannotated producer in memory. This runs
    /// through WESL and Naga too; it is not a source-text assertion.
    fn new(remove_invariance: &[&str]) -> Self {
        let mut compiler = Self {
            cache: ShaderCache::new((), |_, source, _| {
                let ShaderCacheSource::Wgsl(wgsl) = source else {
                    panic!("WESL must compose to WGSL");
                };
                let module = naga::front::wgsl::parse_str(&wgsl)
                    .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&wgsl)));
                let info = naga::valid::Validator::new(
                    naga::valid::ValidationFlags::all(),
                    naga::valid::Capabilities::default(),
                )
                .validate(&module)
                .unwrap_or_else(|error| panic!("{}", error.emit_to_string(&wgsl)));
                Ok(Compiled { module, info })
            }),
            shaders: HashMap::new(),
            assets: Assets::default(),
        };
        let engine = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
        for (package, directory) in [
            ("bevy_pbr", "vendor/bevy_pbr/src"),
            ("bevy_render", "vendor/bevy_render/src"),
            ("nvforever", "nvforever/src"),
        ] {
            let root = engine.join(directory);
            compiler.register_directory(package, &root, &root, remove_invariance);
        }
        compiler
    }

    fn register_directory(
        &mut self,
        package: &str,
        root: &Path,
        directory: &Path,
        remove: &[&str],
    ) {
        let mut paths: Vec<_> = fs::read_dir(directory)
            .expect("production shader directory")
            .map(|entry| entry.expect("shader directory entry").path())
            .collect();
        paths.sort();
        for path in paths {
            if path.is_dir() {
                self.register_directory(package, root, &path, remove);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "wesl")
            {
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_str()
                    .unwrap()
                    .replace('\\', "/");
                let name = format!("{package}/{relative}");
                let mut source = fs::read_to_string(&path).expect("production WESL source");
                if name == "bevy_pbr/render/pbr_functions.wesl" {
                    // LightsPlugin prepares this actual module before grass uses it.
                    source = production_display_shader::display_patched(
                        &production_display_shader::unix_lines(&source),
                    )
                    .expect("production display-lighting patch anchors match the selected shader");
                }
                if remove.contains(&name.as_str()) {
                    source = source.replace("@invariant ", "");
                }
                self.register(&name, source);
            }
        }
    }

    fn register(&mut self, name: &str, source: String) {
        let shader = Shader::from_wesl(source, format!("embedded://{name}"));
        let id = self.assets.add(shader.clone()).id();
        self.cache.set_shader(id, shader);
        self.shaders.insert(name.to_owned(), id);
    }

    fn compile(&mut self, name: &str, extra: &[&'static str]) -> Arc<Compiled> {
        // Representative storage-buffer mesh view; constants are the renderer's
        // specialization inputs, not substitute shader declarations.
        let prepass =
            name == "bevy_pbr/prepass/prepass.wesl" || extra.contains(&"PREPASS_PIPELINE");
        // PrepassPipeline::specialize only supplies normal/tangent attributes for
        // normal/deferred prepasses; the depth-only morph vertex has neither field.
        let normal_attributes = !prepass || extra.contains(&"NORMAL_PREPASS_OR_DEFERRED_PREPASS");
        let mut definitions: Vec<ShaderDefVal> = [
            "VERTEX_POSITIONS",
            "VERTEX_UVS",
            "VERTEX_UVS_A",
            "VERTEX_UVS_B",
            "VERTEX_COLORS",
            "VERTEX_OUTPUT_INSTANCE_INDEX",
            "AVAILABLE_STORAGE_BUFFER_BINDINGS__GE_3",
            "AVAILABLE_STORAGE_BUFFER_BINDINGS__GE_6",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        if prepass && !extra.contains(&"PREPASS_PIPELINE") {
            definitions.push("PREPASS_PIPELINE".into());
        }
        if normal_attributes {
            definitions.extend(
                ["VERTEX_NORMALS", "VERTEX_TANGENTS"]
                    .into_iter()
                    .map(ShaderDefVal::from),
            );
        }
        definitions.extend(
            extra
                .iter()
                .copied()
                .filter(|feature| {
                    normal_attributes
                        || !matches!(
                            *feature,
                            "VERTEX_NORMALS_COMPRESSED" | "VERTEX_TANGENTS_COMPRESSED"
                        )
                })
                .map(ShaderDefVal::from),
        );
        definitions.extend([
            ShaderDefVal::UInt("MATERIAL_BIND_GROUP".into(), 3),
            ShaderDefVal::UInt("MAX_CASCADES_PER_LIGHT".into(), 4),
            ShaderDefVal::UInt("MAX_DIRECTIONAL_LIGHTS".into(), 10),
            ShaderDefVal::UInt("MAX_RECT_LIGHTS".into(), 1),
        ]);
        self.cache
            .get(0, self.shaders[name], &definitions)
            .unwrap_or_else(|error| panic!("{name}, {extra:?}: {error}"))
    }
}

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
