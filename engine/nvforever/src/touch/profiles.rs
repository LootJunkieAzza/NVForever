//! Original control palettes and placement presets. No bindings or game state change.
#[derive(Clone, Debug, PartialEq)]
pub struct Appearance {
    pub theme: String,
    pub shape: String,
    /// None keeps the existing global SVG-family setting authoritative.
    pub family: Option<String>,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            theme: "glass".into(),
            shape: "circle".into(),
            family: None,
        }
    }
}
#[derive(Clone, Copy)]
pub struct Theme {
    pub id: &'static str,
    pub label: &'static str,
    pub background: [f32; 3],
    pub foreground: [f32; 3],
    pub border: [f32; 3],
    pub pressed: [f32; 3],
    pub fill: f32,
}
/// Shared outer button boundary, including the border. The joystick does not use this.
#[derive(Clone, Copy, Debug)]
pub struct ButtonGeometry {
    pub width: f32,
    pub height: f32,
    pub radius: f32,
}
impl ButtonGeometry {
    pub fn contains(self, offset: [f32; 2]) -> bool {
        if !offset.into_iter().all(f32::is_finite) || self.width <= 0.0 || self.height <= 0.0 {
            return false;
        }
        let [x, y] = offset.map(f32::abs);
        if x > self.width * 0.5 || y > self.height * 0.5 {
            return false;
        }
        let x = (x - (self.width * 0.5 - self.radius)).max(0.0);
        let y = (y - (self.height * 0.5 - self.radius)).max(0.0);
        // Inclusive analytic edge, allowing only a few floating-point ULPs.
        let squared = self.radius * self.radius;
        x * x + y * y <= squared + f32::EPSILON * squared.max(1.0) * 4.0
    }
}
pub const THEMES: &[Theme] = &[
    Theme {
        id: "glass",
        label: "Glass",
        background: [0.02, 0.06, 0.07],
        foreground: [1.0, 1.0, 1.0],
        border: [0.7, 0.9, 0.85],
        pressed: [0.18, 0.55, 0.43],
        fill: 1.0,
    },
    Theme {
        id: "outline",
        label: "Outline",
        background: [0.0, 0.0, 0.0],
        foreground: [1.0, 1.0, 1.0],
        border: [1.0, 1.0, 1.0],
        pressed: [0.3, 0.5, 0.6],
        fill: 0.0,
    },
    Theme {
        id: "slate",
        label: "Slate",
        background: [0.06, 0.08, 0.12],
        foreground: [0.9, 0.95, 1.0],
        border: [0.48, 0.7, 0.95],
        pressed: [0.16, 0.36, 0.65],
        fill: 1.0,
    },
    Theme {
        id: "amber",
        label: "Amber",
        background: [0.10, 0.06, 0.01],
        foreground: [1.0, 0.78, 0.3],
        border: [1.0, 0.65, 0.16],
        pressed: [0.46, 0.25, 0.02],
        fill: 0.85,
    },
    Theme {
        id: "mint",
        label: "Mint",
        background: [0.01, 0.09, 0.06],
        foreground: [0.72, 1.0, 0.85],
        border: [0.35, 0.95, 0.7],
        pressed: [0.05, 0.4, 0.26],
        fill: 0.8,
    },
    Theme {
        id: "frost",
        label: "Frost",
        background: [0.88, 0.94, 1.0],
        foreground: [0.02, 0.06, 0.12],
        border: [0.15, 0.35, 0.6],
        pressed: [0.55, 0.75, 0.96],
        fill: 1.0,
    },
    Theme {
        id: "ember",
        label: "Ember",
        background: [0.12, 0.025, 0.035],
        foreground: [1.0, 0.86, 0.83],
        border: [1.0, 0.4, 0.3],
        pressed: [0.5, 0.08, 0.05],
        fill: 0.9,
    },
    Theme {
        id: "contrast",
        label: "High contrast",
        background: [0.0, 0.0, 0.0],
        foreground: [1.0, 1.0, 1.0],
        border: [1.0, 1.0, 1.0],
        pressed: [0.24, 0.24, 0.24],
        fill: 1.0,
    },
];
pub const SHAPES: &[(&str, &str)] = &[
    ("circle", "Circle"),
    ("rounded", "Rounded"),
    ("square", "Square"),
    ("capsule", "Capsule"),
];
pub const PRESETS: &[(&str, &str)] = &[
    ("classic", "Classic"),
    ("explorer", "Explorer"),
    ("combat", "Combat"),
    ("left", "Left handed"),
    ("claw", "Claw"),
    ("compact", "Compact"),
    ("wide", "Wide reach"),
    ("controller", "Controller"),
];
pub const SLOT_NAMES: &[&str] = &[
    "My layout",
    "Relaxed",
    "Precision",
    "Travel",
    "Close reach",
    "Two thumbs",
    "Four fingers",
    "Custom",
];
pub const MAX_SLOTS: usize = 8;
pub const MAX_FILE_BYTES: u64 = 128 * 1024;
pub const ACTIONS: [&str; 12] = [
    "Theme",
    "Shape",
    "Icons",
    "Preset",
    "Slot",
    "Load slot",
    "Store slot",
    "Rename slot",
    "Reset layout",
    "Save",
    "Cancel",
    "Layout",
];

pub fn cycle<'a>(current: &str, ids: impl IntoIterator<Item = &'a str>) -> &'a str {
    let values: Vec<_> = ids.into_iter().collect();
    let next = values
        .iter()
        .position(|v| *v == current)
        .map_or(0, |i| (i + 1) % values.len());
    values[next]
}
impl Appearance {
    pub fn palette(&self) -> Theme {
        *THEMES
            .iter()
            .find(|t| t.id == self.theme)
            .unwrap_or(&THEMES[0])
    }
    pub fn next_theme(&mut self) {
        self.theme = cycle(&self.theme, THEMES.iter().map(|t| t.id)).into();
    }
    pub fn next_shape(&mut self) {
        self.shape = cycle(&self.shape, SHAPES.iter().map(|s| s.0)).into();
    }
    pub fn shape_label(&self) -> &str {
        SHAPES
            .iter()
            .find(|s| s.0 == self.shape)
            .map_or("Circle", |s| s.1)
    }
    pub fn geometry(&self, diameter: f32) -> ButtonGeometry {
        if !diameter.is_finite() || diameter <= 0.0 {
            return ButtonGeometry {
                width: 0.0,
                height: 0.0,
                radius: 0.0,
            };
        }
        let height = if self.shape == "capsule" {
            (diameter * 0.72).max(48.0).min(diameter)
        } else {
            diameter
        };
        let radius = match self.shape.as_str() {
            "square" => 0.0,
            "rounded" => diameter * 0.2,
            _ => diameter * 0.5,
        }
        .min(height * 0.5);
        ButtonGeometry {
            width: diameter,
            height,
            radius,
        }
    }
}
/// Only established gameplay IDs are repositioned; menu and future actions keep their positions.
pub fn position(preset: &str, id: usize, original: [f32; 2]) -> [f32; 2] {
    let mut p = original;
    match preset {
        "explorer" if (3..=8).contains(&id) => {
            p[0] -= 0.025;
            p[1] -= 0.025;
        }
        "combat" if (3..=8).contains(&id) && id.is_multiple_of(2) => p[0] -= 0.04,
        "claw" if id == 7 => p = [0.10, 0.23],
        "claw" if id == 6 => p = [0.25, 0.23],
        "compact" if (3..=8).contains(&id) => p[0] -= 0.045,
        "wide" if (3..=8).contains(&id) && id.is_multiple_of(2) => p[0] -= 0.08,
        "controller" if id < 3 => p[0] -= 0.08,
        _ => {}
    }
    if preset == "left" && id < 9 {
        p[0] = 1.0 - p[0];
    }
    p
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_shape_uses_its_analytic_visible_boundary() {
        let d = 120.0;
        for (shape, _) in SHAPES {
            let appearance = Appearance { shape: (*shape).into(), ..Appearance::default() };
            let g = appearance.geometry(d);
            assert!(g.contains([0.0, 0.0]), "{shape}");
            assert!(g.contains([g.width * 0.5, 0.0]), "{shape}");
            assert!(g.contains([0.0, g.height * 0.5]), "{shape}");
            assert!(!g.contains([g.width * 0.5 + 0.01, 0.0]), "{shape}");
            assert!(!g.contains([0.0, g.height * 0.5 + 0.01]), "{shape}");
            if g.radius == 0.0 {
                assert!(g.contains([g.width * 0.5, g.height * 0.5]));
            } else {
                let x = g.width * 0.5 - g.radius + g.radius * std::f32::consts::FRAC_1_SQRT_2;
                let y = g.height * 0.5 - g.radius + g.radius * std::f32::consts::FRAC_1_SQRT_2;
                for sx in [-1.0, 1.0] {
                    for sy in [-1.0, 1.0] {
                        assert!(
                            g.contains([sx * (x - 0.01), sy * (y - 0.01)]),
                            "inside {shape}"
                        );
                        assert!(g.contains([sx * x, sy * y]), "edge {shape}");
                        assert!(
                            !g.contains([sx * (x + 0.01), sy * (y + 0.01)]),
                            "outside {shape}"
                        );
                    }
                }
                assert!(
                    !g.contains([g.width * 0.5, g.height * 0.5]),
                    "cut corner {shape}"
                );
            }
        }
    }
    #[test]
    fn capsule_does_not_accept_the_invisible_circle_above_its_top() {
        let a = Appearance { shape: "capsule".into(), ..Appearance::default() };
        assert!(!a.geometry(120.0).contains([0.0, 55.0]));
        assert_eq!(a.geometry(120.0).height, 86.4);
        for d in [24.0, 48.0, 60.0] {
            let g = a.geometry(d);
            assert!(g.height <= g.width && g.radius <= g.height * 0.5);
        }
        assert!(!a.geometry(f32::NAN).contains([0.0, 0.0]));
        assert!(!a.geometry(120.0).contains([f32::NAN, 0.0]));
    }
    #[test]
    fn every_choice_cycles_without_losing_the_first_choice() {
        let mut a = Appearance::default();
        for _ in THEMES {
            a.next_theme();
        }
        assert_eq!(a, Appearance::default());
        for _ in SHAPES {
            a.next_shape();
        }
        assert_eq!(a, Appearance::default());
        a.theme = "future".into();
        assert_eq!(a.palette().id, "glass");
        a.next_theme();
        assert_eq!(a.theme, "glass");
    }
    #[test]
    fn palettes_are_finite_bounded_and_have_readable_foreground_contrast() {
        for t in THEMES {
            for c in [t.background, t.foreground, t.border, t.pressed] {
                assert!(
                    c.into_iter()
                        .all(|n| n.is_finite() && (0.0..=1.0).contains(&n))
                );
            }
            let luminance = |c: [f32; 3]| {
                let c = c.map(|v| {
                    if v <= 0.04045 {
                        v / 12.92
                    } else {
                        ((v + 0.055) / 1.055).powf(2.4)
                    }
                });
                c[0] * 0.2126 + c[1] * 0.7152 + c[2] * 0.0722
            };
            let a = luminance(t.background);
            let b = luminance(t.foreground);
            assert!((a.max(b) + 0.05) / (a.min(b) + 0.05) >= 4.5, "{}", t.id);
        }
    }
    #[test]
    fn handedness_mirrors_without_rebinding_or_moving_future_actions() {
        assert_eq!(position("left", 7, [0.9, 0.8]), [1.0 - 0.9, 0.8]);
        for (id, _) in PRESETS {
            assert_eq!(position(id, 18, [0.41, 0.32]), [0.41, 0.32]);
        }
    }
    #[test]
    fn gameplay_presets_remain_in_normalized_space() {
        let base = [
            [0.935, 0.09],
            [0.815, 0.09],
            [0.695, 0.09],
            [0.935, 0.62],
            [0.815, 0.62],
            [0.935, 0.76],
            [0.815, 0.76],
            [0.935, 0.90],
            [0.815, 0.90],
        ];
        for (id, _) in PRESETS {
            for (i, p) in base.iter().enumerate() {
                let p = position(id, i, *p);
                assert!(p.into_iter().all(|n| (0.0..=1.0).contains(&n)));
            }
        }
    }
    #[test]
    fn unknown_shapes_fall_back_without_destroying_the_stored_identity() {
        let a = Appearance { shape: "future-shape".into(), ..Appearance::default() };
        assert_eq!(a.geometry(80.0).radius, 40.0);
        assert_eq!(a.shape, "future-shape");
    }
}
