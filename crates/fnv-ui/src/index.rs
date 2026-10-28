//! Menu names to files [design: docs/design/ui-tile-tree.md §3.2].

use crate::{xml, Source};
use std::collections::BTreeMap;

#[derive(Default)]
pub struct MenuIndex {
    files: BTreeMap<String, String>,
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
                xml::Node::Element(e) if e.is("menu") => e.attr("name"),
                _ => None,
            });
            if let Some(name) = menu.filter(|n| !n.is_empty()) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::MemSource;

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
