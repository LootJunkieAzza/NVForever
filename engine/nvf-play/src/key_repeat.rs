//! Held keys repeat in menus [design]: an arrow key, W/A/S/D or Page Up/Down held down
//! in a menu (any state but play, or the pause menu) presses again after
//! [`FIRST_DELAY`] and then every [`INTERVAL`], so a long list scrolls by holding. The
//! controller's D-pad and stick come through as these keys (`crate::pad::menu_keys`) and
//! repeat too. A repeat is a release and press in one frame, after the input systems,
//! so every menu's own `just_pressed` reads it as a press.
//!
//! The player's Key Repeat Acceleration tweak (`crate::tweaks`, Stewie's Tweaks
//! `bKeyRepeatAcceleration`) makes the repeats faster the longer the key is held: by its
//! first multiplier after its first delay, by its second after its second [design:
//! docs/design/tweaks.md].
//!
//! The original repeats held menu keys too [hypothesis: the tweak speeds up a repeat that
//! is there]; its timings are not measured, so [`FIRST_DELAY`] and [`INTERVAL`] are ours.

use bevy::prelude::*;

/// The keys that repeat.
const KEYS: [KeyCode; 10] = [
    KeyCode::ArrowUp,
    KeyCode::ArrowDown,
    KeyCode::ArrowLeft,
    KeyCode::ArrowRight,
    KeyCode::KeyW,
    KeyCode::KeyA,
    KeyCode::KeyS,
    KeyCode::KeyD,
    KeyCode::PageUp,
    KeyCode::PageDown,
];

/// Seconds a key is held before its first repeat, and between repeats after it [design].
pub const FIRST_DELAY: f32 = 0.4;
pub const INTERVAL: f32 = 0.1;

/// The Key Repeat Acceleration tweak's numbers: after `delay` seconds held the repeats
/// come `mult` times as often, after `delay_alt` `mult_alt` times (the larger of the two
/// that apply).
#[derive(Resource, Clone, Copy, Debug, PartialEq)]
pub struct Acceleration {
    pub on: bool,
    pub delay: f32,
    pub mult: f32,
    pub delay_alt: f32,
    pub mult_alt: f32,
}

impl Default for Acceleration {
    fn default() -> Self {
        Self { on: false, delay: 1.0, mult: 2.0, delay_alt: 2.5, mult_alt: 4.0 }
    }
}

impl Acceleration {
    /// The time between repeats after the key has been held `held` seconds.
    pub fn interval(&self, held: f32) -> f32 {
        if !self.on {
            return INTERVAL;
        }
        let mut mult: f32 = 1.0;
        if held >= self.delay {
            mult = mult.max(self.mult);
        }
        if held >= self.delay_alt {
            mult = mult.max(self.mult_alt);
        }
        INTERVAL / mult.max(0.01)
    }
}

/// A held key: how long it has been held, and when it repeats next.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    held: f32,
    next: f32,
}

/// The repeats under way.
#[derive(Resource, Default)]
struct Repeats(Vec<(KeyCode, Held)>);

/// How many repeats one key gives in `dt` more seconds held (the timer moved on).
fn advance(h: &mut Held, dt: f32, acc: &Acceleration) -> u32 {
    h.held += dt;
    let mut n = 0;
    while h.held >= h.next && n < 8 {
        n += 1;
        h.next += acc.interval(h.next);
    }
    n
}

fn repeat(
    time: Res<Time<Real>>,
    state: Option<Res<State<crate::PlayState>>>,
    paused: Option<Res<crate::pause::Paused>>,
    acc: Res<Acceleration>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut repeats: ResMut<Repeats>,
) {
    let in_menu = state.is_some_and(|s| !matches!(s.get(), crate::PlayState::Playing | crate::PlayState::Loading)) || paused.is_some_and(|p| p.0);
    if !in_menu {
        repeats.0.clear();
        return;
    }
    let dt = time.delta_secs();
    repeats.0.retain(|(k, _)| keys.pressed(*k));
    for k in KEYS {
        if keys.just_pressed(k) {
            repeats.0.retain(|(o, _)| *o != k);
            repeats.0.push((k, Held { held: 0.0, next: FIRST_DELAY }));
        }
    }
    for (k, h) in &mut repeats.0 {
        if keys.just_pressed(*k) {
            continue;
        }
        if advance(h, dt, &acc) > 0 {
            keys.release(*k);
            keys.press(*k);
        }
    }
}

pub struct KeyRepeatPlugin;

impl Plugin for KeyRepeatPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Acceleration>()
            .init_resource::<Repeats>()
            .add_systems(PreUpdate, repeat.after(bevy::input::InputSystems).after(crate::pad::menu_keys).after(crate::test_aid::real_presses).before(bevy::ui::UiSystems::Focus));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_held_key_repeats_after_the_delay_then_steadily() {
        let acc = Acceleration::default();
        let mut h = Held { held: 0.0, next: FIRST_DELAY };
        assert_eq!(advance(&mut h, 0.3, &acc), 0);
        assert_eq!(advance(&mut h, 0.15, &acc), 1);
        // A second later, frame by frame: ten more.
        assert_eq!((0..20).map(|_| advance(&mut h, 0.05, &acc)).sum::<u32>(), 10);
    }

    #[test]
    fn the_tweak_speeds_the_repeats_after_its_delays() {
        let acc = Acceleration { on: true, ..Default::default() };
        assert_eq!(acc.interval(0.5), INTERVAL);
        assert_eq!(acc.interval(1.0), INTERVAL / 2.0);
        assert_eq!(acc.interval(3.0), INTERVAL / 4.0);
        assert_eq!(Acceleration { mult_alt: 1.0, ..acc }.interval(3.0), INTERVAL / 2.0);
    }
}
