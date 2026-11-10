use std::path::Path;
use wgpu::{Device, Queue, TextureFormat};

// Run the actual vendored cluster selection functions, rather than a second
// implementation of the new viewport mapping. Only their resource namespace is
// replaced with synthetic per-fragment inputs.
pub(crate) fn function(source: &str, name: &str) -> String {
    let start = source.find(&format!("fn {name}(")).unwrap();
    let body = source[start..].find('{').unwrap() + start;
    let mut depth = 0;
    for (offset, c) in source[body..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return source[start..=body + offset].replace("bindings::", "");
                }
            }
            _ => {}
        }
    }
    panic!("unterminated shader function {name}")
}

pub fn run(device: &Device, queue: &Queue, path: &Path) {
    let source = std::fs::read_to_string(path).unwrap();
    let functions = [
        "view_z_to_z_slice",
        "view_fragment_cluster_index",
        "fragment_cluster_index",
    ]
    .map(|name| function(&source, name))
    .join("\n");
    let output = [1919.0f32, 1079.0];
    let mut viewports = Vec::new();
    let mut renderports = Vec::new();
    let mut fragments = Vec::new();
    let mut expected = Vec::new();
    let mut labels = Vec::new();
    for ratio in [1.0f32, 1.5, 1.7, 2.0, 3.0] {
        let render = output.map(|v| (v / ratio).floor());
        for origin in [[0.0f32, 0.0], [73.0, 41.0]] {
            for (y, cy) in [(0.5, 0), (render[1] * 0.5, 4), (render[1] - 0.5, 8)] {
                // One pixel to the right of the centre avoids landing exactly
                // on the 16-column grid's central boundary (division rounding
                // may legitimately select either adjoining cluster there).
                for (x, cx) in [(0.5, 0), (render[0] * 0.5 + 1.0, 8), (render[0] - 0.5, 15)] {
                    viewports.push(format!(
                        "vec4<f32>({}, {}, {}, {})",
                        origin[0], origin[1], output[0], output[1]
                    ));
                    renderports.push(format!(
                        "vec4<f32>({}, {}, {}, {})",
                        origin[0], origin[1], render[0], render[1]
                    ));
                    fragments.push(format!("vec2<f32>({}, {})", origin[0] + x, origin[1] + y));
                    // Reference: where the same normalized position falls in a
                    // native-resolution 16x9 cluster grid. Slice 16 is the fixed
                    // test depth (-50) under the unchanged logarithmic Z mapping.
                    expected.push((cy * 16 + cx) * 24 + 16);
                    labels.push(format!(
                        "ratio {ratio}, origin {origin:?}, pixel [{x}, {y}]"
                    ));
                }
            }
        }
    }
    let n = expected.len();
    let shader = format!(
        r#"
struct View {{ viewport: vec4<f32>, main_pass_viewport: vec4<f32> }}
struct Lights {{ cluster_factors: vec4<f32>, cluster_dimensions: vec4<u32> }}
var<private> view: View;
var<private> lights: Lights;
const viewports = array<vec4<f32>, {n}>({});
const renderports = array<vec4<f32>, {n}>({});
const fragments = array<vec2<f32>, {n}>({});
{functions}
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {{
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}}
@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) u32 {{
    let i = u32(p.x);
    view.viewport = viewports[i];
    view.main_pass_viewport = renderports[i];
    lights.cluster_dimensions = vec4<u32>(16u, 9u, 24u, 3456u);
    lights.cluster_factors = vec4<f32>(vec2<f32>(16.0, 9.0) / view.viewport.zw, 4.0, 0.0);
    return view_fragment_cluster_index(fragments[i], -50.0, false);
}}
"#,
        viewports.join(","),
        renderports.join(","),
        fragments.join(",")
    );
    let pipeline = crate::gpu::pipeline(device, &shader, "fragment", Some(TextureFormat::R32Uint));
    let target = crate::gpu::texture(device, TextureFormat::R32Uint, [n as u32, 1]);
    let mut encoder = device.create_command_encoder(&Default::default());
    crate::gpu::draw(&mut encoder, &target, &pipeline, None);
    queue.submit([encoder.finish()]);
    let actual: Vec<_> = crate::gpu::read(device, queue, &target, 4)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| u32::from_le_bytes(*b))
        .collect();
    for (i, (&actual, &expected)) in actual.iter().zip(&expected).enumerate() {
        assert_eq!(actual, expected, "cluster selection: {}", labels[i]);
    }
    println!("PASS GPU clustered lighting: {n} center/edge/corner cases, five ratios, zero/offset viewports");
}
