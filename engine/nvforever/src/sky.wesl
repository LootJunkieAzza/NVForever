// Sky parts (sky.rs): Bevy's unlit standard fragment, then the colour is mapped so that
// the camera's tonemapper (Tony McMapface) gives back the sky's display value.
//
// The weather's sky colours are display values that the game grades directly; the
// renderer's lit surfaces go through the tonemapper first. TONY_IN / TONY_OUT sample
// the tonemapper's response to grey (input linear, output linear), measured from
// renders of a flat grey sky without the grade [observed: Bevy 0.19.1 Tony McMapface];
// the inverse is piecewise linear between the samples, per channel (Tony McMapface also
// shifts saturated colours, which this ignores).

import bevy_pbr::{
    render::pbr_fragment::pbr_input_from_standard_material,
    render::pbr_functions::{alpha_discard, premultiply_alpha},
    render::forward_io::{VertexOutput, FragmentOutput},
    render::pbr_types::{STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS, STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD},
    render::mesh_view_bindings::{view, globals},
    render::mesh_functions::get_world_from_local,
};

// sky.rs SkyLight: daylight on the sky's colours, and the sun.
struct SkyLight {
    // x scale, y 1 additive weighted by texture alpha, z 1 if the light applies to this
    // part, w 2 sun disc and glare, 1 glare.
    params: vec4<f32>,
    glow: vec4<f32>,
    sun_dir: vec4<f32>,
    // rgb colour × strength, w tan(half size)
    sun: vec4<f32>,
    glare: vec4<f32>,
    // Night sky detail (night_sky.rs; all zero with its switch off): xyz where the moon disc is
    // drawn from the sky's centre (sky.rs SUN_DISTANCE out), w the Henyey-Greenstein g of the
    // moon's lobe.
    moon: vec4<f32>,
    // By part: the stars x the twinkle's amplitude, y lattice cells per unit of direction;
    // the dome and clouds rgb the moon term's gain, w its cap (0: none).
    night: vec4<f32>,
}

@group(constants::MATERIAL_BIND_GROUP) @binding(100) var<uniform> sky: SkyLight;
@group(constants::MATERIAL_BIND_GROUP) @binding(101) var sun_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(102) var sun_sampler: sampler;
@group(constants::MATERIAL_BIND_GROUP) @binding(103) var glare_texture: texture_2d<f32>;
@group(constants::MATERIAL_BIND_GROUP) @binding(104) var glare_sampler: sampler;

// A texture spread over the angular square of half-size atan(half_tan) around the sun
// (as on a billboard facing the camera), added as colour × alpha.
fn sun_texel(t: texture_2d<f32>, s: sampler, dir: vec3<f32>, half_tan: f32) -> vec3<f32> {
    let sd = normalize(sky.sun_dir.xyz);
    let c = dot(dir, sd);
    if c <= 0.0 {
        return vec3(0.0);
    }
    // Point on the plane through the sun's direction, perpendicular to it.
    let p = dir / c - sd;
    var up = vec3(0.0, 1.0, 0.0);
    if abs(sd.y) > 0.999 {
        up = vec3(0.0, 0.0, 1.0);
    }
    let u = normalize(cross(up, sd));
    let v = cross(sd, u);
    let uv = vec2(dot(p, u), -dot(p, v)) / (2.0 * half_tan) + 0.5;
    if any(uv < vec2(0.0)) || any(uv > vec2(1.0)) {
        return vec3(0.0);
    }
    let texel = textureSampleLevel(t, s, uv, 0.0);
    return texel.rgb * texel.a;
}

const N: u32 = 24u;
var<private> TONY_IN: array<f32, 24> = array<f32, 24>(0.0, 0.005, 0.01, 0.02, 0.04, 0.07, 0.1, 0.15, 0.2, 0.3, 0.4, 0.5, 0.6, 0.8, 1.0, 1.3, 1.7, 2.2, 3.0, 5.0, 8.0, 16.0, 40.0, 100.0);
var<private> TONY_OUT: array<f32, 24> = array<f32, 24>(0.0, 0.0056, 0.0113, 0.0222, 0.0437, 0.0723, 0.0999, 0.1470, 0.1812, 0.2542, 0.3140, 0.3663, 0.4125, 0.4910, 0.5457, 0.6172, 0.6867, 0.7454, 0.8070, 0.8796, 0.9216, 0.9473, 0.9771, 0.9875);

// Linear input that the tonemapper maps to the linear output `y`; `y` itself when the
// view shows display values clipped (sky.glow.w 1, nvforever lights.rs GAME_CLIP).
fn tony_inverse(y: f32) -> f32 {
    if sky.glow.w > 0.5 {
        return y;
    }
    for (var i = 1u; i < N; i++) {
        if y <= TONY_OUT[i] {
            let t = (y - TONY_OUT[i - 1u]) / max(TONY_OUT[i] - TONY_OUT[i - 1u], 1e-6);
            return mix(TONY_IN[i - 1u], TONY_IN[i], t);
        }
    }
    return TONY_IN[N - 1u];
}

// ---- Night sky detail (night_sky.rs, docs/design/graphics-enhanced.md §3.19) ----
// An engineering choice for the enhanced look, not the original's behaviour. Every value
// comes from `sky.moon` / `sky.night`, zero with the switch off (the branches are skipped).

const NIGHT_TAU: f32 = 6.2831855;
// Below this added light (linear) a star fragment is not twinkled (most of the sky).
const NIGHT_TWINKLE_SKIP: f32 = 0.0001;

// MurmurHash3's 32-bit finaliser (fmix32, A. Appleby's published constants).
fn night_mix(x: u32) -> u32 {
    var h = x;
    h ^= h >> 16u;
    h *= 0x85ebca6bu;
    h ^= h >> 13u;
    h *= 0xc2b2ae35u;
    h ^= h >> 16u;
    return h;
}

// A lattice cell's hash: the cell's coordinates times Teschner et al.'s primes ("Optimized
// Spatial Hashing for Collision Detection of Deformable Objects", 2003), xor-ed, then mixed.
fn night_cell_hash(c: vec3<i32>) -> u32 {
    let u = bitcast<vec3<u32>>(c);
    return night_mix((u.x * 73856093u) ^ (u.y * 19349663u) ^ (u.z * 83492791u));
}

// One lattice corner's wave at `hour` (the clock's fraction of its one-hour wrap): the mean of
// two sines whose phases and rates (3600..14400 whole cycles an hour, 1 to 4 Hz) come from the
// corner's hash. Whole cycles an hour, so the clock's wrap at 3600 s joins without a jump.
// Mean 0 over time, within -1..1.
fn night_wave(cell: vec3<i32>, hour: f32) -> f32 {
    let h = night_cell_hash(cell);
    let r = night_mix(h ^ 0x9e3779b9u);
    let k1 = f32(3600u + r % 10801u);
    let k2 = f32(3600u + (r >> 16u) % 10801u);
    let p1 = f32(h & 0xffffu) / 65536.0;
    let p2 = f32(h >> 16u) / 65536.0;
    return 0.5 * (sin(NIGHT_TAU * fract(k1 * hour + p1)) + sin(NIGHT_TAU * fract(k2 * hour + p2)));
}

// The sky part's centre in the world: the main camera's place, which every sky part follows
// (sky.rs). Another camera (the water reflection's, mirrored below the water) sees the sky
// from elsewhere, so what belongs to the dome is measured from here, not from the eye.
fn night_centre(instance_index: u32) -> vec3<f32> {
    return get_world_from_local(instance_index)[3].xyz;
}

// The factor on a star's added light along the dome direction `dir` (world axes, from the
// star dome's centre; the dome never turns, so the lattice is anchored to it, not to the
// screen or the eye): lattice value
// noise in time (the corner waves, smoothstep-interpolated over the 3D cell of dir × cells),
// times the amplitude, times 1 - 1/X with X Rozenberg's air mass, 0 at the zenith and 0.975
// at the horizon. Mean 1 over time; within 1 ± amplitude.
fn night_twinkle(dir: vec3<f32>) -> f32 {
    let p = dir * sky.night.y;
    let cell = floor(p);
    let f = p - cell;
    let w = f * f * (3.0 - 2.0 * f);
    let c0 = vec3<i32>(cell);
    let hour = globals.time / 3600.0;
    var s = 0.0;
    for (var i = 0u; i < 8u; i++) {
        let o = vec3<u32>(i & 1u, (i >> 1u) & 1u, (i >> 2u) & 1u);
        let wo = select(vec3(1.0) - w, w, o == vec3(1u));
        s += wo.x * wo.y * wo.z * night_wave(c0 + vec3<i32>(o), hour);
    }
    let up = clamp(dir.y, 0.0, 1.0);
    let air = clamp(1.0 - (up + 0.025 * exp(-11.0 * up)), 0.0, 1.0);
    return 1.0 + clamp(sky.night.x, 0.0, 1.0) * air * s;
}

// The moon's light added along the view ray `dir` to a part whose colour is `base` (1 on the
// dome: the glow; the cloud's own colour on the clouds): `night.rgb` × a Henyey-Greenstein
// lobe about `to_moon`, the ray from the eye to the disc's centre, divided by its value on the
// axis (1 there), the result scaled down so that no channel exceeds `night.w`.
fn night_moon(base: vec3<f32>, dir: vec3<f32>, to_moon: vec3<f32>) -> vec3<f32> {
    if dot(to_moon, to_moon) < 1e-8 {
        return vec3(0.0);
    }
    let g = clamp(sky.moon.w, 0.0, 0.99);
    let cos_t = dot(dir, normalize(to_moon));
    // (1 - g)² + 2 g (1 - cos): 1 + g² - 2 g cos without its cancellation near the axis.
    let d = max((1.0 - g) * (1.0 - g) + 2.0 * g * (1.0 - cos_t), 1e-6);
    let lobe = pow(min((1.0 - g) * (1.0 - g) / d, 1.0), 1.5);
    var add = base * sky.night.rgb * lobe;
    let most = max(add.r, max(add.g, add.b));
    if most > sky.night.w {
        add *= sky.night.w / most;
    }
    return max(add, vec3(0.0));
}

@fragment
fn fragment(in: VertexOutput, @builtin(front_facing) is_front: bool) -> FragmentOutput {
    var pbr_input = pbr_input_from_standard_material(in, is_front);
    pbr_input.material.base_color = alpha_discard(pbr_input.material.flags, pbr_input.material.alpha_cutoff, pbr_input.material.base_color);
    var c = pbr_input.material.base_color;
    if sky.params.z > 0.5 {
        c = vec4(c.rgb * sky.params.x + sky.glow.rgb, c.a);
    }
    if sky.params.w > 0.5 {
        // The sky is centred on the camera: the view ray is the fragment's direction.
        let dir = normalize(in.world_position.xyz - view.world_position.xyz);
        var add = sky.glare.rgb * sun_texel(glare_texture, glare_sampler, dir, sky.glare.w);
        if sky.params.w > 1.5 {
            add += sky.sun.rgb * sun_texel(sun_texture, sun_sampler, dir, sky.sun.w);
        }
        // Night sky detail: the moon's glow on the dome (behind the clouds), moonlight on the
        // clouds (their colour; the blend weighs it by the texture's alpha).
        // Aimed from this eye at the disc (drawn at the sky's centre + moon.xyz), so a mirrored
        // eye sees the glow round the disc. The clouds' own colour without the lightning flash
        // in `glow` (the flash lights the sky once, not again through the moon).
        if sky.night.w > 0.0 {
            let to_moon = night_centre(in.instance_index) + sky.moon.xyz - view.world_position.xyz;
            let own = pbr_input.material.base_color.rgb * sky.params.x;
            add += night_moon(select(own, vec3(1.0), sky.params.w > 1.5), dir, to_moon);
        }
        c = vec4(c.rgb + add, c.a);
    }
    var out: FragmentOutput;
    let alpha_mode = pbr_input.material.flags & STANDARD_MATERIAL_FLAGS_ALPHA_MODE_RESERVED_BITS;
    if alpha_mode == STANDARD_MATERIAL_FLAGS_ALPHA_MODE_ADD {
        // Additive parts: map what is added (colour, × alpha when params.y is 1), not the colour.
        var a = c.rgb * select(1.0, c.a, sky.params.y > 0.5);
        // Night sky detail: the stars (the additive parts the light does not apply to) twinkle.
        if sky.params.z < 0.5 && sky.night.x > 0.0 && max(a.r, max(a.g, a.b)) > NIGHT_TWINKLE_SKIP {
            let dir = normalize(in.world_position.xyz - night_centre(in.instance_index));
            a *= night_twinkle(dir);
        }
        out.color = vec4(tony_inverse(a.r), tony_inverse(a.g), tony_inverse(a.b), 1.0);
    } else {
        out.color = vec4(tony_inverse(c.r), tony_inverse(c.g), tony_inverse(c.b), c.a);
    }
    @if(PREMULTIPLY_ALPHA) out.color = premultiply_alpha(pbr_input.material.flags, out.color);
    return out;
}
