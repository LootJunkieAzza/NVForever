//! Script messages on screen: a minimal overlay (layout and wording are NVForever's own,
//! not a claim about the original's interface).
//!
//! - **Lines** (messages without the message-box flag, objectives shown or completed,
//!   quests completed, items a script gave or took): bottom left (nvforever's status text
//!   is top left), the newest last, each for [`NOTE_SECONDS`].
//! - **Message boxes**: the runtime's queue (`fnv_game::Game::boxes`, scripts.md §7.7),
//!   the first one in the middle of the view while the open menu is 1001, with numbered
//!   buttons. Number keys 1–9 pick a button (`ScriptState` → `Runtime::answer`, which
//!   ends in `Game::close_menu` after the frame's MenuMode 1001 ran, so a `MenuMode 1001`
//!   block still reads `GetButtonPressed`); a box without buttons closes with 1, Enter or
//!   Space (OK). The next queued box then opens (the runtime's rule).
//! - **Menus scripts open** for character creation (name 1051, race 1036, S.P.E.C.I.A.L.
//!   1074, tag skills 1048, traits 1084): a placeholder panel that names the menu; Enter
//!   closes it as the player's Done would (`Game::close_menu`). What the player chooses
//!   there is Layer 7; nothing is chosen here.
//! - While a script's menu or box is open the player cannot move or look
//!   (`PlayState::Menu`, `script::hold_player`), and script time stops (the runtime's
//!   `Game::menu`).
//! - **Subtitles**: the dialogue lines being said (`Game::speech`), bottom centre, as
//!   "Speaker: text", one response of the INFO at a time, each for its share of the line's
//!   length (`fnv_game::game::line_seconds` per response, the runtime's stand-in for the
//!   voice file).

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

/// The placeholder panel of a menu a script opened.
pub fn menu_lines(code: u16) -> String {
    format!("{} menu (menu {code})\n\nCharacter creation is not built yet (Layer 7); nothing is chosen here.\n\nEnter) Done", menu_name(code))
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

/// Which response of a line is being said, `elapsed` seconds into it: each response
/// takes its share of the line (`fnv_game::game::line_seconds` of its length).
pub fn response_at(chars: &[usize], elapsed: f64) -> usize {
    let mut end = 0.0;
    for (i, &c) in chars.iter().enumerate() {
        end += fnv_game::game::line_seconds(&[c]);
        if elapsed < end {
            return i;
        }
    }
    chars.len().saturating_sub(1)
}

/// What the overlay shows this frame: the panel (a message box or a menu placeholder)
/// and the subtitles.
fn view(hud: &ScriptHud, script: &ScriptState, sim: &SimState) -> (Option<String>, String) {
    let Some(rt) = script.rt.as_ref() else { return (None, String::new()) };
    let g = &rt.game;
    let panel = match g.menu {
        Some(MENU_MESSAGE) => g.boxes.first().map(|&m| box_lines(&hud.box_view(m, script))),
        Some(code) if crate::script::is_script_menu(code) => Some(menu_lines(code)),
        _ => None,
    };
    let mut subs = Vec::new();
    for s in &g.speech {
        let Some(texts) = hud.lines.get(&s.info.0) else { continue };
        let chars: Vec<usize> = g.data.infos.get(&s.info).map(|d| d.response_chars.clone()).unwrap_or_default();
        let total = fnv_game::game::line_seconds(&chars);
        let i = response_at(&chars, total - s.remaining);
        let Some(text) = texts.get(i).or_else(|| texts.last()) else { continue };
        let base = sim.base_of(s.speaker.0).unwrap_or(s.speaker);
        let who = sim.name(base).map(str::to_string).unwrap_or_else(|| g.data.name(s.speaker));
        subs.push(format!("{who}: {}", text.trim()));
    }
    (panel, subs.join("\n"))
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    time: Res<Time>,
    mut hud: ResMut<ScriptHud>,
    script: Res<ScriptState>,
    sim: Res<SimState>,
    mut shown: Local<(Option<String>, String)>,
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
    let now = view(&hud, &script, &sim);
    if now == *shown {
        return;
    }
    for mut v in &mut panel {
        v.set_if_neq(if now.0.is_some() { Visibility::Inherited } else { Visibility::Hidden });
    }
    if let Some(f) = &now.0 {
        for mut t in &mut boxed {
            t.0.clone_from(f);
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

/// Keys for a script's open menu (before the frame's scripts run): number keys pick a
/// button of the front message box; 1, Enter or Space close a box without buttons;
/// Enter closes a character-creation menu.
pub fn menu_keys(keys: Res<ButtonInput<KeyCode>>, hud: Res<ScriptHud>, mut script: ResMut<ScriptState>) {
    use KeyCode::*;
    let Some(code) = script.rt.as_ref().and_then(|rt| rt.game.menu) else { return };
    if !crate::script::is_script_menu(code) {
        return;
    }
    let enter = keys.any_just_pressed([Enter, NumpadEnter]);
    if code != MENU_MESSAGE {
        if enter {
            script.answer(None, "Enter");
        }
        return;
    }
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let pads = [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    let picked = (0..9).find(|&i| keys.just_pressed(digits[i]) || keys.just_pressed(pads[i]));
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
