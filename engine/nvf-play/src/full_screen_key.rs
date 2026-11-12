//! Alt+Enter switches between a window and full screen while the game runs (#2209), as
//! many PC games and Windows programs do [design: the issue; the original's own keys for
//! it are not captured]. It flips the settings page's "Full screen" row
//! (`crate::settings::flip` on [`KEY`], registered by the program, nvforever's
//! `window_mode`), so the switch is the row's own: the display chosen in the Monitor row,
//! the full-screen resolution kept in `resolution=<window>;<full screen>` (exclusive full
//! screen at a smaller one, left as the row leaves it), back to the window at its own
//! size, kept in `NVForever.ini`, and the row shows it when the page is next drawn.
//!
//! Either Alt (Left Alt, Right Alt or AltGr) held, Enter or the keypad's Enter pressed: in
//! play, in every menu, on the main menu and while loading. The combination's keys reach
//! nothing else:
//! - its Enter is not pressed for the menus (no row or button acts on it), and holding it
//!   does not switch again (its press is taken, the key stays down, so its repeats press
//!   nothing);
//! - its Alt is let go for the rest of the game until it is lifted (or pressed afresh), so
//!   Left Alt's Block stops once Enter goes down and its repeats do not start it again.
//!   Alt alone before Enter does block as it always has: Alt cannot be told apart from a
//!   block until Enter comes [design, the issue's second way].
//!
//! While the settings page's "keep this resolution?" countdown runs, the combination keeps
//! the waiting resolution first, so the two do not fight [design]. A controller has no
//! such combination. The switch is logged ("full screen key: Alt+Enter: full screen on"),
//! and nvforever's `window_mode` logs the mode the window takes.

use bevy::input::keyboard::KeyboardInput;
use bevy::input::ButtonState;
use bevy::prelude::*;

/// The settings row Alt+Enter flips.
pub const KEY: &str = "borderless";

const ALTS: [KeyCode; 2] = [KeyCode::AltLeft, KeyCode::AltRight];
const ENTERS: [KeyCode; 2] = [KeyCode::Enter, KeyCode::NumpadEnter];

/// The Alt keys as the combination sees them.
#[derive(Resource, Default, Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Alts {
    /// Held, from the keyboard's own presses and lifts (the loading screen lets go of every
    /// key each frame, `crate::loading`).
    held: [bool; 2],
    /// Let go for the game since a combination used it, until lifted or pressed afresh.
    taken: [bool; 2],
}

/// Alt+Enter asked for this frame, for [`switch`].
#[derive(Resource, Default)]
pub(crate) struct Asked(bool);

/// What one frame's keys do to [`Alts`] and whether they make the combination, for each
/// Alt: `pressed` (as the game's keys have it now), `struck` and `lifted` (the keyboard's
/// own press, not a repeat, and lift) and `fresh` (pressed this frame, not as a repeat:
/// the keyboard's, or the test aid's, which sends no keyboard event), and whether an Enter
/// was pressed this frame. A taken Alt counts as down only while the keyboard holds it
/// (the test aid's lift is not seen). Pure, for the tests.
pub(crate) fn step(alts: &mut Alts, pressed: [bool; 2], keyboard: ([bool; 2], [bool; 2]), fresh: [bool; 2], enter: bool) -> bool {
    let (struck, lifted) = keyboard;
    for i in 0..2 {
        if lifted[i] {
            alts.held[i] = false;
            alts.taken[i] = false;
        }
        if struck[i] {
            alts.held[i] = true;
        }
        if fresh[i] {
            alts.taken[i] = false;
        }
    }
    let down = |i: usize| pressed[i] || alts.held[i];
    let fire = enter && (down(0) || down(1));
    if fire {
        let downs = [down(0), down(1)];
        for (taken, d) in alts.taken.iter_mut().zip(downs) {
            *taken |= d;
        }
    }
    fire
}

/// Read the keys right after the test aid's presses and before the menus' and the
/// controller's: take the combination's keys ([`step`]) and ask for the switch.
pub(crate) fn read((mut events, switching): (MessageReader<KeyboardInput>, Option<Res<crate::pad::SwitchingInput>>), mut keys: ResMut<ButtonInput<KeyCode>>, mut alts: ResMut<Alts>, mut asked: ResMut<Asked>) {
    let (mut repeated, mut lifted, mut struck) = ([false; 2], [false; 2], [false; 2]);
    for e in events.read() {
        let Some(i) = ALTS.iter().position(|k| *k == e.key_code) else { continue };
        // The shared filter always allows releases [design, #2200].
        if e.state.is_pressed() && switching.as_ref().is_some_and(|s| !s.allows(e)) {
            continue;
        }
        match e.state {
            ButtonState::Released => lifted[i] = true,
            ButtonState::Pressed if e.repeat => repeated[i] = true,
            ButtonState::Pressed => struck[i] = true,
        }
    }
    let view = switching.as_ref().map_or_else(|| std::borrow::Cow::Borrowed(&*keys), |s| s.filter(&keys));
    let pressed = ALTS.map(|k| view.pressed(k));
    // A fresh press: the keyboard's own, or the test aid's (it sends no keyboard event).
    let fresh = [0, 1].map(|i| struck[i] || (view.just_pressed(ALTS[i]) && !repeated[i]));
    let enter = view.any_just_pressed(ENTERS);
    drop(view);
    if step(&mut alts, pressed, (struck, lifted), fresh, enter) {
        for k in ENTERS {
            keys.clear_just_pressed(k);
        }
        asked.0 = true;
    }
    for (i, k) in ALTS.into_iter().enumerate() {
        if alts.taken[i] && keys.pressed(k) {
            keys.reset(k);
        }
    }
}

/// Flip the Full screen row when Alt+Enter was asked (keeping a waiting resolution first).
pub(crate) fn switch(world: &mut World) {
    if !std::mem::take(&mut world.resource_mut::<Asked>().0) {
        return;
    }
    let Some(i) = world.get_resource::<crate::settings::Settings>().and_then(|s| s.index(KEY)) else {
        info!("full screen key: Alt+Enter: no Full screen row in this run");
        return;
    };
    if world.contains_resource::<crate::settings::PendingKeep>() {
        crate::settings::keep(world);
    }
    match crate::settings::flip(world, i) {
        Some(on) => info!("full screen key: Alt+Enter: full screen {}", if on { "on" } else { "off" }),
        None => info!("full screen key: Alt+Enter: the Full screen row cannot be switched now"),
    }
}

pub(crate) fn setup(app: &mut App) {
    app.init_resource::<Alts>().init_resource::<Asked>().add_systems(
        PreUpdate,
        read.after(bevy::input::InputSystems).after(crate::test_aid::real_presses).before(crate::pad::menu_keys).before(crate::loading::hold_input).before(bevy::ui::UiSystems::Focus),
    );
    app.add_systems(Update, switch);
}

#[cfg(test)]
mod tests {
    use super::*;

    const NO: [bool; 2] = [false, false];
    const LEFT: [bool; 2] = [true, false];

    #[test]
    fn alt_and_enter_make_the_combination_and_enter_alone_does_not() {
        let mut a = Alts::default();
        assert!(!step(&mut a, NO, (NO, NO), NO, true), "Enter alone");
        assert!(!step(&mut a, LEFT, (LEFT, NO), LEFT, false), "Alt alone");
        assert!(step(&mut a, LEFT, (NO, NO), NO, true), "Alt held, Enter");
        assert!(step(&mut Alts::default(), [false, true], ([false, true], NO), [false, true], true), "Right Alt too");
    }

    #[test]
    fn a_used_alt_stays_let_go_until_it_is_lifted_and_still_counts_for_another_enter() {
        let mut a = Alts::default();
        step(&mut a, LEFT, (LEFT, NO), LEFT, false);
        assert!(step(&mut a, LEFT, (NO, NO), NO, true));
        assert_eq!(a.taken, LEFT);
        // Let go for the game (not pressed), still held on the keyboard: a second Enter
        // switches again.
        assert!(step(&mut a, NO, (NO, NO), NO, true));
        // Lifted: no longer taken, and Enter alone is Enter.
        assert!(!step(&mut a, NO, (NO, LEFT), NO, false));
        assert_eq!(a, Alts::default());
        assert!(!step(&mut a, NO, (NO, NO), NO, true));
    }

    #[test]
    fn the_test_aids_alt_counts_while_pressed_and_a_fresh_press_is_its_own() {
        // The test aid sends no keyboard event: its Alt is down only while pressed.
        let mut a = Alts::default();
        assert!(step(&mut a, LEFT, (NO, NO), LEFT, true));
        assert_eq!(a, Alts { held: NO, taken: LEFT });
        // Let go for the game and its lift not seen: Enter alone is Enter.
        assert!(!step(&mut a, NO, (NO, NO), NO, true));
        // Pressed again: its own, no longer taken.
        assert!(!step(&mut a, LEFT, (NO, NO), LEFT, false));
        assert_eq!(a.taken, NO);
    }
}
