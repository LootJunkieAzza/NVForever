// Water surface (WATR), see water.rs for the model and docs/spec-notes/world.md §6 for
// which parts are documented and which are hypotheses.
//
// Bevy world space: y up. Game space (x, y, z) = Bevy (x, -z, y).

#import bevy_pbr::{
    forward_io::VertexOutput,
    mesh_view_bindings::{view, globals, lights, view_transmission_texture, view_transmission_sampler},
    pbr_types,
    pbr_functions,
    view_transformations::{frag_coord_to_ndc, frag_coord_to_uv, ndc_to_uv, position_ndc_to_world},
}
#ifdef DEPTH_PREPASS
#import bevy_pbr::prepass_utils
#endif

struct WaterParams {
    shallow: vec4<f32>,
    deep: vec4<f32>,
    // rgb = reflection colour, a = 1 when the type is reflective (reflects the sky).
    reflection: vec4<f32>,
    sky_upper: vec4<f32>,
    sky_horizon: vec4<f32>,
    // near, far, amount, unused
    above_fog: vec4<f32>,
    under_fog: vec4<f32>,
    // reflectivity, fresnel amount, shininess, wave height (world units)
    surface: vec4<f32>,
    // per layer: uv velocity (per second), 1 / uv scale (per world unit), amplitude
    layers: array<vec4<f32>, 3>,
    // x = Bevy y of the plane the reflection texture shows, y = 1 when it is in use
    reflection_plane: vec4<f32>,
};

@group(#{MATERIAL_BIND_GROUP}) @binding(0) var<uniform> water: WaterParams;
@group(#{MATERIAL_BIND_GROUP}) @binding(1) var noise_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(2) var noise_sampler: sampler;
@group(#{MATERIAL_BIND_GROUP}) @binding(3) var reflection_texture: texture_2d<f32>;
@group(#{MATERIAL_BIND_GROUP}) @binding(4) var reflection_sampler: sampler;

// Distance along the wavy reflected ray at which the reflection texture is sampled: the
// texture has no depth, so the wave offset is that of a point this far away [tuned by eye].
const REFLECTION_SAMPLE_DISTANCE: f32 = 500.0;

// Refraction: the scene behind the surface is sampled this far (in screen UV per unit of
// view-space normal tilt) from the pixel, growing to that over this much water
// [tuned by eye; the engine's refraction rule is unknown].
const REFRACTION_STRENGTH: f32 = 0.08;
const REFRACTION_DEPTH: f32 = 200.0;

// The opaque surface behind screen position `fc` (depth prepass): world position, and
// w = 1 when there is one (0 for the sky or without a prepass).
fn scene_point(fc: vec2<f32>) -> vec4<f32> {
#ifdef DEPTH_PREPASS
    let d = prepass_utils::prepass_depth(vec4(fc, 0.0, 1.0), 0u);
    if d > 0.0 {
        let ndc = frag_coord_to_ndc(vec4(fc, d, 1.0));
        return vec4(position_ndc_to_world(ndc), 1.0);
    }
#endif
    return vec4(0.0);
}

// Slope (dh/dx, dh/dy in game space) of one noise layer: height = texture channel `mask`
// (0..1) times the layer amplitude times the wave height.
fn layer_slope(g: vec2<f32>, layer: vec4<f32>, mask: vec4<f32>, t: f32) -> vec2<f32> {
    let uv = g * layer.z + layer.xy * t;
    let e = 1.0 / vec2<f32>(textureDimensions(noise_texture));
    let h0 = dot(textureSample(noise_texture, noise_sampler, uv), mask);
    let hu = dot(textureSample(noise_texture, noise_sampler, uv + vec2(e.x, 0.0)), mask);
    let hv = dot(textureSample(noise_texture, noise_sampler, uv + vec2(0.0, e.y)), mask);
    // d(uv)/d(world) = layer.z
    return vec2(hu - h0, hv - h0) / e * layer.z * layer.w * water.surface.w;
}

fn sky_colour(y: f32) -> vec3<f32> {
    // The same gradient as the sky dome above the horizon (sky.rs).
    return mix(water.sky_horizon.rgb, water.sky_upper.rgb, sqrt(clamp(y / 0.5, 0.0, 1.0)));
}

@fragment
fn fragment(in: VertexOutput) -> @location(0) vec4<f32> {
    let pos = in.world_position.xyz;
    let to_cam = view.world_position.xyz - pos;
    let dist = max(length(to_cam), 1e-3);
    let V = to_cam / dist;
    let above = V.y >= 0.0;

    let g = vec2(pos.x, -pos.z);
    let t = globals.time;
    var s = layer_slope(g, water.layers[0], vec4(1.0, 0.0, 0.0, 0.0), t);
    s += layer_slope(g, water.layers[1], vec4(0.0, 1.0, 0.0, 0.0), t);
    s += layer_slope(g, water.layers[2], vec4(0.0, 0.0, 1.0, 0.0), t);
    // Game-space normal (-sx, -sy, 1) in Bevy space.
    var N = normalize(vec3(-s.x, 1.0, s.y));
    var up = vec3(0.0, 1.0, 0.0);
    if !above {
        N = -N;
        up = -up;
    }

    // Fog distance: the length of the view ray inside the water. From above, from the
    // surface to the opaque surface behind it (depth prepass), so 0 at the waterline;
    // from below, from the camera to the surface.
    let size = view.viewport.zw;
    let uv0 = frag_coord_to_uv(in.position.xy);
    let behind0 = scene_point(in.position.xy);
    var fog_distance = 1.0e7;
    if behind0.w > 0.5 {
        fog_distance = distance(behind0.xyz, pos);
    }
    // Refraction: shift the sample by the wave normal's tilt in view space, more through
    // deeper water; keep the unshifted sample where the shifted one lands on something in
    // front of the water (it would bleed into the surface).
    let tilt = (view.view_from_world * vec4(N - up, 0.0)).xy;
    let depth_scale = clamp(min(fog_distance, dist) / REFRACTION_DEPTH, 0.0, 1.0);
    var uv = clamp(uv0 + vec2(tilt.x, -tilt.y) * REFRACTION_STRENGTH * depth_scale, vec2(0.0), vec2(1.0));
    if above {
        let behind = scene_point(view.viewport.xy + uv * size);
        if behind.w > 0.5 && distance(behind.xyz, view.world_position.xyz) < dist {
            uv = uv0;
        } else if behind.w > 0.5 {
            fog_distance = distance(behind.xyz, pos);
        }
    }
    if !above {
        fog_distance = dist;
    }
    let refracted = textureSampleLevel(view_transmission_texture, view_transmission_sampler, uv, 0.0).rgb;
    let fog = select(water.under_fog, water.above_fog, above);
    let depth01 = clamp((fog_distance - fog.x) / max(fog.y - fog.x, 1.0), 0.0, 1.0);
    // Opaque beyond the far distance; the fog "amount" is not used (its role is unknown:
    // as a cap it left the lake bed showing through far out).
    let body_alpha = depth01;
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
    if above {
        let R = reflect(-V, N);
        let schlick = 0.02 + 0.98 * pow(1.0 - clamp(dot(N, V), 0.0, 1.0), 5.0);
        fr = clamp(water.surface.x * mix(1.0, schlick, water.surface.y), 0.0, 1.0);
        // Sampled unconditionally (uniform control flow), used when it shows this plane.
        // The mirror image of the wavy reflected ray's point, seen from the camera; for a
        // flat surface that is this very pixel.
        let mirrored = pos + vec3(R.x, -R.y, R.z) * REFLECTION_SAMPLE_DISTANCE;
        let clip = view.clip_from_world * vec4(mirrored, 1.0);
        var uv = frag_coord_to_uv(in.position.xy);
        if clip.w > 0.0 {
            uv = ndc_to_uv(clip.xy / clip.w);
        }
        let world_reflection = textureSampleLevel(reflection_texture, reflection_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
        let mirror_here = water.reflection_plane.y > 0.5 && abs(pos.y - water.reflection_plane.x) < 2.0;
        let sky = select(sky_colour(R.y), world_reflection, mirror_here);
        reflected = select(water.reflection.rgb, sky, water.reflection.a > 0.5);
        if lights.n_directional_lights > 0u {
            let sun = lights.directional_lights[0];
            let spec = pow(max(dot(R, sun.direction_to_light), 0.0), max(water.surface.z, 1.0));
            reflected += spec * sun.color.rgb * view.exposure;
        }
    } else {
        // From below, the sky shows only within Snell's window (refractive index 1.33:
        // cos > 0.66 from the vertical); outside it the surface reflects the water body.
        let c = clamp(dot(N, V), 0.0, 1.0);
        fr = 1.0 - smoothstep(0.62, 0.70, c);
        reflected = lit;
    }

    let alpha = fr + (1.0 - fr) * body_alpha;
    let premultiplied = fr * reflected + (1.0 - fr) * body_alpha * lit;
    var colour = vec4(premultiplied / max(alpha, 1e-4), alpha);
    // Distance fog and (non-HDR) tonemapping as for the standard material, then over the
    // refracted scene (already fogged and tonemapped: a copy of the opaque pass).
    pbr.material.flags = pbr_types::STANDARD_MATERIAL_FLAGS_FOG_ENABLED_BIT;
    colour = pbr_functions::main_pass_post_lighting_processing(pbr, colour);
    return vec4(colour.rgb * colour.a + refracted * (1.0 - colour.a), 1.0);
}
