//! The pointer and keys on the menus drawn from tiles (task T8; docs/design/ui-tile-tree.md
//! §4.1 step 1, §5.2): the engine gives a frame's input, [`step`] writes `mouseover`,
//! `clicked` and the keys' clicks into the tree (`fnv_ui::input`) and fires the JIP menu
//! events for scripts.
//!
//! The engine calls it before scripts, only for the menus that take tile input (a menu
//! drawn from its file), so the default game does no new work a frame [design: the
//! maintainer's review of #902].

use crate::game::Game;
use fnv_script::vm::XVal;
use fnv_ui::input::{By, Input, Measure, Report};
use fnv_ui::value::menu_class_number;
use fnv_ui::{TileId, Ui, Value};

/// A menu's ID for the events: its `class` [documented: Hardcoded UI Shortcuts, the menu
/// class IDs; SetOnMenuClickEventHandler, "Menu Mode ID - 1002/1009/1075"]; 0 when it has
/// none known.
pub fn menu_id(ui: &Ui, tile: TileId) -> f64 {
    let root = ui.root_of(tile);
    match ui.tile(root).get("class") {
        Some(Value::Sym(name)) => menu_class_number(name).map_or(0.0, f64::from),
        Some(v) => v.num(),
        None => 0.0,
    }
}

/// A tile's `id`, -1 without one [documented: SetOnMenuClickEventHandler, "Only for
/// tiles with a <id> property. Otherwise -1 is passed"].
pub fn tile_id(ui: &Ui, tile: TileId) -> f64 {
    ui.tile(tile).get("id").map_or(-1.0, Value::num)
}

/// One frame of tile input for `menus` (back to front), then the script events:
/// SetOnMouseoverChangeEventHandler's when the highlight moved (with the menu ID of the
/// tile now highlighted, else of the one left) and SetOnMenuClickEventHandler's for each
/// click [documented: their pages, jip-events.md §2]. A template list row's name is passed
/// without the `:<index>` the original adds [hypothesis: not built yet].
pub fn step(g: &mut Game, menus: &[TileId], input: &Input, measure: &Measure<'_>) -> Report {
    let report = g.ui_pointer.step(&mut g.ui_tree, menus, input, measure);
    if let Some((from, to)) = report.over {
        if let Some(t) = to.or(from).filter(|&t| g.ui_tree.is_live(t)) {
            let id = menu_id(&g.ui_tree, t);
            g.jip_event("mouseoverchange", None, vec![XVal::Num(id)]);
        }
    }
    for click in &report.clicks {
        if !g.ui_tree.is_live(click.tile) {
            continue;
        }
        let ui = &g.ui_tree;
        let menu = ui.tile(ui.root_of(click.tile)).name.clone();
        let (path, mid, id, name) = (ui.path_of(click.tile), menu_id(ui, click.tile), tile_id(ui, click.tile), ui.tile(click.tile).name.clone());
        g.jip_menu_click(&menu, &path, mid, id, &name);
    }
    report
}

/// A frame with no menu taking tile input. A highlight left from the last menu that took
/// it goes (mouseover back to 0, its event fired), and the clicks left over (`clicked`
/// pulses, the scripts' ClickMenuButton and ClickMenuTile) end and fire their events, so a
/// script's `clicked` lasts one frame there too and nothing waits for a menu to take
/// input [design: the maintainer's review of #957]. With neither, no tree work: `None`.
pub fn idle(g: &mut Game) -> Option<Report> {
    if g.ui_pointer.over.is_none() && !g.ui_pointer.pending() {
        return None;
    }
    Some(step(g, &[], &Input::default(), &fnv_ui::input::no_measure))
}

/// One line for the log about a click.
pub fn click_line(ui: &Ui, c: &fnv_ui::input::Click) -> String {
    let by = match &c.by {
        By::Pointer => "the pointer".to_string(),
        By::Key(k) => format!("key {k}"),
        By::Script => "a script".to_string(),
    };
    let id = c.id.map_or_else(|| "no id".to_string(), |n| format!("id {}", Value::Num(n).text()));
    format!("clicked {} ({id}) by {by}", ui.path_of(c.tile))
}
