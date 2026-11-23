//! Challenges as the player sees them (docs/spec-notes/challenges.md §4–§5): the corner
//! messages of a challenge's progress and completion, and the rows of the Pip-Boy's
//! Challenges list. The counting is the rules' (`fnv_game::challenges`).

use crate::corner_messages::CornerMessages;
use crate::script::ScriptState;
use crate::script_events::Note;
use bevy::prelude::*;
use fnv_game::challenges::{Notice, RECURRING, SHOW_ZERO_PROGRESS, START_DISABLED};
use fnv_sim::FormId;

/// The corner line of a notice [hypothesis: the wording; challenges.md §4].
pub fn notice_text(g: &fnv_game::Game, n: &Notice) -> String {
    match n {
        Notice::Progress { challenge, progress, threshold } => format!("{} {progress}/{threshold}", title(g, *challenge)),
        Notice::Completed { challenge } => format!("Challenge completed: {}", title(g, *challenge)),
    }
}

/// The corner notification for a challenge progress or completion notice. Uses the
/// default Vault Boy corner icon (`icon: None` -> `corner_messages::DEFAULT_ICON`,
/// `glow_message_vaultboy_neutral.dds`) rather than Pip-Boy perk art (`perk_challenge.dds`,
/// a 256x256 full-body illustration not in `InterfaceShared.tai`) [observed: no master
/// `CHAL` record has an `ICON` subrecord and no GMST names a challenge icon; hypothesis:
/// default neutral face pending an unmodded original-game capture, challenges.md §4].
pub fn notice_note(g: &fnv_game::Game, n: &Notice) -> Note {
    Note { text: notice_text(g, n), icon: None, seconds: None }
}

fn title(g: &fnv_game::Game, c: FormId) -> String {
    g.data.challenge_defs.get(&c).map_or_else(|| g.data.name(c), |d| d.title(g))
}

/// Notices of the rules' challenge steps go to the corner messages.
pub(crate) fn notices(mut script: Option<ResMut<ScriptState>>, mut corner: Option<ResMut<CornerMessages>>) {
    let Some(rt) = script.as_deref_mut().and_then(|s| s.rt.as_mut()) else { return };
    for n in rt.game.take_challenge_notices() {
        let note = notice_note(&rt.game, &n);
        info!("challenge notice: {}", note.text);
        if let Some(c) = corner.as_deref_mut() {
            c.push(note);
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
    /// Marked as starting again when done, under the player's tweak [design: docs/design/tweaks.md].
    pub recurring: bool,
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
            let recurring = g.tweaks.mark_recurring_challenges && d.has(RECURRING) && !s.is_some_and(|s| s.not_recurring);
            shown.then(|| Row { challenge: d.form, name, description: d.description.clone().unwrap_or_default(), progress, threshold: d.goal(), completed, recurring })
        })
        .collect();
    out.sort_by(|a, b| a.completed.cmp(&b.completed).then_with(|| a.name.cmp(&b.name)).then_with(|| a.challenge.cmp(&b.challenge)));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_esm::{LoadOrder, Plugin, Schema};
    use fnv_game::{Game, GameConfig, GameData};
    use fnv_sim::{FormStore, RefTable, SimConfig, World};

    const CHAL_ID: u32 = 0x0013_5678;

    fn sub(sig: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(6 + data.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(data.len() as u16).to_le_bytes());
        out.extend_from_slice(data);
        out
    }

    fn record(sig: &[u8; 4], flags: u32, id: u32, subs: &[Vec<u8>]) -> Vec<u8> {
        let body: Vec<u8> = subs.iter().flatten().copied().collect();
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(sig);
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&id.to_le_bytes());
        out.extend_from_slice(&[0; 8]);
        out.extend_from_slice(&body);
        out
    }

    fn top_group(label: &[u8; 4], body: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(24 + body.len());
        out.extend_from_slice(b"GRUP");
        out.extend_from_slice(&((24 + body.len()) as u32).to_le_bytes());
        out.extend_from_slice(label);
        out.extend_from_slice(&[0; 12]);
        out.extend_from_slice(body);
        out
    }

    fn chal_plugin_bytes() -> Vec<u8> {
        let hedr = [1.34f32.to_le_bytes(), 1i32.to_le_bytes(), 0x800u32.to_le_bytes()].concat();
        // CHAL DATA: type=13 (scripted), threshold=100, flags=0, interval=25, value1/2/3=0.
        let mut data = Vec::with_capacity(24);
        data.extend_from_slice(&13u32.to_le_bytes());
        data.extend_from_slice(&100u32.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        data.extend_from_slice(&25u32.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u16.to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes());
        let chal = record(
            b"CHAL",
            0,
            CHAL_ID,
            &[
                sub(b"EDID", b"TestLimberJack\0"),
                sub(b"FULL", b"Limber Jack\0"),
                sub(b"DESC", b"Cripple limbs.\0"),
                sub(b"DATA", &data),
            ],
        );
        [record(b"TES4", 1, 0, &[sub(b"HEDR", &hedr)]), top_group(b"CHAL", &chal)].concat()
    }

    /// #3433: both challenge progress and challenge completion corner notifications must
    /// leave `icon` as `None` (default Vault Boy corner icon) rather than attaching the
    /// 256x256 Pip-Boy perk illustration `perk_challenge.dds`.
    #[test]
    fn challenge_notices_use_default_corner_icon() {
        let plugins = vec![Plugin::parse(&chal_plugin_bytes(), "TestChallenge.esm".into()).unwrap()];
        let lo = LoadOrder::new(&plugins).unwrap();
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../schemas");
        let schema = Schema::load_dir(root.join("fnv")).unwrap();
        let functions = root.join("script/functions.json");
        let json: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&functions).unwrap()).unwrap();
        let (forms, refs) = (FormStore::build(&lo, &schema), RefTable::build(&lo, &schema));
        let data = GameData::build(&lo, &schema, fnv_script::FunctionTable::load(&functions).unwrap(), &json, &refs);
        let mut game = Game::new(&data, World::new(&forms, &refs, SimConfig::default()), GameConfig::default());
        game.new_game();

        let id = FormId(CHAL_ID);
        game.challenge_step(id, 75.0);
        let step_notices = game.take_challenge_notices();
        assert_eq!(step_notices, vec![Notice::Progress { challenge: id, progress: 75, threshold: 100 }]);
        let progress = notice_note(&game, &step_notices[0]);
        assert_eq!(progress, Note { text: "Limber Jack 75/100".into(), icon: None, seconds: None });

        game.challenge_step(id, 25.0);
        let done_notices = game.take_challenge_notices();
        assert_eq!(done_notices, vec![Notice::Completed { challenge: id }]);
        let completed = notice_note(&game, &done_notices[0]);
        assert_eq!(completed, Note { text: "Challenge completed: Limber Jack".into(), icon: None, seconds: None });

        let mut corner = CornerMessages::default();
        assert!(corner.push(completed.clone()));
        assert!(corner.step(0.0));
        assert_eq!(corner.showing(), Some(&completed));
    }
}
