// The stop settle: NVForever's own damped springs (one per channel), not sampled from any
// mod [our choice, 2026-11-23: the 0.8.1 co-op ship carries no mod data]. Each channel is
// `sin(2π f t) · e^(-k t) · (1 - (t/L)²)`, scaled so its largest swing is `peak`: a kick
// that overshoots once or twice and is still by `L` seconds.

/// One channel of the settle: frequency (Hz), largest swing (units or degrees), and the
/// raw shape's own largest value (so `peak` is reached).
struct Settle {
    freq: f32,
    peak: f32,
    raw_peak: f32,
}

/// The settle's length, seconds [our choice].
const STOP_LENGTH: f32 = 0.866_666_7;
/// How fast every channel dies away [our choice].
const STOP_DECAY: f32 = 4.0;

/// Forward (the gun dips toward the eye and back).
const STOP_FORWARD: Settle = Settle { freq: 1.666_666_7, peak: 10.6, raw_peak: 0.579_168 };
const STOP_UP: Settle = Settle { freq: 1.25, peak: -2.5, raw_peak: 0.496_111 };
const STOP_ROLL_DEG: Settle = Settle { freq: 1.851_851_9, peak: -3.0, raw_peak: 0.608_364 };
const STOP_PITCH_DEG: Settle = Settle { freq: 2.173_913, peak: -6.0, raw_peak: 0.650_882 };
