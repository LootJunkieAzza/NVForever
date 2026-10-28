//! The UI script functions over the menus' tile tree ([`Game::ui_tree`], `fnv_ui`;
//! docs/design/ui-tile-tree.md §5, docs/spec-notes/menu-xml.md §5): GetUIFloat,
//! SetUIFloat, SetUIString, ModUIFloat, GetUIString, IsComponentLoaded, InjectUIXML,
//! InjectUIComponent, UnloadUIComponent, AddTileFromTemplate, MenuTapKey, with their Alt
//! and Ex forms.
//!
//! [`call`] takes a function by name, as the script extender's `Host::ext_call` gives
//! it (`crate::nvse::call` hands the names no other family takes here). Arguments are taken as the VM
//! gives them; the VM formats a format string and its values (SetUIStringEx,
//! SetUIStringAlt, InjectUIComponent, AddTileFromTemplate) before the call, so the text
//! arrives here in their place.
//!
//! The deprecated forms (SetUIFloat, GetUIFloat, SetUIString, SetUIStringEx) behave as
//! their Alt forms for now: the wiki calls them "bugged for templated children tiles"
//! and says MCM depends on the bug, but the bug's page is not in the export
//! (menu-xml.md §8 item 5).

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_ui::{MemSource, Source, Value};

/// GetUIFloat's answer for a path that names no trait [documented: GetUIFloat, "Returns
/// -999 if UI trait was not found"].
pub const NOT_FOUND: f64 = -999.0;

fn text(args: &[XVal], k: usize) -> String {
    match args.get(k) {
        Some(XVal::Str(s)) => s.clone(),
        Some(XVal::Num(n)) => Value::Num(*n).text(),
        _ => String::new(),
    }
}

fn num(args: &[XVal], k: usize) -> f64 {
    match args.get(k) {
        Some(XVal::Num(n)) => *n,
        Some(XVal::Str(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn yes(b: bool) -> XVal {
    XVal::Num(if b { 1.0 } else { 0.0 })
}

/// The UI function `name` (any case) with its arguments; `None` when it is not one of
/// these.
pub fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let lname = name.to_ascii_lowercase();
    let path = text(args, 0);
    Some(Ok(match lname.as_str() {
        "getuifloat" | "getuifloatalt" => XVal::Num(g.ui_tree.get_float(&path).unwrap_or(NOT_FOUND)),
        "getuistring" => XVal::Str(g.ui_tree.get_string(&path).unwrap_or_default()),
        "setuifloat" | "setuifloatalt" => yes(g.ui_tree.set_float(&path, num(args, 1))),
        "setuistring" | "setuistringalt" | "setuistringex" => yes(g.ui_tree.set_string(&path, &text(args, 1))),
        "moduifloat" => yes(g.ui_tree.mod_float(&path, num(args, 1))),
        "iscomponentloaded" => yes(g.ui_tree.is_loaded(&path)),
        "injectuixml" => {
            let file = text(args, 1);
            match g.ui_files.as_deref() {
                Some(src) => yes(g.ui_tree.inject_file(src, &path, &file)),
                None => yes(false),
            }
        }
        "injectuicomponent" => {
            let xml = text(args, 1);
            let empty = MemSource::default();
            let src: &dyn Source = match g.ui_files.as_deref() {
                Some(s) => s,
                None => &empty,
            };
            yes(g.ui_tree.inject_text(src, &path, &xml))
        }
        "unloaduicomponent" => match g.ui_tree.find_tile(&path) {
            Some(t) => {
                g.ui_tree.unload_tile(t);
                yes(true)
            }
            None => yes(false),
        },
        // "componentPath|templateName|altTileName" [documented: AddTileFromTemplate].
        "addtilefromtemplate" => {
            let mut parts = path.splitn(3, '|');
            let (at, template) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
            let alt = parts.next().map(str::trim).filter(|s| !s.is_empty());
            let empty = MemSource::default();
            let src: &dyn Source = match g.ui_files.as_deref() {
                Some(s) => s,
                None => &empty,
            };
            yes(g.ui_tree.add_from_template(src, at.trim(), template.trim(), alt).is_some())
        }
        "menutapkey" => {
            g.ui_taps.push(num(args, 0).max(0.0) as u32);
            XVal::Num(0.0)
        }
        _ => return None,
    }))
}
