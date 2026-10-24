// Sky parts (sky.rs): Bevy's unlit standard fragment, then the colour is mapped so that
// the camera's tonemapper (Tony McMapface) gives back the sky's display value.
//
// The weather's sky colours are display values that the game grades directly; the
// renderer's lit surfaces go through the tonemapper first. TONY_IN / TONY_OUT sample
// the tonemapper's response to grey (input linear, output linear), measured from
// renders of a flat grey sky without the grade [observed: Bevy 0.19.1 Tony McMapface];
// the inverse is piecewise linear between the samples, per channel (Tony McMapface also
// shifts saturated colours, which this ignores).

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, premultiply_alpha},
    forward_io::{VertexOutput, FragmentOutput},
    pbr_types::{STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD},
    mesh_view_bindings::view,
}

// sky.rs SkyLight: daylight on the sky's colours, and the sun.
struct SkyLight {
    // x scale, y 1 additive weighted by texture alpha, z 1 if the light applies to this
    // part, w 2 sun disc and glare, 1 glare.
    params: vec4<f32>,
    glow: vec4<f32>,
    sun_dir: vec4<f32>,
    // rgb colour × strength, w tan(half size)
    sun: vec4<f32>,
    glare: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> sky: SkyLight;
@group(#{MATERIAL_BIND_GROUP}) @binding(101) var sun_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(102) var sun_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(103) var glare_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(104) var glare_sampler: sampler;

// A texture spread over the angular square of half-size atan(half_tan) around the sun
// (as on a billboard facing the camera), added as colour × alpha.
fn sun_texel(t: texture_2d<f32>, s: sampler, dir: vec3<f32>, half_tan: f32) -> vec3<f32> {
    let sd = normalize(sky.sun_dir.xyz);
    let c = dot(dir, sd);
    if c <= 0.0 {
        return vec3(0.0);
    }
    // Point on the plane through the sun's direction, perpendicular to it.
    let p = dir / c - sd;
    var up = vec3(0.0, 1.0, 0.0);
    if abs(sd.y) > 0.999 {
        up = vec3(0.0, 0.0, 1.0);
    }
    let u = normalize(cross(up, sd));
    let v = cross(sd, u);
    let uv = vec2(dot(p, u), -dot(p, v)) / (2.0 * half_tan) + 0.5;
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
        return vec3(0.0);
    }
    let texel = textureSampleLevel(t, s, uv, 0.0);
    return texel.rgb * texel.a;
}

const N: u32 = 24u;
var<private> TONY_IN: array<f32, 24> = array<f32, 24>(0.0, 0.005, 0.01, 0.02, 0.04, 0.07, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0, 1.3, 1.7, 2.2, 3.0, 5.0, 8.0, 16.0, 40.0, 100.0);
var<private> TONY_OUT: array<f32, 24> = array<f32, 24>(0.0, 0.0056, 0.0113, 0.0222, 0.0437, 0.0723, 0.0999, 0.1470, 0.1812, 0.2542, 0.3140, 0.3663, 0.4125, 0.4910, 0.5457, 0.6172, 0.6867, 0.7454, 0.8070, 0.8796, 0.9216, 0.9473, 0.9771, 0.9875);

// Linear input that the tonemapper maps to the linear output `y`.
fn tony_inverse(y: f32) -> f32 {
    for (var i = 1u; i < N; i++) {
        if y <= TONY_OUT[i] {
            let t = (y - TONY_OUT[i - 1u]) / max(TONY_OUT[i] - TONY_OUT[i - 1u], 1e-6);
            return mix(TONY_IN[i - 1u], TONY_IN[i], t);
        }
    }
    return TONY_IN[N - 1u];
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);
    var c = pbr_input.material.base_color;
    if sky.params.z > 0.5 {
        c = vec4(c.rgb * sky.params.x + sky.glow.rgb, c.a);
    }
    if sky.params.w > 0.5 {
        // The sky is centred on the camera: the view ray is the fragment's direction.
        let dir = normalize(in.world_position.xyz - view.world_position.xyz);
        var add = sky.glare.rgb * sun_texel(glare_texture, glare_sampler, dir, sky.glare.w);
        if sky.params.w > 1.5 {
            add += sky.sun.rgb * sun_texel(sun_texture, sun_sampler, dir, sky.sun.w);
        }
        c = vec4(c.rgb + add, c.a);
    }
    var out: FragmentOutput;
    let alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD {
        // Additive parts: map what is added (colour, × alpha when params.y is 1), not the colour.
        let a = c.rgb * select(1.0, c.a, sky.params.y > 0.5);
        out.color = vec4(tony_inverse(a.r), tony_inverse(a.g), tony_inverse(a.b), 1.0);
    } else {
        out.color = vec4(tony_inverse(c.r), tony_inverse(c.g), tony_inverse(c.b), c.a);
    }
#ifdef PREMULTIPLY_ALPHA
    out.color = premultiply_alpha(pbr_input.material.flags, out.color);
#endif
    return out;
}
