//! The classic VATS style's target screen drawn from `menus\vats_menu.xml` [design:
//! docs/design/vats-classic.md §11; the file: docs/spec-notes/vats-menu.md].
//!
//! - **The switch**: on by default [design: the fork owner's decision 2026-11-09, the
//!   game's own file first and the engine-drawn screen as the fallback]; `[UI]
//!   VatsMenuFile=0` in the home's `NVForever.ini` or the test aid
//!   `NVF_PLAY_TEST_VATS_MENU_FILE=0` keeps the engine-drawn screen ([`VatsTiles::on`]).
//!   It is its own key and not a name in `[UI] TileMenus=`: that list is empty by default
//!   and turns menus on.
//! - **The menu's life**: VATS is no `Game::menu` in NVForever, so the file is built only
//!   while the classic style chooses targets (`Game::ui_vats`, [`write_system`]), at once
//!   on the frame the screen comes up, and dropped on the frame it goes. While it is up
//!   `screen()` is written in interface units, as for the other menus drawn from their
//!   files (`super::interface_screen`), and its `cropx` and `cropy` are held at the safe
//!   zone the file places its arrows and buttons by ([`hold_crop`]).
//! - **What the model writes** ([`write`], every frame the screen is up, after the mode's
//!   own work and the frame's scripts): one `body_part_percent` copy per body part of the
//!   selected target (its place on the view, its side, its chance, its name, its
//!   condition), one `queued_action` row per queued attack, whether Accept can be pressed,
//!   and the special attack button's words. The rules stay `crate::vats_mode`'s.
//! - **What stays the engine's drawing**: the selected target's name and health at the top
//!   (the file leaves `EnemyHealth`, `HitPoints` and `ActionPoints` as empty boxes for
//!   code; the HUD's own meters go on being drawn), and the whole screen when the file is
//!   missing or lacks a path below ([`built`]): the reason is warned once.
//! - **Input** stays the mode's keys (`crate::vats_mode::read_keys`); a left click on one
//!   of the file's arrows or buttons does what it stands for ([`click`], [`hit_boxes`]).

use super::menus::{SizesWritten, Tiled};
use crate::settings::SettingsFile;
use crate::vats_mode::{ClassicView, VatsMode};
use crate::PlayerCamera;
use bevy::prelude::*;
use fnv_install::ini_get;
use fnv_ui::{Origin, TileId, Ui, Value};

/// The menu's name [documented: vats_menu.xml].
pub const VATS_MENU: &str = "VATSMenu";

/// The tiles the model writes or makes copies under [documented: vats_menu.xml].
const BRANCH: &str = "VATSMenu/GlowBranch";
const QUEUE: &str = "VATSMenu/GlowBranch/queued_actions";
const ACCEPT: &str = "VATSMenu/GlowBranch/accept_button";
const RETURN: &str = "VATSMenu/GlowBranch/return_button";
const SPECIAL: &str = "VATSMenu/GlowBranch/special_attack_button";
const SPECIAL_2: &str = "VATSMenu/GlowBranch/special_attack_2_button";
const SELECT: &str = "VATSMenu/GlowBranch/Select_button";
const BODY_PART: &str = "VATSMenu/GlowBranch/BodyPart_button";
const TARGET: &str = "VATSMenu/GlowBranch/Target_button";
const LEFT_ARROW: &str = "VATSMenu/GlowBranch/left_arrow";
const RIGHT_ARROW: &str = "VATSMenu/GlowBranch/right_arrow";
/// The templates and the tiles they make [documented: vats_menu.xml].
const PART_TEMPLATE: &str = "body_part_percent";
const PART: &str = "limb_percent";
const ROW_TEMPLATE: &str = "queued_action";
const ROW: &str = "queued_action_list_container";
const ROW_TEXT: &str = "ListItemText";

/// The INI key and the test aid of the switch (the module's notes).
pub const SECTION: &str = "UI";
pub const KEY: &str = "VatsMenuFile";
pub const AID: &str = "NVF_PLAY_TEST_VATS_MENU_FILE";

/// The stacking level: under the HUD drawn from tiles (`super::draw::TILES_Z`), over the
/// engine-drawn marks [design].
pub const VATS_Z: i32 = super::draw::TILES_Z - 1;

/// `screen()`'s `cropx` and `cropy` while the screen is up: the safe zone
/// (`super::dialog_menu`'s, [observed there: 15 at 1280 x 720; hypothesis: the same here]).
const CROP: f64 = 15.0;

/// The selected part's copy's `alpha`; the others are drawn whole [observed: the owner's
/// screenshots of the original's VATS, 2026-11-10: the box and name of the part the
/// pointer is on (the settler's head, Sunny Smiles's torso, both lit on the body) are
/// fainter than the other parts' boxes]. The copy's pieces copy its `alpha` and stand
/// under it, so 215 draws them at 181 [design: by eye, still easily read].
const DIM_ALPHA: f64 = 215.0;

/// Whether the switch's text turns the file off: `0`, `false`, `off` or `no`.
pub fn switched_off(text: &str) -> bool {
    matches!(text.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no")
}

/// [design] Every fixed path the model writes and both templates are part of a successful
/// build; a file a mod restructured keeps the engine-drawn screen.
pub fn built(ui: &Ui) -> Tiled {
    for path in [VATS_MENU, BRANCH, QUEUE, ACCEPT, RETURN, SPECIAL, SPECIAL_2, SELECT, BODY_PART, TARGET, LEFT_ARROW, RIGHT_ARROW] {
        if ui.find_tile(path).is_none() {
            return Tiled::No(format!("{path} missing"));
        }
    }
    for (at, template) in [(BRANCH, PART_TEMPLATE), (QUEUE, ROW_TEMPLATE)] {
        let mut look = ui.find_tile(at);
        let mut found = false;
        while let Some(t) = look {
            if ui.tile(t).templates.iter().any(|tp| tp.name.eq_ignore_ascii_case(template)) {
                found = true;
                break;
            }
            look = ui.tile(t).parent;
        }
        if !found {
            return Tiled::No(format!("template {template} missing"));
        }
    }
    Tiled::Yes
}

/// Which bracket a part's copy takes: the file picks the picture and the texts' places by
/// the copy's `justify` [documented: vats_menu.xml, `_chanceToHitType` 1 for `&right;`, 2
/// for `&left;`, 3 for `&down;` (the chest's bracket), 4 for `&up;` (the head's)].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Down,
    Up,
}

impl Side {
    /// The `justify` a box on this side of the body is given. The file's bracket for
    /// `&right;` has its upright line on the left and its tip on the right, the one for
    /// `&left;` the other way round [observed: the pictures as the engine draws them,
    /// 2026-11-10], and the tip is to point at the body [design: the fork owner
    /// 2026-11-10, "the arrow side should point towards the body part not away from it"]:
    /// a box left of the body takes `&right;`'s, one right of it `&left;`'s.
    fn justify(self) -> &'static str {
        match self {
            Side::Left => "right",
            Side::Right => "left",
            Side::Down => "down",
            Side::Up => "up",
        }
    }
}

/// The buttons' rows are this far apart [documented: vats_menu.xml, each button's `_y` is
/// the one before's and 47].
const BUTTON_ROW: f64 = 47.0;
/// The last row's top stands this far above the arrows' top, less a row; no row's top is
/// nearer the view's top than [`BUTTONS_LEAST`] [design].
const BUTTONS_GAP: f64 = 8.0;
const BUTTONS_LEAST: f64 = 10.0;

/// The tops (`_y`) of `count` button rows standing one under the other, the last ending
/// above the arrows, whose top is `arrow_top` (interface units) [design: the fork owner
/// 2026-11-10, "the button hints overlap with the left/right arrows, move the hints up but
/// make sure they do not go offscreen at 50% hud scale": the file starts its column 300
/// under the view's top, which is where the arrows stand now that they are halfway down;
/// hung from the arrows, the column is on the view at every scale].
pub fn button_rows(count: usize, arrow_top: f64) -> Vec<f64> {
    (0..count).map(|i| (arrow_top - BUTTONS_GAP - BUTTON_ROW * (count - i) as f64).max(BUTTONS_LEAST + BUTTON_ROW * i as f64)).collect()
}

/// A part's side [hypothesis: the pictures' names; which part gets which is the
/// original's code]: the head's bracket for the head (condition actor value 25, and the
/// brain's 31), the chest's for the torso (26), and for every other part the bracket on its
/// own side of the target's middle `middle` (interface units), so the brackets open away
/// from the body.
pub fn side_of(value: Option<u16>, x: f64, middle: f64) -> Side {
    match value {
        Some(25 | 31) => Side::Up,
        Some(26) => Side::Down,
        _ if x < middle => Side::Left,
        _ => Side::Right,
    }
}

/// One part as the file shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct PartShown {
    /// Its place among the target's parts (`ClassicPart::index`).
    pub index: usize,
    pub name: String,
    /// The chance in whole percent.
    pub chance: u32,
    /// The condition meter's share, 0 to 1 (0 for a part without a condition).
    pub condition: f64,
    pub selected: bool,
    pub side: Side,
    /// Its place in interface units.
    pub at: (f64, f64),
    /// How far it is from the camera, game units: the pointer takes the nearest.
    pub depth: f64,
    /// Where the part itself is on the screen, interface units (`at` is its box's place,
    /// beside it: [`spread`]).
    pub point: (f64, f64),
}

/// What the log tells of a [`Shown`]: how many parts, the selected one's name, the queue
/// and the special attack.
type Told = (usize, Option<String>, Vec<String>, Option<String>);

/// What the screen shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shown {
    /// Dead Eye's screen: the parts' boxes alone (`ClassicView::bare`).
    pub bare: bool,
    /// A controller works the screen: its buttons' names are the hints, and the buttons
    /// the file keeps for a controller show.
    pub pad: bool,
    pub parts: Vec<PartShown>,
    pub queue: Vec<String>,
    pub special: Option<String>,
}

impl Shown {
    /// `view` with each part's point placed by `place` (interface units; `None` for a
    /// point not on the view, whose part is left out).
    pub fn of(view: &ClassicView, place: impl Fn([f32; 3]) -> Option<((f64, f64), f64)>) -> Shown {
        let placed: Vec<_> = view.parts.iter().filter_map(|p| place(p.point).map(|(at, depth)| (p, at, depth))).collect();
        let middle = if placed.is_empty() { 0.0 } else { placed.iter().map(|(_, at, _)| at.0).sum::<f64>() / placed.len() as f64 };
        let parts = placed
            .into_iter()
            .map(|(p, at, depth)| PartShown {
                depth,
                index: p.index,
                name: p.name.clone(),
                chance: p.chance.clamp(0.0, 100.0).round() as u32,
                condition: f64::from(p.condition.unwrap_or(0.0).clamp(0.0, 1.0)),
                selected: p.selected,
                side: side_of(p.value, at.0, middle),
                at,
                point: at,
            })
            .collect::<Vec<_>>();
        let mut parts = parts;
        spread(&mut parts);
        Shown { bare: view.bare, pad: false, parts, queue: view.queue.clone(), special: view.special.clone() }
    }

    /// What the log tells of it (not the places or the chances, which move every frame of
    /// a slowed clock).
    fn told(&self) -> Told {
        (self.parts.len(), self.parts.iter().find(|p| p.selected).map(|p| p.name.clone()), self.queue.clone(), self.special.clone())
    }
}

/// How far a part's box stands out from the target: a limb's box past the outermost part
/// on its side, the head's box above the head, interface units; and the room a
/// box takes with its bracket, its meter and its chance, within which no other box's
/// place may lie [observed: the owner's screenshot of the original's VATS, 2026-11-10, a
/// Jackal Gang Member: the head's box above the head, the arms' and the legs' boxes out
/// to each side at their own height, the torso's on the chest, none over another;
/// design: the distances, by eye].
const OUT_SIDE: f64 = 95.0;
const OUT_UP: f64 = 85.0;
const BOX_ROOM: (f64, f64) = (140.0, 80.0);

/// Set each part's box beside its part and apart from the others (`at`, from `point` and
/// `side`): the head's above the head, a limb's out to its side at the limb's own
/// height, the torso's on it; then any two whose rooms still meet are moved apart up and
/// down, half the way each, until none does.
pub fn spread(parts: &mut [PartShown]) {
    if parts.is_empty() {
        return;
    }
    let left = parts.iter().map(|p| p.point.0).fold(f64::INFINITY, f64::min);
    let right = parts.iter().map(|p| p.point.0).fold(f64::NEG_INFINITY, f64::max);
    for p in parts.iter_mut() {
        p.at = match p.side {
            Side::Left => (left - OUT_SIDE, p.point.1),
            Side::Right => (right + OUT_SIDE, p.point.1),
            Side::Up => (p.point.0, p.point.1 - OUT_UP),
            Side::Down => p.point,
        };
    }
    for _ in 0..16 {
        let mut moved = false;
        for i in 0..parts.len() {
            for j in i + 1..parts.len() {
                let (a, b) = (parts[i].at, parts[j].at);
                let down = b.1 - a.1;
                if (a.0 - b.0).abs() < BOX_ROOM.0 && down.abs() < BOX_ROOM.1 {
                    let push = (BOX_ROOM.1 - down.abs()) / 2.0 + 0.5;
                    let way = if down >= 0.0 { 1.0 } else { -1.0 };
                    parts[i].at.1 -= way * push;
                    parts[j].at.1 += way * push;
                    moved = true;
                }
            }
        }
        if !moved {
            break;
        }
    }
}

/// A window point (logical pixels) in the screen's interface units: the screen is drawn
/// as the HUD is, from the window's top left corner, an interface unit `ui_scale` (at
/// most 1, `crate::interface::menu_scale`) times the window's height over 960
/// (`super::draw`, `super::write_screen_scaled`).
pub fn to_units(point: (f32, f32), window: (f32, f32), ui_scale: f32) -> (f64, f64) {
    let unit = window.1 / 960.0 * ui_scale;
    if unit <= 0.0 {
        return (0.0, 0.0);
    }
    (f64::from(point.0 / unit), f64::from(point.1 / unit))
}

fn copies(ui: &Ui, under: &str, name: &str) -> Vec<TileId> {
    let Some(parent) = ui.find_tile(under) else { return Vec::new() };
    ui.tile(parent).children.iter().copied().filter(|&c| ui.is_live(c) && ui.tile(c).origin == Origin::Code && ui.tile(c).name.eq_ignore_ascii_case(name)).collect()
}

/// The copies the model made: the parts' and the queue's rows, in the order made.
pub fn part_tiles(ui: &Ui) -> Vec<TileId> {
    copies(ui, BRANCH, PART)
}

pub fn row_tiles(ui: &Ui) -> Vec<TileId> {
    copies(ui, QUEUE, ROW)
}

/// As many copies of `template` under `under` as `n`, the made ones kept.
fn fit(ui: &mut Ui, under: &str, template: &str, name: &str, n: usize) -> Vec<TileId> {
    let mut made = copies(ui, under, name);
    while made.len() > n {
        if let Some(t) = made.pop() {
            ui.unload_tile(t);
        }
    }
    while made.len() < n {
        // The templates' includes were spliced in when the file was read (menu-xml.md §2.2).
        match ui.add_code_tile_from_template(&fnv_ui::MemSource::default(), under, template, None) {
            Some(t) => made.push(t),
            None => break,
        }
    }
    made
}

fn sym(name: &str) -> Value {
    Value::Sym(name.into())
}

fn flag(on: bool) -> Value {
    sym(if on { "true" } else { "false" })
}

/// Write what `VATSMenu` shows of `shown` [documented: vats_menu.xml, the traits its
/// expressions read and leave unset; design: what each is given]:
/// - the menu's `alpha` 255: the file starts it at 0 with an `explorefade` of a quarter
///   second [hypothesis: the original's code fades the menu in; here it is shown at once,
///   as the clock stops at once];
/// - one `limb_percent` per part: `x` and `y` its place, `justify` its side's bracket
///   ([`side_of`], [`Side::justify`]),
///   `string` its chance in whole percent with its sign [observed: the owner's
///   screenshots of the original's VATS, 2026-11-10, "41%", "62%"], `_LimbName` its
///   name and `_LimbNameVisible` on for the selected part [design], `_MeterPercent`
///   its condition (the file's meter is 70 wide times it) and `alpha` [`DIM_ALPHA`] for
///   the selected part (it is lit on the body instead), 255 for the others;
/// - one `queued_action_list_container` per queued attack under `queued_actions`: its
///   `string` and `listindex` (the file stacks the rows up from the list's bottom by it);
/// - `accept_button`'s `target` on with an attack queued (the file dims a button that is
///   no target); `special_attack_button`'s `string` and `visible` for a weapon with a
///   special attack; `special_attack_2_button` hidden (no weapon record has a second).
pub fn write(ui: &mut Ui, shown: &Shown) {
    if !built(ui).yes() {
        return;
    }
    if let Some(menu) = ui.menu(VATS_MENU) {
        ui.set_trait(menu, "alpha", Value::Num(255.0));
    }
    let parts = fit(ui, BRANCH, PART_TEMPLATE, PART, shown.parts.len());
    for (&t, p) in parts.iter().zip(&shown.parts) {
        ui.set_trait(t, "x", Value::Num(p.at.0));
        ui.set_trait(t, "y", Value::Num(p.at.1));
        ui.set_trait(t, "justify", sym(p.side.justify()));
        ui.set_trait(t, "string", Value::Str(format!("{}%", p.chance)));
        ui.set_trait(t, "_LimbName", Value::Str(p.name.clone()));
        ui.set_trait(t, "_LimbNameVisible", flag(p.selected && !p.name.is_empty()));
        ui.set_trait(t, "_MeterPercent", Value::Num(p.condition));
        // (In Dead Eye too: the part under the crosshair is lit on the body there as the
        // selected one is here.)
        ui.set_trait(t, "alpha", Value::Num(if p.selected { DIM_ALPHA } else { 255.0 }));
    }
    let rows = fit(ui, QUEUE, ROW_TEMPLATE, ROW, shown.queue.len());
    for (i, (&t, words)) in rows.iter().zip(&shown.queue).enumerate() {
        ui.set_trait(t, "string", Value::Str(words.clone()));
        ui.set_trait(t, "listindex", Value::Num(i as f64));
        // The row is a rect of `alpha` 0 (list_box_template.xml) over its text, and this
        // renderer passes a rect's alpha down (`super::layout`): shown whole [design: a
        // row that hid its own words would serve nothing].
        ui.set_trait(t, "alpha", Value::Num(255.0));
        // Its words are right-justified at 15 from the row's left edge, so they lie left of
        // the row's own box, which crops what is in it (`clips`): the words are marked as
        // outside that crop [design, for the same reason; the list's box itself starts 65
        // from the view's right edge and is 350 wide, so its rows can only be read there].
        if let Some(text) = ui.tile(t).children.iter().copied().find(|&c| ui.is_live(c) && ui.tile(c).name.eq_ignore_ascii_case(ROW_TEXT)) {
            ui.set_trait(text, fnv_ui::input::OUTSIDE_CLIP, Value::Num(1.0));
        }
    }
    // The list's `childcount`, which the file's rows and separator read and the tree does
    // not keep by itself [hypothesis: the original's code counts a tile's children; the
    // file takes one off "for the scroll bar" and shows the separator above 2, so the
    // highlight box and the scroll bar count].
    if let Some(list) = ui.find_tile(QUEUE) {
        let count = ui.tile(list).children.iter().filter(|&&c| ui.is_live(c)).count();
        ui.set_trait(list, "childcount", Value::Num(count as f64));
    }
    if let Some(t) = ui.find_tile(ACCEPT) {
        ui.set_trait(t, "target", flag(!shown.queue.is_empty()));
    }
    // Return's hint in this screen's own short words ([`RETURN_HINT`]); a controller's
    // names replace it below.
    if let Some(t) = ui.find_tile(RETURN) {
        ui.set_trait(t, "_PCButtonText", Value::Str(RETURN_HINT.into()));
    }
    // Dead Eye's screen is the parts' boxes alone: its marks, keys and free aim are its
    // own, so the arrows and the buttons are not drawn (on the classic screen the two
    // arrows keep the file's own rule, shown off the console).
    for path in [ACCEPT, RETURN] {
        if let Some(t) = ui.find_tile(path) {
            ui.set_trait(t, "visible", flag(!shown.bare));
        }
    }
    // The arrows stand halfway down the view, on one line, whatever the scale [design:
    // the fork owner 2026-11-10; the file puts them a quarter of the way down, as the
    // owner's screenshots of the original show them].
    let view_height = ui.tile(ui.screen()).get("height").map_or(960.0, Value::num);
    let mut arrow_top = view_height / 2.0;
    for path in [LEFT_ARROW, RIGHT_ARROW] {
        if let Some(t) = ui.find_tile(path) {
            ui.set_trait(t, "visible", flag(!shown.bare));
            let height = ui.tile(t).get("height").map_or(0.0, Value::num);
            ui.set_trait(t, "y", Value::Num(view_height / 2.0 - height / 2.0));
            arrow_top = arrow_top.min(view_height / 2.0 - height / 2.0);
        }
    }
    // The buttons that can show, in the file's order, one row each above the arrows
    // ([`button_rows`]). Select keeps its row while it is not shown, so the others do not
    // jump as the pointer comes onto a part.
    let column: Vec<&str> =
        [Some(SELECT), Some(ACCEPT), Some(RETURN), shown.special.is_some().then_some(SPECIAL), shown.pad.then_some(TARGET), shown.pad.then_some(BODY_PART)].into_iter().flatten().collect();
    for (path, top) in column.iter().zip(button_rows(column.len(), arrow_top)) {
        if let Some(t) = ui.find_tile(path) {
            ui.set_trait(t, "_y", Value::Num(top));
        }
    }
    if let Some(t) = ui.find_tile(SPECIAL) {
        ui.set_trait(t, "string", Value::Str(shown.special.clone().unwrap_or_default()));
        ui.set_trait(t, "visible", flag(shown.special.is_some() && !shown.bare));
    }
    if let Some(t) = ui.find_tile(SPECIAL_2) {
        ui.set_trait(t, "visible", flag(false));
    }
    // With a controller: its buttons' names as the hints, as the other menus drawn from
    // their files show them ("[A]", `crate::pad::game_hint`; the file's own button
    // pictures are not drawn), and the three buttons the file keeps for a controller
    // [documented: vats_menu.xml, `_xbox_button` A, B, Y, RT, the left and right sticks].
    // The menu is built again each time the screen comes up, so the keyboard's hints are
    // the file's own again then.
    if shown.pad && !shown.bare {
        use bevy::prelude::GamepadButton as B;
        let name = |b: B| format!("[{}]", crate::pad::pad_shown(crate::pad::PadInput::Button(b)));
        for (path, hint, visible) in [
            (ACCEPT, name(B::South), true),
            (RETURN, name(B::East), true),
            (SPECIAL, name(B::North), shown.special.is_some()),
            (SELECT, name(B::RightTrigger2), true),
            (BODY_PART, "[L Stick]".to_string(), true),
            (TARGET, "[R Stick]".to_string(), true),
        ] {
            if let Some(t) = ui.find_tile(path) {
                ui.set_trait(t, "_PCButtonText", Value::Str(hint));
                ui.set_trait(t, "visible", flag(visible));
            }
        }
    }
}

/// The rows' and the list's heights, once the rows' texts are measured: a row as high as
/// its text and its `_VerticalSpacing` [documented: list_box_template.xml, "only used if
/// height == 0"], the list `_number_of_visible_items` rows high [documented: list_box.xml,
/// "the height and _scroll_delta traits will be overwritten in code to compensate"]. With
/// no row yet the list keeps the height last written.
pub fn write_heights(ui: &mut Ui) {
    let rows = row_tiles(ui);
    let num = |ui: &Ui, t: TileId, n: &str| ui.tile(t).get(n).map_or(0.0, Value::num);
    let mut row_height = 0.0;
    for &row in &rows {
        let text = ui.tile(row).children.iter().copied().find(|&c| ui.is_live(c) && ui.tile(c).name.eq_ignore_ascii_case(ROW_TEXT));
        let height = text.map_or(0.0, |t| num(ui, t, "height")) + num(ui, row, "_VerticalSpacing");
        ui.set_trait(row, "height", Value::Num(height));
        row_height = height;
    }
    if let (Some(list), true) = (ui.find_tile(QUEUE), row_height > 0.0) {
        let visible = num(ui, list, "_number_of_visible_items").max(1.0);
        ui.set_trait(list, "height", Value::Num(visible * row_height));
        ui.set_trait(list, "_scroll_delta", Value::Num(row_height));
    }
}

/// `screen()`'s `cropx` and `cropy` as they were before the screen held them, and what it
/// wrote (`super::dialog_menu`'s way: a script's own value since stands).
struct CropKept {
    screen: TileId,
    before: [Option<Value>; 2],
}

const CROPS: [&str; 2] = ["cropx", "cropy"];

fn hold_crop(ui: &mut Ui, kept: &mut Option<CropKept>) {
    let screen = ui.screen();
    if kept.as_ref().is_some_and(|k| k.screen != screen) {
        *kept = None;
    }
    if kept.is_none() {
        *kept = Some(CropKept { screen, before: CROPS.map(|n| ui.tile(screen).get(n).cloned()) });
        for name in CROPS {
            ui.set_trait(screen, name, Value::Num(CROP));
        }
    }
}

fn release_crop(ui: &mut Ui, kept: &mut Option<CropKept>) {
    let Some(k) = kept.take() else { return };
    let screen = ui.screen();
    if k.screen != screen {
        return;
    }
    for (name, before) in CROPS.into_iter().zip(k.before) {
        // A script's own value since stands.
        if ui.tile(screen).get(name).map(Value::num) != Some(CROP) {
            continue;
        }
        match before {
            Some(v) => ui.set_trait(screen, name, v),
            None => {
                ui.remove_trait(screen, name);
            }
        }
    }
}

/// How far left of its hint a button's words reach, interface units [design: "Accept",
/// "Return" and "Select" are about 110 wide in the menu's font; observed: the engine's
/// screenshot at 1280 x 720].
pub const BUTTON_WORDS: f64 = 120.0;

/// A button of the target screen the pointer can click.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenButton {
    Select,
    Accept,
    Return,
    Special,
}

/// What a left click on the target screen does (`crate::vats_mode`'s keys): on an arrow
/// the target before or after, on a button what the button says, anywhere else an attack
/// queued on the selected part, as the attack control does without a pointer. Accept is
/// no click target with nothing queued (the file's own rule, `target`), so a click on it
/// then does nothing [the owner's review of #2102: a click on a button queued one more
/// attack].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Click {
    Queue,
    Target(i32),
    Accept,
    Return,
    Special,
    Nothing,
}

pub fn click(arrow: Option<i32>, button: Option<ScreenButton>, queued: bool) -> Click {
    match (arrow, button) {
        (Some(step), _) => Click::Target(step),
        (None, Some(ScreenButton::Accept)) if queued => Click::Accept,
        (None, Some(ScreenButton::Accept)) => Click::Nothing,
        (None, Some(ScreenButton::Return)) => Click::Return,
        (None, Some(ScreenButton::Special)) => Click::Special,
        (None, Some(ScreenButton::Select) | None) => Click::Queue,
    }
}

/// The boxes the pointer can be on, interface units: each part copy's (in the copies'
/// order), the two arrows' with the step each stands for, and the shown buttons'.
#[derive(Default)]
pub struct HitBoxes {
    pub parts: Vec<Option<super::layout::Rect>>,
    pub arrows: Vec<(i32, super::layout::Rect)>,
    pub buttons: Vec<(ScreenButton, super::layout::Rect)>,
}

fn inside(r: &super::layout::Rect, at: (f64, f64), pad: f64) -> bool {
    at.0 >= r.left - pad && at.0 <= r.left + r.width + pad && at.1 >= r.top - pad && at.1 <= r.top + r.height + pad
}

/// Where the file drew the things the pointer can be on: a part's box is the room its
/// chance and its meter take together (its bracket's picture is far larger than what
/// shows of it).
pub fn hit_boxes(ui: &Ui, menu: TileId) -> HitBoxes {
    let items = super::layout::layout(ui, menu, &|_, _, _| (0.0, 0.0), true);
    let parts = part_tiles(ui)
        .into_iter()
        .map(|part| {
            let mut all: Option<super::layout::Rect> = None;
            for i in items.iter().filter(|i| ui.tile(i.tile).parent == Some(part) && ["chance_to_hit", "meter"].iter().any(|n| ui.tile(i.tile).name.eq_ignore_ascii_case(n))) {
                // A meter draws from its own place by its `justify`; its width is the room.
                let r = i.rect;
                all = Some(match all {
                    None => r,
                    Some(a) => {
                        let (left, top) = (a.left.min(r.left), a.top.min(r.top));
                        let (right, bottom) = ((a.left + a.width).max(r.left + r.width), (a.top + a.height).max(r.top + r.height));
                        super::layout::Rect { left, top, width: right - left, height: bottom - top }
                    }
                });
            }
            all
        })
        .collect();
    let arrows = [(-1, LEFT_ARROW), (1, RIGHT_ARROW)].into_iter().filter_map(|(step, path)| {
        let tile = ui.find_tile(path)?;
        items.iter().find(|i| i.tile == tile && i.visible).map(|i| (step, i.rect))
    });
    // A button's box: its own and its words' and hint's, whatever of it is shown.
    let within = |tile: TileId, of: TileId| std::iter::successors(Some(tile), |&t| ui.tile(t).parent).any(|t| t == of);
    let buttons = [(ScreenButton::Select, SELECT), (ScreenButton::Accept, ACCEPT), (ScreenButton::Return, RETURN), (ScreenButton::Special, SPECIAL)].into_iter().filter_map(|(button, path)| {
        let tile = ui.find_tile(path)?;
        // (A button is a container and has no item of its own: its shown pieces are it.)
        let mut all: Option<super::layout::Rect> = None;
        for r in items.iter().filter(|i| i.visible && within(i.tile, tile)).map(|i| i.rect).filter(|r| r.width > 0.0 && r.height > 0.0) {
            all = Some(match all {
                None => r,
                Some(a) => {
                    let (left, top) = (a.left.min(r.left), a.top.min(r.top));
                    let (right, bottom) = ((a.left + a.width).max(r.left + r.width), (a.top + a.height).max(r.top + r.height));
                    super::layout::Rect { left, top, width: right - left, height: bottom - top }
                }
            });
        }
        // The words' own width is the renderer's to measure and is not in these boxes
        // (they begin at the words' right end, where the hint stands): the box reaches
        // [`BUTTON_WORDS`] further left, over the words.
        all.map(|r| (button, super::layout::Rect { left: r.left - BUTTON_WORDS, width: r.width + BUTTON_WORDS, ..r }))
    });
    HitBoxes { parts, arrows: arrows.collect(), buttons: buttons.collect() }
}

/// How far round a part's box, and how far from a part's own point on the target, the
/// pointer still counts as on it, interface units [design: the original's pointer picks a
/// part on the body itself, observed on the owner's screenshot with the pointer on Sunny
/// Smiles's torso and the torso lit; here the nearest part's point stands in for the
/// body's shape].
const BOX_PAD: f64 = 6.0;
const POINT_REACH: f64 = 40.0;

/// The parts the pointer at `at` may be on (their `index`): first, of the parts whose box
/// it is in, the one nearest the camera; second, of the parts whose point on the target
/// is within [`POINT_REACH`], the one nearest the camera [design: the fork owner
/// 2026-11-10, "front body parts closer to the player should have selection priority":
/// where two parts' boxes lie over each other the near leg is taken, not the far one].
/// Between the two the mode asks the body itself (`crate::vats_mode`, the pointer's line
/// into the world): the second is only for a pointer beside the body.
pub fn part_under(shown: &Shown, boxes: &[Option<super::layout::Rect>], at: (f64, f64)) -> (Option<usize>, Option<usize>) {
    let front = |a: &&PartShown, b: &&PartShown| a.depth.total_cmp(&b.depth);
    let boxed = shown.parts.iter().zip(boxes).filter(|(_, r)| r.as_ref().is_some_and(|r| inside(r, at, BOX_PAD))).map(|(p, _)| p).min_by(front).map(|p| p.index);
    let away = |p: &PartShown| ((p.point.0 - at.0).powi(2) + (p.point.1 - at.1).powi(2)).sqrt();
    let near = shown.parts.iter().filter(|p| away(p) <= POINT_REACH).min_by(front).map(|p| p.index);
    (boxed, near)
}

impl VatsTiles {
    /// The part that lies the way `dir` (a unit step on the screen, y down) from the
    /// selected one: of the parts within 60 degrees of that way, the nearest; none when no
    /// part is selected or none lies that way (the controller's left stick,
    /// `crate::vats_mode`).
    pub fn toward(&self, dir: (f64, f64)) -> Option<usize> {
        step_toward(&self.places, dir)
    }
}

/// [`VatsTiles::toward`] over `places` (each part's index, place and whether it is the
/// selected one).
pub fn step_toward(places: &[(usize, (f64, f64), bool)], dir: (f64, f64)) -> Option<usize> {
    let from = places.iter().find(|p| p.2)?.1;
    places
        .iter()
        .filter(|p| !p.2)
        .filter_map(|p| {
            let v = (p.1 .0 - from.0, p.1 .1 - from.1);
            let len = (v.0 * v.0 + v.1 * v.1).sqrt();
            let along = v.0 * dir.0 + v.1 * dir.1;
            (len > 0.0 && along / len >= 0.5).then_some((len, p.0))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, i)| i)
}

/// The Select and Return buttons' hints on this screen: NVForever's own short words for
/// the left and the right mouse button. The file gives a hint its button's own width as
/// room (vats-menu.md §2), and the built-in `sPCMenuHintRMB`, "[Right Click]"
/// (`fnv_ui::strings`, shared with the other menus), ran off the view after "Return"
/// [observed: the engine's screenshot at 1280 x 720]. The original's on-screen
/// abbreviations are not carried over (LEGAL.md; the owner's review of #2102).
pub const SELECT_HINT: &str = "[LClick]";
pub const RETURN_HINT: &str = "[RClick]";

/// What the file shows for the pointer: the Select button with the left mouse button's
/// hint while the pointer is on a part [observed: the owner's screenshots of the
/// original's VATS, 2026-11-10: Select and its hint above Return with the pointer on a
/// part, absent with the pointer off the target; design: NVForever's own hint words,
/// [`SELECT_HINT`]], and each arrow's `mouseover`, by which the file switches its picture.
pub fn write_pointer(ui: &mut Ui, on_part: bool, arrow: Option<i32>) {
    if let Some(t) = ui.find_tile(SELECT) {
        ui.set_trait(t, "visible", flag(on_part));
        ui.set_trait(t, "_PCButtonText", Value::Str(SELECT_HINT.into()));
    }
    for (step, path) in [(-1, LEFT_ARROW), (1, RIGHT_ARROW)] {
        if let Some(t) = ui.find_tile(path) {
            ui.set_trait(t, "mouseover", Value::Num(f64::from(u8::from(arrow == Some(step)))));
        }
    }
}

/// The screen's state between frames.
#[derive(Resource, Default)]
pub struct VatsTiles {
    /// The switch (the module's notes).
    pub on: bool,
    /// This frame the target screen is the file's: `super::draw` draws `VATSMenu` and
    /// `crate::vats_mode` leaves its own labels and bars out.
    pub drawn: bool,
    /// The part the pointer is on (its `ClassicPart::index`), whether the pointer moved
    /// this frame, and the arrow it is on (-1 the left one, 1 the right one): read by the
    /// mode's keys on the next frame (`crate::vats_mode`).
    pub hover: Option<usize>,
    pub hover_moved: bool,
    pub arrow: Option<i32>,
    /// The button the pointer is on ([`click`]), and the buttons last told in the log.
    pub button: Option<ScreenButton>,
    buttons_told: Vec<ScreenButton>,
    /// The part whose point on the target is nearest the pointer, within reach, and the
    /// pointer's line into the world (from, direction; game space), with which the mode
    /// finds the part of the body under it.
    pub near: Option<usize>,
    pub ray: Option<([f32; 3], [f32; 3])>,
    /// Where each part stands on the screen, and which is selected: for the controller's
    /// left stick ([`VatsTiles::toward`]).
    places: Vec<(usize, (f64, f64), bool)>,
    pointer: Option<(f32, f32)>,
    /// Why the file is not used for the rest of the run, once it failed.
    failed: Option<String>,
    told: Option<Told>,
    sizes: SizesWritten,
    crop: Option<CropKept>,
}

/// Read the switch once at start and tell it when it is off.
pub(crate) fn setup(app: &mut App) {
    let unattended = crate::test_aid::Unattended::from_env().0;
    let from_ini = || {
        let path = SettingsFile::for_run(unattended).0?;
        let text = std::fs::read_to_string(path).ok()?;
        ini_get(&text, SECTION, KEY)
    };
    let (text, from) = match std::env::var(AID) {
        Ok(v) => (Some(v), format!("test aid {AID}")),
        Err(_) => (from_ini(), format!("[{SECTION}] {KEY}")),
    };
    let on = !text.as_deref().is_some_and(switched_off);
    if !on {
        info!("tiles: {VATS_MENU} not drawn from its file ({from}): the classic VATS screen is the engine's own");
    }
    app.insert_resource(VatsTiles { on, ..Default::default() });
}

/// Build, write or drop the screen for this frame (the module's notes): after the mode's
/// work and the frame's scripts, before the tree is worked out and drawn.
pub(crate) fn write_system(
    mut script: ResMut<crate::script::ScriptState>,
    vats: Option<Res<VatsMode>>,
    files: super::MenuFiles,
    mut fonts: super::draw::FontKit,
    cameras: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut st: ResMut<VatsTiles>,
) {
    let Some(rt) = script.rt.as_mut() else {
        st.drawn = false;
        return;
    };
    let game = &mut rt.game;
    let up = st.on && st.failed.is_none() && game.ui_files.is_some() && vats.as_ref().is_some_and(|v| v.screen_up());
    if !up {
        if game.ui_vats || st.drawn {
            release_crop(&mut game.ui_tree, &mut st.crop);
            game.ui_vats = false;
            // Drops the menu and gives `screen()` its units back now.
            files.write(game);
            st.drawn = false;
            st.told = None;
            st.sizes.clear();
        }
        (st.hover, st.hover_moved, st.arrow, st.button, st.near, st.ray) = (None, false, None, None, None, None);
        st.places.clear();
        st.buttons_told.clear();
        return;
    }
    if !game.ui_vats {
        game.ui_vats = true;
        hold_crop(&mut game.ui_tree, &mut st.crop);
        // Builds the menu and writes `screen()` in interface units now, so the screen is
        // the file's on its first frame.
        files.write(game);
    }
    let status = if game.ui_tree.menu(VATS_MENU).is_some() { built(&game.ui_tree) } else { Tiled::No(format!("{VATS_MENU}: its file did not build")) };
    if let Tiled::No(reason) = status {
        warn!("{VATS_MENU} drawn by hand: {reason}");
        st.failed = Some(reason);
        release_crop(&mut game.ui_tree, &mut st.crop);
        game.ui_vats = false;
        files.write(game);
        st.drawn = false;
        return;
    }
    let Some(menu) = game.ui_tree.menu(VATS_MENU) else { return };
    // (Before the parts are worked out: the screen of the style, with none yet.)
    let bare = vats.as_ref().is_some_and(|v| v.style() == crate::vats_classic::VatsStyle::DeadEye);
    let view = vats.as_ref().and_then(|v| v.target_screen()).unwrap_or(ClassicView { bare, ..Default::default() });
    let pad = vats.as_ref().is_some_and(|v| v.pad());
    let window = files.window_size().unwrap_or((1280.0, 720.0));
    let scale = crate::interface::menu_scale();
    let camera = cameras.single().ok();
    let shown = Shown::of(&view, |p| {
        let (cam, t) = camera?;
        let world = crate::space::to_bevy(p);
        let at = cam.world_to_viewport(t, world).ok()?;
        Some((to_units((at.x, at.y), window, scale), f64::from(t.translation().distance(world))))
    });
    let shown = Shown { pad, ..shown };
    let ui = &mut game.ui_tree;
    write(ui, &shown);
    // The copies' texts read their strings before they are measured, and the rows' heights
    // follow from the measures.
    ui.update();
    let texts = super::menus::texts(ui, menu);
    {
        let size = fonts.menu_measure(&super::menus::fonts_of(ui, &texts));
        super::menus::write_text_sizes_of(ui, &texts, &mut st.sizes, &size);
    }
    write_heights(ui);
    // The pointer: the part and the arrow it is on, for the mode's keys next frame, and
    // what the file shows for it now.
    // (Dead Eye's screen has no pointer: the crosshair chooses. Nor has a controller's.)
    let pointer = files.pointer().filter(|_| !shown.bare && !pad);
    st.hover_moved = pointer.is_some() && pointer != st.pointer;
    st.pointer = pointer;
    let under = pointer.map(|p| to_units(p, window, scale));
    let boxes = hit_boxes(ui, menu);
    (st.hover, st.near) = under.map_or((None, None), |at| part_under(&shown, &boxes.parts, at));
    st.arrow = under.and_then(|at| boxes.arrows.iter().find(|(_, r)| inside(r, at, 0.0)).map(|(step, _)| *step));
    st.button = under.and_then(|at| boxes.buttons.iter().find(|(_, r)| inside(r, at, 0.0)).map(|(b, _)| *b));
    // Told once a stay and when the buttons change (Select comes and goes with the pointer).
    let kinds: Vec<ScreenButton> = boxes.buttons.iter().map(|(b, _)| *b).filter(|b| *b != ScreenButton::Select).collect();
    if !shown.bare && !pad && kinds != st.buttons_told {
        if !kinds.is_empty() {
            let each = |(b, r): &(ScreenButton, super::layout::Rect)| format!("{b:?} at {:.0}, {:.0} ({:.0} x {:.0})", r.left, r.top, r.width, r.height);
            info!("tiles: {VATS_MENU}: the pointer can click {}", boxes.buttons.iter().filter(|(b, _)| *b != ScreenButton::Select).map(each).collect::<Vec<_>>().join("; "));
        }
        st.buttons_told = kinds;
    }
    // The pointer's line into the world, for the part of the body under it.
    st.ray = pointer.zip(camera).and_then(|(p, (cam, t))| cam.viewport_to_world(t, Vec2::new(p.0, p.1)).ok()).map(|ray| {
        let dir = crate::space::to_game(*ray.direction);
        let len = (dir[0] * dir[0] + dir[1] * dir[1] + dir[2] * dir[2]).sqrt().max(1e-6);
        (crate::space::to_game(ray.origin), [dir[0] / len, dir[1] / len, dir[2] / len])
    });
    st.places = shown.parts.iter().map(|p| (p.index, p.at, p.selected)).collect();
    // Select shows with the pointer on a part, by its box here or by the mode's own
    // finding on the body (`VatsMode::pointed`, a frame behind); a controller's is
    // written with its other buttons (`write`).
    if !pad {
        write_pointer(ui, st.hover.is_some() || vats.as_ref().is_some_and(|v| v.pointed()), st.arrow);
    }
    st.drawn = true;
    let told = shown.told();
    if st.told.as_ref() != Some(&told) {
        info!("{}", describe(&shown));
        st.told = Some(told);
    }
}

/// A line for the log when what the screen shows changes (smoke reads these).
pub fn describe(shown: &Shown) -> String {
    let selected = shown.parts.iter().find(|p| p.selected).map_or_else(|| "none".to_string(), |p| format!("\"{}\" {}", p.name, p.chance));
    let special = shown.special.as_ref().map_or_else(|| "no special attack".to_string(), |s| format!("special \"{s}\""));
    if shown.bare {
        return format!("tiles: {VATS_MENU} drawn from tiles for Dead Eye: {} parts, under the crosshair {selected}", shown.parts.len());
    }
    format!("tiles: {VATS_MENU} drawn from tiles: {} parts, selected {selected}, {} queued [{}], {special}", shown.parts.len(), shown.queue.len(), shown.queue.join(", "))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vats_mode::ClassicPart;
    use fnv_ui::MemSource;

    /// [design] A menu in the vanilla file's shape (its names, its two templates and the
    /// traits the copies' pieces read), cut down to what these tests read.
    fn menu_changed(change: impl FnOnce(&str) -> String) -> Ui {
        let src = MemSource::default().with(
            "vats.xml",
            &change(
                r#"<menu name="VATSMenu"><alpha>0</alpha><rect name="GlowBranch">
  <template name="body_part_percent"><rect name="limb_percent"><alpha>0</alpha><justify>&down;</justify><_MeterPercent>1.0</_MeterPercent>
    <_type><copy>0</copy><add><copy>2</copy><onlyif><copy src="me()" trait="justify"/><eq>&left;</eq></onlyif></add>
      <add><copy>1</copy><onlyif><copy src="me()" trait="justify"/><eq>&right;</eq></onlyif></add>
      <add><copy>3</copy><onlyif><copy src="me()" trait="justify"/><eq>&down;</eq></onlyif></add>
      <add><copy>4</copy><onlyif><copy src="me()" trait="justify"/><eq>&up;</eq></onlyif></add></_type>
    <text name="limb_name"><string><copy src="parent()" trait="_LimbName"/></string><visible><copy src="parent()" trait="_LimbNameVisible"/></visible></text>
    <text name="chance_to_hit"><string><copy src="parent()" trait="string"/></string><alpha><copy src="parent()" trait="alpha"/></alpha></text>
    <rect name="meter"><width>70</width><_Value><copy src="parent()" trait="_MeterPercent"/></_Value>
      <image name="MeterImage"><width><copy src="parent()" trait="width"/><mul src="parent()" trait="_Value"/></width></image></rect>
  </rect></template>
  <image name="left_arrow"><x>10</x><y>200</y><width>86</width><height>134</height></image>
  <image name="right_arrow"><x>1600</x><y>200</y><width>86</width><height>134</height></image>
  <hotrect name="Select_button"><visible>&false;</visible></hotrect>
  <hotrect name="BodyPart_button"><visible>&false;</visible></hotrect>
  <hotrect name="Target_button"><visible>&false;</visible></hotrect>
  <hotrect name="accept_button"><target>&false;</target></hotrect>
  <hotrect name="return_button"/>
  <hotrect name="special_attack_button"><string> </string></hotrect>
  <hotrect name="special_attack_2_button"><string> </string></hotrect>
  <hotrect name="queued_actions"><height></height><_number_of_visible_items>6</_number_of_visible_items><_scroll_delta>26</_scroll_delta>
    <hotrect name="lb_highlight_box"/><image name="lb_scrollbar"/></hotrect>
  <template name="queued_action"><rect name="queued_action_list_container"><height>0</height><_VerticalSpacing>0</_VerticalSpacing>
    <y><copy src="queued_actions" trait="height"/><sub><copy src="queued_actions" trait="childcount"/><sub>1</sub><sub src="me()" trait="listindex"/><mult src="me()" trait="height"/></sub><add src="me()" trait="height"/></y>
    <text name="ListItemText"><string><copy src="parent()" trait="string"/></string></text></rect></template>
</rect></menu>"#,
            ),
        );
        let mut ui = Ui::new();
        ui.load_menu(&src, "vats.xml").unwrap();
        ui.update();
        ui
    }

    fn part(name: &str, chance: f32, value: Option<u16>, selected: bool, x: f32) -> ClassicPart {
        ClassicPart { index: 0, name: name.into(), chance, condition: Some(0.5), value, selected, point: [x, 0.0, 0.0] }
    }

    fn view() -> ClassicView {
        ClassicView {
            parts: vec![part("Head", 41.4, Some(25), false, 500.0), part("Torso", 95.0, Some(26), true, 500.0), part("Left Arm", 60.6, Some(27), false, 300.0), part("Right Arm", 60.0, Some(28), false, 700.0)],
            queue: vec!["Torso".into(), "Head".into()],
            special: None,
            bare: false,
        }
    }

    fn shown(view: &ClassicView) -> Shown {
        // Further right is further from the camera here.
        Shown::of(view, |p| Some(((f64::from(p[0]), 400.0), f64::from(p[0]))))
    }

    #[test]
    fn the_parts_get_their_sides_chances_and_places() {
        let s = shown(&view());
        assert_eq!(s.parts.iter().map(|p| p.side).collect::<Vec<_>>(), [Side::Up, Side::Down, Side::Left, Side::Right]);
        assert_eq!(s.parts.iter().map(|p| p.chance).collect::<Vec<_>>(), [41, 95, 61, 60]);
        let mut ui = menu_changed(str::to_string);
        write(&mut ui, &s);
        ui.update();
        let tiles = part_tiles(&ui);
        assert_eq!(tiles.len(), 4);
        let num = |t: TileId, n: &str| ui.tile(t).get(n).map_or(-1.0, Value::num);
        // The file's own type from the side written: head 4, chest 3, left 2, right 1.
        assert_eq!(tiles.iter().map(|&t| num(t, "_type")).collect::<Vec<_>>(), [4.0, 3.0, 1.0, 2.0], "the head's, the chest's, then a left box with the tip on its right and a right box with it on its left");
        // The left arm's box stands out to the left of the leftmost part (300), at its height.
        assert_eq!((num(tiles[2], "x"), num(tiles[2], "y")), (300.0 - OUT_SIDE, 400.0));
        assert_eq!((num(tiles[1], "alpha"), num(tiles[0], "alpha")), (DIM_ALPHA, 255.0));
        assert_eq!(num(ui.menu(VATS_MENU).unwrap(), "alpha"), 255.0, "the menu is shown");
        let text = |i: usize, name: &str| ui.tile(tiles[i]).children.iter().copied().find(|&c| ui.tile(c).name == name).unwrap();
        assert_eq!(ui.tile(text(1, "chance_to_hit")).get("string").map(Value::text).as_deref(), Some("95%"));
        assert_eq!(ui.tile(text(1, "limb_name")).get("string").map(Value::text).as_deref(), Some("Torso"));
        assert!(ui.tile(text(1, "limb_name")).get("visible").is_some_and(Value::truthy), "the selected part is named");
        assert!(!ui.tile(text(0, "limb_name")).get("visible").is_some_and(Value::truthy));
        // The condition meter: 70 wide times the share.
        let meter = text(0, "meter");
        assert_eq!(num(ui.tile(meter).children[0], "width"), 35.0);
    }

    #[test]
    fn copies_follow_the_parts_and_the_queue() {
        let mut ui = menu_changed(str::to_string);
        let mut v = view();
        write(&mut ui, &shown(&v));
        let first = part_tiles(&ui);
        assert_eq!((first.len(), row_tiles(&ui).len()), (4, 2));
        // The same parts again: the copies are kept, not made again.
        write(&mut ui, &shown(&v));
        assert_eq!(part_tiles(&ui), first);
        v.parts.truncate(1);
        v.queue.clear();
        write(&mut ui, &shown(&v));
        assert_eq!(part_tiles(&ui), first[..1]);
        assert!(row_tiles(&ui).is_empty());
        // A part off the view is left out.
        let none = Shown::of(&view(), |_| None);
        write(&mut ui, &none);
        assert!(part_tiles(&ui).is_empty());
    }

    #[test]
    fn the_queue_stacks_up_from_the_lists_bottom() {
        let mut ui = menu_changed(str::to_string);
        write(&mut ui, &shown(&view()));
        ui.update();
        // The engine measures the rows' texts; here each is 30 high.
        for row in row_tiles(&ui) {
            let text = ui.tile(row).children[0];
            ui.set_trait(text, "height", Value::Num(30.0));
        }
        write_heights(&mut ui);
        ui.update();
        let list = ui.find_tile(QUEUE).unwrap();
        let num = |t: TileId, n: &str| ui.tile(t).get(n).map_or(-1.0, Value::num);
        assert_eq!(num(list, "height"), 180.0, "six rows high");
        let rows = row_tiles(&ui);
        // The file's own place: the last queued on the list's bottom row, the first above it.
        assert_eq!((num(rows[0], "y"), num(rows[1], "y")), (120.0, 150.0));
        let accept = ui.find_tile(ACCEPT).unwrap();
        assert!(ui.tile(accept).get("target").is_some_and(Value::truthy), "Accept can be pressed with attacks queued");
    }

    #[test]
    fn the_buttons_follow_the_queue_and_the_weapon() {
        let mut ui = menu_changed(str::to_string);
        let mut v = view();
        v.queue.clear();
        v.special = Some("special attack: damage x0.7, 16 AP".into());
        let s = shown(&v);
        write(&mut ui, &s);
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        assert!(!get(ACCEPT, "target").truthy());
        assert!(get(SPECIAL, "visible").truthy());
        assert_eq!(get(SPECIAL, "string").text(), "special attack: damage x0.7, 16 AP");
        assert!(!get(SPECIAL_2, "visible").truthy());
        assert_eq!(describe(&s), "tiles: VATSMenu drawn from tiles: 4 parts, selected \"Torso\" 95, 0 queued [], special \"special attack: damage x0.7, 16 AP\"");
        v.special = None;
        write(&mut ui, &shown(&v));
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        assert!(!get(SPECIAL, "visible").truthy());
    }

    #[test]
    fn cut_down_files_keep_the_engine_drawn_screen() {
        for (name, reason) in [("queued_actions", format!("{QUEUE} missing")), ("accept_button", format!("{ACCEPT} missing")), ("body_part_percent", "template body_part_percent missing".to_string()), ("queued_action\"", "template queued_action missing".to_string())] {
            let mut ui = menu_changed(|xml| xml.replace(&format!("name=\"{}", name), "name=\"Modded"));
            assert_eq!(built(&ui), Tiled::No(reason), "{name}");
            write(&mut ui, &shown(&view()));
            assert!(part_tiles(&ui).is_empty(), "nothing written");
        }
        assert_eq!(built(&menu_changed(str::to_string)), Tiled::Yes);
    }

    #[test]
    fn the_pointer_picks_a_part_by_its_box_or_its_point_and_an_arrow() {
        let mut v = view();
        for (i, p) in v.parts.iter_mut().enumerate() {
            p.index = i + 10;
        }
        let s = shown(&v);
        let boxes = vec![None, Some(crate::tiles::layout::Rect { left: 900.0, top: 100.0, width: 70.0, height: 40.0 }), None, None];
        // In the torso's box, though far from its point.
        assert_eq!(part_under(&s, &boxes, (930.0, 120.0)).0, Some(11));
        assert_eq!(part_under(&s, &boxes, (974.0, 120.0)).0, Some(11), "the pad round the box");
        // (The parts' own points are what the pointer is near, not their boxes' places.)
        // Beside the boxes, a point within reach: the left arm's at 300, 400.
        assert_eq!(part_under(&s, &boxes, (320.0, 420.0)), (None, Some(12)));
        // Two boxes over each other: the part nearer the camera (the head and the torso
        // stand at the same depth here, the first of them is kept; the left arm is nearer).
        let over = Some(crate::tiles::layout::Rect { left: 900.0, top: 100.0, width: 70.0, height: 40.0 });
        assert_eq!(part_under(&s, &[None, over, over, over], (930.0, 120.0)).0, Some(12), "the left arm, 300 away, before the torso and the right arm");
        // Two points within reach: the nearer part, not the nearer point.
        let mut close = s.clone();
        close.parts[3].point = (310.0, 400.0);
        assert_eq!(part_under(&close, &boxes, (312.0, 400.0)).1, Some(12));
        assert_eq!(part_under(&s, &boxes, (100.0, 100.0)), (None, None));
        // The arrows and the Select button.
        let mut ui = menu_changed(str::to_string);
        write(&mut ui, &s);
        ui.update();
        let menu = ui.menu(VATS_MENU).unwrap();
        let hit = hit_boxes(&ui, menu);
        assert_eq!(hit.arrows.iter().map(|a| a.0).collect::<Vec<_>>(), [-1, 1]);
        // The right arrow's box is at the view's right and holds its own middle; the left
        // one's does not (the arrows stand halfway down the view now, not a quarter).
        let right = &hit.arrows[1].1;
        let middle = (right.left + right.width / 2.0, right.top + right.height / 2.0);
        assert!(middle.0 > hit.arrows[0].1.left + hit.arrows[0].1.width, "{middle:?}");
        assert!(inside(&hit.arrows[1].1, middle, 0.0) && !inside(&hit.arrows[0].1, middle, 0.0));
        assert_eq!(hit.parts.len(), 4);
        // A click: on an arrow the target, on a button what it says, elsewhere an attack.
        assert_eq!(click(None, None, true), Click::Queue);
        assert_eq!(click(Some(-1), Some(ScreenButton::Accept), true), Click::Target(-1));
        assert_eq!(click(None, Some(ScreenButton::Accept), true), Click::Accept);
        assert_eq!(click(None, Some(ScreenButton::Accept), false), Click::Nothing, "no click target with nothing queued");
        assert_eq!(click(None, Some(ScreenButton::Return), false), Click::Return);
        assert_eq!(click(None, Some(ScreenButton::Special), true), Click::Special);
        assert_eq!(click(None, Some(ScreenButton::Select), false), Click::Queue);
        write_pointer(&mut ui, true, Some(1));
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        assert!(get(SELECT, "visible").truthy());
        assert_eq!(get(SELECT, "_PCButtonText").text(), SELECT_HINT);
        assert!(SELECT_HINT.len() <= 8 && RETURN_HINT.len() <= 8, "a hint has its button's width as room");
        assert_eq!((get(LEFT_ARROW, "mouseover").num(), get(RIGHT_ARROW, "mouseover").num()), (0.0, 1.0));
        write_pointer(&mut ui, false, None);
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        assert!(!get(SELECT, "visible").truthy());
    }

    #[test]
    fn dead_eyes_screen_is_the_parts_boxes_alone() {
        let mut ui = menu_changed(str::to_string);
        let v = ClassicView { bare: true, queue: Vec::new(), ..view() };
        let s = shown(&v);
        write(&mut ui, &s);
        ui.update();
        assert_eq!(part_tiles(&ui).len(), 4);
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        for path in [ACCEPT, RETURN, LEFT_ARROW, RIGHT_ARROW, SPECIAL, SPECIAL_2] {
            assert!(!get(path, "visible").truthy(), "{path}");
        }
        // The part under the crosshair is lit on the body, its box drawn as the classic
        // screen draws the selected part's.
        let tiles = part_tiles(&ui);
        let num = |t: TileId, n: &str| ui.tile(t).get(n).map_or(-1.0, Value::num);
        assert_eq!((num(tiles[1], "alpha"), num(tiles[0], "alpha")), (DIM_ALPHA, 255.0));
        assert_eq!(describe(&s), "tiles: VATSMenu drawn from tiles for Dead Eye: 4 parts, under the crosshair \"Torso\" 95");
        // The classic screen shows them again.
        write(&mut ui, &shown(&view()));
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        assert!(get(ACCEPT, "visible").truthy() && get(LEFT_ARROW, "visible").truthy());
    }

    #[test]
    fn the_arrows_stand_halfway_down_the_view_at_any_scale() {
        let mut ui = menu_changed(str::to_string);
        let screen = ui.screen();
        // 134 high: its middle on the view's, for a view 960 units high and one 1920 (50 %).
        for (high, y) in [(960.0, 413.0), (1920.0, 893.0), (640.0, 253.0)] {
            ui.set_trait(screen, "height", Value::Num(high));
            write(&mut ui, &shown(&view()));
            for path in [LEFT_ARROW, RIGHT_ARROW] {
                assert_eq!(ui.tile(ui.find_tile(path).unwrap()).get("y").map(Value::num), Some(y), "{path} in a view {high} high");
            }
        }
    }

    #[test]
    fn the_buttons_rows_end_above_the_arrows() {
        // The arrows' top at 413 in a view 960 high: three rows, the last a row and the
        // gap above it.
        assert_eq!(button_rows(3, 413.0), [264.0, 311.0, 358.0]);
        assert_eq!(button_rows(6, 413.0).first(), Some(&123.0), "with a controller's six");
        // At 50 % the view is 1920 high and the arrows' top at 893: still hung from them.
        assert_eq!(button_rows(3, 893.0).last(), Some(&838.0));
        // No room above the arrows: from the view's top down, never off it.
        assert_eq!(button_rows(3, 50.0), [10.0, 57.0, 104.0]);
        // The file's buttons take them.
        let mut ui = menu_changed(str::to_string);
        ui.set_trait(ui.screen(), "height", Value::Num(960.0));
        write(&mut ui, &shown(&view()));
        let top = |ui: &Ui, path: &str| ui.tile(ui.find_tile(path).unwrap()).get("_y").map(Value::num);
        assert_eq!(top(&ui, RETURN).zip(top(&ui, ACCEPT)).map(|(r, a)| r - a), Some(BUTTON_ROW));
        assert!(top(&ui, RETURN).unwrap() + BUTTON_ROW <= 413.0, "the last row ends above the arrows");
    }

    #[test]
    fn the_boxes_stand_beside_the_target_and_never_over_each_other() {
        let part = |name: &str, side: Side, point: (f64, f64)| PartShown { index: 0, name: name.into(), chance: 50, condition: 1.0, selected: false, side, at: point, depth: 0.0, point };
        // A person: head, torso, two arms, two legs.
        let mut person = vec![
            part("Head", Side::Up, (500.0, 200.0)),
            part("Torso", Side::Down, (500.0, 330.0)),
            part("Left Arm", Side::Left, (430.0, 320.0)),
            part("Right Arm", Side::Right, (570.0, 320.0)),
            part("Left Leg", Side::Left, (470.0, 560.0)),
            part("Right Leg", Side::Right, (530.0, 560.0)),
        ];
        spread(&mut person);
        assert_eq!(person[0].at, (500.0, 200.0 - OUT_UP), "the head's box above the head");
        assert_eq!(person[1].at, (500.0, 330.0), "the torso's on the chest");
        assert_eq!(person[2].at, (430.0 - OUT_SIDE, 320.0), "the left arm's out to the left, at its height");
        assert_eq!(person[5].at, (570.0 + OUT_SIDE, 560.0), "the right leg's out to the right, at its height");
        // A dog: two legs a side, front and back, at one height: they are moved apart.
        let mut dog = vec![part("Right Legs", Side::Right, (600.0, 500.0)), part("Right Legs", Side::Right, (640.0, 505.0)), part("Torso", Side::Down, (500.0, 400.0))];
        spread(&mut dog);
        for i in 0..dog.len() {
            for j in i + 1..dog.len() {
                let (a, b) = (dog[i].at, dog[j].at);
                assert!((a.0 - b.0).abs() >= BOX_ROOM.0 || (a.1 - b.1).abs() >= BOX_ROOM.1, "{} over {}: {a:?} {b:?}", dog[i].name, dog[j].name);
            }
        }
        assert!(dog[0].at.1 < dog[1].at.1, "the upper one goes up, the lower one down");
    }

    #[test]
    fn the_left_stick_steps_to_the_part_that_lies_that_way() {
        // Head above, torso in the middle (selected), a leg below left, an arm to the right.
        let places = [(0, (500.0, 200.0), false), (1, (500.0, 400.0), true), (2, (420.0, 620.0), false), (3, (700.0, 410.0), false)];
        assert_eq!(step_toward(&places, (0.0, -1.0)), Some(0), "up: the head");
        assert_eq!(step_toward(&places, (0.0, 1.0)), Some(2), "down: the leg");
        assert_eq!(step_toward(&places, (1.0, 0.0)), Some(3), "right: the arm");
        assert_eq!(step_toward(&places, (-1.0, 0.0)), None, "nothing lies to the left within 60 degrees");
        assert_eq!(step_toward(&places[..1], (0.0, 1.0)), None, "no part selected");
    }

    #[test]
    fn a_controller_gets_its_buttons_names_and_the_stick_buttons() {
        let mut ui = menu_changed(str::to_string);
        let s = Shown { pad: true, ..shown(&view()) };
        write(&mut ui, &s);
        let get = |path: &str, n: &str| ui.tile(ui.find_tile(path).unwrap()).get(n).cloned().unwrap();
        for path in [ACCEPT, RETURN, SELECT, BODY_PART, TARGET] {
            assert!(get(path, "visible").truthy(), "{path}");
            assert!(get(path, "_PCButtonText").text().starts_with('['), "{path}");
        }
        assert_eq!(get(BODY_PART, "_PCButtonText").text(), "[L Stick]");
        assert!(!get(SPECIAL, "visible").truthy(), "no special attack with this weapon");
    }

    #[test]
    fn window_points_become_interface_units() {
        // At the original's size a unit is the window's height over 960.
        assert_eq!(to_units((960.0, 540.0), (1920.0, 1080.0), 1.0), (853.3333129882813, 480.0));
        let (x, y) = to_units((640.0, 360.0), (1280.0, 720.0), 1.0);
        assert!((x - 853.333).abs() < 0.01 && (y - 480.0).abs() < 0.01);
        // At half the UI scale the view is twice as many units each way, from the same
        // corner: the window's middle is the middle of 3413 x 1920 units.
        let (x, y) = to_units((640.0, 360.0), (1280.0, 720.0), 0.5);
        assert!((x - 1706.667).abs() < 0.01 && (y - 960.0).abs() < 0.01);
        assert_eq!(to_units((0.0, 0.0), (1280.0, 720.0), 0.5), (0.0, 0.0));
        // And at 150 % two thirds as many.
        let (x, y) = to_units((1280.0, 720.0), (1280.0, 720.0), 1.5);
        assert!((x - 1137.778).abs() < 0.01 && (y - 640.0).abs() < 0.01);
    }

    #[test]
    fn the_crops_are_held_and_given_back() {
        let mut ui = menu_changed(str::to_string);
        let screen = ui.screen();
        let mut kept = None;
        hold_crop(&mut ui, &mut kept);
        assert_eq!(ui.tile(screen).get("cropx").map(Value::num), Some(CROP));
        release_crop(&mut ui, &mut kept);
        assert!(ui.tile(screen).get("cropx").is_none(), "absent again");
        // A script's own value since stands.
        hold_crop(&mut ui, &mut kept);
        ui.set_trait(screen, "cropy", Value::Num(7.0));
        release_crop(&mut ui, &mut kept);
        assert_eq!(ui.tile(screen).get("cropy").map(Value::num), Some(7.0));
        assert!(ui.tile(screen).get("cropx").is_none());
    }

    #[test]
    fn the_switch_is_on_unless_turned_off() {
        assert!(switched_off("0") && switched_off(" Off ") && switched_off("false") && switched_off("no"));
        assert!(!switched_off("1") && !switched_off("") && !switched_off("yes"));
    }
}
