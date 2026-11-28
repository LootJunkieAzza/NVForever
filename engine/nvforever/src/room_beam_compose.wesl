// Compose one bounded sum of indoor shafts into the main HDR view [design, #4698].
struct BeamView {
    clip_from_world: mat4x4<f32>,
    world_from_view: mat4x4<f32>,
    proj: vec4<f32>,
    viewport: vec4<f32>,
    // x: PEAK; y: G; z: PHASECAP; w: NEAR.
    tune: vec4<f32>,
    flags: vec4<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var beam_sum: texture_2d<f32>;
@group(0) @binding(2) var<uniform> beam_view: BeamView;

// 1 - e^-1 (room_beam.rs UNION_NORM).
const BEAM_UNION_NORM: f32 = 0.63212056;

// The union bound (room_beam.rs `union_bound`).
fn beam_union(tau: f32) -> f32 {
    return (1.0 - exp(-max(tau, 0.0))) / BEAM_UNION_NORM;
}

// `added` (display values) over `under` (linear) in display values, back to linear; held below
// the larger of the picture and 2 in display values.
fn beam_add_display(added: vec3<f32>, under: vec3<f32>) -> vec3<f32> {
    let b = max(under, vec3<f32>(0.0));
    let shown = pow(b, vec3<f32>(1.0 / 2.2));
    let sum = min(shown + max(added, vec3<f32>(0.0)), max(shown, vec3<f32>(2.0)));
    return pow(sum, vec3<f32>(2.2));
}

@fragment
fn beam_compose(@builtin(position) position: vec4<f32>) -> @location(0) vec4<f32> {
    let px = vec2<i32>(floor(position.xy));
    let src = textureLoad(picture, px, 0);
    let dims = vec2<i32>(textureDimensions(beam_sum));
    let sum = textureLoad(beam_sum, clamp(px, vec2<i32>(0), dims - vec2<i32>(1)), 0);
    if sum.a <= 1e-6 {
        return src;
    }
    let colour = sum.rgb / sum.a;
    let added = colour * (beam_view.tune.x * beam_union(sum.a));
    if beam_view.flags.x > 0.5 {
        return vec4<f32>(src.rgb + added, src.a);
    }
    return vec4<f32>(beam_add_display(added, src.rgb), src.a);
}
