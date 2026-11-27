// Water surface (WATR), see water.rs for the model and docs/spec-notes/world.md §6 for
// which parts are documented and which are hypotheses.
//
// Bevy world space: y up. Game space (x, y, z) = Bevy (x, -z, y).

import bevy_pbr::{
    render::forward_io::VertexOutput,
    render::mesh_view_bindings::{view, globals, lights, view_transmission_texture, view_transmission_sampler},
    render::pbr_types,
    render::pbr_functions,
    render::view_transformations::{ndc_to_uv, position_ndc_to_world},
};
@if(DEPTH_PREPASS) import bevy_pbr::prepass::utils as prepass_utils;
@if(DEPTH_PREPASS) import bevy_pbr::render::mesh_view_bindings::depth_prepass_texture;
// The sun visibility (nvforever sun_visibility.rs): clouds (and later terrain) on the
// sun's highlight, only while the view has the plumbing.
@if(NVF_SUN_VISIBILITY) import bevy_pbr::render::mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT;
@if(NVF_SUN_VISIBILITY) import nvforever::sun_visibility::nvf_sun_visibility;
import nvforever::caustics::nvf_caustics_filtered;

// During temporal upscaling, the main pass occupies only the render-sized region
// of full-sized attachments. Geometry UVs and texture UVs are distinct.
fn frag_coord_to_uv(fc: vec2<f32>) -> vec2<f32> {
    return (fc - view.main_pass_viewport.xy) / view.main_pass_viewport.zw;
}
fn frag_coord_to_ndc(fc: vec4<f32>) -> vec3<f32> {
    let uv = frag_coord_to_uv(fc.xy);
    return vec3(uv * vec2(2.0, -2.0) + vec2(-1.0, 1.0), fc.z);
}
fn transmission_uv(uv: vec2<f32>) -> vec2<f32> {
    let pixel = view.main_pass_viewport.xy + uv * view.main_pass_viewport.zw;
    let bounded = clamp(pixel, view.main_pass_viewport.xy + 0.5,
        view.main_pass_viewport.xy + view.main_pass_viewport.zw - 0.5);
    return bounded / vec2<f32>(textureDimensions(view_transmission_texture));
}

struct WaterParams {
    shallow: vec4<f32>,
    deep: vec4<f32>,
    // rgb = reflection colour, a = 1 when the type is reflective (reflects the sky).
    reflection: vec4<f32>,
    sky_upper: vec4<f32>,
    sky_horizon: vec4<f32>,
    // near, far, amount, 1 when the amount caps the opacity (placed water)
    above_fog: vec4<f32>,
    // near, far, amount, enhanced depth attenuation enabled.
    under_fog: vec4<f32>,
    // reflectivity, fresnel amount, shininess, wave height (world units)
    surface: vec4<f32>,
    // per layer: uv velocity (per second), 1 / uv scale (per world unit), amplitude
    layers: array<vec4<f32>, 3>,
    // x = Bevy y of the plane the reflection texture shows, y = 1 when it is in use,
    // z = 1 when screen-space reflections stand in where it is not (enhanced graphics)
    reflection_plane: vec4<f32>,
    // ripple grid: game x, y of its corner, units per cell, height scale (0: none)
    ripple: vec4<f32>,
    // shore fade: depth falloff start, end (units of water crossed), 1 when in use
    edge: vec4<f32>,
    // x = counted plane height, y = enable, z/w = low/high halves of the probe generation
    seen_plane: vec4<f32>,
    // xyz = direction toward the sun (Bevy world), w = enabled daylight strength.
    water_sun: vec4<f32>,
    // The F11 window's Water tab (ScreenSpaceShaders' water factors): x the sky
    // reflection's share, y the scene's, z the reflection's vibrance, w the march's steps.
    tuning: vec4<f32>,
    // The Water tab's second row (the mod's `ssfx_water` and `ssfx_water_setup1`): x the
    // reflection's blur, y its blur pattern (glassy patches among blurred ones), z the
    // wave distortion of the reflection, w the wave parallax's height.
    tuning2: vec4<f32>,
    // The screen-space reflection's own row: x its intensity (1: the fresnel's share),
    // y the ray's length in game units, z the thickness's scale, w unused.
    tuning3: vec4<f32>,
};

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> water: WaterParams;
@group(constants::MATERIAL_BIND_GROUP) @binding(1) var noise_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(2) var noise_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(3) var reflection_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(4) var reflection_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(5) var ripple_texture: texture_2d<f32>;
// Reset/tagged before the draw and copied after it (reflection.rs WaterSample).
struct WaterVisibility {
    count: atomic<u32>,
    generation: u32,
    frame: u32,
    time_bits: u32,
};
@group(constants::MATERIAL_BIND_GROUP) @binding(6) var<storage, read_write> water_seen: WaterVisibility;

// Whether opaque surfaces (depth prepass) lie in front of this fragment in every sample
// of its pixel: the depth test would throw the whole pixel away. Reverse z: nearer is
// larger. False without a prepass.
fn hidden_by_scene(fc: vec4<f32>) -> bool {
    @if(DEPTH_PREPASS && MULTISAMPLED) if true {
        let n = textureNumSamples(depth_prepass_texture);
        for (var i = 0u; i < n; i += 1u) {
            if textureLoad(depth_prepass_texture, vec2<i32>(fc.xy), i32(i)) <= fc.z {
                return false;
            }
        }
        return true;
    }
    @if(DEPTH_PREPASS && !MULTISAMPLED) if true {
        return textureLoad(depth_prepass_texture, vec2<i32>(fc.xy), 0) > fc.z;
    }
    return false;
}

// Ripple height at grid position `t` (cells), bilinear (the texture is not filterable).
fn ripple_height(t: vec2<f32>) -> f32 {
    let n = vec2<i32>(textureDimensions(ripple_texture)) - 1;
    let i = vec2<i32>(floor(t));
    let f = t - floor(t);
    let a = textureLoad(ripple_texture, clamp(i, vec2(0), n), 0).r;
    let b = textureLoad(ripple_texture, clamp(i + vec2(1, 0), vec2(0), n), 0).r;
    let c = textureLoad(ripple_texture, clamp(i + vec2(0, 1), vec2(0), n), 0).r;
    let d = textureLoad(ripple_texture, clamp(i + vec2(1, 1), vec2(0), n), 0).r;
    return mix(mix(a, b, f.x), mix(c, d, f.x), f.y);
}

// Slope (game space) of the ripples at game position `g`, faded out towards the grid edge.
fn ripple_slope(g: vec2<f32>) -> vec2<f32> {
    if water.ripple.w <= 0.0 {
        return vec2(0.0);
    }
    let size = f32(textureDimensions(ripple_texture).x);
    let t = (g - water.ripple.xy) / water.ripple.z;
    let edge = min(min(t.x, t.y), min(size - t.x, size - t.y));
    let fade = clamp((edge - 2.0) / 24.0, 0.0, 1.0);
    if fade <= 0.0 {
        return vec2(0.0);
    }
    let dx = ripple_height(t + vec2(1.0, 0.0)) - ripple_height(t - vec2(1.0, 0.0));
    let dy = ripple_height(t + vec2(0.0, 1.0)) - ripple_height(t - vec2(0.0, 1.0));
    return vec2(dx, dy) / (2.0 * water.ripple.z) * water.ripple.w * fade;
}

// Distance along the wavy reflected ray at which the reflection texture is sampled: the
// texture has no depth, so the wave offset is that of a point this far away [tuned by eye].
const REFLECTION_SAMPLE_DISTANCE: f32 = 500.0;

// Refraction: the scene behind the surface is sampled this far (in screen UV per unit of
// view-space normal tilt) from the pixel, growing to that over this much water
// [tuned by eye; the engine's refraction rule is unknown].
const REFRACTION_STRENGTH: f32 = 0.08;
const REFRACTION_DEPTH: f32 = 200.0;

// Fade the wave relief by 25 m and limit the travel along the surface at shallow angles.
const PARALLAX_WAVE_DISTANCE: f32 = 1000.0;
const PARALLAX_WAVE_MAX_TRAVEL: f32 = 8.0;

// Water effects: the bed seen from above takes the water's shallow hue, down to this
// share of its brightness, over this length of ray in the water [design, by eye].
const SHALLOW_TINT_LENGTH: f32 = 90.0;
const SHALLOW_TINT_FLOOR: f32 = 0.62;

// The opaque surface behind screen position `fc` (depth prepass): world position, and
// w = 1 when there is one (0 for the sky or without a prepass).
fn scene_point(fc: vec2<f32>) -> vec4<f32> {
    @if(DEPTH_PREPASS) let d = prepass_utils::prepass_depth(vec4(fc, 0.0, 1.0), 0u);
    @if(DEPTH_PREPASS) if d > 0.0 {
        let ndc = frag_coord_to_ndc(vec4(fc, d, 1.0));
        return vec4(position_ndc_to_world(ndc), 1.0);
    }
    return vec4(0.0);
}

// Screen-space reflections (enhanced graphics, graphics-enhanced.md §3.8): where the
// mirrored camera does not show this plane (water at another height, interiors), the
// reflected ray is marched through the depth prepass and the opaque picture (the
// transmission copy) is taken where it crosses real geometry [documented: the mod's
// `screenspace_water.h`, `SSFX_ssr_water_ray`]. Returns the colour (display values: fogged
// and tonemapped, as the copy holds them) and in w the confidence (0: no valid crossing, so
// the sky reflection stays). Only geometry is a caster: the depth's sky (0) is not, and the
// bed (below the surface) is not; a miss never takes the colour of whatever was last in
// front of the ray, which is what drew horizontal bands when the sky left the screen.
// The least share of the surface a screen-space reflection takes while the row is on
// (water.wesl's fresnel step): the authored reflectivity of a pond is often low, and the
// reflection then read as none at all [engineering choice]. It scales ScreenSpaceShaders'
// water fresnel, (1 - 2 cos²)³, which is strong at the grazing looks water is seen at and
// nothing straight down; their `G_SSR_WATER_REFLECTION` default is 0.8.
const SSR_REFLECTION_FLOOR: f32 = 0.8;
// Whether the depth buffer's surface at screen UV `uv` (raw depth `depth`) lies above the
// water's surface at `level`. The water is not in the depth prepass, so the bed sits right
// behind every water pixel; a grazing reflected ray rises above the plane, and an earlier
// test of the ray's own height at the bed's depth then let the bed through as a caster —
// its grazing crossings drew the repeating horizontal bands when the reflected sky left the
// screen (the owner's screenshot, 2026-11-25). The scene point itself must be above water.
fn above_surface(uv: vec2<f32>, depth: f32, level: f32) -> bool {
    let fc = view.main_pass_viewport.xy + uv * view.main_pass_viewport.zw;
    let ndc = frag_coord_to_ndc(vec4<f32>(fc, depth, 1.0));
    return position_ndc_to_world(ndc).y > level + 0.5;
}

@if(DEPTH_PREPASS) fn scene_depth(uv: vec2<f32>) -> f32 {
    let px = view.main_pass_viewport.xy + uv * view.main_pass_viewport.zw;
    return prepass_utils::prepass_depth(vec4<f32>(px, 0.0, 1.0), 0u);
}

// The march steps in screen space, as theirs: the start and the end are projected once and
// every step is a UV offset, the ray's distance found by the reciprocal interpolation of
// its ends, so a step costs one depth load and no matrix [documented: the mod's
// `screenspace_water.h`, `SSFX_ssr_water_ray`]. The length is the Water tab's row (150 m by
// default, their water ray); the march refines each crossing with a short bisection
// (`q_steps`) so the hit does not land on the coarse step grid.

// Use a filtered wave normal for the reflection ray. Keeping the tilt small lets the
// reflection follow the waves without making grazing rays swing below the surface.
const SSR_WAVE_TILT: f32 = 0.25;

// The reciprocal-interpolated distance along the view axis of the ray at screen UV `p`,
// the same value the march compares to the depth prepass [documented: the mod's
// `SSFX_ray_intersect`]. The refinement bisects with it.
@if(DEPTH_PREPASS) fn ssr_ray_z(p: vec2<f32>, uv0: vec2<f32>, uv1: vec2<f32>, z0: f32, z1: f32) -> f32 {
    let total = max(length(uv1 - uv0), 1e-5);
    let alpha = clamp(length(p - uv0) / total, 0.0, 1.0);
    return 1.0 / mix(1.0 / z0, 1.0 / z1, alpha);
}

fn ssr(pos: vec3<f32>, R: vec3<f32>, variation: f32) -> vec4<f32> {
    let to_eye = normalize(pos - view.world_position.xyz);
    // A reflected ray that points back at the eye leaves the picture at once (the mod's
    // `edgeFade`).
    if R.y <= 0.0 || dot(R, to_eye) < -0.1 {
        return vec4(0.0);
    }
    @if(DEPTH_PREPASS) let o = (view.view_from_world * vec4(pos, 1.0)).xyz;
    @if(DEPTH_PREPASS) let d = normalize((view.view_from_world * vec4(R, 0.0)).xyz);
    @if(DEPTH_PREPASS) let steps = u32(clamp(water.tuning.w, 8.0, 64.0));
    // The crossing is bisected until its depth error is small: with one or two halves a
// grazing surface's depth changes too much across a step for the residual to fit the
// tolerance, and the rejected crossings left a screen-door pattern of sky and object
// (the owner's pond looking level, 2026-11-25). Four halves (five at the low profile) put
// the residual well inside it.
    @if(DEPTH_PREPASS) var refine = 4u;
    @if(DEPTH_PREPASS) if steps <= 16u {
        refine = 5u;
    }
    @if(DEPTH_PREPASS) let near = view.clip_from_view[3][2];
    @if(DEPTH_PREPASS) let reach = clamp(water.tuning3.y, 800.0, 20000.0);
    @if(DEPTH_PREPASS) let end = o + d * reach;
    @if(DEPTH_PREPASS) let clip0 = view.clip_from_view * vec4(o, 1.0);
    @if(DEPTH_PREPASS) let clip1 = view.clip_from_view * vec4(end, 1.0);
    @if(DEPTH_PREPASS) let uv0 = ndc_to_uv(clip0.xy / clip0.w);
    @if(DEPTH_PREPASS) let uv1 = ndc_to_uv(clip1.xy / clip1.w);
    @if(DEPTH_PREPASS) let z0 = -o.z;
    @if(DEPTH_PREPASS) let z1 = -end.z;
    @if(DEPTH_PREPASS) let step_uv = (uv1 - uv0) / f32(steps);
    // The assumed thickness of what the depth holds, 1 to 3 m as theirs, in game units,
    // scaled by the Water tab's row.
    @if(DEPTH_PREPASS) let thick = clamp(1920.0 / f32(steps), 40.0, 120.0) * clamp(water.tuning3.z, 0.25, 4.0);
    // No jitter of the first step: with the bisection the hit no longer lands on the step
    // grid, and a fixed per-pixel offset without a history to average it only dithered the
    // hit/miss boundary into a dot pattern (the owner's pond fence, 2026-11-25).
    @if(DEPTH_PREPASS) var cur = uv0 + step_uv;
    @if(DEPTH_PREPASS) var prev_uv = uv0;
    @if(DEPTH_PREPASS) var hit = false;
    @if(DEPTH_PREPASS) var hit_uv = vec2<f32>(0.0);
    @if(DEPTH_PREPASS) var hit_err = 1.0e9;
    @if(DEPTH_PREPASS) var hit_depth = 0.0;
    @if(DEPTH_PREPASS) for (var i = 1u; i <= steps; i += 1u) {
        if cur.x <= 0.0 || cur.x >= 1.0 || cur.y <= 0.0 || cur.y >= 1.0 {
            break;
        }
        let alpha = f32(i) / f32(steps);
        let ray_z = 1.0 / mix(1.0 / z0, 1.0 / z1, alpha);
        let scene = scene_depth(cur);
        // The sky is no caster; a surface at or below the water is the bed, not a reflection.
        if scene > 0.0 && above_surface(cur, scene, pos.y) {
            let scene_z = near / scene;
            let diff = ray_z - scene_z;
            if diff > 0.0 {
                // Binary refinement between the last point in front and this one behind,
                // always: the step that first crosses a surface is already within the
                // thickness tolerance for near geometry, and taking that step's UV put the
                // hit on the march's coarse grid — a palisade fence reflected through it
                // read as a lattice of dashes (the owner's screenshots, 2026-11-25). The
                // thickness now gates the refined crossing: a depth discontinuity the
                // bisection cannot resolve is not a surface.
                var lo = prev_uv;
                var hi = cur;
                for (var k = 0u; k < refine; k += 1u) {
                    let mid = mix(lo, hi, 0.5);
                    let mid_scene = scene_depth(mid);
                    var front = true;
                    if mid_scene > 0.0 && above_surface(mid, mid_scene, pos.y) {
                        front = ssr_ray_z(mid, uv0, uv1, z0, z1) <= near / mid_scene;
                    }
                    if front {
                        lo = mid;
                    } else {
                        hi = mid;
                    }
                }
                let hit_scene = scene_depth(hi);
                if hit_scene > 0.0 && above_surface(hi, hit_scene, pos.y) {
                    let hit_z = near / hit_scene;
                    let err = abs(ssr_ray_z(hi, uv0, uv1, z0, z1) - hit_z);
                    // A crossing the bisection cannot bring within the tolerance is a
                    // depth discontinuity, not a surface. The tolerance grows a little with
                    // the distance the crossing lies at, where a step spans more depth.
                    let tol = thick * (1.0 + min(hit_z / 3000.0, 1.0));
                    if err <= tol {
                        hit = true;
                        hit_uv = hi;
                        hit_err = err;
                        hit_depth = hit_z;
                        break;
                    }
                }
            }
        }
        prev_uv = cur;
        cur += step_uv;
    }
    // No `behind_hit`: a miss is a miss. Reusing the last point in front of something as a
    // reflection is what drew the horizontal bands — the sky was not a caster there, so the
    // march kept the colour of whatever stood in front at that step, quantised to the step
    // grid (the owner's screenshot of the water looking down, 2026-11-25).
    @if(DEPTH_PREPASS) if !hit {
        return vec4(0.0);
    }
    // The depth and colour reads must address the same texel. Linear colour sampling
    // at a valid hit can still pick up the bright sky across a tree silhouette.
    @if(DEPTH_PREPASS) let viewport = view.main_pass_viewport;
    @if(DEPTH_PREPASS) let pixel = vec2<i32>(viewport.xy + hit_uv * viewport.zw);
    @if(DEPTH_PREPASS) let tol = thick * (1.0 + min(hit_depth / 3000.0, 1.0));
    @if(DEPTH_PREPASS) let far = mix(1.0, 1.6, smoothstep(800.0, 4000.0, hit_depth));
    @if(DEPTH_PREPASS) let soft = clamp(water.tuning2.x * (1.0 - 0.7 * clamp(variation, 0.0, 1.0) * water.tuning2.y), 0.0, 1.0);
    @if(DEPTH_PREPASS) let radius = (1.0 + 8.0 * soft) * far;
    @if(DEPTH_PREPASS) var sum = textureLoad(view_transmission_texture, pixel, 0).rgb;
    @if(DEPTH_PREPASS) var weight_sum = 1.0;
    @if(DEPTH_PREPASS) for (var i = 0u; i < 8u; i += 1u) {
        let angle = f32(i) * 0.78539816;
        let offset = vec2<i32>(round(vec2(cos(angle), sin(angle)) * radius));
        let tap = pixel + offset;
        let uv = (vec2<f32>(tap) + 0.5 - viewport.xy) / viewport.zw;
        if all(uv >= vec2(0.0)) && all(uv < vec2(1.0)) {
            let depth = scene_depth(uv);
            if depth > 0.0 {
                let depth_delta = abs(near / depth - hit_depth);
                let weight = 1.0 - smoothstep(tol, 2.0 * tol, depth_delta);
                if weight > 0.0 && above_surface(uv, depth, pos.y) {
                    sum += weight * textureLoad(view_transmission_texture, tap, 0).rgb;
                    weight_sum += weight;
                }
            }
        }
    }
    // How much the found geometry may replace the sky reflection: a sharp fade at the
    // picture's border (where the copy has no neighbours to sample), a fit by the depth
    // error of the crossing, and a mild fade for hits near the ray's end where screen-space
    // accuracy is poorest. Zero anywhere means the sky stays (no bands, no black).
    @if(DEPTH_PREPASS) let edge = min(min(hit_uv.x, 1.0 - hit_uv.x), min(hit_uv.y, 1.0 - hit_uv.y));
    @if(DEPTH_PREPASS) let edge_conf = smoothstep(0.0, 0.06, edge);
    // Only a hit close to the tolerance is uncertain; accepted crossings sit far inside it,
    // and a continuous confidence across the whole range dithered sky and object per pixel.
    @if(DEPTH_PREPASS) let fit_conf = 1.0 - smoothstep(tol * 0.9, tol, hit_err);
    @if(DEPTH_PREPASS) let travelled = (hit_depth - z0) / max(-d.z, 1e-4);
    @if(DEPTH_PREPASS) let dist_conf = 1.0 - smoothstep(reach * 0.75, reach, travelled);
    @if(DEPTH_PREPASS) return vec4(sum / weight_sum, edge_conf * fit_conf * dist_conf);
    @if(!DEPTH_PREPASS) return vec4(0.0);
}
// Slope (dh/dx, dh/dy in game space) of one noise layer: height = texture channel `mask`
// (0..1) times the layer amplitude times the wave height.
fn layer_slope(g: vec2<f32>, layer: vec4<f32>, mask: vec4<f32>, t: f32, grad_x: vec2<f32>, grad_y: vec2<f32>) -> vec2<f32> {
    let uv = g * layer.z + layer.xy * t;
    // The slope from three taps round the point, a texel apart near the camera and a
    // pixel's footprint apart beyond (a single texel's step there moved a sharp
    // reflection several pixels as the waves travelled). A wider span was tried and
    // flattened the short waves: two texels each side keeps 0.64 of an 8-texel wave.
    let dx = grad_x * layer.z;
    let dy = grad_y * layer.z;
    let e = max(1.0 / f32(textureDimensions(noise_texture).x), 0.5 * max(length(dx), length(dy)));
    let a = vec2(1.0, 0.0);
    let b = vec2(-0.5, 0.8660254);
    let c = vec2(-0.5, -0.8660254);
    let ha = dot(textureSampleGrad(noise_texture, noise_sampler, uv + e * a, dx, dy), mask);
    let hb = dot(textureSampleGrad(noise_texture, noise_sampler, uv + e * b, dx, dy), mask);
    let hc = dot(textureSampleGrad(noise_texture, noise_sampler, uv + e * c, dx, dy), mask);
    // d(uv)/d(world) = layer.z
    return (2.0 / (3.0 * e)) * (ha * a + hb * b + hc * c) * layer.z * layer.w * water.surface.w;
}

fn wave_relief_depth(g: vec2<f32>, t: f32, grad_x: vec2<f32>, grad_y: vec2<f32>, weights: vec3<f32>) -> f32 {
    let a = water.layers[0];
    let b = water.layers[1];
    let c = water.layers[2];
    let h0 = textureSampleGrad(noise_texture, noise_sampler, g * a.z + a.xy * t, grad_x * a.z, grad_y * a.z).r;
    let h1 = textureSampleGrad(noise_texture, noise_sampler, g * b.z + b.xy * t, grad_x * b.z, grad_y * b.z).g;
    let h2 = textureSampleGrad(noise_texture, noise_sampler, g * c.z + c.xy * t, grad_x * c.z, grad_y * c.z).b;
    return clamp(0.5 - dot(vec3(h0, h1, h2) - 0.5, weights), 0.0, 1.0);
}

// Trace from the top of a centred height field to its first crossing, then interpolate
// between the two samples around it (RC6 Water_DoParallax). All offsets are in game XY.
fn parallax_wave_coord(g: vec2<f32>, t: f32, V: vec3<f32>, dist: f32, grad_x: vec2<f32>, grad_y: vec2<f32>) -> vec2<f32> {
    let fade = (1.0 - smoothstep(0.5 * PARALLAX_WAVE_DISTANCE, PARALLAX_WAVE_DISTANCE, dist))
        * smoothstep(0.06, 0.25, V.y);
    let amplitudes = vec3(water.layers[0].w, water.layers[1].w, water.layers[2].w);
    let amplitude = dot(abs(amplitudes), vec3(1.0));
    let height = min(abs(water.surface.w) * amplitude * water.tuning2.w, PARALLAX_WAVE_MAX_TRAVEL);
    let view_xy = vec2(V.x, -V.z);
    let travel = fade * height * view_xy / max(V.y, 0.25);
    let offset = travel * min(1.0, PARALLAX_WAVE_MAX_TRAVEL / max(length(travel), 1e-4));
    if length(offset) < 0.01 {
        return g;
    }
    let weights = amplitudes / max(amplitude, 1e-5);
    let start = g + 0.5 * offset;
    let steps = u32(mix(8.0, 4.0, clamp(V.y, 0.0, 1.0)));
    var previous = wave_relief_depth(start, t, grad_x, grad_y, weights);
    for (var i = 1u; i <= steps; i += 1u) {
        let depth = f32(i) / f32(steps);
        let coord = start - offset * depth;
        let gap = wave_relief_depth(coord, t, grad_x, grad_y, weights) - depth;
        if gap <= 0.0 {
            let fraction = previous / max(previous - gap, 1e-5);
            return coord + offset * ((1.0 - fraction) / f32(steps));
        }
        previous = gap;
    }
    return g;
}

fn sky_colour(y: f32) -> vec3<f32> {
    // The same gradient as the sky dome above the horizon (sky.rs).
    return mix(water.sky_horizon.rgb, water.sky_upper.rgb, sqrt(clamp(y / 0.5, 0.0, 1.0)));
}

fn reflection_vibrance(colour: vec3<f32>) -> vec3<f32> {
    let lum = dot(colour, vec3<f32>(0.2126, 0.7152, 0.0722));
    return mix(vec3<f32>(lum), colour, 1.0 + water.tuning.z);
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let pos = in.world_position.xyz;
    let g = vec2(pos.x, -pos.z);
    let grad_x = dpdx(g);
    let grad_y = dpdy(g);
    // The counter's writes turn off the GPU's early depth test, so water behind the
    // scene is thrown away here, before the costly part, as the depth test would. What
    // is left counts, at the reflected height, one pixel in 16 (#1060).
    //
    // Keep the texture sampling below in helper functions (layer_slope and the rest):
    // naga checks uniform control flow per function, and a textureSample written
    // directly in fragment() after this discard would fail validation, so water would
    // stop drawing.
    if hidden_by_scene(in.position) {
        discard;
    }
    // Keep the cost at one candidate pixel in 16 per frame. Rotate the grid so a
    // thin strip of water cannot stay between the same four columns or rows forever.
    let grid = vec2<u32>(in.position.xy) & vec2(3u);
    let phase = vec2(globals.frame_count & 3u, (globals.frame_count >> 2u) & 3u);
    if water.seen_plane.y > 0.5 && abs(pos.y - water.seen_plane.x) < 2.0 && all(grid == phase) {
        let generation = u32(water.seen_plane.z) | (u32(water.seen_plane.w) << 16u);
        // A material awaiting GPU preparation can still have the previous probe's uniform.
        if generation == water_seen.generation {
            atomicAdd(&water_seen.count, 1u);
        }
    }
    let to_cam = view.world_position.xyz - pos;
    let dist = max(length(to_cam), 1e-3);
    let V = to_cam / dist;
    let above = V.y >= 0.0;

    let t = globals.time;
    // Keep the unshifted texture footprint across relief intersections and the ripples
    // anchored to their simulated positions.
    var g_parallax = g;
    if above && water.tuning2.w > 0.0 && dist < PARALLAX_WAVE_DISTANCE {
        g_parallax = parallax_wave_coord(g, t, V, dist, grad_x, grad_y);
    }
    var s = layer_slope(g_parallax, water.layers[0], vec4(1.0, 0.0, 0.0, 0.0), t, grad_x, grad_y);
    s += layer_slope(g_parallax, water.layers[1], vec4(0.0, 1.0, 0.0, 0.0), t, grad_x, grad_y);
    s += layer_slope(g_parallax, water.layers[2], vec4(0.0, 0.0, 1.0, 0.0), t, grad_x, grad_y);
    s += ripple_slope(g);
    // The mod's `water_perlin`: a slow noise that quiets the waves in patches and keeps
    // other patches glassy (`Navg *= saturate(0.3 + water_perlin)`), reused for the
    // reflection's blur.
    let variation = textureSampleLevel(noise_texture, noise_sampler, g * 0.0016 + t * 0.004, 0.0).r;
    s *= clamp(0.3 + variation, 0.0, 1.0);
    // Game-space normal (-sx, -sy, 1) in Bevy space.
    var N = normalize(vec3(-s.x, 1.0, s.y));
    var up = vec3(0.0, 1.0, 0.0);
    if !above {
        N = -N;
        up = -up;
    }

    // The length of the view ray inside the water: from above, from the surface to the
    // opaque surface behind it (depth prepass), so 0 at the waterline; from below, from
    // the camera to the surface. It sets the body's opacity, the refraction and the shore
    // fade.
    //
    // The fog depth: from above, how far below the surface that opaque surface lies
    // (the above fog's near and far planes are "the distance in game units below the
    // surface of the water" of the shallow and the deep colour plane [documented: GECK
    // wiki WaterType]); it sets the body's colour, so a shallow pool keeps its shallow
    // colour at a grazing look;
    // from below, the distance from the camera (the under fog's planes are distances
    // "from the player" [documented: same page]). Nothing behind: the far end of both.
    let size = view.main_pass_viewport.zw;
    let uv0 = frag_coord_to_uv(in.position.xy);
    let behind0 = scene_point(in.position.xy);
    var receiver = behind0;
    var fog_distance = 1.0e7;
    var fog_depth = 1.0e7;
    if behind0.w > 0.5 {
        fog_distance = distance(behind0.xyz, pos);
        fog_depth = max(pos.y - behind0.y, 0.0);
    }
    // Refraction: shift the sample by the wave normal's tilt in view space, more through
    // deeper water; keep the unshifted sample where the shifted one lands on something in
    // front of the water (it would bleed into the surface).
    let tilt = (view.view_from_world * vec4(N - up, 0.0)).xy;
    let depth_scale = clamp(min(fog_distance, dist) / REFRACTION_DEPTH, 0.0, 1.0);
    let half_pixel = 0.5 / size;
    var uv = clamp(uv0 + vec2(tilt.x, -tilt.y) * REFRACTION_STRENGTH * depth_scale, half_pixel, 1.0 - half_pixel);
    if above {
        var behind = scene_point(view.main_pass_viewport.xy + uv * size);
        // The offset pixel has its own view ray. Test against the water plane
        // there. Comparing camera distances on two different rays can refract
        // a whole patch of dry shore or a building across the surface.
        if behind.w < 0.5 || behind.y >= pos.y {
            // Near silhouettes the waves repeatedly carry an offset across the
            // edge. Keep the last underwater sample along the offset instead of
            // switching the whole lookup back to its undistorted starting point.
            var outside = uv;
            uv = uv0;
            behind = behind0;
            if behind0.w > 0.5 && behind0.y < pos.y {
                for (var i = 0; i < 3; i += 1) {
                    let mid = 0.5 * (uv + outside);
                    let hit = scene_point(view.main_pass_viewport.xy + mid * size);
                    if hit.w > 0.5 && hit.y < pos.y {
                        uv = mid;
                        behind = hit;
                    } else {
                        outside = mid;
                    }
                }
            }
        }
        if behind.w > 0.5 && behind.y < pos.y {
            receiver = behind;
            // Length below the plane on that ray, from its own entry point.
            fog_depth = pos.y - behind.y;
            let to_receiver = behind.xyz - view.world_position.xyz;
            fog_distance = length(to_receiver) * (fog_depth / max(-to_receiver.y, 1e-3));
        }
    }
    if !above {
        fog_distance = dist;
        fog_depth = dist;
    }
    var refracted = textureSampleLevel(view_transmission_texture, view_transmission_sampler, transmission_uv(uv), 0.0).rgb;
    // Brighten the actual refracted opaque receiver, including non-reflective
    // water types. The existing body opacity/reflection then attenuates the light.
    if above && receiver.w > 0.5 && water.water_sun.w > 0.0 && dist < 4096.0 {
        // The projected light loses definition with viewing distance.
        let fade = 1.0 - smoothstep(2048.0, 4096.0, dist);
        // A pixel's width on the bed, stretched where the bed is seen at a slant (at most
        // six times): waves finer than that fade out instead of sparkling.
        let to_bed = receiver.xyz - view.world_position.xyz;
        let bed_distance = max(length(to_bed), 0.001);
        let footprint = 2.0 * bed_distance / (view.clip_from_view[1][1] * size.y) / max(abs(to_bed.y) / bed_distance, 1.0 / 6.0);
        refracted *= 1.0 + fade * nvf_caustics_filtered(receiver.xyz, pos.y, water.water_sun.xyz, water.water_sun.w, t, footprint);
    }
    let fog = select(water.under_fog, water.above_fog, above);
    // The colour: shallow to deep by the fog depth (#1787). The opacity: by the length of
    // the view ray in the water over the same near to far ramp, at most the above fog's
    // amount for placed water ("controls the transparency of the Deep Color water plane",
    // 1.0 opaque [documented: GECK wiki WaterType]; above_fog.w = 1). Cell and distant
    // water, and any water seen from below, are opaque beyond the far distance: capped,
    // Lake Mead's bed showed through far out [design, until a far-out Lake Mead
    // comparison is made, world.md §6].
    // [hypothesis, checked against the original at the Ultra-Luxe fountain, the
    // Goodsprings Source trough and Boulder Beach, world.md §6]: by the depth alone a
    // 14-unit pool under a 36-unit fog showed its floor where the original is opaque.
    let depth01 = clamp((fog_depth - fog.x) / max(fog.y - fog.x, 1.0), 0.0, 1.0);
    let ray01 = clamp((fog_distance - fog.x) / max(fog.y - fog.x, 1.0), 0.0, 1.0);
    // As the bed descends, scatter more light along the submerged part of the
    // ray. Preserve the shallow end, the authored far distance and PWAT opacity cap.
    var opacity = ray01;
    if above && water.under_fog.w > 0.5 && fog.y > max(fog.x, 0.0) {
        let column = smoothstep(max(fog.x, 0.0), fog.y, fog_depth);
        opacity += (1.0 - ray01) * ray01 * column;
        // The first stretch of water [design, #2674]: by the ramp alone a wading
        // depth of NVCleanWater is under a fifth opaque and the shallows cannot be
        // told from the dry shore. The bed seen through the water takes the water's
        // own shallow hue and darkens over SHALLOW_TINT_LENGTH of ray, and the body
        // gathers along the ramp twice as fast at its thin end (the same far end).
        opacity = 1.0 - (1.0 - opacity) * (1.0 - ray01);
        let hue = water.shallow.rgb / max(max(water.shallow.r, max(water.shallow.g, water.shallow.b)), 1e-3);
        let through = exp(-max(fog_distance, 0.0) / SHALLOW_TINT_LENGTH);
        refracted *= mix(hue * SHALLOW_TINT_FLOOR, vec3(1.0), through);
    }
    let body_alpha = select(opacity, opacity * clamp(water.above_fog.z, 0.0, 1.0), above && water.above_fog.w > 0.5);
    let body_colour = mix(water.shallow.rgb, water.deep.rgb, depth01);

    // The water body is lit like a diffuse surface facing up (from either side).
    var pbr = pbr_types::pbr_input_new();
    pbr.material.base_color = vec4(body_colour, 1.0);
    pbr.material.perceptual_roughness = 1.0;
    pbr.material.metallic = 0.0;
    pbr.material.reflectance = vec3(0.0);
    pbr.frag_coord = in.position;
    pbr.world_position = in.world_position;
    pbr.world_normal = vec3(0.0, 1.0, 0.0);
    pbr.N = vec3(0.0, 1.0, 0.0);
    pbr.V = V;
    pbr.is_orthographic = view.clip_from_view[3].w == 1.0;
    let lit = pbr_functions::apply_pbr_lighting(pbr).rgb;

    // Reflection: the world (reflective types: the mirrored camera's texture when it shows
    // this plane, else the sky gradient) or the reflection colour, weighted by a Schlick
    // fresnel term blended with a constant by the fresnel amount.
    var fr = 0.0;
    var reflected = vec3(0.0);
    // Screen-space reflection where the mirror camera does not cover (display values).
    var traced = vec4(0.0);
    var fallback = vec3(0.0);
    if above {
        let R = reflect(-V, N);
        let cos_v = clamp(dot(N, V), 0.0, 1.0);
        let schlick = 0.02 + 0.98 * pow(1.0 - cos_v, 5.0);
        fr = clamp(water.surface.x * mix(1.0, schlick, water.surface.y), 0.0, 1.0);
        // Screen-space reflections are our enhancement, and the authored reflectivity made
        // them a faint wash (this pond's is 0.23): while the row is on they take at least
        // their default share, the way ScreenSpaceShaders' water shows what it reflects
        // [engineering choice, the owner's note of 2026-11-25]. The share keeps the mod's
        // angle dependence: (1 - 2 cos²)³, none straight down, most at a grazing look.
        let grazing = pow(clamp(1.0 - 2.0 * cos_v * cos_v, 0.0, 1.0), 3.0);
        if water.reflection_plane.z > 0.5 {
            fr = max(fr, SSR_REFLECTION_FLOOR * grazing);
        }
        // Sampled unconditionally (uniform control flow), used when it shows this plane.
        // The mirror image of the wavy reflected ray's point, seen from the camera; for a
        // flat surface that is this very pixel. Projected without the view's temporal
        // jitter (FSR, TAA): the reflection camera draws without it, so the jittered
        // projection put each lookup up to half a render pixel off, differently in every
        // frame, and the reflections wobbled. Without jitter the two are the same.
        let mirrored = pos + vec3(R.x, -R.y, R.z) * REFLECTION_SAMPLE_DISTANCE;
        let clip = view.unjittered_clip_from_world * vec4(mirrored, 1.0);
        var uv = frag_coord_to_uv(in.position.xy);
        if clip.w > 0.0 {
            uv = ndc_to_uv(clip.xy / clip.w);
        }
        let world_reflection = textureSampleLevel(reflection_texture, reflection_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
        let mirror_here = water.reflection_plane.y > 0.5 && abs(pos.y - water.reflection_plane.x) < 2.0;
        // Screen-space reflections are our enhancement, and a type the game authors without
        // the "Reflects" flag (a pond by Nipton among them) then reflects as every other
        // water does: `reflection_plane.w` carries the switch's state [design, the owner's
        // run of 2026-11-25: no reflection showed at all on such a pond].
        let reflective = water.reflection.w > 0.5 || water.reflection_plane.w > 0.5;
        let screen_space = !mirror_here && water.reflection_plane.z > 0.5 && reflective;
        var sky_ray = R;
        if screen_space {
            // RC6 warps its combined reflection buffer in the water pass. Here the
            // ray mask and both colours follow the same softened wave direction.
            let wave_normal = normalize(mix(up, N, SSR_WAVE_TILT * water.tuning2.z));
            sky_ray = reflect(-V, wave_normal);
            traced = ssr(pos, sky_ray, variation);
        }
        let sky = select(sky_colour(sky_ray.y) * water.tuning.x, world_reflection * water.tuning.y, mirror_here);
        // ScreenSpaceShaders' water `G_SSR_WATER_VIBRANCE`: the reflection takes back some of
        // the saturation the sky and the fog wash out of it. Apply it to both sources.
        fallback = reflection_vibrance(select(water.reflection.rgb, sky, reflective));
        reflected = fallback;
        if lights.n_directional_lights > 0u {
            // The sun's highlight: directional light 0, or with the sun visibility's
            // plumbing the light flagged as the outdoor sun (none flagged indoors: 0, as
            // without it).
            var sun_index = 0u;
            @if(NVF_SUN_VISIBILITY)
            for (var i = 0u; i < lights.n_directional_lights; i += 1u) {
                if (lights.directional_lights[i].flags & DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT) != 0u {
                    sun_index = i;
                    break;
                }
            }
            let sun = lights.directional_lights[sun_index];
            let spec = pow(max(dot(R, sun.direction_to_light), 0.0), max(water.surface.z, 1.0));
            // In a world view the sun's colour holds display values (nvforever lights.rs
            // DISPLAY_HELPERS): its light is that value raised to the display gamma.
            let sun_light = select(sun.color.rgb * view.exposure, 3.14159265 * pow(max(sun.color.rgb * view.exposure / 3.14159265, vec3(0.0)), vec3(2.2)), lights.ambient_color.a < 0.0);
            // The highlight takes the sun visibility once (no cascade shadow: water casts
            // and receives none, C = 1); the reflection image is left as it is.
            var sun_visibility = 1.0;
            @if(NVF_SUN_VISIBILITY)
            if (sun.flags & DIRECTIONAL_LIGHT_FLAGS_NVF_OUTDOOR_SUN_BIT) != 0u {
                let view_z = dot(vec4<f32>(view.view_from_world[0].z, view.view_from_world[1].z, view.view_from_world[2].z, view.view_from_world[3].z), vec4<f32>(pos, 1.0));
                sun_visibility = nvf_sun_visibility(pos, sun.direction_to_light, 1.0, view_z);
            }
            reflected += spec * sun_light * sun_visibility;
        }
    } else {
        // From below, the sky shows only within Snell's window (refractive index 1.33:
        // cos > 0.66 from the vertical); outside it the surface reflects the water body.
        let c = clamp(dot(N, V), 0.0, 1.0);
        fr = 1.0 - smoothstep(0.62, 0.70, c);
        reflected = lit;
    }

    // Shore fade (water.rs): from above, the surface fades in over the depth falloff.
    var edge = 1.0;
    if above && water.edge.z > 0.5 {
        edge = clamp((fog_distance - water.edge.x) / max(water.edge.y - water.edge.x, 1e-3), 0.0, 1.0);
    }
    let alpha = (fr + (1.0 - fr) * body_alpha) * edge;
    let reflected_share = fr * edge;
    let premultiplied = reflected_share * reflected + (1.0 - fr) * body_alpha * lit * edge;
    let surface_colour = vec4(premultiplied / max(alpha, 1e-4), alpha);
    // Distance fog and (non-HDR) tonemapping as for the standard material, then over the
    // refracted scene (already fogged and tonemapped: a copy of the opaque pass).
    pbr.material.flags = pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    let colour = pbr_functions::main_pass_post_lighting_processing(pbr, surface_colour);
    var result = colour.rgb * colour.a + refracted * (1.0 - colour.a);
    let object_weight = saturate(traced.w * water.tuning3.x);
    if object_weight > 0.0 && reflected_share > 0.0 {
        // Measure the fallback contribution after fog and tonemapping. The opaque copy
        // already contains both, so replace that contribution with the traced colour in
        // the same space while keeping the water body and sun highlight in the result.
        let fallback_share = fallback * reflected_share / max(alpha, 1e-4);
        let without_fallback = pbr_functions::main_pass_post_lighting_processing(
            pbr, vec4(surface_colour.rgb - fallback_share, alpha));
        let sky_part = (colour.rgb - without_fallback.rgb) * colour.a;
        let object_part = reflection_vibrance(traced.rgb) * (water.tuning.y * reflected_share);
        result += object_weight * (object_part - sky_part);
    }
    // `NVF_DEBUG_WATER_SSR=1`: what the march found, as magenta (nothing found: the water
    // as it is).
    @if(NVF_DEBUG_WATER_SSR) result = select(result, vec3<f32>(1.0, 0.0, 1.0), traced.w > 0.0);
    return vec4(result, 1.0);
}
