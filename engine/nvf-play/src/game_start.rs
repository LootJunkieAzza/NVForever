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
//! The game's own INIs come first (owner, 2026-11-11: a mod list set up for the original,
//! such as an MO2 profile with TTW, must start the way it does there): `Fallout.ini`, then
//! `FalloutCustom.ini` over it, read (never written) from `Documents\My Games\FalloutNV`,
//! which MO2 maps to the profile's own INIs when the profile has local settings
//! [documented: MO2 profile local INIs; hypothesis: FalloutCustom.ini's keys win over
//! Fallout.ini's, as TTW's install guide relies on]. `NVForever.ini` gives what neither
//! sets. The original's default film name (`Fallout INTRO Vsk.bik`, which New Vegas does
//! not ship) counts as not set, so the owner's default above stays. Unattended runs read
//! none of the game's INIs unless `NVF_PLAY_GAME_INIS=1`.
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
/// The original's default film name [documented: Fallout_default.ini], which New Vegas
/// does not ship: a game INI naming it sets nothing here.
const ORIGINAL_DEFAULT_INTRO: &str = "Fallout INTRO Vsk.bik";

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
        s.apply(text);
        s
    }

    /// The keys an INI's text sets, over what is there; true when it set one.
    pub fn apply(&mut self, text: &str) -> bool {
        let mut set = false;
        if let Some(q) = ini_get(text, SECTION, CHARGEN_KEY).map(|v| unquote(&v).to_string()).filter(|v| !v.is_empty()) {
            self.chargen_quest = q;
            set = true;
        }
        if let Some(v) = ini_get(text, SECTION, INTRO_KEY).filter(|v| !unquote(v).eq_ignore_ascii_case(ORIGINAL_DEFAULT_INTRO)) {
            self.intro = film_path(unquote(&v));
            set = true;
        }
        set
    }

    /// NVForever.ini's keys, then the game's `Fallout.ini` and `FalloutCustom.ini` over
    /// them (module notes); the names of the game INIs that set a key, in order.
    pub fn chain(nvf: Option<&str>, game: &[(String, String)]) -> (GameStart, Vec<String>) {
        let mut s = GameStart::default();
        let mut from = Vec::new();
        if let Some(t) = nvf {
            s.apply(t);
        }
        for (name, text) in game {
            if s.apply(text) {
                from.push(name.clone());
            }
        }
        (s, from)
    }

    /// The game's `Fallout.ini` and `FalloutCustom.ini` that exist, by path, as text.
    fn game_inis() -> Vec<(String, String)> {
        let Some(dir) = fnv_install::original_user_dir() else { return Vec::new() };
        ["Fallout.ini", "FalloutCustom.ini"]
            .iter()
            .filter_map(|n| {
                let p = dir.join(n);
                let b = std::fs::read(&p).ok()?;
                Some((p.display().to_string(), String::from_utf8_lossy(&b).trim_start_matches('\u{feff}').to_string()))
            })
            .collect()
    }

    /// The start-up's: the INI this run reads ([`SettingsFile::for_run`]), else the
    /// defaults.
    pub fn for_run(unattended: bool) -> GameStart {
        let file = SettingsFile::for_run(unattended).0;
        let text = file.as_deref().and_then(|p| std::fs::read_to_string(p).ok());
        // An unattended run reads them only when asked (`NVF_PLAY_GAME_INIS=1`: a test of a
        // mod list's start through MO2, which maps them to its profile).
        let asked = std::env::var("NVF_PLAY_GAME_INIS").is_ok_and(|v| v == "1");
        let game = if unattended && !asked { Vec::new() } else { GameStart::game_inis() };
        let (s, game_from) = GameStart::chain(text.as_deref(), &game);
        let mut from = match (&file, &text) {
            (Some(p), Some(_)) => p.display().to_string(),
            (Some(p), None) => format!("defaults, no {}", p.display()),
            (None, _) => "defaults, unattended run".into(),
        };
        if !game_from.is_empty() {
            from = format!("{} over {from}", game_from.join(" over "));
        }
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
            g.data.by_edid(s).ok_or_else(|| format!("no form named {s:?} in the load order"))?
        };
        if g.data.is_quest(q) {
            Ok(q)
        } else {
            Err(not_a_quest(s, q))
        }
    }
}

/// The start's warning when `sCharGenQuest` names no quest the load order can start
/// (issue #1394): the player is then left at the New Vegas opening, easy to mistake for
/// the INI line being ignored. A mod's start quest (Tale of Two Wastelands' `001FFFF8`) is
/// not a quest until its plugin is enabled in NVForever's own `plugins.txt`
/// [observed: `fnvtool esm decode`, FalloutNV.esm has no `001FFFF8`]; NVForever reads only
/// its own `NVForever.ini`, never the original's `Fallout.ini` (docs/design/install-layout.md).
pub fn not_a_quest(value: &str, resolved: FormId) -> String {
    format!(
        "{value} ({resolved}) is not a quest in the load order; New Game starts nothing and \
         leaves the player at the New Vegas opening. If {value} names a mod's start quest, \
         enable that plugin in NVForever's own plugins.txt (NVForever reads only its own \
         NVForever.ini, never the original's)"
    )
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
    fn the_game_inis_win_over_nvforever_ini_and_custom_over_fallout_ini() {
        let nvf = "[General]\nsCharGenQuest=VCG00\nsIntroMovie=video\\fnvintro.bik\n";
        let fallout = ("Fallout.ini".to_string(), "[General]\nSCharGenQuest=00102037\nsIntroMovie=Fallout INTRO Vsk.bik\n".to_string());
        let custom = ("FalloutCustom.ini".to_string(), "[General]\nSCharGenQuest=001FFFF8\nSIntroMovie=\n".to_string());
        let (s, from) = GameStart::chain(Some(nvf), &[fallout.clone(), custom]);
        assert_eq!((s.chargen_quest.as_str(), s.intro.as_deref()), ("001FFFF8", None));
        assert_eq!(from, ["Fallout.ini", "FalloutCustom.ini"]);
        // The original's default film name is not a film New Vegas has: NVForever's stays.
        let (s, _) = GameStart::chain(Some(nvf), &[fallout]);
        assert_eq!((s.chargen_quest.as_str(), s.intro.as_deref()), ("00102037", Some("video\\fnvintro.bik")));
        // Without the game's INIs, NVForever.ini's.
        assert_eq!(GameStart::chain(Some(nvf), &[]).0.chargen_quest, "VCG00");
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

    #[test]
    fn a_start_quest_not_in_the_load_order_names_the_plugin_to_enable() {
        let m = not_a_quest("001FFFF8", FormId(0x001FFFF8));
        assert!(m.contains("001FFFF8 (0x001FFFF8) is not a quest in the load order"), "{m}");
        assert!(m.contains("New Game starts nothing and leaves the player at the New Vegas opening"), "{m}");
        assert!(m.contains("enable that plugin in NVForever's own plugins.txt"), "{m}");
        assert!(m.contains("NVForever reads only its own NVForever.ini, never the original's"), "{m}");
    }
}

/// The INI section and key of the base archive list, the original's (`[Archive]
/// SArchiveList` [documented: its `Fallout_default.ini`; observed: the owner's
/// `Fallout.ini`, docs/spec-notes/bsa.md]), read from NVForever's own INI, never the
/// game's (docs/design/install-layout.md).
pub const ARCHIVE_SECTION: &str = "Archive";
pub const ARCHIVE_LIST_KEY: &str = "SArchiveList";

/// The base archive list an INI's text sets, if it sets a non-empty one.
pub fn archive_list_of(text: &str) -> Option<Vec<String>> {
    let list = fnv_vfs::parse_archive_list(unquote(&ini_get(text, ARCHIVE_SECTION, ARCHIVE_LIST_KEY)?));
    (!list.is_empty()).then_some(list)
}

/// The base archive list NVForever.ini sets for this run (issue #1142: a mod setup that
/// adds base archives), with the file it came from; `None` without the key, and in an
/// unattended run without its own home (as every INI key, `SettingsFile::for_run`).
pub fn archive_list_for_run() -> Option<(Vec<String>, String)> {
    let file = SettingsFile::for_run(crate::test_aid::Unattended::from_env().0).0?;
    let list = archive_list_of(&std::fs::read_to_string(&file).ok()?)?;
    Some((list, file.display().to_string()))
}

#[cfg(test)]
mod archive_list_tests {
    use super::*;

    #[test]
    fn the_inis_archive_list_replaces_the_default_only_when_set() {
        let text = "[Archive]\nSArchiveList=Fallout - Textures.bsa, Fallout - Textures2.bsa, Fallout - Meshes.bsa, MyBase.bsa\n";
        assert_eq!(archive_list_of(text).unwrap().last().map(String::as_str), Some("MyBase.bsa"));
        assert_eq!(archive_list_of(text).unwrap().len(), 4);
        assert_eq!(archive_list_of("[Archive]\nSArchiveList=\n"), None);
        assert_eq!(archive_list_of("[General]\nSArchiveList=a.bsa\n"), None);
    }
}
