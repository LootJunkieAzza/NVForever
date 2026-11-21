//! A new game's start: the start quest and the intro film the game reads from
//! NVForever.ini's `[General]` (`sCharGenQuest`, `sIntroMovie`).

use fnv_install::{ini_get, ini_set};

/// The section and keys the game reads (nvf-play game_start.rs `SECTION`, `CHARGEN_KEY`,
/// `INTRO_KEY`).
pub const SECTION: &str = "General";
pub const QUEST_KEY: &str = "sCharGenQuest";
pub const INTRO_KEY: &str = "sIntroMovie";

/// The card's fields.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Fields {
    /// `sCharGenQuest` (a FormID or an Editor ID); empty: no line, the New Vegas opening.
    pub quest: String,
    /// `sIntroMovie` (a file in `Data\Video`); empty: no line, the New Vegas film.
    pub film: String,
    /// `sIntroMovie=` with nothing after it: no film at all.
    pub no_film: bool,
}

fn value(text: &str, key: &str) -> Option<String> {
    ini_get(text, SECTION, key).map(|v| {
        let v = v.trim();
        v.strip_prefix('"').and_then(|r| r.strip_suffix('"')).unwrap_or(v).to_string()
    })
}

impl Fields {
    /// The fields as the INI's text has them.
    pub fn read(text: &str) -> Fields {
        let film = value(text, INTRO_KEY);
        Fields { quest: value(text, QUEST_KEY).unwrap_or_default(), no_film: film.as_deref().is_some_and(str::is_empty), film: film.unwrap_or_default() }
    }

    /// `text` with these fields written: an empty field takes its line out (the game's
    /// default), "no film" writes `sIntroMovie=`; every other line is kept.
    pub fn write(&self, text: &str) -> String {
        let quest = self.quest.trim();
        let t = if quest.is_empty() { ini_remove(text, SECTION, QUEST_KEY) } else { ini_set(text, SECTION, QUEST_KEY, quest) };
        let film = self.film.trim();
        if self.no_film {
            ini_set(&t, SECTION, INTRO_KEY, "")
        } else if film.is_empty() {
            ini_remove(&t, SECTION, INTRO_KEY)
        } else {
            ini_set(&t, SECTION, INTRO_KEY, film)
        }
    }
}

/// `text` without the `key=` lines of `[section]` (names without case); every other line kept.
fn ini_remove(text: &str, section: &str, key: &str) -> String {
    let mut in_section = false;
    let mut out: Vec<&str> = Vec::new();
    for line in text.lines() {
        let t = line.trim();
        if t.starts_with('[') && t.ends_with(']') {
            in_section = t[1..t.len() - 1].trim().eq_ignore_ascii_case(section);
        } else if in_section && !t.starts_with(';') && !t.starts_with('#') && t.split_once('=').is_some_and(|(k, _)| k.trim().eq_ignore_ascii_case(key)) {
            continue;
        }
        out.push(line);
    }
    let mut s = out.join("\n");
    if text.ends_with('\n') {
        s.push('\n');
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_keys_are_the_games() {
        let src = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../engine/nvf-play/src/game_start.rs")).unwrap();
        for line in ["pub const SECTION: &str = \"General\";", "pub const CHARGEN_KEY: &str = \"sCharGenQuest\";", "pub const INTRO_KEY: &str = \"sIntroMovie\";"] {
            assert!(src.contains(line), "game_start.rs no longer has {line}");
        }
        // The game's own test reads TTW's lines as its quest and no film.
        assert!(src.contains("SCharGenQuest=001FFFF8\\nSIntroMovie=\\n"));
    }

    #[test]
    fn read_and_write() {
        let ini = "[Game]\nPath=F:\\FNV\n\n[Settings]\ngrass=1\n";
        assert_eq!(Fields::read(ini), Fields::default());
        let on = Fields { quest: "001FFFF8".into(), film: String::new(), no_film: true }.write(ini);
        assert_eq!(ini_get(&on, "General", "sCharGenQuest").as_deref(), Some("001FFFF8"));
        assert_eq!(ini_get(&on, "General", "sIntroMovie").as_deref(), Some(""));
        assert_eq!(Fields::read(&on), Fields { quest: "001FFFF8".into(), film: String::new(), no_film: true });
        // Emptied: both lines go, every other line stays.
        let off = Fields::default().write(&on);
        assert_eq!(ini_get(&off, "General", "sCharGenQuest"), None);
        assert_eq!(ini_get(&off, "General", "sIntroMovie"), None);
        assert_eq!(ini_get(&off, "Settings", "grass").as_deref(), Some("1"));
        assert_eq!(ini_get(&off, "Game", "Path").as_deref(), Some("F:\\FNV"));
        // Another mod's start and film, typed with spaces around.
        let other = Fields { quest: " MyStart ".into(), film: "Mine.bik ".into(), no_film: false }.write(ini);
        assert_eq!(Fields::read(&other), Fields { quest: "MyStart".into(), film: "Mine.bik".into(), no_film: false });
        let none = Fields { quest: String::new(), film: "Mine.bik".into(), no_film: true }.write(ini);
        assert_eq!(ini_get(&none, "General", "sIntroMovie").as_deref(), Some(""));
        let quoted = Fields::read("[general]\nSCharGenQuest=\"001fffF8\"\nSIntroMovie=\n");
        assert_eq!(quoted.quest, "001fffF8");
        assert!(quoted.no_film);
        // A comment naming the key is not a key.
        assert!(Fields::default().write("[General]\n; sCharGenQuest=old\nsCharGenQuest=001FFFF8\n").contains("; sCharGenQuest=old"));
    }
}
