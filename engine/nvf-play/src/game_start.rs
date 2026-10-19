//! What a new game starts with, kept in `[General]` of the home's `NVForever.ini` under
//! the original's own key names, so a mod's INI lines can be copied in as they are (Tale
//! of Two Wastelands asks for `SCharGenQuest=001FFFF8` and an empty `SIntroMovie=`):
//!
//! - `sCharGenQuest=<FormID or Editor ID>`: the quest New Game starts
//!   (`crate::script::Runtime::start_opening`, `fnv_game::game::Game::start_chargen_quest`).
//!   Without the key, VCG00, as the original's defaults have it (`SCharGenQuest=00102037`
//!   [documented: Fallout_default.ini]). A FormID is the load-order one, eight hex digits
//!   with or without `0x` (a quest a mod adds into FalloutNV.esm's FormIDs, as TTW's
//!   `TTWStart` is, keeps its `00` [observed: TaleOfTwoWastelands.esm]). One that names no
//!   quest starts nothing, with a warning.
//! - `sIntroMovie=<file>`: the film New Game plays before the opening (`crate::intro`), a
//!   loose file under `Data\Video`; empty: none. Without the key, `FNVIntro.bik` [owner,
//!   2026-10-15], not the original's default `Fallout INTRO Vsk.bik` (Fallout 3's film,
//!   which New Vegas does not ship [documented: Fallout_default.ini; observed: the owner's
//!   `Data\Video`, 2026-10-18]; TTW brings it). The opening's own call for `FNVIntro.bik`
//!   (VCG00 stage 0, a PlayBink) is a script's film and still plays when this names
//!   another film or none, as in the original [hypothesis].
//!
//! Both are read once as the game starts: a change counts from the next start. The
//! player's first place does not follow the quest: a new game starts at the opening's
//! marker in Doc Mitchell's house (`crate::script::new_game_start`) and a start quest
//! moves the player itself (TTW's MenuMode block moves the player to its own marker
//! [observed: TaleOfTwoWastelands.esm, SCPT 0x0C010FAD]). Unattended runs (test aids)
//! read the INI only under their own `NVFOREVER_HOME`, as `crate::settings` does.
//!
//! Log: `game start: sCharGenQuest=… sIntroMovie=… (<where from>)`.

use crate::settings::SettingsFile;
use bevy::prelude::*;
use fnv_game::game::Game;
use fnv_install::ini_get;
use fnv_sim::FormId;

/// The INI section, the original's.
pub const SECTION: &str = "General";
pub const CHARGEN_KEY: &str = "sCharGenQuest";
pub const INTRO_KEY: &str = "sIntroMovie";
/// The quest New Game starts without `sCharGenQuest` (the original's default names it by
/// FormID, `00102037`).
pub const DEFAULT_CHARGEN_QUEST: &str = "VCG00";
/// The intro film without `sIntroMovie` [owner, 2026-10-15].
pub const DEFAULT_INTRO: &str = "video\\fnvintro.bik";

/// What a new game starts with (module notes).
#[derive(Resource, Clone, Debug, PartialEq)]
pub struct GameStart {
    /// `sCharGenQuest` as written (a FormID or an Editor ID).
    pub chargen_quest: String,
    /// The intro film's path in the data (`video\…`, lower case); `None`: no film.
    pub intro: Option<String>,
}

impl Default for GameStart {
    fn default() -> GameStart {
        GameStart { chargen_quest: DEFAULT_CHARGEN_QUEST.into(), intro: Some(DEFAULT_INTRO.into()) }
    }
}

impl GameStart {
    /// The keys of an INI's text; a key not there keeps its default.
    pub fn parse(text: &str) -> GameStart {
        let mut s = GameStart::default();
        if let Some(q) = ini_get(text, SECTION, CHARGEN_KEY).map(|v| unquote(&v).to_string()).filter(|v| !v.is_empty()) {
            s.chargen_quest = q;
        }
        if let Some(v) = ini_get(text, SECTION, INTRO_KEY) {
            s.intro = film_path(unquote(&v));
        }
        s
    }

    /// The start-up's: the INI this run reads ([`SettingsFile::for_run`]), else the
    /// defaults.
    pub fn for_run(unattended: bool) -> GameStart {
        let file = SettingsFile::for_run(unattended).0;
        let text = file.as_deref().and_then(|p| std::fs::read_to_string(p).ok());
        let s = text.as_deref().map(GameStart::parse).unwrap_or_default();
        let from = match (&file, &text) {
            (Some(p), Some(_)) => p.display().to_string(),
            (Some(p), None) => format!("defaults, no {}", p.display()),
            (None, _) => "defaults, unattended run".into(),
        };
        info!("game start: {CHARGEN_KEY}={} {INTRO_KEY}={} ({from})", s.chargen_quest, s.intro.as_deref().unwrap_or("(none)"));
        s
    }

    /// The run's, read once: the resource when one is there (a test's), else
    /// [`GameStart::for_run`], inserted.
    pub fn of_app(app: &mut App) -> GameStart {
        if let Some(s) = app.world().get_resource::<GameStart>() {
            return s.clone();
        }
        let s = GameStart::for_run(crate::test_aid::Unattended::from_env().0);
        app.insert_resource(s.clone());
        s
    }

    /// The start quest in `g`'s data.
    pub fn quest(&self, g: &Game) -> Result<FormId, String> {
        let s = self.chargen_quest.trim();
        let hex = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")).unwrap_or(s);
        let q = if hex.len() == 8 && hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            FormId(u32::from_str_radix(hex, 16).map_err(|e| e.to_string())?)
        } else {
            g.data.by_edid(s).ok_or_else(|| format!("no form named {s:?}"))?
        };
        if g.data.is_quest(q) {
            Ok(q)
        } else {
            Err(format!("{s} ({q}) is not a quest in the load order"))
        }
    }
}

/// A value without the quotes around it.
fn unquote(v: &str) -> &str {
    let v = v.trim();
    v.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(v)
}

/// The data path of a film named in the INI: a file in `Data\Video` (`video\` added
/// unless given), lower case, backslashes; empty: none.
pub fn film_path(name: &str) -> Option<String> {
    let n = name.trim().replace('/', "\\").to_ascii_lowercase();
    let n = n.trim_start_matches('\\');
    if n.is_empty() {
        return None;
    }
    Some(if n.starts_with("video\\") { n.to_string() } else { format!("video\\{n}") })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_the_keys_the_opening_and_its_film_are_the_defaults() {
        assert_eq!(GameStart::parse(""), GameStart::default());
        assert_eq!(GameStart::parse("[Settings]\ngrass=1\n").chargen_quest, "VCG00");
        assert_eq!(GameStart::default().intro.as_deref(), Some("video\\fnvintro.bik"));
    }

    #[test]
    fn ttws_lines_copied_in_name_its_quest_and_no_film() {
        let ini = "[Game]\nPath=F:\\FNV\n\n[General]\n; Tale of Two Wastelands specific\nSCharGenQuest=001FFFF8\nSIntroMovie=\n";
        let s = GameStart::parse(ini);
        assert_eq!(s.chargen_quest, "001FFFF8");
        assert_eq!(s.intro, None);
    }

    #[test]
    fn a_film_is_a_file_in_the_video_folder() {
        assert_eq!(film_path("Fallout INTRO Vsk.bik").as_deref(), Some("video\\fallout intro vsk.bik"));
        assert_eq!(film_path("Video/FNVIntro.bik").as_deref(), Some("video\\fnvintro.bik"));
        assert_eq!(film_path("  ").as_deref(), None);
        assert_eq!(GameStart::parse("[General]\nsIntroMovie=\"B01.bik\"\n").intro.as_deref(), Some("video\\b01.bik"));
        // An empty quest keeps the default rather than starting nothing.
        assert_eq!(GameStart::parse("[General]\nsCharGenQuest=\n").chargen_quest, "VCG00");
    }
}
