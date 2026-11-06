// A slice of the weapon wheel (docs/design/weapon-wheel.md §7): an annular sector,
// filled and outlined, drawn over a square node whose middle is the wheel's centre.
import bevy_ui_render::ui_vertex_output::UiVertexOutput;

struct Slice {
    fill: vec4<f32>,
    edge: vec4<f32>,
    // From and to angle (radians clockwise from straight up), inner and outer radius
    // (parts of the node's half width).
    shape: vec4<f32>,
    // The outline's width; half the gap to the next slices at the centre, and how much
    // half the gap widens per unit of radius: cut along lines beside the spokes (0, 0:
    // none, the angles alone). Widths are parts of the half width.
    line: vec4<f32>,
}

@group(1) @binding(0)
var<uniform> slice: Slice;

const TAU: f32 = 6.28318530718;

fn wrap(a: f32) -> f32 {
    return a - TAU * floor((a + TAU * 0.5) / TAU);
}

@fragment
fn fragment(in: UiVertexOutput) -> @location(0) vec4<f32> {
    // Nudged off the exact centre, where the angle has no value.
    let p = (in.uv - vec2<f32>(0.5)) * 2.0 + vec2<f32>(1e-4, 0.0);
    let r = length(p);
    let a = atan2(p.x, -p.y);
    let span = slice.shape.y - slice.shape.x;
    let mid = slice.shape.x + span * 0.5;
    // Outside the sector by angle, as a length along the arc; by radius.
    var off_angle = (abs(wrap(a - mid)) - span * 0.5) * r;
    // A gap cut beside the spokes: outside either side's line, each moved into the slice
    // by the half gap at that radius (a slice narrower than half the wheel, so the two
    // lines bound it).
    if (slice.line.y > 0.0 || slice.line.z > 0.0) && span < TAU * 0.5 {
        let from_out = -(p.x * cos(slice.shape.x) + p.y * sin(slice.shape.x));
        let to_out = p.x * cos(slice.shape.y) + p.y * sin(slice.shape.y);
        off_angle = max(from_out, to_out) + slice.line.y + slice.line.z * r;
    }
    // No inner radius (the hub): no inner edge, else its outline drew a dot at the centre.
    let off_radius = select(max(slice.shape.z - r, r - slice.shape.w), r - slice.shape.w, slice.shape.z <= 0.0);
    let d = select(max(off_angle, off_radius), off_radius, span >= TAU - 1e-4);
    let aa = max(fwidth(d), 1e-4);
    let inside = 1.0 - smoothstep(-aa, aa, d);
    let w = slice.line.x;
    let on_edge = 1.0 - smoothstep(w * 0.5 - aa, w * 0.5 + aa, abs(d + w * 0.5));
    let edge = select(on_edge, 0.0, w <= 0.0);
    let c = mix(slice.fill, slice.edge, edge);
    return vec4<f32>(c.rgb, c.a * inside);
}
