//! The player's controls beyond walking and looking, on the game's bindings
//! ([`crate::bindings`]; docs/design/player-body.md §1.2):
//!
//! - Crouch/Sneak (Left Ctrl) toggles sneaking.
//! - Ready Item (R): with the weapon away, draws it; with it out, a tap reloads and a hold
//!   of [`HOLSTER_HOLD`] puts it away [hypothesis: the game's tap/hold split]. Reloading
//!   is `crate::combat`'s.
//! - Use (left mouse) attacks while held ([`PlayerControl::trigger`], `crate::combat`);
//!   with the weapon away it draws it first [hypothesis].
//! - Ammo Swap (hotkey 2) asks `crate::combat` for the next carried ammo type.
//! - Block (right mouse, Left Alt), held: guns aim down the sights, fists and melee
//!   weapons block.
//! - Always Run (Caps Lock) toggles running as the default (on by default:
//!   `bAlwaysRunByDefault=1`); Run (Left Shift) held does the other; Auto Move (Q) walks
//!   forward until Forward or Back is pressed (player.rs).
//! - Toggle POV (F, middle mouse): a tap switches first and third person; held while the
//!   mouse moves, it circles the camera around the player without turning them
//!   ([`Orbit`]; from first person the view goes to third person meanwhile), and the
//!   camera swings back behind when it is let go [hypothesis: the game's hold-POV camera].
//!   The mouse wheel zooms the third-person camera.
//! - The game's other controls (Rest, VATS, the hotkeys, Ammo Swap, Quick Save, Quick
//!   Load, Grab) are bound but not built: pressing one says so on screen.
//!
//! This module holds the requests; what they look like is the player body's job
//! (nvforever `player_body.rs`), which also writes the eye it sees from into
//! [`crate::player::Player::body_eye`]. The camera ([`place_camera`]) sits at that eye in
//! first person, and behind the shoulder in third person, pulled in where collision is in
//! the way.

use crate::activate::HudMessage;
use crate::bindings::{Bindings, Control, ControlInput};
use crate::phys::PhysState;
use crate::player::Player;
use crate::sim::SimState;
use crate::{space, PlayState, PlayerCamera};
use bevy::input::mouse::AccumulatedMouseScroll;
use bevy::prelude::*;
use fnv_phys::Filter;

/// First- or third-person view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Pov {
    #[default]
    First,
    Third,
}

/// The camera circling the player while Toggle POV is held: angles added to the view's
/// heading and pitch for the camera only.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Orbit {
    pub yaw: f32,
    pub pitch: f32,
    /// Toggle POV is down.
    pub held: bool,
    /// The mouse moved while it was down (so letting go does not switch the view).
    pub moved: bool,
}

impl Orbit {
    /// Whether the camera is off its place behind the player (circling, or swinging back).
    pub fn active(&self) -> bool {
        (self.held && self.moved) || self.yaw.abs() + self.pitch.abs() > 1e-3
    }
}

/// What the player asks of their character besides moving.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct PlayerControl {
    /// Crouched (sneaking).
    pub sneak: bool,
    /// Wants the weapon out (fists without one).
    pub weapon_out: bool,
    /// Block held: aiming down the sights, or blocking.
    pub aiming: bool,
    /// Running is the default (Always Run).
    pub always_run: bool,
    /// Walking forward on its own (Auto Move).
    pub auto_move: bool,
    pub pov: Pov,
    /// Third-person camera distance behind the pivot, game units.
    pub distance: f32,
    pub orbit: Orbit,
    /// How long Ready Item has been held with the weapon out (a hold holsters).
    pub ready_held: Option<f32>,
    /// Use held with the weapon out (the trigger; `crate::combat` reads it).
    pub trigger: bool,
    /// A reload was asked for (Ready Item tapped with the weapon out); `crate::combat`
    /// clears it.
    pub reload: bool,
    /// Ammo Swap pressed; `crate::combat` clears it.
    pub ammo_swap: bool,
}

impl Default for PlayerControl {
    fn default() -> Self {
        PlayerControl {
            sneak: false,
            weapon_out: false,
            aiming: false,
            always_run: crate::bindings::ALWAYS_RUN_BY_DEFAULT,
            auto_move: false,
            pov: Pov::First,
            distance: DEFAULT_DISTANCE,
            orbit: Orbit::default(),
            ready_held: None,
            trigger: false,
            reload: false,
            ammo_swap: false,
        }
    }
}

impl PlayerControl {
    /// Whether the camera shows the player from outside (third person, or circling).
    pub fn third_person_view(&self) -> bool {
        self.pov == Pov::Third || self.orbit.active()
    }
}

/// Holding Ready Item this long with the weapon out holsters it, seconds [hypothesis].
pub const HOLSTER_HOLD: f32 = 0.5;
/// The circling camera swings back behind the player with this time constant, seconds
/// [engineering choice].
pub const ORBIT_RETURN: f32 = 0.15;
/// Third-person distance range and default, game units [engineering choice].
pub const MIN_DISTANCE: f32 = 60.0;
pub const MAX_DISTANCE: f32 = 450.0;
pub const DEFAULT_DISTANCE: f32 = 170.0;
/// Third-person camera offset from the pivot, to the right and up (game units):
/// FalloutNV.esm's `fOverShoulderPosX` (40) and `fOverShoulderPosZ` (−10) [observed:
/// GMST values; hypothesis: their meaning, from the names], used when the plugins lack
/// them.
pub const DEFAULT_SHOULDER: [f32; 2] = [40.0, -10.0];
/// How close the third-person camera may come to what is behind it, game units
/// [engineering choice].
const WALL_MARGIN: f32 = 8.0;

/// Speed while sneaking as a fraction of walking or running: `fMoveSneakMult`, 0.57 in
/// FalloutNV.esm (used when the plugins lack it) [observed: the player sneaking forward
/// at a run moved 197.7 units/s = 344 × 0.575, physics.md §6.3; hypothesis: the same
/// factor walking, backwards and sideways].
pub const DEFAULT_SNEAK_MULT: f32 = 0.57;

/// Game settings the controls use.
#[derive(Resource, Debug, Clone, Copy, PartialEq)]
pub struct ControlSettings {
    pub sneak_mult: f32,
    /// Right and up offset of the third-person camera.
    pub shoulder: [f32; 2],
}

impl ControlSettings {
    pub fn from_settings(sneak: Option<f32>, x: Option<f32>, z: Option<f32>) -> ControlSettings {
        let ok = |v: Option<f32>| v.filter(|v| v.is_finite());
        ControlSettings {
            sneak_mult: ok(sneak).filter(|v| *v > 0.0).unwrap_or(DEFAULT_SNEAK_MULT),
            shoulder: [ok(x).unwrap_or(DEFAULT_SHOULDER[0]), ok(z).unwrap_or(DEFAULT_SHOULDER[1])],
        }
    }
}

/// The body that shows the player, written by the body each frame (nvforever), read by
/// the camera.
#[derive(Resource, Debug, Clone, Copy, Default, PartialEq)]
pub struct BodyView {
    /// Third-person pivot (game space): above the body, following crouching.
    pub pivot: Option<[f32; 3]>,
}

pub fn init_settings(mut commands: Commands, sim: Res<SimState>) {
    let f = |n: &str| sim.forms.setting_f32(n);
    let s = ControlSettings::from_settings(f("fMoveSneakMult"), f("fOverShoulderPosX"), f("fOverShoulderPosZ"));
    info!("player controls: the game's bindings; sneak speed × {:.2}, third-person shoulder offset {:?}", s.sneak_mult, s.shoulder);
    commands.insert_resource(s);
}

/// The controls that are bound but not built yet.
const NOT_BUILT: [Control; 12] = [
    Control::Rest,
    Control::Vats,
    Control::Hotkey1,
    Control::Hotkey3,
    Control::Hotkey4,
    Control::Hotkey5,
    Control::Hotkey6,
    Control::Hotkey7,
    Control::Hotkey8,
    Control::QuickSave,
    Control::QuickLoad,
    Control::Grab,
];

/// Toggles, holds, the third-person zoom; see the module notes.
pub fn read_controls(
    input: ControlInput,
    time: Res<Time>,
    scroll: Res<AccumulatedMouseScroll>,
    state: Res<State<PlayState>>,
    mut control: ResMut<PlayerControl>,
    hud: Option<ResMut<HudMessage>>,
) {
    if *state.get() != PlayState::Playing {
        control.aiming = false;
        control.trigger = false;
        control.ready_held = None;
        control.orbit.held = false;
        return;
    }
    let mut message: Option<String> = None;
    let c = &mut *control;
    if input.just_pressed(Control::CrouchSneak) {
        c.sneak = !c.sneak;
    }
    if input.just_pressed(Control::AlwaysRun) {
        c.always_run = !c.always_run;
    }
    if input.just_pressed(Control::AutoMove) {
        c.auto_move = !c.auto_move;
    }
    // Ready Item: draw at once; with the weapon out, tap to reload, hold to holster.
    if input.just_pressed(Control::ReadyItem) {
        if c.weapon_out {
            c.ready_held = Some(0.0);
        } else {
            c.weapon_out = true;
        }
    }
    if let Some(t) = c.ready_held.as_mut() {
        *t += time.delta_secs();
        if *t >= HOLSTER_HOLD {
            c.weapon_out = false;
            c.ready_held = None;
        } else if input.just_released(Control::ReadyItem) || !input.pressed(Control::ReadyItem) {
            c.ready_held = None;
            c.reload = true;
        }
    }
    // Use draws the weapon when it is away; held with it out, it is the trigger. The
    // press that draws does not fire.
    if input.just_pressed(Control::Use) && !c.weapon_out {
        c.weapon_out = true;
        c.trigger = false;
    } else {
        c.trigger = c.weapon_out && input.pressed(Control::Use) && (c.trigger || input.just_pressed(Control::Use));
    }
    if input.just_pressed(Control::AmmoSwap) {
        c.ammo_swap = true;
    }
    for ctl in NOT_BUILT {
        if input.just_pressed(ctl) {
            message = Some(format!("{}: not available yet", Bindings::name(ctl)));
        }
    }
    c.aiming = input.pressed(Control::Block);
    // Toggle POV: a tap switches the view; held with the mouse moving, the camera circles
    // (player::mouse_look turns `orbit` instead of the view).
    if input.just_pressed(Control::TogglePov) {
        c.orbit.held = true;
        c.orbit.moved = false;
    }
    if c.orbit.held && input.just_released(Control::TogglePov) {
        c.orbit.held = false;
        if !c.orbit.moved {
            c.pov = match c.pov {
                Pov::First => Pov::Third,
                Pov::Third => Pov::First,
            };
        }
    }
    if c.third_person_view() && scroll.delta.y != 0.0 {
        c.distance = zoom(c.distance, scroll.delta.y);
    }
    if let Some(m) = message {
        info!("{m}");
        if let Some(mut h) = hud {
            h.show(m);
        }
    }
}

/// Third-person distance after a wheel motion (+ towards the player).
pub fn zoom(distance: f32, wheel: f32) -> f32 {
    (distance * 0.9f32.powf(wheel)).clamp(MIN_DISTANCE, MAX_DISTANCE)
}

/// Where the third-person camera wants to be: from the pivot, offset right and up, then
/// back along the view by `distance` (game space).
pub fn third_person_spot(pivot: [f32; 3], heading: f32, pitch: f32, shoulder: [f32; 2], distance: f32) -> ([f32; 3], [f32; 3]) {
    let (s, c) = heading.sin_cos();
    let right = [c, -s, 0.0];
    let base = [pivot[0] + right[0] * shoulder[0], pivot[1] + right[1] * shoulder[0], pivot[2] + shoulder[1]];
    let d = space::look_dir(heading, pitch);
    (base, [base[0] - d[0] * distance, base[1] - d[1] * distance, base[2] - d[2] * distance])
}

/// The shoulder offset while circling: it fades to the centre as the camera turns away
/// from behind, so the camera keeps looking at the player [engineering choice].
pub fn orbit_shoulder(shoulder: [f32; 2], yaw: f32) -> [f32; 2] {
    let k = 1.0 - (yaw.abs() / 0.5).min(1.0);
    [shoulder[0] * k, shoulder[1]]
}

/// The camera: at the eye in first person; behind the shoulder in third person (turned by
/// the orbit while circling), in front of whatever collision lies between the pivot and
/// that spot.
pub fn place_camera(
    time: Res<Time>,
    player: Option<Res<Player>>,
    mut control: ResMut<PlayerControl>,
    settings: Option<Res<ControlSettings>>,
    body: Res<BodyView>,
    mut phys: ResMut<PhysState>,
    mut cams: Query<&mut Transform, With<PlayerCamera>>,
) {
    if !control.orbit.held {
        // Swing back behind the player.
        let k = (-time.delta_secs() / ORBIT_RETURN).exp();
        let o = &mut control.orbit;
        (o.yaw, o.pitch) = (o.yaw * k, o.pitch * k);
        if o.yaw.abs() + o.pitch.abs() < 1e-3 {
            (o.yaw, o.pitch) = (0.0, 0.0);
        }
    }
    let (Some(p), Ok(mut t)) = (player, cams.single_mut()) else { return };
    let eye = p.eye();
    if !control.third_person_view() {
        t.rotation = space::look_rotation(p.heading, p.pitch);
        t.translation = space::to_bevy(eye);
        return;
    }
    let o = control.orbit;
    let heading = p.heading + o.yaw;
    let pitch = (p.pitch + o.pitch).clamp(-crate::player::MAX_PITCH, crate::player::MAX_PITCH);
    t.rotation = space::look_rotation(heading, pitch);
    let shoulder = orbit_shoulder(settings.map_or(DEFAULT_SHOULDER, |s| s.shoulder), o.yaw);
    let pivot = body.pivot.unwrap_or(eye);
    let (base, want) = third_person_spot(pivot, heading, pitch, shoulder, control.distance);
    // The camera is not pushed in by actors' hit volumes (combat C3).
    let filter = Filter { exclude_layers: fnv_phys::layers::character_ignores().union(fnv_phys::layers::actor_volumes()), ..Default::default() };
    // From the pivot sideways to the shoulder, then back along the view.
    let mut from = pivot;
    let mut to = base;
    let mut out = base;
    for leg in 0..2 {
        let d = [to[0] - from[0], to[1] - from[1], to[2] - from[2]];
        let len = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if len > 1e-3 {
            let reach = match phys.ray(from, d, len + WALL_MARGIN, filter.clone()) {
                Some(h) => (h.distance - WALL_MARGIN).max(0.0).min(len),
                None => len,
            };
            out = [from[0] + d[0] / len * reach, from[1] + d[1] / len * reach, from[2] + d[2] / len * reach];
            if reach < len {
                break;
            }
        }
        if leg == 0 {
            (from, to) = (out, want);
        }
    }
    t.translation = space::to_bevy(out);
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::FRAC_PI_2;

    #[test]
    fn third_person_sits_behind_and_right() {
        // Facing north, level: right is east, behind is south.
        let (base, cam) = third_person_spot([0.0, 0.0, 100.0], 0.0, 0.0, [40.0, -10.0], 170.0);
        assert!((base[0] - 40.0).abs() < 1e-4 && base[1].abs() < 1e-4 && (base[2] - 90.0).abs() < 1e-4);
        assert!((cam[0] - 40.0).abs() < 1e-4 && (cam[1] + 170.0).abs() < 1e-4 && (cam[2] - 90.0).abs() < 1e-4);
        // Facing east and looking down: behind is west and up.
        let (_, cam) = third_person_spot([0.0, 0.0, 0.0], FRAC_PI_2, -0.5, [0.0, 0.0], 100.0);
        assert!(cam[0] < -50.0 && cam[2] > 40.0, "{cam:?}");
        // Circling half way round: in front of the player, centred.
        assert_eq!(orbit_shoulder([40.0, -10.0], std::f32::consts::PI), [0.0, -10.0]);
        assert_eq!(orbit_shoulder([40.0, -10.0], 0.0), [40.0, -10.0]);
    }

    #[test]
    fn zoom_steps_and_clamps() {
        assert!(zoom(170.0, 1.0) < 170.0 && zoom(170.0, -1.0) > 170.0);
        assert_eq!(zoom(170.0, 100.0), MIN_DISTANCE);
        assert_eq!(zoom(170.0, -100.0), MAX_DISTANCE);
    }

    #[test]
    fn settings_fall_back_to_the_masters() {
        let s = ControlSettings::from_settings(None, Some(f32::NAN), Some(-12.0));
        assert_eq!(s, ControlSettings { sneak_mult: DEFAULT_SNEAK_MULT, shoulder: [40.0, -12.0] });
    }

    fn app() -> App {
        let mut app = App::new();
        app.add_plugins(bevy::state::app::StatesPlugin)
            .init_resource::<ButtonInput<KeyCode>>()
            .init_resource::<ButtonInput<MouseButton>>()
            .init_resource::<Bindings>()
            .init_resource::<AccumulatedMouseScroll>()
            .init_resource::<PlayerControl>()
            .init_resource::<Time>()
            .insert_state(PlayState::Playing)
            .add_systems(Update, read_controls);
        app
    }

    /// Press and release `key` over two frames (clearing the frame's edges as the input
    /// plugin would).
    fn tap(app: &mut App, key: KeyCode) {
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().press(key);
        app.update();
        let mut k = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
        k.clear();
        k.release(key);
        app.update();
        app.world_mut().resource_mut::<ButtonInput<KeyCode>>().clear();
    }

    fn control(app: &App) -> PlayerControl {
        *app.world().resource::<PlayerControl>()
    }

    #[test]
    fn game_keys_toggle() {
        let mut app = app();
        tap(&mut app, KeyCode::ControlLeft);
        assert!(control(&app).sneak);
        tap(&mut app, KeyCode::CapsLock);
        assert!(!control(&app).always_run);
        tap(&mut app, KeyCode::KeyQ);
        assert!(control(&app).auto_move);
        // Ready Item draws; a tap with the weapon out does not put it away.
        tap(&mut app, KeyCode::KeyR);
        assert!(control(&app).weapon_out);
        tap(&mut app, KeyCode::KeyR);
        assert!(control(&app).weapon_out && control(&app).ready_held.is_none());
        // Toggle POV tapped without moving the mouse switches the view.
        tap(&mut app, KeyCode::KeyF);
        assert_eq!(control(&app).pov, Pov::Third);
    }

    #[test]
    fn orbit_does_not_switch_the_view() {
        let mut app = app();
        app.world_mut().resource_mut::<ButtonInput<MouseButton>>().press(MouseButton::Middle);
        app.update();
        assert!(control(&app).orbit.held);
        app.world_mut().resource_mut::<PlayerControl>().orbit.moved = true; // mouse_look's part
        let mut m = app.world_mut().resource_mut::<ButtonInput<MouseButton>>();
        m.clear();
        m.release(MouseButton::Middle);
        app.update();
        let c = control(&app);
        assert!(!c.orbit.held && c.pov == Pov::First);
    }
}
