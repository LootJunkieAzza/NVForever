// Eye adaptation's metering mask (eye_adapt.rs): for each mask pixel, the share of the
// depth prepass under it that is the world rather than the sky dome, as a weight between
// the sky's small weight and 1.

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Meter {
    // x: distance (game units) beyond which the depth is the sky dome's, y: the
    // projection's near plane, z: the sky's weight, w unused.
    params: vec4<f32>,
}

#ifdef MULTISAMPLED
@group(0) @binding(0) var depth: texture_depth_multisampled_2d;
#else
@group(0) @binding(0) var depth: texture_depth_2d;
#endif
@group(0) @binding(1) var<uniform> meter: Meter;

// Taps per side of each mask pixel.
const TAPS: i32 = 4;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<f32>(textureDimensions(depth));
    let world_reach = meter.params.x;
    let near = meter.params.y;
    let span = fwidth(in.uv);
    var world = 0.0;
    for (var i = 0; i < TAPS * TAPS; i++) {
        let o = (vec2(f32(i % TAPS), f32(i / TAPS)) + 0.5) / f32(TAPS) - 0.5;
        // The mask pixel's footprint in the depth texture (in.uv is its centre).
        let uv = in.uv + o * span;
        let q = clamp(vec2<i32>(uv * dims), vec2(0), vec2<i32>(dims) - 1);
        let d = textureLoad(depth, q, 0);
        // Reverse depth with an infinite far plane: distance = near / depth.
        if d > 0.0 && near / d <= world_reach {
            world += 1.0;
        }
    }
    world /= f32(TAPS * TAPS);
    let sky = meter.params.z;
    return vec4(sky + (1.0 - sky) * world, 0.0, 0.0, 1.0);
}
