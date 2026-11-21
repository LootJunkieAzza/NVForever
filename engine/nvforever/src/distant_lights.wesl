// Distant lights at night (distant_lights.rs, docs/design/graphics-enhanced.md §3.17): an
// enhanced-look switch of NVForever's own, not a fact about the original game.
//
// One batched mesh of glows; each glow is four vertices (distant_lights.rs `glow_mesh`):
//   location 0 POSITION  the card's centre (game space under the world root);
//   location 1 NORMAL    a shaft's half vector from its centre to its bright end `a`
//                        (zero for a round card);
//   location 2 UV_0      the corner (-1..1);
//   location 3 UV_1      x: the round half size (world units), y: the motion code (0 steady,
//                        1 flicker, 2 pulse) plus a phase in its fraction;
//   location 4 TANGENT   x, y: a shaft's half widths at `a` and `b`, z: its light per pixel,
//                        w: the card's kind (0 round, 1 shaft, 2 shaft fading from `a`,
//                        3 a placed light's core and halo; `CARD_*`);
//   location 5 COLOR     the colour as display values times its strength (merged lights, the
//                        record's fade, the hand-over with its cell's fade).
//
// Vertex, round: the card is spread in the plane of the view (always facing the camera), at
// least `glow.shape.w` output pixels and `glow.extra.z` render pixels in half size (the two
// differ under an upscaler) so a far light stays a few pixels wide; a card widened by that
// floor gives less light per pixel (its own area over the area drawn, held to
// `glow.extra.x`). Shaft: the card spans the segment between its ends, widened across by
// each end's half width (held to the same floors) and facing the camera around its axis
// (an axial billboard; CPU copy `axial_corners`), its fragments mapped across by their
// offset from the axis over the half width at that point (`shaft_x`); a shaft shorter on
// screen than two floors, seen end-on, or with an end behind the camera is drawn as its
// round card (`shaft_shows`).
// Faded out over the reach's last cells (`glow.shape.yz`), by the night (`glow.shape.x`,
// with the peak), and a slow shimmer for flickering and pulsing lights.
//
// Fragment: a soft round spot (a placed light: a small bright core and a fainter halo, the
// halo's share `glow.extra.w`), or along a shaft a soft profile across and a falloff along
// (from a short soft start at `a` to nothing at `b` when it fades from `a`, the lamp at `a`
// being a round card of its own; else bright in its middle), unlit, as the NIF materials' additive shapes are drawn
// (nvf_assets nif_material.wesl, nif.md §6 "Additive blend", "Fog on added light"): the
// colour, a display value, is made linear (raised to 2.2, `display_emissive`), taken through
// the view's fog and, in a view without HDR (Light glow off), its tonemapping, less black
// through the same (the fog's own colour is not added: the frame behind holds it), and
// added to the opaque frame behind it in display values (`add_display`, the view
// transmission texture the anchor of nif_material.rs keeps in every view). Returned as the
// linear difference with alpha 0 (Bevy's premultiplied blend for AlphaMode::Add adds it).

import bevy_pbr::{
    render::mesh_functions,
    render::mesh_view_bindings::{view, globals},
    render::view_transformations::position_world_to_clip,
};
import bevy_pbr::render::mesh_view_bindings::{view_transmission_texture, view_transmission_sampler};
@if(DISTANCE_FOG) import bevy_pbr::render::mesh_view_bindings::fog;
@if(DISTANCE_FOG) import bevy_pbr::render::pbr_functions::apply_fog;
@if(TONEMAP_IN_SHADER) import bevy_core_pipeline::tonemapping::tone_mapping;

struct GlowParams {
    // x: night share times the peak; y, z: distance where the reach fade starts and ends;
    // w: the smallest half size in pixels.
    shape: vec4<f32>,
    // x: the least light per pixel a widened card keeps; y: shimmer depth; z: the smallest
    // half size in render pixels; w: a placed light's halo share.
    extra: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(0) var<uniform> glow: GlowParams;

struct GlowVertex {
    @builtin(instance_index) instance_index: u32,
    @location(0) position: vec3<f32>,
    @location(1) offset: vec3<f32>,
    @location(2) corner: vec2<f32>,
    @location(3) size: vec2<f32>,
    @location(4) shape: vec4<f32>,
    @location(5) color: vec4<f32>,
}

struct GlowOut {
    @builtin(position) position: vec4<f32>,
    @location(0) world_position: vec3<f32>,
    // Round: the corner (-1..1); shaft: x the offset across in world units (minus and plus
    // the drawn half width at each end), y along from `a` (0) to `b` (1).
    @location(1) corner: vec2<f32>,
    @location(2) color: vec4<f32>,
    // The kind drawn (a shaft drawn round is 0).
    @location(3) @interpolate(flat) kind: f32,
    // A shaft's drawn half widths at `a` and `b` (the same at all four corners).
    @location(4) @interpolate(flat) widths: vec2<f32>,
}

// One output pixel's and one render pixel's size (world units) at distance `d` (the main
// pass's own height under an upscaler; both the same without one). CPU copy: `drawn_size`.
fn pixels(d: f32) -> vec2<f32> {
    let f = max(view.clip_from_view[1][1], 1e-4);
    return vec2<f32>(2.0 * d / (f * max(view.viewport.w, 1.0)), 2.0 * d / (f * max(view.main_pass_viewport.w, 1.0)));
}

// The smallest half size at distance `d`: the output and render pixel floors.
fn floor_half(d: f32) -> f32 {
    let px = pixels(d);
    return max(glow.shape.w * px.x, glow.extra.z * px.y);
}

@vertex
fn vertex(vertex: GlowVertex) -> GlowOut {
    var out: GlowOut;
    let world_from_local = mesh_functions::get_world_from_local(vertex.instance_index);
    let centre = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position, 1.0)).xyz;
    let d = distance(centre, view.world_position.xyz);
    let own = max(vertex.size.x, 1e-3);
    let half = max(own, floor_half(d));
    var share = clamp((own / half) * (own / half), glow.extra.x, 1.0);
    let reach = 1.0 - smoothstep(glow.shape.y, glow.shape.z, d);
    let motion = floor(vertex.size.y);
    let phase = fract(vertex.size.y) * 6.2831853;
    var shimmer: f32 = 1.0;
    if motion > 0.5 {
        // Flicker a little quicker than pulse; both slow (seen from afar).
        let speed = select(0.9, 2.3, motion < 1.5);
        shimmer = 1.0 - glow.extra.y * (0.5 + 0.5 * sin(globals.time * speed + phase));
    }
    let right = view.world_from_view[0].xyz;
    let up = view.world_from_view[1].xyz;
    var world = centre + (right * vertex.corner.x + up * vertex.corner.y) * half;
    var corner = vertex.corner;
    var widths = vec2<f32>(1.0, 1.0);
    var kind = select(0.0, 3.0, vertex.shape.w > 2.5);
    if vertex.shape.w > 0.5 && vertex.shape.w < 2.5 {
        // A shaft, when it shows as one: both ends in front of the camera, its length on
        // screen at least two pixel floors, not seen end-on.
        let a = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position + vertex.offset, 1.0)).xyz;
        let b = mesh_functions::mesh_position_local_to_world(world_from_local, vec4<f32>(vertex.position - vertex.offset, 1.0)).xyz;
        // Decided once for the whole card, from both ends (the same for all four corners):
        // both ends between the near and far planes (clip depth within 0..w), neither seen
        // end-on, and the length on screen at least two floors in output and in render pixels
        // (CPU copies: `axial_corners`, `shaft_shows`).
        let ca = position_world_to_clip(a);
        let cb = position_world_to_clip(b);
        let axis = normalize(b - a);
        let side_a = cross(axis, normalize(view.world_position.xyz - a));
        let side_b = cross(axis, normalize(view.world_position.xyz - b));
        let in_view = ca.w > 0.0 && cb.w > 0.0 && ca.z >= 0.0 && ca.z <= ca.w && cb.z >= 0.0 && cb.z <= cb.w;
        if in_view && dot(side_a, side_a) > 1e-8 && dot(side_b, side_b) > 1e-8 {
            let ndc = (ca.xy / ca.w - cb.xy / cb.w) * 0.5;
            let length_px = length(ndc * view.viewport.zw);
            let length_render = length(ndc * view.main_pass_viewport.zw);
            if length_px >= 2.0 * glow.shape.w && length_render >= 2.0 * glow.extra.z {
                let at_b = vertex.corner.y > 0.0;
                let end = select(a, b, at_b);
                let side = select(side_a, side_b, at_b);
                // Both ends' drawn half widths at every corner, for the fragment's mapping
                // across (`shaft_x`).
                let own = max(vertex.shape.xy, vec2<f32>(1e-3));
                widths = max(own, vec2<f32>(floor_half(distance(a, view.world_position.xyz)), floor_half(distance(b, view.world_position.xyz))));
                let own_w = select(own.x, own.y, at_b);
                let half_w = select(widths.x, widths.y, at_b);
                share = clamp(own_w / half_w, glow.extra.x, 1.0) * vertex.shape.z;
                world = end + normalize(side) * (vertex.corner.x * half_w);
                corner = vec2<f32>(vertex.corner.x * half_w, select(0.0, 1.0, at_b));
                kind = vertex.shape.w;
            }
        }
    }
    out.position = position_world_to_clip(world);
    out.world_position = world;
    out.corner = corner;
    out.kind = kind;
    out.widths = widths;
    out.color = vec4<f32>(vertex.color.rgb * (glow.shape.x * share * reach * shimmer), 1.0);
    return out;
}

// A shaft fragment's across coordinate (-1..1): the interpolated offset across `s` (world
// units) over the drawn half width at `v`, both linear in each of the card's triangles, so
// the profile is centred on the axis at every cross-section, as the CPU's energy integral
// assumes (CPU copy: `shaft_x`).
fn shaft_x(s: f32, v: f32, widths: vec2<f32>) -> f32 {
    return s / max(mix(widths.x, widths.y, clamp(v, 0.0, 1.0)), 1e-6);
}

// How bright a card is at `corner` for its `kind` (1 at its brightest; 0 outside): see the
// notes at the top. CPU copy: `card_profile`.
fn profile(corner: vec2<f32>, kind: f32) -> f32 {
    if kind > 0.5 && kind < 2.5 {
        let x = corner.x;
        if abs(x) >= 1.0 {
            return 0.0;
        }
        let v = clamp(corner.y, 0.0, 1.0);
        let across = exp(-4.0 * x * x) * (1.0 - x * x);
        let along = select(1.0 - (2.0 * v - 1.0) * (2.0 * v - 1.0), smoothstep(0.0, 0.08, v) * (1.0 - v) * (1.0 - v), kind > 1.5);
        return across * along;
    }
    let r2 = dot(corner, corner);
    if r2 >= 1.0 {
        return 0.0;
    }
    if kind > 2.5 {
        let h = glow.extra.w;
        return (1.0 - h) * exp(-14.0 * r2) + h * exp(-3.0 * r2) * (1.0 - r2);
    }
    return exp(-4.0 * r2) * (1.0 - r2);
}

// What ordinary materials' post-lighting step does to a colour (bevy_pbr
// `main_pass_post_lighting_processing`): the view's fog, then, without HDR, the tonemapping.
fn post(c: vec3<f32>, world_position: vec3<f32>, frag: vec2<f32>) -> vec3<f32> {
    var o = vec4<f32>(c, 1.0);
    @if(DISTANCE_FOG) {
        o = apply_fog(fog, o, world_position, view.world_position.xyz, frag);
    }
    @if(TONEMAP_IN_SHADER) {
        o = tone_mapping(o, view.color_grading);
    }
    return o.rgb;
}

// The frame behind a fragment: the view transmission texture at its pixel (the main pass's
// own region under an upscaler; as nif_material.wesl `transmission_uv`).
fn behind(frag: vec2<f32>) -> vec3<f32> {
    let lo = view.main_pass_viewport.xy + 0.5;
    let hi = view.main_pass_viewport.xy + view.main_pass_viewport.zw - 0.5;
    let uv = clamp(frag, lo, hi) / vec2<f32>(textureDimensions(view_transmission_texture));
    return textureSampleLevel(view_transmission_texture, view_transmission_sampler, uv, 0.0).rgb;
}

// The linear light added over the frame `under` (linear) for the linear colour `colour`:
// the sum formed in display values (gamma 2.2), held to 2.0 or the frame's own value if
// brighter (nif_material.wesl `add_display` with alpha 1; CPU mirror nvf_assets
// `nif_material::add_display`).
fn add_display(colour: vec3<f32>, under: vec3<f32>) -> vec3<f32> {
    let b = max(under, vec3<f32>(0.0));
    let shown = pow(b, vec3<f32>(1.0 / 2.2));
    let added = pow(max(colour, vec3<f32>(0.0)), vec3<f32>(1.0 / 2.2));
    let sum = min(shown + added, max(shown, vec3<f32>(2.0)));
    return max(pow(sum, vec3<f32>(2.2)) - b, vec3<f32>(0.0));
}

@fragment
fn fragment(in: GlowOut) -> @location(0) vec4<f32> {
    var corner = in.corner;
    if in.kind > 0.5 && in.kind < 2.5 {
        corner.x = shaft_x(corner.x, corner.y, in.widths);
    }
    let shape = profile(corner, in.kind);
    if shape <= 0.0 {
        discard;
    }
    // The card's profile, in display values.
    let display = in.color.rgb * shape;
    let linear = pow(max(display, vec3<f32>(0.0)), vec3<f32>(2.2));
    let added = max(post(linear, in.world_position, in.position.xy) - post(vec3<f32>(0.0), in.world_position, in.position.xy), vec3<f32>(0.0));
    return vec4<f32>(add_display(added, behind(in.position.xy)), 0.0);
}
