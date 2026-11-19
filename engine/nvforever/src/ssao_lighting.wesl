// Ambient occlusion inside the lighting (nvforever ssao_lighting.rs; #2897 stage two,
// docs/design/graphics-enhanced.md §3.1.2): the factor the occlusion pass (ssao.rs,
// ssao.wesl) worked out at half resolution scales the display-value ambient term and the
// light of lamps that cast no shadow (`nvf_lamp_visibility`), never the sun, the moon or a
// highlight. An engineering choice, not the original game's.
//
// Imported by bevy_pbr's `apply_pbr_lighting` (patched in by ssao_lighting.rs `HOOK_EDITS`)
// only with the shader def NVF_AMBIENT_OCCLUSION, which the view's pipelines that write
// depth have exactly when its mesh view bind group holds bindings 43-44 (bevy_pbr
// mesh_view_bindings.rs `NvfAmbientOcclusionBindings`).
//
// Every texture read is textureLoad, no derivatives: the lighting runs in non-uniform
// control flow (per-fragment branches, and callers such as nif_material.wesl
// `with_shade_light`).

struct NvfAmbientOcclusion {
    // x: the exponent the factor is raised to before it scales the display-value sum (1:
    // the shown value falls by the factor itself, as on the picture; ssao_lighting.rs
    // `EXPONENT`); y: 1 to draw the factor alone in place of the lit
    // colour (the debug view); z: the view distance (game units) from which the effect is
    // off (the occlusion pass's fade end, beyond which its texels hold 1); w: the share of
    // the fragment's distance (plus one unit) within which a texel's distance counts as the
    // same surface.
    params: vec4<f32>,
    // x: the strength the factor was made with, for a later multibounce; y: the weight of
    // the factor on the light of lamps that cast no shadow (0..1, ssao_lighting.rs `LAMPS`;
    // 0 leaves them whole); z, w unused.
    extra: vec4<f32>,
}

@group(0) @binding(43) var<uniform> nvf_ao: NvfAmbientOcclusion;
// Half the size of the area the depth prepass covers: texel q stands for the pixel 2q of
// the depth prepass (the same pixel coordinates as a fragment's position); r how open
// (1 open, 0 closed), g that pixel's view distance (game units), 0 where nothing was
// written.
@group(0) @binding(44) var nvf_ao_texture: texture_2d<f32>;

// Below this sum of the weights no texel shares the fragment's surface and the factor is
// 1 [engineering choice: a surface the depth prepass does not hold is never darkened].
const NVF_AO_MIN_WEIGHT: f32 = 0.001;

// Whether the debug view is asked for.
fn nvf_ao_debug() -> bool {
    return nvf_ao.params.y > 0.5;
}

// The factor on the ambient light of the fragment at `frag` (its position, pixel centres at
// .5) and view distance `dist` (game units, from its own world position): the 2 x 2 texels
// around it, each weighed bilinearly and by how near its distance is to `dist`, so a
// fragment never takes the factor of another surface across a depth edge. 1 beyond the
// fade, and where no texel matches.
fn nvf_ambient_visibility(frag: vec2<f32>, dist: f32) -> f32 {
    // `dist > 0.0` is false for NaN too.
    if !(dist > 0.0) || dist >= nvf_ao.params.z {
        return 1.0;
    }
    let size = vec2<i32>(textureDimensions(nvf_ao_texture));
    // In texels: the pixel centred at 2q + 0.5 sits on texel q.
    let t = (frag - 0.5) * 0.5;
    let base = floor(t);
    let f = t - base;
    let first = vec2<i32>(base);
    let tolerance = nvf_ao.params.w * dist + 1.0;
    var total = 0.0;
    var weight = 0.0;
    for (var y = 0; y < 2; y++) {
        for (var x = 0; x < 2; x++) {
            let q = clamp(first + vec2(x, y), vec2(0), size - 1);
            let texel = textureLoad(nvf_ao_texture, q, 0);
            let bilinear = select(1.0 - f.x, f.x, x == 1) * select(1.0 - f.y, f.y, y == 1);
            // A texel with no distance (never written) matches nothing.
            let surface = select(0.0, max(1.0 - abs(texel.g - dist) / tolerance, 0.0), texel.g > 0.0);
            let w = bilinear * surface;
            total += texel.r * w;
            weight += w;
        }
    }
    if !(weight >= NVF_AO_MIN_WEIGHT) {
        return 1.0;
    }
    let open = total / weight;
    // Never NaN nor outside 0..1 (comparisons with NaN are false); kept above 0 for pow.
    let safe = select(1.0, clamp(open, 0.0001, 1.0), open >= 0.0);
    return pow(safe, nvf_ao.params.x);
}

// The factor on the light of lamps (point and spot lights) that cast no shadow at this
// fragment: mix(1, g, w), g the ambient's factor as the caller already has it from
// `nvf_ambient_visibility` (so the texels are read once a fragment; the same display value
// scale as the lamps' part of the sum), w the uniform's weight. Exactly 1 while the weight
// is 0. Never on the directional lights.
fn nvf_lamp_visibility(g: f32) -> f32 {
    let w = clamp(nvf_ao.extra.y, 0.0, 1.0);
    if !(w > 0.0) {
        return 1.0;
    }
    return mix(1.0, g, w);
}
