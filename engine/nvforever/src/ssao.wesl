// Screen-space ambient occlusion from the depth prepass (ssao.rs; enhanced graphics,
// docs/design/graphics-enhanced.md §3.1). Entry `occlusion` runs at half resolution: for
// each 2 x 2 block of the picture it works out how open the block's top-left pixel is
// (1 open, 0 closed) and writes that with the pixel's distance into a texture of its own;
// entry `apply` denoises that over the 4 x 4 texels its sample pattern repeats over, keeping
// to texels on this pixel's surface (so nothing bleeds across a depth edge), and
// multiplies the picture by it. Under FSR the passes run at the render size before FSR
// instead, and entry `multiply` gives the factor to a blend (ssao.rs `ao_before_fsr`). An
// engineering choice, not the original game's.
//
// Two estimates of the openness [engineering choice, #2897]: ground-truth ambient occlusion
// (`gtao_visibility`, the default) and the older count of buried samples on a spiral
// (`old_openness`, kept as the fallback: `NVF_PLAY_DEBUG_AO=old`, ssao.rs).

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import nvf_assets::first_person_depth::unsqueeze;

struct Ao {
    // x: radius (game units), y: strength, z: the projection's near plane, w: the
    // distance (game units) beyond which the effect has faded out.
    params: vec4<f32>,
    // x, y: the projection's x and y scale (clip_from_view[0][0], [1][1]); z: the
    // largest radius on the screen (pixels); w: the distance where the fade starts.
    proj: vec4<f32>,
    // x, y: the size the depth prepass was drawn at; z, w: the picture's size. They differ
    // under FSR's upscaling (fsr.rs `prepare`), whose depth is expanded to the picture's
    // size by nearest texels (fsr_depth.wgsl `expand_depth`); all 1 otherwise.
    render_scale: vec4<f32>,
    // x, y: the size of the picture the passes cover, from the top left of the depth
    // texture: FSR's render size while the occlusion is applied before FSR (entry
    // `multiply`, ssao.rs `ao_before_fsr`); 0 for the depth texture's whole size.
    area: vec4<f32>,
    // x: 1 for the old estimate, 0 for ground-truth ambient occlusion; y: 1 to show the
    // factor alone as a grey picture (debug switch NVF_PLAY_DEBUG_AO, ssao.rs `AoDebug`);
    // z, w unused.
    mode: vec4<f32>,
}

@if(MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> ao: Ao;
// `apply` only.
@group(0) @binding(2) var screen: texture_2d<f32>;
@group(0) @binding(3) var screen_sampler: sampler;
// Half resolution: r how open, g the distance (game units).
@group(0) @binding(4) var openness: texture_2d<f32>;

const SAMPLES: i32 = 12;
// Turns of the sample spiral.
const TURNS: f32 = 3.0;
// The distance written for depth 0, and the most any texel holds: half floats end at
// 65,504, and the sky dome lies in the depth prepass at 175,000 units or more. Far beyond
// the fade, so nothing compares against it but other far texels.
const SKY: f32 = 60000.0;

// What `occlusion` writes: how open, and the distance held to SKY.
fn texel(open: f32, dist: f32) -> vec4<f32> {
    return vec4(open, min(dist, SKY), 0.0, 0.0);
}
// Below this cosine between the normal and a sample a surface does not occlude itself.
const BIAS: f32 = 0.12;
const TAU: f32 = 6.2831853;
const PI: f32 = 3.14159265;
const HALF_PI: f32 = 1.57079633;

// Ground-truth ambient occlusion, the constants of the shader side (the strength and the
// radius are ssao.rs's; ssao.rs `GTAO_SLICES`/`GTAO_STEPS` mirror the first two, a test
// keeps them the same) [engineering choice: to be traded against the cost once measured].
// Slice directions per pixel (each a line through the pixel, both sides marched).
const SLICES: i32 = 2;
// Depth samples on each side of a slice.
const STEPS: i32 = 4;
// How far a sample's cosine to the view vector is pulled down before it may raise a
// horizon, so a sample on the pixel's own plane (depth rounding, a normal from the
// neighbouring depths) does not occlude it.
const HORIZON_BIAS: f32 = 0.03;
// An occluder counts fully within this share of the squared radius (half the radius) and
// not at all at the radius: the falloff by distance that keeps a thin thing far in front
// of a surface (a post before a wall) from hiding the surface's whole sky.
const FALLOFF_START: f32 = 0.25;
// The denoise keeps texels within this share of the distance (plus one unit) of the
// distance this pixel's plane has at them.
const DENOISE_TOLERANCE: f32 = 0.03;
// Where the plane-aware weights of the 16 texels add up to less than PLANE_WEIGHT_FEW
// (depth with no plane: alpha-tested plants, grass, a bush's leaves) the denoise weighs
// the texels by their gap to this pixel's own distance alone, within FALLBACK_TOLERANCE of
// it (plus one unit: the old blur's weight); above PLANE_WEIGHT_FEW + PLANE_WEIGHT_BLEND
// by the plane alone; smoothly between, so no edge is drawn where it changes. Without it a
// bush kept its own texel and one or two others, and the 4 x 4 turns showed as a grain
// [observed, 2026-11-18, main session's runs: the occlusion-alone pictures at Goodsprings]
// [engineering choice: the values, to be confirmed in pictures].
const PLANE_WEIGHT_FEW: f32 = 1.0;
const PLANE_WEIGHT_BLEND: f32 = 2.0;
const FALLBACK_TOLERANCE: f32 = 0.05;

fn dims() -> vec2<i32> {
    if ao.area.x > 0.0 {
        return vec2<i32>(ao.area.xy);
    }
    return vec2<i32>(textureDimensions(depth));
}

// The depth prepass's value at a pixel (reverse z: 0 is the far plane, the sky), the
// first-person arms and weapon at the depth they were squeezed from (nvf_assets
// nif_material.rs "First-person depth").
fn depth_at(p: vec2<i32>) -> f32 {
    return unsqueeze(textureLoad(depth, clamp(p, vec2(0), dims() - 1), 0));
}

// The view-space position of pixel `p` at depth value `d` (reverse infinite z:
// distance = near / d), looking down −z.
fn view_pos(p: vec2<f32>, d: f32) -> vec3<f32> {
    let size = vec2<f32>(dims());
    let ndc = vec2(p.x / size.x * 2.0 - 1.0, 1.0 - p.y / size.y * 2.0);
    let z = ao.params.z / max(d, 1e-7);
    return vec3(ndc.x * z / ao.proj.x, ndc.y * z / ao.proj.y, -z);
}

// The texel of the depth as drawn that the pixel centred at `p` shows, as fsr_depth.wgsl
// `expand_depth` picks it; without upscaling, the pixel itself.
fn drawn_texel(p: vec2<f32>) -> vec2<f32> {
    return floor(p * ao.render_scale.xy / ao.render_scale.zw);
}

// The centre of drawn texel `t`, in pixels of the picture.
fn texel_centre(t: vec2<f32>) -> vec2<f32> {
    return (t + 0.5) * ao.render_scale.zw / ao.render_scale.xy;
}

// The depth value at drawn texel `t`, read at a pixel that shows it.
fn texel_depth(t: vec2<f32>) -> f32 {
    return depth_at(vec2<i32>(floor(texel_centre(t))));
}

// The view-space position at the centre of drawn texel `t`, its depth read at a pixel
// that shows it.
fn texel_pos(t: vec2<f32>) -> vec3<f32> {
    let c = texel_centre(t);
    return view_pos(c, depth_at(vec2<i32>(floor(c))));
}

// The view-space normal at pixel `px`, whose depth is `d`: from the neighbouring texels of
// the depth as drawn, on each axis the side with the smaller step in depth, so an edge
// does not bend it. Without upscaling a texel is a pixel. Under FSR's upscaling a texel
// covers 1.5 to 3 pixels, so neighbouring pixels often held this pixel's own depth: the
// normal faced the camera and every surface seen at an angle occluded itself, a grey veil
// over the ground and walls [observed 2026-11-06, FSR Performance at Goodsprings: mean
// darkening 4.2%, against 0.7% without FSR].
fn normal_at(px: vec2<i32>, d: f32) -> vec3<f32> {
    let t = drawn_texel(vec2<f32>(px) + 0.5);
    let p = view_pos(texel_centre(t), d);
    let l = texel_pos(t - vec2(1.0, 0.0));
    let r = texel_pos(t + vec2(1.0, 0.0));
    let u = texel_pos(t - vec2(0.0, 1.0));
    let b = texel_pos(t + vec2(0.0, 1.0));
    let dx = select(p - l, r - p, abs(r.z - p.z) < abs(p.z - l.z));
    let dy = select(p - u, b - p, abs(b.z - p.z) < abs(p.z - u.z));
    var n = normalize(cross(dx, dy));
    if dot(n, -p) < 0.0 {
        n = -n;
    }
    return n;
}

// The 4 x 4 ordered pattern that turns each pixel's spiral (0..1).
fn pattern(p: vec2<i32>) -> f32 {
    var b = array<f32, 16>(0.0, 8.0, 2.0, 10.0, 12.0, 4.0, 14.0, 6.0, 3.0, 11.0, 1.0, 9.0, 15.0, 7.0, 13.0, 5.0);
    let i = (p.y & 3) * 4 + (p.x & 3);
    return (b[i] + 0.5) / 16.0;
}

// The offset of the first step along each side of a slice (0..1), from the same 4 x 4
// cell as `pattern` but its sixteen values in another order (i -> 7i + 5 mod 16, a
// permutation), so within the denoise's 4 x 4 texels every turn and every offset occurs
// once and the two do not run together.
fn step_offset(p: vec2<i32>) -> f32 {
    let k = i32(pattern(p) * 16.0);
    return (f32((k * 7 + 5) & 15) + 0.5) / 16.0;
}

@fragment
fn occlusion(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let d = depth_at(px);
    if d <= 0.0 {
        return texel(1.0, SKY);
    }
    let c = vec2<f32>(px) + 0.5;
    let p = view_pos(c, d);
    let dist = -p.z;
    let fade = 1.0 - smoothstep(ao.proj.w, ao.params.w, dist);
    if fade <= 0.0 {
        return texel(1.0, dist);
    }
    let n = normal_at(px, d);
    let radius = ao.params.x;
    let size = vec2<f32>(dims());
    let on_screen = min(radius * ao.proj.y * 0.5 * size.y / dist, ao.proj.z);
    if on_screen < 1.0 {
        return texel(1.0, dist);
    }
    var open = 1.0;
    if ao.mode.x > 0.5 {
        open = old_openness(half, c, p, n, on_screen);
    } else {
        // The centre at the centre of the drawn texel the pixel shows (as `normal_at` and
        // `horizon_cosine` take positions): under an upscaler's expanded depth
        // (NVF_AO_BEFORE_FSR=0) a display pixel's centre with its texel's depth lies off the
        // surface and occluded itself; without upscaling it is the pixel's centre.
        let centre = view_pos(texel_centre(drawn_texel(c)), d);
        open = 1.0 - ao.params.y * (1.0 - gtao_visibility(half, c, centre, n, on_screen));
    }
    // Never NaN nor outside 0..1 (comparisons with NaN are false).
    let safe = select(1.0, clamp(open, 0.0, 1.0), open >= 0.0);
    return texel(mix(1.0, safe, fade), dist);
}

// The old estimate: 12 samples on a 3-turn spiral, each occluding by its cosine to the
// normal above BIAS, fading out between half and the whole radius; the strength applied.
fn old_openness(half: vec2<i32>, c: vec2<f32>, p: vec3<f32>, n: vec3<f32>, on_screen: f32) -> f32 {
    let radius = ao.params.x;
    // Turned by the half-resolution texel, so the 4 x 4 blur in `apply` sees every turn.
    let turn = pattern(half) * TAU;
    var sum = 0.0;
    for (var i = 0; i < SAMPLES; i++) {
        let a = (f32(i) + 0.5) / f32(SAMPLES);
        let angle = a * TURNS * TAU + turn;
        let q = c + vec2(cos(angle), sin(angle)) * a * on_screen;
        let qd = depth_at(vec2<i32>(floor(q)));
        if qd <= 0.0 {
            continue;
        }
        let v = view_pos(floor(q) + 0.5, qd) - p;
        let vv = dot(v, v);
        let cosine = dot(v, n) * inverseSqrt(max(vv, 1e-4));
        // Nothing from beyond the radius, full weight within half of it.
        let range = 1.0 - smoothstep(0.25, 1.0, vv / (radius * radius));
        sum += max(cosine - BIAS, 0.0) * range;
    }
    return 1.0 - ao.params.y * sum / f32(SAMPLES) * 2.0;
}

// Ground-truth ambient occlusion [documented: Jimenez, Wu, Pesce, Jarabo, "Practical
// Real-Time Strategies for Accurate Indirect Occlusion", SIGGRAPH 2016 course, the
// horizon-based visibility integral; written from the paper's maths, no code copied]: the
// cosine-weighted share of the hemisphere above pixel `c` (view-space position `p`,
// normal `n`) that the depth leaves open, 1 open, 0 closed. Per slice (a direction on
// the screen through the pixel and the view vector) the depth is marched outwards on both
// sides for the highest horizon, and the visible arc between the two horizons integrated
// analytically against the normal projected into the slice.
//
// [engineering choice, own maths] The slices' sum is divided by the same slices' integral
// with nothing in the way, not by their count: with a few slices the plain mean is 1 for
// open ground only on average over the directions (for a surface tilted by a from the
// view a slice gives between cos a and cos a + a sin a), which the pattern would show as
// a grain on open ground. The ratio is exactly 1 there with any slice count and tends to
// the same integral as the slices increase.
fn gtao_visibility(half: vec2<i32>, c: vec2<f32>, p: vec3<f32>, n: vec3<f32>, on_screen: f32) -> f32 {
    let size = vec2<f32>(dims());
    let radius = ao.params.x;
    let v = normalize(-p);
    let turn = pattern(half);
    let offset = step_offset(half);
    var visible = 0.0;
    var unblocked = 0.0;
    for (var k = 0; k < SLICES; k++) {
        let phi = (f32(k) + turn) * PI / f32(SLICES);
        // The slice's direction on the screen (pixels, y down) and in view space (y up).
        let dir = vec2(cos(phi), sin(phi));
        let along = vec3(dir.x, -dir.y, 0.0);
        // The slice's tangent: in the slice, perpendicular to the view vector, towards `dir`.
        let tangent = normalize(along - v * dot(along, v));
        // The normal projected into the slice: its length and its angle from the view
        // vector, positive towards `dir`.
        let nt = dot(n, tangent);
        let nv = dot(n, v);
        let np_len = length(vec2(nt, nv));
        let gamma = select(0.0, atan2(nt, nv), np_len > 1e-6);
        let sin_g = sin(gamma);
        let cos_g = cos(gamma);
        // The highest horizon on each side as its cosine from the view vector, starting at
        // the edge of the normal's hemisphere (angle gamma + pi/2 towards `dir`, gamma - pi/2
        // away from it).
        var c_pos = -sin_g;
        var c_neg = sin_g;
        for (var j = 0; j < STEPS; j++) {
            let f = (f32(j) + offset) / f32(STEPS);
            // From a pixel away out to the radius on the screen, closer together near the
            // pixel, where the contact shadows are [engineering choice].
            let reach = 1.0 + f * f * (on_screen - 1.0);
            c_pos = max(c_pos, horizon_cosine(c + dir * reach, p, v, -sin_g, size, radius));
            c_neg = max(c_neg, horizon_cosine(c - dir * reach, p, v, sin_g, size, radius));
        }
        let h_neg = max(-acos(clamp(c_neg, -1.0, 1.0)), gamma - HALF_PI);
        let h_pos = min(acos(clamp(c_pos, -1.0, 1.0)), gamma + HALF_PI);
        visible += np_len * (visible_arc(h_neg, gamma, cos_g, sin_g) + visible_arc(h_pos, gamma, cos_g, sin_g));
        // The same with nothing in the way (h = gamma -/+ pi/2): cos g + g sin g.
        unblocked += np_len * (cos_g + gamma * sin_g);
    }
    if unblocked <= 1e-5 {
        return 1.0;
    }
    return clamp(visible / unblocked, 0.0, 1.0);
}

// The integral of cos(t - gamma) |sin t| from the view vector (t = 0) to the horizon at
// angle `h` in the slice [documented: the paper's closed form,
// 1/4 (-cos(2h - gamma) + cos(gamma) + 2 h sin(gamma)), the same for either side].
fn visible_arc(h: f32, gamma: f32, cos_g: f32, sin_g: f32) -> f32 {
    return 0.25 * (cos_g - cos(2.0 * h - gamma) + 2.0 * h * sin_g);
}

// The cosine from the view vector `v` at `p` to the depth at screen position `q`, for
// raising a horizon: `lowest` (the hemisphere's edge on that side) when `q` is off the
// screen or the sky, and blended towards `lowest` as the sample's distance from `p` nears
// the radius (FALLOFF_START).
fn horizon_cosine(q: vec2<f32>, p: vec3<f32>, v: vec3<f32>, lowest: f32, size: vec2<f32>, radius: f32) -> f32 {
    if any(q < vec2(0.0)) || any(q >= size) {
        return lowest;
    }
    // The position at the centre of the drawn texel `q` shows (without upscaling, the
    // pixel's centre), as `texel_pos` takes it.
    let st = texel_centre(drawn_texel(q));
    let qd = depth_at(vec2<i32>(floor(st)));
    if qd <= 0.0 {
        return lowest;
    }
    let delta = view_pos(st, qd) - p;
    let dd = dot(delta, delta);
    // The pixel's own drawn texel again (an upscaled depth, a step shorter than a texel):
    // no horizon from it.
    if dd < 1e-8 {
        return lowest;
    }
    let cosine = dot(delta, v) * inverseSqrt(max(dd, 1e-4)) - HORIZON_BIAS;
    let weight = 1.0 - smoothstep(FALLOFF_START, 1.0, dd / (radius * radius));
    return mix(lowest, cosine, weight);
}

@fragment
fn apply(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(screen, screen_sampler, in.uv);
    let px = vec2<i32>(floor(in.position.xy));
    let d = depth_at(px);
    let shown = ao.mode.y > 0.5;
    if d <= 0.0 {
        return select(src, vec4(1.0, 1.0, 1.0, src.a), shown);
    }
    let open = openness_at(px, d);
    // The factor alone as grey (NVF_PLAY_DEBUG_AO=view, view-old).
    return select(vec4(src.rgb * open, src.a), vec4(open, open, open, src.a), shown);
}

// Before FSR: the factor the picture is multiplied by, through the pipeline's blend (the
// render-size rectangle of the HDR picture and of FSR's opaque copy, ssao.rs `ao_before_fsr`).
// With the grey view the pipeline's blend replaces the colour instead (ssao.rs).
@fragment
fn multiply(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = vec2<i32>(floor(in.position.xy));
    let d = depth_at(px);
    if d <= 0.0 {
        return vec4(1.0);
    }
    // Never a value outside 0..1 nor NaN (comparisons with NaN are false): FSR's exposure
    // averages the whole HDR picture, so one NaN pixel would blacken the frame.
    let open = openness_at(px, d);
    let safe = select(1.0, min(open, 1.0), open >= 0.0);
    return vec4(safe, safe, safe, 1.0);
}

// How open pixel `px` (depth value `d`, not the sky) is, from the half-resolution texels.
fn openness_at(px: vec2<i32>, d: f32) -> f32 {
    if ao.mode.x > 0.5 {
        return old_openness_at(px, d);
    }
    let near = ao.params.z;
    let z = near / d;
    let grad = depth_gradient(px, d);
    let size = vec2<i32>(textureDimensions(openness));
    // The 4 x 4 texels around this pixel: texel q stands for full pixel 2q. All of equal
    // weight but for their surface, so the sixteen turns of the pattern cancel.
    let first = (px >> vec2(1u)) - 2 + (px & vec2(1));
    var total = 0.0;
    var weight = 0.0;
    // The same texels weighed by the gap to this pixel's own distance alone (the fallback
    // where few lie on its plane, PLANE_WEIGHT_FEW).
    var dist_total = 0.0;
    var dist_weight = 0.0;
    // The texel nearest this pixel's distance, for a pixel no texel shares a surface with
    // (a pole or a grass blade a pixel wide, a silhouette pixel in an odd row or column).
    var nearest = 1.0;
    var nearest_gap = 3.4e38;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let q = clamp(first + vec2(x, y), vec2(0), size - 1);
            let t = textureLoad(openness, q, 0);
            // The depth value this pixel's plane has at the texel's pixel: under the reverse
            // infinite projection a plane's depth values are affine in the screen position
            // (depth = near / distance), so the plane is followed exactly, a slope seen at a
            // grazing angle too, and a texel off it (across a depth edge) is left out.
            let expected = d + dot(grad, vec2<f32>(q * 2 - px));
            let plane = near / max(expected, 1e-7);
            let gap = select(3.4e38, abs(t.g - plane), expected > 0.0);
            let w = max(1.0 - gap / (DENOISE_TOLERANCE * plane + 1.0), 0.0);
            total += t.r * w;
            weight += w;
            let own_gap = abs(t.g - z);
            let wd = max(1.0 - own_gap / (FALLBACK_TOLERANCE * z + 1.0), 0.0);
            dist_total += t.r * wd;
            dist_weight += wd;
            if own_gap < nearest_gap {
                nearest_gap = own_gap;
                nearest = t.r;
            }
        }
    }
    // The plane's weights where enough texels lie on it, the distance's where few do.
    let planar = smoothstep(PLANE_WEIGHT_FEW, PLANE_WEIGHT_FEW + PLANE_WEIGHT_BLEND, weight);
    let sum = mix(dist_total, total, planar);
    let sum_weight = mix(dist_weight, weight, planar);
    return select(nearest, sum / sum_weight, sum_weight > 0.0);
}

// How the depth value changes per pixel at pixel `px` (depth value `d`): on each axis from
// the neighbouring texel of the depth as drawn on the side with the smaller step, as
// `normal_at` takes it, so a depth edge beside the pixel does not tilt its plane.
fn depth_gradient(px: vec2<i32>, d: f32) -> vec2<f32> {
    let t = drawn_texel(vec2<f32>(px) + 0.5);
    // Pixels from one drawn texel's centre to the next (1 without upscaling).
    let pitch = ao.render_scale.zw / ao.render_scale.xy;
    let l = texel_depth(t - vec2(1.0, 0.0));
    let r = texel_depth(t + vec2(1.0, 0.0));
    let u = texel_depth(t - vec2(0.0, 1.0));
    let b = texel_depth(t + vec2(0.0, 1.0));
    let gx = select(d - l, r - d, abs(r - d) < abs(d - l));
    let gy = select(d - u, b - d, abs(b - d) < abs(d - u));
    return vec2(gx, gy) / pitch;
}

// The old estimate's blur: the same 4 x 4 texels, kept to texels within about 5 % of this
// pixel's distance.
fn old_openness_at(px: vec2<i32>, d: f32) -> f32 {
    let z = ao.params.z / d;
    let size = vec2<i32>(textureDimensions(openness));
    // The 4 x 4 texels around this pixel: texel q stands for full pixel 2q.
    let first = (px >> vec2(1u)) - 2 + (px & vec2(1));
    var total = 0.0;
    var weight = 0.0;
    // The texel nearest this pixel's distance, for a pixel no texel shares it with (a
    // pole or a grass blade a pixel wide, a silhouette pixel in an odd row or column).
    var nearest = 1.0;
    var nearest_gap = 3.4e38;
    for (var y = 0; y < 4; y++) {
        for (var x = 0; x < 4; x++) {
            let t = textureLoad(openness, clamp(first + vec2(x, y), vec2(0), size - 1), 0);
            let gap = abs(t.g - z);
            // Only texels within a few per cent of this pixel's distance.
            let w = max(1.0 - gap / (0.05 * z + 1.0), 0.0);
            total += t.r * w;
            weight += w;
            if gap < nearest_gap {
                nearest_gap = gap;
                nearest = t.r;
            }
        }
    }
    return select(nearest, total / weight, weight > 0.0);
}
