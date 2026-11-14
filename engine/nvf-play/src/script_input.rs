//! The extender's input functions' side in the engine (task D6;
//! docs/spec-notes/ext-input.md): each frame, right after Bevy reads the devices, the
//! keys and mouse buttons held and the controls' bindings go to the script runtime
//! (`fnv_game::script_input`), the keys scripts disabled go to [`ScriptBlocked`] (which
//! `bindings::ControlInput` leaves out, so the player's controls ignore them), and the
//! keys scripts tapped are pressed for one frame.

use crate::bindings::{dik_key, Binding, Bindings, Control, ControlInput, GAME_CONTROLS, GAME_INI_CONTROLS};
use bevy::prelude::*;
use fnv_sim::FormId;
use fnv_game::script_input::MOUSE_BASE;

pub struct ScriptInputPlugin;

impl Plugin for ScriptInputPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptBlocked>()
            // [design, #2200] Scripts see consumed switching presses as absent too.
            .add_systems(PreUpdate, script_input.after(bevy::input::InputSystems).after(crate::pad::device::automatic))
            .add_systems(PreUpdate, publish_virtual_controls.after(script_input).after(crate::pad::PlatformInputSystems::Remember).before(bevy::ui::UiSystems::Focus))
            // The player's state the scripts read (sights up, for the HUD bars' aiming
            // condition) is handed over after combat has updated it this frame and before
            // the scripts run [design]. `tell_magazine` keeps its own place: it also
            // applies the scripts' pending ammo writes, which must reach combat first.
            .add_systems(Update, tell_player_state.after(crate::PlaySet::Activation).before(crate::PlaySet::Scripts))
            .add_systems(Update, (tell_magazine, mod_file_roots))
            .add_systems(Update, script_moves.after(crate::PlaySet::Input));
    }
}

/// The keys and mouse buttons scripts disabled (DisableKey, DisableControl).
#[derive(Resource, Default)]
pub struct ScriptBlocked {
    pub keys: Vec<KeyCode>,
    pub mouse: Vec<MouseButton>,
}

/// The actual non-key movement view used by IsControlPressed, without inventing
/// physical keys. Both the publisher and App regression use this central admission.
pub fn admitted_virtual_controls(input: &ControlInput) -> std::collections::BTreeSet<u8> {
    GAME_CONTROLS[..GAME_INI_CONTROLS].iter().enumerate().filter_map(|(i,(c,_,_))|
        (matches!(c,Control::Forward|Control::Back|Control::SlideLeft|Control::SlideRight) && input.touch_movement_pressed(*c)).then_some(i as u8)).collect()
}
fn publish_virtual_controls(input: ControlInput, mut script: Option<ResMut<crate::script::ScriptState>>) {
    if let Some(rt)=script.as_deref_mut().and_then(|s|s.rt.as_mut()) {
        rt.game.input.virtual_controls=admitted_virtual_controls(&input);
    }
}

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

/// Let go of a key or mouse button the scripts pressed.
fn release(c: u16, keys: &mut ButtonInput<KeyCode>, mouse: &mut ButtonInput<MouseButton>) {
    match c.checked_sub(MOUSE_BASE) {
        Some(_) => mouse_of(c).into_iter().for_each(|b| mouse.release(b)),
        None => dik_key(c as u8).into_iter().for_each(|k| keys.release(k)),
    }
}

#[allow(clippy::too_many_arguments)]
fn script_input(
    mut script: Option<ResMut<crate::script::ScriptState>>,
    mut bindings: Option<ResMut<Bindings>>,
    mut keys: ResMut<ButtonInput<KeyCode>>,
    mut mouse: ResMut<ButtonInput<MouseButton>>,
    mut blocked: ResMut<ScriptBlocked>,
    switching: Option<Res<crate::pad::SwitchingInput>>,
    mut tapped: Local<Vec<u16>>,
    mut script_held: Local<Vec<u16>>,
) {
    // Last frame's taps end.
    for c in tapped.drain(..) {
        release(c, &mut keys, &mut mouse);
    }
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let input = &mut rt.game.input;
    // Keys scripts held last frame and no longer hold (ReleaseKey, ReleaseControl) are let go.
    for c in script_held.drain(..).filter(|c| !input.script_held.contains(c)) {
        release(c, &mut keys, &mut mouse);
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
    input.held = action_keys.get_pressed().filter_map(|&k| key_code(k)).chain(action_mouse.get_pressed().filter_map(|&b| mouse_code(b))).collect();
    drop((action_keys, action_mouse));
    if let Some(b) = bindings.as_deref() {
        // The engine's controls are the extender's 0–27, in the same order.
        input.bindings = GAME_CONTROLS[..GAME_INI_CONTROLS].iter().map(|(c, _, _)| {
            let bind = b.get(*c);
            (bind.key.and_then(key_code), bind.mouse.and_then(mouse_code))
        }).collect();
    }
    let codes = input.blocked();
    blocked.keys = codes.iter().filter(|&&c| c < MOUSE_BASE).filter_map(|&c| dik_key(c as u8)).collect();
    blocked.mouse = codes.iter().filter_map(|&c| mouse_of(c)).collect();
    for c in std::mem::take(&mut input.taps).into_iter().chain(input.script_held.iter().copied()) {
        match c.checked_sub(MOUSE_BASE) {
            Some(_) => mouse_of(c).into_iter().for_each(|b| mouse.press(b)),
            None => dik_key(c as u8).into_iter().for_each(|k| keys.press(k)),
        }
        if !input.script_held.contains(&c) {
            tapped.push(c);
        }
    }
    script_held.extend(input.script_held.iter().copied());
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
        i.analog_scale = 1.0; // An explicit script movement request owns its native pace.
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

/// Once the game's Data is open: mods' files are read from NVForever's mods layer, then
/// the game's Data, and the archives answer FileExists (fnv_game::mod_files).
fn mod_file_roots(data: Option<Res<nvf_assets::GameData>>, mut done: Local<bool>) {
    let Some(d) = data.filter(|_| !*done) else { return };
    *done = true;
    let vfs = d.vfs.clone();
    let reader_vfs = d.vfs.clone();
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
