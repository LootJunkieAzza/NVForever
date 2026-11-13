//! "Map marker added": a script's ShowMap puts a place on the player's map, and a corner
//! message says so.
//!
//! - **When** [hypothesis: the original shows such a pop-up, as Stewie's Tweaks' readme
//!   speaks of "the 'Marker marker added.' pop-up" (bDescriptiveMarkerAddedMessage); its
//!   words are not in the data]: a marker joins the scripts' list of shown markers
//!   (`fnv_game::game::PlayerState::map_markers`, ShowMap), in play or a conversation (a
//!   result script's ShowMap); not behind a loading screen (a loaded save's).
//! - **The words** [design: our own]: "Map marker added"; under the player's Descriptive
//!   Marker Added Message tweak, with the marker's name ([`line`]).
//!
//! Log: `marker added: …`.

use crate::script::ScriptState;
use crate::sim::SimState;
use crate::{PlaySet, PlayState, PlayWorld};
use bevy::prelude::*;
use fnv_sim::FormId;
use std::collections::{BTreeSet, HashMap};

/// How long the message shows, seconds [design].
const SECONDS: f32 = 3.0;

/// The message for a marker named `name`, its name included when `named`.
pub fn line(name: Option<&str>, named: bool) -> String {
    match name.filter(|n| named && !n.is_empty()) {
        Some(n) => format!("Map marker added: {n}"),
        None => "Map marker added".to_string(),
    }
}

pub struct MarkerAddedPlugin;

impl Plugin for MarkerAddedPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, watch.in_set(PlaySet::Ui));
    }
}

#[allow(clippy::too_many_arguments)]
fn watch(
    state: Res<State<PlayState>>,
    script: Option<ResMut<ScriptState>>,
    sim: Res<SimState>,
    play: Option<Res<PlayWorld>>,
    tweaks: Option<Res<crate::tweaks::TweakSettings>>,
    mut seen: Local<Option<BTreeSet<FormId>>>,
    mut names: Local<Option<HashMap<u32, String>>>,
) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let now = &rt.game.player.map_markers;
    let Some(before) = seen.as_ref() else {
        *seen = Some(now.clone());
        return;
    };
    if before == now {
        return;
    }
    let added: Vec<FormId> = now.difference(before).copied().collect();
    *seen = Some(now.clone());
    // A load or a new game replaces the list: nothing to say for it.
    if *state.get() == PlayState::Loading || added.is_empty() {
        return;
    }
    let named = tweaks.as_ref().is_some_and(|t| t.descriptive_marker_added);
    if named && names.is_none() {
        *names = play.as_deref().map(|p| crate::pipboy::data::read::markers(p.0.lo, p.0.schema, sim.refs).into_iter().map(|m| (m.form, m.name)).collect());
    }
    for m in added {
        let name = names.as_ref().and_then(|n| n.get(&m.0)).map(String::as_str);
        let text = line(name, named);
        info!("marker added: {m} ({text})");
        rt.game.emit(fnv_game::trace::EventKind::NoteShown { text, icon: None, seconds: SECONDS });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_line_names_the_place_under_the_tweak() {
        assert_eq!(line(Some("Goodsprings"), true), "Map marker added: Goodsprings");
        assert_eq!(line(Some("Goodsprings"), false), "Map marker added");
        assert_eq!(line(None, true), "Map marker added");
    }
}
