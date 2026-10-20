// Screen blur of image space modifiers (imagespace.rs): a Gaussian blur in two passes,
// across and then down, before the grade (docs/spec-notes/image-modifiers.md §5), and
// the depth of field, each pass mixing a point towards a blur of fixed radius by its
// depth's weight (§8).

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

// The grade pass's uniform (imagespace.wgsl); the blur radius, the sharp depth and the
// depth of field are read here.
struct Grade {
    tint: vec4<f32>,
    fade: vec4<f32>,
    radial: vec4<f32>,
    radial2: vec4<f32>,
    saturation: f32,
    contrast: f32,
    contrast_avg_lum: f32,
    brightness: f32,
    blur: f32,
    time: f32,
    sharp: f32,
    vats: vec4<f32>,
    // Depth of field: strength, distance, range, the target's distance (< 0: the screen
    // centre's); use target, flags, the camera's near plane (imagespace.rs).
    dof: vec4<f32>,
    dof_mode: vec4<f32>,
}

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: Grade;
#ifdef MULTISAMPLED
@group(0) @binding(3) var depth: texture_depth_multisampled_2d;
#else
@group(0) @binding(3) var depth: texture_depth_2d;
#endif

// Whether the picture at `uv` is nearer than `grade.sharp` (reverse depth: larger is
// nearer), so neither blurred nor blurred into what is behind it.
fn near(uv: vec2<f32>) -> bool {
    if grade.sharp <= 0.0 {
        return false;
    }
    let dims = vec2<i32>(textureDimensions(depth));
    let p = clamp(vec2<i32>(uv * vec2<f32>(dims)), vec2(0), dims - 1);
    return textureLoad(depth, p, 0) > grade.sharp;
}

// A blur radius of 1 reaches this part of the screen height to each side [hypothesis:
// the wiki gives 0 as none and 7 as the most; image-modifiers.md §5].
const REACH_PER_RADIUS: f32 = 1.0 / 270.0;
const MAX_PAIRS: i32 = 48;

// The blurred picture of the depth of field has this blur radius [hypothesis, §8.5].
const DOF_BLUR_RADIUS: f32 = 3.0;

// A Gaussian blur of `radius` (blur radius units) along `dir`; `centre` is the point's
// own value.
fn gauss(uv: vec2<f32>, dir: vec2<f32>, radius: f32, centre: vec4<f32>) -> vec4<f32> {
    let dims = vec2<f32>(textureDimensions(screen));
    // The reach is two standard deviations of the Gaussian.
    let sigma = 0.5 * radius * dims.y * REACH_PER_RADIUS;
    if sigma < 0.25 {
        return centre;
    }
    let k = -0.5 / (sigma * sigma);
    let pairs = min(i32(ceil(1.25 * sigma)), MAX_PAIRS);
    var acc = centre.rgb;
    var total = 1.0;
    // Pixels 1 and 2, 3 and 4, ... to each side, each pair read as one filtered sample
    // between them.
    for (var p = 0; p < pairs; p++) {
        let a = f32(2 * p + 1);
        let wa = exp(k * a * a);
        let wb = exp(k * (a + 1.0) * (a + 1.0));
        let w = wa + wb;
        let off = dir * ((a * wa + (a + 1.0) * wb) / w) / dims;
        // What is near is left out of the blur behind it.
        let wp = select(w, 0.0, near(uv + off));
        let wm = select(w, 0.0, near(uv - off));
        acc += wp * textureSampleLevel(screen, screen_sampler, uv + off, 0.0).rgb + wm * textureSampleLevel(screen, screen_sampler, uv - off, 0.0).rgb;
        total += wp + wm;
    }
    return vec4(acc / total, centre.a);
}

// Distance from the eye along the view, in game units, at `uv` (infinite for the sky:
// reverse depth, 0 where nothing was drawn).
fn view_distance(uv: vec2<f32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(depth));
    let p = clamp(vec2<i32>(uv * vec2<f32>(dims)), vec2(0), dims - 1);
    let d = textureLoad(depth, p, 0);
    if d <= 0.0 {
        return 1.0e30;
    }
    return grade.dof_mode.z / d;
}

// fnv_world::image_modifier::dof_ramp, the same rule (§8.3).
fn dof_ramp(z: f32) -> f32 {
    let flags = u32(grade.dof_mode.y + 0.5);
    let sky = z >= 1.0e29;
    if sky && (flags & 4u) != 0u {
        return 0.0;
    }
    let d = grade.dof.y;
    let r = grade.dof.z;
    var x = z;
    if grade.dof_mode.x > 0.5 {
        var f = grade.dof.w;
        if f < 0.0 {
            f = view_distance(vec2(0.5, 0.5));
        }
        let mode = flags & 3u;
        let front = mode != 2u;
        let back = mode != 1u;
        if (z < f && !front) || (z >= f && !back) {
            return 0.0;
        }
        x = abs(z - f);
    }
    if r > d {
        return clamp((x - d) / (r - d), 0.0, 1.0);
    }
    if r < d {
        return clamp((d - x) / (d - r), 0.0, 1.0);
    }
    return select(0.0, 1.0, x >= d);
}

fn blur(uv: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    let centre = textureSampleLevel(screen, screen_sampler, uv, 0.0);
    if near(uv) {
        return centre;
    }
    let base = gauss(uv, dir, grade.blur, centre);
    if grade.dof.x == 0.0 {
        return base;
    }
    let w = grade.dof.x * dof_ramp(view_distance(uv));
    if w == 0.0 {
        return base;
    }
    // Not clamped: past 1 the mix goes beyond the blur (§8.5).
    let blurred = gauss(uv, dir, max(DOF_BLUR_RADIUS, grade.blur), centre);
    return vec4(max(mix(base.rgb, blurred.rgb, w), vec3(0.0)), centre.a);
}

@fragment
fn blur_across(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(1.0, 0.0));
}

@fragment
fn blur_down(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(0.0, 1.0));
}
