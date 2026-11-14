// Screen blur of image space modifiers (imagespace.rs): a Gaussian blur in two passes,
// across and then down, before the grade (docs/spec-notes/image-modifiers.md §5), and
// the depth of field, each pass mixing a point towards a blur of fixed radius by its
// depth's weight (§8).

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

// The grade pass's uniform (imagespace.wesl); the blur radius, the sharp depth and the
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
    // VATS mode: the grade's colour, what a picture's lightness is multiplied by.
    vats_tint: vec4<f32>,
    // Depth of field: strength, distance, range, the target's distance (< 0: the screen
    // centre's); use target, flags, the camera's near plane (imagespace.rs).
    dof: vec4<f32>,
    dof_mode: vec4<f32>,
    // Motion blur: the exposure in frames (0: none), the sharp depth of the player's own
    // body, the longest smear (a part of the screen height); this frame's clip space to
    // the last one's (imagespace.rs).
    motion: vec4<f32>,
    reproject: mat4x4<f32>,
    // Photo mode: warmth, vignette, film grain, letterbox aspect (imagespace.rs).
    photo: vec4<f32>,
    // Photo mode's lens blur (bokeh.rs): the aperture (0: off), the focus (game units),
    // the largest disc (a part of the screen height).
    bokeh: vec4<f32>,
}

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: Grade;
@if(MULTISAMPLED) @group(0) @binding(3) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(0) @binding(3) var depth: texture_depth_2d;

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

// Motion blur (image-modifiers.md §9.4): samples along the point's own motion on the
// screen over the exposure [hypothesis, the count].
const MOTION_TAPS: i32 = 12;

// The depth prepass's value at `uv` (reverse depth: larger is nearer, 0 the sky).
fn raw_depth(uv: vec2<f32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(depth));
    let p = clamp(vec2<i32>(uv * vec2<f32>(dims)), vec2(0), dims - 1);
    return textureLoad(depth, p, 0);
}

// The player's own body, which moves with the view and is left out of the smear.
fn own_body(d: f32) -> bool {
    return grade.motion.y > 0.0 && d > grade.motion.y;
}

// Each point smeared back along where it was on the screen: its depth reprojected into
// the last frame's view gives its motion over the frame, scaled to the exposure and held
// to the longest smear; the samples average the picture along that line, leaving out the
// player's own body. The sky's depth (0) reprojects as a direction, so it smears by the
// turn alone.
@fragment
fn motion_blur(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let centre = textureSampleLevel(screen, screen_sampler, uv, 0.0);
    let d = raw_depth(uv);
    if grade.motion.x <= 0.0 || own_body(d) {
        return centre;
    }
    let before = grade.reproject * vec4(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, d, 1.0);
    if before.w <= 1.0e-6 {
        return centre;
    }
    let ndc = before.xy / before.w;
    let was = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    let dims = vec2<f32>(textureDimensions(screen));
    var smear = (uv - was) * grade.motion.x * dims;
    let len = length(smear);
    let most = grade.motion.z * dims.y;
    if len > most {
        smear *= most / len;
    }
    if length(smear) < 0.5 {
        return centre;
    }
    let step = smear / dims / f32(MOTION_TAPS - 1);
    var acc = centre.rgb;
    var total = 1.0;
    for (var i = 1; i < MOTION_TAPS; i++) {
        let at = clamp(uv - step * f32(i), vec2(0.0), vec2(1.0));
        if own_body(raw_depth(at)) {
            continue;
        }
        acc += textureSampleLevel(screen, screen_sampler, at, 0.0).rgb;
        total += 1.0;
    }
    return vec4(acc / total, centre.a);
}

@fragment
fn blur_across(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(1.0, 0.0));
}

@fragment
fn blur_down(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    return blur(in.uv, vec2(0.0, 1.0));
}

// ---- Photo mode's lens blur (bokeh.rs has the rules and their tests) -------------------
//
// Three passes while photo mode's Blur is up (grade.bokeh.x > 0): `bokeh_prefilter`
// spreads what is blurred over the spiral's spacing (so a small bright point is met by
// the taps), `bokeh` gathers each point's disc, `bokeh_fill` smooths the spiral's grain.

const BOKEH_TAPS: i32 = 96;
const PREFILTER_TAPS: i32 = 12;
const GOLDEN_ANGLE: f32 = 2.3999631;
const SHARP_PX: f32 = 0.5;
const HIGHLIGHT_GAIN: f32 = 6.0;
const HIGHLIGHT_FROM: f32 = 0.7;

// The largest disc this frame (pixels, radius): the aperture's share of the largest.
fn bokeh_reach() -> f32 {
    return grade.bokeh.z * f32(textureDimensions(screen).y) * clamp(grade.bokeh.x, 0.0, 1.0);
}

// bokeh.rs coc_px: the disc (pixels, radius) a point `z` game units away is drawn as.
fn coc_px(z: f32) -> f32 {
    let focus = max(grade.bokeh.y, 1.0);
    var spread = 1.0;
    if z < 1.0e29 {
        spread = abs(z - focus) / max(z, 1.0e-3);
    }
    let most = grade.bokeh.z * f32(textureDimensions(screen).y);
    let r = min(most * clamp(grade.bokeh.x, 0.0, 1.0) * spread, most);
    return select(r, 0.0, r < SHARP_PX);
}

// bokeh.rs tap_weight: a point behind reaches no farther than the gathering point's disc.
fn tap_weight(centre_coc: f32, tap_coc: f32, behind: bool, dist: f32) -> f32 {
    var reach = tap_coc;
    if behind {
        reach = min(tap_coc, centre_coc);
    }
    return clamp(reach - dist + 0.5, 0.0, 1.0);
}

// bokeh.rs highlight: near-white points weigh more.
fn highlight(c: vec3<f32>) -> f32 {
    let luma = dot(c, vec3(0.2126, 0.7152, 0.0722));
    let t = clamp((luma - HIGHLIGHT_FROM) / (1.0 - HIGHLIGHT_FROM), 0.0, 1.0);
    let s = t * t * (3.0 - 2.0 * t);
    return 1.0 + HIGHLIGHT_GAIN * s * s;
}

// Tap `i` of `n` on the golden-angle spiral, on the unit disc (bokeh.rs spiral).
fn spiral(i: i32, n: i32) -> vec2<f32> {
    let r = sqrt((f32(i) + 0.5) / f32(n));
    let a = f32(i) * GOLDEN_ANGLE;
    return vec2(cos(a), sin(a)) * r;
}

fn inside(uv: vec2<f32>) -> bool {
    return all(uv >= vec2(0.0)) && all(uv <= vec2(1.0));
}

// The spacing of the main pass's taps at its widest (pixels).
fn tap_spacing(reach: f32) -> f32 {
    return reach * 1.8 / sqrt(f32(BOKEH_TAPS));
}

@fragment
fn bokeh_prefilter(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let centre = textureSampleLevel(screen, screen_sampler, uv, 0.0);
    let reach = bokeh_reach();
    let r = tap_spacing(reach);
    if grade.bokeh.x <= 0.0 || r < 1.0 || coc_px(view_distance(uv)) < r {
        return centre;
    }
    let dims = vec2<f32>(textureDimensions(screen));
    let h0 = highlight(centre.rgb);
    var acc = centre.rgb * h0;
    var total = h0;
    for (var i = 0; i < PREFILTER_TAPS; i++) {
        let at = uv + spiral(i, PREFILTER_TAPS) * r / dims;
        // Only what is blurred as much: a sharp edge stays out.
        if !inside(at) || coc_px(view_distance(at)) < r {
            continue;
        }
        let s = textureSampleLevel(screen, screen_sampler, at, 0.0).rgb;
        let w = highlight(s);
        acc += s * w;
        total += w;
    }
    return vec4(acc / total, centre.a);
}

@fragment
fn bokeh(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let centre = textureSampleLevel(screen, screen_sampler, uv, 0.0);
    let reach = bokeh_reach();
    if grade.bokeh.x <= 0.0 || reach < SHARP_PX {
        return centre;
    }
    let dims = vec2<f32>(textureDimensions(screen));
    let z0 = view_distance(uv);
    let c0 = coc_px(z0);
    let h0 = highlight(centre.rgb);
    var acc = centre.rgb * h0;
    var total = h0;
    for (var i = 0; i < BOKEH_TAPS; i++) {
        let off = spiral(i, BOKEH_TAPS) * reach;
        let at = uv + off / dims;
        if !inside(at) {
            continue;
        }
        let z = view_distance(at);
        let w = tap_weight(c0, coc_px(z), z > z0, length(off));
        if w <= 0.0 {
            continue;
        }
        let s = textureSampleLevel(screen, screen_sampler, at, 0.0).rgb;
        let hw = w * highlight(s);
        acc += s * hw;
        total += hw;
    }
    return vec4(acc / total, centre.a);
}

@fragment
fn bokeh_fill(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let uv = in.uv;
    let centre = textureSampleLevel(screen, screen_sampler, uv, 0.0);
    if grade.bokeh.x <= 0.0 {
        return centre;
    }
    let c0 = coc_px(view_distance(uv));
    let r = min(c0 * 0.15, tap_spacing(bokeh_reach()));
    if r < 1.0 {
        return centre;
    }
    let dims = vec2<f32>(textureDimensions(screen));
    var acc = centre.rgb;
    var total = 1.0;
    for (var i = 0; i < 8; i++) {
        let a = f32(i) * 0.7853982;
        let at = uv + vec2(cos(a), sin(a)) * r / dims;
        // Neighbours at least half as blurred: a sharp edge is not pulled in.
        if !inside(at) || coc_px(view_distance(at)) < 0.5 * c0 {
            continue;
        }
        acc += textureSampleLevel(screen, screen_sampler, at, 0.0).rgb;
        total += 1.0;
    }
    return vec4(acc / total, centre.a);
}
