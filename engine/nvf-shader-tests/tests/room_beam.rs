//! The room beam passes compile through Bevy's shader cache with either depth format.

mod support;
use support::Compiler;

fn compiler() -> Compiler {
    let mut compiler = Compiler::new(&[]);
    // The accumulate pass imports nvf_assets' first-person depth helpers.
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../nvf-assets/src/first_person_depth.wesl");
    compiler.register("nvf_assets/first_person_depth.wesl", std::fs::read_to_string(path).expect("first_person_depth.wesl"));
    compiler
}

#[test]
fn the_room_beam_shaders_compile() {
    let mut compiler = compiler();
    for defs in [&[][..], &["MULTISAMPLED"][..]] {
        let compiled = compiler.compile("nvforever/room_beam.wesl", defs);
        for (stage, entry) in [(naga::ShaderStage::Vertex, "beam_vertex"), (naga::ShaderStage::Fragment, "beam_fragment")] {
            assert!(compiled.module.entry_points.iter().any(|e| e.stage == stage && e.name == entry), "{defs:?}: {entry}");
        }
        let compiled = compiler.compile("nvforever/room_beam_compose.wesl", defs);
        assert!(compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment && e.name == "beam_compose"), "{defs:?}");
    }
}
