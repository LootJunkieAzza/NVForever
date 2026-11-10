use bevy_asset::AssetId;
use bevy_shader::{
    Shader, ShaderCache, ShaderCacheError, ShaderCacheSource, ShaderDefVal, ValidateShader,
};
use std::{collections::HashMap, path::Path};
mod clusters;
mod fg;
mod fsr;
mod gpu;
mod motion;

fn load(_: &(), source: ShaderCacheSource, _: &ValidateShader) -> Result<(), ShaderCacheError> {
    let ShaderCacheSource::Wgsl(source) = source else {
        panic!("not WGSL")
    };
    let module = naga::front::wgsl::parse_str(&source)
        .map_err(|e| ShaderCacheError::ProcessShaderError(e.emit_to_string(&source)))?;
    naga::valid::Validator::new(
        naga::valid::ValidationFlags::all(),
        naga::valid::Capabilities::all(),
    )
    .validate(&module)
    .map_err(|e| ShaderCacheError::ProcessShaderError(e.emit_to_string(&source)))?;
    Ok(())
}

fn add_tree(
    root: &Path,
    folder: &Path,
    package: &str,
    cache: &mut ShaderCache<(), ()>,
    ids: &mut HashMap<String, AssetId<Shader>>,
    counter: &mut u128,
) {
    for entry in std::fs::read_dir(folder).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            add_tree(root, &path, package, cache, ids, counter);
        } else if path.extension().is_some_and(|e| e == "wesl") {
            let name = format!(
                "{}/{}",
                package,
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/")
            );
            *counter += 1;
            let id = AssetId::Uuid {
                uuid: uuid::Uuid::from_u128(*counter),
            };
            let shader = Shader::from_wesl(
                std::fs::read_to_string(&path).unwrap(),
                format!("embedded://{name}"),
            );
            cache.set_shader(id, shader);
            ids.insert(name, id);
        }
    }
}

fn main() {
    if std::env::args().any(|a| a == "--frame-generation-only") {
        let (device, queue) = gpu::device();
        fg::run(&device, &queue);
        return;
    }
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    let cargo_home = std::env::var_os("CARGO_HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::PathBuf::from(
                std::env::var_os("USERPROFILE")
                    .or_else(|| std::env::var_os("HOME"))
                    .expect("set CARGO_HOME"),
            )
            .join(".cargo")
        });
    let registry = std::env::var_os("NVF_SHADER_REGISTRY")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::fs::read_dir(cargo_home.join("registry/src"))
                .expect("run cargo fetch --locked --manifest-path engine/Cargo.toml first")
                .filter_map(Result::ok)
                .map(|e| e.path())
                .find(|p| p.join("bevy_pbr-0.20.1").is_dir())
                .expect("Bevy 0.20.1 shader sources missing; run engine cargo fetch")
        });
    let source = repository.join("engine");
    let mut cache = ShaderCache::new((), load);
    let mut ids = HashMap::new();
    let mut counter = 0;
    for package in [
        "bevy_pbr",
        "bevy_render",
        "bevy_core_pipeline",
        "bevy_mesh",
        "bevy_light",
        "bevy_camera",
        "bevy_image",
        "bevy_color",
    ] {
        let patched = source.join("vendor").join(package).join("src");
        let root = if patched.is_dir() {
            patched
        } else {
            registry.join(format!("{package}-0.20.1/src"))
        };
        add_tree(&root, &root, package, &mut cache, &mut ids, &mut counter);
    }
    for (package, directory) in [("nvf_assets", "nvf-assets"), ("nvforever", "nvforever")] {
        let root = source.join(directory).join("src");
        add_tree(&root, &root, package, &mut cache, &mut ids, &mut counter);
    }
    let common = [
        "PREPASS_PIPELINE",
        "VERTEX_POSITIONS",
        "VERTEX_UVS",
        "VERTEX_UVS_A",
        "VERTEX_UVS_B",
        "VERTEX_COLORS",
        "VERTEX_OUTPUT_INSTANCE_INDEX",
    ];
    let variants: &[(&str, &[&str])] = &[
        ("nvf_assets/nif_prepass.wesl", &[]),
        (
            "nvf_assets/nif_prepass.wesl",
            &["MAY_DISCARD", "VISIBILITY_RANGE_DITHER"],
        ),
        (
            "nvf_assets/nif_prepass.wesl",
            &[
                "MOTION_VECTOR_PREPASS",
                "PREPASS_FRAGMENT",
                "MAY_DISCARD",
                "VISIBILITY_RANGE_DITHER",
            ],
        ),
        (
            "nvf_assets/nif_prepass.wesl",
            &[
                "MOTION_VECTOR_PREPASS",
                "PREPASS_FRAGMENT",
                "FIRST_PERSON_DEPTH",
                "MAY_DISCARD",
                "VISIBILITY_RANGE_DITHER",
            ],
        ),
        (
            "nvf_assets/nif_prepass.wesl",
            &["FIRST_PERSON_DEPTH", "MAY_DISCARD"],
        ),
        (
            "nvforever/grass.wesl",
            &[
                "MOTION_VECTOR_PREPASS",
                "PREPASS_FRAGMENT",
                "VISIBILITY_RANGE_DITHER",
            ],
        ),
        ("nvforever/grass.wesl", &["VISIBILITY_RANGE_DITHER"]),
        (
            "nvforever/tree.wesl",
            &[
                "MOTION_VECTOR_PREPASS",
                "PREPASS_FRAGMENT",
                "VISIBILITY_RANGE_DITHER",
                "TREE_BARK",
            ],
        ),
        (
            "nvforever/tree.wesl",
            &[
                "MOTION_VECTOR_PREPASS",
                "PREPASS_FRAGMENT",
                "VISIBILITY_RANGE_DITHER",
                "TREE_LEAVES",
            ],
        ),
        (
            "nvforever/tree.wesl",
            &[
                "MOTION_VECTOR_PREPASS",
                "PREPASS_FRAGMENT",
                "VISIBILITY_RANGE_DITHER",
                "TREE_BILLBOARD",
            ],
        ),
        (
            "nvforever/tree.wesl",
            &["TREE_LEAVES", "VISIBILITY_RANGE_DITHER"],
        ),
        (
            "nvforever/tree.wesl",
            &["TREE_BILLBOARD", "VISIBILITY_RANGE_DITHER"],
        ),
        (
            "nvf_assets/nif_material.wesl",
            &[
                "DEPTH_PREPASS",
                "VERTEX_NORMALS",
                "VIEW_PROJECTION_PERSPECTIVE",
                "READS_VIEW_TRANSMISSION_TEXTURE",
            ],
        ),
        (
            "nvf_assets/nif_material.wesl",
            &[
                "DEPTH_PREPASS",
                "VERTEX_NORMALS",
                "VIEW_PROJECTION_PERSPECTIVE",
                "READS_VIEW_TRANSMISSION_TEXTURE",
                "REFRACT",
            ],
        ),
        (
            "nvf_assets/nif_material.wesl",
            &[
                "DEPTH_PREPASS",
                "VERTEX_NORMALS",
                "VIEW_PROJECTION_PERSPECTIVE",
                "READS_VIEW_TRANSMISSION_TEXTURE",
                "FIRST_PERSON_DEPTH",
            ],
        ),
        (
            "nvforever/water.wesl",
            &[
                "DEPTH_PREPASS",
                "VERTEX_NORMALS",
                "VIEW_PROJECTION_PERSPECTIVE",
                "READS_VIEW_TRANSMISSION_TEXTURE",
            ],
        ),
    ];
    for (i, (name, extra)) in variants.iter().enumerate() {
        let mut defs: Vec<ShaderDefVal> = common
            .iter()
            .chain(extra.iter())
            .map(|s| (*s).to_string().into())
            .collect();
        defs.push(ShaderDefVal::UInt(
            "MATERIAL_BIND_GROUP".into(),
            if name.contains("nif_material") || name.contains("water.wesl") {
                3
            } else {
                2
            },
        ));
        defs.push(ShaderDefVal::UInt(
            "PER_OBJECT_BUFFER_BATCH_SIZE".into(),
            64,
        ));
        defs.push(ShaderDefVal::UInt(
            "AVAILABLE_STORAGE_BUFFER_BINDINGS".into(),
            10,
        ));
        defs.push(ShaderDefVal::UInt("MAX_CASCADES_PER_LIGHT".into(), 4));
        defs.push(ShaderDefVal::UInt("MAX_DIRECTIONAL_LIGHTS".into(), 10));
        defs.push(ShaderDefVal::UInt("MAX_LIGHT_PROBES".into(), 8));
        defs.push(ShaderDefVal::UInt("MAX_RECT_LIGHTS".into(), 8));
        defs.push(ShaderDefVal::UInt(
            "SCREEN_SPACE_SPECULAR_TRANSMISSION_BLUR_TAPS".into(),
            4,
        ));
        defs.push(ShaderDefVal::UInt(
            "TONEMAPPING_LUT_SAMPLER_BINDING_INDEX".into(),
            19,
        ));
        defs.push(ShaderDefVal::UInt(
            "TONEMAPPING_LUT_TEXTURE_BINDING_INDEX".into(),
            18,
        ));
        if name.contains("nif_material") || name.contains("water.wesl") {
            defs.retain(|v| *v != ShaderDefVal::from("PREPASS_PIPELINE"));
            defs.push("VIEW_TRANSMISSION_TEXTURE".into());
        }
        cache
            .get(i, ids[*name], &defs)
            .unwrap_or_else(|e| panic!("{name} {extra:?}: {e}"));
        println!("PASS {name} {extra:?}");
    }
    for name in ["fsr_reactive.wgsl", "fsr_depth.wgsl", "fsr_fallback.wgsl"] {
        let text = std::fs::read_to_string(source.join("nvforever/src").join(name)).unwrap();
        load(&(), ShaderCacheSource::Wgsl(text), &ValidateShader::Enabled).unwrap();
        println!("PASS {name}");
    }
    gpu::run(&source.join("nvforever/src"));
}
