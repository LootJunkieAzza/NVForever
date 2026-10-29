//! The game strings table: the text of `&-sName;`, the game setting `sName`
//! (docs/spec-notes/menu-xml.md §2.4; task T10).
//!
//! Two places answer a name, in this order:
//!
//! 1. the load order's string game settings (GMST records, the last plugin's record
//!    winning), read from the owner's plugins at run time and never shipped;
//! 2. [`BUILT_IN`]: the original's own default text for a setting no plugin defines
//!    [observed: 144 of the 145 names of the vanilla menu files have no GMST record in
//!    FalloutNV.esm or the nine official add-ons, 2026-10-29]. It is filled only from
//!    observer readings of the original (`GetStringGameSetting`, menu-xml.md §2.4), and is
//!    empty until they exist.
//!
//! A name in neither has no text: the reader puts in nothing (menu-xml.md §2.4).
//! Names are compared without case [hypothesis: the vanilla files spell `sOK` and `sOk`,
//! `sQuit` and `squit`; Editor IDs are found without case elsewhere in the engine].

use crate::xml::Node;
use std::collections::{BTreeSet, HashMap};

/// The original's default text for the settings no plugin defines, by setting name. Empty
/// until the observer readings of menu-xml.md §2.4 exist; never filled from guesses.
pub const BUILT_IN: &[(&str, &str)] = &[];

/// Where a name's text came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    /// A plugin's GMST record.
    LoadOrder,
    /// [`BUILT_IN`] (or the table a test passes).
    BuiltIn,
}

/// The table one tree reads from: the load order's string settings over a built-in table.
#[derive(Clone, Debug, Default)]
pub struct GameStrings {
    /// By lower-case name.
    settings: HashMap<String, String>,
    built_in: &'static [(&'static str, &'static str)],
}

impl GameStrings {
    /// The load order's string settings, in load order (a later one of the same name
    /// replaces an earlier, as a later plugin's record wins), over [`BUILT_IN`].
    pub fn from_settings<N: AsRef<str>, V: Into<String>>(settings: impl IntoIterator<Item = (N, V)>) -> GameStrings {
        GameStrings::with_built_in(settings, BUILT_IN)
    }

    /// As [`GameStrings::from_settings`] over another built-in table (tests).
    pub fn with_built_in<N: AsRef<str>, V: Into<String>>(
        settings: impl IntoIterator<Item = (N, V)>,
        built_in: &'static [(&'static str, &'static str)],
    ) -> GameStrings {
        let settings = settings.into_iter().map(|(n, v)| (n.as_ref().to_ascii_lowercase(), v.into())).collect();
        GameStrings { settings, built_in }
    }

    /// The text of `&-name;` and where it came from, or `None` when neither the load
    /// order nor the built-in table has the name.
    pub fn lookup(&self, name: &str) -> Option<(&str, Origin)> {
        if let Some(v) = self.settings.get(&name.to_ascii_lowercase()) {
            return Some((v, Origin::LoadOrder));
        }
        self.built_in.iter().find(|(n, _)| n.eq_ignore_ascii_case(name)).map(|&(_, v)| (v, Origin::BuiltIn))
    }

    /// The text of `&-name;`, if any.
    pub fn get(&self, name: &str) -> Option<String> {
        self.lookup(name).map(|(v, _)| v.to_owned())
    }
}

/// The game string names (`sName` of `&-sName;`) a file's nodes use, as written.
pub fn names_in(nodes: &[Node]) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack: Vec<&[Node]> = vec![nodes];
    while let Some(nodes) = stack.pop() {
        for n in nodes {
            match n {
                Node::Entity(e) => {
                    if let Some(name) = e.strip_prefix('-') {
                        out.insert(name.to_owned());
                    }
                }
                Node::Element(e) => stack.push(&e.children),
                Node::Text(_) => {}
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    // Synthetic names and text only: no game text in tests (charter).
    const TABLE: &[(&str, &str)] = &[("sNvfBuiltOnly", "built one"), ("sNvfBoth", "built two")];

    #[test]
    fn load_order_first_then_built_in_last_plugin_wins() {
        let s = GameStrings::with_built_in([("sNvfBoth", "first plugin"), ("SNVFBOTH", "last plugin"), ("sNvfPlugin", "p")], TABLE);
        assert_eq!(s.lookup("snvfboth"), Some(("last plugin", Origin::LoadOrder)));
        assert_eq!(s.lookup("sNvfBuiltOnly"), Some(("built one", Origin::BuiltIn)));
        assert_eq!(s.get("SNVFPLUGIN").as_deref(), Some("p"));
        assert_eq!(s.lookup("sNvfNowhere"), None);
        assert_eq!(GameStrings::default().get("sNvfBoth"), None);
    }

    #[test]
    fn the_shipped_table_is_empty_until_readings_exist() {
        // menu-xml.md §2.4: the built-in text comes only from observer readings.
        assert!(BUILT_IN.is_empty());
        assert_eq!(GameStrings::from_settings([("sNvfA", "a")]).lookup("sNvfA"), Some(("a", Origin::LoadOrder)));
    }

    #[test]
    fn names_in_a_file() {
        let nodes = crate::xml::parse(
            "<menu name=\"M\"><text name=\"t\"><string>&-sNvfOne; and &-sNvfTwo;</string><x><copy>&-sNvfOne;</copy></x></text><visible>&true;</visible></menu>",
        )
        .nodes;
        let names: Vec<_> = names_in(&nodes).into_iter().collect();
        assert_eq!(names, ["sNvfOne", "sNvfTwo"]);
    }
}
