// Image-space grade (imagespace.rs): saturation, tint, brightness and contrast on the
// display (sRGB) values of the tonemapped picture [hypothesis, see imagespace.rs], with the
// screen effects of image space modifiers: double vision and radial blur on the picture
// read, the fade colour over the graded result (docs/spec-notes/image-modifiers.md §5).

#import bevy_core_pipeline::fullscreen_vertex_shader::FullscreenVertexOutput

struct Grade {
    tint: vec4<f32>,
    fade: vec4<f32>,
    // Radial blur: strength, ramp up, start, ramp down.
    radial: vec4<f32>,
    // Radial blur down start, centre x, centre y; double vision strength.
    radial2: vec4<f32>,
    saturation: f32,
    contrast: f32,
    contrast_avg_lum: f32,
    brightness: f32,
    blur: f32,
    time: f32,
    sharp: f32,
}

@group(0) @binding(0) var screen: texture_2d<f32>;
@group(0) @binding(1) var screen_sampler: sampler;
@group(0) @binding(2) var<uniform> grade: Grade;

// Radial blur: how far a point's samples reach towards the centre, as a part of its
// distance from the centre, per unit of strength [hypothesis: the wiki calls strength 2.5
// moderate and 5 strong].
const RADIAL_REACH: f32 = 0.02;
const RADIAL_TAPS: i32 = 12;

fn to_srgb(c: vec3<f32>) -> vec3<f32> {
    return select(1.055 * pow(c, vec3(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3(0.0031308));
}

fn to_linear(c: vec3<f32>) -> vec3<f32> {
    return select(pow((c + 0.055) / 1.055, vec3(2.4)), c / 12.92, c <= vec3(0.04045));
}

fn scene(uv: vec2<f32>) -> vec3<f32> {
    return textureSampleLevel(screen, screen_sampler, clamp(uv, vec2(0.0), vec2(1.0)), 0.0).rgb;
}

// The picture at a point, doubled: two copies `half` apart to each side.
fn doubled(uv: vec2<f32>, half: f32) -> vec3<f32> {
    if half <= 0.0 {
        return scene(uv);
    }
    return 0.5 * (scene(uv + vec2(half, 0.0)) + scene(uv - vec2(half, 0.0)));
}

// How strongly a point is blurred: nothing inside `start` (a distance from the centre, 1
// at the far corner of a centred screen), rising at a rate set by ramp up (0 never, 1 at
// once), and falling again from `down start` at a rate set by ramp down.
fn radial_amount(uv: vec2<f32>, centre: vec2<f32>) -> f32 {
    let d = length((uv - centre) * 2.0) / 1.41421356;
    let up_rate = grade.radial.y;
    var up = 0.0;
    if up_rate >= 1.0 {
        up = step(grade.radial.z, d);
    } else if up_rate > 0.0 {
        up = clamp((d - grade.radial.z) * up_rate / (1.0 - up_rate), 0.0, 1.0);
    }
    let down_rate = grade.radial.w;
    var down = 1.0;
    if down_rate >= 1.0 {
        down = 1.0 - step(grade.radial2.x, d);
    } else if down_rate > 0.0 {
        down = 1.0 - clamp((d - grade.radial2.x) * down_rate / (1.0 - down_rate), 0.0, 1.0);
    }
    return grade.radial.x * up * down;
}

@fragment
fn fragment(in: FullscreenVertexOutput) -> @location(0) vec4<f32> {
    let src = textureSample(screen, screen_sampler, in.uv);
    var lin = src.rgb;
    let double_vision = grade.radial2.w;
    if double_vision > 0.0 || grade.radial.x > 0.0 {
        // The two copies swing between 0.2 and 1 of the strength, as a part of the screen
        // width [hypothesis: the wiki says the effect animates by itself].
        let half = 0.5 * double_vision * (0.6 + 0.4 * sin(grade.time * 1.5));
        let centre = grade.radial2.yz;
        let amount = radial_amount(in.uv, centre) * RADIAL_REACH;
        if amount > 0.0 {
            var acc = vec3(0.0);
            for (var i = 0; i < RADIAL_TAPS; i++) {
                let t = f32(i) / f32(RADIAL_TAPS - 1);
                acc += doubled(centre + (in.uv - centre) * (1.0 - amount * t), half);
            }
            lin = acc / f32(RADIAL_TAPS);
        } else {
            lin = doubled(in.uv, half);
        }
    }
    var c = to_srgb(clamp(lin, vec3(0.0), vec3(1.0)));
    let lum = dot(c, vec3(0.2125, 0.7154, 0.0721));
    c = mix(vec3(lum), c, grade.saturation);
    c = mix(c, lum * grade.tint.rgb, grade.tint.a);
    c = c * grade.brightness;
    c = mix(vec3(grade.contrast_avg_lum), c, grade.contrast);
    c = clamp(c, vec3(0.0), vec3(1.0));
    // The fade colour covers the graded picture [observed: the opening is white while a
    // modifier holds the brightness at zero, image-modifiers.md §5].
    c = mix(c, grade.fade.rgb, clamp(grade.fade.a, 0.0, 1.0));
    return vec4(to_linear(clamp(c, vec3(0.0), vec3(1.0))), src.a);
}
