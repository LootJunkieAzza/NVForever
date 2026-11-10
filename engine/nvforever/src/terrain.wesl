// Landscape with its first texture layers folded into the base's draw (terrain.rs "One
// draw for the base and its first layers"): the fragment shader of `TerrainMaterial`.
//
// The mesh carries the opacity of up to four layers a vertex (terrain.rs `folded_mesh`):
// the vertex colour's alpha, the second texture coordinate's two numbers, and the tangent's
// fourth number, whose size past 1 is the opacity while its sign stays the tangent's
// handedness. The surface colour and the normal are the base's, blended towards each
// layer's in the layers' order by opacity x the layer texture's alpha, as the layers'
// own blended draws blended their lit colours over the base; then the surface is lit once.
// What follows the lighting is nif_material.wesl's for an opaque lit shape with no emitted
// light: Bevy's ambient specular taken out again, then fog (and tonemapping in a view
// without HDR).

import bevy_pbr::{
    render::forward_io::{VertexOutput, FragmentOutput},
    render::pbr_fragment::pbr_input_from_standard_material,
    render::pbr_functions::{apply_pbr_lighting, main_pass_post_lighting_processing, calculate_F0, calculate_tbn_mikktspace, apply_normal_mapping},
    render::pbr_lighting::{EnvBRDFApprox, F_AB},
    render::mesh_view_bindings::{lights, view},
    render::mesh_bindings::mesh,
    render::pbr_types::StandardMaterial,
};
@if(VISIBILITY_RANGE_DITHER) import bevy_pbr::render::pbr_functions::visibility_range_dither;
@if(BINDLESS) import bevy_render::bindless::{bindless_textures_2d, bindless_samplers_filtering};
@if(BINDLESS) import bevy_pbr::render::pbr_bindings::{material_indices, material_array};
@if(!BINDLESS) import bevy_pbr::render::pbr_bindings;

struct TerrainLayers {
    // x: how many layers the material holds (0 to 4); y: bit i set when layer i has a
    // normal map (without one its normal is the surface's own).
    counts: vec4<u32>,
}

// The extension's index table (indices 50-58 of TerrainLayers' bindless attribute).
@if(BINDLESS) struct TerrainIndices {
    layers: u32,    // 50
    diffuse0: u32,  // 51
    normal0: u32,   // 52
    diffuse1: u32,  // 53
    normal1: u32,   // 54
    diffuse2: u32,  // 55
    normal2: u32,   // 56
    diffuse3: u32,  // 57
    normal3: u32,   // 58
}
@if(BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(100) var<storage> terrain_indices: array<TerrainIndices>;
@if(BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(101) var<storage> terrain_layers: array<TerrainLayers>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(50) var<uniform> terrain_layer_values: TerrainLayers;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(51) var diffuse0: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(52) var normal0: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(53) var diffuse1: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(54) var normal1: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(55) var diffuse2: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(56) var normal2: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(57) var diffuse3: texture_2d<f32>;
@if(!BINDLESS) @group(constants::MATERIAL_BIND_GROUP) @binding(58) var normal3: texture_2d<f32>;

// The material's slot in the bindless tables (unused without bindless).
fn material_slot(instance_index: u32) -> u32 {
    return mesh[instance_index].material_and_lightmap_bind_group_slot & 0xffffu;
}

fn layer_counts(instance_index: u32) -> vec4<u32> {
    @if(BINDLESS) return terrain_layers[terrain_indices[material_slot(instance_index)].layers].counts;
    @if(!BINDLESS) return terrain_layer_values.counts;
}

fn standard_material(instance_index: u32) -> StandardMaterial {
    @if(BINDLESS) return material_array[material_indices[material_slot(instance_index)].material];
    @if(!BINDLESS) return pbr_bindings::material;
}

// Layer `k`'s diffuse texture at `uv`, read with the base texture's sampler.
fn layer_diffuse(k: u32, instance_index: u32, uv: vec2<f32>) -> vec4<f32> {
    @if(BINDLESS) {
        let slot = material_slot(instance_index);
        let i = terrain_indices[slot];
        var t = i.diffuse0;
        if k == 1u { t = i.diffuse1; }
        if k == 2u { t = i.diffuse2; }
        if k == 3u { t = i.diffuse3; }
        return textureSampleBias(bindless_textures_2d[t], bindless_samplers_filtering[material_indices[slot].base_color_sampler], uv, view.mip_bias);
    }
    @if(!BINDLESS) {
        if k == 1u { return textureSampleBias(diffuse1, pbr_bindings::base_color_sampler, uv, view.mip_bias); }
        if k == 2u { return textureSampleBias(diffuse2, pbr_bindings::base_color_sampler, uv, view.mip_bias); }
        if k == 3u { return textureSampleBias(diffuse3, pbr_bindings::base_color_sampler, uv, view.mip_bias); }
        return textureSampleBias(diffuse0, pbr_bindings::base_color_sampler, uv, view.mip_bias);
    }
}

// Layer `k`'s normal map at `uv`.
fn layer_normal(k: u32, instance_index: u32, uv: vec2<f32>) -> vec3<f32> {
    @if(BINDLESS) {
        let slot = material_slot(instance_index);
        let i = terrain_indices[slot];
        var t = i.normal0;
        if k == 1u { t = i.normal1; }
        if k == 2u { t = i.normal2; }
        if k == 3u { t = i.normal3; }
        return textureSampleBias(bindless_textures_2d[t], bindless_samplers_filtering[material_indices[slot].base_color_sampler], uv, view.mip_bias).rgb;
    }
    @if(!BINDLESS) {
        if k == 1u { return textureSampleBias(normal1, pbr_bindings::base_color_sampler, uv, view.mip_bias).rgb; }
        if k == 2u { return textureSampleBias(normal2, pbr_bindings::base_color_sampler, uv, view.mip_bias).rgb; }
        if k == 3u { return textureSampleBias(normal3, pbr_bindings::base_color_sampler, uv, view.mip_bias).rgb; }
        return textureSampleBias(normal0, pbr_bindings::base_color_sampler, uv, view.mip_bias).rgb;
    }
}

@fragment
fn fragment(
    vertex: VertexOutput,
    @builtin(front_facing) is_front: bool,
) -> FragmentOutput {
    var in = vertex;
    @if(VISIBILITY_RANGE_DITHER) visibility_range_dither(in.position, in.visibility_range_dither);
    // The layers' opacities, taken out of the attributes that carried them, which then
    // hold what the standard material expects: an opaque vertex colour and a tangent of
    // handedness +-1.
    var opacity = vec4(0.0);
    var tint = vec3(1.0);
    @if(VERTEX_COLORS) opacity.x = in.color.a;
    @if(VERTEX_COLORS) tint = in.color.rgb;
    @if(VERTEX_COLORS) in.color = vec4(in.color.rgb, 1.0);
    @if(VERTEX_UVS_B) opacity.y = in.uv_b.x;
    @if(VERTEX_UVS_B) opacity.z = in.uv_b.y;
    @if(VERTEX_TANGENTS) opacity.w = abs(in.world_tangent.w) - 1.0;
    @if(VERTEX_TANGENTS) in.world_tangent = vec4(in.world_tangent.xyz, sign(in.world_tangent.w));

    var pbr_input = pbr_input_from_standard_material(in, is_front);
    let counts = layer_counts(in.instance_index);
    @if(VERTEX_UVS_A) {
        let standard = standard_material(in.instance_index);
        let uv = (standard.uv_transform * vec3(in.uv, 1.0)).xy;
        var colour = pbr_input.material.base_color.rgb;
        var normal = pbr_input.N;
        let plain = normalize(pbr_input.world_normal);
        @if(VERTEX_TANGENTS) let tbn = calculate_tbn_mikktspace(in.world_normal, in.world_tangent);
        for (var k = 0u; k < min(counts.x, 4u); k += 1u) {
            let texel = layer_diffuse(k, in.instance_index, uv);
            let share = saturate(opacity[k]) * texel.a;
            var layer_n = plain;
            @if(VERTEX_TANGENTS) if (counts.y & (1u << k)) != 0u {
                layer_n = apply_normal_mapping(standard.flags, tbn, false, is_front, layer_normal(k, in.instance_index, uv));
            }
            colour = mix(colour, texel.rgb * tint * standard.base_color.rgb, share);
            normal = mix(normal, layer_n, share);
        }
        pbr_input.material.base_color = vec4(colour, 1.0);
        pbr_input.N = normalize(normal);
    }
    pbr_input.material.base_color.a = 1.0;

    var out: FragmentOutput;
    out.color = apply_pbr_lighting(pbr_input);
    // Bevy's ambient specular out again, as in nif_material.wesl (the game's ambient light
    // is a flat colour; highlights come from lights).
    let m = pbr_input.material;
    let F0 = calculate_F0(m.base_color.rgb, m.metallic, m.reflectance);
    let NdotV = max(dot(pbr_input.N, pbr_input.V), 0.0001);
    let spec = EnvBRDFApprox(F0, F_AB(m.perceptual_roughness, NdotV));
    let spec_occlusion = saturate(dot(F0, vec3(50.0 * 0.33)));
    var ambient_spec = spec * spec_occlusion * lights.ambient_color.rgb * pbr_input.diffuse_occlusion;
    if lights.ambient_color.a < 0.0 {
        ambient_spec *= pow(max(lights.ambient_color.rgb * view.exposure, vec3(1e-6)), vec3(1.2));
    }
    out.color = vec4(max(out.color.rgb - view.exposure * ambient_spec, vec3(0.0)), out.color.a);
    out.color = main_pass_post_lighting_processing(pbr_input, out.color);
    return out;
}
