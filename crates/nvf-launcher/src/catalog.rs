//! The settings page's rows as the launcher shows them: key, label, tab, heading and note,
//! read from `docs/settings-reference.md`, built into the launcher (the reference lists the
//! rows in the page's order with the page's own notes, and a test keeps it the same as the
//! game's registry). What kind of row a key is (on/off, slider, list) the reference does not
//! say: the launcher goes by the value the INI holds ([`Kind::of_value`]).

use std::collections::HashMap;

const REFERENCE: &str = include_str!("../../../docs/settings-reference.md");

/// The heading after which the reference lists the Controls tab's own rows (kept in
/// `[Controls]`, not `[Settings]`): the launcher stops there.
const OWN_ROWS: &str = "the tab's own rows";

/// One row of the settings page.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub tab: String,
    pub heading: Option<String>,
    pub note: String,
}

/// Every row of the reference, in the page's order.
pub fn rows() -> Vec<Row> {
    parse(REFERENCE)
}

/// The rows of `text` (the reference's Markdown).
pub fn parse(text: &str) -> Vec<Row> {
    let mut out: Vec<Row> = Vec::new();
    let (mut tab, mut heading) = (String::new(), None::<String>);
    let mut in_note = false;
    for line in text.lines() {
        if let Some(h) = line.strip_prefix("## ") {
            if h.contains(OWN_ROWS) {
                break;
            }
            let mut parts = h.split('›').map(str::trim);
            tab = parts.next().unwrap_or_default().to_string();
            heading = parts.next().map(str::to_string);
            in_note = false;
        } else if let Some(h) = line.strip_prefix("### `") {
            in_note = false;
            if let Some((key, rest)) = h.split_once('`') {
                let label = rest.trim_start_matches(':').trim().to_string();
                out.push(Row { key: key.to_string(), label, tab: tab.clone(), heading: heading.clone(), note: String::new() });
            }
        } else if let Some(n) = line.strip_prefix("- On the page:") {
            if let Some(r) = out.last_mut() {
                r.note = n.trim().to_string();
                in_note = true;
            }
        } else if in_note && line.starts_with("  ") && !line.trim_start().starts_with("- ") {
            if let Some(r) = out.last_mut() {
                r.note.push(' ');
                r.note.push_str(line.trim());
            }
        } else {
            in_note = false;
        }
    }
    out
}

/// How a row is changed, from the text its INI line holds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// `key=1` / `key=0`.
    Switch,
    /// A part of a full amount, `key=0.50`.
    Slider,
    /// Anything else: a list's choice by its own text (`resolution=1600x900;desktop`).
    Text,
}

impl Kind {
    pub fn of_value(v: &str) -> Kind {
        match v.trim() {
            "0" | "1" => Kind::Switch,
            t if t.contains('.') && t.parse::<f32>().is_ok() => Kind::Slider,
            _ => Kind::Text,
        }
    }
}

/// Sliders whose steps the page shows as words, not a percentage: the words for each step,
/// first to last (the step `i` of `n` is kept as `i / (n - 1)`, `Slider::choice` in
/// engine/nvf-play/src/settings.rs). Words are NVForever's own (the page's).
pub fn step_words() -> HashMap<&'static str, &'static [&'static str]> {
    HashMap::from([("difficulty", &["Very Easy", "Easy", "Normal", "Hard", "Very Hard"][..])])
}

/// List rows whose choices are fixed, by their names as the INI keeps them (each list's
/// `name()` in the engine; a test checks the engine still has every name). Other lists
/// (the resolution, the monitor) depend on the machine and stay text.
pub fn list_choices() -> HashMap<&'static str, &'static [&'static str]> {
    HashMap::from([("kill-camera", &["Last enemy", "Every kill", "Off"][..]), ("vats-style", &["Classic", "Mark and Fire"][..])])
}

/// The tabs in the page's order (engine/nvf-play/src/settings_layout.rs, `TAB_ORDER`), then
/// any other tab the reference names, in its order.
pub fn tabs(rows: &[Row]) -> Vec<String> {
    let mut out: Vec<String> = ["Gameplay", "Display", "Audio", "Controls", "Graphics", "Tweaks", "Debug"].iter().map(|s| s.to_string()).collect();
    for r in rows {
        if !out.contains(&r.tab) {
            out.push(r.tab.clone());
        }
    }
    out.retain(|t| rows.iter().any(|r| &r.tab == t));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_rows_tabs_headings_and_notes() {
        let t = "# S\n\n## Gameplay › Game\n\n### `difficulty`: Difficulty\n\n- On the page: How hard the\n  fights are.\n- In full: more\n  words.\n- Registered in x\n\n## Audio\n\n### `music`: Music\n\n- On the page: Plays it.\n\n## Controls › the tab's own rows\n\n### `x`: X\n";
        let r = parse(t);
        assert_eq!(r.len(), 2);
        assert_eq!(r[0], Row { key: "difficulty".into(), label: "Difficulty".into(), tab: "Gameplay".into(), heading: Some("Game".into()), note: "How hard the fights are.".into() });
        assert_eq!((r[1].tab.as_str(), r[1].heading.as_deref(), r[1].note.as_str()), ("Audio", None, "Plays it."));
    }

    #[test]
    fn the_built_in_reference_has_the_pages_rows() {
        let r = rows();
        assert!(r.len() > 50, "{} rows", r.len());
        for key in ["difficulty", "menu-pause", "resolution", "borderless"] {
            let row = r.iter().find(|x| x.key == key).unwrap_or_else(|| panic!("{key} missing"));
            assert!(!row.note.is_empty() && !row.label.is_empty(), "{row:?}");
        }
        assert!(tabs(&r).starts_with(&["Gameplay".to_string(), "Display".to_string()]));
    }

    #[test]
    fn list_choices_match_the_engines_names() {
        let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../../engine/nvf-play/src/");
        for (key, file) in [("kill-camera", "kill_cam.rs"), ("vats-style", "vats_classic.rs")] {
            let src = std::fs::read_to_string(format!("{root}{file}")).unwrap();
            for name in list_choices()[key] {
                assert!(src.contains(&format!("=> \"{name}\"")), "{key}: {name} is no longer a choice in {file}");
            }
        }
    }

    #[test]
    fn kinds_from_values() {
        assert_eq!(Kind::of_value("1"), Kind::Switch);
        assert_eq!(Kind::of_value("0.50"), Kind::Slider);
        assert_eq!(Kind::of_value("1920x1080;desktop"), Kind::Text);
        assert_eq!(Kind::of_value("2"), Kind::Text);
    }
}
