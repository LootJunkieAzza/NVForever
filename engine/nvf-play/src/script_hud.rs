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
//!   or Space. While a box with buttons waits, scripts run in MenuMode 1001
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
            children![(Text::new(""), font(16.0), TextColor(Color::srgb(1.0, 0.85, 0.4)), BoxText)],
        )],
    ));
}

/// The box as text: its message, then its numbered buttons.
pub fn box_lines(b: &BoxMessage) -> String {
    let mut s = b.text.trim_end().to_string();
    s.push_str("\n\n");
    if b.buttons.is_empty() {
        s.push_str("1) OK");
    } else {
        let buttons: Vec<String> = b.buttons.iter().enumerate().map(|(i, t)| format!("{}) {t}", i + 1)).collect();
        s.push_str(&buttons.join("\n"));
    }
    s
}

fn draw(
    time: Res<Time>,
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
    let front = hud.front().map(box_lines);
    for mut v in &mut panel {
        v.set_if_neq(if front.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Some(f) = front {
        for mut t in &mut boxed {
            t.0.clone_from(&f);
        }
    }
}

/// Number keys pick a button of the front box (before the frame's scripts run).
pub fn box_keys(keys: Res<ButtonInput<KeyCode>>, mut hud: ResMut<ScriptHud>, mut script: ResMut<ScriptState>) {
    use KeyCode::*;
    let Some(front) = hud.front() else { return };
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let pads = [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    let picked = (0..9).find(|&i| keys.just_pressed(digits[i]) || keys.just_pressed(pads[i]));
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
