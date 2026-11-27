//! The two compute shaders around the network: the HDR scene to the display proxy the network
//! reads, and the head's answer back onto the scene. The transforms are the demo's
//! (`OpenDLSS-NR/docs/frame.md`): white divide, the soft shoulder above 0.75 (K = 5.770780),
//! sRGB, the half grid; the composite inverts them. Single-frame prototype: no history blend yet
//! (the network's blend logit is read but the temporal path is a later phase).

/// f32 -> half -> f32 round-to-nearest-even for values in the normal half range (the proxy is
/// 0..1): the classic mantissa rounding, exact enough for the display proxy (the bit-exact
/// publication matters for parity fixtures, not for the picture).
const ROUND_F16: &str = r#"
fn round_f16(x: f32) -> f32 {
    let u = bitcast<u32>(x);
    let rounded = (u + 0x0fffu + ((u >> 13u) & 1u)) & 0xffffe000u;
    return bitcast<f32>(rounded);
}
fn round_f16_3(v: vec3<f32>) -> vec3<f32> {
    return vec3(round_f16(v.x), round_f16(v.y), round_f16(v.z));
}
"#;

pub const ENCODE: &str = r#"
struct Params {
    width: u32,
    height: u32,
    paper_white: f32,
    exposure: f32,
}
@group(0) @binding(0) var color: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> proxy: array<vec4<f32>>;
@group(0) @binding(2) var<uniform> params: Params;

fn srgb_encode(v: vec3<f32>) -> vec3<f32> {
    let lo = 12.92 * v;
    let hi = 1.055 * pow(max(v, vec3(0.0)), vec3(1.0 / 2.4)) - 0.055;
    return select(lo, hi, v > vec3(0.0031308));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) { return; }
    let scene = max(textureLoad(color, vec2<i32>(id.xy), 0).rgb, vec3(0.0));
    var v = scene * params.exposure / max(params.paper_white, 0.05);
    let over = v > vec3(0.75);
    let knee = vec3(0.75) + 0.25 * (vec3(1.0) - exp(-5.770780 * (v - vec3(0.75))));
    v = select(v, knee, over);
    let code = round_f16_3(srgb_encode(clamp(v, vec3(0.0), vec3(1.0))));
    proxy[id.y * params.width + id.x] = vec4(code, 1.0);
}
"#;

pub const COMPOSITE: &str = r#"
struct Params {
    width: u32,
    height: u32,
    full_width: u32,
    full_height: u32,
    paper_white: f32,
    exposure: f32,
    intensity: f32,
    style: f32,
    local_tone: f32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}
@group(0) @binding(0) var out_color: texture_storage_2d<rgba16float, read_write>;
@group(0) @binding(1) var<storage, read> head: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> proxy: array<vec4<f32>>;
@group(0) @binding(3) var<uniform> params: Params;

fn srgb_decode(v: vec3<f32>) -> vec3<f32> {
    let lo = v / 12.92;
    let hi = pow((v + 0.055) / 1.055, vec3(2.4));
    return select(lo, hi, v > vec3(0.04045));
}

// The native runtime's Natural/Cinematic grade (OpenDLSS-NR demo nr_composite.comp, MIT):
// per-channel exposure + smoothstep contrast in proxy code space, then a saturation scale in HSL.
fn nr_hsl(rgb: vec3<f32>) -> vec3<f32> {
    let high = max(max(rgb.r, rgb.g), rgb.b);
    let low = min(min(rgb.r, rgb.g), rgb.b);
    let sum = high + low;
    let light = sum * 0.5;
    var hue = 0.0;
    var saturation = 0.0;
    if (high > low) {
        let delta = high - low;
        saturation = select(delta / sum, delta / ((2.0 - high) - low), light > 0.5);
        if (high == rgb.r) {
            hue = ((rgb.g - rgb.b) / delta + select(0.0, 6.0, rgb.g < rgb.b)) / 6.0;
        } else if (high == rgb.g) {
            hue = ((rgb.b - rgb.r) / delta + 2.0) / 6.0;
        } else {
            hue = ((rgb.r - rgb.g) / delta + 4.0) / 6.0;
        }
    }
    return vec3(hue, saturation, light);
}

fn nr_hue(p: f32, q: f32, hue_in: f32) -> f32 {
    var h = hue_in;
    if (h < 0.0) { h += 1.0; }
    if (h > 1.0) { h -= 1.0; }
    if (h < 0.16666667) { return p + (q - p) * 6.0 * h; }
    if (h < 0.5) { return q; }
    if (h < 0.6666667) { return p + (q - p) * (0.6666667 - h) * 6.0; }
    return p;
}

fn nr_rgb(hsl: vec3<f32>) -> vec3<f32> {
    if (hsl.y <= 0.0) { return vec3(hsl.z); }
    let q = select(hsl.z + hsl.y - hsl.z * hsl.y, hsl.z * (hsl.y + 1.0), hsl.z < 0.5);
    let p = 2.0 * hsl.z - q;
    return vec3(nr_hue(p, q, hsl.x + 0.33333334), nr_hue(p, q, hsl.x), nr_hue(p, q, hsl.x - 0.33333334));
}

fn nr_style(neural: vec3<f32>, style: f32, tone: f32) -> vec3<f32> {
    let style_tone = clamp(tone, 0.0, 1.0);
    let exposure = select(0.0, -0.1 * style_tone, style == 1.0);
    let contrast = select(0.0, -0.25 * style_tone, style == 1.0);
    let saturation = select(-0.15 * style_tone, -0.1 * style_tone, style == 1.0);
    var color = vec3(0.0);
    for (var c = 0u; c < 3u; c++) {
        let value = clamp(neural[c], 0.0, 1.0);
        let exposed = clamp(value * exp2(exposure), 0.0, 1.0);
        let square = exposed * exposed;
        let curve_delta = square * (3.0 - 2.0 * exposed) - exposed;
        let curved = clamp(contrast * curve_delta + exposed, 0.0, 1.0);
        let first_gamma = exp2(log2(max(curved, 1e-7)));
        color[c] = exp2(log2(max(first_gamma, 1e-7)));
    }
    let hsl = nr_hsl(color);
    let adjusted = nr_rgb(vec3(hsl.x, clamp(hsl.y * (saturation + 1.0), 0.0, 1.0), hsl.z));
    let again = nr_hsl(adjusted);
    return clamp(nr_rgb(vec3(again.x, clamp(exp2(log2(max(again.y, 1e-7))), 0.0, 1.0), again.z)), vec3(0.0), vec3(1.0));
}

@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if (id.x >= params.width || id.y >= params.height) { return; }
    let p = proxy[id.y * params.width + id.x].rgb;
    // The head is field-indexed (the padded field the network ran on).
    let h = head[id.y * params.full_width + id.x];
    var neural = clamp(p + h.rgb / 4.0, vec3(0.0), vec3(1.0));
    if (params.style != 0.0) {
        neural = nr_style(neural, params.style, params.local_tone);
    }
    neural = clamp(params.intensity * (neural - p) + p, vec3(0.0), vec3(1.0));
    var v = srgb_decode(neural);
    let over = v > vec3(0.75);
    let inv = vec3(0.75) - log(max(vec3(1.0) - (v - vec3(0.75)) / 0.25, vec3(1e-6))) / 5.770780;
    v = select(v, inv, over);
    let hdr = v * max(params.paper_white, 0.05) / max(params.exposure, 1e-6);
    let alpha = textureLoad(out_color, vec2<i32>(id.xy)).a;
    textureStore(out_color, vec2<i32>(id.xy), vec4(hdr, alpha));
}
"#;

/// The encode shader with the half-rounding helper prepended.
pub fn encode_source() -> String {
    format!("{ROUND_F16}\n{ENCODE}")
}
