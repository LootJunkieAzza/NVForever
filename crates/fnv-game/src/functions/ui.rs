//! Functions that ask something of the Pip-Boy, the terminal and the character-creation
//! menus, and the scripted challenges (docs/design/script-stubs.md §3). The game keeps
//! the request ([`UiRequests`], `crate::terminal::Terminals::back`); the engine's menus
//! take it.

use super::{none, Args, Ctx};
use crate::game::{menus, Game};
use fnv_script::vm::Value;
use fnv_sim::FormId;

/// How a script opened the S.P.E.C.I.A.L. menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecialMenu {
    /// AddSPECIALPoints: this many points more to give out, none to take back.
    Add(i32),
    /// SetSPECIALPoints: the attributes add up to this when the menu is left.
    Set(i32),
}

/// SetTagSkills' rules for the tag-skill menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SetTags {
    /// The skills tagged when the menu may be left.
    pub total: usize,
    /// The skills tagged before are shown (tagged on the sheet).
    pub show_tagged: bool,
}

/// What scripts asked of the menus, for the engine to take.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct UiRequests {
    /// ForceActiveQuest: the quest to make the Pip-Boy's active quest, and how many
    /// times a quest was forced (the Pip-Boy takes each new count once).
    pub active_quest: Option<FormId>,
    pub active_quest_count: u64,
    /// The rules of the next S.P.E.C.I.A.L. menu (`crate::chargen::SpecialSheet::for_menu`).
    pub special: Option<SpecialMenu>,
    /// AddTagSkills: skills to tag in the next tag-skill menu besides those tagged
    /// (`crate::chargen::TagSheet::for_menu`).
    pub extra_tags: Option<usize>,
    /// SetTagSkills: the rules of the next tag-skill menu (`crate::chargen::TagSheet::for_menu`).
    pub set_tags: Option<SetTags>,
    /// SexChange with resetAppearance: the player's look is to become the race's starting
    /// face for the new sex, which the engine's creator data has (`nvf_play::creator`).
    pub reset_look: bool,
    /// SetGlobalTimeMultiplier: the game's speed (`None`: never set, 1). The engine
    /// applies it to its clock while no menu holds the game.
    pub time_multiplier: Option<f64>,
    /// SetGlobalTimeMultiplier's `keepSoundUnchanged` of the last call: sounds keep their
    /// speed (else they follow the multiplier).
    pub keep_sound: bool,
    /// Autosave calls the engine has not taken yet.
    pub autosaves: u32,
    /// The console's camera functions (`super::camera`).
    pub camera: super::camera::CameraRequests,
}

/// `IncrementScriptedChallenge challenge`: one step towards the challenge's threshold
/// [documented: GECK wiki IncrementScriptedChallenge]. At the threshold (CHAL DATA) it
/// is completed (GetChallengeCompleted); a recurring one then starts again from 0, a
/// non-recurring one stays at its threshold [documented: GECK wiki GetChallengeProgress
/// (a ShowOff function), "will return the max Threshold value" / "will return to 0"].
/// A challenge flagged Start Disabled does not move until UnlockChallenge [hypothesis:
/// what "Start Disabled" means for progress]. The XP reward and the "challenge
/// completed" message of the original are not given yet.
pub fn increment_scripted_challenge(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let c = a.form(0)?;
    let Some(&flags) = g.data.challenges.get(&c) else {
        return Err(format!("{} is not a challenge", g.data.name(c)));
    };
    let threshold = g.data.challenge_thresholds.get(&c).copied().unwrap_or(1).max(1);
    let s = g.player.challenges.entry(c).or_default();
    let recurring = flags & 2 != 0 && !s.not_recurring;
    if (flags & 1 != 0 && !s.unlocked) || (s.completed && !recurring) {
        return none();
    }
    s.progress += 1;
    if s.progress >= threshold {
        s.completed = true;
        s.progress = if recurring { 0 } else { threshold };
        g.count_stat(crate::misc_stats::CHALLENGES_COMPLETED, 1.0);
        tracing::info!(challenge = %g.data.name(c), "challenge completed");
    }
    none()
}

/// `ForceActiveQuest quest`: the quest becomes the active quest [documented: GECK wiki
/// ForceActiveQuest]: the Pip-Boy's Quests list marks it and its notes and map markers
/// follow (`nvf_play::pipboy` takes [`UiRequests::active_quest`]).
/// `Autosave`: the game saves over the current autosave slot [documented: GECK wiki
/// Autosave]. The engine takes the request at the end of the frame (`nvf_play::save`).
pub fn autosave(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.ui.autosaves += 1;
    Ok(Value::ZERO)
}

pub fn force_active_quest(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    let q = a.form(0)?;
    if !g.data.quests.contains_key(&q) {
        return Err(format!("{} is not a quest", g.data.name(q)));
    }
    g.ui.active_quest = Some(q);
    g.ui.active_quest_count += 1;
    none()
}

/// `ForceTerminalBack`: called from a terminal menu item's result script, the terminal
/// goes back one level, or closes from its top level [documented: GECK wiki
/// ForceTerminalBack]. Handed to the engine's terminal by `crate::terminal::choose`.
pub fn force_terminal_back(g: &mut Game, _ctx: &Ctx, _a: &Args) -> Result<Value, String> {
    g.terminals.back = true;
    none()
}

/// `AddSPECIALPoints n`: opens the S.P.E.C.I.A.L. menu with n more points to give out;
/// the attributes cannot go below what they were [documented: GECK wiki
/// AddSPECIALPoints, used by the Intense Training perk]. The menu is the character
/// menu's attributes page, "ATTRIBUTES: 0/1 Points Assigned" [observed: the owner's
/// screenshots after Intense Training, docs/spec-notes/levelup.md §8; documented:
/// char_gen_menu.xml's attribute template and `_OnlyAdd`; the catalogue, "Opens the
/// character-generation Attributes page"], the menu the tag-skill page is (1048)
/// [hypothesis: its MenuMode code is not captured].
pub fn add_special_points(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.ui.special = Some(SpecialMenu::Add(a.int(0)?));
    g.open_menu(menus::TAG_SKILLS);
    none()
}

/// `SetSPECIALPoints n`: opens the S.P.E.C.I.A.L. menu, which may be left once the
/// attributes add up to n [documented: GECK wiki SetSPECIALPoints].
pub fn set_special_points(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.ui.special = Some(SpecialMenu::Set(a.int(0)?));
    g.open_menu(menus::LOVE_TESTER);
    none()
}

/// `AddTagSkills n`: opens the tag-skill menu to tag n more skills; the skills tagged
/// before cannot be untagged [documented: GECK wiki AddTagSkills; its note that only
/// four tags take effect is not applied: the Tag! perk's own rule is the perk system's].
pub fn add_tag_skills(g: &mut Game, _ctx: &Ctx, a: &Args) -> Result<Value, String> {
    g.ui.extra_tags = Some(a.int(0)?.max(0) as usize);
    g.open_menu(menus::TAG_SKILLS);
    none()
}
