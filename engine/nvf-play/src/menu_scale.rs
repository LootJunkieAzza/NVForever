//! Independent menu scale, retaining the effective legacy UI size on migration.
//! Policy and limits: docs/spec-notes/hud-menu-scale.md (#2519).
use crate::settings::{self, Setting, Slider};
use bevy::prelude::*;
use std::sync::atomic::{AtomicU32, Ordering};

pub const KEY: &str = "menu-scale";
const CHOICES: usize = 11;
static DRAW_SCALE: AtomicU32 = AtomicU32::new(0x3f80_0000);

/// Pending next-start menu scale. It never follows later HUD setting edits.
#[derive(Resource, Clone, Copy)]
pub struct MenuScale(f32);

impl Default for MenuScale {
    fn default() -> Self { Self(1.0) }
}

impl MenuScale {
    pub fn scale(self) -> f32 { self.0 }
}

fn fit(scale: f32) -> f32 {
    if !scale.is_finite() { return 1.0; }
    (scale.clamp(0.5, 1.0) * 100.0).round() / 100.0
}

fn part(scale: f32) -> f32 { ((fit(scale) - 0.5) * 2.0 * 1000.0).round() / 1000.0 }
fn of_part(value: f32) -> f32 {
    let value = if value.is_finite() { value.clamp(0.0, 1.0) } else { 1.0 };
    fit(0.5 + value * 0.5)
}

/// Legacy ui-scale is a 50..150% slider part, not an already decoded scale.
fn legacy_menu(value: &str) -> Vec<(&'static str, String)> {
    let Some(value) = value.parse::<f32>().ok().filter(|v| v.is_finite()) else { return Vec::new() };
    let value = settings::choice_part(settings::choice_index(value, crate::interface::UI_SCALE_CHOICES), crate::interface::UI_SCALE_CHOICES);
    let scale = crate::interface::menu_scale_of(crate::interface::ui_scale_of_part(value));
    vec![(KEY, format!("{:.3}", part(scale)))]
}

pub fn register(app: &mut App, group: &'static str) {
    app.init_resource::<MenuScale>();
    settings::add_setting(app, Setting::slider(
        KEY, group, "Menu scale",
        "The size of the main, pause and other menus, centred in the view. 50% to 100%: the existing fixed layouts may shrink but cannot grow beyond the screen. Independent of HUD scale. Takes effect at the next start (loading a save starts the game again). Not in the original.",
        Slider::choice(CHOICES, |w| part(w.resource::<MenuScale>().0), |w, v| w.resource_mut::<MenuScale>().0 = of_part(v), |v| format!("{:.0}%", of_part(v) * 100.0)),
    ));
    // Synthesize only the missing menu value before ordinary settings loading.
    // store() writes both registered rows on any edit, freezing this initial value
    // even when the player changes only the HUD. An explicit menu key always wins.
    settings::set_older_key(app, KEY, "ui-scale");
    settings::set_older_split(app, "ui-scale", legacy_menu);
}

pub(crate) fn publish(scale: f32) { DRAW_SCALE.store(fit(scale).to_bits(), Ordering::Relaxed); }
pub(crate) fn drawing() -> f32 { f32::from_bits(DRAW_SCALE.load(Ordering::Relaxed)) }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_choices_retain_the_existing_effective_50_to_100_percent_range() {
        for i in 0..CHOICES {
            let value = settings::choice_part(i, CHOICES);
            assert!((of_part(value) - (0.5 + i as f32 * 0.05)).abs() < 0.00001);
            assert_eq!(part(of_part(value)), value);
        }
        assert_eq!((fit(0.1), fit(2.0), fit(f32::NAN)), (0.5, 1.0, 1.0));
        assert_eq!((of_part(0.0), of_part(0.5), of_part(1.0)), (0.5, 0.75, 1.0));
    }

    #[test]
    fn legacy_ui_values_seed_only_a_missing_menu_value_and_keep_slider_units() {
        let mut app = App::new(); register(&mut app, "Display");
        for (old, expected) in [("0", "0.000"), ("0.25", "0.500"), ("0.5", "1.000"), ("0.75", "1.000"), ("7", "1.000"), ("-2", "0.000")] {
            let input = format!("[Settings]\nui-scale={old}\n[Other]\nkeep=yes\n");
            let text = app.world().resource::<settings::Settings>().split_older(&input);
            assert_eq!(fnv_install::ini_get(&text, "Settings", KEY).as_deref(), Some(expected));
            assert_eq!(fnv_install::ini_get(&text, "Settings", "ui-scale").as_deref(), Some(old));
            assert_eq!(fnv_install::ini_get(&text, "Other", "keep").as_deref(), Some("yes"));
        }
        for explicit in ["0.1", "0.5", "1.0", "bad"] {
            let input = format!("[Settings]\nui-scale=0.25\nmenu-scale={explicit}\n");
            assert_eq!(app.world().resource::<settings::Settings>().split_older(&input), input);
        }
        assert!(legacy_menu("NaN").is_empty());
        assert!(legacy_menu("big").is_empty());
    }

    #[test]
    fn independent_menu_units_keep_rendered_points_and_menu_hit_coordinates_together() {
        use crate::interface::{self, BOTTOM_MIDDLE, MIDDLE};
        let view = Vec2::new(1280.0, 720.0);
        let point = Vec2::new(823.0, 457.0);
        for (hud, menu) in [(0.75, 1.0), (1.5, 0.75), (1.0, 0.5)] {
            let _hud = interface::units_at(hud);
            assert_eq!(interface::u(96.0), Val::Vh(96.0 * hud / 9.6));
            {
                let _menu = interface::units_at(menu);
                assert_eq!(interface::u(96.0), Val::Vh(96.0 * menu / 9.6));
                for anchor in [MIDDLE, BOTTOM_MIDDLE] {
                    let drawn = interface::to_menu_frame(point, view, anchor);
                    assert!((drawn - (anchor * view * (1.0 - menu) + point * menu)).length() < 0.0001);
                    assert!((interface::from_menu_frame(drawn, view, anchor) - point).length() < 0.0001);
                }
                let _canvas = interface::original_units();
                assert_eq!(interface::u(96.0), Val::Vh(96.0 / 9.6));
            }
            assert_eq!(interface::unit_scale(), hud, "leaving a menu/canvas restores the HUD scope");
        }
    }
}
