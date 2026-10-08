// View-angle falloff of BSShaderNoLightingProperty (falloff.rs): the standard material's
// fragment shader, with the alpha multiplied by the falloff opacity for the cosine of the
// angle between the surface normal and the view direction (docs/spec-notes/nif.md §6,
// "View-angle falloff"). Blended materials have no prepass, so only the forward path
// needs it; the prepass entry stays the standard one.

#import bevy_pbr::{
    pbr_fragment::pbr_input_from_standard_material,
    pbr_functions::{alpha_discard, apply_pbr_lighting, main_pass_post_lighting_processing},
    pbr_types::STANDARD_MATERIAL_FLAGS_UNLIT_BIT,
    forward_io::{VertexOutput, FragmentOutput},
}

struct Falloff {
    // x start angle (cosine), y stop angle (cosine), z start opacity, w stop opacity.
    params: vec4<f32>,
}

@group(#{MATERIAL_BIND_GROUP}) @binding(100) var<uniform> falloff: Falloff;

// Linear in the cosine from the start opacity at the start angle to the stop opacity at the
// stop angle, held beyond either end [hypothesis: linear and clamped].
fn falloff_opacity(cos_angle: f32, p: vec4<f32>) -> f32 {
    let t = clamp((cos_angle - p.y) / (p.x - p.y), 0.0, 1.0);
    return mix(p.w, p.z, t);
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    // |N·V|: the normal is already flipped towards the viewer on back faces of double-sided
    // shapes; abs() also covers single-sided ones seen edge-on.
    let cos_angle = abs(dot(normalize(pbr_input.world_normal), pbr_input.V));
    pbr_input.material.base_color.a *= falloff_opacity(cos_angle, falloff.params);
    pbr_input.material.base_color = alpha_discard(pbr_input.material, pbr_input.material.base_color);

    var out: FragmentOutput;
    if (pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_UNLIT_BIT) == 0u {
        out.color = apply_pbr_lighting(pbr_input);
    } else {
        out.color = pbr_input.material.base_color;
    }
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
