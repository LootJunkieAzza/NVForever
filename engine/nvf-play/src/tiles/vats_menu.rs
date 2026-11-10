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
//! - **Input** stays the mode's keys (`crate::vats_mode::read_keys`); the file's arrows
//!   and buttons are drawn and not clicked yet (the pointer is the view's in play).

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

/// A part that is not the selected one: its copy's `alpha` [design: the file leaves the
/// copies' `alpha` to code; the selected part is drawn whole, the others at about half:
/// the copy's pieces copy its `alpha` and stand under it, so 180 draws them at 127].
const DIM_ALPHA: f64 = 180.0;

/// Whether the switch's text turns the file off: `0`, `false`, `off` or `no`.
pub fn switched_off(text: &str) -> bool {
    matches!(text.trim().to_ascii_lowercase().as_str(), "0" | "false" | "off" | "no")
}

/// [design] Every fixed path the model writes and both templates are part of a successful
/// build; a file a mod restructured keeps the engine-drawn screen.
pub fn built(ui: &Ui) -> Tiled {
    for path in [VATS_MENU, BRANCH, QUEUE, ACCEPT, RETURN, SPECIAL, SPECIAL_2] {
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
    fn name(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Down => "down",
            Side::Up => "up",
        }
    }
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
    pub name: String,
    /// The chance in whole percent.
    pub chance: u32,
    /// The condition meter's share, 0 to 1 (0 for a part without a condition).
    pub condition: f64,
    pub selected: bool,
    pub side: Side,
    /// Its place in interface units.
    pub at: (f64, f64),
}

/// What the log tells of a [`Shown`]: how many parts, the selected one's name, the queue
/// and the special attack.
type Told = (usize, Option<String>, Vec<String>, Option<String>);

/// What the screen shows.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Shown {
    pub parts: Vec<PartShown>,
    pub queue: Vec<String>,
    pub special: Option<String>,
}

impl Shown {
    /// `view` with each part's point placed by `place` (interface units; `None` for a
    /// point not on the view, whose part is left out).
    pub fn of(view: &ClassicView, place: impl Fn([f32; 3]) -> Option<(f64, f64)>) -> Shown {
        let placed: Vec<_> = view.parts.iter().filter_map(|p| place(p.point).map(|at| (p, at))).collect();
        let middle = if placed.is_empty() { 0.0 } else { placed.iter().map(|(_, at)| at.0).sum::<f64>() / placed.len() as f64 };
        let parts = placed
            .into_iter()
            .map(|(p, at)| PartShown {
                name: p.name.clone(),
                chance: p.chance.clamp(0.0, 100.0).round() as u32,
                condition: f64::from(p.condition.unwrap_or(0.0).clamp(0.0, 1.0)),
                selected: p.selected,
                side: side_of(p.value, at.0, middle),
                at,
            })
            .collect();
        Shown { parts, queue: view.queue.clone(), special: view.special.clone() }
    }

    /// What the log tells of it (not the places or the chances, which move every frame of
    /// a slowed clock).
    fn told(&self) -> Told {
        (self.parts.len(), self.parts.iter().find(|p| p.selected).map(|p| p.name.clone()), self.queue.clone(), self.special.clone())
    }
}

/// A window point (logical pixels) in the menu's interface units: the menu is drawn
/// `unit` logical pixels to a unit in a frame `menu_scale` times the window, kept at the
/// window's middle (`super::draw::menu_anchor`, `crate::interface::menu_frame`).
pub fn to_units(point: (f32, f32), window: (f32, f32), menu_scale: f32) -> (f64, f64) {
    let unit = window.1 / 960.0 * menu_scale;
    if unit <= 0.0 {
        return (0.0, 0.0);
    }
    let left = 0.5 * window.0 * (1.0 - menu_scale);
    let top = 0.5 * window.1 * (1.0 - menu_scale);
    (f64::from((point.0 - left) / unit), f64::from((point.1 - top) / unit))
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
/// - one `limb_percent` per part: `x` and `y` its place, `justify` its side ([`side_of`]),
///   `string` its chance in whole percent [design: the bare number], `_LimbName` its
///   name and `_LimbNameVisible` on for the selected part [design], `_MeterPercent`
///   its condition (the file's meter is 70 wide times it) and `alpha` 255 for the selected
///   part, [`DIM_ALPHA`] for the others;
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
        ui.set_trait(t, "justify", sym(p.side.name()));
        ui.set_trait(t, "string", Value::Str(p.chance.to_string()));
        ui.set_trait(t, "_LimbName", Value::Str(p.name.clone()));
        ui.set_trait(t, "_LimbNameVisible", flag(p.selected && !p.name.is_empty()));
        ui.set_trait(t, "_MeterPercent", Value::Num(p.condition));
        ui.set_trait(t, "alpha", Value::Num(if p.selected { 255.0 } else { DIM_ALPHA }));
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
    if let Some(t) = ui.find_tile(SPECIAL) {
        ui.set_trait(t, "string", Value::Str(shown.special.clone().unwrap_or_default()));
        ui.set_trait(t, "visible", flag(shown.special.is_some()));
    }
    if let Some(t) = ui.find_tile(SPECIAL_2) {
        ui.set_trait(t, "visible", flag(false));
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

/// The screen's state between frames.
#[derive(Resource, Default)]
pub struct VatsTiles {
    /// The switch (the module's notes).
    pub on: bool,
    /// This frame the target screen is the file's: `super::draw` draws `VATSMenu` and
    /// `crate::vats_mode` leaves its own labels and bars out.
    pub drawn: bool,
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
    let up = st.on && st.failed.is_none() && game.ui_files.is_some() && vats.as_ref().is_some_and(|v| v.selected().is_some());
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
    let view = vats.as_ref().and_then(|v| v.classic_view()).unwrap_or_default();
    let window = files.window_size().unwrap_or((1280.0, 720.0));
    let scale = crate::interface::menu_scale();
    let camera = cameras.single().ok();
    let shown = Shown::of(&view, |p| {
        let (cam, t) = camera?;
        let at = cam.world_to_viewport(t, crate::space::to_bevy(p)).ok()?;
        Some(to_units((at.x, at.y), window, scale))
    });
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
        ClassicPart { name: name.into(), chance, condition: Some(0.5), value, selected, point: [x, 0.0, 0.0] }
    }

    fn view() -> ClassicView {
        ClassicView {
            parts: vec![part("Head", 41.4, Some(25), false, 500.0), part("Torso", 95.0, Some(26), true, 500.0), part("Left Arm", 60.6, Some(27), false, 300.0), part("Right Arm", 60.0, Some(28), false, 700.0)],
            queue: vec!["Torso".into(), "Head".into()],
            special: None,
        }
    }

    fn shown(view: &ClassicView) -> Shown {
        Shown::of(view, |p| Some((f64::from(p[0]), 400.0)))
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
        assert_eq!(tiles.iter().map(|&t| num(t, "_type")).collect::<Vec<_>>(), [4.0, 3.0, 2.0, 1.0]);
        assert_eq!((num(tiles[2], "x"), num(tiles[2], "y")), (300.0, 400.0));
        assert_eq!((num(tiles[1], "alpha"), num(tiles[0], "alpha")), (255.0, DIM_ALPHA));
        assert_eq!(num(ui.menu(VATS_MENU).unwrap(), "alpha"), 255.0, "the menu is shown");
        let text = |i: usize, name: &str| ui.tile(tiles[i]).children.iter().copied().find(|&c| ui.tile(c).name == name).unwrap();
        assert_eq!(ui.tile(text(1, "chance_to_hit")).get("string").map(Value::text).as_deref(), Some("95"));
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
    fn window_points_become_interface_units() {
        // At the original's size a unit is the window's height over 960.
        assert_eq!(to_units((960.0, 540.0), (1920.0, 1080.0), 1.0), (853.3333129882813, 480.0));
        let (x, y) = to_units((640.0, 360.0), (1280.0, 720.0), 1.0);
        assert!((x - 853.333).abs() < 0.01 && (y - 480.0).abs() < 0.01);
        // Scaled down, the menu's frame keeps the middle: the middle point stays the middle.
        let (x, y) = to_units((640.0, 360.0), (1280.0, 720.0), 0.5);
        assert!((x - 853.333).abs() < 0.01 && (y - 480.0).abs() < 0.01);
        // And the frame's top left corner is its origin.
        assert_eq!(to_units((320.0, 180.0), (1280.0, 720.0), 0.5), (0.0, 0.0));
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
