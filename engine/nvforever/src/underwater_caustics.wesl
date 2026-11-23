import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import nvforever::caustics::nvf_caustics_filtered;
import nvf_assets::first_person_depth::FIRST_PERSON_DEPTH_SHARE;

struct Caustics {
    world_from_view: mat4x4<f32>,
    projection: vec4<f32>,
    viewport: vec4<f32>,
    sun: vec4<f32>,
    water: vec4<f32>,
    fog: vec4<f32>,
    wet: vec4<u32>,
}
@if(MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> caustics: Caustics;
// After tonemapping (no BEFORE_UPSCALER): the picture this pass reads and writes again.
// Before a temporal upscaler (BEFORE_UPSCALER): the pass returns the factor alone and its
// pipeline's blend multiplies it into the HDR picture (underwater_caustics.rs
// `caustics_before_upscaler`).
@if(!BEFORE_UPSCALER) @group(0) @binding(2) var screen: texture_2d<f32>;
@if(!BEFORE_UPSCALER) @group(0) @binding(3) var screen_sampler: sampler;

const SUBMERGED_GAIN: f32 = 1.5;

// The factor the pixel's colour is multiplied by: 1 where no caustic light falls.
// `pixel`: the depth texel; `viewport_uv`: the pixel in the camera's view (0..1).
fn caustic_factor(pixel: vec2<i32>, viewport_uv: vec2<f32>) -> f32 {
    let d = textureLoad(depth, pixel, 0);
    // The arms use a different view/projection, not the world's camera. The common
    // upscaler depth expansion preserves this encoding, so skip it in every mode.
    if d <= 0.0 || d > 1.0 - FIRST_PERSON_DEPTH_SHARE { return 1.0; }
    let distance = caustics.projection.z / d;
    // The sky dome can have nonzero depth. Keep it and distant fog unmodified.
    if distance > 4096.0 { return 1.0; }
    if any(viewport_uv < vec2(0.0)) || any(viewport_uv > vec2(1.0)) { return 1.0; }
    let ndc = viewport_uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0);
    let view = vec3(ndc * distance / caustics.projection.xy, -distance);
    let receiver = (caustics.world_from_view * vec4(view, 1.0)).xyz;
    // Match bevy_pbr::pbr_functions::apply_fog: radial world-space distance,
    // rather than view-axis depth. Fade added caustic light, not the existing fog.
    let fog_distance = length(receiver - caustics.world_from_view[3].xyz);
    let fog_share = clamp((fog_distance - caustics.fog.x) / max(caustics.fog.y - caustics.fog.x, 0.0001), 0.0, 1.0);
    let visibility = 1.0 - clamp(caustics.fog.z, 0.0, 1.0) * fog_share;
    if visibility <= 0.0 { return 1.0; }
    if receiver.y >= caustics.water.x { return 1.0; }
    let game_xy = vec2(receiver.x, -receiver.z);
    let cell = vec2<i32>(floor(game_xy / caustics.water.w) - caustics.water.yz);
    if any(cell < vec2(0)) || any(cell >= vec2(5)) { return 1.0; }
    let bit = 1u << u32(cell.y * 5 + cell.x);
    if (caustics.wet.x & bit) == 0u { return 1.0; }
    let fade = 1.0 - smoothstep(2048.0, 4096.0, distance);
    // The pixel's size on the receiver, for the pattern's band limit: its width across the
    // view ray, stretched on a bed seen at a grazing angle (capped at six times). The
    // camera's viewport is the display size also under an upscaler, so the band limit is
    // the displayed pixel's, as the textures' mip bias makes it for the scene.
    let across = 2.0 * distance / (caustics.projection.y * max(caustics.viewport.w, 1.0));
    let steepness = abs(receiver.y - caustics.world_from_view[3].y) / max(fog_distance, 0.001);
    let footprint = across / max(steepness, 1.0 / 6.0);
    let gain = nvf_caustics_filtered(receiver, caustics.water.x, caustics.sun.xyz, caustics.sun.w, caustics.projection.w, footprint);
    // The murk between the camera and the receiver already dims the picture this
    // multiplies, so the light is given more weight here than from above [by eye].
    return 1.0 + SUBMERGED_GAIN * gain * fade * visibility;
}

@if(!BEFORE_UPSCALER) @fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let color = textureSampleLevel(screen, screen_sampler, in.uv, 0.0);
    // Temporal upscalers expand the depth to display size before this post pass.
    let size = vec2<f32>(textureDimensions(depth));
    let pixel = clamp(vec2<i32>(in.uv * size), vec2(0), vec2<i32>(size) - 1);
    let viewport_uv = (in.position.xy - caustics.viewport.xy) / caustics.viewport.zw;
    return vec4(color.rgb * caustic_factor(pixel, viewport_uv), color.a);
}

// Drawn with the viewport set to the upscaler's render size, from the top left: the main
// pass drew the depth there, before the upscaler expands it, so the fragment's position is
// its texel and the full-screen uv spans the camera's view.
@if(BEFORE_UPSCALER) @fragment
fn multiply(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let m = caustic_factor(vec2<i32>(floor(in.position.xy)), in.uv);
    return vec4(m, m, m, 1.0);
}
