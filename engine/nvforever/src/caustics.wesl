// Sun caustics on submerged opaque receivers [design, #2674].
// All positions and the direction toward the sun use Bevy world space (y up).
//
// The surface is a sum of eight travelling sine waves h(x) = sum a_i sin(theta_i).
// Light refracted through a height field lands displaced by about D grad h, D growing
// with the path length under water; where neighbouring rays converge the light is
// focused, by 1 / |det(I + D Hessian h)| (the Jacobian of that displacement). Both the
// gradient and the Hessian of a sum of sines are analytic: no texture reads. One
// inverse step (entry = receiver - D grad h) bends the fold lines into the cusped,
// webbed network of real caustics instead of smooth contour loops.
//
// Per wave, s_i = D a_i k_i^2 is its focusing strength; it rises with the path and
// falls once the wave would focus well above the receiver, so short waves give a fine
// sharp web in shallow water and long waves a broader, softer one in deep water.
// Directions spread over the half circle; wavelengths 41-239 units; crests travel
// 33-81 units/s (half the deep-water dispersion speed for each wavelength).

// Travel directions (unit vectors in the xz plane), split in two vec4 groups.
const NVF_CAUSTICS_DX_A: vec4<f32> = vec4<f32>(0.9925, 0.5150, -0.6157, -0.9781);
const NVF_CAUSTICS_DY_A: vec4<f32> = vec4<f32>(0.1219, -0.8572, -0.7880, 0.2079);
const NVF_CAUSTICS_DX_B: vec4<f32> = vec4<f32>(-0.1564, -0.8572, 0.8290, 0.2419);
const NVF_CAUSTICS_DY_B: vec4<f32> = vec4<f32>(0.9877, -0.5150, -0.5592, 0.9703);
// Wavelengths in game units (70 units ~ 1 m).
const NVF_CAUSTICS_LAMBDA_A: vec4<f32> = vec4<f32>(41.0, 97.0, 57.0, 178.0);
const NVF_CAUSTICS_LAMBDA_B: vec4<f32> = vec4<f32>(49.0, 131.0, 74.0, 239.0);
// Temporal frequencies in cycles per second.
const NVF_CAUSTICS_RATE_A: vec4<f32> = vec4<f32>(0.8159, 0.5305, 0.6920, 0.3916);
const NVF_CAUSTICS_RATE_B: vec4<f32> = vec4<f32>(0.7464, 0.4565, 0.6073, 0.3379);
// Start phases in cycles.
const NVF_CAUSTICS_PHASE_A: vec4<f32> = vec4<f32>(0.00, 0.27, 0.65, 0.41);
const NVF_CAUSTICS_PHASE_B: vec4<f32> = vec4<f32>(0.84, 0.13, 0.58, 0.92);
// Peak focusing strength of one wave, and the path (in wavelengths) where it peaks.
const NVF_CAUSTICS_STRENGTH: f32 = 0.33;
const NVF_CAUSTICS_FOCUS: f32 = 0.9;
// Softening of the focus singularity; the intensity clamp; gain per unit intensity.
const NVF_CAUSTICS_SOFT: f32 = 0.15;
const NVF_CAUSTICS_PEAK: f32 = 3.2;
const NVF_CAUSTICS_GAIN: f32 = 0.5;
// The intensity of calm water, 1 / sqrt(1 + SOFT^2).
const NVF_CAUSTICS_CALM: f32 = 0.98894;
// Mean clamped intensity above the calm-water value with every wave present (measured
// offline over the depth range; keeps the average brightness of the bed about unchanged).
const NVF_CAUSTICS_MEAN_EXCESS: f32 = 0.05;

// Phases in radians of both wave groups at p.
fn nvf_caustics_phase(p: vec2<f32>, time: f32, dx: vec4<f32>, dy: vec4<f32>, wavelength: vec4<f32>, rate: vec4<f32>, start: vec4<f32>) -> vec4<f32> {
    // Count cycles, wrap, then scale: large world coordinates and long run times keep
    // their precision, and sin/cos see small arguments.
    let cycles = (dx * p.x + dy * p.y) / wavelength + start - rate * time;
    return 6.2831853 * fract(cycles);
}

// Fractional light gain, to multiply the underlying colour by 1 + gain. `footprint` is
// the receiver's pixel size in game units (0 = unknown); waves the pixel cannot resolve
// fade out so the pattern does not alias at a distance or a grazing view.
fn nvf_caustics_filtered(receiver: vec3<f32>, surface_y: f32, sun_dir: vec3<f32>, daylight: f32, time: f32, footprint: f32) -> f32 {
    let depth = surface_y - receiver.y;
    if daylight <= 0.0 || depth <= 0.0 || depth >= 600.0 || sun_dir.y <= 0.0 {
        return 0.0;
    }
    let sun = sun_dir / max(length(sun_dir), 0.001);
    // Trace toward the surface using Snell's law (air/water index ratio 0.75).
    // The refracted vertical component stays safely away from zero at the horizon.
    let horizontal = sun.xz * 0.75;
    let vertical = sqrt(max(1.0 - dot(horizontal, horizontal), 0.001));
    let path_len = depth / vertical;
    let p = receiver.xz + horizontal * path_len;

    // Focusing strength per wave: peaks where the path is FOCUS wavelengths.
    let u_a = path_len / (NVF_CAUSTICS_FOCUS * NVF_CAUSTICS_LAMBDA_A);
    let u_b = path_len / (NVF_CAUSTICS_FOCUS * NVF_CAUSTICS_LAMBDA_B);
    // Band limit: a wave fades as the footprint grows from 1/12 to 1/4 of its wavelength.
    let keep_a = 1.0 - smoothstep(vec4(0.5), vec4(1.5), footprint * 6.2831853 / NVF_CAUSTICS_LAMBDA_A);
    let keep_b = 1.0 - smoothstep(vec4(0.5), vec4(1.5), footprint * 6.2831853 / NVF_CAUSTICS_LAMBDA_B);
    let s_a = NVF_CAUSTICS_STRENGTH * 2.0 * u_a / (1.0 + u_a * u_a) * keep_a;
    let s_b = NVF_CAUSTICS_STRENGTH * 2.0 * u_b / (1.0 + u_b * u_b) * keep_b;

    // One inverse step of the refraction displacement: D grad h = sum s_i / k_i d_i cos.
    let pull_a = s_a * NVF_CAUSTICS_LAMBDA_A * 0.15915494 * cos(nvf_caustics_phase(p, time, NVF_CAUSTICS_DX_A, NVF_CAUSTICS_DY_A, NVF_CAUSTICS_LAMBDA_A, NVF_CAUSTICS_RATE_A, NVF_CAUSTICS_PHASE_A));
    let pull_b = s_b * NVF_CAUSTICS_LAMBDA_B * 0.15915494 * cos(nvf_caustics_phase(p, time, NVF_CAUSTICS_DX_B, NVF_CAUSTICS_DY_B, NVF_CAUSTICS_LAMBDA_B, NVF_CAUSTICS_RATE_B, NVF_CAUSTICS_PHASE_B));
    let entry_xz = p - vec2(
        dot(pull_a, NVF_CAUSTICS_DX_A) + dot(pull_b, NVF_CAUSTICS_DX_B),
        dot(pull_a, NVF_CAUSTICS_DY_A) + dot(pull_b, NVF_CAUSTICS_DY_B),
    );

    // D Hessian h at the entry point: -sum s_i d_i d_i^T sin.
    let bend_a = s_a * sin(nvf_caustics_phase(entry_xz, time, NVF_CAUSTICS_DX_A, NVF_CAUSTICS_DY_A, NVF_CAUSTICS_LAMBDA_A, NVF_CAUSTICS_RATE_A, NVF_CAUSTICS_PHASE_A));
    let bend_b = s_b * sin(nvf_caustics_phase(entry_xz, time, NVF_CAUSTICS_DX_B, NVF_CAUSTICS_DY_B, NVF_CAUSTICS_LAMBDA_B, NVF_CAUSTICS_RATE_B, NVF_CAUSTICS_PHASE_B));
    let hxx = -(dot(bend_a, NVF_CAUSTICS_DX_A * NVF_CAUSTICS_DX_A) + dot(bend_b, NVF_CAUSTICS_DX_B * NVF_CAUSTICS_DX_B));
    let hxy = -(dot(bend_a, NVF_CAUSTICS_DX_A * NVF_CAUSTICS_DY_A) + dot(bend_b, NVF_CAUSTICS_DX_B * NVF_CAUSTICS_DY_B));
    let hyy = -(dot(bend_a, NVF_CAUSTICS_DY_A * NVF_CAUSTICS_DY_A) + dot(bend_b, NVF_CAUSTICS_DY_B * NVF_CAUSTICS_DY_B));
    let jacobian = (1.0 + hxx) * (1.0 + hyy) - hxy * hxy;
    let intensity = min(inverseSqrt(jacobian * jacobian + NVF_CAUSTICS_SOFT * NVF_CAUSTICS_SOFT), NVF_CAUSTICS_PEAK);

    // Subtract the mean so the gain averages near zero: the calm-water value plus the
    // excess of the waves still present.
    let wave_share = (dot(keep_a, vec4(1.0)) + dot(keep_b, vec4(1.0))) * 0.125;
    let gain = max(NVF_CAUSTICS_GAIN * (intensity - NVF_CAUSTICS_CALM - NVF_CAUSTICS_MEAN_EXCESS * wave_share), -0.15);

    let envelope = smoothstep(0.0, 2.0, depth) * (1.0 - smoothstep(200.0, 600.0, depth));
    return gain * envelope * clamp(daylight, 0.0, 1.0) * smoothstep(0.0, 0.18, sun.y);
}

// Unfiltered: for callers that do not know the receiver's pixel footprint.
fn nvf_caustics(receiver: vec3<f32>, surface_y: f32, sun_dir: vec3<f32>, daylight: f32, time: f32) -> f32 {
    return nvf_caustics_filtered(receiver, surface_y, sun_dir, daylight, time, 0.0);
}
