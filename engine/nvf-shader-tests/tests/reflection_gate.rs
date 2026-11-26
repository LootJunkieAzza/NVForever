//! The reflection gate (nvforever lights/reflection_gate.rs, #2981 reflections slice 1):
//! `pbr_functions` with the display-value patch, the ambient occlusion's hook and the gate,
//! composed through Bevy's shader cache and validated by Naga. No render device is created.
//!
//! In a view with an environment map (`ENVIRONMENT_MAP`) the specular cube (group 1,
//! binding 1, the single-map layout) is read only by the NIF fragment that opts in with
//! `NVF_PBR_REFLECT`; every other lit shader the view draws (Bevy's own fragment: tree bark,
//! impact decals; terrain, grass, tree leaves; the NIF fragment without the opt-in) compiles
//! without reading it, as it did without `ENVIRONMENT_MAP`.

mod support;
use support::{Compiled, Compiler};

/// Whether Naga reports static fragment usage of the global at `group`, `binding`.
fn fragment_reads(compiled: &Compiled, group: u32, binding: u32) -> bool {
    let Some((handle, _)) = compiled
        .module
        .global_variables
        .iter()
        .find(|(_, g)| g.binding.as_ref().is_some_and(|b| b.group == group && b.binding == binding))
    else {
        return false;
    };
    compiled
        .module
        .entry_points
        .iter()
        .enumerate()
        .filter(|(_, e)| e.stage == naga::ShaderStage::Fragment)
        .any(|(i, _)| !compiled.info.get_entry_point(i)[handle].is_empty())
}

const SPECULAR_CUBE: (u32, u32) = (1, 1);
/// The view's light probe uniform (`view_cubemap_index`), group 0 binding 12
/// (bevy_pbr mesh_view_bindings.wesl).
const LIGHT_PROBES: (u32, u32) = (0, 12);

#[test]
fn only_the_opted_in_nif_fragment_reads_the_view_cube() {
    let mut compiler = Compiler::with_reflection_gate();
    for (shader, extra, reads) in [
        // The NIF surface's main pass with the opt-in: both lighting calls, the plain one
        // and the first-person shadow correction's.
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..], true),
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "MAY_DISCARD"][..], true),
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..], true),
        // Without the opt-in (the switch off) the view's cube is not read.
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP"][..], false),
        // The reflection channel view (NVF_PBR_VIEW=reflection) reads it in the opted-in
        // fragment only; with the reflections off it draws the authored view's colours.
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "NVF_PBR_VIEW", "NVF_PBR_VIEW_REFLECTION"][..], true),
        ("nvf_assets/nif_material.wesl", &["ENVIRONMENT_MAP", "NVF_PBR_VIEW", "NVF_PBR_VIEW_REFLECTION"][..], false),
        ("nvf_assets/nif_material.wesl", &["NVF_PBR_REFLECT", "NVF_PBR_VIEW", "NVF_PBR_VIEW_REFLECTION"][..], false),
        // Every other lit shader of a view with a cube.
        ("bevy_pbr/render/pbr.wesl", &["ENVIRONMENT_MAP"][..], false),
        ("nvforever/terrain.wesl", &["ENVIRONMENT_MAP"][..], false),
        ("nvforever/grass.wesl", &["ENVIRONMENT_MAP"][..], false),
        ("nvforever/tree.wesl", &["ENVIRONMENT_MAP", "TREE_LEAVES"][..], false),
    ] {
        let compiled = compiler.compile(shader, extra);
        assert_eq!(fragment_reads(&compiled, SPECULAR_CUBE.0, SPECULAR_CUBE.1), reads, "{shader} {extra:?}: the specular cube");
    }
}

#[test]
fn the_opted_in_nif_fragment_compiles_in_a_view_without_a_cube() {
    // The switch on, a camera without a cube (the water mirror, the scope, the field's
    // second camera): the def alone, metallic held at 0 in the shader.
    let mut compiler = Compiler::with_reflection_gate();
    for extra in [&["NVF_PBR_REFLECT"][..], &["NVF_PBR_REFLECT", "FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..]] {
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", extra);
        assert!(!fragment_reads(&compiled, SPECULAR_CUBE.0, SPECULAR_CUBE.1), "{extra:?}");
        assert!(compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment), "{extra:?}: a fragment entry point");
    }
}

#[test]
fn the_metal_base_lift_composes_in_the_opted_in_fragment() {
    // Metal base colour (nvf-assets nif_material.wesl `nvf_metal_base`, the gain from the
    // uniform's terrain.y): compiled with the opt-in in a view with a cube, in the plain, the
    // alpha-tested and the first-person shade-light variants; without a cube or without the
    // opt-in it is not compiled. The cube is read exactly as before.
    let mut compiler = Compiler::with_reflection_gate();
    for extra in [
        &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..],
        &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "MAY_DISCARD"][..],
        &["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..],
        &["NVF_PBR_REFLECT"][..],
        &["ENVIRONMENT_MAP"][..],
    ] {
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", extra);
        assert!(compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment), "{extra:?}: a fragment entry point");
        // By name (the composer may decorate it with its module's path).
        let lift = compiled.module.functions.iter().any(|(_, f)| f.name.as_deref().is_some_and(|n| n.contains("nvf_metal_base")));
        let wanted = extra.contains(&"ENVIRONMENT_MAP") && extra.contains(&"NVF_PBR_REFLECT");
        assert_eq!(lift, wanted, "{extra:?}: the lift only with the opt-in and a cube");
        assert_eq!(fragment_reads(&compiled, SPECULAR_CUBE.0, SPECULAR_CUBE.1), wanted, "{extra:?}: the cube read as without the lift");
    }
}

#[test]
fn the_opted_in_fragment_asks_whether_the_view_has_its_own_cube() {
    // ENVIRONMENT_MAP is also set for a view that only sees a light-probe volume, with
    // view_cubemap_index -1 (nif_material.wesl `nvf_view_has_cube`): with the opt-in and the
    // def, the fragment reads the view's light probe uniform for the metallic hold, the
    // reflection weight, the lift and the mirror view; without either it does not read it.
    let mut compiler = Compiler::with_reflection_gate();
    for (extra, reads) in [
        (&["ENVIRONMENT_MAP", "NVF_PBR_REFLECT"][..], true),
        (&["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "FIRST_PERSON_DEPTH", "SHADE_LIGHT"][..], true),
        (&["ENVIRONMENT_MAP", "NVF_PBR_REFLECT", "NVF_PBR_VIEW", "NVF_PBR_VIEW_REFLECTION"][..], true),
        (&["NVF_PBR_REFLECT"][..], false),
        (&["NVF_PBR_REFLECT", "NVF_PBR_VIEW", "NVF_PBR_VIEW_REFLECTION"][..], false),
    ] {
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", extra);
        assert!(compiled.module.entry_points.iter().any(|e| e.stage == naga::ShaderStage::Fragment), "{extra:?}: a fragment entry point");
        let helper = compiled.module.functions.iter().any(|(_, f)| f.name.as_deref().is_some_and(|n| n.contains("nvf_view_has_cube")));
        assert_eq!(helper, reads, "{extra:?}: the own-cube test compiled");
        if reads {
            assert!(fragment_reads(&compiled, LIGHT_PROBES.0, LIGHT_PROBES.1), "{extra:?}: the light probe uniform read");
        }
    }
}

#[test]
fn a_refracting_nif_main_pass_with_the_opt_in_composes_with_the_gated_transmitted_light() {
    // NIF refraction returns its screen sample before lighting. Naga counts cube
    // references in the lighting code that follows, so test composition and the
    // screen binding here. The opaque variants above check cube isolation.
    const VIEW_TRANSMISSION: (u32, u32) = (0, 24);
    let mut compiler = Compiler::with_reflection_gate();
    let transmission = [
        "ENVIRONMENT_MAP",
        "VIEW_TRANSMISSION_TEXTURE",
        "STANDARD_MATERIAL_SPECULAR_TRANSMISSION",
        "STANDARD_MATERIAL_DIFFUSE_OR_SPECULAR_TRANSMISSION",
    ];
    let mut refract = transmission.to_vec();
    refract.extend(["NVF_PBR_REFLECT", "REFRACT"]);
    let mut refract_cut = refract.clone();
    refract_cut.push("MAY_DISCARD");
    for extra in [&refract, &refract_cut] {
        let compiled = compiler.compile("nvf_assets/nif_material.wesl", extra);
        assert!(
            fragment_reads(&compiled, VIEW_TRANSMISSION.0, VIEW_TRANSMISSION.1),
            "{extra:?}: the fragment uses the view transmission texture",
        );
    }
    // Bevy's standard transmissive fragment still uses the screen texture with
    // the reflection opt-in absent, and its lighting does not read the cube.
    let compiled = compiler.compile("bevy_pbr/render/pbr.wesl", &transmission);
    assert!(fragment_reads(&compiled, VIEW_TRANSMISSION.0, VIEW_TRANSMISSION.1));
    assert!(!fragment_reads(&compiled, SPECULAR_CUBE.0, SPECULAR_CUBE.1));
}
