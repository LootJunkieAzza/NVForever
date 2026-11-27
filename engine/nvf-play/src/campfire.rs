//! Lighting the base game's cold campfires (docs/design/campfires.md, `fnv_game::campfire`)
//! [design: NVForever's own, not in the original].
//!
//! With the Gameplay switch `light-campfires` on (off by default, kept in the INI), a
//! campfire under the crosshair with no fire at it ([`fnv_game::game::Game::is_campfire`],
//! [`fnv_game::game::Game::campfire_burns`]) gets a second action: the prompt reads
//! "E) Craft   R) Light fire" (both through `tr`, `lang/`), the keys as Activate and Ready Item are bound
//! (`crate::reticle`, [`prompt_actions`]). Activate still runs the campfire's own script
//! (the recipe menu). Ready Item lights it ([`fnv_game::game::Game::light_campfire`]: a
//! fire and its light placed at it) and does not draw, reload or holster the weapon that
//! frame ([`SecondAction::took`], read by `crate::control::read_controls`).
//!
//! With the switch off, over a campfire that burns (a fire placed there, or one this
//! lit), or over anything else, the prompt and Ready Item are as without this module.
//!
//! Log: `campfire: 0x… lit (fire 0x…, light 0x…)`, or `campfire: 0x… not lit: …`.

use crate::activate::Target;
use crate::bindings::{Bindings, Control, ControlInput};
use crate::script::{with_world, ScriptState};
use crate::settings::{add_setting, Setting};
use crate::sim::SimState;
use crate::PlayState;
use bevy::prelude::*;
use fnv_util::words::tr;
use fnv_sim::FormId;

/// The control of the second action.
pub const SECOND: Control = Control::ReadyItem;

/// The switch, and the cold campfire under the crosshair.
#[derive(Resource, Default, Debug)]
pub struct Campfires {
    /// The settings row `light-campfires`.
    pub on: bool,
    /// The reference under the crosshair when it is a cold campfire and the switch is on
    /// ([`watch`]).
    pub cold: Option<u32>,
}

/// This frame's press of Ready Item went to a cold campfire: the weapon stays as it is.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SecondAction {
    pub took: bool,
}

/// The prompt's actions over a reference: Activate's word and the second action's key and
/// word, when it is a cold campfire (`cold`); `None` otherwise (the prompt as it is).
pub fn actions(cold: bool, key: impl FnOnce() -> String) -> Option<(&'static str, (String, String))> {
    cold.then(|| (tr("Craft"), (key(), tr("Light fire").to_string())))
}

/// What the prompt over reference `form_id` reads, when it is the cold campfire under the
/// crosshair ([`actions`]).
pub fn prompt_actions(campfires: Option<&Campfires>, form_id: u32, bindings: Option<&Bindings>) -> Option<(&'static str, (String, String))> {
    let c = campfires?;
    actions(c.on && c.cold == Some(form_id), || bindings.map(|b| b.prompt(SECOND)).unwrap_or_else(|| "R)".into()))
}

/// The switch into the running game ([`fnv_game::game::Game::light_campfires`]), each
/// frame, so a game loaded or started anew takes it too.
fn sync(campfires: Res<Campfires>, script: Option<ResMut<ScriptState>>) {
    let Some(mut script) = script else { return };
    if script.rt.as_ref().is_none_or(|rt| rt.game.light_campfires == campfires.on) {
        return;
    }
    if let Some(rt) = script.rt.as_mut() {
        rt.game.light_campfires = campfires.on;
        info!("campfire: lighting cold campfires {}", if campfires.on { "on" } else { "off" });
    }
}

/// Whether the reference under the crosshair is a cold campfire, kept in
/// [`Campfires::cold`] for the prompt and Ready Item. The fires are references of the
/// simulation's world, which the game only holds inside [`with_world`].
fn watch(mut campfires: ResMut<Campfires>, state: Res<State<PlayState>>, target: Option<Res<Target>>, mut sim: ResMut<SimState>, script: Option<ResMut<ScriptState>>) {
    let at = target.as_ref().and_then(|t| t.0.as_ref()).map(|t| t.form_id).filter(|_| campfires.on && *state.get() == PlayState::Playing);
    let cold = match (at, script) {
        (Some(id), Some(mut script)) => {
            let base = sim.base_of(id);
            script.rt.as_mut().filter(|rt| base.is_some_and(|b| rt.game.is_campfire_base(b))).is_some_and(|rt| with_world(rt, &mut sim, |rt| !rt.game.campfire_burns(FormId(id)))).then_some(id)
        }
        _ => None,
    };
    if campfires.cold != cold {
        campfires.cold = cold;
    }
}

/// Ready Item on the cold campfire under the crosshair lights it.
#[allow(clippy::too_many_arguments)]
fn light(
    input: ControlInput,
    state: Res<State<PlayState>>,
    campfires: Res<Campfires>,
    target: Option<Res<Target>>,
    mut took: ResMut<SecondAction>,
    mut sim: ResMut<SimState>,
    script: Option<ResMut<ScriptState>>,
    locks: Option<Res<crate::control::ControlLocks>>,
) {
    took.took = false;
    if *state.get() != PlayState::Playing || campfires.cold.is_none() || !input.just_pressed(SECOND) || crate::quick_loot::holds_input() {
        return;
    }
    let Some(id) = target.as_ref().and_then(|t| t.0.as_ref()).map(|t| t.form_id).filter(|&id| campfires.cold == Some(id)) else { return };
    took.took = true;
    // As Activate: nothing while a script has the player's movement off.
    if locks.is_some_and(|l| l.movement) {
        info!("campfire: 0x{id:08X} not lit: a script has disabled the player's controls (movement)");
        return;
    }
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    match with_world(rt, &mut sim, |rt| rt.game.light_campfire(FormId(id))) {
        Some(placed) => match &placed[..] {
            [fire, lamp] => info!("campfire: 0x{id:08X} lit (fire 0x{:08X}, light 0x{:08X})", fire.0, lamp.0),
            _ => warn!("campfire: 0x{id:08X} lit only in part ({} of 2 placed)", placed.len()),
        },
        None => warn!("campfire: 0x{id:08X} not lit: no fire or light form in the load order, or it burns already"),
    }
}

pub struct CampfirePlugin;

impl Plugin for CampfirePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Campfires>()
            .init_resource::<SecondAction>()
            .add_systems(Update, (sync, watch, light).chain().before(crate::control::read_controls));
        add_setting(
            app,
            Setting {
                key: "light-campfires",
                group: crate::pause::GROUP_MENUS,
                label: "Light campfires",
                note: "On: at a campfire with no fire, Ready Item lights it; Activate still opens crafting. Off: campfires are the original's. Not in the original.",
                switches: "",
                get: |w| w.resource::<Campfires>().on,
                set: |w, on| w.resource_mut::<Campfires>().on = on,
                slider: None,
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_cold_campfire_with_the_switch_on_takes_a_second_action() {
        let key = || "R)".to_string();
        assert_eq!(actions(true, key), Some(("Craft", ("R)".to_string(), "Light fire".to_string()))));
        assert_eq!(actions(false, key), None, "not a cold campfire: its own prompt");
        let on = Campfires { on: true, cold: Some(0x0000_4242) };
        assert_eq!(prompt_actions(Some(&on), 0x0000_4242, None).map(|(a, (k, w))| (a, k, w)), Some(("Craft", "R)".to_string(), "Light fire".to_string())));
        assert_eq!(prompt_actions(Some(&on), 0x0000_1234, None), None, "another reference");
        let off = Campfires { on: false, cold: Some(0x0000_4242) };
        assert_eq!(prompt_actions(Some(&off), 0x0000_4242, None), None, "switch off: the prompt as on main");
        assert_eq!(prompt_actions(None, 0x0000_4242, None), None);
    }
}
