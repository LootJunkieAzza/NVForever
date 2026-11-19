//! T11: the pause menu's Mods page, a mod's part of `StartMenu` drawn from its tiles
//! (issue #976) [design: ui-tile-tree.md §7, owner-review.md #34: NVForever's column
//! stays; a "Mods" entry joins it only when a mod added one, and opens the mod's page
//! from tiles; docs/handoff/ui-tile-tree-tasks.md, T11].
//!
//! - **The switch**: `[UI] TileMenus=StartMenu` (`super::vanilla::TileMenus::start`), on
//!   by default (`-StartMenu` in the list turns it off; `super::vanilla`). With it on and a
//!   mod's tiles in `StartMenu` (MCM's include, menu-xml.md §10;
//!   `fnv_ui::Ui::has_mod_added_tiles`), the column has a Mods entry
//!   (`crate::pause::ModsEntry`, [`available`]). `StartMenu` itself is built from the files
//!   whenever the pause menu is open (T3b, `fnv_game::ui_loading::sync`).
//! - **The page** (`crate::pause::Page::Mods`): the mod's tiles of `StartMenu` are drawn
//!   over the pause menu (`super::draw`, [`MODS_Z`]; only the tiles mods added, as T5 draws
//!   the HUD's: a loose replacement of the start menu's file draws its own tiles, not the
//!   archived file's), in interface units; T8's pointer and keys go to `StartMenu`
//!   (`super::input`); and the start menu's MenuMode blocks run each frame
//!   (`Begin MenuMode 1013`, [`tick`]), as they do under the original's start menu, so a
//!   mod's script fills in its page. Esc or Back leave it.
//! - **MCM**: MCM's script shows its trigger only when the original's pause column,
//!   filled in by the game's code, reports `childcount` 8; [`prepare_column`] and
//!   [`column`] write that, the fade and one code row into the tile tree's column while the
//!   page is up. Its mod list is filled through MCM.dll's functions
//!   (`fnv_game::ui_functions`: SetMCMModList, GetMCMListWidth, SetMCMFloatMass,
//!   SetMCMStringMass, menu-xml.md §10.2); its inputs, the slider's buttons among them,
//!   reach its main script through its own quest stage 2, from its tiles (menu-xml.md §3).
//!   See the tasks doc, T11.
//! - **The original's pause column, for MCM** ([`prepare_column`], then [`column`] each frame): MCM shows its trigger
//!   only while `StartMenu/NOGLOW_BRANCH/main_container` reports `childcount` 8 and places
//!   it from that column. The original's code fills the column with six rows of the file's
//!   `lb_item_template` and writes `childcount` 8, `_alpha` 255 and `_enabled` 1 [observed:
//!   menu-xml.md §10.1]; NVForever's column is hand-built, so the page writes those traits
//!   and makes one code row (the one MCM reads) when it opens. `_enabled` is MCM's after
//!   that (it turns the column off while its window is open).
//! - **MCM opens with the page** ([`tick`]): a click on MCM's trigger (its `_clicked`, one
//!   frame) once the trigger has been placed and a mod has registered with MCM, as the
//!   player's click on the original's "Mod Configuration" row; MCM's window closing itself
//!   (its Back, its trigger: `_Menu` back to 0) closes the page ([`ModsPage::closed`])
//!   [design].
//! - **The click waits for MCM's list** (issue #2944): an MCM mod adds itself to MCM's list
//!   (`MCMMods`) from its own `MenuMode 1013` block, Just Assorted Mods after half a second
//!   of `GetSecondsPassed` and then `ListAddForm` [observed: its script source]. The start
//!   menu's frames before the game normally do it (`crate::script`, before a save is read),
//!   but a mod that registers only under the page would meet a click on its 3rd frame with
//!   an empty list, and MCM stops at its "No MCM Mods" check [observed: MCM's script source].
//!   The click waits for the list to have members, at most [`WAIT_FOR`] frames (a list no
//!   mod fills still gets its click) [design].
use bevy::prelude::info;
use fnv_game::game::menus;
use fnv_ui::Ui;
use std::collections::HashSet;

/// The start menu's name [documented: start_menu.xml].
pub const START_MENU: &str = "StartMenu";

/// The Mods page's stacking level: over the pause menu's (110 to 113, `crate::pause`)
/// [design].
pub const MODS_Z: i32 = 115;

/// The archived start menu file's tile names, kept per file path (`super::draw`).
pub type KeptNames = Option<(String, Option<HashSet<String>>)>;

/// Whether the column has a Mods entry: the switch on and a tile in `StartMenu` that a mod
/// adds, as the HUD's mod tiles are told apart (`Ui::mod_tile_drawn`): injected, a
/// template's, a loose UIO include's, or a tile of a loose replacement that the archived
/// file does not have. A loose `start_menu.xml` that only reskins the menu adds none
/// [design]. `files` reads the archived file's names, once per path into `kept`.
pub fn available(switch: super::vanilla::TileMenus, ui: &Ui, files: Option<&dyn fnv_ui::Source>, kept: &mut KeptNames) -> bool {
    if !switch.start {
        return false;
    }
    let Some(menu) = ui.menu(START_MENU).filter(|&m| ui.has_mod_added_tiles(m)) else { return false };
    let vanilla = files.and_then(|f| super::draw::archived_names(ui, menu, f, kept));
    let mut stack = ui.mod_added_tiles(menu);
    while let Some(t) = stack.pop() {
        if !ui.tile(t).live {
            continue;
        }
        if ui.mod_tile_drawn(t, vanilla) {
            return true;
        }
        stack.extend(ui.tile(t).children.iter().copied());
    }
    false
}

/// MCM's Back on its open page [observed: mcm.xml, `hotrect name="MCM_Back"` under the
/// `MCM` rect, shown while the rect's `_Menu` is set].
pub const OWN_BACK: &str = "StartMenu/MCM/MCM_Back";

/// Whether the mod's page shows its own Back now: [`OWN_BACK`] is live and shown, it and
/// every tile above it (`crate::pause::ModsOwnBack`) [design].
pub fn own_back(ui: &Ui) -> bool {
    let mut at = ui.find_tile(OWN_BACK);
    let found = at.is_some();
    while let Some(t) = at {
        let tile = ui.tile(t);
        if !tile.live || tile.get("visible").is_some_and(|v| !v.truthy()) {
            return false;
        }
        at = tile.parent;
    }
    found
}

/// MCM's own rect and its trigger [observed: MCM's `MCM.xml`, menu-xml.md §10].
pub const MCM: &str = "StartMenu/MCM";
pub const MCM_TRIGGER: &str = "StartMenu/MCM/MCM_Trigger";
/// MCM's pop-up windows: the slider's and the list's [observed: MCM's Scale.xml, List.xml].
const MCM_WINDOWS: [&str; 2] = ["StartMenu/MCM/MCM_Scale", "StartMenu/MCM/MCM_List"];
/// How far they are drawn over the summed depth: past the option rows' 25 from the
/// windows' own 15 [design: see `super::layout::DEPTH_LIFT`].
const MCM_WINDOW_LIFT: f64 = 20.0;
/// The frame of the page MCM's trigger is clicked on: after MCM's first frames have placed
/// it (its install checks run on the first) [design].
const CLICK_AT: u64 = 3;
/// The most frames the click is held for [design].
const CLICK_FOR: u64 = 60;
/// The most frames past [`CLICK_AT`] the click waits for a mod in MCM's list (`MCMMods`):
/// Just Assorted Mods registers after half a second of the start menu's frames; three
/// seconds at 60 frames a second leave room for slower mods [design].
pub const WAIT_FOR: u64 = 180;
/// MCM's list of the mods registered with it [observed: The Mod Configuration Menu.esp].
pub const MCM_MODS: &str = "MCMMods";

/// The Mods page's state over its frames (`crate::script` keeps it; `crate::pause` reads
/// [`ModsPage::closed`]).
#[derive(bevy::prelude::Resource, Default, Debug, Clone, PartialEq, Eq)]
pub struct ModsPage {
    /// Frames of the page so far (0 before its first).
    pub frames: u64,
    /// MCM's window was seen open on this page.
    pub mcm_open: bool,
    /// MCM's window closed itself after being open: the page goes back to the column.
    pub closed: bool,
    /// MCM's trigger is held clicked, until its window opens.
    pub clicking: bool,
    /// MCM's slider window and row dimming as last told (`tick`'s log line).
    pub dim_told: String,
    /// The page's frame the trigger was clicked on (one click a page).
    pub clicked_at: Option<u64>,
}

/// Whether the page clicks MCM's trigger on this frame: its first frames done, MCM's
/// trigger placed, its window shut, no click yet on this page, and a mod in MCM's list
/// (`listed`) or the wait over ([`WAIT_FOR`]) [design: issue #2944].
pub fn click_now(page: &ModsPage, trigger: bool, open: bool, listed: usize) -> bool {
    page.frames >= CLICK_AT && trigger && !open && page.clicked_at.is_none() && (listed > 0 || page.frames >= CLICK_AT + WAIT_FOR)
}

/// The original's column as MCM reads it, written into `StartMenu` when it was (re)built
/// (with the menu's `alpha` 255, its fade-in done): `childcount` 8, `_alpha` 255 and `_enabled` 1 on [`COLUMN`], and one code row of the
/// file's `lb_item_template` (`lb_item_hotrect`, `target` 1) under it [observed: menu-xml.md
/// §10.1, the pause menu's column; design: one row stands for the six, MCM reads only the
/// first's `target`]. Whether it wrote anything (a fresh tree).
pub fn prepare_column(ui: &mut Ui, files: Option<&dyn fnv_ui::Source>) -> bool {
    use fnv_ui::value::Value;
    let Some(column) = ui.find_tile(COLUMN) else { return false };
    if ui.tile(column).get("childcount").is_some() {
        return false;
    }
    // The menu faded in, as the code does for a menu it shows (its file starts it at 0;
    // `super::vats_menu::write` likewise) [design].
    if let Some(menu) = ui.menu(START_MENU) {
        ui.set_trait(menu, "alpha", Value::Num(255.0));
    }
    ui.set_trait(column, "childcount", Value::Num(8.0));
    ui.set_trait(column, "_alpha", Value::Num(255.0));
    ui.set_trait(column, "_enabled", Value::Num(1.0));
    if ui.find_tile(&format!("{COLUMN}/lb_item_hotrect")).is_none() {
        let empty = fnv_ui::MemSource::default();
        if let Some(row) = ui.add_code_tile_from_template(files.unwrap_or(&empty), COLUMN, "lb_item_template", None) {
            ui.set_trait(row, "target", Value::Num(1.0));
        }
    }
    true
}

/// MCM's hotrects that set their own `red`, `green` or `blue` marked to draw their box
/// filled (`super::layout::HOTRECT_FILL`): the slider window's `Scale_BG` and
/// `Title_BG`, the list window's `List_BG` (black, alpha 255) and the colour previews
/// `Scale_RGB` and each option's `RGB` [observed: MCM's Scale.xml, List.xml and
/// OptionRect.xml, which name the black ones backgrounds; the parity check of 2026-11-12
/// saw the page show through the slider window without it; hypothesis: the original
/// fills a hotrect that sets its own colour, as these files expect]. Only under MCM's
/// window: the vanilla menus' one such hotrect (the race menu's `RSM_Face_Grab`, red at
/// alpha 128) is a grab area, and hotrects elsewhere stay unfilled [design]. Marks each
/// tile once; a hotrect made later is marked on the next frame. MCM's two pop-up windows
/// are lifted over its option rows too (`super::layout::DEPTH_LIFT`, [`MCM_WINDOW_LIFT`]).
pub fn fill_mcm_boxes(ui: &mut Ui) -> usize {
    use fnv_ui::value::Value;
    let Some(root) = ui.find_tile(MCM) else { return 0 };
    for window in MCM_WINDOWS {
        if let Some(w) = ui.find_tile(window).filter(|&w| ui.tile(w).get(super::layout::DEPTH_LIFT).is_none()) {
            ui.set_trait(w, super::layout::DEPTH_LIFT, Value::Num(MCM_WINDOW_LIFT));
        }
    }
    let mut stack = vec![root];
    let mut marked = Vec::new();
    while let Some(id) = stack.pop() {
        let tile = ui.tile(id);
        if tile.kind == "hotrect"
            && tile.get(super::layout::HOTRECT_FILL).is_none()
            && ["red", "green", "blue"].iter().any(|n| tile.get(n).is_some())
        {
            marked.push(id);
        }
        stack.extend(tile.children.iter().copied());
    }
    for &id in &marked {
        ui.set_trait(id, super::layout::HOTRECT_FILL, Value::Num(1.0));
    }
    if !marked.is_empty() {
        info!("tiles: MCM: {} hotrects with their own colour draw their box filled", marked.len());
    }
    marked.len()
}

/// One frame of the start menu's scripts under the Mods page: the column made ready
/// ([`prepare_column`]), MCM's trigger clicked at [`CLICK_AT`] if its window is not open,
/// then only the `MenuMode 1013` blocks run (`Game::page_tick`; no GameMode block, no game
/// time) and the open menu is left alone, so a box a block shows stays open [design: the
/// original runs them under its start menu, as MCM's mods expect, menu-xml.md §10;
/// NVForever's pause menu otherwise holds every script, `crate::script`]. The caller puts
/// the played world in the game first (`crate::script::with_world`).
pub fn tick(game: &mut fnv_game::Game<'_>, page: &mut ModsPage) {
    if page.frames == 0 {
        *page = ModsPage::default();
        // JIP's menu-open handlers for the start menu, as the original's pause menu opens
        // (`Game::page_menu_event`); MCM Extender adds its menus to MCM's list there.
        let ran = game.page_menu_event(menus::START, true);
        let listed = game.data.by_edid(MCM_MODS).map(|l| fnv_game::functions::list_members(game, l).len());
        info!("tiles: the start menu's opening sent to {ran} JIP handlers; MCM's list of mods (MCMMods) holds {listed:?}");
    }
    let files = game.ui_files.as_deref().map(|f| f as &dyn fnv_ui::Source);
    if prepare_column(&mut game.ui_tree, files) {
        info!("tiles: {COLUMN}: the original's column traits written for MCM");
    }
    column(&mut game.ui_tree);
    fill_mcm_boxes(&mut game.ui_tree);
    let listed = game.data.by_edid(MCM_MODS).map_or(0, |l| fnv_game::functions::list_members(game, l).len());
    let ui = &mut game.ui_tree;
    let open = |ui: &Ui| ui.get_float(&format!("{MCM}/_Menu")) == Some(1.0);
    // The click is held until MCM's window is open, at most [`CLICK_FOR`] frames: MCM skips
    // a click in the frame after it reset itself (its `_UnClick`) [observed: MCM's script
    // source; the original opened with `_clicked` left at 1, menu-xml.md §10.1] (from
    // upstream ff5dbb106).
    // The trigger's `clicked`, as the pointer's click sets it: its `_clicked` is MCM.xml's
    // formula over `clicked` (less MCM's `_UnClick`), so a value written to `_clicked`
    // itself is replaced at the next update [observed: MCM.xml, `MCM_Trigger/_clicked`;
    // the smoke run of 2026-11-12 with `_clicked` written left the window shut]. The trait
    // is made if the tile has none yet, as the pointer's first click makes it.
    let press = |ui: &mut Ui, v: f64| {
        if let Some(t) = ui.find_tile(MCM_TRIGGER) {
            ui.set_trait(t, fnv_ui::input::CLICKED, fnv_ui::Value::Num(v));
        }
    };
    if click_now(page, ui.find_tile(MCM_TRIGGER).is_some(), open(ui), listed) {
        press(ui, 1.0);
        page.clicking = true;
        page.clicked_at = Some(page.frames);
        if listed > 0 {
            info!("tiles: MCM's trigger clicked: its window opens with the Mods page ({listed} mods in MCM's list, page frame {})", page.frames);
        } else {
            info!("tiles: MCM's trigger clicked with no mod in MCM's list after {WAIT_FOR} frames: its window opens with the Mods page");
        }
    } else if page.clicking && (open(ui) || page.clicked_at.is_some_and(|at| page.frames >= at + CLICK_FOR)) {
        if !open(ui) {
            let read = |p: &str| ui.get_float(p).map_or("none".to_string(), |v| v.to_string());
            bevy::prelude::warn!(
                "tiles: MCM's window did not open: MCM visible {}, its trigger visible {}, {COLUMN} childcount {}",
                read(&format!("{MCM}/visible")),
                read(&format!("{MCM_TRIGGER}/visible")),
                read(&format!("{COLUMN}/childcount"))
            );
        }
        press(ui, 0.0);
        page.clicking = false;
    }
    game.page_tick(menus::START);
    // MCM's slider window and how MCM dims the other rows with it (its OptionRect.xml
    // `_alpha`, 3 on a row that is not the one selected): told when any of it changes, as the
    // tree last worked it out (a frame behind a script's write).
    {
        let ui = &game.ui_tree;
        let read = |p: &str| ui.get_float(&format!("StartMenu/MCM/{p}")).map_or("none".to_string(), |v| v.to_string());
        let told = format!(
            "slider window visible {}: _selected {}, Option1 _alpha {} title alpha {} brightness {}, _optionID {} _enable {}",
            read("MCM_Scale/visible"),
            read("MCM_Options/_selected"),
            read("MCM_Options/Option1/_alpha"),
            read("MCM_Options/Option1/title/alpha"),
            read("MCM_Options/Option1/title/brightness"),
            read("MCM_Options/Option1/_optionID"),
            read("MCM_Options/Option1/_enable")
        );
        if told != page.dim_told {
            info!("tiles: MCM's {told}");
            page.dim_told = told;
        }
    }
    // The frames around the click, as MCM's scripts see them: the trigger's traits and the
    // quest variables its stage 2 and main script read (a window that stays shut says why).
    if (CLICK_AT..CLICK_AT + 4).contains(&page.frames) {
        let ui = &game.ui_tree;
        let read = |p: &str| ui.get_float(p).map_or("none".to_string(), |v| v.to_string());
        let data = game.data;
        let vars = data.by_edid("MCM").filter(|&q| data.is_quest(q)).and_then(|q| {
            let code = data.script(data.quests[&q].script?)?;
            let inst = fnv_game::InstKey::Quest { quest: q };
            Some(["iInputTrigger", "iMenu", "iInputHeld", "iInstallCheck"].map(|v| code.var_index(v).map_or("none".to_string(), |i| game.var(inst, i).num().to_string())).join(" "))
        });
        info!(
            "tiles: MCM frame {}: trigger clicked {} _clicked {}, _UnClick {}, _Menu {}; iInputTrigger iMenu iInputHeld iInstallCheck {}",
            page.frames,
            read(&format!("{MCM_TRIGGER}/clicked")),
            read(&format!("{MCM_TRIGGER}/_clicked")),
            read(&format!("{MCM}/_UnClick")),
            read(&format!("{MCM}/_Menu")),
            vars.unwrap_or_default()
        );
    }
    // Let go as soon as the window is open: a click held into the next frame is a second
    // click on the trigger, which closes MCM again (its script: `iMenu` 1 and the trigger
    // closes; the close resets its mod list's quest) [observed: MCM's script source].
    if page.clicking && open(&game.ui_tree) {
        press(&mut game.ui_tree, 0.0);
        page.clicking = false;
    }
    if open(&game.ui_tree) {
        if !page.mcm_open {
            info!("tiles: MCM's window is open on the Mods page");
        }
        page.mcm_open = true;
    } else if page.mcm_open && !page.closed {
        page.closed = true;
        info!("tiles: MCM's window closed: the Mods page closes");
    }
    page.frames += 1;
}

/// The start menu's main column, `StartMenu/NOGLOW_BRANCH/main_container`.
pub const COLUMN: &str = "StartMenu/NOGLOW_BRANCH/main_container";

/// The start menu as the game's code leaves it, written while the Mods page is up: the
/// menu faded in, and its main column's `childcount` 8, its eight entries, and `_alpha` 255, faded in. MCM shows its
/// trigger only on that count, and copies the trigger's place, size and fade from the
/// column (`x`, `y` and `width` come from the file's own expressions, `_enabled` is the
/// file's and MCM's own) [hypothesis, from MCMMainScript's MenuMode 1013 block: `if
/// GetUIFloat "StartMenu/NOGLOW_BRANCH/main_container/childcount" == 8`; start_menu.xml
/// says `_alpha` is "set in code by fade control"]. NVForever's own column is hand-built,
/// so the tile tree's stands in for it.
pub fn column(ui: &mut Ui) {
    // The menu faded in, as the game's code fades it (`<alpha> 0 </alpha>` and
    // `<explorefade> 0.25 </explorefade>` in start_menu.xml): every tile under it takes
    // the root's alpha (`super::layout`) [hypothesis: the fade ends at 255].
    if let Some(m) = ui.menu(START_MENU) {
        ui.set_trait(m, "alpha", fnv_ui::Value::Num(255.0));
        // MCM's tiles place themselves by `locus` (`fnv_ui::input::places_children`).
        ui.set_trait(m, fnv_ui::input::LOCUS_ON, fnv_ui::Value::Num(1.0));
        // And reads each click over two frames (`CLICK_TWO_FRAMES`).
        ui.set_trait(m, fnv_ui::input::CLICK_TWO_FRAMES, fnv_ui::Value::Num(1.0));
    }
    let Some(c) = ui.find_tile(COLUMN) else { return };
    ui.set_trait(c, "childcount", fnv_ui::Value::Num(8.0));
    ui.set_trait(c, "_alpha", fnv_ui::Value::Num(255.0));
    // MCM's trigger takes the column's `x` and `width`, and its `y` less one item's
    // height, as its own place (MCMMainScript), counted from the menu's top-left: the
    // `MCM` rect has no `locus`. NVForever's
    // column is hand-built elsewhere, so the column reports the place that puts the
    // trigger in the middle of the page, a third of the way down [design].
    let screen = ui.screen();
    let (sw, sh) = (ui.tile(screen).get("width").map_or(0.0, |v| v.num()), ui.tile(screen).get("height").map_or(0.0, |v| v.num()));
    let num = |p: &str| ui.get_float(p).unwrap_or(0.0);
    let width = num(&format!("{COLUMN}/width"));
    let item = num("StartMenu/_item_height");
    if sw > 0.0 && width > 0.0 {
        ui.set_trait(c, "x", fnv_ui::Value::Num((sw - (width - 5.0)) / 2.0));
        ui.set_trait(c, "y", fnv_ui::Value::Num(sh / 3.0 + item));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fnv_ui::MemSource;

    #[test]
    fn the_click_waits_for_a_mod_in_mcms_list() {
        let at = |frames| ModsPage { frames, ..Default::default() };
        assert!(!click_now(&at(CLICK_AT - 1), true, false, 1), "before the first frames are done");
        assert!(click_now(&at(CLICK_AT), true, false, 1), "a mod registered: clicked at once");
        assert!(!click_now(&at(CLICK_AT + 30), true, false, 0), "an empty list (after a load): waits");
        assert!(click_now(&at(CLICK_AT + 31), true, false, 1), "clicked once the mod registers");
        assert!(click_now(&at(CLICK_AT + WAIT_FOR), true, false, 0), "a list no mod fills: clicked after the wait");
        assert!(!click_now(&at(CLICK_AT), false, false, 1), "no trigger");
        assert!(!click_now(&at(CLICK_AT), true, true, 1), "the window already open");
        let clicked = ModsPage { frames: CLICK_AT + 5, clicked_at: Some(CLICK_AT), ..Default::default() };
        assert!(!click_now(&clicked, true, false, 1), "one click a page");
    }

    #[test]
    fn the_entry_needs_the_switch_and_a_mods_tiles() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let off = super::super::vanilla::TileMenus::default();
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        let mut kept = None;
        let mut ui = Ui::new();
        assert!(!available(on, &ui, None, &mut kept), "no start menu");
        // The vanilla file from the archive: no mod's tiles.
        let src = MemSource::default().with(path, vanilla);
        ui.load_menu(&src, path).unwrap();
        assert!(!available(on, &ui, Some(&src), &mut kept));
        // A mod's tile injected into it.
        assert!(ui.inject_text(&src, "StartMenu", r#"<rect name="MCM"/>"#));
        assert!(available(on, &ui, Some(&src), &mut kept));
        assert!(!available(off, &ui, Some(&src), &mut kept), "the switch off");
    }

    #[test]
    fn the_column_reports_the_originals_count_and_fade() {
        let src = MemSource::default().with(
            r"menus\options\start_menu.xml",
            r#"<menu name="StartMenu"> <alpha> 0 </alpha> <rect name="NOGLOW_BRANCH"> <hotrect name="main_container"> <_alpha></_alpha> <_enabled> 1 </_enabled> </hotrect> </rect> </menu>"#,
        );
        let mut ui = Ui::new();
        column(&mut ui);
        ui.load_menu(&src, r"menus\options\start_menu.xml").unwrap();
        column(&mut ui);
        assert_eq!(ui.get_float("StartMenu/alpha"), Some(255.0), "faded in");
        assert_eq!(ui.get_float(&format!("{COLUMN}/childcount")), Some(8.0));
        assert_eq!(ui.get_float(&format!("{COLUMN}/_alpha")), Some(255.0));
        assert_eq!(ui.get_float(&format!("{COLUMN}/_enabled")), Some(1.0), "MCM's own trait is left alone");
    }

    #[test]
    fn mcms_coloured_hotrects_draw_filled_and_no_others() {
        let src = MemSource::default().with(
            r"menus\options\start_menu.xml",
            r#"<menu name="StartMenu"> <hotrect name="Other"> <red> 0 </red> </hotrect>
              <rect name="MCM"> <rect name="MCM_Scale"> <hotrect name="Scale_BG"> <alpha> 255 </alpha> <red> 0 </red> <green> 0 </green> <blue> 0 </blue> </hotrect>
                <hotrect name="Scale_Accept"> <target> 1 </target> </hotrect> </rect> </rect> </menu>"#,
        );
        let mut ui = Ui::new();
        assert_eq!(fill_mcm_boxes(&mut ui), 0, "no MCM");
        ui.load_menu(&src, r"menus\options\start_menu.xml").unwrap();
        assert_eq!(fill_mcm_boxes(&mut ui), 1);
        let fill = |p: &str| ui.get_float(&format!("StartMenu/{p}/{}", super::super::layout::HOTRECT_FILL));
        assert_eq!(fill("MCM/MCM_Scale/Scale_BG"), Some(1.0));
        assert_eq!(fill("MCM/MCM_Scale/Scale_Accept"), None, "no colour of its own");
        assert_eq!(fill("Other"), None, "outside MCM's window");
        assert_eq!(ui.get_float(&format!("StartMenu/MCM/MCM_Scale/{}", super::super::layout::DEPTH_LIFT)), Some(MCM_WINDOW_LIFT), "the slider window lifted");
        assert_eq!(fill_mcm_boxes(&mut ui), 0, "marked once");
    }

    #[test]
    fn a_loose_reskin_of_the_start_menu_is_no_mods_page() {
        let on = super::super::vanilla::TileMenus { start: true, ..Default::default() };
        let path = r"menus\options\start_menu.xml";
        let vanilla = r#"<menu name="StartMenu"> <rect name="Panel"/> </menu>"#;
        // A UI overhaul's loose copy: the same tiles, other looks.
        let reskin = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"> <width>300</width> </rect> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&reskin, path).unwrap();
        assert!(ui.has_mod_added_tiles(ui.menu(START_MENU).unwrap()), "the loose file's tiles count as loose");
        assert!(!available(on, &ui, Some(&reskin), &mut None));
        // A loose copy that adds a tile of its own.
        let added = MemSource::default().with_loose_over(path, r#"<menu name="StartMenu"> <rect name="Panel"/> <rect name="ModPage"/> </menu>"#, vanilla);
        let mut ui = Ui::new();
        ui.load_menu(&added, path).unwrap();
        assert!(available(on, &ui, Some(&added), &mut None));
    }
}
