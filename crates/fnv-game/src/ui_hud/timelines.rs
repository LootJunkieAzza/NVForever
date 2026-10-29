//! [design] Tile-only clocks, independent of the hand-built HUD's timing and menu delay.
//! Approximate timelines [observed: private jobs hud_xp_limbs x1-x9, l1-l3;
//! hud_xp_limbs2 x9_level_up_long; hypothesis: linear fades/glide between samples].

use crate::experience::Standing;

#[derive(Clone, Debug)]
pub struct XpFrame {
    pub age: f64,
    pub amount: f64,
    pub level: i32,
    pub from: f64,
    pub to: f64,
    pub level_up: bool,
}

#[derive(Clone, Debug, Default)]
pub struct AnimationFrame {
    pub seconds: f64,
    pub xp: Option<XpFrame>,
    pub limb_age: Option<f64>,
    /// [design] Retain the last displayed broken pictures even after healing.
    pub broken: [bool; 6],
}

#[derive(Default)]
pub struct Animation {
    seen_xp: Option<Standing>,
    seen_limbs: [bool; 6],
    frame: AnimationFrame,
}

impl Animation {
    /// [design] Pause clocks while the HUD is unavailable. A new menu tree resets this
    /// model in the adapter; reset XP/level changes cancel a show. Further gains during
    /// a show update its endpoint, retaining the first amount and clock [observed:
    /// private job hud_xp_limbs x2b; hypothesis: repeat gains outside that one sample].
    pub fn step(&mut self, dt: f64, ticking: bool, standing: Standing, crippled: [bool; 6]) -> AnimationFrame {
        let dt = if ticking && dt.is_finite() { dt.max(0.0) } else { 0.0 };
        self.frame.seconds += dt;
        if let Some(xp) = &mut self.frame.xp {
            xp.age += dt;
            if xp.age >= if xp.level_up { 9.35 } else { 6.0 } {
                self.frame.xp = None;
            }
        }
        if let Some(age) = &mut self.frame.limb_age {
            *age += dt;
            if *age >= 5.0 {
                self.frame.limb_age = None;
            }
        }
        if let Some(before) = self.seen_xp.replace(standing) {
            if standing.level != before.level || standing.xp < before.xp || standing.at_max() {
                self.frame.xp = None;
            } else if standing.xp > before.xp {
                if let Some(show) = &mut self.frame.xp {
                    show.to = f64::from(standing.fraction());
                    show.level_up |= standing.due();
                } else {
                    self.frame.xp = Some(XpFrame {
                        age: 0.0,
                        amount: standing.xp - before.xp,
                        level: standing.level,
                        from: f64::from(standing.fraction_of(before.xp)),
                        to: f64::from(standing.fraction()),
                        level_up: standing.due(),
                    });
                }
            }
        }
        if crippled.iter().zip(self.seen_limbs).any(|(now, old)| *now && !old) {
            self.frame.limb_age = Some(0.0);
            self.frame.broken = crippled;
        } else if crippled != self.seen_limbs {
            // [design] Healing does not start a display; cancel a currently shown one.
            self.frame.limb_age = None;
        }
        self.seen_limbs = crippled;
        self.frame.clone()
    }
}

/// [observed: private job hud_xp_limbs2, x9_level_up_long; hypothesis: linear ramps
/// and exact breakpoints within the 0.33 s sampling interval, 0..9.35 s].
pub(super) fn xp_alpha(age: f64) -> f64 {
    255.0 * (age / 2.0).min((6.0 - age) / 2.0).clamp(0.0, 1.0)
}

pub(super) fn level_alpha(age: f64) -> f64 {
    if age < 7.2 {
        return 255.0 * ((7.03 - age) / 0.67).clamp(0.0, 1.0);
    }
    255.0 * ((age - 7.2) / 0.34).min((9.35 - age) / 0.35).clamp(0.0, 1.0)
}

pub(super) fn limb_alpha(age: f64, broken: bool) -> f64 {
    if broken {
        if age < 4.3 { (age * 470.0).rem_euclid(255.0) } else { -16.0 }
    } else {
        255.0 * (age / 0.85).min((5.0 - age) / 0.85).clamp(0.0, 1.0)
    }
}
