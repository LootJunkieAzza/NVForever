// HDR output's highlights (hdr.rs). The picture comes unclipped: the tonemapping pass and
// the image-space grade keep light above 1 (lights.rs `GAME_HDR`, imagespace.wesl).
// 1. The SDR picture (each channel held to 1) keeps everything below the knee. Above it,
//    the brightest channel is stretched (square of the distance, slope kept at the knee)
//    so SDR white lands at `white`: paper white plus the expansion's share of 60 % of the
//    room to the peak. Colour ratios stay. Expansion 0 keeps the SDR picture exactly.
// 2. Light past SDR white (what SDR cuts off: sun, fire, neon), times the source gain,
//    rises from `white` towards the peak with a Reinhard shoulder (slope 1) on its
//    brightest channel, the other channels scaled by the same factor, so its colour
//    stays, no value passes the peak and none is cut.
// 3. The hue of a changed pixel is turned, in Oklab, towards the hue of the SDR picture
//    (the original's per-channel clip) by the hue restore; lightness and chroma stay.
// The HUD is drawn after this pass at the HUD's white.
// `params.a`: x the peak and z the picture's white, both in units of the HUD's white; y the
// hue restore (0 to 1); w above 0.5 shows the brightness heatmap instead. `params.b`: x the
// source gain, y the expansion (0 to 1), z the knee.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
struct HdrParams {
    a: vec4<f32>,
    b: vec4<f32>,
}

@group(0) @binding(2) var<uniform> params: HdrParams;

// Linear sRGB to Oklab and back (Björn Ottosson's published matrices).
fn to_oklab(c: vec3<f32>) -> vec3<f32> {
    let l = 0.4122214708 * c.r + 0.5363325363 * c.g + 0.0514459929 * c.b;
    let m = 0.2119034982 * c.r + 0.6806995451 * c.g + 0.1073969566 * c.b;
    let s = 0.0883024619 * c.r + 0.2817188376 * c.g + 0.6299787005 * c.b;
    let lms = pow(max(vec3(l, m, s), vec3(0.0)), vec3(1.0 / 3.0));
    return vec3(
        0.2104542553 * lms.x + 0.7936177850 * lms.y - 0.0040720468 * lms.z,
        1.9779984951 * lms.x - 2.4285922050 * lms.y + 0.4505937099 * lms.z,
        0.0259040371 * lms.x + 0.7827717662 * lms.y - 0.8086757660 * lms.z,
    );
}

fn from_oklab(o: vec3<f32>) -> vec3<f32> {
    let l = o.x + 0.3963377774 * o.y + 0.2158037573 * o.z;
    let m = o.x - 0.1055613458 * o.y - 0.0638541728 * o.z;
    let s = o.x - 0.0894841775 * o.y - 1.2914855480 * o.z;
    let lms = vec3(l * l * l, m * m * m, s * s * s);
    return vec3(
        4.0767416621 * lms.x - 3.3077115913 * lms.y + 0.2309699292 * lms.z,
        -1.2684380046 * lms.x + 2.6097574011 * lms.y - 0.3413193965 * lms.z,
        -0.0041960863 * lms.x - 0.7034186147 * lms.y + 1.7076147010 * lms.z,
    );
}

// The heatmap: grey (the SDR picture's luminance) up to paper white, then by the share of
// the room up to the peak: cyan, green, yellow, orange, and red from 98 % of the peak.
fn heat(rgb: vec3<f32>, room: f32) -> vec3<f32> {
    let m = max(max(rgb.r, rgb.g), rgb.b);
    if (m <= 1.0) {
        return vec3(dot(rgb, vec3(0.2126, 0.7152, 0.0722)) * 0.6);
    }
    let t = (m - 1.0) / max(room, 1e-4);
    if (t >= 0.98) {
        return vec3(1.0, 0.0, 0.0);
    }
    if (t >= 0.75) {
        return vec3(1.0, 0.5, 0.0);
    }
    if (t >= 0.5) {
        return vec3(1.0, 1.0, 0.0);
    }
    if (t >= 0.25) {
        return vec3(0.0, 1.0, 0.0);
    }
    return vec3(0.0, 1.0, 1.0);
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let c = textureSample(in_texture, in_sampler, in.uv);
    let scene = params.a.z;
    let rgb = max(c.rgb, vec3(0.0));
    let room = params.a.x / max(scene, 0.01) - 1.0;
    let sdr = min(rgb, vec3(1.0));
    let m = max(max(sdr.r, sdr.g), sdr.b);
    let knee = params.b.z;
    let white = 1.0 + clamp(params.b.y, 0.0, 1.0) * 0.6 * max(room, 0.0);
    if ((m <= knee && max(max(rgb.r, rgb.g), rgb.b) <= 1.0) || room <= 0.0) {
        if (params.a.w > 0.5) {
            return vec4(heat(sdr, 1.0) * scene, c.a);
        }
        return vec4(sdr * scene, c.a);
    }
    var out = sdr;
    if (m > knee && white > 1.0) {
        let t = (m - knee) / (1.0 - knee);
        let a = (white - knee) / (1.0 - knee) - 1.0;
        let out_m = knee + (m - knee) * (1.0 + a * t * t);
        out = sdr * (out_m / m);
    }
    let e = (rgb - sdr) * max(params.b.x, 0.0);
    let room2 = params.a.x / max(scene, 0.01) - white;
    let em = max(max(e.r, e.g), e.b);
    if (room2 > 0.0 && em > 0.0) {
        out += e * (room2 / (em + room2));
    }
    let strength = clamp(params.a.y, 0.0, 1.0);
    if (strength > 0.0) {
        let o = to_oklab(out);
        let r = to_oklab(sdr);
        let chroma = length(o.yz);
        let rc = length(r.yz);
        if (rc > 1e-4 && chroma > 1e-4) {
            let ab = mix(o.yz, r.yz / rc * chroma, strength);
            out = max(from_oklab(vec3(o.x, ab)), vec3(0.0));
        }
    }
    if (params.a.w > 0.5) {
        return vec4(heat(out, room) * scene, c.a);
    }
    return vec4(out * scene, c.a);
}
