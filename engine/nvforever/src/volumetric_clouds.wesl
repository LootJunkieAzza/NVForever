// Volumetric clouds (volumetric_clouds.rs; docs/design/graphics-enhanced.md §3.23, a
// prototype): the fragment of the sky proxy (the dome's own mesh, drawn depth-Equal on it).
// The slab is no longer marched here: the volumetric medium's trace pass (medium.rs,
// medium_trace.wesl) marched it before the main pass into the far buffer, and this fragment
// reads that buffer where its own view ray lies (medium_upsample.wesl) and writes it
// premultiplied over the sky dome, the stars and the moon. The colour is the weather's linear
// colours, as sky.wesl's are, mapped through the same inverse of Tony McMapface when the view
// does not show display values clipped, once, here. An engineering choice, not the original
// game's.

import bevy_pbr::render::{forward_io::VertexOutput, mesh_view_bindings::view};
import nvforever::medium_upsample::{medium_screen_uv, medium_far};

// volumetric_clouds.rs CloudComposeUniform: x 1 when the view shows display values clipped
// (no Tony inverse); y 1 to hold (nothing added: the proxy is drawn so that its pipeline is
// made, while the flat layers still show); z, w unused.
struct CloudCompose {
    flags: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> compose: CloudCompose;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var far_light: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var far_sampler: sampler;

// sky.wesl's samples of the tonemapper's response to grey (input linear, output linear).
const N: u32 = 24u;
var<private> TONY_IN: array<f32, 24> = array<f32, 24>(0.0, 0.005, 0.01, 0.02, 0.04, 0.07, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0, 1.3, 1.7, 2.2, 3.0, 5.0, 8.0, 16.0, 40.0, 100.0);
var<private> TONY_OUT: array<f32, 24> = array<f32, 24>(0.0, 0.0056, 0.0113, 0.0222, 0.0437, 0.0723, 0.0999, 0.1470, 0.1812, 0.2542, 0.3140, 0.3663, 0.4125, 0.4910, 0.5457, 0.6172, 0.6867, 0.7454, 0.8070, 0.8796, 0.9216, 0.9473, 0.9771, 0.9875);

// Linear input that the tonemapper maps to the linear output `y` (sky.wesl's, without its
// clip test: the caller makes it).
fn tony_inverse(y: f32) -> f32 {
    for (var i = 1u; i < N; i++) {
        if y <= TONY_OUT[i] {
            let t = (y - TONY_OUT[i - 1u]) / max(TONY_OUT[i] - TONY_OUT[i - 1u], 1e-6);
            return mix(TONY_IN[i - 1u], TONY_IN[i], t);
        }
    }
    return TONY_IN[N - 1u];
}

fn cloud_radiance(colour: vec3<f32>) -> vec3<f32> {
    if compose.flags.x < 0.5 {
        return vec3<f32>(tony_inverse(colour.r), tony_inverse(colour.g), tony_inverse(colour.b));
    }
    return colour;
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    if compose.flags.y > 0.5 {
        return vec4<f32>(0.0);
    }
    let uv = medium_screen_uv(view.unjittered_clip_from_world, in.world_position.xyz);
    var c = medium_far(far_light, far_sampler, uv);
    // Convert to the framebuffer's lighting space before compositing.
    if c.a > 1e-4 {
        c = vec4<f32>(cloud_radiance(c.rgb / c.a) * c.a, c.a);
    }
    return c;
}
