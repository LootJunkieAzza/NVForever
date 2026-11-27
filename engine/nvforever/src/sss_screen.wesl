// Screen-space skin scattering (nvforever sss/screen.rs; NVF_SSS=1 under NVF_PHYSICAL_PBR=1,
// unless NVF_SSSS=0). After the opaque pass, the diffuse light of skin is blurred across
// the skin by the d'Eon & Irving (2007) 6-Gaussian diffusion profile, as a separable pair of
// one-dimensional passes sized by depth (Jimenez, Sundstedt & Gutierrez 2009, "Screen-Space
// Perceptual Rendering of Human Skin"; Jimenez et al. 2015, "Separable Subsurface
// Scattering"), so the light of the sky, the ambient and every lamp spreads under the skin
// as the sun's did through the pre-integration.
//
// The skin's mark (sss/hook.rs `SKIN_MARK_TEXT`): an opaque skin fragment drawn under
// NVF_SKIN_SPLIT writes alpha = -1 - s, s its specular light's luminance as drawn. The
// specular light is taken as grey: the diffuse share is the colour less s, it is blurred,
// and s is added back. Where the blur leaves the light as it was the colour is the same
// as drawn, whatever the specular light's own tint.
//
// blur_h: the main texture in, the horizontal blur out (rgb: diffuse light, a: s, or -1
//   off the skin).
// blur_v: that in, the vertical blur times the skin's post-scatter colour (`albedo`, the
//   skin's fragments wrote it, sss.wesl `nvf_skin_post_scatter`: its square root; their
//   diffuse light came with it divided out) drawn over the main texture's skin with alpha 1
//   (the mark taken out), everything else discarded.
//
// Each tap is read between pixels along the pass's axis: the two texels either side of its
// offset, each checked on its own (skin, depth), mixed by the fraction. The step is
// shortened by the surface's slope along the axis (`tilt`; Jimenez et al. 2015): the
// kernel's millimetres lie on the surface, which a tilted surface foreshortens on screen.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import nvf_assets::first_person_depth::FIRST_PERSON_DEPTH_SHARE;

const TAPS: u32 = 17u;

struct Ssss {
    // x: pixels per millimetre at a distance of one game unit (the focal length in pixels
    // over the millimetres in a unit), y: the projection's near plane, z: the widest step
    // in pixels, w: the kernel's widest offset (mm).
    params: vec4<f32>,
    // The render rectangle in texels: x, y its first texel, z, w one past its last.
    rect: vec4<f32>,
    // The kernel from -w to +w (sss/screen.rs `kernel`): rgb the weight of each channel
    // (each channel's weights add to 1), a the offset in millimetres.
    kernel: array<vec4<f32>, 17>,
}

@group(0) @binding(0) var depth: texture_depth_2d;
@group(0) @binding(1) var<uniform> ssss: Ssss;
@group(0) @binding(2) var src: texture_2d<f32>;
@if(VERTICAL) @group(0) @binding(3) var albedo: texture_2d<f32>;
@if(VERTICAL) @group(0) @binding(4) var spec_tex: texture_2d<f32>;

// Alpha below this marks skin (-1.0); every other surface writes 0 to 1.
const SKIN_MARK: f32 = -0.5;
// Millimetres in one game unit (the same scale as sss.wesl's shadow-map thickness).
const MM_PER_UNIT: f32 = 14.3;

fn is_first_person(raw_d: f32) -> bool {
    return raw_d > 1.0 - FIRST_PERSON_DEPTH_SHARE;
}

// The skin's diffuse light at p (a >= 0 on skin, < 0 off the skin).
@if(!VERTICAL) fn fetch(p: vec2<i32>) -> vec4<f32> {
    let c = textureLoad(src, p, 0);
    if c.a > SKIN_MARK {
        return vec4(c.rgb, -1.0);
    }
    return vec4(c.rgb, 0.0);
}

@if(VERTICAL) fn fetch(p: vec2<i32>) -> vec4<f32> {
    return textureLoad(src, p, 0);
}

fn inside(p: vec2<i32>) -> vec2<i32> {
    return clamp(p, vec2<i32>(ssss.rect.xy), vec2<i32>(ssss.rect.zw) - vec2(1));
}

// Pixels per millimetre at view distance z, or 0 when the kernel would stay inside one
// pixel (the light as drawn).
fn px_per_mm(z: f32) -> f32 {
    let scale = min(ssss.params.x / max(z, 1e-3), ssss.params.z / ssss.params.w);
    return select(0.0, scale, scale * ssss.params.w >= 0.5);
}

// The light of the texel `step` pixels from px along dir, for a tap `reach` mm out. A texel
// off the skin, or on another surface (a depth step larger than the offset can explain: an
// ear in front of the head, the chin over the neck), gives the centre's light instead, so
// light neither leaves nor enters the skin there (Jimenez et al. 2009 "follow the surface").
fn tap_light(px: vec2<i32>, dir: vec2<i32>, step: i32, centre: vec4<f32>, z: f32, reach: f32) -> vec3<f32> {
    if step == 0 {
        return centre.rgb;
    }
    let p = inside(px + dir * step);
    let tap = fetch(p);
    let raw = textureLoad(depth, p, 0);
    if tap.a >= 0.0 && raw > 0.0 && !is_first_person(raw) {
        let dz_mm = abs(ssss.params.y / raw - z) * MM_PER_UNIT;
        let away = smoothstep(0.85 * reach + 1.0, 1.85 * reach + 2.5, dz_mm);
        return mix(tap.rgb, centre.rgb, away);
    }
    return centre.rgb;
}

// The view distance at p, or -1 off the world (no depth, the first-person shapes).
fn distance_at(p: vec2<i32>) -> f32 {
    let raw = textureLoad(depth, inside(p), 0);
    return select(-1.0, ssss.params.y / max(raw, 1e-12), raw > 0.0 && !is_first_person(raw));
}

// The share of `scale` along dir the surface's slope leaves: 1 / sqrt(1 + slope²), slope
// the depth's change per millimetre across the screen, the gentler side's (a silhouette's
// step on one side is no slope), no less than 0.35 [hypothesis: skin research notes
// 2026-11-26 §2.1].
fn tilt(px: vec2<i32>, dir: vec2<i32>, z: f32, scale: f32) -> f32 {
    let a = distance_at(px - dir);
    let b = distance_at(px + dir);
    var dz = 1e9;
    if a > 0.0 {
        dz = min(dz, abs(z - a));
    }
    if b > 0.0 {
        dz = min(dz, abs(b - z));
    }
    if dz >= 1e9 {
        return 1.0;
    }
    let slope = dz * MM_PER_UNIT * scale;
    return clamp(inverseSqrt(1.0 + slope * slope), 0.35, 1.0);
}

// One direction of the blur.
fn blur(px: vec2<i32>, dir: vec2<i32>, centre: vec4<f32>, z: f32, scale_in: f32) -> vec3<f32> {
    let scale = scale_in * tilt(px, dir, z, scale_in);
    var sum = vec3(0.0);
    for (var i = 0u; i < TAPS; i++) {
        let k = ssss.kernel[i];
        let at = k.a * scale;
        let lo = floor(at);
        let t = at - lo;
        let step = i32(lo);
        let reach = abs(k.a);
        var light = tap_light(px, dir, step, centre, z, reach);
        if t > 1e-3 {
            light = mix(light, tap_light(px, dir, step + 1, centre, z, reach), t);
        }
        sum += light * k.rgb;
    }
    return sum;
}

// The hairline's shade (blur_v): skin beside hair is shaded by it, the light under a
// hairline's thinning hairs and over the scalp between them being less and warmer, red
// least taken (it scatters in from the lit skin around). The skin-colour texture's alpha
// tells what drew each pixel (sss.wesl, NVF_MARK_*: hair 0, skin from 0.6 with its strand
// noise, anything else the clear's 0.3). Around a skin pixel, rings out to HAIR_REACH_MM
// are read in eight directions, turned per pixel (a fixed dither); a tap counts if it is
// hair at about the skin's depth (from HAIR_BACK_MM behind to HAIR_FRONT_MM in front: the
// head's own hair, not another figure's nor hair seen past the head), the nearer rings
// more. At the hair's edge about half the taps are hair. The pixel's strand noise widens
// its reach, so the shade runs out from the hairline in streaks, as hairs do. Independent
// of how a hair mesh is built (alpha-tested cards, blended shells): only where hair is
// drawn next to skin counts. [hypothesis: the constants, starting values to tune by eye.]
const HAIR_REACH_MM: f32 = 6.0;
const HAIR_FRONT_MM: f32 = 40.0;
const HAIR_BACK_MM: f32 = 10.0;
const HAIR_DARK: vec3<f32> = vec3(0.30, 0.36, 0.40);
const MARK_HAIR_BELOW: f32 = 0.15;
const MARK_SKIN: f32 = 0.6;
const MARK_STRAND: f32 = 0.4;

@if(VERTICAL) fn hairline(px: vec2<i32>, z: f32, scale: f32) -> vec3<f32> {
    let reach_px = HAIR_REACH_MM * scale;
    if reach_px < 1.0 {
        return vec3(1.0);
    }
    let strand = saturate((textureLoad(albedo, px, 0).a - MARK_SKIN) / MARK_STRAND);
    let p = vec2<f32>(px);
    let turn = fract(52.9829189 * fract(0.06711056 * p.x + 0.00583715 * p.y)) * 0.7853982;
    var cover = 0.0;
    var total = 0.0;
    for (var r = 1u; r <= 4u; r++) {
        let rr = f32(r) * 0.25;
        let w = 1.25 - rr;
        for (var d = 0u; d < 8u; d++) {
            let a = turn + f32(d) * 0.7853982 + rr * 0.4;
            let q = inside(px + vec2<i32>(round(vec2(cos(a), sin(a)) * rr * reach_px)));
            total += w;
            if textureLoad(albedo, q, 0).a >= MARK_HAIR_BELOW {
                continue;
            }
            let raw = textureLoad(depth, q, 0);
            if raw <= 0.0 || is_first_person(raw) {
                continue;
            }
            let nearer_mm = (z - ssss.params.y / raw) * MM_PER_UNIT;
            if nearer_mm > -HAIR_BACK_MM && nearer_mm < HAIR_FRONT_MM {
                cover += w;
            }
        }
    }
    let h = saturate(2.0 * (cover / total) * (0.5 + 1.5 * strand));
    return vec3(1.0) - HAIR_DARK * (h * h * (3.0 - 2.0 * h));
}

// The skin's post-scatter colour at p (sss.wesl `nvf_skin_post_scatter`).
@if(VERTICAL) fn post_scatter(p: vec2<i32>) -> vec3<f32> {
    let a = textureLoad(albedo, p, 0).rgb;
    return a * a;
}

@if(!VERTICAL) @fragment
fn blur_h(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = inside(vec2<i32>(floor(in.position.xy)));
    let centre = fetch(px);
    if centre.a < 0.0 {
        return vec4(0.0, 0.0, 0.0, -1.0);
    }
    let raw = textureLoad(depth, px, 0);
    if raw <= 0.0 || is_first_person(raw) {
        return centre;
    }
    let z = ssss.params.y / raw;
    let scale = px_per_mm(z);
    if scale <= 0.0 {
        return centre;
    }
    return vec4(blur(px, vec2(1, 0), centre, z, scale), centre.a);
}

@if(VERTICAL) @fragment
fn blur_v(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = vec2<i32>(floor(in.position.xy));
    if any(vec2<f32>(px) < ssss.rect.xy) || any(vec2<f32>(px) >= ssss.rect.zw) {
        discard;
    }
    let centre = fetch(px);
    if centre.a < 0.0 {
        discard;
    }
    let raw = textureLoad(depth, px, 0);
    var light = centre.rgb;
    var shade = vec3(1.0);
    if raw > 0.0 && !is_first_person(raw) {
        let z = ssss.params.y / raw;
        let scale = px_per_mm(z);
        if scale > 0.0 {
            light = blur(px, vec2(0, 1), centre, z, scale);
            shade = hairline(px, z, scale);
        }
    }
    let spec = textureLoad(spec_tex, px, 0).rgb;
    return vec4(max((light * post_scatter(px) + spec) * shade, vec3(0.0)), 1.0);
}
