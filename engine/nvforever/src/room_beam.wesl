// Accumulate optical depth for the window shafts [design, #4698].
// The depth prepass clips the view ray against the scene. A five-point Gauss-Legendre
// rule integrates the degree-nine density outside the near fade.

import nvf_assets::first_person_depth::FIRST_PERSON_DEPTH_SHARE;

struct BeamView {
    // Bevy world to clip for this frame's depth projection.
    clip_from_world: mat4x4<f32>,
    // The camera's world-from-view transform (unjittered).
    world_from_view: mat4x4<f32>,
    // x, y: the projection's x and y scale; z: its near plane; w: the beams this frame.
    proj: vec4<f32>,
    // The main pass's viewport: origin and size in pixels.
    viewport: vec4<f32>,
    // x: PEAK; y: G; z: PHASECAP; w: NEAR.
    tune: vec4<f32>,
    // yz: jitter offset in the reverse infinite projection.
    flags: vec4<f32>,
}

struct Beam {
    // xyz: the middle of the bright end; w: the length.
    a: vec4<f32>,
    // xyz: the section's first axis; w: its half width.
    u: vec4<f32>,
    // xyz: the second axis; w: its half width.
    v: vec4<f32>,
    // xyz: the axis (from the window into the room); w: 1 / the geometric mean of the full widths.
    axis: vec4<f32>,
    // xy: how the end planes lean (along the axis per unit of u and v).
    lean: vec4<f32>,
    // rgb: the colour (display values).
    colour: vec4<f32>,
}

@group(0) @binding(0) var<uniform> beam_view: BeamView;
@group(0) @binding(1) var<storage, read> beams: array<Beam>;
@if(MULTISAMPLED) @group(0) @binding(2) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(2) var depth: texture_depth_2d;

// The prism's triangles over its 8 corners, counter-clockwise seen from outside (room_beam.rs
// `BOX`): corners 0-3 the bright end's ring, 4-7 the far end's.
const BEAM_BOX = array<u32, 36>(0u, 2u, 1u, 0u, 3u, 2u, 4u, 5u, 6u, 4u, 6u, 7u, 0u, 1u, 5u, 0u, 5u, 4u, 1u, 2u, 6u, 1u, 6u, 5u, 2u, 3u, 7u, 2u, 7u, 6u, 3u, 0u, 4u, 3u, 4u, 7u);

// Gauss-Legendre nodes and weights on [-1, 1] (room_beam.rs tests mirror them).
const BEAM_G5_X = array<f32, 5>(0.0, -0.5384693, 0.5384693, -0.9061798, 0.9061798);
const BEAM_G5_W = array<f32, 5>(0.5688889, 0.47862867, 0.47862867, 0.23692688, 0.23692688);
const BEAM_MEAN: f32 = 0.53333336;

struct BeamOut {
    @builtin(position) position: vec4<f32>,
    @location(0) @interpolate(flat) index: u32,
}

// Corner `c` of a beam's ring, in half widths.
fn beam_ring(c: u32) -> vec2<f32> {
    var ring = array<vec2<f32>, 4>(vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0));
    return ring[c % 4u];
}

@vertex
fn beam_vertex(@builtin(vertex_index) vertex: u32, @builtin(instance_index) instance: u32) -> BeamOut {
    var box = BEAM_BOX;
    let corner = box[vertex % 36u];
    let b = beams[instance];
    let r = beam_ring(corner) * vec2<f32>(b.u.w, b.v.w);
    let z = select(0.0, b.a.w, corner >= 4u) + dot(b.lean.xy, r);
    let world = b.a.xyz + b.u.xyz * r.x + b.v.xyz * r.y + b.axis.xyz * z;
    var out: BeamOut;
    out.position = beam_view.clip_from_world * vec4<f32>(world, 1.0);
    out.index = instance;
    return out;
}

// A ray in a beam's own space: origin and direction (x, y across, z along), and s = z - lean.xy
// (the distance from the bright end's leaning plane, along the axis) at the origin and per unit.
struct BeamRay {
    o: vec3<f32>,
    d: vec3<f32>,
    s0: f32,
    sd: f32,
}

fn beam_ray(b: Beam, eye: vec3<f32>, dir: vec3<f32>) -> BeamRay {
    let rel = eye - b.a.xyz;
    var r: BeamRay;
    r.o = vec3<f32>(dot(rel, b.u.xyz), dot(rel, b.v.xyz), dot(rel, b.axis.xyz));
    r.d = vec3<f32>(dot(dir, b.u.xyz), dot(dir, b.v.xyz), dot(dir, b.axis.xyz));
    r.s0 = r.o.z - dot(b.lean.xy, r.o.xy);
    r.sd = r.d.z - dot(b.lean.xy, r.d.xy);
    return r;
}

// The part of `range` where o + d t lies between lo and hi (empty: x above y).
fn beam_slab(o: f32, d: f32, lo: f32, hi: f32, range: vec2<f32>) -> vec2<f32> {
    if abs(d) < 1e-8 {
        if o < lo || o > hi {
            return vec2<f32>(1.0, 0.0);
        }
        return range;
    }
    let ta = (lo - o) / d;
    let tb = (hi - o) / d;
    return vec2<f32>(max(range.x, min(ta, tb)), min(range.y, max(ta, tb)));
}

// The ray's distances inside the prism, from the eye to `far` at most.
fn beam_clip(b: Beam, r: BeamRay, far: f32) -> vec2<f32> {
    var range = vec2<f32>(0.0, far);
    range = beam_slab(r.o.x, r.d.x, -b.u.w, b.u.w, range);
    range = beam_slab(r.o.y, r.d.y, -b.v.w, b.v.w, range);
    range = beam_slab(r.s0, r.sd, 0.0, b.a.w, range);
    return range;
}

fn beam_density(b: Beam, r: BeamRay, t: f32, near: f32) -> f32 {
    let x = (r.o.x + r.d.x * t) / b.u.w;
    let y = (r.o.y + r.d.y * t) / b.v.w;
    let w = (r.s0 + r.sd * t) / b.a.w;
    let px = max(1.0 - x * x, 0.0);
    let py = max(1.0 - y * y, 0.0);
    let pw = clamp(1.0 - w, 0.0, 1.0);
    let density = px * px * py * py * pw;
    if near > 0.0 {
        return density * smoothstep(0.0, near, t);
    }
    return density;
}

fn beam_gauss(b: Beam, r: BeamRay, t0: f32, t1: f32, near: f32) -> f32 {
    let mid = 0.5 * (t0 + t1);
    let half = 0.5 * (t1 - t0);
    var sum = 0.0;
    for (var i = 0u; i < 5u; i = i + 1u) {
        sum = sum + BEAM_G5_W[i] * beam_density(b, r, mid + half * BEAM_G5_X[i], near);
    }
    return sum * half;
}

// The integral over `range`, split where the near fade ends.
fn beam_integral(b: Beam, r: BeamRay, range: vec2<f32>, near: f32) -> f32 {
    if range.y <= range.x {
        return 0.0;
    }
    let split = clamp(near, range.x, range.y);
    var sum = 0.0;
    if split > range.x {
        sum = sum + beam_gauss(b, r, range.x, split, near);
    }
    if range.y > split {
        sum = sum + beam_gauss(b, r, split, range.y, 0.0);
    }
    return sum;
}

// The phase toward the window (room_beam.rs `phase`): Henyey-Greenstein G on the cosine between
// the axis and the ray to the eye, 1 at a right angle, at most PHASECAP.
fn beam_phase(cos_theta: f32) -> f32 {
    let g = beam_view.tune.y;
    let g2 = g * g;
    return min(pow((1.0 + g2) / max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4), 1.5), beam_view.tune.z);
}

@if(MULTISAMPLED) fn beam_samples() -> u32 { return textureNumSamples(depth); }
@if(!MULTISAMPLED) fn beam_samples() -> u32 { return 1u; }

// The scene's distance along the ray at depth sample `s` (reverse infinite z: view depth =
// near / depth); cleared depth is far away. A first-person sample (whatever is drawn with its
// depth squeezed into the nearest share of the range: the arms and weapon in view, nvf_assets
// first_person_depth.wesl) is distance 0: no beam over them, whatever is between them and the
// eye.
fn beam_scene(px: vec2<i32>, s: u32, ray_len: f32) -> f32 {
    let dims = vec2<i32>(textureDimensions(depth));
    let d = textureLoad(depth, clamp(px, vec2<i32>(0), dims - vec2<i32>(1)), i32(s));
    if d > 1.0 - FIRST_PERSON_DEPTH_SHARE {
        return 0.0;
    }
    if d <= 0.0 {
        return 1e30;
    }
    return beam_view.proj.z / d * ray_len;
}

@fragment
fn beam_fragment(in: BeamOut) -> @location(0) vec4<f32> {
    let b = beams[in.index];
    // The view ray through this pixel (the main pass's own region).
    let size = max(beam_view.viewport.zw, vec2<f32>(1.0));
    let uv = (in.position.xy - beam_view.viewport.xy) / size;
    let ndc = vec2<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0);
    let view_xy = (ndc + beam_view.flags.yz) / beam_view.proj.xy;
    let ray_view = vec3<f32>(view_xy, -1.0);
    let ray_len = length(ray_view);
    let m = beam_view.world_from_view;
    let dir = normalize(mat3x3<f32>(m[0].xyz, m[1].xyz, m[2].xyz) * ray_view);
    let r = beam_ray(b, m[3].xyz, dir);
    let near = beam_view.tune.w;
    // The scene: the nearest and farthest depth samples (MSAA edges, High), mixed by count.
    let px = vec2<i32>(floor(in.position.xy));
    let n = beam_samples();
    var nearest = 1e30;
    var farthest = 0.0;
    for (var s = 0u; s < n; s = s + 1u) {
        let t = beam_scene(px, s, ray_len);
        nearest = min(nearest, t);
        farthest = max(farthest, t);
    }
    let range = beam_clip(b, r, farthest);
    if range.y <= range.x {
        discard;
    }
    var integral = beam_integral(b, r, range, near);
    if nearest < farthest * 0.999 {
        let middle = 0.5 * (nearest + farthest);
        var close = 0u;
        for (var s = 0u; s < n; s = s + 1u) {
            close = close + select(0u, 1u, beam_scene(px, s, ray_len) < middle);
        }
        let near_integral = beam_integral(b, r, beam_clip(b, r, nearest), near);
        integral = mix(integral, near_integral, f32(close) / f32(n));
    }
    let tau = integral * b.axis.w / BEAM_MEAN;
    if tau <= 1e-6 {
        discard;
    }
    let phase = beam_phase(dot(b.axis.xyz, -dir));
    return vec4<f32>(b.colour.rgb * (tau * phase), tau);
}
