//! Sprint: the shared state of NVF Features › Advanced Movement's Sprint switch [design, #3000]. Off by default (`crate::inertia::MovementInertia::sprint`) and acting
//! only under the Global Toggle ([`MovementInertia::sprint_on`]). This module holds the
//! plumbing and the contract; the feature itself (speed, AP, out of breath) is
//! `crate::sprint`, its look nvforever's `sprint_view.rs`.
//!
//! # The contract
//!
//! **Switch**: read [`MovementInertia::sprint_on`] (`advanced && sprint`), never the bare
//! field, so the Global Toggle turns it off. Command line `--sprint` / `--no-sprint`; row
//! `sprint` (registered in `crate::player`).
//!
//! **Input** (`crate::control::read_controls`, `PlaySet::Input`; struck by the scripts'
//! locks in `crate::control::enforce_locks`, the frame's input already applied):
//! [`PlayerControl::sprint_held`]: the Sprint control is held and Sprint is on. Struck by the
//! movement lock.
//!
//! **Keys** (`crate::bindings`): Sprint is a binding of its own (Controls › Movement) with no
//! key or button until the player binds one, greyed and not rebindable while its switch is
//! off ([`binding_greyed`]). Run keeps its own key (Left Shift) whatever Sprint's state
//! [design, #3000: the maintainer's decision on #3002]. [`log_keys`] says both keys whenever
//! the switch changes.
//!
//! **Motion** ([`StanceMotion`], one resource the camera and animation code read): the
//! Sprint feature writes [`StanceMotion::sprinting`], [`StanceMotion::sprint_share`] and
//! [`StanceMotion::winded`]; nothing else writes them; everything may read them. With the
//! switch off the feature leaves its fields at rest (false, 0) after easing out.
//!
//! **Sprint and the perks**: the speed perks and stats already scale every gait
//! (`crate::player::PerkSpeed`, Modify Run Speed perks; `crate::player::StatSpeed`, Speed
//! Mult and armour; `crate::player::RunPace`): the Sprint feature multiplies the run speed
//! they give, so they apply to it, and spends AP through the same paths the game's AP costs
//! use (`crate::combat`, the AP actor value), so the AP perks apply.
//!
//! [`MovementInertia::sprint_on`]: crate::inertia::MovementInertia::sprint_on
//! [`PlayerControl::sprint_held`]: crate::control::PlayerControl::sprint_held

use crate::bindings::{Bindings, Control};
use crate::inertia::MovementInertia;
use bevy::prelude::*;

/// What the Sprint feature is doing now, for the camera and the animation code (module notes:
/// who writes which field).
#[derive(Resource, Debug, Clone, Copy, PartialEq, Default)]
pub struct StanceMotion {
    /// The share of a sprint now, 0 (none) to 1 (a full sprint), eased in and out by the
    /// Sprint feature: what the camera, the arms and the body blend by.
    pub sprint_share: f32,
    /// Sprinting is the target state (the key held, moving forward, AP left).
    pub sprinting: bool,
    /// Out of breath after a sprint, 0 (not) to 1 (fully winded): the strength of the view's
    /// and the gun's breathing shake.
    pub winded: f32,
}

/// Why control `c`'s Controls row is greyed now (Sprint with its switch off, or the Global
/// Toggle off), in the words the page shows under the row; `None`: it is not. A greyed
/// binding cannot be rebound or cleared (`crate::bindings::rebind`). Without
/// [`MovementInertia`] nothing is greyed.
pub fn binding_greyed(world: &World, c: Control) -> Option<&'static str> {
    let m = world.get_resource::<MovementInertia>()?;
    let (own, why) = match c {
        Control::Sprint => (m.sprint, SPRINT_OFF),
        _ => return None,
    };
    if !own {
        Some(why)
    } else if !m.advanced {
        Some(GLOBAL_TOGGLE_OFF)
    } else {
        None
    }
}

/// What a greyed Sprint binding's note says ([`binding_greyed`]).
pub const SPRINT_OFF: &str = "Turn on Sprint in NVF Features > Advanced Movement first.";
pub const GLOBAL_TOGGLE_OFF: &str = "Turn on the Global Toggle in NVF Features > Advanced Movement first.";

/// Log Sprint's switch with Sprint's key and Run's at the start and whenever the switch
/// changes (a smoke check: Run keeps its key, Sprint has none until bound).
pub fn log_keys(inertia: Option<Res<MovementInertia>>, bindings: Option<Res<Bindings>>, mut last: Local<Option<bool>>) {
    let (Some(m), Some(b)) = (inertia, bindings) else { return };
    let now = m.sprint_on();
    if *last != Some(now) {
        *last = Some(now);
        info!("controls: Sprint {}; Sprint's key {}, Run's {}", if now { "on" } else { "off" }, b.get(Control::Sprint).text(), b.get(Control::Run).text());
    }
}

/// [`StanceMotion`] and the keys' log ([`log_keys`]).
pub struct StancePlugin;

impl Plugin for StancePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<StanceMotion>().add_systems(PreUpdate, log_keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sprint_has_no_key_run_keeps_shift_and_a_greyed_row_says_why() {
        let mut app = App::new();
        app.init_resource::<Bindings>().init_resource::<MovementInertia>().add_plugins(StancePlugin);
        app.update();
        assert_eq!(*app.world().resource::<StanceMotion>(), StanceMotion::default());
        assert_eq!(binding_greyed(app.world(), Control::Sprint), Some(SPRINT_OFF));
        assert_eq!(binding_greyed(app.world(), Control::Run), None);
        let keys = |app: &App| {
            let b = app.world().resource::<Bindings>();
            (b.get(Control::Sprint).key, b.get(Control::Run).key)
        };
        assert_eq!(keys(&app), (None, Some(KeyCode::ShiftLeft)), "Sprint unbound by default");
        // Sprint on: Run keeps Left Shift; Sprint still has none until the player binds it.
        app.world_mut().resource_mut::<MovementInertia>().sprint = true;
        app.update();
        assert_eq!(keys(&app), (None, Some(KeyCode::ShiftLeft)), "Run does not move");
        assert_eq!(binding_greyed(app.world(), Control::Sprint), None);
        // The Global Toggle off: greyed for it, and Sprint keeps its choice.
        app.world_mut().resource_mut::<MovementInertia>().advanced = false;
        app.update();
        assert_eq!(binding_greyed(app.world(), Control::Sprint), Some(GLOBAL_TOGGLE_OFF));
        // A greyed binding is not rebound or cleared.
        app.world_mut().resource_mut::<MovementInertia>().advanced = true;
        app.world_mut().resource_mut::<MovementInertia>().sprint = false;
        crate::bindings::rebind(app.world_mut(), Control::Sprint, crate::bindings::Input::Key(KeyCode::KeyK));
        assert_eq!(keys(&app).0, None);
        app.world_mut().resource_mut::<MovementInertia>().sprint = true;
        crate::bindings::rebind(app.world_mut(), Control::Sprint, crate::bindings::Input::Key(KeyCode::KeyK));
        assert_eq!(keys(&app), (Some(KeyCode::KeyK), Some(KeyCode::ShiftLeft)), "Sprint is on: it rebinds, Run untouched");
        crate::bindings::clear_key(app.world_mut(), Control::Sprint);
        assert_eq!(keys(&app).0, None, "and clears");
    }
}
