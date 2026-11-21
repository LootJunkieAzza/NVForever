// Heat haze at distance (heat_haze.rs; docs/design/graphics-enhanced.md §3.15, a prototype):
// the far scene near the horizon read again from a slightly displaced place, mostly up and
// down, by two layers of a rising value noise; nothing near (the depth prepass), the upper
// sky never. Screen-space refraction with a mask against leaks from the front as Sousa's
// "Generic Refraction Simulation" (GPU Gems 2, 2005, ch. 19) describes it, lattice value
// noise as Ebert et al.'s "Texturing and Modeling" does; written from those descriptions.
// Entry `heat_haze`. An engineering choice, not the original game's.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct Haze {
    // x: strength (0..1), y: the most a pixel moves per picture line at strength 1, z: the
    // projection's near plane, w: 1 to draw the mask (debug view).
    params: vec4<f32>,
    // x, y: the projection's x and y scale; z: distance beyond which the depth is the sky
    // dome's; w: the horizontal part of the displacement.
    proj: vec4<f32>,
    // xyz: the world's up in view space.
    up: vec4<f32>,
    // x: distance where it starts, y: where it is full, z: the sky's share, w: the nearest
    // distance a displaced read may show.
    far: vec4<f32>,
    // Degrees above the horizon where it is still full (x) and gone (y); below it, still
    // full (z) and gone (w), both negative.
    elevation: vec4<f32>,
    // The two noise layers' cells across and down per picture height (xy, zw).
    scales: vec4<f32>,
    // The two noise layers' offsets in cells (xy, zw).
    scroll: vec4<f32>,
}

@group(0) @binding(0) var picture: texture_2d<f32>;
@group(0) @binding(1) var picture_sampler: sampler;
@group(0) @binding(2) var<uniform> haze: Haze;
@if(MULTISAMPLED) @group(0) @binding(3) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(3) var depth: texture_depth_2d;

// The noise repeats every PERIOD cells (heat_haze.rs NOISE_PERIOD wraps the scroll there).
const PERIOD: i32 = 64;
// Distance given to a pixel with nothing drawn (depth 0, the far plane): the sky.
const NOTHING: f32 = 1.0e9;

// A lattice point's value in -1..1: an integer hash of its cell (within one period) and the
// layer's seed (odd multipliers chosen at random, xor-shifts between them).
fn lattice(p: vec2<i32>, seed: u32) -> f32 {
    let q = vec2<u32>(p & vec2<i32>(PERIOD - 1));
    var h: u32 = q.x * 0x7a3d5c19u + q.y * 0x4e8b21f7u + seed * 0xb1c96e53u;
    h = (h ^ (h >> 15u)) * 0x63f0a2d5u;
    h = (h ^ (h >> 13u)) * 0x9d4e17bbu;
    h = h ^ (h >> 16u);
    return f32(h) * (2.0 / 4294967295.0) - 1.0;
}

// Value noise in -1..1: the four lattice values around `x`, smoothly interpolated.
fn value_noise(x: vec2<f32>, seed: u32) -> f32 {
    let cell = floor(x);
    let i = vec2<i32>(cell);
    let f = x - cell;
    let u = f * f * (3.0 - 2.0 * f);
    let a = lattice(i, seed);
    let b = lattice(i + vec2<i32>(1, 0), seed);
    let c = lattice(i + vec2<i32>(0, 1), seed);
    let d = lattice(i + vec2<i32>(1, 1), seed);
    return mix(mix(a, b, u.x), mix(c, d, u.x), u.y);
}

// Distance (game units) along the view ray to what pixel `p` shows: reverse infinite depth,
// view depth = near / depth, times the ray's length per unit of view depth. The first-person
// shapes' squeezed depth reads as within 1.02 times the near plane: near, as they are.
fn distance_at(p: vec2<i32>, ray_len: f32) -> f32 {
    let dims = vec2<i32>(textureDimensions(depth));
    let d = textureLoad(depth, clamp(p, vec2<i32>(0), dims - vec2<i32>(1)), 0);
    if d <= 0.0 {
        return NOTHING;
    }
    return haze.params.z / d * ray_len;
}

// Whether what pixel `p` shows is nearer than a displaced read may show.
fn near_at(p: vec2<i32>, ray_len: f32) -> bool {
    return distance_at(p, ray_len) < haze.far.w;
}

// Whether any of pixel `p`'s four direct neighbours is near: the anti-aliased fringe of a
// near outline (the first-person weapon, a near post) is not moved.
fn near_around(p: vec2<i32>, ray_len: f32) -> bool {
    return near_at(p + vec2<i32>(1, 0), ray_len) || near_at(p - vec2<i32>(1, 0), ray_len)
        || near_at(p + vec2<i32>(0, 1), ray_len) || near_at(p - vec2<i32>(0, 1), ray_len);
}

// Whether any of the four texels a bilinear read at `q` (pixel coordinates, centres at +0.5)
// takes is near, clamped to the picture.
fn footprint_near(q: vec2<f32>, size: vec2<f32>, ray_len: f32) -> bool {
    let last = vec2<i32>(size) - vec2<i32>(1);
    let t = vec2<i32>(floor(q - vec2<f32>(0.5)));
    let a = clamp(t, vec2<i32>(0), last);
    let b = clamp(t + vec2<i32>(1, 1), vec2<i32>(0), last);
    return near_at(a, ray_len) || near_at(vec2<i32>(b.x, a.y), ray_len) || near_at(vec2<i32>(a.x, b.y), ray_len) || near_at(b, ray_len);
}

// How much this pixel may shimmer (0..1): by its distance, by its view ray's elevation above
// the horizon (degrees), and less for the sky itself.
fn haze_mask(dist: f32, elevation_deg: f32) -> f32 {
    let far = smoothstep(haze.far.x, haze.far.y, dist);
    let above = 1.0 - smoothstep(haze.elevation.x, haze.elevation.y, elevation_deg);
    let below = smoothstep(haze.elevation.w, haze.elevation.z, elevation_deg);
    let sky = select(1.0, haze.far.z, dist >= haze.proj.z);
    return far * above * below * sky;
}

@fragment
fn heat_haze(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = vec2<i32>(floor(in.position.xy));
    let src = textureLoad(picture, px, 0);
    let size = vec2<f32>(textureDimensions(picture));
    // The pixel's view ray (view space, one unit of view depth) and its elevation.
    let ndc = vec2<f32>(in.uv.x * 2.0 - 1.0, 1.0 - in.uv.y * 2.0);
    let ray = vec3<f32>(ndc.x / haze.proj.x, ndc.y / haze.proj.y, -1.0);
    let ray_len = length(ray);
    let sin_up = clamp(dot(ray, haze.up.xyz) / ray_len, -1.0, 1.0);
    var m = haze_mask(distance_at(px, ray_len), degrees(asin(sin_up)));
    // The neighbours' depths only where the pixel itself may shimmer.
    if m > 0.0 && near_around(px, ray_len) {
        m = 0.0;
    }
    if haze.params.w > 0.5 {
        return vec4<f32>(vec3<f32>(m * haze.params.x), src.a);
    }
    if m <= 0.0 {
        return src;
    }
    // Two layers of noise over the picture, in picture heights, scrolled (rising, drifting
    // with the wind, following the camera's turns: heat_haze.rs `follow_weather`).
    let c = in.position.xy / size.y;
    let la = c * haze.scales.xy + haze.scroll.xy;
    let lb = c * haze.scales.zw + haze.scroll.zw;
    let na = vec2<f32>(value_noise(la, 1u), value_noise(la, 2u));
    let nb = vec2<f32>(value_noise(lb, 3u), value_noise(lb, 4u));
    let n = na * 0.65 + nb * 0.35;
    let amp = haze.params.x * haze.params.y * size.y * m;
    let q = in.position.xy + vec2<f32>(n.x * haze.proj.w, n.y) * amp;
    // A displaced read whose bilinear footprint touches something near is not taken: near
    // things never bleed into the shimmer around their outline.
    if footprint_near(q, size, ray_len) {
        return src;
    }
    let moved = textureSampleLevel(picture, picture_sampler, q / size, 0.0);
    return vec4<f32>(moved.rgb, src.a);
}
