//! The extender functions Just Assorted Mods (`JustAssortedMods.esp`) calls that no other
//! batch built, and the JIP events it waits for (key down and up, a weapon fired)
//! [observed: the mod's script sources, read privately with `fnvtool`-style SCTX dumps;
//! its loot menu JLM, sprint JVS, bullet time JBT, objectives overlay JVO and hold-breath
//! JHB modules]. Behaviour is from each function's page where it says, else from how
//! JAM uses it [hypothesis, tagged per function].

use crate::game::{Game, RunCtx};
use fnv_script::vm::{ArrayKind, Value, XKey, XVal};
use fnv_sim::FormId;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

/// The player's view as the engine tells it each frame, for WorldToScreen: the matrix
/// from Bevy world space (Y up; `to_bevy` of a game point) to clip space, column major
/// [design: the engine's camera; nvf-play `script_input::tell_view`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewTold {
    pub clip_from_world: [f32; 16],
}

/// This module's session state (not saved [hypothesis: each is set again by the mod's
/// scripts when they run; JAM sets them every time its menu opens or closes]).
#[derive(Debug, Default)]
pub struct JamState {
    /// References whose default activation DisableScriptedActivate turned off.
    pub no_scripted_activate: HashSet<FormId>,
    /// ToggleVanityWheel 0: the mouse wheel does not zoom the third-person camera.
    pub vanity_wheel_off: bool,
    /// ToggleDisableSaves 1: the game does not save.
    pub saves_disabled: bool,
    /// The camera, when the engine gives it.
    pub view: Option<ViewTold>,
    /// Keys and buttons held last frame, for the key events.
    pub keys_down: BTreeSet<u16>,
    /// The key the loot menu's take key was last set to from the player's Activate
    /// binding ([`Game::jam_take_follows_activate`]).
    pub take_followed: Option<f32>,
    /// Each model's sequences (name lower case, frequency), from the engine's hook.
    sequences: HashMap<String, Option<Vec<(String, f32)>>>,
}

/// A model's NiControllerSequences (name, frequency) by its path under `meshes\`; `None`
/// when the file does not open.
pub type SequenceLookup = Arc<dyn Fn(&str) -> Option<Vec<(String, f32)>> + Send + Sync>;

static SEQUENCES: Mutex<Option<SequenceLookup>> = Mutex::new(None);

/// The engine's reader of a model's sequences (GetAnimSequenceFrequency).
pub fn set_sequence_lookup(f: SequenceLookup) {
    if let Ok(mut g) = SEQUENCES.lock() {
        *g = Some(f);
    }
}

fn lookup_sequences(model: &str) -> Option<Vec<(String, f32)>> {
    let f = SEQUENCES.lock().ok()?.clone()?;
    f(model)
}

fn num(args: &[XVal], k: usize, d: f64) -> f64 {
    match args.get(k) {
        Some(XVal::Num(x)) => *x,
        _ => d,
    }
}

fn form(args: &[XVal], k: usize) -> Option<FormId> {
    match args.get(k)? {
        XVal::Ref(0) => None,
        XVal::Ref(r) => Some(FormId(*r)),
        _ => None,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// The begin-block code of an event name (`OnOpen`), from the function table.
fn block_code(g: &Game, name: &str) -> Option<u16> {
    (0u16..0x40).find(|&c| g.data.table.block_name(c).is_some_and(|n| n.eq_ignore_ascii_case(name)))
}

/// A Bevy-space point of a game-space one (`nvf-play` space.rs).
fn to_bevy(p: [f32; 3]) -> [f32; 3] {
    [p[0], p[2], -p[1]]
}

/// `p` (game space) on the screen: x and y as fractions of the screen from its top
/// left, the depth in front of the camera (negative behind it).
pub fn project(view: &ViewTold, p: [f32; 3]) -> (f64, f64, f64) {
    let v = to_bevy(p);
    let m = &view.clip_from_world;
    let row = |r: usize| f64::from(m[r] * v[0] + m[4 + r] * v[1] + m[8 + r] * v[2] + m[12 + r]);
    let (cx, cy, cw) = (row(0), row(1), row(3));
    let w = if cw.abs() < 1e-6 { 1e-6f64.copysign(cw) } else { cw };
    ((cx / w + 1.0) / 2.0, (1.0 - cy / w) / 2.0, cw)
}

/// One of this module's functions by catalogue name (lower case); `None` when it is not
/// one of them.
pub(crate) fn call(g: &mut Game, _ctx: &RunCtx, name: &str, subject: Option<FormId>, args: &[XVal]) -> Option<Result<XVal, String>> {
    let zero = XVal::Num(0.0);
    Some(Ok(match name {
        // `ref.DisableScriptedActivate 1/0`: while on, activating the reference runs its
        // OnActivate block but not its default action, even when the block calls
        // Activate [hypothesis: JLM wraps `Activate PlayerRef 1` in it on a container
        // with an OnActivate block, to run the block's effects without opening the
        // container menu over its own loot menu].
        "disablescriptedactivate" => {
            if let Some(r) = subject {
                if num(args, 0, 0.0) != 0.0 {
                    g.jam.no_scripted_activate.insert(r);
                } else {
                    g.jam.no_scripted_activate.remove(&r);
                }
            }
            zero
        }
        // The container's open (CONT SNAM) and close (QNAM) sounds, on a reference or its
        // base [documented: the record's fields, schemas/fnv; hypothesis: the functions
        // read them, as their names say and JLM plays them when it opens a container
        // without an Open sequence].
        "getcontaineropensound" | "getcontainerclosesound" => {
            let Some(f) = subject.or_else(|| form(args, 0)) else { return Some(Ok(XVal::Ref(0))) };
            let base = crate::functions::base_of(g, f).unwrap_or(f);
            let sub = if name == "getcontaineropensound" { "SNAM" } else { "QNAM" };
            match crate::nvse::record_value(g, base, sub, "") {
                Some(XVal::Ref(r)) => XVal::Ref(r),
                _ => XVal::Ref(0),
            }
        }
        // `ref.FakeScriptEvent "event" [filter]`: runs the reference's script block of that
        // event as if it happened, `filter` the block's parameter and action reference;
        // 1 when a block ran [documented: the catalogue's success:0/1; hypothesis: the
        // block runs at once and only the reference's own script, JLM's OnOpen and
        // OnClose of a container it loots].
        "fakescriptevent" => {
            let (Some(r), Some(XVal::Str(ev))) = (subject, args.first()) else { return Some(Ok(zero)) };
            let Some(code) = block_code(g, ev) else { return Some(Ok(zero)) };
            let by = form(args, 1);
            yes(g.ref_event(r, code, by.map(|b| Value::Ref(b.0)), by))
        }
        // `ref.GetAnimSequenceFrequency sequence`: the frequency of that sequence in the
        // reference's model, -1 without one [documented: the catalogue's frequency:float;
        // hypothesis: -1 for none, as JLM tests `!= -1` before PlayAnimSequence].
        "getanimsequencefrequency" | "getanimseqfreq" => {
            let none = XVal::Num(-1.0);
            let (Some(r), Some(XVal::Str(seq))) = (subject, args.first()) else { return Some(Ok(none)) };
            let base = crate::functions::base_of(g, r).unwrap_or(r);
            let Some(XVal::Str(model)) = crate::nvse::record_value(g, base, "MODL", "") else { return Some(Ok(none)) };
            let key = model.to_ascii_lowercase();
            let seqs = g.jam.sequences.entry(key.clone()).or_insert_with(|| lookup_sequences(&key)).clone();
            seqs.and_then(|s| s.into_iter().find(|(n, _)| n.eq_ignore_ascii_case(seq)).map(|(_, f)| XVal::Num(f64::from(f)))).unwrap_or(none)
        }
        // `ToggleVanityWheel [1/0]`: whether the mouse wheel zooms the third-person
        // camera; given, sets it [documented: the catalogue's isToggled:0/1, the
        // optional toggle]. The state after the call [hypothesis].
        "togglevanitywheel" => {
            if let Some(XVal::Num(x)) = args.first() {
                g.jam.vanity_wheel_off = *x == 0.0;
            }
            yes(!g.jam.vanity_wheel_off)
        }
        // `ToggleDisableSaves 1/0`: the game does not save while on [documented: the
        // catalogue's onOrOff; hypothesis: every save, autosaves too; JBT turns it on
        // through its bullet time].
        "toggledisablesaves" => {
            g.jam.saves_disabled = num(args, 0, 0.0) != 0.0;
            zero
        }
        // The VATS body-part outline JBT draws on its target [not built: the engine
        // draws no outline; handled so the scripts go on].
        "highlightbodypartalt" | "deactivateallhighlightsalt" => zero,
        // `WorldToScreen xOut yOut zOut x y z handleType [ref]`: the point (x y z added
        // to the reference's position with one, else in the world) on the screen; the VM
        // writes the outputs (`out_params`). x and y are fractions of the screen from its
        // top left [hypothesis: JVO's markers multiply them by `screen()`'s width and
        // height], z the depth in front of the camera [hypothesis]. 1 when on screen
        // [documented: isOnScreen]. Off it, handleType 1 keeps the point at the screen's
        // edge [hypothesis]; behind the camera the point is mirrored, so it lands on the
        // side it is on [hypothesis].
        "worldtoscreen" => {
            let mut p = [num(args, 0, 0.0) as f32, num(args, 1, 0.0) as f32, num(args, 2, 0.0) as f32];
            if let Some(r) = form(args, 4) {
                match g.ref_pos_rot(r) {
                    Some((at, _)) => (0..3).for_each(|i| p[i] += at[i]),
                    None => return Some(Ok(out(g, false, -1.0, -1.0, 0.0))),
                }
            }
            let Some(view) = g.jam.view else { return Some(Ok(out(g, false, -1.0, -1.0, 0.0))) };
            let (mut x, mut y, depth) = project(&view, p);
            if depth <= 0.0 {
                // Mirrored about the middle, and pushed past the nearest edge when that
                // lands it on the screen.
                let (dx, dy) = (0.5 - x, 0.5 - y);
                let m = dx.abs().max(dy.abs()).max(1e-6);
                let k = if m < 0.5 { 0.505 / m } else { 1.0 };
                (x, y) = (0.5 + dx * k, 0.5 + dy * k);
            }
            let on = depth > 0.0 && (0.0..=1.0).contains(&x) && (0.0..=1.0).contains(&y);
            if !on && num(args, 3, 0.0) as i64 == 1 {
                (x, y) = (x.clamp(0.0, 1.0), y.clamp(0.0, 1.0));
            }
            out(g, on, x, y, depth)
        }
        // `ref.SetDefaultOpen 1/0` (vanilla, but without an opcode in the catalogue, so
        // a mod's source reaches it by name): the reference is open (1) or closed by
        // default, as the placed reference's Open by Default flag [documented: catalogue
        // summary]; the open flag GetOpenState reads [hypothesis: the same state; JLM
        // sets it on a container it opens without an Open sequence and clears it on
        // closing].
        "setdefaultopen" => {
            let Some(r) = subject else { return Some(Err("SetDefaultOpen without a reference".into())) };
            match g.world.get(r) {
                Ok(s) => s.open = num(args, 0, 0.0) != 0.0,
                Err(e) => return Some(Err(e.to_string())),
            }
            zero
        }
        _ => return None,
    }))
}

/// WorldToScreen's answer for the VM's outputs: [on, x, y, z].
fn out(g: &mut Game, on: bool, x: f64, y: f64, z: f64) -> XVal {
    let entries = [yes(on), XVal::Num(x), XVal::Num(y), XVal::Num(z)].into_iter().enumerate().map(|(i, v)| (XKey::Num(i as f64), v)).collect();
    XVal::Array(g.ext.new_array(ArrayKind::List, entries))
}


impl Game<'_> {
    /// JIP's key events of this frame: OnKeyDown for each key or mouse button (wheel
    /// included, 264/265) held now and not last frame, OnKeyUp for each let go; the
    /// handler gets the code [documented: SetOnKeyDownEventHandler's keyID filter;
    /// hypothesis: the moment, and that a key a script disabled sends them too: JLM
    /// disables its keys so the game's controls ignore them and reads them from its
    /// handler].
    /// JAM's loot menu takes with its "1st Key" (the global `JLMKey1`), 18 by default,
    /// the original's Activate key E, while the player's Activate opens the container
    /// [observed: JLMScript registers its key handler on JLMKey1 and DisableKeys it while
    /// the menu shows]. While that key is still the default (or the one set here last),
    /// it follows the player's own Activate binding, and the menu takes its settings
    /// again (`JLM.iSettingUpdate` 1: handlers registered anew, the key hint rewritten);
    /// a key the player picked on JAM's page stays [owner 2026-11-05: "follow interact set
    /// by player"; design].
    pub fn jam_take_follows_activate(&mut self) {
        const DEFAULT: f32 = 18.0;
        const ACTIVATE: usize = 5;
        let Some(global) = self.world.forms.by_edid("JLMKey1") else { return };
        let Some(key) = self.input.bindings.get(ACTIVATE).and_then(|b| b.0) else { return };
        let key = f32::from(key);
        let Ok(now) = self.world.global(global) else { return };
        if now == key || !(now == DEFAULT || Some(now) == self.jam.take_followed) {
            return;
        }
        if self.world.set_global(global, key).is_err() {
            return;
        }
        self.jam.take_followed = Some(key);
        let quest = self.world.forms.by_edid("JLM");
        if let Some((inst, script)) = quest.and_then(|q| self.instance_of(q)) {
            if let Some(index) = self.data.script(script).and_then(|s| s.var_index("iSettingUpdate")) {
                self.store_raw(inst, Some(script), index, Value::Num(1.0));
            }
        }
    }

    pub fn jam_key_events(&mut self) {
        if !self.jam_keys_changed() {
            return;
        }
        let now: BTreeSet<u16> = self.input.held.union(&self.input.script_held).copied().collect();
        let down: Vec<u16> = now.difference(&self.jam.keys_down).copied().collect();
        let up: Vec<u16> = self.jam.keys_down.difference(&now).copied().collect();
        self.jam.keys_down = now;
        for k in down {
            self.jip_event("keydown", None, vec![XVal::Num(f64::from(k))]);
        }
        for k in up {
            self.jip_event("keyup", None, vec![XVal::Num(f64::from(k))]);
        }
    }

    /// Whether a key or button went down or up since the last key events.
    pub fn jam_keys_changed(&self) -> bool {
        !self.input.held.union(&self.input.script_held).copied().eq(self.jam.keys_down.iter().copied())
    }

    /// Whether DisableScriptedActivate holds `target`'s default action.
    pub(crate) fn scripted_activate_off(&self, target: FormId) -> bool {
        self.jam.no_scripted_activate.contains(&target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A view looking north (game +Y) from the origin: Bevy looks along -Z, so game +Y is
    /// straight ahead; a 90° vertical field of view, square screen.
    fn north() -> ViewTold {
        // clip = P * V with V the identity (camera at the origin looking along -Z).
        let (f, near) = (1.0f32, 1.0f32);
        // Bevy's infinite reverse-Z perspective: x' = f x, y' = f y, z' = near, w' = -z.
        ViewTold { clip_from_world: [f, 0.0, 0.0, 0.0, 0.0, f, 0.0, 0.0, 0.0, 0.0, 0.0, -1.0, 0.0, 0.0, near, 0.0] }
    }

    #[test]
    fn a_point_ahead_is_mid_screen_and_one_behind_is_not() {
        let v = north();
        let (x, y, d) = project(&v, [0.0, 100.0, 0.0]);
        assert!((x - 0.5).abs() < 1e-9 && (y - 0.5).abs() < 1e-9 && (d - 100.0).abs() < 1e-6);
        // To the right (east, game +X) and above (game +Z) at 45°: the right edge's top.
        let (x, y, _) = project(&v, [100.0, 100.0, 100.0]);
        assert!((x - 1.0).abs() < 1e-9 && y.abs() < 1e-9);
        let (_, _, d) = project(&v, [0.0, -100.0, 0.0]);
        assert!(d < 0.0);
    }
}
