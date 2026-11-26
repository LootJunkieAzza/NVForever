import package::fullscreen_vertex_shader::FullscreenVertexOutput;
@if(SRGB_TO_LINEAR) import bevy_render::color_operations::srgb_to_linear;
@if(OKLAB_TO_LINEAR) import bevy_render::color_operations::oklab_to_linear_rgb;

@group(0) @binding(0) var in_texture: texture_2d<f32>;
@group(0) @binding(1) var in_sampler: sampler;
// NVForever HDR output: x the HUD's white in nits over scRGB's 80 (blit/mod.rs).
@group(0) @binding(2) var<uniform> nvf_hdr_scale: vec4<f32>;

@fragment
fn fs_main(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    var color = textureSample(in_texture, in_sampler, in.uv);
    @if(SRGB_TO_LINEAR)
    color = vec4(srgb_to_linear(color.rgb), color.a);
    @if(OKLAB_TO_LINEAR)
    color = vec4(oklab_to_linear_rgb(color.rgb), color.a);
    // NVForever HDR output, gamma: the game was made for SDR screens that decode with gamma
    // 2.2; linear values decoded from sRGB's curve lift the shadows. Within 0..1 the value is
    // encoded with sRGB's curve and decoded with 2.2; above 1 (highlights) it stays.
    @if(NVF_HDR_GAMMA22)
    {
        let c = clamp(color.rgb, vec3(0.0), vec3(1.0));
        let enc = select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3(0.0031308));
        color = vec4(select(color.rgb, pow(enc, vec3(2.2)), color.rgb <= vec3(1.0)), color.a);
    }
    // NVForever HDR output: scRGB's 1.0 is 80 nits; display-referred 1.0 goes to the HUD's white.
    @if(NVF_HDR_PAPER_WHITE)
    color = vec4(color.rgb * nvf_hdr_scale.x, color.a);
    return color;
}
