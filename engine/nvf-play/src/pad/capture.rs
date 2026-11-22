//! The Controls tab taking a controller binding by the gesture made (docs/design/
//! controller.md §2a) [design]: with a row waiting for its new button (`crate::pause::
//! Listening`), the buttons pressed decide its kind, and the binding is taken once every
//! button is up again, so no press of the gesture reaches the page after it.
//!
//! - one button pressed and let go, and not pressed again within the Double-tap window:
//!   a press;
//! - one button held for the Hold time: a hold (the row shows "Hold X" from that moment,
//!   so the player knows when to let go);
//! - one button pressed twice within the Double-tap window: a double tap;
//! - a second button pressed while the first is down: a combo, the first its modifier.
//!
//! A stick direction is taken at once as a press, as before; Start cancels
//! (`crate::pause`).

use crate::pad::binding::PadBinding;
use crate::pad::resolver::Timing;
use bevy::prelude::*;

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum State {
    #[default]
    Idle,
    /// The first button down, held this long.
    First(GamepadButton, f32),
    /// A second button went down while the first was: the combo, waiting for both to come up.
    Combo(GamepadButton, GamepadButton),
    /// The first button came up after a short press; this long since.
    Wait(GamepadButton, f32),
    /// Decided; waiting for every button to come up.
    Done(PadBinding),
}

/// The gesture so far ([`PadCapture::step`]).
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct PadCapture {
    state: State,
}

impl PadCapture {
    /// Start again (a row starts waiting, or stops).
    pub fn reset(&mut self) {
        self.state = State::Idle;
    }

    /// No gesture begun.
    pub fn is_idle(&self) -> bool {
        self.state == State::Idle
    }

    /// One frame: the buttons down now, those that went down this frame (Start left out by
    /// the caller), the seconds since the last frame. The binding, once the gesture is
    /// decided and every button is up.
    pub fn step(&mut self, down: &[GamepadButton], pressed: &[GamepadButton], timing: Timing, dt: f32) -> Option<PadBinding> {
        self.state = match self.state {
            State::Idle => match pressed.first() {
                Some(&b) => State::First(b, 0.0),
                None => State::Idle,
            },
            State::First(b, t) => match pressed.iter().find(|&&x| x != b) {
                Some(&x) if down.contains(&b) => State::Combo(b, x),
                _ if down.contains(&b) => State::First(b, t + dt),
                _ if t + dt >= timing.hold => State::Done(PadBinding::Hold(b)),
                _ => State::Wait(b, 0.0),
            },
            State::Combo(m, x) => State::Combo(m, x),
            State::Wait(b, w) => {
                if pressed.contains(&b) {
                    State::Done(PadBinding::Double(b))
                } else if w + dt > timing.double {
                    State::Done(PadBinding::Press(crate::pad::PadInput::Button(b)))
                } else {
                    State::Wait(b, w + dt)
                }
            }
            State::Done(p) => State::Done(p),
        };
        if down.is_empty() {
            if let State::Combo(m, x) = self.state {
                self.state = State::Done(PadBinding::Combo { modifier: m, button: x });
            }
            if let State::Done(p) = self.state {
                self.state = State::Idle;
                return Some(p);
            }
        }
        None
    }

    /// What the row shows while the gesture goes on: the hold once it is one ("Hold X").
    pub fn shown(&self, timing: Timing) -> Option<PadBinding> {
        match self.state {
            State::First(b, t) if t >= timing.hold => Some(PadBinding::Hold(b)),
            State::Combo(m, x) => Some(PadBinding::Combo { modifier: m, button: x }),
            State::Done(p) => Some(p),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::PadInput;
    use GamepadButton::*;

    const DT: f32 = 1.0 / 60.0;

    /// Feed `frames` frames of `down` (presses: the buttons not down the frame before).
    fn feed(c: &mut PadCapture, before: &mut Vec<GamepadButton>, down: &[GamepadButton], frames: usize) -> Option<PadBinding> {
        let mut got = None;
        for _ in 0..frames {
            let pressed: Vec<GamepadButton> = down.iter().copied().filter(|b| !before.contains(b)).collect();
            got = got.or(c.step(down, &pressed, Timing::default(), DT));
            *before = down.to_vec();
        }
        got
    }

    #[test]
    fn each_gesture_gives_its_kind() {
        let t = Timing::default();
        let frames = |s: f32| (s / DT).ceil() as usize + 2;
        // A press: up again, and no second press within the window.
        let (mut c, mut b) = (PadCapture::default(), Vec::new());
        assert_eq!(feed(&mut c, &mut b, &[West], 3), None);
        assert_eq!(feed(&mut c, &mut b, &[], frames(t.double)), Some(PadBinding::Press(PadInput::Button(West))));
        // A hold: shown as one once held long enough, taken when let go.
        let (mut c, mut b) = (PadCapture::default(), Vec::new());
        assert_eq!(feed(&mut c, &mut b, &[West], frames(t.hold)), None);
        assert_eq!(c.shown(t), Some(PadBinding::Hold(West)));
        assert_eq!(feed(&mut c, &mut b, &[], 1), Some(PadBinding::Hold(West)));
        // A double tap.
        let (mut c, mut b) = (PadCapture::default(), Vec::new());
        feed(&mut c, &mut b, &[North], 3);
        feed(&mut c, &mut b, &[], 3);
        assert_eq!(feed(&mut c, &mut b, &[North], 3), None, "taken when let go");
        assert_eq!(feed(&mut c, &mut b, &[], 1), Some(PadBinding::Double(North)));
        // A combo, the first button its modifier, taken once both are up.
        let (mut c, mut b) = (PadCapture::default(), Vec::new());
        feed(&mut c, &mut b, &[LeftTrigger], 4);
        assert_eq!(feed(&mut c, &mut b, &[LeftTrigger, South], 4), None);
        assert_eq!(feed(&mut c, &mut b, &[LeftTrigger], 2), None, "the modifier still down");
        assert_eq!(feed(&mut c, &mut b, &[], 1), Some(PadBinding::Combo { modifier: LeftTrigger, button: South }));
    }
}
