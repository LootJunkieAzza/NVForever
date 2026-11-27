// Sun contact shadows of the screen (nvforever sun_contact_shadows.rs; the settings row Sun
// contact shadows): short-range shadows from what the depth prepass holds, ray-marched
// towards the sun. Entry `march` runs at half the render size: for each texel it walks from
// the surface towards the sun along the view space direction and compares the depth it
// finds with the ray's own, writing 1 where nothing stands in the way and below 1 in the
// shadow of a gap the shadow maps do not resolve (a blade of grass, a kerb, a crate's foot
// on the ground). Entry `denoise` blurs that over the 4 x 4 texels its pattern repeats over,
// so the per-texel jitter does not show as grain. The lighting reads the result through the
// sun visibility's slot (sun_visibility.wesl `nvf_screen_visibility`), where it scales the
// outdoor sun's direct light alone. An enhancement of our own, not the original's.
//
// The first-person arms and weapon are left out: their depth is squeezed into the nearest
// 1/64 of the range (nvf_assets first_person_depth.wesl), so neither their place in the
// world nor the world's behind them can be read from the depth. An engineering choice: the
// sun's own shadows on the arms come from the shade light (nif_material.rs).

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

struct ContactShadow {
    // The view's projection as drawn (the jitter included) and its inverse, so a depth
    // value and a screen place give the view space point, and the other way round.
    clip_from_view: mat4x4<f32>,
    view_from_clip: mat4x4<f32>,
    // The view's place in the world (for the temporal reprojection) and the previous
    // frame's world-to-clip matrix.
    world_from_view: mat4x4<f32>,
    prev_clip_from_world: mat4x4<f32>,
    // xyz: the sun's direction in view space (unit, towards the light); w: strength 0..1.
    light: vec4<f32>,
    // x: the ray's length (game units); y: the thickness assumed of what the depth holds;
    // z: the bias along the ray; w: steps.
    params: vec4<f32>,
    // x, y: the render size (the main pass's viewport, the scale a screen uv is taken at);
    // z, w: this pass's size (half of it, rounded up).
    size: vec4<f32>,
    // x, y: the depth texture's size in texels (the loads' clamp); z, w: the distances the
    // effect fades out between (the F11 window's Distance).
    extra: vec4<f32>,
    // x: the denoise's softness, 0 (a 3 x 3 tent) to 1 (a 7 x 7); y: the history's weight,
    // 0 none; z: 1 on the first frame (the history is not to be trusted); w: unused.
    softness: vec4<f32>,
    // xyz: the world's up in view space (which receivers are ground); w: distant range.
    up: vec4<f32>,
}

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> contact: ContactShadow;
// `denoise` only.
@group(0) @binding(2) var marched: texture_2d<f32>;
@group(0) @binding(3) var history: texture_2d<f32>;
@group(0) @binding(4) var history_sampler: sampler;

// The share of the depth range the first-person shapes are squeezed into (nvf_assets
// first_person_depth.wesl `FIRST_PERSON_DEPTH_SHARE`, a test keeps them equal).
const FIRST_PERSON_SHARE: f32 = 0.015625;

// RC6 uses a longer ray on terrain and grows its reach with camera distance. With only
// depth available, an upward receiver normal selects the ground profile. Keep the
// short section for thin grass and reserve the rest of the steps for longer blockers.
const FAR_FROM: f32 = 1300.0;
const FAR_TO: f32 = 3800.0;
const RAY_CAP: f32 = 800.0;
const GROUND_LEN: f32 = 3.0;
const GROUND_COS: f32 = 0.7;

fn first_person(d: f32) -> bool {
    return d > 1.0 - FIRST_PERSON_SHARE;
}

fn depth_texel(p: vec2<i32>) -> f32 {
    let last = vec2<i32>(contact.extra.xy) - vec2<i32>(1, 1);
    return textureLoad(depth, clamp(p, vec2<i32>(0, 0), max(last, vec2<i32>(0, 0))), 0);
}

// The depth value at a screen uv (0..1 of the render size).
fn depth_at(uv: vec2<f32>) -> f32 {
    return depth_texel(vec2<i32>(floor(uv * contact.size.xy)));
}

fn depth_uv(pixel: vec2<i32>) -> vec2<f32> {
    return (vec2<f32>(pixel) + 0.5) / max(contact.size.xy, vec2(1.0));
}

// The view space point a depth value `d` has at screen uv (reverse z: the clip space z is
// the depth).
fn view_point(uv: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec3<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d);
    let p = contact.view_from_clip * vec4<f32>(ndc, 1.0);
    return p.xyz / p.w;
}

// The screen uv of a view space point (0..1 of the render size; outside where the point is
// behind the eye).
fn view_uv(p: vec3<f32>) -> vec2<f32> {
    let c = contact.clip_from_view * vec4<f32>(p, 1.0);
    let ndc = c.xy / max(c.w, 1e-7);
    return vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
}

// The surface normal at uv from the neighbouring depths (the nearer of the two on each
// side, as ssao.wesl's), turned towards the eye; a zero vector where a neighbour is sky or
// a first-person shape, so such a pixel takes no facing test.
fn normal_at(uv: vec2<f32>, d: f32) -> vec3<f32> {
    let texel = 1.0 / max(contact.size.xy, vec2<f32>(1.0));
    let dl = depth_at(uv - vec2<f32>(texel.x, 0.0));
    let dr = depth_at(uv + vec2<f32>(texel.x, 0.0));
    let du = depth_at(uv - vec2<f32>(0.0, texel.y));
    let db = depth_at(uv + vec2<f32>(0.0, texel.y));
    if dl <= 0.0 || dr <= 0.0 || du <= 0.0 || db <= 0.0 || first_person(dl) || first_person(dr) || first_person(du) || first_person(db) {
        return vec3<f32>(0.0);
    }
    let p = view_point(uv, d);
    let l = view_point(uv - vec2<f32>(texel.x, 0.0), dl);
    let r = view_point(uv + vec2<f32>(texel.x, 0.0), dr);
    let u = view_point(uv - vec2<f32>(0.0, texel.y), du);
    let b = view_point(uv + vec2<f32>(0.0, texel.y), db);
    let dx = select(p - l, r - p, abs(r.z - p.z) < abs(p.z - l.z));
    let dy = select(p - u, b - p, abs(b.z - p.z) < abs(p.z - u.z));
    let n = cross(dx, dy);
    if dot(n, n) <= 1e-12 {
        return vec3<f32>(0.0);
    }
    var unit = normalize(n);
    if dot(unit, -p) < 0.0 {
        unit = -unit;
    }
    return unit;
}

fn blocker_gap(uv: vec2<f32>, ray_z: f32, p: vec3<f32>, n: vec3<f32>, bias: f32, thickness: f32) -> f32 {
    if any(uv <= vec2(0.0)) || any(uv >= vec2(1.0)) {
        return thickness;
    }
    let pixel = vec2<i32>(floor(uv * contact.size.xy));
    let depth = depth_texel(pixel);
    if depth <= 0.0 || first_person(depth) {
        return thickness;
    }
    let caster = view_point(depth_uv(pixel), depth);
    let gap = caster.z - ray_z;
    // Test the actual depth texel's position against the receiver plane.
    let above = select(gap > bias, dot(caster - p, n) > bias, dot(n, n) > 0.5);
    return select(thickness, gap, above && gap > 0.0);
}

// Thin foliage leaves gaps in a single sun ray and can shrink to one depth texel. Gather
// nearby blockers above the receiver plane to fill the contact at the base of a clump.
// The footprint sets a minimum screen radius, and the view-space bounds cap the reach.
fn ground_contact(pixel: vec2<i32>, p: vec3<f32>, n: vec3<f32>, light: vec3<f32>, footprint: f32, dist: f32) -> f32 {
    let far = smoothstep(650.0, 4600.0, dist);
    let reach = mix(25.0, 64.0, far);
    let screen_reach = clamp(reach / max(footprint, 0.2), 2.0, 20.0);
    let min_height = max(1.4, 0.48 * footprint);
    let light_on_plane = light - n * dot(n, light);
    var peak = 0.0;
    var coverage = 0.0;
    for (var i = 0u; i < 12u; i += 1u) {
        let angle = f32(i) * 2.3999632;
        let radius = screen_reach * sqrt((f32(i) + 0.5) / 12.0);
        let offset = vec2<i32>(round(vec2(cos(angle), sin(angle)) * radius));
        let tap = pixel + offset;
        if all(tap >= vec2(0)) && all(tap < vec2<i32>(contact.size.xy)) {
            let tap_depth = depth_texel(tap);
            if tap_depth > 0.0 && !first_person(tap_depth) {
                let delta = view_point(depth_uv(tap), tap_depth) - p;
                let height = dot(delta, n);
                let lateral = delta - n * height;
                let span = length(lateral);
                // Reject terrain slope and distant depth layers. Give nearby blockers
                // some weight on all sides, with more on the sunward side.
                let lift = max(min_height, 0.16 * span);
                let rises = smoothstep(lift, lift + max(1.4, 0.6 * footprint), height);
                let range = 1.0 - smoothstep(reach * 0.35, reach, span);
                let low = 1.0 - smoothstep(0.70 * reach, 1.45 * reach, height);
                let sunward = clamp(dot(lateral, light_on_plane) / max(span, 1e-3), 0.0, 1.0);
                let blocker = rises * range * low * mix(0.68, 1.0, sunward);
                peak = max(peak, blocker);
                coverage += blocker;
            }
        }
    }
    // One visible blade can contribute at distance. Dense clumps have a stronger core.
    return min(0.78, mix(0.35, 0.49, far) * peak + 0.17 * coverage);
}

// The distant profile traces larger silhouettes. Grow the depth window with the pixel's
// footprint, and require a blocker above the receiver plane to reject grazing terrain.
fn distant_shadow(uv: vec2<f32>, p: vec3<f32>, n: vec3<f32>, footprint: f32, dist: f32) -> f32 {
    let light = contact.light.xyz;
    let range = mix(1150.0, 2200.0, smoothstep(contact.extra.z, contact.up.w, dist));
    let near_plane = contact.clip_from_view[3].z;
    let ray_len = min(range, max(dist - near_plane * 1.05, 0.0) / max(light.z, 1e-4));
    let end = p + light * ray_len;
    let end_uv = view_uv(end);
    let thickness = clamp(20.0 + 1.6 * footprint, 28.0, 160.0);
    let bias = max(contact.params.z, 0.60 * footprint);
    for (var i = 0u; i < 12u; i += 1u) {
        let t = (f32(i) + 0.5) / 12.0;
        let at = mix(uv, end_uv, t);
        if any(at <= vec2(0.0)) || any(at >= vec2(1.0)) {
            break;
        }
        let z = -1.0 / mix(1.0 / dist, 1.0 / -end.z, t);
        let gap = blocker_gap(at, z, p, n, bias, thickness);
        if gap < thickness {
            let confidence = 1.0 - smoothstep(0.65 * thickness, thickness, gap);
            let travel = 1.0 - smoothstep(0.5, 1.0, t);
            let edge = min(min(at.x, 1.0 - at.x), min(at.y, 1.0 - at.y));
            return confidence * travel * smoothstep(0.0, 0.02, edge);
        }
    }
    return 0.0;
}

@fragment
fn march(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let pixel = vec2<i32>(floor(in.position.xy));
    let size = max(contact.size.zw, vec2<f32>(1.0));
    let depth_pixel = vec2<i32>((vec2<f32>(pixel) + 0.5) * contact.size.xy / size);
    let uv = depth_uv(depth_pixel);
    let d = depth_at(uv);
    // The sky and the arms: no receiver, and no sign of what stands between.
    if d <= 0.0 || first_person(d) {
        return vec4<f32>(1.0);
    }
    let p = view_point(uv, d);
    let dist = -p.z;
    if dist >= max(contact.extra.w, contact.up.w) {
        return vec4<f32>(1.0);
    }
    let l = contact.light.xyz;
    let n = normal_at(uv, d);
    let n_valid = dot(n, n) > 0.5;
    if n_valid && dot(n, l) <= 0.01 {
        return vec4<f32>(1.0);
    }
    let steps = u32(contact.params.w);
    if steps == 0u {
        return vec4<f32>(1.0);
    }
    let footprint = 2.0 * dist / max(contact.size.y * abs(contact.clip_from_view[1].y), 1.0);
    var distant = 0.0;
    if n_valid && contact.up.w > contact.extra.w && dist > contact.extra.z {
        let fade_in = smoothstep(contact.extra.z, contact.extra.w, dist);
        let fade_out = 1.0 - smoothstep(max(contact.extra.w, contact.up.w * 0.85), contact.up.w, dist);
        distant = 0.85 * distant_shadow(uv, p, n, footprint, dist) * fade_in * fade_out;
    }
    if dist >= contact.extra.w {
        return vec4(1.0 - distant * contact.light.w);
    }
    let ground = n_valid && dot(n, contact.up.xyz) > GROUND_COS;
    let mid_range = smoothstep(500.0, 1500.0, dist);
    let far = smoothstep(FAR_FROM, FAR_TO, dist);
    let short_ray = contact.params.x * select(1.0, GROUND_LEN, ground);
    let mid_ray = max(short_ray, min(short_ray * mix(1.0, 6.0, mid_range), 400.0));
    let far_ray = min(contact.params.x * select(28.0, 40.0, ground), RAY_CAP);
    let ray_limit = mix(mid_ray, far_ray, far);
    let near_plane = contact.clip_from_view[3].z;
    let ray_len = select(ray_limit, min(ray_limit, max(dist - near_plane * 1.05, 0.0) / max(l.z, 1e-4)), l.z > 1e-4);
    if ray_len <= 1e-3 {
        return vec4(1.0 - distant * contact.light.w);
    }
    let short_len = min(short_ray * mix(1.0, 2.0, mid_range), ray_len);
    let split = min(short_len / ray_len, 0.5);
    let plane_bias = contact.params.z + select(min(0.28 * footprint, 5.0), min(0.12 * footprint, 2.0), ground);
    let bias = select(max(plane_bias, footprint), plane_bias, n_valid);
    let close_thickness = contact.params.y * select(1.0, 2.0, ground);
    let distant_thickness = max(close_thickness, min(64.0, max(close_thickness * mix(1.0, 5.0, mid_range),
        close_thickness + 2.0 * footprint + 8.0 * far)));
    let end = p + l * ray_len;
    let uv_end = view_uv(end);
    let rcp_z = 1.0 / dist;
    let rcp_end_z = 1.0 / -end.z;
    let fade = 1.0 - smoothstep(contact.extra.z, contact.extra.w, dist);
    var contact_shadow = 0.0;
    if ground {
        contact_shadow = ground_contact(depth_pixel, p, n, l, footprint, dist);
    }
    var directional = 0.0;
    var prev = 0.0;
    for (var i = 0u; i < steps; i += 1u) {
        let u = (f32(i) + 0.5) / f32(steps);
        let fraction = select(mix(split, 1.0, 2.0 * u - 1.0), 2.0 * u * split, u < 0.5);
        let t = ray_len * fraction;
        let z = p.z + l.z * t;
        // Perspective-correct projection along the pixel-space line, using the ray's view depth.
        let proj = fraction * end.z / z;
        let at = mix(uv, uv_end, proj);
        if any(at <= vec2(0.0)) || any(at >= vec2(1.0)) {
            break;
        }
        let thick = mix(close_thickness, distant_thickness, smoothstep(0.5 * short_len, short_len, t));
        var gap = blocker_gap(at, z, p, n, bias, thick);
        if gap < thick {
            var lo = prev;
            var hi = proj;
            for (var k = 0u; k < 3u; k += 1u) {
                let mid = (lo + hi) * 0.5;
                let mid_z = -1.0 / mix(rcp_z, rcp_end_z, mid);
                let mid_t = ray_len * mid * mid_z / end.z;
                let mid_thick = mix(close_thickness, distant_thickness, smoothstep(0.5 * short_len, short_len, mid_t));
                let mid_gap = blocker_gap(mix(uv, uv_end, mid), mid_z, p, n, bias, mid_thick);
                if mid_gap < mid_thick {
                    hi = mid;
                    gap = mid_gap;
                } else {
                    lo = mid;
                }
            }
            // RC6 keeps a confirmed blocker strong until late in the ray. Fade only
            // uncertain depth hits and the end of the ray, rather than all positive gaps.
            let confidence = 1.0 - smoothstep(0.45 * thick, thick, gap);
            let travel_fade = 1.0 - smoothstep(0.60, 1.0, fraction);
            let edge = min(min(at.x, 1.0 - at.x), min(at.y, 1.0 - at.y));
            directional = confidence * travel_fade * smoothstep(0.0, 0.018, edge);
            break;
        }
        prev = proj;
    }
    let near = (directional + (1.0 - directional) * contact_shadow) * fade;
    return vec4(1.0 - (near + (1.0 - near) * distant) * contact.light.w);
}

struct DenoiseOut {
    @location(0) value: vec4<f32>,
    @location(1) history: vec4<f32>,
};

fn shadow_depth(pixel: vec2<i32>, scale: vec2<f32>) -> f32 {
    return depth_texel(vec2<i32>((vec2<f32>(pixel) + 0.5) * scale));
}

@fragment
fn denoise(in: FullscreenVertexOutput) -> DenoiseOut {
    let centre = vec2<i32>(floor(in.position.xy));
    let last = max(vec2<i32>(contact.size.zw) - vec2<i32>(1, 1), vec2<i32>(0, 0));
    // The pass may run at half the render size or at it (`SunContactTuning::full`): the
    // depth is at the render size either way.
    let scale = max(contact.size.xy / max(contact.size.zw, vec2<f32>(1.0)), vec2<f32>(1.0));
    // The inverse depth of this texel's surface (proportional to its distance): a neighbour
    // whose own differs is another surface, and its grain is not this one's to average. That
    // keeps a caster's edge sharp while the march's noise inside a surface smooths out.
    let d = shadow_depth(centre, scale);
    if d <= 0.0 || first_person(d) {
        return DenoiseOut(vec4(1.0), vec4(1.0, 0.0, 0.0, 1.0));
    }
    let uv = depth_uv(vec2<i32>((vec2<f32>(centre) + 0.5) * scale));
    let p = view_point(uv, d);
    let dist = -p.z;
    if dist >= max(contact.extra.w, contact.up.w) {
        return DenoiseOut(vec4(1.0), vec4(1.0, dist * 0.001, 0.0, 1.0));
    }
    let local_range = dist < contact.extra.w;
    let zc = 1.0 / d;
    let raw = textureLoad(marched, centre, 0).r;
    // World-space width of one shadow texel. A fixed pixel kernel covers an ever
    // larger piece of ground with distance, washing small grass shadows into light.
    let world_texel = 2.0 * dist * max(scale.x, scale.y) / max(contact.size.y * abs(contact.clip_from_view[1].y), 1.0);
    let detail = smoothstep(1.5, 8.0, world_texel);
    let softness = contact.softness.x * mix(1.0, 0.15, detail);
    var local_min = raw;
    var local_max = raw;
    // A 3 x 3 tent and a 5 x 5 tent, mixed by the F11 window's Softness: the crossing is
// refined along the ray, so the march's noise is low and the small tent keeps a blade's
// shadow crisp; at Full resolution that reads as too crisp, and the wide tent softens it
// [engineering choice, the owner's note of 2026-11-25].
    var w3 = array<f32, 3>(1.0, 2.0, 1.0);
    var w5 = array<f32, 5>(1.0, 2.0, 3.0, 2.0, 1.0);
    var small = 0.0;
    var small_total = 0.0;
    var wide = 0.0;
    var wide_total = 0.0;
    let kernel = select(1, 2, local_range);
    for (var y = -kernel; y <= kernel; y = y + 1) {
        for (var x = -kernel; x <= kernel; x = x + 1) {
            let p = clamp(centre + vec2<i32>(x, y), vec2<i32>(0, 0), last);
            let zs = 1.0 / max(shadow_depth(p, scale), 1e-5);
            let same = saturate(1.0 - abs(zc - zs) / max(0.02 * zc, 1e-6));
            let value = textureLoad(marched, p, 0).r;
            let weight = w5[x + 2] * w5[y + 2];
            wide += weight * same * value;
            wide_total += weight * same;
            if (abs(x) <= 1 && abs(y) <= 1) {
                let inner = w3[x + 1] * w3[y + 1];
                small += inner * same * value;
                small_total += inner * same;
                if same > 0.5 {
                    local_min = min(local_min, value);
                    local_max = max(local_max, value);
                }
            }
        }
    }
    let crisp = select(raw, small / max(small_total, 1e-6), small_total > 1e-6);
    let soft = select(crisp, wide / wide_total, wide_total > 1e-6);
    // Over the half the Softness goes on to a 7 x 7 tent (radius 3 texels): Full resolution
    // wanted still more blur than the 5 x 5 gave (the owner's note of 2026-11-25). Its 49
    // taps are only read past the half, so the crisp settings cost what they did.
    var base = mix(crisp, soft, saturate(softness * 2.0));
    if local_range && softness > 0.5 {
        var w7 = array<f32, 7>(1.0, 2.0, 3.0, 4.0, 3.0, 2.0, 1.0);
        var widest = 0.0;
        var widest_total = 0.0;
        for (var y = -3; y <= 3; y = y + 1) {
            for (var x = -3; x <= 3; x = x + 1) {
                let p = clamp(centre + vec2<i32>(x, y), vec2<i32>(0, 0), last);
                let zs = 1.0 / max(shadow_depth(p, scale), 1e-5);
                let same = saturate(1.0 - abs(zc - zs) / max(0.02 * zc, 1e-6));
                let weight = w7[x + 3] * w7[y + 3];
                widest += weight * same * textureLoad(marched, p, 0).r;
                widest_total += weight * same;
            }
        }
        let widest_value = select(soft, widest / widest_total, widest_total > 1e-6);
        base = mix(base, widest_value, saturate((softness - 0.5) * 2.0));
    }
    // A four-tap blur turned by each texel's own angle (ScreenSpaceShaders'
    // `ssfx_sss_blur.ps`): the taps never line up from one texel to the next, so its cost
    // buys softness without the grid a square kernel leaves.
    let hx = u32(centre.x) * 374761393u;
    let hy = u32(centre.y) * 668265263u;
    let mixed = hx ^ hy;
    let bits = mixed & 1023u;
    let angle = f32(bits) * 0.006135923;
    let radius = mix(0.75, 3.0, saturate(softness));
    var turn = 0.0;
    var turn_total = 0.0;
    for (var i = 0; i < select(0, 4, local_range); i++) {
        let a = angle + f32(i) * 1.5707963;
        let delta = vec2<i32>(round(vec2(cos(a), sin(a)) * radius));
        let p = clamp(centre + delta, vec2<i32>(0, 0), last);
        let zs = 1.0 / max(shadow_depth(p, scale), 1e-5);
        let same = saturate(1.0 - abs(zc - zs) / max(0.02 * zc, 1e-6));
        turn += same * textureLoad(marched, p, 0).r;
        turn_total += same;
    }
    let filtered = mix(base, turn / max(turn_total, 1e-6), 0.3 * saturate(turn_total));
    // Keep some near-texel contrast when softening one-pixel grass shadows.
    let blurred = select(mix(raw, crisp, 0.65), mix(crisp, filtered, 0.65), local_range);
    // Preserve the measured hit rather than expanding the darkest neighbor: no
    // artificial halo or new shadow on a texel where the march found none.
    let value = mix(blurred, raw, 0.85 * detail);
    // Temporal accumulation (ScreenSpaceShaders' `screenspace_docc.h`): carry this texel's
    // world point into last frame's screen with last frame's world-to-clip matrix, reject
    // the history where the depth found there is another surface, and blend.
    var result = value;
    if contact.softness.y > 0.0 && contact.softness.z < 0.5 {
        let world = (contact.world_from_view * vec4<f32>(p, 1.0)).xyz;
        let clip = contact.prev_clip_from_world * vec4<f32>(world, 1.0);
        if (clip.w > 1e-6) {
            let ndc = clip.xy / clip.w;
            let prev_uv = vec2<f32>(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
            if (prev_uv.x > 0.0 && prev_uv.x < 1.0 && prev_uv.y > 0.0 && prev_uv.y < 1.0) {
                let h = textureSampleLevel(history, history_sampler, prev_uv, 0.0);
                let hr = select(1.0, h.r, h.r == h.r);
                let hd = h.g * 1000.0;
                let same = select(0.0, saturate(1.0 - abs(hd - clip.w) / max(0.02 * clip.w, 2.0)), hd > 0.0);
                let bounded = clamp(hr, min(value, local_min) - 0.05, max(value, local_max) + 0.05);
                // Wind and disocclusion can put a fresh narrow shadow on previously
                // lit ground. Do not let a valid ground depth retain that stale light.
                let agreement = 1.0 - smoothstep(0.03, 0.20, abs(bounded - value));
                let history_weight = saturate(contact.softness.y) * same * mix(1.0, agreement, detail);
                result = clamp(mix(value, bounded, history_weight), 0.0, 1.0);
            }
        }
    }
    return DenoiseOut(vec4<f32>(result), vec4<f32>(result, dist * 0.001, 0.0, 1.0));
}

// The clear (a texture no pipeline could write this frame) is sun_contact_clear.wesl, a
// shader of its own, so a march shader that fails to compile cannot blacken the sun.
