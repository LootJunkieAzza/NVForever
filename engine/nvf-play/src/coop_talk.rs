//! Co-op: who is in a conversation with a player, and what may hurt them meanwhile
//! (owner, 2026-11-10: "Players only and a setting"; queue items 195 and 197).
//!
//! - Each game tells the others the actor its player is talking to (a table action,
//!   [`game::TALK`], table 0: the actor's reference, 0 when the talk is over), again
//!   every [`AGAIN`] seconds while it lasts; what is not heard for [`STALE`] seconds is
//!   forgotten (a player that left).
//! - Every game gives its scripts' game the whole set each frame
//!   (`fnv_game::Game::coop_talking`): its own player's and the others'. The actors in it
//!   hold their place (nvforever `residents`), and only the players' hits hurt them
//!   (`fnv_game::combat::talk_guard`): not another NPC's, a creature's, a trap's or an
//!   explosion nobody owns. Hostiles still pick them as targets; the damage is what is
//!   ignored [design: the simplest rule].
//! - **The setting** `kill-in-conversation` (Settings > Co-op, "Players can kill NPCs in
//!   conversation", on by default, kept in NVForever.ini): off, nobody's hit hurts an
//!   actor in a conversation. The host's game decides every hit on the world's actors
//!   (a guest's hits go to it), so the host's value is the session's.
//!
//! Outside a session nothing is sent and the set is empty: solo is unchanged.
//!
//! Log: `co-op talk: in conversation now: [...]` when the set changes; the hits turned
//! away are logged by `fnv_game` (`co-op: <actor> is in a conversation with a player:
//! the hit by <who> does no damage (...)`).

use crate::coop_tables::{game, Tables};
use crate::script::ScriptState;
use crate::settings::{add_setting, Setting};
use crate::PlaySet;
use bevy::prelude::*;
use fnv_sim::FormId;
use std::collections::{BTreeSet, HashMap};

/// Seconds between two tellings of a talk that lasts, and after which one not heard
/// again is forgotten.
const AGAIN: f32 = 1.0;
const STALE: f32 = 3.0;

#[derive(Resource)]
pub struct TalkGuard {
    /// The setting: players may kill an NPC in a conversation.
    pub players_may_kill: bool,
    /// What each other player talks to, and when it was last heard.
    heard: HashMap<u8, (u32, f32)>,
    /// What this game last told, and when.
    told: (u32, f32),
}

impl Default for TalkGuard {
    fn default() -> Self {
        // `NVF_COOP_KILL_IN_TALK=0` starts with the setting off (the two-window check).
        TalkGuard { players_may_kill: std::env::var("NVF_COOP_KILL_IN_TALK").map_or(true, |v| v != "0"), heard: HashMap::new(), told: (0, 0.0) }
    }
}

pub struct CoopTalkPlugin;

impl Plugin for CoopTalkPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TalkGuard>().add_systems(Update, share.in_set(PlaySet::Ui));
        add_setting(
            app,
            Setting {
                key: "kill-in-conversation",
                group: "Co-op",
                label: "Players can kill NPCs in conversation",
                note: "On: a player can hurt and kill an NPC another player is talking to (nothing else can meanwhile). Off: nobody can until the talk ends. The host's choice counts for everyone.",
                get: |w| w.resource::<TalkGuard>().players_may_kill,
                set: |w, on| {
                    info!("co-op talk: players can{} kill NPCs in conversation", if on { "" } else { "not" });
                    w.resource_mut::<TalkGuard>().players_may_kill = on;
                },
                slider: None,
            },
        );
    }
}

/// The set a game gives its scripts: its own talk and the others' still fresh.
fn talking(own: u32, heard: &HashMap<u8, (u32, f32)>, now: f32) -> BTreeSet<FormId> {
    heard.values().filter(|(_, at)| now - at < STALE).map(|(a, _)| *a).chain([own]).filter(|a| *a != 0).map(FormId).collect()
}

fn share(time: Res<Time<Real>>, mut script: Option<ResMut<ScriptState>>, mut tables: Option<ResMut<Tables>>, mut st: ResMut<TalkGuard>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    let g = &mut rt.game;
    g.coop_talk_kill = st.players_may_kill;
    let Some(tables) = tables.as_deref_mut().filter(|t| t.on) else {
        if !g.coop_talking.is_empty() {
            g.coop_talking.clear();
        }
        st.heard.clear();
        st.told = (0, 0.0);
        return;
    };
    let now = time.elapsed_secs();
    let own = g.dialogue.as_ref().map_or(0, |c| c.actor.0);
    if own != st.told.0 || (own != 0 && now - st.told.1 >= AGAIN) {
        tables.send(0, game::TALK, own.to_le_bytes().to_vec());
        st.told = (own, now);
    }
    for (from, _, blob) in tables.take_actions(game::TALK) {
        let Some(actor) = blob.get(0..4).and_then(|b| b.try_into().ok()).map(u32::from_le_bytes) else { continue };
        if actor == 0 {
            st.heard.remove(&from);
        } else {
            st.heard.insert(from, (actor, now));
        }
    }
    st.heard.retain(|_, (_, at)| now - *at < STALE);
    let set = talking(own, &st.heard, now);
    if set != g.coop_talking {
        info!("co-op talk: in conversation now: {:?}", set.iter().map(|r| r.to_string()).collect::<Vec<_>>());
        g.coop_talking = set;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_set_is_this_players_talk_and_the_others_still_fresh() {
        let mut heard = HashMap::new();
        heard.insert(1u8, (0x0010_4E85, 10.0));
        heard.insert(2u8, (0x0010_4C79, 5.0));
        // Player 2's was last heard 5.5 s ago: forgotten.
        assert_eq!(talking(0x0010_4C80, &heard, 10.5), [FormId(0x0010_4C80), FormId(0x0010_4E85)].into_iter().collect());
        assert!(talking(0, &HashMap::new(), 1.0).is_empty());
    }
}
