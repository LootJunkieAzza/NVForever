//! [design] UIO data-list reader and condition evaluation, from menu-xml.md §7 item 3.
//! [observed: UIO - User Interface Organizer/uio/supported.txt,
//! MCM Extender/uio/Public/MCMExtender.txt]
//! Lists pair an include/target line with a condition; no plugin code is consulted.

use crate::{xml, Source};
use std::collections::HashSet;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Condition {
    True,
    /// [design] Literal false condition, avoiding an unknown-condition warning.
    False,
    /// [hypothesis] Built-in UIO condition for Vanilla UI Plus (`vui+`; UIO DLL/source not consulted).
    VuiPlus,
    /// [hypothesis] Built-in UIO condition for DarNified UI (`darn` / `darnui`; UIO DLL/source not consulted).
    DarnUi,
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
        } else if text.eq_ignore_ascii_case("false") {
            Ok(Self::False)
        } else if text.eq_ignore_ascii_case("vui+") {
            Ok(Self::VuiPlus)
        } else if text.eq_ignore_ascii_case("darn") || text.eq_ignore_ascii_case("darnui") {
            Ok(Self::DarnUi)
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
            Self::False => false,
            Self::VuiPlus => has_vui_plus(src, plugin),
            Self::DarnUi => has_darn_ui(src, plugin),
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

/// [hypothesis] Detects Vanilla UI Plus from its active plugin or core VUI+ prefabs
/// observed in public UIO lists (`@Prefabs\VUI+\settings.xml`, `#VUI+\hudcenteredtext.xml`).
/// UIO's DLL and source code were not consulted.
fn has_vui_plus(src: &dyn Source, plugin: &dyn Fn(&str) -> bool) -> bool {
    plugin("vanilla ui plus.esp")
        || src.read(&prefab_path("vui+\\settings.xml")).is_some()
        || src.read(&prefab_path("vui+\\hudcenteredtext.xml")).is_some()
}

/// [hypothesis] Detects DarNified UI from its active plugin (`DarNifiedUINV.esp`),
/// main settings prefab (`DUINVSettings.xml`), or core `darn\text_box.xml` prefab
/// observed in public UIO lists (`@Prefabs\darn\text_box.xml`). Standalone font packs
/// such as `menus\prefabs\DarN_Fonts\*` do not match. UIO's DLL and source code were
/// not consulted.
fn has_darn_ui(src: &dyn Source, plugin: &dyn Fn(&str) -> bool) -> bool {
    plugin("darnifieduinv.esp")
        || src.read(&prefab_path("duinvsettings.xml")).is_some()
        || src.read(&prefab_path("darn\\text_box.xml")).is_some()
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
                // [hypothesis] Some public UIO lists (AimCollisionIndicator.txt, Colorful
                // Icons Support - ySI.txt, ySIAssortedFixes.txt) follow a condition line
                // with a trailing `true` line. Consume at most one trailing `true` line
                // while retaining line 2's condition; UIO DLL/source were not consulted.
                if lines.get(i).is_some_and(|&(_, extra)| extra.eq_ignore_ascii_case("true")) {
                    i += 1;
                }
                let target = normalize(parts[1]);
                let target = if let Some(p) = target.strip_prefix('@') {
                    let Some(resolved) = menus_path(p) else {
                        return Err("unknown prefab target".into());
                    };
                    Target::Prefab(resolved)
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

/// Resolves a UIO `@<path>` target relative to `menus\`, rejecting empty or
/// parent-escaping paths (`@`, `@.`, `@..\...`) [design].
pub(crate) fn menus_path(path: &str) -> Option<String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return None;
    }
    let resolved = normalize(&format!("menus\\{trimmed}"));
    (resolved.starts_with("menus\\") && resolved.len() > "menus\\".len()).then_some(resolved)
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
        assert!(unknown.is_empty(), "darnui is now a recognized built-in condition: {unknown:?}");
        let mut unknown = Vec::new();
        assert!(!Condition::parse("?Absent.esp && unknownui").unwrap().evaluate(&src, &|_| false, &mut unknown));
        assert_eq!(unknown, ["unknownui"]);
        for (condition, expected) in [
            ("#probe.xml && !?Absent.esp", true),
            ("#missing.xml", false),
            ("?Absent.esp || #probe.xml && true", true),
            ("!#probe.xml", false),
            ("darnui", false),
            ("darn", false),
            ("vui+", false),
            ("false", false),
            ("!false", true),
            ("!?Jetpack.esp", false),
        ] {
            let mut u = Vec::new();
            assert_eq!(Condition::parse(condition).unwrap().evaluate(&src, &|n| n == "jetpack.esp", &mut u), expected);
            assert!(u.is_empty(), "condition {condition:?} reported unknown: {u:?}");
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
            .with("uio/public/a.txt", "added.xml::HUDMainMenu::Container\n?JustAssortedMods.esp\nadded.xml::HUDMainMenu::NotHere\ntrue\nadded.xml::HUDMainMenu\n?Absent.esp && unknownui")
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
        assert!(!ui.has_mod_added_tiles(root));
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
        // [design: #938] The UIO tile counts, and no longer once the UIO tiles are forgotten.
        assert!(ui.has_mod_added_tiles(root));
        ui.configure_uio(&src, |_| false);
        assert!(ui.mod_added_tiles(root).is_empty() && !ui.has_mod_added_tiles(root));
    }

    #[test]
    fn menus_relative_at_targets_patch_globals_and_menus_and_reject_escapes() {
        let parsed = Lists::parse(
            "at.txt",
            "preload.xml::@globals.xml\ntrue\n\
             patch.xml::@main\\hud.xml\ntrue\n\
             bad1.xml::@\ntrue\n\
             bad2.xml::@..\ntrue\n\
             bad3.xml::@..\\outside.xml\ntrue\n",
        );
        assert_eq!(parsed.entries.len(), 2);
        assert_eq!(parsed.entries[0].target, Target::Prefab("menus\\globals.xml".into()));
        assert_eq!(parsed.entries[1].target, Target::Prefab("menus\\main\\hud.xml".into()));
        assert_eq!(parsed.warnings.len(), 3, "{:?}", parsed.warnings);

        let src = MemSource::default()
            .with("menus/globals.xml", r#"<rect name="Strings"><_base>1</_base></rect>"#)
            .with("menus/main/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Base"/></menu>"#)
            .with_loose("menus/prefabs/preload.xml", r#"<_preloaded>42</_preloaded>"#)
            .with_loose("menus/prefabs/patch.xml", r#"<rect name="Added"><x>7</x></rect>"#)
            .with_loose("menus/sibling/extra.xml", r#"<_from_sibling>9</_from_sibling>"#)
            .with(
                "uio/public/at_targets.txt",
                "preload.xml::@globals.xml\ntrue\n\
                 patch.xml::@main\\hud.xml\ntrue\n\
                 ..\\sibling\\extra.xml::HUDMainMenu\ntrue\n",
            );
        let mut ui = Ui::new();
        ui.configure_uio(&src, |_| false);
        assert!(ui.uio.warnings.is_empty(), "{:?}", ui.uio.warnings);
        assert!(ui.load_globals(&src));
        let hud = ui.load_menu(&src, "menus\\main\\hud.xml").unwrap();
        ui.update();
        assert_eq!(ui.uio.applied(), 3);
        assert_eq!(ui.get_float("globals()/_base"), Some(1.0));
        assert_eq!(ui.get_float("globals()/_preloaded"), Some(42.0));
        assert_eq!(ui.get_float("HUDMainMenu/Added/x"), Some(7.0));
        assert_eq!(ui.get_float("HUDMainMenu/_from_sibling"), Some(9.0));
        assert!(ui.warnings.is_empty(), "{:?}", ui.warnings);
        assert!(ui.has_mod_added_tiles(hud));
    }

    #[test]
    fn vanilla_unchanged_without_uio_lists() {
        // [design] Unmodded vanilla control: no `uio/` lists means zero decisions, zero
        // warnings, and identical loaded tile tree.
        let src = fixture();
        let mut plain = Ui::new();
        let plain_hud = plain.load_menu(&src, "menus/hud.xml").unwrap();
        let mut with_uio = Ui::new();
        with_uio.configure_uio(&src, |_| false);
        let uio_hud = with_uio.load_menu(&src, "menus/hud.xml").unwrap();
        assert!(with_uio.uio.decisions.is_empty());
        assert!(with_uio.uio.warnings.is_empty());
        assert_eq!(with_uio.uio.applied(), 0);
        assert_eq!(with_uio.tile(uio_hud).children.len(), plain.tile(plain_hud).children.len());
        assert!(!with_uio.has_mod_added_tiles(uio_hud));
    }

    #[test]
    fn builtin_vui_plus_and_darn_conditions() {
        // Parsing and deduplication of `vui+`, `darn`, `darnui`, and `false`.
        assert_eq!(Condition::parse("vui+").unwrap(), Condition::VuiPlus);
        assert_eq!(Condition::parse("VUI+").unwrap(), Condition::VuiPlus);
        assert_eq!(Condition::parse("darn").unwrap(), Condition::DarnUi);
        assert_eq!(Condition::parse("DarN").unwrap(), Condition::DarnUi);
        assert_eq!(Condition::parse("darnui").unwrap(), Condition::DarnUi);
        assert_eq!(Condition::parse("DarnUI").unwrap(), Condition::DarnUi);
        assert_eq!(Condition::parse("false").unwrap(), Condition::False);
        assert_eq!(Condition::parse("FALSE").unwrap(), Condition::False);

        // Vanilla (neither VUI+ nor DarN installed): all built-ins false, zero unknown warnings.
        let vanilla = MemSource::default().with("menus/ySI/ySI.xml", "<rect name=\"ySI\"/>");
        for (expr, expected) in [
            ("vui+", false),
            ("darn", false),
            ("darnui", false),
            ("vui+ || darn", false),
            ("!vui+ && !darn && #..\\ySI\\ySI.xml", true),
            ("vui+ && #..\\ySI\\ySI.xml", false),
            ("darn && #..\\ySI\\ySI.xml", false),
        ] {
            let mut u = Vec::new();
            assert_eq!(Condition::parse(expr).unwrap().evaluate(&vanilla, &|_| false, &mut u), expected, "vanilla expr {expr}");
            assert!(u.is_empty(), "{expr}: {u:?}");
        }

        // Standalone DarN_Fonts pack must NOT false-positive as DarNified UI or VUI+.
        let darn_fonts_only = MemSource::default().with("menus/prefabs/DarN_Fonts/DarN_Fonts.xml", "<rect/>");
        for expr in ["darn", "darnui", "vui+"] {
            let mut u = Vec::new();
            assert!(!Condition::parse(expr).unwrap().evaluate(&darn_fonts_only, &|_| false, &mut u), "DarN_Fonts must not match {expr}");
            assert!(u.is_empty());
        }

        // Positive triggers for `vui+`: plugin, settings.xml, hudcenteredtext.xml.
        let mut u = Vec::new();
        assert!(Condition::VuiPlus.evaluate(&MemSource::default(), &|p| p == "vanilla ui plus.esp", &mut u));
        assert!(Condition::VuiPlus.evaluate(&MemSource::default().with("menus/prefabs/VUI+/settings.xml", ""), &|_| false, &mut u));
        assert!(Condition::VuiPlus.evaluate(&MemSource::default().with("menus/prefabs/VUI+/hudcenteredtext.xml", ""), &|_| false, &mut u));
        assert!(!Condition::DarnUi.evaluate(&MemSource::default().with("menus/prefabs/VUI+/settings.xml", ""), &|p| p == "vanilla ui plus.esp", &mut u));
        assert!(u.is_empty());

        // Positive triggers for `darn` / `darnui`: plugin, DUINVSettings.xml, darn\text_box.xml.
        assert!(Condition::DarnUi.evaluate(&MemSource::default(), &|p| p == "darnifieduinv.esp", &mut u));
        assert!(Condition::DarnUi.evaluate(&MemSource::default().with("menus/prefabs/DUINVSettings.xml", ""), &|_| false, &mut u));
        assert!(Condition::DarnUi.evaluate(&MemSource::default().with("menus/prefabs/darn/text_box.xml", ""), &|_| false, &mut u));
        assert!(!Condition::VuiPlus.evaluate(&MemSource::default().with("menus/prefabs/DUINVSettings.xml", ""), &|p| p == "darnifieduinv.esp", &mut u));
        assert!(u.is_empty());

        // End-to-end UIO branching (PipBoyUITweaks / MCMExtender pattern) + `darn`/`darnui` dedup.
        let base = || {
            MemSource::default()
                .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><rect name="Root"/></menu>"#)
                .with("menus/ySI/ySI.xml", "<rect name=\"ySI\"/>")
                .with_loose("menus/prefabs/vanilla_branch.xml", r#"<rect name="VanillaBranch"/>"#)
                .with_loose("menus/prefabs/vui_branch.xml", r#"<rect name="VuiBranch"/>"#)
                .with_loose("menus/prefabs/darn_branch.xml", r#"<rect name="DarnBranch"/>"#)
                .with(
                    "uio/public/branches.txt",
                    "vanilla_branch.xml::HUDMainMenu\n!vui+ && !darn && #..\\ySI\\ySI.xml\n\
                     vui_branch.xml::HUDMainMenu\nvui+ && #..\\ySI\\ySI.xml\n\
                     darn_branch.xml::HUDMainMenu\ndarn\n\
                     darn_branch.xml::HUDMainMenu\ndarnui\n",
                )
        };

        // With VUI+ prefab + DarN_Fonts: VUI+ branch applies, vanilla & DarN branches do not.
        let vui_src = base()
            .with("menus/prefabs/VUI+/settings.xml", "")
            .with("menus/prefabs/DarN_Fonts/DarN_Fonts.xml", "");
        let mut ui = Ui::new();
        ui.configure_uio(&vui_src, |_| false);
        assert_eq!(ui.uio.duplicates, 1, "darn and darnui normalize to the same condition for dedup");
        assert!(ui.uio.warnings.is_empty(), "{:?}", ui.uio.warnings);
        ui.load_menu(&vui_src, "menus/hud.xml").unwrap();
        assert!(ui.find_tile("HUDMainMenu/VuiBranch").is_some());
        assert!(ui.find_tile("HUDMainMenu/VanillaBranch").is_none());
        assert!(ui.find_tile("HUDMainMenu/DarnBranch").is_none());

        // With DarNifiedUINV.esp: DarN branch applies, vanilla & VUI+ branches do not.
        let darn_src = base();
        let mut ui = Ui::new();
        ui.configure_uio(&darn_src, |p| p.eq_ignore_ascii_case("DarNifiedUINV.esp"));
        assert!(ui.uio.warnings.is_empty(), "{:?}", ui.uio.warnings);
        ui.load_menu(&darn_src, "menus/hud.xml").unwrap();
        assert!(ui.find_tile("HUDMainMenu/DarnBranch").is_some());
        assert!(ui.find_tile("HUDMainMenu/VanillaBranch").is_none());
        assert!(ui.find_tile("HUDMainMenu/VuiBranch").is_none());
    }

    #[test]
    fn three_line_public_list_entries_and_malformed_edge_cases() {
        // [observed: AimCollisionIndicator.txt, Colorful Icons Support - ySI.txt,
        // ySIAssortedFixes.txt] 3-line entries keep line 2's condition and consume
        // the trailing `true` without warnings.
        let text = "\
            main.xml::HUDMainMenu\n\
            true\n\
            jdc_patch.xml::@Prefabs\\JDC\\JDC.xml::JDC\n\
            #JDC\\JDC.xml\n\
            true\n\
            jlm_patch.xml::@Prefabs\\JLM\\JLM.xml::JLMIconImage\n\
            #JLM\\JLM.xml\n\
            TRUE\n\
            hotkey.xml::InventoryMenu::IM_HotkeyButton\n\
            !#VUI+\\hudcenteredtext.xml\n\
            true\n\
            modbtn.xml::InventoryMenu::IM_ModButton\n\
            true\n";
        let parsed = Lists::parse("three_line.txt", text);
        assert!(parsed.warnings.is_empty(), "{:?}", parsed.warnings);
        assert_eq!(parsed.entries.len(), 5);
        assert_eq!(parsed.entries[1].condition, Condition::File("jdc\\jdc.xml".into()));
        assert_eq!(parsed.entries[2].condition, Condition::File("jlm\\jlm.xml".into()));
        assert_eq!(parsed.entries[3].condition, Condition::Not(Box::new(Condition::File("vui+\\hudcenteredtext.xml".into()))));
        assert_eq!(parsed.entries[4].include, "modbtn.xml");
        assert_eq!(parsed.entries[4].condition, Condition::True);

        // End-to-end evaluation & application for 3-line entries (both true and false line 2).
        let src = MemSource::default()
            .with("menus/hud.xml", r#"<menu name="HUDMainMenu"><include src="JLM/JLM.xml"/></menu>"#)
            .with("menus/prefabs/JLM/JLM.xml", r#"<rect name="JLMIconImage"/>"#)
            .with("menus/prefabs/VUI+/hudcenteredtext.xml", "<rect/>")
            .with_loose("menus/prefabs/jlm_color.xml", r#"<systemcolor>0</systemcolor>"#)
            .with_loose("menus/prefabs/hotkey_fix.xml", r#"<rect name="HotkeyFix"/>"#)
            .with(
                "uio/public/three_line_live.txt",
                "jlm_color.xml::@Prefabs\\JLM\\JLM.xml::JLMIconImage\n#JLM\\JLM.xml\ntrue\n\
                 hotkey_fix.xml::HUDMainMenu\n!#VUI+\\hudcenteredtext.xml\ntrue\n",
            );
        let mut ui = Ui::new();
        ui.configure_uio(&src, |_| false);
        assert!(ui.uio.warnings.is_empty(), "{:?}", ui.uio.warnings);
        ui.load_menu(&src, "menus/hud.xml").unwrap();
        ui.update();
        assert_eq!(ui.get_float("HUDMainMenu/JLMIconImage/systemcolor"), Some(0.0));
        assert!(ui.find_tile("HUDMainMenu/HotkeyFix").is_none(), "line 2 condition !#VUI+\\hudcenteredtext.xml must stay false despite trailing true");

        // Edge cases: 4th stray `true`, 3rd-line `false`, 3rd-line garbage, broken line 2 + `true`,
        // and standalone `true` all emit `malformed` warnings without corrupting valid neighbors.
        let malformed = "\
            a.xml::HUDMainMenu\n#x.xml\ntrue\ntrue\n\
            b.xml::HUDMainMenu\n#x.xml\nfalse\n\
            c.xml::HUDMainMenu\ntrue\ngarbage\n\
            d.xml::HUDMainMenu\n?\ntrue\n\
            e.xml::HUDMainMenu\ntrue\n";
        let edge = Lists::parse("edge.txt", malformed);
        assert_eq!(
            edge.entries.iter().map(|e| e.include.as_str()).collect::<Vec<_>>(),
            ["a.xml", "b.xml", "c.xml", "e.xml"]
        );
        assert_eq!(edge.entries[0].condition, Condition::File("x.xml".into()));
        assert_eq!(edge.entries[1].condition, Condition::File("x.xml".into()));
        assert_eq!(edge.entries[2].condition, Condition::True);
        // Warnings: line 4 (4th true), line 7 (3rd false), line 10 (garbage), line 11 (broken ?), line 13 (stray true after broken entry).
        assert_eq!(edge.warnings.len(), 5, "{:?}", edge.warnings);
    }
}
