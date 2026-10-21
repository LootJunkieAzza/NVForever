//! Challenges as the player sees them (docs/spec-notes/challenges.md §4–§5): the corner
//! messages of a challenge's progress and completion, and the rows of the Pip-Boy's
//! Challenges list. The counting is the rules' (`fnv_game::challenges`).

use crate::corner_messages::CornerMessages;
use crate::script::ScriptState;
use crate::script_events::Note;
use bevy::prelude::*;
use fnv_game::challenges::{Notice, SHOW_ZERO_PROGRESS, START_DISABLED};
use fnv_sim::FormId;

/// The completion message's icon, the game's one challenge picture [hypothesis: the only
/// interface texture named for challenges, `perk_challenge.dds`].
pub const ICON: &str = r"interface\icons\pipboyimages\perks\perk_challenge.dds";

/// The corner line of a notice [hypothesis: the wording; challenges.md §4].
pub fn notice_text(g: &fnv_game::Game, n: &Notice) -> String {
    match n {
        Notice::Progress { challenge, progress, threshold } => format!("{} {progress}/{threshold}", title(g, *challenge)),
        Notice::Completed { challenge } => format!("Challenge completed: {}", title(g, *challenge)),
    }
}

fn title(g: &fnv_game::Game, c: FormId) -> String {
    g.data.challenge_defs.get(&c).map_or_else(|| g.data.name(c), |d| d.title(g))
}

/// Notices of the rules' challenge steps go to the corner messages.
pub(crate) fn notices(mut script: Option<ResMut<ScriptState>>, mut corner: Option<ResMut<CornerMessages>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    for n in rt.game.take_challenge_notices() {
        let text = notice_text(&rt.game, &n);
        info!("challenge notice: {text}");
        if let Some(c) = corner.as_deref_mut() {
            let icon = matches!(n, Notice::Completed { .. }).then(|| ICON.to_string());
            c.push(Note { text, icon, seconds: None });
        }
    }
}

/// One row of the Pip-Boy's Challenges list.
#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub challenge: FormId,
    pub name: String,
    pub description: String,
    pub progress: u32,
    pub threshold: u32,
    pub completed: bool,
}

/// The challenges the list shows: unlocked ones (not Start Disabled, or unlocked by
/// UnlockChallenge) with progress or completed, and those flagged Show Zero Progress
/// also at 0 [hypothesis: the flag's name]. Running ones first, then completed, each by
/// name [hypothesis: as the quest list puts done quests last].
pub fn rows(g: &fnv_game::Game) -> Vec<Row> {
    let mut out: Vec<Row> = g
        .data
        .challenge_defs
        .values()
        .filter_map(|d| {
            let s = g.player.challenges.get(&d.form);
            let unlocked = !d.has(START_DISABLED) || s.is_some_and(|s| s.unlocked);
            let completed = s.is_some_and(|s| s.completed);
            let progress = s.map_or(0, |s| s.progress);
            let shown = unlocked && (completed || progress > 0 || d.has(SHOW_ZERO_PROGRESS));
            // A nameless challenge has nothing to list.
            let name = d.name.clone()?;
            shown.then(|| Row { challenge: d.form, name, description: d.description.clone().unwrap_or_default(), progress, threshold: d.goal(), completed })
        })
        .collect();
    out.sort_by(|a, b| a.completed.cmp(&b.completed).then_with(|| a.name.cmp(&b.name)).then_with(|| a.challenge.cmp(&b.challenge)));
    out
}
