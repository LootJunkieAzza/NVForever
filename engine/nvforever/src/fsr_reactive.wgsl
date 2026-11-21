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
// pixel's two colours once and writes its presence and its light change (ln of this frame's
// luminance over the history's at the same surface point); `fragment` makes the masks from
// those. In one pass the 3x3 neighbourhoods re-read about 30 colour texels a pixel: 0.25
// ms a frame at 4K Quality on an RTX 5090, a third of FSR's own time [observed 2026-11-08,
// `--log-frame-times` sub-span `nvf_fsr_3_1/inputs`]. Since 2026-11-10 the light change of a
// texel is worked out once, in the first pass, where the second read each texel's motion,
// history and luminance again for all nine of a pixel's neighbourhood, and the second pass
// takes its 3x3 neighbourhoods four texels at a read (`textureGather`): 15 reads a pixel
// where there were 51 [observed 2026-11-10, 4K Native AA: docs/design/fsr-upscaling.md
// "Performance"].

// The first pass's inputs: the opaque-only copy and the finished frame (display-size
// textures, the render rectangle read).
@group(0) @binding(0) var opaque: texture_2d<f32>;
@group(0) @binding(1) var scene: texture_2d<f32>;
// Both passes: Bevy's motion-vector prepass, current minus previous position, in UV of the
// render rectangle (the top-left render-size part of a display-size texture).
@group(0) @binding(2) var motion: texture_2d<f32>;
// The second pass: last frame's presence (render size), cleared on a history reset.
@group(0) @binding(3) var previous_presence: texture_2d<f32>;
// The first pass: the luminance of FSR's last output (render size; fsr_fallback.wgsl
// `luma`); 0: none.
@group(0) @binding(4) var history_luma: texture_2d<f32>;
// Both passes; clamped at the edges, which the 3x3 neighbourhoods rely on.
@group(0) @binding(5) var linear_clamp: sampler;
// The first pass. xy: FSR's jitter this frame (render pixels, AMD's sign); zw unused.
@group(0) @binding(6) var<uniform> jitter: vec4<f32>;
// The second pass: this frame's presence and light change (render size), from the first.
@group(0) @binding(7) var presence_now: texture_2d<f32>;
@group(0) @binding(8) var change_now: texture_2d<f32>;

struct Pixel {
    // This frame's presence (kept for the next frame too).
    @location(0) presence: f32,
    // ln(this frame's luminance / the history's) at this texel's surface point, or
    // NO_CHANGE_KNOWN.
    @location(1) change: f32,
}

// A texel without a light change: no history, or its surface point was off screen last
// frame. Far above any real one (|ln| of two half floats' ratio is under 15).
const NO_CHANGE_KNOWN: f32 = 1000.0;

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

// The first pass: presence and light change of each render pixel.
@fragment fn presence_pass(@builtin(position) p: vec4<f32>) -> Pixel {
    let at = vec2<i32>(p.xy);
    let a = max(textureLoad(opaque, at, 0).rgb, vec3<f32>(0.0));
    let b = max(textureLoad(scene, at, 0).rgb, vec3<f32>(0.0));
    let d = abs(a - b) / max(max(a, b), vec3<f32>(0.05));
    var pixel: Pixel;
    pixel.presence = clamp(max(d.r, max(d.g, d.b)), 0.0, 0.9);
    pixel.change = NO_CHANGE_KNOWN;
    // The surface point this texel shows (its centre less this frame's jitter) and where it
    // was last frame, in render pixels; FSR's output is unjittered.
    let size = vec2<f32>(textureDimensions(history_luma));
    let point = p.xy - jitter.xy - textureLoad(motion, at, 0).xy * size;
    if all(point >= vec2<f32>(0.0)) && all(point <= size) {
        let history = textureSampleLevel(history_luma, linear_clamp, point / size, 0.0).r;
        if history > 0.0 {
            pixel.change = log((luminance(b) + LIGHT_FLOOR) / (history + LIGHT_FLOOR));
        }
    }
    return pixel;
}

// The four texels of `t` whose top left one is `at`, read at once: x (0, 1), y (1, 1),
// z (1, 0), w (0, 0) from `at`; past an edge, the edge's texel.
fn block(t: texture_2d<f32>, at: vec2<i32>) -> vec4<f32> {
    return textureGather(0, t, linear_clamp, vec2<f32>(at + 1) / vec2<f32>(textureDimensions(t)));
}

// The most of the 3x3 texels of `t` around `c` (past an edge, the edge's texel): four
// overlapping blocks.
fn most_around(t: texture_2d<f32>, c: vec2<i32>) -> f32 {
    let m = max(max(block(t, c - 1), block(t, c + vec2<i32>(0, -1))), max(block(t, c + vec2<i32>(-1, 0)), block(t, c)));
    return max(max(m.x, m.y), max(m.z, m.w));
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

// The two smallest so far (x the smallest) with `r` taken in, and the two largest.
fn lower(low: vec2<f32>, r: f32) -> vec2<f32> {
    return vec2<f32>(min(low.x, r), min(low.y, max(low.x, r)));
}

fn higher(high: vec2<f32>, r: f32) -> vec2<f32> {
    return vec2<f32>(max(high.x, r), max(high.y, min(high.x, r)));
}

// How far the light around `at` has moved away from FSR's history: of the light change at
// the 3x3 texels (past an edge, the edge's texel), the second smallest when it is above 0 (8
// of 9 brighter), or minus the second largest when that is below 0 (8 of 9 darker); 0
// otherwise, without a history, or where a texel's surface point was off screen.
fn light_change(at: vec2<i32>) -> f32 {
    // The nine texels from four blocks: the first whole, then the texels it lacks.
    let a = block(change_now, at - 1);
    let b = block(change_now, at + vec2<i32>(0, -1));
    let c = block(change_now, at + vec2<i32>(-1, 0));
    let d = block(change_now, at);
    // The two smallest and the two largest, in registers: a sorted array indexed by a
    // variable went to the GPU's local memory and was the pass's cost.
    var low = vec2<f32>(1e30);
    var high = vec2<f32>(-1e30);
    low = lower(low, a.w); high = higher(high, a.w);
    low = lower(low, a.z); high = higher(high, a.z);
    low = lower(low, b.z); high = higher(high, b.z);
    low = lower(low, a.x); high = higher(high, a.x);
    low = lower(low, a.y); high = higher(high, a.y);
    low = lower(low, b.y); high = higher(high, b.y);
    low = lower(low, c.x); high = higher(high, c.x);
    low = lower(low, c.y); high = higher(high, c.y);
    low = lower(low, d.y); high = higher(high, d.y);
    if high.x > 0.5 * NO_CHANGE_KNOWN {
        return 0.0;
    }
    return max(max(low.y, -high.y), 0.0);
}

// The most presence last frame within a texel of `source` (render pixels): what a blended
// shape left at and around this pixel's history. Texels off the screen hold none.
fn presence_before_around(source: vec2<f32>) -> f32 {
    let size = vec2<i32>(textureDimensions(previous_presence));
    let c = vec2<i32>(floor(source));
    if any(c < vec2<i32>(-1)) || any(c > size) {
        return 0.0;
    }
    return most_around(previous_presence, c);
}

// The most presence this frame within a texel of `at`: a shape still there, by the jitter.
fn presence_around(at: vec2<i32>) -> f32 {
    return most_around(presence_now, at);
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
