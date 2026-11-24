//! The idle vanity camera: when the player has touched nothing for a while, the camera
//! goes out to third person and slowly circles them; any input ends it.
//!
//! - **When** [observed: FalloutNV.esm `fVanityModeAutoDelay` 120; hypothesis: its meaning,
//!   seconds without input before the vanity camera starts, from its name]: after the
//!   setting's seconds with no key, mouse button, mouse motion or controller input, while
//!   the game is played (no menu), the HUD is up and the free camera is off. 0 or less
//!   turns it off [design]. A run-time change (the console's `setgs fVanityModeAutoDelay`)
//!   counts from the next frame ([`delay_of`]) [hypothesis: as the original reads its game
//!   settings]. The camera shows this player's own body from outside, with no name tag over
//!   it: in co-op that is the one body more in this player's view (the 2026-11-20 split
//!   relay frames: every untagged body was its own player's, the frames taken after
//!   `vanity camera: on`) [observed].
//! - **What** [hypothesis: the original's circling camera is not captured; observer check
//!   109]: the same circling camera as Toggle POV held (`crate::control::Orbit`), turning
//!   [`TURN_RATE`] radians a second around the player, from first person the view going to
//!   third meanwhile.
//! - **The end**: the first input of any kind (it still does what it does), a menu, or the
//!   HUD going. The camera swings back behind the player and the view stays third person,
//!   as after Toggle POV held [design: the owner's rule for that camera, 2026-10-19]. The
//!   player's Vanity Camera Returns to First Person tweak puts a view that was first person
//!   back to first person, after this camera and after Toggle POV held
//!   ([`end_view`]).
//!
//! Log: `vanity camera: on after N s idle` and `vanity camera: off (why)`.

use crate::control::{PlayerControl, Pov};
use crate::sim::SimState;
use crate::{PlayState, PlaySet};
use bevy::input::mouse::AccumulatedMouseMotion;
use bevy::prelude::*;

/// How fast the idle camera circles, radians a second [design: a turn in about 42 s].
pub const TURN_RATE: f32 = 0.15;
/// Seconds without input when the master lacks `fVanityModeAutoDelay` [observed: its
/// value in FalloutNV.esm].
pub const DEFAULT_DELAY: f32 = 120.0;
/// A stick pushed this far counts as input [design].
const STICK_DEAD: f32 = 0.2;

/// The idle camera's state.
#[derive(Resource, Debug, Default, Clone, Copy, PartialEq)]
pub struct Vanity {
    /// Seconds without input.
    pub idle: f32,
    /// The camera circles now, and whether the view was first person when it began.
    pub on: Option<bool>,
}

impl Vanity {
    /// One frame of `dt` seconds: whether there was input, and whether the camera may
    /// circle now at all (`allowed`). What changed: `Some(true)` the camera starts,
    /// `Some(false)` it ends.
    pub fn step(&mut self, dt: f32, input: bool, allowed: bool, delay: f32) -> Option<bool> {
        if input || !allowed || delay <= 0.0 {
            self.idle = 0.0;
            return self.on.take().map(|_| false);
        }
        self.idle += dt;
        (self.on.is_none() && self.idle >= delay).then_some(true)
    }
}

/// Seconds without input before the camera circles: the game setting as it is now (a
/// console change, else the master's), else [`DEFAULT_DELAY`].
pub fn delay_of(world: &fnv_sim::World) -> f32 {
    world.setting_f32("fVanityModeAutoDelay").unwrap_or(DEFAULT_DELAY)
}

/// The view after a circling camera ends: first person again when it was so and the
/// player's tweak asks for it, else third person.
pub fn end_view(was_first: bool, return_first: bool) -> Pov {
    if was_first && return_first {
        Pov::First
    } else {
        Pov::Third
    }
}

pub struct VanityPlugin;

impl Plugin for VanityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Vanity>().add_systems(Update, idle.in_set(PlaySet::Input).before(crate::control::read_controls));
    }
}

#[allow(clippy::too_many_arguments)]
fn idle(
    time: Res<Time>,
    state: Res<State<PlayState>>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse: Res<ButtonInput<MouseButton>>,
    motion: Res<AccumulatedMouseMotion>,
    pads: Query<&Gamepad>,
    view: Option<Res<crate::hud::HudView>>,
    free: Option<Res<crate::free_cam::FreeCam>>,
    sim: Option<Res<SimState>>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    mut control: ResMut<PlayerControl>,
    mut vanity: ResMut<Vanity>,
) {
    let pad = pads.iter().any(|p| p.get_pressed().next().is_some() || p.left_stick().length() > STICK_DEAD || p.right_stick().length() > STICK_DEAD);
    let input = keys.get_pressed().next().is_some() || mouse.get_pressed().next().is_some() || motion.delta != Vec2::ZERO || pad;
    let c = &*control;
    let allowed = *state.get() == PlayState::Playing
        && view.is_some_and(|v| v.shown)
        && !free.is_some_and(|f| f.on)
        && c.portrait.is_none()
        && !c.cinematic
        && !c.auto_move
        && !c.orbit.free
        && (vanity.on.is_some() || !c.orbit.held);
    let delay = sim.map_or(DEFAULT_DELAY, |s| delay_of(&s.world));
    let was_first = vanity.on;
    match vanity.step(time.delta_secs(), input, allowed, delay) {
        Some(true) => {
            vanity.on = Some(control.pov == Pov::First);
            control.orbit.held = true;
            control.orbit.moved = true;
            info!("vanity camera: on after {delay:.0} s idle");
        }
        Some(false) => {
            control.orbit = control.orbit.released();
            let back = tweaks.is_some_and(|t| t.vanity_returns_first);
            control.pov = end_view(was_first.unwrap_or(false), back);
            info!("vanity camera: off ({}), the view {:?}", if input { "input" } else { "not allowed now" }, control.pov);
        }
        None => {}
    }
    if vanity.on.is_some() {
        let o = &mut control.orbit;
        o.yaw = (o.yaw + TURN_RATE * time.delta_secs() + std::f32::consts::PI).rem_euclid(std::f32::consts::TAU) - std::f32::consts::PI;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_starts_after_the_delay_and_any_input_ends_it() {
        let mut v = Vanity::default();
        assert_eq!(v.step(60.0, false, true, 120.0), None);
        assert_eq!(v.step(59.0, false, true, 120.0), None);
        assert_eq!(v.step(1.0, false, true, 120.0), Some(true));
        v.on = Some(true);
        assert_eq!(v.step(1.0, false, true, 120.0), None);
        assert_eq!(v.step(0.1, true, true, 120.0), Some(false));
        assert_eq!((v.on, v.idle), (None, 0.0));
        // Input before the delay starts the count again; a menu does too.
        assert_eq!(v.step(100.0, false, true, 120.0), None);
        assert_eq!(v.step(0.1, false, false, 120.0), None);
        assert_eq!(v.step(100.0, false, true, 120.0), None);
        // 0 turns it off.
        assert_eq!(v.step(1000.0, false, true, 0.0), None);
    }

    /// `setgs fVanityModeAutoDelay 0` turns it off for the run (the co-op test runs, where
    /// nobody touches a player for minutes), as a change of the master's 120 does.
    #[test]
    fn a_console_change_of_the_delay_counts() {
        use fnv_sim::settings::Setting;
        let mut forms = fnv_sim::forms::FormStore::default();
        forms.settings.insert("fvanitymodeautodelay".into(), Setting::Float(120.0));
        let forms: &'static fnv_sim::forms::FormStore = Box::leak(Box::new(forms));
        let refs: &'static fnv_sim::refs::RefTable = Box::leak(Box::default());
        let mut world = fnv_sim::World::new(forms, refs, fnv_sim::SimConfig::default());
        assert_eq!(delay_of(&world), 120.0);
        world.change_setting("fVanityModeAutoDelay", Setting::Float(0.0)).unwrap();
        assert_eq!(delay_of(&world), 0.0);
        let mut v = Vanity::default();
        assert_eq!(v.step(1000.0, false, true, delay_of(&world)), None, "off");
    }

    #[test]
    fn the_view_after_it_by_the_tweak() {
        assert_eq!(end_view(true, false), Pov::Third);
        assert_eq!(end_view(true, true), Pov::First);
        assert_eq!(end_view(false, true), Pov::Third);
    }
}
