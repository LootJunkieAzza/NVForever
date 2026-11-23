import bevy_render::bindless::{bindless_samplers_filtering, bindless_textures_2d};

import package::render::{
    pbr_bindings::{depth_map_texture, depth_map_sampler},
    mesh_bindings::mesh
};

@if(BINDLESS)
import package::render::pbr_bindings::material_indices;

fn sample_depth_map(uv: vec2<f32>, material_bind_group_slot: u32) -> f32 {
    // We use `textureSampleLevel` over `textureSample` because the wgpu DX12
    // backend (Fxc) panics when using "gradient instructions" inside a loop.
    // It results in the whole loop being unrolled by the shader compiler,
    // which it can't do because the upper limit of the loop in steep parallax
    // mapping is a variable set by the user.
    // The "gradient instructions" comes from `textureSample` computing MIP level
    // based on UV derivative. With `textureSampleLevel`, we provide ourselves
    // the MIP level, so no gradient instructions are used, and we can use
    // sample_depth_map in our loop.
    // See https://stackoverflow.com/questions/56581141/direct3d11-gradient-instruction-used-in-a-loop-with-varying-iteration-forcing
    @if(BINDLESS)
    let height = textureSampleLevel(
        bindless_textures_2d[material_indices[material_bind_group_slot].depth_map_texture],
        bindless_samplers_filtering[material_indices[material_bind_group_slot].depth_map_sampler],
        uv,
        0.0
    ).r;
    @else
    let height = textureSampleLevel(
        depth_map_texture,
        depth_map_sampler,
        uv,
        0.0
    ).r;
    // NVForever height maps: the texture stores height (white = high, the authored `_p` of
    // docs/spec-notes/pbr-materials.md), the march reads depth (white = deepest); one shared
    // inversion for both bindings. Upstream returns the red channel as it is.
    // NVForever height range (#2979): the height is held to 0..1 first, so a signed texture
    // (a game slot-3 map in BC4/BC5 signed, read through the linear loader) cannot push the
    // depth outside the march's 0..1 layers.
    return 1.0 - clamp(height, 0.0, 1.0);
}

/// An implementation of parallax mapping, see https://en.wikipedia.org/wiki/Parallax_mapping
/// Code derived from: https://web.archive.org/web/20150419215321/http://sunandblackcat.com/tipFullView.php?l=eng&topicid=28
fn parallaxed_uv(
    depth_scale: f32,
    max_layer_count: f32,
    max_steps: u32,
    /// The original interpolated uv
    original_uv: vec2<f32>,
    /// The vector from the camera to the fragment at the surface in tangent space
    Vt: vec3<f32>,
    material_bind_group_slot: u32,
) -> vec2<f32> {
    if max_layer_count < 1.0 {
        return original_uv;
    }
    var uv = original_uv;

    // Steep Parallax Mapping
    // ======================
    // Split the depth map into `layer_count` layers.
    // When Vt hits the surface of the mesh (excluding depth displacement),
    // if the depth is not below or on surface including depth displacement (textureSample), then
    // look forward (+= delta_uv) on depth texture according to
    // Vt and distance between hit surface and depth map surface,
    // repeat until below the surface.
    //
    // Where `layer_count` is interpolated between `1.0` and
    // `max_layer_count` according to the steepness of Vt.

    let view_steepness = abs(Vt.z);
    // We mix with minimum value 1.0 because otherwise,
    // with 0.0, we get a division by zero in surfaces parallel to viewport,
    // resulting in a singularity.
    let layer_count = mix(max_layer_count, 1.0, view_steepness);
    let layer_depth = 1.0 / layer_count;
    // NVForever grazing clamp (#2979): upstream divides by `view_steepness` unbounded, so a
    // shallow view shifts the samples several texture repeats away and a view along the
    // surface (|Vt.z| = 0) gives non-finite coordinates. The divisor stops at 0.15 (a view
    // about 81 degrees from the normal), so the full-depth shift is at most about 6.6 x the
    // depth scale [design]. Mirrored on the CPU by nvf-assets `authored::Parallax::full_depth_shift`,
    // whose test reads this line.
    var delta_uv = depth_scale * layer_depth * Vt.xy * vec2(1.0, -1.0) / max(view_steepness, 0.15);

    var current_layer_depth = 0.0;
    var texture_depth = sample_depth_map(uv, material_bind_group_slot);

    // texture_depth > current_layer_depth means the depth map depth is deeper
    // than the depth the ray would be at this UV offset so the ray has not
    // intersected the surface
    for (var i: i32 = 0; texture_depth > current_layer_depth && i <= i32(layer_count); i++) {
        current_layer_depth += layer_depth;
        uv += delta_uv;
        texture_depth = sample_depth_map(uv, material_bind_group_slot);
    }

    @if(RELIEF_MAPPING) {
    // Relief Mapping
    // ==============
    // "Refine" the rough result from Steep Parallax Mapping
    // with a **binary search** between the layer selected by steep parallax
    // and the next one to find a point closer to the depth map surface.
    // This reduces the jaggy step artifacts from steep parallax mapping.

    delta_uv *= 0.5;
    var delta_depth = 0.5 * layer_depth;

    uv -= delta_uv;
    current_layer_depth -= delta_depth;

    for (var i: u32 = 0u; i < max_steps; i++) {
        texture_depth = sample_depth_map(uv, material_bind_group_slot);

        // Halve the deltas for the next step
        delta_uv *= 0.5;
        delta_depth *= 0.5;

        // Step based on whether the current depth is above or below the depth map
        if (texture_depth > current_layer_depth) {
            uv += delta_uv;
            current_layer_depth += delta_depth;
        } else {
            uv -= delta_uv;
            current_layer_depth -= delta_depth;
        }
    }
    } @else {
    // Parallax Occlusion mapping
    // ==========================
    // "Refine" Steep Parallax Mapping by interpolating between the
    // previous layer's depth and the computed layer depth.
    // Only requires a single lookup, unlike Relief Mapping, but
    // may skip small details and result in writhing material artifacts.
    let previous_uv = uv - delta_uv;
    let next_depth = texture_depth - current_layer_depth;
    let previous_depth = sample_depth_map(previous_uv, material_bind_group_slot) -
        current_layer_depth + layer_depth;

    // NVForever occlusion guard (#2979): upstream divides by `next_depth - previous_depth`
    // whatever they are. Only when the march brackets a hit (the last layer at or below the
    // surface, the one before above it) is the weight in 0..1; otherwise (the march ran out
    // of layers, or a flat stretch makes the denominator 0) the last layer's coordinates are
    // kept (weight 0) instead of extrapolating or dividing by zero.
    let denominator = next_depth - previous_depth;
    let bracketed = next_depth <= 0.0 && previous_depth >= 0.0 && denominator < -1e-6;
    let weight = select(0.0, next_depth / min(denominator, -1e-6), bracketed);

    uv = mix(uv, previous_uv, weight);

    current_layer_depth += mix(next_depth, previous_depth, weight);
    }

    // Note: `current_layer_depth` is not returned, but may be useful
    // for light computation later on in future improvements of the pbr shader.
    return uv;
}
