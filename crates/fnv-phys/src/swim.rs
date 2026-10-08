//! Swimming speeds: the stroke cycle (physics.md §6.6).
//!
//! The game's swimmer does not move at a constant speed: from the moment it starts to
//! move it goes at a steady 339 units/s (running; Always Run on) for a little over a third
//! of a second, then speeds up and slows down in a stroke cycle of 23/16 s whose mean is
//! again 339 [observed: `sw_trial` at Lake Mead, per-1/16-s speeds of eight runs, the
//! forward ones agreeing within 1–10 units/s; the same cycle along the view when diving].
//! Backwards and sideways have cycles of their own with the same mean and period. The
//! tables below hold the cycle's mean speed over each 1/16 s as a fraction of 339.
//! Walking (the Run key held) starts at 86 units/s and also varies (82–122 over the 2 s
//! captured, no full cycle seen); it is taken as constant [hypothesis].

/// Running swim speed at the start and on average over a stroke, units/s [observed: 339.0
/// over the first 0.3 s of every direction; the forward cycle averages 339.1].
pub const SWIM_RUN_SPEED: f32 = 339.0;
/// Walking swim speed (the Run key held with Always Run on) at the start, units/s
/// [observed: 85.7–86.3 over the first 0.25 s].
pub const SWIM_WALK_SPEED: f32 = 86.0;
/// Length of a stroke cycle, seconds [observed: 23 sixteenths in every direction; minima
/// 1.4375 s apart over 4 s of swimming].
pub const STROKE_PERIOD: f32 = 23.0 / 16.0;

/// Which stroke the swimmer makes: forward (also diagonals with a forward part, and with
/// the Sneak control on [observed: forward-right and sneak-forward runs follow it]),
/// backward, or sideways [observed: right and left alike].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stroke {
    Forward,
    Back,
    Side,
}

impl Stroke {
    /// The stroke for a movement intent: `forward` +1 ahead, −1 back; `right` sideways.
    pub fn of(forward: f32, right: f32) -> Option<Stroke> {
        if forward > 0.0 {
            Some(Stroke::Forward)
        } else if forward < 0.0 {
            Some(Stroke::Back)
        } else if right != 0.0 {
            Some(Stroke::Side)
        } else {
            None
        }
    }

    /// Time from the start of motion to the start of the first cycle, seconds [observed:
    /// 6/16 forward, 5/16 back and sideways].
    pub fn lead_in(self) -> f32 {
        match self {
            Stroke::Forward => 6.0 / 16.0,
            Stroke::Back | Stroke::Side => 5.0 / 16.0,
        }
    }

    fn table(self) -> &'static [f32; 23] {
        match self {
            Stroke::Forward => &FORWARD,
            Stroke::Back => &BACK,
            Stroke::Side => &SIDE,
        }
    }

    /// Speed factor (of [`SWIM_RUN_SPEED`]) `t` seconds after the swimmer started to move:
    /// 1 during the lead-in, then the cycle, linear between the middles of its sixteenths.
    pub fn factor(self, t: f32) -> f32 {
        let lead = self.lead_in();
        if t.is_nan() || t < lead {
            return 1.0;
        }
        let table = self.table();
        let n = table.len();
        // Position in sixteenths from the middle of the cycle's first sixteenth.
        let x = ((t - lead) * 16.0 - 0.5).rem_euclid(n as f32);
        let i = (x.floor() as usize).min(n - 1);
        let f = x - i as f32;
        table[i] + (table[(i + 1) % n] - table[i]) * f
    }
}

/// Forward stroke: mean speed over each sixteenth of the cycle, as a fraction of 339
/// [observed: average of the forward, dive 30°, 60° and 89° runs of `sw_trial`].
const FORWARD: [f32; 23] = [
    1.0518, 1.0965, 1.1346, 1.1653, 1.1876, 1.2046, 1.2147, 1.2133, 1.2105, 1.1954, 1.1771, 1.1479, 1.1143, 1.0719, 1.0231,
    0.966, 0.9032, 0.8319, 0.752, 0.6679, 0.573, 0.4756, 0.627,
];
/// Backward stroke [observed: one run of `sw_trial`].
const BACK: [f32; 23] = [
    0.9221, 0.9448, 0.9623, 0.9848, 0.9969, 1.0137, 1.0207, 1.037, 1.0576, 1.0684, 1.0837, 1.0847, 1.0906, 1.085, 1.0768,
    1.0611, 1.044, 1.0164, 0.9858, 0.9336, 0.8992, 0.8622, 0.8387,
];
/// Sideways stroke [observed: mean of the right and left runs of `sw_trial`].
const SIDE: [f32; 23] = [
    1.0433, 1.0686, 1.0877, 1.1044, 1.1115, 1.1182, 1.1137, 1.1095, 1.0972, 1.0784, 1.0572, 1.0317, 1.0011, 0.9678, 0.9428,
    0.9199, 0.8998, 0.8827, 0.8687, 0.8616, 0.8524, 0.8474, 0.9338,
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strokes_start_steady_and_average_to_the_base_speed() {
        for s in [Stroke::Forward, Stroke::Back, Stroke::Side] {
            assert_eq!(s.factor(0.0), 1.0);
            assert_eq!(s.factor(s.lead_in() - 1e-4), 1.0);
            // Mean over a cycle, sampled finely: the base speed within 0.5%.
            let n = 2300;
            let mean: f32 = (0..n).map(|i| s.factor(s.lead_in() + STROKE_PERIOD * (i as f32 + 0.5) / n as f32)).sum::<f32>() / n as f32;
            assert!((mean - 1.0).abs() < 0.005, "{s:?} {mean}");
            // Periodic.
            let t = s.lead_in() + 0.4;
            assert!((s.factor(t) - s.factor(t + STROKE_PERIOD)).abs() < 1e-4);
        }
        // The forward stroke peaks at about 412 units/s 0.8 s in.
        assert!((Stroke::Forward.factor(0.8) * SWIM_RUN_SPEED - 411.0).abs() < 3.0);
        assert_eq!(Stroke::of(1.0, 1.0), Some(Stroke::Forward));
        assert_eq!(Stroke::of(-1.0, 0.0), Some(Stroke::Back));
        assert_eq!(Stroke::of(0.0, -1.0), Some(Stroke::Side));
        assert_eq!(Stroke::of(0.0, 0.0), None);
    }
}
