//! A control's controller binding and how it is worked (docs/design/controller.md §2a)
//! [design: three kinds beyond the plain press: combo, double tap and hold].
//!
//! The original keeps one controller button per control, the `JJ` byte of its `[Controls]`
//! values [observed: docs/spec-notes/controls.md §2.1], so every kind here past [`Press`]
//! is NVForever's own:
//!
//! - [`Press`]: today's binding, a button or a left-stick direction, acting on press.
//! - [`Hold`]: the control acts once the button has been held for the Hold time
//!   (`crate::bindings::Bindings::hold_time`) and stays held until it is let go.
//! - [`Double`]: the control acts on a second press within the Double-tap window and stays
//!   held while that press is held.
//! - [`Combo`]: a modifier held, then a button (`LB + A`): the control acts as the button
//!   goes down while the modifier is down, and the button's own binding does not.
//! - [`BuiltIn`]: a split control's place inside its original control
//!   (`crate::bindings::SPLITS`): holding Menu Mode for the light, Ready Item to holster.
//!
//! The INI keeps the Xbox names whatever the button style (§3a): `A`, `Hold X`,
//! `Double Y`, `LB + A`, `With Menu Mode`. A bare name is a [`Press`], so every INI written
//! before the kinds reads as it did.
//!
//! [`Press`]: PadBinding::Press
//! [`Hold`]: PadBinding::Hold
//! [`Double`]: PadBinding::Double
//! [`Combo`]: PadBinding::Combo
//! [`BuiltIn`]: PadBinding::BuiltIn

use super::{pad_label, pad_name, parse_pad, Labels, PadInput, RESERVED_PAD};
use bevy::prelude::*;

/// A control's controller binding [design, §2a].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PadBinding {
    /// A button or a stick direction, acting on press (every binding before the kinds).
    Press(PadInput),
    /// The button held for the Hold time.
    Hold(GamepadButton),
    /// The button pressed twice within the Double-tap window.
    Double(GamepadButton),
    /// `modifier` held, then `button` pressed.
    Combo { modifier: GamepadButton, button: GamepadButton },
    /// Where a split control is built in (`crate::bindings::SPLITS`): its original control's
    /// own hold, on the original control's button, with its old timing.
    BuiltIn,
}

/// The words the INI and the page put before a hold and a double tap.
const HOLD_WORD: &str = "Hold";
const DOUBLE_WORD: &str = "Double";
/// The page's word for a double tap (the INI's is shorter).
const DOUBLE_SHOWN: &str = "Double-tap";
const WITH_WORD: &str = "With";

impl PadBinding {
    /// The button (or stick direction) the binding is worked by: a combo's button, not its
    /// modifier; none for [`PadBinding::BuiltIn`], whose button is its original control's.
    pub fn main(self) -> Option<PadInput> {
        match self {
            PadBinding::Press(p) => Some(p),
            PadBinding::Hold(b) | PadBinding::Double(b) | PadBinding::Combo { button: b, .. } => Some(PadInput::Button(b)),
            PadBinding::BuiltIn => None,
        }
    }

    /// Every button the binding needs (a combo's two).
    pub fn buttons(self) -> Vec<GamepadButton> {
        match self {
            PadBinding::Press(PadInput::Button(b)) | PadBinding::Hold(b) | PadBinding::Double(b) => vec![b],
            PadBinding::Combo { modifier, button } => vec![modifier, button],
            PadBinding::Press(PadInput::Stick(_)) | PadBinding::BuiltIn => Vec::new(),
        }
    }

    /// The binding as the INI writes it; `origin` names a [`PadBinding::BuiltIn`]'s
    /// original control ("Menu Mode").
    pub fn ini_text(self, origin: &str) -> String {
        let n = |b: GamepadButton| pad_name(PadInput::Button(b));
        match self {
            PadBinding::Press(p) => pad_name(p).to_string(),
            PadBinding::Hold(b) => format!("{HOLD_WORD} {}", n(b)),
            PadBinding::Double(b) => format!("{DOUBLE_WORD} {}", n(b)),
            PadBinding::Combo { modifier, button } => format!("{} + {}", n(modifier), n(button)),
            PadBinding::BuiltIn => format!("{WITH_WORD} {origin}"),
        }
    }

    /// The binding as the page and the prompts show it with the labels `l` (#1452's names):
    /// `A`, `Hold X`, `Double-tap Y`, `L1 + Cross`. A [`PadBinding::BuiltIn`] is shown by
    /// `crate::bindings` (it needs the original control's binding); here `None`.
    pub fn shown(self, l: Labels) -> Option<String> {
        let n = |b: GamepadButton| pad_label(PadInput::Button(b), l);
        Some(match self {
            PadBinding::Press(p) => pad_label(p, l).to_string(),
            PadBinding::Hold(b) => format!("{HOLD_WORD} {}", n(b)),
            PadBinding::Double(b) => format!("{DOUBLE_SHOWN} {}", n(b)),
            PadBinding::Combo { modifier, button } => format!("{} + {}", n(modifier), n(button)),
            PadBinding::BuiltIn => return None,
        })
    }

    /// A binding from the INI's text (without case, any spaces around `+`). Start is kept
    /// for the pause menu; a stick direction is a press only; a combo needs two different
    /// buttons.
    pub fn parse(text: &str) -> Result<PadBinding, String> {
        let t = text.trim();
        if let Some((m, b)) = t.split_once('+') {
            let (modifier, button) = (button(m)?, button(b)?);
            if modifier == button {
                return Err(format!("{t:?}: a combo needs two different buttons"));
            }
            return Ok(PadBinding::Combo { modifier, button });
        }
        if let Some(rest) = word(t, HOLD_WORD) {
            return Ok(PadBinding::Hold(button(rest)?));
        }
        if let Some(rest) = word(t, DOUBLE_WORD).or_else(|| word(t, DOUBLE_SHOWN)) {
            return Ok(PadBinding::Double(button(rest)?));
        }
        if word(t, WITH_WORD).is_some() {
            return Ok(PadBinding::BuiltIn);
        }
        match parse_pad(t) {
            Some(p) if RESERVED_PAD.contains(&p) => Err(format!("{t:?} is kept for the pause menu")),
            Some(p) => Ok(PadBinding::Press(p)),
            None => Err(format!("{t:?} is not a controller button name")),
        }
    }
}

/// `text` without its leading `w` and the space after it (without case).
fn word<'a>(text: &'a str, w: &str) -> Option<&'a str> {
    let (head, rest) = text.split_once(char::is_whitespace)?;
    head.eq_ignore_ascii_case(w).then_some(rest.trim())
}

/// A button by its name, for the kinds that take a button (not a stick direction, not Start).
fn button(name: &str) -> Result<GamepadButton, String> {
    let n = name.trim();
    match parse_pad(n) {
        Some(p) if RESERVED_PAD.contains(&p) => Err(format!("{n:?} is kept for the pause menu")),
        Some(PadInput::Button(b)) => Ok(b),
        Some(PadInput::Stick(_)) => Err(format!("{n:?}: a stick direction can only be pressed")),
        None => Err(format!("{n:?} is not a controller button name")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pad::PAD_NAMES;
    use GamepadButton::*;

    /// Every name an INI written before the kinds can hold reads as the same press, and no
    /// name of [`PAD_NAMES`] can be taken for a kind's text.
    #[test]
    fn every_old_name_is_still_a_press() {
        for (p, n) in PAD_NAMES {
            let parsed = PadBinding::parse(n);
            if RESERVED_PAD.contains(p) {
                assert!(parsed.is_err(), "{n}");
                continue;
            }
            assert_eq!(parsed, Ok(PadBinding::Press(*p)), "{n}");
            assert_eq!(PadBinding::Press(*p).ini_text(""), *n);
            assert!(!n.contains('+'), "{n}");
            for w in [HOLD_WORD, DOUBLE_WORD, DOUBLE_SHOWN, WITH_WORD] {
                assert!(word(n, w).is_none(), "{n} starts with {w}");
            }
        }
    }

    #[test]
    fn the_kinds_round_trip_in_any_case_and_spacing() {
        let cases = [
            (PadBinding::Hold(West), "Hold X", ["hold x", "HOLD   X", " Hold X "]),
            (PadBinding::Double(North), "Double Y", ["double y", "Double-tap Y", "DOUBLE   y"]),
            (PadBinding::Combo { modifier: LeftTrigger, button: South }, "LB + A", ["lb+a", "LB  +  A", " lb + a"]),
            (PadBinding::Hold(DPadUp), "Hold D-Pad Up", ["hold d-pad up", "Hold  D-Pad Up", "HOLD D-PAD UP"]),
        ];
        for (b, text, others) in cases {
            assert_eq!(b.ini_text(""), text);
            assert_eq!(PadBinding::parse(text), Ok(b));
            for o in others {
                assert_eq!(PadBinding::parse(o), Ok(b), "{o}");
            }
        }
        assert_eq!(PadBinding::BuiltIn.ini_text("Menu Mode"), "With Menu Mode");
        assert_eq!(PadBinding::parse("With Menu Mode"), Ok(PadBinding::BuiltIn));
        assert_eq!(PadBinding::parse("with hotkeys"), Ok(PadBinding::BuiltIn));
    }

    #[test]
    fn bad_forms_are_refused_with_a_reason() {
        for bad in ["Hold Left Stick Up", "Start + A", "A + Start", "LB + LB", "Hold", "LB +", "+ A", "Hold Start", "Double", "Triple A", "Start", "Q"] {
            assert!(PadBinding::parse(bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn the_page_shows_the_button_style_names() {
        let c = PadBinding::Combo { modifier: LeftTrigger, button: South };
        assert_eq!(c.shown(Labels::Xbox).as_deref(), Some("LB + A"));
        assert_eq!(c.shown(Labels::PlayStation).as_deref(), Some("L1 + Cross"));
        assert_eq!(PadBinding::Double(North).shown(Labels::Xbox).as_deref(), Some("Double-tap Y"));
        assert_eq!(PadBinding::Hold(Select).shown(Labels::PlayStation).as_deref(), Some("Hold Select"));
        assert_eq!(PadBinding::BuiltIn.shown(Labels::Xbox), None);
        assert_eq!(c.main(), Some(PadInput::Button(South)));
        assert_eq!(c.buttons(), vec![LeftTrigger, South]);
    }
}
