// NVForever's reactive and transparency/composition masks for FSR 3.1 (fsr.rs
// `AuxiliaryPipelines::prepare_inputs`; docs/design/fsr-upscaling.md "The reactive mask").
//
// Presence: how much the transparent phase changed a pixel (water, refraction, particles,
// glows, glass, hair): the opaque-only HDR copy against the finished frame, relative to the
// brighter of the two, at most 0.9.
//
// FSR takes a pixel's history from where its motion vector points, and a blended shape is
// in no prepass: the vector is that of the opaque surface behind it. So the history of the
// pixels a blended shape covered last frame still holds the shape, and a moving one (a
// projectile's glow, a muzzle flash, smoke, hair, a lamp's halo) left a trail where it had
// been. Last frame's presence is looked up where this pixel's history comes from:
// - The transparency/composition mask takes the more of this frame's presence and that one:
//   wherever a blended shape is or was, FSR clamps the history tighter (no trail).
// - The reactive mask (FSR keeps less history) takes only what changed: presence that
//   appeared (above all of last frame's within a texel of the history's source) or went
//   (above all of this frame's within a texel). A blended shape that stays on the surface
//   behind it (a neon tube, a sign, a window, a decal) keeps its history and its edges are
//   anti-aliased over time; marked reactive, a thin, very bright tube was drawn from one
//   jittered frame at a time, its jagged edges moved every frame and the bloom after FSR
//   spread them into a wobbling halo [observed 2026-11-08: a player's video of the Strip at
//   night; the harness's neon case, frame-to-frame change on the tube 0.051 marked reactive,
//   0.003 now]. The one-texel margin keeps the jitter, which makes a thin tube's presence come
//   and go between samples, from counting as a change.
//
// Light change: a light that moves (a muzzle flash, a projectile's light, the flashlight, a
// swaying tree's shadow) changes the shading of surfaces that do not move. Their motion
// vectors carry the old shading along, and FSR's history, an average over many frames, lags
// the light even when each frame changes little; its colour clamp lets it, as a light's
// smooth gradient widens the clamp. FSR 3.1's own shading-change detection compares sorted
// sets of luminance, which a textured surface's detail keeps overlapping: it barely tells a
// moving light from a flicker [observed, tools/fsr-validation ghosting.rs]. Here each of a
// pixel's 3x3 texels is compared with what FSR's last output showed at the same surface
// point (by the texel's own motion, this frame's jitter undone); where at least 8 of the 9
// are brighter, or darker, by more than a texture's detail makes them differ, the history is
// stale and the reactive mask says so. With 7 of 9 thin aliased detail (fence slats, a
// ridge against a hill, where the jitter makes samples come and go) set it off now and then
// and flickered more [observed, the harness's fence case].
//
// Two passes, so that the neighbourhoods read small textures: `presence_pass` reads each
// pixel's two colours once and writes its presence and luminance; `fragment` makes the masks
// from those. In one pass the 3x3 neighbourhoods re-read about 30 colour texels a pixel: 0.25
// ms a frame at 4K Quality on an RTX 5090, a third of FSR's own time [observed 2026-11-08,
// `--log-frame-times` sub-span `nvf_fsr_3_1/inputs`].

// The first pass's inputs: the opaque-only copy and the finished frame (display-size
// textures, the render rectangle read).
@group(0) @binding(0) var opaque: texture_2d<f32>;
@group(0) @binding(1) var scene: texture_2d<f32>;
// The second pass's inputs.
// Bevy's motion-vector prepass: current minus previous position, in UV of the render
// rectangle (the top-left render-size part of a display-size texture).
@group(0) @binding(2) var motion: texture_2d<f32>;
// Last frame's presence (render size), cleared on a history reset.
@group(0) @binding(3) var previous_presence: texture_2d<f32>;
// The luminance of FSR's last output (render size; fsr_fallback.wgsl `luma`); 0: none.
@group(0) @binding(4) var history_luma: texture_2d<f32>;
@group(0) @binding(5) var linear_clamp: sampler;
// xy: FSR's jitter this frame (render pixels, AMD's sign); zw unused.
@group(0) @binding(6) var<uniform> jitter: vec4<f32>;
// This frame's presence and luminance (render size), from the first pass.
@group(0) @binding(7) var presence_now: texture_2d<f32>;
@group(0) @binding(8) var luma_now: texture_2d<f32>;

struct Pixel {
    // This frame's presence (kept for the next frame too) and the finished frame's luminance.
    @location(0) presence: f32,
    @location(1) luma: f32,
}

struct Masks {
    // FSR's reactive mask: keep less history.
    @location(0) reactive: f32,
    // FSR's transparency and composition mask: clamp the history tighter.
    @location(1) composition: f32,
}

// The light change (|ln| of the ratio, agreed by 8 of the 9 texels) below which the mask
// stays clear, and at which it is full. A fire's flicker (lights.rs: smoothed steps between
// 0.8 and 1, 12 a second) changes a frame by a few percent [engineering choice, the GPU
// harness's moving light and flicker cases, tools/fsr-validation ghosting.rs].
const LIGHT_CHANGE_CLEAR: f32 = 0.12;
const LIGHT_CHANGE_FULL: f32 = 0.5;
// Luminance added to both sides, so that changes among near-black values do not count.
const LIGHT_FLOOR: f32 = 0.05;

@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

fn luminance(c: vec3<f32>) -> f32 {
    return dot(max(c, vec3<f32>(0.0)), vec3<f32>(0.2126, 0.7152, 0.0722));
}

// The first pass: presence and luminance of each render pixel.
@fragment fn presence_pass(@builtin(position) p: vec4<f32>) -> Pixel {
    let at = vec2<i32>(p.xy);
    let a = max(textureLoad(opaque, at, 0).rgb, vec3<f32>(0.0));
    let b = max(textureLoad(scene, at, 0).rgb, vec3<f32>(0.0));
    let d = abs(a - b) / max(max(a, b), vec3<f32>(0.05));
    var pixel: Pixel;
    pixel.presence = clamp(max(d.r, max(d.g, d.b)), 0.0, 0.9);
    pixel.luma = luminance(b);
    return pixel;
}

fn presence(at: vec2<i32>) -> f32 {
    return textureLoad(presence_now, at, 0).r;
}

// The most presence last frame among the texels this pixel's history is read from: those
// of the four around `source` (render pixels) with a real share of the bilinear read. At a
// texel's centre (no motion, or whole-texel motion) that is the texel alone, so a still
// view keeps the masks it had without the lookup.
fn presence_before(source: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(previous_presence));
    let f = source - 0.5;
    let base = floor(f);
    let w = f - base;
    let weights = vec4<f32>((1.0 - w.x) * (1.0 - w.y), w.x * (1.0 - w.y), (1.0 - w.x) * w.y, w.x * w.y);
    var most = 0.0;
    for (var i = 0; i < 4; i++) {
        let at = vec2<i32>(base) + vec2<i32>(i & 1, i >> 1);
        if weights[i] > 0.0625 && all(at >= vec2<i32>(0)) && all(at < size) {
            most = max(most, textureLoad(previous_presence, at, 0).r);
        }
    }
    return most;
}

// How far the light around `at` has moved away from FSR's history: of ln(now / history) at
// the 3x3 texels, the second smallest when it is above 0 (8 of 9 brighter), or minus the
// second largest when that is below 0 (8 of 9 darker); 0 otherwise, without a history, or
// where a texel's surface point was off screen.
fn light_change(at: vec2<i32>) -> f32 {
    let size = vec2<f32>(textureDimensions(previous_presence));
    // The two smallest and the two largest ratios, in registers: a sorted array indexed by a
    // variable went to the GPU's local memory and was the pass's cost.
    var low = vec2<f32>(1e30);
    var high = vec2<f32>(-1e30);
    for (var i = 0; i < 9; i++) {
        let here = clamp(at + vec2<i32>(i % 3 - 1, i / 3 - 1), vec2<i32>(0), vec2<i32>(size) - 1);
        // The surface point this texel shows (its centre less this frame's jitter) and where
        // it was last frame, in render pixels; FSR's output is unjittered.
        let point = vec2<f32>(here) + 0.5 - jitter.xy - textureLoad(motion, here, 0).xy * size;
        if any(point < vec2<f32>(0.0)) || any(point > size) {
            return 0.0;
        }
        let history = textureSampleLevel(history_luma, linear_clamp, point / size, 0.0).r;
        if history <= 0.0 {
            return 0.0;
        }
        let now = textureLoad(luma_now, here, 0).r;
        let r = log((now + LIGHT_FLOOR) / (history + LIGHT_FLOOR));
        low = vec2<f32>(min(low.x, r), min(low.y, max(low.x, r)));
        high = vec2<f32>(max(high.x, r), max(high.y, min(high.x, r)));
    }
    return max(max(low.y, -high.y), 0.0);
}

// The most presence last frame within a texel of `source` (render pixels): what a blended
// shape left at and around this pixel's history.
fn presence_before_around(source: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(previous_presence));
    let c = vec2<i32>(floor(source));
    var most = 0.0;
    for (var i = 0; i < 9; i++) {
        let t = c + vec2<i32>(i % 3 - 1, i / 3 - 1);
        if all(t >= vec2<i32>(0)) && all(t < size) {
            most = max(most, textureLoad(previous_presence, t, 0).r);
        }
    }
    return most;
}

// The most presence this frame within a texel of `at`: a shape still there, by the jitter.
fn presence_around(at: vec2<i32>) -> f32 {
    let size = vec2<i32>(textureDimensions(previous_presence));
    var most = 0.0;
    for (var i = 0; i < 9; i++) {
        most = max(most, presence(clamp(at + vec2<i32>(i % 3 - 1, i / 3 - 1), vec2<i32>(0), size - 1)));
    }
    return most;
}

@fragment fn fragment(@builtin(position) p: vec4<f32>) -> Masks {
    let at = vec2<i32>(p.xy);
    let size = vec2<f32>(textureDimensions(previous_presence));
    // Where this pixel's history comes from, in render pixels.
    let source = p.xy - textureLoad(motion, at, 0).xy * size;
    let now = presence(at);
    let before = presence_before(source);
    let appeared = now - presence_before_around(source);
    let went = before - presence_around(at);
    let light = 0.9 * smoothstep(LIGHT_CHANGE_CLEAR, LIGHT_CHANGE_FULL, light_change(at));
    var masks: Masks;
    masks.reactive = max(max(appeared, went), max(light, 0.0));
    masks.composition = max(now, before);
    return masks;
}
