//! Menu names and class constants to files [design: docs/design/ui-tile-tree.md §3.2].

use crate::{expr, value::menu_class_name, xml, Source, Value};
use std::collections::BTreeMap;

/// Canonical vanilla path for `BookMenu` [documented: `menus\book_menu.xml`, audio.md].
pub const CANONICAL_BOOK_MENU: &str = "menus\\book_menu.xml";
const BOOK_MENU_KEY: &str = "bookmenu";

#[derive(Default)]
pub struct MenuIndex {
    files: BTreeMap<String, String>,
    classes: BTreeMap<String, String>,
    variants: BTreeMap<String, Vec<String>>,
    pub warnings: Vec<String>,
}

impl MenuIndex {
    pub fn build(src: &dyn Source, paths: impl IntoIterator<Item = String>) -> Self {
        let mut index = Self::default();
        let mut paths: Vec<_> = paths.into_iter().map(|p| p.replace('/', "\\").to_ascii_lowercase()).collect();
        paths.sort();
        paths.dedup();
        let mut entries: Vec<(String, Option<String>, Option<String>)> = Vec::new();
        for path in paths {
            if !path.starts_with("menus\\") || !path.ends_with(".xml") || path.starts_with("menus\\prefabs\\") {
                continue;
            }
            let Some(bytes) = src.read(&path) else {
                index.warnings.push(format!("{path}: not found"));
                continue;
            };
            let doc = xml::parse(&xml::decode(&bytes));
            index.warnings.extend(doc.warnings.iter().map(|w| format!("{path}: {w}")));
            let menu = doc.nodes.iter().find_map(|n| match n {
                xml::Node::Element(e) if e.is("menu") => Some(e),
                _ => None,
            });
            if let Some(menu) = menu {
                // Only the menu's own class trait, never a child's or a template's [design].
                let class = menu
                    .elements()
                    .filter(|e| e.is("class"))
                    .last()
                    .and_then(|c| match expr::literal(&c.children, &|n| src.game_string(n)) {
                        Value::Sym(name) => Some(name),
                        _ => None,
                    });
                let name = menu.attr("name").filter(|n| !n.is_empty()).map(str::to_owned);
                entries.push((path, class, name));
            }
        }
        // Book-display mods place alternate `<menu name="BookMenu"><class>&BookMenu;</class>`
        // background XMLs directly under `menus\` beside vanilla `menus\book_menu.xml`. When
        // the canonical file is present, keep it as the primary `BookMenu` entry and record
        // the alternate layouts as variants without duplicate-menu warnings [design: menu-xml.md §12].
        let has_canonical_book_class = entries
            .iter()
            .any(|(p, c, _)| p == CANONICAL_BOOK_MENU && c.as_deref() == Some(BOOK_MENU_KEY));
        let has_canonical_book_name = entries
            .iter()
            .any(|(p, _, n)| p == CANONICAL_BOOK_MENU && n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(BOOK_MENU_KEY)));
        let push_variant = |variants: &mut BTreeMap<String, Vec<String>>, key: &str, path: &str| {
            let list = variants.entry(key.to_string()).or_default();
            if !list.iter().any(|existing| existing == path) {
                list.push(path.to_string());
            }
        };
        for (path, class, name) in entries {
            if let Some(class_name) = class {
                if class_name == BOOK_MENU_KEY && has_canonical_book_class {
                    if path == CANONICAL_BOOK_MENU {
                        index.classes.insert(class_name, path.clone());
                    } else {
                        push_variant(&mut index.variants, BOOK_MENU_KEY, &path);
                    }
                } else if let Some(old) = index.classes.insert(class_name.clone(), path.clone()) {
                    index.warnings.push(format!("{path}: duplicate menu class {class_name}; replaces {old}"));
                }
            }
            let Some(menu_name) = name else { continue };
            let key = menu_name.to_ascii_lowercase();
            if key == BOOK_MENU_KEY && has_canonical_book_name {
                if path == CANONICAL_BOOK_MENU {
                    index.files.insert(key, path);
                } else {
                    push_variant(&mut index.variants, BOOK_MENU_KEY, &path);
                }
            } else if let Some(old) = index.files.insert(key, path.clone()) {
                // Later sorted paths win a duplicate name [design].
                index.warnings.push(format!("{path}: duplicate menu {menu_name}; replaces {old}"));
            }
        }
        index
    }

    pub fn path(&self, name: &str) -> Option<&str> {
        self.files.get(&name.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn class_path(&self, name: &str) -> Option<&str> {
        self.classes.get(&name.to_ascii_lowercase()).map(String::as_str)
    }

    pub fn code_path(&self, code: u16) -> Option<&str> {
        self.class_path(menu_class_name(code)?)
    }

    /// Alternate layout XML paths recorded beside a canonical menu file (such as
    /// `BookMenu` background variants beside `menus\book_menu.xml`), in sorted order.
    pub fn variants(&self, name: &str) -> &[String] {
        self.variants.get(&name.to_ascii_lowercase()).map_or(&[], Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemSource;

    #[test]
    fn classes_not_names_or_filenames() {
        let mut src = MemSource::default();
        for (i, &(name, code)) in crate::value::MENU_CLASSES.iter().enumerate() {
            src = src.with(&format!("menus/added/{i}.xml"), &format!(r#"<menu name="Added{i}"><class> &{name}; </class></menu>"#));
            let index = MenuIndex::build(&src, src.files.keys().cloned());
            assert_eq!(index.code_path(code), Some(format!("menus\\added\\{i}.xml").as_str()));
            assert_eq!(index.class_path(&name.to_ascii_uppercase()), index.code_path(code));
        }
        src = src
            .with("menus/barter_menu.xml", r#"<menu name="BarterMenu"><rect name="Child"><class>&BarterMenu;</class></rect></menu>"#)
            .with("menus/quantity.xml", r#"<menu name="QuantityMenu"><class>1016</class></menu>"#)
            .with("menus/prefabs/hidden.xml", r#"<menu name="Hidden"><class>&BarterMenu;</class></menu>"#);
        let index = MenuIndex::build(&src, src.files.keys().cloned());
        let barter = crate::value::menu_class_number("BarterMenu").unwrap();
        let expected = index.code_path(barter).unwrap().to_string();
        assert!(expected.starts_with("menus\\added\\"));
        assert_eq!(index.code_path(1), None); // MenuMode group, not a class [documented: MenuMode (Function)].
        let src = MemSource::default()
            .with("menus/barter_menu.xml", r#"<menu name="BarterMenu"><rect name="Child"><class>&BarterMenu;</class></rect></menu>"#)
            .with("menus/quantity.xml", r#"<menu name="QuantityMenu"><class>1016</class></menu>"#);
        let index = MenuIndex::build(&src, src.files.keys().cloned());
        assert!(index.code_path(barter).is_none());
        assert!(index.code_path(crate::value::menu_class_number("QuantityMenu").unwrap()).is_none());
    }

    #[test]
    fn names_not_filenames_and_no_prefabs() {
        let src = MemSource::default()
            .with("menus/added/renamed.xml", r#"<menu name="HUDMainMenu"/>"#)
            .with("menus/prefabs/hidden.xml", r#"<menu name="HiddenMenu"/>"#)
            .with("menus/globals.xml", r#"<rect name="Strings"/>"#)
            .with("other/menu.xml", r#"<menu name="Outside"/>"#);
        let index = MenuIndex::build(&src, src.files.keys().map(|p| p.to_ascii_uppercase()));
        assert_eq!(index.path("hudMAINmenu"), Some("menus\\added\\renamed.xml"));
        assert_eq!(index.path("HiddenMenu"), None);
        assert_eq!(index.path("Strings"), None);
        assert_eq!(index.path("Outside"), None);
        assert!(index.warnings.is_empty());
    }

    #[test]
    fn book_menu_variants_preserve_canonical_path_without_duplicate_warnings() {
        let book = crate::value::menu_class_number("BookMenu").unwrap();
        // Unmodded vanilla: single `menus/book_menu.xml`, zero variants and zero warnings.
        let vanilla = MemSource::default().with("menus/book_menu.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#);
        let vanilla_idx = MenuIndex::build(&vanilla, vanilla.files.keys().cloned());
        assert_eq!(vanilla_idx.path("BookMenu"), Some(CANONICAL_BOOK_MENU));
        assert_eq!(vanilla_idx.class_path("BookMenu"), Some(CANONICAL_BOOK_MENU));
        assert_eq!(vanilla_idx.code_path(book), Some(CANONICAL_BOOK_MENU));
        assert!(vanilla_idx.variants("BookMenu").is_empty());
        assert!(vanilla_idx.warnings.is_empty());

        // Alternate `BookMenu` XMLs sorting both before (`alt_book.xml`) and after
        // (`BookBackground1..4.xml`) `menus/book_menu.xml` keep canonical `book_menu.xml`
        // as primary, record all variants in sorted order, and emit no duplicate warnings.
        let modded = vanilla
            .with("menus/alt_book.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#)
            .with("menus/BookBackground1.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#)
            .with("menus/BookBackground2.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#)
            .with("menus/BookBackground3.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#)
            .with("menus/BookBackground4.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#);
        let modded_idx = MenuIndex::build(&modded, modded.files.keys().cloned());
        assert_eq!(modded_idx.path("BookMenu"), Some(CANONICAL_BOOK_MENU));
        assert_eq!(modded_idx.class_path("BookMenu"), Some(CANONICAL_BOOK_MENU));
        assert_eq!(modded_idx.code_path(book), Some(CANONICAL_BOOK_MENU));
        assert_eq!(
            modded_idx.variants("BOOKMENU"),
            [
                "menus\\alt_book.xml",
                "menus\\bookbackground1.xml",
                "menus\\bookbackground2.xml",
                "menus\\bookbackground3.xml",
                "menus\\bookbackground4.xml",
            ]
        );
        assert!(modded_idx.warnings.is_empty(), "unexpected warnings: {:?}", modded_idx.warnings);

        // Non-`BookMenu` duplicate classes/names keep the existing later-wins warning.
        let dup_other = MemSource::default()
            .with("menus/a_barter.xml", r#"<menu name="BarterMenu"><class>&BarterMenu;</class></menu>"#)
            .with("menus/b_barter.xml", r#"<menu name="BarterMenu"><class>&BarterMenu;</class></menu>"#);
        let dup_idx = MenuIndex::build(&dup_other, dup_other.files.keys().cloned());
        assert_eq!(dup_idx.path("BarterMenu"), Some("menus\\b_barter.xml"));
        assert_eq!(dup_idx.class_path("BarterMenu"), Some("menus\\b_barter.xml"));
        assert_eq!(dup_idx.warnings.len(), 2);
        assert!(dup_idx.warnings[0].contains("duplicate menu class bartermenu"));
        assert!(dup_idx.warnings[1].contains("duplicate menu BarterMenu"));

        // Without canonical `menus/book_menu.xml`, two colliding BookMenu XMLs keep the
        // existing duplicate warning behavior.
        let no_canonical = MemSource::default()
            .with("menus/BookBackground1.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#)
            .with("menus/BookBackground2.xml", r#"<menu name="BookMenu"><class>&BookMenu;</class></menu>"#);
        let no_can_idx = MenuIndex::build(&no_canonical, no_canonical.files.keys().cloned());
        assert_eq!(no_can_idx.path("BookMenu"), Some("menus\\bookbackground2.xml"));
        assert_eq!(no_can_idx.code_path(book), Some("menus\\bookbackground2.xml"));
        assert!(no_can_idx.variants("BookMenu").is_empty());
        assert_eq!(no_can_idx.warnings.len(), 2);
    }
}
