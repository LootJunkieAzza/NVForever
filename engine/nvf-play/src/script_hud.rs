//! Script messages on screen. The lines and subtitles are a minimal overlay (layout and
//! wording are NVForever's own, not a claim about the original's interface); the message
//! box is drawn as the original's (`crate::message_menu`).
//!
//! - **Lines** (messages without the message-box flag, objectives shown or completed,
//!   quests completed, items a script gave or took): bottom left (nvforever's status text
//!   is top left), the newest last, each for [`NOTE_SECONDS`].
//! - **Message boxes**: the runtime's queue (`fnv_game::Game::boxes`, scripts.md §7.7),
//!   the first one in the middle of the view while the open menu is 1001, with the
//!   message's title and its buttons one under the other, the chosen one framed. A click
//!   picks a button; Up and Down (or the pointer) move the frame and Enter picks the
//!   framed one; number keys 1–9 pick a button by its place [design: the keys]. All end
//!   in `ScriptState` → `Runtime::answer`, which ends in `Game::close_menu` after the
//!   frame's MenuMode 1001 ran, so a `MenuMode 1001` block still reads
//!   `GetButtonPressed`. A box without buttons has an OK button, which Space also
//!   presses. The buttons are clickable as the original's are [observed by the
//!   coordinator playing the original, 2026-10-09]; the cursor is free while a box is
//!   open (`PlayState::Menu`). The next queued box then opens (the runtime's rule).
//! - **Menus scripts open** for character creation (name 1051, race 1036, S.P.E.C.I.A.L.
//!   1074, tag skills 1048, traits 1084): a placeholder in the same box that names the
//!   menu; Enter or a click on Done closes it as the player's Done would
//!   (`Game::close_menu`). What the player chooses there is Layer 7; nothing is chosen
//!   here.
//! - While a script's menu or box is open the player cannot move or look
//!   (`PlayState::Menu`, `script::hold_player`), and script time stops (the runtime's
//!   `Game::menu`).
//! - **Subtitles**: the dialogue lines being said (`Game::speech`), bottom centre, as
//!   "Speaker: text", one response of the INFO at a time in response-number order, each
//!   for as long as it lasts (`crate::voice::speech_response`: its voice file's length,
//!   or the length fitted to its text without one); and the response being said in the
//!   conversation with the player (`Game::dialogue`, `crate::dialogue::subtitle`), for as
//!   long as the runtime says it.

use crate::message_menu::{MessagePanel, PanelView};
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

/// The placeholder panel of a menu a script opened: its name, a text and its one button.
pub fn menu_panel(code: u16) -> PanelView {
    PanelView {
        title: Some(menu_name(code)),
        text: format!("Menu {code}.\n\nCharacter creation is not built yet (Layer 7); nothing is chosen here."),
        buttons: vec!["Done".into()],
    }
}

pub struct ScriptHudPlugin;

impl Plugin for ScriptHudPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ScriptHud>()
            .add_plugins(crate::message_menu::MessageMenuPlugin)
            .add_systems(Startup, spawn)
            .add_systems(Update, draw.in_set(crate::PlaySet::Ui).before(crate::message_menu::DrawBox));
    }
}

#[derive(Component)]
struct NotesText;

#[derive(Component)]
struct SubtitleText;

/// A clickable button of the message box (0-based).
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
}

/// The box's buttons as drawn: its own, or OK for a box without any.
pub fn box_buttons(b: &BoxMessage) -> Vec<String> {
    if b.buttons.is_empty() {
        vec!["OK".into()]
    } else {
        b.buttons.clone()
    }
}

/// What the overlay shows this frame: the panel (a message box or a menu placeholder),
/// the box on screen, and the subtitles.
fn view(hud: &ScriptHud, script: &ScriptState, sim: &SimState) -> (Option<PanelView>, Option<BoxMessage>, String) {
    let Some(rt) = script.rt.as_ref() else { return (None, None, String::new()) };
    let g = &rt.game;
    let front = match g.menu {
        Some(MENU_MESSAGE) => g.boxes.first().map(|&m| hud.box_view(m, script)),
        _ => None,
    };
    let panel = match g.menu {
        // The title is the message's own (MESG FULL).
        Some(MENU_MESSAGE) => front.as_ref().map(|b| PanelView {
            title: g.data.messages.get(&FormId(b.message)).and_then(|d| d.title.clone()),
            text: b.text.trim_end().to_string(),
            buttons: box_buttons(b),
        }),
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
    // The response being said in the conversation with the player (`crate::dialogue`).
    subs.extend(crate::dialogue::subtitle(g, sim));
    (panel, front, subs.join("\n"))
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    time: Res<Time>,
    mut hud: ResMut<ScriptHud>,
    script: Res<ScriptState>,
    sim: Res<SimState>,
    mut panel: ResMut<MessagePanel>,
    mut shown: Local<String>,
    mut notes: Query<&mut Text, (With<NotesText>, Without<SubtitleText>)>,
    mut subtitles: Query<&mut Text, (With<SubtitleText>, Without<NotesText>)>,
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
    if panel.view != p {
        panel.set(p);
    }
    if subs != *shown {
        if !subs.is_empty() {
            info!("subtitle: {}", subs.replace('\n', " / "));
        }
        for mut t in &mut subtitles {
            t.0.clone_from(&subs);
        }
        *shown = subs;
    }
}

/// Keys and clicks for a script's open menu (before the frame's scripts run): a click
/// or a number key picks a button of the front message box, Up and Down move the frame
/// and Enter picks the framed button; Space also presses the OK of a box without
/// buttons; Enter or a click on Done closes a character-creation menu. All go through
/// `ScriptState::answer`.
pub fn menu_keys(
    keys: Res<ButtonInput<KeyCode>>,
    clicks: Query<(&Interaction, &BoxButton), Changed<Interaction>>,
    hud: Res<ScriptHud>,
    mut panel: ResMut<MessagePanel>,
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
    if keys.just_pressed(ArrowUp) {
        panel.step(-1);
    }
    if keys.just_pressed(ArrowDown) {
        panel.step(1);
    }
    let digits = [Digit1, Digit2, Digit3, Digit4, Digit5, Digit6, Digit7, Digit8, Digit9];
    let pads = [Numpad1, Numpad2, Numpad3, Numpad4, Numpad5, Numpad6, Numpad7, Numpad8, Numpad9];
    let picked = (0..9).find(|&i| keys.just_pressed(digits[i]) || keys.just_pressed(pads[i])).or(clicked).or(enter.then_some(panel.selected));
    let Some(front) = script.rt.as_ref().and_then(|rt| rt.game.boxes.first().copied()) else { return };
    let buttons = hud.box_view(front, &script).buttons.len();
    if buttons == 0 {
        if picked == Some(0) || keys.just_pressed(Space) {
            script.answer(None, "OK");
        }
    } else if let Some(i) = picked.filter(|&i| i < buttons) {
        script.answer(Some(i as i32), "key");
    }
}
