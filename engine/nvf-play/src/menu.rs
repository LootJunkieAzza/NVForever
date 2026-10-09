//! Part U (docs/design/layer4-play.md §4): container and inventory menus.
//!
//! [`model`] is the menu as plain data over the simulation (rows, selection, commands
//! moving items through `fnv_sim::World`), unit-tested without Bevy. This file draws it
//! with `bevy_ui` and turns keys and clicks into [`model::Command`]s.
//!
//! - `OpenContainer { container }` (Part A, or the F9 test aid) opens the container menu:
//!   container | player, in `PlayState::Menu`.
//! - Tab while playing opens the player's inventory alone: equip/unequip (aid items are
//!   used), drop, the encumbrance line.
//! - Bindings are ours, listed in each menu's footer ([`model::MenuModel::bindings`]):
//!   Up/Down (or W/S) select, Left/Right (or A/D) switch column, Enter/Space move one
//!   (inventory: equip/unequip), Shift+Enter/Space move the whole stack, R take all, X
//!   drop one, Shift+X drop the stack, Page Up/Down and the mouse wheel scroll, a click
//!   selects a row and a click on the selected row moves one (inventory: equips); E, Esc
//!   or Tab close; the button bar is clickable too. [hypothesis: R is the original's
//!   "take all" key on PC; not captured. Nothing else claims to follow the original's
//!   bindings or layout]
//! - The cursor is shown and released on entering a menu; Part P grabs it again when
//!   play resumes.
//! - The game clock stops while a menu is open (the simulation's clock runs only in
//!   `PlayState::Playing`, sim.rs). [hypothesis: not captured for the container and
//!   inventory menus]
//! - Dropping creates references in the simulation (`World::drop_item`); the menu sends
//!   `RefChanged` for each, and `crate::dropped` and `crate::phys` give them a model and
//!   collision. Each drop is logged.
//! - Transfers between inventories do not change any reference's presence, so they send
//!   no `RefChanged`.
//!
//! Test aids (only with `--play`, since this plugin exists only then):
//! - F9 while playing opens the container menu on the container reference nearest to
//!   the camera (in the current interior, or the worldspace), logging which one.
//! - `NVF_PLAY_TEST_MENU=<container FormID hex | nearest>` runs a scripted check (see
//!   [`test_aid`]).
//! - `NVF_PLAY_TEST_DROP=<seconds>:<item FormID hex>[:stack][,...]` drops one unit (or
//!   the stack) of a carried item through the inventory menu at those times (see
//!   [`test_aid`]).

pub mod model;
mod test_aid;

use crate::sim::SimState;
use crate::{space, ItemUsed, Location, OpenContainer, PlaySet, PlayState, PlayerCamera, RefChanged};
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use fnv_sim::FormId;
use model::{Command, MenuModel, Mode, Side};

/// The open menu, if any (`Some` exactly while in `PlayState::Menu`).
#[derive(Resource, Default)]
pub struct ActiveMenu(pub Option<MenuModel>);

/// A command for the open menu (keys, mouse, test aid).
#[derive(Message, Clone, Copy, Debug)]
pub struct MenuInput(pub Command);

/// Open the player's inventory (Tab, or the test aid).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenInventory;

/// Test aid: open the nearest container (F9, or `NVF_PLAY_TEST_MENU=nearest`).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenNearestContainer;

#[derive(Component)]
struct MenuRoot;

/// A clickable row.
#[derive(Component, Clone, Copy)]
struct RowButton(Side, usize);

/// A clickable command in the button bar.
#[derive(Component, Clone, Copy)]
struct CmdButton(Command);

pub struct MenuPlugin;

impl Plugin for MenuPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ActiveMenu>()
            .add_message::<MenuInput>()
            .add_message::<OpenInventory>()
            .add_message::<OpenNearestContainer>()
            .add_systems(
                Update,
                (
                    (tab_opens_inventory, f9_opens_nearest_container).run_if(in_state(PlayState::Playing)),
                    test_aid::run,
                    test_aid::run_drops,
                    open_menus,
                    (menu_keys, menu_mouse, apply_input, draw).chain().run_if(in_state(PlayState::Menu)),
                )
                    .chain()
                    .in_set(PlaySet::Ui),
            )
            .add_systems(OnEnter(PlayState::Menu), release_cursor)
            .add_systems(OnExit(PlayState::Menu), close_menu);
        test_aid::setup(app);
    }
}

fn tab_opens_inventory(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<OpenInventory>) {
    if keys.just_pressed(KeyCode::Tab) {
        out.write(OpenInventory);
    }
}

/// Test aid: F9 opens the nearest container.
fn f9_opens_nearest_container(
    keys: Res<ButtonInput<KeyCode>>,
    here: Res<Location>,
    sim: Res<SimState>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut requests: MessageReader<OpenNearestContainer>,
    mut out: MessageWriter<OpenContainer>,
) {
    if !keys.just_pressed(KeyCode::F9) && requests.read().count() == 0 {
        return;
    }
    let Ok(cam) = cam.single() else { return };
    let pos = space::to_game(cam.translation());
    match model::nearest_container(&sim.world, sim.refs, here.interior.map(FormId), FormId(here.world), pos) {
        Some((id, dist)) => {
            let name = sim.base_of(id.0).and_then(|b| sim.name(b)).unwrap_or("?");
            info!("F9: nearest container {id} ({name}) at {dist:.0} units");
            out.write(OpenContainer { container: id.0 });
        }
        None => info!("F9: no container here"),
    }
}

/// Open a menu on request while playing.
fn open_menus(
    mut containers: MessageReader<OpenContainer>,
    mut inventory: MessageReader<OpenInventory>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut active: ResMut<ActiveMenu>,
    mut sim: ResMut<SimState>,
    script: Option<ResMut<crate::script::ScriptState>>,
) {
    let container = containers.read().last().copied();
    let inv = inventory.read().count() > 0;
    if container.is_none() && !inv {
        return;
    }
    if *state.get() != PlayState::Playing || active.0.is_some() {
        info!("menu request ignored: a menu is already open");
        return;
    }
    let opened = match container {
        Some(c) => MenuModel::open_container(&mut sim.world, FormId(c.container)),
        None => MenuModel::open_inventory(&mut sim.world),
    };
    match opened {
        Ok(m) => {
            info!("menu: open {:?} \"{}\"", m.mode, m.title);
            // Scripts run MenuMode 1008 / 1002 from now on (layer5-play.md).
            crate::script::engine_menu_opened(script.map(|s| s.into_inner()), m.mode);
            active.0 = Some(m);
            next.set(PlayState::Menu);
        }
        Err(e) => warn!("menu: cannot open: {e}"),
    }
}

fn release_cursor(mut cursor: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    if let Ok(mut c) = cursor.single_mut() {
        c.visible = true;
        c.grab_mode = CursorGrabMode::None;
    }
}

fn close_menu(
    mut commands: Commands,
    mut active: ResMut<ActiveMenu>,
    roots: Query<Entity, With<MenuRoot>>,
    script: Option<ResMut<crate::script::ScriptState>>,
) {
    if let Some(m) = active.0.take() {
        crate::script::engine_menu_closed(script.map(|s| s.into_inner()), m.mode);
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
}

fn menu_keys(keys: Res<ButtonInput<KeyCode>>, mut out: MessageWriter<MenuInput>) {
    use KeyCode::*;
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    let mut send = |c| {
        out.write(MenuInput(c));
    };
    if keys.any_just_pressed([Escape, Tab, KeyE]) {
        send(Command::Close);
        return;
    }
    let map = [
        ([ArrowUp, KeyW], Command::Up),
        ([ArrowDown, KeyS], Command::Down),
        ([PageUp, PageUp], Command::PageUp),
        ([PageDown, PageDown], Command::PageDown),
        ([ArrowLeft, KeyA], Command::Focus(Side::Container)),
        ([ArrowRight, KeyD], Command::Focus(Side::Player)),
        ([KeyR, KeyR], Command::TakeAll),
    ];
    for (k, c) in map {
        if keys.any_just_pressed(k) {
            send(c);
        }
    }
    if keys.any_just_pressed([Enter, NumpadEnter, Space]) {
        send(if shift { Command::Stack } else { Command::One });
    }
    if keys.just_pressed(KeyX) {
        send(if shift { Command::DropStack } else { Command::Drop });
    }
}

fn menu_mouse(
    rows: Query<(&Interaction, &RowButton), Changed<Interaction>>,
    buttons: Query<(&Interaction, &CmdButton), Changed<Interaction>>,
    mut wheel: MessageReader<MouseWheel>,
    mut lines: Local<f32>,
    mut out: MessageWriter<MenuInput>,
) {
    for (i, r) in &rows {
        if *i == Interaction::Pressed {
            out.write(MenuInput(Command::Select(r.0, r.1)));
        }
    }
    for (i, b) in &buttons {
        if *i == Interaction::Pressed {
            out.write(MenuInput(b.0));
        }
    }
    for w in wheel.read() {
        *lines += match w.unit {
            MouseScrollUnit::Line => w.y,
            MouseScrollUnit::Pixel => w.y / 20.0,
        };
    }
    let whole = lines.trunc();
    if whole != 0.0 {
        // Wheel up (positive) moves the selection up.
        out.write(MenuInput(Command::Scroll(-whole as i32)));
        *lines -= whole;
    }
}

fn apply_input(
    mut input: MessageReader<MenuInput>,
    mut active: ResMut<ActiveMenu>,
    mut sim: ResMut<SimState>,
    mut next: ResMut<NextState<PlayState>>,
    mut changed: MessageWriter<RefChanged>,
    mut used: MessageWriter<ItemUsed>,
) {
    for &MenuInput(cmd) in input.read() {
        let Some(m) = active.0.as_mut() else { return };
        let a = m.apply(&mut sim.world, cmd);
        let moves_items = matches!(cmd, Command::One | Command::Stack | Command::TakeAll | Command::Drop | Command::DropStack);
        if moves_items || a.changed {
            if let Some(s) = m.status.as_deref().filter(|s| !s.is_empty()) {
                info!("menu: {cmd:?}: {s}");
            }
        }
        if let Some(item) = a.used {
            used.write(ItemUsed { item: item.0 });
        }
        for id in &a.created {
            let (at, count) = sim.world.peek(*id).map(|s| (s.pos, s.count)).unwrap_or_default();
            info!("menu: dropped reference {id} (count {count}) at ({:.1}, {:.1}, {:.1})", at[0], at[1], at[2]);
            changed.write(RefChanged { form_id: id.0 });
        }
        if a.close {
            info!("menu: close");
            next.set(PlayState::Playing);
            return;
        }
    }
}

// ---- drawing -----------------------------------------------------------------------

const BACKDROP: Color = Color::srgba(0.0, 0.0, 0.0, 0.45);
const PANEL: Color = Color::srgba(0.05, 0.08, 0.06, 0.94);
const TEXT: Color = Color::srgb(0.85, 0.95, 0.80);
const DIM: Color = Color::srgb(0.55, 0.65, 0.52);
const ACCENT: Color = Color::srgb(1.0, 0.75, 0.30);
const WARN: Color = Color::srgb(1.0, 0.40, 0.35);
const SELECTED: Color = Color::srgba(0.35, 0.55, 0.30, 0.85);
const SELECTED_UNFOCUSED: Color = Color::srgba(0.30, 0.38, 0.28, 0.6);
const BUTTON: Color = Color::srgba(0.20, 0.28, 0.20, 0.9);

const ROW_HEIGHT: f32 = 22.0;
const NAME_WIDTH: f32 = 290.0;
const NUMBER_WIDTH: f32 = 64.0;

fn text(s: impl Into<String>, size: f32, color: Color) -> impl Bundle {
    (Text::new(s), TextFont { font_size: FontSize::Px(size), ..default() }, TextColor(color))
}

fn cell(s: impl Into<String>, width: f32, color: Color) -> impl Bundle {
    (Node { width: Val::Px(width), overflow: Overflow::clip(), ..default() }, children![text(s, 15.0, color)])
}

fn number(v: Option<impl std::fmt::Display>) -> String {
    v.map_or("?".into(), |v| v.to_string())
}

/// Rebuild the menu's UI when the model changed.
fn draw(mut commands: Commands, active: Res<ActiveMenu>, roots: Query<Entity, With<MenuRoot>>) {
    if !active.is_changed() && !roots.is_empty() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(m) = active.0.as_ref() else { return };
    let backdrop = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    commands.spawn((MenuRoot, backdrop, BackgroundColor(BACKDROP), GlobalZIndex(100))).with_children(|root| {
        let panel = Node {
            flex_direction: FlexDirection::Column,
            padding: UiRect::all(Val::Px(16.0)),
            row_gap: Val::Px(8.0),
            border: UiRect::all(Val::Px(1.0)),
            ..default()
        };
        root.spawn((panel, BackgroundColor(PANEL), BorderColor::all(DIM))).with_children(|p| panel_contents(p, m));
    });
}

fn panel_contents(p: &mut ChildSpawnerCommands, m: &MenuModel) {
    p.spawn(text(m.title.clone(), 24.0, ACCENT));
    p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(20.0), ..default() }).with_children(|row| {
        let sides: &[Side] = match m.mode {
            Mode::Container(_) => &[Side::Container, Side::Player],
            Mode::Inventory => &[Side::Player],
        };
        for &side in sides {
            column(row, m, side);
        }
    });
    let (enc, color) = match &m.encumbrance {
        Ok(e) => (
            format!("Carry weight {:.1} / {:.0}{}", e.weight, e.capacity, if e.over { "   OVER-ENCUMBERED" } else { "" }),
            if e.over { WARN } else { TEXT },
        ),
        Err(e) => (format!("Carry weight unknown: {e}"), WARN),
    };
    p.spawn(text(enc, 16.0, color));
    let status = m.status.clone().unwrap_or_default();
    let color = if status.starts_with("Cannot") { WARN } else { ACCENT };
    p.spawn(text(status, 16.0, color));
    p.spawn(Node { flex_direction: FlexDirection::Row, column_gap: Val::Px(10.0), ..default() }).with_children(|bar| {
        let buttons: &[(&str, Command)] = match m.mode {
            Mode::Container(_) => &[
                ("Move one (Enter)", Command::One),
                ("Move stack (Shift+Enter)", Command::Stack),
                ("Take all (R)", Command::TakeAll),
                ("Close (E)", Command::Close),
            ],
            Mode::Inventory => &[
                ("Equip / unequip (Enter)", Command::One),
                ("Drop one (X)", Command::Drop),
                ("Drop stack (Shift+X)", Command::DropStack),
                ("Close (Tab)", Command::Close),
            ],
        };
        for &(label, cmd) in buttons {
            let node = Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() };
            bar.spawn((Button, CmdButton(cmd), node, BackgroundColor(BUTTON))).with_child(text(label, 15.0, TEXT));
        }
    });
    for line in m.bindings() {
        p.spawn(text(line, 13.0, DIM));
    }
}

fn column(parent: &mut ChildSpawnerCommands, m: &MenuModel, side: Side) {
    let focused = m.focus == side;
    let list = m.list(side);
    let heading = match (side, m.mode) {
        (Side::Container, _) => m.title.clone(),
        (Side::Player, Mode::Container(_)) => "You".into(),
        (Side::Player, Mode::Inventory) => "Items".into(),
    };
    let node = Node {
        flex_direction: FlexDirection::Column,
        min_width: Val::Px(NAME_WIDTH + 3.0 * NUMBER_WIDTH + 14.0),
        padding: UiRect::all(Val::Px(6.0)),
        border: UiRect::all(Val::Px(1.0)),
        row_gap: Val::Px(2.0),
        ..default()
    };
    parent.spawn((node, BorderColor::all(if focused { ACCENT } else { DIM }))).with_children(|c| {
        c.spawn(text(heading, 18.0, if focused { ACCENT } else { TEXT }));
        c.spawn((
            Node { flex_direction: FlexDirection::Row, ..default() },
            children![
                cell("  Item", NAME_WIDTH, DIM),
                cell("Count", NUMBER_WIDTH, DIM),
                cell("Weight", NUMBER_WIDTH, DIM),
                cell("Value", NUMBER_WIDTH, DIM)
            ],
        ));
        let range = m.visible(side);
        let above = if range.start > 0 { format!("  ^ {} more", range.start) } else { String::new() };
        c.spawn(text(above, 12.0, DIM));
        if list.rows.is_empty() {
            c.spawn(text("  (empty)", 15.0, DIM));
        }
        for i in range.clone() {
            let r = &list.rows[i];
            let bg = match (m.selected[side as usize] == i, focused) {
                (true, true) => SELECTED,
                (true, false) => SELECTED_UNFOCUSED,
                _ => Color::NONE,
            };
            let name = format!("{} {}", if r.equipped { "*" } else { " " }, r.name);
            let node = Node { flex_direction: FlexDirection::Row, height: Val::Px(ROW_HEIGHT), align_items: AlignItems::Center, ..default() };
            c.spawn((Button, RowButton(side, i), node, BackgroundColor(bg))).with_children(|row| {
                row.spawn(cell(name, NAME_WIDTH, TEXT));
                row.spawn(cell(r.count.to_string(), NUMBER_WIDTH, TEXT));
                row.spawn(cell(number(r.weight), NUMBER_WIDTH, TEXT));
                row.spawn(cell(number(r.value), NUMBER_WIDTH, TEXT));
            });
        }
        let below = if range.end < list.rows.len() { format!("  v {} more", list.rows.len() - range.end) } else { String::new() };
        c.spawn(text(below, 12.0, DIM));
        c.spawn(text(format!("{} stacks   weight {:.1}   value {}", list.rows.len(), list.weight, list.value), 14.0, DIM));
    });
}
