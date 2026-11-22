//! The extender's input functions' side in the engine (task D6;
//! docs/spec-notes/ext-input.md): each frame, right after Bevy reads the devices, the
//! keys and mouse buttons held and the controls' bindings go to the script runtime
//! (`fnv_game::script_input`), the keys scripts disabled go to [`ScriptBlocked`] (which
//! `bindings::ControlInput` leaves out, so the player's controls ignore them), and the
//! keys scripts tapped are pressed for one frame.

use crate::bindings::{dik_key, Binding, Bindings, GAME_CONTROLS, GAME_INI_CONTROLS};
use bevy::input::keyboard::KeyboardInput;
use bevy::input::mouse::MouseButtonInput;
use bevy::input::ButtonState;
use bevy::prelude::*;
use bevy::input::mouse::MouseWheel;
use bevy::window::PrimaryWindow;
use std::collections::{BTreeSet, HashSet};
use fnv_sim::FormId;
use fnv_game::script_input::MOUSE_BASE;

pub struct ScriptInputPlugin;

/// Physical script input observes acknowledged cursor state before callbacks.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct ScriptInputSet;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptBlocked>()
            .add_message::<MouseWheel>()
            .configure_sets(PreUpdate, ScriptInputSet.after(bevy::input::InputSystems))
            // [design, #2200] Scripts see consumed switching presses as absent too.
            .add_systems(PreUpdate, (script_input, key_events).chain().in_set(ScriptInputSet).after(crate::pad::device::automatic).after(crate::pad::update_stick_buttons))
            // The player's state the scripts read (sights up, for the HUD bars' aiming
            // condition) is handed over after combat has updated it this frame and before
            // the scripts run [design]. `tell_magazine` keeps its own place: it also
            // applies the scripts' pending ammo writes, which must reach combat first.
            .add_systems(Update, tell_player_state.after(crate::PlaySet::Activation).before(crate::PlaySet::Scripts))
            .add_systems(Update, (tell_magazine, mod_file_roots, tell_view))
            .add_systems(Update, script_moves.after(crate::PlaySet::Input));
    }
}

/// The keys and mouse buttons scripts disabled (DisableKey, DisableControl).
#[derive(Resource, Default)]
pub struct ScriptBlocked {
    pub keys: Vec<KeyCode>,
    pub mouse: Vec<MouseButton>,
    /// ToggleVanityWheel 0: the wheel does not zoom the third-person camera
    /// (`fnv_game::nvse_jam`).
    pub wheel: bool,
}

/// The extender's codes of the wheel turned up and down [documented: ext-input.md §1,
/// 264 wheel up, 265 wheel down].
const WHEEL_UP: u16 = MOUSE_BASE + 8;
const WHEEL_DOWN: u16 = MOUSE_BASE + 9;

/// The extender's code of a mouse button: 256 left, 257 right, 258 middle, then the extra
/// buttons [documented: Template:DirectX Scancodes].
fn mouse_code(b: MouseButton) -> Option<u16> {
    Some(MOUSE_BASE
        + match b {
            MouseButton::Left => 0,
            MouseButton::Right => 1,
            MouseButton::Middle => 2,
            MouseButton::Back => 3,
            MouseButton::Forward => 4,
            MouseButton::Other(n) if n < 8 => n,
            MouseButton::Other(_) => return None,
        })
}

fn mouse_of(code: u16) -> Option<MouseButton> {
    [MouseButton::Left, MouseButton::Right, MouseButton::Middle, MouseButton::Back, MouseButton::Forward, MouseButton::Other(5), MouseButton::Other(6), MouseButton::Other(7)]
        .into_iter()
        .find(|&b| mouse_code(b) == Some(code))
}

fn key_code(k: KeyCode) -> Option<u16> {
    (0u8..=255).find(|&c| dik_key(c) == Some(k)).map(u16::from)
}

/// The keys and mouse buttons scripts pressed that were up when they did: only these are
/// theirs to let go. One the player presses on the device while a script holds it becomes
/// the player's.
#[derive(Default)]
struct Pressed {
    /// Last frame's taps and the keys the scripts held.
    last: (Vec<u16>, Vec<u16>),
    keys: HashSet<KeyCode>,
    mouse: HashSet<MouseButton>,
}

impl Pressed {
    fn player_pressed(&mut self, typed: &[(KeyCode, bool)], clicked: &[(MouseButton, bool)]) {
        for (k, _) in typed {
            self.keys.remove(k);
        }
        for (b, _) in clicked {
            self.mouse.remove(b);
        }
    }

    fn press(&mut self, c: u16, keys: &mut ButtonInput<KeyCode>, mouse: &mut ButtonInput<MouseButton>) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().for_each(|b| {
                if !mouse.pressed(b) {
                    self.mouse.insert(b);
                    mouse.press(b);
                }
            }),
            None => dik_key(c as u8).into_iter().for_each(|k| {
                if !keys.pressed(k) {
                    self.keys.insert(k);
                    keys.press(k);
                }
            }),
        }
    }

    /// Let go of a key or mouse button the scripts pressed, unless the player holds it
    /// [hypothesis: ReleaseControl, ReleaseKey and a tap's end let go of the script's
    /// press only, and the key the player holds stays down. JAM's sprint holds Forward
    /// and releases it as the sprint ends in first person, and the player holding W walks
    /// on].
    fn release(&mut self, c: u16, keys: &mut ButtonInput<KeyCode>, mouse: &mut ButtonInput<MouseButton>) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().filter(|b| self.mouse.remove(b)).for_each(|b| mouse.release(b)),
            None => dik_key(c as u8).into_iter().filter(|k| self.keys.remove(k)).for_each(|k| keys.release(k)),
        }
    }
}

/// What the player did on the devices this frame: key and button presses (the wheel is
/// read from its own messages). Absent without the input plugin.
#[derive(bevy::ecs::system::SystemParam)]
struct Devices<'w, 's> {
    typed: Option<MessageReader<'w, 's, KeyboardInput>>,
    clicked: Option<MessageReader<'w, 's, MouseButtonInput>>,
}

impl Devices<'_, '_> {
    /// The keys and mouse buttons pressed this frame, each with whether the actions see
    /// the press (the input switch consumes its switching press) [design, #2200].
    #[allow(clippy::type_complexity)]
    fn presses(&mut self, switching: Option<&crate::pad::SwitchingInput>) -> (Vec<(KeyCode, bool)>, Vec<(MouseButton, bool)>) {
        let mut keys = Vec::new();
        let mut buttons = Vec::new();
        if let Some(typed) = self.typed.as_mut() {
            for k in typed.read().filter(|k| k.state == ButtonState::Pressed) {
                keys.push((k.key_code, switching.is_none_or(|s| s.allows(k))));
            }
        }
        if let Some(clicked) = self.clicked.as_mut() {
            for b in clicked.read().filter(|b| b.state == ButtonState::Pressed) {
                buttons.push((b.button, switching.is_none_or(|s| s.allows_mouse(b))));
            }
        }
        (keys, buttons)
    }
}

#[allow(clippy::too_many_arguments)]
fn script_input(
    mut script: Option<ResMut<crate::script::ScriptState>>,
    mut bindings: Option<ResMut<Bindings>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut wheel: MessageReader<MouseWheel>,
    primary: Query<Entity, With<PrimaryWindow>>,
    mut blocked: ResMut<ScriptBlocked>,
    switching: Option<Res<crate::pad::SwitchingInput>>,
    pads: crate::pad::ActionPads,
    (sticks, resolver): (Option<Res<crate::pad::StickButtons>>, Option<Res<crate::pad::resolver::PadResolver>>),
    mut devices: Devices,
    mut ours: Local<Pressed>,
) {
    let (typed, clicked) = devices.presses(switching.as_deref());
    ours.player_pressed(&typed, &clicked);
    // Last frame's taps and the keys the scripts held.
    let (mut tapped, mut script_held) = std::mem::take(&mut ours.last);
    // Last frame's taps end.
    for c in tapped.drain(..) {
        ours.release(c, &mut keys, &mut mouse);
    }
    // Wheel directions are physical pulses, not MouseButton::Other aliases. Drain
    // current messages even without a runtime/window so they cannot replay later.
    let window = primary.single().ok();
    let mut wheel_codes = [false; 2];
    for event in wheel.read() {
        if Some(event.window) != window || !event.y.is_finite() { continue; }
        if event.y > 0.0 { wheel_codes[0] = true; }
        else if event.y < 0.0 { wheel_codes[1] = true; }
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else {
        ours.last.1 = script_held;
        return;
    };
    blocked.wheel = rt.game.jam.vanity_wheel_off;
    let input = &mut rt.game.input;
    // Keys scripts held last frame and no longer hold (ReleaseKey, ReleaseControl) are let go.
    for c in script_held.drain(..).filter(|c| !input.script_held.contains(c)) {
        ours.release(c, &mut keys, &mut mouse);
    }
    // SetControl's changes reach the player's controls first.
    if let Some(b) = bindings.as_deref_mut() {
        for (c, mouse, code) in std::mem::take(&mut input.rebinds) {
            let Some(&(control, _, _)) = GAME_CONTROLS.get(c) else { continue };
            let old = b.get(control);
            let new = if mouse {
                Binding { mouse: code.and_then(mouse_of), ..old }
            } else {
                Binding { key: code.and_then(|k| u8::try_from(k).ok()).and_then(dik_key), ..old }
            };
            b.set(control, new);
        }
    }
    let action_keys = switching.as_ref().map_or_else(|| std::borrow::Cow::Borrowed(&*keys), |s| s.filter(&keys));
    let action_mouse = switching.as_ref().map_or_else(|| std::borrow::Cow::Borrowed(&*mouse), |s| s.filter(&mouse));
    input.held = held_codes(&action_keys, &action_mouse, &typed, &clicked);
    drop((action_keys, action_mouse));
    // 264/265 are the documented up/down wheel codes. Rebuilding held each frame
    // releases a direction immediately when that frame has no matching message.
    input.held.extend(wheel_codes.into_iter().zip([WHEEL_UP, WHEEL_DOWN])
        .filter_map(|(pressed, code)| pressed.then_some(code)));
    if let Some(b) = bindings.as_deref() {
        // The engine's controls are the extender's 0–27, in the same order.
        input.bindings = GAME_CONTROLS[..GAME_INI_CONTROLS].iter().map(|(c, _, _)| {
            let bind = b.get(*c);
            (bind.key.and_then(key_code), bind.mouse.and_then(mouse_code))
        }).collect();
    }
    // Before taps/holds below alter engine keys: use the published input snapshot,
    // selected device and the same controller binding reader as ControlInput.
    // DisableControl blocks gameplay, not these documented event transitions.
    input.event_controls = bindings.as_deref().map(|b| {
        let controller = b.device == crate::bindings::InputDevice::Controller;
        let keys = !controller || crate::bindings::keys_with_pad();
        GAME_CONTROLS.iter().take(fnv_game::script_input::CONTROLS).enumerate().fold(0, |mask, (c, (control, _, _))| {
            let keyboard = keys && input.bindings.get(c).is_some_and(|(k, m)| [*k, *m].into_iter().flatten().any(|code| input.held.contains(&code)));
            let pad = controller && crate::bindings::pad_control_held(b, *control, resolver.as_deref(), &pads, sticks.as_deref());
            mask | (u32::from(keyboard || pad) << c)
        })
    });
    set_blocked(&mut blocked, input);
    for c in std::mem::take(&mut input.taps).into_iter().chain(input.script_held.iter().copied()) {
        ours.press(c, &mut keys, &mut mouse);
        if !input.script_held.contains(&c) {
            tapped.push(c);
        }
    }
    script_held.extend(input.script_held.iter().copied());
    ours.last = (tapped, script_held);
}

/// The keys and mouse buttons held this frame, by the extender's codes, with those pressed
/// this frame and already let go: a tap shorter than a frame (a quick press at a low
/// frame rate) is held for that one frame, so it gives scripts one key-down and, the frame
/// after, one key-up, as the wheel's notch does [hypothesis: the original reads the keys
/// buffered and never misses a press; observed: at ~60 ms frames an 11 ms press of V
/// reached no handler, and JAM's bullet time stayed on]. `keys` and `mouse` are the
/// actions' view (#2200); the presses are this frame's device messages, each with whether
/// the actions see it, so a consumed switching press is no tap either.
fn held_codes(keys: &ButtonInput<KeyCode>, mouse: &ButtonInput<MouseButton>, typed: &[(KeyCode, bool)], clicked: &[(MouseButton, bool)]) -> BTreeSet<u16> {
    let keys = keys.get_pressed().copied().chain(typed.iter().filter(|p| p.1).map(|p| p.0)).filter_map(key_code);
    let buttons = mouse.get_pressed().copied().chain(clicked.iter().filter(|p| p.1).map(|p| p.0)).filter_map(mouse_code);
    keys.chain(buttons).collect()
}

/// The keys and buttons the player's controls leave out: those scripts disabled.
fn set_blocked(blocked: &mut ScriptBlocked, input: &fnv_game::script_input::ScriptInput) {
    let codes = input.blocked();
    blocked.keys = codes.iter().filter(|&&c| c < MOUSE_BASE).filter_map(|&c| dik_key(c as u8)).collect();
    blocked.mouse = codes.iter().filter_map(|&c| mouse_of(c)).collect();
}

/// JIP's key events of this frame (`fnv_game::nvse_jam`), run before the player's controls
/// read the keys, and the keys their handlers disabled left out of those controls at once
/// [hypothesis: the original's extender sees a key before the game does, so a handler's
/// DisableControl holds the key's own control from its first frame: JAM's sprint
/// (Left Shift, also the game's Run key) disables Run in its handler and needs the player
/// still running on the next frame].
/// With `NVF_PLAY_TEST_KEYS` set, each key-down and key-up sent is logged (a test's proof
/// of one of each per press).
fn key_events(mut script: Option<ResMut<crate::script::ScriptState>>, sim: Option<ResMut<crate::sim::SimState>>, mut blocked: ResMut<ScriptBlocked>, mut logged: Local<Option<bool>>) {
    let (Some(rt), Some(mut sim)) = (script.as_deref_mut().and_then(|s| s.rt.as_mut()), sim) else { return };
    if !rt.game.jam_keys_changed() {
        return;
    }
    let before = logged.get_or_insert_with(|| std::env::var_os("NVF_PLAY_TEST_KEYS").is_some()).then(|| rt.game.jam.keys_down.clone());
    crate::script::with_world(rt, &mut sim, |rt| rt.game.jam_key_events());
    if let Some(before) = before {
        let after = &rt.game.jam.keys_down;
        let down: Vec<_> = after.difference(&before).collect();
        let up: Vec<_> = before.difference(after).collect();
        info!("script key events: down {down:?} up {up:?}");
    }
    set_blocked(&mut blocked, &rt.game.input);
}

/// The rounds in the player's magazine for GetPlayerCurrentAmmoRounds.
fn tell_magazine(mut combat: Option<ResMut<crate::combat::Combat>>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    // SetPlayerCurrentAmmoRounds: the rounds go in the weapon in hand.
    if let (Some(n), Some(c)) = (rt.game.other.set_magazine.take(), combat.as_deref_mut()) {
        if let Some((w, _)) = c.magazine_for_save() {
            c.restore_magazine((w, n), None);
        }
    }
    rt.game.player_magazine = combat.and_then(|c| c.magazine_for_save()).map(|m| m.1);
}

/// The player's speed resources script_moves reads back for GetRunSpeed.
type PlayerSpeeds<'w> = (Option<Res<'w, crate::player::MoveSpeeds>>, Option<Res<'w, crate::player::StatSpeed>>, Option<Res<'w, crate::player::PerkSpeed>>);

/// What scripts ask of the player's and actors' moves (`fnv_game::nvse_jg9`: Jump,
/// SetAutoMove, SetPlayerMovementFlags, SetVelEx), after the controls are read so this
/// frame's moves carry them; and what those functions read back: the player's run speed,
/// spread, grenade hold and view.
#[allow(clippy::too_many_arguments)]
fn script_moves(
    mut script: Option<ResMut<crate::script::ScriptState>>,
    mut control: Option<ResMut<crate::control::PlayerControl>>,
    mut intent: Option<ResMut<crate::player::MoveIntent>>,
    mut phys: Option<ResMut<crate::phys::PhysState>>,
    mut knocks: Option<ResMut<Messages<crate::combat::Knockback>>>,
    combat: Option<Res<crate::combat::Combat>>,
    speeds: PlayerSpeeds,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let st = &mut rt.game.other.jg9;
    for who in std::mem::take(&mut st.jumps) {
        if who == FormId::PLAYER_REF {
            if let Some(i) = intent.as_deref_mut() {
                i.jump = true;
            }
        } else if let Some(b) = phys.as_deref_mut().and_then(|p| p.npcs.get_mut(&who.0)) {
            b.jump = true;
        }
        info!("script moves: {who} jumps");
    }
    if let (Some(on), Some(c)) = (st.auto_move.take(), control.as_deref_mut()) {
        c.auto_move = on;
        info!("script moves: auto move {}", if on { "on" } else { "off" });
    }
    // The flags as the keys: forward and back, left and right, walking, running and
    // sneaking, jumping (SetPlayerMovementFlags' page lists them).
    if let (Some(f), Some(i)) = (st.move_flags.take(), intent.as_deref_mut()) {
        let axis = |plus: u32, minus: u32| f32::from(u8::from(f & plus != 0)) - f32::from(u8::from(f & minus != 0));
        i.forward = axis(1, 2);
        i.right = axis(8, 4);
        if f & (256 | 512) != 0 {
            i.walk = f & 256 != 0 && f & 512 == 0;
        }
        i.sneak = f & 1024 != 0;
        i.jump |= f & 4096 != 0;
    }
    for (who, v) in std::mem::take(&mut st.velocities) {
        let line = set_vel_ex(who, v, phys.as_deref_mut(), knocks.as_deref_mut());
        info!("script moves: {line}");
    }
    let (moves, stat, perk) = speeds;
    if let Some(m) = moves {
        let factor = stat.map_or(1.0, |s| s.0) * perk.map_or(1.0, |p| p.0);
        st.run_speeds.insert(FormId::PLAYER_REF, m.run * factor);
    }
    if let Some(c) = combat {
        if let Some(s) = c.player_spread {
            st.spreads.insert(FormId::PLAYER_REF, s);
        }
        st.grenade_hold = c.grenade_hold;
    }
    st.third_person = control.as_deref().is_some_and(|c| c.pov != crate::control::Pov::First);
}

/// SetVelEx's velocity on `who`: a character goes up or down at z and is pushed along
/// the ground at (x, y) as the console's SetVel pushes it (`crate::console::set_vel`, one
/// push along the two axes together); a loose object's bodies take all three.
fn set_vel_ex(who: FormId, v: [f32; 3], phys: Option<&mut crate::phys::PhysState>, knocks: Option<&mut Messages<crate::combat::Knockback>>) -> String {
    let Some(p) = phys else { return "SetVelEx: no physics in this run".into() };
    let id = who.0;
    let ch = if who == FormId::PLAYER_REF { p.player.as_mut() } else { p.npcs.get_mut(&id).map(|n| &mut n.ch) };
    let Some(ch) = ch else {
        let n: usize = (0..3).map(|axis| p.world.set_ref_velocity(id, axis, v[axis])).max().unwrap_or(0);
        return format!("SetVelEx: {who} at ({:.0}, {:.0}, {:.0}) units/s ({n} bodies)", v[0], v[1], v[2]);
    };
    if v[2] != 0.0 {
        ch.vertical_speed = v[2];
        ch.grounded = false;
    }
    let along = v[0].hypot(v[1]);
    if let (true, Some(k)) = (along > 0.0, knocks) {
        let dir = [v[0] / along, v[1] / along];
        k.write(crate::combat::Knockback { form_id: id, dir, speed: along, secs: crate::console::SET_VEL_SECS, attacker: 0, blow: 0, power: false });
    }
    format!("SetVelEx: {who} {:.0} units/s up, pushed {along:.0} units/s along the ground", v[2])
}

/// The pad's buttons as the extender's controller functions number them
/// (`fnv_game::nvse_jip2::Pad`).
const PAD_BITS: [(GamepadButton, u32); 14] = [
    (GamepadButton::DPadUp, 0x1),
    (GamepadButton::DPadDown, 0x2),
    (GamepadButton::DPadLeft, 0x4),
    (GamepadButton::DPadRight, 0x8),
    (GamepadButton::Start, 0x10),
    (GamepadButton::Select, 0x20),
    (GamepadButton::LeftThumb, 0x40),
    (GamepadButton::RightThumb, 0x80),
    (GamepadButton::LeftTrigger, 0x100),
    (GamepadButton::RightTrigger, 0x200),
    (GamepadButton::South, 0x1000),
    (GamepadButton::East, 0x2000),
    (GamepadButton::West, 0x4000),
    (GamepadButton::North, 0x8000),
];

/// Tell the scripts the player's auto-move and aiming and the first connected pad
/// (GetAutoMove, GetPCUsingScope/IronSights, the controller functions), and whether the
/// sights are up (`Combat::player_sights_up`: not while blocking with a melee weapon or
/// while a reload holds them down), for SUP's HUD bars' aiming condition.
fn tell_player_state(
    control: Option<Res<crate::control::PlayerControl>>,
    combat: Option<Res<crate::combat::Combat>>,
    anim: Option<Res<crate::combat::WeaponAnim>>,
    pads: crate::pad::ActionPads,
    mut script: Option<ResMut<crate::script::ScriptState>>,
) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let told = &mut rt.game.jip2.told;
    if let Some(c) = control {
        told.auto_move = c.auto_move;
        told.aiming = c.aiming && c.weapon_out;
        told.sights_up = match (combat.as_deref(), anim.as_deref()) {
            (Some(cb), Some(a)) => cb.player_sights_up(a, told.aiming),
            _ => false,
        };
    }
    let pad = &mut told.pad;
    match pads.iter().next() {
        Some(g) => {
            pad.connected = true;
            pad.buttons = PAD_BITS.iter().filter(|(b, _)| g.pressed(*b)).map(|(_, m)| m).sum();
            pad.triggers = [g.get(GamepadButton::LeftTrigger2).unwrap_or(0.0), g.get(GamepadButton::RightTrigger2).unwrap_or(0.0)];
            let (l, r) = (g.left_stick(), g.right_stick());
            pad.sticks = [l.x, l.y, r.x, r.y];
        }
        None => {
            pad.connected = false;
            pad.buttons = 0;
            pad.triggers = [0.0; 2];
            pad.sticks = [0.0; 4];
        }
    }
}

/// The player's camera for WorldToScreen (`fnv_game::nvse_jam::ViewTold`): Bevy world to
/// clip space.
fn tell_view(cams: Query<(&Camera, &GlobalTransform), With<crate::PlayerCamera>>, mut script: Option<ResMut<crate::script::ScriptState>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    rt.game.jam.view = cams.iter().find(|(c, _)| c.is_active).map(|(c, t)| fnv_game::nvse_jam::ViewTold {
        clip_from_world: (c.clip_from_view() * t.to_matrix().inverse()).to_cols_array(),
    });
}

/// Once the game's Data is open: mods' files are read from NVForever's mods layer, then
/// the game's Data, and the archives answer FileExists (fnv_game::mod_files).
fn mod_file_roots(data: Option<Res<nvf_assets::GameData>>, mut done: Local<bool>) {
    let Some(d) = data.filter(|_| !*done) else { return };
    *done = true;
    let vfs = d.vfs.clone();
    let reader_vfs = d.vfs.clone();
    // A model's sequences for GetAnimSequenceFrequency.
    let (seq_vfs, schema) = (d.vfs.clone(), d.schema.clone());
    fnv_game::nvse_jam::set_sequence_lookup(std::sync::Arc::new(move |model: &str| {
        let path = format!("meshes\\{}", model.trim_start_matches(['\\', '/']));
        let data = seq_vfs.read(&path)?.ok()?;
        let f = fnv_nif::NifFile::parse(&data, Some(&schema)).ok()?;
        let dec = fnv_nif::scene::Decoded::new(&f, &data, &schema);
        Some(fnv_nif::anim::sequences(&f, &dec).into_iter().filter_map(|s| Some((s.name?, s.frequency))).collect())
    }));
    let mods = fnv_install::EngineHome::default_root().map(|r| fnv_install::EngineHome::new(r).mods());
    let data_dir = vfs.data_dir().to_path_buf();
    let archives: Vec<String> = vfs.archives().map(str::to_string).collect();
    fnv_game::mod_files::update_roots(move |r| {
        r.read = mods.into_iter().chain(std::iter::once(data_dir)).collect();
        r.archives = archives;
        r.reader = Some(std::sync::Arc::new(move |p: &str| reader_vfs.read(p).and_then(Result::ok)));
        r.lookup = Some(std::sync::Arc::new(move |p: &str| {
            if vfs.loose_path(p).is_some() {
                1
            } else if vfs.exists(p) {
                2
            } else {
                0
            }
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A press let go within the frame is held for that frame and gone the next: one
    /// key-down, then one key-up, for JIP's handlers.
    #[test]
    fn a_tap_shorter_than_a_frame_is_held_for_one_frame() {
        let mut keys = ButtonInput::<KeyCode>::default();
        let mut mouse = ButtonInput::<MouseButton>::default();
        let v = key_code(KeyCode::KeyV).unwrap();
        // Pressed and let go before the frame read the devices (as Bevy's keyboard system
        // applies both events in one frame).
        keys.press(KeyCode::KeyV);
        keys.release(KeyCode::KeyV);
        mouse.press(MouseButton::Left);
        mouse.release(MouseButton::Left);
        let held = held_codes(&keys, &mouse, &[(KeyCode::KeyV, true)], &[(MouseButton::Left, true)]);
        assert!(held.contains(&v));
        assert!(held.contains(&MOUSE_BASE));
        // The next frame: up.
        keys.clear();
        mouse.clear();
        assert!(held_codes(&keys, &mouse, &[], &[]).is_empty());
        // A key held across frames stays held, once.
        keys.press(KeyCode::KeyV);
        assert_eq!(held_codes(&keys, &mouse, &[(KeyCode::KeyV, true)], &[]).into_iter().collect::<Vec<_>>(), vec![v]);
        keys.clear();
        assert_eq!(held_codes(&keys, &mouse, &[], &[]).into_iter().collect::<Vec<_>>(), vec![v]);
        // A tap the input switch consumed is no tap for the scripts either (#2200).
        keys.release(KeyCode::KeyV);
        keys.clear();
        assert!(held_codes(&keys, &mouse, &[(KeyCode::KeyV, false)], &[]).is_empty());
    }
}

#[cfg(test)]
#[path = "script_input_wheel_tests.rs"]
mod wheel_tests;

#[cfg(test)]
#[path = "script_input_mcm_page_tests.rs"]
mod mcm_page_tests;
