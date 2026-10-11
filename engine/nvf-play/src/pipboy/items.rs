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
) {
    let Some((body, _)) = bodies.iter().find(|(_, b)| b.page == Page::Items) else { return };
    if added.get(body).is_err() && !active.is_changed() {
        return;
    }
    let Some(m) = active.0.as_ref().filter(|m| m.mode == Mode::Inventory) else { return };
    let caps = sim.bypass_change_detection().world.item_count(FormId::PLAYER_REF, model::CAPS).unwrap_or(0);
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
    });
}

fn list(p: &mut ChildSpawnerCommands, c: &mut Ctx, m: &MenuModel) {
    let bright = amber(TEXT);
    let line = amber(LINE);
    let x0 = MAIN.min.x + LIST.min.x + ROW_X;
    let mut y = MAIN.min.y + LIST.min.y;
    let rows = &m.list(Side::Player).rows;
    let selected = m.selected[Side::Player as usize];
    for i in m.visible(Side::Player) {
        let r = &rows[i];
        let lines = c.lines(2, &row_text(r), Some(WRAP));
        let h = ROW_H + LINE_PITCH * lines.len().saturating_sub(1) as f32;
        if y + h > MAIN.min.y + LIST.max.y {
            break;
        }
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
}
