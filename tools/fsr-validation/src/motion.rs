//! Execute Bevy's actual prepass velocity function with known camera/object poses.
use std::path::Path;
use wgpu::{Device, Queue, TextureFormat};

pub fn run(device: &Device, queue: &Queue, path: &Path) {
    let source = std::fs::read_to_string(path).unwrap();
    let function = crate::clusters::function(&source, "calculate_motion_vector");
    let origin = [0.0f32, 0.0, 0.0, 1.0];
    let point = [0.0f32, 0.0, -10.0, 1.0];
    // FOV 90 degrees, aspect 1. At distance 10 an X/Y movement of one
    // world unit spans 0.05 UV; positive world Y is negative screen V.
    let cases = [
        ("static", point, point, origin, origin, [0.0, 0.0]),
        (
            "object right",
            [1.0, 0.0, -10.0, 1.0],
            point,
            origin,
            origin,
            [0.05, 0.0],
        ),
        (
            "object up",
            [0.0, 1.0, -10.0, 1.0],
            point,
            origin,
            origin,
            [0.0, -0.05],
        ),
        (
            "camera right",
            point,
            point,
            [1.0, 0.0, 0.0, 1.0],
            origin,
            [-0.05, 0.0],
        ),
        (
            "camera up",
            point,
            point,
            [0.0, 1.0, 0.0, 1.0],
            origin,
            [0.0, 0.05],
        ),
        (
            "camera/object together",
            [1.0, 0.0, -10.0, 1.0],
            point,
            [1.0, 0.0, 0.0, 1.0],
            origin,
            [0.0, 0.0],
        ),
        (
            "perspective approach",
            [1.0, 0.0, -5.0, 1.0],
            [1.0, 0.0, -10.0, 1.0],
            origin,
            origin,
            [0.05, 0.0],
        ),
        (
            "camera stops",
            point,
            point,
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 1.0],
            [0.0, 0.0],
        ),
    ];
    let vec4 = |v: [f32; 4]| format!("vec4<f32>({}, {}, {}, {})", v[0], v[1], v[2], v[3]);
    let current_points = cases
        .iter()
        .map(|c| vec4(c.1))
        .collect::<Vec<_>>()
        .join(",");
    let previous_points = cases
        .iter()
        .map(|c| vec4(c.2))
        .collect::<Vec<_>>()
        .join(",");
    let current_cameras = cases
        .iter()
        .map(|c| vec4(c.3))
        .collect::<Vec<_>>()
        .join(",");
    let previous_cameras = cases
        .iter()
        .map(|c| vec4(c.4))
        .collect::<Vec<_>>()
        .join(",");
    let n = cases.len();
    let shader = format!(
        r#"
struct View {{ unjittered_clip_from_world: mat4x4<f32> }}
var<private> view: View;
var<private> previous_view_uniforms: View;
const current_points = array<vec4<f32>, {n}>({current_points});
const previous_points = array<vec4<f32>, {n}>({previous_points});
const current_cameras = array<vec4<f32>, {n}>({current_cameras});
const previous_cameras = array<vec4<f32>, {n}>({previous_cameras});
{function}
fn clip_from_camera(camera: vec4<f32>) -> mat4x4<f32> {{
    let projection = mat4x4<f32>(vec4(1.0,0.0,0.0,0.0),vec4(0.0,1.0,0.0,0.0),vec4(0.0,0.0,0.0,-1.0),vec4(0.0,0.0,5.0,0.0));
    let from_world = mat4x4<f32>(vec4(1.0,0.0,0.0,0.0),vec4(0.0,1.0,0.0,0.0),vec4(0.0,0.0,1.0,0.0),vec4(-camera.xyz,1.0));
    return projection * from_world;
}}
@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {{
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}}
@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) vec2<f32> {{
    let i = u32(p.x);
    view.unjittered_clip_from_world = clip_from_camera(current_cameras[i]);
    previous_view_uniforms.unjittered_clip_from_world = clip_from_camera(previous_cameras[i]);
    return calculate_motion_vector(current_points[i], previous_points[i]);
}}
"#
    );
    let pipeline =
        crate::gpu::pipeline(device, &shader, "fragment", Some(TextureFormat::Rg32Float));
    let target = crate::gpu::texture(device, TextureFormat::Rg32Float, [n as u32, 1]);
    let mut encoder = device.create_command_encoder(&Default::default());
    crate::gpu::draw(&mut encoder, &target, &pipeline, None);
    queue.submit([encoder.finish()]);
    let raw = crate::gpu::read(device, queue, &target, 8);
    for (i, case) in cases.iter().enumerate() {
        for axis in 0..2 {
            let at = i * 8 + axis * 4;
            let actual = f32::from_le_bytes(raw[at..at + 4].try_into().unwrap());
            assert!(
                (actual - case.5[axis]).abs() < 0.00001,
                "Bevy motion {} axis {axis}: {actual}, expected {}",
                case.0,
                case.5[axis]
            );
        }
    }
    println!("PASS GPU Bevy motion convention: {n} camera/object/stop cases, unjittered currentUV-previousUV");
}
