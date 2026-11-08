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

// The luminance of FSR's output (display size), kept for the next frame's reactive mask
// (fsr_reactive.wgsl `light_change`): what FSR's history shows. At least 1e-4, so that 0
// means "none" (the texture is cleared when FSR did not reconstruct).
@fragment fn luma(@builtin(position) p: vec4<f32>) -> @location(0) f32 {
    let c = max(textureLoad(source, vec2<i32>(p.xy), 0).rgb, vec3<f32>(0.0));
    return max(dot(c, vec3<f32>(0.2126, 0.7152, 0.0722)), 1.0e-4);
}
