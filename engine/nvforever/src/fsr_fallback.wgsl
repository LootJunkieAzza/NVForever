@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<uniform> resolution: vec4<u32>;
@group(0) @binding(2) var linear_sampler: sampler;

@vertex fn vertex(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let p = vec2<f32>(f32((index << 1u) & 2u), f32(index & 2u));
    return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
}

@fragment fn fragment(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let at = clamp(p.xy * vec2<f32>(resolution.xy) / vec2<f32>(resolution.zw), vec2<f32>(0.5), vec2<f32>(resolution.xy) - vec2<f32>(0.5));
    return textureSampleLevel(source, linear_sampler, at / vec2<f32>(textureDimensions(source)), 0.0);
}

// The alpha of the nearest render-resolution texel, written alone over FSR's output.
@fragment fn alpha(@builtin(position) p: vec4<f32>) -> @location(0) vec4<f32> {
    let at = min(vec2<u32>(p.xy * vec2<f32>(resolution.xy) / vec2<f32>(resolution.zw)), resolution.xy - 1u);
    return vec4<f32>(0.0, 0.0, 0.0, textureLoad(source, at, 0).a);
}

// The luminance of FSR's output, at the render size (each texel the output filtered over its
// area), kept for the next frame's reactive mask (fsr_reactive.wgsl `light_change`): what
// FSR's history shows. At least 1e-4, so that 0 means "none" (the texture is cleared when
// FSR did not reconstruct). At the render size it costs a fifth to a ninth of the display
// size's bandwidth; the mask reads it at render-texel spacing anyway.
@fragment fn luma(@builtin(position) p: vec4<f32>) -> @location(0) f32 {
    let c = max(textureSampleLevel(source, linear_sampler, p.xy / vec2<f32>(resolution.xy), 0.0).rgb, vec3<f32>(0.0));
    return max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1.0e-4);
}
