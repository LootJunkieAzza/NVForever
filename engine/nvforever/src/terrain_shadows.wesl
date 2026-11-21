// Terrain shadow heights (#2671, stage T1; terrain_shadows.rs): for every texel of the sun
// heightfield (sun_heightfield.rs, R height, G 1 valid / 0 invalid, row 0 the southernmost),
// the height S under which a receiver over that texel is in the landscape's shadow:
//   S = max over the texels the march visits of [H(texel) - entry(texel) * tan(elevation)],
// entry the horizontal distance at which the ray towards the light enters that texel. Output
// R = S, G = the horizontal distance (world units) to the texel that gave S, both
// NO_SHADOW / 0 where nothing casts; B = the ground's relief around the texel (`relief`,
// world units; 0 for an invalid texel), for the receivers' bias (stage T2). Plain WGSL (no
// imports, no shader defs), so the module test validates exactly this text.
// terrain_shadows.rs `march_texel` mirrors `march` line for line and `texel_relief`
// mirrors `relief`; change both together.

struct Params {
    // Unit horizontal direction towards the light in texel space: x east (+column), y north (+row).
    dir: vec2<f32>,
    // Height lost per texel of horizontal distance: tan(elevation) * texel.
    tan_texel: f32,
    // World units per texel (for G).
    texel: f32,
    // Highest and lowest valid height of the map.
    top: f32,
    bottom: f32,
    // Texels per side.
    side: u32,
    // The most samples per texel (N).
    steps: u32,
    // The first steps' length in texels.
    step_min: f32,
    _pad0: f32,
    _pad1: f32,
    _pad2: f32,
}

const NO_SHADOW: f32 = -1.0e30;
// Texels short of the reach where the samples stop (terrain_shadows.rs `EDGE_INSET`).
const EDGE_INSET: f32 = 1.0e-3;

@group(0) @binding(0) var heights: texture_2d<f32>;
@group(0) @binding(1) var shadow_out: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<uniform> params: Params;

// Where the ray from p along d enters the span [c, c + 1] on one axis (texel units); an axis
// the ray does not move along never limits the entry.
fn axis_entry(c: f32, p: f32, d: f32) -> f32 {
    if abs(d) < 1.0e-6 {
        return -1.0e30;
    }
    let edge = select(c + 1.0, c, d > 0.0);
    return (edge - p) / d;
}

// Horizontal distance (texels) from p along d to the map's edge.
fn edge_distance(p: vec2<f32>, d: vec2<f32>, side: f32) -> f32 {
    var t = 1.0e30;
    if d.x > 1.0e-6 {
        t = min(t, (side - p.x) / d.x);
    } else if d.x < -1.0e-6 {
        t = min(t, -p.x / d.x);
    }
    if d.y > 1.0e-6 {
        t = min(t, (side - p.y) / d.y);
    } else if d.y < -1.0e-6 {
        t = min(t, -p.y / d.y);
    }
    return t;
}

// The march from texel `own`'s centre (terrain_shadows.rs module notes, "The march").
fn march(own: vec2<i32>) -> vec2<f32> {
    let side = i32(params.side);
    let p = vec2<f32>(own) + vec2<f32>(0.5, 0.5);
    let d = params.dir;
    // Past this distance nothing can rise above the lowest valid height, and past the
    // map's edge nothing is known.
    let reach = min(edge_distance(p, d, f32(params.side)), max(params.top - params.bottom, 0.0) / params.tan_texel);
    let n = min(params.steps, u32(ceil(max(reach, 0.0) / params.step_min)));
    let b = max(reach - f32(params.steps) * params.step_min, 0.0) / (f32(params.steps) * f32(params.steps));
    // The last sample lies just inside the reach, never on the map's edge itself (where it
    // would read outside the map and skip the last texel the ray crosses).
    let end = max(reach - EDGE_INSET, 0.0);
    var best = NO_SHADOW;
    var best_t = 0.0;
    var last = own;
    for (var i = 1u; i <= n; i = i + 1u) {
        let fi = f32(i);
        let t = min(params.step_min * fi + b * fi * fi, end);
        let c = vec2<i32>(floor(p + t * d));
        if c.x < 0 || c.y < 0 || c.x >= side || c.y >= side {
            break;
        }
        // The receiver's own texel (its maximum is the receiver's own ground) and a texel
        // already visited are left out.
        if all(c == last) {
            continue;
        }
        last = c;
        let entry = max(max(axis_entry(f32(c.x), p.x, d.x), axis_entry(f32(c.y), p.y, d.y)), 0.0);
        let fall = entry * params.tan_texel;
        // Every texel after this one is entered later: none can beat `best` any more.
        if params.top - fall <= best {
            break;
        }
        let h = textureLoad(heights, c, 0);
        if h.g > 0.5 {
            let s = h.r - fall;
            if s > best {
                best = s;
                best_t = entry;
            }
        }
    }
    if best <= NO_SHADOW {
        return vec2<f32>(NO_SHADOW, 0.0);
    }
    return vec2<f32>(best, best_t * params.texel);
}

// |H(c) - h0| for a valid texel c inside the map; -1 for one outside it or invalid.
fn rise(c: vec2<i32>, h0: f32) -> f32 {
    let side = i32(params.side);
    if c.x < 0 || c.y < 0 || c.x >= side || c.y >= side {
        return -1.0;
    }
    let h = textureLoad(heights, c, 0);
    if h.g <= 0.5 {
        return -1.0;
    }
    return abs(h.r - h0);
}

// The smaller of the height differences to the two neighbours along `along` (one side alone
// when the other is missing, 0 when both are): a uniform slope's rise per texel, but 0 at
// the foot or the top of a cliff, so the bias it gives does not lift a cliff's own shadow
// off the ground at its foot.
fn axis_relief(own: vec2<i32>, along: vec2<i32>, h0: f32) -> f32 {
    let a = rise(own - along, h0);
    let b = rise(own + along, h0);
    if a < 0.0 && b < 0.0 {
        return 0.0;
    }
    if a < 0.0 {
        return b;
    }
    if b < 0.0 {
        return a;
    }
    return min(a, b);
}

// The ground's relief around texel `own` (height h0): the east-west and north-south
// differences summed. On a uniform slope it is the rise per texel, about how far the
// texel's maximum stands above its ground and its neighbours' maxima above a receiver
// between them; the receivers' bias is twice it. It is a heuristic, not an error bound:
// - on terraced ground it can lift a receiver out of a real shadow: sunward maxima 0, 128,
//   256, 384, 384 (constant across rows) under tan(e) = 0.2 give a relief of 128 at the
//   centre of the texel whose maximum is 128, a lift of 2 * 128 + 16 = 272, so a receiver
//   on a flat terrace at 128 is read at 400, above S = 384 - 384 * 0.2 = 307.2 from the
//   uphill 384s (entered 384 units away): lit, though under S (review of f8d532cca,
//   finding 3, derived, not seen in a picture);
// - it sees only the differences between texel maxima: relief inside a texel whose
//   neighbours' maxima are equal gives 0 (a narrow sunlit channel along the light gets no
//   bias while the neighbouring maxima may shade it).
// The bias was chosen against speckle on lit slopes (sun_visibility.rs `lit_slope_has_no_acne`,
// planes only); keeping terrace shadows as well needs better ground-error data than B.
fn relief(own: vec2<i32>, h0: f32) -> f32 {
    return axis_relief(own, vec2<i32>(1, 0), h0) + axis_relief(own, vec2<i32>(0, 1), h0);
}

@compute @workgroup_size(8, 8, 1)
fn produce(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.side || id.y >= params.side {
        return;
    }
    let own = vec2<i32>(id.xy);
    var result = vec4<f32>(NO_SHADOW, 0.0, 0.0, 0.0);
    // An invalid receiver texel gets no shadow and no relief; every texel gets no shadow
    // when there is no light to march towards (steps 0).
    let h0 = textureLoad(heights, own, 0);
    if h0.g > 0.5 {
        result.z = relief(own, h0.r);
        if params.steps > 0u {
            let m = march(own);
            result.x = m.x;
            result.y = m.y;
        }
    }
    textureStore(shadow_out, own, result);
}
