//! Script messages on screen: a minimal overlay (layout and wording are NVForever's own,
//! not a claim about the original's interface).
//!
//! - **Lines** (messages without the message-box flag, objectives shown or completed,
//!   quests completed, items a script gave or took): bottom left (nvforever's status text
//!   is top left), the newest last, each for [`NOTE_SECONDS`].
//! - **Message boxes**: the runtime's queue (`fnv_game::Game::boxes`, scripts.md §7.7),
//!   the first one in the middle of the view while the open menu is 1001, with numbered
//!   buttons. Number keys 1–9 or a click pick a button (`ScriptState` →
//!   `Runtime::answer`, which ends in `Game::close_menu` after the frame's MenuMode 1001
//!   ran, so a `MenuMode 1001` block still reads `GetButtonPressed`); a box without
//!   buttons closes with 1, Enter, Space or a click on OK. The buttons are clickable as
//!   the original's are [observed by the coordinator playing the original, 2026-10-09];
//!   the cursor is free while a box is open (`PlayState::Menu`). The next queued box then
//!   opens (the runtime's rule).
//! - **Menus scripts open** for character creation (name 1051, race 1036, S.P.E.C.I.A.L.
//!   1074, tag skills 1048, traits 1084): a placeholder panel that names the menu; Enter
//!   or a click on Done closes it as the player's Done would (`Game::close_menu`). What the player chooses
//!   there is Layer 7; nothing is chosen here.
//! - While a script's menu or box is open the player cannot move or look
//!   (`PlayState::Menu`, `script::hold_player`), and script time stops (the runtime's
//!   `Game::menu`).
//! - **Subtitles**: the dialogue lines being said (`Game::speech`), bottom centre, as
//!   "Speaker: text", one response of the INFO at a time in response-number order, each
//!   for as long as it lasts (`crate::voice::speech_response`: its voice file's length,
//!   or the length fitted to its text without one).

use crate::script::{ScriptState, MENU_MESSAGE};
use crate::script_events::BoxMessage;
use crate::sim::SimState;
use bevy::prelude::*;
use fnv_game::game::menus;
use fnv_sim::FormId;
use std::collections::{HashMap, VecDeque};

/// Seconds a line stays on screen.
pub const NOTE_SECONDS: f32 = 6.0;
/// Lines on screen at once (older ones go first).
const MAX_NOTES: usize = 6;

#[derive(Resource, Default)]
pub struct ScriptHud {
    notes: VecDeque<(String, f32)>,
    /// The text and buttons each message was last shown with (the trace's
    /// `message_shown`), for the box queue, which holds only FormIDs.
    shown: HashMap<u32, BoxMessage>,
    /// The response texts (NAM1) of the dialogue lines said, by INFO.
    lines: HashMap<u32, Vec<String>>,
    /// The message box on screen (as last drawn).
    on_screen: Option<BoxMessage>,
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

    /// A message box was shown: remember its text for when it reaches the front.
    pub fn message_box(&mut self, b: BoxMessage) {
        info!("message box shown: {} {:?}", b.text.replace('\n', " / "), b.buttons);
        self.shown.insert(b.message, b);
    }

    /// The text of a message box as shown, else as the message record has it.
    pub fn box_view(&self, message: FormId, script: &ScriptState) -> BoxMessage {
        if let Some(b) = self.shown.get(&message.0) {
            return b.clone();
        }
        let def = script.rt.as_ref().and_then(|rt| rt.game.data.messages.get(&message));
        BoxMessage {
            message: message.0,
            text: def.and_then(|d| d.text.clone()).unwrap_or_else(|| format!("(message {message})")),
            buttons: def.map(|d| d.buttons.clone()).unwrap_or_default(),
        }
    }

    /// The message box on screen, if any.
    pub fn front(&self) -> Option<&BoxMessage> {
        self.on_screen.as_ref()
    }

    pub fn knows_line(&self, info: FormId) -> bool {
        self.lines.contains_key(&info.0)
    }

    pub fn remember_line(&mut self, info: FormId, responses: Vec<String>) {
        self.lines.insert(info.0, responses);
    }
}

/// A character-creation menu's name for the placeholder panel.
pub fn menu_name(code: u16) -> String {
    match code {
        menus::NAME => "Name".into(),
        menus::RACE => "Race and appearance".into(),
        menus::LOVE_TESTER => "S.P.E.C.I.A.L. (Vit-o-matic Vigor Tester)".into(),
        menus::TAG_SKILLS => "Tag skills".into(),
        menus::TRAITS => "Traits".into(),
        c => format!("Menu {c}"),
    }
}

/// The placeholder panel of a menu a script opened: its text and its one button.
pub fn menu_panel(code: u16) -> (String, Vec<String>) {
    (
        format!("{} menu (menu {code})\n\nCharacter creation is not built yet (Layer 7); nothing is chosen here.", menu_name(code)),
        vec!["Done (Enter)".into()],
    )
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

#[derive(Component)]
struct SubtitleText;

/// The row of the front box's buttons.
#[derive(Component)]
struct BoxButtons;

/// A clickable button of the panel (0-based).
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
            bottom: Val::Px(110.0),
            justify_content: JustifyContent::Center,
            ..default()
        },
        children![(
            Node { max_width: Val::Px(760.0), padding: UiRect::axes(Val::Px(10.0), Val::Px(4.0)), ..default() },
            children![(Text::new(""), font(18.0), TextColor(Color::srgb(0.95, 0.95, 0.9)), TextLayout::justify(Justify::Center), SubtitleText)],
        )],
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

/// What the panel shows: text and button labels.
type Panel = (String, Vec<String>);

/// What the overlay shows this frame: the panel (a message box or a menu placeholder),
/// the box on screen, and the subtitles.
fn view(hud: &ScriptHud, script: &ScriptState, sim: &SimState) -> (Option<Panel>, Option<BoxMessage>, String) {
    let Some(rt) = script.rt.as_ref() else { return (None, None, String::new()) };
    let g = &rt.game;
    let front = match g.menu {
        Some(MENU_MESSAGE) => g.boxes.first().map(|&m| hud.box_view(m, script)),
        _ => None,
    };
    let panel = match g.menu {
        Some(MENU_MESSAGE) => front.as_ref().map(|b| (b.text.trim_end().to_string(), box_buttons(b))),
        Some(code) if crate::script::is_script_menu(code) => Some(menu_panel(code)),
        _ => None,
    };
    let mut subs = Vec::new();
    for s in &g.speech {
        let Some(texts) = hud.lines.get(&s.info.0) else { continue };
        let i = crate::voice::speech_response(g, sim, s);
        let Some(text) = texts.get(i).or_else(|| texts.last()) else { continue };
        let base = sim.base_of(s.speaker.0).unwrap_or(s.speaker);
        let who = sim.name(base).map(str::to_string).unwrap_or_else(|| g.data.name(s.speaker));
        subs.push(format!("{who}: {}", text.trim()));
    }
    (panel, front, subs.join("\n"))
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    mut commands: Commands,
    rows: Query<Entity, With<BoxButtons>>,
    time: Res<Time>,
    mut hud: ResMut<ScriptHud>,
    script: Res<ScriptState>,
    sim: Res<SimState>,
    mut shown: Local<(Option<Panel>, String)>,
    mut notes: Query<&mut Text, (With<NotesText>, Without<BoxText>, Without<SubtitleText>)>,
    mut panel: Query<&mut Visibility, With<BoxPanel>>,
    mut boxed: Query<&mut Text, (With<BoxText>, Without<NotesText>, Without<SubtitleText>)>,
    mut subtitles: Query<&mut Text, (With<SubtitleText>, Without<NotesText>, Without<BoxText>)>,
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
    if std::mem::take(&mut hud.dirty) {
        let text: Vec<&str> = hud.notes.iter().map(|n| n.0.as_str()).collect();
        for mut t in &mut notes {
            t.0 = text.join("\n");
        }
    }
    let (p, front, subs) = view(&hud, &script, &sim);
    hud.on_screen = front;
    let now = (p, subs);
    if now == *shown {
        return;
    }
    for mut v in &mut panel {
        v.set_if_neq(if now.0.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    if now.0 != shown.0 {
        for row in &rows {
            commands.entity(row).despawn_related::<Children>();
        }
    }
    if let Some((text, buttons)) = now.0.as_ref().filter(|_| now.0 != shown.0) {
        for mut t in &mut boxed {
            t.0.clone_from(text);
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
    if now.1 != shown.1 {
        if !now.1.is_empty() {
            info!("subtitle: {}", now.1.replace('\n', " / "));
        }
        for mut t in &mut subtitles {
            t.0.clone_from(&now.1);
        }
    }
    *shown = now;
}

/// Keys and clicks for a script's open menu (before the frame's scripts run): number keys
/// or a click pick a button of the front message box; 1, Enter, Space or a click on OK
/// close a box without buttons; Enter or a click on Done closes a character-creation
/// menu. All go through `ScriptState::answer`.
pub fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Query<(&Interaction, &BoxButton), Changed<Interaction>>,
    hud: Res<ScriptHud>,
    mut script: ResMut<ScriptState>,
) {
    use KeyCode::*;
    let Some(code) = script.rt.as_ref().and_then(|rt| rt.game.menu) else { return };
    if !crate::script::is_script_menu(code) {
        return;
    }
    let clicked = clicks.iter().find(|(i, _)| **i == Interaction::Pressed).map(|(_, b)| b.0);
    let enter = keys.any_just_pressed([Enter, NumpadEnter]);
    if code != MENU_MESSAGE {
        if enter || clicked.is_some() {
            script.answer(None, if enter { "Enter" } else { "click" });
        }
        return;
    }
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let pads = [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    let picked = (0..9).find(|&i| keys.just_pressed(digits[i]) || keys.just_pressed(pads[i])).or(clicked);
    let Some(front) = script.rt.as_ref().and_then(|rt| rt.game.boxes.first().copied()) else { return };
    let buttons = hud.box_view(front, &script).buttons.len();
    if buttons == 0 {
        if picked == Some(0) || enter || keys.just_pressed(Space) {
            script.answer(None, "OK");
        }
    } else if let Some(i) = picked.filter(|&i| i < buttons) {
        script.answer(Some(i as i32), "key");
    }
}
