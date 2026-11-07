//! The campfires of Burning Campfire Redone (`fnv_game::campfire`) get two actions under
//! the crosshair [design, owner's queue item 54, 2026-11-06]: Activate is "Craft" (the
//! plugin's script: the fire and the recipe menu), and Ready Item is "Light fire" (only
//! the fire, [`fnv_game::game::Game::light_campfire`]). The second action is drawn after
//! the first on the prompt's action line, in the same style, with the key Ready Item is
//! bound to (`crate::reticle`). The second key is Ready Item because JAM's loot menu puts
//! its second action ("R) Open") on R, the key Ready Item has by default.
//!
//! A press of Ready Item on such a campfire lights it and does not draw or reload the
//! weapon ([`SecondAction::took`], read by `crate::control::read_controls`). Without the
//! plugin, campfires keep their single action.
//!
//! Log: `campfire 0x… <name>: lit, N fires placed` or `…: burning already, N fires start
//! again`.

use crate::activate::Target;
use crate::bindings::{Bindings, Control, ControlInput};
use crate::script::{with_world, ScriptState};
use crate::sim::SimState;
use crate::PlayState;
use bevy::prelude::*;
use fnv_game::campfire::Lit;
use fnv_sim::FormId;
use std::collections::HashSet;

/// The word for Activate on a campfire.
pub const CRAFT: &str = "Craft";
/// The word for the second action.
pub const LIGHT: &str = "Light fire";
/// The control of the second action.
pub const SECOND: Control = Control::ReadyItem;

/// The campfire bases, found once the scripts run; empty without the plugin.
#[derive(Resource, Default, Debug)]
pub struct Campfires {
    known: bool,
    pub bases: HashSet<FormId>,
}

impl Campfires {
    /// Whether reference `form_id` is one of the plugin's campfires.
    pub fn is_campfire(&self, sim: &SimState, form_id: u32) -> bool {
        !self.bases.is_empty() && sim.base_of(form_id).is_some_and(|b| self.bases.contains(&b))
    }
}

/// This frame's press of the second key went to a campfire: the weapon stays as it is.
#[derive(Resource, Default, Debug, Clone, Copy)]
pub struct SecondAction {
    pub took: bool,
}

/// What the prompt over reference `form_id` adds: the second action's key and word.
pub fn second_action(campfires: Option<&Campfires>, sim: &SimState, form_id: u32, bindings: Option<&Bindings>) -> Option<(String, String)> {
    campfires.filter(|c| c.is_campfire(sim, form_id))?;
    Some((bindings.map(|b| b.prompt(SECOND)).unwrap_or_else(|| "R)".into()), LIGHT.into()))
}

fn learn(mut campfires: ResMut<Campfires>, script: Option<Res<ScriptState>>) {
    if campfires.known {
        return;
    }
    let Some(rt) = script.as_ref().and_then(|s| s.rt.as_ref()) else { return };
    campfires.known = true;
    campfires.bases = rt.game.campfire_bases().into_iter().collect();
    if !campfires.bases.is_empty() {
        info!("campfire: Burning Campfire Redone loaded: {} campfire bases take \"{CRAFT}\" and \"{LIGHT}\"", campfires.bases.len());
    }
}

/// The second key on a campfire under the crosshair lights it.
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
    if *state.get() != PlayState::Playing || campfires.bases.is_empty() || !input.just_pressed(SECOND) {
        return;
    }
    let Some(t) = target.as_ref().and_then(|t| t.0.as_ref()).filter(|t| campfires.is_campfire(&sim, t.form_id)) else { return };
    took.took = true;
    // As Activate: nothing while a script has the player's movement off.
    if locks.is_some_and(|l| l.movement) {
        info!("campfire 0x{:08X} {}: not lit, a script has disabled the player's controls (movement)", t.form_id, t.name);
        return;
    }
    let Some(mut script) = script else { return };
    let Some(rt) = script.rt.as_mut() else { return };
    let (id, name) = (t.form_id, t.name.clone());
    match with_world(rt, &mut sim, |rt| rt.game.light_campfire(FormId(id))) {
        Ok(Lit::Placed(fires)) => info!("campfire 0x{id:08X} {name}: lit, {} fires placed", fires.len()),
        Ok(Lit::Restarted(fires)) => info!("campfire 0x{id:08X} {name}: burning already, {} fires start again", fires.len()),
        Err(e) => warn!("campfire 0x{id:08X} {name}: not lit: {e}"),
    }
}

pub struct CampfirePlugin;

impl Plugin for CampfirePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<Campfires>()
            .init_resource::<SecondAction>()
            .add_systems(Update, (learn, light).chain().before(crate::control::read_controls));
    }
}
