// Bloom and sharpening of the enhanced graphics (post.rs; docs/design/graphics-enhanced.md
// §3.4-§3.5), on the tonemapped picture before the image-space grade. Entries: `bright`
// (what is brighter than the threshold, at a quarter of the size), `blur_across` and
// `blur_down` (a wide Gaussian at that size), `composite` (the picture, sharpened, plus
// the blurred light). Engineering choices, not the original game's.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct Post {
    // x: threshold, y: knee (softness), z: intensity (0: no bloom), w: sharpening (0..1).
    params: vec4<f32>,
    // x: the remaster's colour grade (`grade`), 0 off.
    grade: vec4<f32>,
}

@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var source_sampler: sampler;
@group(0) @binding(2) var<uniform> post: Post;
// `composite` only: the blurred light.
@group(0) @binding(3) var glow: texture_2d<f32>;

fn luminance(c: vec3<f32>) -> f32 {
    return dot(c, vec3(0.2126, 0.7152, 0.0722));
}

// The remaster's colour grade on display values [engineering choice, by eye]: a gentle
// S-curve around the middle grey (deeper blacks, the highlights held), the shade a little
// cooler and the light a little warmer, and more colour where there is little.
fn grade(c_in: vec3<f32>) -> vec3<f32> {
    // A little brighter first: the remaster's occlusion takes some light from the shade.
    var c = clamp(c_in * 1.06, vec3(0.0), vec3(1.0));
    // S-curve on the luminance: a smoothstep blended in a little, with the toe lifted so
    // the shade keeps its detail (the mean stays about where it was).
    let l0 = luminance(c);
    let s = l0 * l0 * (3.0 - 2.0 * l0);
    let l1 = mix(l0, s, 0.14) + 0.03 * (1.0 - l0) * (1.0 - l0) * l0 * 4.0;
    c *= l1 / max(l0, 1e-4);
    let l = luminance(c);
    // Split toning: shade a touch cooler, light a touch warmer.
    let shade = 1.0 - smoothstep(0.0, 0.4, l);
    let light = smoothstep(0.5, 1.0, l);
    c *= mix(vec3(1.0), vec3(0.985, 0.995, 1.015), shade);
    c *= mix(vec3(1.0), vec3(1.03, 1.0, 0.96), light * 0.8);
    // Vibrance: lift the saturation of greyish colours more than of saturated ones.
    let l2 = luminance(c);
    let sat = max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b));
    c = mix(vec3(l2), c, 1.0 + 0.25 * (1.0 - saturate(sat * 2.0)));
    return clamp(c, vec3(0.0), vec3(1.0));
}

@fragment
fn bright(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    // Four bilinear taps cover the 4 x 4 pixels of the full picture under this one.
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    var c = vec3(0.0);
    for (var i = 0; i < 4; i++) {
        let o = vec2(f32(i & 1) * 2.0 - 1.0, f32(i >> 1u) * 2.0 - 1.0) * texel;
        c += textureSampleLevel(source, source_sampler, in.uv + o, 0.0).rgb;
    }
    c *= 0.25;
    // A soft knee around the threshold, scaled by how far the brightest channel is over.
    let m = max(c.r, max(c.g, c.b));
    let t = post.params.x;
    let k = max(post.params.y, 1e-4);
    let soft = clamp(m - t + k, 0.0, 2.0 * k);
    let over = max(soft * soft / (4.0 * k), m - t);
    return vec4(c * (over / max(m, 1e-4)), 1.0);
}

// The light glow's bright pass on the HDR picture (post.rs `BrightPass`): as `bright`,
// with the threshold in HDR values, and each pixel held to `HDR_BRIGHT_MAX` first so a few
// pixels far over white (neon at multiplier 25) do not show the blur's taps.
const HDR_BRIGHT_MAX: f32 = 16.0;

@fragment
fn bright_hdr(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let texel = 1.0 / vec2<f32>(textureDimensions(source));
    var c = vec3(0.0);
    for (var i = 0; i < 4; i++) {
        let o = vec2(f32(i & 1) * 2.0 - 1.0, f32(i >> 1u) * 2.0 - 1.0) * texel;
        let s = textureSampleLevel(source, source_sampler, in.uv + o, 0.0).rgb;
        c += s * min(1.0, HDR_BRIGHT_MAX / max(max(s.r, max(s.g, s.b)), 1e-4));
    }
    c *= 0.25;
    let m = max(c.r, max(c.g, c.b));
    let t = post.params.x;
    let k = max(post.params.y, 1e-4);
    let soft = clamp(m - t + k, 0.0, 2.0 * k);
    let over = max(soft * soft / (4.0 * k), m - t);
    return vec4(c * (over / max(m, 1e-4)), 1.0);
}

// Nine bilinear taps, `step` texels apart: a Gaussian about 17 × `step` quarter-size
// texels wide (2 for the bloom; under 1, tighter and without gaps between the taps, for
// the HDR bright pass).
fn blur(uv: vec2<f32>, dir: vec2<f32>) -> vec4<f32> {
    return blur_step(uv, dir, 2.0);
}

// The HDR bright pass's blur: its step in `-post.params.w`, negative so that the shared
// composite does not read it as a sharpening strength (post.rs `BrightPass`).
@fragment
fn blur_across_fine(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur_step(in.uv, vec2(1.0, 0.0), -post.params.w);
}

@fragment
fn blur_down_fine(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur_step(in.uv, vec2(0.0, 1.0), -post.params.w);
}

fn blur_step(uv: vec2<f32>, dir: vec2<f32>, step: f32) -> vec4<f32> {
    let w = array<f32, 5>(0.1964, 0.1748, 0.1232, 0.0688, 0.0304);
    let texel = dir / vec2<f32>(textureDimensions(source));
    var c = textureSampleLevel(source, source_sampler, uv, 0.0).rgb * w[0];
    for (var i = 1; i < 5; i++) {
        let o = texel * f32(i) * step;
        c += (textureSampleLevel(source, source_sampler, uv + o, 0.0).rgb + textureSampleLevel(source, source_sampler, uv - o, 0.0).rgb) * w[i];
    }
    return vec4(c / (w[0] + 2.0 * (w[1] + w[2] + w[3] + w[4])), 1.0);
}

@fragment
fn blur_across(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(1.0, 0.0));
}

@fragment
fn blur_down(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(0.0, 1.0));
}

@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let dims = vec2<i32>(textureDimensions(source));
    let p = vec2<i32>(floor(in.position.xy));
    let src = textureLoad(source, p, 0);
    var c = src.rgb;
    let s = post.params.w;
    if s > 0.0 {
        // Sharpen against the four neighbours, less where the local contrast is already
        // high (contrast-adaptive, after AMD's CAS idea), and at most a little past the
        // neighbours (no bright or dark rims).
        let n = textureLoad(source, clamp(p + vec2(0, -1), vec2(0), dims - 1), 0).rgb;
        let w = textureLoad(source, clamp(p + vec2(-1, 0), vec2(0), dims - 1), 0).rgb;
        let e = textureLoad(source, clamp(p + vec2(1, 0), vec2(0), dims - 1), 0).rgb;
        let so = textureLoad(source, clamp(p + vec2(0, 1), vec2(0), dims - 1), 0).rgb;
        let lo = min(c, min(min(n, w), min(e, so)));
        let hi = max(c, max(max(n, w), max(e, so)));
        let contrast = max(hi.r - lo.r, max(hi.g - lo.g, hi.b - lo.b));
        let amount = s * 0.25 * (1.0 - smoothstep(0.1, 0.6, contrast));
        c = clamp(c + (4.0 * c - n - w - e - so) * amount, lo * 0.85, hi * 1.15);
    }
    if post.params.z > 0.0 {
        c += textureSampleLevel(glow, source_sampler, in.uv, 0.0).rgb * post.params.z;
    }
    if post.grade.x > 0.0 {
        c = mix(c, grade(c), post.grade.x);
    }
    return vec4(c, src.a);
}
