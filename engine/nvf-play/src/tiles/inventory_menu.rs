//! The Pip-Boy's ITEMS page drawn from `menus\main\inventory_menu.xml`, behind
//! `[UI] TileMenus=InventoryMenu` [design: ui-tile-tree.md §6.2 step 3; the contract:
//! docs/spec-notes/pipboy.md §5.2.1; issues #1513, #1468]. Off (the default), none of this runs
//! and the hand-built page (`crate::pipboy::items`) draws.
//!
//! - **The one decision** ([`InventoryTiles::ready`]): the switch on, ITEMS showing with the repair
//!   list and the mod menu closed, the menu files read, the file's tiles there ([`built`]), the
//!   tabs made and a usable measured layout ([`usable`]). Painting (`super::draw`), hand-built
//!   suppression (`crate::pipboy::frame::draw`), the prompt and input read it and nothing else,
//!   and [`settle`] publishes it each frame before any of them. A pass that fails leaves the whole
//!   hand-built page, says why once, and is tried again the next frame.
//! - **The rules stay the hand-built page's** (`crate::menu::model`, `crate::pipboy::items`,
//!   `crate::pipboy::repair`, `crate::pipboy::item_mod`, `crate::pipboy::hotkey_panel`). This module
//!   writes values as the file's "set in code" traits and says what a tile stands for; commands,
//!   sounds and events go where the hand-built page sends them. Only the list's scroll is its own
//!   (`super::item_menu`), which the model never had in units.
//! - **The list** is `super::item_menu`'s with one side: the container's path is empty, which no
//!   tile answers to. A scrolled list stands partly outside its window by design, so [`usable`]
//!   asks of the rows whose words lie whole inside it, and of at least one.
//! - **The tabs** are template rows here, unlike STATS' fixed tiles [documented: the file]. Their
//!   words are [`TAB_WORDS`], NVForever's own. Five tiles from one template have no place of their
//!   own, and a first run found each tab's words under another tab or no target [observed: smoke
//!   2026-11-07, `Tab(1) does not take the pointer`; which of the two is not known yet]. So
//!   [`place_tabs`] stands them along the line as the hand-built tab line does and makes them
//!   targets [design]. A tab that does not show is a fallback; one that still does not take the
//!   pointer is warned of and the page stays: the keys and the device's knob still turn the tabs.
//! - **Page content only**, as `super::stats_menu`: the page paints into the Pip-Boy's image, whose
//!   background and scanlines are the shell's, so `IM_Background` stays hidden in both frames.
//!   The screen menu has no title and its heading line runs on [design: the owner, 2026-10-31,
//!   `stats_menu::write_frame_of`].
//! - **The hotkey panel** shows while a hotkey's key is held [design: the owner 2026-10-26, `hotkey_panel`
//!   module notes]; the Hotkey button stays hidden, as the file's own way into the panel is not
//!   this game's. The panel stands at the file's place behind the switch, not the 420 the owner
//!   chose for the hand-built panel (`hotkey_panel::PANEL`) [design; said in the PR].
//! - **The repair list and the mod menu** have their own files, not read here: while one is open
//!   the page is not eligible and the hand-built body draws it, as it does today.
//! - **The prompt** is `QuantityMenu`'s while the page is ready (`super::menus::quantity_tiled`),
//!   drawn on the window; [`feed`] takes nothing under it.
//! - **Input** ([`feed`], before the scripts): the pointer is the Pip-Boy's own, in canvas units
//!   (`crate::pipboy::PipBoyScreen::pointer`), read and never written here. A row the pointer
//!   moves onto is selected unless the wheel holds the selection; a click on a row sends what the
//!   hand-built row sends (`items::on_row_click`: `Command::Select`, or with a hotkey's key held
//!   the assignment), a right click the hand-built drop [owner 2026-11-02, #1393]. The keys and
//!   the wheel stay `items::keys`' and `pipboy::keys`' ([`HAND_BUILT_KEYS`]); a file's binding acts
//!   for a script's tap, not for a physical press.
//! - **Measuring**: with `NVF_PLAY_TEST_TILE_COUNTS` a line a frame for each pass, `tile counts:
//!   InventoryMenu pass=<early|late> writes=<n> changed=<n> micros=<n>`, the late one with
//!   `rows=<n> rows_in_clip=<n> hit_tests=<n> candidates=<n> pieces=<n>` (`super` module notes).

use super::draw::{Look, Piece};
use super::input::{frame_keys, log_report, Devices};
use super::item_card;
use super::item_menu::{self, on, ListShown, MenuTiles, Paths, Snapshot};
use super::list::{self, BarPiece, RowTile, Wanted};
use super::menus::{self, SizesWritten, Tiled};
use super::stats_menu::{self, find_named, modal_over, shows_whole, FlatChanges, PageFonts};
use super::vanilla::TileMenus;
use crate::hotkeys::{Hotkeys, AMMO_SWAP, SLOTS};
use crate::menu::model::{Command, MenuModel, Mode, Side};
use crate::menu::{ActiveMenu, MenuInput};
use crate::pipboy::hotkey_panel::{self, HotkeyHold};
use crate::pipboy::item_mod::{ModInput, PipItemMod};
use crate::pipboy::items::{self, Info};
use crate::pipboy::repair::{self, PipRepair, RepairInput};
use crate::pipboy::sounds::TabHovered;
use crate::pipboy::stats::read::Forms;
use crate::pipboy::{Frame, Page, PipBoy, PipBoyAid, PipBoyInput, PipBoyScreen};
use crate::script::ScriptState;
use bevy::ecs::system::SystemParam;
use bevy::prelude::*;
use fnv_sim::forms::ItemKind;
use fnv_sim::FormId;
use fnv_ui::{TileId, Ui, Value};
use std::collections::{HashMap, HashSet};

/// The menu's name [documented: inventory_menu.xml].
pub const INVENTORY_MENU: &str = "InventoryMenu";
/// Test aid: a tile or template of the file to take as missing, as `stats_menu::CUT_AID`:
/// `<name>`, or `<name>@<seconds>` to stop at that time, for the page's recovery.
pub const CUT_AID: &str = "NVF_PLAY_TEST_INVENTORY_TILE_CUT";

macro_rules! glow {
    ($path:literal) => {
        concat!("InventoryMenu/GLOW_BRANCH/", $path)
    };
}
macro_rules! main_rect {
    ($path:literal) => {
        concat!("InventoryMenu/GLOW_BRANCH/IM_MainRect/", $path)
    };
}

// The tiles the model writes and the input reads, by path [documented: inventory_menu.xml].
const BACKGROUND: &str = glow!("IM_Background");
const TABLINE: &str = glow!("IM_Tabline");
const TITLE: &str = main_rect!("IM_HeadlineRect/IM_Headline_Title");
const LINE: &str = main_rect!("IM_HeadlineRect/IM_Headline_h2");
/// Wg, HP, DT and Caps, as [`Shown::figures`].
const HEADLINE: [&str; 4] = [
    main_rect!("IM_HeadlineRect/IM_Headline_PlayerWGInfo"),
    main_rect!("IM_HeadlineRect/IM_Headline_PlayerHPInfo"),
    main_rect!("IM_HeadlineRect/IM_Headline_PlayerDTInfo"),
    main_rect!("IM_HeadlineRect/IM_Headline_PlayerCapsInfo"),
];
/// DR, which this game's page has no card for [documented: `crate::pipboy::items`' heading]; a
/// file without it serves.
const HEADLINE_DR: &str = main_rect!("IM_HeadlineRect/IM_Headline_PlayerDRInfo");
const LIST: &str = main_rect!("IM_InventoryList");
/// A picture under the list, not in a row: what it shows is not known yet, so it stays hidden
/// [hypothesis until the capture].
const EQUIP_MARKER: &str = main_rect!("IM_InventoryList/IM_EquipItemMarker");
const WHEEL: &str = main_rect!("IM_HotKeyWheel");
const WHEEL_ICON: &str = main_rect!("IM_HotKeyWheel/IM_HotkeyIcon");
const BUTTON_RECT: &str = main_rect!("IM_ButtonRect");
const ICON: &str = main_rect!("IM_ItemIcon");
const BADGE: &str = main_rect!("IM_ItemIconBadge");
const SKILL_REQ: &str = main_rect!("IM_StrReq");
const CARD: &str = main_rect!("IM_ItemInfoRect");
const TEMPLATE: &str = "IM_InventoryListTemplate";
const ROW: &str = "IM_InventoryListTemplateRect";
const ROW_TEXT: &str = "ListItemText";
const ROW_MARK: &str = "IM_Template_ItemMarker";
const TAB_TEMPLATE: &str = "TabButtonTemplate";
/// The tile a tab's template makes [documented: the file]; it keeps this name [design until the
/// capture says what the original names its tabs].
const TAB_ROW: &str = "TabButton";
/// Inside a tab, as STATS' fixed tabs have them [hypothesis for the template's: the same prefab].
const TAB_INNER: &str = "stb_text_box";
const TAB_TEXT: &str = "button_text";
/// The hotkey slots' tiles: every slot's but Ammo Swap's, which has its own picture in the file
/// [documented: the file names `HK_Item_` 0 and 2 to 7, and `IM_AmmoSwap`].
const SLOT_PREFIX: &str = "HK_Item_";

/// The tabs' words: the kind of item each holds, in NVForever's own words [design: LEGAL.md; no
/// setting of the load order is known for them, and the strings table has none, as no menu file
/// names one; capture question Q10]. `crate::pipboy::Page::tabs` holds what was read off the
/// original's screen for the hand-built page: those are not reused here.
pub const TAB_WORDS: [&str; 5] = ["Weapons", "Armor", "Aid", "Misc", "Ammo"];

/// The buttons, by their tiles' names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Button {
    Equip,
    Drop,
    Repair,
    Hotkey,
    Mod,
    Cancel,
}

const BUTTONS: [(&str, Button); 6] = [("IM_EquipButton", Button::Equip), ("IM_DropButton", Button::Drop), ("IM_RepairButton", Button::Repair), ("IM_HotkeyButton", Button::Hotkey), ("IM_ModButton", Button::Mod), ("IM_CancelButton", Button::Cancel)];

/// The list's paths for `super::item_menu`: one list, on the player's side. Clicks are told apart
/// here ([`items_tile`]), not by a table of names there.
pub(super) const PATHS: Paths = Paths { menu: INVENTORY_MENU, lists: ["", LIST], titles: ["", ""], template: TEMPLATE, row: ROW, row_text: ROW_TEXT, row_mark: ROW_MARK, targets: &[] };

fn button_path(name: &str) -> String {
    format!("{BUTTON_RECT}/{name}")
}

fn slot_path(slot: usize) -> String {
    format!("{WHEEL}/{SLOT_PREFIX}{slot}")
}

/// Every path the model writes or input reads.
fn required_paths() -> Vec<String> {
    let mut paths: Vec<String> = [INVENTORY_MENU, BACKGROUND, TABLINE, TITLE, LINE, LIST, WHEEL, WHEEL_ICON, BUTTON_RECT, ICON, BADGE, SKILL_REQ, CARD].iter().map(|s| (*s).to_string()).collect();
    paths.extend(HEADLINE.iter().map(|s| (*s).to_string()));
    paths.push(format!("{LIST}/{}", list::HIGHLIGHT));
    for piece in [list::MARKER, list::UP, list::DOWN, list::PAGE_UP, list::PAGE_DOWN] {
        paths.push(format!("{LIST}/{}/{piece}", list::SCROLLBAR));
    }
    for (name, button) in BUTTONS {
        // The Hotkey button stays hidden: a file without it serves.
        if button != Button::Hotkey {
            paths.push(button_path(name));
            paths.push(format!("{}/button_text", button_path(name)));
        }
    }
    paths.extend((0..SLOTS).filter(|&s| s != AMMO_SWAP).map(slot_path));
    paths.extend(item_card::CARDS.iter().chain(&[item_card::EFFECTS, item_card::CONDITION_ARROWS, item_card::CONDITION_METER, item_card::CONDITION_METER_IMAGE]).map(|c| format!("{CARD}/{c}")));
    for card in item_card::CARDS.iter().chain(&[item_card::EFFECTS]) {
        paths.extend(item_card::CARD_TEXTS.map(|t| format!("{CARD}/{card}/{t}")));
    }
    paths
}

/// Whether the file has every tile and template the page needs; a file a mod restructured keeps
/// the hand-built page whole. `cut`: [`CUT_AID`].
pub fn built(ui: &Ui, cut: Option<&str>) -> Tiled {
    let paths = required_paths();
    if let Some(cut) = cut {
        if cut.eq_ignore_ascii_case(TEMPLATE) || cut.eq_ignore_ascii_case(TAB_TEMPLATE) {
            return Tiled::No(format!("{cut} missing"));
        }
        if let Some(path) = paths.iter().find(|p| p.rsplit('/').next().is_some_and(|n| n.eq_ignore_ascii_case(cut))) {
            return Tiled::No(format!("{path} missing"));
        }
    }
    let list_built = item_menu::built(ui, &PATHS, paths, &[("text", ROW_TEXT), ("image", ROW_MARK)]);
    if !list_built.yes() {
        return list_built;
    }
    let tab_template = ui.menu(INVENTORY_MENU).is_some_and(|m| ui.tile(m).templates.iter().any(|t| t.name.eq_ignore_ascii_case(TAB_TEMPLATE)));
    if !tab_template {
        return Tiled::No(format!("{TAB_TEMPLATE} missing"));
    }
    Tiled::Yes
}

/// The hotkey panel as shown.
#[derive(Clone, Debug, PartialEq)]
pub struct Panel {
    /// The held key's slot (0 is key 1).
    pub slot: usize,
    /// The line under it (`hotkey_panel::panel_line`).
    pub text: String,
    /// Each slot's icon (`hotkey_panel::icon_of`), none when empty.
    pub boxes: [Option<&'static str>; SLOTS],
    /// The selected row's icon, in the panel's middle.
    pub centre: Option<&'static str>,
}

/// The keys' hints beside the buttons' words (`_PCButtonText`), as the buttons' order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Hints {
    pub equip: String,
    pub drop: String,
    pub repair: String,
    pub modify: String,
    pub cancel: String,
}

/// What the page shows of the model, as written last. A plain snapshot, so tests need no world.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    /// The list, the item's picture and its figures.
    pub list: Snapshot,
    pub tab: usize,
    pub device: bool,
    /// Wg, HP, DT and Caps.
    pub figures: [String; 4],
    pub equip: bool,
    pub drop: bool,
    /// The Repair button: shown, and whether R does anything then (dim if not).
    pub repair: Option<bool>,
    pub modify: bool,
    /// The selected row can be worn, wielded or used [hypothesis: the root's `_EquippableItem`].
    pub equippable: bool,
    /// A Misc folder is open [hypothesis: the root's `_KeyringOpen`].
    pub keyring: bool,
    pub hints: Hints,
    pub panel: Option<Panel>,
}

impl Shown {
    /// What `m` shows; `card` the selected row's figures, without DPS as on the hand-built page.
    #[allow(clippy::too_many_arguments)]
    pub fn of(m: &MenuModel, card: Option<Info>, tab: usize, device: bool, figures: [String; 4], repair: Option<bool>, hints: Hints, panel: Option<Panel>) -> Shown {
        let selected = m.selected_row(Side::Player);
        let list = ListShown {
            rows: item_menu::rows_shown(&m.list(Side::Player).rows),
            category: m.category[Side::Player as usize],
            selected: selected.map(|_| m.selected[Side::Player as usize]),
            filtered: m.filtered[Side::Player as usize],
        };
        let modify = card.as_ref().is_some_and(|c| c.mod_hint);
        Shown {
            list: Snapshot {
                lists: [ListShown::default(), list],
                titles: Default::default(),
                weight: String::new(),
                focus: Side::Player,
                pointer_pick: m.pointer_pick,
                asking: m.quantity.is_some(),
                navigated: m.navigated,
                card,
                icon: selected.and_then(|r| r.icon.clone()),
                take_all: false,
                take_all_hint: String::new(),
                note: None,
            },
            tab,
            device,
            figures,
            equip: selected.is_some(),
            drop: selected.is_some_and(|r| !r.quest && r.folder.is_none()),
            repair,
            modify,
            equippable: selected.is_some_and(|r| matches!(r.kind, Some(ItemKind::Weapon | ItemKind::Armor | ItemKind::Aid))),
            keyring: m.folder.is_some(),
            hints,
            panel,
        }
    }
}

/// Sets a trait by path as the game's code does (`Ui::write_changed`): what a script set
/// meanwhile stands until the model changes.
fn put(ui: &mut Ui, path: &str, name: &str, v: Value) {
    if let Some(t) = ui.find_tile(path) {
        ui.write_changed(t, name, v);
    }
}

fn text(s: &str) -> Value {
    Value::Str(s.to_string())
}

/// Write what the page shows of `s`. A list's places are `item_menu::settle`'s, once the texts
/// are measured.
pub fn write_model(ui: &mut Ui, s: &Shown, view: &mut MenuTiles) {
    item_menu::write_lists(ui, &s.list, view);
    put(ui, BACKGROUND, "visible", on(false));
    put(ui, HEADLINE_DR, "visible", on(false));
    put(ui, EQUIP_MARKER, "visible", on(false));
    for (card, figure) in HEADLINE.iter().zip(&s.figures) {
        put(ui, card, "_Value", text(figure));
    }
    put(ui, INVENTORY_MENU, "_EquippableItem", on(s.equippable));
    put(ui, INVENTORY_MENU, "_KeyringOpen", on(s.keyring));
    // The panel hides the picture, the badge, the skill and the buttons; the figures stay
    // [documented: `crate::pipboy::hotkey_panel`'s notes].
    let panel = s.panel.is_some();
    let card = s.list.card.as_ref();
    put(ui, ICON, "filename", text(s.list.icon.as_deref().unwrap_or_default()));
    put(ui, ICON, "visible", on(!panel && s.list.icon.is_some()));
    let badge = card.and_then(|c| c.badge.as_deref());
    put(ui, BADGE, "filename", text(badge.unwrap_or_default()));
    put(ui, BADGE, "visible", on(!panel && badge.is_some()));
    let skill = card.and_then(|c| c.skill_req);
    put(ui, SKILL_REQ, "string", Value::Str(skill.map_or_else(String::new, |n| n.to_string())));
    put(ui, SKILL_REQ, "visible", on(!panel && skill.is_some()));
    item_card::write(ui, CARD, card);
    write_buttons(ui, s);
    write_panel(ui, s.panel.as_ref());
}

/// The buttons' `visible` and hints; their words are the file's.
fn write_buttons(ui: &mut Ui, s: &Shown) {
    for (name, button) in BUTTONS {
        let (shown, hint) = match button {
            Button::Equip => (s.equip, &s.hints.equip),
            Button::Drop => (s.drop, &s.hints.drop),
            Button::Repair => (s.repair.is_some(), &s.hints.repair),
            Button::Mod => (s.modify, &s.hints.modify),
            Button::Cancel => (true, &s.hints.cancel),
            // The hotkeys are held digits [design: the owner 2026-10-26, `hotkey_panel`].
            Button::Hotkey => {
                put(ui, &button_path(name), "visible", on(false));
                continue;
            }
        };
        let path = button_path(name);
        put(ui, &path, "visible", on(shown));
        put(ui, &path, "_PCButtonText", text(hint));
    }
    // Dim while R does nothing, as the hand-built hint (`repair::draw_hint`); the text inherits
    // no parent's brightness [hypothesis: as barter's Accept, `super::barter_menu`].
    let repair = button_path("IM_RepairButton");
    let brightness = if s.repair == Some(false) { 128.0 } else { 255.0 };
    for path in [repair.clone(), format!("{repair}/button_text"), format!("{repair}/PCShortcutLabel")] {
        put(ui, &path, "brightness", Value::Num(brightness));
    }
}

/// The hotkey panel's traits [hypothesis until the capture: what the file reads of them, Q14].
fn write_panel(ui: &mut Ui, panel: Option<&Panel>) {
    put(ui, WHEEL, "visible", on(panel.is_some()));
    put(ui, BUTTON_RECT, "visible", on(panel.is_none()));
    let Some(p) = panel else { return };
    put(ui, WHEEL, "_SelectedHotkey", Value::Num(p.slot as f64));
    put(ui, WHEEL, "_SelectedText", text(&p.text));
    for slot in (0..SLOTS).filter(|&s| s != AMMO_SWAP) {
        let path = slot_path(slot);
        put(ui, &path, "_HotKeyIcon", text(p.boxes[slot].unwrap_or_default()));
        put(ui, &path, "_HotKeyNumber", Value::Num(slot as f64 + 1.0));
        put(ui, &path, "_HotKeyAssigned", on(p.boxes[slot].is_some()));
    }
    put(ui, WHEEL_ICON, "filename", text(p.centre.unwrap_or_default()));
    put(ui, WHEEL_ICON, "visible", on(p.centre.is_some()));
}

/// The five tabs made from their template, each kept for good; whether any was made.
fn ensure_tabs(ui: &mut Ui, tabs: &mut Vec<RowTile<usize>>) -> Result<bool, String> {
    if tabs.len() == TAB_WORDS.len() && tabs.iter().all(|t| ui.is_live(t.tile)) {
        return Ok(false);
    }
    let keys = [0, 1, 2, 3, 4];
    let wanted: Vec<Wanted<'_, usize>> = keys.iter().map(|key| Wanted { key, name: TAB_ROW }).collect();
    if !list::reconcile(ui, TABLINE, TAB_TEMPLATE, tabs, &wanted) {
        return Err(format!("{TAB_TEMPLATE} made no tab"));
    }
    ui.update();
    Ok(true)
}

/// What the tab line is told, and the same marks as STATS' tabs have (`stats_menu::write_tabs`):
/// the tab gives its children no place. The word goes on the tab and on its text: which of them
/// the template reads is [hypothesis until the capture, Q9].
fn write_tabs(ui: &mut Ui, tabs: &[RowTile<usize>], tab: usize) {
    put(ui, TABLINE, "_ButtonCount", Value::Num(TAB_WORDS.len() as f64));
    put(ui, TABLINE, "_CurrentTab", Value::Num(tab as f64));
    put(ui, TABLINE, "_enabled", on(true));
    for row in tabs {
        ui.write_changed(row.tile, "_enabled", on(true));
        let word = text(TAB_WORDS[row.key]);
        ui.write_changed(row.tile, "string", word.clone());
        if let Some(t) = find_named(ui, row.tile, TAB_TEXT) {
            ui.write_changed(t, "string", word);
        }
        if find_named(ui, row.tile, TAB_INNER).is_some() {
            ui.write_changed(row.tile, fnv_ui::input::NO_ORIGIN, Value::Num(1.0));
        }
    }
}

fn num(ui: &Ui, t: TileId, name: &str) -> f64 {
    ui.tile(t).get(name).map_or(0.0, Value::num)
}

/// Puts a value right that the file may work out otherwise, on every pass, writing only when it
/// differs, as `stats_menu::write_frame_of` does for the heading's line.
fn keep(ui: &mut Ui, t: TileId, name: &str, want: f64) {
    if ui.tile(t).get(name).map(Value::num) != Some(want) {
        ui.set_trait(t, name, Value::Num(want));
    }
}

/// Stands the tabs along the line by their measured words, the hand-built tab line's rule
/// (`frame::tab_boxes`, even lines between them), and makes them targets, the file leaving both to
/// code [hypothesis: nothing in a tile from one template tells it from its sisters but
/// `listindex`]. A tab and its inner rect repeat one place (module `stats_menu::write_tabs`), so
/// both get it; with the mark `write_tabs` gives, the two are not added. `y` stays the file's.
fn place_tabs(ui: &mut Ui, tabs: &[RowTile<usize>]) {
    let Some(line) = ui.find_tile(TABLINE) else { return };
    let widths: Vec<f32> = tabs.iter().map(|r| find_named(ui, r.tile, TAB_TEXT).map_or(0.0, |t| num(ui, t, "width")) as f32).collect();
    let (boxes, gap) = crate::pipboy::frame::tab_boxes(&widths);
    let mut left = 0.0;
    let mut at = Some(line);
    while let Some(t) = at {
        left += num(ui, t, "x");
        at = ui.tile(t).parent;
    }
    keep(ui, line, "_LeftLineLength", f64::from(gap));
    for (row, b) in tabs.iter().zip(&boxes) {
        let x = f64::from(b.min.x) - left;
        keep(ui, row.tile, "x", x);
        if let Some(inner) = find_named(ui, row.tile, TAB_INNER) {
            keep(ui, inner, "x", x);
        }
        keep(ui, row.tile, "target", 1.0);
    }
}

/// Where the tabs stand as the tree holds them, for the log when it changes.
fn tab_places(ui: &Ui, tabs: &[RowTile<usize>]) -> String {
    let n = |x: f64| Value::Num(x).text();
    let places: Vec<String> = tabs.iter().map(|r| format!("{} at {}, {} size {} x {} target {}", r.key, n(num(ui, r.tile, "x")), n(num(ui, r.tile, "y")), n(num(ui, r.tile, "width")), n(num(ui, r.tile, "height")), n(num(ui, r.tile, "target")))).collect();
    places.join("; ")
}

/// What [`usable`] went through, for the frame's counts.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Checked {
    pub rows: usize,
    pub rows_in_clip: usize,
    pub hit_tests: usize,
    pub candidates: usize,
    /// The first tab that did not take the pointer, and what did: warned of, not a fallback.
    pub tab_missed: Option<String>,
}

/// Whether what is about to be painted, `pieces`, can be used, on every final pass, so what a
/// script did since is seen: every tab's words among the pieces, whole on the canvas and inside
/// their clips; the rows whose words lie whole on the canvas and inside the list's window, at
/// least one when there are rows, and these taking the pointer at their middles, whatever else
/// came to lie over them. A row wholly or partly outside the window is passed over: a scrolled
/// list has those by design [design: plan §5]. A list that scrolls must have a window. The tabs but
/// the one showing are asked the same, and one that does not take the pointer is told in
/// [`Checked::tab_missed`]: the page does without its click, the keys and the knob turning the tabs
/// [design]. The hit test is one walk of the menu however many rows there are.
pub fn usable(ui: &Ui, menu: TileId, view: &MenuTiles, tabs: &[RowTile<usize>], tab: usize, pieces: &[Piece]) -> Result<Checked, String> {
    let is_text = |x: &&Piece| matches!(x.look, Look::Text { .. });
    let texts: HashMap<TileId, &Piece> = pieces.iter().filter(is_text).map(|x| (x.tile, x)).collect();
    let rows = &view.rows[Side::Player as usize];
    let scrolls = ui.find_tile(LIST).and_then(|l| list::child(ui, l, list::SCROLLBAR)).and_then(|b| ui.tile(b).get("_number_of_items").map(Value::num)).is_some_and(|n| n > 1.0);
    let mut wanted: Vec<(ItemsTile, (f64, f64))> = Vec::with_capacity(rows.len() + tabs.len());
    for (i, row) in rows.iter().enumerate() {
        let Some(piece) = list::child(ui, row.tile, ROW_TEXT).and_then(|t| texts.get(&t)) else { continue };
        if scrolls && piece.clip.is_none() {
            return Err("the list scrolls and has no window".into());
        }
        if let Some(middle) = shows_whole(piece) {
            wanted.push((ItemsTile::Row(i), middle));
        }
    }
    let rows_in_clip = wanted.len();
    if !rows.is_empty() && rows_in_clip == 0 {
        return Err("no row shows whole".into());
    }
    for row in tabs {
        // A tab's words, as they are painted.
        let piece = pieces.iter().filter(is_text).find(|x| stats_menu::inside(ui, x.tile, row.tile)).ok_or_else(|| format!("tab {} is not shown", row.key))?;
        let middle = shows_whole(piece).ok_or_else(|| format!("tab {} does not fit", row.key))?;
        // The tab showing need not take it: a click on it does nothing.
        if row.key != tab {
            wanted.push((ItemsTile::Tab(row.key), middle));
        }
    }
    let found = fnv_ui::input::hit_candidates(ui, menu, &fnv_ui::input::no_measure);
    let mut tab_missed = None;
    for &(what, at) in &wanted {
        let hit = fnv_ui::input::top_hit(ui, &found, at);
        if hit.and_then(|h| items_tile(ui, view, tabs, h.tile)) != Some(what) {
            let there = hit.map_or_else(|| "nothing".to_string(), |h| ui.path_of(h.tile));
            let line = format!("{what:?} does not take the pointer at ({:.0}, {:.0}): {there} does", at.0, at.1);
            match what {
                ItemsTile::Tab(_) => tab_missed = tab_missed.or(Some(line)),
                _ => return Err(line),
            }
        }
    }
    Ok(Checked { rows: rows.len(), rows_in_clip, hit_tests: wanted.len(), candidates: found.len(), tab_missed })
}

/// What a tile of the page stands for, by the tile or a parent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemsTile {
    Row(usize),
    Tab(usize),
    Button(Button),
    Bar(BarPiece),
}

/// What `tile` stands for: a tab or a button, else what the list makes of it.
pub fn items_tile(ui: &Ui, view: &MenuTiles, tabs: &[RowTile<usize>], tile: TileId) -> Option<ItemsTile> {
    let mut at = Some(tile);
    while let Some(t) = at {
        if let Some(row) = tabs.iter().find(|r| r.tile == t) {
            return Some(ItemsTile::Tab(row.key));
        }
        if let Some((_, button)) = BUTTONS.iter().find(|(name, _)| ui.tile(t).name.eq_ignore_ascii_case(name)) {
            return Some(ItemsTile::Button(*button));
        }
        at = ui.tile(t).parent;
    }
    match view.clicked(ui, tile)? {
        item_menu::MenuTile::Row(Side::Player, i) => Some(ItemsTile::Row(i)),
        item_menu::MenuTile::Bar(Side::Player, piece) => Some(ItemsTile::Bar(piece)),
        _ => None,
    }
}

/// What [`page`] keeps: the tabs, what was last written, and the reasons warned of.
#[derive(Default)]
pub(crate) struct Written {
    menu: Option<TileId>,
    /// The tiles [`built`] looked at, while the file is known to have them.
    checked: Vec<TileId>,
    tabs: Vec<RowTile<usize>>,
    shown: Option<Shown>,
    flat: Option<FlatChanges>,
    /// What is shown changed since the log last said so.
    tell: bool,
    /// The panel's key last logged.
    panel_told: Option<usize>,
    /// The Pip-Boy was open at the last pass: an opening starts the list at its top.
    was_open: bool,
    /// What the last final pass went through.
    counted: Checked,
    /// The tabs' places last logged.
    places_told: Option<String>,
    /// Final passes in a row that failed: a state that lasts a frame or two is no fallback to warn of.
    failing: u32,
    fallbacks: HashSet<String>,
}

/// The decision every reader shares, and what the page keeps.
#[derive(Resource)]
pub struct InventoryTiles {
    /// The menu, while the page is ready to display this frame ([`settle`]).
    pub ready: Option<TileId>,
    /// The menu painted by the last drawing (`super::draw`): what the player sees.
    pub shown: Option<TileId>,
    /// The pieces [`usable`] checked this frame, for the drawing to paint: one measured layout, so
    /// what was checked is what is painted.
    pub(crate) pieces: Vec<Piece>,
    /// The list's rows, scroll and drag (`super::item_menu`).
    pub view: MenuTiles,
    /// [`CUT_AID`].
    pub cut: Option<String>,
    /// [`CUT_AID`]'s time.
    pub cut_until: Option<f32>,
    /// A menu over the page when this frame began (`stats_menu::modal_over`): set before the
    /// scripts, so it holds through the frame that menu closes in.
    pub modal: bool,
    /// Another menu took this frame's tile input (`super::input::feed`).
    pub others: bool,
    written: Written,
}

impl Default for InventoryTiles {
    fn default() -> Self {
        InventoryTiles { ready: None, shown: None, pieces: Vec::new(), view: MenuTiles::new(&PATHS), cut: None, cut_until: None, modal: false, others: false, written: Written::default() }
    }
}

/// The tiles of [`required_paths`], or why the page cannot be drawn.
fn check_file(ui: &Ui, cut: Option<&str>) -> Result<Vec<TileId>, String> {
    match built(ui, cut) {
        Tiled::No(reason) => Err(reason),
        Tiled::Yes => Ok(required_paths().iter().filter_map(|p| ui.find_tile(p)).collect()),
    }
}

/// One pass over the page; the menu when all of it went through. `new`: what the model shows, when
/// it may have changed. `late`: the pass before the drawing; the other, before the scripts, does
/// the sizes only for a model that changed.
pub(crate) fn page(ui: &mut Ui, new: Option<Shown>, late: bool, tiles: &mut InventoryTiles, fonts: &mut dyn PageFonts) -> Result<TileId, String> {
    let menu = ui.menu(INVENTORY_MENU).ok_or_else(|| format!("{INVENTORY_MENU} is not in the tree"))?;
    // A new menu tile (the tree built again): its rows, sizes and writes are all to do again.
    if tiles.written.menu != Some(menu) {
        let fallbacks = std::mem::take(&mut tiles.written.fallbacks);
        let was_open = tiles.written.was_open;
        tiles.written = Written { menu: Some(menu), fallbacks, was_open, ..Default::default() };
        tiles.view.close();
    }
    let w = &mut tiles.written;
    if w.checked.is_empty() || !w.checked.iter().all(|&t| ui.is_live(t)) {
        w.checked = check_file(ui, tiles.cut.as_deref())?;
    }
    let made = ensure_tabs(ui, &mut w.tabs)?;
    tiles.view.menu = Some(menu);
    let mut changed = false;
    if let Some(s) = new {
        let list_new = tiles.view.is_new(menu, &s.list);
        if list_new || w.shown.as_ref() != Some(&s) {
            write_model(ui, &s, &mut tiles.view);
            w.shown = Some(s);
            w.tell = true;
            changed = true;
        }
    }
    let Some(s) = w.shown.as_ref() else { return Err("nothing to show yet".into()) };
    let (tab, device) = (s.tab, s.device);
    if changed || made || late {
        write_tabs(ui, &w.tabs, tab);
        let texts = menus::texts(ui, menu);
        tiles.view.settle_shown(ui, &mut |ui: &mut Ui, sizes: &mut SizesWritten| fonts.write_sizes(ui, &texts, sizes));
        place_tabs(ui, &w.tabs);
        let title = ui.find_tile(TITLE).ok_or("the title is gone")?;
        let line = ui.find_tile(LINE).ok_or("the heading's line is gone")?;
        stats_menu::write_frame_of(ui, title, line, device, &mut w.flat);
        item_menu::mark_tiles(ui, &PATHS);
        ui.update();
    } else {
        // The scroll over the file's own count of the clicks and their pulses' ends.
        tiles.view.rewrite(ui);
    }
    if late {
        ui.update();
        // Every final pass, so readiness is about the tree that is about to be painted.
        let pieces = fonts.set_out(ui, menu);
        let places = tab_places(ui, &w.tabs);
        if w.places_told.as_ref() != Some(&places) {
            info!("tiles: {INVENTORY_MENU} tabs stand: {places}");
            w.places_told = Some(places);
        }
        w.counted = usable(ui, menu, &tiles.view, &w.tabs, tab, &pieces)?;
        if let Some(missed) = w.counted.tab_missed.clone() {
            if w.fallbacks.insert(missed.clone()) {
                warn!("{INVENTORY_MENU}: {missed}");
            }
        }
        tiles.pieces = pieces;
        if std::mem::take(&mut w.tell) {
            info!("{}", describe(ui, w));
        }
        let panel = w.shown.as_ref().and_then(|s| s.panel.as_ref()).map(|p| p.slot);
        if w.panel_told != panel {
            match panel {
                Some(slot) => info!("tiles: {INVENTORY_MENU} hotkey panel shown, key {}", slot + 1),
                None => info!("tiles: {INVENTORY_MENU} hotkey panel hidden"),
            }
            w.panel_told = panel;
        }
        tiles.view.check_drawn(ui, &mut |line: String| info!("{line}"));
    }
    Ok(menu)
}

/// A line for the log when what the page shows changes (smoke reads these).
fn describe(ui: &Ui, w: &Written) -> String {
    let Some(s) = w.shown.as_ref() else { return format!("tiles: {INVENTORY_MENU} page written") };
    let list = &s.list.lists[Side::Player as usize];
    let selected = list.selected.and_then(|i| list.rows.get(i)).map_or("none", |r| r.text.as_str());
    format!(
        "tiles: {INVENTORY_MENU} page from tiles: {} rows, \"{selected}\" selected, tab {}, Wg {}, HP {}, DT {}, Caps {}, figures: {}, {}",
        list.rows.len(),
        s.tab,
        s.figures[0],
        s.figures[1],
        s.figures[2],
        s.figures[3],
        item_card::describe(ui, CARD),
        if s.device { "device" } else { "screen menu" },
    )
}

/// Whether the page is eligible to prepare: the switch on, ITEMS showing, and the repair list and
/// the mod menu closed.
pub fn eligible(switch: TileMenus, pip: &PipBoy, lists_closed: bool) -> bool {
    switch.inventory && pip.shown() && pip.page == Page::Items && lists_closed
}

/// Whether the repair list and the mod menu are closed.
pub fn lists_closed(repair: Option<&PipRepair>, modding: Option<&PipItemMod>) -> bool {
    !repair.is_some_and(PipRepair::is_open) && !modding.is_some_and(PipItemMod::is_open)
}

/// The keys the hand-built page acts on itself (`crate::pipboy::items::keys`,
/// `crate::pipboy::repair::keys`, `crate::pipboy::item_mod::keys`, `crate::pipboy::keys`), by the
/// tile input's names, and the controller's buttons standing in for them or acting there
/// (`crate::pad::menu_buttons`, the X drop in `items::keys`).
pub const HAND_BUILT_KEYS: [&str; 19] = ["W", "S", "A", "D", "E", "R", "X", "xup", "xdown", "xleft", "xright", "xbuttona", "xbuttonb", "xbuttonx", "xbuttony", "xbuttonlb", "xbuttonrb", "xbuttonlt", "xbuttonrt"];

/// Whether a click a key made on the page's tiles acts: as `menus::key_click_acts`, and never for
/// a controller's button the hand-built page answers to, which only a real press can make, stood
/// in or not (its X drops by `items::keys`, on a controller working the controls alone).
fn key_acts(key: &str, physical: &[&str]) -> bool {
    menus::key_click_acts(INVENTORY_MENU, key, physical) && !(key.to_ascii_lowercase().starts_with("xbutton") && menus::hand_built_takes(INVENTORY_MENU, key))
}

/// The menu, when the page takes this frame's tile input: painted last frame, and the page up,
/// with nothing over it and no prompt. Every reader of the frame's input decides by this.
pub fn takes(tiles: &InventoryTiles, switch: TileMenus, pip: &PipBoy, game: &fnv_game::Game<'_>, paused: bool, closed: bool, prompt: bool) -> Option<TileId> {
    if !pip.open() || !eligible(switch, pip, closed) || paused || prompt || modal_over(game.menu) {
        return None;
    }
    tiles.shown.filter(|&m| game.ui_files.is_some() && game.ui_tree.is_live(m))
}

/// The page's keys in the hints: what works on the hand-built page, by the game's settings where
/// they name the key [design: as `super::container_menu`'s Take All]. The Equip key has no setting;
/// a pointer's right click is the keyboard's Drop, the controller's X.
fn hints_of(game: &fnv_game::Game<'_>) -> Hints {
    use fnv_game::ui_loading::game_string;
    let at = crate::pad::at_menu(fnv_game::game::menus::INVENTORY);
    let drop = match crate::pad::labels() {
        crate::pad::Labels::Keys => game_string(game, "sPCMenuHintRMB"),
        l => format!("[{}]", crate::pad::pad_label(crate::pad::PadInput::Button(GamepadButton::West), l)),
    };
    Hints {
        equip: crate::pad::hint_bracketed(&[KeyCode::Enter], at),
        drop,
        repair: crate::pad::game_hint(&game_string(game, "sPCMenuHintR"), &[KeyCode::KeyR], at),
        modify: crate::pad::game_hint(&game_string(game, "sPCMenuHintX"), &[KeyCode::KeyX], at),
        cancel: crate::pad::hint_bracketed(&[KeyCode::Escape], at),
    }
}

/// What a pass reads besides the model.
#[derive(SystemParam)]
pub(crate) struct PageSources<'w> {
    repair: Option<Res<'w, PipRepair>>,
    modding: Option<Res<'w, PipItemMod>>,
    hold: Option<Res<'w, HotkeyHold>>,
    hotkeys: Option<Res<'w, Hotkeys>>,
    combat: Option<Res<'w, crate::combat::Combat>>,
    play: Option<Res<'w, crate::PlayWorld>>,
}

/// What a pass over the page reads.
#[derive(SystemParam)]
pub(crate) struct ItemsPage<'w, 's> {
    script: Option<ResMut<'w, ScriptState>>,
    switch: Option<Res<'w, TileMenus>>,
    pip: Option<Res<'w, PipBoy>>,
    active: Option<Res<'w, ActiveMenu>>,
    sim: Option<ResMut<'w, crate::sim::SimState>>,
    sources: PageSources<'w>,
    tiles: ResMut<'w, InventoryTiles>,
    fonts: super::draw::FontKit<'w>,
    time: crate::pause::AidTime<'w>,
    forms: Local<'s, Option<Forms>>,
}

/// A pass; `None` when the page is not eligible (nothing is touched then).
fn pass(p: &mut ItemsPage<'_, '_>, late: bool) -> Option<Result<TileId, String>> {
    let switch = p.switch.as_deref().copied().unwrap_or_default();
    if !switch.inventory {
        return None;
    }
    let pip = p.pip.as_deref()?;
    // A new opening has a new model, its selection at the top: the list too (`m.scroll` is the
    // hand-built page's). A list kept on STATS and back stays where it was.
    if p.tiles.written.was_open != pip.open() {
        p.tiles.written.was_open = pip.open();
        if pip.open() {
            p.tiles.view.scroll_to(Side::Player, 0.0);
        }
    }
    let closed = lists_closed(p.sources.repair.as_deref(), p.sources.modding.as_deref());
    if !eligible(switch, pip, closed) {
        return None;
    }
    let device = pip.frame == Frame::Device;
    let tab = pip.tab();
    let script = p.script.as_mut()?.bypass_change_detection();
    script.rt.as_ref()?.game.ui_files.as_ref()?;
    let active = p.active.as_deref()?;
    let model = active.0.as_ref().filter(|m| m.mode == Mode::Inventory);
    let hold = p.sources.hold.as_ref();
    let hotkeys = p.sources.hotkeys.as_ref();
    let stale = p.tiles.written.shown.as_ref().is_none_or(|s| s.tab != tab || s.device != device);
    // No model and nothing written (the Pip-Boy going down from the hand-built page, a model that
    // comes later): nothing to decide yet, and no failure.
    if model.is_none() && p.tiles.written.shown.is_none() {
        return None;
    }
    let due = stale || p.active.as_ref().is_some_and(|a| a.is_changed()) || hold.is_some_and(|h| h.is_changed()) || hotkeys.is_some_and(|h| h.is_changed());
    // With the model gone (the Pip-Boy going down) the page stays as it was last written.
    let new = model.filter(|_| due).and_then(|m| {
        if p.forms.is_none() {
            *p.forms = p.sources.play.as_ref().map(|w| Forms::new(w.lo, w.schema));
        }
        let forms = &mut *p.forms;
        let sim = p.sim.as_mut()?.bypass_change_detection();
        let combat = p.sources.combat.as_deref();
        let selected = m.selected_row(Side::Player);
        // No DPS on this page, as the hand-built one (`items::draw`).
        let card = selected.map(|r| Info { dps: None, ..items::info_of(r, combat, sim, forms.as_mut(), Some(&mut *script)) });
        let hp = forms.as_mut().and_then(|f| crate::pipboy::stats::read::player_hp(f, sim, Some(&mut *script)));
        let dt = combat.map(|c| crate::combat::target_armor(c, Some(&mut *script), sim, FormId::PLAYER_REF).0);
        let caps = sim.world.item_count(FormId::PLAYER_REF, crate::menu::model::CAPS).unwrap_or(0);
        let figures = [items::wg_text(m), items::hp_text(hp), items::dt_text(dt), caps.to_string()];
        let repair = repair::current(active, sim, Some(&*script)).map(|o| o.usable());
        let panel = hold.zip(hotkeys).and_then(|(hold, hotkeys)| {
            let slot = hold.slot?;
            let icon = |item: u32| hotkey_panel::icon_of(FormId(item), sim, combat);
            Some(Panel {
                slot,
                text: hotkey_panel::panel_line(hotkeys, hold, sim).unwrap_or_default(),
                boxes: std::array::from_fn(|s| hotkeys.0[s].and_then(|h| icon(h.item))),
                centre: selected.and_then(|r| hotkey_panel::icon_of(r.item, sim, combat)),
            })
        });
        let hints = hints_of(&script.rt.as_ref()?.game);
        Some(Shown::of(m, card, tab, device, figures, repair, hints, panel))
    });
    let rt = script.rt.as_mut()?;
    Some(page(&mut rt.game.ui_tree, new, late, &mut p.tiles, &mut p.fonts))
}

/// The trait writes so far, while `NVF_PLAY_TEST_TILE_COUNTS` asks for them.
fn counts_before(p: &ItemsPage<'_, '_>) -> Option<((u64, u64), std::time::Instant)> {
    let rt = p.script.as_ref()?.rt.as_ref()?;
    super::counts_on().then(|| (rt.game.ui_tree.trait_write_counts(), std::time::Instant::now()))
}

/// The pass's share of the frame's counts (module notes); a page that is not up has none.
fn tell_counts(p: &ItemsPage<'_, '_>, before: Option<((u64, u64), std::time::Instant)>, late: bool, up: bool) {
    let (Some(((writes, changes), start)), true, Some(rt)) = (before, up, p.script.as_ref().and_then(|s| s.rt.as_ref())) else { return };
    let micros = start.elapsed().as_micros();
    let (writes_now, changes_now) = rt.game.ui_tree.trait_write_counts();
    let (writes, changed) = (writes_now - writes, changes_now - changes);
    if late {
        let c = &p.tiles.written.counted;
        info!(
            "tile counts: {INVENTORY_MENU} pass=late writes={writes} changed={changed} micros={micros} rows={} rows_in_clip={} hit_tests={} candidates={} pieces={}",
            c.rows,
            c.rows_in_clip,
            c.hit_tests,
            c.candidates,
            p.tiles.pieces.len(),
        );
    } else {
        info!("tile counts: {INVENTORY_MENU} pass=early writes={writes} changed={changed} micros={micros}");
    }
}

/// Before the scripts, after the tile input: this frame's model in the tree, and the scroll.
pub(crate) fn write(mut p: ItemsPage) {
    let before = counts_before(&p);
    // A failure here is told by `settle`, which decides.
    let up = matches!(pass(&mut p, false), Some(Ok(_)));
    tell_counts(&p, before, false, up);
}

/// Before the drawing, after the model's keys and commands: what they changed, the tree worked
/// out, and the one decision every reader of this frame shares.
pub(crate) fn settle(mut p: ItemsPage) {
    if p.tiles.cut_until.is_some_and(|until| p.time.elapsed_secs() >= until) {
        info!("test aid: {CUT_AID} over");
        p.tiles.cut = None;
        p.tiles.cut_until = None;
        p.tiles.written.checked.clear();
    }
    let before = counts_before(&p);
    let ready = match pass(&mut p, true) {
        Some(Ok(menu)) => {
            p.tiles.written.failing = 0;
            Some(menu)
        }
        Some(Err(reason)) => {
            // The hand-built page has this frame in any case; the third frame in a row is no
            // layout still settling.
            p.tiles.written.failing += 1;
            let line = format!("{INVENTORY_MENU} drawn by hand: {reason}");
            if p.tiles.written.failing >= FAILING_PASSES && p.tiles.written.fallbacks.insert(line.clone()) {
                warn!("{line}");
            }
            None
        }
        None => {
            p.tiles.written.failing = 0;
            None
        }
    };
    tell_counts(&p, before, true, ready.is_some());
    if ready.is_none() {
        // The next time it is written whole; the rows stay.
        p.tiles.written.shown = None;
        p.tiles.pieces.clear();
        p.tiles.view.menu = None;
    }
    if p.tiles.ready != ready {
        p.tiles.ready = ready;
    }
}

/// Final passes in a row that fail before the reason is warned of.
const FAILING_PASSES: u32 = 3;

/// What [`feed`] keeps between frames.
#[derive(Default)]
pub(crate) struct Feed {
    /// The page took input last frame, in this frame of the Pip-Boy.
    taking: Option<Frame>,
    /// Where the pointer was last frame, and the tile under it then.
    last_at: Option<(f64, f64)>,
    under: Option<TileId>,
}

/// What the page's tile input reads of the Pip-Boy.
#[derive(SystemParam)]
pub(crate) struct FeedPip<'w> {
    pip: Res<'w, PipBoy>,
    screen: Option<Res<'w, PipBoyScreen>>,
    aid: Res<'w, PipBoyAid>,
    paused: Option<Res<'w, crate::pause::Paused>>,
    active: Option<Res<'w, ActiveMenu>>,
    repair: Option<Res<'w, PipRepair>>,
    modding: Option<Res<'w, PipItemMod>>,
    wheel: Option<Res<'w, crate::menu::WheelHold>>,
}

/// What the page's tile input sends: where the hand-built page sends the same.
#[derive(SystemParam)]
pub(crate) struct FeedOut<'w> {
    tabs: MessageWriter<'w, PipBoyInput>,
    hovered: MessageWriter<'w, TabHovered>,
    commands: MessageWriter<'w, MenuInput>,
    repair: MessageWriter<'w, RepairInput>,
    modding: MessageWriter<'w, ModInput>,
    hold: ResMut<'w, HotkeyHold>,
    hotkeys: ResMut<'w, Hotkeys>,
}

/// Commands on a row behind a guard saying what its tile stood for, checked as they apply.
fn send_on_row(out: &mut FeedOut<'_>, view: &MenuTiles, index: usize, on_it: bool, commands: &[Command]) {
    if let Some(guard) = view.guard(Mode::Inventory, Side::Player, index, on_it, commands.len() as u8) {
        out.commands.write(MenuInput(Command::Expect(guard)));
    }
    for &cmd in commands {
        info!("tiles: {INVENTORY_MENU} {cmd:?} on its tiles");
        out.commands.write(MenuInput(cmd));
    }
}

/// The page's tile input, on a frame it takes it ([`takes`]): see the module notes. The Pip-Boy's
/// pointer and its test aid are only read here.
pub(crate) fn feed(mut script: ResMut<ScriptState>, switch: Res<TileMenus>, from: FeedPip, mut tiles: ResMut<InventoryTiles>, devices: Devices, mut out: FeedOut, mut state: Local<Feed>) {
    if !switch.inventory {
        return;
    }
    let FeedPip { pip, screen, aid, paused, active, repair, modding, wheel } = from;
    let Some(rt) = script.rt.as_mut() else { return };
    let game = &mut rt.game;
    let closed = lists_closed(repair.as_deref(), modding.as_deref());
    let modal = eligible(*switch, &pip, closed) && pip.open() && modal_over(game.menu);
    if tiles.modal != modal {
        tiles.modal = modal;
    }
    let is_paused = paused.as_ref().is_some_and(|p| p.0);
    let model = active.as_ref().and_then(|a| a.0.as_ref()).filter(|m| m.mode == Mode::Inventory);
    let prompt = model.is_some_and(|m| m.quantity.is_some());
    let menu = takes(&tiles, *switch, &pip, game, is_paused, closed, prompt).filter(|_| !tiles.others);
    let (Some(menu), Some(m)) = (menu, model) else {
        *state = Feed::default();
        tiles.view.drag = None;
        return;
    };
    let pointer = screen.as_ref().and_then(|s| s.pointer);
    let at = pointer.map(|p| (f64::from(p.x), f64::from(p.y)));
    if state.taking != Some(pip.frame) {
        // Where the pointer rests when the page comes up is no move onto a row.
        *state = Feed { taking: Some(pip.frame), last_at: at, under: None };
    }
    // The test aid's click (`crate::pipboy::drive_pointer`): 1 is its press's frame.
    let aid_press = aid.click == 1 && !aid.right;
    let aid_right = aid.click == 1 && aid.right;
    let held = aid_press || devices.mouse.pressed(MouseButton::Left);
    let mut input = fnv_ui::input::Input {
        at,
        press: aid_press || devices.mouse.just_pressed(MouseButton::Left),
        shift: devices.keys.pressed(KeyCode::ShiftLeft) || devices.keys.pressed(KeyCode::ShiftRight),
        ..Default::default()
    };
    let shift = input.shift;
    let pad_menu = crate::pad::MenuAt { code: Some(fnv_game::game::menus::INVENTORY), tab: 0 };
    let physical = frame_keys(&mut input, &devices, game, pad_menu);
    let report = fnv_game::ui_input::step(game, &[menu], &input, &fnv_ui::input::no_measure);
    let ui = &game.ui_tree;
    let moved = at != state.last_at;
    let right = aid_right || devices.mouse.just_pressed(MouseButton::Right);
    // What is under the pointer itself; looked for again only when it is needed.
    if report.over.is_some() || moved || right {
        state.under = at.and_then(|at| fnv_ui::input::hit(ui, &[menu], at, &fnv_ui::input::no_measure)).map(|h| h.tile);
        state.last_at = at;
    }
    log_report(ui, &report);
    let tiles = &mut *tiles;
    let tabs = &tiles.written.tabs;
    if !held {
        tiles.view.drag = None;
    }
    // At most one row click acts a frame: the commands apply later, on rows the first may change.
    let mut row_acted = false;
    let mut pressed_marker = false;
    for c in report.clicks.iter().filter(|c| ui.is_live(c.tile)) {
        if let fnv_ui::input::By::Key(k) = &c.by {
            if !key_acts(k, &physical) {
                continue;
            }
        }
        let Some(tile) = items_tile(ui, &tiles.view, tabs, c.tile) else { continue };
        info!("tiles: {INVENTORY_MENU} {tile:?} on its tiles");
        match tile {
            ItemsTile::Row(index) => {
                let Some(index) = tiles.view.row_at(m, Side::Player, index).filter(|_| !row_acted) else {
                    info!("tiles: {INVENTORY_MENU} a click on a row no longer there is ignored");
                    continue;
                };
                row_acted = true;
                // With a hotkey's key held the row's item goes in that slot; else as the
                // hand-built row: a click selects it, on the selected row it acts
                // (`items::on_row_click`).
                match out.hold.slot {
                    Some(slot) => {
                        if let Some(row) = m.list(Side::Player).rows.get(index) {
                            let said = hotkey_panel::assign_row(&mut out.hotkeys, slot, row);
                            out.hold.said = Some(said);
                        }
                    }
                    None => {
                        let on_it = m.selected[Side::Player as usize] == index;
                        send_on_row(&mut out, &tiles.view, index, on_it, &[Command::Select(Side::Player, index)]);
                    }
                }
            }
            ItemsTile::Tab(i) => {
                out.tabs.write(PipBoyInput::Tab(i));
            }
            ItemsTile::Button(Button::Equip) => {
                out.commands.write(MenuInput(Command::One));
            }
            ItemsTile::Button(Button::Drop) => {
                out.commands.write(MenuInput(Command::Drop));
            }
            ItemsTile::Button(Button::Repair) => {
                out.repair.write(RepairInput::Open);
            }
            ItemsTile::Button(Button::Mod) => {
                out.modding.write(ModInput::Open);
            }
            ItemsTile::Button(Button::Cancel) => {
                out.commands.write(MenuInput(Command::Close));
            }
            ItemsTile::Button(Button::Hotkey) => {}
            ItemsTile::Bar(BarPiece::Marker) => {
                pressed_marker = true;
                let list_tile = tiles.view.list_tile(ui, Side::Player);
                tiles.view.drag = at.filter(|_| held && matches!(c.by, fnv_ui::input::By::Pointer)).zip(list_tile).and_then(|((_, y), l)| list::marker_grab(ui, menu, l, y));
            }
            ItemsTile::Bar(piece) => {
                let value = tiles.view.value(Side::Player);
                if let Some(v) = tiles.view.list_tile(ui, Side::Player).and_then(|l| list::after_piece(ui, l, piece, value)) {
                    tiles.view.scroll_to(Side::Player, v);
                }
            }
        }
    }
    // A drag: the scroll under the pointer while the press holds.
    if let (false, Some(drag), Some((_, y))) = (pressed_marker, tiles.view.drag, at) {
        if let Some(v) = list::value_under(ui, drag, y) {
            tiles.view.scroll_to(Side::Player, v);
        }
    }
    // A right click on a row drops one, with Shift the stack; not with a hotkey's key held
    // [design: owner 2026-11-02, #1393; `items::on_row_click`].
    if right && !row_acted && out.hold.slot.is_none() {
        if let Some(ItemsTile::Row(index)) = state.under.and_then(|t| items_tile(ui, &tiles.view, tabs, t)) {
            if let Some(index) = tiles.view.row_at(m, Side::Player, index) {
                row_acted = true;
                send_on_row(&mut out, &tiles.view, index, false, &[Command::Hover(Side::Player, index), if shift { Command::DropStack } else { Command::Drop }]);
            }
        }
    }
    if let Some((_, Some(to))) = report.over {
        match items_tile(ui, &tiles.view, tabs, to) {
            Some(ItemsTile::Tab(i)) => {
                out.hovered.write(TabHovered(i));
            }
            // The row the pointer comes onto takes the selection, unless the wheel holds it.
            Some(ItemsTile::Row(index)) => {
                let already = m.selected[Side::Player as usize] == index;
                let holds = wheel.as_ref().is_some_and(|h| h.held());
                if !row_acted && item_menu::hover_selects(moved, holds, already) && tiles.view.row_at(m, Side::Player, index).is_some() {
                    send_on_row(&mut out, &tiles.view, index, false, &[Command::Hover(Side::Player, index)]);
                }
            }
            _ => {}
        }
    }
}

/// The resource, with the test aid read.
pub(crate) fn setup(app: &mut App) {
    let (cut, cut_until) = std::env::var(CUT_AID).ok().map_or((None, None), |v| {
        let (name, until) = stats_menu::parse_cut(&v);
        (Some(name), until)
    });
    app.insert_resource(InventoryTiles { cut, cut_until, ..Default::default() });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::model::{Category, RowKey};
    use fnv_ui::MemSource;

    /// [design] A small layout written for these tests, with its own structure, numbers and
    /// expressions; it does not come from the game's files and says nothing about them. It keeps
    /// the tile, template and path names the model and the input look up, which are the engine's
    /// interface ([`required_paths`]), and only the dependencies the tests exercise: a row stands at
    /// its `_y` less its list's scroll, a tab at its `listindex`'s place with its words in a text
    /// under it. So these tests do not cover the file's own places, how its expressions read what
    /// is written (the tab template's inside, `_CurrentTab`, `_LeftLineLength`, a tab's `_selected`,
    /// `_EquippableItem`, `_KeyringOpen`, the panel's traits, `IM_EquipItemMarker`, the headline
    /// cards' `_Value`), or whether a name is where the file has it: the smoke scenarios
    /// `tiles-items*` check those on the real file, and their shots what they look like.
    fn tree_changed(change: impl FnOnce(String) -> String) -> Ui {
        let card = |name: &str, inner: &str| format!(r#"<rect name="{name}"><_Title>{name}</_Title><_Value></_Value><visible>&false;</visible><text name="CI_TitleText"></text><text name="CI_ValueText"></text>{inner}</rect>"#);
        let cards: String = ["DAMInfo", "DamageThresholdInfo", "DPSInfo", "WeightInfo", "ValueInfo", "StrengthReqInfo", "AmmoInfo", "EffectsInfo"].iter().map(|c| card(c, "")).collect::<String>() + &card("ConditionInfo", r#"<rect name="ConditionMeter"><image name="MeterImage"></image></rect><image name="CNDArrows"></image>"#);
        let headline: String = ["WG", "HP", "DT", "DR", "Caps"].iter().map(|c| format!(r#"<rect name="IM_Headline_Player{c}Info"><_Value></_Value></rect>"#)).collect();
        let slots: String = [0, 2, 3, 4, 5, 6, 7].iter().map(|n| format!(r#"<rect name="HK_Item_{n}"/>"#)).collect();
        let button = |name: &str, y: u32| format!(r#"<image name="{name}"><target>&true;</target><y>{y}</y><width>70</width><height>20</height><text name="button_text"><string>Go</string></text><text name="PCShortcutLabel"></text></image>"#);
        let buttons: String = [("IM_EquipButton", 0), ("IM_DropButton", 30), ("IM_RepairButton", 60), ("IM_ModButton", 90), ("IM_CancelButton", 120)].iter().map(|(n, y)| button(n, *y)).collect();
        let xml = format!(
            r#"<menu name="InventoryMenu"><rect name="NOGLOW_BRANCH"/><rect name="GLOW_BRANCH">
            <image name="IM_Background"/>
            <rect name="IM_MainRect">
              <rect name="IM_HeadlineRect"><text name="IM_Headline_Title"><string>Words</string></text>
                <rect name="IM_Headline_h2"><x>300</x><_length>40</_length></rect>{headline}</rect>
              <hotrect name="IM_InventoryList"><x>20</x><y>50</y><width>300</width><height>200</height><clipwindow>&true;</clipwindow>
                <_scroll_delta>15</_scroll_delta>
                <_offset><copy src="child(lb_scrollbar)" trait="_current_value"/><mul src="me()" trait="_scroll_delta"/></_offset>
                <hotrect name="lb_highlight_box"/>
                <image name="lb_scrollbar"><_step_size>1</_step_size><_jump_size>2</_jump_size><_current_value>0</_current_value>
                  <hotrect name="scrollbar_vert_page_up"/><hotrect name="scrollbar_vert_page_down"/>
                  <image name="scrollbar_vert_marker"/><image name="scrollbar_vert_up"/><image name="scrollbar_vert_down"/>
                </image>
                <image name="IM_EquipItemMarker"/>
              </hotrect>
              <rect name="IM_HotKeyWheel"><visible>&false;</visible>{slots}<image name="IM_HotkeyIcon"/></rect>
              <rect name="IM_ButtonRect"><x>500</x><y>300</y>{buttons}</rect>
              <image name="IM_ItemIcon"/><image name="IM_ItemIconBadge"/><text name="IM_StrReq"></text>
              <rect name="IM_ItemInfoRect"><user1>30</user1>{cards}</rect>
            </rect>
            <rect name="IM_Tabline"><y>560</y></rect>
            </rect>
            <template name="IM_InventoryListTemplate"><hotrect name="IM_InventoryListTemplateRect"><target>&true;</target><x>10</x><width>250</width>
              <_y>0</_y><_VerticalSpacing>10</_VerticalSpacing>
              <y><copy src="me()" trait="_y"/><sub src="parent()" trait="_offset"/></y>
              <text name="ListItemText"></text><image name="IM_Template_ItemMarker"/>
            </hotrect></template>
            <template name="TabButtonTemplate"><hotrect name="TabButton"><target>&true;</target><width>100</width><height>30</height><_pitch>120</_pitch>
              <x><copy src="me()" trait="listindex"/><mul src="me()" trait="_pitch"/></x>
              <text name="button_text"></text>
            </hotrect></template></menu>"#
        );
        let mut ui = Ui::new();
        ui.load_menu(&MemSource::default().with("i.xml", &change(xml)), "i.xml").unwrap();
        ui.update();
        ui
    }

    fn tree() -> Ui {
        tree_changed(|x| x)
    }

    /// Each row's height in the fixture: its text's 30 and its own spacing of 10.
    const ROW_H: f64 = 40.0;

    fn row(item: u32, text: &str) -> item_menu::RowShown {
        item_menu::RowShown { key: RowKey { units: (FormId(item), None, 0), reference: None, nth: 0 }, name: format!("Item {item}"), text: text.into(), equipped: item == 1 }
    }

    fn shown(rows: usize, selected: usize) -> Shown {
        let list = ListShown { rows: (1..=rows as u32).map(|i| row(i, &format!("Row {i}"))).collect(), category: Category::Weapons, selected: (selected < rows).then_some(selected), filtered: 0 };
        Shown {
            list: Snapshot {
                lists: [ListShown::default(), list],
                titles: Default::default(),
                weight: String::new(),
                focus: Side::Player,
                pointer_pick: false,
                asking: false,
                navigated: 0,
                card: (selected < rows).then(|| Info { value: Some("30".into()), weight: Some("5.00".into()), badge: Some("pics\\badge.dds".into()), skill_req: Some(25), ..Info::default() }),
                icon: (selected < rows).then(|| "pics\\item.dds".to_string()),
                take_all: false,
                take_all_hint: String::new(),
                note: None,
            },
            tab: 0,
            device: true,
            figures: ["52/200".into(), "90/100".into(), "8.0".into(), "1234".into()],
            equip: selected < rows,
            drop: selected < rows,
            repair: None,
            modify: false,
            equippable: true,
            keyring: false,
            hints: Hints { equip: "[Enter]".into(), drop: "[Right Click]".into(), repair: "[R]".into(), modify: "[X]".into(), cancel: "[Esc]".into() },
            panel: None,
        }
    }

    /// A measure of 10 a character and 30 a line.
    fn measure(_: f64, text: &str, _: Option<f64>) -> (f64, f64) {
        (10.0 * text.chars().count() as f64, 30.0 * text.split('|').count() as f64)
    }

    struct TestFonts;

    impl PageFonts for TestFonts {
        fn write_sizes(&mut self, ui: &mut Ui, texts: &[TileId], written: &mut SizesWritten) -> usize {
            menus::write_text_sizes_of(ui, texts, written, &measure)
        }
        fn set_out(&mut self, ui: &Ui, menu: TileId) -> Vec<Piece> {
            super::super::draw::pieces(ui, menu, &measure, super::super::draw::Select::Whole { kept: &[], strip: None }, true).0
        }
    }

    fn run(ui: &mut Ui, s: Option<Shown>, t: &mut InventoryTiles) -> Result<TileId, String> {
        page(ui, s, true, t, &mut TestFonts)
    }

    fn at(ui: &Ui, path: &str, name: &str) -> f64 {
        ui.get_float(&format!("{path}/{name}")).unwrap_or(-999.0)
    }

    fn words(ui: &Ui, path: &str, name: &str) -> String {
        ui.get_string(&format!("{path}/{name}")).unwrap_or_default()
    }

    fn rows(t: &InventoryTiles) -> &[RowTile<RowKey>] {
        &t.view.rows[Side::Player as usize]
    }

    /// The middle of a tile's text piece, as painted.
    fn middle(t: &InventoryTiles, tile: TileId) -> (f64, f64) {
        let p = t.pieces.iter().find(|p| p.tile == tile).unwrap();
        (p.rect.left + p.rect.width / 2.0, p.rect.top + p.rect.height / 2.0)
    }

    #[test]
    fn the_model_writes_the_figures_rows_tabs_and_cards() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        let menu = run(&mut ui, Some(shown(3, 1)), &mut t).unwrap();
        for (card, want) in HEADLINE.iter().zip(["52/200", "90/100", "8.0", "1234"]) {
            assert_eq!(words(&ui, card, "_Value"), want);
        }
        assert_eq!(at(&ui, HEADLINE_DR, "visible"), 0.0, "this page has no DR card");
        assert_eq!(at(&ui, BACKGROUND, "visible"), 0.0);
        assert_eq!(at(&ui, EQUIP_MARKER, "visible"), 0.0);
        // The rows, named after their items, the list's own places.
        assert_eq!(rows(&t).len(), 3);
        assert_eq!(ui.tile(rows(&t)[2].tile).name, "Item 3");
        assert_eq!(ui.tile(rows(&t)[2].tile).get("_y").map(Value::num), Some(2.0 * ROW_H));
        let text_tile = list::child(&ui, rows(&t)[1].tile, ROW_TEXT).unwrap();
        assert_eq!(ui.tile(text_tile).get("string").map(|v| v.text().to_string()).as_deref(), Some("Row 2"));
        let mark = |ui: &Ui, i: usize| ui.tile(list::child(ui, rows(&t)[i].tile, ROW_MARK).unwrap()).get("visible").map(Value::num);
        assert_eq!((mark(&ui, 0), mark(&ui, 1)), (Some(1.0), Some(0.0)), "the equipped mark");
        assert_eq!((at(&ui, LIST, "_highlight_y"), at(&ui, LIST, "_selected_height")), (ROW_H, ROW_H));
        // The tabs: five made under the tab line, each with its word.
        let tabs = t.written.tabs.clone();
        assert_eq!(tabs.len(), 5);
        for (row, word) in tabs.iter().zip(TAB_WORDS) {
            assert_eq!(ui.tile(row.tile).get("string").map(|v| v.text().to_string()).as_deref(), Some(word));
            let inner = find_named(&ui, row.tile, TAB_TEXT).unwrap();
            assert_eq!(ui.tile(inner).get("string").map(|v| v.text().to_string()).as_deref(), Some(word));
        }
        assert_eq!((at(&ui, TABLINE, "_ButtonCount"), at(&ui, TABLINE, "_CurrentTab")), (5.0, 0.0));
        // The picture, the badge, the skill, the figures, the buttons.
        assert_eq!(words(&ui, ICON, "filename"), "pics\\item.dds");
        assert_eq!(at(&ui, ICON, "visible"), 1.0);
        assert_eq!(words(&ui, BADGE, "filename"), "pics\\badge.dds");
        assert_eq!(words(&ui, SKILL_REQ, "string"), "25");
        assert_eq!(words(&ui, &format!("{CARD}/WeightInfo"), "_Value"), "5.00");
        assert_eq!(at(&ui, &button_path("IM_EquipButton"), "visible"), 1.0);
        assert_eq!(words(&ui, &button_path("IM_EquipButton"), "_PCButtonText"), "[Enter]");
        assert_eq!(at(&ui, &button_path("IM_RepairButton"), "visible"), 0.0);
        assert_eq!(at(&ui, &button_path("IM_ModButton"), "visible"), 0.0);
        assert_eq!(words(&ui, &button_path("IM_DropButton"), "_PCButtonText"), "[Right Click]");
        assert_eq!(at(&ui, &button_path("IM_CancelButton"), "visible"), 1.0);
        assert_eq!((at(&ui, INVENTORY_MENU, "_EquippableItem"), at(&ui, INVENTORY_MENU, "_KeyringOpen")), (1.0, 0.0));
        assert_eq!(at(&ui, WHEEL, "visible"), 0.0);
        assert_eq!(at(&ui, BUTTON_RECT, "visible"), 1.0);
        assert_eq!(ui.menu(INVENTORY_MENU), Some(menu));
        assert!(describe(&ui, &t.written).contains("3 rows, \"Row 2\" selected, tab 0, Wg 52/200"), "{}", describe(&ui, &t.written));
    }

    #[test]
    fn a_scrolled_list_is_usable_and_only_rows_inside_its_window_are_asked_of() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        // 12 rows of 40 in a window of 200: five whole.
        let s = shown(12, 0);
        let menu = run(&mut ui, Some(s.clone()), &mut t).unwrap();
        assert_eq!(t.written.counted.rows, 12);
        assert_eq!(t.written.counted.rows_in_clip, 5, "the rows below the window are passed over");
        // A tab less (the one showing) and the five rows.
        assert_eq!(t.written.counted.hit_tests, 9);
        assert!(t.view.scroll_to(Side::Player, 4.0));
        run(&mut ui, None, &mut t).unwrap();
        let (kept, total) = (t.written.counted.rows_in_clip, t.written.counted.rows);
        assert!(kept > 0 && kept < total, "{kept} of {total}");
        // What stands under the window's top row is the row the view moved there.
        let scale = at(&ui, LIST, "_scroll_delta");
        let first = (4.0 * scale / ROW_H).ceil() as usize;
        let tile = list::child(&ui, rows(&t)[first].tile, ROW_TEXT).unwrap();
        let hit = fnv_ui::input::hit(&ui, &[menu], middle(&t, tile), &fnv_ui::input::no_measure).unwrap();
        assert_eq!(items_tile(&ui, &t.view, &t.written.tabs, hit.tile), Some(ItemsTile::Row(first)));
        // Past the window's bottom nothing of the list takes the pointer, though rows stand there.
        let list_rect = t.pieces.iter().find(|p| p.tile == list::child(&ui, rows(&t)[11].tile, ROW_TEXT).unwrap()).unwrap().clip.unwrap();
        let below = (list_rect.left + 50.0, list_rect.top + list_rect.height + 5.0);
        let past = fnv_ui::input::hit(&ui, &[menu], below, &fnv_ui::input::no_measure).and_then(|h| items_tile(&ui, &t.view, &t.written.tabs, h.tile));
        assert_eq!(past, None);
    }

    #[test]
    fn a_list_that_scrolls_without_a_window_falls_back() {
        let mut ui = tree_changed(|x| x.replace("<clipwindow>&true;</clipwindow>", ""));
        let mut t = InventoryTiles::default();
        assert_eq!(run(&mut ui, Some(shown(12, 0)), &mut t), Err("the list scrolls and has no window".to_string()));
        // A list that fits needs none.
        let (mut ui, mut t) = (tree_changed(|x| x.replace("<clipwindow>&true;</clipwindow>", "")), InventoryTiles::default());
        assert!(run(&mut ui, Some(shown(2, 0)), &mut t).is_ok());
    }

    #[test]
    fn an_empty_tab_is_a_page() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        run(&mut ui, Some(shown(0, 0)), &mut t).unwrap();
        assert!(rows(&t).is_empty());
        assert_eq!((at(&ui, LIST, "_highlight_y"), at(&ui, LIST, "_selected_height")), (-1.0, 0.0));
        assert_eq!(at(&ui, ICON, "visible"), 0.0);
        assert_eq!(at(&ui, &button_path("IM_EquipButton"), "visible"), 0.0);
        assert_eq!(at(&ui, &button_path("IM_CancelButton"), "visible"), 1.0);
    }

    #[test]
    fn rows_change_with_the_tab_and_keep_their_tiles_tabs_stay() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        run(&mut ui, Some(shown(3, 0)), &mut t).unwrap();
        let (tabs, first) = (t.written.tabs.clone(), rows(&t)[0].tile);
        let mut s = shown(2, 0);
        s.tab = 4;
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!(t.written.tabs, tabs, "the tabs are made once");
        assert_eq!(rows(&t)[0].tile, first, "a row keeps what it stands for");
        assert_eq!(rows(&t).len(), 2);
        assert_eq!(at(&ui, TABLINE, "_CurrentTab"), 4.0);
    }

    #[test]
    fn a_tile_stands_for_its_row_tab_button_or_bar_piece() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        let of = |path: &str| items_tile(&ui, &t.view, &t.written.tabs, ui.find_tile(path).unwrap());
        assert_eq!(of(&format!("{}/button_text", button_path("IM_DropButton"))), Some(ItemsTile::Button(Button::Drop)));
        assert_eq!(of(&button_path("IM_CancelButton")), Some(ItemsTile::Button(Button::Cancel)));
        assert_eq!(of(&format!("{LIST}/lb_scrollbar/scrollbar_vert_page_down")), Some(ItemsTile::Bar(BarPiece::PageDown)));
        assert_eq!(of(&format!("{LIST}/lb_scrollbar")), None);
        assert_eq!(of(ICON), None, "an unnamed path of the absent side is no list");
        let tab = t.written.tabs[3].tile;
        assert_eq!(items_tile(&ui, &t.view, &t.written.tabs, find_named(&ui, tab, TAB_TEXT).unwrap()), Some(ItemsTile::Tab(3)));
        let row = list::child(&ui, rows(&t)[1].tile, ROW_TEXT).unwrap();
        assert_eq!(items_tile(&ui, &t.view, &t.written.tabs, row), Some(ItemsTile::Row(1)));
        // The tab line stands where the fixture put it: a tab takes the pointer at its words.
        let menu = ui.menu(INVENTORY_MENU).unwrap();
        let words_tile = find_named(&ui, t.written.tabs[2].tile, TAB_TEXT).unwrap();
        let hit = fnv_ui::input::hit(&ui, &[menu], middle(&t, words_tile), &fnv_ui::input::no_measure).unwrap();
        assert_eq!(items_tile(&ui, &t.view, &t.written.tabs, hit.tile), Some(ItemsTile::Tab(2)));
    }

    #[test]
    fn a_row_or_a_tab_something_lies_over_falls_back() {
        // A hotrect of the file's own over everything: nothing takes the pointer but it.
        let mut ui = tree_changed(|x| x.replace(r#"<image name="IM_Background"/>"#, r#"<image name="IM_Background"/><hotrect name="Cover"><target>&true;</target><depth>500</depth><width>1000</width><height>800</height></hotrect>"#));
        let mut t = InventoryTiles::default();
        let reason = run(&mut ui, Some(shown(2, 0)), &mut t).unwrap_err();
        assert!(reason.starts_with("Row(0) does not take the pointer at ("), "{reason}");
        assert!(t.pieces.is_empty(), "nothing goes to the painter");
    }

    /// The tabs' middles as painted take the pointer, each its own.
    #[track_caller]
    fn assert_each_tab_takes_the_pointer(ui: &Ui, t: &InventoryTiles) {
        let menu = ui.menu(INVENTORY_MENU).unwrap();
        for row in &t.written.tabs {
            let words = find_named(ui, row.tile, TAB_TEXT).unwrap();
            let hit = fnv_ui::input::hit(ui, &[menu], middle(t, words), &fnv_ui::input::no_measure).unwrap();
            assert_eq!(items_tile(ui, &t.view, &t.written.tabs, hit.tile), Some(ItemsTile::Tab(row.key)));
        }
    }

    #[test]
    fn tabs_the_file_stands_at_one_place_are_set_along_the_line() {
        // A template that tells its tiles nothing apart: each tab's words under the last tab.
        let mut ui = tree_changed(|x| x.replace("<_pitch>120</_pitch>", "<_pitch>0</_pitch>"));
        let mut t = InventoryTiles::default();
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        assert_eq!(t.written.counted.tab_missed, None);
        assert_each_tab_takes_the_pointer(&ui, &t);
        // In order along the line, the hand-built rule: 75 between boxes of the words and 35.
        let xs: Vec<f64> = t.written.tabs.iter().map(|r| num(&ui, r.tile, "x")).collect();
        assert_eq!(xs, [125.0, 305.0, 465.0, 605.0, 755.0]);
        assert_eq!(at(&ui, TABLINE, "_LeftLineLength"), 75.0);
        // Worked out again by the file, put right on the next pass.
        let first = t.written.tabs[0].tile;
        ui.set_trait(first, "x", Value::Num(0.0));
        run(&mut ui, None, &mut t).unwrap();
        assert_eq!(num(&ui, first, "x"), 125.0);
    }

    #[test]
    fn the_line_s_own_place_is_taken_off_and_an_inner_rect_repeats_the_tab_s() {
        let mut ui = tree_changed(|x| {
            x.replace(r#"<rect name="IM_Tabline"><y>560</y></rect>"#, r#"<rect name="IM_Tabline"><x>50</x><y>560</y></rect>"#)
                .replace(r#"<text name="button_text"></text>
            </hotrect></template></menu>"#, r#"<rect name="stb_text_box"><text name="button_text"></text></rect>
            </hotrect></template></menu>"#)
        });
        let mut t = InventoryTiles::default();
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        let tab = t.written.tabs[1].tile;
        let inner = find_named(&ui, tab, TAB_INNER).unwrap();
        assert_eq!((num(&ui, tab, "x"), num(&ui, inner, "x")), (255.0, 255.0));
        assert_eq!(num(&ui, tab, fnv_ui::input::NO_ORIGIN), 1.0);
        assert_each_tab_takes_the_pointer(&ui, &t);
    }

    #[test]
    fn a_template_that_leaves_target_to_code_gets_it() {
        let mut ui = tree_changed(|x| x.replace(r#"<hotrect name="TabButton"><target>&true;</target>"#, r#"<hotrect name="TabButton">"#));
        let mut t = InventoryTiles::default();
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        assert_eq!(t.written.counted.tab_missed, None);
        assert_each_tab_takes_the_pointer(&ui, &t);
    }

    #[test]
    fn a_tab_something_lies_over_is_told_and_the_page_stays() {
        let mut ui = tree_changed(|x| x.replace(r#"<rect name="IM_Tabline">"#, r#"<hotrect name="Cover"><target>&true;</target><depth>500</depth><y>550</y><width>1000</width><height>60</height></hotrect><rect name="IM_Tabline">"#));
        let mut t = InventoryTiles::default();
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        let missed = t.written.counted.tab_missed.clone().unwrap();
        assert!(missed.starts_with("Tab(1) does not take the pointer at ("), "{missed}");
        assert!(missed.contains("Cover"), "{missed}");
        assert!(!t.pieces.is_empty(), "the page goes to the painter");
    }

    #[test]
    fn tabs_without_words_fall_back() {
        let mut ui = tree_changed(|x| x.replace(r#"<text name="button_text"></text>
            </hotrect></template></menu>"#, r#"</hotrect></template></menu>"#));
        let mut t = InventoryTiles::default();
        // The fixture's tab then holds no text tile: its word is written on the tab alone.
        assert_eq!(run(&mut ui, Some(shown(2, 0)), &mut t), Err("tab 0 is not shown".to_string()));
    }

    #[test]
    fn a_missing_tile_or_template_keeps_the_hand_built_page() {
        assert_eq!(built(&tree(), None), Tiled::Yes);
        for (name, missing) in [("IM_Tabline", TABLINE), ("IM_ItemIcon", ICON), ("IM_HotKeyWheel", WHEEL)] {
            let ui = tree_changed(|x| x.replace(&format!("name=\"{name}\""), "name=\"Modded\""));
            assert_eq!(built(&ui, None), Tiled::No(format!("{missing} missing")));
        }
        for piece in ["IM_DropButton", "scrollbar_vert_page_up", "HK_Item_5", "WeightInfo", "IM_Headline_PlayerCapsInfo"] {
            let ui = tree_changed(|x| x.replace(&format!("name=\"{piece}\""), "name=\"Modded\""));
            assert!(matches!(built(&ui, None), Tiled::No(reason) if reason.contains(piece)), "{piece}");
        }
        let ui = tree_changed(|x| x.replace("name=\"ListItemText\"", "name=\"Modded\""));
        assert_eq!(built(&ui, None), Tiled::No("IM_InventoryListTemplate/IM_InventoryListTemplateRect/ListItemText missing".into()));
        let ui = tree_changed(|x| x.replace("name=\"IM_InventoryListTemplate\"", "name=\"Modded\""));
        assert_eq!(built(&ui, None), Tiled::No("IM_InventoryListTemplate missing".into()));
        let ui = tree_changed(|x| x.replace("name=\"TabButtonTemplate\"", "name=\"Modded\""));
        assert_eq!(built(&ui, None), Tiled::No("TabButtonTemplate missing".into()));
        // Neither of the two the page can do without.
        for piece in ["IM_HotkeyButton", "IM_Headline_PlayerDRInfo"] {
            let ui = tree_changed(|x| x.replace(&format!("name=\"{piece}\""), "name=\"Modded\""));
            assert_eq!(built(&ui, None), Tiled::Yes, "{piece}");
        }
    }

    #[test]
    fn the_test_aid_cuts_a_name_and_the_page_recovers() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        t.cut = Some("IM_InventoryListTemplate".into());
        assert_eq!(run(&mut ui, Some(shown(2, 0)), &mut t), Err("IM_InventoryListTemplate missing".to_string()));
        assert!(rows(&t).is_empty(), "nothing made");
        t.cut = Some("IM_ItemIcon".into());
        assert_eq!(run(&mut ui, Some(shown(2, 0)), &mut t), Err(format!("{ICON} missing")));
        t.cut = None;
        assert!(run(&mut ui, Some(shown(2, 0)), &mut t).is_ok());
    }

    #[test]
    fn the_panel_hides_the_buttons_and_the_picture_and_names_its_slots() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        let mut s = shown(2, 0);
        let mut boxes = [None; SLOTS];
        boxes[2] = Some("icons\\gun.dds");
        s.panel = Some(Panel { slot: 2, text: "Gun".into(), boxes, centre: Some("icons\\row.dds") });
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!((at(&ui, WHEEL, "visible"), at(&ui, BUTTON_RECT, "visible")), (1.0, 0.0));
        assert_eq!((at(&ui, ICON, "visible"), at(&ui, BADGE, "visible"), at(&ui, SKILL_REQ, "visible")), (0.0, 0.0, 0.0));
        assert_eq!(words(&ui, &format!("{CARD}/WeightInfo"), "_Value"), "5.00", "the figures stay");
        assert_eq!((at(&ui, WHEEL, "_SelectedHotkey"), words(&ui, WHEEL, "_SelectedText")), (2.0, "Gun".to_string()));
        let (filled, empty) = (slot_path(2), slot_path(3));
        assert_eq!(words(&ui, &filled, "_HotKeyIcon"), "icons\\gun.dds");
        assert_eq!((at(&ui, &filled, "_HotKeyNumber"), at(&ui, &filled, "_HotKeyAssigned")), (3.0, 1.0));
        assert_eq!((words(&ui, &empty, "_HotKeyIcon"), at(&ui, &empty, "_HotKeyAssigned")), (String::new(), 0.0));
        assert_eq!(words(&ui, WHEEL_ICON, "filename"), "icons\\row.dds");
        // Let go: back as it was.
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        assert_eq!((at(&ui, WHEEL, "visible"), at(&ui, BUTTON_RECT, "visible"), at(&ui, ICON, "visible")), (0.0, 1.0, 1.0));
    }

    #[test]
    fn a_repair_that_does_nothing_is_dim() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        let mut s = shown(2, 0);
        s.repair = Some(false);
        run(&mut ui, Some(s.clone()), &mut t).unwrap();
        let repair = button_path("IM_RepairButton");
        assert_eq!((at(&ui, &repair, "visible"), at(&ui, &repair, "brightness")), (1.0, 128.0));
        assert_eq!(at(&ui, &format!("{repair}/button_text"), "brightness"), 128.0);
        s.repair = Some(true);
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!(at(&ui, &repair, "brightness"), 255.0);
    }

    #[test]
    fn the_screen_menu_has_no_title_and_the_device_gets_it_back() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        let mut s = shown(2, 0);
        s.device = false;
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_eq!(at(&ui, TITLE, "visible"), 0.0);
        run(&mut ui, Some(shown(2, 0)), &mut t).unwrap();
        assert_eq!(at(&ui, LINE, "x"), 300.0, "the file's place again");
        assert_ne!(at(&ui, TITLE, "visible"), 0.0);
    }

    #[test]
    fn a_new_tree_starts_over_and_a_new_pass_makes_nothing_twice() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        let s = shown(2, 0);
        run(&mut ui, Some(s.clone()), &mut t).unwrap();
        run(&mut ui, Some(s.clone()), &mut t).unwrap();
        run(&mut ui, None, &mut t).unwrap();
        let line = ui.find_tile(TABLINE).unwrap();
        let live = |ui: &Ui, p: TileId| ui.tile(p).children.iter().filter(|&&c| ui.is_live(c)).count();
        assert_eq!(live(&ui, line), 5, "five tabs, not ten");
        let old = rows(&t)[0].tile;
        let mut ui = tree();
        run(&mut ui, Some(s), &mut t).unwrap();
        assert_ne!(rows(&t)[0].tile, old);
        assert_eq!(rows(&t).len(), 2);
        assert_eq!(live(&ui, ui.find_tile(TABLINE).unwrap()), 5);
    }

    #[test]
    fn nothing_written_yet_is_no_page() {
        let (mut ui, mut t) = (tree(), InventoryTiles::default());
        assert_eq!(run(&mut ui, None, &mut t), Err("nothing to show yet".to_string()));
    }

    #[test]
    fn a_physical_key_is_the_hand_built_pages_a_scripts_tap_the_files() {
        // A key the page acts on itself, pressed: not through the file.
        for key in ["R", "X", "E"] {
            assert!(!key_acts(key, &[key]), "{key}");
            assert!(key_acts(key, &[]), "a script's tap of {key}");
        }
        // The controller's X has no key stood in for it and drops by `items::keys`: never twice.
        assert!(!key_acts("xbuttonx", &[]));
        assert!(!key_acts("xbuttony", &["xbuttony"]));
        // A key the page does not take is the file's.
        assert!(key_acts("Q", &["Q"]));
    }

    #[test]
    fn every_hint_and_word_written_has_its_source() {
        // [design] The tab words are NVForever's own: not the words read off the original's
        // screen for the hand-built page wherever the two differ.
        assert_eq!(TAB_WORDS.len(), crate::pipboy::items::TABS.len());
        assert_ne!(TAB_WORDS, crate::pipboy::Page::Items.tabs());
        // The settings the hints read are in the strings table.
        for name in ["sPCMenuHintR", "sPCMenuHintX", "sPCMenuHintRMB"] {
            assert!(fnv_ui::strings::built_in(name).is_some(), "{name}");
        }
    }

    #[test]
    fn the_page_is_eligible_only_on_items_with_its_lists_closed() {
        let on = TileMenus { inventory: true, ..TileMenus::default() };
        let mut pip = PipBoy::default();
        pip.page = Page::Items;
        assert!(!eligible(on, &pip, true), "down: not shown");
        pip.phase = crate::pipboy::Phase::Up;
        assert!(eligible(on, &pip, true));
        assert!(!eligible(TileMenus::default(), &pip, true), "off, never");
        assert!(!eligible(on, &pip, false), "the repair list or the mod menu has the page");
        pip.page = Page::Stats;
        assert!(!eligible(on, &pip, true));
        // The two lists read as closed without their resources.
        assert!(lists_closed(None, None));
    }
}
