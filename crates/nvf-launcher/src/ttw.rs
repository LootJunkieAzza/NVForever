//! Tale of Two Wastelands' start, as a box on the Mods page: TTW asks for two lines in the
//! game's INI (`[General] SCharGenQuest=001FFFF8`, `SIntroMovie=`), which NVForever reads
//! from its own `NVForever.ini` (docs/design/install-layout.md; nvf-play game_start.rs).
//! The box writes them, or takes them out again, so the player need not edit the file.

use fnv_install::{ini_get, ini_set};

/// TTW's master; the box shows only while the load order has it.
pub const PLUGIN: &str = "TaleOfTwoWastelands.esm";
/// The section and keys the game reads (nvf-play game_start.rs `SECTION`, `CHARGEN_KEY`,
/// `INTRO_KEY`).
pub const SECTION: &str = "General";
pub const QUEST_KEY: &str = "sCharGenQuest";
pub const INTRO_KEY: &str = "sIntroMovie";
/// TTW's start quest, as its installation guide writes it.
pub const START_QUEST: &str = "001FFFF8";

fn quest(text: &str) -> Option<String> {
    ini_get(text, SECTION, QUEST_KEY).map(|v| v.trim().trim_matches('"').to_string()).filter(|v| !v.is_empty())
}

/// Whether New Game starts TTW's quest.
pub fn is_on(text: &str) -> bool {
    quest(text).is_some_and(|q| q.eq_ignore_ascii_case(START_QUEST))
}

/// Another start quest the INI names (a different mod's), if any.
pub fn other_start(text: &str) -> Option<String> {
    quest(text).filter(|q| !q.eq_ignore_ascii_case(START_QUEST))
}

/// TTW's two lines set (any earlier start quest or film replaced).
pub fn turn_on(text: &str) -> String {
    ini_set(&ini_set(text, SECTION, QUEST_KEY, START_QUEST), SECTION, INTRO_KEY, "")
}

/// TTW's two lines taken out (New Game is the New Vegas opening and its film again); a start
/// quest that is not TTW's is left alone.
pub fn turn_off(text: &str) -> String {
    if !is_on(text) {
        return text.to_string();
    }
    let t = ini_remove(text, SECTION, QUEST_KEY);
    match ini_get(&t, SECTION, INTRO_KEY) {
        Some(v) if v.trim().trim_matches('"').is_empty() => ini_remove(&t, SECTION, INTRO_KEY),
        _ => t,
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
    fn on_and_off() {
        let ini = "[Game]\nPath=F:\\FNV\n\n[Settings]\ngrass=1\n";
        let on = turn_on(ini);
        assert!(is_on(&on));
        assert_eq!(ini_get(&on, "General", "sIntroMovie").as_deref(), Some(""));
        assert_eq!(other_start(&on), None);
        let off = turn_off(&on);
        assert!(!is_on(&off));
        assert_eq!(ini_get(&off, "General", "sCharGenQuest"), None);
        assert_eq!(ini_get(&off, "General", "sIntroMovie"), None);
        assert_eq!(ini_get(&off, "Settings", "grass").as_deref(), Some("1"));
        assert_eq!(ini_get(&off, "Game", "Path").as_deref(), Some("F:\\FNV"));
        // Lines copied in by hand, any case, count; another mod's start is left alone.
        assert!(is_on("[general]\nSCharGenQuest=\"001fffF8\"\nSIntroMovie=\n"));
        let other = "[General]\nsCharGenQuest=MyStart\nsIntroMovie=Mine.bik\n";
        assert_eq!(other_start(other).as_deref(), Some("MyStart"));
        assert_eq!(turn_off(other), other);
        assert!(is_on(&turn_on(other)) && ini_get(&turn_on(other), "General", "sIntroMovie").as_deref() == Some(""));
        // A film the player set by hand stays when the box goes off.
        let kept = turn_off("[General]\nsCharGenQuest=001FFFF8\nsIntroMovie=Mine.bik\n");
        assert_eq!(ini_get(&kept, "General", "sIntroMovie").as_deref(), Some("Mine.bik"));
        // A comment naming the key is not a key.
        assert!(turn_off("[General]\n; sCharGenQuest=old\nsCharGenQuest=001FFFF8\n").contains("; sCharGenQuest=old"));
    }
}
