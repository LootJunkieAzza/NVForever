//! The container and barter menus drawn as the original draws them
//! (docs/spec-notes/barter-menu.md): `menus\container_menu.xml` and
//! `menus\barter_menu.xml` set out, with the game's fonts and pictures read from the
//! owner's archives at run time. Lengths are in the interface's units, 960 to the height
//! of the view, measured from the top left corner of the menus' 1024 × 800 rectangle,
//! which is centred on the screen.
//!
//! - [`layout`] places what moves with the menu's content (the titles' arrows, the caps
//!   arrow, the rows); it is pure and tested without the game's files.
//! - Without the game's files the same layout is drawn with Bevy's font and plain lines.

use super::barter::{self, Flow};
use super::model::{Category, Command, MenuModel, Mode, Row, Side};
use super::{CmdButton, RowButton};
use crate::interface::{amber, at, face, plain_font, set_line, Face, Picture, Pictures};
use bevy::prelude::*;
use fnv_font::{Font, Line};
use fnv_sim::forms::ItemKind;
use nvf_assets::GameData;

// ---- the interface files' numbers [documented: §2] -------------------------------------

/// The menus' rectangle, centred on the screen.
pub const MAIN: Vec2 = Vec2::new(1024.0, 800.0);
/// The two halves: the player's at the left, the container's or merchant's 560 further.
const LEFT_X: f32 = 20.0;
const RIGHT_X: f32 = 580.0;
const TOP: f32 = 20.0;
const HALF_W: f32 = 464.0;
/// The lists: 15 left of their half, 60 down, 365 high; the player's is 462 wide, the
/// other 464.
const LIST_DX: f32 = -15.0;
const LIST_Y: f32 = 60.0;
const LIST_H: f32 = 365.0;
/// The scroll bar at a list's left, 32 wide, with 16-high arrows at its ends; its marker
/// 6 wide, 12 in.
const BAR_W: f32 = 32.0;
const BAR_ARROW: f32 = 16.0;
/// A list scrolls by half a row [documented: `_scroll_delta` 26].
const SCROLL_STEP: f32 = 26.0;
/// In a row: the mark at 15, 15 (25 square), the name at 40, 20 down, the price's right
/// end at 403.
const MARK_AT: f32 = 15.0;
const MARK: f32 = 25.0;
const NAME_X: f32 = 40.0;
const TEXT_Y: f32 = 20.0;
const PRICE_RIGHT: f32 = 403.0;
/// Names wrap at 300 in the barter lists and 370 in the container's.
const WRAP_BARTER: f32 = 300.0;
const WRAP_CONTAINER: f32 = 370.0;
/// The heading: a line at the top with the title 50 in (the barter file 10 above the
/// line, the container file 12.5), the arrows 17.5 × 35 at 30 and after the title, the
/// caps or weight figure right-aligned at 449, 20 down.
const TITLE_X: f32 = 50.0;
const ARROW_X: f32 = 30.0;
const ARROW: Vec2 = Vec2::new(17.5, 35.0);
const FIGURE_RIGHT: f32 = 449.0;
const CORNER: f32 = 40.0;
const FOOTER_Y: f32 = 480.0;
/// The bottom bracket, 820 down the screen.
const BRACKET_Y: f32 = 740.0;
/// Accept / Take All and Exit: right-aligned at 1034 less the box's 10, 670 down the
/// screen; each box is its text and 15 high.
const BUTTONS_RIGHT: f32 = 1024.0;
const BUTTONS_Y: f32 = 590.0;
const BUTTON_PAD: f32 = 15.0;
/// The item's picture (256 square) and the item's figures (530 × 120 in tiles a third
/// as wide and high).
const ICON_AT: Vec2 = Vec2::new(42.0, 492.0);
const ICON: f32 = 256.0;
const DATA_Y: f32 = 555.0;
const DATA_X_BARTER: f32 = 287.0;
const DATA_X_CONTAINER: f32 = 327.0;
const TILE: Vec2 = Vec2::new(530.0 / 3.0, 40.0);
/// The caps arrow: its rectangle at 449 (20 more when the player is paid), 180 down;
/// the picture 120 × 360 at 10 (less 25), -15; the figure centred at 32.5 (plus 7.5),
/// 50 down; "Caps" 2.5 right of it, 93.5 down.
const FLOW_X: f32 = LEFT_X + 449.0;
const FLOW_Y: f32 = 180.0;
/// The container's item arrow: 64 × 128 at 474 from the player's half, 170 down (6 more
/// pointing left).
const ITEM_ARROW_X: f32 = LEFT_X + 474.0;
const ITEM_ARROW_Y: f32 = 170.0;
/// The note under the container menu, centred, 800 down the screen and half its box.
const NOTE_Y: f32 = 720.0;
/// Every line is 2 thick [documented: menus\globals.xml, on a PC].
const THICK: f32 = 2.0;
/// The highlight box: a line at brightness 200 and a fill of 40 at half brightness.
const HIGHLIGHT_LINE: f32 = 200.0;
const HIGHLIGHT_FILL: f32 = 40.0;
/// A button the player cannot use is drawn at 128 of 255.
const DIM: f32 = 128.0 / 255.0;

// ---- measured on the owner's screenshots [observed, §3] ----------------------------------

/// A row is the body font's 31 and 20 [observed: rows 51 apart at 1024 lines; the
/// message box's button rows, message-menu.md §3].
pub const ROW_H: f32 = 51.0;
/// How far under a text's top its letters stand: the body font and the title font
/// (message-menu.md §3).
const BODY_BASE: f32 = 15.0;
const TITLE_BASE: f32 = 21.5;
/// Lines of a text are 35 apart (message-menu.md §3).
const LINE_PITCH: f32 = 35.0;
/// The key hint stands this far right of its button's text ("Accept A)").
const HINT_GAP: f32 = 16.0;
/// How dark the picture behind the menu makes the world: about a third at its edge on
/// the barter screenshot (the world behind is also blurred there; not built).
const SHADE: f32 = 0.33;

const BODY_FONT: &str = "textures\\fonts\\monofonto_large.fnt";
const TITLE_FONT: &str = "textures\\fonts\\glow_monofonto_vl_dialogs.fnt";

/// What the menus are drawn with, read when the first one opens.
pub struct Kit {
    body: Face,
    title: Face,
    pictures: Pictures,
}

impl Kit {
    pub fn load(data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) -> Kit {
        Kit {
            body: face(BODY_FONT, plain_font(31.0, 12.0), data, images, bc),
            title: face(TITLE_FONT, plain_font(34.0, 14.0), data, images, bc),
            pictures: Pictures::load(data),
        }
    }
}

/// Where a text's letters start for a given anchor.
#[derive(Clone, Copy)]
enum Align {
    Left,
    Right,
    Centre,
}

fn first_line(font: &Font, s: &str) -> Line {
    font.lines(s, None).into_iter().next().unwrap_or_default()
}

/// The title of a half: the player's is the category's; the other half's is the
/// container's or merchant's name while it shows everything [hypothesis: its title for
/// other categories is not observed].
pub fn half_title(m: &MenuModel, side: Side) -> String {
    let c = m.category[side as usize];
    match side {
        Side::Player => c.title().into(),
        Side::Container if c == Category::All => m.title.clone(),
        Side::Container => c.title().into(),
    }
}

/// A row's text: the name, and the count when there is more than one [observed].
pub fn row_text(r: &Row) -> String {
    if r.count > 1 {
        format!("{} ({})", r.name, r.count)
    } else {
        r.name.clone()
    }
}

/// The figure at the top right of a half: the caps on either side of a barter, the
/// player's weight in a container's [observed: "Caps 35877", "Wg 35/165"].
pub fn half_figure(m: &MenuModel, side: Side) -> Option<String> {
    match (m.mode, side, &m.trade) {
        (Mode::Barter(_), _, Some(t)) => Some(format!("Caps  {}", t.purse[side as usize])),
        (Mode::Container(_), Side::Player, _) => {
            m.encumbrance.as_ref().ok().map(|e| format!("Wg  {:.0}/{:.0}", e.weight, e.capacity))
        }
        _ => None,
    }
}

/// Where the parts of a heading go, from its half's left end: the left arrow, the title,
/// the right arrow, and the top line after it (start, length).
pub fn heading(title_width: f32) -> (f32, f32, f32, (f32, f32)) {
    let right_arrow = TITLE_X + 10.0 + title_width;
    let line = right_arrow + 20.0;
    (ARROW_X, TITLE_X, right_arrow, (line, (HALF_W - line).max(0.0)))
}

/// Where the caps figure is centred, from the menu's left: the file's "somewhat centred"
/// rule, which moves a figure wider than 70 by its excess (left when the player pays,
/// right when paid).
pub fn flow_figure_x(flow: Flow, figure_width: f32) -> f32 {
    let sell = flow == Flow::Sell;
    let rect = FLOW_X + if sell { 20.0 } else { 0.0 };
    let excess = if figure_width > 70.0 { figure_width - 70.0 } else { 0.0 };
    rect + 32.5 + if sell { 7.5 } else { 0.0 } - excess * if sell { -1.0 } else { 1.0 }
}

/// A row's height for a name of `lines` lines: the text and 20 [documented: the template's
/// `_VerticalSpacing`], a text of n lines being 31 + 35 (n − 1) high (message-menu.md §3).
pub fn row_height(lines: usize) -> f32 {
    ROW_H + LINE_PITCH * lines.saturating_sub(1) as f32
}

/// The first row to draw: the list's own first row, or a later one when rows of two
/// lines above the selection would push it out of the list.
pub fn first_shown(heights: &[f32], first: usize, selected: usize) -> usize {
    let mut first = first.min(heights.len());
    while first < selected && heights[first..=selected.min(heights.len().saturating_sub(1))].iter().sum::<f32>() > LIST_H {
        first += 1;
    }
    first
}

/// The scroll bar's marker: its top and height inside the bar's track, or `None` when the
/// list fits. Counted in steps of half a row [hypothesis: the steps not shown are the
/// marker's positions].
pub fn marker(rows: usize, first: usize) -> Option<(f32, f32)> {
    let track = LIST_H - 2.0 * BAR_ARROW;
    let shown = (LIST_H / SCROLL_STEP).floor();
    let total = (rows as f32 * ROW_H / SCROLL_STEP).ceil();
    if total <= shown {
        return None;
    }
    let positions = total - shown + 1.0;
    let h = (track / positions).max(32.0).min(track);
    let at = (first as f32 * ROW_H / SCROLL_STEP).min(positions - 1.0);
    Some((at / (positions - 1.0) * (track - h), h))
}

// ---- drawing -------------------------------------------------------------------------------

struct Ctx<'a> {
    kit: &'a mut Kit,
    data: Option<&'a GameData>,
    images: &'a mut Assets<Image>,
    bc: bool,
}

impl Ctx<'_> {
    fn picture(&mut self, path: &str) -> Option<Picture> {
        self.kit.pictures.get(path, self.data, self.images, self.bc)
    }

    /// A picture tinted the interface's colour (at `alpha`), stretched over `node`.
    fn image(&mut self, p: &mut ChildSpawnerCommands, path: &str, node: Node, color: Color) -> Option<Entity> {
        self.image_with(p, path, node, color, ())
    }

    /// The same, with more components on the picture (a button).
    fn image_with(&mut self, p: &mut ChildSpawnerCommands, path: &str, node: Node, color: Color, extra: impl Bundle) -> Option<Entity> {
        let (image, rect) = self.picture(path)?;
        Some(p.spawn((node, ImageNode { rect, color, image_mode: NodeImageMode::Stretch, ..ImageNode::new(image) }, extra)).id())
    }

    /// A straight line, from the atlas's solid piece (or a plain fill).
    fn line(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, w: f32, h: f32, color: Color) {
        if self.image(p, "solid.dds", at(x, y, w, h), color).is_none() {
            p.spawn((at(x, y, w, h), BackgroundColor(color)));
        }
    }

    /// A line 40 long that fades out downwards (or upwards, `up`), standing at `y`.
    fn fade(&mut self, p: &mut ChildSpawnerCommands, x: f32, y: f32, up: bool, color: Color) {
        let name = if up { "fade_to_top.dds" } else { "fade_to_bottom.dds" };
        self.image(p, name, at(x, y, THICK, CORNER), color);
    }

    #[allow(clippy::too_many_arguments)]
    fn text(&mut self, p: &mut ChildSpawnerCommands, title: bool, s: &str, x: f32, top: f32, align: Align, color: Color) -> f32 {
        let face = if title { &self.kit.title } else { &self.kit.body };
        let line = first_line(&face.font, s);
        let x = match align {
            Align::Left => x,
            Align::Right => x - line.width,
            Align::Centre => x - line.width / 2.0,
        };
        set_line(p, face, &line, x, top + if title { TITLE_BASE } else { BODY_BASE }, color);
        line.width
    }
}

/// Spawn the menu under `root`. `note`: the line under the menu.
pub fn spawn(root: &mut ChildSpawnerCommands, m: &MenuModel, note: Option<&str>, kit: &mut Kit, data: Option<&GameData>, images: &mut Assets<Image>, bc: bool) {
    let mut c = Ctx { kit, data, images, bc };
    let bright = amber(255.0);
    let main = Node {
        width: crate::interface::u(MAIN.x),
        height: crate::interface::u(MAIN.y),
        ..default()
    };
    root.spawn(main).with_children(|p| {
        // [documented] The dark picture behind, 40 left of and 105 above the menu.
        let behind = at(-40.0, -105.0, 1144.0, 970.0);
        let shade = Color::srgba(0.0, 0.0, 0.0, SHADE);
        if c.image(p, "interface\\shared\\background\\solid_black.dds", behind.clone(), shade).is_none() {
            p.spawn((behind, BackgroundColor(shade)));
        }
        for side in [Side::Player, Side::Container] {
            half(p, &mut c, m, side);
        }
        // The bottom bracket: a line with a short line fading up at each end.
        c.line(p, LEFT_X, BRACKET_Y, MAIN.x, THICK, bright);
        for x in [LEFT_X, LEFT_X + MAIN.x - THICK] {
            c.fade(p, x, BRACKET_Y - CORNER + 1.0, true, bright);
        }
        match (m.mode, &m.trade) {
            (Mode::Barter(_), Some(t)) => {
                if let Some(flow) = barter::flow(t.balance) {
                    caps_flow(p, &mut c, flow, t.balance.unsigned_abs());
                }
            }
            (Mode::Container(_), _) if m.selected_row(m.focus).is_some() => {
                // [documented: the arrow points the way the item would go]
                let (name, dy) = if m.focus == Side::Player { ("right.dds", 0.0) } else { ("left.dds", 6.0) };
                c.image(p, &format!("interface\\shared\\arrow\\{name}"), at(ITEM_ARROW_X, ITEM_ARROW_Y + dy, 64.0, 128.0), bright);
            }
            _ => {}
        }
        buttons(p, &mut c, m);
        if let Some(r) = m.selected_row(m.focus) {
            item_data(p, &mut c, m, r);
        }
        if let Some(n) = note.filter(|n| !n.is_empty()) {
            c.text(p, false, n, MAIN.x / 2.0, NOTE_Y + (31.0 + 20.0) / 2.0 + 20.0, Align::Centre, bright);
        }
    });
}

fn half(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel, side: Side) {
    let bright = amber(255.0);
    let x0 = if side == Side::Player { LEFT_X } else { RIGHT_X };
    let title_top = TOP + if matches!(m.mode, Mode::Barter(_)) { -10.0 } else { -12.5 };
    let title = half_title(m, side);
    let title_w = first_line(&c.kit.title.font, &title).width;
    let (left_arrow, title_x, right_arrow, (line_x, line_len)) = heading(title_w);
    // The heading.
    c.fade(p, x0, TOP, false, bright);
    c.line(p, x0, TOP, 25.0, THICK, bright);
    for (x, name, by) in [(left_arrow, "main_arrow_left.dds", -1), (right_arrow, "main_arrow_right.dds", 1)] {
        let node = at(x0 + x, TOP - 10.0, ARROW.x, ARROW.y);
        c.image_with(p, &format!("interface\\main\\{name}"), node, bright, (Button, CmdButton(Command::Category(side, by))));
    }
    c.text(p, true, &title, x0 + title_x, title_top, Align::Left, bright);
    c.line(p, x0 + line_x, TOP, line_len, THICK, bright);
    if let Some(f) = half_figure(m, side) {
        c.text(p, false, &f, x0 + FIGURE_RIGHT, TOP + 20.0, Align::Right, bright);
    }
    c.fade(p, x0 + HALF_W - THICK, TOP, false, bright);
    // The footer.
    c.fade(p, x0, TOP + FOOTER_Y - CORNER, true, bright);
    c.line(p, x0, TOP + FOOTER_Y, HALF_W, THICK, bright);
    c.fade(p, x0 + HALF_W - THICK, TOP + FOOTER_Y - CORNER, true, bright);
    list(p, c, m, side, x0);
}

fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel, side: Side, x0: f32) {
    let bright = amber(255.0);
    let width = if side == Side::Player { 462.0 } else { 464.0 };
    let rows = &m.list(side).rows;
    let first = m.scroll[side as usize];
    let barter = matches!(m.mode, Mode::Barter(_));
    let node = Node { overflow: Overflow::clip(), ..at(x0 + LIST_DX, TOP + LIST_Y, width, LIST_H) };
    p.spawn(node).with_children(|l| {
        if let Some((y, h)) = marker(rows.len(), first) {
            c.image(l, "arrow_up.dds", at(0.0, 0.0, BAR_W, BAR_ARROW), bright);
            c.image(l, "arrow_down.dds", at(0.0, LIST_H - BAR_ARROW, BAR_W, BAR_ARROW), bright);
            if c.image(l, "vert_marker.dds", at(12.0, BAR_ARROW + y, 6.0, h), bright).is_none() {
                l.spawn((at(12.0, BAR_ARROW + y, 6.0, h), BackgroundColor(bright)));
            }
        }
        let row_w = width - BAR_W - 5.0;
        let wrap = if barter { WRAP_BARTER } else { WRAP_CONTAINER };
        let texts: Vec<Vec<Line>> = rows.iter().map(|r| c.kit.body.font.lines(&row_text(r), Some(wrap))).collect();
        let heights: Vec<f32> = texts.iter().map(|t| row_height(t.len())).collect();
        let first = first_shown(&heights, first, m.selected[side as usize]);
        let mut y = 0.0;
        for i in first..rows.len() {
            if y >= LIST_H {
                break;
            }
            let r = &rows[i];
            let row_h = heights[i];
            let row_y = y;
            y += row_h;
            l.spawn((Button, RowButton(side, i), at(BAR_W, row_y, row_w, row_h))).with_children(|row| {
                // [documented] The highlight box of the list that has the focus, one
                // unit short of the row.
                if m.focus == side && m.selected[side as usize] == i {
                    let h = row_h - 1.0;
                    let fill = amber(128.0).with_alpha((HIGHLIGHT_FILL / 255.0).powf(2.2));
                    row.spawn((at(0.0, 0.0, row_w, h), BackgroundColor(fill)));
                    let line = amber(HIGHLIGHT_LINE);
                    for yy in [0.0, h - THICK] {
                        c.line(row, 0.0, yy, row_w, THICK, line);
                    }
                    for xx in [0.0, row_w - THICK] {
                        c.line(row, xx, THICK, THICK, h - 2.0 * THICK, line);
                    }
                }
                // [documented] A filled square for an equipped item, an open one for an
                // item put up for the trade.
                let mark = if r.bartered { Some("square.dds") } else if r.equipped { Some("square_filled.dds") } else { None };
                if let Some(mark) = mark {
                    c.image(row, mark, at(MARK_AT, MARK_AT, MARK, MARK), bright);
                }
                for (k, line) in texts[i].iter().enumerate() {
                    set_line(row, &c.kit.body, line, NAME_X, TEXT_Y + BODY_BASE + LINE_PITCH * k as f32, bright);
                }
                if barter {
                    let price = r.price.map_or("--".to_string(), barter::price_text);
                    c.text(row, false, &price, PRICE_RIGHT, TEXT_Y, Align::Right, bright);
                }
            });
        }
    });
}

fn caps_flow(p: &mut ChildSpawnerCommands, c: &mut Ctx, flow: Flow, caps: u64) {
    let bright = amber(255.0);
    let sell = flow == Flow::Sell;
    let rect_x = FLOW_X + if sell { 20.0 } else { 0.0 };
    let name = if sell { "barter_arrow_sell.dds" } else { "barter_arrow_buy.dds" };
    c.image(p, &format!("interface\\barter\\{name}"), at(rect_x + 10.0 - if sell { 25.0 } else { 0.0 }, FLOW_Y - 15.0, 120.0, 360.0), bright);
    let figure = caps.to_string();
    let w = first_line(&c.kit.title.font, &figure).width;
    let x = flow_figure_x(flow, w);
    c.text(p, true, &figure, x, FLOW_Y + 50.0, Align::Centre, bright);
    c.text(p, false, "Caps", x + 2.5, FLOW_Y + 93.5, Align::Centre, bright);
}

fn buttons(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel) {
    let list: Vec<(&str, &str, Command, bool)> = match (m.mode, &m.trade) {
        (Mode::Barter(_), Some(t)) => vec![("Accept", "A)", Command::Accept, !t.is_empty()), ("Exit", "X)", Command::Close, true)],
        _ => vec![("Take All", "R)", Command::TakeAll, !m.list(Side::Container).rows.is_empty()), ("Exit", "E)", Command::Close, true)],
    };
    let h = 31.0 + BUTTON_PAD;
    for (i, (label, hint, cmd, on)) in list.into_iter().enumerate() {
        let color = amber(255.0).with_alpha(if on { 1.0 } else { DIM });
        let y = BUTTONS_Y + i as f32 * h;
        let w = c.text(p, false, label, BUTTONS_RIGHT, y + BUTTON_PAD, Align::Right, color);
        c.text(p, false, hint, BUTTONS_RIGHT + HINT_GAP, y + BUTTON_PAD, Align::Left, color);
        p.spawn((Button, CmdButton(cmd), at(BUTTONS_RIGHT - w - 10.0, y, w + 20.0 + HINT_GAP + 30.0, h)));
    }
}

/// The selected item's picture and figures: weight and value, and the condition of a
/// weapon or armour as a bar [documented: the tiles of `item_stats_display.xml`; the
/// damage, resistance, strength, ammunition, effects and mods tiles are not built].
fn item_data(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel, r: &Row) {
    let bright = amber(255.0);
    if let Some(icon) = r.icon.as_deref() {
        c.image(p, icon, at(ICON_AT.x, ICON_AT.y, ICON, ICON), bright);
    }
    let x0 = if matches!(m.mode, Mode::Barter(_)) { DATA_X_BARTER } else { DATA_X_CONTAINER };
    let prices = m.trade.as_ref().map(|t| t.prices).unwrap_or_default();
    let value = r.value.map(|v| prices.value_at(v, r.condition).round() as i64);
    let tiles = [
        (x0 + TILE.x + 10.0, "WG", r.weight.map(|w| format!("{w:.2}"))),
        (x0 + 2.0 * (TILE.x + 10.0), "VAL", value.map(|v| v.to_string())),
    ];
    for (x, title, v) in tiles {
        card(p, c, x, DATA_Y, title, v.as_deref().unwrap_or("--"));
    }
    if let (Some(cond), Some(ItemKind::Weapon | ItemKind::Armor)) = (r.condition, r.kind) {
        card(p, c, x0, DATA_Y + TILE.y, "CND", "");
        // [documented: meter.xml placed 62 in, 15 down, 20 high, 72 short of the tile]
        let (x, y, w) = (x0 + 62.0, DATA_Y + TILE.y + 15.0, TILE.x - 72.0);
        p.spawn((at(x, y, w, 20.0), BackgroundColor(amber(255.0).with_alpha(0.15))));
        p.spawn((at(x, y, w * cond.clamp(0.0, 1.0), 20.0), BackgroundColor(bright)));
    }
}

/// A tile of figures: a line along its top and one fading down its right side, the
/// title at the left and the value at the right.
fn card(p: &mut ChildSpawnerCommands, c: &mut Ctx, x: f32, y: f32, title: &str, value: &str) {
    let bright = amber(255.0);
    c.line(p, x, y, TILE.x, THICK, bright);
    c.fade(p, x + TILE.x - THICK, y, false, bright);
    c.text(p, false, title, x + 5.0, y + 20.0, Align::Left, bright);
    if !value.is_empty() {
        c.text(p, false, value, x + TILE.x - 10.0, y + 20.0, Align::Right, bright);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn headings_and_the_caps_figure() {
        // The barter screenshot's "ITEMS" (title font) at 50; the line resumes 20 after
        // the right arrow.
        let (l, t, r, (x, len)) = heading(80.0);
        assert_eq!((l, t, r, x, len), (30.0, 50.0, 140.0, 160.0, 304.0));
        assert_eq!(heading(500.0).3, (580.0, 0.0));
        // "9492" in the title font is about 70 wide: centred 509 from the menu's left
        // when the player is paid, as the screenshot shows.
        assert_eq!(flow_figure_x(Flow::Sell, 70.0), 20.0 + 449.0 + 20.0 + 32.5 + 7.5);
        assert_eq!(flow_figure_x(Flow::Buy, 90.0), 20.0 + 449.0 + 32.5 - 20.0);
        assert_eq!(flow_figure_x(Flow::Sell, 90.0), 20.0 + 449.0 + 20.0 + 40.0 + 20.0);
    }

    #[test]
    fn two_line_names_make_taller_rows() {
        assert_eq!((row_height(1), row_height(2)), (51.0, 86.0));
        // Seven rows of one line fit; two-line rows above the selection push the first
        // drawn row down until the selection fits.
        assert_eq!(first_shown(&[51.0; 10], 0, 6), 0);
        let mut h = vec![51.0; 10];
        h[1] = 86.0;
        h[2] = 86.0;
        assert_eq!(first_shown(&h, 0, 6), 2);
        assert_eq!(first_shown(&h, 0, 2), 0);
    }

    #[test]
    fn the_scroll_marker() {
        // Seven rows fit (14 half-rows of 26 in 365).
        assert_eq!(marker(7, 0), None);
        let (y, h) = marker(20, 0).unwrap();
        assert_eq!(y, 0.0);
        assert!(h >= 32.0);
        // Further down, further down the track, never past its end.
        let (mid, _) = marker(20, 6).unwrap();
        let (end, _) = marker(20, 13).unwrap();
        assert!(0.0 < mid && mid < end && end + h <= LIST_H - 2.0 * BAR_ARROW + 1e-3);
    }
}
