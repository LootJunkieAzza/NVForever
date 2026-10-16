//! The ITEMS page (docs/spec-notes/pipboy.md §4): the player's inventory, one tab per
//! kind of item (Weapons, Apparel, Aid, Misc, Ammo), over the inventory model
//! (`menu::model`, `Mode::Inventory`).
//!
//! Lengths are the canvas's, from the menus' rectangle (`frame::MAIN`) as
//! `menus\main\inventory_menu.xml` gives them [documented] unless marked otherwise.
//!
//! - The heading's cards: Wg (carried weight / what the player can carry), HP, DT and
//!   Caps [observed: the owner's screenshots; documented: their places].
//! - The list: one row per stack, its name in font 2 with the count in brackets, a filled
//!   square before what is equipped; the selected row boxed.
//! - Keys: Up/Down (W/S) select, Enter or Space equip, unequip or use, X drop one,
//!   Shift+X the stack [design: as the inventory had]; the wheel scrolls; a click on a
//!   row selects it, a click on the selected row acts on it.
//! - The scroll bar at the list's left, only while the list is longer than its window:
//!   the arrows at its ends and a line for the part shown [observed: the owner's scroll
//!   screenshot; the DATA lists' bar, `data::scroll_bar`].
//! - The selected (pointed at) item at the right: its picture (`IM_ItemIcon`, 450, 10,
//!   380 square) and its figures (`IM_ItemInfoRect`, 385, 340, 446 × 150, three cards a
//!   row of a third of it, 10 apart, a third of its height high, `item_stats_display.xml`)
//!   [documented: places]: [`info_tiles`].

use super::frame::{self, Align, Card, Ctx, Kit, LINE, MAIN, TEXT, THICK};
use super::{Page, PageBody, PipBoy};
use crate::interface::{amber, at};
use crate::menu::model::{self, Category, Command, MenuModel, Mode, Row, Side};
use crate::menu::{ActiveMenu, MenuInput};
use crate::sim::SimState;
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use fnv_sim::FormId;
use nvf_assets::GameData;

/// The heading's cards: x and width inside the menus' rectangle.
const WG: (f32, f32) = (215.0, 155.0);
const HP: (f32, f32) = (380.0, 155.0);
const DT: (f32, f32) = (545.0, 110.0);
const CAPS: (f32, f32) = (665.0, 190.0);
/// The list: 15 left of the menus' rectangle, 75 down, 400 × 468; a row 32 in and 393
/// wide, its name 40 in and 20 down, broken at 300; the equipped square 25 across at
/// 15, 18.
const LIST: Rect = Rect { min: Vec2::new(-15.0, 75.0), max: Vec2::new(385.0, 543.0) };
const ROW_X: f32 = 32.0;
const ROW_W: f32 = 393.0;
const NAME_X: f32 = 40.0;
const NAME_Y: f32 = 20.0;
const WRAP: f32 = 300.0;
const MARK: Rect = Rect { min: Vec2::new(15.0, 18.0), max: Vec2::new(40.0, 43.0) };
/// A row is 51 high and its name's further lines 35 below the first [hypothesis: as the
/// barter menu's rows, measured there, barter-menu.md §3].
pub const ROW_H: f32 = 51.0;
const LINE_PITCH: f32 = 35.0;

/// The category of each tab.
pub const TABS: [Category; 5] = [Category::Weapons, Category::Apparel, Category::Aid, Category::Misc, Category::Ammo];

/// A row's text: its name, and its count in brackets when more than one.
pub fn row_text(r: &Row) -> String {
    if r.count > 1 {
        format!("{} ({})", r.name, r.count)
    } else {
        r.name.clone()
    }
}

/// The item picture's box and the figures' rectangle, inside the menus' rectangle.
const ICON: Rect = Rect { min: Vec2::new(450.0, 10.0), max: Vec2::new(830.0, 390.0) };
const INFO: Rect = Rect { min: Vec2::new(385.0, 340.0), max: Vec2::new(831.0, 490.0) };
/// The condition meter inside its card: 62 in, 15 down, 20 high, 72 short of the card
/// [documented: item_stats_display.xml ConditionMeter].
const METER: (f32, f32, f32, f32) = (62.0, 15.0, 20.0, 72.0);
/// The scroll bar: 32 wide, arrows 32 × 16 [documented: list_box.xml, the DATA lists].
const SCROLLBAR: f32 = 32.0;

/// What the figures show of an item.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    /// The first card: ("DAM", damage) of a weapon, ("DT", threshold) of armour.
    pub first: Option<(&'static str, String)>,
    pub weight: Option<String>,
    pub value: Option<String>,
    /// A weapon's Strength requirement, over the value.
    pub strength: Option<u32>,
    /// Condition 0 to 1 of a weapon or armour.
    pub condition: Option<f32>,
    /// Beside the condition: a weapon's ammunition "20ga (2/30)", armour's class.
    pub beside: Option<String>,
    /// The effects' text ("HP +40", "AGL -1").
    pub effects: Option<String>,
}

/// One card of the figures, canvas units inside the figures' rectangle.
#[derive(Clone, Debug, PartialEq)]
pub struct Tile {
    pub rect: Rect,
    pub title: String,
    pub value: String,
    /// The condition meter's fill.
    pub meter: Option<f32>,
}

/// The cards of the figures [documented: item_stats_display.xml: a card is a third of the
/// rectangle wide and high; damage or DT at the left of the first row, weight and value
/// 10 apart after it; the Strength requirement over the value; the second row the
/// condition and beside it the ammunition two cards and 10 wide; effects across the
/// three cards and 20 (hypothesis: on the first free row, as on the owner's apparel and
/// aid screenshots)].
pub fn info_tiles(i: &Info) -> Vec<Tile> {
    let (w, h) = (INFO.width() / 3.0, INFO.height() / 3.0);
    let col = |n: f32| n * (w + 10.0);
    let tile = |x: f32, y: f32, width: f32, title: &str, value: String| Tile { rect: Rect::new(x, y, x + width, y + h), title: title.into(), value, meter: None };
    let mut out = Vec::new();
    if let Some((title, v)) = &i.first {
        out.push(tile(0.0, 0.0, w, title, v.clone()));
    }
    out.push(tile(col(1.0), 0.0, w, "WG", i.weight.clone().unwrap_or_else(|| "--".into())));
    out.push(tile(col(2.0), 0.0, w, "VAL", i.value.clone().unwrap_or_else(|| "--".into())));
    if let Some(s) = i.strength {
        out.push(tile(col(2.0), -h, w, "STR", s.to_string()));
    }
    let mut row = 1.0;
    if i.condition.is_some() || i.beside.is_some() {
        if let Some(cnd) = i.condition {
            out.push(Tile { meter: Some(cnd.clamp(0.0, 1.0)), ..tile(0.0, h, w, "CND", String::new()) });
        }
        if let Some(b) = &i.beside {
            out.push(tile(col(1.0), h, 2.0 * w + 10.0, b, String::new()));
        }
        row = 2.0;
    }
    if let Some(e) = &i.effects {
        out.push(tile(0.0, row * h, 3.0 * w + 20.0, "EFFECTS", e.clone()));
    }
    out
}

/// A weight or value of a stack: "--" for nothing [observed: the Stimpak's WG "--"; its
/// VAL 675 is 9 × 75, so the figures are the stack's (hypothesis for the weight)].
fn figure(v: f32, decimals: bool) -> Option<String> {
    (v > 0.0).then(|| if decimals { format!("{v:.2}") } else { format!("{v:.0}") })
}

/// What the figures show of a row, from the records.
fn info_of(r: &Row, combat: Option<&crate::combat::Combat>, sim: &mut SimState, forms: Option<&mut super::stats::read::Forms>) -> Info {
    use fnv_sim::forms::ItemKind;
    let n = r.count.max(1) as f32;
    let mut i = Info {
        weight: r.weight.and_then(|w| figure(w * n, true)),
        value: r.value.and_then(|v| figure(v as f32 * n, false)),
        condition: r.condition.filter(|_| matches!(r.kind, Some(ItemKind::Weapon | ItemKind::Armor))),
        ..Info::default()
    };
    let cf = combat.map(|c| &c.forms);
    match r.kind {
        Some(ItemKind::Weapon) => {
            if let Some(w) = cf.and_then(|f| f.weapons.get(&r.item)) {
                // Binoculars and the like are weapons without damage: no DAM or STR
                // [hypothesis].
                i.first = (w.base_damage > 0).then(|| ("DAM", w.base_damage.to_string()));
                i.strength = (w.strength_req > 0).then_some(w.strength_req);
                let f = cf.expect("weapon stats came from it");
                let choices = f.ammo_choices(w);
                let carried: Vec<i32> = choices.iter().map(|&a| sim.world.item_count(FormId::PLAYER_REF, a).unwrap_or(0)).collect();
                let pick = choices.iter().zip(&carried).find(|(_, n)| **n > 0).or(choices.iter().zip(&carried).next());
                if let Some((&ammo, &count)) = pick {
                    let a = &f.ammo[&ammo];
                    let name = a.short_name.clone().or_else(|| a.name.clone()).unwrap_or_default();
                    // In the clip: as much as a clip holds of what is carried [hypothesis:
                    // "(2/30)" on the owner's screenshot reads as clip/carried].
                    let clip = (w.clip_size as i32).min(count).max(0);
                    i.beside = Some(format!("{name} ({clip}/{count})"));
                }
            }
        }
        Some(ItemKind::Armor) => {
            if let Some(a) = cf.and_then(|f| f.armor.get(&r.item)) {
                i.first = Some(("DT", format!("{:.0}", a.dt)));
            }
            i.beside = sim.forms.item(r.item).map(|it| format!("{:?}", it.armor_class));
        }
        _ => {}
    }
    if let Some(forms) = forms {
        // Aid items carry their effects; apparel and weapons their enchantment's.
        let ench = if r.kind == Some(ItemKind::Aid) { Some(r.item.0) } else { forms.item_ench(r.item.0) };
        if let Some(e) = ench.map(|e| forms.ench(e)).filter(|e| !e.changes.is_empty()) {
            let changes: Vec<super::stats::model::Change> = e.changes.iter().map(|&(code, amount, detrimental)| super::stats::model::Change { value: forms.short_name(code), amount, detrimental }).collect();
            i.effects = Some(super::stats::model::changes_text(&changes));
        }
    }
    i
}

/// A clickable row of the list.
#[derive(Component, Clone, Copy)]
struct RowButton(usize);

/// The ITEMS page's keys and wheel, while it is shown.
pub fn keys(
    pip: Res<PipBoy>,
    active: Res<ActiveMenu>,
    keys: Res<ButtonInput<KeyCode>>,
    mut wheel: MessageReader<MouseWheel>,
    mut lines: Local<f32>,
    mut out: MessageWriter<MenuInput>,
) {
    use KeyCode::*;
    let items = pip.open() && pip.page == Page::Items && active.0.as_ref().is_some_and(|m| m.mode == Mode::Inventory);
    let mut send = |c| {
        out.write(MenuInput(c));
    };
    if !items {
        wheel.clear();
        return;
    }
    // The model shows the tab's kind of item.
    let want = TABS[pip.tab()];
    if active.0.as_ref().is_some_and(|m| m.category[Side::Player as usize] != want) {
        send(Command::SetCategory(Side::Player, want));
    }
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    for (k, c) in [([ArrowUp, KeyW], Command::Up), ([ArrowDown, KeyS], Command::Down), ([PageUp, PageUp], Command::PageUp), ([PageDown, PageDown], Command::PageDown)] {
        if keys.any_just_pressed(k) {
            send(c);
        }
    }
    if keys.any_just_pressed([Enter, NumpadEnter, Space]) {
        send(Command::One);
    }
    if keys.just_pressed(KeyX) {
        send(if shift { Command::DropStack } else { Command::Drop });
    }
    for w in wheel.read() {
        *lines += match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 20.0,
        };
    }
    let whole = lines.trunc();
    if whole != 0.0 {
        send(Command::Scroll(-whole as i32));
        *lines -= whole;
    }
}

fn on_row_over(over: On<Pointer<Over>>, rows: Query<&RowButton>, active: Res<ActiveMenu>, mut out: MessageWriter<MenuInput>) {
    let (Ok(r), Some(m)) = (rows.get(over.entity), active.0.as_ref()) else { return };
    if m.selected[Side::Player as usize] != r.0 {
        out.write(MenuInput(Command::Hover(Side::Player, r.0)));
    }
}

fn on_row_click(click: On<Pointer<Click>>, rows: Query<&RowButton>, mut out: MessageWriter<MenuInput>) {
    if let Ok(r) = rows.get(click.entity) {
        out.write(MenuInput(Command::Select(Side::Player, r.0)));
    }
}

/// Fill the ITEMS page's body when it is new or the inventory changed.
#[allow(clippy::too_many_arguments)]
pub fn draw(
    mut commands: Commands,
    bodies: Query<(Entity, &PageBody)>,
    added: Query<(), Added<PageBody>>,
    active: Res<ActiveMenu>,
    mut sim: ResMut<SimState>,
    mut kit: ResMut<Kit>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    combat: Option<Res<crate::combat::Combat>>,
    play: Option<Res<crate::PlayWorld>>,
    mut forms: Local<Option<super::stats::read::Forms>>,
) {
    let Some((body, _)) = bodies.iter().find(|(_, b)| b.page == Page::Items) else { return };
    if added.get(body).is_err() && !active.is_changed() {
        return;
    }
    let Some(m) = active.0.as_ref().filter(|m| m.mode == Mode::Inventory) else { return };
    let sim = sim.bypass_change_detection();
    let caps = sim.world.item_count(FormId::PLAYER_REF, model::CAPS).unwrap_or(0);
    if forms.is_none() {
        *forms = play.as_ref().map(|p| super::stats::read::Forms::new(p.lo, p.schema));
    }
    let selected = m.list(Side::Player).rows.get(m.selected[Side::Player as usize]);
    let info = selected.map(|r| (r.icon.clone(), info_of(r, combat.as_deref(), sim, forms.as_mut())));
    if let (Some(r), Some((_, i))) = (selected, &info) {
        info!("pipboy items: {} selected: {:?}", r.name, i);
    }
    let mut c = Ctx::new(&mut kit, data.as_deref(), &mut images, formats.as_deref());
    commands.entity(body).despawn_children().with_children(|p| {
        let wg = match &m.encumbrance {
            Ok(e) => format!("{:.0}/{:.0}", e.weight, e.capacity),
            Err(_) => "--".into(),
        };
        let cards = [
            Card::new(WG.0, WG.1, "Wg", wg),
            Card::new(HP.0, HP.1, "HP", "--"),
            Card::new(DT.0, DT.1, "DT", "--"),
            Card::new(CAPS.0, CAPS.1, "Caps", caps.to_string()),
        ];
        frame::heading(p, &mut c, Page::Items.title(), &cards);
        list(p, &mut c, m);
        if let Some((icon, i)) = &info {
            item_info(p, &mut c, icon.as_deref(), i);
        }
    });
}

/// The selected item's picture and figures.
fn item_info(p: &mut ChildSpawnerCommands, c: &mut Ctx, icon: Option<&str>, i: &Info) {
    let bright = amber(TEXT);
    if let Some(icon) = icon {
        let r = Rect::from_corners(MAIN.min + ICON.min, MAIN.min + ICON.max);
        c.image(p, icon, at(r.min.x, r.min.y, r.width(), r.height()), bright);
    }
    let o = MAIN.min + INFO.min;
    for t in info_tiles(i) {
        let (x, y, w) = (o.x + t.rect.min.x, o.y + t.rect.min.y, t.rect.width());
        frame::card(p, c, x, y, w, &t.title, &t.value);
        if let Some(fill) = t.meter {
            let (mx, my, mh, short) = METER;
            let mw = w - short;
            p.spawn((at(x + mx, y + my, mw, mh), BackgroundColor(bright.with_alpha(0.15))));
            p.spawn((at(x + mx, y + my, mw * fill, mh), BackgroundColor(bright)));
        }
    }
}

fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel) {
    let bright = amber(TEXT);
    let line = amber(LINE);
    let x0 = MAIN.min.x + LIST.min.x + ROW_X;
    let mut y = MAIN.min.y + LIST.min.y;
    let rows = &m.list(Side::Player).rows;
    let selected = m.selected[Side::Player as usize];
    let shown = m.visible(Side::Player);
    let first = shown.start;
    let mut fit = 0;
    for i in shown {
        let r = &rows[i];
        let lines = c.lines(2, &row_text(r), Some(WRAP));
        let h = ROW_H + LINE_PITCH * lines.len().saturating_sub(1) as f32;
        if y + h > MAIN.min.y + LIST.max.y {
            break;
        }
        fit += 1;
        if i == selected {
            c.boxed(p, Rect::new(x0, y, x0 + ROW_W, y + h), line, None);
        }
        if r.equipped {
            let mk = Rect::new(x0 + MARK.min.x, y + MARK.min.y, x0 + MARK.max.x, y + MARK.max.y);
            if c.image(p, "square_filled.dds", at(mk.min.x, mk.min.y, mk.width(), mk.height()), bright).is_none() {
                p.spawn((at(mk.min.x, mk.min.y, mk.width(), mk.height()), BackgroundColor(bright)));
            }
        }
        for (k, l) in lines.iter().enumerate() {
            c.set(p, 2, l, x0 + NAME_X, y + NAME_Y - THICK + LINE_PITCH * k as f32, Align::Left, bright);
        }
        p.spawn((at(x0, y, ROW_W, h), RowButton(i), Pickable::default())).observe(on_row_over).observe(on_row_click);
        y += h;
    }
    // The scroll bar, only while some rows do not show.
    if let Some((from, part)) = scroll_part(first, fit, rows.len()) {
        let (x, top) = (MAIN.min.x + LIST.min.x, MAIN.min.y + LIST.min.y);
        p.spawn((at(x, top, SCROLLBAR, LIST.height()), Pickable::IGNORE)).with_children(|b| super::data::scroll_bar(b, c, 0.0, LIST.height(), from, part));
    }
}

/// Where the scroll bar's line starts and how much of the track it takes, when the rows
/// from `first`, `fit` of them, are not all of `len`.
pub fn scroll_part(first: usize, fit: usize, len: usize) -> Option<(f32, f32)> {
    (len > 0 && (first > 0 || first + fit < len)).then(|| (first as f32 / len as f32, fit as f32 / len as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stack_shows_its_count() {
        let mut r = Row {
            item: FormId(1),
            name: "Stimpak".into(),
            count: 9,
            weight: None,
            value: None,
            kind: None,
            equipped: false,
            quest: false,
            condition: None,
            icon: None,
            price: None,
            bartered: false,
        };
        assert_eq!(row_text(&r), "Stimpak (9)");
        r.count = 1;
        assert_eq!(row_text(&r), "Stimpak");
    }

    #[test]
    fn the_scroll_bar_shows_only_for_a_longer_list() {
        assert_eq!(scroll_part(0, 5, 5), None);
        assert_eq!(scroll_part(0, 0, 0), None);
        assert_eq!(scroll_part(0, 9, 18), Some((0.0, 0.5)));
        assert_eq!(scroll_part(9, 9, 18), Some((0.5, 0.5)));
    }

    #[test]
    fn the_figures_are_laid_out_as_the_file() {
        let (w, h) = (INFO.width() / 3.0, INFO.height() / 3.0);
        // A weapon: DAM, WG, VAL; STR over VAL; CND and the ammunition.
        let weapon = Info {
            first: Some(("DAM", "72".into())),
            weight: Some("5.50".into()),
            value: Some("299".into()),
            strength: Some(3),
            condition: Some(0.8),
            beside: Some("40mm Grenade (1/19)".into()),
            effects: None,
        };
        let t = info_tiles(&weapon);
        let titles: Vec<&str> = t.iter().map(|t| t.title.as_str()).collect();
        assert_eq!(titles, ["DAM", "WG", "VAL", "STR", "CND", "40mm Grenade (1/19)"]);
        assert_eq!(t[1].rect.min, Vec2::new(w + 10.0, 0.0));
        assert_eq!(t[3].rect.min, Vec2::new(2.0 * (w + 10.0), -h));
        assert_eq!(t[4].meter, Some(0.8));
        assert!((t[5].rect.width() - (2.0 * w + 10.0)).abs() < 1e-4);
        // An aid item: WG and VAL, effects on the second row.
        let aid = Info { value: Some("675".into()), effects: Some("HP +40".into()), ..Info::default() };
        let t = info_tiles(&aid);
        assert_eq!(t[0].value, "--");
        assert_eq!((t[2].title.as_str(), t[2].rect.min.y), ("EFFECTS", h));
        // Armour: effects under the condition row.
        let armour = Info { condition: Some(1.0), beside: Some("Medium".into()), effects: Some("AGL -1".into()), ..Info::default() };
        assert_eq!(info_tiles(&armour).last().map(|t| t.rect.min.y), Some(2.0 * h));
    }
}
