// Full-screen weapon scope overlay composite (scope_view.rs, docs/spec-notes/weapon-scopes.md §3):
// multiplies the post-graded zoomed world by the scope NIF's multiply-blended shapes (such as the
// Red Glare's SuperRedFilter:0, Gauss Rifle's Reticule:2, or Old World Blues' Edge shapes), adds
// its additive shapes (lens glare, OWB's GreenFilter/RedFilter), and lays the opaque mask and
// alpha-blended/tested reticle (premultiplied in over_tex) on top.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;

@group(0) @binding(0) var screen_tex: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var over_tex: texture_2d<f32>;
@group(0) @binding(3) var multiply_tex: texture_2d<f32>;
@group(0) @binding(4) var add_tex: texture_2d<f32>;

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let world = textureSample(screen_tex, screen_sampler, in.uv);
    let mul = textureSample(multiply_tex, screen_sampler, in.uv).rgb;
    let add = textureSample(add_tex, screen_sampler, in.uv).rgb;
    let over = textureSample(over_tex, screen_sampler, in.uv);
    let tinted = max(world.rgb, vec3(0.0)) * mul + add;
    let out_rgb = over.rgb + tinted * (1.0 - over.a);
    return vec4(out_rgb, world.a);
}
