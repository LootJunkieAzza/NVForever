// Volumetric atmosphere of the remastered look (atmos.rs, docs/design/graphics-remaster.md
// §3). An engineering choice, not the original game's.
//
// `march` (half resolution): from the camera to the surface (or `atmos.params.w` for the
// sky), 24 jittered steps through a fog that thins with height; at each the sun's light
// reaches the step unless its shadow cascades say something stands between (the sun's
// own shadow maps, so buildings, rocks and trees cast shafts into the air), scattered
// towards the camera by a Henyey-Greenstein phase, plus the ambient light scattered
// evenly. Writes the light gathered (rgb) and how much of what lies behind still shows
// (a), and the texel's distance.
// `temporal`: blended with last frame's at the same place on the screen (the steps' jitter
// averages out over frames).
// `composite` (full resolution, blended into the HDR picture before tonemapping): picture ×
// transmittance + light, from the four half-resolution texels around the pixel, those at
// about the pixel's distance weighed most.

import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput;
import bevy_pbr::render::mesh_view_bindings::{view, lights};
import bevy_pbr::render::mesh_view_types::DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT;
import bevy_pbr::render::shadow_sampling::sample_shadow_map_hardware;
import bevy_pbr::render::shadows::{get_cascade_index, world_to_directional_light_local};
import nvf_assets::first_person_depth::unsqueeze;

struct Atmos {
    // Last frame's clip space from this frame's world (camera-relative: world minus this
    // frame's camera position).
    prev_clip_from_rel: mat4x4<f32>,
    // x: density at the reference height (per game unit); y: 1 / the height over which it
    // falls to 1/e; z: the reference height; w: the farthest the march goes.
    params: vec4<f32>,
    // x: the phase's anisotropy g; y: sun strength; z: ambient strength; w: frame number.
    params2: vec4<f32>,
    // The picture's size (x, y) and the half-resolution size (z, w).
    size: vec4<f32>,
    // x: the share of the new frame in `temporal`; y: 1 when last frame's textures are
    // valid.
    params3: vec4<f32>,
}

@if(MULTISAMPLED) @group(1) @binding(0) var depth: texture_depth_multisampled_2d;
@if(!MULTISAMPLED) @group(1) @binding(0) var depth: texture_depth_2d;
@group(1) @binding(1) var<uniform> atmos: Atmos;
// temporal: this frame's march; composite: the accumulated result.
@group(1) @binding(2) var tex_a: texture_2d<f32>;
@group(1) @binding(3) var linear_sampler: sampler;
// temporal: last frame's accumulated result; composite: the half-resolution distances.
@group(1) @binding(4) var tex_b: texture_2d<f32>;

const PI: f32 = 3.14159265;
const STEPS: u32 = 24u;
const SKY: f32 = 60000.0;

fn depth_at(p: vec2<i32>) -> f32 {
    let dims = vec2<i32>(textureDimensions(depth));
    return unsqueeze(textureLoad(depth, clamp(p, vec2(0), dims - 1), 0));
}

fn ign(p: vec2<f32>, frame: f32) -> f32 {
    let q = p + 5.588238 * (frame % 64.0);
    return fract(52.9829189 * fract(dot(q, vec2(0.06711056, 0.00583715))));
}

// World position of the picture's point `p` (pixels) at depth `d`.
fn world_at(p: vec2<f32>, d: f32) -> vec3<f32> {
    let ndc = vec2(p.x / atmos.size.x * 2.0 - 1.0, 1.0 - p.y / atmos.size.y * 2.0);
    let w = view.world_from_clip * vec4(ndc, max(d, 1e-7), 1.0);
    return w.xyz / w.w;
}

fn phase_hg(g: f32, cos_theta: f32) -> f32 {
    let g2 = g * g;
    return (1.0 - g2) / (4.0 * PI * pow(max(1.0 + g2 - 2.0 * g * cos_theta, 1e-4), 1.5));
}

// The sun's light reaching world point `p` (1 lit, 0 in shadow), from its shadow cascades.
fn sun_visible(sun: u32, p: vec3<f32>) -> f32 {
    let light = &lights.directional_lights[sun];
    if ((*light).flags & DIRECTIONAL_LIGHT_FLAGS_SHADOWS_ENABLED_BIT) == 0u {
        return 1.0;
    }
    let view_z = (view.view_from_world * vec4(p, 1.0)).z;
    let cascade = get_cascade_index(sun, view_z);
    if cascade >= (*light).num_cascades {
        return 1.0;
    }
    let local = world_to_directional_light_local(sun, cascade, vec4(p, 1.0));
    if local.w == 0.0 {
        return 1.0;
    }
    return sample_shadow_map_hardware(local.xy, local.z, i32((*light).depth_texture_base_index + cascade));
}

// The sun among the directional lights: the one whose shadow cascades reach furthest
// (the first-person arms' shade light reaches a few metres), else the first.
fn find_sun() -> i32 {
    var best = -1;
    var reach = -1.0;
    for (var i = 0u; i < lights.n_directional_lights; i++) {
        let l = &lights.directional_lights[i];
        var r = 0.0;
        if (*l).num_cascades > 0u {
            r = (*l).cascades[(*l).num_cascades - 1u].far_bound;
        }
        if r > reach {
            reach = r;
            best = i32(i);
        }
    }
    return best;
}

struct MarchOut {
    @location(0) light: vec4<f32>,
    @location(1) dist: vec4<f32>,
}

@fragment
fn march(in: FullscreenVertexOutput) -> MarchOut {
    var out: MarchOut;
    let half = vec2<i32>(floor(in.position.xy));
    let px = half * 2;
    let d = depth_at(px);
    let camera = view.world_position;
    let far_point = world_at(vec2<f32>(px) + 0.5, select(1e-6, d, d > 0.0));
    let to = far_point - camera;
    var dist = length(to);
    out.dist = vec4(select(SKY, min(dist, SKY), d > 0.0), 0.0, 0.0, 0.0);
    let dir = to / max(dist, 1e-3);
    if atmos.params.x <= 0.0 {
        out.light = vec4(0.0, 0.0, 0.0, 1.0);
        return out;
    }
    dist = min(dist, atmos.params.w);
    let frame = atmos.params2.w;
    let jitter = ign(vec2<f32>(half), frame);
    let sun_index = find_sun();
    var sun_colour = vec3(0.0);
    var cos_sun = 0.0;
    if sun_index >= 0 {
        let light = &lights.directional_lights[u32(sun_index)];
        // The light's colour as display values (lights.rs DISPLAY_HELPERS), to linear.
        sun_colour = pow(max((*light).color.rgb * view.exposure / PI, vec3(0.0)), vec3(2.2)) * atmos.params2.y;
        cos_sun = dot(dir, (*light).direction_to_light.xyz);
    }
    let ambient = pow(max(lights.ambient_color.rgb * view.exposure, vec3(0.0)), vec3(2.2)) * atmos.params2.z;
    let phase = phase_hg(atmos.params2.x, cos_sun);
    var light = vec3(0.0);
    var transmittance = 1.0;
    var t0 = 0.0;
    for (var i = 0u; i < STEPS; i++) {
        // More steps near the camera, where shafts are sharpest.
        let f1 = (f32(i) + 1.0) / f32(STEPS);
        let t1 = dist * f1 * f1;
        let t = mix(t0, t1, jitter);
        let p = camera + dir * t;
        // Thin near the camera (about 15 m to 70 m), so what is close stays crisp.
        let near = smoothstep(1000.0, 5000.0, t);
        let density = atmos.params.x * near * exp(-max(p.y - atmos.params.z, 0.0) * atmos.params.y);
        let dt = t1 - t0;
        var sun = vec3(0.0);
        if sun_index >= 0 {
            sun = sun_colour * (phase * sun_visible(u32(sun_index), p));
        }
        let step_t = exp(-density * dt);
        // In-scattered light, integrated over the step (energy-conserving form).
        light += transmittance * (sun + ambient / (4.0 * PI)) * (1.0 - step_t);
        transmittance *= step_t;
        t0 = t1;
    }
    out.light = vec4(light, transmittance);
    return out;
}

@fragment
fn temporal(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let half = vec2<i32>(floor(in.position.xy));
    let current = textureLoad(tex_a, half, 0);
    if atmos.params3.y < 0.5 {
        return current;
    }
    let px = half * 2;
    let d = depth_at(px);
    let p = world_at(vec2<f32>(px) + 0.5, select(1e-6, d, d > 0.0)) - view.world_position;
    let c = atmos.prev_clip_from_rel * vec4(p, 1.0);
    if c.w <= 0.0 {
        return current;
    }
    let ndc = c.xy / c.w;
    let uv = vec2(ndc.x * 0.5 + 0.5, 0.5 - ndc.y * 0.5);
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
        return current;
    }
    let old = textureSampleLevel(tex_b, linear_sampler, uv, 0.0);
    // Held near this frame's (what moved in front of the light changes it at once).
    let lo = min(current, vec4(0.0, 0.0, 0.0, 0.0)) + current * 0.5;
    let hi = current * 2.0 + vec4(0.02, 0.02, 0.02, 0.05);
    return mix(clamp(old, lo, hi), current, atmos.params3.x);
}

@fragment
fn composite(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let px = vec2<i32>(floor(in.position.xy));
    let d = depth_at(px);
    let camera = view.world_position;
    let z = select(SKY, min(length(world_at(vec2<f32>(px) + 0.5, max(d, 1e-6)) - camera), SKY), d > 0.0);
    let dims = vec2<i32>(textureDimensions(tex_a));
    let base = vec2<i32>(floor((vec2<f32>(px) + 0.5) * 0.5 - 0.5));
    var sum = vec4(0.0);
    var wsum = 0.0;
    var nearest = vec4(0.0, 0.0, 0.0, 1.0);
    var nearest_dz = 1e9;
    for (var y = 0; y <= 1; y++) {
        for (var x = 0; x <= 1; x++) {
            let t = clamp(base + vec2(x, y), vec2(0), dims - 1);
            let v = textureLoad(tex_a, t, 0);
            let tz = textureLoad(tex_b, t, 0).r;
            let dz = abs(tz - z);
            let w = exp(-dz / (0.05 * z + 8.0));
            sum += v * w;
            wsum += w;
            if dz < nearest_dz {
                nearest_dz = dz;
                nearest = v;
            }
        }
    }
    let v = select(nearest, sum / max(wsum, 1e-4), wsum > 1e-3);
    // Blended as picture × a + rgb (atmos.rs `composite` blend state).
    return vec4(v.rgb, saturate(v.a));
}
