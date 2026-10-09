//! Script messages on screen: a minimal overlay (layout and wording are NVForever's own,
//! not a claim about the original's interface).
//!
//! - Lines (messages without the message-box flag, objectives shown or completed, quests
//!   completed, items a script gave or took): bottom left (nvforever's status text is top
//!   left), the newest last, each for
//!   [`NOTE_SECONDS`].
//! - Message boxes (`MESG` flagged "Message Box", or with buttons): one at a time in the
//!   middle of the view, with numbered buttons. Number keys 1–9 pick a button
//!   (`GetButtonPressed` returns it, 0-based); a box without buttons closes with 1, Enter
//!   or Space. The buttons can also be clicked: the cursor is released while a box is on
//!   screen (`crate::player`), as the original's buttons are clickable [observed by the
//!   coordinator playing the original, 2026-10-09]. While a box with buttons waits, scripts run in MenuMode 1001
//!   (`script::MENU_MESSAGE`); the player is not stopped [decision: the original pauses
//!   the game under a message box; nvf-play has no pause state outside its menus yet].

use crate::script::ScriptState;
use crate::script_events::BoxMessage;
use bevy::prelude::*;
use std::collections::VecDeque;

/// Seconds a line stays on screen.
pub const NOTE_SECONDS: f32 = 6.0;
/// Lines on screen at once (older ones go first).
const MAX_NOTES: usize = 6;

#[derive(Resource, Default)]
pub struct ScriptHud {
    notes: VecDeque<(String, f32)>,
    boxes: VecDeque<BoxMessage>,
    dirty: bool,
}

impl ScriptHud {
    pub fn notify(&mut self, text: String) {
        let text = text.trim().to_string();
        info!("on screen: {}", text.replace('\n', " / "));
        self.notes.push_back((text, 0.0));
        while self.notes.len() > MAX_NOTES {
            self.notes.pop_front();
        }
        self.dirty = true;
    }

    /// Queue a box. A new box with buttons replaces older ones with buttons: the runtime
    /// waits for one box at a time (`fnv_game::game::MessageBox`).
    pub fn message_box(&mut self, b: BoxMessage) {
        info!("on screen (box): {} {:?}", b.text.replace('\n', " / "), b.buttons);
        if b.waits() {
            self.boxes.retain(|o| !o.waits());
        }
        self.boxes.push_back(b);
        self.dirty = true;
    }

    /// Close the front box if it has no buttons.
    pub fn dismiss_front(&mut self) -> bool {
        if self.boxes.front().is_some_and(|b| !b.waits()) {
            self.boxes.pop_front();
            self.dirty = true;
            return true;
        }
        false
    }

    /// Whether a box waiting for a button is queued (it may be behind others).
    pub fn has_waiting(&self) -> bool {
        self.boxes.iter().any(BoxMessage::waits)
    }

    /// The waiting box got its button.
    pub fn pop_waiting(&mut self) {
        if let Some(i) = self.boxes.iter().position(BoxMessage::waits) {
            self.boxes.remove(i);
            self.dirty = true;
        }
    }

    pub fn front(&self) -> Option<&BoxMessage> {
        self.boxes.front()
    }
}

pub struct ScriptHudPlugin;

impl Plugin for ScriptHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptHud>().add_systems(Startup, spawn).add_systems(Update, draw.in_set(crate::PlaySet::Ui));
    }
}

#[derive(Component)]
struct NotesText;

#[derive(Component)]
struct BoxPanel;

#[derive(Component)]
struct BoxText;

/// The row of the front box's buttons.
#[derive(Component)]
struct BoxButtons;

/// A clickable button of the front box (0-based).
#[derive(Component, Clone, Copy, Debug)]
pub struct BoxButton(pub usize);

fn spawn(mut commands: Commands) {
    let font = |size: f32| TextFont { font_size: FontSize::Px(size), ..default() };
    commands.spawn((
        Node { position_type: PositionType::Absolute, left: Val::Px(16.0), bottom: Val::Px(48.0), max_width: Val::Percent(45.0), ..default() },
        children![(Text::new(""), font(15.0), TextColor(Color::srgb(0.95, 0.9, 0.7)), NotesText)],
    ));
    commands.spawn((
        Node {
            position_type: PositionType::Absolute,
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        },
        BoxPanel,
        Visibility::Hidden,
        children![(
            Node { max_width: Val::Px(560.0), padding: UiRect::all(Val::Px(16.0)), margin: UiRect::all(Val::Px(16.0)), ..default() },
            BackgroundColor(Color::srgba(0.05, 0.05, 0.03, 0.88)),
            children![(
                Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(12.0), ..default() },
                children![
                    (Text::new(""), font(16.0), TextColor(Color::srgb(1.0, 0.85, 0.4)), BoxText),
                    (Node { flex_direction: FlexDirection::Column, row_gap: Val::Px(6.0), ..default() }, BoxButtons),
                ],
            )],
        )],
    ));
}

/// The box's numbered buttons ("1) OK" for a box without buttons).
pub fn box_buttons(b: &BoxMessage) -> Vec<String> {
    if b.buttons.is_empty() {
        vec!["1) OK".into()]
    } else {
        b.buttons.iter().enumerate().map(|(i, t)| format!("{}) {t}", i + 1)).collect()
    }
}

/// The box as text: its message, then its numbered buttons.
pub fn box_lines(b: &BoxMessage) -> String {
    format!("{}\n\n{}", b.text.trim_end(), box_buttons(b).join("\n"))
}

#[allow(clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    time: Res<Time>,
    rows: Query<Entity, With<BoxButtons>>,
    mut hud: ResMut<ScriptHud>,
    mut notes: Query<&mut Text, (With<NotesText>, Without<BoxText>)>,
    mut panel: Query<&mut Visibility, With<BoxPanel>>,
    mut boxed: Query<&mut Text, (With<BoxText>, Without<NotesText>)>,
) {
    let dt = time.delta_secs();
    let before = hud.notes.len();
    for n in hud.notes.iter_mut() {
        n.1 += dt;
    }
    hud.notes.retain(|n| n.1 < NOTE_SECONDS);
    if hud.notes.len() != before {
        hud.dirty = true;
    }
    if !hud.dirty {
        return;
    }
    hud.dirty = false;
    let text: Vec<&str> = hud.notes.iter().map(|n| n.0.as_str()).collect();
    for mut t in &mut notes {
        t.0 = text.join("
");
    }
    let front = hud.front().map(|b| (b.text.trim_end().to_string(), box_buttons(b)));
    for mut v in &mut panel {
        v.set_if_neq(if front.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    for row in &rows {
        commands.entity(row).despawn_related::<Children>();
    }
    if let Some((text, buttons)) = front {
        for mut t in &mut boxed {
            t.0.clone_from(&text);
        }
        for row in &rows {
            for (i, label) in buttons.iter().enumerate() {
                commands.entity(row).with_child((
                    Button,
                    BoxButton(i),
                    Node { padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() },
                    BackgroundColor(Color::srgba(0.20, 0.18, 0.08, 0.9)),
                    children![(Text::new(label.clone()), TextFont { font_size: FontSize::Px(16.0), ..default() }, TextColor(Color::srgb(1.0, 0.85, 0.4)))],
                ));
            }
        }
    }
}

/// Number keys or a click pick a button of the front box (before the frame's scripts run).
pub fn box_keys(
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Query<(&Interaction, &BoxButton), Changed<Interaction>>,
    mut hud: ResMut<ScriptHud>,
    mut script: ResMut<ScriptState>,
) {
    use KeyCode::*;
    let Some(front) = hud.front() else { return };
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let pads = [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    let clicked = clicks.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| b.0);
    let picked = (0..9).find(|&i| keys.just_pressed(digits[i]) || keys.just_pressed(pads[i])).or(clicked);
    if !front.waits() {
        if picked == Some(0) || keys.any_just_pressed([Enter, NumpadEnter, Space]) {
            hud.dismiss_front();
        }
        return;
    }
    let Some(i) = picked else { return };
    if i < front.buttons.len() {
        // A box the runtime no longer waits for (another ShowMessage replaced it) closes
        // all the same.
        if !script.press(i as i32) {
            warn!("message box: the scripts no longer wait for button {}", i + 1);
        }
        hud.pop_waiting();
    }
}
