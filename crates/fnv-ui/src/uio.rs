//! [design] UIO data-list reader and condition evaluation, from menu-xml.md §7 item 3.
//! [observed: UIO - User Interface Organizer/uio/supported.txt,
//! MCM Extender/uio/Public/MCMExtender.txt]
//! Lists pair an include/target line with a condition; no plugin code is consulted.

use crate::{xml, Source};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    True,
    Plugin(String),
    File(String),
    Unknown(String),
    Not(Box<Condition>),
    And(Vec<Condition>),
    Or(Vec<Condition>),
}

impl Condition {
    pub fn parse(text: &str) -> Result<Self, String> {
        let text = text.trim();
        if text.contains("||") {
            return text.split("||").map(Self::parse).collect::<Result<_, _>>().map(Self::Or);
        }
        if text.contains("&&") {
            return text.split("&&").map(Self::parse).collect::<Result<_, _>>().map(Self::And);
        }
        if let Some(rest) = text.strip_prefix('!') {
            return Self::parse(rest).map(Box::new).map(Self::Not);
        }
        if text.is_empty() || text.contains(['|', ':', '!']) {
            return Err(format!("malformed condition {text:?}"));
        }
        if text.eq_ignore_ascii_case("true") {
            Ok(Self::True)
        } else if let Some(name) = text.strip_prefix('?').filter(|s| !s.trim().is_empty()) {
            Ok(Self::Plugin(name.trim().to_ascii_lowercase()))
        } else if let Some(path) = text.strip_prefix('#').filter(|s| !s.trim().is_empty()) {
            Ok(Self::File(normalize(path.trim())))
        } else if text.contains(['?', '#', '&']) || text.split_whitespace().count() != 1 {
            Err(format!("malformed condition {text:?}"))
        } else {
            Ok(Self::Unknown(text.to_ascii_lowercase()))
        }
    }

    pub fn evaluate(&self, src: &dyn Source, plugin: &dyn Fn(&str) -> bool, unknown: &mut Vec<String>) -> bool {
        match self {
            Self::True => true,
            Self::Plugin(name) => plugin(name),
            // [hypothesis] # uses the include base, consistent with #UII\PerkBrowser.xml
            // in Pip-Boy UI Tweaks/UIO/Public/PipBoyUITweaks.txt and the file under
            // UI Improvements/menus/prefabs/UII (menu-xml.md §7 item 3).
            Self::File(path) => src.read(&prefab_path(path)).is_some(),
            Self::Unknown(name) => {
                unknown.push(name.clone());
                false
            }
            Self::Not(c) => !c.evaluate(src, plugin, unknown),
            // [design] Evaluate every atom so unknown words are reported even in a false branch.
            Self::And(cs) => {
                let mut value = true;
                for c in cs {
                    value &= c.evaluate(src, plugin, unknown);
                }
                value
            }
            Self::Or(cs) => {
                let mut value = false;
                for c in cs {
                    value |= c.evaluate(src, plugin, unknown);
                }
                value
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Target {
    Menu(String),
    Prefab(String),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub list: String,
    pub line: usize,
    pub include: String,
    pub target: Target,
    pub tile: Option<String>,
    pub condition: Condition,
}

#[derive(Default, Debug)]
pub struct Lists {
    pub entries: Vec<Entry>,
    pub warnings: Vec<String>,
    pub duplicates: usize,
}

impl Lists {
    /// [design] supported.txt first, public lists alphabetically; normalized duplicate
    /// include/target/tile/condition tuples apply once, keeping the first location.
    pub fn read(src: &dyn Source) -> Self {
        let mut paths: Vec<_> = src
            .list("uio/public")
            .into_iter()
            .map(|p| normalize(&p))
            .filter(|p| p.strip_prefix("uio\\public\\").is_some_and(|p| !p.contains('\\') && p.ends_with(".txt")))
            .collect();
        paths.sort();
        paths.dedup();
        if src.read("uio/supported.txt").is_some() {
            paths.insert(0, "uio\\supported.txt".into());
        }
        let mut out = Self::default();
        let mut seen = HashSet::new();
        for path in paths {
            let Some(bytes) = src.read(&path) else {
                out.warnings.push(format!("{path}: list unreadable"));
                continue;
            };
            let parsed = Self::parse(&path, &xml::decode(&bytes));
            out.warnings.extend(parsed.warnings);
            for e in parsed.entries {
                if seen.insert((e.include.clone(), e.target.clone(), e.tile.clone(), e.condition.clone())) {
                    out.entries.push(e);
                } else {
                    out.duplicates += 1;
                }
            }
        }
        out
    }

    pub fn parse(path: &str, text: &str) -> Self {
        let mut out = Self::default();
        let lines: Vec<_> = text
            .trim_start_matches('\u{feff}')
            .lines()
            .enumerate()
            .map(|(i, s)| (i + 1, s.trim()))
            .filter(|(_, s)| !s.is_empty() && !s.starts_with(';'))
            .collect();
        let mut i = 0;
        while i < lines.len() {
            let (line, text) = lines[i];
            i += 1;
            let result: Result<Entry, String> = (|| {
                let delimiter = if text.contains("::") { "::" } else { ":" };
                let parts: Vec<_> = text.split(delimiter).map(str::trim).collect();
                if !(2..=3).contains(&parts.len()) || parts.iter().any(|s| s.is_empty()) || !parts[0].to_ascii_lowercase().ends_with(".xml") {
                    return Err("malformed include/target".into());
                }
                let Some(&(_, condition)) = lines.get(i) else {
                    return Err("condition missing".into());
                };
                // [design] A following entry is retained when this one's condition is missing.
                if condition.contains(':') {
                    return Err("condition missing".into());
                }
                i += 1;
                let condition = Condition::parse(condition)?;
                let target = normalize(parts[1]);
                let target = if let Some(p) = target.strip_prefix("@prefabs\\").filter(|s| !s.is_empty()) {
                    Target::Prefab(prefab_path(p))
                } else if target.starts_with('@') {
                    return Err("unknown prefab target".into());
                } else {
                    Target::Menu(target)
                };
                Ok(Entry {
                    list: path.into(),
                    line,
                    include: normalize(parts[0]),
                    target,
                    tile: parts.get(2).map(|s| s.to_ascii_lowercase()),
                    condition,
                })
            })();
            match result {
                Ok(e) => out.entries.push(e),
                Err(e) => out.warnings.push(format!("{path}: line {line}: {e}")),
            }
        }
        out
    }
}

#[derive(Clone, Debug)]
pub struct Decision {
    pub entry: Entry,
    pub enabled: bool,
    pub applied: usize,
    pub reason: String,
}

#[derive(Default, Debug)]
pub struct Report {
    pub decisions: Vec<Decision>,
    pub duplicates: usize,
    pub warnings: Vec<String>,
}

impl Report {
    pub fn read(src: &dyn Source, plugin: &dyn Fn(&str) -> bool) -> Self {
        let lists = Lists::read(src);
        let mut out = Self { duplicates: lists.duplicates, warnings: lists.warnings, ..Self::default() };
        for entry in lists.entries {
            let mut unknown = Vec::new();
            let holds = entry.condition.evaluate(src, plugin, &mut unknown);
            unknown.sort();
            unknown.dedup();
            for word in unknown {
                out.warnings.push(format!("{}: line {}: unknown condition {word:?}; false [hypothesis]", entry.list, entry.line));
            }
            let (enabled, reason) = if !holds {
                (false, "condition false")
            } else if src.read(&prefab_path(&entry.include)).is_none() {
                (false, "file missing")
            } else {
                (true, "target missing")
            };
            out.decisions.push(Decision { entry, enabled, applied: 0, reason: reason.into() });
        }
        out
    }

    pub fn applied(&self) -> usize {
        self.decisions.iter().filter(|d| d.applied > 0).count()
    }

    pub fn lines(&self) -> Vec<String> {
        let mut lines = self.warnings.clone();
        lines.extend(self.decisions.iter().map(|d| {
            format!(
                "{}: line {}: {} -> {:?}{}: {} ({} applications)",
                d.entry.list,
                d.entry.line,
                d.entry.include,
                d.entry.target,
                d.entry.tile.as_ref().map_or(String::new(), |s| format!("::{s}")),
                d.reason,
                d.applied
            )
        }));
        lines
    }
}

pub(crate) fn normalize(path: &str) -> String {
    let path = path.replace('/', "\\").to_ascii_lowercase();
    let mut parts = Vec::new();
    for part in path.split('\\') {
        match part {
            "" | "." => {}
            ".." if parts.last().is_some_and(|p| *p != "..") => {
                parts.pop();
            }
            _ => parts.push(part),
        }
    }
    parts.join("\\")
}

pub(crate) fn prefab_path(path: &str) -> String {
    normalize(&format!("menus\\prefabs\\{path}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{MemSource, Origin, Ui};

    #[test]
    fn real_lines_and_both_delimiters() {
        // [observed: UIO - User Interface Organizer/uio/supported.txt]
        let old = Lists::parse("supported.txt", "wmm\\wmm.xml:inventorymenu:glow_branch\n?The Weapon Mod Menu.esp\nmcm\\mcm.xml:startmenu\n?The Mod Configuration Menu.esp\t||\t?Project Nevada - Core.esm");
        assert!(old.warnings.is_empty());
        assert_eq!(old.entries[0].tile.as_deref(), Some("glow_branch"));
        assert!(matches!(old.entries[1].condition, Condition::Or(_)));
        // [observed: UIO - User Interface Organizer/uio/supported.txt]
        assert_eq!(
            Condition::parse("?JIP Companions Command & Control.esp").unwrap(),
            Condition::Plugin("jip companions command & control.esp".into())
        );
        // [observed: Just Assorted Mods/uio/public/JHM.txt;
        // MCM Extender/UIO/Public/MCMExtender.txt; UI Improvements/uio/public/UII.txt]
        let new = Lists::parse("public.txt", "JHM\\JHM.xml::HUDMainMenu\ntrue\nMCMExtender\\ListRect\\text.xml::@Prefabs\\MCM\\ListRect.xml::text\ntrue\nUII\\CharGenBox.xml::CharGenMenu::TM_DescriptionBox\ntrue");
        assert!(new.warnings.is_empty());
        assert_eq!(new.entries.len(), 3);
        assert_eq!(new.entries[1].target, Target::Prefab("menus\\prefabs\\mcm\\listrect.xml".into()));
    }

    #[test]
    fn forgiving_pairs_comments_bom_and_recovery() {
        // [design] Malformed synthetic inputs retain subsequent valid entries.
        let lists = Lists::parse(
            "test.txt",
            "\u{feff}; comment\r\nbad entry\r\ntrue\r\nx.xml::HUD\r\ny.xml::HUD\r\ntrue\r\nz.xml::HUD\r\n?\r\nw.xml::HUD\r\n",
        );
        assert_eq!(lists.entries.len(), 1);
        assert_eq!(lists.entries[0].include, "y.xml");
        assert_eq!(lists.warnings.len(), 5);
    }

    #[test]
    fn conditions_plugins_files_negation_and_unknowns() {
        // [observed: UIO - User Interface Organizer/uio/supported.txt]
        let real = Condition::parse("?Jetpack.esp\t&&\t!darnui").unwrap();
        let src = MemSource::default().with("menus/prefabs/probe.xml", "");
        let mut unknown = Vec::new();
        assert!(real.evaluate(&src, &|n| n == "jetpack.esp", &mut unknown));
        assert_eq!(unknown, ["darnui"]);
        for (condition, expected) in [
            ("#probe.xml && !?Absent.esp", true),
            ("#missing.xml", false),
            ("?Absent.esp || #probe.xml && true", true),
            ("!#probe.xml", false),
            ("darnui", false),
            ("!?Jetpack.esp", false),
        ] {
            assert_eq!(Condition::parse(condition).unwrap().evaluate(&src, &|n| n == "jetpack.esp", &mut Vec::new()), expected);
        }
        assert_eq!(prefab_path("../ySI/ySI.xml"), "menus\\ysi\\ysi.xml");
    }

    fn fixture() -> MemSource {
        // [design] Synthetic XML exercises menu, included prefab and deferred templates.
        MemSource::default()
            .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Existing"/><include src="base.xml"/></menu>"#)
            .with(
                "menus/prefabs/base.xml",
                r#"<rect name="Container"/><template name="Row"><rect name="row"><include src="row.xml"/></rect></template>"#,
            )
            .with("menus/prefabs/row.xml", r#"<text name="label"><string>old</string></text>"#)
            .with_loose("menus/prefabs/added.xml", r#"<rect name="Existing"><include src="child.xml"/></rect>"#)
            .with("menus/prefabs/child.xml", r#"<text name="Child"/>"#)
            .with_loose("menus/prefabs/change.xml", "<string>patched</string>")
    }

    #[test]
    fn stable_lists_deduplicate_and_report_reasons() {
        let src = fixture()
            .with("UIO/supported.txt", "added.xml:hudmainmenu\ntrue")
            .with("UIO/Public/z.txt", "added.xml::HUDMAINMENU\ntrue\nabsent.xml::HUDMainMenu\ntrue\nadded.xml::MissingMenu\ntrue")
            .with("uio/public/a.txt", "added.xml::HUDMainMenu::Container\n?JustAssortedMods.esp\nadded.xml::HUDMainMenu::NotHere\ntrue\nadded.xml::HUDMainMenu\n?Absent.esp && darnui")
            .with("uio/public/deeper/ignored.txt", "added.xml::HUDMainMenu\ntrue");
        let mut ui = Ui::new();
        ui.configure_uio(&src, |n| n.eq_ignore_ascii_case("JustAssortedMods.esp"));
        assert_eq!(ui.uio.duplicates, 1);
        assert!(ui.uio.decisions[1].entry.list.ends_with("a.txt"));
        assert_eq!(ui.uio.warnings.len(), 1);
        let root = ui.load_menu(&src, "menus/hud.xml").unwrap();
        assert_eq!(ui.uio.applied(), 2);
        assert_eq!(
            ui.uio.decisions.iter().map(|d| d.reason.as_str()).collect::<Vec<_>>(),
            ["applied", "applied", "target missing", "condition false", "file missing", "target missing"]
        );
        assert_eq!(ui.tile(*ui.tile(root).children.last().unwrap()).name, "Existing");
        let patched = ui.find_tile("HUDMainMenu/Container/Existing").unwrap();
        assert_eq!(ui.tile(patched).origin, Origin::File { path: "menus\\prefabs\\added.xml".into(), loose: true });
        // [design] Both collision with a vanilla name and unavailable archived names
        // must allow a loose UIO addition and its ordinary include descendants to draw.
        let names = Ui::file_name_paths(&src, "menus/hud.xml").unwrap();
        let added = *ui.tile(root).children.last().unwrap();
        assert!(ui.mod_tile_drawn(added, Some(&names)));
        assert!(ui.mod_tile_drawn(added, None));
        assert!(ui.mod_tile_drawn(ui.tile(added).children[0], None));
        assert!(!ui.mod_tile_drawn(root, None));
        ui.load_menu(&src, "menus/hud.xml").unwrap();
        assert_eq!(ui.uio.decisions[0].applied, 2);
        assert!(!ui.tile(root).live);
    }

    #[test]
    fn prefab_named_targets_whole_fragments_and_templates() {
        let src = fixture().with(
            "uio/public/test.txt",
            "added.xml::@Prefabs\\base.xml::Container\ntrue\nchange.xml::@Prefabs\\row.xml::label\ntrue\nadded.xml::@Prefabs\\base.xml\ntrue",
        );
        let mut ui = Ui::new();
        ui.configure_uio(&src, |_| false);
        ui.load_menu(&src, "menus/hud.xml").unwrap();
        assert_eq!(ui.uio.applied(), 3);
        assert!(ui.find_tile("HUDMainMenu/Container/Existing/Child").is_some());
        ui.add_from_template(&src, "HUDMainMenu", "Row", Some("Made")).unwrap();
        assert_eq!(ui.get_string("HUDMainMenu/Made/label/string").as_deref(), Some("patched"));
    }

    #[test]
    fn recursive_uio_includes_are_bounded_and_archived_ones_stay_hidden() {
        let src = MemSource::default()
            .with("menus/hud.xml", "<menu name=\"HUD\"/>")
            .with("menus/prefabs/cycle.xml", "<rect name=\"Cycle\"/>")
            .with("uio/public/test.txt", "cycle.xml::HUD\ntrue\ncycle.xml::@Prefabs\\cycle.xml\ntrue");
        let mut ui = Ui::new();
        ui.configure_uio(&src, |_| false);
        let root = ui.load_menu(&src, "menus/hud.xml").unwrap();
        assert!(ui.warnings.iter().any(|w| w.contains("nested too deep")));
        assert!(ui.mod_added_tiles(root).is_empty());
        assert!(!ui.mod_tile_drawn(ui.tile(root).children[0], None));
    }

    #[test]
    fn renderer_tracks_the_addition_not_every_use_of_its_filename() {
        // [design] A loose prefab may be included normally and by UIO in the same menu.
        let src = MemSource::default()
            .with("menus/hud.xml", "<menu name=\"HUD\"><include src=\"same.xml\"/></menu>")
            .with_loose("menus/prefabs/same.xml", "<include src=\"archived.xml\"/>")
            .with("menus/prefabs/archived.xml", "<rect name=\"Same\"/>")
            .with("uio/public/test.txt", "same.xml::HUD\ntrue");
        let mut ui = Ui::new();
        ui.configure_uio(&src, |_| false);
        let root = ui.load_menu(&src, "menus/hud.xml").unwrap();
        let children = &ui.tile(root).children;
        assert_eq!(children.len(), 2);
        assert!(!ui.mod_tile_drawn(children[0], None));
        assert!(ui.mod_tile_drawn(children[1], None));
        assert_eq!(ui.mod_added_tiles(root), [children[1]]);
        assert!(matches!(ui.tile(children[1]).origin, Origin::File { loose: false, .. }));
    }
}
