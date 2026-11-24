// 1-bounce linear HDR Screen-Space Global Illumination (nvforever ssgi.rs; Phase 2 under
// `NVF_PHYSICAL_PBR=1`). Runs at half resolution after temporal upscaling and before bloom,
// auto-exposure, and AgX tonemapping: traces indirect diffuse bounce radiance from nearby
// lit surfaces using horizon-based differential form factors, denoises with a reverse-Z
// plane-aware bilateral filter, and composites onto world pixels using estimated diffuse
// reflectance (skipping sky, `SCREEN_MARK` Pip-Boy screen, and `FIRST_PERSON_DEPTH` arms).

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import nvf_assets::first_person_depth::FIRST_PERSON_DEPTH_SHARE;

struct Ssgi {
    // x: world radius (game units), y: composite strength, z: projection near plane, w: fade end.
    params: vec4<f32>,
    // x, y: projection x and y scale, z: max screen radius (px), w: fade start.
    proj: vec4<f32>,
    // x, y: depth render size, z, w: picture size (all 1 when native).
    render_scale: vec4<f32>,
}

@if(MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> gi: Ssgi;
@group(0) @binding(2) var screen: texture_2d<f32>;
@group(0) @binding(3) var screen_sampler: sampler;
@group(0) @binding(4) var gi_tex: texture_2d<f32>;

const SKY: f32 = 60000.0;
const SCREEN_MARK: f32 = 0.25;
const SSGI_SLICES: i32 = 4;
const SSGI_STEPS: i32 = 8;
const PI: f32 = 3.14159265;
const HALF_PI: f32 = 1.57079633;
const HORIZON_BIAS: f32 = 0.04;
const DENOISE_TOLERANCE: f32 = 0.03;
const FALLBACK_TOLERANCE: f32 = 0.05;
const PLANE_WEIGHT_FEW: f32 = 1.0;
const PLANE_WEIGHT_BLEND: f32 = 2.0;
const SPARSE_FULL: f32 = 3.0;
const SPARSE_NONE: f32 = 8.0;
const SOFT_TOLERANCE: f32 = 0.4;
const SOFT_PULL: f32 = 0.65;

@if(MULTISAMPLED) fn depth_samples() -> i32 { return i32(textureNumSamples(depth)); }
@if(!MULTISAMPLED) fn depth_samples() -> i32 { return 1; }

fn dims() -> vec2<i32> {
    return vec2<i32>(textureDimensions(depth));
}

fn is_first_person(raw_d: f32) -> bool {
    return raw_d > 1.0 - FIRST_PERSON_DEPTH_SHARE;
}

fn raw_depth_at(p: vec2<i32>) -> f32 {
    return textureLoad(depth, clamp(p, vec2(0), dims() - 1), 0);
}

fn drawn_texel(p: vec2<f32>) -> vec2<f32> {
    return floor(p * gi.render_scale.xy / gi.render_scale.zw);
}

fn texel_centre(t: vec2<f32>) -> vec2<f32> {
    return (t + 0.5) * gi.render_scale.zw / gi.render_scale.xy;
}

fn view_pos(p: vec2<f32>, d: f32) -> vec3<f32> {
    let size = vec2<f32>(dims());
    let ndc = vec2(p.x / size.x * 2.0 - 1.0, 1.0 - p.y / size.y * 2.0);
    let z = gi.params.z / max(d, 1e-7);
    return vec3(ndc.x * z / gi.proj.x, ndc.y * z / gi.proj.y, -z);
}

fn texel_pos(t: vec2<f32>) -> vec3<f32> {
    let c = texel_centre(t);
    return view_pos(c, raw_depth_at(vec2<i32>(floor(c))));
}

fn normal_at(px: vec2<i32>, d: f32) -> vec3<f32> {
    let t = drawn_texel(vec2<f32>(px) + 0.5);
    let p = view_pos(texel_centre(t), d);
    let l = texel_pos(t - vec2(1.0, 0.0));
    let r = texel_pos(t + vec2(1.0, 0.0));
    let u = texel_pos(t - vec2(0.0, 1.0));
    let b = texel_pos(t + vec2(0.0, 1.0));
    let dx = select(p - l, r - p, abs(r.z - p.z) < abs(p.z - l.z));
    let dy = select(p - u, b - p, abs(b.z - p.z) < abs(p.z - u.z));
    let cxy = cross(dx, dy);
    let nlen2 = dot(cxy, cxy);
    var n = select(vec3(0.0, 0.0, 1.0), cxy * inverseSqrt(nlen2), nlen2 > 1e-10);
    if dot(n, -p) < 0.0 {
        n = -n;
    }
    return n;
}

// Symmetric min-depth-delta view-space emitter normal estimate around drawn texel at `st`.
fn emitter_normal_at(st: vec2<f32>, p: vec3<f32>) -> vec3<f32> {
    let td = drawn_texel(st);
    let pl = texel_pos(td - vec2(1.0, 0.0));
    let pr = texel_pos(td + vec2(1.0, 0.0));
    let pu = texel_pos(td - vec2(0.0, 1.0));
    let pb = texel_pos(td + vec2(0.0, 1.0));
    let dx = select(p - pl, pr - p, abs(pr.z - p.z) < abs(p.z - pl.z));
    let dy = select(p - pu, pb - p, abs(pb.z - p.z) < abs(p.z - pu.z));
    let cxy = cross(dx, dy);
    let nlen2 = dot(cxy, cxy);
    if nlen2 <= 1e-10 {
        return normalize(-p);
    }
    var n = cxy * inverseSqrt(nlen2);
    if dot(n, -p) < 0.0 {
        n = -n;
    }
    return n;
}

fn pattern(p: vec2<i32>) -> f32 {
    var b = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let i = (p.y & 3) * 4 + (p.x & 3);
    return (b[i] + 0.5) / 16.0;
}

fn step_offset(p: vec2<i32>) -> f32 {
    let k = i32(pattern(p) * 16.0);
    return (f32((k * 7 + 5) & 15) + 0.5) / 16.0;
}

fn tap_emitter(pos_px: vec2<f32>, size: vec2<f32>) -> vec3<f32> {
    if any(pos_px < vec2(0.0)) || any(pos_px >= size) {
        return vec3(0.0);
    }
    let d = raw_depth_at(vec2<i32>(floor(pos_px)));
    if d <= 0.0 || is_first_person(d) {
        return vec3(0.0);
    }
    let samp = textureSampleLevel(screen, screen_sampler, pos_px / size, 0.0);
    if abs(samp.a - SCREEN_MARK) < 0.05 {
        return vec3(0.0);
    }
    let safe_l = select(vec3(0.0), max(samp.rgb, vec3(0.0)), samp.rgb == samp.rgb);
    let lum = dot(safe_l, vec3(0.2126, 0.7152, 0.0722));
    return safe_l / (1.0 + 0.30 * lum);
}

// 5-tap area-filtered radiance sample scaled to the ray step interval so thin neon strokes
// and bulbs integrate smoothly instead of point-aliasing.
fn sample_emitter_radiance(st: vec2<f32>, dir: vec2<f32>, step_px: f32, size: vec2<f32>) -> vec3<f32> {
    let center_s = textureSampleLevel(screen, screen_sampler, st / size, 0.0);
    if abs(center_s.a - SCREEN_MARK) < 0.05 {
        return vec3(0.0);
    }
    let c_safe = select(vec3(0.0), max(center_s.rgb, vec3(0.0)), center_s.rgb == center_s.rgb);
    let c_lum = dot(c_safe, vec3(0.2126, 0.7152, 0.0722));
    let c_comp = c_safe / (1.0 + 0.30 * c_lum);
    let spread = clamp(step_px * 0.55, 2.0, 16.0);
    let along = dir * spread;
    let perp = vec2(-dir.y, dir.x) * (spread * 0.70);
    return c_comp * 0.36
        + tap_emitter(st + along, size) * 0.16
        + tap_emitter(st - along, size) * 0.16
        + tap_emitter(st + perp, size) * 0.16
        + tap_emitter(st - perp, size) * 0.16;
}

// Marches one side (`side_sign` in {-1.0, +1.0}) of a slice and accumulates bounded
// per-step differential form-factor indirect diffuse radiance from lit & emissive surfaces.
fn march_slice_side(
    c: vec2<f32>,
    dir: vec2<f32>,
    side_sign: f32,
    p: vec3<f32>,
    n: vec3<f32>,
    v: vec3<f32>,
    sin_g: f32,
    on_screen: f32,
    offset: f32,
    size: vec2<f32>,
    radius: f32,
) -> vec3<f32> {
    let lowest = select(sin_g, -sin_g, side_sign > 0.0);
    var h_cos = lowest;
    var bounce = vec3(0.0);
    let r2 = radius * radius;
    let proj_scale = max(gi.proj.y * size.y, 1.0);

    for (var j = 0; j < SSGI_STEPS; j++) {
        let f0 = f32(j) / f32(SSGI_STEPS);
        let f1 = f32(j + 1) / f32(SSGI_STEPS);
        let g0 = 0.30 * f0 + 0.70 * f0 * f0;
        let g1 = 0.30 * f1 + 0.70 * f1 * f1;
        let r_start = 2.5 + g0 * (on_screen - 2.5);
        let r_end = 2.5 + g1 * (on_screen - 2.5);
        let step_px = max(r_end - r_start, 1.0);
        let f = (f32(j) + offset) / f32(SSGI_STEPS);
        let reach = 2.5 + (0.30 * f + 0.70 * f * f) * (on_screen - 2.5);
        let q = c + dir * (side_sign * reach);
        if any(q < vec2(0.0)) || any(q >= size) {
            continue;
        }
        let st = texel_centre(drawn_texel(q));
        let qpx = vec2<i32>(floor(st));
        let qd = raw_depth_at(qpx);
        if qd <= 0.0 || is_first_person(qd) {
            continue;
        }
        let q_pos = view_pos(st, qd);
        let delta = q_pos - p;
        let dd = dot(delta, delta);
        if dd < 16.0 || dd >= r2 {
            continue;
        }
        let inv_dist = inverseSqrt(dd);
        let wi = delta * inv_dist;
        let s_cos = dot(wi, v) - HORIZON_BIAS;
        let vis = smoothstep(h_cos - 0.38, h_cos + 0.02, s_cos);
        let recv_cos = max(dot(n, wi) - 0.03, 0.0);
        if recv_cos > 0.0 && vis > 1e-4 {
            let zq = max(-q_pos.z, gi.params.z);
            let step_world = step_px * (2.0 * zq / proj_scale);
            let d_theta = clamp(step_world * inv_dist, 0.025, 0.32);
            let df = recv_cos * d_theta * vis;
            if df > 1e-5 {
                let l_clamped = sample_emitter_radiance(st, dir, step_px, size);
                let max_c = max(l_clamped.r, max(l_clamped.g, l_clamped.b));
                if max_c > 1e-5 {
                    let nq = emitter_normal_at(st, q_pos);
                    let emit_facing = smoothstep(-0.15, 0.25, dot(nq, -wi));
                    let emissive_omni = smoothstep(0.65, 2.0, max_c);
                    let eff_facing = mix(emit_facing, max(emit_facing, 0.78), emissive_omni);
                    let range = 1.0 - smoothstep(0.30, 1.0, dd / r2);
                    bounce += l_clamped * (df * range * eff_facing);
                }
            }
        }
        h_cos = select(mix(h_cos, max(s_cos, lowest), 0.55), mix(h_cos, s_cos, 0.40), s_cos > h_cos);
    }
    return bounce;
}

@fragment
fn resolve_depth(in: FullscreenVertexOutput) -> @builtin(frag_depth) f32 {
    let p = clamp(vec2<i32>(floor(in.position.xy)), vec2(0), dims() - 1);
    let fallback = textureLoad(depth, p, 0);
    var best = 0.0;
    for (var s = 0; s < depth_samples(); s++) {
        let r = textureLoad(depth, p, s);
        if r > 0.0 && !is_first_person(r) && r > best {
            best = r;
        }
    }
    return select(fallback, best, best > 0.0);
}

@fragment
fn trace(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let raw_d = raw_depth_at(px);
    if raw_d <= 0.0 {
        return vec4(0.0, 0.0, 0.0, SKY);
    }
    if is_first_person(raw_d) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }
    let c = vec2<f32>(px) + 0.5;
    let centre = view_pos(texel_centre(drawn_texel(c)), raw_d);
    let dist = -centre.z;
    let fade_end = max(gi.params.w, gi.proj.w + 1.0);
    if dist >= fade_end {
        return vec4(0.0, 0.0, 0.0, min(dist, SKY));
    }
    let dist_fade = 1.0 - smoothstep(gi.proj.w, fade_end, dist);
    if dist_fade <= 0.0 {
        return vec4(0.0, 0.0, 0.0, min(dist, SKY));
    }
    let n = normal_at(px, raw_d);
    let v = normalize(-centre);
    let size = vec2<f32>(dims());
    let radius = gi.params.x;
    let max_px = max(gi.proj.z, 14.0);
    let on_screen = clamp(85.0 * gi.proj.y * 0.5 * size.y / dist, 14.0, max_px);
    let turn = pattern(half);
    let offset = step_offset(half);

    var bounce = vec3(0.0);
    var unblocked = 0.0;
    for (var k = 0; k < SSGI_SLICES; k++) {
        let phi = (f32(k) + turn) * PI / f32(SSGI_SLICES);
        let dir = vec2(cos(phi), sin(phi));
        let along = vec3(dir.x, -dir.y, 0.0);
        let tangent = normalize(along - v * dot(along, v));
        let nt = dot(n, tangent);
        let nv = dot(n, v);
        let np_len = length(vec2(nt, nv));
        let gamma = select(0.0, atan2(nt, nv), np_len > 1e-6);
        let sin_g = sin(gamma);
        let cos_g = cos(gamma);

        bounce += march_slice_side(c, dir, 1.0, centre, n, v, sin_g, on_screen, offset, size, radius);
        bounce += march_slice_side(c, dir, -1.0, centre, n, v, sin_g, on_screen, offset, size, radius);
        unblocked += np_len * (cos_g + gamma * sin_g);
    }

    let raw_irr = bounce / max(unblocked, 1e-4) * dist_fade;
    let safe_irr = select(vec3(0.0), clamp(raw_irr, vec3(0.0), vec3(8.0)), raw_irr == raw_irr);
    return vec4(safe_irr, min(dist, SKY));
}

fn depth_gradient(px: vec2<i32>, d: f32) -> vec2<f32> {
    let t = drawn_texel(vec2<f32>(px) + 0.5);
    let pitch = gi.render_scale.zw / gi.render_scale.xy;
    let l = raw_depth_at(vec2<i32>(floor(texel_centre(t - vec2(1.0, 0.0)))));
    let r = raw_depth_at(vec2<i32>(floor(texel_centre(t + vec2(1.0, 0.0)))));
    let u = raw_depth_at(vec2<i32>(floor(texel_centre(t - vec2(0.0, 1.0)))));
    let b = raw_depth_at(vec2<i32>(floor(texel_centre(t + vec2(0.0, 1.0)))));
    let gx = select(d - l, r - d, abs(r - d) < abs(d - l));
    let gy = select(d - u, b - d, abs(b - d) < abs(d - u));
    return vec2(gx, gy) / pitch;
}

fn karis_compress(c: vec3<f32>) -> vec3<f32> {
    let s = select(vec3(0.0), max(c, vec3(0.0)), c == c);
    let l = dot(s, vec3(0.2126, 0.7152, 0.0722));
    return s / (1.0 + 0.45 * l);
}

fn karis_expand(c: vec3<f32>) -> vec3<f32> {
    let s = select(vec3(0.0), max(c, vec3(0.0)), c == c);
    let l = dot(s, vec3(0.2126, 0.7152, 0.0722));
    return s / max(1.0 - 0.45 * l, 0.15);
}

// Stage 1: 4x4 reverse-Z plane-aware bilateral denoise with Karis outlier compression.
@fragment
fn denoise(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let d = raw_depth_at(px);
    if d <= 0.0 {
        return vec4(0.0, 0.0, 0.0, SKY);
    }
    if is_first_person(d) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }
    let near = gi.params.z;
    let z = near / d;
    let dist_blur = smoothstep(160.0, 650.0, z);
    let grad = depth_gradient(px, d);
    let size = vec2<i32>(textureDimensions(gi_tex));
    let first = (px >> vec2(1u)) - 2 + (px & vec2(1));
    var total = vec3(0.0);
    var weight = 0.0;
    var dist_total = vec3(0.0);
    var dist_weight = 0.0;
    var nearest = vec3(0.0);
    var nearest_gap = 3.4e38;
    var soft_total = vec3(0.0);
    var soft_count = 0.0;
    var min_lum = 1e6;
    var max_lum = 0.0;
    var valid_minmax = false;
    let own_tol = (FALLBACK_TOLERANCE + 0.25 * dist_blur) * z + 1.0 + 25.0 * dist_blur;
    let soft_tol = (SOFT_TOLERANCE + 0.35 * dist_blur) * z + 1.0 + 35.0 * dist_blur;

    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let q = clamp(first + vec2(x, y), vec2(0), size - 1);
            let t = textureLoad(gi_tex, q, 0);
            if t.a <= 0.0 || t.a >= SKY * 0.99 {
                continue;
            }
            let tc = karis_compress(t.rgb);
            let expected = d + dot(grad, vec2<f32>(q * 2 - px));
            let plane = near / max(expected, 1e-7);
            let gap = select(3.4e38, abs(t.a - plane), expected > 0.0);
            let plane_tol = (DENOISE_TOLERANCE + 0.14 * dist_blur) * plane + 1.0 + 18.0 * dist_blur;
            let w = max(1.0 - gap / plane_tol, 0.0);
            total += tc * w;
            weight += w;
            let own_gap = abs(t.a - z);
            let wd = max(1.0 - own_gap / own_tol, 0.0);
            dist_total += tc * wd;
            dist_weight += wd;
            if wd > 0.0 {
                let tl = dot(tc, vec3(0.2126, 0.7152, 0.0722));
                min_lum = min(min_lum, tl);
                max_lum = max(max_lum, tl);
                valid_minmax = true;
            }
            if own_gap < nearest_gap {
                nearest_gap = own_gap;
                nearest = tc;
            }
            if own_gap <= soft_tol {
                soft_total += tc;
                soft_count += 1.0;
            }
        }
    }
    let spread = select(0.0, max_lum - min_lum, valid_minmax);
    let var_pull = smoothstep(0.06, 0.22, spread);
    let planar = smoothstep(PLANE_WEIGHT_FEW, PLANE_WEIGHT_FEW + PLANE_WEIGHT_BLEND, weight) * (1.0 - 0.65 * var_pull) * (1.0 - 0.75 * dist_blur);
    let sum = mix(dist_total, total, planar);
    let sum_w = mix(dist_weight, weight, planar);
    var out_c = select(nearest, sum / sum_w, sum_w > 0.0);
    if soft_count > 0.0 {
        let sparse = 1.0 - smoothstep(SPARSE_FULL, SPARSE_NONE, weight);
        let pull = max(SOFT_PULL * sparse, max(0.50 * var_pull, 0.85 * dist_blur));
        if pull > 0.0 {
            out_c = mix(out_c, soft_total / soft_count, pull);
        }
    }
    let out_rgb = karis_expand(out_c);
    let safe_rgb = select(vec3(0.0), max(out_rgb, vec3(0.0)), out_rgb == out_rgb);
    return vec4(safe_rgb, min(z, SKY));
}

// Stage 2: 7x7 distance-adaptive Gaussian a-trous bilateral denoise (up to 50x50 px).
@fragment
fn denoise_wide(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let d = raw_depth_at(px);
    if d <= 0.0 {
        return vec4(0.0, 0.0, 0.0, SKY);
    }
    if is_first_person(d) {
        return vec4(0.0, 0.0, 0.0, 0.0);
    }
    let near = gi.params.z;
    let z = near / d;
    let dist_blur = smoothstep(160.0, 650.0, z);
    let stride = select(2, 3, z > 200.0) + select(0, 1, z > 500.0);
    let grad = depth_gradient(px, d);
    let size = vec2<i32>(textureDimensions(gi_tex));
    let center = textureLoad(gi_tex, clamp(half, vec2(0), size - 1), 0);
    var g_w = array<f32, 7>(1.0, 6.0, 15.0, 20.0, 15.0, 6.0, 1.0);
    let own_tol = (0.08 + 0.35 * dist_blur) * z + 3.0 + 45.0 * dist_blur;

    var plane_sum = vec3(0.0);
    var plane_w = 0.0;
    var dist_sum = vec3(0.0);
    var dist_w = 0.0;
    for (var dy = -3; dy <= 3; dy++) {
        let wy = g_w[dy + 3];
        for (var dx = -3; dx <= 3; dx++) {
            let spatial = wy * g_w[dx + 3];
            let q = clamp(half + vec2(dx, dy) * stride, vec2(0), size - 1);
            let t = textureLoad(gi_tex, q, 0);
            if t.a <= 0.0 || t.a >= SKY * 0.99 {
                continue;
            }
            let tc = karis_compress(t.rgb);
            let expected = d + dot(grad, vec2<f32>(q * 2 - px));
            let plane = near / max(expected, 1e-7);
            let gap = select(3.4e38, abs(t.a - plane), expected > 0.0);
            let plane_tol = (0.045 + 0.22 * dist_blur) * plane + 1.5 + 30.0 * dist_blur;
            let wp = max(1.0 - gap / plane_tol, 0.0) * spatial;
            plane_sum += tc * wp;
            plane_w += wp;

            let own_gap = abs(t.a - z);
            let wd = max(1.0 - own_gap / own_tol, 0.0) * spatial;
            dist_sum += tc * wd;
            dist_w += wd;
        }
    }
    let fallback = select(vec3(0.0), karis_compress(center.rgb), center.a > 0.0 && center.a < SKY * 0.99);
    let planar = smoothstep(400.0, 1600.0, plane_w) * (1.0 - 0.75 * dist_blur);
    let sum = mix(dist_sum, plane_sum, planar);
    let sum_w = mix(dist_w, plane_w, planar);
    let out_c = select(fallback, sum / sum_w, sum_w > 1e-4);
    let out_rgb = karis_expand(out_c);
    let safe_rgb = select(vec3(0.0), max(out_rgb, vec3(0.0)), out_rgb == out_rgb);
    return vec4(safe_rgb, min(z, SKY));
}

// Depth-aware 3x3 tent upsample and additive HDR composite onto world geometry.
@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let scr_size = vec2<i32>(textureDimensions(screen));
    let px = vec2<i32>(floor(in.position.xy));
    let src = textureLoad(screen, clamp(px, vec2(0), scr_size - 1), 0);
    let raw_d = raw_depth_at(px);
    if raw_d <= 0.0 || is_first_person(raw_d) || abs(src.a - SCREEN_MARK) < 0.05 {
        return src;
    }
    let dist = gi.params.z / raw_d;
    let fade_end = max(gi.params.w, gi.proj.w + 1.0);
    if dist >= fade_end {
        return src;
    }

    let gi_size = vec2<i32>(textureDimensions(gi_tex));
    let t = vec2<f32>(px) * 0.5;
    let center_q = vec2<i32>(round(t));
    let tolerance = 0.06 * dist + 1.5;
    var total = vec3(0.0);
    var weight = 0.0;
    var nearest = vec3(0.0);
    var nearest_gap = 3.4e38;
    for (var dy = -1; dy <= 1; dy++) {
        for (var dx = -1; dx <= 1; dx++) {
            let q = clamp(center_q + vec2(dx, dy), vec2(0), gi_size - 1);
            let texel = textureLoad(gi_tex, q, 0);
            if texel.a <= 0.0 || texel.a >= SKY * 0.99 {
                continue;
            }
            let off = abs(vec2<f32>(q) - t);
            let tent = max(1.5 - off.x, 0.0) * max(1.5 - off.y, 0.0);
            let gap = abs(texel.a - dist);
            if gap < nearest_gap {
                nearest_gap = gap;
                nearest = texel.rgb;
            }
            let surface = max(1.0 - gap / tolerance, 0.0);
            let w = tent * surface;
            total += texel.rgb * w;
            weight += w;
        }
    }
    if weight <= 0.001 && nearest_gap >= dist * 0.25 + 8.0 {
        return src;
    }
    let irradiance = select(nearest, total / weight, weight > 0.001);
    if gi.params.y < 0.0 {
        let dbg = irradiance * (-gi.params.y);
        return vec4(select(vec3(0.0), max(dbg, vec3(0.0)), dbg == dbg), src.a);
    }
    let safe_src = select(vec3(0.0), max(src.rgb, vec3(0.0)), src.rgb == src.rgb);
    let y_lum = dot(safe_src, vec3(0.2126, 0.7152, 0.0722));
    let y_s = y_lum / (y_lum + 0.006);
    let black_guard = smoothstep(0.0008, 0.006, y_lum);
    let refl_mag = 0.68 * y_s * black_guard;
    let chroma = clamp(safe_src / max(y_lum, 1e-4), vec3(0.45), vec3(1.6));
    let rho_est = refl_mag * mix(vec3(1.0), chroma, 0.30);
    let emissive_mask = 1.0 - smoothstep(1.2, 3.5, y_lum);
    let add = irradiance * rho_est * gi.params.y * emissive_mask;
    let safe_add = select(vec3(0.0), max(add, vec3(0.0)), add == add);
    return vec4(src.rgb + safe_add, src.a);
}
