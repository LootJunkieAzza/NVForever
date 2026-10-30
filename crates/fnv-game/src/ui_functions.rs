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

/// The functions [`call`] takes that may read or change the HUD's traits, in lower case:
/// their first call on the HUD ([`on_hud`]) makes the HUD's kept-back writes
/// (`crate::ui_hud::flush`). Not MenuTapKey, and not MCM's shorthands, which reach
/// `StartMenu/MCM/...` only, so a mod's settings menu does not start the HUD's per-frame
/// tree writes [design].
const TREE_FUNCTIONS: [&str; 15] = [
    "getuifloat", "getuifloatalt", "getuifloatinherited", "getuistring", "setuifloat", "setuifloatalt", "setuistring", "setuistringalt", "setuistringex", "moduifloat",
    "iscomponentloaded", "injectuixml", "injectuicomponent", "unloaduicomponent", "addtilefromtemplate",
];

/// The menu whose writes wait while nothing reads them (`crate::ui_hud::defer`).
const HUD_MENU: &str = "HUDMainMenu";

/// Whether a path is in the HUD: its first segment, the menu, is `HUDMainMenu` (any case;
/// segments split on `/` or `\` as `fnv_ui` splits them). A call on another menu neither
/// reads nor changes the HUD's traits, so it leaves the HUD's writes waiting [design:
/// maintainer's review of #902, #937].
fn on_hud(path: &str) -> bool {
    path.split(['/', '\\']).map(str::trim).find(|s| !s.is_empty()).is_some_and(|menu| menu.eq_ignore_ascii_case(HUD_MENU))
}

/// The trait path MCM's functions stand for: `StartMenu/MCM/*:child/*:grandchild/setting`.
fn mcm_path(args: &[XVal]) -> String {
    let i = |k| num(args, k) as i64;
    format!("StartMenu/MCM/*:{}/*:{}/{}", i(0), i(1), text(args, 2))
}

/// GetUIFloatInherited: `x`, `y` and `depth` with the values of every parent tile whose
/// `locus` is on added, up to the menu's root, which `exclude_root` leaves out; any other
/// trait as GetUIFloatAlt reads it [documented: GetUIFloatInherited; observed: hud.md §14,
/// the page's example `HUDMainMenu/RadiationMeter/radiation_text/depth` -50, and 0 with
/// ExcludeMenuRoot]. Whether a parent without `locus` stops the sum is open
/// [hypothesis: it does not; the page counts "all parent tiles with an active locus"].
fn inherited(ui: &fnv_ui::Ui, path: &str, exclude_root: bool) -> Option<f64> {
    let (tile_path, name) = path.rsplit_once(['/', '\\'])?;
    if !["x", "y", "depth"].iter().any(|t| t.eq_ignore_ascii_case(name)) {
        return ui.get_float(path);
    }
    // A tile without the trait counts 0 [observed: radiation_text has no depth in
    // hud_main_menu.xml, and the example still gives -50].
    let mut id = ui.find_tile(tile_path)?;
    let own = ui.get_float(path).unwrap_or(0.0);
    let screen = ui.screen();
    let mut sum = own;
    while let Some(parent) = ui.tile(id).parent.filter(|&p| p != screen) {
        let p = ui.tile(parent);
        let root = p.parent.is_none_or(|pp| pp == screen);
        let locus = p.get("locus").is_some_and(|v| v.num() != 0.0);
        if locus && !(root && exclude_root) {
            sum += p.get(name).map_or(0.0, |v| v.num());
        }
        id = parent;
    }
    Some(sum)
}

/// The UI function `name` (any case) with its arguments; `None` when it is not one of
/// these.
pub fn call(g: &mut Game, name: &str, args: &[XVal]) -> Option<Result<XVal, String>> {
    let lname = name.to_ascii_lowercase();
    let path = text(args, 0);
    // The path names the trait or tile read or changed, the tile injected into or
    // unloaded; AddTileFromTemplate's is before its first `|` [documented:
    // AddTileFromTemplate].
    let target = if lname == "addtilefromtemplate" { path.split('|').next().unwrap_or("") } else { path.as_str() };
    if TREE_FUNCTIONS.contains(&lname.as_str()) && on_hud(target) {
        // The HUD's writes kept back while nothing read the HUD, made now; the engine
        // writes them every frame from here on (`crate::ui_hud::defer`) [design].
        g.ui_used = true;
        crate::ui_hud::flush(g);
    }
    Some(Ok(match lname.as_str() {
        "getuifloat" | "getuifloatalt" => XVal::Num(g.ui_tree.get_float(&path).unwrap_or(NOT_FOUND)),
        "getuifloatinherited" => XVal::Num(inherited(&g.ui_tree, &path, num(args, 1) != 0.0).unwrap_or(NOT_FOUND)),
        "getuistring" => XVal::Str(g.ui_tree.get_string(&path).unwrap_or_default()),
        "setuifloat" | "setuifloatalt" => yes(g.ui_tree.set_float(&path, num(args, 1))),
        "setuistring" | "setuistringalt" | "setuistringex" => yes(g.ui_tree.set_string(&path, &text(args, 1))),
        "moduifloat" => yes(g.ui_tree.mod_float(&path, num(args, 1))),
        // MCM's shorthands for the traits of `StartMenu/MCM/*:child/*:grandchild/setting`
        // [documented: SetMCMFloat and SetMCMString give the SetUIFloat and SetUIString
        // they equal]. A trait that is not there reads 0, not GetUIFloat's -999 [observed:
        // the mcm set, GetMCMFloat 1 1 "_enable" 0 and GetUIFloat on StartMenu -999 with
        // the start menu closed]; SetMCMFloat returns 0 [hypothesis].
        "getmcmfloat" => XVal::Num(g.ui_tree.get_float(&mcm_path(args)).unwrap_or(0.0)),
        "setmcmfloat" => {
            g.ui_tree.set_float(&mcm_path(args), num(args, 3));
            XVal::Num(0.0)
        }
        "setmcmstring" => {
            g.ui_tree.set_string(&mcm_path(args), &text(args, 3));
            XVal::Num(0.0)
        }
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
