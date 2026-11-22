//! The binding kinds worked out frame by frame (docs/design/controller.md §2a) [design].
//!
//! [`PadResolver::step`] reads each controller button's state (already filtered by
//! automatic input switching, `super::device`) and gives every control whose binding needs
//! working out a *virtual button*: pressed, and its edges from the frame before.
//! [`crate::bindings::ControlInput`] reads that in place of the raw button, so the readers
//! (Ready Item's tap and hold, the weapon wheel, the melee power attack) see one button and
//! keep their own rules.
//!
//! A control is worked out here when its binding is a hold, a double tap or a combo, or a
//! press on a button that one of those shares ([`PadResolver::resolved`]). Every other
//! press reads the raw button as before, with no delay: a player who binds nothing new
//! sees no change.
//!
//! - **Hold**: the control goes down once the button has been held [`Timing::hold`].
//! - **Double tap**: it goes down on a second press within [`Timing::double`] of the
//!   first press, and stays down while that press is held.
//! - **Combo**: it goes down when its button goes down while its modifier is down; that
//!   press of the button is then *used up*: no other binding on the button acts on it.
//! - **A press sharing its button** with a hold or a double tap acts when the button comes
//!   up before the hold (or when the double tap's window ends), down for one frame. A press
//!   on a combo's modifier acts so too, when no combo used the modifier meanwhile, unless
//!   its control needs a real hold ([`held_nature`]): then it acts at once, beside the
//!   combo.

use crate::bindings::Control;
use crate::pad::binding::PadBinding;
use crate::pad::PadInput;
use bevy::prelude::*;
use std::collections::HashMap;

/// The Hold time and Double-tap window, seconds (the Controls tab's rows, `[Controller]`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub hold: f32,
    pub double: f32,
}

/// Default Hold time: `grab::ACTIVATE_HOLD`'s, so the two holds a player meets feel alike
/// [design].
pub const HOLD_TIME: f32 = 0.4;
/// Default Double-tap window: `control::DOUBLE_TAP`'s, the Double-Tap Reload tweak's
/// [design].
pub const DOUBLE_TAP_WINDOW: f32 = 0.3;
/// The Hold time row's choices, seconds (six: a list, settings-page.md §0.5).
pub const HOLD_TIMES: [f32; 6] = [0.25, 0.3, 0.4, 0.5, 0.6, 0.8];
/// The Double-tap window row's choices, seconds.
pub const DOUBLE_TAP_WINDOWS: [f32; 6] = [0.2, 0.25, 0.3, 0.35, 0.4, 0.5];

impl Default for Timing {
    fn default() -> Self {
        Timing { hold: HOLD_TIME, double: DOUBLE_TAP_WINDOW }
    }
}

/// The controls whose readers need the button really held, not a one-frame press: a press
/// of theirs on a combo's modifier stays live [design, §2a]. Movement and the stick's
/// directions, attack, aim and block, run, sprint, grab, Activate (held, it grabs), Ready
/// Item (held, it holsters), Toggle POV (held, it circles the camera), the hotkeys (held,
/// the wheel), and the Hold to … tweaks' Jump, Rest and Crouch/Sneak.
pub fn held_nature(c: Control) -> bool {
    use Control::*;
    matches!(
        c,
        Forward | Back | SlideLeft | SlideRight | Use | Block | Run | Sprint | Grab | Activate | ReadyItem | TogglePov | Hotkey1 | AmmoSwap | Hotkey3 | Hotkey4 | Hotkey5 | Hotkey6 | Hotkey7 | Hotkey8 | NoCollision | RepeatActivate | Jump | Rest | CrouchSneak | OrbitCamera | WeaponWheel | ActivateGrab
    )
}

/// A button this frame: down, went down, came up (any controller's, filtered).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ButtonNow {
    pub pressed: bool,
    pub just_pressed: bool,
    pub just_released: bool,
}

/// A control's button as the resolver works it out.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct VirtualButton {
    pub pressed: bool,
    pub before: bool,
}

impl VirtualButton {
    pub fn just_pressed(self) -> bool {
        self.pressed && !self.before
    }

    pub fn just_released(self) -> bool {
        !self.pressed && self.before
    }
}

/// What a button is used for by the bindings in use.
#[derive(Debug, Clone, Default)]
struct Roles {
    /// Controls pressing it, worked out here (it is shared).
    press: Vec<Control>,
    hold: Vec<Control>,
    double: Vec<Control>,
    /// Combos with it as their button: (modifier, control).
    combo: Vec<(GamepadButton, Control)>,
    /// A combo's modifier.
    modifier: bool,
}

impl Roles {
    /// A press on it waits for the release (or the double tap's window).
    fn deferred(&self) -> bool {
        !self.hold.is_empty() || !self.double.is_empty() || (self.modifier && !self.press.is_empty())
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    /// Down, deciding; seconds since the press.
    Down(f32),
    /// Down, the press acting (not deferred).
    Live,
    HoldFired,
    /// Up after a first press; seconds since that press.
    WaitSecond(f32),
    SecondDown,
    /// Used up by this combo control until the button comes up.
    Used(Control),
}

#[derive(Debug, Clone, Copy, Default)]
struct ButtonState {
    phase: Phase,
    /// A combo used this button as its modifier during the hold.
    modifier_used: bool,
}

/// The resolver's state (a resource; [`resolve`] steps it).
#[derive(Resource, Debug, Default, Clone)]
pub struct PadResolver {
    buttons: HashMap<GamepadButton, ButtonState>,
    virt: HashMap<Control, VirtualButton>,
    /// Pulses waiting to be let go next frame.
    pulsed: Vec<Control>,
}

impl PadResolver {
    /// The control's virtual button, when the resolver works it out ([`PadResolver::resolved`]).
    pub fn get(&self, c: Control) -> Option<VirtualButton> {
        self.virt.get(&c).copied()
    }

    /// Forget every pending hold, tap and window (a device change, a menu) [design, #2200:
    /// a switch cannot split a press from its release]. The controls held come up.
    pub fn clear(&mut self) {
        self.buttons.clear();
        self.pulsed.clear();
        for v in self.virt.values_mut() {
            *v = VirtualButton { pressed: false, before: v.pressed };
        }
    }

    /// The controls the bindings `binds` have worked out here: holds, double taps, combos,
    /// and presses on a button one of those shares (a press on a combo's modifier only when
    /// its control is not [`held_nature`]).
    pub fn resolved(binds: &[(Control, PadBinding)]) -> Vec<Control> {
        let roles = roles(binds);
        let mut out: Vec<Control> = binds.iter().filter(|(_, b)| matches!(b, PadBinding::Hold(_) | PadBinding::Double(_) | PadBinding::Combo { .. })).map(|(c, _)| *c).collect();
        for r in roles.values() {
            out.extend(r.press.iter().copied());
        }
        out
    }

    /// One frame: the bindings in use, each button's state, the seconds since the last frame.
    pub fn step(&mut self, binds: &[(Control, PadBinding)], timing: Timing, button: impl Fn(GamepadButton) -> ButtonNow, dt: f32) {
        let roles = roles(binds);
        let mut down: Vec<Control> = Vec::new();
        // Combos fired this frame used their modifiers: noted after the loop.
        let mut used_modifiers: Vec<GamepadButton> = Vec::new();
        let mut pulses: Vec<Control> = Vec::new();
        let mut keys: Vec<GamepadButton> = roles.keys().copied().collect();
        keys.sort_by_key(|b| format!("{b:?}"));
        for b in keys {
            let r = &roles[&b];
            let now = button(b);
            let st = self.buttons.entry(b).or_default();
            if now.just_pressed {
                let combo = r.combo.iter().find(|(m, _)| button(*m).pressed).copied();
                st.phase = match (combo, st.phase) {
                    (Some((m, c)), _) => {
                        used_modifiers.push(m);
                        Phase::Used(c)
                    }
                    (None, Phase::WaitSecond(_)) if !r.double.is_empty() => Phase::SecondDown,
                    _ if r.press.is_empty() || r.deferred() => Phase::Down(0.0),
                    _ => Phase::Live,
                };
            } else if now.pressed {
                if let Phase::Down(t) = st.phase {
                    let t = t + dt;
                    st.phase = if !r.hold.is_empty() && t >= timing.hold { Phase::HoldFired } else { Phase::Down(t) };
                }
            }
            if now.just_released || !now.pressed {
                st.phase = match st.phase {
                    Phase::Down(t) if !r.double.is_empty() => Phase::WaitSecond(t),
                    Phase::Down(_) => {
                        if !st.modifier_used {
                            pulses.extend(r.press.iter().copied());
                        }
                        Phase::Idle
                    }
                    Phase::WaitSecond(t) => {
                        let t = t + dt;
                        if t > timing.double {
                            pulses.extend(r.press.iter().copied());
                            Phase::Idle
                        } else {
                            Phase::WaitSecond(t)
                        }
                    }
                    _ => Phase::Idle,
                };
                if st.phase == Phase::Idle || matches!(st.phase, Phase::WaitSecond(_)) {
                    st.modifier_used = false;
                }
            }
            match st.phase {
                Phase::Live => down.extend(r.press.iter().copied()),
                Phase::HoldFired => down.extend(r.hold.iter().copied()),
                Phase::SecondDown => down.extend(r.double.iter().copied()),
                Phase::Used(c) => down.push(c),
                _ => {}
            }
        }
        for m in used_modifiers {
            self.buttons.entry(m).or_default().modifier_used = true;
        }
        // Buttons no binding uses any more are forgotten.
        self.buttons.retain(|b, _| roles.contains_key(b));
        let resolved = PadResolver::resolved(binds);
        let mut virt = HashMap::with_capacity(resolved.len());
        for c in resolved {
            let before = self.virt.get(&c).is_some_and(|v| v.pressed);
            let pressed = down.contains(&c) || pulses.contains(&c);
            virt.insert(c, VirtualButton { pressed, before });
        }
        self.virt = virt;
        self.pulsed = pulses;
    }
}

/// Each button's roles under `binds`; a press is listed only when the button is shared.
fn roles(binds: &[(Control, PadBinding)]) -> HashMap<GamepadButton, Roles> {
    let mut roles: HashMap<GamepadButton, Roles> = HashMap::new();
    for (c, b) in binds {
        match *b {
            PadBinding::Hold(x) => roles.entry(x).or_default().hold.push(*c),
            PadBinding::Double(x) => roles.entry(x).or_default().double.push(*c),
            PadBinding::Combo { modifier, button } => {
                roles.entry(button).or_default().combo.push((modifier, *c));
                roles.entry(modifier).or_default().modifier = true;
            }
            _ => {}
        }
    }
    for (c, b) in binds {
        let PadBinding::Press(PadInput::Button(x)) = *b else { continue };
        let Some(r) = roles.get_mut(&x) else { continue };
        // A press of a control that needs a real hold stays live on a combo's modifier.
        let live_modifier = r.modifier && r.hold.is_empty() && r.double.is_empty() && held_nature(*c);
        if !live_modifier {
            r.press.push(*c);
        }
    }
    roles
}

/// Step the resolver each frame from the filtered controllers (`PreUpdate`, after the
/// switching filter and before every reader).
pub(crate) fn resolve(bindings: Option<Res<crate::bindings::Bindings>>, pads: super::ActionPads, time: Res<Time<Real>>, state: Option<Res<State<crate::PlayState>>>, mut r: ResMut<PadResolver>) {
    let Some(b) = bindings else { return };
    if b.device != crate::bindings::InputDevice::Controller || state.as_ref().is_some_and(|s| s.is_changed()) {
        r.clear();
    }
    if b.device != crate::bindings::InputDevice::Controller {
        return;
    }
    let binds = b.pad_bindings();
    let timing = b.timing();
    let button = |x: GamepadButton| ButtonNow {
        pressed: pads.iter().any(|g| g.pressed(x)),
        just_pressed: pads.iter().any(|g| g.just_pressed(x)),
        just_released: pads.iter().any(|g| g.just_released(x)) && !pads.iter().any(|g| g.pressed(x)),
    };
    let before: Vec<(Control, bool)> = binds.iter().filter_map(|(c, _)| r.get(*c).map(|v| (*c, v.pressed))).collect();
    r.step(&binds, timing, button, time.delta_secs());
    for (c, was) in before {
        if let Some(v) = r.get(c) {
            if v.pressed && !was {
                if let Some(pb) = b.pad_binding(c) {
                    info!("pad: {} by {}", crate::bindings::Bindings::name(c), pb.ini_text(""));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use GamepadButton::*;

    const DT: f32 = 1.0 / 60.0;

    /// A pad's buttons over time for the tests: which are down each frame.
    struct Pad {
        now: Vec<GamepadButton>,
        before: Vec<GamepadButton>,
    }

    impl Pad {
        fn new() -> Pad {
            Pad { now: Vec::new(), before: Vec::new() }
        }

        fn set(&mut self, down: &[GamepadButton]) {
            self.before = std::mem::replace(&mut self.now, down.to_vec());
        }

        fn state(&self, b: GamepadButton) -> ButtonNow {
            let (n, w) = (self.now.contains(&b), self.before.contains(&b));
            ButtonNow { pressed: n, just_pressed: n && !w, just_released: !n && w }
        }
    }

    /// Run `frames` frames with `down` held; the frames on which each control went down.
    fn run(r: &mut PadResolver, pad: &mut Pad, binds: &[(Control, PadBinding)], down: &[GamepadButton], frames: usize, log: &mut Vec<(Control, &'static str)>) {
        for _ in 0..frames {
            pad.set(down);
            r.step(binds, Timing::default(), |b| pad.state(b), DT);
            for (c, _) in binds {
                if let Some(v) = r.get(*c) {
                    if v.just_pressed() {
                        log.push((*c, "down"));
                    }
                    if v.just_released() {
                        log.push((*c, "up"));
                    }
                }
            }
        }
    }

    fn frames(secs: f32) -> usize {
        (secs / DT).ceil() as usize
    }

    #[test]
    fn an_unshared_press_is_not_worked_out_here() {
        let binds = [(Control::Activate, PadBinding::Press(PadInput::Button(South))), (Control::Jump, PadBinding::Press(PadInput::Button(North)))];
        assert!(PadResolver::resolved(&binds).is_empty(), "raw buttons, no delay");
        let mut r = PadResolver::default();
        r.step(&binds, Timing::default(), |_| ButtonNow { pressed: true, just_pressed: true, just_released: false }, DT);
        assert_eq!(r.get(Control::Activate), None);
    }

    #[test]
    fn a_hold_acts_at_the_hold_time_and_until_let_go() {
        let binds = [(Control::Rest, PadBinding::Hold(Select))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[Select], frames(HOLD_TIME) - 2, &mut log);
        assert!(log.is_empty(), "not before the hold time");
        run(&mut r, &mut pad, &binds, &[Select], 3, &mut log);
        assert_eq!(log, vec![(Control::Rest, "down")]);
        run(&mut r, &mut pad, &binds, &[Select], 20, &mut log);
        run(&mut r, &mut pad, &binds, &[], 1, &mut log);
        assert_eq!(log, vec![(Control::Rest, "down"), (Control::Rest, "up")]);
        // A short press does nothing.
        log.clear();
        run(&mut r, &mut pad, &binds, &[Select], 5, &mut log);
        run(&mut r, &mut pad, &binds, &[], 5, &mut log);
        assert!(log.is_empty());
    }

    #[test]
    fn a_press_sharing_a_hold_acts_on_release_for_one_frame() {
        let binds = [(Control::Rest, PadBinding::Hold(Select)), (Control::Vats, PadBinding::Press(PadInput::Button(Select)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[Select], 5, &mut log);
        assert!(log.is_empty(), "the tap waits for the release");
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log, vec![(Control::Vats, "down"), (Control::Vats, "up")]);
        // Held past the hold time: the hold, not the tap.
        log.clear();
        run(&mut r, &mut pad, &binds, &[Select], frames(HOLD_TIME) + 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log, vec![(Control::Rest, "down"), (Control::Rest, "up")]);
    }

    #[test]
    fn a_double_tap_acts_inside_the_window_only() {
        let binds = [(Control::Hotkey5, PadBinding::Double(North)), (Control::Jump, PadBinding::Press(PadInput::Button(North)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[North], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], 4, &mut log);
        run(&mut r, &mut pad, &binds, &[North], 10, &mut log);
        assert_eq!(log, vec![(Control::Hotkey5, "down")], "held while the second press is held");
        run(&mut r, &mut pad, &binds, &[], 1, &mut log);
        assert_eq!(log.last(), Some(&(Control::Hotkey5, "up")));
        // One press: the single press acts when the window ends.
        log.clear();
        run(&mut r, &mut pad, &binds, &[], frames(1.0), &mut log);
        run(&mut r, &mut pad, &binds, &[North], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], frames(DOUBLE_TAP_WINDOW) - 6, &mut log);
        assert!(log.is_empty(), "still waiting for a second press");
        run(&mut r, &mut pad, &binds, &[], 10, &mut log);
        assert_eq!(log, vec![(Control::Jump, "down"), (Control::Jump, "up")]);
        // Too slow a second press is two single presses.
        log.clear();
        run(&mut r, &mut pad, &binds, &[], frames(1.0), &mut log);
        run(&mut r, &mut pad, &binds, &[North], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], frames(DOUBLE_TAP_WINDOW) + 2, &mut log);
        run(&mut r, &mut pad, &binds, &[North], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], frames(DOUBLE_TAP_WINDOW) + 2, &mut log);
        assert_eq!(log.iter().filter(|(c, w)| *c == Control::Jump && *w == "down").count(), 2);
        assert!(!log.iter().any(|(c, _)| *c == Control::Hotkey5));
    }

    #[test]
    fn hold_and_double_tap_share_a_button() {
        let binds = [(Control::Hotkey6, PadBinding::Hold(West)), (Control::Hotkey7, PadBinding::Double(West))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[West], frames(HOLD_TIME) + 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], frames(1.0), &mut log);
        assert_eq!(log, vec![(Control::Hotkey6, "down"), (Control::Hotkey6, "up")]);
        log.clear();
        run(&mut r, &mut pad, &binds, &[West], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[West], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], 3, &mut log);
        assert_eq!(log, vec![(Control::Hotkey7, "down"), (Control::Hotkey7, "up")]);
    }

    #[test]
    fn a_combo_uses_up_its_button_and_needs_the_modifier_first() {
        let combo = PadBinding::Combo { modifier: LeftTrigger, button: South };
        let binds = [(Control::Hotkey5, combo), (Control::Activate, PadBinding::Press(PadInput::Button(South)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[LeftTrigger, South], 5, &mut log);
        assert_eq!(log, vec![(Control::Hotkey5, "down")], "Activate does not act");
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 1, &mut log);
        assert_eq!(log, vec![(Control::Hotkey5, "down"), (Control::Hotkey5, "up")]);
        // The button alone is its own press, at once.
        log.clear();
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[South], 1, &mut log);
        assert_eq!(log, vec![(Control::Activate, "down")], "no delay on the combo's button");
        // The button first and the modifier after is no combo.
        log.clear();
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[South], 1, &mut log);
        run(&mut r, &mut pad, &binds, &[South, LeftTrigger], 3, &mut log);
        assert!(!log.iter().any(|(c, _)| *c == Control::Hotkey5));
    }

    #[test]
    fn a_modifiers_own_press_acts_on_release_only_when_no_combo_used_it() {
        let combo = PadBinding::Combo { modifier: LeftTrigger, button: South };
        let binds = [(Control::Hotkey5, combo), (Control::Vats, PadBinding::Press(PadInput::Button(LeftTrigger)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log, vec![(Control::Vats, "down"), (Control::Vats, "up")]);
        log.clear();
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[LeftTrigger, South], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], 3, &mut log);
        assert!(!log.iter().any(|(c, _)| *c == Control::Vats), "the combo used the modifier: {log:?}");
    }

    #[test]
    fn a_held_control_on_a_modifier_stays_live() {
        let combo = PadBinding::Combo { modifier: LeftTrigger2, button: South };
        let binds = [(Control::Hotkey5, combo), (Control::Block, PadBinding::Press(PadInput::Button(LeftTrigger2)))];
        assert_eq!(PadResolver::resolved(&binds), vec![Control::Hotkey5], "Block reads the raw trigger: aims at once");
    }

    #[test]
    fn clearing_drops_pending_taps_and_windows() {
        let binds = [(Control::Hotkey5, PadBinding::Double(North)), (Control::Jump, PadBinding::Press(PadInput::Button(North)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[North], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        r.clear();
        run(&mut r, &mut pad, &binds, &[], frames(1.0), &mut log);
        assert!(log.is_empty(), "no stray tap after a device change: {log:?}");
    }
}
