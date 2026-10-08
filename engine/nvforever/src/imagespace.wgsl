// Image-space grade (imagespace.rs): saturation, tint, brightness and contrast on the
// display (sRGB) values of the tonemapped picture [hypothesis, see imagespace.rs].

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Grade {
    tint: vec4<f32>,
    saturation: f32,
    contrast: f32,
    contrast_avg_lum: f32,
    brightness: f32,
}

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: Grade;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(screen, screen_sampler, in.uv);
    var c = to_srgb(clamp(src.rgb, vec3(0.0), vec3(1.0)));
    let lum = dot(c, vec3(0.2125, 0.7154, 0.0721));
    c = mix(vec3(lum), c, grade.saturation);
    c = mix(c, lum * grade.tint.rgb, grade.tint.a);
    c = c * grade.brightness;
    c = mix(vec3(grade.contrast_avg_lum), c, grade.contrast);
    return vec4(to_linear(clamp(c, vec3(0.0), vec3(1.0))), src.a);
}
