// The lit body part of VATS (vats_highlight.rs): a node over the whole view, drawn from
// the mask the part's camera makes. Where the mask is the part's: the fill, in level
// lines. Round it, within the outline's width: the outline, whole.
//
// The outline is not drawn round the mask's own edge, which is the model's triangles' and
// ends in points where two of them meet: the mask is first smoothed over a round brush
// twice the line's width, and the line runs round where half of the brush is the part's.
// So a corner is rounded, a notch narrower than the brush is bridged, a hole smaller than
// it is closed, and a strand thinner than a pixel or two gets no line of its own.
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Mask {
    // The fill's colour and how much of it covers the part.
    fill: vec4<f32>,
    // The outline's colour.
    line: vec4<f32>,
    // A pixel of the mask as a part of it (x, y), the outline's width and the lines'
    // period, both in pixels.
    shape: vec4<f32>,
    // What is left of the fill on a line's faint half; above what the mask is the part's;
    // how much of the fill and the outline is drawn now (the pulse).
    more: vec4<f32>,
}

@group(1) @binding(0)
var<uniform> mask: Mask;
@group(1) @binding(1)
var mask_texture: texture_2d<f32>;
@group(1) @binding(2)
var mask_sampler: sampler;

// How much the mask is the part's at `uv`: 0 to 1 over the mask's own edge.
fn cover(uv: vec2<f32>) -> f32 {
    let v = textureSampleLevel(mask_texture, mask_sampler, uv, 0.0).r;
    return smoothstep(mask.more.y - 0.1, mask.more.y + 0.1, v);
}

// The share of a round brush `radius` pixels about `uv` that is the part's: three rings of
// twelve, each counted by its share of the brush's area.
fn brushed(uv: vec2<f32>, radius: f32) -> f32 {
    let px = mask.shape.xy * radius;
    var sum = cover(uv) * 0.5;
    var total = 0.5;
    for (var ring = 1; ring <= 3; ring = ring + 1) {
        let r = f32(ring) / 3.0;
        for (var k = 0; k < 12; k = k + 1) {
            let a = (f32(k) + f32(ring) * 0.33) * 0.5235988;
            sum = sum + cover(uv + vec2<f32>(cos(a), sin(a)) * px * r) * r;
            total = total + r;
        }
    }
    return sum / total;
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    let width = mask.shape.z;
    let b = brushed(in.uv, width * 2.0);
    // Inside: the part itself, and what the brush closes of it.
    let inside = max(cover(in.uv), smoothstep(0.46, 0.54, b));
    // The outline: outside, as far as the line's width from where half the brush is the
    // part's (a fifth of the brush is, at that distance from a straight edge).
    let edge = smoothstep(0.16, 0.24, b) * (1.0 - inside);
    // The fill's lines, level across the view.
    let row = in.uv.y / mask.shape.y;
    let lines = mix(mask.more.x, 1.0, step(0.5, fract(row / mask.shape.w)));
    let fill = vec4<f32>(mask.fill.rgb, mask.fill.a * lines * inside * mask.more.z);
    return mix(fill, vec4<f32>(mask.line.rgb, mask.more.z), edge);
}
