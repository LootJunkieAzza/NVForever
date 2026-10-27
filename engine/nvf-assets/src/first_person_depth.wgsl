// The depth of first-person shapes (nif_material.rs "First-person depth"; the Rust side:
// `FIRST_PERSON_DEPTH_SHARE`): the arms and weapon in view are drawn with their depth
// squeezed into the nearest share of the depth range, so the world never hides them while
// they still hide each other in order. Used by nif_material.wgsl and nif_prepass.wgsl to
// write it, and by the passes that read the depth prepass as distances (soft
// intersections, nvforever ssao.wgsl) to undo it.

#define_import_path nvf_assets::first_person_depth

// The share of the depth range (reverse z: 1 at the near plane) the first-person shapes
// are squeezed into: [1 - share, 1]. The same value as nif_material.rs
// `FIRST_PERSON_DEPTH_SHARE`.
const FIRST_PERSON_DEPTH_SHARE: f32 = 0.015625;

// The depth written for a first-person fragment at depth `z`.
fn squeeze(z: f32) -> f32 {
    return 1.0 - FIRST_PERSON_DEPTH_SHARE * (1.0 - z);
}

// The depth a fragment of the depth prepass was at before it was squeezed: values in the
// squeezed share are first-person shapes (a world surface there would be nearer than
// near / (1 - share), inside the near plane's clip); every other value is returned as it is.
fn unsqueeze(d: f32) -> f32 {
    return select(d, 1.0 - (1.0 - d) / FIRST_PERSON_DEPTH_SHARE, d > 1.0 - FIRST_PERSON_DEPTH_SHARE);
}
