//! Which animation group each placed object (not a door) plays and how far into it it
//! is: the one record the renderer and the physics both follow, so an animated
//! activator's drawn model and its collision cannot part (issue #1396; activators.md §5).
//! Doors keep their own clock (`crate::doors`).
//!
//! - **Written by the renderer** (nvforever `object_anims.rs`): a group a script starts
//!   (PlayGroup, PlayAnimSequence) is recorded with [`ObjectGroups::play`]; every frame,
//!   after the drawn objects' clips advanced, the sequence and time each drawn rig shows
//!   (a script's group, its own idle, a radio's Open) are copied in with
//!   [`ObjectGroups::sync_drawn`]. A request for an object not drawn is recorded too,
//!   from its start.
//! - **Read by the physics** (`crate::phys`, `pose_objects`): a reference whose model has
//!   a sequence moving a node that carries a keyframed collision object has its bodies
//!   posed from the group recorded here, held at its end once a group played once has
//!   ended; with nothing recorded, in the file's stored pose.
//! - **Objects not drawn** (out of the drawn area, or a run without a renderer): a
//!   script's group goes on by itself here ([`ObjectGroups::advance`]), so a gate opened
//!   out of sight is open when the player comes back [design]. An object's own idle is
//!   forgotten when it is no longer drawn (its collision stands in the stored pose).
//! - **Kept**: a script's group stays after it ended (the gate left open) and when the
//!   object's cell unloads, and is saved (save hook `object_groups`); `Reset3DState`
//!   forgets it. When the object is drawn again (or after a load), the renderer shows the
//!   recorded group where it is (nvforever `object_anims.rs`).

use bevy::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// The sequence a reference shows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupClock {
    /// The sequence's name as the model spells it (`Forward`, `Idle`).
    pub name: String,
    /// The animation group code a script asked for (`fnv_game::anims::AnimRequest::group`);
    /// `None` for what the object plays by itself.
    pub group: Option<u16>,
    /// Seconds since the sequence started; the reader wraps or holds it by the sequence's
    /// cycle type (`fnv_collide::anim::AnimSequence::cycle_time`).
    pub time: f32,
    /// A drawn rig wrote it this frame (its time is the drawn clip's); otherwise it runs
    /// on by itself.
    #[serde(skip)]
    pub drawn: bool,
}

impl GroupClock {
    /// Started by a script (saved and shown again when the object is drawn).
    pub fn scripted(&self) -> bool {
        self.group.is_some()
    }
}

/// Longest time a clock runs on by itself, seconds: past it a group played once is long
/// at its end, and a float this size still wraps a loop finely enough [engineering choice].
const MAX_TIME: f32 = 1.0e6;

#[derive(Resource, Default, Debug)]
pub struct ObjectGroups {
    map: HashMap<u32, GroupClock>,
    /// Bumped when a load puts the groups back, so the renderer shows them on objects
    /// drawn before that.
    generation: u32,
}

impl ObjectGroups {
    pub fn generation(&self) -> u32 {
        self.generation
    }

    pub fn get(&self, id: u32) -> Option<&GroupClock> {
        self.map.get(&id)
    }

    pub fn len(&self) -> usize {
        self.map.len()
    }

    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// A script started `group` (sequence `name`) on `id`, `time` seconds in.
    pub fn play(&mut self, id: u32, group: u16, name: &str, time: f32, drawn: bool) {
        self.map.insert(id, GroupClock { name: name.to_string(), group: Some(group), time: time.max(0.0), drawn });
    }

    /// Forget what `id` plays (Reset3DState): it stands in its stored pose.
    pub fn reset(&mut self, id: u32) {
        self.map.remove(&id);
    }

    /// The drawn rigs' sequences this frame: (reference, sequence name, seconds since its
    /// start). A script's group keeps its code while the rig still shows it; a rig showing
    /// another sequence (its idle, a radio's Open) replaces it. Clocks of rigs not drawn
    /// now run on by themselves; those of what an object played by itself are dropped.
    pub fn sync_drawn<'a>(&mut self, drawn: impl IntoIterator<Item = (u32, &'a str, f32)>) {
        for c in self.map.values_mut() {
            c.drawn = false;
        }
        for (id, name, time) in drawn {
            let time = if time.is_finite() { time.max(0.0) } else { 0.0 };
            match self.map.get_mut(&id) {
                Some(c) if c.name.eq_ignore_ascii_case(name) => {
                    c.time = time;
                    c.drawn = true;
                }
                _ => {
                    self.map.insert(id, GroupClock { name: name.to_string(), group: None, time, drawn: true });
                }
            }
        }
        self.map.retain(|_, c| c.drawn || c.scripted());
    }

    /// Run the clocks no drawn rig writes on by `dt` seconds.
    pub fn advance(&mut self, dt: f32) {
        let dt = if dt.is_finite() { dt.max(0.0) } else { 0.0 };
        for c in self.map.values_mut().filter(|c| !c.drawn) {
            c.time = (c.time + dt).min(MAX_TIME);
        }
    }

    /// The scripts' groups, for the save.
    fn saved(&self) -> BTreeMap<String, GroupClock> {
        self.map.iter().filter(|(_, c)| c.scripted()).map(|(id, c)| (format!("{id:08X}"), c.clone())).collect()
    }

    fn restore(&mut self, saved: BTreeMap<String, GroupClock>) {
        self.generation = self.generation.wrapping_add(1);
        self.map = saved.into_iter().filter_map(|(k, c)| Some((u32::from_str_radix(&k, 16).ok()?, GroupClock { drawn: false, ..c }))).collect();
    }
}

fn advance(time: Res<Time>, mut groups: ResMut<ObjectGroups>) {
    groups.advance(time.delta_secs());
}

/// The scripts' groups in the engine's part of a save, by reference FormID in hex.
pub const SAVE_HOOK: crate::save::SaveHook = crate::save::SaveHook {
    key: "object_groups",
    save: |w| {
        let g = w.get_resource::<ObjectGroups>()?.saved();
        (!g.is_empty()).then(|| serde_json::to_value(g).ok()).flatten()
    },
    load: |w, v| {
        let saved: BTreeMap<String, GroupClock> = serde_json::from_value(v.clone()).unwrap_or_default();
        let n = saved.len();
        w.get_resource_or_insert_with(ObjectGroups::default).restore(saved);
        info!("load: {n} object animation group(s) put back");
    },
    remap: Some(|v, r| {
        if let Some(o) = v.as_object_mut() {
            *o = std::mem::take(o).into_iter().filter_map(|(k, x)| Some((format!("{:08X}", r.map(u32::from_str_radix(&k, 16).ok()?)?), x))).collect();
        }
    }),
};

/// Added by `crate::PlayPlugin` (with the physics that reads it).
pub struct ObjectGroupsPlugin;

impl Plugin for ObjectGroupsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ObjectGroups>().add_systems(Update, advance.before(crate::PlaySet::Physics));
        crate::save::add_save_hook(app, SAVE_HOOK);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_scripts_group_is_kept_saved_and_runs_on_out_of_sight() {
        let mut g = ObjectGroups::default();
        g.play(7, 0x10, "Forward", 0.0, true);
        g.sync_drawn([(7, "Forward", 0.5), (8, "Idle", 1.0)]);
        assert_eq!(g.get(7).map(|c| (c.time, c.drawn)), Some((0.5, true)));
        g.advance(1.0);
        assert_eq!(g.get(7).unwrap().time, 0.5, "a drawn rig's clock is the drawn clip's");
        // Out of sight: the script's group runs on; the idle is forgotten.
        g.sync_drawn([]);
        assert!(g.get(8).is_none());
        g.advance(2.0);
        assert_eq!(g.get(7).unwrap().time, 2.5);
        // Saved and put back (reference 7 keeps its group and time).
        let mut back = ObjectGroups::default();
        back.restore(serde_json::from_value(serde_json::to_value(g.saved()).unwrap()).unwrap());
        assert_eq!(back.get(7).map(|c| (c.name.as_str(), c.group, c.time, c.drawn)), Some(("Forward", Some(0x10), 2.5, false)));
        // A rig showing another sequence replaces the script's group; a reset forgets it.
        g.sync_drawn([(7, "Idle", 0.1)]);
        assert!(!g.get(7).unwrap().scripted());
        g.play(7, 0x10, "Forward", 0.0, false);
        g.reset(7);
        assert!(g.get(7).is_none());
    }
}
