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
const SSGI_SLICES: i32 = 3;
const SSGI_STEPS: i32 = 7;
const PI: f32 = 3.14159265;
const HALF_PI: f32 = 1.57079633;
const HORIZON_BIAS: f32 = 0.04;
const THIN_DECAY: f32 = 0.40;
const DENOISE_TOLERANCE: f32 = 0.03;
const FALLBACK_TOLERANCE: f32 = 0.05;
const PLANE_WEIGHT_FEW: f32 = 1.0;
const PLANE_WEIGHT_BLEND: f32 = 2.0;
const SPARSE_FULL: f32 = 3.0;
const SPARSE_NONE: f32 = 8.0;
const SOFT_TOLERANCE: f32 = 0.4;
const SOFT_PULL: f32 = 0.65;

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

// Fast 2-tap view-space emitter normal estimate around drawn texel at `st`.
fn emitter_normal_at(st: vec2<f32>, p: vec3<f32>) -> vec3<f32> {
    let td = drawn_texel(st);
    let cr = texel_centre(td + vec2(1.0, 0.0));
    let cb = texel_centre(td + vec2(0.0, 1.0));
    let dr = raw_depth_at(vec2<i32>(floor(cr)));
    let db = raw_depth_at(vec2<i32>(floor(cb)));
    if dr <= 0.0 || db <= 0.0 {
        return normalize(-p);
    }
    let pr = view_pos(cr, dr);
    let pb = view_pos(cb, db);
    let cxy = cross(pr - p, pb - p);
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

fn visible_arc(h: f32, gamma: f32, cos_g: f32, sin_g: f32) -> f32 {
    return 0.25 * (cos_g - cos(2.0 * h - gamma) + 2.0 * h * sin_g);
}

// Marches one side (`side_sign` in {-1.0, +1.0}) of a horizon slice and accumulates
// cosine-weighted 1-bounce indirect diffuse radiance from lit scene surfaces.
fn march_slice_side(
    c: vec2<f32>,
    dir: vec2<f32>,
    side_sign: f32,
    p: vec3<f32>,
    n: vec3<f32>,
    v: vec3<f32>,
    gamma: f32,
    cos_g: f32,
    sin_g: f32,
    np_len: f32,
    on_screen: f32,
    offset: f32,
    size: vec2<f32>,
    radius: f32,
) -> vec3<f32> {
    let h_edge = select(gamma - HALF_PI, gamma + HALF_PI, side_sign > 0.0);
    let lowest = select(sin_g, -sin_g, side_sign > 0.0);
    var h_cos = lowest;
    var h_ang = h_edge;
    var bounce = vec3(0.0);
    let r2 = radius * radius;

    for (var j = 0; j < SSGI_STEPS; j++) {
        let f = (f32(j) + offset) / f32(SSGI_STEPS);
        let reach = 4.0 + f * f * (on_screen - 4.0);
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
        let wi = delta * inverseSqrt(dd);
        let s_cos = dot(wi, v) - HORIZON_BIAS;
        if s_cos > h_cos {
            let ang = acos(clamp(s_cos, -1.0, 1.0));
            var h_new = h_ang;
            if side_sign > 0.0 {
                h_new = clamp(ang, 0.0, h_ang);
            } else {
                h_new = clamp(-ang, h_ang, 0.0);
            }
            if dot(n, wi) > 0.04 {
                let arc_old = visible_arc(h_ang, gamma, cos_g, sin_g);
                let arc_new = visible_arc(h_new, gamma, cos_g, sin_g);
                let df = max(arc_old - arc_new, 0.0);
                if df > 1e-5 {
                    let samp = textureSampleLevel(screen, screen_sampler, st / size, 0.0);
                    if abs(samp.a - SCREEN_MARK) >= 0.05 {
                        let nq = emitter_normal_at(st, q_pos);
                        let emit_facing = smoothstep(-0.15, 0.25, dot(nq, -wi));
                        let range = 1.0 - smoothstep(0.30, 1.0, dd / r2);
                        let safe_l = select(vec3(0.0), max(samp.rgb, vec3(0.0)), samp.rgb == samp.rgb);
                        let lum = dot(safe_l, vec3(0.2126, 0.7152, 0.0722));
                        let l_clamped = safe_l / (1.0 + 0.25 * lum);
                        bounce += l_clamped * (df * np_len * range * emit_facing);
                    }
                }
            }
            h_cos = s_cos;
            h_ang = h_new;
        } else {
            h_cos = mix(h_cos, max(s_cos, lowest), THIN_DECAY);
            let ang = acos(clamp(h_cos, -1.0, 1.0));
            if side_sign > 0.0 {
                h_ang = clamp(ang, 0.0, h_edge);
            } else {
                h_ang = clamp(-ang, h_edge, 0.0);
            }
        }
    }
    return bounce;
}

@fragment
fn resolve_depth(in: FullscreenVertexOutput) -> @builtin(frag_depth) f32 {
    return textureLoad(depth, vec2<i32>(floor(in.position.xy)), 0);
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
    let max_px = max(gi.proj.z, 12.0);
    let on_screen = clamp(radius * gi.proj.y * 0.5 * size.y / dist, 12.0, max_px);
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

        bounce += march_slice_side(c, dir, 1.0, centre, n, v, gamma, cos_g, sin_g, np_len, on_screen, offset, size, radius);
        bounce += march_slice_side(c, dir, -1.0, centre, n, v, gamma, cos_g, sin_g, np_len, on_screen, offset, size, radius);
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

// 4x4 reverse-Z plane-aware bilateral denoise of the half-res SSGI irradiance buffer.
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

    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let q = clamp(first + vec2(x, y), vec2(0), size - 1);
            let t = textureLoad(gi_tex, q, 0);
            if t.a <= 0.0 {
                continue;
            }
            let expected = d + dot(grad, vec2<f32>(q * 2 - px));
            let plane = near / max(expected, 1e-7);
            let gap = select(3.4e38, abs(t.a - plane), expected > 0.0);
            let w = max(1.0 - gap / (DENOISE_TOLERANCE * plane + 1.0), 0.0);
            total += t.rgb * w;
            weight += w;
            let own_gap = abs(t.a - z);
            let wd = max(1.0 - own_gap / (FALLBACK_TOLERANCE * z + 1.0), 0.0);
            dist_total += t.rgb * wd;
            dist_weight += wd;
            if own_gap < nearest_gap {
                nearest_gap = own_gap;
                nearest = t.rgb;
            }
            if own_gap <= SOFT_TOLERANCE * z + 1.0 {
                soft_total += t.rgb;
                soft_count += 1.0;
            }
        }
    }
    let planar = smoothstep(PLANE_WEIGHT_FEW, PLANE_WEIGHT_FEW + PLANE_WEIGHT_BLEND, weight);
    let sum = mix(dist_total, total, planar);
    let sum_w = mix(dist_weight, weight, planar);
    var out_rgb = select(nearest, sum / sum_w, sum_w > 0.0);
    if soft_count > 0.0 {
        let sparse = 1.0 - smoothstep(SPARSE_FULL, SPARSE_NONE, weight);
        if sparse > 0.0 {
            out_rgb = mix(out_rgb, soft_total / soft_count, SOFT_PULL * sparse);
        }
    }
    let safe_rgb = select(vec3(0.0), max(out_rgb, vec3(0.0)), out_rgb == out_rgb);
    return vec4(safe_rgb, min(z, SKY));
}

// Depth-aware 2x2 upsample and additive HDR composite onto world geometry.
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
    let base = floor(t);
    let f = t - base;
    let first = vec2<i32>(base);
    let tolerance = 0.05 * dist + 1.0;
    var total = vec3(0.0);
    var weight = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let q = clamp(first + vec2(x, y), vec2(0), gi_size - 1);
            let texel = textureLoad(gi_tex, q, 0);
            let bilinear = select(1.0 - f.x, f.x, x == 1) * select(1.0 - f.y, f.y, y == 1);
            let surface = select(0.0, max(1.0 - abs(texel.a - dist) / tolerance, 0.0), texel.a > 0.0);
            let w = bilinear * surface;
            total += texel.rgb * w;
            weight += w;
        }
    }
    if weight <= 0.001 {
        return src;
    }
    let irradiance = total / weight;
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
