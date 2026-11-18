// Sun visibility (nvforever sun_visibility.rs, docs/design/sun-visibility.md): a factor
// V in [0, 1] on the outdoor sun's direct light, V = K * Vsolid, with K the cloud
// transmittance and Vsolid the solid occluders' visibility: min(C, T) of the cascade
// shadow C and the terrain visibility T, fading to T over the last cascade's end band.
//
// Imported by bevy_pbr's `apply_pbr_lighting` (patched in by sun_visibility.rs
// `HOOK_EDITS`) only with the shader def NVF_SUN_VISIBILITY, which the view's pipelines
// have exactly when its mesh view bind group holds bindings 39-42 (bevy_pbr
// mesh_view_bindings.rs `NvfSunVisibilityBindings`). NVF_CLOUD_VISIBILITY and
// NVF_TERRAIN_VISIBILITY switch the two samples on; without them K and T are 1.
//
// Every texture read is textureSampleLevel: the lighting runs in non-uniform control flow
// (per-fragment branches here, and callers such as nif_material.wesl `with_shade_light`).

// Bevy world axes throughout (y up); positions and heights in game units.
struct NvfSunVisibility {
    // World xz to cloud texture uv: xy scale, zw offset (uv = hit.xz * scale + offset).
    cloud_uv: vec4<f32>,
    // x: the cloud plane's height; y: strength (0 no shadow, 1 the texture's full
    // transmittance); z, w: the sine of the light's elevation from which the cloud shadow
    // starts (z) and at which it is whole (w).
    cloud: vec4<f32>,
    // World xz to terrain map uv: xy scale, zw offset.
    terrain_uv: vec4<f32>,
    // x, y: the band of view distance over which Vsolid goes from min(C, T) to T (the last
    // cascade's end); z: the width (uv) over which T fades to 1 at the map's edge; w: the
    // height (game units) over which a receiver goes from shadowed to lit at the map's
    // shadow height.
    terrain: vec4<f32>,
    // x: feature mask (1 cloud, 2 terrain); y: debug view (0 off, 1 cloud K, 2 terrain T,
    // 3 combined V).
    mode: vec4<u32>,
}

@group(0) @binding(39) var<uniform> nvf_sun: NvfSunVisibility;
// Cloud transmittance (R: 1 clear, 0 opaque), repeat-wrapped by the sampler.
@group(0) @binding(40) var nvf_cloud_texture: texture_2d<f32>;
// Terrain shadow-height map (#2671; a 1x1 stand-in until its producer exists).
@group(0) @binding(41) var nvf_terrain_texture: texture_2d<f32>;
@group(0) @binding(42) var nvf_sun_visibility_sampler: sampler;

// The debug view asked for (NVF_PLAY_DEBUG_SUN_VISIBILITY), 0 when off.
fn nvf_sun_debug_mode() -> u32 {
    return nvf_sun.mode.y;
}

// K: the share of the sun's light the clouds let through to a receiver at P, the light
// coming from L (unit, towards the light). The ray from P along L meets the cloud plane
// at height cloud.x; the transmittance there, weakened by the strength, is K. A receiver
// at or above the plane, or a light too low (its elevation's sine at most cloud.z), has
// no cloud shadow; between cloud.z and cloud.w the shadow fades in, so the hit point does
// not run off to infinity at the horizon [engineering choice].
@if(NVF_CLOUD_VISIBILITY)
fn nvf_cloud_transmittance(P: vec3<f32>, L: vec3<f32>) -> f32 {
    let height = nvf_sun.cloud.x;
    let up = L.y;
    if up <= nvf_sun.cloud.z || P.y >= height {
        return 1.0;
    }
    let hit = P.xz + L.xz * ((height - P.y) / up);
    let uv = hit * nvf_sun.cloud_uv.xy + nvf_sun.cloud_uv.zw;
    let transmittance = textureSampleLevel(nvf_cloud_texture, nvf_sun_visibility_sampler, uv, 0.0).r;
    let k = saturate(1.0 - nvf_sun.cloud.y * (1.0 - transmittance));
    return mix(1.0, k, smoothstep(nvf_sun.cloud.z, nvf_sun.cloud.w, up));
}

@if(!NVF_CLOUD_VISIBILITY)
fn nvf_cloud_transmittance(P: vec3<f32>, L: vec3<f32>) -> f32 {
    return 1.0;
}

// T: the terrain's visibility of the sun at P. A stub until the shadow-height producer
// (#2671, stage T1) exists: stage T2 samples `nvf_terrain_texture` here at
// P.xz * terrain_uv.xy + terrain_uv.zw and compares P.y with the shadow height there
// (softened over terrain.w, faded to 1 over terrain.z at the map's edge).
@if(NVF_TERRAIN_VISIBILITY)
fn nvf_terrain_visibility(P: vec3<f32>, L: vec3<f32>) -> f32 {
    return 1.0;
}

@if(!NVF_TERRAIN_VISIBILITY)
fn nvf_terrain_visibility(P: vec3<f32>, L: vec3<f32>) -> f32 {
    return 1.0;
}

// Vsolid from the cascade shadow C and the terrain visibility T at view-space depth
// view_z (negative in front of the camera). Nearer than the band min(C, T): where both
// see the same ridge the darker one counts once, never C * T (which would square one
// filtered edge). Beyond it T alone (Bevy's cascades return 1 past the last one, so the
// object shadows fade out there instead of stopping at a line). Without terrain, C.
@if(NVF_TERRAIN_VISIBILITY)
fn nvf_solid_visibility(cascade: f32, terrain: f32, view_z: f32) -> f32 {
    let band = nvf_sun.terrain.xy;
    let x = saturate((-view_z - band.x) / max(band.y - band.x, 0.001));
    let w = x * x * (3.0 - 2.0 * x);
    return mix(min(cascade, terrain), terrain, w);
}

@if(!NVF_TERRAIN_VISIBILITY)
fn nvf_solid_visibility(cascade: f32, terrain: f32, view_z: f32) -> f32 {
    return cascade;
}

// The parts of the sun's visibility at P (light from L, cascade shadow C, view depth
// view_z): x K, y T, z Vsolid, w V = K * Vsolid.
fn nvf_sun_visibility_parts(P: vec3<f32>, L: vec3<f32>, cascade_shadow: f32, view_z: f32) -> vec4<f32> {
    let k = nvf_cloud_transmittance(P, L);
    let t = nvf_terrain_visibility(P, L);
    let solid = nvf_solid_visibility(cascade_shadow, t, view_z);
    return vec4<f32>(k, t, solid, k * solid);
}

// V, the factor that takes the place of the sun's cascade shadow in its direct light.
fn nvf_sun_visibility(P: vec3<f32>, L: vec3<f32>, cascade_shadow: f32, view_z: f32) -> f32 {
    return nvf_sun_visibility_parts(P, L, cascade_shadow, view_z).w;
}

// The value the debug view shows from the parts: K, T or V.
fn nvf_sun_debug_value(parts: vec4<f32>) -> f32 {
    switch nvf_sun.mode.y {
        case 1u: {
            return parts.x;
        }
        case 2u: {
            return parts.y;
        }
        default: {
            return parts.w;
        }
    }
}
