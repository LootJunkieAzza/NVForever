//! The input the extender's input functions read and change (task D6;
//! docs/spec-notes/ext-input.md). The engine fills it each frame (keys held, the
//! controls' bindings) and takes back the disabled keys and the taps. Codes are the
//! extender's: DirectX scancodes for keys, 256 + n for mouse buttons, controls 0–27
//! [documented: ext-input.md §1]. Not saved [hypothesis: ext-input.md §5 leaves the
//! lifetime open].

use fnv_script::vm::XVal;
use std::collections::BTreeSet;

/// The first mouse code [documented: Template:DirectX Scancodes].
pub const MOUSE_BASE: u16 = 256;
/// The number of controls [documented: IsControlPressed, codes 0–27].
pub const CONTROLS: usize = 28;
/// What GetControl gives for a control without a key or button [hypothesis: the INI's
/// "none" byte, FF].
pub const UNBOUND: f64 = 255.0;

#[derive(Debug, Clone, Default)]
pub struct ScriptInput {
    /// Keys and mouse buttons held this frame, as the player pressed them.
    pub held: BTreeSet<u16>,
    /// Each control's key and mouse button (`Bindings` in the engine).
    pub bindings: Vec<(Option<u16>, Option<u16>)>,
    pub disabled_keys: BTreeSet<u16>,
    pub disabled_controls: BTreeSet<u8>,
    /// Keys a script holds down (HoldKey) until ReleaseKey.
    pub script_held: BTreeSet<u16>,
    /// Taps asked for this frame (TapKey, TapControl as its key); the engine presses each
    /// for one frame [documented: TapKey].
    pub taps: Vec<u16>,
}

impl ScriptInput {
    fn binding(&self, control: i64) -> (Option<u16>, Option<u16>) {
        usize::try_from(control).ok().and_then(|c| self.bindings.get(c)).copied().unwrap_or_default()
    }

    /// Disabled by DisableKey, or bound to a control DisableControl disabled
    /// [documented: IsKeyDisabled].
    pub fn key_disabled(&self, code: u16) -> bool {
        self.disabled_keys.contains(&code)
            || self.disabled_controls.iter().any(|&c| {
                let (k, m) = self.binding(i64::from(c));
                k == Some(code) || m == Some(code)
            })
    }

    /// Every key and button the engine should treat as not pressed this frame.
    pub fn blocked(&self) -> BTreeSet<u16> {
        let mut out = self.disabled_keys.clone();
        for &c in &self.disabled_controls {
            let (k, m) = self.binding(i64::from(c));
            out.extend(k);
            out.extend(m);
        }
        out
    }

    /// Held, by the player or a script; with `ignore` 2 a disabled key counts too
    /// [documented: IsKeyPressed].
    fn pressed(&self, code: u16, ignore: bool) -> bool {
        (self.held.contains(&code) || self.script_held.contains(&code)) && (ignore || !self.key_disabled(code))
    }

    fn pressed_list(&self, mouse: bool, ignore: bool) -> Vec<u16> {
        let mut all: BTreeSet<u16> = self.held.union(&self.script_held).copied().collect();
        all.retain(|&c| (c >= MOUSE_BASE) == mouse && (ignore || !self.key_disabled(c)));
        all.into_iter().collect()
    }
}

fn int(args: &[XVal], k: usize, dflt: i64) -> i64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x as i64,
        _ => dflt,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(f64::from(u8::from(b)))
}

/// The input functions [documented: ext-input.md §2–3]; `None` for any other name.
pub(crate) fn call(input: &mut ScriptInput, name: &str, args: &[XVal]) -> Option<XVal> {
    let code = u16::try_from(int(args, 0, -1)).ok();
    let ignore = int(args, 1, 0) == 2;
    Some(match name {
        "iskeypressed" => yes(code.is_some_and(|c| input.pressed(c, ignore))),
        "iscontrolpressed" => {
            let (k, m) = input.binding(int(args, 0, -1));
            yes([k, m].into_iter().flatten().any(|c| input.pressed(c, ignore)))
        }
        // `GetControl control [type]`: 0 the key, 1 the mouse button [documented].
        "getcontrol" | "getaltcontrol" => {
            let (k, m) = input.binding(int(args, 0, -1));
            let mouse = name == "getaltcontrol" || int(args, 1, 0) == 1;
            XVal::Num((if mouse { m } else { k }).map_or(UNBOUND, f64::from))
        }
        // The n-th key held, in scancode order [hypothesis: the order is open]; -1 when
        // fewer are held [hypothesis].
        "getkeypress" | "getmousebuttonpress" => {
            let list = input.pressed_list(name == "getmousebuttonpress", ignore);
            XVal::Num(usize::try_from(int(args, 0, -1)).ok().and_then(|n| list.get(n)).map_or(-1.0, |&c| f64::from(c)))
        }
        "getnumkeyspressed" | "getnummousebuttonspressed" => {
            let ignore = int(args, 0, 0) == 2;
            XVal::Num(input.pressed_list(name == "getnummousebuttonspressed", ignore).len() as f64)
        }
        "disablekey" | "enablekey" => {
            if let Some(c) = code {
                if name == "disablekey" {
                    input.disabled_keys.insert(c);
                } else {
                    input.disabled_keys.remove(&c);
                }
            }
            XVal::Num(0.0)
        }
        "iskeydisabled" => yes(code.is_some_and(|c| input.key_disabled(c))),
        "disablecontrol" | "enablecontrol" => {
            if let Ok(c) = u8::try_from(int(args, 0, -1)) {
                if name == "disablecontrol" {
                    input.disabled_controls.insert(c);
                } else {
                    input.disabled_controls.remove(&c);
                }
            }
            XVal::Num(0.0)
        }
        "iscontroldisabled" => yes(u8::try_from(int(args, 0, -1)).is_ok_and(|c| input.disabled_controls.contains(&c))),
        "tapkey" => {
            input.taps.extend(code);
            XVal::Num(0.0)
        }
        // [documented: TapControl] taps whatever key (else mouse button) the control has.
        "tapcontrol" => {
            let (k, m) = input.binding(int(args, 0, -1));
            input.taps.extend(k.or(m));
            XVal::Num(0.0)
        }
        "holdkey" => {
            input.script_held.extend(code);
            XVal::Num(0.0)
        }
        "releasekey" => {
            if let Some(c) = code {
                input.script_held.remove(&c);
            }
            XVal::Num(0.0)
        }
        _ => return None,
    })
}
