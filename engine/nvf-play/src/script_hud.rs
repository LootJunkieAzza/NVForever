//! Script messages on screen. The subtitles are drawn in the interface's colour and the
//! HUD's font (docs/spec-notes/corner-messages.md §8); the message box is
//! drawn as the original's (`crate::message_menu`), and so are the short lines at the
//! top left (`crate::corner_messages`: messages without the message-box flag, objectives
//! shown or completed, quests completed, items a script gave or took).
//!
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
//! - **Menus scripts open** for character creation: the name, S.P.E.C.I.A.L., tag-skill
//!   and trait menus are drawn by `crate::chargen`; the race menu (1036) is a
//!   placeholder in the same box that names the menu, and Enter or a click on Done
//!   closes it as the player's Done would (`Game::close_menu`); nothing is chosen there.
//! - While a script's menu or box is open the player cannot move or look
//!   (`PlayState::Menu`, `script::hold_player`), and script time stops (the runtime's
//!   `Game::menu`).
//! - **Subtitles**: the dialogue lines being said (`Game::speech`), bottom centre, the
//!   text alone on screen and "Speaker: text" in the log, one response of the INFO at a time in response-number order, each
//!   for as long as it lasts (`crate::voice::speech_response`: its voice file's length,
//!   or the length fitted to its text without one). The response being said in the
//!   conversation with the player (`Game::dialogue`, `crate::dialogue::subtitle`) is in
//!   the log's `subtitle:` lines too, but drawn by the dialogue menu
//!   (`crate::dialogue_menu`). A radio's talk line heard loudly enough is subtitled too
//!   (`crate::radio::RadioSubtitle`). Every text is shown without its acting notes in
//!   braces (`crate::dialogue::shown_text`). A line said where the player does not hear
//!   it (another cell, or beyond `crate::voice::DIALOG_MAX_DISTANCE`) has no subtitle.

use crate::interface::{amber, bc_supported, face, plain_font, set_line, u, Face};
use crate::message_menu::{MessagePanel, PanelView};
use crate::script::{ScriptState, MENU_MESSAGE};
use crate::script_events::BoxMessage;
use crate::sim::SimState;
use bevy::image::CompressedImageFormatSupport;
use bevy::prelude::*;
use fnv_font::{Font, Line};
use fnv_game::game::menus;
use fnv_sim::FormId;
use nvf_assets::GameData;
use std::collections::HashMap;

#[derive(Resource, Default)]
pub struct ScriptHud {
    /// The text and buttons each message was last shown with (the trace's
    /// `message_shown`), for the box queue, which holds only FormIDs.
    shown: HashMap<u32, BoxMessage>,
    /// The response texts (NAM1) of the dialogue lines said, by INFO.
    lines: HashMap<u32, Vec<String>>,
    /// The message box on screen (as last drawn).
    on_screen: Option<BoxMessage>,
}

impl ScriptHud {
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
            .add_plugins((crate::message_menu::MessageMenuPlugin, crate::corner_messages::CornerMessagesPlugin, crate::quest_messages::QuestMessagesPlugin))
            .add_systems(Startup, spawn)
            .add_systems(Update, draw.in_set(crate::PlaySet::Ui).before(crate::message_menu::DrawBox));
    }
}

/// The subtitles' area, a column as wide as the text may be, centred in the view.
#[derive(Component)]
struct SubtitleArea;

// ---- the subtitles (corner-messages.md §8) -----------------------------------------------

/// The text may be as wide as the HUD's Subtitles rect [documented:
/// menus\main\hud_main_menu.xml, `Subtitles` width 800].
const SUBTITLE_WIDTH: f32 = 800.0;
/// The first line's base under the view's top [observed: the owner's screenshot].
const SUBTITLE_BASE: f32 = 633.0;
/// Lines are the font's size and this much apart [observed: 38 units on the screenshot].
const SUBTITLE_LEADING: f32 = 2.0;
/// Font 7, the HUD's text templates' [documented: menus\prefabs\hudtemplates.xml].
const SUBTITLE_FONT: &str = "textures\\fonts\\baked-in_monofonto_large.fnt";

/// The subtitles' lines, each centred in the area: the line, its start and its base, from
/// the area's top left. Every text starts a line of its own.
pub fn subtitle_layout(texts: &[String], font: &Font) -> Vec<(Line, f32, f32)> {
    let pitch = font.size + SUBTITLE_LEADING;
    let lines = texts.iter().flat_map(|t| font.lines(t.trim(), Some(SUBTITLE_WIDTH)));
    lines
        .enumerate()
        .map(|(i, l)| {
            let x = ((SUBTITLE_WIDTH - l.width) / 2.0).max(0.0);
            (l, x, pitch * i as f32)
        })
        .collect()
}

/// A clickable button of the message box (0-based).
#[derive(Component, Clone, Copy, Debug)]
pub struct BoxButton(pub usize);

fn spawn(mut commands: Commands) {
    commands.spawn((
        Node { position_type: PositionType::Absolute, width: Val::Percent(100.0), top: u(0.0), justify_content: JustifyContent::Center, ..default() },
        children![(Node { width: u(SUBTITLE_WIDTH), ..default() }, SubtitleArea)],
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
fn view(hud: &ScriptHud, script: &ScriptState, sim: &SimState, radio: Option<&crate::radio::RadioSubtitle>, player: Option<&crate::player::Player>) -> (Option<PanelView>, Option<BoxMessage>, Subtitles) {
    let Some(rt) = script.rt.as_ref() else { return (None, None, Subtitles::default()) };
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
        // The name, S.P.E.C.I.A.L., tag-skill and trait menus draw themselves.
        Some(code) if crate::script::is_script_menu(code) && !crate::chargen::has_own_menu(code) => Some(menu_panel(code)),
        _ => None,
    };
    let mut subs = Vec::new();
    let mut on_screen = Vec::new();
    for s in &g.speech {
        // A line the player does not hear has no subtitle (`crate::voice::audible`).
        if !crate::voice::audible(sim, player, s.speaker) {
            continue;
        }
        let Some(texts) = hud.lines.get(&s.info.0) else { continue };
        let i = crate::voice::speech_response(g, sim, s);
        let Some(text) = texts.get(i).or_else(|| texts.last()) else { continue };
        let base = sim.base_of(s.speaker.0).unwrap_or(s.speaker);
        let who = sim.name(base).map(str::to_string).unwrap_or_else(|| g.data.name(s.speaker));
        let text = crate::dialogue::shown_text(text);
        if text.is_empty() {
            continue;
        }
        subs.push(format!("{who}: {text}"));
        on_screen.push(text);
    }
    // A radio's talk line (`crate::radio::RadioSubtitle`), the station's name as the speaker.
    if let Some((station, info, response)) = radio.and_then(|r| r.0) {
        if let Some(text) = hud.lines.get(&info.0).and_then(|t| t.get(response)).map(|t| crate::dialogue::shown_text(t)).filter(|t| !t.is_empty()) {
            let base = sim.base_of(station.0).unwrap_or(station);
            let who = sim.name(base).map(str::to_string).unwrap_or_else(|| g.data.name(station));
            subs.push(format!("{who}: {text}"));
            on_screen.push(text);
        }
    }
    // The response being said in the conversation with the player: drawn by the dialogue
    // menu (`crate::dialogue_menu`), logged with the others.
    subs.extend(crate::dialogue::subtitle(g, sim));
    (panel, front, Subtitles { logged: subs.join("\n"), on_screen })
}

/// The subtitles: all of them for the log ("Speaker: text"), those drawn here for the
/// screen (the text alone).
#[derive(Default, PartialEq)]
struct Subtitles {
    logged: String,
    on_screen: Vec<String>,
}

#[allow(clippy::type_complexity, clippy::too_many_arguments)]
fn draw(
    mut hud: ResMut<ScriptHud>,
    script: Res<ScriptState>,
    sim: Res<SimState>,
    mut panel: ResMut<MessagePanel>,
    mut shown: Local<Subtitles>,
    mut commands: Commands,
    areas: Query<Entity, With<SubtitleArea>>,
    (radio, play, player): (Option<Res<crate::radio::RadioSubtitle>>, Res<crate::PlayWorld>, Option<Res<crate::player::Player>>),
    (data, mut images, formats, mut font): (Option<Res<GameData>>, ResMut<Assets<Image>>, Option<Res<CompressedImageFormatSupport>>, Local<Option<Face>>),
) {
    if let Some((_, info, _)) = radio.as_deref().and_then(|r| r.0) {
        if !hud.knows_line(info) {
            hud.remember_line(info, crate::script::responses(&play, info));
        }
    }
    let (p, front, subs) = view(&hud, &script, &sim, radio.as_deref(), player.as_deref());
    hud.on_screen = front;
    if panel.view != p {
        panel.set(p);
    }
    if subs != *shown {
        if subs.logged != shown.logged && !subs.logged.is_empty() {
            info!("subtitle: {}", subs.logged.replace('\n', " / "));
        }
        if subs.on_screen != shown.on_screen {
            let face = font.get_or_insert_with(|| face(SUBTITLE_FONT, plain_font(36.0, 17.0), data.as_deref(), &mut images, bc_supported(formats.as_deref())));
            let lines = subtitle_layout(&subs.on_screen, &face.font);
            for area in &areas {
                commands.entity(area).despawn_related::<Children>().with_children(|a| {
                    for (line, x, base) in &lines {
                        set_line(a, face, line, *x, SUBTITLE_BASE + *base, amber(255.0));
                    }
                });
            }
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
    if crate::chargen::has_own_menu(code) {
        return;
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subtitles_are_centred_lines_of_the_hud_rect() {
        // Every character 17 wide: 47 fit the rect's 800.
        let font = plain_font(36.0, 17.0);
        let long = "a".repeat(30) + " " + &"b".repeat(30);
        let l = subtitle_layout(&[long, "Hi.".into()], &font);
        assert_eq!(l.len(), 3);
        assert_eq!((l[0].0.width, l[0].1, l[0].2), (510.0, 145.0, 0.0));
        assert_eq!((l[1].1, l[1].2), (145.0, 38.0));
        // A second speaker's text starts a line of its own.
        assert_eq!((l[2].0.width, l[2].1, l[2].2), (51.0, 374.5, 76.0));
    }
}
