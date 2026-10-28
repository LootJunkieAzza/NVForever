//! Menu names and class constants to files [design: docs/design/ui-tile-tree.md §3.2].

use crate::{expr, value::menu_class_name, xml, Source, Value};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct MenuIndex {
    files: BTreeMap<String, String>,
    classes: BTreeMap<String, String>,
    pub warnings: Vec<String>,
}

impl MenuIndex {
    pub fn build(src: &dyn Source, paths: impl IntoIterator<Item = String>) -> Self {
        let mut index = Self::default();
        let mut paths: Vec<_> = paths.into_iter().map(|p| p.replace('/', "\\").to_ascii_lowercase()).collect();
        paths.sort();
        paths.dedup();
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
                if let Some(class) = menu.elements().filter(|e| e.is("class")).last() {
                    if let Value::Sym(name) = expr::literal(&class.children, &|n| src.game_string(n)) {
                        if let Some(old) = index.classes.insert(name.clone(), path.clone()) {
                            index.warnings.push(format!("{path}: duplicate menu class {name}; replaces {old}"));
                        }
                    }
                }
                let Some(name) = menu.attr("name").filter(|n| !n.is_empty()) else { continue };
                // Later sorted paths win a duplicate name [design].
                if let Some(old) = index.files.insert(name.to_ascii_lowercase(), path.clone()) {
                    index.warnings.push(format!("{path}: duplicate menu {name}; replaces {old}"));
                }
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
}
