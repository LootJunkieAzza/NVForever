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
//! - **Combo**: it goes down when its two buttons go down within [`Timing::combo`] of each
//!   other, in either order, or when its button goes down while its modifier has been held
//!   longer. Those presses are then *used up* until they come up: no other binding on
//!   either button acts on them [design, owner 2026-11-22 playtest: "if there's 8ms
//!   between a button press in a combo, it still activates the combo bind and ignores the
//!   individual inputs"]. A modifier held past the window is not used up: a held control
//!   on it (Sneak, Block) stays live beside the combo.
//! - **The combo window**: a press of a button that is part of any combo waits
//!   [`Timing::combo`] before its own binding acts, so the other button can still come.
//!   Held past the window it acts as if pressed then (a hold still counts from the
//!   press); let go inside the window with no combo, its own binding acts late, as a
//!   one-frame press.
//! - **A press sharing its button** with a hold or a double tap acts when the button comes
//!   up before the hold (or when the double tap's window ends), down for one frame. A press
//!   on a combo's modifier acts so too, when no combo used the modifier meanwhile, unless
//!   its control needs a real hold ([`held_nature`]): then it acts once the combo window
//!   has passed, while the button is held.

use crate::bindings::Control;
use crate::pad::binding::PadBinding;
use crate::pad::PadInput;
use bevy::prelude::*;
use std::collections::HashMap;

/// The Hold time, Double-tap window and Combo window, seconds (the Controls tab's rows,
/// `[Controller]`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Timing {
    pub hold: f32,
    pub double: f32,
    pub combo: f32,
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
/// Default Combo window: two buttons meant together come down some tens of milliseconds
/// apart (the owner's report: 8 ms apart still split the combo), and a tenth of a second
/// is about as long as a single press on a combo's button can wait without feeling late
/// [design; hypothesis, to be tuned by the owner's playtest].
pub const COMBO_WINDOW: f32 = 0.1;
/// The Combo window row's choices, seconds (all below the shortest Hold time).
pub const COMBO_WINDOWS: [f32; 6] = [0.05, 0.075, 0.1, 0.125, 0.15, 0.2];

impl Default for Timing {
    fn default() -> Self {
        Timing { hold: HOLD_TIME, double: DOUBLE_TAP_WINDOW, combo: COMBO_WINDOW }
    }
}

/// The controls whose readers need the button really held, not a one-frame press: a press
/// of theirs on a combo's modifier acts once the combo window has passed and stays live
/// beside a later combo [design, §2a]. Movement and the stick's
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
    /// Presses of controls that need a real hold ([`held_nature`]) on a combo's modifier:
    /// down while the button is, once the combo window has passed and no combo used it.
    live: Vec<Control>,
}

impl Roles {
    /// A press on it waits for the release (or the double tap's window).
    fn deferred(&self) -> bool {
        !self.hold.is_empty() || !self.double.is_empty() || (self.modifier && !self.press.is_empty())
    }

    /// Part of a combo (its modifier or its button): a press waits the combo window.
    fn combo_part(&self) -> bool {
        self.modifier || !self.combo.is_empty()
    }

    /// What a press becomes once no combo can claim it: `second` when it came inside a
    /// double tap's window, `t` the seconds since it went down.
    fn after_wait(&self, second: bool, t: f32) -> Phase {
        if second && !self.double.is_empty() {
            Phase::SecondDown
        } else if self.press.is_empty() || self.deferred() {
            Phase::Down(t)
        } else {
            Phase::Live
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
enum Phase {
    #[default]
    Idle,
    /// Down inside the combo window: nothing acts yet; seconds since the press, and
    /// whether it came inside a double tap's window.
    Pending(f32, bool),
    /// Down, deciding; seconds since the press.
    Down(f32),
    /// Down, the press acting (not deferred).
    Live,
    HoldFired,
    /// Up after a first press; seconds since that press.
    WaitSecond(f32),
    SecondDown,
    /// Used up by this combo control until the button comes up (the combo's button).
    Used(Control),
    /// Used up by a combo until the button comes up (the combo's modifier, pressed inside
    /// the window): nothing on it acts.
    Swallowed,
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
    /// and presses on a button one of those shares (a combo's modifier or button too: their
    /// presses wait the combo window).
    pub fn resolved(binds: &[(Control, PadBinding)]) -> Vec<Control> {
        let roles = roles(binds);
        let mut out: Vec<Control> = binds.iter().filter(|(_, b)| matches!(b, PadBinding::Hold(_) | PadBinding::Double(_) | PadBinding::Combo { .. })).map(|(c, _)| *c).collect();
        for r in roles.values() {
            out.extend(r.press.iter().copied());
            out.extend(r.live.iter().copied());
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
        // Combos first: a button going down whose partner is down already. Inside the
        // window either may have come first; past it only the modifier may (it is not used
        // up then: a held control on it stays live).
        let mut claimed: Vec<GamepadButton> = Vec::new();
        for &b in &keys {
            for &(m, c) in &roles[&b].combo {
                let (nb, nm) = (button(b), button(m));
                let pending = |x: GamepadButton| matches!(self.buttons.get(&x).map(|s| s.phase), Some(Phase::Pending(..)));
                let swallow = if nb.just_pressed && nm.pressed {
                    nm.just_pressed || pending(m)
                } else if nm.just_pressed && nb.pressed && pending(b) {
                    true
                } else {
                    continue;
                };
                if claimed.contains(&b) {
                    continue;
                }
                self.buttons.entry(b).or_default().phase = Phase::Used(c);
                claimed.push(b);
                if swallow {
                    self.buttons.entry(m).or_default().phase = Phase::Swallowed;
                    claimed.push(m);
                } else {
                    used_modifiers.push(m);
                }
            }
        }
        for b in keys {
            let r = &roles[&b];
            let now = button(b);
            let st = self.buttons.entry(b).or_default();
            if now.just_pressed && !claimed.contains(&b) {
                let second = matches!(st.phase, Phase::WaitSecond(_));
                st.phase = if r.combo_part() { Phase::Pending(0.0, second) } else { r.after_wait(second, 0.0) };
            } else if now.pressed {
                match st.phase {
                    Phase::Pending(t, second) => {
                        let t = t + dt;
                        st.phase = if t >= timing.combo { r.after_wait(second, t) } else { Phase::Pending(t, second) };
                    }
                    Phase::Down(t) => {
                        let t = t + dt;
                        st.phase = if !r.hold.is_empty() && t >= timing.hold { Phase::HoldFired } else { Phase::Down(t) };
                    }
                    _ => {}
                }
            }
            if now.just_released || !now.pressed {
                // Let go inside the combo window with no combo: its own binding acts late.
                if let Phase::Pending(t, second) = st.phase {
                    pulses.extend(r.live.iter().copied());
                    st.phase = match r.after_wait(second, t) {
                        Phase::Live => {
                            pulses.extend(r.press.iter().copied());
                            Phase::Idle
                        }
                        Phase::SecondDown => {
                            pulses.extend(r.double.iter().copied());
                            Phase::Idle
                        }
                        p => p,
                    };
                }
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
            if now.pressed && !matches!(st.phase, Phase::Idle | Phase::Pending(..) | Phase::Swallowed | Phase::Used(_)) {
                down.extend(r.live.iter().copied());
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
        if live_modifier {
            r.live.push(*c);
        } else {
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
    fn a_combo_uses_up_its_button_and_the_buttons_own_press_waits_the_window() {
        let combo = PadBinding::Combo { modifier: LeftTrigger, button: South };
        let binds = [(Control::Hotkey5, combo), (Control::Activate, PadBinding::Press(PadInput::Button(South)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[LeftTrigger, South], 5, &mut log);
        assert_eq!(log, vec![(Control::Hotkey5, "down")], "Activate does not act");
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 1, &mut log);
        assert_eq!(log, vec![(Control::Hotkey5, "down"), (Control::Hotkey5, "up")]);
        // The button alone is its own press once the combo window has passed.
        log.clear();
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[South], frames(COMBO_WINDOW) - 1, &mut log);
        assert!(log.is_empty(), "still inside the combo window: {log:?}");
        run(&mut r, &mut pad, &binds, &[South], 3, &mut log);
        assert_eq!(log, vec![(Control::Activate, "down")], "then held as Activate");
        // A quick tap of the button alone is a late one-frame press.
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        log.clear();
        run(&mut r, &mut pad, &binds, &[South], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log, vec![(Control::Activate, "down"), (Control::Activate, "up")]);
        // Held past the window, the button is its own press: a modifier after it is no combo.
        log.clear();
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[South], frames(COMBO_WINDOW) + 2, &mut log);
        run(&mut r, &mut pad, &binds, &[South, LeftTrigger], 3, &mut log);
        assert!(!log.iter().any(|(c, _)| *c == Control::Hotkey5), "{log:?}");
    }

    /// The owner's playtest (2026-11-22): VATS on LB + RB, Sneak on LB.
    fn vats_on_lb_rb_and_sneak_on_lb() -> [(Control, PadBinding); 2] {
        [(Control::Vats, PadBinding::Combo { modifier: LeftTrigger, button: RightTrigger }), (Control::CrouchSneak, PadBinding::Press(PadInput::Button(LeftTrigger)))]
    }

    #[test]
    fn a_combo_pressed_nearly_together_fires_in_either_order_and_swallows_both_singles() {
        let binds = vats_on_lb_rb_and_sneak_on_lb();
        // LB a frame (about 17 ms) before RB, RB a frame before LB, both on one frame, and
        // the two just inside the window.
        for (first, second, gap) in [(LeftTrigger, RightTrigger, 1), (RightTrigger, LeftTrigger, 1), (LeftTrigger, RightTrigger, 0), (RightTrigger, LeftTrigger, frames(COMBO_WINDOW) - 1)] {
            let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
            run(&mut r, &mut pad, &binds, &[first], gap, &mut log);
            run(&mut r, &mut pad, &binds, &[first, second], 10, &mut log);
            run(&mut r, &mut pad, &binds, &[second], 3, &mut log);
            run(&mut r, &mut pad, &binds, &[], 5, &mut log);
            assert_eq!(log, vec![(Control::Vats, "down"), (Control::Vats, "up")], "{first:?} then {second:?}, {gap} frame(s) apart: no Sneak");
        }
    }

    #[test]
    fn lb_alone_still_sneaks() {
        let binds = vats_on_lb_rb_and_sneak_on_lb();
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        // Held: Sneak goes down once the combo window has passed and stays down until let go.
        run(&mut r, &mut pad, &binds, &[LeftTrigger], frames(COMBO_WINDOW) - 1, &mut log);
        assert!(log.is_empty(), "waiting for RB: {log:?}");
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 30, &mut log);
        assert_eq!(log, vec![(Control::CrouchSneak, "down")]);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log, vec![(Control::CrouchSneak, "down"), (Control::CrouchSneak, "up")]);
        // A quick tap: Sneak for one frame when it is let go.
        log.clear();
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 2, &mut log);
        run(&mut r, &mut pad, &binds, &[], 3, &mut log);
        assert_eq!(log, vec![(Control::CrouchSneak, "down"), (Control::CrouchSneak, "up")]);
    }

    #[test]
    fn a_modifier_held_past_the_window_stays_live_beside_its_combo() {
        let binds = vats_on_lb_rb_and_sneak_on_lb();
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[LeftTrigger], frames(COMBO_WINDOW) + 5, &mut log);
        run(&mut r, &mut pad, &binds, &[LeftTrigger, RightTrigger], 5, &mut log);
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 5, &mut log);
        assert_eq!(log, vec![(Control::CrouchSneak, "down"), (Control::Vats, "down"), (Control::Vats, "up")], "Sneak held through the combo");
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log.last(), Some(&(Control::CrouchSneak, "up")));
        // RB alone (no binding of its own): nothing.
        log.clear();
        run(&mut r, &mut pad, &binds, &[RightTrigger], 20, &mut log);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert!(log.is_empty(), "{log:?}");
    }

    #[test]
    fn a_held_control_on_a_combos_button_is_held_not_tapped() {
        // The owner's playtest (2026-11-22): Toggle POV on LS + RS, Sprint on LS. Sprint
        // must stay held while LS is, whichever of the two the combo took as its modifier.
        for combo in [PadBinding::Combo { modifier: LeftThumb, button: RightThumb }, PadBinding::Combo { modifier: RightThumb, button: LeftThumb }] {
            let binds = [(Control::TogglePov, combo), (Control::Sprint, PadBinding::Press(PadInput::Button(LeftThumb)))];
            let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
            let held_frames = |r: &mut PadResolver, pad: &mut Pad, log: &mut Vec<(Control, &'static str)>| {
                let mut n = 0;
                for _ in 0..frames(2.0) {
                    run(r, pad, &binds, &[LeftThumb], 1, log);
                    n += usize::from(r.get(Control::Sprint).is_some_and(|v| v.pressed));
                }
                n
            };
            let n = held_frames(&mut r, &mut pad, &mut log);
            assert!(n >= frames(2.0) - frames(COMBO_WINDOW) - 1, "{combo:?}: Sprint held {n} of {} frames", frames(2.0));
            assert_eq!(log, vec![(Control::Sprint, "down")], "{combo:?}: one press, held, no taps");
            run(&mut r, &mut pad, &binds, &[], 2, &mut log);
            assert_eq!(log, vec![(Control::Sprint, "down"), (Control::Sprint, "up")], "{combo:?}");
        }
    }

    #[test]
    fn a_modifiers_own_press_acts_on_release_only_when_no_combo_used_it() {
        let combo = PadBinding::Combo { modifier: LeftTrigger, button: South };
        let binds = [(Control::Hotkey5, combo), (Control::Vats, PadBinding::Press(PadInput::Button(LeftTrigger)))];
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[LeftTrigger], 3, &mut log);
        run(&mut r, &mut pad, &binds, &[], 2, &mut log);
        assert_eq!(log, vec![(Control::Vats, "down"), (Control::Vats, "up")]);
        // Used inside the window, and past it: no Vats either way.
        for held in [3, frames(COMBO_WINDOW) + 5] {
            log.clear();
            run(&mut r, &mut pad, &binds, &[LeftTrigger], held, &mut log);
            run(&mut r, &mut pad, &binds, &[LeftTrigger, South], 3, &mut log);
            run(&mut r, &mut pad, &binds, &[], 3, &mut log);
            assert!(!log.iter().any(|(c, _)| *c == Control::Vats), "the combo used the modifier: {log:?}");
            assert_eq!(log.iter().filter(|(c, w)| *c == Control::Hotkey5 && *w == "down").count(), 1);
        }
    }

    #[test]
    fn a_held_control_on_a_modifier_waits_the_window_then_aims() {
        let combo = PadBinding::Combo { modifier: LeftTrigger2, button: South };
        let binds = [(Control::Hotkey5, combo), (Control::Block, PadBinding::Press(PadInput::Button(LeftTrigger2)))];
        let mut resolved = PadResolver::resolved(&binds);
        resolved.sort_by_key(|c| format!("{c:?}"));
        assert_eq!(resolved, vec![Control::Block, Control::Hotkey5]);
        let (mut r, mut pad, mut log) = (PadResolver::default(), Pad::new(), Vec::new());
        run(&mut r, &mut pad, &binds, &[LeftTrigger2], frames(COMBO_WINDOW) + 3, &mut log);
        assert_eq!(log, vec![(Control::Block, "down")], "aims once the window has passed");
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
