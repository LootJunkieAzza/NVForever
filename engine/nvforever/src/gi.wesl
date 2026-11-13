// Screen-space global illumination of the remastered look (gi.rs, docs/design/
// graphics-remaster.md). An engineering choice, not the original game's.
//
// Entries, in the order they run each frame:
// - `trace` (half resolution, after the depth prepass): for each 2 x 2 block's top-left
//   pixel, two slices of the hemisphere above it are marched in screen space; each slice
//   keeps a 32-sector bitmask of the angles the depth buffer closes (each sample a surface
//   of `thickness` behind what the depth shows, so thin things do not close the whole
//   horizon behind them: "screen space indirect lighting with visibility bitmask"). The
//   sectors a sample closes first take its light from last frame's picture (`history`,
//   reprojected), weighed by the cosine to the normal; the share of sectors left open is
//   how open the pixel is to the ambient light.
// - `temporal`: that blended with last frame's result at the same surface (90 % old while
//   the distance agrees), so 2 slices a frame grow to about 20.
// - `upsample` (full resolution): the 3 x 3 half-resolution texels around each pixel,
//   weighed by how near their distance is to the pixel's, into the texture the lighting
//   reads (bevy_pbr ScreenIndirectLight, lights.rs `nvf_open`, `nvf_bounce`).
// - `capture` (half resolution, after the main passes, before tonemapping): the HDR picture
//   averaged over each 2 x 2 block, with the block's distance, as next frame's `history`.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import nvf_assets::first_person_depth::unsqueeze;

struct Gi {
    // Last frame's view from this frame's view (camera motion).
    prev_view_from_view: mat4x4<f32>,
    // x, y: the projection's x and y scale; z: near plane; w: frame number.
    proj: vec4<f32>,
    // x, y: the picture's size (pixels); z, w: the half-resolution size.
    size: vec4<f32>,
    // x: radius (game units); y: thickness (game units); z: bounce strength; w: the
    // distance where the effect has faded out.
    params: vec4<f32>,
    // x: occlusion strength; y: the share of the new frame in `temporal`; z: the most
    // light a history texel gives; w: 1 when last frame's textures hold last frame.
    params2: vec4<f32>,
    // x: a debug view (1 distance, 2 openness, 3 bounced light), y: its scale.
    debug: vec4<f32>,
}

@if(MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> gi: Gi;
// trace: last frame's picture (rgb) and distance (a), half resolution.
// temporal: this frame's trace. upsample: the accumulated result. capture: the HDR picture.
@group(0) @binding(2) var input_a: texture_2d<f32>;
@group(0) @binding(3) var linear_sampler: sampler;
// temporal: last frame's accumulated result. upsample: this frame's distances.
@group(0) @binding(4) var input_b: texture_2d<f32>;
// temporal: last frame's distances (history alpha) and this frame's (r).
@group(0) @binding(5) var input_c: texture_2d<f32>;
@group(0) @binding(6) var input_d: texture_2d<f32>;

const PI: f32 = 3.14159265;
const HALF_PI: f32 = 1.5707963;
const SLICES: u32 = 2u;
const STEPS: u32 = 8u;
// Distance written for the sky (depth 0) and the most a texel holds.
const SKY: f32 = 60000.0;

fn full_dims() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth));
}

fn depth_at(p: vec2<i32>) -> f32 {
    return unsqueeze(textureLoad(depth, clamp(p, vec2(0), full_dims() - 1), 0));
}

// View-space position of the picture's point `p` (pixels) at depth `d` (reverse
// infinite z: distance = near / d), looking down -z.
fn view_pos(p: vec2<f32>, d: f32) -> vec3<f32> {
    let size = gi.size.xy;
    let ndc = vec2(p.x / size.x * 2.0 - 1.0, 1.0 - p.y / size.y * 2.0);
    let z = gi.proj.z / max(d, 1e-7);
    return vec3(ndc.x * z / gi.proj.x, ndc.y * z / gi.proj.y, -z);
}

fn pos_at(px: vec2<i32>) -> vec3<f32> {
    return view_pos(vec2<f32>(px) + 0.5, depth_at(px));
}

// Normal from the neighbouring depths, on each axis the side with the smaller step.
fn normal_at(px: vec2<i32>, p: vec3<f32>) -> vec3<f32> {
    let l = pos_at(px - vec2(1, 0));
    let r = pos_at(px + vec2(1, 0));
    let u = pos_at(px - vec2(0, 1));
    let b = pos_at(px + vec2(0, 1));
    let dx = select(p - l, r - p, abs(r.z - p.z) < abs(p.z - l.z));
    let dy = select(p - u, b - p, abs(b.z - p.z) < abs(p.z - u.z));
    var n = normalize(cross(dx, dy));
    if dot(n, -p) < 0.0 {
        n = -n;
    }
    return n;
}

// Where view-space point `p` was on the screen last frame (0..1), and its distance then.
fn prev_uv(p: vec3<f32>) -> vec3<f32> {
    let q = (gi.prev_view_from_view * vec4(p, 1.0)).xyz;
    let z = max(-q.z, 1e-3);
    let ndc = vec2(q.x * gi.proj.x / z, q.y * gi.proj.y / z);
    return vec3(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5, -q.z);
}

fn inside(uv: vec2<f32>) -> bool {
    return all(uv >= vec2(0.0)) && all(uv <= vec2(1.0));
}

// Interleaved gradient noise (Jimenez 2014), turned each frame.
fn ign(p: vec2<f32>, frame: f32) -> f32 {
    let q = p + 5.588238 * (frame % 64.0);
    return fract(52.9829189 * fract(dot(q, vec2(0.06711056, 0.00583715))));
}

fn sector_mask(t0: f32, t1: f32) -> u32 {
    let a = clamp(t0, 0.0, 1.0);
    let b = clamp(t1, 0.0, 1.0);
    let start = u32(floor(min(a, b) * 32.0));
    let count = u32(ceil((max(a, b) - min(a, b)) * 32.0));
    if count == 0u || start >= 32u {
        return 0u;
    }
    let bits = select(0xFFFFFFFFu >> (32u - min(count, 32u)), 0xFFFFFFFFu, count >= 32u);
    return bits << start;
}

struct TraceOut {
    @location(0) light: vec4<f32>,
    @location(1) dist: vec4<f32>,
}

@fragment
fn trace(in: FullscreenVertexOutput) -> TraceOut {
    var out: TraceOut;
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let d = depth_at(px);
    let p = view_pos(vec2<f32>(px) + 0.5, d);
    let z = -p.z;
    out.dist = vec4(min(z, SKY), 0.0, 0.0, 0.0);
    out.light = vec4(0.0);
    if d <= 0.0 || z >= gi.params.w {
        return out;
    }
    let n = normal_at(px, p);
    let v = normalize(-p);
    let frame = gi.proj.w;
    let noise_a = ign(vec2<f32>(half), frame);
    let noise_b = ign(vec2<f32>(half) + vec2(17.0, 59.0), frame + 7.0);
    // The radius on the screen, in the picture's pixels.
    let radius_px = clamp(gi.params.x * gi.proj.y * 0.5 * gi.size.y / z, f32(STEPS), 0.3 * gi.size.y);
    let thickness = gi.params.y * max(1.0, z / 1500.0);
    var light = vec3(0.0);
    var closed = 0.0;
    for (var s = 0u; s < SLICES; s++) {
        let phi = (f32(s) + noise_a) * PI / f32(SLICES);
        let dir = vec2(cos(phi), sin(phi));
        let dir_v = vec3(dir.x, -dir.y, 0.0);
        let ortho = dir_v - dot(dir_v, v) * v;
        let axis = normalize(cross(ortho, v));
        let proj_n = n - axis * dot(n, axis);
        let proj_len = max(length(proj_n), 1e-4);
        let sign_n = select(-1.0, 1.0, dot(ortho, proj_n) >= 0.0);
        let n_angle = sign_n * acos(clamp(dot(proj_n, v) / proj_len, -1.0, 1.0));
        var mask = 0u;
        for (var side = 0u; side < 2u; side++) {
            let sgn = select(-1.0, 1.0, side == 0u);
            for (var i = 0u; i < STEPS; i++) {
                var t = (f32(i) + fract(noise_b + f32(i) * 0.618034)) / f32(STEPS);
                t = t * t;
                let off = max(t * radius_px, f32(i) + 1.5);
                let sp = vec2<f32>(px) + 0.5 + sgn * dir * off;
                if any(sp < vec2(0.0)) || any(sp >= gi.size.xy) {
                    break;
                }
                let spx = vec2<i32>(floor(sp));
                let sd = depth_at(spx);
                if sd <= 0.0 {
                    continue;
                }
                let q = view_pos(sp, sd);
                let delta = q - p;
                let len2 = dot(delta, delta);
                if len2 < 1e-4 {
                    continue;
                }
                let front = delta * inverseSqrt(len2);
                let back = normalize(delta - v * thickness);
                let a_front = sgn * acos(clamp(dot(front, v), -1.0, 1.0));
                let a_back = sgn * acos(clamp(dot(back, v), -1.0, 1.0));
                // Sectors equal in projected solid angle (cosine-weighted): sin of the
                // angle from the normal, so the share of sectors is the share of the
                // irradiance a uniform light would give.
                let t_front = 0.5 + 0.5 * sin(clamp(a_front - n_angle, -HALF_PI, HALF_PI));
                let t_back = 0.5 + 0.5 * sin(clamp(a_back - n_angle, -HALF_PI, HALF_PI));
                let m = sector_mask(t_front, t_back);
                let fresh = m & ~mask;
                if fresh != 0u && gi.params2.w > 0.5 {
                    if dot(n, front) > 0.0 {
                        let h = prev_uv(q);
                        if inside(h.xy) {
                            let hist = textureSampleLevel(input_a, linear_sampler, h.xy, 0.0);
                            // Only light from the same surface last frame (not what moved in front).
                            if abs(hist.a - h.z) < 0.1 * h.z + 20.0 {
                                let falloff = 1.0 - smoothstep(0.6, 1.0, sqrt(len2) / (gi.params.x * 1.5));
                                light += min(hist.rgb, vec3(gi.params2.z)) * (f32(countOneBits(fresh)) / 32.0) * falloff;
                            }
                        }
                    }
                }
                mask |= m;
            }
        }
        closed += f32(countOneBits(mask)) / 32.0;
    }
    let fade = 1.0 - smoothstep(gi.params.w * 0.6, gi.params.w, z);
    let occlusion = min(saturate(closed / f32(SLICES)), gi.params2.x) * fade;
    out.light = vec4(light * (gi.params.z / f32(SLICES)) * fade, occlusion);
    return out;
}

@fragment
fn temporal(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let current = textureLoad(input_a, half, 0);
    let z = textureLoad(input_c, half, 0).r;
    if gi.params2.w < 0.5 || z >= SKY {
        return current;
    }
    let px = half * 2;
    let p = view_pos(vec2<f32>(px) + 0.5, gi.proj.z / z);
    let h = prev_uv(p);
    if !inside(h.xy) {
        return current;
    }
    // Last frame's distance at that spot (history alpha) against this point's then.
    let then = textureSampleLevel(input_d, linear_sampler, h.xy, 0.0).a;
    if abs(then - h.z) > 0.05 * h.z + 10.0 {
        return current;
    }
    let old = textureSampleLevel(input_b, linear_sampler, h.xy, 0.0);
    return mix(old, current, gi.params2.y);
}

@fragment
fn upsample(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    if gi.debug.x > 0.5 {
        let hp = vec2<i32>(floor(in.position.xy * 0.5));
        let acc = textureLoad(input_a, hp, 0);
        let dd = textureLoad(input_b, hp, 0).r;
        if gi.debug.x < 1.5 { return vec4(vec3(dd / 3000.0), -1.0); }
        if gi.debug.x < 2.5 { return vec4(vec3(1.0 - acc.a), -1.0); }
        return vec4(acc.rgb * gi.debug.y, -1.0);
    }
    let px = vec2<i32>(floor(in.position.xy));
    let d = depth_at(px);
    if d <= 0.0 {
        return vec4(0.0);
    }
    let z = gi.proj.z / d;
    let half_dims = vec2<i32>(textureDimensions(input_a));
    let c = vec2<i32>(floor(vec2<f32>(px) * 0.5));
    var sum = vec4(0.0);
    var wsum = 0.0;
    var nearest = vec4(0.0);
    var nearest_dz = 1e9;
    for (var y = -1; y <= 1; y++) {
        for (var x = -1; x <= 1; x++) {
            let t = clamp(c + vec2(x, y), vec2(0), half_dims - 1);
            let tz = textureLoad(input_b, t, 0).r;
            let v = textureLoad(input_a, t, 0);
            let dz = abs(tz - z);
            let spatial = select(0.5, 1.0, x == 0 && y == 0);
            let w = spatial * exp(-dz / (0.03 * z + 4.0));
            sum += v * w;
            wsum += w;
            if dz < nearest_dz {
                nearest_dz = dz;
                nearest = v;
            }
        }
    }
    if wsum < 1e-3 {
        return nearest;
    }
    return sum / wsum;
}

@fragment
fn capture(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let uv = (vec2<f32>(px) + 1.0) / gi.size.xy;
    let c = textureSampleLevel(input_a, linear_sampler, uv, 0.0).rgb;
    let d = depth_at(px);
    let z = select(SKY, min(gi.proj.z / max(d, 1e-7), SKY), d > 0.0);
    return vec4(max(c, vec3(0.0)), z);
}
