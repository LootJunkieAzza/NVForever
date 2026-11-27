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
// blur_v: that in, the vertical blur drawn over the main texture's skin with alpha 1 (the
//   mark taken out), everything else discarded.

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

// Alpha below this marks skin (-1 - s, s >= 0); every other surface writes 0 to 1.
const SKIN_MARK: f32 = -0.5;
// Millimetres in one game unit (the same scale as sss.wesl's shadow-map thickness).
const MM_PER_UNIT: f32 = 14.3;

fn is_first_person(raw_d: f32) -> bool {
    return raw_d > 1.0 - FIRST_PERSON_DEPTH_SHARE;
}

// The skin's diffuse light and s at p, or a < 0 off the skin.
@if(!VERTICAL) fn fetch(p: vec2<i32>) -> vec4<f32> {
    let c = textureLoad(src, p, 0);
    if c.a > SKIN_MARK {
        return vec4(c.rgb, -1.0);
    }
    let s = -1.0 - c.a;
    return vec4(c.rgb - vec3(s), s);
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

// One direction of the blur. A tap off the skin, or on another surface (a depth step
// larger than the offset can explain: an ear in front of the head, the chin over the
// neck), takes the centre's light instead, so light neither leaves nor enters the skin
// there (Jimenez et al. 2009 "follow the surface").
fn blur(px: vec2<i32>, dir: vec2<i32>, centre: vec4<f32>, z: f32, scale: f32) -> vec3<f32> {
    var sum = vec3(0.0);
    for (var i = 0u; i < TAPS; i++) {
        let k = ssss.kernel[i];
        let step = i32(round(k.a * scale));
        if step == 0 {
            sum += centre.rgb * k.rgb;
            continue;
        }
        let p = inside(px + dir * step);
        let tap = fetch(p);
        let raw = textureLoad(depth, p, 0);
        var light = centre.rgb;
        if tap.a >= 0.0 && raw > 0.0 && !is_first_person(raw) {
            let dz_mm = abs(ssss.params.y / raw - z) * MM_PER_UNIT;
            let reach = abs(k.a);
            let away = smoothstep(2.0 * reach, 4.0 * reach + 4.0, dz_mm);
            light = mix(tap.rgb, centre.rgb, away);
        }
        sum += light * k.rgb;
    }
    return sum;
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
    if raw > 0.0 && !is_first_person(raw) {
        let z = ssss.params.y / raw;
        let scale = px_per_mm(z);
        if scale > 0.0 {
            light = blur(px, vec2(0, 1), centre, z, scale);
        }
    }
    return vec4(max(light + vec3(centre.a), vec3(0.0)), 1.0);
}
