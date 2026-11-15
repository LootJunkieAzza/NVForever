// Volumetric flame (fire_volume.rs): a ray marched through a box around a fire, through a
// flame shaped from rising 3D noise, coloured by its heat. NVForever's own [design]; the
// technique (noise-shaped density, temperature to colour, emission summed along the ray)
// is the common one of real-time volumetric fire.
//
// Bevy world space: y up.

import bevy_pbr::{
    render::forward_io::VertexOutput,
    render::mesh_view_bindings::{view, globals},
    render::view_transformations::position_ndc_to_world,
};
@if(DEPTH_PREPASS) import bevy_pbr::prepass::utils as prepass_utils;

struct FireParams {
    // xyz: the flame's base in Bevy world space; w: its height (units).
    base: vec4<f32>,
    // x: radius at the base (units), y: brightness, z: seed, w: steps.
    shape: vec4<f32>,
    // x: noise frequency across, y: density gain, z: edge, w: swirl.
    look: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> fire: FireParams;

// The march's bounds (fire_volume.rs MIN_STEPS / MAX_STEPS).
const MIN_STEPS: i32 = 4;
const MAX_STEPS: i32 = 40;

fn frag_coord_to_ndc(fc: vec4<f32>) -> vec3<f32> {
    let uv = (fc.xy - view.main_pass_viewport.xy) / view.main_pass_viewport.zw;
    return vec3(uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0), fc.z);
}

fn hash3(p: vec3<f32>) -> f32 {
    let q = fract(p * vec3(0.1031, 0.1030, 0.0973));
    let r = q + dot(q, q.yzx + 33.33);
    return fract((r.x + r.y) * r.z);
}

// Value noise in 0..1, smoothly eased between lattice points.
fn noise3(p: vec3<f32>) -> f32 {
    let i = floor(p);
    let f = fract(p);
    let u = f * f * (3.0 - 2.0 * f);
    let a = mix(mix(hash3(i), hash3(i + vec3(1.0, 0.0, 0.0)), u.x),
                mix(hash3(i + vec3(0.0, 1.0, 0.0)), hash3(i + vec3(1.0, 1.0, 0.0)), u.x), u.y);
    let b = mix(mix(hash3(i + vec3(0.0, 0.0, 1.0)), hash3(i + vec3(1.0, 0.0, 1.0)), u.x),
                mix(hash3(i + vec3(0.0, 1.0, 1.0)), hash3(i + vec3(1.0, 1.0, 1.0)), u.x), u.y);
    return mix(a, b, u.z);
}

fn fbm(p: vec3<f32>) -> f32 {
    var s = 0.0;
    var amp = 0.5;
    var q = p;
    for (var i = 0; i < 4; i += 1) {
        s += amp * noise3(q);
        q = q * 2.03 + vec3(1.7, 9.2, 4.1);
        amp *= 0.5;
    }
    return s;
}

// Flame density and heat at `q` (x and z across, in heights; y up, 0 at the base and
// 1 at the top), at time `t`.
// Two octaves, for the swirl that bends the flame (four were three times its cost and
// not seen apart, fire_volume.rs "Cost").
fn fbm2(p: vec3<f32>) -> f32 {
    return 0.5 * noise3(p) + 0.25 * noise3(p * 2.03 + vec3(1.7, 9.2, 4.1));
}

fn flame(q: vec3<f32>, t: f32) -> vec2<f32> {
    let seed = fire.shape.z;
    let freq = fire.look.x;
    let radius = fire.shape.x / fire.base.w;
    // Past the teardrop's reach whatever the noise does (the swirl pushes at most
    // 0.71 x swirl x (0.2 + height) aside, the noise adds at most 0.44 x (1 + 1.4 x height)):
    // empty, and no noise is drawn for it. Most of the box's corners are.
    if q.y <= 0.0 || q.y >= 1.0 {
        return vec2(0.0);
    }
    let w = radius * (1.0 - q.y) * (0.7 + 0.3 * smoothstep(0.0, 0.2, q.y));
    let reach = 0.71 * abs(fire.look.w) * (0.2 + q.y);
    let most = (1.0 - max(length(q.xz) - reach, 0.0) / max(w, 1e-3)) * fire.look.z + 0.44 * (1.0 + 1.4 * q.y) - q.y * 0.45;
    if fire.look.z >= 0.0 && most <= 0.0 {
        return vec2(0.0);
    }
    // Rising, swirling noise: faster and wilder higher up, as hot gas speeds up.
    let rise = vec3(0.0, t * 1.2, 0.0);
    let warp = (vec3(fbm2(q * 2.0 - rise + seed), 0.0, fbm2(q * 2.0 - rise + seed + 7.3)) - vec3(0.375, 0.0, 0.375)) * 1.33;
    let p = q + warp * fire.look.w * (0.2 + q.y);
    let n = fbm(vec3(p.x * freq, p.y * freq * 0.55 - t * 2.4, p.z * freq) + seed);
    // A teardrop: widest a little above the base, closing to the tip (`w` above).
    let body = 1.0 - length(p.xz) / max(w, 1e-3);
    let d = body * fire.look.z + (n - 0.5) * (1.0 + 1.4 * q.y) - q.y * 0.45;
    // Sharp-edged: the noise cuts the body into tongues.
    let density = smoothstep(0.0, 0.18, d) * smoothstep(0.0, 0.05, q.y) * (1.0 - smoothstep(0.85, 1.0, q.y));
    let hot = clamp(d * 1.6 * (1.0 - 0.55 * q.y), 0.0, 1.0);
    return vec2(density, hot);
}

// Heat to colour: deep red where it is cool, through orange to a pale yellow core;
// the core above 1 so the light glow takes it.
fn heat(t: f32) -> vec3<f32> {
    let red = vec3(0.35, 0.04, 0.005);
    let orange = vec3(1.0, 0.32, 0.04);
    let yellow = vec3(1.6, 1.05, 0.45);
    return mix(mix(red, orange, smoothstep(0.05, 0.45, t)), yellow, smoothstep(0.5, 1.0, t));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let eye = view.world_position.xyz;
    let dir = normalize(in.world_position.xyz - eye);
    let h = fire.base.w;
    let r = fire.shape.x;
    let lo = fire.base.xyz - vec3(r, 0.0, r);
    let hi = fire.base.xyz + vec3(r, h, r);
    // The ray through the box.
    let inv = 1.0 / dir;
    let t0 = (lo - eye) * inv;
    let t1 = (hi - eye) * inv;
    let tn = min(t0, t1);
    let tf = max(t0, t1);
    var enter = max(max(max(tn.x, tn.y), tn.z), 0.0);
    var leave = min(min(tf.x, tf.y), tf.z);
    // Stopped by what the scene drew in front (the logs, the ground, a wall).
    @if(DEPTH_PREPASS) if true {
        let depth = prepass_utils::prepass_depth(in.position, 0u);
        if depth > 0.0 {
            let scene = position_ndc_to_world(vec3(frag_coord_to_ndc(in.position).xy, depth));
            leave = min(leave, distance(scene, eye));
        }
    }
    if !(leave > enter) || leave - enter > 4.0 * (h + 2.0 * r) {
        discard;
    }
    // The steps fire_volume.rs chose for this flame's share of the screen, never more
    // than MAX_STEPS whatever the uniform holds (a NaN or a huge value would march for
    // ever: fire_volume.rs "Cost").
    let wanted = fire.shape.w;
    let steps = clamp(select(MIN_STEPS, i32(wanted), wanted == wanted && wanted >= 1.0), MIN_STEPS, MAX_STEPS);
    let step = (leave - enter) / f32(steps);
    // A per-pixel start offset hides the steps' banding.
    let jitter = fract(52.9829189 * fract(dot(in.position.xy, vec2(0.06711056, 0.00583715))) + globals.time * 7.0);
    var light = vec3(0.0);
    var through = 1.0;
    let t = globals.time;
    for (var i = 0; i < steps; i += 1) {
        let at = eye + dir * (enter + (f32(i) + jitter) * step);
        let q = (at - fire.base.xyz) / h;
        let f = flame(q, t);
        if f.x > 0.001 {
            // Optical depth of this step: the flame's front hides what burns behind it,
            // so its tongues show, not a sum of everything along the ray.
            let k = f.x * step / h * fire.look.y * 6.0;
            let a = 1.0 - exp(-k);
            light += heat(f.y) * a * through;
            through *= 1.0 - a;
            if through < 0.02 {
                break;
            }
        }
    }
    light *= fire.shape.y;
    if max(light.r, max(light.g, light.b)) < 0.002 {
        discard;
    }
    // Added to what is behind.
    return vec4(light, 0.0);
}
