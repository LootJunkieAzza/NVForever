//! The settings page as the game lays it out in a run, written beside `NVForever.ini`
//! ([`MENU_FILE`]) once every row is registered, for the launcher [design: owner
//! 2026-11-16: the launcher's settings must always be the game's, so it never names a menu
//! the game does not have]. The game writes it from its own registry and the table in this
//! crate (`engine/nvf-play/src/settings_menu.rs`); the launcher shows it while its
//! [`Menu::version`] is the launcher's own (an update brings a new game that has not run
//! yet: the launcher then shows its built-in copy of the table, `crates/nvf-launcher/src/catalog.rs`).
//!
//! The words are as the page shows them in the game's language (Title Case in English). The
//! Controls tab's own rows (bindings, sensitivities, kept in `[Controls]`) are not in it: the
//! launcher changes `[Settings]` only.

use serde::{Deserialize, Serialize};

/// The file's name, beside `NVForever.ini`.
pub const MENU_FILE: &str = "settings-menu.json";

/// The page: its tabs in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Menu {
    /// The game's version (the workspace's), which the launcher compares with its own.
    pub version: String,
    pub tabs: Vec<Tab>,
}

/// A tab with rows (a tab with none is left out, as the launcher shows none).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Tab {
    /// Its name in the table (`crate::TAB_ORDER`, English), what both menus go by.
    pub name: String,
    /// Its name as the page shows it.
    pub label: String,
    pub categories: Vec<Category>,
}

/// A category of a tab: the Tweaks tab's group, its heading (none on a tab of one
/// category), its rows in order.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Category {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading: Option<String>,
    pub rows: Vec<Row>,
}

/// A row: its `[Settings]` key, its label and note as the page shows them, how it changes,
/// and what of other rows it needs (`crate::shown_with`, `crate::needs`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Row {
    pub key: String,
    pub label: String,
    pub note: String,
    pub kind: Kind,
    /// Shown only while this holds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shown_with: Option<Rule>,
    /// Greyed while one of these does not hold (the first one's `why` said).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs: Vec<Rule>,
}

/// What a row needs of another row ([`crate::Rule`]): `parent` (a `[Settings]` key, or
/// [`crate::DEVICE_ROW`]) on when `choices` is empty, else on one of `choices` (by name);
/// `why` in the page's words.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    pub parent: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub choices: Vec<String>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub why: String,
}

impl Rule {
    /// The shared table's rule, its `why` through `words` (the game's translation).
    pub fn of(r: crate::Rule, words: impl Fn(&'static str) -> String) -> Rule {
        Rule { parent: r.parent.to_string(), choices: r.choices.iter().map(|c| c.to_string()).collect(), why: if r.why.is_empty() { String::new() } else { words(r.why) } }
    }
}

/// How a row changes, as the INI keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum Kind {
    /// On or off: `key=1` / `key=0`.
    Switch,
    /// A part of a full amount from `min` to 1 in `step`s (`key=0.50`); `steps`: the words the
    /// page shows for each value it reaches, when it shows words and not a percentage.
    Slider {
        min: f32,
        step: f32,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        steps: Vec<Step>,
    },
    /// A list: its choices as the run had them (a machine's displays, the language files).
    List { choices: Vec<Choice> },
}

/// A value a slider reaches and the words the page shows for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Step {
    pub value: f32,
    pub label: String,
}

/// A list's choice: its name (what the INI keeps for most lists) and its words on the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Choice {
    pub name: String,
    pub label: String,
}

impl Menu {
    /// The menu from the file's text; `None`: not one (an older or a broken file).
    pub fn parse(text: &str) -> Option<Menu> {
        serde_json::from_str(text).ok()
    }

    /// The file's text.
    pub fn to_text(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }

    /// Every row, tab by tab, in the page's order.
    pub fn rows(&self) -> impl Iterator<Item = &Row> {
        self.tabs.iter().flat_map(|t| t.categories.iter().flat_map(|c| c.rows.iter()))
    }
}

impl Step {
    /// The words of the step nearest `v` among `steps`.
    pub fn nearest(steps: &[Step], v: f32) -> Option<&str> {
        steps.iter().min_by(|a, b| (a.value - v).abs().total_cmp(&(b.value - v).abs())).map(|s| s.label.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_reads_back_as_written() {
        let m = Menu {
            version: "0.7.0".into(),
            tabs: vec![Tab {
                name: "Gameplay".into(),
                label: "Gameplay".into(),
                categories: vec![Category {
                    group: None,
                    heading: Some("Game".into()),
                    rows: vec![
                        Row { key: "hardcore".into(), label: "Hardcore Mode".into(), note: "On: …".into(), kind: Kind::Switch, shown_with: None, needs: vec![] },
                        Row { key: "difficulty".into(), label: "Difficulty".into(), note: "".into(), kind: Kind::Slider { min: 0.0, step: 0.25, steps: vec![Step { value: 0.0, label: "Very Easy".into() }, Step { value: 0.25, label: "Easy".into() }] }, shown_with: None, needs: vec![] },
                        Row {
                            key: "vats-slowdown".into(),
                            label: "V.A.T.S. Time Slowdown".into(),
                            note: "".into(),
                            kind: Kind::List { choices: vec![Choice { name: "100% (full stop)".into(), label: "100% (Full Stop)".into() }] },
                            shown_with: None,
                            needs: crate::needs("vats-slowdown").into_iter().map(|r| Rule::of(r, str::to_string)).collect(),
                        },
                    ],
                }],
            }],
        };
        let text = m.to_text();
        assert!(!text.contains("\"group\""), "no group outside the Tweaks tab: {text}");
        assert_eq!(Menu::parse(&text), Some(m.clone()));
        assert_eq!(m.rows().map(|r| r.key.as_str()).collect::<Vec<_>>(), ["hardcore", "difficulty", "vats-slowdown"]);
        assert!(text.contains("\"parent\": \"vats-style\"") && !text.contains("shown_with"), "only the rules a row has: {text}");
        assert_eq!(Menu::parse("{\"version\": 1}"), None);
    }

    #[test]
    fn the_rules_of_a_row() {
        assert_eq!(crate::shown_with("dlss-model").map(|r| (r.parent, r.choices)), Some(("upscaler", &["DLSS"][..])));
        assert_eq!(crate::shown_with("aim-lock").map(|r| (r.parent, r.choices)), Some((crate::DEVICE_ROW, &[crate::CONTROLLER_DEVICE][..])));
        assert_eq!(crate::shown_with("difficulty"), None);
        let lows: Vec<&str> = crate::needs("fps-counter-low").iter().map(|r| r.parent).collect();
        assert_eq!(lows, ["fps-counter", "fps-counter-fps"], "every need, in the table's order");
        assert_eq!(crate::needs("vats-slowdown")[0].choices, ["Classic"]);
        let r = Rule::of(crate::needs("shadow-distance")[0], |w| format!("<{w}>"));
        assert_eq!((r.parent.as_str(), r.choices.len(), r.why.as_str()), ("sun-shadows", 0, "<Needs Sun Shadows.>"));
        // Every rule names placed rows (a Controls row by its table name).
        let placed = |k: &str| crate::SECTIONS.iter().any(|s| s.keys.contains(&k));
        for (row, parent, _) in crate::SHOWN_WITH {
            assert!(placed(row) && placed(parent), "{row} / {parent}");
        }
        for (row, parent, _, _) in crate::NEEDS_CHOICE {
            assert!(placed(row) && placed(parent), "{row} / {parent}");
        }
        assert!(placed(crate::DEVICE_ROW));
    }

    #[test]
    fn the_nearest_steps_words() {
        let s = [Step { value: 0.0, label: "Off".into() }, Step { value: 0.5, label: "Half".into() }, Step { value: 1.0, label: "Full".into() }];
        assert_eq!(Step::nearest(&s, 0.4), Some("Half"));
        assert_eq!(Step::nearest(&s, 0.9), Some("Full"));
        assert_eq!(Step::nearest(&[], 0.9), None);
    }
}
