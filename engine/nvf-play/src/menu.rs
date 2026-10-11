//! Part U (docs/design/layer4-play.md §4): container, barter and inventory menus.
//!
//! [`model`] is the menu as plain data over the simulation (rows, selection, commands
//! moving items through `fnv_sim::World`), unit-tested without Bevy; [`barter`] the
//! prices. [`look`] draws the container and barter menus as the original does
//! (docs/spec-notes/barter-menu.md); the inventory is the Pip-Boy's ITEMS page
//! (`crate::pipboy`), which draws it and takes its keys. This file opens and closes the
//! menus and turns the container and barter menus' keys and clicks into
//! [`model::Command`]s.
//!
//! - `OpenContainer { container }` (Part A, or the F10 test aid) opens the container menu:
//!   player | container, in `PlayState::Menu`.
//! - [`OpenBarter`] opens the barter menu: player | merchant. Scripts ask for it with
//!   `ShowBarterMenu` (`fnv_game::game::Game::barter_requests`), mostly from a dialogue
//!   choice's result script; then it opens over the conversation, which waits under it
//!   (`crate::dialogue::held`) and goes on when it closes [hypothesis: the original
//!   returns to the conversation; its BarterExit lines are not said yet].
//! - Tab tapped while playing opens the player's inventory alone, when it is let go (a
//!   hold switches the flashlight, `crate::flashlight`), and with it the Pip-Boy
//!   (`crate::pipboy`): equip/unequip (aid items are used), drop.
//! - Keys of the container menu: Up/Down (or W/S) select, Left/Right (or A/D) switch
//!   list, Enter/Space take or store one, Shift+Enter/Space the whole stack, R take all, E,
//!   Esc or Tab close [observed: the screenshot's "Take All R)" and "Exit E)"]. Barter:
//!   the same, but Left/Right only, A accepts and X exits [observed: "Accept A)", "Exit
//!   X)"]; Esc and Tab exit too. In both, `[` and `]` change the focused list's category
//!   [design], the pointer selects the row it is on and a click acts on it, Shift+click
//!   on the whole stack; the title arrows and the buttons are clickable. The inventory's
//!   keys are the Pip-Boy's (`crate::pipboy::items`). [hypothesis:
//!   the original's PC keys beyond the hints shown are not captured]
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
//! - F10 while playing opens the container menu on the container reference nearest to
//!   the camera (in the current interior, or the worldspace), logging which one.
//! - `NVF_PLAY_TEST_MENU=<container FormID hex | nearest>` runs a scripted check (see
//!   [`test_aid`]).
//! - `NVF_PLAY_TEST_BARTER=<merchant FormID hex>[:discount]` opens the barter menu and
//!   makes one trade (see [`test_aid`]).
//! - `NVF_PLAY_TEST_DROP=<seconds>:<item FormID hex>[:stack][,...]` drops one unit (or
//!   the stack) of a carried item through the inventory menu at those times (see
//!   [`test_aid`]).

pub mod barter;
mod look;
pub mod model;
mod test_aid;

use crate::sim::SimState;
use crate::{space, ItemUsed, Location, OpenContainer, PlaySet, PlayState, PlayerCamera, RefChanged};
use bevy::image::CompressedImageFormatSupport;
use bevy::input::mouse::{MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy::window::{CursorGrabMode, CursorOptions, PrimaryWindow};
use fnv_sim::forms::BaseData;
use fnv_sim::FormId;
use model::{Command, MenuModel, Mode, Side};
use nvf_assets::GameData;

/// The open menu, if any: `Some` while in `PlayState::Menu`, and while a barter menu is
/// open over a conversation (`PlayState::Dialogue`).
#[derive(Resource, Default)]
pub struct ActiveMenu(pub Option<MenuModel>);

/// Open the barter menu with this merchant (an actor reference); `discount` as
/// `ShowBarterMenu`'s.
#[derive(Message, Clone, Copy, Debug)]
pub struct OpenBarter {
    pub merchant: u32,
    pub discount: i32,
}

/// The player's Barter skill when the script runtime cannot say (no scripts): every
/// skill of a new character [observed: actor-values.md §7, the player's skills all 15].
const DEFAULT_BARTER_SKILL: f32 = 15.0;

/// The line under the container menu for a container that refills [observed: the
/// owner's container screenshot].
const RESPAWN_NOTE: &str = "This container respawns and is unsafe to store your items.";

/// A command for the open menu (keys, mouse, test aid).
#[derive(Message, Clone, Copy, Debug)]
pub struct MenuInput(pub Command);

/// Open the player's inventory (Tab tapped, `crate::flashlight`; or the test aid).
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct OpenInventory;

/// The menus' systems (the Pip-Boy follows what they opened or closed).
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub struct MenuSystems;

/// Test aid: open the nearest container (F10, or `NVF_PLAY_TEST_MENU=nearest`).
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
            .add_message::<OpenBarter>()
            .add_systems(
                Update,
                (
                    f9_opens_nearest_container.run_if(in_state(PlayState::Playing)),
                    test_aid::run,
                    test_aid::run_drops,
                    test_aid::run_barter,
                    scripts_open_barter,
                    open_menus,
                    keep_menu_state,
                    (menu_keys, menu_mouse, apply_input).chain().run_if(menu_open),
                    draw,
                )
                    .chain()
                    .in_set(PlaySet::Ui)
                    .in_set(MenuSystems),
            )
            .add_systems(OnEnter(PlayState::Menu), release_cursor)
            .add_systems(OnExit(PlayState::Menu), close_menu);
        test_aid::setup(app);
    }
}

/// Test aid: F10 opens the nearest container (F9 is the game's Quick Load).
fn f9_opens_nearest_container(
    keys: Res<ButtonInput<KeyCode>>,
    here: Res<Location>,
    sim: Res<SimState>,
    cam: Query<&GlobalTransform, With<PlayerCamera>>,
    mut requests: MessageReader<OpenNearestContainer>,
    mut out: MessageWriter<OpenContainer>,
) {
    if !keys.just_pressed(KeyCode::F10) && requests.read().count() == 0 {
        return;
    }
    let Ok(cam) = cam.single() else { return };
    let pos = space::to_game(cam.translation());
    match model::nearest_container(&sim.world, sim.refs, here.interior.map(FormId), FormId(here.world), pos) {
        Some((id, dist)) => {
            let name = sim.base_of(id.0).and_then(|b| sim.name(b)).unwrap_or("?");
            info!("F10: nearest container {id} ({name}) at {dist:.0} units");
            out.write(OpenContainer { container: id.0 });
        }
        None => info!("F10: no container here"),
    }
}

/// Whether one of these menus is open.
pub fn menu_open(active: Res<ActiveMenu>) -> bool {
    active.0.is_some()
}

/// Whether none is (for keys other menus take, such as the pause menu's Esc).
pub fn no_menu(active: Res<ActiveMenu>) -> bool {
    active.0.is_none()
}

/// The barter menus scripts asked for (ShowBarterMenu), as [`OpenBarter`].
fn scripts_open_barter(script: Option<ResMut<crate::script::ScriptState>>, mut out: MessageWriter<OpenBarter>) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    for r in std::mem::take(&mut rt.game.barter_requests) {
        info!("menu: a script opens the barter menu with {} (discount {})", r.merchant, r.discount);
        out.write(OpenBarter { merchant: r.merchant.0, discount: r.discount });
    }
}

/// The player's Barter skill as scripts see it (`GetActorValue Barter`, within 0 to
/// 100), else [`DEFAULT_BARTER_SKILL`].
fn barter_skill(script: Option<&mut crate::script::ScriptState>, sim: &mut SimState) -> f32 {
    let Some(rt) = script.and_then(|s| s.rt.as_mut()) else { return DEFAULT_BARTER_SKILL };
    crate::script::with_world(rt, sim, |rt| fnv_game::functions::actor_value_capped(&mut rt.game, FormId::PLAYER_REF, fnv_game::actor_values::BARTER)) as f32
}

/// Open a menu on request while playing. The inventory stays shut while a script has
/// turned the Pip-Boy off [documented: GECK wiki DisablePlayerControls, PipboyFlag]; a
/// container still opens (a script's own `Activate` asks for it). The barter menu also
/// opens over a conversation.
#[allow(clippy::too_many_arguments)]
pub(crate) fn open_menus(
    mut containers: MessageReader<OpenContainer>,
    mut inventory: MessageReader<OpenInventory>,
    mut barters: MessageReader<OpenBarter>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut active: ResMut<ActiveMenu>,
    mut sim: ResMut<SimState>,
    script: Option<ResMut<crate::script::ScriptState>>,
    locks: Option<Res<crate::control::ControlLocks>>,
    stats: Option<ResMut<crate::control::LockStats>>,
    wait: Option<Res<crate::wait::WaitMenu>>,
) {
    let mut script = script.map(|s| s.into_inner());
    let container = containers.read().last().copied();
    let barter = barters.read().last().copied();
    let mut inv = inventory.read().count() > 0;
    if inv && locks.is_some_and(|l| l.pipboy) {
        info!("menu: the inventory stays shut, a script has disabled the player's controls (Pip-Boy)");
        if let Some(mut s) = stats {
            s.held[crate::control::group::PIPBOY] += 1;
        }
        inv = false;
    }
    if container.is_none() && !inv && barter.is_none() {
        return;
    }
    let talking = *state.get() == PlayState::Dialogue;
    let allowed = *state.get() == PlayState::Playing || (barter.is_some() && talking);
    if !allowed || active.0.is_some() || wait.is_some_and(|w| w.is_open()) {
        info!("menu request ignored: a menu is already open");
        if let Some(b) = barter {
            // Not left open for the scripts either.
            crate::script::engine_menu_closed(script.as_deref_mut(), Mode::Barter(FormId(b.merchant)));
        }
        return;
    }
    let opened = match (barter, container) {
        (Some(b), _) => {
            let merchant = FormId(b.merchant);
            let stock = sim.refs.get(merchant).and_then(|p| p.merchant_container).unwrap_or(merchant);
            let skill = barter_skill(script.as_deref_mut(), &mut sim);
            info!("menu: barter with {merchant}: stock in {stock}, Barter skill {skill}");
            MenuModel::open_barter(&mut sim.world, merchant, stock, model::CAPS, skill, b.discount)
        }
        (None, Some(c)) => MenuModel::open_container(&mut sim.world, FormId(c.container)),
        (None, None) => MenuModel::open_inventory(&mut sim.world),
    };
    match opened {
        Ok(m) => {
            info!("menu: open {:?} \"{}\"", m.mode, m.title);
            // Scripts run MenuMode 1008 / 1002 / 1053 from now on (layer5-play.md).
            crate::script::engine_menu_opened(script, m.mode);
            active.0 = Some(m);
            if !talking {
                next.set(PlayState::Menu);
            }
        }
        Err(e) => {
            warn!("menu: cannot open: {e}");
            if let Some(b) = barter {
                crate::script::engine_menu_closed(script, Mode::Barter(FormId(b.merchant)));
            }
        }
    }
}

/// A barter menu opened over a conversation stays a menu when the conversation ends
/// under it.
fn keep_menu_state(active: Res<ActiveMenu>, state: Res<State<PlayState>>, mut next: ResMut<NextState<PlayState>>) {
    if active.0.is_some() && *state.get() == PlayState::Playing {
        next.set(PlayState::Menu);
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

fn menu_keys(keys: Res<ButtonInput<KeyCode>>, active: Res<ActiveMenu>, mut out: MessageWriter<MenuInput>) {
    use KeyCode::*;
    let Some(m) = active.0.as_ref().filter(|m| m.mode != Mode::Inventory) else { return };
    let shift = keys.any_pressed([ShiftLeft, ShiftRight]);
    let mut send = |c| {
        out.write(MenuInput(c));
    };
    // The original's letters for the buttons (barter: A accept, X exit; container: R
    // take all, E exit).
    let barter = matches!(m.mode, Mode::Barter(_));
    let close: &[KeyCode] = if barter { &[Escape, Tab, KeyX] } else { &[Escape, Tab, KeyE] };
    if keys.any_just_pressed(close.iter().copied()) {
        send(Command::Close);
        return;
    }
    let (left, right) = if barter { (ArrowLeft, ArrowRight) } else { (KeyA, KeyD) };
    // The player's list is drawn at the left of the container and barter menus.
    let map = [
        ([ArrowUp, KeyW], Command::Up),
        ([ArrowDown, KeyS], Command::Down),
        ([PageUp, PageUp], Command::PageUp),
        ([PageDown, PageDown], Command::PageDown),
        ([ArrowLeft, left], Command::Focus(Side::Player)),
        ([ArrowRight, right], Command::Focus(Side::Container)),
        ([BracketLeft, BracketLeft], Command::Category(m.focus, -1)),
        ([BracketRight, BracketRight], Command::Category(m.focus, 1)),
    ];
    for (k, c) in map {
        if keys.any_just_pressed(k) {
            send(c);
        }
    }
    if keys.any_just_pressed([Enter, NumpadEnter, Space]) {
        send(if shift { Command::Stack } else { Command::One });
    }
    match m.mode {
        Mode::Barter(_) if keys.just_pressed(KeyA) => send(Command::Accept),
        Mode::Container(_) if keys.just_pressed(KeyR) => send(Command::TakeAll),
        _ => {}
    }
}

/// The pointer selects the row it comes onto (the list's highlight follows it
/// [documented: list_box.xml, the template's rows take the pointer]); a click acts on
/// it, Shift+click on its whole stack.
#[allow(clippy::too_many_arguments)]
fn menu_mouse(
    rows: Query<(&Interaction, &RowButton), Changed<Interaction>>,
    buttons: Query<(&Interaction, &CmdButton), Changed<Interaction>>,
    keys: Res<ButtonInput<KeyCode>>,
    active: Res<ActiveMenu>,
    mut moved: MessageReader<CursorMoved>,
    mut wheel: MessageReader<MouseWheel>,
    mut lines: Local<f32>,
    mut out: MessageWriter<MenuInput>,
) {
    let Some(m) = active.0.as_ref().filter(|m| m.mode != Mode::Inventory) else {
        wheel.clear();
        return;
    };
    let hover = true;
    let shift = keys.any_pressed([KeyCode::ShiftLeft, KeyCode::ShiftRight]);
    // Only a pointer that moves takes the selection: a redrawn row under a resting
    // pointer leaves the keys' choice alone.
    let pointer_moved = moved.read().count() > 0;
    for (i, r) in &rows {
        let on_it = m.focus == r.0 && m.selected[r.0 as usize] == r.1;
        match *i {
            Interaction::Hovered if hover && !on_it && pointer_moved => {
                out.write(MenuInput(Command::Hover(r.0, r.1)));
            }
            Interaction::Pressed if hover && on_it && shift => {
                out.write(MenuInput(Command::Stack));
            }
            Interaction::Pressed if hover && !on_it => {
                out.write(MenuInput(Command::Hover(r.0, r.1)));
                out.write(MenuInput(if shift { Command::Stack } else { Command::One }));
            }
            Interaction::Pressed => {
                out.write(MenuInput(Command::Select(r.0, r.1)));
            }
            _ => {}
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

#[allow(clippy::too_many_arguments)]
fn apply_input(
    mut input: MessageReader<MenuInput>,
    mut active: ResMut<ActiveMenu>,
    mut sim: ResMut<SimState>,
    state: Res<State<PlayState>>,
    mut next: ResMut<NextState<PlayState>>,
    mut changed: MessageWriter<RefChanged>,
    mut used: MessageWriter<ItemUsed>,
    script: Option<ResMut<crate::script::ScriptState>>,
    mut sounds: MessageWriter<crate::ui_sounds::UiSound>,
) {
    let mut script = script.map(|s| s.into_inner());
    for &MenuInput(cmd) in input.read() {
        let Some(m) = active.0.as_mut() else { return };
        let before = crate::ui_sounds::MenuBefore::of(m);
        let a = m.apply(&mut sim.world, cmd);
        // What the command did, for its sound (`crate::ui_sounds`).
        if let Some(event) = crate::ui_sounds::menu_event(cmd, &before, m, &a) {
            sounds.write(crate::ui_sounds::UiSound { menu: crate::ui_sounds::UiMenu::of(m.mode), event });
        }
        let moves_items = matches!(cmd, Command::One | Command::Stack | Command::TakeAll | Command::Drop | Command::DropStack | Command::Accept);
        if moves_items || a.changed {
            if let Some(s) = m.status.as_deref().filter(|s| !s.is_empty()) {
                info!("menu: {cmd:?}: {s}");
            }
        }
        if let Some(item) = a.used {
            used.write(ItemUsed { item: item.0 });
        }
        // A dropped item starts turned as its holder faces (GetAngle Z of the new reference
        // equals the player's, X and Y 0 [observed: capture pc_drop_fall, physics.md
        // §4.3]); `World::drop_item` leaves the rotation 0.
        let heading = sim.world.peek(FormId::PLAYER_REF).map_or(0.0, |p| p.rot[2]);
        for id in &a.created {
            if let Ok(s) = sim.world.get(*id) {
                s.rot = [0.0, 0.0, heading];
            }
            let (at, count) = sim.world.peek(*id).map(|s| (s.pos, s.count)).unwrap_or_default();
            info!("menu: dropped reference {id} (count {count}) at ({:.1}, {:.1}, {:.1})", at[0], at[1], at[2]);
            changed.write(RefChanged { form_id: id.0 });
        }
        if a.close {
            info!("menu: close");
            if *state.get() == PlayState::Menu {
                next.set(PlayState::Playing);
            } else if let Some(m) = active.0.take() {
                // Over a conversation: it goes on (the drawing goes with the model).
                crate::script::engine_menu_closed(script.as_deref_mut(), m.mode);
            }
            return;
        }
    }
}

// ---- drawing -----------------------------------------------------------------------

/// The line under a container menu: the note for a container that refills.
fn note(m: &MenuModel, sim: &SimState) -> Option<String> {
    let refused = m.status.as_deref().filter(|s| s.ends_with("enough caps.") || s.starts_with("Cannot"));
    if let Some(s) = refused {
        return Some(s.to_string());
    }
    let Mode::Container(c) = m.mode else { return None };
    let base = sim.world.peek(c).map(|s| s.base)?;
    matches!(sim.world.forms.get(base).map(|b| &b.data), Some(BaseData::Container(k)) if k.respawns).then(|| RESPAWN_NOTE.to_string())
}

/// Rebuild the container or barter menu's UI when the model changed (the inventory is
/// the Pip-Boy's).
#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    active: Res<ActiveMenu>,
    roots: Query<Entity, With<MenuRoot>>,
    sim: Res<SimState>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    mut kit: Local<Option<look::Kit>>,
) {
    if !active.is_changed() && !roots.is_empty() {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    let Some(m) = active.0.as_ref().filter(|m| m.mode != Mode::Inventory) else { return };
    let bc = crate::interface::bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| look::Kit::load(data.as_deref(), &mut images, bc));
    let note = note(m, &sim);
    let screen = Node {
        position_type: PositionType::Absolute,
        width: Val::Percent(100.0),
        height: Val::Percent(100.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };
    commands.spawn((MenuRoot, screen, GlobalZIndex(100))).with_children(|root| {
        look::spawn(root, m, note.as_deref(), kit, data.as_deref(), &mut images, bc);
    });
}
