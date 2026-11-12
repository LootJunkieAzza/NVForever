//! ShowQuantityMenu's menu (`fnv_game::nvse_jip12`, menu 1016): the item menus' "How
//! many?" prompt (`menus\quantity_menu.xml`, drawn by `menu::look::spawn_quantity`) on its
//! own, from 1 up to the script's most, starting at its initial amount [documented:
//! ShowQuantityMenu]. The keys are the item menus' (`crate::menu`, `crate::crafting`): the
//! right arrow or D one more, the left one less, Page Up and Page Down ten, Enter or A Ok,
//! Escape or E Cancel; the buttons and the meter take the pointer. Ok gives the script's
//! callback the amount, Cancel -1.

use crate::interface::bc_supported;
use crate::menu::model::{amount_at, Command, Quantity, Side, QUANTITY_JUMP};
use crate::script::ScriptState;
use crate::sim::SimState;
use crate::ui_pointer::PointerState;
use crate::PlaySet;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_game::game::menus;
use nvf_assets::GameData;

/// Whether this module draws a script's menu (`crate::script_hud` leaves it).
pub fn draws(code: u16) -> bool {
    code == menus::QUANTITY
}

pub struct ScriptQuantityPlugin;

impl Plugin for ScriptQuantityPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptQuantity>().add_systems(Update, (keys, draw).chain().in_set(PlaySet::Ui));
    }
}

/// The amount chosen and the most, while the menu is open; what was drawn.
#[derive(Resource, Default)]
pub struct ScriptQuantity {
    open: Option<(i32, i32)>,
    drawn: Option<(i32, i32)>,
}

#[derive(Component)]
struct QuantityRoot;

type AmountButton = (&'static PointerState, &'static crate::menu::CmdButton);
type AmountPress = (&'static PointerState, &'static bevy::ui::RelativeCursorPosition);

#[allow(clippy::too_many_arguments)]
fn keys(
    keys: Res<ButtonInput<KeyCode>>,
    mut st: ResMut<ScriptQuantity>,
    script: Option<ResMut<ScriptState>>,
    mut sim: ResMut<SimState>,
    (buttons, meter): (Query<AmountButton, Changed<PointerState>>, Query<AmountPress, With<crate::menu::AmountMeter>>),
) {
    let Some(rt) = script.and_then(|s| s.into_inner().rt.as_mut()) else { return };
    let asked = rt.game.quantity_ask().filter(|_| rt.game.menu == Some(menus::QUANTITY)).map(|a| (a.initial, a.max));
    let Some((initial, most)) = asked else {
        st.open = None;
        return;
    };
    let (amount, _) = *st.open.get_or_insert_with(|| {
        info!("script: quantity menu open, {initial} of up to {most}");
        (initial, most)
    });
    let mut to = amount;
    let (mut ok, mut cancel) = (false, false);
    for (i, b) in &buttons {
        if *i == PointerState::Pressed {
            match b.0 {
                Command::AmountStep(d) => to += d,
                Command::AmountOk => ok = true,
                Command::AmountCancel => cancel = true,
                _ => {}
            }
        }
    }
    for (i, at) in &meter {
        if let (PointerState::Pressed, Some(p)) = (i, at.normalized) {
            to = amount_at(p.x + 0.5, most);
        }
    }
    if keys.any_just_pressed([KeyCode::ArrowRight, KeyCode::KeyD]) {
        to += 1;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) {
        to -= 1;
    }
    if keys.just_pressed(KeyCode::PageUp) {
        to += QUANTITY_JUMP;
    }
    if keys.just_pressed(KeyCode::PageDown) {
        to -= QUANTITY_JUMP;
    }
    let to = to.clamp(1, most.max(1));
    st.open = Some((to, most));
    let done = if keys.any_just_pressed([KeyCode::KeyE, KeyCode::Escape]) || cancel {
        Some(None)
    } else if keys.any_just_pressed([KeyCode::KeyA, KeyCode::Enter, KeyCode::NumpadEnter]) || ok {
        Some(Some(to))
    } else {
        None
    };
    if let Some(amount) = done {
        info!("script: quantity menu {}", amount.map_or("cancelled".to_string(), |n| format!("done, {n}")));
        crate::script::with_world(rt, &mut sim, |rt| rt.game.finish_quantity(amount));
        st.open = None;
    }
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    mut st: ResMut<ScriptQuantity>,
    script: Option<Res<ScriptState>>,
    roots: Query<Entity, With<QuantityRoot>>,
    data: Option<Res<GameData>>,
    mut images: ResMut<Assets<Image>>,
    formats: Option<Res<CompressedImageFormatSupport>>,
    glyphs: Res<crate::pad::PadGlyphs>,
    mut kit: Local<Option<crate::menu::look::Kit>>,
) {
    if st.drawn == st.open && (st.open.is_none() || !roots.is_empty()) {
        return;
    }
    for e in &roots {
        commands.entity(e).despawn();
    }
    st.drawn = st.open;
    let Some((amount, most)) = st.open else { return };
    let bc = bc_supported(formats.as_deref());
    let kit = kit.get_or_insert_with(|| crate::menu::look::Kit::load(data.as_deref(), &mut images, bc));
    let words = crate::menu::look::quantity_words(script.as_deref().and_then(|s| s.rt.as_ref()).map(|rt| &rt.game));
    let q = Quantity { cmd: Command::One, side: Side::Player, row: 0, max: most, amount };
    let screen = Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), height: Val::Percent(100.0), justify_content: JustifyContent::Center, align_items: AlignItems::Center, ..default() };
    commands.spawn((QuantityRoot, screen, GlobalZIndex(crate::tiles::draw::QUANTITY_Z))).with_children(|s| crate::menu::look::spawn_quantity(s, &q, &words, kit, data.as_deref(), &mut images, bc, &glyphs, None));
}
