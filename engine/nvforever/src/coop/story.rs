//! The host's story on a guest (owner bug "guest cant use gate to walk i15", 2026-11-13):
//! the host sends its quests' states and quest script variables (`nvf_coop::story`), and
//! a guest's game keeps them where the world's scripts read them
//! (`fnv_game::coop::HostStory`). So a door or activator whose script asks the story,
//! Long 15's gate at the Mojave Outpost (`ALLong15EntranceQuest.iPapersPlease`, set when
//! the host talked to the guard), does on a guest what it does on the host: the gate's
//! own Activate and MoveTo take the guest through, as any door of the guest's own does.
//!
//! - **The host**, every [`SEND_EVERY`]: its story now (`Game::story_view`) to the link,
//!   which sends a page every `net::STORY_EVERY`.
//! - **A guest**: each page heard goes into its game's `coop_host_story`. One log line
//!   when the first comes, and one when it grows by a tenth.
//! - Leaving the lobby clears it (nvforever `coop`, where the guest's game becomes its
//!   own again).
//! - Test aid `NVF_COOP_NO_HOST_STORY=1` (a guest's): the pages heard are dropped, as
//!   before the fix, to show what a guest's world does on its own story.

use super::*;
use fnv_game::coop::{HostQuestState, HostStory};
use fnv_script::vm::Value;
use nvf_coop::story::{StoryEntry, StoryValue};

/// The host makes its story's entries again this often (seconds).
const SEND_EVERY: f32 = 0.2;

/// The story's state on this side.
#[derive(Default)]
pub(super) struct Shared {
    /// A guest's: pages heard, not yet in its game.
    pub(super) heard: Vec<StoryEntry>,
    /// The host's: when its story last went to the link.
    sent_at: Option<f32>,
    /// A guest's: entries kept when the last log line was written.
    logged: usize,
}

/// The host's story as entries for the link.
pub(super) fn entries(s: &HostStory) -> Vec<StoryEntry> {
    let quests = s.quests.iter().map(|(q, st)| StoryEntry::Quest {
        quest: q.0,
        stage: st.stage.clamp(i16::MIN.into(), i16::MAX.into()) as i16,
        running: st.running,
        ended: st.ended,
        done: st.done.iter().map(|d| (*d).clamp(i16::MIN.into(), i16::MAX.into()) as i16).collect(),
        failed: st.failed,
        objectives: st.objectives.iter().map(|(i, done)| ((*i).clamp(i16::MIN.into(), i16::MAX.into()) as i16, *done)).collect(),
        seen: st.seen.min(u32::MAX.into()) as u32,
    });
    let vars = s.vars.iter().map(|((q, index), v)| StoryEntry::Var {
        quest: q.0,
        index: *index,
        value: match v {
            Value::Num(n) => StoryValue::Num(*n),
            Value::Ref(r) => StoryValue::Ref(*r),
        },
    });
    quests.chain(vars).collect()
}

/// A guest takes entries heard into its game's story.
pub(super) fn take(story: &mut HostStory, heard: impl IntoIterator<Item = StoryEntry>) {
    for e in heard {
        match e {
            StoryEntry::Quest { quest, stage, running, ended, done, failed, objectives, seen } => {
                let objectives = objectives.into_iter().map(|(i, done)| (i32::from(i), done)).collect();
                story.quests.insert(FormId(quest), HostQuestState { stage: stage.into(), running, ended, done: done.into_iter().map(i32::from).collect(), failed, objectives, seen: seen.into() });
            }
            StoryEntry::Var { quest, index, value } => {
                let v = match value {
                    StoryValue::Num(n) => Value::Num(n),
                    StoryValue::Ref(r) => Value::Ref(r),
                };
                story.vars.insert((FormId(quest), index), v);
            }
        }
    }
}

/// Each frame: the host sends its story, a guest keeps what came (module notes).
pub(super) fn share_story(time: Res<Time>, mut coop: ResMut<Coop>, mut script: Option<ResMut<nvf_play::script::ScriptState>>, data: Option<Res<nvf_play::pipboy::data::DataState>>) {
    let now = time.elapsed_secs();
    let coop = &mut *coop;
    let Some(link) = coop.link.as_ref() else {
        coop.story = Shared::default();
        return;
    };
    let Some(g) = script.as_deref_mut().and_then(|s| s.rt.as_mut()).map(|rt| &mut rt.game) else { return };
    if link.is_host() {
        if coop.story.sent_at.is_none_or(|t| now - t >= SEND_EVERY) {
            coop.story.sent_at = Some(now);
            let mut view = g.story_view();
            // The host's quest list's order, for the guests' (item 205).
            if let Some(st) = data.as_deref() {
                for (q, s) in view.quests.iter_mut() {
                    s.seen = nvf_play::pipboy::data::quest_seen(st, q.0);
                }
            }
            link.set_story(entries(&view));
        }
        return;
    }
    if coop.story.heard.is_empty() || !g.coop_guest {
        return;
    }
    if std::env::var_os("NVF_COOP_NO_HOST_STORY").is_some_and(|v| v != "0") {
        coop.story.heard.clear();
        return;
    }
    take(&mut g.coop_host_story, std::mem::take(&mut coop.story.heard));
    let n = g.coop_host_story.quests.len() + g.coop_host_story.vars.len();
    if coop.story.logged == 0 || n > coop.story.logged + coop.story.logged / 10 {
        let shown = g.coop_host_story.quests.values().filter(|q| !q.objectives.is_empty()).count();
        info!("co-op: the host's story: {} quests ({shown} with objectives shown, for the quest list), {} quest variables (the world's scripts here read them)", g.coop_host_story.quests.len(), g.coop_host_story.vars.len());
        coop.story.logged = n;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hosts_story_comes_through_as_it_went() {
        let mut s = HostStory::default();
        s.quests.insert(FormId(0x1E00_2B11), HostQuestState { stage: 20, running: true, ended: false, done: vec![10, 20], failed: false, objectives: [(10, true), (20, false)].into(), seen: 4 });
        s.quests.insert(FormId(0x0010_4C1D), HostQuestState { stage: 80, running: false, ended: true, done: vec![70, 80], failed: true, objectives: [(10, true)].into(), seen: 2 });
        s.vars.insert((FormId(0x1E00_2B11), 1), Value::Num(1.0));
        s.vars.insert((FormId(0x1E00_2B11), 2), Value::Ref(0x0010_C0DE));
        let mut back = HostStory::default();
        take(&mut back, entries(&s));
        assert_eq!(back, s);
    }

    #[test]
    fn a_stage_out_of_range_is_held_at_the_edge() {
        let mut s = HostStory::default();
        s.quests.insert(FormId(1), HostQuestState { stage: 100_000, running: false, ended: true, done: vec![], ..Default::default() });
        assert!(matches!(entries(&s)[0], StoryEntry::Quest { stage: i16::MAX, ended: true, .. }));
    }
}
